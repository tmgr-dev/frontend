use crate::api::{map_error, unwrap_data, Client};
use crate::error::{Error, Result};
use crate::runstate::{split_commit, MAX_COMMITS};
use crate::text::{sanitize_line, truncate_chars};
use serde::Serialize;
use serde_json::{json, Map, Value};

pub const SCOPE: &str = "agent-work:write";

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct RunView {
    pub id: u64,
    pub task_id: Option<u64>,
    pub status: String,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub branch: Option<String>,
    pub pr_url: Option<String>,
    pub commits: Vec<String>,
    pub duration_s: Option<i64>,
}

pub enum Outcome {
    Run(RunView),
    Disabled,
}

#[derive(Debug, Default, Clone)]
pub struct Tests {
    pub passed: Option<u32>,
    pub failed: Option<u32>,
    pub command: Option<String>,
}

#[derive(Debug, Default, Clone)]
pub struct Progress {
    pub branch: Option<String>,
    pub summary: Option<String>,
    pub pr_url: Option<String>,
    pub commits: Option<Vec<String>>,
    pub tests: Option<Tests>,
}

impl Progress {
    pub fn body(&self) -> Value {
        let mut m = Map::new();
        if let Some(b) = &self.branch {
            m.insert("branch".into(), json!(clip(b, 255)));
        }
        if let Some(s) = &self.summary {
            m.insert("summary".into(), json!(truncate_chars(s, 10_000).0));
        }
        if let Some(p) = &self.pr_url {
            m.insert("pr_url".into(), json!(clip(p, 512)));
        }
        if let Some(c) = &self.commits {
            let list: Vec<Value> = c
                .iter()
                .rev()
                .take(MAX_COMMITS)
                .rev()
                .map(|line| {
                    let (sha, msg) = split_commit(line);
                    json!({"sha": sha, "message": clip(msg, 300)})
                })
                .collect();
            m.insert("commits".into(), Value::Array(list));
        }
        if let Some(t) = &self.tests {
            let mut tm = Map::new();
            if let Some(p) = t.passed {
                tm.insert("passed".into(), json!(p));
            }
            if let Some(f) = t.failed {
                tm.insert("failed".into(), json!(f));
            }
            if let Some(c) = &t.command {
                tm.insert("command".into(), json!(clip(c, 300)));
            }
            m.insert("tests".into(), Value::Object(tm));
        }
        Value::Object(m)
    }
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

pub fn parse_run(body: Value) -> Result<RunView> {
    let v = unwrap_data(body);
    let id = v
        .get("id")
        .and_then(Value::as_u64)
        .ok_or_else(|| Error::internal("unexpected agent work response from the server"))?;
    let commits = v
        .get("commits")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|c| {
                    let sha = c.get("sha").and_then(Value::as_str)?;
                    let msg = c.get("message").and_then(Value::as_str).unwrap_or("");
                    Some(clip(&format!("{sha} {msg}"), 300).trim().to_string())
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(RunView {
        id,
        task_id: v.get("task_id").and_then(Value::as_u64),
        status: text(&v, "status", 20).unwrap_or_else(|| "unknown".into()),
        agent: text(&v, "agent", 64),
        model: text(&v, "model", 128),
        branch: text(&v, "branch", 255),
        pr_url: text(&v, "pr_url", 512),
        commits,
        duration_s: v.get("duration_seconds").and_then(Value::as_i64),
    })
}

fn is_disabled(status: u16, body: &Value) -> bool {
    status == 403
        && (body.get("error").and_then(Value::as_str) == Some("feature_disabled")
            || body.get("feature").and_then(Value::as_str) == Some("agent_work"))
}

fn finish_call(status: u16, body: Value) -> Result<Outcome> {
    if is_disabled(status, &body) {
        return Ok(Outcome::Disabled);
    }
    if (200..300).contains(&status) {
        return parse_run(body).map(Outcome::Run);
    }
    Err(map_error(status, &body, SCOPE))
}

pub fn run_commits(client: &Client, task_id: u64, run_id: u64) -> Result<Vec<String>> {
    let (status, body) = client.call(
        "GET",
        &format!("/api/tasks/{task_id}/agent-work"),
        None,
        true,
    )?;
    if !(200..300).contains(&status) {
        return Err(map_error(status, &body, "tasks:read"));
    }
    let data = unwrap_data(body);
    let run = data
        .get("runs")
        .and_then(Value::as_array)
        .and_then(|runs| {
            runs.iter()
                .find(|r| r.get("id").and_then(Value::as_u64) == Some(run_id))
        })
        .cloned()
        .ok_or_else(|| {
            Error::not_found(format!("run {run_id} not found on that task"))
                .hint("check --task and --run")
        })?;
    Ok(parse_run(run)?.commits)
}

pub struct StartBody<'a> {
    pub agent: &'a str,
    pub model: Option<&'a str>,
    pub session_id: Option<&'a str>,
    pub branch: Option<&'a str>,
}

pub fn start(client: &Client, task_id: u64, b: &StartBody) -> Result<Outcome> {
    let mut m = Map::new();
    m.insert("agent".into(), json!(clip(b.agent, 64)));
    if let Some(v) = b.model {
        m.insert("model".into(), json!(clip(v, 128)));
    }
    if let Some(v) = b.session_id {
        m.insert("session_id".into(), json!(clip(v, 191)));
    }
    if let Some(v) = b.branch {
        m.insert("branch".into(), json!(clip(v, 255)));
    }
    let (status, body) = client.call(
        "POST",
        &format!("/api/tasks/{task_id}/agent-work"),
        Some(&Value::Object(m)),
        true,
    )?;
    finish_call(status, body)
}

pub fn update(client: &Client, run_id: u64, p: &Progress, refresh: bool) -> Result<Outcome> {
    let (status, body) = client.call(
        "PATCH",
        &format!("/api/agent-work/{run_id}"),
        Some(&p.body()),
        refresh,
    )?;
    finish_call(status, body)
}

pub fn finish(client: &Client, run_id: u64, run_status: &str, p: &Progress) -> Result<Outcome> {
    let mut body = p.body();
    if let Value::Object(m) = &mut body {
        m.insert("status".into(), json!(run_status));
        m.remove("branch");
    }
    let (status, resp) = client.call(
        "POST",
        &format!("/api/agent-work/{run_id}/finish"),
        Some(&body),
        true,
    )?;
    finish_call(status, resp)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_bodies_are_snake_case_and_carry_only_what_is_set() {
        assert_eq!(Progress::default().body(), json!({}));
        let p = Progress {
            branch: Some("tm-1-x".into()),
            pr_url: Some("https://example.com/pr/1".into()),
            commits: Some(vec!["abc1234 fix: a b".into()]),
            tests: Some(Tests {
                passed: Some(3),
                failed: Some(0),
                command: Some("cargo test".into()),
            }),
            summary: None,
        };
        assert_eq!(
            p.body(),
            json!({
                "branch": "tm-1-x",
                "pr_url": "https://example.com/pr/1",
                "commits": [{"sha": "abc1234", "message": "fix: a b"}],
                "tests": {"passed": 3, "failed": 0, "command": "cargo test"}
            })
        );
    }

    #[test]
    fn commit_lists_are_capped_at_the_servers_limit() {
        let many: Vec<String> = (0..300).map(|i| format!("{i:07x} c{i}")).collect();
        let body = Progress {
            commits: Some(many),
            ..Default::default()
        }
        .body();
        let list = body["commits"].as_array().unwrap();
        assert_eq!(list.len(), 200);
        assert_eq!(list[199]["sha"], "000012b");
    }

    #[test]
    fn run_views_parse_the_servers_snake_case_shape() {
        let v = parse_run(json!({
            "id": 41, "task_id": 8974, "status": "running", "agent": "claude-code",
            "branch": "tm-1", "pr_url": null, "duration_seconds": 12,
            "commits": [{"sha": "abc1234", "message": "x"}]
        }))
        .unwrap();
        assert_eq!(v.id, 41);
        assert_eq!(v.commits, vec!["abc1234 x"]);
        assert_eq!(v.duration_s, Some(12));
        assert!(parse_run(json!({"status": "running"})).is_err());
    }

    #[test]
    fn the_disabled_answer_is_not_an_error() {
        let body = json!({"error": "feature_disabled", "feature": "agent_work", "message": "x"});
        assert!(matches!(finish_call(403, body).unwrap(), Outcome::Disabled));
        let scope = json!({"error": "insufficient_scope"});
        assert_eq!(finish_call(403, scope).err().unwrap().exit_code(), 5);
    }
}
