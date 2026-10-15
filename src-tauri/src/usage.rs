use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, SecondsFormat, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use tmgr_usage::error::UsageError;
use tmgr_usage::limits::LimitWindow;
use tmgr_usage::link::{attribute_at, Link, RunWindow, Totals as LinkTotals};
use tmgr_usage::task::{build, facts_as_events, TaskIdent, TaskInput, TaskUsageAggregate};
use tmgr_usage::paths::{self, Env};
use tmgr_usage::refresh::refresh;
use tmgr_usage::{GroupBy, Query, Row as AggRow, Store, Tz};

const SNAP_REASON: &str = "AI usage is not available in the Snap build, which cannot read other apps' logs.";
const MAX_RUNS: usize = 5000;
const MONTH_DAYS: i64 = 30;

static SCANNING: AtomicBool = AtomicBool::new(false);

struct ScanGuard<'a>(&'a AtomicBool);

impl<'a> ScanGuard<'a> {
  fn acquire(flag: &'a AtomicBool) -> Option<Self> {
    flag
      .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
      .ok()
      .map(|_| ScanGuard(flag))
  }
}

impl Drop for ScanGuard<'_> {
  fn drop(&mut self) {
    self.0.store(false, Ordering::Release);
  }
}

fn user_error(e: UsageError) -> String {
  match e {
    UsageError::SchemaTooNew { .. } => "The usage data was written by a newer TMGR. Update the desktop app.".into(),
    UsageError::Invalid(_) => "The usage data folder could not be found.".into(),
    _ => "The usage data could not be read.".into(),
  }
}

fn snap() -> bool {
  std::env::var_os("SNAP").is_some()
}

fn open(env: &Env) -> Result<(std::path::PathBuf, Store), String> {
  let dir = paths::ensure_data_dir(env).map_err(user_error)?;
  let store = Store::open(&paths::db_path(&dir)).map_err(user_error)?;
  Ok((dir, store))
}

fn round6(v: f64) -> f64 {
  (v * 1_000_000.0).round() / 1_000_000.0
}

fn stamp(t: DateTime<Utc>) -> String {
  t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn day_bounds(date: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
  let at = |d: NaiveDate| {
    let mut naive = d.and_hms_opt(0, 0, 0).expect("midnight exists");
    for _ in 0..4 {
      if let Some(t) = Local.from_local_datetime(&naive).earliest() {
        return t.with_timezone(&Utc);
      }
      naive += Duration::hours(1);
    }
    Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).expect("midnight exists"))
  };
  (at(date), at(date.succ_opt().unwrap_or(date)))
}

fn local_today(now: DateTime<Utc>) -> NaiveDate {
  Tz::Local.day(now)
}

#[derive(Serialize, Debug)]
pub struct ScanOut {
  pub state: &'static str,
  pub files: u64,
  pub events_new: u64,
  pub events_updated: u64,
  pub errors: u64,
  pub bad_lines: u64,
}

fn scan_inner(flag: &AtomicBool, env: &Env, now: DateTime<Utc>) -> Result<ScanOut, String> {
  let Some(_guard) = ScanGuard::acquire(flag) else {
    return Ok(ScanOut { state: "busy", files: 0, events_new: 0, events_updated: 0, errors: 0, bad_lines: 0 });
  };
  let (dir, mut store) = open(env)?;
  let r = refresh(&mut store, &dir, env, now).map_err(user_error)?;
  Ok(ScanOut {
    state: "done",
    files: r.files,
    events_new: r.events_new,
    events_updated: r.events_updated,
    errors: r.errors,
    bad_lines: r.bad_lines,
  })
}

#[derive(Serialize, Debug, Default, PartialEq)]
pub struct Row {
  pub cost_usd: f64,
  pub tokens_total: u64,
  pub input: u64,
  pub output: u64,
  pub cache_read: u64,
  pub cache_write: u64,
  pub reasoning: u64,
  pub events: u64,
  pub unpriced_events: u64,
}

fn row_of(r: &AggRow) -> Row {
  let t = &r.tokens;
  let cache_write = t.cache_write_5m + t.cache_write_1h;
  Row {
    cost_usd: round6(r.cost_usd),
    tokens_total: t.input + t.output + t.cache_read + cache_write,
    input: t.input,
    output: t.output,
    cache_read: t.cache_read,
    cache_write,
    reasoning: t.reasoning,
    events: r.events,
    unpriced_events: r.unpriced_events,
  }
}

