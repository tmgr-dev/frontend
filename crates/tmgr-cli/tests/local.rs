mod common;

use assert_cmd::Command;
use common::{assert_valid, dead_server, git, init_repo, path_with, stdout_json, text};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
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
        local.workspace("personal", "Personal", 13);
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

    fn terminal(&self, args: &[&str], extra_env: &[(&str, &str)], input: &str) -> i32 {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_tmgr"));
        cmd.args(["--ws", "local:personal"]);
        cmd.args(args);
        cmd.env_clear();
        cmd.cwd(self.tmp.path());
        for (k, v) in [
            ("HOME", self.tmp.path().to_path_buf()),
            ("XDG_CONFIG_HOME", self.tmp.path().join("xdg")),
            ("XDG_STATE_HOME", self.tmp.path().join("state")),
            ("TMGR_CREDENTIAL_STORE", "file".into()),
            ("TMGR_SERVER", self.server.clone().into()),
            ("TMGR_LOCAL_ROOT", self.root()),
            ("NO_COLOR", "1".into()),
        ] {
            cmd.env(k, v);
        }
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        let mut child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut writer = pair.master.take_writer().unwrap();
        let _ = std::io::Write::write_all(&mut writer, input.as_bytes());
        std::thread::spawn(move || {
            let mut sink = Vec::new();
            let _ = std::io::Read::read_to_end(&mut reader, &mut sink);
        });
        let status = child.wait().unwrap();
        status.exit_code() as i32
    }

    fn journal(&self) -> Vec<(String, String, String)> {
        let db = self.db("personal");
        let mut stmt = db
            .prepare("SELECT field, author_kind, source FROM edit_journal ORDER BY seq")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect()
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
    local.workspace("future", "Future", 14);
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

fn set_default_workspace(local: &Local, code: &str) {
    let dir = local.tmp.path().join("xdg/tmgr");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("config.toml"),
        format!("default_workspace = \"{code}\"\n"),
    )
    .unwrap();
}

