#![cfg(unix)]

use serde_json::json;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};
use tmgr_ipc::api::{caps, CreateReq, RunCreateReq, SessionInfo, State, StateKind, TaskRef};
use tmgr_ipc::proto::ServerMsg;
use tmgr_runner::client::{parse_event, AttachWriter, Client, ScreenCache};

const HOST: &str = env!("CARGO_BIN_EXE_tmgr-runner-host");
const PY: &str = "/usr/bin/python3";

struct Harness {
    tmp: tempfile::TempDir,
    run: PathBuf,
    homes: tempfile::TempDir,
    work: PathBuf,
    agents: PathBuf,
    runner: Option<Child>,
}

impl Harness {
    fn new() -> Harness {
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
        fs::create_dir(&run).unwrap();
        fs::set_permissions(&run, fs::Permissions::from_mode(0o700)).unwrap();
        let work = tmp.path().join("w");
        fs::create_dir(&work).unwrap();
        let agents = tmp.path().join("a");
        fs::create_dir(&agents).unwrap();
        let homes = tempfile::Builder::new()
            .prefix("th")
            .tempdir_in("/tmp")
            .unwrap();
        std::fs::set_permissions(
            homes.path(),
            <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700),
        )
        .unwrap();
        fs::set_permissions(homes.path(), fs::Permissions::from_mode(0o700)).unwrap();
        Harness {
            tmp,
            run,
            homes,
            work,
            agents,
            runner: None,
        }
    }

    fn start(&mut self, quiet_ms: u64) {
        let err = fs::File::create(self.tmp.path().join("runner.err")).unwrap();
        let child = Command::new(HOST)
            .args(["runner", "serve"])
            .env("TMGR_RUNNER_DIR", &self.run)
            .env("TMGR_RUNNER_HOMES", self.homes.path())
            .env("TMGR_AGENTS_DIR", &self.agents)
            .env("TMGR_T0_QUIET_MS", quiet_ms.to_string())
            .env("TMGR_RUNNER_WRITE_TIMEOUT_MS", "1500")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(err)
            .spawn()
            .unwrap();
        self.runner = Some(child);
        let deadline = Instant::now() + Duration::from_secs(10);
        while let Err(e) = Client::connect(&self.run) {
            assert!(
                Instant::now() < deadline,
                "runner did not start: {e}; {}",
                fs::read_to_string(self.tmp.path().join("runner.err")).unwrap_or_default()
            );
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn client(&self) -> Client {
        Client::connect(&self.run).unwrap()
    }

    fn kill_runner(&mut self) {
        if let Some(mut r) = self.runner.take() {
            r.kill().unwrap();
            r.wait().unwrap();
        }
    }

    fn script(&self, name: &str, body: &str) -> String {
        let p = self.work.join(name);
        fs::write(&p, body).unwrap();
        p.display().to_string()
    }

    fn req(&self, argv: &[&str]) -> CreateReq {
        CreateReq {
            argv: argv.iter().map(|s| s.to_string()).collect(),
            cwd: self.work.display().to_string(),
            name: None,
            profile: None,
            cols: 80,
            rows: 24,
            env_pass: Vec::new(),
            task: None,
        }
    }

    fn profile(&self, id: &str, command: &[&str]) {
        let words: Vec<String> = command.iter().map(|c| format!("{c:?}")).collect();
        let text = format!(
            "id = {id:?}\nname = {id:?}\ncommand = [{}]\n",
            words.join(", ")
        );
        fs::write(self.agents.join(format!("{id}.toml")), text).unwrap();
    }

    fn run_req(&self, profile: &str) -> RunCreateReq {
        RunCreateReq {
            profile: profile.into(),
            task: None,
            cwd: self.work.display().to_string(),
            name: None,
            model: None,
            cols: 80,
            rows: 24,
        }
    }

    fn sh(&self, body: &str) -> CreateReq {
        let path = self.script("agent.sh", body);
        self.req(&["/bin/sh", &path])
    }

    fn pids(&self) -> Vec<i32> {
        let mut pids = Vec::new();
        let Ok(rd) = fs::read_dir(self.run.join("sessions")) else {
            return pids;
        };
        for e in rd.flatten() {
            let Ok(bytes) = fs::read(e.path().join("meta.json")) else {
                continue;
            };
            let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or_default();
            for k in ["child_pid", "shim_pid"] {
                if let Some(p) = v[k].as_i64().filter(|p| *p > 1) {
                    pids.push(p as i32);
                }
            }
        }
        pids
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        for p in self.pids() {
            unsafe {
                libc::kill(-p, libc::SIGKILL);
                libc::kill(p, libc::SIGKILL);
            }
        }
        self.kill_runner();
    }
}

struct Watch {
    rx: Receiver<ServerMsg>,
    writer: AttachWriter,
    cache: ScreenCache,
    events: Vec<(String, State)>,
}

impl Watch {
    fn new(client: Client, ids: &[&str]) -> Watch {
        let ids: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
        let (mut reader, writer) = client.attach(&ids).unwrap().split();
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            while let Ok(Some(msg)) = reader.recv() {
                if tx.send(msg).is_err() {
                    break;
                }
            }
        });
        Watch {
            rx,
            writer,
            cache: ScreenCache::new(),
            events: Vec::new(),
        }
    }

    fn wait(&mut self, secs: u64, done: impl Fn(&Watch) -> bool) -> bool {
        let deadline = Instant::now() + Duration::from_secs(secs);
        loop {
            if done(self) {
                return true;
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            let Ok(msg) = self.rx.recv_timeout(left) else {
                return done(self);
            };
            self.cache.apply(&msg);
            if let Some(ev) = parse_event(&msg) {
                self.events.push((ev.session, ev.state));
            }
        }
    }

    fn text(&self, id: &str) -> Vec<String> {
        self.cache.snapshot(id).map_or_else(Vec::new, |s| {
            s.lines
                .iter()
                .map(|l| {
                    l.spans
                        .iter()
                        .map(|sp| sp.t.as_str())
                        .collect::<String>()
                        .trim_end()
                        .to_string()
                })
                .collect()
        })
    }

    fn has_line(&self, id: &str, needle: &str) -> bool {
        self.text(id).iter().any(|l| l.contains(needle))
    }

    fn kinds(&self) -> Vec<StateKind> {
        let mut out: Vec<StateKind> = Vec::new();
        for (_, s) in &self.events {
            if s.kind != StateKind::Starting && out.last() != Some(&s.kind) {
                out.push(s.kind);
            }
        }
        out
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        let _ = self.writer.detach();
    }
}

