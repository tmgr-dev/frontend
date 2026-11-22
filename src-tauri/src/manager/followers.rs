use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::pair::{close_code, trusted, Flight};
use super::relay::port::RelayPort;

pub const ENROLL_TIMEOUT: Duration = Duration::from_secs(90);
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(10);
const NAME_MAX: usize = 64;

pub enum CliError {
  NoCli,
  TimedOut,
  Failed(String),
}

pub trait LinkCli: Send + Sync {
  fn run(&self, args: &[&str], stdin: Option<&str>, timeout: Duration) -> Result<String, CliError>;
}

pub struct ProcLinkCli;

impl ProcLinkCli {
  fn command(cli: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(cli);
    cmd.args(args).env_clear();
    for key in ["HOME", "PATH"] {
      if let Some(v) = std::env::var_os(key) {
        cmd.env(key, v);
      }
    }
    cmd
  }
}

pub fn run_command(mut cmd: Command, stdin_text: Option<&str>, timeout: Duration) -> Result<String, CliError> {
  let mut child = cmd
    .stdin(if stdin_text.is_some() { Stdio::piped() } else { Stdio::null() })
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()
    .map_err(|e| CliError::Failed(format!("could not start the tmgr command: {e}")))?;
  if let Some(text) = stdin_text {
    let written = child.stdin.take().map(|mut stdin| stdin.write_all(text.as_bytes()).and_then(|_| stdin.write_all(b"\n")));
    if !matches!(written, Some(Ok(()))) {
      let _ = child.kill();
      let _ = child.wait();
      return Err(CliError::Failed("could not hand the join code to the tmgr command".into()));
    }
  }
  let mut stdout = child.stdout.take().ok_or_else(|| CliError::Failed("no output from the tmgr command".into()))?;
  let (tx, rx) = mpsc::channel();
  std::thread::spawn(move || {
    let mut buf = String::new();
    let _ = stdout.read_to_string(&mut buf);
    let _ = tx.send(buf);
  });
  let deadline = Instant::now() + timeout;
  loop {
    match child.try_wait() {
      Ok(Some(_)) => break,
      Ok(None) if Instant::now() >= deadline => {
        let _ = child.kill();
        let _ = child.wait();
        return Err(CliError::TimedOut);
      }
      Ok(None) => std::thread::sleep(Duration::from_millis(50)),
      Err(e) => return Err(CliError::Failed(e.to_string())),
    }
  }
  rx.recv_timeout(Duration::from_secs(2)).map_err(|_| CliError::Failed("the tmgr command printed no result".into()))
}

