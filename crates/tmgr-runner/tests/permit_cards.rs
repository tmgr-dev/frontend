#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use tmgr_ipc::api::{NoSandbox, RunCreateReq, SessionInfo, StateKind};
use tmgr_runner::client::Client;
use tmgr_runner::permit_hook;

const HOST: &str = env!("CARGO_BIN_EXE_tmgr-runner-host");

struct Fx {
    tmp: tempfile::TempDir,
    run: PathBuf,
    homes: PathBuf,
    work: PathBuf,
    agents: PathBuf,
    cfg: PathBuf,
    home: PathBuf,
    runner: Option<Child>,
}

impl Fx {
    fn new() -> Fx {
        let tmp = tempfile::Builder::new()
            .prefix("pc")
            .tempdir_in("/tmp")
            .unwrap();
        let base = fs::canonicalize(tmp.path()).unwrap();
        let dir = |name: &str| {
            let p = base.join(name);
            fs::create_dir(&p).unwrap();
            fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).unwrap();
            p
        };
        let (run, homes, work, agents, cfg, home) = (
            dir("r"),
            dir("h"),
            dir("w"),
            dir("a"),
            dir("c"),
            dir("realhome"),
        );
        fs::create_dir(cfg.join("tmgr")).unwrap();
        fs::set_permissions(cfg.join("tmgr"), fs::Permissions::from_mode(0o700)).unwrap();
        let bin = work.join("bin");
        fs::create_dir(&bin).unwrap();
        for agent in ["claude", "codex", "plain"] {
            let p = bin.join(agent);
            fs::write(
                &p,
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$HOME/argv.txt\"\nsleep 60\n",
            )
            .unwrap();
            fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        }
        let plain = bin.join("plain");
        fs::write(
            agents.join("plain.toml"),
            format!(
                "id = \"plain\"\nname = \"plain\"\ncommand = [{:?}]\n",
                plain.display().to_string()
            ),
        )
        .unwrap();
        Fx {
            tmp,
            run,
            homes,
            work,
            agents,
            cfg,
            home,
            runner: None,
        }
    }

    fn start(&mut self) {
        let err = fs::File::create(self.tmp.path().join("runner.err")).unwrap();
        let path = format!("{}:/usr/bin:/bin", self.work.join("bin").display());
        self.runner = Some(
            Command::new(HOST)
                .args(["runner", "serve"])
                .env("HOME", &self.home)
                .env("PATH", path)
                .env("TMGR_RUNNER_DIR", &self.run)
                .env("TMGR_RUNNER_HOMES", &self.homes)
                .env("TMGR_AGENTS_DIR", &self.agents)
                .env("XDG_CONFIG_HOME", &self.cfg)
                .env("TMGR_T0_QUIET_MS", "5000")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(err)
                .spawn()
                .unwrap(),
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        while Client::connect(&self.run).is_err() {
            assert!(Instant::now() < deadline, "runner did not start");
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn kill_runner(&mut self) {
        if let Some(mut r) = self.runner.take() {
            let _ = r.kill();
            let _ = r.wait();
        }
    }

    fn client(&self) -> Client {
        let c = Client::connect(&self.run).unwrap();
        c.set_timeouts(Duration::from_secs(60)).unwrap();
        c
    }

    fn req(&self, profile: &str, cards: bool) -> RunCreateReq {
        RunCreateReq {
            clones: Vec::new(),
            profile: profile.into(),
            task: None,
            cwd: self.work.display().to_string(),
            name: None,
            model: None,
            cols: 80,
            rows: 24,
            bundle: None,
            bundle_sha256: None,
            cwd_pin: None,
            origin: None,
            permission_mode: None,
            start_key: None,
            clone_of: None,
            trust_cwd: None,
            trust_via_repo: None,
            agent_work: false,
            cloud: None,
            unattended: false,
            exe_pin: None,
            argv_sha256: None,
            no_sandbox: None,
            permission_cards: cards,
        }
    }

    fn unsandboxed(&self, mut r: RunCreateReq) -> RunCreateReq {
        let path = format!("{}:/usr/bin:/bin", self.work.join("bin").display());
        r.no_sandbox = Some(NoSandbox {
            env: [("PATH".to_string(), path)].into_iter().collect(),
        });
        r
    }

    fn run_home(&self, id: &str) -> PathBuf {
        self.homes.join(id)
    }
}

impl Drop for Fx {
    fn drop(&mut self) {
        if self.runner.is_none() && Client::connect(&self.run).is_err() {
            self.start();
        }
        if let Ok(mut c) = Client::connect(&self.run) {
            for s in c.list().unwrap_or_default() {
                let _ = c.kill(&s.id, "kill");
            }
            let _ = c.stop();
        }
        thread::sleep(Duration::from_millis(200));
        self.kill_runner();
    }
}

fn sessions_available() -> bool {
    tmgr_runner::daemon::landlock_abi() >= 6
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !done() {
        assert!(Instant::now() < deadline, "{what}");
        thread::sleep(Duration::from_millis(50));
    }
}

fn argv_of(home: &Path) -> String {
    let p = home.join("argv.txt");
    wait_for("agent argv", || p.exists());
    thread::sleep(Duration::from_millis(100));
    fs::read_to_string(p).unwrap()
}

fn info_of(c: &mut Client, id: &str) -> SessionInfo {
    c.list().unwrap().into_iter().find(|s| s.id == id).unwrap()
}

fn listing(dir: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    v.sort();
    v
}

#[test]
fn off_by_default_leaves_argv_and_home_untouched() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    fx.start();
    for profile in ["claude-code", "codex"] {
        let info = fx.client().create_run(fx.req(profile, false)).unwrap();
        assert!(!info.permission_cards);
        let home = fx.run_home(&info.id);
        let argv = argv_of(&home);
        assert!(
            !argv.contains("--settings") && !argv.contains("permission-hook"),
            "{argv}"
        );
        assert!(!home.join(".codex/config.toml").exists());
        assert!(!home.join(".cache/tmgr-permit").exists());
        let _ = fx.client().kill(&info.id, "kill");
    }
}

#[test]
fn claude_gets_one_additive_settings_argument_and_the_flag_persists() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    fx.start();
    let info = fx.client().create_run(fx.req("claude-code", true)).unwrap();
    assert!(info.permission_cards);
    let argv = argv_of(&fx.run_home(&info.id));
    let lines: Vec<&str> = argv.lines().collect();
    let at = lines.iter().position(|l| *l == "--settings").expect(&argv);
    let json: serde_json::Value = serde_json::from_str(lines[at + 1]).unwrap();
    let command = json["hooks"]["PermissionRequest"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    assert!(command.ends_with(" __permission-hook claude"), "{command}");
    assert!(command.contains("tmgr-runner-host"), "{command}");
    assert_eq!(lines.iter().filter(|l| **l == "--settings").count(), 1);
    assert!(fs::read_dir(fx.home.join(".claude")).is_err());

    fx.kill_runner();
    fx.start();
    assert!(info_of(&mut fx.client(), &info.id).permission_cards);
}

#[test]
fn the_flag_and_the_hook_carry_through_resume() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    fx.start();
    let info = fx.client().create_run(fx.req("claude-code", true)).unwrap();
    argv_of(&fx.run_home(&info.id));
    let _ = fx.client().kill(&info.id, "kill");
    wait_for("exit", || {
        info_of(&mut fx.client(), &info.id).state.kind == StateKind::Exited
    });
    let resumed = fx.client().resume(&info.id, 80, 24).unwrap();
    assert!(resumed.session.permission_cards);
    let argv = argv_of(&fx.run_home(&resumed.session.id));
    assert!(argv.contains("--settings"), "{argv}");
    assert!(argv.contains("--continue"), "{argv}");
}

