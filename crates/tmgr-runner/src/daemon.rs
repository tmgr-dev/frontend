use crate::agentwork;
use crate::bundle;
use crate::client::Client;
use crate::clone;
use crate::exe::{resolve_exe, verify_pin};
use crate::login;
use crate::profile::{self, Matcher, Profile, ProfileSet};
use crate::restore;
use crate::sandbox::{self, SandboxSpec};
use crate::screen::ScreenModel;
use crate::seedguard;
use crate::t0::{self, Detector};
use crate::trust;
use crate::wire::{
    clamp_size, now_ms, screen_pushes, write_atomic, LaunchFile, Slot, MAX_INPUT, SCROLLBACK,
    SHIM_CWD_FD,
};
use crate::worklog;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom};
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread;
use std::time::{Duration, Instant};
use tmgr_ipc::api::{
    limits, method, CreateReq, CwdPin, ErrorCode, EventMsg, ExePin, LookupParams, Origin,
    OriginRevokeParams, OriginRevoked, RestoredRun, RunCreateReq, RunStatus, SessionInfo,
    SessionText, Snapshot, State, StateKind, StatusInfo, TaskRef, TextParams, CLIENT_CLI,
    CLIENT_WORKER,
};
use tmgr_ipc::dirs::{self, ensure_private_dir, ensure_private_leaf};
use tmgr_ipc::endpoint::{self, Endpoint};
use tmgr_ipc::frame::{
    read_frame, write_frame, write_json, BinFrame, Frame, TAG_INPUT, TAG_REPLAY,
};
use tmgr_ipc::proto::{
    new_session_id, valid_session_id, ClientMsg, ErrorBody, ExitInfo, FromShim, Request, Response,
    ServerMsg, ShimHello, ToShim, PROTO_VERSION,
};
use tmgr_ipc::{auth, secret, server};

const MAX_CONNS: usize = 64;
const SHIM_WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const SHIM_QUEUE: usize = 1024;
const SHIM_INPUT_CAP: usize = 1 << 20;
const SHIM_INPUT_FRAMES: usize = 512;
const MAX_REPLIES_PER_SEC: u32 = 32;
const MAX_PENDING: usize = 16;
const REJECT_LOG_EVERY: Duration = Duration::from_secs(10);
const PUSH_INTERVAL: Duration = Duration::from_millis(33);
const MAX_LAYOUT: usize = 64 << 10;
const MAX_WATCHED: usize = 64;
const LOG_TAIL: u64 = 2 << 20;
const PROTECTED_IN_HOME: [&str; 10] = [
    ".ssh",
    ".gnupg",
    ".aws",
    ".config/tmgr",
    ".tmgr.dev",
    ".mozilla",
    ".config/google-chrome",
    "Library/Keychains",
    "Library/Application Support/Google/Chrome",
    "Library/Application Support/Firefox",
];

