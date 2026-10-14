use std::collections::HashMap;

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, NaiveDate};
use rusqlite::types::Value as Sql;
use serde_json::{json, Value};

use super::{int, text};
use crate::context::Context;
use crate::db::{execute, select, select_one, Row};
use crate::js;
use crate::router::{coded, http, not_found, sql, Handled, Outcome, Request};
use crate::serialize::{col, task_key};

const DEFAULT_LIMIT: i64 = 50;
const MAX_LIMIT: i64 = 200;

fn require_agent_work(req: &Request<'_>) -> Result<(), Outcome> {
    if super::catalog::agent_work_enabled(req)? {
        return Ok(());
    }
    let message = "Agent work is disabled in this workspace";
    Err(Outcome::Http {
        status: 403,
        body: json!({ "message": message, "error": "feature_disabled", "feature": "agent_work" }),
    })
}

fn epoch_of(iso: &Value) -> Option<i64> {
    DateTime::parse_from_rfc3339(iso.as_str()?)
        .ok()
        .map(|d| d.timestamp())
}

fn duration(row: &Row, ctx: &Context) -> Value {
    let stored = col(row, "duration_seconds");
    if !stored.is_null() {
        return stored.clone();
    }
    if col(row, "status").as_str() != Some("running") {
        return json!(0);
    }
    match epoch_of(col(row, "started_at")) {
        Some(start) => json!((ctx.epoch() - start).max(0)),
        None => Value::Null,
    }
}

fn persona_names(req: &Request<'_>, rows: &[Row]) -> Result<HashMap<String, Value>, Outcome> {
    let mut uuids: Vec<String> = Vec::new();
    for row in rows {
        if col(row, "actor_kind").as_str() == Some("persona") {
            let id = js::string(col(row, "actor_id"));
            if !uuids.contains(&id) {
                uuids.push(id);
            }
        }
    }
    if uuids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = vec!["?"; uuids.len()].join(",");
    let params: Vec<Sql> = uuids.iter().map(|u| text(u)).collect();
    let found = sql(select(
        req.conn,
        &format!("SELECT uuid, name FROM personas WHERE uuid IN ({placeholders})"),
        &params,
    ))?;
    Ok(found
        .iter()
        .map(|r| (js::string(col(r, "uuid")), col(r, "name").clone()))
        .collect())
}

fn actor_json(row: &Row, ctx: &Context, names: &HashMap<String, Value>) -> Value {
    let id = col(row, "actor_id");
    match col(row, "actor_kind").as_str() {
        Some("persona") => {
            let mut actor = json!({ "kind": "persona", "id": id });
            if let Some(name) = names.get(&js::string(id)) {
                actor["name"] = name.clone();
            }
            actor["owner"] = json!({ "id": ctx.user.id.to_string(), "name": ctx.user.name });
            actor
        }
        Some("plugin") => json!({ "kind": "plugin", "id": id }),
        _ => json!({ "kind": "user", "id": id, "name": ctx.user.name }),
    }
}

fn run_json(row: &Row, ctx: &Context, names: &HashMap<String, Value>) -> Value {
    json!({
        "id": col(row, "id"),
        "task_id": col(row, "task_id"),
        "workspace_id": ctx.workspace.manifest.id,
        "user_id": ctx.user.id,
        "agent": col(row, "agent"),
        "model": col(row, "model"),
        "session_id": col(row, "session_id"),
        "branch": col(row, "branch"),
        "status": col(row, "status"),
        "started_at": col(row, "started_at"),
        "ended_at": col(row, "ended_at"),
        "duration_seconds": duration(row, ctx),
        "summary": col(row, "summary"),
        "pr_url": col(row, "pr_url"),
        "commits": js::parse_json(col(row, "commits"), json!([])),
        "tests": js::parse_json(col(row, "tests"), Value::Null),
        "version": col(row, "version"),
        "actor": actor_json(row, ctx, names),
    })
}

