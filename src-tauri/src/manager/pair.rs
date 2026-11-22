use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tmgr_chat_relay::trust::decode_join_code;

use super::local::ChatCall;
use super::relay::port::{DeviceState, RelayPort, TrustState};

const FIRST_LINE_TIMEOUT: Duration = Duration::from_secs(30);
const WAIT_TIMEOUT: Duration = Duration::from_secs(180);
const WAIT_SLICE: Duration = Duration::from_secs(1);
const JOIN_ARGS: [&str; 3] = ["chat", "join", "--json"];

#[derive(Clone, Copy)]
pub struct Timeouts {
  pub first_line: Duration,
  pub wait: Duration,
  pub slice: Duration,
}

impl Default for Timeouts {
  fn default() -> Self {
    Self { first_line: FIRST_LINE_TIMEOUT, wait: WAIT_TIMEOUT, slice: WAIT_SLICE }
  }
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct PairResult {
  pub state: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub machine: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub device_id: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub message: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub blockers: Option<Vec<String>>,
}

fn result(state: &str, message: Option<String>) -> PairResult {
  PairResult { state: state.into(), message, ..PairResult::default() }
}

fn failed(message: impl Into<String>) -> PairResult {
  result("error", Some(message.into()))
}

fn blocked(labels: Vec<String>, detail: Option<String>) -> PairResult {
  let message = detail.unwrap_or_else(|| format!("update these devices first: {}", labels.join(", ")));
  PairResult { blockers: Some(labels), ..result("blocked", Some(message)) }
}

pub enum SpawnError {
  NoCli,
  Failed(String),
}

pub trait JoinChild: Send {
  fn first_line(&mut self, timeout: Duration) -> Result<String, String>;
  fn wait(&mut self, timeout: Duration) -> Result<Option<bool>, String>;
  fn kill(&mut self);
}

pub trait JoinSpawner: Send + Sync {
  fn spawn(&self, code: &str) -> Result<Box<dyn JoinChild>, SpawnError>;
}

pub struct ProcChild {
  child: Child,
  lines: mpsc::Receiver<String>,
}

impl ProcChild {
  pub fn start(mut cmd: Command, stdin_text: &str) -> Result<Self, String> {
    let mut child = cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| format!("could not start the tmgr command: {e}"))?;
    let written = child.stdin.take().map(|mut stdin| stdin.write_all(stdin_text.as_bytes()).and_then(|_| stdin.write_all(b"\n")));
    if !matches!(written, Some(Ok(()))) {
      let _ = child.kill();
      let _ = child.wait();
      return Err("could not hand the join code to the tmgr command".into());
    }
    let stdout = child.stdout.take().ok_or_else(|| "no output from the tmgr command".to_owned())?;
    let (tx, lines) = mpsc::channel();
    std::thread::spawn(move || {
      for line in BufReader::new(stdout).lines().map_while(Result::ok) {
        if tx.send(line).is_err() {
          break;
        }
      }
    });
    Ok(Self { child, lines })
  }
}

impl JoinChild for ProcChild {
  fn first_line(&mut self, timeout: Duration) -> Result<String, String> {
    self.lines.recv_timeout(timeout).map_err(|_| "the tmgr command did not report the runner identity".to_owned())
  }

  fn wait(&mut self, timeout: Duration) -> Result<Option<bool>, String> {
    let deadline = Instant::now() + timeout;
    loop {
      if let Some(status) = self.child.try_wait().map_err(|e| e.to_string())? {
        return Ok(Some(status.success()));
      }
      if Instant::now() >= deadline {
        return Ok(None);
      }
      std::thread::sleep(Duration::from_millis(100));
    }
  }

  fn kill(&mut self) {
    let _ = self.child.kill();
    let _ = self.child.wait();
  }
}

pub struct CliSpawner;

impl CliSpawner {
  fn command(cli: &Path) -> Command {
    let mut cmd = Command::new(cli);
    cmd.args(JOIN_ARGS).env_clear();
    for key in ["HOME", "PATH"] {
      if let Some(v) = std::env::var_os(key) {
        cmd.env(key, v);
      }
    }
    cmd
  }
}

