use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tmgr_ipc::decisions::{
  new_card_id, valid_card_id, CardIn, Check, DiffSpec, Event, Fact, LineComment, Links, Opt, Source, ThreadRef,
};

pub const TITLE_MAX: usize = 300;
pub const TEXT_MAX: usize = 64 * 1024;
const LABEL_MAX: usize = 300;
const FACT_MAX: usize = 1000;
const FIELD_MAX: usize = 1024;
const URL_MAX: usize = 2048;
const MAX_OPTIONS: usize = 9;
const MAX_ROWS: usize = 50;
const MAX_LINE_COMMENTS: usize = 200;
const LIST_FINISHED: i64 = 200;

pub const KINDS: [&str; 7] = ["decision", "review", "release", "check", "question", "permission", "stopped"];
const REPLY_VIA: [&str; 3] = ["mcp", "cli", "hook"];
pub const CLOSED_SUPERSEDED: &str = "superseded";
pub const CLOSED_IN_SESSION: &str = "answered in the session";

pub fn now_ms() -> i64 {
  SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

fn cap(s: &str, max: usize) -> String {
  let s = s.trim();
  if s.len() <= max {
    return s.to_owned();
  }
  let mut end = max;
  while !s.is_char_boundary(end) {
    end -= 1;
  }
  s[..end].to_owned()
}

fn cap_opt(s: Option<String>, max: usize) -> Option<String> {
  s.map(|v| cap(&v, max)).filter(|v| !v.is_empty())
}

fn pick(value: &str, allowed: &[&str], fallback: &str) -> String {
  if allowed.contains(&value) {
    value.to_owned()
  } else {
    fallback.to_owned()
  }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct AnswerIn {
  #[serde(default)]
  pub choice: Option<String>,
  #[serde(default)]
  pub note: Option<String>,
  #[serde(default)]
  pub free_text: Option<String>,
  #[serde(default)]
  pub line_comments: Vec<LineComment>,
  #[serde(default)]
  pub deferred: bool,
  #[serde(default)]
  pub dismissed: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct MessageIn {
  pub text: String,
  #[serde(default)]
  pub file: Option<String>,
  #[serde(default)]
  pub line: Option<u32>,
  #[serde(default)]
  pub action: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct MessageView {
  pub seq: u64,
  pub author: String,
  pub text: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub file: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub line: Option<u32>,
  pub created_ms: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CardView {
  pub id: String,
  pub created_ms: i64,
  pub state: String,
  pub session_gone: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub closed_reason: Option<String>,
  pub can_reply: bool,
  pub source: Source,
  pub kind: String,
  pub blocking: bool,
  pub title: String,
  pub context: Option<String>,
  pub question: Option<String>,
  pub facts: Vec<Fact>,
  pub options: Vec<Opt>,
  pub diff: Option<DiffSpec>,
  pub checks: Vec<Check>,
  pub links: Links,
  pub reply_via: String,
  pub answer: Option<Event>,
  pub messages: Vec<MessageView>,
}

pub struct Created {
  pub id: String,
  pub fresh: bool,
  pub card: CardIn,
}

pub struct Store {
  conn: Connection,
}

fn sanitize(mut c: CardIn) -> Result<CardIn, String> {
  if let Some(id) = &c.id {
    if !valid_card_id(id) {
      return Err("invalid card id".into());
    }
  }
  c.title = cap(&c.title, TITLE_MAX);
  if c.title.is_empty() {
    return Err("title is required".into());
  }
  if c.kind.is_empty() {
    c.kind = "decision".into();
  } else if !KINDS.contains(&c.kind.as_str()) {
    return Err(format!("unknown kind {}", cap(&c.kind, 40)));
  }
  if c.reply_via.is_empty() {
    c.reply_via = "mcp".into();
  } else if !REPLY_VIA.contains(&c.reply_via.as_str()) {
    return Err(format!("unknown reply_via {}", cap(&c.reply_via, 40)));
  }
  c.context = cap_opt(c.context, TEXT_MAX);
  c.question = cap_opt(c.question, TEXT_MAX);
  c.source.agent = cap(&c.source.agent, 128);
  if c.source.agent.is_empty() {
    c.source.agent = "agent".into();
  }
  c.source.session = cap_opt(c.source.session, FIELD_MAX);
  c.source.cwd = cap_opt(c.source.cwd, FIELD_MAX * 4);
  c.source.title = cap_opt(c.source.title, TITLE_MAX);
  c.source.runner_session = cap_opt(c.source.runner_session, 64);
  c.options.truncate(MAX_OPTIONS);
  for (i, o) in c.options.iter_mut().enumerate() {
    o.label = cap(&o.label, LABEL_MAX);
    o.id = cap(&o.id, 64);
    if o.id.is_empty() {
      o.id = (i + 1).to_string();
    }
  }
  c.facts.truncate(MAX_ROWS);
  for f in &mut c.facts {
    f.state = pick(&f.state, &["ok", "bad", "wait"], "wait");
    f.text = cap(&f.text, FACT_MAX);
    f.link = cap_opt(f.link.take(), URL_MAX);
  }
  c.checks.truncate(MAX_ROWS);
  for k in &mut c.checks {
    k.state = pick(&k.state, &["ok", "bad", "todo"], "todo");
    k.text = cap(&k.text, FACT_MAX);
  }
  c.diff = c.diff.take().and_then(|mut d| {
    d.repo = cap(&d.repo, FIELD_MAX * 4);
    d.base = cap_opt(d.base, 200);
    d.head = cap_opt(d.head, 200);
    (!d.repo.is_empty()).then_some(d)
  });
  c.links.task = cap_opt(c.links.task.take(), URL_MAX);
  c.links.pr = cap_opt(c.links.pr.take(), URL_MAX);
  Ok(c)
}

fn json_err(e: serde_json::Error) -> String {
  e.to_string()
}

fn sql_err(e: rusqlite::Error) -> String {
  e.to_string()
}

struct Row {
  id: String,
  created_ms: i64,
  card: String,
  state: String,
  session_gone: bool,
  answer: Option<String>,
  closed_reason: Option<String>,
}

const CARD_COLS: &str = "id, created_ms, card, state, session_gone, answer, closed_reason";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
  Ok(Row {
    id: r.get(0)?,
    created_ms: r.get(1)?,
    card: r.get(2)?,
    state: r.get(3)?,
    session_gone: r.get::<_, i64>(4)? != 0,
    answer: r.get(5)?,
    closed_reason: r.get(6)?,
  })
}

impl Store {
  pub fn open(path: &Path) -> Result<Store, String> {
    let conn = Connection::open(path).map_err(sql_err)?;
    conn.busy_timeout(std::time::Duration::from_secs(5)).map_err(sql_err)?;
    let _ = conn.pragma_update(None, "journal_mode", "WAL");
    conn
      .execute_batch(
        "CREATE TABLE IF NOT EXISTS cards(
           id TEXT PRIMARY KEY, created_ms INTEGER NOT NULL, card TEXT NOT NULL, state TEXT NOT NULL,
           session_gone INTEGER NOT NULL DEFAULT 0, answer TEXT, closed_reason TEXT);
         CREATE INDEX IF NOT EXISTS cards_state ON cards(state);
         CREATE TABLE IF NOT EXISTS events(
           card_id TEXT NOT NULL, seq INTEGER NOT NULL, json TEXT NOT NULL, PRIMARY KEY(card_id, seq));
         CREATE TABLE IF NOT EXISTS messages(
           card_id TEXT NOT NULL, seq INTEGER NOT NULL, author TEXT NOT NULL, text TEXT NOT NULL,
           file TEXT, line INTEGER, created_ms INTEGER NOT NULL, PRIMARY KEY(card_id, seq));",
      )
      .map_err(sql_err)?;
    Ok(Store { conn })
  }

  pub fn check(card: &CardIn) -> Result<(), String> {
    sanitize(card.clone()).map(|_| ())
  }

  pub fn create(&mut self, card: CardIn) -> Result<Created, String> {
    let mut card = sanitize(card)?;
    let id = card.id.clone().unwrap_or_else(new_card_id);
    card.id = Some(id.clone());
    if let Some(existing) = self.raw(&id)? {
      let card = serde_json::from_str(&existing.card).map_err(json_err)?;
      return Ok(Created { id, fresh: false, card });
    }
    let tx = self.conn.transaction().map_err(sql_err)?;
    if card.supersede && card.reply_via == "hook" {
      if let Some(session) = &card.source.session {
        tx.execute(
          "UPDATE cards SET state='closed', closed_reason=?1 WHERE state='open'
             AND json_extract(card,'$.reply_via')='hook'
             AND json_extract(card,'$.source.agent')=?2 AND json_extract(card,'$.source.session')=?3",
          params![CLOSED_SUPERSEDED, card.source.agent, session],
        )
        .map_err(sql_err)?;
      }
    }
    tx.execute(
      "INSERT INTO cards(id, created_ms, card, state) VALUES(?1, ?2, ?3, 'open')",
      params![id, now_ms(), serde_json::to_string(&card).map_err(json_err)?],
    )
    .map_err(sql_err)?;
    tx.commit().map_err(sql_err)?;
    Ok(Created { id, fresh: true, card })
  }

  fn raw(&self, id: &str) -> Result<Option<Row>, String> {
    self.conn.query_row(&format!("SELECT {CARD_COLS} FROM cards WHERE id=?1"), [id], row).optional().map_err(sql_err)
  }

  fn view(&self, r: Row) -> Result<CardView, String> {
    let c: CardIn = serde_json::from_str(&r.card).map_err(json_err)?;
    let answer = r.answer.as_deref().map(serde_json::from_str::<Event>).transpose().map_err(json_err)?;
    let live = r.state == "open" || r.state == "deferred";
    let mut stmt = self
      .conn
      .prepare("SELECT seq, author, text, file, line, created_ms FROM messages WHERE card_id=?1 ORDER BY seq")
      .map_err(sql_err)?;
    let messages = stmt
      .query_map([&r.id], |m| {
        Ok(MessageView {
          seq: m.get::<_, i64>(0)? as u64,
          author: m.get(1)?,
          text: m.get(2)?,
          file: m.get(3)?,
          line: m.get::<_, Option<i64>>(4)?.map(|l| l as u32),
          created_ms: m.get(5)?,
        })
      })
      .map_err(sql_err)?
      .collect::<Result<Vec<_>, _>>()
      .map_err(sql_err)?;
    Ok(CardView {
      id: r.id,
      created_ms: r.created_ms,
      session_gone: r.session_gone,
      closed_reason: r.closed_reason,
      can_reply: live && (c.reply_via == "mcp" || c.reply_via == "cli"),
      state: r.state,
      source: c.source,
      kind: c.kind,
      blocking: c.blocking,
      title: c.title,
      context: c.context,
      question: c.question,
      facts: c.facts,
      options: c.options,
      diff: c.diff,
      checks: c.checks,
      links: c.links,
      reply_via: c.reply_via,
      answer,
      messages,
    })
  }

  pub fn get(&self, id: &str) -> Result<Option<CardView>, String> {
    self.raw(id)?.map(|r| self.view(r)).transpose()
  }

  pub fn list(&self) -> Result<Vec<CardView>, String> {
    let mut stmt = self
      .conn
      .prepare(&format!(
        "SELECT {CARD_COLS} FROM cards WHERE state IN ('open','deferred') OR id IN
           (SELECT id FROM cards WHERE state NOT IN ('open','deferred') ORDER BY created_ms DESC LIMIT {LIST_FINISHED})
         ORDER BY created_ms DESC"
      ))
      .map_err(sql_err)?;
    let rows = stmt.query_map([], row).map_err(sql_err)?.collect::<Result<Vec<_>, _>>().map_err(sql_err)?;
    rows.into_iter().map(|r| self.view(r)).collect()
  }

  pub fn open_count(&self) -> usize {
    self
      .conn
      .query_row("SELECT COUNT(*) FROM cards WHERE state IN ('open','deferred')", [], |r| r.get::<_, i64>(0))
      .unwrap_or(0) as usize
  }

  fn next_seq(&self, id: &str) -> Result<u64, String> {
    let n: i64 = self
      .conn
      .query_row("SELECT COALESCE(MAX(seq),0)+1 FROM events WHERE card_id=?1", [id], |r| r.get(0))
      .map_err(sql_err)?;
    Ok(n as u64)
  }

  fn next_message_seq(&self, id: &str) -> Result<i64, String> {
    self
      .conn
      .query_row("SELECT COALESCE(MAX(seq),0)+1 FROM messages WHERE card_id=?1", [id], |r| r.get(0))
      .map_err(sql_err)
  }

  fn live_card(&self, id: &str) -> Result<(Row, CardIn), String> {
    let r = self.raw(id)?.ok_or_else(|| "unknown card".to_owned())?;
    if r.state != "open" && r.state != "deferred" {
      return Err("the card is already closed".into());
    }
    let c = serde_json::from_str(&r.card).map_err(json_err)?;
    Ok((r, c))
  }

  pub fn answer(&mut self, id: &str, a: AnswerIn) -> Result<(Event, CardIn), String> {
    let (_, card) = self.live_card(id)?;
    let choice = cap_opt(a.choice, 64);
    let choice_label = match &choice {
      Some(c) if !card.options.is_empty() => {
        Some(card.options.iter().find(|o| &o.id == c).ok_or_else(|| "unknown option".to_owned())?.label.clone())
      }
      _ => None,
    };
    let mut line_comments = a.line_comments;
    line_comments.truncate(MAX_LINE_COMMENTS);
    for l in &mut line_comments {
      l.file = cap(&l.file, FIELD_MAX * 4);
      l.text = cap(&l.text, TEXT_MAX);
    }
    let seq = self.next_seq(id)?;
    let event = Event {
      card_id: id.to_owned(),
      kind: "answer".into(),
      seq,
      choice,
      choice_label,
      note: cap_opt(a.note, TEXT_MAX),
      free_text: cap_opt(a.free_text, TEXT_MAX),
      line_comments,
      deferred: a.deferred && !a.dismissed,
      dismissed: a.dismissed,
      ..Default::default()
    };
    let state = if event.dismissed {
      "dismissed"
    } else if event.deferred {
      "deferred"
    } else {
      "answered"
    };
    let json = serde_json::to_string(&event).map_err(json_err)?;
    let tx = self.conn.transaction().map_err(sql_err)?;
    tx.execute("INSERT INTO events(card_id, seq, json) VALUES(?1, ?2, ?3)", params![id, seq as i64, json])
      .map_err(sql_err)?;
    tx.execute("UPDATE cards SET state=?2, answer=?3, session_gone=CASE WHEN ?2='deferred' THEN session_gone ELSE 0 END WHERE id=?1", params![id, state, json])
      .map_err(sql_err)?;
    tx.commit().map_err(sql_err)?;
    Ok((event, card))
  }

  pub fn owner_message(&mut self, id: &str, m: MessageIn) -> Result<Event, String> {
    self.live_card(id)?;
    let text = cap(&m.text, TEXT_MAX);
    if text.is_empty() {
      return Err("text is required".into());
    }
    let file = cap_opt(m.file, FIELD_MAX * 4);
    let line = file.as_ref().and(m.line);
    let action = pick(m.action.as_deref().unwrap_or("reply"), &["reply", "fix"], "reply");
    let seq = self.next_seq(id)?;
    let event = Event {
      card_id: id.to_owned(),
      kind: "message".into(),
      seq,
      text: Some(text.clone()),
      thread: file.clone().zip(line).map(|(file, line)| ThreadRef { file, line }),
      action: Some(action),
      ..Default::default()
    };
    let mseq = self.next_message_seq(id)?;
    let tx = self.conn.transaction().map_err(sql_err)?;
    tx.execute(
      "INSERT INTO events(card_id, seq, json) VALUES(?1, ?2, ?3)",
      params![id, seq as i64, serde_json::to_string(&event).map_err(json_err)?],
    )
    .map_err(sql_err)?;
    tx.execute(
      "INSERT INTO messages(card_id, seq, author, text, file, line, created_ms) VALUES(?1, ?2, 'owner', ?3, ?4, ?5, ?6)",
      params![id, mseq, text, file, line.map(i64::from), now_ms()],
    )
    .map_err(sql_err)?;
    tx.commit().map_err(sql_err)?;
    Ok(event)
  }

  pub fn agent_message(&mut self, id: &str, text: Option<String>, lines: Vec<LineComment>) -> Result<(), String> {
    self.raw(id)?.ok_or_else(|| "unknown card".to_owned())?;
    let mut items: Vec<(String, Option<String>, Option<u32>)> = Vec::new();
    if let Some(t) = cap_opt(text, TEXT_MAX) {
      items.push((t, None, None));
    }
    for l in lines.into_iter().take(MAX_LINE_COMMENTS) {
      let t = cap(&l.text, TEXT_MAX);
      if !t.is_empty() {
        items.push((t, Some(cap(&l.file, FIELD_MAX * 4)), Some(l.line)));
      }
    }
    if items.is_empty() {
      return Err("text is required".into());
    }
    let first = self.next_message_seq(id)?;
    let tx = self.conn.transaction().map_err(sql_err)?;
    for (seq, (text, file, line)) in (first..).zip(items) {
      tx.execute(
        "INSERT INTO messages(card_id, seq, author, text, file, line, created_ms) VALUES(?1, ?2, 'agent', ?3, ?4, ?5, ?6)",
        params![id, seq, text, file, line.map(i64::from), now_ms()],
      )
      .map_err(sql_err)?;
    }
    tx.commit().map_err(sql_err)
  }

  pub fn close_session(&mut self, agent: &str, session: &str) -> Result<u64, String> {
    let n = self
      .conn
      .execute(
        "UPDATE cards SET state='closed', closed_reason=?1 WHERE state='open'
           AND json_extract(card,'$.reply_via')='hook'
           AND json_extract(card,'$.source.agent')=?2 AND json_extract(card,'$.source.session')=?3",
        params![CLOSED_IN_SESSION, cap(agent, 128), cap(session, FIELD_MAX)],
      )
      .map_err(sql_err)?;
    Ok(n as u64)
  }

  pub fn events(&self, id: &str) -> Result<Vec<Event>, String> {
    if self.raw(id)?.is_none() {
      return Err("unknown card".into());
    }
    let mut stmt = self.conn.prepare("SELECT json FROM events WHERE card_id=?1 ORDER BY seq").map_err(sql_err)?;
    let rows = stmt
      .query_map([id], |r| r.get::<_, String>(0))
      .map_err(sql_err)?
      .collect::<Result<Vec<_>, _>>()
      .map_err(sql_err)?;
    rows.iter().map(|j| serde_json::from_str(j).map_err(json_err)).collect()
  }

  pub fn wait_event(&self, id: &str, since: u64, final_only: bool) -> Result<Option<Event>, String> {
    let events = self.events(id)?;
    if let Some(e) = events.iter().rev().find(|e| e.is_final()) {
      return Ok(Some(e.clone()));
    }
    if final_only {
      return Ok(None);
    }
    Ok(events.into_iter().find(|e| e.seq > since))
  }

  pub fn live_pids(&self) -> Result<Vec<(String, u32)>, String> {
    let mut stmt = self
      .conn
      .prepare(
        "SELECT id, json_extract(card,'$.source.pid') FROM cards WHERE state IN ('open','deferred') AND session_gone=0",
      )
      .map_err(sql_err)?;
    let rows = stmt
      .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<i64>>(1)?)))
      .map_err(sql_err)?
      .collect::<Result<Vec<_>, _>>()
      .map_err(sql_err)?;
    Ok(rows.into_iter().filter_map(|(id, pid)| pid.and_then(|p| u32::try_from(p).ok()).map(|p| (id, p))).collect())
  }

  pub fn mark_gone(&mut self, ids: &[String]) -> Result<(), String> {
    for id in ids {
      self.conn.execute("UPDATE cards SET session_gone=1 WHERE id=?1", [id]).map_err(sql_err)?;
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn mem() -> Store {
    Store::open(Path::new(":memory:")).unwrap()
  }

  fn card(title: &str) -> CardIn {
    CardIn {
      title: title.into(),
      source: Source { agent: "claude".into(), session: Some("s1".into()), ..Default::default() },
      options: vec![
        Opt { id: "a".into(), label: "Ship".into(), recommended: true },
        Opt { id: "b".into(), label: "Hold".into(), recommended: false },
      ],
      ..Default::default()
    }
  }

  fn hook(title: &str, session: &str) -> CardIn {
    CardIn {
      title: title.into(),
      kind: "stopped".into(),
      reply_via: "hook".into(),
      supersede: true,
      source: Source { agent: "claude".into(), session: Some(session.into()), ..Default::default() },
      ..Default::default()
    }
  }

  fn pick_a() -> AnswerIn {
    AnswerIn { choice: Some("a".into()), ..Default::default() }
  }

  #[test]
  fn cards_and_answers_survive_a_reopen() {
    let dir = tempfile::Builder::new().prefix("ds").tempdir_in("/tmp").unwrap();
    let path = dir.path().join("decisions.db");
    let id = {
      let mut s = Store::open(&path).unwrap();
      let id = s.create(card("Ship?")).unwrap().id;
      s.answer(&id, pick_a()).unwrap();
      s.agent_message(&id, Some("thanks".into()), vec![]).unwrap();
      id
    };
    let s = Store::open(&path).unwrap();
    let v = s.get(&id).unwrap().unwrap();
    assert_eq!(v.state, "answered");
    assert_eq!(v.answer.unwrap().choice_label.as_deref(), Some("Ship"));
    assert_eq!(v.messages.len(), 1);
    assert_eq!(s.wait_event(&id, 0, false).unwrap().unwrap().choice.as_deref(), Some("a"));
  }

  #[test]
  fn defaults_validation_and_caps() {
    let mut s = mem();
    let mut c = card(&"t".repeat(500));
    c.options = (0..20).map(|i| Opt { id: String::new(), label: format!("o{i}"), recommended: false }).collect();
    c.facts = (0..80).map(|_| Fact { state: "weird".into(), text: "x".into(), link: None }).collect();
    c.context = Some("é".repeat(TEXT_MAX));
    let id = s.create_id(c);
    let v = s.get(&id).unwrap().unwrap();
    assert_eq!(v.title.len(), TITLE_MAX);
    assert_eq!(v.options.len(), 9);
    assert_eq!(v.options[0].id, "1");
    assert_eq!(v.facts.len(), 50);
    assert_eq!(v.facts[0].state, "wait");
    assert!(v.context.unwrap().len() <= TEXT_MAX);
    assert_eq!(v.kind, "decision");
    assert_eq!(v.reply_via, "mcp");
    assert!(v.can_reply);
    assert!(s.create(CardIn { title: "  ".into(), ..Default::default() }).is_err());
    assert!(s.create(CardIn { title: "x".into(), kind: "nope".into(), ..Default::default() }).is_err());
    assert!(s.create(CardIn { title: "x".into(), id: Some("c_bad".into()), ..Default::default() }).is_err());
  }

  impl Store {
    fn create_id(&mut self, c: CardIn) -> String {
      self.create(c).unwrap().id
    }
  }

  #[test]
  fn duplicate_ids_are_idempotent() {
    let mut s = mem();
    let id = new_card_id();
    let mut c = card("Once");
    c.id = Some(id.clone());
    assert!(s.create(c.clone()).unwrap().fresh);
    assert!(!s.create(c).unwrap().fresh);
    assert_eq!(s.list().unwrap().len(), 1);
  }

  #[test]
  fn final_answer_is_terminal_and_repeatable() {
    let mut s = mem();
    let id = s.create_id(card("Ship?"));
    let (e, _) = s.answer(&id, pick_a()).unwrap();
    assert_eq!((e.seq, e.is_final()), (1, true));
    assert!(s.answer(&id, pick_a()).is_err());
    assert!(s.owner_message(&id, MessageIn { text: "hi".into(), ..Default::default() }).is_err());
    for since in [0, 1, 5] {
      assert_eq!(s.wait_event(&id, since, true).unwrap().unwrap().seq, 1);
    }
    assert!(!s.get(&id).unwrap().unwrap().can_reply);
  }

  #[test]
  fn unknown_option_is_rejected() {
    let mut s = mem();
    let id = s.create_id(card("Ship?"));
    let a = AnswerIn { choice: Some("zzz".into()), ..Default::default() };
    assert!(s.answer(&id, a).is_err());
    assert_eq!(s.get(&id).unwrap().unwrap().state, "open");
  }

  #[test]
  fn deferred_is_an_event_but_the_card_stays_answerable() {
    let mut s = mem();
    let id = s.create_id(card("Ship?"));
    let (d, _) = s.answer(&id, AnswerIn { deferred: true, ..Default::default() }).unwrap();
    assert!(d.deferred && !d.is_final());
    assert_eq!(s.get(&id).unwrap().unwrap().state, "deferred");
    assert_eq!(s.open_count(), 1);
    assert_eq!(s.wait_event(&id, 0, false).unwrap().unwrap().seq, 1);
    assert!(s.wait_event(&id, 0, true).unwrap().is_none());
    assert!(s.wait_event(&id, 1, false).unwrap().is_none());
    let (f, _) = s.answer(&id, pick_a()).unwrap();
    assert_eq!(f.seq, 2);
    assert_eq!(s.wait_event(&id, 1, true).unwrap().unwrap().seq, 2);
    assert_eq!(s.open_count(), 0);
  }

  #[test]
  fn dismiss_is_a_final_answer() {
    let mut s = mem();
    let id = s.create_id(card("Ship?"));
    let (e, _) = s.answer(&id, AnswerIn { dismissed: true, deferred: true, ..Default::default() }).unwrap();
    assert!(e.dismissed && e.is_final());
    assert_eq!(s.get(&id).unwrap().unwrap().state, "dismissed");
  }

  #[test]
  fn owner_messages_get_per_card_seqs_and_a_thread() {
    let mut s = mem();
    let id = s.create_id(card("Review"));
    let other = s.create_id(card("Other"));
    let m = MessageIn { text: "why?".into(), file: Some("a.rs".into()), line: Some(4), action: Some("fix".into()) };
    let e = s.owner_message(&id, m).unwrap();
    assert_eq!((e.seq, e.kind.as_str(), e.action.as_deref()), (1, "message", Some("fix")));
    assert_eq!(e.thread.unwrap().line, 4);
    let e2 = s
      .owner_message(&id, MessageIn { text: "and?".into(), action: Some("bogus".into()), ..Default::default() })
      .unwrap();
    assert_eq!((e2.seq, e2.action.as_deref()), (2, Some("reply")));
    assert_eq!(s.owner_message(&other, MessageIn { text: "x".into(), ..Default::default() }).unwrap().seq, 1);
    assert_eq!(s.wait_event(&id, 1, false).unwrap().unwrap().seq, 2);
    assert!(s.wait_event(&id, 2, false).unwrap().is_none());
    assert!(s.wait_event(&id, 0, true).unwrap().is_none());
    assert_eq!(s.get(&id).unwrap().unwrap().messages.len(), 2);
  }

  #[test]
  fn agent_replies_are_chat_not_events() {
    let mut s = mem();
    let id = s.create_id(card("Review"));
    let l = LineComment { file: "a.rs".into(), line: 3, text: "fixed".into() };
    s.agent_message(&id, Some("done".into()), vec![l]).unwrap();
    let v = s.get(&id).unwrap().unwrap();
    assert_eq!(v.messages.iter().map(|m| m.author.as_str()).collect::<Vec<_>>(), ["agent", "agent"]);
    assert_eq!(v.messages[1].line, Some(3));
    assert!(s.events(&id).unwrap().is_empty());
    assert!(s.agent_message(&id, None, vec![]).is_err());
    assert!(s.agent_message("c_00000000000000000000000000000000", Some("x".into()), vec![]).is_err());
  }

  #[test]
  fn supersede_closes_only_open_hook_cards_of_the_same_session() {
    let mut s = mem();
    let old = s.create_id(hook("old", "s1"));
    let other_session = s.create_id(hook("other", "s2"));
    let mcp = s.create_id(CardIn { supersede: true, ..card("mcp") });
    let answered = s.create_id(hook("answered", "s1"));
    assert_eq!(s.get(&old).unwrap().unwrap().state, "closed");
    s.answer(&answered, AnswerIn::default()).unwrap();
    let fresh = s.create_id(hook("new", "s1"));
    assert_eq!(s.get(&old).unwrap().unwrap().closed_reason.as_deref(), Some("superseded"));
    assert_eq!(s.get(&answered).unwrap().unwrap().state, "answered");
    assert_eq!(s.get(&other_session).unwrap().unwrap().state, "open");
    assert_eq!(s.get(&mcp).unwrap().unwrap().state, "open");
    assert_eq!(s.get(&fresh).unwrap().unwrap().state, "open");
  }

  #[test]
  fn close_session_closes_open_hook_cards_only() {
    let mut s = mem();
    let h = s.create_id(CardIn { supersede: false, ..hook("h", "s1") });
    let h2 = s.create_id(CardIn { supersede: false, ..hook("h2", "s1") });
    let other = s.create_id(hook("o", "s2"));
    let mcp = s.create_id(card("mcp"));
    assert_eq!(s.close_session("claude", "s1").unwrap(), 2);
    assert_eq!(s.close_session("claude", "s1").unwrap(), 0);
    let v = s.get(&h).unwrap().unwrap();
    assert_eq!((v.state.as_str(), v.closed_reason.as_deref()), ("closed", Some("answered in the session")));
    assert_eq!(s.get(&h2).unwrap().unwrap().state, "closed");
    assert_eq!(s.get(&other).unwrap().unwrap().state, "open");
    assert_eq!(s.get(&mcp).unwrap().unwrap().state, "open");
    assert_eq!(s.open_count(), 2);
  }

  #[test]
  fn liveness_marks_only_cards_with_a_pid() {
    let mut s = mem();
    let mut c = card("a");
    c.source.pid = Some(4242);
    let with = s.create_id(c);
    s.create_id(card("b"));
    assert_eq!(s.live_pids().unwrap(), vec![(with.clone(), 4242)]);
    s.mark_gone(std::slice::from_ref(&with)).unwrap();
    assert!(s.live_pids().unwrap().is_empty());
    assert!(s.get(&with).unwrap().unwrap().session_gone);
  }
}
