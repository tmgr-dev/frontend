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
    find_at(&root()?, code)
}

pub fn list_at(root: &Path) -> Vec<LocalWorkspace> {
    tmgr_local::list_in(root)
}

pub fn find_at(root: &Path, code: &str) -> Result<LocalWorkspace> {
    tmgr_local::find(root, code).map_err(|_| {
        Error::not_found(format!("local workspace {code} not found")).hint("tmgr ws list")
    })
}

const HINT_MAX_BYTES: u64 = 4096;
const HINT_MAX_ID: i64 = (1 << 53) - 1;
const HINT_MAX_NAME_CHARS: usize = 80;

fn read_hint(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(not(unix))]
    if !std::fs::symlink_metadata(path).ok()?.is_file() {
        return None;
    }
    let file = options.open(path).ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() > HINT_MAX_BYTES {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.uid() != unsafe { libc::geteuid() } || meta.mode() & 0o022 != 0 {
            return None;
        }
    }
    let mut raw = Vec::new();
    file.take(HINT_MAX_BYTES).read_to_end(&mut raw).ok()?;
    Some(raw)
}

fn is_invisible(c: char) -> bool {
    c.is_control()
        || matches!(c,
            '\u{00AD}' | '\u{0600}'..='\u{0605}' | '\u{061C}' | '\u{06DD}' | '\u{070F}' | '\u{08E2}'
            | '\u{180E}' | '\u{200B}'..='\u{200F}' | '\u{2028}'..='\u{202E}' | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206F}' | '\u{FEFF}' | '\u{FFF9}'..='\u{FFFB}' | '\u{110BD}' | '\u{110CD}'
            | '\u{1D173}'..='\u{1D17A}' | '\u{E0001}' | '\u{E0020}'..='\u{E007F}')
}

fn cloud_identity(dir: &Path) -> Option<(i64, String)> {
    let value: Value =
        serde_json::from_slice(&read_hint(&dir.join("cloud-identity.json"))?).ok()?;
    let id = value
        .get("id")?
        .as_i64()
        .filter(|id| (1..=HINT_MAX_ID).contains(id))?;
    let name: String = value
        .get("name")?
        .as_str()?
        .chars()
        .filter(|c| !is_invisible(*c))
        .collect();
    Some((id, name.trim().chars().take(HINT_MAX_NAME_CHARS).collect()))
}

