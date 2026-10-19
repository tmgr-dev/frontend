use std::fs;
#[cfg(unix)]
use std::io::Write;
use std::path::{Path, PathBuf};

pub fn config_dir() -> Option<PathBuf> {
    base("XDG_CONFIG_HOME", ".config")
}

pub fn state_dir() -> Option<PathBuf> {
    base("XDG_STATE_HOME", ".local/state")
}

fn base(xdg: &str, fallback: &str) -> Option<PathBuf> {
    let abs = |v: std::ffi::OsString| Some(PathBuf::from(v)).filter(|p| p.is_absolute());
    if let Some(x) = std::env::var_os(xdg).and_then(abs) {
        return Some(x.join("tmgr"));
    }
    let home = std::env::var_os("HOME").and_then(abs)?;
    Some(home.join(fallback).join("tmgr"))
}

#[cfg(unix)]
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("invalid path"))?;
    if !dir.exists() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    if fs::metadata(dir)?.permissions().mode() & 0o777 != 0o700 {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
        std::process::id()
    ));
    let _ = fs::remove_file(&tmp);
    let result = (|| {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

pub fn read_opt(path: &Path) -> std::io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(not(unix))]
pub fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, bytes)
}

#[cfg(unix)]
pub struct Lock {
    _file: std::fs::File,
}

#[cfg(unix)]
impl Lock {
    pub fn exclusive(path: &Path) -> std::io::Result<Lock> {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        use std::os::unix::io::AsRawFd;
        if let Some(dir) = path.parent().filter(|d| !d.exists()) {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(dir)?;
        }
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(path)?;
        if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Lock { _file: file })
    }
}
