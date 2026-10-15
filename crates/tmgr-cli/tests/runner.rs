#![cfg(feature = "runner")]

use portable_pty::{native_pty_system, Child, CommandBuilder, PtySize};
use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tmgr_ipc::api::StateKind;
use tmgr_runner::client::Client;

const BIN: &str = env!("CARGO_BIN_EXE_tmgr");

struct Env {
    tmp: tempfile::TempDir,
    run: PathBuf,
    homes: tempfile::TempDir,
    proj: PathBuf,
}

fn private_dir(path: &Path) {
    fs::create_dir_all(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn run_bounded(mut cmd: Command, limit: Duration) -> Output {
    let mut child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut buf = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut buf);
            }
            buf
        })
    };
    let out = drain(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let err = drain(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let deadline = Instant::now() + limit;
    let status = loop {
        if let Some(st) = child.try_wait().unwrap() {
            break st;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("command did not finish within {limit:?}");
        }
        thread::sleep(Duration::from_millis(20));
    };
    Output {
        status,
        stdout: out.join().unwrap(),
        stderr: err.join().unwrap(),
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

impl Env {
    fn new() -> Env {
        let tmp = tempfile::Builder::new()
            .prefix("tr")
            .tempdir_in("/tmp")
            .unwrap();
        std::fs::set_permissions(
            tmp.path(),
            <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700),
        )
        .unwrap();
        let run = tmp.path().join("r");
        private_dir(&run);
        let proj = tmp.path().join("p");
        private_dir(&proj);
        private_dir(&tmp.path().join("cfg"));
        let homes = tempfile::Builder::new()
            .prefix("th")
            .tempdir_in("/tmp")
            .unwrap();
        std::fs::set_permissions(
            homes.path(),
            <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700),
        )
        .unwrap();
        private_dir(homes.path());
        Env {
            tmp,
            run,
            homes,
            proj,
        }
    }

    fn envs(&self) -> Vec<(&'static str, String)> {
        let cfg = self.tmp.path().join("cfg");
        vec![
            ("TMGR_RUNNER_DIR", self.run.display().to_string()),
            ("TMGR_RUNNER_HOMES", self.homes.path().display().to_string()),
            ("TMGR_T0_QUIET_MS", "300".into()),
            ("XDG_CONFIG_HOME", cfg.display().to_string()),
            ("TMGR_AGENTS_DIR", cfg.join("agents").display().to_string()),
            ("TERM", "xterm-256color".into()),
            ("NO_COLOR", "1".into()),
        ]
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
        c.args(args).envs(self.envs()).stdin(Stdio::null());
        c
    }

    fn tmgr(&self, args: &[&str]) -> Output {
        run_bounded(self.cmd(args), Duration::from_secs(60))
    }

    fn data(&self, args: &[&str]) -> Value {
        let out = self.tmgr(args);
        let v: Value = serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| panic!("{args:?}: {e}: {}", String::from_utf8_lossy(&out.stdout)));
        assert_eq!(v["ok"], true, "{args:?}: {v}");
        v["data"].clone()
    }

    fn script(&self, name: &str, body: &str) -> String {
        let p = self.proj.join(name);
        fs::write(&p, format!("#!/bin/sh\n{body}")).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
        p.display().to_string()
    }

    fn start(&self, script: &str, name: &str) -> Option<String> {
        if !sandbox_usable() {
            return None;
        }
        let cwd = self.proj.display().to_string();
        let out = self.tmgr(&[
            "run",
            "--agent-cmd",
            script,
            "--cwd",
            &cwd,
            "--name",
            name,
            "--no-attach",
            "--json",
        ]);
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        if !out.status.success() && text.contains("sandbox unavailable") {
            eprintln!("skipped: {text}");
            return None;
        }
        assert!(
            out.status.success(),
            "{text}{}{}",
            String::from_utf8_lossy(&out.stderr),
            fs::read_to_string(self.run.join("runner.log")).unwrap_or_default()
        );
        let v: Value = serde_json::from_str(&text).unwrap();
        Some(v["data"]["id"].as_str().unwrap().to_string())
    }

    fn sessions(&self) -> Vec<Value> {
        self.data(&["agents", "--json"])["sessions"]
            .as_array()
            .unwrap()
            .clone()
    }

    fn pty(&self, args: &[&str]) -> Pty {
        Pty::spawn(self, args)
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        shutdown_runner(&self.run);
    }
}

