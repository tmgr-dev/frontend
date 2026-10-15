use std::fs::File;
use std::path::{Path, PathBuf};
use tmgr_core::config::{config_dir, ensure_private_dir};
use tmgr_core::error::{Error, Result};

#[derive(Clone, Debug)]
pub struct Paths {
    pub dir: PathBuf,
}

pub struct Lock(#[allow(dead_code)] File);

impl Paths {
    pub fn from_env() -> Result<Paths> {
        Ok(Paths { dir: config_dir()? })
    }

    pub fn at(dir: &Path) -> Paths {
        Paths {
            dir: dir.to_path_buf(),
        }
    }

    pub fn rules(&self) -> PathBuf {
        self.dir.join("queue.toml")
    }

    pub fn state(&self) -> PathBuf {
        self.dir.join("queue-state.json")
    }

    pub fn audit(&self) -> PathBuf {
        self.dir.join("queue-audit.jsonl")
    }

    pub fn audit_anchor(&self) -> PathBuf {
        self.dir.join("queue-audit.anchor")
    }

    pub fn lock(&self) -> Result<Lock> {
        lock_file(&self.dir, "queue.lock")
    }
}

pub(crate) fn lock_file(dir: &Path, name: &str) -> Result<Lock> {
    use std::os::unix::fs::OpenOptionsExt;
    ensure_private_dir(dir)?;
    let f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(dir.join(name))?;
    f.lock()
        .map_err(|e| Error::internal(format!("cannot lock {name}: {e}")))?;
    Ok(Lock(f))
}

pub(crate) fn read_private(path: &Path) -> Result<Option<String>> {
    use std::os::unix::fs::MetadataExt;
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    if !meta.is_file() {
        return Err(Error::new(
            tmgr_core::error::Kind::Forbidden,
            format!("{name} is not a regular file"),
        ));
    }
    if meta.mode() & 0o022 != 0 {
        return Err(Error::new(
            tmgr_core::error::Kind::Forbidden,
            format!("{name} is writable by other users"),
        )
        .hint(format!("chmod 600 {}", path.display())));
    }
    Ok(Some(std::fs::read_to_string(path)?))
}
