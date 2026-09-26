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
const MAX_CALL_BYTES: usize = 1024 * 1024;
const MAX_IN_FLIGHT: usize = 16;

/// Plugin pages run their own scripts but reach nothing: no network, no forms, no other origins.
/// Their only way out is the bridge below, through the one command this window is allowed to call.
const PAGE_CSP: &str = "default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; \
  img-src data: blob:; font-src data:; connect-src 'none'; form-action 'none'; base-uri 'none'; \
  frame-src 'none'; object-src 'none'; frame-ancestors 'none'";

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

#[derive(Clone)]
struct Owner {
  plugin_id: String,
  generation: String,
}

#[derive(Default)]
pub struct PluginWindows {
  pages: Mutex<HashMap<String, String>>,
  owners: Mutex<HashMap<String, Owner>>,
  in_flight: Mutex<HashMap<String, usize>>,
  pending: Mutex<HashMap<u64, oneshot::Sender<Result<Value, String>>>>,
  next_call: AtomicU64,
  next_window: AtomicU64,
}

#[derive(Clone, Serialize)]
struct WindowCall {
  call_id: u64,
  plugin_id: String,
  generation: String,
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

/// Each plugin gets its own origin, `tmgrplugin://<plugin id>/<view>`, so plugins share no storage.
pub fn page_url(key: &str) -> Result<Url, String> {
  let (plugin, view) = key.split_once('/').ok_or("bad plugin page key")?;
  format!("{SCHEME}://{plugin}/{view}").parse().map_err(|e: url::ParseError| e.to_string())
}

fn key_of(uri: &tauri::http::Uri) -> Option<String> {
  let key = format!("{}/{}", uri.host()?, uri.path().trim_start_matches('/'));
  valid_key(&key).then_some(key)
}

/// A page is served only to a window the app opened for that plugin; any other webview gets nothing.
fn may_serve(owners: &HashMap<String, Owner>, webview: &str, key: &str) -> bool {
  owners.get(webview).is_some_and(|owner| key.split('/').next() == Some(owner.plugin_id.as_str()))
}

/// The bridge goes first, so it is defined before any plugin script runs.
pub fn page_with_bridge(html: &str) -> String {
  format!("<!doctype html><meta charset=\"utf-8\">{BRIDGE}{html}")
}

pub fn handle<R: Runtime>(app: &AppHandle<R>, webview: &str, request: Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> {
  let state = app.state::<PluginWindows>();
  let page = key_of(request.uri())
    .filter(|key| state.owners.lock().is_ok_and(|owners| may_serve(&owners, webview, key)))
    .and_then(|key| state.pages.lock().ok().and_then(|pages| pages.get(&key).cloned()));
  let (status, body) = match page {
    Some(html) => (StatusCode::OK, page_with_bridge(&html).into_bytes()),
    None => (StatusCode::NOT_FOUND, b"not found".to_vec()),
  };
  Response::builder()
    .status(status)
    .header("Content-Type", "text/html; charset=utf-8")
    .header("Content-Security-Policy", PAGE_CSP)
    .header("X-Content-Type-Options", "nosniff")
    .header("X-DNS-Prefetch-Control", "off")
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

/// `generation` names the plugin run that opened the window; calls from it are refused once that run ends.
#[tauri::command]
pub fn plugin_window_open<R: Runtime>(
  app: AppHandle<R>,
  key: String,
  title: String,
  generation: String,
) -> Result<(), String> {
  if !valid_key(&key) {
    return Err("bad plugin page key".into());
  }
  let state = app.state::<PluginWindows>();
  let url = page_url(&key)?;
  let plugin_id = key.split('/').next().unwrap_or_default().to_string();
  let label = format!("{LABEL_PREFIX}{}", state.next_window.fetch_add(1, Ordering::Relaxed) + 1);
  state
    .owners
    .lock()
    .map_err(|e| e.to_string())?
    .insert(label.clone(), Owner { plugin_id, generation: generation.chars().take(40).collect() });
  let title: String = title.chars().take(60).collect();
  let allowed = url.clone();
  WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(url))
    .title(format!("Plugin · {title}"))
    .inner_size(720.0, 560.0)
    // The window may only ever show the page it was opened for.
    .on_navigation(move |to| {
      to.scheme() == allowed.scheme() && to.host_str() == allowed.host_str() && to.path() == allowed.path()
    })
    .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
    .build()
    .map(|_| ())
    .map_err(|e| e.to_string())
}

struct InFlight<'a> {
  map: &'a Mutex<HashMap<String, usize>>,
  label: String,
}

impl Drop for InFlight<'_> {
  fn drop(&mut self) {
    if let Ok(mut map) = self.map.lock() {
      if let Some(count) = map.get_mut(&self.label) {
        *count = count.saturating_sub(1);
      }
    }
  }
}

