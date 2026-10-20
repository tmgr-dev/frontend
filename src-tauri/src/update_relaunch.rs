use std::path::Path;
#[cfg(target_os = "macos")]
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager, Runtime};

const MARKER: &str = "update-relaunch";
const FRESH_SECS: u64 = 600;

#[cfg(target_os = "macos")]
static RELAUNCH_BUNDLE: OnceLock<PathBuf> = OnceLock::new();

fn now_secs() -> u64 {
  SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn start_visible(has_hidden: bool, marker_age_secs: Option<u64>) -> bool {
  !has_hidden || is_fresh(marker_age_secs)
}

pub fn is_fresh(marker_age_secs: Option<u64>) -> bool {
  marker_age_secs.is_some_and(|age| age <= FRESH_SECS)
}

fn write_marker(dir: &Path, now: u64) -> std::io::Result<()> {
  std::fs::create_dir_all(dir)?;
  std::fs::write(dir.join(MARKER), now.to_string())
}

/// One-shot: the marker is deleted whatever it holds. A marker from the future counts as missing.
fn take_marker_age(dir: &Path, now: u64) -> Option<u64> {
  let path = dir.join(MARKER);
  let content = std::fs::read_to_string(&path).ok()?;
  let _ = std::fs::remove_file(&path);
  now.checked_sub(content.trim().parse().ok()?)
}

pub fn consume_marker<R: Runtime>(app: &AppHandle<R>) -> Option<u64> {
  take_marker_age(&app.path().app_cache_dir().ok()?, now_secs())
}

#[tauri::command]
pub fn mark_update_relaunch<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
  let dir = app.path().app_cache_dir().map_err(|e| e.to_string())?;
  write_marker(&dir, now_secs()).map_err(|e| e.to_string())
}

/// macOS: relaunch through LaunchServices so the new instance is activated and gets no `--hidden`.
#[tauri::command]
pub fn relaunch_after_update<R: Runtime>(app: AppHandle<R>) {
  #[cfg(target_os = "macos")]
  if let Some(bundle) = std::env::current_exe().ok().as_deref().and_then(app_bundle) {
    let _ = RELAUNCH_BUNDLE.set(bundle);
    app.exit(0);
    return;
  }
  app.request_restart();
}

#[cfg(target_os = "macos")]
fn app_bundle(exe: &Path) -> Option<PathBuf> {
  let macos = exe.parent()?;
  let contents = macos.parent()?;
  let bundle = contents.parent()?;
  let is_bundle = macos.file_name()? == "MacOS"
    && contents.file_name()? == "Contents"
    && bundle.extension()? == "app";
  is_bundle.then(|| bundle.to_path_buf())
}

#[cfg(target_os = "macos")]
pub fn open_pending_bundle<R: Runtime>(app: &AppHandle<R>) {
  let Some(bundle) = RELAUNCH_BUNDLE.get() else { return };
  match std::process::Command::new("/usr/bin/open").arg("-n").arg(bundle).status() {
    Ok(status) if status.success() => {
      log::info!("[updater] relaunched {}", bundle.display());
      return;
    }
    Ok(status) => log::error!("[updater] open -n {} exited with {status}", bundle.display()),
    Err(error) => log::error!("[updater] open -n {} failed: {error}", bundle.display()),
  }
  tauri::process::restart(&app.env());
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn hidden_launch_without_marker_stays_hidden() {
    assert!(!start_visible(true, None));
  }

  #[test]
  fn hidden_launch_with_fresh_marker_is_visible() {
    assert!(start_visible(true, Some(0)));
    assert!(start_visible(true, Some(FRESH_SECS)));
  }

  #[test]
  fn hidden_launch_with_stale_marker_stays_hidden() {
    assert!(!start_visible(true, Some(FRESH_SECS + 1)));
  }

  #[test]
  fn launch_without_hidden_is_visible() {
    assert!(start_visible(false, None));
    assert!(start_visible(false, Some(FRESH_SECS + 1)));
  }

  #[test]
  fn marker_is_read_once_and_deleted() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().join("cache");
    write_marker(&dir, 1_000).unwrap();

    assert_eq!(take_marker_age(&dir, 1_030), Some(30));
    assert!(!dir.join(MARKER).exists());
    assert_eq!(take_marker_age(&dir, 1_030), None);
  }

  #[test]
  fn missing_marker_is_none() {
    let tmp = tempfile::tempdir().unwrap();
    assert_eq!(take_marker_age(tmp.path(), 1_000), None);
  }

  #[test]
  fn garbled_marker_is_none_and_deleted() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join(MARKER), "not a number").unwrap();

    assert_eq!(take_marker_age(tmp.path(), 1_000), None);
    assert!(!tmp.path().join(MARKER).exists());
  }

  #[test]
  fn marker_from_the_future_counts_as_missing() {
    let tmp = tempfile::tempdir().unwrap();
    write_marker(tmp.path(), 2_000).unwrap();

    assert_eq!(take_marker_age(tmp.path(), 1_000), None);
    assert!(!tmp.path().join(MARKER).exists());
  }

  #[cfg(target_os = "macos")]
  #[test]
  fn finds_the_app_bundle_of_the_executable() {
    assert_eq!(
      app_bundle(Path::new("/Applications/TMGR.app/Contents/MacOS/tmgr")),
      Some(PathBuf::from("/Applications/TMGR.app"))
    );
    assert_eq!(app_bundle(Path::new("/repo/src-tauri/target/debug/tmgr")), None);
  }
}
