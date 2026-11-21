pub mod backend;

use std::sync::Arc;

use tauri::utils::config::BackgroundThrottlingPolicy;
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use backend::{Chat, Draft, ManagerBackend, Message, SearchHit, SearchQuery, SlashCommand, StubBackend};

pub const LABEL: &str = "manager";
pub const SHORTCUT: &str = "CmdOrCtrl+Shift+M";
pub const TRAY_ID: &str = "manager";
pub const TITLE: &str = "Manager";

pub struct ManagerState {
  backend: Arc<dyn ManagerBackend>,
}

fn backend_of<R: Runtime>(app: &AppHandle<R>) -> Result<Arc<dyn ManagerBackend>, String> {
  app.try_state::<ManagerState>().map(|m| m.backend.clone()).ok_or_else(|| "the manager chat is not running".to_owned())
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
  tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

fn focus_existing<R: Runtime>(app: &AppHandle<R>) -> bool {
  let Some(window) = app.get_webview_window(LABEL) else { return false };
  let _ = window.unminimize();
  let _ = window.show();
  let _ = window.set_focus();
  true
}

pub fn open_window<R: Runtime>(app: &AppHandle<R>) {
  if focus_existing(app) {
    return;
  }
  let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("manager.html".into()))
    .title(TITLE)
    .inner_size(1280.0, 820.0)
    .min_inner_size(900.0, 600.0)
    .resizable(true)
    .background_throttling(BackgroundThrottlingPolicy::Disabled)
    .center();
  #[cfg(target_os = "macos")]
  let builder = builder.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
  match crate::with_app_webview_handlers(builder, app).build() {
    Ok(window) => {
      let _ = window.show();
      let _ = window.set_focus();
    }
    Err(e) => log::error!("[manager] window failed: {e}"),
  }
}

fn ensure_shortcut<R: Runtime>(app: &AppHandle<R>) {
  let Ok(shortcut) = SHORTCUT.parse::<Shortcut>() else { return };
  let keys = app.global_shortcut();
  if keys.is_registered(shortcut) {
    return;
  }
  let result = keys.on_shortcut(shortcut, |app, _, event| {
    if event.state() == ShortcutState::Pressed {
      open_window(app);
    }
  });
  if let Err(e) = result {
    log::warn!("[manager] shortcut {SHORTCUT} not registered: {e}");
  }
}

pub fn setup<R: Runtime>(app: &AppHandle<R>) {
  app.manage(ManagerState { backend: Arc::new(StubBackend::default()) });
  ensure_shortcut(app);
}

#[tauri::command]
pub async fn manager_chats<R: Runtime>(app: AppHandle<R>) -> Result<Vec<Chat>, String> {
  let backend = backend_of(&app)?;
  blocking(move || backend.chats()).await
}

#[tauri::command]
pub async fn manager_messages<R: Runtime>(
  app: AppHandle<R>,
  chat_id: String,
  before: Option<String>,
  limit: Option<usize>,
) -> Result<Vec<Message>, String> {
  let backend = backend_of(&app)?;
  blocking(move || backend.messages(&chat_id, before.as_deref(), limit.unwrap_or(50).min(200))).await
}

#[tauri::command]
pub async fn manager_send<R: Runtime>(app: AppHandle<R>, chat_id: String, draft: Draft) -> Result<Message, String> {
  let backend = backend_of(&app)?;
  let message = blocking(move || backend.send(&chat_id, draft)).await?;
  let _ = tauri::Emitter::emit_to(&app, LABEL, "manager://changed", &message.chat_id);
  Ok(message)
}

#[tauri::command]
pub async fn manager_commands<R: Runtime>(app: AppHandle<R>, chat_id: String) -> Result<Vec<SlashCommand>, String> {
  let backend = backend_of(&app)?;
  blocking(move || backend.commands(&chat_id)).await
}

#[tauri::command]
pub async fn manager_search<R: Runtime>(app: AppHandle<R>, query: SearchQuery) -> Result<Vec<SearchHit>, String> {
  let backend = backend_of(&app)?;
  blocking(move || backend.search(query)).await
}

#[tauri::command]
pub async fn manager_mark_read<R: Runtime>(app: AppHandle<R>, chat_id: String) -> Result<(), String> {
  let backend = backend_of(&app)?;
  blocking(move || backend.mark_read(&chat_id)).await
}

#[tauri::command]
pub fn manager_open_window<R: Runtime>(app: AppHandle<R>) {
  open_window(&app);
}
