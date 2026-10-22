use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tmgr_core::active::state_dir;
use tmgr_core::config::{ensure_private_dir, write_atomic};
use tmgr_core::error::{Error, Result};
use tmgr_runner::attach::{duration_label, WatchState, WatchStatus};

const IGNORED: &[&str] = &[
    "target",
    "node_modules",
    ".git",
    ".hg",
    ".svn",
    ".venv",
    "__pycache__",
    ".next",
    ".nuxt",
    ".gradle",
    ".turbo",
    ".cache",
    ".idea",
    ".pytest_cache",
    ".mypy_cache",
    ".tox",
    "dist",
    "coverage",
];
const POLL: Duration = Duration::from_millis(500);
const QUIET: Duration = Duration::from_millis(600);
const LONGEST: Duration = Duration::from_secs(3);
const MAX_FILES: usize = 60_000;
const MAX_DEPTH: usize = 16;

pub(super) fn ignored(name: &str) -> bool {
    IGNORED.contains(&name)
}

pub(super) fn too_broad(dir: &Path) -> bool {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    dir.parent().is_none() || home.as_deref() == Some(dir)
}

pub(super) fn detect(dir: &Path) -> Option<String> {
    let has = |f: &str| dir.join(f).exists();
    if has("Cargo.toml") {
        return Some("cargo test".into());
    }
    if has("package.json") {
        let runner = if has("pnpm-lock.yaml") {
            "pnpm"
        } else if has("yarn.lock") {
            "yarn"
        } else if has("bun.lockb") || has("bun.lock") {
            "bun"
        } else {
            "npm"
        };
        return Some(format!("{runner} test"));
    }
    if has("go.mod") {
        return Some("go test ./...".into());
    }
    if has("pyproject.toml") || has("pytest.ini") || has("tox.ini") || has("setup.py") {
        return Some("pytest".into());
    }
    if has("mvnw") || has("pom.xml") {
        return Some(
            if has("mvnw") {
                "./mvnw test"
            } else {
                "mvn test"
            }
            .into(),
        );
    }
    if has("gradlew") {
        return Some("./gradlew test".into());
    }
    if has("composer.json") {
        return Some("composer test".into());
    }
    if has("Gemfile") {
        return Some("bundle exec rake test".into());
    }
    if std::fs::read_to_string(dir.join("Makefile"))
        .is_ok_and(|m| m.lines().any(|l| l.starts_with("test:")))
    {
        return Some("make test".into());
    }
    None
}

pub(super) fn snapshot(root: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    let mut count = 0usize;
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries: Vec<_> = entries.flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if !ignored(name) && depth < MAX_DEPTH {
                    stack.push((entry.path(), depth + 1));
                }
            } else if kind.is_file() {
                count += 1;
                if count > MAX_FILES {
                    return hasher.finish();
                }
                if let Ok(meta) = entry.metadata() {
                    let modified = meta.modified().ok().and_then(|m| {
                        m.duration_since(std::time::UNIX_EPOCH)
                            .ok()
                            .map(|d| d.as_nanos())
                    });
                    (entry.path(), meta.len(), modified).hash(&mut hasher);
                }
            }
        }
    }
    hasher.finish()
}

#[derive(Debug)]
pub(super) struct Debounce {
    quiet: Duration,
    longest: Duration,
    first: Option<Instant>,
    last: Option<Instant>,
}

impl Debounce {
    pub(super) fn new(quiet: Duration, longest: Duration) -> Self {
        Self {
            quiet,
            longest,
            first: None,
            last: None,
        }
    }

    pub(super) fn feed(&mut self, now: Instant, changed: bool) -> bool {
        if changed {
            let first = *self.first.get_or_insert(now);
            self.last = Some(now);
            if now.duration_since(first) >= self.longest {
                self.first = None;
                self.last = None;
                return true;
            }
            return false;
        }
        let (Some(first), Some(last)) = (self.first, self.last) else {
            return false;
        };
        if now.duration_since(last) >= self.quiet || now.duration_since(first) >= self.longest {
            self.first = None;
            self.last = None;
            return true;
        }
        false
    }
}

pub(super) fn outcome(status: &ExitStatus, elapsed: Duration, runs: u32) -> WatchStatus {
    WatchStatus {
        state: if status.success() {
            WatchState::Ok
        } else {
            WatchState::Fail
        },
        code: status.code(),
        ms: u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX),
        runs,
    }
}

pub(super) fn status_dir() -> Option<PathBuf> {
    state_dir().map(|d| d.join("watch"))
}

pub(super) fn status_path(dir: &Path, pid: u32) -> PathBuf {
    dir.join(format!("{pid}.json"))
}

pub(super) fn rerun_path(dir: &Path, pid: u32) -> PathBuf {
    dir.join(format!("{pid}.rerun"))
}

