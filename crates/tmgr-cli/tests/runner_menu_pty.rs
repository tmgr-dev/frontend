#![cfg(all(unix, feature = "runner"))]

use portable_pty::{native_pty_system, Child, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_tmgr");
const WAIT: Duration = Duration::from_secs(15);
const DOWN: &str = "\x1b[B";
const ENTER: &str = "\r";

struct Fx {
    tmp: TempDir,
}

impl Fx {
    fn new() -> Fx {
        let fx = Fx {
            tmp: tempfile::tempdir().unwrap(),
        };
        for d in ["home", "work", "bin"] {
            std::fs::create_dir(fx.tmp.path().join(d)).unwrap();
        }
        let claude = fx.tmp.path().join("bin/claude");
        std::fs::write(&claude, "#!/bin/sh\nexit 0\n").unwrap();
        std::fs::set_permissions(&claude, std::fs::Permissions::from_mode(0o755)).unwrap();
        fx
    }

    fn path(&self, name: &str) -> String {
        self.tmp.path().join(name).to_string_lossy().into_owned()
    }

    fn memory_file(&self) -> std::path::PathBuf {
        self.tmp.path().join("xdg/tmgr/run-menu.toml")
    }

    fn pty(&self, args: &[&str]) -> Pty {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 30,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut cmd = CommandBuilder::new(BIN);
        cmd.args(args);
        cmd.env_clear();
        cmd.cwd(self.path("work"));
        for (k, v) in [
            ("HOME", self.path("home")),
            ("XDG_CONFIG_HOME", self.path("xdg")),
            ("XDG_STATE_HOME", self.path("state")),
            ("XDG_DATA_HOME", self.path("data")),
            ("TMGR_CREDENTIAL_STORE", "file".into()),
            ("TMGR_SERVER", "http://127.0.0.1:9".into()),
            ("TMGR_LOCAL_ROOT", self.path("local")),
            ("TMGR_AGENTS_DIR", self.path("agents")),
            ("TMGR_RUNNER_DIR", self.path("runner")),
            ("NO_COLOR", "1".into()),
            ("TERM", "xterm-256color".into()),
            ("PATH", format!("{}:/usr/bin:/bin", self.path("bin"))),
        ] {
            cmd.env(k, v);
        }
        let child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let screen = Arc::new(Mutex::new(vt100::Parser::new(30, 120, 0)));
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

    fn alternate(&self) -> bool {
        self.screen.lock().unwrap().screen().alternate_screen()
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
fn q_on_the_first_screen_exits_zero_and_restores_the_screen() {
    let fx = Fx::new();
    let mut pty = fx.pty(&["run"]);
    pty.wait_for("Which agent?");
    assert!(pty.alternate());
    pty.wait_for("Codex  (codex) not installed");
    assert!(pty.text().contains("Claude Code  (claude-code)"));
    pty.send("q");
    assert_eq!(pty.exit_code(), 0);
    let end = Instant::now() + WAIT;
    while pty.alternate() {
        assert!(Instant::now() < end, "alternate screen was not left");
        thread::sleep(Duration::from_millis(20));
    }
    assert!(!pty.text().contains("Which agent?"));
    assert!(!fx.memory_file().exists());
}

#[test]
fn walking_to_the_flags_and_pressing_enter_hands_the_argv_to_normal_dispatch() {
    let fx = Fx::new();
    let mut pty = fx.pty(&["run"]);
    pty.wait_for("Which agent?");
    pty.send(ENTER);
    pty.wait_for("Claude Code sessions");
    pty.wait_for("New session");
    pty.send(ENTER);
    pty.wait_for("Folder:");
    pty.send(DOWN);
    pty.send(DOWN);
    pty.send(" ");
    pty.wait_for("[x] No sandbox");
    pty.wait_for("You will be asked to type a confirmation next.");
    pty.wait_for("--no-sandbox");
    assert!(pty.text().contains("tmgr run --agent claude-code --cwd"));
    pty.send(ENTER);
    pty.wait_for("Type no-sandbox to continue:");
    assert!(!pty.alternate());
    assert!(pty.text().contains("starting agent claude-code in"));
    assert!(pty.text().contains("work"));
    pty.send("nope\r");
    assert_ne!(pty.exit_code(), 0);
    let saved = std::fs::read_to_string(fx.memory_file()).unwrap();
    assert!(saved.contains("agent = \"claude-code\""), "{saved}");
    assert!(saved.contains("kind = \"new\""), "{saved}");
    assert!(saved.contains("no_sandbox = true"), "{saved}");
    assert!(saved.contains("bypass = false"), "{saved}");
    let mode = std::fs::metadata(fx.memory_file())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn a_remembered_choice_preselects_but_still_needs_the_typed_confirmation() {
    let fx = Fx::new();
    std::fs::create_dir_all(fx.memory_file().parent().unwrap()).unwrap();
    std::fs::write(
        fx.memory_file(),
        "agent = \"claude-code\"\nkind = \"new\"\nbypass = false\nno_sandbox = true\n",
    )
    .unwrap();
    let mut pty = fx.pty(&["run"]);
    pty.wait_for("Which agent?");
    pty.send(ENTER);
    pty.wait_for("New session");
    pty.send(ENTER);
    pty.wait_for("[x] No sandbox");
    pty.send(ENTER);
    pty.wait_for("Type no-sandbox to continue:");
    pty.send("\x04");
    assert_ne!(pty.exit_code(), 0);
}
