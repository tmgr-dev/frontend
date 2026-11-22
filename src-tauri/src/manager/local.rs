use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde::Deserialize;
use serde_json::Value;
use std::time::{Duration, Instant};

use super::backend::{ChatMessage, HistoryParams, RoomInfo, RoomKind, RoomRef, SearchParams, SendParams, StopOutcome, MAX_HISTORY};

pub trait LocalPort: Send + Sync {
  fn machine(&self) -> Result<String, String>;
  fn rooms(&self) -> Result<Vec<RoomInfo>, String>;
  fn history(&self, params: &HistoryParams) -> Result<Vec<ChatMessage>, String>;
  fn search(&self, params: &SearchParams) -> Result<Vec<ChatMessage>, String>;
  fn send(&self, params: SendParams) -> Result<ChatMessage, String>;
  fn queued(&self, room: &RoomRef) -> Result<Vec<String>, String>;
  fn cancel(&self, room: &RoomRef, id: &str) -> Result<bool, String>;
  fn stop(&self, room: &RoomRef, force: bool) -> Result<StopOutcome, String>;
  fn remove(&self, room: &RoomRef) -> Result<(), String>;
}

pub trait ChatCall: Send + Sync {
  fn call(&self, method: &str, params: Value) -> Result<Value, String>;
}

struct StopTiming {
  poll: Duration,
  int_wait: Duration,
  term_wait: Duration,
}

const STOP_TIMING: StopTiming = StopTiming { poll: Duration::from_millis(200), int_wait: Duration::from_secs(10), term_wait: Duration::from_secs(5) };

#[derive(Deserialize)]
struct SessState {
  kind: String,
}

#[derive(Deserialize)]
struct Sess {
  id: String,
  state: SessState,
  #[serde(default)]
  manager: bool,
}

impl Sess {
  fn exited(&self) -> bool {
    self.state.kind == "exited"
  }
}

fn control_error(e: String) -> String {
  if e.starts_with("busy") {
    "The runner is busy with this chat. Try again in a moment.".to_owned()
  } else if e.starts_with("not_found") {
    "This chat no longer exists in the runner.".to_owned()
  } else if e.starts_with("not_running") {
    "This chat is not running.".to_owned()
  } else {
    e
  }
}

pub struct RunnerLocal {
  runner: Arc<dyn ChatCall>,
  timing: StopTiming,
}

impl RunnerLocal {
  pub fn new(runner: Arc<dyn ChatCall>) -> Self {
    Self { runner, timing: STOP_TIMING }
  }

  fn sessions(&self) -> Result<Vec<Sess>, String> {
    self.request("sessions.list", &Value::Null)
  }

  fn target(&self, room: &RoomRef) -> Result<Sess, String> {
    let id = match (room.kind, room.session.as_deref()) {
      (RoomKind::Session, Some(id)) => id,
      _ => return Err("only a session chat can be controlled".to_owned()),
    };
    crate::runner::check_session_id(id)?;
    if self.machine()? != room.machine {
      return Err("only this machine's chats can be controlled here".to_owned());
    }
    self.sessions()?.into_iter().find(|s| s.id == id).ok_or_else(|| "This chat no longer exists in the runner.".to_owned())
  }

  fn signal(&self, id: &str, signal: &str) -> Result<bool, String> {
    match self.runner.call("sessions.kill", serde_json::json!({ "id": id, "signal": signal })) {
      Ok(_) => Ok(false),
      Err(e) if e.starts_with("not_running") => Ok(true),
      Err(e) => Err(control_error(e)),
    }
  }

  fn wait_exit(&self, id: &str, limit: Duration) -> Result<bool, String> {
    let end = Instant::now() + limit;
    loop {
      if self.sessions()?.iter().find(|s| s.id == id).is_none_or(Sess::exited) {
        return Ok(true);
      }
      if Instant::now() >= end {
        return Ok(false);
      }
      std::thread::sleep(self.timing.poll);
    }
  }

