use super::probe::Probe;
use super::store::NO_SECURE_STORE;
use super::*;
use crate::cmd::chat::transport::failure;
use std::cell::{Cell, RefCell};
use tmgr_chat_relay::api::{Method, Request};
use tmgr_chat_relay::engine::{ChatSink, MemKeyStore, Slot};
use tmgr_chat_relay::wire::random_msg_id;

const LOCAL: &str = "mx_00000000000000aa";
const R1: &str = "mx_00000000000000bb";
const R2: &str = "mx_00000000000000cc";
const WEB: &str = "web-login";
const CLI: &str = "cli-login";
const ALL: &str = "*";

struct Dev {
    id: String,
    token: String,
    kind: String,
    label: String,
    sign_pub: String,
    kex_pub: String,
    intros: Vec<(String, String)>,
}

struct Msg {
    cursor: i64,
    msg_id: String,
    from: String,
    to: String,
    body: String,
}

struct Server {
    devices: Vec<Dev>,
    msgs: Vec<Msg>,
    revocations: Vec<(String, String, String)>,
    next: i64,
    minted: usize,
    cli_scopes: Vec<String>,
    log: Vec<(String, &'static str)>,
    delete_status: Option<u16>,
    delete_error: bool,
}

impl Default for Server {
    fn default() -> Self {
        Server {
            devices: Vec::new(),
            msgs: Vec::new(),
            revocations: Vec::new(),
            next: 0,
            minted: 0,
            cli_scopes: vec!["chat:devices".into()],
            log: Vec::new(),
            delete_status: None,
            delete_error: false,
        }
    }
}

enum Who {
    Login(Vec<String>),
    Device(String),
    Nobody,
}

#[derive(Clone, Default)]
struct Net(Arc<Mutex<Server>>);

#[derive(Clone)]
struct Via {
    net: Net,
    login: Option<&'static str>,
}

impl Via {
    fn web(net: &Net) -> Via {
        Via {
            net: net.clone(),
            login: Some(WEB),
        }
    }

    fn cli(net: &Net) -> Via {
        Via {
            net: net.clone(),
            login: Some(CLI),
        }
    }

