#![cfg(feature = "runner")]

mod common;

use common::{stdout_json, text};
use portable_pty::{native_pty_system, Child, CommandBuilder, PtySize};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_tmgr");

struct Fx {
    tmp: TempDir,
}

struct Pty {
    screen: Arc<Mutex<vt100::Parser>>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    _master: Box<dyn portable_pty::MasterPty + Send>,
}

impl Drop for Pty {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

impl Pty {
    fn text(&self) -> String {
        self.screen.lock().unwrap().screen().contents()
    }

    fn wait_for(&self, needle: &str) {
        let end = Instant::now() + Duration::from_secs(20);
        while !self.text().contains(needle) {
            assert!(
                Instant::now() < end,
                "timed out waiting for {needle:?}; screen:\n{}",
                self.text()
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn line(&mut self, s: &str) {
        self.writer.write_all(format!("{s}\r").as_bytes()).unwrap();
        self.writer.flush().unwrap();
    }

    fn exit(&mut self) -> i32 {
        let end = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(st) = self.child.try_wait().unwrap() {
                return st.exit_code() as i32;
            }
            assert!(
                Instant::now() < end,
                "did not exit; screen:\n{}",
                self.text()
            );
            thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Fx {
    fn new() -> Fx {
        let fx = Fx {
            tmp: tempfile::tempdir().unwrap(),
        };
        std::fs::create_dir_all(fx.repo()).unwrap();
        std::fs::write(fx.repo().join("README.md"), "hi\n").unwrap();
        for args in [
            vec!["init", "-q", "-b", "main"],
            vec!["add", "."],
            vec!["commit", "-q", "-m", "init"],
        ] {
            let out = std::process::Command::new("git")
                .current_dir(fx.repo())
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_AUTHOR_NAME", "t")
                .env("GIT_AUTHOR_EMAIL", "t@example.com")
                .env("GIT_COMMITTER_NAME", "t")
                .env("GIT_COMMITTER_EMAIL", "t@example.com")
                .args(&args)
                .output()
                .unwrap();
            assert!(out.status.success(), "{args:?}");
        }
        let agents = fx.tmp.path().join("agents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::write(
            agents.join("sh-agent.toml"),
            "id = \"sh-agent\"\nname = \"Shell\"\ncommand = [\"/bin/sh\"]\n",
        )
        .unwrap();
        std::fs::create_dir_all(fx.root().join("personal")).unwrap();
        fx.workspace();
        fx
    }

    fn root(&self) -> PathBuf {
        self.tmp.path().join("tmgr-home/workspaces")
    }

    fn repo(&self) -> PathBuf {
        self.tmp.path().join("repo")
    }

    fn cfg(&self) -> PathBuf {
        self.tmp.path().join("xdg/tmgr")
    }

    fn db(&self) -> Connection {
        Connection::open(self.root().join("personal/workspace.db")).unwrap()
    }

    fn workspace(&self) {
        let dir = self.root().join("personal");
        std::fs::write(
            dir.join("workspace.json"),
            json!({"id": 1, "name": "Personal", "code": "personal", "schema_version": 12, "created_at": "2026-10-01T00:00:00Z"}).to_string(),
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
        let conn = self.db();
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
                rusqlite::params![s["id"].as_i64(), s["name"].as_str(), s["type"].as_str(), s["color"].as_str(), s["sort_order"].as_i64()],
            )
            .unwrap();
        }
        conn.execute_batch(
            "INSERT INTO categories (id, title, slug, code, settings, created_at, updated_at)
             VALUES (1, 'Work', 'work', 'WK', '[]', '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z');
             INSERT INTO personas (uuid, owner_user_id, owner_name, name, description, synced_at)
             VALUES ('p-1', -1, 'Owner', 'builder', 'Builds things', '2026-10-01T00:00:00Z');
             INSERT INTO workspace_personas (persona_uuid, permissions, enabled_at)
             VALUES ('p-1', '[]', '2026-10-01T00:00:00Z');",
        )
        .unwrap();
    }

    fn task(&self, id: i64, title: &str, body: &str) {
        self.task_as(id, title, body, true);
    }

    fn task_as(&self, id: i64, title: &str, body: &str, owned: bool) {
        let conn = self.db();
        conn.execute(
            "INSERT INTO tasks (id, title, description, status_id, project_category_id, category_tasks_sequence_id, created_at, updated_at)
             VALUES (?, ?, ?, 1, 1, ?, '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')",
            rusqlite::params![id, title, body, id],
        )
        .unwrap();
        if owned {
            conn.execute(
                "INSERT INTO activity_log (at, actor_kind, actor_id, actor_name, method, route, entity, entity_id, status)
                 VALUES ('2026-10-01T00:00:00Z', 'user', '-1', 'Owner', 'POST', 'tasks', 'tasks', ?, 200)",
                [id.to_string()],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO task_persona_assignees (task_id, persona_uuid, created_at) VALUES (?, 'p-1', '2026-10-01T00:00:00Z')",
            [id],
        )
        .unwrap();
    }

    fn envs(&self) -> Vec<(String, String)> {
        let t = self.tmp.path();
        [
            ("HOME", t.to_path_buf()),
            ("XDG_CONFIG_HOME", t.join("xdg")),
            ("XDG_STATE_HOME", t.join("state")),
            ("TMGR_CREDENTIAL_STORE", "file".into()),
            ("TMGR_SERVER", "http://127.0.0.1:9".into()),
            ("TMGR_LOCAL_ROOT", self.root()),
            ("TMGR_AGENTS_DIR", t.join("agents")),
            ("NO_COLOR", "1".into()),
            (
                "PATH",
                std::env::var_os("PATH")
                    .map(PathBuf::from)
                    .unwrap_or_default(),
            ),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string_lossy().into_owned()))
        .collect()
    }

    fn cmd(&self) -> assert_cmd::Command {
        let mut c = assert_cmd::Command::new(BIN);
        c.current_dir(self.tmp.path()).env_clear();
        for (k, v) in self.envs() {
            c.env(k, v);
        }
        c
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        self.cmd().args(args).output().unwrap()
    }

    fn json(&self, args: &[&str]) -> Value {
        let out = self.run(&[&["--json", "queue"], args].concat());
        assert!(
            out.status.success(),
            "{args:?}: {}{}",
            text(&out.stderr),
            text(&out.stdout)
        );
        stdout_json(&out)
    }

    fn pty(&self, args: &[&str], extra_env: &[(&str, &str)]) -> Pty {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 40,
                cols: 140,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut cmd = CommandBuilder::new(BIN);
        cmd.args(args);
        cmd.env_clear();
        cmd.cwd(self.tmp.path());
        for (k, v) in self.envs() {
            cmd.env(k, v);
        }
        for (k, v) in extra_env {
            cmd.env(k, v);
        }
        let child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let screen = Arc::new(Mutex::new(vt100::Parser::new(120, 140, 0)));
        let sink = screen.clone();
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                sink.lock().unwrap().process(&buf[..n]);
            }
        });
        Pty {
            screen,
            writer: pair.master.take_writer().unwrap(),
            child,
            _master: pair.master,
        }
    }

    fn confirmed(&self, args: &[&str], phrase: &str) -> (i32, String) {
        let mut p = self.pty(args, &[]);
        p.wait_for("Type ");
        p.line(phrase);
        let code = p.exit();
        (code, p.text())
    }

    fn add_rule(&self, name: &str, extra: &[&str]) -> (i32, String) {
        let repo = self.repo();
        let mut args = vec![
            "queue",
            "rules",
            "add",
            name,
            "--workspace",
            "local:personal",
            "--persona",
            "builder",
            "--profile",
            "sh-agent",
            "--repo",
            repo.to_str().unwrap(),
            "--project",
            "WK",
        ];
        args.extend_from_slice(extra);
        self.confirmed(&args, "I AGREE")
    }

    fn audit_events(&self) -> Vec<Value> {
        std::fs::read_to_string(self.cfg().join("queue-audit.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn count(&self, event: &str) -> usize {
        self.audit_events()
            .iter()
            .filter(|e| e["event"] == event)
            .count()
    }
}

fn mode(p: &std::path::Path) -> u32 {
    std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(p).unwrap().permissions()) & 0o777
}

#[test]
fn enabling_without_a_terminal_exits_10_even_with_yes() {
    let fx = Fx::new();
    for args in [
        vec!["queue", "enable"],
        vec!["--yes", "queue", "enable"],
        vec!["--yes", "queue", "approve", "q_x"],
    ] {
        let out = fx.run(&args);
        assert_eq!(
            out.status.code(),
            Some(10),
            "{args:?}: {}",
            text(&out.stderr)
        );
    }
    assert!(!fx.cfg().join("queue.toml").exists());
}

#[test]
fn run_token_in_the_environment_is_refused_with_exit_5() {
    let fx = Fx::new();
    for var in ["TMGR_RUN_TOKEN", "TMGR_RUN_ID"] {
        let out = fx
            .cmd()
            .env(var, "x")
            .args(["queue", "enable"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(5), "{var}");
        let out = fx
            .cmd()
            .env(var, "x")
            .args(["queue", "run-once"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(5), "{var}");
        let out = fx
            .cmd()
            .env(var, "x")
            .args(["queue", "resume"])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(5), "{var}");
        let mut p = fx.pty(&["queue", "enable"], &[(var, "x")]);
        assert_eq!(p.exit(), 5, "{var} on a terminal: {}", p.text());
    }
    assert!(!fx.cfg().join("queue.toml").exists());
}

#[test]
fn a_wrong_phrase_changes_nothing_and_the_right_one_adds_a_private_rule() {
    let fx = Fx::new();
    let (code, screen) = fx.add_rule("builder-backlog", &[]);
    assert_eq!(code, 0, "{screen}");
    let (code, _) = fx.confirmed(&["queue", "enable"], "i agree");
    assert_eq!(code, 130);
    assert!(fx.json(&["status"])["data"]["enabled"] == false);
    let (code, screen) = fx.confirmed(&["queue", "enable"], "I AGREE");
    assert_eq!(code, 0, "{screen}");
    assert!(screen.contains("RISK"));
    let status = fx.json(&["status"]);
    assert_eq!(status["data"]["enabled"], true);
    assert_eq!(status["data"]["rules"][0]["name"], "builder-backlog");
    assert_eq!(status["data"]["rules"][0]["auto"], false);
    assert_eq!(status["data"]["audit"]["ok"], true);
    assert_eq!(mode(&fx.cfg()), 0o700);
    for f in ["queue.toml", "queue-audit.jsonl"] {
        assert_eq!(mode(&fx.cfg().join(f)), 0o600, "{f}");
    }
    for extra in [
        vec!["--max-concurrent", "9"],
        vec!["--auto", "--trusted-author", "agent"],
    ] {
        let repo = fx.repo();
        let mut args = vec![
            "queue",
            "rules",
            "add",
            "bad",
            "--workspace",
            "local:personal",
            "--persona",
            "builder",
            "--profile",
            "sh-agent",
            "--repo",
            repo.to_str().unwrap(),
        ];
        args.extend(extra);
        assert_eq!(fx.pty(&args, &[]).exit(), 2);
    }
}

#[test]
fn cloud_workspaces_and_bypass_profiles_are_refused() {
    let fx = Fx::new();
    let repo = fx.repo();
    let mut p = fx.pty(
        &[
            "queue",
            "rules",
            "add",
            "cloud",
            "--workspace",
            "tmgrdev",
            "--persona",
            "builder",
            "--profile",
            "sh-agent",
            "--repo",
            repo.to_str().unwrap(),
        ],
        &[],
    );
    assert_eq!(p.exit(), 5, "{}", p.text());
    let agents = fx.tmp.path().join("agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(
        agents.join("risky.toml"),
        "id = \"risky\"\nname = \"Risky\"\ncommand = [\"claude\"]\nargs = [\"--dangerously-skip-permissions\"]\n",
    )
    .unwrap();
    let mut p = fx.pty(
        &[
            "queue",
            "rules",
            "add",
            "risky",
            "--workspace",
            "local:personal",
            "--persona",
            "builder",
            "--profile",
            "risky",
            "--repo",
            repo.to_str().unwrap(),
        ],
        &[],
    );
    assert_eq!(p.exit(), 5, "{}", p.text());
    assert!(!fx.cfg().join("queue.toml").exists());
}

#[test]
fn pause_resume_disable_and_removal_need_no_confirmation() {
    let fx = Fx::new();
    assert_eq!(fx.add_rule("r1", &[]).0, 0);
    assert_eq!(fx.confirmed(&["queue", "enable"], "I AGREE").0, 0);
    assert_eq!(fx.json(&["pause"])["data"]["paused"], true);
    assert_eq!(fx.json(&["status"])["data"]["paused"], true);
    assert_eq!(fx.json(&["resume"])["data"]["paused"], false);
    assert_eq!(fx.json(&["rules", "rm", "r1"])["data"]["removed"], "r1");
    assert_eq!(fx.json(&["disable"])["data"]["enabled"], false);
    assert_eq!(
        fx.run(&["queue", "rules", "rm", "r1"]).status.code(),
        Some(3)
    );
}

#[test]
fn the_whole_flow_proposes_approves_starts_once_and_audits() {
    let fx = Fx::new();
    fx.task(1, "Write the report", "Summarise the quarter");
    fx.task(2, "Agent wrote this", "from a persona");
    fx.task_as(3, "Imported", "no trail", false);
    fx.db()
        .execute(
            "INSERT INTO activity_log (at, actor_kind, actor_id, actor_name, method, route, entity, entity_id, status)
             VALUES ('2026-10-02T00:00:00Z', 'persona', 'p-1', 'builder', 'PATCH', 'tasks/:id', 'tasks', '2', 200)",
            [],
        )
        .unwrap();
    assert_eq!(fx.add_rule("r1", &[]).0, 0);
    assert_eq!(fx.confirmed(&["queue", "enable"], "I AGREE").0, 0);

    let first = fx.json(&["run-once"]);
    assert_eq!(first["data"]["mock_runs"], true, "{first}");
    let status = fx.json(&["status"]);
    let pickups = status["data"]["pickups"].as_array().unwrap();
    assert_eq!(pickups.len(), 1, "{status}");
    assert_eq!(pickups[0]["task"], "WK-1");
    assert_eq!(pickups[0]["status"], "pending_approval");
    assert_eq!(fx.count("skipped"), 2);
    let id = pickups[0]["id"].as_str().unwrap().to_string();
    let sha = pickups[0]["bundle_sha256"].as_str().unwrap().to_string();

    fx.run(&["queue", "run-once"]);
    assert!(!fx.cfg().join("queue-mock-runs.json").exists());

    let (code, screen) = fx.confirmed(&["queue", "approve", &id], &sha[..12]);
    assert_eq!(code, 0, "{screen}");
    for needle in [
        "Summarise the quarter",
        "Builds things",
        "permission prompts on",
        "-worktrees/queue/",
        "/bin/sh",
        "Executable",
    ] {
        assert!(screen.contains(needle), "{needle}: {screen}");
    }
    fx.run(&["queue", "run-once"]);
    fx.run(&["queue", "run-once"]);
    let mock: Value = serde_json::from_str(
        &std::fs::read_to_string(fx.cfg().join("queue-mock-runs.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(mock.as_array().unwrap().len(), 1);
    assert_eq!(mock[0]["id"], "mock-1");
    let status = fx.json(&["status"]);
    assert_eq!(status["data"]["pickups"][0]["status"], "started");
    assert_eq!(status["data"]["audit"]["ok"], true);
    assert_eq!(fx.count("approved"), 1);
    assert_eq!(fx.count("started"), 1);
}

#[test]
fn changing_the_task_after_the_prompt_is_shown_voids_the_approval() {
    let fx = Fx::new();
    fx.task(1, "Write the report", "v1");
    assert_eq!(fx.add_rule("r1", &[]).0, 0);
    assert_eq!(fx.confirmed(&["queue", "enable"], "I AGREE").0, 0);
    fx.run(&["queue", "run-once"]);
    let status = fx.json(&["status"]);
    let id = status["data"]["pickups"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let sha = status["data"]["pickups"][0]["bundle_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    let mut p = fx.pty(&["queue", "approve", &id], &[]);
    p.wait_for("Type the first 12");
    fx.run(&["queue", "run-once"]);
    fx.db()
        .execute(
            "UPDATE tasks SET description = 'v2 with new instructions' WHERE id = 1",
            [],
        )
        .unwrap();
    fx.run(&["queue", "run-once"]);
    p.line(&sha[..12]);
    assert_eq!(p.exit(), 6, "{}", p.text());
    fx.run(&["queue", "run-once"]);
    assert!(!fx.cfg().join("queue-mock-runs.json").exists());
    assert_eq!(
        fx.json(&["status"])["data"]["pickups"][0]["status"],
        "pending_approval"
    );
}

#[test]
fn a_hand_edited_rules_file_stops_the_queue() {
    let fx = Fx::new();
    fx.task(1, "Write the report", "v1");
    assert_eq!(fx.add_rule("r1", &[]).0, 0);
    assert_eq!(fx.confirmed(&["queue", "enable"], "I AGREE").0, 0);
    let path = fx.cfg().join("queue.toml");
    let edited = std::fs::read_to_string(&path)
        .unwrap()
        .replace("auto = false", "auto = true");
    std::fs::write(&path, edited).unwrap();
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();
    assert_eq!(fx.run(&["queue", "run-once"]).status.code(), Some(5));
    assert_eq!(fx.json(&["status"])["data"]["tampered"], true);
    assert_eq!(
        fx.run(&["queue", "rules", "rm", "r1"]).status.code(),
        Some(5)
    );
    assert_eq!(fx.count("config_tampered"), 1);
    assert!(!fx.cfg().join("queue-mock-runs.json").exists());
    assert_eq!(fx.json(&["disable"])["data"]["enabled"], false);
}

#[test]
fn a_hand_added_bypass_field_makes_the_file_unreadable() {
    let fx = Fx::new();
    assert_eq!(fx.add_rule("r1", &[]).0, 0);
    let path = fx.cfg().join("queue.toml");
    let edited = std::fs::read_to_string(&path).unwrap() + "mode = \"bypass\"\n";
    std::fs::write(&path, edited).unwrap();
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();
    let out = fx.run(&["queue", "status"]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
}

#[test]
fn the_audit_log_detects_tampering() {
    let fx = Fx::new();
    assert_eq!(fx.add_rule("r1", &[]).0, 0);
    assert_eq!(fx.confirmed(&["queue", "enable"], "I AGREE").0, 0);
    let path = fx.cfg().join("queue-audit.jsonl");
    let edited = std::fs::read_to_string(&path)
        .unwrap()
        .replacen("rule_add", "rule_xxx", 1);
    std::fs::write(&path, edited).unwrap();
    let status = fx.json(&["status"]);
    assert_eq!(status["data"]["audit"]["ok"], false);
}

#[test]
fn the_queue_runs_in_a_clone_not_in_the_repository() {
    let fx = Fx::new();
    fx.task(1, "Write the report", "v1");
    assert_eq!(fx.add_rule("r1", &[]).0, 0);
    assert_eq!(fx.confirmed(&["queue", "enable"], "I AGREE").0, 0);
    fx.run(&["queue", "run-once"]);
    let status = fx.json(&["status"]);
    let id = status["data"]["pickups"][0]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let sha = status["data"]["pickups"][0]["bundle_sha256"]
        .as_str()
        .unwrap()
        .to_string();
    assert_eq!(fx.confirmed(&["queue", "approve", &id], &sha[..12]).0, 0);
    fx.run(&["queue", "run-once"]);
    let clone = fx.tmp.path().join("repo-worktrees/queue").join(&id);
    assert!(clone.join("README.md").is_file());
    assert!(clone.join(".git/objects/info/alternates").is_file());
}

#[test]
fn enable_aborts_when_the_rules_change_while_the_warning_is_shown() {
    let fx = Fx::new();
    assert_eq!(fx.add_rule("r1", &[]).0, 0);
    let mut p = fx.pty(&["queue", "enable"], &[]);
    p.wait_for("Type I AGREE");
    let path = fx.cfg().join("queue.toml");
    let edited = std::fs::read_to_string(&path)
        .unwrap()
        .replace("auto = false", "auto = true");
    std::fs::write(&path, edited).unwrap();
    std::fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();
    p.line("I AGREE");
    assert_eq!(p.exit(), 6, "{}", p.text());
    assert_eq!(fx.json(&["status"])["data"]["enabled"], false);
}

#[test]
fn a_damaged_audit_log_stops_the_queue_until_enable_rebaselines_it() {
    let fx = Fx::new();
    assert_eq!(fx.add_rule("r1", &[]).0, 0);
    assert_eq!(fx.confirmed(&["queue", "enable"], "I AGREE").0, 0);
    let path = fx.cfg().join("queue-audit.jsonl");
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text.replacen("rule_add", "rule_xxx", 1)).unwrap();
    assert_eq!(fx.run(&["queue", "run-once"]).status.code(), Some(5));
    assert_eq!(fx.json(&["status"])["data"]["tampered"], true);
    assert_eq!(fx.confirmed(&["queue", "enable"], "I AGREE").0, 0);
    let status = fx.json(&["status"]);
    assert_eq!(status["data"]["audit"]["ok"], true);
    assert_eq!(status["data"]["tampered"], false);
    assert_eq!(fx.run(&["queue", "run-once"]).status.code(), Some(0));
}
