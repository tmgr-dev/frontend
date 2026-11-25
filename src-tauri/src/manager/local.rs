use std::collections::HashMap;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde::Deserialize;
use serde_json::Value;
use std::time::{Duration, Instant};

use base64::Engine as _;
use sha2::{Digest, Sha256};
use tmgr_chat_relay::engine::blob::{parts_for, MAX_LOCAL_BYTES, PART_BYTES};
use tmgr_ipc::chat::{BlobPutParams, BlobPutResult};

use super::backend::{ChatMessage, HistoryParams, RoomInfo, RoomKind, RoomRef, SearchParams, SendParams, StopOutcome, MAX_HISTORY};

pub trait LocalPort: Send + Sync {
  fn machine(&self) -> Result<String, String>;
  fn rooms(&self) -> Result<Vec<RoomInfo>, String>;
  fn history(&self, params: &HistoryParams) -> Result<Vec<ChatMessage>, String>;
  fn search(&self, params: &SearchParams) -> Result<Vec<ChatMessage>, String>;
  fn send(&self, params: SendParams) -> Result<ChatMessage, String>;
  fn queued(&self, room: &RoomRef) -> Result<Vec<String>, String>;
  fn queued_why(&self, _room: &RoomRef) -> Result<HashMap<String, String>, String> {
    Ok(HashMap::new())
  }
  fn cancel(&self, room: &RoomRef, id: &str) -> Result<bool, String>;
  fn stop(&self, room: &RoomRef, force: bool) -> Result<StopOutcome, String>;
  fn remove(&self, room: &RoomRef) -> Result<(), String>;
  fn mention_inject(&self, room: &RoomRef, mention_id: &str, confirmed_risky: bool) -> Result<(), String>;
  fn blob_put(&self, sha256: &str, bytes: &[u8]) -> Result<(), String>;
}

pub trait ChatCall: Send + Sync {
  fn call(&self, method: &str, params: Value) -> Result<Value, String>;
}

const NOT_RUNNING: &str = "This chat is not running.";

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

fn listing_error(e: String) -> String {
  if e.starts_with("busy") {
    "The runner is busy. Try again in a moment.".to_owned()
  } else {
    e
  }
}

