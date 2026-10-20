use std::path::Path;

use serde::Serialize;
use serde_json::Value;
use tmgr_ipc::api::AutopilotCommand;
use tauri::ipc::Channel;
use tauri::utils::config::BackgroundThrottlingPolicy;
use tauri::{AppHandle, Manager, Runtime, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_opener::OpenerExt;

use super::api::{self, build_start, confirm_run_message, require_caps, session_from_label, validate_start, RunnerSession};
use super::folder;
use super::task_lookup::lookup_in;
use super::pump::{TermFrame, WatchEvent};
use super::{Runner, RunnerStatus, TermInfo};

const MAX_LINK: usize = 2048;

pub fn setup<R: Runtime>(app: &AppHandle<R>) {
  let path = app
    .path()
    .app_config_dir()
    .map(|d| d.join("runner.json"))
    .unwrap_or_else(|_| std::env::temp_dir().join("tmgr-runner.json"));
  app.manage(Runner::with_default_connector(path));
}

pub fn on_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
  if matches!(event, WindowEvent::Destroyed) && window.label().starts_with("runner-") {
    if let Some(runner) = window.try_state::<Runner>() {
      runner.window_destroyed(window.label());
    }
  }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
  tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

async fn confirm<R: Runtime>(app: &AppHandle<R>, title: &'static str, message: String, ok: &'static str) -> bool {
  let app = app.clone();
  tauri::async_runtime::spawn_blocking(move || {
    app
      .dialog()
      .message(message)
      .title(title)
      .kind(MessageDialogKind::Warning)
      .buttons(MessageDialogButtons::OkCancelCustom(ok.to_owned(), "Cancel".to_owned()))
      .blocking_show()
  })
  .await
  .unwrap_or(false)
}

#[tauri::command]
pub async fn runner_allowed_list() -> Result<super::allowed::AllowedList, String> {
  blocking(|| Ok(super::allowed::read_allowed(chrono::Utc::now()))).await
}

#[tauri::command]
pub fn runner_enabled(runner: State<'_, Runner>) -> bool {
  runner.enabled()
}

#[tauri::command]
pub async fn runner_set_enabled(runner: State<'_, Runner>, enabled: bool) -> Result<(), String> {
  let runner = runner.inner().clone();
  blocking(move || runner.set_enabled(enabled)).await
}

#[tauri::command]
pub async fn runner_status(runner: State<'_, Runner>) -> Result<RunnerStatus, String> {
  let runner = runner.inner().clone();
  blocking(move || Ok(runner.status())).await
}

#[tauri::command]
pub fn runner_profiles(runner: State<'_, Runner>) -> Result<Vec<Value>, String> {
  runner.profiles()
}

#[tauri::command]
pub async fn runner_sessions(runner: State<'_, Runner>) -> Result<Vec<RunnerSession>, String> {
  let runner = runner.inner().clone();
  blocking(move || runner.sessions()).await
}

fn local_workspace_code<R: Runtime>(app: &AppHandle<R>, workspace_id: i64) -> Result<String, String> {
  if workspace_id >= 0 {
    return Err("Agent runs are available only for local workspaces".to_owned());
  }
  let root = crate::local_workspaces::root(app)?;
  crate::local_workspaces::list_in(&root)
    .into_iter()
    .find(|w| w.manifest.id == workspace_id)
    .map(|w| w.manifest.code)
    .ok_or_else(|| "workspace not found".to_owned())
}

async fn stored_folder<R: Runtime>(app: &AppHandle<R>, runner: &Runner, code: String) -> Option<String> {
  let (config, home) = (runner.config_path().to_path_buf(), app.path().home_dir().ok());
  blocking(move || Ok(folder::get(&config, &code, home.as_deref()))).await.ok().flatten()
}

async fn pick_folder<R: Runtime>(app: &AppHandle<R>, runner: &Runner, code: String) -> Result<Option<String>, String> {
  let handle = app.clone();
  let picked = tauri::async_runtime::spawn_blocking(move || {
    handle.dialog().file().set_title("Choose the project folder for the agent").blocking_pick_folder()
  })
  .await
  .map_err(|e| e.to_string())?;
  let Some(picked) = picked else { return Ok(None) };
  let path = picked.into_path().map_err(|_| "The selected folder is not a local path".to_owned())?;
  let (config, home) = (runner.config_path().to_path_buf(), app.path().home_dir().ok());
  blocking(move || folder::set(&config, &code, &path, home.as_deref())).await.map(Some)
}

#[tauri::command]
pub async fn runner_project_folder<R: Runtime>(app: AppHandle<R>, runner: State<'_, Runner>, workspace_id: i64) -> Result<Option<String>, String> {
  let runner = runner.inner().clone();
  if !runner.enabled() {
    return Err("disabled".to_owned());
  }
  let code = local_workspace_code(&app, workspace_id)?;
  Ok(stored_folder(&app, &runner, code).await)
}

#[tauri::command]
pub async fn runner_pick_folder<R: Runtime>(app: AppHandle<R>, runner: State<'_, Runner>, workspace_id: i64) -> Result<Option<String>, String> {
  let runner = runner.inner().clone();
  if !runner.enabled() {
    return Err("disabled".to_owned());
  }
  let code = local_workspace_code(&app, workspace_id)?;
  pick_folder(&app, &runner, code).await
}

#[tauri::command]
pub async fn runner_start_run<R: Runtime>(
  app: AppHandle<R>,
  runner: State<'_, Runner>,
  workspace_id: i64,
  task_id: i64,
  profile: String,
) -> Result<RunnerSession, String> {
  let runner = runner.inner().clone();
  if !runner.enabled() {
    return Err("disabled".to_owned());
  }
  validate_start(workspace_id, task_id, &profile)?;
  let code = local_workspace_code(&app, workspace_id)?;
  let root = crate::local_workspaces::root(&app)?;
  let info = blocking(move || lookup_in(&root, workspace_id, task_id)).await?;
  let caps = {
    let runner = runner.clone();
    blocking(move || runner.caps()).await?
  };
  require_caps(&caps)?;
  let chosen = match stored_folder(&app, &runner, code.clone()).await {
    Some(folder) => folder,
    None => pick_folder(&app, &runner, code).await?.ok_or_else(|| "cancelled".to_owned())?,
  };
  let home = app.path().home_dir().ok();
  let h = home.clone();
  let snapshot = blocking(move || folder::Snapshot::capture(Path::new(&chosen), h.as_deref())).await?;
  let req = build_start(&info, (workspace_id, task_id), &profile, snapshot.path(), snapshot.pin(), &caps)?;
  if !confirm(&app, "Run with agent", confirm_run_message(&req), "Run").await {
    return Err("cancelled".to_owned());
  }
  blocking(move || {
    snapshot.verify(home.as_deref())?;
    runner.start_run(&req)
  })
  .await
}

#[tauri::command]
pub async fn runner_halt<R: Runtime>(app: AppHandle<R>, runner: State<'_, Runner>, session: String) -> Result<(), String> {
  let runner = runner.inner().clone();
  api::check_session_id(&session)?;
  let (r, s) = (runner.clone(), session.clone());
  let found = blocking(move || r.sessions()).await?.into_iter().find(|x| x.id == s).ok_or_else(|| "session not found".to_owned())?;
  let message = format!("Stop agent session \"{}\"?\nThe agent process receives a termination signal.", api::display_text(&found.name));
  if !confirm(&app, "Stop agent", message, "Stop").await {
    return Err("cancelled".to_owned());
  }
  blocking(move || runner.kill(&session)).await
}

#[derive(Debug, Clone, Serialize)]
pub struct AutopilotInfo {
  pub running: bool,
  pub pid: Option<u32>,
  pub status: Value,
}

#[tauri::command]
pub async fn runner_autopilot_status(runner: State<'_, Runner>) -> Result<AutopilotInfo, String> {
  let runner = runner.inner().clone();
  blocking(move || runner.autopilot_status().map(|s| AutopilotInfo { running: s.running, pid: s.pid, status: s.status })).await
}

pub fn parse_autopilot_command(raw: &str) -> Result<AutopilotCommand, String> {
  match raw {
    "pause" => Ok(AutopilotCommand::Pause),
    "resume" => Ok(AutopilotCommand::Resume),
    "stop" => Ok(AutopilotCommand::Stop),
    _ => Err("unknown autopilot command".to_owned()),
  }
}

pub fn autopilot_stop_message(status: &Value) -> String {
  let text = |key: &str| status[key].as_str().map_or_else(|| "unknown".to_owned(), api::display_text);
  let runs = status["runs"].as_array().map_or(0, Vec::len);
  format!(
    "Stop the autopilot?\nRule: {}\nMode: {}\nActive runs: {}\nIt will stop picking up new tasks.",
    text("rule"),
    text("mode"),
    runs
  )
}

#[tauri::command]
pub async fn runner_autopilot_control<R: Runtime>(app: AppHandle<R>, runner: State<'_, Runner>, command: String) -> Result<u64, String> {
  let runner = runner.inner().clone();
  let command = parse_autopilot_command(&command)?;
  if command == AutopilotCommand::Stop {
    let r = runner.clone();
    let current = blocking(move || r.autopilot_status()).await?;
    if !confirm(&app, "Stop autopilot", autopilot_stop_message(&current.status), "Stop").await {
      return Err("cancelled".to_owned());
    }
  }
  blocking(move || runner.autopilot_control(command)).await
}

fn focus_existing<R: Runtime>(app: &AppHandle<R>, label: &str) -> Option<WebviewWindow<R>> {
  let window = app.get_webview_window(label)?;
  let _ = window.unminimize();
  let _ = window.show();
  let _ = window.set_focus();
  Some(window)
}

#[tauri::command]
pub async fn runner_open_terminal<R: Runtime>(app: AppHandle<R>, runner: State<'_, Runner>, session: String) -> Result<(), String> {
  let runner = runner.inner().clone();
  api::check_session_id(&session)?;
  let label = format!("runner-{session}");
  if focus_existing(&app, &label).is_some() {
    return Ok(());
  }
  let s = session.clone();
  if !blocking(move || runner.session_exists(&s)).await? {
    return Err("session not found".to_owned());
  }
  let builder = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("runner-terminal.html".into()))
    .title("Agent terminal")
    .inner_size(980.0, 640.0)
    .min_inner_size(480.0, 300.0)
    .resizable(true)
    .disable_drag_drop_handler()
    .background_throttling(BackgroundThrottlingPolicy::Disabled)
    .center();
  let dev = app.config().build.dev_url.clone().filter(|_| cfg!(debug_assertions));
  let window = builder
    .on_navigation(move |url| is_terminal_url(url, dev.as_ref()))
    .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
    .build()
    .map_err(|e| e.to_string())?;
  let _ = window.show();
  let _ = window.set_focus();
  Ok(())
}

