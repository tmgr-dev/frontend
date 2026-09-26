use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;
use tauri::http::{Request, Response, StatusCode};
use tauri::{AppHandle, Emitter, Manager, Runtime, Url, WebviewUrl, WebviewWindowBuilder, Window};
use tokio::sync::oneshot;

pub const SCHEME: &str = "tmgrplugin";
const LABEL_PREFIX: &str = "plugin-";
const MAX_PAGE_BYTES: usize = 2 * 1024 * 1024;
const CALL_TIMEOUT: Duration = Duration::from_secs(20);

/// Plugin pages run their own scripts but reach nothing: no network, no forms, no other origins.
/// Their only way out is the bridge below, through the one command this window is allowed to call.
const PAGE_CSP: &str = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; \
  img-src data: blob:; font-src data:; connect-src 'none'; form-action 'none'; base-uri 'none'; \
  frame-src 'none'; object-src 'none'";

const BRIDGE: &str = r#"<script>
(() => {
  const internals = window.__TAURI_INTERNALS__;
  const call = (method, params) => internals.invoke('plugin_window_call', { method, params: params === undefined ? null : params });
  window.tmgr = Object.freeze({
    call,
    runCommand: (id, args) => call('commands.run', { id, args: args === undefined ? null : args }),
    tasks: Object.freeze({
      list: (query) => call('tasks.list', query || {}),
      get: (id) => call('tasks.get', { id }),
      update: (id, patch) => call('tasks.update', { id, patch }),
    }),
    statuses: Object.freeze({ list: () => call('statuses.list') }),
    categories: Object.freeze({ list: () => call('categories.list') }),
    storage: Object.freeze({
      get: (key) => call('storage.get', { key }),
      set: (key, value) => call('storage.set', { key, value }),
    }),
    settings: Object.freeze({ get: () => call('settings.get') }),
    notify: (message) => call('ui.notify', { message }),
  });
})();
</script>"#;

#[derive(Default)]
pub struct PluginWindows {
  pages: Mutex<HashMap<String, String>>,
  owners: Mutex<HashMap<String, String>>,
  pending: Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>,
  next_call: AtomicU64,
  next_window: AtomicU64,
}

#[derive(Clone, Serialize)]
struct WindowCall {
  call_id: u64,
  plugin_id: String,
  method: String,
  params: Value,
}

fn valid_segment(s: &str) -> bool {
  !s.is_empty()
    && s.len() <= 80
    && s != "."
    && s != ".."
    && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

/// `<plugin id>/<view id>`, both plain words.
pub fn valid_key(key: &str) -> bool {
  let mut parts = key.split('/');
  matches!((parts.next(), parts.next(), parts.next()), (Some(a), Some(b), None) if valid_segment(a) && valid_segment(b))
}

fn percent_decode(input: &str) -> Option<String> {
  let bytes = input.as_bytes();
  let mut out = Vec::with_capacity(bytes.len());
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] == b'%' {
      let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
      out.push(u8::from_str_radix(hex, 16).ok()?);
      i += 3;
    } else {
      out.push(bytes[i]);
      i += 1;
    }
  }
  String::from_utf8(out).ok()
}

/// The bridge goes first, so it is defined before any plugin script runs.
pub fn page_with_bridge(html: &str) -> String {
  format!("<!doctype html><meta charset=\"utf-8\">{BRIDGE}{html}")
}

pub fn handle<R: Runtime>(app: &AppHandle<R>, request: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
  let key = percent_decode(request.uri().path().trim_start_matches('/')).unwrap_or_default();
  let page = valid_key(&key)
    .then(|| app.state::<PluginWindows>().pages.lock().ok().and_then(|pages| pages.get(&key).cloned()))
    .flatten();
  let (status, body) = match page {
    Some(html) => (StatusCode::OK, page_with_bridge(&html).into_bytes()),
    None => (StatusCode::NOT_FOUND, b"not found".to_vec()),
  };
  Response::builder()
    .status(status)
    .header("Content-Type", "text/html; charset=utf-8")
    .header("Content-Security-Policy", PAGE_CSP)
    .header("X-Content-Type-Options", "nosniff")
    .header("Cache-Control", "no-store")
    .body(Cow::Owned(body))
    .unwrap()
}

#[tauri::command]
pub fn plugin_page_put<R: Runtime>(app: AppHandle<R>, key: String, html: String) -> Result<(), String> {
  if !valid_key(&key) {
    return Err("bad plugin page key".into());
  }
  if html.len() > MAX_PAGE_BYTES {
    return Err("plugin page is larger than 2 MB".into());
  }
  app.state::<PluginWindows>().pages.lock().map_err(|e| e.to_string())?.insert(key, html);
  Ok(())
}

