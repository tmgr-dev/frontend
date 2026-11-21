pub mod audit;
pub mod dispatch;
pub mod machines;
pub mod policy;
pub mod relay;
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
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Mutex;
    use tmgr_ipc::api::{Origin, SessionInfo, StateKind, TeamLink, TeamRole};

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

    pub struct Fake {
        pub list: Mutex<Vec<SessionInfo>>,
        pub sent: Mutex<Vec<(String, String)>>,
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
            self.list.lock().unwrap().clone()
        }
        fn text(&self, _: &str, lines: u32) -> Result<Vec<String>, String> {
            Ok((0..lines).map(|_| "x".repeat(self.line_len)).collect())
        }
        fn send(&self, id: &str, text: &str) -> Result<(), String> {
            self.sent.lock().unwrap().push((id.into(), text.into()));
            Ok(())
        }
        fn trusted_dir(&self, _: &str) -> Result<PathBuf, String> {
            panic!("trusted_dir must never be reached")
        }
        fn spawn(&self, _: SpawnSpec) -> Result<SessionInfo, String> {
            panic!("spawn must never be reached")
        }
        fn switch(&self, _: &str, _: &str) -> Result<crate::manager::SwitchResult, String> {
            panic!("switch must never be reached")
        }
    }
}