fn wait_state(c: &mut Client, id: &str, secs: u64, pred: impl Fn(&State) -> bool) -> SessionInfo {
    let deadline = Instant::now() + Duration::from_secs(secs);
    loop {
        let info = c.list().unwrap().into_iter().find(|s| s.id == id).unwrap();
        if pred(&info.state) || Instant::now() >= deadline {
            return info;
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn sessions_available() -> bool {
    let abi = tmgr_runner::daemon::landlock_abi();
    if abi >= 6 {
        return true;
    }
    eprintln!("skipped: Landlock ABI {abi} < 6");
    false
}

fn alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

fn err_text(r: std::io::Result<impl std::fmt::Debug>) -> String {
    r.unwrap_err().to_string()
}

#[test]
fn echo_input_and_exit_code() {
    if !sessions_available() {
        return;
    }
    let mut h = Harness::new();
    h.start(5000);
    let mut c = h.client();
    let info = c
        .create(h.sh("printf 'hello\\n'; read x; printf 'got:%s\\n' \"$x\"; exit 3\n"))
        .unwrap();
    let mut w = Watch::new(h.client(), &[&info.id]);
    assert!(
        w.wait(5, |w| w.has_line(&info.id, "hello")),
        "{:?}",
        w.text(&info.id)
    );
    w.writer.input(&info.id, b"abc\r").unwrap();
    assert!(
        w.wait(5, |w| w.has_line(&info.id, "got:abc")),
        "{:?}",
        w.text(&info.id)
    );
    let done = wait_state(&mut c, &info.id, 5, |s| s.kind == StateKind::Exited);
    assert_eq!(done.state.kind, StateKind::Exited);
    assert_eq!(done.state.exit_code, Some(3));
}

#[test]
fn terminal_queries_are_answered() {
    if !sessions_available() {
        return;
    }
    let mut h = Harness::new();
    h.start(5000);
    let mut c = h.client();
    let body = "import os, sys, termios, tty\n\
        fd = sys.stdin.fileno()\n\
        old = termios.tcgetattr(fd)\n\
        tty.setraw(fd)\n\
        os.write(1, b'\\x1b[6n')\n\
        buf = b''\n\
        while not buf.endswith(b'R'):\n\
        \x20   buf += os.read(0, 1)\n\
        termios.tcsetattr(fd, termios.TCSADRAIN, old)\n\
        print('ok:' + repr(buf))\n";
    let path = h.script("dsr.py", body);
    let info = c.create(h.req(&[PY, &path])).unwrap();
    let mut w = Watch::new(h.client(), &[&info.id]);
    assert!(
        w.wait(3, |w| w.has_line(&info.id, "ok:")),
        "{:?} {:?} {}",
        w.text(&info.id),
        c.list().unwrap()[0].state,
        fs::read_to_string(h.run.join("sessions").join(&info.id).join("shim.log")).unwrap()
    );
    assert!(w.has_line(&info.id, "[1;1R"), "{:?}", w.text(&info.id));
}

#[test]
fn working_waiting_working_exited() {
    if !sessions_available() {
        return;
    }
    let mut h = Harness::new();
    h.start(300);
    let mut c = h.client();
    let body = "i=0; while [ $i -lt 10 ]; do echo p$i; sleep 0.1; i=$((i+1)); done\n\
        echo ready; read x\n\
        i=0; while [ $i -lt 10 ]; do echo q$i; sleep 0.1; i=$((i+1)); done\n\
        exit 0\n";
    let info = c.create(h.sh(body)).unwrap();
    let mut w = Watch::new(h.client(), &[&info.id]);
    assert!(
        w.wait(10, |w| w.has_line(&info.id, "ready")),
        "{:?}",
        w.text(&info.id)
    );
    assert!(
        w.wait(10, |w| w.kinds().contains(&StateKind::Waiting)),
        "{:?}",
        w.kinds()
    );
    w.writer.input(&info.id, b"go\r").unwrap();
    assert!(
        w.wait(10, |w| w.kinds().last() == Some(&StateKind::Exited)),
        "{:?}",
        w.kinds()
    );
    assert_eq!(
        w.kinds(),
        [
            StateKind::Working,
            StateKind::Waiting,
            StateKind::Working,
            StateKind::Exited
        ]
    );
    let last = &w.events.last().unwrap().1;
    assert_eq!(last.exit_code, Some(0));
}

#[test]
fn sessions_survive_runner_restart() {
    if !sessions_available() {
        return;
    }
    let mut h = Harness::new();
    h.start(5000);
    let mut c = h.client();
    let info = c
        .create(h.sh("i=0; while true; do echo n$i; i=$((i+1)); sleep 0.2; done\n"))
        .unwrap();
    let max_n = |w: &Watch| {
        w.text(&info.id)
            .iter()
            .filter_map(|l| l.strip_prefix('n')?.parse::<u32>().ok())
            .max()
    };
    let mut w = Watch::new(h.client(), &[&info.id]);
    assert!(w.wait(5, |w| max_n(w) >= Some(3)), "{:?}", w.text(&info.id));
    let before = max_n(&w).unwrap();
    drop(w);
    drop(c);

    h.kill_runner();
    assert!(alive(info.child_pid), "agent died with the runner");
    assert!(alive(info.shim_pid), "shim died with the runner");

    h.start(5000);
    let mut c = h.client();
    let list = c.list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, info.id);
    let after = wait_state(&mut c, &info.id, 5, |s| s.kind == StateKind::Working);
    assert_eq!(after.state.kind, StateKind::Working);

    let mut w = Watch::new(h.client(), &[&info.id]);
    assert!(
        w.wait(8, |w| max_n(w) > Some(before + 1)),
        "{before} {:?}",
        w.text(&info.id)
    );
    assert!(w.has_line(&info.id, "n0"), "{:?}", w.text(&info.id));
}

#[test]
fn rejects_unauthorized_and_invalid_requests() {
    let mut h = Harness::new();
    h.start(5000);
    let wrong = "0".repeat(64);
    let e = Client::connect_with(&h.run, &wrong).err().unwrap();
    assert_eq!(e.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(e.to_string().contains("identity check failed"), "{e}");

    let mut c = h.client();
    let mut relative = h.req(&["/bin/true"]);
    relative.cwd = "relative/dir".into();
    assert!(err_text(c.create(relative)).contains("invalid_params"));
    let mut missing = h.req(&["/bin/true"]);
    missing.cwd = h.work.join("nope").display().to_string();
    assert!(err_text(c.create(missing)).contains("invalid_params"));
    let mut protected = vec!["/".to_string(), h.run.display().to_string()];
    protected.push(h.run.join("sessions").display().to_string());
    protected.push(h.tmp.path().display().to_string());
    protected.push(h.homes.path().display().to_string());
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        protected.push(home.display().to_string());
        protected.push(home.parent().unwrap().display().to_string());
    }
    for cwd in protected {
        let mut r = h.req(&["/bin/true"]);
        r.cwd = cwd.clone();
        assert!(err_text(c.create(r)).contains("invalid_params"), "{cwd}");
    }
    assert!(err_text(c.create(h.req(&["/bin/echo", "a\0b"]))).contains("invalid_params"));
    assert!(err_text(c.create(h.req(&[]))).contains("invalid_params"));
    assert!(err_text(c.create(h.req(&["./agent"]))).contains("invalid_params"));
    assert!(err_text(c.create(h.req(&["sh"]))).contains("invalid_params"));
    let mut unknown = h.req(&["/bin/true"]);
    unknown.profile = Some("no-such-profile".into());
    assert!(err_text(c.create(unknown)).contains("unknown_profile"));

    for call in ["sessions.kill", "sessions.remove", "sessions.send"] {
        let e = err_text(c.call(call, json!({"id": "../etc", "signal": "term", "text": "x"})));
        assert!(e.contains("invalid_params"), "{call}: {e}");
    }
    let e = err_text(c.call("attach.open", json!({"sessions": ["s_../..//.."]})));
    assert!(e.contains("invalid_params"), "{e}");

    if !sessions_available() {
        return;
    }
    let info = c.create(h.req(&["/bin/sleep", "30"])).unwrap();
    let w = Watch::new(h.client(), &[&info.id]);
    w.writer.resize(&info.id, 0, 0).unwrap();
    w.writer.resize(&info.id, u16::MAX, u16::MAX).unwrap();
    thread::sleep(Duration::from_millis(300));
    let now = c.list().unwrap().remove(0);
    assert_eq!((now.cols, now.rows), (500, 200));
    w.writer.resize(&info.id, 0, 0).unwrap();
    thread::sleep(Duration::from_millis(300));
    let now = c.list().unwrap().remove(0);
    assert_eq!((now.cols, now.rows), (1, 1));
    assert_eq!(c.status().unwrap().sessions, 1);

    for _ in 1..16 {
        c.create(h.req(&["/bin/sleep", "30"])).unwrap();
    }
    assert!(err_text(c.create(h.req(&["/bin/sleep", "30"]))).contains("limit"));
}

#[test]
fn resize_reaches_the_pty() {
    if !sessions_available() {
        return;
    }
    let mut h = Harness::new();
    h.start(5000);
    let mut c = h.client();
    let info = c
        .create(h.sh("echo ready; read x; stty size; sleep 5\n"))
        .unwrap();
    let mut w = Watch::new(h.client(), &[&info.id]);
    assert!(w.wait(5, |w| w.has_line(&info.id, "ready")));
    w.writer.resize(&info.id, 100, 30).unwrap();
    w.writer.input(&info.id, b"\r").unwrap();
    assert!(
        w.wait(5, |w| w.has_line(&info.id, "30 100")),
        "{:?}",
        w.text(&info.id)
    );
    assert_eq!(w.cache.snapshot(&info.id).unwrap().cols, 100);
}

#[test]
fn exited_session_can_be_removed() {
    if !sessions_available() {
        return;
    }
    let mut h = Harness::new();
    h.start(5000);
    let mut c = h.client();
    let running = c.create(h.req(&["/bin/sleep", "30"])).unwrap();
    assert!(err_text(c.remove(&running.id)).contains("busy"));

    let info = c.create(h.req(&["/bin/sh", "-c", "exit 0"])).unwrap();
    let done = wait_state(&mut c, &info.id, 5, |s| s.kind == StateKind::Exited);
    assert_eq!(done.state.kind, StateKind::Exited);
    let sdir = h.run.join("sessions").join(&info.id);
    let home = h.homes.path().join(&info.id);
    assert!(sdir.is_dir() && home.is_dir());
    c.remove(&info.id).unwrap();
    assert!(!sdir.exists() && !home.exists());
    let ids: Vec<String> = c.list().unwrap().into_iter().map(|s| s.id).collect();
    assert_eq!(ids, [running.id]);
}

#[test]
fn send_kill_layout_and_stop() {
    if !sessions_available() {
        return;
    }
    let mut h = Harness::new();
    h.start(5000);
    let mut c = h.client();
    let info = c.create(h.req(&["/bin/cat"])).unwrap();
    c.send(&info.id, "hi there\r").unwrap();
    let mut w = Watch::new(h.client(), &[&info.id]);
    assert!(w.wait(5, |w| w.has_line(&info.id, "hi there")));
    c.kill(&info.id, "term").unwrap();
    let done = wait_state(&mut c, &info.id, 5, |s| s.kind == StateKind::Exited);
    assert_eq!(done.state.exit_code, Some(143));

    assert_eq!(c.layout_get().unwrap(), json!(null));
    c.layout_set(json!({"split": true})).unwrap();
    assert_eq!(c.layout_get().unwrap(), json!({"split": true}));
    let big = "x".repeat(70_000);
    assert!(err_text(c.layout_set(json!(big))).contains("invalid_params"));

    let live = c.create(h.req(&["/bin/sleep", "30"])).unwrap();
    c.stop().unwrap();
    let mut runner = h.runner.take().unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while runner.try_wait().unwrap().is_none() {
        assert!(Instant::now() < deadline, "runner did not stop");
        thread::sleep(Duration::from_millis(50));
    }
    assert!(alive(live.child_pid) && alive(live.shim_pid));
    let _: &Path = &h.run;
}

#[test]
fn send_reports_a_full_input_queue() {
    if !sessions_available() {
        return;
    }
    let mut h = Harness::new();
    h.start(5000);
    let mut c = h.client();
    let info = c.create(h.req(&["/bin/sleep", "30"])).unwrap();
    let chunk = "xxxxxxx\n".repeat(8 * 1024);
    let mut refused = None;
    for _ in 0..200 {
        if let Err(e) = c.send(&info.id, &chunk) {
            refused = Some(e.to_string());
            break;
        }
    }
    let e = refused.expect("a stalled agent must make sessions.send fail");
    assert!(e.contains("busy") && e.contains("input queue full"), "{e}");
}

#[test]
fn start_background_is_idempotent() {
    let h = Harness::new();
    let cfg = tmgr_runner::daemon::RunnerConfig {
        dir: h.run.clone(),
        homes: h.homes.path().to_path_buf(),
        self_exe: PathBuf::from(HOST),
        quiet_ms: 5000,
        version: "test".into(),
        max_sessions: 16,
    };
    let pid = tmgr_runner::daemon::start_background(&cfg).unwrap();
    let again = tmgr_runner::daemon::start_background(&cfg);
    let mut c = h.client();
    let status = c.status().unwrap();
    let log_mode = fs::metadata(h.run.join("runner.log"))
        .unwrap()
        .permissions()
        .mode();
    unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    assert_eq!(again.unwrap(), pid);
    assert_eq!(status.pid, pid);
    assert_eq!(log_mode & 0o777, 0o600);
}

#[test]
fn large_input_is_chunked_and_first_screen_arrives() {
    if !sessions_available() {
        return;
    }
    let mut h = Harness::new();
    h.start(5000);
    let mut c = h.client();
    let info = c
        .create(h.sh("stty raw -echo; echo READY; head -c 200000 | wc -c; sleep 5\n"))
        .unwrap();
    let w = Watch::new(h.client(), &[&info.id]);
    let mut w = w;
    assert!(w.wait(10, |w| w.has_line(&info.id, "READY")));
    w.writer.input(&info.id, &vec![b'a'; 200_000]).unwrap();
    assert!(
        w.wait(10, |w| w.has_line(&info.id, "200000")),
        "{:?}",
        w.text(&info.id)
    );
}

#[cfg(target_os = "linux")]
#[test]
fn start_background_replaces_a_squatted_runner_address() {
    use tmgr_ipc::endpoint::{bind, Endpoint};
    use tmgr_ipc::frame::{read_frame, write_json};
    let h = Harness::new();
    let squat = bind(&Endpoint::new_runner(&h.run).unwrap()).unwrap();
    thread::spawn(move || {
        for s in squat.incoming().flatten() {
            let mut s = s;
            let _ = read_frame(&mut s);
            let _ = write_json(
                &mut s,
                &json!({"type": "challenge", "nonce_s": "aa", "proof_s": "bb"}),
            );
        }
    });
    assert!(Client::connect(&h.run).is_err());
    let cfg = tmgr_runner::daemon::RunnerConfig {
        dir: h.run.clone(),
        homes: h.homes.path().to_path_buf(),
        self_exe: PathBuf::from(HOST),
        quiet_ms: 5000,
        version: "test".into(),
        max_sessions: 16,
    };
    let pid = tmgr_runner::daemon::start_background(&cfg).unwrap();
    let status = h.client().status().unwrap();
    unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    assert_eq!(status.pid, pid);
}

#[test]
fn trickling_handshake_is_disconnected_by_the_runner() {
    use std::io::{Read, Write};
    let mut h = Harness::new();
    h.start(5000);
    let ep = tmgr_ipc::endpoint::Endpoint::load_runner(&h.run).unwrap();
    let mut s = tmgr_ipc::endpoint::connect(&ep).unwrap();
    s.set_read_timeout(Some(Duration::from_millis(100)))
        .unwrap();
    let started = Instant::now();
    let mut closed = false;
    'outer: while started.elapsed() < Duration::from_secs(10) {
        let _ = s.write_all(&[0]);
        let tick = Instant::now();
        while tick.elapsed() < Duration::from_secs(1) {
            let mut b = [0u8; 8];
            match s.read(&mut b) {
                Ok(0) => {
                    closed = true;
                    break 'outer;
                }
                Ok(_) => {}
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) => {}
                Err(_) => {
                    closed = true;
                    break 'outer;
                }
            }
        }
    }
    assert!(closed, "still connected after {:?}", started.elapsed());
    assert!(
        started.elapsed() < Duration::from_secs(7),
        "{:?}",
        started.elapsed()
    );
}

