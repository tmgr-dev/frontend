pub mod send;

use crate::cmd::shared::{parse_date, today_local};
use crate::ctx::Ctx;
use crate::out::Output;
use chrono::{Local, Utc};
use clap::{Args, Subcommand, ValueEnum};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::time::Duration;
use tmgr_core::error::{Error, Result};
use tmgr_core::runs::parse_since;
use tmgr_core::text::sanitize_line;
use tmgr_runner::humantime::{self, Record, SentRecord, Settings, IDLE_MAX_S, IDLE_MIN_S};

const SYNC_LOCK_WAIT: Duration = Duration::from_secs(5);

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct TimeArgs {
    #[command(subcommand)]
    pub command: Option<TimeCommand>,
    /// Only time since local midnight
    #[arg(long, conflicts_with = "since")]
    pub today: bool,
    /// Only time since a date (YYYY-MM-DD) or an age such as 12h or 7d
    #[arg(long, value_name = "T")]
    pub since: Option<String>,
    /// How to group the rows
    #[arg(long, value_enum, default_value_t = By::Session)]
    pub by: By,
}

#[derive(Subcommand)]
pub enum TimeCommand {
    /// Show or change sending your attach time to the bound tasks
    Sync {
        #[arg(value_enum)]
        state: Option<Switch>,
    },
    /// Set the idle cap in seconds: after this long without activity time stops counting
    Idle {
        #[arg(value_name = "SECONDS")]
        seconds: u64,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum By {
    Session,
    Folder,
    Task,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Switch {
    On,
    Off,
}

pub fn run(ctx: &Ctx, args: &TimeArgs) -> Result<Output> {
    let dir = tmgr_ipc::dirs::runner_dir()
        .map_err(|e| Error::internal(format!("cannot locate the runner directory: {e}")))?;
    match &args.command {
        Some(TimeCommand::Sync { state }) => sync(&dir, *state, SYNC_LOCK_WAIT),
        Some(TimeCommand::Idle { seconds }) => idle(&dir, *seconds),
        None => list(ctx, &dir, args),
    }
}

fn sync(dir: &Path, state: Option<Switch>, wait: Duration) -> Result<Output> {
    let mut settings = Settings::load(dir);
    let mut still_sending = false;
    if let Some(s) = state {
        let _lock = match send::Lock::acquire(dir, wait) {
            Ok(lock) => {
                still_sending = lock.is_none();
                lock
            }
            Err(_) => None,
        };
        settings = Settings::load(dir);
        settings.sync = s == Switch::On;
        settings.save(dir)?;
    }
    let word = if settings.sync { "on" } else { "off" };
    let mut text = format!("Sending time to tasks is {word}");
    if still_sending && !settings.sync {
        text.push_str("\nA send may still be finishing");
    }
    Ok(Output::new(json!({"sync": settings.sync}), text))
}

fn idle(dir: &Path, seconds: u64) -> Result<Output> {
    if !(IDLE_MIN_S..=IDLE_MAX_S).contains(&seconds) {
        return Err(Error::usage("the idle cap is out of range")
            .hint(format!("use {IDLE_MIN_S} to {IDLE_MAX_S} seconds")));
    }
    let mut settings = Settings::load(dir);
    settings.idle_s = seconds;
    settings.save(dir)?;
    Ok(Output::new(
        json!({"idle_s": seconds}),
        format!("Idle cap is {seconds} s"),
    ))
}

fn since_s(args: &TimeArgs) -> Result<Option<u64>> {
    let at = if args.today {
        Some(midnight(today_local()))
    } else if let Some(raw) = &args.since {
        Some(match parse_since(raw) {
            Some(age) => (Utc::now() - age).timestamp(),
            None => midnight(
                parse_date(raw)
                    .map_err(|e| e.hint("use YYYY-MM-DD or an age such as 12h or 7d"))?,
            ),
        })
    } else {
        None
    };
    Ok(at.map(|t| t.max(0) as u64))
}

fn midnight(day: chrono::NaiveDate) -> i64 {
    day.and_hms_opt(0, 0, 0)
        .and_then(|t| t.and_local_timezone(Local).earliest())
        .map_or(0, |t| t.timestamp())
}

#[derive(Default)]
struct Row {
    name: String,
    folder: String,
    tasks: BTreeSet<(i64, i64)>,
    sessions: BTreeSet<String>,
    typing_s: u64,
    reading_s: u64,
    states: BTreeSet<&'static str>,
    last_end: u64,
}

const STATE_ORDER: [&str; 7] = [
    "pending",
    "skipped",
    "expired",
    "invalid",
    "local",
    "sent",
    "not bound",
];

fn state_of(r: &Record, sent: &HashMap<String, SentRecord>, sync: bool) -> &'static str {
    if r.task.is_none() {
        return "not bound";
    }
    match sent.get(&r.id).map(|s| s.status.as_str()) {
        Some("created" | "duplicate" | "conflict") => "sent",
        Some("skipped") => "skipped",
        Some("expired") => "expired",
        Some("invalid") => "invalid",
        Some("local") => "local",
        Some(_) => "sent",
        None if sync => "pending",
        None => "skipped",
    }
}

fn worst(states: &BTreeSet<&'static str>) -> &'static str {
    STATE_ORDER
        .into_iter()
        .find(|s| states.contains(s))
        .unwrap_or("not bound")
}

fn task_ref(t: (i64, i64)) -> String {
    format!("{}:{}", t.0, t.1)
}

fn tilde(path: &str) -> String {
    match std::env::var("HOME") {
        Ok(h)
            if !h.is_empty()
                && path
                    .strip_prefix(&h)
                    .is_some_and(|r| r.is_empty() || r.starts_with('/')) =>
        {
            format!("~{}", &path[h.len()..])
        }
        _ => path.to_string(),
    }
}

pub fn duration(s: u64) -> String {
    let (h, m, sec) = (s / 3600, s % 3600 / 60, s % 60);
    if h > 0 {
        format!("{h}h {m:02}m")
    } else if m > 0 {
        format!("{m}m {sec:02}s")
    } else {
        format!("{sec}s")
    }
}

fn list(ctx: &Ctx, dir: &Path, args: &TimeArgs) -> Result<Output> {
    let since = since_s(args)?;
    let settings = Settings::load(dir);
    let sent = humantime::read_sent(dir);
    let mut records = humantime::read_records(dir);
    if let Some(t) = since {
        records.retain(|r| r.end_s >= t);
    }
    records.sort_by_key(|r| r.start_s);

    let mut rows: BTreeMap<String, Row> = BTreeMap::new();
    for r in &records {
        let key = match args.by {
            By::Session => r.session.clone(),
            By::Folder => r.cwd.clone(),
            By::Task => r
                .task
                .map_or_else(String::new, |t| task_ref((t.workspace_id, t.task_id))),
        };
        let row = rows.entry(key).or_default();
        row.name = sanitize_line(&r.name);
        row.folder = sanitize_line(&r.cwd);
        if let Some(t) = r.task {
            row.tasks.insert((t.workspace_id, t.task_id));
        }
        row.sessions.insert(r.session.clone());
        row.typing_s += r.typing_s;
        row.reading_s += r.reading_s;
        row.last_end = row.last_end.max(r.end_s);
        row.states.insert(state_of(r, &sent, settings.sync));
    }
    let mut rows: Vec<Row> = rows.into_values().collect();
    rows.sort_by_key(|r| std::cmp::Reverse(r.last_end));

    let total = |f: fn(&Row) -> u64| rows.iter().map(f).sum::<u64>();
    let (typing, reading) = (total(|r| r.typing_s), total(|r| r.reading_s));

    let cells: Vec<[String; 7]> = rows
        .iter()
        .map(|r| {
            let name = match args.by {
                By::Session => r.name.clone(),
                _ => format!(
                    "{} session{}",
                    r.sessions.len(),
                    if r.sessions.len() == 1 { "" } else { "s" }
                ),
            };
            let tasks = r
                .tasks
                .iter()
                .copied()
                .map(task_ref)
                .collect::<Vec<_>>()
                .join(",");
            [
                name,
                tilde(&r.folder),
                if tasks.is_empty() { "-".into() } else { tasks },
                duration(r.typing_s + r.reading_s),
                duration(r.typing_s),
                duration(r.reading_s),
                worst(&r.states).to_string(),
            ]
        })
        .collect();

    let head = [
        "NAME", "FOLDER", "TASK", "TOTAL", "TYPING", "READING", "SENT",
    ];
    let widths: Vec<usize> = (0..7)
        .map(|i| {
            cells
                .iter()
                .map(|c| c[i].chars().count())
                .chain([head[i].chars().count()])
                .max()
                .unwrap_or(0)
        })
        .collect();
    let line = |c: &[String]| {
        c.iter()
            .enumerate()
            .map(|(i, v)| format!("{v:<w$}", w = widths[i]))
            .collect::<Vec<_>>()
            .join("  ")
            .trim_end()
            .to_string()
    };
    let mut lines = Vec::new();
    if cells.is_empty() {
        lines.push("No time recorded yet".to_string());
    } else {
        let h: Vec<String> = head.iter().map(|s| s.to_string()).collect();
        lines.push(ctx.style.bold(&line(&h)));
        lines.extend(cells.iter().map(|c| line(c)));
        lines.push(String::new());
        lines.push(format!(
            "Total {} (typing {}, reading {})",
            duration(typing + reading),
            duration(typing),
            duration(reading)
        ));
    }
    let state = send::SendState::load(dir);
    lines.push(format!(
        "Sending to tasks: {} - idle cap {} s",
        if settings.sync { "on" } else { "off" },
        settings.idle_s
    ));
    if let Some(h) = &state.hint {
        lines.push(format!("Last send failed: {h}"));
    }

    let data_rows: Vec<_> = rows
        .iter()
        .map(|r| {
            json!({
                "name": r.name,
                "folder": r.folder,
                "tasks": r.tasks.iter().map(|t| json!({"workspace_id": t.0, "task_id": t.1})).collect::<Vec<_>>(),
                "sessions": r.sessions.len(),
                "total_s": r.typing_s + r.reading_s,
                "typing_s": r.typing_s,
                "reading_s": r.reading_s,
                "state": worst(&r.states),
            })
        })
        .collect();
    Ok(Output::new(
        json!({
            "by": match args.by { By::Session => "session", By::Folder => "folder", By::Task => "task" },
            "since": since,
            "rows": data_rows,
            "totals": {"total_s": typing + reading, "typing_s": typing, "reading_s": reading},
            "sync": settings.sync,
            "idle_s": settings.idle_s,
            "hint": state.hint,
        }),
        lines.join("\n"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_off_waits_for_a_running_send_and_still_writes() {
        let d = tempfile::tempdir().unwrap();
        let held = send::Lock::try_acquire(d.path()).unwrap().unwrap();
        let out = sync(d.path(), Some(Switch::Off), Duration::from_millis(100)).unwrap();
        assert!(!Settings::load(d.path()).sync);
        assert!(out.human.contains("may still be finishing"));
        drop(held);
        let out = sync(d.path(), Some(Switch::On), Duration::from_millis(100)).unwrap();
        assert!(Settings::load(d.path()).sync);
        assert!(!out.human.contains("may still"));
    }
}
