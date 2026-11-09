use std::collections::HashMap;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use serde_json::json;
use tauri::{AppHandle, Manager, Runtime};
use tmgr_ipc::decisions::Event;

use super::hub::Hub;
use super::Decisions;

#[allow(unused_imports)]
pub use super::store::delivery_target;

const DELIVERY_MAX: usize = 4000;
const SOCK_DIRS: [&str; 2] = ["/tmp/cc-socks/", "/private/tmp/cc-socks/"];
const SOCK_TIMEOUT: Duration = Duration::from_millis(500);
const QUEUE_EVERY: Duration = Duration::from_secs(2);
const QUEUE_MAX: Duration = Duration::from_secs(2 * 3600);
const WAITING: &str = "Waiting for the agent's prompt";
const UNCONFIRMABLE: &str = "Can't confirm the agent's prompt";
const UNREAD: &str = "Waiting for the agent to read it";
pub const CHANGED: &str = "The agent session changed before delivery";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RouteOutcome {
  Listening,
  Sent,
  Queued,
  Undelivered,
  Skipped,
  TargetChanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushErr {
  Gone,
  Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunErr {
  Busy,
  Gone,
  Other,
}

pub trait SocketSender {
  fn send(&self, endpoint: &str, token: &str, text: &str) -> Result<(), PushErr>;
}

pub trait RunnerSender {
  fn send_at_prompt(&self, session: &str, text: &str) -> Result<(), RunErr>;
}

fn flatten(s: &str) -> Option<String> {
  let flat: String = s
    .chars()
    .map(|c| if c.is_control() { ' ' } else { c })
    .collect::<String>()
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ");
  let flat: String = flat.chars().take(DELIVERY_MAX).collect();
  (!flat.is_empty()).then_some(flat)
}

pub fn delivery_text(event: &Event) -> Option<String> {
  if event.dismissed || event.deferred || event.kind != "answer" {
    return None;
  }
  let mut parts: Vec<String> = Vec::new();
  parts.extend(event.free_text.clone());
  parts.extend(event.note.clone());
  for c in &event.line_comments {
    parts.push(format!("{}:{}: {}", c.file, c.line, c.text));
  }
  flatten(&parts.join("; "))
}

fn text_for(reply_via: &str, id: &str, event: &Event) -> Option<String> {
  if reply_via == "cli" {
    return match event.kind.as_str() {
      "answer" => Some(format!(
        "TMGR Decisions: the owner answered card {id}. Call get_answer (tmgr-notify MCP) with card_id {id} and continue; without the MCP tools run `tmgr decisions answers`."
      )),
      "message" => Some(format!(
        "TMGR Decisions: the owner wrote on card {id}. Call get_answer (tmgr-notify MCP) with card_id {id}, answer with reply_owner. Then continue your work."
      )),
      _ => None,
    };
  }
  if reply_via == "mcp" {
    return match event.kind.as_str() {
      "answer" => Some(format!(
        "TMGR Decisions: the owner answered card {id}. Call get_answer with card_id {id} and continue. If a background agent of yours asked it, pass it on."
      )),
      "message" => Some(format!(
        "TMGR Decisions: the owner wrote on card {id}. Call get_answer with card_id {id}, answer with reply_owner. If a background agent of yours asked it, pass it on."
      )),
      _ => None,
    };
  }
  match event.kind.as_str() {
    "answer" => delivery_text(event),
    "message" => event.text.as_deref().and_then(flatten),
    _ => None,
  }
}

struct Job {
  session: String,
  text: String,
  seq: u64,
  started: Instant,
}

#[derive(Default)]
pub struct Queue {
  jobs: Mutex<HashMap<String, Job>>,
}

impl Queue {
  fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Job>> {
    self.jobs.lock().unwrap_or_else(|p| p.into_inner())
  }

  fn put(&self, card: &str, job: Job) {
    let mut jobs = self.lock();
    if jobs.get(card).is_some_and(|j| j.seq > job.seq) {
      return;
    }
    jobs.insert(card.to_owned(), job);
  }

  pub fn is_empty(&self) -> bool {
    self.lock().is_empty()
  }

  fn finish(&self, card: &str, seq: u64) {
    let mut jobs = self.lock();
    if jobs.get(card).is_some_and(|j| j.seq == seq) {
      jobs.remove(card);
    }
  }

  pub fn tick(&self, hub: &Hub, runner: &dyn RunnerSender, now: Instant) {
    let snapshot: Vec<(String, u64, String, String, Instant)> =
      self.lock().iter().map(|(c, j)| (c.clone(), j.seq, j.session.clone(), j.text.clone(), j.started)).collect();
    for (card, seq, session, text, started) in snapshot {
      let Ok(Some(r)) = hub.routing(&card) else {
        self.finish(&card, seq);
        continue;
      };
      if r.state == "closed" || r.gone {
        hub.record_delivery(&card, seq, "undelivered", "runner", "session closed");
        self.finish(&card, seq);
        continue;
      }
      if r.latest_seq != seq || hub.listening(&card) {
        self.finish(&card, seq);
        continue;
      }
      match runner.send_at_prompt(&session, &format!("{text}\r")) {
        Ok(()) => {
          hub.record_delivery(&card, seq, "delivered", "runner", "");
          self.finish(&card, seq);
        }
        Err(RunErr::Gone) => {
          hub.record_delivery(&card, seq, "undelivered", "runner", "session closed");
          self.finish(&card, seq);
        }
        Err(RunErr::Busy | RunErr::Other) => {
          if now.saturating_duration_since(started) >= QUEUE_MAX {
            hub.record_delivery(&card, seq, "undelivered", "runner", "timed out");
            self.finish(&card, seq);
          }
        }
      }
    }
  }
}

pub fn route_with(
  hub: &Hub,
  queue: &Queue,
  socket: &dyn SocketSender,
  card_id: &str,
  event: &Event,
  bound_target: Option<&str>,
) -> RouteOutcome {
  let Ok(Some(r)) = hub.routing(card_id) else { return RouteOutcome::Skipped };
  let card = &r.card;
  if !(matches!(card.reply_via.as_str(), "mcp" | "cli") || (card.reply_via == "hook" && card.kind == "stopped")) {
    return RouteOutcome::Skipped;
  }
  let Some(text) = text_for(&card.reply_via, &r.id, event) else { return RouteOutcome::Skipped };
  if bound_target.is_some() && bound_target != r.target.as_deref() {
    hub.record_delivery(&r.id, event.seq, "undelivered", "", CHANGED);
    return RouteOutcome::TargetChanged;
  }
  if hub.listening(&r.id) {
    return RouteOutcome::Listening;
  }
  for t in r.targets.iter().filter(|t| t.kind == "claude") {
    let Some(token) = &t.token else { continue };
    match socket.send(&t.endpoint, token.expose(), &text) {
      Ok(()) => {
        hub.record_delivery(&r.id, event.seq, "sent", "claude", "");
        return RouteOutcome::Sent;
      }
      Err(PushErr::Gone) => hub.drop_target(&r.id, &t.endpoint),
      Err(PushErr::Other) => {}
    }
  }
  if let Some(session) = card.source.runner_session.clone() {
    if matches!(card.source.agent.as_str(), "claude-code" | "codex") {
      queue.put(&r.id, Job { session, text, seq: event.seq, started: Instant::now() });
      hub.record_delivery(&r.id, event.seq, "queued", "runner", WAITING);
    } else {
      hub.record_delivery(&r.id, event.seq, "queued", "runner", UNCONFIRMABLE);
    }
    return RouteOutcome::Queued;
  }
  hub.record_delivery(&r.id, event.seq, "undelivered", "", UNREAD);
  RouteOutcome::Undelivered
}

pub fn route<R: Runtime>(
  app: &AppHandle<R>,
  card_id: &str,
  event: &Event,
  bound_target: Option<&str>,
) -> RouteOutcome {
  let Some(d) = app.try_state::<Decisions>() else { return RouteOutcome::Skipped };
  route_with(&d.hub, &d.queue, &ClaudeSocket, card_id, event, bound_target)
}

struct RunnerLink(crate::runner::Runner);

impl RunnerSender for RunnerLink {
  fn send_at_prompt(&self, session: &str, text: &str) -> Result<(), RunErr> {
    self.0.session_send_at_prompt(session, text).map_err(|e| {
      if e == "busy" {
        RunErr::Busy
      } else if e.starts_with("not_found") || e.starts_with("not_running") {
        RunErr::Gone
      } else {
        RunErr::Other
      }
    })
  }
}

pub fn spawn_worker<R: Runtime>(app: &AppHandle<R>, hub: Weak<Hub>, queue: Arc<Queue>) {
  let app = app.clone();
  let spawned = std::thread::Builder::new().name("decisions-push-queue".into()).spawn(move || loop {
    std::thread::sleep(QUEUE_EVERY);
    let Some(hub) = hub.upgrade() else { break };
    if queue.is_empty() {
      continue;
    }
    if let Some(runner) = app.try_state::<crate::runner::Runner>() {
      queue.tick(&hub, &RunnerLink(runner.inner().clone()), Instant::now());
    }
  });
  if spawned.is_err() {
    log::warn!("[decisions] push queue thread failed to start");
  }
}

pub struct ClaudeSocket;

impl SocketSender for ClaudeSocket {
  fn send(&self, endpoint: &str, token: &str, text: &str) -> Result<(), PushErr> {
    if !valid_endpoint(endpoint) {
      return Err(PushErr::Gone);
    }
    deliver_at(Path::new(endpoint), token, text, unsafe { libc::getuid() })
  }
}

fn valid_endpoint(path: &str) -> bool {
  SOCK_DIRS.iter().any(|dir| {
    path
      .strip_prefix(dir)
      .and_then(|name| name.strip_suffix(".sock"))
      .is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()))
  })
}

