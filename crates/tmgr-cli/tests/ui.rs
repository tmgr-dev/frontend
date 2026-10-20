#![cfg(unix)]

use assert_cmd::Command;
use portable_pty::{native_pty_system, Child, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_tmgr");
const WAIT: Duration = Duration::from_secs(15);

const MARKERS: [&str; 6] = [
    "TMGR_RUN_TOKEN",
    "TMGR_RUN_ID",
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "TMGR_AGENT_SESSION",
    "CODEX_SANDBOX_NETWORK_DISABLED",
];

struct Fx {
    tmp: TempDir,
}

impl Fx {
    fn new() -> Fx {
        Fx {
            tmp: tempfile::tempdir().unwrap(),
        }
    }

    fn envs(&self) -> Vec<(&'static str, String)> {
        let t = self.tmp.path();
        let p = |s: &str| t.join(s).to_string_lossy().into_owned();
        vec![
            ("HOME", p("")),
            ("XDG_CONFIG_HOME", p("xdg")),
            ("XDG_STATE_HOME", p("state")),
            ("XDG_DATA_HOME", p("data")),
            ("TMGR_CREDENTIAL_STORE", "file".into()),
            ("TMGR_SERVER", "http://127.0.0.1:9".into()),
            ("TMGR_LOCAL_ROOT", p("local")),
            ("TMGR_AGENTS_DIR", p("agents")),
            ("TMGR_RUNNER_DIR", p("runner")),
            ("NO_COLOR", "1".into()),
            ("TERM", "xterm-256color".into()),
            (
                "PATH",
                std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".into()),
            ),
        ]
    }

    fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(BIN);
        c.env_clear().current_dir(self.tmp.path());
        for (k, v) in self.envs() {
            c.env(k, v);
        }
        c.args(args).write_stdin("").timeout(WAIT);
        c
    }

    fn pty(&self, args: &[&str], extra: &[(&str, &str)]) -> Pty {
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
        for (k, v) in extra {
            cmd.env(k, v);
        }
        let child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let screen = Arc::new(Mutex::new(vt100::Parser::new(40, 140, 0)));
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
        let end = Instant::now() + WAIT;
        while !self.text().contains(needle) {
            assert!(
                Instant::now() < end,
                "timed out waiting for {needle:?}; screen:\n{}",
                self.text()
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn send(&mut self, s: &str) {
        self.writer.write_all(s.as_bytes()).unwrap();
        self.writer.flush().unwrap();
    }

    fn exit_code(&mut self) -> u32 {
        let end = Instant::now() + WAIT;
        loop {
            if let Some(st) = self.child.try_wait().unwrap() {
                return st.exit_code();
            }
            assert!(
                Instant::now() < end,
                "process did not exit; screen:\n{}",
                self.text()
            );
            thread::sleep(Duration::from_millis(20));
        }
    }
}

#[test]
fn bare_without_tty_matches_status() {
    let fx = Fx::new();
    let bare = fx.cmd(&[]).output().unwrap();
    let status = fx.cmd(&["status"]).output().unwrap();
    assert_eq!(bare.status.code(), status.status.code());
    assert_eq!(bare.stdout, status.stdout);
    assert_eq!(bare.stderr, status.stderr);
    assert!(!String::from_utf8_lossy(&bare.stdout).contains("Commands"));
}

#[test]
fn bare_in_a_pty_opens_the_menu_and_q_quits() {
    let fx = Fx::new();
    let mut pty = fx.pty(&[], &[]);
    pty.wait_for("Commands");
    pty.wait_for("Tasks");
    pty.send("q");
    assert_eq!(pty.exit_code(), 0);
}

#[test]
fn bare_in_a_pty_with_an_agent_marker_skips_the_menu() {
    for marker in MARKERS {
        let fx = Fx::new();
        let mut pty = fx.pty(&[], &[(marker, "1")]);
        pty.exit_code();
        assert!(
            !pty.text().contains("Commands"),
            "{marker} opened the menu:\n{}",
            pty.text()
        );
    }
}

#[test]
fn ui_without_a_tty_exits_10() {
    let fx = Fx::new();
    fx.cmd(&["ui"]).assert().code(10);
}

#[test]
fn ui_in_a_pty_with_an_agent_marker_exits_5() {
    for marker in MARKERS {
        let fx = Fx::new();
        let mut pty = fx.pty(&["ui"], &[(marker, "1")]);
        assert_eq!(pty.exit_code(), 5, "{marker}");
        assert!(!pty.text().contains("Commands"), "{marker}");
    }
}

#[test]
fn non_bare_invocations_never_open_the_menu() {
    for args in [&["--json"][..], &["--ws", "x"][..]] {
        let fx = Fx::new();
        let mut pty = fx.pty(args, &[]);
        pty.exit_code();
        assert!(
            !pty.text().contains("Commands"),
            "{args:?} opened the menu:\n{}",
            pty.text()
        );
    }
}

#[test]
fn guarded_command_through_the_menu_keeps_its_typed_confirmation() {
    let fx = Fx::new();
    let mut pty = fx.pty(&[], &[]);
    pty.wait_for("Commands");
    pty.send("/");
    pty.wait_for("Command palette");
    pty.send("queue enable");
    pty.wait_for("queue enable");
    pty.send("\r");
    pty.wait_for("tmgr queue enable");
    pty.send("\r");
    pty.wait_for("enter: continue");
    let shown = pty.text();
    assert!(
        !shown.contains("--yes") && !shown.contains(" -y"),
        "{shown}"
    );
    pty.send("\r");
    pty.wait_for("Type I AGREE to continue:");
    assert!(!pty.text().contains("--yes"));
    pty.send("nope\r");
    pty.wait_for("did not match");
    pty.wait_for("nothing was changed");
    pty.wait_for("Press Enter to return to the menu.");
    pty.send("\r");
    pty.wait_for("Same command next time");
    let done = pty.text();
    assert!(done.contains("queue enable"), "{done}");
    assert!(!done.contains("--yes") && !done.contains(" -y"), "{done}");
    pty.send("q");
    assert_eq!(pty.exit_code(), 0);
}
