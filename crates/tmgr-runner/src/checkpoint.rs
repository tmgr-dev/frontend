use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{CString, OsString};
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, BufWriter, Read, Write};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Output, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tmgr_ipc::api::{
    CheckpointDiff, CheckpointItem, CheckpointKind, CheckpointRestored, CheckpointStatus,
    DiffAgainst,
};
use tmgr_ipc::proto::valid_session_id;

use crate::wire::write_atomic;
use crate::worktree::{run_bounded, GitEnv};

pub const DEFAULT_KEEP: u32 = 50;

const KEEP_MIN: u32 = 3;
const KEEP_MAX: u32 = 1000;
const MAX_FILE: u64 = 100 * 1024 * 1024;
const MAX_TOTAL: u64 = 2 * 1024 * 1024 * 1024;
const FRESH_SECS: i64 = 2;
const DIFF_TEXT_CAP: usize = 1024 * 1024;
const DIFF_RAW_CAP: usize = 16 * 1024 * 1024;
const LIST_CAP: usize = 256 * 1024 * 1024;
const ERR_CAP: usize = 1024 * 1024;
const LINK_CAP: usize = 4096;
const GIT_DEADLINE: Duration = Duration::from_secs(600);
const FACTS_DEADLINE: Duration = Duration::from_secs(5);
const STREAM_DEADLINE: Duration = Duration::from_secs(1800);
const REF_ROOT: &str = "refs/tmgr/checkpoints";
const MANIFEST: &str = "checkpoints.json";
const CACHE: &str = "checkpoints.cache.json";
const NO_EXCLUDES: &str = "/dev/null";
const MODE_FILE: u32 = 0o100644;
const MODE_EXEC: u32 = 0o100755;
const MODE_LINK: u32 = 0o120000;

#[derive(Debug, Clone)]
pub struct Store {
    pub session: String,
    pub sdir: PathBuf,
    pub cwd: PathBuf,
    pub keep: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CpError {
    NotGit(String),
    Refused(String),
    Failed(String),
}

fn failed(what: &str, e: impl std::fmt::Display) -> CpError {
    CpError::Failed(format!("{what}: {e}"))
}

#[derive(Serialize, Deserialize, Clone)]
struct Entry {
    item: CheckpointItem,
    commit: String,
    tree: String,
}

#[derive(Serialize, Deserialize, Default)]
struct Manifest {
    #[serde(default)]
    next_n: u64,
    #[serde(default)]
    turn: u64,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    common_dir: Option<String>,
    #[serde(default)]
    items: Vec<Entry>,
}

struct Env<'a> {
    home: Option<OsString>,
    xdg: Option<OsString>,
    max_file: u64,
    max_total: u64,
    after_before: Option<&'a dyn Fn()>,
}