#[tauri::command]
pub fn plugin_window_open<R: Runtime>(app: AppHandle<R>, key: String, title: String) -> Result<(), String> {
  if !valid_key(&key) {
    return Err("bad plugin page key".into());
  }
  let state = app.state::<PluginWindows>();
  let plugin_id = key.split('/').next().unwrap_or_default().to_string();
  let label = format!("{LABEL_PREFIX}{}", state.next_window.fetch_add(1, Ordering::Relaxed) + 1);
  let url: Url = format!("{SCHEME}://localhost/{}", key.replace('/', "%2F")).parse().map_err(|e: url::ParseError| e.to_string())?;
  state.owners.lock().map_err(|e| e.to_string())?.insert(label.clone(), plugin_id);
  let title: String = title.chars().take(80).collect();
  WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(url))
    .title(format!("{title} (plugin)"))
    .inner_size(720.0, 560.0)
    .on_navigation(|url| url.scheme() == SCHEME || (url.scheme() == "http" && url.host_str() == Some("tmgrplugin.localhost")))
    .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
    .build()
    .map(|_| ())
    .map_err(|e| e.to_string())
}

/// The only command a plugin window may call. Its plugin comes from the window label the app gave it,
/// never from the page; the main window answers through that plugin's broker.
#[tauri::command]
pub async fn plugin_window_call<R: Runtime>(
  app: AppHandle<R>,
  window: Window<R>,
  method: String,
  params: Value,
) -> Result<Value, String> {
  let state = app.state::<PluginWindows>();
  let plugin_id = state
    .owners
    .lock()
    .map_err(|e| e.to_string())?
    .get(window.label())
    .cloned()
    .ok_or("not a plugin window")?;
  let call_id = state.next_call.fetch_add(1, Ordering::Relaxed) + 1;
  let (sender, receiver) = oneshot::channel();
  state.pending.lock().map_err(|e| e.to_string())?.insert(call_id, sender);
  let call = WindowCall { call_id, plugin_id, method: method.chars().take(80).collect(), params };
  if let Err(error) = app.emit_to("main", "plugin-window://call", call) {
    state.pending.lock().map_err(|e| e.to_string())?.remove(&call_id);
    return Err(error.to_string());
  }
  let answer = tokio::time::timeout(CALL_TIMEOUT, receiver).await;
  state.pending.lock().map_err(|e| e.to_string())?.remove(&call_id);
  match answer {
    Ok(Ok(result)) => result,
    Ok(Err(_)) => Err("the app dropped the call".into()),
    Err(_) => Err("the app did not answer in time".into()),
  }
}

#[tauri::command]
pub fn plugin_window_reply<R: Runtime>(
  app: AppHandle<R>,
  call_id: u64,
  ok: bool,
  value: Value,
) -> Result<(), String> {
  let sender = app.state::<PluginWindows>().pending.lock().map_err(|e| e.to_string())?.remove(&call_id);
  if let Some(sender) = sender {
    let _ = sender.send(if ok { Ok(value) } else { Err(value.as_str().unwrap_or("plugin error").to_string()) });
  }
  Ok(())
}

/// Closes a plugin's windows, for example when it is turned off or the workspace changes.
#[tauri::command]
pub fn plugin_windows_close<R: Runtime>(app: AppHandle<R>, plugin_id: Option<String>) -> Result<(), String> {
  let state = app.state::<PluginWindows>();
  let labels: Vec<String> = state
    .owners
    .lock()
    .map_err(|e| e.to_string())?
    .iter()
    .filter(|(_, owner)| plugin_id.as_ref().is_none_or(|id| *owner == id))
    .map(|(label, _)| label.clone())
    .collect();
  for label in labels {
    if let Some(window) = app.get_webview_window(&label) {
      let _ = window.close();
    }
    state.owners.lock().map_err(|e| e.to_string())?.remove(&label);
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn page_keys_are_a_plugin_and_a_view() {
    assert!(valid_key("tmgr.estimate/report"));
    assert!(!valid_key("tmgr.estimate"));
    assert!(!valid_key("a/b/c"));
    assert!(!valid_key("../report"));
    assert!(!valid_key("a/.."));
    assert!(!valid_key("a b/report"));
  }

  #[test]
  fn the_bridge_runs_before_the_plugin_page() {
    let page = page_with_bridge("<script>tmgr.notify('hi')</script>");
    assert!(page.find("window.tmgr").unwrap() < page.find("tmgr.notify('hi')").unwrap());
  }
}
