use chrono::{DateTime, Utc};
use std::path::Path;

use crate::error::Result;
use crate::model::Tool;
use crate::paths::{self, Env};
use crate::{scan_account, Account, PriceBook, Store, Tz};

const RETENTION_DAYS: u32 = 90;
const SNAPSHOT_KEEP_DAYS: u64 = 30;

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
    let accounts = paths::all_accounts(dir, env)?;
    let book = match store.load_prices()? {
        Some((book, _)) => book,
        None => PriceBook::bundled(),
    };
    let mut report = RefreshReport::default();
    for acc in &accounts {
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
    }
    let snapshots = paths::snapshots_dir(dir).join("claude");
    ingest_statusline(store, &snapshots, &accounts);
    let _ = store.prune_with_tz(RETENTION_DAYS, now, Tz::Local);
    crate::statusline::prune_snapshots(&snapshots, now, SNAPSHOT_KEEP_DAYS);
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