fn pid_of(pid: i64) -> Option<i32> {
    i32::try_from(pid).ok().filter(|p| *p > 1)
}

fn shutdown_runner(run: &Path) {
    let Ok(mut c) = Client::connect(run) else {
        return;
    };
    let _ = c.set_timeouts(Duration::from_secs(5));
    if let Ok(list) = c.list() {
        for s in list.iter().filter(|s| s.state.kind != StateKind::Exited) {
            let _ = c.kill(&s.id, "kill");
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            match c.list() {
                Ok(l) if l.iter().any(|s| s.state.kind != StateKind::Exited) => {
                    thread::sleep(Duration::from_millis(50))
                }
                _ => break,
            }
        }
        for s in &list {
            let _ = c.remove(&s.id);
        }
    }
    let _ = c.stop();
    let deadline = Instant::now() + Duration::from_secs(5);
    while Client::connect(run).is_ok() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(50));
    }
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
    fn spawn(env: &Env, args: &[&str]) -> Pty {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 30,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut cmd = CommandBuilder::new(BIN);
        cmd.args(args);
        for (k, v) in env.envs() {
            cmd.env(k, v);
        }
        let child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let screen = Arc::new(Mutex::new(vt100::Parser::new(30, 100, 0)));
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

    fn text(&self) -> String {
        self.screen.lock().unwrap().screen().contents()
    }

    fn wait_for(&self, needle: &str) {
        let end = Instant::now() + Duration::from_secs(15);
        while !self.text().contains(needle) {
            assert!(
                Instant::now() < end,
                "timed out waiting for {needle:?}; screen:\n{}",
                self.text()
            );
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn type_bytes(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).unwrap();
        self.writer.flush().unwrap();
        thread::sleep(Duration::from_millis(100));
    }

    fn wait_exit(&mut self, within: Duration) -> bool {
        let end = Instant::now() + within;
        while Instant::now() < end {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status.success();
            }
            thread::sleep(Duration::from_millis(50));
        }
        panic!("attach did not exit; screen:\n{}", self.text());
    }

    fn wait_exit_quiet(&mut self, within: Duration) -> bool {
        let end = Instant::now() + within;
        while Instant::now() < end {
            if self.child.try_wait().unwrap().is_some() {
                return true;
            }
            thread::sleep(Duration::from_millis(50));
        }
        false
    }

    fn detach(&mut self) {
        self.type_bytes(b"\x1d");
        self.type_bytes(b"d");
        assert!(self.wait_exit(Duration::from_secs(3)));
    }
}

fn is_subsequence(seq: &[String], want: &[&str]) -> bool {
    let mut it = seq.iter();
    want.iter().all(|w| it.any(|s| s == w))
}

const FAKE_AGENT: &str = r#"i=0
while [ $i -lt 10 ]; do echo "progress $i"; i=$((i+1)); sleep 0.1; done
touch made-by-agent.txt
echo "type go"
read line
i=0
while [ $i -lt 10 ]; do echo "more $i"; i=$((i+1)); sleep 0.1; done
exit 0
"#;

const ECHO_AGENT: &str = "echo READY\nwhile read l; do echo \"echo:$l\"; done\n";

