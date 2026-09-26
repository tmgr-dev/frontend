use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use tauri::menu::{CheckMenuItemBuilder, Menu, MenuBuilder, MenuItemBuilder};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_autostart::ManagerExt;

const TRAY_ID: &str = "timer";
const LABEL_MAX: usize = 42;
const TITLE_MAX: usize = 18;

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrayTask {
  pub id: i64,
  pub title: String,
  #[serde(default)]
  pub common_time: i64,
  #[serde(default)]
  pub start_time: i64,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
pub struct TrayState {
  pub running: Vec<TrayTask>,
  pub recent: Vec<TrayTask>,
}

#[derive(Default)]
pub struct TrayStore(pub Mutex<TrayState>);

pub fn now_secs() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map(|d| d.as_secs() as i64)
    .unwrap_or(0)
}

pub fn ticket_code(title: &str) -> Option<&str> {
  let (code, _) = title.split_once(": ")?;
  let (prefix, number) = code.split_once('-')?;
  let valid = !prefix.is_empty()
    && prefix.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
    && !number.is_empty()
    && number.chars().all(|c| c.is_ascii_digit());
  valid.then_some(code)
}

fn truncate(text: &str, max: usize) -> String {
  if text.chars().count() <= max {
    return text.to_owned();
  }
  let cut: String = text.chars().take(max - 1).collect();
  format!("{}…", cut.trim_end())
}

pub fn elapsed(task: &TrayTask, now: i64) -> i64 {
  let running = if task.start_time > 0 { now - task.start_time } else { 0 };
  (task.common_time + running).max(0)
}

pub fn format_elapsed(seconds: i64) -> String {
  let (h, m, s) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
  if h > 0 {
    format!("{h}:{m:02}:{s:02}")
  } else {
    format!("{m}:{s:02}")
  }
}

pub fn tray_title(state: &TrayState, now: i64) -> Option<String> {
  let task = state.running.first()?;
  let name = ticket_code(&task.title)
    .map(str::to_owned)
    .unwrap_or_else(|| truncate(&task.title, TITLE_MAX));
  let more = match state.running.len() {
    0 | 1 => String::new(),
    n => format!(" +{}", n - 1),
  };
  Some(format!("{} · {}{}", format_elapsed(elapsed(task, now)), name, more))
}

fn build_menu<R: Runtime>(app: &AppHandle<R>, state: &TrayState) -> tauri::Result<Menu<R>> {
  let mut menu = MenuBuilder::new(app);
  for task in &state.running {
    menu = menu.item(
      &MenuItemBuilder::with_id(format!("stop:{}", task.id), format!("■  Stop  {}", truncate(&task.title, LABEL_MAX)))
        .build(app)?,
    );
  }
  let recent: Vec<&TrayTask> = state
    .recent
    .iter()
    .filter(|r| !state.running.iter().any(|t| t.id == r.id))
    .collect();
  if !state.running.is_empty() && !recent.is_empty() {
    menu = menu.separator();
  }
  if !recent.is_empty() {
    menu = menu.item(&MenuItemBuilder::new("Switch to").enabled(false).build(app)?);
    for task in recent {
      menu = menu.item(
        &MenuItemBuilder::with_id(format!("switch:{}", task.id), format!("▶  {}", truncate(&task.title, LABEL_MAX)))
          .build(app)?,
      );
    }
  }
  if !state.running.is_empty() || !state.recent.is_empty() {
    menu = menu.separator();
  }
  let autostart = app.autolaunch().is_enabled().unwrap_or(false);
  menu
    .item(&MenuItemBuilder::with_id("open", "Open TMGR").build(app)?)
    .item(
      &CheckMenuItemBuilder::with_id("autostart", "Launch at Login")
        .checked(autostart)
        .build(app)?,
    )
    .separator()
    .item(
      &MenuItemBuilder::with_id("quit", "Quit TMGR")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?,
    )
    .build()
}

pub fn show_main<R: Runtime>(app: &AppHandle<R>) {
  if let Some(window) = app.get_webview_window("main") {
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
  }
}

fn tray<R: Runtime>(app: &AppHandle<R>) -> Option<TrayIcon<R>> {
  app.tray_by_id(TRAY_ID)
}

fn refresh<R: Runtime>(app: &AppHandle<R>, state: &TrayState) {
  let Some(tray) = tray(app) else { return };
  match build_menu(app, state) {
    Ok(menu) => {
      let _ = tray.set_menu(Some(menu));
    }
    Err(err) => log::error!("[tray] menu build failed: {err}"),
  }
  let _ = tray.set_title(Some(tray_title(state, now_secs()).unwrap_or_default()));
}

