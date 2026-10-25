//! Whether this device lets tmgr read the local Claude Code and Codex logs (TM-638).

use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use std::fs::{File, TryLockError};
use std::path::Path;
use std::time::{Duration, Instant};

use crate::error::{Result, UsageError};
use crate::model::{Account, Tool};
use crate::paths::{self, Env};
use crate::store::Store;

pub const READ_LOGS_KEY: &str = "read_logs";
/// How long Forget waits for running scans (any process) to finish.
pub const FORGET_WAIT: Duration = Duration::from_secs(10);

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

/// A field-level change: only the given fields are applied to the stored value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConsentChange {
    pub state: Option<ReadLogs>,
    pub claude: Option<bool>,
    pub codex: Option<bool>,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    state: String,
    #[serde(default)]
    claude: bool,
    #[serde(default)]
    codex: bool,
    #[serde(default)]
    decided_at: Option<String>,
}

impl LogConsent {
    /// `user_run`: a command the user typed in a terminal. Desktop and background scans pass false.
    pub fn allows(&self, tool: Tool, user_run: bool) -> bool {
        let flag = match tool {
            Tool::Claude => self.claude,
            Tool::Codex => self.codex,
        };
        match self.state {
            ReadLogs::Granted => flag,
            ReadLogs::Denied => false,
            ReadLogs::Unset => user_run && flag,
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
            "granted" if s.claude || s.codex => ReadLogs::Granted,
            "granted" | "denied" => ReadLogs::Denied,
            "unset" => ReadLogs::Unset,
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

/// The choice as `conn` sees it (inside its transaction, if one is open).
pub(crate) fn read_consent(conn: &Connection) -> Result<LogConsent> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            [READ_LOGS_KEY],
            |r| r.get(0),
        )
        .optional()?;
    Ok(raw.map_or_else(LogConsent::default, |raw| LogConsent::parse(&raw)))
}

impl Store {
    /// Missing key = unset; a value that cannot be read = denied (fail closed).
    pub fn log_consent(&self) -> Result<LogConsent> {
        read_consent(&self.conn)
    }

    /// Stores the value as given; granted with both tools off is stored as denied.
    pub fn set_log_consent(&self, c: &LogConsent) -> Result<()> {
        let state = match c.state {
            ReadLogs::Granted if !c.claude && !c.codex => ReadLogs::Denied,
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

    /// Applies only the given fields to the stored value in one IMMEDIATE transaction, so a stale
    /// caller cannot undo a later choice. Only an explicit `Granted` grants; tool switches never do.
    /// `Granted` without tool fields while both are off turns both on. No field = no change.
    pub fn change_log_consent(&self, ch: &ConsentChange, now: DateTime<Utc>) -> Result<LogConsent> {
        if *ch == ConsentChange::default() {
            return self.log_consent();
        }
        if ch.state == Some(ReadLogs::Unset) {
            return Err(UsageError::Invalid("unknown choice".into()));
        }
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let mut c = self.log_consent()?;
        if let Some(on) = ch.claude {
            c.claude = on;
        }
        if let Some(on) = ch.codex {
            c.codex = on;
        }
        if let Some(state) = ch.state {
            c.state = state;
            if state == ReadLogs::Granted
                && ch.claude.is_none()
                && ch.codex.is_none()
                && !c.claude
                && !c.codex
            {
                (c.claude, c.codex) = (true, true);
            }
        }
        c.decided_at = Some(now);
        self.set_log_consent(&c)?;
        tx.commit()?;
        self.log_consent()
    }
}

/// Shared lock on `<dir>/scan.lock`, held by every scan (any process) for its whole run.
pub fn scan_lock(dir: &Path) -> Result<File> {
    let f = lock_file(dir)?;
    f.lock_shared()?;
    Ok(f)
}

fn lock_file(dir: &Path) -> Result<File> {
    Ok(std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(dir.join("scan.lock"))?)
}

/// Exclusive lock on `<dir>/scan.lock`: waits up to `wait` for running scans, then `UsageError::Busy`.
pub fn exclusive_lock(dir: &Path, wait: Duration) -> Result<File> {
    let f = lock_file(dir)?;
    let until = Instant::now() + wait;
    loop {
        match f.try_lock() {
            Ok(()) => return Ok(f),
            Err(TryLockError::WouldBlock) if Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(TryLockError::WouldBlock) => return Err(UsageError::Busy),
            Err(TryLockError::Error(e)) => return Err(e.into()),
        }
    }
}

/// Runs `each` for every account whose tool is still allowed, re-reading the choice before each
/// one (Forget may turn reading off meanwhile). Returns the tools covered: allowed by the choice as
/// it stands after the last account, and never skipped.
pub fn each_allowed(
    store: &mut Store,
    accounts: &[Account],
    user_run: bool,
    mut each: impl FnMut(&mut Store, &Account),
) -> Result<Vec<Tool>> {
    let mut skipped = Vec::new();
    for acc in accounts {
        if store.log_consent()?.allows(acc.tool, user_run) {
            each(store, acc);
        } else if !skipped.contains(&acc.tool) {
            skipped.push(acc.tool);
        }
    }
    let last = store.log_consent()?;
    Ok([Tool::Claude, Tool::Codex]
        .into_iter()
        .filter(|t| last.allows(*t, user_run) && !skipped.contains(t))
        .collect())
}

/// For background readers that must not create anything: whether `tool` may be read under the
/// background rule. A missing or unreadable store means no.
pub fn granted_read_only(env: &Env, tool: Tool) -> bool {
    let Ok(dir) = paths::data_dir(env) else {
        return false;
    };
    let db = paths::db_path(&dir);
    db.is_file()
        && Store::open_read_only(&db)
            .and_then(|s| s.log_consent())
            .is_ok_and(|c| c.allows(tool, false))
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

/// "Forget scanned data": turn reading off first (the per-tool flags are kept), then wait up to
/// `wait` for running scans to finish (`UsageError::Busy` after that), then wipe. A failed wipe or a
/// scan racing it never leaves reading on.
pub fn forget(
    store: &mut Store,
    dir: &Path,
    now: DateTime<Utc>,
    wait: Duration,
) -> Result<LogConsent> {
    let deny = ConsentChange {
        state: Some(ReadLogs::Denied),
        ..ConsentChange::default()
    };
    store.change_log_consent(&deny, now)?;
    let _lock = exclusive_lock(dir, wait)?;
    wipe(store, dir)?;
    store.log_consent()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::collections::HashMap;
    use std::path::PathBuf;

    fn store() -> (tempfile::TempDir, Store) {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("usage.db")).unwrap();
        (tmp, store)
    }

    fn t() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 25, 9, 30, 0).unwrap()
    }

