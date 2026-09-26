use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use tauri_plugin_opener::OpenerExt;

/// Where each download was sent: WebKit reports no path when it finishes.
#[derive(Default)]
pub struct PendingDownloads(Mutex<HashMap<String, PathBuf>>);

#[derive(Clone, Serialize)]
struct Finished {
  name: Option<String>,
  path: Option<String>,
  success: bool,
}

pub fn remember<R: Runtime>(app: &AppHandle<R>, url: &str, path: &Path) {
  if let Ok(mut pending) = app.state::<PendingDownloads>().0.lock() {
    pending.insert(url.to_string(), path.to_path_buf());
  }
}

pub fn finished<R: Runtime>(app: &AppHandle<R>, url: &str, path: Option<PathBuf>, success: bool) {
  let remembered = app.state::<PendingDownloads>().0.lock().ok().and_then(|mut p| p.remove(url));
  let path = path.or(remembered);
  let payload = Finished {
    name: path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()),
    path: path.map(|p| p.to_string_lossy().into_owned()),
    success,
  };
  let _ = app.emit("download://finished", payload);
}

pub fn inside(dir: &Path, path: &Path) -> bool {
  path.is_absolute()
    && path.starts_with(dir)
    && path.components().all(|c| !matches!(c, Component::ParentDir))
}

/// Shows a finished download selected in Finder. Only files in the downloads folder.
#[tauri::command]
pub fn reveal_download<R: Runtime>(app: AppHandle<R>, path: String) -> Result<(), String> {
  let dir = app.path().download_dir().map_err(|e| e.to_string())?;
  let path = PathBuf::from(path);
  if !inside(&dir, &path) {
    return Err("not a download".into());
  }
  if !path.exists() {
    return Err("the file is no longer there".into());
  }
  app.opener().reveal_item_in_dir(&path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn only_paths_inside_the_downloads_folder_are_revealed() {
    let dir = Path::new("/home/user/Downloads");
    assert!(inside(dir, Path::new("/home/user/Downloads/report.pdf")));
    assert!(!inside(dir, Path::new("/home/user/Downloads/../.ssh/id_rsa")));
    assert!(!inside(dir, Path::new("/home/user/Documents/report.pdf")));
    assert!(!inside(dir, Path::new("Downloads/report.pdf")));
  }
}
