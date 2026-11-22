pub mod audit;
pub mod dispatch;
pub mod files;
pub mod machines;
pub mod pairing;
pub mod policy;
pub mod relay;
pub mod top;
pub mod transport;

pub use audit::FileAudit;
pub use dispatch::{Audit, AuditRecord, Dispatcher, MemoryAudit};
pub use machines::Machines;
pub use policy::Policy;
pub use transport::{serve_one, Inbound, MemoryNet, Transport};

#[cfg(test)]
pub(crate) mod testkit {
    use crate::manager::{Host, SpawnSpec};
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;
    use tmgr_ipc::api::{Origin, SessionInfo, StateKind, TeamLink, TeamRole};
    use tmgr_ipc::manager::SignedSpawn;

    pub const ME: &str = "mx_00000000000000aa";
    pub const PEER: &str = "mx_00000000000000bb";
    pub const T0: u64 = 10_000_000;

    pub const MANAGER: &str = "s_aaaaaaaaaa";
    pub const WORKER: &str = "s_bbbbbbbbbb";
    pub const LINKED: &str = "s_cccccccccc";
    pub const TEAMMATE: &str = "s_dddddddddd";
    pub const EXITED: &str = "s_eeeeeeeeee";
    pub const BYPASS: &str = "s_ffffffffff";
    pub const OTHER_MANAGER: &str = "s_gggggggggg";
    pub const UNSANDBOXED: &str = "s_hhhhhhhhhh";

    pub type Proposal = (String, Option<String>, Option<String>);

    pub const NEW: &str = "s_nnnnnnnnnn";

    pub struct Fake {
        pub list: Mutex<Vec<SessionInfo>>,
        pub sent: Mutex<Vec<(String, String)>>,
        pub spawned: Mutex<Vec<SpawnSpec>>,
        pub switched: Mutex<Vec<String>>,
        pub verified: Mutex<Vec<(String, Option<String>)>>,
        pub done: Mutex<Vec<(String, String)>>,
        pub proposed: Mutex<Vec<Proposal>>,
        pub approved: Mutex<HashMap<String, (Option<String>, SignedSpawn)>>,
        pub spawned_for: Mutex<Vec<(String, String)>>,
        pub failed: Mutex<Vec<(String, String)>>,
        pub explode: std::sync::atomic::AtomicBool,
        pub unrestricted_manager: std::sync::atomic::AtomicBool,
        pub now: AtomicU64,
        pub line_len: usize,
    }

    pub fn info(id: &str) -> SessionInfo {
        serde_json::from_value(json!({
            "id": id, "name": id, "argv": [], "cwd": "/w", "profile": "claude-code",
            "state": {"kind": "waiting", "since_ms": 1, "detail": null, "exit_code": null},
            "created_ms": 5, "cols": 80, "rows": 24, "shim_pid": 1, "child_pid": 2
        }))
        .unwrap()
    }

    impl Fake {
        pub fn new() -> Fake {
            let mut manager = info(MANAGER);
            manager.manager = true;
            let mut linked = info(LINKED);
            linked.origin = Some(Origin {
                link: "l".into(),
                generation: 1,
            });
            let mut team = info(TEAMMATE);
            team.team = Some(TeamLink {
                role: TeamRole::Lead,
                of: "t".into(),
            });
            let mut gone = info(EXITED);
            gone.state.kind = StateKind::Exited;
            let mut bypass = info(BYPASS);
            bypass.permission_mode = Some("bypass".into());
            let mut other = info(OTHER_MANAGER);
            other.manager = true;
            let mut raw = info(UNSANDBOXED);
            raw.no_sandbox = true;
            Fake {
                list: Mutex::new(vec![
                    manager,
                    info(WORKER),
                    linked,
                    team,
                    gone,
                    bypass,
                    other,
                    raw,
                ]),
                sent: Mutex::default(),
                spawned: Mutex::default(),
                switched: Mutex::default(),
                verified: Mutex::default(),
                done: Mutex::default(),
                proposed: Mutex::default(),
                approved: Mutex::default(),
                spawned_for: Mutex::default(),
                failed: Mutex::default(),
                explode: std::sync::atomic::AtomicBool::new(false),
                unrestricted_manager: std::sync::atomic::AtomicBool::new(false),
                now: AtomicU64::new(T0),
                line_len: 5,
            }
        }

