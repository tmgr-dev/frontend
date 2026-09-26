use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

const MANIFEST: &str = "workspace.json";
const DATABASE: &str = "workspace.db";

/// `workspace.json`: what identifies a local workspace folder. The SQLite file next to it is the
/// source of truth for everything else.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Manifest {
  /// Negative so it can never collide with a server workspace id in the UI.
  pub id: i64,
  pub name: String,
  pub code: String,
  pub schema_version: u32,
  pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LocalWorkspace {
  #[serde(flatten)]
  pub manifest: Manifest,
  pub path: String,
  pub database: String,
}

fn root<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
  let home = app.path().home_dir().map_err(|e| e.to_string())?;
  Ok(home.join(".tmgr.dev").join("workspaces"))
}

pub fn slugify(name: &str) -> String {
  let mut slug = String::new();
  for c in name.trim().to_lowercase().chars() {
    if c.is_ascii_alphanumeric() {
      slug.push(c);
    } else if !slug.ends_with('-') && !slug.is_empty() {
      slug.push('-');
    }
  }
  let slug = slug.trim_end_matches('-').to_string();
  if slug.is_empty() {
    "workspace".to_string()
  } else {
    slug.chars().take(48).collect()
  }
}

fn unique_dir(root: &Path, base: &str) -> PathBuf {
  let first = root.join(base);
  if !first.exists() {
    return first;
  }
  (2..)
    .map(|i| root.join(format!("{base}-{i}")))
    .find(|p| !p.exists())
    .unwrap()
}

/// A stable negative id derived from the folder name, so a copied or moved folder keeps working.
pub fn local_id(code: &str) -> i64 {
  let hash = code.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3));
  -((hash % 1_000_000_000) as i64 + 1)
}

fn read_workspace(dir: &Path) -> Option<LocalWorkspace> {
  let raw = fs::read_to_string(dir.join(MANIFEST)).ok()?;
  let manifest: Manifest = serde_json::from_str(&raw).ok()?;
  Some(LocalWorkspace {
    path: dir.to_string_lossy().into_owned(),
    database: dir.join(DATABASE).to_string_lossy().into_owned(),
    manifest,
  })
}

fn write_manifest(dir: &Path, manifest: &Manifest) -> Result<(), String> {
  let json = serde_json::to_string_pretty(manifest).map_err(|e| e.to_string())?;
  let tmp = dir.join(format!("{MANIFEST}.tmp"));
  fs::write(&tmp, json).map_err(|e| e.to_string())?;
  fs::rename(&tmp, dir.join(MANIFEST)).map_err(|e| e.to_string())
}

pub fn list_in(root: &Path) -> Vec<LocalWorkspace> {
  let Ok(entries) = fs::read_dir(root) else {
    return Vec::new();
  };
  let mut workspaces: Vec<LocalWorkspace> = entries
    .filter_map(Result::ok)
    .filter(|e| e.path().is_dir())
    .filter_map(|e| read_workspace(&e.path()))
    .collect();
  workspaces.sort_by(|a, b| a.manifest.name.to_lowercase().cmp(&b.manifest.name.to_lowercase()));
  workspaces
}

pub fn create_in(root: &Path, name: &str, now: &str) -> Result<LocalWorkspace, String> {
  let name = name.trim();
  if name.is_empty() {
    return Err("name is required".into());
  }
  fs::create_dir_all(root).map_err(|e| e.to_string())?;
  let dir = unique_dir(root, &slugify(name));
  fs::create_dir_all(dir.join("files")).map_err(|e| e.to_string())?;
  fs::create_dir_all(dir.join("backups")).map_err(|e| e.to_string())?;
  let code = dir.file_name().unwrap().to_string_lossy().into_owned();
  let manifest = Manifest {
    id: local_id(&code),
    name: name.to_string(),
    code,
    schema_version: 0,
    created_at: now.to_string(),
  };
  write_manifest(&dir, &manifest)?;
  read_workspace(&dir).ok_or_else(|| "workspace was not written".into())
}