pub(super) fn for_task(req: &Request<'_>) -> Handled {
    require_agent_work(req)?;
    let task = super::tasks::require_task(req, req.id("id"))?;
    let task_id = col(&task, "id").as_i64().unwrap_or(0);
    let rows = sql(select(
        req.conn,
        "SELECT * FROM agent_work_runs WHERE task_id = ? ORDER BY started_at DESC, id DESC LIMIT 50",
        &[int(task_id)],
    ))?;
    let running_rows = sql(select(
        req.conn,
        "SELECT started_at FROM agent_work_runs WHERE task_id = ? AND status = 'running'",
        &[int(task_id)],
    ))?;
    let finished = sql(select_one(
        req.conn,
        "SELECT COALESCE(SUM(duration_seconds), 0) AS finished FROM agent_work_runs
				 WHERE task_id = ? AND status <> 'running'",
        &[int(task_id)],
    ))?
    .map_or(0, |r| col(&r, "finished").as_i64().unwrap_or(0));
    let now = req.ctx.epoch();
    let running: i64 = running_rows
        .iter()
        .filter_map(|r| epoch_of(col(r, "started_at")))
        .map(|start| (now - start).max(0))
        .sum();
    let names = persona_names(req, &rows)?;
    let common = col(&task, "common_time").as_i64().unwrap_or(0);
    let start = col(&task, "start_time").as_i64().unwrap_or(0);
    Ok(json!({
        "runs": rows.iter().map(|r| run_json(r, req.ctx, &names)).collect::<Vec<_>>(),
        "totals": {
            "agent_seconds": finished + running,
            "human_seconds": common + if start > 0 { (now - start).max(0) } else { 0 },
            "human_timer_running": start > 0,
        },
    }))
}

/// `since` (inclusive) / `until` (exclusive) as an ISO instant, or a UTC date; an `until` date covers its whole day.
fn instant(raw: Option<&str>, end_of_day: bool, field: &str) -> Result<Option<i64>, Outcome> {
    let Some(text) = raw.map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    if let Ok(at) = DateTime::parse_from_rfc3339(text) {
        return Ok(Some(at.timestamp()));
    }
    let date = NaiveDate::parse_from_str(text, "%Y-%m-%d").map_err(|_| {
        http(
            422,
            format!("{field} must be an ISO-8601 instant or a YYYY-MM-DD date"),
        )
    })?;
    let day = if end_of_day {
        date.succ_opt().unwrap_or(date)
    } else {
        date
    };
    Ok(Some(
        day.and_hms_opt(0, 0, 0)
            .unwrap_or_default()
            .and_utc()
            .timestamp(),
    ))
}

fn decode_cursor(raw: &str) -> Result<(i64, i64), Outcome> {
    let invalid = || http(422, "invalid cursor");
    let bytes = URL_SAFE_NO_PAD
        .decode(raw.trim_end_matches('='))
        .map_err(|_| invalid())?;
    let text = String::from_utf8(bytes).map_err(|_| invalid())?;
    let (at, id) = text.split_once(':').ok_or_else(invalid)?;
    Ok((
        at.parse().map_err(|_| invalid())?,
        id.parse().map_err(|_| invalid())?,
    ))
}

pub(crate) fn normalize_agent(raw: &str) -> Result<String, Outcome> {
    let normalized = raw
        .trim()
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-");
    if normalized.is_empty() {
        return Err(http(422, "agent is required"));
    }
    if normalized.chars().count() > 64 {
        return Err(http(422, "agent must be at most 64 characters"));
    }
    Ok(normalized)
}