fn check_socket(path: &Path, uid: u32) -> Result<(), PushErr> {
  if !path.is_absolute() {
    return Err(PushErr::Gone);
  }
  let meta = std::fs::symlink_metadata(path).map_err(|_| PushErr::Gone)?;
  if meta.file_type().is_symlink() || !meta.file_type().is_socket() || meta.uid() != uid {
    return Err(PushErr::Gone);
  }
  Ok(())
}

#[cfg(target_os = "macos")]
fn peer_uid(stream: &UnixStream) -> Option<u32> {
  let (mut uid, mut gid) = (0, 0);
  (unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } == 0).then_some(uid)
}

#[cfg(target_os = "linux")]
fn peer_uid(stream: &UnixStream) -> Option<u32> {
  let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
  let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
  let rc = unsafe {
    libc::getsockopt(
      stream.as_raw_fd(),
      libc::SOL_SOCKET,
      libc::SO_PEERCRED,
      (&mut cred as *mut libc::ucred).cast(),
      &mut len,
    )
  };
  (rc == 0).then_some(cred.uid)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn peer_uid(_: &UnixStream) -> Option<u32> {
  None
}

fn deliver_at(path: &Path, token: &str, text: &str, uid: u32) -> Result<(), PushErr> {
  check_socket(path, uid)?;
  let mut stream = UnixStream::connect(path).map_err(|e| match e.kind() {
    std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::NotFound => PushErr::Gone,
    _ => PushErr::Other,
  })?;
  stream.set_read_timeout(Some(SOCK_TIMEOUT)).map_err(|_| PushErr::Other)?;
  stream.set_write_timeout(Some(SOCK_TIMEOUT)).map_err(|_| PushErr::Other)?;
  match peer_uid(&stream) {
    Some(peer) if peer == uid => {}
    Some(_) => return Err(PushErr::Other),
    None => return Err(PushErr::Gone),
  }
  let auth = json!({"type": "auth", "token": token});
  let user = json!({"type": "user", "message": {"role": "user", "content": text}});
  let payload = format!("{auth}\n{user}\n");
  stream.write_all(payload.as_bytes()).map_err(|_| PushErr::Other)?;
  stream.flush().map_err(|_| PushErr::Other)?;
  let _ = stream.shutdown(std::net::Shutdown::Write);
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::decisions::hub::Sink;
  use crate::decisions::store::{AnswerIn, MessageIn, Store};
  use std::io::Read;
  use std::os::unix::net::UnixListener;
  use tmgr_ipc::decisions::{CardIn, Inbox, LineComment, Opt, Secret, Source};

  const TOKEN: &str = "tok-SECRET-9f3";

  #[derive(Default)]
  struct FakeSocket {
    result: Mutex<Vec<Result<(), PushErr>>>,
    calls: Mutex<Vec<(String, String, String)>>,
  }

  impl FakeSocket {
    fn with(results: Vec<Result<(), PushErr>>) -> FakeSocket {
      FakeSocket { result: Mutex::new(results), calls: Mutex::default() }
    }

    fn calls(&self) -> Vec<(String, String, String)> {
      self.calls.lock().unwrap().clone()
    }
  }

  impl SocketSender for FakeSocket {
    fn send(&self, endpoint: &str, token: &str, text: &str) -> Result<(), PushErr> {
      self.calls.lock().unwrap().push((endpoint.into(), token.into(), text.into()));
      let mut r = self.result.lock().unwrap();
      if r.is_empty() {
        Ok(())
      } else {
        r.remove(0)
      }
    }
  }

  struct FakeRunner {
    results: Mutex<Vec<Result<(), RunErr>>>,
    calls: Mutex<Vec<(String, String)>>,
  }

  impl FakeRunner {
    fn with(results: Vec<Result<(), RunErr>>) -> FakeRunner {
      FakeRunner { results: Mutex::new(results), calls: Mutex::default() }
    }

    fn calls(&self) -> Vec<(String, String)> {
      self.calls.lock().unwrap().clone()
    }
  }

  impl RunnerSender for FakeRunner {
    fn send_at_prompt(&self, session: &str, text: &str) -> Result<(), RunErr> {
      self.calls.lock().unwrap().push((session.into(), text.into()));
      let mut r = self.results.lock().unwrap();
      if r.is_empty() {
        Ok(())
      } else {
        r.remove(0)
      }
    }
  }

  fn hub() -> Arc<Hub> {
    let sink: Sink = Box::new(|_, _| {});
    Arc::new(Hub::new(Store::open(Path::new(":memory:")).unwrap(), sink))
  }

  fn inbox(endpoint: &str) -> Inbox {
    Inbox {
      kind: "claude".into(),
      endpoint: endpoint.into(),
      token: Some(Secret::new(TOKEN)),
      session: Some("sess-1".into()),
    }
  }

  fn mcp(title: &str) -> CardIn {
    CardIn {
      title: title.into(),
      options: vec![Opt { id: "a".into(), label: "Ship it now".into(), recommended: true }],
      source: Source { agent: "claude-code".into(), ..Default::default() },
      ..Default::default()
    }
  }

  fn hook(kind: &str, runner: Option<&str>, agent: &str) -> CardIn {
    CardIn {
      title: "Stopped".into(),
      kind: kind.into(),
      reply_via: "hook".into(),
      source: Source {
        agent: agent.into(),
        session: Some("s1".into()),
        runner_session: runner.map(Into::into),
        ..Default::default()
      },
      ..Default::default()
    }
  }

  const SOCK: &str = "/tmp/cc-socks/4242.sock";

  fn answer(hub: &Hub, id: &str, text: Option<&str>) -> Event {
    hub
      .answer(id, AnswerIn { choice: text.is_none().then(|| "a".to_owned()), free_text: text.map(Into::into), ..Default::default() })
      .unwrap()
      .0
  }

  fn state(hub: &Hub, id: &str) -> Option<(String, String, String)> {
    hub.get(id).unwrap().delivery.map(|d| (d.state, d.via, d.detail))
  }

  fn new_route(hub: &Hub, q: &Queue, s: &FakeSocket, id: &str, e: &Event) -> RouteOutcome {
    route_with(hub, q, s, id, e, None)
  }

  fn listen(hub: &Arc<Hub>, id: &str, since: u64) -> std::thread::JoinHandle<()> {
    let (h, i) = (hub.clone(), id.to_owned());
    let t = std::thread::spawn(move || {
      h.wait(&i, since, Duration::from_secs(5), false).unwrap();
    });
    let start = Instant::now();
    while !hub.listening(id) {
      assert!(start.elapsed() < Duration::from_secs(5));
      std::thread::sleep(Duration::from_millis(10));
    }
    t
  }

  #[test]
  fn an_active_listener_gets_no_push() {
    let hub = hub();
    let id = hub.create_with(mcp("Q"), Some(inbox(SOCK))).unwrap().id;
    let waiter = listen(&hub, &id, 0);
    let e = answer(&hub, &id, None);
    let s = FakeSocket::default();
    assert_eq!(new_route(&hub, &Queue::default(), &s, &id, &e), RouteOutcome::Listening);
    waiter.join().unwrap();
    assert!(s.calls().is_empty());
    assert_eq!(state(&hub, &id), Some(("delivered".into(), "agent".into(), String::new())));
  }

  #[test]
  fn a_claude_target_is_pushed_and_recorded_as_sent() {
    let hub = hub();
    let id = hub.create_with(mcp("Q"), Some(inbox(SOCK))).unwrap().id;
    let e = answer(&hub, &id, None);
    let s = FakeSocket::default();
    assert_eq!(new_route(&hub, &Queue::default(), &s, &id, &e), RouteOutcome::Sent);
    let calls = s.calls();
    assert_eq!((calls[0].0.as_str(), calls[0].1.as_str()), (SOCK, TOKEN));
    assert_eq!(state(&hub, &id), Some(("sent".into(), "claude".into(), String::new())));
    assert!(hub.get(&id).unwrap().push);
  }

  #[test]
  fn a_dead_socket_is_dropped_and_the_runner_queue_takes_over() {
    let hub = hub();
    let card = CardIn { source: Source { runner_session: Some("r1".into()), ..mcp("Q").source }, ..mcp("Q") };
    let id = hub.create_with(card, Some(inbox(SOCK))).unwrap().id;
    let e = answer(&hub, &id, None);
    let s = FakeSocket::with(vec![Err(PushErr::Gone)]);
    let q = Queue::default();
    assert_eq!(new_route(&hub, &q, &s, &id, &e), RouteOutcome::Queued);
    assert!(!hub.get(&id).unwrap().push);
    assert_eq!(state(&hub, &id), Some(("queued".into(), "runner".into(), WAITING.into())));
    assert!(!q.is_empty());
  }

  #[test]
  fn another_socket_error_keeps_the_target() {
    let hub = hub();
    let id = hub.create_with(mcp("Q"), Some(inbox(SOCK))).unwrap().id;
    let e = answer(&hub, &id, None);
    let s = FakeSocket::with(vec![Err(PushErr::Other)]);
    assert_eq!(new_route(&hub, &Queue::default(), &s, &id, &e), RouteOutcome::Undelivered);
    assert!(hub.get(&id).unwrap().push);
  }

  #[test]
  fn a_cli_card_is_typed_into_the_runner_session() {
    let hub = hub();
    let card = CardIn {
      reply_via: "cli".into(),
      source: Source { runner_session: Some("r1".into()), ..mcp("Q").source },
      ..mcp("Q")
    };
    let id = hub.create(card).unwrap().id;
    let e = answer(&hub, &id, None);
    let q = Queue::default();
    assert_eq!(new_route(&hub, &q, &FakeSocket::default(), &id, &e), RouteOutcome::Queued);
    assert_eq!(state(&hub, &id), Some(("queued".into(), "runner".into(), WAITING.into())));
    let r = FakeRunner::with(vec![Ok(())]);
    q.tick(&hub, &r, Instant::now());
    let calls = r.calls();
    assert_eq!(calls[0].0, "r1");
    assert!(calls[0].1.contains(&format!("card {id}")) && calls[0].1.contains("get_answer"));
    assert_eq!(state(&hub, &id), Some(("delivered".into(), "runner".into(), String::new())));
  }

  #[test]
  fn an_older_event_never_replaces_a_newer_queued_job() {
    let q = Queue::default();
    let job = |seq| Job { session: "r1".into(), text: "t".into(), seq, started: Instant::now() };
    q.put("c1", job(2));
    q.put("c1", job(1));
    assert_eq!(q.lock().get("c1").unwrap().seq, 2);
    q.put("c1", job(3));
    assert_eq!(q.lock().get("c1").unwrap().seq, 3);
  }

  #[test]
  fn a_cli_listener_gets_no_push() {
    let hub = hub();
    let id = hub.create(CardIn { reply_via: "cli".into(), ..mcp("Q") }).unwrap().id;
    let waiter = listen(&hub, &id, 0);
    let e = answer(&hub, &id, None);
    let q = Queue::default();
    assert_eq!(new_route(&hub, &q, &FakeSocket::default(), &id, &e), RouteOutcome::Listening);
    waiter.join().unwrap();
    assert!(q.is_empty());
  }

  #[test]
  fn a_cli_message_text_names_the_card_and_hides_the_content() {
    let hub = hub();
    let card = CardIn {
      reply_via: "cli".into(),
      source: Source { runner_session: Some("r1".into()), ..mcp("Secret title").source },
      ..mcp("Secret title")
    };
    let id = hub.create(card).unwrap().id;
    let msg = Event { kind: "message".into(), text: Some("secret body".into()), seq: 1, ..Default::default() };
    let t = text_for("cli", &id, &msg).unwrap();
    assert!(t.contains(&format!("card {id}")) && t.contains("reply_owner"));
    assert!(!t.contains("secret body") && !t.contains("Secret title"));
    let a = text_for("cli", &id, &answer(&hub, &id, None)).unwrap();
    assert!(a.contains("get_answer") && a.contains("tmgr decisions answers"));
    assert!(!a.contains("Ship it now"));
    assert!(text_for("cli", &id, &Event { kind: "other".into(), ..Default::default() }).is_none());
    let q = Queue::default();
    assert_eq!(route_with(&hub, &q, &FakeSocket::default(), &id, &msg, None), RouteOutcome::Queued);
    assert_eq!(state(&hub, &id).unwrap().0, "queued");
  }

  #[test]
  fn an_agent_without_a_prompt_matcher_is_never_typed_into() {
    let hub = hub();
    let card = CardIn { source: Source { agent: "aider".into(), runner_session: Some("r1".into()), ..Default::default() }, ..mcp("Q") };
    let id = hub.create_with(card, None).unwrap().id;
    let e = answer(&hub, &id, None);
    let q = Queue::default();
    assert_eq!(new_route(&hub, &q, &FakeSocket::default(), &id, &e), RouteOutcome::Queued);
    assert!(q.is_empty());
    assert_eq!(state(&hub, &id), Some(("queued".into(), "runner".into(), UNCONFIRMABLE.into())));
  }

  #[test]
  fn no_target_is_undelivered() {
    let hub = hub();
    let id = hub.create(mcp("Q")).unwrap().id;
    let e = answer(&hub, &id, None);
    assert_eq!(new_route(&hub, &Queue::default(), &FakeSocket::default(), &id, &e), RouteOutcome::Undelivered);
    assert_eq!(state(&hub, &id), Some(("undelivered".into(), String::new(), UNREAD.into())));
  }

  #[test]
  fn permission_and_question_hook_cards_are_never_pushed() {
    let hub = hub();
    for kind in ["permission", "question"] {
      let id = hub.create_with(hook(kind, Some("r1"), "claude-code"), Some(inbox(SOCK))).unwrap().id;
      let e = answer(&hub, &id, Some("yes"));
      let s = FakeSocket::default();
      let q = Queue::default();
      assert_eq!(new_route(&hub, &q, &s, &id, &e), RouteOutcome::Skipped);
      assert!(s.calls().is_empty() && q.is_empty());
      assert!(state(&hub, &id).is_none());
    }
  }

  #[test]
  fn a_stopped_hook_card_carries_the_owner_text() {
    let hub = hub();
    let id = hub.create_with(hook("stopped", None, "claude-code"), Some(inbox(SOCK))).unwrap().id;
    let e = answer(&hub, &id, Some("do\u{1b}[31m X\n now"));
    let s = FakeSocket::default();
    assert_eq!(new_route(&hub, &Queue::default(), &s, &id, &e), RouteOutcome::Sent);
    assert_eq!(s.calls()[0].2, "do [31m X now");
  }

  #[test]
  fn a_stopped_hook_card_message_event_is_sanitized() {
    let event = Event { kind: "message".into(), text: Some("a\u{7}b\n c".into()), ..Default::default() };
    assert_eq!(text_for("hook", "c1", &event).unwrap(), "a b c");
    let long = Event { kind: "message".into(), text: Some("x".repeat(DELIVERY_MAX * 2)), ..Default::default() };
    assert_eq!(text_for("hook", "c1", &long).unwrap().len(), DELIVERY_MAX);
    assert!(text_for("hook", "c1", &Event { kind: "message".into(), ..Default::default() }).is_none());
  }

  #[test]
  fn a_bound_target_that_changed_pushes_nothing() {
    let hub = hub();
    let id = hub.create_with(mcp("Q"), Some(inbox(SOCK))).unwrap().id;
    let e = answer(&hub, &id, None);
    let s = FakeSocket::default();
    let q = Queue::default();
    let now = hub.routing(&id).unwrap().unwrap().target.unwrap();
    assert!(now.starts_with("claude:sess-1@"));
    assert_eq!(route_with(&hub, &q, &s, &id, &e, Some("claude:other")), RouteOutcome::TargetChanged);
    assert_eq!(route_with(&hub, &q, &s, &id, &e, Some("claude:sess-1@0000000000000000|runner:r2")), RouteOutcome::TargetChanged);
    assert!(s.calls().is_empty());
    assert_eq!(route_with(&hub, &q, &s, &id, &e, Some(&now)), RouteOutcome::Sent);
    let bare = hub.create(mcp("Other")).unwrap().id;
    let e2 = answer(&hub, &bare, None);
    assert_eq!(route_with(&hub, &q, &s, &bare, &e2, Some("claude:1")), RouteOutcome::TargetChanged);
  }

  #[test]
  fn a_changed_target_is_not_recorded_for_events_that_are_never_delivered() {
    let hub = hub();
    let card = CardIn { kind: "stopped".into(), reply_via: "hook".into(), ..mcp("Q") };
    let id = hub.create_with(card, Some(inbox(SOCK))).unwrap().id;
    let (e, _) = hub.answer(&id, AnswerIn { deferred: true, ..Default::default() }).unwrap();
    let s = FakeSocket::default();
    assert_eq!(route_with(&hub, &Queue::default(), &s, &id, &e, Some("claude:other")), RouteOutcome::Skipped);
    assert!(s.calls().is_empty());
    assert_eq!(state(&hub, &id), None);
  }

  #[test]
  fn a_target_re_registered_under_the_same_session_label_is_not_the_bound_target() {
    let hub = hub();
    let id = hub.create_with(mcp("Q"), Some(inbox(SOCK))).unwrap().id;
    let bound = hub.routing(&id).unwrap().unwrap().target.unwrap();
    hub.drop_target(&id, SOCK);
    let swapped = CardIn { id: Some(id.clone()), ..mcp("Q") };
    hub.create_with(swapped, Some(inbox("/tmp/cc-socks/9999.sock"))).unwrap();
    let e = answer(&hub, &id, Some("secret"));
    let s = FakeSocket::default();
    assert_eq!(route_with(&hub, &Queue::default(), &s, &id, &e, Some(&bound)), RouteOutcome::TargetChanged);
    assert!(s.calls().is_empty());
    assert_eq!(state(&hub, &id), Some(("undelivered".into(), String::new(), CHANGED.into())));
  }

  #[test]
  fn a_pipe_in_the_runner_session_cannot_add_a_delivery_target_part() {
    let hub = hub();
    let card = CardIn { source: Source { runner_session: Some("r9|claude:evil".into()), ..mcp("Q").source }, ..mcp("Q") };
    let id = hub.create_with(card, Some(Inbox { session: None, ..inbox("/tmp/cc-socks/77.sock") })).unwrap().id;
    assert_eq!(hub.routing(&id).unwrap().unwrap().target.unwrap(), "claude:77");
  }

  #[test]
  fn delivery_target_lists_sorted_entries_without_secrets() {
    let hub = hub();
    let card = CardIn { source: Source { runner_session: Some("r9".into()), ..mcp("Q").source }, ..mcp("Q") };
    let id = hub.create_with(card, Some(Inbox { session: None, ..inbox("/tmp/cc-socks/77.sock") })).unwrap().id;
    let t = hub.routing(&id).unwrap().unwrap().target.unwrap();
    assert_eq!(t, "claude:77|runner:r9");
    assert!(!t.contains(TOKEN));
    let none = hub.create(mcp("Z")).unwrap().id;
    assert!(hub.routing(&none).unwrap().unwrap().target.is_none());
  }

  #[test]
  fn mcp_text_never_carries_the_title_or_a_choice_label() {
    let ev = Event { kind: "answer".into(), choice_label: Some("Ship it now".into()), note: Some("N".into()), ..Default::default() };
    let t = text_for("mcp", "c_1", &ev).unwrap();
    assert_eq!(
      t,
      "TMGR Decisions: the owner answered card c_1. Call get_answer with card_id c_1 and continue. If a background agent of yours asked it, pass it on."
    );
    assert!(!t.contains("Ship") && !t.contains("Q"));
    let m = text_for("mcp", "c_1", &Event { kind: "message".into(), text: Some("secret body".into()), ..Default::default() }).unwrap();
    assert!(m.starts_with("TMGR Decisions: the owner wrote on card c_1. Call get_answer with card_id c_1, answer with reply_owner."));
    assert!(!m.contains("secret body"));
  }

  #[test]
  fn the_token_stays_out_of_views_debug_output_and_errors() {
    let hub = hub();
    let id = hub.create_with(mcp("Q"), Some(inbox(SOCK))).unwrap().id;
    let view = serde_json::to_string(&hub.get(&id).unwrap()).unwrap();
    assert!(!view.contains(TOKEN));
    let r = hub.routing(&id).unwrap().unwrap();
    assert!(!format!("{:?}", r.targets).contains(TOKEN));
    assert!(!format!("{:?}", inbox(SOCK)).contains(TOKEN));
    assert!(!format!("{:?} {:?}", PushErr::Gone, RunErr::Busy).contains(TOKEN));
    let dir = tempfile::tempdir().unwrap();
    let err = deliver_at(&dir.path().join("1.sock"), TOKEN, "x", unsafe { libc::getuid() }).unwrap_err();
    assert!(!format!("{err:?}").contains(TOKEN));
  }

  #[test]
  fn the_socket_gets_the_auth_line_then_the_user_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("123.sock");
    let listener = UnixListener::bind(&path).unwrap();
    let reader = std::thread::spawn(move || {
      let (mut c, _) = listener.accept().unwrap();
      let mut got = String::new();
      c.read_to_string(&mut got).unwrap();
      got
    });
    deliver_at(&path, TOKEN, "hi \"there\"", unsafe { libc::getuid() }).unwrap();
    let got = reader.join().unwrap();
    let lines: Vec<serde_json::Value> = got.lines().map(|l| serde_json::from_str(l).unwrap()).collect();
    assert_eq!(lines.len(), 2);
    assert_eq!(lines[0], json!({"type": "auth", "token": TOKEN}));
    assert_eq!(lines[1], json!({"type": "user", "message": {"role": "user", "content": "hi \"there\""}}));
  }

  #[test]
  fn the_socket_check_rejects_relative_symlink_non_socket_and_foreign_paths() {
    let dir = tempfile::tempdir().unwrap();
    let uid = unsafe { libc::getuid() };
    let sock = dir.path().join("5.sock");
    let _l = UnixListener::bind(&sock).unwrap();
    assert!(check_socket(&sock, uid).is_ok());
    assert_eq!(check_socket(&sock, uid + 1), Err(PushErr::Gone));
    assert_eq!(check_socket(Path::new("5.sock"), uid), Err(PushErr::Gone));
    let link = dir.path().join("6.sock");
    std::os::unix::fs::symlink(&sock, &link).unwrap();
    assert_eq!(check_socket(&link, uid), Err(PushErr::Gone));
    let file = dir.path().join("7.sock");
    std::fs::write(&file, b"x").unwrap();
    assert_eq!(check_socket(&file, uid), Err(PushErr::Gone));
    assert_eq!(check_socket(&dir.path().join("none.sock"), uid), Err(PushErr::Gone));
    assert_eq!(deliver_at(&link, TOKEN, "x", uid), Err(PushErr::Gone));
  }

  #[test]
  fn only_the_claude_socket_pattern_is_accepted() {
    assert!(valid_endpoint("/tmp/cc-socks/123.sock"));
    assert!(valid_endpoint("/private/tmp/cc-socks/9.sock"));
    for bad in [
      "cc-socks/123.sock",
      "/tmp/cc-socks/abc.sock",
      "/tmp/cc-socks/.sock",
      "/tmp/cc-socks/1.sock.x",
      "/tmp/cc-socks/../x/1.sock",
      "/tmp/cc-socks/1/2.sock",
      "/tmp/other/1.sock",
      "/var/tmp/cc-socks/1.sock",
      "",
    ] {
      assert!(!valid_endpoint(bad), "{bad}");
      assert_eq!(ClaudeSocket.send(bad, TOKEN, "x"), Err(PushErr::Gone), "{bad}");
    }
  }

  fn queued(hub: &Hub, q: &Queue, session: &str) -> (String, Event) {
    let card = CardIn { source: Source { runner_session: Some(session.into()), ..mcp("Q").source }, ..mcp("Q") };
    let id = hub.create(card).unwrap().id;
    let e = answer(hub, &id, None);
    assert_eq!(new_route(hub, q, &FakeSocket::default(), &id, &e), RouteOutcome::Queued);
    (id, e)
  }

  #[test]
  fn the_queue_retries_while_busy_and_delivers_when_ok() {
    let hub = hub();
    let q = Queue::default();
    let (id, e) = queued(&hub, &q, "r1");
    let run = FakeRunner::with(vec![Err(RunErr::Busy), Err(RunErr::Other), Ok(())]);
    let now = Instant::now();
    q.tick(&hub, &run, now);
    q.tick(&hub, &run, now);
    assert_eq!(state(&hub, &id).unwrap().0, "queued");
    q.tick(&hub, &run, now);
    assert_eq!(state(&hub, &id), Some(("delivered".into(), "runner".into(), String::new())));
    assert!(q.is_empty());
    assert_eq!(run.calls().len(), 3);
    assert_eq!(run.calls()[2].0, "r1");
    assert!(run.calls()[2].1.ends_with("continue. If a background agent of yours asked it, pass it on.\r"), "{e:?}");
    q.tick(&hub, &run, now);
    assert_eq!(run.calls().len(), 3);
  }

  #[test]
  fn the_queue_stops_when_the_session_is_gone() {
    let hub = hub();
    let q = Queue::default();
    let (id, _) = queued(&hub, &q, "r1");
    q.tick(&hub, &FakeRunner::with(vec![Err(RunErr::Gone)]), Instant::now());
    assert_eq!(state(&hub, &id), Some(("undelivered".into(), "runner".into(), "session closed".into())));
    assert!(q.is_empty());
  }

  #[test]
  fn the_queue_drops_a_job_when_a_newer_event_exists() {
    let hub = hub();
    let q = Queue::default();
    let card = CardIn { source: Source { runner_session: Some("r1".into()), ..mcp("Q").source }, ..mcp("Q") };
    let id = hub.create(card).unwrap().id;
    hub.answer(&id, AnswerIn { deferred: true, ..Default::default() }).unwrap();
    let first = hub.routing(&id).unwrap().unwrap().latest_seq;
    let e1 = Event { kind: "answer".into(), seq: first, ..Default::default() };
    new_route(&hub, &q, &FakeSocket::default(), &id, &e1);
    hub.message(&id, MessageIn { text: "newer".into(), ..Default::default() }).unwrap();
    let run = FakeRunner::with(vec![]);
    q.tick(&hub, &run, Instant::now());
    assert!(run.calls().is_empty());
    assert!(q.is_empty());
  }

  #[test]
  fn the_queue_replaces_an_entry_with_the_newer_event() {
    let hub = hub();
    let q = Queue::default();
    let card = CardIn { source: Source { runner_session: Some("r1".into()), ..mcp("Q").source }, ..mcp("Q") };
    let id = hub.create(card).unwrap().id;
    hub.answer(&id, AnswerIn { deferred: true, ..Default::default() }).unwrap();
    let m = hub.message(&id, MessageIn { text: "newer".into(), ..Default::default() }).unwrap();
    let s = FakeSocket::default();
    new_route(&hub, &q, &s, &id, &Event { kind: "answer".into(), seq: 1, ..Default::default() });
    new_route(&hub, &q, &s, &id, &m);
    assert_eq!(q.lock().len(), 1);
    let run = FakeRunner::with(vec![]);
    q.tick(&hub, &run, Instant::now());
    assert_eq!(run.calls().len(), 1);
    assert!(run.calls()[0].1.contains("wrote on card"));
  }

  #[test]
  fn the_queue_reports_a_closed_card_and_a_gone_session() {
    let hub = hub();
    let q = Queue::default();
    let open = hub.create(hook("stopped", Some("r1"), "claude-code")).unwrap().id;
    let m = hub.message(&open, MessageIn { text: "hey".into(), ..Default::default() }).unwrap();
    assert_eq!(new_route(&hub, &q, &FakeSocket::default(), &open, &m), RouteOutcome::Queued);
    hub.close_session("claude-code", "s1").unwrap();
    let pid = CardIn { source: Source { pid: Some(1), runner_session: Some("r2".into()), ..mcp("P").source }, ..mcp("P") };
    let gone = hub.create(pid).unwrap().id;
    let e = hub.message(&gone, MessageIn { text: "hey".into(), ..Default::default() }).unwrap();
    assert_eq!(new_route(&hub, &q, &FakeSocket::default(), &gone, &e), RouteOutcome::Queued);
    hub.check_liveness(|_| false);
    let run = FakeRunner::with(vec![]);
    q.tick(&hub, &run, Instant::now());
    assert!(run.calls().is_empty() && q.is_empty());
    assert_eq!(state(&hub, &open).unwrap().2, "session closed");
    assert_eq!(state(&hub, &gone).unwrap().2, "session closed");
  }

  #[test]
  fn the_queue_leaves_the_row_to_a_listening_agent() {
    let hub = hub();
    let q = Queue::default();
    let card = CardIn { source: Source { runner_session: Some("r2".into()), ..mcp("Q").source }, ..mcp("Q") };
    let id = hub.create(card).unwrap().id;
    let e = hub.message(&id, MessageIn { text: "hey".into(), ..Default::default() }).unwrap();
    assert_eq!(new_route(&hub, &q, &FakeSocket::default(), &id, &e), RouteOutcome::Queued);
    let waiter = listen(&hub, &id, e.seq);
    let run = FakeRunner::with(vec![]);
    q.tick(&hub, &run, Instant::now());
    assert!(run.calls().is_empty() && q.is_empty());
    assert_eq!(state(&hub, &id).unwrap().0, "delivered");
    hub.answer(&id, AnswerIn { dismissed: true, ..Default::default() }).unwrap();
    waiter.join().unwrap();
  }

  #[test]
  fn the_queue_gives_up_after_two_hours() {
    let hub = hub();
    let q = Queue::default();
    let (id, _) = queued(&hub, &q, "r1");
    let run = FakeRunner::with(vec![Err(RunErr::Busy), Err(RunErr::Busy)]);
    q.tick(&hub, &run, Instant::now() + QUEUE_MAX - Duration::from_secs(60));
    assert_eq!(state(&hub, &id).unwrap().0, "queued");
    q.tick(&hub, &run, Instant::now() + QUEUE_MAX + Duration::from_secs(1));
    assert_eq!(state(&hub, &id), Some(("undelivered".into(), "runner".into(), "timed out".into())));
    assert!(q.is_empty());
  }

  #[test]
  fn queued_rows_become_undelivered_after_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("d.db");
    let sink: Sink = Box::new(|_, _| {});
    let hub = Hub::new(Store::open(&path).unwrap(), sink);
    let (id, _) = queued(&hub, &Queue::default(), "r1");
    assert_eq!(state(&hub, &id).unwrap().0, "queued");
    drop(hub);
    let hub = Hub::new(Store::open(&path).unwrap(), Box::new(|_, _| {}));
    assert_eq!(state(&hub, &id), Some(("undelivered".into(), "runner".into(), "desktop restarted".into())));
  }

  #[test]
  fn line_comments_are_part_of_the_hook_text() {
    let e = Event {
      kind: "answer".into(),
      note: Some("after lunch".into()),
      line_comments: vec![LineComment { file: "a.rs".into(), line: 3, side: None, text: "rename".into() }],
      ..Default::default()
    };
    assert_eq!(delivery_text(&e).unwrap(), "after lunch; a.rs:3: rename");
  }

  fn answer_event() -> Event {
    Event { kind: "answer".into(), ..Default::default() }
  }

  #[test]
  fn delivery_never_types_the_choice() {
    let e = Event { choice_label: Some("rm -rf x".into()), choice: Some("$(id)".into()), ..answer_event() };
    assert!(delivery_text(&e).is_none());
    let with_note = Event { note: Some("ok".into()), ..e };
    assert_eq!(delivery_text(&with_note).unwrap(), "ok");
  }

  #[test]
  fn delivery_strips_control_characters_and_newlines() {
    let e = Event { free_text: Some("a\u{1b}[31m\nb\r\n\tc".into()), ..answer_event() };
    assert_eq!(delivery_text(&e).unwrap(), "a [31m b c");
  }

  #[test]
  fn delivery_skips_dismissed_deferred_and_empty() {
    assert!(delivery_text(&Event { dismissed: true, free_text: Some("x".into()), ..answer_event() }).is_none());
    assert!(delivery_text(&Event { deferred: true, free_text: Some("x".into()), ..answer_event() }).is_none());
    assert!(delivery_text(&answer_event()).is_none());
    let long = Event { free_text: Some("x".repeat(DELIVERY_MAX * 2)), ..answer_event() };
    assert_eq!(delivery_text(&long).unwrap().len(), DELIVERY_MAX);
  }
}
