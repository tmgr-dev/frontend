use std::io::Write;
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tmgr_core::config::{config_dir, ensure_private_dir};
use tmgr_core::error::{Error, Result};

use super::model::{Answers, Settings, StatusDoc};

#[derive(Clone, Debug)]
pub struct Store {
    pub config: PathBuf,
}

impl Store {
    pub fn from_env() -> Result<Store> {
        Ok(Store {
            config: config_dir()?,
        })
    }

    #[cfg(test)]
    pub fn at(config: &Path) -> Store {
        Store {
            config: config.to_path_buf(),
        }
    }

    pub fn dir(&self) -> PathBuf {
        self.config.join("autopilot")
    }

    pub fn answers_path(&self) -> PathBuf {
        self.config.join("autopilot-answers.json")
    }

    pub fn settings_path(&self, id: &str) -> PathBuf {
        self.dir().join("settings").join(format!("{id}.json"))
    }

    pub fn status_path(&self) -> PathBuf {
        self.dir().join("status.json")
    }

    pub fn control_path(&self) -> PathBuf {
        self.dir().join("control.json")
    }

    pub fn log_path(&self) -> PathBuf {
        self.dir().join("log.jsonl")
    }

    pub fn lock_path(&self) -> PathBuf {
        self.dir().join("autopilot.lock")
    }

    pub fn load_answers(&self) -> Result<Option<Answers>> {
        read_json(&self.answers_path())
    }

    pub fn save_answers(&self, a: &Answers) -> Result<()> {
        write_json(&self.answers_path(), a)
    }

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        write_json(&self.settings_path(&s.id), s)
    }

    pub fn load_settings(&self, id: &str) -> Result<Settings> {
        read_json(&self.settings_path(id))?
            .ok_or_else(|| Error::internal(format!("autopilot settings {id} not found")))
    }

    pub fn write_status(&self, doc: &StatusDoc) -> Result<()> {
        write_json(&self.status_path(), doc)
    }

    pub fn read_status(&self) -> Result<Option<StatusDoc>> {
        read_json(&self.status_path())
    }

    pub fn read_control(&self) -> Result<Option<Value>> {
        read_json(&self.control_path())
    }

    pub fn append_log(&self, line: &Value) -> Result<()> {
        ensure_private_dir(&self.dir())?;
        let mut f = open_private_append(&self.log_path())?;
        writeln!(f, "{line}")?;
        Ok(())
    }
}

fn open_private_append(path: &Path) -> Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    Ok(std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(path)?)
}

fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>> {
    match std::fs::read(path) {
        Ok(b) => serde_json::from_slice(&b)
            .map(Some)
            .map_err(|e| Error::internal(format!("{}: {e}", path.display()))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn write_json<T: Serialize>(path: &Path, v: &T) -> Result<()> {
    let b = serde_json::to_vec_pretty(v).map_err(|e| Error::internal(e.to_string()))?;
    write_private(path, &b)
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let dir = path
        .parent()
        .ok_or_else(|| Error::internal("no parent directory"))?;
    ensure_private_dir(dir)?;
    let tmp = dir.join(format!(
        ".{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("f")
    ));
    {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn answers_round_trip_and_missing_is_none() {
        let t = tempfile::tempdir().unwrap();
        let s = Store::at(t.path());
        assert!(s.load_answers().unwrap().is_none());
        let a = Answers {
            max_concurrent: 3,
            ..Answers::default()
        };
        s.save_answers(&a).unwrap();
        assert_eq!(s.load_answers().unwrap(), Some(a));
    }
}