/// Rust-only: agent work of the whole workspace for `tmgr log`, shaped like the Java `WorkspaceAgentWorkPage`.
pub(super) fn for_workspace(req: &Request<'_>) -> Handled {
    let q = &req.query;
    let since = instant(q.get("since"), false, "since")?;
    let until = instant(q.get("until"), true, "until")?;
    let agent = match q.get("agent").map(str::trim).filter(|a| !a.is_empty()) {
        Some(a) => Some(normalize_agent(a)?),
        None => None,
    };
    let task_id = match q.filled("task_id") {
        Some(v) => Some(
            v.trim()
                .parse::<i64>()
                .map_err(|_| http(400, "task_id must be a number"))?,
        ),
        None => None,
    };
    let limit = match q.filled("limit") {
        Some(v) => v
            .trim()
            .parse::<i64>()
            .map_err(|_| http(400, "limit must be a number"))?,
        None => DEFAULT_LIMIT,
    };
    if limit < 1 {
        return Err(http(422, "limit must be at least 1"));
    }
    let page_size = limit.min(MAX_LIMIT);
    let after = match q.filled("cursor") {
        Some(c) => Some(decode_cursor(c)?),
        None => None,
    };
    require_agent_work(req)?;

    let started = "CAST(strftime('%s', r.started_at) AS INTEGER)";
    let mut query = String::from(
        "SELECT r.*, t.title AS t_title, c.code AS c_code, t.category_tasks_sequence_id AS t_seq
		 FROM agent_work_runs r JOIN tasks t ON t.id = r.task_id
		 LEFT JOIN categories c ON c.id = t.project_category_id
		 WHERE t.deleted_at IS NULL",
    );
    let mut params: Vec<Sql> = Vec::new();
    if let Some(at) = since {
        query.push_str(&format!(" AND {started} >= ?"));
        params.push(int(at));
    }
    if let Some(at) = until {
        query.push_str(&format!(" AND {started} < ?"));
        params.push(int(at));
    }
    if let Some(id) = task_id {
        query.push_str(" AND r.task_id = ?");
        params.push(int(id));
    }
    if let Some(a) = &agent {
        query.push_str(" AND r.agent = ?");
        params.push(text(a));
    }
    if let Some((at, id)) = after {
        query.push_str(&format!(
            " AND ({started} < ? OR ({started} = ? AND r.id < ?))"
        ));
        params.extend([int(at), int(at), int(id)]);
    }
    query.push_str(&format!(
        " ORDER BY {started} DESC, r.id DESC LIMIT {}",
        page_size + 1
    ));
    let mut rows = sql(select(req.conn, &query, &params))?;
    let more = rows.len() as i64 > page_size;
    rows.truncate(page_size as usize);
    let names = persona_names(req, &rows)?;
    let runs: Vec<Value> = rows
        .iter()
        .map(|r| {
            let mut run = run_json(r, req.ctx, &names);
            let id = col(r, "task_id").as_i64().unwrap_or(0);
            let title = col(r, "t_title").as_str().unwrap_or("");
            run["task"] = json!({
                "id": id,
                "key": task_key(title, col(r, "c_code").as_str(), col(r, "t_seq").as_i64(), id),
                "title": col(r, "t_title"),
            });
            run
        })
        .collect();
    let next_cursor = match rows.last() {
        Some(last) if more => {
            let at = epoch_of(col(last, "started_at")).unwrap_or(0);
            let id = col(last, "id").as_i64().unwrap_or(0);
            Value::String(URL_SAFE_NO_PAD.encode(format!("{at}:{id}")))
        }
        _ => Value::Null,
    };
    Ok(json!({ "runs": runs, "next_cursor": next_cursor }))
}

fn limited_or_null(value: &Value, max: usize) -> Result<Option<String>, Outcome> {
    if value.is_null() {
        return Ok(None);
    }
    let s = js::string(value);
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if js::length(trimmed) > max {
        return Err(http(422, format!("must be at most {max} characters")));
    }
    Ok(Some(trimmed.to_string()))
}

fn opt_text(v: Option<String>) -> Sql {
    v.map_or(Sql::Null, Sql::Text)
}

fn http_url(value: &Value) -> Result<String, Outcome> {
    let s = js::string_or_empty(Some(value));
    let trimmed = s.trim();
    let lower = trimmed.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"));
    let host = rest.map(|r| r.split(['/', '?', '#']).next().unwrap_or(""));
    let ok = js::length(trimmed) <= 512
        && host.is_some_and(|h| !h.is_empty() && !h.contains(char::is_whitespace))
        && !trimmed.contains(char::is_whitespace);
    if ok {
        Ok(trimmed.to_string())
    } else {
        Err(http(422, "pr_url must be an http(s) url"))
    }
}

const MAX_COMMITS: usize = 200;

