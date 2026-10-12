use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::Path;
use subtle::ConstantTimeEq;

pub fn generate() -> String {
    let mut b = [0u8; 32];
    getrandom::fill(&mut b).expect("system random source");
    hex(&b)
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub(crate) fn write_private(path: &Path, content: &str) -> io::Result<()> {
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW)
        .mode(0o600)
        .open(path)?;
    f.write_all(content.as_bytes())
}

pub fn write_new(path: &Path) -> io::Result<String> {
    let s = generate();
    write_private(path, &s)?;
    Ok(s)
}

pub fn read(path: &Path) -> io::Result<String> {
    let meta = fs::symlink_metadata(path)?;
    let denied = |why: &str| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{}: {why}", path.display()),
        )
    };
    if !meta.file_type().is_file() {
        return Err(denied("not a regular file"));
    }
    if meta.uid() != crate::euid() {
        return Err(denied("owned by another user"));
    }
    if meta.mode() & 0o077 != 0 {
        return Err(denied("must not be accessible by group or others"));
    }
    Ok(fs::read_to_string(path)?.trim().to_string())
}

pub fn eq(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn write_new_is_0600_and_reads_back() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("client.secret");
        let s = write_new(&p).unwrap();
        assert_eq!(s.len(), 64);
        assert_eq!(fs::metadata(&p).unwrap().mode() & 0o777, 0o600);
        assert_eq!(read(&p).unwrap(), s);
        assert_eq!(
            write_new(&p).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
    }

    #[test]
    fn read_refuses_loose_mode_and_symlink() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("s");
        write_new(&p).unwrap();
        fs::set_permissions(&p, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            read(&p).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        fs::set_permissions(&p, fs::Permissions::from_mode(0o600)).unwrap();
        let l = t.path().join("l");
        symlink(&p, &l).unwrap();
        assert_eq!(
            read(&l).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn read_trims() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("s");
        write_private(&p, "abc\n").unwrap();
        assert_eq!(read(&p).unwrap(), "abc");
    }

    #[test]
    fn eq_compares() {
        let a = generate();
        assert!(eq(&a, &a.clone()));
        assert!(!eq(&a, &generate()));
        assert!(!eq("abc", "abcd"));
        assert!(!eq("", "a"));
    }
}
