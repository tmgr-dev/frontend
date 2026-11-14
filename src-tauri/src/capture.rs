use std::path::{Path, PathBuf};

use tauri::ipc::Response;
use tauri::{AppHandle, Manager, Runtime};

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

#[cfg(any(target_os = "macos", test))]
#[derive(Debug, PartialEq)]
enum Held {
  Empty,
  Text,
  Other,
}

#[cfg(any(target_os = "macos", test))]
const TEXT_TYPES: [&str; 4] = ["public.utf8-plain-text", "public.plain-text", "NSStringPboardType", "public.utf16-plain-text"];
#[cfg(any(target_os = "macos", test))]
const NON_TEXT_TYPES: [&str; 7] =
  ["public.file-url", "NSFilenamesPboardType", "public.png", "public.tiff", "public.jpeg", "public.image", "com.apple.pict"];

#[cfg(any(target_os = "macos", test))]
fn classify(types: &[String]) -> Held {
  if types.is_empty() {
    return Held::Empty;
  }
  let has = |list: &[&str]| types.iter().any(|t| list.contains(&t.as_str()));
  if has(&NON_TEXT_TYPES) || !has(&TEXT_TYPES) {
    Held::Other
  } else {
    Held::Text
  }
}

#[cfg(target_os = "macos")]
fn pasteboard_state() -> (Held, isize) {
  use objc2_app_kit::NSPasteboard;

  let board = NSPasteboard::generalPasteboard();
  let types: Vec<String> = board.types().map(|t| t.iter().map(|s| s.to_string()).collect()).unwrap_or_default();
  (classify(&types), board.changeCount())
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
  use std::time::Duration;
  use tauri_plugin_clipboard_manager::ClipboardExt;

  if !accessibility_trusted(true) {
    return Err("accessibility".into());
  }
  let clipboard = app.clipboard();
  let (held, count) = pasteboard_state();
  if held == Held::Other {
    return Err("clipboard_not_text".into());
  }
  let previous = if held == Held::Text { clipboard.read_text().ok() } else { None };
  press_copy()?;
  std::thread::sleep(Duration::from_millis(250));
  if pasteboard_state().1 == count {
    log::info!("[capture] selection nothing copied");
    return Ok(None);
  }
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

  fn kinds(types: &[&str]) -> Held {
    classify(&types.iter().map(|t| t.to_string()).collect::<Vec<_>>())
  }

  #[test]
  fn an_empty_clipboard_is_empty() {
    assert_eq!(kinds(&[]), Held::Empty);
  }

  #[test]
  fn plain_and_rich_text_count_as_text() {
    assert_eq!(kinds(&["public.utf8-plain-text"]), Held::Text);
    assert_eq!(kinds(&["public.html", "public.utf8-plain-text", "public.rtf"]), Held::Text);
  }

  #[test]
  fn images_files_and_unknown_content_are_not_text() {
    assert_eq!(kinds(&["public.png", "public.tiff"]), Held::Other);
    assert_eq!(kinds(&["public.file-url", "public.utf8-plain-text"]), Held::Other);
    assert_eq!(kinds(&["NSFilenamesPboardType"]), Held::Other);
    assert_eq!(kinds(&["com.example.private"]), Held::Other);
  }

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
