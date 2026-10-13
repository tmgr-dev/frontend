use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde_json::{Map, Value};

static WRITE: Mutex<()> = Mutex::new(());

pub fn read_object(path: &Path) -> Map<String, Value> {
  fs::read(path)
    .ok()
    .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
    .and_then(|v| if let Value::Object(m) = v { Some(m) } else { None })
    .unwrap_or_default()
}

pub fn update_object(path: &Path, change: impl FnOnce(&mut Map<String, Value>)) -> io::Result<()> {
  let _guard = WRITE.lock().unwrap_or_else(|e| e.into_inner());
  if let Some(dir) = path.parent() {
    fs::create_dir_all(dir)?;
  }
  let mut object = read_object(path);
  change(&mut object);
  let tmp = path.with_extension("json.tmp");
  fs::write(&tmp, serde_json::to_vec(&object)?)?;
  fs::rename(&tmp, path)
}

pub struct Flag {
  path: PathBuf,
  on: AtomicBool,
}

impl Flag {
  pub fn load(path: PathBuf) -> Flag {
    let on = read_object(&path).get("enabled").and_then(Value::as_bool).unwrap_or(false);
    Flag { path, on: AtomicBool::new(on) }
  }

  pub fn get(&self) -> bool {
    self.on.load(Ordering::SeqCst)
  }

  pub fn path(&self) -> &Path {
    &self.path
  }

  pub fn set(&self, enabled: bool) -> io::Result<()> {
    update_object(&self.path, |o| {
      o.insert("enabled".to_owned(), Value::Bool(enabled));
    })?;
    self.on.store(enabled, Ordering::SeqCst);
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::Flag;

  #[test]
  fn defaults_to_off_and_persists() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cfg/runner.json");
    let flag = Flag::load(path.clone());
    assert!(!flag.get());
    flag.set(true).unwrap();
    assert!(flag.get());
    assert!(Flag::load(path.clone()).get());
    flag.set(false).unwrap();
    assert!(!Flag::load(path).get());
  }

  #[test]
  fn setting_the_flag_keeps_other_keys() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runner.json");
    std::fs::write(&path, br#"{"folders":{"personal":"/work/a"}}"#).unwrap();
    let flag = Flag::load(path.clone());
    flag.set(true).unwrap();
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(v["enabled"], true);
    assert_eq!(v["folders"]["personal"], "/work/a");
  }

  #[test]
  fn garbage_file_means_off() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runner.json");
    std::fs::write(&path, b"{not json").unwrap();
    assert!(!Flag::load(path).get());
  }
}
