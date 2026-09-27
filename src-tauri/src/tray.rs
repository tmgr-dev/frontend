use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use tauri::menu::{CheckMenuItemBuilder, Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::tray::{TrayIcon, TrayIconBuilder};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_autostart::ManagerExt;

/// The tray shows which do-not-disturb option is active; the timing itself is owned by JS (localStorage).
const DND_OPTIONS: [(&str, &str); 4] =
  [("off", "Off"), ("1h", "For 1 hour"), ("3h", "For 3 hours"), ("tomorrow", "Until tomorrow")];

pub struct DndStore(pub Mutex<String>);

impl Default for DndStore {
  fn default() -> Self {
    Self(Mutex::new("off".to_owned()))
  }
}

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
  /// Cloud and local tasks share the tray and can share an id; the workspace tells them apart.
  #[serde(default)]
  pub workspace_id: i64,
}

impl TrayTask {
  fn key(&self) -> String {
    format!("{}:{}", self.workspace_id, self.id)
  }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct TaskRef {
  task_id: i64,
  workspace_id: i64,
}

fn task_ref(key: &str) -> TaskRef {
  let (workspace, task) = key.split_once(':').unwrap_or(("0", key));
  TaskRef {
    task_id: task.parse().unwrap_or_default(),
    workspace_id: workspace.parse().unwrap_or_default(),
  }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginTrayItem {
  pub id: String,
  pub title: String,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginTraySection {
  pub plugin_name: String,
  pub title: String,
  pub items: Vec<PluginTrayItem>,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TrayState {
  pub running: Vec<TrayTask>,
  pub recent: Vec<TrayTask>,
  #[serde(default)]
  pub plugin_sections: Vec<PluginTraySection>,
  /// Set by the one plugin chosen in Settings for the menu bar text; JS owns the choice.
  #[serde(default)]
  pub tray_title: Option<String>,
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

const PLUGIN_TITLE_MAX: usize = 12;

fn timer_title(state: &TrayState, now: i64) -> Option<String> {
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

/// Trims, drops control characters and line separators, and re-truncates: Rust never trusts JS's own checks.
fn sanitize_plugin_title(text: &str) -> Option<String> {
  let cleaned: String =
    text.chars().filter(|c| !c.is_control() && *c != '\u{2028}' && *c != '\u{2029}').collect();
  let trimmed = cleaned.trim();
  (!trimmed.is_empty()).then(|| truncate(trimmed, PLUGIN_TITLE_MAX))
}

pub fn tray_title(state: &TrayState, now: i64) -> Option<String> {
  let timer = timer_title(state, now);
  let plugin = state.tray_title.as_deref().and_then(sanitize_plugin_title);
  match (timer, plugin) {
    (Some(t), Some(p)) => Some(format!("{t} · {p}")),
    (Some(t), None) => Some(t),
    (None, Some(p)) => Some(p),
    (None, None) => None,
  }
}

fn build_menu<R: Runtime>(app: &AppHandle<R>, state: &TrayState) -> tauri::Result<Menu<R>> {
  let mut menu = MenuBuilder::new(app);
  for task in &state.running {
    menu = menu
      .item(&MenuItemBuilder::with_id(format!("open:{}", task.key()), truncate(&task.title, LABEL_MAX)).build(app)?)
      .item(&MenuItemBuilder::with_id(format!("stop:{}", task.key()), "      ■  Stop timer").build(app)?);
  }
  let recent: Vec<&TrayTask> = state
    .recent
    .iter()
    .filter(|r| !state.running.iter().any(|t| t.key() == r.key()))
    .collect();
  if !state.running.is_empty() && !recent.is_empty() {
    menu = menu.separator();
  }
  if !recent.is_empty() {
    menu = menu.item(&MenuItemBuilder::new("Switch to").enabled(false).build(app)?);
    for task in recent {
      menu = menu.item(
        &MenuItemBuilder::with_id(format!("switch:{}", task.key()), format!("▶  {}", truncate(&task.title, LABEL_MAX)))
          .build(app)?,
      );
    }
  }
  if !state.running.is_empty() || !state.recent.is_empty() {
    menu = menu.separator();
  }
  let autostart = app.autolaunch().is_enabled().unwrap_or(false);
  let dnd_current = app.state::<DndStore>().0.lock().unwrap().clone();
  let mut dnd_menu = SubmenuBuilder::new(app, "Do Not Disturb");
  for (id, label) in DND_OPTIONS {
    dnd_menu = dnd_menu.item(
      &CheckMenuItemBuilder::with_id(format!("dnd:{id}"), label)
        .checked(dnd_current == id)
        .build(app)?,
    );
  }
  menu = menu
    .item(&MenuItemBuilder::with_id("open", "Open TMGR").build(app)?)
    .item(&dnd_menu.build()?)
    .item(&MenuItemBuilder::with_id("shortcuts", "Shortcuts…").build(app)?)
    .item(
      &CheckMenuItemBuilder::with_id("autostart", "Launch at Login")
        .checked(autostart)
        .build(app)?,
    );
  if !state.plugin_sections.is_empty() {
    menu = menu.separator();
    for section in &state.plugin_sections {
      let mut submenu = SubmenuBuilder::new(app, plugin_section_label(section));
      for item in &section.items {
        submenu = submenu.item(
          &MenuItemBuilder::with_id(format!("plugin-item:{}", item.id), truncate(&item.title, LABEL_MAX))
            .build(app)?,
        );
      }
      menu = menu.item(&submenu.build()?);
    }
  }
  menu
    .separator()
    .item(
      &MenuItemBuilder::with_id("quit", "Quit TMGR")
        .accelerator("CmdOrCtrl+Q")
        .build(app)?,
    )
    .build()
}

/// Prefixed with the plugin's own name, so a section titled to look like an app item can't be mistaken for one.
fn plugin_section_label(section: &PluginTraySection) -> String {
  truncate(&format!("{}: {}", section.plugin_name, section.title), LABEL_MAX)
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
  if let Some(task_id) = id.strip_prefix("open:") {
    show_main(app);
    let _ = app.emit("tray://open", task_ref(task_id));
    return;
  }
  if let Some(task_id) = id.strip_prefix("stop:") {
    let _ = app.emit("tray://stop", task_ref(task_id));
    return;
  }
  if let Some(task_id) = id.strip_prefix("switch:") {
    let _ = app.emit("tray://switch", task_ref(task_id));
    return;
  }
  if let Some(item_id) = id.strip_prefix("plugin-item:") {
    // Whether this opens the app is up to JS, once it knows what the click resolves to.
    let _ = app.emit("tray://plugin-item", item_id.to_owned());
    return;
  }
  if let Some(option) = id.strip_prefix("dnd:") {
    *app.state::<DndStore>().0.lock().unwrap() = option.to_owned();
    let state = app.state::<TrayStore>().0.lock().unwrap().clone();
    refresh(app, &state);
    let _ = app.emit("tray://dnd", option.to_owned());
    return;
  }
  match id {
    "open" => show_main(app),
    "shortcuts" => {
      show_main(app);
      let _ = app.emit("tray://shortcuts", ());
    }
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

pub fn normalize_dnd_option(option: &str) -> &str {
  DND_OPTIONS
    .iter()
    .find_map(|(id, _)| (*id == option).then_some(*id))
    .unwrap_or("off")
}

/// Pushed from JS the same way `tray_update` is: JS owns the actual until-timestamp, this is only for the checkmark.
#[tauri::command]
pub fn dnd_update<R: Runtime>(app: AppHandle<R>, dnd: State<'_, DndStore>, option: String) {
  let option = normalize_dnd_option(&option).to_owned();
  {
    let mut current = dnd.0.lock().unwrap();
    if *current == option {
      return;
    }
    *current = option;
  }
  let state = app.state::<TrayStore>().0.lock().unwrap().clone();
  refresh(&app, &state);
}

#[cfg(test)]
mod tests {
  use super::*;

  fn task(id: i64, title: &str, common_time: i64, start_time: i64) -> TrayTask {
    TrayTask { id, title: title.into(), common_time, start_time, workspace_id: 0 }
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
      ..Default::default()
    };
    assert_eq!(tray_title(&state, 1_000 + 42 * 60).as_deref(), Some("42:00 · TM-157 +1"));
  }

  #[test]
  fn title_falls_back_to_truncated_task_name() {
    let state = TrayState {
      running: vec![task(1, "A very long task name without a ticket", 5, 0)],
      recent: vec![],
      ..Default::default()
    };
    assert_eq!(tray_title(&state, 0).as_deref(), Some("0:05 · A very long task…"));
  }

  #[test]
  fn menu_keys_carry_the_workspace() {
    assert_eq!(task_ref("-42:5"), TaskRef { task_id: 5, workspace_id: -42 });
    assert_eq!(task_ref("56:5"), TaskRef { task_id: 5, workspace_id: 56 });
    let cloud = TrayTask { workspace_id: 56, ..task(5, "a", 0, 0) };
    let local = TrayTask { workspace_id: -42, ..task(5, "a", 0, 0) };
    assert_ne!(cloud.key(), local.key());
  }

  #[test]
  fn no_title_when_nothing_runs() {
    assert_eq!(tray_title(&TrayState::default(), 0), None);
  }

  #[test]
  fn dnd_option_falls_back_to_off_when_unknown() {
    assert_eq!(normalize_dnd_option("1h"), "1h");
    assert_eq!(normalize_dnd_option("tomorrow"), "tomorrow");
    assert_eq!(normalize_dnd_option("bogus"), "off");
    assert_eq!(normalize_dnd_option(""), "off");
  }

  #[test]
  fn dnd_store_defaults_to_off() {
    assert_eq!(*DndStore::default().0.lock().unwrap(), "off");
  }

  #[test]
  fn title_combines_the_timer_and_the_chosen_plugin_text() {
    let state = TrayState {
      running: vec![task(1, "TM-157: Tray", 0, 1_000)],
      tray_title: Some("⚑ 3".into()),
      ..Default::default()
    };
    assert_eq!(tray_title(&state, 1_000 + 42 * 60).as_deref(), Some("42:00 · TM-157 · ⚑ 3"));
  }

  #[test]
  fn title_shows_the_plugin_text_alone_when_no_timer_runs() {
    let state = TrayState { tray_title: Some("3 tasks".into()), ..Default::default() };
    assert_eq!(tray_title(&state, 0).as_deref(), Some("3 tasks"));
  }

  #[test]
  fn plugin_title_is_trimmed_stripped_of_control_characters_and_re_truncated() {
    assert_eq!(sanitize_plugin_title("  ok  "), Some("ok".into()));
    assert_eq!(sanitize_plugin_title("a\u{0}b\u{2028}c"), Some("abc".into()));
    assert_eq!(sanitize_plugin_title("   "), None);
    assert_eq!(sanitize_plugin_title(""), None);
    assert_eq!(sanitize_plugin_title("a very long plugin title"), Some("a very long…".into()));
  }

  #[test]
  fn empty_or_blank_plugin_title_gives_no_title_at_all() {
    let state = TrayState { tray_title: Some("   ".into()), ..Default::default() };
    assert_eq!(tray_title(&state, 0), None);
  }

  #[test]
  fn plugin_section_label_names_the_plugin_and_truncates() {
    let section = PluginTraySection {
      plugin_name: "Sprint Board".into(),
      title: "Quick actions".into(),
      items: vec![],
    };
    assert_eq!(plugin_section_label(&section), "Sprint Board: Quick actions");
  }
}
