use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::flag::{read_object, update_object};

const KEY: &str = "folders";
const MAX_PATH_BYTES: usize = 1024;
pub const HIDDEN_CHARS: &str = "Choose a folder whose path has no hidden or control characters (max 1024 characters)";
pub const CHANGED: &str = "The project folder changed; run again";

fn displayable(path: &Path) -> Result<&str, String> {
  let text = path.to_str().ok_or_else(|| HIDDEN_CHARS.to_owned())?;
  let odd_component = path.components().any(|c| {
    let part = c.as_os_str().to_str().unwrap_or_default();
    part.trim() != part
  });
  if text.len() > MAX_PATH_BYTES || odd_component || text.chars().any(super::sanitize::unsafe_char) {
    return Err(HIDDEN_CHARS.to_owned());
  }
  Ok(text)
}

pub fn validate(path: &Path, home: Option<&Path>) -> Result<PathBuf, String> {
  if !path.is_absolute() {
    return Err("The project folder must be an absolute path".to_owned());
  }
  let canon = path.canonicalize().map_err(|_| "The project folder does not exist".to_owned())?;
  if !canon.is_dir() {
    return Err("The project folder is not a directory".to_owned());
  }
  displayable(&canon)?;
  if canon.parent().is_none() {
    return Err("Choose a project folder, not the filesystem root".to_owned());
  }
  if home.and_then(|h| h.canonicalize().ok()).is_some_and(|h| h == canon) {
    return Err("Choose a project folder, not your home directory".to_owned());
  }
  Ok(canon)
}

pub struct Snapshot {
  path: PathBuf,
  dev: u64,
  ino: u64,
}

