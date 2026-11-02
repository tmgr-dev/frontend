use std::collections::HashMap;
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

fn user_slot_allowed(identity: Option<i64>, user_id: i64) -> bool {
  match identity {
    Some(id) => user_id == id,
    None => user_id == LOCAL_OWNER_ID,
  }
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
}

impl SseBuffer {
  pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
    self.pending.extend_from_slice(chunk);
    let mut events = Vec::new();
    while let Some((pos, len)) = find_event_end(&self.pending) {
      let raw: Vec<u8> = self.pending.drain(..pos + len).collect();
      let text = String::from_utf8_lossy(&raw).replace("\r\n", "\n");
      let data: Vec<&str> =
        text.lines().filter_map(|line| line.strip_prefix("data:").map(str::trim_start)).collect();
      if !data.is_empty() {
        events.push(data.join("\n"));
      }
    }
    events
  }
}

/// The offset and length of the blank line ending one SSE event (`\n\n` or `\r\n\r\n`).
fn find_event_end(buf: &[u8]) -> Option<(usize, usize)> {
  let lf = buf.windows(2).position(|w| w == b"\n\n").map(|p| (p, 2));
  let crlf = buf.windows(4).position(|w| w == b"\r\n\r\n").map(|p| (p, 4));
  match (lf, crlf) {
    (Some(a), Some(b)) => Some(if a.0 <= b.0 { a } else { b }),
    (Some(a), None) => Some(a),
    (None, Some(b)) => Some(b),
    (None, None) => None,
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

/// `None` for a chunk with nothing the agent loop cares about (e.g. a bare role delta).
pub fn parse_chat_event(data: &str) -> Option<ChatEvent> {
  let trimmed = data.trim();
  if trimmed == "[DONE]" {
    return Some(ChatEvent::Done);
  }
  let value: Value = serde_json::from_str(trimmed).ok()?;
  let delta = value.get("choices")?.get(0)?.get("delta")?;
  if let Some(content) = delta.get("content").and_then(Value::as_str) {
    if !content.is_empty() {
      return Some(ChatEvent::Delta { text: content.to_string() });
    }
  }
  let call = delta.get("tool_calls").and_then(Value::as_array).and_then(|calls| calls.first())?;
  let index = call.get("index").and_then(Value::as_u64).unwrap_or(0) as u32;
  let id = call.get("id").and_then(Value::as_str).map(str::to_string);
  let function = call.get("function");
  let name = function.and_then(|f| f.get("name")).and_then(Value::as_str).map(str::to_string);
  let arguments = function
    .and_then(|f| f.get("arguments"))
    .and_then(Value::as_str)
    .unwrap_or("")
    .to_string();
  Some(ChatEvent::ToolCall { index, id, name, arguments })
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
  if !user_slot_allowed(identity, user_id) {
    return Err("These AI provider settings belong to another account. If you just signed in, try again in a moment.".into());
  }
  let previous = read_previous()?;
  let new_origin = normalize_origin(&base_url).ok();
  let account = format!("{ACCOUNT}:{user_id}");
  let new_key = api_key.filter(|k| !k.is_empty());

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
  let has_api_key = slot_has_key(&PersonaLlmSecrets, crate::local_identity::current_id(&app), user_id)?;
  let is_local = classify_url(&file.base_url).map(|kind| kind == UrlKind::Local).unwrap_or(false);
  Ok(LlmConfigOut { base_url: file.base_url, model: file.model, has_api_key, is_local })
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

static IN_FLIGHT: LazyLock<Mutex<HashMap<String, tokio::task::AbortHandle>>> =
  LazyLock::new(|| Mutex::new(HashMap::new()));

fn emit_event<R: Runtime>(app: &AppHandle<R>, request_id: &str, event: ChatEvent) {
  let _ = app.emit("llm://chat", serde_json::json!({ "request_id": request_id, "event": event }));
}

pub fn chat_completions_url(base_url: &str) -> String {
  let base = base_url.trim_end_matches('/');
  if base.ends_with("/v1") {
    format!("{base}/chat/completions")
  } else {
    format!("{base}/v1/chat/completions")
  }
}

async fn stream_chat<R: Runtime>(
  app: &AppHandle<R>,
  request_id: &str,
  config: &LlmConfigFile,
  api_key: Option<&str>,
  messages: Vec<Value>,
  tools: Option<Value>,
) -> Result<(), String> {
  if rustls::crypto::CryptoProvider::get_default().is_none() {
    let _ = rustls::crypto::ring::default_provider().install_default();
  }
  // Only the connect is capped: a local model may legitimately take minutes to finish a turn.
  let client = reqwest::Client::builder()
    .connect_timeout(Duration::from_secs(5))
    .build()
    .map_err(|e| e.to_string())?;
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
  let mut builder = client
    .post(&url)
    .header("content-type", "application/json")
    .body(serde_json::to_vec(&body).map_err(|e| e.to_string())?);
  if let Some(key) = api_key {
    builder = builder.header("authorization", format!("Bearer {key}"));
  }
  let mut response = builder.send().await.map_err(|e| format!("LLM server unreachable: {e}"))?;
  if !response.status().is_success() {
    return Err(format!("LLM server returned {}", response.status()));
  }
  let mut buffer = SseBuffer::default();
  while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
    for raw in buffer.push(&chunk) {
      if let Some(event) = parse_chat_event(&raw) {
        let done = matches!(event, ChatEvent::Done);
        emit_event(app, request_id, event);
        if done {
          return Ok(());
        }
      }
    }
  }
  emit_event(app, request_id, ChatEvent::Done);
  Ok(())
}

/// The key never crosses the IPC boundary; only `llm://chat` events do.
#[tauri::command]
pub async fn llm_chat<R: Runtime>(
  app: AppHandle<R>,
  user_id: i64,
  request_id: String,
  messages: Vec<Value>,
  tools: Option<Value>,
) -> Result<(), String> {
  if !user_slot_allowed(crate::local_identity::current_id(&app), user_id) {
    return Err("This AI provider key belongs to another account.".into());
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
    check_key_origin(slot_origin(&config, user_id), &config.base_url)?;
  }

  let task_app = app.clone();
  let task_request_id = request_id.clone();
  let join = tokio::spawn(async move {
    if let Err(message) = stream_chat(&task_app, &task_request_id, &config, api_key.as_deref(), messages, tools).await {
      emit_event(&task_app, &task_request_id, ChatEvent::Error { message });
    }
    IN_FLIGHT.lock().ok().map(|mut map| map.remove(&task_request_id));
  });
  IN_FLIGHT
    .lock()
    .map_err(|_| "lock poisoned".to_string())?
    .insert(request_id, join.abort_handle());
  Ok(())
}

/// Emits an error event too, so a caller blocked reading `llm://chat` unblocks instead of hanging.
#[tauri::command]
pub fn llm_cancel<R: Runtime>(app: AppHandle<R>, request_id: String) -> Result<(), String> {
  if let Some(handle) = IN_FLIGHT.lock().map_err(|_| "lock poisoned".to_string())?.remove(&request_id) {
    handle.abort();
    emit_event(&app, &request_id, ChatEvent::Error { message: "cancelled".into() });
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

  const REFUSAL: &str = "These AI provider settings belong to another account. If you just signed in, try again in a moment.";

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
    assert_eq!(saved.result, Err(REFUSAL.to_string()));
    assert_eq!(store.key(-1).as_deref(), Some("KL"));
    assert!(saved.written.is_none());
  }

  #[test]
  fn the_local_account_cannot_touch_an_accounts_slot() {
    let store = Spy::with(&[(8, "K8")]);
    let saved = run_save(&store, request(None, 8, Some("NEW"), false), config_for_slot("8"));
    assert_eq!(saved.result, Err(REFUSAL.to_string()));
    assert_eq!(store.key(8).as_deref(), Some("K8"));
    assert!(saved.written.is_none());
  }

  #[test]
  fn slot_zero_is_refused() {
    for identity in [None, Some(7)] {
      let store = Spy::with(&[(0, "K0")]);
      let saved = run_save(&store, request(identity, 0, Some("NEW"), false), LlmConfigFile::default());
      assert_eq!(saved.result, Err(REFUSAL.to_string()));
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
      parse_chat_event("{\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}"),
      Some(ChatEvent::Delta { text: "Hi".into() })
    );
    assert_eq!(
      parse_chat_event(
        "{\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_1\",\"function\":{\"name\":\"tmgr_request\",\"arguments\":\"{\\\"method\\\"\"}}]}}]}"
      ),
      Some(ChatEvent::ToolCall {
        index: 0,
        id: Some("call_1".into()),
        name: Some("tmgr_request".into()),
        arguments: "{\"method\"".into(),
      })
    );
    assert_eq!(parse_chat_event("[DONE]"), Some(ChatEvent::Done));
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
  fn ignores_a_role_only_delta_and_invalid_json() {
    assert_eq!(parse_chat_event("{\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}"), None);
    assert_eq!(parse_chat_event("not json"), None);
  }
}
