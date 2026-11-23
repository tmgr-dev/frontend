use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Deserialize;
use tauri::menu::{CheckMenuItemBuilder, Menu, MenuBuilder, MenuItemBuilder, SubmenuBuilder};
use tauri::tray::{TrayIcon, TrayIconBuilder};
#[cfg(any(target_os = "macos", windows))]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_autostart::ManagerExt;

use crate::native_i18n::{decisions_label, tr, Key};

/// The tray shows which do-not-disturb option is active; the timing itself is owned by JS (localStorage).
const DND_OPTIONS: [(&str, Key); 4] =
  [("off", Key::DndOff), ("1h", Key::Dnd1h), ("3h", Key::Dnd3h), ("tomorrow", Key::DndTomorrow)];

#[derive(Default)]
pub struct HotkeyStore(pub Mutex<(Option<String>, Option<String>)>);

pub(crate) fn menu_accelerator(raw: Option<&str>) -> Option<String> {
  let raw = raw?.trim();
  raw.parse::<tauri_plugin_global_shortcut::Shortcut>().ok().map(|_| raw.to_owned())
}

fn with_accelerator<R: Runtime>(
  app: &AppHandle<R>,
  id: &str,
  label: String,
  accelerator: Option<&str>,
) -> tauri::Result<tauri::menu::MenuItem<R>> {
  if let Some(accelerator) = accelerator {
    if let Ok(item) = MenuItemBuilder::with_id(id, label.clone()).accelerator(accelerator).build(app) {
      return Ok(item);
    }
  }
  MenuItemBuilder::with_id(id, label).build(app)
}

pub struct DndStore(pub Mutex<String>, pub Mutex<Option<i64>>);

impl Default for DndStore {
  fn default() -> Self {
    Self(Mutex::new("off".to_owned()), Mutex::new(None))
  }
}

impl DndStore {
  pub fn get(&self) -> (String, Option<i64>) {
    (self.0.lock().unwrap().clone(), *self.1.lock().unwrap())
  }

  /// Returns whether anything changed. "off" never carries an end time.
  pub fn set(&self, option: &str, until: Option<i64>) -> bool {
    let until = if option == "off" { None } else { until };
    let mut current = self.0.lock().unwrap();
    let mut current_until = self.1.lock().unwrap();
    if *current == option && *current_until == until {
      return false;
    }
    *current = option.to_owned();
    *current_until = until;
    true
  }
}

