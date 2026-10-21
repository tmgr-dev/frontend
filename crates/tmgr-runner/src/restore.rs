use crate::wire::write_atomic;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tmgr_ipc::api::{
    limits::MAX_BUNDLE, sha256_hex, CwdPin, EventMsg, ExePin, Origin, RunBundle, RunCreateReq,
    State, StateKind, TaskRef,
};
use tmgr_ipc::proto::valid_session_id;

pub const TTL_MS: u64 = 7 * 24 * 3600 * 1000;
const MAX_INTENT: u64 = 64 * 1024;
const MAX_LISTED: usize = 256;
const STORE: &str = ".intents";
const RELAUNCH: &str = "relaunch.json";
const RELAUNCH_BUNDLE: &str = "relaunch.bundle.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResumeRec {
    pub args: Vec<String>,
    pub at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub by_id: Option<Vec<String>>,
}

impl Intent {
    pub fn needs_queue_pin(&self) -> bool {
        self.exe_pin.is_none()
            && match self.keyed {
                Some(k) => k,
                None => self.agent_work,
            }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Intent {
    pub v: u32,
    pub session: String,
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub name: String,
    pub cwd: String,
    pub cwd_pin: CwdPin,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<Origin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clone_of: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub agent_work: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyed: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exe_pin: Option<ExePin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argv_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume_decl: Option<ResumeRec>,
    #[serde(default)]
    pub bypass: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pids: Vec<u32>,
    pub created_ms: u64,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Restorable,
    Running,
    Restoring,
    Expired,
    Link,
    Bypass,
}

impl Status {
    pub fn reason(self) -> &'static str {
        match self {
            Status::Restorable => "can be restored",
            Status::Running => "still running",
            Status::Restoring => "a restore of it is in progress",
            Status::Expired => "older than 7 days; start it again with tmgr run",
            Status::Link => "started over a link; the leader has to request it again",
            Status::Bypass => "bypass runs are never restored; start it again with tmgr run",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Entry {
    pub intent: Intent,
    pub status: Status,
    pub resumable: bool,
}

pub fn store(homes: &Path) -> PathBuf {
    homes.join(STORE)
}

fn intent_path(store: &Path, id: &str) -> PathBuf {
    store.join(format!("{id}.json"))
}

fn claimed_path(store: &Path, id: &str) -> PathBuf {
    store.join(format!("{id}.claimed"))
}

fn bundle_path(store: &Path, id: &str) -> PathBuf {
    store.join(format!("{id}.bundle.json"))
}

fn bad(m: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.into())
}

fn check_id(id: &str) -> io::Result<()> {
    if valid_session_id(id) {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid session id",
        ))
    }
}

fn read_capped(path: &Path, cap: u64) -> io::Result<Vec<u8>> {
    use std::os::unix::fs::OpenOptionsExt;
    let f = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    if !f.metadata()?.is_file() {
        return Err(bad("not a regular file"));
    }
    let mut buf = Vec::new();
    f.take(cap + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > cap {
        return Err(bad("file is too large"));
    }
    Ok(buf)
}

pub fn record(store: &Path, intent: &Intent, bundle: Option<&str>) -> io::Result<()> {
    check_id(&intent.session)?;
    tmgr_ipc::dirs::ensure_private_leaf(store)?;
    if let Some(b) = bundle {
        write_atomic(&bundle_path(store, &intent.session), b.as_bytes())?;
    }
    let bytes = serde_json::to_vec(intent).map_err(io::Error::other)?;
    write_atomic(&intent_path(store, &intent.session), &bytes)
}

pub fn forget(store: &Path, id: &str) {
    if check_id(id).is_err() {
        return;
    }
    let _ = fs::remove_file(intent_path(store, id));
    let _ = fs::remove_file(bundle_path(store, id));
}

fn clean_exit(s: &State) -> bool {
    s.kind == StateKind::Exited
        && s.exit_code == Some(0)
        && s.detail.as_deref() != Some("interrupted")
}

pub fn on_state(store: &Path, id: &str, s: &State) {
    if clean_exit(s) {
        forget(store, id);
    }
}

pub fn forget_delay() -> Duration {
    std::env::var("TMGR_RESTORE_FORGET_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map_or(Duration::from_secs(10), Duration::from_millis)
}

// Agents often exit 0 on the SIGTERM/SIGHUP of a logout or shutdown; the runner dies within the delay then, so the record survives.
pub fn on_event(store: &Path, ev: &EventMsg, delay: Duration) {
    if !clean_exit(&ev.state) {
        return;
    }
    if delay.is_zero() {
        forget(store, &ev.session);
        return;
    }
    let (store, id) = (store.to_path_buf(), ev.session.clone());
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        forget(&store, &id);
    });
}

fn parse_intent(bytes: &[u8], id: &str) -> io::Result<Intent> {
    let i: Intent = serde_json::from_slice(bytes).map_err(|e| bad(e.to_string()))?;
    if i.v != 1 || i.session != id {
        return Err(bad("intent does not match its file"));
    }
    Ok(i)
}

pub fn load(store: &Path, id: &str) -> io::Result<Intent> {
    check_id(id)?;
    parse_intent(&read_capped(&intent_path(store, id), MAX_INTENT)?, id)
}

#[cfg(target_os = "linux")]
pub fn boot_marker() -> Option<String> {
    fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(target_os = "macos")]
pub fn boot_marker() -> Option<String> {
    let mut tv = libc::timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    let mut len = std::mem::size_of::<libc::timeval>();
    let name = c"kern.boottime";
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            (&mut tv as *mut libc::timeval).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    (rc == 0 && tv.tv_sec > 0).then(|| format!("{}.{}", tv.tv_sec, tv.tv_usec))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn boot_marker() -> Option<String> {
    None
}

fn pid_alive(pid: u32) -> bool {
    let Ok(p) = i32::try_from(pid) else {
        return false;
    };
    p > 1
        && (unsafe { libc::kill(p, 0) } == 0
            || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
}

fn group_alive(pid: u32) -> bool {
    let Ok(p) = i32::try_from(pid) else {
        return false;
    };
    p > 1
        && (unsafe { libc::kill(-p, 0) } == 0
            || io::Error::last_os_error().raw_os_error() == Some(libc::EPERM))
}

fn same_boot(recorded_boot: Option<&str>, boot: Option<&str>) -> bool {
    match (recorded_boot, boot) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

pub fn pids_may_run(pids: &[u32], recorded_boot: Option<&str>, boot: Option<&str>) -> bool {
    same_boot(recorded_boot, boot) && pids.iter().any(|p| pid_alive(*p) || group_alive(*p))
}

pub fn group_may_run(pid: u32, recorded_boot: Option<&str>, boot: Option<&str>) -> bool {
    same_boot(recorded_boot, boot) && group_alive(pid)
}

pub fn may_still_run(i: &Intent, boot: Option<&str>) -> bool {
    pids_may_run(&i.pids, i.boot.as_deref(), boot)
}

pub fn status(i: &Intent, now_ms: u64, running: bool) -> Status {
    if running || may_still_run(i, boot_marker().as_deref()) {
        Status::Running
    } else if i.origin.is_some() {
        Status::Link
    } else if i.bypass {
        Status::Bypass
    } else if now_ms.saturating_sub(i.created_ms) > TTL_MS {
        Status::Expired
    } else {
        Status::Restorable
    }
}

pub fn list(store: &Path, homes: &Path, now_ms: u64, running: &dyn Fn(&str) -> bool) -> Vec<Entry> {
    let Ok(rd) = fs::read_dir(store) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in rd.flatten().take(MAX_LISTED * 3) {
        let name = entry.file_name().to_string_lossy().into_owned();
        let (id, claimed) = match (name.strip_suffix(".json"), name.strip_suffix(".claimed")) {
            (Some(id), _) if valid_session_id(id) => (id.to_string(), false),
            (_, Some(id)) if valid_session_id(id) => (id.to_string(), true),
            _ => continue,
        };
        let Ok(intent) = read_capped(&entry.path(), MAX_INTENT).and_then(|b| parse_intent(&b, &id))
        else {
            continue;
        };
        let status = if claimed {
            Status::Restoring
        } else {
            status(&intent, now_ms, running(&id))
        };
        let resumable = homes.join(&id).is_dir();
        out.push(Entry {
            intent,
            status,
            resumable,
        });
        if out.len() >= MAX_LISTED {
            break;
        }
    }
    out.sort_by(|a, b| {
        (a.intent.created_ms, &a.intent.session).cmp(&(b.intent.created_ms, &b.intent.session))
    });
    out
}

pub fn recover_claims(store: &Path) {
    let Ok(rd) = fs::read_dir(store) else {
        return;
    };
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some(id) = name
            .strip_suffix(".claimed")
            .filter(|id| valid_session_id(id))
        {
            let target = intent_path(store, id);
            if !target.exists() {
                let _ = fs::rename(entry.path(), target);
            }
        }
    }
}

pub fn read_bundle(store: &Path, i: &Intent) -> io::Result<Option<RunBundle>> {
    read_bundle_at(&bundle_path(store, &i.session), i)
}

fn read_bundle_at(path: &Path, i: &Intent) -> io::Result<Option<RunBundle>> {
    let Some(hash) = &i.bundle_sha256 else {
        return Ok(None);
    };
    let bytes = read_capped(path, MAX_BUNDLE as u64)?;
    let bundle: RunBundle = serde_json::from_slice(&bytes).map_err(|e| bad(e.to_string()))?;
    if !sha256_hex(bundle.canonical().as_bytes()).eq_ignore_ascii_case(hash) {
        return Err(bad("the stored task bundle does not match its hash"));
    }
    Ok(Some(bundle))
}

pub fn save_relaunch(sdir: &Path, i: &Intent, bundle: Option<&str>) -> io::Result<()> {
    check_id(&i.session)?;
    if let Some(b) = bundle {
        write_atomic(&sdir.join(RELAUNCH_BUNDLE), b.as_bytes())?;
    }
    let bytes = serde_json::to_vec(i).map_err(io::Error::other)?;
    write_atomic(&sdir.join(RELAUNCH), &bytes)
}

pub fn load_relaunch(sdir: &Path, id: &str) -> io::Result<(Intent, Option<RunBundle>)> {
    check_id(id)?;
    let intent = parse_intent(&read_capped(&sdir.join(RELAUNCH), MAX_INTENT)?, id)?;
    let bundle = read_bundle_at(&sdir.join(RELAUNCH_BUNDLE), &intent)?;
    Ok((intent, bundle))
}

pub fn is_claimed(store: &Path, id: &str) -> bool {
    fs::symlink_metadata(claimed_path(store, id)).is_ok()
}

pub fn request(i: &Intent, bundle: Option<RunBundle>) -> RunCreateReq {
    RunCreateReq {
        clones: Vec::new(),
        profile: i.profile.clone(),
        task: i.task,
        cwd: i.cwd.clone(),
        name: Some(i.name.clone()),
        model: i.model.clone(),
        cols: i.cols,
        rows: i.rows,
        bundle,
        bundle_sha256: i.bundle_sha256.clone(),
        cwd_pin: Some(i.cwd_pin),
        origin: None,
        permission_mode: i.permission_mode.clone(),
        start_key: None,
        clone_of: i.clone_of.clone(),
        trust_cwd: None,
        trust_via_repo: None,
        agent_work: i.agent_work,
        cloud: None,
        unattended: false,
        exe_pin: i.exe_pin.clone(),
        argv_sha256: i.argv_sha256.clone(),
    }
}

pub struct Claim {
    store: PathBuf,
    pub intent: Intent,
}

pub fn claim(store: &Path, id: &str) -> io::Result<Claim> {
    check_id(id)?;
    let from = intent_path(store, id);
    let to = claimed_path(store, id);
    if fs::symlink_metadata(&to).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "a restore of this session is already in progress",
        ));
    }
    fs::rename(&from, &to).map_err(|e| match e.kind() {
        io::ErrorKind::NotFound => io::Error::new(
            io::ErrorKind::NotFound,
            "no restorable session with this id",
        ),
        _ => e,
    })?;
    match read_capped(&to, MAX_INTENT).and_then(|b| parse_intent(&b, id)) {
        Ok(intent) => Ok(Claim {
            store: store.to_path_buf(),
            intent,
        }),
        Err(e) => {
            let _ = fs::rename(&to, &from);
            Err(e)
        }
    }
}