#[test]
fn claude_without_the_sandbox_still_gets_the_settings_argument() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    fx.start();
    let req = fx.unsandboxed(fx.req("claude-code", true));
    let info = fx.client().create_run(req).unwrap();
    assert!(info.no_sandbox && info.permission_cards);
    assert!(argv_of(&fx.home).contains("--settings"));
}

#[test]
fn sandboxed_codex_gets_a_trusted_hook_in_its_run_home() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    fx.start();
    let info = fx.client().create_run(fx.req("codex", true)).unwrap();
    assert!(info.permission_cards);
    let home = fx.run_home(&info.id);
    let argv = argv_of(&home);
    assert!(!argv.contains("--settings"), "{argv}");
    let text = fs::read_to_string(home.join(".codex/config.toml")).unwrap();
    let v: toml::Value = toml::from_str(&text).unwrap();
    let command = v["hooks"]["PermissionRequest"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    assert!(command.ends_with(" __permission-hook codex"), "{command}");
    assert_eq!(v["features"]["hooks"].as_bool(), Some(true));
    let key = format!(
        "{}:permission_request:0:0",
        fs::canonicalize(&home)
            .unwrap()
            .join(".codex/config.toml")
            .display()
    );
    assert_eq!(
        v["hooks"]["state"][key.as_str()]["trusted_hash"].as_str(),
        Some(permit_hook::trust_hash(command).as_str())
    );
    let mode = fs::metadata(home.join(".codex/config.toml"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o077, 0);
}

#[test]
fn codex_without_the_sandbox_writes_nothing_anywhere() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    fx.start();
    let req = fx.unsandboxed(fx.req("codex", true));
    let info = fx.client().create_run(req).unwrap();
    assert!(info.no_sandbox && info.permission_cards);
    let argv = argv_of(&fx.home);
    assert!(
        !argv.contains("--settings") && !argv.contains("permission-hook"),
        "{argv}"
    );
    assert_eq!(listing(&fx.home), ["argv.txt"]);
    assert!(!fx.run_home(&info.id).join(".codex").exists());
}

