use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::api::{StartRun, StateKind};
use super::conn::Conn;
use super::dir;
use super::pump::{TermFrame, WatchEvent};
use super::{Connector, Runner};

const AGENT: &str = r#"printf '\033[31mred line\033[0m\n'
printf '\033[1;32mbold green\033[0m plain\n'
printf '\033]52;c;ZWNobw==\007after-osc52\n'
printf '日本語wide line\n'
printf 'ready> '
read line
printf 'got: %s\n' "$line"
exit 0
"#;

const SLEEPER: &str = "echo up\nwhile true; do sleep 0.2; done\n";

struct Stand {
  root: tempfile::TempDir,
  run: PathBuf,
  project: PathBuf,
  child: Option<Child>,
}

fn private(path: &Path) {
  fs::create_dir_all(path).unwrap();
  fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
}

fn host_binary() -> PathBuf {
  let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
  let exe = std::env::current_exe().unwrap();
  let target = exe.parent().and_then(Path::parent).and_then(Path::parent).unwrap().to_path_buf();
  let status = Command::new("cargo")
    .args(["build", "-q", "-p", "tmgr-runner", "--bin", "tmgr-runner-host", "--manifest-path"])
    .arg(manifest.join("../Cargo.toml"))
    .arg("--target-dir")
    .arg(&target)
    .status()
    .unwrap();
  assert!(status.success(), "building tmgr-runner-host failed");
  target.join("debug/tmgr-runner-host")
}

impl Stand {
  fn start() -> Stand {
    let host = host_binary();
    let root = tempfile::Builder::new().prefix("tmgr-rd-").tempdir_in("/private/tmp").unwrap();
    let base = root.path().canonicalize().unwrap();
    let (run, homes, agents, project) = (base.join("run"), base.join("homes"), base.join("agents"), base.join("project"));
    for d in [&run, &homes, &agents, &project] {
      private(d);
    }
    for (id, script) in [("fake", AGENT), ("sleeper", SLEEPER)] {
      let file = project.join(format!("{id}.sh"));
      fs::write(&file, script).unwrap();
      let command = format!("id = {id:?}\nname = {id:?}\ncommand = [\"/bin/sh\", {:?}]\n", file.display().to_string());
      fs::write(agents.join(format!("{id}.toml")), command).unwrap();
    }
    let err = fs::File::create(base.join("runner.err")).unwrap();
    let child = Command::new(host)
      .args(["runner", "serve"])
      .env("TMGR_RUNNER_DIR", &run)
      .env("TMGR_RUNNER_HOMES", &homes)
      .env("TMGR_AGENTS_DIR", &agents)
      .env("TMGR_T0_QUIET_MS", "5000")
      .stdin(Stdio::null())
      .stdout(Stdio::null())
      .stderr(err)
      .spawn()
      .unwrap();
    let mut stand = Stand { root, run, project, child: Some(child) };
    let deadline = Instant::now() + Duration::from_secs(15);
    while dir::connect_in(&stand.run, None).is_err() {
      assert!(
        Instant::now() < deadline,
        "runner did not start: {}",
        fs::read_to_string(stand.root.path().join("runner.err")).unwrap_or_default()
      );
      std::thread::sleep(Duration::from_millis(50));
    }
    stand.child.as_mut().unwrap().try_wait().unwrap();
    stand
  }

  fn connect(&self) -> Result<(Conn, super::conn::Welcome), super::conn::ConnError> {
    dir::connect_in(&self.run, None)
  }

  fn runner(&self) -> Runner {
    let run = self.run.clone();
    let connector: Connector = Arc::new(move || dir::connect_in(&run, None));
    let runner = Runner::new(self.root.path().join("cfg/runner.json"), connector);
    runner.set_enabled(true).unwrap();
    runner
  }
}

impl Drop for Stand {
  fn drop(&mut self) {
    if let Ok((mut conn, _)) = self.connect() {
      if let Ok(Value::Array(list)) = conn.call("sessions.list", Value::Null) {
        for s in &list {
          let _ = conn.call("sessions.kill", serde_json::json!({"id": s["id"], "signal": "kill"}));
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        for s in &list {
          while conn.call("sessions.remove", serde_json::json!({"id": s["id"]})).is_err() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(50));
          }
        }
      }
      let _ = conn.call("runner.stop", Value::Null);
    }
    if let Some(mut child) = self.child.take() {
      let deadline = Instant::now() + Duration::from_secs(3);
      while child.try_wait().ok().flatten().is_none() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
      }
      let _ = child.kill();
      let _ = child.wait();
    }
  }
}

fn run_req(profile: &str, task_id: i64, project: &Path) -> StartRun {
  StartRun {
    workspace_id: -3,
    task_id,
    profile: profile.into(),
    cwd: project.display().to_string(),
    name: format!("live task {task_id}"),
  }
}

struct Screen {
  rx: Receiver<TermFrame>,
  text: String,
  states: Vec<StateKind>,
}

