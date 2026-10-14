use super::{emit, Opts};
use chrono::{Offset, Utc};
use rusqlite::Connection;
use serde::Deserialize;
use serde_json::{json, Value};
use std::io::Read;
use std::path::Path;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::local;
use tmgr_core::refs::{parse_task_ref, TaskRef as RefKind};
use tmgr_ipc::api::limits::MAX_BUNDLE;
use tmgr_ipc::api::{
    AuthorKind, BundleAuthor, BundleComment, BundleLaunch, BundlePage, RunBundle, TaskRef,
};
use tmgr_local::{Actor, Context, LocalWorkspace};

const MAX_INPUT: usize = 64 * 1024;
const MAX_COMMITS: usize = 200;
const MAX_COMMENTS: usize = 500;
const MAX_PAGES: usize = 50;
const NAMESPACE: &str = "runner";

pub fn runner_actor() -> Actor {
    Actor {
        kind: "plugin".into(),
        id: NAMESPACE.into(),
        name: NAMESPACE.into(),
        owner_id: None,
    }
}

fn runner_agent(profile: &str) -> String {
    format!("plugin:{NAMESPACE}/{profile}")
}

fn db_error(e: tmgr_local::Error) -> Error {
    match e {
        tmgr_local::Error::Busy => Error::new(Kind::Offline, e.to_string())
            .hint("the desktop app is writing to this workspace; try again"),
        tmgr_local::Error::NotFound(_) => Error::not_found(e.to_string()),
        tmgr_local::Error::SchemaTooNew(_)
        | tmgr_local::Error::SchemaTooOld(_)
        | tmgr_local::Error::Unsupported(_) => Error::usage(e.to_string()),
        _ => Error::internal(e.to_string()),
    }
}

fn find_workspace(reference: &str) -> Result<LocalWorkspace> {
    let key = reference.strip_prefix(local::PREFIX).unwrap_or(reference);
    let id = key.parse::<i64>().ok();
    local::list()?
        .into_iter()
        .find(|w| w.manifest.code == key || id == Some(w.manifest.id))
        .ok_or_else(|| Error::usage(format!("{key:?} is not a local workspace")))
}

struct Session {
    ctx: Context,
    conn: Connection,
}

impl Session {
    fn open(ws: &LocalWorkspace, actor: Option<Actor>, write: bool) -> Result<Session> {
        let root = ws
            .path
            .parent()
            .ok_or_else(|| Error::internal("local workspace has no root"))?;
        let conn = if write {
            tmgr_local::open_write(&ws.database)
        } else {
            tmgr_local::open_read(&ws.database)
        }
        .map_err(db_error)?;
        let now = Utc::now();
        let ctx = Context {
            workspace: ws.clone(),
            user: local::identity(root, None),
            now,
            local_offset: now.with_timezone(&chrono::Local).offset().fix(),
            actor,
            origin: tmgr_local::WriteOrigin::Cli,
        };
        Ok(Session { ctx, conn })
    }

    fn status_and_data(
        &self,
        method: &str,
        url: &str,
        body: Option<&Value>,
    ) -> Result<(u16, Value)> {
        let r = tmgr_local::dispatch(&self.conn, &self.ctx, method, url, body)
            .map_err(db_error)?
            .ok_or_else(|| {
                Error::usage(format!(
                    "{method} {url} is not supported for local workspaces"
                ))
            })?;
        let data = match r.data {
            Value::Object(mut m) if m.contains_key("data") => m.remove("data").unwrap_or_default(),
            other => other,
        };
        Ok((r.status, data))
    }

    fn send(&self, method: &str, url: &str, body: Option<&Value>) -> Result<Value> {
        let (status, data) = self.status_and_data(method, url, body)?;
        if status >= 400 {
            return Err(local::status_error(status, &data));
        }
        Ok(data)
    }
}

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Request {
    Start {
        workspace: String,
        task_id: i64,
        agent: String,
        model: Option<String>,
        branch: Option<String>,
        session_id: Option<String>,
    },
    Update {
        workspace: String,
        task_id: i64,
        run_id: i64,
        branch: Option<String>,
        commits: Option<Vec<String>>,
    },
    Finish {
        workspace: String,
        task_id: i64,
        run_id: i64,
        status: String,
        summary: Option<String>,
        commits: Option<Vec<String>>,
        #[serde(rename = "branch")]
        _branch: Option<String>,
    },
}

fn invalid(field: &str, why: &str) -> Error {
    Error::usage(format!("{field} {why}"))
}