fn profile_protected_paths() -> Result<Vec<PathBuf>, ApiErr> {
    let dir = profile::user_dir();
    let decls = profile::secrets_declarations(dir.as_deref()).map_err(|e| {
        api(
            ErrorCode::Sandbox,
            format!("cannot establish the protected paths of the agent profiles, no session starts: {e}"),
        )
    })?;
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    let mut out = login::profile_protected(dir.as_deref(), &decls, &home).map_err(|e| {
        api(
            ErrorCode::Sandbox,
            format!("cannot establish the protected paths of the agent profiles, no session starts: {e}"),
        )
    })?;
    if let Ok(store) = trust::store_dir() {
        let chain = login::path_chain(&store).map_err(|e| {
            api(
                ErrorCode::Sandbox,
                format!("cannot establish the protected trust store path, no session starts: {e}"),
            )
        })?;
        for p in chain {
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    Ok(out)
}

#[derive(Clone)]
pub struct RunnerConfig {
    pub dir: PathBuf,
    pub homes: PathBuf,
    pub self_exe: PathBuf,
    pub quiet_ms: u64,
    pub version: String,
    pub max_sessions: usize,
}

impl RunnerConfig {
    pub fn from_env(self_exe: PathBuf, version: String) -> io::Result<RunnerConfig> {
        Ok(RunnerConfig {
            dir: dirs::runner_dir()?,
            homes: dirs::homes_dir()?,
            self_exe,
            quiet_ms: t0::quiet_ms_from_env(),
            version,
            max_sessions: 16,
        })
    }
}

#[cfg(target_os = "linux")]
pub fn landlock_abi() -> i32 {
    const SYS_LANDLOCK_CREATE_RULESET: libc::c_long = 444;
    const CREATE_RULESET_VERSION: libc::c_ulong = 1;
    let v = unsafe {
        libc::syscall(
            SYS_LANDLOCK_CREATE_RULESET,
            std::ptr::null::<libc::c_void>(),
            0 as libc::size_t,
            CREATE_RULESET_VERSION,
        )
    };
    i32::try_from(v).unwrap_or(0).max(0)
}

#[cfg(not(target_os = "linux"))]
pub fn landlock_abi() -> i32 {
    i32::MAX
}

fn hold_before_spawn() {
    hold_at("TMGR_TEST_HOLD_BEFORE_SPAWN")
}

#[cfg(debug_assertions)]
fn hold_at(var: &str) {
    let Some(go) = std::env::var_os(var).map(PathBuf::from) else {
        return;
    };
    let _ = fs::write(go.with_extension("reached"), "");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !go.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
}

#[cfg(not(debug_assertions))]
fn hold_at(_var: &str) {}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn log(msg: &str) {
    eprintln!("{} {msg}", now_ms() / 1000);
}

struct ApiErr {
    code: ErrorCode,
    msg: String,
}

fn api(code: ErrorCode, msg: impl Into<String>) -> ApiErr {
    ApiErr {
        code,
        msg: msg.into(),
    }
}

fn params<T: DeserializeOwned>(v: &Value) -> Result<T, ApiErr> {
    serde_json::from_value(v.clone()).map_err(|e| api(ErrorCode::InvalidParams, e.to_string()))
}

fn to_json<T: Serialize>(v: &T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

const CLIENT_QUEUE: usize = 256;

#[derive(Default)]
struct Hub {
    subs: Mutex<Vec<SyncSender<Value>>>,
}

impl Hub {
    fn subscribe(&self) -> Receiver<Value> {
        let (tx, rx) = mpsc::sync_channel(CLIENT_QUEUE);
        lock(&self.subs).push(tx);
        rx
    }

    fn broadcast(&self, v: Value) {
        lock(&self.subs).retain(|s| s.try_send(v.clone()).is_ok());
    }
}

fn write_timeout() -> Duration {
    std::env::var("TMGR_RUNNER_WRITE_TIMEOUT_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .map_or(Duration::from_secs(10), Duration::from_millis)
}

type OriginArg<'a> = (&'a Origin, Option<&'a str>);

#[derive(Serialize, Deserialize, Clone)]
struct Meta {
    id: String,
    name: String,
    argv: Vec<String>,
    cwd: String,
    profile: Option<String>,
    created_ms: u64,
    cols: u16,
    rows: u16,
    shim_pid: u32,
    child_pid: u32,
    #[serde(default)]
    last_state: Option<State>,
    #[serde(default)]
    task: Option<TaskRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bundle_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    origin: Option<Origin>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cwd_id: Option<(u64, u64)>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    bundled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    permission_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    clone_of: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    start_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    start_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    exit_signal: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trust_file: Option<String>,
}

#[derive(Default)]
struct RunExtra {
    permission_mode: Option<String>,
    clone_of: Option<String>,
    objects: Option<clone::Objects>,
    start: Option<(String, String)>,
    trust: Option<profile::TrustDecl>,
}

struct Inner {
    meta: Meta,
    screen: ScreenModel,
    det: Detector,
    matcher: Option<Matcher>,
    fixed: Option<State>,
    size_owner: Option<u64>,
}

struct ShimLink {
    tx: SyncSender<Frame>,
    queued_bytes: Arc<AtomicUsize>,
    queued_frames: Arc<AtomicUsize>,
}

impl ShimLink {
    fn spawn(stream: &UnixStream) -> io::Result<ShimLink> {
        let mut out = stream.try_clone()?;
        out.set_write_timeout(Some(SHIM_WRITE_TIMEOUT))?;
        let (tx, rx) = mpsc::sync_channel::<Frame>(SHIM_QUEUE);
        let queued_bytes = Arc::new(AtomicUsize::new(0));
        let queued_frames = Arc::new(AtomicUsize::new(0));
        let (b, f) = (queued_bytes.clone(), queued_frames.clone());
        thread::spawn(move || {
            while let Ok(frame) = rx.recv() {
                let input_len = match &frame {
                    Frame::Bin(bin) if bin.tag == TAG_INPUT => Some(bin.data.len()),
                    _ => None,
                };
                let res = write_frame(&mut out, &frame);
                if let Some(n) = input_len {
                    b.fetch_sub(n, Ordering::SeqCst);
                    f.fetch_sub(1, Ordering::SeqCst);
                }
                if res.is_err() {
                    let _ = out.shutdown(std::net::Shutdown::Both);
                    break;
                }
            }
        });
        Ok(ShimLink {
            tx,
            queued_bytes,
            queued_frames,
        })
    }
}

struct ReplyBudget {
    window: Instant,
    used: u32,
}

struct Session {
    id: String,
    dir: PathBuf,
    hub: Arc<Hub>,
    inner: Mutex<Inner>,
    worklog: worklog::Log,
    version: AtomicU64,
    shim: Mutex<Option<ShimLink>>,
    replies: Mutex<ReplyBudget>,
    drop_logged: AtomicBool,
    input_dropped: AtomicBool,
    meta_lock: Mutex<()>,
    sig_seq: AtomicU64,
    sig_acks: Mutex<HashMap<u64, bool>>,
    revoking: AtomicBool,
    retrying: AtomicBool,
    helper: PathBuf,
    aw_next: AtomicU64,
    aw_busy: Arc<AtomicBool>,
    aw_pending: Mutex<Option<agentwork::Intent>>,
    halted: AtomicBool,
}

fn write_meta(dir: &Path, meta: &Meta) {
    if let Ok(bytes) = serde_json::to_vec(meta) {
        let _ = write_atomic(&dir.join("meta.json"), &bytes);
    }
}

const REVOKING_MARK: &str = "revoking";
const REVOKE_WAIT: Duration = Duration::from_secs(2);

fn finish_seeds(sdir: &Path, id: &str) {
    let (sdir, id) = (sdir.to_path_buf(), id.to_string());
    thread::spawn(move || {
        for line in seedguard::finish(&sdir) {
            log(&format!("session {id}: {line}"));
        }
    });
}

fn terminate(s: &Session) -> bool {
    s.mark_revoking();
    let confirmed = s.kill_via_shim(Instant::now() + REVOKE_WAIT);
    if confirmed {
        s.mark_revoked();
    } else {
        log(&format!(
            "session {} revocation pending: the shim did not confirm the exit",
            s.id
        ));
    }
    confirmed
}

fn is_interrupted(s: &State) -> bool {
    s.kind == StateKind::Exited && s.detail.as_deref() == Some("interrupted")
}

fn interrupted_state() -> State {
    State {
        kind: StateKind::Exited,
        since_ms: now_ms(),
        detail: Some("interrupted".into()),
        exit_code: None,
    }
}

impl Session {
    fn effective(&self, i: &Inner) -> State {
        let s = i.fixed.clone().unwrap_or_else(|| i.det.state().clone());
        if !self.revoking.load(Ordering::SeqCst) {
            return s;
        }
        let s = if is_interrupted(&s) {
            i.det.state().clone()
        } else {
            s
        };
        if s.kind != StateKind::Exited {
            State {
                detail: Some("revoking".into()),
                ..s
            }
        } else {
            s
        }
    }

    fn state(&self) -> State {
        let i = lock(&self.inner);
        self.effective(&i)
    }

    fn info(&self) -> SessionInfo {
        let i = lock(&self.inner);
        let m = &i.meta;
        SessionInfo {
            id: m.id.clone(),
            name: m.name.clone(),
            argv: m.argv.clone(),
            cwd: m.cwd.clone(),
            profile: m.profile.clone(),
            state: self.effective(&i),
            created_ms: m.created_ms,
            cols: m.cols,
            rows: m.rows,
            shim_pid: m.shim_pid,
            child_pid: m.child_pid,
            task: m.task,
            bundle_sha256: m.bundle_sha256.clone(),
            origin: m.origin.clone(),
            permission_mode: m.permission_mode.clone(),
        }
    }

    fn start_key(&self) -> Option<(String, String)> {
        let i = lock(&self.inner);
        Some((i.meta.start_key.clone()?, i.meta.start_hash.clone()?))
    }

    fn run_status(&self) -> RunStatus {
        let alive = self.has_shim();
        let i = lock(&self.inner);
        let state = self.effective(&i);
        let exited = state.kind == StateKind::Exited;
        RunStatus {
            id: i.meta.id.clone(),
            alive: alive && !exited,
            state: state.kind,
            exit_code: state.exit_code.filter(|_| exited),
            signal: i.meta.exit_signal,
            started_ms: i.meta.created_ms,
            ended_ms: exited.then_some(state.since_ms),
        }
    }

    fn text(&self, lines: Option<u32>) -> SessionText {
        let mut i = lock(&self.inner);
        let (rows, cols, scroll_max, mut lines) = i.screen.text_lines(lines.map(|n| n as usize));
        cap_text(&mut lines, MAX_TEXT_BYTES);
        SessionText {
            id: i.meta.id.clone(),
            rows,
            cols,
            lines,
            scroll_max,
        }
    }

    fn holds_cwd(&self, id: (u64, u64), path: &Path) -> Option<bool> {
        let i = lock(&self.inner);
        let same = match i.meta.cwd_id {
            Some(h) => h == id,
            None => Path::new(&i.meta.cwd) == path,
        };
        same.then_some(i.meta.bundled)
    }

    fn recover_old_meta(&self) {
        {
            let mut i = lock(&self.inner);
            if i.meta.bundle_sha256.is_some() {
                i.meta.bundled = true;
            }
        }
        self.persist();
    }

    fn identity_unknown(&self) -> bool {
        !self.is_exited() && lock(&self.inner).meta.cwd_id.is_none()
    }

    fn adopt_cwd_id(&self, hello: &ShimHello) {
        if let (Some(dev), Some(ino)) = (hello.cwd_dev, hello.cwd_ino) {
            lock(&self.inner).meta.cwd_id = Some((dev, ino));
            self.persist();
        }
    }

    fn awaiting_exit(&self) -> bool {
        let i = lock(&self.inner);
        let s = i.fixed.clone().unwrap_or_else(|| i.det.state().clone());
        s.kind != StateKind::Exited || is_interrupted(&s)
    }

    fn is_exited(&self) -> bool {
        self.state().kind == StateKind::Exited
    }

    fn persist(&self) {
        let _g = lock(&self.meta_lock);
        let meta = lock(&self.inner).meta.clone();
        write_meta(&self.dir, &meta);
    }

    fn publish(&self, states: Vec<State>) {
        let Some(last) = states.last().cloned() else {
            return;
        };
        let (task, origin) = {
            let mut i = lock(&self.inner);
            i.meta.last_state = Some(last);
            (i.meta.task, i.meta.origin.clone())
        };
        self.persist();
        for state in states {
            self.hub.broadcast(to_json(&EventMsg {
                session: self.id.clone(),
                state: state.clone(),
                task,
                origin: origin.clone(),
            }));
            self.worklog.record(&state);
        }
    }

    fn change(&self, f: impl FnOnce(&mut Inner, u64, &mut Vec<State>)) {
        let mut out = Vec::new();
        {
            let mut i = lock(&self.inner);
            if i.fixed.is_none() {
                f(&mut i, now_ms(), &mut out);
            }
        }
        self.publish(out);
    }

    fn shim_tx(&self) -> Option<SyncSender<Frame>> {
        lock(&self.shim).as_ref().map(|l| l.tx.clone())
    }

    fn shim_msg(&self, msg: &ToShim) {
        let Some(tx) = self.shim_tx() else { return };
        let Ok(v) = serde_json::to_value(msg) else {
            return;
        };
        let frame = Frame::Json(v);
        match msg {
            ToShim::Resize { .. } | ToShim::Signal { .. } => {
                let _ = tx.try_send(frame);
            }
            _ => {
                let _ = tx.send(frame);
            }
        }
    }

    fn shim_input(&self, bytes: &[u8]) -> bool {
        if bytes.is_empty() {
            return true;
        }
        let g = lock(&self.shim);
        let Some(link) = g.as_ref() else { return false };
        if link.queued_bytes.load(Ordering::SeqCst) + bytes.len() > SHIM_INPUT_CAP
            || link.queued_frames.load(Ordering::SeqCst) >= SHIM_INPUT_FRAMES
        {
            return false;
        }
        link.queued_bytes.fetch_add(bytes.len(), Ordering::SeqCst);
        link.queued_frames.fetch_add(1, Ordering::SeqCst);
        let frame = Frame::Bin(BinFrame {
            tag: TAG_INPUT,
            session: self.id.clone(),
            data: bytes.to_vec(),
        });
        if link.tx.try_send(frame).is_err() {
            link.queued_bytes.fetch_sub(bytes.len(), Ordering::SeqCst);
            link.queued_frames.fetch_sub(1, Ordering::SeqCst);
            return false;
        }
        true
    }

    fn reply_allowed(&self) -> bool {
        let mut b = lock(&self.replies);
        if b.window.elapsed() >= Duration::from_secs(1) {
            b.window = Instant::now();
            b.used = 0;
        }
        b.used += 1;
        b.used <= MAX_REPLIES_PER_SEC
    }

    fn has_shim(&self) -> bool {
        lock(&self.shim).is_some()
    }

    fn feed(&self, bytes: &[u8], live: bool) {
        let mut replies = Vec::new();
        self.change(|i, now, out| {
            let r = i.screen.feed(bytes);
            if live {
                replies = r.replies;
                out.extend(i.det.on_output(now));
                if r.bell || r.notify {
                    out.extend(i.det.on_bell(now));
                }
            } else if !bytes.is_empty() {
                out.extend(i.det.on_output(now));
            }
            if let Some(m) = &i.matcher {
                match m.check(&i.screen.bottom_text(10)) {
                    Some((rule, line)) if rule.starts_with("blocked") => {
                        out.extend(i.det.on_idle(now, false));
                        out.extend(i.det.on_blocked(now, &rule, &line))
                    }
                    found => {
                        out.extend(i.det.on_idle(now, found.is_some()));
                        out.extend(i.det.on_unblocked(now))
                    }
                }
            }
        });
        self.version.fetch_add(1, Ordering::SeqCst);
        for reply in replies {
            if self.reply_allowed() {
                let _ = self.shim_input(&reply);
            }
        }
    }

    fn input(&self, bytes: &[u8]) -> bool {
        let accepted = self.shim_input(bytes);
        if accepted && !bytes.is_empty() {
            self.change(|i, now, out| out.extend(i.det.on_input(now)));
        }
        accepted
    }

    fn resize(&self, cols: u16, rows: u16, owner: Option<u64>) {
        let (cols, rows) = clamp_size(cols, rows);
        {
            let mut i = lock(&self.inner);
            i.screen.resize(rows, cols);
            i.meta.cols = cols;
            i.meta.rows = rows;
            i.size_owner = owner;
        }
        self.persist();
        self.version.fetch_add(1, Ordering::SeqCst);
        self.shim_msg(&ToShim::Resize { cols, rows });
    }

    fn snapshot(&self, offset: usize) -> Snapshot {
        lock(&self.inner).screen.snapshot(offset)
    }

    fn on_stat(&self, cpu: Option<u64>, waiting: Option<bool>) {
        self.change(|i, now, out| out.extend(i.det.on_stat(now, cpu, waiting)));
    }

    fn on_exited(&self, e: &ExitInfo) {
        let at = now_ms();
        self.change(|i, now, out| {
            let changed = i.det.on_exit(now, e.code, e.signal);
            if changed.is_some() {
                i.meta.exit_signal = e.signal;
                log(&format!(
                    "session {} exited code={:?} signal={:?}",
                    self.id, e.code, e.signal
                ));
            }
            out.extend(changed)
        });
        self.worklog.exited(at, e.code, e.signal);
        finish_seeds(&self.dir, &self.id);
        self.aw_next.store(0, Ordering::SeqCst);
    }

    fn on_exit_report(&self, e: &ExitInfo) {
        if self.revoking.load(Ordering::SeqCst) {
            self.mark_revoked_with(e.code.or(e.signal.map(|sig| 128 + sig)));
        }
        self.on_exited(e);
    }

    fn tick(self: &Arc<Self>) {
        self.change(|i, now, out| out.extend(i.det.tick(now)));
        self.agent_work();
    }

    fn prepare_agent_work(&self) {
        let mut pending = lock(&self.aw_pending);
        let Some(intent) = pending.as_ref() else {
            return;
        };
        match agentwork::begin(
            &self.dir,
            intent.task,
            &intent.agent,
            intent.model.as_deref(),
            &self.id,
        ) {
            Ok(()) => *pending = None,
            Err(e) => {
                self.aw_next
                    .store(now_ms() + agentwork::retry_ms(), Ordering::SeqCst);
                log(&format!(
                    "session {}: agent work not prepared: {e}",
                    self.id
                ));
            }
        }
    }

    fn halt(&self) {
        self.halted.store(true, Ordering::SeqCst);
        agentwork::mark_halted(&self.dir);
    }

    fn exit_for_agent_work(&self) -> Option<agentwork::Exit> {
        let state = self.state();
        (state.kind == StateKind::Exited).then(|| agentwork::Exit {
            code: state.exit_code,
            interrupted: is_interrupted(&state),
        })
    }

    fn settle_agent_work(&self) {
        if !agentwork::wanted(&self.dir) {
            return;
        }
        let deadline = Instant::now() + Duration::from_secs(35);
        while self.aw_busy.swap(true, Ordering::SeqCst) {
            if Instant::now() >= deadline {
                log(&format!(
                    "session {}: agent work still busy at removal",
                    self.id
                ));
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        if self.halted.load(Ordering::SeqCst) {
            agentwork::mark_halted(&self.dir);
        }
        if let Err(e) = agentwork::step(
            &self.helper,
            &self.dir,
            now_ms(),
            self.exit_for_agent_work(),
        ) {
            log(&format!(
                "session {}: agent work not finished before removal: {e}",
                self.id
            ));
        }
    }

    fn agent_work(self: &Arc<Self>) {
        let now = now_ms();
        if now < self.aw_next.load(Ordering::SeqCst) {
            return;
        }
        self.prepare_agent_work();
        if now < self.aw_next.load(Ordering::SeqCst) || !agentwork::wanted(&self.dir) {
            return;
        }
        if self.aw_busy.swap(true, Ordering::SeqCst) {
            return;
        }
        self.aw_next
            .store(now + agentwork::retry_ms(), Ordering::SeqCst);
        if self.halted.load(Ordering::SeqCst) {
            agentwork::mark_halted(&self.dir);
        }
        let exit = self.exit_for_agent_work();
        let s = self.clone();
        thread::spawn(move || {
            match agentwork::step(&s.helper, &s.dir, now, exit) {
                Ok(Some("start")) => {
                    log(&format!("session {}: agent work run opened", s.id));
                    s.aw_next.store(0, Ordering::SeqCst);
                }
                Ok(Some(done)) => log(&format!("session {}: agent work {done}", s.id)),
                Ok(None) => {}
                Err(e) => log(&format!("session {}: agent work not recorded: {e}", s.id)),
            }
            s.aw_busy.store(false, Ordering::SeqCst);
        });
    }

    fn interrupt(&self) {
        let state = {
            let mut i = lock(&self.inner);
            if i.fixed.is_some() || i.det.state().kind == StateKind::Exited {
                return;
            }
            let s = interrupted_state();
            i.fixed = Some(s.clone());
            s
        };
        self.publish(vec![state]);
    }

    fn attach_shim(self: &Arc<Self>, stream: UnixStream, hello: ShimHello, fresh: bool) {
        if let Ok(link) = ShimLink::spawn(&stream) {
            *lock(&self.shim) = Some(link);
        }
        if let Some(e) = &hello.exit {
            self.on_exit_report(e);
        }
        let me = self.clone();
        thread::spawn(move || {
            me.read_shim(&stream, fresh);
            *lock(&me.shim) = None;
            if !me.revoking.load(Ordering::SeqCst) {
                me.interrupt();
            }
        });
    }

    fn read_shim(&self, stream: &UnixStream, answer_replay: bool) {
        while let Ok(Some(frame)) = read_frame(&mut &*stream) {
            match frame {
                Frame::Bin(b) => self.feed(&b.data, answer_replay || b.tag != TAG_REPLAY),
                Frame::Json(v) => match serde_json::from_value::<FromShim>(v) {
                    Ok(FromShim::Stat(s)) => self.on_stat(s.fg_cpu_ns, s.fg_waiting_tty),
                    Ok(FromShim::Exited(e)) => self.on_exit_report(&e),
                    Ok(FromShim::SignalAck { id, delivered, .. }) => {
                        let mut acks = lock(&self.sig_acks);
                        if acks.len() >= 64 {
                            acks.clear();
                        }
                        acks.insert(id, delivered);
                    }
                    Ok(FromShim::InputDropped { bytes }) => {
                        self.input_dropped.store(true, Ordering::SeqCst);
                        if !self.drop_logged.swap(true, Ordering::SeqCst) {
                            log(&format!(
                                "session {} dropped {bytes} bytes of input",
                                self.id
                            ));
                        }
                    }
                    _ => {}
                },
            }
        }
    }

    fn send_kill(&self) -> Option<u64> {
        if !self.has_shim() {
            return None;
        }
        let id = self.sig_seq.fetch_add(1, Ordering::SeqCst) + 1;
        self.shim_msg(&ToShim::Signal {
            sig: libc::SIGKILL,
            id,
        });
        Some(id)
    }

    fn kill_via_shim(&self, deadline: Instant) -> bool {
        if self.is_exited() {
            return true;
        }
        let Some(id) = self.send_kill() else {
            return false;
        };
        while Instant::now() < deadline {
            if self.is_exited() {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let ack = lock(&self.sig_acks).remove(&id);
        if ack == Some(false) {
            log(&format!(
                "session {} signal {id} was not delivered",
                self.id
            ));
        }
        self.is_exited()
    }

    fn mark_revoked(&self) {
        self.mark_revoked_with(None)
    }

    fn mark_revoked_with(&self, code: Option<i32>) {
        let state = {
            let mut i = lock(&self.inner);
            if i.fixed
                .as_ref()
                .is_some_and(|f| f.kind == StateKind::Exited && !is_interrupted(f))
            {
                return;
            }
            let s = State {
                kind: StateKind::Exited,
                since_ms: now_ms(),
                detail: Some("revoked".into()),
                exit_code: code.or(i.det.state().exit_code),
            };
            i.fixed = Some(s.clone());
            s
        };
        self.publish(vec![state]);
        let _ = fs::remove_file(self.dir.join(REVOKING_MARK));
    }

    fn mark_revoking(&self) {
        self.revoking.store(true, Ordering::SeqCst);
        self.halt();
        if let Err(e) = write_atomic(&self.dir.join(REVOKING_MARK), b"1") {
            log(&format!(
                "session {} revocation not persisted: {e}",
                self.id
            ));
        }
    }

    fn settle_without_shim(&self) {
        let pid = lock(&self.inner).meta.child_pid as i32;
        let gone = pid > 0
            && unsafe { libc::kill(-pid, 0) } != 0
            && io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
        if gone {
            self.mark_revoked();
        }
    }

    fn nudge(&self) {
        let (cols, rows) = {
            let i = lock(&self.inner);
            (i.meta.cols, i.meta.rows)
        };
        let alt = if cols > 1 { cols - 1 } else { cols + 1 };
        self.shim_msg(&ToShim::Resize { cols: alt, rows });
        thread::sleep(Duration::from_millis(50));
        self.shim_msg(&ToShim::Resize { cols, rows });
    }

    fn load_dead(&self) {
        let mut i = lock(&self.inner);
        if let Ok(mut f) = File::open(self.dir.join("output.log")) {
            let len = f.metadata().map_or(0, |m| m.len());
            let _ = f.seek(SeekFrom::Start(len.saturating_sub(LOG_TAIL)));
            let mut buf = Vec::new();
            if f.take(LOG_TAIL).read_to_end(&mut buf).is_ok() {
                i.screen.feed(&buf);
            }
        }
        let known = i
            .meta
            .last_state
            .clone()
            .filter(|s| s.kind == StateKind::Exited);
        i.fixed = Some(known.unwrap_or_else(interrupted_state));
    }
}

fn adopt_shim(
    dir: &Path,
    id: &str,
    adopt_secret: &str,
    expect_pid: Option<u32>,
) -> io::Result<(UnixStream, ShimHello)> {
    let stream =
        endpoint::connect_within(&Endpoint::load_session(dir, id)?, server::HANDSHAKE_TIMEOUT)?;
    let cred = server::accept_check(&stream)?;
    if let Some(p) = expect_pid {
        if cred.pid != Some(p as i32) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "shim pid does not match the recorded pid",
            ));
        }
    }
    let deadline = Instant::now() + server::HANDSHAKE_TIMEOUT;
    stream.set_write_timeout(Some(server::HANDSHAKE_TIMEOUT))?;
    let nonce_r = auth::nonce();
    write_json(
        &mut &stream,
        &ToShim::Adopt {
            nonce_r: nonce_r.clone(),
        },
    )?;
    let next = || match server::read_frame_by(&stream, deadline)? {
        Some(Frame::Json(v)) => serde_json::from_value::<FromShim>(v)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bad shim message")),
        _ => Err(io::ErrorKind::UnexpectedEof.into()),
    };
    let FromShim::AdoptChallenge { nonce_sh, proof_sh } = next()? else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "bad shim challenge",
        ));
    };
    if !auth::verify(adopt_secret, auth::SHIM, &nonce_r, &nonce_sh, &proof_sh) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "shim identity check failed",
        ));
    }
    let proof_r = auth::proof(adopt_secret, auth::RUNNER_ADOPT, &nonce_sh, &nonce_r);
    write_json(&mut &stream, &ToShim::AdoptAuth { proof_r })?;
    let FromShim::Hello(hello) = next()? else {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "bad shim hello"));
    };
    let _ = stream.set_write_timeout(None);
    Ok((stream, hello))
}

