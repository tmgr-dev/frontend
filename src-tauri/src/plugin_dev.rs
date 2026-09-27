use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
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
  /// `ui/<name>.html` pages, at most 10.
  pages: Vec<(String, String)>,
}

fn read_pages(dir: &Path) -> Vec<(String, String)> {
  let Ok(entries) = fs::read_dir(dir.join("ui")) else { return Vec::new() };
  let mut pages: Vec<(String, String)> = entries
    .flatten()
    .filter_map(|entry| {
      let name = entry.file_name().to_string_lossy().into_owned();
      if !name.ends_with(".html") || name.starts_with('.') {
        return None;
      }
      Some((format!("ui/{name}"), read_small(&entry.path())?))
    })
    .collect();
  pages.sort();
  pages.truncate(10);
  pages
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
        pages: read_pages(&dir),
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

#[derive(Serialize, Debug, PartialEq)]
pub struct FolderFingerprint {
  folder: String,
  fingerprint: String,
}

/// A hash of exactly what `list_in` loads for this folder (manifest.json, main.js, `ui/*.html`, the same
/// bounds as `read_small`/`read_pages`), so hot reload watches the same files the plugin actually runs.
fn folder_fingerprint(dir: &Path) -> String {
  let mut hasher = DefaultHasher::new();
  read_small(&dir.join("manifest.json")).hash(&mut hasher);
  read_small(&dir.join("main.js")).hash(&mut hasher);
  read_pages(dir).hash(&mut hasher);
  format!("{:x}", hasher.finish())
}

/// One fingerprint per `<root>/<folder>/`, cheap enough to poll every couple of seconds.
pub fn fingerprints_in(root: &Path) -> Vec<FolderFingerprint> {
  let Ok(entries) = fs::read_dir(root) else { return Vec::new() };
  let mut out: Vec<FolderFingerprint> = entries
    .flatten()
    .filter(|entry| entry.file_type().map(|t| t.is_dir()).unwrap_or(false))
    .map(|entry| FolderFingerprint {
      folder: entry.file_name().to_string_lossy().into_owned(),
      fingerprint: folder_fingerprint(&entry.path()),
    })
    .collect();
  out.sort_by(|a, b| a.folder.cmp(&b.folder));
  out
}

#[tauri::command]
pub fn plugins_dev_fingerprints<R: Runtime>(app: AppHandle<R>) -> Result<Vec<FolderFingerprint>, String> {
  Ok(fingerprints_in(&root(&app)?))
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

    fs::create_dir_all(root.join("b.good/ui")).unwrap();
    fs::write(root.join("b.good/ui/board.html"), "<h1>Board</h1>").unwrap();
    fs::write(root.join("b.good/ui/notes.txt"), "not a page").unwrap();
    assert_eq!(
      list_in(&root),
      vec![FolderPlugin {
        folder: "b.good".into(),
        manifest: "{}".into(),
        code: "1".into(),
        pages: vec![("ui/board.html".into(), "<h1>Board</h1>".into())],
      }]
    );
    assert!(list_in(&root.join("missing")).is_empty());
    let _ = fs::remove_dir_all(&root);
  }

  #[test]
  fn fingerprint_changes_with_content_and_reverts_when_content_reverts() {
    let root = std::env::temp_dir().join(format!("tmgr-fingerprint-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("p/ui")).unwrap();
    fs::write(root.join("p/manifest.json"), "{}").unwrap();
    fs::write(root.join("p/main.js"), "1").unwrap();
    fs::write(root.join("p/ui/page.html"), "<h1>1</h1>").unwrap();

    let before = fingerprints_in(&root);
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].folder, "p");

    fs::write(root.join("p/main.js"), "2").unwrap();
    let after_main = fingerprints_in(&root);
    assert_ne!(before[0].fingerprint, after_main[0].fingerprint);

    fs::write(root.join("p/main.js"), "1").unwrap();
    fs::write(root.join("p/ui/page.html"), "<h1>2</h1>").unwrap();
    let after_ui = fingerprints_in(&root);
    assert_ne!(before[0].fingerprint, after_ui[0].fingerprint);
    assert_ne!(after_main[0].fingerprint, after_ui[0].fingerprint);

    fs::write(root.join("p/ui/page.html"), "<h1>1</h1>").unwrap();
    let reverted = fingerprints_in(&root);
    assert_eq!(before, reverted);

    assert!(fingerprints_in(&root.join("missing")).is_empty());
    let _ = fs::remove_dir_all(&root);
  }
}
