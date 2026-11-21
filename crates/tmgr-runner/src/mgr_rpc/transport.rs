use super::dispatch::Dispatcher;
use super::policy::Policy;
use crate::manager::Host;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inbound {
    pub from: String,
    pub payload: Vec<u8>,
}

pub trait Transport {
    fn recv(&self) -> Option<Inbound>;
    fn send(&self, to: &str, payload: Vec<u8>) -> Result<(), String>;
}

type Queues = Arc<Mutex<HashMap<String, VecDeque<Inbound>>>>;

#[derive(Default, Clone)]
pub struct MemoryNet(Queues);

pub struct MemoryEndpoint {
    id: String,
    net: Queues,
}

impl MemoryNet {
    pub fn new() -> MemoryNet {
        MemoryNet::default()
    }

    pub fn endpoint(&self, machine: &str) -> MemoryEndpoint {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(machine.into())
            .or_default();
        MemoryEndpoint {
            id: machine.into(),
            net: self.0.clone(),
        }
    }
}

impl Transport for MemoryEndpoint {
    fn recv(&self) -> Option<Inbound> {
        self.net
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get_mut(&self.id)?
            .pop_front()
    }

    fn send(&self, to: &str, payload: Vec<u8>) -> Result<(), String> {
        let mut q = self.net.lock().unwrap_or_else(|e| e.into_inner());
        let inbox = q.get_mut(to).ok_or("unknown peer")?;
        inbox.push_back(Inbound {
            from: self.id.clone(),
            payload,
        });
        Ok(())
    }
}

pub fn serve_one(
    transport: &dyn Transport,
    dispatcher: &Dispatcher,
    policy: &Policy,
    host: &dyn Host,
) -> bool {
    let Some(inbound) = transport.recv() else {
        return false;
    };
    if let Some(res) = dispatcher.handle(policy, host, &inbound.from, &inbound.payload) {
        if let Ok(bytes) = serde_json::to_vec(&res) {
            let _ = transport.send(&inbound.from, bytes);
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::super::dispatch::MemoryAudit;
    use super::super::testkit::*;
    use super::*;
    use crate::manager::{MemoryChat, State};
    use serde_json::json;
    use tmgr_ipc::mgr_rpc::RpcRes;

    fn dispatcher() -> Dispatcher {
        Dispatcher::new(
            ME,
            State::new(Box::new(MemoryChat::default())),
            Arc::new(MemoryAudit::default()),
        )
    }

    #[test]
    fn two_machines_round_trip_over_the_memory_net() {
        let net = MemoryNet::new();
        let top = net.endpoint(PEER);
        let sub = net.endpoint(ME);
        let policy = Policy::parse(&format!("accept_from = [\"{PEER}\"]")).unwrap();
        let d = dispatcher();
        let host = Fake::new();
        let req = json!({"v":1,"rpc":"net-1","op":"list","args":{},"chain":[],"ts_ms":T0});
        top.send(ME, serde_json::to_vec(&req).unwrap()).unwrap();
        assert!(serve_one(&sub, &d, &policy, &host));
        assert!(!serve_one(&sub, &d, &policy, &host));
        let back = top.recv().unwrap();
        assert_eq!(back.from, ME);
        let res: RpcRes = serde_json::from_slice(&back.payload).unwrap();
        assert!(res.ok);
        assert_eq!(res.rpc, "net-1");
        assert!(top.recv().is_none());
    }

    #[test]
    fn a_request_from_a_stranger_gets_no_answer() {
        let net = MemoryNet::new();
        let stranger = net.endpoint("mx_00000000000000cc");
        let sub = net.endpoint(ME);
        let d = dispatcher();
        let host = Fake::new();
        let req = json!({"v":1,"rpc":"net-2","op":"list","args":{},"chain":[],"ts_ms":T0});
        stranger
            .send(ME, serde_json::to_vec(&req).unwrap())
            .unwrap();
        serve_one(&sub, &d, &Policy::deny_all(), &host);
        assert!(stranger.recv().is_none());
    }

    #[test]
    fn an_unreadable_request_gets_no_answer() {
        let net = MemoryNet::new();
        let top = net.endpoint(PEER);
        let sub = net.endpoint(ME);
        let policy = Policy::parse(&format!("accept_from = [\"{PEER}\"]")).unwrap();
        top.send(ME, b"garbage".to_vec()).unwrap();
        assert!(serve_one(&sub, &dispatcher(), &policy, &Fake::new()));
        assert!(top.recv().is_none());
    }

    #[test]
    fn sending_to_an_unknown_peer_fails() {
        let net = MemoryNet::new();
        assert!(net
            .endpoint(ME)
            .send("mx_ffffffffffffffff", vec![])
            .is_err());
    }
}