#[derive(Serialize, Debug)]
pub struct ModelRow {
  pub tool: String,
  pub model: String,
  #[serde(flatten)]
  pub row: Row,
}

#[derive(Serialize, Debug)]
pub struct WindowOut {
  pub key: &'static str,
  pub label: &'static str,
  pub from: String,
  pub to: String,
  pub totals: Row,
  pub rows: Vec<ModelRow>,
}

#[derive(Serialize, Debug)]
pub struct DayOut {
  pub day: String,
  pub tool: String,
  pub cost_usd: f64,
  pub tokens_total: u64,
}

#[derive(Serialize, Debug)]
pub struct AccountOut {
  pub tool: String,
  pub label: String,
}

#[derive(Serialize, Debug)]
pub struct LimitOut {
  pub tool: String,
  pub account: String,
  pub window: String,
  pub window_label: String,
  pub used_pct: f64,
  pub resets_at: Option<String>,
  pub resets_in_s: Option<i64>,
  pub plan: Option<String>,
  pub source: &'static str,
  pub observed_at: String,
  pub age_s: i64,
}

#[derive(Serialize, Debug, Default)]
pub struct OverviewOut {
  pub supported: bool,
  pub unsupported_reason: Option<String>,
  pub has_data: bool,
  pub history_floor: Option<String>,
  pub accounts: Vec<AccountOut>,
  pub windows: Vec<WindowOut>,
  pub days: Vec<DayOut>,
  pub limits: Vec<LimitOut>,
}

fn window_label(w: &LimitWindow) -> String {
  match w {
    LimitWindow::Session => "Session".into(),
    LimitWindow::Weekly => "Weekly".into(),
    LimitWindow::Model(m) => format!("Weekly {m}"),
  }
}

fn query(from: DateTime<Utc>, to: DateTime<Utc>, group_by: Vec<GroupBy>) -> Query {
  Query { from, to, tz: Tz::Local, group_by, filters: Default::default() }
}

fn overview_inner(env: &Env, now: DateTime<Utc>) -> Result<OverviewOut, String> {
  let (dir, store) = open(env)?;
  let today = local_today(now);
  let to = day_bounds(today).1;
  let specs: [(&'static str, &'static str, i64); 3] =
    [("today", "Today", 1), ("week", "Last 7 days", 7), ("month", "Last 30 days", MONTH_DAYS)];
  let mut windows = Vec::new();
  for (key, label, days) in specs {
    let from = day_bounds(today - Duration::days(days - 1)).0;
    let report = store.aggregate(&query(from, to, vec![GroupBy::Tool, GroupBy::Model])).map_err(user_error)?;
    let rows = report
      .rows
      .iter()
      .map(|r| ModelRow { tool: r.keys[0].clone(), model: r.keys[1].clone(), row: row_of(r) })
      .collect();
    windows.push(WindowOut { key, label, from: stamp(from), to: stamp(to), totals: row_of(&report.totals), rows });
  }
  let month_from = day_bounds(today - Duration::days(MONTH_DAYS - 1)).0;
  let by_day = store.aggregate(&query(month_from, to, vec![GroupBy::Day, GroupBy::Tool])).map_err(user_error)?;
  let days = by_day
    .rows
    .iter()
    .map(|r| {
      let row = row_of(r);
      DayOut { day: r.keys[0].clone(), tool: r.keys[1].clone(), cost_usd: row.cost_usd, tokens_total: row.tokens_total }
    })
    .collect();
  let limits = store
    .current_limits(now)
    .map_err(user_error)?
    .into_iter()
    .map(|l| LimitOut {
      tool: l.tool.as_str().to_string(),
      account: l.account,
      window: l.window.as_key(),
      window_label: window_label(&l.window),
      used_pct: l.used_pct,
      resets_at: l.resets_at.map(stamp),
      resets_in_s: l.resets_at.map(|t| (t - now).num_seconds().max(0)),
      plan: l.plan,
      source: l.source.as_str(),
      observed_at: stamp(l.observed_at),
      age_s: (now - l.observed_at).num_seconds().max(0),
    })
    .collect();
  let accounts = paths::all_accounts(&dir, env)
    .map_err(user_error)?
    .into_iter()
    .map(|a| AccountOut { tool: a.tool.as_str().to_string(), label: a.label })
    .collect();
  let history_floor = store.history_floor().map_err(user_error)?;
  let has_data = store.event_count().map_err(user_error)? > 0;
  Ok(OverviewOut {
    supported: true,
    unsupported_reason: None,
    has_data,
    history_floor: history_floor.map(stamp),
    accounts,
    windows,
    days,
    limits,
  })
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
enum Id {
  Num(i64),
  Text(String),
}

impl std::fmt::Display for Id {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Id::Num(n) => write!(f, "{n}"),
      Id::Text(s) => write!(f, "{s}"),
    }
  }
}

