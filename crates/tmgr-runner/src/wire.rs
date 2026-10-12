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
    let mut rnd = [0u8; 8];
    getrandom::fill(&mut rnd).map_err(std::io::Error::other)?;
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let tmp = path.with_file_name(format!(
        ".{name}.{}.tmp",
        rnd.iter().map(|b| format!("{b:02x}")).collect::<String>()
    ));
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW)
        .mode(0o600)
        .open(&tmp)?;
    let res = f.write_all(bytes).and_then(|()| {
        drop(f);
        std::fs::rename(&tmp, path)
    });
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn write_atomic_ignores_planted_symlink_and_is_private() {
        let t = tempfile::tempdir().unwrap();
        let victim = t.path().join("victim");
        std::fs::write(&victim, "keep").unwrap();
        let target = t.path().join("runner.pid");
        symlink(&victim, target.with_extension("tmp")).unwrap();
        write_atomic(&target, b"42").unwrap();
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "42");
        assert_eq!(
            std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let leftovers: Vec<_> = std::fs::read_dir(t.path())
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.starts_with(".runner.pid"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }
}
