use rand_core::{OsRng, RngCore};
use std::ffi::CString;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use tmgr_ipc::proto::valid_session_id;

pub const MAX_IMAGE: usize = 10 * 1024 * 1024;
pub const MAX_CHUNK: usize = 512 * 1024;
const DIR: &str = tmgr_ipc::dirs::PASTE_DIR_NAME;
const KEEP: usize = 16;

pub fn image_ext(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

fn invalid(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, why.to_string())
}

fn c_name(name: &str) -> io::Result<CString> {
    CString::new(name.as_bytes()).map_err(|_| invalid("bad file name"))
}

fn own_private_dir(dir: &File) -> io::Result<()> {
    let meta = dir.metadata()?;
    let mine = unsafe { libc::geteuid() };
    if meta.file_type().is_dir() && meta.uid() == mine && meta.mode() & 0o077 == 0 {
        Ok(())
    } else {
        Err(invalid("the paste directory is not private"))
    }
}

fn open_dir(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path)
}

fn prune(dir: &Path, handle: &File) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(std::time::SystemTime, String)> = rd
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let name = e.file_name().into_string().ok()?;
            let image = [".png", ".jpg", ".gif", ".webp"]
                .iter()
                .any(|e| name.ends_with(e));
            (meta.is_file() && image)
                .then(|| (meta.modified().unwrap_or(std::time::UNIX_EPOCH), name))
        })
        .collect();
    files.sort();
    let extra = files.len().saturating_sub(KEEP);
    for (_, name) in files.into_iter().take(extra) {
        if let Ok(c) = c_name(&name) {
            unsafe { libc::unlinkat(handle.as_raw_fd(), c.as_ptr(), 0) };
        }
    }
}