pub fn find<R: Runtime>(app: &AppHandle<R>, code: &str) -> Result<LocalWorkspace, String> {
  list_in(&root(app)?)
    .into_iter()
    .find(|w| w.manifest.code == code)
    .ok_or_else(|| format!("local workspace {code} not found"))
}

#[tauri::command]
pub fn local_workspaces_list<R: Runtime>(app: AppHandle<R>) -> Result<Vec<LocalWorkspace>, String> {
  Ok(list_in(&root(&app)?))
}

#[tauri::command]
pub fn local_workspace_create<R: Runtime>(app: AppHandle<R>, name: String) -> Result<LocalWorkspace, String> {
  let now = chrono_like_now();
  let created = create_in(&root(&app)?, &name, &now)?;
  log::info!("[local] created workspace {}", created.manifest.code);
  Ok(created)
}

#[tauri::command]
pub fn local_workspace_set_schema<R: Runtime>(app: AppHandle<R>, code: String, version: u32) -> Result<(), String> {
  let workspace = find(&app, &code)?;
  let mut manifest = workspace.manifest;
  manifest.schema_version = version;
  write_manifest(Path::new(&workspace.path), &manifest)
}

fn chrono_like_now() -> String {
  let secs = crate::tray::now_secs();
  let days = secs.div_euclid(86_400);
  let rem = secs.rem_euclid(86_400);
  let (y, m, d) = civil_from_days(days);
  format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, (rem % 3600) / 60, rem % 60)
}

/// Howard Hinnant's days-to-civil conversion.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
  let z = days + 719_468;
  let era = z.div_euclid(146_097);
  let doe = z.rem_euclid(146_097);
  let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
  let y = yoe + era * 400;
  let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
  let mp = (5 * doy + 2) / 153;
  let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
  let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
  (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn temp_root(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tmgr-local-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
  }

  #[test]
  fn slugs_are_filesystem_safe() {
    assert_eq!(slugify("  Личное & Work 2026 "), "work-2026");
    assert_eq!(slugify("NDA: Client X"), "nda-client-x");
    assert_eq!(slugify("***"), "workspace");
  }

  #[test]
  fn ids_are_negative_and_stable() {
    assert!(local_id("personal") < 0);
    assert_eq!(local_id("personal"), local_id("personal"));
    assert_ne!(local_id("personal"), local_id("personal-2"));
  }

  #[test]
  fn create_then_list_finds_the_workspace_and_names_are_unique() {
    let root = temp_root("create");
    let a = create_in(&root, "Personal", "2026-09-26T00:00:00Z").unwrap();
    let b = create_in(&root, "Personal", "2026-09-26T00:00:00Z").unwrap();

    assert_eq!(a.manifest.code, "personal");
    assert_eq!(b.manifest.code, "personal-2");
    assert!(Path::new(&a.path).join("files").is_dir());
    assert!(a.database.ends_with("personal/workspace.db"));
    let listed = list_in(&root);
    assert_eq!(listed.len(), 2);
    assert!(listed.iter().all(|w| w.manifest.id < 0));
    let _ = fs::remove_dir_all(&root);
  }

  #[test]
  fn folders_without_a_valid_manifest_are_ignored() {
    let root = temp_root("ignore");
    fs::create_dir_all(root.join("junk")).unwrap();
    fs::write(root.join("junk").join(MANIFEST), "not json").unwrap();
    assert!(list_in(&root).is_empty());
    assert!(create_in(&root, "  ", "now").is_err());
    let _ = fs::remove_dir_all(&root);
  }

  #[test]
  fn civil_dates_are_correct() {
    assert_eq!(civil_from_days(0), (1970, 1, 1));
    assert_eq!(civil_from_days(20_722), (2026, 9, 26));
  }
}
