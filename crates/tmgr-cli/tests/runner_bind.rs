#![cfg(feature = "runner")]

mod common;

use common::{git, init_repo, stdout_json, text, Env, ACCESS};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

const WS: i64 = 56;
const CAT: i64 = 127;
const TASK: i64 = 9001;

struct Fx {
    env: Env,
}

struct Pty {
    screen: Arc<Mutex<String>>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    _master: Box<dyn portable_pty::MasterPty + Send>,
}

impl Drop for Pty {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

impl Pty {
    fn text(&self) -> String {
        self.screen.lock().unwrap().clone()
    }

    fn wait_for(&self, needle: &str) {
        let end = Instant::now() + Duration::from_secs(20);
        while !self.text().contains(needle) {
            assert!(
                Instant::now() < end,
                "waiting for {needle:?}: {}",
                self.text()
            );
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn line(&mut self, s: &str) {
        self.writer.write_all(format!("{s}\r").as_bytes()).unwrap();
        self.writer.flush().unwrap();
    }

    fn exit(&mut self) -> u32 {
        let end = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(s) = self.child.try_wait().unwrap() {
                return s.exit_code();
            }
            assert!(Instant::now() < end, "did not exit: {}", self.text());
            thread::sleep(Duration::from_millis(50));
        }
    }
}

fn task_body(set: Option<&[&str]>) -> Value {
    let mut category = json!({"id": CAT, "code": "TM", "github_full_name": "acme/widgets", "default_branch": "main"});
    if let Some(list) = set {
        category["repositories"] = json!(list);
    }
    json!({"data": {
        "task_id": TASK, "workspace_id": WS, "workspace_code": "tmgrdev", "viewer_user_id": 1,
        "title": "Fix login", "task_key": "TM-1", "category": category, "setting_enabled": true
    }})
}

impl Fx {
    async fn new(set: Option<&[&str]>) -> Fx {
        let env = Env::new().await;
        env.login_seed();
        env.mock_json(
            "/api/auth/devices/current",
            json!({"data": {"device_id": 9, "user": {"id": 1, "name": "Ann"}, "scopes": ["tasks:read"]}}),
        )
        .await;
        env.mock_json(
            "/api/workspaces",
            json!({"data": [{"id": WS, "name": "Tmgr Dev", "code": "tmgrdev", "is_default": true}]}),
        )
        .await;
        Mock::given(method("GET"))
            .and(path("/api/project_categories"))
            .and(header("authorization", format!("Bearer {ACCESS}").as_str()))
            .and(query_param("workspace_id", WS.to_string().as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"data": [{"id": CAT, "title": "Tmgr", "code": "TM"}, {"id": 128, "title": "Empty", "code": "EM"}]}),
            ))
            .mount(&env.server)
            .await;
        let mut task = common::task_json(TASK as u64, "Fix login");
        task["project_category_id"] = json!(CAT);
        task["workspace_id"] = json!(WS);
        Mock::given(method("GET"))
            .and(path("/api/tasks"))
            .and(query_param("project_category_id", CAT.to_string().as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"data": [task.clone()], "meta": {"current_page": 1, "last_page": 1}}),
            ))
            .mount(&env.server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/tasks"))
            .and(query_param("project_category_id", "128"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!({"data": [], "meta": {"current_page": 1, "last_page": 1}}),
                ),
            )
            .mount(&env.server)
            .await;
        env.mock_json("/api/tasks/by-key/TM-1", json!({"data": task}))
            .await;
        env.mock_json(&format!("/api/agent-runs/tasks/{TASK}"), task_body(set))
            .await;
        let fx = Fx { env };
        fx.write_allow(json!({}), None);
        fx
    }

    fn allow_path(&self) -> PathBuf {
        self.env.config_dir().join("runner-allow.json")
    }

    fn write_allow(&self, repos: Value, expires: Option<&str>) {
        std::fs::create_dir_all(self.env.config_dir()).unwrap();
        let allow = json!({"version": 1, "entries": [{
            "server": self.env.server.address().to_string(), "user_id": 1, "workspace_id": WS,
            "workspace_code": "tmgrdev", "allowed_at": "2025-10-01T00:00:00Z", "expires_at": expires,
            "keys": [], "repos": repos}]});
        std::fs::write(self.allow_path(), allow.to_string()).unwrap();
    }

    fn repos(&self) -> Value {
        let v: Value =
            serde_json::from_str(&std::fs::read_to_string(self.allow_path()).unwrap()).unwrap();
        v["entries"][0]["repos"].clone()
    }

    fn clone_of(&self, dir: &str, origin: &str) -> PathBuf {
        let p = init_repo(&self.env.tmp.path().join(dir));
        git(&p, &["remote", "add", "origin", origin]);
        p
    }

    fn cmd(&self, args: &[&str]) -> std::process::Output {
        self.env.std_cmd().args(args).output().unwrap()
    }

    fn pty(&self, args: &[&str], extra: &[(&str, &str)]) -> Pty {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 40,
                cols: 200,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_tmgr"));
        cmd.args(args);
        cmd.env_clear();
        cmd.cwd(self.env.tmp.path());
        cmd.env("HOME", self.env.tmp.path());
        cmd.env("XDG_CONFIG_HOME", self.env.xdg());
        cmd.env("TMGR_CREDENTIAL_STORE", "file");
        cmd.env("TMGR_SERVER", self.env.server.uri());
        cmd.env("NO_COLOR", "1");
        cmd.env("TERM", "xterm");
        cmd.env("PATH", "/usr/bin:/bin");
        for (k, v) in extra {
            cmd.env(k, v);
        }
        let child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let screen = Arc::new(Mutex::new(String::new()));
        let sink = screen.clone();
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                sink.lock()
                    .unwrap()
                    .push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        });
        Pty {
            screen,
            writer: pair.master.take_writer().unwrap(),
            child,
            _master: pair.master,
        }
    }

    fn bind_with(&self, target: &str, path: &str, answer: &str) -> (u32, String) {
        let mut p = self.pty(&["runner", "bind", target, path], &[]);
        p.wait_for("Type the category code");
        p.line(answer);
        let code = p.exit();
        (code, p.text())
    }
}

