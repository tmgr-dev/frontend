pub mod backend;
pub mod bridge;
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
  switch: Arc<relay::switch::SwitchRelay>,
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

/// Flip once the chat-relay backend (Liquibase 230) ships and `relay_engine` has a real login token and user id.
const RELAY_ENABLED: bool = false;

pub fn setup<R: Runtime>(app: &AppHandle<R>) {
  let runner = app.try_state::<crate::runner::Runner>().map(|r| Arc::new(local::RunnerLocal::new(Arc::new(r.inner().clone()))));
  let machine_source = runner.clone();
  let machine: relay::switch::MachineSource = Box::new(move || match &machine_source {
    Some(local) => local::LocalPort::machine(&**local),
    None => Err("the runner is not available".to_owned()),
  });
  let switch = Arc::new(relay::switch::SwitchRelay::new(RELAY_ENABLED, relay::switch::keychain_builder(), machine));
  let relay: Arc<dyn relay::port::RelayPort> = switch.clone();
  let backend: Arc<dyn ManagerBackend> = match (app.try_state::<crate::runner::Runner>(), runner) {
    (Some(runner), Some(local)) => {
      let handle = app.clone();
      let resync_handle = app.clone();
      bridge::spawn(
        Arc::new(bridge::RunnerSource(runner.inner().clone())),
        move |message| {
          let _ = tauri::Emitter::emit_to(&handle, LABEL, "manager://message", message);
        },
        move || {
          let _ = tauri::Emitter::emit_to(&resync_handle, LABEL, "manager://resync", ());
        },
      );
      Arc::new(composite::CompositeBackend::new(local, relay.clone()))
    }
    _ => Arc::new(StubBackend::default()),
  };
  if switch.enabled() {
    let handle = app.clone();
    relay::switch::spawn_poll(&switch, move |message| {
      let _ = tauri::Emitter::emit_to(&handle, LABEL, "manager://message", message);
    });
  }
  app.manage(ManagerState { backend, relay, switch });
  ensure_shortcut(app);
}

fn switch_of<R: Runtime>(app: &AppHandle<R>) -> Result<Arc<relay::switch::SwitchRelay>, String> {
  app.try_state::<ManagerState>().map(|m| m.switch.clone()).ok_or_else(|| "the manager chat is not running".to_owned())
}

#[tauri::command]
pub async fn manager_set_relay_auth<R: Runtime>(app: AppHandle<R>, token: String, user_id: u64, api_base: String) -> Result<(), String> {
  let auth = relay::auth::RelayAuth::new(token, user_id, &api_base)?;
  let switch = switch_of(&app)?;
  blocking(move || {
    switch.set_auth(auth);
    Ok(())
  })
  .await?;
  trust_changed(&app);
  Ok(())
}

#[tauri::command]
pub async fn manager_clear_relay_auth<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
  let switch = switch_of(&app)?;
  blocking(move || {
    switch.clear();
    Ok(())
  })
  .await?;
  trust_changed(&app);
  Ok(())
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
