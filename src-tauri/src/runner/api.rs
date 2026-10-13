use serde::Serialize;
use serde_json::Value;
use tmgr_ipc::api::{caps, method, limits, CwdPin, RunCreateReq, SessionInfo, State};

pub use tmgr_ipc::api::StateKind;

use super::bundle::{self, Prepared};
use super::task_lookup::TaskInfo;
use super::conn::{valid_session_id, Conn, ConnError, Welcome, UPDATE_RUNNER};
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
  pub cwd_pin: Option<(u64, u64)>,
  pub bundle: Option<Prepared>,
}

pub fn run_name(title: &str, task_id: i64) -> String {
  let name = sanitize::text(title, RUN_NAME_MAX);
  if name.is_empty() {
    format!("Task {task_id}")
  } else {
    name
  }
}

pub fn validate_start(r: &StartRun) -> Result<(), String> {
  if r.workspace_id >= 0 {
    return Err("Agent runs are available only for local workspaces".to_owned());
  }
  if r.task_id <= 0 {
    return Err("invalid task".to_owned());
  }
  if !valid_profile(&r.profile) {
    return Err("invalid agent profile".to_owned());
  }
  Ok(())
}

pub fn attach_approval(r: &mut StartRun, task: &TaskInfo, folder_id: (u64, u64), runner_caps: &[String]) -> Result<(), String> {
  let has = |cap: &str| runner_caps.iter().any(|c| c == cap);
  r.cwd_pin = has(caps::CWD_PIN_V1).then_some(folder_id);
  r.bundle = if has(caps::RUN_BUNDLE_V1) { Some(bundle::prepare(task, r.workspace_id, r.task_id, &r.profile, &r.cwd)?) } else { None };
  Ok(())
}

pub fn profile_label(id: &str) -> &str {
  PROFILES.iter().find(|(p, _)| *p == id).map_or(id, |(_, l)| l)
}

pub fn display_text(s: &str) -> String {
  sanitize::text(s, TITLE_MAX)
}

pub fn confirm_run_message(workspace: &str, task_id: i64, title: &str, folder: &str, profile: &str, bundle: Option<&str>) -> String {
  let bundle = bundle.map(|b| format!("\nRun bundle: {b}")).unwrap_or_default();
  format!(
    "Workspace: {}\nTask #{task_id}: {}\nProject folder: \"{}\"\nAgent: {}{bundle}\nPermission prompts: on",
    display_text(workspace),
    display_text(title),
    folder,
    profile_label(profile)
  )
}

