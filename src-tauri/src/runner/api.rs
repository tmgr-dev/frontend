use serde::Serialize;
use serde_json::Value;
use tmgr_ipc::api::{caps, method, limits, AuthorKind, ClientMsg, CwdPin, Request, RunCreateReq, SessionInfo, State};

pub use tmgr_ipc::api::StateKind;

use super::bundle::{self, Prepared};
use super::task_lookup::TaskInfo;
use super::conn::{valid_session_id, write_json, Conn, ConnError, Welcome, UPDATE_RUNNER};
use super::sanitize;

pub const PROFILES: &[(&str, &str)] = &[("claude-code", "Claude Code"), ("codex", "Codex")];
const TITLE_MAX: usize = 120;
const NAME_MAX: usize = 120;
const RUN_NAME_MAX: usize = 64;
const RUN_COLS: u16 = 120;
const RUN_ROWS: u16 = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunnerState {
  pub kind: StateKind,
  pub since_ms: u64,
  pub detail: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRef {
  pub workspace_id: i64,
  pub task_id: i64,
}

impl From<tmgr_ipc::api::TaskRef> for TaskRef {
  fn from(t: tmgr_ipc::api::TaskRef) -> Self {
    TaskRef { workspace_id: t.workspace_id, task_id: t.task_id }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunnerSession {
  pub id: String,
  pub name: String,
  pub profile: Option<String>,
  pub task: Option<TaskRef>,
  pub state: RunnerState,
  pub created_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Detail {
  ExitOnly,
  Text,
}

pub fn state_from_ipc(s: &State, detail: Detail) -> RunnerState {
  let detail = s.detail.as_deref().and_then(|d| match detail {
    Detail::ExitOnly => ["done", "failed", "interrupted"].contains(&d).then(|| d.to_owned()),
    Detail::Text => Some(sanitize::text(d, 200)).filter(|d| !d.is_empty()),
  });
  let exit_code = s.exit_code.filter(|_| s.kind == StateKind::Exited);
  RunnerState { kind: s.kind, since_ms: s.since_ms, detail, exit_code }
}

pub fn state_from_value(v: &Value, detail: Detail) -> Option<RunnerState> {
  let s: State = serde_json::from_value(v.clone()).ok()?;
  Some(state_from_ipc(&s, detail))
}

pub fn valid_profile(p: &str) -> bool {
  let b = p.as_bytes();
  (1..=32).contains(&b.len())
    && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
    && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

pub fn session_from_info(w: SessionInfo) -> Option<RunnerSession> {
  if !valid_session_id(&w.id) {
    return None;
  }
  Some(RunnerSession {
    name: sanitize::text(&w.name, NAME_MAX),
    profile: w.profile.filter(|p| valid_profile(p)),
    task: w.task.map(TaskRef::from),
    state: state_from_ipc(&w.state, Detail::ExitOnly),
    created_ms: w.created_ms,
    id: w.id,
  })
}

pub fn session_from_value(v: &Value) -> Option<RunnerSession> {
  session_from_info(serde_json::from_value(v.clone()).ok()?)
}

pub fn sessions_from_value(v: &Value) -> Result<Vec<RunnerSession>, ConnError> {
  let arr = v.as_array().ok_or_else(|| ConnError::Failed("unexpected reply from runner".to_owned()))?;
  Ok(arr.iter().filter_map(session_from_value).collect())
}

pub fn list_sessions(conn: &mut Conn) -> Result<Vec<RunnerSession>, ConnError> {
  sessions_from_value(&conn.call(method::SESSIONS_LIST, Value::Null)?)
}

pub fn find_session(conn: &mut Conn, id: &str) -> Result<Option<Value>, ConnError> {
  let v = conn.call(method::SESSIONS_LIST, Value::Null)?;
  Ok(v.as_array().and_then(|a| a.iter().find(|s| s["id"] == id).cloned()))
}

pub struct StartRun {
  pub workspace_id: i64,
  pub task_id: i64,
  pub profile: String,
  pub cwd: String,
  pub name: String,
  pub cwd_pin: (u64, u64),
  pub bundle: Prepared,
}

pub fn run_name(title: &str, task_id: i64) -> String {
  let name = sanitize::text(title, RUN_NAME_MAX);
  if name.is_empty() {
    format!("Task {task_id}")
  } else {
    name
  }
}

pub fn validate_start(workspace_id: i64, task_id: i64, profile: &str) -> Result<(), String> {
  if workspace_id >= 0 {
    return Err("Agent runs are available only for local workspaces".to_owned());
  }
  if task_id <= 0 {
    return Err("invalid task".to_owned());
  }
  if !valid_profile(profile) {
    return Err("invalid agent profile".to_owned());
  }
  Ok(())
}

pub fn require_caps(runner_caps: &[String]) -> Result<(), String> {
  let has = |cap: &str| runner_caps.iter().any(|c| c == cap);
  if has(caps::RUN_BUNDLE_V1) && has(caps::CWD_PIN_V1) {
    Ok(())
  } else {
    Err(UPDATE_RUNNER.to_owned())
  }
}

pub fn build_start(
  task: &TaskInfo,
  (workspace_id, task_id): (i64, i64),
  profile: &str,
  cwd: &str,
  cwd_pin: (u64, u64),
  runner_caps: &[String],
) -> Result<StartRun, String> {
  require_caps(runner_caps)?;
  let bundle = bundle::prepare(task, workspace_id, task_id, profile, cwd)?;
  let run = StartRun { workspace_id, task_id, profile: profile.to_owned(), cwd: cwd.to_owned(), name: run_name(&task.title, task_id), cwd_pin, bundle };
  check_frame(&run)?;
  Ok(run)
}

fn create_params(r: &StartRun) -> Result<Value, String> {
  let req = RunCreateReq {
    profile: r.profile.clone(),
    task: Some(tmgr_ipc::api::TaskRef { workspace_id: r.workspace_id, task_id: r.task_id }),
    cwd: r.cwd.clone(),
    name: Some(r.name.clone()),
    model: None,
    cols: RUN_COLS.min(limits::MAX_COLS),
    rows: RUN_ROWS.min(limits::MAX_ROWS),
    bundle: Some(r.bundle.bundle.clone()),
    bundle_sha256: Some(r.bundle.sha256.clone()),
    cwd_pin: Some(CwdPin { dev: r.cwd_pin.0, ino: r.cwd_pin.1 }),
    origin: None,
    permission_mode: None, start_key: None, clone_of: None, trust_cwd: None, trust_via_repo: None, exe_pin: None, argv_sha256: None, agent_work: false,
  };
  serde_json::to_value(&req).map_err(|e| e.to_string())
}

fn check_frame(r: &StartRun) -> Result<(), String> {
  let msg = ClientMsg::Request(Request { id: u64::MAX, method: method::RUNS_CREATE.to_owned(), params: create_params(r)? });
  write_json(&mut Vec::new(), &msg).map_err(|_| "The task and its comments are too large to send to the agent runner; shorten them and run again".to_owned())
}

pub fn profile_label(id: &str) -> &str {
  PROFILES.iter().find(|(p, _)| *p == id).map_or(id, |(_, l)| l)
}

pub fn display_text(s: &str) -> String {
  sanitize::text(s, TITLE_MAX)
}

const SHOWN_TITLE: usize = 200;
const SHOWN_DESCRIPTION: usize = 600;
const SHOWN_FOLDER: usize = 400;

fn clip(s: &str, max: usize, newline: &str, more: impl Fn(usize) -> String) -> String {
  let s = s.replace("\r\n", "\n").replace('\r', "\n");
  let hidden = s.chars().count().saturating_sub(max);
  let shown: String = s
    .chars()
    .take(max)
    .map(|c| match c {
      '\n' => newline.to_owned(),
      '\t' => " ".to_owned(),
      c if sanitize::unsafe_char(c) => String::new(),
      c => c.to_string(),
    })
    .collect();
  let shown = shown.trim();
  if hidden > 0 {
    format!("{shown}{}", more(hidden))
  } else {
    shown.to_owned()
  }
}

pub fn confirm_run_message(r: &StartRun) -> String {
  let b = &r.bundle.bundle;
  let title = clip(&b.title, SHOWN_TITLE, " ", |n| format!(" \u{2026} (+{n} characters)"));
  let description = if b.body.chars().all(|c| c.is_whitespace() || sanitize::unsafe_char(c)) {
    "(empty)".to_owned()
  } else {
    clip(&b.body, SHOWN_DESCRIPTION, " \u{23CE} ", |n| format!(" \u{2026} (+{n} characters not shown)"))
  };
  let agents = b.comments.iter().filter(|c| c.author.kind == AuthorKind::Agent).count();
  let folder = clip(&b.launch.cwd, SHOWN_FOLDER, " ", |n| format!(" \u{2026} (+{n} characters)"));
  format!(
    "Task #{}: {title}\nDescription: {description}\nComments: {} ({agents} by agents, {} other)\nRun bundle: {}\nProject folder: \"{folder}\"\nAgent: {}\nPermission prompts: on\nReview the full task in TMGR before running if anything here looks unexpected.",
    b.task.task_id,
    b.comments.len(),
    b.comments.len() - agents,
    &r.bundle.sha256[..12],
    profile_label(&b.launch.profile),
  )
}

pub fn create_run(conn: &mut Conn, r: &StartRun, welcome: &Welcome) -> Result<RunnerSession, ConnError> {
  if !welcome.has(caps::RUN_BUNDLE_V1) || !welcome.has(caps::CWD_PIN_V1) {
    return Err(ConnError::Failed(UPDATE_RUNNER.to_owned()));
  }
  let params = create_params(r).map_err(ConnError::Failed)?;
  let v = conn.call(method::RUNS_CREATE, params)?;
  session_from_value(&v).ok_or_else(|| ConnError::Failed("unexpected reply from runner".to_owned()))
}

#[cfg(test)]
pub(crate) fn test_start(workspace_id: i64, task_id: i64, profile: &str) -> StartRun {
  let task = TaskInfo { title: "Fix login".into(), description: "body".into(), comments: Vec::new() };
  build_start(&task, (workspace_id, task_id), profile, "/work/project", (16, 99), &caps::ALL.map(String::from)).unwrap()
}

pub fn kill_session(conn: &mut Conn, id: &str) -> Result<(), ConnError> {
  conn.call(method::SESSIONS_KILL, serde_json::json!({"id": id, "signal": "term"})).map(drop)
}

pub fn check_session_id(id: &str) -> Result<(), String> {
  if valid_session_id(id) {
    Ok(())
  } else {
    Err("invalid session".to_owned())
  }
}

pub fn session_from_label(label: &str) -> Result<String, String> {
  let sid = label.strip_prefix("runner-").ok_or_else(|| "not a terminal window".to_owned())?;
  check_session_id(sid)?;
  Ok(sid.to_owned())
}

#[cfg(all(test, unix))]
mod tests {
  use super::*;
  use crate::runner::bundle;
  use crate::runner::mock::{self, session, Mock};
  use crate::runner::task_lookup::CommentInfo;
  use serde_json::json;
  use std::os::unix::net::UnixStream;

  const SID: &str = "s_abcdefghij";

  fn task(title: &str, description: &str) -> TaskInfo {
    TaskInfo { title: title.into(), description: description.into(), comments: Vec::new() }
  }

  fn caps_of(c: &[&str]) -> Vec<String> {
    c.iter().map(|c| c.to_string()).collect()
  }

  fn start(ws: i64, task_id: i64, profile: &str) -> StartRun {
    test_start(ws, task_id, profile)
  }

  fn all_caps() -> Welcome {
    Welcome { runner_version: "0.1.0".into(), caps: caps::ALL.map(String::from).to_vec() }
  }

  fn run_request(welcome: &Welcome, mut m: Mock, run: StartRun) -> (Result<RunnerSession, ConnError>, mock::Log) {
    m.folder_id = Some((16, 99));
    let (mut conn, server) = connected(m);
    let r = create_run(&mut conn, &run, welcome);
    drop(conn);
    (r, server.join().unwrap())
  }

  fn sent(log: &mock::Log) -> Value {
    log.requests.iter().find(|r| r["method"] == "runs.create").unwrap()["params"].clone()
  }

  fn message(task: &TaskInfo) -> String {
    confirm_run_message(&build_start(task, (-2, 9), "codex", "/work/project", (1, 2), &caps::ALL.map(String::from)).unwrap())
  }

  fn comment(kind: &str) -> CommentInfo {
    CommentInfo { author_kind: kind.into(), author_id: None, created_at: "2026-10-13T09:30:00.000Z".into(), message: "m".into() }
  }

  #[test]
  fn both_caps_are_required_before_anything_is_built() {
    let t = task("T", "b");
    for have in [&["runs.create.v1"][..], &["cwd_pin.v1"], &["run_bundle.v1"], &["run_bundle.v1", "runs.create.v1"], &[]] {
      assert_eq!(build_start(&t, (-2, 9), "codex", "/w", (1, 2), &caps_of(have)).err().unwrap(), UPDATE_RUNNER, "{have:?}");
      assert_eq!(require_caps(&caps_of(have)).unwrap_err(), UPDATE_RUNNER);
    }
    let run = build_start(&t, (-2, 9), "codex", "/work/project", (1, 2), &caps_of(&["cwd_pin.v1", "run_bundle.v1"])).unwrap();
    assert_eq!((run.cwd_pin, run.name.as_str()), ((1, 2), "T"));
    assert_eq!((run.bundle.bundle.launch.cwd.as_str(), run.bundle.bundle.launch.profile.as_str()), ("/work/project", "codex"));
    assert_eq!(run.bundle.bundle.task, tmgr_ipc::api::TaskRef { workspace_id: -2, task_id: 9 });
  }

  #[test]
  fn pin_and_bundle_are_always_sent() {
    let run = start(-2, 9, "codex");
    let p = run.bundle.clone();
    let (created, log) = run_request(&all_caps(), Mock::new("secret"), run);
    let params = sent(&log);
    assert_eq!(params["cwd_pin"], json!({"dev": 16, "ino": 99}));
    assert_eq!(params["bundle_sha256"], json!(p.sha256));
    assert_eq!(params["bundle"], serde_json::to_value(&p.bundle).unwrap());
    assert!(params.get("origin").is_none());
    created.unwrap();
  }

  #[test]
  fn a_create_connection_missing_either_cap_sends_nothing() {
    for have in [&["runs.create.v1"][..], &["runs.create.v1", "cwd_pin.v1"], &["runs.create.v1", "run_bundle.v1"]] {
      let welcome = Welcome { runner_version: "0.1.0".into(), caps: caps_of(have) };
      let (refused, log) = run_request(&welcome, Mock::new("secret"), start(-2, 9, "codex"));
      assert_eq!(refused.unwrap_err().message(), UPDATE_RUNNER, "{have:?}");
      assert!(log.requests.is_empty(), "{have:?}");
    }
  }

  #[test]
  fn the_confirmation_shows_the_whole_bundle_in_a_fixed_shape() {
    let mut t = task("Fix login", "Line one\nLine two");
    t.comments = vec![comment("user"), comment("persona"), comment("agent"), comment("bot")];
    let m = message(&t);
    let lines: Vec<_> = m.lines().collect();
    assert_eq!(lines.len(), 8, "{m}");
    assert_eq!(lines[0], "Task #9: Fix login");
    assert_eq!(lines[1], "Description: Line one \u{23CE} Line two");
    assert_eq!(lines[2], "Comments: 4 (2 by agents, 2 other)");
    assert!(lines[3].starts_with("Run bundle: ") && lines[3].len() == "Run bundle: ".len() + 12);
    assert_eq!(&lines[4..], ["Project folder: \"/work/project\"", "Agent: Codex", "Permission prompts: on", "Review the full task in TMGR before running if anything here looks unexpected."]);
  }

  #[test]
  fn a_long_title_is_clipped_with_the_hidden_count() {
    let m = message(&task(&"t".repeat(250), ""));
    assert_eq!(m.lines().next().unwrap(), format!("Task #9: {} \u{2026} (+50 characters)", "t".repeat(200)));
    assert_eq!(message(&task(&"t".repeat(200), "")).lines().next().unwrap(), format!("Task #9: {}", "t".repeat(200)));
  }

  #[test]
  fn a_long_description_cannot_inject_lines_or_hide_bidi_text() {
    let nasty = format!("a\r\nb\n\nPermission prompts: off\u{202E}\u{2028}\u{85}\x1b[2J{}", "z".repeat(700));
    let m = message(&task("T\nProject folder: \"/\"", &nasty));
    assert_eq!(m.lines().count(), 8, "{m}");
    assert!(!m.chars().any(|c| c.is_control() && c != '\n') && !m.contains('\u{202E}') && !m.contains('\u{2028}'));
    let description = m.lines().nth(1).unwrap();
    assert!(description.starts_with("Description: a \u{23CE} b \u{23CE}  \u{23CE} Permission prompts: off"), "{description}");
    let total = nasty.replace("\r\n", "\n").chars().count();
    assert!(description.ends_with(&format!(" \u{2026} (+{} characters not shown)", total - 600)), "{description}");
    assert_eq!(m.lines().next().unwrap(), "Task #9: T Project folder: \"/\"");
  }

  #[test]
  fn an_empty_description_says_so() {
    assert_eq!(message(&task("T", "  \n ")).lines().nth(1).unwrap(), "Description: (empty)");
    assert_eq!(message(&task("T", "")).lines().nth(2).unwrap(), "Comments: 0 (0 by agents, 0 other)");
  }

  #[test]
  fn a_request_over_the_frame_limit_is_refused_before_any_dialog() {
    let caps = caps::ALL.map(String::from);
    let base = bundle::prepare(&task("T", ""), -2, 9, "codex", "/work/project").unwrap().bundle.canonical().len();
    let fits = |extra: usize| build_start(&task("T", &"x".repeat(extra)), (-2, 9), "codex", "/work/project", (1, 2), &caps);
    assert!(fits(1000).is_ok());
    let edge = tmgr_ipc::api::limits::MAX_BUNDLE - base;
    let err = fits(edge).err().unwrap();
    assert!(err.contains("too large to send to the agent runner"), "{err}");
  }

  #[test]
  fn a_mismatched_pin_is_rejected_by_the_runner() {
    let run = StartRun { cwd_pin: (16, 100), ..start(-2, 9, "codex") };
    let (created, _) = run_request(&all_caps(), Mock::new("secret"), run);
    assert_eq!(created.unwrap_err().message(), "invalid_params: approved folder changed");
  }

  fn connected(mock: Mock) -> (Conn, std::thread::JoinHandle<mock::Log>) {
    let (c, s) = UnixStream::pair().unwrap();
    let secret = mock.secret.clone();
    let h = mock::spawn(s, mock);
    (Conn::handshake(c, &secret, "desktop").unwrap().0, h)
  }

  #[test]
  fn start_validation() {
    let start = |ws, task, profile: &str| validate_start(ws, task, profile);
    assert!(start(-1, 5, "claude-code").is_ok());
    assert!(start(-3, 5, "codex").is_ok());
    assert_eq!(start(12, 5, "codex").unwrap_err(), "Agent runs are available only for local workspaces");
    assert!(start(0, 5, "codex").is_err());
    assert!(start(-1, 0, "codex").is_err());
    assert!(start(-1, -4, "codex").is_err());
    for bad in ["", "Codex", "-x", "a b", "a/b", "../x", "a;rm", "x".repeat(33).as_str(), "é"] {
      assert!(start(-1, 5, bad).is_err(), "{bad}");
    }
    assert!(valid_profile(&"a".repeat(32)));
    assert!(valid_profile("0day-x"));
  }

  #[test]
  fn label_parsing() {
    assert_eq!(session_from_label("runner-s_abcdefghij").unwrap(), SID);
    for bad in [
      "main",
      "runner-",
      "runner-s_short",
      "task-1-2",
      "runner-s_abcdefghij/..",
      "xrunner-s_abcdefghij",
      "runner-S_abcdefghij",
      "quick-add",
    ] {
      assert!(session_from_label(bad).is_err(), "{bad}");
    }
  }

  #[test]
  fn main_window_state_detail_is_whitelisted() {
    let mk = |kind: &str, detail: Value| {
      session_from_value(&session(SID, json!({"kind": kind, "since_ms": 4, "detail": detail, "exit_code": null}))).unwrap()
    };
    assert_eq!(mk("exited", json!("done")).state.detail.as_deref(), Some("done"));
    assert_eq!(mk("exited", json!("interrupted")).state.detail.as_deref(), Some("interrupted"));
    assert_eq!(mk("blocked", json!("Allow rm -rf / ? [y/n]")).state.detail, None);
    assert_eq!(mk("exited", json!("crashed hard")).state.detail, None);
    assert_eq!(mk("working", Value::Null).state.detail, None);
    let text = state_from_value(&json!({"kind": "blocked", "since_ms": 1, "detail": "Allow?\x1b[2J"}), Detail::Text).unwrap();
    assert_eq!(text.detail.as_deref(), Some("Allow?[2J"));
  }

  #[test]
  fn session_text_is_sanitized_and_ids_validated() {
    let mut v = session(SID, json!({"kind": "working", "since_ms": 1}));
    v["name"] = json!(format!("n\x1b[31m{}", "x".repeat(300)));
    v["profile"] = json!("bad profile!");
    let s = session_from_value(&v).unwrap();
    assert_eq!(s.name.chars().count(), 120);
    assert!(!s.name.contains('\x1b'));
    assert_eq!(s.profile, None);
    let mut bad = v.clone();
    bad["id"] = json!("s_../..");
    assert!(session_from_value(&bad).is_none());
    assert_eq!(s.task, Some(TaskRef { workspace_id: -1, task_id: 7 }));
  }

  #[test]
  fn serializes_camel_case() {
    let s = session_from_value(&session(SID, json!({"kind": "exited", "since_ms": 4, "detail": "done"}))).unwrap();
    let v = serde_json::to_value(&s).unwrap();
    assert_eq!(v["createdMs"], 1000);
    assert_eq!(v["task"], json!({"workspaceId": -1, "taskId": 7}));
    assert_eq!(v["state"], json!({"kind": "exited", "sinceMs": 4, "detail": "done"}));
    let mut with_code = session(SID, json!({"kind": "exited", "since_ms": 4, "detail": "failed", "exit_code": 3}));
    with_code["state"]["exit_code"] = json!(3);
    let v = serde_json::to_value(session_from_value(&with_code).unwrap()).unwrap();
    assert_eq!(v["state"]["exitCode"], 3);
    let running = session(SID, json!({"kind": "working", "since_ms": 4, "detail": null, "exit_code": 9}));
    assert_eq!(session_from_value(&running).unwrap().state.exit_code, None);
  }

  #[test]
  fn list_create_kill_against_mock_runner() {
    let mut m = Mock::new("secret");
    m.sessions = vec![session(SID, json!({"kind": "working", "since_ms": 1, "detail": null}))];
    let (mut conn, server) = connected(m);
    let list = list_sessions(&mut conn).unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, SID);
    assert!(find_session(&mut conn, SID).unwrap().is_some());
    assert!(find_session(&mut conn, "s_zzzzzzzzzz").unwrap().is_none());
    let created = create_run(&mut conn, &start(-2, 9, "codex"), &all_caps()).unwrap();
    assert_eq!(created.task, Some(TaskRef { workspace_id: -2, task_id: 9 }));
    assert_eq!(created.profile.as_deref(), Some("codex"));
    kill_session(&mut conn, SID).unwrap();
    drop(conn);
    let log = server.join().unwrap();
    let create = log.requests.iter().find(|r| r["method"] == "runs.create").unwrap();
    let p = &create["params"];
    assert_eq!((&p["profile"], &p["task"], &p["cwd"], &p["name"]), (&json!("codex"), &json!({"workspace_id": -2, "task_id": 9}), &json!("/work/project"), &json!("Fix login")));
    assert_eq!((&p["cols"], &p["rows"]), (&json!(120), &json!(32)));
    let kill = log.requests.iter().find(|r| r["method"] == "sessions.kill").unwrap();
    assert_eq!(kill["params"], json!({"id": SID, "signal": "term"}));
  }

  #[test]
  fn a_very_long_folder_is_clipped_and_the_agent_is_named() {
    let folder = format!("/{}", "d".repeat(900));
    let run = build_start(&task("T", ""), (-2, 9), "claude-code", &folder, (1, 2), &caps::ALL.map(String::from)).unwrap();
    let m = confirm_run_message(&run);
    assert!(m.contains(&format!("\nProject folder: \"/{} \u{2026} (+501 characters)\"\n", "d".repeat(399))), "{m}");
    assert!(m.contains("\nAgent: Claude Code\n"));
  }

  #[test]
  fn run_name_is_a_short_printable_title() {
    assert_eq!(run_name("Fix the login", 7), "Fix the login");
    assert_eq!(run_name("  \x1b[31m\u{202e}  ", 7), "[31m");
    assert_eq!(run_name("\x07\n", 7), "Task 7");
    assert_eq!(run_name(&"é".repeat(200), 7).chars().count(), 64);
  }

  #[test]
  fn busy_is_a_retryable_string_and_other_errors_keep_their_code() {
    let mut m = Mock::new("secret");
    m.busy = true;
    let (mut conn, server) = connected(m);
    assert_eq!(conn.call("sessions.send", json!({"id": SID, "text": "x"})).unwrap_err(), ConnError::Failed("busy".into()));
    assert_eq!(conn.call("sessions.remove", json!({"id": SID})).unwrap_err().message(), "busy");
    assert!(conn.call("nope", Value::Null).unwrap_err().message().starts_with("unknown_method: "));
    drop(conn);
    server.join().unwrap();
  }

  #[test]
  fn runs_create_without_a_cwd_is_invalid_params_on_the_runner() {
    let (mut conn, server) = connected(Mock::new("secret"));
    let e = conn.call("runs.create", json!({"profile": "codex"})).unwrap_err();
    assert!(e.message().starts_with("invalid_params: "), "{e:?}");
    drop(conn);
    server.join().unwrap();
  }

  #[test]
  fn golden_session_info_and_event_parse() {
    let v: Value = serde_json::from_str(include_str!("../../../crates/tmgr-ipc/tests/fixtures/session_info.json")).unwrap();
    let s = session_from_value(&v).unwrap();
    assert_eq!((s.id.as_str(), s.name.as_str(), s.profile.as_deref()), ("s_abcdefghij", "fix login", Some("codex")));
    assert_eq!(s.task, Some(TaskRef { workspace_id: -3, task_id: 412 }));
    assert_eq!(s.state.kind, StateKind::Working);
    let notask: Value = serde_json::from_str(include_str!("../../../crates/tmgr-ipc/tests/fixtures/session_info_notask.json")).unwrap();
    assert_eq!(session_from_value(&notask).unwrap().task, None);
  }
}
