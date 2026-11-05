#![cfg(all(unix, feature = "runner"))]

use portable_pty::{native_pty_system, Child, CommandBuilder, PtySize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tempfile::TempDir;

const BIN: &str = env!("CARGO_BIN_EXE_tmgr");
const WAIT: Duration = Duration::from_secs(15);
const DOWN: &str = "\x1b[B";
const ENTER: &str = "\r";
const C1: &str = "11111111-0000-4000-8000-000000000001";
const C2: &str = "11111111-0000-4000-8000-000000000002";
const C3: &str = "11111111-0000-4000-8000-000000000003";
const X1: &str = "22222222-0000-4000-8000-000000000001";
const X2: &str = "22222222-0000-4000-8000-000000000002";
const CLAUDE_AGENT: &str =
    "id = \"claude-code\"\nname = \"fake\"\ncommand = [\"/bin/sh\", \"SCRIPT\"]\n\
    [permission_modes]\nask = [\"--ask\"]\nbypass = [\"--yolo\"]\n\
    [resume]\nargs = [\"--continue\"]\nby_id = [\"--resume\", \"{resume_id}\"]\n\
    fork_by_id = [\"--resume\", \"{resume_id}\", \"--fork-session\"]\n";
const CODEX_AGENT: &str = "id = \"codex\"\nname = \"fake\"\ncommand = [\"/bin/sh\", \"SCRIPT\"]\n\
    [permission_modes]\nask = [\"--ask\"]\nbypass = [\"--yolo\"]\n\
    [resume]\nargs = [\"resume\", \"--last\"]\nat = \"after_command\"\n\
    by_id = [\"resume\", \"{resume_id}\"]\nfork_by_id = [\"fork\", \"{resume_id}\"]\n";

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

    fn envs(&self) -> Vec<(&'static str, String)> {
        vec![
            ("HOME", self.path("home")),
            ("XDG_CONFIG_HOME", self.path("xdg")),
            ("XDG_STATE_HOME", self.path("state")),
            ("XDG_DATA_HOME", self.path("data")),
            ("TMGR_USAGE_DIR", self.path("usage")),
            ("TMGR_CREDENTIAL_STORE", "file".into()),
            ("TMGR_SERVER", "http://127.0.0.1:9".into()),
            ("TMGR_LOCAL_ROOT", self.path("local")),
            ("TMGR_AGENTS_DIR", self.path("agents")),
            ("TMGR_RUNNER_DIR", self.path("runner")),
            ("NO_COLOR", "1".into()),
            ("TERM", "xterm-256color".into()),
            ("PATH", format!("{}:/usr/bin:/bin", self.path("bin"))),
        ]
    }

    fn work(&self) -> PathBuf {
        std::fs::canonicalize(self.tmp.path().join("work")).unwrap()
    }

    fn seed(&self) {
        let agents = self.tmp.path().join("agents");
        std::fs::create_dir_all(&agents).unwrap();
        std::fs::write(self.tmp.path().join("agent.sh"), "sleep 60\n").unwrap();
        let script = self.path("agent.sh");
        for (name, body) in [
            ("claude-code.toml", CLAUDE_AGENT),
            ("codex.toml", CODEX_AGENT),
        ] {
            std::fs::write(agents.join(name), body.replace("SCRIPT", &script)).unwrap();
        }
        let consent = std::process::Command::new(BIN)
            .args(["usage", "consent", "allow"])
            .env_clear()
            .envs(self.envs())
            .stdin(std::process::Stdio::null())
            .output()
            .unwrap();
        assert!(consent.status.success());
        let cwd = self.work().display().to_string();
        let hostile = "\u{1b}]0;PWNED\u{7}\u{202e}";
        self.claude(
            C1,
            &cwd,
            &format!("alpha title{hostile}"),
            "the secret word is zanzibarquux",
        );
        self.claude(C2, &cwd, "beta title", "nothing special here");
        self.claude(
            C3,
            &format!("/gone{hostile}/folder"),
            "gamma title",
            "third body",
        );
        self.codex(X1, &cwd, "widgets session", "the gadgetron answer");
        self.codex(X2, &cwd, "sprockets session", "nothing special here");
    }

    fn put(&self, path: &Path, lines: &[String]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, lines.join("\n") + "\n").unwrap();
        let f = std::fs::File::options().write(true).open(path).unwrap();
        f.set_modified(std::time::SystemTime::now() - Duration::from_secs(3600))
            .unwrap();
    }

    fn claude(&self, id: &str, cwd: &str, title: &str, body: &str) {
        let user = json!({"type":"user","timestamp":"2026-11-05T10:00:00Z","cwd":cwd,
            "message":{"content":title}});
        let reply = json!({"type":"assistant","message":{"content":[{"type":"text","text":body}]}});
        self.put(
            &self
                .tmp
                .path()
                .join(format!("home/.claude/projects/-w/{id}.jsonl")),
            &[user.to_string(), reply.to_string()],
        );
    }

    fn codex(&self, id: &str, cwd: &str, title: &str, body: &str) {
        let meta = json!({"type":"session_meta","payload":{"id":id,
            "timestamp":"2026-11-05T10:00:00Z","cwd":cwd,"source":"cli"}});
        let user = json!({"type":"event_msg","payload":{"type":"user_message","message":title}});
        let reply = json!({"type":"event_msg","payload":{"type":"agent_message","message":body}});
        self.put(
            &self.tmp.path().join(format!(
                "home/.codex/sessions/2026/11/05/rollout-2026-11-05T10-00-00-{id}.jsonl"
            )),
            &[meta.to_string(), user.to_string(), reply.to_string()],
        );
    }

    fn fingerprint(&self) -> BTreeMap<PathBuf, Vec<u8>> {
        let mut out = BTreeMap::new();
        let mut stack = vec![self.tmp.path().join("home")];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else {
                    out.insert(
                        p.clone(),
                        Sha256::digest(std::fs::read(&p).unwrap()).to_vec(),
                    );
                }
            }
        }
        out
    }

    fn index_file(&self) -> PathBuf {
        self.tmp.path().join("xdg/tmgr/sessions-index.sqlite")
    }

    fn pty(&self, args: &[&str]) -> Pty {
        self.pty_with(args, &[])
    }

    fn pty_with(&self, args: &[&str], extra: &[(&str, &str)]) -> Pty {
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
        for (k, v) in self.envs() {
            cmd.env(k, v);
        }
        for (k, v) in extra {
            cmd.env(k, v);
        }
        let child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let screen = Arc::new(Mutex::new(vt100::Parser::new(30, 120, 0)));
        let sink = screen.clone();
        let raw = Arc::new(Mutex::new(Vec::new()));
        let tap = raw.clone();
        thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                tap.lock().unwrap().extend_from_slice(&buf[..n]);
                sink.lock().unwrap().process(&buf[..n]);
            }
        });
        Pty {
            screen,
            raw,
            writer: pair.master.take_writer().unwrap(),
            child,
            _master: pair.master,
        }
    }
}

