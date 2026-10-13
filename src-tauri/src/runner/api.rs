use std::io::{Read, Write};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::conn::{valid_session_id, Conn, ConnError};
use super::sanitize;

pub const PROFILES: &[(&str, &str)] = &[("claude-code", "Claude Code"), ("codex", "Codex")];
const TITLE_MAX: usize = 120;
const NAME_MAX: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateKind {
  Starting,
  Working,
  Waiting,
  Blocked,
  Exited,
}

#[derive(Debug, Clone, Deserialize)]
struct WireState {
  kind: StateKind,
  #[serde(default)]
  since_ms: u64,
  #[serde(default)]
  detail: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct WireTask {
  workspace_id: i64,
  task_id: i64,
}

#[derive(Debug, Clone, Deserialize)]
struct WireSession {
  id: String,
  #[serde(default)]
  name: String,
  #[serde(default)]
  profile: Option<String>,
  #[serde(default)]
  task: Option<WireTask>,
  state: WireState,
  #[serde(default)]
  created_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunnerState {
  pub kind: StateKind,
  pub since_ms: u64,
  pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRef {
  pub workspace_id: i64,
  pub task_id: i64,
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

fn state_from_wire(s: &WireState, detail: Detail) -> RunnerState {
  let detail = s.detail.as_deref().and_then(|d| match detail {
    Detail::ExitOnly => ["done", "failed", "interrupted"].contains(&d).then(|| d.to_owned()),
    Detail::Text => Some(sanitize::text(d, 200)).filter(|d| !d.is_empty()),
  });
  RunnerState { kind: s.kind, since_ms: s.since_ms, detail }
}

pub fn state_from_value(v: &Value, detail: Detail) -> Option<RunnerState> {
  let s: WireState = serde_json::from_value(v.clone()).ok()?;
  Some(state_from_wire(&s, detail))
}

pub fn valid_profile(p: &str) -> bool {
  let b = p.as_bytes();
  (1..=32).contains(&b.len())
    && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
    && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'-')
}

pub fn session_from_value(v: &Value) -> Option<RunnerSession> {
  let w: WireSession = serde_json::from_value(v.clone()).ok()?;
  if !valid_session_id(&w.id) {
    return None;
  }
  Some(RunnerSession {
    name: sanitize::text(&w.name, NAME_MAX),
    profile: w.profile.filter(|p| valid_profile(p)),
    task: w.task.map(|t| TaskRef { workspace_id: t.workspace_id, task_id: t.task_id }),
    state: state_from_wire(&w.state, Detail::ExitOnly),
    created_ms: w.created_ms,
    id: w.id,
  })
}

pub fn sessions_from_value(v: &Value) -> Result<Vec<RunnerSession>, ConnError> {
  let arr = v.as_array().ok_or_else(|| ConnError::Failed("unexpected reply from runner".to_owned()))?;
  Ok(arr.iter().filter_map(session_from_value).collect())
}

pub fn list_sessions<S: Read + Write>(conn: &mut Conn<S>) -> Result<Vec<RunnerSession>, ConnError> {
  sessions_from_value(&conn.call("sessions.list", Value::Null)?)
}

pub fn find_session<S: Read + Write>(conn: &mut Conn<S>, id: &str) -> Result<Option<Value>, ConnError> {
  let v = conn.call("sessions.list", Value::Null)?;
  Ok(v.as_array().and_then(|a| a.iter().find(|s| s["id"] == id).cloned()))
}

pub struct StartRun {
  pub workspace_id: i64,
  pub task_id: i64,
  pub profile: String,
  pub persona_id: Option<i64>,
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
  if r.persona_id.is_some_and(|p| p <= 0) {
    return Err("invalid persona".to_owned());
  }
  Ok(())
}

pub fn profile_label(id: &str) -> &str {
  PROFILES.iter().find(|(p, _)| *p == id).map_or(id, |(_, l)| l)
}

pub fn display_text(s: &str) -> String {
  sanitize::text(s, TITLE_MAX)
}

pub fn confirm_run_message(task_id: i64, title: &str, profile: &str, persona: Option<&str>) -> String {
  let mut m = format!("Task #{task_id}: {}\nAgent: {}\n", display_text(title), profile_label(profile));
  if let Some(p) = persona.map(display_text).filter(|p| !p.is_empty()) {
    m.push_str(&format!("Persona: {p}\n"));
  }
  m.push_str("Permission prompts: on");
  m
}

pub fn create_run<S: Read + Write>(conn: &mut Conn<S>, r: &StartRun) -> Result<RunnerSession, ConnError> {
  let mut params = json!({
    "task": {"workspace_id": r.workspace_id, "task_id": r.task_id},
    "profile": r.profile,
    "cols": 120,
    "rows": 32,
  });
  if let Some(p) = r.persona_id {
    params["persona_id"] = json!(p);
  }
  let v = conn.call("runs.create", params)?;
  session_from_value(&v).ok_or_else(|| ConnError::Failed("unexpected reply from runner".to_owned()))
}

pub fn kill_session<S: Read + Write>(conn: &mut Conn<S>, id: &str) -> Result<(), ConnError> {
  conn.call("sessions.kill", json!({"id": id, "signal": "TERM"})).map(drop)
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
  use std::os::unix::net::UnixStream;

  const SID: &str = "s_abcdefghij";

  fn start(ws: i64, task: i64, profile: &str, persona: Option<i64>) -> StartRun {
    StartRun { workspace_id: ws, task_id: task, profile: profile.into(), persona_id: persona }
  }

  fn connected(mock: Mock) -> (Conn<UnixStream>, std::thread::JoinHandle<mock::Log>) {
    let (c, s) = UnixStream::pair().unwrap();
    let secret = mock.secret.clone();
    let h = mock::spawn(s, mock);
    (Conn::handshake(c, &secret, "desktop").unwrap().0, h)
  }

  #[test]
  fn start_validation() {
    assert!(validate_start(&start(-1, 5, "claude-code", None)).is_ok());
    assert!(validate_start(&start(-3, 5, "codex", Some(2))).is_ok());
    assert_eq!(validate_start(&start(12, 5, "codex", None)).unwrap_err(), "Agent runs are available only for local workspaces");
    assert!(validate_start(&start(0, 5, "codex", None)).is_err());
    assert!(validate_start(&start(-1, 0, "codex", None)).is_err());
    assert!(validate_start(&start(-1, -4, "codex", None)).is_err());
    assert!(validate_start(&start(-1, 5, "codex", Some(0))).is_err());
    for bad in ["", "Codex", "-x", "a b", "a/b", "../x", "a;rm", "x".repeat(33).as_str(), "é"] {
      assert!(validate_start(&start(-1, 5, bad, None)).is_err(), "{bad}");
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
      session_from_value(&session(SID, json!({"kind": kind, "since_ms": 4, "detail": detail, "exit_code": 0}))).unwrap()
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
    let created = create_run(&mut conn, &start(-2, 9, "codex", Some(3))).unwrap();
    assert_eq!(created.task, Some(TaskRef { workspace_id: -2, task_id: 9 }));
    assert_eq!(created.profile.as_deref(), Some("codex"));
    kill_session(&mut conn, SID).unwrap();
    drop(conn);
    let log = server.join().unwrap();
    let create = log.requests.iter().find(|r| r["method"] == "runs.create").unwrap();
    assert_eq!(
      create["params"],
      json!({"task": {"workspace_id": -2, "task_id": 9}, "profile": "codex", "persona_id": 3, "cols": 120, "rows": 32})
    );
    let kill = log.requests.iter().find(|r| r["method"] == "sessions.kill").unwrap();
    assert_eq!(kill["params"], json!({"id": SID, "signal": "TERM"}));
  }

  #[test]
  fn confirm_message_shows_what_will_run() {
    let m = confirm_run_message(7, &format!("Fix\x1b[2J bug {}", "t".repeat(200)), "claude-code", Some("Reviewer"));
    assert!(m.contains("Agent: Claude Code"));
    assert!(m.contains("Persona: Reviewer"));
    assert!(m.ends_with("Permission prompts: on"));
    assert!(!m.contains('\x1b'));
    assert!(m.lines().next().unwrap().chars().count() < 160);
  }
}
