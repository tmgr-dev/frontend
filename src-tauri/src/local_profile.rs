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
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub linked_user_id: Option<i64>,
}

const LINK_FILE: &str = "profile-link.json";

#[derive(Deserialize, Serialize)]
struct Link {
  linked_user_id: i64,
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

fn write<T: Serialize>(path: &Path, profile: &T) -> Result<(), String> {
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
    linked_user_id: None,
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

fn read_link(path: &Path) -> Result<Option<i64>, String> {
  match fs::read_to_string(path) {
    Ok(raw) => serde_json::from_str::<Link>(&raw)
      .map(|l| Some(l.linked_user_id))
      .map_err(|_| "unreadable profile link".to_string()),
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
    Err(_) => Err("unreadable profile link".into()),
  }
}

pub fn link_at(path: &Path, user_id: i64) -> Result<LocalProfile, String> {
  let mut profile = get_at(path).ok_or("no local profile")?;
  let link_path = path.with_file_name(LINK_FILE);
  let sidecar = read_link(&link_path)?;
  let id = sidecar.or(profile.linked_user_id).unwrap_or(user_id);
  if sidecar != Some(id) {
    write(&link_path, &Link { linked_user_id: id })?;
  }
  if profile.linked_user_id != Some(id) {
    profile.linked_user_id = Some(id);
    write(path, &profile)?;
  }
  Ok(profile)
}

pub fn linked_id_at(path: &Path) -> Result<Option<i64>, String> {
  let sidecar = read_link(&path.with_file_name(LINK_FILE))?;
  Ok(sidecar.or_else(|| get_at(path).and_then(|p| p.linked_user_id)))
}

pub fn require_claim_at(path: &Path, to_user_id: i64) -> Result<(), String> {
  match linked_id_at(path)? {
    Some(id) if id == to_user_id => Ok(()),
    _ => Err("local profile is not linked to this account".into()),
  }
}

pub fn require_claim<R: Runtime>(app: &AppHandle<R>, to_user_id: i64) -> Result<(), String> {
  require_claim_at(&path(app)?, to_user_id)
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

#[tauri::command]
pub fn local_profile_link<R: Runtime>(app: AppHandle<R>, window: tauri::Window<R>, user_id: i64) -> Result<LocalProfile, String> {
  if window.label() != "main" {
    return Err("local_profile_link may only be called from the main window".into());
  }
  link_at(&path(&app)?, crate::local_access::tokens::validate_user_id(user_id)?)
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

  fn created(name: &str) -> PathBuf {
    let file = temp_file(name);
    create_at(&file, Some("Ann"), "now", "u-1".into()).unwrap();
    file
  }

  fn link_file(file: &Path) -> PathBuf {
    file.with_file_name(LINK_FILE)
  }

  fn strip_field(file: &Path) {
    let mut p = get_at(file).unwrap();
    p.linked_user_id = None;
    write(file, &p).unwrap();
  }

  #[test]
  fn link_sets_once_first_wins_and_writes_sidecar() {
    let file = created("link");
    let first = link_at(&file, 5).unwrap();
    assert_eq!(first.linked_user_id, Some(5));
    assert_eq!(link_at(&file, 9).unwrap().linked_user_id, Some(5));
    assert_eq!(get_at(&file).unwrap().linked_user_id, Some(5));
    assert!(fs::read_to_string(link_file(&file)).unwrap().contains("\"linked_user_id\": 5"));
    assert!(link_at(&temp_file("nolink"), 5).is_err());
    let _ = fs::remove_dir_all(file.parent().unwrap());
  }

  #[test]
  fn rename_keeps_link() {
    let file = created("rename-link");
    link_at(&file, 5).unwrap();
    let renamed = update_at(&file, "Bob").unwrap();
    assert_eq!(renamed.linked_user_id, Some(5));
    assert_eq!(get_at(&file).unwrap().linked_user_id, Some(5));
    let _ = fs::remove_dir_all(file.parent().unwrap());
  }

  #[test]
  fn old_file_without_the_field_parses_and_omits_it_when_unlinked() {
    let file = temp_file("old");
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, r#"{"id":-1,"uuid":"u","name":"A","created_at":"t"}"#).unwrap();
    assert_eq!(get_at(&file).unwrap().linked_user_id, None);
    update_at(&file, "B").unwrap();
    assert!(!fs::read_to_string(&file).unwrap().contains("linked_user_id"));
    let _ = fs::remove_dir_all(file.parent().unwrap());
  }

  #[test]
  fn stripped_field_is_restored_from_sidecar_and_other_caller_gets_first_id() {
    let file = created("strip");
    link_at(&file, 5).unwrap();
    strip_field(&file);
    assert_eq!(get_at(&file).unwrap().linked_user_id, None);
    assert_eq!(link_at(&file, 9).unwrap().linked_user_id, Some(5));
    assert_eq!(get_at(&file).unwrap().linked_user_id, Some(5));
    let _ = fs::remove_dir_all(file.parent().unwrap());
  }

  #[test]
  fn missing_sidecar_is_repaired_from_the_field() {
    let file = created("nosidecar");
    link_at(&file, 5).unwrap();
    fs::remove_file(link_file(&file)).unwrap();
    assert_eq!(link_at(&file, 9).unwrap().linked_user_id, Some(5));
    assert!(fs::read_to_string(link_file(&file)).unwrap().contains(": 5"));
    let _ = fs::remove_dir_all(file.parent().unwrap());
  }

  #[test]
  fn sidecar_wins_when_it_disagrees_with_the_field() {
    let file = created("disagree");
    link_at(&file, 5).unwrap();
    let mut p = get_at(&file).unwrap();
    p.linked_user_id = Some(8);
    write(&file, &p).unwrap();
    assert_eq!(link_at(&file, 9).unwrap().linked_user_id, Some(5));
    assert_eq!(get_at(&file).unwrap().linked_user_id, Some(5));
    let _ = fs::remove_dir_all(file.parent().unwrap());
  }

  #[test]
  fn corrupt_sidecar_errors_and_changes_nothing() {
    let file = created("corrupt");
    fs::write(link_file(&file), "{nope").unwrap();
    let before = fs::read_to_string(&file).unwrap();
    assert!(link_at(&file, 5).is_err());
    assert_eq!(fs::read_to_string(&file).unwrap(), before);
    assert_eq!(fs::read_to_string(link_file(&file)).unwrap(), "{nope");
    let _ = fs::remove_dir_all(file.parent().unwrap());
  }

  #[test]
  fn sidecar_write_failure_writes_no_field_and_rerun_links() {
    let file = created("sidecar-fail");
    let tmp = link_file(&file).with_extension("json.tmp");
    fs::create_dir_all(&tmp).unwrap();
    assert!(link_at(&file, 5).is_err());
    assert_eq!(get_at(&file).unwrap().linked_user_id, None);
    assert!(!link_file(&file).exists());
    fs::remove_dir_all(&tmp).unwrap();
    assert_eq!(link_at(&file, 5).unwrap().linked_user_id, Some(5));
    let _ = fs::remove_dir_all(file.parent().unwrap());
  }

  #[test]
  fn claim_requires_a_matching_link() {
    let file = created("claim");
    assert!(require_claim_at(&file, 5).is_err());
    link_at(&file, 5).unwrap();
    assert!(require_claim_at(&file, 5).is_ok());
    assert!(require_claim_at(&file, 9).is_err());
    strip_field(&file);
    assert_eq!(linked_id_at(&file).unwrap(), Some(5));
    assert!(require_claim_at(&file, 5).is_ok());
    assert!(require_claim_at(&file, 9).is_err());
    fs::write(link_file(&file), "{nope").unwrap();
    assert!(require_claim_at(&file, 5).is_err());
    let _ = fs::remove_dir_all(file.parent().unwrap());
  }

  #[test]
  fn claim_refuses_when_nothing_is_linked() {
    let file = created("claim-missing");
    assert!(require_claim_at(&file, 5).is_err());
    let _ = fs::remove_dir_all(file.parent().unwrap());
  }

  #[test]
  fn claim_refuses_a_target_other_than_the_linked_account() {
    let file = created("claim-other");
    link_at(&file, 5).unwrap();
    assert!(require_claim_at(&file, 9).is_err());
    let _ = fs::remove_dir_all(file.parent().unwrap());
  }

  #[test]
  fn claim_from_the_field_alone_when_sidecar_is_missing() {
    let file = created("claim-field");
    link_at(&file, 5).unwrap();
    fs::remove_file(link_file(&file)).unwrap();
    assert!(require_claim_at(&file, 5).is_ok());
    assert!(require_claim_at(&file, 9).is_err());
    let _ = fs::remove_dir_all(file.parent().unwrap());
  }

  #[test]
  fn both_gone_acts_as_a_first_link() {
    let file = created("bothgone");
    link_at(&file, 5).unwrap();
    strip_field(&file);
    fs::remove_file(link_file(&file)).unwrap();
    assert_eq!(link_at(&file, 9).unwrap().linked_user_id, Some(9));
    let _ = fs::remove_dir_all(file.parent().unwrap());
  }
}
