mod accounts;
mod fmt;
mod limits;
mod pricing;
mod runs;
mod statusline;
mod summary;
mod sync;
mod task;

use crate::cli_usage::{ConsentCommand, UsageArgs, UsageCommand};
use crate::ctx::Ctx;
use crate::out::Output;
use chrono::{DateTime, Utc};
use serde_json::json;
use std::io::IsTerminal;
use std::path::PathBuf;
use tmgr_core::error::{Error, Result};
use tmgr_usage::collector::Collector;
use tmgr_usage::consent::ReadLogs;
use tmgr_usage::error::UsageError;
use tmgr_usage::paths::{self, Env};
use tmgr_usage::{scan_account, Account, PriceBook, Store, Tool, Tz};

pub use statusline::statusline_hook;

const RETENTION_DAYS: u32 = 90;
const SNAPSHOT_KEEP_DAYS: u64 = 8;
const HISTORY_DROPPED_NOTICES: u32 = 3;
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
    pub bad_lines: u64,
    pub skipped_before_floor: u64,
    pub reading_off: bool,
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

const READING_OFF: &str = "Reading Claude Code and Codex logs is off on this device. Turn it on in TMGR desktop (AI Usage) or run `tmgr usage consent allow`.";

/// `user_run`: the user typed this command; background scans (autopilot, `sync --auto`) pass false
/// and need reading granted. A scan skipped for consent returns `reading_off`.
pub fn scan(ctx: &Ctx, u: &mut Usage, now: DateTime<Utc>, user_run: bool) -> Result<ScanTotals> {
    let consent = u.store.log_consent().map_err(fail)?;
    if !consent.allows_any(user_run) {
        if user_run {
            ctx.warn("usage_reading_off", READING_OFF);
        }
        return Ok(ScanTotals {
            reading_off: true,
            ..ScanTotals::default()
        });
    }
    let mut accounts = u.accounts()?;
    accounts.retain(|a| consent.allows(a.tool, user_run));
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
                totals.bad_lines += r.bad_lines + r.skipped_oversized;
                totals.skipped_before_floor += r.skipped_before_floor;
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
    if totals.bad_lines > 0 {
        ctx.warn(
            "usage_scan_bad_lines",
            format!(
                "{} log lines could not be parsed or were too long and were skipped",
                totals.bad_lines
            ),
        );
    }
    if totals.skipped_before_floor > 0
        && (ctx.json
            || (!ctx.quiet
                && u.store
                    .take_notice("usage_history_dropped", HISTORY_DROPPED_NOTICES)
                    .unwrap_or(false)))
    {
        ctx.warn(
            "usage_history_dropped",
            format!(
                "{} events are older than the {RETENTION_DAYS} days of kept history and were not stored",
                totals.skipped_before_floor
            ),
        );
    }
    ingest_statusline(u, &accounts);
    let _ = u.store.prune_with_tz(RETENTION_DAYS, now, Tz::Local);
    tmgr_usage::statusline::prune_snapshots(&u.claude_snapshots(), now, SNAPSHOT_KEEP_DAYS);
    Ok(totals)
}

fn ingest_statusline(u: &Usage, accounts: &[Account]) {
    let Ok(snaps) = tmgr_usage::statusline::read_snapshots(&u.claude_snapshots()) else {
        return;
    };
    for snap in snaps {
        let Some(tag) = snap.config_dir_tag.as_deref() else {
            continue;
        };
        let claude_accounts = || accounts.iter().filter(|a| a.tool == Tool::Claude);
        let label = match tag {
            "default" => claude_accounts().find(|a| a.label == "claude"),
            _ => claude_accounts().find(|a| tmgr_usage::statusline::account_tag(&a.root) == tag),
        };
        let Some(label) = label.map(|a| a.label.as_str()) else {
            continue;
        };
        for limit in snap.to_limits(label) {
            let _ = u.store.record_limit(&limit);
        }
    }
}

