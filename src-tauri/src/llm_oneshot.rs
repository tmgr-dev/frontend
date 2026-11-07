use std::collections::BTreeSet;
use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Manager, Runtime};
use url::{Host, Url};

use super::{ChatEvent, SlotAccess};

const LAN_CONFIRM_FILE: &str = "llm_lan_confirm.json";
const DETECT_CANDIDATES: [&str; 2] = ["http://127.0.0.1:11434", "http://127.0.0.1:1234"];
const DETECT_TIMEOUT: Duration = Duration::from_millis(1500);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const MODELS_TIMEOUT: Duration = Duration::from_secs(30);
const TEST_TIMEOUT: Duration = Duration::from_secs(30);
const COMPLETE_TIMEOUT: Duration = Duration::from_secs(120);
const MAX_COMPLETE_TOKENS: u32 = 2000;
const MAX_PROMPT_CHARS: usize = 32 * 1024;
const MAIN_ONLY: &str = "This command may only be called from the main window";
const NOT_LOCAL: &str = "The local AI server must be on this computer or your local network";
const EMPTY_ANSWER: &str = "The model returned an empty answer";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EndpointClass {
  Loopback,
  Lan,
  Remote,
}

fn addr_class(addr: IpAddr) -> EndpointClass {
  let mapped = match addr {
    IpAddr::V4(ip) => Some(ip),
    IpAddr::V6(ip) => ip.to_ipv4_mapped(),
  };
  if addr.is_loopback() || mapped.is_some_and(|ip| ip.is_loopback()) {
    return EndpointClass::Loopback;
  }
  let local = match addr {
    IpAddr::V4(ip) => super::is_local_v4(ip),
    IpAddr::V6(ip) => super::is_local_v6(ip),
  };
  if local {
    EndpointClass::Lan
  } else {
    EndpointClass::Remote
  }
}

pub fn classify_addrs(addrs: &[IpAddr]) -> EndpointClass {
  if addrs.is_empty() {
    return EndpointClass::Remote;
  }
  let mut lan = false;
  for addr in addrs {
    match addr_class(*addr) {
      EndpointClass::Remote => return EndpointClass::Remote,
      EndpointClass::Lan => lan = true,
      EndpointClass::Loopback => {}
    }
  }
  if lan {
    EndpointClass::Lan
  } else {
    EndpointClass::Loopback
  }
}

pub async fn resolve_endpoint(base_url: &str) -> Result<(EndpointClass, String, String, Vec<SocketAddr>), String> {
  let url = Url::parse(base_url.trim()).map_err(|e| e.to_string())?;
  if !url.username().is_empty() || url.password().is_some() {
    return Err("The server address must not contain a username or password".into());
  }
  let origin = super::normalize_origin(base_url)?;
  let port = url.port_or_known_default().ok_or("URL has no port")?;
  let host = url.host_str().ok_or("URL has no host")?.to_string();
  let addrs: Vec<SocketAddr> = match url.host().ok_or("URL has no host")? {
    Host::Ipv4(ip) => vec![SocketAddr::new(IpAddr::V4(ip), port)],
    Host::Ipv6(ip) => vec![SocketAddr::new(IpAddr::V6(ip), port)],
    Host::Domain(domain) => tokio::time::timeout(CONNECT_TIMEOUT, tokio::net::lookup_host((domain, port)))
      .await
      .map_err(|_| "The local AI server address could not be resolved in time".to_string())?
      .map_err(|_| "Could not resolve the local AI server address".to_string())?
      .collect(),
  };
  let ips: Vec<IpAddr> = addrs.iter().map(SocketAddr::ip).collect();
  Ok((classify_addrs(&ips), origin, host, addrs))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct LanConfirm {
  origin: String,
  addrs: Vec<String>,
  user_id: i64,
}

impl LanConfirm {
  fn covers(&self, origin: &str, addrs: &[IpAddr], user_id: i64) -> bool {
    let stored: BTreeSet<IpAddr> = self.addrs.iter().filter_map(|addr| addr.parse().ok()).collect();
    self.origin == origin && self.user_id == user_id && addrs.iter().all(|addr| stored.contains(addr))
  }
}

fn gate(
  class: EndpointClass,
  origin: &str,
  addrs: &[IpAddr],
  user_id: i64,
  confirmed: Option<&LanConfirm>,
) -> Result<(), String> {
  match class {
    EndpointClass::Loopback => Ok(()),
    EndpointClass::Lan if confirmed.is_some_and(|saved| saved.covers(origin, addrs, user_id)) => Ok(()),
    EndpointClass::Lan => Err(format!("needs_confirmation:{origin}")),
    EndpointClass::Remote => Err(NOT_LOCAL.into()),
  }
}

#[derive(Clone)]
struct Endpoint {
  origin: String,
  base_url: String,
  model: String,
  api_key: Option<String>,
  pin: Option<(String, Vec<SocketAddr>)>,
}

impl fmt::Debug for Endpoint {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Endpoint")
      .field("origin", &self.origin)
      .field("base_url", &self.base_url)
      .field("model", &self.model)
      .field("api_key", &self.api_key.as_ref().map(|_| "[hidden]"))
      .field("pin", &self.pin)
      .finish()
  }
}

fn is_domain(host: &str) -> bool {
  host.trim_start_matches('[').trim_end_matches(']').parse::<IpAddr>().is_err()
}

