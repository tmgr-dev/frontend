use std::path::PathBuf;
use tmgr_ipc::api::{
    CheckpointDiff, CheckpointItem, CheckpointRestored, CheckpointStatus, DiffAgainst,
};

pub const DEFAULT_KEEP: u32 = 50;

#[derive(Debug, Clone)]
pub struct Store {
    pub session: String,
    pub sdir: PathBuf,
    pub cwd: PathBuf,
    pub keep: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CpError {
    NotGit(String),
    Refused(String),
    Failed(String),
}

impl Store {
    pub fn snapshot(&self) -> Result<Option<CheckpointItem>, CpError> {
        Err(CpError::Failed("not implemented".into()))
    }

    pub fn list(&self) -> Vec<CheckpointItem> {
        Vec::new()
    }

    pub fn status(&self) -> CheckpointStatus {
        CheckpointStatus {
            available: false,
            note: None,
            count: 0,
        }
    }

    pub fn diff(&self, _n: u64, _against: DiffAgainst) -> Result<CheckpointDiff, CpError> {
        Err(CpError::Failed("not implemented".into()))
    }

    pub fn restore(&self, _n: u64) -> Result<CheckpointRestored, CpError> {
        Err(CpError::Failed("not implemented".into()))
    }

    pub fn remove_all(&self) {}

    pub fn transfer(&self, _to: &Store) -> Result<(), CpError> {
        Err(CpError::Failed("not implemented".into()))
    }
}
