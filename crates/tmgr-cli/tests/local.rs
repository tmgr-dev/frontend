mod common;

use assert_cmd::Command;
use common::{assert_valid, dead_server, git, init_repo, path_with, stdout_json, text};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::PathBuf;
use tempfile::TempDir;

struct Local {
    tmp: TempDir,
    server: String,
}

impl Local {
    fn new() -> Local {
        let local = Local {
            tmp: tempfile::tempdir().unwrap(),
            server: dead_server(),
        };
        local.workspace("personal", "Personal", 12);
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

    fn workspace(&self, code: &str, name: &str, schema_version: u32) {
        let dir = self.root().join(code);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("workspace.json"),
            json!({ "id": 1, "name": name, "code": code, "schema_version": schema_version, "created_at": "2026-10-01T00:00:00Z" })
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
            "INSERT INTO meta (key, value) VALUES ('schema_version', ?)",
            [schema_version.to_string()],
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
            "INSERT INTO categories (id, title, slug, code, settings, created_at, updated_at)
             VALUES (1, 'Work', 'work', 'WK', '[]', '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')",
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
            .env("TMGR_CREDENTIAL_STORE", "file")
            .env("TMGR_SERVER", &self.server)
            .env("TMGR_LOCAL_ROOT", self.root())
            .env("NO_COLOR", "1");
        c
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        self.cmd()
            .args(["--ws", "local:personal"])
            .args(args)
            .output()
            .unwrap()
    }

    fn json(&self, args: &[&str]) -> Value {
        let out = self.run(&[&["--json"], args].concat());
        assert!(
            out.status.success(),
            "{args:?}: {}{}",
            text(&out.stderr),
            text(&out.stdout)
        );
        stdout_json(&out)
    }
}

#[test]
fn ws_list_shows_local_workspaces_without_a_login() {
    let local = Local::new();
    let out = local.cmd().args(["--json", "ws", "list"]).output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_eq!(v["data"]["workspaces"][0]["code"], "local:personal");
    assert_eq!(v["data"]["workspaces"][0]["local"], true);
    assert!(v["data"]["workspaces"][0]["id"].as_i64().unwrap() < 0);

    let used = local
        .cmd()
        .args(["ws", "use", "local:personal"])
        .output()
        .unwrap();
    assert!(used.status.success(), "{}", text(&used.stderr));
    let out = local
        .cmd()
        .args(["--json", "task", "list"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let missing = local
        .cmd()
        .args(["ws", "use", "local:nope"])
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(3));
}

#[test]
fn tasks_are_created_listed_shown_and_moved() {
    let local = Local::new();
    let created = local.json(&["add", "First local task", "-p", "WK"]);
    assert_valid("task_created", &created);
    assert_eq!(created["data"]["ref"], "WK-1");
    assert_eq!(created["data"]["url"], Value::Null);

    let list = local.json(&["task", "list"]);
    assert_valid("task_list", &list);
    assert_eq!(list["data"]["items"][0]["ref"], "WK-1");

    let shown = local.json(&["task", "show", "WK-1"]);
    assert_valid("task", &shown);
    assert_eq!(shown["data"]["task"]["title"], "First local task");

    let started = local.json(&["start", "WK-1", "--no-branch"]);
    assert_valid("start", &started);
    let status = local.json(&["status"]);
    assert_valid("status", &status);
    assert_eq!(status["data"]["active"]["task"]["ref"], "WK-1");
    assert_eq!(status["data"]["active"]["timer"]["running"], true);
    let stopped = local.json(&["stop"]);
    assert_valid("stop", &stopped);
    let done = local.json(&["done", "WK-1", "--no-pr"]);
    assert_valid("done", &done);

    let kind: String = local
        .db("personal")
        .query_row(
            "SELECT s.type FROM tasks t JOIN statuses s ON s.id = t.status_id WHERE t.id = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(kind, "completed");
}

#[test]
fn agent_runs_are_started_updated_finished_and_logged() {
    let local = Local::new();
    local.json(&["add", "Agent task", "-p", "WK"]);
    let started = local.json(&["agent", "start", "WK-1", "--agent", "claude-code"]);
    assert_valid("agent_run", &started);
    let run = started["data"]["run_id"].as_i64().unwrap().to_string();
    local.json(&["agent", "update", "--run", &run, "--summary", "half way"]);
    local.json(&["agent", "finish", "--run", &run, "--status", "succeeded"]);

    let log = local.json(&["log"]);
    assert_valid("run_list", &log);
    assert_eq!(log["data"]["runs"][0]["status"], "succeeded");
    let today = local.json(&["today", "--mine"]);
    assert_valid("today", &today);

    let actor: String = local
        .db("personal")
        .query_row("SELECT actor_id FROM agent_work_runs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(actor, "-1");
}

#[test]
fn routines_are_listed_and_completed() {
    let local = Local::new();
    local
        .db("personal")
        .execute_batch(
            "INSERT INTO routines (id, title, created_at, updated_at) VALUES (1, 'Stretch', 'x', 'x');
             INSERT INTO routine_patterns (routine_id, frequency, interval, start_at, created_at, updated_at)
             VALUES (1, 'DAILY', 1, '2026-01-01', 'x', 'x');",
        )
        .unwrap();
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    let list = local.json(&["routine", "list", "--date", &today]);
    assert_valid("routine_list", &list);
    assert_eq!(list["data"]["routines"][0]["title"], "Stretch");
    let done = local.json(&["routine", "done", "1", "--date", &today]);
    assert_valid("routine_done", &done);
    let status: String = local
        .db("personal")
        .query_row("SELECT status FROM routine_instances", [], |r| r.get(0))
        .unwrap();
    assert_eq!(status, "COMPLETED");
}

#[test]
fn notes_become_pages_that_sync_as_markdown() {
    let local = Local::new();
    let editor = local.tmp.path().join("editor.sh");
    std::fs::write(
        &editor,
        "#!/bin/sh\nprintf '# Local note\\n\\nabout WK-1\\n' > \"$1\"\n",
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&editor, std::fs::Permissions::from_mode(0o755)).unwrap();
    local.json(&["add", "Linked", "-p", "WK"]);
    let out = local
        .cmd()
        .env("EDITOR", &editor)
        .args(["--ws", "local:personal", "note"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));

    let dir = local.tmp.path().join("pages");
    let pulled = local.json(&["page", "pull", dir.to_str().unwrap()]);
    assert_valid("page_sync", &pulled);
    let file = std::fs::read_to_string(dir.join("local-note.md")).unwrap();
    assert!(file.contains("workspace: local:personal"), "{file}");
    assert!(file.contains("[WK-1](tmgr://task/1)"), "{file}");
    std::fs::write(dir.join("local-note.md"), file.replace("about", "edited")).unwrap();
    let pushed = local.json(&["page", "push", dir.to_str().unwrap()]);
    assert_valid("page_sync", &pushed);
    let body: String = local
        .db("personal")
        .query_row("SELECT body FROM pages WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert!(body.contains("edited"), "{body}");
}

#[test]
fn a_newer_schema_is_refused_and_auth_stays_with_the_cloud() {
    let local = Local::new();
    local.workspace("future", "Future", 13);
    let out = local
        .cmd()
        .args(["--ws", "local:future", "task", "list"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("newer TMGR"),
        "{}",
        text(&out.stderr)
    );

    let out = local.run(&["whoami"]);
    assert_eq!(out.status.code(), Some(4), "{}", text(&out.stderr));
}

#[test]
fn a_hook_reports_commits_to_the_run_of_a_local_workspace_without_a_login() {
    let local = Local::new();
    let used = local
        .cmd()
        .args(["ws", "use", "local:personal"])
        .output()
        .unwrap();
    assert!(used.status.success(), "{}", text(&used.stderr));
    local.json(&["add", "Hooked", "-p", "WK"]);
    let repo = init_repo(&local.tmp.path().join("repo"));
    git(&repo, &["checkout", "-q", "-b", "wk-1-hooked"]);
    let started = local
        .cmd()
        .current_dir(&repo)
        .env("PATH", path_with(None))
        .args(["agent", "start", "WK-1", "--agent", "claude-code"])
        .output()
        .unwrap();
    assert!(started.status.success(), "{}", text(&started.stderr));
    std::fs::write(repo.join("a.txt"), "a").unwrap();
    git(&repo, &["add", "a.txt"]);
    git(&repo, &["commit", "-q", "-m", "feat: local change"]);

    let stop = json!({ "session_id": "s1", "cwd": repo.display().to_string() }).to_string();
    let out = local
        .cmd()
        .env("PATH", path_with(None))
        .env("TMGR_HOOK_DEBUG", "1")
        .env("TMGR_HOOK_BUDGET_MS", "5000")
        .args(["hook", "claude", "stop"])
        .write_stdin(stop)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let commits: String = local
        .db("personal")
        .query_row(
            "SELECT commits FROM agent_work_runs WHERE id = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(
        commits.contains("feat: local change"),
        "{commits} / {}",
        text(&out.stderr)
    );
}

#[test]
fn a_key_missing_from_the_local_default_is_not_searched_in_other_workspaces() {
    let local = Local::new();
    local.workspace("team", "Team", 12);
    let used = local
        .cmd()
        .args(["ws", "use", "local:personal"])
        .output()
        .unwrap();
    assert!(used.status.success(), "{}", text(&used.stderr));
    let created = local
        .cmd()
        .args([
            "--json",
            "--ws",
            "local:team",
            "add",
            "Team task",
            "-p",
            "WK",
        ])
        .output()
        .unwrap();
    assert!(created.status.success(), "{}", text(&created.stderr));

    // The local client is bound to one workspace.db, so searching "other"
    // workspaces through it would find the key in the wrong database.
    let shown = local.cmd().args(["task", "show", "WK-1"]).output().unwrap();
    assert_eq!(shown.status.code(), Some(3), "{}", text(&shown.stderr));
    let count: i64 = local
        .db("personal")
        .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}
