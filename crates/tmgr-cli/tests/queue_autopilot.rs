#![cfg(feature = "runner")]

mod common;

use common::{assert_valid, git, init_repo, stdout_json, text, write_script, Env, ACCESS};
use portable_pty::{native_pty_system, Child, CommandBuilder, PtySize};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Output;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, Request, Respond, ResponseTemplate};

const BIN: &str = env!("CARGO_BIN_EXE_tmgr");

struct Fx {
    home: TempDir,
    runner: TempDir,
}

fn private_tmp(prefix: &str) -> TempDir {
    let dir = tempfile::Builder::new()
        .prefix(prefix)
        .tempdir_in("/tmp")
        .unwrap();
    std::fs::set_permissions(
        dir.path(),
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .unwrap();
    dir
}

impl Fx {
    fn new() -> Fx {
        Fx {
            home: private_tmp("apc-h"),
            runner: private_tmp("apc-r"),
        }
    }

    fn run(&self, args: &[&str], extra_env: &[(&str, &str)]) -> Output {
        let t = self.home.path();
        let mut c = assert_cmd::Command::new(BIN);
        c.current_dir(t)
            .env_clear()
            .env("HOME", t)
            .env("XDG_CONFIG_HOME", t.join("xdg"))
            .env("XDG_STATE_HOME", t.join("state"))
            .env("TMGR_CREDENTIAL_STORE", "file")
            .env("TMGR_SERVER", "http://127.0.0.1:9")
            .env("TMGR_RUNNER_DIR", self.runner.path())
            .env("NO_COLOR", "1")
            .env("PATH", "/usr/bin:/bin");
        for (k, v) in extra_env {
            c.env(k, v);
        }
        c.args(args).output().unwrap()
    }
}

#[test]
fn starting_without_a_terminal_exits_10_even_with_yes() {
    let fx = Fx::new();
    for args in [
        vec!["queue", "autopilot"],
        vec!["queue", "autopilot", "start"],
        vec!["--yes", "queue", "autopilot", "--rule", "tm"],
        vec!["--yes", "queue", "autopilot", "start", "--no-usage-limit"],
    ] {
        let out = fx.run(&args, &[]);
        assert_eq!(
            out.status.code(),
            Some(10),
            "{args:?}: {}",
            text(&out.stderr)
        );
    }
}

#[test]
fn an_agent_session_cannot_start_or_resume() {
    let fx = Fx::new();
    for var in ["TMGR_RUN_TOKEN", "TMGR_RUN_ID"] {
        for args in [
            vec!["queue", "autopilot"],
            vec!["--yes", "queue", "autopilot", "start"],
            vec!["queue", "autopilot", "resume"],
        ] {
            let out = fx.run(&args, &[(var, "x")]);
            assert_eq!(
                out.status.code(),
                Some(5),
                "{var} {args:?}: {}",
                text(&out.stderr)
            );
        }
    }
}

#[test]
fn status_without_a_runner_says_so_and_needs_no_terminal() {
    let fx = Fx::new();
    let out = fx.run(&["queue", "autopilot", "status"], &[]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stdout).starts_with("Autopilot is not running."));
    let out = fx.run(&["--json", "queue", "autopilot", "status"], &[]);
    assert_eq!(out.status.code(), Some(0));
    let v: Value = stdout_json(&out);
    assert_eq!(v["data"]["running"], false);
    assert_valid("queue-autopilot-status", &v);
}

