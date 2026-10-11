use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

const FILE: &str = "profile.json";
const NAME_MAX: usize = 80;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct LocalProfile {
  pub id: i64,
  pub uuid: String,
  pub name: String,
  pub created_at: String,
}

pub fn normalize_name(name: &str) -> String {
  name.trim().chars().take(NAME_MAX).collect::<String>().trim_end().to_string()
}

fn path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
  let home = app.path().home_dir().map_err(|e| e.to_string())?;
  Ok(home.join(".tmgr.dev").join(FILE))
}

fn new_uuid() -> String {
  let mut b = [0u8; 16];
  let _ = getrandom::fill(&mut b);
  b[6] = (b[6] & 0x0f) | 0x40;
  b[8] = (b[8] & 0x3f) | 0x80;
  let h: Vec<String> = b.iter().map(|x| format!("{x:02x}")).collect();
  format!("{}-{}-{}-{}-{}", h[0..4].concat(), h[4..6].concat(), h[6..8].concat(), h[8..10].concat(), h[10..16].concat())
}

fn write(path: &Path, profile: &LocalProfile) -> Result<(), String> {
  if let Some(parent) = path.parent() {
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
  }
  let json = serde_json::to_string_pretty(profile).map_err(|e| e.to_string())?;
  let tmp = path.with_extension("json.tmp");
  fs::write(&tmp, json).map_err(|e| e.to_string())?;
  fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn get_at(path: &Path) -> Option<LocalProfile> {
  serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

pub fn create_at(path: &Path, name: Option<&str>, now: &str, uuid: String) -> Result<LocalProfile, String> {
  if let Some(existing) = get_at(path) {
    return Ok(existing);
  }
  let profile = LocalProfile {
    id: -1,
    uuid,
    name: normalize_name(name.unwrap_or("")),
    created_at: now.to_string(),
  };
  write(path, &profile)?;
  Ok(profile)
}

pub fn update_at(path: &Path, name: &str) -> Result<LocalProfile, String> {
  let mut profile = get_at(path).ok_or("no local profile")?;
  profile.name = normalize_name(name);
  write(path, &profile)?;
  Ok(profile)
}

#[tauri::command]
pub fn local_profile_get<R: Runtime>(app: AppHandle<R>) -> Result<Option<LocalProfile>, String> {
  Ok(get_at(&path(&app)?))
}

#[tauri::command]
pub fn local_profile_create<R: Runtime>(app: AppHandle<R>, name: Option<String>) -> Result<LocalProfile, String> {
  let created = create_at(
    &path(&app)?,
    name.as_deref(),
    &crate::local_workspaces::chrono_like_now(),
    new_uuid(),
  )?;
  log::info!("[local] profile ready");
  Ok(created)
}

#[tauri::command]
pub fn local_profile_update<R: Runtime>(app: AppHandle<R>, name: String) -> Result<LocalProfile, String> {
  update_at(&path(&app)?, &name)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn temp_file(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tmgr-profile-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir.join(FILE)
  }

  #[test]
  fn names_are_trimmed_and_capped() {
    assert_eq!(normalize_name("  Ann  "), "Ann");
    assert_eq!(normalize_name("   "), "");
    assert_eq!(normalize_name(&"я".repeat(100)).chars().count(), 80);
  }

  #[test]
  fn uuid_is_v4_shaped() {
    let u = new_uuid();
    assert_eq!(u.len(), 36);
    assert_eq!(&u[14..15], "4");
    assert!(matches!(&u[19..20], "8" | "9" | "a" | "b"));
    assert_ne!(u, new_uuid());
  }

  #[test]
  fn create_is_idempotent_and_serialises_snake_case() {
    let file = temp_file("create");
    assert_eq!(get_at(&file), None);
    let first = create_at(&file, Some(" Ann "), "2026-10-11T00:00:00Z", "u-1".into()).unwrap();
    let second = create_at(&file, Some("Bob"), "2027-01-01T00:00:00Z", "u-2".into()).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.id, -1);
    assert_eq!(first.name, "Ann");
    let raw = fs::read_to_string(&file).unwrap();
    for key in ["\"id\"", "\"uuid\"", "\"name\"", "\"created_at\""] {
      assert!(raw.contains(key), "{key}");
    }
    assert!(!file.with_extension("json.tmp").exists());
    let _ = fs::remove_dir_all(file.parent().unwrap());
  }

  #[test]
  fn empty_name_is_allowed_and_update_keeps_identity() {
    let file = temp_file("update");
    assert!(update_at(&file, "x").is_err());
    let created = create_at(&file, None, "now", "u-1".into()).unwrap();
    assert_eq!(created.name, "");
    let updated = update_at(&file, "  Carol ").unwrap();
    assert_eq!(updated.name, "Carol");
    assert_eq!(updated.uuid, "u-1");
    assert_eq!(get_at(&file), Some(updated));
    let _ = fs::remove_dir_all(file.parent().unwrap());
  }
}