    fn own_token(net: &Net) -> Via {
        Via {
            net: net.clone(),
            login: None,
        }
    }
}

impl Transport for Via {
    fn call(&self, req: &Request, device: Option<&str>) -> std::result::Result<String, String> {
        let t = HttpRelayTransport {
            base_url: "https://tmgr.dev".into(),
            login_token: self.login.map(str::to_owned),
        };
        let bearer = t.bearer(req.auth, device)?;
        self.net.serve(req, &bearer)
    }
}

fn cfg() -> RelayConfig {
    RelayConfig {
        server: "https://tmgr.dev".into(),
        user_id: 7,
    }
}

fn wrap(v: serde_json::Value) -> std::result::Result<String, String> {
    Ok(json!({ "data": v }).to_string())
}

fn status(code: u16) -> String {
    failure(code, None, json!({ "error": "denied" }).to_string())
}

impl Net {
    fn serve(&self, req: &Request, bearer: &str) -> std::result::Result<String, String> {
        let mut s = self.0.lock().unwrap();
        let who = match bearer {
            WEB => Who::Login(vec![ALL.into()]),
            CLI => Who::Login(s.cli_scopes.clone()),
            t => s
                .devices
                .iter()
                .find(|d| d.token == t)
                .map_or(Who::Nobody, |d| Who::Device(d.id.clone())),
        };
        let label = match bearer {
            WEB => "web",
            CLI => "cli",
            _ => "device",
        };
        s.log
            .push((format!("{} {}", req.method.as_str(), req.path), label));
        let full_login = |who: &Who| matches!(who, Who::Login(sc) if sc.iter().any(|x| x == ALL));
        let path = req.path.trim_start_matches("/api/chat-relay").to_owned();
        let body = req.body.clone().unwrap_or(serde_json::Value::Null);
        let seg: Vec<&str> = path
            .split('?')
            .next()
            .unwrap()
            .trim_matches('/')
            .split('/')
            .collect();
        let device = |who: &Who| match who {
            Who::Device(id) => Ok(id.clone()),
            _ => Err(status(401)),
        };
        match (req.method, seg.as_slice()) {
            (Method::Post, ["devices"]) => {
                let Who::Login(scopes) = &who else {
                    return Err(status(401));
                };
                let all = scopes.iter().any(|x| x == ALL);
                let kind = body["kind"].as_str().unwrap().to_owned();
                if !all && !scopes.iter().any(|x| x == "chat:devices") {
                    return Err(status(403));
                }
                if !all && !matches!(kind.as_str(), "runner" | "terminal") {
                    return Err(status(403));
                }
                s.minted += 1;
                let id = format!("00000000-0000-4000-8000-{:012x}", s.minted + 0x1000);
                let token = format!("tok-{id}");
                let g = |k: &str| body[k].as_str().unwrap().to_owned();
                s.devices.push(Dev {
                    id: id.clone(),
                    token: token.clone(),
                    kind,
                    label: g("label"),
                    sign_pub: g("sign_pub"),
                    kex_pub: g("kex_pub"),
                    intros: vec![],
                });
                wrap(json!({ "device_id": id, "device_token": token }))
            }
            (Method::Get, ["devices"]) => {
                if matches!(who, Who::Nobody) {
                    return Err(status(401));
                }
                if !matches!(who, Who::Device(_)) && !full_login(&who) {
                    return Err(status(403));
                }
                let devices: Vec<_> = s
                    .devices
                    .iter()
                    .map(|d| json!({ "device_id": d.id, "kind": d.kind, "label": d.label, "sign_pub": d.sign_pub, "kex_pub": d.kex_pub, "intros": d.intros.iter().map(|i| i.1.clone()).collect::<Vec<_>>(), "last_seen_at": null }))
                    .collect();
                let revs: Vec<_> = s
                    .revocations
                    .iter()
                    .map(|r| json!({ "target_fp": r.0, "statement": r.2 }))
                    .collect();
                wrap(json!({ "devices": devices, "revocations": revs }))
            }
            (Method::Put, ["devices", target, "intros", "self"]) => {
                let by = device(&who)?;
                let intro = body["intro"].as_str().unwrap().to_owned();
                let d = s
                    .devices
                    .iter_mut()
                    .find(|d| d.id == *target)
                    .ok_or_else(|| status(404))?;
                d.intros.retain(|i| i.0 != by);
                d.intros.push((by, intro));
                Ok(String::new())
            }
            (Method::Delete, ["devices", target]) => {
                match &who {
                    Who::Nobody => return Err(status(401)),
                    Who::Login(_) if !full_login(&who) => return Err(status(403)),
                    Who::Device(id) if id != target => return Err(status(403)),
                    _ => {}
                }
                if s.delete_error {
                    return Err("boom".into());
                }
                let existed = s.devices.iter().any(|d| d.id == *target);
                s.devices.retain(|d| d.id != *target);
                s.msgs.retain(|m| m.to != *target && m.from != *target);
                if let Some(code) = s.delete_status {
                    return Err(status(code));
                }
                if existed {
                    Ok(String::new())
                } else {
                    Err(status(404))
                }
            }
            (Method::Post, ["messages"]) => {
                let from = device(&who)?;
                let msg_id = body["msg_id"].as_str().unwrap().to_owned();
                let items = body["items"].as_array().unwrap().clone();
                if items
                    .iter()
                    .any(|i| !s.devices.iter().any(|d| d.id == i["to"].as_str().unwrap()))
                {
                    return Err(r#"{"error":"recipient_unknown"}"#.into());
                }
                let mut stored = 0;
                for i in items {
                    let to = i["to"].as_str().unwrap().to_owned();
                    if s.msgs
                        .iter()
                        .any(|m| m.to == to && m.from == from && m.msg_id == msg_id)
                    {
                        continue;
                    }
                    s.next += 1;
                    let cursor = s.next;
                    s.msgs.push(Msg {
                        cursor,
                        msg_id: msg_id.clone(),
                        from: from.clone(),
                        to,
                        body: i["body"].as_str().unwrap().into(),
                    });
                    stored += 1;
                }
                wrap(json!({ "stored": stored, "duplicates": 0 }))
            }
            (Method::Get, ["messages"]) => {
                let me = device(&who)?;
                let after: i64 = path
                    .split("after=")
                    .nth(1)
                    .unwrap()
                    .split('&')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap();
                let mine: Vec<_> = s
                    .msgs
                    .iter()
                    .filter(|m| m.to == me && m.cursor > after)
                    .map(|m| json!({ "cursor": m.cursor, "msg_id": m.msg_id, "from": m.from, "body": m.body }))
                    .collect();
                wrap(json!({ "messages": mine, "more": false }))
            }
            (Method::Post, ["messages", "ack"]) => {
                let me = device(&who)?;
                let through = body["through"].as_i64().unwrap();
                s.msgs.retain(|m| !(m.to == me && m.cursor <= through));
                wrap(json!({ "deleted": 0 }))
            }
            (Method::Post, ["revocations"]) => {
                let by = device(&who)?;
                let fp = body["target_fp"].as_str().unwrap().to_owned();
                if !s.revocations.iter().any(|r| r.0 == fp && r.1 == by) {
                    s.revocations
                        .push((fp, by, body["statement"].as_str().unwrap().to_owned()));
                }
                Ok(String::new())
            }
            _ => Err(format!(
                "fake: unhandled {} {}",
                req.method.as_str(),
                req.path
            )),
        }
    }

    fn mark(&self) -> usize {
        self.0.lock().unwrap().log.len()
    }

    fn calls_since(&self, mark: usize) -> Vec<(String, &'static str)> {
        self.0.lock().unwrap().log[mark..].to_vec()
    }

    fn devices(&self) -> Vec<(String, String)> {
        self.0
            .lock()
            .unwrap()
            .devices
            .iter()
            .map(|d| (d.id.clone(), d.kind.clone()))
            .collect()
    }
}

#[derive(Clone, Default)]
struct Keys(Arc<MemKeyStore>);

impl KeyStore for Keys {
    fn load(&self, slot: Slot) -> std::result::Result<Option<String>, String> {
        self.0.load(slot)
    }
    fn save(&self, slot: Slot, value: &str) -> std::result::Result<(), String> {
        self.0.save(slot, value)
    }
    fn clear(&self) -> std::result::Result<(), String> {
        self.0.clear()
    }
}

type Body = (String, Option<String>);

#[derive(Clone, Default)]
struct Rec(Arc<Mutex<Vec<Body>>>, Arc<std::sync::atomic::AtomicBool>);

impl ChatSink for Rec {
    fn accepts_owner_input(&self, kind: Option<&str>) -> bool {
        !self.1.load(std::sync::atomic::Ordering::SeqCst)
            && matches!(kind, Some("desktop" | "mobile" | "terminal"))
    }