#[test]
fn stop_and_done_follow_the_workspace_of_the_active_timer() {
    let local = Local::new();
    local.json(&["add", "Timed", "-p", "WK"]);
    local.json(&["start", "WK-1", "--no-branch"]);
    set_default_workspace(&local, "work");
    let out = local.cmd().args(["--json", "stop"]).output().unwrap();
    assert!(
        out.status.success(),
        "{}{}",
        text(&out.stderr),
        text(&out.stdout)
    );
    assert_eq!(stdout_json(&out)["data"]["stopped"], true);
    let running: i64 = local
        .db("personal")
        .query_row("SELECT start_time FROM tasks WHERE id = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(running, 0);
    let out = local
        .cmd()
        .args(["--json", "done", "--no-pr"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}{}",
        text(&out.stderr),
        text(&out.stdout)
    );
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
fn an_explicit_run_id_never_reaches_another_workspace() {
    let local = Local::new();
    local.workspace("other", "Other", 12);
    local.json(&["add", "Agent task", "-p", "WK"]);
    local.json(&["agent", "start", "WK-1", "--agent", "codex"]);
    let out = local
        .cmd()
        .args([
            "--ws",
            "local:other",
            "agent",
            "update",
            "--run",
            "1",
            "--task",
            "WK-1",
            "--summary",
            "wrong place",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let summary: Option<String> = local
        .db("personal")
        .query_row(
            "SELECT summary FROM agent_work_runs WHERE id = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(summary, None);
}

#[test]
fn pages_pulled_from_one_local_workspace_are_not_pushed_into_another() {
    let local = Local::new();
    local.workspace("other", "Other", 12);
    local
        .db("personal")
        .execute_batch(
            "INSERT INTO pages (id, title, slug, body, author_id, author_kind, author_ref, updated_by_id, updated_by_kind, updated_by_ref, created_at, updated_at)
             VALUES (1, 'Mine', 'mine', 'original', -1, 'user', '-1', -1, 'user', '-1', 'x', 'x');",
        )
        .unwrap();
    local
        .db("other")
        .execute_batch(
            "INSERT INTO pages (id, title, slug, body, author_id, author_kind, author_ref, updated_by_id, updated_by_kind, updated_by_ref, created_at, updated_at)
             VALUES (1, 'Theirs', 'theirs', 'untouched', -1, 'user', '-1', -1, 'user', '-1', 'x', 'x');",
        )
        .unwrap();
    let dir = local.tmp.path().join("pages");
    local.json(&["page", "pull", dir.to_str().unwrap()]);
    let file = dir.join("mine.md");
    let text_now = std::fs::read_to_string(&file).unwrap();
    std::fs::write(&file, text_now.replace("original", "edited")).unwrap();
    set_default_workspace(&local, "local:other");
    let out = local
        .cmd()
        .args(["page", "push", dir.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success(), "{}", text(&out.stdout));
    let theirs: String = local
        .db("other")
        .query_row("SELECT body FROM pages WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(theirs, "untouched");
}

#[test]
fn local_tracked_time_is_reported_as_unavailable() {
    let local = Local::new();
    local.json(&["add", "Long one", "-p", "WK"]);
    local
        .db("personal")
        .execute_batch("UPDATE tasks SET common_time = 144000")
        .unwrap();
    for args in [&["today"][..], &["status"][..]] {
        let v = local.json(args);
        let codes: Vec<&str> = v["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|w| w["code"].as_str())
            .collect();
        assert!(codes.contains(&"local_time_unavailable"), "{args:?}: {v}");
    }
    let status = local.json(&["status"]);
    assert_eq!(status["data"]["today"], Value::Null);
}

const AGENT_MARKERS: [&str; 7] = [
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CODEX_SANDBOX",
    "CODEX_SANDBOX_NETWORK_DISABLED",
    "TMGR_AGENT_SESSION",
    "TMGR_RUN_TOKEN",
    "TMGR_RUN_ID",
];

fn row(field: &str, kind: &str, source: &str) -> (String, String, String) {
    (field.into(), kind.into(), source.into())
}

fn snapshot_prefix(local: &Local, id: i64) -> String {
    let conn = tmgr_local::open_read(&local.root().join("personal/workspace.db")).unwrap();
    tmgr_local::snapshot(&conn, id).unwrap().unwrap().hash()[..12].to_string()
}

#[test]
fn a_cli_write_is_never_human_not_even_in_a_terminal() {
    let local = Local::new();
    let out = local.run(&["--json", "add", "Piped task", "-p", "WK"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        local.terminal(&["add", "Typed task", "-p", "WK"], &[], ""),
        0
    );
    assert_eq!(
        local.journal(),
        vec![
            row("create", "unattended", "cli"),
            row("create", "unattended", "cli")
        ]
    );
}

#[test]
fn confirm_needs_a_terminal_and_the_typed_snapshot() {
    let local = Local::new();
    let out = local.run(&["--json", "add", "Script task", "-p", "WK"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let piped = local.run(&["task", "confirm", "WK-1"]);
    assert_eq!(piped.status.code(), Some(10), "{}", text(&piped.stderr));
    let wrong = local.terminal(&["task", "confirm", "WK-1"], &[], "000000000000\n");
    assert_ne!(wrong, 0);
    assert_eq!(local.journal().len(), 1);
    let prefix = snapshot_prefix(&local, 1);
    assert_eq!(
        local.terminal(&["task", "confirm", "WK-1"], &[], &format!("{prefix}\n")),
        0
    );
    assert_eq!(
        local.journal().pop().unwrap(),
        row("confirm", "human", "cli_confirm")
    );
}

#[test]
fn confirm_is_refused_inside_an_agent_session_even_with_the_right_hash() {
    for marker in AGENT_MARKERS {
        let local = Local::new();
        let out = local.run(&["--json", "add", "Script task", "-p", "WK"]);
        assert!(out.status.success(), "{}", text(&out.stderr));
        let prefix = snapshot_prefix(&local, 1);
        let code = local.terminal(
            &["task", "confirm", "WK-1"],
            &[(marker, "1")],
            &format!("{prefix}\n"),
        );
        assert_eq!(code, 5, "{marker}");
        assert_eq!(local.journal().len(), 1, "{marker}");
    }
}

#[test]
fn a_schema_12_workspace_still_works_and_records_nothing() {
    let local = Local::new();
    local.workspace("old", "Old", 12);
    local
        .db("old")
        .execute_batch("DROP TABLE edit_journal")
        .unwrap();
    let out = local
        .cmd()
        .args(["--ws", "local:old", "--json", "add", "Old task", "-p", "WK"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let tasks: i64 = local
        .db("old")
        .query_row("SELECT COUNT(*) FROM tasks", [], |r| r.get(0))
        .unwrap();
    assert_eq!(tasks, 1);
}
