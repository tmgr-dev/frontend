use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tmgr_chat_relay::engine::{KeyStore, Slot};

const CONFIG_FILE: &str = "config";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelayConfig {
    pub server: String,
    pub user_id: u64,
}

pub struct RelayDir {
    dir: PathBuf,
}

fn unusable(e: impl std::fmt::Display) -> String {
    format!("relay dir: {e}")
}

fn check_dir(dir: &Path) -> Result<(), String> {
    let meta = fs::symlink_metadata(dir).map_err(unusable)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(unusable("not a plain directory"));
    }
    Ok(())
}

impl RelayDir {
    pub fn open(runner_dir: &Path) -> Result<RelayDir, String> {
        Self::open_named(runner_dir, "relay")
    }

    pub fn open_named(runner_dir: &Path, name: &str) -> Result<RelayDir, String> {
        if name.is_empty() || name.contains(['/', '\\']) || name.starts_with('.') {
            return Err(unusable("bad directory name"));
        }
        let dir = runner_dir.join(name);
        match fs::symlink_metadata(&dir) {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                fs::DirBuilder::new()
                    .mode(0o700)
                    .create(&dir)
                    .map_err(unusable)?;
            }
            Err(e) => return Err(unusable(e)),
        }
        check_dir(&dir)?;
        let mode = fs::symlink_metadata(&dir)
            .map_err(unusable)?
            .permissions()
            .mode();
        if mode & 0o777 != 0o700 {
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).map_err(unusable)?;
        }
        Ok(RelayDir { dir })
    }

    fn path(&self, slot: Slot) -> PathBuf {
        self.dir.join(slot.name())
    }
}

impl RelayDir {
    fn read(&self, path: PathBuf) -> Result<Option<String>, String> {
        check_dir(&self.dir)?;
        let mut f = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
        {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(unusable(e)),
        };
        let mut out = String::new();
        f.read_to_string(&mut out).map_err(unusable)?;
        Ok(Some(out))
    }

    fn write(&self, path: PathBuf, value: &str) -> Result<(), String> {
        check_dir(&self.dir)?;
        let mut tmp = tempfile::Builder::new()
            .prefix(".tmp-")
            .tempfile_in(&self.dir)
            .map_err(unusable)?;
        tmp.write_all(value.as_bytes()).map_err(unusable)?;
        tmp.as_file().sync_all().map_err(unusable)?;
        tmp.persist(path).map_err(|e| unusable(e.error))?;
        File::open(&self.dir)
            .and_then(|d| d.sync_all())
            .map_err(unusable)
    }

    pub fn load_config(&self) -> Result<Option<RelayConfig>, String> {
        match self.read(self.dir.join(CONFIG_FILE))? {
            Some(raw) => serde_json::from_str(&raw)
                .map(Some)
                .map_err(|_| unusable("config is unreadable")),
            None => Ok(None),
        }
    }

    pub fn save_config(&self, cfg: &RelayConfig) -> Result<(), String> {
        let raw = serde_json::to_string(cfg).map_err(unusable)?;
        self.write(self.dir.join(CONFIG_FILE), &raw)
    }

    pub fn clear_config(&self) -> Result<(), String> {
        self.remove(self.dir.join(CONFIG_FILE))
    }

    pub fn remove_file(&self, name: &str) -> Result<(), String> {
        self.remove(self.plain(name)?)
    }

    fn remove(&self, path: PathBuf) -> Result<(), String> {
        check_dir(&self.dir)?;
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(unusable(e)),
        }
    }

    pub fn read_file(&self, name: &str) -> Result<Option<String>, String> {
        self.read(self.plain(name)?)
    }

    pub fn write_file(&self, name: &str, value: &str) -> Result<(), String> {
        self.write(self.plain(name)?, value)
    }

    fn plain(&self, name: &str) -> Result<PathBuf, String> {
        let ok = !name.is_empty()
            && name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
            && !name.starts_with('.');
        if !ok {
            return Err(unusable("bad file name"));
        }
        Ok(self.dir.join(name))
    }
}

impl KeyStore for RelayDir {
    fn load(&self, slot: Slot) -> Result<Option<String>, String> {
        self.read(self.path(slot))
    }

    fn save(&self, slot: Slot, value: &str) -> Result<(), String> {
        self.write(self.path(slot), value)
    }