fn retry_revocation(dir: &Path, sdir: &Path, s: &Arc<Session>) {
    let pid = fs::read_to_string(sdir.join("shim.pid"))
        .ok()
        .and_then(|p| p.trim().parse::<u32>().ok());
    let Some(pid) = pid else {
        s.settle_without_shim();
        return;
    };
    let Ok(sec) = secret::read(&sdir.join("adopt.secret")) else {
        return;
    };
    match adopt_shim(dir, &s.id, &sec, Some(pid)) {
        Ok((stream, hello)) => {
            s.attach_shim(stream, hello, false);
            s.send_kill();
        }
        Err(e)
            if matches!(
                e.kind(),
                io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused
            ) =>
        {
            s.settle_without_shim()
        }
        Err(_) => {}
    }
}

struct Conn {
    id: u64,
    w: Mutex<UnixStream>,
    watched: Mutex<Vec<String>>,
    scroll: Mutex<HashMap<String, usize>>,
    sizes: Mutex<HashMap<String, (u16, u16)>>,
    closed: AtomicBool,
    attached: AtomicBool,
    client: String,
    pending_rx: Mutex<Option<Receiver<Value>>>,
}

impl Conn {
    fn write(&self, msg: &ServerMsg) -> io::Result<()> {
        write_json(&mut *lock(&self.w), msg)
    }
}

#[derive(Default)]
struct Rejects {
    last: Option<Instant>,
    suppressed: u64,
}

struct Runner {
    cfg: RunnerConfig,
    secret: String,
    started: Instant,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    hub: Arc<Hub>,
    create_lock: Mutex<()>,
    key_lock: Mutex<()>,
    layout_lock: Mutex<()>,
    stop: AtomicBool,
    conns: Arc<AtomicUsize>,
    pending: Arc<AtomicUsize>,
    rejects: Mutex<Rejects>,
    next_conn: AtomicU64,
}

fn default_name(argv0: &str) -> String {
    let base = Path::new(argv0)
        .file_name()
        .map_or_else(|| argv0.to_string(), |n| n.to_string_lossy().into_owned());
    let name: String = crate::sanitize::sanitize_text(&base)
        .chars()
        .take(64)
        .collect();
    if name.is_empty() {
        "session".into()
    } else {
        name
    }
}

fn valid_env_name(n: &str) -> bool {
    let b = n.as_bytes();
    !b.is_empty()
        && b.len() <= 64
        && !b[0].is_ascii_digit()
        && b.iter().all(|c| c.is_ascii_alphanumeric() || *c == b'_')
}

fn base_env() -> Vec<(String, String)> {
    std::env::vars_os()
        .filter_map(|(k, v)| Some((k.into_string().ok()?, v.into_string().ok()?)))
        .filter(|(k, _)| {
            matches!(
                k.as_str(),
                "PATH" | "USER" | "LOGNAME" | "LANG" | "SHELL" | "TMPDIR" | "COLORTERM"
            ) || k.starts_with("LC_")
        })
        .collect()
}

fn signal_number(name: &str) -> Option<i32> {
    match name {
        "term" => Some(libc::SIGTERM),
        "kill" => Some(libc::SIGKILL),
        "int" => Some(libc::SIGINT),
        "hup" => Some(libc::SIGHUP),
        _ => None,
    }
}

fn valid_start_key(k: &str) -> bool {
    (1..=64).contains(&k.len())
        && k.bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b':' | b'-'))
}

type PinCheck = (String, Result<PathBuf, ApiErr>);

fn check_pin_fields(pin: &ExePin) -> Result<(), ApiErr> {
    if !Path::new(&pin.path).is_absolute() {
        return Err(api(
            ErrorCode::InvalidParams,
            "exe_pin.path must be absolute",
        ));
    }
    if pin.sha256.len() != 64
        || !pin
            .sha256
            .bytes()
            .all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
    {
        return Err(api(
            ErrorCode::InvalidParams,
            "exe_pin.sha256 must be 64 lowercase hex characters",
        ));
    }
    Ok(())
}

fn check_argv_sha256(req: &RunCreateReq) -> Result<(), ApiErr> {
    let Some(d) = &req.argv_sha256 else {
        return Ok(());
    };
    if req.exe_pin.is_none() {
        return Err(api(ErrorCode::InvalidParams, "argv_sha256 needs exe_pin"));
    }
    if d.len() != 64 || !d.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')) {
        return Err(api(
            ErrorCode::InvalidParams,
            "argv_sha256 must be 64 lowercase hex characters",
        ));
    }
    Ok(())
}

fn check_pin(command: &str, path: &std::ffi::OsStr, pin: &ExePin) -> Result<PathBuf, ApiErr> {
    let exe = resolve_exe(command, path, None).map_err(|e| api(ErrorCode::Forbidden, e))?;
    verify_pin(&exe, &pin.path, &pin.sha256).map_err(|e| api(ErrorCode::Forbidden, e))
}

fn params_hash(req: &RunCreateReq) -> String {
    let mut v = serde_json::to_value(req).unwrap_or(Value::Null);
    if let Some(m) = v.as_object_mut() {
        for k in ["cols", "rows", "start_key"] {
            m.remove(k);
        }
    }
    tmgr_ipc::api::sha256_hex(tmgr_ipc::api::canonical_json(&v).as_bytes())
}

fn resolve_mode(
    prof: &Profile,
    requested: Option<&str>,
) -> Result<(Option<String>, Vec<String>), ApiErr> {
    if requested.is_some_and(|m| !tmgr_ipc::api::valid_permission_mode(m)) {
        return Err(api(
            ErrorCode::InvalidParams,
            "permission_mode must be ask, accept-edits or plan",
        ));
    }
    let effective = requested.or(prof.has_permission_modes().then_some("ask"));
    match effective {
        None => Ok((None, Vec::new())),
        Some(m) => prof
            .permission_args(m)
            .map(|a| (Some(m.to_string()), a.to_vec()))
            .map_err(|e| api(ErrorCode::InvalidParams, e)),
    }
}

fn check_agent_work(req: &RunCreateReq, mode: Option<&str>) -> Result<(), ApiErr> {
    if req.origin.is_some() {
        return Err(api(
            ErrorCode::Forbidden,
            "agent_work is not accepted for link runs",
        ));
    }
    match req.task {
        None => return Err(api(ErrorCode::InvalidParams, "agent_work needs a task")),
        Some(t) if t.workspace_id >= 0 => {
            return Err(api(
                ErrorCode::InvalidParams,
                "agent_work: the runner starts tasks of local workspaces only (decision 16a.11); run the task in a local workspace",
            ))
        }
        Some(_) => {}
    }
    if mode.is_none() {
        return Err(api(
            ErrorCode::InvalidParams,
            format!(
                "profile {:?} has no [permission_modes] table; task runs need its ask entry",
                req.profile
            ),
        ));
    }
    Ok(())
}

fn session_id_param(v: &Value) -> Result<String, ApiErr> {
    let id = v
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| api(ErrorCode::InvalidParams, "id is required"))?;
    if valid_session_id(id) {
        Ok(id.to_string())
    } else {
        Err(api(ErrorCode::InvalidParams, "invalid session id"))
    }
}

impl Runner {
    fn session(&self, id: &str) -> Option<Arc<Session>> {
        lock(&self.sessions).get(id).cloned()
    }

    fn all(&self) -> Vec<Arc<Session>> {
        lock(&self.sessions).values().cloned().collect()
    }

    fn sdir(&self, id: &str) -> PathBuf {
        self.cfg.dir.join("sessions").join(id)
    }

