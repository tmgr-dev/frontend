pub mod agent_models;
pub mod backend;
pub mod bridge;
pub mod composite;
pub mod local;
pub mod machines;
pub mod pair;
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
const RELAY_ENABLED: bool = true;

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
      let activity_handle = app.clone();
      bridge::spawn(
        Arc::new(bridge::RunnerSource(runner.inner().clone())),
        move |message| {
          let _ = tauri::Emitter::emit_to(&handle, LABEL, "manager://message", message);
        },
        move |event| {
          let _ = tauri::Emitter::emit_to(&activity_handle, LABEL, "manager://activity", event);
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
    let trust_handle = app.clone();
    relay::switch::spawn_poll(
      &switch,
      move |message| {
        let _ = tauri::Emitter::emit_to(&handle, LABEL, "manager://message", message);
      },
      move || trust_changed(&trust_handle),
    );
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

#[tauri::command]
pub async fn manager_queued<R: Runtime>(app: AppHandle<R>, room: RoomRef) -> Result<Vec<String>, String> {
  let backend = backend_of(&app)?;
  blocking(move || backend.queued(&room)).await
}

#[tauri::command]
pub async fn manager_cancel<R: Runtime>(app: AppHandle<R>, room: RoomRef, id: String) -> Result<bool, String> {
  let backend = backend_of(&app)?;
  blocking(move || backend.cancel(&room, &id)).await
}

#[tauri::command]
pub async fn manager_agent_models<R: Runtime>(app: AppHandle<R>, agent: String) -> Result<Vec<agent_models::ModelChoice>, String> {
  let home = app.path().home_dir().ok();
  blocking(move || agent_models::load(&agent, home.as_deref())).await
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
pub async fn manager_confirm_device_kind<R: Runtime>(app: AppHandle<R>, device_id: String, kind: String) -> Result<(), String> {
  let relay = relay_of(&app)?;
  blocking(move || relay.confirm_device_kind(&device_id, &kind)).await?;
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

#[derive(serde::Serialize)]
pub struct JoinCodeView {
  code: String,
  qr: String,
  expires_at_ms: u64,
}

fn qr_data_url(text: &str) -> Result<String, String> {
  use base64::Engine;
  let code = qrcode::QrCode::new(text.as_bytes()).map_err(|_| "could not draw the QR code".to_owned())?;
  let svg = code.render::<qrcode::render::svg::Color>().min_dimensions(320, 320).quiet_zone(true).build();
  Ok(format!("data:image/svg+xml;base64,{}", base64::engine::general_purpose::STANDARD.encode(svg)))
}

fn join_code_view(relay: &dyn relay::port::RelayPort) -> Result<JoinCodeView, String> {
  let code = relay.open_join_code()?;
  let expires_at_ms = tmgr_chat_relay::trust::decode_join_code(&code).map_err(|_| "could not read the join code".to_owned())?.exp_ms;
  let qr = qr_data_url(&code)?;
  Ok(JoinCodeView { code, qr, expires_at_ms })
}

#[tauri::command]
pub async fn manager_open_join_code<R: Runtime>(app: AppHandle<R>) -> Result<JoinCodeView, String> {
  let relay = relay_of(&app)?;
  blocking(move || join_code_view(relay.as_ref())).await
}

#[tauri::command]
pub async fn manager_close_join_code<R: Runtime>(app: AppHandle<R>, expires_at_ms: u64) -> Result<(), String> {
  let relay = relay_of(&app)?;
  blocking(move || relay.close_join_code(expires_at_ms)).await
}

#[tauri::command]
pub async fn manager_join_with_code<R: Runtime>(app: AppHandle<R>, code: String) -> Result<(), String> {
  let relay = relay_of(&app)?;
  blocking(move || relay.join_with_code(&code)).await?;
  trust_changed(&app);
  Ok(())
}

static PAIR_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
pub async fn manager_pair_local_runner<R: Runtime>(app: AppHandle<R>) -> Result<pair::PairResult, String> {
  let relay = relay_of(&app)?;
  let runner = app.try_state::<crate::runner::Runner>().map(|r| r.inner().clone());
  let out = blocking(move || {
    let runner: Box<dyn local::ChatCall> = match runner {
      Some(r) => Box::new(r),
      None => Box::new(pair::NoRunner),
    };
    Ok(pair::pair_local_runner(relay.as_ref(), &*runner, &pair::CliSpawner, &PAIR_BUSY, pair::Timeouts::default()))
  })
  .await?;
  trust_changed(&app);
  Ok(out)
}

#[tauri::command]
pub async fn manager_machines<R: Runtime>(app: AppHandle<R>) -> Result<Vec<machines::MachineInfo>, String> {
  let relay = relay_of(&app)?;
  let runner = app.try_state::<crate::runner::Runner>().map(|r| r.inner().clone());
  blocking(move || {
    let local = runner.and_then(|r| local::LocalPort::machine(&local::RunnerLocal::new(Arc::new(r))).ok());
    Ok(machines::machines(local, machines::host_name(), relay.machine_indexes()))
  })
  .await
}

fn valid_card_id(id: &str) -> bool {
  id.strip_prefix("card_").is_some_and(|hex| hex.len() == 32 && hex.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')))
}

#[tauri::command]
pub async fn manager_spawn_card<R: Runtime>(app: AppHandle<R>, card_id: String) -> Result<serde_json::Value, String> {
  if !valid_card_id(&card_id) {
    return Err("invalid card id".to_owned());
  }
  let runner = app.try_state::<crate::runner::Runner>().ok_or_else(|| "the runner is not available".to_owned())?.inner().clone();
  blocking(move || runner.chat_call("spawn.card", serde_json::json!({ "card_id": card_id }))).await
}

#[cfg(test)]
mod tests {
  use super::valid_card_id;

  #[test]
  fn card_id_needs_prefix_and_32_lowercase_hex() {
    assert!(valid_card_id("card_0123456789abcdef0123456789abcdef"));
    assert!(!valid_card_id("card_0123456789ABCDEF0123456789abcdef"));
    assert!(!valid_card_id("card_0123456789abcdef0123456789abcde"));
    assert!(!valid_card_id("card_0123456789abcdef0123456789abcdef0"));
    assert!(!valid_card_id("0123456789abcdef0123456789abcdef"));
    assert!(!valid_card_id("card_0123456789abcdef0123456789abcdeg"));
    assert!(!valid_card_id(""));
  }
}
