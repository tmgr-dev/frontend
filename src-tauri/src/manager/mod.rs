pub mod agent_models;
pub mod backend;
pub mod bridge;
pub mod composite;
pub mod followers;
pub mod local;
pub mod machines;
pub mod pair;
pub mod relay;
pub mod remote_policy;
pub mod tts;

use std::sync::Arc;

use tauri::utils::config::BackgroundThrottlingPolicy;
use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};

use backend::{ChatMessage, HistoryParams, ManagerBackend, RoomInfo, RoomRef, SearchParams, SendParams, SlashCommand, StopOutcome, StubBackend};

pub const LABEL: &str = "manager";
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
    let receipt_handle = app.clone();
    let visible_handle = app.clone();
    relay::switch::spawn_poll(
      &switch,
      move |message| {
        let _ = tauri::Emitter::emit_to(&handle, LABEL, "manager://message", composite::without_mention(message));
      },
      move |receipt| {
        let _ = tauri::Emitter::emit_to(&receipt_handle, LABEL, "manager://receipt", receipt);
      },
      move || trust_changed(&trust_handle),
      move || manager_window_visible(&visible_handle),
    );
  }
  app.manage(ManagerState { backend, relay, switch });
}

fn manager_window_visible<R: Runtime>(app: &AppHandle<R>) -> bool {
  app.get_webview_window(LABEL).is_some_and(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false))
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
pub async fn manager_put_blob<R: Runtime>(app: AppHandle<R>, request: tauri::ipc::Request<'_>) -> Result<(), String> {
  let tauri::ipc::InvokeBody::Raw(body) = request.body() else {
    return Err("file body must be raw bytes".into());
  };
  let target = request.headers().get("x-tmgr-blob").and_then(|v| v.to_str().ok()).ok_or("missing file target")?;
  let BlobTarget { room, sha256 } = serde_json::from_str(target).map_err(|e| format!("bad file target: {e}"))?;
  let backend = backend_of(&app)?;
  let body = body.clone();
  blocking(move || backend.blob_put(&room, &sha256, &body)).await
}

#[derive(serde::Deserialize)]
struct BlobTarget {
  room: RoomRef,
  sha256: String,
}

