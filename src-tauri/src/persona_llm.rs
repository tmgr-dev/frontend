use std::collections::HashMap;
use std::net::{Ipv4Addr, Ipv6Addr};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use url::{Host, Url};

const SERVICE: &str = "dev.tmgr.persona-llm";
const ACCOUNT: &str = "api-key";
const CONFIG_FILE: &str = "llm_config.json";

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct LlmConfigFile {
  base_url: String,
  model: String,
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

/// Classifies the LLM base URL so the UI can warn when it leaves the user's own machine/LAN.
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

/// Reassembles `data:` lines from a byte stream that may split events, or a multi-byte UTF-8
/// character, across chunk boundaries. Buffers raw bytes and only decodes a span once it is
/// terminated by a full blank line, so a split character never gets decoded on its own.
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

/// The byte offset and length of the blank line ending one SSE event: `\n\n`, or `\r\n\r\n` for a
/// server that sends CRLF line endings, whichever comes first.
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

/// One `data: <json>` payload of an OpenAI-compatible `chat.completions` stream -> a normalized
/// event, or `None` for a chunk with nothing the agent loop cares about (e.g. a bare role delta).
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

fn keyring_entry() -> Result<keyring::Entry, String> {
  keyring::Entry::new(SERVICE, ACCOUNT).map_err(|e| e.to_string())
}

/// Non-secret settings go to a plain JSON file; the key (if any) goes to the OS keychain and is
/// never returned to JS (`llm_config_get` only reports whether one is set). `api_key: None` (or
/// blank) leaves a previously saved key untouched; pass `clear_api_key: true` to remove it.
#[tauri::command]
pub fn llm_config_set<R: Runtime>(
  app: AppHandle<R>,
  base_url: String,
  model: String,
  api_key: Option<String>,
  clear_api_key: bool,
) -> Result<(), String> {
  let file = LlmConfigFile { base_url, model };
  std::fs::write(config_path(&app)?, serde_json::to_vec(&file).map_err(|e| e.to_string())?)
    .map_err(|e| e.to_string())?;
  let entry = keyring_entry()?;
  if clear_api_key {
    match entry.delete_credential() {
      Ok(()) | Err(keyring::Error::NoEntry) => {}
      Err(e) => return Err(e.to_string()),
    }
  } else if let Some(key) = api_key.filter(|k| !k.is_empty()) {
    entry.set_password(&key).map_err(|e| e.to_string())?;
  }
  Ok(())
}

#[tauri::command]
pub fn llm_config_get<R: Runtime>(app: AppHandle<R>) -> Result<LlmConfigOut, String> {
  let file = read_config(&app)?;
  let has_api_key = match keyring_entry()?.get_password() {
    Ok(_) => true,
    Err(keyring::Error::NoEntry) => false,
    Err(e) => return Err(e.to_string()),
  };
  let is_local = classify_url(&file.base_url).map(|kind| kind == UrlKind::Local).unwrap_or(false);
  Ok(LlmConfigOut { base_url: file.base_url, model: file.model, has_api_key, is_local })
}

static IN_FLIGHT: LazyLock<Mutex<HashMap<String, tokio::task::AbortHandle>>> =
  LazyLock::new(|| Mutex::new(HashMap::new()));

fn emit_event<R: Runtime>(app: &AppHandle<R>, request_id: &str, event: ChatEvent) {
  let _ = app.emit("llm://chat", serde_json::json!({ "request_id": request_id, "event": event }));
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
  let client = reqwest::Client::builder()
    .timeout(Duration::from_secs(120))
    .build()
    .map_err(|e| e.to_string())?;
  let url = format!("{}/v1/chat/completions", config.base_url.trim_end_matches('/'));
  // `messages` is forwarded exactly as the agent loop built it (assistant `tool_calls`, `tool`
  // messages with `tool_call_id`, ...): a typed struct here would silently drop those fields.
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

/// Streams one chat turn and emits `llm://chat` events (`{request_id, event}`) to the webview; the
/// key never crosses the IPC boundary. `llm_cancel` aborts the matching `request_id`.
#[tauri::command]
pub async fn llm_chat<R: Runtime>(
  app: AppHandle<R>,
  request_id: String,
  messages: Vec<Value>,
  tools: Option<Value>,
) -> Result<(), String> {
  let config = read_config(&app)?;
  if config.base_url.is_empty() {
    return Err("Set an LLM base URL first".into());
  }
  let api_key = match keyring_entry()?.get_password() {
    Ok(key) => Some(key),
    Err(keyring::Error::NoEntry) => None,
    Err(e) => return Err(e.to_string()),
  };

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

/// Aborts the in-flight request and emits `Error{message:"cancelled"}` so a caller blocked reading
/// `llm://chat` events for this `request_id` unblocks instead of waiting forever.
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
    for url in [
      "http://my-mac.local:8000",
      "http://10.0.0.5:8000",
      "http://172.16.0.5:8000",
      "http://192.168.1.5:8000",
      "http://169.254.1.1:8000",
      "http://[fe80::1]:8000",
      "http://[fc00::1]:8000",
    ] {
      assert_eq!(classify_url(url).unwrap(), UrlKind::Local, "{url}");
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
    // Cyrillic "П" is 2 bytes (0xD0 0x9F) in UTF-8; splitting it mid-character must not turn
    // either half into a replacement character once the full event is reassembled.
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
  fn ignores_a_role_only_delta_and_invalid_json() {
    assert_eq!(parse_chat_event("{\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}"), None);
    assert_eq!(parse_chat_event("not json"), None);
  }
}
