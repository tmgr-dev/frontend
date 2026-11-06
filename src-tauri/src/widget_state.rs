use std::io;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

use crate::tray::{elapsed, now_secs, sanitize_plugin_text, TrayState};

const MAX_TIMERS: usize = 3;
const TITLE_MAX: usize = 80;
const RELOAD_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetTimer {
  pub title: String,
  pub started_at: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetPayload {
  pub version: u32,
  pub updated_at: i64,
  pub total: usize,
  pub timers: Vec<WidgetTimer>,
}

impl WidgetPayload {
  fn same_content(&self, other: &Self) -> bool {
    self.version == other.version && self.total == other.total && self.timers == other.timers
  }
}

pub fn build_payload(state: &TrayState, now: i64) -> WidgetPayload {
  let timers = state
    .running
    .iter()
    .take(MAX_TIMERS)
    .map(|task| WidgetTimer {
      title: sanitize_plugin_text(&task.title, TITLE_MAX).unwrap_or_default(),
      started_at: now - elapsed(task, now),
    })
    .collect();
  WidgetPayload { version: 1, updated_at: now, total: state.running.len(), timers }
}

#[derive(Default)]
struct Publisher {
  last: Option<WidgetPayload>,
  last_reload: Option<Instant>,
  pending_reload: bool,
}

impl Publisher {
  /// Writes the payload unless it matches the previous one; returns whether a reload is due.
  fn publish(&mut self, dir: &Path, payload: WidgetPayload, at: Instant) -> io::Result<bool> {
    if self.last.as_ref().is_some_and(|last| last.same_content(&payload)) {
      return Ok(false);
    }
    write_atomic(dir, &payload)?;
    self.last = Some(payload);
    let due = self.reload_due(at);
    self.pending_reload = !due;
    if due {
      self.last_reload = Some(at);
    }
    Ok(due)
  }

  fn reload_due(&self, at: Instant) -> bool {
    self.last_reload.is_none_or(|t| at.duration_since(t) >= RELOAD_INTERVAL)
  }

  /// Returns true once when a reload skipped by the throttle can go out now.
  fn flush_pending(&mut self, at: Instant) -> bool {
    if !self.pending_reload || !self.reload_due(at) {
      return false;
    }
    self.pending_reload = false;
    self.last_reload = Some(at);
    true
  }
}

fn write_atomic(dir: &Path, payload: &WidgetPayload) -> io::Result<()> {
  use std::io::Write;

  std::fs::create_dir_all(dir)?;
  let json = serde_json::to_vec(payload).map_err(io::Error::other)?;
  let tmp = dir.join("timer.json.tmp");
  let mut options = std::fs::OpenOptions::new();
  options.write(true).create(true).truncate(true);
  #[cfg(unix)]
  std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
  let mut file = options.open(&tmp)?;
  file.write_all(&json)?;
  drop(file);
  std::fs::rename(&tmp, dir.join("timer.json"))
}

#[cfg(target_os = "macos")]
extern "C" {
  fn tmgr_widget_reload();
}

static PUBLISHER: Mutex<Option<Publisher>> = Mutex::new(None);

#[cfg(target_os = "macos")]
pub fn flush_pending_reload() {
  let mut guard = PUBLISHER.lock().unwrap_or_else(|e| e.into_inner());
  if guard.as_mut().is_some_and(|p| p.flush_pending(Instant::now())) {
    unsafe { tmgr_widget_reload() }
  }
}

#[cfg(not(target_os = "macos"))]
pub fn flush_pending_reload() {}

#[cfg(target_os = "macos")]
pub fn publish<R: Runtime>(app: &AppHandle<R>, state: &TrayState) {
  let dir = match app.path().app_data_dir() {
    Ok(dir) => dir.join("widget"),
    Err(err) => return log::warn!("[widget] no data dir: {err}"),
  };
  let payload = build_payload(state, now_secs());
  let mut guard = PUBLISHER.lock().unwrap_or_else(|e| e.into_inner());
  match guard.get_or_insert_with(Publisher::default).publish(&dir, payload, Instant::now()) {
    Ok(true) => unsafe { tmgr_widget_reload() },
    Ok(false) => {}
    Err(err) => log::warn!("[widget] write failed: {err}"),
  }
}

#[cfg(not(target_os = "macos"))]
pub fn publish<R: Runtime>(_app: &AppHandle<R>, _state: &TrayState) {}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::tray::TrayTask;

  fn task(id: i64, title: &str, common_time: i64, start_time: i64) -> TrayTask {
    TrayTask { id, title: title.into(), common_time, start_time, workspace_id: 0 }
  }

  fn state(running: Vec<TrayTask>) -> TrayState {
    TrayState { running, ..Default::default() }
  }

  #[test]
  fn empty_when_nothing_runs() {
    let json = serde_json::to_string(&build_payload(&state(vec![]), 1000)).unwrap();
    assert_eq!(json, r#"{"version":1,"updatedAt":1000,"total":0,"timers":[]}"#);
  }

  #[test]
  fn one_timer_matches_the_contract() {
    let json = serde_json::to_string(&build_payload(&state(vec![task(1, "Write", 0, 940)]), 1000)).unwrap();
    assert_eq!(json, r#"{"version":1,"updatedAt":1000,"total":1,"timers":[{"title":"Write","startedAt":940}]}"#);
  }

  #[test]
  fn caps_timers_at_three_but_counts_all() {
    let running = (1..=5).map(|i| task(i, &format!("T{i}"), 0, 900)).collect();
    let p = build_payload(&state(running), 1000);
    assert_eq!(p.total, 5);
    assert_eq!(p.timers.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["T1", "T2", "T3"]);
  }

  #[test]
  fn started_at_accounts_for_common_time() {
    let p = build_payload(&state(vec![task(1, "A", 100, 900)]), 1000);
    assert_eq!(p.timers[0].started_at, 800);
  }

  #[test]
  fn titles_lose_format_controls_and_are_truncated() {
    let long = "x".repeat(200);
    let p = build_payload(&state(vec![task(1, "a\u{202E}b\u{200B}c\nd", 0, 900), task(2, &long, 0, 900)]), 1000);
    assert_eq!(p.timers[0].title, "abc d");
    assert_eq!(p.timers[1].title.chars().count(), TITLE_MAX);
  }

  #[test]
  fn unchanged_payload_is_not_rewritten() {
    let dir = tempfile::tempdir().unwrap();
    let mut publisher = Publisher::default();
    let st = state(vec![task(1, "A", 0, 900)]);
    let now = Instant::now();
    assert!(publisher.publish(dir.path(), build_payload(&st, 1000), now).unwrap());
    std::fs::remove_file(dir.path().join("timer.json")).unwrap();
    assert!(!publisher.publish(dir.path(), build_payload(&st, 1005), now).unwrap());
    assert!(!dir.path().join("timer.json").exists());
  }

  #[test]
  fn changed_payload_is_written_atomically_and_reload_is_throttled() {
    let dir = tempfile::tempdir().unwrap();
    let mut publisher = Publisher::default();
    let t0 = Instant::now();
    assert!(publisher.publish(dir.path(), build_payload(&state(vec![task(1, "A", 0, 900)]), 1000), t0).unwrap());
    assert!(!publisher.publish(dir.path(), build_payload(&state(vec![]), 1001), t0 + Duration::from_secs(1)).unwrap());
    assert!(publisher.pending_reload);
    assert!(!publisher.flush_pending(t0 + Duration::from_secs(1)));
    assert!(publisher.flush_pending(t0 + Duration::from_secs(2)));
    assert!(!publisher.pending_reload);
    assert!(!publisher.flush_pending(t0 + Duration::from_secs(3)));
    assert!(publisher
      .publish(dir.path(), build_payload(&state(vec![task(2, "B", 0, 900)]), 1002), t0 + Duration::from_secs(5))
      .unwrap());
    let written: serde_json::Value =
      serde_json::from_slice(&std::fs::read(dir.path().join("timer.json")).unwrap()).unwrap();
    assert_eq!(written["timers"][0]["title"], "B");
    assert!(!dir.path().join("timer.json.tmp").exists());
  }
}
