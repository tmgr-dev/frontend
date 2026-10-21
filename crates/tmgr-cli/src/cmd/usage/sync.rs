use super::runs::WantedTask;
use super::task::{Bucket, Corpus};
use super::{fail, Usage};
use crate::cli_usage::{AutoCommand, SyncArgs, SyncCommand, SyncRunArgs};
use crate::cmd::shared::resolve_task;
use crate::ctx::Ctx;
use crate::interact;
use crate::out::Output;
use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tmgr_core::api::{scopes_hint, Client};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::local;
use tmgr_core::refs::{parse_task_ref, TaskRef};
use tmgr_core::text::sanitize_line;
use tmgr_usage::sync::LedgerEntry;
use tmgr_usage::task::{validate_json, TaskIdent};

const PUT_PATH: &str = "/api/agent-usage";
const SCOPE: &str = "agent-usage:write";
const MAX_BODY: usize = 256 * 1024;
const PUTS_PER_WINDOW: usize = 25;
const WINDOW: Duration = Duration::from_secs(60);
const MAX_RETRY_AFTER_S: u64 = 120;
const LOCK_STALE: Duration = Duration::from_secs(600);
const ID_MISMATCH: &str = "task id mismatch";
const SETTINGS_HINT: &str =
    "Turn on Agent usage sync in TMGR Settings → Agent work (web or desktop)";
const AUTO_QUESTION: &str = "Also upload automatically after each agent run?";
const AUTO_DISCLAIMER: &str = "\
Automatic upload: after each agent run on a task of a cloud workspace, tmgr uploads that task's summary without asking again.\n\
\n\
Sent (the `task_usage.v1` summary of the task):\n\
  - the task's workspace code and task reference or id\n\
  - the period covered and the totals: AI cost (API-equivalent USD), number of events, tokens by kind, active time, turns\n\
  - for every agent in the tree: agent type, model, start and end times, cost, tokens, and the names and counts of the tools it used\n\
  - context: names of tools, skills, plugins and slash commands, MCP server and tool names, hook names, permission modes, how many files were edited and with which extensions, agent versions and entrypoints\n\
  - rate-limit percentages and the model prices used to compute the cost\n\
Pull requests stay on your machine.\n\
Never sent: prompts, replies, code, file contents, file paths or command lines.\n\
\n\
Where it goes: the TMGR server this CLI is logged in to. Every member of the task's workspace can see it.\n\
\n\
Turn it off: `tmgr usage sync auto off` (or `tmgr usage sync off`). Delete what was uploaded: `tmgr usage sync forget`.\n";

fn wait_override() -> Option<Duration> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var("TMGR_USAGE_SYNC_WAIT_MS")
        .ok()?
        .parse()
        .ok()
        .map(Duration::from_millis)
}

fn limit_override() -> Option<usize> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var("TMGR_USAGE_SYNC_PER_WINDOW")
        .ok()?
        .parse()
        .ok()
}

struct Pace {
    window: Duration,
    limit: usize,
    sent: VecDeque<Instant>,
}

impl Pace {
    fn new(window: Duration) -> Pace {
        Pace {
            window,
            limit: limit_override().unwrap_or(PUTS_PER_WINDOW),
            sent: VecDeque::new(),
        }
    }

    fn delay(&mut self, now: Instant) -> Duration {
        while self
            .sent
            .front()
            .is_some_and(|t| now.duration_since(*t) >= self.window)
        {
            self.sent.pop_front();
        }
        if self.sent.len() < self.limit {
            return Duration::ZERO;
        }
        (self.sent[0] + self.window).saturating_duration_since(now)
    }

    fn record(&mut self, at: Instant) {
        self.sent.push_back(at);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Outcome {
    Sent,
    Stale,
    Unchanged,
    RefusedShrink,
    Invalid,
    Skipped,
    Failed,
    WouldSend,
}

impl Outcome {
    fn name(self) -> &'static str {
        match self {
            Outcome::Sent => "sent",
            Outcome::Stale => "stale",
            Outcome::Unchanged => "unchanged",
            Outcome::RefusedShrink => "refused_shrink",
            Outcome::Invalid => "invalid",
            Outcome::Skipped => "skipped",
            Outcome::Failed => "failed",
            Outcome::WouldSend => "would_send",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Outcome::RefusedShrink => "not sent",
            Outcome::WouldSend => "would send",
            other => other.name(),
        }
    }
}

const ALL: [Outcome; 8] = [
    Outcome::Sent,
    Outcome::Stale,
    Outcome::Unchanged,
    Outcome::RefusedShrink,
    Outcome::Invalid,
    Outcome::Skipped,
    Outcome::Failed,
    Outcome::WouldSend,
];

struct Row {
    workspace: String,
    task_ref: String,
    outcome: Outcome,
    reason: Option<String>,
    aggregate: Option<Value>,
}

impl Row {
    fn new(p: &Prepared, outcome: Outcome, reason: Option<String>) -> Row {
        Row {
            workspace: p.workspace.clone(),
            task_ref: p.task_ref.clone(),
            outcome,
            reason,
            aggregate: None,
        }
    }

