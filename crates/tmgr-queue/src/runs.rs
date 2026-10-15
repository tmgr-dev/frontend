use tmgr_core::error::Result;
use tmgr_ipc::api::{caps, RunBundle, TaskRef};

pub const PERMISSION_MODE: &str = "ask";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lookup {
    Found(String),
    NotFound,
    Unsupported,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunState {
    Running,
    Exited,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct StartRequest {
    pub start_key: String,
    pub permission_mode: &'static str,
    pub rule: String,
    pub trigger: String,
    pub profile: String,
    pub model: Option<String>,
    pub cwd: String,
    pub clone_of: String,
    pub name: String,
    pub task: TaskRef,
    pub bundle: RunBundle,
    pub bundle_sha256: String,
}

pub trait RunsClient {
    fn create(&self, req: &StartRequest) -> Result<String>;
    fn status(&self, run_id: &str) -> RunState;
    fn lookup(&self, start_key: &str) -> Lookup;
}

pub const REQUIRED_CAPS: [&str; 6] = [
    caps::RUN_BUNDLE_V1,
    caps::PERMISSION_MODE_V1,
    caps::START_KEY_V1,
    caps::RUN_STATUS_V1,
    caps::CLONE_DIR_V1,
    caps::AGENT_WORK_V1,
];

pub fn runner_supports_queue(runner_caps: &[String]) -> bool {
    REQUIRED_CAPS
        .iter()
        .all(|c| runner_caps.iter().any(|have| have == c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_queue_needs_every_cap_it_relies_on() {
        let all: Vec<String> = caps::ALL.iter().map(|c| c.to_string()).collect();
        assert!(runner_supports_queue(&all));
        for missing in REQUIRED_CAPS {
            let some: Vec<String> = all.iter().filter(|c| *c != missing).cloned().collect();
            assert!(!runner_supports_queue(&some), "{missing}");
        }
    }
}