fn valid_commits(value: &Value) -> Result<Value, Outcome> {
    let Some(items) = value.as_array().filter(|a| a.len() <= MAX_COMMITS) else {
        return Err(http(422, format!("at most {MAX_COMMITS} commits per run")));
    };
    let mut out = Vec::new();
    for commit in items {
        let Some(sha) = commit.get("sha").and_then(Value::as_str) else {
            continue;
        };
        if sha.trim().is_empty() {
            continue;
        }
        let sha = limited_or_null(&json!(sha), 64)?.unwrap_or_default();
        let message = limited_or_null(commit.get("message").unwrap_or(&Value::Null), 500)?;
        out.push(json!({ "sha": sha, "message": message }));
    }
    Ok(Value::Array(out))
}

fn valid_tests(value: &Value) -> Result<Value, Outcome> {
    let get = |k: &str| value.get(k).filter(|v| !v.is_null());
    let num = |v: &Value| match v {
        Value::Bool(b) => *b as i64 as f64,
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::String(s) => js::parse_number(s),
        _ => f64::NAN,
    };
    if get("passed").is_some_and(|v| num(v) < 0.0) || get("failed").is_some_and(|v| num(v) < 0.0) {
        return Err(http(422, "test counts cannot be negative"));
    }
    Ok(json!({
        "passed": get("passed").map_or(Value::Null, |v| js::float_json(num(v))),
        "failed": get("failed").map_or(Value::Null, |v| js::float_json(num(v))),
        "command": limited_or_null(value.get("command").unwrap_or(&Value::Null), 500)?,
    }))
}

/// Column assignments of a progress update, in the TS order.
fn progress(body: &Value, include_branch: bool) -> Result<(Vec<&'static str>, Vec<Sql>), Outcome> {
    let mut sets = Vec::new();
    let mut values = Vec::new();
    let present = |k: &str| body.get(k).filter(|v| !v.is_null());
    if include_branch {
        if let Some(v) = present("branch") {
            sets.push("branch = ?");
            values.push(opt_text(limited_or_null(v, 255)?));
        }
    }
    if let Some(v) = present("summary") {
        sets.push("summary = ?");
        values.push(opt_text(limited_or_null(v, 10_000)?));
    }
    if let Some(v) = present("pr_url") {
        sets.push("pr_url = ?");
        values.push(Sql::Text(http_url(v)?));
    }
    if let Some(v) = present("commits") {
        sets.push("commits = ?");
        values.push(Sql::Text(js::stringify(&valid_commits(v)?)));
    }
    if let Some(v) = present("tests") {
        sets.push("tests = ?");
        values.push(Sql::Text(js::stringify(&valid_tests(v)?)));
    }
    Ok((sets, values))
}

fn load_run(req: &Request<'_>, id: i64) -> Handled {
    let row = sql(select_one(
        req.conn,
        "SELECT * FROM agent_work_runs WHERE id = ?",
        &[int(id)],
    ))?
    .unwrap_or_default();
    let names = persona_names(req, std::slice::from_ref(&row))?;
    Ok(run_json(&row, req.ctx, &names))
}

/// Only the run's own starting actor may change it, and never after it finished.
fn own_running_run(req: &Request<'_>, id: i64) -> Result<Row, Outcome> {
    let row = sql(select_one(
        req.conn,
        "SELECT * FROM agent_work_runs WHERE id = ?",
        &[int(id)],
    ))?
    .ok_or_else(|| not_found("Agent work run"))?;
    let actor = req.ctx.acting();
    let same_actor = col(&row, "actor_kind").as_str() == Some(actor.kind.as_str())
        && js::string(col(&row, "actor_id")) == actor.owner();
    let agent = js::string(col(&row, "agent"));
    let namespace = format!("plugin:{}", actor.id);
    let same_namespace =
        actor.kind != "plugin" || agent == namespace || agent.starts_with(&format!("{namespace}/"));
    if !same_actor || !same_namespace {
        return Err(coded(
            403,
            "Only the user the agent works for can change this run",
            "NOT_OWN",
        ));
    }
    let status = js::string(col(&row, "status"));
    if status != "running" {
        return Err(http(409, format!("Agent work run is already {status}")));
    }
    Ok(row)
}

