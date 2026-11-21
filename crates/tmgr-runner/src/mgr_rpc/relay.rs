use super::dispatch::{Audit, AuditRecord, Dispatcher};
use super::machines::{Machines, MACHINES_FILE};
use super::policy::Policy;
use super::transport::{serve_one, Inbound, Transport};
use crate::manager::Host;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::os::unix::fs::MetadataExt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tmgr_chat_relay::engine::RpcSink;
use tmgr_chat_relay::wire::random_msg_id;
use tmgr_ipc::mgr_rpc::{ErrorCode, Op, RpcReq, RpcRes, KIND_REQ, KIND_RES, MGR_RPC_V};

const INBOX_CAP: usize = 64;

pub type RpcSend = Arc<dyn Fn(&str, &str, &str, &Value) -> Result<(), String> + Send + Sync>;
pub type Clock = Arc<dyn Fn() -> u64 + Send + Sync>;

#[derive(Debug, Clone)]
pub struct RpcItem {
    pub device: String,
    pub fp: String,
    pub msg_id: String,
    pub payload: Value,
}

pub struct RpcInbox {
    req_tx: SyncSender<RpcItem>,
    res_tx: SyncSender<RpcItem>,
    req_rx: Mutex<Receiver<RpcItem>>,
    res_rx: Mutex<Receiver<RpcItem>>,
    dropped: AtomicU64,
}

impl Default for RpcInbox {
    fn default() -> Self {
        RpcInbox::new()
    }
}

impl RpcInbox {
    pub fn new() -> RpcInbox {
        let (req_tx, req_rx) = sync_channel(INBOX_CAP);
        let (res_tx, res_rx) = sync_channel(INBOX_CAP);
        RpcInbox {
            req_tx,
            res_tx,
            req_rx: Mutex::new(req_rx),
            res_rx: Mutex::new(res_rx),
            dropped: AtomicU64::new(0),
        }
    }

    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::SeqCst)
    }

    fn try_req(&self) -> Option<RpcItem> {
        self.req_rx
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .try_recv()
            .ok()
    }

    fn recv_res(&self, wait: Duration) -> Option<RpcItem> {
        self.res_rx
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .recv_timeout(wait)
            .ok()
    }
}

impl RpcSink for RpcInbox {
    fn deliver_rpc(&self, from_device: &str, from_fp: &str, kind: &str, msg_id: &str, payload: Value) {
        let tx = match kind {
            KIND_REQ => &self.req_tx,
            KIND_RES => &self.res_tx,
            _ => return,
        };
        let item = RpcItem {
            device: from_device.into(),
            fp: from_fp.into(),
            msg_id: msg_id.into(),
            payload,
        };
        if tx.try_send(item).is_err() {
            self.dropped.fetch_add(1, Ordering::SeqCst);
        }
    }
}

type Stamp = Option<(i64, i64, u64, u64)>;

pub struct MachinesCache {
    dir: PathBuf,
    cached: Mutex<Option<(Stamp, Arc<Machines>)>>,
}

impl MachinesCache {
    pub fn new(runner_dir: &Path) -> MachinesCache {
        MachinesCache {
            dir: runner_dir.into(),
            cached: Mutex::new(None),
        }
    }

    fn stamp(&self) -> Stamp {
        std::fs::symlink_metadata(self.dir.join(MACHINES_FILE))
            .ok()
            .map(|m| (m.ctime(), m.ctime_nsec(), m.len(), m.ino()))
    }

    pub fn current(&self) -> Arc<Machines> {
        let stamp = self.stamp();
        let mut g = self.cached.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((s, m)) = g.as_ref() {
            if *s == stamp {
                return m.clone();
            }
        }
        let m = Arc::new(if stamp.is_some() {
            Machines::load(&self.dir)
        } else {
            Machines::default()
        });
        *g = Some((stamp, m.clone()));
        m
    }
}

pub struct RelayTransport {
    inbox: Arc<RpcInbox>,
    machines: Arc<MachinesCache>,
    audit: Arc<dyn Audit>,
    now: Clock,
    send: RpcSend,
}

impl RelayTransport {
    pub fn new(
        inbox: Arc<RpcInbox>,
        machines: Arc<MachinesCache>,
        audit: Arc<dyn Audit>,
        now: Clock,
        send: RpcSend,
    ) -> RelayTransport {
        RelayTransport {
            inbox,
            machines,
            audit,
            now,
            send,
        }
    }
}