struct Pty {
    screen: Arc<Mutex<vt100::Parser>>,
    raw: Arc<Mutex<Vec<u8>>>,
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

    fn wait_until(&self, what: &str, ok: impl Fn(&str) -> bool) {
        let end = Instant::now() + WAIT;
        while !ok(&self.text()) {
            assert!(
                Instant::now() < end,
                "timed out waiting for {what}; screen:\n{}",
                self.text()
            );
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn raw(&self) -> String {
        String::from_utf8_lossy(&self.raw.lock().unwrap()).into_owned()
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

fn narrow_to(pty: &mut Pty, word: &str, keep: &str, drop: &str) {
    pty.wait_for(keep);
    pty.wait_for(drop);
    pty.send(word);
    pty.wait_until("the list to narrow", |t| {
        t.contains(keep) && !t.contains(drop)
    });
}

fn resume_a_body_match(
    agent_downs: usize,
    word: &str,
    keep: &str,
    drop: &str,
    id: &str,
    sessions: &str,
) {
    let fx = Fx::new();
    fx.seed();
    let before = fx.fingerprint();
    let mut pty = fx.pty(&["run"]);
    pty.wait_for("Which agent?");
    for _ in 0..agent_downs {
        pty.send(DOWN);
    }
    pty.send(ENTER);
    pty.wait_for(sessions);
    narrow_to(&mut pty, word, keep, drop);
    pty.send(DOWN);
    pty.send(ENTER);
    pty.wait_for("r Resume");
    pty.send("r");
    pty.wait_for("[x] No sandbox (locked)");
    pty.wait_for("Ordinary sessions live in your real home");
    pty.wait_for(&format!("tmgr resume {id} --no-sandbox"));
    pty.send(ENTER);
    pty.wait_for("Type no-sandbox to continue:");
    assert!(pty.text().contains(&fx.work().display().to_string()));
    pty.send("nope\r");
    assert_ne!(pty.exit_code(), 0);
    assert_eq!(fx.fingerprint(), before);
}

#[test]
fn a_claude_session_found_by_a_word_only_in_its_body_resumes_through_the_typed_consent() {
    resume_a_body_match(
        0,
        "zanzibarquux",
        "alpha title",
        "beta title",
        C1,
        "Claude Code sessions",
    );
}

#[test]
fn a_codex_session_found_by_a_word_only_in_its_body_resumes_through_the_typed_consent() {
    resume_a_body_match(
        1,
        "gadgetron",
        "widgets session",
        "sprockets session",
        X1,
        "Codex sessions",
    );
}

#[test]
fn escape_sequences_in_titles_and_paths_never_reach_the_terminal() {
    let fx = Fx::new();
    fx.seed();
    let mut pty = fx.pty(&["run"]);
    pty.wait_for("Which agent?");
    pty.send(ENTER);
    pty.wait_for("alpha title");
    pty.wait_for("gamma title");
    pty.wait_for("(folder gone)");
    let raw = pty.raw();
    assert!(!raw.contains("\u{1b}]0;PWNED"), "{raw:?}");
    assert!(!raw.contains('\u{202e}') && !raw.contains('\u{7}'));
    assert!(pty.text().contains("?]0;PWNED?"));
    pty.send("\x03");
    assert_eq!(pty.exit_code(), 0);
}

#[test]
fn a_folder_gone_session_cannot_be_chosen() {
    let fx = Fx::new();
    fx.seed();
    let mut pty = fx.pty(&["run"]);
    pty.wait_for("Which agent?");
    pty.send(ENTER);
    pty.wait_for("(folder gone)");
    pty.send("gamma");
    pty.wait_until("gamma only", |t| !t.contains("alpha title"));
    pty.send(DOWN);
    pty.wait_for("The session's folder is missing or resolves elsewhere");
    pty.send(ENTER);
    thread::sleep(Duration::from_millis(300));
    assert!(!pty.text().contains("r Resume"));
    assert!(!pty.text().contains("No sandbox"));
    pty.send("\x03");
    assert_eq!(pty.exit_code(), 0);
}

#[test]
fn quitting_while_the_index_updates_leaves_it_consistent() {
    let fx = Fx::new();
    fx.seed();
    let mut pty = fx.pty(&["run"]);
    pty.wait_for("Which agent?");
    pty.send(ENTER);
    pty.wait_for("Claude Code sessions");
    let end = Instant::now() + WAIT;
    while !fx.index_file().exists() {
        assert!(Instant::now() < end, "index was never created");
        thread::sleep(Duration::from_millis(10));
    }
    pty.send("\x03");
    assert_eq!(pty.exit_code(), 0);
    let conn = rusqlite::Connection::open(fx.index_file()).unwrap();
    let verdict: String = conn
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(verdict, "ok");
}

#[test]
fn without_consent_the_menu_filters_by_substring_and_creates_no_index() {
    let fx = Fx::new();
    fx.seed();
    let off = std::process::Command::new(BIN)
        .args(["usage", "consent", "deny"])
        .env_clear()
        .envs(fx.envs())
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(off.status.success());
    let mut pty = fx.pty(&["run"]);
    pty.wait_for("Which agent?");
    pty.send(ENTER);
    pty.wait_for("Reading session logs is off");
    pty.send("zanzibarquux");
    thread::sleep(Duration::from_millis(300));
    assert!(!pty.text().contains("alpha title"));
    pty.send("\x03");
    assert_eq!(pty.exit_code(), 0);
    assert!(!fx.index_file().exists());
}

#[test]
fn a_bare_run_with_an_agent_marker_on_a_terminal_does_not_open_the_menu() {
    let fx = Fx::new();
    let mut pty = fx.pty_with(&["run"], &[("TMGR_RUN_ID", "1")]);
    assert_eq!(pty.exit_code(), 2);
    assert!(!pty.text().contains("Which agent?"));
    assert!(!pty.alternate());
}

#[test]
fn run_with_a_flag_on_a_terminal_does_not_open_the_menu() {
    let fx = Fx::new();
    let mut pty = fx.pty(&["run", "--agent", "nope-agent", "--no-attach"]);
    let code = pty.exit_code();
    assert_ne!(code, 0);
    assert!(!pty.text().contains("Which agent?"));
}

fn wait_for_index(fx: &Fx) {
    let end = Instant::now() + WAIT;
    while !fx.index_file().exists() {
        assert!(Instant::now() < end, "index was never created");
        thread::sleep(Duration::from_millis(10));
    }
    thread::sleep(Duration::from_millis(700));
}

#[test]
fn a_deny_while_the_menu_is_open_removes_the_index_and_it_is_not_recreated() {
    let fx = Fx::new();
    fx.seed();
    let mut pty = fx.pty(&["run"]);
    pty.wait_for("Which agent?");
    pty.send(ENTER);
    pty.wait_for("alpha title");
    wait_for_index(&fx);
    let deny = std::process::Command::new(BIN)
        .args(["usage", "consent", "deny"])
        .env_clear()
        .envs(fx.envs())
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(deny.status.success());
    pty.send("zanzibarquux");
    thread::sleep(Duration::from_millis(800));
    assert!(!pty.text().contains("alpha title"));
    assert!(!fx.index_file().exists());
    pty.send("\x03");
    assert_eq!(pty.exit_code(), 0);
    assert!(!fx.index_file().exists());
}

#[test]
fn typing_stays_responsive_while_the_index_is_write_locked() {
    let fx = Fx::new();
    fx.seed();
    let mut pty = fx.pty(&["run"]);
    pty.wait_for("Which agent?");
    pty.send(ENTER);
    pty.wait_for("alpha title");
    wait_for_index(&fx);
    let holder = rusqlite::Connection::open(fx.index_file()).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    narrow_to(&mut pty, "zanzibarquux", "alpha title", "beta title");
    pty.send("\x1b");
    pty.wait_for("Which agent?");
    pty.send("q");
    assert_eq!(pty.exit_code(), 0);
    drop(holder);
}

#[test]
fn a_typed_a_filters_and_does_not_turn_reading_on() {
    let fx = Fx::new();
    fx.seed();
    let off = std::process::Command::new(BIN)
        .args(["usage", "consent", "deny"])
        .env_clear()
        .envs(fx.envs())
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(off.status.success());
    let mut pty = fx.pty(&["run"]);
    pty.wait_for("Which agent?");
    pty.send(ENTER);
    pty.wait_for("Reading session logs is off");
    pty.send("a");
    pty.wait_for("Filter: a_");
    pty.send("\x7f\t");
    pty.wait_for("Press Tab again to allow reading session logs");
    pty.send("x");
    thread::sleep(Duration::from_millis(300));
    assert!(!pty.text().contains("Reading session logs is on."));
    pty.send("\x03");
    assert_eq!(pty.exit_code(), 0);
    let status = std::process::Command::new(BIN)
        .args(["usage", "consent", "status"])
        .env_clear()
        .envs(fx.envs())
        .stdin(std::process::Stdio::null())
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&status.stdout).contains("off"));
}

#[test]
fn sigterm_restores_the_terminal() {
    let fx = Fx::new();
    let mut pty = fx.pty(&["run"]);
    pty.wait_for("Which agent?");
    assert!(pty.alternate());
    let pid = pty.child.process_id().unwrap();
    unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    assert_eq!(pty.exit_code(), 128 + libc::SIGTERM as u32);
    let end = Instant::now() + WAIT;
    while pty.alternate() {
        assert!(Instant::now() < end, "alternate screen was not left");
        thread::sleep(Duration::from_millis(20));
    }
}