pub(super) fn start(req: &Request<'_>) -> Handled {
    require_agent_work(req)?;
    let task_id = req.id("id");
    let body = &req.body;
    let none = Value::Null;
    let agent = normalize_agent(&js::string_or_empty(body.get("agent")))?;
    let model = limited_or_null(body.get("model").unwrap_or(&none), 128)?;
    let session = limited_or_null(body.get("session_id").unwrap_or(&none), 191)?;
    let branch = limited_or_null(body.get("branch").unwrap_or(&none), 255)?;
    let actor = req.ctx.acting();
    let now = req.ctx.iso_now();
    sql(execute(
        req.conn,
        "UPDATE agent_work_runs SET status = 'abandoned', ended_at = COALESCE(updated_at, started_at),
					duration_seconds = CAST((julianday(COALESCE(updated_at, started_at)) - julianday(started_at)) * 86400 AS INTEGER),
					version = version + 1, updated_at = ?
				 WHERE task_id = ? AND actor_kind = ? AND actor_id = ? AND agent = ? AND status = 'running'",
        &[text(&now), int(task_id), text(&actor.kind), text(actor.owner()), text(&agent)],
    ))?;
    let inserted = sql(execute(
        req.conn,
        "INSERT INTO agent_work_runs
					(task_id, agent, model, session_id, branch, status, started_at, actor_kind, actor_id, version, created_at, updated_at)
				 SELECT ?, ?, ?, ?, ?, 'running', ?, ?, ?, 1, ?, ?
				 WHERE EXISTS (SELECT 1 FROM tasks WHERE id = ? AND deleted_at IS NULL)",
        &[
            int(task_id),
            text(&agent),
            opt_text(model),
            opt_text(session),
            opt_text(branch),
            text(&now),
            text(&actor.kind),
            text(actor.owner()),
            text(&now),
            text(&now),
            int(task_id),
        ],
    ))?;
    if inserted.changes == 0 {
        return Err(not_found("Task"));
    }
    load_run(req, inserted.last_id)
}

pub(super) fn update(req: &Request<'_>) -> Handled {
    require_agent_work(req)?;
    let run = own_running_run(req, req.id("id"))?;
    let (mut sets, mut values) = progress(&req.body, true)?;
    sets.extend(["version = version + 1", "updated_at = ?"]);
    values.push(text(&req.ctx.iso_now()));
    let id = col(&run, "id").as_i64().unwrap_or(0);
    values.push(int(id));
    sql(execute(
        req.conn,
        &format!(
            "UPDATE agent_work_runs SET {} WHERE id = ?",
            sets.join(", ")
        ),
        &values,
    ))?;
    load_run(req, id)
}

pub(super) fn finish(req: &Request<'_>) -> Handled {
    require_agent_work(req)?;
    let run = own_running_run(req, req.id("id"))?;
    let status = js::string_or_empty(req.body.get("status"))
        .trim()
        .to_lowercase();
    if !["succeeded", "failed", "cancelled"].contains(&status.as_str()) {
        return Err(http(
            422,
            "status must be one of succeeded, failed, cancelled",
        ));
    }
    let (mut sets, mut values) = progress(&req.body, false)?;
    let now = req.ctx.now;
    let started = DateTime::parse_from_rfc3339(col(&run, "started_at").as_str().unwrap_or(""))
        .map(|d| d.with_timezone(&chrono::Utc))
        .unwrap_or(now);
    let end = if now < started { started } else { now };
    let duration = ((end - started).num_milliseconds() as f64 / 1000.0).round() as i64;
    sets.extend([
        "status = ?",
        "ended_at = ?",
        "duration_seconds = ?",
        "version = version + 1",
        "updated_at = ?",
    ]);
    values.extend([
        text(&status),
        text(&end.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)),
        int(duration),
        text(&req.ctx.iso_now()),
    ]);
    let id = col(&run, "id").as_i64().unwrap_or(0);
    values.push(int(id));
    sql(execute(
        req.conn,
        &format!(
            "UPDATE agent_work_runs SET {} WHERE id = ?",
            sets.join(", ")
        ),
        &values,
    ))?;
    load_run(req, id)
}

