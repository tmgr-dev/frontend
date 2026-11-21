#![cfg(feature = "runner")]

use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde_json::Value;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tmgr_link::store::Paths;
use tmgr_runner::client::Client;

const BIN: &str = env!("CARGO_BIN_EXE_tmgr");
const PIN: &str = "owner-pin-1";

struct Env {
    tmp: tempfile::TempDir,
    home: PathBuf,
    run: PathBuf,
    paths: Paths,
}

fn private(path: &Path) {
    std::fs::create_dir_all(path).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
}

impl Env {
    fn new() -> Env {
        let tmp = tempfile::Builder::new()
            .prefix("tx")
            .tempdir_in("/tmp")
            .unwrap();
        private(tmp.path());
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let home = root.join("home");
        let run = root.join("r");
        private(&home);
        private(&run);
        private(&root.join("homes"));
        let paths = Paths {
            config: home.join(".config").join("tmgr"),
            state: home.join(".local").join("state").join("tmgr"),
            home: home.clone(),
        };
        Env {
            tmp,
            home,
            run,
            paths,
        }
    }

    fn envs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("HOME", self.home.display().to_string()),
            ("TMGR_RUNNER_DIR", self.run.display().to_string()),
            (
                "TMGR_RUNNER_HOMES",
                self.tmp.path().join("homes").display().to_string(),
            ),
            ("TERM", "xterm".into()),
            ("NO_COLOR", "1".into()),
        ]
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
        c.env_clear().args(args).envs(self.envs());
        c
    }

    fn set_pin(&self) {
        tmgr_link::pin::set(&self.paths, PIN).unwrap();
    }

    fn folder(&self, name: &str) -> PathBuf {
        let d = self.tmp.path().join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::canonicalize(d).unwrap()
    }

    fn audit(&self) -> String {
        std::fs::read_dir(self.paths.audit_dir())
            .map(|rd| {
                rd.flatten()
                    .filter_map(|e| std::fs::read_to_string(e.path()).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn runner_started(&self) -> bool {
        Client::connect(&self.run).is_ok()
    }
}

impl Drop for Env {
    fn drop(&mut self) {
        if let Ok(mut c) = Client::connect(&self.run) {
            let _ = c.stop();
        }
    }
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
        let mut cmd = CommandBuilder::new(BIN);
        cmd.args(args);
        cmd.env_clear();
        for (k, v) in env.envs() {
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

    fn text(&self) -> String {
        self.screen.lock().unwrap().clone()
    }

    fn wait_for(&self, needle: &str) {
        let end = Instant::now() + Duration::from_secs(30);
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
        let end = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(s) = self.child.try_wait().unwrap() {
                return s.exit_code();
            }
            assert!(Instant::now() < end, "did not exit: {}", self.text());
            thread::sleep(Duration::from_millis(50));
        }
    }
}

fn data(out: &std::process::Output) -> Value {
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    v["data"].clone()
}

#[test]
fn an_agent_session_cannot_allow_or_deny() {
    let env = Env::new();
    env.set_pin();
    let dir = env.folder("proj");
    let d = dir.to_str().unwrap();
    for var in ["TMGR_RUN_TOKEN", "TMGR_RUN_ID", "TMGR_SESSION_ID"] {
        for sub in ["allow", "deny"] {
            let out = env
                .cmd(&["runner", "objects", sub, d])
                .env(var, "x")
                .output()
                .unwrap();
            assert_eq!(out.status.code(), Some(5), "{var} {sub}");
        }
    }
    assert!(env.audit().is_empty());
    assert!(!env.runner_started());
}

#[test]
fn allow_without_a_terminal_is_refused_and_yes_does_not_help() {
    let env = Env::new();
    env.set_pin();
    let dir = env.folder("proj");
    let out = env
        .cmd(&["runner", "objects", "allow", dir.to_str().unwrap(), "--yes"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10));
    assert!(env.audit().is_empty());
}

#[test]
fn allow_without_a_pin_set_is_refused_and_names_the_command() {
    let env = Env::new();
    let dir = env.folder("proj");
    let mut p = Pty::spawn(&env, &["runner", "objects", "allow", dir.to_str().unwrap()]);
    assert_eq!(p.exit_code(), 5, "{}", p.text());
    assert!(p.text().contains("tmgr link pin set"), "{}", p.text());
    assert!(!p.text().contains("Follower PIN"));
    assert!(env.audit().is_empty());
    assert!(!env.runner_started());
}

#[test]
fn a_wrong_pin_is_refused_counts_toward_the_lockout_and_adds_nothing() {
    let env = Env::new();
    env.set_pin();
    let dir = env.folder("proj");
    let mut p = Pty::spawn(&env, &["runner", "objects", "allow", dir.to_str().unwrap()]);
    p.wait_for("Follower PIN");
    p.type_line("not-the-pin");
    assert_eq!(p.exit_code(), 5, "{}", p.text());
    assert!(p.text().contains("wrong follower PIN"), "{}", p.text());
    assert!(!p.text().contains("not-the-pin"));
    assert_eq!(tmgr_link::pin::status(&env.paths).unwrap().failures, 1);
    assert!(env.audit().is_empty());
    assert!(!env.runner_started());
}

#[test]
fn a_folder_with_a_file_linked_from_outside_is_refused_even_with_the_right_pin() {
    let env = Env::new();
    env.set_pin();
    let dir = env.folder("proj");
    let outside = env.folder("elsewhere").join("shared.pack");
    std::fs::write(&outside, b"x").unwrap();
    std::fs::hard_link(&outside, dir.join("inside.pack")).unwrap();
    let mut p = Pty::spawn(&env, &["runner", "objects", "allow", dir.to_str().unwrap()]);
    p.wait_for("Follower PIN");
    p.type_line(PIN);
    assert_eq!(p.exit_code(), 5, "{}", p.text());
    assert!(p.text().contains("linked from outside"), "{}", p.text());
    assert!(!p.text().contains("Type exempt"));
    assert!(env.audit().is_empty());
    assert!(!env.runner_started());
}

#[test]
fn check_reports_without_a_pin_and_works_inside_an_agent_session() {
    let env = Env::new();
    let clean = env.folder("clean");
    std::fs::write(clean.join("a"), b"x").unwrap();
    let out = env
        .cmd(&[
            "runner",
            "objects",
            "check",
            clean.to_str().unwrap(),
            "--json",
        ])
        .env("TMGR_RUN_TOKEN", "x")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let d = data(&out);
    assert_eq!(d["clean"], true);
    assert_eq!(d["files_seen"], 1);

    let dirty = env.folder("dirty");
    let outside = env.folder("elsewhere").join("f");
    std::fs::write(&outside, b"x").unwrap();
    std::fs::hard_link(&outside, dirty.join("g")).unwrap();
    let out = env
        .cmd(&[
            "runner",
            "objects",
            "check",
            dirty.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let d = data(&out);
    assert_eq!(d["clean"], false);
    assert_eq!(d["outside_count"], 1);
    assert!(env.audit().is_empty());
    assert!(!env.runner_started());
}

#[test]
fn allow_list_and_deny_round_trip_with_audit_entries() {
    let env = Env::new();
    env.set_pin();
    let dir = env.folder("proj");
    std::fs::write(dir.join("a"), b"x").unwrap();
    let d = dir.to_str().unwrap();
    let mut p = Pty::spawn(&env, &["runner", "objects", "allow", d]);
    p.wait_for("Follower PIN");
    p.type_line(PIN);
    p.wait_for("Type exempt");
    assert!(p.text().contains("git clone --no-local"));
    p.type_line("exempt");
    assert_eq!(p.exit_code(), 0, "{}", p.text());
    assert!(env.audit().contains("object_exempt.add"));

    let out = env
        .cmd(&["runner", "objects", "list", "--json"])
        .output()
        .unwrap();
    let rows = data(&out);
    assert_eq!(rows[0]["path"], d);
    assert_eq!(rows[0]["stale"], false);

    let out = env
        .cmd(&["runner", "objects", "deny", d, "--json"])
        .output()
        .unwrap_or_else(|_| panic!("deny"));
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(data(&out)["removed"], true);
    assert!(env.audit().contains("object_exempt.remove"));
    let out = env
        .cmd(&["runner", "objects", "list", "--json"])
        .output()
        .unwrap();
    assert_eq!(data(&out).as_array().unwrap().len(), 0);
}
