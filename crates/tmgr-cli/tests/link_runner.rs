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
use tmgr_core::creds::Backend;
use tmgr_link::authkeys;
use tmgr_link::grammar;
use tmgr_link::leader;
use tmgr_link::scope::Scope;
use tmgr_link::store::{Follower, Leader, Paths, Store};

const BIN: &str = env!("CARGO_BIN_EXE_tmgr");
const ENR: &str = "0123456789abcdef0123456789abcdef";

const AGENT: &str = "printf '\\033]52;c;aGk=\\007\\033]0;owned\\007\\033[2J\\033[31mEVIL\\033[0m\\n'\necho READY\nwhile read l; do echo \"echo:$l\"; done\n";

struct Guard(std::process::Child);

impl Drop for Guard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn private(path: &Path) {
    fs::create_dir_all(path).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn keygen(path: &Path) {
    let ok = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(path)
        .status()
        .unwrap()
        .success();
    assert!(ok);
}

struct Rig {
    _tmp: tempfile::TempDir,
    root: PathBuf,
    repo: PathBuf,
    run_dir: PathBuf,
    fp: Paths,
    _sshd: Guard,
    _runner: Guard,
}

#[derive(Clone, Copy)]
enum Side {
    Leader,
    Follower,
}

impl Rig {
    fn env(&self, side: Side) -> Vec<(&'static str, String)> {
        let (cfg, state, home) = match side {
            Side::Leader => ("lcfg", "lstate", "lhome"),
            Side::Follower => ("fcfg", "fstate", "fhome"),
        };
        let mut v = vec![
            ("HOME", self.root.join(home).display().to_string()),
            ("XDG_CONFIG_HOME", self.root.join(cfg).display().to_string()),
            (
                "XDG_STATE_HOME",
                self.root.join(state).display().to_string(),
            ),
            ("TMGR_CREDENTIAL_STORE", "file".into()),
            ("TMGR_RUNNER_DIR", self.run_dir.display().to_string()),
            (
                "TMGR_RUNNER_HOMES",
                self.root.join("homes").display().to_string(),
            ),
            (
                "TMGR_AGENTS_DIR",
                self.root.join("agents").display().to_string(),
            ),
            ("TMGR_T0_QUIET_MS", "300".into()),
            ("NO_COLOR", "1".into()),
            ("TERM", "xterm-256color".into()),
        ];
        v.retain(|(k, _)| !(matches!(side, Side::Leader) && *k == "TMGR_RUNNER_DIR"));
        v
    }

    fn tmgr(&self, side: Side, args: &[&str]) -> Output {
        let mut c = Command::new(BIN);
        c.args(args)
            .env_remove("TMGR_RUN_TOKEN")
            .env_remove("TMGR_RUN_ID")
            .env_remove("XDG_RUNTIME_DIR")
            .env_remove("TMGR_RUNNER_DIR")
            .stdin(Stdio::null());
        for (k, v) in self.env(side) {
            c.env(k, v);
        }
        c.output().unwrap()
    }

