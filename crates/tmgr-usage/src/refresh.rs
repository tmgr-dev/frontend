use chrono::{DateTime, Utc};
use std::path::Path;

use crate::error::Result;
use crate::model::Tool;
use crate::paths::{self, Env};
use crate::{scan_account, Account, PriceBook, Store, Tz};

const RETENTION_DAYS: u32 = 90;
const SNAPSHOT_KEEP_DAYS: u64 = 8;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RefreshReport {
    pub files: u64,
    pub events_new: u64,
    pub events_updated: u64,
    pub errors: u64,
    pub bad_lines: u64,
}

pub fn refresh(
    store: &mut Store,
    dir: &Path,
    env: &Env,
    now: DateTime<Utc>,
) -> Result<RefreshReport> {
    let _lock = crate::consent::scan_lock(dir)?;
    if !store.log_consent()?.allows_any(false) {
        return Ok(RefreshReport::default());
    }
    let accounts = paths::all_accounts(dir, env)?;
    let book = match store.load_prices()? {
        Some((book, _)) => book,
        None => PriceBook::bundled(),
    };
    let mut report = RefreshReport::default();
    crate::consent::each_allowed(store, &accounts, false, |store, acc| {
        match scan_account(store, acc, &book, now) {
            Ok(r) => {
                report.files += r.files;
                report.events_new += r.events_new;
                report.events_updated += r.events_updated;
                report.errors += r.errors;
                report.bad_lines += r.bad_lines + r.skipped_oversized;
            }
            Err(_) => report.errors += 1,
        }
    })?;
    if store.log_consent()?.allows(Tool::Claude, false) {
        let snapshots = paths::snapshots_dir(dir).join("claude");
        ingest_statusline(store, &snapshots, &accounts);
        crate::statusline::prune_snapshots(&snapshots, now, SNAPSHOT_KEEP_DAYS);
    }
    let _ = store.prune_with_tz(RETENTION_DAYS, now, Tz::Local);
    Ok(report)
}

