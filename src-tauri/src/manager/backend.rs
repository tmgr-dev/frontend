//! The seam between the Manager window and its data source.
//! STUB: `StubBackend` is the only implementation until the runner IPC client (track A, chat.v1) is wired in.
//! The wire types mirror crates/tmgr-ipc/src/chat.rs on branch feat/tm-1018-manager.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RoomKind {
  Manager,
  Session,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoomRef {
  pub kind: RoomKind,
  pub machine: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub session: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Author {
  pub kind: String,
  pub id: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub machine: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub agent: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Attachment {
  pub name: String,
  pub mime: String,
  pub size: u64,
  pub sha256: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecisionRef {
  pub card_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChatMessage {
  pub v: u32,
  pub id: String,
  pub room: RoomRef,
  pub author: Author,
  pub ts_ms: u64,
  pub kind: String,
  pub body_md: String,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub attachments: Vec<Attachment>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub reply_to: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub decision: Option<DecisionRef>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoomInfo {
  pub room: RoomRef,
  pub title: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub agent: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub state: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub context_pct: Option<u8>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub context_tokens: Option<u64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub last: Option<ChatMessage>,
  #[serde(default = "yes")]
  pub local: bool,
}

fn yes() -> bool {
  true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HistoryParams {
  pub room: RoomRef,
  pub before: Option<String>,
  pub limit: Option<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SendParams {
  pub room: RoomRef,
  pub body_md: String,
  pub reply_to: Option<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub attachments: Vec<Attachment>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub id: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub ts_ms: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchParams {
  pub query: String,
  pub room: Option<RoomRef>,
  pub limit: Option<u32>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct SlashCommand {
  pub name: String,
  pub description: String,
  pub args: Option<String>,
  pub group: String,
  pub dangerous: bool,
}

pub const MAX_BODY: usize = 64 << 10;
pub const MAX_ATTACHMENTS: usize = 16;
pub const MAX_HISTORY: u32 = 500;
pub const MAX_QUERY: usize = 256;

pub trait ManagerBackend: Send + Sync {
  fn rooms(&self) -> Result<Vec<RoomInfo>, String>;
  fn history(&self, params: HistoryParams) -> Result<Vec<ChatMessage>, String>;
  fn send(&self, params: SendParams) -> Result<ChatMessage, String>;
  fn search(&self, params: SearchParams) -> Result<Vec<ChatMessage>, String>;
  fn commands(&self, room: &RoomRef) -> Result<Vec<SlashCommand>, String>;
}

fn valid_attachment(a: &Attachment) -> Result<(), String> {
  let name_ok = !a.name.is_empty() && a.name.len() <= 255 && !a.name.contains(['/', '\\', '\0']);
  let sha_ok = a.sha256.len() == 64 && a.sha256.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'));
  if !name_ok || a.mime.is_empty() || a.mime.len() > 255 || !sha_ok {
    return Err("invalid attachment".into());
  }
  Ok(())
}

const MACHINE: &str = "mx_stub";

fn manager_room() -> RoomRef {
  RoomRef { kind: RoomKind::Manager, machine: MACHINE.into(), session: None }
}

fn session_room(id: &str) -> RoomRef {
  RoomRef { kind: RoomKind::Session, machine: MACHINE.into(), session: Some(id.into()) }
}

fn author(kind: &str, id: &str, agent: Option<&str>) -> Author {
  Author { kind: kind.into(), id: id.into(), machine: Some(MACHINE.into()), agent: agent.map(Into::into) }
}

fn message(n: u64, room: RoomRef, who: Author, body: &str) -> ChatMessage {
  ChatMessage {
    v: 1,
    id: format!("m_{n:032x}"),
    room,
    author: who,
    ts_ms: n * 1000,
    kind: "text".into(),
    body_md: body.into(),
    attachments: vec![],
    reply_to: None,
    decision: None,
  }
}

pub struct StubBackend {
  state: Mutex<State>,
}

struct State {
  sessions: Vec<(String, String, String, u8)>,
  messages: Vec<ChatMessage>,
  next: u64,
}

impl Default for StubBackend {
  fn default() -> Self {
    let sessions = vec![
      ("claude-backend".into(), "Cursor pagination for /tasks".into(), "claude".into(), 71),
      ("codex-studio".into(), "Offline queue conflict rules".into(), "codex".into(), 32),
    ];
    let mgr = author("manager", "manager", Some("claude"));
    let messages = vec![
      message(1, manager_room(), mgr.clone(), "Morning. Backend work on **TM-1021** is `working`, tests green."),
      message(2, manager_room(), mgr, "Frontend is waiting for the next-page token name."),
      message(3, session_room("claude-backend"), author("session", "claude-backend", Some("claude")), "Editing `TaskPage.java`."),
    ];
    Self { state: Mutex::new(State { sessions, messages, next: 100 }) }
  }
}

impl StubBackend {
  fn with<T>(&self, f: impl FnOnce(&mut State) -> T) -> T {
    f(&mut self.state.lock().unwrap_or_else(|e| e.into_inner()))
  }
}

fn known(state: &State, room: &RoomRef) -> bool {
  match (room.kind, room.session.as_deref()) {
    (RoomKind::Manager, None) => room.machine == MACHINE,
    (RoomKind::Session, Some(id)) => room.machine == MACHINE && state.sessions.iter().any(|s| s.0 == id),
    _ => false,
  }
}

impl ManagerBackend for StubBackend {
  fn rooms(&self) -> Result<Vec<RoomInfo>, String> {
    Ok(self.with(|s| {
      let last = |room: &RoomRef| s.messages.iter().filter(|m| &m.room == room).max_by_key(|m| m.ts_ms).cloned();
      let mut rooms = vec![RoomInfo {
        room: manager_room(),
        title: "Manager".into(),
        agent: Some("claude".into()),
        state: None,
        context_pct: None,
        context_tokens: None,
        last: last(&manager_room()),
        local: true,
      }];
      let mut sessions: Vec<RoomInfo> = s
        .sessions
        .iter()
        .map(|(id, title, agent, pct)| RoomInfo {
          room: session_room(id),
          title: title.clone(),
          agent: Some(agent.clone()),
          state: Some("working".into()),
          context_pct: Some(*pct),
          context_tokens: Some(u64::from(*pct) * 2000),
          last: last(&session_room(id)),
          local: true,
        })
        .collect();
      sessions.sort_by(|a, b| {
        b.last.as_ref().map(|m| m.ts_ms).cmp(&a.last.as_ref().map(|m| m.ts_ms)).then(a.title.cmp(&b.title))
      });
      rooms.extend(sessions);
      rooms
    }))
  }

  fn history(&self, params: HistoryParams) -> Result<Vec<ChatMessage>, String> {
    let limit = params.limit.unwrap_or(50).min(MAX_HISTORY) as usize;
    self.with(|s| {
      if !known(s, &params.room) {
        return Err("unknown room".into());
      }
      let mut all: Vec<ChatMessage> = s.messages.iter().filter(|m| m.room == params.room).cloned().collect();
      all.sort_by(|a, b| a.id.cmp(&b.id));
      if let Some(before) = params.before.as_deref() {
        let cut = all.iter().position(|m| m.id == before).unwrap_or(all.len());
        all.truncate(cut);
      }
      let skip = all.len().saturating_sub(limit);
      Ok(all.split_off(skip))
    })
  }

  fn send(&self, params: SendParams) -> Result<ChatMessage, String> {
    if params.body_md.trim().is_empty() && params.attachments.is_empty() {
      return Err("empty message".into());
    }
    if params.body_md.len() > MAX_BODY {
      return Err("body_md is too large".into());
    }
    if params.attachments.len() > MAX_ATTACHMENTS {
      return Err("too many attachments".into());
    }
    params.attachments.iter().try_for_each(valid_attachment)?;
    self.with(|s| {
      if !known(s, &params.room) {
        return Err("unknown room".into());
      }
      s.next += 1;
      let mut sent = message(s.next, params.room, author("owner", "owner", None), &params.body_md);
      sent.attachments = params.attachments;
      sent.reply_to = params.reply_to;
      s.messages.push(sent.clone());
      Ok(sent)
    })
  }

  fn search(&self, params: SearchParams) -> Result<Vec<ChatMessage>, String> {
    let needle = params.query.trim().to_lowercase();
    if needle.is_empty() || params.query.len() > MAX_QUERY {
      return Ok(vec![]);
    }
    let limit = params.limit.unwrap_or(50).min(MAX_HISTORY) as usize;
    Ok(self.with(|s| {
      let mut hits: Vec<ChatMessage> = s
        .messages
        .iter()
        .filter(|m| params.room.as_ref().is_none_or(|r| r == &m.room))
        .filter(|m| m.body_md.to_lowercase().contains(&needle) || m.attachments.iter().any(|a| a.name.to_lowercase().contains(&needle)))
        .cloned()
        .collect();
      hits.sort_by_key(|m| std::cmp::Reverse(m.ts_ms));
      hits.truncate(limit);
      hits
    }))
  }

  fn commands(&self, room: &RoomRef) -> Result<Vec<SlashCommand>, String> {
    let cmd = |name: &str, description: &str, args: Option<&str>, group: &str, dangerous| SlashCommand {
      name: name.into(),
      description: description.into(),
      args: args.map(Into::into),
      group: group.into(),
      dangerous,
    };
    self.with(|s| {
      if !known(s, room) {
        return Err("unknown room".into());
      }
      Ok(match room.kind {
        RoomKind::Manager => vec![
          cmd("/summary", "Morning summary of all sessions", None, "TMGR", false),
          cmd("/ask", "Ask a session a question", Some("@session <question>"), "TMGR", false),
          cmd("/stop", "Stop a session", Some("@session"), "TMGR", true),
        ],
        RoomKind::Session => vec![
          cmd("/compact", "Compact the conversation", Some("[instructions]"), "Agent built-ins", false),
          cmd("/clear", "Clear conversation history", None, "Agent built-ins", true),
          cmd("/model", "Switch model", Some("<name>"), "Agent built-ins", false),
        ],
      })
    })
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn send(room: RoomRef, body: &str) -> SendParams {
    SendParams { room, body_md: body.into(), reply_to: None, attachments: vec![], id: None, ts_ms: None }
  }

  fn hist(room: RoomRef, before: Option<String>, limit: u32) -> HistoryParams {
    HistoryParams { room, before, limit: Some(limit) }
  }

  #[test]
  fn manager_room_comes_first() {
    let rooms = StubBackend::default().rooms().unwrap();
    assert_eq!(rooms[0].room.kind, RoomKind::Manager);
    assert_eq!(rooms[1].room.session.as_deref(), Some("claude-backend"));
    assert!(rooms[1].context_tokens.is_some());
  }

  #[test]
  fn send_stores_an_owner_message_with_reply() {
    let backend = StubBackend::default();
    let mut p = send(manager_room(), "hello");
    p.reply_to = Some(format!("m_{:032x}", 1));
    let sent = backend.send(p).unwrap();
    assert_eq!(sent.author.kind, "owner");
    assert!(sent.reply_to.is_some());
    let all = backend.history(hist(manager_room(), None, 50)).unwrap();
    assert_eq!(all.last().unwrap().id, sent.id);
  }

  #[test]
  fn send_rejects_empty_oversized_and_unknown_room() {
    let backend = StubBackend::default();
    assert!(backend.send(send(manager_room(), "  ")).is_err());
    assert!(backend.send(send(manager_room(), &"x".repeat(MAX_BODY + 1))).is_err());
    assert!(backend.send(send(session_room("nope"), "x")).is_err());
    let mut many = send(manager_room(), "x");
    many.attachments = vec![Attachment { name: "a".into(), mime: "text/plain".into(), size: 1, sha256: "0".repeat(64) }; 17];
    assert!(backend.send(many).is_err());
    let mut bad = send(manager_room(), "x");
    bad.attachments = vec![Attachment { name: "../a".into(), mime: "text/plain".into(), size: 1, sha256: "0".repeat(64) }];
    assert!(backend.send(bad).is_err());
  }

  #[test]
  fn history_pages_backwards_from_a_cursor() {
    let backend = StubBackend::default();
    backend.send(send(manager_room(), "third")).unwrap();
    let page = backend.history(hist(manager_room(), None, 2)).unwrap();
    assert_eq!(page.len(), 2);
    let older = backend.history(hist(manager_room(), Some(page[0].id.clone()), 10)).unwrap();
    assert_eq!(older.len(), 1);
  }

  #[test]
  fn commands_depend_on_the_room() {
    let backend = StubBackend::default();
    assert!(backend.commands(&manager_room()).unwrap().iter().any(|c| c.name == "/summary"));
    assert!(backend.commands(&session_room("claude-backend")).unwrap().iter().any(|c| c.name == "/compact"));
    assert!(backend.commands(&session_room("nope")).is_err());
  }

  #[test]
  fn search_is_case_insensitive_and_scoped() {
    let backend = StubBackend::default();
    let q = |text: &str, room| SearchParams { query: text.into(), room, limit: None };
    assert_eq!(backend.search(q("TM-1021", None)).unwrap().len(), 1);
    assert!(backend.search(q("editing", Some(manager_room()))).unwrap().is_empty());
    assert!(backend.search(q(" ", None)).unwrap().is_empty());
  }
}