#[test]
fn acceptance_working_waiting_working_exited() {
    let env = Env::new();
    let script = env.script("fake-agent.sh", FAKE_AGENT);
    let Some(id) = env.start(&script, "fake") else {
        return;
    };
    let mut seen: Vec<String> = Vec::new();
    let mut sent = false;
    let mut exit_code = Value::Null;
    let end = Instant::now() + Duration::from_secs(15);
    while Instant::now() < end {
        let list = env.sessions();
        let s = list.iter().find(|s| s["id"] == id.as_str()).unwrap();
        let kind = s["state"]["kind"].as_str().unwrap().to_string();
        if seen.last() != Some(&kind) {
            seen.push(kind.clone());
        }
        if kind == "waiting" && !sent {
            let out = env.tmgr(&["send", &id, "go"]);
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
            sent = true;
        }
        if kind == "exited" {
            exit_code = s["state"]["exit_code"].clone();
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    assert!(
        is_subsequence(&seen, &["working", "waiting", "working", "exited"]),
        "{seen:?}"
    );
    assert_eq!(exit_code, 0);
    assert!(env.proj.join("made-by-agent.txt").exists());
}

#[test]
fn attach_drives_the_real_ui_and_reattach_restores_the_screen() {
    let env = Env::new();
    let script = env.script("echo-agent.sh", ECHO_AGENT);
    let Some(id) = env.start(&script, "echoer") else {
        return;
    };
    let mut pty = env.pty(&["attach", &id]);
    pty.wait_for("READY");
    pty.type_bytes(b"hi\r");
    pty.wait_for("echo:hi");
    pty.detach();
    assert!(pty.text().contains("detached from"), "{}", pty.text());
    let list = env.sessions();
    assert_ne!(list[0]["state"]["kind"], "exited");

    let mut again = env.pty(&["attach"]);
    again.wait_for("echo:hi");
    again.detach();
}

#[test]
fn second_pane_and_zoom() {
    let env = Env::new();
    let a = env.script("a.sh", "echo READY-A\nsleep 300\n");
    let b = env.script("b.sh", "echo READY-B\nsleep 300\n");
    let Some(ida) = env.start(&a, "alpha") else {
        return;
    };
    env.start(&b, "beta").unwrap();
    let mut pty = env.pty(&["attach", &ida]);
    pty.wait_for("READY-A");
    pty.type_bytes(b"\x1d");
    pty.type_bytes(b"s");
    pty.wait_for("READY-B");
    let text = pty.text();
    assert!(text.contains("READY-A"), "{text}");
    assert!(
        text.contains("alpha · ") && text.contains("beta · "),
        "{text}"
    );
    pty.type_bytes(b"\x1d");
    pty.type_bytes(b"z");
    let end = Instant::now() + Duration::from_secs(10);
    while pty.text().contains("alpha · ") {
        assert!(Instant::now() < end, "{}", pty.text());
        thread::sleep(Duration::from_millis(50));
    }
    assert!(pty.text().contains("beta · "), "{}", pty.text());
    pty.detach();
}

#[test]
fn bad_run_requests_are_refused() {
    let env = Env::new();
    let cwd = env.proj.display().to_string();
    let out = env.tmgr(&[
        "run",
        "--agent-cmd",
        "definitely-not-a-command-xyz",
        "--cwd",
        &cwd,
        "--no-attach",
    ]);
    assert_eq!(out.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&out.stderr).contains("definitely-not-a-command-xyz"));
    assert!(env.sessions().is_empty());

    let out = env.tmgr(&["run", "--agent", "nope", "--cwd", &cwd, "--no-attach"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown agent profile"));

    let script = env.script("x.sh", "sleep 5\n");
    let home = std::env::var("HOME").unwrap();
    let out = env.tmgr(&["run", "--agent-cmd", &script, "--cwd", &home, "--no-attach"]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("home directory"), "{err}");
    assert!(env.sessions().is_empty());
}

#[test]
fn an_agent_without_a_login_is_refused_before_anything_starts() {
    let env = Env::new();
    let home = env.tmp.path().join("home");
    private_dir(&home);
    let cwd = env.proj.display().to_string();
    for (agent, want) in [
        (
            "claude-code",
            "Claude Code has no login for the runner yet: run `claude setup-token`, then `tmgr-exp agents login claude-code`",
        ),
        ("codex", "run `codex` once"),
    ] {
        let mut cmd = env.cmd(&["run", "--agent", agent, "--cwd", &cwd, "--no-attach"]);
        cmd.arg0("tmgr-exp")
            .env("HOME", &home)
            .env("PATH", "/usr/bin:/bin");
        let out = run_bounded(cmd, Duration::from_secs(60));
        let err = String::from_utf8_lossy(&out.stderr);
        assert_eq!(out.status.code(), Some(2), "{err}");
        assert!(err.contains(want), "{err}");
    }
    assert!(!env.run.join("runner.pid").exists());
}

#[test]
fn hints_name_the_binary_that_was_invoked() {
    let env = Env::new();
    if !sandbox_usable() {
        return;
    }
    let script = env.script("idle.sh", "sleep 30\n");
    let cwd = env.proj.display().to_string();
    let mut cmd = env.cmd(&["run", "--agent-cmd", &script, "--cwd", &cwd, "--no-attach"]);
    cmd.arg0("tmgr-exp");
    let out = run_bounded(cmd, Duration::from_secs(60));
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}");
    assert!(text.contains("attach with: tmgr-exp attach s_"), "{text}");
}

#[test]
fn run_output_never_carries_control_sequences_from_file_names() {
    let env = Env::new();
    let script = env.script("evil\x1b]0;pwned\x07.sh", "sleep 5\n");
    let cwd = env.proj.display().to_string();
    let out = env.tmgr(&["run", "--agent-cmd", &script, "--cwd", &cwd, "--no-attach"]);
    for stream in [&out.stdout, &out.stderr] {
        assert!(
            !stream.iter().any(|b| *b == 0x1b || *b == 0x07),
            "{}",
            String::from_utf8_lossy(stream)
        );
    }
    assert!(String::from_utf8_lossy(&out.stderr).contains("evil?]0;pwned?.sh"));
}

#[test]
fn detach_returns_and_restores_the_terminal_while_the_runner_is_frozen() {
    let env = Env::new();
    let script = env.script("echo-agent.sh", ECHO_AGENT);
    let Some(id) = env.start(&script, "echoer") else {
        return;
    };
    let mut pty = env.pty(&["attach", &id]);
    pty.wait_for("READY");
    let pid = pid_of(
        env.data(&["runner", "status", "--json"])["pid"]
            .as_i64()
            .unwrap(),
    )
    .unwrap();
    unsafe { libc::kill(pid, libc::SIGSTOP) };
    let started = Instant::now();
    pty.type_bytes(b"\x1d");
    pty.type_bytes(b"d");
    let exited = pty.wait_exit_quiet(Duration::from_secs(4));
    unsafe { libc::kill(pid, libc::SIGCONT) };
    assert!(exited, "attach hung on a frozen runner");
    assert!(started.elapsed() < Duration::from_secs(4));
    assert!(pty.text().contains("detached from"), "{}", pty.text());
}

#[test]
fn agents_without_a_runner_is_empty_and_does_not_start_one() {
    let env = Env::new();
    let out = env.tmgr(&["agents"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("No agent sessions"));
    let data = env.data(&["agents", "--json"]);
    assert_eq!(data["sessions"].as_array().unwrap().len(), 0);
    assert_eq!(data["summary"]["working"], 0);
    assert_eq!(fs::read_dir(&env.run).unwrap().count(), 0);
    assert_eq!(env.tmgr(&["runner", "status"]).status.code(), Some(7));
    assert!(env.tmgr(&["runner", "stop"]).status.success());
}

#[test]
fn halt_send_and_rm_manage_a_session() {
    let env = Env::new();
    let script = env.script("sleeper.sh", "sleep 300\n");
    let Some(id) = env.start(&script, "sleeper") else {
        return;
    };
    let st = env.data(&["runner", "status", "--json"]);
    assert_eq!(st["sessions"], 1);
    assert!(st["pid"].as_u64().unwrap() > 1);
    for key in ["version", "proto", "uptime_s"] {
        assert!(st.get(key).is_some(), "{key}");
    }
    assert_eq!(env.tmgr(&["rm", "sleeper"]).status.code(), Some(6));
    assert_eq!(env.tmgr(&["halt", "missing"]).status.code(), Some(3));
    let halted = env.data(&["halt", "sleeper", "--json"]);
    assert_eq!(halted["exited"], true);
    let data = env.data(&["agents", "--json"]);
    assert_eq!(data["summary"]["exited"], 1);
    env.data(&["rm", &id, "--json"]);
    assert!(env.sessions().is_empty());
}

#[test]
fn run_is_refused_without_a_usable_sandbox() {
    if tmgr_runner::daemon::landlock_abi() >= 6 {
        eprintln!("skipped: the sandbox is usable here");
        return;
    }
    let env = Env::new();
    let script = env.script("never.sh", "sleep 5\n");
    let cwd = env.proj.display().to_string();
    let out = env.tmgr(&["run", "--agent-cmd", &script, "--cwd", &cwd, "--no-attach"]);
    assert!(!out.status.success());
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(text.contains("sandbox unavailable"), "{text}");
    assert!(env.sessions().is_empty());
}

#[test]
fn agents_login_stores_a_claude_credential_without_echoing_the_token() {
    const TOKEN: &str = concat!("sk-", "ant-oat01-FAKEFAKEFAKEFAKEFAKE");
    let env = Env::new();
    let home = env.tmp.path().join("fakehome");
    private_dir(&home);
    let file = home.join(".config/tmgr/agents/claude-code.credentials.json");
    let run = |args: &[&str], stdin: &str| {
        let input = env.tmp.path().join("stdin.txt");
        fs::write(&input, stdin).unwrap();
        let mut c = env.cmd(args);
        c.env("HOME", &home).stdin(fs::File::open(&input).unwrap());
        run_bounded(c, Duration::from_secs(60))
    };
    let text = |o: &Output| {
        format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        )
    };

    let out = run(&["agents", "login", "claude-code"], &format!("{TOKEN}\n"));
    assert!(out.status.success(), "login failed");
    assert!(!text(&out).contains(TOKEN), "output leaks the token");
    let shown = text(&out);
    assert!(shown.contains("credentials.json") && shown.contains("Next session will be logged in"));
    let v: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    let o = &v["claudeAiOauth"];
    assert_eq!(o["accessToken"], TOKEN);
    assert!(o["refreshToken"].is_null() && o["subscriptionType"].is_null());
    assert_eq!(o["scopes"], serde_json::json!(["user:inference"]));
    assert!(o["expiresAt"].as_u64().unwrap() > 300 * 24 * 3600 * 1000);
    let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&file), 0o600);
    assert_eq!(mode(file.parent().unwrap()), 0o700);

    let out = run(&["agents", "login", "claude-code"], &format!("{TOKEN}\n"));
    assert!(!out.status.success() && text(&out).contains("--replace"));
    assert!(!text(&out).contains(TOKEN), "output leaks the token");

    let other = concat!("sk-", "ant-oat01-OTHEROTHEROTHEROTHER");
    let out = run(&["agents", "login", "claude-code", "--replace"], other);
    assert!(out.status.success(), "replace failed");
    let v: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
    assert_eq!(v["claudeAiOauth"]["accessToken"], other);

    let before = fs::read(&file).unwrap();
    for bad in [
        concat!("sk-", "ant-api03-FAKEFAKEFAKEFAKEFAKE"),
        "sk-ant-oat01-short",
        "",
    ] {
        let out = run(&["agents", "login", "claude-code", "--replace"], bad);
        assert!(!out.status.success(), "bad token accepted");
        assert!(
            bad.is_empty() || !text(&out).contains(bad),
            "output leaks the token"
        );
    }
    assert_eq!(fs::read(&file).unwrap(), before);

    assert!(!run(&["agents", "login", "no-such"], TOKEN).status.success());
    assert!(!run(&["agents", "login", "codex"], TOKEN).status.success());

    let out = run(&["agents", "logout", "claude-code"], "");
    assert!(out.status.success() && !file.exists());
    let out = run(&["agents", "logout", "claude-code"], "");
    assert!(out.status.success() && text(&out).contains("nothing to remove"));
}