/// Who the CLI is in a local workspace: `local_user_id` from the config when set, else the cloud account
/// the desktop wrote to `cloud-identity.json` next to the workspaces folder, else the accountless profile
/// (`~/.tmgr.dev/profile.json`), else id -1 with no name. The hint file is untrusted input.
pub fn identity(root: &Path, configured_id: Option<i64>) -> User {
    let dir = root.parent();
    let profile: Option<Value> = dir
        .and_then(|dir| std::fs::read_to_string(dir.join("profile.json")).ok())
        .and_then(|raw| serde_json::from_str(&raw).ok());
    let field = |k: &str| profile.as_ref().and_then(|p| p.get(k).cloned());
    let cloud = dir.and_then(cloud_identity);
    let id = configured_id
        .or(cloud.as_ref().map(|(id, _)| *id))
        .or_else(|| field("id").and_then(|v| v.as_i64()))
        .unwrap_or(-1);
    let name = match cloud {
        Some((cloud_id, name)) if cloud_id == id => name,
        _ => field("name")
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default(),
    };
    User {
        id,
        name,
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

/// Walks the edit journal of the local workspace with manifest id `workspace_id` from genesis and refuses when
/// it does not verify; on success the walk renews the checkpoint's full-verification time (best effort).
pub fn verify_journal_from_genesis(workspace_id: i64) -> Result<()> {
    let workspace = list()?
        .into_iter()
        .find(|w| w.manifest.id == workspace_id)
        .ok_or_else(|| Error::not_found("local workspace of the pickup not found"))?;
    let conn = tmgr_local::open_read(&workspace.database).map_err(local_error)?;
    let status = tmgr_local::verify_chain_full(&conn).map_err(local_error)?;
    if !status.ok {
        return Err(Error::new(
            Kind::Forbidden,
            "the edit journal of this workspace does not verify; nothing is approved",
        )
        .hint("open the workspace in the desktop app and check its backups"));
    }
    if let Ok(writer) = tmgr_local::open_write(&workspace.database) {
        let _ = tmgr_local::store_checkpoint(&writer, &status);
    }
    Ok(())
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

    fn hint(dir: &Path, content: &str) {
        std::fs::write(dir.join("cloud-identity.json"), content).unwrap();
    }

    fn hinted(content: &str) -> (i64, String) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("profile.json"),
            r#"{"id":-1,"name":"Owner"}"#,
        )
        .unwrap();
        hint(dir.path(), content);
        let me = identity(&dir.path().join("workspaces"), None);
        (me.id, me.name)
    }

    #[test]
    fn the_desktop_account_hint_beats_the_profile_and_loses_to_the_config() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspaces");
        hint(dir.path(), r#"{"id":7,"name":"  Ann\u0000 \u200bLee "}"#);
        let me = identity(&root, None);
        assert_eq!((me.id, me.name.as_str()), (7, "Ann Lee"));
        let me = identity(&root, Some(42));
        assert_eq!((me.id, me.name.as_str()), (42, ""));
        std::fs::write(
            dir.path().join("profile.json"),
            r#"{"id":9,"name":"Profile"}"#,
        )
        .unwrap();
        assert_eq!(identity(&root, None).id, 7);
        assert_eq!(identity(&root, Some(9)).name, "Profile");
        assert_eq!(identity(&root, Some(7)).name, "Ann Lee");
    }

    #[test]
    fn a_usable_hint_ignores_extra_keys_and_caps_the_name() {
        let long = "x".repeat(200);
        let (id, name) = hinted(&format!(r#"{{"id":5,"name":"{long}","other":[1]}}"#));
        assert_eq!((id, name.chars().count()), (5, 80));
        let (id, _) = hinted(r#"{"id":9007199254740991,"name":"Max"}"#);
        assert_eq!(id, 9_007_199_254_740_991);
    }

    #[test]
    fn an_unusable_hint_falls_back_to_the_profile() {
        let big = format!(r#"{{"id":5,"name":"{}"}}"#, "x".repeat(5000));
        let cases = [
            "not json",
            "[]",
            r#"{"id":"5","name":"A"}"#,
            r#"{"id":0,"name":"A"}"#,
            r#"{"id":-3,"name":"A"}"#,
            r#"{"id":5.5,"name":"A"}"#,
            r#"{"id":5.0,"name":"A"}"#,
            r#"{"id":9007199254740992,"name":"A"}"#,
            r#"{"id":99999999999999999999,"name":"A"}"#,
            r#"{"id":5,"name":7}"#,
            r#"{"id":5}"#,
            big.as_str(),
        ];
        for case in cases {
            assert_eq!(hinted(case), (-1, "Owner".to_string()), "{case}");
        }
    }

    #[test]
    fn a_missing_hint_or_a_directory_in_its_place_is_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspaces");
        assert_eq!(identity(&root, None).id, -1);
        std::fs::create_dir(dir.path().join("cloud-identity.json")).unwrap();
        assert_eq!(identity(&root, None).id, -1);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_or_writable_or_special_hint_is_ignored() {
        use std::os::unix::fs::{symlink, PermissionsExt};
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("workspaces");
        let real = dir.path().join("real.json");
        std::fs::write(&real, r#"{"id":5,"name":"A"}"#).unwrap();
        let link = dir.path().join("cloud-identity.json");
        symlink(&real, &link).unwrap();
        assert_eq!(identity(&root, None).id, -1);
        std::fs::remove_file(&link).unwrap();

        std::fs::copy(&real, &link).unwrap();
        assert_eq!(identity(&root, None).id, 5);
        for mode in [0o666, 0o662, 0o620] {
            std::fs::set_permissions(&link, std::fs::Permissions::from_mode(mode)).unwrap();
            assert_eq!(identity(&root, None).id, -1, "{mode:o}");
        }
        std::fs::set_permissions(&link, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert_eq!(identity(&root, None).id, 5);
        std::fs::remove_file(&link).unwrap();

        let fifo = std::ffi::CString::new(link.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert_eq!(identity(&root, None).id, -1);
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
