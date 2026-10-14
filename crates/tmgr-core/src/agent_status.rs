use crate::api::{map_error, unwrap_data, Client};
use crate::error::{Error, Result};
use crate::refs::TaskRef;
use crate::text::{sanitize_line, truncate_chars};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::{json, Map, Value};

pub const SCOPE: &str = "agent-status:write";
pub const STATES: [&str; 6] = ["working", "waiting", "blocked", "idle", "done", "exited"];
pub const DEFAULT_PROFILE: &str = "claude-code";
pub const UPLOAD_NOTE: &str = "Status upload is on. Each update sends the task, the state (working, waiting, blocked, idle, done, exited), the agent profile id, timestamps and this device's host label to your workspace. Terminal content is never sent.";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct TaskView {
    pub id: Option<u64>,
    pub key: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct UserView {
    pub id: Option<i64>,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SessionView {
    pub session_id: String,
    pub workspace_id: Option<i64>,
    pub task: Option<TaskView>,
    pub state: String,
    pub state_since: Option<String>,
    pub started_at: Option<String>,
    pub updated_at: Option<String>,
    pub ended_at: Option<String>,
    pub agent_profile: Option<String>,
    pub host_label: Option<String>,
    pub user: Option<UserView>,
    pub stale: bool,
}

pub struct PutBody<'a> {
    pub task: Option<&'a TaskRef>,
    pub workspace_id: i64,
    pub agent_profile: &'a str,
    pub state: &'a str,
    pub started_at: Option<&'a str>,
    pub state_since: Option<&'a str>,
}

impl PutBody<'_> {
    pub fn body(&self) -> Value {
        let mut m = Map::new();
        match self.task {
            Some(TaskRef::Key(k)) => {
                m.insert("task_key".into(), json!(k));
            }
            Some(TaskRef::Id(id)) => {
                m.insert("task_id".into(), json!(id));
            }
            None => {}
        }
        m.insert("workspace_id".into(), json!(self.workspace_id));
        m.insert("agent_profile".into(), json!(self.agent_profile));
        m.insert("state".into(), json!(self.state));
        if let Some(v) = self.started_at {
            m.insert("started_at".into(), json!(v));
        }
        if let Some(v) = self.state_since {
            m.insert("state_since".into(), json!(v));
        }
        Value::Object(m)
    }
}

pub fn validate_session_id(raw: &str) -> Result<&str> {
    let ok = (8..=64).contains(&raw.len())
        && raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if ok {
        Ok(raw)
    } else {
        Err(Error::usage("invalid session id").hint("8 to 64 characters: letters, digits, _ and -"))
    }
}

pub fn validate_profile(raw: &str) -> Result<&str> {
    let ok = (1..=32).contains(&raw.len())
        && raw
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if ok {
        Ok(raw)
    } else {
        Err(Error::usage("invalid --profile")
            .hint("1 to 32 characters: lowercase letters, digits and -"))
    }
}

pub fn validate_state(raw: &str) -> Result<&str> {
    STATES.iter().copied().find(|s| *s == raw).ok_or_else(|| {
        Error::usage("invalid --state").hint(format!("one of: {}", STATES.join(", ")))
    })
}

pub fn normalize_instant(raw: &str, flag: &str) -> Result<String> {
    DateTime::parse_from_rfc3339(raw.trim())
        .map(|d| {
            d.with_timezone(&Utc)
                .to_rfc3339_opts(SecondsFormat::Secs, true)
        })
        .map_err(|_| {
            Error::usage(format!("invalid {flag}"))
                .hint("an RFC 3339 time such as 2026-10-14T09:30:00Z")
        })
}

fn clip(s: &str, max: usize) -> String {
    truncate_chars(&sanitize_line(s), max).0
}

fn text(v: &Value, key: &str, max: usize) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(|s| clip(s, max))
        .filter(|s| !s.is_empty())
}

pub fn parse_session(v: &Value) -> Result<SessionView> {
    let session_id = text(v, "session_id", 64)
        .ok_or_else(|| Error::internal("unexpected agent status response from the server"))?;
    let task = v.get("task").filter(|t| t.is_object()).map(|t| TaskView {
        id: t.get("id").and_then(Value::as_u64),
        key: text(t, "key", 40),
    });
    let user = v.get("user").filter(|u| u.is_object()).map(|u| UserView {
        id: u.get("id").and_then(Value::as_i64),
        name: text(u, "name", 80),
    });
    Ok(SessionView {
        session_id,
        workspace_id: v.get("workspace_id").and_then(Value::as_i64),
        task,
        state: text(v, "state", 16).unwrap_or_else(|| "unknown".into()),
        state_since: text(v, "state_since", 40),
        started_at: text(v, "started_at", 40),
        updated_at: text(v, "updated_at", 40),
        ended_at: text(v, "ended_at", 40),
        agent_profile: text(v, "agent_profile", 32),
        host_label: text(v, "host_label", 80),
        user,
        stale: v.get("stale").and_then(Value::as_bool).unwrap_or(false),
    })
}

pub fn parse_list(body: Value) -> Result<Vec<SessionView>> {
    match unwrap_data(body) {
        Value::Array(a) => a.iter().map(parse_session).collect(),
        Value::Null => Ok(Vec::new()),
        _ => Err(Error::internal(
            "unexpected agent status response from the server",
        )),
    }
}

