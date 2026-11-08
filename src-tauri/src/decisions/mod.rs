mod diff;
mod hub;
pub mod phone;
pub mod push;
mod server;
mod store;

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use tauri::utils::config::BackgroundThrottlingPolicy;
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_opener::OpenerExt;
use tmgr_ipc::decisions::{decisions_dir, Event, DB};

use diff::DiffGroups;
use hub::{pid_alive, Change, Hub};
use server::Server;
use store::{AnswerIn, CardView, MessageIn, PendingTaskComment, Store};

pub const LABEL: &str = "decisions";
pub const SHORTCUT: &str = "CmdOrCtrl+Shift+D";
pub const TRAY_ID: &str = "decisions";
const LIVENESS_EVERY: Duration = Duration::from_secs(15);

static OPEN: AtomicUsize = AtomicUsize::new(0);

pub fn open_count() -> usize {
  OPEN.load(Ordering::SeqCst)
}

pub struct Decisions {
  hub: Arc<Hub>,
  queue: Arc<push::Queue>,
  _server: Mutex<Option<Server>>,
}

fn hub_of<R: Runtime>(app: &AppHandle<R>) -> Result<Arc<Hub>, String> {
  app.try_state::<Decisions>().map(|d| d.hub.clone()).ok_or_else(|| "the decisions inbox is not running".to_owned())
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
  let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("decisions.html".into()))
    .title("Decisions · Doom Solving")
    .inner_size(1280.0, 820.0)
    .min_inner_size(980.0, 640.0)
    .resizable(true)
    .disable_drag_drop_handler()
    .background_throttling(BackgroundThrottlingPolicy::Disabled)
    .center();
  #[cfg(target_os = "macos")]
  let builder = builder.title_bar_style(tauri::TitleBarStyle::Overlay).hidden_title(true);
  match crate::with_app_webview_handlers(builder, app).build() {
    Ok(window) => {
      let _ = window.show();
      let _ = window.set_focus();
    }
    Err(e) => log::error!("[decisions] window failed: {e}"),
  }
}

fn on_change<R: Runtime>(app: &AppHandle<R>, change: &Change, count: usize) {
  OPEN.store(count, Ordering::SeqCst);
  let _ = app.emit_to(LABEL, "decisions://changed", json!({ "count": count }));
  crate::tray::refresh_current(app);
  phone::on_hub_change(app);
  if let Change::Created { title, body } = change {
    crate::plugin_notify::plugin_notify(app.clone(), title.clone(), body.clone(), None);
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
    log::warn!("[decisions] shortcut {SHORTCUT} not registered: {e}");
  }
}

pub fn setup<R: Runtime>(app: &AppHandle<R>) -> Result<(), String> {
  let dir = decisions_dir().map_err(|e| e.to_string())?;
  tmgr_ipc::dirs::ensure_private_dir(&dir).map_err(|e| e.to_string())?;
  let store = Store::open(&dir.join(DB))?;
  let handle = app.clone();
  let hub = Arc::new(
    Hub::new(store, Box::new(move |change, count| on_change(&handle, change, count))).with_spool(dir.clone()),
  );
  OPEN.store(hub.open_count(), Ordering::SeqCst);
  hub.ingest_spool(&dir);
  let server = match Server::start(hub.clone(), &dir) {
    Ok(s) => Some(s),
    Err(e) => {
      log::error!("[decisions] receiver failed to start: {e}");
      None
    }
  };
  let queue = Arc::new(push::Queue::default());
  push::spawn_worker(app, Arc::downgrade(&hub), queue.clone());
  app.manage(Decisions { hub: hub.clone(), queue, _server: Mutex::new(server) });
  phone::setup(app, hub.clone(), &dir);
  crate::tray::refresh_current(app);
  ensure_shortcut(app);

  let (weak, handle) = (Arc::downgrade(&hub), app.clone());
  let spawned = std::thread::Builder::new().name("decisions-liveness".into()).spawn(move || loop {
    std::thread::sleep(LIVENESS_EVERY);
    let Some(hub) = weak.upgrade() else { break };
    hub.check_liveness(pid_alive);
    ensure_shortcut(&handle);
  });
  if spawned.is_err() {
    log::warn!("[decisions] liveness thread failed to start");
  }
  Ok(())
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, String> + Send + 'static) -> Result<T, String> {
  tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())?
}