/// The only command a plugin window may call. Its plugin and run come from what the app recorded for the
/// window label, never from the page; the main window answers through that plugin's broker.
#[tauri::command]
pub async fn plugin_window_call<R: Runtime>(
  app: AppHandle<R>,
  window: Window<R>,
  method: String,
  params: Value,
) -> Result<Value, String> {
  let state = app.state::<PluginWindows>();
  let owner =
    state.owners.lock().map_err(|e| e.to_string())?.get(window.label()).cloned().ok_or("not a plugin window")?;
  if serde_json::to_string(&params).map_err(|e| e.to_string())?.len() > MAX_CALL_BYTES {
    return Err("call arguments are larger than 1 MB".into());
  }
  let _slot = {
    let mut in_flight = state.in_flight.lock().map_err(|e| e.to_string())?;
    let count = in_flight.entry(window.label().to_string()).or_insert(0);
    if *count >= MAX_IN_FLIGHT {
      return Err("RATE_LIMITED: too many calls in flight".into());
    }
    *count += 1;
    InFlight { map: &state.in_flight, label: window.label().to_string() }
  };
  #[cfg(feature = "isolation-selftest")]
  if owner.generation == crate::plugin_selftest::GENERATION {
    return crate::plugin_selftest::answer(&app, &method, params);
  }
  let call_id = state.next_call.fetch_add(1, Ordering::Relaxed) + 1;
  let (sender, receiver) = oneshot::channel();
  state.pending.lock().map_err(|e| e.to_string())?.insert(call_id, sender);
  let call = WindowCall {
    call_id,
    plugin_id: owner.plugin_id,
    generation: owner.generation,
    method: method.chars().take(80).collect(),
    params,
  };
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
pub fn plugin_window_reply<R: Runtime>(app: AppHandle<R>, call_id: u64, ok: bool, value: Value) -> Result<(), String> {
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
    .filter(|(_, owner)| plugin_id.as_ref().is_none_or(|id| owner.plugin_id == *id))
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
  fn each_plugin_has_its_own_origin() {
    assert_eq!(page_url("tmgr.estimate/report").unwrap().as_str(), "tmgrplugin://tmgr.estimate/report");
    assert_ne!(page_url("a.one/x").unwrap().origin(), page_url("b.two/x").unwrap().origin());
  }

  #[test]
  fn pages_are_served_only_to_their_plugin_window() {
    let owner = |plugin: &str| Owner { plugin_id: plugin.into(), generation: "1".into() };
    let owners = HashMap::from([("plugin-1".to_string(), owner("a.one")), ("plugin-2".to_string(), owner("b.two"))]);
    assert!(may_serve(&owners, "plugin-1", "a.one/page"));
    assert!(!may_serve(&owners, "plugin-1", "b.two/page"));
    assert!(!may_serve(&owners, "main", "a.one/page"));
    assert!(!may_serve(&owners, "quick-add", "a.one/page"));
  }

  #[test]
  fn the_bridge_runs_before_the_plugin_page() {
    let page = page_with_bridge("<script>tmgr.notify('hi')</script>");
    assert!(page.find("window.tmgr").unwrap() < page.find("tmgr.notify('hi')").unwrap());
  }
}