#[derive(Deserialize, Debug)]
pub struct RawRun {
  id: Id,
  task_id: Id,
  workspace: String,
  status: Option<String>,
  started_at: Option<String>,
  ended_at: Option<String>,
  duration_seconds: Option<i64>,
  session_id: Option<String>,
  branch: Option<String>,
}

#[derive(Serialize, Debug, Default, PartialEq)]
pub struct TotalsOut {
  pub cost_usd: f64,
  pub tokens_total: u64,
  pub events: u64,
  pub unpriced_events: u64,
}

fn totals_of(t: &LinkTotals) -> TotalsOut {
  TotalsOut { cost_usd: round6(t.cost_usd), tokens_total: t.tokens, events: t.events, unpriced_events: t.unpriced_events }
}

#[derive(Serialize, Debug)]
pub struct TaskOut {
  pub task_ref: String,
  #[serde(flatten)]
  pub totals: TotalsOut,
}

#[derive(Serialize, Debug)]
pub struct TasksOut {
  pub tasks: Vec<TaskOut>,
  pub ambiguous: TotalsOut,
  pub unattributed: TotalsOut,
  pub runs_used: usize,
  pub runs_dropped: usize,
  pub runs_truncated: bool,
}

fn parse_ts(raw: &str) -> Option<DateTime<Utc>> {
  DateTime::parse_from_rfc3339(raw)
    .map(|d| d.with_timezone(&Utc))
    .ok()
    .or_else(|| NaiveDateTime::parse_from_str(raw, "%Y-%m-%d %H:%M:%S").ok().map(|n| n.and_utc()))
}

fn non_empty(v: &Option<String>) -> Option<String> {
  v.clone().filter(|s| !s.is_empty())
}

fn window_of(run: &RawRun, now: DateTime<Utc>) -> Option<(DateTime<Utc>, Option<DateTime<Utc>>)> {
  let start = run.started_at.as_deref().and_then(parse_ts)?;
  let end = match run.ended_at.as_deref().and_then(parse_ts) {
    Some(e) => Some(e),
    None if run.status.as_deref() == Some("running") => None,
    None => Some(start + Duration::seconds(run.duration_seconds.unwrap_or(0).max(0))),
  };
  (start <= now).then_some((start, end))
}

fn run_windows(runs: &[RawRun], now: DateTime<Utc>) -> Vec<RunWindow> {
  let mut windows = Vec::new();
  for run in runs {
    let (session_id, branch) = (non_empty(&run.session_id), non_empty(&run.branch));
    if session_id.is_none() && branch.is_none() {
      continue;
    }
    let Some((started_at, ended_at)) = window_of(run, now) else {
      continue;
    };
    windows.push(RunWindow {
      run_ref: format!("{}#{}", run.workspace, run.id),
      task_ref: format!("{}/{}", run.workspace, run.task_id),
      workspace: run.workspace.clone(),
      session_id,
      branch,
      project_hash: None,
      started_at,
      ended_at,
    });
  }
  windows
}