    fn respond(self: &Arc<Self>, conn: &Arc<Conn>, req: Request) -> Response {
        match self.dispatch(conn, &req) {
            Ok(v) => Response {
                id: req.id,
                ok: Some(v),
                err: None,
            },
            Err(e) => Response {
                id: req.id,
                ok: None,
                err: Some(ErrorBody {
                    code: e.code,
                    message: e.msg,
                }),
            },
        }
    }

    fn dispatch(self: &Arc<Self>, conn: &Arc<Conn>, req: &Request) -> Result<Value, ApiErr> {
        let p = &req.params;
        match req.method.as_str() {
            method::RUNNER_STATUS => Ok(to_json(&StatusInfo {
                pid: std::process::id(),
                version: self.cfg.version.clone(),
                proto: PROTO_VERSION,
                uptime_s: self.started.elapsed().as_secs(),
                sessions: lock(&self.sessions).len(),
            })),
            method::RUNNER_STOP => Ok(Value::Null),
            method::CANDIDATES_LIST => Ok(to_json(&seedguard::pending(&self.cfg.dir))),
            method::SESSIONS_LIST => {
                let mut list: Vec<SessionInfo> = self.all().iter().map(|s| s.info()).collect();
                list.sort_by(|a, b| (a.created_ms, &a.id).cmp(&(b.created_ms, &b.id)));
                Ok(to_json(&list))
            }
            method::SESSIONS_CREATE => {
                if conn.client != CLIENT_CLI {
                    return Err(api(
                        ErrorCode::Forbidden,
                        "raw commands are accepted only from the tmgr CLI; use runs.create with a profile",
                    ));
                }
                self.create(params(p)?).map(|i| to_json(&i))
            }
            method::RUNS_CREATE => {
                let req: RunCreateReq = params(p)?;
                if req.origin.is_some() != (conn.client == CLIENT_WORKER) {
                    return Err(api(
                        ErrorCode::Forbidden,
                        "runs for a link come only from the worker and must name their origin",
                    ));
                }
                if req.trust_cwd.is_some() {
                    if conn.client != CLIENT_CLI {
                        return Err(api(
                            ErrorCode::Forbidden,
                            "trust_cwd is accepted only from the tmgr CLI",
                        ));
                    }
                    if req.start_key.is_some() || req.clone_of.is_some() || req.origin.is_some() {
                        return Err(api(
                            ErrorCode::InvalidParams,
                            "trust_cwd cannot be combined with start_key, clone_of or origin",
                        ));
                    }
                }
                self.create_run(req).map(|i| to_json(&i))
            }
            method::RUNS_LOOKUP => {
                let key = params::<LookupParams>(p)?.start_key;
                self.by_key(&key)
                    .map(|s| to_json(&s.info()))
                    .ok_or_else(|| api(ErrorCode::NotFound, "no run with this start key"))
            }
            method::RUNS_STATUS => {
                let s = self
                    .session(&session_id_param(p)?)
                    .ok_or_else(|| api(ErrorCode::NotFound, "no such session"))?;
                Ok(to_json(&s.run_status()))
            }
            method::RUNS_RESTORE => {
                if conn.client != CLIENT_CLI {
                    return Err(api(
                        ErrorCode::Forbidden,
                        "runs.restore is accepted only from the tmgr CLI",
                    ));
                }
                self.restore_run(&session_id_param(p)?).map(|r| to_json(&r))
            }
            method::ORIGIN_REVOKE => {
                if conn.client != CLIENT_CLI {
                    return Err(api(
                        ErrorCode::Forbidden,
                        "origin.revoke is accepted only from the tmgr CLI",
                    ));
                }
                self.origin_revoke(params(p)?).map(|r| to_json(&r))
            }
            method::SESSIONS_KILL => {
                let s = self.live(&session_id_param(p)?)?;
                let name = p.get("signal").and_then(Value::as_str).unwrap_or("");
                let sig = signal_number(name).ok_or_else(|| {
                    api(
                        ErrorCode::InvalidParams,
                        "signal must be term, kill, int or hup",
                    )
                })?;
                let id = s.sig_seq.fetch_add(1, Ordering::SeqCst) + 1;
                s.halt();
                s.shim_msg(&ToShim::Signal { sig, id });
                restore::forget(&self.intents(), &s.id);
                Ok(Value::Null)
            }
            method::SESSIONS_SEND => {
                let s = self.live(&session_id_param(p)?)?;
                let text = p
                    .get("text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| api(ErrorCode::InvalidParams, "text is required"))?;
                if text.len() > MAX_INPUT {
                    return Err(api(ErrorCode::InvalidParams, "text is too large"));
                }
                if s.input_dropped.swap(false, Ordering::SeqCst) || !s.input(text.as_bytes()) {
                    return Err(api(
                        ErrorCode::Busy,
                        "input queue full; the agent is not reading its input",
                    ));
                }
                Ok(Value::Null)
            }
            method::SESSIONS_TEXT => {
                if conn.client == CLIENT_WORKER {
                    return Err(api(
                        ErrorCode::Forbidden,
                        "screen text is never available to link workers",
                    ));
                }
                let q: TextParams = params(p)?;
                if !valid_session_id(&q.id) {
                    return Err(api(ErrorCode::InvalidParams, "invalid session id"));
                }
                if q.lines
                    .is_some_and(|n| !(1..=limits::MAX_TEXT_LINES).contains(&n))
                {
                    return Err(api(ErrorCode::InvalidParams, "lines must be 1 to 10000"));
                }
                let s = self
                    .session(&q.id)
                    .ok_or_else(|| api(ErrorCode::NotFound, "no such session"))?;
                Ok(to_json(&s.text(q.lines)))
            }
            method::SESSIONS_WORKLOG => {
                let s = self
                    .session(&session_id_param(p)?)
                    .ok_or_else(|| api(ErrorCode::NotFound, "no such session"))?;
                Ok(to_json(&worklog::summary(&s.dir)))
            }
            method::SESSIONS_REMOVE => {
                let id = session_id_param(p)?;
                self.remove(&id)?;
                restore::forget(&self.intents(), &id);
                Ok(Value::Null)
            }
            method::LAYOUT_GET => Ok(self.layout_get()),
            method::LAYOUT_SET => self.layout_set(p).map(|_| Value::Null),
            method::ATTACH_OPEN => self.attach_open(conn, p),
            method::ATTACH_SET => self.attach_set(conn, p),
            m => Err(api(ErrorCode::UnknownMethod, format!("unknown method {m}"))),
        }
    }

    fn live(&self, id: &str) -> Result<Arc<Session>, ApiErr> {
        let s = self
            .session(id)
            .ok_or_else(|| api(ErrorCode::NotFound, "no such session"))?;
        if s.is_exited() || !s.has_shim() {
            return Err(api(ErrorCode::NotRunning, "session is not running"));
        }
        Ok(s)
    }

    fn layout_path(&self) -> PathBuf {
        self.cfg.dir.join("layout.json")
    }

    fn layout_get(&self) -> Value {
        let _g = lock(&self.layout_lock);
        fs::read(self.layout_path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(Value::Null)
    }

    fn layout_set(&self, p: &Value) -> Result<(), ApiErr> {
        let layout = p
            .get("layout")
            .ok_or_else(|| api(ErrorCode::InvalidParams, "layout is required"))?;
        let bytes =
            serde_json::to_vec(layout).map_err(|e| api(ErrorCode::InvalidParams, e.to_string()))?;
        if bytes.len() > MAX_LAYOUT {
            return Err(api(ErrorCode::InvalidParams, "layout is too large"));
        }
        let _g = lock(&self.layout_lock);
        write_atomic(&self.layout_path(), &bytes)
            .map_err(|e| api(ErrorCode::Internal, e.to_string()))
    }

    fn validate(&self, r: &CreateReq) -> Result<PathBuf, ApiErr> {
        let bad = |m: &str| Err(api(ErrorCode::InvalidParams, m));
        if r.argv.is_empty() || r.argv.len() > 64 {
            return bad("argv must have 1 to 64 entries");
        }
        if r.argv.iter().any(|a| a.len() > 4096 || a.contains('\0')) {
            return bad("argv entries must be at most 4096 bytes without NUL");
        }
        if r.argv[0].is_empty() {
            return bad("argv[0] must not be empty");
        }
        if !Path::new(&r.argv[0]).is_absolute() {
            return bad("argv[0] must be an absolute path");
        }
        if let Some(n) = &r.name {
            if n.is_empty() || n.chars().count() > 64 || n.chars().any(char::is_control) {
                return bad("name must be 1 to 64 printable characters");
            }
        }
        if r.env_pass.len() > 64 || !r.env_pass.iter().all(|n| valid_env_name(n)) {
            return bad("invalid env_pass");
        }
        let cwd = Path::new(&r.cwd);
        if !cwd.is_absolute() || r.cwd.contains('\0') {
            return bad("cwd must be an absolute path");
        }
        let cwd = fs::canonicalize(cwd)
            .ok()
            .filter(|p| p.is_dir())
            .ok_or_else(|| {
                api(
                    ErrorCode::InvalidParams,
                    "cwd must be an existing directory",
                )
            })?;
        if cwd == Path::new("/") || self.protected_overlap(&cwd)? {
            return bad("cwd must not be /, the home directory or overlap protected paths (including the agent profile directory and secrets files)");
        }
        Ok(cwd)
    }

    fn protected_overlap(&self, cwd: &Path) -> Result<bool, ApiErr> {
        let canon = |p: &Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let mut denied = vec![canon(&self.cfg.dir), canon(&self.cfg.homes)];
        denied.extend(profile_protected_paths()?);
        let mut home = None;
        if let Some(h) = std::env::var_os("HOME").map(PathBuf::from) {
            let h = canon(&h);
            denied.extend(PROTECTED_IN_HOME.iter().map(|sub| h.join(sub)));
            home = Some(h);
        }
        Ok(home.is_some_and(|h| h.starts_with(cwd))
            || denied
                .iter()
                .any(|p| cwd.starts_with(p) || p.starts_with(cwd)))
    }

    fn revoked_path(&self) -> PathBuf {
        self.cfg.dir.join("revoked.json")
    }

    fn revoked(&self) -> Result<BTreeMap<String, u64>, ApiErr> {
        match fs::read(self.revoked_path()) {
            Ok(b) => serde_json::from_slice(&b)
                .map_err(|_| api(ErrorCode::Internal, "revoked.json is damaged")),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(api(ErrorCode::Internal, e.to_string())),
        }
    }

    fn origin_is_revoked(&self, s: &Session) -> bool {
        let Some(o) = s.info().origin else {
            return false;
        };
        self.revoked()
            .map_or(true, |r| r.get(&o.link).is_some_and(|g| o.generation <= *g))
    }

    fn check_origin(
        &self,
        origin: &Origin,
        model: Option<&str>,
        req: &CreateReq,
        dir: Option<&bundle::Dir>,
    ) -> Result<(), ApiErr> {
        let denied = |m: &str| api(ErrorCode::Forbidden, m);
        if self
            .revoked()?
            .get(&origin.link)
            .is_some_and(|g| origin.generation <= *g)
        {
            return Err(denied("link revoked"));
        }
        let trust = trust::read(&origin.link)
            .map_err(|e| denied(&e))?
            .ok_or_else(|| denied("unknown or revoked link"))?;
        if trust.generation != origin.generation {
            return Err(denied("link changed; re-request"));
        }
        let scope = &trust.scope;
        let profile = req.profile.as_deref().unwrap_or("");
        if !scope.agents.iter().any(|a| a == profile) {
            return Err(denied("agent profile is outside the link's scope"));
        }
        if !scope.models.is_empty() && !model.is_some_and(|m| scope.models.iter().any(|a| a == m)) {
            return Err(denied("model is outside the link's scope"));
        }
        let dir = dir.ok_or_else(|| api(ErrorCode::Internal, "cwd was not opened"))?;
        if !dir.is_beneath_any(&scope.repos) {
            return Err(denied("folder is outside the link's repositories"));
        }
        let live = lock(&self.sessions)
            .values()
            .filter(|s| !s.is_exited() && s.info().origin.is_some_and(|o| o.link == origin.link))
            .count();
        if live >= scope.max as usize {
            return Err(api(
                ErrorCode::Limit,
                "the link's concurrent run limit is reached",
            ));
        }
        Ok(())
    }

    fn origin_revoke(&self, p: OriginRevokeParams) -> Result<OriginRevoked, ApiErr> {
        let _g = lock(&self.create_lock);
        let mut revoked = self.revoked()?;
        let at = revoked.entry(p.link.clone()).or_insert(0);
        *at = (*at).max(p.generation);
        let bytes =
            serde_json::to_vec(&revoked).map_err(|e| api(ErrorCode::Internal, e.to_string()))?;
        write_atomic(&self.revoked_path(), &bytes)
            .map_err(|e| api(ErrorCode::Internal, e.to_string()))?;
        let victims: Vec<Arc<Session>> = self
            .all()
            .into_iter()
            .filter(|s| {
                s.awaiting_exit()
                    && s.info()
                        .origin
                        .is_some_and(|o| o.link == p.link && o.generation <= p.generation)
            })
            .collect();
        let confirmed: Vec<bool> = thread::scope(|scope| {
            let jobs: Vec<_> = victims
                .iter()
                .map(|s| scope.spawn(|| terminate(s)))
                .collect();
            jobs.into_iter()
                .map(|j| j.join().unwrap_or(false))
                .collect()
        });
        let unconfirmed: Vec<String> = victims
            .iter()
            .zip(&confirmed)
            .filter(|(_, ok)| !**ok)
            .map(|(s, _)| s.info().id)
            .collect();
        Ok(OriginRevoked {
            killed: (victims.len() - unconfirmed.len()) as u32,
            unconfirmed,
        })
    }

    fn intents(&self) -> PathBuf {
        restore::store(&self.cfg.homes)
    }

    fn seed_destinations(prof: Option<&Profile>) -> Vec<String> {
        prof.map_or_else(Vec::new, |p| {
            p.sandbox
                .seed
                .iter()
                .map(|s| s.to.clone())
                .chain(p.secrets.files.iter().map(|f| f.to.clone()))
                .collect()
        })
    }

    fn trust_file_of(&self, id: &str) -> Option<String> {
        let bytes = fs::read(self.sdir(id).join("meta.json")).ok()?;
        serde_json::from_slice::<Meta>(&bytes).ok()?.trust_file
    }

    fn by_key(&self, key: &str) -> Option<Arc<Session>> {
        self.all()
            .into_iter()
            .find(|s| s.start_key().is_some_and(|(k, _)| k == key))
    }

    fn restore_run(self: &Arc<Self>, old: &str) -> Result<RestoredRun, ApiErr> {
        let store = self.intents();
        let claim = restore::claim(&store, old).map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => api(ErrorCode::NotFound, e.to_string()),
            io::ErrorKind::AlreadyExists => api(ErrorCode::Busy, e.to_string()),
            _ => api(ErrorCode::InvalidParams, e.to_string()),
        })?;
        let running = self.session(old).is_some_and(|s| !s.is_exited());
        let result = match restore::status(&claim.intent, now_ms(), running) {
            restore::Status::Restorable if claim.intent.keyed && claim.intent.exe_pin.is_none() => {
                Err(api(
                    ErrorCode::Forbidden,
                    "this queue run was recorded without an approved executable; queue the task again",
                ))
            }
            restore::Status::Restorable => restore::read_bundle(&store, &claim.intent)
                .map_err(|e| api(ErrorCode::InvalidParams, e.to_string()))
                .and_then(|bundle| {
                    self.create_run_as(restore::request(&claim.intent, bundle), Some(old))
                }),
            restore::Status::Running => Err(api(ErrorCode::Busy, "the session is still running")),
            st => Err(api(ErrorCode::Forbidden, st.reason())),
        };
        match result {
            Ok((session, resumed, recorded)) => {
                if recorded {
                    claim.finish();
                } else {
                    log(&format!(
                        "session {old}: its restore record stays claimed because the new one was not written"
                    ));
                    claim.keep();
                }
                if self.session(old).is_some_and(|s| s.is_exited()) {
                    let _ = self.remove(old);
                }
                log(&format!("session {old} restored as {}", session.id));
                Ok(RestoredRun { session, resumed })
            }
            Err(e) => {
                claim.release();
                Err(e)
            }
        }
    }