    fn deliver(
        &self,
        _: &str,
        kind: Option<&str>,
        msg: ChatMessage,
    ) -> std::result::Result<(), String> {
        if !self.accepts_owner_input(kind) {
            return Ok(());
        }
        self.0
            .lock()
            .unwrap()
            .push((msg.body_md, kind.map(str::to_owned)));
        Ok(())
    }
}

struct Runner {
    engine: RelayEngine<Via, Keys, Rec>,
    rec: Rec,
    machine: &'static str,
    capable: bool,
    active: Cell<bool>,
    last_owners: RefCell<Vec<String>>,
}

struct World {
    net: Net,
    owner: RelayEngine<Via, Keys, Rec>,
    runners: RefCell<Vec<Runner>>,
    approvals: Cell<usize>,
    ticks: Cell<usize>,
    keys: Keys,
    cache: Arc<RoomCache>,
    clock: Clock,
    dir: tempfile::TempDir,
}

const T0: u64 = 1_700_000_000_000;
const TICK_MS: u64 = 1_000;

type Clock = Arc<std::sync::atomic::AtomicU64>;

fn engine(
    net: &Net,
    clock: &Clock,
    label: &str,
    kind: &str,
    machine: &str,
    scope: RoomScope,
) -> (RelayEngine<Via, Keys, Rec>, Rec) {
    let rec = Rec::default();
    let e = RelayEngine::new(
        Via::web(net),
        Keys::default(),
        rec.clone(),
        EngineConfig {
            user_id: 7,
            label: label.into(),
            local_machine: machine.into(),
        },
    )
    .unwrap()
    .with_clock(clock_fn(clock))
    .with_kind(kind)
    .with_room_scope(scope);
    (e, rec)
}

fn clock_fn(clock: &Clock) -> impl Fn() -> u64 + Send + Sync + 'static {
    let c = clock.clone();
    move || c.load(std::sync::atomic::Ordering::SeqCst)
}

impl World {
    fn new() -> World {
        let net = Net::default();
        let clock: Clock = Arc::new(std::sync::atomic::AtomicU64::new(T0));
        let (owner, _) = engine(
            &net,
            &clock,
            "Owner",
            "desktop",
            "mx_00000000000000dd",
            RoomScope::Local,
        );
        let dir = tempfile::tempdir().unwrap();
        World {
            net,
            owner,
            runners: RefCell::default(),
            approvals: Cell::new(0),
            ticks: Cell::new(0),
            keys: Keys::default(),
            cache: RoomCache::memory(),
            clock,
            dir,
        }
    }

    fn add_runner(&self, machine: &'static str, capable: bool, active: bool) {
        let (e, rec) = engine(
            &self.net,
            &self.clock,
            "Runner",
            "runner",
            machine,
            RoomScope::Local,
        );
        e.join_with_code(&self.owner.open_join_code().unwrap())
            .unwrap();
        self.owner.poll_once().unwrap();
        self.owner
            .decide_join(&e.own_device().unwrap().0, true)
            .unwrap();
        e.poll_once().unwrap();
        self.runners.borrow_mut().push(Runner {
            engine: e,
            rec,
            machine,
            capable,
            active: Cell::new(active),
            last_owners: RefCell::default(),
        });
    }