fn s(p: &Path) -> String {
    p.display().to_string()
}

const WIDGETS: &str = "https://github.com/acme/widgets.git";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bind_stores_the_canonical_clone_after_the_category_code_is_typed() {
    let fx = Fx::new(None).await;
    let repo = fx.clone_of("widgets", WIDGETS);
    let (code, screen) = fx.bind_with("tmgrdev:TM", &s(&repo), "TM");
    assert_eq!(code, 0, "{screen}");
    assert_eq!(fx.repos(), json!({ CAT.to_string(): s(&repo) }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bind_through_a_task_key_and_a_home_relative_path() {
    let fx = Fx::new(None).await;
    let repo = fx.clone_of("widgets", WIDGETS);
    let (code, screen) = fx.bind_with("tmgrdev:TM-1", "~/widgets", "TM");
    assert_eq!(code, 0, "{screen}");
    assert_eq!(fx.repos(), json!({ CAT.to_string(): s(&repo) }));
    let (code, screen) = fx.bind_with("tmgrdev:TM", "$HOME/widgets", "TM");
    assert_eq!(code, 0, "{screen}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wrong_confirmation_changes_nothing() {
    let fx = Fx::new(None).await;
    let repo = fx.clone_of("widgets", WIDGETS);
    let (code, _) = fx.bind_with("tmgrdev:TM", &s(&repo), "yes");
    assert_ne!(code, 0);
    assert_eq!(fx.repos(), json!({}));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_origin_of_another_repository_or_a_worktree_is_refused() {
    let fx = Fx::new(None).await;
    let other = fx.clone_of("other", "https://github.com/acme/other.git");
    let mut p = fx.pty(&["runner", "bind", "tmgrdev:TM", &s(&other)], &[]);
    assert_ne!(p.exit(), 0);
    assert!(
        p.text().contains("does not fetch from and push to"),
        "{}",
        p.text()
    );
    assert!(!p.text().contains("Type the category code"));
    let repo = fx.clone_of("widgets", WIDGETS);
    let tree = fx.env.tmp.path().join("tree");
    git(&repo, &["worktree", "add", "-q", &s(&tree), "-b", "wt"]);
    let mut p = fx.pty(&["runner", "bind", "tmgrdev:TM", &s(&tree)], &[]);
    assert_ne!(p.exit(), 0);
    assert!(p.text().contains("worktree"), "{}", p.text());
    assert_eq!(fx.repos(), json!({}));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bind_and_unbind_are_refused_without_a_terminal_and_inside_an_agent_session() {
    let fx = Fx::new(None).await;
    let repo = fx.clone_of("widgets", WIDGETS);
    fx.write_allow(json!({ CAT.to_string(): s(&repo) }), None);
    for args in [
        vec!["runner", "bind", "tmgrdev:TM", "/x"],
        vec!["runner", "unbind", "tmgrdev:TM", "--yes"],
    ] {
        let out = fx.cmd(&args);
        assert_eq!(out.status.code(), Some(10), "{args:?}");
        let out = fx
            .env
            .std_cmd()
            .env("TMGR_RUN_TOKEN", "x")
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(5), "{args:?}");
        let mut p = fx.pty(&args, &[("TMGR_RUN_ID", "r1")]);
        assert_eq!(p.exit(), 5, "{}", p.text());
    }
    assert_eq!(fx.repos(), json!({ CAT.to_string(): s(&repo) }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_expired_permission_points_to_runner_allow() {
    let fx = Fx::new(None).await;
    let repo = fx.clone_of("widgets", WIDGETS);
    fx.write_allow(json!({}), Some("2025-10-02T00:00:00Z"));
    let mut p = fx.pty(&["runner", "bind", "tmgrdev:TM", &s(&repo)], &[]);
    assert_ne!(p.exit(), 0);
    assert!(
        p.text().contains("tmgr runner allow tmgrdev"),
        "{}",
        p.text()
    );
    assert_eq!(fx.repos(), json!({}));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_category_without_tasks_asks_for_a_task_ref() {
    let fx = Fx::new(None).await;
    let repo = fx.clone_of("widgets", WIDGETS);
    let mut p = fx.pty(&["runner", "bind", "tmgrdev:EM", &s(&repo)], &[]);
    assert_ne!(p.exit(), 0);
    assert!(
        p.text().contains("pass a task of the category"),
        "{}",
        p.text()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_set_binds_the_named_clone_and_asks_for_the_rest() {
    let fx = Fx::new(Some(&["acme/widgets", "acme/gadgets"])).await;
    let widgets = fx.clone_of("widgets", WIDGETS);
    let gadgets = fx.clone_of("gadgets", "git@github.com:acme/gadgets.git");
    let mut p = fx.pty(&["runner", "bind", "tmgrdev:TM", &s(&widgets)], &[]);
    p.wait_for("Path of your clone of github.com/acme/gadgets");
    p.line("~/gadgets");
    p.wait_for("Type the category code");
    p.line("TM");
    assert_eq!(p.exit(), 0, "{}", p.text());
    assert_eq!(
        fx.repos(),
        json!({ CAT.to_string(): {"acme/widgets": s(&widgets), "acme/gadgets": s(&gadgets)} })
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_repository_of_a_bound_set_can_be_moved_by_name() {
    let fx = Fx::new(Some(&["acme/widgets", "acme/gadgets"])).await;
    let old = fx.clone_of("old", WIDGETS);
    let gadgets = fx.clone_of("gadgets", "git@github.com:acme/gadgets.git");
    let new = fx.clone_of("widgets", WIDGETS);
    fx.write_allow(
        json!({ CAT.to_string(): {"acme/widgets": s(&old), "acme/gadgets": s(&gadgets)} }),
        None,
    );
    let (code, screen) = fx.bind_with("acme/widgets", &s(&new), "TM");
    assert_eq!(code, 0, "{screen}");
    assert_eq!(
        fx.repos(),
        json!({ CAT.to_string(): {"acme/widgets": s(&new), "acme/gadgets": s(&gadgets)} })
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn replacing_a_set_with_one_clone_asks_first() {
    let fx = Fx::new(None).await;
    let widgets = fx.clone_of("widgets", WIDGETS);
    let gadgets = fx.clone_of("gadgets", "git@github.com:acme/gadgets.git");
    let set =
        json!({ CAT.to_string(): {"acme/widgets": s(&widgets), "acme/gadgets": s(&gadgets)} });
    fx.write_allow(set.clone(), None);
    let mut p = fx.pty(&["runner", "bind", "tmgrdev:TM", &s(&widgets)], &[]);
    p.wait_for("Replace it?");
    p.line("n");
    assert_ne!(p.exit(), 0);
    assert_eq!(fx.repos(), set);
    let mut p = fx.pty(&["runner", "bind", "tmgrdev:TM", &s(&widgets)], &[]);
    p.wait_for("Replace it?");
    p.line("y");
    p.wait_for("Type the category code");
    p.line("TM");
    assert_eq!(p.exit(), 0, "{}", p.text());
    assert_eq!(fx.repos(), json!({ CAT.to_string(): s(&widgets) }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unbind_asks_and_removes_only_that_category() {
    let fx = Fx::new(None).await;
    let repo = fx.clone_of("widgets", WIDGETS);
    let bound = json!({ CAT.to_string(): s(&repo), "999": "/other" });
    fx.write_allow(bound.clone(), None);
    let mut p = fx.pty(&["runner", "unbind", "tmgrdev:TM"], &[]);
    p.wait_for("[y/N]");
    p.line("n");
    assert_ne!(p.exit(), 0);
    assert_eq!(fx.repos(), bound);
    let mut p = fx.pty(&["runner", "unbind", "tmgrdev:TM"], &[]);
    p.wait_for("[y/N]");
    p.line("y");
    assert_eq!(p.exit(), 0, "{}", p.text());
    assert_eq!(fx.repos(), json!({ "999": "/other" }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_shows_every_binding_without_a_terminal_or_an_agent_guard() {
    let fx = Fx::new(None).await;
    fx.write_allow(
        json!({ CAT.to_string(): {"acme/widgets": "/w", "acme/gadgets": "/g"}, "5": "/single" }),
        None,
    );
    let out = fx
        .env
        .std_cmd()
        .env("TMGR_RUN_TOKEN", "x")
        .args(["--json", "runner", "list"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let rows = stdout_json(&out)["data"]["bindings"].clone();
    let rows = rows.as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().any(|r| r["workspace"] == "tmgrdev"
        && r["category_id"] == CAT
        && r["full_name"] == "acme/widgets"
        && r["path"] == "/w"));
    assert!(rows
        .iter()
        .any(|r| r["category_id"] == 5 && r["path"] == "/single" && r.get("full_name").is_none()));
    let human = fx.cmd(&["runner", "list"]);
    assert!(text(&human.stdout).contains("/single"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn home_markers_are_expanded_in_path_arguments() {
    let fx = Fx::new(None).await;
    let home = fx.env.tmp.path().to_path_buf();
    let out = fx.cmd(&["runner", "trust", "$HOME/nowhere"]);
    assert!(
        text(&out.stderr).contains(&format!("{}/nowhere is not an existing folder", s(&home))),
        "{}",
        text(&out.stderr)
    );
    let out = fx
        .env
        .std_cmd()
        .env("TMGR_RUNNER_DIR", "/dev/null/none")
        .args(["run", "--agent-cmd", "/usr/bin/true", "--cwd", "~/nowhere"])
        .output()
        .unwrap();
    assert!(
        text(&out.stderr).contains(&format!("in {}/nowhere", s(&home))),
        "{}",
        text(&out.stderr)
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rules_add_binds_an_unbound_clone_of_a_cloud_rule_after_the_typed_code() {
    let fx = Fx::new(None).await;
    let repo = fx.clone_of("widgets", WIDGETS);
    let agents = fx.env.tmp.path().join("agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(
        agents.join("sh-agent.toml"),
        "id = \"sh-agent\"\nname = \"Shell\"\ncommand = [\"/bin/sh\", \"-c\", \"true\", \"agent\"]\n[permission_modes]\nask = [\"--ask\"]\n",
    )
    .unwrap();
    let agents_dir = s(&agents);
    let args = [
        "queue",
        "rules",
        "add",
        "cloud",
        "--workspace",
        "tmgrdev",
        "--project",
        "TM",
        "--auto",
        "--persona",
        "builder",
        "--profile",
        "sh-agent",
        "--repo",
        "~/widgets",
    ];
    let extra = [("TMGR_AGENTS_DIR", agents_dir.as_str())];
    let mut p = fx.pty(&args, &extra);
    p.wait_for("Type the category code");
    p.line("TM");
    p.wait_for("I AGREE");
    assert_eq!(fx.repos(), json!({ CAT.to_string(): s(&repo) }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rules_add_refuses_to_bind_without_a_terminal_or_for_a_foreign_origin() {
    let fx = Fx::new(None).await;
    let repo = fx.clone_of("widgets", WIDGETS);
    let other = fx.clone_of("other", "https://github.com/acme/other.git");
    let base = [
        "queue",
        "rules",
        "add",
        "cloud",
        "--workspace",
        "tmgrdev",
        "--project",
        "TM",
        "--auto",
        "--persona",
        "builder",
        "--profile",
        "sh-agent",
    ];
    let with = |r: &Path| {
        let mut a: Vec<String> = base.iter().map(|x| x.to_string()).collect();
        a.extend(["--repo".to_string(), s(r)]);
        a
    };
    let args = with(&repo);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    assert_eq!(fx.cmd(&refs).status.code(), Some(10));
    let out = fx
        .env
        .std_cmd()
        .env("TMGR_RUN_ID", "r")
        .args(&refs)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    let args = with(&other);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut p = fx.pty(&refs, &[]);
    assert_ne!(p.exit(), 0);
    assert!(
        p.text().contains("does not fetch from and push to"),
        "{}",
        p.text()
    );
    assert_eq!(fx.repos(), json!({}));
}

fn agents_dir(fx: &Fx) -> String {
    let agents = fx.env.tmp.path().join("agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(
        agents.join("sh-agent.toml"),
        "id = \"sh-agent\"\nname = \"Shell\"\ncommand = [\"/bin/sh\", \"-c\", \"true\", \"agent\"]\n[permission_modes]\nask = [\"--ask\"]\n",
    )
    .unwrap();
    s(&agents)
}

fn rule_args(repos: &[&str]) -> Vec<String> {
    let mut a: Vec<String> = [
        "queue",
        "rules",
        "add",
        "cloud",
        "--workspace",
        "tmgrdev",
        "--project",
        "TM",
        "--auto",
        "--persona",
        "builder",
        "--profile",
        "sh-agent",
    ]
    .iter()
    .map(|x| x.to_string())
    .collect();
    for r in repos {
        a.extend(["--repo".to_string(), r.to_string()]);
    }
    a
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rules_add_binds_only_into_the_entry_of_this_server_and_account() {
    let fx = Fx::new(None).await;
    let repo = fx.clone_of("widgets", WIDGETS);
    let entry = |server: &str, user: i64| {
        json!({
            "server": server, "user_id": user, "workspace_id": WS,
            "workspace_code": "tmgrdev", "allowed_at": "2025-10-01T00:00:00Z", "expires_at": null,
            "keys": [], "repos": {}})
    };
    let mine = fx.env.server.address().to_string();
    std::fs::write(
        fx.allow_path(),
        json!({"version": 1, "entries": [entry(&mine, 2), entry("other.example:443", 1), entry(&mine, 1)]})
            .to_string(),
    )
    .unwrap();
    let dir = agents_dir(&fx);
    let args = rule_args(&[&s(&repo)]);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut p = fx.pty(&refs, &[("TMGR_AGENTS_DIR", dir.as_str())]);
    p.wait_for("Type the category code");
    p.line("TM");
    p.wait_for("I AGREE");
    let v: Value =
        serde_json::from_str(&std::fs::read_to_string(fx.allow_path()).unwrap()).unwrap();
    assert_eq!(v["entries"][0]["repos"], json!({}));
    assert_eq!(v["entries"][1]["repos"], json!({}));
    assert_eq!(
        v["entries"][2]["repos"],
        json!({ CAT.to_string(): s(&repo) })
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rules_add_refuses_several_unbound_repos_for_a_single_repository_category() {
    let fx = Fx::new(None).await;
    let one = fx.clone_of("widgets", WIDGETS);
    let two = fx.clone_of("widgets2", WIDGETS);
    let args = rule_args(&[&s(&one), &s(&two)]);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut p = fx.pty(&refs, &[]);
    assert_eq!(p.exit(), 2, "{}", p.text());
    assert!(p.text().contains("one repository"), "{}", p.text());
    assert_eq!(fx.repos(), json!({}));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rules_add_binds_every_repo_of_a_set_with_one_typed_confirmation() {
    let fx = Fx::new(Some(&["acme/widgets", "acme/gadgets"])).await;
    let widgets = fx.clone_of("widgets", WIDGETS);
    let gadgets = fx.clone_of("gadgets", "git@github.com:acme/gadgets.git");
    let dir = agents_dir(&fx);
    let args = rule_args(&[&s(&widgets), &s(&gadgets)]);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let mut p = fx.pty(&refs, &[("TMGR_AGENTS_DIR", dir.as_str())]);
    p.wait_for("Type the category code");
    p.line("TM");
    p.wait_for("I AGREE");
    assert_eq!(
        fx.repos(),
        json!({ CAT.to_string(): {"acme/widgets": s(&widgets), "acme/gadgets": s(&gadgets)} })
    );
    assert_eq!(
        p.text().matches("Type the category code").count(),
        1,
        "{}",
        p.text()
    );
    assert!(!p.text().contains("Path of your clone"), "{}", p.text());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bare_repository_found_in_several_bindings_asks_for_ws_code() {
    let fx = Fx::new(Some(&["acme/widgets"])).await;
    let new = fx.clone_of("widgets", WIDGETS);
    let host = fx.env.server.address().to_string();
    let entry = |ws: i64, code: &str| {
        json!({
            "server": host, "user_id": 1, "workspace_id": ws,
            "workspace_code": code, "allowed_at": "2025-10-01T00:00:00Z", "expires_at": null,
            "keys": [], "repos": { "127": {"acme/widgets": "/old"} }})
    };
    std::fs::write(
        fx.allow_path(),
        json!({"version": 1, "entries": [entry(WS, "tmgrdev"), entry(57, "other")]}).to_string(),
    )
    .unwrap();
    let mut p = fx.pty(&["runner", "bind", "acme/widgets", &s(&new)], &[]);
    assert_eq!(p.exit(), 2, "{}", p.text());
    assert!(p.text().contains("WS:CODE"), "{}", p.text());
}
