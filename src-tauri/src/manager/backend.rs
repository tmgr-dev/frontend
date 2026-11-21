//! The seam between the Manager window and its data source.
//! STUB: `StubBackend` is the only implementation until track A (runner IPC) and B (relay) land.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ChatKind {
  Room,
  Session,
  Machine,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Chat {
  pub id: String,
  pub kind: ChatKind,
  pub title: String,
  pub agent: Option<String>,
  pub machine: Option<String>,
  pub ctx_tokens: Option<u32>,
  pub status: Option<String>,
  pub unread: u32,
  pub pinned: bool,
  pub last_preview: Option<String>,
  pub last_ts: Option<i64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Author {
  pub kind: String,
  pub id: String,
  pub label: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Attachment {
  pub id: String,
  pub name: String,
  pub mime: String,
  pub size: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Message {
  pub id: String,
  pub chat_id: String,
  pub author: Author,
  pub ts: i64,
  pub body_md: String,
  pub attachments: Vec<Attachment>,
  pub reply_to: Option<String>,
  pub kind: String,
  pub decision_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Draft {
  pub body_md: String,
  pub reply_to: Option<String>,
  #[serde(default)]
  pub attachments: Vec<Attachment>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct SlashCommand {
  pub name: String,
  pub description: String,
  pub args: Option<String>,
  pub group: String,
  pub dangerous: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SearchQuery {
  pub text: String,
  pub chat_id: Option<String>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct SearchHit {
  pub chat_id: String,
  pub message_id: String,
  pub ts: i64,
  pub snippet: String,
}

pub trait ManagerBackend: Send + Sync {
  fn chats(&self) -> Result<Vec<Chat>, String>;
  fn messages(&self, chat_id: &str, before: Option<&str>, limit: usize) -> Result<Vec<Message>, String>;
  fn send(&self, chat_id: &str, draft: Draft) -> Result<Message, String>;
  fn commands(&self, chat_id: &str) -> Result<Vec<SlashCommand>, String>;
  fn search(&self, query: SearchQuery) -> Result<Vec<SearchHit>, String>;
  fn mark_read(&self, chat_id: &str) -> Result<(), String>;
}

pub struct StubBackend {
  state: Mutex<State>,
}

struct State {
  chats: Vec<Chat>,
  messages: Vec<Message>,
  next: u64,
}

fn author(kind: &str, id: &str, label: &str) -> Author {
  Author { kind: kind.into(), id: id.into(), label: label.into() }
}

fn text(id: &str, chat: &str, who: Author, ts: i64, body: &str) -> Message {
  Message {
    id: id.into(),
    chat_id: chat.into(),
    author: who,
    ts,
    body_md: body.into(),
    attachments: vec![],
    reply_to: None,
    kind: "text".into(),
    decision_id: None,
  }
}

impl Default for StubBackend {
  fn default() -> Self {
    let chat = |id: &str, kind, title: &str, agent: Option<&str>, ctx: Option<u32>, status: Option<&str>, unread, pinned| Chat {
      id: id.into(),
      kind,
      title: title.into(),
      agent: agent.map(Into::into),
      machine: Some("mac".into()),
      ctx_tokens: ctx,
      status: status.map(Into::into),
      unread,
      pinned,
      last_preview: None,
      last_ts: None,
    };
    let chats = vec![
      chat("claude-backend", ChatKind::Session, "Cursor pagination for /tasks", Some("claude"), Some(142_000), Some("working"), 0, false),
      chat("manager", ChatKind::Room, "Manager", None, None, None, 2, true),
      chat("codex-studio", ChatKind::Session, "Offline queue conflict rules", Some("codex"), Some(64_000), Some("working"), 0, false),
    ];
    let mgr = author("manager", "manager", "Manager");
    let messages = vec![
      text("m1", "manager", mgr.clone(), 1_000, "Morning. Backend work on **TM-1021** is `working`, tests green."),
      text("m2", "manager", mgr, 2_000, "Frontend is waiting for the next-page token name."),
      text("m3", "claude-backend", author("session", "claude-backend", "Claude Code"), 3_000, "Editing `TaskPage.java`."),
    ];
    Self { state: Mutex::new(State { chats, messages, next: 100 }) }
  }
}

impl StubBackend {
  fn with<T>(&self, f: impl FnOnce(&mut State) -> T) -> T {
    f(&mut self.state.lock().unwrap_or_else(|e| e.into_inner()))
  }
}

fn sort_chats(chats: &mut [Chat]) {
  chats.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.last_ts.cmp(&a.last_ts)).then(a.title.cmp(&b.title)));
}

impl ManagerBackend for StubBackend {
  fn chats(&self) -> Result<Vec<Chat>, String> {
    Ok(self.with(|s| {
      let mut out = s.chats.clone();
      for chat in &mut out {
        if let Some(last) = s.messages.iter().filter(|m| m.chat_id == chat.id).max_by_key(|m| m.ts) {
          chat.last_preview = Some(last.body_md.clone());
          chat.last_ts = Some(last.ts);
        }
      }
      sort_chats(&mut out);
      out
    }))
  }

  fn messages(&self, chat_id: &str, before: Option<&str>, limit: usize) -> Result<Vec<Message>, String> {
    Ok(self.with(|s| {
      let mut all: Vec<Message> = s.messages.iter().filter(|m| m.chat_id == chat_id).cloned().collect();
      all.sort_by_key(|m| m.ts);
      if let Some(before) = before {
        let cut = all.iter().position(|m| m.id == before).unwrap_or(all.len());
        all.truncate(cut);
      }
      let skip = all.len().saturating_sub(limit);
      all.split_off(skip)
    }))
  }

  fn send(&self, chat_id: &str, draft: Draft) -> Result<Message, String> {
    if draft.body_md.trim().is_empty() && draft.attachments.is_empty() {
      return Err("empty message".into());
    }
    self.with(|s| {
      if !s.chats.iter().any(|c| c.id == chat_id) {
        return Err(format!("unknown chat {chat_id}"));
      }
      s.next += 1;
      let ts = s.messages.iter().map(|m| m.ts).max().unwrap_or(0) + 1;
      let message = Message {
        id: format!("m{}", s.next),
        chat_id: chat_id.into(),
        author: author("owner", "owner", "You"),
        ts,
        body_md: draft.body_md,
        attachments: draft.attachments,
        reply_to: draft.reply_to,
        kind: "text".into(),
        decision_id: None,
      };
      s.messages.push(message.clone());
      Ok(message)
    })
  }

  fn commands(&self, chat_id: &str) -> Result<Vec<SlashCommand>, String> {
    let cmd = |name: &str, description: &str, args: Option<&str>, group: &str, dangerous| SlashCommand {
      name: name.into(),
      description: description.into(),
      args: args.map(Into::into),
      group: group.into(),
      dangerous,
    };
    let agent = self.with(|s| s.chats.iter().find(|c| c.id == chat_id).map(|c| (c.kind, c.agent.clone())));
    match agent {
      None => Err(format!("unknown chat {chat_id}")),
      Some((ChatKind::Room, _)) => Ok(vec![
        cmd("/summary", "Morning summary of all sessions", None, "TMGR", false),
        cmd("/ask", "Ask a session a question", Some("@session <question>"), "TMGR", false),
        cmd("/stop", "Stop a session", Some("@session"), "TMGR", true),
      ]),
      Some((_, _)) => Ok(vec![
        cmd("/compact", "Compact the conversation", Some("[instructions]"), "Agent built-ins", false),
        cmd("/clear", "Clear conversation history", None, "Agent built-ins", true),
        cmd("/model", "Switch model", Some("<name>"), "Agent built-ins", false),
      ]),
    }
  }

  fn search(&self, query: SearchQuery) -> Result<Vec<SearchHit>, String> {
    let needle = query.text.trim().to_lowercase();
    if needle.is_empty() {
      return Ok(vec![]);
    }
    Ok(self.with(|s| {
      let mut hits: Vec<SearchHit> = s
        .messages
        .iter()
        .filter(|m| query.chat_id.as_deref().map_or(true, |id| id == m.chat_id))
        .filter(|m| m.body_md.to_lowercase().contains(&needle))
        .map(|m| SearchHit { chat_id: m.chat_id.clone(), message_id: m.id.clone(), ts: m.ts, snippet: m.body_md.clone() })
        .collect();
      hits.sort_by(|a, b| b.ts.cmp(&a.ts));
      hits
    }))
  }

  fn mark_read(&self, chat_id: &str) -> Result<(), String> {
    self.with(|s| {
      if let Some(chat) = s.chats.iter_mut().find(|c| c.id == chat_id) {
        chat.unread = 0;
      }
    });
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn draft(body: &str) -> Draft {
    Draft { body_md: body.into(), reply_to: None, attachments: vec![] }
  }

  #[test]
  fn manager_room_is_pinned_first() {
    let chats = StubBackend::default().chats().unwrap();
    assert_eq!(chats[0].id, "manager");
    assert!(chats[0].pinned);
    assert_eq!(chats[0].kind, ChatKind::Room);
  }

  #[test]
  fn sessions_are_ordered_by_latest_message() {
    let chats = StubBackend::default().chats().unwrap();
    assert_eq!(chats[1].id, "claude-backend");
  }

  #[test]
  fn send_appends_an_owner_message_with_reply() {
    let backend = StubBackend::default();
    let mut d = draft("hello");
    d.reply_to = Some("m1".into());
    let sent = backend.send("manager", d).unwrap();
    assert_eq!(sent.author.kind, "owner");
    assert_eq!(sent.reply_to.as_deref(), Some("m1"));
    let all = backend.messages("manager", None, 50).unwrap();
    assert_eq!(all.last().unwrap().id, sent.id);
  }

  #[test]
  fn send_rejects_empty_and_unknown_chat() {
    let backend = StubBackend::default();
    assert!(backend.send("manager", draft("  ")).is_err());
    assert!(backend.send("nope", draft("x")).is_err());
  }

  #[test]
  fn messages_page_backwards_from_a_cursor() {
    let backend = StubBackend::default();
    backend.send("manager", draft("third")).unwrap();
    let page = backend.messages("manager", None, 2).unwrap();
    assert_eq!(page.len(), 2);
    let older = backend.messages("manager", Some(&page[0].id), 10).unwrap();
    assert_eq!(older.len(), 1);
    assert_eq!(older[0].id, "m1");
  }

  #[test]
  fn commands_depend_on_the_chat() {
    let backend = StubBackend::default();
    assert!(backend.commands("manager").unwrap().iter().any(|c| c.name == "/summary"));
    assert!(backend.commands("claude-backend").unwrap().iter().any(|c| c.name == "/compact"));
    assert!(backend.commands("nope").is_err());
  }

  #[test]
  fn search_is_case_insensitive_and_scoped() {
    let backend = StubBackend::default();
    let all = backend.search(SearchQuery { text: "TM-1021".into(), chat_id: None }).unwrap();
    assert_eq!(all.len(), 1);
    let scoped = backend.search(SearchQuery { text: "editing".into(), chat_id: Some("manager".into()) }).unwrap();
    assert!(scoped.is_empty());
    assert!(backend.search(SearchQuery { text: " ".into(), chat_id: None }).unwrap().is_empty());
  }

  #[test]
  fn mark_read_clears_unread() {
    let backend = StubBackend::default();
    backend.mark_read("manager").unwrap();
    let chats = backend.chats().unwrap();
    assert_eq!(chats.iter().find(|c| c.id == "manager").unwrap().unread, 0);
  }
}