#[tauri::command]
pub async fn runner_watch(runner: State<'_, Runner>, on_event: Channel<WatchEvent>) -> Result<u64, String> {
  let runner = runner.inner().clone();
  blocking(move || runner.watch(move |ev| on_event.send(ev).is_ok())).await
}

#[tauri::command]
pub fn runner_unwatch(runner: State<'_, Runner>, id: u64) {
  runner.unwatch(id);
}

fn term_session<R: Runtime>(window: &WebviewWindow<R>) -> Result<String, String> {
  session_from_label(window.label())
}

#[tauri::command]
pub async fn runner_term_attach<R: Runtime>(
  window: WebviewWindow<R>,
  runner: State<'_, Runner>,
  on_frame: Channel<TermFrame>,
  cols: u32,
  rows: u32,
) -> Result<TermInfo, String> {
  let sid = term_session(&window)?;
  let runner = runner.inner().clone();
  blocking(move || runner.term_attach(&sid, cols, rows, move |f| on_frame.send(f).is_ok())).await
}

#[tauri::command]
pub async fn runner_term_ack<R: Runtime>(window: WebviewWindow<R>, runner: State<'_, Runner>, seq: u64) -> Result<(), String> {
  runner.term_ack(&term_session(&window)?, seq)
}

#[tauri::command]
pub async fn runner_term_input<R: Runtime>(window: WebviewWindow<R>, runner: State<'_, Runner>, data: String) -> Result<(), String> {
  let (runner, sid) = (runner.inner().clone(), term_session(&window)?);
  blocking(move || runner.term_input(&sid, &data)).await
}