fn bounded(field: &str, value: &str, max: usize, multiline: bool) -> Result<String> {
    let clean: String = value
        .chars()
        .filter(|c| !c.is_control() || (multiline && matches!(c, '\n' | '\t')))
        .collect();
    if clean.chars().count() > max {
        return Err(invalid(field, &format!("must be at most {max} characters")));
    }
    Ok(clean)
}

fn optional(field: &str, value: Option<&str>, max: usize) -> Result<Option<String>> {
    match value {
        None => Ok(None),
        Some(v) => Ok(Some(bounded(field, v, max, false)?).filter(|s| !s.is_empty())),
    }
}

fn word(field: &str, value: &str, max: usize, extra: &str) -> Result<String> {
    let ok = !value.is_empty()
        && value.chars().count() <= max
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || extra.contains(c));
    ok.then(|| value.to_string())
        .ok_or_else(|| invalid(field, "is malformed"))
}

fn positive(field: &str, value: i64) -> Result<i64> {
    (value > 0)
        .then_some(value)
        .ok_or_else(|| invalid(field, "must be a positive number"))
}

fn commits(lines: &[String]) -> Result<Value> {
    if lines.len() > MAX_COMMITS {
        return Err(invalid(
            "commits",
            &format!("has more than {MAX_COMMITS} entries"),
        ));
    }
    let mut out = Vec::new();
    for line in lines {
        let line = bounded("commits[]", line, 300, false)?;
        let (sha, message) = line.split_once(char::is_whitespace).unwrap_or((&line, ""));
        let hex = (4..=64).contains(&sha.len()) && sha.bytes().all(|b| b.is_ascii_hexdigit());
        if !hex {
            return Err(invalid("commits[]", "must start with a commit sha"));
        }
        out.push(json!({ "sha": sha, "message": Some(message.trim()).filter(|m| !m.is_empty()) }));
    }
    Ok(Value::Array(out))
}

fn profile(raw: &str) -> Result<String> {
    let name = raw
        .strip_prefix(&format!("plugin:{NAMESPACE}/"))
        .unwrap_or(raw);
    word("agent", name, 40, "._-").map(|p| runner_agent(&p))
}

fn id_of(value: &Value) -> Result<i64> {
    value
        .get("id")
        .and_then(Value::as_i64)
        .ok_or_else(|| Error::internal("the local workspace answered without an id"))
}

fn owned_run(s: &Session, task_id: i64, run_id: i64) -> Result<()> {
    let runs = s.send("GET", &format!("tasks/{task_id}/agent-work"), None)?;
    let found = runs["runs"]
        .as_array()
        .is_some_and(|r| r.iter().any(|run| run["id"].as_i64() == Some(run_id)));
    found
        .then_some(())
        .ok_or_else(|| Error::usage(format!("run {run_id} is not a run of task {task_id}")))
}

fn move_to_active(s: &Session, task_id: i64) -> Result<Option<i64>> {
    let task = s.send("GET", &format!("tasks/{task_id}"), None)?;
    if task["status"].as_str() != Some("default") {
        return Ok(None);
    }
    let statuses = s.send("GET", "workspaces/statuses", None)?;
    let Some(active) = statuses
        .as_array()
        .and_then(|all| all.iter().find(|st| st["type"].as_str() == Some("active")))
        .and_then(|st| st["id"].as_i64())
    else {
        return Ok(None);
    };
    s.send(
        "PATCH",
        &format!("tasks/{task_id}"),
        Some(&json!({ "status_id": active })),
    )?;
    Ok(Some(active))
}

