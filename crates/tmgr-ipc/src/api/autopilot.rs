use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Params of `autopilot.start`: only the id of the saved settings; the runner builds the command itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutopilotStartParams {
    pub settings_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutopilotStarted {
    pub pid: u32,
}

/// Result of `autopilot.status`: `status` is the content of `status.json`, or `null` when there is none.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutopilotStatus {
    pub running: bool,
    #[serde(default)]
    pub pid: Option<u32>,
    #[serde(default)]
    pub status: Value,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutopilotCommand {
    Pause,
    Resume,
    Stop,
    Suspend,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutopilotControlParams {
    pub command: AutopilotCommand,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutopilotControlled {
    pub seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AutopilotLogParams {
    pub tail: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutopilotLog {
    pub lines: Vec<Value>,
}