impl Transport for RelayTransport {
    fn recv(&self) -> Option<Inbound> {
        loop {
            let item = self.inbox.try_req()?;
            let known = self.machines.current();
            let Some(machine) = known.machine_for(&item.device, &item.fp) else {
                self.audit.record(AuditRecord {
                    ts_ms: (self.now)(),
                    from: item.device.chars().take(64).collect(),
                    rpc: None,
                    op: None,
                    session: None,
                    outcome: ErrorCode::NotAllowed.as_str().into(),
                });
                continue;
            };
            let Ok(payload) = serde_json::to_vec(&item.payload) else {
                continue;
            };
            return Some(Inbound {
                from: machine.into(),
                payload,
            });
        }
    }

    fn send(&self, to: &str, payload: Vec<u8>) -> Result<(), String> {
        let known = self.machines.current();
        let (device, _) = known.device_for(to).ok_or("unknown machine")?;
        let value: Value = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
        let id = random_msg_id(KIND_RES).map_err(|_| "random source failed".to_string())?;
        (self.send)(device, KIND_RES, &id, &value)
    }
}

pub struct RpcClient {
    inbox: Arc<RpcInbox>,
    machines: Arc<MachinesCache>,
    send: RpcSend,
    now: Clock,
    gate: Mutex<()>,
}

impl RpcClient {
    pub fn new(
        inbox: Arc<RpcInbox>,
        machines: Arc<MachinesCache>,
        send: RpcSend,
        now: Clock,
    ) -> RpcClient {
        RpcClient {
            inbox,
            machines,
            send,
            now,
            gate: Mutex::new(()),
        }
    }

    pub fn call(
        &self,
        machine: &str,
        op: Op,
        args: Value,
        timeout: Duration,
    ) -> Result<RpcRes, String> {
        let _one_at_a_time = self.gate.lock().unwrap_or_else(|e| e.into_inner());
        let deadline = Instant::now() + timeout;
        let known = self.machines.current();
        let (device, fp) = known.device_for(machine).ok_or("unknown machine")?;
        let rpc = fresh_id("c")?;
        let req = RpcReq {
            v: MGR_RPC_V,
            rpc: rpc.clone(),
            op,
            args,
            chain: Vec::new(),
            ts_ms: (self.now)(),
        };
        let payload = serde_json::to_value(&req).map_err(|e| e.to_string())?;
        let msg_id = random_msg_id(KIND_REQ).map_err(|_| "random source failed".to_string())?;
        (self.send)(device, KIND_REQ, &msg_id, &payload)?;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let item = self
                .inbox
                .recv_res(left)
                .ok_or_else(|| "timed out waiting for the response".to_string())?;
            if item.device != device || item.fp != fp {
                continue;
            }
            match serde_json::from_value::<RpcRes>(item.payload) {
                Ok(res) if res.rpc == rpc => return Ok(res),
                _ => continue,
            }
        }
    }
}

fn fresh_id(prefix: &str) -> Result<String, String> {
    let mut r = [0u8; 16];
    getrandom::fill(&mut r).map_err(|_| "random source failed".to_string())?;
    let hex: String = r.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!("{prefix}_{hex}"))
}

pub fn serve_tick(
    transport: &dyn Transport,
    dispatcher: &Dispatcher,
    runner_dir: &Path,
    host: &dyn Host,
) -> bool {
    let policy = Policy::load(runner_dir);
    serve_one(transport, dispatcher, &policy, host)
}

#[cfg(test)]
mod tests {
    use super::super::dispatch::MemoryAudit;
    use super::super::policy::POLICY_FILE;
    use super::super::testkit::*;
    use super::*;
    use crate::manager::{MemoryChat, State};
    use serde_json::json;
    use std::os::unix::fs::PermissionsExt;

    const DEV_A: &str = "11111111-2222-3333-4444-555555555555";
    const DEV_B: &str = "66666666-7777-8888-9999-aaaaaaaaaaaa";
    const DEV_X: &str = "99999999-7777-8888-9999-aaaaaaaaaaaa";
    const FP_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const FP_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const OTHER: &str = "mx_00000000000000cc";

    type Sent = Arc<Mutex<Vec<(String, String, String, Value)>>>;

    struct Rig {
        dir: tempfile::TempDir,
        inbox: Arc<RpcInbox>,
        audit: Arc<MemoryAudit>,
        sent: Sent,
        transport: RelayTransport,
        dispatcher: Dispatcher,
        host: Fake,
    }