fn handle(request: Request) -> Result<Value> {
    match request {
        Request::Start {
            workspace,
            task_id,
            agent,
            model,
            branch,
            session_id,
        } => {
            let ws = find_workspace(&workspace)?;
            let task_id = positive("task_id", task_id)?;
            let session = match session_id.as_deref() {
                Some(v) => Some(word("session_id", v, 100, "_.:-")?),
                None => None,
            };
            let body = json!({
                "agent": profile(&agent)?,
                "model": optional("model", model.as_deref(), 128)?,
                "branch": optional("branch", branch.as_deref(), 255)?,
                "session_id": session,
            });
            let s = Session::open(&ws, Some(runner_actor()), true)?;
            let run = s.send("POST", &format!("tasks/{task_id}/agent-work"), Some(&body))?;
            let run_id = id_of(&run)?;
            let mut out = json!({ "run_id": run_id });
            if let Ok(Some(status)) = move_to_active(&s, task_id) {
                out["moved_to"] = json!(status);
            }
            Ok(out)
        }
        Request::Update {
            workspace,
            task_id,
            run_id,
            branch,
            commits: lines,
        } => {
            let ws = find_workspace(&workspace)?;
            let (task_id, run_id) = (positive("task_id", task_id)?, positive("run_id", run_id)?);
            let mut body = json!({});
            if let Some(b) = optional("branch", branch.as_deref(), 255)? {
                body["branch"] = json!(b);
            }
            if let Some(lines) = &lines {
                body["commits"] = commits(lines)?;
            }
            let s = Session::open(&ws, Some(runner_actor()), true)?;
            owned_run(&s, task_id, run_id)?;
            s.send("PATCH", &format!("agent-work/{run_id}"), Some(&body))?;
            Ok(json!({ "run_id": run_id }))
        }
        Request::Finish {
            workspace,
            task_id,
            run_id,
            status,
            summary,
            commits: lines,
            ..
        } => {
            let ws = find_workspace(&workspace)?;
            let (task_id, run_id) = (positive("task_id", task_id)?, positive("run_id", run_id)?);
            if !["succeeded", "failed", "cancelled"].contains(&status.as_str()) {
                return Err(invalid("status", "must be succeeded, failed or cancelled"));
            }
            let mut body = json!({ "status": status });
            if let Some(text) = summary.as_deref() {
                let text = bounded("summary", text, 2000, true)?;
                if !text.trim().is_empty() {
                    body["summary"] = json!(text);
                }
            }
            if let Some(lines) = &lines {
                body["commits"] = commits(lines)?;
            }
            let s = Session::open(&ws, Some(runner_actor()), true)?;
            owned_run(&s, task_id, run_id)?;
            s.send("POST", &format!("agent-work/{run_id}/finish"), Some(&body))?;
            Ok(json!({ "run_id": run_id, "status": status }))
        }
    }
}

fn read_request() -> Result<Request> {
    let mut raw = Vec::new();
    std::io::stdin()
        .take(MAX_INPUT as u64 + 1)
        .read_to_end(&mut raw)
        .map_err(|e| Error::internal(e.to_string()))?;
    if raw.len() > MAX_INPUT {
        return Err(Error::usage("input is larger than 64 KiB"));
    }
    serde_json::from_slice(&raw).map_err(|e| Error::usage(format!("invalid request: {e}")))
}

pub fn agent_work_main() -> i32 {
    match read_request().and_then(handle) {
        Ok(out) => {
            println!("{out}");
            0
        }
        Err(e) => {
            println!("{}", json!({ "error": e.message }));
            1
        }
    }
}

fn is_author_id(id: &str) -> bool {
    id.chars().count() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._:@-".contains(c))
}

fn author(kind: Option<&str>, id: Option<&str>) -> BundleAuthor {
    let (kind, id) = match kind {
        Some("persona") => (AuthorKind::Agent, id),
        _ => (AuthorKind::Other, id),
    };
    BundleAuthor {
        kind,
        id: id.filter(|i| is_author_id(i)).map(str::to_string),
    }
}