impl Env<'static> {
    fn real() -> Env<'static> {
        Env {
            home: std::env::var_os("HOME"),
            xdg: std::env::var_os("XDG_CONFIG_HOME"),
            max_file: MAX_FILE,
            max_total: MAX_TOTAL,
            after_before: None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
struct Fp {
    dev: u64,
    ino: u64,
    size: u64,
    mt_s: i64,
    mt_ns: i64,
    ct_s: i64,
    ct_ns: i64,
    mode: u32,
}

#[derive(Serialize, Deserialize, Default)]
struct Cache {
    files: BTreeMap<String, CacheEntry>,
}

#[derive(Serialize, Deserialize)]
struct CacheEntry {
    fp: Fp,
    oid: String,
}

struct Meta {
    mode: u32,
    oid: String,
    fp: Fp,
}

struct Captured {
    tree: String,
    entries: BTreeMap<Vec<u8>, Meta>,
    skipped: u32,
}

struct Repo {
    common_dir: String,
}

struct TreeEntry {
    mode: u32,
    ty: String,
    oid: String,
    path: Vec<u8>,
}

#[derive(Default)]
struct Report {
    written: u32,
    deleted: u32,
    left: BTreeSet<String>,
}

enum Outcome {
    Written,
    Deleted,
    Same,
    Left,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn rand_hex() -> String {
    let mut b = [0u8; 8];
    getrandom::fill(&mut b).expect("system random source");
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn kind_str(k: CheckpointKind) -> &'static str {
    match k {
        CheckpointKind::Start => "start",
        CheckpointKind::Turn => "turn",
        CheckpointKind::BeforeRestore => "before_restore",
        CheckpointKind::Restored => "restored",
    }
}

fn valid_oid(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_dot_git(comp: &[u8]) -> bool {
    let text = String::from_utf8_lossy(comp);
    let stripped: String = text
        .chars()
        .filter(|c| {
            !matches!(
                c,
                '\u{200c}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{206a}'..='\u{206f}' | '\u{feff}'
            )
        })
        .collect();
    let name = stripped.trim_end_matches([' ', '.']).to_ascii_lowercase();
    name == ".git" || name == "git~1"
}

fn valid_comp(c: &[u8]) -> bool {
    !c.is_empty() && c != b"." && c != b".." && !c.contains(&0) && !is_dot_git(c)
}

fn valid_path(p: &[u8]) -> bool {
    !p.is_empty() && p.split(|b| *b == b'/').all(valid_comp)
}

fn split_path(p: &[u8]) -> (Vec<&[u8]>, &[u8]) {
    let mut comps: Vec<&[u8]> = p.split(|b| *b == b'/').collect();
    let name = comps.pop().unwrap_or(&[]);
    (comps, name)
}

fn sanitize(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || matches!(c, '\t' | '\n'))
        .collect()
}

fn cstr(b: &[u8]) -> io::Result<CString> {
    CString::new(b).map_err(|_| io::Error::from_raw_os_error(libc::EINVAL))
}

fn cvt(r: libc::c_int) -> io::Result<libc::c_int> {
    if r < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(r)
    }
}

fn owned(fd: libc::c_int) -> OwnedFd {
    unsafe { OwnedFd::from_raw_fd(fd) }
}

fn open_root(path: &Path) -> io::Result<OwnedFd> {
    let p = cstr(path.as_os_str().as_bytes())?;
    let fd = cvt(unsafe {
        libc::open(
            p.as_ptr(),
            libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC,
        )
    })?;
    Ok(owned(fd))
}

fn open_at(dir: BorrowedFd, name: &[u8], flags: libc::c_int, mode: u32) -> io::Result<OwnedFd> {
    let p = cstr(name)?;
    let fd = cvt(unsafe {
        libc::openat(
            dir.as_raw_fd(),
            p.as_ptr(),
            flags | libc::O_CLOEXEC,
            mode as libc::c_uint,
        )
    })?;
    Ok(owned(fd))
}

fn open_dir_at(dir: BorrowedFd, name: &[u8]) -> io::Result<OwnedFd> {
    open_at(
        dir,
        name,
        libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW,
        0,
    )
}

fn stat_at(dir: BorrowedFd, name: &[u8]) -> io::Result<libc::stat> {
    let p = cstr(name)?;
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    cvt(unsafe {
        libc::fstatat(
            dir.as_raw_fd(),
            p.as_ptr(),
            &mut st,
            libc::AT_SYMLINK_NOFOLLOW,
        )
    })?;
    Ok(st)
}

fn fstat_fd(fd: BorrowedFd) -> io::Result<libc::stat> {
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    cvt(unsafe { libc::fstat(fd.as_raw_fd(), &mut st) })?;
    Ok(st)
}

fn unlink_at(dir: BorrowedFd, name: &[u8], flags: libc::c_int) -> io::Result<()> {
    let p = cstr(name)?;
    cvt(unsafe { libc::unlinkat(dir.as_raw_fd(), p.as_ptr(), flags) }).map(drop)
}

fn mkdir_at(dir: BorrowedFd, name: &[u8]) -> io::Result<()> {
    let p = cstr(name)?;
    cvt(unsafe { libc::mkdirat(dir.as_raw_fd(), p.as_ptr(), 0o755) }).map(drop)
}

fn rename_at(dir: BorrowedFd, from: &[u8], to: &[u8]) -> io::Result<()> {
    let (f, t) = (cstr(from)?, cstr(to)?);
    cvt(unsafe { libc::renameat(dir.as_raw_fd(), f.as_ptr(), dir.as_raw_fd(), t.as_ptr()) })
        .map(drop)
}

fn symlink_at(target: &[u8], dir: BorrowedFd, name: &[u8]) -> io::Result<()> {
    let (t, n) = (cstr(target)?, cstr(name)?);
    cvt(unsafe { libc::symlinkat(t.as_ptr(), dir.as_raw_fd(), n.as_ptr()) }).map(drop)
}

fn read_link_at(dir: BorrowedFd, name: &[u8]) -> io::Result<Vec<u8>> {
    let p = cstr(name)?;
    let mut buf = vec![0u8; LINK_CAP + 1];
    let n = unsafe {
        libc::readlinkat(
            dir.as_raw_fd(),
            p.as_ptr(),
            buf.as_mut_ptr().cast(),
            buf.len(),
        )
    };
    if n < 0 {
        return Err(io::Error::last_os_error());
    }
    buf.truncate(n as usize);
    if buf.len() > LINK_CAP {
        return Err(io::Error::from_raw_os_error(libc::ENAMETOOLONG));
    }
    Ok(buf)
}

fn walk_dirs(root: BorrowedFd, comps: &[&[u8]]) -> io::Result<OwnedFd> {
    let mut cur = root.try_clone_to_owned()?;
    for c in comps {
        cur = open_dir_at(cur.as_fd(), c)?;
    }
    Ok(cur)
}

#[allow(clippy::unnecessary_cast)]
fn mode_bits(st: &libc::stat) -> u32 {
    st.st_mode as u32
}

#[allow(clippy::unnecessary_cast)]
fn is_kind(st: &libc::stat, kind: libc::mode_t) -> bool {
    mode_bits(st) & libc::S_IFMT as u32 == kind as u32
}

#[allow(clippy::unnecessary_cast)]
fn fp_of(st: &libc::stat) -> Fp {
    Fp {
        dev: st.st_dev as u64,
        ino: st.st_ino as u64,
        size: st.st_size as u64,
        mt_s: st.st_mtime as i64,
        mt_ns: st.st_mtime_nsec as i64,
        ct_s: st.st_ctime as i64,
        ct_ns: st.st_ctime_nsec as i64,
        mode: mode_bits(st),
    }
}

fn blob_mode(st: &libc::stat) -> u32 {
    if mode_bits(st) & 0o100 != 0 {
        MODE_EXEC
    } else {
        MODE_FILE
    }
}

enum ReadFile {
    Gone,
    Skip,
    Data(Fp, u32, Vec<u8>),
}

fn read_regular(dir: BorrowedFd, name: &[u8], max: u64) -> ReadFile {
    let flags = libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK;
    let fd = match open_at(dir, name, flags, 0) {
        Ok(fd) => fd,
        Err(e) => {
            return match e.raw_os_error() {
                Some(libc::ENOENT | libc::ELOOP | libc::ENOTDIR) => ReadFile::Gone,
                _ => ReadFile::Skip,
            }
        }
    };
    let Ok(st) = fstat_fd(fd.as_fd()) else {
        return ReadFile::Skip;
    };
    if !is_kind(&st, libc::S_IFREG) {
        return ReadFile::Gone;
    }
    let size = st.st_size as u64;
    if st.st_nlink > 1 || size > max {
        return ReadFile::Skip;
    }
    let mut data = Vec::with_capacity(size as usize);
    if File::from(fd).take(size).read_to_end(&mut data).is_err() || data.len() as u64 != size {
        return ReadFile::Skip;
    }
    ReadFile::Data(fp_of(&st), blob_mode(&st), data)
}

struct Proc {
    child: Child,
    reaped: Arc<Mutex<bool>>,
    pgid: libc::pid_t,
}

impl Proc {
    fn spawn(mut cmd: Command) -> Result<Proc, CpError> {
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        unsafe {
            cmd.pre_exec(|| {
                if libc::setpgid(0, 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = cmd.spawn().map_err(|e| failed("cannot spawn git", e))?;
        let pgid = child.id() as libc::pid_t;
        let reaped = Arc::new(Mutex::new(false));
        let watch = reaped.clone();
        thread::spawn(move || {
            let start = Instant::now();
            loop {
                thread::sleep(Duration::from_millis(200));
                let done = watch.lock().unwrap_or_else(PoisonError::into_inner);
                if *done {
                    return;
                }
                if start.elapsed() >= STREAM_DEADLINE {
                    unsafe { libc::kill(-pgid, libc::SIGKILL) };
                    return;
                }
            }
        });
        Ok(Proc {
            child,
            reaped,
            pgid,
        })
    }

    fn wait(&mut self) -> Result<ExitStatus, CpError> {
        loop {
            {
                let mut done = self.reaped.lock().unwrap_or_else(PoisonError::into_inner);
                match self.child.try_wait() {
                    Ok(Some(status)) => {
                        *done = true;
                        return Ok(status);
                    }
                    Ok(None) => {}
                    Err(e) => return Err(failed("git wait", e)),
                }
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
}

impl Drop for Proc {
    fn drop(&mut self) {
        let mut done = self.reaped.lock().unwrap_or_else(PoisonError::into_inner);
        if !*done {
            unsafe { libc::kill(-self.pgid, libc::SIGKILL) };
            let _ = self.child.wait();
            *done = true;
        }
    }
}

fn drain_err(mut r: impl Read + Send + 'static) -> JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut v = Vec::new();
        let _ = (&mut r).take(ERR_CAP as u64).read_to_end(&mut v);
        let _ = io::copy(&mut r, &mut io::sink());
        v
    })
}

struct Importer {
    proc: Proc,
    stdin: BufWriter<ChildStdin>,
    oids: JoinHandle<Vec<String>>,
    err: JoinHandle<Vec<u8>>,
    marks: usize,
}

impl Importer {
    fn start(g: &GitEnv) -> Result<Importer, CpError> {
        let mut cmd = g.command();
        cmd.args(["fast-import", "--quiet"]);
        let mut proc = Proc::spawn(cmd)?;
        let (Some(stdin), Some(stdout), Some(stderr)) = (
            proc.child.stdin.take(),
            proc.child.stdout.take(),
            proc.child.stderr.take(),
        ) else {
            return Err(CpError::Failed("fast-import pipes missing".into()));
        };
        let oids = thread::spawn(move || {
            BufReader::new(stdout)
                .lines()
                .map_while(Result::ok)
                .collect()
        });
        Ok(Importer {
            proc,
            stdin: BufWriter::new(stdin),
            oids,
            err: drain_err(stderr),
            marks: 0,
        })
    }

    fn add(&mut self, data: &[u8]) -> Result<usize, CpError> {
        self.marks += 1;
        let header = format!("blob\nmark :{}\ndata {}\n", self.marks, data.len());
        self.stdin
            .write_all(header.as_bytes())
            .and_then(|()| self.stdin.write_all(data))
            .and_then(|()| self.stdin.write_all(b"\n"))
            .map_err(|e| failed("git fast-import", e))?;
        Ok(self.marks)
    }

    fn finish(self) -> Result<Vec<String>, CpError> {
        let Importer {
            mut proc,
            mut stdin,
            oids,
            err,
            marks,
        } = self;
        for i in 1..=marks {
            writeln!(stdin, "get-mark :{i}").map_err(|e| failed("git fast-import", e))?;
        }
        stdin
            .into_inner()
            .map_err(|e| failed("git fast-import", e.error()))?;
        let oids = oids.join().unwrap_or_default();
        let err = err.join().unwrap_or_default();
        let status = proc.wait()?;
        if !status.success() || oids.len() != marks || !oids.iter().all(|o| valid_oid(o)) {
            return Err(CpError::Failed(format!(
                "git fast-import failed: {}",
                String::from_utf8_lossy(&err).trim()
            )));
        }
        Ok(oids)
    }
}

struct Blobs {
    _proc: Proc,
    stdin: ChildStdin,
    out: BufReader<ChildStdout>,
    _err: JoinHandle<Vec<u8>>,
}

impl Blobs {
    fn open(g: &GitEnv) -> Result<Blobs, CpError> {
        let mut cmd = g.command();
        cmd.args(["cat-file", "--batch"]);
        let mut proc = Proc::spawn(cmd)?;
        let (Some(stdin), Some(stdout), Some(stderr)) = (
            proc.child.stdin.take(),
            proc.child.stdout.take(),
            proc.child.stderr.take(),
        ) else {
            return Err(CpError::Failed("cat-file pipes missing".into()));
        };
        Ok(Blobs {
            _proc: proc,
            stdin,
            out: BufReader::new(stdout),
            _err: drain_err(stderr),
        })
    }

    fn fetch(
        &mut self,
        oid: &str,
        sink: &mut dyn FnMut(&[u8]) -> io::Result<()>,
    ) -> Result<Result<(), String>, CpError> {
        if !valid_oid(oid) {
            return Ok(Err("bad object id".into()));
        }
        let proto = |e: io::Error| failed("git cat-file", e);
        writeln!(self.stdin, "{oid}")
            .and_then(|()| self.stdin.flush())
            .map_err(proto)?;
        let mut header = String::new();
        if self.out.read_line(&mut header).map_err(proto)? == 0 {
            return Err(CpError::Failed("git cat-file closed unexpectedly".into()));
        }
        let parts: Vec<&str> = header.trim_end().split(' ').collect();
        if parts.first() != Some(&oid) {
            return Err(CpError::Failed("unexpected git cat-file output".into()));
        }
        if parts.get(1) == Some(&"missing") {
            return Ok(Err("object is missing".into()));
        }
        let (Some(ty), Some(Ok(size))) = (parts.get(1), parts.get(2).map(|s| s.parse::<u64>()))
        else {
            return Err(CpError::Failed("unexpected git cat-file output".into()));
        };
        let is_blob = *ty == "blob";
        let mut left = size;
        let mut sink_err = None;
        let mut buf = vec![0u8; 64 * 1024];
        while left > 0 {
            let want = buf.len().min(left as usize);
            let n = self.out.read(&mut buf[..want]).map_err(proto)?;
            if n == 0 {
                return Err(CpError::Failed("git cat-file closed unexpectedly".into()));
            }
            left -= n as u64;
            if is_blob && sink_err.is_none() {
                if let Err(e) = sink(&buf[..n]) {
                    sink_err = Some(e.to_string());
                }
            }
        }
        let mut nl = [0u8; 1];
        self.out.read_exact(&mut nl).map_err(proto)?;
        if !is_blob {
            return Ok(Err("object is not a blob".into()));
        }
        Ok(sink_err.map_or(Ok(()), Err))
    }
}

fn base_git(dir: &Path, excludes: &str) -> GitEnv {
    let cfg = [
        ("protocol.allow", "never"),
        ("core.logAllRefUpdates", "false"),
        ("core.splitIndex", "false"),
        ("gc.auto", "0"),
        ("commit.gpgsign", "false"),
        ("core.excludesFile", excludes),
    ];
    let mut g = GitEnv::bare(dir)
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_CONFIG_COUNT", &cfg.len().to_string());
    for (i, (k, v)) in cfg.iter().enumerate() {
        g = g
            .env(&format!("GIT_CONFIG_KEY_{i}"), k)
            .env(&format!("GIT_CONFIG_VALUE_{i}"), v);
    }
    g
}

fn exec(g: &GitEnv, args: &[&str], stdin: Option<&Path>, cap: usize) -> Result<Output, CpError> {
    let mut cmd = g.command();
    cmd.args(args);
    if let Some(p) = stdin {
        cmd.stdin(File::open(p).map_err(|e| failed("cannot open git input", e))?);
    }
    let name = args.first().copied().unwrap_or("");
    run_bounded(cmd, GIT_DEADLINE, cap, ERR_CAP).map_err(|e| failed(&format!("git {name}"), e))
}

fn out(g: &GitEnv, args: &[&str], stdin: Option<&Path>, cap: usize) -> Result<Vec<u8>, CpError> {
    let o = exec(g, args, stdin, cap)?;
    if !o.status.success() {
        return Err(CpError::Failed(format!(
            "git {} failed: {}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&o.stderr).trim()
        )));
    }
    Ok(o.stdout)
}

fn text(g: &GitEnv, args: &[&str]) -> Result<String, CpError> {
    Ok(String::from_utf8_lossy(&out(g, args, None, ERR_CAP)?)
        .trim()
        .to_string())
}

fn global_excludes(env: &Env) -> String {
    let mut cmd = Command::new("git");
    cmd.env_clear()
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .current_dir("/")
        .stdin(Stdio::null())
        .args(["config", "--global", "--path", "--get", "core.excludesFile"]);
    if let Some(p) = std::env::var_os("PATH") {
        cmd.env("PATH", p);
    }
    if let Some(h) = &env.home {
        cmd.env("HOME", h);
    }
    if let Some(x) = &env.xdg {
        cmd.env("XDG_CONFIG_HOME", x);
    }
    if let Ok(o) = run_bounded(cmd, FACTS_DEADLINE, 64 * 1024, 64 * 1024) {
        let path = String::from_utf8_lossy(&o.stdout).trim().to_string();
        if o.status.success() && Path::new(&path).is_absolute() {
            return path;
        }
    }
    let dir = match (&env.xdg, &env.home) {
        (Some(x), _) if !x.is_empty() => Path::new(x).to_path_buf(),
        (_, Some(h)) => Path::new(h).join(".config"),
        _ => return NO_EXCLUDES.to_string(),
    };
    dir.join("git/ignore").to_string_lossy().into_owned()
}

impl Store {
    fn manifest_path(&self) -> PathBuf {
        self.sdir.join(MANIFEST)
    }

    fn cache_path(&self) -> PathBuf {
        self.sdir.join(CACHE)
    }

    fn load(&self) -> Manifest {
        fs::read(self.manifest_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    fn save(&self, m: &Manifest) -> Result<(), CpError> {
        let bytes = serde_json::to_vec(m).map_err(|e| failed("cannot encode manifest", e))?;
        write_atomic(&self.manifest_path(), &bytes).map_err(|e| failed("cannot write manifest", e))
    }

    fn check_session(&self) -> Result<(), CpError> {
        if valid_session_id(&self.session) {
            Ok(())
        } else {
            Err(CpError::Failed("invalid session id".into()))
        }
    }

    fn ref_name(&self, n: u64) -> String {
        format!("{REF_ROOT}/{}/{n}", self.session)
    }

    fn keep_clamped(&self) -> usize {
        self.keep.clamp(KEEP_MIN, KEEP_MAX) as usize
    }

    fn snapshot_in(&self, env: &Env) -> Result<Option<CheckpointItem>, CpError> {
        self.check_session()?;
        let op = Op::new(self, env);
        let mut m = self.load();
        let repo = op.repo_or_mark(&mut m)?;
        let cap = op.capture(&[])?;
        if m.items.last().is_some_and(|e| e.tree == cap.tree) {
            if m.note.take().is_some() {
                self.save(&m)?;
            }
            return Ok(None);
        }
        let kind = if m.items.is_empty() {
            CheckpointKind::Start
        } else {
            CheckpointKind::Turn
        };
        op.record(&mut m, &repo, &cap, kind, None, &[]).map(Some)
    }

    fn restore_in(&self, env: &Env, n: u64) -> Result<CheckpointRestored, CpError> {
        self.check_session()?;
        let op = Op::new(self, env);
        let mut m = self.load();
        let Some(entry) = m.items.iter().find(|e| e.item.n == n).cloned() else {
            return Err(CpError::Refused("no such checkpoint".into()));
        };
        let repo = op.repo_or_mark(&mut m)?;
        op.verify_ref(n, &entry.commit)?;
        let target = op.read_target(&entry.commit)?;
        let before = op.capture(&op.collision_paths(&target))?;
        let before_n = op.record_before(&mut m, &repo, &before, n)?;
        if let Some(hook) = env.after_before {
            hook();
        }
        let mut rep = Report::default();
        op.apply(&target, &before, &mut rep)?;
        let after_n = op
            .capture(&[])
            .and_then(|after| {
                op.record(
                    &mut m,
                    &repo,
                    &after,
                    CheckpointKind::Restored,
                    Some(n),
                    &[n, before_n],
                )
            })
            .map_err(|e| {
                CpError::Failed(format!(
                    "files were restored, but the final checkpoint failed: {}",
                    match e {
                        CpError::NotGit(s) | CpError::Refused(s) | CpError::Failed(s) => s,
                    }
                ))
            })?
            .n;
        Ok(CheckpointRestored {
            n,
            before_n,
            after_n,
            written: rep.written,
            deleted: rep.deleted,
            left_as_is: rep.left.into_iter().collect(),
        })
    }

    fn diff_in(&self, env: &Env, n: u64, against: DiffAgainst) -> Result<CheckpointDiff, CpError> {
        self.check_session()?;
        let m = self.load();
        let Some(pos) = m.items.iter().position(|e| e.item.n == n) else {
            return Err(CpError::Refused("no such checkpoint".into()));
        };
        let op = Op::new(self, env);
        op.repo()?;
        let (a, b) = match against {
            DiffAgainst::Previous => {
                let prev = match pos.checked_sub(1) {
                    Some(i) => m.items[i].tree.clone(),
                    None => op.empty_tree()?,
                };
                (prev, m.items[pos].tree.clone())
            }
            DiffAgainst::Current => (m.items[pos].tree.clone(), op.capture(&[])?.tree),
        };
        op.diff(n, &a, &b)
    }

    pub fn snapshot(&self) -> Result<Option<CheckpointItem>, CpError> {
        self.snapshot_in(&Env::real())
    }

    pub fn list(&self) -> Vec<CheckpointItem> {
        self.load().items.into_iter().map(|e| e.item).collect()
    }

    pub fn status(&self) -> CheckpointStatus {
        let m = self.load();
        CheckpointStatus {
            available: m.note.is_none(),
            note: m.note,
            count: m.items.len() as u32,
        }
    }

    pub fn diff(&self, n: u64, against: DiffAgainst) -> Result<CheckpointDiff, CpError> {
        self.diff_in(&Env::real(), n, against)
    }

    pub fn restore(&self, n: u64) -> Result<CheckpointRestored, CpError> {
        self.restore_in(&Env::real(), n)
    }

    pub fn remove_all(&self) {
        if self.check_session().is_ok() {
            if let Some(dir) = self.load().common_dir {
                self.delete_refs(&dir);
            }
        }
        for name in [MANIFEST, CACHE] {
            let _ = fs::remove_file(self.sdir.join(name));
        }
        let Ok(entries) = fs::read_dir(&self.sdir) else {
            return;
        };
        for e in entries.flatten() {
            let name = e.file_name();
            let name = name.to_string_lossy();
            let leftover = name.starts_with("checkpoint.index.")
                || name.starts_with(".checkpoint.info.")
                || (name.starts_with(".checkpoints.") && name.ends_with(".tmp"));
            if leftover {
                let _ = fs::remove_file(e.path());
            }
        }
    }

    pub fn transfer(&self, to: &Store) -> Result<(), CpError> {
        self.check_session()?;
        to.check_session()?;
        let m = self.load();
        let Some(dir) = m.common_dir.clone() else {
            return Ok(());
        };
        fs::create_dir_all(&to.sdir).map_err(|e| failed("cannot create session directory", e))?;
        let g = base_git(&to.sdir, NO_EXCLUDES).env("GIT_DIR", &dir);
        for e in &m.items {
            text(&g, &["update-ref", &to.ref_name(e.item.n), &e.commit])?;
        }
        to.save(&m)?;
        if let Ok(bytes) = fs::read(self.cache_path()) {
            let _ = write_atomic(&to.cache_path(), &bytes);
        }
        self.remove_all();
        Ok(())
    }

    fn delete_refs(&self, common_dir: &str) {
        let g = base_git(&self.sdir, NO_EXCLUDES).env("GIT_DIR", common_dir);
        let prefix = format!("{REF_ROOT}/{}/", self.session);
        let Ok(list) = text(&g, &["for-each-ref", "--format=%(refname)", &prefix]) else {
            return;
        };
        let input: String = list
            .lines()
            .filter(|r| r.starts_with(&prefix))
            .map(|r| format!("delete {r}\n"))
            .collect();
        if input.is_empty() {
            return;
        }
        let file = self.sdir.join(format!(".checkpoint.info.{}", rand_hex()));
        if fs::write(&file, input).is_ok() {
            let _ = out(&g, &["update-ref", "--stdin"], Some(&file), ERR_CAP);
        }
        let _ = fs::remove_file(file);
    }
}

fn trim_history(m: &mut Manifest, keep: usize, protect: &[u64]) -> Vec<u64> {
    let mut dropped = Vec::new();
    let mut i = 0;
    while m.items.len() > keep && i < m.items.len() {
        if protect.contains(&m.items[i].item.n) {
            i += 1;
        } else {
            dropped.push(m.items.remove(i).item.n);
        }
    }
    dropped
}

struct Op<'a> {
    store: &'a Store,
    env: &'a Env<'a>,
    excludes: String,
}

impl<'a> Op<'a> {
    fn new(store: &'a Store, env: &'a Env<'a>) -> Op<'a> {
        Op {
            store,
            env,
            excludes: global_excludes(env),
        }
    }

    fn git(&self) -> GitEnv {
        base_git(&self.store.cwd, &self.excludes)
    }

    fn repo(&self) -> Result<Repo, CpError> {
        let g = self.git();
        let o = exec(&g, &["rev-parse", "--is-inside-work-tree"], None, ERR_CAP)?;
        if !o.status.success() {
            let err = String::from_utf8_lossy(&o.stderr);
            return Err(CpError::NotGit(if err.contains("not a git repository") {
                "not a git repository".to_string()
            } else {
                format!("git: {}", err.lines().next().unwrap_or("").trim())
            }));
        }
        if String::from_utf8_lossy(&o.stdout).trim() != "true" {
            return Err(CpError::NotGit("not inside a git work tree".into()));
        }
        let o = out(
            &g,
            &[
                "rev-parse",
                "--path-format=absolute",
                "--show-toplevel",
                "--git-common-dir",
                "--show-prefix",
            ],
            None,
            ERR_CAP,
        )?;
        let mut lines = o.split(|b| *b == b'\n');
        let (Some(top), Some(common)) = (lines.next(), lines.next()) else {
            return Err(CpError::Failed("unexpected git rev-parse output".into()));
        };
        let prefix = lines.next().unwrap_or(b"");
        let at =
            Path::new(std::ffi::OsStr::from_bytes(top)).join(std::ffi::OsStr::from_bytes(prefix));
        let same = match (fs::canonicalize(at), fs::canonicalize(&self.store.cwd)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        };
        if !same {
            return Err(CpError::NotGit(
                "the work tree does not match the session directory".into(),
            ));
        }
        Ok(Repo {
            common_dir: String::from_utf8_lossy(common).into_owned(),
        })
    }

    fn repo_or_mark(&self, m: &mut Manifest) -> Result<Repo, CpError> {
        match self.repo() {
            Err(CpError::NotGit(note)) => {
                if m.note.as_deref() != Some(&note) {
                    m.note = Some(note.clone());
                    let _ = self.store.save(m);
                }
                Err(CpError::NotGit(note))
            }
            other => other,
        }
    }

    fn enumerate(&self) -> Result<BTreeSet<Vec<u8>>, CpError> {
        let o = out(
            &self.git(),
            &["ls-files", "-c", "-o", "--exclude-standard", "-z"],
            None,
            LIST_CAP,
        )?;
        Ok(o.split(|b| *b == 0)
            .filter(|p| !p.is_empty() && !p.ends_with(b"/"))
            .map(<[u8]>::to_vec)
            .collect())
    }

    fn ls_tree(&self, treeish: &str) -> Result<Vec<TreeEntry>, CpError> {
        let o = out(
            &self.git(),
            &["ls-tree", "-r", "-z", "--full-tree", treeish],
            None,
            LIST_CAP,
        )?;
        let bad = || CpError::Failed("unexpected git ls-tree output".into());
        let mut entries = Vec::new();
        for rec in o.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            let tab = rec.iter().position(|b| *b == b'\t').ok_or_else(bad)?;
            let head = std::str::from_utf8(&rec[..tab]).map_err(|_| bad())?;
            let mut it = head.split(' ');
            let (Some(mode), Some(ty), Some(oid), None) =
                (it.next(), it.next(), it.next(), it.next())
            else {
                return Err(bad());
            };
            entries.push(TreeEntry {
                mode: u32::from_str_radix(mode, 8).map_err(|_| bad())?,
                ty: ty.to_string(),
                oid: oid.to_string(),
                path: rec[tab + 1..].to_vec(),
            });
        }
        Ok(entries)
    }

    fn changed(&self, a: &str, b: &str) -> Result<Vec<(u8, Vec<u8>)>, CpError> {
        let o = out(
            &self.git(),
            &[
                "diff-tree",
                "-r",
                "--no-renames",
                "--name-status",
                "-z",
                a,
                b,
            ],
            None,
            LIST_CAP,
        )?;
        let mut fields = o.split(|f| *f == 0).filter(|f| !f.is_empty());
        let mut changes = Vec::new();
        while let (Some(status), Some(path)) = (fields.next(), fields.next()) {
            changes.push((status[0], path.to_vec()));
        }
        Ok(changes)
    }

    fn empty_tree(&self) -> Result<String, CpError> {
        let o = out(
            &self.git(),
            &["hash-object", "-t", "tree", "--stdin"],
            Some(Path::new("/dev/null")),
            ERR_CAP,
        )?;
        Ok(String::from_utf8_lossy(&o).trim().to_string())
    }

    fn diff(&self, n: u64, a: &str, b: &str) -> Result<CheckpointDiff, CpError> {
        let (mut added, mut modified, mut deleted) = (0, 0, 0);
        for (status, _) in self.changed(a, b)? {
            match status {
                b'A' => added += 1,
                b'D' => deleted += 1,
                _ => modified += 1,
            }
        }
        let g = self.git();
        let flags = [
            "--no-ext-diff",
            "--no-textconv",
            "--no-color",
            "--no-renames",
        ];
        let full: Vec<&str> = ["diff"]
            .into_iter()
            .chain(flags)
            .chain([a, b, "--"])
            .collect();
        let stat: Vec<&str> = ["diff", "--stat"]
            .into_iter()
            .chain(flags)
            .chain([a, b, "--"])
            .collect();
        let (raw, mut truncated) = match exec(&g, &full, None, DIFF_RAW_CAP) {
            Ok(o) if o.status.success() => (o.stdout, false),
            _ => (out(&g, &stat, None, DIFF_RAW_CAP)?, true),
        };
        let mut text = String::from_utf8_lossy(&raw).into_owned();
        if text.len() > DIFF_TEXT_CAP {
            let mut end = DIFF_TEXT_CAP;
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            text.truncate(end);
            truncated = true;
        }
        Ok(CheckpointDiff {
            n,
            text: sanitize(&text),
            truncated,
            added,
            modified,
            deleted,
        })
    }

    fn record(
        &self,
        m: &mut Manifest,
        repo: &Repo,
        cap: &Captured,
        kind: CheckpointKind,
        from: Option<u64>,
        protect: &[u64],
    ) -> Result<CheckpointItem, CpError> {
        let n = m.next_n.max(1);
        let files = match m.items.last() {
            None => 0,
            Some(prev) => self.changed(&prev.tree, &cap.tree)?.len() as u32,
        };
        let turn = if kind == CheckpointKind::Turn && files > 0 {
            m.turn + 1
        } else {
            m.turn
        };
        let mut msg = format!(
            "tmgr checkpoint {n}\n\nkind: {}\nturn: {turn}\n",
            kind_str(kind)
        );
        if let Some(f) = from {
            msg.push_str(&format!("restored_from: {f}\n"));
        }
        let g = self
            .git()
            .env("GIT_AUTHOR_NAME", "tmgr")
            .env("GIT_AUTHOR_EMAIL", "checkpoints@tmgr.invalid")
            .env("GIT_COMMITTER_NAME", "tmgr")
            .env("GIT_COMMITTER_EMAIL", "checkpoints@tmgr.invalid");
        let commit = text(&g, &["commit-tree", "--no-gpg-sign", "-m", &msg, &cap.tree])?;
        let rname = self.store.ref_name(n);
        text(&g, &["update-ref", &rname, &commit])?;
        let item = CheckpointItem {
            n,
            kind,
            turn,
            files,
            skipped: cap.skipped,
            at_ms: now_ms(),
            restored_from: from,
        };
        m.items.push(Entry {
            item: item.clone(),
            commit,
            tree: cap.tree.clone(),
        });
        m.next_n = n + 1;
        m.turn = turn;
        m.note = None;
        m.common_dir = Some(repo.common_dir.clone());
        let mut keep_these = protect.to_vec();
        keep_these.push(n);
        let dropped = trim_history(m, self.store.keep_clamped(), &keep_these);
        if let Err(e) = self.store.save(m) {
            let _ = text(&g, &["update-ref", "-d", &rname]);
            return Err(e);
        }
        for d in dropped {
            let _ = text(&g, &["update-ref", "-d", &self.store.ref_name(d)]);
        }
        Ok(item)
    }

    fn record_before(
        &self,
        m: &mut Manifest,
        repo: &Repo,
        before: &Captured,
        target: u64,
    ) -> Result<u64, CpError> {
        if let Some(last) = m.items.last() {
            if last.tree == before.tree {
                return Ok(last.item.n);
            }
        }
        let item = self.record(
            m,
            repo,
            before,
            CheckpointKind::BeforeRestore,
            None,
            &[target],
        )?;
        Ok(item.n)
    }

    fn verify_ref(&self, n: u64, commit: &str) -> Result<(), CpError> {
        if !valid_oid(commit) {
            return Err(CpError::Refused(
                "the recorded checkpoint is invalid".into(),
            ));
        }
        let spec = format!("{}^{{commit}}", self.store.ref_name(n));
        let o = exec(
            &self.git(),
            &["rev-parse", "--verify", "-q", &spec],
            None,
            ERR_CAP,
        )?;
        if o.status.success() && String::from_utf8_lossy(&o.stdout).trim() == commit {
            Ok(())
        } else {
            Err(CpError::Refused(
                "the checkpoint ref does not match the recorded checkpoint".into(),
            ))
        }
    }

    fn read_target(&self, commit: &str) -> Result<Vec<TreeEntry>, CpError> {
        let mut entries = self.ls_tree(commit)?;
        let unsafe_tree = |why: &str| CpError::Refused(format!("checkpoint {why}"));
        let mut paths = BTreeSet::new();
        for e in &entries {
            if e.ty != "blob" || !matches!(e.mode, MODE_FILE | MODE_EXEC | MODE_LINK) {
                return Err(unsafe_tree("holds an entry that cannot be restored"));
            }
            if !valid_path(&e.path) || !valid_oid(&e.oid) {
                return Err(unsafe_tree("holds an unsafe path"));
            }
            paths.insert(e.path.as_slice());
        }
        for e in &entries {
            let mut p = e.path.as_slice();
            while let Some(i) = p.iter().rposition(|b| *b == b'/') {
                p = &p[..i];
                if paths.contains(p) {
                    return Err(unsafe_tree("holds conflicting paths"));
                }
            }
        }
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(entries)
    }

    fn collision_paths(&self, target: &[TreeEntry]) -> Vec<Vec<u8>> {
        let mut set = BTreeSet::new();
        for e in target {
            let mut p = e.path.as_slice();
            set.insert(p.to_vec());
            while let Some(i) = p.iter().rposition(|b| *b == b'/') {
                p = &p[..i];
                set.insert(p.to_vec());
            }
        }
        set.into_iter().collect()
    }
}

impl Op<'_> {
    fn capture(&self, extra: &[Vec<u8>]) -> Result<Captured, CpError> {
        let cache = fs::read(self.store.cache_path())
            .ok()
            .and_then(|b| serde_json::from_slice::<Cache>(&b).ok())
            .unwrap_or_default();
        let mut hits = 0;
        match self.capture_with(extra, &cache, &mut hits) {
            Err(CpError::Failed(_)) if hits > 0 => {
                let _ = fs::remove_file(self.store.cache_path());
                self.capture_with(extra, &Cache::default(), &mut 0)
            }
            other => other,
        }
    }

    fn capture_with(
        &self,
        extra: &[Vec<u8>],
        cache: &Cache,
        hits: &mut usize,
    ) -> Result<Captured, CpError> {
        let mut paths = self.enumerate()?;
        paths.extend(extra.iter().cloned());
        let root = open_root(&self.store.cwd).map_err(|e| failed("cannot open directory", e))?;
        let now_s = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let g = self.git();
        let mut importer: Option<Importer> = None;
        let mut pending: Vec<(Vec<u8>, u32, Fp, usize)> = Vec::new();
        let mut entries: BTreeMap<Vec<u8>, Meta> = BTreeMap::new();
        let (mut skipped, mut total) = (0u32, 0u64);
        let mut parent: Option<(Vec<u8>, OwnedFd)> = None;
        for path in &paths {
            if !valid_path(path) {
                continue;
            }
            let (dirs, name) = split_path(path);
            let dir_key = path[..path.len() - name.len()].to_vec();
            if parent.as_ref().map(|(k, _)| k) != Some(&dir_key) {
                parent = walk_dirs(root.as_fd(), &dirs).ok().map(|fd| (dir_key, fd));
            }
            let Some((_, pfd)) = &parent else { continue };
            let Ok(st) = stat_at(pfd.as_fd(), name) else {
                continue;
            };
            let (fp, mode, data) = if is_kind(&st, libc::S_IFLNK) {
                match read_link_at(pfd.as_fd(), name) {
                    Ok(target) => (fp_of(&st), MODE_LINK, target),
                    Err(_) => continue,
                }
            } else if is_kind(&st, libc::S_IFREG) {
                let size = st.st_size as u64;
                if st.st_nlink > 1 || size > self.env.max_file {
                    skipped += 1;
                    continue;
                }
                let fp = fp_of(&st);
                if let Some(oid) = cached_oid(cache, path, &fp, now_s) {
                    *hits += 1;
                    entries.insert(
                        path.clone(),
                        Meta {
                            mode: blob_mode(&st),
                            oid,
                            fp,
                        },
                    );
                    continue;
                }
                if total + size > self.env.max_total {
                    skipped += 1;
                    continue;
                }
                match read_regular(pfd.as_fd(), name, self.env.max_file) {
                    ReadFile::Data(fp, mode, data) => {
                        total += size;
                        (fp, mode, data)
                    }
                    ReadFile::Skip => {
                        skipped += 1;
                        continue;
                    }
                    ReadFile::Gone => continue,
                }
            } else {
                continue;
            };
            if importer.is_none() {
                importer = Some(Importer::start(&g)?);
            }
            if let Some(imp) = importer.as_mut() {
                pending.push((path.clone(), mode, fp, imp.add(&data)?));
            }
        }
        if let Some(imp) = importer {
            let oids = imp.finish()?;
            for (path, mode, fp, mark) in pending {
                entries.insert(
                    path,
                    Meta {
                        mode,
                        oid: oids[mark - 1].clone(),
                        fp,
                    },
                );
            }
        }
        let tree = self.write_tree(&entries)?;
        let in_tree: BTreeSet<Vec<u8>> = self.ls_tree(&tree)?.into_iter().map(|e| e.path).collect();
        entries.retain(|p, _| in_tree.contains(p));
        self.save_cache(&entries, now_s);
        Ok(Captured {
            tree,
            entries,
            skipped,
        })
    }

    fn write_tree(&self, entries: &BTreeMap<Vec<u8>, Meta>) -> Result<String, CpError> {
        let id = rand_hex();
        let index = self.store.sdir.join(format!("checkpoint.index.{id}"));
        let info = self.store.sdir.join(format!(".checkpoint.info.{id}"));
        let mut lines = Vec::new();
        for (path, meta) in entries {
            lines.extend_from_slice(format!("{:o} {}\t", meta.mode, meta.oid).as_bytes());
            lines.extend_from_slice(path);
            lines.push(0);
        }
        let result = (|| {
            fs::write(&info, &lines).map_err(|e| failed("cannot write index input", e))?;
            let index_str = index
                .to_str()
                .ok_or_else(|| CpError::Failed("session directory is not valid UTF-8".into()))?;
            let g = self.git().env("GIT_INDEX_FILE", index_str);
            out(
                &g,
                &["update-index", "-z", "--add", "--index-info"],
                Some(&info),
                ERR_CAP,
            )?;
            text(&g, &["write-tree"])
        })();
        let _ = fs::remove_file(&info);
        let _ = fs::remove_file(&index);
        let mut lock = index.into_os_string();
        lock.push(".lock");
        let _ = fs::remove_file(lock);
        result
    }

    fn save_cache(&self, entries: &BTreeMap<Vec<u8>, Meta>, now_s: i64) {
        let mut cache = Cache::default();
        for (path, meta) in entries {
            let Ok(key) = std::str::from_utf8(path) else {
                continue;
            };
            if meta.mode != MODE_LINK && now_s - meta.fp.mt_s > FRESH_SECS {
                cache.files.insert(
                    key.to_string(),
                    CacheEntry {
                        fp: meta.fp,
                        oid: meta.oid.clone(),
                    },
                );
            }
        }
        if let Ok(bytes) = serde_json::to_vec(&cache) {
            let _ = write_atomic(&self.store.cache_path(), &bytes);
        }
    }

    fn apply(
        &self,
        target: &[TreeEntry],
        before: &Captured,
        rep: &mut Report,
    ) -> Result<(), CpError> {
        let root = open_root(&self.store.cwd).map_err(|e| failed("cannot open directory", e))?;
        let mut blobs = Blobs::open(&self.git())?;
        for e in target {
            match self.write_one(root.as_fd(), e, before, &mut blobs)? {
                Outcome::Written => rep.written += 1,
                Outcome::Left => {
                    rep.left
                        .insert(String::from_utf8_lossy(&e.path).into_owned());
                }
                Outcome::Same | Outcome::Deleted => {}
            }
        }
        let wanted: BTreeSet<&[u8]> = target.iter().map(|e| e.path.as_slice()).collect();
        let mut emptied = BTreeSet::new();
        for path in self.enumerate()? {
            if wanted.contains(path.as_slice()) {
                continue;
            }
            let Some(meta) = before.entries.get(&path) else {
                continue;
            };
            match delete_one(root.as_fd(), &path, meta) {
                Outcome::Deleted => {
                    rep.deleted += 1;
                    let (_, name) = split_path(&path);
                    emptied.insert(path[..path.len() - name.len()].to_vec());
                }
                Outcome::Left => {
                    rep.left.insert(String::from_utf8_lossy(&path).into_owned());
                }
                Outcome::Same | Outcome::Written => {}
            }
        }
        for dir in emptied.iter().rev() {
            prune_empty(root.as_fd(), dir.strip_suffix(b"/").unwrap_or(dir));
        }
        Ok(())
    }

    fn write_one(
        &self,
        root: BorrowedFd,
        e: &TreeEntry,
        before: &Captured,
        blobs: &mut Blobs,
    ) -> Result<Outcome, CpError> {
        if before
            .entries
            .get(&e.path)
            .is_some_and(|b| b.mode == e.mode && b.oid == e.oid)
        {
            return Ok(Outcome::Same);
        }
        let (dirs, name) = split_path(&e.path);
        let Some(dir) = open_parents(root, &dirs, before) else {
            return Ok(Outcome::Left);
        };
        let dir = dir.as_fd();
        let mut keep_perm = None;
        match stat_at(dir, name) {
            Err(err) if err.raw_os_error() == Some(libc::ENOENT) => {}
            Err(_) => return Ok(Outcome::Left),
            Ok(st) if is_kind(&st, libc::S_IFDIR) => {
                if unlink_at(dir, name, libc::AT_REMOVEDIR).is_err() {
                    return Ok(Outcome::Left);
                }
            }
            Ok(st) => {
                if !before
                    .entries
                    .get(&e.path)
                    .is_some_and(|b| b.fp == fp_of(&st))
                {
                    return Ok(Outcome::Left);
                }
                if is_kind(&st, libc::S_IFREG) {
                    keep_perm = Some(mode_bits(&st) & 0o777);
                }
            }
        }
        let tmp = format!(".tmgr-restore-{}", rand_hex()).into_bytes();
        let made = if e.mode == MODE_LINK {
            write_link(dir, &tmp, &e.oid, blobs)
        } else {
            write_file(dir, &tmp, e, keep_perm, blobs)
        };
        let made = made.inspect_err(|_| {
            let _ = unlink_at(dir, &tmp, 0);
        })?;
        let placed = made.is_ok() && rename_at(dir, &tmp, name).is_ok();
        if placed {
            Ok(Outcome::Written)
        } else {
            let _ = unlink_at(dir, &tmp, 0);
            Ok(Outcome::Left)
        }
    }
}

fn cached_oid(cache: &Cache, path: &[u8], fp: &Fp, now_s: i64) -> Option<String> {
    let entry = cache.files.get(std::str::from_utf8(path).ok()?)?;
    (entry.fp == *fp && now_s - fp.mt_s > FRESH_SECS && valid_oid(&entry.oid))
        .then(|| entry.oid.clone())
}

fn open_parents(root: BorrowedFd, dirs: &[&[u8]], before: &Captured) -> Option<OwnedFd> {
    let mut cur = root.try_clone_to_owned().ok()?;
    let mut prefix: Vec<u8> = Vec::new();
    for comp in dirs {
        if !prefix.is_empty() {
            prefix.push(b'/');
        }
        prefix.extend_from_slice(comp);
        let next = match open_dir_at(cur.as_fd(), comp) {
            Ok(fd) => fd,
            Err(err) => {
                match err.raw_os_error() {
                    Some(libc::ENOENT) => {}
                    Some(libc::ENOTDIR | libc::ELOOP) => {
                        let st = stat_at(cur.as_fd(), comp).ok()?;
                        let known = before.entries.get(&prefix)?;
                        if is_kind(&st, libc::S_IFDIR) || known.fp != fp_of(&st) {
                            return None;
                        }
                        unlink_at(cur.as_fd(), comp, 0).ok()?;
                    }
                    _ => return None,
                }
                mkdir_at(cur.as_fd(), comp).ok()?;
                open_dir_at(cur.as_fd(), comp).ok()?
            }
        };
        cur = next;
    }
    Some(cur)
}

fn write_file(
    dir: BorrowedFd,
    tmp: &[u8],
    e: &TreeEntry,
    keep_perm: Option<u32>,
    blobs: &mut Blobs,
) -> Result<Result<(), String>, CpError> {
    let exec = e.mode == MODE_EXEC;
    let flags = libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW;
    let fd = match open_at(dir, tmp, flags, if exec { 0o755 } else { 0o644 }) {
        Ok(fd) => fd,
        Err(err) => return Ok(Err(err.to_string())),
    };
    let mut file = File::from(fd);
    if let Err(why) = blobs.fetch(&e.oid, &mut |chunk| file.write_all(chunk))? {
        return Ok(Err(why));
    }
    if let Some(perm) = keep_perm {
        let perm = if exec {
            perm | ((perm & 0o444) >> 2)
        } else {
            perm & !0o111
        };
        if let Err(err) = file.set_permissions(fs::Permissions::from_mode(perm)) {
            return Ok(Err(err.to_string()));
        }
    }
    Ok(file.flush().map_err(|err| err.to_string()))
}

fn write_link(
    dir: BorrowedFd,
    tmp: &[u8],
    oid: &str,
    blobs: &mut Blobs,
) -> Result<Result<(), String>, CpError> {
    let mut target = Vec::new();
    let fetched = blobs.fetch(oid, &mut |chunk| {
        if target.len() + chunk.len() > LINK_CAP {
            return Err(io::Error::other("link target is too long"));
        }
        target.extend_from_slice(chunk);
        Ok(())
    })?;
    if let Err(why) = fetched {
        return Ok(Err(why));
    }
    Ok(symlink_at(&target, dir, tmp).map_err(|err| err.to_string()))
}

fn delete_one(root: BorrowedFd, path: &[u8], meta: &Meta) -> Outcome {
    let (dirs, name) = split_path(path);
    let Ok(dir) = walk_dirs(root, &dirs) else {
        return Outcome::Left;
    };
    match stat_at(dir.as_fd(), name) {
        Err(err) if err.raw_os_error() == Some(libc::ENOENT) => Outcome::Same,
        Ok(st) if fp_of(&st) == meta.fp && unlink_at(dir.as_fd(), name, 0).is_ok() => {
            Outcome::Deleted
        }
        _ => Outcome::Left,
    }
}

fn prune_empty(root: BorrowedFd, dir: &[u8]) {
    let mut cur = dir.to_vec();
    while !cur.is_empty() {
        let (parents, name) = split_path(&cur);
        let Ok(pfd) = walk_dirs(root, &parents) else {
            return;
        };
        if unlink_at(pfd.as_fd(), name, libc::AT_REMOVEDIR).is_err() {
            return;
        }
        let keep = cur.len() - name.len();
        cur.truncate(keep.saturating_sub(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::ffi::OsStrExt;
    use std::os::unix::fs::{symlink, PermissionsExt};
    use std::path::Path;
    use std::process::{Command, Stdio};
    use tempfile::TempDir;
    use tmgr_ipc::api::CheckpointKind;

    const S1: &str = "s_abcdefghij";
    const S2: &str = "s_klmnopqrst";

    struct Fx {
        root: PathBuf,
        _t: TempDir,
        home: PathBuf,
        xdg: PathBuf,
        repo: PathBuf,
        sdir: PathBuf,
        out: PathBuf,
    }

    fn pg_in(dir: &Path, home: &Path, args: &[&str], input: &[u8]) -> (bool, Vec<u8>) {
        let mut cmd = Command::new("git");
        cmd.env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", home)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .current_dir(dir)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = cmd.spawn().unwrap();
        let _ = child.stdin.take().unwrap().write_all(input);
        let out = child.wait_with_output().unwrap();
        (out.status.success(), out.stdout)
    }

    impl Fx {
        fn new() -> Fx {
            let t = TempDir::new().unwrap();
            let root = t.path().canonicalize().unwrap();
            let fx = Fx {
                home: root.join("home"),
                xdg: root.join("xdg"),
                repo: root.join("repo"),
                sdir: root.join("sdir"),
                out: root.join("outside"),
                root,
                _t: t,
            };
            for d in [&fx.home, &fx.xdg, &fx.repo, &fx.sdir, &fx.out] {
                fs::create_dir_all(d).unwrap();
            }
            fx.pg(&["init", "-q"]);
            fx
        }

        fn try_pg(&self, args: &[&str]) -> (bool, Vec<u8>) {
            pg_in(&self.repo, &self.home, args, b"")
        }

        fn pgb(&self, args: &[&str]) -> Vec<u8> {
            let (ok, out) = self.try_pg(args);
            assert!(ok, "git {args:?} failed");
            out
        }

        fn pg(&self, args: &[&str]) -> String {
            String::from_utf8_lossy(&self.pgb(args)).trim().to_string()
        }

        fn store(&self) -> Store {
            Store {
                session: S1.to_string(),
                sdir: self.sdir.clone(),
                cwd: self.repo.clone(),
                keep: 50,
            }
        }

        fn env(&self) -> Env<'static> {
            Env {
                home: Some(self.home.clone().into_os_string()),
                xdg: Some(self.xdg.clone().into_os_string()),
                max_file: MAX_FILE,
                max_total: MAX_TOTAL,
                after_before: None,
            }
        }

        fn w(&self, rel: &str, data: &[u8]) {
            let p = self.repo.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, data).unwrap();
        }

        fn r(&self, rel: &str) -> Vec<u8> {
            fs::read(self.repo.join(rel)).unwrap()
        }

        fn exists(&self, rel: &str) -> bool {
            fs::symlink_metadata(self.repo.join(rel)).is_ok()
        }

        fn commit_all(&self, msg: &str) {
            self.pg(&["add", "-A"]);
            self.pg(&["commit", "-q", "-m", msg]);
        }

        fn dump(&self) -> BTreeMap<String, Vec<u8>> {
            let mut m = BTreeMap::new();
            walk(&self.repo, &self.repo, &mut m);
            m
        }
    }

    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
        for e in fs::read_dir(dir).unwrap() {
            let e = e.unwrap();
            let p = e.path();
            let rel = p.strip_prefix(base).unwrap().to_string_lossy().into_owned();
            if rel == ".git" {
                continue;
            }
            let ft = e.file_type().unwrap();
            if ft.is_dir() {
                out.insert(format!("{rel}/"), Vec::new());
                walk(base, &p, out);
            } else if ft.is_symlink() {
                out.insert(
                    rel,
                    fs::read_link(&p).unwrap().as_os_str().as_bytes().to_vec(),
                );
            } else {
                out.insert(rel, fs::read(&p).unwrap());
            }
        }
    }

    fn snap(fx: &Fx, st: &Store) -> CheckpointItem {
        st.snapshot_in(&fx.env()).unwrap().unwrap()
    }

    fn commit_of(st: &Store, n: u64) -> String {
        st.load()
            .items
            .iter()
            .find(|e| e.item.n == n)
            .unwrap()
            .commit
            .clone()
    }

    fn tree_paths(fx: &Fx, commit: &str) -> Vec<String> {
        fx.pg(&["ls-tree", "-r", "--name-only", commit])
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn blob_at(fx: &Fx, commit: &str, path: &str) -> Vec<u8> {
        fx.pgb(&["cat-file", "-p", &format!("{commit}:{path}")])
    }

    fn script(path: &Path, body: &str) {
        fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn refs_under(fx: &Fx, prefix: &str) -> Vec<String> {
        fx.pg(&["for-each-ref", "--format=%(refname)", prefix])
            .lines()
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn snapshot_includes_tracked_and_untracked() {
        let fx = Fx::new();
        fx.w("tracked.txt", b"t");
        fx.commit_all("c");
        fx.w("untracked.txt", b"u");
        let st = fx.store();
        let item = snap(&fx, &st);
        assert_eq!(item.kind, CheckpointKind::Start);
        assert_eq!(item.n, 1);
        assert_eq!(item.files, 0);
        let c = commit_of(&st, 1);
        assert_eq!(tree_paths(&fx, &c), ["tracked.txt", "untracked.txt"]);
        assert_eq!(
            refs_under(&fx, "refs/tmgr"),
            [format!("refs/tmgr/checkpoints/{S1}/1")]
        );
    }

    #[test]
    fn snapshot_kinds_and_turn_numbers() {
        let fx = Fx::new();
        fx.w("a", b"1");
        let st = fx.store();
        let first = snap(&fx, &st);
        fx.w("a", b"2");
        fx.w("b", b"x");
        let second = snap(&fx, &st);
        assert_eq!((first.kind, first.turn), (CheckpointKind::Start, 0));
        assert_eq!(
            (second.kind, second.turn, second.files),
            (CheckpointKind::Turn, 1, 2)
        );
        assert_eq!(st.list().len(), 2);
        let status = st.status();
        assert!(status.available);
        assert_eq!(status.count, 2);
    }

    #[test]
    fn identical_tree_is_not_recorded_again() {
        let fx = Fx::new();
        fx.w("a", b"1");
        let st = fx.store();
        snap(&fx, &st);
        assert_eq!(st.snapshot_in(&fx.env()), Ok(None));
        assert_eq!(st.list().len(), 1);
        assert_eq!(refs_under(&fx, "refs/tmgr").len(), 1);
    }

    #[test]
    fn repo_ignore_sources_are_excluded() {
        let fx = Fx::new();
        fx.w(".gitignore", b"ig1\n");
        fs::write(fx.repo.join(".git/info/exclude"), "ig2\n").unwrap();
        for f in ["ig1", "ig2", "keep"] {
            fx.w(f, b"x");
        }
        let st = fx.store();
        snap(&fx, &st);
        assert_eq!(tree_paths(&fx, &commit_of(&st, 1)), [".gitignore", "keep"]);
    }

    #[test]
    fn global_excludes_from_configured_path() {
        let fx = Fx::new();
        let list = fx.home.join("my-ignore");
        fs::write(&list, "ig3\n").unwrap();
        fs::write(
            fx.home.join(".gitconfig"),
            format!("[core]\n\texcludesFile = {}\n", list.display()),
        )
        .unwrap();
        fs::create_dir_all(fx.xdg.join("git")).unwrap();
        fs::write(fx.xdg.join("git/ignore"), "xdgonly\n").unwrap();
        for f in ["ig3", "xdgonly", "keep"] {
            fx.w(f, b"x");
        }
        let st = fx.store();
        snap(&fx, &st);
        assert_eq!(tree_paths(&fx, &commit_of(&st, 1)), ["keep", "xdgonly"]);
    }

    #[test]
    fn global_excludes_xdg_default_and_home_default() {
        let fx = Fx::new();
        fs::create_dir_all(fx.xdg.join("git")).unwrap();
        fs::write(fx.xdg.join("git/ignore"), "ig4\n").unwrap();
        fs::create_dir_all(fx.home.join(".config/git")).unwrap();
        fs::write(fx.home.join(".config/git/ignore"), "ig5\n").unwrap();
        for f in ["ig4", "ig5", "keep"] {
            fx.w(f, b"x");
        }
        let st = fx.store();
        snap(&fx, &st);
        assert_eq!(tree_paths(&fx, &commit_of(&st, 1)), ["ig5", "keep"]);

        let fx = Fx::new();
        fs::create_dir_all(fx.home.join(".config/git")).unwrap();
        fs::write(fx.home.join(".config/git/ignore"), "ig5\n").unwrap();
        for f in ["ig5", "keep"] {
            fx.w(f, b"x");
        }
        let st = fx.store();
        let env = Env {
            xdg: None,
            ..fx.env()
        };
        st.snapshot_in(&env).unwrap().unwrap();
        assert_eq!(tree_paths(&fx, &commit_of(&st, 1)), ["keep"]);
    }

    #[test]
    fn round_trip_is_byte_exact() {
        let fx = Fx::new();
        fx.w(".gitattributes", b"* text=auto\n*.id ident\n");
        let files: Vec<(String, Vec<u8>)> = vec![
            ("crlf.txt".into(), b"a\r\nb\r\n".to_vec()),
            ("mixed.txt".into(), b"a\r\nb\nc\r".to_vec()),
            ("ident.id".into(), b"$Id: deadbeef $\n".to_vec()),
            ("bin.dat".into(), (0u8..=255).collect()),
            ("deep/er/file.txt".into(), b"deep".to_vec()),
            ("empty".into(), Vec::new()),
        ];
        let odd: &[u8] = b"caf\xe9.txt";
        let odd_path = fx.repo.join(std::ffi::OsStr::from_bytes(odd));
        let odd_ok = fs::write(&odd_path, b"latin").is_ok();
        for (p, d) in &files {
            fx.w(p, d);
        }
        fx.w("run.sh", b"#!/bin/sh\n");
        fs::set_permissions(fx.repo.join("run.sh"), fs::Permissions::from_mode(0o755)).unwrap();
        symlink("crlf.txt", fx.repo.join("link")).unwrap();
        symlink("/nonexistent/abs", fx.repo.join("dangling")).unwrap();
        let before = fx.dump();
        let st = fx.store();
        snap(&fx, &st);
        for (p, _) in &files {
            fx.w(p, b"changed");
        }
        fs::remove_file(fx.repo.join("link")).unwrap();
        fs::remove_file(fx.repo.join("dangling")).unwrap();
        fs::remove_file(fx.repo.join("run.sh")).unwrap();
        if odd_ok {
            fs::write(&odd_path, b"changed").unwrap();
        }
        let res = st.restore_in(&fx.env(), 1).unwrap();
        assert!(res.left_as_is.is_empty(), "{:?}", res.left_as_is);
        assert_eq!(fx.dump(), before);
        let mode = fs::metadata(fx.repo.join("run.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111);
        if odd_ok {
            assert_eq!(fs::read(&odd_path).unwrap(), b"latin");
        }
    }

    fn git_state(fx: &Fx) -> Vec<Vec<u8>> {
        vec![
            fs::read(fx.repo.join(".git/index")).unwrap(),
            fs::read(fx.repo.join(".git/HEAD")).unwrap(),
            fs::read(fx.repo.join(".git/packed-refs")).unwrap_or_default(),
            fs::read(fx.repo.join(".git/refs/stash")).unwrap_or_default(),
            fx.pgb(&["for-each-ref", "refs/heads", "refs/tags", "refs/stash"]),
            fx.pgb(&["stash", "list"]),
        ]
    }

    #[test]
    fn git_state_is_untouched() {
        let fx = Fx::new();
        fx.w("tracked.txt", b"v1");
        fx.commit_all("c1");
        fx.pg(&["branch", "side"]);
        fx.w("tracked.txt", b"stashed");
        fx.pg(&["stash"]);
        fx.pg(&["pack-refs", "--all"]);
        fx.w("tracked.txt", b"v2");
        fx.w("new.txt", b"n");
        fx.pg(&["add", "new.txt"]);
        let state = git_state(&fx);
        let st = fx.store();
        snap(&fx, &st);
        fx.w("tracked.txt", b"v3");
        fx.w("other.txt", b"o");
        snap(&fx, &st);
        st.restore_in(&fx.env(), 1).unwrap();
        st.restore_in(&fx.env(), 2).unwrap();
        assert_eq!(git_state(&fx), state);
    }

    #[test]
    fn restore_round_trip_modified_deleted_added() {
        let fx = Fx::new();
        fx.w("a.txt", b"one");
        fx.w("b.txt", b"bee");
        fx.commit_all("c");
        let st = fx.store();
        snap(&fx, &st);
        fx.w("a.txt", b"two");
        fs::remove_file(fx.repo.join("b.txt")).unwrap();
        fx.w("c.txt", b"new");
        snap(&fx, &st);
        let res = st.restore_in(&fx.env(), 1).unwrap();
        assert_eq!(fx.r("a.txt"), b"one");
        assert_eq!(fx.r("b.txt"), b"bee");
        assert!(!fx.exists("c.txt"));
        assert_eq!(
            (res.n, res.before_n, res.after_n, res.written, res.deleted),
            (1, 2, 3, 2, 1)
        );
        assert!(res.left_as_is.is_empty());
        let items = st.list();
        assert_eq!(items[2].kind, CheckpointKind::Restored);
        assert_eq!(items[2].restored_from, Some(1));
    }

    #[test]
    fn restore_records_before_restore_when_state_is_new() {
        let fx = Fx::new();
        fx.w("a", b"1");
        let st = fx.store();
        snap(&fx, &st);
        fx.w("a", b"2");
        let res = st.restore_in(&fx.env(), 1).unwrap();
        assert_eq!((res.before_n, res.after_n), (2, 3));
        let items = st.list();
        assert_eq!(items[1].kind, CheckpointKind::BeforeRestore);
        assert_eq!(blob_at(&fx, &commit_of(&st, 2), "a"), b"2");
    }

    #[test]
    fn restore_is_undoable_through_before_n() {
        let fx = Fx::new();
        fx.w("a", b"1");
        let st = fx.store();
        snap(&fx, &st);
        fx.w("a", b"2");
        fx.w("extra", b"e");
        let dirty = fx.dump();
        let res = st.restore_in(&fx.env(), 1).unwrap();
        assert_eq!(fx.r("a"), b"1");
        assert!(!fx.exists("extra"));
        st.restore_in(&fx.env(), res.before_n).unwrap();
        assert_eq!(fx.dump(), dirty);
    }

    #[test]
    fn ignored_file_survives_restore() {
        let fx = Fx::new();
        fx.w(".gitignore", b"*.log\n");
        fx.w("a", b"1");
        let st = fx.store();
        snap(&fx, &st);
        fx.w("a", b"2");
        fx.w("debug.log", b"precious");
        st.restore_in(&fx.env(), 1).unwrap();
        assert_eq!(fx.r("a"), b"1");
        assert_eq!(fx.r("debug.log"), b"precious");
    }

    #[test]
    fn ignored_collision_is_saved_in_before() {
        let fx = Fx::new();
        fx.w("build.txt", b"v1");
        let st = fx.store();
        snap(&fx, &st);
        fx.w(".gitignore", b"build.txt\n");
        fx.w("build.txt", b"v2 precious");
        let res = st.restore_in(&fx.env(), 1).unwrap();
        assert_eq!(fx.r("build.txt"), b"v1");
        assert_eq!(
            blob_at(&fx, &commit_of(&st, res.before_n), "build.txt"),
            b"v2 precious"
        );
    }

    #[test]
    fn targets_gitignore_spares_a_new_file() {
        let fx = Fx::new();
        fx.w(".gitignore", b"*.log\n");
        fx.w("a", b"1");
        let st = fx.store();
        snap(&fx, &st);
        fs::remove_file(fx.repo.join(".gitignore")).unwrap();
        fx.w("late.log", b"keep me");
        st.restore_in(&fx.env(), 1).unwrap();
        assert!(fx.exists(".gitignore"));
        assert_eq!(fx.r("late.log"), b"keep me");
    }

    #[test]
    fn directory_with_an_ignored_file_is_not_removed() {
        let fx = Fx::new();
        fx.w(".gitignore", b"*.log\n");
        fx.w("top.txt", b"t");
        let st = fx.store();
        snap(&fx, &st);
        fx.w("d/a.txt", b"a");
        fx.w("d/x.log", b"ignored");
        fx.w("e/only.txt", b"o");
        let res = st.restore_in(&fx.env(), 1).unwrap();
        assert!(!fx.exists("d/a.txt"));
        assert_eq!(fx.r("d/x.log"), b"ignored");
        assert!(!fx.exists("e"));
        assert_eq!(res.deleted, 2);
    }

    #[test]
    fn file_changed_after_capture_is_left_as_is() {
        let fx = Fx::new();
        fx.w("a.txt", b"one");
        fx.w("keep.txt", b"k");
        let st = fx.store();
        snap(&fx, &st);
        fx.w("a.txt", b"two");
        fx.w("gone.txt", b"g");
        let repo = fx.repo.clone();
        let hook = move || {
            fs::write(repo.join("a.txt"), b"written by the agent").unwrap();
            fs::write(repo.join("gone.txt"), b"changed too").unwrap();
            fs::write(repo.join("late.txt"), b"late").unwrap();
        };
        let env = Env {
            after_before: Some(&hook),
            ..fx.env()
        };
        let res = st.restore_in(&env, 1).unwrap();
        assert_eq!(fx.r("a.txt"), b"written by the agent");
        assert_eq!(fx.r("gone.txt"), b"changed too");
        assert_eq!(fx.r("late.txt"), b"late");
        assert_eq!(res.left_as_is, ["a.txt", "gone.txt"]);
        assert_eq!(res.written, 0);
        assert_eq!(res.deleted, 0);
    }

    #[test]
    fn symlinked_parent_is_not_captured() {
        let fx = Fx::new();
        fx.w("d/secret", b"inside");
        fx.commit_all("c");
        fs::remove_dir_all(fx.repo.join("d")).unwrap();
        fs::write(fx.out.join("secret"), b"OUTSIDE").unwrap();
        symlink(&fx.out, fx.repo.join("d")).unwrap();
        let st = fx.store();
        snap(&fx, &st);
        let c = commit_of(&st, 1);
        let paths = tree_paths(&fx, &c);
        assert!(!paths.contains(&"d/secret".to_string()), "{paths:?}");
        for p in &paths {
            assert_ne!(blob_at(&fx, &c, p), b"OUTSIDE");
        }
    }

    #[test]
    fn symlinked_parent_is_not_followed_on_restore() {
        let fx = Fx::new();
        fx.w("d/f", b"inner");
        fx.w("top", b"1");
        let st = fx.store();
        snap(&fx, &st);
        fs::remove_dir_all(fx.repo.join("d")).unwrap();
        fs::write(fx.out.join("f"), b"OUTSIDE").unwrap();
        symlink(&fx.out, fx.repo.join("d")).unwrap();
        st.restore_in(&fx.env(), 1).unwrap();
        assert_eq!(fs::read(fx.out.join("f")).unwrap(), b"OUTSIDE");
        assert_eq!(fs::read_dir(&fx.out).unwrap().count(), 1);
        assert!(fs::symlink_metadata(fx.repo.join("d")).unwrap().is_dir());
        assert_eq!(fx.r("d/f"), b"inner");
    }

    #[test]
    fn symlinked_leaf_is_replaced_not_written_through() {
        let fx = Fx::new();
        fx.w("f", b"inner");
        let st = fx.store();
        snap(&fx, &st);
        fs::remove_file(fx.repo.join("f")).unwrap();
        fs::write(fx.out.join("f"), b"OUTSIDE").unwrap();
        symlink(fx.out.join("f"), fx.repo.join("f")).unwrap();
        st.restore_in(&fx.env(), 1).unwrap();
        assert_eq!(fs::read(fx.out.join("f")).unwrap(), b"OUTSIDE");
        assert_eq!(fx.r("f"), b"inner");
        assert!(!fs::symlink_metadata(fx.repo.join("f"))
            .unwrap()
            .is_symlink());
    }

    #[test]
    fn symlink_in_the_way_of_a_directory_keeps_the_outside_intact() {
        let fx = Fx::new();
        fx.w("a/b/c", b"deep");
        let st = fx.store();
        snap(&fx, &st);
        fs::remove_dir_all(fx.repo.join("a")).unwrap();
        fs::create_dir_all(fx.out.join("b")).unwrap();
        fs::write(fx.out.join("b/c"), b"OUTSIDE").unwrap();
        symlink(&fx.out, fx.repo.join("a")).unwrap();
        st.restore_in(&fx.env(), 1).unwrap();
        assert_eq!(fs::read(fx.out.join("b/c")).unwrap(), b"OUTSIDE");
        assert_eq!(fs::read_dir(&fx.out).unwrap().count(), 1);
    }

    #[test]
    fn forged_ref_is_refused() {
        let fx = Fx::new();
        fx.w("a", b"1");
        let st = fx.store();
        snap(&fx, &st);
        fx.w("a", b"2");
        snap(&fx, &st);
        let other = commit_of(&st, 2);
        fx.pg(&[
            "update-ref",
            &format!("refs/tmgr/checkpoints/{S1}/1"),
            &other,
        ]);
        let err = st.restore_in(&fx.env(), 1).unwrap_err();
        assert!(matches!(err, CpError::Refused(_)), "{err:?}");
        assert_eq!(fx.r("a"), b"2");
        assert_eq!(st.list().len(), 2);

        fx.pg(&["update-ref", "-d", &format!("refs/tmgr/checkpoints/{S1}/1")]);
        assert!(matches!(
            st.restore_in(&fx.env(), 1),
            Err(CpError::Refused(_))
        ));
        assert!(matches!(
            st.restore_in(&fx.env(), 99),
            Err(CpError::Refused(_))
        ));
        assert_eq!(fx.r("a"), b"2");
    }

    #[test]
    fn replace_refs_cannot_swap_restored_content() {
        let fx = Fx::new();
        fx.w("a", b"one");
        let st = fx.store();
        snap(&fx, &st);
        let good = fx.pg(&["hash-object", "a"]);
        let (_, evil) = pg_in(
            &fx.repo,
            &fx.home,
            &["hash-object", "-w", "--stdin"],
            b"EVIL",
        );
        let evil = String::from_utf8(evil).unwrap().trim().to_string();
        fx.pg(&["update-ref", &format!("refs/replace/{good}"), &evil]);
        assert_eq!(fx.pgb(&["cat-file", "-p", &good]), b"EVIL");
        fx.w("a", b"two");
        st.restore_in(&fx.env(), 1).unwrap();
        assert_eq!(fx.r("a"), b"one");
    }

    #[test]
    fn planted_core_worktree_is_not_a_repository_for_the_session() {
        let fx = Fx::new();
        fx.w("a", b"1");
        let elsewhere = fx.root.join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        fx.pg(&["config", "core.worktree", elsewhere.to_str().unwrap()]);
        let st = fx.store();
        let err = st.snapshot_in(&fx.env()).unwrap_err();
        assert!(matches!(err, CpError::NotGit(_)), "{err:?}");
        assert!(refs_under(&fx, "refs/tmgr").is_empty());
    }

    #[test]
    fn restore_keeps_permission_bits_and_follows_the_exec_bit() {
        let fx = Fx::new();
        fx.w("private", b"1");
        fx.w("tool", b"1");
        let mode = |p: &str, m: u32| {
            fs::set_permissions(fx.repo.join(p), fs::Permissions::from_mode(m)).unwrap()
        };
        let bits = |p: &str| fs::metadata(fx.repo.join(p)).unwrap().permissions().mode() & 0o777;
        mode("private", 0o600);
        mode("tool", 0o755);
        let st = fx.store();
        snap(&fx, &st);
        fx.w("private", b"2");
        fx.w("tool", b"2");
        mode("tool", 0o640);
        st.restore_in(&fx.env(), 1).unwrap();
        assert_eq!(fx.r("private"), b"1");
        assert_eq!(bits("private"), 0o600);
        assert_eq!(fx.r("tool"), b"1");
        assert_eq!(bits("tool"), 0o750);
    }

    #[test]
    fn non_utf8_paths_are_valid_but_never_cached() {
        let odd = b"caf\xe9/\xff.txt";
        assert!(valid_path(odd));
        assert!(!valid_path(b"a/../b"));
        assert!(!valid_path(b"/abs"));
        assert!(!valid_path(b"a//b"));
        assert!(!valid_path(b"a/.Git/b"));
        assert!(!valid_path(b"GIT~1/x"));
        assert!(!valid_path(b".git /x"));
        assert!(!valid_path(b".g\xe2\x80\x8cit/x"));
        assert!(valid_path(b".github/x"));
        let fp = Fp {
            dev: 1,
            ino: 2,
            size: 3,
            mt_s: 0,
            mt_ns: 0,
            ct_s: 0,
            ct_ns: 0,
            mode: 0o100644,
        };
        let mut cache = Cache::default();
        cache.files.insert(
            "plain".into(),
            CacheEntry {
                fp,
                oid: "a".repeat(40),
            },
        );
        assert!(cached_oid(&cache, b"plain", &fp, 1_000_000).is_some());
        assert!(cached_oid(&cache, odd, &fp, 1_000_000).is_none());
        assert!(cached_oid(&cache, b"plain", &fp, 1).is_none());
    }

    fn raw_tree(fx: &Fx, entries: &[(&str, &str, &str)]) -> String {
        let mut buf = Vec::new();
        for (mode, name, oid) in entries {
            buf.extend_from_slice(mode.as_bytes());
            buf.push(b' ');
            buf.extend_from_slice(name.as_bytes());
            buf.push(0);
            for i in (0..oid.len()).step_by(2) {
                buf.push(u8::from_str_radix(&oid[i..i + 2], 16).unwrap());
            }
        }
        let (ok, out) = pg_in(
            &fx.repo,
            &fx.home,
            &["hash-object", "-t", "tree", "-w", "--literally", "--stdin"],
            &buf,
        );
        assert!(ok);
        String::from_utf8(out).unwrap().trim().to_string()
    }

    #[test]
    fn forged_trees_are_refused_without_any_write() {
        let fx = Fx::new();
        fx.w("a", b"1");
        let st = fx.store();
        snap(&fx, &st);
        fx.w("a", b"2");
        snap(&fx, &st);
        let blob = {
            let (ok, out) = pg_in(
                &fx.repo,
                &fx.home,
                &["hash-object", "-w", "--stdin"],
                b"EVIL",
            );
            assert!(ok);
            String::from_utf8(out).unwrap().trim().to_string()
        };
        let evil_dir = raw_tree(&fx, &[("100644", "evil", &blob)]);
        let dotdot_in_a = raw_tree(&fx, &[("40000", "..", &evil_dir)]);
        let git_cfg = raw_tree(&fx, &[("100644", "config", &blob)]);
        let sub_git = raw_tree(&fx, &[("100644", ".git", &blob)]);
        let mk = |e: (&str, &str, &str)| raw_tree(&fx, &[("100644", "ok.txt", &blob), e]);
        let cases: Vec<(&str, String)> = vec![
            ("top dotdot", mk(("40000", "..", &evil_dir))),
            ("nested dotdot", mk(("40000", "a", &dotdot_in_a))),
            ("dot", mk(("100644", ".", &blob))),
            ("dotgit file", mk(("100644", ".git", &blob))),
            ("dotgit upper", mk(("100644", ".GIT", &blob))),
            ("dotgit dir", mk(("40000", ".git", &git_cfg))),
            ("sub dotgit", mk(("40000", "sub", &sub_git))),
            ("gitlink", mk(("160000", "sm", &blob))),
        ];
        for (label, tree) in cases {
            let commit = fx.pg(&["commit-tree", &tree, "-m", "forged"]);
            let mut m = st.load();
            m.items[0].commit = commit.clone();
            m.items[0].tree = tree;
            st.save(&m).unwrap();
            fx.pg(&[
                "update-ref",
                &format!("refs/tmgr/checkpoints/{S1}/1"),
                &commit,
            ]);
            let before = fx.dump();
            let res = st.restore_in(&fx.env(), 1);
            assert!(matches!(res, Err(CpError::Refused(_))), "{label}: {res:?}");
            assert_eq!(fx.dump(), before, "{label}");
            assert_eq!(st.list().len(), 2, "{label}");
            assert!(!fx.root.join("evil").exists(), "{label}");
            assert!(!fx.repo.join("evil").exists(), "{label}");
            assert_eq!(fs::read_dir(&fx.out).unwrap().count(), 0, "{label}");
        }
    }

    #[test]
    fn retention_keeps_the_last_n_and_removes_refs() {
        let fx = Fx::new();
        let mut st = fx.store();
        st.keep = 3;
        for i in 0..6 {
            fx.w("a", format!("{i}").as_bytes());
            snap(&fx, &st);
        }
        let ns: Vec<u64> = st.list().iter().map(|i| i.n).collect();
        assert_eq!(ns, [4, 5, 6]);
        let refs = refs_under(&fx, "refs/tmgr");
        assert_eq!(refs.len(), 3);
        assert!(refs
            .iter()
            .all(|r| !r.ends_with("/1") && !r.ends_with("/3")));
        st.keep = 1;
        fx.w("a", b"more");
        snap(&fx, &st);
        assert_eq!(st.list().len(), 3);
    }

    #[test]
    fn retention_never_drops_the_restore_pair() {
        let fx = Fx::new();
        let mut st = fx.store();
        st.keep = 3;
        fx.w("a", b"0");
        snap(&fx, &st);
        for i in 1..=3 {
            fx.w("a", format!("{i}").as_bytes());
            snap(&fx, &st);
        }
        let res = st.restore_in(&fx.env(), st.list()[0].n).unwrap();
        let ns: Vec<u64> = st.list().iter().map(|i| i.n).collect();
        assert!(ns.contains(&res.n), "{ns:?}");
        assert!(ns.contains(&res.before_n));
        assert!(ns.contains(&res.after_n));
        assert_eq!(fx.r("a"), b"1");
    }

    #[test]
    fn hooks_filters_fsmonitor_and_signing_never_run() {
        let fx = Fx::new();
        let hooks = fx.repo.join(".git/hooks");
        let out = fx.out.display().to_string();
        script(
            &hooks.join("reference-transaction"),
            &format!("touch {out}/ref"),
        );
        script(
            &hooks.join("post-index-change"),
            &format!("touch {out}/idx"),
        );
        let fsmon = fx.root.join("fsmon.sh");
        script(&fsmon, &format!("touch {out}/fsmon\nprintf '\\0'"));
        let filter = fx.root.join("filter.sh");
        script(&filter, &format!("touch {out}/filter\ncat"));
        let gpg = fx.root.join("gpg.sh");
        script(&gpg, &format!("touch {out}/gpg\nexit 1"));
        fx.pg(&["config", "core.fsmonitor", fsmon.to_str().unwrap()]);
        fx.pg(&["config", "filter.x.clean", filter.to_str().unwrap()]);
        fx.pg(&["config", "gpg.program", gpg.to_str().unwrap()]);
        fx.w(".gitattributes", b"* filter=x\n");
        fx.w("a", b"1");

        fx.pg(&["add", "-A"]);
        fx.pg(&["-c", "commit.gpgsign=false", "commit", "-q", "-m", "c"]);
        fx.pg(&["update-ref", "refs/heads/zz", "HEAD"]);
        fx.pg(&["status", "--porcelain"]);
        fx.try_pg(&[
            "-c",
            "commit.gpgsign=true",
            "commit",
            "--allow-empty",
            "-m",
            "s",
        ]);
        for c in ["ref", "idx", "fsmon", "filter", "gpg"] {
            assert!(fx.out.join(c).exists(), "positive control {c}");
            fs::remove_file(fx.out.join(c)).unwrap();
        }

        fx.pg(&["config", "commit.gpgsign", "true"]);
        let st = fx.store();
        snap(&fx, &st);
        fx.w("a", b"2");
        fx.w("b", b"new");
        snap(&fx, &st);
        st.diff_in(&fx.env(), 2, DiffAgainst::Previous).unwrap();
        st.diff_in(&fx.env(), 1, DiffAgainst::Current).unwrap();
        st.restore_in(&fx.env(), 1).unwrap();
        assert_eq!(fs::read_dir(&fx.out).unwrap().count(), 0);
    }

    #[test]
    fn local_ssh_command_and_credential_config_do_not_block() {
        let fx = Fx::new();
        fx.pg(&["config", "core.sshCommand", "false"]);
        fx.pg(&["config", "credential.helper", "false"]);
        fx.pg(&["config", "diff.external", "false"]);
        fx.w("a", b"1");
        let st = fx.store();
        snap(&fx, &st);
        fx.w("a", b"2");
        snap(&fx, &st);
        st.diff_in(&fx.env(), 2, DiffAgainst::Previous).unwrap();
        st.restore_in(&fx.env(), 1).unwrap();
        assert_eq!(fx.r("a"), b"1");
    }

    #[test]
    fn over_cap_file_is_untouched_and_counted() {
        let fx = Fx::new();
        fx.w("small", b"tiny");
        let st = fx.store();
        let env = Env {
            max_file: 10,
            ..fx.env()
        };
        st.snapshot_in(&env).unwrap().unwrap();
        fx.w("big.bin", &[7u8; 20]);
        fx.w("small", b"tiny2");
        let item = st.snapshot_in(&env).unwrap().unwrap();
        assert_eq!(item.skipped, 1);
        assert_eq!(tree_paths(&fx, &commit_of(&st, 2)), ["small"]);
        let res = st.restore_in(&env, 1).unwrap();
        assert_eq!(fx.r("small"), b"tiny");
        assert_eq!(fx.r("big.bin"), [7u8; 20]);
        assert!(res.left_as_is.is_empty());
        assert!(st.list().last().unwrap().skipped >= 1);
    }

    #[test]
    fn total_cap_skips_the_overflow() {
        let fx = Fx::new();
        fx.w("a", b"123456");
        fx.w("b", b"123456");
        let st = fx.store();
        let env = Env {
            max_total: 10,
            ..fx.env()
        };
        let item = st.snapshot_in(&env).unwrap().unwrap();
        assert_eq!(item.skipped, 1);
        assert_eq!(tree_paths(&fx, &commit_of(&st, 1)), ["a"]);
    }

    #[test]
    fn hard_linked_file_is_skipped() {
        let fx = Fx::new();
        fx.w("a", b"1");
        fs::hard_link(fx.repo.join("a"), fx.repo.join("b")).unwrap();
        fx.w("c", b"3");
        let st = fx.store();
        let item = snap(&fx, &st);
        assert_eq!(item.skipped, 2);
        assert_eq!(tree_paths(&fx, &commit_of(&st, 1)), ["c"]);
    }

    #[test]
    fn subdirectory_cwd_scopes_capture_and_restore() {
        let fx = Fx::new();
        fx.w("top.txt", b"top");
        fx.w("sub/in.txt", b"in");
        fx.w("sub/dir/f", b"f");
        let mut st = fx.store();
        st.cwd = fx.repo.join("sub");
        snap(&fx, &st);
        assert_eq!(tree_paths(&fx, &commit_of(&st, 1)), ["dir/f", "in.txt"]);
        fx.w("top.txt", b"top changed");
        fx.w("sub/in.txt", b"in changed");
        fx.w("sub/new.txt", b"n");
        fx.w("other/x", b"x");
        st.restore_in(&fx.env(), 1).unwrap();
        assert_eq!(fx.r("top.txt"), b"top changed");
        assert_eq!(fx.r("other/x"), b"x");
        assert_eq!(fx.r("sub/in.txt"), b"in");
        assert!(!fx.exists("sub/new.txt"));
    }

    #[test]
    fn not_a_repository_is_reported_and_recovers() {
        let fx = Fx::new();
        let plain = fx.root.join("plain");
        fs::create_dir_all(&plain).unwrap();
        fs::write(plain.join("a"), b"1").unwrap();
        let mut st = fx.store();
        st.cwd = plain.clone();
        let err = st.snapshot_in(&fx.env()).unwrap_err();
        assert!(matches!(err, CpError::NotGit(_)), "{err:?}");
        let status = st.status();
        assert!(!status.available);
        assert_eq!(status.note.as_deref(), Some("not a git repository"));
        assert!(st.list().is_empty());
        pg_in(&plain, &fx.home, &["init", "-q"], b"");
        st.snapshot_in(&fx.env()).unwrap().unwrap();
        let status = st.status();
        assert!(status.available);
        assert_eq!(status.note, None);
    }

    #[test]
    fn nested_repository_is_not_captured() {
        let fx = Fx::new();
        fx.w("a", b"1");
        let nested = fx.repo.join("nested");
        fs::create_dir_all(&nested).unwrap();
        pg_in(&nested, &fx.home, &["init", "-q"], b"");
        fs::write(nested.join("secret"), b"s").unwrap();
        let st = fx.store();
        snap(&fx, &st);
        assert_eq!(tree_paths(&fx, &commit_of(&st, 1)), ["a"]);
    }

    #[test]
    fn extra_paths_under_git_dir_are_not_captured() {
        let fx = Fx::new();
        fx.w("a", b"1");
        let st = fx.store();
        let env = fx.env();
        let op = Op::new(&st, &env);
        op.repo().unwrap();
        let cap = op
            .capture(&[b".git/HEAD".to_vec(), b".GIT/config".to_vec()])
            .unwrap();
        assert_eq!(cap.entries.keys().collect::<Vec<_>>(), [&b"a".to_vec()]);
        assert_eq!(fx.pg(&["ls-tree", "-r", "--name-only", &cap.tree]), "a");
    }

    fn backdate(path: &Path) {
        let c = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        let t = libc::timespec {
            tv_sec: (SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                - 3600) as libc::time_t,
            tv_nsec: 0,
        };
        let times = [t, t];
        assert_eq!(
            unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), times.as_ptr(), 0) },
            0
        );
    }

    fn cache_json(st: &Store) -> serde_json::Value {
        serde_json::from_slice(&fs::read(st.sdir.join("checkpoints.cache.json")).unwrap()).unwrap()
    }

    #[test]
    fn stat_cache_skips_old_files_only() {
        let fx = Fx::new();
        fx.w("old", b"old");
        fx.w("fresh", b"fresh");
        backdate(&fx.repo.join("old"));
        let st = fx.store();
        snap(&fx, &st);
        let cache = cache_json(&st);
        let text = cache.to_string();
        assert!(text.contains("\"old\""), "{text}");
        assert!(!text.contains("\"fresh\""), "{text}");

        fx.w("old", b"changed content");
        backdate(&fx.repo.join("old"));
        snap(&fx, &st);
        assert_eq!(blob_at(&fx, &commit_of(&st, 2), "old"), b"changed content");
    }

    #[test]
    fn stat_cache_hit_reuses_the_recorded_object() {
        let fx = Fx::new();
        fx.w("old", b"old");
        fx.w("other", b"o1");
        backdate(&fx.repo.join("old"));
        let st = fx.store();
        snap(&fx, &st);
        let (_, d) = pg_in(
            &fx.repo,
            &fx.home,
            &["hash-object", "-w", "--stdin"],
            b"DECOY",
        );
        let decoy = String::from_utf8(d).unwrap().trim().to_string();
        let mut cache = cache_json(&st);
        cache["files"]["old"]["oid"] = serde_json::Value::String(decoy);
        fs::write(st.sdir.join("checkpoints.cache.json"), cache.to_string()).unwrap();
        fx.w("other", b"o2");
        snap(&fx, &st);
        assert_eq!(blob_at(&fx, &commit_of(&st, 2), "old"), b"DECOY");
    }

    #[test]
    fn stale_cache_object_is_recovered_by_one_retry() {
        let fx = Fx::new();
        fx.w("old", b"old");
        fx.w("other", b"o1");
        backdate(&fx.repo.join("old"));
        let st = fx.store();
        snap(&fx, &st);
        let ghost = "1234567890123456789012345678901234567890".to_string();
        let mut cache = cache_json(&st);
        cache["files"]["old"]["oid"] = serde_json::Value::String(ghost);
        fs::write(st.sdir.join("checkpoints.cache.json"), cache.to_string()).unwrap();
        fx.w("other", b"o2");
        snap(&fx, &st);
        assert_eq!(blob_at(&fx, &commit_of(&st, 2), "old"), b"old");
    }

    #[test]
    fn diff_previous_and_current() {
        let fx = Fx::new();
        fx.w("a.txt", b"one\n");
        fx.w("gone.txt", b"g\n");
        let st = fx.store();
        snap(&fx, &st);
        fx.w("a.txt", b"two\n");
        fs::remove_file(fx.repo.join("gone.txt")).unwrap();
        fx.w("new.txt", b"n\x1b[31mred\n");
        snap(&fx, &st);
        let d = st.diff_in(&fx.env(), 2, DiffAgainst::Previous).unwrap();
        assert_eq!((d.added, d.modified, d.deleted), (1, 1, 1));
        assert!(d.text.contains("+two"), "{}", d.text);
        assert!(!d.text.contains('\x1b'));
        assert!(!d.truncated);

        let first = st.diff_in(&fx.env(), 1, DiffAgainst::Previous).unwrap();
        assert_eq!((first.added, first.modified, first.deleted), (2, 0, 0));

        fx.w("a.txt", b"three\n");
        fx.w("extra", b"x");
        let cur = st.diff_in(&fx.env(), 1, DiffAgainst::Current).unwrap();
        assert_eq!((cur.added, cur.modified, cur.deleted), (2, 1, 1));
        let missing = st.diff_in(&fx.env(), 99, DiffAgainst::Previous);
        assert!(matches!(missing, Err(CpError::Refused(_))));
    }

    #[test]
    fn large_diff_is_truncated() {
        let fx = Fx::new();
        fx.w("a", b"x\n");
        let st = fx.store();
        snap(&fx, &st);
        let big = "line of text to fill the diff\n".repeat(60_000);
        fx.w("big.txt", big.as_bytes());
        snap(&fx, &st);
        let d = st.diff_in(&fx.env(), 2, DiffAgainst::Previous).unwrap();
        assert!(d.truncated);
        assert!(d.text.len() <= 1024 * 1024);
        assert_eq!(d.added, 1);
    }

    #[test]
    fn remove_all_deletes_refs_and_state() {
        let fx = Fx::new();
        fx.w("a", b"1");
        let st = fx.store();
        snap(&fx, &st);
        fx.w("a", b"2");
        snap(&fx, &st);
        let other_sdir = fx.root.join("sdir2");
        fs::create_dir_all(&other_sdir).unwrap();
        let other = Store {
            session: S2.to_string(),
            sdir: other_sdir,
            ..fx.store()
        };
        snap(&fx, &other);
        st.remove_all();
        assert_eq!(
            refs_under(&fx, "refs/tmgr"),
            [format!("refs/tmgr/checkpoints/{S2}/1")]
        );
        assert!(!st.sdir.join("checkpoints.json").exists());
        assert!(!st.sdir.join("checkpoints.cache.json").exists());
        assert!(st.list().is_empty());
    }

    #[test]
    fn transfer_moves_history_to_the_new_session() {
        let fx = Fx::new();
        fx.w("a", b"1");
        let st = fx.store();
        snap(&fx, &st);
        fx.w("a", b"2");
        snap(&fx, &st);
        let to_sdir = fx.root.join("sdir2");
        fs::create_dir_all(&to_sdir).unwrap();
        let to = Store {
            session: S2.to_string(),
            sdir: to_sdir,
            cwd: fx.repo.clone(),
            keep: 50,
        };
        let items = st.list();
        st.transfer(&to).unwrap();
        assert_eq!(to.list(), items);
        assert!(st.list().is_empty());
        assert_eq!(
            refs_under(&fx, "refs/tmgr"),
            [
                format!("refs/tmgr/checkpoints/{S2}/1"),
                format!("refs/tmgr/checkpoints/{S2}/2")
            ]
        );
        fx.w("a", b"3");
        to.restore_in(&fx.env(), 1).unwrap();
        assert_eq!(fx.r("a"), b"1");
        assert_eq!(to.list().last().unwrap().n, 4);
    }

    #[test]
    fn invalid_session_id_is_refused() {
        let fx = Fx::new();
        fx.w("a", b"1");
        let st = Store {
            session: "../x".into(),
            ..fx.store()
        };
        assert!(st.snapshot_in(&fx.env()).is_err());
        assert!(refs_under(&fx, "refs/tmgr").is_empty());
    }
}
