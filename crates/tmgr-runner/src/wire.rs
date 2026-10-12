use crate::screen::{Line, Modes, Snapshot};
use crate::t0::State;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

pub const SCROLLBACK: usize = 5000;
pub const MAX_COLS: u16 = 500;
pub const MAX_ROWS: u16 = 200;
pub const MAX_INPUT: usize = 64 << 10;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

pub fn clamp_size(cols: u16, rows: u16) -> (u16, u16) {
    (cols.clamp(1, MAX_COLS), rows.clamp(1, MAX_ROWS))
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)?;
    f.write_all(bytes)?;
    drop(f);
    std::fs::rename(&tmp, path)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LaunchFile {
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
}

fn default_cols() -> u16 {
    80
}

fn default_rows() -> u16 {
    24
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusInfo {
    pub pid: u32,
    pub version: String,
    pub proto: u32,
    pub uptime_s: u64,
    pub sessions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventMsg {
    pub session: String,
    pub state: State,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
