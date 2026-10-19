use crate::daemon::{daemon_env, log, ENV_CLEAN};
use crate::wire::write_atomic;
use serde_json::{json, Value};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};
use tmgr_ipc::api::{limits::MAX_AUTOPILOT_LOG, AutopilotCommand, AutopilotStatus, ErrorCode};
use tmgr_ipc::dirs::ensure_private_dir;

const LOCK: &str = "autopilot.lock";
const STATUS: &str = "status.json";
const CONTROL: &str = "control.json";
const LOG: &str = "log.jsonl";
const SERVE_LOG: &str = "serve.log";
const SERVE_LOG_CAP: u64 = 5 << 20;
const READ_CAP: u64 = 1 << 20;
const STOP_GRACE: Duration = Duration::from_secs(20);
const TERM_GRACE: Duration = Duration::from_secs(5);

type Fail = (ErrorCode, String);

fn fail(code: ErrorCode, msg: impl Into<String>) -> Fail {
    (code, msg.into())
}

fn internal(e: impl std::fmt::Display) -> Fail {
    fail(ErrorCode::Internal, e.to_string())
}

pub fn state_dir() -> io::Result<PathBuf> {
    tmgr_approval::allow_path()
        .and_then(|p| p.parent().map(|d| d.join("autopilot")))
        .ok_or_else(|| io::Error::other("cannot locate the config directory"))
}

pub fn valid_settings_id(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn open_read(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
}

#[allow(clippy::unnecessary_cast)]
fn lock_owner(dir: &Path) -> Option<u32> {
    let f = open_read(&dir.join(LOCK)).ok()?;
    let mut fl: libc::flock = unsafe { std::mem::zeroed() };
    fl.l_type = libc::F_RDLCK as _;
    fl.l_whence = libc::SEEK_SET as _;
    if unsafe { libc::fcntl(f.as_raw_fd(), libc::F_GETLK, &mut fl) } != 0 {
        return None;
    }
    (fl.l_type as i32 != libc::F_UNLCK as i32)
        .then(|| u32::try_from(fl.l_pid).ok())
        .flatten()
}

fn lock_held(dir: &Path) -> bool {
    let Ok(f) = open_read(&dir.join(LOCK)) else {
        return false;
    };
    unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_SH | libc::LOCK_NB) != 0 }
}

fn read_status(dir: &Path) -> Option<Value> {
    let mut text = String::new();
    open_read(&dir.join(STATUS))
        .ok()?
        .take(READ_CAP)
        .read_to_string(&mut text)
        .ok()?;
    serde_json::from_str(&text).ok()
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn rotate_serve_log(dir: &Path) {
    let path = dir.join(SERVE_LOG);
    if fs::symlink_metadata(&path).is_ok_and(|m| m.len() > SERVE_LOG_CAP) {
        let _ = fs::rename(&path, dir.join(format!("{SERVE_LOG}.1")));
    }
}

fn describe(status: &ExitStatus) -> String {
    match (status.code(), status.signal()) {
        (Some(c), _) => format!("exit code {c}"),
        (_, Some(s)) => format!("signal {s}"),
        _ => "unknown exit".into(),
    }
}

fn mark_crashed(self_exe: &Path, dir: &Path, how: &str) {
    let mut status = read_status(dir)
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({"version": 1}));
    status["state"] = json!("stopped");
    status["stop_reason"] = json!("crashed");
    status["exit"] = json!(how);
    status["stopped_at"] = json!(now_iso());
    match serde_json::to_vec(&status) {
        Ok(body) => {
            if let Err(e) = write_atomic(&dir.join(STATUS), &body) {
                log(&format!("autopilot: status not updated after a crash: {e}"));
            }
        }
        Err(e) => log(&format!("autopilot: status not encoded: {e}")),
    }
    disable_queue(self_exe);
}