fn tasks_inner(env: &Env, now: DateTime<Utc>, runs: &[RawRun]) -> Result<TasksOut, String> {
  let mut windows = run_windows(runs, now);
  windows.sort_by(|a, b| b.started_at.cmp(&a.started_at));
  let runs_truncated = windows.len() > MAX_RUNS;
  windows.truncate(MAX_RUNS);
  let (_, store) = open(env)?;
  let today = local_today(now);
  let from = day_bounds(today - Duration::days(MONTH_DAYS - 1)).0;
  let events = store.link_events(from, day_bounds(today).1).map_err(user_error)?;
  let report = attribute_at(&events, &windows, now);
  let mut per_task = report.per_task.clone();
  per_task.sort_by(|a, b| b.cost_usd.total_cmp(&a.cost_usd).then(a.key.cmp(&b.key)));
  let tasks = per_task.iter().map(|t| TaskOut { task_ref: t.key.clone(), totals: totals_of(t) }).collect();
  Ok(TasksOut {
    tasks,
    ambiguous: totals_of(&report.ambiguous_totals),
    unattributed: totals_of(&report.unattributed_totals),
    runs_used: windows.len(),
    runs_dropped: runs.len() - windows.len(),
    runs_truncated,
  })
}

#[derive(Serialize, Debug, Default)]
pub struct TaskUsageOut {
  pub supported: bool,
  pub unsupported_reason: Option<String>,
  pub aggregate: Option<TaskUsageAggregate>,
}

fn task_inner(env: &Env, now: DateTime<Utc>, runs: &[RawRun], workspace: &str, task_id: i64) -> Result<TaskUsageOut, String> {
  let windows = run_windows(runs, now);
  let task_ref = format!("{workspace}/{task_id}");
  let Some(first) = windows.iter().filter(|w| w.task_ref == task_ref).map(|w| w.started_at).min() else {
    return Ok(TaskUsageOut { supported: true, ..Default::default() });
  };
  let (from, to) = (first - Duration::days(1), now + Duration::minutes(1));
  let (_, store) = open(env)?;
  let (events, tokens): (Vec<_>, Vec<_>) = store.link_events_with_tokens(from, to).map_err(user_error)?.into_iter().unzip();
  let facts = store.facts(from, to).map_err(user_error)?;
  let agents = store.agents().map_err(user_error)?;
  let mine: HashSet<&str> = windows.iter().filter(|w| w.task_ref == task_ref).map(|w| w.run_ref.as_str()).collect();
  let matching = |links: &[Link]| -> Vec<usize> {
    links.iter().filter(|l| mine.contains(l.run_ref.as_str())).map(|l| l.event_index).collect()
  };
  let linked_events = matching(&attribute_at(&events, &windows, now).links);
  let linked_facts = matching(&attribute_at(&facts_as_events(&facts, &events), &windows, now).links);
  let aggregate = build(&TaskInput {
    source_id: store.install_id().map_err(user_error)?,
    generated_at: now,
    task: TaskIdent { workspace: workspace.to_string(), task_ref: format!("#{task_id}"), id: u64::try_from(task_id).ok() },
    from,
    to,
    events: &events,
    tokens: &tokens,
    linked_events: &linked_events,
    facts: &facts,
    linked_facts: &linked_facts,
    agents: &agents,
    limits: &store.current_limits(now).map_err(user_error)?,
  });
  Ok(TaskUsageOut { supported: true, unsupported_reason: None, aggregate })
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
  tokio::task::spawn_blocking(f).await.map_err(|_| "The usage data could not be read.".to_string())?
}

#[tauri::command]
pub async fn usage_scan() -> Result<ScanOut, String> {
  if snap() {
    return Err(SNAP_REASON.into());
  }
  blocking(|| scan_inner(&SCANNING, &Env::from_process(), Utc::now())).await
}

#[tauri::command]
pub async fn usage_overview() -> Result<OverviewOut, String> {
  if snap() {
    return Ok(OverviewOut { supported: false, unsupported_reason: Some(SNAP_REASON.into()), ..Default::default() });
  }
  blocking(|| overview_inner(&Env::from_process(), Utc::now())).await
}

#[tauri::command]
pub async fn usage_tasks(runs: Vec<RawRun>) -> Result<TasksOut, String> {
  if snap() {
    return Err(SNAP_REASON.into());
  }
  blocking(move || tasks_inner(&Env::from_process(), Utc::now(), &runs)).await
}

