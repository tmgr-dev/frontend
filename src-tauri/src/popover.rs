#![cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Runtime, State, WindowEvent};
use tauri_plugin_autostart::ManagerExt;

use crate::tray::{
  normalize_dnd_option, plugin_section_label, sanitize_plugin_label, DndStore, TrayState, TrayStore, TrayTask,
  MAX_PLUGIN_ITEMS, MAX_PLUGIN_SECTIONS,
};

pub const LABEL: &str = "popover";

const WIDTH: f64 = 320.0;
const DEFAULT_HEIGHT: f64 = 400.0;
const MIN_HEIGHT: f64 = 80.0;
const MAX_HEIGHT: f64 = 900.0;
const GAP: f64 = 6.0;
const EDGE_MARGIN: f64 = 8.0;
const BLUR_GRACE_MS: u64 = 150;
const TOGGLE_DEBOUNCE_MS: u64 = 250;
const SHOW_FALLBACK: Duration = Duration::from_millis(400);
const QUICK_ADD_MAX: usize = 200;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Area {
  pub x: f64,
  pub y: f64,
  pub w: f64,
  pub h: f64,
}

impl Area {
  pub fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
    Self { x, y, w, h }
  }
  fn right(&self) -> f64 {
    self.x + self.w
  }
  fn bottom(&self) -> f64 {
    self.y + self.h
  }
  fn cx(&self) -> f64 {
    self.x + self.w / 2.0
  }
  fn cy(&self) -> f64 {
    self.y + self.h / 2.0
  }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Edge {
  Top,
  Bottom,
  Left,
  Right,
}

/// The side the taskbar sits on: where the work area is smaller than the monitor, else the nearest monitor edge to the icon.
pub fn taskbar_edge(monitor: Area, work: Area, icon: Area) -> Edge {
  if work.y > monitor.y {
    Edge::Top
  } else if work.bottom() < monitor.bottom() {
    Edge::Bottom
  } else if work.x > monitor.x {
    Edge::Left
  } else if work.right() < monitor.right() {
    Edge::Right
  } else {
    let distances = [
      (Edge::Top, icon.cy() - monitor.y),
      (Edge::Bottom, monitor.bottom() - icon.cy()),
      (Edge::Left, icon.cx() - monitor.x),
      (Edge::Right, monitor.right() - icon.cx()),
    ];
    distances.into_iter().min_by(|a, b| a.1.total_cmp(&b.1)).map_or(Edge::Bottom, |(edge, _)| edge)
  }
}

fn clamp_axis(value: f64, low: f64, high: f64) -> f64 {
  if high < low {
    low
  } else {
    value.clamp(low, high)
  }
}

/// Physical top-left of the popover: on the screen side the icon's bar is on, centred on the icon, kept inside the work area.
pub fn position(icon: Area, work: Area, scale: f64, size: (f64, f64), edge: Edge) -> (i32, i32) {
  let (w, h) = size;
  let gap = GAP * scale;
  let margin = EDGE_MARGIN * scale;
  let (x, y) = match edge {
    Edge::Top => (icon.cx() - w / 2.0, icon.bottom() + gap),
    Edge::Bottom => (icon.cx() - w / 2.0, icon.y - h - gap),
    Edge::Left => (icon.right() + gap, icon.cy() - h / 2.0),
    Edge::Right => (icon.x - w - gap, icon.cy() - h / 2.0),
  };
  let (x_margin, y_margin) = match edge {
    Edge::Top | Edge::Bottom => (margin, 0.0),
    Edge::Left | Edge::Right => (0.0, margin),
  };
  let x = clamp_axis(x, work.x + x_margin, work.right() - w - x_margin);
  let y = clamp_axis(y, work.y + y_margin, work.bottom() - h - y_margin);
  (x.round() as i32, y.round() as i32)
}

#[derive(Debug, PartialEq)]
pub enum Toggle {
  Show,
  Hide,
  Ignore,
}

/// Debounce state, with timestamps passed in so it stays pure.
#[derive(Default)]
pub struct Gate {
  shown_at: Option<u64>,
  blur_hidden_at: Option<u64>,
  visible_on_down: bool,
}

fn within(since: Option<u64>, now: u64, window_ms: u64) -> bool {
  since.is_some_and(|t| now.saturating_sub(t) < window_ms)
}

