use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, Runtime, State};

use crate::tray::{DndStore, TrayStore};

pub const LABEL: &str = "toast";

const WIDTH: f64 = 356.0;
const DEFAULT_HEIGHT: f64 = 140.0;
const MIN_HEIGHT: f64 = 40.0;
const MAX_HEIGHT: f64 = 900.0;
const MARGIN: f64 = 8.0;
const MAX_VISIBLE: usize = 2;
const MAX_EXPANDED: usize = 6;
const MAX_QUEUE: usize = 50;
const MISSED_CAP: usize = 20;
const DEFAULT_TTL_MS: u64 = 10_000;
const MAX_PAUSE_MS: i64 = 60_000;
const MAX_ACTIONS: usize = 4;
const MAX_ID: usize = 128;
const MAX_TITLE: usize = 120;
const MAX_BODY: usize = 300;
const MAX_META: usize = 120;
const MAX_LABEL: usize = 40;
const MAX_ACTION_ID: usize = 64;
const SWEEP: Duration = Duration::from_millis(500);
pub const IDLE_ID: &str = "idle:timer";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
  Decision,
  Agent,
  Idle,
  Limit,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToastAction {
  pub id: String,
  pub label: String,
  #[serde(default)]
  pub primary: bool,
}

fn some<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<u64>>, D::Error> {
  Option::<u64>::deserialize(d).map(Some)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Toast {
  pub id: String,
  pub kind: Kind,
  pub title: String,
  pub body: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub meta: Option<String>,
  #[serde(default)]
  pub actions: Vec<ToastAction>,
  #[serde(default, deserialize_with = "some", skip_serializing_if = "Option::is_none")]
  pub ttl_ms: Option<Option<u64>>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub data: Option<Value>,
}

impl Toast {
  fn decision_id(&self) -> Option<String> {
    self.data.as_ref()?.get("decisionId")?.as_str().map(str::to_owned)
  }

  fn seen_ms(&self) -> Option<i64> {
    self.data.as_ref()?.get("seenMs")?.as_i64()
  }
}

fn cut(text: &str, max: usize) -> String {
  text.trim().chars().take(max).collect()
}

pub fn clean(mut toast: Toast) -> Result<Toast, String> {
  toast.id = cut(&toast.id, MAX_ID);
  toast.title = cut(&toast.title, MAX_TITLE);
  if toast.id.is_empty() || toast.title.is_empty() {
    return Err("toast needs an id and a title".to_owned());
  }
  toast.body = cut(&toast.body, MAX_BODY);
  toast.meta = toast.meta.map(|m| cut(&m, MAX_META)).filter(|m| !m.is_empty());
  toast.actions.truncate(MAX_ACTIONS);
  for action in &mut toast.actions {
    action.id = cut(&action.id, MAX_ACTION_ID);
    action.label = cut(&action.label, MAX_LABEL);
  }
  toast.actions.retain(|a| !a.id.is_empty() && !a.label.is_empty());
  Ok(toast)
}

pub fn effective_ttl(ttl_ms: Option<Option<u64>>) -> Option<u64> {
  ttl_ms.unwrap_or(Some(DEFAULT_TTL_MS))
}

pub fn stack<T: Clone>(queue: &[T], expanded: bool) -> (Vec<T>, usize) {
  let shown = if expanded { MAX_EXPANDED } else { MAX_VISIBLE };
  (queue.iter().take(shown).cloned().collect(), queue.len().saturating_sub(shown))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gate {
  Show,
  Defer,
  Fallback,
}

pub fn dnd_active(dnd: (&str, Option<i64>), now_ms: i64) -> bool {
  dnd.0 != "off" && dnd.1.is_some_and(|until| now_ms < until)
}

pub fn gate(dnd: (&str, Option<i64>), now_ms: i64, fullscreen: bool, window_ok: bool) -> Gate {
  if !window_ok {
    Gate::Fallback
  } else if dnd_active(dnd, now_ms) || fullscreen {
    Gate::Defer
  } else {
    Gate::Show
  }
}

#[derive(Clone, Debug, PartialEq)]
struct Entry {
  toast: Toast,
  at_ms: i64,
  total_ms: Option<u64>,
  expires_at: Option<i64>,
  paused_left: Option<i64>,
  paused_at: Option<i64>,
}

impl Entry {
  fn remaining(&self, now_ms: i64) -> Option<i64> {
    let left = self.paused_left.or_else(|| self.expires_at.map(|at| at - now_ms))?;
    Some(left.max(0))
  }

  fn set_hover(&mut self, hovering: bool, now_ms: i64) -> bool {
    let Some(left) = self.remaining(now_ms) else { return false };
    match (hovering, self.paused_left) {
      (true, None) => {
        self.paused_left = Some(left);
        self.paused_at = Some(now_ms);
      }
      (false, Some(_)) => {
        self.expires_at = Some(now_ms + left);
        self.paused_left = None;
        self.paused_at = None;
      }
      _ => return false,
    }
    true
  }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Missed {
  pub id: String,
  pub kind: Kind,
  pub title: String,
  pub body: String,
  pub at: i64,
  #[serde(skip)]
  decision_id: Option<String>,
}

#[derive(Default)]
struct Inner {
  queue: Vec<Entry>,
  missed: Vec<Missed>,
  expanded: bool,
}

impl Inner {
  fn enqueue(&mut self, toast: Toast, now_ms: i64) {
    self.queue.retain(|e| e.toast.id != toast.id);
    let total_ms = effective_ttl(toast.ttl_ms);
    let expires_at = total_ms.map(|ttl| now_ms + ttl as i64);
    self.queue.insert(0, Entry { toast, at_ms: now_ms, total_ms, expires_at, paused_left: None, paused_at: None });
    self.queue.truncate(MAX_QUEUE);
  }

  fn defer(&mut self, toast: Toast, now_ms: i64) {
    self.missed.retain(|m| m.id != toast.id);
    let decision_id = toast.decision_id();
    self.missed.insert(
      0,
      Missed { id: toast.id, kind: toast.kind, title: toast.title, body: toast.body, at: now_ms / 1000, decision_id },
    );
    self.missed.truncate(MISSED_CAP);
  }

  fn sync_decisions(&mut self, open: &HashMap<String, (i64, Vec<DecisionOption>)>) -> bool {
    let before = (self.queue.clone(), self.missed.len());
    self.queue.retain(|e| e.toast.decision_id().is_none_or(|id| open.contains_key(&id)));
    self.missed.retain(|m| m.decision_id.as_ref().is_none_or(|id| open.contains_key(id)));
    for entry in &mut self.queue {
      let Some((seen, options)) = entry.toast.decision_id().and_then(|id| open.get(&id)) else { continue };
      if entry.toast.seen_ms() == Some(*seen) {
        continue;
      }
      let decision = entry.toast.decision_id().unwrap_or_default();
      entry.toast = decision_toast(&decision, &entry.toast.body, entry.toast.meta.clone(), options, Some(*seen));
    }
    before != (self.queue.clone(), self.missed.len())
  }

  fn release_pauses(&mut self, now_ms: i64, visible: usize) -> bool {
    let mut any = false;
    for (index, entry) in self.queue.iter_mut().enumerate() {
      let stale = entry.paused_at.is_some_and(|at| now_ms - at >= MAX_PAUSE_MS);
      if index >= visible || stale {
        any |= entry.set_hover(false, now_ms);
      }
    }
    any
  }

  fn expire(&mut self, now_ms: i64) -> bool {
    let released = self.release_pauses(now_ms, if self.expanded { MAX_EXPANDED } else { MAX_VISIBLE });
    let (gone, kept): (Vec<Entry>, Vec<Entry>) = std::mem::take(&mut self.queue)
      .into_iter()
      .partition(|e| e.paused_left.is_none() && e.expires_at.is_some_and(|at| now_ms >= at));
    self.queue = kept;
    let any = !gone.is_empty();
    for entry in gone.into_iter().rev() {
      self.defer(entry.toast, now_ms);
    }
    any || released
  }

  fn hover(&mut self, id: &str, hovering: bool, now_ms: i64) -> bool {
    self.queue.iter_mut().find(|e| e.toast.id == id).is_some_and(|e| e.set_hover(hovering, now_ms))
  }

  fn remove(&mut self, id: &str) -> bool {
    let (q, m) = (self.queue.len(), self.missed.len());
    self.queue.retain(|e| e.toast.id != id);
    self.missed.retain(|x| x.id != id);
    (q, m) != (self.queue.len(), self.missed.len())
  }

  fn has_decisions(&self) -> bool {
    self.queue.iter().any(|e| e.toast.decision_id().is_some()) || self.missed.iter().any(|m| m.decision_id.is_some())
  }

  fn remove_decision(&mut self, decision_id: &str) -> bool {
    let (q, m) = (self.queue.len(), self.missed.len());
    self.queue.retain(|e| e.toast.decision_id().as_deref() != Some(decision_id));
    self.missed.retain(|x| x.decision_id.as_deref() != Some(decision_id));
    (q, m) != (self.queue.len(), self.missed.len())
  }
}

#[derive(Default)]
pub struct ToastStore {
  state: Mutex<Inner>,
  fullscreen: crate::toast_fullscreen::Cache,
  height: Mutex<Option<f64>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct View {
  #[serde(flatten)]
  toast: Toast,
  at: i64,
  remaining_ms: Option<i64>,
  total_ms: Option<u64>,
  paused: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListPayload {
  visible: Vec<View>,
  more: usize,
  expanded: bool,
}

fn payload_of(state: &Inner, now_ms: i64) -> ListPayload {
  let (visible, more) = stack(&state.queue, state.expanded);
  ListPayload {
    visible: visible
      .into_iter()
      .map(|e| View {
        remaining_ms: e.remaining(now_ms),
        total_ms: e.total_ms,
        paused: e.paused_left.is_some(),
        at: e.at_ms,
        toast: e.toast,
      })
      .collect(),
    more,
    expanded: state.expanded,
  }
}

fn now_ms() -> i64 {
  SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

pub fn missed_records<R: Runtime>(app: &AppHandle<R>) -> Vec<Value> {
  let Some(store) = app.try_state::<ToastStore>() else { return Vec::new() };
  let state = store.state.lock().unwrap();
  state.missed.iter().filter_map(|m| serde_json::to_value(m).ok()).collect()
}

fn refresh_popover<R: Runtime>(app: &AppHandle<R>) {
  #[cfg(any(target_os = "macos", windows))]
  crate::popover::refresh(app);
  #[cfg(not(any(target_os = "macos", windows)))]
  let _ = app;
}

pub fn place(work: (f64, f64, f64, f64), scale: f64, width: f64) -> (i32, i32) {
  let (x, y, w, _) = work;
  let margin = MARGIN * scale;
  ((x + w - width * scale - margin).round() as i32, (y + margin).round() as i32)
}

fn place_window<R: Runtime>(app: &AppHandle<R>, window: &tauri::WebviewWindow<R>) {
  let Some(monitor) = app.primary_monitor().ok().flatten() else { return };
  let work = monitor.work_area();
  let (pos, size) = (work.position, work.size);
  let at = place(
    (f64::from(pos.x), f64::from(pos.y), f64::from(size.width), f64::from(size.height)),
    monitor.scale_factor(),
    WIDTH,
  );
  let _ = window.set_position(PhysicalPosition::new(at.0, at.1));
}

fn render<R: Runtime>(app: &AppHandle<R>) {
  let Some(store) = app.try_state::<ToastStore>() else { return };
  let payload = {
    let mut state = store.state.lock().unwrap();
    if state.queue.is_empty() {
      state.expanded = false;
    }
    let now = now_ms();
    let shown = if state.expanded { MAX_EXPANDED } else { MAX_VISIBLE };
    state.release_pauses(now, shown);
    payload_of(&state, now)
  };
  let empty = payload.visible.is_empty();
  let _ = app.emit_to(LABEL, "toast://list", &payload);
  let Some(window) = app.get_webview_window(LABEL) else { return };
  if empty {
    let _ = window.hide();
  } else if !window.is_visible().unwrap_or(false) {
    place_window(app, &window);
    let _ = window.show();
  }
}

fn fallback<R: Runtime>(app: &AppHandle<R>, toast: &Toast, text: Option<(String, String)>) {
  let (title, body) = text.unwrap_or_else(|| (toast.title.clone(), toast.body.clone()));
  match toast.decision_id() {
    Some(id) if toast.kind == Kind::Decision => crate::plugin_notify::notify_decision(app.clone(), title, body, id),
    _ => crate::plugin_notify::plugin_notify(app.clone(), title, body, None),
  }
}

pub fn push<R: Runtime>(app: &AppHandle<R>, toast: Toast) {
  push_inner(app, toast, None);
}

pub fn push_with_fallback<R: Runtime>(app: &AppHandle<R>, toast: Toast, text: (String, String)) {
  push_inner(app, toast, Some(text));
}

fn push_inner<R: Runtime>(app: &AppHandle<R>, toast: Toast, text: Option<(String, String)>) {
  let Some(store) = app.try_state::<ToastStore>() else {
    fallback(app, &toast, text);
    return;
  };
  let now = now_ms();
  let dnd = app.try_state::<DndStore>().map(|d| d.get()).unwrap_or_else(|| ("off".to_owned(), None));
  let dnd = (dnd.0.as_str(), dnd.1);
  let window_ok = app.get_webview_window(LABEL).is_some();
  let fullscreen =
    window_ok && !dnd_active(dnd, now) && store.fullscreen.get(now as u64, crate::toast_fullscreen::probe);
  match gate(dnd, now, fullscreen, window_ok) {
    Gate::Fallback => fallback(app, &toast, text),
    Gate::Defer => {
      store.state.lock().unwrap().defer(toast, now);
      refresh_popover(app);
    }
    Gate::Show => {
      {
        let mut state = store.state.lock().unwrap();
        state.missed.retain(|m| m.id != toast.id);
        state.enqueue(toast, now);
      }
      render(app);
      refresh_popover(app);
    }
  }
}

pub fn dismiss<R: Runtime>(app: &AppHandle<R>, id: &str) {
  let Some(store) = app.try_state::<ToastStore>() else { return };
  if store.state.lock().unwrap().remove(id) {
    render(app);
    refresh_popover(app);
  }
}

pub fn has_decision_toasts<R: Runtime>(app: &AppHandle<R>) -> bool {
  let Some(store) = app.try_state::<ToastStore>() else { return false };
  let state = store.state.lock().unwrap();
  state.has_decisions()
}

pub fn sync_decisions<R: Runtime>(app: &AppHandle<R>, open: &HashMap<String, (i64, Vec<DecisionOption>)>) {
  let Some(store) = app.try_state::<ToastStore>() else { return };
  if store.state.lock().unwrap().sync_decisions(open) {
    render(app);
    refresh_popover(app);
  }
}

pub fn decision_answered<R: Runtime>(app: &AppHandle<R>, decision_id: &str) {
  let Some(store) = app.try_state::<ToastStore>() else { return };
  if store.state.lock().unwrap().remove_decision(decision_id) {
    render(app);
    refresh_popover(app);
  }
}

pub fn setup<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
  use tauri::utils::config::BackgroundThrottlingPolicy;
  use tauri::{WebviewUrl, WebviewWindowBuilder};

  let builder = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("toast.html".into()))
    .title("TMGR")
    .inner_size(WIDTH, DEFAULT_HEIGHT)
    .resizable(false)
    .decorations(false)
    .transparent(true)
    .shadow(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .visible(false)
    .focused(false)
    .focusable(false)
    .accept_first_mouse(true)
    .background_throttling(BackgroundThrottlingPolicy::Disabled)
    .on_navigation(crate::is_app_url);
  #[cfg(target_os = "macos")]
  let builder = builder.visible_on_all_workspaces(true);
  builder.build()?;
  let handle = app.clone();
  std::thread::spawn(move || loop {
    std::thread::sleep(SWEEP);
    let Some(store) = handle.try_state::<ToastStore>() else { continue };
    if store.state.lock().unwrap().expire(now_ms()) {
      render(&handle);
      refresh_popover(&handle);
    }
  });
  Ok(())
}

pub type DecisionOption = (String, String, bool);

fn capped(toast: Toast) -> Toast {
  clean(toast.clone()).unwrap_or(toast)
}

pub fn decision_toast(
  id: &str,
  title: &str,
  meta: Option<String>,
  options: &[DecisionOption],
  seen_ms: Option<i64>,
) -> Toast {
  let mut picked: Vec<&DecisionOption> = options.iter().take(3).collect();
  if let Some(rec) = options.iter().find(|o| o.2) {
    if !picked.iter().any(|o| o.0 == rec.0) {
      picked.pop();
      picked.push(rec);
    }
  }
  let mut actions: Vec<ToastAction> = picked
    .into_iter()
    .map(|(opt, label, rec)| ToastAction {
      id: format!("answer:{opt}"),
      label: if *rec { format!("{label} ★") } else { label.clone() },
      primary: *rec,
    })
    .collect();
  if actions.is_empty() {
    actions.push(ToastAction { id: "open-decisions".to_owned(), label: "Open".to_owned(), primary: true });
  }
  capped(Toast {
    id: format!("decision:{id}"),
    kind: Kind::Decision,
    title: "Decision needed".to_owned(),
    body: title.to_owned(),
    meta,
    actions,
    ttl_ms: None,
    data: Some(json!({ "decisionId": id, "seenMs": seen_ms })),
  })
}

pub fn ticket_of(title: &str) -> Option<&str> {
  let (head, _) = title.split_once([':', ' ']).unwrap_or((title, ""));
  let (prefix, number) = head.split_once('-')?;
  let ok = !prefix.is_empty()
    && prefix.chars().all(|c| c.is_ascii_uppercase())
    && !number.is_empty()
    && number.chars().all(|c| c.is_ascii_digit());
  ok.then_some(head)
}

pub fn duration_text(seconds: i64) -> String {
  let minutes = seconds.max(0) / 60;
  match (minutes / 60, minutes % 60) {
    (0, m) => format!("{m}m"),
    (h, m) => format!("{h}h {m:02}m"),
  }
}

pub fn idle_toast(task_title: &str, elapsed_secs: i64, idle_secs: i64, away_since: i64, others: usize) -> Toast {
  let label = ticket_of(task_title).unwrap_or(task_title);
  let label = if others > 0 { format!("{label} +{others}") } else { label.to_owned() };
  let minutes = idle_secs.max(0) / 60;
  capped(Toast {
    id: IDLE_ID.to_owned(),
    kind: Kind::Idle,
    title: "Timer still running".to_owned(),
    body: format!("{label} for {}, no activity {minutes} min.", duration_text(elapsed_secs)),
    meta: None,
    actions: vec![
      ToastAction { id: "stop".to_owned(), label: "Stop timer".to_owned(), primary: true },
      ToastAction { id: "keep".to_owned(), label: "Keep going".to_owned(), primary: false },
      ToastAction { id: "stop-at".to_owned(), label: format!("Stop at {minutes} min ago"), primary: false },
    ],
    ttl_ms: Some(None),
    data: Some(json!({ "awaySince": away_since })),
  })
}

pub fn push_idle<R: Runtime>(app: &AppHandle<R>, away_since: i64, now: i64) {
  let state = app.state::<TrayStore>().0.lock().unwrap().clone();
  let Some(task) = state.running.first() else { return };
  let elapsed = task.common_time + if task.start_time > 0 { now - task.start_time } else { 0 };
  push(app, idle_toast(&task.title, elapsed, now - away_since, away_since, state.running.len() - 1));
}

fn state_payload<R: Runtime>(app: &AppHandle<R>) -> Value {
  let Some(store) = app.try_state::<ToastStore>() else { return json!({ "visible": [], "more": 0, "expanded": false }) };
  let state = store.state.lock().unwrap();
  serde_json::to_value(payload_of(&state, now_ms())).unwrap_or_default()
}

#[tauri::command]
pub fn toast_state<R: Runtime>(app: AppHandle<R>) -> Value {
  state_payload(&app)
}

#[tauri::command]
pub fn toast_push<R: Runtime>(app: AppHandle<R>, toast: Toast) -> Result<(), String> {
  push(&app, clean(toast)?);
  Ok(())
}

#[tauri::command]
pub fn toast_dismiss<R: Runtime>(app: AppHandle<R>, id: String) {
  dismiss(&app, &id);
}

#[tauri::command]
pub fn toast_hover<R: Runtime>(app: AppHandle<R>, store: State<'_, ToastStore>, id: String, hovering: bool) {
  if store.state.lock().unwrap().hover(&id, hovering, now_ms()) {
    render(&app);
  }
}

#[tauri::command]
pub fn toast_expand<R: Runtime>(app: AppHandle<R>, store: State<'_, ToastStore>) {
  store.state.lock().unwrap().expanded = true;
  render(&app);
}

#[tauri::command]
pub fn toast_resize<R: Runtime>(app: AppHandle<R>, store: State<'_, ToastStore>, height: f64) -> Result<(), String> {
  if !height.is_finite() {
    return Err("bad height".to_owned());
  }
  let Some(window) = app.get_webview_window(LABEL) else { return Err("toast window missing".to_owned()) };
  let height = height.clamp(MIN_HEIGHT, MAX_HEIGHT);
  *store.height.lock().unwrap() = Some(height);
  window.set_size(LogicalSize::new(WIDTH, height)).map_err(|e| e.to_string())
}

#[derive(Debug, PartialEq)]
pub(crate) enum Route {
  Answer(String, String),
  OpenDecisions,
  Main,
}

pub(crate) fn route(toast: &Toast, action_id: &str) -> Result<Route, String> {
  if !toast.actions.iter().any(|a| a.id == action_id) {
    return Err(format!("unknown action {action_id}"));
  }
  if toast.kind == Kind::Decision {
    if let (Some(decision), Some(choice)) = (toast.decision_id(), action_id.strip_prefix("answer:")) {
      return Ok(Route::Answer(decision, choice.to_owned()));
    }
    if action_id == "open-decisions" {
      return Ok(Route::OpenDecisions);
    }
  }
  Ok(Route::Main)
}

#[tauri::command]
pub fn toast_action<R: Runtime>(
  app: AppHandle<R>,
  store: State<'_, ToastStore>,
  toast_id: String,
  action_id: String,
) -> Result<(), String> {
  let toast = {
    let state = store.state.lock().unwrap();
    state.queue.iter().find(|e| e.toast.id == toast_id).map(|e| e.toast.clone())
  }
  .ok_or_else(|| "toast is gone".to_owned())?;
  let route = route(&toast, &action_id)?;
  dismiss(&app, &toast_id);
  match route {
    #[cfg(unix)]
    Route::Answer(decision, choice) => {
      let answer = crate::decisions::AnswerIn { choice: Some(choice), seen_ms: toast.seen_ms(), ..Default::default() };
      tauri::async_runtime::spawn(async move {
        if let Err(e) = crate::decisions::decisions_answer(app, decision, answer).await {
          log::warn!("[toast] decision answer failed: {e}");
        }
      });
    }
    #[cfg(not(unix))]
    Route::Answer(..) => {}
    #[cfg(unix)]
    Route::OpenDecisions => crate::decisions::open_window(&app),
    #[cfg(not(unix))]
    Route::OpenDecisions => {}
    Route::Main => {
      let _ = app.emit_to(
        "main",
        "toast://action",
        json!({ "toastId": toast.id, "kind": toast.kind, "actionId": action_id, "data": toast.data }),
      );
    }
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn toast(id: &str, kind: Kind) -> Toast {
    Toast {
      id: id.to_owned(),
      kind,
      title: "T".to_owned(),
      body: "B".to_owned(),
      meta: None,
      actions: vec![ToastAction { id: "ok".to_owned(), label: "OK".to_owned(), primary: false }],
      ttl_ms: None,
      data: None,
    }
  }

  #[test]
  fn stack_shows_two_and_counts_the_rest() {
    let q: Vec<i32> = (0..5).collect();
    assert_eq!(stack(&q, false), (vec![0, 1], 3));
    assert_eq!(stack(&q, true), (vec![0, 1, 2, 3, 4], 0));
    assert_eq!(stack(&[1], false), (vec![1], 0));
    assert_eq!(stack::<i32>(&[], false), (vec![], 0));
    let many: Vec<i32> = (0..9).collect();
    assert_eq!(stack(&many, true), (vec![0, 1, 2, 3, 4, 5], 3));
  }

  #[test]
  fn gate_table() {
    let now = 1_000;
    let table = [
      (("off", None), false, true, Gate::Show),
      (("1h", Some(2_000)), false, true, Gate::Defer),
      (("1h", Some(1_000)), false, true, Gate::Show),
      (("1h", Some(500)), false, true, Gate::Show),
      (("1h", None), false, true, Gate::Show),
      (("off", Some(2_000)), false, true, Gate::Show),
      (("off", None), true, true, Gate::Defer),
      (("1h", Some(2_000)), true, true, Gate::Defer),
      (("off", None), false, false, Gate::Fallback),
      (("1h", Some(2_000)), true, false, Gate::Fallback),
    ];
    for (dnd, fullscreen, window_ok, expected) in table {
      assert_eq!(gate(dnd, now, fullscreen, window_ok), expected, "{dnd:?} {fullscreen} {window_ok}");
    }
  }

  #[test]
  fn enqueue_dedupes_by_id_and_puts_newest_first() {
    let mut s = Inner::default();
    s.enqueue(toast("a", Kind::Idle), 1);
    s.enqueue(toast("b", Kind::Idle), 2);
    let mut again = toast("a", Kind::Idle);
    again.title = "new".to_owned();
    s.enqueue(again, 3);
    let ids: Vec<_> = s.queue.iter().map(|e| e.toast.id.as_str()).collect();
    assert_eq!(ids, ["a", "b"]);
    assert_eq!(s.queue[0].toast.title, "new");
  }

  #[test]
  fn ttl_defaults_to_ten_seconds_for_every_kind() {
    assert_eq!(effective_ttl(None), Some(10_000));
    assert_eq!(effective_ttl(Some(Some(5))), Some(5));
    assert_eq!(effective_ttl(Some(None)), None);
    let mut s = Inner::default();
    for (i, kind) in [Kind::Agent, Kind::Decision, Kind::Idle, Kind::Limit].into_iter().enumerate() {
      s.enqueue(toast(&format!("t{i}"), kind), 0);
    }
    assert!(!s.expire(9_999));
    assert!(s.expire(10_000));
    assert!(s.queue.is_empty());
    assert_eq!(s.missed.len(), 4);
    assert!(!s.expire(1_000_000));
  }

  #[test]
  fn sticky_only_with_explicit_null_ttl() {
    let mut s = Inner::default();
    let mut t = toast("sticky", Kind::Decision);
    t.ttl_ms = Some(None);
    s.enqueue(t, 0);
    assert!(!s.expire(1_000_000));
    assert_eq!(s.queue[0].remaining(5), None);
  }

  #[test]
  fn expiry_moves_to_missed_but_dismiss_removes() {
    let mut s = Inner::default();
    s.enqueue(toast("a", Kind::Idle), 0);
    s.enqueue(toast("b", Kind::Idle), 0);
    assert!(s.remove("a"));
    assert!(s.missed.is_empty());
    assert!(s.expire(10_000));
    assert_eq!(s.missed.len(), 1);
    assert_eq!(s.missed[0].id, "b");
    assert_eq!(s.missed[0].at, 10);
  }

  #[test]
  fn hover_pauses_and_resumes_with_the_remaining_time() {
    let mut s = Inner::default();
    s.enqueue(toast("a", Kind::Agent), 0);
    assert_eq!(s.queue[0].remaining(4_000), Some(6_000));
    assert!(s.hover("a", true, 4_000));
    assert!(!s.hover("a", true, 5_000));
    assert_eq!(s.queue[0].remaining(50_000), Some(6_000));
    assert!(!s.expire(50_000));
    assert!(s.hover("a", false, 50_000));
    assert!(!s.hover("a", false, 50_001));
    assert_eq!(s.queue[0].remaining(53_000), Some(3_000));
    assert!(!s.expire(55_999));
    assert!(s.expire(56_000));
    assert!(!s.hover("a", true, 60_000));
  }

  #[test]
  fn missed_decisions_count_as_decision_toasts() {
    let mut s = Inner::default();
    assert!(!s.has_decisions());
    let mut t = toast("decision:d", Kind::Decision);
    t.data = Some(json!({"decisionId": "d"}));
    s.defer(t, 0);
    assert!(s.has_decisions());
    assert!(s.sync_decisions(&HashMap::new()));
    assert!(!s.has_decisions());
  }

  #[test]
  fn idle_toast_is_sticky() {
    let t = idle_toast("TM-1 x", 60, 60, 0, 0);
    assert_eq!(effective_ttl(t.ttl_ms), None);
    let mut s = Inner::default();
    s.enqueue(t, 0);
    assert!(!s.expire(3_600_000));
    assert_eq!(s.queue.len(), 1);
  }

  #[test]
  fn pushed_out_toast_loses_its_pause() {
    let mut s = Inner::default();
    s.enqueue(toast("a", Kind::Agent), 0);
    assert!(s.hover("a", true, 1_000));
    for id in ["b", "c"] {
      s.enqueue(toast(id, Kind::Agent), 2_000);
    }
    assert_eq!(s.queue[2].toast.id, "a");
    assert!(s.release_pauses(3_000, MAX_VISIBLE));
    assert_eq!(s.queue[2].paused_left, None);
    assert_eq!(s.queue[2].remaining(3_000), Some(9_000));
  }

  #[test]
  fn pause_is_capped_at_sixty_seconds() {
    let mut s = Inner::default();
    s.enqueue(toast("a", Kind::Agent), 0);
    assert!(s.hover("a", true, 1_000));
    assert!(!s.expire(60_999));
    assert!(s.queue[0].paused_left.is_some());
    assert!(s.expire(61_000));
    assert_eq!(s.queue[0].paused_left, None);
    assert!(!s.expire(61_001));
    assert!(s.expire(61_000 + 9_001));
    assert_eq!(s.missed.len(), 1);
  }

  #[test]
  fn hover_ignores_sticky_and_unknown_toasts() {
    let mut s = Inner::default();
    let mut t = toast("a", Kind::Agent);
    t.ttl_ms = Some(None);
    s.enqueue(t, 0);
    assert!(!s.hover("a", true, 1));
    assert!(!s.hover("nope", true, 1));
  }

  #[test]
  fn re_enqueue_restarts_the_countdown_and_clears_the_pause() {
    let mut s = Inner::default();
    s.enqueue(toast("a", Kind::Agent), 0);
    s.hover("a", true, 8_000);
    s.enqueue(toast("a", Kind::Agent), 9_000);
    assert_eq!(s.queue.len(), 1);
    assert_eq!(s.queue[0].remaining(9_000), Some(10_000));
    assert!(s.queue[0].paused_left.is_none());
    assert!(s.expire(19_000));
  }

  #[test]
  fn payload_carries_remaining_total_and_paused() {
    let mut s = Inner::default();
    s.enqueue(toast("a", Kind::Agent), 0);
    s.hover("a", true, 2_500);
    let v = serde_json::to_value(payload_of(&s, 9_000)).unwrap();
    let t = &v["visible"][0];
    assert_eq!(t["remainingMs"], 7_500);
    assert_eq!(t["totalMs"], 10_000);
    assert_eq!(t["paused"], true);
  }

  #[test]
  fn ttl_field_distinguishes_null_from_absent() {
    let absent: Toast = serde_json::from_value(json!({"id":"a","kind":"agent","title":"t","body":"b"})).unwrap();
    assert_eq!(absent.ttl_ms, None);
    let null: Toast = serde_json::from_value(json!({"id":"a","kind":"agent","title":"t","body":"b","ttlMs":null})).unwrap();
    assert_eq!(null.ttl_ms, Some(None));
    let num: Toast = serde_json::from_value(json!({"id":"a","kind":"agent","title":"t","body":"b","ttlMs":50})).unwrap();
    assert_eq!(num.ttl_ms, Some(Some(50)));
  }

  #[test]
  fn missed_list_is_capped_deduped_and_clearable() {
    let mut s = Inner::default();
    for i in 0..30 {
      s.defer(toast(&format!("t{i}"), Kind::Agent), i * 1000);
    }
    assert_eq!(s.missed.len(), 20);
    assert_eq!(s.missed[0].id, "t29");
    assert_eq!(s.missed[0].at, 29);
    s.defer(toast("t29", Kind::Agent), 99_000);
    assert_eq!(s.missed.len(), 20);
    let mut d = toast("decision:x", Kind::Decision);
    d.data = Some(json!({"decisionId": "x"}));
    s.defer(d, 0);
    assert!(s.remove_decision("x"));
    assert!(!s.remove_decision("x"));
    assert!(s.remove("t28"));
    assert!(!s.remove("t28"));
  }

  #[test]
  fn clean_validates_and_trims() {
    let mut t = toast(" id ", Kind::Agent);
    t.title = "x".repeat(500);
    t.actions = (0..9).map(|i| ToastAction { id: format!("a{i}"), label: "L".repeat(100), primary: false }).collect();
    let c = clean(t).unwrap();
    assert_eq!(c.id, "id");
    assert_eq!(c.title.chars().count(), 120);
    assert_eq!(c.actions.len(), 4);
    assert_eq!(c.actions[0].label.chars().count(), 40);
    let mut bad = toast("", Kind::Idle);
    assert!(clean(bad.clone()).is_err());
    bad.id = "x".to_owned();
    bad.title = "  ".to_owned();
    assert!(clean(bad).is_err());
  }

  #[test]
  fn decision_toast_marks_the_recommended_option_and_caps_buttons() {
    let options: Vec<(String, String, bool)> = [("a", "A", false), ("b", "B", false), ("c", "C", false), ("d", "D", true)]
      .into_iter()
      .map(|(i, l, r)| (i.to_owned(), l.to_owned(), r))
      .collect();
    let t = decision_toast("c1", "Tag it?", Some("agent".to_owned()), &options, Some(42));
    assert_eq!(t.id, "decision:c1");
    let ids: Vec<_> = t.actions.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(ids, ["answer:a", "answer:b", "answer:d"]);
    assert!(t.actions[2].primary);
    assert_eq!(t.actions[2].label, "D ★");
    assert_eq!(t.data, Some(json!({"decisionId": "c1", "seenMs": 42})));
    assert_eq!(t.seen_ms(), Some(42));
    assert_eq!(decision_toast("c2", "Q", None, &[], None).actions[0].id, "open-decisions");
  }

  #[test]
  fn action_routing_validates_the_action_id() {
    let t = decision_toast("c1", "Q", None, &[("go".to_owned(), "Go".to_owned(), true)], None);
    assert_eq!(route(&t, "answer:go"), Ok(Route::Answer("c1".to_owned(), "go".to_owned())));
    assert!(route(&t, "answer:nope").is_err());
    let idle = idle_toast("TM-1 x", 60, 60, 0, 0);
    assert_eq!(route(&idle, "stop"), Ok(Route::Main));
    assert!(route(&idle, "answer:go").is_err());
    let open = decision_toast("c3", "Q", None, &[], None);
    assert_eq!(route(&open, "open-decisions"), Ok(Route::OpenDecisions));
  }

  #[test]
  fn rust_built_toasts_respect_the_length_caps() {
    let long = "x".repeat(1_000);
    let options = vec![("a".to_owned(), "L".repeat(200), true)];
    let d = decision_toast(&"i".repeat(300), &long, Some(long.clone()), &options, None);
    assert_eq!(d.id.chars().count(), 128);
    assert_eq!(d.body.chars().count(), 300);
    assert_eq!(d.meta.as_deref().map(|m| m.chars().count()), Some(120));
    assert_eq!(d.actions[0].label.chars().count(), 40);
    let i = idle_toast(&long, 60, 60, 0, 0);
    assert!(i.body.chars().count() <= 300);
  }

  #[test]
  fn decision_toasts_follow_the_card() {
    let opt = |id: &str| (id.to_owned(), id.to_uppercase(), false);
    let mut s = Inner::default();
    s.enqueue(decision_toast("c1", "Q1", None, &[opt("a")], Some(1)), 0);
    s.enqueue(decision_toast("c2", "Q2", None, &[opt("a")], Some(1)), 0);
    s.enqueue(toast("idle", Kind::Idle), 0);
    s.defer(decision_toast("c3", "Q3", None, &[], Some(1)), 0);
    let mut open = HashMap::new();
    open.insert("c1".to_owned(), (1, vec![opt("a")]));
    open.insert("c2".to_owned(), (2, vec![opt("x"), opt("y")]));
    assert!(s.sync_decisions(&open));
    let ids: Vec<_> = s.queue.iter().map(|e| e.toast.id.as_str()).collect();
    assert_eq!(ids, ["idle", "decision:c2", "decision:c1"]);
    assert!(s.missed.is_empty());
    let c2 = &s.queue[1].toast;
    assert_eq!(c2.seen_ms(), Some(2));
    assert_eq!(c2.body, "Q2");
    let actions: Vec<_> = c2.actions.iter().map(|a| a.id.as_str()).collect();
    assert_eq!(actions, ["answer:x", "answer:y"]);
    assert!(!s.sync_decisions(&open));
  }

  #[test]
  fn idle_toast_text() {
    let t = idle_toast("TM-1020: Desktop Manager", 3 * 3600 + 5 * 60, 25 * 60, 100, 0);
    assert_eq!(t.body, "TM-1020 for 3h 05m, no activity 25 min.");
    assert_eq!(t.actions[2].label, "Stop at 25 min ago");
    assert!(t.actions[0].primary);
    assert_eq!(idle_toast("Plain task", 90, 600, 0, 2).body, "Plain task +2 for 1m, no activity 10 min.");
    assert_eq!(ticket_of("TM-9"), Some("TM-9"));
    assert_eq!(ticket_of("tm-9: x"), None);
    assert_eq!(ticket_of("Fix: x"), None);
  }

  #[test]
  fn toast_window_sits_in_the_top_right_of_the_work_area() {
    assert_eq!(place((0.0, 50.0, 3000.0, 1850.0), 2.0, 356.0), (3000 - 712 - 16, 50 + 16));
    assert_eq!(place((1920.0, 0.0, 1920.0, 1040.0), 1.0, 356.0), (3840 - 356 - 8, 8));
  }
}
