use crate::paths::{lock_file, Paths};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tmgr_core::config::write_atomic;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::{RunBundle, TaskRef};

pub const PERMISSION_MODE: &str = "ask";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunState {
    Running,
    Exited,
    Unknown,
}

#[derive(Clone, Debug)]
pub struct StartRequest {
    pub permission_mode: &'static str,
    pub rule: String,
    pub trigger: String,
    pub profile: String,
    pub model: Option<String>,
    pub cwd: String,
    pub task: TaskRef,
    pub bundle: RunBundle,
    pub bundle_sha256: String,
}

pub trait RunsClient {
    fn create(&self, req: &StartRequest) -> Result<String>;
    fn status(&self, run_id: &str) -> RunState;
    fn is_mock(&self) -> bool;
}

pub struct RunnerRuns;

impl RunsClient for RunnerRuns {
    fn create(&self, _req: &StartRequest) -> Result<String> {
        Err(Error::new(
            Kind::Offline,
            "queue runs are not available until the runner supports them",
        ))
    }

    fn status(&self, _run_id: &str) -> RunState {
        RunState::Unknown
    }

    fn is_mock(&self) -> bool {
        false
    }
}

pub fn runner_supports_queue() -> bool {
    false
}

#[derive(Debug, Deserialize, Serialize)]
pub struct MockRun {
    pub id: String,
    pub rule: String,
    pub task_id: i64,
    pub bundle_sha256: String,
    #[serde(default)]
    pub exited: bool,
}

pub struct MockRuns<'a>(pub &'a Paths);

impl MockRuns<'_> {
    pub fn list(&self) -> Result<Vec<MockRun>> {
        match std::fs::read_to_string(self.0.mock_runs()) {
            Ok(t) => serde_json::from_str(&t).map_err(|e| Error::internal(e.to_string())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }
}

impl MockRuns<'_> {
    pub fn finish(&self, id: &str) -> Result<()> {
        let _lock = lock_file(&self.0.dir, "queue-mock.lock")?;
        let mut runs = self.list()?;
        for r in runs.iter_mut().filter(|r| r.id == id) {
            r.exited = true;
        }
        write_atomic(
            &self.0.mock_runs(),
            json!(runs).to_string().as_bytes(),
            0o600,
        )
    }
}

impl RunsClient for MockRuns<'_> {
    fn status(&self, run_id: &str) -> RunState {
        match self.list() {
            Ok(runs) => match runs.iter().find(|r| r.id == run_id) {
                Some(r) if r.exited => RunState::Exited,
                Some(_) => RunState::Running,
                None => RunState::Unknown,
            },
            Err(_) => RunState::Unknown,
        }
    }

    fn create(&self, req: &StartRequest) -> Result<String> {
        let _lock = lock_file(&self.0.dir, "queue-mock.lock")?;
        let mut runs = self.list()?;
        let id = format!("mock-{}", runs.len() + 1);
        runs.push(MockRun {
            id: id.clone(),
            rule: req.rule.clone(),
            task_id: req.task.task_id,
            bundle_sha256: req.bundle_sha256.clone(),
            exited: false,
        });
        let value: Value = json!(runs);
        write_atomic(&self.0.mock_runs(), value.to_string().as_bytes(), 0o600)?;
        Ok(id)
    }

    fn is_mock(&self) -> bool {
        true
    }
}