const OLD_TOKEN: &str = concat!("old-", "refresh-FAKEOLD1234");
const NEW_TOKEN: &str = concat!("new-", "refresh-FAKENEW5678");

struct Cand {
    home: PathBuf,
    source: PathBuf,
    old: String,
    new: String,
}

fn login_doc(days_ago: i64, token: &str) -> String {
    serde_json::json!({
        "auth_mode": "chatgpt",
        "last_refresh": (chrono::Utc::now() - chrono::Duration::days(days_ago)).to_rfc3339(),
        "tokens": {"account_id": "acc", "refresh_token": token},
    })
    .to_string()
}

fn candidate(env: &Env) -> Cand {
    let home = env.tmp.path().join("fakehome").canonicalize_or_create();
    fs::create_dir_all(home.join(".codex")).unwrap();
    let source = home.join(".codex/auth.json");
    let (old, new) = (login_doc(5, OLD_TOKEN), login_doc(0, NEW_TOKEN));
    fs::write(&source, &old).unwrap();
    let meta = tmgr_runner::seedguard::Meta {
        root: home.clone(),
        source: source.clone(),
        sha: tmgr_runner::seedguard::sha256_hex(old.as_bytes()),
        source_sha: tmgr_runner::seedguard::sha256_hex(old.as_bytes()),
        session: "s_abcdefghij".into(),
        created_ms: tmgr_runner::wire::now_ms(),
        json_key: Some("last_refresh".into()),
        same: vec!["tokens.account_id".into(), "auth_mode".into()],
    };
    tmgr_runner::seedguard::store(&env.run, "codex", 0, &meta, new.as_bytes()).unwrap();
    Cand {
        home,
        source,
        old,
        new,
    }
}

