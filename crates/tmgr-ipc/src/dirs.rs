use std::fs::{self, DirBuilder};
use std::io;
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

fn home() -> io::Result<PathBuf> {
    env_path("HOME").ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))
}

#[cfg(target_os = "macos")]
fn app_support() -> io::Result<PathBuf> {
    Ok(home()?.join("Library/Application Support/dev.tmgr.cli"))
}

pub fn runner_dir() -> io::Result<PathBuf> {
    if let Some(p) = env_path("TMGR_RUNNER_DIR") {
        return Ok(p);
    }
    #[cfg(target_os = "macos")]
    {
        Ok(app_support()?.join("run"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Some(p) = env_path("XDG_RUNTIME_DIR") {
            return Ok(p.join("tmgr"));
        }
        state_home().map(|p| p.join("tmgr/run"))
    }
}

pub fn homes_dir() -> io::Result<PathBuf> {
    if let Some(p) = env_path("TMGR_RUNNER_HOMES") {
        return Ok(p);
    }
    #[cfg(target_os = "macos")]
    {
        Ok(app_support()?.join("homes"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        state_home().map(|p| p.join("tmgr/homes"))
    }
}

#[cfg(not(target_os = "macos"))]
fn state_home() -> io::Result<PathBuf> {
    match env_path("XDG_STATE_HOME") {
        Some(p) => Ok(p),
        None => Ok(home()?.join(".local/state")),
    }
}

fn denied(msg: String) -> io::Error {
    io::Error::new(io::ErrorKind::PermissionDenied, msg)
}

pub fn check_private_dir(path: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(path)?;
    let shown = path.display();
    if meta.file_type().is_symlink() {
        return Err(denied(format!("{shown} is a symlink")));
    }
    if !meta.is_dir() {
        return Err(denied(format!("{shown} is not a directory")));
    }
    if meta.uid() != crate::euid() {
        return Err(denied(format!("{shown} is owned by another user")));
    }
    if meta.mode() & 0o077 != 0 {
        return Err(denied(format!(
            "{shown} must not be accessible by group or others (mode {:o})",
            meta.mode() & 0o777
        )));
    }
    Ok(())
}

pub fn ensure_private_dir(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    match DirBuilder::new().mode(0o700).create(path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    check_private_dir(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn creates_private_dir_with_parents() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path().join("a/b");
        ensure_private_dir(&d).unwrap();
        assert_eq!(fs::metadata(&d).unwrap().mode() & 0o777, 0o700);
        ensure_private_dir(&d).unwrap();
    }

    #[test]
    fn refuses_symlink() {
        let t = tempfile::tempdir().unwrap();
        let real = t.path().join("real");
        ensure_private_dir(&real).unwrap();
        let link = t.path().join("link");
        symlink(&real, &link).unwrap();
        let e = ensure_private_dir(&link).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
        assert!(e.to_string().contains("symlink"));
    }

    #[test]
    fn refuses_open_mode() {
        let t = tempfile::tempdir().unwrap();
        let d = t.path().join("open");
        fs::create_dir(&d).unwrap();
        fs::set_permissions(&d, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(check_private_dir(&d).is_err());
        assert!(ensure_private_dir(&d).is_err());
    }

    #[test]
    fn refuses_file_and_missing() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("f");
        fs::write(&f, "x").unwrap();
        assert!(check_private_dir(&f).is_err());
        assert!(check_private_dir(&t.path().join("none")).is_err());
    }
}