impl JoinSpawner for CliSpawner {
  fn spawn(&self, code: &str) -> Result<Box<dyn JoinChild>, SpawnError> {
    let cli = crate::agent_connect::resolve_tmgr_cli().ok_or(SpawnError::NoCli)?;
    ProcChild::start(Self::command(&cli), code).map(|c| Box::new(c) as Box<dyn JoinChild>).map_err(SpawnError::Failed)
  }
}

#[derive(Deserialize)]
struct RelaySelf {
  id: String,
  device_id: String,
  fp: String,
}

#[derive(Deserialize)]
struct JoinLine {
  id: String,
  device_id: String,
  fp: String,
  #[serde(default)]
  already_joined: bool,
}

struct Flight<'a>(&'a AtomicBool);

impl Drop for Flight<'_> {
  fn drop(&mut self) {
    self.0.store(false, Ordering::SeqCst);
  }
}

fn trusted(ts: &TrustState, device_id: &str, fp: &str) -> bool {
  ts.devices.iter().any(|d| d.id == device_id && d.fp == fp && d.state == DeviceState::Trusted)
}

fn close_code(relay: &dyn RelayPort, code: &str) {
  if let Ok(c) = decode_join_code(code) {
    let _ = relay.close_join_code(c.exp_ms);
  }
}

fn stop(relay: &dyn RelayPort, code: &str, child: &mut dyn JoinChild, out: PairResult) -> PairResult {
  child.kill();
  close_code(relay, code);
  out
}

fn after_failure(relay: &dyn RelayPort, fallback: &str) -> PairResult {
  let ts = relay.trust_state().unwrap_or_default();
  match (ts.gate_blockers.is_empty(), ts.auto_join_error) {
    (false, detail) => blocked(ts.gate_blockers, detail),
    (true, Some(detail)) => failed(detail),
    (true, None) => failed(fallback),
  }
}

pub struct NoRunner;

impl ChatCall for NoRunner {
  fn call(&self, _: &str, _: Value) -> Result<Value, String> {
    Err("the runner is not available".into())
  }
}