#[test]
fn rejected_connections_do_not_flood_the_log() {
    let mut h = Harness::new();
    h.start(5000);
    let ep = tmgr_ipc::endpoint::Endpoint::load_runner(&h.run).unwrap();
    let mut made = 0;
    for _ in 0..10_000 {
        if tmgr_ipc::endpoint::connect(&ep).is_ok() {
            made += 1;
        }
    }
    assert!(made > 100, "only {made} connections made");
    thread::sleep(Duration::from_millis(500));
    let log = fs::read_to_string(h.tmp.path().join("runner.err")).unwrap();
    let lines = log
        .lines()
        .filter(|l| l.contains("rejected connection"))
        .count();
    assert!(lines <= 3, "{lines} rejection lines\n{log}");
    assert!(h.client().status().is_ok());
}

#[test]
fn shim_caps_pending_connections() {
    if !sessions_available() {
        return;
    }
    let mut h = Harness::new();
    h.start(5000);
    let mut c = h.client();
    let info = c.create(h.req(&["/bin/sleep", "30"])).unwrap();
    let ep = tmgr_ipc::endpoint::Endpoint::load_session(&h.run, &info.id).unwrap();
    let held: Vec<_> = (0..4)
        .map(|_| tmgr_ipc::endpoint::connect(&ep).unwrap())
        .collect();
    thread::sleep(Duration::from_millis(200));
    let mut extra = tmgr_ipc::endpoint::connect(&ep).unwrap();
    extra
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let mut b = [0u8; 1];
    let n = std::io::Read::read(&mut extra, &mut b);
    assert!(matches!(n, Ok(0)), "extra connection was not closed: {n:?}");
    drop(held);
}

