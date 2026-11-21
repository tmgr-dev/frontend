use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;

use super::backend::{ChatMessage, HistoryParams, RoomInfo, RoomKind, SearchParams, SendParams, MAX_HISTORY};

pub trait LocalPort: Send + Sync {
  fn machine(&self) -> Result<String, String>;
  fn rooms(&self) -> Result<Vec<RoomInfo>, String>;
  fn history(&self, params: &HistoryParams) -> Result<Vec<ChatMessage>, String>;
  fn search(&self, params: &SearchParams) -> Result<Vec<ChatMessage>, String>;
  fn send(&self, params: SendParams) -> Result<ChatMessage, String>;
}

pub trait ChatCall: Send + Sync {
  fn call(&self, method: &str, params: Value) -> Result<Value, String>;
}

pub struct RunnerLocal {
  runner: Arc<dyn ChatCall>,
}

impl RunnerLocal {
  pub fn new(runner: Arc<dyn ChatCall>) -> Self {
    Self { runner }
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
  fn malformed_reply_is_an_error() {
    let (l, _) = local(|_, _| Ok(json!({"x": 1})));
    assert!(l.rooms().unwrap_err().contains("bad reply"));
  }
}
