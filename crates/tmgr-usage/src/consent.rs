//! Whether this device lets tmgr read the local Claude Code and Codex logs (TM-638).

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::path::Path;

use crate::error::Result;
use crate::model::Tool;
use crate::paths;
use crate::store::Store;

pub const READ_LOGS_KEY: &str = "read_logs";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadLogs {
    Unset,
    Granted,
    Denied,
}

impl ReadLogs {
    pub fn as_str(self) -> &'static str {
        match self {
            ReadLogs::Unset => "unset",
            ReadLogs::Granted => "granted",
            ReadLogs::Denied => "denied",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogConsent {
    pub state: ReadLogs,
    pub claude: bool,
    pub codex: bool,
    pub decided_at: Option<DateTime<Utc>>,
}

impl Default for LogConsent {
    fn default() -> Self {
        LogConsent {
            state: ReadLogs::Unset,
            claude: true,
            codex: true,
            decided_at: None,
        }
    }
}

#[derive(Serialize, Deserialize)]
struct Stored {
    state: String,
    #[serde(default = "yes")]
    claude: bool,
    #[serde(default = "yes")]
    codex: bool,
    #[serde(default)]
    decided_at: Option<String>,
}

fn yes() -> bool {
    true
}

impl LogConsent {
    /// `user_run`: a command the user typed in a terminal. Desktop and background scans pass false.
    pub fn allows(&self, tool: Tool, user_run: bool) -> bool {
        match self.state {
            ReadLogs::Granted => match tool {
                Tool::Claude => self.claude,
                Tool::Codex => self.codex,
            },
            ReadLogs::Denied => false,
            ReadLogs::Unset => user_run,
        }
    }

    pub fn allows_any(&self, user_run: bool) -> bool {
        self.allows(Tool::Claude, user_run) || self.allows(Tool::Codex, user_run)
    }

    fn parse(raw: &str) -> LogConsent {
        let denied = LogConsent {
            state: ReadLogs::Denied,
            ..LogConsent::default()
        };
        let Ok(s) = serde_json::from_str::<Stored>(raw) else {
            return denied;
        };
        let state = match s.state.as_str() {
            "granted" => ReadLogs::Granted,
            "denied" => ReadLogs::Denied,
            _ => return denied,
        };
        LogConsent {
            state,
            claude: s.claude,
            codex: s.codex,
            decided_at: s
                .decided_at
                .and_then(|v| DateTime::parse_from_rfc3339(&v).ok())
                .map(|t| t.with_timezone(&Utc)),
        }
    }
}

impl Store {
    /// Missing key = unset; a value that cannot be read = denied (fail closed).
    pub fn log_consent(&self) -> Result<LogConsent> {
        Ok(self
            .setting(READ_LOGS_KEY)?
            .map_or_else(LogConsent::default, |raw| LogConsent::parse(&raw)))
    }

    /// Stores granted or denied; granted with both tools off is stored as denied. `Unset` removes the choice.
    pub fn set_log_consent(&self, c: &LogConsent) -> Result<()> {
        let state = match c.state {
            ReadLogs::Granted if !c.claude && !c.codex => ReadLogs::Denied,
            ReadLogs::Unset => {
                self.conn
                    .execute("DELETE FROM settings WHERE key = ?1", [READ_LOGS_KEY])?;
                return Ok(());
            }
            s => s,
        };
        let stored = Stored {
            state: state.as_str().into(),
            claude: c.claude,
            codex: c.codex,
            decided_at: c
                .decided_at
                .map(|t| t.to_rfc3339_opts(SecondsFormat::Secs, true)),
        };
        self.set_setting(READ_LOGS_KEY, &serde_json::to_string(&stored)?)
    }
}

/// Deletes everything scanned (database rows and statusLine snapshots); the reading choice stays.
pub fn wipe(store: &mut Store, dir: &Path) -> Result<()> {
    store.delete_all()?;
    let snaps = paths::snapshots_dir(dir);
    if snaps.is_dir() {
        std::fs::remove_dir_all(&snaps)?;
    }
    Ok(())
}

/// "Forget scanned data": turn reading off first (the per-tool flags are kept), then wipe, so a failed
/// wipe or a scan racing it never leaves reading on.
pub fn forget(store: &mut Store, dir: &Path, now: DateTime<Utc>) -> Result<LogConsent> {
    let c = LogConsent {
        state: ReadLogs::Denied,
        decided_at: Some(now),
        ..store.log_consent()?
    };
    store.set_log_consent(&c)?;
    wipe(store, dir)?;
    store.log_consent()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn store() -> (tempfile::TempDir, Store) {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("usage.db")).unwrap();
        (tmp, store)
    }

    fn t() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 25, 9, 30, 0).unwrap()
    }

    #[test]
    fn missing_key_is_unset_and_only_user_runs_read() {
        let (_tmp, s) = store();
        let c = s.log_consent().unwrap();
        assert_eq!(c, LogConsent::default());
        assert!(c.allows(Tool::Claude, true) && c.allows(Tool::Codex, true));
        assert!(!c.allows_any(false));
    }

    #[test]
    fn granted_round_trips_with_per_tool_flags() {
        let (_tmp, s) = store();
        let c = LogConsent {
            state: ReadLogs::Granted,
            claude: true,
            codex: false,
            decided_at: Some(t()),
        };
        s.set_log_consent(&c).unwrap();
        assert_eq!(s.log_consent().unwrap(), c);
        assert_eq!(
            s.setting(READ_LOGS_KEY).unwrap().unwrap(),
            r#"{"state":"granted","claude":true,"codex":false,"decided_at":"2026-10-25T09:30:00Z"}"#
        );
        for user_run in [true, false] {
            assert!(c.allows(Tool::Claude, user_run));
            assert!(!c.allows(Tool::Codex, user_run));
        }
    }

    #[test]
    fn granted_with_both_tools_off_is_stored_denied() {
        let (_tmp, s) = store();
        let c = LogConsent {
            state: ReadLogs::Granted,
            claude: false,
            codex: false,
            decided_at: Some(t()),
        };
        s.set_log_consent(&c).unwrap();
        let got = s.log_consent().unwrap();
        assert_eq!(got.state, ReadLogs::Denied);
        assert!(!got.claude && !got.codex);
        assert!(!got.allows_any(true));
    }

    #[test]
    fn garbage_or_unknown_state_reads_as_denied() {
        let (_tmp, s) = store();
        for raw in ["not json", r#"{"state":"maybe"}"#, r#"{"claude":true}"#] {
            s.set_setting(READ_LOGS_KEY, raw).unwrap();
            let c = s.log_consent().unwrap();
            assert_eq!(c.state, ReadLogs::Denied, "{raw}");
            assert!(!c.allows_any(true), "{raw}");
        }
        s.set_setting(READ_LOGS_KEY, r#"{"state":"granted"}"#)
            .unwrap();
        let c = s.log_consent().unwrap();
        assert!(c.claude && c.codex && c.decided_at.is_none());
    }

    #[test]
    fn delete_all_keeps_the_reading_choice() {
        let (_tmp, mut s) = store();
        let c = LogConsent {
            state: ReadLogs::Granted,
            decided_at: Some(t()),
            ..LogConsent::default()
        };
        s.set_log_consent(&c).unwrap();
        s.delete_all().unwrap();
        assert_eq!(s.log_consent().unwrap(), c);
    }

    #[test]
    fn forget_wipes_data_and_snapshots_then_denies_keeping_flags() {
        let (tmp, mut s) = store();
        s.set_log_consent(&LogConsent {
            state: ReadLogs::Granted,
            claude: true,
            codex: false,
            decided_at: Some(t()),
        })
        .unwrap();
        s.set_sync_consent(t()).unwrap();
        let snaps = paths::snapshots_dir(tmp.path()).join("claude");
        std::fs::create_dir_all(&snaps).unwrap();
        std::fs::write(snaps.join("x.json"), "{}").unwrap();
        let later = t() + chrono::Duration::hours(1);
        let c = forget(&mut s, tmp.path(), later).unwrap();
        assert_eq!(c.state, ReadLogs::Denied);
        assert!(c.claude && !c.codex);
        assert_eq!(c.decided_at, Some(later));
        assert!(!paths::snapshots_dir(tmp.path()).exists());
        assert!(s.sync_consent().unwrap().is_none());
        assert_eq!(forget(&mut s, tmp.path(), later).unwrap(), c);
    }

    #[cfg(unix)]
    #[test]
    fn a_failed_wipe_still_leaves_reading_off() {
        use std::os::unix::fs::PermissionsExt;
        let (tmp, mut s) = store();
        s.set_log_consent(&LogConsent {
            state: ReadLogs::Granted,
            decided_at: Some(t()),
            ..LogConsent::default()
        })
        .unwrap();
        let locked = paths::snapshots_dir(tmp.path()).join("claude");
        std::fs::create_dir_all(&locked).unwrap();
        std::fs::write(locked.join("x.json"), "{}").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();
        let result = forget(&mut s, tmp.path(), t());
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_err());
        assert_eq!(s.log_consent().unwrap().state, ReadLogs::Denied);
    }
}