        pub fn set_now(&self, ms: u64) {
            self.now.store(ms, Ordering::SeqCst);
        }
    }

    impl Host for Fake {
        fn me(&self) -> &str {
            MANAGER
        }
        fn machine(&self) -> String {
            ME.into()
        }
        fn now_ms(&self) -> u64 {
            self.now.load(Ordering::SeqCst)
        }
        fn sessions(&self) -> Vec<SessionInfo> {
            assert!(!self.explode.load(Ordering::SeqCst), "boom");
            self.list.lock().unwrap().clone()
        }
        fn text(&self, _: &str, lines: u32) -> Result<Vec<String>, String> {
            Ok((0..lines).map(|_| "x".repeat(self.line_len)).collect())
        }
        fn send(&self, id: &str, text: &str) -> Result<(), String> {
            self.sent.lock().unwrap().push((id.into(), text.into()));
            Ok(())
        }
        fn local_manager_unrestricted(&self) -> bool {
            self.unrestricted_manager.load(Ordering::SeqCst)
        }
        fn trusted_dir(&self, cwd: &str) -> Result<crate::manager::TrustedDir, String> {
            if cwd == "/tmp/ok" {
                Ok(crate::manager::TrustedDir {
                    path: "/tmp/ok".into(),
                    pin: tmgr_ipc::api::CwdPin { dev: 1, ino: 2 },
                })
            } else {
                Err("cwd is not a trusted folder".into())
            }
        }
        fn precheck_spawn(&self, _: &SpawnSpec) -> Result<(), String> {
            Ok(())
        }
        fn spawn(&self, spec: SpawnSpec) -> Result<SessionInfo, String> {
            self.spawned.lock().unwrap().push(spec);
            Ok(info(NEW))
        }
        fn switch(&self, id: &str, to: &str) -> Result<crate::manager::SwitchResult, String> {
            self.switched.lock().unwrap().push(format!("{id}:{to}"));
            Ok(crate::manager::SwitchResult {
                session: info(NEW),
                packet_id: "h_1".into(),
                warnings: Vec::new(),
            })
        }
        fn verify_spawn(
            &self,
            approval: &crate::manager::Approval,
            _: &tmgr_spawn_auth::SpawnProposal,
            proposer: Option<&str>,
        ) -> Result<crate::manager::Verdict, String> {
            self.verified
                .lock()
                .unwrap()
                .push((approval.card_id().to_owned(), proposer.map(String::from)));
            Ok(crate::manager::Verdict::Verified)
        }
        fn spawn_done(&self, card: &str, session: &str) {
            self.done
                .lock()
                .unwrap()
                .push((card.into(), session.into()));
        }
        fn propose_spawn(
            &self,
            p: &tmgr_spawn_auth::SpawnProposal,
            proposer: Option<&str>,
            prompt: Option<&str>,
        ) -> Result<crate::manager::Proposed, String> {
            let card = format!("card_{}", self.proposed.lock().unwrap().len() + 1);
            self.proposed.lock().unwrap().push((
                card.clone(),
                proposer.map(String::from),
                prompt.map(String::from),
            ));
            Ok(crate::manager::Proposed {
                card_id: card,
                exp_ms: 9,
                req_sha256: p.req_sha256(),
            })
        }
        fn spawn_failed(&self, card: &str, reason: &str) {
            self.failed
                .lock()
                .unwrap()
                .push((card.into(), reason.into()));
        }
        fn approved_spawn(
            &self,
            card: &str,
            proposer: Option<&str>,
        ) -> Result<crate::manager::Approval, crate::manager::CardMiss> {
            let Some((by, signed)) = self.approved.lock().unwrap().get(card).cloned() else {
                return Err(crate::manager::CardMiss::Pending);
            };
            if proposer.is_some() && proposer == by.as_deref() {
                Ok(crate::manager::Approval::Signed(signed))
            } else {
                Err(crate::manager::CardMiss::Gone)
            }
        }
        fn proposer(&self) -> Option<String> {
            Some(MANAGER.into())
        }
        fn remote_caller(&self) -> Option<&str> {
            None
        }
        fn spawned_for(&self, session: &str, from: &str) -> bool {
            self.spawned_for
                .lock()
                .unwrap()
                .contains(&(session.to_string(), format!("remote:{from}")))
        }
    }
}
