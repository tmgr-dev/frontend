//! Real-build check that a plugin window is contained. Built only with `--features isolation-selftest`
//! and started with `--plugin-isolation-selftest <report.json>`: the app opens a probe page through the
//! real `plugin_window_open`, scheme handler and capabilities, the page tries to get out, and the app
//! writes what happened and exits with 0 only if nothing got through.

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Manager, Runtime};

pub const GENERATION: &str = "isolation-selftest";
const KEY: &str = "selftest.probe/page";
const ARG: &str = "--plugin-isolation-selftest";

const APP_COMMANDS: [&str; 15] = [
  "plugin_install",
  "plugin_uninstall",
  "plugin_github_release",
  "local_db_execute",
  "local_db_batch",
  "local_export_write",
  "plugin_pick_file",
  "plugin_window_reply",
  "plugin_page_put",
  "plugin_window_open",
  "plugin_windows_close",
  "plugin_fetch",
  "local_db_select",
  "plugin_catalog",
  "plugin:opener|open_url",
];

struct Run {
  report: PathBuf,
  hits: Arc<Mutex<Vec<String>>>,
  page: Mutex<Option<Value>>,
}

pub fn requested() -> Option<PathBuf> {
  let args: Vec<String> = std::env::args().collect();
  args.iter().position(|arg| arg == ARG).and_then(|i| args.get(i + 1)).map(PathBuf::from)
}

fn probe(port: u16) -> String {
  let commands = serde_json::to_string(&APP_COMMANDS).unwrap();
  format!(
    r#"<script>
(async () => {{
  const results = {{ invoke: {{}} }};
  const base = 'http://127.0.0.1:{port}';
  const invoke = window.__TAURI_INTERNALS__.invoke;
  const args = {{
    plugin_install: {{ plugin: {{ id: 'evil.plugin', repo: 'evil/plugin', tag: 'v1', sha256: '', bundle: '', signature: '', public_key: '' }} }},
    plugin_uninstall: {{ id: 'tmgr.estimate' }},
    plugin_github_release: {{ repo: 'evil/plugin' }},
    local_db_execute: {{ code: 'x', sql: 'DELETE FROM tasks', params: [] }},
    local_db_batch: {{ code: 'x', statements: [{{ sql: 'DELETE FROM tasks', params: [] }}] }},
    local_export_write: {{ code: 'x', folder: 'x', files: [] }},
    plugin_pick_file: {{ title: 'x' }},
    plugin_window_reply: {{ callId: 1, ok: true, value: null }},
    plugin_page_put: {{ key: 'other.plugin/page', html: 'x' }},
    plugin_window_open: {{ key: 'other.plugin/page', title: 'x', generation: 'x' }},
    plugin_windows_close: {{ pluginId: null }},
    plugin_fetch: {{ request: {{ url: base + '/plugin-fetch', method: 'GET', headers: [], body: null }} }},
    local_db_select: {{ code: 'x', sql: 'SELECT 1', params: [] }},
    plugin_catalog: {{}},
    'plugin:opener|open_url': {{ url: base + '/opener' }},
  }};
  for (const command of {commands}) {{
    try {{ await invoke(command, args[command]); results.invoke[command] = 'ALLOWED'; }}
    catch (error) {{ results.invoke[command] = String(error); }}
  }}
  results.echo = await tmgr.call('selftest.echo', {{ n: 1 }}).then((r) => JSON.stringify(r), (e) => 'ERR ' + e);
  try {{ localStorage.setItem('k', 'v'); results.storage = localStorage.getItem('k'); }}
  catch (error) {{ results.storage = 'ERR ' + error; }}
  const refused = (promise) => promise.then(() => 'ALLOWED', () => 'blocked');
  results.fetch = {{
    loopback: await refused(fetch(base + '/fetch')),
    internet: await refused(fetch('https://example.com/')),
    otherPlugin: await refused(fetch('tmgrplugin://other.plugin/page')),
    app: await refused(fetch('tauri://localhost/')),
  }};
  new Image().src = base + '/img';
  const frame = document.createElement('iframe');
  frame.src = base + '/iframe';
  document.body.appendChild(frame);
  try {{ new WebSocket('ws://127.0.0.1:{port}/ws'); }} catch (error) {{}}
  try {{ navigator.sendBeacon(base + '/beacon', 'x'); }} catch (error) {{}}
  results.windowOpen = String(window.open(base + '/popup'));
  await new Promise((resolve) => setTimeout(resolve, 800));
  await tmgr.call('selftest.report', results);
  location.href = base + '/navigate';
  setTimeout(() => {{ location.href = 'tmgrplugin://other.plugin/page'; }}, 300);
}})();
</script>"#
  )
}

fn listen(hits: Arc<Mutex<Vec<String>>>) -> u16 {
  let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
  let port = listener.local_addr().unwrap().port();
  std::thread::spawn(move || {
    for stream in listener.incoming().flatten() {
      let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
      let mut line = String::new();
      let _ = BufReader::new(&stream).read_line(&mut line);
      hits.lock().unwrap().push(line.trim().to_string());
      let _ = (&stream).write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok");
    }
  });
  port
}