    fn private(dir: &Path, name: &str, text: &str) {
        let p = dir.join(name);
        std::fs::write(&p, text).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600)).unwrap();
    }

    fn recorder(sent: &Sent) -> RpcSend {
        let sent = sent.clone();
        Arc::new(move |d, k, i, p| {
            sent.lock()
                .unwrap()
                .push((d.into(), k.into(), i.into(), p.clone()));
            Ok(())
        })
    }

    fn rig(policy: Option<&str>) -> Rig {
        let dir = tempfile::Builder::new()
            .prefix("mr")
            .tempdir_in("/tmp")
            .unwrap();
        private(
            dir.path(),
            MACHINES_FILE,
            &json!({"machines": [
                {"id": PEER, "label": "top", "device_id": DEV_A, "fp": FP_A},
                {"id": OTHER, "label": "other", "device_id": DEV_B, "fp": FP_B},
            ]})
            .to_string(),
        );
        if let Some(p) = policy {
            private(dir.path(), POLICY_FILE, p);
        }
        let inbox = Arc::new(RpcInbox::new());
        let audit = Arc::new(MemoryAudit::default());
        let sent: Sent = Arc::default();
        let transport = RelayTransport::new(
            inbox.clone(),
            Arc::new(MachinesCache::new(dir.path())),
            audit.clone(),
            Arc::new(|| T0),
            recorder(&sent),
        );
        let dispatcher = Dispatcher::new(
            ME,
            State::new(Box::new(MemoryChat::default())),
            audit.clone(),
        );
        Rig {
            dir,
            inbox,
            audit,
            sent,
            transport,
            dispatcher,
            host: Fake::new(),
        }
    }

    fn list_req(rpc: &str) -> Value {
        json!({"v":1,"rpc":rpc,"op":"list","args":{},"chain":[],"ts_ms":T0})
    }

    impl Rig {
        fn deliver(&self, device: &str, fp: &str, kind: &str, payload: Value) {
            self.inbox.deliver_rpc(device, fp, kind, "g_1", payload);
        }

        fn tick(&self) -> bool {
            serve_tick(
                &self.transport,
                &self.dispatcher,
                self.dir.path(),
                &self.host,
            )
        }
    }

    #[test]
    fn an_unknown_device_gets_no_reply_and_one_audit_line() {
        let r = rig(Some(&format!("accept_from = [\"{PEER}\"]")));
        r.deliver(DEV_X, FP_A, KIND_REQ, list_req("r1"));
        assert!(r.transport.recv().is_none());
        assert!(r.sent.lock().unwrap().is_empty());
        let log = r.audit.records();
        assert_eq!(log.len(), 1);
        assert_eq!(log[0].outcome, "not_allowed");
        assert_eq!((log[0].rpc.clone(), log[0].op), (None, None));
        assert_eq!(log[0].from, DEV_X);
    }

    #[test]
    fn a_fingerprint_mismatch_is_dropped_and_audited() {
        let r = rig(Some(&format!("accept_from = [\"{PEER}\"]")));
        r.deliver(DEV_A, FP_B, KIND_REQ, list_req("r1"));
        assert!(!r.tick());
        assert!(r.sent.lock().unwrap().is_empty());
        assert_eq!(r.audit.records().len(), 1);
    }

    #[test]
    fn a_pinned_device_is_served_and_the_reply_goes_only_to_it() {
        let r = rig(Some(&format!("accept_from = [\"{PEER}\"]")));
        r.deliver(DEV_A, FP_A, KIND_REQ, list_req("r1"));
        assert!(r.tick());
        let sent = r.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let (device, kind, id, payload) = &sent[0];
        assert_eq!(device, DEV_A);
        assert_eq!(kind, KIND_RES);
        assert!(id.starts_with("h_"));
        assert_eq!(payload["rpc"], "r1");
        assert_eq!(payload["ok"], true);
    }

    #[test]
    fn a_known_machine_outside_accept_from_gets_no_reply() {
        let r = rig(Some(&format!("accept_from = [\"{PEER}\"]")));
        r.deliver(DEV_B, FP_B, KIND_REQ, list_req("r1"));
        assert!(r.tick());
        assert!(r.sent.lock().unwrap().is_empty());
        assert_eq!(r.audit.records()[0].outcome, "not_allowed");
    }

    #[test]
    fn with_no_policy_file_everything_is_dropped() {
        let r = rig(None);
        for op in ["list", "read", "send", "ask"] {
            let req = json!({"v":1,"rpc":format!("r-{op}"),"op":op,"args":{},"chain":[],"ts_ms":T0});
            r.deliver(DEV_A, FP_A, KIND_REQ, req);
            assert!(r.tick());
        }
        assert!(r.sent.lock().unwrap().is_empty());
        assert!(r.host.sent.lock().unwrap().is_empty());
        assert!(r
            .audit
            .records()
            .iter()
            .all(|l| l.outcome == "not_allowed"));
    }

    #[test]
    fn requests_and_responses_are_routed_to_their_own_side() {
        let r = rig(Some(&format!("accept_from = [\"{PEER}\"]")));
        r.deliver(DEV_A, FP_A, KIND_RES, json!({"v":1,"rpc":"x","ok":true}));
        assert!(r.transport.recv().is_none());
        assert!(r.inbox.recv_res(Duration::from_millis(10)).is_some());
    }

    #[test]
    fn a_full_inbox_drops_and_counts() {
        let r = rig(None);
        for i in 0..(INBOX_CAP + 6) {
            r.deliver(DEV_A, FP_A, KIND_REQ, list_req(&format!("r{i}")));
        }
        assert_eq!(r.inbox.dropped(), 6);
        let mut n = 0;
        while r.inbox.try_req().is_some() {
            n += 1;
        }
        assert_eq!(n, INBOX_CAP);
    }

    #[test]
    fn sending_to_an_unknown_machine_fails() {
        let r = rig(None);
        assert!(r.transport.send("mx_ffffffffffffffff", b"{}".to_vec()).is_err());
        assert!(r.sent.lock().unwrap().is_empty());
    }

    struct ClientRig {
        client: RpcClient,
        sent: Sent,
        _dir: tempfile::TempDir,
    }

    fn client_rig(script: impl Fn(&RpcInbox, &Value) + Send + Sync + 'static) -> ClientRig {
        let r = rig(None);
        let sent: Sent = Arc::default();
        let log = sent.clone();
        let inbox = r.inbox.clone();
        let send: RpcSend = Arc::new(move |d, k, i, p| {
            log.lock()
                .unwrap()
                .push((d.into(), k.into(), i.into(), p.clone()));
            script(&inbox, p);
            Ok(())
        });
        let client = RpcClient::new(
            r.inbox.clone(),
            Arc::new(MachinesCache::new(r.dir.path())),
            send,
            Arc::new(|| T0),
        );
        ClientRig {
            client,
            sent,
            _dir: r.dir,
        }
    }

    fn answer(rpc: &str) -> Value {
        serde_json::to_value(RpcRes::ok(rpc, json!({"n": 1}))).unwrap()
    }

    #[test]
    fn the_client_returns_the_matching_response_and_ignores_the_rest() {
        let c = client_rig(|inbox, req| {
            let rpc = req["rpc"].as_str().unwrap();
            inbox.deliver_rpc(DEV_B, FP_B, KIND_RES, "h_1", answer(rpc));
            inbox.deliver_rpc(DEV_A, FP_A, KIND_RES, "h_2", answer("someone-else"));
            inbox.deliver_rpc(DEV_A, FP_B, KIND_RES, "h_3", answer(rpc));
            inbox.deliver_rpc(DEV_A, FP_A, KIND_RES, "h_4", answer(rpc));
        });
        let res = c
            .client
            .call(PEER, Op::List, json!({}), Duration::from_secs(2))
            .unwrap();
        assert!(res.ok);
        let sent = c.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        let (device, kind, id, payload) = &sent[0];
        assert_eq!((device.as_str(), kind.as_str()), (DEV_A, KIND_REQ));
        assert!(id.starts_with("g_"));
        let req: RpcReq = serde_json::from_value(payload.clone()).unwrap();
        assert_eq!((req.op, req.ts_ms, req.chain.len()), (Op::List, T0, 0));
        assert_eq!(res.rpc, req.rpc);
    }

    #[test]
    fn the_client_times_out_when_only_wrong_answers_arrive() {
        let c = client_rig(|inbox, req| {
            let rpc = req["rpc"].as_str().unwrap();
            inbox.deliver_rpc(DEV_B, FP_B, KIND_RES, "h_1", answer(rpc));
            inbox.deliver_rpc(DEV_A, FP_A, KIND_RES, "h_2", answer("nope"));
        });
        let err = c
            .client
            .call(PEER, Op::List, json!({}), Duration::from_millis(100))
            .unwrap_err();
        assert!(err.contains("timed out"), "{err}");
    }

    #[test]
    fn the_client_refuses_an_unknown_machine_without_sending() {
        let c = client_rig(|_, _| {});
        let err = c
            .client
            .call("mx_ffffffffffffffff", Op::List, json!({}), Duration::from_millis(10))
            .unwrap_err();
        assert!(err.contains("unknown machine"));
        assert!(c.sent.lock().unwrap().is_empty());
    }
}
