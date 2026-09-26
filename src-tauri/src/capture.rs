use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::ipc::Response;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::tray::now_secs;

fn capture_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
  let dir = app.path().temp_dir().map_err(|e| e.to_string())?.join("tmgr-captures");
  std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
  Ok(dir)
}

pub fn is_inside(dir: &Path, path: &Path) -> bool {
  match (dir.canonicalize(), path.canonicalize()) {
    (Ok(dir), Ok(path)) => path.starts_with(dir),
    _ => false,
  }
}

#[tauri::command]
pub async fn capture_screenshot<R: Runtime>(app: AppHandle<R>) -> Result<Option<String>, String> {
  let path = capture_dir(&app)?.join(format!("screenshot-{}.png", now_secs()));
  let status = std::process::Command::new("/usr/sbin/screencapture")
    .arg("-i")
    .arg("-x")
    .arg(&path)
    .status()
    .map_err(|e| e.to_string())?;
  log::info!("[capture] screencapture exit={status}");
  Ok(path.exists().then(|| path.to_string_lossy().into_owned()))
}

#[tauri::command]
pub fn take_capture<R: Runtime>(app: AppHandle<R>, path: String) -> Result<Response, String> {
  let path = PathBuf::from(path);
  if !is_inside(&capture_dir(&app)?, &path) {
    return Err("not a capture file".into());
  }
  let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
  let _ = std::fs::remove_file(&path);
  Ok(Response::new(bytes))
}

#[cfg(target_os = "macos")]
fn accessibility_trusted(prompt: bool) -> bool {
  use core_foundation::base::TCFType;
  use core_foundation::boolean::CFBoolean;
  use core_foundation::dictionary::CFDictionary;
  use core_foundation::string::{CFString, CFStringRef};

  #[link(name = "ApplicationServices", kind = "framework")]
  extern "C" {
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn AXIsProcessTrustedWithOptions(options: core_foundation::dictionary::CFDictionaryRef) -> bool;
  }
  unsafe {
    let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
    let options = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::from(prompt))]);
    AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef())
  }
}

#[cfg(target_os = "macos")]
fn press_copy() -> Result<(), String> {
  use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
  use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

  const KEY_C: u16 = 8;
  let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState).map_err(|_| "event source")?;
  for down in [true, false] {
    let event = CGEvent::new_keyboard_event(source.clone(), KEY_C, down).map_err(|_| "key event")?;
    event.set_flags(CGEventFlags::CGEventFlagCommand);
    event.post(CGEventTapLocation::HID);
  }
  Ok(())
}

#[cfg(target_os = "macos")]
#[tauri::command]
pub async fn capture_selection<R: Runtime>(app: AppHandle<R>) -> Result<Option<String>, String> {
  if !accessibility_trusted(true) {
    return Err("accessibility".into());
  }
  let clipboard = app.clipboard();
  let previous = clipboard.read_text().ok();
  let _ = clipboard.clear();
  std::thread::sleep(Duration::from_millis(120));
  press_copy()?;
  std::thread::sleep(Duration::from_millis(250));
  let text = clipboard.read_text().ok().filter(|t| !t.trim().is_empty());
  match previous {
    Some(previous) => {
      let _ = clipboard.write_text(previous);
    }
    None => {
      let _ = clipboard.clear();
    }
  }
  log::info!("[capture] selection {} chars", text.as_ref().map_or(0, |t| t.chars().count()));
  Ok(text)
}

#[cfg(not(target_os = "macos"))]
#[tauri::command]
pub async fn capture_selection<R: Runtime>(_app: AppHandle<R>) -> Result<Option<String>, String> {
  Ok(None)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn only_files_inside_the_capture_dir_are_readable() {
    let dir = std::env::temp_dir().join("tmgr-capture-test");
    std::fs::create_dir_all(&dir).unwrap();
    let inside = dir.join("a.png");
    std::fs::write(&inside, b"x").unwrap();
    let outside = std::env::temp_dir().join("tmgr-capture-outside.png");
    std::fs::write(&outside, b"x").unwrap();

    assert!(is_inside(&dir, &inside));
    assert!(!is_inside(&dir, &outside));
    assert!(!is_inside(&dir, &dir.join("../tmgr-capture-outside.png")));
    assert!(!is_inside(&dir, &dir.join("missing.png")));
  }
}