    fn refuse_input(&self, machine: &str) {
        let rs = self.runners.borrow();
        let r = rs.iter().find(|r| r.machine == machine).unwrap();
        r.rec.1.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    fn tick(&self) {
        self.ticks.set(self.ticks.get() + 1);
        self.clock
            .fetch_add(TICK_MS, std::sync::atomic::Ordering::SeqCst);
        let _ = self.owner.poll_once();
        for pj in self.owner.trust_state().unwrap().pending_joins {
            if self.owner.decide_join(&pj.device_id, true).is_ok() {
                self.approvals.set(self.approvals.get() + 1);
            }
        }
        for r in self.runners.borrow().iter().filter(|r| r.active.get()) {
            let _ = r.engine.poll_once();
            let owners = r.engine.owner_device_ids().unwrap_or_default();
            if owners != *r.last_owners.borrow() || self.ticks.get().is_multiple_of(4) {
                let index = json!({
                    "v": 1, "machine": r.machine, "machine_label": format!("Box {}", &r.machine[r.machine.len()-2..]),
                    "role": "top", "terminal_input": r.capable,
                    "rooms": [{"room": {"kind": "session", "machine": r.machine, "session": "s_1"}, "title": "work"}]
                });
                let id = random_msg_id("rooms").unwrap();
                if r.engine
                    .send_sealed_to_owners("rooms", &id, &index, None)
                    .is_ok()
                {
                    *r.last_owners.borrow_mut() = owners;
                }
            }
        }
    }

    fn relay_dir(&self) -> RelayDir {
        config_dir(self.dir.path()).unwrap()
    }

    fn device(&self, via: Via) -> AttachDevice<Via, Keys> {
        self.clocked(build(via, self.keys.clone(), self.cache.clone(), &cfg(), LOCAL).unwrap())
    }

    fn clocked(&self, dev: AttachDevice<Via, Keys>) -> AttachDevice<Via, Keys> {
        AttachDevice {
            engine: dev.engine.with_clock(clock_fn(&self.clock)),
            ..dev
        }
    }

    fn cli_device(&self) -> AttachDevice<Via, Keys> {
        self.device(Via::cli(&self.net))
    }

    fn pair(&self, dev: &mut AttachDevice<Via, Keys>) -> Result<PairOutcome> {
        let read = || self.owner.open_join_code().map_err(Error::internal);
        let sleep = |_| self.tick();
        let say = |_: &str| {};
        let own = || Ok(self.device(Via::own_token(&self.net)));
        let login = || Ok(self.cli_device());
        let out = pair_with(&own, &login, &self.relay_dir(), &cfg(), &read, &sleep, &say)?;
        *dev = self.cli_device();
        Ok(out)
    }

    fn bodies(&self, machine: &str) -> Vec<Body> {
        let rs = self.runners.borrow();
        let got = rs
            .iter()
            .find(|r| r.machine == machine)
            .unwrap()
            .rec
            .0
            .lock()
            .unwrap()
            .clone();
        got
    }

    fn unpair(&self) -> Result<String> {
        unpair_with(self.dir.path(), |parts| {
            build(
                Via::own_token(&self.net),
                self.keys.clone(),
                RoomCache::open(config_dir(self.dir.path())?)?,
                &parts.cfg,
                "",
            )
            .map(|dev| self.clocked(dev))
        })
    }
}

fn room(machine: &str) -> RoomRef {
    RoomRef::session(machine, "s_1")
}

#[test]
fn the_fake_relay_enforces_the_backend_rule_for_a_cli_token() {
    let net = Net::default();
    let cli = Via::cli(&net);
    let register = |kind: &str| {
        let keys = tmgr_chat_relay::keys::DeviceKeys::generate().unwrap();
        let req = tmgr_chat_relay::api::register_device(
            kind,
            "x",
            &keys.sign_pub_b64(),
            &keys.kex_pub_b64(),
            None,
        )
        .unwrap();
        cli.call(&req, None)
    };
    assert!(register("terminal").is_ok());
    assert!(register("runner").is_ok());
    for kind in ["desktop", "mobile"] {
        let e = register(kind).unwrap_err();
        let v: serde_json::Value = serde_json::from_str(&e).unwrap();
        assert_eq!(v["http_status"], 403, "{kind}");
    }
    for req in [
        tmgr_chat_relay::api::list_devices(),
        tmgr_chat_relay::api::delete_device(&net.devices()[0].0).unwrap(),
    ] {
        let e = Via {
            net: net.clone(),
            login: Some(CLI),
        }
        .call(&req, None)
        .unwrap_err();
        let v: serde_json::Value = serde_json::from_str(&e).unwrap();
        assert_eq!(v["http_status"], 403, "{}", req.path);
    }
    net.0.lock().unwrap().cli_scopes = vec![ALL.into()];
    assert!(cli
        .call(&tmgr_chat_relay::api::list_devices(), None)
        .is_ok());
}

#[test]
fn pair_with_a_cli_token_registers_a_terminal_after_one_approval_and_never_lists_or_deletes_with_the_login(
) {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mark = w.net.mark();
    let mut dev = w.cli_device();
    let out = w.pair(&mut dev).unwrap();
    assert!(!out.already);
    assert_eq!(w.approvals.get(), 1);
    assert_eq!(out.probe, Probe::Confirmed);
    assert!(w.net.devices().iter().any(|d| d.1 == "terminal"));
    assert!(w
        .net
        .devices()
        .iter()
        .all(|d| d.1 != "desktop" || d.0 == w.owner.own_device().unwrap().0));
    assert!(dev.is_paired().unwrap());
    let cli_calls: Vec<String> = w
        .net
        .calls_since(mark)
        .into_iter()
        .filter(|c| c.1 == "cli")
        .map(|c| c.0)
        .collect();
    assert_eq!(cli_calls.len(), 1, "{cli_calls:?}");
    assert!(cli_calls[0].starts_with("POST") && cli_calls[0].ends_with("/devices"));
    assert_eq!(w.relay_dir().load_config().unwrap(), Some(cfg()));
}

#[test]
fn a_paired_terminal_writes_into_a_remote_session_and_the_runner_sees_the_terminal_kind() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    for _ in 0..3 {
        w.tick();
    }
    dev.poll().unwrap();
    dev.send(&room(R1), "from the terminal").unwrap();
    w.tick();
    assert_eq!(
        w.bodies(R1),
        vec![("from the terminal".to_owned(), Some("terminal".to_owned()))]
    );
}

#[test]
fn a_login_without_the_chat_devices_scope_cannot_pair_and_is_told_how_to_fix_it() {
    let w = World::new();
    w.add_runner(R1, true, true);
    w.net.0.lock().unwrap().cli_scopes = vec!["tasks:read".into()];
    let mut dev = w.cli_device();
    let e = w.pair(&mut dev).err().expect("refused");
    assert_eq!(e.exit_code(), 5);
    let hint = e.hint.unwrap_or_default();
    assert!(
        hint.contains("tmgr login --scopes") && hint.contains("chat:devices"),
        "{hint}"
    );
    assert!(w.net.devices().iter().all(|d| d.1 != "terminal"));
}

#[test]
fn a_second_pair_is_already_paired_and_only_re_checks() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    let approvals = w.approvals.get();
    let again = w.pair(&mut dev).unwrap();
    assert!(again.already);
    assert_eq!(w.approvals.get(), approvals);
    assert_eq!(
        w.net.devices().iter().filter(|d| d.1 == "terminal").count(),
        1
    );
}