#[test]
fn watch_with_json_is_a_usage_error() {
    let fx = Fx::new();
    let out = fx.run(&["--json", "queue", "autopilot", "status", "--watch"], &[]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn pause_stop_and_log_without_a_runner_say_autopilot_is_not_running() {
    let fx = Fx::new();
    for args in [
        vec!["queue", "autopilot", "pause"],
        vec!["queue", "autopilot", "stop"],
        vec!["queue", "autopilot", "log"],
    ] {
        let out = fx.run(&args, &[]);
        assert_eq!(
            out.status.code(),
            Some(6),
            "{args:?}: {}",
            text(&out.stderr)
        );
        assert!(text(&out.stderr).contains("autopilot is not running"));
    }
}

#[test]
fn the_help_lists_every_subcommand_and_flag() {
    let fx = Fx::new();
    let out = fx.run(&["queue", "autopilot", "--help"], &[]);
    let help = text(&out.stdout);
    for word in [
        "start",
        "status",
        "pause",
        "resume",
        "stop",
        "log",
        "--rule",
        "--tasks",
        "--max-teams",
        "--team-size",
        "--team-mode",
        "--profile",
        "--only",
        "--skip",
        "--order",
        "--dry-run",
        "--usage-soft",
        "--usage-hard",
        "--no-usage-limit",
        "--resume-after-reset",
        "--max-cost",
        "--max-hours",
        "--max-tasks",
    ] {
        assert!(help.contains(word), "{word} missing in\n{help}");
    }
}

const FIXTURES: &str = include_str!("../../tmgr-core/tests/fixtures/agent-approval-fixtures.json");
const TASK: i64 = 9002;
const TASK2: i64 = 9003;
const WS: i64 = 56;
const CATEGORY: i64 = 127;
const OWN_KEY_ID: &str = "5d0e7c9a-2b1f-4a63-8c47-0a9e1f2b3c4d";
const PR: &str = "https://github.com/acme/widgets/pull/5";

#[derive(Default)]
struct Remote {
    public_key: Option<String>,
    bulk: Vec<Value>,
    approvals: std::collections::BTreeMap<i64, Value>,
}

type Shared = Arc<Mutex<Remote>>;

struct Keys(Shared, Value);

impl Respond for Keys {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let mut list = vec![self.1.clone()];
        if let Some(pk) = &self.0.lock().unwrap().public_key {
            list.push(json!({"key_id": OWN_KEY_ID, "public_key": pk, "device_name": "CLI test", "created_at": "2026-10-19T00:00:00Z"}));
        }
        ResponseTemplate::new(200).set_body_json(json!({ "data": list }))
    }
}

struct Register(Shared);

impl Respond for Register {
    fn respond(&self, r: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&r.body).unwrap();
        self.0.lock().unwrap().public_key = body["public_key"].as_str().map(str::to_string);
        ResponseTemplate::new(200).set_body_json(json!({"data": {"key_id": OWN_KEY_ID}}))
    }
}

struct Bulk(Shared);

impl Respond for Bulk {
    fn respond(&self, r: &Request) -> ResponseTemplate {
        let body: Value = serde_json::from_slice(&r.body).unwrap();
        let mut remote = self.0.lock().unwrap();
        let mut answers = Vec::new();
        for item in body["items"].as_array().unwrap() {
            remote.bulk.push(item.clone());
            let mut approval = item.clone();
            approval["revoked_at"] = Value::Null;
            approval["revoked_reason"] = Value::Null;
            remote
                .approvals
                .insert(item["task_id"].as_i64().unwrap(), approval);
            answers.push(json!({"task_id": item["task_id"], "ok": true}));
        }
        ResponseTemplate::new(200).set_body_json(json!({ "data": answers }))
    }
}

struct Eligibility(Shared, i64);

impl Respond for Eligibility {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        let (title, description, key) = if self.1 == TASK {
            ("Fix login", "The login form drops the session", "TM-410")
        } else {
            ("Update docs", "The install guide is outdated", "TM-411")
        };
        ResponseTemplate::new(200).set_body_json(json!({"data": {
            "task_id": self.1, "workspace_id": WS, "workspace_code": "tmgrdev", "viewer_user_id": 1,
            "title": title, "description": description, "task_key": key,
            "setting_enabled": true,
            "category": {"id": CATEGORY, "code": "TM", "github_full_name": "acme/widgets", "default_branch": "main"},
            "approval": self.0.lock().unwrap().approvals.get(&self.1).cloned(),
            "persona": persona_json(),
            "context": {"pages": context_pages()},
        }}))
    }
}

fn v3_case() -> Value {
    let f: Value = serde_json::from_str(FIXTURES).unwrap();
    f["cases_v3"][0].clone()
}

fn persona_json() -> Value {
    v3_case()["persona"].clone()
}

fn context_pages() -> Value {
    v3_case()["context"].clone()
}

fn open_task() -> Value {
    open_task_n(TASK, "Fix login", "The login form drops the session")
}