#[test]
fn non_agent_profiles_ignore_the_flag() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    fx.start();
    let info = fx.client().create_run(fx.req("plain", true)).unwrap();
    assert!(!info.permission_cards);
    let argv = argv_of(&fx.run_home(&info.id));
    assert!(!argv.contains("--settings"), "{argv}");
}

#[test]
fn only_a_manual_cli_run_may_ask_for_cards() {
    if !sessions_available() {
        return;
    }
    let mut fx = Fx::new();
    fx.start();
    let mut desktop = Client::connect_as(&fx.run, "desktop").unwrap();
    let e = desktop
        .create_run(fx.req("claude-code", true))
        .unwrap_err()
        .to_string();
    assert!(e.contains("forbidden"), "{e}");
    let mut keyed = fx.req("claude-code", true);
    keyed.start_key = Some("k1".into());
    let e = fx.client().create_run(keyed).unwrap_err().to_string();
    assert!(e.contains("forbidden"), "{e}");
    assert!(fx.client().list().unwrap().is_empty());
}

fn run_hook(home: &Path, session: Option<&str>, stdin: &[u8]) -> std::process::Output {
    let mut cmd = Command::new(HOST);
    cmd.args(["__permission-hook", "claude"])
        .env_clear()
        .env("HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(s) = session {
        cmd.env("TMGR_SESSION_ID", s);
    }
    let mut child = cmd.spawn().unwrap();
    let _ = child.stdin.take().unwrap().write_all(stdin);
    child.wait_with_output().unwrap()
}

#[test]
fn the_hook_command_prints_nothing_and_exits_zero() {
    let tmp = tempfile::Builder::new()
        .prefix("ph")
        .tempdir_in("/tmp")
        .unwrap();
    let home = fs::canonicalize(tmp.path()).unwrap();
    let ok = run_hook(&home, Some("s_abc"), br#"{"tool_name":"Bash"}"#);
    assert!(ok.status.success() && ok.stdout.is_empty() && ok.stderr.is_empty());
    let spooled = listing(&permit_hook::spool_dir(&home, "s_abc"));
    assert_eq!(spooled.len(), 1);
    for bad in [
        run_hook(&home, Some("s_abc"), b"\xff garbage"),
        run_hook(&home, Some("../x"), b"{}"),
        run_hook(&home, None, b"{}"),
    ] {
        assert!(bad.status.success() && bad.stdout.is_empty());
    }
    assert_eq!(listing(&permit_hook::spool_dir(&home, "s_abc")), spooled);
    assert_eq!(listing(&home), [".cache"]);
    let other = Command::new(HOST)
        .args(["__permission-hook", "nonsense"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(other.status.success() && other.stdout.is_empty());
}