  fn request<P: Serialize, T: DeserializeOwned>(&self, method: &str, params: &P) -> Result<T, String> {
    let value = serde_json::to_value(params).map_err(|e| e.to_string())?;
    let reply = self.runner.call(method, value)?;
    serde_json::from_value(reply).map_err(|e| format!("bad reply from the runner: {e}"))
  }
}

fn is_conflict(e: &str) -> bool {
  e.starts_with("conflict")
}

impl LocalPort for RunnerLocal {
  fn machine(&self) -> Result<String, String> {
    self
      .rooms()?
      .into_iter()
      .find(|r| r.room.kind == RoomKind::Manager)
      .map(|r| r.room.machine)
      .ok_or_else(|| "the runner has no manager room".to_owned())
  }

  fn rooms(&self) -> Result<Vec<RoomInfo>, String> {
    self.request("chat.rooms", &Value::Null)
  }

  fn history(&self, params: &HistoryParams) -> Result<Vec<ChatMessage>, String> {
    self.request("chat.history", params)
  }

  fn search(&self, params: &SearchParams) -> Result<Vec<ChatMessage>, String> {
    self.request("chat.search", params)
  }

  fn queued(&self, room: &RoomRef) -> Result<Vec<String>, String> {
    self.request("chat.queued", &serde_json::json!({ "room": room }))
  }

  fn cancel(&self, room: &RoomRef, id: &str) -> Result<bool, String> {
    self.request("chat.cancel", &serde_json::json!({ "room": room, "id": id }))
  }

  fn stop(&self, room: &RoomRef, force: bool) -> Result<StopOutcome, String> {
    let sess = self.target(room)?;
    if sess.exited() {
      return Err("This chat is not running.".to_owned());
    }
    if sess.manager {
      return Err("The manager session cannot be stopped from here.".to_owned());
    }
    let first = if force { "kill" } else { "int" };
    if self.signal(&sess.id, first)? || self.wait_exit(&sess.id, self.timing.int_wait)? {
      return Ok(StopOutcome { exited: true });
    }
    if force {
      return Ok(StopOutcome { exited: false });
    }
    if self.signal(&sess.id, "term")? {
      return Ok(StopOutcome { exited: true });
    }
    Ok(StopOutcome { exited: self.wait_exit(&sess.id, self.timing.term_wait)? })
  }

  fn remove(&self, room: &RoomRef) -> Result<(), String> {
    let sess = self.target(room)?;
    if !sess.exited() {
      return Err("Stop the chat first. Only a stopped chat can be deleted.".to_owned());
    }
    self.runner.call("sessions.remove", serde_json::json!({ "id": sess.id })).map(drop).map_err(control_error)
  }

  fn send(&self, params: SendParams) -> Result<ChatMessage, String> {
    match self.request("chat.send", &params) {
      Err(e) if is_conflict(&e) => {
        let id = params.id.clone().ok_or(e)?;
        let page = self.history(&HistoryParams { room: params.room, before: None, limit: Some(MAX_HISTORY) })?;
        page.into_iter().find(|m| m.id == id).ok_or_else(|| "message id is already used by different content".to_owned())
      }
      other => other,
    }
  }
}