    fn clear(&self) -> Result<(), String> {
        check_dir(&self.dir)?;
        for slot in Slot::ALL {
            match fs::remove_file(self.path(slot)) {
                Ok(()) => {}
                Err(e) if e.kind() == io::ErrorKind::NotFound => {}
                Err(e) => return Err(unusable(e)),
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn mode(p: &Path) -> u32 {
        fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn dir_is_private_and_tightened() {
        let t = tempfile::tempdir().unwrap();
        let d = RelayDir::open(t.path()).unwrap();
        assert_eq!(mode(&t.path().join("relay")), 0o700);
        fs::set_permissions(&d.dir, fs::Permissions::from_mode(0o755)).unwrap();
        RelayDir::open(t.path()).unwrap();
        assert_eq!(mode(&t.path().join("relay")), 0o700);
    }

    #[test]
    fn a_named_dir_is_separate_from_the_runners_own() {
        let t = tempfile::tempdir().unwrap();
        let own = RelayDir::open(t.path()).unwrap();
        own.save(Slot::Identity, "runner").unwrap();
        let other = RelayDir::open_named(t.path(), "attach-relay").unwrap();
        assert_eq!(mode(&t.path().join("attach-relay")), 0o700);
        assert_eq!(other.load(Slot::Identity).unwrap(), None);
        other.save(Slot::Identity, "attach").unwrap();
        other.clear().unwrap();
        assert_eq!(own.load(Slot::Identity).unwrap().as_deref(), Some("runner"));
        for bad in ["", ".", "..", "a/b", "../relay", ".hidden"] {
            assert!(RelayDir::open_named(t.path(), bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn named_files_are_private_and_config_can_be_cleared() {
        let t = tempfile::tempdir().unwrap();
        let d = RelayDir::open_named(t.path(), "attach-relay").unwrap();
        assert_eq!(d.read_file("cache.json").unwrap(), None);
        d.write_file("cache.json", "{}").unwrap();
        assert_eq!(d.read_file("cache.json").unwrap().as_deref(), Some("{}"));
        assert_eq!(mode(&d.dir.join("cache.json")), 0o600);
        d.remove_file("cache.json").unwrap();
        d.remove_file("cache.json").unwrap();
        assert_eq!(d.read_file("cache.json").unwrap(), None);
        for bad in ["", "../x", "a/b", ".h"] {
            assert!(d.write_file(bad, "x").is_err(), "{bad}");
        }
        d.save_config(&RelayConfig {
            server: "https://tmgr.dev".into(),
            user_id: 7,
        })
        .unwrap();
        d.clear_config().unwrap();
        d.clear_config().unwrap();
        assert_eq!(d.load_config().unwrap(), None);
    }

    #[test]
    fn files_are_private_and_overwrite_atomically() {
        let t = tempfile::tempdir().unwrap();
        let d = RelayDir::open(t.path()).unwrap();
        d.save(Slot::State, "one").unwrap();
        assert_eq!(mode(&d.dir.join("state")), 0o600);
        d.save(Slot::State, "two").unwrap();
        assert_eq!(d.load(Slot::State).unwrap().as_deref(), Some("two"));
        let names: Vec<_> = fs::read_dir(&d.dir).unwrap().collect();
        assert_eq!(names.len(), 1);
    }

    #[test]
    fn missing_file_is_none() {
        let t = tempfile::tempdir().unwrap();
        let d = RelayDir::open(t.path()).unwrap();
        assert_eq!(d.load(Slot::Identity).unwrap(), None);
    }

    #[test]
    fn symlinked_dir_is_refused() {
        let t = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        symlink(other.path(), t.path().join("relay")).unwrap();
        assert!(RelayDir::open(t.path()).is_err());
    }

    #[test]
    fn symlinked_file_is_not_followed() {
        let t = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        fs::write(other.path().join("secret"), "x").unwrap();
        let d = RelayDir::open(t.path()).unwrap();
        symlink(other.path().join("secret"), d.dir.join("identity")).unwrap();
        assert!(d.load(Slot::Identity).is_err());
    }

    #[test]
    fn config_round_trips_privately() {
        let t = tempfile::tempdir().unwrap();
        let d = RelayDir::open(t.path()).unwrap();
        assert_eq!(d.load_config().unwrap(), None);
        let cfg = RelayConfig {
            server: "https://tmgr.dev".into(),
            user_id: 7,
        };
        d.save_config(&cfg).unwrap();
        assert_eq!(d.load_config().unwrap(), Some(cfg));
        assert_eq!(mode(&d.dir.join("config")), 0o600);
    }

    #[test]
    fn clear_removes_all_slots() {
        let t = tempfile::tempdir().unwrap();
        let d = RelayDir::open(t.path()).unwrap();
        for s in Slot::ALL {
            d.save(s, "v").unwrap();
        }
        d.clear().unwrap();
        for s in Slot::ALL {
            assert_eq!(d.load(s).unwrap(), None);
        }
        d.clear().unwrap();
    }
}
