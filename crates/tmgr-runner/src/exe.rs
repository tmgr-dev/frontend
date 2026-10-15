use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs::File;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

fn executable(p: &Path) -> bool {
    p.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Resolves a command to an absolute executable. A name with `/` is taken as a
/// path (relative ones only against `base`); a bare name is searched in `path`.
pub fn resolve_exe(name: &str, path: &OsStr, base: Option<&Path>) -> Result<PathBuf, String> {
    let missing = || format!("command not found or not executable: {name}");
    if name.is_empty() || name.contains('\0') {
        return Err(missing());
    }
    let found = if name.contains('/') {
        let p = Path::new(name);
        let abs = if p.is_absolute() {
            p.to_path_buf()
        } else {
            base.ok_or_else(|| format!("command path must be absolute: {name}"))?
                .join(p)
        };
        Some(abs).filter(|p| executable(p))
    } else {
        std::env::split_paths(path)
            .filter(|d| d.is_absolute())
            .map(|d| d.join(name))
            .find(|p| executable(p))
    };
    found.ok_or_else(missing)
}

const PIN_MAX: u64 = 512 << 20;

/// Checks that `resolved` is the pinned executable and returns the canonical path to exec.
pub fn verify_pin(resolved: &Path, pin_path: &str, pin_sha256: &str) -> Result<PathBuf, String> {
    let differs = || "the agent executable differs from the approved one".to_string();
    let canonical = resolved.canonicalize().map_err(|_| differs())?;
    if canonical != Path::new(pin_path).canonicalize().map_err(|_| differs())? {
        return Err(differs());
    }
    let file =
        File::open(&canonical).map_err(|_| "the agent executable cannot be read".to_string())?;
    let meta = file.metadata().map_err(|_| differs())?;
    if !meta.is_file() || meta.len() > PIN_MAX {
        return Err("the agent executable is not a regular file within the size limit".into());
    }
    if hash(file)? != pin_sha256 {
        return Err("the agent executable does not match the approved digest".into());
    }
    Ok(canonical)
}

fn hash(file: File) -> Result<String, String> {
    let mut sha = Sha256::new();
    let mut buf = vec![0u8; 64 << 10];
    let mut total = 0u64;
    let mut src = file.take(PIN_MAX + 1);
    loop {
        let n = src
            .read(&mut buf)
            .map_err(|_| "the agent executable cannot be read".to_string())?;
        if n == 0 {
            break;
        }
        total += n as u64;
        sha.update(&buf[..n]);
    }
    if total > PIN_MAX {
        return Err("the agent executable is not a regular file within the size limit".into());
    }
    Ok(sha.finalize().iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn script(dir: &Path, name: &str, mode: u32) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
        p
    }

    #[test]
    fn bare_names_use_the_given_path_only() {
        let t = tempfile::tempdir().unwrap();
        let exe = script(t.path(), "agent", 0o755);
        script(t.path(), "plain", 0o644);
        let path = std::env::join_paths([t.path()]).unwrap();
        assert_eq!(resolve_exe("agent", &path, None).unwrap(), exe);
        assert!(resolve_exe("plain", &path, None).is_err());
        assert!(resolve_exe("agent", OsStr::new("/nonexistent"), None).is_err());
    }

    #[test]
    fn relative_paths_need_a_base() {
        let t = tempfile::tempdir().unwrap();
        let exe = script(t.path(), "agent", 0o755);
        let abs = exe.display().to_string();
        assert_eq!(resolve_exe(&abs, OsStr::new(""), None).unwrap(), exe);
        let err = resolve_exe("./agent", OsStr::new(""), None).unwrap_err();
        assert!(err.contains("absolute"), "{err}");
        assert_eq!(
            resolve_exe("./agent", OsStr::new(""), Some(t.path())).unwrap(),
            t.path().join("./agent")
        );
    }

    #[test]
    fn relative_and_empty_path_entries_are_skipped() {
        let t = tempfile::tempdir().unwrap();
        std::fs::create_dir(t.path().join("bin")).unwrap();
        script(&t.path().join("bin"), "agent", 0o755);
        let exe = script(t.path(), "agent", 0o755);
        for raw in ["./bin", "bin", ""] {
            let path = std::ffi::OsString::from(raw);
            assert!(
                resolve_exe("agent", &path, Some(t.path())).is_err(),
                "{raw}"
            );
        }
        for raw in ["./bin:", ":"] {
            let path = std::ffi::OsString::from(raw);
            assert!(
                resolve_exe("agent", &path, Some(t.path())).is_err(),
                "{raw}"
            );
        }
        let mixed = format!("./bin::{}", t.path().display());
        let found = resolve_exe("agent", OsStr::new(&mixed), Some(t.path())).unwrap();
        assert_eq!(found, exe);
        assert!(found.is_absolute());
    }
}
