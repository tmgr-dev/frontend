#![cfg(feature = "runner")]

mod common;

use common::{Env, ACCESS};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

const KEY: &str =
    "BImU1e2SPEdD/HKLzjfuQlNuLgKE44/qEF5a+626Bj0KtbPSIGIwmRlfUMIyP75vzLHw7BUXRArO87Ad8xnmazE=";
const OTHER: &str =
    "BD7GudbwbOUVIvsBsw0CvYo7MXeWK1HUMFt+qGPDXCqeED93aEkCI5814pbvM3gcEwEBre/NlqieFT46CB9RzdA=";
const KEY_ID: &str = "0b6f1c2e-6a7d-4e8f-9a10-1b2c3d4e5f60";
const OTHER_ID: &str = "7c1d0e2f-1111-4222-8333-444455556666";

async fn mount(env: &Env, p: &str, body: Value) {
    Mock::given(method("GET"))
        .and(path(p))
        .and(header("authorization", format!("Bearer {ACCESS}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&env.server)
        .await;
}

async fn setup(keys: Vec<Value>) -> Env {
    let env = Env::new().await;
    env.login_seed();
    mount(
        &env,
        "/api/auth/devices/current",
        json!({"data": {"device_id": 9, "user": {"id": 1, "name": "Ann"}, "scopes": ["tasks:read"]}}),
    )
    .await;
    mount(
        &env,
        "/api/workspaces",
        json!({"data": [{"id": 56, "name": "Tmgr Dev", "code": "tmgrdev", "is_default": true}]}),
    )
    .await;
    mount(&env, "/api/agent-runs/keys", json!({ "data": keys })).await;
    env
}

fn key(id: &str, public: &str, device: &str) -> Value {
    json!({"key_id": id, "public_key": public, "device_name": device, "created_at": "2025-10-01T00:00:00Z", "fingerprint": "ignored"})
}

fn allow_file(env: &Env) -> std::path::PathBuf {
    env.config_dir().join("runner-allow.json")
}

fn read_allow(env: &Env) -> Value {
    serde_json::from_str(&std::fs::read_to_string(allow_file(env)).unwrap()).unwrap()
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
    fn spawn(env: &Env, args: &[&str]) -> Pty {
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
        cmd.env("HOME", env.tmp.path());
        cmd.env("XDG_CONFIG_HOME", env.xdg());
        cmd.env("TMGR_CREDENTIAL_STORE", "file");
        cmd.env("TMGR_SERVER", env.server.uri());
        cmd.env("NO_COLOR", "1");
        cmd.env("TERM", "xterm");
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

    fn type_line(&mut self, line: &str) {
        self.writer
            .write_all(format!("{line}\r").as_bytes())
            .unwrap();
        self.writer.flush().unwrap();
    }

    fn exit_code(&mut self) -> u32 {
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn agent_sessions_are_refused_with_exit_5() {
    let env = setup(vec![key(KEY_ID, KEY, "laptop")]).await;
    let out = env
        .cmd()
        .env("TMGR_RUN_TOKEN", "x")
        .args(["runner", "allow", "tmgrdev"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    assert!(!allow_file(&env).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_terminal_is_refused_with_exit_10_and_yes_does_not_help() {
    let env = setup(vec![key(KEY_ID, KEY, "laptop")]).await;
    let out = env
        .cmd()
        .args(["runner", "allow", "tmgrdev", "--yes"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10));
    assert!(!allow_file(&env).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn typed_workspace_code_pins_the_keys_with_private_modes() {
    let env = setup(vec![key(KEY_ID, KEY, "laptop")]).await;
    let mut p = Pty::spawn(&env, &["runner", "allow", "tmgrdev", "--for", "7d"]);
    p.wait_for("Type the workspace code");
    assert!(p.text().contains("laptop"));
    p.type_line("tmgrdev");
    assert_eq!(p.exit_code(), 0, "{}", p.text());
    let f = read_allow(&env);
    let e = &f["entries"][0];
    assert_eq!(f["version"], 1);
    assert_eq!(e["workspace_id"], 56);
    assert_eq!(e["user_id"], 1);
    assert_eq!(e["workspace_code"], "tmgrdev");
    assert_eq!(e["keys"][0]["key_id"], KEY_ID);
    assert_eq!(e["keys"][0]["public_key"], KEY);
    assert_eq!(e["server"], env.server.address().to_string());
    assert!(e["repos"].as_object().unwrap().is_empty());
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&allow_file(&env)), 0o600);
    assert_eq!(mode(&env.config_dir()), 0o700);

    let out = env
        .cmd()
        .args(["runner", "allowed", "--json"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let d = &v["data"][0];
    assert_eq!(d["workspace_code"], "tmgrdev");
    assert_eq!(d["days_left"], 6);
    assert_eq!(d["expiring_soon"], false);
    assert!(d["expires_at"].as_str().unwrap().ends_with('Z'));
    assert_eq!(d["keys"][0]["key_id"], KEY_ID);
    assert_eq!(d["keys"][0]["fingerprint"].as_str().unwrap().len(), 16);
    assert_ne!(d["keys"][0]["fingerprint"], "ignored");
    for field in ["workspace_id", "user_id", "server", "repos"] {
        assert!(d.get(field).is_some(), "{field}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_code_changes_nothing() {
    let env = setup(vec![key(KEY_ID, KEY, "laptop")]).await;
    let mut p = Pty::spawn(&env, &["runner", "allow", "tmgrdev"]);
    p.wait_for("Type the workspace code");
    p.type_line("yes");
    assert_ne!(p.exit_code(), 0);
    assert!(!allow_file(&env).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn account_without_keys_is_refused_with_the_hint() {
    let env = setup(vec![]).await;
    let mut p = Pty::spawn(&env, &["runner", "allow", "tmgrdev"]);
    assert_eq!(p.exit_code(), 6, "{}", p.text());
    assert!(p.text().contains("web app"), "{}", p.text());
    assert!(!allow_file(&env).exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_workspaces_are_refused() {
    let env = setup(vec![key(KEY_ID, KEY, "laptop")]).await;
    let mut p = Pty::spawn(&env, &["runner", "allow", "local:notes"]);
    assert_eq!(p.exit_code(), 2, "{}", p.text());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn re_allow_marks_new_keys_keeps_repos_and_forever_has_no_expiry() {
    let env = setup(vec![
        key(KEY_ID, KEY, "laptop"),
        key(OTHER_ID, OTHER, "evil-phone"),
    ])
    .await;
    std::fs::create_dir_all(env.config_dir()).unwrap();
    let seeded = json!({"version": 1, "entries": [{
        "server": env.server.address().to_string(), "user_id": 1, "workspace_id": 56,
        "workspace_code": "tmgrdev", "allowed_at": "2025-10-01T00:00:00Z", "expires_at": "2025-10-02T00:00:00Z",
        "keys": [{"key_id": KEY_ID, "public_key": KEY, "device_name": "laptop"}],
        "repos": {"127": "/abs/repo"}}]});
    std::fs::write(allow_file(&env), seeded.to_string()).unwrap();
    let mut p = Pty::spawn(&env, &["runner", "allow", "tmgrdev", "--for", "forever"]);
    p.wait_for("Type the workspace code");
    let shown = p.text();
    let new_line = shown.lines().find(|l| l.contains("evil-phone")).unwrap();
    assert!(new_line.contains("NEW"), "{shown}");
    let old_line = shown.lines().find(|l| l.contains("laptop")).unwrap();
    assert!(!old_line.contains("NEW"), "{shown}");
    p.type_line("tmgrdev");
    assert_eq!(p.exit_code(), 0);
    let e = &read_allow(&env)["entries"][0];
    assert_eq!(read_allow(&env)["entries"].as_array().unwrap().len(), 1);
    assert_eq!(e["keys"].as_array().unwrap().len(), 2);
    assert_eq!(e["repos"]["127"], "/abs/repo");
    assert!(e["expires_at"].is_null());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn expired_entries_are_listed_as_expired_and_disallow_removes_them() {
    let env = setup(vec![]).await;
    std::fs::create_dir_all(env.config_dir()).unwrap();
    let seeded = json!({"version": 1, "entries": [{
        "server": env.server.address().to_string(), "user_id": 1, "workspace_id": 56,
        "workspace_code": "tmgrdev", "allowed_at": "2025-10-01T00:00:00Z", "expires_at": "2025-10-02T00:00:00Z",
        "keys": [{"key_id": KEY_ID, "public_key": KEY, "device_name": "laptop"}], "repos": {}}]});
    std::fs::write(allow_file(&env), seeded.to_string()).unwrap();
    let out = env.cmd().args(["runner", "allowed"]).output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    assert!(
        text.contains("tmgrdev") && text.contains("expired 2025-10-02"),
        "{text}"
    );
    let out = env
        .cmd()
        .env("TMGR_RUN_TOKEN", "x")
        .args(["runner", "disallow", "tmgrdev"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(read_allow(&env)["entries"].as_array().unwrap().is_empty());
    let out = env
        .cmd()
        .args(["runner", "disallow", "tmgrdev"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("was not allowed"));
    let out = env
        .cmd()
        .args(["runner", "allowed", "--json"])
        .output()
        .unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"], json!([]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn running_a_cloud_task_of_a_workspace_that_is_not_allowed_names_the_fix() {
    let env = setup(vec![key(KEY_ID, KEY, "laptop")]).await;
    mount(
        &env,
        "/api/tasks/9002",
        common::task_json(9002, "Fix login"),
    )
    .await;
    mount(
        &env,
        "/api/agent-runs/tasks/9002",
        json!({"data": {
            "task_id": 9002, "workspace_id": 56, "workspace_code": "tmgrdev", "viewer_user_id": 1,
            "title": "Fix login", "description": "x", "setting_enabled": true, "approval": null,
            "cwd": "/etc", "mode": "bypass", "argv": ["sh"], "comments": [], "start": true
        }}),
    )
    .await;
    let out = env
        .cmd()
        .args(["--ws", "tmgrdev", "run", "9002", "--agent", "fake", "--yes"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5), "{}", common::text(&out.stderr));
    let err = common::text(&out.stderr);
    assert!(err.contains("not allowed"), "{err}");
    assert!(err.contains("tmgr runner allow tmgrdev"), "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cli_key_of_another_machine_is_listed_but_not_pinned() {
    let mut cli = key(OTHER_ID, OTHER, "CLI (macOS)");
    cli["kind"] = json!("cli");
    let mut browser = key(KEY_ID, KEY, "laptop");
    browser["kind"] = json!("browser");
    let env = setup(vec![browser, cli]).await;
    let mut p = Pty::spawn(&env, &["runner", "allow", "tmgrdev"]);
    p.wait_for("Type the workspace code");
    assert!(
        p.text().contains("CLI key of another machine"),
        "{}",
        p.text()
    );
    p.type_line("tmgrdev");
    assert_eq!(p.exit_code(), 0, "{}", p.text());
    let keys = read_allow(&env)["entries"][0]["keys"].clone();
    assert_eq!(keys.as_array().unwrap().len(), 1);
    assert_eq!(keys[0]["key_id"], KEY_ID);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_foreign_cli_keys_means_no_signing_key_for_this_machine() {
    let mut cli = key(OTHER_ID, OTHER, "CLI (macOS)");
    cli["kind"] = json!("cli");
    let env = setup(vec![cli]).await;
    let mut p = Pty::spawn(&env, &["runner", "allow", "tmgrdev"]);
    assert_eq!(p.exit_code(), 6, "{}", p.text());
    assert!(!allow_file(&env).exists());
}