#[test]
fn the_probe_reports_machines_that_did_not_accept_and_changes_nothing() {
    let w = World::new();
    w.add_runner(R1, true, true);
    w.add_runner(R2, true, true);
    w.refuse_input(R2);
    let mut dev = w.cli_device();
    let mark = w.net.mark();
    let out = w.pair(&mut dev).unwrap();
    assert_eq!(out.probe, Probe::Unconfirmed(vec!["Box cc".to_owned()]));
    let text = out.probe.text().unwrap();
    assert!(
        text.starts_with("Runner on Box cc did not confirm"),
        "{text}"
    );
    assert!(w
        .net
        .calls_since(mark)
        .iter()
        .all(|c| !c.0.starts_with("DELETE")));
    assert!(dev.is_paired().unwrap());
    assert!(w.net.devices().iter().any(|d| d.1 == "terminal"));
}

#[test]
fn a_machine_that_never_answered_is_named_by_its_device_id_when_it_has_no_label() {
    let w = World::new();
    w.add_runner(R1, true, true);
    w.add_runner(R2, true, false);
    let mut dev = w.cli_device();
    let out = w.pair(&mut dev).unwrap();
    let silent = w.runners.borrow()[1].engine.own_device().unwrap().0;
    assert_eq!(out.probe, Probe::Unconfirmed(vec![silent]));
}

#[test]
fn a_probe_with_no_runners_is_skipped_and_the_pair_still_succeeds() {
    let w = World::new();
    let mut dev = w.cli_device();
    let out = w.pair(&mut dev).unwrap();
    assert!(matches!(out.probe, Probe::Skipped(_)));
    assert!(dev.is_paired().unwrap());
}

#[test]
fn a_paired_terminal_refuses_a_remote_session_of_a_non_capable_machine_and_still_writes_elsewhere()
{
    let w = World::new();
    w.add_runner(R1, true, true);
    w.add_runner(R2, false, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    for _ in 0..3 {
        w.tick();
    }
    dev.poll().unwrap();
    let e = dev.send(&room(R2), "nope").unwrap_err();
    assert_eq!(
        e,
        "Runner on Box cc does not accept terminal input. Update tmgr there"
    );
    dev.send(&room(R1), "ok").unwrap();
    w.tick();
    assert!(w.bodies(R2).is_empty());
    assert_eq!(w.bodies(R1).len(), 1);
    assert!(!dev.handles(&RoomRef::manager(R2)));
    assert!(!dev.handles(&RoomRef::manager(R1)));
}

#[test]
fn a_failed_receipt_from_the_bound_runner_becomes_a_note() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    for _ in 0..3 {
        w.tick();
    }
    dev.poll().unwrap();
    let m = dev.send(&room(R1), "hello").unwrap();
    w.tick();
    let runners = w.runners.borrow();
    runners[0]
        .engine
        .send_receipt(
            &dev.engine.own_device().unwrap().0,
            &m.id,
            ReceiptState::Failed,
            Some("session is busy"),
        )
        .unwrap();
    dev.poll().unwrap();
    assert_eq!(
        dev.receipt_notes(),
        ["remote send failed: session is busy".to_owned()]
    );
}

#[test]
fn unpair_deletes_the_row_with_the_devices_own_token_clears_keys_and_config_and_is_idempotent() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    let id = dev.engine.own_device().unwrap().0;
    let mark = w.net.mark();
    let human = w.unpair().unwrap();
    assert!(human.starts_with("Unpaired"), "{human}");
    assert!(w.net.devices().iter().all(|d| d.0 != id));
    assert!(w.keys.load(Slot::Identity).unwrap().is_none());
    assert_eq!(w.relay_dir().load_config().unwrap(), None);
    let calls = w.net.calls_since(mark);
    assert!(calls
        .iter()
        .any(|c| c.0.starts_with("DELETE") && c.1 == "device"));
    assert!(calls.iter().all(|c| c.1 == "device"), "{calls:?}");
    let after = w.net.mark();
    assert_eq!(w.unpair().unwrap(), "Not paired.");
    assert_eq!(w.net.mark(), after);
}

#[test]
fn unpair_tells_the_other_devices_with_a_self_revoke_first() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    let fp = dev.engine.trust_state().unwrap().self_fp.unwrap();
    w.unpair().unwrap();
    assert!(w
        .net
        .0
        .lock()
        .unwrap()
        .revocations
        .iter()
        .any(|r| r.0 == fp));
}