    fn create_run(self: &Arc<Self>, req: RunCreateReq) -> Result<SessionInfo, ApiErr> {
        self.create_run_as(req, None).map(|(info, _, _)| info)
    }

    fn precheck_pin(&self, req: &RunCreateReq) -> Result<Option<PinCheck>, ApiErr> {
        check_argv_sha256(req)?;
        let Some(pin) = &req.exe_pin else {
            return Ok(None);
        };
        check_pin_fields(pin)?;
        let (set, _) = ProfileSet::load(profile::user_dir().as_deref());
        let Some(cmd) = set
            .get(&req.profile)
            .and_then(|p| p.command.first().cloned())
        else {
            return Ok(None);
        };
        let path = std::env::var_os("PATH").unwrap_or_default();
        let done = check_pin(&cmd, &path, pin);
        Ok(Some((cmd, done)))
    }

    fn create_run_as(
        self: &Arc<Self>,
        req: RunCreateReq,
        restore_from: Option<&str>,
    ) -> Result<(SessionInfo, bool, bool), ApiErr> {
        let start = match &req.start_key {
            Some(k) if !valid_start_key(k) => {
                return Err(api(
                    ErrorCode::InvalidParams,
                    "start_key must be 1 to 64 characters of A-Za-z0-9._:-",
                ))
            }
            Some(k) => Some((k.clone(), params_hash(&req))),
            None => None,
        };
        let keyed = start.is_some();
        let prechecked = self.precheck_pin(&req)?;
        let _keyed = start.as_ref().map(|_| lock(&self.key_lock));
        if let Some((key, hash)) = &start {
            if let Some(existing) = self.by_key(key) {
                return if existing.start_key().is_some_and(|(_, h)| h == *hash) {
                    Ok((existing.info(), false, true))
                } else {
                    Err(api(
                        ErrorCode::Busy,
                        "start key already used for a different request",
                    ))
                };
            }
        }
        let checked = bundle::check(&req, tmgr_ipc::api::limits::MAX_BUNDLE)
            .map_err(|(code, m)| api(code, m))?;
        let pinned = match &checked {
            None => Some(bundle::open_dir(&req).map_err(|(code, m)| api(code, m))?),
            Some(_) => None,
        };
        let dir = checked.as_ref().map(|c| &c.dir).or(pinned.as_ref());
        let (set, _) = ProfileSet::load(profile::user_dir().as_deref());
        let prof = set.get(&req.profile).ok_or_else(|| {
            let known: Vec<&str> = set.list().iter().map(|p| p.id.as_str()).collect();
            api(
                ErrorCode::UnknownProfile,
                format!(
                    "unknown agent profile {:?}; known profiles: {}",
                    req.profile,
                    known.join(", ")
                ),
            )
        })?;
        hold_at("TMGR_TEST_HOLD_AFTER_PROFILE");
        let (mode, perm_args) = resolve_mode(prof, req.permission_mode.as_deref())?;
        if req.agent_work {
            check_agent_work(&req, mode.as_deref())?;
        }
        if let Some(b) = &req.bundle {
            match (b.launch.permission_mode.as_deref(), mode.as_deref()) {
                (Some(x), Some(y)) if x == y => {}
                (None, None | Some("ask")) => {}
                _ => {
                    return Err(api(
                        ErrorCode::InvalidParams,
                        "bundle permission_mode differs from the effective mode",
                    ))
                }
            }
        }
        let objects = match (&req.clone_of, dir) {
            (Some(_), _) if req.origin.is_some() => {
                return Err(api(
                    ErrorCode::Forbidden,
                    "clone_of is not accepted for link runs",
                ))
            }
            (Some(repo), Some(d)) => Some(clone::check(d, repo).map_err(|(code, m)| api(code, m))?),
            _ => None,
        };
        let resume = match restore_from {
            Some(old) if self.cfg.homes.join(old).is_dir() => {
                prof.resume_argv_with(req.model.as_deref(), &perm_args)
            }
            _ => Ok(None),
        }
        .map_err(|e| api(ErrorCode::InvalidParams, e))?;
        let resumed = resume.is_some();
        let mut argv = match resume {
            Some(argv) => argv,
            None => prof
                .argv_with(req.model.as_deref(), &perm_args)
                .map_err(|e| api(ErrorCode::InvalidParams, e))?,
        };
        let trust = match (req.trust_cwd == Some(true), &prof.trust) {
            (false, _) => None,
            (true, Some(t)) => Some(t.clone()),
            (true, None) => {
                return Err(api(
                    ErrorCode::InvalidParams,
                    format!(
                        "profile {} has no folder-trust entry to pre-accept",
                        prof.id
                    ),
                ))
            }
        };
        let extra = RunExtra {
            trust,
            permission_mode: mode.clone(),
            clone_of: req.clone_of.clone(),
            objects,
            start,
        };
        if argv.is_empty() {
            return Err(api(ErrorCode::InvalidParams, "profile has no command"));
        }
        let path = std::env::var_os("PATH").unwrap_or_default();
        argv[0] = match (&req.exe_pin, prechecked) {
            (None, _) => resolve_exe(&argv[0], &path, None)
                .map_err(|e| api(ErrorCode::InvalidParams, e))?
                .display()
                .to_string(),
            (Some(_), Some((cmd, done))) if cmd == argv[0] => done?.display().to_string(),
            (Some(p), _) => check_pin(&argv[0], &path, p)?.display().to_string(),
        };
        if let Some(want) = &req.argv_sha256 {
            let mut basis = if resumed {
                prof.argv_with(req.model.as_deref(), &perm_args)
                    .map_err(|e| api(ErrorCode::InvalidParams, e))?
            } else {
                argv.clone()
            };
            basis[0].clone_from(&argv[0]);
            if tmgr_ipc::api::argv_sha256(&basis) != *want {
                return Err(api(
                    ErrorCode::Forbidden,
                    "the agent command line differs from the approved one",
                ));
            }
        }
        let pin = dir.map(|d| {
            let (dev, ino) = d.id();
            CwdPin { dev, ino }
        });
        let info = self.create_with(
            CreateReq {
                argv,
                cwd: dir
                    .as_ref()
                    .map_or(req.cwd.clone(), |d| d.path.display().to_string()),
                name: req.name.clone(),
                profile: Some(req.profile.clone()),
                cols: req.cols,
                rows: req.rows,
                env_pass: Vec::new(),
                task: req.task,
            },
            (
                checked.as_ref(),
                dir,
                req.origin.as_ref().map(|o| (o, req.model.as_deref())),
            ),
            (Some(prof), &extra),
            restore_from,
        )?;
        let mut recorded = false;
        let info_model = req.model.clone();
        if let Some(cwd_pin) = pin {
            let intent = restore::Intent {
                v: 1,
                session: info.id.clone(),
                profile: req.profile,
                model: req.model,
                name: info.name.clone(),
                cwd: info.cwd.clone(),
                cwd_pin,
                task: req.task,
                bundle_sha256: checked.as_ref().map(|c| c.sha256.clone()),
                origin: req.origin,
                permission_mode: mode,
                clone_of: req.clone_of,
                agent_work: req.agent_work,
                keyed,
                exe_pin: req.exe_pin,
                argv_sha256: req.argv_sha256,
                bypass: false,
                boot: restore::boot_marker(),
                pids: vec![info.shim_pid, info.child_pid],
                created_ms: info.created_ms,
                cols: info.cols,
                rows: info.rows,
            };
            let canonical = checked.as_ref().map(|c| c.canonical.as_str());
            recorded = restore::record(&self.intents(), &intent, canonical)
                .map_err(|e| {
                    log(&format!(
                        "session {}: restore record not written: {e}",
                        info.id
                    ));
                })
                .is_ok();
            if let Some(s) = self.session(&info.id) {
                restore::on_state(&self.intents(), &info.id, &s.state());
            }
        }
        if let (true, Some(task), Some(s)) = (req.agent_work, req.task, self.session(&info.id)) {
            *lock(&s.aw_pending) = Some(agentwork::Intent {
                task,
                agent: info.profile.clone().unwrap_or_default(),
                model: info_model,
            });
            s.prepare_agent_work();
        }
        Ok((info, resumed, recorded))
    }

    fn create(self: &Arc<Self>, req: CreateReq) -> Result<SessionInfo, ApiErr> {
        let (set, _) = ProfileSet::load(profile::user_dir().as_deref());
        let prof =
            match &req.profile {
                None => None,
                Some(pid) => Some(set.get(pid).ok_or_else(|| {
                    api(ErrorCode::UnknownProfile, format!("unknown profile {pid}"))
                })?),
            };
        self.create_with(req, (None, None, None), (prof, &RunExtra::default()), None)
    }