fn disable_queue(self_exe: &Path) {
    let mut cmd = Command::new(self_exe);
    cmd.args(["queue", "disable", "--reason", "autopilot crashed"])
        .env_clear()
        .envs(daemon_env())
        .env(ENV_CLEAN, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    match cmd.spawn() {
        Ok(mut child) => {
            thread::spawn(move || match child.wait() {
                Ok(st) if st.success() => {}
                Ok(st) => log(&format!(
                    "autopilot: queue not disabled ({})",
                    describe(&st)
                )),
                Err(e) => log(&format!("autopilot: queue not disabled: {e}")),
            });
        }
        Err(e) => log(&format!("autopilot: queue not disabled: {e}")),
    }
}

fn write_control(dir: &Path, command: AutopilotCommand) -> io::Result<u64> {
    let last = fs::read(dir.join(CONTROL))
        .ok()
        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
        .and_then(|v| v["seq"].as_u64())
        .unwrap_or(0);
    let seq = last + 1;
    let body = json!({"seq": seq, "command": command, "at": now_iso()});
    write_atomic(&dir.join(CONTROL), body.to_string().as_bytes())?;
    Ok(seq)
}

#[derive(Default)]
struct Inner {
    child: Option<Child>,
    adopted: bool,
    stop_requested: bool,
}

#[derive(Default)]
pub struct Host {
    inner: Mutex<Inner>,
}

enum Gone {
    Exited(ExitStatus),
    Unseen,
}

fn guard<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Inner {
    fn running(&self) -> bool {
        self.child.is_some() || self.adopted
    }

    fn pid(&self, dir: &Path) -> Option<u32> {
        match &self.child {
            Some(c) => Some(c.id()),
            None if self.adopted => read_status(dir)
                .and_then(|s| s["pid"].as_u64())
                .and_then(|p| u32::try_from(p).ok()),
            None => None,
        }
    }

    fn refresh(&mut self, self_exe: &Path, dir: &Path) {
        let gone = match self.child.as_mut() {
            Some(c) => match c.try_wait() {
                Ok(Some(st)) => Gone::Exited(st),
                _ => return,
            },
            None => match (lock_held(dir), self.adopted) {
                (true, false) => {
                    self.adopted = true;
                    return;
                }
                (false, true) => Gone::Unseen,
                _ => return,
            },
        };
        self.child = None;
        self.adopted = false;
        let asked = std::mem::take(&mut self.stop_requested);
        let how = match gone {
            Gone::Exited(st) if st.success() => return,
            Gone::Exited(st) => {
                if read_status(dir).is_some_and(|s| s["state"] == "stopped") {
                    return;
                }
                describe(&st)
            }
            Gone::Unseen => {
                let stopped = read_status(dir).is_some_and(|s| s["state"] == "stopped");
                if asked || stopped {
                    return;
                }
                "lost without a final status".into()
            }
        };
        log(&format!("autopilot: crashed ({how})"));
        mark_crashed(self_exe, dir, &how);
    }
}

impl Host {
    pub fn tick(&self, self_exe: &Path) {
        let Ok(dir) = state_dir() else { return };
        guard(&self.inner).refresh(self_exe, &dir);
    }

    pub fn start(&self, self_exe: &Path, settings_id: &str) -> Result<u32, Fail> {
        if !valid_settings_id(settings_id) {
            return Err(fail(
                ErrorCode::InvalidParams,
                "settings_id must be 1 to 64 characters of a-z, 0-9 and -",
            ));
        }
        let dir = state_dir().map_err(internal)?;
        ensure_private_dir(&dir).map_err(internal)?;
        let mut g = guard(&self.inner);
        g.refresh(self_exe, &dir);
        if g.running() || lock_held(&dir) {
            return Err(fail(ErrorCode::Busy, "autopilot is already running"));
        }
        let _ = fs::rename(dir.join(STATUS), dir.join("status.prev.json"));
        let _ = fs::remove_file(dir.join(CONTROL));
        rotate_serve_log(&dir);
        let out = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(dir.join(SERVE_LOG))
            .map_err(internal)?;
        let mut cmd = Command::new(self_exe);
        cmd.args(["queue", "autopilot", "serve", "--settings", settings_id])
            .env_clear()
            .envs(daemon_env())
            .env(ENV_CLEAN, "1")
            .current_dir(&dir)
            .stdin(Stdio::null())
            .stdout(out.try_clone().map_err(internal)?)
            .stderr(out);
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let child = cmd
            .spawn()
            .map_err(|e| internal(format!("cannot start autopilot: {e}")))?;
        let pid = child.id();
        g.child = Some(child);
        g.stop_requested = false;
        Ok(pid)
    }

    pub fn status(&self, self_exe: &Path) -> Result<AutopilotStatus, Fail> {
        let dir = state_dir().map_err(internal)?;
        let mut g = guard(&self.inner);
        g.refresh(self_exe, &dir);
        Ok(AutopilotStatus {
            running: g.running(),
            pid: g.pid(&dir),
            status: read_status(&dir).unwrap_or(Value::Null),
        })
    }

    pub fn control(&self, self_exe: &Path, command: AutopilotCommand) -> Result<u64, Fail> {
        let dir = state_dir().map_err(internal)?;
        let mut g = guard(&self.inner);
        g.refresh(self_exe, &dir);
        if !g.running() {
            return Err(fail(ErrorCode::NotRunning, "autopilot is not running"));
        }
        let seq = write_control(&dir, command).map_err(internal)?;
        if command == AutopilotCommand::Stop {
            g.stop_requested = true;
        }
        Ok(seq)
    }

    pub fn log(&self, tail: u32) -> Result<Vec<Value>, Fail> {
        if !(1..=MAX_AUTOPILOT_LOG).contains(&tail) {
            return Err(fail(
                ErrorCode::InvalidParams,
                format!("tail must be 1 to {MAX_AUTOPILOT_LOG}"),
            ));
        }
        let dir = state_dir().map_err(internal)?;
        let mut f = match open_read(&dir.join(LOG)) {
            Ok(f) => f,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(internal(e)),
        };
        let len = f.metadata().map_err(internal)?.len();
        let cut = len.saturating_sub(READ_CAP);
        f.seek(SeekFrom::Start(cut)).map_err(internal)?;
        let mut bytes = Vec::new();
        f.take(READ_CAP).read_to_end(&mut bytes).map_err(internal)?;
        let text = String::from_utf8_lossy(&bytes);
        let mut lines = text.lines();
        if cut > 0 {
            lines.next();
        }
        let all: Vec<Value> = lines.filter_map(|l| serde_json::from_str(l).ok()).collect();
        let skip = all.len().saturating_sub(tail as usize);
        Ok(all.into_iter().skip(skip).collect())
    }

    pub fn shutdown(&self, self_exe: &Path) {
        let Ok(dir) = state_dir() else { return };
        let pid = {
            let mut g = guard(&self.inner);
            g.refresh(self_exe, &dir);
            if !g.running() {
                return;
            }
            if let Err(e) = write_control(&dir, AutopilotCommand::Stop) {
                log(&format!("autopilot: stop not sent: {e}"));
            }
            g.stop_requested = true;
            (g.pid(&dir), g.child.is_some())
        };
        let (pid, spawned_here) = pid;
        if self.wait_gone(self_exe, &dir, stop_grace()) {
            return;
        }
        if let Some(pid) = pid.filter(|p| spawned_here || lock_owner(&dir) == Some(*p)) {
            let Ok(pid) = i32::try_from(pid) else { return };
            log("autopilot: did not stop in time, sending SIGTERM");
            unsafe { libc::kill(pid, libc::SIGTERM) };
            if self.wait_gone(self_exe, &dir, TERM_GRACE) {
                return;
            }
        }
        log("autopilot: still running at runner exit");
    }

    fn wait_gone(&self, self_exe: &Path, dir: &Path, within: Duration) -> bool {
        let deadline = Instant::now() + within;
        loop {
            {
                let mut g = guard(&self.inner);
                g.refresh(self_exe, dir);
                if !g.running() {
                    return true;
                }
            }
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(Duration::from_millis(50));
        }
    }
}

#[cfg(debug_assertions)]
fn stop_grace() -> Duration {
    std::env::var("TMGR_TEST_AUTOPILOT_GRACE_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map_or(STOP_GRACE, Duration::from_millis)
}

#[cfg(not(debug_assertions))]
fn stop_grace() -> Duration {
    STOP_GRACE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_process_holding_the_lock_is_its_owner() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(LOCK);
        fs::write(&path, "").unwrap();
        assert_eq!(lock_owner(dir.path()), None);
        let mut gate = [0 as libc::c_int; 2];
        assert_eq!(unsafe { libc::pipe(gate.as_mut_ptr()) }, 0);
        let c_path = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        let pid = unsafe { libc::fork() };
        if pid == 0 {
            unsafe {
                let fd = libc::open(c_path.as_ptr(), libc::O_RDWR);
                let mut fl: libc::flock = std::mem::zeroed();
                fl.l_type = libc::F_WRLCK as _;
                fl.l_whence = libc::SEEK_SET as _;
                libc::fcntl(fd, libc::F_SETLK, &fl);
                libc::write(gate[1], b"x".as_ptr().cast(), 1);
                libc::pause();
                libc::_exit(0);
            }
        }
        assert!(pid > 0);
        let mut byte = [0u8; 1];
        unsafe { libc::read(gate[0], byte.as_mut_ptr().cast(), 1) };
        assert_eq!(lock_owner(dir.path()), Some(pid as u32));
        unsafe {
            libc::kill(pid, libc::SIGKILL);
            libc::waitpid(pid, std::ptr::null_mut(), 0);
        }
        assert_eq!(lock_owner(dir.path()), None);
    }

    #[test]
    fn settings_ids_are_lowercase_words() {
        for ok in ["a", "default-1", &"x".repeat(64)] {
            assert!(valid_settings_id(ok), "{ok}");
        }
        for bad in ["", "A", "a b", "a/b", "a_b", "..", &"x".repeat(65)] {
            assert!(!valid_settings_id(bad), "{bad}");
        }
    }
}
