use std::fs::{self, DirBuilder};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

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

pub fn time_dir(runner_dir: &Path) -> PathBuf {
    persistent_dir(runner_dir, "time")
}

pub fn questions_dir(runner_dir: &Path) -> PathBuf {
    persistent_dir(runner_dir, "questions")
}

pub const QUESTIONS_LOCK: &str = "scan.lock";

/// `Ok(false)`: a scan holds the lock for longer than `wait`, nothing was removed.
pub fn clear_questions(runner_dir: &Path, wait: Duration) -> io::Result<bool> {
    let dir = questions_dir(runner_dir);
    match fs::symlink_metadata(&dir) {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err(denied(format!("{} is a symlink", dir.display())));
        }
        Ok(meta) if !meta.is_dir() => {
            return Err(denied(format!("{} is not a directory", dir.display())));
        }
        Ok(_) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(true),
        Err(e) => return Err(e),
    }
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dir.join(QUESTIONS_LOCK))?;
    let until = Instant::now() + wait;
    while unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        let err = io::Error::last_os_error();
        if err.kind() != io::ErrorKind::WouldBlock {
            return Err(err);
        }
        if Instant::now() >= until {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let saved = name.ends_with(".json") && name != "settings.json";
        let stale_temp =
            name.starts_with('.') && name.ends_with(".tmp") && !name.starts_with(".settings.json.");
        if saved || stale_temp || name == "marks.jsonl" {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(true)
}

fn persistent_dir(runner_dir: &Path, name: &str) -> PathBuf {
    #[cfg(not(target_os = "macos"))]
    if let Some(rt) = env_path("XDG_RUNTIME_DIR") {
        if runner_dir == rt.join("tmgr") {
            if let Ok(state) = state_home() {
                return state.join("tmgr").join(name);
            }
        }
    }
    runner_dir.join(name)
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

fn check_leaf(path: &Path) -> io::Result<()> {
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

pub fn check_private_dir(path: &Path) -> io::Result<()> {
    check_leaf(path)?;
    check_components(path)?;
    check_ancestors(&fs::canonicalize(path)?)
}

fn check_trusted_dir(dir: &Path, what: &str) -> io::Result<()> {
    let meta = fs::metadata(dir)?;
    let shown = dir.display();
    if meta.uid() != 0 && meta.uid() != crate::euid() {
        return Err(denied(format!("{what} {shown} is owned by another user")));
    }
    if meta.mode() & 0o022 != 0 && meta.mode() & 0o1000 == 0 {
        return Err(denied(format!(
            "{what} {shown} is writable by group or others without the sticky bit (mode {:o}); \
             run `chmod g-w,o-w {shown}` or set TMGR_RUNNER_DIR to a path under a private parent",
            meta.mode() & 0o7777
        )));
    }
    Ok(())
}

fn check_components(path: &Path) -> io::Result<()> {
    let mut cur = PathBuf::new();
    for comp in path.components() {
        cur.push(comp);
        if matches!(
            comp,
            Component::RootDir | Component::Prefix(_) | Component::CurDir
        ) {
            continue;
        }
        let meta = fs::symlink_metadata(&cur)?;
        if !meta.file_type().is_symlink() {
            continue;
        }
        if meta.uid() != 0 && meta.uid() != crate::euid() {
            return Err(denied(format!(
                "symlink {} is owned by another user",
                cur.display()
            )));
        }
        let parent = match cur.parent().filter(|p| !p.as_os_str().is_empty()) {
            Some(p) => p,
            None => Path::new("."),
        };
        check_trusted_dir(parent, "directory holding a symlink in the path:")?;
    }
    Ok(())
}

fn check_ancestors(canon: &Path) -> io::Result<()> {
    for anc in canon.ancestors().skip(1) {
        check_trusted_dir(anc, "ancestor")?;
    }
    Ok(())
}

/// Stricter than `check_private_dir`: no symlink at all between `home` (or `/` when `dir` is not under it) and `dir`.
pub fn check_no_symlink_ancestors(dir: &Path, home: Option<&Path>) -> io::Result<()> {
    if !dir.is_absolute()
        || dir
            .components()
            .any(|c| matches!(c, Component::ParentDir | Component::CurDir))
    {
        return Err(denied(
            "runner directory path must be absolute and normalized".into(),
        ));
    }
    let (mut cur, rest) = match home.filter(|h| h.is_absolute() && dir.starts_with(h)) {
        Some(h) => (h.to_path_buf(), dir.strip_prefix(h).unwrap_or(dir)),
        None => (PathBuf::from("/"), dir.strip_prefix("/").unwrap_or(dir)),
    };
    for part in rest.components() {
        cur.push(part);
        if cur != dir && fs::symlink_metadata(&cur)?.file_type().is_symlink() {
            return Err(denied(format!(
                "a parent of the runner directory is a symlink: {}",
                cur.display()
            )));
        }
    }
    Ok(())
}

pub fn ensure_private_leaf(path: &Path) -> io::Result<()> {
    create_private(path)?;
    check_leaf(path)
}

fn create_private(path: &Path) -> io::Result<()> {
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
    Ok(())
}

pub fn ensure_private_dir(path: &Path) -> io::Result<()> {
    create_private(path)?;
    check_private_dir(path)
}

#[cfg(test)]
mod tests {
    fn private_tmp() -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        std::fs::set_permissions(
            t.path(),
            <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700),
        )
        .unwrap();
        t
    }

    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    #[test]
    fn creates_private_dir_with_parents() {
        let t = private_tmp();
        let d = t.path().join("a/b");
        ensure_private_dir(&d).unwrap();
        assert_eq!(fs::metadata(&d).unwrap().mode() & 0o777, 0o700);
        ensure_private_dir(&d).unwrap();
    }

    #[test]
    fn refuses_symlink() {
        let t = private_tmp();
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
        let t = private_tmp();
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
        let t = private_tmp();
        let loose = t.path().join("loose");
        mkdir(&loose, 0o777);
        let d = loose.join("mid/priv");
        let e = ensure_private_dir(&d).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
        assert!(e.to_string().contains("loose"), "{e}");
        assert!(check_private_dir(&d).is_err());
    }

    #[test]
    fn refuses_group_writable_non_sticky_ancestor() {
        let t = private_tmp();
        let shared = t.path().join("shared");
        mkdir(&shared, 0o770);
        let e = ensure_private_dir(&shared.join("priv")).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
        assert!(e.to_string().contains("shared"), "{e}");
        assert!(e.to_string().contains("chmod"), "{e}");
    }

    #[test]
    fn refuses_symlink_component_held_in_a_writable_directory() {
        let t = private_tmp();
        let real = t.path().join("real");
        ensure_private_dir(&real.join("run")).unwrap();
        let loose = t.path().join("loose");
        mkdir(&loose, 0o777);
        symlink(&real, loose.join("link")).unwrap();
        let e = check_private_dir(&loose.join("link/run")).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
        assert!(e.to_string().contains("loose"), "{e}");
    }

    #[test]
    fn accepts_symlink_component_held_in_a_private_directory() {
        let t = private_tmp();
        let real = t.path().join("real");
        ensure_private_dir(&real.join("run")).unwrap();
        let mine = t.path().join("mine");
        mkdir(&mine, 0o700);
        symlink(&real, mine.join("link")).unwrap();
        check_private_dir(&mine.join("link/run")).unwrap();
    }

    #[test]
    fn leaf_check_ignores_ancestor_modes() {
        let t = private_tmp();
        let loose = t.path().join("loose");
        mkdir(&loose, 0o777);
        ensure_private_leaf(&loose.join("homes")).unwrap();
        assert!(ensure_private_dir(&loose.join("homes")).is_err());
    }

    #[test]
    fn sticky_writable_ancestor_is_accepted() {
        let t = tempfile::Builder::new()
            .prefix("dirs")
            .tempdir_in("/tmp")
            .unwrap();
        std::fs::set_permissions(
            t.path(),
            <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700),
        )
        .unwrap();
        ensure_private_dir(&t.path().join("a")).unwrap();
        let sticky = t.path().join("sticky");
        mkdir(&sticky, 0o1777);
        ensure_private_dir(&sticky.join("priv")).unwrap();
    }

    #[test]
    fn no_symlink_ancestors_between_home_and_dir() {
        let t = private_tmp();
        let base = fs::canonicalize(t.path()).unwrap();
        let real = base.join("real");
        mkdir(&real, 0o700);
        mkdir(&real.join("run"), 0o700);
        let link = base.join("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let run = real.join("run");
        assert!(check_no_symlink_ancestors(&run, Some(&base)).is_ok());
        assert!(check_no_symlink_ancestors(&run, None).is_ok());
        let via = link.join("run");
        assert_eq!(
            check_no_symlink_ancestors(&via, Some(&base))
                .unwrap_err()
                .kind(),
            io::ErrorKind::PermissionDenied
        );
        assert!(check_no_symlink_ancestors(&via, None).is_err());
        assert!(check_no_symlink_ancestors(&via, Some(&link)).is_ok());
        assert!(check_no_symlink_ancestors(&run.join("..").join("run"), None).is_err());
        assert!(check_no_symlink_ancestors(Path::new("relative/run"), None).is_err());
    }

    #[test]
    fn refuses_file_and_missing() {
        let t = private_tmp();
        let f = t.path().join("f");
        fs::write(&f, "x").unwrap();
        assert!(check_private_dir(&f).is_err());
        assert!(check_private_dir(&t.path().join("none")).is_err());
    }

    fn questions_fixture() -> (tempfile::TempDir, PathBuf) {
        let t = private_tmp();
        let q = questions_dir(t.path());
        ensure_private_dir(&q).unwrap();
        for (name, body) in [
            ("a1b2.json", "{}"),
            ("c3d4.json", "{}"),
            ("marks.jsonl", "\n{}\n"),
            (".a1b2.json.00ff.tmp", "{}"),
            ("settings.json", "{\"haiku\":false}"),
            (QUESTIONS_LOCK, ""),
        ] {
            fs::write(q.join(name), body).unwrap();
        }
        fs::create_dir(q.join("haiku-cwd")).unwrap();
        fs::write(q.join("haiku-cwd/keep.json"), "x").unwrap();
        (t, q)
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut v: Vec<_> = fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn clear_questions_removes_sessions_and_marks_and_keeps_settings_lock_and_haiku_cwd() {
        let (t, q) = questions_fixture();
        assert!(clear_questions(t.path(), Duration::from_millis(100)).unwrap());
        assert_eq!(names(&q), ["haiku-cwd", "scan.lock", "settings.json"]);
        assert!(q.join("haiku-cwd/keep.json").exists());
        assert!(clear_questions(t.path(), Duration::from_millis(100)).unwrap());
    }

    #[test]
    fn clear_questions_with_no_questions_dir_creates_nothing() {
        let t = private_tmp();
        assert!(clear_questions(t.path(), Duration::from_millis(100)).unwrap());
        assert_eq!(names(t.path()), Vec::<String>::new());
    }

    #[test]
    fn clear_questions_reports_busy_and_removes_nothing_while_the_scan_lock_is_held() {
        let (t, q) = questions_fixture();
        let held = fs::File::open(q.join(QUESTIONS_LOCK)).unwrap();
        assert_eq!(
            unsafe { libc::flock(held.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
            0
        );
        let before = names(&q);
        assert!(!clear_questions(t.path(), Duration::from_millis(120)).unwrap());
        assert_eq!(names(&q), before);
        drop(held);
        assert!(clear_questions(t.path(), Duration::from_millis(120)).unwrap());
    }

    #[test]
    fn clear_questions_refuses_a_symlinked_questions_dir() {
        let (t, q) = questions_fixture();
        let moved = t.path().join("elsewhere");
        fs::rename(&q, &moved).unwrap();
        symlink(&moved, &q).unwrap();
        let e = clear_questions(t.path(), Duration::from_millis(100)).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
        assert!(moved.join("a1b2.json").exists());
    }

    #[test]
    fn time_dir_of_an_explicit_runner_dir_stays_inside_it() {
        let t = private_tmp();
        assert_eq!(time_dir(t.path()), t.path().join("time"));
    }
}