    fn json(&self) -> Value {
        let mut m = Map::new();
        m.insert("workspace".into(), json!(self.workspace));
        m.insert("ref".into(), json!(self.task_ref));
        m.insert("outcome".into(), json!(self.outcome.name()));
        if let Some(r) = &self.reason {
            m.insert("reason".into(), json!(r));
        }
        if let Some(a) = &self.aggregate {
            m.insert("aggregate".into(), a.clone());
        }
        Value::Object(m)
    }
}

struct Ready {
    value: Value,
    digest: String,
    events: u64,
    cost: f64,
    generated_at: String,
    first_event_at: Option<String>,
    last_event_at: Option<String>,
}

enum State {
    Skip(String),
    Invalid(Vec<String>),
    Ready(Box<Ready>),
}

struct Prepared {
    workspace: String,
    task_ref: String,
    task_id: Option<u64>,
    state: State,
}

struct Stop {
    code: &'static str,
    message: String,
    hint: Option<String>,
}

enum Refusal {
    SyncOff,
    WorkspaceOff,
    Scope,
    Other,
}

fn refusal(body: &Value) -> Refusal {
    match body.get("error").and_then(Value::as_str) {
        Some("agent_usage_sync_disabled") => Refusal::SyncOff,
        Some("feature_disabled") => Refusal::WorkspaceOff,
        Some("insufficient_scope") => Refusal::Scope,
        _ => Refusal::Other,
    }
}

fn stop_for(refusal: &Refusal) -> Stop {
    match refusal {
        Refusal::SyncOff => Stop {
            code: "sync_disabled",
            message: "Agent usage sync is off on the server".into(),
            hint: Some(SETTINGS_HINT.into()),
        },
        Refusal::Scope => Stop {
            code: "insufficient_scope",
            message: format!("this login cannot upload usage (needs scope {SCOPE})"),
            hint: Some(scopes_hint(SCOPE)),
        },
        _ => Stop {
            code: "forbidden",
            message: "the server refused the request (403)".into(),
            hint: Some(scopes_hint(SCOPE)),
        },
    }
}

fn digest_of(value: &Value) -> String {
    let mut v = value.clone();
    if let Value::Object(m) = &mut v {
        m.shift_remove("generated_at");
        m.shift_remove("source_id");
    }
    let bytes = serde_json::to_vec(&v).unwrap_or_default();
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn span(events: Vec<DateTime<Utc>>, facts: Vec<DateTime<Utc>>) -> (Option<String>, Option<String>) {
    let times = if events.is_empty() { facts } else { events };
    let stamp = |t: Option<DateTime<Utc>>| t.map(|t| t.to_rfc3339_opts(SecondsFormat::Secs, true));
    (
        stamp(times.iter().min().copied()),
        stamp(times.iter().max().copied()),
    )
}

fn prepare(c: &Corpus, b: &Bucket) -> Prepared {
    let mut p = Prepared {
        workspace: sanitize_line(&b.workspace),
        task_ref: sanitize_line(&b.task_ref),
        task_id: b.id,
        state: State::Skip("no usage".into()),
    };
    if local::local_code(&b.workspace).is_some() {
        p.state = State::Skip("local workspace".into());
        return p;
    }
    if b.id_mismatch {
        p.state = State::Skip(ID_MISMATCH.into());
        return p;
    }
    let ident = TaskIdent {
        workspace: b.workspace.clone(),
        task_ref: b.task_ref.clone(),
        id: b.id,
    };
    let Some(mut aggregate) = c.aggregate(ident, &b.events, &b.facts) else {
        return p;
    };
    aggregate.context.prs.clear();
    let (events, cost) = (aggregate.totals.events, aggregate.totals.cost_usd);
    let generated_at = aggregate.generated_at.clone();
    let (first_event_at, last_event_at) = span(
        b.events
            .iter()
            .filter_map(|&i| c.events.get(i))
            .map(|e| e.ts)
            .collect(),
        b.facts
            .iter()
            .filter_map(|&i| c.facts.get(i))
            .map(|f| f.ts)
            .collect(),
    );
    let Ok(bytes) = serde_json::to_vec(&aggregate) else {
        p.state = State::Invalid(vec!["the aggregate could not be serialized".into()]);
        return p;
    };
    if bytes.len() > MAX_BODY {
        p.state = State::Invalid(vec!["the body is larger than 256 KB".into()]);
        return p;
    }
    let Ok(value) = serde_json::from_slice::<Value>(&bytes) else {
        p.state = State::Invalid(vec!["the aggregate is not valid JSON".into()]);
        return p;
    };
    if serde_json::to_vec(&value).ok().as_deref() != Some(bytes.as_slice()) {
        p.state = State::Invalid(vec!["the body does not serialize the same way twice".into()]);
        return p;
    }
    p.state = match validate_json(&value, c.now) {
        Err(violations) => State::Invalid(violations),
        Ok(()) => State::Ready(Box::new(Ready {
            digest: digest_of(&value),
            value,
            events,
            cost,
            generated_at,
            first_event_at,
            last_event_at,
        })),
    };
    p
}

fn matches(b: &Bucket, want: &WantedTask) -> bool {
    match want {
        WantedTask::Id(id) => b.id == Some(*id),
        WantedTask::Key(k) => b.task_ref.eq_ignore_ascii_case(k),
    }
}

fn consent_error() -> Error {
    Error::usage("uploading usage is not turned on")
        .hint("see what would be sent with --dry-run, then run `tmgr usage sync on` in a terminal")
}

fn collect(
    ctx: &Ctx,
    u: &mut Usage,
    days: i64,
    no_scan: bool,
    task: Option<&str>,
) -> Result<(Corpus, Vec<Prepared>)> {
    let want = match task.map(parse_task_ref).transpose()? {
        Some(TaskRef::Id(id)) => Some(WantedTask::Id(id)),
        Some(TaskRef::Key(k)) => Some(WantedTask::Key(k)),
        None => None,
    };
    let c = Corpus::load(ctx, u, days, no_scan)?;
    let mut buckets = c.buckets();
    if let Some(want) = &want {
        buckets.retain(|b| matches(b, want));
        let workspaces: BTreeSet<&str> = buckets.iter().map(|b| b.workspace.as_str()).collect();
        if workspaces.len() > 1 {
            return Err(Error::usage(format!(
                "{} matches tasks in several workspaces ({})",
                sanitize_line(task.unwrap_or("")),
                workspaces.into_iter().collect::<Vec<_>>().join(", ")
            ))
            .hint("pick one with --ws CODE"));
        }
    }
    let prepared = buckets.iter().map(|b| prepare(&c, b)).collect();
    Ok((c, prepared))
}

pub fn run(ctx: &Ctx, args: &SyncArgs) -> Result<Output> {
    match &args.command {
        None => sync(ctx, &args.run),
        Some(SyncCommand::On { yes_auto }) => on(ctx, *yes_auto),
        Some(SyncCommand::Auto { command }) => match command {
            AutoCommand::On { yes_auto } => auto_on(*yes_auto),
            AutoCommand::Off => auto_off(),
            AutoCommand::Status => auto_status(),
        },
        Some(SyncCommand::Off) => off(),
        Some(SyncCommand::Status) => status(),
        Some(SyncCommand::Forget { task }) => forget(ctx, task.as_deref()),
    }
}

fn counts_json(rows: &[Row]) -> Value {
    let mut m = Map::new();
    for o in ALL {
        m.insert(
            o.name().into(),
            json!(rows.iter().filter(|r| r.outcome == o).count()),
        );
    }
    Value::Object(m)
}

fn summary_text(rows: &[Row]) -> String {
    let parts: Vec<String> = ALL
        .iter()
        .filter_map(|o| {
            let n = rows.iter().filter(|r| r.outcome == *o).count();
            (n > 0).then(|| format!("{n} {}", o.label()))
        })
        .collect();
    if parts.is_empty() {
        "Nothing to upload.".into()
    } else {
        format!("Done: {}.", parts.join(", "))
    }
}

struct Sender<'a> {
    client: &'a Client,
    pace: Pace,
    wait: Option<Duration>,
}

impl Sender<'_> {
    fn put(&mut self, value: &Value, consent: &dyn Fn() -> bool) -> Result<Option<(u16, Value)>> {
        for attempt in 0..2 {
            let pause = self.pace.delay(Instant::now());
            if !pause.is_zero() {
                std::thread::sleep(pause);
            }
            if !consent() {
                return Ok(None);
            }
            self.pace.record(Instant::now());
            let Some((status, body)) =
                self.client
                    .call_guarded("PUT", PUT_PATH, Some(value), consent)?
            else {
                return Ok(None);
            };
            if status != 429 || attempt == 1 {
                return Ok(Some((status, body)));
            }
            let pause = self.wait.unwrap_or_else(|| {
                let secs = body
                    .get("retry_after_s")
                    .and_then(Value::as_u64)
                    .unwrap_or(WINDOW.as_secs());
                Duration::from_secs(secs.clamp(1, MAX_RETRY_AFTER_S))
            });
            std::thread::sleep(pause);
        }
        unreachable!("the loop returns on its last attempt")
    }
}

