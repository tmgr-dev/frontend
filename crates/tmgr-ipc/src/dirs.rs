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
    check_ancestors(&fs::canonicalize(path)?)
}

fn own_private_group(gid: u32) -> bool {
    let mut buf = vec![0 as libc::c_char; 4096];
    let mut grp: libc::group = unsafe { std::mem::zeroed() };
    let mut pw: libc::passwd = unsafe { std::mem::zeroed() };
    let mut gres: *mut libc::group = std::ptr::null_mut();
    let mut pres: *mut libc::passwd = std::ptr::null_mut();
    let mut pbuf = vec![0 as libc::c_char; 4096];
    unsafe {
        if libc::getgrgid_r(gid, &mut grp, buf.as_mut_ptr(), buf.len(), &mut gres) != 0
            || gres.is_null()
            || libc::getpwuid_r(
                crate::euid(),
                &mut pw,
                pbuf.as_mut_ptr(),
                pbuf.len(),
                &mut pres,
            ) != 0
            || pres.is_null()
        {
            return false;
        }
        std::ffi::CStr::from_ptr(grp.gr_name) == std::ffi::CStr::from_ptr(pw.pw_name)
            && libc::getegid() == gid
    }
}

fn check_ancestors(canon: &Path) -> io::Result<()> {
    for anc in canon.ancestors().skip(1) {
        let meta = fs::metadata(anc)?;
        let shown = anc.display();
        if meta.uid() != 0 && meta.uid() != crate::euid() {
            return Err(denied(format!("ancestor {shown} is owned by another user")));
        }
        let group_ok = meta.mode() & 0o002 == 0
            && meta.uid() == crate::euid()
            && own_private_group(meta.gid());
        if meta.mode() & 0o022 != 0 && meta.mode() & 0o1000 == 0 && !group_ok {
            return Err(denied(format!(
                "ancestor {shown} is writable by group or others without the sticky bit (mode {:o})",
                meta.mode() & 0o7777
            )));
        }
    }
    Ok(())
}

pub fn ensure_private_dir(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)?;
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

    fn mkdir(p: &Path, mode: u32) {
        fs::create_dir_all(p).unwrap();
        fs::set_permissions(p, fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn refuses_writable_non_sticky_ancestor() {
        let t = tempfile::tempdir().unwrap();
        let loose = t.path().join("loose");
        mkdir(&loose, 0o777);
        let d = loose.join("mid/priv");
        let e = ensure_private_dir(&d).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
        assert!(e.to_string().contains("loose"), "{e}");
        assert!(check_private_dir(&d).is_err());
    }

    #[test]
    fn sticky_writable_ancestor_is_accepted() {
        let t = tempfile::Builder::new()
            .prefix("dirs")
            .tempdir_in("/tmp")
            .unwrap();
        ensure_private_dir(&t.path().join("a")).unwrap();
        let sticky = t.path().join("sticky");
        mkdir(&sticky, 0o1777);
        ensure_private_dir(&sticky.join("priv")).unwrap();
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