impl LinkCli for ProcLinkCli {
  fn run(&self, args: &[&str], stdin: Option<&str>, timeout: Duration) -> Result<String, CliError> {
    let cli = crate::agent_connect::resolve_tmgr_cli().ok_or(CliError::NoCli)?;
    run_command(Self::command(&cli, args), stdin, timeout)
  }
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct FollowerResult {
  pub follower: String,
  pub state: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub machine: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub message: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct FollowerInfo {
  pub follower: String,
  pub chat_enroll: Option<bool>,
}

fn outcome(follower: &str, state: &str, message: Option<String>) -> FollowerResult {
  FollowerResult { follower: follower.into(), state: state.into(), machine: None, message }
}

fn failed(follower: &str, message: impl Into<String>) -> FollowerResult {
  outcome(follower, "error", Some(message.into()))
}

pub fn valid_follower_name(name: &str) -> bool {
  let len = name.trim().chars().count();
  len > 0 && len <= NAME_MAX && !name.starts_with('-') && !name.trim_start().starts_with('-') && !name.chars().any(char::is_control)
}

struct Envelope {
  data: Value,
}

struct EnvelopeError {
  code: String,
  message: String,
}

fn parse_envelope(out: &str) -> Result<Result<Envelope, EnvelopeError>, ()> {
  let line = out.lines().rev().find(|l| !l.trim().is_empty()).ok_or(())?;
  let v: Value = serde_json::from_str(line.trim()).map_err(|_| ())?;
  if v.get("ok").and_then(Value::as_bool) == Some(true) {
    return Ok(Ok(Envelope { data: v.get("data").cloned().unwrap_or(Value::Null) }));
  }
  let err = v.get("error").ok_or(())?;
  let text = |k: &str| err.get(k).and_then(Value::as_str).unwrap_or_default().to_owned();
  Ok(Err(EnvelopeError { code: text("code"), message: text("message") }))
}

fn cli_failure(follower: &str, e: CliError) -> FollowerResult {
  match e {
    CliError::NoCli => outcome(follower, "error", Some("the tmgr command line tool was not found".into())),
    CliError::TimedOut => outcome(follower, "unreachable", Some("the follower did not answer in time".into())),
    CliError::Failed(m) => failed(follower, m),
  }
}

fn envelope_failure(follower: &str, e: EnvelopeError) -> FollowerResult {
  let lower = e.message.to_lowercase();
  if lower.contains("chat_enroll") || lower.contains("chat enroll") || lower.contains("scope") || (e.code == "forbidden" && e.message.is_empty()) {
    return outcome(follower, "refused", Some("the follower's link scope does not allow chat enroll".into()));
  }
  if matches!(e.code.as_str(), "link" | "offline") {
    return outcome(follower, "unreachable", Some(e.message));
  }
  failed(follower, if e.message.is_empty() { "the follower could not be enrolled".to_owned() } else { e.message })
}

#[derive(Deserialize)]
struct Enrolled {
  id: String,
  device_id: String,
  fp: String,
}

pub fn list_followers(cli: &dyn LinkCli) -> Result<Vec<String>, String> {
  let out = cli.run(&["link", "ls", "--json"], None, PROBE_TIMEOUT).map_err(|e| match e {
    CliError::NoCli => "the tmgr command line tool was not found".to_owned(),
    CliError::TimedOut => "tmgr link ls timed out".to_owned(),
    CliError::Failed(m) => m,
  })?;
  let env = parse_envelope(&out).map_err(|_| "tmgr link ls printed an unexpected result".to_owned())?.map_err(|e| e.message)?;
  let names = env.data.get("followers").and_then(Value::as_array).ok_or_else(|| "tmgr link ls printed an unexpected result".to_owned())?;
  Ok(names.iter().filter_map(|f| f.get("name").and_then(Value::as_str)).map(str::to_owned).collect())
}

pub fn followers(cli: &dyn LinkCli) -> Vec<FollowerInfo> {
  let Ok(names) = list_followers(cli) else {
    return vec![];
  };
  names
    .into_iter()
    .map(|follower| {
      let chat_enroll = if valid_follower_name(&follower) {
        cli.run(&["link", "test", &follower, "--json"], None, PROBE_TIMEOUT).ok().and_then(|out| parse_envelope(&out).ok()?.ok()).and_then(|env| env.data.get("chat_enroll").and_then(Value::as_bool))
      } else {
        None
      };
      FollowerInfo { follower, chat_enroll }
    })
    .collect()
}

fn enroll_one(relay: &dyn RelayPort, cli: &dyn LinkCli, follower: &str) -> FollowerResult {
  if !valid_follower_name(follower) {
    return failed(follower, "invalid follower name");
  }
  let code = match relay.open_join_code() {
    Ok(code) => code,
    Err(e) => return failed(follower, e),
  };
  let out = match cli.run(&["link", "chat-enroll", follower, "--json"], Some(&code), ENROLL_TIMEOUT) {
    Ok(out) => out,
    Err(e) => {
      close_code(relay, &code);
      return cli_failure(follower, e);
    }
  };
  let data = match parse_envelope(&out) {
    Ok(Ok(env)) => env.data,
    Ok(Err(e)) => {
      close_code(relay, &code);
      return envelope_failure(follower, e);
    }
    Err(()) => {
      close_code(relay, &code);
      return failed(follower, "the tmgr command printed an unexpected line");
    }
  };
  let enrolled = match serde_json::from_value::<Enrolled>(data) {
    Ok(e) if !e.id.is_empty() && !e.device_id.is_empty() && !e.fp.is_empty() => e,
    _ => {
      close_code(relay, &code);
      return failed(follower, "the tmgr command printed an unexpected line");
    }
  };
  let known = relay.trust_state().is_ok_and(|t| trusted(&t, &enrolled.device_id, &enrolled.fp));
  let state = if known {
    close_code(relay, &code);
    "already"
  } else if let Err(e) = relay.expect_join(&enrolled.device_id, &enrolled.fp) {
    close_code(relay, &code);
    return failed(follower, e);
  } else {
    "enrolled"
  };
  FollowerResult { machine: Some(enrolled.id), ..outcome(follower, state, None) }
}

pub fn connect_followers(relay: &dyn RelayPort, cli: &dyn LinkCli, busy: &AtomicBool) -> Vec<FollowerResult> {
  if busy.swap(true, Ordering::SeqCst) {
    return vec![outcome("", "pending", Some("connecting followers is already running".into()))];
  }
  let _flight = Flight(busy);
  let names = match list_followers(cli) {
    Ok(names) => names,
    Err(e) => return vec![failed("", e)],
  };
  if let Some(blockers) = relay.trust_state().ok().map(|t| t.gate_blockers).filter(|b| !b.is_empty()) {
    let message = format!("update these devices first: {}", blockers.join(", "));
    return names.iter().map(|n| failed(n, message.clone())).collect();
  }
  names.iter().map(|n| enroll_one(relay, cli, n)).collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::manager::backend::{ChatMessage, HistoryParams, RoomInfo, RoomRef, SearchParams, SendParams};
  use crate::manager::relay::port::{DeviceState, TrustDevice, TrustState};
  use serde_json::json;
  use std::collections::HashMap;
  use std::sync::{Arc, Mutex};

  #[derive(Default)]
  struct Calls {
    opened: usize,
    closed: usize,
    expected: Vec<(String, String)>,
    trusted: Vec<(String, String)>,
    blockers: Vec<String>,
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
      let devices = c
        .trusted
        .iter()
        .map(|(id, fp)| TrustDevice {
          id: id.clone(),
          kind: "runner".into(),
          label: "Runner".into(),
          fp: fp.clone(),
          state: DeviceState::Trusted,
          is_self: false,
          kind_verified: true,
          can_confirm_kind: false,
        })
        .collect();
      Ok(TrustState { devices, gate_blockers: c.blockers.clone(), ..TrustState::default() })
    }
    fn decide_join(&self, _: &str, _: bool) -> Result<(), String> {
      Ok(())
    }
    fn revoke(&self, _: &str) -> Result<(), String> {
      Ok(())
    }
    fn open_join_code(&self) -> Result<String, String> {
      let mut c = self.0.lock().unwrap();
      c.opened += 1;
      Ok(format!("tmgr-chat-join:v2:code-{}", c.opened))
    }
    fn close_join_code(&self, _: u64) -> Result<(), String> {
      self.0.lock().unwrap().closed += 1;
      Ok(())
    }
    fn expect_join(&self, device_id: &str, fp: &str) -> Result<(), String> {
      self.0.lock().unwrap().expected.push((device_id.into(), fp.into()));
      Ok(())
    }
    fn reset_identity(&self) -> Result<(), String> {
      Ok(())
    }
    fn poll_once(&self) -> Result<Vec<ChatMessage>, String> {
      Ok(vec![])
    }
  }

  type Seen = Arc<Mutex<Vec<(Vec<String>, Option<String>)>>>;

  struct FakeCli {
    replies: HashMap<String, Result<String, CliErrorKind>>,
    list: String,
    seen: Seen,
  }

  #[derive(Clone)]
  enum CliErrorKind {
    TimedOut,
  }

  impl LinkCli for FakeCli {
    fn run(&self, args: &[&str], stdin: Option<&str>, _: Duration) -> Result<String, CliError> {
      self.seen.lock().unwrap().push((args.iter().map(|a| (*a).to_owned()).collect(), stdin.map(str::to_owned)));
      if args[1] == "ls" {
        return Ok(self.list.clone());
      }
      let key = format!("{} {}", args[1], args[2]);
      match self.replies.get(&key).cloned().unwrap_or_else(|| Ok(String::new())) {
        Ok(s) => Ok(s),
        Err(CliErrorKind::TimedOut) => Err(CliError::TimedOut),
      }
    }
  }

  fn list_of(names: &[&str]) -> String {
    json!({"schema": "tmgr.v1", "ok": true, "data": {"followers": names.iter().map(|n| json!({"name": n})).collect::<Vec<_>>(), "leaders": []}, "warnings": []}).to_string()
  }

  fn enrolled(id: &str, dev: &str, fp: &str) -> String {
    json!({"schema": "tmgr.v1", "ok": true, "data": {"id": id, "device_id": dev, "fp": fp, "follower": "x"}, "warnings": []}).to_string()
  }

  fn refusal(code: &str, message: &str) -> String {
    json!({"schema": "tmgr.v1", "ok": false, "error": {"code": code, "message": message, "exit": 5}}).to_string()
  }

  fn cli(names: &[&str], replies: Vec<(&str, Result<String, CliErrorKind>)>) -> (FakeCli, Seen) {
    let seen = Seen::default();
    let replies = replies.into_iter().map(|(k, v)| (k.to_owned(), v)).collect();
    (FakeCli { replies, list: list_of(names), seen: seen.clone() }, seen)
  }

  fn run(relay: &FakeRelay, cli: &FakeCli) -> Vec<FollowerResult> {
    connect_followers(relay, cli, &AtomicBool::new(false))
  }

  fn states(out: &[FollowerResult]) -> Vec<(&str, &str)> {
    out.iter().map(|r| (r.follower.as_str(), r.state.as_str())).collect()
  }

  #[test]
  fn two_followers_are_enrolled_and_each_identity_is_expected() {
    let relay = FakeRelay::default();
    let (cli, seen) = cli(&["studio", "server"], vec![("chat-enroll studio", Ok(enrolled("mx_a", "dev-a", "fa"))), ("chat-enroll server", Ok(enrolled("mx_b", "dev-b", "fb")))]);
    let out = run(&relay, &cli);
    assert_eq!(states(&out), [("studio", "enrolled"), ("server", "enrolled")]);
    assert_eq!(out[1].machine.as_deref(), Some("mx_b"));
    let c = relay.0.lock().unwrap();
    assert_eq!(c.opened, 2);
    assert_eq!(c.expected, [("dev-a".to_owned(), "fa".to_owned()), ("dev-b".to_owned(), "fb".to_owned())]);
    let seen = seen.lock().unwrap();
    let enroll: Vec<_> = seen.iter().filter(|(a, _)| a[1] == "chat-enroll").collect();
    assert_eq!(enroll.len(), 2);
    assert!(enroll.iter().all(|(a, stdin)| a.iter().all(|x| !x.contains("code-")) && stdin.as_deref().is_some_and(|s| s.starts_with("tmgr-chat-join:"))));
  }

  #[test]
  fn a_scope_refusal_is_mapped_and_closes_the_code() {
    let relay = FakeRelay::default();
    let (cli, _) = cli(&["studio"], vec![("chat-enroll studio", Ok(refusal("forbidden", "the follower's scope does not allow chat enrolment")))]);
    let out = run(&relay, &cli);
    assert_eq!(states(&out), [("studio", "refused")]);
    assert_eq!(out[0].message.as_deref(), Some("the follower's link scope does not allow chat enroll"));
    let c = relay.0.lock().unwrap();
    assert!(c.expected.is_empty());
  }

  #[test]
  fn a_leader_binding_refusal_keeps_the_cli_reason() {
    let relay = FakeRelay::default();
    let (cli, _) = cli(&["studio"], vec![("chat-enroll studio", Ok(refusal("forbidden", "this link is bound to a different leader machine")))]);
    let out = run(&relay, &cli);
    assert_eq!(states(&out), [("studio", "error")]);
    assert_eq!(out[0].message.as_deref(), Some("this link is bound to a different leader machine"));
    assert!(relay.0.lock().unwrap().expected.is_empty());
  }

  #[test]
  fn connection_failures_are_unreachable() {
    let relay = FakeRelay::default();
    let (cli, _) =
      cli(&["a", "b", "c"], vec![("chat-enroll a", Ok(refusal("link", "connection refused"))), ("chat-enroll b", Ok(refusal("offline", "no route"))), ("chat-enroll c", Err(CliErrorKind::TimedOut))]);
    let out = run(&relay, &cli);
    assert_eq!(states(&out), [("a", "unreachable"), ("b", "unreachable"), ("c", "unreachable")]);
    assert!(relay.0.lock().unwrap().expected.is_empty());
  }

  #[test]
  fn other_cli_errors_carry_their_message() {
    let relay = FakeRelay::default();
    let (cli, _) = cli(&["a"], vec![("chat-enroll a", Ok(refusal("auth", "not logged in as the same user")))]);
    let out = run(&relay, &cli);
    assert_eq!((out[0].state.as_str(), out[0].message.as_deref()), ("error", Some("not logged in as the same user")));
  }

  #[test]
  fn bad_json_is_an_error_and_nothing_is_expected() {
    let relay = FakeRelay::default();
    let (cli, _) = cli(&["a", "b"], vec![("chat-enroll a", Ok("not json".into())), ("chat-enroll b", Ok(json!({"schema": "tmgr.v1", "ok": true, "data": {"id": "mx_b"}}).to_string()))]);
    let out = run(&relay, &cli);
    assert_eq!(states(&out), [("a", "error"), ("b", "error")]);
    let c = relay.0.lock().unwrap();
    assert!(c.expected.is_empty());
    assert_eq!(c.closed, 0);
  }

  #[test]
  fn a_name_with_a_leading_dash_is_rejected_without_spawning() {
    let relay = FakeRelay::default();
    let (cli, seen) = cli(&["--json", " -x", "ok"], vec![("chat-enroll ok", Ok(enrolled("mx_a", "dev-a", "fa")))]);
    let out = run(&relay, &cli);
    assert_eq!(states(&out), [("--json", "error"), (" -x", "error"), ("ok", "enrolled")]);
    let seen = seen.lock().unwrap();
    assert!(seen.iter().filter(|(a, _)| a[1] != "ls").all(|(a, _)| a.get(2).is_none_or(|n| !n.starts_with('-') && !n.starts_with(' '))));
    assert_eq!(relay.0.lock().unwrap().opened, 1);
  }

  #[test]
  fn an_already_trusted_identity_is_already_and_not_expected() {
    let relay = FakeRelay::default();
    relay.0.lock().unwrap().trusted = vec![("dev-a".into(), "fa".into())];
    let (cli, _) = cli(&["a"], vec![("chat-enroll a", Ok(enrolled("mx_a", "dev-a", "fa")))]);
    let out = run(&relay, &cli);
    assert_eq!((out[0].state.as_str(), out[0].machine.as_deref()), ("already", Some("mx_a")));
    let c = relay.0.lock().unwrap();
    assert!(c.expected.is_empty());
    assert_eq!(c.closed, 0);
  }

  #[test]
  fn a_closed_gate_reports_blockers_without_opening_codes() {
    let relay = FakeRelay::default();
    relay.0.lock().unwrap().blockers = vec!["Phone".into()];
    let (cli, _) = cli(&["a"], vec![]);
    let out = run(&relay, &cli);
    assert_eq!(states(&out), [("a", "error")]);
    assert!(out[0].message.as_deref().unwrap().contains("Phone"));
    assert_eq!(relay.0.lock().unwrap().opened, 0);
  }

  #[test]
  fn a_missing_cli_or_list_failure_is_a_single_error() {
    struct NoCliAtAll;
    impl LinkCli for NoCliAtAll {
      fn run(&self, _: &[&str], _: Option<&str>, _: Duration) -> Result<String, CliError> {
        Err(CliError::NoCli)
      }
    }
    let out = connect_followers(&FakeRelay::default(), &NoCliAtAll, &AtomicBool::new(false));
    assert_eq!(states(&out), [("", "error")]);
    assert!(followers(&NoCliAtAll).is_empty());
  }

  #[test]
  fn a_second_call_while_one_runs_is_pending_and_the_flag_is_released() {
    let relay = FakeRelay::default();
    let (cli, _) = cli(&[], vec![]);
    let busy = AtomicBool::new(true);
    assert_eq!(connect_followers(&relay, &cli, &busy)[0].state, "pending");
    assert!(busy.load(Ordering::SeqCst));
    busy.store(false, Ordering::SeqCst);
    connect_followers(&relay, &cli, &busy);
    assert!(!busy.load(Ordering::SeqCst));
  }

  #[test]
  fn followers_report_the_chat_enroll_flag_with_null_for_old_followers() {
    let probe = |v: Value| json!({"schema": "tmgr.v1", "ok": true, "data": v}).to_string();
    let (cli, _) = cli(
      &["new", "old", "off", "down"],
      vec![
        ("test new", Ok(probe(json!({"link": "new", "chat_enroll": true})))),
        ("test old", Ok(probe(json!({"link": "old", "chat_enroll": null})))),
        ("test off", Ok(probe(json!({"link": "off", "chat_enroll": false})))),
        ("test down", Ok(refusal("link", "unreachable"))),
      ],
    );
    let got: Vec<_> = followers(&cli).into_iter().map(|f| (f.follower, f.chat_enroll)).collect();
    assert_eq!(got, [("new".to_owned(), Some(true)), ("old".to_owned(), None), ("off".to_owned(), Some(false)), ("down".to_owned(), None)]);
  }

  #[test]
  fn names_follow_the_cli_rules() {
    assert!(valid_follower_name("my host"));
    assert!(!valid_follower_name(""));
    assert!(!valid_follower_name("-x"));
    assert!(!valid_follower_name("a\nb"));
    assert!(!valid_follower_name(&"x".repeat(65)));
  }

  #[test]
  fn the_command_line_carries_only_home_and_path() {
    let cmd = ProcLinkCli::command(Path::new("/bin/tmgr"), &["link", "ls", "--json"]);
    let envs: Vec<_> = cmd.get_envs().map(|(k, _)| k.to_string_lossy().into_owned()).collect();
    assert!(envs.iter().all(|k| k == "HOME" || k == "PATH"));
  }

  #[cfg(unix)]
  #[test]
  fn a_real_child_gets_stdin_and_a_slow_one_times_out() {
    let mut cmd = Command::new("/bin/sh");
    cmd.args(["-c", "read code; printf '%s\\n' \"$code\""]);
    assert_eq!(run_command(cmd, Some("the-code"), Duration::from_secs(5)).ok().as_deref(), Some("the-code\n"));
    let mut slow = Command::new("/bin/sh");
    slow.args(["-c", "sleep 30"]);
    assert!(matches!(run_command(slow, None, Duration::from_millis(100)), Err(CliError::TimedOut)));
  }
}