fn is_disabled(status: u16, body: &Value) -> bool {
    status == 403 && body.get("error").and_then(Value::as_str) == Some("feature_disabled")
}

fn fail(status: u16, body: &Value, scope: &str) -> Error {
    if status == 429 {
        let wait = body
            .get("retry_after_s")
            .and_then(Value::as_u64)
            .map(|s| format!("{s}s"))
            .unwrap_or_else(|| "a minute".into());
        return Error::conflict(format!("rate limited by the server; retry in {wait}"));
    }
    map_error(status, body, scope)
}

fn session_call(
    client: &Client,
    method: &str,
    path: &str,
    body: Option<&Value>,
) -> Result<Option<SessionView>> {
    let (status, resp) = client.call(method, path, body, true)?;
    if is_disabled(status, &resp) {
        return Ok(None);
    }
    if (200..300).contains(&status) {
        return parse_session(&unwrap_data(resp)).map(Some);
    }
    Err(fail(status, &resp, SCOPE))
}

pub fn put(client: &Client, session_id: &str, b: &PutBody) -> Result<Option<SessionView>> {
    session_call(
        client,
        "PUT",
        &format!("/api/agent-status/{session_id}"),
        Some(&b.body()),
    )
}

pub fn clear(client: &Client, session_id: &str) -> Result<Option<SessionView>> {
    session_call(
        client,
        "DELETE",
        &format!("/api/agent-status/{session_id}"),
        None,
    )
}

pub fn list(client: &Client, workspace_id: i64) -> Result<Option<Vec<SessionView>>> {
    let (status, body) = client.call(
        "GET",
        &format!("/api/workspaces/{workspace_id}/agent-status"),
        None,
        true,
    )?;
    if is_disabled(status, &body) {
        return Ok(None);
    }
    if (200..300).contains(&status) {
        return parse_list(body).map(Some);
    }
    Err(fail(status, &body, "tasks:read"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn put_bodies_are_snake_case_and_carry_one_task_field() {
        let key = TaskRef::Key("TM-512".into());
        let b = PutBody {
            task: Some(&key),
            workspace_id: 1,
            agent_profile: "claude-code",
            state: "working",
            started_at: None,
            state_since: Some("2026-10-14T09:30:00Z"),
        };
        assert_eq!(
            b.body(),
            json!({"task_key": "TM-512", "workspace_id": 1, "agent_profile": "claude-code",
                   "state": "working", "state_since": "2026-10-14T09:30:00Z"})
        );
        let id = TaskRef::Id(8974);
        let b = PutBody {
            task: Some(&id),
            workspace_id: 1,
            agent_profile: "codex",
            state: "idle",
            started_at: None,
            state_since: None,
        };
        assert_eq!(b.body()["task_id"], 8974);
        assert!(b.body().get("task_key").is_none());
    }

    #[test]
    fn grammars_match_the_servers() {
        assert!(validate_session_id("abcd1234").is_ok());
        assert!(validate_session_id("abc").is_err());
        assert!(validate_session_id("has space 1234").is_err());
        assert!(validate_session_id(&"a".repeat(65)).is_err());
        assert!(validate_profile("claude-code").is_ok());
        assert!(validate_profile("Claude").is_err());
        assert!(validate_profile(&"a".repeat(33)).is_err());
        for s in STATES {
            assert!(validate_state(s).is_ok());
        }
        assert!(validate_state("running").is_err());
    }

    #[test]
    fn instants_are_normalised_to_utc() {
        assert_eq!(
            normalize_instant("2026-10-14T11:30:00+02:00", "--since").unwrap(),
            "2026-10-14T09:30:00Z"
        );
        assert!(normalize_instant("yesterday", "--since").is_err());
    }

    #[test]
    fn sessions_are_parsed_and_sanitised() {
        let s = parse_session(&json!({
            "session_id": "sess-1234", "workspace_id": 1, "task": {"id": 9, "key": "TM-1"},
            "state": "working", "host_label": "box\u{1b}[31m", "stale": true,
            "user": {"id": 3, "name": "Ann"}, "ended_at": null
        }))
        .unwrap();
        assert_eq!(s.task.unwrap().key.as_deref(), Some("TM-1"));
        assert!(!s.host_label.unwrap().contains('\u{1b}'));
        assert!(s.stale);
        assert_eq!(s.ended_at, None);
        assert!(parse_session(&json!({"state": "idle"})).is_err());
        assert!(parse_session(&json!({"session_id": "x", "task": null}))
            .unwrap()
            .task
            .is_none());
    }

    #[test]
    fn rate_limits_name_the_wait_and_exit_6() {
        let e = fail(429, &json!({"retry_after_s": 17}), SCOPE);
        assert_eq!(e.exit_code(), 6);
        assert!(e.message.contains("17s"));
        assert!(fail(429, &Value::Null, SCOPE).message.contains("a minute"));
        let e = fail(403, &json!({"error": "insufficient_scope"}), SCOPE);
        assert_eq!(e.exit_code(), 5);
        assert!(e.hint.unwrap().ends_with("agent-status:write"));
    }

    #[test]
    fn the_disabled_answer_is_recognised() {
        assert!(is_disabled(403, &json!({"error": "feature_disabled"})));
        assert!(!is_disabled(403, &json!({"error": "insufficient_scope"})));
        assert!(!is_disabled(500, &json!({"error": "feature_disabled"})));
    }
}