#[tauri::command]
pub async fn usage_task(runs: Vec<RawRun>, workspace: String, task_id: i64) -> Result<TaskUsageOut, String> {
  if snap() {
    return Ok(TaskUsageOut { supported: false, unsupported_reason: Some(SNAP_REASON.into()), aggregate: None });
  }
  blocking(move || task_inner(&Env::from_process(), Utc::now(), &runs, &workspace, task_id)).await
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::collections::HashMap;
  use std::path::Path;

  fn env_in(home: &Path) -> Env {
    Env { home: Some(home.to_path_buf()), vars: HashMap::new() }
  }

  fn local_noon() -> DateTime<Utc> {
    let day = Local::now().date_naive();
    Local.from_local_datetime(&day.and_hms_opt(12, 0, 0).unwrap()).earliest().unwrap().with_timezone(&Utc)
  }

  fn at(now: DateTime<Utc>, minutes_ago: i64) -> String {
    stamp(now - Duration::minutes(minutes_ago))
  }

  fn write(path: &Path, lines: &[String]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, lines.join("\n") + "\n").unwrap();
  }

  fn claude_log(home: &Path, now: DateTime<Utc>) {
    let line = |msg: &str, ago: i64, branch: &str| {
      format!(
        r#"{{"type":"assistant","sessionId":"sess-1","timestamp":"{}","cwd":"/work/alpha","gitBranch":"{branch}","isSidechain":false,"requestId":"req-{msg}","message":{{"id":"{msg}","model":"claude-test-opus-1","content":[{{"type":"text","text":"x"}}],"usage":{{"input_tokens":100,"output_tokens":20,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}}}}"#,
        at(now, ago)
      )
    };
    write(
      &home.join(".claude/projects/-work-alpha/sess-1.jsonl"),
      &[line("m1", 60, "feat/one"), line("m2", 50, "feat/one")],
    );
  }

  fn codex_log(home: &Path, now: DateTime<Utc>) {
    let reset = (now + Duration::hours(2)).timestamp();
    let lines = vec![
      format!(r#"{{"timestamp":"{}","type":"session_meta","payload":{{"id":"cx1","session_id":"cx1","cwd":"/work/beta","cli_version":"0.150.0","git":{{"branch":"main"}}}}}}"#, at(now, 40)),
      format!(r#"{{"timestamp":"{}","type":"turn_context","payload":{{"model":"gpt-test","cwd":"/work/beta"}}}}"#, at(now, 39)),
      format!(
        r#"{{"timestamp":"{}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":100,"cached_input_tokens":40,"output_tokens":10,"reasoning_output_tokens":4,"total_tokens":110}},"last_token_usage":{{"input_tokens":100,"cached_input_tokens":40,"output_tokens":10,"reasoning_output_tokens":4,"total_tokens":110}}}},"rate_limits":{{"plan_type":"pro","primary":{{"used_percent":42.0,"window_minutes":300,"resets_at":{reset}}},"secondary":null}}}}}}"#,
        at(now, 38)
      ),
    ];
    write(&home.join(".codex/sessions/2026/10/14/rollout-cx1.jsonl"), &lines);
  }

  fn raw(id: i64, task: i64, session: Option<&str>, branch: Option<&str>, now: DateTime<Utc>) -> RawRun {
    RawRun {
      id: Id::Num(id),
      task_id: Id::Num(task),
      workspace: "ws".into(),
      status: Some("succeeded".into()),
      started_at: Some(at(now, 120)),
      ended_at: Some(at(now, 10)),
      duration_seconds: None,
      session_id: session.map(String::from),
      branch: branch.map(String::from),
    }
  }

  #[test]
  fn overview_shows_scanned_tools_models_windows_and_limits() {
    let tmp = tempfile::tempdir().unwrap();
    let env = env_in(tmp.path());
    let now = local_noon();
    claude_log(tmp.path(), now);
    codex_log(tmp.path(), now);
    let flag = AtomicBool::new(false);
    let scan = scan_inner(&flag, &env, now).unwrap();
    assert_eq!(scan.state, "done");
    assert_eq!(scan.events_new, 3);
    assert!(paths::data_dir(&env).unwrap().starts_with(tmp.path()));
    assert!(!flag.load(Ordering::Acquire));

    let ov = overview_inner(&env, now).unwrap();
    assert!(ov.supported && ov.has_data);
    assert_eq!(ov.accounts.len(), 2);
    let keys: Vec<_> = ov.windows.iter().map(|w| w.key).collect();
    assert_eq!(keys, ["today", "week", "month"]);
    let today = &ov.windows[0];
    assert_eq!(today.totals.events, 3);
    let pairs: Vec<_> = today.rows.iter().map(|r| (r.tool.as_str(), r.model.as_str())).collect();
    assert!(pairs.contains(&("claude", "claude-test-opus-1")) && pairs.contains(&("codex", "gpt-test")));
    assert!(ov.days.iter().any(|d| d.tool == "codex" && d.tokens_total > 0));
    assert_eq!(ov.limits.len(), 1);
    let l = &ov.limits[0];
    assert_eq!((l.tool.as_str(), l.window.as_str(), l.source), ("codex", "session", "rollout"));
    assert_eq!(l.used_pct, 42.0);
    assert!(l.resets_in_s.unwrap() > 0);
    let text = serde_json::to_string(&ov).unwrap();
    assert!(!text.contains("sess-1") && !text.contains("alpha") && !text.contains(&*tmp.path().to_string_lossy()));
  }

  #[test]
  fn empty_home_has_no_data() {
    let tmp = tempfile::tempdir().unwrap();
    let env = env_in(tmp.path());
    let now = local_noon();
    let scan = scan_inner(&AtomicBool::new(false), &env, now).unwrap();
    assert_eq!((scan.state, scan.files), ("done", 0));
    let ov = overview_inner(&env, now).unwrap();
    assert!(ov.supported && !ov.has_data);
    assert!(ov.accounts.is_empty() && ov.limits.is_empty());
    assert!(ov.windows.iter().all(|w| w.totals.events == 0 && w.rows.is_empty()));
  }

  #[test]
  fn unknown_home_gives_a_fixed_sentence() {
    let err = overview_inner(&Env::default(), local_noon()).unwrap_err();
    assert_eq!(err, "The usage data folder could not be found.");
  }

  #[test]
  fn second_scan_is_busy_and_the_guard_releases() {
    let tmp = tempfile::tempdir().unwrap();
    let env = env_in(tmp.path());
    let flag = AtomicBool::new(false);
    let held = ScanGuard::acquire(&flag).unwrap();
    let busy = scan_inner(&flag, &env, local_noon()).unwrap();
    assert_eq!((busy.state, busy.files), ("busy", 0));
    drop(held);
    assert_eq!(scan_inner(&flag, &env, local_noon()).unwrap().state, "done");
  }

  #[test]
  fn schema_errors_are_readable() {
    let e = UsageError::SchemaTooNew { found: 9, supported: 1 };
    assert_eq!(user_error(e), "The usage data was written by a newer TMGR. Update the desktop app.");
    assert_eq!(user_error(UsageError::Db("secret path".into())), "The usage data could not be read.");
  }

  #[test]
  fn tasks_link_by_session_id_and_drop_runs_without_keys() {
    let tmp = tempfile::tempdir().unwrap();
    let env = env_in(tmp.path());
    let now = local_noon();
    claude_log(tmp.path(), now);
    scan_inner(&AtomicBool::new(false), &env, now).unwrap();

    let runs = vec![raw(1, 7, Some("sess-1"), None, now), raw(2, 8, None, None, now)];
    let out = tasks_inner(&env, now, &runs).unwrap();
    assert_eq!((out.runs_used, out.runs_dropped), (1, 1));
    assert!(!out.runs_truncated);
    assert_eq!(out.tasks.len(), 1);
    assert_eq!(out.tasks[0].task_ref, "ws/7");
    assert_eq!(out.tasks[0].totals.events, 2);
    assert_eq!(out.tasks[0].totals.tokens_total, 240);
    assert_eq!(out.unattributed.events, 0);
  }

  #[test]
  fn tasks_leave_unmatched_usage_unattributed_and_skip_future_runs() {
    let tmp = tempfile::tempdir().unwrap();
    let env = env_in(tmp.path());
    let now = local_noon();
    claude_log(tmp.path(), now);
    scan_inner(&AtomicBool::new(false), &env, now).unwrap();

    let mut future = raw(3, 9, Some("sess-1"), None, now);
    future.started_at = Some(stamp(now + Duration::hours(1)));
    let out = tasks_inner(&env, now, &[raw(4, 5, Some("other"), None, now), future]).unwrap();
    assert_eq!((out.runs_used, out.runs_dropped), (1, 1));
    assert!(out.tasks.is_empty());
    assert_eq!(out.unattributed.events, 2);
  }

  #[test]
  fn run_cap_keeps_the_newest_eligible_runs_and_reports_truncation() {
    let tmp = tempfile::tempdir().unwrap();
    let env = env_in(tmp.path());
    let now = local_noon();
    claude_log(tmp.path(), now);
    scan_inner(&AtomicBool::new(false), &env, now).unwrap();

    let mut runs = vec![raw(1, 7, Some("sess-1"), None, now)];
    runs[0].started_at = Some(at(now, 600));
    for i in 0..MAX_RUNS as i64 {
      let mut r = raw(100 + i, 8, Some("other"), None, now);
      r.started_at = Some(at(now, 500 - (i % 400)));
      runs.push(r);
    }
    runs.push(raw(9000, 9, None, None, now));
    let out = tasks_inner(&env, now, &runs).unwrap();
    assert_eq!(out.runs_used, MAX_RUNS);
    assert!(out.runs_truncated);
    assert!(out.tasks.iter().all(|t| t.task_ref != "ws/7"));
    assert_eq!(out.unattributed.events, 2);
  }

  #[test]
  fn tasks_command_reports_the_snap_reason() {
    std::env::set_var("SNAP", "1");
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    let err = rt.block_on(usage_tasks(Vec::new())).unwrap_err();
    std::env::remove_var("SNAP");
    assert_eq!(err, SNAP_REASON);
  }

  #[test]
  fn running_run_has_an_open_window() {
    let now = local_noon();
    let mut run = raw(1, 1, Some("s"), None, now);
    run.ended_at = None;
    run.status = Some("running".into());
    assert_eq!(window_of(&run, now).unwrap().1, None);
  }

  #[test]
  fn task_usage_links_by_session_id() {
    let tmp = tempfile::tempdir().unwrap();
    let env = env_in(tmp.path());
    let now = local_noon();
    claude_log(tmp.path(), now);
    scan_inner(&AtomicBool::new(false), &env, now).unwrap();
    let runs = vec![raw(1, 7, Some("sess-1"), None, now), raw(2, 8, Some("other"), None, now)];
    let out = task_inner(&env, now, &runs, "ws", 7).unwrap();
    assert!(out.supported);
    let a = out.aggregate.unwrap();
    assert_eq!(a.totals.events, 2);
    assert_eq!(a.totals.tokens.input, 200);
    assert_eq!(a.task.task_ref, "#7");
    assert_eq!(a.task.id, Some(7));
    assert!(!a.nodes.is_empty());
  }

  #[test]
  fn task_usage_excludes_events_won_by_a_competing_run() {
    let tmp = tempfile::tempdir().unwrap();
    let env = env_in(tmp.path());
    let now = local_noon();
    claude_log(tmp.path(), now);
    scan_inner(&AtomicBool::new(false), &env, now).unwrap();
    let mut mine = raw(1, 7, None, Some("feat/one"), now);
    mine.started_at = Some(at(now, 120));
    mine.ended_at = Some(at(now, 55));
    let mut other = raw(2, 8, None, Some("feat/one"), now);
    other.started_at = Some(at(now, 54));
    other.ended_at = Some(at(now, 10));
    let runs = vec![mine, other];
    let a7 = task_inner(&env, now, &runs, "ws", 7).unwrap().aggregate;
    let a8 = task_inner(&env, now, &runs, "ws", 8).unwrap().aggregate;
    assert_eq!(a7.as_ref().map_or(0, |a| a.totals.events), 1);
    assert_eq!(a8.as_ref().map_or(0, |a| a.totals.events), 1);
  }

  #[test]
  fn task_usage_without_runs_has_no_aggregate() {
    let tmp = tempfile::tempdir().unwrap();
    let env = env_in(tmp.path());
    let now = local_noon();
    claude_log(tmp.path(), now);
    scan_inner(&AtomicBool::new(false), &env, now).unwrap();
    let out = task_inner(&env, now, &[raw(2, 8, Some("sess-1"), None, now)], "ws", 7).unwrap();
    assert!(out.supported);
    assert!(out.aggregate.is_none());
  }

  #[test]
  fn task_usage_command_reports_the_snap_reason() {
    std::env::set_var("SNAP", "1");
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    let out = rt.block_on(usage_task(Vec::new(), "ws".into(), 1)).unwrap();
    std::env::remove_var("SNAP");
    assert!(!out.supported);
    assert_eq!(out.unsupported_reason.as_deref(), Some(SNAP_REASON));
  }
}
