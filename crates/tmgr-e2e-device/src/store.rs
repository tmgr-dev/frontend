use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use tmgr_chat_relay::engine::{KeyStore, Slot};

pub fn private_dir(dir: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())
}

pub fn write_private(path: &Path, value: &str) -> Result<(), String> {
    let tmp = path.with_extension("tmp");
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&tmp)
        .map_err(|e| e.to_string())?;
    f.write_all(value.as_bytes()).map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub struct FileKeyStore(PathBuf);

impl FileKeyStore {
    pub fn open(dir: &Path) -> Result<Self, String> {
        let p = dir.join("relay");
        private_dir(&p)?;
        Ok(Self(p))
    }

    fn path(&self, slot: Slot) -> PathBuf {
        self.0.join(slot.name())
    }
}

impl KeyStore for FileKeyStore {
    fn load(&self, slot: Slot) -> Result<Option<String>, String> {
        match std::fs::read_to_string(self.path(slot)) {
            Ok(s) => Ok(Some(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn save(&self, slot: Slot, value: &str) -> Result<(), String> {
        write_private(&self.path(slot), value)
    }

    fn clear(&self) -> Result<(), String> {
        for s in Slot::ALL {
            match std::fs::remove_file(self.path(s)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(())
    }
}
