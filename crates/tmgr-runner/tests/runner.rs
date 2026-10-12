#![cfg(unix)]

use serde_json::json;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};
use tmgr_ipc::proto::ServerMsg;
use tmgr_runner::client::{parse_event, AttachWriter, Client, ScreenCache};
use tmgr_runner::t0::{State, StateKind};
use tmgr_runner::wire::{CreateReq, SessionInfo};

const HOST: &str = env!("CARGO_BIN_EXE_tmgr-runner-host");
const PY: &str = "/usr/bin/python3";

struct Harness {
    tmp: tempfile::TempDir,
    run: PathBuf,
    homes: tempfile::TempDir,
    work: PathBuf,
    runner: Option<Child>,
}

impl Harness {
    fn new() -> Harness {
        let tmp = tempfile::Builder::new()
            .prefix("tr")
            .tempdir_in("/tmp")
            .unwrap();
        let run = tmp.path().join("r");
        fs::create_dir(&run).unwrap();
        fs::set_permissions(&run, fs::Permissions::from_mode(0o700)).unwrap();
        let work = tmp.path().join("w");
        fs::create_dir(&work).unwrap();
        let homes = tempfile::Builder::new()
            .prefix("th")
            .tempdir_in("/tmp")
            .unwrap();
        fs::set_permissions(homes.path(), fs::Permissions::from_mode(0o700)).unwrap();
        Harness {
            tmp,
            run,
            homes,
            work,
            runner: None,
        }
    }

    fn start(&mut self, quiet_ms: u64) {
        let err = fs::File::create(self.tmp.path().join("runner.err")).unwrap();
        let child = Command::new(HOST)
            .args(["runner", "serve"])
            .env("TMGR_RUNNER_DIR", &self.run)
            .env("TMGR_RUNNER_HOMES", self.homes.path())
            .env("TMGR_T0_QUIET_MS", quiet_ms.to_string())
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

fn alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

fn err_text(r: std::io::Result<impl std::fmt::Debug>) -> String {
    r.unwrap_err().to_string()
}

#[test]
fn echo_input_and_exit_code() {
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
    assert!(e.to_string().contains("unauthorized"), "{e}");

    let mut c = h.client();
    let mut relative = h.req(&["/bin/true"]);
    relative.cwd = "relative/dir".into();
    assert!(err_text(c.create(relative)).contains("invalid_params"));
    let mut missing = h.req(&["/bin/true"]);
    missing.cwd = h.work.join("nope").display().to_string();
    assert!(err_text(c.create(missing)).contains("invalid_params"));
    assert!(err_text(c.create(h.req(&["/bin/echo", "a\0b"]))).contains("invalid_params"));
    assert!(err_text(c.create(h.req(&[]))).contains("invalid_params"));
    let mut unknown = h.req(&["/bin/true"]);
    unknown.profile = Some("no-such-profile".into());
    assert!(err_text(c.create(unknown)).contains("unknown_profile"));

    for call in ["sessions.kill", "sessions.remove", "sessions.send"] {
        let e = err_text(c.call(call, json!({"id": "../etc", "signal": "term", "text": "x"})));
        assert!(e.contains("invalid_params"), "{call}: {e}");
    }
    let e = err_text(c.call("attach.open", json!({"sessions": ["s_../..//.."]})));
    assert!(e.contains("invalid_params"), "{e}");

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