#[test]
fn unpair_succeeds_when_the_relay_already_forgot_the_device_by_401() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    let id = dev.engine.own_device().unwrap().0;
    w.net.0.lock().unwrap().devices.retain(|d| d.id != id);
    let human = w.unpair().unwrap();
    assert!(human.starts_with("Unpaired"), "{human}");
    assert!(w.keys.load(Slot::Identity).unwrap().is_none());
    assert_eq!(w.relay_dir().load_config().unwrap(), None);
}

#[test]
fn unpair_succeeds_when_the_relay_answers_404_and_the_device_is_confirmed_absent() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    w.net.0.lock().unwrap().delete_status = Some(404);
    let human = w.unpair().unwrap();
    assert!(human.starts_with("Unpaired"), "{human}");
    assert!(w.keys.load(Slot::Identity).unwrap().is_none());
    assert_eq!(w.relay_dir().load_config().unwrap(), None);
}

#[test]
fn a_failed_unpair_keeps_the_keys_and_the_config_and_names_the_device() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    let id = dev.engine.own_device().unwrap().0;
    w.net.0.lock().unwrap().delete_error = true;
    let e = w.unpair().unwrap_err();
    assert!(e.message.contains(&id), "{}", e.message);
    let hint = e.hint.unwrap_or_default();
    assert!(hint.contains("tmgr device unpair again"), "{hint}");
    assert!(!e.message.contains("nothing was deleted"), "{}", e.message);
    assert!(e.message.contains("already revoked"), "{}", e.message);
    assert!(w.keys.load(Slot::Identity).unwrap().is_some());
    assert_eq!(w.relay_dir().load_config().unwrap(), Some(cfg()));
    w.net.0.lock().unwrap().delete_error = false;
    assert!(w.unpair().unwrap().starts_with("Unpaired"));
}

#[test]
fn unpair_after_a_crash_between_clearing_the_keys_and_the_config_finishes_cleanly() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    w.device(Via::own_token(&w.net))
        .engine
        .leave_confirmed()
        .unwrap();
    assert!(w.relay_dir().load_config().unwrap().is_some());
    assert_eq!(w.unpair().unwrap(), "Not paired.");
    assert_eq!(w.relay_dir().load_config().unwrap(), None);
}

#[test]
fn unpair_wipes_the_room_cache_file() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.device(Via::cli(&w.net));
    w.pair(&mut dev).unwrap();
    let cache = RoomCache::open(config_dir(w.dir.path()).unwrap()).unwrap();
    cache.add(owner_message(&room(R1), "cached", 1)).unwrap();
    let file = w.dir.path().join(DIR).join("cache.json");
    assert!(file.exists());
    w.unpair().unwrap();
    assert!(!file.exists());
}

#[test]
fn the_paired_device_is_opened_by_attach_only_after_a_real_pairing() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let make = |_: &Parts, _: &str| Ok(w.device(Via::own_token(&w.net)));
    assert!(open_with(w.dir.path(), LOCAL, make).unwrap().is_none());
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    drop(dev);
    let opened = open_with(w.dir.path(), LOCAL, |_: &Parts, _: &str| {
        Ok(w.device(Via::own_token(&w.net)))
    })
    .unwrap()
    .expect("paired");
    assert!(opened.is_paired().unwrap());
}

struct StepUpRun {
    prompts: RefCell<Vec<String>>,
    logins: RefCell<Vec<Vec<String>>>,
}

impl StepUpRun {
    fn run(
        w: &World,
        have: &[&str],
        interactive: bool,
        answer: bool,
    ) -> (StepUpRun, Result<PairOutcome>) {
        let run = StepUpRun {
            prompts: RefCell::default(),
            logins: RefCell::default(),
        };
        w.net.0.lock().unwrap().cli_scopes = have.iter().map(|s| (*s).to_owned()).collect();
        let have: Vec<String> = have.iter().map(|s| (*s).to_owned()).collect();
        let ask = |p: &str| {
            run.prompts.borrow_mut().push(p.to_owned());
            answer
        };
        let login = |scopes: &[String]| {
            run.logins.borrow_mut().push(scopes.to_vec());
            w.net.0.lock().unwrap().cli_scopes = scopes.to_vec();
            Ok(())
        };
        let seams = step_up::Seams {
            interactive,
            ask: &ask,
            login: &login,
        };
        let out = step_up::pair_after_step_up(Some(&have), &seams, || {
            let mut dev = w.cli_device();
            w.pair(&mut dev)
        });
        (run, out)
    }
}

#[test]
fn a_missing_scope_accepted_logs_in_again_with_the_old_scopes_plus_chat_devices_and_pairs() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let (run, out) = StepUpRun::run(&w, &["tasks:read", "workspaces:read"], true, true);
    assert!(out.is_ok(), "{:?}", out.err());
    assert_eq!(
        run.prompts.borrow().as_slice(),
        [step_up::PROMPT.to_owned()]
    );
    assert_eq!(
        run.logins.borrow().as_slice(),
        [vec![
            "tasks:read".to_owned(),
            "workspaces:read".to_owned(),
            "chat:devices".to_owned()
        ]]
    );
    assert!(w.net.devices().iter().any(|d| d.1 == "terminal"));
}