    fn create_with(
        self: &Arc<Self>,
        req: CreateReq,
        (bundle, dir, origin): (
            Option<&bundle::Checked>,
            Option<&bundle::Dir>,
            Option<OriginArg<'_>>,
        ),
        (prof, extra): (Option<&Profile>, &RunExtra),
        restore_from: Option<&str>,
    ) -> Result<SessionInfo, ApiErr> {
        let cwd = self.validate(&req)?;
        sandbox::landlock_tier(landlock_abi()).map_err(|e| api(ErrorCode::Sandbox, e))?;
        let matcher = match prof {
            None => None,
            Some(p) => Some(Matcher::new(p).map_err(|e| api(ErrorCode::UnknownProfile, e))?),
        };
        if dir.is_some_and(|d| d.path != cwd) {
            return Err(api(
                ErrorCode::InvalidParams,
                "cwd changed after the bundle was checked",
            ));
        }
        let opened;
        let dir = match dir {
            Some(d) => d,
            None => {
                opened = bundle::open_path(cwd.clone()).map_err(|(code, m)| api(code, m))?;
                &opened
            }
        };
        let _g = lock(&self.create_lock);
        let _fence = match origin {
            Some(_) => Some(trust::fence_shared().map_err(|e| api(ErrorCode::Busy, e))?),
            None => None,
        };
        if let Some((o, model)) = origin {
            self.check_origin(o, model, &req, Some(dir))?;
        }
        {
            let sessions = lock(&self.sessions);
            if sessions.len() >= self.cfg.max_sessions {
                return Err(api(ErrorCode::Limit, "too many sessions"));
            }
            if bundle.is_some() && sessions.values().any(|s| s.identity_unknown()) {
                return Err(api(
                    ErrorCode::Busy,
                    "a session from an older runner has an unknown directory identity; \
                     bundled runs are refused until it exits",
                ));
            }
            let id = dir.id();
            if sessions.values().any(|s| {
                !s.is_exited()
                    && s.holds_cwd(id, &dir.path)
                        .is_some_and(|their_bundle| bundle.is_some() || their_bundle)
            }) {
                return Err(api(
                    ErrorCode::Busy,
                    "another session is running in this directory",
                ));
            }
        }
        let id = new_session_id();
        let sdir = self.sdir(&id);
        let home = self.cfg.homes.join(&id);
        let mut moved_home = false;
        if let Some(old) = restore_from {
            for line in seedguard::finish(&self.sdir(old)) {
                log(&format!("session {old}: {line}"));
            }
            if !restore::staging_complete(&self.sdir(old)) {
                return Err(api(
                    ErrorCode::Internal,
                    "the refreshed logins of the old session could not be staged for tmgr agents sync; nothing was restored",
                ));
            }
            let moved = restore::adopt_home(&self.cfg.homes, old, &id).map_err(|e| {
                api(
                    ErrorCode::Internal,
                    format!("cannot reuse the run home: {e}"),
                )
            })?;
            moved_home = moved;
            if moved {
                let mut tos = Self::seed_destinations(prof);
                tos.extend(self.trust_file_of(old));
                match restore::clear_seed_destinations(&home, &tos) {
                    Ok(removed) => {
                        for r in removed {
                            log(&format!("session {id}: removed the old copy at {r}"));
                        }
                    }
                    Err(e) => {
                        if !restore::return_home(&self.cfg.homes, &id, old) {
                            log(&format!(
                                "session {old}: its home stays at {} after a failed restore",
                                home.display()
                            ));
                        }
                        return Err(api(
                            ErrorCode::Sandbox,
                            format!("cannot clear the old logins in the run home: {e}"),
                        ));
                    }
                }
            }
        }
        let mut installed = None;
        let result = self.spawn_session(
            &req,
            &cwd,
            &id,
            (&sdir, &home),
            (bundle, dir, origin.map(|(o, _)| o), &mut installed),
            (matcher, prof, extra),
        );
        if result.is_err() {
            if let Some(prior) = &installed {
                bundle::restore(prior);
            }
            let home_kept = match restore_from {
                Some(old) if moved_home => !restore::return_home(&self.cfg.homes, &id, old),
                _ => false,
            };
            let _ = fs::remove_dir_all(&sdir);
            if home_kept {
                log(&format!(
                    "session {id}: kept the restored home after a failed start"
                ));
            } else {
                let _ = fs::remove_dir_all(&home);
            }
        }
        result
    }

    fn spawn_session(
        self: &Arc<Self>,
        req: &CreateReq,
        cwd: &Path,
        id: &str,
        (sdir, home): (&Path, &Path),
        (bundle, dir, origin, installed): (
            Option<&bundle::Checked>,
            &bundle::Dir,
            Option<&Origin>,
            &mut Option<bundle::Prior>,
        ),
        (matcher, prof, extra): (Option<Matcher>, Option<&Profile>, &RunExtra),
    ) -> Result<SessionInfo, ApiErr> {
        let internal = |e: io::Error| api(ErrorCode::Internal, e.to_string());
        ensure_private_dir(sdir).map_err(internal)?;
        ensure_private_leaf(home).map_err(internal)?;
        let real_home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default();
        let mut base = base_env();
        let profile_protected = profile_protected_paths()?;
        let mut grants = sandbox::Grants::default();
        let mut seeds = Vec::new();
        let mut secret_files = Vec::new();
        if let Some(p) = prof {
            let pid = &p.id;
            {
                let mut protected = vec![
                    self.cfg.dir.clone(),
                    home.parent().unwrap_or(home).to_path_buf(),
                ];
                protected.extend(profile_protected.iter().cloned());
                let prep = login::prepare(p, &real_home, &protected, |_| None)
                    .map_err(|e| api(ErrorCode::Sandbox, e))?;
                for w in &prep.warnings {
                    log(&format!("profile {pid}: {w}"));
                }
                for n in &p.env_pass {
                    if !prep.pass_values.iter().any(|(k, _)| k == n) {
                        log(&format!(
                            "profile {pid}: {n} has no value in the profile's secrets file; the agent starts without it"
                        ));
                    }
                }
                base.retain(|(k, _)| !prep.pass_values.iter().any(|(n, _)| n == k));
                base.extend(prep.pass_values);
                grants = prep.grants;
                seeds = prep.seeds;
                secret_files = prep.secret_files;
            }
        }
        grants.deny = profile_protected;
        if let Some(o) = &extra.objects {
            o.reverify().map_err(|e| api(ErrorCode::Sandbox, e))?;
            grants.objects_ro.push((o.path.clone(), o.id));
        }
        let launch = sandbox::prepare(&SandboxSpec {
            session_id: id.to_string(),
            argv: req.argv.clone(),
            cwd: cwd.to_path_buf(),
            home: home.to_path_buf(),
            real_home,
            runner_dir: self.cfg.dir.clone(),
            self_exe: self.cfg.self_exe.clone(),
            env_pass: req.env_pass.clone(),
            base_env: base,
            grants,
            cwd_id: Some(dir.id()),
        })
        .map_err(|e| api(ErrorCode::Sandbox, e))?;
        for n in &launch.notes {
            log(&format!("session {id}: {n}"));
        }
        let records = login::apply_seeds(&seeds, home).map_err(|e| api(ErrorCode::Sandbox, e))?;
        seedguard::save(sdir, &records).map_err(internal)?;
        if let Some(t) = &extra.trust {
            login::apply_trust(home, t, cwd).map_err(|e| api(ErrorCode::Sandbox, e))?;
        }
        for note in login::apply_secret_files(&secret_files, home)
            .map_err(|e| api(ErrorCode::Sandbox, e))?
        {
            log(&format!("session {id}: {note}"));
        }
        let (cols, rows) = clamp_size(req.cols, req.rows);
        let lf = LaunchFile {
            argv: launch.argv,
            env: launch.env,
            cwd: launch.cwd.display().to_string(),
            cols,
            rows,
        };
        let bytes = serde_json::to_vec(&lf).map_err(|e| api(ErrorCode::Internal, e.to_string()))?;
        write_atomic(&sdir.join("launch.json"), &bytes).map_err(internal)?;
        let adopt_secret = secret::write_new(&sdir.join("adopt.secret")).map_err(internal)?;
        bundle::verify_cwd(dir).map_err(|(code, m)| api(code, m))?;
        if let Some(b) = bundle {
            *installed = Some(bundle::install(sdir, b).map_err(|(code, m)| api(code, m))?);
        }
        hold_before_spawn();
        if let Some(o) = &extra.objects {
            o.reverify().map_err(|e| api(ErrorCode::Sandbox, e))?;
        }
        let log_file = OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(sdir.join("shim.log"))
            .map_err(internal)?;
        let handover = unsafe { libc::fcntl(dir.raw_fd(), libc::F_DUPFD_CLOEXEC, 10) };
        if handover < 0 {
            return Err(internal(io::Error::last_os_error()));
        }
        let handover = unsafe { File::from_raw_fd(handover) };
        let src = handover.as_raw_fd();
        let mut shim_cmd = Command::new(&self.cfg.self_exe);
        shim_cmd
            .env_clear()
            .envs(daemon_env())
            .arg("__shim")
            .arg(&self.cfg.dir)
            .arg(id)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(log_file);
        unsafe {
            shim_cmd.pre_exec(move || {
                if libc::dup2(src, SHIM_CWD_FD) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            })
        };
        let spawned = shim_cmd.spawn();
        drop(handover);
        let mut child = spawned.map_err(internal)?;
        let shim_pid = child.id();
        let deadline = Instant::now() + Duration::from_secs(3);
        let adopted = loop {
            match adopt_shim(&self.cfg.dir, id, &adopt_secret, Some(shim_pid)) {
                Ok(r) => break Ok(r),
                Err(e) => {
                    if let Ok(Some(st)) = child.try_wait() {
                        break Err(api(
                            ErrorCode::Internal,
                            format!("shim exited early ({st})"),
                        ));
                    }
                    if Instant::now() >= deadline {
                        unsafe { libc::kill(shim_pid as i32, libc::SIGKILL) };
                        let _ = child.wait();
                        break Err(api(ErrorCode::Internal, format!("shim did not start: {e}")));
                    }
                    thread::sleep(Duration::from_millis(30));
                }
            }
        };
        thread::spawn(move || {
            let _ = child.wait();
        });
        let (stream, hello) = adopted?;
        let meta = Meta {
            id: id.to_string(),
            name: req
                .name
                .clone()
                .unwrap_or_else(|| default_name(&req.argv[0])),
            argv: req.argv.clone(),
            cwd: cwd.display().to_string(),
            profile: req.profile.clone(),
            created_ms: now_ms(),
            cols,
            rows,
            shim_pid,
            child_pid: hello.child_pid,
            last_state: None,
            task: req.task,
            bundle_sha256: bundle.map(|b| b.sha256.clone()),
            origin: origin.cloned(),
            cwd_id: Some(dir.id()),
            bundled: bundle.is_some(),
            permission_mode: extra.permission_mode.clone(),
            clone_of: extra.clone_of.clone(),
            start_key: extra.start.as_ref().map(|(k, _)| k.clone()),
            start_hash: extra.start.as_ref().map(|(_, h)| h.clone()),
            exit_signal: None,
            trust_file: extra.trust.as_ref().map(|t| t.file.clone()),
        };
        write_meta(sdir, &meta);
        let session = self.new_session(meta, matcher, sdir);
        session
            .worklog
            .created(&session.info(), extra.trust.is_some());
        session.attach_shim(stream, hello, true);
        lock(&self.sessions).insert(id.to_string(), session.clone());
        log(&format!("session {id} created: {}", req.argv[0]));
        Ok(session.info())
    }

    fn new_session(&self, meta: Meta, matcher: Option<Matcher>, sdir: &Path) -> Arc<Session> {
        let screen = ScreenModel::new(meta.rows, meta.cols, SCROLLBACK);
        Arc::new(Session {
            id: meta.id.clone(),
            dir: sdir.to_path_buf(),
            hub: self.hub.clone(),
            worklog: worklog::Log::open(sdir, PathBuf::from(&meta.cwd)),
            inner: Mutex::new(Inner {
                meta,
                screen,
                det: Detector::new(self.cfg.quiet_ms, now_ms()),
                matcher,
                fixed: None,
                size_owner: None,
            }),
            version: AtomicU64::new(0),
            shim: Mutex::new(None),
            replies: Mutex::new(ReplyBudget {
                window: Instant::now(),
                used: 0,
            }),
            drop_logged: AtomicBool::new(false),
            input_dropped: AtomicBool::new(false),
            meta_lock: Mutex::new(()),
            sig_seq: AtomicU64::new(0),
            sig_acks: Mutex::new(HashMap::new()),
            revoking: AtomicBool::new(false),
            retrying: AtomicBool::new(false),
            helper: self.cfg.self_exe.clone(),
            aw_next: AtomicU64::new(0),
            aw_busy: Arc::new(AtomicBool::new(false)),
            aw_pending: Mutex::new(None),
            halted: AtomicBool::new(false),
        })
    }