pub fn create_run(conn: &mut Conn, r: &StartRun, welcome: &Welcome) -> Result<RunnerSession, ConnError> {
  if r.bundle.is_some() && !welcome.has(caps::RUN_BUNDLE_V1) {
    return Err(ConnError::Failed(UPDATE_RUNNER.to_owned()));
  }
  let req = RunCreateReq {
    profile: r.profile.clone(),
    task: Some(tmgr_ipc::api::TaskRef { workspace_id: r.workspace_id, task_id: r.task_id }),
    cwd: r.cwd.clone(),
    name: Some(r.name.clone()),
    model: None,
    cols: RUN_COLS.min(limits::MAX_COLS),
    rows: RUN_ROWS.min(limits::MAX_ROWS),
    bundle: r.bundle.as_ref().map(|p| p.bundle.clone()),
    bundle_sha256: r.bundle.as_ref().map(|p| p.sha256.clone()),
    cwd_pin: r.cwd_pin.filter(|_| welcome.has(caps::CWD_PIN_V1)).map(|(dev, ino)| CwdPin { dev, ino }),
    origin: None,
  };
  let params = serde_json::to_value(&req).map_err(|e| ConnError::Failed(e.to_string()))?;
  let v = conn.call(method::RUNS_CREATE, params)?;
  session_from_value(&v).ok_or_else(|| ConnError::Failed("unexpected reply from runner".to_owned()))
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
  use crate::runner::mock::{self, session, Mock};
  use serde_json::json;
  use std::os::unix::net::UnixStream;

  const SID: &str = "s_abcdefghij";

  fn start(ws: i64, task: i64, profile: &str) -> StartRun {
    StartRun { workspace_id: ws, task_id: task, profile: profile.into(), cwd: "/work/project".into(), name: "fix login".into(), cwd_pin: None, bundle: None }
  }

  fn all_caps() -> Welcome {
    Welcome { runner_version: "0.1.0".into(), caps: caps::ALL.map(String::from).to_vec() }
  }

  fn prepared() -> Prepared {
    use crate::runner::bundle::prepare;
    use crate::runner::task_lookup::TaskInfo;
    let task = TaskInfo { workspace: "W".into(), title: "Fix login".into(), description: "body".into(), comments: Vec::new() };
    prepare(&task, -2, 9, "codex", "/work/project").unwrap()
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

  #[test]
  fn approval_attaches_only_what_the_runner_understands() {
    let task = TaskInfo { workspace: "W".into(), title: "T".into(), description: "b".into(), comments: Vec::new() };
    let caps = |c: &[&str]| c.iter().map(|c| c.to_string()).collect::<Vec<_>>();
    let mut r = start(-2, 9, "codex");
    attach_approval(&mut r, &task, (1, 2), &caps(&["runs.create.v1"])).unwrap();
    assert!(r.cwd_pin.is_none() && r.bundle.is_none());
    attach_approval(&mut r, &task, (1, 2), &caps(&["cwd_pin.v1"])).unwrap();
    assert!(r.cwd_pin == Some((1, 2)) && r.bundle.is_none());
    attach_approval(&mut r, &task, (1, 2), &caps(&["run_bundle.v1"])).unwrap();
    assert!(r.cwd_pin.is_none());
    let b = r.bundle.unwrap();
    assert_eq!((b.bundle.launch.cwd.as_str(), b.bundle.launch.profile.as_str()), ("/work/project", "codex"));
    assert_eq!(b.bundle.task, tmgr_ipc::api::TaskRef { workspace_id: -2, task_id: 9 });
  }

  #[test]
  fn pin_and_bundle_are_sent_when_the_runner_advertises_them() {
    let p = prepared();
    let run = StartRun { cwd_pin: Some((16, 99)), bundle: Some(p.clone()), ..start(-2, 9, "codex") };
    let (created, log) = run_request(&all_caps(), Mock::new("secret"), run);
    let params = sent(&log);
    assert_eq!(params["cwd_pin"], json!({"dev": 16, "ino": 99}));
    assert_eq!(params["bundle_sha256"], json!(p.sha256));
    assert_eq!(params["bundle"], serde_json::to_value(&p.bundle).unwrap());
    assert!(params.get("origin").is_none());
    created.unwrap();
  }

  #[test]
  fn without_the_caps_no_pin_is_sent_and_a_bundle_is_refused() {
    let none = Welcome { runner_version: "0.1.0".into(), caps: vec!["runs.create.v1".into()] };
    let run = StartRun { cwd_pin: Some((16, 99)), ..start(-2, 9, "codex") };
    let (created, log) = run_request(&none, Mock::new("secret"), run);
    created.unwrap();
    let params = sent(&log);
    for key in ["cwd_pin", "bundle", "bundle_sha256", "origin"] {
      assert!(params.get(key).is_none(), "{key}");
    }
    let run = StartRun { bundle: Some(prepared()), ..start(-2, 9, "codex") };
    let (refused, log) = run_request(&none, Mock::new("secret"), run);
    assert_eq!(refused.unwrap_err().message(), UPDATE_RUNNER);
    assert!(log.requests.iter().all(|r| r["method"] != "runs.create"));
  }

  #[test]
  fn a_mismatched_pin_is_rejected_by_the_runner() {
    let run = StartRun { cwd_pin: Some((16, 100)), ..start(-2, 9, "codex") };
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
    assert!(validate_start(&start(-1, 5, "claude-code")).is_ok());
    assert!(validate_start(&start(-3, 5, "codex")).is_ok());
    assert_eq!(validate_start(&start(12, 5, "codex")).unwrap_err(), "Agent runs are available only for local workspaces");
    assert!(validate_start(&start(0, 5, "codex")).is_err());
    assert!(validate_start(&start(-1, 0, "codex")).is_err());
    assert!(validate_start(&start(-1, -4, "codex")).is_err());
    for bad in ["", "Codex", "-x", "a b", "a/b", "../x", "a;rm", "x".repeat(33).as_str(), "é"] {
      assert!(validate_start(&start(-1, 5, bad)).is_err(), "{bad}");
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
    assert_eq!(
      create["params"],
      json!({"profile": "codex", "task": {"workspace_id": -2, "task_id": 9}, "cwd": "/work/project", "name": "fix login", "cols": 120, "rows": 32})
    );
    let kill = log.requests.iter().find(|r| r["method"] == "sessions.kill").unwrap();
    assert_eq!(kill["params"], json!({"id": SID, "signal": "term"}));
  }

  #[test]
  fn confirm_message_shows_the_full_folder_quoted() {
    let folder = format!("/{}", "d".repeat(900));
    let m = confirm_run_message("W", 1, "T", &folder, "codex", None);
    assert!(m.contains(&format!("\nProject folder: \"{folder}\"\n")));
  }

  #[test]
  fn confirm_message_shows_what_will_run() {
    let m = confirm_run_message(
      "Work\u{202e}space",
      7,
      &format!("Fix\x1b[2J bug {}", "t".repeat(200)),
      "/work/project",
      "claude-code",
      Some("0123456789ab, 3 comments"),
    );
    assert!(m.starts_with("Workspace: Workspace\n"));
    assert!(m.contains("Task #7: Fix[2J bug"));
    assert!(m.contains("\nProject folder: \"/work/project\"\n"));
    assert!(m.contains("Agent: Claude Code"));
    assert!(m.contains("\nAgent: Claude Code\nRun bundle: 0123456789ab, 3 comments\nPermission prompts: on"));
    assert!(m.ends_with("Permission prompts: on"));
    assert!(!confirm_run_message("W", 1, "T", "/f", "codex", None).contains("Run bundle"));
    assert!(!m.contains('\x1b'));
    assert!(m.lines().nth(1).unwrap().chars().count() < 160);
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