impl Gate {
  pub fn on_down(&mut self, visible: bool) {
    self.visible_on_down = visible;
  }

  pub fn on_up(&mut self, now: u64) -> Toggle {
    if self.visible_on_down {
      Toggle::Hide
    } else if within(self.blur_hidden_at, now, TOGGLE_DEBOUNCE_MS) {
      Toggle::Ignore
    } else {
      Toggle::Show
    }
  }

  pub fn shown(&mut self, now: u64) {
    self.shown_at = Some(now);
  }

  /// Whether a focus loss should hide the popover.
  pub fn on_blur(&mut self, now: u64) -> bool {
    if within(self.shown_at, now, BLUR_GRACE_MS) {
      return false;
    }
    self.blur_hidden_at = Some(now);
    true
  }
}

fn now_ms() -> u64 {
  static START: OnceLock<Instant> = OnceLock::new();
  START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PopoverTask {
  pub id: i64,
  pub workspace_id: i64,
  pub title: String,
  pub common_time: i64,
  pub start_time: i64,
}

impl From<&TrayTask> for PopoverTask {
  fn from(task: &TrayTask) -> Self {
    Self {
      id: task.id,
      workspace_id: task.workspace_id,
      title: task.title.clone(),
      common_time: task.common_time,
      start_time: task.start_time,
    }
  }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PopoverPluginItem {
  pub id: String,
  pub title: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PopoverPluginSection {
  pub plugin_name: String,
  pub label: String,
  pub items: Vec<PopoverPluginItem>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PopoverDnd {
  pub option: String,
  pub until: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PopoverModel {
  pub platform: &'static str,
  pub version: String,
  pub now: i64,
  pub running: Vec<PopoverTask>,
  pub recent: Vec<PopoverTask>,
  pub plugin_sections: Vec<PopoverPluginSection>,
  pub dnd: PopoverDnd,
  pub autostart: bool,
  pub updater_enabled: bool,
  pub decisions_available: bool,
  pub open_decisions: usize,
  pub extras: Value,
  pub missed: Vec<Value>,
}

pub struct Inputs {
  pub dnd: (String, Option<i64>),
  pub autostart: bool,
  pub updater_enabled: bool,
  pub decisions_available: bool,
  pub open_decisions: usize,
  pub extras: Value,
  pub version: String,
  pub platform: &'static str,
  pub now: i64,
}

fn platform() -> &'static str {
  if cfg!(target_os = "macos") {
    "macos"
  } else if cfg!(windows) {
    "windows"
  } else {
    "linux"
  }
}

/// Recent tasks that are not running, in the order the tray menu lists them.
fn idle_recent(state: &TrayState) -> impl Iterator<Item = &TrayTask> {
  state.recent.iter().filter(|r| !state.running.iter().any(|t| t.key() == r.key()))
}

pub fn build_model(state: &TrayState, inputs: Inputs) -> PopoverModel {
  let plugin_sections = state
    .plugin_sections
    .iter()
    .take(MAX_PLUGIN_SECTIONS)
    .map(|section| PopoverPluginSection {
      plugin_name: sanitize_plugin_label(&section.plugin_name).unwrap_or_else(|| "Plugin".to_owned()),
      label: plugin_section_label(section),
      items: section
        .items
        .iter()
        .take(MAX_PLUGIN_ITEMS)
        .map(|item| PopoverPluginItem {
          id: item.id.clone(),
          title: sanitize_plugin_label(&item.title).unwrap_or_else(|| "…".to_owned()),
        })
        .collect(),
    })
    .collect();
  PopoverModel {
    platform: inputs.platform,
    version: inputs.version,
    now: inputs.now,
    running: state.running.iter().map(PopoverTask::from).collect(),
    recent: idle_recent(state).map(PopoverTask::from).collect(),
    plugin_sections,
    dnd: PopoverDnd { option: inputs.dnd.0, until: inputs.dnd.1 },
    autostart: inputs.autostart,
    updater_enabled: inputs.updater_enabled,
    decisions_available: inputs.decisions_available,
    open_decisions: inputs.open_decisions,
    extras: inputs.extras,
    missed: Vec::new(),
  }
}

#[derive(Clone, Copy)]
struct Anchor {
  icon: Area,
  monitor: Area,
  work: Area,
  scale: f64,
}

#[derive(Default)]
pub struct PopoverStore {
  extras: Mutex<Option<Value>>,
  last: Mutex<Option<PopoverModel>>,
  gate: Mutex<Gate>,
  anchor: Mutex<Option<Anchor>>,
  height: Mutex<Option<f64>>,
  pending: AtomicBool,
}

fn gather<R: Runtime>(app: &AppHandle<R>) -> Inputs {
  let extras = app
    .try_state::<PopoverStore>()
    .and_then(|store| store.extras.lock().unwrap().clone())
    .unwrap_or_else(|| json!({}));
  let dnd = app.try_state::<DndStore>().map(|d| d.get()).unwrap_or_else(|| ("off".to_owned(), None));
  Inputs {
    dnd,
    autostart: app.autolaunch().is_enabled().unwrap_or(false),
    updater_enabled: crate::updater_enabled() && cfg!(not(target_os = "macos")),
    decisions_available: cfg!(unix),
    open_decisions: open_decisions(),
    extras,
    version: app.package_info().version.to_string(),
    platform: platform(),
    now: crate::tray::now_secs(),
  }
}

fn open_decisions() -> usize {
  #[cfg(unix)]
  return crate::decisions::open_count();
  #[cfg(not(unix))]
  0
}

fn tray_state<R: Runtime>(app: &AppHandle<R>) -> TrayState {
  app.try_state::<TrayStore>().map(|s| s.0.lock().unwrap().clone()).unwrap_or_default()
}

/// Pushes the model to the popover unless only the clock moved since the last push.
pub fn emit_model<R: Runtime>(app: &AppHandle<R>, state: &TrayState) {
  let Some(store) = app.try_state::<PopoverStore>() else { return };
  let model = build_model(state, gather(app));
  let key = PopoverModel { now: 0, ..model.clone() };
  {
    let mut last = store.last.lock().unwrap();
    if last.as_ref() == Some(&key) {
      return;
    }
    *last = Some(key);
  }
  let _ = app.emit_to(LABEL, "popover://model", &model);
}

#[cfg(any(target_os = "macos", windows))]
pub fn setup<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
  use tauri::utils::config::BackgroundThrottlingPolicy;
  use tauri::{WebviewUrl, WebviewWindowBuilder};

  let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("popover.html".into()))
    .title("TMGR")
    .inner_size(WIDTH, DEFAULT_HEIGHT)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .focused(false)
    .background_throttling(BackgroundThrottlingPolicy::Disabled)
    .on_navigation(crate::is_app_url);
  #[cfg(target_os = "macos")]
  let builder = builder.visible_on_all_workspaces(true);
  builder.build()?;
  Ok(())
}

fn rect_area(rect: &tauri::Rect) -> Area {
  let position = rect.position.to_physical::<f64>(1.0);
  let size = rect.size.to_physical::<f64>(1.0);
  Area::new(position.x, position.y, size.width, size.height)
}

fn anchor_for<R: Runtime>(app: &AppHandle<R>, icon: Area) -> Option<Anchor> {
  let monitor = app
    .monitor_from_point(icon.cx(), icon.cy())
    .ok()
    .flatten()
    .or_else(|| app.primary_monitor().ok().flatten())?;
  let (pos, size, work) = (monitor.position(), monitor.size(), monitor.work_area());
  Some(Anchor {
    icon,
    monitor: Area::new(f64::from(pos.x), f64::from(pos.y), f64::from(size.width), f64::from(size.height)),
    work: Area::new(
      f64::from(work.position.x),
      f64::from(work.position.y),
      f64::from(work.size.width),
      f64::from(work.size.height),
    ),
    scale: monitor.scale_factor(),
  })
}

fn place<R: Runtime>(app: &AppHandle<R>, window: &tauri::WebviewWindow<R>) {
  let store = app.state::<PopoverStore>();
  let Some(anchor) = *store.anchor.lock().unwrap() else { return };
  let height = store.height.lock().unwrap().unwrap_or(DEFAULT_HEIGHT);
  let size = (WIDTH * anchor.scale, height * anchor.scale);
  let edge = if cfg!(target_os = "macos") {
    Edge::Top
  } else {
    taskbar_edge(anchor.monitor, anchor.work, anchor.icon)
  };
  let (x, y) = position(anchor.icon, anchor.work, anchor.scale, size, edge);
  let _ = window.set_position(PhysicalPosition::new(x, y));
}

fn reveal<R: Runtime>(app: &AppHandle<R>, window: &tauri::WebviewWindow<R>) {
  place(app, window);
  let _ = window.show();
  place(app, window);
  let _ = window.set_focus();
  app.state::<PopoverStore>().gate.lock().unwrap().shown(now_ms());
}

fn hide<R: Runtime>(app: &AppHandle<R>) {
  if let Some(store) = app.try_state::<PopoverStore>() {
    store.pending.store(false, Ordering::SeqCst);
  }
  if let Some(window) = app.get_webview_window(LABEL) {
    let _ = window.hide();
  }
}

fn request_show<R: Runtime>(app: &AppHandle<R>, rect: &tauri::Rect) {
  let store = app.state::<PopoverStore>();
  *store.anchor.lock().unwrap() = anchor_for(app, rect_area(rect));
  store.pending.store(true, Ordering::SeqCst);
  let _ = app.emit_to(LABEL, "popover://show", ());
  let handle = app.clone();
  std::thread::spawn(move || {
    std::thread::sleep(SHOW_FALLBACK);
    let store = handle.state::<PopoverStore>();
    if store.pending.swap(false, Ordering::SeqCst) {
      if let Some(window) = handle.get_webview_window(LABEL) {
        reveal(&handle, &window);
      }
    }
  });
}

pub fn on_tray_click<R: Runtime>(app: &AppHandle<R>, up: bool, rect: tauri::Rect) {
  let Some(window) = app.get_webview_window(LABEL) else { return };
  let Some(store) = app.try_state::<PopoverStore>() else { return };
  if !up {
    let visible = window.is_visible().unwrap_or(false);
    store.gate.lock().unwrap().on_down(visible);
    return;
  }
  let toggle = store.gate.lock().unwrap().on_up(now_ms());
  match toggle {
    Toggle::Ignore => {}
    Toggle::Hide => hide(app),
    Toggle::Show => request_show(app, &rect),
  }
}

pub fn on_window_event<R: Runtime>(app: &AppHandle<R>, event: &WindowEvent) {
  if let WindowEvent::Focused(false) = event {
    let Some(store) = app.try_state::<PopoverStore>() else { return };
    let hide_now = store.gate.lock().unwrap().on_blur(now_ms());
    if hide_now {
      hide(app);
    }
  }
}

/// What a validated popover action turns into.
#[derive(Debug, PartialEq)]
pub(crate) enum Action {
  Close,
  Menu(String),
  OpenUsage,
  OpenRunner,
  RoutineDone(i64),
}

const MENU_IDS: [&str; 6] = ["open", "manager", "shortcuts", "autostart", "quit", "tray_check_for_updates"];

pub(crate) fn classify(id: &str, state: &TrayState, decisions_available: bool) -> Result<Action, String> {
  let known = |key: &str| state.running.iter().chain(&state.recent).any(|t| t.key() == key);
  let menu = |id: &str| Ok(Action::Menu(id.to_owned()));
  if let Some(key) = id.strip_prefix("open:") {
    return if known(key) { menu(id) } else { Err(format!("unknown task {key}")) };
  }
  if let Some(key) = id.strip_prefix("stop:") {
    return if state.running.iter().any(|t| t.key() == key) { menu(id) } else { Err(format!("task {key} is not running")) };
  }
  if let Some(key) = id.strip_prefix("switch:") {
    return if known(key) { menu(id) } else { Err(format!("unknown task {key}")) };
  }
  if let Some(item) = id.strip_prefix("plugin-item:") {
    let present = state
      .plugin_sections
      .iter()
      .take(MAX_PLUGIN_SECTIONS)
      .any(|s| s.items.iter().take(MAX_PLUGIN_ITEMS).any(|i| i.id == item));
    return if present { menu(id) } else { Err("unknown plugin item".to_owned()) };
  }
  if let Some(option) = id.strip_prefix("dnd:") {
    return if normalize_dnd_option(option) == option { menu(id) } else { Err(format!("unknown dnd option {option}")) };
  }
  if let Some(routine) = id.strip_prefix("routine-done:") {
    return routine.parse::<i64>().map(Action::RoutineDone).map_err(|_| "bad routine id".to_owned());
  }
  match id {
    "close" => Ok(Action::Close),
    "open-usage" => Ok(Action::OpenUsage),
    "open-runner" => Ok(Action::OpenRunner),
    "resume-last" => idle_recent(state)
      .next()
      .map(|t| Action::Menu(format!("switch:{}", t.key())))
      .ok_or_else(|| "no recent task".to_owned()),
    "decisions" if decisions_available => menu(id),
    other if MENU_IDS.contains(&other) => menu(id),
    other => Err(format!("unknown action {other}")),
  }
}

#[tauri::command]
pub fn popover_state<R: Runtime>(app: AppHandle<R>) -> PopoverModel {
  build_model(&tray_state(&app), gather(&app))
}

#[tauri::command]
pub fn popover_action<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), String> {
  let action = classify(&id, &tray_state(&app), cfg!(unix))?;
  hide(&app);
  match action {
    Action::Close => {}
    Action::Menu(menu_id) => crate::tray::on_menu_event(&app, &menu_id),
    Action::OpenUsage => {
      crate::tray::show_main(&app);
      let _ = app.emit_to("main", "popover://open-usage", ());
    }
    Action::OpenRunner => {
      crate::tray::show_main(&app);
      let _ = app.emit_to("main", "popover://open-runner", ());
    }
    Action::RoutineDone(routine) => {
      let _ = app.emit_to("main", "popover://routine-done", json!({ "id": routine }));
    }
  }
  Ok(())
}

pub(crate) fn clean_quick_add(title: &str) -> Result<String, String> {
  let title = title.trim();
  match title.chars().count() {
    1..=QUICK_ADD_MAX => Ok(title.to_owned()),
    _ => Err("title must be 1 to 200 characters".to_owned()),
  }
}

#[tauri::command]
pub fn popover_quick_add<R: Runtime>(app: AppHandle<R>, title: String) -> Result<(), String> {
  let title = clean_quick_add(&title)?;
  let _ = app.emit_to("main", "popover://quick-add", json!({ "title": title }));
  Ok(())
}

#[tauri::command]
pub fn popover_resize<R: Runtime>(app: AppHandle<R>, height: f64) -> Result<(), String> {
  if !height.is_finite() {
    return Err("bad height".to_owned());
  }
  let Some(window) = app.get_webview_window(LABEL) else { return Err("popover window missing".to_owned()) };
  let store = app.state::<PopoverStore>();
  let height = height.clamp(MIN_HEIGHT, MAX_HEIGHT);
  *store.height.lock().unwrap() = Some(height);
  window.set_size(LogicalSize::new(WIDTH, height)).map_err(|e| e.to_string())?;
  if store.pending.swap(false, Ordering::SeqCst) {
    reveal(&app, &window);
  } else if window.is_visible().unwrap_or(false) {
    place(&app, &window);
  }
  Ok(())
}

#[tauri::command]
pub fn popover_extras_update<R: Runtime>(
  app: AppHandle<R>,
  store: State<'_, PopoverStore>,
  extras: Value,
) -> Result<(), String> {
  if !extras.is_object() {
    return Err("extras must be an object".to_owned());
  }
  *store.extras.lock().unwrap() = Some(extras);
  emit_model(&app, &tray_state(&app));
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::tray::{PluginTrayItem, PluginTraySection};

  fn area(x: f64, y: f64, w: f64, h: f64) -> Area {
    Area::new(x, y, w, h)
  }

  fn task(workspace_id: i64, id: i64) -> TrayTask {
    TrayTask { id, title: format!("T{id}"), common_time: 0, start_time: 0, workspace_id }
  }

  const SIZE: (f64, f64) = (640.0, 800.0);

  #[test]
  fn mac_popover_opens_below_the_menu_bar_icon_centred() {
    let monitor = area(0.0, 0.0, 3000.0, 1900.0);
    let work = area(0.0, 50.0, 3000.0, 1850.0);
    let icon = area(2000.0, 0.0, 44.0, 50.0);
    let (x, y) = position(icon, work, 2.0, SIZE, Edge::Top);
    assert_eq!(x, 2022 - 320);
    assert_eq!(y, 50 + 12);
    assert_eq!(taskbar_edge(monitor, work, icon), Edge::Top);
  }

  #[test]
  fn windows_popover_sits_next_to_the_taskbar_edge() {
    let icon = area(1500.0, 1040.0, 24.0, 40.0);
    let bottom = position(icon, area(0.0, 0.0, 1920.0, 1040.0), 1.0, (320.0, 400.0), Edge::Bottom);
    assert_eq!(bottom, (1512 - 160, 1040 - 400 - 6));
    let top = position(area(1500.0, 0.0, 24.0, 40.0), area(0.0, 40.0, 1920.0, 1040.0), 1.0, (320.0, 400.0), Edge::Top);
    assert_eq!(top, (1512 - 160, 46));
    let left = position(area(0.0, 600.0, 40.0, 40.0), area(40.0, 0.0, 1880.0, 1080.0), 1.0, (320.0, 400.0), Edge::Left);
    assert_eq!(left, (46, 620 - 200));
    let right = position(area(1880.0, 600.0, 40.0, 40.0), area(0.0, 0.0, 1880.0, 1080.0), 1.0, (320.0, 400.0), Edge::Right);
    assert_eq!(right, (1880 - 320 - 6, 620 - 200));
  }

  #[test]
  fn popover_is_clamped_inside_the_work_area() {
    let work = area(0.0, 0.0, 1920.0, 1040.0);
    let far_right = position(area(1900.0, 1040.0, 20.0, 40.0), work, 1.0, (320.0, 400.0), Edge::Bottom);
    assert_eq!(far_right.0, 1920 - 320 - 8);
    let far_left = position(area(0.0, 1040.0, 20.0, 40.0), work, 1.0, (320.0, 400.0), Edge::Bottom);
    assert_eq!(far_left.0, 8);
    let tall = position(area(1000.0, 1040.0, 20.0, 40.0), work, 1.0, (320.0, 2000.0), Edge::Bottom);
    assert_eq!(tall.1, 0);
    let low_vertical = position(area(1880.0, 1070.0, 40.0, 40.0), work, 1.0, (320.0, 400.0), Edge::Right);
    assert_eq!(low_vertical.1, 1040 - 400 - 8);
  }

  #[test]
  fn scale_widens_the_gap_and_margin() {
    let work = area(0.0, 0.0, 4000.0, 2000.0);
    let (_, y) = position(area(100.0, 0.0, 40.0, 50.0), work, 2.0, SIZE, Edge::Top);
    assert_eq!(y, 62);
    let (x, _) = position(area(0.0, 0.0, 40.0, 50.0), work, 2.0, SIZE, Edge::Top);
    assert_eq!(x, 16);
  }

  #[test]
  fn taskbar_edge_comes_from_the_work_area_or_the_icon() {
    let monitor = area(0.0, 0.0, 1920.0, 1080.0);
    assert_eq!(taskbar_edge(monitor, area(0.0, 0.0, 1920.0, 1040.0), area(0.0, 0.0, 1.0, 1.0)), Edge::Bottom);
    assert_eq!(taskbar_edge(monitor, area(48.0, 0.0, 1872.0, 1080.0), area(0.0, 0.0, 1.0, 1.0)), Edge::Left);
    assert_eq!(taskbar_edge(monitor, area(0.0, 0.0, 1872.0, 1080.0), area(0.0, 0.0, 1.0, 1.0)), Edge::Right);
    assert_eq!(taskbar_edge(monitor, monitor, area(900.0, 1060.0, 20.0, 20.0)), Edge::Bottom);
    assert_eq!(taskbar_edge(monitor, monitor, area(900.0, 0.0, 20.0, 20.0)), Edge::Top);
    assert_eq!(taskbar_edge(monitor, monitor, area(1900.0, 500.0, 20.0, 20.0)), Edge::Right);
  }

  #[test]
  fn blur_is_ignored_right_after_show() {
    let mut gate = Gate::default();
    gate.shown(1_000);
    assert!(!gate.on_blur(1_100));
    assert!(gate.on_blur(1_150));
  }

  #[test]
  fn tray_click_right_after_a_blur_hide_does_not_reopen() {
    let mut gate = Gate::default();
    gate.shown(0);
    assert!(gate.on_blur(5_000));
    gate.on_down(false);
    assert_eq!(gate.on_up(5_100), Toggle::Ignore);
    gate.on_down(false);
    assert_eq!(gate.on_up(5_300), Toggle::Show);
  }

  #[test]
  fn tray_click_toggles_by_visibility_on_press() {
    let mut gate = Gate::default();
    gate.on_down(false);
    assert_eq!(gate.on_up(10), Toggle::Show);
    gate.on_down(true);
    assert_eq!(gate.on_up(20), Toggle::Hide);
  }

  #[test]
  fn press_while_open_then_blur_still_resolves_to_hide() {
    let mut gate = Gate::default();
    gate.shown(0);
    gate.on_down(true);
    assert!(gate.on_blur(5_000));
    assert_eq!(gate.on_up(5_020), Toggle::Hide);
  }

  fn state() -> TrayState {
    TrayState {
      running: vec![task(1, 10), task(1, 11)],
      recent: vec![task(1, 10), task(2, 20), task(2, 21)],
      plugin_sections: vec![PluginTraySection {
        plugin_name: "Board".into(),
        title: "Quick".into(),
        items: vec![PluginTrayItem { id: "p1".into(), title: "Do it".into() }],
      }],
      tray_title: None,
    }
  }

  fn menu(id: &str) -> Result<Action, String> {
    Ok(Action::Menu(id.to_owned()))
  }

  #[test]
  fn action_ids_are_accepted_or_rejected_against_the_tray_state() {
    let s = state();
    let table: Vec<(&str, Result<Action, String>)> = vec![
      ("open:1:10", menu("open:1:10")),
      ("open:2:20", menu("open:2:20")),
      ("stop:1:11", menu("stop:1:11")),
      ("switch:2:21", menu("switch:2:21")),
      ("plugin-item:p1", menu("plugin-item:p1")),
      ("dnd:off", menu("dnd:off")),
      ("dnd:1h", menu("dnd:1h")),
      ("dnd:3h", menu("dnd:3h")),
      ("dnd:tomorrow", menu("dnd:tomorrow")),
      ("open", menu("open")),
      ("manager", menu("manager")),
      ("shortcuts", menu("shortcuts")),
      ("autostart", menu("autostart")),
      ("quit", menu("quit")),
      ("tray_check_for_updates", menu("tray_check_for_updates")),
      ("decisions", menu("decisions")),
      ("resume-last", menu("switch:2:20")),
      ("close", Ok(Action::Close)),
      ("open-usage", Ok(Action::OpenUsage)),
      ("open-runner", Ok(Action::OpenRunner)),
      ("routine-done:42", Ok(Action::RoutineDone(42))),
    ];
    for (id, expected) in table {
      assert_eq!(classify(id, &s, true), expected, "{id}");
    }
    for id in [
      "open:9:9", "stop:2:20", "switch:0:0", "plugin-item:nope", "dnd:forever", "dnd:", "routine-done:x", "bogus", "",
      "open-usage ", "Quit",
    ] {
      assert!(classify(id, &s, true).is_err(), "{id} should be rejected");
    }
  }

  #[test]
  fn decisions_is_rejected_where_unavailable_and_resume_needs_a_recent_task() {
    assert!(classify("decisions", &state(), false).is_err());
    assert!(classify("resume-last", &TrayState::default(), true).is_err());
    let only_running = TrayState { running: vec![task(1, 1)], recent: vec![task(1, 1)], ..Default::default() };
    assert!(classify("resume-last", &only_running, true).is_err());
  }

  #[test]
  fn plugin_items_beyond_the_caps_are_rejected() {
    let items = (0..=MAX_PLUGIN_ITEMS).map(|i| PluginTrayItem { id: format!("i{i}"), title: "t".into() }).collect();
    let mut sections: Vec<PluginTraySection> = (0..=MAX_PLUGIN_SECTIONS)
      .map(|i| PluginTraySection { plugin_name: format!("p{i}"), title: "s".into(), items: vec![PluginTrayItem { id: format!("s{i}"), title: "t".into() }] })
      .collect();
    sections[0].items = items;
    let s = TrayState { plugin_sections: sections, ..Default::default() };
    assert!(classify("plugin-item:i0", &s, true).is_ok());
    assert!(classify(&format!("plugin-item:i{MAX_PLUGIN_ITEMS}"), &s, true).is_err());
    assert!(classify(&format!("plugin-item:s{}", MAX_PLUGIN_SECTIONS - 1), &s, true).is_ok());
    assert!(classify(&format!("plugin-item:s{MAX_PLUGIN_SECTIONS}"), &s, true).is_err());
  }

  #[test]
  fn menu_actions_dispatch_the_native_menu_id_unchanged() {
    for id in ["open", "manager", "shortcuts", "autostart", "quit", "tray_check_for_updates", "dnd:3h", "stop:1:10"] {
      assert_eq!(classify(id, &state(), true), Ok(Action::Menu(id.to_owned())));
    }
  }

  fn inputs() -> Inputs {
    Inputs {
      dnd: ("1h".into(), Some(99)),
      autostart: true,
      updater_enabled: false,
      decisions_available: true,
      open_decisions: 3,
      extras: json!({}),
      version: "1.2.3".into(),
      platform: "macos",
      now: 500,
    }
  }

  #[test]
  fn model_includes_every_running_task_and_drops_running_from_recent() {
    let mut s = state();
    s.running = (1..=7).map(|i| task(1, i)).collect();
    s.recent = vec![task(1, 3), task(1, 50)];
    let model = build_model(&s, inputs());
    assert_eq!(model.running.len(), 7);
    assert_eq!(model.recent.iter().map(|t| t.id).collect::<Vec<_>>(), vec![50]);
    assert!(model.decisions_available);
    assert_eq!(model.open_decisions, 3);
    assert_eq!(model.dnd, PopoverDnd { option: "1h".into(), until: Some(99) });
  }

  #[test]
  fn model_sanitizes_and_caps_plugin_text() {
    let long = "x".repeat(200);
    let sections = (0..30)
      .map(|i| PluginTraySection {
        plugin_name: format!("Plug\u{202e}{i}"),
        title: format!("Ti\ntle{long}"),
        items: (0..25).map(|j| PluginTrayItem { id: format!("{i}-{j}"), title: format!("a\u{200b}\nb{long}") }).collect(),
      })
      .collect();
    let model = build_model(&TrayState { plugin_sections: sections, ..Default::default() }, inputs());
    assert_eq!(model.plugin_sections.len(), MAX_PLUGIN_SECTIONS);
    let first = &model.plugin_sections[0];
    assert_eq!(first.plugin_name, "Plug0");
    assert!(first.label.starts_with("Plug0: Ti tle"));
    assert!(first.label.chars().count() <= 42);
    assert_eq!(first.items.len(), MAX_PLUGIN_ITEMS);
    assert!(first.items[0].title.starts_with("a b"));
    assert!(first.items[0].title.chars().count() <= 42);
  }

  #[test]
  fn model_serializes_in_the_ts_contract_shape() {
    let value = serde_json::to_value(build_model(&state(), inputs())).unwrap();
    let keys: Vec<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();
    for key in [
      "platform", "version", "now", "running", "recent", "pluginSections", "dnd", "autostart", "updaterEnabled",
      "decisionsAvailable", "openDecisions", "extras", "missed",
    ] {
      assert!(keys.contains(&key), "{key} missing");
    }
    assert_eq!(keys.len(), 13);
    assert_eq!(value["running"][0]["workspaceId"], 1);
    assert_eq!(value["running"][0]["commonTime"], 0);
    assert_eq!(value["pluginSections"][0]["pluginName"], "Board");
    assert_eq!(value["dnd"]["until"], 99);
    assert_eq!(value["extras"], json!({}));
  }

  #[test]
  fn quick_add_titles_are_trimmed_and_bounded() {
    assert_eq!(clean_quick_add("  hi  "), Ok("hi".to_owned()));
    assert!(clean_quick_add("   ").is_err());
    assert!(clean_quick_add(&"a".repeat(200)).is_ok());
    assert!(clean_quick_add(&"a".repeat(201)).is_err());
  }
}