fn failure_reason(status: u16, body: &Value) -> String {
    match status {
        404 => "the task is unknown to the server or deleted".into(),
        409 => "this task already has the most sources the server keeps (20)".into(),
        413 => "the body is larger than the server accepts (256 KB)".into(),
        s => format!("the server answered HTTP {s} ({})", error_code(body)),
    }
}

const SERVER_CODES: &[&str] = &[
    "agent_usage_sync_disabled",
    "feature_disabled",
    "rate_limited",
    "insufficient_scope",
];

fn error_code(body: &Value) -> &'static str {
    let code = body.get("error").and_then(Value::as_str);
    SERVER_CODES
        .iter()
        .copied()
        .find(|c| Some(*c) == code)
        .unwrap_or("unknown")
}

struct AutoLock(PathBuf);

impl AutoLock {
    fn take(dir: &Path, task_id: u64) -> Option<AutoLock> {
        let path = dir.join(format!("sync-auto-{task_id}.lock"));
        for _ in 0..2 {
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(_) => return Some(AutoLock(path)),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let age = std::fs::metadata(&path)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.elapsed().ok());
                    if age.is_some_and(|a| a < LOCK_STALE) {
                        return None;
                    }
                    let _ = std::fs::remove_file(&path);
                }
                Err(_) => return None,
            }
        }
        None
    }
}

