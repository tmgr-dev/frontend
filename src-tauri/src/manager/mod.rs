pub mod backend;
pub mod composite;
pub mod local;
pub mod relay;

use std::sync::Arc;

use tauri::utils::config::BackgroundThrottlingPolicy;
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

use backend::{ChatMessage, HistoryParams, ManagerBackend, RoomInfo, RoomRef, SearchParams, SendParams, SlashCommand, StubBackend};

pub const LABEL: &str = "manager";
pub const SHORTCUT: &str = "CmdOrCtrl+Shift+M";
pub const TRAY_ID: &str = "manager";
pub const TITLE: &str = "Manager";

pub struct ManagerState {
  backend: Arc<dyn ManagerBackend>,
  relay: Arc<dyn relay::port::RelayPort>,
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
  // STUB: NoRelay until the chat-relay backend (Liquibase 230) ships and a real Transport exists
  let relay: Arc<dyn relay::port::RelayPort> = Arc::new(relay::port::NoRelay);
  let backend: Arc<dyn ManagerBackend> = match app.try_state::<crate::runner::Runner>() {
    Some(runner) => Arc::new(composite::CompositeBackend::new(
      Arc::new(local::RunnerLocal::new(Arc::new(runner.inner().clone()))),
      relay.clone(),
    )),
    None => Arc::new(StubBackend::default()),
  };
  app.manage(ManagerState { backend, relay });
  ensure_shortcut(app);
}

#[tauri::command]
pub async fn manager_rooms<R: Runtime>(app: AppHandle<R>) -> Result<Vec<RoomInfo>, String> {
  let backend = backend_of(&app)?;
  blocking(move || backend.rooms()).await
}

#[tauri::command]
pub async fn manager_history<R: Runtime>(app: AppHandle<R>, params: HistoryParams) -> Result<Vec<ChatMessage>, String> {
  let backend = backend_of(&app)?;
  blocking(move || backend.history(params)).await
}

#[tauri::command]
pub async fn manager_send<R: Runtime>(app: AppHandle<R>, params: SendParams) -> Result<ChatMessage, String> {
  let backend = backend_of(&app)?;
  blocking(move || backend.send(params)).await
}

#[tauri::command]
pub async fn manager_search<R: Runtime>(app: AppHandle<R>, params: SearchParams) -> Result<Vec<ChatMessage>, String> {
  let backend = backend_of(&app)?;
  blocking(move || backend.search(params)).await
}

#[tauri::command]
pub async fn manager_commands<R: Runtime>(app: AppHandle<R>, room: RoomRef) -> Result<Vec<SlashCommand>, String> {
  let backend = backend_of(&app)?;
  blocking(move || backend.commands(&room)).await
}

fn relay_of<R: Runtime>(app: &AppHandle<R>) -> Result<Arc<dyn relay::port::RelayPort>, String> {
  app.try_state::<ManagerState>().map(|m| m.relay.clone()).ok_or_else(|| "the manager chat is not running".to_owned())
}

fn trust_changed<R: Runtime>(app: &AppHandle<R>) {
  let _ = tauri::Emitter::emit_to(app, LABEL, "manager://trust", ());
}

#[tauri::command]
pub async fn manager_trust_state<R: Runtime>(app: AppHandle<R>) -> Result<relay::port::TrustState, String> {
  let relay = relay_of(&app)?;
  blocking(move || relay.trust_state()).await
}

#[tauri::command]
pub async fn manager_join_decide<R: Runtime>(app: AppHandle<R>, device_id: String, accept: bool) -> Result<(), String> {
  let relay = relay_of(&app)?;
  blocking(move || relay.decide_join(&device_id, accept)).await?;
  trust_changed(&app);
  Ok(())
}

#[tauri::command]
pub async fn manager_revoke_decide<R: Runtime>(app: AppHandle<R>, id: String, accept: bool) -> Result<(), String> {
  let relay = relay_of(&app)?;
  blocking(move || relay.decide_revoke(&id, accept)).await?;
  trust_changed(&app);
  Ok(())
}

#[tauri::command]
pub async fn manager_revoke<R: Runtime>(app: AppHandle<R>, device_id: String) -> Result<(), String> {
  let relay = relay_of(&app)?;
  blocking(move || relay.revoke(&device_id)).await?;
  trust_changed(&app);
  Ok(())
}

#[tauri::command]
pub async fn manager_reset_identity<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
  let relay = relay_of(&app)?;
  blocking(move || relay.reset_identity()).await?;
  trust_changed(&app);
  Ok(())
}
