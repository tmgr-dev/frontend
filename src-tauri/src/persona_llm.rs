use std::collections::{BTreeMap, HashMap};
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use url::{Host, Url};

use crate::local_access::tokens::{
  migrate_secret, validate_user_id, LlmMigrateOut, MigrateStatus, SecretStore,
};

const SERVICE: &str = "dev.tmgr.persona-llm";
const ACCOUNT: &str = "api-key";
const CONFIG_FILE: &str = "llm_config.json";
const LOCAL_OWNER_ID: i64 = -1;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct LlmConfigFile {
  base_url: String,
  model: String,
  /// The pre-per-slot origin; read-only, new code never writes it.
  #[serde(default)]
  key_origin: Option<String>,
  /// The origin each account's saved Keychain key was issued for, keyed by user id.
  #[serde(default)]
  key_origins: HashMap<String, String>,
}

const UNKNOWN_ORIGIN: &str = "";

fn slot_origin(file: &LlmConfigFile, user_id: i64) -> Option<&str> {
  file.key_origins.get(&user_id.to_string()).or(file.key_origin.as_ref()).map(String::as_str)
}

/// A saved key may only be sent to the origin it was issued for.
fn key_may_be_sent(slot_origin: Option<&str>, base_url: &str) -> bool {
  match (slot_origin, normalize_origin(base_url)) {
    (Some(issued), Ok(current)) => issued == current,
    _ => false,
  }
}

/// Pure on purpose: a refusal only reports, it never touches the saved key.
fn check_key_origin(slot_origin: Option<&str>, base_url: &str) -> Result<(), String> {
  if key_may_be_sent(slot_origin, base_url) {
    return Ok(());
  }
  let host = Url::parse(base_url.trim()).ok().and_then(|u| u.host_str().map(str::to_string));
  let target = host.unwrap_or_else(|| "this server".to_string());
  Err(if slot_origin.is_some_and(|o| o != UNKNOWN_ORIGIN) {
    format!("Your saved API key was issued for a different server. Enter it again in Settings to use it with {target}.")
  } else {
    format!("Your saved API key is not tied to a server. Enter it again in Settings to use it with {target}.")
  })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SlotAccess {
  Allowed,
  NotReady,
  OtherAccount,
}

impl SlotAccess {
  fn save_refusal(self) -> String {
    match self {
      SlotAccess::OtherAccount => "These AI provider settings belong to another account.",
      _ => NOT_READY_MESSAGE,
    }
    .into()
  }

  fn chat_refusal(self) -> String {
    match self {
      SlotAccess::OtherAccount => "This AI provider key belongs to another account.",
      _ => NOT_READY_MESSAGE,
    }
    .into()
  }

  fn state(self) -> &'static str {
    match self {
      SlotAccess::Allowed => "ok",
      SlotAccess::NotReady => "not_ready",
      SlotAccess::OtherAccount => "other_account",
    }
  }
}

const NOT_READY_MESSAGE: &str = "Your account is not available to the desktop app yet. Reload the app or sign in again, then retry.";

fn slot_access(identity: Option<i64>, user_id: i64) -> SlotAccess {
  match (identity, user_id) {
    (Some(id), u) if u == id => SlotAccess::Allowed,
    (None, LOCAL_OWNER_ID) => SlotAccess::Allowed,
    (Some(_), u) if u > 0 => SlotAccess::OtherAccount,
    _ => SlotAccess::NotReady,
  }
}

fn user_slot_allowed(identity: Option<i64>, user_id: i64) -> bool {
  slot_access(identity, user_id) == SlotAccess::Allowed
}

fn next_key_origins(
  mut origins: HashMap<String, String>,
  user_id: i64,
  key_after: bool,
  new_origin: Option<String>,
) -> HashMap<String, String> {
  match new_origin {
    Some(origin) if key_after => origins.insert(user_id.to_string(), origin),
    None if key_after => origins.insert(user_id.to_string(), UNKNOWN_ORIGIN.to_string()),
    _ => origins.remove(&user_id.to_string()),
  };
  origins
}

fn move_local_key_origin(origins: &mut HashMap<String, String>, status: &MigrateStatus, to: i64) -> bool {
  if !matches!(status, MigrateStatus::Moved | MigrateStatus::Already) {
    return false;
  }
  let Some(origin) = origins.remove(&LOCAL_OWNER_ID.to_string()) else {
    return matches!(status, MigrateStatus::Moved) && origins.remove(&to.to_string()).is_some();
  };
  if matches!(status, MigrateStatus::Moved) {
    origins.insert(to.to_string(), origin);
  } else {
    origins.entry(to.to_string()).or_insert(origin);
  }
  true
}

/// Scheme + lowercased host + port (default ports folded in), so a path-only edit to `base_url` is not an origin change.
fn normalize_origin(input: &str) -> Result<String, String> {
  let trimmed = input.trim();
  if trimmed.is_empty() {
    return Err("empty URL".into());
  }
  let url = Url::parse(trimmed).map_err(|e| e.to_string())?;
  let scheme = url.scheme().to_ascii_lowercase();
  let host = url.host_str().ok_or("URL has no host")?.to_ascii_lowercase();
  let default_port = match scheme.as_str() {
    "https" => 443,
    "http" => 80,
    _ => 0,
  };
  let port = url.port().unwrap_or(default_port);
  Ok(format!("{scheme}://{host}:{port}"))
}

/// Whether the saved key should still exist after this `llm_config_set` call, before anything is written.
fn key_will_exist(clear_api_key: bool, new_key_present: bool, origin_changed: bool, had_key: bool) -> bool {
  if clear_api_key {
    false
  } else if new_key_present {
    true
  } else if origin_changed {
    false
  } else {
    had_key
  }
}

#[derive(Debug, PartialEq, Eq)]
struct SavePlan {
  key_after: bool,
  stale_key: bool,
  drop_old_key: bool,
}

/// `stale_key`: a keyless save that did not move the URL itself, so the slot's key (issued for another origin) is kept as it was.
fn plan_save(
  slot_origin: Option<&str>,
  previous_url: &str,
  base_url: &str,
  new_key_present: bool,
  clear_api_key: bool,
  had_key: bool,
) -> SavePlan {
  let new_origin = normalize_origin(base_url).ok();
  let origin_changed = new_origin.as_deref() != slot_origin;
  let url_moved = normalize_origin(previous_url).ok() != new_origin;
  let stale_key = origin_changed && !url_moved && !new_key_present && !clear_api_key;
  SavePlan {
    key_after: if stale_key { had_key } else { key_will_exist(clear_api_key, new_key_present, origin_changed, had_key) },
    stale_key,
    drop_old_key: clear_api_key || (origin_changed && !stale_key),
  }
}

fn origins_after(
  plan: &SavePlan,
  previous: HashMap<String, String>,
  user_id: i64,
  new_origin: Option<String>,
) -> HashMap<String, String> {
  if plan.stale_key && plan.key_after {
    previous
  } else {
    next_key_origins(previous, user_id, plan.key_after, new_origin)
  }
}

/// The old key is deleted before the config moves to a new server, so a failed save can never leave it paired with that server.
fn apply_save<S: SecretStore>(
  store: &S,
  account: &str,
  plan: &SavePlan,
  new_key: Option<&str>,
  write_config: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
  if plan.drop_old_key {
    store.delete(account)?;
  }
  write_config()?;
  if let Some(key) = new_key {
    store.set(account, key)?;
  }
  Ok(())
}

/// A key must never be saved for use over plaintext HTTP to a host outside the local network.
fn refuses_plaintext_key(base_url: &str, key_will_exist: bool) -> Result<(), String> {
  if !key_will_exist {
    return Ok(());
  }
  let is_remote = matches!(classify_url(base_url), Ok(UrlKind::Remote));
  let is_http = Url::parse(base_url.trim()).is_ok_and(|u| u.scheme() == "http");
  if is_remote && is_http {
    return Err("Refusing to save an API key for a remote endpoint over plain HTTP; use HTTPS.".into());
  }
  Ok(())
}

#[derive(Clone, Debug, Serialize)]
pub struct LlmConfigOut {
  base_url: String,
  model: String,
  has_api_key: bool,
  is_local: bool,
  slot_state: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UrlKind {
  Local,
  Remote,
}

/// Warns, never blocks: a local LLM may legitimately sit on another box on the LAN.
pub fn classify_url(input: &str) -> Result<UrlKind, String> {
  let trimmed = input.trim();
  if trimmed.is_empty() {
    return Err("empty URL".into());
  }
  let url = Url::parse(trimmed).map_err(|e| e.to_string())?;
  let host = url.host().ok_or("URL has no host")?;
  let local = match host {
    Host::Domain(domain) => {
      domain.eq_ignore_ascii_case("localhost") || domain.to_ascii_lowercase().ends_with(".local")
    }
    Host::Ipv4(ip) => is_local_v4(ip),
    Host::Ipv6(ip) => is_local_v6(ip),
  };
  Ok(if local { UrlKind::Local } else { UrlKind::Remote })
}

fn is_local_v4(ip: Ipv4Addr) -> bool {
  ip.is_loopback() || ip.is_private() || ip.is_link_local()
}

fn is_local_v6(ip: Ipv6Addr) -> bool {
  if ip.is_loopback() {
    return true;
  }
  if let Some(v4) = ip.to_ipv4_mapped() {
    return is_local_v4(v4);
  }
  let seg0 = ip.segments()[0];
  (seg0 & 0xfe00) == 0xfc00 /* fc00::/7 unique local */ || (seg0 & 0xffc0) == 0xfe80 /* fe80::/10 link local */
}

/// Buffers raw bytes so a multi-byte UTF-8 character split across chunks isn't decoded alone.
#[derive(Default)]
pub struct SseBuffer {
  pending: Vec<u8>,
  after_cr: bool,
  overflowed: bool,
}

impl SseBuffer {
  pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
    for &byte in chunk {
      match byte {
        b'\r' => {
          self.pending.push(b'\n');
          self.after_cr = true;
        }
        b'\n' if self.after_cr => self.after_cr = false,
        _ => {
          self.pending.push(byte);
          self.after_cr = false;
        }
      }
    }
    let mut events = Vec::new();
    while let Some(pos) = self.pending.windows(2).position(|w| w == b"\n\n") {
      let raw: Vec<u8> = self.pending.drain(..pos + 2).collect();
      events.extend(event_data(&raw));
    }
    if self.pending.len() > JSON_BODY_CAP {
      self.pending = Vec::new();
      self.overflowed = true;
    }
    events
  }

  pub fn overflowed(&self) -> bool {
    self.overflowed
  }

  pub fn finish(&mut self) -> Option<String> {
    self.after_cr = false;
    event_data(&std::mem::take(&mut self.pending))
  }
}

