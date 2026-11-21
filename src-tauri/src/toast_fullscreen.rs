#![cfg_attr(not(any(target_os = "macos", windows)), allow(dead_code))]

use std::sync::Mutex;

pub const CACHE_MS: u64 = 2_000;
const EDGE_TOLERANCE: f64 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
  pub x: f64,
  pub y: f64,
  pub w: f64,
  pub h: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Win {
  pub layer: i64,
  pub bounds: Rect,
  pub pid: i64,
}

fn same(a: Rect, b: Rect) -> bool {
  (a.x - b.x).abs() <= EDGE_TOLERANCE
    && (a.y - b.y).abs() <= EDGE_TOLERANCE
    && (a.w - b.w).abs() <= EDGE_TOLERANCE
    && (a.h - b.h).abs() <= EDGE_TOLERANCE
}

/// `windows` is ordered front to back. The frontmost normal window is fullscreen when it fills a whole display and is not ours.
pub fn front_covers_display(windows: &[Win], displays: &[Rect], own_pid: i64) -> bool {
  let Some(front) = windows.iter().find(|w| w.layer == 0 && w.bounds.w > 0.0 && w.bounds.h > 0.0) else {
    return false;
  };
  front.pid != own_pid && displays.iter().any(|d| same(*d, front.bounds))
}

/// QUNS_BUSY (a fullscreen app), QUNS_RUNNING_D3D_FULL_SCREEN and QUNS_PRESENTATION_MODE.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn quns_is_fullscreen(state: i32) -> bool {
  matches!(state, 2..=4)
}

#[derive(Default)]
pub struct Cache(Mutex<Option<(u64, bool)>>);

impl Cache {
  pub fn get(&self, now_ms: u64, probe: impl FnOnce() -> bool) -> bool {
    let mut slot = self.0.lock().unwrap();
    if let Some((at, value)) = *slot {
      if now_ms.saturating_sub(at) < CACHE_MS {
        return value;
      }
    }
    let value = probe();
    *slot = Some((now_ms, value));
    value
  }
}

#[cfg(target_os = "macos")]
pub fn probe() -> bool {
  use core_foundation::base::{CFType, TCFType};
  use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
  use core_foundation::number::CFNumber;
  use core_foundation::string::CFString;
  use core_graphics::display::{CGDisplay, CGRect};
  use core_graphics::window::{
    copy_window_info, kCGNullWindowID, kCGWindowBounds, kCGWindowLayer, kCGWindowListOptionOnScreenOnly,
    kCGWindowOwnerPID,
  };

  #[link(name = "CoreGraphics", kind = "framework")]
  extern "C" {
    fn CGRectMakeWithDictionaryRepresentation(dict: CFDictionaryRef, rect: *mut CGRect) -> bool;
  }

  let Some(list) = copy_window_info(kCGWindowListOptionOnScreenOnly, kCGNullWindowID) else { return false };
  let (layer_key, bounds_key, pid_key) = unsafe {
    (
      CFString::wrap_under_get_rule(kCGWindowLayer),
      CFString::wrap_under_get_rule(kCGWindowBounds),
      CFString::wrap_under_get_rule(kCGWindowOwnerPID),
    )
  };
  let mut windows = Vec::new();
  for item in list.iter() {
    let dict: CFDictionary<CFString, CFType> = unsafe { CFDictionary::wrap_under_get_rule(*item as CFDictionaryRef) };
    let number = |key: &CFString| dict.find(key).and_then(|v| v.downcast::<CFNumber>()).and_then(|n| n.to_i64());
    let (Some(layer), Some(pid)) = (number(&layer_key), number(&pid_key)) else { continue };
    let Some(bounds) = dict.find(&bounds_key) else { continue };
    let mut rect = CGRect::default();
    if !unsafe { CGRectMakeWithDictionaryRepresentation(bounds.as_CFTypeRef() as CFDictionaryRef, &mut rect) } {
      continue;
    }
    windows.push(Win {
      layer,
      pid,
      bounds: Rect { x: rect.origin.x, y: rect.origin.y, w: rect.size.width, h: rect.size.height },
    });
  }
  let displays: Vec<Rect> = CGDisplay::active_displays()
    .unwrap_or_default()
    .into_iter()
    .map(|id| {
      let b = CGDisplay::new(id).bounds();
      Rect { x: b.origin.x, y: b.origin.y, w: b.size.width, h: b.size.height }
    })
    .collect();
  front_covers_display(&windows, &displays, i64::from(std::process::id()))
}

#[cfg(windows)]
pub fn probe() -> bool {
  use windows_sys::Win32::UI::Shell::SHQueryUserNotificationState;
  let mut state = 0;
  let hr = unsafe { SHQueryUserNotificationState(&mut state) };
  hr >= 0 && quns_is_fullscreen(state)
}

#[cfg(not(any(target_os = "macos", windows)))]
pub fn probe() -> bool {
  false
}

#[cfg(test)]
mod tests {
  use super::*;

  const DISPLAY: Rect = Rect { x: 0.0, y: 0.0, w: 1512.0, h: 982.0 };
  const MAXIMIZED: Rect = Rect { x: 0.0, y: 38.0, w: 1512.0, h: 944.0 };

  fn win(layer: i64, bounds: Rect, pid: i64) -> Win {
    Win { layer, bounds, pid }
  }

  #[test]
  fn a_foreign_window_filling_the_display_is_fullscreen() {
    assert!(front_covers_display(&[win(0, DISPLAY, 10)], &[DISPLAY], 99));
  }

  #[test]
  fn maximized_windows_below_the_menu_bar_are_not() {
    assert!(!front_covers_display(&[win(0, MAXIMIZED, 10)], &[DISPLAY], 99));
  }

  #[test]
  fn our_own_fullscreen_window_does_not_count() {
    assert!(!front_covers_display(&[win(0, DISPLAY, 99)], &[DISPLAY], 99));
  }

  #[test]
  fn only_the_frontmost_normal_window_decides() {
    let list = [win(25, MAXIMIZED, 1), win(0, MAXIMIZED, 10), win(0, DISPLAY, 11)];
    assert!(!front_covers_display(&list, &[DISPLAY], 99));
    let list = [win(3, MAXIMIZED, 1), win(0, DISPLAY, 11), win(0, MAXIMIZED, 10)];
    assert!(front_covers_display(&list, &[DISPLAY], 99));
  }

  #[test]
  fn any_display_matches_and_empty_lists_are_not_fullscreen() {
    let second = Rect { x: 1512.0, y: 0.0, w: 1920.0, h: 1080.0 };
    assert!(front_covers_display(&[win(0, second, 10)], &[DISPLAY, second], 99));
    assert!(!front_covers_display(&[], &[DISPLAY], 99));
    assert!(!front_covers_display(&[win(0, DISPLAY, 10)], &[], 99));
  }

  #[test]
  fn windows_notification_states() {
    for (state, expected) in [(1, false), (2, true), (3, true), (4, true), (5, false), (6, false), (7, false)] {
      assert_eq!(quns_is_fullscreen(state), expected, "{state}");
    }
  }

  #[test]
  fn the_probe_result_is_cached_for_two_seconds() {
    let cache = Cache::default();
    let mut calls = 0;
    assert!(cache.get(10_000, || {
      calls += 1;
      true
    }));
    assert!(cache.get(11_900, || {
      calls += 1;
      false
    }));
    assert!(!cache.get(12_000, || {
      calls += 1;
      false
    }));
    assert_eq!(calls, 2);
  }
}