async fn endpoint_for(
  base_url: &str,
  model: &str,
  api_key: Option<String>,
  user_id: i64,
  confirmed: Option<&LanConfirm>,
) -> Result<Endpoint, String> {
  let (class, origin, host, addrs) = resolve_endpoint(base_url).await?;
  let ips: Vec<IpAddr> = addrs.iter().map(SocketAddr::ip).collect();
  gate(class, &origin, &ips, user_id, confirmed)?;
  let pin = is_domain(&host).then(|| (host, addrs));
  Ok(Endpoint { origin, base_url: base_url.trim().to_string(), model: model.to_string(), api_key, pin })
}

fn read_lan_confirm(path: &Path) -> Option<LanConfirm> {
  let bytes = std::fs::read(path).ok()?;
  serde_json::from_slice(&bytes).ok()
}

fn write_lan_confirm(path: &Path, confirm: Option<&LanConfirm>) -> Result<(), String> {
  match confirm {
    Some(confirm) => {
      let bytes = serde_json::to_vec(confirm).map_err(|e| e.to_string())?;
      std::fs::write(path, bytes).map_err(|e| e.to_string())
    }
    None => match std::fs::remove_file(path) {
      Ok(()) => Ok(()),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
      Err(e) => Err(e.to_string()),
    },
  }
}

fn record_lan_confirm(
  base_url: &str,
  origin: &str,
  user_id: i64,
  addrs: &[IpAddr],
  confirmed: bool,
  path: &Path,
) -> Result<(), String> {
  if !confirmed {
    return write_lan_confirm(path, None);
  }
  if super::normalize_origin(base_url).ok().as_deref() != Some(origin) {
    return Err("The server address changed. Check it in Settings and try again.".into());
  }
  let addrs: BTreeSet<String> = addrs.iter().map(IpAddr::to_string).collect();
  let confirm = LanConfirm { origin: origin.to_string(), addrs: addrs.into_iter().collect(), user_id };
  write_lan_confirm(path, Some(&confirm))
}

fn prompt_too_long(system: Option<&str>, prompt: &str) -> bool {
  let system_chars = system.map_or(0, |text| text.chars().count());
  system_chars + prompt.chars().count() > MAX_PROMPT_CHARS
}

fn split_query(input: &str) -> (&str, Option<&str>) {
  let without_fragment = input.trim().split('#').next().unwrap_or("");
  match without_fragment.split_once('?') {
    Some((head, query)) => (head, Some(query).filter(|q| !q.is_empty())),
    None => (without_fragment, None),
  }
}

pub fn models_url(base_url: &str) -> String {
  let (head, query) = split_query(base_url);
  let base = head.trim_end_matches('/');
  const CHAT_SUFFIX: &str = "/chat/completions";
  let root = if base.to_ascii_lowercase().ends_with(CHAT_SUFFIX) {
    &base[..base.len() - CHAT_SUFFIX.len()]
  } else {
    base
  };
  let root = root.trim_end_matches('/');
  let mut url = if root.to_ascii_lowercase().ends_with("/v1") {
    format!("{root}/models")
  } else {
    format!("{root}/v1/models")
  };
  if let Some(query) = query {
    url.push('?');
    url.push_str(query);
  }
  url
}

pub fn complete_body(model: &str, system: Option<&str>, user: &str, max_tokens: u32, temperature: f32) -> Value {
  let mut messages = Vec::new();
  if let Some(system) = system {
    messages.push(json!({"role": "system", "content": system}));
  }
  messages.push(json!({"role": "user", "content": user}));
  json!({
    "model": model,
    "messages": messages,
    "max_tokens": max_tokens,
    "temperature": temperature,
    "stream": false,
  })
}

fn clamp_request(max_tokens: u32, temperature: f32) -> (u32, f32) {
  let temperature = if temperature.is_finite() { temperature.clamp(0.0, 1.0) } else { 0.0 };
  (max_tokens.clamp(1, MAX_COMPLETE_TOKENS), temperature)
}

fn is_embedding_model(id: &str) -> bool {
  let id = id.to_ascii_lowercase();
  id.contains("embed") || id.contains("bge-")
}

pub fn parse_models(body: &str) -> Vec<String> {
  let Ok(value) = serde_json::from_str::<Value>(body.trim()) else {
    return Vec::new();
  };
  let ids: BTreeSet<String> = value
    .get("data")
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    .filter_map(|model| model.get("id").and_then(Value::as_str))
    .filter(|id| !is_embedding_model(id))
    .map(str::to_string)
    .collect();
  ids.into_iter().collect()
}