impl Claim {
    pub fn release(self) {
        let id = &self.intent.session;
        let _ = fs::rename(claimed_path(&self.store, id), intent_path(&self.store, id));
    }

    pub fn keep(self) {}

    pub fn finish(self) {
        let id = &self.intent.session;
        let _ = fs::remove_file(claimed_path(&self.store, id));
        let _ = fs::remove_file(bundle_path(&self.store, id));
    }
}

pub fn adopt_home(homes: &Path, old: &str, new: &str) -> io::Result<bool> {
    check_id(old)?;
    check_id(new)?;
    let from = homes.join(old);
    match fs::symlink_metadata(&from) {
        Ok(m) if m.is_dir() => {}
        Ok(_) => return Err(bad("the previous run home is not a directory")),
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e),
    }
    let to = homes.join(new);
    if fs::symlink_metadata(&to).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "the new run home already exists",
        ));
    }
    fs::rename(&from, &to)?;
    Ok(true)
}

fn open_dir_at(dirfd: i32, name: &std::ffi::CStr) -> io::Result<fs::File> {
    use std::os::fd::FromRawFd;
    let fd = unsafe {
        libc::openat(
            dirfd,
            name.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { fs::File::from_raw_fd(fd) })
}

fn kind_at(dirfd: i32, name: &std::ffi::CStr) -> Option<libc::mode_t> {
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::fstatat(dirfd, name.as_ptr(), &mut st, libc::AT_SYMLINK_NOFOLLOW) };
    (rc == 0).then_some(st.st_mode & libc::S_IFMT)
}

