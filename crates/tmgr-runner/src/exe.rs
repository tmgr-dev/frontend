use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs::File;
use std::fs::Metadata;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
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

const CODEX_FORK_MIN: (u64, u64, u64) = (0, 156, 0);

fn version_key(text: &str) -> Option<(u64, u64, u64)> {
    text.split_whitespace().find_map(|tok| {
        let tok = tok.strip_prefix('v').unwrap_or(tok);
        let core = tok.split(['-', '+']).next()?;
        let mut n = core.split('.').map(|p| p.parse::<u64>().ok());
        let (a, b, c) = (n.next()??, n.next()??, n.next()??);
        n.next().is_none().then_some((a, b, c))
    })
}

/// `Ok` when `codex --version` names a release that takes the profile's `--no-daemon` (0.156.0) and so also has `codex fork <id>` (0.81.0).
pub fn codex_fork_supported(version_output: &str) -> Result<(), String> {
    let (a, b, c) = version_key(version_output).ok_or_else(|| {
        "cannot tell which Codex version is installed; forking needs Codex 0.156.0 or newer"
            .to_string()
    })?;
    if (a, b, c) < CODEX_FORK_MIN {
        return Err(format!(
            "Codex {a}.{b}.{c} cannot fork a conversation; update to Codex 0.156.0 or newer"
        ));
    }
    Ok(())
}

pub fn version_output(command: &[String]) -> String {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::time::Duration;
    let Some((exe, args)) = command.split_first() else {
        return String::new();
    };
    let mut cmd = Command::new(exe);
    cmd.args(args)
        .arg("--version")
        .env_clear()
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .process_group(0);
    for key in ["PATH", "HOME"] {
        if let Some(v) = std::env::var_os(key) {
            cmd.env(key, v);
        }
    }
    let Ok(mut child) = cmd.spawn() else {
        return String::new();
    };
    let pid = child.id() as i32;
    let Some(stdout) = child.stdout.take() else {
        return String::new();
    };
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut out = String::new();
        let _ = stdout.take(4096).read_to_string(&mut out);
        let _ = tx.send(out);
    });
    let out = rx.recv_timeout(Duration::from_secs(5)).unwrap_or_default();
    unsafe { libc::kill(-pid, libc::SIGKILL) };
    let _ = child.kill();
    let _ = child.wait();
    out
}

const PIN_MAX: u64 = 512 << 20;

/// Checks that `resolved` is the pinned executable and returns the canonical path to exec.
pub fn verify_pin(resolved: &Path, pin_path: &str, pin_sha256: &str) -> Result<PathBuf, String> {
    let differs = || "the agent executable differs from the approved one".to_string();
    let canonical = resolved.canonicalize().map_err(|_| differs())?;
    if canonical != Path::new(pin_path).canonicalize().map_err(|_| differs())? {
        return Err(differs());
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&canonical)
        .map_err(|_| "the agent executable cannot be read".to_string())?;
    let before = file.metadata().map_err(|_| differs())?;
    if !before.is_file() || before.len() > PIN_MAX {
        return Err("the agent executable is not a regular file within the size limit".into());
    }
    let digest = hash(&file)?;
    let after = file.metadata().map_err(|_| differs())?;
    let on_disk = std::fs::symlink_metadata(&canonical).map_err(|_| differs())?;
    if digest != pin_sha256 {
        return Err("the agent executable does not match the approved digest".into());
    }
    if !same_file(&before, &after) || !same_file(&after, &on_disk) {
        return Err("the agent executable changed while it was checked".into());
    }
    Ok(canonical)
}

fn same_file(a: &Metadata, b: &Metadata) -> bool {
    (a.dev(), a.ino(), a.len(), a.mtime(), a.mtime_nsec())
        == (b.dev(), b.ino(), b.len(), b.mtime(), b.mtime_nsec())
}

fn hash(file: &File) -> Result<String, String> {
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
    #[test]
    fn codex_fork_needs_0_156_or_newer() {
        use super::codex_fork_supported as ok;
        assert!(ok("codex-cli 0.164.0\n").is_ok());
        assert!(ok("codex-cli 0.156.0").is_ok());
        assert!(ok("codex-cli 1.0.0").is_ok());
        assert!(ok("codex-cli 0.156.1-alpha.2").is_ok());
        assert!(ok("codex-cli 0.156.0-alpha.7").is_ok());
        for old in [
            "codex-cli 0.80.0",
            "codex 0.9.12",
            "codex-cli 0.155.1",
            "codex-cli 0.155.9-alpha.1",
        ] {
            let e = ok(old).unwrap_err();
            assert!(e.contains("0.156.0 or newer"), "{old}: {e}");
        }
        for junk in [
            "",
            "codex-cli",
            "codex-cli dev",
            "codex-cli 0.81",
            "codex-cli 0.81.0.1",
            "codex-cli -1.0.0",
        ] {
            let e = ok(junk).unwrap_err();
            assert!(
                e.contains("cannot tell which Codex version"),
                "{junk:?}: {e}"
            );
        }
    }

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

    #[test]
    fn a_pin_needs_the_same_file_digest_and_a_non_link_path() {
        let t = tempfile::tempdir().unwrap();
        let exe = script(t.path(), "agent", 0o755).canonicalize().unwrap();
        let sha = tmgr_ipc::api::sha256_hex(b"#!/bin/sh\n");
        let p = exe.display().to_string();
        assert_eq!(verify_pin(&exe, &p, &sha).unwrap(), exe);
        assert!(verify_pin(&exe, &p, &"0".repeat(64)).is_err());
        let link = t.path().join("link");
        std::os::unix::fs::symlink(&exe, &link).unwrap();
        assert_eq!(verify_pin(&link, &p, &sha).unwrap(), exe);
    }

    #[test]
    fn a_rewritten_file_is_not_the_same_file() {
        let t = tempfile::tempdir().unwrap();
        let exe = script(t.path(), "agent", 0o755);
        let before = std::fs::metadata(&exe).unwrap();
        assert!(same_file(&before, &std::fs::metadata(&exe).unwrap()));
        let f = std::fs::OpenOptions::new().write(true).open(&exe).unwrap();
        f.set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5))
            .unwrap();
        assert!(!same_file(&before, &std::fs::metadata(&exe).unwrap()));
    }
}
