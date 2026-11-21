use super::policy::{read_private, Policy};
use crate::manager::{execute, Host, State};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tmgr_ipc::chat::{self, Author, AuthorKind, ChatMessage, MessageKind, RoomRef};
use tmgr_ipc::manager::{limits as mlimits, tool, valid_req_id, SpoolReq, DATA_NOTICE, SPOOL_V};
use tmgr_ipc::mgr_rpc::{limits, valid_machine_id, ErrorCode, Op, RpcReq, RpcRes, MGR_RPC_V};

pub const SEEN_FILE: &str = "mgr-rpc-seen.json";
const MAX_STORE_BYTES: u64 = 8 * 1024 * 1024;
const RATE_WINDOW_MS: u64 = 60_000;
const MAX_REFUSAL: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    pub ts_ms: u64,
    pub from: String,
    pub rpc: Option<String>,
    pub op: Option<Op>,
    pub session: Option<String>,
    pub outcome: String,
}

pub trait Audit: Send + Sync {
    fn record(&self, rec: AuditRecord);
}

#[derive(Default)]
pub struct MemoryAudit(Mutex<Vec<AuditRecord>>);

impl MemoryAudit {
    pub fn records(&self) -> Vec<AuditRecord> {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

impl Audit for MemoryAudit {
    fn record(&self, rec: AuditRecord) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).push(rec);
    }
}

enum Slot {
    Marker,
    InFlight,
    Lost,
    Answered(RpcRes),
}

struct Entry {
    expires_ms: u64,
    slot: Slot,
}