// A reused home is the old agent's: a seed destination or any parent of it may now be a file or a symlink.
pub fn clear_seed_destinations(home: &Path, tos: &[String]) -> io::Result<Vec<String>> {
    use std::ffi::CString;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Component;
    let mut removed = Vec::new();
    let root = open_dir_at(
        libc::AT_FDCWD,
        &CString::new(home.as_os_str().as_bytes()).map_err(|_| bad("home path has a NUL"))?,
    )?;
    'next: for to in tos {
        let parts: Vec<&std::ffi::OsStr> = match Path::new(to)
            .components()
            .map(|c| match c {
                Component::Normal(n) => Ok(n),
                _ => Err(()),
            })
            .collect::<Result<Vec<_>, ()>>()
        {
            Ok(p) if !p.is_empty() => p,
            _ => continue,
        };
        let mut dir = root.try_clone()?;
        for (i, part) in parts.iter().enumerate() {
            let name = CString::new(part.as_bytes()).map_err(|_| bad("seed path has a NUL"))?;
            let last = i + 1 == parts.len();
            match kind_at(dir.as_raw_fd(), &name) {
                None => continue 'next,
                Some(libc::S_IFDIR) if !last => {
                    dir = open_dir_at(dir.as_raw_fd(), &name)?;
                }
                Some(libc::S_IFREG) | Some(libc::S_IFLNK) => {
                    if unsafe { libc::unlinkat(dir.as_raw_fd(), name.as_ptr(), 0) } != 0 {
                        return Err(io::Error::last_os_error());
                    }
                    removed.push(
                        parts[..=i]
                            .iter()
                            .map(|p| p.to_string_lossy())
                            .collect::<Vec<_>>()
                            .join("/"),
                    );
                    continue 'next;
                }
                Some(_) => {
                    return Err(bad(format!(
                        "seed destination {to} is blocked by a directory or special file"
                    )))
                }
            }
        }
    }
    Ok(removed)
}

