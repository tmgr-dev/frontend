use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_opener::OpenerExt;

const MAX_FILE_BYTES: u64 = 1024 * 1024;

#[derive(Serialize, Debug, PartialEq)]
pub struct FolderPlugin {
  folder: String,
  manifest: String,
  code: String,
}

fn root<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
  let home = app.path().home_dir().map_err(|e| e.to_string())?;
  Ok(home.join(".tmgr.dev").join("plugins"))
}

/// A plain file of at most 1 MB. The size is enforced on the read itself, so a file swapped for a
/// symlink or a device between the check and the read still yields at most MAX_FILE_BYTES + 1 bytes.
fn read_small(path: &Path) -> Option<String> {
  if !fs::symlink_metadata(path).ok()?.is_file() {
    return None;
  }
  let file = fs::File::open(path).ok()?;
  if !file.metadata().ok()?.is_file() {
    return None;
  }
  let mut bytes = Vec::new();
  file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).ok()?;
  if bytes.len() as u64 > MAX_FILE_BYTES {
    return None;
  }
  String::from_utf8(bytes).ok()
}

/// Every `<root>/<folder>/` with a `manifest.json` and a `main.js` (plain files, at most 1 MB each).
/// The app validates the manifest; nothing here runs plugin code.
pub fn list_in(root: &Path) -> Vec<FolderPlugin> {
  let Ok(entries) = fs::read_dir(root) else { return Vec::new() };
  let mut plugins: Vec<FolderPlugin> = entries
    .flatten()
    .filter(|entry| entry.file_type().map(|t| t.is_dir()).unwrap_or(false))
    .filter_map(|entry| {
      let dir = entry.path();
      Some(FolderPlugin {
        folder: entry.file_name().to_string_lossy().into_owned(),
        manifest: read_small(&dir.join("manifest.json"))?,
        code: read_small(&dir.join("main.js"))?,
      })
    })
    .collect();
  plugins.sort_by(|a, b| a.folder.cmp(&b.folder));
  plugins
}

#[tauri::command]
pub fn plugins_dev_list<R: Runtime>(app: AppHandle<R>) -> Result<Vec<FolderPlugin>, String> {
  Ok(list_in(&root(&app)?))
}

#[tauri::command]
pub fn plugins_dev_reveal<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
  let dir = root(&app)?;
  fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
  app.opener().reveal_item_in_dir(&dir).map_err(|e| e.to_string())
}

/// `--safe-mode` on the command line starts the app with every plugin off.
#[tauri::command]
pub fn plugins_safe_mode() -> bool {
  std::env::args().any(|arg| arg == "--safe-mode")
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn lists_folders_with_a_manifest_and_code_only() {
    let root = std::env::temp_dir().join(format!("tmgr-plugins-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("b.good")).unwrap();
    fs::write(root.join("b.good/manifest.json"), "{}").unwrap();
    fs::write(root.join("b.good/main.js"), "1").unwrap();
    fs::create_dir_all(root.join("a.no-code")).unwrap();
    fs::write(root.join("a.no-code/manifest.json"), "{}").unwrap();
    fs::create_dir_all(root.join("c.huge")).unwrap();
    fs::write(root.join("c.huge/manifest.json"), "{}").unwrap();
    fs::write(root.join("c.huge/main.js"), vec![b'x'; (MAX_FILE_BYTES + 1) as usize]).unwrap();
    fs::write(root.join("loose.js"), "1").unwrap();

    assert_eq!(
      list_in(&root),
      vec![FolderPlugin { folder: "b.good".into(), manifest: "{}".into(), code: "1".into() }]
    );
    assert!(list_in(&root.join("missing")).is_empty());
    let _ = fs::remove_dir_all(&root);
  }
}