fn event_data(raw: &[u8]) -> Option<String> {
  let text = String::from_utf8_lossy(raw);
  let data: Vec<&str> = text.lines().filter_map(|line| line.strip_prefix("data:").map(str::trim_start)).collect();
  if data.is_empty() {
    None
  } else {
    Some(data.join("\n"))
  }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ChatEvent {
  Delta { text: String },
  ToolCall { index: u32, id: Option<String>, name: Option<String>, arguments: String },
  Done,
  Error { message: String },
}

/// Empty for a chunk with nothing the agent loop cares about (e.g. a bare role delta); content comes before tool calls.
pub fn parse_chat_events(data: &str, api_key: Option<&str>) -> Vec<ChatEvent> {
  let trimmed = data.trim();
  if trimmed == "[DONE]" {
    return vec![ChatEvent::Done];
  }
  let Ok(value) = serde_json::from_str::<Value>(trimmed) else {
    return Vec::new();
  };
  let error = present_error(&value).map(|e| error_message(e, api_key));
  let finish_error = finish_reason(&value).and_then(finish_error_message);
  let Some(delta) = value.get("choices").and_then(|c| c.get(0)).and_then(|c| c.get("delta")) else {
    return error.or(finish_error).map(|message| ChatEvent::Error { message }).into_iter().collect();
  };
  let mut events = Vec::new();
  if let Some(content) = delta.get("content").and_then(Value::as_str) {
    if !content.is_empty() {
      events.push(ChatEvent::Delta { text: content.to_string() });
    }
  }
  for call in delta.get("tool_calls").and_then(Value::as_array).into_iter().flatten() {
    let index = call.get("index").and_then(Value::as_u64).unwrap_or(0) as u32;
    let id = call.get("id").and_then(Value::as_str).map(str::to_string);
    let function = call.get("function");
    let name = function.and_then(|f| f.get("name")).and_then(Value::as_str).map(str::to_string);
    let arguments = function
      .and_then(|f| f.get("arguments"))
      .and_then(Value::as_str)
      .unwrap_or("")
      .to_string();
    events.push(ChatEvent::ToolCall { index, id, name, arguments });
  }
  if let Some(message) = error.or(finish_error) {
    events.push(ChatEvent::Error { message });
  }
  events
}

pub fn finish_reason(value: &Value) -> Option<&str> {
  value
    .get("choices")
    .and_then(|c| c.get(0))
    .and_then(|c| c.get("finish_reason"))
    .and_then(Value::as_str)
    .filter(|r| !r.is_empty())
}

pub fn stream_finish_reason(data: &str) -> Option<String> {
  let value = serde_json::from_str::<Value>(data.trim()).ok()?;
  finish_reason(&value).map(str::to_string)
}

fn finish_error_message(reason: &str) -> Option<String> {
  match reason.to_ascii_lowercase().as_str() {
    "length" | "max_tokens" | "model_length" => Some("The answer was cut off: the model hit its length limit".into()),
    "content_filter" => Some("The answer was blocked by the server's content filter".into()),
    "error" => Some("The LLM server ended the answer with an error".into()),
    _ => None,
  }
}

const NO_USABLE_CONTENT: &str = "The LLM server returned no usable content";
const ERROR_MESSAGE_CHARS: usize = 200;

fn truncate_chars(text: &str, max: usize) -> String {
  if text.chars().count() <= max {
    return text.to_string();
  }
  let mut cut: String = text.chars().take(max).collect();
  cut.push('…');
  cut
}

fn server_message(error: &Value) -> Option<&str> {
  error
    .as_str()
    .or_else(|| error.get("message").and_then(Value::as_str))
    .map(str::trim)
    .filter(|m| !m.is_empty())
}

fn present_error(value: &Value) -> Option<&Value> {
  value.get("error").filter(|e| match e {
    Value::Null | Value::Bool(false) => false,
    Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
    Value::Array(items) => !items.is_empty(),
    Value::String(text) => !text.trim().is_empty(),
    Value::Object(fields) => !fields.is_empty(),
    _ => true,
  })
}

const REDACTED: &str = "[redacted]";
/// Shorter values are placeholders ("ollama", "none"); cutting them out of normal text would garble it (D-002).
const MIN_REDACTED_KEY_CHARS: usize = 8;

/// The one place a saved key is found in text: trimmed, ASCII case-insensitive, raw or percent-encoded.
#[derive(Default)]
struct KeyRedactor {
  variants: Vec<String>,
  short: bool,
}

impl KeyRedactor {
  fn new(api_key: Option<&str>) -> Self {
    let Some(key) = api_key.map(str::trim).filter(|k| !k.is_empty()) else {
      return Self::default();
    };
    let mut variants = vec![key.to_ascii_lowercase()];
    let encoded = url::form_urlencoded::byte_serialize(key.as_bytes()).collect::<String>().to_ascii_lowercase();
    if !variants.contains(&encoded) {
      variants.push(encoded);
    }
    Self { variants, short: key.chars().count() < MIN_REDACTED_KEY_CHARS }
  }

  fn contains(&self, text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    self.variants.iter().any(|v| lower.contains(v.as_str()))
  }

  /// ASCII lowercasing keeps byte offsets, so a match in the lowered copy is a valid range in the original.
  fn replace(&self, text: &str) -> String {
    let mut text = text.to_string();
    for variant in &self.variants {
      let lower = text.to_ascii_lowercase();
      let mut out = String::with_capacity(text.len());
      let mut last = 0;
      for (start, _) in lower.match_indices(variant.as_str()) {
        out.push_str(&text[last..start]);
        out.push_str(REDACTED);
        last = start + variant.len();
      }
      out.push_str(&text[last..]);
      text = out;
    }
    text
  }

  /// `None`: the text shows a short key that can't be cut out cleanly, so the caller shows its generic message.
  fn error_text(&self, text: &str) -> Option<String> {
    if self.short {
      return (!self.contains(text)).then(|| text.to_string());
    }
    Some(self.replace(text))
  }

  fn model_text(&self, text: &str) -> String {
    if self.short {
      text.to_string()
    } else {
      self.replace(text)
    }
  }

  /// Chars a stream must keep back so a key split across chunks is still seen whole.
  fn hold(&self) -> usize {
    if self.short {
      return 0;
    }
    self.variants.iter().map(|v| v.chars().count()).max().unwrap_or(0).saturating_sub(1)
  }

  /// Redacts `carry` and returns all of it but the last `hold()` chars, which stay in `carry`.
  fn release(&self, carry: &mut String) -> String {
    let redacted = self.replace(carry);
    let split = redacted.char_indices().rev().nth(self.hold().saturating_sub(1)).map_or(0, |(i, _)| i);
    *carry = redacted[split..].to_string();
    redacted[..split].to_string()
  }
}

/// Model text (deltas, tool-call arguments) redacted across chunk borders; a terminal event flushes what was held.
struct StreamRedactor {
  keys: KeyRedactor,
  text: String,
  args: BTreeMap<u32, (Option<String>, String)>,
}

impl StreamRedactor {
  fn new(api_key: Option<&str>) -> Self {
    Self { keys: KeyRedactor::new(api_key), text: String::new(), args: BTreeMap::new() }
  }

  fn push(&mut self, event: ChatEvent) -> Vec<ChatEvent> {
    if self.keys.hold() == 0 {
      return vec![event];
    }
    match event {
      ChatEvent::Delta { text } => {
        self.text.push_str(&text);
        let text = self.keys.release(&mut self.text);
        if text.is_empty() {
          Vec::new()
        } else {
          vec![ChatEvent::Delta { text }]
        }
      }
      ChatEvent::ToolCall { index, id, name, arguments } => {
        let id = id.filter(|id| !id.is_empty());
        let mut out = Vec::new();
        let first = !self.args.contains_key(&index);
        let (last_id, carry) = self.args.entry(index).or_default();
        // Mirrors llmClient.ts: only a different id after a known one starts a new call; a first id keeps the carry.
        if id.is_some() && last_id.is_some() && *last_id != id && !carry.is_empty() {
          // An id-less event appends to the call already open at this index, so the old tail lands there.
          let tail = self.keys.replace(&std::mem::take(carry));
          out.push(ChatEvent::ToolCall { index, id: None, name: None, arguments: tail });
        }
        if id.is_some() {
          *last_id = id.clone();
        }
        carry.push_str(&arguments);
        let arguments = self.keys.release(carry);
        // The first event of an index always goes out, so TS opens its buffers in the order the server sent them.
        if first || id.is_some() || name.is_some() || !arguments.is_empty() {
          out.push(ChatEvent::ToolCall { index, id, name, arguments });
        }
        out
      }
      terminal => {
        let mut out = self.finish();
        out.push(terminal);
        out
      }
    }
  }

  fn finish(&mut self) -> Vec<ChatEvent> {
    let mut out = Vec::new();
    if !self.text.is_empty() {
      out.push(ChatEvent::Delta { text: self.keys.replace(&std::mem::take(&mut self.text)) });
    }
    for (&index, (_, carry)) in self.args.iter_mut().filter(|(_, (_, carry))| !carry.is_empty()) {
      out.push(ChatEvent::ToolCall { index, id: None, name: None, arguments: self.keys.replace(&std::mem::take(carry)) });
    }
    out
  }
}

fn clean_server_message(message: &str, api_key: Option<&str>) -> Option<String> {
  let message = KeyRedactor::new(api_key).error_text(message)?;
  if message.trim_start().starts_with('<') {
    return None;
  }
  Some(truncate_chars(&message, ERROR_MESSAGE_CHARS))
}

fn error_message(error: &Value, api_key: Option<&str>) -> String {
  server_message(error)
    .and_then(|m| clean_server_message(m, api_key))
    .unwrap_or_else(|| "The LLM server returned an error".to_string())
}

pub fn parse_chat_json_reply(body: &str, api_key: Option<&str>) -> Vec<ChatEvent> {
  let no_content = || vec![ChatEvent::Error { message: NO_USABLE_CONTENT.into() }];
  let Ok(value) = serde_json::from_str::<Value>(body.trim()) else {
    return no_content();
  };
  if let Some(error) = present_error(&value) {
    return vec![ChatEvent::Error { message: error_message(error, api_key) }];
  }
  let Some(message) = value.get("choices").and_then(|c| c.get(0)).and_then(|c| c.get("message")) else {
    return no_content();
  };
  let keys = KeyRedactor::new(api_key);
  let mut events = Vec::new();
  if let Some(content) = message.get("content").and_then(Value::as_str).filter(|c| !c.is_empty()) {
    events.push(ChatEvent::Delta { text: keys.model_text(content) });
  }
  for (index, call) in message.get("tool_calls").and_then(Value::as_array).into_iter().flatten().enumerate() {
    let function = call.get("function");
    events.push(ChatEvent::ToolCall {
      index: index as u32,
      id: call.get("id").and_then(Value::as_str).map(str::to_string),
      name: function.and_then(|f| f.get("name")).and_then(Value::as_str).map(str::to_string),
      arguments: keys.model_text(function.and_then(|f| f.get("arguments")).and_then(Value::as_str).unwrap_or("")),
    });
  }
  if let Some(message) = finish_reason(&value).and_then(finish_error_message) {
    events.push(ChatEvent::Error { message });
    return events;
  }
  if events.is_empty() {
    return no_content();
  }
  events.push(ChatEvent::Done);
  events
}

pub fn sanitize_error_body(status: &str, body: &str, api_key: Option<&str>) -> String {
  let base = format!("LLM server returned {status}");
  let message = serde_json::from_str::<Value>(body.trim())
    .ok()
    .and_then(|v| v.get("error").and_then(server_message).map(str::to_string));
  let Some(message) = message.and_then(|m| clean_server_message(&m, api_key)) else {
    return base;
  };
  format!("{base}: {message}")
}

fn drain_events(
  raws: Vec<String>,
  api_key: Option<&str>,
  useful: &mut bool,
  saw_finish: &mut bool,
) -> (Vec<ChatEvent>, bool) {
  let mut out = Vec::new();
  for raw in raws {
    *saw_finish |= stream_finish_reason(&raw).is_some();
    for event in parse_chat_events(&raw, api_key) {
      let terminal = matches!(event, ChatEvent::Done | ChatEvent::Error { .. });
      *useful |= matches!(event, ChatEvent::Delta { .. } | ChatEvent::ToolCall { .. });
      out.push(event);
      if terminal {
        return (out, true);
      }
    }
  }
  (out, false)
}

pub fn parse_sse_text(body: &str, api_key: Option<&str>) -> Vec<ChatEvent> {
  let mut buffer = SseBuffer::default();
  let mut raws = buffer.push(body.as_bytes());
  raws.extend(buffer.push(b"\n\n"));
  let mut useful = false;
  let mut saw_finish = false;
  let (mut events, terminal) = drain_events(raws, api_key, &mut useful, &mut saw_finish);
  if !terminal {
    events.push(end_of_stream_event(useful, saw_finish));
  }
  let mut redactor = StreamRedactor::new(api_key);
  events.into_iter().flat_map(|event| redactor.push(event)).collect()
}

pub fn end_of_stream_event(useful: bool, saw_finish: bool) -> ChatEvent {
  if !saw_finish {
    ChatEvent::Error { message: "The LLM server closed the stream before the answer finished".into() }
  } else if useful {
    ChatEvent::Done
  } else {
    ChatEvent::Error { message: NO_USABLE_CONTENT.into() }
  }
}

fn config_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
  let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
  std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
  Ok(dir.join(CONFIG_FILE))
}