pub fn store(home: &Path, bytes: &[u8], ext: &str) -> io::Result<PathBuf> {
    if !matches!(ext, "png" | "jpg" | "gif" | "webp") {
        return Err(invalid("unsupported image type"));
    }
    let home_meta = fs::symlink_metadata(home)?;
    if !home_meta.file_type().is_dir() || home_meta.uid() != unsafe { libc::geteuid() } {
        return Err(invalid("the session home is not a private directory"));
    }
    let dir = home.join(DIR);
    match DirBuilder::new().mode(0o700).create(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    let handle = open_dir(&dir)?;
    own_private_dir(&handle)?;
    let mut raw = [0u8; 16];
    OsRng.fill_bytes(&mut raw);
    let hex: String = raw.iter().map(|b| format!("{b:02x}")).collect();
    let name = format!("{hex}.{ext}");
    let c = c_name(&name)?;
    let fd = unsafe {
        libc::openat(
            handle.as_raw_fd(),
            c.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600 as libc::c_uint,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let mut file = unsafe { File::from_raw_fd(fd) };
    let path = dir.join(&name);
    if let Err(e) = file.write_all(bytes).and_then(|()| file.sync_all()) {
        remove(&path);
        return Err(e);
    }
    prune(&dir, &handle);
    Ok(path)
}

fn private_subdir(parent: &Path, name: &str) -> io::Result<PathBuf> {
    let dir = parent.join(name);
    match DirBuilder::new().mode(0o700).create(&dir) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    let meta = fs::symlink_metadata(&dir)?;
    if meta.file_type().is_dir()
        && meta.uid() == unsafe { libc::geteuid() }
        && meta.mode() & 0o077 == 0
    {
        Ok(dir)
    } else {
        Err(invalid("the paste directory is not private"))
    }
}

pub fn store_private(root: &Path, id: &str, bytes: &[u8], ext: &str) -> io::Result<PathBuf> {
    if !valid_session_id(id) {
        return Err(invalid("invalid session id"));
    }
    tmgr_ipc::dirs::ensure_private_dir(root)?;
    let home = private_subdir(root, id)?;
    store(&home, bytes, ext)
}

pub fn remove(path: &Path) {
    let _ = fs::remove_file(path);
}

pub fn paste_text(path: &Path, bracketed: bool) -> String {
    let mut escaped = String::new();
    for c in path.to_string_lossy().chars() {
        if c.is_alphanumeric() || "/._-+:@%,=~".contains(c) {
            escaped.push(c);
        } else if !c.is_control() {
            escaped.push('\\');
            escaped.push(c);
        }
    }
    if bracketed {
        format!("\x1b[200~{escaped}\x1b[201~")
    } else {
        escaped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn private_tmp() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        fs::set_permissions(
            t.path(),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .unwrap();
        t
    }

    #[test]
    fn only_png_jpeg_gif_and_webp_are_recognised_by_their_magic_bytes() {
        assert_eq!(image_ext(b"\x89PNG\r\n\x1a\n\0\0"), Some("png"));
        assert_eq!(image_ext(&[0xff, 0xd8, 0xff, 0xe0, 0, 0]), Some("jpg"));
        assert_eq!(image_ext(b"GIF89a\0\0"), Some("gif"));
        assert_eq!(image_ext(b"GIF87a\0\0"), Some("gif"));
        assert_eq!(image_ext(b"RIFF\x01\0\0\0WEBPVP8 "), Some("webp"));
        assert_eq!(image_ext(b"RIFF\x01\0\0\0WAVEfmt "), None);
        assert_eq!(image_ext(b"<svg xmlns=\"\"></svg>"), None);
        assert_eq!(image_ext(b"#!/bin/sh\nrm -rf ~\n"), None);
        assert_eq!(image_ext(b"BM\0\0\0\0\0\0\0\0\0\0"), None);
        assert_eq!(image_ext(b""), None);
    }

    #[test]
    fn a_stored_image_is_private_random_named_and_inside_a_private_directory() {
        let d = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(d.path()).unwrap();
        let a = store(&home, b"\x89PNG\r\n\x1a\nA", "png").unwrap();
        let b = store(&home, b"\x89PNG\r\n\x1a\nB", "png").unwrap();
        assert_ne!(a, b);
        assert_eq!(a.parent().unwrap(), home.join(DIR));
        let name = a.file_name().unwrap().to_str().unwrap();
        assert_eq!(name.len(), 32 + 4);
        assert!(name.ends_with(".png"));
        assert_eq!(fs::read(&a).unwrap(), b"\x89PNG\r\n\x1a\nA");
        assert_eq!(fs::metadata(&a).unwrap().mode() & 0o777, 0o600);
        assert_eq!(fs::metadata(home.join(DIR)).unwrap().mode() & 0o777, 0o700);
    }

    #[test]
    fn a_planted_symlink_or_open_directory_is_refused_and_nothing_is_written() {
        let d = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(d.path()).unwrap();
        let elsewhere = home.join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, home.join(DIR)).unwrap();
        assert!(store(&home, b"\x89PNG\r\n\x1a\n", "png").is_err());
        assert_eq!(fs::read_dir(&elsewhere).unwrap().count(), 0);
        fs::remove_file(home.join(DIR)).unwrap();
        fs::create_dir(home.join(DIR)).unwrap();
        fs::set_permissions(
            home.join(DIR),
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .unwrap();
        assert!(store(&home, b"\x89PNG\r\n\x1a\n", "png").is_err());
        assert_eq!(fs::read_dir(home.join(DIR)).unwrap().count(), 0);
    }

    #[test]
    fn a_home_that_is_missing_or_a_symlink_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(d.path()).unwrap();
        assert!(store(&root.join("none"), b"x", "png").is_err());
        let real = root.join("real");
        fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, root.join("link")).unwrap();
        assert!(store(&root.join("link"), b"x", "png").is_err());
        assert!(!real.join(DIR).exists());
    }

    #[test]
    fn an_unknown_extension_is_refused() {
        let d = tempfile::tempdir().unwrap();
        assert!(store(d.path(), b"x", "sh").is_err());
    }

    #[test]
    fn only_the_newest_images_are_kept() {
        let d = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(d.path()).unwrap();
        for _ in 0..KEEP + 4 {
            store(&home, b"\x89PNG\r\n\x1a\n", "png").unwrap();
        }
        assert_eq!(fs::read_dir(home.join(DIR)).unwrap().count(), KEEP);
    }

    #[test]
    fn without_a_session_home_the_image_goes_to_a_private_per_session_directory() {
        let d = private_tmp();
        let root = fs::canonicalize(d.path()).unwrap().join("paste");
        let a = store_private(&root, "s_aaaaaaaaa3", b"\x89PNG\r\n\x1a\nA", "png").unwrap();
        assert_eq!(a.parent().unwrap(), root.join("s_aaaaaaaaa3").join(DIR));
        assert_eq!(fs::read(&a).unwrap(), b"\x89PNG\r\n\x1a\nA");
        assert_eq!(fs::metadata(&a).unwrap().mode() & 0o777, 0o600);
        assert_eq!(
            fs::metadata(root.join("s_aaaaaaaaa3")).unwrap().mode() & 0o777,
            0o700
        );
        assert_eq!(fs::metadata(&root).unwrap().mode() & 0o777, 0o700);
        let b = store_private(&root, "s_aaaaaaaaa3", b"\x89PNG\r\n\x1a\nB", "png").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn a_planted_symlink_or_open_session_directory_is_refused_in_the_fallback() {
        let d = private_tmp();
        let root = fs::canonicalize(d.path()).unwrap().join("paste");
        let elsewhere = fs::canonicalize(d.path()).unwrap().join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        store_private(&root, "s_aaaaaaaaa5", b"\x89PNG\r\n\x1a\n", "png").unwrap();
        std::os::unix::fs::symlink(&elsewhere, root.join("s_aaaaaaaaa3")).unwrap();
        assert!(store_private(&root, "s_aaaaaaaaa3", b"\x89PNG\r\n\x1a\n", "png").is_err());
        assert_eq!(fs::read_dir(&elsewhere).unwrap().count(), 0);
        fs::create_dir(root.join("s_aaaaaaaaa2")).unwrap();
        fs::set_permissions(
            root.join("s_aaaaaaaaa2"),
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .unwrap();
        assert!(store_private(&root, "s_aaaaaaaaa2", b"\x89PNG\r\n\x1a\n", "png").is_err());
        assert_eq!(fs::read_dir(root.join("s_aaaaaaaaa2")).unwrap().count(), 0);
    }

    #[test]
    fn a_fallback_root_that_is_a_symlink_or_open_is_refused() {
        let d = private_tmp();
        let base = fs::canonicalize(d.path()).unwrap();
        let elsewhere = base.join("elsewhere");
        fs::create_dir(&elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, base.join("link")).unwrap();
        assert!(store_private(&base.join("link"), "s_aaaaaaaaa3", b"x", "png").is_err());
        assert_eq!(fs::read_dir(&elsewhere).unwrap().count(), 0);
        let open = base.join("open");
        fs::create_dir(&open).unwrap();
        fs::set_permissions(&open, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        assert!(store_private(&open, "s_aaaaaaaaa3", b"x", "png").is_err());
    }

    #[test]
    fn the_fallback_refuses_ids_that_could_leave_the_directory() {
        let d = private_tmp();
        let root = fs::canonicalize(d.path()).unwrap().join("paste");
        for id in ["", ".", "..", "../x", "a/b", "s_1/../../x"] {
            assert!(store_private(&root, id, b"x", "png").is_err(), "{id}");
        }
    }

    #[test]
    fn only_the_newest_images_are_kept_in_the_fallback() {
        let d = private_tmp();
        let root = fs::canonicalize(d.path()).unwrap().join("paste");
        for _ in 0..KEEP + 4 {
            store_private(&root, "s_aaaaaaaaa3", b"\x89PNG\r\n\x1a\n", "png").unwrap();
        }
        assert_eq!(
            fs::read_dir(root.join("s_aaaaaaaaa3").join(DIR))
                .unwrap()
                .count(),
            KEEP
        );
    }

    #[test]
    fn the_pasted_path_is_escaped_and_bracketed_only_when_asked() {
        let p = Path::new("/h/Application Support/s_1/.tmgr-paste/ab.png");
        assert_eq!(
            paste_text(p, false),
            "/h/Application\\ Support/s_1/.tmgr-paste/ab.png"
        );
        assert_eq!(
            paste_text(Path::new("/h/a.png"), true),
            "\x1b[200~/h/a.png\x1b[201~"
        );
    }
}
