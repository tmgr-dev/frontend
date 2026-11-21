use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
    Identity,
    State,
    Log,
}

impl Slot {
    const ALL: [Slot; 3] = [Slot::Identity, Slot::State, Slot::Log];

    fn name(self) -> &'static str {
        match self {
            Slot::Identity => "identity",
            Slot::State => "state",
            Slot::Log => "log",
        }
    }
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
        let dir = runner_dir.join("relay");
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

    pub fn load(&self, slot: Slot) -> Result<Option<String>, String> {
        check_dir(&self.dir)?;
        let mut f = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(self.path(slot))
        {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(unusable(e)),
        };
        let mut out = String::new();
        f.read_to_string(&mut out).map_err(unusable)?;
        Ok(Some(out))
    }

    pub fn save(&self, slot: Slot, value: &str) -> Result<(), String> {
        check_dir(&self.dir)?;
        let mut tmp = tempfile::Builder::new()
            .prefix(".tmp-")
            .tempfile_in(&self.dir)
            .map_err(unusable)?;
        tmp.write_all(value.as_bytes()).map_err(unusable)?;
        tmp.as_file().sync_all().map_err(unusable)?;
        tmp.persist(self.path(slot))
            .map_err(|e| unusable(e.error))?;
        File::open(&self.dir)
            .and_then(|d| d.sync_all())
            .map_err(unusable)
    }

    pub fn clear(&self) -> Result<(), String> {
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