trait CanonicalOrCreate {
    fn canonicalize_or_create(self) -> PathBuf;
}

impl CanonicalOrCreate for PathBuf {
    fn canonicalize_or_create(self) -> PathBuf {
        private_dir(&self);
        self.canonicalize().unwrap()
    }
}

fn out_text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn no_tokens(text: &str) {
    for frag in [OLD_TOKEN, NEW_TOKEN, "FAKEOLD1234", "FAKENEW5678"] {
        assert!(!text.contains(frag), "output leaks a token fragment");
    }
}

fn candidate_files(env: &Env) -> bool {
    env.run.join("candidates/codex/0.json").exists()
}

#[test]
fn agents_warns_about_a_pending_refreshed_login() {
    let env = Env::new();
    let c = candidate(&env);
    let mut cmd = env.cmd(&["agents"]);
    cmd.env("HOME", &c.home);
    let out = out_text(&run_bounded(cmd, Duration::from_secs(60)));
    assert!(
        out.contains(
            "codex refreshed its login inside a session; run `tmgr agents sync codex` to apply it to ~/.codex/auth.json"
        ),
        "{out}"
    );
    no_tokens(&out);
    let json = out_text(&env.tmgr(&["agents", "--json"]));
    assert!(!json.contains("refreshed its login"));
}