pub(super) fn read_status(dir: &Path, pid: u32) -> Option<WatchStatus> {
    let text = std::fs::read_to_string(status_path(dir, pid)).ok()?;
    serde_json::from_str(&text).ok()
}

pub(super) fn request_rerun(dir: &Path, pid: u32) -> std::io::Result<()> {
    ensure_private_dir(dir).map_err(std::io::Error::other)?;
    write_atomic(&rerun_path(dir, pid), b"1", 0o600).map_err(std::io::Error::other)
}

fn publish(dir: &Path, pid: u32, status: &WatchStatus) {
    if let Ok(text) = serde_json::to_string(status) {
        let _ = write_atomic(&status_path(dir, pid), text.as_bytes(), 0o600);
    }
}

fn take_rerun(dir: &Path, pid: u32) -> bool {
    std::fs::remove_file(rerun_path(dir, pid)).is_ok()
}

static TERMINATED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_terminate(_: libc::c_int) {
    TERMINATED.store(true, Ordering::SeqCst);
}

fn install_handlers() {
    for sig in [libc::SIGTERM, libc::SIGHUP, libc::SIGINT] {
        unsafe {
            libc::signal(
                sig,
                on_terminate as extern "C" fn(libc::c_int) as libc::sighandler_t,
            );
        }
    }
}

fn terminated() -> bool {
    TERMINATED.load(Ordering::SeqCst)
}