    fn remove(&self, id: &str) -> Result<(), ApiErr> {
        let s = self
            .session(id)
            .ok_or_else(|| api(ErrorCode::NotFound, "no such session"))?;
        if !s.is_exited() {
            return Err(api(ErrorCode::Busy, "only exited sessions can be removed"));
        }
        s.settle_agent_work();
        s.shim_msg(&ToShim::Reap);
        for line in seedguard::finish(&self.sdir(id)) {
            log(&format!("session {id}: {line}"));
        }
        lock(&self.sessions).remove(id);
        let _ = fs::remove_dir_all(self.sdir(id));
        let _ = fs::remove_dir_all(self.cfg.homes.join(id));
        let s_dir = self.cfg.dir.join("s");
        let _ = fs::remove_file(s_dir.join(format!("{id}.sock")));
        let _ = fs::remove_file(s_dir.join(format!("{id}.addr")));
        log(&format!("session {id} removed"));
        Ok(())
    }

    fn adopt_all(self: &Arc<Self>) {
        let Ok(rd) = fs::read_dir(self.cfg.dir.join("sessions")) else {
            return;
        };
        for entry in rd.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if !valid_session_id(&name) {
                log("skipping session directory with an invalid name");
                continue;
            }
            match self.load_session(&name) {
                Ok(()) => {}
                Err(e) => log(&format!("session {name} not loaded: {e}")),
            }
        }
    }

    fn load_session(self: &Arc<Self>, id: &str) -> io::Result<()> {
        let sdir = self.sdir(id);
        let meta: Meta = serde_json::from_slice(&fs::read(sdir.join("meta.json"))?)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        if meta.id != id {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "meta id mismatch",
            ));
        }
        let matcher = meta.profile.as_deref().and_then(|pid| {
            let (set, _) = ProfileSet::load(profile::user_dir().as_deref());
            set.get(pid).and_then(|p| Matcher::new(p).ok())
        });
        let confirmed = meta
            .last_state
            .as_ref()
            .is_some_and(|s| s.kind == StateKind::Exited && !is_interrupted(s));
        let session = self.new_session(meta, matcher, &sdir);
        if sdir.join(REVOKING_MARK).exists() || (!confirmed && self.origin_is_revoked(&session)) {
            session.mark_revoking();
        }
        session.recover_old_meta();
        let shim_pid = fs::read_to_string(sdir.join("shim.pid"))
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok());
        let adopt_secret = secret::read(&sdir.join("adopt.secret"));
        let adopted = match (shim_pid, adopt_secret) {
            (Some(pid), Ok(sec)) => adopt_shim(&self.cfg.dir, id, &sec, Some(pid)),
            _ => Err(io::Error::new(
                io::ErrorKind::NotFound,
                "shim record missing",
            )),
        };
        match adopted {
            Ok((stream, hello)) => {
                session.adopt_cwd_id(&hello);
                session.attach_shim(stream, hello, false);
                if session.revoking.load(Ordering::SeqCst) {
                    let done = terminate(&session);
                    log(&format!(
                        "session {id} adopted and revoked ({})",
                        if done { "terminated" } else { "pending" }
                    ));
                }
                let s = session.clone();
                thread::spawn(move || {
                    thread::sleep(Duration::from_millis(150));
                    s.nudge();
                });
                log(&format!("session {id} adopted"));
            }
            Err(e) => {
                session.load_dead();
                session.worklog.record(&session.state());
                log(&format!("session {id} has no live shim ({:?})", e.kind()));
            }
        }
        if session.is_exited() {
            finish_seeds(&sdir, id);
        }
        lock(&self.sessions).insert(id.to_string(), session);
        Ok(())
    }

    fn retry_revocations(&self) {
        for s in self.all() {
            if !s.revoking.load(Ordering::SeqCst) || s.is_exited() {
                continue;
            }
            if s.has_shim() {
                s.send_kill();
                continue;
            }
            if s.retrying.swap(true, Ordering::SeqCst) {
                continue;
            }
            let (dir, sdir) = (self.cfg.dir.clone(), self.sdir(&s.id));
            thread::spawn(move || {
                retry_revocation(&dir, &sdir, &s);
                s.retrying.store(false, Ordering::SeqCst);
            });
        }
    }

    fn watch_list(&self, p: &Value) -> Result<Vec<String>, ApiErr> {
        let ids: Vec<String> = match p.get("sessions") {
            Some(v) => serde_json::from_value(v.clone())
                .map_err(|_| api(ErrorCode::InvalidParams, "sessions must be a list of ids"))?,
            None => Vec::new(),
        };
        if ids.len() > MAX_WATCHED {
            return Err(api(ErrorCode::InvalidParams, "too many sessions"));
        }
        for id in &ids {
            if !valid_session_id(id) {
                return Err(api(ErrorCode::InvalidParams, "invalid session id"));
            }
            if self.session(id).is_none() {
                return Err(api(ErrorCode::NotFound, format!("no such session {id}")));
            }
        }
        Ok(ids)
    }

    fn attach_set(&self, conn: &Arc<Conn>, p: &Value) -> Result<Value, ApiErr> {
        let ids = self.watch_list(p)?;
        if let Some(sizes) = p.get("sizes").and_then(Value::as_object) {
            for (id, v) in sizes {
                if let (true, Ok((cols, rows))) = (
                    ids.contains(id),
                    serde_json::from_value::<(u16, u16)>(v.clone()),
                ) {
                    lock(&conn.sizes).insert(id.clone(), (cols, rows));
                    if let Some(s) = self.session(id) {
                        s.resize(cols, rows, Some(conn.id));
                    }
                }
            }
        }
        lock(&conn.scroll).retain(|id, _| ids.contains(id));
        lock(&conn.sizes).retain(|id, _| ids.contains(id));
        *lock(&conn.watched) = ids;
        Ok(Value::Null)
    }

    fn attach_open(&self, conn: &Arc<Conn>, p: &Value) -> Result<Value, ApiErr> {
        self.attach_set(conn, p)?;
        if !conn.attached.swap(true, Ordering::SeqCst) {
            *lock(&conn.pending_rx) = Some(self.hub.subscribe());
        }
        Ok(Value::Null)
    }

    fn pusher(&self, conn: &Arc<Conn>, rx: Receiver<Value>) {
        struct Sent {
            version: u64,
            offset: usize,
            snap: Snapshot,
            at: Instant,
        }
        let mut sent: HashMap<String, Sent> = HashMap::new();
        let fail = |conn: &Conn| {
            let _ = lock(&conn.w).shutdown(std::net::Shutdown::Both);
        };
        while !conn.closed.load(Ordering::SeqCst) {
            let mut events = Vec::new();
            match rx.recv_timeout(PUSH_INTERVAL) {
                Ok(ev) => events.push(ev),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    let _ = conn.write(&ServerMsg::Closed {
                        reason: "client too slow".into(),
                    });
                    return fail(conn);
                }
            }
            while let Ok(ev) = rx.try_recv() {
                events.push(ev);
            }
            for ev in events {
                if conn.write(&ServerMsg::Event(ev)).is_err() {
                    return fail(conn);
                }
            }
            let watched = lock(&conn.watched).clone();
            sent.retain(|id, _| watched.contains(id));
            for id in watched {
                let Some(s) = self.session(&id) else { continue };
                let offset = lock(&conn.scroll).get(&id).copied().unwrap_or(0);
                let version = s.version.load(Ordering::SeqCst);
                if let Some(e) = sent.get(&id) {
                    if (e.version == version && e.offset == offset)
                        || e.at.elapsed() < PUSH_INTERVAL
                    {
                        continue;
                    }
                } else {
                    let state = s.state();
                    let (task, origin) = {
                        let i = lock(&s.inner);
                        (i.meta.task, i.meta.origin.clone())
                    };
                    let ev = EventMsg {
                        session: id.clone(),
                        state,
                        task,
                        origin,
                    };
                    if conn.write(&ServerMsg::Event(to_json(&ev))).is_err() {
                        return fail(conn);
                    }
                }
                let snap = s.snapshot(offset);
                for push in screen_pushes(&id, sent.get(&id).map(|p| &p.snap), &snap) {
                    if conn.write(&ServerMsg::Screen(to_json(&push))).is_err() {
                        return fail(conn);
                    }
                }
                sent.insert(
                    id,
                    Sent {
                        version,
                        offset,
                        snap,
                        at: Instant::now(),
                    },
                );
            }
        }
    }

    fn client_input(&self, conn: &Conn, id: &str, data: &[u8]) {
        let Some(s) = self.session(id) else { return };
        let owner_matches = lock(&s.inner).size_owner == Some(conn.id);
        if !owner_matches {
            let size = lock(&conn.sizes).get(id).copied();
            if let Some((cols, rows)) = size {
                s.resize(cols, rows, Some(conn.id));
            }
        }
        let _ = s.input(data);
    }

    fn client_resize(&self, conn: &Conn, id: &str, cols: u16, rows: u16) {
        let Some(s) = self.session(id) else { return };
        let (cols, rows) = clamp_size(cols, rows);
        lock(&conn.sizes).insert(id.to_string(), (cols, rows));
        s.resize(cols, rows, Some(conn.id));
    }

    fn log_reject(&self, why: &str) {
        let mut r = lock(&self.rejects);
        if r.last.is_some_and(|t| t.elapsed() < REJECT_LOG_EVERY) {
            r.suppressed += 1;
            return;
        }
        let n = std::mem::take(&mut r.suppressed);
        r.last = Some(Instant::now());
        drop(r);
        if n > 0 {
            log(&format!("rejected connection: {why} ({n} more suppressed)"));
        } else {
            log(&format!("rejected connection: {why}"));
        }
    }

    fn serve_conn(self: &Arc<Self>, stream: UnixStream, conn_slot: Slot, pending: Slot) {
        let _slot = conn_slot;
        let hello = match server::handshake_server(&stream, &self.secret, &self.cfg.version) {
            Ok(h) => h,
            Err(e) => {
                self.log_reject(&e.to_string());
                return;
            }
        };
        drop(pending);
        let _ = stream.set_write_timeout(Some(write_timeout()));
        let Ok(w) = stream.try_clone() else { return };
        let conn = Arc::new(Conn {
            id: self.next_conn.fetch_add(1, Ordering::SeqCst),
            w: Mutex::new(w),
            watched: Mutex::new(Vec::new()),
            scroll: Mutex::new(HashMap::new()),
            sizes: Mutex::new(HashMap::new()),
            closed: AtomicBool::new(false),
            attached: AtomicBool::new(false),
            client: hello.client,
            pending_rx: Mutex::new(None),
        });
        let closed = |reason: &str| {
            let _ = conn.write(&ServerMsg::Closed {
                reason: reason.into(),
            });
        };
        while let Ok(Some(frame)) = read_frame(&mut &stream) {
            match frame {
                Frame::Bin(b) if b.tag == TAG_INPUT => {
                    if b.data.len() > MAX_INPUT {
                        closed("input frame too large");
                        break;
                    }
                    self.client_input(&conn, &b.session, &b.data);
                }
                Frame::Json(v) => match serde_json::from_value::<ClientMsg>(v) {
                    Ok(ClientMsg::Request(req)) => {
                        let stop = req.method == method::RUNNER_STOP;
                        let resp = self.respond(&conn, req);
                        if conn.write(&ServerMsg::Response(resp)).is_err() {
                            break;
                        }
                        let pending = lock(&conn.pending_rx).take();
                        if let Some(rx) = pending {
                            let (rt, conn) = (self.clone(), conn.clone());
                            thread::spawn(move || rt.pusher(&conn, rx));
                        }
                        if stop {
                            log("stop requested");
                            self.stop.store(true, Ordering::SeqCst);
                        }
                    }
                    Ok(ClientMsg::Resize(r)) => {
                        if lock(&conn.watched).contains(&r.session) {
                            self.client_resize(&conn, &r.session, r.cols, r.rows);
                        }
                    }
                    Ok(ClientMsg::Scroll(s)) => {
                        if lock(&conn.watched).contains(&s.session) {
                            lock(&conn.scroll).insert(s.session, s.offset as usize);
                        }
                    }
                    Ok(ClientMsg::Focus(_)) => {}
                    Ok(ClientMsg::Detach) => break,
                    Err(_) => {
                        closed("bad message");
                        break;
                    }
                },
                Frame::Bin(_) => {
                    closed("unexpected binary frame");
                    break;
                }
            }
        }
        conn.closed.store(true, Ordering::SeqCst);
        let _ = stream.shutdown(std::net::Shutdown::Both);
    }
}

const LOG_CAP: u64 = 5 << 20;

#[allow(clippy::unnecessary_cast)]
fn rotate_log(dir: &Path, fds: &[RawFd], cap: u64) -> io::Result<bool> {
    use std::os::unix::fs::MetadataExt;
    let path = dir.join("runner.log");
    let meta = fs::symlink_metadata(&path)?;
    if meta.len() <= cap {
        return Ok(false);
    }
    let mut old = path.clone().into_os_string();
    old.push(".1");
    fs::rename(&path, PathBuf::from(old))?;
    let fresh = OpenOptions::new()
        .create(true)
        .append(true)
        .custom_flags(libc::O_NOFOLLOW)
        .mode(0o600)
        .open(&path)?;
    for &fd in fds {
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstat(fd, &mut st) } == 0
            && st.st_ino as u64 == meta.ino()
            && st.st_dev as u64 == meta.dev()
        {
            unsafe { libc::dup2(fresh.as_raw_fd(), fd) };
        }
    }
    Ok(true)
}