pub fn parse_complete_reply(body: &str, api_key: Option<&str>) -> Result<String, String> {
  let mut text = String::new();
  for event in super::parse_chat_json_reply(body, api_key) {
    match event {
      ChatEvent::Delta { text: delta } => text.push_str(&delta),
      ChatEvent::Error { message } if message == super::NO_USABLE_CONTENT => return Err(EMPTY_ANSWER.into()),
      ChatEvent::Error { message } => return Err(message),
      ChatEvent::ToolCall { .. } | ChatEvent::Done => {}
    }
  }
  if text.trim().is_empty() {
    Err(EMPTY_ANSWER.into())
  } else {
    Ok(text)
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reachable {
  ModelOk,
  ModelMissing,
  EmptyAnswer,
}

impl Reachable {
  fn as_str(self) -> &'static str {
    match self {
      Reachable::ModelOk => "model_ok",
      Reachable::ModelMissing => "model_missing",
      Reachable::EmptyAnswer => "empty_answer",
    }
  }
}

pub fn test_verdict(
  status: reqwest::StatusCode,
  body: &str,
  model: &str,
  api_key: Option<&str>,
) -> Result<Reachable, String> {
  if status.is_success() {
    let answered = super::parse_chat_json_reply(body, api_key).iter().any(|e| matches!(e, ChatEvent::Delta { .. }));
    let truncated = serde_json::from_str::<Value>(body.trim())
      .ok()
      .is_some_and(|v| super::finish_reason(&v) == Some("length"));
    return Ok(if answered || truncated { Reachable::ModelOk } else { Reachable::EmptyAnswer });
  }
  let mentions_model = !model.is_empty() && body.to_ascii_lowercase().contains(&model.to_ascii_lowercase());
  match status.as_u16() {
    400 | 404 if mentions_model => Ok(Reachable::ModelMissing),
    _ => Err(super::sanitize_error_body(&status.to_string(), body, api_key)),
  }
}

#[derive(Debug, Clone, Serialize)]
pub struct Detected {
  base_url: String,
  models: usize,
}

fn install_crypto_provider() {
  if rustls::crypto::CryptoProvider::get_default().is_none() {
    let _ = rustls::crypto::ring::default_provider().install_default();
  }
}

fn pinned_client(endpoint: &Endpoint, total: Duration) -> Result<reqwest::Client, String> {
  install_crypto_provider();
  let mut builder = reqwest::Client::builder()
    .connect_timeout(CONNECT_TIMEOUT)
    .redirect(reqwest::redirect::Policy::none())
    .no_proxy()
    .timeout(total);
  if let Some((host, addrs)) = &endpoint.pin {
    builder = builder.resolve_to_addrs(host, addrs);
  }
  builder.build().map_err(|e| e.to_string())
}

async fn exchange(
  endpoint: &Endpoint,
  total: Duration,
  url: &str,
  body: Option<&Value>,
) -> Result<(reqwest::StatusCode, String), String> {
  let client = pinned_client(endpoint, total)?;
  let mut request = match body {
    Some(body) => client
      .post(url)
      .header("content-type", "application/json")
      .body(serde_json::to_vec(body).map_err(|e| e.to_string())?),
    None => client.get(url),
  };
  if let Some(key) = &endpoint.api_key {
    request = request.bearer_auth(key);
  }
  let mut response = request
    .send()
    .await
    .map_err(|e| format!("LLM server unreachable: {}", e.without_url()))?;
  let status = response.status();
  if status.is_redirection() {
    let location = response.headers().get("location").and_then(|v| v.to_str().ok());
    return Err(super::redirect_error(location, endpoint.api_key.as_deref()));
  }
  let cap = if status.is_success() { super::JSON_BODY_CAP } else { super::ERROR_BODY_CAP };
  let bytes = super::read_capped(&mut response, cap).await?;
  Ok((status, String::from_utf8_lossy(&bytes).into_owned()))
}

fn http_error(status: reqwest::StatusCode, body: &str, api_key: Option<&str>) -> String {
  super::sanitize_error_body(&status.to_string(), body, api_key)
}

async fn fetch_models(endpoint: &Endpoint) -> Result<Vec<String>, String> {
  let (status, body) = exchange(endpoint, MODELS_TIMEOUT, &models_url(&endpoint.base_url), None).await?;
  if !status.is_success() {
    return Err(http_error(status, &body, endpoint.api_key.as_deref()));
  }
  Ok(parse_models(&body))
}

async fn run_complete(
  endpoint: &Endpoint,
  system: Option<&str>,
  prompt: &str,
  max_tokens: u32,
  temperature: f32,
) -> Result<String, String> {
  let body = complete_body(&endpoint.model, system, prompt, max_tokens, temperature);
  let url = super::chat_completions_url(&endpoint.base_url);
  let (status, text) = exchange(endpoint, COMPLETE_TIMEOUT, &url, Some(&body)).await?;
  if !status.is_success() {
    return Err(http_error(status, &text, endpoint.api_key.as_deref()));
  }
  parse_complete_reply(&text, endpoint.api_key.as_deref())
}

async fn run_test(endpoint: &Endpoint) -> Result<&'static str, String> {
  let body = complete_body(&endpoint.model, None, "ping", 16, 0.0);
  let url = super::chat_completions_url(&endpoint.base_url);
  let (status, text) = exchange(endpoint, TEST_TIMEOUT, &url, Some(&body)).await?;
  test_verdict(status, &text, &endpoint.model, endpoint.api_key.as_deref()).map(Reachable::as_str)
}

async fn probe(base: &str) -> Option<Detected> {
  let client = reqwest::Client::builder()
    .no_proxy()
    .redirect(reqwest::redirect::Policy::none())
    .timeout(DETECT_TIMEOUT)
    .build()
    .ok()?;
  let mut response = client.get(models_url(base)).send().await.ok()?;
  let status = response.status();
  if status.is_redirection() || !status.is_success() {
    return None;
  }
  let bytes = super::read_capped(&mut response, super::JSON_BODY_CAP).await.ok()?;
  Some(Detected {
    base_url: base.to_string(),
    models: parse_models(&String::from_utf8_lossy(&bytes)).len(),
  })
}

async fn probe_candidates(bases: &[&str]) -> Vec<Detected> {
  let mut found = Vec::new();
  for base in bases {
    if let Some(detected) = probe(base).await {
      found.push(detected);
    }
  }
  found
}

fn ensure_main<R: Runtime>(window: &tauri::Window<R>) -> Result<(), String> {
  if window.label() != "main" {
    return Err(MAIN_ONLY.into());
  }
  Ok(())
}

fn confirm_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
  let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
  Ok(dir.join(LAN_CONFIRM_FILE))
}

fn ensure_slot_access<R: Runtime>(app: &AppHandle<R>, user_id: i64) -> Result<(), String> {
  let access = super::slot_access(crate::local_identity::current_id(app), user_id);
  if access != SlotAccess::Allowed {
    return Err(access.chat_refusal());
  }
  Ok(())
}

