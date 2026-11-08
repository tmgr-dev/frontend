use std::collections::BTreeSet;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use super::phone;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tmgr_phone::AckResult;
use tmgr_ipc::decisions::{
  new_card_id, valid_card_id, CardIn, Check, DiffSpec, Event, Fact, Found, Inbox, LineComment, LinkItem, Links, Opt,
  PatchIn, PendingDelivery, Secret, Source, ThreadRef, MAX_DIFFS, MAX_LINKS, MAX_PATCHES, MAX_PATCH_BYTES, OUTCOME_CREATED, OUTCOME_FINAL,
  OUTCOME_SIMILAR, OUTCOME_UPDATED,
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
const THREADS_LISTED: usize = 3;
const LIST_FINISHED: i64 = 200;
const LINK_KINDS: [&str; 5] = ["pr", "commit", "tag", "task", "url"];
pub const FIND_LIMIT: usize = 10;
const FIND_MAX: usize = 50;
const FIND_SCAN: usize = 2000;
const SIMILAR_WINDOW_MS: i64 = 7 * 24 * 3600 * 1000;
const SIMILAR_SCAN: usize = 50;
const SIMILAR_MAX: usize = 3;
const SIMILAR_MIN_SCORE: f64 = 0.8;
const FOUND_TEXT_MAX: usize = 2000;
const UNPICKED_MS: i64 = 30 * 60 * 1000;
const NOT_TASK_KEYS: [&str; 8] = ["utf", "sha", "gpt", "iso", "rfc", "node", "python", "es"];

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
  pub side: Option<String>,
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
  #[serde(skip_serializing_if = "Option::is_none")]
  pub side: Option<String>,
  pub created_ms: i64,
  pub delivered: bool,
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
  pub listening: bool,
  pub push: bool,
  pub delivery: Option<DeliveryView>,
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
  pub unpicked: bool,
  #[serde(skip)]
  pub rev: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PendingTaskComment {
  pub card_id: String,
  pub seq: u64,
  pub task_id: i64,
  pub origin: String,
  pub body: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DeliveryView {
  pub state: String,
  pub via: String,
  pub detail: String,
  pub at_ms: i64,
}

#[derive(Debug, Clone)]
pub struct Target {
  pub kind: String,
  pub endpoint: String,
  pub token: Option<Secret>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recorded {
  Same,
  New,
  Changed,
}

pub struct Routing {
  pub id: String,
  pub card: CardIn,
  pub state: String,
  pub gone: bool,
  pub latest_seq: u64,
  pub targets: Vec<Target>,
  pub target: Option<String>,
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
  pub similar: Vec<Found>,
}

const PHONE_ANSWER_KEEP_MS: i64 = 72 * 3600 * 1000;

pub struct PhoneReq {
  pub answer_id: String,
  pub phone_fp: String,
  pub card_id: String,
  pub card_digest: String,
  pub session: String,
  pub action: tmgr_phone::Action,
  pub choice: Option<String>,
  pub text: Option<String>,
  pub issued_at: i64,
}

#[derive(Debug)]
pub struct PhoneOutcome {
  pub result: AckResult,
  pub winner: Option<String>,
  pub label: Option<String>,
  pub applied: Option<(Event, CardIn)>,
  pub target: String,
}

impl PhoneOutcome {
  fn rejected(result: AckResult, winner: Option<String>, label: Option<String>) -> PhoneOutcome {
    PhoneOutcome { result, winner, label, applied: None, target: String::new() }
  }
}

#[derive(Serialize, Deserialize)]
struct StoredAck {
  result: String,
  winner: Option<String>,
  label: Option<String>,
  #[serde(default)]
  routed: bool,
}

fn event_label(e: &Event) -> String {
  if e.dismissed {
    "Dismissed".into()
  } else if e.deferred {
    "Deferred".into()
  } else {
    e.choice_label.clone().unwrap_or_else(|| "Text reply".into())
  }
}

fn phone_answer_in_of(r: &PhoneReq, card: &CardIn) -> Option<AnswerIn> {
  use tmgr_phone::Action;
  match r.action {
    Action::Defer | Action::Dismiss if r.choice.is_some() || r.text.is_some() => None,
    Action::Defer => Some(AnswerIn { deferred: true, ..Default::default() }),
    Action::Dismiss => Some(AnswerIn { dismissed: true, ..Default::default() }),
    Action::Answer => {
      let choice_ok = match &r.choice {
        Some(c) => card.options.iter().any(|o| &o.id == c),
        None => true,
      };
      let text_ok = r.text.as_deref().is_none_or(|t| !t.trim().is_empty() && tmgr_phone::check_text(t).is_ok());
      if !choice_ok || !text_ok || (r.choice.is_none() && r.text.is_none()) {
        return None;
      }
      Some(AnswerIn { choice: r.choice.clone(), free_text: r.text.clone(), ..Default::default() })
    }
  }
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
    f.link = safe_ref(f.link.take());
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
  c.links.task = safe_ref(c.links.task.take());
  c.links.pr = safe_ref(c.links.pr.take());
  c.replaces = cap_opt(c.replaces.take(), 64);
  c.key = cap_opt(c.key.take(), KEY_MAX);
  Ok(c)
}

fn safe_ref(v: Option<String>) -> Option<String> {
  cap_opt(v, URL_MAX).filter(|s| !tmgr_ipc::decisions::has_foreign_scheme(s))
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

fn key_len(title: &str) -> Option<usize> {
  let t = title.trim_start();
  let letters = t.bytes().take_while(u8::is_ascii_alphabetic).count();
  if !(2..=6).contains(&letters) {
    return None;
  }
  let digits = t[letters..].strip_prefix('-')?.bytes().take_while(u8::is_ascii_digit).count();
  let word = t[..letters].to_ascii_lowercase();
  (digits > 0 && !NOT_TASK_KEYS.contains(&word.as_str())).then_some(letters + 1 + digits)
}

fn title_key(title: &str) -> Option<String> {
  let t = title.trim_start();
  key_len(t).map(|n| t[..n].to_ascii_lowercase())
}

fn title_body(title: &str) -> &str {
  let t = title.trim_start();
  &t[key_len(t).unwrap_or(0)..]
}

fn task_path(link: &str) -> String {
  match link.rsplit_once("/tasks/") {
    Some((_, n)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => format!("tasks/{n}"),
    _ => link.to_owned(),
  }
}

fn task_ids(task: Option<&str>, title: &str) -> BTreeSet<String> {
  let mut ids: BTreeSet<String> = title_key(title).into_iter().collect();
  if let Some(t) = task.map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()) {
    ids.insert(task_path(&t));
  }
  ids
}

fn is_exact_token(t: &str) -> bool {
  t.bytes().any(|b| b.is_ascii_digit()) || (t.chars().count() == 1 && t.is_ascii())
}

fn tokens(c: &CardIn) -> Vec<String> {
  let question = c.question.as_deref().unwrap_or("");
  format!("{} {}", title_body(&c.title), question)
    .split(|ch: char| !ch.is_alphanumeric())
    .filter(|t| !t.is_empty())
    .map(str::to_lowercase)
    .collect()
}

fn words(c: &CardIn) -> (BTreeSet<String>, BTreeSet<String>) {
  let (mut exact, mut stems) = (BTreeSet::new(), BTreeSet::new());
  for t in tokens(c) {
    if is_exact_token(&t) {
      exact.insert(t);
    } else if t.chars().count() >= 3 {
      stems.insert(t.chars().take(5).collect());
    }
  }
  (exact, stems)
}

fn has_question(c: &CardIn) -> bool {
  c.question.as_deref().is_some_and(|q| !q.trim().is_empty())
}

fn similarity(new: &CardIn, old: &CardIn) -> Option<f64> {
  if !has_question(new) || !has_question(old) {
    return None;
  }
  let new_ids = task_ids(new.links.task.as_deref(), &new.title);
  if new_ids.is_disjoint(&task_ids(old.links.task.as_deref(), &old.title)) {
    return None;
  }
  let ((exact_a, a), (exact_b, b)) = (words(new), words(old));
  if exact_a != exact_b {
    return None;
  }
  let (inter, union) = (a.intersection(&b).count(), a.union(&b).count());
  let score = if union == 0 { 0.0 } else { inter as f64 / union as f64 };
  (score >= SIMILAR_MIN_SCORE).then_some(score)
}

fn parse_answer(r: &Row) -> Result<Option<Event>, String> {
  r.answer.as_deref().map(serde_json::from_str::<Event>).transpose().map_err(json_err)
}

fn found(r: &Row, c: &CardIn, answer: Option<&Event>) -> Found {
  let dismissed = r.state == "dismissed";
  Found {
    card_id: r.id.clone(),
    title: c.title.clone(),
    question: cap_opt(c.question.clone(), FOUND_TEXT_MAX),
    state: r.state.clone(),
    choice_label: answer.filter(|_| !dismissed).and_then(|a| a.choice_label.clone()),
    free_text: cap_opt(answer.and_then(|a| a.free_text.clone()), FOUND_TEXT_MAX),
    note: cap_opt(answer.and_then(|a| a.note.clone()), FOUND_TEXT_MAX),
    dismissed,
    key: c.key.clone(),
    task: c.links.task.clone().or_else(|| title_key(&c.title)),
    created_ms: r.created_ms,
  }
}

fn expects_agent(c: &CardIn) -> bool {
  matches!(c.reply_via.as_str(), "mcp" | "cli") || (c.reply_via == "hook" && c.kind == "stopped")
}

fn cloud_task(c: &CardIn) -> Option<(String, i64)> {
  let urls = c.links.task.iter().map(String::as_str).chain(c.links.items.iter().filter(|i| i.kind == "task").map(|i| i.url.as_str()));
  urls.filter_map(parse_cloud_task).next()
}

fn parse_cloud_task(link: &str) -> Option<(String, i64)> {
  let url = url::Url::parse(link.trim()).ok()?;
  if !matches!(url.scheme(), "http" | "https") {
    return None;
  }
  let host = url.host_str()?;
  let segments: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
  let [ws, "tasks", id] = segments[..] else { return None };
  if ws.is_empty() || id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
    return None;
  }
  let origin = match url.port() {
    Some(port) => format!("{}://{host}:{port}", url.scheme()),
    None => format!("{}://{host}", url.scheme()),
  };
  Some((origin, id.parse().ok()?))
}

fn comment_body(c: &CardIn, a: &Event, card_id: &str) -> String {
  let mut lines = vec![format!("Decision answered: {}", c.title)];
  if let Some(q) = c.question.as_deref().filter(|q| !q.trim().is_empty()) {
    lines.push(format!("Question: {q}"));
  }
  if let Some(choice) = a.choice_label.as_deref().or(a.choice.as_deref()).filter(|v| !v.is_empty()) {
    lines.push(format!("Answer: {choice}"));
  }
  if let Some(t) = a.free_text.as_deref().filter(|t| !t.trim().is_empty()) {
    lines.push(format!("Free text: {t}"));
  }
  if let Some(n) = a.note.as_deref().filter(|n| !n.trim().is_empty()) {
    lines.push(format!("Note: {n}"));
  }
  lines.push(format!("Card: {card_id}"));
  lines.push(format!("[decision {card_id}#{}]", a.seq));
  lines.join("\n\n")
}

fn is_live(state: &str) -> bool {
  state == "open" || state == "deferred"
}

fn pid_of(endpoint: &str) -> Option<String> {
  let name = endpoint.rsplit('/').next()?;
  let digits: String = name.chars().take_while(char::is_ascii_digit).collect();
  (!digits.is_empty()).then_some(digits)
}

fn plain(s: String, max: usize) -> Option<String> {
  (!s.is_empty() && s.chars().count() <= max && !s.chars().any(|c| c.is_control() || c == '|')).then_some(s)
}

fn endpoint_tag(endpoint: &str) -> String {
  tmgr_phone::hex(&Sha256::digest(endpoint.as_bytes())).chars().take(16).collect()
}

pub fn delivery_target(conn: &Connection, card_id: &str) -> Option<String> {
  let mut parts: Vec<String> = Vec::new();
  if let Ok(mut stmt) = conn.prepare("SELECT kind, endpoint, session FROM push_targets WHERE card_id=?1") {
    let rows = stmt
      .query_map([card_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?)))
      .map(|rows| rows.filter_map(Result::ok).collect::<Vec<_>>())
      .unwrap_or_default();
    for (kind, endpoint, session) in rows {
      let who = match session.filter(|s| !s.is_empty()) {
        Some(s) if kind == "claude" => format!("{s}@{}", endpoint_tag(&endpoint)),
        Some(s) => s,
        None => pid_of(&endpoint).unwrap_or_else(|| "unknown".into()),
      };
      parts.push(format!("{kind}:{who}"));
    }
  }
  let runner: Option<String> = conn
    .query_row("SELECT json_extract(card,'$.source.runner_session') FROM cards WHERE id=?1", [card_id], |r| r.get(0))
    .ok()
    .flatten();
  parts.extend(runner.and_then(|r| plain(r, 64)).map(|r| format!("runner:{r}")));
  parts.sort();
  parts.dedup();
  (!parts.is_empty()).then(|| parts.join("|"))
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

type Thread = (String, u32, Option<String>);
type Item = (String, Option<String>, Option<u32>, Option<String>);

struct ThreadRow {
  owner: bool,
  thread: Option<Thread>,
}

fn resolve_side(rows: &[ThreadRow], file: &str, line: u32, given: Option<String>) -> Option<String> {
  if given.is_some() {
    return given;
  }
  let mut threads: Vec<(&Option<String>, bool)> = Vec::new();
  for row in rows {
    if let Some((f, l, side)) = &row.thread {
      if f == file && *l == line {
        match threads.iter_mut().find(|(s, _)| *s == side) {
          Some(t) => t.1 = row.owner,
          None => threads.push((side, row.owner)),
        }
      }
    }
  }
  match threads.iter().find(|(_, pending)| *pending).or(threads.last()) {
    Some((side, _)) => (*side).clone(),
    None => None,
  }
}

impl Store {
  pub fn open(path: &Path) -> Result<Store, String> {
    let conn = Connection::open(path).map_err(sql_err)?;
    conn.busy_timeout(std::time::Duration::from_secs(5)).map_err(sql_err)?;
    let _ = conn.pragma_update(None, "journal_mode", "WAL");
    let had_comments: bool = conn
      .query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='task_comments')", [], |r| r.get(0))
      .map_err(sql_err)?;
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
         CREATE TABLE IF NOT EXISTS aliases(alias TEXT PRIMARY KEY, card_id TEXT NOT NULL);
         CREATE TABLE IF NOT EXISTS phone_answers(
           answer_id TEXT PRIMARY KEY, phone_fp TEXT NOT NULL, card_id TEXT NOT NULL, ack TEXT NOT NULL,
           received_ms INTEGER NOT NULL);
         CREATE TABLE IF NOT EXISTS push_targets(
           card_id TEXT NOT NULL, endpoint TEXT NOT NULL, kind TEXT NOT NULL, token TEXT, session TEXT,
           updated_ms INTEGER NOT NULL, PRIMARY KEY(card_id, endpoint));
         CREATE TABLE IF NOT EXISTS deliveries(
           card_id TEXT NOT NULL, seq INTEGER NOT NULL, state TEXT NOT NULL, via TEXT NOT NULL,
           detail TEXT NOT NULL, at_ms INTEGER NOT NULL, PRIMARY KEY(card_id, seq));
         CREATE TABLE IF NOT EXISTS task_comments(
           card_id TEXT NOT NULL, seq INTEGER NOT NULL, posted_ms INTEGER NOT NULL, PRIMARY KEY(card_id, seq));
         UPDATE deliveries SET state='undelivered', detail='desktop restarted' WHERE state='queued';",
      )
      .map_err(sql_err)?;
    let has_updated: i64 = conn
      .query_row("SELECT COUNT(*) FROM pragma_table_info('cards') WHERE name='updated_ms'", [], |r| r.get(0))
      .map_err(sql_err)?;
    if has_updated == 0 {
      conn.execute("ALTER TABLE cards ADD COLUMN updated_ms INTEGER", []).map_err(sql_err)?;
    }
    let has_side: i64 = conn
      .query_row("SELECT COUNT(*) FROM pragma_table_info('messages') WHERE name='side'", [], |r| r.get(0))
      .map_err(sql_err)?;
    if has_side == 0 {
      conn.execute("ALTER TABLE messages ADD COLUMN side TEXT", []).map_err(sql_err)?;
    }
    let has_at: i64 = conn
      .query_row("SELECT COUNT(*) FROM pragma_table_info('events') WHERE name='at_ms'", [], |r| r.get(0))
      .map_err(sql_err)?;
    if has_at == 0 {
      conn.execute("ALTER TABLE events ADD COLUMN at_ms INTEGER", []).map_err(sql_err)?;
    }
    if !had_comments {
      conn
        .execute(
          "INSERT OR IGNORE INTO task_comments(card_id, seq, posted_ms)
           SELECT d.card_id, d.seq, 0 FROM deliveries d JOIN cards c ON c.id=d.card_id
           WHERE d.state IN ('delivered','sent') AND c.state IN ('answered','dismissed')
             AND d.seq = json_extract(c.answer, '$.seq')",
          [],
        )
        .map_err(sql_err)?;
    }
    Ok(Store { conn })
  }

  pub fn check(card: &CardIn) -> Result<(), String> {
    sanitize(card.clone()).map(|_| ())
  }

  pub fn create(&mut self, card: CardIn) -> Result<Created, String> {
    self.create_with(card, None)
  }

  pub fn create_with(&mut self, card: CardIn, inbox: Option<Inbox>) -> Result<Created, String> {
    let created = self.create_card(card)?;
    if let Some(inbox) = inbox.filter(|_| matches!(created.outcome, OUTCOME_CREATED | OUTCOME_UPDATED)) {
      self.upsert_target(&created.id, inbox)?;
    }
    Ok(created)
  }

  fn upsert_target(&self, id: &str, inbox: Inbox) -> Result<(), String> {
    let (Some(kind), Some(endpoint)) = (plain(inbox.kind, 32), plain(inbox.endpoint, 512)) else { return Ok(()) };
    let token = inbox.token.map(|t| t.expose().to_owned()).filter(|t| !t.is_empty());
    let session = inbox.session.and_then(|s| plain(s, 256));
    self
      .conn
      .execute(
        "INSERT INTO push_targets(card_id, endpoint, kind, token, session, updated_ms) VALUES(?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(card_id, endpoint) DO UPDATE SET kind=?3, token=?4, session=?5, updated_ms=?6",
        params![id, endpoint, kind, token, session, now_ms()],
      )
      .map(drop)
      .map_err(sql_err)
  }

  pub fn drop_target(&self, id: &str, endpoint: &str) {
    let _ = self.conn.execute("DELETE FROM push_targets WHERE card_id=?1 AND endpoint=?2", params![id, endpoint]);
  }

  pub fn record_delivery(&self, id: &str, seq: u64, state: &str, via: &str, detail: &str) -> Result<Recorded, String> {
    let same = self
      .conn
      .query_row(
        "SELECT state=?3 AND via=?4 AND detail=?5,
                (state IN ('delivered','sent') AND ?3 NOT IN ('delivered','sent')) OR (state='delivered' AND ?3='sent')
         FROM deliveries WHERE card_id=?1 AND seq=?2",
        params![id, seq as i64, state, via, detail],
        |r| Ok((r.get::<_, bool>(0)?, r.get::<_, bool>(1)?)),
      )
      .optional()
      .map_err(sql_err)?;
    if matches!(same, Some((true, _) | (_, true))) {
      return Ok(Recorded::Same);
    }
    self
      .conn
      .execute(
        "INSERT INTO deliveries(card_id, seq, state, via, detail, at_ms) VALUES(?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(card_id, seq) DO UPDATE SET state=?3, via=?4, detail=?5, at_ms=?6",
        params![id, seq as i64, state, via, detail, now_ms()],
      )
      .map_err(sql_err)?;
    Ok(if same.is_some() { Recorded::Changed } else { Recorded::New })
  }

  pub fn routing(&self, id: &str) -> Result<Option<Routing>, String> {
    let Some(id) = self.resolve(id)? else { return Ok(None) };
    let Some(r) = self.raw(&id)? else { return Ok(None) };
    let card = serde_json::from_str(&r.card).map_err(json_err)?;
    let latest_seq: i64 =
      self.conn.query_row("SELECT COALESCE(MAX(seq),0) FROM events WHERE card_id=?1", [&id], |r| r.get(0)).map_err(sql_err)?;
    let targets = self
      .conn
      .prepare("SELECT kind, endpoint, token FROM push_targets WHERE card_id=?1 ORDER BY endpoint")
      .map_err(sql_err)?
      .query_map([&id], |t| {
        Ok(Target {
          kind: t.get(0)?,
          endpoint: t.get(1)?,
          token: t.get::<_, Option<String>>(2)?.map(Secret::new),
        })
      })
      .map_err(sql_err)?
      .collect::<Result<Vec<_>, _>>()
      .map_err(sql_err)?;
    let target = self.delivery_target(&id)?;
    Ok(Some(Routing { id, card, state: r.state, gone: r.session_gone, latest_seq: latest_seq as u64, targets, target }))
  }

  pub fn delivery_target(&self, id: &str) -> Result<Option<String>, String> {
    Ok(delivery_target(&self.conn, &self.canon(id)?))
  }

  fn create_card(&mut self, card: CardIn) -> Result<Created, String> {
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
      return Ok(Created { id: canon, fresh: false, changed: false, outcome, card, similar: Vec::new() });
    }
    let patches = std::mem::take(&mut card.patches);
    let target = self.resend_target(&card)?.filter(|(t, _)| t.state != "closed");
    if let Some((target, old)) = target {
      let tx = self.conn.transaction().map_err(sql_err)?;
      tx.execute("INSERT INTO aliases(alias, card_id) VALUES(?1, ?2)", params![id, target.id]).map_err(sql_err)?;
      if !is_live(&target.state) {
        tx.commit().map_err(sql_err)?;
        return Ok(Created { id: target.id, fresh: false, changed: false, outcome: OUTCOME_FINAL, card, similar: Vec::new() });
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
      return Ok(Created { id: target.id, fresh: false, changed: true, outcome: OUTCOME_UPDATED, card, similar: Vec::new() });
    }
    let similar = self.similar(&card, !patches.is_empty())?;
    if let Some(best) = similar.first().map(|f| f.card_id.clone()) {
      return Ok(Created { id: best, fresh: false, changed: false, outcome: OUTCOME_SIMILAR, card, similar });
    }
    let tx = self.conn.transaction().map_err(sql_err)?;
    if card.supersede && card.reply_via == "hook" {
      if let Some(session) = &card.source.session {
        tx.execute(
          "DELETE FROM push_targets WHERE card_id IN (SELECT id FROM cards WHERE state='open'
             AND json_extract(card,'$.reply_via')='hook'
             AND json_extract(card,'$.source.agent')=?1 AND json_extract(card,'$.source.session')=?2)",
          params![card.source.agent, session],
        )
        .map_err(sql_err)?;
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
    Ok(Created { id, fresh: true, changed: true, outcome: OUTCOME_CREATED, card, similar: Vec::new() })
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

  fn similar(&self, card: &CardIn, carries_patches: bool) -> Result<Vec<Found>, String> {
    if card.force_new != Some(false)
      || card.reply_via == "hook"
      || matches!(card.kind.as_str(), "review" | "release" | "check")
      || card.diff.is_some()
      || !card.diffs.is_empty()
      || carries_patches
    {
      return Ok(Vec::new());
    }
    let since = now_ms() - SIMILAR_WINDOW_MS;
    let mut stmt = self
      .conn
      .prepare(&format!(
        "SELECT {CARD_COLS} FROM cards WHERE state IN ('answered','dismissed','open','deferred') AND created_ms>=?1
           AND COALESCE(json_extract(card,'$.reply_via'),'')!='hook' ORDER BY created_ms DESC, rowid DESC LIMIT {SIMILAR_SCAN}"
      ))
      .map_err(sql_err)?;
    let rows = stmt.query_map([since], row).map_err(sql_err)?.collect::<Result<Vec<_>, _>>().map_err(sql_err)?;
    let mut hits = Vec::new();
    for r in rows {
      let old: CardIn = serde_json::from_str(&r.card).map_err(json_err)?;
      if old.reply_via == "hook" || old.diff.is_some() || !old.diffs.is_empty() || !self.patch_info(&r.id)?.is_empty() {
        continue;
      }
      if let Some(score) = similarity(card, &old) {
        let answer = parse_answer(&r)?;
        hits.push((score, found(&r, &old, answer.as_ref())));
      }
    }
    Ok(hits.into_iter().take(SIMILAR_MAX).map(|(_, f)| f).collect())
  }

  pub fn find(
    &self,
    task: Option<&str>,
    key_prefix: Option<&str>,
    query: Option<&str>,
    limit: usize,
  ) -> Result<Vec<Found>, String> {
    let wanted = task.map(|t| task_ids(Some(t), t)).filter(|ids| !ids.is_empty());
    let key_prefix = key_prefix.filter(|p| !p.is_empty());
    let tokens: Vec<String> = query.unwrap_or("").split_whitespace().map(str::to_lowercase).collect();
    let limit = limit.clamp(1, FIND_MAX);
    let mut stmt = self
      .conn
      .prepare(&format!(
        "SELECT {CARD_COLS} FROM cards WHERE state IN ('answered','dismissed') ORDER BY created_ms DESC, rowid DESC LIMIT {FIND_SCAN}"
      ))
      .map_err(sql_err)?;
    let mut rows = stmt.query([]).map_err(sql_err)?;
    let mut out = Vec::new();
    while let Some(raw) = rows.next().map_err(sql_err)? {
      let r = row(raw).map_err(sql_err)?;
      let c: CardIn = serde_json::from_str(&r.card).map_err(json_err)?;
      if c.reply_via == "hook" {
        continue;
      }
      if let Some(ids) = &wanted {
        if task_ids(c.links.task.as_deref(), &c.title).is_disjoint(ids) {
          continue;
        }
      }
      if let Some(p) = key_prefix {
        if !c.key.as_deref().is_some_and(|k| k.starts_with(p)) {
          continue;
        }
      }
      let answer = parse_answer(&r)?;
      if !tokens.is_empty() {
        let text = [
          Some(c.title.as_str()),
          c.question.as_deref(),
          c.context.as_deref(),
          answer.as_ref().and_then(|a| a.choice_label.as_deref()),
          answer.as_ref().and_then(|a| a.free_text.as_deref()),
          answer.as_ref().and_then(|a| a.note.as_deref()),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase();
        if !tokens.iter().all(|t| text.contains(t.as_str())) {
          continue;
        }
      }
      out.push(found(&r, &c, answer.as_ref()));
      if out.len() == limit {
        break;
      }
    }
    Ok(out)
  }

  fn raw(&self, id: &str) -> Result<Option<Row>, String> {
    self.conn.query_row(&format!("SELECT {CARD_COLS} FROM cards WHERE id=?1"), [id], row).optional().map_err(sql_err)
  }

  fn answered_ms(&self, r: &Row, seq: u64) -> Result<i64, String> {
    self
      .conn
      .query_row(
        "SELECT COALESCE((SELECT at_ms FROM events WHERE card_id=?1 AND seq=?2),
                         (SELECT updated_ms FROM cards WHERE id=?1),
                         (SELECT at_ms FROM deliveries WHERE card_id=?1 AND seq=?2),
                         ?3)",
        params![r.id, seq as i64, r.created_ms],
        |t| t.get(0),
      )
      .map_err(sql_err)
  }

  fn delivery_state(&self, id: &str, seq: u64) -> Result<Option<String>, String> {
    self
      .conn
      .query_row("SELECT state FROM deliveries WHERE card_id=?1 AND seq=?2", params![id, seq as i64], |t| t.get(0))
      .optional()
      .map_err(sql_err)
  }

  fn unpicked(&self, r: &Row, c: &CardIn, answer: Option<&Event>, now: i64) -> Result<bool, String> {
    let Some(a) = answer.filter(|a| !a.deferred && matches!(r.state.as_str(), "answered" | "dismissed")) else {
      return Ok(false);
    };
    if !expects_agent(c) || matches!(self.delivery_state(&r.id, a.seq)?.as_deref(), Some("delivered" | "sent")) {
      return Ok(false);
    }
    Ok(now - self.answered_ms(r, a.seq)? > UNPICKED_MS)
  }

  fn view(&self, r: Row, now: i64) -> Result<CardView, String> {
    let c: CardIn = serde_json::from_str(&r.card).map_err(json_err)?;
    let answer = r.answer.as_deref().map(serde_json::from_str::<Event>).transpose().map_err(json_err)?;
    let live = is_live(&r.state);
    let patches = self.patch_info(&r.id)?;
    let push = self
      .conn
      .query_row("SELECT EXISTS(SELECT 1 FROM push_targets WHERE card_id=?1)", [&r.id], |t| t.get::<_, bool>(0))
      .map_err(sql_err)?;
    let delivery = self
      .conn
      .query_row(
        "SELECT state, via, detail, at_ms FROM deliveries WHERE card_id=?1 ORDER BY seq DESC LIMIT 1",
        [&r.id],
        |t| Ok(DeliveryView { state: t.get(0)?, via: t.get(1)?, detail: t.get(2)?, at_ms: t.get(3)? }),
      )
      .optional()
      .map_err(sql_err)?;
    let mut stmt = self
      .conn
      .prepare("SELECT seq, author, text, file, line, side, created_ms FROM messages WHERE card_id=?1 ORDER BY seq")
      .map_err(sql_err)?;
    let messages = stmt
      .query_map([&r.id], |m| {
        Ok(MessageView {
          seq: m.get::<_, i64>(0)? as u64,
          author: m.get(1)?,
          text: m.get(2)?,
          file: m.get(3)?,
          line: m.get::<_, Option<i64>>(4)?.map(|l| l as u32),
          side: m.get(5)?,
          created_ms: m.get(6)?,
          delivered: false,
        })
      })
      .map_err(sql_err)?
      .collect::<Result<Vec<_>, _>>()
      .map_err(sql_err)?;
    let rev = self.revision(&r, &messages)?;
    let unpicked = self.unpicked(&r, &c, answer.as_ref(), now)?;
    Ok(CardView {
      id: r.id,
      created_ms: r.created_ms,
      session_gone: r.session_gone,
      closed_reason: r.closed_reason,
      can_reply: live && (c.reply_via == "mcp" || c.reply_via == "cli"),
      listening: false,
      push,
      delivery,
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
      unpicked,
      rev,
    })
  }

  fn revision(&self, r: &Row, messages: &[MessageView]) -> Result<String, String> {
    let mut h = Sha256::new();
    let mut put = |s: &str| {
      h.update((s.len() as u64).to_be_bytes());
      h.update(s.as_bytes());
    };
    put(&r.card);
    put(&r.state);
    put(if r.session_gone { "gone" } else { "live" });
    for m in messages {
      put(&m.author);
      put(&m.text);
    }
    put("patches");
    for p in self.patches(&r.id)? {
      put(&p.label);
      put(&p.text);
    }
    Ok(tmgr_phone::hex(&h.finalize()))
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
    self.get_at(id, now_ms())
  }

  pub fn get_at(&self, id: &str, now: i64) -> Result<Option<CardView>, String> {
    let Some(id) = self.resolve(id)? else { return Ok(None) };
    self.raw(&id)?.map(|r| self.view(r, now)).transpose()
  }

  pub fn list(&self) -> Result<Vec<CardView>, String> {
    self.list_at(now_ms())
  }

  pub fn list_at(&self, now: i64) -> Result<Vec<CardView>, String> {
    let mut stmt = self
      .conn
      .prepare(&format!(
        "SELECT {CARD_COLS} FROM cards WHERE state IN ('open','deferred') OR id IN
           (SELECT id FROM cards WHERE state NOT IN ('open','deferred') ORDER BY created_ms DESC LIMIT {LIST_FINISHED})
         ORDER BY created_ms DESC"
      ))
      .map_err(sql_err)?;
    let rows = stmt.query_map([], row).map_err(sql_err)?.collect::<Result<Vec<_>, _>>().map_err(sql_err)?;
    rows.into_iter().map(|r| self.view(r, now)).collect()
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

  pub fn pending_task_comments(&self) -> Result<Vec<PendingTaskComment>, String> {
    let mut stmt = self
      .conn
      .prepare(&format!(
        "SELECT {CARD_COLS} FROM cards c WHERE state IN ('answered','dismissed') AND answer IS NOT NULL
           AND NOT EXISTS(SELECT 1 FROM task_comments t WHERE t.card_id=c.id AND t.seq=json_extract(c.answer, '$.seq'))
         ORDER BY created_ms, rowid"
      ))
      .map_err(sql_err)?;
    let rows = stmt.query_map([], row).map_err(sql_err)?.collect::<Result<Vec<_>, _>>().map_err(sql_err)?;
    let mut out = Vec::new();
    for r in rows {
      let (Some(a), Ok(c)) = (parse_answer(&r)?, serde_json::from_str::<CardIn>(&r.card)) else { continue };
      let text = a.free_text.as_deref().is_some_and(|t| !t.trim().is_empty());
      if a.deferred || (a.dismissed && !text) {
        continue;
      }
      let Some((origin, task_id)) = cloud_task(&c) else { continue };
      out.push(PendingTaskComment { body: comment_body(&c, &a, &r.id), card_id: r.id, seq: a.seq, task_id, origin });
    }
    Ok(out)
  }

  pub fn mark_task_comment(&self, card_id: &str, seq: u64) -> Result<(), String> {
    self
      .conn
      .execute(
        "INSERT OR IGNORE INTO task_comments(card_id, seq, posted_ms) VALUES(?1, ?2, ?3)",
        params![card_id, seq as i64, now_ms()],
      )
      .map(drop)
      .map_err(sql_err)
  }

  pub fn pending_delivery(&self) -> Result<Vec<PendingDelivery>, String> {
    let mut stmt = self
      .conn
      .prepare(&format!("SELECT {CARD_COLS} FROM cards WHERE state IN ('answered','dismissed') AND answer IS NOT NULL"))
      .map_err(sql_err)?;
    let rows = stmt.query_map([], row).map_err(sql_err)?.collect::<Result<Vec<_>, _>>().map_err(sql_err)?;
    let mut out = Vec::new();
    for r in rows {
      let (Some(a), Ok(c)) = (parse_answer(&r)?, serde_json::from_str::<CardIn>(&r.card)) else { continue };
      let state = self.delivery_state(&r.id, a.seq)?;
      if a.deferred || !expects_agent(&c) || matches!(state.as_deref(), Some("delivered" | "sent")) {
        continue;
      }
      out.push(PendingDelivery {
        answered_ms: self.answered_ms(&r, a.seq)?,
        task: c.links.task.clone().or_else(|| title_key(&c.title)),
        delivery_state: state.unwrap_or_else(|| "none".into()),
        card_id: r.id,
        seq: a.seq,
        title: c.title,
      });
    }
    out.sort_by(|x, y| x.answered_ms.cmp(&y.answered_ms).then_with(|| x.card_id.cmp(&y.card_id)));
    Ok(out)
  }

  pub fn answer(&mut self, id: &str, a: AnswerIn) -> Result<(Event, CardIn), String> {
    let id = &self.canon(id)?;
    let (_, card) = self.live_card(id)?;
    let tx = self.conn.unchecked_transaction().map_err(sql_err)?;
    let done = self.write_answer(&tx, id, card, a, None)?;
    tx.commit().map_err(sql_err)?;
    Ok(done)
  }

  fn write_answer(
    &self,
    conn: &Connection,
    id: &str,
    card: CardIn,
    a: AnswerIn,
    via: Option<(&str, &str)>,
  ) -> Result<(Event, CardIn), String> {
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
      via: via.map(|(v, _)| v.to_owned()),
      via_fp: via.map(|(_, fp)| fp.to_owned()),
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
    conn
      .execute("INSERT INTO events(card_id, seq, json, at_ms) VALUES(?1, ?2, ?3, ?4)", params![id, seq as i64, json, now_ms()])
      .map_err(sql_err)?;
    conn.execute("UPDATE cards SET state=?2, answer=?3, session_gone=CASE WHEN ?2='deferred' THEN session_gone ELSE 0 END WHERE id=?1", params![id, state, json])
      .map_err(sql_err)?;
    Ok((event, card))
  }

  #[cfg(test)]
  pub fn stored_phone_ack(&self, answer_id: &str) -> Option<String> {
    self.conn.query_row("SELECT ack FROM phone_answers WHERE answer_id=?1", [answer_id], |x| x.get(0)).ok()
  }

  pub fn set_phone_ack_result(&self, answer_id: &str, result: AckResult) -> Result<(), String> {
    let ack = StoredAck { result: result.as_str().to_owned(), winner: None, label: None, routed: true };
    self
      .conn
      .execute(
        "UPDATE phone_answers SET ack=?2 WHERE answer_id=?1",
        params![answer_id, serde_json::to_string(&ack).map_err(json_err)?],
      )
      .map(drop)
      .map_err(sql_err)
  }

  pub fn phone_answer(&mut self, r: &PhoneReq, now: i64) -> Result<PhoneOutcome, String> {
    self.conn.execute_batch("BEGIN IMMEDIATE").map_err(sql_err)?;
    match self.phone_answer_in(r, now) {
      Ok(out) => {
        if let Err(e) = self.conn.execute_batch("COMMIT") {
          let _ = self.conn.execute_batch("ROLLBACK");
          return Err(sql_err(e));
        }
        Ok(out)
      }
      Err(e) => {
        let _ = self.conn.execute_batch("ROLLBACK");
        Err(e)
      }
    }
  }

  fn phone_answer_in(&mut self, r: &PhoneReq, now: i64) -> Result<PhoneOutcome, String> {
    self
      .conn
      .execute("DELETE FROM phone_answers WHERE received_ms < ?1", [now - PHONE_ANSWER_KEEP_MS])
      .map_err(sql_err)?;
    let seen: Option<String> = self
      .conn
      .query_row("SELECT ack FROM phone_answers WHERE answer_id=?1", [&r.answer_id], |x| x.get(0))
      .optional()
      .map_err(sql_err)?;
    let record = |this: &Self, out: PhoneOutcome| -> Result<PhoneOutcome, String> {
      let ack = serde_json::to_string(&StoredAck {
        result: out.result.as_str().to_owned(),
        winner: out.winner.clone(),
        label: out.label.clone(),
        routed: false,
      })
      .map_err(json_err)?;
      this
        .conn
        .execute(
          "INSERT INTO phone_answers(answer_id, phone_fp, card_id, ack, received_ms) VALUES(?1, ?2, ?3, ?4, ?5)",
          params![r.answer_id, r.phone_fp, r.card_id, ack, now],
        )
        .map_err(sql_err)?;
      Ok(out)
    };
    let reject = |this: &Self, result| record(this, PhoneOutcome::rejected(result, None, None));
    if tmgr_phone::check_issued_at(r.issued_at, now).is_err() {
      if seen.is_some() {
        return Ok(PhoneOutcome::rejected(AckResult::Expired, None, None));
      }
      return reject(self, AckResult::Expired);
    }
    if let Some(stored) = seen {
      let first: StoredAck = serde_json::from_str(&stored).map_err(json_err)?;
      let result = if first.routed && first.result == AckResult::Changed.as_str() { AckResult::Changed } else { AckResult::Duplicate };
      return Ok(PhoneOutcome::rejected(result, first.winner, first.label));
    }
    if self.resolve(&r.card_id)?.as_deref() != Some(r.card_id.as_str()) {
      return reject(self, AckResult::Closed);
    }
    let Some(row) = self.raw(&r.card_id)? else { return reject(self, AckResult::Closed) };
    if !is_live(&row.state) {
      let ended = row.answer.as_deref().and_then(|a| serde_json::from_str::<Event>(a).ok());
      let (winner, label) = match ended {
        Some(e) => (
          Some(e.via_fp.clone().filter(|_| e.via.as_deref() == Some("phone")).unwrap_or_else(|| "desktop".into())),
          Some(event_label(&e)),
        ),
        None => (None, row.closed_reason.clone()),
      };
      return record(self, PhoneOutcome::rejected(AckResult::Closed, winner, label));
    }
    let card: CardIn = serde_json::from_str(&row.card).map_err(json_err)?;
    let view = self.view(row, now_ms())?;
    let target = delivery_target(&self.conn, &r.card_id).unwrap_or_default();
    match phone::project(&view, &target) {
      Some(p) if p.digest == r.card_digest && target == r.session => {}
      _ => return reject(self, AckResult::Changed),
    }
    if !phone::eligible(&view) {
      return reject(self, AckResult::NotEligible);
    }
    let Some(a) = phone_answer_in_of(r, &card) else { return reject(self, AckResult::Invalid) };
    let fp = r.phone_fp.get(..8).unwrap_or(&r.phone_fp);
    let (event, card) = self.write_answer(&self.conn, &r.card_id, card, a, Some(("phone", fp)))?;
    let label = event_label(&event);
    let out =
      PhoneOutcome { result: AckResult::Applied, winner: None, label: Some(label), applied: Some((event, card)), target };
    record(self, out)
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
    let side = line.and(m.side).filter(|s| s == "old" || s == "new");
    let action = pick(m.action.as_deref().unwrap_or("reply"), &["reply", "fix"], "reply");
    let seq = self.next_seq(id)?;
    let event = Event {
      card_id: id.to_owned(),
      kind: "message".into(),
      seq,
      text: Some(text.clone()),
      thread: file.clone().zip(line).map(|(file, line)| ThreadRef { file, line, side: side.clone() }),
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
      "INSERT INTO messages(card_id, seq, author, text, file, line, side, created_ms) VALUES(?1, ?2, 'owner', ?3, ?4, ?5, ?6, ?7)",
      params![id, mseq, text, file, line.map(i64::from), side, now_ms()],
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
    let rows = self.thread_rows(id)?;
    let mut items: Vec<Item> = Vec::new();
    for l in lines.into_iter().take(MAX_LINE_COMMENTS) {
      let t = cap(&l.text, TEXT_MAX);
      if !t.is_empty() {
        let file = cap(&l.file, FIELD_MAX * 4);
        let side = resolve_side(&rows, &file, l.line, l.side.filter(|s| s == "old" || s == "new"));
        items.push((t, Some(file), Some(l.line), side));
      }
    }
    if let Some(t) = cap_opt(text, TEXT_MAX) {
      let thread = self.thread_for_text(id, &rows, &items)?;
      let (file, line, side) = match thread {
        Some((f, l, s)) => (Some(f), Some(l), s),
        None => (None, None, None),
      };
      items.insert(0, (t, file, line, side));
    }
    if items.is_empty() {
      return Err("text is required".into());
    }
    let first = self.next_message_seq(id)?;
    let tx = self.conn.transaction().map_err(sql_err)?;
    for (seq, (text, file, line, side)) in (first..).zip(items) {
      tx.execute(
        "INSERT INTO messages(card_id, seq, author, text, file, line, side, created_ms) VALUES(?1, ?2, 'agent', ?3, ?4, ?5, ?6, ?7)",
        params![id, seq, text, file, line.map(i64::from), side, now_ms()],
      )
      .map_err(sql_err)?;
    }
    tx.commit().map_err(sql_err)?;
    Ok(id.clone())
  }

  fn thread_rows(&self, id: &str) -> Result<Vec<ThreadRow>, String> {
    let mut stmt = self
      .conn
      .prepare("SELECT author, file, line, side FROM messages WHERE card_id=?1 ORDER BY seq")
      .map_err(sql_err)?;
    let rows = stmt
      .query_map([id], |m| {
        let owner = m.get::<_, String>(0)? == "owner";
        let file = m.get::<_, Option<String>>(1)?;
        let line = m.get::<_, Option<i64>>(2)?;
        let side = m.get::<_, Option<String>>(3)?;
        Ok(ThreadRow { owner, thread: file.zip(line).map(|(f, l)| (f, l as u32, side)) })
      })
      .map_err(sql_err)?
      .collect::<Result<Vec<_>, _>>()
      .map_err(sql_err)?;
    Ok(rows)
  }

  fn thread_for_text(
    &self,
    id: &str,
    rows: &[ThreadRow],
    line_replies: &[Item],
  ) -> Result<Option<Thread>, String> {
    if !is_live(&self.raw(id)?.ok_or_else(|| "unknown card".to_owned())?.state) {
      return Ok(None);
    }
    let mut chat_last_owner = false;
    let mut threads: Vec<(Thread, bool)> = Vec::new();
    let mut note = |key: Option<Thread>, owner: bool| match key {
      None => chat_last_owner = owner,
      Some(key) => match threads.iter_mut().find(|(k, _)| *k == key) {
        Some(t) => t.1 = owner,
        None => threads.push((key, owner)),
      },
    };
    for row in rows {
      note(row.thread.clone(), row.owner);
    }
    for (_, file, line, side) in line_replies {
      note(file.clone().zip(*line).map(|(f, l)| (f, l, side.clone())), false);
    }
    if chat_last_owner {
      return Ok(None);
    }
    let label = |(f, l, side): &Thread| match side.as_deref() {
      Some("old") => format!("{f}:{l} (deleted)"),
      _ => format!("{f}:{l}"),
    };
    let pending: Vec<String> = threads.iter().filter(|(_, p)| *p).map(|(k, _)| label(k)).collect();
    match pending.len() {
      0 => Ok(None),
      1 => Ok(threads.into_iter().find(|(_, p)| *p).map(|(k, _)| k)),
      n => {
        let shown = pending.iter().take(THREADS_LISTED).cloned().collect::<Vec<_>>().join(", ");
        let more = if n > THREADS_LISTED { format!(" and {} more", n - THREADS_LISTED) } else { String::new() };
        Err(format!("{n} line threads are waiting: {shown}{more}; answer them with line_replies"))
      }
    }
  }

  pub fn close_session(&mut self, agent: &str, session: &str) -> Result<u64, String> {
    self
      .conn
      .execute(
        "DELETE FROM push_targets WHERE card_id IN (SELECT id FROM cards WHERE state='open'
           AND json_extract(card,'$.reply_via')='hook'
           AND json_extract(card,'$.source.agent')=?1 AND json_extract(card,'$.source.session')=?2)",
        params![cap(agent, 128), cap(session, FIELD_MAX)],
      )
      .map_err(sql_err)?;
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

  pub fn message_event_seqs(&self, id: &str) -> Vec<u64> {
    let query = || -> rusqlite::Result<Vec<u64>> {
      self
        .conn
        .prepare("SELECT seq FROM events WHERE card_id=?1 AND json_extract(json,'$.type')='message' ORDER BY seq")?
        .query_map([id], |r| Ok(r.get::<_, i64>(0)? as u64))?
        .collect()
    };
    query().unwrap_or_default()
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
      self.conn.execute("DELETE FROM push_targets WHERE card_id=?1", [id]).map_err(sql_err)?;
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
    let m = MessageIn { text: "why?".into(), file: Some("a.rs".into()), line: Some(4), side: None, action: Some("fix".into()) };
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
    let l = LineComment { file: "a.rs".into(), line: 3, side: None, text: "fixed".into() };
    s.agent_message(&id, Some("done".into()), vec![l]).unwrap();
    let v = s.get(&id).unwrap().unwrap();
    assert_eq!(v.messages.iter().map(|m| m.author.as_str()).collect::<Vec<_>>(), ["agent", "agent"]);
    assert_eq!(v.messages[1].line, Some(3));
    assert!(s.events(&id).unwrap().is_empty());
    assert!(s.agent_message(&id, None, vec![]).is_err());
    assert!(s.agent_message("c_00000000000000000000000000000000", Some("x".into()), vec![]).is_err());
  }

  fn owner_at(s: &mut Store, id: &str, text: &str, thread: Option<(&str, u32)>) {
    let m = MessageIn {
      text: text.into(),
      file: thread.map(|(f, _)| f.into()),
      line: thread.map(|(_, l)| l),
      ..Default::default()
    };
    s.owner_message(id, m).unwrap();
  }

  fn placed(s: &Store, id: &str) -> Vec<(String, String, Option<String>, Option<u32>)> {
    s.get(id).unwrap().unwrap().messages.into_iter().map(|m| (m.author, m.text, m.file, m.line)).collect()
  }

  fn lr(file: &str, line: u32, text: &str) -> LineComment {
    LineComment { file: file.into(), line, side: None, text: text.into() }
  }

  #[test]
  fn text_reply_with_pending_chat_goes_to_chat_even_with_a_pending_thread() {
    let mut s = mem();
    let id = s.create_id(card("Review"));
    owner_at(&mut s, &id, "in line", Some(("a.rs", 3)));
    owner_at(&mut s, &id, "in chat", None);
    s.agent_message(&id, Some("ok".into()), vec![]).unwrap();
    let last = placed(&s, &id).pop().unwrap();
    assert_eq!((last.1.as_str(), last.2, last.3), ("ok", None, None));
  }

  #[test]
  fn text_reply_goes_to_the_only_pending_thread() {
    let mut s = mem();
    let id = s.create_id(card("Review"));
    owner_at(&mut s, &id, "hello", None);
    s.agent_message(&id, Some("chat answer".into()), vec![]).unwrap();
    owner_at(&mut s, &id, "why?", Some(("a.rs", 3)));
    s.agent_message(&id, Some("because".into()), vec![]).unwrap();
    let last = placed(&s, &id).pop().unwrap();
    assert_eq!((last.0.as_str(), last.1.as_str(), last.2.as_deref(), last.3), ("agent", "because", Some("a.rs"), Some(3)));
  }

  #[test]
  fn text_reply_with_two_pending_threads_is_refused_and_stores_nothing() {
    let mut s = mem();
    let id = s.create_id(card("Review"));
    owner_at(&mut s, &id, "one", Some(("a.rs", 3)));
    owner_at(&mut s, &id, "two", Some(("b.rs", 9)));
    let before = placed(&s, &id).len();
    let err = s.agent_message(&id, Some("hm".into()), vec![]).unwrap_err();
    assert_eq!(err, "2 line threads are waiting: a.rs:3, b.rs:9; answer them with line_replies");
    let err = s.agent_message(&id, Some("hm".into()), vec![lr("c.rs", 1, "x")]).unwrap_err();
    assert!(err.starts_with("2 line threads are waiting"), "{err}");
    assert_eq!(placed(&s, &id).len(), before);
  }

  #[test]
  fn refusal_lists_at_most_three_threads() {
    let mut s = mem();
    let id = s.create_id(card("Review"));
    for n in 1..=5 {
      owner_at(&mut s, &id, "q", Some(("a.rs", n)));
    }
    let err = s.agent_message(&id, Some("hm".into()), vec![]).unwrap_err();
    assert_eq!(err, "5 line threads are waiting: a.rs:1, a.rs:2, a.rs:3 and 2 more; answer them with line_replies");
  }

  #[test]
  fn text_reply_without_pending_threads_goes_to_chat() {
    let mut s = mem();
    let id = s.create_id(card("Review"));
    s.agent_message(&id, Some("hi".into()), vec![]).unwrap();
    owner_at(&mut s, &id, "q", Some(("a.rs", 3)));
    s.agent_message(&id, None, vec![lr("a.rs", 3, "a")]).unwrap();
    s.agent_message(&id, Some("again".into()), vec![]).unwrap();
    let last = placed(&s, &id).pop().unwrap();
    assert_eq!((last.1.as_str(), last.2, last.3), ("again", None, None));
  }

  #[test]
  fn text_and_line_replies_in_one_call_answer_the_only_pending_thread_then_chat() {
    let mut s = mem();
    let id = s.create_id(card("Review"));
    owner_at(&mut s, &id, "q", Some(("a.rs", 3)));
    s.agent_message(&id, Some("summary".into()), vec![lr("a.rs", 3, "fixed")]).unwrap();
    let all = placed(&s, &id);
    assert_eq!((all[1].1.as_str(), all[1].2.as_deref()), ("summary", None));
    assert_eq!((all[2].1.as_str(), all[2].2.as_deref(), all[2].3), ("fixed", Some("a.rs"), Some(3)));
  }

  #[test]
  fn line_replies_can_resolve_two_threads_so_text_goes_to_the_third() {
    let mut s = mem();
    let id = s.create_id(card("Review"));
    owner_at(&mut s, &id, "q", Some(("a.rs", 3)));
    owner_at(&mut s, &id, "q", Some(("b.rs", 9)));
    s.agent_message(&id, Some("rest".into()), vec![lr("a.rs", 3, "x")]).unwrap();
    let all = placed(&s, &id);
    assert_eq!((all[2].1.as_str(), all[2].2.as_deref(), all[2].3), ("rest", Some("b.rs"), Some(9)));
  }

  #[test]
  fn answered_cards_keep_routing_text_to_chat() {
    let mut s = mem();
    let id = s.create_id(card("Review"));
    owner_at(&mut s, &id, "q", Some(("a.rs", 3)));
    s.answer(&id, pick_a()).unwrap();
    s.agent_message(&id, Some("done".into()), vec![]).unwrap();
    let last = placed(&s, &id).pop().unwrap();
    assert_eq!((last.1.as_str(), last.2, last.3), ("done", None, None));
  }

  #[test]
  fn qualified_multi_repo_paths_route_and_match() {
    let mut s = mem();
    let id = s.create_id(card("Review"));
    owner_at(&mut s, &id, "q", Some(("web:src/a.vue", 7)));
    s.agent_message(&id, Some("because".into()), vec![]).unwrap();
    let last = placed(&s, &id).pop().unwrap();
    assert_eq!((last.2.as_deref(), last.3), (Some("web:src/a.vue"), Some(7)));
    s.agent_message(&id, Some("chat".into()), vec![]).unwrap();
    assert_eq!(placed(&s, &id).pop().unwrap().2, None);
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
  fn foreign_schemes_on_task_pr_and_fact_links_are_dropped_without_an_error() {
    let mut s = mem();
    let fact = |link: &str| Fact { state: "ok".into(), text: "t".into(), link: Some(link.into()) };
    let c = CardIn {
      links: Links { task: Some("java\tscript:alert(1)".into()), pr: Some("JAVASCRIPT:alert(1)".into()), ..Default::default() },
      facts: vec![fact("data:text/html,x"), fact("https://example.com/a"), fact("TM-9")],
      ..card("x")
    };
    assert!(Store::check(&c).is_ok());
    let id = s.create_id(c);
    let v = s.get(&id).unwrap().unwrap();
    assert_eq!((v.links.task, v.links.pr), (None, None));
    assert_eq!(v.facts.iter().map(|f| f.link.as_deref()).collect::<Vec<_>>(), [None, Some("https://example.com/a"), Some("TM-9")]);
  }

  #[test]
  fn bare_references_on_task_and_pr_are_kept() {
    let mut s = mem();
    let links = Links { task: Some("TM-9".into()), pr: Some("PR-77".into()), ..Default::default() };
    let id = s.create_id(CardIn { links, ..card("x") });
    let v = s.get(&id).unwrap().unwrap();
    assert_eq!((v.links.task.as_deref(), v.links.pr.as_deref()), (Some("TM-9"), Some("PR-77")));
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

  fn owner_side(s: &mut Store, id: &str, text: &str, line: u32, side: &str) -> Event {
    let m = MessageIn {
      text: text.into(),
      file: Some("a.rs".into()),
      line: Some(line),
      side: Some(side.into()),
      ..Default::default()
    };
    s.owner_message(id, m).unwrap()
  }

  fn sides(s: &Store, id: &str) -> Vec<(String, String, Option<String>)> {
    s.get(id).unwrap().unwrap().messages.into_iter().map(|m| (m.author, m.text, m.side)).collect()
  }

  #[test]
  fn owner_sides_are_stored_and_reach_the_agent_event() {
    let mut s = mem();
    let id = s.create_id(card("Review"));
    let e = owner_side(&mut s, &id, "gone?", 10, "old");
    assert_eq!(e.thread.unwrap().side.as_deref(), Some("old"));
    owner_side(&mut s, &id, "added?", 10, "new");
    let bogus = MessageIn { text: "x".into(), file: Some("a.rs".into()), line: Some(10), side: Some("up".into()), ..Default::default() };
    s.owner_message(&id, bogus).unwrap();
    let got: Vec<_> = sides(&s, &id).into_iter().map(|m| m.2).collect();
    assert_eq!(got, [Some("old".into()), Some("new".into()), None]);
  }

  #[test]
  fn sideless_reply_joins_the_existing_thread_on_that_line() {
    let mut s = mem();
    let id = s.create_id(card("Review"));
    owner_side(&mut s, &id, "gone?", 10, "old");
    s.agent_message(&id, None, vec![lr("a.rs", 10, "yes")]).unwrap();
    assert_eq!(sides(&s, &id).pop().unwrap(), ("agent".into(), "yes".into(), Some("old".into())));
  }

  #[test]
  fn sideless_reply_prefers_the_thread_waiting_for_the_agent() {
    let mut s = mem();
    let id = s.create_id(card("Review"));
    owner_side(&mut s, &id, "gone?", 10, "old");
    s.agent_message(&id, None, vec![lr("a.rs", 10, "yes")]).unwrap();
    owner_side(&mut s, &id, "added?", 10, "new");
    s.agent_message(&id, None, vec![lr("a.rs", 10, "also")]).unwrap();
    assert_eq!(sides(&s, &id).pop().unwrap().2.as_deref(), Some("new"));
  }

  #[test]
  fn sideless_reply_on_a_fresh_line_stays_sideless() {
    let mut s = mem();
    let id = s.create_id(card("Review"));
    s.agent_message(&id, None, vec![lr("a.rs", 4, "note")]).unwrap();
    assert_eq!(sides(&s, &id).pop().unwrap().2, None);
    let l = LineComment { side: Some("old".into()), ..lr("a.rs", 5, "was") };
    s.agent_message(&id, None, vec![l]).unwrap();
    assert_eq!(sides(&s, &id).pop().unwrap().2.as_deref(), Some("old"));
  }

  #[test]
  fn text_reply_lands_in_the_pending_sided_thread() {
    let mut s = mem();
    let id = s.create_id(card("Review"));
    owner_side(&mut s, &id, "gone?", 10, "old");
    s.agent_message(&id, Some("yes".into()), vec![]).unwrap();
    let last = s.get(&id).unwrap().unwrap().messages.pop().unwrap();
    assert_eq!((last.line, last.side.as_deref()), (Some(10), Some("old")));
  }

  #[test]
  fn two_sides_of_one_line_count_as_two_waiting_threads() {
    let mut s = mem();
    let id = s.create_id(card("Review"));
    owner_side(&mut s, &id, "a", 10, "old");
    owner_side(&mut s, &id, "b", 10, "new");
    let err = s.agent_message(&id, Some("hm".into()), vec![]).unwrap_err();
    assert_eq!(err, "2 line threads are waiting: a.rs:10 (deleted), a.rs:10; answer them with line_replies");
  }

  #[test]
  fn legacy_rows_without_a_side_keep_their_anchor_and_replies_stay_sideless() {
    let dir = tempfile::Builder::new().prefix("ds").tempdir_in("/tmp").unwrap();
    let path = dir.path().join("decisions.db");
    let id = "c_11111111111111111111111111111111";
    {
      let c = Connection::open(&path).unwrap();
      c.execute_batch(
        "CREATE TABLE cards(id TEXT PRIMARY KEY, created_ms INTEGER NOT NULL, card TEXT NOT NULL, state TEXT NOT NULL,
           session_gone INTEGER NOT NULL DEFAULT 0, answer TEXT, closed_reason TEXT);
         CREATE TABLE messages(card_id TEXT NOT NULL, seq INTEGER NOT NULL, author TEXT NOT NULL, text TEXT NOT NULL,
           file TEXT, line INTEGER, created_ms INTEGER NOT NULL, PRIMARY KEY(card_id, seq));
         INSERT INTO cards(id, created_ms, card, state) VALUES('c_11111111111111111111111111111111', 5,
           '{\"title\":\"Legacy\"}', 'open');
         INSERT INTO messages(card_id, seq, author, text, file, line, created_ms)
           VALUES('c_11111111111111111111111111111111', 1, 'owner', 'old row', 'a.rs', 10, 6);",
      )
      .unwrap();
    }
    let mut s = Store::open(&path).unwrap();
    assert_eq!(sides(&s, id), [("owner".to_owned(), "old row".to_owned(), None)]);
    s.agent_message(id, None, vec![lr("a.rs", 10, "reply")]).unwrap();
    assert_eq!(sides(&s, id).pop().unwrap(), ("agent".into(), "reply".into(), None));
    owner_side(&mut s, id, "new thread", 10, "new");
    assert_eq!(sides(&s, id).pop().unwrap().2.as_deref(), Some("new"));
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

  const TASK_211: &str = "https://tmgr.dev/ws/tasks/211";
  const TASK_212: &str = "https://tmgr.dev/ws/tasks/212";
  const SHIP_Q: &str = "Should we ship the 2.4 release to production today?";
  const DAY_MS: i64 = 86_400_000;

  fn asked(title: &str, question: &str, task: Option<&str>) -> CardIn {
    CardIn {
      question: Some(question.into()),
      links: Links { task: task.map(Into::into), ..Default::default() },
      force_new: Some(false),
      ..card(title)
    }
  }

  fn finished(s: &mut Store, c: CardIn) -> String {
    let id = s.create_id(c);
    s.answer(&id, pick_a()).unwrap();
    id
  }

  fn backdate(s: &Store, id: &str, ms: i64) {
    s.conn.execute("UPDATE cards SET created_ms=?1 WHERE id=?2", params![ms, id]).unwrap();
  }

  fn ids(found: Vec<Found>) -> Vec<String> {
    found.into_iter().map(|f| f.card_id).collect()
  }

  #[test]
  fn a_similar_answered_card_is_returned_instead_of_a_new_one() {
    let mut s = mem();
    let old = finished(&mut s, asked("Ship 2.4?", SHIP_Q, Some(TASK_211)));
    let again = s.create(CardIn { key: Some("new-key".into()), ..asked("Ship 2.4 now?", SHIP_Q, Some(TASK_211)) }).unwrap();
    assert_eq!((again.id.as_str(), again.fresh, again.changed, again.outcome), (old.as_str(), false, false, OUTCOME_SIMILAR));
    assert_eq!(again.similar.len(), 1);
    assert_eq!(again.similar[0].card_id, old);
    assert_eq!(again.similar[0].choice_label.as_deref(), Some("Ship"));
    assert_eq!(s.list().unwrap().len(), 1);
  }

  #[test]
  fn a_different_version_number_or_letter_is_not_similar() {
    let mut s = mem();
    finished(&mut s, asked("Ship 2.4?", SHIP_Q, Some(TASK_211)));
    let c = s.create(asked("Ship 2.5?", "Should we ship the 2.5 release to production today?", Some(TASK_211))).unwrap();
    assert!(c.fresh);
    finished(&mut s, asked("Approve X", "Do you approve the migration plan for the billing tables?", Some(TASK_212)));
    let c = s.create(asked("Approve Y", "Do you approve the migration plan for the billing tables?", Some(TASK_212))).unwrap();
    assert!(c.fresh);
  }

  #[test]
  fn a_half_overlap_is_not_similar() {
    let mut s = mem();
    finished(&mut s, asked("Flag", "Keep the feature flag for checkout?", Some(TASK_211)));
    assert!(s.create(asked("Flag", "Remove the feature flag for checkout?", Some(TASK_211))).unwrap().fresh);
  }

  #[test]
  fn the_newest_match_comes_first() {
    let mut s = mem();
    let first = finished(&mut s, asked("Ship 2.4?", SHIP_Q, Some(TASK_211)));
    backdate(&s, &first, now_ms() - 2 * DAY_MS);
    let second = s.create_id(CardIn { force_new: Some(true), ..asked("Ship 2.4?", SHIP_Q, Some(TASK_211)) });
    s.answer(&second, pick_a()).unwrap();
    let again = s.create(asked("Ship 2.4?", SHIP_Q, Some(TASK_211))).unwrap();
    assert_eq!(ids(again.similar), [second, first]);
  }

  #[test]
  fn hook_cards_do_not_crowd_out_the_scan() {
    let mut s = mem();
    let old = finished(&mut s, asked("Ship 2.4?", SHIP_Q, Some(TASK_211)));
    backdate(&s, &old, now_ms() - DAY_MS);
    for i in 0..60 {
      s.create_id(hook(&format!("hook {i}"), &format!("s{i}")));
    }
    assert_eq!(s.create(asked("Ship 2.4?", SHIP_Q, Some(TASK_211))).unwrap().outcome, OUTCOME_SIMILAR);
  }

  #[test]
  fn leading_non_task_prefixes_are_not_keys() {
    assert_eq!(title_key("UTF-8 handling"), None);
    assert_eq!(title_key("TM-211: x").as_deref(), Some("tm-211"));
  }

  #[test]
  fn another_task_is_never_similar() {
    let mut s = mem();
    finished(&mut s, asked("Ship 2.4?", SHIP_Q, Some(TASK_211)));
    assert!(s.create(asked("Ship 2.4?", SHIP_Q, Some(TASK_212))).unwrap().fresh);
    assert_eq!(s.list().unwrap().len(), 2);
  }

  #[test]
  fn different_questions_for_the_same_task_are_not_similar() {
    let mut s = mem();
    finished(&mut s, asked("TM-211: Ship 2.4?", SHIP_Q, None));
    let other = "Which database should hold the audit log for the billing service?";
    assert!(s.create(asked("TM-211: Ship 2.4?", other, None)).unwrap().fresh);
  }

  #[test]
  fn cards_without_a_task_are_never_deduped() {
    let mut s = mem();
    finished(&mut s, asked("Ship 2.4?", SHIP_Q, None));
    assert!(s.create(asked("Ship 2.4?", SHIP_Q, None)).unwrap().fresh);
    assert_eq!(s.list().unwrap().len(), 2);
  }

  #[test]
  fn a_task_key_at_the_start_of_the_title_is_the_task_without_a_link() {
    let mut s = mem();
    let old = finished(&mut s, asked("TM-211: Ship 2.4?", SHIP_Q, None));
    let again = s.create(asked("TM-211: Ship 2.4 now?", SHIP_Q, None)).unwrap();
    assert_eq!((again.id.as_str(), again.outcome), (old.as_str(), OUTCOME_SIMILAR));
  }

  #[test]
  fn a_task_key_in_the_middle_of_the_title_is_not_a_task() {
    let mut s = mem();
    finished(&mut s, asked("Ship 2.4 for TM-211?", SHIP_Q, None));
    assert!(s.create(asked("Ship 2.4 for TM-211 now?", SHIP_Q, None)).unwrap().fresh);
  }

  #[test]
  fn force_new_true_and_old_clients_create_a_card() {
    let mut s = mem();
    finished(&mut s, asked("Ship 2.4?", SHIP_Q, Some(TASK_211)));
    let forced = CardIn { force_new: Some(true), ..asked("Ship 2.4?", SHIP_Q, Some(TASK_211)) };
    assert!(s.create(forced).unwrap().fresh);
    let old_client = CardIn { force_new: None, ..asked("Ship 2.4?", SHIP_Q, Some(TASK_211)) };
    assert!(s.create(old_client).unwrap().fresh);
    assert_eq!(s.list().unwrap().len(), 3);
  }

  #[test]
  fn dismissed_matches_count_and_are_flagged_without_a_choice() {
    let mut s = mem();
    let old = s.create_id(asked("Ship 2.4?", SHIP_Q, Some(TASK_211)));
    s.answer(&old, AnswerIn { choice: Some("a".into()), dismissed: true, ..Default::default() }).unwrap();
    let again = s.create(asked("Ship 2.4 now?", SHIP_Q, Some(TASK_211))).unwrap();
    assert_eq!(again.outcome, OUTCOME_SIMILAR);
    assert_eq!(again.similar[0].state, "dismissed");
    assert!(again.similar[0].dismissed);
    assert_eq!(again.similar[0].choice_label, None);
  }

  #[test]
  fn open_matches_count_and_report_their_state() {
    let mut s = mem();
    let old = s.create_id(asked("Ship 2.4?", SHIP_Q, Some(TASK_211)));
    let again = s.create(asked("Ship 2.4 now?", SHIP_Q, Some(TASK_211))).unwrap();
    assert_eq!((again.id.as_str(), again.outcome), (old.as_str(), OUTCOME_SIMILAR));
    assert_eq!(again.similar[0].state, "open");
  }

  #[test]
  fn review_release_and_check_cards_are_never_matched() {
    let mut s = mem();
    finished(&mut s, asked("Ship 2.4?", SHIP_Q, Some(TASK_211)));
    for kind in ["review", "release", "check"] {
      let c = CardIn { kind: kind.into(), ..asked("Ship 2.4?", SHIP_Q, Some(TASK_211)) };
      assert!(s.create(c).unwrap().fresh, "{kind}");
    }
  }

  #[test]
  fn cards_older_than_seven_days_are_not_matched() {
    let mut s = mem();
    let old = finished(&mut s, asked("Ship 2.4?", SHIP_Q, Some(TASK_211)));
    backdate(&s, &old, now_ms() - 6 * DAY_MS);
    assert_eq!(s.create(asked("Ship 2.4 now?", SHIP_Q, Some(TASK_211))).unwrap().outcome, OUTCOME_SIMILAR);
    backdate(&s, &old, now_ms() - 8 * DAY_MS);
    assert!(s.create(asked("Ship 2.4 now?", SHIP_Q, Some(TASK_211))).unwrap().fresh);
  }

  #[test]
  fn the_key_resend_path_runs_before_the_similarity_check() {
    let mut s = mem();
    let old = s.create_id(CardIn { key: Some("k".into()), ..asked("Ship 2.4?", SHIP_Q, Some(TASK_211)) });
    let again = s.create(CardIn { key: Some("k".into()), ..asked("Ship 2.4 now?", SHIP_Q, Some(TASK_211)) }).unwrap();
    assert_eq!((again.id.as_str(), again.outcome), (old.as_str(), OUTCOME_UPDATED));
    let forced = CardIn { force_new: Some(true), key: Some("k".into()), ..asked("Ship 2.4 later?", SHIP_Q, Some(TASK_211)) };
    assert_eq!(s.create(forced).unwrap().id, old);
    assert_eq!(s.list().unwrap().len(), 1);
  }

  #[test]
  fn find_filters_by_task_key_query_and_answer_text_newest_first() {
    let mut s = mem();
    let a = finished(&mut s, asked("TM-211: Ship 2.4?", SHIP_Q, None));
    let b = s.create_id(CardIn { key: Some("ops/2".into()), ..asked("Roll back the cache", "Which path?", Some(TASK_211)) });
    s.answer(&b, AnswerIn { free_text: Some("use the fallback path".into()), ..Default::default() }).unwrap();
    let c = s.create_id(asked("Unrelated ask", "Anything?", None));
    s.answer(&c, AnswerIn { note: Some("database migration rollback".into()), ..Default::default() }).unwrap();
    let open = s.create_id(asked("Open one", "Still open?", Some(TASK_211)));
    let hook_card = CardIn { reply_via: "hook".into(), kind: "stopped".into(), ..asked("Stopped", "Done?", Some(TASK_211)) };
    let hooked = s.create_id(hook_card);
    s.answer(&hooked, pick_a()).unwrap();
    let g = finished(&mut s, asked("TM-211: Ship again", "Which release train carries the 2.5 hotfixes?", Some(TASK_211)));

    assert_eq!(ids(s.find(Some(TASK_211), None, None, 10).unwrap()), [g.clone(), b.clone()]);
    assert_eq!(ids(s.find(Some("TM-211"), None, None, 10).unwrap()), [g.clone(), a.clone()]);
    assert_eq!(ids(s.find(None, None, Some("ship"), 10).unwrap()), [g.clone(), a.clone()]);
    assert_eq!(ids(s.find(None, None, Some("rollback MIGRATION"), 10).unwrap()), std::slice::from_ref(&c));
    assert_eq!(ids(s.find(None, None, Some("fallback"), 10).unwrap()), std::slice::from_ref(&b));
    assert_eq!(ids(s.find(None, Some("ops/"), None, 10).unwrap()), std::slice::from_ref(&b));
    assert_eq!(ids(s.find(None, None, None, 1).unwrap()), std::slice::from_ref(&g));
    let all = ids(s.find(None, None, None, 10).unwrap());
    assert!(!all.contains(&open) && !all.contains(&hooked));
    let found_b = s.find(Some(TASK_211), None, None, 10).unwrap().pop().unwrap();
    assert_eq!(found_b.question.as_deref(), Some("Which path?"));
    assert_eq!(found_b.free_text.as_deref(), Some("use the fallback path"));
    assert_eq!(found_b.key.as_deref(), Some("ops/2"));
    assert_eq!(found_b.task.as_deref(), Some(TASK_211));
  }

  #[test]
  fn find_caps_the_limit_at_fifty() {
    let mut s = mem();
    for n in 0..55 {
      finished(&mut s, asked(&format!("Ask {n}"), "Q?", None));
    }
    assert_eq!(s.find(None, None, None, 1000).unwrap().len(), 50);
    assert_eq!(s.find(None, None, None, 10).unwrap().len(), 10);
  }

  fn inbox_for(endpoint: &str) -> Inbox {
    Inbox { kind: "claude".into(), endpoint: endpoint.into(), token: Some(Secret::new("tok")), session: Some("sx".into()) }
  }

  fn targets(s: &Store, id: &str) -> Vec<String> {
    s.routing(id).unwrap().unwrap().targets.into_iter().map(|t| t.endpoint).collect()
  }

  #[test]
  fn a_create_with_an_inbox_registers_the_target_under_the_canonical_id() {
    let mut s = mem();
    let first = s.create_with(keyed("Alpha", "k1"), Some(inbox_for("/tmp/cc-socks/1.sock"))).unwrap();
    assert_eq!(first.outcome, OUTCOME_CREATED);
    let again = s.create_with(CardIn { id: Some("c_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into()), ..keyed("Alpha 2", "k1") }, Some(inbox_for("/tmp/cc-socks/2.sock"))).unwrap();
    assert_eq!(again.outcome, OUTCOME_UPDATED);
    assert_eq!(again.id, first.id);
    assert_eq!(targets(&s, &first.id), ["/tmp/cc-socks/1.sock", "/tmp/cc-socks/2.sock"]);
    let alias = s.create_with(replacing("Alpha 3", &first.id), Some(inbox_for("/tmp/cc-socks/3.sock"))).unwrap();
    assert_eq!(alias.id, first.id);
    assert_eq!(targets(&s, &first.id).len(), 3);
    assert!(s.get(&first.id).unwrap().unwrap().push);
  }

  #[test]
  fn similar_and_final_outcomes_register_no_target() {
    let mut s = mem();
    let done = finished(&mut s, asked("Ship v2 release", "Which one?", Some("TM-1")));
    let sim = s.create_with(asked("Ship v2 release", "Which one?", Some("TM-1")), Some(inbox_for("/tmp/cc-socks/9.sock"))).unwrap();
    assert_eq!(sim.outcome, OUTCOME_SIMILAR);
    assert!(targets(&s, &done).is_empty());
    let fin = s.create_with(replacing("again", &done), Some(inbox_for("/tmp/cc-socks/9.sock"))).unwrap();
    assert_eq!(fin.outcome, OUTCOME_FINAL);
    assert!(targets(&s, &done).is_empty());
  }

  #[test]
  fn closing_marking_gone_and_superseding_delete_targets() {
    let mut s = mem();
    let a = s.create_with(hook("one", "s1"), Some(inbox_for("/tmp/cc-socks/1.sock"))).unwrap().id;
    s.close_session("claude", "s1").unwrap();
    assert!(targets(&s, &a).is_empty());
    let b = s.create_with(hook("two", "s2"), Some(inbox_for("/tmp/cc-socks/2.sock"))).unwrap().id;
    let c = s.create_with(hook("three", "s2"), Some(inbox_for("/tmp/cc-socks/3.sock"))).unwrap().id;
    assert!(targets(&s, &b).is_empty());
    assert_eq!(targets(&s, &c).len(), 1);
    s.mark_gone(std::slice::from_ref(&c)).unwrap();
    assert!(targets(&s, &c).is_empty());
    assert!(!s.get(&c).unwrap().unwrap().push);
  }

  #[test]
  fn the_latest_delivery_row_wins_and_a_later_state_overwrites() {
    let mut s = mem();
    let id = s.create(card("Q")).unwrap().id;
    assert_eq!(s.record_delivery(&id, 1, "sent", "claude", "").unwrap(), Recorded::New);
    assert_eq!(s.record_delivery(&id, 1, "sent", "claude", "").unwrap(), Recorded::Same);
    assert_eq!(s.record_delivery(&id, 1, "delivered", "agent", "").unwrap(), Recorded::Changed);
    s.record_delivery(&id, 2, "queued", "runner", "w").unwrap();
    let d = s.get(&id).unwrap().unwrap().delivery.unwrap();
    assert_eq!((d.state.as_str(), d.via.as_str()), ("queued", "runner"));
    let json = serde_json::to_value(s.get(&id).unwrap().unwrap()).unwrap();
    assert_eq!(json["delivery"]["detail"], "w");
    assert_eq!(json["push"], false);
  }

  const CLOUD_TASK: &str = "https://tmgr.dev/ws/tasks/211";

  fn linked(title: &str, task: &str) -> CardIn {
    CardIn { links: Links { task: Some(task.into()), ..Default::default() }, ..card(title) }
  }

  fn delivery_state(s: &Store, id: &str, seq: u64) -> (String, String, String) {
    s.conn
      .query_row("SELECT state, via, detail FROM deliveries WHERE card_id=?1 AND seq=?2", params![id, seq as i64], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
      })
      .unwrap()
  }

  #[test]
  fn a_delivered_or_sent_row_is_never_downgraded() {
    let mut s = mem();
    let id = s.create(card("Q")).unwrap().id;
    s.record_delivery(&id, 1, "delivered", "agent", "").unwrap();
    assert_eq!(s.record_delivery(&id, 1, "undelivered", "runner", "session closed").unwrap(), Recorded::Same);
    assert_eq!(delivery_state(&s, &id, 1), ("delivered".into(), "agent".into(), "".into()));
    s.record_delivery(&id, 2, "sent", "claude", "").unwrap();
    assert_eq!(s.record_delivery(&id, 2, "queued", "runner", "w").unwrap(), Recorded::Same);
    assert_eq!(delivery_state(&s, &id, 2).0, "sent");
    assert_eq!(s.record_delivery(&id, 2, "delivered", "agent", "").unwrap(), Recorded::Changed);
    assert_eq!(s.record_delivery(&id, 2, "sent", "claude", "").unwrap(), Recorded::Same);
    assert_eq!(delivery_state(&s, &id, 2).0, "delivered");
    s.record_delivery(&id, 3, "undelivered", "runner", "x").unwrap();
    assert_eq!(s.record_delivery(&id, 3, "queued", "runner", "w").unwrap(), Recorded::Changed);
  }

  #[test]
  fn answers_store_their_time_in_the_event_row() {
    let mut s = mem();
    let id = s.create(card("Q")).unwrap().id;
    let before = now_ms();
    s.answer(&id, pick_a()).unwrap();
    let at: Option<i64> =
      s.conn.query_row("SELECT at_ms FROM events WHERE card_id=?1 AND seq=1", [&id], |r| r.get(0)).unwrap();
    assert!(at.unwrap() >= before);
  }

  #[test]
  fn a_final_answer_on_a_cloud_task_is_pending_once() {
    let mut s = mem();
    let id = s.create(linked("Ship?", CLOUD_TASK)).unwrap().id;
    assert!(s.pending_task_comments().unwrap().is_empty());
    s.answer(&id, pick_a()).unwrap();
    let _ = s.answer(&id, pick_a());
    let pending = s.pending_task_comments().unwrap();
    assert_eq!(pending.len(), 1);
    let p = &pending[0];
    assert_eq!((p.card_id.as_str(), p.seq, p.task_id, p.origin.as_str()), (id.as_str(), 1, 211, "https://tmgr.dev"));
    assert!(p.body.contains(&format!("[decision {id}#1]")));
    assert!(p.body.contains("Ship?") && p.body.contains("Ship") && p.body.contains(&id));
    s.mark_task_comment(&id, 1).unwrap();
    s.mark_task_comment(&id, 1).unwrap();
    assert!(s.pending_task_comments().unwrap().is_empty());
    let _ = s.answer(&id, pick_a());
    assert!(s.pending_task_comments().unwrap().is_empty());
  }

  #[test]
  fn body_carries_question_free_text_and_note() {
    let mut s = mem();
    let c = CardIn { question: Some("Which way?".into()), ..linked("Route", CLOUD_TASK) };
    let id = s.create(c).unwrap().id;
    s.answer(&id, AnswerIn { free_text: Some("Take the long one".into()), note: Some("trust me".into()), ..Default::default() })
      .unwrap();
    let body = s.pending_task_comments().unwrap().remove(0).body;
    for part in ["Route", "Which way?", "Take the long one", "trust me", &id] {
      assert!(body.contains(part), "{part} missing in {body}");
    }
  }

  #[test]
  fn task_links_are_found_in_items_and_unusable_ones_are_skipped() {
    let mut s = mem();
    let items = CardIn {
      links: Links {
        items: vec![LinkItem { kind: "task".into(), url: "http://localhost:8080/team/tasks/9".into(), label: None }],
        ..Default::default()
      },
      ..card("Items")
    };
    let a = s.create(items).unwrap().id;
    let bare = s.create(linked("Bare", "TM-5")).unwrap().id;
    let none = s.create(card("None")).unwrap().id;
    let deferred = s.create(linked("Later", CLOUD_TASK)).unwrap().id;
    let bare_dismissed = s.create(linked("Dismissed", CLOUD_TASK)).unwrap().id;
    let text_dismissed = s.create(linked("Dismissed with text", "https://tmgr.dev/ws/tasks/12")).unwrap().id;
    for id in [&a, &bare, &none] {
      s.answer(id, pick_a()).unwrap();
    }
    s.answer(&deferred, AnswerIn { deferred: true, ..Default::default() }).unwrap();
    s.answer(&bare_dismissed, AnswerIn { dismissed: true, ..Default::default() }).unwrap();
    s.answer(&text_dismissed, AnswerIn { dismissed: true, free_text: Some("not needed".into()), ..Default::default() })
      .unwrap();
    let mut got: Vec<_> = s.pending_task_comments().unwrap().into_iter().map(|p| (p.card_id, p.task_id, p.origin)).collect();
    got.sort();
    let mut want = vec![
      (a, 9, "http://localhost:8080".to_owned()),
      (text_dismissed, 12, "https://tmgr.dev".to_owned()),
    ];
    want.sort();
    assert_eq!(got, want);
  }

  #[test]
  fn history_picked_up_before_the_table_existed_is_backfilled_and_the_rest_is_pending() {
    let dir = tempfile::Builder::new().prefix("ds").tempdir_in("/tmp").unwrap();
    let path = dir.path().join("decisions.db");
    let (picked, unpicked, none) = {
      let mut s = Store::open(&path).unwrap();
      let picked = s.create(linked("Picked", CLOUD_TASK)).unwrap().id;
      let unpicked = s.create(linked("Unpicked", CLOUD_TASK)).unwrap().id;
      let none = s.create(linked("Old", CLOUD_TASK)).unwrap().id;
      for id in [&picked, &unpicked, &none] {
        s.answer(id, pick_a()).unwrap();
      }
      s.record_delivery(&picked, 1, "delivered", "agent", "").unwrap();
      s.record_delivery(&unpicked, 1, "undelivered", "runner", "session closed").unwrap();
      s.conn.execute("DROP TABLE task_comments", []).unwrap();
      (picked, unpicked, none)
    };
    let s = Store::open(&path).unwrap();
    let mut got: Vec<_> = s.pending_task_comments().unwrap().into_iter().map(|p| p.card_id).collect();
    got.sort();
    let mut want = vec![unpicked, none];
    want.sort();
    assert_eq!(got, want);
    let kept: i64 = s.conn.query_row("SELECT COUNT(*) FROM task_comments WHERE card_id=?1", [&picked], |r| r.get(0)).unwrap();
    assert_eq!(kept, 1);
    drop(s);
    let reopened = Store::open(&path).unwrap();
    assert_eq!(reopened.pending_task_comments().unwrap().len(), 2);
  }

  fn expecting(title: &str, via: &str, kind: &str) -> CardIn {
    CardIn { reply_via: via.into(), kind: kind.into(), ..card(title) }
  }

  const HALF_HOUR: i64 = 30 * 60 * 1000;

  fn answered_at(s: &mut Store, c: CardIn, at: i64) -> String {
    let id = s.create(c).unwrap().id;
    s.answer(&id, pick_a()).unwrap();
    s.conn.execute("UPDATE events SET at_ms=?2 WHERE card_id=?1", params![id, at]).unwrap();
    id
  }

  fn unpicked_at(s: &Store, id: &str, now: i64) -> bool {
    s.get_at(id, now).unwrap().unwrap().unpicked
  }

  #[test]
  fn unpicked_flips_after_thirty_minutes_without_a_pickup() {
    let mut s = mem();
    let id = answered_at(&mut s, expecting("Q", "mcp", "decision"), 1_000);
    assert!(!unpicked_at(&s, &id, 1_000 + HALF_HOUR));
    assert!(unpicked_at(&s, &id, 1_001 + HALF_HOUR));
    s.record_delivery(&id, 1, "undelivered", "runner", "session closed").unwrap();
    assert!(unpicked_at(&s, &id, 1_001 + HALF_HOUR));
    s.record_delivery(&id, 1, "delivered", "agent", "").unwrap();
    assert!(!unpicked_at(&s, &id, 1_001 + HALF_HOUR));
    let sent = answered_at(&mut s, expecting("Q2", "cli", "decision"), 1_000);
    s.record_delivery(&sent, 1, "sent", "claude", "").unwrap();
    assert!(!unpicked_at(&s, &sent, 10 * HALF_HOUR));
  }

  #[test]
  fn unpicked_only_counts_cards_that_expect_an_agent() {
    let mut s = mem();
    let late = 10 * HALF_HOUR;
    let cli = answered_at(&mut s, expecting("A", "cli", "decision"), 1);
    let stopped = answered_at(&mut s, expecting("B", "hook", "stopped"), 1);
    let permission = answered_at(&mut s, expecting("C", "hook", "permission"), 1);
    assert!(unpicked_at(&s, &cli, late) && unpicked_at(&s, &stopped, late));
    assert!(!unpicked_at(&s, &permission, late));
    let open = s.create(expecting("D", "mcp", "decision")).unwrap().id;
    assert!(!unpicked_at(&s, &open, late));
    let dismissed = s.create(expecting("E", "mcp", "decision")).unwrap().id;
    s.answer(&dismissed, AnswerIn { dismissed: true, ..Default::default() }).unwrap();
    assert!(unpicked_at(&s, &dismissed, now_ms() + late));
    let deferred = s.create(expecting("F", "mcp", "decision")).unwrap().id;
    s.answer(&deferred, AnswerIn { deferred: true, ..Default::default() }).unwrap();
    assert!(!unpicked_at(&s, &deferred, now_ms() + late));
  }

  #[test]
  fn answer_time_falls_back_to_the_card_update_then_the_delivery() {
    let mut s = mem();
    let id = answered_at(&mut s, expecting("Q", "mcp", "decision"), 0);
    s.conn.execute("UPDATE events SET at_ms=NULL WHERE card_id=?1", [&id]).unwrap();
    s.conn.execute("UPDATE cards SET updated_ms=5000 WHERE id=?1", [&id]).unwrap();
    assert!(!unpicked_at(&s, &id, 5_000 + HALF_HOUR));
    assert!(unpicked_at(&s, &id, 5_001 + HALF_HOUR));
    s.conn.execute("UPDATE cards SET updated_ms=NULL WHERE id=?1", [&id]).unwrap();
    s.record_delivery(&id, 1, "undelivered", "runner", "x").unwrap();
    let at: i64 = s.conn.query_row("SELECT at_ms FROM deliveries WHERE card_id=?1", [&id], |r| r.get(0)).unwrap();
    assert!(!unpicked_at(&s, &id, at + HALF_HOUR));
    assert!(unpicked_at(&s, &id, at + HALF_HOUR + 1));
  }

  #[test]
  fn pending_delivery_lists_unpicked_final_cards_oldest_answer_first() {
    let mut s = mem();
    let late = answered_at(&mut s, linked_via("Late", "mcp"), 2_000);
    let early = answered_at(&mut s, linked_via("Early", "cli"), 1_000);
    let picked = answered_at(&mut s, linked_via("Picked", "mcp"), 500);
    s.record_delivery(&picked, 1, "delivered", "agent", "").unwrap();
    let hooked = answered_at(&mut s, expecting("Hook", "hook", "permission"), 100);
    let _ = hooked;
    let got = s.pending_delivery().unwrap();
    assert_eq!(got.iter().map(|d| d.card_id.as_str()).collect::<Vec<_>>(), vec![early.as_str(), late.as_str()]);
    assert_eq!(got[0].answered_ms, 1_000);
    assert_eq!(got[0].delivery_state, "none");
    assert_eq!(got[0].title, "Early");
    assert_eq!(got[0].task.as_deref(), Some(CLOUD_TASK));
    s.record_delivery(&early, 1, "undelivered", "runner", "session closed").unwrap();
    assert_eq!(s.pending_delivery().unwrap()[0].delivery_state, "undelivered");
  }

  fn linked_via(title: &str, via: &str) -> CardIn {
    CardIn { reply_via: via.into(), ..linked(title, CLOUD_TASK) }
  }
}