#[tauri::command]
pub async fn runner_term_resize<R: Runtime>(
  window: WebviewWindow<R>,
  runner: State<'_, Runner>,
  cols: u32,
  rows: u32,
) -> Result<(), String> {
  let (runner, sid) = (runner.inner().clone(), term_session(&window)?);
  blocking(move || runner.term_resize(&sid, cols, rows)).await
}

#[tauri::command]
pub async fn runner_term_scroll<R: Runtime>(window: WebviewWindow<R>, runner: State<'_, Runner>, offset: u32) -> Result<(), String> {
  let (runner, sid) = (runner.inner().clone(), term_session(&window)?);
  blocking(move || runner.term_scroll(&sid, offset)).await
}

#[tauri::command]
pub async fn runner_term_detach<R: Runtime>(window: WebviewWindow<R>, runner: State<'_, Runner>) -> Result<(), String> {
  runner.term_detach(&term_session(&window)?);
  let _ = window.close();
  Ok(())
}

pub fn is_terminal_url(url: &url::Url, dev: Option<&url::Url>) -> bool {
  if url.path() != "/runner-terminal.html" || !url.username().is_empty() || url.password().is_some() {
    return false;
  }
  let dev = dev.is_some_and(|d| d.scheme() == "http" && d.host_str() == Some("localhost") && d.port().is_some() && (d.scheme(), d.host_str(), d.port()) == (url.scheme(), url.host_str(), url.port()));
  dev
    || matches!(
      (url.scheme(), url.host_str(), url.port()),
      ("tauri", Some("localhost"), None) | ("http" | "https", Some("tauri.localhost"), None)
    )
}