#[test]
fn a_missing_scope_declined_keeps_the_hint_and_neither_logs_in_nor_registers() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let (run, out) = StepUpRun::run(&w, &["tasks:read"], true, false);
    let e = out.err().expect("refused");
    assert_eq!(e.exit_code(), 5);
    assert!(e.hint.unwrap_or_default().contains("tmgr login --scopes"));
    assert_eq!(run.prompts.borrow().len(), 1);
    assert!(run.logins.borrow().is_empty());
    assert!(w.net.devices().iter().all(|d| d.1 != "terminal"));
}

#[test]
fn a_missing_scope_without_a_terminal_gives_the_hint_and_never_prompts() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let (run, out) = StepUpRun::run(&w, &["tasks:read"], false, true);
    let e = out.err().expect("refused");
    assert_eq!(e.exit_code(), 5);
    assert!(e.hint.unwrap_or_default().contains("chat:devices"));
    assert!(run.prompts.borrow().is_empty());
    assert!(run.logins.borrow().is_empty());
    assert!(w.net.devices().iter().all(|d| d.1 != "terminal"));
}

#[test]
fn a_token_with_the_scope_pairs_without_a_prompt_or_login() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let (run, out) = StepUpRun::run(&w, &["tasks:read", "chat:devices"], true, true);
    assert!(out.is_ok(), "{:?}", out.err());
    assert!(run.prompts.borrow().is_empty());
    assert!(run.logins.borrow().is_empty());
}

#[test]
fn unknown_scopes_fall_back_to_the_defaults_plus_chat_devices() {
    let wanted = step_up::wanted_scopes(&[]);
    assert_eq!(wanted.len(), tmgr_core::api::DEFAULT_SCOPES.len() + 1);
    assert_eq!(wanted.last().map(String::as_str), Some("chat:devices"));
}

fn terminal_id(dev: &AttachDevice<Via, Keys>) -> String {
    dev.engine.own_device().unwrap().0
}

fn settle(w: &World) {
    for _ in 0..3 {
        w.tick();
    }
}

#[test]
fn unpair_without_a_secure_store_is_refused_and_keeps_the_config() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    let e = unpair_with(w.dir.path(), |_| {
        Err::<AttachDevice<Via, Keys>, String>(NO_SECURE_STORE.to_owned())
    })
    .expect_err("refused");
    assert!(
        e.message.contains("No secure key store available"),
        "{}",
        e.message
    );
    assert_eq!(w.relay_dir().load_config().unwrap(), Some(cfg()));
}

#[test]
fn pairing_an_already_paired_terminal_restores_a_missing_config() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    w.relay_dir().clear_config().unwrap();
    let out = w.pair(&mut dev).unwrap();
    assert!(out.already);
    assert_eq!(w.relay_dir().load_config().unwrap(), Some(cfg()));
}

#[test]
fn pairing_without_a_secure_store_is_refused_and_no_key_file_is_written() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let absent = || Err::<AttachDevice<Via, Keys>, String>(NO_SECURE_STORE.to_owned());
    let read = || -> Result<String> { panic!("no join code is read without a secure store") };
    let sleep = |_| w.tick();
    let say = |_: &str| {};
    let e = pair_with(
        &absent,
        &absent,
        &w.relay_dir(),
        &cfg(),
        &read,
        &sleep,
        &say,
    )
    .err()
    .expect("refused");
    assert!(
        e.message.contains("No secure key store available"),
        "{}",
        e.message
    );
    assert!(e.message.contains("Secret Service on Linux"));
    assert!(w.net.devices().iter().all(|d| d.1 != "terminal"));
    assert_eq!(w.relay_dir().load_config().unwrap(), None);
    let files: Vec<String> = std::fs::read_dir(w.dir.path().join(DIR))
        .unwrap()
        .map(|f| f.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    for secret in ["identity", "state", "log"] {
        assert!(!files.iter().any(|f| f == secret), "{files:?}");
    }
}

#[test]
fn status_says_a_revoked_terminal_was_removed_from_the_devices() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    assert_eq!(standing(&dev).unwrap().0, "paired");
    w.owner.revoke(&terminal_id(&dev)).unwrap();
    let own = w.device(Via::own_token(&w.net));
    assert_eq!(
        standing(&own).unwrap(),
        (
            "removed",
            "This terminal was removed from your devices. Run tmgr device pair to pair it again."
                .to_owned()
        )
    );
}

#[test]
fn status_while_attach_holds_the_device_says_in_use_or_removed() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    let _held = super::lock::acquire(w.dir.path()).unwrap().unwrap();
    let status = || device_with(w.dir.path(), |_| Ok(w.device(Via::own_token(&w.net)))).unwrap();
    let (data, text) = status();
    assert_eq!(data["state"], "in_use");
    assert_eq!(text, "Paired and in use by tmgr attach.");
    w.owner.revoke(&terminal_id(&dev)).unwrap();
    let (data, text) = status();
    assert_eq!(data["state"], "removed");
    assert_eq!(text, REMOVED_TEXT);
}

#[test]
fn status_says_removed_when_the_revoke_row_delete_failed() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    settle(&w);
    w.net.0.lock().unwrap().delete_error = true;
    assert!(w.owner.revoke(&terminal_id(&dev)).is_ok());
    w.net.0.lock().unwrap().delete_error = false;
    let own = w.device(Via::own_token(&w.net));
    assert_eq!(standing(&own).unwrap().0, "removed");
}

