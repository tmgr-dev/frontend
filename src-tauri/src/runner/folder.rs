use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::flag::{read_object, update_object};

const KEY: &str = "folders";

pub fn validate(path: &Path, home: Option<&Path>) -> Result<PathBuf, String> {
  if !path.is_absolute() {
    return Err("The project folder must be an absolute path".to_owned());
  }
  let canon = path.canonicalize().map_err(|_| "The project folder does not exist".to_owned())?;
  if !canon.is_dir() {
    return Err("The project folder is not a directory".to_owned());
  }
  if canon.parent().is_none() {
    return Err("Choose a project folder, not the filesystem root".to_owned());
  }
  if home.and_then(|h| h.canonicalize().ok()).is_some_and(|h| h == canon) {
    return Err("Choose a project folder, not your home directory".to_owned());
  }
  Ok(canon)
}

pub fn get(config: &Path, code: &str, home: Option<&Path>) -> Option<String> {
  let stored = read_object(config).get(KEY)?.get(code)?.as_str()?.to_owned();
  validate(Path::new(&stored), home).ok().map(|p| p.to_string_lossy().into_owned())
}

pub fn set(config: &Path, code: &str, path: &Path, home: Option<&Path>) -> Result<String, String> {
  let canon = validate(path, home)?;
  let text = canon.to_str().ok_or_else(|| "The project folder path is not valid UTF-8".to_owned())?.to_owned();
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