fn spawn_route<R: Runtime>(app: &AppHandle<R>, id: String, event: Event) {
  let app = app.clone();
  std::thread::spawn(move || {
    push::route(&app, &id, &event, None);
  });
}

#[tauri::command]
pub async fn decisions_list<R: Runtime>(app: AppHandle<R>) -> Result<Vec<CardView>, String> {
  let hub = hub_of(&app)?;
  blocking(move || hub.list()).await
}

#[tauri::command]
pub async fn decisions_get<R: Runtime>(app: AppHandle<R>, id: String) -> Result<CardView, String> {
  let hub = hub_of(&app)?;
  blocking(move || hub.get(&id)).await
}

#[tauri::command]
pub async fn decisions_answer<R: Runtime>(app: AppHandle<R>, id: String, answer: AnswerIn) -> Result<CardView, String> {
  let hub = hub_of(&app)?;
  let (event, view) = blocking(move || {
    let (event, _) = hub.answer(&id, answer)?;
    Ok((event, hub.get(&id)?))
  })
  .await?;
  spawn_route(&app, view.id.clone(), event);
  Ok(view)
}

#[tauri::command]
pub async fn decisions_message<R: Runtime>(
  app: AppHandle<R>,
  id: String,
  message: MessageIn,
) -> Result<CardView, String> {
  let hub = hub_of(&app)?;
  let (event, view) = blocking(move || {
    let event = hub.message(&id, message)?;
    Ok((event, hub.get(&id)?))
  })
  .await?;
  spawn_route(&app, view.id.clone(), event);
  Ok(view)
}

#[tauri::command]
pub async fn decisions_diff<R: Runtime>(app: AppHandle<R>, id: String) -> Result<DiffGroups, String> {
  let hub = hub_of(&app)?;
  blocking(move || {
    let (card, patches) = hub.get_with_patches(&id)?;
    let repos: Vec<_> = card.diff.iter().chain(&card.diffs).cloned().collect();
    Ok(diff::groups(&repos, &patches))
  })
  .await
}

fn main_window_only<R: Runtime>(window: &tauri::Window<R>, command: &str) -> Result<(), String> {
  if window.label() != "main" {
    return Err(format!("{command} may only be called from the main window"));
  }
  Ok(())
}

#[tauri::command]
pub async fn decisions_pending_task_comments<R: Runtime>(
  app: AppHandle<R>,
  window: tauri::Window<R>,
) -> Result<Vec<PendingTaskComment>, String> {
  main_window_only(&window, "decisions_pending_task_comments")?;
  let hub = hub_of(&app)?;
  blocking(move || hub.pending_task_comments()).await
}

#[tauri::command]
pub async fn decisions_mark_task_comment<R: Runtime>(
  app: AppHandle<R>,
  window: tauri::Window<R>,
  card_id: String,
  seq: u64,
) -> Result<(), String> {
  main_window_only(&window, "decisions_mark_task_comment")?;
  let hub = hub_of(&app)?;
  blocking(move || hub.mark_task_comment(&card_id, seq)).await
}

#[tauri::command]
pub async fn decisions_skip_task_comment<R: Runtime>(
  app: AppHandle<R>,
  window: tauri::Window<R>,
  card_id: String,
  seq: u64,
  reason: String,
) -> Result<(), String> {
  main_window_only(&window, "decisions_skip_task_comment")?;
  let hub = hub_of(&app)?;
  blocking(move || hub.skip_task_comment(&card_id, seq, &reason)).await
}

#[tauri::command]
pub fn decisions_open_link<R: Runtime>(app: AppHandle<R>, url: String) -> Result<(), String> {
  let parsed = url::Url::parse(&url).map_err(|_| "invalid link".to_owned())?;
  if !matches!(parsed.scheme(), "http" | "https") {
    return Err("only http and https links open".to_owned());
  }
  app.opener().open_url(parsed.as_str(), None::<&str>).map_err(|e| e.to_string())
}