#[test]
fn a_client_that_never_reads_is_disconnected_and_others_keep_receiving() {
    if !sessions_available() {
        return;
    }
    let mut h = Harness::new();
    h.start(5000);
    let mut c = h.client();
    let body = "import os\ni = 0\nwhile True:\n    os.write(1, ('\\x07line %d ' % i + 'x' * 60 + '\\n').encode())\n    i += 1\n";
    let path = h.script("flood.py", body);
    let info = c.create(h.req(&[PY, &path])).unwrap();
    let mut stalled = h.client().attach(std::slice::from_ref(&info.id)).unwrap();
    let mut w = Watch::new(h.client(), &[&info.id]);
    assert!(w.wait(30, |w| w.has_line(&info.id, "line")));
    thread::sleep(Duration::from_secs(4));
    let before = w.text(&info.id);
    assert!(
        w.wait(15, |w| w.text(&info.id) != before),
        "healthy client stopped receiving"
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut ended = false;
    while Instant::now() < deadline {
        match stalled.recv() {
            Ok(Some(ServerMsg::Closed { .. })) | Ok(None) | Err(_) => {
                ended = true;
                break;
            }
            Ok(Some(_)) => {}
        }
    }
    assert!(ended, "stalled client is still being served");
    assert!(c.status().is_ok());
}

#[test]
fn kill_works_while_the_agent_ignores_input_and_spams_queries() {
    if !sessions_available() {
        return;
    }
    let mut h = Harness::new();
    h.start(5000);
    let mut c = h.client();
    let body = "import os, time, tty\ntty.setraw(0)\nwhile True:\n    os.write(1, b'\\x1b[6n' * 50)\n    time.sleep(0.001)\n";
    let path = h.script("spam.py", body);
    let info = c.create(h.req(&[PY, &path])).unwrap();
    let mut w = Watch::new(h.client(), &[&info.id]);
    assert!(w.wait(5, |w| w.cache.snapshot(&info.id).is_some()));
    let writer = w.writer.clone();
    let id = info.id.clone();
    thread::spawn(move || {
        let _ = writer.input(&id, &vec![b'a'; 10 << 20]);
    });
    thread::sleep(Duration::from_secs(1));
    c.kill(&info.id, "kill").unwrap();
    let done = wait_state(&mut c, &info.id, 2, |s| s.kind == StateKind::Exited);
    assert_eq!(done.state.kind, StateKind::Exited, "{:?}", done.state);
}

#[test]
fn kill_after_exit_is_not_running() {
    if !sessions_available() {
        return;
    }
    let mut h = Harness::new();
    h.start(5000);
    let mut c = h.client();
    let info = c.create(h.req(&["/bin/sh", "-c", "exit 0"])).unwrap();
    wait_state(&mut c, &info.id, 5, |s| s.kind == StateKind::Exited);
    for sig in ["term", "kill"] {
        assert!(err_text(c.kill(&info.id, sig)).contains("not_running"));
    }
}

#[test]
fn a_500x200_screen_of_alternating_colours_reaches_the_client_intact() {
    if !sessions_available() {
        return;
    }
    let mut h = Harness::new();
    h.start(5000);
    let mut c = h.client();
    let body = "import sys, time\ntime.sleep(1.5)\nfor r in range(199):\n    sys.stdout.write(''.join('\\x1b[3%dm%s' % (1 + c % 2, chr(97 + c % 26)) for c in range(500)) + '\\x1b[0m\\r\\n')\nsys.stdout.write('done')\nsys.stdout.flush()\ntime.sleep(30)\n";
    let path = h.script("wide.py", body);
    let info = c.create(h.req(&[PY, &path])).unwrap();
    let mut w = Watch::new(h.client(), &[&info.id]);
    w.writer.resize(&info.id, 500, 200).unwrap();
    assert!(
        w.wait(30, |w| w.has_line(&info.id, "done")),
        "{:?}",
        w.cache.snapshot(&info.id).map(|s| (s.rows, s.cols))
    );
    let snap = w.cache.snapshot(&info.id).unwrap();
    assert_eq!((snap.rows, snap.cols, snap.lines.len()), (200, 500, 200));
    let text: String = snap.lines[198].spans.iter().map(|s| s.t.as_str()).collect();
    assert_eq!(text.len(), 500);
    assert!(text.starts_with("abcdefghij"));
    assert!(snap.lines[198].spans.len() >= 500);
    assert!(c.status().is_ok());
}

#[test]
fn create_is_refused_without_a_usable_sandbox() {
    let abi = tmgr_runner::daemon::landlock_abi();
    if abi >= 6 {
        eprintln!("skipped: Landlock ABI {abi} is sufficient");
        return;
    }
    let mut h = Harness::new();
    h.start(5000);
    let e = err_text(h.client().create(h.req(&["/bin/sleep", "30"])));
    assert!(e.contains("sandbox unavailable"), "{e}");
    assert!(h.client().list().unwrap().is_empty());
}

#[test]
fn welcome_lists_every_contract_capability() {
    let mut h = Harness::new();
    h.start(5000);
    let c = h.client();
    for cap in [
        "sessions.v1",
        "attach.v1",
        "runs.create.v1",
        "task_ref.v1",
        "layout.v1",
    ] {
        assert!(c.welcome().caps.iter().any(|x| x == cap), "{cap}");
    }
    assert_eq!(c.welcome().caps.len(), caps::ALL.len());
}

#[test]
fn raw_argv_is_refused_for_clients_other_than_the_cli() {
    let mut h = Harness::new();
    h.start(5000);
    let mut desktop = Client::connect_as(&h.run, "desktop").unwrap();
    let e = err_text(desktop.create(h.req(&["/bin/true"])));
    assert!(e.starts_with("forbidden: "), "{e}");
    assert!(e.contains("runs.create"), "{e}");
    assert!(desktop.list().unwrap().is_empty());
}

#[test]
fn runs_create_rejects_unknown_profiles_and_missing_executables() {
    let mut h = Harness::new();
    h.profile("ghost", &["tmgr-no-such-agent-binary"]);
    h.start(5000);
    let mut c = Client::connect_as(&h.run, "desktop").unwrap();
    let e = err_text(c.create_run(h.run_req("nope")));
    assert!(e.starts_with("unknown_profile: "), "{e}");
    let e = err_text(c.create_run(h.run_req("ghost")));
    assert!(e.starts_with("invalid_params: "), "{e}");
    assert!(e.contains("tmgr-no-such-agent-binary"), "{e}");
    let mut bad_model = h.run_req("ghost");
    bad_model.model = Some("bad model!".into());
    assert!(err_text(c.create_run(bad_model)).starts_with("invalid_params: "));
}

#[test]
fn runs_create_starts_a_profile_and_the_task_survives_a_runner_restart() {
    if !sessions_available() {
        return;
    }
    let mut h = Harness::new();
    let script = h.script(
        "fake-agent.sh",
        "echo ready\nwhile true; do sleep 0.2; done\n",
    );
    h.profile("fake", &["/bin/sh", &script]);
    h.start(5000);
    let task = TaskRef {
        workspace_id: -3,
        task_id: 412,
    };
    let mut c = Client::connect_as(&h.run, "desktop").unwrap();
    let mut req = h.run_req("fake");
    req.task = Some(task);
    req.name = Some("fix login".into());
    let info = c.create_run(req).unwrap();
    assert_eq!(info.profile.as_deref(), Some("fake"));
    assert_eq!(info.task, Some(task));
    assert_eq!(info.name, "fix login");
    assert_eq!(info.argv, ["/bin/sh".to_string(), script.clone()]);

    let mut stream = h.client().attach(std::slice::from_ref(&info.id)).unwrap();
    let mut seen = None;
    for _ in 0..20 {
        let Some(msg) = stream.recv().unwrap() else {
            break;
        };
        if let Some(ev) = msg.event() {
            seen = Some(ev);
            break;
        }
    }
    let ev = seen.expect("an event push");
    assert_eq!(
        (ev.session.as_str(), ev.task),
        (info.id.as_str(), Some(task))
    );
    drop(stream);
    drop(c);

    h.kill_runner();
    h.start(5000);
    let list = h.client().list().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].task, Some(task));
    assert_eq!(list[0].profile.as_deref(), Some("fake"));
}