#[derive(Default)]
struct SenderState {
    seen: HashMap<String, Entry>,
    rate: VecDeque<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoreFile {
    v: u32,
    senders: BTreeMap<String, BTreeMap<String, StoredEntry>>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredEntry {
    expires_ms: u64,
    answer: Option<RpcRes>,
}

#[derive(Default)]
struct Meta {
    rpc: Option<String>,
    op: Option<Op>,
    session: Option<String>,
}

enum Args {
    List,
    Read { session: String, lines: Option<u32> },
    Send { session: String, text: String },
    Ask { text: String },
}

type Refusal = (ErrorCode, String);

#[derive(Deserialize)]
struct IdOnly {
    rpc: String,
}

enum Admit {
    Reply(RpcRes),
    Run(Box<RpcReq>, Args),
}

pub struct Dispatcher {
    machine: String,
    state: State,
    audit: Arc<dyn Audit>,
    senders: Mutex<HashMap<String, SenderState>>,
    store: Option<PathBuf>,
    blind_until: u64,
    pub(crate) rate_per_min: usize,
    pub(crate) max_seen: usize,
}

impl Dispatcher {
    pub fn new(machine: &str, state: State, audit: Arc<dyn Audit>) -> Dispatcher {
        Dispatcher::with_store(machine, state, audit, None, 0)
    }

    pub fn with_store(
        machine: &str,
        state: State,
        audit: Arc<dyn Audit>,
        store: Option<PathBuf>,
        now_ms: u64,
    ) -> Dispatcher {
        let (senders, readable) = store
            .as_deref()
            .map_or((HashMap::new(), true), |p| load_store(p, now_ms));
        let blind_until = if readable {
            0
        } else {
            now_ms + 2 * limits::FRESH_MS + limits::SEEN_MARGIN_MS
        };
        Dispatcher {
            machine: machine.into(),
            state,
            audit,
            senders: Mutex::new(senders),
            store,
            blind_until,
            rate_per_min: limits::RPC_PER_MINUTE,
            max_seen: limits::MAX_SEEN_PER_SENDER,
        }
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    fn persist(&self, senders: &HashMap<String, SenderState>) -> std::io::Result<()> {
        let Some(path) = &self.store else {
            return Ok(());
        };
        let mut out: BTreeMap<String, BTreeMap<String, StoredEntry>> = BTreeMap::new();
        for (from, st) in senders {
            for (rpc, e) in &st.seen {
                let answer = match &e.slot {
                    Slot::Marker => continue,
                    Slot::Answered(a) => Some(a.clone()),
                    Slot::InFlight | Slot::Lost => None,
                };
                out.entry(from.clone()).or_default().insert(
                    rpc.clone(),
                    StoredEntry {
                        expires_ms: e.expires_ms,
                        answer,
                    },
                );
            }
        }
        let bytes = serde_json::to_vec(&StoreFile { v: 1, senders: out })?;
        let mut tmp = path.clone().into_os_string();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        let _ = std::fs::remove_file(&tmp);
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)?;
        match path.parent() {
            Some(dir) => std::fs::File::open(dir)?.sync_all(),
            None => Ok(()),
        }
    }

    pub fn handle(
        &self,
        policy: &Policy,
        host: &dyn Host,
        from: &str,
        payload: &[u8],
    ) -> Option<RpcRes> {
        let now = host.now_ms();
        if !valid_machine_id(from) || from == self.machine || !policy.accepts(from) {
            self.audit.record(AuditRecord {
                ts_ms: now,
                from: from.chars().take(64).collect(),
                rpc: None,
                op: None,
                session: None,
                outcome: ErrorCode::NotAllowed.as_str().into(),
            });
            return None;
        }
        let mut meta = Meta::default();
        let res = self.process(policy, host, from, payload, now, &mut meta);
        let outcome = match &res {
            None => "dropped",
            Some(r) => r.error.as_ref().map_or("ok", |e| e.code.as_str()),
        };
        self.audit.record(AuditRecord {
            ts_ms: now,
            from: from.chars().take(64).collect(),
            rpc: meta.rpc,
            op: meta.op,
            session: meta.session,
            outcome: outcome.into(),
        });
        res
    }

    fn process(
        &self,
        policy: &Policy,
        host: &dyn Host,
        from: &str,
        payload: &[u8],
        now: u64,
        meta: &mut Meta,
    ) -> Option<RpcRes> {
        let (req, args) = match self.admit(policy, from, payload, now, meta)? {
            Admit::Reply(res) => return Some(res),
            Admit::Run(req, args) => (req, args),
        };
        let rpc = req.rpc.as_str();
        let res = cap_size(
            rpc,
            match self.exec(host, from, rpc, args) {
                Ok(data) => RpcRes::ok(rpc, wrap(data)),
                Err((code, msg)) => RpcRes::err(rpc, code, msg),
            },
        );
        if durable(req.op) {
            let mut senders = self.senders.lock().unwrap_or_else(|e| e.into_inner());
            let st = senders.entry(from.to_string()).or_default();
            let e = st.seen.entry(req.rpc.clone()).or_insert(Entry {
                expires_ms: expiry(now, req.ts_ms),
                slot: Slot::Marker,
            });
            e.slot = Slot::Answered(res.clone());
            if let Err(err) = self.persist(&senders) {
                crate::daemon::log(&format!("manager rpc: seen store not written: {err}"));
            }
        }
        Some(res)
    }

    fn admit(
        &self,
        policy: &Policy,
        from: &str,
        payload: &[u8],
        now: u64,
        meta: &mut Meta,
    ) -> Option<Admit> {
        let mut senders = self.senders.lock().unwrap_or_else(|e| e.into_inner());
        let st = senders.entry(from.to_string()).or_default();
        st.rate.retain(|t| now.saturating_sub(*t) < RATE_WINDOW_MS);
        if st.rate.len() >= self.rate_per_min {
            let id = readable_id(payload)?;
            meta.rpc = Some(id.clone());
            return Some(Admit::Reply(RpcRes::err(
                &id,
                ErrorCode::RateLimited,
                "too many requests",
            )));
        }
        st.rate.push_back(now);
        let Some(req) = parse(payload) else {
            let id = readable_id(payload)?;
            meta.rpc = Some(id.clone());
            return Some(Admit::Reply(RpcRes::err(
                &id,
                ErrorCode::BadRequest,
                "malformed request",
            )));
        };
        meta.rpc = Some(req.rpc.clone());
        meta.op = Some(req.op);
        meta.session = req
            .args
            .get("session")
            .and_then(Value::as_str)
            .filter(|s| tmgr_ipc::proto::valid_session_id(s))
            .map(String::from);
        let rpc = req.rpc.as_str();
        let fail = |code, msg: &str| Some(Admit::Reply(RpcRes::err(rpc, code, msg)));
        if req.v != MGR_RPC_V {
            return fail(ErrorCode::BadRequest, "unsupported version");
        }
        st.seen.retain(|_, e| e.expires_ms > now);
        match st.seen.get(rpc).map(|e| &e.slot) {
            Some(Slot::Answered(answer)) => return Some(Admit::Reply(answer.clone())),
            Some(Slot::InFlight) => return fail(ErrorCode::RateLimited, "in progress"),
            Some(Slot::Lost) => return fail(ErrorCode::Unavailable, "outcome unknown"),
            _ => {}
        }
        if now.abs_diff(req.ts_ms) > limits::FRESH_MS {
            return fail(
                ErrorCode::Stale,
                "timestamp is outside the freshness window",
            );
        }
        if !req.chain.is_empty() {
            return fail(ErrorCode::Depth, "forwarded requests are not accepted");
        }
        if st.seen.len() >= self.max_seen && !st.seen.contains_key(rpc) {
            return fail(ErrorCode::RateLimited, "too many requests");
        }
        let expires = expiry(now, req.ts_ms);
        let e = st.seen.entry(req.rpc.clone()).or_insert(Entry {
            expires_ms: expires,
            slot: Slot::Marker,
        });
        e.expires_ms = e.expires_ms.max(expires);
        let args = match prepare(policy, from, &req) {
            Ok(args) => args,
            Err(res) => return Some(Admit::Reply(*res)),
        };
        if durable(req.op) && now < self.blind_until {
            if let Some(st) = senders.get_mut(from) {
                st.seen.remove(&req.rpc);
            }
            return Some(Admit::Reply(RpcRes::err(
                &req.rpc,
                ErrorCode::Unavailable,
                "request history unreadable",
            )));
        }
        if durable(req.op) {
            e.slot = Slot::InFlight;
            if self.persist(&senders).is_err() {
                if let Some(st) = senders.get_mut(from) {
                    st.seen.remove(&req.rpc);
                }
                return Some(Admit::Reply(RpcRes::err(
                    &req.rpc,
                    ErrorCode::Unavailable,
                    "request could not be recorded",
                )));
            }
        }
        Some(Admit::Run(Box::new(req), args))
    }

    fn exec(&self, host: &dyn Host, from: &str, rpc: &str, args: Args) -> Result<Value, Refusal> {
        let call = |name: &str, args: Value| {
            execute(
                host,
                &self.state,
                &SpoolReq {
                    v: SPOOL_V,
                    id: rpc.into(),
                    tool: name.into(),
                    args,
                },
            )
            .map_err(refused)
        };
        match args {
            Args::List => {
                let local: HashSet<String> = host
                    .sessions()
                    .into_iter()
                    .filter(|s| s.origin.is_none())
                    .map(|s| s.id)
                    .collect();
                let mut rows = call(tool::LIST_SESSIONS, json!({}))?;
                if let Value::Array(a) = &mut rows {
                    a.retain(|r| r["id"].as_str().is_some_and(|id| local.contains(id)));
                }
                Ok(rows)
            }
            Args::Read { session, lines } => {
                let mut lines = lines.unwrap_or(mlimits::DEFAULT_READ_LINES);
                loop {
                    let out = call(
                        tool::READ_SESSION,
                        json!({"session": session, "lines": lines}),
                    )?;
                    let text = out
                        .as_str()
                        .and_then(|s| s.strip_prefix(DATA_NOTICE))
                        .map_or("", |s| s.strip_prefix('\n').unwrap_or(s));
                    let data = Value::String(text.into());
                    if wire_len(&RpcRes::ok(rpc, wrap(data.clone()))) <= limits::MAX_RES_BYTES {
                        return Ok(data);
                    }
                    if lines == 1 {
                        return Err(refused("output is too large"));
                    }
                    lines = (lines / 2).max(1);
                }
            }
            Args::Send { session, text } => {
                if host.sessions().iter().any(|s| s.id == session && s.manager) {
                    return Err(refused(
                        "manager sessions cannot be written to; talk to a manager with ask",
                    ));
                }
                call(
                    tool::SEND_TO_SESSION,
                    json!({"session": session, "text": text}),
                )
            }
            Args::Ask { text } => {
                let msg = ChatMessage {
                    v: chat::CHAT_V,
                    id: chat::new_message_id(),
                    room: RoomRef::manager(&self.machine),
                    author: Author {
                        kind: AuthorKind::Manager,
                        id: "manager".into(),
                        machine: Some(self.machine.clone()),
                        device: None,
                        agent: None,
                    },
                    ts_ms: host.now_ms(),
                    kind: MessageKind::Text,
                    body_md: format!("{}{text}", provenance(from)),
                    attachments: Vec::new(),
                    reply_to: None,
                    decision: None,
                    session_meta: None,
                };
                msg.validate().map_err(refused)?;
                let sent = self.state.chat.post(msg).map_err(refused)?;
                Ok(json!({"id": sent.id}))
            }
        }
    }
}

fn durable(op: Op) -> bool {
    matches!(op, Op::Send | Op::Ask)
}

fn expiry(now: u64, ts_ms: u64) -> u64 {
    now.max(ts_ms) + limits::FRESH_MS + limits::SEEN_MARGIN_MS
}

fn prepare(policy: &Policy, from: &str, req: &RpcReq) -> Result<Args, Box<RpcRes>> {
    let rpc = req.rpc.as_str();
    let err = |code, msg: &str| Box::new(RpcRes::err(rpc, code, msg));
    if !policy.allows(req.op) {
        return Err(err(ErrorCode::OpDenied, "operation is not allowed"));
    }
    if matches!(req.op, Op::SpawnChallenge | Op::Spawn | Op::Switch) {
        return Err(err(
            ErrorCode::Unavailable,
            "operation is not available yet",
        ));
    }
    parse_args(req.op, &req.args, from)
        .ok_or_else(|| err(ErrorCode::BadRequest, "invalid arguments"))
}

fn load_store(path: &Path, now: u64) -> (HashMap<String, SenderState>, bool) {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name().and_then(|n| n.to_str())) else {
        return (HashMap::new(), false);
    };
    let text = match read_private(dir, name, MAX_STORE_BYTES) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (HashMap::new(), true),
        Err(_) => {
            crate::daemon::log("manager rpc: seen store unreadable; refusing send/ask for now");
            return (HashMap::new(), false);
        }
    };
    let Ok(file) = serde_json::from_str::<StoreFile>(&text) else {
        crate::daemon::log("manager rpc: seen store is not valid; refusing send/ask for now");
        return (HashMap::new(), false);
    };
    let intact = file.v == 1
        && file.senders.iter().all(|(from, entries)| {
            valid_machine_id(from) && entries.keys().all(|rpc| valid_req_id(rpc))
        });
    if !intact {
        crate::daemon::log("manager rpc: seen store is not valid; refusing send/ask for now");
        return (HashMap::new(), false);
    }
    let mut out = HashMap::new();
    for (from, entries) in file.senders {
        let st: &mut SenderState = out.entry(from).or_default();
        for (rpc, e) in entries {
            if e.expires_ms <= now {
                continue;
            }
            let slot = e.answer.map_or(Slot::Lost, Slot::Answered);
            st.seen.insert(
                rpc,
                Entry {
                    expires_ms: e.expires_ms,
                    slot,
                },
            );
        }
    }
    (out, true)
}

