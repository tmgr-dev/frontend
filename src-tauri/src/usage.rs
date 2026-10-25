use chrono::{DateTime, Duration, Local, NaiveDate, NaiveDateTime, SecondsFormat, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use tmgr_usage::consent::{forget, ConsentChange, ReadLogs, FORGET_WAIT};
use tmgr_usage::error::UsageError;
use tmgr_usage::limits::LimitWindow;
use tmgr_usage::pricing::PriceEntry;
use tmgr_usage::bind::{facts_from, Linker, RunIds};
use tmgr_usage::link::{Link, RunWindow, Totals as LinkTotals};
use tmgr_usage::task::{build, facts_as_events, TaskIdent, TaskInput, TaskUsageAggregate};
use tmgr_usage::paths::{self, Env};
use tmgr_usage::refresh::refresh;
use tmgr_usage::{GroupBy, PriceBook, Query, Row as AggRow, Store, Tokens, Tz};

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
  if !store.log_consent().map_err(user_error)?.allows_any(false) {
    return Ok(ScanOut { state: "off", files: 0, events_new: 0, events_updated: 0, errors: 0, bad_lines: 0 });
  }
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

#[derive(Serialize, Debug)]
pub struct FolderOut {
  pub tool: &'static str,
  pub label: String,
  pub path: String,
}

#[derive(Serialize, Debug)]
pub struct UsageConsentOut {
  pub supported: bool,
  pub unsupported_reason: Option<String>,
  pub state: &'static str,
  pub claude: bool,
  pub codex: bool,
  pub decided_at: Option<String>,
  pub folders: Vec<FolderOut>,
  pub has_data: bool,
}

fn shown_path(path: &std::path::Path, home: Option<&std::path::Path>) -> String {
  if let Some(home) = home.filter(|h| h.as_os_str().len() > 1) {
    for base in [home.to_path_buf(), paths::canonical(home)] {
      for p in [path.to_path_buf(), paths::canonical(path)] {
        if let Ok(rest) = p.strip_prefix(&base) {
          return if rest.as_os_str().is_empty() { "~".into() } else { format!("~/{}", rest.display()) };
        }
      }
    }
  }
  path.display().to_string()
}

fn consent_out(env: &Env, dir: &std::path::Path, store: &Store) -> Result<UsageConsentOut, String> {
  let c = store.log_consent().map_err(user_error)?;
  let folders = paths::all_accounts(dir, env)
    .map_err(user_error)?
    .into_iter()
    .map(|a| FolderOut { tool: a.tool.as_str(), path: shown_path(&a.root, env.home.as_deref()), label: a.label })
    .collect();
  Ok(UsageConsentOut {
    supported: true,
    unsupported_reason: None,
    state: c.state.as_str(),
    claude: c.claude,
    codex: c.codex,
    decided_at: c.decided_at.map(stamp),
    folders,
    has_data: store.event_count().map_err(user_error)? > 0,
  })
}

fn consent_inner(env: &Env) -> Result<UsageConsentOut, String> {
  let (dir, store) = open(env)?;
  consent_out(env, &dir, &store)
}

fn consent_set_inner(
  env: &Env,
  now: DateTime<Utc>,
  state: Option<&str>,
  claude: Option<bool>,
  codex: Option<bool>,
) -> Result<UsageConsentOut, String> {
  let state = match state {
    None => None,
    Some("granted") => Some(ReadLogs::Granted),
    Some("denied") => Some(ReadLogs::Denied),
    Some(_) => return Err("Unknown choice.".into()),
  };
  let (dir, store) = open(env)?;
  store.change_log_consent(&ConsentChange { state, claude, codex }, now).map_err(user_error)?;
  consent_out(env, &dir, &store)
}

const BUSY: &str = "A scan is running. Try again in a moment.";

fn forget_inner(flag: &AtomicBool, env: &Env, now: DateTime<Utc>, wait: std::time::Duration) -> Result<UsageConsentOut, String> {
  let (dir, mut store) = open(env)?;
  let Some(_guard) = ScanGuard::acquire(flag) else {
    let deny = ConsentChange { state: Some(ReadLogs::Denied), ..ConsentChange::default() };
    store.change_log_consent(&deny, now).map_err(user_error)?;
    return Err(BUSY.into());
  };
  forget(&mut store, &dir, now, wait).map_err(|e| match e {
    UsageError::Busy => BUSY.to_string(),
    _ => "The usage data could not be deleted.".to_string(),
  })?;
  consent_out(env, &dir, &store)
}

#[derive(Serialize, Debug, Default, Clone, Copy, PartialEq)]
pub struct CostByType {
  pub input: f64,
  pub output: f64,
  pub cache_read: f64,
  pub cache_write: f64,
}

impl CostByType {
  fn add(&mut self, o: &CostByType) {
    self.input += o.input;
    self.output += o.output;
    self.cache_read += o.cache_read;
    self.cache_write += o.cache_write;
  }

  fn rounded(self) -> CostByType {
    CostByType {
      input: round6(self.input),
      output: round6(self.output),
      cache_read: round6(self.cache_read),
      cache_write: round6(self.cache_write),
    }
  }
}

// Splits one (tool, model) row's stored cost by tokens x base list rate. Never reprices:
// cost_at on summed tokens would trip the long-context switch.
fn split_cost(entry: Option<&PriceEntry>, t: &Tokens, cost: f64) -> CostByType {
  let Some(e) = entry else {
    return CostByType::default();
  };
  let get = |k: &str| e.costs.get(k).copied();
  let (Some(r_in), Some(r_out)) = (get("input_cost_per_token"), get("output_cost_per_token")) else {
    return CostByType::default();
  };
  let r_cr = get("cache_read_input_token_cost").unwrap_or(r_in);
  let r_cw5 = get("cache_creation_input_token_cost").unwrap_or(r_in);
  let r_cw1 = get("cache_creation_input_token_cost_above_1hr").unwrap_or(r_cw5);
  let w_in = t.input as f64 * r_in;
  let w_out = t.output as f64 * r_out;
  let w_cr = t.cache_read as f64 * r_cr;
  let w_cw = t.cache_write_5m as f64 * r_cw5 + t.cache_write_1h as f64 * r_cw1;
  let sum = w_in + w_out + w_cr + w_cw;
  if !(sum.is_finite() && sum > 0.0) {
    return CostByType::default();
  }
  let part = |w: f64| cost * w / sum;
  CostByType { input: part(w_in), output: part(w_out), cache_read: part(w_cr), cache_write: part(w_cw) }
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
  pub cost_by_type: CostByType,
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
    cost_by_type: CostByType::default(),
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
  pub projects: Vec<ProjectRow>,
}

#[derive(Serialize, Debug)]
pub struct ProjectRow {
  pub key: String,
  pub name: String,
  #[serde(flatten)]
  pub row: Row,
}

#[derive(Serialize, Debug)]
pub struct DayOut {
  pub day: String,
  pub tool: String,
  pub model: String,
  pub cost_usd: f64,
  pub tokens_total: u64,
  pub events: u64,
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
  pub window_s: Option<i64>,
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

fn window_secs(w: &LimitWindow) -> Option<i64> {
  match w {
    LimitWindow::Session => Some(5 * 3600),
    LimitWindow::Weekly => Some(7 * 86400),
    LimitWindow::Model(m) => match m.strip_prefix("window:") {
      Some(rest) => rest.strip_suffix('m')?.parse::<i64>().ok().filter(|n| *n > 0)?.checked_mul(60),
      None => Some(7 * 86400),
    },
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
  let book = match store.load_prices().map_err(user_error)? {
    Some((book, _)) => book,
    None => PriceBook::bundled(),
  };
  let mut windows = Vec::new();
  for (key, label, days) in specs {
    let from = day_bounds(today - Duration::days(days - 1)).0;
    let report = store.aggregate(&query(from, to, vec![GroupBy::Tool, GroupBy::Model])).map_err(user_error)?;
    let mut split_total = CostByType::default();
    let rows = report
      .rows
      .iter()
      .map(|r| {
        let split = split_cost(book.lookup(&r.keys[1]), &r.tokens, r.cost_usd);
        split_total.add(&split);
        let mut row = row_of(r);
        row.cost_by_type = split.rounded();
        ModelRow { tool: r.keys[0].clone(), model: r.keys[1].clone(), row }
      })
      .collect();
    let mut totals = row_of(&report.totals);
    totals.cost_by_type = split_total.rounded();
    let by_project = store.aggregate(&query(from, to, vec![GroupBy::Project])).map_err(user_error)?;
    let mut projects: Vec<ProjectRow> = by_project
      .rows
      .iter()
      .map(|r| ProjectRow { key: r.keys[0].chars().take(12).collect(), name: r.labels[0].clone(), row: row_of(r) })
      .collect();
    projects.sort_by(|a, b| {
      b.row.cost_usd.total_cmp(&a.row.cost_usd).then(b.row.tokens_total.cmp(&a.row.tokens_total))
    });
    windows.push(WindowOut { key, label, from: stamp(from), to: stamp(to), totals, rows, projects });
  }
  let month_from = day_bounds(today - Duration::days(MONTH_DAYS - 1)).0;
  let by_day = store.aggregate(&query(month_from, to, vec![GroupBy::Day, GroupBy::Tool, GroupBy::Model])).map_err(user_error)?;
  let days = by_day
    .rows
    .iter()
    .map(|r| {
      let row = row_of(r);
      DayOut {
        day: r.keys[0].clone(),
        tool: r.keys[1].clone(),
        model: r.keys[2].clone(),
        cost_usd: row.cost_usd,
        tokens_total: row.tokens_total,
        events: row.events,
      }
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
      window_s: window_secs(&l.window),
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

impl Id {
  fn number(&self) -> Option<u64> {
    match self {
      Id::Num(n) => u64::try_from(*n).ok(),
      Id::Text(s) => s.parse().ok(),
    }
  }
}

fn run_windows(runs: &[RawRun], now: DateTime<Utc>) -> (Vec<RunWindow>, Vec<RunIds>) {
  let mut windows = Vec::new();
  let mut ids = Vec::new();
  for run in runs {
    let (session_id, branch) = (non_empty(&run.session_id), non_empty(&run.branch));
    let Some((started_at, ended_at)) = window_of(run, now) else {
      continue;
    };
    ids.push(RunIds { run_id: run.id.number().unwrap_or(0), task_id: run.task_id.number() });
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
  (windows, ids)
}

fn earlier_facts(
  store: &Store,
  windows: &[RunWindow],
  from: DateTime<Utc>,
  to: DateTime<Utc>,
  now: DateTime<Utc>,
) -> Result<Option<Vec<tmgr_usage::facts::StoredFact>>, String> {
  let overlapping: Vec<RunWindow> =
    windows.iter().filter(|w| w.started_at <= to && w.ended_at.unwrap_or(now) >= from).cloned().collect();
  facts_from(&overlapping, from).map(|f| store.facts(f, to)).transpose().map_err(user_error)
}

fn newest_first(windows: Vec<RunWindow>, ids: Vec<RunIds>) -> (Vec<RunWindow>, Vec<RunIds>) {
  let mut pairs: Vec<_> = windows.into_iter().zip(ids).collect();
  pairs.sort_by_key(|p| std::cmp::Reverse(p.0.started_at));
  pairs.into_iter().unzip()
}

fn tasks_inner(env: &Env, now: DateTime<Utc>, runs: &[RawRun]) -> Result<TasksOut, String> {
  let (windows, ids) = run_windows(runs, now);
  let (mut windows, mut ids) = newest_first(windows, ids);
  let runs_truncated = windows.len() > MAX_RUNS;
  windows.truncate(MAX_RUNS);
  ids.truncate(MAX_RUNS);
  let (_, store) = open(env)?;
  let today = local_today(now);
  let from = day_bounds(today - Duration::days(MONTH_DAYS - 1)).0;
  let to = day_bounds(today).1;
  let events = store.link_events(from, to).map_err(user_error)?;
  let facts = store.facts(from, to).map_err(user_error)?;
  let earlier = earlier_facts(&store, &windows, from, to, now)?;
  let agents = store.agents().map_err(user_error)?;
  let linker = Linker::new(&windows, &ids, &events, earlier.as_deref().unwrap_or(&facts), &agents, now);
  let report = linker.attribute(&events);
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
  let (windows, ids) = run_windows(runs, now);
  let task_ref = format!("{workspace}/{task_id}");
  let Some(first) = windows.iter().filter(|w| w.task_ref == task_ref).map(|w| w.started_at).min() else {
    return Ok(TaskUsageOut { supported: true, ..Default::default() });
  };
  let mine_windows = || windows.iter().filter(|w| w.task_ref == task_ref);
  let last_end = if mine_windows().any(|w| w.ended_at.is_none()) {
    now
  } else {
    mine_windows().filter_map(|w| w.ended_at).max().unwrap_or(now).min(now)
  };
  let (from, to) = (first - Duration::days(1), last_end + Duration::hours(1));
  let (_, store) = open(env)?;
  let (events, tokens): (Vec<_>, Vec<_>) = store.link_events_with_tokens(from, to).map_err(user_error)?.into_iter().unzip();
  let facts = store.facts(from, to).map_err(user_error)?;
  let agents = store.agents().map_err(user_error)?;
  let earlier = earlier_facts(&store, &windows, from, to, now)?;
  let linker = Linker::new(&windows, &ids, &events, earlier.as_deref().unwrap_or(&facts), &agents, now);
  let mine: HashSet<&str> = windows.iter().filter(|w| w.task_ref == task_ref).map(|w| w.run_ref.as_str()).collect();
  let matching = |links: &[Link]| -> Vec<usize> {
    links.iter().filter(|l| mine.contains(l.run_ref.as_str())).map(|l| l.event_index).collect()
  };
  let linked_events = matching(&linker.attribute(&events).links);
  let linked_facts = matching(&linker.attribute(&facts_as_events(&facts, &events)).links);
  let aggregate = build(&TaskInput {
    source_id: store.install_id().map_err(user_error)?,
    generated_at: now,
    task: TaskIdent { workspace: workspace.to_string(), task_ref: task_id.to_string(), id: u64::try_from(task_id).ok() },
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
pub async fn usage_consent() -> Result<UsageConsentOut, String> {
  if snap() {
    return Ok(UsageConsentOut {
      supported: false,
      unsupported_reason: Some(SNAP_REASON.into()),
      state: "unset",
      claude: true,
      codex: true,
      decided_at: None,
      folders: Vec::new(),
      has_data: false,
    });
  }
  blocking(|| consent_inner(&Env::from_process())).await
}

#[tauri::command]
pub async fn usage_consent_set(
  state: Option<String>,
  claude: Option<bool>,
  codex: Option<bool>,
) -> Result<UsageConsentOut, String> {
  if snap() {
    return Err(SNAP_REASON.into());
  }
  blocking(move || consent_set_inner(&Env::from_process(), Utc::now(), state.as_deref(), claude, codex)).await
}

#[tauri::command]
pub async fn usage_forget() -> Result<UsageConsentOut, String> {
  if snap() {
    return Err(SNAP_REASON.into());
  }
  blocking(|| forget_inner(&SCANNING, &Env::from_process(), Utc::now(), FORGET_WAIT)).await
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

  fn granted(home: &Path) -> Env {
    let env = env_in(home);
    consent_set_inner(&env, local_noon(), Some("granted"), Some(true), Some(true)).unwrap();
    env
  }

  fn events(env: &Env) -> u64 {
    open(env).unwrap().1.event_count().unwrap()
  }

  #[test]
  fn no_choice_yet_reads_nothing_and_lists_the_folders() {
    let tmp = tempfile::tempdir().unwrap();
    let env = env_in(tmp.path());
    let now = local_noon();
    claude_log(tmp.path(), now);
    codex_log(tmp.path(), now);
    let scan = scan_inner(&AtomicBool::new(false), &env, now).unwrap();
    assert_eq!((scan.state, scan.files, scan.events_new), ("off", 0, 0));
    assert_eq!(events(&env), 0);
    let c = consent_inner(&env).unwrap();
    assert!(c.supported && c.unsupported_reason.is_none());
    assert_eq!((c.state, c.claude, c.codex, c.has_data), ("unset", true, true, false));
    assert!(c.decided_at.is_none());
    let folders: Vec<_> = c.folders.iter().map(|f| (f.tool, f.label.as_str(), f.path.as_str())).collect();
    assert_eq!(folders, [("claude", "claude", "~/.claude"), ("codex", "codex", "~/.codex")]);
  }

  #[test]
  fn granted_reads_both_tools_and_codex_off_reads_only_claude() {
    let tmp = tempfile::tempdir().unwrap();
    let env = env_in(tmp.path());
    let now = local_noon();
    claude_log(tmp.path(), now);
    codex_log(tmp.path(), now);
    let c = consent_set_inner(&env, now, Some("granted"), Some(true), Some(false)).unwrap();
    assert_eq!((c.state, c.claude, c.codex), ("granted", true, false));
    assert_eq!(c.decided_at.as_deref(), Some(stamp(now).as_str()));
    let scan = scan_inner(&AtomicBool::new(false), &env, now).unwrap();
    assert_eq!((scan.state, scan.events_new), ("done", 2));
    let ov = overview_inner(&env, now).unwrap();
    assert!(ov.windows[0].rows.iter().all(|r| r.tool == "claude"));
    consent_set_inner(&env, now, Some("granted"), Some(true), Some(true)).unwrap();
    assert_eq!(scan_inner(&AtomicBool::new(false), &env, now).unwrap().events_new, 1);
    assert!(consent_inner(&env).unwrap().has_data);
  }

  #[test]
  fn denied_or_both_tools_off_reads_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let env = env_in(tmp.path());
    let now = local_noon();
    claude_log(tmp.path(), now);
    let c = consent_set_inner(&env, now, Some("granted"), Some(false), Some(false)).unwrap();
    assert_eq!((c.state, c.claude, c.codex), ("denied", false, false));
    assert_eq!(scan_inner(&AtomicBool::new(false), &env, now).unwrap().state, "off");
    consent_set_inner(&env, now, Some("denied"), Some(true), Some(true)).unwrap();
    let again = consent_set_inner(&env, now, Some("denied"), Some(true), Some(true)).unwrap();
    assert_eq!((again.state, again.claude, again.codex), ("denied", true, true));
    assert_eq!(scan_inner(&AtomicBool::new(false), &env, now).unwrap().state, "off");
    assert_eq!(events(&env), 0);
    assert_eq!(consent_set_inner(&env, now, Some("unset"), None, None).unwrap_err(), "Unknown choice.");
    assert_eq!(consent_inner(&env).unwrap().state, "denied");
  }

  #[test]
  fn forget_deletes_events_limits_and_snapshots_and_turns_reading_off() {
    let tmp = tempfile::tempdir().unwrap();
    let env = granted(tmp.path());
    let now = local_noon();
    claude_log(tmp.path(), now);
    codex_log(tmp.path(), now);
    scan_inner(&AtomicBool::new(false), &env, now).unwrap();
    assert!(overview_inner(&env, now).unwrap().limits.len() == 1);
    let snaps = paths::snapshots_dir(&paths::data_dir(&env).unwrap()).join("claude");
    std::fs::create_dir_all(&snaps).unwrap();
    std::fs::write(snaps.join("s.json"), "{}").unwrap();

    let flag = AtomicBool::new(false);
    let c = forget_inner(&flag, &env, now, FORGET_WAIT).unwrap();
    assert_eq!((c.state, c.claude, c.codex, c.has_data), ("denied", true, true, false));
    assert!(!flag.load(Ordering::Acquire));
    assert_eq!(events(&env), 0);
    let ov = overview_inner(&env, now).unwrap();
    assert!(!ov.has_data && ov.limits.is_empty());
    assert!(!snaps.exists());
    assert_eq!(scan_inner(&flag, &env, now).unwrap().state, "off");
    assert_eq!(forget_inner(&flag, &env, now, FORGET_WAIT).unwrap().state, "denied");
  }

  #[test]
  fn forget_while_a_scan_runs_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let env = granted(tmp.path());
    let now = local_noon();
    claude_log(tmp.path(), now);
    scan_inner(&AtomicBool::new(false), &env, now).unwrap();
    let flag = AtomicBool::new(false);
    let held = ScanGuard::acquire(&flag).unwrap();
    assert_eq!(forget_inner(&flag, &env, now, FORGET_WAIT).unwrap_err(), "A scan is running. Try again in a moment.");
    assert_eq!(events(&env), 2);
    assert_eq!(consent_inner(&env).unwrap().state, "denied", "reading is off even though nothing was deleted");
    drop(held);
  }

  #[test]
  fn forget_while_another_process_scans_times_out_with_reading_off() {
    let tmp = tempfile::tempdir().unwrap();
    let env = granted(tmp.path());
    let now = local_noon();
    claude_log(tmp.path(), now);
    scan_inner(&AtomicBool::new(false), &env, now).unwrap();
    let held = tmgr_usage::consent::scan_lock(&paths::data_dir(&env).unwrap()).unwrap();
    let flag = AtomicBool::new(false);
    let err = forget_inner(&flag, &env, now, std::time::Duration::from_millis(120)).unwrap_err();
    assert_eq!(err, "A scan is running. Try again in a moment.");
    assert_eq!(events(&env), 2);
    assert_eq!(consent_inner(&env).unwrap().state, "denied");
    drop(held);
    assert!(!forget_inner(&flag, &env, now, FORGET_WAIT).unwrap().has_data);
  }

  #[test]
  fn consent_changes_are_field_level_so_a_stale_page_cannot_grant() {
    let tmp = tempfile::tempdir().unwrap();
    let env = env_in(tmp.path());
    let now = local_noon();
    let c = consent_set_inner(&env, now, Some("denied"), None, None).unwrap();
    assert_eq!((c.state, c.claude, c.codex), ("denied", true, true));
    let stale = consent_set_inner(&env, now, None, Some(false), None).unwrap();
    assert_eq!((stale.state, stale.claude, stale.codex), ("denied", false, true));
    let none = consent_set_inner(&env, now + Duration::hours(1), None, None, None).unwrap();
    assert_eq!((none.state, none.decided_at.clone()), ("denied", stale.decided_at.clone()));
    consent_set_inner(&env, now, None, None, Some(false)).unwrap();
    let on = consent_set_inner(&env, now, Some("granted"), None, None).unwrap();
    assert_eq!((on.state, on.claude, on.codex), ("granted", true, true), "granted with both off turns both on");
    let one = consent_set_inner(&env, now, None, None, Some(false)).unwrap();
    assert_eq!((one.state, one.claude, one.codex), ("granted", true, false));
    let off = consent_set_inner(&env, now, None, Some(false), None).unwrap();
    assert_eq!((off.state, off.claude, off.codex), ("denied", false, false));
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
    let env = granted(tmp.path());
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
    assert!(!text.contains("sess-1") && !text.contains("/work/alpha") && !text.contains(&*tmp.path().to_string_lossy()));
  }

  #[test]
  fn empty_home_has_no_data() {
    let tmp = tempfile::tempdir().unwrap();
    let env = granted(tmp.path());
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
    let env = granted(tmp.path());
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
  fn tasks_link_by_session_id_and_keep_runs_without_keys() {
    let tmp = tempfile::tempdir().unwrap();
    let env = granted(tmp.path());
    let now = local_noon();
    claude_log(tmp.path(), now);
    scan_inner(&AtomicBool::new(false), &env, now).unwrap();

    let runs = vec![raw(1, 7, Some("sess-1"), None, now), raw(2, 8, None, None, now)];
    let out = tasks_inner(&env, now, &runs).unwrap();
    assert_eq!((out.runs_used, out.runs_dropped), (2, 0));
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
    let env = granted(tmp.path());
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
    let env = granted(tmp.path());
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

  static SNAP_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

  #[test]
  fn tasks_command_reports_the_snap_reason() {
    let _lock = SNAP_LOCK.lock().unwrap_or_else(|e| e.into_inner());
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
    let env = granted(tmp.path());
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


  fn mcp_only_log(home: &Path, now: DateTime<Utc>) {
    let line = |msg: &str, ago: i64, call: &str| {
      format!(
        r#"{{"type":"assistant","sessionId":"sess-mcp","timestamp":"{}","cwd":"/work/beta","gitBranch":"main","isSidechain":false,"requestId":"req-{msg}","message":{{"id":"{msg}","model":"claude-test-opus-1","content":[{{"type":"text","text":"x"}}{call}],"usage":{{"input_tokens":100,"output_tokens":20,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}}}}"#,
        at(now, ago)
      )
    };
    let start = r#",{"type":"tool_use","id":"tu-start","name":"mcp__tmgr__start_agent_work","input":{"taskId":7,"agent":"claude-code"}}"#;
    write(
      &home.join(".claude/projects/-work-beta/sess-mcp.jsonl"),
      &[line("n1", 119, start), line("n2", 100, ""), line("n3", 60, "")],
    );
  }

  #[test]
  fn task_usage_links_an_mcp_only_run_through_its_start_call() {
    let tmp = tempfile::tempdir().unwrap();
    let env = granted(tmp.path());
    let now = local_noon();
    mcp_only_log(tmp.path(), now);
    scan_inner(&AtomicBool::new(false), &env, now).unwrap();
    let runs = vec![raw(1, 7, None, None, now), raw(2, 8, None, None, now)];
    let a = task_inner(&env, now, &runs, "ws", 7).unwrap().aggregate.unwrap();
    assert_eq!(a.totals.events, 3);
    assert_eq!(a.totals.tokens.input, 300);
    assert!(task_inner(&env, now, &runs, "ws", 8).unwrap().aggregate.is_none());
    let listed = tasks_inner(&env, now, &runs).unwrap();
    assert_eq!(listed.tasks.iter().map(|t| t.task_ref.as_str()).collect::<Vec<_>>(), vec!["ws/7"]);
    assert_eq!(listed.tasks[0].totals.events, 3);
  }


  #[test]
  fn runs_with_a_text_id_still_link_by_session() {
    let tmp = tempfile::tempdir().unwrap();
    let env = granted(tmp.path());
    let now = local_noon();
    claude_log(tmp.path(), now);
    scan_inner(&AtomicBool::new(false), &env, now).unwrap();
    let mut run = raw(1, 7, Some("sess-1"), None, now);
    run.id = Id::Text("local-a".into());
    let a = task_inner(&env, now, &[run], "ws", 7).unwrap().aggregate.unwrap();
    assert_eq!(a.totals.events, 2);
  }

  #[test]
  fn successive_mcp_tasks_in_one_session_split_at_the_finish() {
    let tmp = tempfile::tempdir().unwrap();
    let env = granted(tmp.path());
    let now = local_noon();
    let line = |msg: &str, ago: i64, call: &str| {
      format!(
        r#"{{"type":"assistant","sessionId":"sess-two","timestamp":"{}","cwd":"/work/gamma","gitBranch":"main","isSidechain":false,"requestId":"req-{msg}","message":{{"id":"{msg}","model":"claude-test-opus-1","content":[{{"type":"text","text":"x"}}{call}],"usage":{{"input_tokens":100,"output_tokens":20,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}}}}"#,
        at(now, ago)
      )
    };
    let tool = |id: &str, name: &str, input: &str| format!(r#",{{"type":"tool_use","id":"{id}","name":"mcp__tmgr__{name}","input":{input}}}"#);
    write(
      &tmp.path().join(".claude/projects/-work-gamma/sess-two.jsonl"),
      &[
        line("g1", 119, &tool("tu-1", "start_agent_work", r#"{"taskId":7}"#)),
        line("g2", 100, ""),
        line("g3", 80, &tool("tu-2", "finish_agent_work", r#"{"runId":1}"#)),
        line("g4", 79, &tool("tu-3", "start_agent_work", r#"{"taskId":8}"#)),
        line("g5", 50, ""),
      ],
    );
    scan_inner(&AtomicBool::new(false), &env, now).unwrap();
    let mut first = raw(1, 7, None, None, now);
    first.ended_at = Some(at(now, 80));
    let mut second = raw(2, 8, None, None, now);
    second.started_at = Some(at(now, 79));
    let runs = vec![first, second];
    let one = task_inner(&env, now, &runs, "ws", 7).unwrap().aggregate.unwrap();
    let two = task_inner(&env, now, &runs, "ws", 8).unwrap().aggregate.unwrap();
    assert_eq!(one.totals.events, 3);
    assert_eq!(two.totals.events, 2);
  }

  #[test]
  fn task_usage_excludes_events_won_by_a_competing_run() {
    let tmp = tempfile::tempdir().unwrap();
    let env = granted(tmp.path());
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
    let env = granted(tmp.path());
    let now = local_noon();
    claude_log(tmp.path(), now);
    scan_inner(&AtomicBool::new(false), &env, now).unwrap();
    let out = task_inner(&env, now, &[raw(2, 8, Some("sess-1"), None, now)], "ws", 7).unwrap();
    assert!(out.supported);
    assert!(out.aggregate.is_none());
  }

  #[test]
  fn task_usage_command_reports_the_snap_reason() {
    let _lock = SNAP_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    std::env::set_var("SNAP", "1");
    let rt = tokio::runtime::Builder::new_current_thread().build().unwrap();
    let out = rt.block_on(usage_task(Vec::new(), "ws".into(), 1)).unwrap();
    std::env::remove_var("SNAP");
    assert!(!out.supported);
    assert_eq!(out.unsupported_reason.as_deref(), Some(SNAP_REASON));
  }

  fn priced_log(home: &Path, now: DateTime<Utc>) {
    let line = |msg: &str, ago: i64, cwd: &str, out: u32| {
      format!(
        r#"{{"type":"assistant","sessionId":"sess-{msg}","timestamp":"{}",{cwd}"gitBranch":"main","isSidechain":false,"requestId":"req-{msg}","message":{{"id":"{msg}","model":"claude-sonnet-4-5","content":[{{"type":"text","text":"x"}}],"usage":{{"input_tokens":1000,"output_tokens":{out},"cache_read_input_tokens":5000,"cache_creation_input_tokens":400}}}}}}"#,
        at(now, ago)
      )
    };
    write(&home.join(".claude/projects/-work-delta/sess-p1.jsonl"), &[line("p1", 30, r#""cwd":"/work/delta","#, 200)]);
    write(&home.join(".claude/projects/-nocwd/sess-p2.jsonl"), &[line("p2", 20, "", 20)]);
  }

  fn scanned(now: DateTime<Utc>) -> (tempfile::TempDir, Env) {
    let tmp = tempfile::tempdir().unwrap();
    let env = granted(tmp.path());
    claude_log(tmp.path(), now);
    codex_log(tmp.path(), now);
    priced_log(tmp.path(), now);
    scan_inner(&AtomicBool::new(false), &env, now).unwrap();
    (tmp, env)
  }

  #[test]
  fn cost_by_type_parts_add_up_to_the_row_cost() {
    let now = local_noon();
    let (_tmp, env) = scanned(now);
    let ov = overview_inner(&env, now).unwrap();
    let w = &ov.windows[0];
    let sonnet = w.rows.iter().find(|r| r.model == "claude-sonnet-4-5").unwrap();
    assert!(sonnet.row.cost_usd > 0.0);
    let c = &sonnet.row.cost_by_type;
    assert!(c.input > 0.0 && c.output > 0.0 && c.cache_read > 0.0 && c.cache_write > 0.0);
    for win in &ov.windows {
      for r in &win.rows {
        let sum = r.row.cost_by_type;
        let parts = sum.input + sum.output + sum.cache_read + sum.cache_write;
        if r.model == "claude-sonnet-4-5" {
          assert!((parts - r.row.cost_usd).abs() < 1e-6, "{parts} vs {}", r.row.cost_usd);
        }
      }
      let t = win.totals.cost_by_type;
      let model_sum: f64 = win.rows.iter().map(|r| r.row.cost_by_type.input).sum();
      assert!((t.input - model_sum).abs() < 1e-5);
    }
    let unpriced = w.rows.iter().find(|r| r.model == "claude-test-opus-1").unwrap();
    assert_eq!(unpriced.row.cost_by_type, CostByType::default());
  }

  #[test]
  fn split_cost_follows_the_cache_fallbacks_and_skips_unrated_models() {
    let t = Tokens { input: 100, output: 50, cache_read: 10, cache_write_5m: 20, cache_write_1h: 30, reasoning: 0 };
    let entry = |pairs: &[(&str, f64)]| PriceEntry { costs: pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect() };
    let only_base = entry(&[("input_cost_per_token", 1.0), ("output_cost_per_token", 2.0)]);
    let c = split_cost(Some(&only_base), &t, 10.0);
    // weights: in 100, out 100, cache_read 10 (input rate), cache_write 50 (input rate for 5m and 1h)
    let total = 100.0 + 100.0 + 10.0 + 50.0;
    assert!((c.input - 10.0 * 100.0 / total).abs() < 1e-9);
    assert!((c.cache_write - 10.0 * 50.0 / total).abs() < 1e-9);
    assert!((c.input + c.output + c.cache_read + c.cache_write - 10.0).abs() < 1e-9);
    let one_hour = entry(&[
      ("input_cost_per_token", 1.0),
      ("output_cost_per_token", 2.0),
      ("cache_creation_input_token_cost", 1.5),
      ("cache_creation_input_token_cost_above_1hr", 3.0),
    ]);
    let c = split_cost(Some(&one_hour), &t, 10.0);
    assert!((c.cache_write - 10.0 * (20.0 * 1.5 + 30.0 * 3.0) / (100.0 + 100.0 + 10.0 + 120.0)).abs() < 1e-9);
    assert_eq!(split_cost(Some(&entry(&[("input_cost_per_token", 1.0)])), &t, 10.0), CostByType::default());
    assert_eq!(split_cost(None, &t, 10.0), CostByType::default());
    assert_eq!(split_cost(Some(&only_base), &Tokens::default(), 10.0), CostByType::default());
  }

  #[test]
  fn projects_group_by_hash_with_basename_and_a_short_key() {
    let now = local_noon();
    let (_tmp, env) = scanned(now);
    let ov = overview_inner(&env, now).unwrap();
    for w in &ov.windows {
      for p in &w.projects {
        assert!(!p.key.contains(['/', '\\']) && !p.name.contains(['/', '\\']));
        assert!(p.key.is_empty() || p.key.len() == 12);
        assert_eq!(p.row.cost_by_type, CostByType::default());
      }
      assert_eq!(w.projects.iter().map(|p| p.row.events).sum::<u64>(), w.totals.events);
      assert!(w.projects.windows(2).all(|p| {
        let (a, b) = (&p[0].row, &p[1].row);
        a.cost_usd > b.cost_usd || (a.cost_usd == b.cost_usd && a.tokens_total >= b.tokens_total)
      }));
    }
    let names: Vec<_> = ov.windows[0].projects.iter().map(|p| (p.name.as_str(), p.key.len())).collect();
    assert!(names.contains(&("alpha", 12)) && names.contains(&("beta", 12)) && names.contains(&("delta", 12)));
    let none = ov.windows[0].projects.iter().find(|p| p.name.is_empty()).unwrap();
    assert_eq!((none.key.as_str(), none.row.events), ("", 1));
    assert_eq!(ov.windows[0].projects[0].name, "delta");
    let text = serde_json::to_string(&ov).unwrap();
    assert!(!text.contains("/work/"));
  }

  #[test]
  fn days_carry_model_and_events_and_still_sum_to_the_day_totals() {
    let now = local_noon();
    let (_tmp, env) = scanned(now);
    let ov = overview_inner(&env, now).unwrap();
    assert!(ov.days.iter().all(|d| !d.model.is_empty() && d.events > 0));
    assert!(ov.days.iter().any(|d| d.model == "gpt-test") && ov.days.iter().any(|d| d.model == "claude-sonnet-4-5"));
    let (_, store) = open(&env).unwrap();
    let from = day_bounds(local_today(now) - Duration::days(MONTH_DAYS - 1)).0;
    let old = store.aggregate(&query(from, day_bounds(local_today(now)).1, vec![GroupBy::Day, GroupBy::Tool])).unwrap();
    assert!(!old.rows.is_empty());
    for r in &old.rows {
      let mine: Vec<_> = ov.days.iter().filter(|d| d.day == r.keys[0] && d.tool == r.keys[1]).collect();
      let (cost, tokens, events) = (r.cost_usd, row_of(r).tokens_total, r.events);
      assert!((mine.iter().map(|d| d.cost_usd).sum::<f64>() - cost).abs() < 1e-5);
      assert_eq!(mine.iter().map(|d| d.tokens_total).sum::<u64>(), tokens);
      assert_eq!(mine.iter().map(|d| d.events).sum::<u64>(), events);
    }
  }

  #[test]
  fn limit_window_lengths_in_seconds() {
    let secs = |k: &str| window_secs(&LimitWindow::from_key(k).unwrap());
    assert_eq!(secs("session"), Some(18_000));
    assert_eq!(secs("weekly"), Some(604_800));
    assert_eq!(secs("model:opus"), Some(604_800));
    assert_eq!(secs("model:window:300m"), Some(18_000));
    assert_eq!(secs("model:window:90m"), Some(5_400));
    assert_eq!(secs("model:window:m"), None);
    assert_eq!(secs("model:window:0m"), None);
    assert_eq!(secs("model:window:xm"), None);
    assert_eq!(secs("model:window:300"), None);
    let now = local_noon();
    let (_tmp, env) = scanned(now);
    let ov = overview_inner(&env, now).unwrap();
    assert_eq!(ov.limits[0].window_s, Some(18_000));
  }

  // QA regression (TM-578): the frontend's "last 7 days" strip sums `days`; it must equal the "Last 7 days" window.
  #[test]
  fn last_seven_local_days_of_days_equal_the_week_window() {
    let now = local_noon();
    let tmp = tempfile::tempdir().unwrap();
    let env = granted(tmp.path());
    let line = |n: i64, out: u32| {
      format!(
        r#"{{"type":"assistant","sessionId":"sess-d{n}","timestamp":"{}","cwd":"/work/alpha","gitBranch":"main","isSidechain":false,"requestId":"req-d{n}","message":{{"id":"d{n}","model":"claude-sonnet-4-5","content":[{{"type":"text","text":"x"}}],"usage":{{"input_tokens":1000,"output_tokens":{out},"cache_read_input_tokens":5000,"cache_creation_input_tokens":400}}}}}}"#,
        at(now, n * 24 * 60)
      )
    };
    let offsets = [0_i64, 1, 3, 6, 7, 8, 13, 14, 20, 29];
    let lines: Vec<String> = offsets.iter().map(|n| line(*n, 100 + *n as u32 * 10)).collect();
    write(&tmp.path().join(".claude/projects/-work-alpha/sess-d.jsonl"), &lines);
    scan_inner(&AtomicBool::new(false), &env, now).unwrap();
    let ov = overview_inner(&env, now).unwrap();
    let today = local_today(now);
    for (idx, span) in [(0usize, 1i64), (1, 7), (2, 30)] {
      let keys: Vec<String> = (0..span).map(|k| (today - Duration::days(k)).format("%Y-%m-%d").to_string()).collect();
      let mine: Vec<_> = ov.days.iter().filter(|d| keys.contains(&d.day)).collect();
      let w = &ov.windows[idx].totals;
      assert_eq!(mine.iter().map(|d| d.events).sum::<u64>(), w.events, "events, window {idx}");
      assert_eq!(mine.iter().map(|d| d.tokens_total).sum::<u64>(), w.tokens_total, "tokens, window {idx}");
      assert!((mine.iter().map(|d| d.cost_usd).sum::<f64>() - w.cost_usd).abs() < 1e-5, "cost, window {idx}");
    }
    assert_eq!(ov.windows[1].totals.events, 4);
    assert_eq!(ov.windows[2].totals.events, 10);
  }
}
