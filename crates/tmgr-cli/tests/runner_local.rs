#![cfg(feature = "runner")]

mod common;

use assert_cmd::Command;
use common::{stdout_json, text};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::PathBuf;
use tmgr_ipc::api::{AuthorKind, RunBundle};

struct Local {
    tmp: tempfile::TempDir,
}

impl Local {
    fn new() -> Local {
        let local = Local {
            tmp: tempfile::tempdir().unwrap(),
        };
        local.workspace("personal");
        local.workspace("second");
        std::fs::write(
            local.root().parent().unwrap().join("profile.json"),
            r#"{"id":-1,"uuid":"u","name":"Owner","created_at":"2026-10-01T00:00:00Z"}"#,
        )
        .unwrap();
        local
    }

    fn root(&self) -> PathBuf {
        self.tmp.path().join("tmgr-home/workspaces")
    }

    fn db(&self, code: &str) -> Connection {
        Connection::open(self.root().join(code).join("workspace.db")).unwrap()
    }

    fn workspace(&self, code: &str) {
        let dir = self.root().join(code);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("workspace.json"),
            json!({ "id": 1, "name": code, "code": code, "schema_version": 12, "created_at": "2026-10-01T00:00:00Z" })
                .to_string(),
        )
        .unwrap();
        let golden: Value = serde_json::from_str(
            &std::fs::read_to_string(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../tmgr-local/tests/parity/golden/schema.json"
            ))
            .unwrap(),
        )
        .unwrap();
        let conn = self.db(code);
        for object in golden["sqlite_master"].as_array().unwrap() {
            if let Some(ddl) = object["sql"].as_str() {
                conn.execute_batch(ddl).unwrap();
            }
        }
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('schema_version', '12')",
            [],
        )
        .unwrap();
        for s in golden["statuses"].as_array().unwrap() {
            conn.execute(
                "INSERT INTO statuses (id, name, type, color, sort_order, created_at, updated_at) VALUES (?, ?, ?, ?, ?, 'x', 'x')",
                rusqlite::params![
                    s["id"].as_i64(),
                    s["name"].as_str(),
                    s["type"].as_str(),
                    s["color"].as_str(),
                    s["sort_order"].as_i64()
                ],
            )
            .unwrap();
        }
        conn.execute_batch(
            "INSERT INTO tasks (id, title, description, status_id, created_at, updated_at)
             VALUES (1, 'Fix login', 'Users cannot sign in', 1, '2026-10-01T10:00:00.000Z', '2026-10-01T10:00:00.000Z'),
                    (2, 'Already moving', 'x', 2, '2026-10-01T10:00:00.000Z', '2026-10-01T10:00:00.000Z')",
        )
        .unwrap();
    }

    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("tmgr").unwrap();
        c.current_dir(self.tmp.path())
            .env_clear()
            .env("HOME", self.tmp.path())
            .env("XDG_CONFIG_HOME", self.tmp.path().join("xdg"))
            .env("XDG_STATE_HOME", self.tmp.path().join("state"))
            .env("TMGR_LOCAL_ROOT", self.root())
            .env("NO_COLOR", "1");
        c
    }

    fn helper_raw(&self, input: &[u8]) -> (bool, Value) {
        let out = self
            .cmd()
            .arg("__agent-work")
            .write_stdin(input.to_vec())
            .output()
            .unwrap();
        let body = text(&out.stdout);
        let parsed: Value = serde_json::from_str(body.trim_end())
            .unwrap_or_else(|_| panic!("stdout is not one JSON object ({} bytes)", body.len()));
        assert!(body.ends_with('\n') && body.trim_end().lines().count() == 1);
        assert!(parsed.is_object());
        (out.status.success(), parsed)
    }

    fn helper(&self, request: Value) -> (bool, Value) {
        self.helper_raw(request.to_string().as_bytes())
    }

    fn ok(&self, request: Value) -> Value {
        let (ok, out) = self.helper(request);
        assert!(ok, "helper failed: {out}");
        out
    }

    fn refused(&self, request: Value) -> String {
        let (ok, out) = self.helper(request);
        assert!(!ok);
        assert_eq!(out.as_object().unwrap().len(), 1);
        out["error"].as_str().unwrap().to_string()
    }

    fn start(&self, task: i64) -> Value {
        self.ok(json!({
            "op": "start", "workspace": "personal", "task_id": task, "agent": "codex",
            "model": "gpt-5", "branch": "tm-1-fix", "session_id": "s_abc123"
        }))
    }

    fn bundle(&self, cwd: &std::path::Path) -> std::process::Output {
        self.cmd()
            .args(["--json", "--ws", "local:personal", "agents", "bundle", "1"])
            .args(["--agent", "codex", "--model", "gpt-5", "--cwd"])
            .arg(cwd)
            .output()
            .unwrap()
    }
}

