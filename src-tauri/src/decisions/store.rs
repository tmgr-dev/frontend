use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tmgr_ipc::decisions::{
  new_card_id, valid_card_id, CardIn, Check, DiffSpec, Event, Fact, LineComment, LinkItem, Links, Opt, PatchIn, Source,
  ThreadRef, MAX_DIFFS, MAX_LINKS, MAX_PATCHES, MAX_PATCH_BYTES, OUTCOME_CREATED, OUTCOME_FINAL, OUTCOME_UPDATED,
};

pub const TITLE_MAX: usize = 300;
pub const TEXT_MAX: usize = 64 * 1024;
const LABEL_MAX: usize = 300;
const FACT_MAX: usize = 1000;
const FIELD_MAX: usize = 1024;
const URL_MAX: usize = 2048;
const KEY_MAX: usize = 200;
const MAX_OPTIONS: usize = 9;
const MAX_ROWS: usize = 50;
const MAX_LINE_COMMENTS: usize = 200;
const LIST_FINISHED: i64 = 200;
const LINK_KINDS: [&str; 5] = ["pr", "commit", "tag", "task", "url"];

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
  pub diffs: Vec<DiffSpec>,
  pub patches: Vec<PatchInfo>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub updated_ms: Option<i64>,
  pub checks: Vec<Check>,
  pub links: Links,
  pub reply_via: String,
  pub answer: Option<Event>,
  pub messages: Vec<MessageView>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PatchInfo {
  pub label: String,
  pub bytes: u64,
}

pub struct Created {
  pub id: String,
  pub fresh: bool,
  pub changed: bool,
  pub outcome: &'static str,
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
  if usize::from(c.diff.is_some()) + c.diffs.len() > MAX_DIFFS {
    return Err(format!("a card takes at most {MAX_DIFFS} repositories"));
  }
  c.diff = c.diff.take().and_then(clean_diff);
  c.diffs = std::mem::take(&mut c.diffs).into_iter().filter_map(clean_diff).collect();
  if c.patches.len() > MAX_PATCHES {
    return Err(format!("a card takes at most {MAX_PATCHES} patches"));
  }
  if c.patches.iter().map(|p| p.text.len()).sum::<usize>() > MAX_PATCH_BYTES {
    return Err(format!("patches are over {} KiB in total", MAX_PATCH_BYTES / 1024));
  }
  for p in &mut c.patches {
    p.label = cap(&p.label, LABEL_MAX);
  }
  if c.links.items.len() > MAX_LINKS {
    return Err(format!("a card takes at most {MAX_LINKS} links"));
  }
  c.links.items = std::mem::take(&mut c.links.items).into_iter().filter_map(clean_link).collect();
  c.links.task = cap_opt(c.links.task.take(), URL_MAX);
  c.links.pr = cap_opt(c.links.pr.take(), URL_MAX);
  c.replaces = cap_opt(c.replaces.take(), 64);
  c.key = cap_opt(c.key.take(), KEY_MAX);
  Ok(c)
}

fn clean_diff(mut d: DiffSpec) -> Option<DiffSpec> {
  d.repo = cap(&d.repo, FIELD_MAX * 4);
  d.base = cap_opt(d.base, 200);
  d.head = cap_opt(d.head, 200);
  (!d.repo.is_empty()).then_some(d)
}

fn clean_link(mut l: LinkItem) -> Option<LinkItem> {
  let url = l.url.trim();
  if url.len() > URL_MAX || !url::Url::parse(url).is_ok_and(|u| matches!(u.scheme(), "http" | "https")) {
    return None;
  }
  l.url = url.to_owned();
  l.kind = pick(&l.kind, &LINK_KINDS, "url");
  l.label = cap_opt(l.label.take(), LABEL_MAX);
  Some(l)
}

fn json_err(e: serde_json::Error) -> String {
  e.to_string()
}

fn sql_err(e: rusqlite::Error) -> String {
  e.to_string()
}