impl Drop for AutoLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn silent(days: i64) -> Output {
    Output::new(
        json!({
            "action": "sync",
            "dry_run": false,
            "days": days,
            "counts": counts_json(&[]),
            "results": [],
            "stopped": null,
        }),
        "",
    )
}

fn sync(ctx: &Ctx, args: &SyncRunArgs) -> Result<Output> {
    let mut u = Usage::open()?;
    let auto_task = args.task_id.filter(|_| args.auto);
    let mut _lock = None;
    let task = match auto_task {
        Some(id) => {
            if !u.store.sync_auto_active().map_err(fail)? {
                return Ok(silent(args.days));
            }
            let Some(lock) = AutoLock::take(&u.dir, id) else {
                return Ok(silent(args.days));
            };
            _lock = Some(lock);
            Some(format!("#{id}"))
        }
        None => args.task.clone(),
    };
    if !args.dry_run && u.store.sync_consent().map_err(fail)?.is_none() {
        return Err(consent_error());
    }
    let (_, prepared) = collect(ctx, &mut u, args.days, args.no_scan, task.as_deref())?;
    let mut rows: Vec<Row> = Vec::new();
    let mut stopped: Option<Stop> = None;
    let mut off_workspaces: BTreeSet<String> = BTreeSet::new();
    let mut sender: Option<Sender> = None;
    for p in &prepared {
        if let Some(s) = &stopped {
            rows.push(Row::new(
                p,
                Outcome::Skipped,
                Some(format!("run stopped: {}", s.message)),
            ));
            continue;
        }
        let ready = match &p.state {
            State::Skip(why) => {
                if why == ID_MISMATCH {
                    ctx.warn(
                        "usage_sync_id_mismatch",
                        format!("{} is not sent: {ID_MISMATCH}", p.task_ref),
                    );
                }
                rows.push(Row::new(p, Outcome::Skipped, Some(why.clone())));
                continue;
            }
            State::Invalid(violations) => {
                ctx.warn(
                    "usage_sync_invalid",
                    format!("{} is not sent: {}", p.task_ref, violations.join("; ")),
                );
                rows.push(Row::new(p, Outcome::Invalid, Some(violations.join("; "))));
                continue;
            }
            State::Ready(r) => r,
        };
        if args.dry_run {
            let mut row = Row::new(p, Outcome::WouldSend, None);
            row.aggregate = Some(ready.value.clone());
            rows.push(row);
            continue;
        }
        if off_workspaces.contains(&p.workspace) {
            rows.push(Row::new(
                p,
                Outcome::Skipped,
                Some("Agent work is off in this workspace".into()),
            ));
            continue;
        }
        if !args.force {
            if let Some(last) = u
                .store
                .ledger_get(&p.workspace, &p.task_ref)
                .map_err(fail)?
            {
                if last.digest == ready.digest {
                    rows.push(Row::new(p, Outcome::Unchanged, None));
                    continue;
                }
                let slid = matches!(
                    (&ready.first_event_at, &last.first_event_at),
                    (Some(now), Some(before)) if now > before
                );
                if ready.events < last.events || ready.cost < last.cost_usd - 1e-9 || slid {
                    ctx.warn(
                        "usage_sync_shrink",
                        format!(
                            "{} has less usage locally than what was uploaded before, so it is not sent (the local history may be shorter); `tmgr usage sync --task {} --force` sends it anyway",
                            p.task_ref, p.task_ref
                        ),
                    );
                    rows.push(Row::new(
                        p,
                        Outcome::RefusedShrink,
                        Some("less usage than uploaded before".into()),
                    ));
                    continue;
                }
            }
        }
        if sender.is_none() {
            sender = Some(Sender {
                client: ctx.cloud_client()?,
                pace: Pace::new(wait_override().unwrap_or(WINDOW)),
                wait: wait_override(),
            });
        }
        let sender = sender.as_mut().expect("sender is set above");
        let has_consent = || {
            u.store.sync_consent().ok().flatten().is_some()
                && (auto_task.is_none() || u.store.sync_auto().ok().flatten().is_some())
        };
        let (status, body) = match sender.put(&ready.value, &has_consent) {
            Ok(Some(r)) => r,
            Ok(None) => {
                let stop = Stop {
                    code: "consent_withdrawn",
                    message: "consent withdrawn".into(),
                    hint: Some(
                        "run `tmgr usage sync on` in a terminal to turn uploading on again".into(),
                    ),
                };
                rows.push(Row::new(p, Outcome::Skipped, Some(stop.message.clone())));
                stopped = Some(stop);
                continue;
            }
            Err(e) if e.kind == Kind::Auth => return Err(e),
            Err(e) => {
                stopped = Some(Stop {
                    code: "request_failed",
                    message: e.message.clone(),
                    hint: e.hint.clone(),
                });
                rows.push(Row::new(p, Outcome::Failed, Some(e.message)));
                continue;
            }
        };
        match status {
            200..=299 => {
                let stale = body["data"]["stale"].as_bool().unwrap_or(false);
                if stale {
                    rows.push(Row::new(
                        p,
                        Outcome::Stale,
                        Some("the server has newer data".into()),
                    ));
                } else {
                    let entry = LedgerEntry {
                        workspace: p.workspace.clone(),
                        task_ref: p.task_ref.clone(),
                        task_id: p.task_id,
                        generated_at: ready.generated_at.clone(),
                        events: ready.events,
                        cost_usd: ready.cost,
                        digest: ready.digest.clone(),
                        first_event_at: ready.first_event_at.clone(),
                        last_event_at: ready.last_event_at.clone(),
                    };
                    if u.store.ledger_put(&entry).is_err() {
                        ctx.warn(
                            "usage_sync_ledger",
                            "the upload record could not be saved: the next run sends this task again",
                        );
                    }
                    rows.push(Row::new(p, Outcome::Sent, None));
                }
            }
            403 => match refusal(&body) {
                Refusal::WorkspaceOff => {
                    ctx.warn(
                        "usage_sync_workspace_off",
                        format!(
                            "Agent work is off in workspace {}: its tasks are not uploaded",
                            p.workspace
                        ),
                    );
                    off_workspaces.insert(p.workspace.clone());
                    rows.push(Row::new(
                        p,
                        Outcome::Skipped,
                        Some("Agent work is off in this workspace".into()),
                    ));
                }
                other => {
                    if matches!(other, Refusal::SyncOff) {
                        let _ = u.store.ledger_clear();
                    }
                    let stop = stop_for(&other);
                    rows.push(Row::new(p, Outcome::Failed, Some(stop.message.clone())));
                    stopped = Some(stop);
                }
            },
            401 => {
                let stop = Stop {
                    code: "auth",
                    message: "not logged in or the session expired".into(),
                    hint: Some("tmgr login".into()),
                };
                rows.push(Row::new(p, Outcome::Failed, Some(stop.message.clone())));
                stopped = Some(stop);
            }
            429 => {
                let stop = Stop {
                    code: "rate_limited",
                    message: "the server is rate limiting uploads".into(),
                    hint: Some("run `tmgr usage sync` again in a minute".into()),
                };
                rows.push(Row::new(p, Outcome::Failed, Some(stop.message.clone())));
                stopped = Some(stop);
            }
            s => {
                let reason = failure_reason(s, &body);
                ctx.warn(
                    "usage_sync_failed",
                    format!("{} was not uploaded: {reason}", p.task_ref),
                );
                rows.push(Row::new(p, Outcome::Failed, Some(reason)));
            }
        }
    }
    if let Some(s) = &stopped {
        ctx.warn("usage_sync_stopped", s.message.clone());
    }
    let failed = rows.iter().any(|r| r.outcome == Outcome::Failed) || stopped.is_some();
    let now = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    if !args.dry_run {
        let record = json!({"at": now, "counts": counts_json(&rows), "stopped": stopped.as_ref().map(|s| s.code)});
        let _ = u.store.set_sync_last_run(&record.to_string());
    }
    let human = render(&rows, stopped.as_ref(), args.dry_run);
    let stop_json = stopped
        .as_ref()
        .map(|s| json!({"code": s.code, "message": s.message, "hint": s.hint}));
    Ok(Output::new(
        json!({
            "action": "sync",
            "dry_run": args.dry_run,
            "days": args.days,
            "counts": counts_json(&rows),
            "results": rows.iter().map(Row::json).collect::<Vec<_>>(),
            "stopped": stop_json,
        }),
        human,
    )
    .exit(i32::from(failed)))
}