fn stored_comments(ws: &LocalWorkspace, task_id: i64) -> Result<Vec<BundleComment>> {
    let conn = tmgr_local::open_read(&ws.database).map_err(db_error)?;
    let sql_err = |e: rusqlite::Error| Error::internal(e.to_string());
    let (count, bytes): (i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), COALESCE(SUM(LENGTH(CAST(message AS BLOB))), 0) FROM comments
             WHERE task_id = ? AND deleted_at IS NULL",
            [task_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(sql_err)?;
    if count > MAX_COMMENTS as i64 || bytes > MAX_BUNDLE as i64 {
        return Err(Error::usage(format!(
            "the task has too many or too large comments for a run bundle ({count} comments)"
        )));
    }
    let mut stmt = conn
        .prepare(
            "SELECT author_kind, author_id, created_at, message FROM comments
             WHERE task_id = ? AND deleted_at IS NULL ORDER BY id ASC",
        )
        .map_err(sql_err)?;
    let rows = stmt
        .query_map([task_id], |r| {
            Ok((
                r.get::<_, Option<String>>(0)?,
                r.get::<_, Option<rusqlite::types::Value>>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })
        .and_then(|rows| rows.collect::<std::result::Result<Vec<_>, _>>())
        .map_err(sql_err)?;
    Ok(rows
        .into_iter()
        .map(|(kind, id, at, body)| {
            let id = match id {
                Some(rusqlite::types::Value::Integer(i)) => Some(i.to_string()),
                Some(rusqlite::types::Value::Text(t)) => Some(t),
                _ => None,
            };
            BundleComment {
                author: author(kind.as_deref(), id.as_deref()),
                at: at.unwrap_or_default(),
                body: body.unwrap_or_default(),
            }
        })
        .collect())
}

fn linked_pages(ws: &LocalWorkspace, task_id: i64) -> Result<Vec<i64>> {
    let conn = tmgr_local::open_read(&ws.database).map_err(db_error)?;
    let mut stmt = conn
        .prepare(
            "SELECT page_id FROM page_links WHERE target_kind = 'task' AND target_id = ?
             ORDER BY page_id LIMIT ?",
        )
        .map_err(|e| Error::internal(e.to_string()))?;
    let ids = stmt
        .query_map([task_id, MAX_PAGES as i64], |r| r.get(0))
        .and_then(|rows| rows.collect::<std::result::Result<Vec<i64>, _>>())
        .map_err(|e| Error::internal(e.to_string()))?;
    Ok(ids)
}

pub fn bundle_from_local(
    ws: &LocalWorkspace,
    task_id: i64,
    launch: BundleLaunch,
) -> Result<(RunBundle, String)> {
    let s = Session::open(ws, None, false)?;
    let task = s.send("GET", &format!("tasks/{task_id}"), None)?;
    let comments = stored_comments(ws, task_id)?;
    let mut pages = Vec::new();
    for id in linked_pages(ws, task_id)? {
        let (status, page) = s.status_and_data("GET", &format!("pages/{id}"), None)?;
        if status == 404 {
            continue;
        }
        if status >= 400 {
            return Err(local::status_error(status, &page));
        }
        pages.push(BundlePage {
            id,
            revision: page["version"].as_i64().unwrap_or(1),
            title: page["title"].as_str().unwrap_or("").to_string(),
            body: page["body"].as_str().unwrap_or("").to_string(),
        });
    }
    let bundle = RunBundle {
        v: 1,
        task: TaskRef {
            workspace_id: ws.manifest.id,
            task_id,
        },
        title: task["title"].as_str().unwrap_or("").to_string(),
        body: task["description"].as_str().unwrap_or("").to_string(),
        author: author(None, None),
        comments,
        pages,
        persona: None,
        launch,
    };
    bundle.validate().map_err(Error::usage)?;
    if bundle.canonical().len() > MAX_BUNDLE {
        return Err(Error::usage(
            "the run bundle is larger than 1 MiB (task, comments and pages together)",
        )
        .hint("shorten the task text or unlink large pages"));
    }
    let sha = bundle.sha256();
    Ok((bundle, sha))
}

fn task_id_of(ws: &LocalWorkspace, reference: &str) -> Result<i64> {
    match parse_task_ref(reference)? {
        RefKind::Id(id) => i64::try_from(id).map_err(|_| Error::usage("invalid task reference")),
        RefKind::Key(key) => {
            let s = Session::open(ws, None, false)?;
            id_of(&s.send("GET", &format!("tasks/by-key/{key}"), None)?)
        }
    }
}

pub fn bundle_cmd(
    o: &Opts,
    task: &str,
    profile: &str,
    model: Option<&str>,
    cwd: &Path,
    mode: super::PermissionMode,
) -> Result<i32> {
    let reference = o.ws.as_deref().ok_or_else(|| {
        Error::usage("pass --ws local:<code>").hint("run bundles are built from local workspaces")
    })?;
    let ws = find_workspace(reference)?;
    let cwd = std::fs::canonicalize(cwd)
        .map_err(|e| Error::usage(format!("cwd {}: {e}", cwd.display())))?;
    let launch = BundleLaunch {
        profile: profile.to_string(),
        model: model.map(str::to_string),
        cwd: cwd.to_string_lossy().into_owned(),
        permission_mode: Some(mode.as_str().to_string()),
    };
    let (bundle, sha256) = bundle_from_local(&ws, task_id_of(&ws, task)?, launch)?;
    let human = format!(
        "{}\nsha256: {sha256}",
        serde_json::to_string_pretty(&bundle).map_err(|e| Error::internal(e.to_string()))?
    );
    Ok(emit(
        o,
        json!({ "bundle": bundle, "sha256": sha256 }),
        human,
    ))
}