pub fn pair_local_runner(relay: &dyn RelayPort, runner: &dyn ChatCall, spawner: &dyn JoinSpawner, busy: &AtomicBool, timeouts: Timeouts) -> PairResult {
  if busy.swap(true, Ordering::SeqCst) {
    return result("pending", Some("pairing is already running".into()));
  }
  let _flight = Flight(busy);
  let own: Option<RelaySelf> = runner.call("chat.relay_self", Value::Null).ok().and_then(|v| serde_json::from_value(v).ok());
  let mut ids: Option<(String, String)> = own.as_ref().map(|o| (o.id.clone(), o.device_id.clone()));
  let with_ids = |ids: &Option<(String, String)>, mut r: PairResult| {
    if let Some((machine, device_id)) = ids {
      r.machine = Some(machine.clone());
      r.device_id = Some(device_id.clone());
    }
    r
  };
  let ts = match relay.trust_state() {
    Ok(ts) => ts,
    Err(e) => return with_ids(&ids, failed(e)),
  };
  if own.as_ref().is_some_and(|o| trusted(&ts, &o.device_id, &o.fp)) {
    return with_ids(&ids, result("already", None));
  }
  if !ts.gate_blockers.is_empty() {
    return with_ids(&ids, blocked(ts.gate_blockers, None));
  }
  let code = match relay.open_join_code() {
    Ok(code) => code,
    Err(e) => return with_ids(&ids, failed(e)),
  };
  let mut child = match spawner.spawn(&code) {
    Ok(child) => child,
    Err(SpawnError::NoCli) => {
      close_code(relay, &code);
      return with_ids(&ids, result("no_cli", Some("the tmgr command line tool was not found".into())));
    }
    Err(SpawnError::Failed(e)) => {
      close_code(relay, &code);
      return with_ids(&ids, failed(e));
    }
  };
  let line = match child.first_line(timeouts.first_line) {
    Ok(line) => line,
    Err(e) => return with_ids(&ids, stop(relay, &code, &mut *child, failed(e))),
  };
  let Ok(parsed) = serde_json::from_str::<JoinLine>(line.trim()) else {
    return with_ids(&ids, stop(relay, &code, &mut *child, failed("the tmgr command printed an unexpected line")));
  };
  if own.as_ref().is_some_and(|o| parsed.device_id != o.device_id || parsed.fp != o.fp) {
    return with_ids(&ids, stop(relay, &code, &mut *child, failed("the joining runner does not match this computer's runner")));
  }
  ids = Some((parsed.id.clone(), parsed.device_id.clone()));
  if parsed.already_joined {
    let known = relay.trust_state().is_ok_and(|t| trusted(&t, &parsed.device_id, &parsed.fp));
    let out = if known { result("already", None) } else { failed("this computer's runner already joined a chat that this app does not trust") };
    return with_ids(&ids, stop(relay, &code, &mut *child, out));
  }
  if let Err(e) = relay.expect_join(&parsed.device_id, &parsed.fp) {
    return with_ids(&ids, stop(relay, &code, &mut *child, failed(e)));
  }
  let deadline = Instant::now() + timeouts.wait;
  let exited = loop {
    match child.wait(timeouts.slice) {
      Ok(Some(success)) => break success,
      Ok(None) => {
        if relay.trust_state().is_ok_and(|t| t.auto_join_error.is_some()) {
          return with_ids(&ids, stop(relay, &code, &mut *child, after_failure(relay, "the join did not complete")));
        }
        if Instant::now() >= deadline {
          return with_ids(&ids, stop(relay, &code, &mut *child, result("pending", Some("the runner is waiting for approval".into()))));
        }
      }
      Err(e) => return with_ids(&ids, stop(relay, &code, &mut *child, failed(e))),
    }
  };
  if !exited {
    return with_ids(&ids, after_failure(relay, "the join did not complete"));
  }
  match relay.trust_state() {
    Ok(ts) if trusted(&ts, &parsed.device_id, &parsed.fp) => with_ids(&ids, result("paired", None)),
    _ => with_ids(&ids, result("pending", Some("the runner is waiting for approval".into()))),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::manager::backend::{ChatMessage, HistoryParams, RoomInfo, RoomRef, SearchParams, SendParams};
  use crate::manager::relay::port::TrustDevice;
  use serde_json::json;
  use std::collections::VecDeque;
  use std::sync::{Arc, Mutex};

  const DEV: &str = "00000000-0000-4000-8000-00000000000a";
  const FP: &str = "ab";

  #[derive(Default)]
  struct Calls {
    opened: usize,
    closed: usize,
    expected: Vec<(String, String)>,
    trusted: bool,
    blockers: Vec<String>,
    auto_error: Option<String>,
    late: bool,
    stay_untrusted: bool,
  }

  #[derive(Clone, Default)]
  struct FakeRelay(Arc<Mutex<Calls>>);

  impl RelayPort for FakeRelay {
    fn rooms(&self) -> Result<Vec<RoomInfo>, String> {
      Ok(vec![])
    }
    fn history(&self, _: &HistoryParams) -> Result<Vec<ChatMessage>, String> {
      Ok(vec![])
    }
    fn search(&self, _: &SearchParams) -> Result<Vec<ChatMessage>, String> {
      Ok(vec![])
    }
    fn send(&self, _: SendParams) -> Result<ChatMessage, String> {
      Err("no".into())
    }
    fn owns(&self, _: &RoomRef) -> bool {
      false
    }
    fn trust_state(&self) -> Result<TrustState, String> {
      let c = self.0.lock().unwrap();
      let devices = if c.trusted {
        vec![TrustDevice { id: DEV.into(), kind: "runner".into(), label: "Runner".into(), fp: FP.into(), state: DeviceState::Trusted, is_self: false, kind_verified: true, can_confirm_kind: false }]
      } else {
        vec![]
      };
      let visible = !c.late || !c.expected.is_empty();
      Ok(TrustState {
        devices,
        gate_blockers: if visible { c.blockers.clone() } else { vec![] },
        auto_join_error: if visible { c.auto_error.clone() } else { None },
        ..TrustState::default()
      })
    }
    fn decide_join(&self, _: &str, _: bool) -> Result<(), String> {
      Ok(())
    }
    fn revoke(&self, _: &str) -> Result<(), String> {
      Ok(())
    }
    fn open_join_code(&self) -> Result<String, String> {
      self.0.lock().unwrap().opened += 1;
      Ok("tmgr-chat-join:v2:not-decodable".into())
    }
    fn close_join_code(&self, _: u64) -> Result<(), String> {
      self.0.lock().unwrap().closed += 1;
      Ok(())
    }
    fn expect_join(&self, device_id: &str, fp: &str) -> Result<(), String> {
      let mut c = self.0.lock().unwrap();
      c.expected.push((device_id.into(), fp.into()));
      c.trusted = c.auto_error.is_none() && !c.stay_untrusted;
      Ok(())
    }
    fn reset_identity(&self) -> Result<(), String> {
      Ok(())
    }
    fn poll_once(&self) -> Result<Vec<ChatMessage>, String> {
      Ok(vec![])
    }
  }

  struct FakeRunner(Result<Value, String>);

  impl ChatCall for FakeRunner {
    fn call(&self, method: &str, _: Value) -> Result<Value, String> {
      assert_eq!(method, "chat.relay_self");
      self.0.clone()
    }
  }

  fn runner_ok() -> FakeRunner {
    FakeRunner(Ok(json!({ "id": "mx_a", "device_id": DEV, "fp": FP })))
  }

  struct FakeChild {
    line: Result<String, String>,
    waits: VecDeque<Option<bool>>,
    killed: Arc<Mutex<bool>>,
  }

  impl JoinChild for FakeChild {
    fn first_line(&mut self, _: Duration) -> Result<String, String> {
      self.line.clone()
    }
    fn wait(&mut self, slice: Duration) -> Result<Option<bool>, String> {
      let next = self.waits.pop_front().unwrap_or(Some(true));
      if next.is_none() {
        std::thread::sleep(slice);
      }
      Ok(next)
    }
    fn kill(&mut self) {
      *self.killed.lock().unwrap() = true;
    }
  }

  struct FakeSpawner {
    spawn: Mutex<Option<Result<FakeChild, SpawnError>>>,
    codes: Mutex<Vec<String>>,
  }

  impl JoinSpawner for FakeSpawner {
    fn spawn(&self, code: &str) -> Result<Box<dyn JoinChild>, SpawnError> {
      self.codes.lock().unwrap().push(code.into());
      self.spawn.lock().unwrap().take().unwrap().map(|c| Box::new(c) as Box<dyn JoinChild>)
    }
  }

  fn spawner(child: Result<FakeChild, SpawnError>) -> FakeSpawner {
    FakeSpawner { spawn: Mutex::new(Some(child)), codes: Mutex::new(vec![]) }
  }

  fn child(line: &str, waits: &[Option<bool>]) -> FakeChild {
    FakeChild { line: Ok(line.into()), waits: waits.iter().copied().collect(), killed: Arc::default() }
  }

  fn good_line() -> String {
    json!({ "id": "mx_a", "device_id": DEV, "fp": FP, "already_joined": false }).to_string()
  }

  fn timeouts() -> Timeouts {
    Timeouts { first_line: Duration::from_millis(10), wait: Duration::from_millis(50), slice: Duration::from_millis(1) }
  }

  fn run(relay: &FakeRelay, runner: &FakeRunner, sp: &FakeSpawner) -> PairResult {
    pair_local_runner(relay, runner, sp, &AtomicBool::new(false), timeouts())
  }

  #[test]
  fn an_already_trusted_runner_opens_no_code() {
    let relay = FakeRelay::default();
    relay.0.lock().unwrap().trusted = true;
    let sp = spawner(Err(SpawnError::NoCli));
    let out = run(&relay, &runner_ok(), &sp);
    assert_eq!((out.state.as_str(), out.machine.as_deref(), out.device_id.as_deref()), ("already", Some("mx_a"), Some(DEV)));
    assert_eq!(relay.0.lock().unwrap().opened, 0);
    assert!(sp.codes.lock().unwrap().is_empty());
  }

  #[test]
  fn an_unjoined_runner_gets_a_code_and_the_parsed_identity_is_expected() {
    let relay = FakeRelay::default();
    let sp = spawner(Ok(child(&good_line(), &[Some(true)])));
    let out = run(&relay, &runner_ok(), &sp);
    assert_eq!(out.state, "paired");
    assert_eq!(out.machine.as_deref(), Some("mx_a"));
    let c = relay.0.lock().unwrap();
    assert_eq!(c.opened, 1);
    assert_eq!(c.expected, [(DEV.to_owned(), FP.to_owned())]);
    assert_eq!(sp.codes.lock().unwrap().len(), 1);
  }

  #[test]
  fn a_bad_first_line_is_an_error_without_an_expectation() {
    let relay = FakeRelay::default();
    let c = child("not json", &[]);
    let killed = c.killed.clone();
    let out = run(&relay, &runner_ok(), &spawner(Ok(c)));
    assert_eq!(out.state, "error");
    assert!(relay.0.lock().unwrap().expected.is_empty());
    assert!(*killed.lock().unwrap());
    assert_eq!(relay.0.lock().unwrap().closed, 0);
  }

  #[test]
  fn a_line_for_another_identity_is_an_error_without_an_expectation() {
    let relay = FakeRelay::default();
    let other = json!({ "id": "mx_a", "device_id": DEV, "fp": "cd" }).to_string();
    let out = run(&relay, &runner_ok(), &spawner(Ok(child(&other, &[]))));
    assert_eq!(out.state, "error");
    assert!(relay.0.lock().unwrap().expected.is_empty());
  }

  #[test]
  fn an_already_joined_line_is_not_expected() {
    let relay = FakeRelay::default();
    let line = json!({ "id": "mx_a", "device_id": DEV, "fp": FP, "already_joined": true }).to_string();
    let out = run(&relay, &runner_ok(), &spawner(Ok(child(&line, &[]))));
    assert_eq!(out.state, "error");
    assert!(relay.0.lock().unwrap().expected.is_empty());
  }

  #[test]
  fn a_missing_cli_is_no_cli() {
    let relay = FakeRelay::default();
    let out = run(&relay, &runner_ok(), &spawner(Err(SpawnError::NoCli)));
    assert_eq!(out.state, "no_cli");
    assert!(relay.0.lock().unwrap().expected.is_empty());
  }

  fn fresh_runner() -> FakeRunner {
    FakeRunner(Err("the chat relay is not configured or not ready on this runner".into()))
  }

  #[test]
  fn a_fresh_runner_without_relay_self_pairs_through_the_cli_line() {
    let relay = FakeRelay::default();
    let sp = spawner(Ok(child(&good_line(), &[Some(true)])));
    let out = run(&relay, &fresh_runner(), &sp);
    assert_eq!((out.state.as_str(), out.machine.as_deref(), out.device_id.as_deref()), ("paired", Some("mx_a"), Some(DEV)));
    let c = relay.0.lock().unwrap();
    assert_eq!(c.opened, 1);
    assert_eq!(c.expected, [(DEV.to_owned(), FP.to_owned())]);
  }

  #[test]
  fn a_fresh_runner_with_no_cli_is_no_cli() {
    let relay = FakeRelay::default();
    let out = run(&relay, &fresh_runner(), &spawner(Err(SpawnError::NoCli)));
    assert_eq!(out.state, "no_cli");
    assert_eq!(relay.0.lock().unwrap().opened, 1);
  }

  #[test]
  fn an_already_joined_line_that_this_app_trusts_is_already_and_closes_the_code() {
    let relay = FakeRelay::default();
    relay.0.lock().unwrap().trusted = true;
    let line = json!({ "id": "mx_a", "device_id": DEV, "fp": FP, "already_joined": true }).to_string();
    let c = child(&line, &[]);
    let killed = c.killed.clone();
    let out = run(&relay, &fresh_runner(), &spawner(Ok(c)));
    assert_eq!((out.state.as_str(), out.machine.as_deref()), ("already", Some("mx_a")));
    assert!(relay.0.lock().unwrap().expected.is_empty());
    assert!(*killed.lock().unwrap());
  }

  #[test]
  fn a_closed_gate_is_blocked_before_any_code_is_opened() {
    let relay = FakeRelay::default();
    relay.0.lock().unwrap().blockers = vec!["Phone".into(), "Laptop".into()];
    let out = run(&relay, &runner_ok(), &spawner(Err(SpawnError::NoCli)));
    assert_eq!(out.state, "blocked");
    assert_eq!(out.blockers, Some(vec!["Phone".to_owned(), "Laptop".to_owned()]));
    assert!(out.message.unwrap().contains("Phone, Laptop"));
    assert_eq!(relay.0.lock().unwrap().opened, 0);
  }

  #[test]
  fn a_gate_failure_during_the_join_is_blocked_and_stops_the_child() {
    let relay = FakeRelay::default();
    let c = child(&good_line(), &[None, None, None]);
    let killed = c.killed.clone();
    {
      let mut s = relay.0.lock().unwrap();
      s.auto_error = Some("update these devices first: Phone".into());
      s.blockers = vec!["Phone".into()];
      s.late = true;
    }
    let out = run(&relay, &runner_ok(), &spawner(Ok(c)));
    assert_eq!(out.state, "blocked");
    assert_eq!(out.blockers, Some(vec!["Phone".to_owned()]));
    assert!(*killed.lock().unwrap());
  }

  #[test]
  fn a_wait_that_runs_out_is_pending() {
    let relay = FakeRelay::default();
    relay.0.lock().unwrap().stay_untrusted = true;
    let c = child(&good_line(), &vec![None; 10_000]);
    let killed = c.killed.clone();
    let out = run(&relay, &runner_ok(), &spawner(Ok(c)));
    assert_eq!(out.state, "pending");
    assert!(*killed.lock().unwrap());
  }

  #[test]
  fn a_second_call_while_one_is_running_is_pending() {
    let relay = FakeRelay::default();
    let busy = AtomicBool::new(true);
    let out = pair_local_runner(&relay, &runner_ok(), &spawner(Err(SpawnError::NoCli)), &busy, timeouts());
    assert_eq!(out.state, "pending");
    assert_eq!(relay.0.lock().unwrap().opened, 0);
    assert!(busy.load(Ordering::SeqCst));
    let out = run(&relay, &runner_ok(), &spawner(Err(SpawnError::NoCli)));
    assert_eq!(out.state, "no_cli");
  }

  #[test]
  fn the_flight_flag_is_released_after_a_run() {
    let relay = FakeRelay::default();
    let busy = AtomicBool::new(false);
    pair_local_runner(&relay, &runner_ok(), &spawner(Err(SpawnError::NoCli)), &busy, timeouts());
    assert!(!busy.load(Ordering::SeqCst));
  }

  #[test]
  fn the_command_line_never_carries_the_code() {
    let cmd = CliSpawner::command(Path::new("/bin/tmgr"));
    let args: Vec<_> = cmd.get_args().map(|a| a.to_string_lossy().into_owned()).collect();
    assert_eq!(args, ["chat", "join", "--json"]);
    let envs: Vec<_> = cmd.get_envs().map(|(k, _)| k.to_string_lossy().into_owned()).collect();
    assert!(envs.iter().all(|k| k == "HOME" || k == "PATH"));
  }

  #[cfg(unix)]
  #[test]
  fn a_real_child_gets_the_code_on_stdin_and_reports_its_first_line() {
    let mut cmd = Command::new("/bin/sh");
    cmd.args(["-c", "read code; printf '%s\\n' \"{\\\"got\\\":\\\"$code\\\"}\"; exit 0"]);
    let mut child = ProcChild::start(cmd, "the-code").unwrap();
    assert_eq!(child.first_line(Duration::from_secs(5)).unwrap(), r#"{"got":"the-code"}"#);
    assert_eq!(child.wait(Duration::from_secs(5)).unwrap(), Some(true));
  }

  #[cfg(unix)]
  #[test]
  fn a_failing_real_child_reports_failure_and_a_silent_one_times_out() {
    let mut cmd = Command::new("/bin/sh");
    cmd.args(["-c", "cat >/dev/null; exit 3"]);
    let mut child = ProcChild::start(cmd, "x").unwrap();
    assert_eq!(child.wait(Duration::from_secs(5)).unwrap(), Some(false));
    let mut cmd = Command::new("/bin/sh");
    cmd.args(["-c", "sleep 30"]);
    let mut slow = ProcChild::start(cmd, "x").unwrap();
    assert!(slow.first_line(Duration::from_millis(50)).is_err());
    assert_eq!(slow.wait(Duration::from_millis(50)).unwrap(), None);
    slow.kill();
  }
}