fn render(rows: &[Row], stopped: Option<&Stop>, dry_run: bool) -> String {
    if rows.is_empty() {
        return "No usage linked to a task in this period.".into();
    }
    let many = rows
        .iter()
        .map(|r| &r.workspace)
        .collect::<BTreeSet<_>>()
        .len()
        > 1;
    let mut lines: Vec<String> = rows
        .iter()
        .map(|r| {
            let name = if many {
                format!("{}/{}", r.workspace, r.task_ref)
            } else {
                r.task_ref.clone()
            };
            let reason = r.reason.as_deref().map_or(String::new(), |why| {
                if r.outcome == Outcome::Stale {
                    format!(" ({why})")
                } else {
                    format!(" ({})", sanitize_line(why))
                }
            });
            format!("{name:<14}  {}{reason}", r.outcome.label())
        })
        .collect();
    lines.push(String::new());
    lines.push(summary_text(rows));
    if let Some(s) = stopped {
        lines.push(format!("Stopped: {}", sanitize_line(&s.message)));
        if let Some(h) = &s.hint {
            lines.push(format!("hint: {h}"));
        }
    }
    if dry_run {
        for r in rows.iter().filter(|r| r.aggregate.is_some()) {
            lines.push(String::new());
            lines.push(format!("{}/{}", r.workspace, r.task_ref));
            lines.push(
                serde_json::to_string_pretty(r.aggregate.as_ref().expect("filtered above"))
                    .unwrap_or_default(),
            );
        }
    }
    lines.join("\n")
}