#[test]
fn sync_without_a_terminal_is_refused_and_discard_still_works() {
    let env = Env::new();
    let c = candidate(&env);
    let out = env.tmgr(&["agents", "sync", "codex"]);
    assert_eq!(out.status.code(), Some(10), "{}", out_text(&out));
    no_tokens(&out_text(&out));
    let out = env.tmgr(&["agents", "sync", "codex", "--json"]);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(fs::read_to_string(&c.source).unwrap(), c.old);
    assert!(candidate_files(&env));
    let out = env.tmgr(&["agents", "sync", "codex", "--discard"]);
    assert!(out.status.success(), "{}", out_text(&out));
    assert!(!candidate_files(&env));
    assert_eq!(fs::read_to_string(&c.source).unwrap(), c.old);
}

#[test]
fn sync_in_a_terminal_shows_a_value_free_summary_and_applies_only_after_apply() {
    let env = Env::new();
    let c = candidate(&env);
    let mut pty = env.pty(&["agents", "sync", "codex"]);
    pty.wait_for("Type apply");
    let shown = pty.text();
    assert!(shown.contains("~/.codex/auth.json") || shown.contains(".codex/auth.json"));
    assert!(shown.contains("same tokens.account_id: yes") && shown.contains("s_abcdefghij"));
    no_tokens(&shown);
    pty.type_bytes(b"yes\r");
    assert!(pty.wait_exit_quiet(Duration::from_secs(10)));
    assert_eq!(fs::read_to_string(&c.source).unwrap(), c.old);
    assert!(candidate_files(&env));

    let mut pty = env.pty(&["agents", "sync", "codex"]);
    pty.wait_for("Type apply");
    pty.type_bytes(b"apply\r");
    assert!(pty.wait_exit(Duration::from_secs(10)));
    assert!(pty.text().contains("applied"));
    no_tokens(&pty.text());
    assert_eq!(fs::read_to_string(&c.source).unwrap(), c.new);
    let backup = c.home.join(".codex/auth.json.tmgr-backup");
    assert_eq!(fs::read_to_string(&backup).unwrap(), c.old);
    let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!((mode(&backup), mode(&c.source)), (0o600, 0o600));
    assert!(!candidate_files(&env));
}

#[test]
fn auto_mode_needs_a_terminal_and_the_exact_agreement() {
    let env = Env::new();
    let flag = env.tmp.path().join("cfg/agents/codex.autosync");
    let out = env.tmgr(&["agents", "sync", "codex", "--auto", "on"]);
    assert_eq!(out.status.code(), Some(10), "{}", out_text(&out));
    assert!(!flag.exists());

    let mut pty = env.pty(&["agents", "sync", "codex", "--auto", "on"]);
    pty.wait_for("Type I AGREE");
    assert!(pty.text().contains("another account's tokens"));
    pty.type_bytes(b"i agree\r");
    assert!(pty.wait_exit_quiet(Duration::from_secs(10)));
    assert!(!flag.exists());

    let mut pty = env.pty(&["agents", "sync", "codex", "--auto", "on"]);
    pty.wait_for("Type I AGREE");
    pty.type_bytes(b"I AGREE\r");
    assert!(pty.wait_exit(Duration::from_secs(10)));
    assert!(flag.exists());
    assert_eq!(
        fs::metadata(&flag).unwrap().permissions().mode() & 0o777,
        0o600
    );

    let out = env.tmgr(&["agents", "sync", "codex", "--auto", "off"]);
    assert!(out.status.success() && !flag.exists());
}