    fn change(state: Option<ReadLogs>, claude: Option<bool>, codex: Option<bool>) -> ConsentChange {
        ConsentChange {
            state,
            claude,
            codex,
        }
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
    fn garbage_unknown_state_or_missing_tools_read_as_denied() {
        let (_tmp, s) = store();
        for raw in [
            "not json",
            r#"{"state":"maybe"}"#,
            r#"{"claude":true}"#,
            r#"{"state":"granted"}"#,
        ] {
            s.set_setting(READ_LOGS_KEY, raw).unwrap();
            let c = s.log_consent().unwrap();
            assert_eq!(c.state, ReadLogs::Denied, "{raw}");
            assert!(!c.allows_any(true), "{raw}");
        }
        s.set_setting(READ_LOGS_KEY, r#"{"state":"granted","claude":true}"#)
            .unwrap();
        let c = s.log_consent().unwrap();
        assert_eq!(c.state, ReadLogs::Granted);
        assert!(c.claude && !c.codex && c.decided_at.is_none());
    }

    #[test]
    fn changes_apply_only_the_given_fields() {
        let (_tmp, s) = store();
        let c = s
            .change_log_consent(&change(None, None, Some(false)), t())
            .unwrap();
        assert_eq!((c.state, c.claude, c.codex), (ReadLogs::Unset, true, false));
        assert!(c.allows(Tool::Claude, true) && !c.allows(Tool::Codex, true));
        let c = s
            .change_log_consent(&change(Some(ReadLogs::Granted), None, None), t())
            .unwrap();
        assert_eq!(
            (c.state, c.claude, c.codex),
            (ReadLogs::Granted, true, false)
        );
        let c = s
            .change_log_consent(&change(Some(ReadLogs::Denied), None, None), t())
            .unwrap();
        assert_eq!(
            (c.state, c.claude, c.codex),
            (ReadLogs::Denied, true, false)
        );
        let stale = s
            .change_log_consent(&change(None, Some(false), None), t())
            .unwrap();
        assert_eq!(stale.state, ReadLogs::Denied, "a tool switch never grants");
        let later = t() + chrono::Duration::hours(1);
        let c = s
            .change_log_consent(&change(Some(ReadLogs::Granted), None, None), later)
            .unwrap();
        assert_eq!(
            (c.state, c.claude, c.codex),
            (ReadLogs::Granted, true, true)
        );
        assert_eq!(c.decided_at, Some(later));
        let off = s
            .change_log_consent(&change(None, Some(false), Some(false)), t())
            .unwrap();
        assert_eq!(off.state, ReadLogs::Denied);
        assert_eq!(
            s.change_log_consent(&ConsentChange::default(), later)
                .unwrap(),
            off
        );
        assert!(s
            .change_log_consent(&change(Some(ReadLogs::Unset), None, None), t())
            .is_err());
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
        let c = forget(&mut s, tmp.path(), later, FORGET_WAIT).unwrap();
        assert_eq!(c.state, ReadLogs::Denied);
        assert!(c.claude && !c.codex);
        assert_eq!(c.decided_at, Some(later));
        assert!(!paths::snapshots_dir(tmp.path()).exists());
        assert!(s.sync_consent().unwrap().is_none());
        assert_eq!(forget(&mut s, tmp.path(), later, FORGET_WAIT).unwrap(), c);
    }

    #[test]
    fn a_running_scan_makes_forget_time_out_with_reading_already_off() {
        let (tmp, mut s) = store();
        s.set_log_consent(&LogConsent {
            state: ReadLogs::Granted,
            decided_at: Some(t()),
            ..LogConsent::default()
        })
        .unwrap();
        s.set_sync_consent(t()).unwrap();
        let held = scan_lock(tmp.path()).unwrap();
        let err = forget(&mut s, tmp.path(), t(), Duration::from_millis(120)).unwrap_err();
        assert!(matches!(err, UsageError::Busy), "{err}");
        assert_eq!(s.log_consent().unwrap().state, ReadLogs::Denied);
        assert!(s.sync_consent().unwrap().is_some(), "nothing was wiped");
        drop(held);
        forget(&mut s, tmp.path(), t(), Duration::from_millis(120)).unwrap();
        assert!(s.sync_consent().unwrap().is_none());
    }

    #[test]
    fn reading_turned_off_between_accounts_skips_the_rest() {
        let (_tmp, mut s) = store();
        s.set_log_consent(&LogConsent {
            state: ReadLogs::Granted,
            ..LogConsent::default()
        })
        .unwrap();
        let acc = |label: &str| Account {
            tool: Tool::Claude,
            label: label.into(),
            root: PathBuf::from("/nowhere"),
        };
        let mut seen = Vec::new();
        let covered = each_allowed(&mut s, &[acc("a"), acc("b")], false, |store, a| {
            seen.push(a.label.clone());
            store
                .change_log_consent(&change(None, Some(false), None), t())
                .unwrap();
        })
        .unwrap();
        assert_eq!(seen, ["a"]);
        assert_eq!(covered, [Tool::Codex], "claude was turned off midway");
    }

    #[test]
    fn reading_turned_off_during_the_last_account_is_not_covered() {
        let (_tmp, mut s) = store();
        s.set_log_consent(&LogConsent {
            state: ReadLogs::Granted,
            ..LogConsent::default()
        })
        .unwrap();
        let acc = Account {
            tool: Tool::Codex,
            label: "codex".into(),
            root: PathBuf::from("/nowhere"),
        };
        let covered = each_allowed(&mut s, &[acc], false, |store, _| {
            store
                .change_log_consent(&change(Some(ReadLogs::Denied), None, None), t())
                .unwrap();
        })
        .unwrap();
        assert!(covered.is_empty(), "{covered:?}");
    }

    #[test]
    fn background_check_reads_without_creating_the_store() {
        let tmp = tempfile::tempdir().unwrap();
        let env = Env {
            home: None,
            vars: HashMap::from([(
                "TMGR_USAGE_DIR".to_string(),
                tmp.path().join("usage").display().to_string(),
            )]),
        };
        assert!(!granted_read_only(&env, Tool::Claude));
        assert!(!tmp.path().join("usage").exists());
        std::fs::create_dir_all(tmp.path().join("usage")).unwrap();
        let s = Store::open(&tmp.path().join("usage/usage.db")).unwrap();
        assert!(
            !granted_read_only(&env, Tool::Claude),
            "unset is not granted"
        );
        s.change_log_consent(&change(Some(ReadLogs::Granted), None, Some(false)), t())
            .unwrap();
        assert!(granted_read_only(&env, Tool::Claude));
        assert!(!granted_read_only(&env, Tool::Codex));
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
        let result = forget(&mut s, tmp.path(), t(), FORGET_WAIT);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_err());
        assert_eq!(s.log_consent().unwrap().state, ReadLogs::Denied);
    }
}