fn on_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
  log::info!("[tray] menu {id}");
  if let Some(task_id) = id.strip_prefix("stop:") {
    let _ = app.emit("tray://stop", task_id.parse::<i64>().unwrap_or_default());
    return;
  }
  if let Some(task_id) = id.strip_prefix("switch:") {
    let _ = app.emit("tray://switch", task_id.parse::<i64>().unwrap_or_default());
    return;
  }
  match id {
    "open" => show_main(app),
    "autostart" => {
      let launcher = app.autolaunch();
      let result = if launcher.is_enabled().unwrap_or(false) {
        launcher.disable()
      } else {
        launcher.enable()
      };
      if let Err(err) = result {
        log::error!("[tray] autostart toggle failed: {err}");
      }
      let state = app.state::<TrayStore>().0.lock().unwrap().clone();
      refresh(app, &state);
    }
    "quit" => app.exit(0),
    _ => {}
  }
}

pub fn setup<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
  let state = TrayState::default();
  TrayIconBuilder::with_id(TRAY_ID)
    .icon(tauri::include_image!("icons/tray.png"))
    .icon_as_template(true)
    .tooltip("TMGR")
    .menu(&build_menu(app, &state)?)
    .show_menu_on_left_click(true)
    .on_menu_event(|app, event| on_menu_event(app, event.id().as_ref()))
    .build(app)?;

  let ticker = app.clone();
  std::thread::spawn(move || {
    let mut last: Option<String> = None;
    loop {
      std::thread::sleep(Duration::from_millis(1000));
      let state = ticker.state::<TrayStore>().0.lock().unwrap().clone();
      let title = tray_title(&state, now_secs());
      if title != last {
        if let Some(tray) = tray(&ticker) {
          let _ = tray.set_title(Some(title.clone().unwrap_or_default()));
        }
        last = title;
      }
    }
  });
  Ok(())
}

#[tauri::command]
pub fn tray_update<R: Runtime>(app: AppHandle<R>, store: State<'_, TrayStore>, state: TrayState) {
  let mut current = store.0.lock().unwrap();
  if *current == state {
    return;
  }
  log::info!(
    "[tray] state running={:?} recent={}",
    state.running.iter().map(|t| t.id).collect::<Vec<_>>(),
    state.recent.len()
  );
  *current = state.clone();
  drop(current);
  refresh(&app, &state);
}

#[cfg(test)]
mod tests {
  use super::*;

  fn task(id: i64, title: &str, common_time: i64, start_time: i64) -> TrayTask {
    TrayTask { id, title: title.into(), common_time, start_time }
  }

  #[test]
  fn extracts_ticket_code_from_prefixed_title() {
    assert_eq!(ticket_code("TM-157: Tray timer"), Some("TM-157"));
    assert_eq!(ticket_code("TMBE2-9: x"), Some("TMBE2-9"));
    assert_eq!(ticket_code("Fix: login"), None);
    assert_eq!(ticket_code("tm-1: lower"), None);
    assert_eq!(ticket_code("No prefix"), None);
  }

  #[test]
  fn formats_elapsed_compactly() {
    assert_eq!(format_elapsed(0), "0:00");
    assert_eq!(format_elapsed(42 * 60 + 5), "42:05");
    assert_eq!(format_elapsed(3600 + 2 * 60 + 3), "1:02:03");
  }

  #[test]
  fn elapsed_adds_the_running_segment_to_common_time() {
    assert_eq!(elapsed(&task(1, "a", 100, 1_000), 1_060), 160);
    assert_eq!(elapsed(&task(1, "a", 100, 0), 1_060), 100);
  }

  #[test]
  fn title_shows_first_running_task_and_count_of_others() {
    let state = TrayState {
      running: vec![task(1, "TM-157: Tray", 0, 1_000), task(2, "Other", 0, 1_000)],
      recent: vec![],
    };
    assert_eq!(tray_title(&state, 1_000 + 42 * 60).as_deref(), Some("42:00 · TM-157 +1"));
  }

  #[test]
  fn title_falls_back_to_truncated_task_name() {
    let state = TrayState {
      running: vec![task(1, "A very long task name without a ticket", 5, 0)],
      recent: vec![],
    };
    assert_eq!(tray_title(&state, 0).as_deref(), Some("0:05 · A very long task…"));
  }

  #[test]
  fn no_title_when_nothing_runs() {
    assert_eq!(tray_title(&TrayState::default(), 0), None);
  }
}