#[cfg(test)]
mod tests {
    use crate::testing::{ctx, db, get};
    use serde_json::json;

    const SEED: &str = "
        INSERT INTO categories (id, title, code, created_at, updated_at) VALUES (1, 'Work', 'WK', 'x', 'x');
        INSERT INTO tasks (id, title, project_category_id, category_tasks_sequence_id, created_at, updated_at)
        VALUES (1, 'TM-9: one', NULL, NULL, 'x', 'x'), (2, 'two', 1, 4, 'x', 'x');
        INSERT INTO agent_work_runs (id, task_id, agent, status, started_at, duration_seconds, actor_kind, actor_id, created_at, updated_at)
        VALUES
          (1, 1, 'codex', 'succeeded', '2026-10-10T10:00:00.000Z', 300, 'user', '1', 'x', 'x'),
          (2, 2, 'claude-code', 'running', '2026-10-13T11:00:00.000Z', NULL, 'user', '1', 'x', 'x'),
          (3, 2, 'claude-code', 'failed', '2026-10-11T09:30:00.000Z', 60, 'user', '1', 'x', 'x');
    ";

    #[test]
    fn lists_workspace_runs_newest_first_with_their_task() {
        let conn = db(SEED);
        let c = ctx("2026-10-13T12:00:00.000Z");
        let (status, body) = get(&conn, &c, "workspaces/-42/agent-work");
        assert_eq!(status, 200);
        let runs = body["data"]["runs"].as_array().unwrap();
        let ids: Vec<i64> = runs.iter().map(|r| r["id"].as_i64().unwrap()).collect();
        assert_eq!(ids, [2, 3, 1]);
        assert_eq!(
            runs[0]["task"],
            json!({ "id": 2, "key": "WK-4", "title": "two" })
        );
        assert_eq!(runs[0]["duration_seconds"], 3600);
        assert_eq!(runs[2]["task"]["key"], "TM-9");
        assert_eq!(body["data"]["next_cursor"], json!(null));
    }

    #[test]
    fn filters_and_pages_with_a_cursor() {
        let conn = db(SEED);
        let c = ctx("2026-10-13T12:00:00.000Z");
        let (_, first) = get(&conn, &c, "workspaces/-42/agent-work?limit=2");
        let cursor = first["data"]["next_cursor"].as_str().unwrap().to_string();
        assert_eq!(first["data"]["runs"].as_array().unwrap().len(), 2);
        let (_, rest) = get(
            &conn,
            &c,
            &format!("workspaces/-42/agent-work?limit=2&cursor={cursor}"),
        );
        assert_eq!(rest["data"]["runs"][0]["id"], 1);
        assert_eq!(rest["data"]["next_cursor"], json!(null));

        let ids = |url: &str| -> Vec<i64> {
            get(&conn, &c, url).1["data"]["runs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["id"].as_i64().unwrap())
                .collect()
        };
        assert_eq!(ids("workspaces/-42/agent-work?agent=Claude%20Code"), [2, 3]);
        assert_eq!(ids("workspaces/-42/agent-work?task_id=1"), [1]);
        assert_eq!(
            ids("workspaces/-42/agent-work?since=2026-10-11&until=2026-10-12"),
            [3]
        );
        assert_eq!(
            ids("workspaces/-42/agent-work?until=2026-10-11T09:30:00Z"),
            [1]
        );
        assert_eq!(get(&conn, &c, "workspaces/-42/agent-work?limit=0").0, 422);
        assert_eq!(
            get(&conn, &c, "workspaces/-42/agent-work?cursor=%%%").0,
            422
        );
        assert_eq!(
            get(&conn, &c, "workspaces/-42/agent-work?since=yesterday").0,
            422
        );
    }

    #[test]
    fn a_disabled_module_answers_feature_disabled() {
        let conn = db("INSERT INTO meta (key, value) VALUES ('feature.agent_work', '0');");
        let c = ctx("2026-10-13T12:00:00.000Z");
        let (status, body) = get(&conn, &c, "workspaces/-42/agent-work");
        assert_eq!(status, 403);
        assert_eq!(body["error"], "feature_disabled");
    }
}
