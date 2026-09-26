use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use tauri::{AppHandle, Runtime};
use tauri_plugin_opener::OpenerExt;

use crate::local_workspaces;

#[derive(Deserialize)]
pub struct ExportFile {
  path: String,
  content: String,
}

/// A path inside the export folder: relative, only normal segments (task titles may be Unicode).
pub fn safe_relative(path: &str) -> Option<PathBuf> {
  let candidate = Path::new(path);
  if path.is_empty() || path.contains('\0') || path.contains('\\') {
    return None;
  }
  let mut clean = PathBuf::new();
  for component in candidate.components() {
    match component {
      Component::Normal(part) => clean.push(part),
      _ => return None,
    }
  }
  Some(clean)
}

pub fn write_export(dir: &Path, files: &[ExportFile]) -> Result<(), String> {
  for file in files {
    let relative = safe_relative(&file.path).ok_or_else(|| format!("bad export path {}", file.path))?;
    let target = dir.join(relative);
    if let Some(parent) = target.parent() {
      fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    fs::write(&target, &file.content).map_err(|e| e.to_string())?;
  }
  Ok(())
}

/// Writes Markdown into `<workspace>/exports/<folder>/` and returns that folder.
#[tauri::command]
pub fn local_export_write<R: Runtime>(
  app: AppHandle<R>,
  code: String,
  folder: String,
  files: Vec<ExportFile>,
) -> Result<String, String> {
  let workspace = local_workspaces::find(&app, &code)?;
  let folder = safe_relative(&folder).ok_or("bad export folder")?;
  let dir = Path::new(&workspace.path).join("exports").join(folder);
  write_export(&dir, &files)?;
  log::info!("[local] exported {} file(s) of {code}", files.len());
  Ok(dir.to_string_lossy().into_owned())
}

/// Shows a path of a local workspace (its folder, an export) in Finder.
#[tauri::command]
pub fn local_reveal<R: Runtime>(app: AppHandle<R>, code: String, relative: Option<String>) -> Result<(), String> {
  let workspace = local_workspaces::find(&app, &code)?;
  let mut path = PathBuf::from(&workspace.path);
  if let Some(relative) = relative.filter(|r| !r.is_empty()) {
    path = path.join(safe_relative(&relative).ok_or("bad path")?);
  }
  app.opener().reveal_item_in_dir(&path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn export_paths_cannot_leave_the_export_folder() {
    assert_eq!(safe_relative("tasks/TM-1-личное.md"), Some(PathBuf::from("tasks/TM-1-личное.md")));
    assert_eq!(safe_relative("../secrets.md"), None);
    assert_eq!(safe_relative("tasks/../../x.md"), None);
    assert_eq!(safe_relative("/etc/passwd"), None);
    assert_eq!(safe_relative(""), None);
  }

  #[test]
  fn writes_nested_files() {
    let dir = std::env::temp_dir().join(format!("tmgr-export-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    write_export(
      &dir,
      &[
        ExportFile { path: "README.md".into(), content: "# Index".into() },
        ExportFile { path: "tasks/TM-1.md".into(), content: "task".into() },
      ],
    )
    .unwrap();
    assert_eq!(fs::read_to_string(dir.join("tasks/TM-1.md")).unwrap(), "task");
    assert!(write_export(&dir, &[ExportFile { path: "../x.md".into(), content: String::new() }]).is_err());
    let _ = fs::remove_dir_all(&dir);
  }
}
