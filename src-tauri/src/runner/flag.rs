use std::fs;
use std::io;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

pub struct Flag {
  path: PathBuf,
  on: AtomicBool,
}

impl Flag {
  pub fn load(path: PathBuf) -> Flag {
    let on = fs::read(&path)
      .ok()
      .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
      .and_then(|v| v.get("enabled").and_then(serde_json::Value::as_bool))
      .unwrap_or(false);
    Flag { path, on: AtomicBool::new(on) }
  }

  pub fn get(&self) -> bool {
    self.on.load(Ordering::SeqCst)
  }

  pub fn set(&self, enabled: bool) -> io::Result<()> {
    if let Some(dir) = self.path.parent() {
      fs::create_dir_all(dir)?;
    }
    let tmp = self.path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec(&serde_json::json!({ "enabled": enabled }))?)?;
    fs::rename(&tmp, &self.path)?;
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
  fn garbage_file_means_off() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("runner.json");
    std::fs::write(&path, b"{not json").unwrap();
    assert!(!Flag::load(path).get());
  }
}