fn control_error(e: String) -> String {
  if e.starts_with("busy") {
    "The runner is busy with this chat. Try again in a moment.".to_owned()
  } else if e.starts_with("not_found") {
    "This chat no longer exists in the runner.".to_owned()
  } else if e.starts_with("not_running") {
    NOT_RUNNING.to_owned()
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
    self.request("sessions.list", &Value::Null).map_err(listing_error)
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
    let sess = self.sessions()?.into_iter().find(|s| s.id == id).ok_or_else(|| "This chat no longer exists in the runner.".to_owned())?;
    if sess.manager {
      return Err("The manager session cannot be stopped or deleted from here.".to_owned());
    }
    Ok(sess)
  }

  fn running(&self, room: &RoomRef) -> Result<Sess, String> {
    let sess = self.target(room)?;
    if sess.exited() {
      return Err(NOT_RUNNING.to_owned());
    }
    Ok(sess)
  }

  fn stopped(&self, room: &RoomRef) -> Result<Sess, String> {
    let sess = self.target(room)?;
    if !sess.exited() {
      return Err("Stop the chat first. Only a stopped chat can be deleted.".to_owned());
    }
    Ok(sess)
  }

  fn guarded_signal(&self, room: &RoomRef, signal: &str) -> Result<bool, String> {
    let sess = self.running(room)?;
    self.signal(&sess.id, signal)
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
      if Instant::now() >= end {
        return Ok(false);
      }
      if self.sessions()?.iter().find(|s| s.id == id).is_none_or(Sess::exited) {
        return Ok(true);
      }
      std::thread::sleep(self.timing.poll.min(end.saturating_duration_since(Instant::now())));
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

  fn queued_why(&self, room: &RoomRef) -> Result<HashMap<String, String>, String> {
    Ok(self.request("chat.queued_why", &serde_json::json!({ "room": room })).unwrap_or_default())
  }

  fn cancel(&self, room: &RoomRef, id: &str) -> Result<bool, String> {
    self.request("chat.cancel", &serde_json::json!({ "room": room, "id": id }))
  }

  fn stop(&self, room: &RoomRef, force: bool) -> Result<StopOutcome, String> {
    let id = self.running(room)?.id;
    let first = if force { "kill" } else { "int" };
    if self.guarded_signal(room, first)? || self.wait_exit(&id, self.timing.int_wait)? {
      return Ok(StopOutcome { exited: true });
    }
    if force {
      return Ok(StopOutcome { exited: false });
    }
    if self.running(room).is_err_and(|e| e == NOT_RUNNING) {
      return Ok(StopOutcome { exited: true });
    }
    if self.guarded_signal(room, "term")? {
      return Ok(StopOutcome { exited: true });
    }
    Ok(StopOutcome { exited: self.wait_exit(&id, self.timing.term_wait)? })
  }

  fn remove(&self, room: &RoomRef) -> Result<(), String> {
    self.stopped(room)?;
    let id = self.stopped(room)?.id;
    self.runner.call("sessions.remove", serde_json::json!({ "id": id })).map(drop).map_err(control_error)
  }

  fn mention_inject(&self, room: &RoomRef, mention_id: &str, confirmed_risky: bool) -> Result<(), String> {
    let params = serde_json::json!({ "room": room, "mention_id": mention_id, "confirmed_risky": confirmed_risky });
    self.runner.call("chat.mention_inject", params).map(drop)
  }

  fn send(&self, mut params: SendParams) -> Result<ChatMessage, String> {
    if params.id.is_some() && params.ts_ms.is_none() {
      params.ts_ms = Some(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64));
    }
    match self.request("chat.send", &params) {
      Err(e) if is_conflict(&e) => {
        let id = params.id.clone().ok_or(e)?;
        let page = self.history(&HistoryParams { room: params.room, before: None, limit: Some(MAX_HISTORY) })?;
        page.into_iter().find(|m| m.id == id).ok_or_else(|| "message id is already used by different content".to_owned())
      }
      other => other,
    }
  }

  fn blob_put(&self, sha256: &str, bytes: &[u8]) -> Result<(), String> {
    let size = bytes.len() as u64;
    if size == 0 {
      return Err("The file is empty".to_owned());
    }
    if size > MAX_LOCAL_BYTES {
      return Err("The file is larger than 25 MiB".to_owned());
    }
    let digest: String = Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect();
    if digest != sha256 {
      return Err("The file changed while it was being sent".to_owned());
    }
    let parts = parts_for(size) as u32;
    for (index, chunk) in bytes.chunks(PART_BYTES).enumerate() {
      let part = BlobPutParams {
        v: 1,
        sha256: sha256.to_owned(),
        size,
        part: index as u32,
        parts,
        data: base64::engine::general_purpose::STANDARD.encode(chunk),
      };
      let result: BlobPutResult = self.request("chat.blob.put", &part)?;
      if index as u32 + 1 == parts && result.complete != Some(true) {
        return Err("The runner did not store the file".to_owned());
      }
    }
    Ok(())
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
      mention: None,
      poll: None,
    }
  }

  fn send_params(id: Option<&str>) -> SendParams {
    SendParams { room: room(), body_md: "hi".into(), reply_to: None, attachments: vec![], id: id.map(Into::into), ts_ms: None, mention: None }
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
  fn a_send_with_an_id_and_no_ts_ms_gets_the_current_time_before_it_reaches_the_runner() {
    let m = serde_json::to_value(msg("m_1")).unwrap();
    let (l, fake) = local(move |_, _| Ok(m.clone()));
    l.send(send_params(Some("m_1"))).unwrap();
    let ts = fake.log.lock().unwrap()[0].1["ts_ms"].as_u64().expect("ts_ms is filled");
    assert!(ts > 1_700_000_000_000);
  }

  #[test]
  fn a_send_keeps_the_ts_ms_it_was_given_and_adds_none_without_an_id() {
    let m = serde_json::to_value(msg("m_1")).unwrap();
    let (l, fake) = local(move |_, _| Ok(m.clone()));
    let mut p = send_params(Some("m_1"));
    p.ts_ms = Some(42);
    l.send(p).unwrap();
    l.send(send_params(None)).unwrap();
    let log = fake.log.lock().unwrap();
    assert_eq!(log[0].1["ts_ms"], 42);
    assert!(log[1].1.get("ts_ms").is_none());
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
  fn queued_why_maps_ids_to_reasons_and_is_empty_for_an_older_runner() {
    let (l, fake) = local(|method, _| if method == "chat.queued_why" { Ok(json!({ "m_1": "turn" })) } else { Err("unknown method".into()) });
    assert_eq!(l.queued_why(&room()).unwrap()["m_1"], "turn");
    assert_eq!(fake.log.lock().unwrap()[0].0, "chat.queued_why");
    let (old, _) = local(|_, _| Err("unknown method chat.queued_why".into()));
    assert!(old.queued_why(&room()).unwrap().is_empty());
  }

  #[test]
  fn mention_inject_calls_the_runner_with_the_risk_flag() {
    let (l, fake) = local(|_, _| Ok(Value::Null));
    l.mention_inject(&room(), "m_1", true).unwrap();
    let log = fake.log.lock().unwrap();
    assert_eq!(log[0].0, "chat.mention_inject");
    assert_eq!((log[0].1["mention_id"].as_str(), log[0].1["confirmed_risky"].as_bool()), (Some("m_1"), Some(true)));
  }

  #[test]
  fn mention_rides_on_chat_send_untouched() {
    let m = serde_json::to_value(msg("m_1")).unwrap();
    let (l, fake) = local(move |_, _| Ok(m.clone()));
    let mut p = send_params(None);
    p.mention = Some(crate::manager::backend::MentionReq { target: room(), confirmed_risky: true });
    l.send(p).unwrap();
    let sent = fake.log.lock().unwrap()[0].1.clone();
    assert_eq!(sent["mention"]["confirmed_risky"], true);
    assert_eq!(sent["mention"]["target"]["kind"], "manager");
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
      if manager {
        json!([{ "id": SID, "state": { "kind": kind }, "manager": true }])
      } else {
        json!([{ "id": SID, "state": { "kind": kind } }])
      }
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
    fn exited_manager_session_is_not_removed() {
      let (l, f) = local(|method, _| match method {
        "chat.rooms" => Ok(rooms_reply()),
        "sessions.list" => Ok(sess_json("exited", true)),
        _ => Ok(Value::Null),
      });
      assert!(l.remove(&sess_room()).is_err());
      assert!(l.stop(&sess_room(), true).is_err());
      assert!(!methods(&f).contains(&"sessions.remove".to_owned()));
      assert!(signals(&f).is_empty());
    }

    #[test]
    fn runner_json_without_the_manager_field_is_an_ordinary_session() {
      assert!(sess_json("working", false)[0].get("manager").is_none());
      let (l, f) = stateful("int", false);
      assert!(l.stop(&sess_room(), false).unwrap().exited);
      assert_eq!(signals(&f), ["int"]);
      let (l, f) = local(|method, _| match method {
        "chat.rooms" => Ok(rooms_reply()),
        "sessions.list" => Ok(sess_json("exited", false)),
        _ => Ok(Value::Null),
      });
      l.remove(&sess_room()).unwrap();
      assert!(methods(&f).contains(&"sessions.remove".to_owned()));
    }

    #[test]
    fn session_that_exits_before_term_counts_as_exited() {
      let lists = Arc::new(std::sync::atomic::AtomicUsize::new(0));
      let (l, f) = local(move |method, _| match method {
        "chat.rooms" => Ok(rooms_reply()),
        "sessions.list" => {
          let n = lists.fetch_add(1, Ordering::SeqCst);
          Ok(sess_json(if n >= 2 { "exited" } else { "working" }, false))
        }
        _ => Ok(Value::Null),
      });
      let mut l = fast(l);
      l.timing.int_wait = Duration::ZERO;
      assert!(l.stop(&sess_room(), false).unwrap().exited);
      assert_eq!(signals(&f), ["int"]);
    }

    #[test]
    fn busy_listing_errors_are_not_blamed_on_the_chat() {
      let (l, _) = local(|method, _| match method {
        "chat.rooms" => Ok(rooms_reply()),
        _ => Err("busy: too many connections".into()),
      });
      let e = l.stop(&sess_room(), false).unwrap_err();
      assert!(e.contains("runner is busy") && !e.contains("this chat"));
    }

    fn flipping(first: Value, then: Value) -> (RunnerLocal, Arc<Fake>) {
      let lists = Arc::new(std::sync::atomic::AtomicUsize::new(0));
      let (l, f) = local(move |method, _| match method {
        "chat.rooms" => Ok(rooms_reply()),
        "sessions.list" => Ok(if lists.fetch_add(1, Ordering::SeqCst) == 0 { first.clone() } else { then.clone() }),
        _ => Ok(Value::Null),
      });
      (fast(l), f)
    }

    #[test]
    fn session_that_becomes_manager_before_kill_is_refused() {
      let (l, f) = flipping(sess_json("working", false), sess_json("working", true));
      assert!(l.stop(&sess_room(), false).is_err());
      assert!(signals(&f).is_empty());
    }

    #[test]
    fn session_that_exits_before_kill_is_not_signalled() {
      let (l, f) = flipping(sess_json("working", false), sess_json("exited", false));
      assert!(l.stop(&sess_room(), false).is_err());
      assert!(signals(&f).is_empty());
    }

    #[test]
    fn session_that_becomes_manager_or_running_before_remove_is_refused() {
      for second in [sess_json("exited", true), sess_json("working", false)] {
        let (l, f) = flipping(sess_json("exited", false), second);
        assert!(l.remove(&sess_room()).is_err());
        assert!(!methods(&f).contains(&"sessions.remove".to_owned()));
      }
    }

    #[test]
    fn wait_never_exceeds_the_deadline_by_more_than_one_list_call() {
      let (l, _) = local(|method, _| {
        if method == "sessions.list" {
          std::thread::sleep(Duration::from_millis(30));
          return Ok(sess_json("working", false));
        }
        Ok(rooms_reply())
      });
      let start = Instant::now();
      assert!(!l.wait_exit(SID, Duration::from_millis(50)).unwrap());
      assert!(start.elapsed() < Duration::from_millis(50 + 30 + 40));
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

  fn blob_reply() -> impl Fn(&str, &Value) -> Result<Value, String> + Send + Sync {
    |_, p| {
      let last = p["part"].as_u64() == Some(p["parts"].as_u64().unwrap() - 1);
      Ok(if last { json!({"complete": true}) } else { json!({"have": p["part"], "parts": p["parts"]}) })
    }
  }

  fn digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
  }

  #[test]
  fn blob_put_sends_each_part_with_the_wire_shape_in_order() {
    let bytes = vec![7u8; PART_BYTES + 1];
    let sha = digest(&bytes);
    let (l, fake) = local(blob_reply());
    l.blob_put(&sha, &bytes).unwrap();
    let log = fake.log.lock().unwrap();
    assert_eq!(log.len(), 2);
    assert!(log.iter().all(|c| c.0 == "chat.blob.put"));
    assert_eq!(log[0].1["v"], 1);
    assert_eq!(log[0].1["sha256"], sha);
    assert_eq!(log[0].1["size"], bytes.len());
    assert_eq!((log[0].1["part"].clone(), log[0].1["parts"].clone()), (json!(0), json!(2)));
    assert_eq!(log[1].1["part"], 1);
    let first = base64::engine::general_purpose::STANDARD.decode(log[0].1["data"].as_str().unwrap()).unwrap();
    let second = base64::engine::general_purpose::STANDARD.decode(log[1].1["data"].as_str().unwrap()).unwrap();
    assert_eq!((first.len(), second.len()), (PART_BYTES, 1));
    assert!(serde_json::to_vec(&log[0].1).unwrap().len() < 1024 * 1024);
  }

  #[test]
  fn blob_put_of_exactly_one_part_is_a_single_call() {
    let bytes = vec![1u8; PART_BYTES];
    let (l, fake) = local(blob_reply());
    l.blob_put(&digest(&bytes), &bytes).unwrap();
    assert_eq!(fake.log.lock().unwrap().len(), 1);
  }

  #[test]
  fn blob_put_accepts_the_largest_file_in_36_parts() {
    let bytes = vec![3u8; MAX_LOCAL_BYTES as usize];
    let (l, fake) = local(blob_reply());
    l.blob_put(&digest(&bytes), &bytes).unwrap();
    assert_eq!(fake.log.lock().unwrap().len(), 36);
  }

  #[test]
  fn blob_put_refuses_empty_oversized_and_mismatched_files_without_calling_the_runner() {
    let (l, fake) = local(blob_reply());
    assert!(l.blob_put(&digest(&[]), &[]).is_err());
    let big = vec![0u8; MAX_LOCAL_BYTES as usize + 1];
    assert!(l.blob_put(&digest(&big), &big).is_err());
    assert!(l.blob_put(&digest(b"other"), b"data").unwrap_err().contains("changed"));
    assert!(fake.log.lock().unwrap().is_empty());
  }

  #[test]
  fn blob_put_stops_at_the_first_error() {
    let bytes = vec![5u8; PART_BYTES * 3];
    let (l, fake) = local(|_, p| if p["part"] == 1 { Err("disk full".into()) } else { Ok(json!({"have": 1, "parts": 3})) });
    assert_eq!(l.blob_put(&digest(&bytes), &bytes).unwrap_err(), "disk full");
    assert_eq!(fake.log.lock().unwrap().len(), 2);
  }

  #[test]
  fn blob_put_fails_when_the_last_part_does_not_report_complete() {
    let bytes = vec![5u8; 10];
    let (l, _) = local(|_, _| Ok(json!({"have": 0, "parts": 1})));
    assert!(l.blob_put(&digest(&bytes), &bytes).is_err());
  }
}