pub fn run(ctx: &Ctx, args: &UsageArgs) -> Result<Output> {
    match &args.command {
        None => summary::run(ctx, &args.summary),
        Some(UsageCommand::Limits { no_scan }) => limits::run(ctx, *no_scan),
        Some(UsageCommand::Task(task_args)) => task::run(ctx, task_args),
        Some(UsageCommand::Scan) => run_scan(ctx),
        Some(UsageCommand::Accounts { command }) => accounts::run(ctx, command.as_ref()),
        Some(UsageCommand::Pricing { command }) => pricing::run(ctx, command.as_ref()),
        Some(UsageCommand::Sync(sync_args)) => sync::run(ctx, sync_args),
        Some(UsageCommand::Consent { command }) => consent(*command),
        Some(UsageCommand::Reset) => reset(ctx),
        Some(UsageCommand::Statusline { command }) => statusline::run(ctx, command),
        Some(UsageCommand::StatuslineHook(_)) => {
            Err(Error::internal("handled before the session starts"))
        }
    }
}

fn run_scan(ctx: &Ctx) -> Result<Output> {
    let mut u = Usage::open()?;
    let t = scan(ctx, &mut u, Utc::now(), true)?;
    let events = u.store.event_count().map_err(fail)?;
    Ok(Output::new(
        json!({
            "files": t.files,
            "events_new": t.events_new,
            "events_updated": t.events_updated,
            "errors": t.errors,
            "bad_lines": t.bad_lines,
            "skipped_before_floor": t.skipped_before_floor,
            "reading_off": t.reading_off,
            "events_total": events,
        }),
        format!(
            "Scanned {} log files: {} new events, {} updated ({} stored in total).",
            t.files, t.events_new, t.events_updated, events
        ),
    ))
}

fn consent(command: Option<ConsentCommand>) -> Result<Output> {
    let u = Usage::open()?;
    let mut c = u.store.log_consent().map_err(fail)?;
    match command.unwrap_or(ConsentCommand::Status) {
        ConsentCommand::Status => {}
        ConsentCommand::Allow => {
            if !c.claude && !c.codex {
                (c.claude, c.codex) = (true, true);
            }
            c.state = ReadLogs::Granted;
            c.decided_at = Some(Utc::now());
            u.store.set_log_consent(&c).map_err(fail)?;
        }
        ConsentCommand::Deny => {
            c.state = ReadLogs::Denied;
            c.decided_at = Some(Utc::now());
            u.store.set_log_consent(&c).map_err(fail)?;
        }
    }
    let c = u.store.log_consent().map_err(fail)?;
    let tools: Vec<&str> = [(c.claude, "Claude Code"), (c.codex, "Codex")]
        .into_iter()
        .filter_map(|(on, name)| on.then_some(name))
        .collect();
    let kept = "Kept: counts, tokens, cost, model names, timestamps, session ids, project folders and branches, names of the tools, skills and commands used, edited file types and pull requests. Prompt and reply text is never stored. Nothing leaves this device unless you upload it with `tmgr usage sync`.";
    let human = match c.state {
        ReadLogs::Granted => format!(
            "Reading {} logs is on for this device. {kept}",
            tools.join(" and ")
        ),
        ReadLogs::Denied => "Reading Claude Code and Codex logs is off on this device; usage shows only stored data. `tmgr usage consent allow` turns it on.".to_string(),
        ReadLogs::Unset => format!(
            "Not decided yet: tmgr reads the Claude Code and Codex logs only when you run a usage command; TMGR desktop and background scans wait for `tmgr usage consent allow`. {kept}"
        ),
    };
    Ok(Output::new(
        json!({
            "state": c.state.as_str(),
            "claude": c.claude,
            "codex": c.codex,
            "decided_at": c.decided_at.map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        }),
        human,
    ))
}

fn reset(ctx: &Ctx) -> Result<Output> {
    crate::interact::confirm(
        "Delete all stored usage data (the AI tool logs themselves stay)?",
        true,
        ctx.yes,
    )?;
    let mut u = Usage::open()?;
    tmgr_usage::consent::wipe(&mut u.store, &u.dir).map_err(fail)?;
    let denied = u.store.log_consent().map_err(fail)?.state == ReadLogs::Denied;
    Ok(Output::new(
        json!({"reset": true}),
        if denied {
            "Usage data deleted. Reading the logs stays off (`tmgr usage consent allow` turns it on)."
        } else {
            "Usage data deleted. The next `tmgr usage` reads the logs again."
        },
    ))
}