fn provenance(from: &str) -> String {
    format!("_From the top manager ({from})_\n\n")
}

fn wrap(data: Value) -> Value {
    json!({"notice": DATA_NOTICE, "data": data})
}

fn refused(msg: impl AsRef<str>) -> Refusal {
    (
        ErrorCode::Refused,
        msg.as_ref().chars().take(MAX_REFUSAL).collect(),
    )
}

fn wire_len(res: &RpcRes) -> usize {
    serde_json::to_vec(res).map_or(usize::MAX, |b| b.len())
}

fn cap_size(rpc: &str, res: RpcRes) -> RpcRes {
    if wire_len(&res) > limits::MAX_RES_BYTES {
        RpcRes::err(rpc, ErrorCode::Refused, "response is too large")
    } else {
        res
    }
}

fn parse(payload: &[u8]) -> Option<RpcReq> {
    if payload.len() > limits::MAX_REQ_BYTES {
        return None;
    }
    serde_json::from_slice::<RpcReq>(payload)
        .ok()
        .filter(|r| valid_req_id(&r.rpc))
}

fn readable_id(payload: &[u8]) -> Option<String> {
    if payload.len() > limits::MAX_REQ_BYTES {
        return None;
    }
    serde_json::from_slice::<IdOnly>(payload)
        .ok()
        .map(|i| i.rpc)
        .filter(|id| valid_req_id(id))
}

fn text_in(v: &Value, max: usize) -> Option<String> {
    v.as_str()
        .filter(|s| !s.is_empty() && s.len() <= max)
        .map(String::from)
}

