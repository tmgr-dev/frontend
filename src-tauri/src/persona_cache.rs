use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, Runtime};

/// Prompt/skills cache, keyed by owner account then persona uuid, outside the workspace folder:
/// the prompt is the owner's data, not workspace data, and must not leak to the next account signed
/// into the same machine.
fn cache_root<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
  Ok(app.path().app_data_dir().map_err(|e| e.to_string())?.join("persona-cache"))
}

fn cache_dir<R: Runtime>(app: &AppHandle<R>, user_id: i64) -> Result<PathBuf, String> {
  let dir = cache_root(app)?.join(user_id.to_string());
  fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
  Ok(dir)
}

/// Rejects anything but a plain uuid, so it can only ever resolve inside `cache_dir`.
fn safe_uuid_filename(uuid: &str) -> Result<String, String> {
  if uuid.is_empty() || uuid.len() > 64 || !uuid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
    return Err("invalid persona uuid".into());
  }
  Ok(format!("{uuid}.json"))
}

fn cache_path<R: Runtime>(app: &AppHandle<R>, user_id: i64, uuid: &str) -> Result<PathBuf, String> {
  Ok(cache_dir(app, user_id)?.join(safe_uuid_filename(uuid)?))
}

#[tauri::command]
pub fn persona_cache_put<R: Runtime>(app: AppHandle<R>, user_id: i64, uuid: String, data: Value) -> Result<(), String> {
  let path = cache_path(&app, user_id, &uuid)?;
  fs::write(path, serde_json::to_vec(&data).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn persona_cache_get<R: Runtime>(app: AppHandle<R>, user_id: i64, uuid: String) -> Result<Option<Value>, String> {
  let path = cache_path(&app, user_id, &uuid)?;
  if !path.exists() {
    return Ok(None);
  }
  let bytes = fs::read(&path).map_err(|e| e.to_string())?;
  serde_json::from_slice(&bytes).map(Some).map_err(|e| e.to_string())
}

/// Best-effort logout cleanup: this user's cache dir, plus any pre-namespacing flat `<uuid>.json` files.
#[tauri::command]
pub fn persona_cache_clear_for_user<R: Runtime>(app: AppHandle<R>, user_id: i64) -> Result<(), String> {
  let dir = cache_dir(&app, user_id)?;
  if dir.exists() {
    fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
  }
  if let Ok(entries) = fs::read_dir(cache_root(&app)?) {
    for entry in entries.flatten() {
      if entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
        let _ = fs::remove_file(entry.path());
      }
    }
  }
  Ok(())
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct CacheMigrateOut {
  pub moved: u32,
  pub conflicts: u32,
}

pub fn migrate_dir(src: &Path, dst: &Path) -> Result<CacheMigrateOut, String> {
  let mut out = CacheMigrateOut::default();
  if !src.is_dir() {
    return Ok(out);
  }
  fs::create_dir_all(dst).map_err(|e| e.to_string())?;
  for entry in fs::read_dir(src).map_err(|e| e.to_string())? {
    let entry = entry.map_err(|e| e.to_string())?;
    if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
      continue;
    }
    let target = dst.join(entry.file_name());
    if target.exists() {
      out.conflicts += 1;
    } else {
      fs::rename(entry.path(), &target).map_err(|e| e.to_string())?;
      out.moved += 1;
    }
  }
  Ok(out)
}

#[tauri::command]
pub fn persona_cache_migrate_local<R: Runtime>(
  app: AppHandle<R>,
  window: tauri::Window<R>,
  to_user_id: i64,
) -> Result<CacheMigrateOut, String> {
  if window.label() != "main" {
    return Err("persona_cache_migrate_local may only be called from the main window".into());
  }
  let to = crate::local_access::tokens::validate_user_id(to_user_id)?;
  let root = cache_root(&app)?;
  migrate_dir(&root.join("-1"), &root.join(to.to_string()))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn safe_uuid_filename_accepts_a_plain_uuid() {
    assert_eq!(safe_uuid_filename("p-1").unwrap(), "p-1.json");
  }

  #[test]
  fn safe_uuid_filename_rejects_path_traversal_and_separators() {
    assert!(safe_uuid_filename("../../etc/passwd").is_err());
    assert!(safe_uuid_filename("a/b").is_err());
    assert!(safe_uuid_filename("a\\b").is_err());
  }

  #[test]
  fn safe_uuid_filename_rejects_empty_or_overlong_input() {
    assert!(safe_uuid_filename("").is_err());
    assert!(safe_uuid_filename(&"a".repeat(65)).is_err());
    assert!(safe_uuid_filename(&"a".repeat(64)).is_ok());
  }

  fn put(dir: &Path, name: &str, body: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(dir.join(name), body).unwrap();
  }

  #[test]
  fn migrate_dir_moves_files_and_counts_conflicts() {
    let tmp = tempfile::tempdir().unwrap();
    let (src, dst) = (tmp.path().join("-1"), tmp.path().join("7"));
    put(&src, "a.json", "A");
    put(&src, "b.json", "B");
    put(&dst, "b.json", "OLD");
    let out = migrate_dir(&src, &dst).unwrap();
    assert_eq!(out, CacheMigrateOut { moved: 1, conflicts: 1 });
    assert_eq!(fs::read_to_string(dst.join("a.json")).unwrap(), "A");
    assert_eq!(fs::read_to_string(dst.join("b.json")).unwrap(), "OLD");
    assert_eq!(fs::read_to_string(src.join("b.json")).unwrap(), "B");
    assert!(!src.join("a.json").exists());
  }

  #[test]
  fn migrate_dir_missing_source_is_zero_and_creates_nothing() {
    let tmp = tempfile::tempdir().unwrap();
    let (src, dst) = (tmp.path().join("-1"), tmp.path().join("7"));
    assert_eq!(migrate_dir(&src, &dst).unwrap(), CacheMigrateOut::default());
    assert!(!src.exists() && !dst.exists());
  }

  #[test]
  fn migrate_dir_rerun_after_partial_move_finishes_the_rest() {
    let tmp = tempfile::tempdir().unwrap();
    let (src, dst) = (tmp.path().join("-1"), tmp.path().join("7"));
    put(&src, "a.json", "A");
    put(&src, "b.json", "B");
    put(&dst, "a.json", "A");
    fs::remove_file(src.join("a.json")).unwrap();
    assert_eq!(migrate_dir(&src, &dst).unwrap(), CacheMigrateOut { moved: 1, conflicts: 0 });
    assert_eq!(migrate_dir(&src, &dst).unwrap(), CacheMigrateOut::default());
    assert_eq!(fs::read_to_string(dst.join("b.json")).unwrap(), "B");
  }
}
