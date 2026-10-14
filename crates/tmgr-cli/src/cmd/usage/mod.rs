mod accounts;
mod fmt;
mod limits;
mod pricing;
mod runs;
mod statusline;
mod summary;

use crate::cli_usage::{UsageArgs, UsageCommand};
use crate::ctx::Ctx;
use crate::out::Output;
use chrono::{DateTime, Utc};
use serde_json::json;
use std::io::IsTerminal;
use std::path::PathBuf;
use tmgr_core::error::{Error, Result};
use tmgr_usage::collector::Collector;
use tmgr_usage::error::UsageError;
use tmgr_usage::paths::{self, Env};
use tmgr_usage::{scan_account, Account, PriceBook, Store, Tool};

pub use statusline::statusline_hook;

const RETENTION_DAYS: u32 = 90;
const BIG_FIRST_SCAN_BYTES: u64 = 50 * 1024 * 1024;

pub fn fail(e: UsageError) -> Error {
    match e {
        UsageError::Invalid(msg) => Error::usage(msg),
        UsageError::Unsupported => Error::internal("this part of tmgr usage is not available yet"),
        other => Error::internal(format!("usage data: {other}")),
    }
}

pub struct Usage {
    pub env: Env,
    pub dir: PathBuf,
    pub store: Store,
    pub book: PriceBook,
}

impl Usage {
    pub fn open() -> Result<Usage> {
        let env = Env::from_process();
        let dir = paths::ensure_data_dir(&env).map_err(fail)?;
        let store = Store::open(&paths::db_path(&dir)).map_err(fail)?;
        let book = match store.load_prices().map_err(fail)? {
            Some((book, _)) => book,
            None => PriceBook::bundled(),
        };
        Ok(Usage {
            env,
            dir,
            store,
            book,
        })
    }

    pub fn accounts(&self) -> Result<Vec<Account>> {
        paths::all_accounts(&self.dir, &self.env).map_err(fail)
    }

    pub fn claude_snapshots(&self) -> PathBuf {
        paths::snapshots_dir(&self.dir).join("claude")
    }
}

#[derive(Default, Clone, Copy)]
pub struct ScanTotals {
    pub files: u64,
    pub events_new: u64,
    pub events_updated: u64,
    pub errors: u64,
}

fn first_scan_is_big(u: &Usage, accounts: &[Account]) -> bool {
    if u.store.event_count().unwrap_or(1) > 0 {
        return false;
    }
    let mut bytes = 0u64;
    for acc in accounts {
        let files = match acc.tool {
            Tool::Claude => tmgr_usage::claude::ClaudeCollector.discover(acc),
            Tool::Codex => tmgr_usage::codex::CodexCollector.discover(acc),
        };
        bytes += files
            .iter()
            .filter_map(|f| std::fs::metadata(&f.path).ok())
            .map(|m| m.len())
            .sum::<u64>();
    }
    bytes > BIG_FIRST_SCAN_BYTES
}

pub fn scan(ctx: &Ctx, u: &mut Usage, now: DateTime<Utc>) -> Result<ScanTotals> {
    let accounts = u.accounts()?;
    if !ctx.json && std::io::stderr().is_terminal() && first_scan_is_big(u, &accounts) {
        eprintln!("Reading AI tool logs for the first time, this can take a minute...");
    }
    let mut totals = ScanTotals::default();
    for acc in &accounts {
        match scan_account(&mut u.store, acc, &u.book, now) {
            Ok(r) => {
                totals.files += r.files;
                totals.events_new += r.events_new;
                totals.events_updated += r.events_updated;
                totals.errors += r.errors;
            }
            Err(_) => {
                totals.errors += 1;
                ctx.warn(
                    "usage_scan_failed",
                    format!("scanning the account {} failed", acc.label),
                );
            }
        }
    }
    if totals.errors > 0 {
        ctx.warn(
            "usage_scan_errors",
            format!("{} log files could not be read", totals.errors),
        );
    }
    ingest_statusline(u, &accounts);
    let _ = u.store.prune(RETENTION_DAYS, now);
    Ok(totals)
}

fn ingest_statusline(u: &Usage, accounts: &[Account]) {
    let Ok(snaps) = tmgr_usage::statusline::read_snapshots(&u.claude_snapshots()) else {
        return;
    };
    for snap in snaps {
        let label = snap
            .config_dir_tag
            .as_deref()
            .and_then(|tag| {
                accounts
                    .iter()
                    .find(|a| a.tool == Tool::Claude && a.root.to_str() == Some(tag))
            })
            .map_or("claude", |a| a.label.as_str());
        for limit in snap.to_limits(label) {
            let _ = u.store.record_limit(&limit);
        }
    }
}

pub fn run(ctx: &Ctx, args: &UsageArgs) -> Result<Output> {
    match &args.command {
        None => summary::run(ctx, &args.summary),
        Some(UsageCommand::Limits { no_scan }) => limits::run(ctx, *no_scan),
        Some(UsageCommand::Scan) => run_scan(ctx),
        Some(UsageCommand::Accounts { command }) => accounts::run(ctx, command.as_ref()),
        Some(UsageCommand::Pricing { command }) => pricing::run(ctx, command.as_ref()),
        Some(UsageCommand::Reset) => reset(ctx),
        Some(UsageCommand::Statusline { command }) => statusline::run(ctx, command),
        Some(UsageCommand::StatuslineHook(_)) => {
            Err(Error::internal("handled before the session starts"))
        }
    }
}

fn run_scan(ctx: &Ctx) -> Result<Output> {
    let mut u = Usage::open()?;
    let t = scan(ctx, &mut u, Utc::now())?;
    let events = u.store.event_count().map_err(fail)?;
    Ok(Output::new(
        json!({
            "files": t.files,
            "events_new": t.events_new,
            "events_updated": t.events_updated,
            "errors": t.errors,
            "events_total": events,
        }),
        format!(
            "Scanned {} log files: {} new events, {} updated ({} stored in total).",
            t.files, t.events_new, t.events_updated, events
        ),
    ))
}

fn reset(ctx: &Ctx) -> Result<Output> {
    crate::interact::confirm(
        "Delete all stored usage data (the AI tool logs themselves stay)?",
        true,
        ctx.yes,
    )?;
    let mut u = Usage::open()?;
    u.store.delete_all().map_err(fail)?;
    let snaps = paths::snapshots_dir(&u.dir);
    if snaps.is_dir() {
        let _ = std::fs::remove_dir_all(&snaps);
    }
    Ok(Output::new(
        json!({"reset": true}),
        "Usage data deleted. The next `tmgr usage` reads the logs again.",
    ))
}
