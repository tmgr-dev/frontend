use serde_json::Value;
use tauri::ipc::Channel;
use tauri::utils::config::BackgroundThrottlingPolicy;
use tauri::{AppHandle, Manager, Runtime, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder, Window, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_opener::OpenerExt;

use super::api::{self, confirm_run_message, session_from_label, validate_start, RunnerSession, StartRun};
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
  let req = StartRun { workspace_id, task_id, profile };
  validate_start(&req)?;
  let root = crate::local_workspaces::root(&app)?;
  let info = blocking(move || lookup_in(&root, workspace_id, task_id)).await?;
  let message = confirm_run_message(&info.workspace, task_id, &info.title, &req.profile);
  if !confirm(&app, "Run with agent", message, "Run").await {
    return Err("cancelled".to_owned());
  }
  blocking(move || runner.start_run(&req)).await
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
  let window = builder
    .on_navigation(|url| is_terminal_url(url))
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
  runner.term_input(&term_session(&window)?, &data)
}

#[tauri::command]
pub async fn runner_term_resize<R: Runtime>(
  window: WebviewWindow<R>,
  runner: State<'_, Runner>,
  cols: u32,
  rows: u32,
) -> Result<(), String> {
  runner.term_resize(&term_session(&window)?, cols, rows)
}

#[tauri::command]
pub async fn runner_term_scroll<R: Runtime>(window: WebviewWindow<R>, runner: State<'_, Runner>, offset: u32) -> Result<(), String> {
  runner.term_scroll(&term_session(&window)?, offset)
}

#[tauri::command]
pub async fn runner_term_detach<R: Runtime>(window: WebviewWindow<R>, runner: State<'_, Runner>) -> Result<(), String> {
  runner.term_detach(&term_session(&window)?);
  let _ = window.close();
  Ok(())
}

pub fn is_terminal_url(url: &url::Url) -> bool {
  crate::is_app_url(url) && matches!(url.scheme(), "tauri" | "http" | "https") && url.path() == "/runner-terminal.html"
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
  use super::{is_terminal_url, validate_link};

  #[test]
  fn terminal_window_stays_on_its_page() {
    for ok in ["tauri://localhost/runner-terminal.html", "http://tauri.localhost/runner-terminal.html", "http://localhost:5173/runner-terminal.html"] {
      assert!(is_terminal_url(&url::Url::parse(ok).unwrap()), "{ok}");
    }
    for bad in ["tauri://localhost/index.html", "http://localhost:5173/", "https://example.com/runner-terminal.html", "data:text/html,x", "about:blank"] {
      assert!(!is_terminal_url(&url::Url::parse(bad).unwrap()), "{bad}");
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
}