fn stop(child: &mut std::process::Child) {
    let group = -(child.id() as libc::pid_t);
    unsafe {
        libc::kill(group, libc::SIGTERM);
    }
    for _ in 0..20 {
        if child.try_wait().ok().flatten().is_some() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    unsafe {
        libc::kill(group, libc::SIGKILL);
    }
    let _ = child.wait();
}

fn forget(dir: &Path, pid: u32) {
    let _ = std::fs::remove_file(status_path(dir, pid));
    let _ = std::fs::remove_file(rerun_path(dir, pid));
}

pub(super) fn run(dir: &str, command: &str) -> Result<i32> {
    let root = PathBuf::from(dir);
    if !root.is_dir() {
        return Err(Error::usage("the folder to watch does not exist"));
    }
    if command.trim().is_empty() {
        return Err(Error::usage("no command to run"));
    }
    let status_dir = status_dir().ok_or_else(|| Error::internal("no state directory"))?;
    ensure_private_dir(&status_dir).map_err(|e| Error::internal(e.to_string()))?;
    let pid = std::process::id();
    forget(&status_dir, pid);
    install_handlers();
    let code = watch_loop(&root, command, &status_dir, pid);
    forget(&status_dir, pid);
    code
}

fn watch_loop(root: &Path, command: &str, status_dir: &Path, pid: u32) -> Result<i32> {
    let mut runs = 0u32;
    let mut wait_for_change = false;
    loop {
        if wait_for_change {
            let mut seen = snapshot(root);
            let mut debounce = Debounce::new(QUIET, LONGEST);
            loop {
                std::thread::sleep(POLL);
                if terminated() {
                    return Ok(130);
                }
                if take_rerun(status_dir, pid) {
                    break;
                }
                let now = snapshot(root);
                let changed = now != seen;
                seen = now;
                if debounce.feed(Instant::now(), changed) {
                    break;
                }
            }
        }
        runs += 1;
        println!("\n\x1b[2m$ {command}\x1b[0m");
        publish(
            status_dir,
            pid,
            &WatchStatus {
                state: WatchState::Running,
                code: None,
                ms: 0,
                runs,
            },
        );
        let started = Instant::now();
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .current_dir(root)
            .process_group(0)
            .spawn()
            .map_err(|e| Error::internal(format!("cannot run the command: {e}")))?;
        let result = loop {
            if terminated() {
                stop(&mut child);
                return Ok(130);
            }
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {}
                Err(e) => {
                    stop(&mut child);
                    return Err(Error::internal(e.to_string()));
                }
            }
            if take_rerun(status_dir, pid) {
                stop(&mut child);
                break None;
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        match result {
            Some(status) => {
                let out = outcome(&status, started.elapsed(), runs);
                println!(
                    "\x1b[2m{} {} · watching for changes, click the pane header to rerun\x1b[0m",
                    out.glyph(),
                    duration_label(out.ms)
                );
                publish(status_dir, pid, &out);
                wait_for_change = true;
            }
            None => wait_for_change = false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn the_ignore_list_covers_build_output_dependencies_and_vcs() {
        for n in ["target", "node_modules", ".git"] {
            assert!(ignored(n), "{n}");
        }
        assert!(!ignored("src"));
    }

    #[test]
    fn a_snapshot_ignores_ignored_folders_and_sees_real_changes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join("target/debug")).unwrap();
        std::fs::create_dir_all(root.join("node_modules/x")).unwrap();
        std::fs::write(root.join("src/a.rs"), "1").unwrap();
        let before = snapshot(root);
        std::fs::write(root.join("target/debug/out"), "x").unwrap();
        std::fs::write(root.join("node_modules/x/i.js"), "x").unwrap();
        assert_eq!(snapshot(root), before);
        std::fs::write(root.join("src/a.rs"), "22").unwrap();
        assert_ne!(snapshot(root), before);
        let again = snapshot(root);
        std::fs::write(root.join("src/b.rs"), "").unwrap();
        assert_ne!(snapshot(root), again);
    }

    #[test]
    fn the_root_and_home_are_too_broad_to_watch() {
        assert!(too_broad(Path::new("/")));
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap();
        assert!(too_broad(&home));
        assert!(!too_broad(&home.join("project")));
    }

    #[test]
    fn constant_writes_still_fire_at_the_cap() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        let mut d = Debounce::new(ms(600), ms(3000));
        let fired: Vec<bool> = (0..8).map(|i| d.feed(t0 + ms(i * 500), true)).collect();
        assert_eq!(fired.iter().filter(|f| **f).count(), 1);
        assert!(fired[6]);
    }

    #[test]
    fn a_whole_process_group_is_stopped() {
        let mut child = Command::new("sh")
            .arg("-c")
            .arg("sleep 30 & wait")
            .process_group(0)
            .spawn()
            .unwrap();
        let group = -(child.id() as libc::pid_t);
        std::thread::sleep(Duration::from_millis(200));
        stop(&mut child);
        std::thread::sleep(Duration::from_millis(200));
        assert_eq!(unsafe { libc::kill(group, 0) }, -1);
    }

    #[test]
    fn the_debounce_waits_for_a_quiet_gap_but_never_longer_than_the_cap() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        let mut d = Debounce::new(ms(600), ms(3000));
        assert!(!d.feed(t0, false));
        assert!(!d.feed(t0 + ms(100), true));
        assert!(!d.feed(t0 + ms(300), true));
        assert!(!d.feed(t0 + ms(700), false));
        assert!(d.feed(t0 + ms(900), false));
        assert!(!d.feed(t0 + ms(1000), false));
        let mut d = Debounce::new(ms(600), ms(3000));
        d.feed(t0, true);
        for i in 1..=5 {
            assert!(!d.feed(t0 + ms(500 * i), true));
        }
        assert!(d.feed(t0 + ms(3100), false));
    }

    #[test]
    fn the_command_comes_from_the_repo_type() {
        let case = |files: &[&str]| {
            let dir = tempfile::tempdir().unwrap();
            for f in files {
                std::fs::write(dir.path().join(f), "").unwrap();
            }
            detect(dir.path())
        };
        assert_eq!(case(&["Cargo.toml"]).as_deref(), Some("cargo test"));
        assert_eq!(case(&["package.json"]).as_deref(), Some("npm test"));
        assert_eq!(
            case(&["package.json", "pnpm-lock.yaml"]).as_deref(),
            Some("pnpm test")
        );
        assert_eq!(
            case(&["package.json", "yarn.lock"]).as_deref(),
            Some("yarn test")
        );
        assert_eq!(case(&["go.mod"]).as_deref(), Some("go test ./..."));
        assert_eq!(case(&["pyproject.toml"]).as_deref(), Some("pytest"));
        assert_eq!(case(&[]), None);
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Makefile"), "all:\n\ttrue\ntest:\n\ttrue\n").unwrap();
        assert_eq!(detect(dir.path()).as_deref(), Some("make test"));
    }

    #[test]
    fn the_result_comes_from_the_exit_code() {
        let ok = outcome(&ExitStatus::from_raw(0), Duration::from_millis(3200), 2);
        assert_eq!(ok.state, WatchState::Ok);
        assert_eq!(ok.ms, 3200);
        let bad = outcome(&ExitStatus::from_raw(1 << 8), Duration::from_secs(1), 3);
        assert_eq!(bad.state, WatchState::Fail);
        assert_eq!(bad.code, Some(1));
        let killed = outcome(&ExitStatus::from_raw(9), Duration::from_secs(1), 3);
        assert_eq!(killed.state, WatchState::Fail);
        assert_eq!(killed.code, None);
    }

    #[test]
    fn status_and_rerun_files_round_trip_in_a_private_dir() {
        let dir = tempfile::tempdir().unwrap();
        let s = WatchStatus {
            state: WatchState::Fail,
            code: Some(2),
            ms: 1500,
            runs: 4,
        };
        publish(dir.path(), 7, &s);
        assert_eq!(read_status(dir.path(), 7), Some(s));
        assert!(read_status(dir.path(), 8).is_none());
        assert!(!take_rerun(dir.path(), 7));
        request_rerun(dir.path(), 7).unwrap();
        assert!(take_rerun(dir.path(), 7));
        assert!(!take_rerun(dir.path(), 7));
    }
}
