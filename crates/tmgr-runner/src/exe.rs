use std::ffi::OsStr;
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
}