fn take_lock(dir: &Path) -> io::Result<File> {
    let f = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(dir.join("runner.lock"))?;
    if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            "runner already running",
        ));
    }
    Ok(f)
}

fn load_client_secret(dir: &Path) -> io::Result<String> {
    let path = dir.join("client.secret");
    match secret::read(&path) {
        Ok(s) if s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()) => Ok(s),
        Ok(_) => {
            fs::remove_file(&path)?;
            secret::write_new(&path)
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => secret::write_new(&path),
        Err(e) => Err(e),
    }
}

pub fn serve(cfg: RunnerConfig) -> io::Result<()> {
    ensure_private_dir(&cfg.dir)?;
    ensure_private_dir(&cfg.dir.join("sessions"))?;
    ensure_private_leaf(&cfg.homes)?;
    let _lock = take_lock(&cfg.dir)?;
    write_atomic(
        &cfg.dir.join("runner.pid"),
        std::process::id().to_string().as_bytes(),
    )?;
    let secret = load_client_secret(&cfg.dir)?;
    let rt = Arc::new(Runner {
        cfg: cfg.clone(),
        secret,
        started: Instant::now(),
        sessions: Mutex::new(HashMap::new()),
        hub: Arc::new(Hub::default()),
        create_lock: Mutex::new(()),
        key_lock: Mutex::new(()),
        layout_lock: Mutex::new(()),
        stop: AtomicBool::new(false),
        conns: Arc::new(AtomicUsize::new(0)),
        pending: Arc::new(AtomicUsize::new(0)),
        rejects: Mutex::new(Rejects::default()),
        next_conn: AtomicU64::new(1),
    });
    log(&format!("runner {} starting", cfg.version));
    let intents = rt.intents();
    restore::recover_claims(&intents);
    let exits = rt.hub.subscribe();
    let delay = restore::forget_delay();
    thread::spawn(move || {
        while let Ok(v) = exits.recv() {
            if let Ok(ev) = serde_json::from_value::<EventMsg>(v) {
                restore::on_event(&intents, &ev, delay);
            }
        }
    });
    rt.adopt_all();
    let ep = Endpoint::new_runner(&cfg.dir)?;
    let listener: UnixListener = endpoint::bind(&ep)?;
    listener.set_nonblocking(true)?;

    let ticker = rt.clone();
    thread::spawn(move || {
        while !ticker.stop.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(250));
            let _ = rotate_log(&ticker.cfg.dir, &[1, 2], LOG_CAP);
            for s in ticker.all() {
                s.tick();
            }
        }
    });

    let revoker = rt.clone();
    thread::spawn(move || {
        while !revoker.stop.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_secs(1));
            revoker.retry_revocations();
        }
    });

    while !rt.stop.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((stream, _)) => {
                if stream.set_nonblocking(false).is_err() {
                    continue;
                }
                if server::accept_check(&stream).is_err() {
                    rt.log_reject("other user");
                    continue;
                }
                let Some(pending) = Slot::take(&rt.pending, MAX_PENDING) else {
                    rt.log_reject("too many pending connections");
                    continue;
                };
                let Some(conn_slot) = Slot::take(&rt.conns, MAX_CONNS) else {
                    rt.log_reject("too many connections");
                    continue;
                };
                let rt = rt.clone();
                thread::spawn(move || rt.serve_conn(stream, conn_slot, pending));
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(30));
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => {
                log(&format!("accept failed: {e}"));
                thread::sleep(Duration::from_millis(100));
            }
        }
    }
    thread::sleep(Duration::from_millis(100));
    match &ep {
        Endpoint::Path(p) => {
            let _ = fs::remove_file(p);
        }
        Endpoint::Abstract(_) => {
            let _ = fs::remove_file(cfg.dir.join("runner.addr"));
        }
    }
    let _ = fs::remove_file(cfg.dir.join("runner.pid"));
    log("runner stopped");
    Ok(())
}

// Same-user processes can read each other's initial environment on macOS, so the runner and shims start from an allowlist.
pub const ENV_CLEAN: &str = "TMGR_RUNNER_ENV_CLEAN";

pub fn daemon_env() -> Vec<(std::ffi::OsString, std::ffi::OsString)> {
    const KEEP: &[&str] = &[
        "HOME",
        "PATH",
        "USER",
        "LOGNAME",
        "SHELL",
        "LANG",
        "TMPDIR",
        "COLORTERM",
        "TERM_PROGRAM",
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "XDG_RUNTIME_DIR",
        "TMGR_RUNNER_DIR",
        "TMGR_RUNNER_HOMES",
        "TMGR_T0_QUIET_MS",
        "TMGR_AGENTS_DIR",
        "TMGR_LOCAL_ROOT",
        "TMGR_RUNNER_WRITE_TIMEOUT_MS",
        "TMGR_TEST_HOLD_BEFORE_SPAWN",
        "TMGR_TEST_SHIM_NO_CWD_ID",
        ENV_CLEAN,
    ];
    std::env::vars_os()
        .filter(|(k, _)| {
            k.to_str()
                .is_some_and(|k| KEEP.contains(&k) || k.starts_with("LC_"))
        })
        .collect()
}

pub fn start_background(cfg: &RunnerConfig) -> io::Result<u32> {
    if let Ok(mut c) = Client::connect(&cfg.dir) {
        return Ok(c.status()?.pid);
    }
    ensure_private_dir(&cfg.dir)?;
    let log_file = OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(cfg.dir.join("runner.log"))?;
    let mut cmd = Command::new(&cfg.self_exe);
    cmd.args(["runner", "serve"])
        .env_clear()
        .envs(daemon_env())
        .env(ENV_CLEAN, "1")
        .env("TMGR_RUNNER_DIR", &cfg.dir)
        .env("TMGR_RUNNER_HOMES", &cfg.homes)
        .env("TMGR_T0_QUIET_MS", cfg.quiet_ms.to_string())
        .stdin(Stdio::null())
        .stdout(log_file.try_clone()?)
        .stderr(log_file);
    unsafe {
        cmd.pre_exec(|| {
            match libc::fork() {
                -1 => return Err(io::Error::last_os_error()),
                0 => {}
                _ => libc::_exit(0),
            }
            libc::setsid();
            Ok(())
        });
    }
    let status = cmd.spawn()?.wait()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "runner launcher failed ({status})"
        )));
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(mut c) = Client::connect(&cfg.dir) {
            if let Ok(st) = c.status() {
                return Ok(st.pid);
            }
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "runner did not start within 5 s; see runner.log",
            ));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

const MAX_TEXT_BYTES: usize = 512 << 10;

fn cap_text(lines: &mut Vec<String>, max: usize) {
    let mut total: usize = lines.iter().map(|l| l.len() + 8).sum();
    let mut drop = 0;
    while total > max && drop < lines.len() {
        total -= lines[drop].len() + 8;
        drop += 1;
    }
    lines.drain(..drop);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::thread::JoinHandle;

    #[test]
    fn session_text_drops_the_oldest_lines_to_fit_one_frame() {
        let mut lines: Vec<String> = (0..10).map(|i| format!("{i}{}", "x".repeat(99))).collect();
        cap_text(&mut lines, 5 * 108);
        assert_eq!(lines.len(), 5);
        assert!(lines[0].starts_with('5') && lines[4].starts_with('9'));
        let mut small = vec!["a".to_string()];
        cap_text(&mut small, 100);
        assert_eq!(small, ["a"]);
    }

    #[test]
    fn default_names_are_sanitized_and_bounded() {
        assert_eq!(default_name("/usr/bin/claude"), "claude");
        assert_eq!(default_name("/tmp/a\u{1b}]0;x\u{7}b"), "a?]0;x?b");
        assert_eq!(default_name(&format!("/tmp/{}", "n".repeat(200))).len(), 64);
        assert_eq!(default_name("/"), "/");
        assert_eq!(default_name(""), "session");
    }

    const ID: &str = "s_abcdefghij";
    const REAL: &str = "feedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedfacefeedface";

    fn fake_shim(dir: &Path, secret: &'static str) -> (JoinHandle<Vec<u8>>, UnixListener) {
        let ep = Endpoint::new_session(dir, ID).unwrap();
        let l = endpoint::bind(&ep).unwrap();
        let l2 = l.try_clone().unwrap();
        let h = thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let Some(Frame::Json(v)) = read_frame(&mut s).unwrap() else {
                return Vec::new();
            };
            let ToShim::Adopt { nonce_r } = serde_json::from_value(v).unwrap() else {
                return Vec::new();
            };
            let nonce_sh = auth::nonce();
            let challenge = FromShim::AdoptChallenge {
                proof_sh: auth::proof(secret, auth::SHIM, &nonce_r, &nonce_sh),
                nonce_sh: nonce_sh.clone(),
            };
            write_json(&mut s, &challenge).unwrap();
            let mut rest = Vec::new();
            let Ok(Some(Frame::Json(v))) = read_frame(&mut s) else {
                return rest;
            };
            if let Ok(ToShim::AdoptAuth { proof_r }) = serde_json::from_value(v.clone()) {
                if auth::verify(secret, auth::RUNNER_ADOPT, &nonce_sh, &nonce_r, &proof_r) {
                    let hello = FromShim::Hello(ShimHello {
                        pid: 1,
                        child_pid: 2,
                        started_at_ms: 3,
                        exit: None,
                        cwd_dev: None,
                        cwd_ino: None,
                    });
                    write_json(&mut s, &hello).unwrap();
                    return b"ok".to_vec();
                }
            }
            s.read_to_end(&mut rest).unwrap();
            rest
        });
        (h, l2)
    }

    fn short_dir() -> tempfile::TempDir {
        let t = tempfile::Builder::new()
            .prefix("td")
            .tempdir_in("/tmp")
            .unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(t.path(), fs::Permissions::from_mode(0o700)).unwrap();
        t
    }

    #[test]
    fn adoption_succeeds_with_the_shared_secret() {
        let t = short_dir();
        let dir = t.path().join("r");
        let (h, _l) = fake_shim(&dir, REAL);
        let (_, hello) = adopt_shim(&dir, ID, REAL, None).unwrap();
        assert_eq!(hello.child_pid, 2);
        assert_eq!(h.join().unwrap(), b"ok");
    }

    #[test]
    fn fake_shim_is_rejected_and_learns_nothing() {
        let t = short_dir();
        let dir = t.path().join("r");
        let (h, _l) = fake_shim(&dir, "some-other-secret");
        let e = adopt_shim(&dir, ID, REAL, None).err().unwrap();
        assert_eq!(e.kind(), io::ErrorKind::PermissionDenied);
        assert!(e.to_string().contains("identity check failed"), "{e}");
        assert!(h.join().unwrap().is_empty());
    }

    #[test]
    fn log_over_the_cap_is_rotated_once_and_the_fd_follows() {
        use std::io::Write;
        let t = short_dir();
        let log_path = t.path().join("runner.log");
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .unwrap();
        f.write_all(&[b'x'; 2000]).unwrap();
        assert!(!rotate_log(t.path(), &[f.as_raw_fd()], 5000).unwrap());
        let fd = unsafe { libc::dup(f.as_raw_fd()) };
        assert!(rotate_log(t.path(), &[fd], 1000).unwrap());
        assert_eq!(
            fs::metadata(t.path().join("runner.log.1")).unwrap().len(),
            2000
        );
        assert_eq!(fs::metadata(&log_path).unwrap().len(), 0);
        let mut moved = unsafe { <File as std::os::fd::FromRawFd>::from_raw_fd(fd) };
        moved.write_all(b"after\n").unwrap();
        assert_eq!(fs::metadata(&log_path).unwrap().len(), 6);
        assert!(!rotate_log(t.path(), &[fd], 1000).unwrap());
    }

    #[test]
    fn a_stalled_subscriber_is_dropped_at_the_queue_cap_while_others_keep_receiving() {
        let hub = Hub::default();
        let stalled = hub.subscribe();
        let live = hub.subscribe();
        let mut seen = 0;
        for i in 0..1000 {
            hub.broadcast(serde_json::json!({"n": i}));
            while live.try_recv().is_ok() {
                seen += 1;
            }
        }
        assert_eq!(seen, 1000);
        assert_eq!(stalled.try_iter().count(), CLIENT_QUEUE);
        assert!(stalled.try_recv().is_err());
        assert_eq!(lock(&hub.subs).len(), 1);
    }
}