fn read_config<R: Runtime>(app: &AppHandle<R>) -> Result<LlmConfigFile, String> {
  let path = config_path(app)?;
  if !path.exists() {
    return Ok(LlmConfigFile::default());
  }
  serde_json::from_slice(&std::fs::read(&path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn keyring_entry(user_id: i64) -> Result<keyring::Entry, String> {
  keyring::Entry::new(SERVICE, &format!("{ACCOUNT}:{user_id}")).map_err(|e| e.to_string())
}

/// Predates per-user namespacing; only ever touched to clean it up on logout.
fn legacy_keyring_entry() -> Result<keyring::Entry, String> {
  keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| e.to_string())
}

struct SaveRequest {
  identity: Option<i64>,
  user_id: i64,
  base_url: String,
  model: String,
  api_key: Option<String>,
  clear_api_key: bool,
}

fn save_config<S: SecretStore, W: FnOnce(&LlmConfigFile) -> Result<(), String>>(
  store: &S,
  request: SaveRequest,
  read_previous: impl FnOnce() -> Result<LlmConfigFile, String>,
  prepare_write: impl FnOnce() -> Result<W, String>,
) -> Result<(), String> {
  let SaveRequest { identity, user_id, base_url, model, api_key, clear_api_key } = request;
  let new_key = api_key.filter(|k| !k.is_empty());
  let access = slot_access(identity, user_id);
  if access != SlotAccess::Allowed {
    if access == SlotAccess::OtherAccount || new_key.is_some() || clear_api_key {
      return Err(access.save_refusal());
    }
    let previous = read_previous()?;
    let file = LlmConfigFile { base_url, model, ..previous };
    return prepare_write()?(&file);
  }
  let previous = read_previous()?;
  let new_origin = normalize_origin(&base_url).ok();
  let account = format!("{ACCOUNT}:{user_id}");

  let had_key = store.get(&account)?.is_some();
  let plan = plan_save(slot_origin(&previous, user_id), &previous.base_url, &base_url, new_key.is_some(), clear_api_key, had_key);
  refuses_plaintext_key(&base_url, plan.key_after && !plan.stale_key)?;

  let file = LlmConfigFile {
    base_url,
    model,
    key_origin: previous.key_origin,
    key_origins: origins_after(&plan, previous.key_origins, user_id, new_origin),
  };
  let write = prepare_write()?;
  apply_save(store, &account, &plan, new_key.as_deref().filter(|_| !clear_api_key), || write(&file))
}

fn slot_has_key<S: SecretStore>(store: &S, identity: Option<i64>, user_id: i64) -> Result<bool, String> {
  if !user_slot_allowed(identity, user_id) {
    return Ok(false);
  }
  Ok(store.get(&format!("{ACCOUNT}:{user_id}"))?.is_some())
}

/// `api_key: None`/blank leaves a previously saved key untouched unless `base_url` moved to a new
/// origin, in which case the key is dropped rather than carried over; `clear_api_key: true` always removes it.
#[tauri::command]
pub fn llm_config_set<R: Runtime>(
  app: AppHandle<R>,
  window: tauri::Window<R>,
  user_id: i64,
  base_url: String,
  model: String,
  api_key: Option<String>,
  clear_api_key: bool,
) -> Result<(), String> {
  if window.label() != "main" {
    return Err("llm_config_set may only be called from the main window".into());
  }
  let request = SaveRequest {
    identity: crate::local_identity::current_id(&app),
    user_id,
    base_url,
    model,
    api_key,
    clear_api_key,
  };
  save_config(
    &PersonaLlmSecrets,
    request,
    || read_config(&app),
    || {
      let path = config_path(&app)?;
      Ok(move |file: &LlmConfigFile| {
        std::fs::write(path, serde_json::to_vec(file).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
      })
    },
  )
}

#[tauri::command]
pub fn llm_config_get<R: Runtime>(app: AppHandle<R>, window: tauri::Window<R>, user_id: i64) -> Result<LlmConfigOut, String> {
  if window.label() != "main" {
    return Err("llm_config_get may only be called from the main window".into());
  }
  let file = read_config(&app)?;
  let identity = crate::local_identity::current_id(&app);
  let has_api_key = slot_has_key(&PersonaLlmSecrets, identity, user_id)?;
  let is_local = classify_url(&file.base_url).map(|kind| kind == UrlKind::Local).unwrap_or(false);
  Ok(LlmConfigOut { base_url: file.base_url, model: file.model, has_api_key, is_local, slot_state: slot_access(identity, user_id).state() })
}

/// Best-effort logout cleanup: this user's saved key, plus the pre-namespacing global key if one is still around.
#[tauri::command]
pub fn persona_llm_clear_for_user(user_id: i64) -> Result<(), String> {
  match keyring_entry(user_id)?.delete_credential() {
    Ok(()) | Err(keyring::Error::NoEntry) => {}
    Err(e) => return Err(e.to_string()),
  }
  match legacy_keyring_entry()?.delete_credential() {
    Ok(()) | Err(keyring::Error::NoEntry) => {}
    Err(e) => return Err(e.to_string()),
  }
  Ok(())
}

struct PersonaLlmSecrets;

impl SecretStore for PersonaLlmSecrets {
  fn set(&self, id: &str, secret: &str) -> Result<(), String> {
    keyring::Entry::new(SERVICE, id).map_err(|e| e.to_string())?.set_password(secret).map_err(|e| e.to_string())
  }

  fn get(&self, id: &str) -> Result<Option<String>, String> {
    match keyring::Entry::new(SERVICE, id).map_err(|e| e.to_string())?.get_password() {
      Ok(secret) => Ok(Some(secret)),
      Err(keyring::Error::NoEntry) => Ok(None),
      Err(e) => Err(e.to_string()),
    }
  }

  fn delete(&self, id: &str) -> Result<(), String> {
    match keyring::Entry::new(SERVICE, id).map_err(|e| e.to_string())?.delete_credential() {
      Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
      Err(e) => Err(e.to_string()),
    }
  }
}

#[tauri::command]
pub fn persona_llm_migrate_local<R: Runtime>(app: AppHandle<R>, window: tauri::Window<R>, to_user_id: i64) -> Result<LlmMigrateOut, String> {
  if window.label() != "main" {
    return Err("persona_llm_migrate_local may only be called from the main window".into());
  }
  let to = validate_user_id(to_user_id)?;
  crate::local_profile::require_claim(&app, to)?;
  let mut file = read_config(&app)?;
  let status = migrate_secret(&PersonaLlmSecrets, &format!("{ACCOUNT}:-1"), &format!("{ACCOUNT}:{to}"))?;
  if move_local_key_origin(&mut file.key_origins, &status, to) {
    std::fs::write(config_path(&app)?, serde_json::to_vec(&file).map_err(|e| e.to_string())?)
      .map_err(|e| e.to_string())?;
  }
  Ok(LlmMigrateOut { status })
}

struct InFlight {
  window: String,
  token: u64,
  abort: tokio::task::AbortHandle,
}

#[derive(Default)]
struct InFlightState {
  entries: HashMap<String, InFlight>,
  /// Bumped on every window abort and never removed: a missing label reads as 0, so dropping it would let a
  /// generation captured before the abort match again.
  generations: HashMap<String, u64>,
  next_token: u64,
}

impl InFlightState {
  fn generation(&self, window: &str) -> u64 {
    self.generations.get(window).copied().unwrap_or(0)
  }
}

type InFlightMap = Mutex<InFlightState>;

static IN_FLIGHT: LazyLock<InFlightMap> = LazyLock::new(Default::default);

fn emit_event<R: Runtime>(app: &AppHandle<R>, window: &str, request_id: &str, event: ChatEvent) {
  let _ = app.emit_to(
    tauri::EventTarget::labeled(window),
    "llm://chat",
    serde_json::json!({ "request_id": request_id, "event": event }),
  );
}

pub fn chat_completions_url(base_url: &str) -> String {
  let trimmed = base_url.trim();
  let without_fragment = trimmed.split('#').next().unwrap_or("");
  let (head, query) = match without_fragment.split_once('?') {
    Some((head, query)) => (head, Some(query).filter(|q| !q.is_empty())),
    None => (without_fragment, None),
  };
  let base = head.trim_end_matches('/');
  let lower = base.to_ascii_lowercase();
  let mut url = if lower.ends_with("/chat/completions") {
    base.to_string()
  } else if lower.ends_with("/v1") {
    format!("{base}/chat/completions")
  } else {
    format!("{base}/v1/chat/completions")
  };
  if let Some(query) = query {
    url.push('?');
    url.push_str(query);
  }
  url
}

const ERROR_BODY_CAP: usize = 64 * 1024;
const JSON_BODY_CAP: usize = 4 * 1024 * 1024;

async fn read_capped(response: &mut reqwest::Response, cap: usize) -> Result<Vec<u8>, String> {
  let mut body = Vec::new();
  while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
    body.extend_from_slice(&chunk);
    if body.len() > cap {
      return Err("The LLM server reply is too large".into());
    }
  }
  Ok(body)
}

/// A key may go over plain HTTP to a local/LAN host, so that request must never pass through an env proxy (D-001).
fn bypasses_env_proxy(base_url: &str) -> bool {
  matches!(classify_url(base_url), Ok(UrlKind::Local))
}

fn llm_http_client(base_url: &str) -> Result<reqwest::Client, String> {
  if rustls::crypto::CryptoProvider::get_default().is_none() {
    let _ = rustls::crypto::ring::default_provider().install_default();
  }
  // Only the connect is capped: a local model may legitimately take minutes to finish a turn.
  let builder = reqwest::Client::builder()
    .connect_timeout(Duration::from_secs(5))
    .redirect(reqwest::redirect::Policy::none());
  let builder = if bypasses_env_proxy(base_url) { builder.no_proxy() } else { builder };
  builder.build().map_err(|e| e.to_string())
}

fn redirect_target(location: Option<&str>, api_key: Option<&str>) -> String {
  const FALLBACK: &str = "another address";
  let Some(url) = location.and_then(|l| Url::parse(l).ok()) else {
    return FALLBACK.into();
  };
  let Some(host) = url.host_str() else {
    return FALLBACK.into();
  };
  let mut origin = format!("{}://{host}", url.scheme());
  if let Some(port) = url.port() {
    origin.push_str(&format!(":{port}"));
  }
  if KeyRedactor::new(api_key).contains(&origin) {
    return FALLBACK.into();
  }
  truncate_chars(&origin, ERROR_MESSAGE_CHARS)
}

fn redirect_error(location: Option<&str>, api_key: Option<&str>) -> String {
  let host = redirect_target(location, api_key);
  format!("The LLM server redirected the request to {host}. Update the base URL in Settings.")
}

async fn post_chat(
  client: &reqwest::Client,
  url: &str,
  body: Vec<u8>,
  api_key: Option<&str>,
) -> Result<reqwest::Response, String> {
  let mut builder = client.post(url).header("content-type", "application/json").body(body);
  if let Some(key) = api_key {
    builder = builder.header("authorization", format!("Bearer {key}"));
  }
  let response = builder.send().await.map_err(|e| format!("LLM server unreachable: {}", e.without_url()))?;
  if response.status().is_redirection() {
    let location = response.headers().get("location").and_then(|v| v.to_str().ok());
    return Err(redirect_error(location, api_key));
  }
  Ok(response)
}

async fn stream_chat<R: Runtime>(
  app: &AppHandle<R>,
  window: &str,
  request_id: &str,
  config: &LlmConfigFile,
  api_key: Option<&str>,
  messages: Vec<Value>,
  tools: Option<Value>,
) -> Result<(), String> {
  let client = llm_http_client(&config.base_url)?;
  let url = chat_completions_url(&config.base_url);
  // A typed struct here would silently drop fields the agent loop already built (tool_calls, tool_call_id).
  let mut body = serde_json::json!({
    "model": config.model,
    "stream": true,
    "messages": messages,
  });
  if let Some(tools) = tools {
    body["tools"] = tools;
  }
  let body = serde_json::to_vec(&body).map_err(|e| e.to_string())?;
  let mut response = post_chat(&client, &url, body, api_key).await?;
  if !response.status().is_success() {
    let status = response.status().to_string();
    let body = tokio::time::timeout(Duration::from_secs(5), read_capped(&mut response, ERROR_BODY_CAP))
      .await
      .ok()
      .and_then(Result::ok)
      .unwrap_or_default();
    return Err(sanitize_error_body(&status, &String::from_utf8_lossy(&body), api_key));
  }
  let is_json = response
    .headers()
    .get("content-type")
    .and_then(|v| v.to_str().ok())
    .is_some_and(|v| v.to_ascii_lowercase().contains("json"));
  if is_json {
    let body = read_capped(&mut response, JSON_BODY_CAP).await?;
    let text = String::from_utf8_lossy(&body);
    let events = if text.trim_start().starts_with("data:") {
      parse_sse_text(&text, api_key)
    } else {
      parse_chat_json_reply(&text, api_key)
    };
    for event in events {
      emit_event(app, window, request_id, event);
    }
    return Ok(());
  }
  pump_stream(&mut response, api_key, |event| emit_event(app, window, request_id, event)).await
}

/// Every exit flushes the redactor's held tail: terminal events through `push`, early errors through `finish`.
async fn pump_stream<F: FnMut(ChatEvent)>(
  response: &mut reqwest::Response,
  api_key: Option<&str>,
  mut emit: F,
) -> Result<(), String> {
  let mut redactor = StreamRedactor::new(api_key);
  let result = pump_events(response, api_key, |event| redactor.push(event).into_iter().for_each(&mut emit)).await;
  redactor.finish().into_iter().for_each(&mut emit);
  result
}

async fn pump_events<F: FnMut(ChatEvent)>(
  response: &mut reqwest::Response,
  api_key: Option<&str>,
  mut emit: F,
) -> Result<(), String> {
  let mut buffer = SseBuffer::default();
  let mut useful = false;
  let mut saw_finish = false;
  let mut saw_data = false;
  loop {
    let chunk = match response.chunk().await {
      Ok(chunk) => chunk,
      Err(_) if saw_finish && useful => {
        emit(ChatEvent::Done);
        return Ok(());
      }
      Err(e) => return Err(e.without_url().to_string()),
    };
    let eof = chunk.is_none();
    let raws = match chunk {
      Some(chunk) => buffer.push(&chunk),
      None => buffer.finish().into_iter().collect(),
    };
    if buffer.overflowed() {
      return Err("The LLM server reply is too large".into());
    }
    saw_data |= !raws.is_empty();
    let (events, terminal) = drain_events(raws, api_key, &mut useful, &mut saw_finish);
    events.into_iter().for_each(&mut emit);
    if terminal {
      return Ok(());
    }
    if eof {
      break;
    }
  }
  emit(if saw_data { end_of_stream_event(useful, saw_finish) } else { ChatEvent::Error { message: NO_USABLE_CONTENT.into() } });
  Ok(())
}

/// The key never crosses the IPC boundary; only `llm://chat` events do.
#[tauri::command]
pub async fn llm_chat<R: Runtime>(
  app: AppHandle<R>,
  window: tauri::Window<R>,
  user_id: i64,
  request_id: String,
  messages: Vec<Value>,
  tools: Option<Value>,
) -> Result<(), String> {
  let generation = window_generation(&IN_FLIGHT, window.label())?;
  let access = slot_access(crate::local_identity::current_id(&app), user_id);
  if access != SlotAccess::Allowed {
    return Err(access.chat_refusal());
  }
  let config = read_config(&app)?;
  if config.base_url.is_empty() {
    return Err("Set an LLM base URL first".into());
  }
  let api_key = match keyring_entry(user_id)?.get_password() {
    Ok(key) => Some(key),
    Err(keyring::Error::NoEntry) => None,
    Err(e) => return Err(e.to_string()),
  };
  if api_key.is_some() {
    refuses_plaintext_key(&config.base_url, true)?;
    check_key_origin(slot_origin(&config, user_id), &config.base_url)?;
  }

  let label = window.label().to_string();
  let task_label = label.clone();
  let task_request_id = request_id.clone();
  register_in_flight(&IN_FLIGHT, request_id, label, generation, async move {
    if let Err(message) =
      stream_chat(&app, &task_label, &task_request_id, &config, api_key.as_deref(), messages, tools).await
    {
      emit_event(&app, &task_label, &task_request_id, ChatEvent::Error { message });
    }
  })?;
  Ok(())
}

fn window_generation(map: &InFlightMap, window: &str) -> Result<u64, String> {
  Ok(map.lock().map_err(|_| "lock poisoned".to_string())?.generation(window))
}

struct Unregister {
  map: &'static InFlightMap,
  request_id: String,
  token: u64,
}

impl Drop for Unregister {
  fn drop(&mut self) {
    if let Ok(mut state) = self.map.lock() {
      if state.entries.get(&self.request_id).is_some_and(|entry| entry.token == self.token) {
        state.entries.remove(&self.request_id);
      }
    }
  }
}

/// Spawns while holding the lock, so the task's own removal (on finish, abort or panic) can't run before the
/// insert, and refuses a request whose window was aborted after `generation` was read.
fn register_in_flight<F: std::future::Future<Output = ()> + Send + 'static>(
  map: &'static InFlightMap,
  request_id: String,
  window: String,
  generation: u64,
  work: F,
) -> Result<tokio::task::JoinHandle<()>, String> {
  let mut state = map.lock().map_err(|_| "lock poisoned".to_string())?;
  if state.generation(&window) != generation {
    return Err("The window was closed or reloaded before the request started".into());
  }
  let token = state.next_token;
  state.next_token += 1;
  let task_request_id = request_id.clone();
  let join = tokio::spawn(async move {
    let _unregister = Unregister { map, request_id: task_request_id, token };
    work.await;
  });
  state.entries.insert(request_id, InFlight { window, token, abort: join.abort_handle() });
  Ok(join)
}

fn cancel_in_flight(map: &InFlightMap, request_id: &str) -> Result<Option<String>, String> {
  let entry = map.lock().map_err(|_| "lock poisoned".to_string())?.entries.remove(request_id);
  Ok(entry.map(|entry| {
    entry.abort.abort();
    entry.window
  }))
}

fn abort_window_in(map: &InFlightMap, window: &str) -> usize {
  let mut aborts = Vec::new();
  if let Ok(mut state) = map.lock() {
    *state.generations.entry(window.to_string()).or_default() += 1;
    state.entries.retain(|_, entry| {
      let keep = entry.window != window;
      if !keep {
        aborts.push(entry.abort.clone());
      }
      keep
    });
  }
  aborts.iter().for_each(tokio::task::AbortHandle::abort);
  aborts.len()
}

fn window_event_aborts(event: &tauri::WindowEvent) -> bool {
  matches!(event, tauri::WindowEvent::Destroyed)
}

fn page_load_aborts(event: tauri::webview::PageLoadEvent) -> bool {
  event == tauri::webview::PageLoadEvent::Started
}

/// A destroyed or reloading page never runs its Vue unmount hooks, so its requests are stopped here.
pub fn on_window_event(label: &str, event: &tauri::WindowEvent) {
  if window_event_aborts(event) {
    abort_window_in(&IN_FLIGHT, label);
  }
}

pub fn on_page_load(label: &str, event: tauri::webview::PageLoadEvent) {
  if page_load_aborts(event) {
    abort_window_in(&IN_FLIGHT, label);
  }
}

/// Emits an error event too, so a caller blocked reading `llm://chat` unblocks instead of hanging.
#[tauri::command]
pub fn llm_cancel<R: Runtime>(app: AppHandle<R>, request_id: String) -> Result<(), String> {
  if let Some(window) = cancel_in_flight(&IN_FLIGHT, &request_id)? {
    emit_event(&app, &window, &request_id, ChatEvent::Error { message: "cancelled".into() });
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn classifies_localhost_and_loopback_as_local() {
    for url in ["http://localhost:8000", "http://127.0.0.1:8000", "http://[::1]:8000"] {
      assert_eq!(classify_url(url).unwrap(), UrlKind::Local, "{url}");
    }
  }

  #[test]
  fn classifies_dot_local_and_rfc1918_and_link_local_as_local() {
    let octets = [[10, 0, 0, 5], [172, 16, 0, 5], [192, 168, 1, 5], [169, 254, 1, 1]];
    let mut urls: Vec<String> = octets
      .iter()
      .map(|o| format!("http://{}.{}.{}.{}:8000", o[0], o[1], o[2], o[3]))
      .collect();
    urls.push("http://my-mac.local:8000".into());
    urls.push("http://[fe80::1]:8000".into());
    urls.push("http://[fc00::1]:8000".into());
    for url in urls {
      assert_eq!(classify_url(&url).unwrap(), UrlKind::Local, "{url}");
    }
  }

  #[test]
  fn classifies_public_hosts_as_remote() {
    for url in ["https://api.openai.com/v1", "http://8.8.8.8:8000", "https://example.com"] {
      assert_eq!(classify_url(url).unwrap(), UrlKind::Remote, "{url}");
    }
  }

  #[test]
  fn rejects_an_empty_or_unparseable_url() {
    assert!(classify_url("").is_err());
    assert!(classify_url("not a url").is_err());
  }

  #[test]
  fn normalize_origin_folds_default_ports_and_ignores_path_and_case() {
    assert_eq!(normalize_origin("https://API.example.com/v1").unwrap(), "https://api.example.com:443");
    assert_eq!(normalize_origin("https://api.example.com:443/v1/chat").unwrap(), "https://api.example.com:443");
    assert_eq!(normalize_origin("http://localhost:8000/v1").unwrap(), "http://localhost:8000");
  }

  #[test]
  fn normalize_origin_treats_a_different_port_or_scheme_as_a_different_origin() {
    assert_ne!(
      normalize_origin("https://api.example.com").unwrap(),
      normalize_origin("https://api.example.com:8443").unwrap(),
    );
    assert_ne!(
      normalize_origin("https://api.example.com").unwrap(),
      normalize_origin("http://api.example.com").unwrap(),
    );
  }

  #[test]
  fn normalize_origin_rejects_an_empty_or_unparseable_url() {
    assert!(normalize_origin("").is_err());
    assert!(normalize_origin("not a url").is_err());
  }

  fn file_with(legacy: Option<&str>, slots: &[(&str, &str)]) -> LlmConfigFile {
    LlmConfigFile {
      key_origin: legacy.map(str::to_string),
      key_origins: slots.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
      ..Default::default()
    }
  }

  #[test]
  fn slot_origin_prefers_the_slot_entry_over_the_legacy_value() {
    let file = file_with(Some("https://old.example:443"), &[("7", "https://a.example:443")]);
    assert_eq!(slot_origin(&file, 7), Some("https://a.example:443"));
    assert_eq!(slot_origin(&file, 8), Some("https://old.example:443"));
    assert_eq!(slot_origin(&file_with(None, &[]), 8), None);
  }

  #[test]
  fn a_key_is_sent_only_to_the_origin_it_was_issued_for() {
    assert!(key_may_be_sent(Some("https://api.example.com:443"), "https://API.example.com/v1"));
    assert!(!key_may_be_sent(Some("https://a.example:443"), "https://b.example/v1"));
    assert!(!key_may_be_sent(Some("https://a.example:443"), "http://a.example/v1"));
    assert!(!key_may_be_sent(Some("https://a.example:443"), "not a url"));
    assert!(!key_may_be_sent(Some("https://a.example:443"), ""));
  }

  #[test]
  fn refusal_names_only_the_host_and_never_credentials_or_path() {
    let err = check_key_origin(Some("https://a.example:443"), "https://user:s3cret@b.example:8443/v1/path?k=1").unwrap_err();
    assert!(err.contains("b.example"), "{err}");
    for leaked in ["user", "s3cret", "/v1", "k=1", "8443"] {
      assert!(!err.contains(leaked), "{err}");
    }
    let err = check_key_origin(None, "https://user:s3cret@b.example/v1").unwrap_err();
    assert!(err.contains("b.example") && !err.contains("s3cret"), "{err}");
    assert!(!err.contains("different server"), "{err}");
    assert!(check_key_origin(Some("https://a.example:443"), "not a url").unwrap_err().contains("this server"));
  }

  #[test]
  fn a_refusal_leaves_the_stored_key_and_origins_untouched() {
    let file = file_with(None, &[("1", "https://a.example:443")]);
    let before = file.key_origins.clone();
    let store = crate::local_access::tokens::MemorySecrets::new();
    store.set("api-key:1", "sk-test").unwrap();
    assert!(check_key_origin(slot_origin(&file, 1), "https://b.example").is_err());
    assert_eq!(store.get("api-key:1").unwrap().as_deref(), Some("sk-test"));
    assert_eq!(file.key_origins, before);
  }

  #[test]
  fn a_key_moved_by_the_migration_is_allowed_in_the_new_slot() {
    let mut file = file_with(None, &[("-1", "https://a.example:443")]);
    assert!(check_key_origin(slot_origin(&file, 7), "https://a.example/v1").is_err());
    assert!(move_local_key_origin(&mut file.key_origins, &MigrateStatus::Moved, 7));
    assert!(check_key_origin(slot_origin(&file, 7), "https://a.example/v1").is_ok());
    assert!(check_key_origin(slot_origin(&file, -1), "https://a.example/v1").is_err());
  }

  #[test]
  fn a_key_migrated_before_slot_origins_existed_falls_back_to_the_legacy_origin() {
    let file = file_with(Some("https://a.example:443"), &[]);
    assert!(check_key_origin(slot_origin(&file, 7), "https://a.example/v1").is_ok());
    assert!(check_key_origin(slot_origin(&file, 7), "https://b.example/v1").is_err());
    assert!(check_key_origin(slot_origin(&file_with(None, &[]), 7), "https://a.example/v1").is_err());
  }

  #[test]
  fn the_local_profile_slot_goes_through_the_same_origin_check() {
    let file = file_with(None, &[("-1", "https://a.example:443")]);
    assert!(user_slot_allowed(None, -1));
    assert!(check_key_origin(slot_origin(&file, -1), "https://a.example").is_ok());
    assert!(check_key_origin(slot_origin(&file, -1), "https://b.example").is_err());
  }

  #[test]
  fn a_key_without_a_recorded_origin_is_not_sent() {
    assert!(!key_may_be_sent(None, "https://a.example/v1"));
  }

  #[test]
  fn slot_origins_are_tracked_per_account() {
    let mut file = file_with(None, &[("1", "https://a.example:443")]);
    file.key_origins = next_key_origins(file.key_origins, 2, true, Some("https://b.example:443".into()));
    assert!(key_may_be_sent(slot_origin(&file, 2), "https://b.example"));
    assert!(!key_may_be_sent(slot_origin(&file, 1), "https://b.example"));
    assert!(key_may_be_sent(slot_origin(&file, 1), "https://a.example"));
  }

  #[test]
  fn next_key_origins_sets_or_removes_only_this_slot() {
    let start: HashMap<String, String> =
      [("1".to_string(), "https://a.example:443".to_string())].into_iter().collect();
    let set = next_key_origins(start.clone(), 2, true, Some("https://b.example:443".into()));
    assert_eq!(set.len(), 2);
    assert_eq!(set["1"], "https://a.example:443");
    assert_eq!(set["2"], "https://b.example:443");
    let no_key = next_key_origins(set.clone(), 2, false, Some("https://b.example:443".into()));
    assert_eq!(no_key, start);
    let no_origin = next_key_origins(set, 2, true, None);
    assert_eq!(no_origin["2"], UNKNOWN_ORIGIN);
    assert_eq!(no_origin["1"], "https://a.example:443");
  }

  #[test]
  fn a_kept_key_without_a_url_origin_gets_a_placeholder_entry() {
    let start = origins(&[("1", "https://a.example:443")]);
    let kept = next_key_origins(start.clone(), 2, true, None);
    assert_eq!(kept["2"], UNKNOWN_ORIGIN);
    assert_eq!(kept["1"], "https://a.example:443");
    assert_eq!(next_key_origins(kept, 2, false, None), start);
  }

  #[test]
  fn a_placeholder_entry_shadows_the_legacy_origin_and_is_never_sendable() {
    let file = file_with(Some("https://a.example:443"), &[("2", UNKNOWN_ORIGIN)]);
    assert_eq!(slot_origin(&file, 2), Some(UNKNOWN_ORIGIN));
    assert!(!key_may_be_sent(slot_origin(&file, 2), "https://a.example"));
    let err = check_key_origin(slot_origin(&file, 2), "https://a.example/v1").unwrap_err();
    assert!(err.contains("not tied to a server"), "{err}");
    assert!(!err.contains("different server"), "{err}");
  }

  #[test]
  fn a_keyless_save_keeps_the_placeholder_and_a_new_key_replaces_it() {
    let start = origins(&[("2", UNKNOWN_ORIGIN)]);
    let keyless = plan_save(Some(UNKNOWN_ORIGIN), "https://a.example", "https://a.example", false, false, true);
    assert!(keyless.stale_key && keyless.key_after);
    let kept = origins_after(&keyless, start.clone(), 2, normalize_origin("https://a.example").ok());
    assert_eq!(kept, start);
    assert!(!key_may_be_sent(kept.get("2").map(String::as_str), "https://a.example"));
    let fresh = plan_save(Some(UNKNOWN_ORIGIN), "https://a.example", "https://a.example", true, false, true);
    let replaced = origins_after(&fresh, start, 2, normalize_origin("https://a.example").ok());
    assert_eq!(replaced, origins(&[("2", "https://a.example:443")]));
  }

  #[test]
  fn a_moved_key_does_not_inherit_a_leftover_account_origin() {
    let mut map = origins(&[("7", "https://stale.example:443"), ("9", "https://c.example:443")]);
    assert!(move_local_key_origin(&mut map, &MigrateStatus::Moved, 7));
    assert_eq!(map, origins(&[("9", "https://c.example:443")]));
    assert!(!move_local_key_origin(&mut map, &MigrateStatus::Moved, 7));
  }

  #[test]
  fn user_slot_allowed_follows_the_shared_identity() {
    assert!(user_slot_allowed(Some(5), 5));
    assert!(!user_slot_allowed(Some(5), 6));
    assert!(!user_slot_allowed(Some(5), -1));
    assert!(!user_slot_allowed(Some(5), 0));
    assert!(user_slot_allowed(None, -1));
    assert!(!user_slot_allowed(None, 0));
    assert!(!user_slot_allowed(None, 5));
  }

  fn origins(entries: &[(&str, &str)]) -> HashMap<String, String> {
    entries.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
  }

  #[test]
  fn migrating_moves_the_local_origin_to_the_account() {
    for status in [MigrateStatus::Moved, MigrateStatus::Already] {
      let mut map = origins(&[("-1", "https://a.example:443"), ("9", "https://c.example:443")]);
      assert!(move_local_key_origin(&mut map, &status, 7));
      assert_eq!(map, origins(&[("7", "https://a.example:443"), ("9", "https://c.example:443")]));
    }
  }

  #[test]
  fn migrating_never_overwrites_an_existing_account_origin() {
    let mut map = origins(&[("-1", "https://a.example:443"), ("7", "https://b.example:443")]);
    assert!(move_local_key_origin(&mut map, &MigrateStatus::Already, 7));
    assert_eq!(map, origins(&[("7", "https://b.example:443")]));
  }

  #[test]
  fn a_moved_key_takes_its_own_origin_over_a_stale_account_entry() {
    let mut map = origins(&[("-1", "https://a.example:443"), ("7", "https://stale.example:443")]);
    assert!(move_local_key_origin(&mut map, &MigrateStatus::Moved, 7));
    assert_eq!(map, origins(&[("7", "https://a.example:443")]));
  }

  #[test]
  fn migrating_changes_nothing_on_conflict_or_none() {
    for status in [MigrateStatus::Conflict, MigrateStatus::None] {
      let mut map = origins(&[("-1", "https://a.example:443")]);
      assert!(!move_local_key_origin(&mut map, &status, 7));
      assert_eq!(map, origins(&[("-1", "https://a.example:443")]));
    }
  }

  #[test]
  fn legacy_config_without_slot_origins_still_parses() {
    let file: LlmConfigFile =
      serde_json::from_str(r#"{"base_url":"https://a.example","model":"m","key_origin":"https://a.example:443"}"#).unwrap();
    assert!(file.key_origins.is_empty());
    assert_eq!(slot_origin(&file, 3), Some("https://a.example:443"));
  }

  #[test]
  fn key_will_exist_matrix() {
    // clear_api_key always wins.
    assert!(!key_will_exist(true, true, true, true));
    // a freshly supplied key always ends up saved.
    assert!(key_will_exist(false, true, true, false));
    assert!(key_will_exist(false, true, false, false));
    // keeping a blank api_key drops the old key once the origin moved...
    assert!(!key_will_exist(false, false, true, true));
    // ...but keeps it when the origin did not change.
    assert!(key_will_exist(false, false, false, true));
    // nothing to keep if there was never a key.
    assert!(!key_will_exist(false, false, false, false));
  }

  #[test]
  fn refuses_a_key_over_plain_http_to_a_remote_host() {
    let err = refuses_plaintext_key("http://api.openai.com/v1", true).unwrap_err();
    assert!(err.contains("HTTPS"), "{err}");
  }

  #[test]
  fn allows_plain_http_to_a_remote_host_when_no_key_will_be_saved() {
    assert!(refuses_plaintext_key("http://api.openai.com/v1", false).is_ok());
  }

  #[test]
  fn allows_a_key_over_plain_http_to_localhost_or_the_lan() {
    assert!(refuses_plaintext_key("http://localhost:8000", true).is_ok());
    assert!(refuses_plaintext_key("http://my-mac.local:8000", true).is_ok());
  }

  #[test]
  fn allows_a_key_over_https_to_a_remote_host() {
    assert!(refuses_plaintext_key("https://api.openai.com/v1", true).is_ok());
  }

  struct FailingSet(crate::local_access::tokens::MemorySecrets);

  impl SecretStore for FailingSet {
    fn set(&self, _id: &str, _secret: &str) -> Result<(), String> {
      Err("keychain unavailable".into())
    }
    fn get(&self, id: &str) -> Result<Option<String>, String> {
      self.0.get(id)
    }
    fn delete(&self, id: &str) -> Result<(), String> {
      self.0.delete(id)
    }
  }

  struct FailingDelete(crate::local_access::tokens::MemorySecrets);

  impl SecretStore for FailingDelete {
    fn set(&self, id: &str, secret: &str) -> Result<(), String> {
      self.0.set(id, secret)
    }
    fn get(&self, id: &str) -> Result<Option<String>, String> {
      self.0.get(id)
    }
    fn delete(&self, _id: &str) -> Result<(), String> {
      Err("keychain unavailable".into())
    }
  }

  const A: &str = "https://a.example/v1";
  const B: &str = "https://b.example/v1";
  const A_ORIGIN: &str = "https://a.example:443";
  const B_ORIGIN: &str = "https://b.example:443";

  #[test]
  fn a_failed_key_write_on_a_new_server_leaves_no_old_key_behind() {
    let store = FailingSet(crate::local_access::tokens::MemorySecrets::new());
    store.0.set("api-key:7", "K1").unwrap();
    let plan = plan_save(Some(A_ORIGIN), A, B, true, false, true);
    let mut config_origin = A_ORIGIN;
    let result = apply_save(&store, "api-key:7", &plan, Some("K2"), || {
      config_origin = B_ORIGIN;
      Ok(())
    });
    assert!(result.is_err());
    assert_eq!(config_origin, B_ORIGIN);
    assert_eq!(store.get("api-key:7").unwrap(), None);
  }

  #[test]
  fn a_failed_key_delete_writes_nothing() {
    let store = FailingDelete(crate::local_access::tokens::MemorySecrets::new());
    store.0.set("api-key:7", "K1").unwrap();
    let plan = plan_save(Some(A_ORIGIN), A, B, true, false, true);
    let mut written = false;
    let result = apply_save(&store, "api-key:7", &plan, Some("K2"), || {
      written = true;
      Ok(())
    });
    assert!(result.is_err());
    assert!(!written);
    assert_eq!(store.get("api-key:7").unwrap().as_deref(), Some("K1"));
  }

  #[test]
  fn a_failed_key_write_at_the_same_server_keeps_the_old_key() {
    let store = FailingSet(crate::local_access::tokens::MemorySecrets::new());
    store.0.set("api-key:7", "K1").unwrap();
    let plan = plan_save(Some(A_ORIGIN), A, A, true, false, true);
    assert!(apply_save(&store, "api-key:7", &plan, Some("K2"), || Ok(())).is_err());
    assert_eq!(store.get("api-key:7").unwrap().as_deref(), Some("K1"));
  }

  #[test]
  fn a_failed_config_write_keeps_the_old_key_at_the_same_server() {
    let store = crate::local_access::tokens::MemorySecrets::new();
    store.set("api-key:7", "K1").unwrap();
    let plan = plan_save(Some(A_ORIGIN), A, A, true, false, true);
    assert!(apply_save(&store, "api-key:7", &plan, Some("K2"), || Err("disk full".into())).is_err());
    assert_eq!(store.get("api-key:7").unwrap().as_deref(), Some("K1"));
  }

  #[test]
  fn a_new_key_at_a_new_server_replaces_the_old_one() {
    let store = crate::local_access::tokens::MemorySecrets::new();
    store.set("api-key:7", "K1").unwrap();
    let plan = plan_save(Some(A_ORIGIN), A, B, true, false, true);
    apply_save(&store, "api-key:7", &plan, Some("K2"), || Ok(())).unwrap();
    assert_eq!(store.get("api-key:7").unwrap().as_deref(), Some("K2"));
  }

  #[test]
  fn a_save_that_does_not_move_the_server_keeps_the_key_and_its_origin() {
    let plan = plan_save(Some(A_ORIGIN), B, B, false, false, true);
    assert!(plan.stale_key);
    assert!(plan.key_after);
    assert!(!plan.drop_old_key);
    let store = crate::local_access::tokens::MemorySecrets::new();
    store.set("api-key:7", "K1").unwrap();
    apply_save(&store, "api-key:7", &plan, None, || Ok(())).unwrap();
    assert_eq!(store.get("api-key:7").unwrap().as_deref(), Some("K1"));
  }

  #[test]
  fn a_refused_key_stays_refused_after_a_save_that_did_not_move_the_server() {
    let plan = plan_save(Some(A_ORIGIN), B, B, false, false, true);
    let before = origins(&[("7", A_ORIGIN)]);
    let after = origins_after(&plan, before.clone(), 7, normalize_origin(B).ok());
    assert_eq!(after, before);
    assert!(!key_may_be_sent(after.get("7").map(String::as_str), B));
  }

  #[test]
  fn a_user_who_moves_the_server_with_a_blank_key_still_drops_the_key() {
    let plan = plan_save(Some(A_ORIGIN), A, B, false, false, true);
    assert!(!plan.stale_key);
    assert!(!plan.key_after);
    assert!(plan.drop_old_key);
  }

  #[test]
  fn returning_to_the_slot_origin_keeps_the_key() {
    let plan = plan_save(Some(A_ORIGIN), B, A, false, false, true);
    assert!(!plan.stale_key);
    assert!(plan.key_after);
    assert!(!plan.drop_old_key);
  }

  #[test]
  fn a_keyless_save_at_an_unchanged_url_keeps_a_key_with_no_origin() {
    let plan = plan_save(None, A, A, false, false, true);
    assert!(plan.stale_key);
    assert!(plan.key_after);
    assert!(!plan.drop_old_key);
  }

  #[test]
  fn a_new_key_or_a_clear_is_never_stale() {
    assert!(!plan_save(Some(A_ORIGIN), B, B, true, false, true).stale_key);
    assert!(!plan_save(Some(A_ORIGIN), B, B, false, true, true).stale_key);
    assert!(plan_save(Some(A_ORIGIN), B, B, false, true, true).drop_old_key);
  }

  #[test]
  fn refuses_a_key_over_plain_http_written_in_unusual_forms() {
    for url in ["http:api.example.com", "http:/api.example.com", "ht\ttp://api.example.com", "HTTP://api.example.com", "  http://api.example.com"] {
      assert!(refuses_plaintext_key(url, true).is_err(), "{url:?}");
    }
  }

  #[test]
  fn unusual_http_forms_to_local_hosts_stay_allowed() {
    for url in ["http:localhost:8000", "http:/my-mac.local", "ht\ttp://my-mac.local:8000"] {
      assert!(refuses_plaintext_key(url, true).is_ok(), "{url:?}");
    }
  }

  struct Spy {
    inner: crate::local_access::tokens::MemorySecrets,
    gets: std::sync::atomic::AtomicU32,
  }

  impl Spy {
    fn with(slots: &[(i64, &str)]) -> Self {
      let inner = crate::local_access::tokens::MemorySecrets::new();
      for (id, key) in slots {
        inner.set(&format!("api-key:{id}"), key).unwrap();
      }
      Spy { inner, gets: std::sync::atomic::AtomicU32::new(0) }
    }
    fn key(&self, id: i64) -> Option<String> {
      self.inner.get(&format!("api-key:{id}")).unwrap()
    }
  }

  impl SecretStore for Spy {
    fn set(&self, id: &str, secret: &str) -> Result<(), String> {
      self.inner.set(id, secret)
    }
    fn get(&self, id: &str) -> Result<Option<String>, String> {
      self.gets.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
      self.inner.get(id)
    }
    fn delete(&self, id: &str) -> Result<(), String> {
      self.inner.delete(id)
    }
  }

  struct FailingGet;

  impl SecretStore for FailingGet {
    fn set(&self, _id: &str, _secret: &str) -> Result<(), String> {
      Ok(())
    }
    fn get(&self, _id: &str) -> Result<Option<String>, String> {
      Err("keychain unavailable".into())
    }
    fn delete(&self, _id: &str) -> Result<(), String> {
      Ok(())
    }
  }

  const REFUSAL: &str = "These AI provider settings belong to another account.";
  const NOT_READY: &str = "Your account is not available to the desktop app yet. Reload the app or sign in again, then retry.";

  fn request(identity: Option<i64>, user_id: i64, api_key: Option<&str>, clear_api_key: bool) -> SaveRequest {
    SaveRequest {
      identity,
      user_id,
      base_url: B.into(),
      model: "m".into(),
      api_key: api_key.map(String::from),
      clear_api_key,
    }
  }

  struct Saved {
    result: Result<(), String>,
    written: Option<LlmConfigFile>,
    read: bool,
  }

  fn run_save(store: &Spy, req: SaveRequest, config: LlmConfigFile) -> Saved {
    let read = std::cell::Cell::new(false);
    let written = std::cell::RefCell::new(None);
    let result = save_config(
      store,
      req,
      || {
        read.set(true);
        Ok(config)
      },
      || {
        Ok(|file: &LlmConfigFile| {
          *written.borrow_mut() = Some(file.clone());
          Ok(())
        })
      },
    );
    let written = written.into_inner();
    Saved { result, written, read: read.get() }
  }

  fn config_for_slot(user_id: &str) -> LlmConfigFile {
    file_with(None, &[(user_id, A_ORIGIN)])
  }

  #[test]
  fn saving_a_new_key_for_another_accounts_slot_is_refused_and_touches_nothing() {
    let store = Spy::with(&[(8, "K8")]);
    let saved = run_save(&store, request(Some(7), 8, Some("NEW"), false), config_for_slot("8"));
    assert_eq!(saved.result, Err(REFUSAL.to_string()));
    assert_eq!(store.key(8).as_deref(), Some("K8"));
    assert!(saved.written.is_none());
  }

  #[test]
  fn clearing_another_accounts_key_is_refused_and_touches_nothing() {
    let store = Spy::with(&[(8, "K8")]);
    let saved = run_save(&store, request(Some(7), 8, None, true), config_for_slot("8"));
    assert_eq!(saved.result, Err(REFUSAL.to_string()));
    assert_eq!(store.key(8).as_deref(), Some("K8"));
    assert!(saved.written.is_none());
  }

  #[test]
  fn a_signed_in_account_cannot_touch_the_local_slot() {
    let store = Spy::with(&[(-1, "KL")]);
    let saved = run_save(&store, request(Some(7), -1, None, true), config_for_slot("-1"));
    assert_eq!(saved.result, Err(NOT_READY.to_string()));
    assert_eq!(store.key(-1).as_deref(), Some("KL"));
    assert!(saved.written.is_none());
  }

  #[test]
  fn the_local_account_cannot_touch_an_accounts_slot() {
    let store = Spy::with(&[(8, "K8")]);
    let saved = run_save(&store, request(None, 8, Some("NEW"), false), config_for_slot("8"));
    assert_eq!(saved.result, Err(NOT_READY.to_string()));
    assert_eq!(store.key(8).as_deref(), Some("K8"));
    assert!(saved.written.is_none());
  }

  #[test]
  fn slot_zero_is_refused() {
    for identity in [None, Some(7)] {
      let store = Spy::with(&[(0, "K0")]);
      let saved = run_save(&store, request(identity, 0, Some("NEW"), false), LlmConfigFile::default());
      assert_eq!(saved.result, Err(NOT_READY.to_string()));
      assert_eq!(store.key(0).as_deref(), Some("K0"));
      assert!(saved.written.is_none());
    }
  }

  #[test]
  fn a_refused_save_reads_neither_the_config_nor_the_keychain() {
    let store = Spy::with(&[(8, "K8")]);
    let result = save_config(
      &store,
      request(Some(7), 8, Some("NEW"), false),
      || panic!("config was read"),
      || Ok(|_: &LlmConfigFile| panic!("config was written")),
    );
    assert_eq!(result, Err(REFUSAL.to_string()));
    assert_eq!(store.gets.load(std::sync::atomic::Ordering::SeqCst), 0);
  }

  #[test]
  fn slot_access_classifies_identity_and_slot() {
    for (identity, user_id, expected) in [
      (Some(5), 0, SlotAccess::NotReady),
      (None, 7, SlotAccess::NotReady),
      (Some(5), -1, SlotAccess::NotReady),
      (None, 0, SlotAccess::NotReady),
      (None, -2, SlotAccess::NotReady),
      (Some(5), 6, SlotAccess::OtherAccount),
      (Some(5), 5, SlotAccess::Allowed),
      (None, -1, SlotAccess::Allowed),
    ] {
      assert_eq!(slot_access(identity, user_id), expected, "{identity:?} {user_id}");
    }
  }

  fn keyless_edit(identity: Option<i64>, user_id: i64, base_url: &str, model: &str) -> SaveRequest {
    SaveRequest { identity, user_id, base_url: base_url.into(), model: model.into(), api_key: None, clear_api_key: false }
  }

  #[test]
  fn a_keyless_edit_is_saved_without_the_keychain_when_not_allowed() {
    for (identity, user_id) in [(Some(5), 0), (None, 7)] {
      let store = Spy::with(&[(user_id, "K")]);
      let config = LlmConfigFile { base_url: A.into(), model: "old".into(), ..config_for_slot(&user_id.to_string()) };
      let saved = run_save(&store, keyless_edit(identity, user_id, B, "new"), config.clone());
      assert_eq!(saved.result, Ok(()), "{identity:?} {user_id}");
      let written = saved.written.unwrap();
      assert_eq!(written.base_url, B);
      assert_eq!(written.model, "new");
      assert_eq!(written.key_origins, config.key_origins);
      assert_eq!(written.key_origin, config.key_origin);
      assert_eq!(store.gets.load(std::sync::atomic::Ordering::SeqCst), 0);
      assert_eq!(store.key(user_id).as_deref(), Some("K"));
    }
  }

  #[test]
  fn a_keyless_edit_for_another_account_is_refused_and_writes_nothing() {
    let store = Spy::with(&[(6, "K")]);
    let saved = run_save(&store, keyless_edit(Some(5), 6, B, "new"), config_for_slot("6"));
    assert_eq!(saved.result, Err(REFUSAL.to_string()));
    assert!(saved.written.is_none());
    assert!(!saved.read);
  }

  #[test]
  fn a_keyful_save_is_refused_with_the_matching_message_when_not_allowed() {
    for (identity, user_id, message) in [(Some(5), 0, NOT_READY), (None, 7, NOT_READY), (Some(5), 6, REFUSAL)] {
      for (api_key, clear) in [(Some("NEW"), false), (None, true)] {
        let store = Spy::with(&[(user_id, "K")]);
        let saved = run_save(&store, request(identity, user_id, api_key, clear), config_for_slot(&user_id.to_string()));
        assert_eq!(saved.result, Err(message.to_string()), "{identity:?} {user_id}");
        assert!(saved.written.is_none());
        assert!(!saved.read);
        assert_eq!(store.key(user_id).as_deref(), Some("K"));
      }
    }
  }

  #[test]
  fn a_keyless_origin_move_while_not_ready_keeps_the_key_and_a_later_save_back_still_uses_it() {
    let store = Spy::with(&[(7, "K7")]);
    let config = file_with(None, &[("7", A_ORIGIN)]);
    let moved = run_save(&store, keyless_edit(None, 7, B, "m"), config);
    assert_eq!(moved.result, Ok(()));
    let after_move = moved.written.unwrap();
    assert_eq!(after_move.key_origins.get("7").map(String::as_str), Some(A_ORIGIN));
    assert_eq!(store.key(7).as_deref(), Some("K7"));

    let back = run_save(&store, keyless_edit(Some(7), 7, A, "m"), after_move);
    assert_eq!(back.result, Ok(()));
    assert_eq!(store.key(7).as_deref(), Some("K7"));
    let written = back.written.unwrap();
    assert_eq!(written.key_origins.get("7").map(String::as_str), Some(A_ORIGIN));
    assert!(!plan_save(slot_origin(&written, 7), &written.base_url, &written.base_url, false, false, true).stale_key);
  }

  #[test]
  fn a_save_for_the_current_account_stores_the_key_and_its_origin() {
    let store = Spy::with(&[]);
    let saved = run_save(&store, request(Some(7), 7, Some("NEW"), false), LlmConfigFile::default());
    assert_eq!(saved.result, Ok(()));
    assert!(saved.read);
    assert_eq!(store.key(7).as_deref(), Some("NEW"));
    let written = saved.written.unwrap();
    assert_eq!(written.key_origins.get("7").map(String::as_str), Some(B_ORIGIN));
  }

  #[test]
  fn a_save_for_the_local_account_works_when_signed_out() {
    let store = Spy::with(&[]);
    let saved = run_save(&store, request(None, -1, Some("NEW"), false), LlmConfigFile::default());
    assert_eq!(saved.result, Ok(()));
    assert_eq!(store.key(-1).as_deref(), Some("NEW"));
    assert!(saved.written.is_some());
  }

  #[test]
  fn a_config_path_error_surfaces_and_deletes_nothing() {
    let store = Spy::with(&[(7, "K7")]);
    let result = save_config(
      &store,
      request(Some(7), 7, None, true),
      || Ok(config_for_slot("7")),
      || Err::<fn(&LlmConfigFile) -> Result<(), String>, _>("no data dir".to_string()),
    );
    assert_eq!(result, Err("no data dir".to_string()));
    assert_eq!(store.key(7).as_deref(), Some("K7"));
  }

  #[test]
  fn another_accounts_slot_reports_no_key_without_reading_the_store() {
    let store = Spy::with(&[(8, "K8")]);
    assert_eq!(slot_has_key(&store, Some(7), 8), Ok(false));
    assert_eq!(store.gets.load(std::sync::atomic::Ordering::SeqCst), 0);
  }

  #[test]
  fn the_current_accounts_slot_reports_whether_it_holds_a_key() {
    let store = Spy::with(&[(7, "K7")]);
    assert_eq!(slot_has_key(&store, Some(7), 7), Ok(true));
    assert_eq!(slot_has_key(&Spy::with(&[]), Some(7), 7), Ok(false));
  }

  #[test]
  fn a_store_error_on_the_current_accounts_slot_propagates() {
    assert!(slot_has_key(&FailingGet, Some(7), 7).is_err());
  }

  #[test]
  fn sse_buffer_reassembles_an_event_split_across_chunks() {
    let mut buffer = SseBuffer::default();
    assert!(buffer.push(b"data: {\"choices\":[{\"delta\":{\"content\":\"Hel").is_empty());
    let events = buffer.push(b"lo\"}}]}\n\n");
    assert_eq!(events, vec!["{\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}"]);
  }

  #[test]
  fn sse_buffer_handles_multiple_events_in_one_chunk_and_a_done_sentinel() {
    let mut buffer = SseBuffer::default();
    let events = buffer.push(b"data: {\"a\":1}\n\ndata: [DONE]\n\n");
    assert_eq!(events, vec!["{\"a\":1}", "[DONE]"]);
  }

  #[test]
  fn sse_buffer_never_decodes_a_multi_byte_character_split_across_chunks() {
    // "П" is 2 UTF-8 bytes (0xD0 0x9F); split mid-character it must not decode as U+FFFD.
    let payload = "{\"choices\":[{\"delta\":{\"content\":\"Привет\"}}]}";
    let bytes = payload.as_bytes();
    let split = 3;
    let mut buffer = SseBuffer::default();
    let mut first = b"data: ".to_vec();
    first.extend_from_slice(&bytes[..split]);
    assert!(buffer.push(&first).is_empty());
    let mut rest = bytes[split..].to_vec();
    rest.extend_from_slice(b"\n\n");
    let events = buffer.push(&rest);
    assert_eq!(events, vec![payload]);
  }

  #[test]
  fn parses_a_text_delta_a_tool_call_and_done() {
    assert_eq!(
      parse_chat_events("{\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}", None),
      vec![ChatEvent::Delta { text: "Hi".into() }]
    );
    assert_eq!(
      parse_chat_events(
        "{\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"tmgr_request\",\"arguments\":\"{\\\"method\\\"\"}}]}}]}",
        None
      ),
      vec![ChatEvent::ToolCall {
        index: 0,
        id: Some("call_1".into()),
        name: Some("tmgr_request".into()),
        arguments: "{\"method\"".into(),
      }]
    );
    assert_eq!(parse_chat_events("[DONE]", None), vec![ChatEvent::Done]);
  }

  #[test]
  fn keeps_every_tool_call_in_a_chunk_and_the_content_before_them() {
    let chunk = r#"{"choices":[{"delta":{"content":"Let me look.","tool_calls":[
      {"index":0,"id":"call_1","function":{"name":"tmgr_request","arguments":"{\"a\":1}"}},
      {"index":1,"id":"call_2","function":{"name":"tmgr_request","arguments":"{\"b\":2}"}}]}}]}"#;
    assert_eq!(
      parse_chat_events(chunk, None),
      vec![
        ChatEvent::Delta { text: "Let me look.".into() },
        ChatEvent::ToolCall {
          index: 0,
          id: Some("call_1".into()),
          name: Some("tmgr_request".into()),
          arguments: "{\"a\":1}".into(),
        },
        ChatEvent::ToolCall {
          index: 1,
          id: Some("call_2".into()),
          name: Some("tmgr_request".into()),
          arguments: "{\"b\":2}".into(),
        },
      ]
    );
  }

  #[test]
  fn ignores_a_role_only_delta_and_invalid_json() {
    assert!(parse_chat_events("{\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}", None).is_empty());
    assert!(parse_chat_events("not json", None).is_empty());
  }

  #[test]
  fn builds_the_chat_completions_url_from_any_base() {
    for (base, expected) in [
      ("http://localhost:1234", "http://localhost:1234/v1/chat/completions"),
      ("http://localhost:1234/", "http://localhost:1234/v1/chat/completions"),
      ("http://localhost:1234/v1", "http://localhost:1234/v1/chat/completions"),
      ("http://localhost:1234/v1/", "http://localhost:1234/v1/chat/completions"),
      ("https://host.example/api", "https://host.example/api/v1/chat/completions"),
      ("https://host.example/api/v1", "https://host.example/api/v1/chat/completions"),
    ] {
      assert_eq!(chat_completions_url(base), expected, "{base}");
    }
  }

  #[test]
  fn builds_the_chat_completions_url_from_unusual_base_forms() {
    for (base, expected) in [
      ("  http://localhost:1234/v1  ", "http://localhost:1234/v1/chat/completions"),
      ("http://localhost:1234/v1#frag", "http://localhost:1234/v1/chat/completions"),
      ("http://localhost:1234#frag", "http://localhost:1234/v1/chat/completions"),
      ("http://localhost:1234/V1", "http://localhost:1234/V1/chat/completions"),
      ("http://localhost:1234/v1/chat/completions", "http://localhost:1234/v1/chat/completions"),
      ("http://localhost:1234/v1/chat/completions/", "http://localhost:1234/v1/chat/completions"),
      ("http://localhost:1234/V1/Chat/Completions", "http://localhost:1234/V1/Chat/Completions"),
      ("http://localhost:1234//", "http://localhost:1234/v1/chat/completions"),
      ("http://localhost:1234/v1//", "http://localhost:1234/v1/chat/completions"),
      ("https://h.example/api?key=1", "https://h.example/api/v1/chat/completions?key=1"),
      ("https://h.example/api/v1/?key=1#x", "https://h.example/api/v1/chat/completions?key=1"),
      ("https://h.example/?", "https://h.example/v1/chat/completions"),
      ("localhost:1234", "localhost:1234/v1/chat/completions"),
    ] {
      assert_eq!(chat_completions_url(base), expected, "{base}");
    }
  }

  #[test]
  fn the_request_goes_to_the_origin_the_key_check_validated() {
    for base in [
      "http://localhost:1234",
      "  https://Host.Example:443/v1  ",
      "https://h.example/api?key=1",
      "https://h.example/api/v1/chat/completions",
      "https://h.example#@evil.example",
      "https://h.example/V1#frag",
      "http://user:pw@h.example:80/v1",
      "http://127.0.0.1:8080//",
      "http://[::1]:11434/v1/",
    ] {
      let built = chat_completions_url(base);
      let parsed = Url::parse(&built).unwrap_or_else(|e| panic!("{built}: {e}"));
      assert_eq!(normalize_origin(base).unwrap(), normalize_origin(&built).unwrap(), "{base}");
      assert_eq!(Url::parse(base.trim()).unwrap().host_str(), parsed.host_str(), "{base}");
      assert_eq!(parsed.fragment(), None, "{base}");
    }
  }

  fn push_all(chunks: &[&[u8]]) -> Vec<String> {
    let mut buffer = SseBuffer::default();
    chunks.iter().flat_map(|c| buffer.push(c)).collect()
  }

  #[test]
  fn sse_buffer_splits_events_on_every_line_ending_style() {
    for (name, input, expected) in [
      ("cr only", "data: a\rdata: b\r\rdata: c\r\r", vec!["a\nb", "c"]),
      ("double cr", "data: a\r\rdata: c\r\r", vec!["a", "c"]),
      ("lf crlf", "data: a\n\r\ndata: c\n\r\n", vec!["a", "c"]),
      ("crlf lf", "data: a\r\n\ndata: c\r\n\n", vec!["a", "c"]),
      ("crlf", "data: a\r\n\r\ndata: c\r\n\r\n", vec!["a", "c"]),
    ] {
      assert_eq!(push_all(&[input.as_bytes()]), expected, "{name}");
    }
  }

  #[test]
  fn sse_buffer_cr_only_joins_data_lines_of_one_event() {
    assert_eq!(push_all(&[b"data: a\rdata: b\r\r"]), vec!["a\nb"]);
  }

  #[test]
  fn sse_buffer_handles_a_chunk_split_between_cr_and_lf() {
    assert_eq!(push_all(&[b"data: a\r", b"\n\r", b"\ndata: b\r\n\r\n"]), vec!["a", "b"]);
    assert_eq!(push_all(&[b"data: a\r\n\r", b"\n"]), vec!["a"]);
  }

  #[test]
  fn sse_buffer_cr_at_chunk_end_does_not_swallow_the_next_event() {
    assert_eq!(push_all(&[b"data: a\r\r", b"data: b\r\r"]), vec!["a", "b"]);
  }

  #[test]
  fn parses_a_mid_stream_error_object_with_a_message() {
    assert_eq!(
      parse_chat_events("{\"error\":{\"message\":\"Context length exceeded\",\"type\":\"x\"}}", None),
      vec![ChatEvent::Error { message: "Context length exceeded".into() }]
    );
  }

  #[test]
  fn parses_a_mid_stream_error_given_as_a_string() {
    assert_eq!(
      parse_chat_events("{\"error\":\"model crashed\"}", None),
      vec![ChatEvent::Error { message: "model crashed".into() }]
    );
  }

  #[test]
  fn a_delta_comes_before_an_error_in_the_same_chunk() {
    assert_eq!(
      parse_chat_events("{\"choices\":[{\"delta\":{\"content\":\"Hi\"}}],\"error\":{\"message\":\"boom\"}}", None),
      vec![ChatEvent::Delta { text: "Hi".into() }, ChatEvent::Error { message: "boom".into() }]
    );
  }

  #[test]
  fn an_error_without_a_message_gets_a_generic_one() {
    assert_eq!(
      parse_chat_events("{\"error\":{\"code\":500}}", None),
      vec![ChatEvent::Error { message: "The LLM server returned an error".into() }]
    );
  }

  #[test]
  fn a_null_error_field_is_not_an_error() {
    assert_eq!(
      parse_chat_events("{\"error\":null,\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}", None),
      vec![ChatEvent::Delta { text: "Hi".into() }]
    );
  }

  #[test]
  fn a_long_stream_error_message_is_truncated() {
    let long = "x".repeat(500);
    let events = parse_chat_events(&format!("{{\"error\":\"{long}\"}}"), None);
    let ChatEvent::Error { message } = &events[0] else { panic!("{events:?}") };
    assert!(message.chars().count() <= 201, "{}", message.chars().count());
  }

  #[test]
  fn parses_a_non_streaming_reply_with_content() {
    assert_eq!(
      parse_chat_json_reply("{\"choices\":[{\"message\":{\"role\":\"assistant\",\"content\":\"Hello\"}}]}", None),
      vec![ChatEvent::Delta { text: "Hello".into() }, ChatEvent::Done]
    );
  }

  #[test]
  fn non_streaming_tool_calls_use_the_array_position_as_index() {
    let body = r#"{"choices":[{"message":{"content":null,"tool_calls":[
      {"id":"call_a","type":"function","function":{"name":"tmgr_request","arguments":"{\"a\":1}"}},
      {"id":"call_b","type":"function","function":{"name":"tmgr_request","arguments":"{\"b\":2}"}}]}}]}"#;
    assert_eq!(
      parse_chat_json_reply(body, None),
      vec![
        ChatEvent::ToolCall {
          index: 0,
          id: Some("call_a".into()),
          name: Some("tmgr_request".into()),
          arguments: "{\"a\":1}".into(),
        },
        ChatEvent::ToolCall {
          index: 1,
          id: Some("call_b".into()),
          name: Some("tmgr_request".into()),
          arguments: "{\"b\":2}".into(),
        },
        ChatEvent::Done,
      ]
    );
  }

  #[test]
  fn a_non_streaming_error_body_becomes_an_error_event() {
    assert_eq!(
      parse_chat_json_reply("{\"error\":{\"message\":\"Invalid model\"}}", None),
      vec![ChatEvent::Error { message: "Invalid model".into() }]
    );
    assert_eq!(
      parse_chat_json_reply("{\"error\":\"overloaded\"}", None),
      vec![ChatEvent::Error { message: "overloaded".into() }]
    );
  }

  #[test]
  fn a_non_streaming_reply_without_content_or_tool_calls_is_an_error() {
    for body in ["{\"choices\":[{\"message\":{\"content\":\"\"}}]}", "{\"choices\":[]}", "{}", "not json", ""] {
      assert_eq!(
        parse_chat_json_reply(body, None),
        vec![ChatEvent::Error { message: "The LLM server returned no usable content".into() }],
        "{body}"
      );
    }
  }

  #[test]
  fn an_empty_stream_is_an_error_and_a_useful_one_is_done() {
    assert_eq!(
      end_of_stream_event(false, true),
      ChatEvent::Error { message: "The LLM server returned no usable content".into() }
    );
    assert_eq!(end_of_stream_event(true, true), ChatEvent::Done);
  }

  #[test]
  fn a_stream_that_ends_without_done_or_finish_reason_is_an_error() {
    let truncated = ChatEvent::Error { message: "The LLM server closed the stream before the answer finished".into() };
    assert_eq!(end_of_stream_event(true, false), truncated);
    assert_eq!(end_of_stream_event(false, false), truncated);
  }

  #[test]
  fn stream_finish_reason_reads_the_first_choice() {
    assert_eq!(stream_finish_reason("{\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}").as_deref(), Some("stop"));
    assert_eq!(stream_finish_reason("{\"choices\":[{\"delta\":{},\"finish_reason\":null}]}"), None);
    assert_eq!(stream_finish_reason("{\"choices\":[{\"delta\":{\"content\":\"a\"}}]}"), None);
    assert_eq!(stream_finish_reason("[DONE]"), None);
  }

  #[test]
  fn a_length_or_filter_finish_in_a_stream_is_an_error_after_the_content() {
    let length = parse_chat_events("{\"choices\":[{\"delta\":{\"content\":\"abc\"},\"finish_reason\":\"length\"}]}", None);
    assert_eq!(
      length,
      vec![
        ChatEvent::Delta { text: "abc".into() },
        ChatEvent::Error { message: "The answer was cut off: the model hit its length limit".into() },
      ]
    );
    assert_eq!(
      parse_chat_events("{\"choices\":[{\"delta\":{},\"finish_reason\":\"content_filter\"}]}", None),
      vec![ChatEvent::Error { message: "The answer was blocked by the server's content filter".into() }]
    );
    assert_eq!(
      parse_chat_events("{\"choices\":[{\"finish_reason\":\"length\"}]}", None),
      vec![ChatEvent::Error { message: "The answer was cut off: the model hit its length limit".into() }]
    );
  }

  #[test]
  fn a_stop_or_tool_calls_finish_adds_no_event() {
    for reason in ["stop", "tool_calls", "function_call"] {
      let chunk = format!("{{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"{reason}\"}}]}}");
      assert!(parse_chat_events(&chunk, None).is_empty(), "{reason}");
    }
  }

  #[test]
  fn a_length_or_filter_finish_in_a_json_reply_is_an_error_instead_of_done() {
    assert_eq!(
      parse_chat_json_reply("{\"choices\":[{\"message\":{\"content\":\"abc\"},\"finish_reason\":\"length\"}]}", None),
      vec![
        ChatEvent::Delta { text: "abc".into() },
        ChatEvent::Error { message: "The answer was cut off: the model hit its length limit".into() },
      ]
    );
    assert_eq!(
      parse_chat_json_reply("{\"choices\":[{\"message\":{\"content\":null},\"finish_reason\":\"content_filter\"}]}", None),
      vec![ChatEvent::Error { message: "The answer was blocked by the server's content filter".into() }]
    );
    assert_eq!(
      parse_chat_json_reply("{\"choices\":[{\"message\":{\"content\":\"abc\"},\"finish_reason\":\"stop\"}]}", None),
      vec![ChatEvent::Delta { text: "abc".into() }, ChatEvent::Done]
    );
  }

  #[test]
  fn sse_buffer_finish_returns_an_unterminated_final_event() {
    let mut buffer = SseBuffer::default();
    assert!(buffer.push(b"data: {\"a\":1}\n\ndata: [DONE]").len() == 1);
    assert_eq!(buffer.finish().as_deref(), Some("[DONE]"));
    assert_eq!(buffer.finish(), None);
    let mut crlf = SseBuffer::default();
    assert!(crlf.push(b"data: x\r\n").is_empty());
    assert_eq!(crlf.finish().as_deref(), Some("x"));
    assert_eq!(SseBuffer::default().finish(), None);
  }

  fn test_map() -> &'static InFlightMap {
    Box::leak(Box::default())
  }

  fn entry_count(map: &InFlightMap) -> usize {
    map.lock().unwrap().entries.len()
  }

  fn register<F: std::future::Future<Output = ()> + Send + 'static>(
    map: &'static InFlightMap,
    request_id: &str,
    window: &str,
    work: F,
  ) -> Result<tokio::task::JoinHandle<()>, String> {
    let generation = window_generation(map, window).unwrap();
    register_in_flight(map, request_id.into(), window.into(), generation, work)
  }

  fn sleeper(map: &'static InFlightMap, request_id: &str, window: &str) -> tokio::task::JoinHandle<()> {
    register(map, request_id, window, tokio::time::sleep(Duration::from_secs(60))).unwrap()
  }

  #[tokio::test]
  async fn cancelling_removes_the_entry_and_aborts_the_task() {
    let map = test_map();
    let task = sleeper(map, "r1", "task-1");
    assert_eq!(cancel_in_flight(map, "other").unwrap(), None);
    assert_eq!(entry_count(map), 1);
    assert_eq!(cancel_in_flight(map, "r1").unwrap().as_deref(), Some("task-1"));
    assert_eq!(entry_count(map), 0);
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(cancel_in_flight(map, "r1").unwrap(), None);
  }

  #[tokio::test]
  async fn a_cancelled_task_does_not_remove_a_newer_entry_with_the_same_id() {
    let map = test_map();
    let (started, wait_started) = tokio::sync::oneshot::channel();
    let old = register(map, "r1", "task-1", async move {
      let _ = started.send(());
      tokio::time::sleep(Duration::from_secs(60)).await;
    })
    .unwrap();
    wait_started.await.unwrap();
    cancel_in_flight(map, "r1").unwrap();
    let new = sleeper(map, "r1", "task-1");
    assert!(old.await.unwrap_err().is_cancelled());
    assert_eq!(entry_count(map), 1);
    assert!(map.lock().unwrap().entries.get("r1").unwrap().abort.id() == new.id());
    new.abort();
  }

  #[test]
  fn a_task_never_runs_before_its_entry_is_registered() {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).build().unwrap();
    let map = test_map();
    let task = {
      let _entered = runtime.enter();
      register(map, "r1", "task-1", async move {
        assert!(map.lock().unwrap().entries.contains_key("r1"));
      })
      .unwrap()
    };
    runtime.block_on(task).unwrap();
    assert_eq!(entry_count(map), 0);
  }

  #[tokio::test]
  async fn aborting_a_window_stops_only_its_requests() {
    let map = test_map();
    let first = sleeper(map, "r1", "task-1");
    let second = sleeper(map, "r2", "task-1");
    let other = sleeper(map, "r3", "task-2");
    assert_eq!(abort_window_in(map, "task-1"), 2);
    assert!(first.await.unwrap_err().is_cancelled());
    assert!(second.await.unwrap_err().is_cancelled());
    assert_eq!(map.lock().unwrap().entries.keys().collect::<Vec<_>>(), ["r3"]);
    assert!(!other.is_finished());
    assert_eq!(abort_window_in(map, "task-1"), 0);
    assert_eq!(abort_window_in(map, "main"), 0);
    assert_eq!(entry_count(map), 1);
    other.abort();
  }

  #[tokio::test]
  async fn a_window_aborted_while_the_request_was_set_up_refuses_to_start_it() {
    let map = test_map();
    let stale = window_generation(map, "task-1").unwrap();
    let other = window_generation(map, "task-2").unwrap();
    assert_eq!(abort_window_in(map, "task-1"), 0);
    let (ran, did_run) = tokio::sync::oneshot::channel::<()>();
    let refused = register_in_flight(map, "r1".into(), "task-1".into(), stale, async move {
      let _ = ran.send(());
    });
    assert!(refused.is_err());
    assert!(did_run.await.is_err());
    assert_eq!(entry_count(map), 0);
    let fresh = register(map, "r2", "task-1", async {}).unwrap();
    fresh.await.unwrap();
    let unaffected =
      register_in_flight(map, "r3".into(), "task-2".into(), other, tokio::time::sleep(Duration::from_secs(60))).unwrap();
    assert_eq!(map.lock().unwrap().entries.keys().collect::<Vec<_>>(), ["r3"]);
    unaffected.abort();
  }

  #[test]
  fn only_a_destroyed_window_or_a_started_page_load_aborts() {
    use tauri::webview::PageLoadEvent;
    use tauri::WindowEvent;
    assert!(window_event_aborts(&WindowEvent::Destroyed));
    assert!(!window_event_aborts(&WindowEvent::Focused(false)));
    assert!(!window_event_aborts(&WindowEvent::Focused(true)));
    assert!(!window_event_aborts(&WindowEvent::Resized(tauri::PhysicalSize::new(800, 600))));
    assert!(page_load_aborts(PageLoadEvent::Started));
    assert!(!page_load_aborts(PageLoadEvent::Finished));
  }

  #[test]
  fn error_body_uses_the_server_message_when_it_is_json() {
    assert_eq!(
      sanitize_error_body("401 Unauthorized", "{\"error\":{\"message\":\"Bad key\"}}", None),
      "LLM server returned 401 Unauthorized: Bad key"
    );
    assert_eq!(
      sanitize_error_body("400 Bad Request", "{\"error\":\"nope\"}", None),
      "LLM server returned 400 Bad Request: nope"
    );
  }

  #[test]
  fn error_body_replaces_an_echoed_api_key() {
    let message = sanitize_error_body(
      "401 Unauthorized",
      "{\"error\":{\"message\":\"Incorrect API key provided: sk-secret-123\"}}",
      Some("sk-secret-123"),
    );
    assert!(!message.contains("sk-secret-123"), "{message}");
    assert!(message.contains("[redacted]"), "{message}");
  }

  #[test]
  fn error_body_is_truncated_by_characters_not_bytes() {
    let long = "Ж".repeat(500);
    let message = sanitize_error_body("500 Internal Server Error", &format!("{{\"error\":\"{long}\"}}"), None);
    let shown = message.strip_prefix("LLM server returned 500 Internal Server Error: ").unwrap();
    assert!(shown.chars().count() <= 201, "{}", shown.chars().count());
    assert!(shown.starts_with("ЖЖЖ"));
  }

  #[test]
  fn error_body_never_shows_html_or_plain_text() {
    for body in ["<html><body>Bad gateway</body></html>", "  <!DOCTYPE html><h1>x</h1>", "plain text oops", ""] {
      assert_eq!(
        sanitize_error_body("502 Bad Gateway", body, None),
        "LLM server returned 502 Bad Gateway",
        "{body}"
      );
    }
    assert_eq!(
      sanitize_error_body("502 Bad Gateway", "{\"error\":\"<b>html</b>\"}", None),
      "LLM server returned 502 Bad Gateway"
    );
  }

  const KEY: &str = "sk-secret-key-123456";

  fn error_text(events: &[ChatEvent]) -> &str {
    match events {
      [ChatEvent::Error { message }] => message,
      other => panic!("{other:?}"),
    }
  }

  #[test]
  fn a_key_echoed_in_a_mid_stream_error_is_redacted() {
    let events = parse_chat_events(&format!("{{\"error\":{{\"message\":\"bad key {KEY} given\"}}}}"), Some(KEY));
    assert_eq!(error_text(&events), "bad key [redacted] given");
  }

  #[test]
  fn a_key_echoed_in_a_200_json_reply_is_redacted() {
    let events = parse_chat_json_reply(&format!("{{\"error\":\"invalid {KEY}\"}}"), Some(KEY));
    assert_eq!(error_text(&events), "invalid [redacted]");
  }

  #[test]
  fn a_key_straddling_the_truncation_cut_does_not_survive() {
    let message = format!("{}{KEY}", "x".repeat(190));
    let streamed = parse_chat_events(&format!("{{\"error\":\"{message}\"}}"), Some(KEY));
    let replied = parse_chat_json_reply(&format!("{{\"error\":\"{message}\"}}"), Some(KEY));
    for events in [streamed, replied] {
      let text = error_text(&events);
      assert!(!text.contains("sk-secret"), "{text}");
      assert!(text.contains("[redacted]"), "{text}");
    }
  }

  #[test]
  fn an_html_error_message_gets_the_generic_text() {
    let streamed = parse_chat_events("{\"error\":\"<html>nope</html>\"}", None);
    let replied = parse_chat_json_reply("{\"error\":{\"message\":\" <html>nope</html>\"}}", None);
    assert_eq!(error_text(&streamed), "The LLM server returned an error");
    assert_eq!(error_text(&replied), "The LLM server returned an error");
  }

  #[test]
  fn falsy_error_values_do_not_kill_a_healthy_stream() {
    for falsy in ["\"\"", "false", "{}", "null", "\"  \""] {
      assert_eq!(
        parse_chat_events(&format!("{{\"error\":{falsy},\"choices\":[{{\"delta\":{{\"content\":\"Hi\"}}}}]}}"), None),
        vec![ChatEvent::Delta { text: "Hi".into() }],
        "{falsy}"
      );
    }
    let reply = "{\"error\":false,\"choices\":[{\"message\":{\"content\":\"Hi\"}}]}";
    assert_eq!(parse_chat_json_reply(reply, None), vec![ChatEvent::Delta { text: "Hi".into() }, ChatEvent::Done]);
  }

  #[test]
  fn truthy_error_values_stay_errors() {
    for truthy in ["{\"code\":500}", "\"boom\"", "{\"message\":\"boom\"}", "true"] {
      let events = parse_chat_events(&format!("{{\"error\":{truthy}}}"), None);
      assert!(matches!(events.as_slice(), [ChatEvent::Error { .. }]), "{truthy}: {events:?}");
    }
  }

  #[test]
  fn data_lines_under_a_json_content_type_are_parsed_as_a_stream() {
    let body = "data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n\ndata: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}]}\n\ndata: [DONE]\n\n";
    assert_eq!(
      parse_sse_text(body, None),
      vec![ChatEvent::Delta { text: "Hel".into() }, ChatEvent::Delta { text: "lo".into() }, ChatEvent::Done]
    );
  }

  const TRUNCATED: &str = "The LLM server closed the stream before the answer finished";

  #[test]
  fn a_data_body_without_a_trailing_blank_line_is_still_parsed() {
    assert_eq!(
      parse_sse_text("data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}", None),
      vec![ChatEvent::Delta { text: "Hi".into() }, ChatEvent::Error { message: TRUNCATED.into() }]
    );
    assert_eq!(
      parse_sse_text("data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"},\"finish_reason\":\"stop\"}]}", None),
      vec![ChatEvent::Delta { text: "Hi".into() }, ChatEvent::Done]
    );
  }

  #[test]
  fn a_data_body_with_no_usable_content_is_an_error_and_redacts_the_key() {
    assert_eq!(
      parse_sse_text("data: {\"choices\":[{\"delta\":{}}]}\n\n", None),
      vec![ChatEvent::Error { message: TRUNCATED.into() }]
    );
    assert_eq!(
      parse_sse_text("data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n", None),
      vec![ChatEvent::Error { message: NO_USABLE_CONTENT.into() }]
    );
    let events = parse_sse_text(&format!("data: {{\"error\":\"{KEY}\"}}\n\n"), Some(KEY));
    assert_eq!(error_text(&events), "[redacted]");
  }

  async fn serve_once(listener: tokio::net::TcpListener, reply: String, received: std::sync::Arc<Mutex<Vec<u8>>>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    while let Ok((mut socket, _)) = listener.accept().await {
      let mut buf = [0u8; 4096];
      let mut seen = Vec::new();
      while let Ok(Ok(n)) = tokio::time::timeout(Duration::from_millis(100), socket.read(&mut buf)).await {
        if n == 0 {
          break;
        }
        seen.extend_from_slice(&buf[..n]);
        if seen.windows(4).any(|w| w == b"\r\n\r\n") && seen.ends_with(b"}") {
          break;
        }
      }
      received.lock().unwrap().extend_from_slice(&seen);
      let _ = socket.write_all(reply.as_bytes()).await;
    }
  }

  #[tokio::test]
  async fn llm_requests_do_not_follow_redirects() {
    use std::sync::Arc;
    let target = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let target_port = target.local_addr().unwrap().port();
    let target_seen = Arc::new(Mutex::new(Vec::new()));
    tokio::spawn(serve_once(
      target,
      "HTTP/1.1 200 OK\r\ncontent-length: 0\r\n\r\n".into(),
      target_seen.clone(),
    ));
    let origin = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin_port = origin.local_addr().unwrap().port();
    tokio::spawn(serve_once(
      origin,
      format!("HTTP/1.1 307 Temporary Redirect\r\nlocation: http://localhost:{target_port}/steal?x=1\r\ncontent-length: 0\r\n\r\n"),
      Arc::new(Mutex::new(Vec::new())),
    ));
    let client = llm_http_client("http://127.0.0.1").unwrap();
    let url = format!("http://127.0.0.1:{origin_port}/v1/chat/completions");
    let result = post_chat(&client, &url, br#"{"secret":"payload"}"#.to_vec(), Some("sk-test")).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
      result.err().as_deref(),
      Some(format!("The LLM server redirected the request to http://localhost:{target_port}. Update the base URL in Settings.").as_str())
    );
    assert!(target_seen.lock().unwrap().is_empty());
  }

  #[tokio::test]
  async fn an_unreachable_server_message_has_no_query() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let client = llm_http_client("http://127.0.0.1").unwrap();
    let url = format!("http://127.0.0.1:{port}/v1/chat/completions?key=QUERYSECRET");
    let message = post_chat(&client, &url, b"{}".to_vec(), None).await.err().unwrap();
    assert!(message.starts_with("LLM server unreachable"), "{message}");
    assert!(!message.contains("QUERYSECRET"), "{message}");
  }

  fn redirect_text(location: Option<&str>, key: Option<&str>) -> String {
    redirect_error(location, key)
  }

  #[test]
  fn a_redirect_names_only_the_target_origin() {
    assert_eq!(
      redirect_text(Some("https://evil.example:8443/steal?x=1#f"), None),
      "The LLM server redirected the request to https://evil.example:8443. Update the base URL in Settings."
    );
    assert!(redirect_text(Some("http://evil.example/a/b?token=1"), None).contains("to http://evil.example."));
  }

  #[test]
  fn a_relative_or_missing_redirect_target_is_another_address() {
    for location in [Some("/other/path?x=1"), Some(""), Some("mailto:a@b.c"), None] {
      assert_eq!(
        redirect_text(location, None),
        "The LLM server redirected the request to another address. Update the base URL in Settings.",
        "{location:?}"
      );
    }
  }

  #[test]
  fn a_redirect_target_containing_the_key_or_too_long_is_cut_down() {
    let leaky = format!("https://{}.example.com/x", KEY.to_uppercase());
    assert!(redirect_text(Some(&leaky), Some(KEY)).contains("to another address."));
    let long = format!("https://{}.example.com/x", "a".repeat(300));
    let text = redirect_text(Some(&long), None);
    assert!(text.contains('…') && text.len() < 400, "{text}");
  }

  #[test]
  fn finish_reasons_are_matched_case_insensitively() {
    let length = "The answer was cut off: the model hit its length limit";
    for reason in ["length", "LENGTH", "max_tokens", "Max_Tokens", "model_length"] {
      let data = format!("{{\"choices\":[{{\"delta\":{{}},\"finish_reason\":\"{reason}\"}}]}}");
      assert_eq!(error_text(&parse_chat_events(&data, None)), length, "{reason}");
    }
    let data = "{\"choices\":[{\"delta\":{},\"finish_reason\":\"Content_Filter\"}]}";
    assert_eq!(error_text(&parse_chat_events(data, None)), "The answer was blocked by the server's content filter");
    let data = "{\"choices\":[{\"delta\":{},\"finish_reason\":\"error\"}]}";
    assert_eq!(error_text(&parse_chat_events(data, None)), "The LLM server ended the answer with an error");
    assert!(parse_chat_events("{\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}]}", None).is_empty());
  }

  #[test]
  fn a_server_error_wins_over_a_finish_error_with_or_without_a_delta() {
    let with_delta = "{\"error\":\"boom\",\"choices\":[{\"delta\":{\"content\":\"Hi\"},\"finish_reason\":\"length\"}]}";
    assert_eq!(
      parse_chat_events(with_delta, None),
      vec![ChatEvent::Delta { text: "Hi".into() }, ChatEvent::Error { message: "boom".into() }]
    );
    let without = "{\"error\":\"boom\",\"choices\":[{\"finish_reason\":\"length\"}]}";
    assert_eq!(parse_chat_events(without, None), vec![ChatEvent::Error { message: "boom".into() }]);
  }

  async fn pump(reply: String) -> (Result<(), String>, Vec<ChatEvent>) {
    pump_with_key(reply, None).await
  }

  async fn pump_with_key(reply: String, key: Option<&str>) -> (Result<(), String>, Vec<ChatEvent>) {
    use std::sync::Arc;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();
    tokio::spawn(serve_once(listener, reply, Arc::new(Mutex::new(Vec::new()))));
    let client = llm_http_client("http://127.0.0.1").unwrap();
    let url = format!("http://127.0.0.1:{port}/v1/chat/completions");
    let mut response = post_chat(&client, &url, b"{}".to_vec(), None).await.unwrap();
    let mut events = Vec::new();
    let result = pump_stream(&mut response, key, |e| events.push(e)).await;
    (result, events)
  }

  fn sse_reply(body: &str) -> String {
    format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n{body}")
  }

  fn chunk(delta: &str, finish: &str) -> String {
    format!("data: {{\"choices\":[{{\"delta\":{delta},\"finish_reason\":{finish}}}]}}\n\n")
  }

  #[tokio::test]
  async fn an_unterminated_done_marker_then_close_is_done() {
    let (result, events) = pump(sse_reply("data: [DONE]")).await;
    assert_eq!((result, events), (Ok(()), vec![ChatEvent::Done]));
  }

  #[tokio::test]
  async fn content_then_close_is_the_truncation_error() {
    let (result, events) = pump(sse_reply(&chunk("{\"content\":\"Hi\"}", "null"))).await;
    assert_eq!(result, Ok(()));
    assert_eq!(events, vec![ChatEvent::Delta { text: "Hi".into() }, ChatEvent::Error { message: TRUNCATED.into() }]);
  }

  #[tokio::test]
  async fn content_then_an_empty_stop_chunk_then_close_is_done() {
    let body = format!("{}{}", chunk("{\"content\":\"Hi\"}", "null"), chunk("{}", "\"stop\""));
    let (result, events) = pump(sse_reply(&body)).await;
    assert_eq!(result, Ok(()));
    assert_eq!(events, vec![ChatEvent::Delta { text: "Hi".into() }, ChatEvent::Done]);
  }

  #[tokio::test]
  async fn a_tool_call_stream_ending_with_tool_calls_is_tool_calls_then_done() {
    let call = "{\"tool_calls\":[{\"index\":0,\"id\":\"c1\",\"function\":{\"name\":\"f\",\"arguments\":\"{}\"}}]}";
    let body = format!("{}{}", chunk(call, "null"), chunk("{}", "\"tool_calls\""));
    let (result, events) = pump(sse_reply(&body)).await;
    assert_eq!(result, Ok(()));
    assert_eq!(
      events,
      vec![
        ChatEvent::ToolCall { index: 0, id: Some("c1".into()), name: Some("f".into()), arguments: "{}".into() },
        ChatEvent::Done
      ]
    );
  }

  #[tokio::test]
  async fn a_json_body_under_an_event_stream_type_is_no_usable_content() {
    let (result, events) = pump(sse_reply("{\"choices\":[]}")).await;
    assert_eq!(result, Ok(()));
    assert_eq!(events, vec![ChatEvent::Error { message: NO_USABLE_CONTENT.into() }]);
  }

  #[tokio::test]
  async fn a_transport_failure_after_the_finish_reason_is_done() {
    let body = format!("{}{}", chunk("{\"content\":\"Hi\"}", "null"), chunk("{}", "\"stop\""));
    let reply = format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\n\r\n{body}", body.len() + 100);
    let (result, events) = pump(reply).await;
    assert_eq!(result, Ok(()));
    assert_eq!(events, vec![ChatEvent::Delta { text: "Hi".into() }, ChatEvent::Done]);
  }

  #[tokio::test]
  async fn a_transport_failure_before_the_finish_reason_stays_an_error() {
    let body = chunk("{\"content\":\"Hi\"}", "null");
    let reply = format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\n\r\n{body}", body.len() + 100);
    let (result, events) = pump(reply).await;
    assert!(result.is_err());
    assert_eq!(events, vec![ChatEvent::Delta { text: "Hi".into() }]);
  }

  #[tokio::test]
  async fn an_unbounded_event_is_rejected_as_too_large() {
    let body = format!("data: {}", "x".repeat(JSON_BODY_CAP + 1024));
    let (result, _) = pump(sse_reply(&body)).await;
    assert_eq!(result, Err("The LLM server reply is too large".into()));
  }

  #[test]
  fn the_sse_buffer_drops_a_pending_event_over_the_cap() {
    let mut buffer = SseBuffer::default();
    assert!(buffer.push(&vec![b'x'; JSON_BODY_CAP]).is_empty());
    assert!(!buffer.overflowed());
    buffer.push(b"x");
    assert!(buffer.overflowed());
    assert_eq!(buffer.finish(), None);
  }

  const FAKE: &str = "sk-test-FAKEKEY123";
  /// Percent-encodes differently from the raw form, so the encoded variant is really exercised.
  const FAKE_ENCODABLE: &str = "sk-test/FAKE+KEY=123";
  const FAKE_ENCODED_LOWER_HEX: &str = "sk-test%2ffake%2bkey%3d123";
  const GENERIC: &str = "The LLM server returned an error";

  fn redact_stream(key: Option<&str>, events: Vec<ChatEvent>) -> Vec<ChatEvent> {
    let mut redactor = StreamRedactor::new(key);
    let mut out = Vec::new();
    for event in events {
      out.extend(redactor.push(event));
    }
    out.extend(redactor.finish());
    out
  }

  fn joined_text(events: &[ChatEvent]) -> String {
    events.iter().filter_map(|e| if let ChatEvent::Delta { text } = e { Some(text.as_str()) } else { None }).collect()
  }

  fn joined_args(events: &[ChatEvent], at: u32) -> String {
    events
      .iter()
      .filter_map(|e| match e {
        ChatEvent::ToolCall { index, arguments, .. } if *index == at => Some(arguments.as_str()),
        _ => None,
      })
      .collect()
  }

  fn delta(text: &str) -> ChatEvent {
    ChatEvent::Delta { text: text.into() }
  }

  fn args(index: u32, id: Option<&str>, arguments: &str) -> ChatEvent {
    ChatEvent::ToolCall { index, id: id.map(str::to_string), name: id.map(|_| "f".to_string()), arguments: arguments.into() }
  }

  fn content_chunk(text: &str) -> String {
    chunk(&serde_json::json!({ "content": text }).to_string(), "null")
  }

  #[test]
  fn a_key_straddling_the_redirect_cut_is_never_shown() {
    let host = format!("{}{}", "a".repeat(190), FAKE.to_ascii_lowercase());
    let location = format!("https://{host}.example.com/x");
    let text = redirect_text(Some(&location), Some(FAKE));
    assert!(text.contains("to another address."), "{text}");
    assert!(!text.contains("sk-te"), "{text}");
    let control = redirect_text(Some(&location), None);
    assert!(control.contains('…') && !control.contains("another address"), "{control}");
  }

  #[test]
  fn key_matching_ignores_ascii_case_and_stored_whitespace() {
    let padded = format!("  {FAKE}\n");
    for shown in [FAKE.to_uppercase(), FAKE.to_lowercase()] {
      let body = format!("{{\"error\":{{\"message\":\"bad key {shown} given\"}}}}");
      assert_eq!(sanitize_error_body("401 Unauthorized", &body, Some(&padded)), "LLM server returned 401 Unauthorized: bad key [redacted] given");
      let events = parse_chat_events(&format!("{{\"error\":\"bad key {shown}\"}}"), Some(&padded));
      assert_eq!(error_text(&events), "bad key [redacted]");
    }
  }

  #[test]
  fn a_percent_encoded_key_is_redacted_in_error_bodies_and_server_messages() {
    let body = format!("{{\"error\":\"GET /v1?key={FAKE_ENCODED_LOWER_HEX} denied, raw {FAKE_ENCODABLE}\"}}");
    let message = sanitize_error_body("403 Forbidden", &body, Some(FAKE_ENCODABLE));
    assert_eq!(message, "LLM server returned 403 Forbidden: GET /v1?key=[redacted] denied, raw [redacted]");
    let events = parse_chat_events(&format!("{{\"error\":{{\"message\":\"key={}\"}}}}", FAKE_ENCODED_LOWER_HEX.to_uppercase()), Some(FAKE_ENCODABLE));
    assert_eq!(error_text(&events), "key=[redacted]");
  }

  #[test]
  fn a_short_key_in_an_error_text_gives_the_generic_message() {
    let short = "olla1";
    assert_eq!(error_text(&parse_chat_events("{\"error\":\"bad key OLLA1 here\"}", Some(short))), GENERIC);
    assert_eq!(sanitize_error_body("401 Unauthorized", "{\"error\":\"bad key olla1\"}", Some(short)), "LLM server returned 401 Unauthorized");
    assert!(redirect_text(Some("https://olla1.example.com"), Some(short)).contains("to another address."));
  }

  #[test]
  fn a_short_key_leaves_other_texts_and_model_text_unchanged() {
    let short = "olla1";
    assert_eq!(error_text(&parse_chat_events("{\"error\":\"model not found\"}", Some(short))), "model not found");
    assert_eq!(sanitize_error_body("404 Not Found", "{\"error\":\"nope\"}", Some(short)), "LLM server returned 404 Not Found: nope");
    let events = vec![delta("say olla1"), args(0, Some("c1"), "{\"v\":\"olla1\"}"), ChatEvent::Done];
    assert_eq!(redact_stream(Some(short), events.clone()), events);
    let reply = "{\"choices\":[{\"message\":{\"content\":\"olla1\"}}]}";
    assert_eq!(parse_chat_json_reply(reply, Some(short)), vec![delta("olla1"), ChatEvent::Done]);
  }

  #[test]
  fn a_key_split_across_two_deltas_is_redacted() {
    let out = redact_stream(Some(FAKE), vec![delta("hello sk-test-FA"), delta("KEkey123 world"), ChatEvent::Done]);
    assert_eq!(joined_text(&out), "hello [redacted] world");
    assert_eq!(out.last(), Some(&ChatEvent::Done));
  }

  #[test]
  fn a_key_split_across_two_tool_argument_chunks_is_redacted() {
    let out = redact_stream(
      Some(FAKE),
      vec![args(0, Some("c1"), "{\"k\":\"sk-te"), args(0, None, "st-FAKEKEY123\"}"), args(1, Some("c2"), "{}"), ChatEvent::Done],
    );
    assert_eq!(joined_args(&out, 0), "{\"k\":\"[redacted]\"}");
    assert_eq!(joined_args(&out, 1), "{}");
    assert_eq!(out[0], args(0, Some("c1"), ""));
    let flushed = &out[out.len() - 3..];
    assert!(matches!(flushed[0], ChatEvent::ToolCall { index: 0, id: None, name: None, .. }), "{out:?}");
    assert!(matches!(flushed[1], ChatEvent::ToolCall { index: 1, id: None, name: None, .. }), "{out:?}");
    assert_eq!(flushed[2], ChatEvent::Done);
  }

  #[test]
  fn a_new_call_at_the_same_index_takes_no_tail_of_the_previous_one() {
    let out = redact_stream(Some(FAKE), vec![args(0, Some("c1"), "{\"a\":1}"), args(0, Some("c2"), "{\"b\":2}"), ChatEvent::Done]);
    assert_eq!(
      out,
      vec![
        args(0, Some("c1"), ""),
        ChatEvent::ToolCall { index: 0, id: None, name: None, arguments: "{\"a\":1}".into() },
        args(0, Some("c2"), ""),
        ChatEvent::ToolCall { index: 0, id: None, name: None, arguments: "{\"b\":2}".into() },
        ChatEvent::Done,
      ]
    );
  }

  #[test]
  fn a_repeated_id_does_not_flush_the_held_tail() {
    let out = redact_stream(Some(FAKE), vec![args(0, Some("c1"), "{\"k\":\"sk-test-"), args(0, Some("c1"), "FAKEKEY123\"}"), ChatEvent::Done]);
    assert_eq!(joined_args(&out, 0), "{\"k\":\"[redacted]\"}");
  }

  #[test]
  fn a_normal_stream_comes_out_byte_identical() {
    let text = "Привет, мир! 你好 🌍 — sk-test-almost but not the key; ".repeat(5);
    let chars: Vec<char> = text.chars().collect();
    let mut events: Vec<ChatEvent> = chars.chunks(3).map(|c| delta(&c.iter().collect::<String>())).collect();
    let arguments = "{\"q\":\"ключ 🗝 sk-test-FAKEKEY12\"}";
    events.push(args(0, Some("c1"), &arguments[..5]));
    events.push(args(0, None, &arguments[5..]));
    events.push(ChatEvent::Done);
    let out = redact_stream(Some(FAKE), events);
    assert_eq!(joined_text(&out), text);
    assert_eq!(joined_args(&out, 0), arguments);
    assert_eq!(out.last(), Some(&ChatEvent::Done));
  }

  #[tokio::test]
  async fn the_held_tail_is_flushed_before_done() {
    let body = format!("{}{}data: [DONE]\n\n", content_chunk("a sk-test-FAKE"), content_chunk("KEY123 b"));
    let (result, events) = pump_with_key(sse_reply(&body), Some(FAKE)).await;
    assert_eq!(result, Ok(()));
    assert_eq!(joined_text(&events), "a [redacted] b");
    assert_eq!(events.last(), Some(&ChatEvent::Done));
  }

  #[tokio::test]
  async fn the_held_tail_is_flushed_on_a_finish_reason_then_close() {
    let body = format!("{}{}{}", content_chunk("a sk-"), content_chunk("test-FAKEKEY123 b"), chunk("{}", "\"stop\""));
    let (result, events) = pump_with_key(sse_reply(&body), Some(FAKE)).await;
    assert_eq!(result, Ok(()));
    assert_eq!(joined_text(&events), "a [redacted] b");
    assert_eq!(events.last(), Some(&ChatEvent::Done));
  }

  #[tokio::test]
  async fn the_held_tail_is_flushed_before_a_mid_stream_error() {
    let body = format!("{}{}data: {{\"error\":\"boom\"}}\n\n", content_chunk("a sk-test-"), content_chunk("FAKEKEY123 b"));
    let (result, events) = pump_with_key(sse_reply(&body), Some(FAKE)).await;
    assert_eq!(result, Ok(()));
    assert_eq!(joined_text(&events), "a [redacted] b");
    assert_eq!(events.last(), Some(&ChatEvent::Error { message: "boom".into() }));
  }

  #[tokio::test]
  async fn the_held_tail_is_flushed_when_the_transport_fails() {
    let body = content_chunk("Hi there");
    let reply = format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\n\r\n{body}", body.len() + 100);
    let (result, events) = pump_with_key(reply, Some(FAKE)).await;
    assert!(result.is_err());
    assert_eq!(events, vec![delta("Hi there")]);
  }

  #[test]
  fn sse_in_json_redacts_a_key_split_across_deltas() {
    let body = format!("{}{}data: [DONE]\n\n", content_chunk("a sk-test-FA"), content_chunk("KEKEY123 b"));
    let events = parse_sse_text(&body, Some(FAKE));
    assert_eq!(joined_text(&events), "a [redacted] b");
    assert_eq!(events.last(), Some(&ChatEvent::Done));
  }

  #[test]
  fn a_json_reply_redacts_content_and_tool_arguments() {
    let reply = serde_json::json!({
      "choices": [{ "message": {
        "content": format!("key {FAKE}"),
        "tool_calls": [{ "id": "c1", "function": { "name": "f", "arguments": format!("{{\"k\":\"{}\"}}", FAKE.to_uppercase()) } }]
      }}]
    })
    .to_string();
    assert_eq!(
      parse_chat_json_reply(&reply, Some(FAKE)),
      vec![
        delta("key [redacted]"),
        ChatEvent::ToolCall { index: 0, id: Some("c1".into()), name: Some("f".into()), arguments: "{\"k\":\"[redacted]\"}".into() },
        ChatEvent::Done,
      ]
    );
  }

  #[test]
  fn present_error_matrix() {
    for falsy in ["0", "0.0", "[]", "{}", "null", "false", "\"\""] {
      let value: Value = serde_json::from_str(&format!("{{\"error\":{falsy}}}")).unwrap();
      assert!(present_error(&value).is_none(), "{falsy}");
      assert_eq!(
        parse_chat_events(&format!("{{\"error\":{falsy},\"choices\":[{{\"delta\":{{\"content\":\"Hi\"}}}}]}}"), None),
        vec![delta("Hi")],
        "{falsy}"
      );
    }
    for truthy in ["1", "-2", "0.5", "[1]", "[\"x\"]", "\"x\"", "{\"a\":1}", "true"] {
      let value: Value = serde_json::from_str(&format!("{{\"error\":{truthy}}}")).unwrap();
      assert!(present_error(&value).is_some(), "{truthy}");
    }
  }

  #[test]
  fn only_local_urls_bypass_env_proxies() {
    let lan = format!("http://{}.{}.{}.{}:8000", 192, 168, 1, 5);
    for url in ["http://localhost:11434", "http://127.0.0.1:8000/v1", lan.as_str(), "http://[::1]:8000", "http://box.local"] {
      assert!(bypasses_env_proxy(url), "{url}");
    }
    for url in ["https://api.openai.com/v1", "http://8.8.8.8", "", "not a url"] {
      assert!(!bypasses_env_proxy(url), "{url}");
    }
  }

  #[test]
  fn no_key_or_an_empty_key_leaves_texts_unchanged() {
    for key in [None, Some(""), Some("   ")] {
      assert_eq!(error_text(&parse_chat_events("{\"error\":\"bad key given\"}", key)), "bad key given");
      assert_eq!(sanitize_error_body("401 Unauthorized", "{\"error\":\"nope\"}", key), "LLM server returned 401 Unauthorized: nope");
      assert_eq!(error_text(&parse_chat_events("{\"error\":{\"code\":1}}", key)), GENERIC);
      assert!(redirect_text(Some("https://evil.example"), key).contains("to https://evil.example."));
      let events = vec![delta("hi"), args(0, Some("c1"), "{}"), ChatEvent::Done];
      assert_eq!(redact_stream(key, events.clone()), events);
    }
  }

  /// Mirrors `src/local/llmClient.ts`: a new buffer per index when a different non-empty id arrives, id-less appends.
  fn assemble_like_ts(events: &[ChatEvent]) -> Vec<(String, String)> {
    let mut buffers: Vec<(String, String)> = Vec::new();
    let mut current: HashMap<u32, usize> = HashMap::new();
    for event in events {
      if let ChatEvent::ToolCall { index, id, arguments, .. } = event {
        let id = id.clone().unwrap_or_default();
        let open = current.get(index).copied();
        let at = match open {
          Some(at) if id.is_empty() || buffers[at].0.is_empty() || buffers[at].0 == id => at,
          _ => {
            buffers.push((String::new(), String::new()));
            current.insert(*index, buffers.len() - 1);
            buffers.len() - 1
          }
        };
        if !id.is_empty() {
          buffers[at].0 = id;
        }
        buffers[at].1.push_str(arguments);
      }
    }
    buffers
  }

  fn char_splits(text: &str) -> Vec<(String, String, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    for i in 0..=chars.len() {
      for j in i..=chars.len() {
        out.push((chars[..i].iter().collect(), chars[i..j].iter().collect(), chars[j..].iter().collect()));
      }
    }
    out
  }

  #[test]
  fn qa_a_key_split_at_every_two_and_three_way_position_never_leaks_from_deltas() {
    let mixed: String = FAKE.chars().enumerate().map(|(i, c)| if i % 2 == 0 { c.to_ascii_uppercase() } else { c }).collect();
    let cases = [
      (FAKE, FAKE.to_string()),
      (FAKE, FAKE.to_uppercase()),
      (FAKE, mixed),
      (FAKE_ENCODABLE, FAKE_ENCODABLE.to_string()),
      (FAKE_ENCODABLE, FAKE_ENCODED_LOWER_HEX.to_string()),
      (FAKE_ENCODABLE, FAKE_ENCODED_LOWER_HEX.to_uppercase()),
    ];
    for (key, shown) in cases {
      let text = format!("й{shown}🌍 ок");
      for (a, b, c) in char_splits(&text) {
        let out = redact_stream(Some(key), vec![delta(&a), delta(&b), delta(&c), ChatEvent::Done]);
        assert_eq!(joined_text(&out), "й[redacted]🌍 ок", "{key} {a:?}|{b:?}|{c:?}");
        assert_eq!(out.last(), Some(&ChatEvent::Done));
        for event in &out {
          if let ChatEvent::Delta { text } = event {
            assert!(!text.to_ascii_lowercase().contains(&shown.to_ascii_lowercase()), "{event:?}");
          }
        }
      }
    }
  }

  #[test]
  fn qa_a_key_split_at_every_position_never_leaks_from_tool_arguments() {
    for (key, shown) in [(FAKE, FAKE.to_uppercase()), (FAKE_ENCODABLE, FAKE_ENCODED_LOWER_HEX.to_uppercase())] {
      let arguments = format!("{{\"body\":\"ключ {shown}🗝\"}}");
      for (a, b, c) in char_splits(&arguments) {
        let events = vec![
          args(0, Some("c1"), &a),
          args(0, None, &b),
          args(1, Some("c2"), "{}"),
          args(0, None, &c),
          ChatEvent::Done,
        ];
        let out = redact_stream(Some(key), events);
        let calls = assemble_like_ts(&out);
        assert_eq!(
          calls,
          vec![("c1".to_string(), "{\"body\":\"ключ [redacted]🗝\"}".to_string()), ("c2".to_string(), "{}".to_string())],
          "{a:?}|{b:?}|{c:?}"
        );
        assert_eq!(out.last(), Some(&ChatEvent::Done));
      }
    }
  }

  #[test]
  fn qa_redaction_keeps_the_ts_assembly_of_calls_identical_when_no_key_is_present() {
    let events = vec![
      args(0, Some("c1"), "{\"method\":\"GET\","),
      args(0, None, "\"path\":\"tasks/1\"}"),
      args(1, Some("c2"), "{\"method\":"),
      args(0, Some("c3"), "{\"method\":\"POST\",\"path\":\"tasks/1/comments\",\"body\":{\"text\":\"Привет sk-test-FAKEKEY12"),
      args(1, None, "\"GET\",\"path\":\"tasks/2\"}"),
      args(0, Some("c3"), "\"}}"),
      ChatEvent::Done,
    ];
    let out = redact_stream(Some(FAKE), events.clone());
    assert_eq!(assemble_like_ts(&out), assemble_like_ts(&events));
  }

  #[test]
  fn an_empty_id_continues_the_open_call_like_the_ts_client() {
    let out = redact_stream(
      Some(FAKE),
      vec![args(0, Some("c1"), "{\"k\":\"sk-test-"), args(0, Some(""), "FAKEKEY123\"}"), ChatEvent::Done],
    );
    let calls = assemble_like_ts(&out);
    assert_eq!(calls, vec![("c1".into(), "{\"k\":\"[redacted]\"}".into())]);
  }

  #[test]
  fn a_late_first_id_keeps_the_key_carry_of_the_same_call() {
    let out = redact_stream(
      Some(FAKE),
      vec![args(0, None, "{\"k\":\"sk-test-FA"), args(0, Some("c1"), "KEKEY123\"}"), ChatEvent::Done],
    );
    let calls = assemble_like_ts(&out);
    assert_eq!(calls, vec![("c1".into(), "{\"k\":\"[redacted]\"}".into())]);
  }

  #[test]
  fn id_less_calls_keep_their_order_when_the_first_chunk_is_held_back() {
    let events = vec![
      args(0, None, "{}"),
      args(1, None, "{\"method\":\"GET\",\"path\":\"tasks/2\"}"),
      ChatEvent::Done,
    ];
    let out = redact_stream(Some(FAKE), events.clone());
    assert_eq!(assemble_like_ts(&out), assemble_like_ts(&events));
  }

  #[test]
  fn qa_an_id_change_at_the_same_index_mid_key_puts_no_key_in_either_call() {
    let out = redact_stream(
      Some(FAKE),
      vec![args(0, Some("c1"), "{\"k\":\"sk-test-FA"), args(0, Some("c2"), "KEKEY123\"}"), ChatEvent::Done],
    );
    let calls = assemble_like_ts(&out);
    assert_eq!(calls, vec![("c1".into(), "{\"k\":\"sk-test-FA".into()), ("c2".into(), "KEKEY123\"}".into())]);
  }

  #[test]
  fn qa_both_calls_at_one_index_are_redacted_when_each_carries_the_key() {
    let out = redact_stream(
      Some(FAKE),
      vec![args(0, Some("c1"), &format!("{{\"k\":\"{FAKE}")), args(0, Some("c2"), &format!("{FAKE}\"}}")), ChatEvent::Done],
    );
    assert_eq!(assemble_like_ts(&out), vec![("c1".into(), "{\"k\":\"[redacted]".into()), ("c2".into(), "[redacted]\"}".into())]);
  }

  #[tokio::test]
  async fn qa_a_whole_key_held_when_the_transport_fails_is_flushed_redacted() {
    let body = content_chunk(&format!("a {FAKE}"));
    let reply = format!("HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\n\r\n{body}", body.len() + 100);
    let (result, events) = pump_with_key(reply, Some(FAKE)).await;
    assert!(result.is_err());
    assert_eq!(events, vec![delta("a [redacted]")]);
  }

  #[tokio::test]
  async fn qa_a_key_in_the_chunk_before_done_and_in_tool_arguments_is_redacted_in_the_pump() {
    let call = serde_json::json!({ "tool_calls": [{ "index": 0, "id": "c1", "function": { "name": "f", "arguments": format!("{{\"k\":\"{}", &FAKE[..9]) } }] });
    let tail = serde_json::json!({ "tool_calls": [{ "index": 0, "function": { "arguments": format!("{}\"}}", &FAKE[9..]) } }] });
    let body = format!(
      "{}{}{}data: [DONE]\n\n",
      content_chunk(&format!("x {}", FAKE.to_uppercase())),
      chunk(&call.to_string(), "null"),
      chunk(&tail.to_string(), "\"tool_calls\"")
    );
    let (result, events) = pump_with_key(sse_reply(&body), Some(FAKE)).await;
    assert_eq!(result, Ok(()));
    assert_eq!(joined_text(&events), "x [redacted]");
    assert_eq!(assemble_like_ts(&events), vec![("c1".into(), "{\"k\":\"[redacted]\"}".into())]);
    assert_eq!(events.last(), Some(&ChatEvent::Done));
  }

  #[test]
  fn qa_sse_in_json_redacts_a_split_key_before_a_mid_stream_error_that_echoes_it() {
    let body = format!("{}{}data: {{\"error\":\"bad {FAKE}\"}}\n\n", content_chunk("a sk-test-FAKE"), content_chunk("KEY123 b"));
    let events = parse_sse_text(&body, Some(FAKE));
    assert_eq!(joined_text(&events), "a [redacted] b");
    assert_eq!(events.last(), Some(&ChatEvent::Error { message: "bad [redacted]".into() }));
  }

  #[test]
  fn qa_the_short_key_threshold_is_eight_characters() {
    let seven = "abcdefg";
    assert_eq!(error_text(&parse_chat_events("{\"error\":\"bad ABCDEFG\"}", Some(seven))), GENERIC);
    assert_eq!(redact_stream(Some(seven), vec![delta("abcdefg"), ChatEvent::Done]), vec![delta("abcdefg"), ChatEvent::Done]);
    assert_eq!(error_text(&parse_chat_events("{\"error\":\"bad ABCDEFG\"}", Some("  abcdefg  "))), GENERIC);
    let eight = "abcdefgh";
    assert_eq!(error_text(&parse_chat_events("{\"error\":\"bad ABCDEFGH\"}", Some(eight))), "bad [redacted]");
    assert_eq!(joined_text(&redact_stream(Some(eight), vec![delta("x abcd"), delta("EFGH y"), ChatEvent::Done])), "x [redacted] y");
  }
}