fn on(ctx: &Ctx, yes_auto: bool) -> Result<Output> {
    if !interact::is_interactive() {
        return Err(
            Error::needs_confirmation("turning on uploads needs a terminal")
                .hint("run `tmgr usage sync on` in a terminal: this never accepts --yes"),
        );
    }
    let mut u = Usage::open()?;
    if let Some(at) = u.store.sync_consent().map_err(fail)? {
        return Ok(Output::new(
            json!({"action": "on", "enabled": true, "consent_at": at.to_rfc3339_opts(SecondsFormat::Secs, true)}),
            "Uploading is already on. `tmgr usage sync off` turns it off.",
        ));
    }
    let (c, prepared) = collect(ctx, &mut u, 30, false, None)?;
    let cloud: Vec<(&Prepared, &Ready)> = prepared
        .iter()
        .filter_map(|p| match &p.state {
            State::Ready(r) => Some((p, r.as_ref())),
            _ => None,
        })
        .collect();
    let newest = cloud
        .iter()
        .max_by_key(|(_, r)| r.value["period"]["to"].as_str().unwrap_or("").to_string());
    let Some((sample_task, sample)) = newest else {
        return Err(Error::usage(
            "no usage is linked to a task of a cloud workspace yet, so there is no sample to show",
        )
        .hint("work on a task with `tmgr agent start`, then try again; `tmgr usage task REF` shows what is linked"));
    };
    let workspaces: BTreeSet<&str> = cloud.iter().map(|(p, _)| p.workspace.as_str()).collect();
    eprintln!(
        "Turning this on lets `tmgr usage sync` upload one summary per task to TMGR.\n\n\
         Sent: AI cost (API-equivalent USD), token counts, time, the tree of agents (agent type, model, tool names and counts) and the names of skills, plugins, MCP servers and hooks used.\n\
         Never sent: prompts, replies, file contents, paths, command lines, URLs, branch names or session ids.\n\n\
         Who can see it: every member of the task's workspace sees the AI cost, the tree and the tools. Workspaces with linked usage now: {}.\n\n\
         Nothing is uploaded automatically unless you also confirm automatic uploads at the end (default no); otherwise you run `tmgr usage sync` yourself. The server setting \"Agent usage sync\" (TMGR Settings → Agent work) must be on too, and local workspaces are never uploaded.\n\n\
         This is exactly what would be sent for your most recent task ({}/{}):\n",
        workspaces.iter().copied().collect::<Vec<_>>().join(", "),
        sample_task.workspace,
        sample_task.task_ref,
    );
    eprintln!(
        "{}\n",
        serde_json::to_string_pretty(&sample.value).unwrap_or_default()
    );
    interact::confirm_typed("Type yes to turn on uploading: ", "yes")?;
    let now: DateTime<Utc> = Utc::now();
    u.store.clear_sync_auto().map_err(fail)?;
    u.store.set_sync_consent(now).map_err(fail)?;
    let _ = c;
    let auto = if yes_auto {
        eprintln!("{AUTO_DISCLAIMER}");
        true
    } else {
        eprintln!("\n{AUTO_DISCLAIMER}");
        interact::confirm(AUTO_QUESTION, false, false).is_ok()
    };
    if auto {
        u.store.set_sync_auto(now).map_err(fail)?;
    }
    let human = if auto {
        "Uploading is on, and so are automatic uploads after each agent run (`tmgr usage sync auto off` stops them)."
    } else {
        "Uploading is on. Run `tmgr usage sync --dry-run` to preview and `tmgr usage sync` to upload. Automatic uploads are off (`tmgr usage sync auto on` turns them on)."
    };
    Ok(Output::new(
        json!({"action": "on", "enabled": true, "auto": auto, "consent_at": now.to_rfc3339_opts(SecondsFormat::Secs, true)}),
        human,
    ))
}

fn confirm_auto(interactive: bool, yes_auto: bool, ask: impl FnOnce() -> Result<()>) -> Result<()> {
    if yes_auto {
        return Ok(());
    }
    if !interactive {
        return Err(
            Error::needs_confirmation("automatic uploads need a confirmation")
                .hint("run `tmgr usage sync auto on` in a terminal, or pass --yes-auto"),
        );
    }
    ask()
}

fn auto_json(on: bool, at: Option<DateTime<Utc>>) -> Value {
    json!({
        "action": "auto",
        "auto": on,
        "auto_at": at.map(|t| t.to_rfc3339_opts(SecondsFormat::Secs, true)),
    })
}

