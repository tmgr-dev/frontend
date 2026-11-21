use std::time::Duration;

use serde::Serialize;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::tray::{now_secs, TrayStore};

const POLL_SECS: i64 = 5;
const BACK_WITHIN_SECS: f64 = 5.0;
const SLEEP_GAP_SECS: i64 = 60;
const DEFAULT_IDLE_SECS: i64 = 10 * 60;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AwayEvent {
  pub away_since: i64,
  pub away_seconds: i64,
}

#[derive(Debug)]
pub struct IdleTracker {
  threshold: i64,
  last_poll: Option<i64>,
  was_running: bool,
  away_since: Option<i64>,
  announced: bool,
  pending_start: Option<i64>,
}

impl IdleTracker {
  pub fn new(threshold: i64) -> Self {
    Self { threshold, last_poll: None, was_running: false, away_since: None, announced: false, pending_start: None }
  }

  pub fn tick(&mut self, now: i64, idle_secs: f64, timer_running: bool) -> Option<AwayEvent> {
    let previous = self.last_poll.replace(now);
    let was_running = std::mem::replace(&mut self.was_running, timer_running);
    if !timer_running {
      self.away_since = None;
      self.announced = false;
      return None;
    }
    if let (Some(previous), true) = (previous, was_running) {
      if now - previous > POLL_SECS + SLEEP_GAP_SECS && self.away_since.is_none() {
        self.away_since = Some(previous);
      }
    }
    if idle_secs as i64 >= self.threshold && self.away_since.is_none() {
      self.away_since = Some(now - idle_secs as i64);
    }
    match self.away_since {
      Some(since) if idle_secs < BACK_WITHIN_SECS => {
        self.away_since = None;
        self.announced = false;
        Some(AwayEvent { away_since: since, away_seconds: now - since })
      }
      Some(since) => {
        if !self.announced {
          self.announced = true;
          self.pending_start = Some(since);
        }
        None
      }
      None => None,
    }
  }

  /// The start of an away period, once per period, as soon as the user is known to still be away.
  pub fn take_away_started(&mut self) -> Option<i64> {
    self.pending_start.take()
  }
}

#[cfg(target_os = "macos")]
pub(crate) fn system_idle_secs() -> f64 {
  #[link(name = "CoreGraphics", kind = "framework")]
  extern "C" {
    fn CGEventSourceSecondsSinceLastEventType(state: i32, event_type: u32) -> f64;
  }
  const COMBINED_SESSION_STATE: i32 = 0;
  const ANY_INPUT_EVENT: u32 = u32::MAX;
  unsafe { CGEventSourceSecondsSinceLastEventType(COMBINED_SESSION_STATE, ANY_INPUT_EVENT) }
}

#[cfg(windows)]
pub(crate) fn system_idle_secs() -> f64 {
  use windows_sys::Win32::System::SystemInformation::GetTickCount;
  use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};
  let mut info = LASTINPUTINFO { cbSize: std::mem::size_of::<LASTINPUTINFO>() as u32, dwTime: 0 };
  if unsafe { GetLastInputInfo(&mut info) } == 0 {
    return 0.0;
  }
  let now = unsafe { GetTickCount() };
  f64::from(now.wrapping_sub(info.dwTime)) / 1000.0
}

#[cfg(not(any(target_os = "macos", windows)))]
pub(crate) fn system_idle_secs() -> f64 {
  0.0
}

/// False where `system_idle_secs` has no real source, so the UI can hide idle-related settings.
#[tauri::command]
pub fn idle_supported() -> bool {
  cfg!(any(target_os = "macos", windows))
}

