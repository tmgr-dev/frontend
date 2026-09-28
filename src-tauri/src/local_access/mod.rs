mod server;
mod tokens;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tokio::sync::{oneshot, watch};

pub use tokens::TokenInfo;

#[cfg(unix)]
use server::{BoxFuture, BridgeFn, Server, ServerDeps};
use tokens::{chmod, IssueError, IssueParams, KeychainSecrets, TokenStore};

const SETTINGS_FILE: &str = "settings.json";
const SOCKET_PATH_FILE: &str = "socket-path";

/// Plain data, so it compiles on every target even though only unix builds ever construct or send one:
/// `LocalAccessState.pending` below needs the type regardless of platform.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeRequest {
  pub id: u64,
  pub workspace_code: String,
  pub workspace_id: i64,
  pub persona_uuid: String,
  pub persona_name: String,
  pub token_id: String,
  pub method: String,
  pub path: String,
  pub body: Option<String>,
}

#[derive(Clone, Debug)]
pub struct BridgeReply {
  pub status: u16,
  pub body: String,
}

fn now_epoch() -> i64 {
  crate::tray::now_secs()
}

#[derive(Serialize, Deserialize, Default)]
struct Settings {
  enabled: bool,
}

fn local_access_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
  let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("local-access");
  fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
  chmod(&dir, 0o700)?;
  Ok(dir)
}

fn settings_path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
  Ok(local_access_dir(app)?.join(SETTINGS_FILE))
}

fn read_settings<R: Runtime>(app: &AppHandle<R>) -> Result<Settings, String> {
  let path = settings_path(app)?;
  if !path.exists() {
    return Ok(Settings::default());
  }
  let bytes = fs::read(&path).map_err(|e| e.to_string())?;
  if bytes.is_empty() {
    return Ok(Settings::default());
  }
  serde_json::from_slice(&bytes).map_err(|e| e.to_string())
}