#[test]
fn pairing_replaces_a_terminal_whose_revoke_row_delete_failed_in_one_run() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    settle(&w);
    let old = terminal_id(&dev);
    w.net.0.lock().unwrap().delete_error = true;
    assert!(w.owner.revoke(&old).is_ok());
    w.net.0.lock().unwrap().delete_error = false;
    let out = w.pair(&mut dev).unwrap();
    assert!(out.replaced && !out.already);
    assert_ne!(terminal_id(&dev), old);
    assert!(dev.is_paired().unwrap());
}

#[test]
fn pairing_a_removed_terminal_clears_the_stale_state_and_pairs_fresh_in_one_run() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    let old = terminal_id(&dev);
    w.owner.revoke(&old).unwrap();
    let approvals = w.approvals.get();
    let mark = w.net.mark();
    let out = w.pair(&mut dev).unwrap();
    assert!(out.replaced && !out.already);
    let new = terminal_id(&dev);
    assert_ne!(old, new);
    assert_eq!(w.approvals.get(), approvals + 1);
    assert!(w.net.devices().iter().all(|d| d.0 != old));
    assert!(w
        .net
        .devices()
        .iter()
        .any(|d| d.0 == new && d.1 == "terminal"));
    assert!(dev.is_paired().unwrap());
    assert_eq!(w.relay_dir().load_config().unwrap(), Some(cfg()));
    let cli: Vec<String> = w
        .net
        .calls_since(mark)
        .into_iter()
        .filter(|c| c.1 == "cli")
        .map(|c| c.0)
        .collect();
    assert_eq!(cli.len(), 1, "{cli:?}");
    assert!(cli[0].starts_with("POST"));
}

#[test]
fn a_removed_terminal_stops_routing_and_hides_its_rooms_without_more_calls() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    settle(&w);
    dev.poll().unwrap();
    assert!(dev.handles(&room(R1)));
    assert_eq!(dev.rooms().len(), 1);
    w.owner.revoke(&terminal_id(&dev)).unwrap();
    assert!(dev.poll().is_err());
    assert!(dev.removed());
    assert!(!dev.handles(&room(R1)));
    assert!(dev.rooms().is_empty());
    let kept = vec![RoomInfo {
        room: RoomRef::manager(R1),
        title: "Box bb".into(),
        agent: None,
        state: None,
        context_pct: None,
        context_tokens: None,
        model: None,
        last: None,
        unrestricted: false,
        mention: None,
    }];
    assert_eq!(dev.merge(kept.clone()), kept);
    let mark = w.net.mark();
    assert_eq!(dev.poll().unwrap_err(), REMOVED_TEXT);
    assert!(w.net.calls_since(mark).is_empty());
}

#[test]
fn a_send_that_meets_a_401_marks_the_terminal_removed() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    settle(&w);
    dev.poll().unwrap();
    w.owner.revoke(&terminal_id(&dev)).unwrap();
    assert!(dev.send(&room(R1), "late").is_err());
    assert!(dev.removed());
}

#[test]
fn the_unconfirmed_note_is_added_only_after_a_send_that_succeeded() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    settle(&w);
    dev.poll().unwrap();
    dev.send_with(&room(R1), "hi", &[]).unwrap();
    assert_eq!(dev.receipt_notes(), [unconfirmed_note(R1)]);
}

#[test]
fn a_receipt_after_an_attach_restart_still_resolves_through_the_persisted_sent_log() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    settle(&w);
    dev.poll().unwrap();
    let m = dev.send(&room(R1), "hello").unwrap();
    w.tick();
    let me = terminal_id(&dev);
    drop(dev);
    let again = w.cli_device();
    w.runners.borrow()[0]
        .engine
        .send_receipt(&me, &m.id, ReceiptState::Failed, Some("session is busy"))
        .unwrap();
    again.poll().unwrap();
    assert_eq!(
        again.receipt_notes(),
        ["remote send failed: session is busy".to_owned()]
    );
}

#[test]
fn a_removed_terminal_refuses_remote_session_rooms_with_the_re_pair_hint_and_leaves_others_alone() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    settle(&w);
    dev.poll().unwrap();
    assert_eq!(dev.refusal(&room(R1)), None);
    w.owner.revoke(&terminal_id(&dev)).unwrap();
    assert!(dev.poll().is_err());
    assert_eq!(dev.refusal(&room(R1)), Some(REMOVED_TEXT));
    assert_eq!(dev.refusal(&RoomRef::manager(R1)), None);
    assert_eq!(dev.refusal(&RoomRef::session(LOCAL, "s_1")), None);
}

#[test]
fn a_revoke_whose_row_delete_failed_still_marks_the_terminal_removed_on_its_next_poll() {
    let w = World::new();
    w.add_runner(R1, true, true);
    let mut dev = w.cli_device();
    w.pair(&mut dev).unwrap();
    settle(&w);
    dev.poll().unwrap();
    w.net.0.lock().unwrap().delete_error = true;
    assert!(w.owner.revoke(&terminal_id(&dev)).is_ok());
    w.net.0.lock().unwrap().delete_error = false;
    assert!(w.net.devices().iter().any(|d| d.0 == terminal_id(&dev)));
    let _ = dev.poll();
    assert!(dev.removed());
    assert_eq!(dev.refusal(&room(R1)), Some(REMOVED_TEXT));
    assert!(dev.rooms().is_empty());
}