    fn data(&self, side: Side, args: &[&str]) -> Value {
        let out = self.tmgr(side, args);
        let v: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
            panic!(
                "{args:?}: {e}: {}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        });
        assert_eq!(v["ok"], true, "{args:?}: {v}");
        v["data"].clone()
    }

    fn pty(&self, side: Side, args: &[&str]) -> Pty {
        Pty::spawn(self.env(side), args)
    }

    fn set_scope(&self, f: impl FnOnce(&mut Scope)) {
        Store::update(&self.fp, |s| {
            f(&mut s.leaders.get_mut("laptop").unwrap().scope);
            Ok(())
        })
        .unwrap();
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let mut c = Command::new(BIN);
        c.args(["runner", "stop"])
            .env_remove("XDG_RUNTIME_DIR")
            .env("HOME", self.root.join("fhome"))
            .env("XDG_CONFIG_HOME", self.root.join("fcfg"))
            .env("XDG_STATE_HOME", self.root.join("fstate"))
            .env("TMGR_CREDENTIAL_STORE", "file")
            .env("TMGR_RUNNER_DIR", &self.run_dir)
            .stdin(Stdio::null());
        let _ = c.output();
        let Ok(rd) = fs::read_dir(self.run_dir.join("sessions")) else {
            return;
        };
        for e in rd.flatten() {
            let meta = fs::read(e.path().join("meta.json")).unwrap_or_default();
            let v: Value = serde_json::from_slice(&meta).unwrap_or_default();
            for k in ["child_pid", "shim_pid"] {
                if let Some(p) = v[k].as_i64().filter(|p| *p > 1) {
                    unsafe {
                        libc::kill(-(p as i32), libc::SIGKILL);
                        libc::kill(p as i32, libc::SIGKILL);
                    }
                }
            }
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

impl Pty {
    fn spawn(env: Vec<(&'static str, String)>, args: &[&str]) -> Pty {
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
        cmd.env_remove("XDG_RUNTIME_DIR");
        cmd.env_remove("TMGR_RUNNER_DIR");
        for (k, v) in env {
            cmd.env(k, v);
        }
        let child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let screen = Arc::new(Mutex::new(vt100::Parser::new(30, 100, 0)));
        let raw = Arc::new(Mutex::new(Vec::new()));
        let (sink, tap) = (screen.clone(), raw.clone());
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
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn type_bytes(&mut self, bytes: &[u8]) {
        self.writer.write_all(bytes).unwrap();
        self.writer.flush().unwrap();
        thread::sleep(Duration::from_millis(150));
    }

    fn exited(&mut self, within: Duration) -> bool {
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
        assert!(self.exited(Duration::from_secs(5)), "{}", self.text());
    }

    fn assert_no_escape_beyond_the_allowlist(&self) {
        let raw = self.raw.lock().unwrap();
        for bad in [
            &b"\x1b]"[..],
            b"\x1bP",
            b"\x1b^",
            b"\x1b_",
            b"\x1bX",
            &[0x9b],
            &[0x9d],
        ] {
            assert!(
                !raw.windows(bad.len()).any(|w| w == bad),
                "found {bad:?} in the leader's terminal output"
            );
        }
    }
}

fn rig() -> Option<Rig> {
    if std::env::var("TMGR_LINK_SSHD_TEST").as_deref() != Ok("1") {
        eprintln!("skipped: set TMGR_LINK_SSHD_TEST=1 to run against a local sshd");
        return None;
    }
    if tmgr_runner::daemon::landlock_abi() < 6 {
        eprintln!("skipped: the runner sandbox is unavailable here");
        return None;
    }
    let tmp = tempfile::Builder::new()
        .prefix("lk")
        .tempdir_in("/tmp")
        .unwrap();
    let root = fs::canonicalize(tmp.path()).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let repo = root.join("repo");
    let run_dir = root.join("r");
    for d in ["repo", "r", "homes", "agents", "lhome", "fhome"] {
        private(&root.join(d));
    }
    let agent = repo.join("agent.sh");
    fs::write(&agent, format!("#!/bin/sh\n{AGENT}")).unwrap();
    fs::set_permissions(&agent, fs::Permissions::from_mode(0o755)).unwrap();
    fs::write(
        root.join("agents").join("fakeagent.toml"),
        format!(
            "id = \"fakeagent\"\nname = \"fakeagent\"\ncommand = [{:?}]\n",
            agent.display().to_string()
        ),
    )
    .unwrap();

    let lp = Paths {
        config: root.join("lcfg").join("tmgr"),
        state: root.join("lstate").join("tmgr"),
        home: root.join("lhome"),
    };
    let fp = Paths {
        config: root.join("fcfg").join("tmgr"),
        state: root.join("fstate").join("tmgr"),
        home: root.join("fhome"),
    };
    let cred = leader::credential_store(&lp, Some(Backend::File));
    let id = leader::init(&lp, &cred, false).unwrap();
    let key = grammar::parse_pubkey(&id.public).unwrap();

    let bin = fs::canonicalize(BIN).unwrap();
    let authorized = root.join("authorized_keys");
    let line = authkeys::build_line(&bin, "laptop", ENR, &["127.0.0.1".to_string()], &key).unwrap();
    authkeys::upsert(&authorized, "laptop", &line, &key).unwrap();
    Store::update(&fp, |s| {
        s.leaders.insert(
            "laptop".into(),
            Leader {
                key: key.blob.clone(),
                fingerprint: key.fingerprint.clone(),
                added: "2026-10-13T00:00:00Z".into(),
                enrollment: ENR.into(),
                authorized_keys: authorized.display().to_string(),
                scope: Scope {
                    repos: vec![repo.display().to_string()],
                    agents: vec!["fakeagent".into()],
                    models: vec![],
                    max: 2,
                    from: vec!["127.0.0.1".into()],
                    allow_attach_input: false,
                },
            },
        );
        Ok(())
    })
    .unwrap();

    let host_key = root.join("host_key");
    keygen(&host_key);
    let port = free_port();
    let me = std::env::var("USER").expect("USER");
    let cfg = root.join("sshd_config");
    fs::write(
        &cfg,
        format!(
            "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nPidFile {}\nAuthorizedKeysFile {}\nStrictModes no\nUsePAM no\nPermitUserEnvironment no\nAllowUsers {me}\nPubkeyAuthentication yes\nPasswordAuthentication no\nSetEnv XDG_CONFIG_HOME={} XDG_STATE_HOME={} TMGR_RUNNER_DIR={}\n",
            host_key.display(),
            root.join("sshd.pid").display(),
            authorized.display(),
            root.join("fcfg").display(),
            root.join("fstate").display(),
            run_dir.display(),
        ),
    )
    .unwrap();
    let sshd = Guard(
        Command::new("/usr/sbin/sshd")
            .args(["-D", "-e", "-f"])
            .arg(&cfg)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("sshd"),
    );
    for _ in 0..50 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let pinned = leader::scan_host("127.0.0.1", port).unwrap();
    leader::pin_host(&lp, "spark", &pinned).unwrap();
    Store::update(&lp, |s| {
        s.followers.insert(
            "spark".into(),
            Follower {
                user: me.clone(),
                host: "127.0.0.1".into(),
                port,
                host_key_fingerprint: pinned.fingerprint.clone(),
                added: "2026-10-13T00:00:00Z".into(),
            },
        );
        Ok(())
    })
    .unwrap();

    let mut runner_cmd = Command::new(BIN);
    runner_cmd
        .args(["runner", "serve"])
        .env_remove("XDG_RUNTIME_DIR")
        .env("XDG_CONFIG_HOME", root.join("fcfg"))
        .env("XDG_STATE_HOME", root.join("fstate"))
        .env("TMGR_CREDENTIAL_STORE", "file")
        .env("TMGR_RUNNER_DIR", &run_dir)
        .env("TMGR_RUNNER_HOMES", root.join("homes"))
        .env("TMGR_AGENTS_DIR", root.join("agents"))
        .env("TMGR_T0_QUIET_MS", "300")
        .env("HOME", root.join("fhome"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(fs::File::create(root.join("runner.err")).unwrap());
    let runner = Guard(runner_cmd.spawn().unwrap());
    let end = Instant::now() + Duration::from_secs(15);
    while tmgr_link::runner::RunnerClient::connect(&run_dir).is_err() {
        assert!(Instant::now() < end, "the runner did not start");
        thread::sleep(Duration::from_millis(50));
    }
    Some(Rig {
        _tmp: tmp,
        root,
        repo,
        run_dir,
        fp,
        _sshd: sshd,
        _runner: runner,
    })
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn a_leader_runs_attaches_and_is_cut_off_through_a_real_sshd_and_a_real_runner() {
    let Some(rig) = rig() else { return };

    let started = rig.data(
        Side::Leader,
        &[
            "run",
            "w1:1",
            "--on",
            "spark",
            "--agent",
            "fakeagent",
            "--json",
        ],
    );
    let run = started["run"]["id"].as_str().unwrap().to_string();
    assert_eq!(started["run"]["task"], "w1:1");
    assert!(started["run"].get("argv").is_none());

    let local = rig.data(
        Side::Follower,
        &[
            "run",
            "--agent",
            "fakeagent",
            "--cwd",
            rig.repo.to_str().unwrap(),
            "--no-attach",
            "--json",
        ],
    );
    let local_id = local["id"].as_str().unwrap().to_string();

    let seen = rig.data(Side::Leader, &["agents", "--host", "spark", "--json"]);
    let ids: Vec<&str> = seen["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, [run.as_str()], "the leader must not see local runs");

    let table = rig.tmgr(Side::Leader, &["agents", "--host", "spark"]);
    eprintln!("$ tmgr agents --host spark\n{}", text(&table));
    let mine = rig.data(Side::Follower, &["agents", "--json"]);
    let origin = |id: &str| {
        mine["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id)
            .and_then(|s| s["origin"].as_str().map(str::to_string))
    };
    assert_eq!(origin(&run).as_deref(), Some("link:laptop"));
    assert_eq!(origin(&local_id), None);

    let refused = rig.tmgr(
        Side::Leader,
        &["run", "w1:2", "--on", "spark", "--agent", "someone-else"],
    );
    assert_eq!(code(&refused), 5, "{}", text(&refused));
    let cloud_ref = rig.tmgr(
        Side::Leader,
        &["run", "TM-512", "--on", "spark", "--agent", "fakeagent"],
    );
    assert_eq!(code(&cloud_ref), 5, "{}", text(&cloud_ref));
    let argv = rig.tmgr(
        Side::Leader,
        &["run", "w1:2", "--on", "spark", "--agent-cmd", "/bin/sh"],
    );
    assert_eq!(code(&argv), 2, "{}", text(&argv));
    let mut elsewhere = rig.pty(Side::Leader, &["attach", &local_id, "--on", "spark"]);
    assert!(
        elsewhere.exited(Duration::from_secs(15)),
        "{}",
        elsewhere.text()
    );
    assert!(elsewhere.text().contains("no run"), "{}", elsewhere.text());

    let mut ro = rig.pty(Side::Leader, &["attach", &run, "--on", "spark"]);
    ro.wait_for("READY");
    assert!(ro.text().contains("EVIL"), "{}", ro.text());
    ro.type_bytes(b"typed-while-read-only\r");
    ro.wait_for("read-only");
    thread::sleep(Duration::from_millis(500));
    assert!(!ro.text().contains("echo:typed"), "{}", ro.text());
    ro.assert_no_escape_beyond_the_allowlist();
    ro.detach();

    rig.set_scope(|s| s.allow_attach_input = true);
    let mut rw = rig.pty(Side::Leader, &["attach", &run, "--on", "spark"]);
    rw.wait_for("READY");
    rw.type_bytes(b"hello\r");
    rw.wait_for("echo:hello");
    rw.assert_no_escape_beyond_the_allowlist();
    rw.detach();

    let second = rig.data(
        Side::Leader,
        &[
            "run",
            "w1:3",
            "--on",
            "spark",
            "--agent",
            "fakeagent",
            "--json",
        ],
    );
    let second_id = second["run"]["id"].as_str().unwrap().to_string();
    let third = rig.tmgr(
        Side::Leader,
        &["run", "w1:4", "--on", "spark", "--agent", "fakeagent"],
    );
    assert_eq!(code(&third), 5, "max running reached: {}", text(&third));
    let halted = rig.data(
        Side::Leader,
        &["halt", &second_id, "--on", "spark", "--json"],
    );
    assert_eq!(halted["result"]["exited"], true);
    let no_such = rig.tmgr(Side::Leader, &["halt", &local_id, "--on", "spark"]);
    assert_eq!(code(&no_such), 3, "{}", text(&no_such));

    let mut rw = rig.pty(Side::Leader, &["attach", &run, "--on", "spark"]);
    rw.wait_for("READY");
    let revoke = rig.data(
        Side::Follower,
        &["link", "revoke", "laptop", "--halt-runs", "--json"],
    );
    eprintln!("$ tmgr link revoke laptop --halt-runs --json (follower)\n{revoke}");
    assert!(revoke["workers_stopped"].as_u64().unwrap() >= 1, "{revoke}");
    assert_eq!(revoke["runs_halted"], 1);
    assert!(
        rw.exited(Duration::from_secs(15)),
        "the attach survived the revoke:\n{}",
        rw.text()
    );

    let cut = rig.tmgr(Side::Leader, &["agents", "--host", "spark"]);
    eprintln!(
        "$ tmgr agents --host spark (after the revoke) -> exit {}\n{}",
        code(&cut),
        text(&cut)
    );
    assert_eq!(code(&cut), 9, "{}", text(&cut));
    let after = rig.data(Side::Follower, &["agents", "--json"]);
    let state = |id: &str| {
        after["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["id"] == id)
            .map(|s| s["state"]["kind"].as_str().unwrap().to_string())
    };
    assert_eq!(state(&run).as_deref(), Some("exited"));
    assert_ne!(state(&local_id).as_deref(), Some("exited"));
    let workers = rig.fp.link_dir().join("workers");
    let left = fs::read_dir(&workers).map_or(0, |d| d.count());
    assert_eq!(left, 0);
    let audit = Command::new(BIN)
        .args(["link", "audit", "--tail", "200", "--json"])
        .env_remove("XDG_RUNTIME_DIR")
        .envs(rig.env(Side::Follower))
        .output()
        .unwrap();
    let log: Value = serde_json::from_slice(&audit.stdout).unwrap();
    let events: Vec<String> = log["data"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e["event"].as_str().unwrap().to_string())
        .collect();
    for want in [
        "worker.run_created",
        "worker.attach_end",
        "link.revoke",
        "link.revoke.contain",
    ] {
        assert!(events.iter().any(|e| e == want), "{want} in {events:?}");
    }
}