#[test]
fn sync_and_auto_are_refused_inside_a_session() {
    let env = Env::new();
    let c = candidate(&env);
    for args in [
        vec!["agents", "sync", "codex"],
        vec!["agents", "sync", "codex", "--auto", "on"],
    ] {
        let mut cmd = env.cmd(&args);
        cmd.env("TMGR_SESSION_ID", "s_abcdefghij");
        let out = run_bounded(cmd, Duration::from_secs(60));
        assert_eq!(out.status.code(), Some(5), "{args:?}: {}", out_text(&out));
    }
    assert_eq!(fs::read_to_string(&c.source).unwrap(), c.old);
    assert!(!env.tmp.path().join("cfg/agents/codex.autosync").exists());
}

#[test]
fn runner_serve_started_directly_drops_the_shell_environment() {
    let env = Env::new();
    let mut c = env.cmd(&["runner", "serve"]);
    c.env_clear()
        .envs(env.envs())
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", env.tmp.path())
        .env("SHELL_EXPORTED_TOKEN_FOR_TEST", "s3cr3t-shell-value")
        .env("TMGR_NOTIFY_TOKEN", "s3cr3t-notify-value")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = c.spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !env.run.join("runner.pid").exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    let out = Command::new("ps")
        .args(["eww", "-o", "command=", "-p", &child.id().to_string()])
        .output()
        .unwrap();
    let seen = String::from_utf8_lossy(&out.stdout).into_owned();
    let _ = env.tmgr(&["runner", "stop"]);
    let _ = child.kill();
    let _ = child.wait();
    assert!(
        seen.contains("TMGR_RUNNER_DIR"),
        "runner env lacks TMGR_RUNNER_DIR"
    );
    assert!(
        !seen.contains("s3cr3t-shell-value"),
        "shell variable reached the runner"
    );
    assert!(
        !seen.contains("s3cr3t-notify-value"),
        "TMGR_NOTIFY_TOKEN reached the runner"
    );
}

fn git_cli(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .args(["-c", "core.hooksPath=/dev/null"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?}");
}

#[test]
fn agents_log_shows_time_branch_and_commits() {
    let env = Env::new();
    git_cli(&env.proj, &["init", "-q", "-b", "main"]);
    fs::write(env.proj.join("a.txt"), "a").unwrap();
    git_cli(&env.proj, &["add", "--", "a.txt"]);
    git_cli(&env.proj, &["commit", "-q", "-m", "initial"]);
    let script = env.script("worker.sh", "echo start; sleep 2.5; echo end\n");
    let Some(id) = env.start(&script, "worker") else {
        return;
    };
    fs::write(env.proj.join("b.txt"), "b").unwrap();
    git_cli(&env.proj, &["add", "--", "b.txt"]);
    git_cli(&env.proj, &["commit", "-q", "-m", "feat: add b"]);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let w = env.data(&["agents", "log", "worker", "--json"]);
        if w["summary"]["ended_ms"].is_u64() {
            assert_eq!(w["id"], id.as_str());
            let s = &w["summary"];
            assert_eq!(s["branch"], "main");
            assert_eq!(s["commits"][0]["subject"], "feat: add b");
            assert_eq!(s["exit"]["code"], 0);
            assert!(s["duration_s"].as_u64().unwrap() >= 2, "{s}");
            for key in ["working_s", "waiting_s", "blocked_s", "started_ms"] {
                assert!(s[key].is_u64(), "{key}");
            }
            break;
        }
        assert!(Instant::now() < deadline, "the session did not exit");
        thread::sleep(Duration::from_millis(200));
    }
    let human = env.tmgr(&["agents", "log", &id]);
    let text = String::from_utf8_lossy(&human.stdout).into_owned();
    assert!(human.status.success(), "{text}");
    for needle in [
        "worker",
        "total: working",
        "branch: main",
        "feat: add b",
        "exit code: 0",
    ] {
        assert!(text.contains(needle), "{needle}: {text}");
    }
    assert_eq!(
        env.tmgr(&["agents", "log", "missing"]).status.code(),
        Some(3)
    );
}