impl ChatCall for crate::runner::Runner {
  fn call(&self, method: &str, params: Value) -> Result<Value, String> {
    self.chat_call(method, params)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::manager::backend::{Author, RoomRef};
  use serde_json::json;
  use std::sync::Mutex;

  type Reply = Box<dyn Fn(&str, &Value) -> Result<Value, String> + Send + Sync>;

  struct Fake {
    reply: Reply,
    log: Mutex<Vec<(String, Value)>>,
  }

  impl ChatCall for Fake {
    fn call(&self, method: &str, params: Value) -> Result<Value, String> {
      self.log.lock().unwrap().push((method.into(), params.clone()));
      (self.reply)(method, &params)
    }
  }

  fn local(reply: impl Fn(&str, &Value) -> Result<Value, String> + Send + Sync + 'static) -> (RunnerLocal, Arc<Fake>) {
    let fake = Arc::new(Fake { reply: Box::new(reply), log: Mutex::new(vec![]) });
    (RunnerLocal::new(fake.clone()), fake)
  }

  fn room() -> RoomRef {
    RoomRef { kind: RoomKind::Manager, machine: "mx_a".into(), session: None }
  }

  fn msg(id: &str) -> ChatMessage {
    ChatMessage {
      v: 1,
      id: id.into(),
      room: room(),
      author: Author { kind: "owner".into(), id: "owner".into(), machine: None, agent: None },
      ts_ms: 1,
      kind: "text".into(),
      body_md: "hi".into(),
      attachments: vec![],
      reply_to: None,
      decision: None,
    }
  }

  fn send_params(id: Option<&str>) -> SendParams {
    SendParams { room: room(), body_md: "hi".into(), reply_to: None, attachments: vec![], id: id.map(Into::into), ts_ms: None }
  }

  #[test]
  fn machine_comes_from_the_manager_room() {
    let rooms = json!([{"room": {"kind": "session", "machine": "mx_x", "session": "s"}, "title": "s"}, {"room": {"kind": "manager", "machine": "mx_a"}, "title": "Manager"}]);
    let (l, _) = local(move |_, _| Ok(rooms.clone()));
    assert_eq!(l.machine().unwrap(), "mx_a");
  }

  #[test]
  fn machine_fails_without_a_manager_room() {
    let (l, _) = local(|_, _| Ok(json!([])));
    assert!(l.machine().is_err());
  }

  #[test]
  fn history_and_search_use_chat_methods() {
    let m = serde_json::to_value(vec![msg("m1")]).unwrap();
    let (l, fake) = local(move |_, _| Ok(m.clone()));
    let h = l.history(&HistoryParams { room: room(), before: None, limit: Some(5) }).unwrap();
    let s = l.search(&SearchParams { query: "q".into(), room: None, limit: None }).unwrap();
    assert_eq!((h.len(), s.len()), (1, 1));
    let methods: Vec<String> = fake.log.lock().unwrap().iter().map(|c| c.0.clone()).collect();
    assert_eq!(methods, ["chat.history", "chat.search"]);
  }

  #[test]
  fn send_passes_id_and_never_a_device() {
    let m = serde_json::to_value(msg("m_1")).unwrap();
    let (l, fake) = local(move |_, _| Ok(m.clone()));
    l.send(send_params(Some("m_1"))).unwrap();
    let params = fake.log.lock().unwrap()[0].1.clone();
    assert_eq!(params["id"], "m_1");
    assert!(params.get("device").is_none());
  }

  #[test]
  fn conflict_with_an_existing_id_returns_the_stored_message() {
    let stored = serde_json::to_value(vec![msg("m_1")]).unwrap();
    let (l, _) = local(move |method, _| if method == "chat.send" { Err("conflict: id reused".into()) } else { Ok(stored.clone()) });
    assert_eq!(l.send(send_params(Some("m_1"))).unwrap().id, "m_1");
  }

  #[test]
  fn conflict_without_a_stored_message_is_an_error() {
    let (l, _) = local(|method, _| if method == "chat.send" { Err("conflict: id reused".into()) } else { Ok(json!([])) });
    assert!(l.send(send_params(Some("m_1"))).is_err());
    assert_eq!(l.send(send_params(None)).unwrap_err(), "conflict: id reused");
  }

  #[test]
  fn runner_unavailable_is_a_clear_error() {
    let (l, _) = local(|_, _| Err("Agent runner is not running".into()));
    assert_eq!(l.rooms().unwrap_err(), "Agent runner is not running");
    assert!(l.send(send_params(None)).is_err());
  }

  #[test]
  fn queue_calls_carry_the_room_and_message_id() {
    let (l, fake) = local(|method, _| Ok(if method == "chat.queued" { json!(["m_1"]) } else { json!(true) }));
    assert_eq!(l.queued(&room()).unwrap(), ["m_1"]);
    assert!(l.cancel(&room(), "m_1").unwrap());
    let log = fake.log.lock().unwrap();
    assert_eq!(log[0].0, "chat.queued");
    assert_eq!(log[0].1["room"]["kind"], "manager");
    assert_eq!(log[1].0, "chat.cancel");
    assert_eq!(log[1].1["id"], "m_1");
  }

  #[test]
  fn malformed_reply_is_an_error() {
    let (l, _) = local(|_, _| Ok(json!({"x": 1})));
    assert!(l.rooms().unwrap_err().contains("bad reply"));
  }

  mod control {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    const SID: &str = "s_abcdefghij";

    fn sess_room() -> RoomRef {
      RoomRef { kind: RoomKind::Session, machine: "mx_a".into(), session: Some(SID.into()) }
    }

    fn sess_json(kind: &str, manager: bool) -> Value {
      json!([{ "id": SID, "state": { "kind": kind }, "manager": manager }])
    }

    fn fast(mut l: RunnerLocal) -> RunnerLocal {
      l.timing = StopTiming { poll: Duration::from_millis(1), int_wait: Duration::from_millis(40), term_wait: Duration::from_millis(40) };
      l
    }

    fn rooms_reply() -> Value {
      json!([{"room": {"kind": "manager", "machine": "mx_a"}, "title": "Manager"}])
    }

    fn signals(fake: &Fake) -> Vec<String> {
      fake.log.lock().unwrap().iter().filter(|c| c.0 == "sessions.kill").map(|c| c.1["signal"].as_str().unwrap().to_owned()).collect()
    }

    fn methods(fake: &Fake) -> Vec<String> {
      fake.log.lock().unwrap().iter().map(|c| c.0.clone()).collect()
    }

    fn stateful(exit_on: &'static str, manager: bool) -> (RunnerLocal, Arc<Fake>) {
      let dead = Arc::new(AtomicBool::new(false));
      let (l, f) = local(move |method, params| match method {
        "chat.rooms" => Ok(rooms_reply()),
        "sessions.list" => Ok(sess_json(if dead.load(Ordering::SeqCst) { "exited" } else { "working" }, manager)),
        "sessions.kill" => {
          if params["signal"] == exit_on {
            dead.store(true, Ordering::SeqCst);
          }
          Ok(Value::Null)
        }
        _ => Ok(Value::Null),
      });
      (fast(l), f)
    }

    #[test]
    fn stop_sends_int_and_waits_for_exit() {
      let (l, f) = stateful("int", false);
      assert!(l.stop(&sess_room(), false).unwrap().exited);
      assert_eq!(signals(&f), ["int"]);
    }

    #[test]
    fn stop_escalates_to_term_when_int_is_ignored() {
      let (l, f) = stateful("term", false);
      assert!(l.stop(&sess_room(), false).unwrap().exited);
      assert_eq!(signals(&f), ["int", "term"]);
    }

    #[test]
    fn stop_reports_still_running_when_nothing_works() {
      let (l, f) = stateful("never", false);
      assert!(!l.stop(&sess_room(), false).unwrap().exited);
      assert_eq!(signals(&f), ["int", "term"]);
    }

    #[test]
    fn force_stop_sends_only_kill() {
      let (l, f) = stateful("kill", false);
      assert!(l.stop(&sess_room(), true).unwrap().exited);
      assert_eq!(signals(&f), ["kill"]);
    }

    #[test]
    fn force_stop_that_does_not_exit_does_not_escalate() {
      let (l, f) = stateful("never", false);
      assert!(!l.stop(&sess_room(), true).unwrap().exited);
      assert_eq!(signals(&f), ["kill"]);
    }

    #[test]
    fn not_running_during_kill_counts_as_exited() {
      let (l, _) = local(|method, _| match method {
        "chat.rooms" => Ok(rooms_reply()),
        "sessions.list" => Ok(sess_json("working", false)),
        _ => Err("not_running: gone".into()),
      });
      assert!(fast(l).stop(&sess_room(), false).unwrap().exited);
    }

    #[test]
    fn guards_reject_non_session_remote_and_malformed_rooms() {
      let (l, f) = stateful("int", false);
      let bad = [
        RoomRef { kind: RoomKind::Manager, machine: "mx_a".into(), session: None },
        RoomRef { kind: RoomKind::Session, machine: "mx_a".into(), session: None },
        RoomRef { kind: RoomKind::Session, machine: "mx_other".into(), session: Some(SID.into()) },
        RoomRef { kind: RoomKind::Session, machine: "mx_a".into(), session: Some("../x".into()) },
      ];
      for room in &bad {
        assert!(l.stop(room, false).is_err());
        assert!(l.stop(room, true).is_err());
        assert!(l.remove(room).is_err());
      }
      assert!(signals(&f).is_empty());
      assert!(!methods(&f).contains(&"sessions.remove".to_owned()));
    }

    #[test]
    fn unknown_session_is_not_found() {
      let (l, f) = local(|method, _| if method == "chat.rooms" { Ok(rooms_reply()) } else { Ok(json!([])) });
      assert!(l.stop(&sess_room(), false).unwrap_err().contains("no longer exists"));
      assert!(l.remove(&sess_room()).unwrap_err().contains("no longer exists"));
      assert!(signals(&f).is_empty());
    }

    #[test]
    fn live_manager_session_is_never_stopped_or_removed() {
      let (l, f) = stateful("int", true);
      assert!(l.stop(&sess_room(), false).is_err());
      assert!(l.stop(&sess_room(), true).is_err());
      assert!(l.remove(&sess_room()).is_err());
      assert!(signals(&f).is_empty());
      assert!(!methods(&f).contains(&"sessions.remove".to_owned()));
    }

    #[test]
    fn stop_of_an_exited_session_is_refused() {
      let (l, f) = local(|method, _| if method == "chat.rooms" { Ok(rooms_reply()) } else { Ok(sess_json("exited", false)) });
      assert!(l.stop(&sess_room(), false).unwrap_err().contains("not running"));
      assert!(signals(&f).is_empty());
    }

    #[test]
    fn remove_only_works_after_exit() {
      let (l, f) = local(|method, _| if method == "chat.rooms" { Ok(rooms_reply()) } else { Ok(sess_json("working", false)) });
      assert!(l.remove(&sess_room()).unwrap_err().contains("Stop the chat first"));
      assert!(!methods(&f).contains(&"sessions.remove".to_owned()));
    }

    #[test]
    fn remove_of_an_exited_session_calls_sessions_remove() {
      let (l, f) = local(|method, _| match method {
        "chat.rooms" => Ok(rooms_reply()),
        "sessions.list" => Ok(sess_json("exited", false)),
        _ => Ok(Value::Null),
      });
      l.remove(&sess_room()).unwrap();
      let log = f.log.lock().unwrap();
      let last = log.last().unwrap();
      assert_eq!((last.0.as_str(), last.1["id"].as_str()), ("sessions.remove", Some(SID)));
    }

    #[test]
    fn runner_errors_become_clear_messages() {
      let (l, _) = local(|method, _| match method {
        "chat.rooms" => Ok(rooms_reply()),
        "sessions.list" => Ok(sess_json("exited", false)),
        _ => Err("busy: processes are alive".into()),
      });
      assert!(l.remove(&sess_room()).unwrap_err().contains("busy with this chat"));
      let (l, _) = local(|method, _| match method {
        "chat.rooms" => Ok(rooms_reply()),
        "sessions.list" => Ok(sess_json("working", false)),
        _ => Err("not_found: x".into()),
      });
      assert!(fast(l).stop(&sess_room(), false).unwrap_err().contains("no longer exists"));
    }

    #[test]
    fn disabled_runner_gives_the_existing_error() {
      let (l, _) = local(|_, _| Err("Agent runner is not running".into()));
      assert_eq!(l.stop(&sess_room(), false).unwrap_err(), "Agent runner is not running");
      assert_eq!(l.remove(&sess_room()).unwrap_err(), "Agent runner is not running");
    }
  }
}
