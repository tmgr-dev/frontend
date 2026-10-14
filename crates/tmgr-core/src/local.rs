//! Local (desktop) workspaces as a transport of `api::Client`: requests go to `tmgr-local` instead of
//! the server, never over the network and without a login.

use std::cell::RefCell;
use std::path::{Path, PathBuf};

use chrono::{Offset, Utc};
use rusqlite::Connection;
use serde_json::Value;
use tmgr_local::Context;
pub use tmgr_local::{agent_session_detected, LocalWorkspace, User};

use crate::error::{Error, Kind, Result};

pub const PREFIX: &str = "local:";

/// The code of a `local:<code>` workspace reference.
pub fn local_code(reference: &str) -> Option<&str> {
    reference
        .strip_prefix(PREFIX)
        .filter(|code| crate::refs::is_workspace_code(code))
}

/// A cloud workspace code or a `local:<code>` reference.
pub fn is_workspace_ref(reference: &str) -> bool {
    crate::refs::is_workspace_code(reference) || local_code(reference).is_some()
}

pub fn root() -> Result<PathBuf> {
    tmgr_local::root()
        .ok_or_else(|| Error::internal("cannot locate local workspaces (HOME is not set)"))
}

pub fn list() -> Result<Vec<LocalWorkspace>> {
    Ok(tmgr_local::list_in(&root()?))
}

pub fn find(code: &str) -> Result<LocalWorkspace> {
    tmgr_local::find(&root()?, code).map_err(|_| {
        Error::not_found(format!("local workspace {code} not found")).hint("tmgr ws list")
    })
}

/// Who the CLI is in a local workspace: `local_user_id` from the config when set, else the desktop's
/// accountless profile (`~/.tmgr.dev/profile.json`), else id -1 with no name.
pub fn identity(root: &Path, configured_id: Option<i64>) -> User {
    let profile: Option<Value> = root
        .parent()
        .and_then(|dir| std::fs::read_to_string(dir.join("profile.json")).ok())
        .and_then(|raw| serde_json::from_str(&raw).ok());
    let field = |k: &str| profile.as_ref().and_then(|p| p.get(k).cloned());
    User {
        id: configured_id
            .or_else(|| field("id").and_then(|v| v.as_i64()))
            .unwrap_or(-1),
        name: field("name")
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default(),
        email: String::new(),
    }
}

/// The task text that would reach an agent, read from the workspace `code`.
pub fn task_snapshot(code: &str, task_id: i64) -> Result<tmgr_local::Snapshot> {
    let workspace = find(code)?;
    let conn = tmgr_local::open_read(&workspace.database).map_err(local_error)?;
    tmgr_local::snapshot(&conn, task_id)
        .map_err(local_error)?
        .ok_or_else(|| Error::not_found(format!("task {task_id} not found")))
}

/// Records the owner's confirmation of the task text whose snapshot hash is `expected`. The caller must have
/// checked that a person is at a terminal and typed that hash.
pub fn confirm_task(
    code: &str,
    local_user_id: Option<i64>,
    task_id: i64,
    expected: &str,
) -> Result<String> {
    let workspace = find(code)?;
    let user = identity(&root()?, local_user_id);
    let conn = tmgr_local::open_write(&workspace.database).map_err(local_error)?;
    let now = Utc::now();
    let ctx = Context {
        workspace,
        user,
        now,
        local_offset: now.with_timezone(&chrono::Local).offset().fix(),
        actor: None,
        origin: tmgr_local::WriteOrigin::Cli,
    };
    tmgr_local::confirm_task(&conn, &ctx, task_id, Some(expected)).map_err(local_error)
}

/// A client of the local workspace `code`, acting as the local identity.
pub fn client(
    code: &str,
    local_user_id: Option<i64>,
    user_agent: &str,
    verbose: bool,
) -> Result<crate::api::Client> {
    let workspace = find(code)?;
    let user = identity(&root()?, local_user_id);
    crate::api::Client::for_local(Local::new(workspace, user), user_agent, verbose)
}

pub struct Local {
    workspace: LocalWorkspace,
    user: User,
    read: RefCell<Option<Connection>>,
    write: RefCell<Option<Connection>>,
}

impl Local {
    pub fn new(workspace: LocalWorkspace, user: User) -> Local {
        Local {
            workspace,
            user,
            read: RefCell::new(None),
            write: RefCell::new(None),
        }
    }

    pub fn workspace(&self) -> &LocalWorkspace {
        &self.workspace
    }

    pub fn user(&self) -> &User {
        &self.user
    }