pub fn return_home(homes: &Path, new: &str, old: &str) -> bool {
    if check_id(old).is_err() || check_id(new).is_err() {
        return false;
    }
    let to = homes.join(old);
    fs::symlink_metadata(&to).is_err() && fs::rename(homes.join(new), to).is_ok()
}

pub fn staging_complete(sdir: &Path) -> bool {
    !sdir.join("seeds.json").exists() || sdir.join("seeds.done").exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const A: &str = "s_aaaaaaaaaa";
    const B: &str = "s_bbbbbbbbbb";

    fn intent(id: &str) -> Intent {
        Intent {
            v: 1,
            session: id.into(),
            profile: "codex".into(),
            model: Some("m1".into()),
            name: "codex".into(),
            cwd: "/work/project".into(),
            cwd_pin: CwdPin { dev: 1, ino: 2 },
            task: Some(TaskRef {
                workspace_id: -3,
                task_id: 7,
            }),
            bundle_sha256: None,
            origin: None,
            permission_mode: None,
            clone_of: None,
            agent_work: false,
            keyed: None,
            resume_decl: None,
            exe_pin: None,
            argv_sha256: None,
            bypass: false,
            boot: None,
            pids: Vec::new(),
            created_ms: 1_000,
            cols: 100,
            rows: 30,
        }
    }

    fn dirs() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let homes = t.path().join("homes");
        fs::create_dir(&homes).unwrap();
        fs::set_permissions(&homes, fs::Permissions::from_mode(0o700)).unwrap();
        let st = store(&homes);
        (t, homes, st)
    }

    fn exit(id: &str, code: Option<i32>, detail: Option<&str>) -> EventMsg {
        EventMsg {
            session: id.into(),
            state: State {
                kind: StateKind::Exited,
                since_ms: 1,
                detail: detail.map(String::from),
                exit_code: code,
            },
            task: None,
            origin: None,
        }
    }

    #[test]
    fn recorded_intent_is_private_and_lists_as_restorable() {
        let (_t, homes, st) = dirs();
        record(&st, &intent(A), None).unwrap();
        let mode = fs::metadata(&st).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        let fmode = fs::metadata(st.join(format!("{A}.json")))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(fmode, 0o600);
        let l = list(&st, &homes, 2_000, &|_| false);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].status, Status::Restorable);
        assert!(!l[0].resumable);
        fs::create_dir(homes.join(A)).unwrap();
        assert!(list(&st, &homes, 2_000, &|_| false)[0].resumable);
    }

    #[test]
    fn status_rules() {
        let i = intent(A);
        assert_eq!(status(&i, 2_000, true), Status::Running);
        assert_eq!(status(&i, 1_000 + TTL_MS + 1, false), Status::Expired);
        let mut l = i.clone();
        l.origin = Some(Origin {
            link: "laptop".into(),
            generation: 1,
        });
        assert_eq!(status(&l, 2_000, false), Status::Link);
        let mut b = i.clone();
        b.bypass = true;
        assert_eq!(status(&b, 2_000, false), Status::Bypass);
        assert_eq!(status(&i, 2_000, false), Status::Restorable);
    }

    #[test]
    fn live_process_from_the_same_boot_counts_as_running() {
        let mut i = intent(A);
        i.pids = vec![std::process::id()];
        i.boot = Some("b1".into());
        assert!(may_still_run(&i, Some("b1")));
        assert!(!may_still_run(&i, Some("b2")));
        let mut child = std::process::Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .spawn()
            .unwrap();
        let dead = child.id();
        child.wait().unwrap();
        i.pids = vec![dead];
        assert!(!may_still_run(&i, Some("b1")));
        assert!(boot_marker().is_some());
    }

    #[test]
    fn a_surviving_process_group_member_counts_as_running() {
        use std::os::unix::process::CommandExt;
        let mut leader = std::process::Command::new("/bin/sh")
            .args(["-c", "sleep 30 & exit 0"])
            .process_group(0)
            .spawn()
            .unwrap();
        let pid = leader.id();
        leader.wait().unwrap();
        let mut i = intent(A);
        i.pids = vec![pid];
        assert!(may_still_run(&i, None));
        unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while may_still_run(&i, None) {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn clean_exit_forgets_but_interruption_and_failure_keep() {
        let (_t, homes, st) = dirs();
        record(&st, &intent(A), None).unwrap();
        on_event(&st, &exit(A, None, Some("interrupted")), Duration::ZERO);
        on_event(&st, &exit(A, Some(1), None), Duration::ZERO);
        on_event(&st, &exit(A, None, Some("signal 15")), Duration::ZERO);
        assert_eq!(list(&st, &homes, 2_000, &|_| false).len(), 1);
        on_event(&st, &exit(A, Some(0), None), Duration::ZERO);
        assert!(list(&st, &homes, 2_000, &|_| false).is_empty());
    }

    #[test]
    fn claim_is_exclusive_and_release_restores() {
        let (_t, homes, st) = dirs();
        record(&st, &intent(A), None).unwrap();
        let c = claim(&st, A).unwrap();
        assert_eq!(c.intent, intent(A));
        assert!(claim(&st, A).is_err());
        let l = list(&st, &homes, 2_000, &|_| false);
        assert_eq!(l[0].status, Status::Restoring);
        c.release();
        let c = claim(&st, A).unwrap();
        c.finish();
        assert!(list(&st, &homes, 2_000, &|_| false).is_empty());
        assert_eq!(claim(&st, A).err().unwrap().kind(), io::ErrorKind::NotFound);
    }

    #[test]
    fn stale_claims_return_at_start() {
        let (_t, homes, st) = dirs();
        record(&st, &intent(A), None).unwrap();
        let c = claim(&st, A).unwrap();
        std::mem::forget(c);
        recover_claims(&st);
        assert_eq!(
            list(&st, &homes, 2_000, &|_| false)[0].status,
            Status::Restorable
        );
    }

    #[test]
    fn mismatched_or_foreign_files_are_ignored() {
        let (_t, homes, st) = dirs();
        record(&st, &intent(A), None).unwrap();
        fs::copy(st.join(format!("{A}.json")), st.join(format!("{B}.json"))).unwrap();
        fs::write(st.join("junk.json"), b"{}").unwrap();
        fs::write(st.join("s_cccccccccc.json"), b"not json").unwrap();
        let ids: Vec<_> = list(&st, &homes, 2_000, &|_| false)
            .into_iter()
            .map(|e| e.intent.session)
            .collect();
        assert_eq!(ids, vec![A]);
        assert!(load(&st, B).is_err());
        assert!(load(&st, "../x").is_err());
    }

    #[test]
    fn symlinked_intent_is_not_followed() {
        let (t, homes, st) = dirs();
        record(&st, &intent(B), None).unwrap();
        let elsewhere = t.path().join("elsewhere.json");
        fs::write(&elsewhere, serde_json::to_vec(&intent(A)).unwrap()).unwrap();
        std::os::unix::fs::symlink(&elsewhere, st.join(format!("{A}.json"))).unwrap();
        let ids: Vec<_> = list(&st, &homes, 2_000, &|_| false)
            .into_iter()
            .map(|e| e.intent.session)
            .collect();
        assert_eq!(ids, vec![B]);
        assert!(claim(&st, A).is_err());
    }

    fn bundle() -> RunBundle {
        serde_json::from_str(include_str!(
            "../../tmgr-ipc/tests/fixtures/run_bundle.json"
        ))
        .unwrap()
    }

    #[test]
    fn bundle_round_trip_and_tamper_detection() {
        let (_t, _homes, st) = dirs();
        let b = bundle();
        let canonical = b.canonical();
        let mut i = intent(A);
        i.bundle_sha256 = Some(sha256_hex(canonical.as_bytes()));
        record(&st, &i, Some(&canonical)).unwrap();
        assert_eq!(read_bundle(&st, &i).unwrap(), Some(b.clone()));
        let mut changed = b;
        changed.body.push('!');
        fs::write(st.join(format!("{A}.bundle.json")), changed.canonical()).unwrap();
        assert!(read_bundle(&st, &i).is_err());
        assert_eq!(read_bundle(&st, &intent(B)).unwrap(), None);
    }

    #[test]
    fn relaunch_record_lives_in_the_session_dir_and_checks_its_bundle() {
        let (t, _homes, st) = dirs();
        let sdir = t.path().join("sdir");
        fs::create_dir(&sdir).unwrap();
        let b = bundle();
        let canonical = b.canonical();
        let mut i = intent(A);
        i.bundle_sha256 = Some(sha256_hex(canonical.as_bytes()));
        save_relaunch(&sdir, &i, Some(&canonical)).unwrap();
        for f in [RELAUNCH, RELAUNCH_BUNDLE] {
            let mode = fs::metadata(sdir.join(f)).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "{f}");
        }
        assert_eq!(
            load_relaunch(&sdir, A).unwrap(),
            (i.clone(), Some(b.clone()))
        );
        assert!(load_relaunch(&sdir, B).is_err());
        assert!(load_relaunch(&sdir, "../x").is_err());
        assert!(!st.exists());
        let mut changed = b;
        changed.body.push('!');
        fs::write(sdir.join(RELAUNCH_BUNDLE), changed.canonical()).unwrap();
        assert!(load_relaunch(&sdir, A).is_err());
        fs::remove_file(sdir.join(RELAUNCH_BUNDLE)).unwrap();
        assert!(load_relaunch(&sdir, A).is_err());
        save_relaunch(&sdir, &intent(A), None).unwrap();
        assert_eq!(load_relaunch(&sdir, A).unwrap().1, None);
    }

    #[test]
    fn a_symlinked_relaunch_record_is_not_read() {
        let (t, _homes, _st) = dirs();
        let sdir = t.path().join("sdir");
        fs::create_dir(&sdir).unwrap();
        save_relaunch(&sdir, &intent(A), None).unwrap();
        fs::rename(sdir.join(RELAUNCH), t.path().join("elsewhere")).unwrap();
        std::os::unix::fs::symlink(t.path().join("elsewhere"), sdir.join(RELAUNCH)).unwrap();
        assert!(load_relaunch(&sdir, A).is_err());
    }

    #[test]
    fn claimed_records_are_detected() {
        let (_t, _homes, st) = dirs();
        record(&st, &intent(A), None).unwrap();
        assert!(!is_claimed(&st, A));
        let c = claim(&st, A).unwrap();
        assert!(is_claimed(&st, A));
        c.release();
        assert!(!is_claimed(&st, A));
    }

    #[test]
    fn request_rebuilds_the_recorded_launch() {
        let i = intent(A);
        let req = request(&i, None);
        assert_eq!(req.profile, "codex");
        assert_eq!(req.model.as_deref(), Some("m1"));
        assert_eq!(req.cwd, "/work/project");
        assert_eq!(req.cwd_pin, Some(CwdPin { dev: 1, ino: 2 }));
        assert_eq!(req.task, i.task);
        assert_eq!(req.origin, None);
        assert_eq!((req.cols, req.rows), (100, 30));
    }

    #[test]
    fn home_moves_to_the_new_session_and_back() {
        let (_t, homes, _st) = dirs();
        assert!(!adopt_home(&homes, A, B).unwrap());
        fs::create_dir(homes.join(A)).unwrap();
        fs::write(homes.join(A).join("history"), b"x").unwrap();
        assert!(adopt_home(&homes, A, B).unwrap());
        assert!(homes.join(B).join("history").exists());
        assert!(!homes.join(A).exists());
        assert!(return_home(&homes, B, A));
        assert!(homes.join(A).join("history").exists());
        assert!(!return_home(&homes, B, A));
        fs::create_dir(homes.join(B)).unwrap();
        assert!(adopt_home(&homes, A, B).is_err());
        assert!(adopt_home(&homes, "../x", B).is_err());
    }

    #[test]
    fn seed_destinations_are_cleared_without_following_links() {
        let (t, homes, _st) = dirs();
        let home = homes.join(A);
        let outside = t.path().join("outside");
        fs::create_dir_all(home.join(".codex")).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(outside.join("auth.json"), b"keep").unwrap();
        fs::write(home.join(".codex/auth.json"), b"old").unwrap();
        fs::write(home.join(".codex/history"), b"keep").unwrap();
        std::os::unix::fs::symlink(&outside, home.join(".claude")).unwrap();
        fs::write(home.join(".x"), b"file where a dir is expected").unwrap();
        let tos: Vec<String> = [
            ".codex/auth.json",
            ".claude/.credentials.json",
            ".x/y.json",
            ".missing/z.json",
            "../escape",
        ]
        .map(String::from)
        .to_vec();
        let mut removed = clear_seed_destinations(&home, &tos).unwrap();
        removed.sort();
        assert_eq!(removed, vec![".claude", ".codex/auth.json", ".x"]);
        assert!(!home.join(".codex/auth.json").exists());
        assert!(home.join(".codex/history").exists());
        assert_eq!(fs::read(outside.join("auth.json")).unwrap(), b"keep");
        fs::create_dir_all(home.join(".d/auth.json")).unwrap();
        assert!(clear_seed_destinations(&home, &[".d/auth.json".into()]).is_err());
    }

    #[test]
    fn symlinked_home_is_refused() {
        let (t, homes, _st) = dirs();
        std::os::unix::fs::symlink(t.path(), homes.join(A)).unwrap();
        assert!(adopt_home(&homes, A, B).is_err());
    }
}