fn auto_on(yes_auto: bool) -> Result<Output> {
    let u = Usage::open()?;
    if u.store.sync_consent().map_err(fail)?.is_none() {
        return Err(Error::usage("uploading usage is not turned on")
            .hint("run `tmgr usage sync on` in a terminal first"));
    }
    if let Some(at) = u.store.sync_auto().map_err(fail)? {
        return Ok(Output::new(
            auto_json(true, Some(at)),
            "Automatic uploads are already on. `tmgr usage sync auto off` turns them off.",
        ));
    }
    eprintln!("{AUTO_DISCLAIMER}");
    confirm_auto(interact::is_interactive(), yes_auto, || {
        interact::confirm(AUTO_QUESTION, false, false)
    })?;
    let now = Utc::now();
    u.store.set_sync_auto(now).map_err(fail)?;
    Ok(Output::new(
        auto_json(true, Some(now)),
        "Automatic uploads are on: after each agent run the task's summary is uploaded. `tmgr usage sync auto off` stops it.",
    ))
}

fn auto_off() -> Result<Output> {
    let u = Usage::open()?;
    u.store.clear_sync_auto().map_err(fail)?;
    Ok(Output::new(
        auto_json(false, None),
        "Automatic uploads are off. `tmgr usage sync` still uploads when you run it. What was uploaded stays until you run `tmgr usage sync forget`.",
    ))
}

fn auto_status() -> Result<Output> {
    let u = Usage::open()?;
    let on = u.store.sync_auto_active().map_err(fail)?;
    let at = u.store.sync_auto().map_err(fail)?.filter(|_| on);
    let human = match at {
        Some(at) => format!(
            "Automatic uploads: on since {}",
            at.format("%Y-%m-%d %H:%M UTC")
        ),
        None => "Automatic uploads: off (`tmgr usage sync auto on` turns them on)".into(),
    };
    Ok(Output::new(auto_json(on, at), human))
}

fn off() -> Result<Output> {
    let u = Usage::open()?;
    u.store.clear_sync_consent().map_err(fail)?;
    Ok(Output::new(
        json!({"action": "off", "enabled": false, "auto": false}),
        "Uploading is off.\n\
         What you uploaded earlier stays on the server until the Agent usage sync setting is turned off (that deletes it) or you run `tmgr usage sync forget`.\n\
         Run `tmgr usage sync forget` before `tmgr usage reset`: reset clears the local upload history but keeps this install's source id, so afterwards a smaller upload could replace the fuller one on the server.",
    ))
}

fn status() -> Result<Output> {
    let u = Usage::open()?;
    let consent = u.store.sync_consent().map_err(fail)?;
    let last: Option<Value> = u
        .store
        .sync_last_run()
        .map_err(fail)?
        .and_then(|s| serde_json::from_str(&s).ok());
    let tracked = u.store.ledger_count().map_err(fail)?;
    let auto = u.store.sync_auto_active().map_err(fail)?;
    let mut lines = vec![match consent {
        Some(at) => format!("Uploading: on since {}", at.format("%Y-%m-%d %H:%M UTC")),
        None => "Uploading: off (`tmgr usage sync on` turns it on)".into(),
    }];
    if let Some(l) = &last {
        let counts = ALL
            .iter()
            .filter_map(|o| {
                let n = l["counts"][o.name()].as_u64().unwrap_or(0);
                (n > 0).then(|| format!("{n} {}", o.label()))
            })
            .collect::<Vec<_>>()
            .join(", ");
        lines.push(format!(
            "Last run: {} ({})",
            l["at"].as_str().unwrap_or("unknown time"),
            if counts.is_empty() {
                "nothing to upload"
            } else {
                &counts
            }
        ));
    } else {
        lines.push("Last run: never".into());
    }
    lines.push(format!("Tasks uploaded: {tracked}"));
    lines.push(
        if auto {
            "Automatic uploads: on (after each agent run; `tmgr usage sync auto off` turns them off)"
        } else {
            "Automatic uploads: off (`tmgr usage sync auto on` turns them on)"
        }
        .into(),
    );
    lines.push("The server setting \"Agent usage sync\" must be on too.".into());
    Ok(Output::new(
        json!({
            "action": "status",
            "enabled": consent.is_some(),
            "auto": auto,
            "consent_at": consent.map(|t| t.to_rfc3339_opts(SecondsFormat::Secs, true)),
            "last_run": last,
            "tasks_uploaded": tracked,
        }),
        lines.join("\n"),
    ))
}

