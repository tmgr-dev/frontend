use super::bundle::RunBundle;
use super::screen::{Line, Modes, Snapshot};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

fn default_cols() -> u16 {
    80
}

fn default_rows() -> u16 {
    24
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateKind {
    Starting,
    Working,
    Waiting,
    Blocked,
    Exited,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct State {
    pub kind: StateKind,
    pub since_ms: u64,
    pub detail: Option<String>,
    pub exit_code: Option<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskRef {
    pub workspace_id: i64,
    pub task_id: i64,
}

/// Params of `sessions.create` (raw argv, `client: "cli"` only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateReq {
    pub argv: Vec<String>,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<String>,
    #[serde(default = "default_cols")]
    pub cols: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_pass: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskRef>,
}

/// Params of `runs.create` (the runner resolves argv from the profile).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunCreateReq {
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskRef>,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default = "default_cols")]
    pub cols: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle: Option<RunBundle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd_pin: Option<CwdPin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clone_of: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust_cwd: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exe_pin: Option<ExePin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argv_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub agent_work: bool,
}

/// The leader to follower link a run was started for, and the link's generation at that time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Origin {
    pub link: String,
    pub generation: u64,
}

/// Params of `origin.revoke`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginRevokeParams {
    pub link: String,
    pub generation: u64,
}

/// Result of `origin.revoke`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginRevoked {
    pub killed: u32,
    #[serde(default)]
    pub unconfirmed: Vec<String>,
}

/// One element of the `candidates.list` result: a refreshed agent login waiting for `tmgr agents sync`. Never carries contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateInfo {
    pub profile: String,
    pub source: String,
    pub created_ms: u64,
    pub session: String,
}

/// Result of `runs.restore`: the new session and whether the agent was started with its resume arguments.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RestoredRun {
    pub session: SessionInfo,
    pub resumed: bool,
}

/// The agent executable a human approved: canonical path and the SHA-256 of its contents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExePin {
    pub path: String,
    pub sha256: String,
}

/// Identity (`stat` device and inode) of the folder a human approved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CwdPin {
    pub dev: u64,
    pub ino: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub name: String,
    pub argv: Vec<String>,
    pub cwd: String,
    pub profile: Option<String>,
    pub state: State,
    pub created_ms: u64,
    pub cols: u16,
    pub rows: u16,
    pub shim_pid: u32,
    pub child_pid: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
}

/// Params of `runs.lookup`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LookupParams {
    pub start_key: String,
}

/// Result of `runs.status`: cheap, no screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunStatus {
    pub id: String,
    pub alive: bool,
    pub state: StateKind,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub started_ms: u64,
    pub ended_ms: Option<u64>,
}

/// Params of `sessions.text`; `lines` is 1..=10000, default is the screen height.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextParams {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lines: Option<u32>,
}

/// Result of `sessions.text`: plain text, oldest line first, the last line is the bottom screen row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionText {
    pub id: String,
    pub rows: u16,
    pub cols: u16,
    pub lines: Vec<String>,
    pub scroll_max: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StatusInfo {
    pub pid: u32,
    pub version: String,
    pub proto: u32,
    pub uptime_s: u64,
    pub sessions: usize,
}

/// Payload of an `Event` push (a session state change).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EventMsg {
    pub session: String,
    pub state: State,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
}

/// Payload of a `Screen` push; `full` carries `snapshot`, otherwise `lines` are changed rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScreenPush {
    pub session: String,
    pub full: bool,
    #[serde(default)]
    pub snapshot: Option<Snapshot>,
    #[serde(default)]
    pub lines: Vec<(usize, Line)>,
    #[serde(default)]
    pub cursor: Option<(u16, u16)>,
    #[serde(default)]
    pub modes: Option<Modes>,
    #[serde(default)]
    pub scroll_offset: usize,
    #[serde(default)]
    pub scroll_max: usize,
}

/// Params of `sessions.remove`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdParams {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KillParams {
    pub id: String,
    pub signal: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendParams {
    pub id: String,
    pub text: String,
}

/// Params of `attach.open` and `attach.set`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttachParams {
    #[serde(default)]
    pub sessions: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sizes: BTreeMap<String, (u16, u16)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayoutSetParams {
    pub layout: Value,
}