async fn prepare<R: Runtime>(app: &AppHandle<R>, window: &tauri::Window<R>, user_id: i64) -> Result<Endpoint, String> {
  ensure_main(window)?;
  ensure_slot_access(app, user_id)?;
  let (config, api_key, confirmed) = {
    let _guard = super::config_lock();
    let config = super::read_config(app)?;
    if config.base_url.trim().is_empty() {
      return Err("Set an LLM base URL first".into());
    }
    let api_key = match super::keyring_entry(user_id)?.get_password() {
      Ok(key) => Some(key),
      Err(keyring::Error::NoEntry) => None,
      Err(e) => return Err(e.to_string()),
    };
    if api_key.is_some() {
      super::refuses_plaintext_key(&config.base_url, true)?;
      super::check_key_origin(super::slot_origin(&config, user_id), &config.base_url)?;
    }
    let confirmed = read_lan_confirm(&confirm_path(app)?);
    (config, api_key, confirmed)
  };
  endpoint_for(&config.base_url, &config.model, api_key, user_id, confirmed.as_ref()).await
}

#[tauri::command]
pub async fn llm_detect<R: Runtime>(window: tauri::Window<R>) -> Result<Vec<Detected>, String> {
  ensure_main(&window)?;
  Ok(probe_candidates(&DETECT_CANDIDATES).await)
}

#[tauri::command]
pub async fn llm_lan_confirm<R: Runtime>(
  app: AppHandle<R>,
  window: tauri::Window<R>,
  origin: String,
  confirmed: bool,
  user_id: i64,
) -> Result<(), String> {
  ensure_main(&window)?;
  ensure_slot_access(&app, user_id)?;
  let base_url = {
    let _guard = super::config_lock();
    super::read_config(&app)?.base_url
  };
  let addrs = if confirmed { resolve_endpoint(&base_url).await?.3 } else { Vec::new() };
  let ips: Vec<IpAddr> = addrs.iter().map(SocketAddr::ip).collect();
  let _guard = super::config_lock();
  let current = super::read_config(&app)?.base_url;
  record_lan_confirm(&current, &origin, user_id, &ips, confirmed, &confirm_path(&app)?)
}

#[tauri::command]
pub async fn llm_models<R: Runtime>(app: AppHandle<R>, window: tauri::Window<R>, user_id: i64) -> Result<Vec<String>, String> {
  let endpoint = prepare(&app, &window, user_id).await?;
  fetch_models(&endpoint).await
}

#[tauri::command]
pub async fn llm_test<R: Runtime>(app: AppHandle<R>, window: tauri::Window<R>, user_id: i64) -> Result<String, String> {
  let endpoint = prepare(&app, &window, user_id).await?;
  run_test(&endpoint).await.map(str::to_string)
}

#[tauri::command]
pub async fn llm_complete<R: Runtime>(
  app: AppHandle<R>,
  window: tauri::Window<R>,
  user_id: i64,
  system: Option<String>,
  prompt: String,
  max_tokens: u32,
  temperature: f32,
) -> Result<String, String> {
  if prompt_too_long(system.as_deref(), &prompt) {
    return Err("The prompt is too long".into());
  }
  let (max_tokens, temperature) = clamp_request(max_tokens, temperature);
  let endpoint = prepare(&app, &window, user_id).await?;
  run_complete(&endpoint, system.as_deref(), &prompt, max_tokens, temperature).await
}

#[cfg(test)]
mod llm_oneshot_tests {
  use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
  use std::sync::{Arc, Mutex};

  use serde_json::Value;
  use tokio::io::{AsyncReadExt, AsyncWriteExt};
  use tokio::net::{TcpListener, TcpStream};

  use super::*;

