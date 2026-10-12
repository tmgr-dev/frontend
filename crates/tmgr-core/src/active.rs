use crate::config::{ensure_private_dir, write_atomic};
use crate::error::{Error, Result};
use crate::refs::parse_key;
use crate::text::{sanitize_line, truncate_chars};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub const ACTIVE_SCHEMA: &str = "tmgr.active.v1";
const MAX_FILE: u64 = 16 * 1024;
const MAX_TITLE: usize = 200;
const MAX_FIELD: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveTask {
    pub id: u64,
    pub key: String,
    pub title: String,
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActiveState {
    pub schema: String,
    pub task: ActiveTask,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub timer_started_at: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub repo: Option<String>,
}

impl ActiveState {
    pub fn new(task: ActiveTask) -> Self {
        Self {
            schema: ACTIVE_SCHEMA.to_string(),
            task,
            workspace: None,
            timer_started_at: None,
            branch: None,
            repo: None,
        }
    }

    pub fn started_epoch(&self) -> Option<i64> {
        let raw = self.timer_started_at.as_deref()?;
        chrono::DateTime::parse_from_rfc3339(raw)
            .ok()
            .map(|d| d.timestamp())
    }

    pub fn timer_running(&self) -> bool {
        self.started_epoch().is_some()
    }

    pub fn elapsed_s(&self, now: i64) -> Option<i64> {
        self.started_epoch().map(|s| (now - s).max(0))
    }
}

pub fn state_dir() -> Option<PathBuf> {
    state_dir_from(std::env::var_os("XDG_STATE_HOME"), std::env::var_os("HOME"))
}

fn state_dir_from(
    xdg: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Option<PathBuf> {
    if let Some(x) = xdg.map(PathBuf::from).filter(|p| p.is_absolute()) {
        return Some(x.join("tmgr"));
    }
    let home = home.map(PathBuf::from).filter(|p| p.is_absolute())?;
    Some(home.join(".local").join("state").join("tmgr"))
}

pub fn active_path() -> Option<PathBuf> {
    state_dir().map(|d| d.join("active.json"))
}

pub fn load() -> Option<ActiveState> {
    load_from(&active_path()?)
}

pub fn load_from(path: &Path) -> Option<ActiveState> {
    let file = fs::File::open(path).ok()?;
    let mut buf = Vec::new();
    file.take(MAX_FILE + 1).read_to_end(&mut buf).ok()?;
    if buf.len() as u64 > MAX_FILE {
        return None;
    }
    parse(&buf)
}

pub fn parse(bytes: &[u8]) -> Option<ActiveState> {
    let mut state: ActiveState = serde_json::from_slice(bytes).ok()?;
    if state.schema != ACTIVE_SCHEMA {
        return None;
    }
    if !valid_key(&state.task.key) || state.task.id == 0 {
        return None;
    }
    state.task.title = truncate_chars(&sanitize_line(&state.task.title), MAX_TITLE).0;
    state.task.url = state.task.url.take().and_then(clean_field);
    state.workspace = state.workspace.take().and_then(clean_field);
    state.branch = state.branch.take().and_then(clean_field);
    state.repo = state.repo.take().and_then(clean_field);
    state.timer_started_at = state
        .timer_started_at
        .take()
        .filter(|s| chrono::DateTime::parse_from_rfc3339(s).is_ok());
    Some(state)
}

fn clean_field(raw: String) -> Option<String> {
    let clean = truncate_chars(&sanitize_line(&raw), MAX_FIELD).0;
    (!clean.trim().is_empty()).then_some(clean)
}

pub fn valid_key(key: &str) -> bool {
    if parse_key(key).is_some() {
        return true;
    }
    key.strip_prefix('#')
        .is_some_and(|d| (1..=9).contains(&d.len()) && d.bytes().all(|b| b.is_ascii_digit()))
}

pub fn save(state: &ActiveState) -> Result<()> {
    let dir = state_dir().ok_or_else(|| Error::internal("cannot locate the state directory"))?;
    ensure_private_dir(&dir)?;
    let bytes = serde_json::to_vec_pretty(state)
        .map_err(|_| Error::internal("cannot serialize the active task"))?;
    write_atomic(&dir.join("active.json"), &bytes, 0o600)
}

pub fn clear() -> Result<()> {
    if let Some(path) = active_path() {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ActiveState {
        let mut s = ActiveState::new(ActiveTask {
            id: 8974,
            key: "TM-509".into(),
            title: "Repo prep".into(),
            url: Some("https://tmgr.dev/personal/tasks/8974".into()),
        });
        s.workspace = Some("personal".into());
        s.timer_started_at = Some("2026-10-12T09:00:00Z".into());
        s
    }

    #[test]
    fn round_trips_through_json() {
        let s = sample();
        let back = parse(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!(back, s);
        assert_eq!(back.started_epoch(), Some(1_791_795_600));
        assert_eq!(back.elapsed_s(1_791_795_600 + 4320), Some(4320));
        assert_eq!(back.elapsed_s(0), Some(0));
    }

    #[test]
    fn garbage_is_ignored() {
        for bad in [
            &b""[..],
            b"not json",
            b"{}",
            b"[]",
            br#"{"schema":"other","task":{"id":1,"key":"TM-1","title":"x"}}"#,
            br#"{"schema":"tmgr.active.v1","task":{"id":1,"key":"x;rm","title":"x"}}"#,
            br#"{"schema":"tmgr.active.v1","task":{"id":0,"key":"TM-1","title":"x"}}"#,
            br#"{"schema":"tmgr.active.v1","task":{"id":-1,"key":"TM-1","title":"x"}}"#,
            &[0xff, 0xfe, 0x00],
        ] {
            assert!(parse(bad).is_none(), "{:?}", String::from_utf8_lossy(bad));
        }
    }

    #[test]
    fn control_characters_are_stripped_from_text_fields() {
        let raw = r#"{"schema":"tmgr.active.v1","task":{"id":1,"key":"TM-1","title":"a\u001b[31mb\nc","url":"https://x/\u0007y"},"workspace":"w\u001b","branch":"b\r","timer_started_at":"nonsense"}"#;
        let s = parse(raw.as_bytes()).unwrap();
        assert!(!s.task.title.contains(['\u{1b}', '\n']));
        assert!(!s.task.url.unwrap().contains('\u{7}'));
        assert!(!s.workspace.unwrap().contains('\u{1b}'));
        assert!(!s.branch.unwrap().contains('\r'));
        assert_eq!(s.timer_started_at, None);
    }

    #[test]
    fn oversized_files_are_ignored() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("active.json");
        let mut s = sample();
        s.task.title = "x".repeat(20_000);
        fs::write(&p, serde_json::to_vec(&s).unwrap()).unwrap();
        assert!(load_from(&p).is_none());
        fs::write(&p, serde_json::to_vec(&sample()).unwrap()).unwrap();
        assert!(load_from(&p).is_some());
        assert!(load_from(&tmp.path().join("missing.json")).is_none());
    }

    #[test]
    fn long_titles_are_truncated() {
        let mut s = sample();
        s.task.title = "é".repeat(500);
        let back = parse(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert_eq!(back.task.title.chars().count(), 200);
    }

    #[test]
    fn ids_without_a_key_use_the_hash_form() {
        assert!(valid_key("#8974"));
        assert!(valid_key("TM-1"));
        assert!(!valid_key("#"));
        assert!(!valid_key("#12a"));
        assert!(!valid_key("tm-1"));
    }

    #[test]
    fn state_dir_prefers_absolute_xdg_state_home() {
        assert_eq!(
            state_dir_from(Some("/s".into()), Some("/h".into())),
            Some(PathBuf::from("/s/tmgr"))
        );
        assert_eq!(
            state_dir_from(Some("rel".into()), Some("/h".into())),
            Some(PathBuf::from("/h/.local/state/tmgr"))
        );
        assert_eq!(state_dir_from(None, None), None);
    }
}
