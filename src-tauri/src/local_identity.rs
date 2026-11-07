use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

const FILE: &str = "cloud-identity.json";
const NAME_MAX: usize = 80;
const ID_MAX: i64 = (1 << 53) - 1;

#[derive(Serialize)]
struct Identity<'a> {
  id: i64,
  name: &'a str,
}

fn path<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
  let home = app.path().home_dir().map_err(|e| e.to_string())?;
  Ok(home.join(".tmgr.dev").join(FILE))
}

fn clean_name(name: &str) -> String {
  let stripped: String = name.chars().filter(|c| !c.is_control()).collect();
  stripped.trim().chars().take(NAME_MAX).collect::<String>().trim_end().to_string()
}

fn create_dir(dir: &Path) -> Result<(), String> {
  if dir.exists() {
    return Ok(());
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().recursive(true).mode(0o700).create(dir).map_err(|e| e.to_string())
  }
  #[cfg(not(unix))]
  {
    fs::create_dir_all(dir).map_err(|e| e.to_string())
  }
}

fn create_temp(dir: &Path) -> Result<(PathBuf, fs::File), String> {
  for _ in 0..8 {
    let mut b = [0u8; 8];
    getrandom::fill(&mut b).map_err(|e| e.to_string())?;
    let suffix: String = b.iter().map(|x| format!("{x:02x}")).collect();
    let tmp = dir.join(format!("cloud-identity.{}.{suffix}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
      use std::os::unix::fs::OpenOptionsExt;
      options.mode(0o600);
    }
    match options.open(&tmp) {
      Ok(file) => return Ok((tmp, file)),
      Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
      Err(e) => return Err(e.to_string()),
    }
  }
  Err("could not create a temp file".into())
}

pub fn set_at(path: &Path, id: i64, name: &str) -> Result<(), String> {
  if id <= 0 || id > ID_MAX {
    return Err("invalid user id".into());
  }
  let dir = path.parent().ok_or("no parent dir")?;
  create_dir(dir)?;
  let json = serde_json::to_string(&Identity { id, name: &clean_name(name) }).map_err(|e| e.to_string())?;
  let (tmp, mut file) = create_temp(dir)?;
  let written = file
    .write_all(json.as_bytes())
    .and_then(|_| file.sync_all())
    .map_err(|e| e.to_string())
    .and_then(|_| {
      drop(file);
      fs::rename(&tmp, path).map_err(|e| e.to_string())
    });
  if written.is_err() {
    let _ = fs::remove_file(&tmp);
  }
  written
}

pub fn clear_at(path: &Path) -> Result<(), String> {
  match fs::remove_file(path) {
    Ok(()) => Ok(()),
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
    Err(e) => Err(e.to_string()),
  }
}

pub fn read_at(path: &Path) -> Option<i64> {
  let value: serde_json::Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
  let id = value.get("id")?.as_i64()?;
  (id > 0 && id <= ID_MAX).then_some(id)
}

pub fn current_id<R: Runtime>(app: &AppHandle<R>) -> Option<i64> {
  read_at(&path(app).ok()?)
}

#[tauri::command]
pub fn local_identity_set<R: Runtime>(app: AppHandle<R>, id: i64, name: String) -> Result<(), String> {
  set_at(&path(&app)?, id, &name)
}

#[tauri::command]
pub fn local_identity_clear<R: Runtime>(app: AppHandle<R>) -> Result<(), String> {
  clear_at(&path(&app)?)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn target(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join(".tmgr.dev").join(FILE)
  }

  fn entries(path: &Path) -> Vec<String> {
    fs::read_dir(path.parent().unwrap())
      .unwrap()
      .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
      .collect()
  }

  #[test]
  fn writes_exactly_id_and_name() {
    let dir = tempfile::tempdir().unwrap();
    let file = target(&dir);
    set_at(&file, 42, "  Ann\u{7}\n Lee ").unwrap();
    let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(value, serde_json::json!({"id": 42, "name": "Ann Lee"}));
    assert_eq!(entries(&file), vec![FILE.to_string()]);
  }

  #[test]
  fn name_is_capped() {
    let dir = tempfile::tempdir().unwrap();
    let file = target(&dir);
    set_at(&file, 1, &"я".repeat(200)).unwrap();
    let value: serde_json::Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
    assert_eq!(value["name"].as_str().unwrap().chars().count(), 80);
  }

  #[cfg(unix)]
  #[test]
  fn file_is_private_and_new_dir_is_0700() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let file = target(&dir);
    set_at(&file, 1, "A").unwrap();
    assert_eq!(fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(fs::metadata(file.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
  }

  #[cfg(unix)]
  #[test]
  fn replaces_a_symlink_without_writing_through_it() {
    let dir = tempfile::tempdir().unwrap();
    let file = target(&dir);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    let victim = dir.path().join("victim.txt");
    fs::write(&victim, "keep").unwrap();
    std::os::unix::fs::symlink(&victim, &file).unwrap();
    set_at(&file, 7, "A").unwrap();
    assert_eq!(fs::read_to_string(&victim).unwrap(), "keep");
    assert!(!fs::symlink_metadata(&file).unwrap().file_type().is_symlink());
    assert!(fs::read_to_string(&file).unwrap().contains("\"id\":7"));
  }

  #[test]
  fn rejects_bad_ids_and_leaves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let file = target(&dir);
    for id in [0, -1, i64::MIN, ID_MAX + 1, i64::MAX] {
      assert!(set_at(&file, id, "A").is_err(), "{id}");
    }
    assert!(!file.exists());
    set_at(&file, ID_MAX, "A").unwrap();
  }

  #[test]
  fn read_returns_the_written_id() {
    let dir = tempfile::tempdir().unwrap();
    let file = target(&dir);
    assert_eq!(read_at(&file), None);
    set_at(&file, 42, "A").unwrap();
    assert_eq!(read_at(&file), Some(42));
    clear_at(&file).unwrap();
    assert_eq!(read_at(&file), None);
  }

  #[test]
  fn read_ignores_unparseable_files_and_out_of_range_ids() {
    let dir = tempfile::tempdir().unwrap();
    let file = target(&dir);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    for body in ["", "not json", "{}", r#"{"id":"5"}"#, r#"{"id":0}"#, r#"{"id":-1}"#, r#"{"id":1.5}"#, r#"{"id":9007199254740992}"#] {
      fs::write(&file, body).unwrap();
      assert_eq!(read_at(&file), None, "{body}");
    }
    fs::write(&file, r#"{"id":9007199254740991}"#).unwrap();
    assert_eq!(read_at(&file), Some(ID_MAX));
  }

  #[test]
  fn clear_removes_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let file = target(&dir);
    clear_at(&file).unwrap();
    set_at(&file, 1, "A").unwrap();
    clear_at(&file).unwrap();
    assert!(!file.exists());
    clear_at(&file).unwrap();
  }

  #[test]
  fn second_set_overwrites_without_temp_files() {
    let dir = tempfile::tempdir().unwrap();
    let file = target(&dir);
    set_at(&file, 1, "A").unwrap();
    set_at(&file, 2, "B").unwrap();
    assert!(fs::read_to_string(&file).unwrap().contains("\"id\":2"));
    assert_eq!(entries(&file), vec![FILE.to_string()]);
  }
}