  fn v4(a: u8, b: u8, c: u8, d: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new(a, b, c, d))
  }

  fn lan(a: u8, b: u8, c: u8, d: u8) -> String {
    Ipv4Addr::new(a, b, c, d).to_string()
  }

  #[test]
  fn loopback_only_when_every_address_is_loopback() {
    let loopback = [v4(127, 0, 0, 1), IpAddr::V6(Ipv6Addr::LOCALHOST), "::ffff:127.0.0.9".parse().unwrap()];
    for addr in loopback {
      assert_eq!(classify_addrs(&[addr]), EndpointClass::Loopback, "{addr}");
    }
    assert_eq!(classify_addrs(&[v4(127, 0, 0, 1), IpAddr::V6(Ipv6Addr::LOCALHOST)]), EndpointClass::Loopback);
  }

  #[test]
  fn private_link_local_and_ula_addresses_are_lan() {
    let lan: [IpAddr; 6] = [
      v4(10, 1, 2, 3),
      v4(172, 16, 0, 5),
      v4(192, 168, 1, 20),
      v4(169, 254, 3, 4),
      "fd12::5".parse().unwrap(),
      format!("::ffff:{}", lan(10, 1, 2, 3)).parse().unwrap(),
    ];
    for addr in lan {
      assert_eq!(classify_addrs(&[addr]), EndpointClass::Lan, "{addr}");
    }
  }

  #[test]
  fn a_mix_of_loopback_and_lan_is_lan() {
    assert_eq!(classify_addrs(&[v4(127, 0, 0, 1), v4(192, 168, 0, 9)]), EndpointClass::Lan);
  }

  #[test]
  fn any_public_address_or_no_address_is_remote() {
    assert_eq!(classify_addrs(&[]), EndpointClass::Remote);
    assert_eq!(classify_addrs(&[v4(203, 0, 113, 8)]), EndpointClass::Remote);
    assert_eq!(classify_addrs(&[v4(127, 0, 0, 1), v4(203, 0, 113, 44)]), EndpointClass::Remote);
    assert_eq!(classify_addrs(&[v4(192, 168, 0, 9), "2001:db8::1111".parse().unwrap()]), EndpointClass::Remote);
    assert_eq!(classify_addrs(&["::ffff:203.0.113.8".parse().unwrap()]), EndpointClass::Remote);
  }

  #[tokio::test]
  async fn ip_literal_hosts_need_no_lookup() {
    let ip = lan(192, 168, 1, 20);
    let (class, origin, host, addrs) = resolve_endpoint(&format!("http://{ip}:1234/v1")).await.unwrap();
    assert_eq!(class, EndpointClass::Lan);
    assert_eq!(origin, format!("http://{ip}:1234"));
    assert_eq!(host, ip);
    assert_eq!(addrs, vec![SocketAddr::new(v4(192, 168, 1, 20), 1234)]);
  }

  #[tokio::test]
  async fn localhost_resolves_to_loopback_addresses() {
    let (class, origin, host, addrs) = resolve_endpoint("http://localhost:11434").await.unwrap();
    assert_eq!(class, EndpointClass::Loopback);
    assert_eq!(origin, "http://localhost:11434");
    assert_eq!(host, "localhost");
    assert!(!addrs.is_empty());
  }

  #[tokio::test]
  async fn a_host_name_is_pinned_to_the_addresses_it_resolved_to() {
    let endpoint = endpoint_for("http://localhost:11434", "m", None, 1, None).await.unwrap();
    let (host, addrs) = endpoint.pin.unwrap();
    assert_eq!(host, "localhost");
    assert!(!addrs.is_empty());
  }

  fn confirmation(origin: &str, addrs: &[IpAddr], user_id: i64) -> LanConfirm {
    let mut addrs: Vec<String> = addrs.iter().map(IpAddr::to_string).collect();
    addrs.sort();
    LanConfirm { origin: origin.to_string(), addrs, user_id }
  }

  #[test]
  fn gate_allows_loopback_and_blocks_unconfirmed_lan() {
    let origin = format!("http://{}:1234", lan(192, 168, 1, 20));
    let addrs = [v4(192, 168, 1, 20)];
    assert!(gate(EndpointClass::Loopback, &origin, &addrs, 1, None).is_ok());
    let err = gate(EndpointClass::Lan, &origin, &addrs, 1, None).unwrap_err();
    assert_eq!(err, format!("needs_confirmation:{origin}"));
    let saved = confirmation(&origin, &addrs, 1);
    assert!(gate(EndpointClass::Lan, &origin, &addrs, 1, Some(&saved)).is_ok());
  }

  #[test]
  fn a_different_private_address_for_the_same_origin_is_unconfirmed() {
    let origin = format!("http://{}:1234", lan(192, 168, 1, 20));
    let saved = confirmation(&origin, &[v4(192, 168, 1, 20)], 1);
    let moved = [v4(192, 168, 1, 99)];
    assert!(gate(EndpointClass::Lan, &origin, &moved, 1, Some(&saved)).unwrap_err().starts_with("needs_confirmation:"));
  }

  #[test]
  fn a_subset_of_the_confirmed_addresses_stays_confirmed() {
    let origin = format!("http://{}:1234", lan(192, 168, 1, 20));
    let saved = confirmation(&origin, &[v4(192, 168, 1, 20), v4(192, 168, 1, 99)], 1);
    assert!(gate(EndpointClass::Lan, &origin, &[v4(192, 168, 1, 99)], 1, Some(&saved)).is_ok());
  }

  #[test]
  fn another_user_is_unconfirmed_for_the_same_origin() {
    let origin = format!("http://{}:1234", lan(192, 168, 1, 20));
    let addrs = [v4(192, 168, 1, 20)];
    let saved = confirmation(&origin, &addrs, 1);
    assert!(gate(EndpointClass::Lan, &origin, &addrs, 2, Some(&saved)).unwrap_err().starts_with("needs_confirmation:"));
  }

  #[test]
  fn a_port_or_scheme_change_makes_a_lan_origin_unconfirmed_again() {
    let ip = lan(192, 168, 1, 20);
    let addrs = [v4(192, 168, 1, 20)];
    let confirmed = confirmation(&format!("http://{ip}:1234"), &addrs, 1);
    let other_port = format!("http://{ip}:4321");
    let other_scheme = format!("https://{ip}:1234");
    assert!(gate(EndpointClass::Lan, &other_port, &addrs, 1, Some(&confirmed)).unwrap_err().starts_with("needs_confirmation:"));
    assert!(gate(EndpointClass::Lan, &other_scheme, &addrs, 1, Some(&confirmed)).unwrap_err().starts_with("needs_confirmation:"));
  }

  #[test]
  fn remote_is_never_allowed_even_when_confirmed() {
    let origin = "http://203.0.113.8:1234";
    let addrs = [v4(203, 0, 113, 8)];
    let expected = "The local AI server must be on this computer or your local network";
    assert_eq!(gate(EndpointClass::Remote, origin, &addrs, 1, None).unwrap_err(), expected);
    let saved = confirmation(origin, &addrs, 1);
    assert_eq!(gate(EndpointClass::Remote, origin, &addrs, 1, Some(&saved)).unwrap_err(), expected);
  }

  #[tokio::test]
  async fn endpoint_for_refuses_remote_and_unconfirmed_lan_before_any_send() {
    let lan_origin = format!("http://{}:1234", lan(192, 168, 1, 20));
    assert!(endpoint_for("http://203.0.113.8:1234", "m", None, 1, None).await.is_err());
    assert_eq!(
      endpoint_for(&lan_origin, "m", None, 1, None).await.err().unwrap(),
      format!("needs_confirmation:{lan_origin}")
    );
    let saved = confirmation(&lan_origin, &[v4(192, 168, 1, 20)], 1);
    let ok = endpoint_for(&lan_origin, "m", None, 1, Some(&saved)).await.unwrap();
    assert_eq!(ok.origin, lan_origin);
    assert!(ok.pin.is_none());
  }

  #[test]
  fn lan_confirmation_round_trips_through_its_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(LAN_CONFIRM_FILE);
    let origin = format!("http://{}:1234", lan(192, 168, 1, 20));
    let saved = confirmation(&origin, &[v4(192, 168, 1, 20)], 7);
    assert_eq!(read_lan_confirm(&path), None);
    write_lan_confirm(&path, Some(&saved)).unwrap();
    assert_eq!(read_lan_confirm(&path), Some(saved));
    write_lan_confirm(&path, None).unwrap();
    assert_eq!(read_lan_confirm(&path), None);
    write_lan_confirm(&path, None).unwrap();
  }

  #[test]
  fn an_old_origin_only_confirmation_file_counts_as_unconfirmed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(LAN_CONFIRM_FILE);
    std::fs::write(&path, format!(r#"{{"origin":"http://{}:1234"}}"#, lan(192, 168, 1, 20))).unwrap();
    assert_eq!(read_lan_confirm(&path), None);
  }

  #[test]
  fn a_malformed_confirmation_file_counts_as_unconfirmed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(LAN_CONFIRM_FILE);
    std::fs::write(&path, b"{not json").unwrap();
    assert_eq!(read_lan_confirm(&path), None);
  }

  #[test]
  fn recording_a_confirmation_needs_the_saved_server_origin() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(LAN_CONFIRM_FILE);
    let ip = lan(192, 168, 1, 20);
    let base = format!("http://{ip}:1234/v1");
    let origin = format!("http://{ip}:1234");
    let other = format!("http://{}:1234", lan(192, 168, 1, 99));
    assert!(record_lan_confirm(&base, &other, 1, &[v4(192, 168, 1, 20)], true, &path).is_err());
    assert!(!path.exists());
    record_lan_confirm(&base, &origin, 1, &[v4(192, 168, 1, 20)], true, &path).unwrap();
    assert_eq!(read_lan_confirm(&path), Some(confirmation(&origin, &[v4(192, 168, 1, 20)], 1)));
  }

  #[test]
  fn forgetting_a_confirmation_skips_the_origin_check() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(LAN_CONFIRM_FILE);
    let ip = lan(192, 168, 1, 20);
    let base = format!("http://{ip}:1234/v1");
    let origin = format!("http://{ip}:1234");
    write_lan_confirm(&path, Some(&confirmation(&origin, &[v4(192, 168, 1, 20)], 1))).unwrap();
    let other = format!("http://{}:4321", lan(192, 168, 1, 99));
    record_lan_confirm(&base, &other, 1, &[], false, &path).unwrap();
    assert!(!path.exists());
  }

  #[tokio::test]
  async fn a_base_url_with_userinfo_is_refused() {
    let err = resolve_endpoint(&format!("http://user:secret@{}:1234", lan(192, 168, 1, 20))).await.unwrap_err();
    assert_eq!(err, "The server address must not contain a username or password");
  }

  #[test]
  fn an_empty_model_never_matches_a_missing_model_message() {
    let status = reqwest::StatusCode::from_u16(404).unwrap();
    let body = r#"{"error":{"message":"model not found"}}"#;
    assert!(test_verdict(status, body, "", None).is_err());
  }

  #[test]
  fn models_url_mirrors_the_chat_completions_base_handling() {
    let cases = [
      ("http://127.0.0.1:1234", "http://127.0.0.1:1234/v1/models"),
      ("http://127.0.0.1:1234/", "http://127.0.0.1:1234/v1/models"),
      ("http://127.0.0.1:1234/v1", "http://127.0.0.1:1234/v1/models"),
      ("http://127.0.0.1:1234/v1/", "http://127.0.0.1:1234/v1/models"),
      ("http://127.0.0.1:1234/v1/chat/completions", "http://127.0.0.1:1234/v1/models"),
      ("http://127.0.0.1:1234/chat/completions", "http://127.0.0.1:1234/v1/models"),
      ("http://127.0.0.1:1234/V1/Chat/Completions", "http://127.0.0.1:1234/V1/models"),
      ("http://127.0.0.1:1234/v1?key=abc", "http://127.0.0.1:1234/v1/models?key=abc"),
      ("http://127.0.0.1:1234/v1/chat/completions?x=1", "http://127.0.0.1:1234/v1/models?x=1"),
      ("http://127.0.0.1:1234/v1#frag", "http://127.0.0.1:1234/v1/models"),
    ];
    for (base, expected) in cases {
      assert_eq!(models_url(base), expected, "{base}");
    }
  }

  #[test]
  fn complete_body_has_only_the_plain_completion_fields() {
    let body = complete_body("qwen", Some("be brief"), "hi", 16, 0.5);
    assert_eq!(body["model"], "qwen");
    assert_eq!(body["max_tokens"], 16);
    assert_eq!(body["stream"], false);
    assert!((body["temperature"].as_f64().unwrap() - 0.5).abs() < 1e-6);
    assert_eq!(
      body["messages"],
      serde_json::json!([{"role": "system", "content": "be brief"}, {"role": "user", "content": "hi"}])
    );
    assert!(body.get("tools").is_none());

    let bare = complete_body("qwen", None, "hi", 16, 0.0);
    assert_eq!(bare["messages"], serde_json::json!([{"role": "user", "content": "hi"}]));
    assert_eq!(bare.as_object().unwrap().len(), 5);
  }

  #[test]
  fn requests_are_clamped_to_safe_bounds() {
    assert_eq!(clamp_request(0, 0.7), (1, 0.7));
    assert_eq!(clamp_request(5000, 3.0), (2000, 1.0));
    assert_eq!(clamp_request(100, -1.0), (100, 0.0));
    assert_eq!(clamp_request(100, f32::NAN), (100, 0.0));
  }

  #[test]
  fn the_prompt_limit_counts_the_system_text_too() {
    let half = "a".repeat(MAX_PROMPT_CHARS / 2);
    assert!(!prompt_too_long(Some(half.as_str()), &half));
    assert!(prompt_too_long(Some(half.as_str()), &format!("{half}b")));
    assert!(!prompt_too_long(None, &"a".repeat(MAX_PROMPT_CHARS)));
  }

  #[test]
  fn parse_models_keeps_chat_models_sorted_and_unique() {
    let body = r#"{"object":"list","data":[
      {"id":"qwen3"},{"id":"nomic-embed-text"},{"id":"llama"},{"id":"qwen3"},
      {"id":"BGE-M3"},{"id":"text-embedding-3-small"},{"id":"bge-small"}]}"#;
    assert_eq!(parse_models(body), vec!["llama".to_string(), "qwen3".to_string()]);
  }

  #[test]
  fn parse_models_returns_nothing_for_garbage() {
    assert!(parse_models("<html>").is_empty());
    assert!(parse_models(r#"{"data":"x"}"#).is_empty());
  }

  #[test]
  fn parse_complete_reply_joins_text_and_fails_on_empty_or_error() {
    let body = r#"{"choices":[{"message":{"content":"Hello, "},"finish_reason":"stop"}]}"#;
    assert_eq!(parse_complete_reply(body, None).unwrap(), "Hello, ");
    let empty = r#"{"choices":[{"message":{"content":""},"finish_reason":"stop"}]}"#;
    assert_eq!(parse_complete_reply(empty, None).unwrap_err(), "The model returned an empty answer");
    let error = r#"{"error":{"message":"bad key sk-live-abcdef123456 rejected"}}"#;
    let message = parse_complete_reply(error, Some("sk-live-abcdef123456")).unwrap_err();
    assert!(!message.contains("sk-live-abcdef123456"));
  }

  #[test]
  fn test_verdict_reports_reachability_and_missing_models() {
    let ok = r#"{"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}]}"#;
    let truncated = r#"{"choices":[{"message":{"content":""},"finish_reason":"length"}]}"#;
    let silent = r#"{"choices":[{"message":{"content":""},"finish_reason":"stop"}]}"#;
    let status = |code: u16| reqwest::StatusCode::from_u16(code).unwrap();
    assert_eq!(test_verdict(status(200), ok, "qwen", None), Ok(Reachable::ModelOk));
    assert_eq!(test_verdict(status(200), truncated, "qwen", None), Ok(Reachable::ModelOk));
    assert_eq!(test_verdict(status(200), silent, "qwen", None), Ok(Reachable::EmptyAnswer));
    let mentions = r#"{"error":{"message":"model 'qwen' not found"}}"#;
    assert!(test_verdict(status(404), "", "qwen", None).is_err());
    assert_eq!(test_verdict(status(404), mentions, "qwen", None), Ok(Reachable::ModelMissing));
    assert_eq!(test_verdict(status(400), mentions, "qwen", None), Ok(Reachable::ModelMissing));
    let unrelated = r#"{"error":{"message":"max_tokens too large"}}"#;
    assert!(test_verdict(status(400), unrelated, "qwen", None).is_err());
    assert!(test_verdict(status(500), "", "qwen", None).is_err());
    assert_eq!(Reachable::ModelOk.as_str(), "model_ok");
    assert_eq!(Reachable::ModelMissing.as_str(), "model_missing");
    assert_eq!(Reachable::EmptyAnswer.as_str(), "empty_answer");
  }

  fn http(status: &str, extra_headers: &str, body: &str) -> String {
    format!(
      "HTTP/1.1 {status}\r\n{extra_headers}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
      body.len()
    )
  }

  struct Stub {
    base: String,
    requests: Arc<Mutex<Vec<String>>>,
  }

  impl Stub {
    fn hits(&self) -> usize {
      self.requests.lock().unwrap().len()
    }

    fn last_request(&self) -> String {
      self.requests.lock().unwrap().last().cloned().unwrap_or_default()
    }
  }

  async fn read_request(socket: &mut TcpStream) -> String {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
      let n = socket.read(&mut chunk).await.unwrap_or(0);
      if n == 0 {
        break;
      }
      buf.extend_from_slice(&chunk[..n]);
      if let Some(end) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
        let headers = String::from_utf8_lossy(&buf[..end]).to_ascii_lowercase();
        let length = headers
          .lines()
          .find_map(|line| line.strip_prefix("content-length:"))
          .and_then(|v| v.trim().parse::<usize>().ok())
          .unwrap_or(0);
        if buf.len() >= end + 4 + length {
          break;
        }
      }
    }
    String::from_utf8_lossy(&buf).into_owned()
  }

  async fn serve(response: String) -> Stub {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let requests = Arc::new(Mutex::new(Vec::new()));
    let seen = requests.clone();
    tokio::spawn(async move {
      while let Ok((mut socket, _)) = listener.accept().await {
        let request = read_request(&mut socket).await;
        seen.lock().unwrap().push(request);
        let _ = socket.write_all(response.as_bytes()).await;
        let _ = socket.shutdown().await;
      }
    });
    Stub { base: format!("http://{addr}"), requests }
  }

  fn loopback_endpoint(base: &str, key: Option<&str>) -> Endpoint {
    Endpoint {
      origin: super::super::normalize_origin(base).unwrap(),
      base_url: base.to_string(),
      model: "qwen".to_string(),
      api_key: key.map(str::to_string),
      pin: None,
    }
  }

  fn body_of(raw: &str) -> Value {
    let (_, body) = raw.split_once("\r\n\r\n").unwrap();
    serde_json::from_str(body).unwrap()
  }

  #[tokio::test]
  async fn complete_posts_max_tokens_and_no_tools_with_the_bearer_key() {
    let reply = r#"{"choices":[{"message":{"content":"pong"},"finish_reason":"stop"}]}"#;
    let stub = serve(http("200 OK", "", reply)).await;
    let endpoint = loopback_endpoint(&stub.base, Some("sk-test-key-123456"));
    let answer = run_complete(&endpoint, None, "ping", 16, 0.0).await.unwrap();
    assert_eq!(answer, "pong");
    let raw = stub.last_request();
    assert!(raw.starts_with("POST /v1/chat/completions "));
    assert!(raw.to_ascii_lowercase().contains("authorization: bearer sk-test-key-123456"));
    let body = body_of(&raw);
    assert_eq!(body["max_tokens"], 16);
    assert_eq!(body["stream"], false);
    assert!(body.get("tools").is_none());
  }

  #[tokio::test]
  async fn models_returns_the_filtered_list() {
    let list = r#"{"data":[{"id":"qwen3"},{"id":"nomic-embed-text"},{"id":"llama"}]}"#;
    let stub = serve(http("200 OK", "", list)).await;
    let endpoint = loopback_endpoint(&stub.base, None);
    assert_eq!(fetch_models(&endpoint).await.unwrap(), vec!["llama".to_string(), "qwen3".to_string()]);
    assert!(stub.last_request().starts_with("GET /v1/models "));
  }

  #[tokio::test]
  async fn a_redirect_is_an_error_and_its_target_is_never_contacted() {
    let target = serve(http("200 OK", "", r#"{"data":[{"id":"qwen"}]}"#)).await;
    let redirect = serve(http("302 Found", &format!("Location: {}/v1/models\r\n", target.base), "")).await;
    let endpoint = loopback_endpoint(&redirect.base, None);
    assert!(fetch_models(&endpoint).await.is_err());
    assert_eq!(target.hits(), 0);
  }

  #[tokio::test]
  async fn detect_sends_no_authorization_and_counts_only_2xx_answers() {
    let ok = serve(http("200 OK", "", r#"{"data":[{"id":"qwen"},{"id":"bge-m3"}]}"#)).await;
    let redirect = serve(http("302 Found", "Location: http://192.0.2.1/\r\n", "")).await;
    let found = probe_candidates(&[ok.base.as_str(), redirect.base.as_str()]).await;
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].base_url, ok.base);
    assert_eq!(found[0].models, 1);
    assert!(!ok.last_request().to_ascii_lowercase().contains("authorization"));
    assert_eq!(redirect.hits(), 1);
    assert!(!redirect.last_request().to_ascii_lowercase().contains("authorization"));
  }

  #[tokio::test]
  async fn detect_skips_servers_that_do_not_answer() {
    let closed = TcpListener::bind("127.0.0.1:0").await.unwrap().local_addr().unwrap();
    let base = format!("http://{closed}");
    let found = probe_candidates(&[base.as_str()]).await;
    assert!(found.is_empty());
  }

  #[tokio::test]
  async fn test_request_is_the_same_small_completion() {
    let reply = r#"{"choices":[{"message":{"content":"ok"},"finish_reason":"stop"}]}"#;
    let stub = serve(http("200 OK", "", reply)).await;
    let endpoint = loopback_endpoint(&stub.base, None);
    assert_eq!(run_test(&endpoint).await.unwrap(), "model_ok");
    let body = body_of(&stub.last_request());
    assert_eq!(body["max_tokens"], 16);
    assert_eq!(body["model"], "qwen");
  }

  #[tokio::test]
  async fn a_pinned_host_name_is_reached_through_the_pinned_address() {
    let reply = r#"{"choices":[{"message":{"content":"pong"},"finish_reason":"stop"}]}"#;
    let stub = serve(http("200 OK", "", reply)).await;
    let addr: SocketAddr = stub.base.trim_start_matches("http://").parse().unwrap();
    let endpoint = Endpoint {
      origin: format!("http://pin.invalid:{}", addr.port()),
      base_url: format!("http://pin.invalid:{}/v1", addr.port()),
      model: "qwen".to_string(),
      api_key: None,
      pin: Some(("pin.invalid".to_string(), vec![addr])),
    };
    assert_eq!(run_complete(&endpoint, None, "ping", 16, 0.0).await.unwrap(), "pong");
    assert_eq!(stub.hits(), 1);
  }

  #[test]
  fn endpoint_debug_never_prints_the_api_key() {
    let endpoint = loopback_endpoint("http://127.0.0.1:1234", Some("sk-test-key-123456"));
    let printed = format!("{endpoint:?}");
    assert!(!printed.contains("sk-test-key-123456"));
    assert!(printed.contains("[hidden]"));
  }
}