    /// One request; a user GET uses a read-only handle, anything else the write handle.
    pub fn send(
        &self,
        method: &str,
        path: &str,
        query: &[(&str, String)],
        body: Option<&Value>,
    ) -> Result<(u16, Value)> {
        let mut url = path.to_string();
        if !query.is_empty() {
            let encoded: String = form_urlencoded::Serializer::new(String::new())
                .extend_pairs(query.iter().map(|(k, v)| (*k, v.as_str())))
                .finish();
            url.push('?');
            url.push_str(&encoded);
        }
        let now = Utc::now();
        let ctx = Context {
            workspace: self.workspace.clone(),
            user: self.user.clone(),
            now,
            local_offset: now.with_timezone(&chrono::Local).offset().fix(),
            actor: None,
            origin: tmgr_local::WriteOrigin::Cli,
        };
        let cell = if method.eq_ignore_ascii_case("GET") {
            &self.read
        } else {
            &self.write
        };
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            let opened = if method.eq_ignore_ascii_case("GET") {
                tmgr_local::open_read(&self.workspace.database)
            } else {
                tmgr_local::open_write(&self.workspace.database)
            };
            *slot = Some(opened.map_err(local_error)?);
        }
        let conn = slot
            .as_ref()
            .ok_or_else(|| Error::internal("local database is not open"))?;
        match tmgr_local::dispatch(conn, &ctx, method, &url, body).map_err(local_error)? {
            Some(response) => Ok((response.status, response.data)),
            None => Err(Error::usage(format!(
                "{} {} is not supported for local workspaces yet",
                method.to_ascii_uppercase(),
                path.trim_start_matches("/api")
            ))),
        }
    }
}

fn local_error(e: tmgr_local::Error) -> Error {
    match e {
        tmgr_local::Error::Busy => Error::new(Kind::Offline, e.to_string())
            .hint("the desktop app is writing to this workspace; try again"),
        tmgr_local::Error::NotFound(_) => Error::not_found(e.to_string()),
        tmgr_local::Error::SchemaTooNew(_)
        | tmgr_local::Error::SchemaTooOld(_)
        | tmgr_local::Error::Unsupported(_) => Error::usage(e.to_string()),
        tmgr_local::Error::Io(_) | tmgr_local::Error::Sql(_) => Error::internal(e.to_string()),
    }
}

/// An answer of the local API as a CLI error; no login hints, nothing here is about the server.
pub fn status_error(status: u16, body: &Value) -> Error {
    let msg = body
        .get("message")
        .and_then(Value::as_str)
        .map(crate::text::sanitize_line)
        .unwrap_or_else(|| format!("local request failed (status {status})"));
    let kind = match status {
        401 => Kind::Auth,
        403 => Kind::Forbidden,
        404 => Kind::NotFound,
        409 => Kind::Conflict,
        400 | 413 | 422 => Kind::Usage,
        _ => Kind::Internal,
    };
    Error::new(kind, msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_references_need_a_valid_code() {
        assert_eq!(local_code("local:personal"), Some("personal"));
        assert_eq!(local_code("local:"), None);
        assert_eq!(local_code("local:a b"), None);
        assert_eq!(local_code("personal"), None);
    }

    #[test]
    fn identity_prefers_the_config_then_the_desktop_profile() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspaces");
        assert_eq!(identity(&root, None).id, -1);
        std::fs::write(
            dir.path().join("profile.json"),
            r#"{"id":-1,"uuid":"u","name":"Owner","created_at":"x"}"#,
        )
        .unwrap();
        let me = identity(&root, None);
        assert_eq!((me.id, me.name.as_str()), (-1, "Owner"));
        assert_eq!(identity(&root, Some(42)).id, 42);
    }

    #[test]
    fn an_unported_route_is_a_clear_usage_error() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("personal");
        std::fs::create_dir_all(&ws).unwrap();
        std::fs::write(
            ws.join("workspace.json"),
            r#"{"id":1,"name":"P","code":"personal","schema_version":12,"created_at":"x"}"#,
        )
        .unwrap();
        let golden: Value = serde_json::from_str(include_str!(
            "../../tmgr-local/tests/parity/golden/schema.json"
        ))
        .unwrap();
        let conn = Connection::open(ws.join("workspace.db")).unwrap();
        for object in golden["sqlite_master"].as_array().unwrap() {
            if let Some(ddl) = object["sql"].as_str() {
                conn.execute_batch(ddl).unwrap();
            }
        }
        conn.execute_batch("INSERT INTO meta VALUES ('schema_version', '12')")
            .unwrap();
        let workspace = tmgr_local::find(dir.path(), "personal").unwrap();
        let local = Local::new(workspace, identity(dir.path(), None));
        let err = local.send("GET", "/api/pages", &[], None).unwrap_err();
        assert_eq!(err.kind, Kind::Usage);
        assert_eq!(
            err.message,
            "GET /pages is not supported for local workspaces yet"
        );
        let (status, body) = local
            .send("GET", "/api/tasks", &[("per_page", "5".into())], None)
            .unwrap();
        assert_eq!(
            (status, &body["meta"]["per_page"]),
            (200, &serde_json::json!(5))
        );
    }

    #[test]
    fn a_busy_desktop_is_retryable_not_a_stale_run() {
        let e = local_error(tmgr_local::Error::Busy);
        assert_eq!(e.kind, Kind::Offline);
        assert!(e.hint.is_some());
    }

    #[test]
    fn local_answers_map_to_cli_errors_without_login_hints() {
        let e = status_error(403, &serde_json::json!({ "message": "nope" }));
        assert_eq!(e.kind, Kind::Forbidden);
        assert!(e.hint.is_none());
        assert_eq!(status_error(422, &Value::Null).kind, Kind::Usage);
    }
}
