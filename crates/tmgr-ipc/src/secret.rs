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

const MAX_FILE: u64 = 256;

fn open_private(path: &Path) -> io::Result<fs::File> {
    let denied = |why: &str| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{}: {why}", path.display()),
        )
    };
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .map_err(|e| {
            if e.raw_os_error() == Some(libc::ELOOP) {
                denied("is a symlink")
            } else {
                e
            }
        })?;
    let meta = file.metadata()?;
    if !meta.file_type().is_file() {
        return Err(denied("not a regular file"));
    }
    if meta.uid() != crate::euid() {
        return Err(denied("owned by another user"));
    }
    if meta.mode() & 0o077 != 0 {
        return Err(denied("must not be accessible by group or others"));
    }
    if meta.len() == 0 || meta.len() > MAX_FILE {
        return Err(denied("has an unexpected size"));
    }
    Ok(file)
}

pub fn read(path: &Path) -> io::Result<String> {
    let mut buf = Vec::new();
    io::Read::read_to_end(
        &mut io::Read::take(open_private(path)?, MAX_FILE + 1),
        &mut buf,
    )?;
    let malformed = |why: &str| {
        io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{}: {why}", path.display()),
        )
    };
    if buf.len() as u64 > MAX_FILE {
        return Err(malformed("has an unexpected size"));
    }
    String::from_utf8(buf)
        .map(|s| s.trim().to_string())
        .map_err(|_| malformed("is malformed"))
}

pub fn valid_secret(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

pub fn valid_addr(s: &str) -> bool {
    let Some((uid, rand)) = s.strip_prefix("tmgr-").and_then(|r| r.split_once('-')) else {
        return false;
    };
    !uid.is_empty()
        && uid.len() <= 10
        && uid.bytes().all(|c| c.is_ascii_digit())
        && rand.len() == 16
        && rand.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

fn read_checked(path: &Path, ok: fn(&str) -> bool, what: &str) -> io::Result<String> {
    let s = read(path)?;
    if ok(&s) {
        Ok(s)
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("{}: {what} is malformed", path.display()),
        ))
    }
}

pub fn read_secret(path: &Path) -> io::Result<String> {
    read_checked(path, valid_secret, "secret")
}

pub fn read_addr(path: &Path) -> io::Result<String> {
    read_checked(path, valid_addr, "address")
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
    fn read_refuses_oversized_empty_and_non_utf8_files() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("s");
        write_private(&p, &"a".repeat(257)).unwrap();
        assert_eq!(
            read(&p).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        let e = t.path().join("e");
        write_private(&e, "").unwrap();
        assert_eq!(
            read(&e).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        let b = t.path().join("b");
        write_private(&b, "ok").unwrap();
        fs::write(&b, [0xff, 0xfe]).unwrap();
        assert_eq!(
            read(&b).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        write_private(&t.path().join("m"), &"a".repeat(256)).unwrap();
        assert_eq!(read(&t.path().join("m")).unwrap().len(), 256);
    }

    #[test]
    fn read_does_not_open_a_fifo_or_a_directory() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("fifo");
        assert_eq!(
            unsafe {
                libc::mkfifo(
                    std::ffi::CString::new(f.to_str().unwrap())
                        .unwrap()
                        .as_ptr(),
                    0o600,
                )
            },
            0
        );
        assert_eq!(
            read(&f).unwrap_err().kind(),
            io::ErrorKind::PermissionDenied
        );
        assert!(read(t.path()).is_err());
    }

    #[test]
    fn read_secret_wants_64_lowercase_hex() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("client.secret");
        let s = write_new(&p).unwrap();
        assert_eq!(read_secret(&p).unwrap(), s);
        for (i, bad) in ["abc", &"A".repeat(64), &"g".repeat(64), &"a".repeat(65)]
            .iter()
            .enumerate()
        {
            let q = t.path().join(format!("bad{i}"));
            write_private(&q, bad).unwrap();
            assert_eq!(
                read_secret(&q).unwrap_err().kind(),
                io::ErrorKind::PermissionDenied,
                "{bad}"
            );
        }
    }

    #[test]
    fn read_addr_wants_the_runner_address_shape() {
        assert!(valid_addr("tmgr-501-0123456789abcdef"));
        for bad in [
            "",
            "tmgr-",
            "tmgr--0123456789abcdef",
            "tmgr-x-0123456789abcdef",
            "tmgr-501-0123456789abcde",
            "tmgr-501-0123456789ABCDEF",
            "tmgr-12345678901-0123456789abcdef",
            "other-501-0123456789abcdef",
            "tmgr-501-0123456789abcdef\0evil",
        ] {
            assert!(!valid_addr(bad), "{bad:?}");
        }
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("runner.addr");
        write_private(&p, "tmgr-501-0123456789abcdef\n").unwrap();
        assert_eq!(read_addr(&p).unwrap(), "tmgr-501-0123456789abcdef");
        write_private(&t.path().join("x"), "nope").unwrap();
        assert!(read_addr(&t.path().join("x")).is_err());
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