fn ingest_statusline(store: &Store, snapshots: &Path, accounts: &[Account]) {
    let Ok(snaps) = crate::statusline::read_snapshots(snapshots) else {
        return;
    };
    for snap in snaps {
        let Some(tag) = snap.config_dir_tag.as_deref() else {
            continue;
        };
        let claude_accounts = || accounts.iter().filter(|a| a.tool == Tool::Claude);
        let label = match tag {
            "default" => claude_accounts().find(|a| a.label == "claude"),
            _ => claude_accounts().find(|a| crate::statusline::account_tag(&a.root) == tag),
        };
        let Some(label) = label.map(|a| a.label.as_str()) else {
            continue;
        };
        for limit in snap.to_limits(label) {
            let _ = store.record_limit(&limit);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consent::{LogConsent, ReadLogs};
    use crate::{Filters, GroupBy, Query};
    use chrono::Duration;
    use std::collections::HashMap;

    fn write(path: &Path, line: String) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, line + "\n").unwrap();
    }

    fn fixture(home: &Path, now: DateTime<Utc>) -> (Env, std::path::PathBuf, Store) {
        let ts = (now - Duration::minutes(5)).to_rfc3339();
        write(
            &home.join(".claude/projects/-work-a/s1.jsonl"),
            format!(
                r#"{{"type":"assistant","sessionId":"s1","timestamp":"{ts}","cwd":"/work/a","requestId":"r1","message":{{"id":"m1","model":"claude-test","usage":{{"input_tokens":10,"output_tokens":1}}}}}}"#
            ),
        );
        write(
            &home.join(".codex/sessions/2026/10/25/rollout-c1.jsonl"),
            [
                format!(r#"{{"timestamp":"{ts}","type":"session_meta","payload":{{"id":"c1","session_id":"c1","cwd":"/work/b"}}}}"#),
                format!(r#"{{"timestamp":"{ts}","type":"turn_context","payload":{{"model":"gpt-test","cwd":"/work/b"}}}}"#),
                format!(r#"{{"timestamp":"{ts}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":10,"cached_input_tokens":0,"output_tokens":1,"reasoning_output_tokens":0,"total_tokens":11}},"last_token_usage":{{"input_tokens":10,"cached_input_tokens":0,"output_tokens":1,"reasoning_output_tokens":0,"total_tokens":11}}}}}}}}"#),
            ]
            .join("\n"),
        );
        let env = Env {
            home: Some(home.to_path_buf()),
            vars: HashMap::new(),
        };
        let dir = home.join("data");
        let store = Store::open(&dir.join("usage.db")).unwrap();
        (env, dir, store)
    }

    fn tools(store: &Store, now: DateTime<Utc>) -> Vec<String> {
        let q = Query {
            from: now - Duration::days(1),
            to: now + Duration::days(1),
            tz: Tz::Local,
            group_by: vec![GroupBy::Tool],
            filters: Filters::default(),
        };
        let r = store.aggregate(&q).unwrap();
        r.rows.iter().map(|r| r.keys[0].clone()).collect()
    }

    #[test]
    fn nothing_is_read_until_reading_is_granted() {
        let tmp = tempfile::tempdir().unwrap();
        let now = Utc::now();
        let (env, dir, mut store) = fixture(tmp.path(), now);
        let r = refresh(&mut store, &dir, &env, now).unwrap();
        assert_eq!(r, RefreshReport::default());
        let denied = LogConsent {
            state: ReadLogs::Denied,
            ..LogConsent::default()
        };
        store.set_log_consent(&denied).unwrap();
        let r = refresh(&mut store, &dir, &env, now).unwrap();
        assert_eq!(r, RefreshReport::default());
        assert_eq!(store.event_count().unwrap(), 0);
    }

    #[test]
    fn granted_reads_only_the_allowed_tools() {
        let tmp = tempfile::tempdir().unwrap();
        let now = Utc::now();
        let (env, dir, mut store) = fixture(tmp.path(), now);
        let mut c = LogConsent {
            state: ReadLogs::Granted,
            codex: false,
            ..LogConsent::default()
        };
        store.set_log_consent(&c).unwrap();
        let r = refresh(&mut store, &dir, &env, now).unwrap();
        assert_eq!(r.files, 1);
        assert_eq!(tools(&store, now), ["claude"]);
        c.codex = true;
        store.set_log_consent(&c).unwrap();
        refresh(&mut store, &dir, &env, now).unwrap();
        assert_eq!(tools(&store, now), ["claude", "codex"]);
    }

    #[test]
    fn statusline_snapshots_are_read_only_under_the_claude_rule() {
        let tmp = tempfile::tempdir().unwrap();
        let now = Utc::now();
        let (env, dir, mut store) = fixture(tmp.path(), now);
        let reset = (now + Duration::hours(2)).timestamp();
        let body = format!(
            r#"{{"session_id":"abc","rate_limits":{{"five_hour":{{"used_percentage":55,"resets_at":{reset}}}}}}}"#
        );
        let snaps = paths::snapshots_dir(&dir).join("claude");
        crate::statusline::write_snapshot(&snaps, Some("default"), body.as_bytes()).unwrap();
        let claude_limits = |store: &Store| {
            store
                .current_limits(now)
                .unwrap()
                .iter()
                .filter(|l| l.tool == Tool::Claude)
                .count()
        };
        let mut c = LogConsent {
            state: ReadLogs::Granted,
            claude: false,
            ..LogConsent::default()
        };
        store.set_log_consent(&c).unwrap();
        refresh(&mut store, &dir, &env, now).unwrap();
        assert_eq!(claude_limits(&store), 0);
        c.claude = true;
        store.set_log_consent(&c).unwrap();
        refresh(&mut store, &dir, &env, now).unwrap();
        assert_eq!(claude_limits(&store), 1);
    }
}