fn write_settings<R: Runtime>(app: &AppHandle<R>, settings: &Settings) -> Result<(), String> {
  fs::write(settings_path(app)?, serde_json::to_vec(settings).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

fn issue_error_message(err: IssueError) -> String {
  match err {
    IssueError::BadLabel => "Label must be between 1 and 60 characters".into(),
    IssueError::BadExpiry => "Expiry must be between 1 and 365 days".into(),
    IssueError::RateLimited => "No more than 10 tokens may be issued per hour".into(),
    IssueError::Internal(message) => message,
  }
}

struct RunningServer {
  shutdown: watch::Sender<bool>,
  socket_path: PathBuf,
}

/// Owns the token store and (on unix) the running socket server. One instance is `app.manage`d.
pub struct LocalAccessState {
  token_store: Arc<TokenStore>,
  user_id: Arc<Mutex<Option<i64>>>,
  #[cfg_attr(not(unix), allow(dead_code))]
  pending: Arc<Mutex<HashMap<u64, oneshot::Sender<BridgeReply>>>>,
  /// Shared across restarts so a bridge reply from a previous run can never resolve a new request.
  #[cfg_attr(not(unix), allow(dead_code))]
  next_id: Arc<std::sync::atomic::AtomicU64>,
  running: Mutex<Option<RunningServer>>,
}

pub fn setup<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
  let dir = local_access_dir(app)?;
  let token_store = Arc::new(TokenStore::open(dir, Arc::new(KeychainSecrets))?);
  let state = LocalAccessState {
    token_store,
    user_id: Arc::new(Mutex::new(None)),
    pending: Arc::new(Mutex::new(HashMap::new())),
    next_id: Arc::new(std::sync::atomic::AtomicU64::new(0)),
    running: Mutex::new(None),
  };
  app.manage(state);
  let settings = read_settings(app)?;
  if settings.enabled && !crate::plugin_dev::plugins_safe_mode() {
    let state = app.state::<LocalAccessState>();
    ensure_started(app, &state)?;
  }
  Ok(())
}

#[cfg(unix)]
struct PendingGuard {
  pending: Arc<Mutex<HashMap<u64, oneshot::Sender<BridgeReply>>>>,
  id: u64,
  done: bool,
}

#[cfg(unix)]
impl Drop for PendingGuard {
  /// A bridge call the server gave up on (15 s timeout) still has to free its slot, even though
  /// nothing after the `.await` that owns it runs once this future is dropped.
  fn drop(&mut self) {
    if !self.done {
      if let Ok(mut pending) = self.pending.lock() {
        pending.remove(&self.id);
      }
    }
  }
}

#[cfg(unix)]
fn internal_error_reply() -> BridgeReply {
  BridgeReply { status: 500, body: r#"{"message":"Internal error","code":"INTERNAL"}"#.into() }
}

/// The real bridge: emits `local-access://request` to the main window and waits for
/// `local_access_reply`, the same call/reply shape `plugin_windows::plugin_window_call` uses.
#[cfg(unix)]
fn make_bridge<R: Runtime>(
  app: AppHandle<R>,
  pending: Arc<Mutex<HashMap<u64, oneshot::Sender<BridgeReply>>>>,
) -> BridgeFn {
  Arc::new(move |request: BridgeRequest| -> BoxFuture<BridgeReply> {
    let app = app.clone();
    let pending = pending.clone();
    Box::pin(async move {
      let (sender, receiver) = oneshot::channel();
      let id = request.id;
      match pending.lock() {
        Ok(mut map) => {
          map.insert(id, sender);
        }
        Err(_) => return internal_error_reply(),
      }
      let mut guard = PendingGuard { pending: pending.clone(), id, done: false };
      if app.emit_to("main", "local-access://request", request).is_err() {
        return internal_error_reply();
      }
      let reply = match receiver.await {
        Ok(reply) => reply,
        Err(_) => internal_error_reply(),
      };
      guard.done = true;
      reply
    })
  })
}

#[cfg(unix)]
fn ensure_started<R: Runtime>(app: &AppHandle<R>, state: &LocalAccessState) -> Result<(), String> {
  if crate::plugin_dev::plugins_safe_mode() {
    log::info!("[local-access] safe mode: the socket stays closed");
    return Ok(());
  }
  let mut running = state.running.lock().map_err(|e| e.to_string())?;
  if running.is_some() {
    return Ok(());
  }
  let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
  let local_dir = local_access_dir(app)?;
  let socket_path = server::resolve_socket_path(&app_data_dir);
  let socket_path_file = local_dir.join(SOCKET_PATH_FILE);
  if socket_path.starts_with(std::env::temp_dir()) {
    fs::write(&socket_path_file, socket_path.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
  } else {
    let _ = fs::remove_file(&socket_path_file);
  }

  let bridge = make_bridge(app.clone(), state.pending.clone());
  let user_id = state.user_id.clone();
  let app_for_workspaces = app.clone();
  let deps = ServerDeps {
    tokens: state.token_store.clone(),
    workspace_exists: Arc::new(move |code: &str| {
      crate::local_workspaces::find(&app_for_workspaces, code).ok().map(|w| w.manifest.id)
    }),
    ready: Arc::new(move || user_id.lock().map(|guard| guard.is_some()).unwrap_or(false)),
    bridge,
    app_version: app.package_info().version.to_string(),
    now_epoch: Arc::new(now_epoch),
  };
  let server = Server::new(deps, state.next_id.clone());
  let (shutdown_tx, shutdown_rx) = watch::channel(false);
  let (bound_tx, bound_rx) = oneshot::channel();
  let socket_path_for_task = socket_path.clone();
  tauri::async_runtime::spawn(async move {
    if let Err(error) = server.serve(&socket_path_for_task, shutdown_rx, Some(bound_tx)).await {
      log::error!("[local-access] server stopped: {error}");
    }
  });
  // Recorded before the bind is confirmed, so a second `ensure_started` racing with this one does
  // not also try to bind; cleared below if the bind itself turns out to have failed.
  *running = Some(RunningServer { shutdown: shutdown_tx, socket_path });
  drop(running);
  let app_for_bind_check = app.clone();
  tauri::async_runtime::spawn(async move {
    if let Ok(Err(error)) = bound_rx.await {
      log::error!("[local-access] failed to bind the local access socket: {error}");
      if let Ok(mut running) = app_for_bind_check.state::<LocalAccessState>().running.lock() {
        running.take();
      }
    }
  });
  Ok(())
}

#[cfg(not(unix))]
fn ensure_started<R: Runtime>(_app: &AppHandle<R>, _state: &LocalAccessState) -> Result<(), String> {
  log::warn!("[local-access] the local access socket is only available on macOS and Linux");
  Ok(())
}

fn stop(state: &LocalAccessState) -> Result<(), String> {
  let mut running = state.running.lock().map_err(|e| e.to_string())?;
  if let Some(running) = running.take() {
    let _ = running.shutdown.send(true);
    let _ = fs::remove_file(&running.socket_path);
  }
  Ok(())
}


#[tauri::command]
pub async fn local_token_issue<R: Runtime>(
  app: AppHandle<R>,
  state: tauri::State<'_, LocalAccessState>,
  persona_uuid: String,
  persona_name: String,
  workspace_code: String,
  label: String,
  expires_in_days: u32,
  plugin_id: Option<String>,
) -> Result<TokenInfo, String> {
  let workspace = crate::local_workspaces::find(&app, &workspace_code)?;
  let (info, _secret) = state
    .token_store
    .issue(
      IssueParams {
        persona_uuid,
        persona_name,
        workspace_code: workspace.manifest.code,
        workspace_id: workspace.manifest.id,
        label,
        expires_in_days,
        plugin_id,
      },
      now_epoch(),
    )
    .map_err(issue_error_message)?;
  write_settings(&app, &Settings { enabled: true })?;
  ensure_started(&app, &state)?;
  Ok(info)
}

#[tauri::command]
pub fn local_token_list(
  state: tauri::State<'_, LocalAccessState>,
  workspace_code: Option<String>,
) -> Result<Vec<TokenInfo>, String> {
  state.token_store.list(workspace_code.as_deref())
}

#[tauri::command]
pub fn local_token_revoke(state: tauri::State<'_, LocalAccessState>, id: String) -> Result<bool, String> {
  state.token_store.revoke(&id, now_epoch())
}

#[tauri::command]
pub fn local_token_revoke_all(
  state: tauri::State<'_, LocalAccessState>,
  persona_uuid: Option<String>,
  plugin_id: Option<String>,
  workspace_code: Option<String>,
) -> Result<usize, String> {
  state.token_store.revoke_all(persona_uuid.as_deref(), plugin_id.as_deref(), workspace_code.as_deref(), now_epoch())
}

#[tauri::command]
pub fn local_token_copy<R: Runtime>(
  app: AppHandle<R>,
  state: tauri::State<'_, LocalAccessState>,
  id: String,
) -> Result<(), String> {
  let secret = state.token_store.copy_secret(&id)?.ok_or_else(|| "token not found".to_string())?;
  use tauri_plugin_clipboard_manager::ClipboardExt;
  app.clipboard().write_text(secret).map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusInfo {
  pub enabled: bool,
  pub listening: bool,
  pub socket_path: Option<String>,
  pub safe_mode: bool,
  pub ready: bool,
  pub bridge_command: String,
}

#[tauri::command]
pub fn local_access_status<R: Runtime>(
  app: AppHandle<R>,
  state: tauri::State<'_, LocalAccessState>,
) -> Result<StatusInfo, String> {
  let settings = read_settings(&app)?;
  let running = state.running.lock().map_err(|e| e.to_string())?;
  let bridge_command = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
  Ok(StatusInfo {
    enabled: settings.enabled,
    listening: running.is_some(),
    socket_path: running.as_ref().map(|r| r.socket_path.to_string_lossy().into_owned()),
    safe_mode: crate::plugin_dev::plugins_safe_mode(),
    ready: state.user_id.lock().map_err(|e| e.to_string())?.is_some(),
    bridge_command,
  })
}

#[tauri::command]
pub fn local_access_set_enabled<R: Runtime>(
  app: AppHandle<R>,
  state: tauri::State<'_, LocalAccessState>,
  enabled: bool,
) -> Result<(), String> {
  write_settings(&app, &Settings { enabled })?;
  if enabled {
    ensure_started(&app, &state)
  } else {
    stop(&state)
  }
}

#[tauri::command]
pub fn local_access_ready<R: Runtime>(
  window: tauri::Window<R>,
  state: tauri::State<'_, LocalAccessState>,
  user_id: Option<i64>,
) -> Result<(), String> {
  if window.label() != "main" {
    return Err("local_access_ready may only be called from the main window".into());
  }
  *state.user_id.lock().map_err(|e| e.to_string())? = user_id;
  Ok(())
}

#[tauri::command]
pub fn local_access_reply<R: Runtime>(
  window: tauri::Window<R>,
  state: tauri::State<'_, LocalAccessState>,
  id: u64,
  status: u16,
  body: String,
) -> Result<(), String> {
  if window.label() != "main" {
    return Err("local_access_reply may only be called from the main window".into());
  }
  #[cfg(unix)]
  {
    let sender = state.pending.lock().map_err(|e| e.to_string())?.remove(&id);
    if let Some(sender) = sender {
      let _ = sender.send(BridgeReply { status, body });
    }
  }
  #[cfg(not(unix))]
  {
    let _ = (state, id, status, body);
  }
  Ok(())
}