fn parse_args(op: Op, a: &Value, from: &str) -> Option<Args> {
    let m = a.as_object()?;
    let only = |keys: &[&str]| m.keys().all(|k| keys.contains(&k.as_str()));
    let session = || {
        m.get("session")
            .and_then(Value::as_str)
            .filter(|s| tmgr_ipc::proto::valid_session_id(s))
            .map(String::from)
    };
    match op {
        Op::List => m.is_empty().then_some(Args::List),
        Op::Read if only(&["session", "lines"]) => {
            let lines = match m.get("lines") {
                None => None,
                Some(v) => Some(
                    v.as_u64()
                        .filter(|n| (1..=u64::from(mlimits::MAX_READ_LINES)).contains(n))?
                        as u32,
                ),
            };
            Some(Args::Read {
                session: session()?,
                lines,
            })
        }
        Op::Send if only(&["session", "text"]) => Some(Args::Send {
            session: session()?,
            text: text_in(m.get("text")?, mlimits::MAX_SEND_BYTES)?,
        }),
        Op::Ask if only(&["text"]) => Some(Args::Ask {
            text: text_in(
                m.get("text")?,
                chat::limits::MAX_BODY.saturating_sub(provenance(from).len()),
            )?,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::*;
    use crate::manager::MemoryChat;
    use std::os::unix::fs::PermissionsExt;
    use tmgr_ipc::mgr_rpc::Op;

    const OTHER: &str = "mx_00000000000000cc";
    const MIN: u64 = 60_000;

    struct Rig {
        d: Dispatcher,
        audit: Arc<MemoryAudit>,
        host: Fake,
    }

    fn rig() -> Rig {
        let audit = Arc::new(MemoryAudit::default());
        Rig {
            d: Dispatcher::new(
                ME,
                State::new(Box::new(MemoryChat::default())),
                audit.clone(),
            ),
            audit,
            host: Fake::new(),
        }
    }

    fn policy(allow: &[&str]) -> Policy {
        let list = allow
            .iter()
            .map(|a| format!("\"{a}\""))
            .collect::<Vec<_>>()
            .join(",");
        Policy::parse(&format!("accept_from = [\"{PEER}\"]\nallow = [{list}]")).unwrap()
    }

    fn open() -> Policy {
        policy(&[
            "list",
            "read",
            "send",
            "ask",
            "spawn_challenge",
            "spawn",
            "switch",
        ])
    }

    fn body(rpc: &str, op: &str, args: Value, ts: u64) -> Vec<u8> {
        serde_json::to_vec(&json!({"v":1,"rpc":rpc,"op":op,"args":args,"chain":[],"ts_ms":ts}))
            .unwrap()
    }

    impl Rig {
        fn call_at(&self, p: &Policy, from: &str, payload: &[u8]) -> Option<RpcRes> {
            self.d.handle(p, &self.host, from, payload)
        }

        fn call(&self, p: &Policy, rpc: &str, op: &str, args: Value) -> RpcRes {
            self.call_at(p, PEER, &body(rpc, op, args, self.host.now_ms()))
                .unwrap()
        }
    }

    fn code(r: &RpcRes) -> ErrorCode {
        assert!(!r.ok, "{r:?}");
        r.error.as_ref().unwrap().code
    }

    fn data(r: &RpcRes) -> &Value {
        assert!(r.ok, "{r:?}");
        let res = r.result.as_ref().unwrap();
        assert_eq!(res["notice"], DATA_NOTICE);
        &res["data"]
    }

    fn valid_args(op: &str) -> Value {
        match op {
            "list" => json!({}),
            "read" => json!({"session": WORKER}),
            "send" => json!({"session": WORKER, "text": "hi"}),
            "ask" => json!({"text": "hi"}),
            _ => json!({}),
        }
    }

    #[test]
    fn a_missing_policy_accepts_nobody_for_every_op() {
        let r = rig();
        for op in Op::ALL {
            let res = r.call_at(
                &Policy::deny_all(),
                PEER,
                &body("r1", op.as_str(), valid_args(op.as_str()), T0),
            );
            assert!(res.is_none(), "{op:?}");
        }
        assert!(r.host.sent.lock().unwrap().is_empty());
    }

    #[test]
    fn a_sender_outside_accept_from_is_refused_for_every_op() {
        let r = rig();
        for op in Op::ALL {
            let res = r.call_at(
                &open(),
                OTHER,
                &body("r1", op.as_str(), valid_args(op.as_str()), T0),
            );
            assert!(res.is_none(), "{op:?}");
        }
    }

    #[test]
    fn this_machine_and_malformed_senders_are_refused() {
        let r = rig();
        let selfish = Policy::parse(&format!("accept_from = [\"{ME}\"]")).unwrap();
        let p = body("r1", "list", json!({}), T0);
        assert!(r.call_at(&selfish, ME, &p).is_none());
        for bad in ["", "mx_AAAA", "laptop", "mx_00000000000000bbb"] {
            assert!(r.call_at(&open(), bad, &p).is_none());
        }
    }

    #[test]
    fn unreadable_payloads_are_dropped_and_audited_once() {
        let r = rig();
        for junk in [&b"not json"[..], b"{}", b"[]", br#"{"rpc":"../x"}"#, b""] {
            assert!(r.call_at(&open(), PEER, junk).is_none());
        }
        let log = r.audit.records();
        assert_eq!(log.len(), 5);
        assert!(log
            .iter()
            .all(|l| l.outcome == "dropped" && l.rpc.is_none()));
    }

    #[test]
    fn malformed_requests_with_a_readable_id_get_bad_request() {
        let r = rig();
        let cases: Vec<Vec<u8>> = vec![
            br#"{"rpc":"r1"}"#.to_vec(),
            br#"{"rpc":"r1","v":1,"op":"rm","args":{},"chain":[],"ts_ms":1}"#.to_vec(),
            br#"{"rpc":"r1","v":1,"op":"list","args":{},"chain":[],"ts_ms":1,"approval":{}}"#
                .to_vec(),
            br#"{"rpc":"r1","v":1,"op":"list","args":{},"chain":["a"],"ts_ms":"x"}"#.to_vec(),
        ];
        for c in cases {
            let res = r.call_at(&open(), PEER, &c).unwrap();
            assert_eq!(code(&res), ErrorCode::BadRequest);
            assert_eq!(res.rpc, "r1");
        }
    }

    #[test]
    fn an_oversize_request_is_dropped_without_an_id_scan_and_not_executed() {
        let r = rig();
        let big = "a".repeat(limits::MAX_REQ_BYTES);
        let res = r.call_at(&open(), PEER, &body("r1", "ask", json!({"text": big}), T0));
        assert!(res.is_none());
        assert_eq!(r.audit.records()[0].rpc, None);
        let msgs = r.d.state().chat.history(&RoomRef::manager(ME), 10).unwrap();
        assert!(msgs.is_empty());
    }

    #[test]
    fn a_non_accepted_sender_with_garbage_gets_no_reply_and_an_audit_line() {
        let r = rig();
        for junk in [&b"not json"[..], br#"{"rpc":"r1"}"#, b""] {
            assert!(r.call_at(&open(), OTHER, junk).is_none());
        }
        let log = r.audit.records();
        assert_eq!(log.len(), 3);
        assert!(log
            .iter()
            .all(|l| l.outcome == "not_allowed" && l.rpc.is_none() && l.op.is_none()));
    }

    #[test]
    fn malformed_payloads_from_an_accepted_sender_spend_the_rate_budget() {
        let r = rig();
        let junk = br#"{"rpc":"r1"}"#;
        for i in 0..limits::RPC_PER_MINUTE {
            let res = r.call_at(&open(), PEER, junk).unwrap();
            assert_eq!(code(&res), ErrorCode::BadRequest, "{i}");
        }
        let res = r.call_at(&open(), PEER, junk).unwrap();
        assert_eq!(code(&res), ErrorCode::RateLimited);
        assert_eq!(res.rpc, "r1");
    }

    #[test]
    fn an_accepted_sender_over_budget_with_an_unreadable_payload_gets_no_reply() {
        let r = rig();
        for _ in 0..limits::RPC_PER_MINUTE {
            r.call_at(&open(), PEER, b"junk");
        }
        assert!(r.call_at(&open(), PEER, b"junk").is_none());
    }

    #[test]
    fn an_oversize_payload_with_a_late_rpc_id_is_not_scanned() {
        let r = rig();
        let mut p = vec![b' '; 5000];
        p.extend_from_slice(br#"{"rpc":"late"}"#);
        p.resize(limits::MAX_REQ_BYTES + 1, b' ');
        assert!(r.call_at(&open(), PEER, &p).is_none());
        assert_eq!(r.audit.records()[0].rpc, None);
    }

    #[test]
    fn a_sender_without_an_allow_key_may_not_ask() {
        let r = rig();
        let p = Policy::parse(&format!("accept_from = [\"{PEER}\"]")).unwrap();
        let res = r.call(&p, "q1", "ask", valid_args("ask"));
        assert_eq!(code(&res), ErrorCode::OpDenied);
        assert!(r.call(&p, "l1", "list", json!({})).ok);
        let msgs = r.d.state().chat.history(&RoomRef::manager(ME), 10).unwrap();
        assert!(msgs.is_empty());
    }

    #[test]
    fn a_wrong_version_is_a_bad_request() {
        let r = rig();
        let p = serde_json::to_vec(
            &json!({"v":2,"rpc":"r1","op":"list","args":{},"chain":[],"ts_ms":T0}),
        )
        .unwrap();
        assert_eq!(
            code(&r.call_at(&open(), PEER, &p).unwrap()),
            ErrorCode::BadRequest
        );
    }

    #[test]
    fn stale_requests_are_refused_in_both_directions() {
        let r = rig();
        let edge = limits::FRESH_MS;
        for (rpc, ts, stale) in [
            ("a", T0 - edge, false),
            ("b", T0 + edge, false),
            ("c", T0 - edge - 1, true),
            ("d", T0 + edge + 1, true),
        ] {
            let res = r
                .call_at(&open(), PEER, &body(rpc, "list", json!({}), ts))
                .unwrap();
            assert_eq!(!res.ok, stale, "{rpc}");
            if stale {
                assert_eq!(code(&res), ErrorCode::Stale);
            }
        }
    }

    #[test]
    fn a_non_empty_chain_is_refused() {
        let r = rig();
        for chain in [json!([PEER]), json!([ME]), json!([OTHER, PEER])] {
            let p = serde_json::to_vec(
                &json!({"v":1,"rpc":"r1","op":"list","args":{},"chain":chain,"ts_ms":T0}),
            )
            .unwrap();
            assert_eq!(
                code(&r.call_at(&open(), PEER, &p).unwrap()),
                ErrorCode::Depth
            );
        }
    }

    #[test]
    fn the_rate_limit_is_per_sender_and_slides_with_the_clock() {
        let r = rig();
        let both = Policy::parse(&format!("accept_from = [\"{PEER}\", \"{OTHER}\"]")).unwrap();
        for i in 0..limits::RPC_PER_MINUTE {
            let res = r
                .call_at(&both, PEER, &body(&format!("r{i}"), "list", json!({}), T0))
                .unwrap();
            assert!(res.ok, "{i}");
        }
        let over = body("over", "list", json!({}), T0);
        assert_eq!(
            code(&r.call_at(&both, PEER, &over).unwrap()),
            ErrorCode::RateLimited
        );
        assert!(r.call_at(&both, OTHER, &over).unwrap().ok);
        r.host.set_now(T0 + MIN);
        let again = body("over", "list", json!({}), T0 + MIN);
        assert!(r.call_at(&both, PEER, &again).unwrap().ok);
    }

    #[test]
    fn a_rate_limited_answer_is_not_stored() {
        let r = rig();
        for i in 0..limits::RPC_PER_MINUTE {
            r.call(&open(), &format!("r{i}"), "list", json!({}));
        }
        let first = r.call(&open(), "ask1", "ask", json!({"text": "hello"}));
        assert_eq!(code(&first), ErrorCode::RateLimited);
        r.host.set_now(T0 + MIN);
        let second = r
            .call_at(
                &open(),
                PEER,
                &body("ask1", "ask", json!({"text": "hello"}), T0 + MIN),
            )
            .unwrap();
        assert!(second.ok);
    }

    #[test]
    fn ops_outside_the_policy_are_denied() {
        let r = rig();
        let default = policy(&["list", "read", "ask"]);
        let res = r.call(&default, "r1", "send", valid_args("send"));
        assert_eq!(code(&res), ErrorCode::OpDenied);
        assert!(r.host.sent.lock().unwrap().is_empty());
        for op in [
            "list",
            "read",
            "send",
            "ask",
            "spawn_challenge",
            "spawn",
            "switch",
        ] {
            let none = policy(&[]);
            let res = r.call(&none, &format!("d-{op}"), op, valid_args(op));
            assert_eq!(code(&res), ErrorCode::OpDenied, "{op}");
        }
    }

    #[test]
    fn spawn_challenge_spawn_and_switch_are_unavailable_even_when_allowed() {
        let r = rig();
        for op in ["spawn_challenge", "spawn", "switch"] {
            for (i, args) in [
                json!({}),
                json!({"profile":"claude-code","cwd":"/tmp","permission_mode":"bypass"}),
                json!({"approval":{"device":"d","target_machine":"m","nonce":"n","ts_ms":1,"alg":"es256","sig":"s"}}),
            ]
            .into_iter()
            .enumerate()
            {
                let res = r.call(&open(), &format!("{op}-{i}"), op, args);
                assert_eq!(code(&res), ErrorCode::Unavailable, "{op} {i}");
            }
        }
    }

    #[test]
    fn unknown_and_missing_args_are_bad_requests() {
        let r = rig();
        let cases = [
            ("list", json!({"x": 1})),
            ("list", json!(null)),
            ("list", json!([])),
            ("read", json!({})),
            ("read", json!({"session": "../x"})),
            ("read", json!({"session": WORKER, "lines": 0})),
            ("read", json!({"session": WORKER, "lines": 2001})),
            ("read", json!({"session": WORKER, "lines": "5"})),
            ("read", json!({"session": WORKER, "lines": null})),
            ("read", json!({"session": WORKER, "extra": 1})),
            ("send", json!({"session": WORKER})),
            ("send", json!({"text": "x"})),
            ("send", json!({"session": WORKER, "text": ""})),
            ("send", json!({"session": WORKER, "text": 5})),
            ("send", json!({"session": WORKER, "text": "x", "extra": 1})),
            ("ask", json!({})),
            ("ask", json!({"text": ""})),
            ("ask", json!({"text": "x", "session": WORKER})),
            ("ask", json!({"text": "a".repeat(64 * 1024)})),
        ];
        for (i, (op, args)) in cases.into_iter().enumerate() {
            let res = r.call(&open(), &format!("b{i}"), op, args.clone());
            assert_eq!(code(&res), ErrorCode::BadRequest, "{op} {args}");
        }
        assert!(r.host.sent.lock().unwrap().is_empty());
    }

    #[test]
    fn an_approval_argument_is_refused_as_unknown_on_every_executable_op() {
        let r = rig();
        let approval = json!({"device":"d","target_machine":"m","nonce":"n","ts_ms":1,"alg":"es256","sig":"s"});
        for op in ["list", "read", "send", "ask"] {
            let mut args = valid_args(op);
            args["approval"] = approval.clone();
            let res = r.call(&open(), &format!("ap-{op}"), op, args);
            assert_eq!(code(&res), ErrorCode::BadRequest, "{op}");
        }
        assert!(r.host.sent.lock().unwrap().is_empty());
    }

    #[test]
    fn a_duplicate_send_returns_the_stored_answer_and_runs_once() {
        let r = rig();
        let first = r.call(&open(), "dup", "send", valid_args("send"));
        assert!(first.ok);
        let again = r.call(&open(), "dup", "send", valid_args("send"));
        assert_eq!(first, again);
        assert_eq!(r.host.sent.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_stored_answer_wins_even_if_the_duplicate_is_for_another_op() {
        let r = rig();
        r.call(&open(), "dup", "send", valid_args("send"));
        let again = r.call(&open(), "dup", "list", json!({}));
        assert!(again.ok);
        assert_eq!(data(&again), &json!({"sent": true}));
        assert_eq!(r.host.sent.lock().unwrap().len(), 1);
    }

    #[test]
    fn the_same_rpc_id_from_two_senders_is_independent() {
        let r = rig();
        let both = Policy::parse(&format!(
            "accept_from = [\"{PEER}\", \"{OTHER}\"]\nallow = [\"send\"]"
        ))
        .unwrap();
        let p = body("same", "send", valid_args("send"), T0);
        assert!(r.call_at(&both, PEER, &p).unwrap().ok);
        assert!(r.call_at(&both, OTHER, &p).unwrap().ok);
        assert_eq!(r.host.sent.lock().unwrap().len(), 2);
    }

    #[test]
    fn a_duplicate_ask_posts_once() {
        let r = rig();
        let a = r.call(&open(), "q1", "ask", valid_args("ask"));
        let b = r.call(&open(), "q1", "ask", valid_args("ask"));
        assert_eq!(a, b);
        let msgs = r.d.state().chat.history(&RoomRef::manager(ME), 10).unwrap();
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn a_duplicate_list_runs_again() {
        let r = rig();
        let first = r.call(&open(), "l1", "list", json!({}));
        let before = data(&first).as_array().unwrap().len();
        r.host.list.lock().unwrap().push(info("s_zzzzzzzzzz"));
        let second = r.call(&open(), "l1", "list", json!({}));
        assert_eq!(data(&second).as_array().unwrap().len(), before + 1);
    }

    #[test]
    fn a_duplicate_read_runs_again() {
        let r = rig();
        let first = r.call(
            &open(),
            "rd",
            "read",
            json!({"session": WORKER, "lines": 2}),
        );
        assert_eq!(data(&first), &json!("xxxxx\nxxxxx"));
        let second = r.call(
            &open(),
            "rd",
            "read",
            json!({"session": WORKER, "lines": 3}),
        );
        assert_eq!(data(&second), &json!("xxxxx\nxxxxx\nxxxxx"));
    }

    #[test]
    fn a_send_stamped_five_minutes_ahead_runs_once_even_ten_minutes_later() {
        let r = rig();
        let p = body("future", "send", valid_args("send"), T0 + limits::FRESH_MS);
        assert!(r.call_at(&open(), PEER, &p).unwrap().ok);
        r.host.set_now(T0 + 2 * limits::FRESH_MS);
        let again = r.call_at(&open(), PEER, &p).unwrap();
        assert!(again.ok);
        assert_eq!(r.host.sent.lock().unwrap().len(), 1);
        r.host
            .set_now(T0 + limits::FRESH_MS + limits::FRESH_MS + limits::SEEN_MARGIN_MS + 1);
        let late = r.call_at(&open(), PEER, &p).unwrap();
        assert_eq!(code(&late), ErrorCode::Stale);
        assert_eq!(r.host.sent.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_full_cache_refuses_new_requests_and_never_evicts_live_entries() {
        let mut r = rig();
        r.d.max_seen = 3;
        let p = policy(&["ask", "list"]);
        for i in 0..3 {
            assert!(r.call(&p, &format!("q{i}"), "ask", json!({"text": "x"})).ok);
        }
        let over = r.call(&p, "q3", "ask", json!({"text": "x"}));
        assert_eq!(code(&over), ErrorCode::RateLimited);
        let list = r.call(&p, "l0", "list", json!({}));
        assert_eq!(code(&list), ErrorCode::RateLimited);
        assert!(r.call(&p, "q0", "ask", json!({"text": "x"})).ok);
        let msgs = r.d.state().chat.history(&RoomRef::manager(ME), 10).unwrap();
        assert_eq!(msgs.len(), 3);
        r.host
            .set_now(T0 + limits::FRESH_MS + limits::SEEN_MARGIN_MS + 1);
        r.d.rate_per_min = 100;
        let ts = r.host.now_ms();
        let fresh = r
            .call_at(&p, PEER, &body("q3", "ask", json!({"text": "x"}), ts))
            .unwrap();
        assert!(fresh.ok);
    }

    #[test]
    fn read_carries_the_notice_and_the_text() {
        let r = rig();
        let res = r.call(
            &open(),
            "r1",
            "read",
            json!({"session": WORKER, "lines": 2}),
        );
        assert_eq!(data(&res), &json!("xxxxx\nxxxxx"));
        let res = r.call(&open(), "r2", "read", json!({"session": WORKER}));
        assert_eq!(
            data(&res).as_str().unwrap().lines().count(),
            mlimits::DEFAULT_READ_LINES as usize
        );
    }

    #[test]
    fn read_refuses_link_sessions_and_unknown_ones() {
        let r = rig();
        for (i, id) in [LINKED, "s_yyyyyyyyyy"].into_iter().enumerate() {
            let res = r.call(&open(), &format!("x{i}"), "read", json!({"session": id}));
            assert_eq!(code(&res), ErrorCode::Refused, "{id}");
        }
    }

    #[test]
    fn a_big_read_shrinks_until_the_response_fits() {
        let mut r = rig();
        r.host.line_len = 1000;
        let res = r.call(
            &open(),
            "big",
            "read",
            json!({"session": WORKER, "lines": 2000}),
        );
        assert!(res.ok);
        let wire = serde_json::to_vec(&res).unwrap();
        assert!(wire.len() <= limits::MAX_RES_BYTES);
        let text = data(&res).as_str().unwrap();
        assert!(text.lines().count() >= 500, "{}", text.lines().count());
    }

    #[test]
    fn a_single_line_over_the_cap_is_refused() {
        let mut r = rig();
        r.host.line_len = limits::MAX_RES_BYTES + 1;
        let res = r.call(
            &open(),
            "huge",
            "read",
            json!({"session": WORKER, "lines": 5}),
        );
        assert_eq!(code(&res), ErrorCode::Refused);
    }

    #[test]
    fn send_reaches_a_plain_session_once() {
        let r = rig();
        let res = r.call(
            &open(),
            "s1",
            "send",
            json!({"session": WORKER, "text": "do it"}),
        );
        assert_eq!(data(&res), &json!({"sent": true}));
        assert_eq!(
            *r.host.sent.lock().unwrap(),
            vec![(WORKER.to_string(), "do it".to_string())]
        );
    }

    #[test]
    fn send_keeps_every_local_refusal() {
        let r = rig();
        for (i, id) in [
            LINKED,
            TEAMMATE,
            BYPASS,
            UNSANDBOXED,
            EXITED,
            "s_yyyyyyyyyy",
        ]
        .into_iter()
        .enumerate()
        {
            let res = r.call(
                &open(),
                &format!("s{i}"),
                "send",
                json!({"session": id, "text": "x"}),
            );
            assert_eq!(code(&res), ErrorCode::Refused, "{id}");
        }
        assert!(r.host.sent.lock().unwrap().is_empty());
    }

    #[test]
    fn send_to_a_manager_session_is_refused_with_a_hint_to_ask() {
        let r = rig();
        for (i, id) in [MANAGER, OTHER_MANAGER].into_iter().enumerate() {
            let res = r.call(
                &open(),
                &format!("m{i}"),
                "send",
                json!({"session": id, "text": "x"}),
            );
            assert_eq!(code(&res), ErrorCode::Refused);
            assert!(res.error.unwrap().message.contains("ask"));
        }
        assert!(r.host.sent.lock().unwrap().is_empty());
    }

    #[test]
    fn list_hides_link_sessions() {
        let r = rig();
        let res = r.call(&open(), "l1", "list", json!({}));
        let ids: Vec<&str> = data(&res)
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect();
        assert!(!ids.contains(&LINKED));
        assert_eq!(ids.len(), 7);
        assert!(ids.contains(&WORKER));
    }

    #[test]
    fn ask_posts_once_with_provenance_and_this_machine_as_author_machine() {
        let r = rig();
        let res = r.call(&open(), "q1", "ask", json!({"text": "status please"}));
        assert!(data(&res)["id"].as_str().unwrap().starts_with("m_"));
        let msgs = r.d.state().chat.history(&RoomRef::manager(ME), 10).unwrap();
        assert_eq!(msgs.len(), 1);
        let m = &msgs[0];
        assert_eq!(m.author.kind, AuthorKind::Manager);
        assert_eq!(m.author.machine.as_deref(), Some(ME));
        assert_eq!(m.room, RoomRef::manager(ME));
        assert_eq!(
            m.body_md,
            format!("_From the top manager ({PEER})_\n\nstatus please")
        );
        m.validate().unwrap();
    }

    #[test]
    fn ask_accepts_the_largest_text_that_still_fits_the_room_body() {
        let r = rig();
        let max = chat::limits::MAX_BODY - provenance(PEER).len();
        let ok = r.call(&open(), "q1", "ask", json!({"text": "a".repeat(max)}));
        assert!(ok.ok);
        let over = r.call(&open(), "q2", "ask", json!({"text": "a".repeat(max + 1)}));
        assert_eq!(code(&over), ErrorCode::BadRequest);
    }

    #[test]
    fn errors_and_the_audit_log_never_carry_the_senders_text() {
        let r = rig();
        let secret = "TOPSECRETPAYLOAD";
        let mut seen = Vec::new();
        seen.push(r.call(
            &open(),
            "a1",
            "send",
            json!({"session": LINKED, "text": secret}),
        ));
        seen.push(r.call(
            &open(),
            "a2",
            "send",
            json!({"session": MANAGER, "text": secret}),
        ));
        seen.push(r.call(
            &open(),
            "a3",
            "send",
            json!({"session": WORKER, "text": secret, secret: 1}),
        ));
        seen.push(r.call(&open(), "a4", "ask", json!({"text": secret})));
        seen.push(r.call(&open(), "a5", "bogus", json!({"text": secret})));
        seen.push(r.call(&policy(&[]), "a6", "ask", json!({"text": secret})));
        r.call_at(
            &open(),
            PEER,
            format!("{{\"text\":\"{secret}\"}}").as_bytes(),
        );
        for s in &seen {
            if !s.ok {
                assert!(!serde_json::to_string(s).unwrap().contains(secret));
            }
        }
        let log = format!("{:?}", r.audit.records());
        assert!(!log.contains(secret), "{log}");
        assert_eq!(r.audit.records().len(), 7);
    }

    #[test]
    fn the_audit_record_has_op_session_and_outcome() {
        let r = rig();
        r.call(&open(), "a1", "read", json!({"session": WORKER}));
        r.call(&open(), "a2", "read", json!({"session": "junk"}));
        r.call(&policy(&[]), "a3", "list", json!({}));
        r.call_at(&open(), OTHER, &body("a4", "ask", json!({"text": "x"}), T0));
        let log = r.audit.records();
        assert_eq!(log[0].op, Some(Op::Read));
        assert_eq!(log[0].session.as_deref(), Some(WORKER));
        assert_eq!(log[0].outcome, "ok");
        assert_eq!(log[0].from, PEER);
        assert_eq!(log[0].rpc.as_deref(), Some("a1"));
        assert_eq!(log[0].ts_ms, T0);
        assert_eq!(log[1].session, None);
        assert_eq!(log[1].outcome, "bad_request");
        assert_eq!(log[2].outcome, "op_denied");
        assert_eq!(log[3].outcome, "not_allowed");
        assert_eq!(log[3].from, OTHER);
        assert_eq!((log[3].rpc.clone(), log[3].op), (None, None));
    }

    struct Hooked<'a> {
        inner: &'a Fake,
        on_send: Box<dyn Fn() + Send + Sync + 'a>,
    }

    impl crate::manager::Host for Hooked<'_> {
        fn me(&self) -> &str {
            self.inner.me()
        }
        fn machine(&self) -> String {
            self.inner.machine()
        }
        fn now_ms(&self) -> u64 {
            self.inner.now_ms()
        }
        fn sessions(&self) -> Vec<tmgr_ipc::api::SessionInfo> {
            self.inner.sessions()
        }
        fn text(&self, id: &str, lines: u32) -> Result<Vec<String>, String> {
            self.inner.text(id, lines)
        }
        fn send(&self, id: &str, text: &str) -> Result<(), String> {
            (self.on_send)();
            self.inner.send(id, text)
        }
        fn trusted_dir(&self, cwd: &str) -> Result<PathBuf, String> {
            self.inner.trusted_dir(cwd)
        }
        fn spawn(
            &self,
            spec: crate::manager::SpawnSpec,
        ) -> Result<tmgr_ipc::api::SessionInfo, String> {
            self.inner.spawn(spec)
        }
    }

    fn store_dir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("md")
            .tempdir_in("/tmp")
            .unwrap()
    }

    fn stored_dispatcher(path: &Path, now: u64) -> Dispatcher {
        Dispatcher::with_store(
            ME,
            State::new(Box::new(MemoryChat::default())),
            Arc::new(MemoryAudit::default()),
            Some(path.to_path_buf()),
            now,
        )
    }

    fn send_body(rpc: &str) -> Vec<u8> {
        body(rpc, "send", valid_args("send"), T0)
    }

    #[test]
    fn ask_author_machine_equals_the_room_machine() {
        let r = rig();
        r.call(&open(), "q1", "ask", valid_args("ask"));
        let msgs = r.d.state().chat.history(&RoomRef::manager(ME), 10).unwrap();
        assert_eq!(
            msgs[0].author.machine.as_deref(),
            Some(msgs[0].room.machine.as_str())
        );
    }

    #[test]
    fn after_a_restart_a_stored_answer_is_returned_and_send_ran_once() {
        let d = store_dir();
        let path = d.path().join(SEEN_FILE);
        let (first_host, second_host) = (Fake::new(), Fake::new());
        let first = stored_dispatcher(&path, T0)
            .handle(&open(), &first_host, PEER, &send_body("dup"))
            .unwrap();
        assert!(first.ok);
        let again = stored_dispatcher(&path, T0)
            .handle(&open(), &second_host, PEER, &send_body("dup"))
            .unwrap();
        assert_eq!(first, again);
        assert_eq!(first_host.sent.lock().unwrap().len(), 1);
        assert!(second_host.sent.lock().unwrap().is_empty());
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_restart_between_in_flight_and_store_refuses_and_never_runs_again() {
        let d = store_dir();
        let path = d.path().join(SEEN_FILE);
        let snap = d.path().join("snapshot.json");
        let base = Fake::new();
        let host = Hooked {
            inner: &base,
            on_send: Box::new(|| {
                std::fs::copy(&path, &snap).unwrap();
            }),
        };
        assert!(
            stored_dispatcher(&path, T0)
                .handle(&open(), &host, PEER, &send_body("dup"))
                .unwrap()
                .ok
        );
        let restarted = d.path().join("restarted.json");
        std::fs::copy(&snap, &restarted).unwrap();
        let after = Fake::new();
        let res = stored_dispatcher(&restarted, T0)
            .handle(&open(), &after, PEER, &send_body("dup"))
            .unwrap();
        assert_eq!(code(&res), ErrorCode::Unavailable);
        assert_eq!(res.error.unwrap().message, "outcome unknown");
        assert!(after.sent.lock().unwrap().is_empty());
    }

    #[test]
    fn expired_entries_are_dropped_on_load() {
        let d = store_dir();
        let path = d.path().join(SEEN_FILE);
        let host = Fake::new();
        stored_dispatcher(&path, T0).handle(&open(), &host, PEER, &send_body("dup"));
        let later = T0 + 2 * limits::FRESH_MS + limits::SEEN_MARGIN_MS;
        let fresh_host = Fake::new();
        fresh_host.set_now(later);
        let res = stored_dispatcher(&path, later)
            .handle(&open(), &fresh_host, PEER, &send_body("dup"))
            .unwrap();
        assert_eq!(code(&res), ErrorCode::Stale);
        assert!(fresh_host.sent.lock().unwrap().is_empty());
    }

    #[test]
    fn a_corrupt_store_refuses_send_until_old_requests_expire() {
        let d = store_dir();
        let path = d.path().join(SEEN_FILE);
        std::fs::write(&path, b"not json").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let host = Fake::new();
        let dispatcher = stored_dispatcher(&path, T0);
        let res = dispatcher
            .handle(&open(), &host, PEER, &send_body("dup"))
            .unwrap();
        assert_eq!(code(&res), ErrorCode::Unavailable);
        assert_eq!(res.error.unwrap().message, "request history unreadable");
        assert!(host.sent.lock().unwrap().is_empty());
        let later = T0 + 2 * limits::FRESH_MS + limits::SEEN_MARGIN_MS;
        host.set_now(later);
        let res = dispatcher
            .handle(
                &open(),
                &host,
                PEER,
                &body("fresh", "send", valid_args("send"), later),
            )
            .unwrap();
        assert!(res.ok);
        assert_eq!(host.sent.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_store_with_an_invalid_entry_is_rejected_whole() {
        let d = store_dir();
        let path = d.path().join(SEEN_FILE);
        let text = format!(
            r#"{{"v":1,"senders":{{"{PEER}":{{"bad id!":{{"expires_ms":{}}}}}}}}}"#,
            T0 + 1000
        );
        std::fs::write(&path, text).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let host = Fake::new();
        let res = stored_dispatcher(&path, T0)
            .handle(&open(), &host, PEER, &send_body("dup"))
            .unwrap();
        assert_eq!(code(&res), ErrorCode::Unavailable);
        assert!(host.sent.lock().unwrap().is_empty());
    }

    #[test]
    fn a_send_that_cannot_be_recorded_is_not_executed() {
        let d = store_dir();
        let path = d.path().join("missing-dir").join(SEEN_FILE);
        let host = Fake::new();
        let res = stored_dispatcher(&path, T0)
            .handle(&open(), &host, PEER, &send_body("dup"))
            .unwrap();
        assert_eq!(code(&res), ErrorCode::Unavailable);
        assert!(host.sent.lock().unwrap().is_empty());
    }

    #[test]
    fn a_concurrent_duplicate_never_executes_twice() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Barrier;
        let r = rig();
        let barrier = Barrier::new(2);
        let entered = AtomicBool::new(false);
        let host = Hooked {
            inner: &r.host,
            on_send: Box::new(|| {
                entered.store(true, Ordering::SeqCst);
                barrier.wait();
            }),
        };
        let p = open();
        let payload = send_body("dup");
        std::thread::scope(|s| {
            let first = s.spawn(|| r.d.handle(&p, &host, PEER, &payload));
            while !entered.load(Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            let dup = r.d.handle(&p, &host, PEER, &payload).unwrap();
            assert_eq!(code(&dup), ErrorCode::RateLimited);
            assert_eq!(dup.error.unwrap().message, "in progress");
            barrier.wait();
            assert!(first.join().unwrap().unwrap().ok);
        });
        assert_eq!(r.host.sent.lock().unwrap().len(), 1);
        let again = r.d.handle(&p, &host, PEER, &payload).unwrap();
        assert!(again.ok);
        assert_eq!(r.host.sent.lock().unwrap().len(), 1);
    }
}