fn runs(local: &Local) -> Vec<(String, String, String, String)> {
    let conn = local.db("personal");
    let mut stmt = conn
        .prepare("SELECT agent, actor_kind, actor_id, status FROM agent_work_runs ORDER BY id")
        .unwrap();
    stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

fn run_actor_kind_via_local_api(local: &Local, task: i64) -> String {
    let ws = tmgr_local::find(&local.root(), "personal").unwrap();
    let conn = tmgr_local::open_read(&ws.database).unwrap();
    let now = chrono::Utc::now();
    let ctx = tmgr_local::Context {
        workspace: ws,
        user: tmgr_local::User {
            id: -1,
            name: "Owner".into(),
            email: String::new(),
        },
        now,
        local_offset: chrono::FixedOffset::east_opt(0).unwrap(),
        actor: None,
        origin: tmgr_local::WriteOrigin::Cli,
    };
    let r = tmgr_local::dispatch(
        &conn,
        &ctx,
        "GET",
        &format!("tasks/{task}/agent-work"),
        None,
    )
    .unwrap()
    .unwrap();
    r.data["data"]["runs"][0]["actor"]["kind"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn start_moves_a_backlog_task_to_active_and_records_the_runner_not_the_user() {
    let local = Local::new();
    let out = local.start(1);
    assert_eq!(out, json!({ "run_id": 1, "moved_to": 2 }));
    let status: i64 = local
        .db("personal")
        .query_row("SELECT status_id FROM tasks WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(status, 2);
    assert_eq!(
        runs(&local),
        [(
            "plugin:runner/codex".to_string(),
            "plugin".to_string(),
            "runner".to_string(),
            "running".to_string()
        )]
    );
    assert_eq!(run_actor_kind_via_local_api(&local, 1), "plugin");
}

#[test]
fn a_repeated_start_of_the_same_session_returns_its_running_run() {
    let local = Local::new();
    assert_eq!(local.start(2)["run_id"], 1);
    assert_eq!(local.start(2)["run_id"], 1);
    let all: Vec<String> = runs(&local).into_iter().map(|r| r.3).collect();
    assert_eq!(all, ["running"]);
}

#[test]
fn start_leaves_a_task_that_is_not_in_the_backlog_where_it_is() {
    let local = Local::new();
    let out = local.start(2);
    assert_eq!(out, json!({ "run_id": 1 }));
}

#[test]
fn update_then_finish_succeeded_and_failed() {
    let local = Local::new();
    local.start(1);
    local.ok(json!({
        "op": "update", "workspace": "personal", "task_id": 1, "run_id": 1,
        "branch": "tm-1-fix", "commits": ["abc1234 first change", "def5678 second change"]
    }));
    let out = local.ok(json!({
        "op": "finish", "workspace": "local:personal", "task_id": 1, "run_id": 1,
        "status": "succeeded", "summary": "done in 12 min\nwith 2 commits", "commits": ["abc1234 first change"]
    }));
    assert_eq!(out, json!({ "run_id": 1, "status": "succeeded" }));
    let conn = local.db("personal");
    let (status, summary, commits): (String, String, String) = conn
        .query_row(
            "SELECT status, summary, commits FROM agent_work_runs WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(status, "succeeded");
    assert!(summary.contains("with 2 commits"));
    assert!(commits.contains("abc1234") && commits.contains("first change"));

    let again = local.refused(json!({
        "op": "update", "workspace": "personal", "task_id": 1, "run_id": 1, "branch": "x"
    }));
    assert!(again.contains("already succeeded"), "{again}");

    let second = local.start(1);
    assert_eq!(second["run_id"], 2);
    local.ok(json!({
        "op": "finish", "workspace": "personal", "task_id": 1, "run_id": 2,
        "status": "failed", "summary": "exit 1"
    }));
    local.start(1);
    local.ok(json!({
        "op": "finish", "workspace": "personal", "task_id": 1, "run_id": 3, "status": "cancelled"
    }));
    let all: Vec<String> = runs(&local).into_iter().map(|r| r.3).collect();
    assert_eq!(all, ["succeeded", "failed", "cancelled"]);
    assert!(runs(&local).iter().all(|r| r.1 == "plugin"));
}

#[test]
fn a_run_of_another_task_is_refused() {
    let local = Local::new();
    local.start(1);
    let err = local.refused(json!({
        "op": "finish", "workspace": "personal", "task_id": 2, "run_id": 1, "status": "failed"
    }));
    assert!(err.contains("not a run of task 2"), "{err}");
}

#[test]
fn cloud_and_unknown_workspaces_are_refused() {
    let local = Local::new();
    for ws in ["acme", "12345", "local:nope", ""] {
        let err = local.refused(json!({
            "op": "start", "workspace": ws, "task_id": 1, "agent": "codex"
        }));
        assert!(err.contains("not a local workspace"), "{ws}: {err}");
    }
}

#[test]
fn invalid_input_is_refused_with_a_json_error() {
    let local = Local::new();
    let long = "x".repeat(301);
    let commits: Vec<String> = (0..201).map(|_| "abc1234 msg".to_string()).collect();
    let base = |extra: Value| {
        let mut v = json!({ "op": "update", "workspace": "personal", "task_id": 1, "run_id": 1 });
        for (k, val) in extra.as_object().unwrap() {
            v[k] = val.clone();
        }
        v
    };
    local.start(1);
    let cases = [
        json!({ "op": "start", "workspace": "personal", "task_id": 0, "agent": "codex" }),
        json!({ "op": "start", "workspace": "personal", "task_id": "1", "agent": "codex" }),
        json!({ "op": "start", "workspace": "personal", "task_id": 1, "agent": "co dex" }),
        json!({ "op": "start", "workspace": "personal", "task_id": 1, "agent": "codex", "model": "m".repeat(129) }),
        json!({ "op": "start", "workspace": "personal", "task_id": 1, "agent": "codex", "session_id": "s/../x" }),
        json!({ "op": "start", "workspace": "personal", "task_id": 1, "agent": "codex", "extra": 1 }),
        json!({ "op": "nope" }),
        base(json!({ "commits": commits })),
        base(json!({ "commits": [long] })),
        base(json!({ "commits": ["not-a-sha message"] })),
        base(json!({ "run_id": -4 })),
        json!({ "op": "finish", "workspace": "personal", "task_id": 1, "run_id": 1, "status": "done" }),
        json!({ "op": "finish", "workspace": "personal", "task_id": 1, "run_id": 1, "status": "failed", "summary": "s".repeat(2001) }),
    ];
    for case in cases {
        let (ok, out) = local.helper(case.clone());
        assert!(!ok, "accepted {case}");
        assert!(out["error"].is_string(), "{case}");
    }
    let (ok, out) = local.helper_raw(b"not json");
    assert!(!ok && out["error"].is_string());
    let (ok, out) = local.helper_raw(&vec![b' '; 64 * 1024 + 1]);
    assert!(!ok && out["error"].as_str().unwrap().contains("64 KiB"));
    assert_eq!(runs(&local).len(), 1);
}

#[test]
fn control_characters_are_stripped_from_text_fields() {
    let local = Local::new();
    local.ok(json!({
        "op": "start", "workspace": "personal", "task_id": 1, "agent": "codex",
        "branch": "tm\u{1b}[31m-1"
    }));
    let branch: String = local
        .db("personal")
        .query_row("SELECT branch FROM agent_work_runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(branch, "tm[31m-1");
}

fn seed_context(local: &Local) {
    let conn = local.db("personal");
    conn.execute_batch(
        "INSERT INTO personas (uuid, owner_user_id, owner_name, name, synced_at) VALUES ('p-1', -1, 'Owner', 'Scribe', 'x');
         INSERT INTO comments (task_id, message, created_at, updated_at, author_kind, author_id, author_name) VALUES
           (1, 'from me', '2026-10-01T10:05:00.000Z', '2026-10-01T10:05:00.000Z', 'user', '-1', 'Owner'),
           (1, 'legacy', '2026-10-01T10:05:30.000Z', '2026-10-01T10:05:30.000Z', 'user', NULL, NULL),
           (1, 'from persona', '2026-10-01T10:06:00.000Z', '2026-10-01T10:06:00.000Z', 'persona', 'p-1', 'Scribe'),
           (1, 'from plugin', '2026-10-01T10:07:00.000Z', '2026-10-01T10:07:00.000Z', 'plugin', 'tmgr.x', 'X'),
           (1, 'from someone', '2026-10-01T10:08:00.000Z', '2026-10-01T10:08:00.000Z', 'user', '77', 'Other');
         INSERT INTO pages (id, title, slug, body, version, author_id, author_kind, author_ref, updated_by_id, updated_by_kind, updated_by_ref, created_at, updated_at)
           VALUES (9, 'Spec', 'spec', '# Spec', 3, -1, 'user', '-1', -1, 'user', '-1', 'x', 'x');
         INSERT INTO page_links (page_id, target_kind, target_id) VALUES (9, 'task', 1);",
    )
    .unwrap();
}

#[test]
fn bundle_maps_authors_includes_pages_and_hashes_stably() {
    let local = Local::new();
    seed_context(&local);
    let cwd = local.tmp.path().join("work");
    std::fs::create_dir_all(&cwd).unwrap();
    let first = local.bundle(&cwd);
    assert!(first.status.success(), "{}", text(&first.stderr));
    let v = stdout_json(&first);
    let bundle: RunBundle = serde_json::from_value(v["data"]["bundle"].clone()).unwrap();
    assert!(bundle.validate().is_ok());
    assert_eq!(v["data"]["sha256"], bundle.sha256());
    assert_eq!(bundle.title, "Fix login");
    assert_eq!(bundle.body, "Users cannot sign in");
    assert_eq!(bundle.author.kind, AuthorKind::Other);
    assert_eq!(bundle.task.task_id, 1);
    assert!(bundle.task.workspace_id < 0);
    assert!(bundle.persona.is_none());
    let kinds: Vec<(AuthorKind, Option<&str>)> = bundle
        .comments
        .iter()
        .map(|c| (c.author.kind, c.author.id.as_deref()))
        .collect();
    assert_eq!(
        kinds,
        [
            (AuthorKind::Other, Some("-1")),
            (AuthorKind::Other, None),
            (AuthorKind::Agent, Some("p-1")),
            (AuthorKind::Other, Some("tmgr.x")),
            (AuthorKind::Other, Some("77")),
        ]
    );
    assert_ne!(bundle.author.kind, AuthorKind::Human);
    assert!(bundle
        .comments
        .iter()
        .all(|c| c.author.kind != AuthorKind::Human));
    assert_eq!(bundle.comments[0].at, "2026-10-01T10:05:00.000Z");
    assert_eq!(bundle.pages.len(), 1);
    assert_eq!((bundle.pages[0].id, bundle.pages[0].revision), (9, 3));
    assert_eq!(bundle.launch.model.as_deref(), Some("gpt-5"));
    assert_eq!(bundle.launch.permission_mode.as_deref(), Some("ask"));
    assert_eq!(
        bundle.launch.cwd,
        std::fs::canonicalize(&cwd).unwrap().to_string_lossy()
    );

    let second = stdout_json(&local.bundle(&cwd));
    assert_eq!(second["data"]["sha256"], v["data"]["sha256"]);

    let planned = stdout_json(
        &local
            .cmd()
            .args(["--json", "--ws", "local:personal", "agents", "bundle", "1"])
            .args([
                "--agent", "codex", "--model", "gpt-5", "--mode", "plan", "--cwd",
            ])
            .arg(&cwd)
            .output()
            .unwrap(),
    );
    assert_eq!(
        planned["data"]["bundle"]["launch"]["permission_mode"],
        "plan"
    );
    assert_ne!(planned["data"]["sha256"], v["data"]["sha256"]);
}

#[test]
fn bundle_accepts_a_task_key_and_refuses_cloud_workspaces() {
    let local = Local::new();
    let cwd = local.tmp.path().join("work");
    std::fs::create_dir_all(&cwd).unwrap();
    local
        .db("personal")
        .execute(
            "UPDATE tasks SET title = 'TM-7: Fix login' WHERE id = 1",
            [],
        )
        .unwrap();
    let out = local
        .cmd()
        .args([
            "--json",
            "--ws",
            "local:personal",
            "agents",
            "bundle",
            "TM-7",
        ])
        .args(["--agent", "codex", "--cwd"])
        .arg(&cwd)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(stdout_json(&out)["data"]["bundle"]["task"]["task_id"], 1);

    let out = local
        .cmd()
        .args([
            "--ws", "acme", "agents", "bundle", "1", "--agent", "codex", "--cwd",
        ])
        .arg(&cwd)
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(text(&out.stderr).contains("not a local workspace"));
}

#[test]
fn a_two_mebibyte_task_is_refused_with_a_clear_message() {
    let local = Local::new();
    local
        .db("personal")
        .execute(
            "UPDATE tasks SET description = ? WHERE id = 1",
            ["x".repeat(2 * 1024 * 1024)],
        )
        .unwrap();
    let cwd = local.tmp.path().join("work");
    std::fs::create_dir_all(&cwd).unwrap();
    let out = local.bundle(&cwd);
    assert!(!out.status.success());
    let err = text(&out.stderr) + &text(&out.stdout);
    assert!(err.contains("larger than 1 MiB"), "{err}");
}

#[test]
fn a_comment_with_a_malformed_timestamp_is_refused() {
    let local = Local::new();
    local
        .db("personal")
        .execute(
            "INSERT INTO comments (task_id, message, created_at, updated_at) VALUES (1, 'x', 'yesterday', 'x')",
            [],
        )
        .unwrap();
    let cwd = local.tmp.path().join("work");
    std::fs::create_dir_all(&cwd).unwrap();
    let out = local.bundle(&cwd);
    assert!(!out.status.success());
    assert!((text(&out.stderr) + &text(&out.stdout)).contains("RFC 3339"));
}

#[test]
fn bundle_refuses_a_task_with_too_many_comments_before_loading_them() {
    let local = Local::new();
    let conn = local.db("personal");
    for i in 0..501 {
        conn.execute(
            "INSERT INTO comments (task_id, message, created_at, updated_at, author_kind, author_id, author_name)
             VALUES (1, ?, '2026-10-01T10:05:00.000Z', '2026-10-01T10:05:00.000Z', 'user', '-1', 'Owner')",
            [format!("c{i}")],
        )
        .unwrap();
    }
    let cwd = local.tmp.path().join("work");
    std::fs::create_dir_all(&cwd).unwrap();
    let out = local.bundle(&cwd);
    assert!(!out.status.success());
    assert!(text(&out.stdout).contains("too many or too large comments"));
}