pub fn validate_link(raw: &str) -> Result<url::Url, String> {
  if raw.len() > MAX_LINK {
    return Err("link too long".to_owned());
  }
  let url = url::Url::parse(raw).map_err(|_| "invalid link".to_owned())?;
  if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
    return Err("only http and https links can be opened".to_owned());
  }
  Ok(url)
}

#[tauri::command]
pub async fn runner_term_open_link<R: Runtime>(window: WebviewWindow<R>, app: AppHandle<R>, url: String) -> Result<(), String> {
  term_session(&window)?;
  let url = validate_link(&url)?;
  app.opener().open_url(url.as_str(), None::<&str>).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
  use super::{autopilot_stop_message, is_terminal_url, parse_autopilot_command, validate_link};
  use serde_json::json;
  use tmgr_ipc::api::AutopilotCommand;
  #[test]
  fn terminal_window_stays_on_its_page() {
    for ok in ["tauri://localhost/runner-terminal.html", "http://tauri.localhost/runner-terminal.html", "https://tauri.localhost/runner-terminal.html"] {
      assert!(is_terminal_url(&url::Url::parse(ok).unwrap(), None), "{ok}");
    }
    let page = url::Url::parse("http://localhost:5191/runner-terminal.html").unwrap();
    let dev = url::Url::parse("http://localhost:5191/").unwrap();
    assert!(is_terminal_url(&page, Some(&dev)));
    assert!(!is_terminal_url(&page, None));
    assert!(!is_terminal_url(&page, Some(&url::Url::parse("http://localhost:5173/").unwrap())));
    assert!(!is_terminal_url(&page, Some(&url::Url::parse("https://localhost:5191/").unwrap())));
    for bad in [
      "tauri://localhost/index.html",
      "http://localhost:5173/",
      "http://localhost/runner-terminal.html",
      "http://localhost:8080/runner-terminal.html",
      "https://localhost:5173/runner-terminal.html",
      "http://tauri.localhost:8080/runner-terminal.html",
      "http://user@tauri.localhost/runner-terminal.html",
      "https://example.com/runner-terminal.html",
      "http://tauri.localhost.evil.com/runner-terminal.html",
      "blob:http://tauri.localhost/runner-terminal.html",
      "data:text/html,x",
      "about:blank",
    ] {
      assert!(!is_terminal_url(&url::Url::parse(bad).unwrap(), Some(&dev)), "{bad}");
    }
  }

  #[test]
  fn only_http_links_open() {
    assert!(validate_link("https://example.com/a?b=1").is_ok());
    assert!(validate_link("http://localhost:3000").is_ok());
    for bad in [
      "file:///etc/passwd",
      "javascript:alert(1)",
      "mailto:a@b.c",
      "ssh://host",
      "tmgr://x",
      "http://",
      "not a url",
      "data:text/html,x",
      "",
    ] {
      assert!(validate_link(bad).is_err(), "{bad}");
    }
    assert!(validate_link(&format!("https://example.com/{}", "a".repeat(3000))).is_err());
  }

  #[test]
  fn only_known_autopilot_commands_parse() {
    assert_eq!(parse_autopilot_command("pause"), Ok(AutopilotCommand::Pause));
    assert_eq!(parse_autopilot_command("resume"), Ok(AutopilotCommand::Resume));
    assert_eq!(parse_autopilot_command("stop"), Ok(AutopilotCommand::Stop));
    for bad in ["suspend", "Stop", "", "start", "stop "] {
      assert!(parse_autopilot_command(bad).is_err(), "{bad}");
    }
  }

  #[test]
  fn stop_confirmation_shows_rule_mode_and_runs() {
    let m = autopilot_stop_message(&json!({"rule": "all", "mode": "pr", "runs": [{}, {}]}));
    assert!(m.contains("Rule: all") && m.contains("Mode: pr") && m.contains("Active runs: 2"), "{m}");
    assert!(autopilot_stop_message(&json!(null)).contains("Active runs: 0"));
  }
}