#[tauri::command]
pub async fn manager_mention_inject<R: Runtime>(app: AppHandle<R>, room: RoomRef, mention_id: String, confirmed_risky: bool) -> Result<(), String> {
  let backend = backend_of(&app)?;
  blocking(move || backend.mention_inject(&room, &mention_id, confirmed_risky)).await
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
pub async fn manager_queued_why<R: Runtime>(app: AppHandle<R>, room: RoomRef) -> Result<std::collections::HashMap<String, String>, String> {
  let backend = backend_of(&app)?;
  blocking(move || backend.queued_why(&room)).await
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

#[tauri::command]
pub async fn manager_stop<R: Runtime>(app: AppHandle<R>, room: RoomRef, force: bool) -> Result<StopOutcome, String> {
  let backend = backend_of(&app)?;
  blocking(move || backend.stop(&room, force)).await
}

#[tauri::command]
pub async fn manager_remove<R: Runtime>(app: AppHandle<R>, room: RoomRef) -> Result<(), String> {
  let backend = backend_of(&app)?;
  blocking(move || backend.remove(&room)).await
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
pub async fn manager_permit_answer<R: Runtime>(window: tauri::WebviewWindow<R>, app: AppHandle<R>, card_id: String, choice: relay::permit::PermitAnswer, confirmed: bool) -> Result<(), String> {
  if window.label() != LABEL {
    return Err("not allowed from this window".to_owned());
  }
  let relay = relay_of(&app)?;
  blocking(move || relay.answer_permit(&card_id, choice, confirmed)).await
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

static FOLLOWERS_BUSY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
pub async fn manager_connect_followers<R: Runtime>(app: AppHandle<R>) -> Result<Vec<followers::FollowerResult>, String> {
  let relay = relay_of(&app)?;
  let out = blocking(move || Ok(followers::connect_followers(relay.as_ref(), &followers::ProcLinkCli, &FOLLOWERS_BUSY))).await?;
  trust_changed(&app);
  Ok(out)
}

#[tauri::command]
pub async fn manager_followers() -> Result<Vec<followers::FollowerInfo>, String> {
  blocking(|| Ok(followers::followers(&followers::ProcLinkCli))).await
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

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct SpawnEdit {
  pub cwd: String,
  pub mode: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub task: Option<String>,
}

fn request_sha256(request: &str) -> String {
  use sha2::{Digest, Sha256};
  Sha256::digest(request.as_bytes()).iter().map(|b| format!("{b:02x}")).collect()
}

fn decide_params(card_id: &str, request: &str, decision: &str, edit: Option<&SpawnEdit>) -> Result<serde_json::Value, String> {
  if !valid_card_id(card_id) {
    return Err("invalid card id".to_owned());
  }
  if !matches!(decision, "approve" | "reject" | "edit") {
    return Err("invalid decision".to_owned());
  }
  if !matches!((decision == "edit", edit.is_some()), (true, true) | (false, false)) {
    return Err("edit is required for, and only allowed with, the edit decision".to_owned());
  }
  if let Some(e) = edit {
    if e.cwd.trim().is_empty() || !matches!(e.mode.as_str(), "ask" | "accept-edits" | "bypass") {
      return Err("invalid edit".to_owned());
    }
  }
  let mut params = serde_json::json!({ "card_id": card_id, "req_sha256": request_sha256(request), "decision": decision });
  if let Some(e) = edit {
    params["edit"] = serde_json::to_value(e).map_err(|e| e.to_string())?;
  }
  Ok(params)
}

fn outdated(err: String) -> String {
  if err.to_lowercase().contains("unknown method") {
    "runner_outdated".to_owned()
  } else {
    err
  }
}

fn runner_of<R: Runtime>(app: &AppHandle<R>) -> Result<crate::runner::Runner, String> {
  app.try_state::<crate::runner::Runner>().map(|r| r.inner().clone()).ok_or_else(|| "the runner is not available".to_owned())
}

#[tauri::command]
pub async fn manager_remote_policy<R: Runtime>(window: tauri::WebviewWindow<R>, app: AppHandle<R>) -> Result<tmgr_ipc::api::RemotePolicyInfo, String> {
  if window.label() != LABEL {
    return Err("not allowed from this window".to_owned());
  }
  let runner = runner_of(&app)?;
  blocking(move || remote_policy::get(&runner)).await
}

#[tauri::command]
pub async fn manager_remote_policy_set<R: Runtime>(
  window: tauri::WebviewWindow<R>,
  app: AppHandle<R>,
  leader: String,
  ask: bool,
) -> Result<tmgr_ipc::api::RemotePolicyInfo, String> {
  if window.label() != LABEL {
    return Err("not allowed from this window".to_owned());
  }
  let runner = runner_of(&app)?;
  blocking(move || remote_policy::set_ask(&runner, &leader, ask)).await
}

#[tauri::command]
pub async fn manager_spawn_decide<R: Runtime>(
  window: tauri::WebviewWindow<R>,
  app: AppHandle<R>,
  card_id: String,
  request: String,
  decision: String,
  edit: Option<SpawnEdit>,
) -> Result<serde_json::Value, String> {
  if window.label() != LABEL {
    return Err("not allowed from this window".to_owned());
  }
  let params = decide_params(&card_id, &request, &decision, edit.as_ref())?;
  let runner = app.try_state::<crate::runner::Runner>().ok_or_else(|| "the runner is not available".to_owned())?.inner().clone();
  blocking(move || {
    if !runner.caps()?.iter().any(|c| c == tmgr_ipc::api::caps::SPAWN_DECIDE_V1) {
      return Err("runner_outdated".to_owned());
    }
    runner.chat_call("spawn.decide", params).map_err(outdated)
  })
  .await
}

#[cfg(test)]
mod tests {
  use super::{decide_params, outdated, request_sha256, valid_card_id, SpawnEdit};

  const CARD: &str = "card_0123456789abcdef0123456789abcdef";

  fn edit() -> SpawnEdit {
    SpawnEdit { cwd: "/work".into(), mode: "ask".into(), name: None, task: Some("t".into()) }
  }

  #[test]
  fn hashes_request_bytes_as_lowercase_hex() {
    assert_eq!(request_sha256("abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
  }

  #[test]
  fn decide_params_carry_the_hash_and_validate_inputs() {
    let p = decide_params(CARD, "abc", "approve", None).unwrap();
    assert_eq!(p["req_sha256"], request_sha256("abc"));
    assert_eq!(p["decision"], "approve");
    assert!(p.get("edit").is_none());
    assert!(decide_params("card_x", "abc", "approve", None).is_err());
    assert!(decide_params(CARD, "abc", "yes", None).is_err());
    assert!(decide_params(CARD, "abc", "approve", Some(&edit())).is_err());
    assert!(decide_params(CARD, "abc", "reject", Some(&edit())).is_err());
    assert!(decide_params(CARD, "abc", "edit", None).is_err());
    let p = decide_params(CARD, "abc", "edit", Some(&edit())).unwrap();
    assert_eq!(p["edit"], serde_json::json!({"cwd": "/work", "mode": "ask", "task": "t"}));
    let mut bad = edit();
    bad.mode = "default".into();
    assert!(decide_params(CARD, "abc", "edit", Some(&bad)).is_err());
    bad = edit();
    bad.cwd = " ".into();
    assert!(decide_params(CARD, "abc", "edit", Some(&bad)).is_err());
  }

  #[test]
  fn unknown_method_maps_to_runner_outdated() {
    assert_eq!(outdated("unknown method spawn.decide".into()), "runner_outdated");
    assert_eq!(outdated("card expired".into()), "card expired");
  }

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