fn is_live(state: &str) -> bool {
  state == "open" || state == "deferred"
}

fn insert_patches(tx: &rusqlite::Transaction<'_>, id: &str, patches: &[PatchIn]) -> Result<(), String> {
  for (i, p) in patches.iter().enumerate() {
    tx.execute(
      "INSERT INTO patches(card_id, idx, label, text) VALUES(?1, ?2, ?3, ?4)",
      params![id, i as i64, p.label, p.text],
    )
    .map_err(sql_err)?;
  }
  Ok(())
}

struct Row {
  id: String,
  created_ms: i64,
  card: String,
  state: String,
  session_gone: bool,
  answer: Option<String>,
  closed_reason: Option<String>,
  updated_ms: Option<i64>,
}

const CARD_COLS: &str = "id, created_ms, card, state, session_gone, answer, closed_reason, updated_ms";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Row> {
  Ok(Row {
    id: r.get(0)?,
    created_ms: r.get(1)?,
    card: r.get(2)?,
    state: r.get(3)?,
    session_gone: r.get::<_, i64>(4)? != 0,
    answer: r.get(5)?,
    closed_reason: r.get(6)?,
    updated_ms: r.get(7)?,
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
           file TEXT, line INTEGER, created_ms INTEGER NOT NULL, PRIMARY KEY(card_id, seq));
         CREATE TABLE IF NOT EXISTS patches(
           card_id TEXT NOT NULL, idx INTEGER NOT NULL, label TEXT NOT NULL, text TEXT NOT NULL,
           PRIMARY KEY(card_id, idx));
         CREATE TABLE IF NOT EXISTS aliases(alias TEXT PRIMARY KEY, card_id TEXT NOT NULL);",
      )
      .map_err(sql_err)?;
    let has_updated: i64 = conn
      .query_row("SELECT COUNT(*) FROM pragma_table_info('cards') WHERE name='updated_ms'", [], |r| r.get(0))
      .map_err(sql_err)?;
    if has_updated == 0 {
      conn.execute("ALTER TABLE cards ADD COLUMN updated_ms INTEGER", []).map_err(sql_err)?;
    }
    Ok(Store { conn })
  }

  pub fn check(card: &CardIn) -> Result<(), String> {
    sanitize(card.clone()).map(|_| ())
  }

  pub fn create(&mut self, card: CardIn) -> Result<Created, String> {
    let mut card = sanitize(card)?;
    let id = card.id.clone().unwrap_or_else(new_card_id);
    card.id = Some(id.clone());
    if let Some(canon) = self.resolve(&id)? {
      let existing = self.raw(&canon)?.ok_or_else(|| "unknown card".to_owned())?;
      let outcome = if canon == id {
        OUTCOME_CREATED
      } else if is_live(&existing.state) {
        OUTCOME_UPDATED
      } else {
        OUTCOME_FINAL
      };
      let card = serde_json::from_str(&existing.card).map_err(json_err)?;
      return Ok(Created { id: canon, fresh: false, changed: false, outcome, card });
    }
    let patches = std::mem::take(&mut card.patches);
    let target = self.resend_target(&card)?.filter(|(t, _)| t.state != "closed");
    if let Some((target, old)) = target {
      let tx = self.conn.transaction().map_err(sql_err)?;
      tx.execute("INSERT INTO aliases(alias, card_id) VALUES(?1, ?2)", params![id, target.id]).map_err(sql_err)?;
      if !is_live(&target.state) {
        tx.commit().map_err(sql_err)?;
        return Ok(Created { id: target.id, fresh: false, changed: false, outcome: OUTCOME_FINAL, card });
      }
      card.id = Some(target.id.clone());
      card.key = card.key.take().or(old.key);
      tx.execute(
        "UPDATE cards SET card=?2, session_gone=0, updated_ms=?3 WHERE id=?1",
        params![target.id, serde_json::to_string(&card).map_err(json_err)?, now_ms()],
      )
      .map_err(sql_err)?;
      tx.execute("DELETE FROM patches WHERE card_id=?1", [&target.id]).map_err(sql_err)?;
      insert_patches(&tx, &target.id, &patches)?;
      tx.commit().map_err(sql_err)?;
      return Ok(Created { id: target.id, fresh: false, changed: true, outcome: OUTCOME_UPDATED, card });
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
    insert_patches(&tx, &id, &patches)?;
    tx.commit().map_err(sql_err)?;
    Ok(Created { id, fresh: true, changed: true, outcome: OUTCOME_CREATED, card })
  }

  fn resolve(&self, id: &str) -> Result<Option<String>, String> {
    self
      .conn
      .query_row(
        "SELECT id FROM cards WHERE id=?1 UNION ALL SELECT card_id FROM aliases WHERE alias=?1 LIMIT 1",
        [id],
        |r| r.get(0),
      )
      .optional()
      .map_err(sql_err)
  }

  pub fn canon(&self, id: &str) -> Result<String, String> {
    self.resolve(id)?.ok_or_else(|| "unknown card".to_owned())
  }

  fn resend_target(&self, card: &CardIn) -> Result<Option<(Row, CardIn)>, String> {
    if card.reply_via == "hook" {
      return Ok(None);
    }
    let canon = if let Some(r) = &card.replaces {
      self.resolve(r)?
    } else if let Some(key) = &card.key {
      self
        .conn
        .query_row(
          "SELECT id FROM cards WHERE json_extract(card,'$.key')=?1 AND json_extract(card,'$.source.agent')=?2
             AND json_extract(card,'$.source.cwd') IS ?3 AND COALESCE(json_extract(card,'$.reply_via'),'')!='hook'
           ORDER BY created_ms DESC, rowid DESC LIMIT 1",
          params![key, card.source.agent, card.source.cwd],
          |r| r.get(0),
        )
        .optional()
        .map_err(sql_err)?
    } else {
      None
    };
    let Some(row) = canon.map(|c| self.raw(&c)).transpose()?.flatten() else { return Ok(None) };
    let old: CardIn = serde_json::from_str(&row.card).map_err(json_err)?;
    Ok((old.reply_via != "hook").then_some((row, old)))
  }

  fn raw(&self, id: &str) -> Result<Option<Row>, String> {
    self.conn.query_row(&format!("SELECT {CARD_COLS} FROM cards WHERE id=?1"), [id], row).optional().map_err(sql_err)
  }

  fn view(&self, r: Row) -> Result<CardView, String> {
    let c: CardIn = serde_json::from_str(&r.card).map_err(json_err)?;
    let answer = r.answer.as_deref().map(serde_json::from_str::<Event>).transpose().map_err(json_err)?;
    let live = is_live(&r.state);
    let patches = self.patch_info(&r.id)?;
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
      diffs: c.diffs,
      patches,
      updated_ms: r.updated_ms,
      checks: c.checks,
      links: c.links,
      reply_via: c.reply_via,
      answer,
      messages,
    })
  }

  fn patch_info(&self, id: &str) -> Result<Vec<PatchInfo>, String> {
    self
      .conn
      .prepare("SELECT label, length(CAST(text AS BLOB)) FROM patches WHERE card_id=?1 ORDER BY idx")
      .map_err(sql_err)?
      .query_map([id], |m| Ok(PatchInfo { label: m.get(0)?, bytes: m.get::<_, i64>(1)? as u64 }))
      .map_err(sql_err)?
      .collect::<Result<Vec<_>, _>>()
      .map_err(sql_err)
  }

  pub fn patches(&self, id: &str) -> Result<Vec<PatchIn>, String> {
    let id = self.canon(id)?;
    self
      .conn
      .prepare("SELECT label, text FROM patches WHERE card_id=?1 ORDER BY idx")
      .map_err(sql_err)?
      .query_map([&id], |m| Ok(PatchIn { label: m.get(0)?, text: m.get(1)? }))
      .map_err(sql_err)?
      .collect::<Result<Vec<_>, _>>()
      .map_err(sql_err)
  }

  pub fn get(&self, id: &str) -> Result<Option<CardView>, String> {
    let Some(id) = self.resolve(id)? else { return Ok(None) };
    self.raw(&id)?.map(|r| self.view(r)).transpose()
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
    if !is_live(&r.state) {
      return Err("the card is already closed".into());
    }
    let c = serde_json::from_str(&r.card).map_err(json_err)?;
    Ok((r, c))
  }

  pub fn answer(&mut self, id: &str, a: AnswerIn) -> Result<(Event, CardIn), String> {
    let id = &self.canon(id)?;
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
    let id = &self.canon(id)?;
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

  pub fn agent_message(
    &mut self,
    id: &str,
    text: Option<String>,
    lines: Vec<LineComment>,
  ) -> Result<String, String> {
    let id = &self.canon(id)?;
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
    tx.commit().map_err(sql_err)?;
    Ok(id.clone())
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
    let id = &self.canon(id)?;
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

  fn patch(label: &str, text: &str) -> PatchIn {
    PatchIn { label: label.into(), text: text.into() }
  }

  fn spec(repo: &str) -> DiffSpec {
    DiffSpec { repo: repo.into(), base: None, head: None }
  }

  fn keyed(title: &str, key: &str) -> CardIn {
    CardIn { key: Some(key.into()), ..card(title) }
  }

  fn replacing(title: &str, target: &str) -> CardIn {
    CardIn { replaces: Some(target.into()), ..card(title) }
  }

  #[test]
  fn limits_are_refused_not_truncated() {
    let mut s = mem();
    let too_many_repos = CardIn { diff: Some(spec("/a")), diffs: (0..MAX_DIFFS).map(|_| spec("/b")).collect(), ..card("x") };
    assert!(s.create(too_many_repos).is_err());
    let max_repos = CardIn { diff: Some(spec("/a")), diffs: (1..MAX_DIFFS).map(|_| spec("/b")).collect(), ..card("x") };
    assert!(s.create(max_repos).is_ok());
    let many_patches = CardIn { patches: (0..=MAX_PATCHES).map(|i| patch(&i.to_string(), "x")).collect(), ..card("x") };
    assert!(s.create(many_patches).is_err());
    let half = "a".repeat(MAX_PATCH_BYTES / 2 + 1);
    let heavy = CardIn { patches: vec![patch("a", &half), patch("b", &half)], ..card("x") };
    assert!(s.create(heavy).is_err());
    let exact = CardIn { patches: vec![patch("a", &"a".repeat(MAX_PATCH_BYTES))], ..card("x") };
    let id = s.create_id(exact);
    assert_eq!(s.get(&id).unwrap().unwrap().patches[0].bytes, MAX_PATCH_BYTES as u64);
    let link = || LinkItem { kind: "url".into(), url: "https://example.com/x".into(), label: None };
    let links = |n: usize| Links { items: (0..n).map(|_| link()).collect(), ..Default::default() };
    assert!(s.create(CardIn { links: links(MAX_LINKS + 1), ..card("x") }).is_err());
    assert!(s.create(CardIn { links: links(MAX_LINKS), ..card("x") }).is_ok());
  }

  #[test]
  fn extra_repos_are_validated_like_the_first() {
    let mut s = mem();
    let long = DiffSpec { repo: "/r".into(), base: Some("b".repeat(500)), head: None };
    let id = s.create_id(CardIn { diffs: vec![spec("  "), long, spec("/ok")], ..card("x") });
    let v = s.get(&id).unwrap().unwrap();
    assert_eq!(v.diffs.len(), 2);
    assert_eq!(v.diffs[0].base.as_ref().unwrap().len(), 200);
  }

  #[test]
  fn links_keep_only_web_urls_and_known_kinds() {
    let mut s = mem();
    let item = |kind: &str, url: &str| LinkItem { kind: kind.into(), url: url.into(), label: Some("l".into()) };
    let long = format!("https://example.com/{}", "a".repeat(URL_MAX));
    let links = Links {
      items: vec![
        item("pr", "https://github.com/o/r/pull/1"),
        item("commit", "javascript:alert(1)"),
        item("tag", "file:///etc/passwd"),
        item("weird", "http://example.com/x"),
        item("url", &long),
        item("url", "not a url"),
      ],
      ..Default::default()
    };
    let id = s.create_id(CardIn { links, ..card("x") });
    let items = s.get(&id).unwrap().unwrap().links.items;
    assert_eq!(items.iter().map(|i| (i.kind.as_str(), i.url.as_str())).collect::<Vec<_>>(), [
      ("pr", "https://github.com/o/r/pull/1"),
      ("url", "http://example.com/x")
    ]);
  }

  #[test]
  fn patch_text_stays_out_of_the_card_json_and_the_light_views() {
    let mut s = mem();
    let id = s.create_id(CardIn { patches: vec![patch("one", "--- a\n+++ b\n"), patch("", "xy")], ..card("x") });
    let stored: String = s.conn.query_row("SELECT card FROM cards WHERE id=?1", [&id], |r| r.get(0)).unwrap();
    assert!(!stored.contains("+++"), "{stored}");
    for v in [s.get(&id).unwrap().unwrap(), s.list().unwrap().remove(0)] {
      assert_eq!(v.patches, vec![PatchInfo { label: "one".into(), bytes: 12 }, PatchInfo { label: String::new(), bytes: 2 }]);
    }
    assert_eq!(s.patches(&id).unwrap()[0].text, "--- a\n+++ b\n");
  }

  #[test]
  fn resend_by_replaces_updates_in_place_and_keeps_the_thread() {
    let mut s = mem();
    let a = s.create_id(CardIn { patches: vec![patch("p", "old")], ..card("first") });
    s.owner_message(&a, MessageIn { text: "why?".into(), ..Default::default() }).unwrap();
    s.agent_message(&a, Some("because".into()), vec![]).unwrap();
    s.answer(&a, AnswerIn { deferred: true, ..Default::default() }).unwrap();
    s.mark_gone(std::slice::from_ref(&a)).unwrap();
    let before = s.get(&a).unwrap().unwrap();
    assert_eq!(before.updated_ms, None);
    let b = new_card_id();
    let r = s
      .create(CardIn {
        id: Some(b.clone()),
        patches: vec![patch("p2", "new1"), patch("p3", "new2")],
        ..replacing("second", &a)
      })
      .unwrap();
    assert_eq!((r.id.as_str(), r.fresh, r.changed, r.outcome), (a.as_str(), false, true, "updated"));
    let after = s.get(&a).unwrap().unwrap();
    assert_eq!((after.title.as_str(), after.state.as_str()), ("second", "deferred"));
    assert_eq!((after.id.as_str(), after.created_ms), (a.as_str(), before.created_ms));
    assert!(!after.session_gone && after.updated_ms.is_some());
    assert_eq!(after.messages.len(), 2);
    assert_eq!(s.events(&a).unwrap().len(), 2);
    assert_eq!(after.patches.len(), 2);
    assert_eq!(s.patches(&a).unwrap()[0].text, "new1");
    assert_eq!(s.list().unwrap().len(), 1);
    assert_eq!(s.get(&b).unwrap().unwrap().id, a);
    let again = s.create(CardIn { id: Some(b.clone()), ..replacing("third", &a) }).unwrap();
    assert_eq!((again.id.as_str(), again.changed, again.outcome), (a.as_str(), false, "updated"));
    assert_eq!(s.get(&a).unwrap().unwrap().title, "second");
  }

  #[test]
  fn resend_chain_resolves_every_id_to_the_first_card() {
    let mut s = mem();
    let a = s.create_id(card("a"));
    let b = new_card_id();
    let c = new_card_id();
    s.create(CardIn { id: Some(b.clone()), ..replacing("b", &a) }).unwrap();
    let r = s.create(CardIn { id: Some(c.clone()), ..replacing("c", &b) }).unwrap();
    assert_eq!(r.id, a);
    assert_eq!(s.list().unwrap().len(), 1);
    let (e, _) = s.answer(&c, pick_a()).unwrap();
    assert_eq!(e.card_id, a);
    for id in [&a, &b, &c] {
      assert_eq!(s.get(id).unwrap().unwrap().state, "answered");
      assert_eq!(s.wait_event(id, 0, true).unwrap().unwrap().seq, 1);
    }
  }

  #[test]
  fn answered_and_dismissed_targets_are_never_reopened() {
    let mut s = mem();
    for dismissed in [false, true] {
      let a = s.create_id(card("a"));
      s.answer(&a, AnswerIn { choice: Some("a".into()), dismissed, ..Default::default() }).unwrap();
      let b = new_card_id();
      let r = s.create(CardIn { id: Some(b.clone()), patches: vec![patch("p", "x")], ..replacing("again", &a) }).unwrap();
      assert_eq!((r.id.as_str(), r.fresh, r.changed, r.outcome), (a.as_str(), false, false, "final"));
      let v = s.get(&b).unwrap().unwrap();
      assert_eq!((v.id.as_str(), v.title.as_str()), (a.as_str(), "a"));
      assert!(v.patches.is_empty() && v.updated_ms.is_none());
      assert_eq!(s.wait_event(&b, 0, true).unwrap().unwrap().dismissed, dismissed);
      let redo = s.create(CardIn { id: Some(b), ..replacing("again", &a) }).unwrap();
      assert_eq!(redo.outcome, "final");
    }
    assert_eq!(s.list().unwrap().len(), 2);
  }

  #[test]
  fn closed_and_unknown_targets_create_a_new_card() {
    let mut s = mem();
    let h = s.create_id(CardIn { supersede: false, reply_via: "mcp".into(), ..card("old") });
    s.conn.execute("UPDATE cards SET state='closed', closed_reason='superseded' WHERE id=?1", [&h]).unwrap();
    let r = s.create(replacing("new", &h)).unwrap();
    assert!(r.fresh && r.outcome == "created" && r.id != h);
    let unknown = s.create(replacing("fresh", "c_00000000000000000000000000000000")).unwrap();
    assert!(unknown.fresh && unknown.outcome == "created");
    assert_eq!(s.get(&h).unwrap().unwrap().title, "old");
  }

  #[test]
  fn replaces_wins_over_key_even_when_unknown() {
    let mut s = mem();
    let a = s.create_id(keyed("a", "k"));
    let r = s.create(CardIn { replaces: Some("c_00000000000000000000000000000000".into()), ..keyed("b", "k") }).unwrap();
    assert!(r.fresh);
    assert_eq!(s.get(&a).unwrap().unwrap().title, "a");
  }

  #[test]
  fn hook_cards_are_neither_source_nor_target() {
    let mut s = mem();
    let hid = s.create_id(CardIn { key: Some("k".into()), ..hook("h", "s1") });
    assert!(s.create(replacing("x", &hid)).unwrap().fresh);
    assert!(s.create(keyed("y", "k")).unwrap().fresh);
    let a = s.create_id(keyed("a", "k2"));
    let h2 = s.create(CardIn { replaces: Some(a.clone()), key: Some("k2".into()), ..hook("h2", "s1") }).unwrap();
    assert!(h2.fresh);
    assert_eq!(s.get(&a).unwrap().unwrap().title, "a");
  }

  #[test]
  fn shared_task_link_alone_never_merges() {
    let mut s = mem();
    let links = || Links { task: Some("https://tmgr.dev/ws/tasks/1".into()), ..Default::default() };
    let a = s.create_id(CardIn { links: links(), ..card("a") });
    let b = s.create_id(CardIn { links: links(), ..card("b") });
    assert_ne!(a, b);
    assert_eq!(s.list().unwrap().len(), 2);
  }

  #[test]
  fn key_scope_is_agent_and_cwd() {
    let mut s = mem();
    let with = |agent: &str, cwd: Option<&str>| {
      let mut c = keyed("t", "k");
      c.source.agent = agent.into();
      c.source.cwd = cwd.map(Into::into);
      c
    };
    let a = s.create_id(with("claude", Some("/p")));
    assert!(s.create(with("claude", Some("/q"))).unwrap().fresh);
    assert!(s.create(with("codex", Some("/p"))).unwrap().fresh);
    assert!(s.create(with("claude", None)).unwrap().fresh);
    let same = s.create(with("claude", Some("/p"))).unwrap();
    assert_eq!((same.id.as_str(), same.fresh, same.outcome), (a.as_str(), false, "updated"));
    let none = s.create_id(with("cursor", None));
    assert_eq!(s.create(with("cursor", None)).unwrap().id, none);
    assert_eq!(s.list().unwrap().len(), 5);
  }

  #[test]
  fn key_targets_the_newest_card_and_a_final_answer_stays_final() {
    let mut s = mem();
    let a = s.create_id(keyed("a", "k"));
    s.answer(&a, pick_a()).unwrap();
    let again = s.create(keyed("a2", "k")).unwrap();
    assert_eq!((again.id.as_str(), again.outcome), (a.as_str(), "final"));
    assert_eq!(s.list().unwrap().len(), 1);
  }

  #[test]
  fn supersede_runs_only_for_new_cards() {
    let mut s = mem();
    let old = s.create_id(hook("old", "s1"));
    let a = s.create_id(CardIn { supersede: true, ..keyed("a", "k") });
    assert_eq!(s.get(&old).unwrap().unwrap().state, "open");
    s.create(CardIn { supersede: true, ..keyed("a2", "k") }).unwrap();
    assert_eq!(s.get(&old).unwrap().unwrap().state, "open");
    assert_eq!(s.get(&a).unwrap().unwrap().title, "a2");
  }

  #[test]
  fn old_shape_rows_and_old_databases_still_open() {
    let dir = tempfile::Builder::new().prefix("ds").tempdir_in("/tmp").unwrap();
    let path = dir.path().join("decisions.db");
    {
      let c = Connection::open(&path).unwrap();
      c.execute_batch(
        "CREATE TABLE cards(id TEXT PRIMARY KEY, created_ms INTEGER NOT NULL, card TEXT NOT NULL, state TEXT NOT NULL,
           session_gone INTEGER NOT NULL DEFAULT 0, answer TEXT, closed_reason TEXT);
         CREATE TABLE events(card_id TEXT NOT NULL, seq INTEGER NOT NULL, json TEXT NOT NULL, PRIMARY KEY(card_id, seq));
         CREATE TABLE messages(card_id TEXT NOT NULL, seq INTEGER NOT NULL, author TEXT NOT NULL, text TEXT NOT NULL,
           file TEXT, line INTEGER, created_ms INTEGER NOT NULL, PRIMARY KEY(card_id, seq));
         INSERT INTO cards(id, created_ms, card, state) VALUES('c_11111111111111111111111111111111', 5,
           '{\"title\":\"Legacy\",\"diff\":{\"repo\":\"/r\"},\"links\":{\"pr\":\"https://x.test/1\"}}', 'open');",
      )
      .unwrap();
    }
    let mut s = Store::open(&path).unwrap();
    let id = "c_11111111111111111111111111111111";
    let v = s.get(id).unwrap().unwrap();
    assert_eq!((v.title.as_str(), v.created_ms, v.updated_ms), ("Legacy", 5, None));
    assert!(v.diffs.is_empty() && v.patches.is_empty() && v.diff.is_some());
    assert_eq!(s.list().unwrap().len(), 1);
    let r = s.create(CardIn { replaces: Some(id.into()), ..card("Fresh") }).unwrap();
    assert_eq!((r.id.as_str(), r.outcome), (id, "updated"));
    assert!(Store::open(&path).unwrap().get(id).unwrap().unwrap().updated_ms.is_some());
  }
}
