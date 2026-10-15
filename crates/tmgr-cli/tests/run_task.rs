#![cfg(feature = "runner")]

use rusqlite::Connection;
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tmgr_ipc::api::StateKind;
use tmgr_runner::client::Client;

const BIN: &str = env!("CARGO_BIN_EXE_tmgr");
const WS_ID: i64 = -444_001;

struct Fx {
    tmp: tempfile::TempDir,
    homes: tempfile::TempDir,
}

fn private_dir(path: &Path) {
    fs::create_dir_all(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn sandbox_usable() -> bool {
    let abi = tmgr_runner::daemon::landlock_abi();
    if abi >= 6 {
        return true;
    }
    eprintln!("skipped: Landlock ABI {abi} < 6");
    false
}

impl Fx {
    fn new() -> Fx {
        let tmp = tempfile::Builder::new()
            .prefix("rt")
            .tempdir_in("/tmp")
            .unwrap();
        private_dir(tmp.path());
        let homes = tempfile::Builder::new()
            .prefix("rh")
            .tempdir_in("/tmp")
            .unwrap();
        private_dir(homes.path());
        let fx = Fx { tmp, homes };
        private_dir(&fx.run());
        private_dir(&fx.agents());
        fs::create_dir_all(fx.repo()).unwrap();
        fs::write(fx.repo().join("README.md"), "hi\n").unwrap();
        git(&fx.repo(), &["init", "-q", "-b", "main"]);
        git(&fx.repo(), &["add", "."]);
        git(&fx.repo(), &["commit", "-q", "-m", "init"]);
        fx.workspace();
        fx
    }

    fn run(&self) -> PathBuf {
        self.tmp.path().join("r")
    }

    fn agents(&self) -> PathBuf {
        self.tmp.path().join("xdg/tmgr/agents")
    }

    fn root(&self) -> PathBuf {
        self.tmp.path().join("tmgr-home/workspaces")
    }

    fn repo(&self) -> PathBuf {
        fs::canonicalize(self.tmp.path()).unwrap().join("repo")
    }

    fn db(&self) -> Connection {
        Connection::open(self.root().join("personal/workspace.db")).unwrap()
    }

    fn workspace(&self) {
        let dir = self.root().join("personal");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("workspace.json"),
            json!({"id": WS_ID, "name": "Personal", "code": "personal", "schema_version": 13, "created_at": "2026-10-01T00:00:00Z"}).to_string(),
        )
        .unwrap();
        fs::write(
            self.tmp.path().join("tmgr-home/profile.json"),
            r#"{"id":-1,"uuid":"u","name":"Owner","created_at":"2026-10-01T00:00:00Z"}"#,
        )
        .unwrap();
        let golden: Value = serde_json::from_str(
            &fs::read_to_string(concat!(
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
            "INSERT INTO meta (key, value) VALUES ('schema_version', '13')",
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
             VALUES (1, 'Work', 'work', 'WK', '[]', '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z');",
        )
        .unwrap();
        let workspace = tmgr_local::find(&self.root(), "personal").unwrap();
        let conn = tmgr_local::open_write(&workspace.database).unwrap();
        let ctx = tmgr_local::Context {
            workspace,
            user: tmgr_local::User {
                id: -1,
                name: "Owner".into(),
                email: String::new(),
            },
            now: chrono::Utc::now(),
            local_offset: chrono::FixedOffset::east_opt(0).unwrap(),
            actor: None,
            origin: tmgr_local::WriteOrigin::DesktopUi,
        };
        let made = tmgr_local::dispatch(
            &conn,
            &ctx,
            "POST",
            "tasks",
            Some(&json!({ "title": "Fix login", "description": "Users cannot sign in", "status_id": 1, "project_category_id": 1 })),
        )
        .unwrap()
        .unwrap();
        assert!(made.status < 400, "{}", made.data);
    }

    fn agent(&self, body: &str) {
        fs::write(
            self.agents().join("fake.toml"),
            format!(
                "id = \"fake\"\nname = \"fake\"\ncommand = [\"/bin/sh\", \"-c\", {body:?}, \"fake\"]\n[permission_modes]\nask = [\"--ask\"]\nplan = [\"--plan\"]\n"
            ),
        )
        .unwrap();
    }

    fn map_project(&self) {
        let out = self.tmgr(&["project", "map", "WK", &self.repo().display().to_string()]);
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
        c.args(args)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_default())
            .env("HOME", self.tmp.path())
            .env("XDG_CONFIG_HOME", self.tmp.path().join("xdg"))
            .env("XDG_STATE_HOME", self.tmp.path().join("state"))
            .env("TMGR_LOCAL_ROOT", self.root())
            .env("TMGR_RUNNER_DIR", self.run())
            .env("TMGR_RUNNER_HOMES", self.homes.path())
            .env("TMGR_AGENTS_DIR", self.agents())
            .env("TMGR_T0_QUIET_MS", "300")
            .env("NO_COLOR", "1")
            .stdin(Stdio::null());
        c
    }

    fn tmgr(&self, args: &[&str]) -> Output {
        self.cmd(args).output().unwrap()
    }

    fn json(&self, args: &[&str]) -> (bool, Value) {
        let out = self.tmgr(args);
        let v: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "{args:?}: {e}: {}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        });
        (out.status.success(), v)
    }

    fn runs(&self) -> Vec<(String, String)> {
        let conn = self.db();
        let mut stmt = conn
            .prepare("SELECT agent, status FROM agent_work_runs ORDER BY id")
            .unwrap();
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    }
}

impl Drop for Fx {
    fn drop(&mut self) {
        let Ok(mut c) = Client::connect(&self.run()) else {
            return;
        };
        let _ = c.set_timeouts(Duration::from_secs(5));
        if let Ok(list) = c.list() {
            for s in list.iter().filter(|s| s.state.kind != StateKind::Exited) {
                let _ = c.kill(&s.id, "kill");
            }
        }
        let _ = c.stop();
    }
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool, context: impl Fn() -> String) {
    let deadline = Instant::now() + Duration::from_secs(30);
    while !done() {
        assert!(Instant::now() < deadline, "{what}\n{}", context());
        thread::sleep(Duration::from_millis(100));
    }
}

#[test]
fn a_local_task_runs_in_its_own_clone_and_is_recorded_as_agent_work() {
    if !sandbox_usable() {
        return;
    }
    let fx = Fx::new();
    fx.agent("printf '%s\\n' \"$@\" > args.txt\necho done > result.txt\ngit add result.txt\ngit -c user.name=a -c user.email=a@example.com commit -q -m 'agent change'\nexit 0\n");
    fx.map_project();
    let (ok, v) = fx.json(&[
        "--json",
        "--ws",
        "local:personal",
        "run",
        "WK-1",
        "--agent",
        "fake",
        "--yes",
    ]);
    assert!(ok, "{v}");
    let data = &v["data"];
    let clone = PathBuf::from(data["clone"].as_str().unwrap());
    let branch = data["branch"].as_str().unwrap().to_string();
    assert_eq!(branch, "wk-1-fix-login");
    assert_eq!(
        clone,
        fx.repo()
            .parent()
            .unwrap()
            .join("repo-worktrees/wk-1-fix-login")
    );
    assert_eq!(data["author"], "human");
    assert_eq!(data["session"]["permission_mode"], "ask");
    assert_eq!(
        data["fetch"].as_str().unwrap(),
        format!(
            "git -C {} fetch {} {branch}:{branch}",
            fx.repo().display(),
            clone.display()
        )
    );
    wait_for(
        "agent work finished",
        || fx.runs() == [("plugin:runner/fake".to_string(), "succeeded".to_string())],
        || {
            format!(
                "runs: {:?}\nrunner log:\n{}",
                fx.runs(),
                fs::read_to_string(fx.run().join("runner.log")).unwrap_or_default()
            )
        },
    );
    assert_eq!(
        fs::read_to_string(clone.join("args.txt")).unwrap(),
        "--ask\n"
    );
    assert!(clone.join(".tmgr/task.md").is_file());
    git(
        &fx.repo(),
        &[
            "fetch",
            "-q",
            &clone.display().to_string(),
            &format!("{branch}:{branch}"),
        ],
    );
    assert_eq!(
        git(&fx.repo(), &["log", "-1", "--format=%s", &branch]),
        "agent change"
    );
    assert!(!fx.repo().join("result.txt").exists());
}

#[test]
fn a_cloud_workspace_is_refused_with_the_local_only_rule() {
    let fx = Fx::new();
    fx.agent("exit 0\n");
    let (ok, v) = fx.json(&[
        "--json", "--ws", "tmgrdev", "run", "TM-123", "--agent", "fake", "--yes",
    ]);
    assert!(!ok);
    assert_eq!(v["error"]["code"], "usage", "{v}");
    let msg = v["error"]["message"].as_str().unwrap();
    assert!(
        msg.contains("local workspaces only") && msg.contains("16a.11"),
        "{msg}"
    );
    let out = fx.tmgr(&[
        "--ws", "tmgrdev", "run", "TM-123", "--agent", "fake", "--yes",
    ]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn an_unmapped_project_and_a_foreign_folder_at_the_clone_path_are_refused() {
    let fx = Fx::new();
    fx.agent("exit 0\n");
    let args = [
        "--json",
        "--ws",
        "local:personal",
        "run",
        "WK-1",
        "--agent",
        "fake",
        "--yes",
    ];
    let (ok, v) = fx.json(&args);
    assert!(!ok);
    assert!(
        v["error"]["message"]
            .as_str()
            .unwrap()
            .contains("no repository is mapped to project WK"),
        "{v}"
    );
    fx.map_project();
    let squatter = fx
        .repo()
        .parent()
        .unwrap()
        .join("repo-worktrees/wk-1-fix-login");
    fs::create_dir_all(&squatter).unwrap();
    let (ok, v) = fx.json(&args);
    assert!(!ok);
    assert_eq!(v["error"]["code"], "conflict", "{v}");
    assert!(fx.runs().is_empty());
}

#[test]
fn without_a_terminal_a_task_run_needs_yes() {
    let fx = Fx::new();
    fx.agent("exit 0\n");
    fx.map_project();
    let out = fx.tmgr(&["--ws", "local:personal", "run", "WK-1", "--agent", "fake"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--yes"));
    assert!(!fx.repo().parent().unwrap().join("repo-worktrees").exists());
}
