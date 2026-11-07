use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointKind {
    Start,
    Turn,
    BeforeRestore,
    Restored,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointItem {
    pub n: u64,
    pub kind: CheckpointKind,
    pub turn: u64,
    pub files: u32,
    pub skipped: u32,
    pub at_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restored_from: Option<u64>,
}

/// `SessionInfo.checkpoints`; absent means the session has checkpoints off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointStatus {
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointEnableParams {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointList {
    pub status: CheckpointStatus,
    pub items: Vec<CheckpointItem>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffAgainst {
    #[default]
    Previous,
    Current,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointDiffParams {
    pub id: String,
    pub n: u64,
    #[serde(default)]
    pub against: DiffAgainst,
}

/// With `against: current`, `added` paths are removed by a restore and the others rewritten.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointDiff {
    pub n: u64,
    pub text: String,
    pub truncated: bool,
    pub added: u32,
    pub modified: u32,
    pub deleted: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointRestoreParams {
    pub id: String,
    pub n: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointRestored {
    pub n: u64,
    pub before_n: u64,
    pub after_n: u64,
    pub written: u32,
    pub deleted: u32,
    pub left_as_is: Vec<String>,
}