fn forget(ctx: &Ctx, task: Option<&str>) -> Result<Output> {
    let client = ctx.cloud_client()?;
    let target = match task {
        Some(reference) => {
            let t = resolve_task(ctx, client, reference)?;
            Some((t.id, sanitize_line(reference)))
        }
        None => None,
    };
    let question = match &target {
        Some((_, r)) => format!("Delete your uploaded usage of {r} from the server?"),
        None => "Delete all your uploaded usage from the server?".to_string(),
    };
    interact::confirm(&question, true, ctx.yes)?;
    let path = match &target {
        Some((id, _)) => format!("/api/tasks/{id}/agent-usage"),
        None => PUT_PATH.to_string(),
    };
    let (status, body) = client.call("DELETE", &path, None, true)?;
    match status {
        200..=299 => {}
        403 => {
            let stop = stop_for(&refusal(&body));
            let mut e = Error::new(Kind::Forbidden, stop.message);
            if let Some(h) = stop.hint {
                e = e.hint(h);
            }
            return Err(e);
        }
        s => {
            let mut e = tmgr_core::api::map_error(s, &body, SCOPE);
            e.message = failure_reason(s, &body);
            return Err(e);
        }
    }
    let u = Usage::open()?;
    match &target {
        Some((id, r)) => {
            let plain = r.rsplit('/').next().unwrap_or(r);
            u.store
                .ledger_forget_task(Some(plain), Some(*id))
                .map_err(fail)?;
        }
        None => u.store.ledger_clear().map_err(fail)?,
    }
    let what = target
        .as_ref()
        .map_or("all tasks".to_string(), |(_, r)| r.clone());
    Ok(Output::new(
        json!({"action": "forget", "scope": if target.is_some() { "task" } else { "all" }, "task": target.as_ref().map(|(_, r)| r)}),
        format!(
            "Deleted your uploaded usage of {what} from the server. The local upload record was cleared too, so the next sync sends it again if uploading is on."
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn coverage_falls_back_to_facts_when_there_are_no_events() {
        let t = |s: &str| s.parse::<DateTime<Utc>>().unwrap();
        let (a, b) = (t("2026-10-16T10:00:00Z"), t("2026-10-16T11:00:00Z"));
        assert_eq!(
            span(vec![b, a], vec![]),
            (
                Some("2026-10-16T10:00:00Z".into()),
                Some("2026-10-16T11:00:00Z".into())
            )
        );
        assert_eq!(
            span(vec![], vec![b, a]),
            (
                Some("2026-10-16T10:00:00Z".into()),
                Some("2026-10-16T11:00:00Z".into())
            )
        );
        assert_eq!(span(vec![], vec![]), (None, None));
    }

    #[test]
    fn pace_waits_only_after_the_window_is_full() {
        let t0 = Instant::now();
        let mut p = Pace::new(Duration::from_secs(60));
        for i in 0..PUTS_PER_WINDOW {
            assert_eq!(
                p.delay(t0 + Duration::from_millis(i as u64)),
                Duration::ZERO
            );
            p.record(t0 + Duration::from_millis(i as u64));
        }
        let wait = p.delay(t0 + Duration::from_secs(10));
        assert_eq!(wait, Duration::from_secs(50));
        assert_eq!(p.delay(t0 + Duration::from_secs(61)), Duration::ZERO);
    }

    #[test]
    fn auto_needs_a_terminal_or_the_flag() {
        let asked = std::cell::Cell::new(false);
        let ask = || {
            asked.set(true);
            Ok(())
        };
        let e = confirm_auto(false, false, ask).unwrap_err();
        assert_eq!(e.kind, Kind::NeedsConfirmation);
        assert!(!asked.get());
        assert!(confirm_auto(false, true, || panic!("no prompt with the flag")).is_ok());
        assert!(confirm_auto(true, true, || panic!("no prompt with the flag")).is_ok());
        assert!(confirm_auto(true, false, ask).is_ok());
        assert!(asked.get());
        let no = || Err(Error::new(Kind::Interrupted, "cancelled"));
        assert!(confirm_auto(true, false, no).is_err());
    }

    #[test]
    fn auto_lock_is_exclusive_released_on_drop_and_expires() {
        let dir = std::env::temp_dir().join(format!("tmgr-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = AutoLock::take(&dir, 7).unwrap();
        assert!(AutoLock::take(&dir, 7).is_none());
        assert!(AutoLock::take(&dir, 8).is_some());
        drop(first);
        let again = AutoLock::take(&dir, 7).unwrap();
        let old = std::time::SystemTime::now() - Duration::from_secs(700);
        std::fs::File::options()
            .write(true)
            .open(&again.0)
            .unwrap()
            .set_modified(old)
            .unwrap();
        let stolen = AutoLock::take(&dir, 7);
        assert!(stolen.is_some());
        std::mem::forget(again);
        drop(stolen);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn digest_ignores_generated_at_and_source_id() {
        let a = json!({"generated_at": "1", "source_id": "a", "x": 1});
        let b = json!({"generated_at": "2", "source_id": "b", "x": 1});
        let c = json!({"generated_at": "1", "source_id": "a", "x": 2});
        assert_eq!(digest_of(&a), digest_of(&b));
        assert_ne!(digest_of(&a), digest_of(&c));
    }

    #[test]
    fn forbidden_bodies_map_to_their_refusals() {
        assert!(matches!(
            refusal(&json!({"error": "feature_disabled"})),
            Refusal::WorkspaceOff
        ));
        let stop = stop_for(&refusal(&json!({"error": "agent_usage_sync_disabled"})));
        assert!(stop.hint.unwrap().contains("Settings"));
        let stop = stop_for(&refusal(&json!({"error": "insufficient_scope"})));
        assert_eq!(
            stop.hint.unwrap(),
            "tmgr login --scopes workspaces:read,tasks:read,tasks:write,tasks:create,comments:create,time:read,time:write,agent-usage:write"
        );
    }
}