pub fn start<R: Runtime>(app: &AppHandle<R>, report: PathBuf) -> Result<(), String> {
  let hits = Arc::new(Mutex::new(Vec::new()));
  let port = listen(hits.clone());
  // Positive control: the listener must see a request the app itself makes.
  let mut control = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
  control.write_all(b"GET /control HTTP/1.1\r\n\r\n").map_err(|e| e.to_string())?;
  app.manage(Run { report, hits, page: Mutex::new(None) });
  crate::plugin_windows::plugin_page_put(app.clone(), KEY.into(), probe(port))?;
  crate::plugin_windows::plugin_window_open(app.clone(), KEY.into(), "Isolation self-test".into(), GENERATION.into())?;
  let handle = app.clone();
  std::thread::spawn(move || {
    std::thread::sleep(Duration::from_secs(30));
    finish(&handle);
  });
  Ok(())
}

/// Answers calls from the probe window in place of the main window's broker.
pub fn answer<R: Runtime>(app: &AppHandle<R>, method: &str, params: Value) -> Result<Value, String> {
  match method {
    "selftest.echo" => Ok(params),
    "selftest.report" => {
      *app.state::<Run>().page.lock().unwrap() = Some(params);
      let handle = app.clone();
      std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        finish(&handle);
      });
      Ok(Value::Null)
    }
    _ => Err("not part of the self-test".into()),
  }
}

/// Every way out that got through; empty means the window is contained.
pub fn failures(report: &Value) -> Vec<String> {
  let mut failed = Vec::new();
  let page = &report["page"];
  if page.is_null() {
    return vec!["the probe page never reported".into()];
  }
  for command in APP_COMMANDS {
    let result = page["invoke"][command].as_str().unwrap_or("missing");
    if !result.contains("not allowed") {
      failed.push(format!("invoke {command}: {result}"));
    }
  }
  if page["echo"] != json!("{\"n\":1}") {
    failed.push(format!("the bridge did not answer: {}", page["echo"]));
  }
  if page["storage"] != json!("v") {
    failed.push(format!("own-origin storage did not work: {}", page["storage"]));
  }
  for (name, result) in page["fetch"].as_object().into_iter().flatten() {
    if result != "blocked" {
      failed.push(format!("fetch {name}: {result}"));
    }
  }
  if page["windowOpen"] != json!("null") {
    failed.push(format!("window.open returned {}", page["windowOpen"]));
  }
  let hits: Vec<&str> = report["listener"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
  if !hits.iter().any(|hit| hit.starts_with("GET /control")) {
    failed.push("the loopback listener missed its control request".into());
  }
  for hit in hits.iter().filter(|hit| !hit.starts_with("GET /control")) {
    failed.push(format!("the page reached the loopback listener: {hit}"));
  }
  if report["window_url"] != json!(format!("tmgrplugin://{KEY}")) {
    failed.push(format!("the window left its page: {}", report["window_url"]));
  }
  failed
}

fn finish<R: Runtime>(app: &AppHandle<R>) {
  let run = app.state::<Run>();
  let window_url = app
    .webview_windows()
    .into_iter()
    .find(|(label, _)| label.starts_with("plugin-"))
    .and_then(|(_, window)| window.url().ok())
    .map(|url| url.to_string());
  let mut report = json!({
    "page": run.page.lock().unwrap().clone(),
    "listener": run.hits.lock().unwrap().clone(),
    "window_url": window_url,
  });
  let failed = failures(&report);
  report["failures"] = json!(failed);
  let _ = std::fs::write(&run.report, serde_json::to_string_pretty(&report).unwrap_or_default());
  app.exit(if failed.is_empty() { 0 } else { 1 });
}

#[cfg(test)]
mod tests {
  use super::*;

  fn contained() -> Value {
    let invoke: serde_json::Map<String, Value> = APP_COMMANDS
      .iter()
      .map(|c| (c.to_string(), json!(format!("Command {c} not allowed by ACL"))))
      .collect();
    json!({
      "page": {
        "invoke": invoke,
        "echo": "{\"n\":1}",
        "storage": "v",
        "fetch": { "loopback": "blocked", "internet": "blocked" },
        "windowOpen": "null",
      },
      "listener": ["GET /control HTTP/1.1"],
      "window_url": "tmgrplugin://selftest.probe/page",
    })
  }

  #[test]
  fn a_contained_window_passes() {
    assert_eq!(failures(&contained()), Vec::<String>::new());
  }

  #[test]
  fn every_leak_and_every_missing_control_fails() {
    assert_eq!(failures(&json!({ "page": null })), vec!["the probe page never reported"]);
    let mut report = contained();
    report["page"]["invoke"]["plugin_page_put"] = json!("ALLOWED");
    report["page"]["invoke"]["local_db_select"] = json!("missing required key code");
    report["page"]["echo"] = json!("ERR timeout");
    report["page"]["fetch"]["loopback"] = json!("ALLOWED");
    report["listener"] = json!(["GET /img HTTP/1.1"]);
    report["window_url"] = json!("tmgrplugin://other.plugin/page");
    let failed = failures(&report).join("\n");
    for expected in [
      "invoke plugin_page_put",
      "invoke local_db_select",
      "bridge did not answer",
      "fetch loopback",
      "missed its control request",
      "reached the loopback listener: GET /img",
      "left its page",
    ] {
      assert!(failed.contains(expected), "{expected} not in {failed}");
    }
  }
}