#[cfg(unix)]
fn identity(meta: &std::fs::Metadata) -> Result<(u64, u64), String> {
  use std::os::unix::fs::MetadataExt;
  Ok((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn identity(_meta: &std::fs::Metadata) -> Result<(u64, u64), String> {
  Err("The runner is not available on this platform".to_owned())
}

impl Snapshot {
  pub fn capture(path: &Path, home: Option<&Path>) -> Result<Snapshot, String> {
    let canon = validate(path, home)?;
    let meta = std::fs::symlink_metadata(&canon).map_err(|_| CHANGED.to_owned())?;
    if !meta.is_dir() {
      return Err(CHANGED.to_owned());
    }
    let (dev, ino) = identity(&meta)?;
    Ok(Snapshot { path: canon, dev, ino })
  }

  pub fn path(&self) -> &str {
    self.path.to_str().unwrap_or_default()
  }

  pub fn pin(&self) -> (u64, u64) {
    (self.dev, self.ino)
  }

  pub fn verify(&self, home: Option<&Path>) -> Result<(), String> {
    let now = Snapshot::capture(&self.path, home).map_err(|_| CHANGED.to_owned())?;
    if now.path == self.path && now.dev == self.dev && now.ino == self.ino {
      Ok(())
    } else {
      Err(CHANGED.to_owned())
    }
  }
}

pub fn get(config: &Path, code: &str, home: Option<&Path>) -> Option<String> {
  let stored = read_object(config).get(KEY)?.get(code)?.as_str()?.to_owned();
  validate(Path::new(&stored), home).ok().map(|p| p.to_string_lossy().into_owned())
}

pub fn set(config: &Path, code: &str, path: &Path, home: Option<&Path>) -> Result<String, String> {
  let canon = validate(path, home)?;
  let text = displayable(&canon)?.to_owned();
  update_object(config, |o| {
    let folders = o.entry(KEY).or_insert_with(|| Value::Object(Map::new()));
    if !folders.is_object() {
      *folders = Value::Object(Map::new());
    }
    if let Value::Object(m) = folders {
      m.insert(code.to_owned(), Value::String(text.clone()));
    }
  })
  .map_err(|e| e.to_string())?;
  Ok(text)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn dirs() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().canonicalize().unwrap();
    let home = root.join("home");
    let project = home.join("project");
    std::fs::create_dir_all(&project).unwrap();
    (t, home, project)
  }

  #[test]
  fn rejects_root_home_relative_missing_and_files() {
    let (t, home, project) = dirs();
    assert!(validate(&project, Some(&home)).is_ok());
    assert!(validate(Path::new("/"), Some(&home)).is_err());
    assert!(validate(&home, Some(&home)).is_err());
    assert!(validate(&home.join("project/.."), Some(&home)).is_err());
    assert!(validate(Path::new("relative/dir"), Some(&home)).is_err());
    assert!(validate(&home.join("missing"), Some(&home)).is_err());
    let file = home.join("file");
    std::fs::write(&file, b"x").unwrap();
    assert!(validate(&file, Some(&home)).is_err());
    drop(t);
  }

  #[test]
  fn rejects_paths_that_cannot_be_shown_losslessly() {
    let t = tempfile::tempdir().unwrap();
    let base = t.path().canonicalize().unwrap();
    for name in ["a\x1bb", "a\u{202e}b", "a\u{200b}b", "a\nb", "a\u{feff}b", " lead", "trail ", "a\tb"] {
      let dir = base.join(name);
      std::fs::create_dir(&dir).unwrap();
      assert_eq!(validate(&dir, None).unwrap_err(), HIDDEN_CHARS, "{name:?}");
    }
    let outer = base.join("outer ");
    std::fs::create_dir_all(outer.join("inner")).unwrap();
    assert_eq!(validate(&outer.join("inner"), None).unwrap_err(), HIDDEN_CHARS);
    assert_eq!(displayable(Path::new(&format!("/{}", "x".repeat(1024)))).unwrap_err(), HIDDEN_CHARS);
    assert!(displayable(Path::new(&format!("/{}", "x".repeat(1023)))).is_ok());
    let edge = base.join("é-ok");
    std::fs::create_dir(&edge).unwrap();
    assert!(validate(&edge, None).is_ok());
  }

  #[test]
  fn rejects_invalid_utf8_paths() {
    use std::os::unix::ffi::OsStrExt;
    let bad = Path::new(std::ffi::OsStr::from_bytes(b"/tmp/bad\xff"));
    assert_eq!(displayable(bad).unwrap_err(), HIDDEN_CHARS);
  }

  #[test]
  fn an_unchanged_folder_verifies() {
    let (_t, home, project) = dirs();
    let snap = Snapshot::capture(&project, Some(&home)).unwrap();
    assert_eq!(snap.path(), project.to_str().unwrap());
    assert!(snap.verify(Some(&home)).is_ok());
  }

  #[test]
  fn a_folder_swapped_for_a_symlink_or_another_directory_fails_verification() {
    let (_t, home, project) = dirs();
    let other = home.join("other");
    std::fs::create_dir(&other).unwrap();
    let snap = Snapshot::capture(&project, Some(&home)).unwrap();
    std::fs::rename(&project, home.join("moved")).unwrap();
    std::os::unix::fs::symlink(&other, &project).unwrap();
    assert_eq!(snap.verify(Some(&home)).unwrap_err(), CHANGED);
    std::fs::remove_file(&project).unwrap();
    std::fs::create_dir(&project).unwrap();
    assert_eq!(snap.verify(Some(&home)).unwrap_err(), CHANGED);
    std::fs::remove_dir(&project).unwrap();
    assert_eq!(snap.verify(Some(&home)).unwrap_err(), CHANGED);
  }

  #[test]
  fn stores_the_canonical_path_next_to_the_flag_and_keeps_it_per_workspace() {
    let (t, home, project) = dirs();
    let config = t.path().join("cfg/runner.json");
    #[cfg(unix)]
    {
      let link = home.join("link");
      std::os::unix::fs::symlink(&project, &link).unwrap();
      let stored = set(&config, "personal", &link, Some(&home)).unwrap();
      assert_eq!(stored, project.to_str().unwrap());
    }
    super::super::flag::Flag::load(config.clone()).set(true).unwrap();
    assert_eq!(get(&config, "personal", Some(&home)).as_deref(), project.to_str());
    assert_eq!(get(&config, "work", Some(&home)), None);
    let other = home.join("other");
    std::fs::create_dir(&other).unwrap();
    set(&config, "work", &other, Some(&home)).unwrap();
    assert_eq!(get(&config, "personal", Some(&home)).as_deref(), project.to_str());
    assert_eq!(get(&config, "work", Some(&home)).as_deref(), other.to_str());
    assert!(super::super::flag::Flag::load(config).get());
  }

  #[test]
  fn a_stored_folder_that_vanished_or_became_home_is_dropped() {
    let (t, home, project) = dirs();
    let config = t.path().join("runner.json");
    set(&config, "personal", &project, Some(&home)).unwrap();
    std::fs::remove_dir(&project).unwrap();
    assert_eq!(get(&config, "personal", Some(&home)), None);
    set(&config, "personal", &home, None).unwrap();
    assert_eq!(get(&config, "personal", Some(&home)), None);
  }

  #[test]
  fn invalid_choices_are_not_stored() {
    let (t, home, _p) = dirs();
    let config = t.path().join("runner.json");
    assert!(set(&config, "personal", Path::new("/"), Some(&home)).is_err());
    assert!(!config.exists());
  }
}