const TRAY_ID: &str = "timer";
/// Not `app_menu::CHECK_FOR_UPDATES_ID`: Tauri calls every menu listener for every menu event, so a shared id
/// would make one click emit `menu://check-for-updates` twice (TM-702).
const CHECK_FOR_UPDATES_ID: &str = "tray_check_for_updates";
const LABEL_MAX: usize = 42;
const TITLE_MAX: usize = 18;
/// Caps how large the plugin part of the tray menu can grow, regardless of how many plugins run.
pub(crate) const MAX_PLUGIN_SECTIONS: usize = 20;
pub(crate) const MAX_PLUGIN_ITEMS: usize = 10;

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
  pub(crate) fn key(&self) -> String {
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

pub(crate) fn truncate(text: &str, max: usize) -> String {
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

/// Unicode format (Cf) characters that can hide or reorder text: bidi overrides, joiners, zero-width marks.
fn is_format_control(c: char) -> bool {
  matches!(c as u32,
    0x00AD
      | 0x0600..=0x0605
      | 0x061C
      | 0x06DD
      | 0x070F
      | 0x180E
      | 0x200B..=0x200F
      | 0x202A..=0x202E
      | 0x2060..=0x2064
      | 0x2066..=0x206F
      | 0xFEFF
      | 0xFFF9..=0xFFFB
      | 0xE0001
      | 0xE0020..=0xE007F
  )
}

/// Maps newlines to a space, drops control/Cf characters, collapses whitespace, trims, and truncates.
pub(crate) fn sanitize_plugin_text(text: &str, max: usize) -> Option<String> {
  let mapped: String = text
    .chars()
    .map(|c| if matches!(c, '\n' | '\r' | '\u{2028}' | '\u{2029}') { ' ' } else { c })
    .filter(|c| !c.is_control() && !is_format_control(*c))
    .collect();
  let collapsed = mapped.split_whitespace().collect::<Vec<_>>().join(" ");
  (!collapsed.is_empty()).then(|| truncate(&collapsed, max))
}

fn sanitize_plugin_title(text: &str) -> Option<String> {
  sanitize_plugin_text(text, PLUGIN_TITLE_MAX)
}

/// Same cleanup as `sanitize_plugin_title`, but for a tray section title, item title or plugin name.
pub(crate) fn sanitize_plugin_label(text: &str) -> Option<String> {
  sanitize_plugin_text(text, LABEL_MAX)
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

/// Open decisions as a menu-bar prefix; empty text keeps just the badge.
pub fn with_badge(title: Option<String>, open: usize) -> Option<String> {
  if open == 0 {
    return title;
  }
  let badge = format!("● {open}");
  Some(match title {
    Some(t) => format!("{badge} {t}"),
    None => badge,
  })
}

fn open_decisions() -> usize {
  #[cfg(unix)]
  return crate::decisions::open_count();
  #[cfg(not(unix))]
  0
}

fn display_title(state: &TrayState, now: i64) -> Option<String> {
  let title = tray_title(state, now);
  if cfg!(target_os = "macos") {
    with_badge(title, open_decisions())
  } else {
    title
  }
}

fn tooltip(open: usize) -> String {
  match open {
    0 => "TMGR".to_owned(),
    1 => "TMGR · 1 decision".to_owned(),
    n => format!("TMGR · {n} decisions"),
  }
}

fn build_menu<R: Runtime>(app: &AppHandle<R>, state: &TrayState) -> tauri::Result<Menu<R>> {
  let mut menu = MenuBuilder::new(app);
  for task in &state.running {
    menu = menu
      .item(&MenuItemBuilder::with_id(format!("open:{}", task.key()), truncate(&task.title, LABEL_MAX)).build(app)?)
      .item(&MenuItemBuilder::with_id(format!("stop:{}", task.key()), format!("      ■  {}", tr(Key::StopTimer))).build(app)?);
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
    menu = menu.item(&MenuItemBuilder::new(tr(Key::SwitchTo)).enabled(false).build(app)?);
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
  let mut dnd_menu = SubmenuBuilder::new(app, tr(Key::DoNotDisturb));
  for (id, label) in DND_OPTIONS {
    dnd_menu = dnd_menu.item(
      &CheckMenuItemBuilder::with_id(format!("dnd:{id}"), tr(label))
        .checked(dnd_current == id)
        .build(app)?,
    );
  }
  let (manager_key, decisions_key) = app.try_state::<HotkeyStore>().map(|s| s.0.lock().unwrap().clone()).unwrap_or_default();
  menu = menu.item(&MenuItemBuilder::with_id("open", tr(Key::OpenApp)).build(app)?);
  #[cfg(unix)]
  {
    let label = decisions_label(crate::decisions::open_count());
    menu = menu.item(&with_accelerator(app, crate::decisions::TRAY_ID, label, decisions_key.as_deref())?);
  }
  #[cfg(not(unix))]
  let _ = decisions_key;
  menu = menu
    .item(&with_accelerator(app, crate::manager::TRAY_ID, crate::manager::TITLE.to_owned(), manager_key.as_deref())?)
    .item(&dnd_menu.build()?)
    .item(&MenuItemBuilder::with_id("shortcuts", tr(Key::Shortcuts)).build(app)?)
    .item(
      &CheckMenuItemBuilder::with_id("autostart", tr(Key::LaunchAtLogin))
        .checked(autostart)
        .build(app)?,
    );
  if cfg!(not(target_os = "macos")) && crate::updater_enabled() {
    menu = menu.item(&MenuItemBuilder::with_id(CHECK_FOR_UPDATES_ID, tr(Key::CheckForUpdates)).build(app)?);
  }
  if !state.plugin_sections.is_empty() {
    menu = menu.separator();
    for section in state.plugin_sections.iter().take(MAX_PLUGIN_SECTIONS) {
      let mut submenu = SubmenuBuilder::new(app, plugin_section_label(section));
      for item in section.items.iter().take(MAX_PLUGIN_ITEMS) {
        let title = sanitize_plugin_label(&item.title).unwrap_or_else(|| "…".to_owned());
        submenu =
          submenu.item(&MenuItemBuilder::with_id(format!("plugin-item:{}", item.id), title).build(app)?);
      }
      menu = menu.item(&submenu.build()?);
    }
  }
  menu
    .separator()
    .item(
      &MenuItemBuilder::with_id("quit", tr(Key::Quit))
        .accelerator("CmdOrCtrl+Q")
        .build(app)?,
    )
    .build()
}

/// Prefixed with the plugin's own name, so a section titled to look like an app item can't be mistaken for one.
pub(crate) fn plugin_section_label(section: &PluginTraySection) -> String {
  let name = sanitize_plugin_label(&section.plugin_name).unwrap_or_else(|| "Plugin".to_owned());
  let title = sanitize_plugin_label(&section.title).unwrap_or_default();
  truncate(&format!("{name}: {title}"), LABEL_MAX)
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

const POPOVER_TRAY: bool = cfg!(any(target_os = "macos", windows));

fn refresh<R: Runtime>(app: &AppHandle<R>, state: &TrayState) {
  if POPOVER_TRAY {
    crate::popover::emit_model(app, state);
  }
  let Some(tray) = tray(app) else { return };
  if !POPOVER_TRAY {
    match build_menu(app, state) {
      Ok(menu) => {
        let _ = tray.set_menu(Some(menu));
      }
      Err(err) => log::error!("[tray] menu build failed: {err}"),
    }
  }
  let _ = tray.set_title(Some(display_title(state, now_secs()).unwrap_or_default()));
  if cfg!(windows) {
    let _ = tray.set_tooltip(Some(tooltip(open_decisions())));
  }
}

pub fn refresh_current<R: Runtime>(app: &AppHandle<R>) {
  let Some(store) = app.try_state::<TrayStore>() else { return };
  let state = store.0.lock().unwrap().clone();
  refresh(app, &state);
}

pub(crate) fn on_menu_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
  log::info!("[tray] menu {id}");
  if let Some(task_id) = id.strip_prefix("open:") {
    // JS shows the main window only when the task has no window of its own.
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
    let _ = app.emit_to("main", "tray://plugin-item", item_id.to_owned());
    return;
  }
  if let Some(option) = id.strip_prefix("dnd:") {
    app.state::<DndStore>().set(option, None);
    let state = app.state::<TrayStore>().0.lock().unwrap().clone();
    refresh(app, &state);
    let _ = app.emit_to("main", "tray://dnd", option.to_owned());
    return;
  }
  match id {
    "open" => show_main(app),
    #[cfg(unix)]
    crate::decisions::TRAY_ID => crate::decisions::open_window(app),
    crate::manager::TRAY_ID => crate::manager::open_window(app),
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
    CHECK_FOR_UPDATES_ID => {
      show_main(app);
      let _ = app.emit("menu://check-for-updates", ());
    }
    "quit" => app.exit(0),
    _ => {}
  }
}

pub fn setup<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
  let builder = TrayIconBuilder::with_id(TRAY_ID)
    .icon(tauri::include_image!("icons/tray.png"))
    .icon_as_template(true)
    .tooltip("TMGR");
  #[cfg(any(target_os = "macos", windows))]
  let builder = {
    let menu = MenuBuilder::new(app)
      .item(&MenuItemBuilder::with_id("open", tr(Key::OpenApp)).build(app)?)
      .item(&MenuItemBuilder::with_id("quit", tr(Key::Quit)).build(app)?)
      .build()?;
    builder
      .menu(&menu)
      .show_menu_on_left_click(false)
      .on_tray_icon_event(|tray, event| {
        if let TrayIconEvent::Click { button: MouseButton::Left, button_state, rect, .. } = event {
          crate::popover::on_tray_click(tray.app_handle(), button_state == MouseButtonState::Up, rect);
        }
      })
  };
  #[cfg(not(any(target_os = "macos", windows)))]
  let builder = builder.menu(&build_menu(app, &TrayState::default())?).show_menu_on_left_click(true);
  builder
    .on_menu_event(|app, event| on_menu_event(app, event.id().as_ref()))
    .build(app)?;

  let ticker = app.clone();
  std::thread::spawn(move || {
    let mut last: Option<String> = None;
    loop {
      std::thread::sleep(Duration::from_millis(1000));
      crate::widget_state::flush_pending_reload();
      let state = ticker.state::<TrayStore>().0.lock().unwrap().clone();
      let title = display_title(&state, now_secs());
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
  crate::widget_state::publish(&app, &state);
}

pub fn normalize_dnd_option(option: &str) -> &str {
  DND_OPTIONS
    .iter()
    .find_map(|(id, _)| (*id == option).then_some(*id))
    .unwrap_or("off")
}

/// Pushed from JS the same way `tray_update` is: JS owns the timing, `until` (ms) only mirrors it for the popover.
#[tauri::command]
pub fn dnd_update<R: Runtime>(
  app: AppHandle<R>,
  dnd: State<'_, DndStore>,
  option: String,
  until: Option<i64>,
) {
  let option = normalize_dnd_option(&option).to_owned();
  if !dnd.set(&option, until) {
    return;
  }
  let state = app.state::<TrayStore>().0.lock().unwrap().clone();
  refresh(&app, &state);
}

#[tauri::command]
pub fn hotkeys_update<R: Runtime>(
  app: AppHandle<R>,
  hotkeys: State<'_, HotkeyStore>,
  manager: Option<String>,
  decisions: Option<String>,
) {
  let next = (menu_accelerator(manager.as_deref()), menu_accelerator(decisions.as_deref()));
  {
    let mut current = hotkeys.0.lock().unwrap();
    if *current == next {
      return;
    }
    *current = next;
  }
  refresh_current(&app);
}

#[tauri::command]
pub fn hotkey_open<R: Runtime>(app: AppHandle<R>, target: String) -> Result<(), String> {
  match target.as_str() {
    "manager" => crate::manager::open_window(&app),
    #[cfg(unix)]
    "decisions" => crate::decisions::open_window(&app),
    _ => return Err(format!("unknown hotkey target {target}")),
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn menu_accelerator_keeps_only_strings_the_tray_can_parse() {
    for ok in ["Alt+Shift+M", "Command+Shift+K", "Control+Alt+D", "CommandOrControl+Shift+K", "Super+Alt+M"] {
      assert_eq!(menu_accelerator(Some(ok)).as_deref(), Some(ok));
    }
    assert_eq!(menu_accelerator(Some(" Alt+Shift+M ")).as_deref(), Some("Alt+Shift+M"));
    assert_eq!(menu_accelerator(Some("Hyper+Nope")), None);
    assert_eq!(menu_accelerator(Some("")), None);
    assert_eq!(menu_accelerator(None), None);
  }

  fn task(id: i64, title: &str, common_time: i64, start_time: i64) -> TrayTask {
    TrayTask { id, title: title.into(), common_time, start_time, workspace_id: 0 }
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn app_menu_ids_are_not_handled_by_the_tray() {
    use crate::app_menu::{CHECK_FOR_UPDATES_ID as APP_UPDATES, NAV_BACK_ID, NAV_FORWARD_ID};
    let tray_ids = ["open", "shortcuts", "autostart", CHECK_FOR_UPDATES_ID, "quit"];
    let tray_prefixes = ["open:", "stop:", "switch:", "plugin-item:", "dnd:"];
    for id in [APP_UPDATES, NAV_BACK_ID, NAV_FORWARD_ID] {
      assert!(!tray_ids.contains(&id), "{id} is handled by both menus");
      assert!(!tray_prefixes.iter().any(|p| id.starts_with(p)), "{id} matches a tray prefix");
    }
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
  fn dnd_until_round_trips_and_off_clears_it() {
    let store = DndStore::default();
    assert_eq!(store.get(), ("off".to_owned(), None));
    assert!(store.set("1h", Some(1_000)));
    assert_eq!(store.get(), ("1h".to_owned(), Some(1_000)));
    assert!(!store.set("1h", Some(1_000)));
    assert!(store.set("1h", Some(2_000)));
    assert!(store.set("off", Some(5)));
    assert_eq!(store.get(), ("off".to_owned(), None));
  }

  #[test]
  fn badge_prefixes_the_open_decisions_count() {
    assert_eq!(with_badge(Some("1:00 · TM-1".into()), 0).as_deref(), Some("1:00 · TM-1"));
    assert_eq!(with_badge(None, 0), None);
    assert_eq!(with_badge(Some("1:00 · TM-1".into()), 2).as_deref(), Some("● 2 1:00 · TM-1"));
    assert_eq!(with_badge(None, 2).as_deref(), Some("● 2"));
    assert_eq!(tooltip(0), "TMGR");
    assert_eq!(tooltip(2), "TMGR · 2 decisions");
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
    assert_eq!(sanitize_plugin_title("a\u{0}b\u{2028}c"), Some("ab c".into()));
    assert_eq!(sanitize_plugin_title("   "), None);
    assert_eq!(sanitize_plugin_title(""), None);
    assert_eq!(sanitize_plugin_title("a very long plugin title"), Some("a very long…".into()));
  }

  #[test]
  fn plugin_text_collapses_newlines_and_runs_of_whitespace_to_a_single_space() {
    assert_eq!(sanitize_plugin_label("Open\ntask\r\nnow"), Some("Open task now".into()));
    assert_eq!(sanitize_plugin_label("a   b"), Some("a b".into()));
  }

  #[test]
  fn plugin_text_strips_unicode_format_characters() {
    assert_eq!(sanitize_plugin_label("a\u{200b}b\u{202e}c"), Some("abc".into()));
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

  #[test]
  fn plugin_section_label_sanitizes_the_name_and_the_title_separately() {
    let section = PluginTraySection {
      plugin_name: "Sprint\u{200b}Board".into(),
      title: "Quick\nactions".into(),
      items: vec![],
    };
    assert_eq!(plugin_section_label(&section), "SprintBoard: Quick actions");
  }
}
