#![cfg(feature = "runner")]

use portable_pty::{native_pty_system, Child, CommandBuilder, PtySize};
use serde_json::Value;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

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
    let child = cmd
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id() as i32;
    let (tx, rx) = std::sync::mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(limit) {
        Ok(out) => out.unwrap(),
        Err(_) => {
            unsafe { libc::kill(pid, libc::SIGKILL) };
            panic!("command did not finish within {limit:?}");
        }
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
        kill_tree(&self.run);
    }
}

fn kill_tree(run: &Path) {
    let kill = |pid: i64| {
        if pid > 1 {
            unsafe {
                libc::kill(-(pid as i32), libc::SIGKILL);
                libc::kill(pid as i32, libc::SIGKILL);
            }
        }
    };
    let pid_file = |p: PathBuf| {
        fs::read_to_string(p)
            .ok()
            .and_then(|s| s.trim().parse::<i64>().ok())
    };
    if let Some(pid) = pid_file(run.join("runner.pid")) {
        kill(pid);
    }
    let Ok(rd) = fs::read_dir(run.join("sessions")) else {
        return;
    };
    for e in rd.flatten() {
        let meta: Value = fs::read(e.path().join("meta.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        for k in ["child_pid", "shim_pid"] {
            if let Some(p) = meta[k].as_i64() {
                kill(p);
            }
        }
        if let Some(p) = pid_file(e.path().join("shim.pid")) {
            kill(p);
        }
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
    let pid = env.data(&["runner", "status", "--json"])["pid"]
        .as_i64()
        .unwrap() as i32;
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
fn agents_login_file_creates_a_private_template_and_never_overwrites() {
    let env = Env::new();
    let home = env.tmp.path().join("fakehome");
    private_dir(&home);
    let file = home.join(".config/tmgr/agents/claude-code.env");
    let run = |args: &[&str]| {
        let mut c = env.cmd(args);
        c.env("HOME", &home);
        run_bounded(c, Duration::from_secs(60))
    };

    let out = run(&["agents", "login-file", "claude-code", "--json"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["created"], true);
    assert!(v["data"]["path"]
        .as_str()
        .unwrap()
        .ends_with("fakehome/.config/tmgr/agents/claude-code.env"));
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "CLAUDE_CODE_OAUTH_TOKEN=\n"
    );
    assert_eq!(
        fs::metadata(&file).unwrap().permissions().mode() & 0o777,
        0o600
    );

    fs::write(&file, "CLAUDE_CODE_OAUTH_TOKEN=keep-me\n").unwrap();
    let out = run(&["agents", "login-file", "claude-code"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success() && text.contains("already exists"),
        "{text}"
    );
    assert!(!text.contains("keep-me"));
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "CLAUDE_CODE_OAUTH_TOKEN=keep-me\n"
    );

    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    let out = run(&["agents", "login-file", "claude-code"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("chmod 600"));

    assert!(!run(&["agents", "login-file", "no-such"]).status.success());
}