impl Screen {
  fn wait_for(&mut self, runner: &Runner, sid: &str, what: &str) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !self.text.contains(what) {
      let left = deadline.saturating_duration_since(Instant::now());
      assert!(!left.is_zero(), "timed out waiting for {what:?} in {:?}", self.text);
      match self.rx.recv_timeout(left) {
        Ok(TermFrame::Data { seq, data }) => {
          self.text.push_str(&data);
          runner.term_ack(sid, seq).unwrap();
        }
        Ok(TermFrame::State { state }) => self.states.push(state.kind),
        Ok(_) => {}
        Err(_) => panic!("no frame while waiting for {what:?} in {:?}", self.text),
      }
    }
  }
}

fn wait_session(runner: &Runner, sid: &str, kind: StateKind) -> super::api::RunnerSession {
  let deadline = Instant::now() + Duration::from_secs(20);
  loop {
    let found = runner.sessions().unwrap().into_iter().find(|s| s.id == sid);
    if let Some(s) = found.filter(|s| s.state.kind == kind) {
      return s;
    }
    assert!(Instant::now() < deadline, "session {sid} never reached {kind:?}");
    std::thread::sleep(Duration::from_millis(100));
  }
}

#[test]
#[ignore = "starts a real tmgr-runner-host; run explicitly with --ignored live"]
fn live_runner_end_to_end() {
  let stand = Stand::start();
  let runner = stand.runner();

  let status = runner.status();
  assert_eq!(status.state, "ready", "{status:?}");
  for cap in ["sessions.v1", "attach.v1", "runs.create.v1", "task_ref.v1"] {
    assert!(status.caps.iter().any(|c| c == cap), "{cap} in {:?}", status.caps);
  }
  println!("runner version {:?}, caps {:?}", status.version, status.caps);

  let (tx, events) = mpsc::channel();
  let watch = runner.watch(move |ev| tx.send(ev).is_ok()).unwrap();

  let created = runner.start_run(&run_req("fake", 412, &stand.project)).unwrap();
  assert_eq!(created.profile.as_deref(), Some("fake"));
  assert_eq!(created.name, "live task 412");
  assert_eq!((created.task.unwrap().workspace_id, created.task.unwrap().task_id), (-3, 412));

  let listed = runner.sessions().unwrap();
  let mine = listed.iter().find(|s| s.id == created.id).expect("created session is listed");
  assert_eq!(mine.task, created.task);

  let (ftx, frames) = mpsc::channel();
  let info = runner.term_attach(&created.id, 100, 30, move |f| ftx.send(f).is_ok()).unwrap();
  assert_eq!(info.task, created.task);
  let mut screen = Screen { rx: frames, text: String::new(), states: Vec::new() };
  for expected in ["red line", "bold green", "after-osc52", "日本語wide line", "ready>"] {
    screen.wait_for(&runner, &created.id, expected);
  }
  assert!(screen.text.contains("\x1b[0;31m") || screen.text.contains("\x1b[0;38;5;1m"), "no colour in {:?}", screen.text);
  assert!(!screen.text.contains("\x1b]"), "OSC leaked: {:?}", screen.text);
  assert!(!screen.text.contains('\x07'));
  assert!(!screen.text.contains("ESC]52"));

  runner.term_input(&created.id, "hello\r").unwrap();
  screen.wait_for(&runner, &created.id, "got: hello");

  let done = wait_session(&runner, &created.id, StateKind::Exited);
  assert_eq!(done.state.detail.as_deref(), Some("done"));
  assert_eq!(done.state.exit_code, Some(0));
  runner.term_detach(&created.id);

  let mut saw_exit_with_task = false;
  let deadline = Instant::now() + Duration::from_secs(10);
  while !saw_exit_with_task && Instant::now() < deadline {
    if let Ok(WatchEvent::State { session, state, task }) = events.recv_timeout(Duration::from_millis(500)) {
      saw_exit_with_task = session == created.id && state.kind == StateKind::Exited && task == created.task;
    }
  }
  assert!(saw_exit_with_task, "watch never reported the exit with its task");
  runner.unwatch(watch);

  let second = runner.start_run(&run_req("sleeper", 413, &stand.project)).unwrap();
  wait_session(&runner, &second.id, StateKind::Working);
  assert!(runner.session_exists(&second.id).unwrap());
  runner.kill(&second.id).unwrap();
  let killed = wait_session(&runner, &second.id, StateKind::Exited);
  println!("killed session ended with {:?}", killed.state);

  assert!(runner.start_run(&run_req("no-such-profile", 414, &stand.project)).unwrap_err().starts_with("unknown_profile: "));
  let bad_cwd = StartRun { cwd: "/".into(), ..run_req("fake", 415, &stand.project) };
  assert!(runner.start_run(&bad_cwd).unwrap_err().starts_with("invalid_params: "));
}