fn open_task_n(id: i64, title: &str, description: &str) -> Value {
    let mut t = common::task_json(id as u64, title);
    t["description"] = json!(description);
    t["status_id"] = json!(1);
    t["project_category_id"] = json!(CATEGORY);
    t["workspace_id"] = json!(WS);
    t["assignees"] = json!([]);
    t["persona_assignees"] = json!([{"id": "p-1", "name": "builder"}]);
    t
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
        let end = Instant::now() + Duration::from_secs(30);
        while !self.text().contains(needle) {
            assert!(
                Instant::now() < end,
                "timed out waiting for {needle:?}; screen:\n{}",
                self.text()
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn wait_for_paged(&mut self, needle: &str) {
        let end = Instant::now() + Duration::from_secs(30);
        let mut answered = 0;
        while !self.text().contains(needle) {
            assert!(
                Instant::now() < end,
                "timed out waiting for {needle:?}; screen:\n{}",
                self.text()
            );
            if self.text().matches("press Enter").count() > answered {
                answered += 1;
                self.line("");
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn line(&mut self, s: &str) {
        self.writer.write_all(format!("{s}\r").as_bytes()).unwrap();
        self.writer.flush().unwrap();
    }

    fn exit(&mut self) -> i32 {
        let end = Instant::now() + Duration::from_secs(30);
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

struct Cloud {
    env: Env,
    stub: TempDir,
    runner: TempDir,
    homes: TempDir,
    repo: PathBuf,
    bare: PathBuf,
    remote: Shared,
    second: bool,
}

impl Drop for Cloud {
    fn drop(&mut self) {
        if let Ok(mut c) = tmgr_runner::client::Client::connect(&self.runner.path().join("r")) {
            let _ = c.set_timeouts(Duration::from_secs(5));
            if let Ok(list) = c.list() {
                for s in list
                    .iter()
                    .filter(|s| s.state.kind != tmgr_ipc::api::StateKind::Exited)
                {
                    let _ = c.kill(&s.id, "kill");
                }
            }
        }
        let _ = self.cmd(&["runner", "stop"]);
    }
}

fn sandbox_usable() -> bool {
    let abi = tmgr_runner::daemon::landlock_abi();
    if abi >= 6 {
        return true;
    }
    eprintln!("skipped: Landlock ABI {abi} < 6");
    false
}

impl Cloud {
    async fn new() -> Cloud {
        Cloud::build(false).await
    }

    async fn build(second: bool) -> Cloud {
        let env = Env::new().await;
        env.login_seed();
        let root = env.tmp.path().to_path_buf();
        std::fs::set_permissions(&root, std::os::unix::fs::PermissionsExt::from_mode(0o700))
            .unwrap();
        std::fs::create_dir_all(env.config_dir()).unwrap();
        std::fs::write(
            env.config_dir().join("config.toml"),
            format!(
                "server = \"{}\"\ncredential_store = \"file\"\n",
                env.server.uri()
            ),
        )
        .unwrap();
        let repo = init_repo(&root.join("code/repo"));
        git(
            &repo,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/acme/widgets.git",
            ],
        );
        git(&repo, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        git(
            &repo,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        );
        let bare = root.join("remote.git");
        git(&root, &["init", "-q", "--bare", bare.to_str().unwrap()]);
        std::fs::write(
            root.join(".gitconfig"),
            format!(
                "[url \"{}\"]\n\tinsteadOf = https://github.com/acme/widgets.git\n",
                bare.display()
            ),
        )
        .unwrap();
        let stub = tempfile::tempdir().unwrap();
        write_script(
            stub.path(),
            "gh",
            &format!(
                "printf '%s\\n' \"$@\" >> \"{}/gh-argv.txt\"\necho {PR}",
                root.display()
            ),
        );
        write_script(
            stub.path(),
            "claude",
            r#"if [ "$1" = "-p" ]; then
  triage=$(grep -B2 '"title": "Update docs"' orchestrator-input.json | grep -o '"ref": *[0-9]*' | grep -o '[0-9]*$')
  plan=""
  for n in $(grep -o '"ref": *[0-9]*' orchestrator-input.json | grep -o '[0-9]*$'); do
    [ "$n" = "$triage" ] && continue
    plan="${plan}${plan:+,}{\"ref\":$n,\"group\":\"\",\"mode\":\"eco\",\"reason\":\"ready\"}"
  done
  tri=""
  [ -n "$triage" ] && tri="{\"ref\":$triage,\"verdict\":\"needs_feedback\",\"reason\":\"The goal is unclear\",\"question\":\"Which guide should change\"}"
  printf '{"version":1,"plan":[%s],"triage":[%s]}' "$plan" "$tri" > orchestrator-plan.json
  exit 0
fi
echo working >> README.md
echo done > agent-out.txt
exec sleep 600"#,
        );
        let f: Value = serde_json::from_str(FIXTURES).unwrap();
        let allow = json!({"version": 1, "entries": [{
            "server": env.server.address().to_string(), "user_id": 1, "workspace_id": WS,
            "workspace_code": "tmgrdev", "allowed_at": "2025-10-01T00:00:00Z", "expires_at": null,
            "keys": [{"key_id": f["key"]["key_id"], "public_key": f["key"]["public_key"], "device_name": "laptop"}],
            "repos": {CATEGORY.to_string(): repo.display().to_string()}}]});
        std::fs::write(
            env.config_dir().join("runner-allow.json"),
            allow.to_string(),
        )
        .unwrap();
        std::fs::write(
            root.join("usage.json"),
            json!({"session": 10.0, "weekly": 5.0, "billing": "subscription"}).to_string(),
        )
        .unwrap();
        let cloud = Cloud {
            env,
            stub,
            runner: private_tmp("apr"),
            homes: private_tmp("aph"),
            repo,
            bare,
            remote: Shared::default(),
            second,
        };
        cloud.mount(&f["key"]).await;
        cloud
    }

    async fn mount(&self, other_key: &Value) {
        let server = &self.env.server;
        let auth = format!("Bearer {ACCESS}");
        let get = |p: &str, body: Value| {
            Mock::given(method("GET"))
                .and(path(p.to_string()))
                .and(header("authorization", auth.as_str()))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .mount(server)
        };
        get(
            "/api/auth/devices/current",
            json!({"data": {"device_id": 9, "user": {"id": 1, "name": "Ann"}, "scopes": ["tasks:read", "agent-approvals:write"]}}),
        )
        .await;
        get(
            "/api/workspaces",
            json!({"data": [{"id": WS, "name": "Tmgr Dev", "code": "tmgrdev", "is_default": true}]}),
        )
        .await;
        get(
            &format!("/api/workspaces/{WS}/statuses"),
            json!({"data": [{"id": 1, "name": "Todo", "type": "default"}, {"id": 4, "name": "Review", "type": "review"}, {"id": 5, "name": "Needs feedback", "type": "active"}]}),
        )
        .await;
        get(
            "/api/project_categories",
            json!({"data": [{"id": CATEGORY, "title": "Tmgr", "code": "TM"}]}),
        )
        .await;
        let mut listed = vec![open_task()];
        if self.second {
            listed.push(open_task_n(
                TASK2,
                "Update docs",
                "The install guide is outdated",
            ));
        }
        get(
            "/api/tasks",
            json!({"data": listed, "meta": {"current_page": 1, "last_page": 1}}),
        )
        .await;
        get(&format!("/api/tasks/{TASK}"), open_task()).await;
        get(
            &format!("/api/tasks/{TASK}/agent-approval"),
            json!({"data": {"repositories": null, "persona": persona_json(), "context": {"pages": context_pages()}}}),
        )
        .await;
        get(
            &format!("/api/tasks/{TASK}/agent-work"),
            json!({"data": {"runs": []}}),
        )
        .await;
        Mock::given(method("GET"))
            .and(path("/api/agent-runs/keys"))
            .respond_with(Keys(self.remote.clone(), other_key.clone()))
            .mount(server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/agent-runs/keys"))
            .respond_with(Register(self.remote.clone()))
            .mount(server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/agent-runs/approvals/bulk"))
            .respond_with(Bulk(self.remote.clone()))
            .mount(server)
            .await;
        let tasks: &[i64] = if self.second { &[TASK, TASK2] } else { &[TASK] };
        for &id in tasks {
            Mock::given(method("GET"))
                .and(path(format!("/api/agent-runs/tasks/{id}")))
                .respond_with(Eligibility(self.remote.clone(), id))
                .mount(server)
                .await;
            Mock::given(method("POST"))
                .and(path(format!("/api/tasks/{id}/comments")))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {}})))
                .mount(server)
                .await;
        }
        if self.second {
            get(
                &format!("/api/tasks/{TASK2}"),
                open_task_n(TASK2, "Update docs", "The install guide is outdated"),
            )
            .await;
            get(
                &format!("/api/tasks/{TASK2}/agent-approval"),
                json!({"data": {"repositories": null, "persona": persona_json(), "context": {"pages": context_pages()}}}),
            )
            .await;
            get(
                &format!("/api/tasks/{TASK2}/agent-work"),
                json!({"data": {"runs": []}}),
            )
            .await;
            Mock::given(method("PUT"))
                .and(path(format!("/api/tasks/{TASK2}/5")))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {}})))
                .mount(server)
                .await;
        }
    }

    fn root(&self) -> PathBuf {
        self.env.tmp.path().to_path_buf()
    }

    fn envs(&self) -> Vec<(String, String)> {
        let t = self.root();
        [
            ("HOME", t.clone()),
            ("XDG_CONFIG_HOME", t.join("xdg")),
            ("XDG_STATE_HOME", t.join(".local/state")),
            ("TMGR_CREDENTIAL_STORE", "file".into()),
            ("TMGR_SERVER", self.env.server.uri().into()),
            ("TMGR_RUNNER_DIR", self.runner.path().join("r")),
            ("TMGR_RUNNER_HOMES", self.homes.path().to_path_buf()),
            ("TMGR_T0_QUIET_MS", "300".into()),
            ("TMGR_TEST_KEYCHAIN_FILE", t.join("keychain.json")),
            ("TMGR_TEST_AUTOPILOT_USAGE", t.join("usage.json")),
            ("TMGR_TEST_AUTOPILOT_TICK_MS", "200".into()),
            ("TMGR_TEST_AUTOPILOT_PROBE_MS", "200".into()),
            ("NO_COLOR", "1".into()),
            ("PATH", common::path_with(Some(self.stub.path())).into()),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v.to_string_lossy().into_owned()))
        .collect()
    }

    fn cmd(&self, args: &[&str]) -> Output {
        let mut c = assert_cmd::Command::new(BIN);
        c.current_dir(self.root()).env_clear();
        for (k, v) in self.envs() {
            c.env(k, v);
        }
        c.args(args).output().unwrap()
    }

    fn pty(&self, args: &[&str]) -> Pty {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 200,
                cols: 160,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut cmd = CommandBuilder::new(BIN);
        cmd.args(args);
        cmd.env_clear();
        cmd.cwd(self.root());
        for (k, v) in self.envs() {
            cmd.env(k, v);
        }
        let child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let screen = Arc::new(Mutex::new(vt100::Parser::new(200, 160, 0)));
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

    fn add_rule(&self) {
        let repo = self.repo.to_str().unwrap();
        let mut p = self.pty(&[
            "queue",
            "rules",
            "add",
            "r1",
            "--workspace",
            "tmgrdev",
            "--persona",
            "builder",
            "--profile",
            "claude-code",
            "--repo",
            repo,
            "--project",
            "TM",
            "--auto",
        ]);
        p.wait_for("Type I AGREE");
        p.line("I AGREE");
        assert_eq!(p.exit(), 0, "{}", p.text());
    }

    fn start_pty(&self, extra: &[&str]) -> Pty {
        let mut args = vec![
            "queue",
            "autopilot",
            "--rule",
            "r1",
            "--tasks",
            "all",
            "--max-teams",
            "1",
            "--profile",
            "claude-code",
            "--team-size",
            "1",
            "--team-mode",
            "eco",
            "--usage-soft",
            "50",
            "--usage-hard",
            "80",
        ];
        args.extend_from_slice(extra);
        let mut p = self.pty(&args);
        for label in [
            "Continue after the usage window resets",
            "Most to spend in USD",
            "Most hours to run",
            "Most tasks to start",
        ] {
            p.wait_for(label);
            p.line("");
        }
        p.wait_for_paged("Type I AGREE");
        p.line("I AGREE");
        p
    }

    fn wait_for_agent_file(&self) {
        let queue = self.root().join("code/repo-worktrees/queue");
        let end = Instant::now() + Duration::from_secs(30);
        loop {
            let found = std::fs::read_dir(&queue)
                .into_iter()
                .flatten()
                .flatten()
                .any(|d| d.path().join("agent-out.txt").exists());
            if found {
                return;
            }
            assert!(Instant::now() < end, "the agent never wrote its file");
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn cfg(&self) -> PathBuf {
        self.env.config_dir()
    }

    fn autopilot_status(&self) -> Option<Value> {
        let t = std::fs::read_to_string(self.cfg().join("autopilot/status.json")).ok()?;
        serde_json::from_str(&t).ok()
    }

    fn wait_status(&self, what: &str, ok: impl Fn(&Value) -> bool) -> Value {
        let end = Instant::now() + Duration::from_secs(60);
        loop {
            if let Some(s) = self.autopilot_status() {
                if ok(&s) {
                    return s;
                }
            }
            assert!(
                Instant::now() < end,
                "{what}: {:?}\nrunner log:\n{}",
                self.autopilot_status(),
                std::fs::read_to_string(self.runner.path().join("r/runner.log"))
                    .unwrap_or_default(),
            );
            thread::sleep(Duration::from_millis(100));
        }
    }

    fn queue_enabled(&self) -> bool {
        std::fs::read_to_string(self.cfg().join("queue.toml"))
            .unwrap()
            .contains("enabled = true")
    }

    fn audit(&self) -> Vec<Value> {
        std::fs::read_to_string(self.cfg().join("queue-audit.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect()
    }

    fn gh_argv(&self) -> Vec<String> {
        std::fs::read_to_string(self.root().join("gh-argv.txt"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    async fn calls(&self) -> Vec<(String, String, String)> {
        self.env
            .server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r.method.to_string(),
                    r.url.path().to_string(),
                    String::from_utf8_lossy(&r.body).to_string(),
                )
            })
            .collect()
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn starting_signs_pins_enables_and_hosts_the_loop_until_stop() {
    if !sandbox_usable() {
        return;
    }
    let cloud = Cloud::new().await;
    cloud.add_rule();
    let mut p = cloud.start_pty(&[]);
    let code = p.exit();
    assert_eq!(code, 0, "{}", p.text());
    let screen = p.text();
    assert!(screen.contains("to be signed now"), "{screen}");

    let calls = cloud.calls().await;
    let uploaded = calls
        .iter()
        .filter(|c| c.1 == "/api/agent-runs/approvals/bulk")
        .count();
    assert_eq!(uploaded, 1, "{calls:?}");
    let item = cloud.remote.lock().unwrap().bulk[0].clone();
    let public_key = cloud.remote.lock().unwrap().public_key.clone().unwrap();
    let case = v3_case();
    let persona_hash = tmgr_core::approval::persona_sha256(
        case["persona"]["id"].as_str().unwrap(),
        case["persona"]["name"].as_str().unwrap(),
        case["persona"]["instructions"].as_str().unwrap(),
    )
    .unwrap();
    let page = &case["context"][0];
    let context_hash = tmgr_core::approval::context_sha256(&[tmgr_core::approval::page_sha256(
        page["id"].as_i64().unwrap(),
        page["title"].as_str().unwrap(),
        page["body"].as_str().unwrap(),
    )])
    .unwrap();
    assert_eq!(item["persona_sha256"], persona_hash.as_str());
    assert_eq!(item["context_sha256"], context_hash.as_str());
    assert!(item.get("repos_sha256").is_none());
    assert_eq!(item["key_id"], OWN_KEY_ID);
    let message = tmgr_core::approval::approval_message(&tmgr_core::approval::Payload {
        host: cloud.env.server.address().to_string(),
        user_id: 1,
        workspace_id: WS,
        task_id: TASK,
        category_id: CATEGORY,
        content_sha256: item["content_sha256"].as_str().unwrap().to_string(),
        repos_sha256: None,
        persona_sha256: Some(persona_hash),
        context_sha256: Some(context_hash),
        key_id: OWN_KEY_ID.to_string(),
        signed_at: item["signed_at"].as_i64().unwrap(),
    });
    assert!(message.starts_with("tmgr-agent-approval:v3:"), "{message}");
    assert!(tmgr_core::approval::verify(
        &public_key,
        &message,
        item["signature"].as_str().unwrap()
    ));
    assert!(
        screen.contains("The login form drops the session"),
        "{screen}"
    );
    assert!(screen.contains("no repository list signed"), "{screen}");
    assert!(screen.contains("Follow the rules."), "{screen}");
    assert!(screen.contains("Rules for parallel teams"), "{screen}");

    let allow: Value = serde_json::from_str(
        &std::fs::read_to_string(cloud.cfg().join("runner-allow.json")).unwrap(),
    )
    .unwrap();
    let keys = allow["entries"][0]["keys"].as_array().unwrap();
    assert!(
        keys.iter()
            .any(|k| k["key_id"] == OWN_KEY_ID && k["public_key"] == public_key.as_str()),
        "{allow}"
    );

    assert!(cloud.queue_enabled());
    assert!(
        cloud.audit().iter().any(|e| e["event"] == "enable"),
        "{:?}",
        cloud.audit()
    );

    let status = cloud.wait_status("a running task", |s| s["tasks"]["in_work"] == 1);
    let out = cloud.cmd(&["--json", "queue", "autopilot", "status"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let v: Value = stdout_json(&out);
    assert_eq!(v["data"]["running"], true, "{v}");
    assert_valid("queue-autopilot-status", &v);
    assert_eq!(status["tasks"]["total"], 1);

    let stop = cloud.cmd(&["queue", "autopilot", "stop"]);
    assert_eq!(stop.status.code(), Some(0), "{}", text(&stop.stderr));
    let done = cloud.wait_status("stopped", |s| s["state"] == "stopped");
    assert_eq!(done["stop_reason"], "stopped by you");
    assert!(done["summary"].is_object(), "{done}");
    assert!(!cloud.queue_enabled());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dry_run_signs_but_never_turns_the_queue_on() {
    if !sandbox_usable() {
        return;
    }
    let cloud = Cloud::new().await;
    cloud.add_rule();
    let mut p = cloud.start_pty(&["--dry-run"]);
    let code = p.exit();
    assert_eq!(code, 0, "{}", p.text());
    let screen = p.text();
    assert!(
        screen.contains("Dry run: the orchestrator plans and triages only."),
        "{screen}"
    );
    assert!(screen.contains("Dry run started"), "{screen}");
    assert!(!screen.contains("turns the queue ON"), "{screen}");

    let uploaded = cloud
        .calls()
        .await
        .iter()
        .filter(|c| c.1 == "/api/agent-runs/approvals/bulk")
        .count();
    assert_eq!(uploaded, 1);
    assert!(!cloud.queue_enabled());
    assert!(
        cloud.audit().iter().all(|e| e["event"] != "enable"),
        "{:?}",
        cloud.audit()
    );
    let status = cloud.wait_status("a status file", |s| s["settings_id"].is_string());
    let settings: Value = serde_json::from_str(
        &std::fs::read_to_string(cloud.cfg().join(format!(
            "autopilot/settings/{}.json",
            status["settings_id"].as_str().unwrap()
        )))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(settings["dry_run"], true, "{settings}");
    let answers = std::fs::read_to_string(cloud.cfg().join("autopilot-answers.json")).unwrap();
    assert!(!answers.contains("dry"), "{answers}");
    let stop = cloud.cmd(&["queue", "autopilot", "stop"]);
    assert_eq!(stop.status.code(), Some(0), "{}", text(&stop.stderr));
    assert!(!cloud.queue_enabled());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_usage_jump_over_the_hard_limit_kills_the_agent_and_pauses_the_task_as_a_draft() {
    if !sandbox_usable() {
        return;
    }
    if !cfg!(target_os = "macos") {
        eprintln!("skipped: the Linux sandbox cannot read the stub agent in a temp directory");
        return;
    }
    let cloud = Cloud::new().await;
    cloud.add_rule();
    let mut p = cloud.start_pty(&[]);
    assert_eq!(p.exit(), 0, "{}", p.text());
    cloud.wait_status("a running task", |s| s["tasks"]["in_work"] == 1);
    cloud.wait_for_agent_file();

    std::fs::write(
        cloud.root().join("usage.json"),
        json!({"session": 90.0, "weekly": 5.0, "billing": "subscription"}).to_string(),
    )
    .unwrap();
    let done = cloud.wait_status("stopped", |s| s["state"] == "stopped");
    assert_eq!(
        done["stop_reason"], "hard usage limit reached: five_hour at 90%",
        "{done}"
    );
    assert_eq!(
        done["summary"]["stopped_by"],
        "hard usage limit reached: five_hour at 90%"
    );
    let events = done["events"].to_string();
    assert!(
        events.contains("Hard usage limit: five_hour at 90%"),
        "{events}"
    );
    let paused = done["summary"]["paused"].as_array().unwrap();
    assert_eq!(paused.len(), 1, "{done}");
    assert!(
        paused[0]["reason"]
            .as_str()
            .unwrap()
            .starts_with("usage limit reached at 90%"),
        "{done}"
    );
    assert!(!cloud.queue_enabled());

    let mut runner = tmgr_runner::client::Client::connect(&cloud.runner.path().join("r")).unwrap();
    assert!(runner
        .list()
        .unwrap()
        .iter()
        .all(|s| s.state.kind == tmgr_ipc::api::StateKind::Exited));

    let argv = cloud.gh_argv();
    assert_eq!(&argv[..2], ["pr", "create"], "{argv:?}");
    assert!(argv.iter().any(|a| a == "--draft"), "{argv:?}");
    let branches = git(&cloud.bare, &["branch", "--list"]);
    assert!(branches.contains("tmgr/tm-410"), "{branches}");
    let branch = branches.trim().trim_start_matches("* ").to_string();
    let readme = git(&cloud.bare, &["show", &format!("{branch}:README.md")]);
    assert!(readme.contains("working"), "{readme}");
    let subject = git(&cloud.bare, &["log", "-1", "--format=%s", &branch]);
    assert!(subject.starts_with("WIP: paused"), "{subject}");

    let calls = cloud.calls().await;
    let comments: Vec<&str> = calls
        .iter()
        .filter(|c| c.1 == format!("/api/tasks/{TASK}/comments"))
        .map(|c| c.2.as_str())
        .collect();
    assert!(
        comments
            .iter()
            .any(|c| c.contains("Paused: usage limit reached at 90%")),
        "{comments:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn starting_a_drain_suspends_the_queue_loop_first_and_leaves_the_queue_on() {
    if !sandbox_usable() {
        return;
    }
    let cloud = Cloud::new().await;
    cloud.add_rule();
    let mut enable = cloud.pty(&["queue", "enable"]);
    enable.wait_for("Type I AGREE");
    enable.line("I AGREE");
    assert_eq!(enable.exit(), 0, "{}", enable.text());
    let queue = cloud.wait_status("the queue loop", |s| {
        s["settings_id"] == "queue" && s["state"] == "running"
    });
    assert_eq!(queue["kind"], "queue");

    let mut p = cloud.start_pty(&[]);
    assert_eq!(p.exit(), 0, "{}", p.text());
    let drain = cloud.wait_status("the drain", |s| {
        s["settings_id"]
            .as_str()
            .is_some_and(|id| id.starts_with("ap-"))
            && (s["tasks"]["in_work"] == 1 || s["state"] == "stopped")
    });
    let prev: Value = serde_json::from_str(
        &std::fs::read_to_string(cloud.cfg().join("autopilot/status.prev.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(prev["settings_id"], "queue");
    assert_eq!(prev["state"], "suspended");
    assert!(cloud.queue_enabled());
    assert!(
        !cloud.cfg().join("autopilot/settings/queue.json").exists(),
        "the uncapped loop settings are gone while the drain runs"
    );

    if drain["state"] == "running" {
        let stop = cloud.cmd(&["queue", "autopilot", "stop"]);
        assert_eq!(stop.status.code(), Some(0), "{}", text(&stop.stderr));
    }
    let back = cloud.wait_status("the queue loop is back", |s| {
        s["settings_id"] == "queue" && s["state"] == "running"
    });
    assert_eq!(back["kind"], "queue");
    assert!(cloud.queue_enabled());
    assert!(cloud.cfg().join("autopilot/settings/queue.json").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_orchestrator_triages_one_task_and_the_other_one_starts() {
    if !sandbox_usable() {
        return;
    }
    if !cfg!(target_os = "macos") {
        eprintln!("skipped: the Linux sandbox cannot read the stub agent in a temp directory");
        return;
    }
    let cloud = Cloud::build(true).await;
    cloud.add_rule();
    let mut p = cloud.start_pty(&[]);
    assert_eq!(p.exit(), 0, "{}", p.text());
    let status = cloud.wait_status("a running planned task", |s| s["tasks"]["in_work"] == 1);
    cloud.wait_for_agent_file();
    assert_eq!(status["plan"]["state"], "done", "{status}");
    assert_eq!(status["triage"][0]["task"], "TM-411", "{status}");
    assert_eq!(status["triage"][0]["applied"], true, "{status}");
    assert_eq!(status["runs"][0]["task"], "TM-410", "{status}");

    let calls = cloud.calls().await;
    let on = |m: &str, path: String| -> Vec<&(String, String, String)> {
        calls.iter().filter(|c| c.0 == m && c.1 == path).collect()
    };
    let comments = on("POST", format!("/api/tasks/{TASK2}/comments"));
    assert_eq!(comments.len(), 1, "{calls:?}");
    assert!(
        comments[0].2.contains("needs feedback"),
        "{}",
        comments[0].2
    );
    assert!(comments[0].2.contains("Which guide should change"));
    assert_eq!(on("PUT", format!("/api/tasks/{TASK2}/5")).len(), 1);
    assert!(on("PUT", format!("/api/tasks/{TASK}/5")).is_empty());
    assert!(
        on("POST", format!("/api/tasks/{TASK}/comments")).is_empty(),
        "{calls:?}"
    );

    let stop = cloud.cmd(&["queue", "autopilot", "stop"]);
    assert_eq!(stop.status.code(), Some(0), "{}", text(&stop.stderr));
    cloud.wait_status("stopped", |s| s["state"] == "stopped");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_dry_run_plans_without_comments_status_changes_or_task_runs() {
    if !sandbox_usable() {
        return;
    }
    if !cfg!(target_os = "macos") {
        eprintln!("skipped: the Linux sandbox cannot read the stub agent in a temp directory");
        return;
    }
    let cloud = Cloud::build(true).await;
    cloud.add_rule();
    let mut p = cloud.start_pty(&["--dry-run"]);
    assert_eq!(p.exit(), 0, "{}", p.text());
    let done = cloud.wait_status("a finished dry run", |s| s["state"] == "stopped");
    assert_eq!(done["stop_reason"], "dry run finished", "{done}");
    assert_eq!(done["plan"]["state"], "done");
    assert_eq!(done["plan"]["dry_run"], true);
    assert_eq!(done["triage"][0]["applied"], false, "{done}");
    assert!(!cloud.queue_enabled());

    let calls = cloud.calls().await;
    assert!(
        calls
            .iter()
            .all(|c| c.0 != "PUT" && !c.1.ends_with("/comments")),
        "{calls:?}"
    );
    assert!(cloud.gh_argv().is_empty());
    let mut runner = tmgr_runner::client::Client::connect(&cloud.runner.path().join("r")).unwrap();
    let sessions = runner.list().unwrap();
    assert!(
        sessions
            .iter()
            .all(|s| s.state.kind == tmgr_ipc::api::StateKind::Exited),
        "{sessions:?}"
    );
    let agent_files = std::fs::read_dir(cloud.root().join("code/repo-worktrees/queue"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|d| d.path().join("agent-out.txt").exists())
        .count();
    assert_eq!(agent_files, 0);
}