pub fn start<R: Runtime>(app: &AppHandle<R>) {
  let threshold = std::env::var("TMGR_IDLE_SECONDS")
    .ok()
    .and_then(|v| v.parse().ok())
    .unwrap_or(DEFAULT_IDLE_SECS);
  let app = app.clone();
  std::thread::spawn(move || {
    let mut tracker = IdleTracker::new(threshold);
    loop {
      std::thread::sleep(Duration::from_secs(POLL_SECS as u64));
      let running = !app.state::<TrayStore>().0.lock().unwrap().running.is_empty();
      let now = now_secs();
      let returned = tracker.tick(now, system_idle_secs(), running);
      if let Some(since) = tracker.take_away_started() {
        log::info!("[idle] away since {since}");
        let _ = app.emit("idle://away-started", json!({ "awaySince": since }));
        crate::toast::push_idle(&app, since, now);
      }
      if let Some(event) = returned {
        log::info!("[idle] back after {}s away", event.away_seconds);
        crate::toast::dismiss(&app, crate::toast::IDLE_ID);
        let _ = app.emit("idle://returned", event);
      }
    }
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  fn poll_until(t: &mut IdleTracker, from: i64, to: i64, idle_at: impl Fn(i64) -> f64, running: bool) -> Vec<AwayEvent> {
    (from..=to)
      .step_by(POLL_SECS as usize)
      .filter_map(|now| t.tick(now, idle_at(now), running))
      .collect()
  }

  #[test]
  fn reports_nothing_while_active() {
    let mut t = IdleTracker::new(600);
    assert!(poll_until(&mut t, 1_000, 2_000, |_| 1.0, true).is_empty());
  }

  #[test]
  fn reports_idle_period_once_the_user_is_back() {
    let mut t = IdleTracker::new(600);
    let events = poll_until(&mut t, 1_000, 1_800, |now| if now < 1_750 { (now - 1_000) as f64 } else { 1.0 }, true);
    assert_eq!(events, vec![AwayEvent { away_since: 1_000, away_seconds: 750 }]);
  }

  #[test]
  fn short_breaks_below_threshold_are_ignored() {
    let mut t = IdleTracker::new(600);
    let events = poll_until(&mut t, 1_000, 1_400, |now| if now < 1_300 { (now - 1_000) as f64 } else { 1.0 }, true);
    assert!(events.is_empty());
  }

  #[test]
  fn a_wall_clock_gap_counts_as_sleep() {
    let mut t = IdleTracker::new(600);
    t.tick(1_000, 1.0, true);
    assert_eq!(t.tick(4_000, 1.0, true), Some(AwayEvent { away_since: 1_000, away_seconds: 3_000 }));
  }

  #[test]
  fn nothing_is_tracked_without_a_running_timer() {
    let mut t = IdleTracker::new(600);
    assert!(poll_until(&mut t, 1_000, 1_800, |now| if now < 1_750 { (now - 1_000) as f64 } else { 1.0 }, false).is_empty());
    assert_eq!(t.tick(4_000, 1.0, true), None);
  }

  #[test]
  fn away_start_is_reported_once_per_period() {
    let mut t = IdleTracker::new(600);
    let mut starts = Vec::new();
    for now in (1_000..=2_000).step_by(POLL_SECS as usize) {
      t.tick(now, (now - 1_000) as f64, true);
      starts.extend(t.take_away_started());
    }
    assert_eq!(starts, vec![1_000]);
    t.tick(2_005, 1.0, true);
    assert_eq!(t.take_away_started(), None);
    let mut again = Vec::new();
    for now in (2_010..=3_000).step_by(POLL_SECS as usize) {
      t.tick(now, (now - 2_005) as f64, true);
      again.extend(t.take_away_started());
    }
    assert_eq!(again, vec![2_005]);
  }

  #[test]
  fn no_away_start_while_active_or_without_a_timer() {
    let mut t = IdleTracker::new(600);
    for now in (1_000..=2_000).step_by(POLL_SECS as usize) {
      t.tick(now, 1.0, true);
      assert_eq!(t.take_away_started(), None);
    }
    for now in (3_000..=4_000).step_by(POLL_SECS as usize) {
      t.tick(now, (now - 3_000) as f64, false);
      assert_eq!(t.take_away_started(), None);
    }
  }

  #[test]
  fn a_sleep_gap_returns_without_an_away_start() {
    let mut t = IdleTracker::new(600);
    t.tick(1_000, 1.0, true);
    assert!(t.tick(4_000, 1.0, true).is_some());
    assert_eq!(t.take_away_started(), None);
  }
}
