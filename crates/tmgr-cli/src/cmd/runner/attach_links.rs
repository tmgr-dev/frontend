use super::remote::{self, RunView};
use super::remote_attach::{age, text_chunks};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{Read, Write};
use std::process::{ChildStdin, ChildStdout};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tmgr_core::config::{self, Config};
use tmgr_core::creds::{Backend, CredentialStore};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::redact::Secret;
use tmgr_ipc::api::{EventMsg, Line, ScreenPush};
use tmgr_ipc::proto::valid_session_id;
use tmgr_link::attach::{read_incoming, write_outgoing, Incoming, Outgoing};
use tmgr_link::leader::{self, Client, KeyFile};
use tmgr_link::store::{Paths, Store};
use tmgr_runner::attach::SessionRow;

const MAX_RUNS_PER_LINK: usize = 200;
const STREAMS_PER_LINK: usize = 2;
const OUT_QUEUE: usize = 256;
const MAX_NAME: usize = 64;
const MAX_DETAIL: usize = 512;
const MAX_WHY: usize = 200;

#[derive(Clone, Copy, Debug)]
pub(super) struct Timing {
    pub poll: Duration,
    pub call: Duration,
    pub open: Duration,
    pub halt: Duration,
    pub backoff_min: Duration,
    pub backoff_max: Duration,
    pub reconnect_after: Duration,
    pub reconnect_requests: u32,
    pub heartbeat: Duration,
    pub silence: Duration,
    pub stream_renew: Duration,
    pub cooldown: Duration,
    pub shutdown_wait: Duration,
    pub kill_grace: Duration,
    pub screen_gap: Duration,
}

impl Default for Timing {
    fn default() -> Timing {
        Timing {
            poll: Duration::from_secs(5),
            call: Duration::from_secs(10),
            open: Duration::from_secs(20),
            halt: Duration::from_secs(20),
            backoff_min: Duration::from_secs(1),
            backoff_max: Duration::from_secs(30),
            reconnect_after: Duration::from_secs(9 * 60),
            reconnect_requests: 200,
            heartbeat: Duration::from_secs(10),
            silence: Duration::from_secs(45),
            stream_renew: Duration::from_secs(55 * 60),
            cooldown: Duration::from_secs(30),
            shutdown_wait: Duration::from_secs(1),
            kill_grace: Duration::from_secs(1),
            screen_gap: Duration::from_millis(33),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum DialErr {
    Offline(String),
    HostKeyChanged,
    NoAccess(String),
    Refused(String),
    Removed,
    Unavailable(String),
}

impl DialErr {
    fn why(&self, link: &str) -> String {
        match self {
            DialErr::Offline(_) => format!("{link} is offline"),
            DialErr::HostKeyChanged => format!("{link}: the host key changed"),
            DialErr::NoAccess(_) => format!("{link}: no access"),
            DialErr::Refused(_) => format!("{link} is busy: no free connection slot"),
            DialErr::Removed => format!("{link} is no longer linked"),
            DialErr::Unavailable(m) => format!("{link}: {m}"),
        }
    }
}

pub(super) fn classify(e: &Error) -> DialErr {
    let m = e.message.to_ascii_lowercase();
    if e.kind == Kind::NotFound && m.starts_with("no follower named") {
        DialErr::Removed
    } else if m.contains("host key mismatch") {
        DialErr::HostKeyChanged
    } else if e.kind == Kind::Forbidden
        || m.contains("permission denied")
        || m.contains("refused this link key")
        || m.contains("forbidden")
        || m.contains("revoked")
    {
        DialErr::NoAccess(e.message.clone())
    } else if m.contains("worker: refused") || m.contains("refused the handshake") {
        DialErr::Refused(e.message.clone())
    } else {
        DialErr::Offline(e.message.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Binding {
    Is(String),
    Gone,
    Unreadable,
}

pub(super) type Kill = Box<dyn FnOnce() + Send>;
pub(super) type Pair = (Box<dyn Read + Send>, Box<dyn Write + Send>);

pub(super) trait Dialer: Send + Sync + 'static {
    fn dial(&self, link: &str) -> std::result::Result<Box<dyn LinkConn>, DialErr>;

    fn binding(&self, _link: &str) -> Binding {
        Binding::Is(String::new())
    }

    fn shutdown(&self) {}

    fn forget(&self) {}
}

pub(super) trait LinkConn: Send {
    fn call(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value>;
    fn into_stream(self: Box<Self>) -> Result<(Pair, Kill)>;
}

pub(super) trait KeySource: Send + Sync + 'static {
    fn load(&self) -> Result<Secret>;
}

struct StoreKey {
    dir: std::path::PathBuf,
    preferred: Option<Backend>,
}

impl KeySource for StoreKey {
    fn load(&self) -> Result<Secret> {
        leader::load_secret(&CredentialStore::new(
            &self.dir,
            leader::KEY_ACCOUNT,
            self.preferred,
        ))
    }
}

#[derive(Default)]
struct Live {
    closed: bool,
    next: u64,
    dirs: HashMap<u64, std::path::PathBuf>,
}

struct HeldKey {
    key: Option<KeyFile>,
    id: u64,
    live: Arc<Mutex<Live>>,
}

impl std::ops::Deref for HeldKey {
    type Target = KeyFile;

    fn deref(&self) -> &KeyFile {
        self.key.as_ref().expect("the key lives until drop")
    }
}

impl Drop for HeldKey {
    fn drop(&mut self) {
        self.key.take();
        lock(&self.live).dirs.remove(&self.id);
    }
}

pub(super) struct SshDialer {
    paths: Paths,
    source: Box<dyn KeySource>,
    root: std::path::PathBuf,
    live: Arc<Mutex<Live>>,
    closed: AtomicBool,
    loading: Mutex<()>,
    held: Mutex<Option<Secret>>,
}

impl SshDialer {
    pub(super) fn new() -> Result<SshDialer> {
        let paths = Paths::from_env()?;
        let cfg = Config::load(&config::config_dir()?)?;
        let preferred = cfg.credential_store.as_deref().and_then(Backend::parse);
        let source = StoreKey {
            dir: paths.config.clone(),
            preferred,
        };
        let root = leader::key_root();
        leader::sweep_stale_keys(&root);
        Ok(SshDialer::with_source(paths, Box::new(source), root))
    }

    fn with_source(
        paths: Paths,
        source: Box<dyn KeySource>,
        root: std::path::PathBuf,
    ) -> SshDialer {
        SshDialer {
            paths,
            source,
            root,
            live: Arc::new(Mutex::new(Live::default())),
            closed: AtomicBool::new(false),
            loading: Mutex::new(()),
            held: Mutex::new(None),
        }
    }

    fn fresh_key(&self) -> std::result::Result<HeldKey, DialErr> {
        let closing = || DialErr::Offline("the hub is shutting down".into());
        let known = lock(&self.held).clone();
        let secret = match known {
            Some(s) => s,
            None => {
                let _one_at_a_time = lock(&self.loading);
                let known = lock(&self.held).clone();
                match known {
                    Some(s) => s,
                    None => {
                        if self.closed.load(Ordering::SeqCst) {
                            return Err(closing());
                        }
                        let secret = self
                            .source
                            .load()
                            .map_err(|e| DialErr::Unavailable(cap(&e.message, MAX_WHY)))?;
                        let mut held = lock(&self.held);
                        if self.closed.load(Ordering::SeqCst) {
                            return Err(closing());
                        }
                        *held = Some(secret.clone());
                        secret
                    }
                }
            }
        };
        let mut live = lock(&self.live);
        if live.closed {
            return Err(closing());
        }
        let key = leader::materialise_secret_in(&secret, &self.root)
            .map_err(|e| DialErr::Unavailable(cap(&e.message, MAX_WHY)))?;
        let id = live.next;
        live.next += 1;
        live.dirs.insert(id, key.dir().to_path_buf());
        Ok(HeldKey {
            key: Some(key),
            id,
            live: Arc::clone(&self.live),
        })
    }
}

fn describe(store: &tmgr_link::store::Store, link: &str) -> Option<String> {
    let f = store.followers.get(link)?;
    let mut text = format!(
        "{}@{}:{}#{}",
        f.user, f.host, f.port, f.host_key_fingerprint
    );
    if let Some(hop) = store.jumps.get(link) {
        match store.followers.get(hop) {
            Some(h) => text.push_str(&format!(
                "|{hop}={}@{}:{}#{}",
                h.user, h.host, h.port, h.host_key_fingerprint
            )),
            None => text.push_str(&format!("|{hop}=?")),
        }
    }
    Some(text)
}

impl Dialer for SshDialer {
    fn dial(&self, link: &str) -> std::result::Result<Box<dyn LinkConn>, DialErr> {
        if self.binding(link) == Binding::Gone {
            return Err(DialErr::Removed);
        }
        let key = self.fresh_key()?;
        let mut client =
            Client::dial_with_key(&self.paths, link, &key).map_err(|e| classify(&e))?;
        client.hello().map_err(|e| classify(&e))?;
        client.disarm();
        Ok(Box::new(SshConn(client)))
    }

    fn binding(&self, link: &str) -> Binding {
        match Store::load(&self.paths) {
            Ok(store) => describe(&store, link).map_or(Binding::Gone, Binding::Is),
            Err(_) => Binding::Unreadable,
        }
    }

    fn shutdown(&self) {
        self.closed.store(true, Ordering::SeqCst);
        lock(&self.held).take();
        let mut live = lock(&self.live);
        live.closed = true;
        for (_, dir) in live.dirs.drain() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    fn forget(&self) {
        lock(&self.held).take();
    }
}

struct SshConn(Client);

impl LinkConn for SshConn {
    fn call(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        remote::check(self.0.call_within(method, params, timeout)?)
    }

    fn into_stream(mut self: Box<Self>) -> Result<(Pair, Kill)> {
        let (out, input) = self.0.take_stream()?;
        let owner = Arc::new(Mutex::new(Some(self.0)));
        let killer = Arc::clone(&owner);
        Ok((
            (
                Box::new(HeldRead(out, Arc::clone(&owner))),
                Box::new(HeldWrite(input, owner)),
            ),
            Box::new(move || {
                lock(&killer).take();
            }),
        ))
    }
}

type Owner = Arc<Mutex<Option<Client>>>;

struct HeldRead(ChildStdout, Owner);
struct HeldWrite(ChildStdin, Owner);

impl Read for HeldRead {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let _keep = &self.1;
        self.0.read(buf)
    }
}

impl Write for HeldWrite {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.0.flush()
    }
}

impl Drop for HeldWrite {
    fn drop(&mut self) {
        let _ = self.0.flush();
        thread::sleep(Duration::from_millis(100));
        lock(&self.1).take();
    }
}

pub(super) fn followers(paths: &Paths) -> Vec<String> {
    Store::load(paths)
        .map(|s| {
            s.followers
                .keys()
                .filter(|n| tmgr_link::grammar::link_name(n).is_ok())
                .filter(|n| tmgr_link::jump::referenced_by(&s, n).is_empty())
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LinkState {
    Connecting,
    Online { latency_ms: u64 },
    Busy,
    Offline,
    NoAccess,
    HostKeyChanged,
    Removed,
}

#[derive(Debug)]
pub(super) struct Permit(Option<Arc<AtomicBool>>);

impl Permit {
    #[cfg(test)]
    pub(super) fn none() -> Permit {
        Permit(None)
    }

    fn held(flag: &Arc<AtomicBool>) -> Permit {
        flag.store(true, Ordering::SeqCst);
        Permit(Some(Arc::clone(flag)))
    }
}

impl Drop for Permit {
    fn drop(&mut self) {
        if let Some(flag) = &self.0 {
            flag.store(false, Ordering::SeqCst);
        }
    }
}

#[derive(Debug)]
pub(super) enum LinkMsg {
    State {
        link: String,
        state: LinkState,
    },
    Runs {
        link: String,
        runs: Vec<RunView>,
    },
    Opened {
        id: String,
        input: bool,
    },
    Screen(Box<ScreenPush>, Permit),
    Event(EventMsg, Permit),
    Notice {
        link: String,
        id: Option<String>,
        text: String,
    },
    StreamClosed {
        id: String,
        why: String,
        reopen: bool,
    },
    Halted {
        id: String,
        result: std::result::Result<(), String>,
    },
}

fn cap(s: &str, max: usize) -> String {
    tmgr_core::text::truncate_chars(s, max).0
}

fn shorten(mut r: RunView) -> RunView {
    r.name = cap(&r.name, MAX_NAME);
    r.agent = r.agent.map(|a| cap(&a, MAX_NAME));
    r.task = r.task.map(|t| cap(&t, MAX_NAME));
    r.state.detail = r.state.detail.map(|d| cap(&d, MAX_DETAIL));
    r
}

pub(super) fn view_id(link: &str, run: &str) -> String {
    format!("{link}/{run}")
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Target<'a> {
    Local(&'a str),
    Remote { link: String, run: String },
    Unknown,
}

pub(super) fn split_local(ids: &[String]) -> Vec<String> {
    ids.iter()
        .filter(|id| valid_session_id(id))
        .cloned()
        .collect()
}

#[derive(Default)]
struct Entry {
    runs: Vec<RunView>,
    removed: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Wants {
    pub open: Vec<(String, String, String)>,
    pub close: Vec<String>,
    pub over_budget: Vec<String>,
}

pub(super) struct RemoteState {
    links: BTreeMap<String, Entry>,
    open: BTreeSet<String>,
}

impl RemoteState {
    pub(super) fn new(links: &[String]) -> RemoteState {
        RemoteState {
            links: links
                .iter()
                .filter(|l| tmgr_link::grammar::link_name(l).is_ok())
                .map(|l| (l.clone(), Entry::default()))
                .collect(),
            open: BTreeSet::new(),
        }
    }

    pub(super) fn target_of<'a>(&self, id: &'a str) -> Target<'a> {
        if valid_session_id(id) {
            return Target::Local(id);
        }
        match id.split_once('/') {
            Some((link, run)) if valid_session_id(run) && self.listed(link, run) => {
                Target::Remote {
                    link: link.to_string(),
                    run: run.to_string(),
                }
            }
            _ => Target::Unknown,
        }
    }

    fn listed(&self, link: &str, run: &str) -> bool {
        self.links
            .get(link)
            .is_some_and(|e| !e.removed && e.runs.iter().any(|r| r.id == run))
    }

    pub(super) fn ingest_runs(&mut self, link: &str, runs: Vec<RunView>) -> Vec<RunView> {
        let Some(entry) = self.links.get_mut(link).filter(|e| !e.removed) else {
            return Vec::new();
        };
        let mut kept: Vec<RunView> = Vec::new();
        for r in runs {
            if kept.len() >= MAX_RUNS_PER_LINK {
                break;
            }
            if valid_session_id(&r.id) && !kept.iter().any(|k| k.id == r.id) {
                kept.push(shorten(r));
            }
        }
        entry.runs = kept.clone();
        kept
    }

    pub(super) fn ingest(&mut self, link: &str, msg: Incoming) -> Option<Incoming> {
        match msg {
            Incoming::Screen(mut push) => {
                if !self.listed(link, &push.session) {
                    return None;
                }
                push.session = view_id(link, &push.session);
                Some(Incoming::Screen(push))
            }
            Incoming::Event(mut ev) => {
                if !self.listed(link, &ev.session) {
                    return None;
                }
                if let Some(r) = self
                    .links
                    .get_mut(link)
                    .and_then(|e| e.runs.iter_mut().find(|r| r.id == ev.session))
                {
                    r.state = ev.state.clone();
                }
                ev.state.detail = ev.state.detail.map(|d| cap(&d, MAX_DETAIL));
                ev.session = view_id(link, &ev.session);
                Some(Incoming::Event(ev))
            }
            other => Some(other),
        }
    }

    pub(super) fn rows(&self, link: &str) -> Vec<SessionRow> {
        let Some(entry) = self.links.get(link).filter(|e| !e.removed) else {
            return Vec::new();
        };
        entry
            .runs
            .iter()
            .map(|r| SessionRow {
                id: view_id(link, &r.id),
                name: r.name.clone(),
                state: r.state.clone(),
                age: age(r.state.since_ms),
                you: None,
                esc: matches!(r.agent.as_deref(), Some("claude-code" | "codex")),
                resumed_from: None,
            })
            .collect()
    }

    pub(super) fn remove_link(&mut self, link: &str) {
        if let Some(e) = self.links.get_mut(link) {
            e.removed = true;
            e.runs.clear();
        }
    }

    fn restore_link(&mut self, link: &str) {
        if let Some(e) = self.links.get_mut(link) {
            e.removed = false;
        }
    }

    fn vanished(&self, link: &str) -> Vec<String> {
        self.open
            .iter()
            .filter(|id| {
                !matches!(self.target_of(id), Target::Remote { link: l, .. } if l == link)
                    && id.split_once('/').is_some_and(|(l, _)| l == link)
            })
            .cloned()
            .collect()
    }

    fn mark_open(&mut self, id: &str) {
        self.open.insert(id.to_string());
    }

    fn mark_closed(&mut self, id: &str) {
        self.open.remove(id);
    }

    pub(super) fn want_streams(&self, visible: &[String]) -> Wants {
        let mut wants = Wants::default();
        let mut per_link: HashMap<String, usize> = HashMap::new();
        for id in &self.open {
            match self.target_of(id) {
                Target::Remote { link, .. } if visible.contains(id) => {
                    *per_link.entry(link).or_default() += 1;
                }
                _ => wants.close.push(id.clone()),
            }
        }
        let mut seen = BTreeSet::new();
        for id in visible {
            if self.open.contains(id) || !seen.insert(id.as_str()) {
                continue;
            }
            if let Target::Remote { link, run } = self.target_of(id) {
                let n = per_link.entry(link.clone()).or_default();
                if *n < STREAMS_PER_LINK {
                    *n += 1;
                    wants.open.push((id.clone(), link, run));
                } else {
                    wants.over_budget.push(id.clone());
                }
            }
        }
        wants
    }
}

fn merge_screens(old: ScreenPush, new: ScreenPush) -> ScreenPush {
    if new.full {
        return new;
    }
    if old.full {
        let mut old = old;
        if let Some(snap) = old.snapshot.as_mut() {
            for (i, line) in new.lines {
                if i >= snap.lines.len() {
                    snap.lines.resize(i + 1, Line { spans: Vec::new() });
                }
                snap.lines[i] = line;
            }
            snap.cursor = new.cursor;
            if let Some(m) = new.modes {
                snap.modes = m;
            }
            snap.scroll_offset = new.scroll_offset;
            snap.scroll_max = new.scroll_max;
        }
        return old;
    }
    let mut lines: BTreeMap<usize, Line> = old.lines.into_iter().collect();
    lines.extend(new.lines);
    ScreenPush {
        lines: lines.into_iter().collect(),
        modes: new.modes.or(old.modes),
        ..new
    }
}

#[derive(Default)]
struct Slot {
    screen: Option<ScreenPush>,
    event: Option<EventMsg>,
}

#[derive(Default)]
struct NoticeGate {
    last: Option<Instant>,
    held: Option<String>,
}

struct Flow {
    notices: Mutex<NoticeGate>,
    slot: Mutex<Slot>,
    wake: Condvar,
    busy: Arc<AtomicBool>,
}

impl Flow {
    fn new() -> Flow {
        Flow {
            notices: Mutex::new(NoticeGate::default()),
            slot: Mutex::new(Slot::default()),
            wake: Condvar::new(),
            busy: Arc::new(AtomicBool::new(false)),
        }
    }

    fn notice(&self, text: String) -> Option<String> {
        let mut gate = lock(&self.notices);
        if gate.last.is_some_and(|at| at.elapsed() < NOTICE_GAP) {
            gate.held = Some(text);
            return None;
        }
        gate.last = Some(Instant::now());
        gate.held = None;
        Some(text)
    }

    fn notice_due(&self) -> bool {
        let gate = lock(&self.notices);
        gate.held.is_some() && gate.last.is_none_or(|at| at.elapsed() >= NOTICE_GAP)
    }

    fn take_due_notice(&self) -> Option<String> {
        let mut gate = lock(&self.notices);
        if gate.last.is_some_and(|at| at.elapsed() < NOTICE_GAP) {
            return None;
        }
        let text = gate.held.take()?;
        gate.last = Some(Instant::now());
        Some(text)
    }

    fn put_screen(&self, push: ScreenPush) {
        let mut slot = lock(&self.slot);
        slot.screen = Some(match slot.screen.take() {
            Some(old) => merge_screens(old, push),
            None => push,
        });
        drop(slot);
        self.wake.notify_one();
    }

    fn put_event(&self, ev: EventMsg) {
        lock(&self.slot).event = Some(ev);
        self.wake.notify_one();
    }

    fn put_back(&self, msg: LinkMsg) {
        let mut slot = lock(&self.slot);
        match msg {
            LinkMsg::Screen(push, _) => {
                slot.screen = Some(match slot.screen.take() {
                    Some(newer) => merge_screens(*push, newer),
                    None => *push,
                });
            }
            LinkMsg::Event(ev, _) => {
                slot.event.get_or_insert(ev);
            }
            _ => {}
        }
    }
}

enum Cmd {
    Halt(String, Option<Binding>),
    Retry,
    Shutdown,
}

struct Stream {
    gen: u64,
    link: String,
    out: Option<SyncSender<Outgoing>>,
    input: Option<bool>,
    utf8: Vec<u8>,
    kill: Option<Kill>,
}

struct Inner {
    dialer: Arc<dyn Dialer>,
    tx: SyncSender<LinkMsg>,
    timing: Timing,
    stop: AtomicBool,
    next_gen: AtomicU64,
    state: Mutex<RemoteState>,
    shown: Mutex<HashMap<String, LinkState>>,
    streams: Mutex<HashMap<String, Stream>>,
    cooldown: Mutex<HashMap<String, Instant>>,
    bound: Mutex<HashMap<String, Binding>>,
    key_notice: AtomicBool,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

impl Inner {
    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    fn emit(&self, mut msg: LinkMsg) -> bool {
        loop {
            if self.stopped() {
                return false;
            }
            match self.tx.try_send(msg) {
                Ok(()) => return true,
                Err(TrySendError::Full(back)) => {
                    msg = back;
                    thread::sleep(Duration::from_millis(5));
                }
                Err(TrySendError::Disconnected(_)) => return false,
            }
        }
    }

    fn set_state(&self, link: &str, state: LinkState) {
        let changed =
            lock(&self.shown).insert(link.to_string(), state.clone()) != Some(state.clone());
        if changed {
            self.emit(LinkMsg::State {
                link: link.to_string(),
                state,
            });
        }
    }

    fn online(&self, link: &str) -> bool {
        matches!(lock(&self.shown).get(link), Some(LinkState::Online { .. }))
    }

    fn in_cooldown(&self, id: &str) -> bool {
        let mut c = lock(&self.cooldown);
        match c.get(id) {
            Some(until) if *until > Instant::now() => true,
            Some(_) => {
                c.remove(id);
                false
            }
            None => false,
        }
    }

    fn end_stream(&self, id: &str, gen: u64) -> bool {
        let mut state = lock(&self.state);
        let mut streams = lock(&self.streams);
        if streams.get(id).is_some_and(|s| s.gen == gen) {
            let gone = streams.remove(id);
            state.mark_closed(id);
            drop(streams);
            drop(state);
            self.retire(gone.and_then(|s| s.kill));
            true
        } else {
            false
        }
    }

    fn retire(&self, kill: Option<Kill>) {
        if let Some(kill) = kill {
            let grace = self.timing.kill_grace;
            thread::spawn(move || {
                thread::sleep(grace);
                kill();
            });
        }
    }

    fn alive(&self, id: &str, gen: u64) -> bool {
        lock(&self.streams).get(id).is_some_and(|s| s.gen == gen)
    }

    fn drop_stream(&self, id: &str) {
        let mut state = lock(&self.state);
        let removed = lock(&self.streams).remove(id);
        state.mark_closed(id);
        drop(state);
        if let Some(s) = removed {
            if let Some(out) = &s.out {
                let _ = out.try_send(Outgoing::Detach);
            }
            self.retire(s.kill);
        }
    }

    fn drop_streams_where(&self, pick: impl Fn(&str, &Stream) -> bool) {
        let ids: Vec<String> = lock(&self.streams)
            .iter()
            .filter(|(id, s)| pick(id, s))
            .map(|(id, _)| id.clone())
            .collect();
        for id in ids {
            self.drop_stream(&id);
        }
    }

    fn lose_link(&self, link: &str) {
        self.drop_streams_where(|_, s| s.link == link);
    }
}

pub(super) struct Hub {
    inner: Arc<Inner>,
    ctl: HashMap<String, Sender<Cmd>>,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl Hub {
    pub(super) fn start(
        dialer: Arc<dyn Dialer>,
        links: Vec<String>,
        tx: SyncSender<LinkMsg>,
    ) -> Hub {
        Hub::start_with(dialer, links, tx, Timing::default())
    }

    pub(super) fn start_with(
        dialer: Arc<dyn Dialer>,
        links: Vec<String>,
        tx: SyncSender<LinkMsg>,
        timing: Timing,
    ) -> Hub {
        let state = RemoteState::new(&links);
        let names: Vec<String> = state.links.keys().cloned().collect();
        let inner = Arc::new(Inner {
            dialer,
            tx,
            timing,
            stop: AtomicBool::new(false),
            next_gen: AtomicU64::new(1),
            state: Mutex::new(state),
            shown: Mutex::new(HashMap::new()),
            streams: Mutex::new(HashMap::new()),
            cooldown: Mutex::new(HashMap::new()),
            bound: Mutex::new(HashMap::new()),
            key_notice: AtomicBool::new(false),
        });
        let mut ctl = HashMap::new();
        let mut threads = Vec::new();
        for link in names {
            let (cmd_tx, cmd_rx) = mpsc::channel();
            let inner = Arc::clone(&inner);
            ctl.insert(link.clone(), cmd_tx);
            threads.push(thread::spawn(move || control(&inner, &link, &cmd_rx)));
        }
        Hub {
            inner,
            ctl,
            threads: Mutex::new(threads),
        }
    }

    fn state(&self) -> MutexGuard<'_, RemoteState> {
        lock(&self.inner.state)
    }

    pub(super) fn rows(&self, link: &str) -> Vec<SessionRow> {
        self.state().rows(link)
    }

    pub(super) fn target_of<'a>(&self, id: &'a str) -> Target<'a> {
        self.state().target_of(id)
    }

    pub(super) fn want_streams(&self, visible: &[String]) -> Wants {
        self.state().want_streams(visible)
    }

    pub(super) fn halt(&self, link: &str, run: &str) -> std::result::Result<(), &'static str> {
        let id = view_id(link, run);
        let ok = matches!(
            self.state().target_of(&id),
            Target::Remote { link: l, run: r } if l == link && r == run
        );
        if !ok {
            return Err("that run is not listed on this machine");
        }
        self.ctl
            .get(link)
            .ok_or("unknown machine")?
            .send(Cmd::Halt(
                run.to_string(),
                lock(&self.inner.bound).get(link).cloned(),
            ))
            .map_err(|_| "the link is shutting down")
    }

    pub(super) fn retry_all(&self) {
        lock(&self.inner.cooldown).clear();
        self.inner.key_notice.store(false, Ordering::SeqCst);
        self.inner.dialer.forget();
        for tx in self.ctl.values() {
            let _ = tx.send(Cmd::Retry);
        }
    }

    pub(super) fn open(
        &self,
        id: &str,
        link: &str,
        run: &str,
        cols: u16,
        rows: u16,
    ) -> std::result::Result<(), &'static str> {
        if cols == 0 || rows == 0 {
            return Err("bad pane size");
        }
        if self.inner.stopped() {
            return Err("the link is shutting down");
        }
        let gen = {
            let mut state = lock(&self.inner.state);
            match state.target_of(id) {
                Target::Remote { link: l, run: r } if l == link && r == run => {}
                _ => return Err("not a session of a linked machine"),
            }
            if !self.inner.online(link) {
                return Err("the machine is not online");
            }
            if self.inner.in_cooldown(id) {
                return Err("try again in a moment");
            }
            let mut streams = lock(&self.inner.streams);
            if streams.contains_key(id) {
                return Ok(());
            }
            if streams.values().filter(|s| s.link == link).count() >= STREAMS_PER_LINK {
                return Err("too many remote panes on this machine");
            }
            let gen = self.inner.next_gen.fetch_add(1, Ordering::SeqCst);
            streams.insert(
                id.to_string(),
                Stream {
                    gen,
                    link: link.to_string(),
                    out: None,
                    input: None,
                    utf8: Vec::new(),
                    kill: None,
                },
            );
            state.mark_open(id);
            gen
        };
        let inner = Arc::clone(&self.inner);
        let spec = StreamSpec {
            id: id.to_string(),
            link: link.to_string(),
            run: run.to_string(),
            cols,
            rows,
            gen,
        };
        thread::spawn(move || stream_main(&inner, &spec));
        Ok(())
    }

    pub(super) fn close(&self, id: &str) {
        self.inner.drop_stream(id);
    }

    pub(super) fn input(&self, id: &str, bytes: &[u8]) -> std::result::Result<(), &'static str> {
        let mut streams = lock(&self.inner.streams);
        let s = streams.get_mut(id).ok_or("not attached")?;
        match s.input {
            Some(true) => {}
            Some(false) => return Err("read-only on this machine"),
            None => return Err("not attached yet"),
        }
        let out = s.out.clone().ok_or("not attached yet")?;
        for text in text_chunks(&mut s.utf8, bytes) {
            if out.try_send(Outgoing::Input(text)).is_err() {
                return Err("the link is busy, input dropped");
            }
        }
        Ok(())
    }

    pub(super) fn resize(&self, id: &str, cols: u16, rows: u16) {
        if cols == 0 || rows == 0 {
            return;
        }
        let streams = lock(&self.inner.streams);
        if let Some(Stream {
            out: Some(out),
            input: Some(true),
            ..
        }) = streams.get(id)
        {
            let _ = out.try_send(Outgoing::Resize { cols, rows });
        }
    }

    pub(super) fn scroll(&self, id: &str, offset: u32) {
        let streams = lock(&self.inner.streams);
        if let Some(Stream { out: Some(out), .. }) = streams.get(id) {
            let _ = out.try_send(Outgoing::Scroll(offset));
        }
    }

    fn signal(&self) {
        self.inner.stop.store(true, Ordering::SeqCst);
        for tx in self.ctl.values() {
            let _ = tx.send(Cmd::Shutdown);
        }
        self.inner.drop_streams_where(|_, _| true);
        self.inner.dialer.shutdown();
    }

    pub(super) fn shutdown(&self) {
        self.signal();
        let deadline = Instant::now() + self.inner.timing.shutdown_wait;
        let mut threads = lock(&self.threads);
        while Instant::now() < deadline && threads.iter().any(|t| !t.is_finished()) {
            thread::sleep(Duration::from_millis(5));
        }
        for t in threads.drain(..) {
            if t.is_finished() {
                let _ = t.join();
            }
        }
    }
}

impl Drop for Hub {
    fn drop(&mut self) {
        self.signal();
    }
}

enum Exit {
    Shutdown,
    Planned,
    Lost,
    Denied,
    Retry,
    Removed,
    Rebind,
}

fn halt_refused(inner: &Inner, link: &str, run: &str, why: &str) {
    inner.emit(LinkMsg::Halted {
        id: view_id(link, run),
        result: Err(why.to_string()),
    });
}

fn pause(inner: &Inner, link: &str, cmds: &Receiver<Cmd>, wait: Option<Duration>) -> bool {
    let until = wait.map(|w| Instant::now() + w);
    loop {
        let got = match until {
            Some(t) => cmds.recv_timeout(t.saturating_duration_since(Instant::now())),
            None => cmds.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match got {
            Err(RecvTimeoutError::Timeout) | Ok(Cmd::Retry) => return true,
            Ok(Cmd::Halt(run, _)) => {
                halt_refused(inner, link, &run, "the machine is not reachable")
            }
            Ok(Cmd::Shutdown) | Err(RecvTimeoutError::Disconnected) => return false,
        }
    }
}

fn clear_rows(inner: &Inner, link: &str) {
    inner.lose_link(link);
    lock(&inner.state).remove_link(link);
}

fn emit_no_runs(inner: &Inner, link: &str) {
    inner.emit(LinkMsg::Runs {
        link: link.to_string(),
        runs: Vec::new(),
    });
}

fn unlink(inner: &Inner, link: &str) {
    clear_rows(inner, link);
    inner.set_state(link, LinkState::Removed);
    emit_no_runs(inner, link);
}

fn control(inner: &Inner, link: &str, cmds: &Receiver<Cmd>) {
    let t = inner.timing;
    let mut backoff = t.backoff_min;
    let mut quiet = false;
    loop {
        if inner.stopped() {
            return;
        }
        if !quiet {
            inner.set_state(link, LinkState::Connecting);
        }
        let bound = inner.dialer.binding(link);
        let conn = match inner.dialer.dial(link) {
            Ok(c) => c,
            Err(e) => {
                quiet = true;
                let keep_going = match e {
                    DialErr::Offline(_) | DialErr::Refused(_) => {
                        let busy = matches!(e, DialErr::Refused(_));
                        inner.set_state(
                            link,
                            if busy {
                                LinkState::Busy
                            } else {
                                LinkState::Offline
                            },
                        );
                        let go = pause(inner, link, cmds, Some(backoff));
                        backoff = (backoff * 2).min(t.backoff_max);
                        go
                    }
                    DialErr::HostKeyChanged | DialErr::NoAccess(_) | DialErr::Unavailable(_) => {
                        inner.lose_link(link);
                        if matches!(e, DialErr::NoAccess(_)) {
                            inner.dialer.forget();
                        }
                        let state = match &e {
                            DialErr::HostKeyChanged => LinkState::HostKeyChanged,
                            _ => LinkState::NoAccess,
                        };
                        inner.set_state(link, state);
                        if let DialErr::Unavailable(why) = e {
                            if !inner.key_notice.swap(true, Ordering::SeqCst) {
                                inner.emit(LinkMsg::Notice {
                                    link: link.to_string(),
                                    id: None,
                                    text: why,
                                });
                            }
                        }
                        let go = pause(inner, link, cmds, None);
                        quiet = false;
                        backoff = t.backoff_min;
                        go
                    }
                    DialErr::Removed => {
                        unlink(inner, link);
                        let go = pause(inner, link, cmds, None);
                        quiet = false;
                        backoff = t.backoff_min;
                        go
                    }
                };
                if !keep_going {
                    return;
                }
                continue;
            }
        };
        lock(&inner.bound).insert(link.to_string(), bound.clone());
        lock(&inner.state).restore_link(link);
        backoff = t.backoff_min;
        match session(inner, link, conn, cmds, &bound) {
            Exit::Shutdown => return,
            Exit::Planned => quiet = true,
            Exit::Rebind => {
                clear_rows(inner, link);
                emit_no_runs(inner, link);
                quiet = true;
            }
            Exit::Retry => quiet = false,
            Exit::Lost => {
                inner.set_state(link, LinkState::Offline);
                quiet = true;
                if !pause(inner, link, cmds, Some(backoff)) {
                    return;
                }
                backoff = (backoff * 2).min(t.backoff_max);
            }
            Exit::Denied => {
                if !pause(inner, link, cmds, None) {
                    return;
                }
                quiet = false;
            }
            Exit::Removed => {
                unlink(inner, link);
                if !pause(inner, link, cmds, None) {
                    return;
                }
                quiet = false;
            }
        }
    }
}

enum Rebound {
    Same,
    Unknown,
    Gone,
    Changed,
}

fn rebound(inner: &Inner, link: &str, bound: &Binding) -> Rebound {
    match inner.dialer.binding(link) {
        Binding::Unreadable => Rebound::Unknown,
        Binding::Gone => Rebound::Gone,
        now if now == *bound => Rebound::Same,
        _ => Rebound::Changed,
    }
}

fn session(
    inner: &Inner,
    link: &str,
    mut conn: Box<dyn LinkConn>,
    cmds: &Receiver<Cmd>,
    bound: &Binding,
) -> Exit {
    let t = inner.timing;
    let started = Instant::now();
    let mut requests = 1u32;
    loop {
        if inner.stopped() {
            return Exit::Shutdown;
        }
        if started.elapsed() >= t.reconnect_after || requests >= t.reconnect_requests {
            return Exit::Planned;
        }
        match rebound(inner, link, bound) {
            Rebound::Same | Rebound::Unknown => {}
            Rebound::Gone => return Exit::Removed,
            Rebound::Changed => {
                inner.lose_link(link);
                return Exit::Rebind;
            }
        }
        let began = Instant::now();
        requests += 1;
        match conn.call("runs.list", Value::Null, t.call) {
            Ok(v) => {
                let latency_ms = began.elapsed().as_millis() as u64;
                let runs = {
                    let mut state = lock(&inner.state);
                    let kept = state.ingest_runs(link, remote::runs_of(&v));
                    let gone = state.vanished(link);
                    drop(state);
                    for id in gone {
                        inner.drop_stream(&id);
                    }
                    kept
                };
                inner.set_state(link, LinkState::Online { latency_ms });
                inner.emit(LinkMsg::Runs {
                    link: link.to_string(),
                    runs,
                });
            }
            Err(e) if e.kind == Kind::Forbidden => {
                inner.lose_link(link);
                inner.dialer.forget();
                inner.set_state(link, LinkState::NoAccess);
                return Exit::Denied;
            }
            Err(_) => return Exit::Lost,
        }
        match cmds.recv_timeout(t.poll) {
            Err(RecvTimeoutError::Timeout) => {}
            Ok(Cmd::Halt(run, stamp)) if stamp.as_ref() != Some(bound) => {
                halt_refused(inner, link, &run, "the link changed, try again");
            }
            Ok(Cmd::Halt(run, _)) => match rebound(inner, link, bound) {
                Rebound::Same => {
                    requests += 1;
                    halt(inner, link, &run, conn.as_mut());
                }
                Rebound::Unknown => {
                    halt_refused(inner, link, &run, "cannot verify the link, try again");
                }
                Rebound::Gone => {
                    halt_refused(inner, link, &run, "the machine is no longer linked");
                    return Exit::Removed;
                }
                Rebound::Changed => {
                    halt_refused(inner, link, &run, "the link changed, try again");
                    inner.lose_link(link);
                    return Exit::Rebind;
                }
            },
            Ok(Cmd::Retry) => return Exit::Retry,
            Ok(Cmd::Shutdown) | Err(RecvTimeoutError::Disconnected) => return Exit::Shutdown,
        }
    }
}

fn halt(inner: &Inner, link: &str, run: &str, conn: &mut dyn LinkConn) {
    let id = view_id(link, run);
    let listed = matches!(
        lock(&inner.state).target_of(&id),
        Target::Remote { link: l, run: r } if l == link && r == run
    );
    let result = if listed {
        conn.call("runs.halt", json!({"run": run}), inner.timing.halt)
            .map(|_| ())
            .map_err(|e| cap(&e.message, MAX_WHY))
    } else {
        Err("that run is not listed on this machine".to_string())
    };
    inner.emit(LinkMsg::Halted { id, result });
}

#[derive(Clone)]
struct StreamSpec {
    id: String,
    link: String,
    run: String,
    cols: u16,
    rows: u16,
    gen: u64,
}

fn open_stream(
    inner: &Inner,
    spec: &StreamSpec,
) -> std::result::Result<(bool, Pair, Kill), String> {
    let held = lock(&inner.bound).get(&spec.link).cloned();
    let unchanged = || {
        held.as_ref()
            .is_some_and(|h| inner.dialer.binding(&spec.link) == *h)
    };
    if !unchanged() {
        return Err("the link changed, try again".to_string());
    }
    let mut conn = inner
        .dialer
        .dial(&spec.link)
        .map_err(|e| e.why(&spec.link))?;
    if !unchanged() {
        return Err("the link changed, try again".to_string());
    }
    let listed = matches!(
        lock(&inner.state).target_of(&spec.id),
        Target::Remote { .. }
    );
    if !listed || !inner.alive(&spec.id, spec.gen) {
        return Err("the run is gone".to_string());
    }
    let v = conn
        .call(
            "attach.open",
            json!({"run": spec.run, "read_only": false, "cols": spec.cols, "rows": spec.rows}),
            inner.timing.open,
        )
        .map_err(|e| cap(&format!("{}: {}", spec.link, e.message), MAX_WHY))?;
    let input = v.get("input").and_then(Value::as_bool).unwrap_or(false);
    let (pair, kill) = conn
        .into_stream()
        .map_err(|e| cap(&format!("{}: {}", spec.link, e.message), MAX_WHY))?;
    Ok((input, pair, kill))
}

fn close_stream(inner: &Inner, spec: &StreamSpec, why: String, reopen: bool) {
    if inner.end_stream(&spec.id, spec.gen) {
        if !reopen {
            lock(&inner.cooldown).insert(spec.id.clone(), Instant::now() + inner.timing.cooldown);
        }
        inner.emit(LinkMsg::StreamClosed {
            id: spec.id.clone(),
            why,
            reopen,
        });
    }
}

fn reopens(why: &str) -> bool {
    why == "idle" || why.starts_with("detached")
}

const NOTICE_GAP: Duration = Duration::from_millis(100);

fn stream_main(inner: &Arc<Inner>, spec: &StreamSpec) {
    let (input, (mut reader, writer), kill) = match open_stream(inner, spec) {
        Ok(v) => v,
        Err(why) => return close_stream(inner, spec, why, false),
    };
    let (out_tx, out_rx) = mpsc::sync_channel::<Outgoing>(OUT_QUEUE);
    {
        let mut streams = lock(&inner.streams);
        match streams.get_mut(&spec.id) {
            Some(s) if s.gen == spec.gen && !inner.stopped() => {
                s.out = Some(out_tx.clone());
                s.input = Some(input);
                s.kill = Some(kill);
            }
            _ => {
                drop(streams);
                inner.retire(Some(kill));
                let mut writer = writer;
                let _ = write_outgoing(&mut writer, &Outgoing::Detach);
                return;
            }
        }
    }
    let still_listed = matches!(
        lock(&inner.state).target_of(&spec.id),
        Target::Remote { .. }
    );
    if !still_listed {
        return close_stream(inner, spec, "the run is gone".to_string(), false);
    }
    inner.emit(LinkMsg::Opened {
        id: spec.id.clone(),
        input,
    });
    let born = Instant::now();
    let heard = Arc::new(Mutex::new(Instant::now()));
    let renewed = Arc::new(AtomicBool::new(false));
    let flow = Arc::new(Flow::new());
    {
        let (inner, spec, heard, renewed) = (
            Arc::clone(inner),
            spec.clone(),
            Arc::clone(&heard),
            Arc::clone(&renewed),
        );
        thread::spawn(move || writer_main(&inner, &spec, writer, &out_rx, &heard, &renewed));
    }
    {
        let (inner, spec, flow) = (Arc::clone(inner), spec.clone(), Arc::clone(&flow));
        thread::spawn(move || emitter_main(&inner, &spec, &flow));
    }
    loop {
        let got = read_incoming(&mut reader, &spec.run);
        if !inner.alive(&spec.id, spec.gen) {
            return;
        }
        *lock(&heard) = Instant::now();
        let near_hour = born.elapsed() >= inner.timing.stream_renew;
        match got {
            Ok(Some(Incoming::Pong)) => {}
            Ok(Some(Incoming::Notice(text))) => {
                if let Some(text) = flow.notice(text) {
                    inner.emit(LinkMsg::Notice {
                        link: spec.link.clone(),
                        id: Some(spec.id.clone()),
                        text,
                    });
                }
            }
            Ok(Some(Incoming::Closed(why))) => {
                let reopen = reopens(&why) && (near_hour || renewed.load(Ordering::SeqCst));
                return close_stream(inner, spec, why, reopen);
            }
            Ok(Some(msg)) => {
                let rekeyed = lock(&inner.state).ingest(&spec.link, msg);
                match rekeyed {
                    Some(Incoming::Screen(push)) => flow.put_screen(*push),
                    Some(Incoming::Event(ev)) => flow.put_event(ev),
                    _ => {}
                }
            }
            Ok(None) => {
                return close_stream(
                    inner,
                    spec,
                    format!("{} closed the connection", spec.link),
                    near_hour,
                )
            }
            Err(e) => return close_stream(inner, spec, e, false),
        }
    }
}

fn emitter_main(inner: &Inner, spec: &StreamSpec, flow: &Flow) {
    let mut last: Option<Instant> = None;
    loop {
        if let Some(text) = flow.take_due_notice() {
            inner.emit(LinkMsg::Notice {
                link: spec.link.clone(),
                id: Some(spec.id.clone()),
                text,
            });
        }
        {
            let mut slot = lock(&flow.slot);
            loop {
                if !inner.alive(&spec.id, spec.gen) {
                    return;
                }
                let pending = slot.screen.is_some() || slot.event.is_some();
                if (pending && !flow.busy.load(Ordering::SeqCst)) || flow.notice_due() {
                    break;
                }
                slot = flow
                    .wake
                    .wait_timeout(slot, Duration::from_millis(10))
                    .unwrap_or_else(|p| p.into_inner())
                    .0;
            }
        }
        if let Some(at) = last {
            let left = inner.timing.screen_gap.saturating_sub(at.elapsed());
            if !left.is_zero() {
                thread::sleep(left);
            }
        }
        let msg = {
            let mut slot = lock(&flow.slot);
            if let Some(ev) = slot.event.take() {
                LinkMsg::Event(ev, Permit::held(&flow.busy))
            } else if let Some(push) = slot.screen.take() {
                LinkMsg::Screen(Box::new(push), Permit::held(&flow.busy))
            } else {
                continue;
            }
        };
        match inner.tx.try_send(msg) {
            Ok(()) => last = Some(Instant::now()),
            Err(TrySendError::Full(back)) => {
                flow.put_back(back);
                thread::sleep(Duration::from_millis(10));
            }
            Err(TrySendError::Disconnected(_)) => return,
        }
    }
}

fn writer_main(
    inner: &Inner,
    spec: &StreamSpec,
    mut w: Box<dyn Write + Send>,
    rx: &Receiver<Outgoing>,
    heard: &Mutex<Instant>,
    renewed: &AtomicBool,
) {
    let t = inner.timing;
    let born = Instant::now();
    loop {
        if born.elapsed() >= t.stream_renew {
            renewed.store(true, Ordering::SeqCst);
            let _ = write_outgoing(&mut w, &Outgoing::Detach);
            return;
        }
        if lock(heard).elapsed() > t.silence {
            let why = format!("{}: no answer", spec.link);
            return close_stream(inner, spec, why, false);
        }
        match rx.recv_timeout(t.heartbeat) {
            Ok(m) => {
                let last = m == Outgoing::Detach;
                if write_outgoing(&mut w, &m).is_err() || last {
                    return;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if write_outgoing(&mut w, &Outgoing::Ping).is_err() {
                    return;
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                let _ = write_outgoing(&mut w, &Outgoing::Detach);
                return;
            }
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use std::collections::VecDeque;
    use tmgr_ipc::api::{Attr, Color, Line, Span, State, StateKind};
    use tmgr_link::attach::{encode_closed, encode_event, encode_notice, encode_screen};
    use tmgr_link::proto::{read_frame, write_frame, MAX_FRAME};

    pub(crate) const A: &str = "s_aaaaaaaaaa";
    pub(crate) const B: &str = "s_bbbbbbbbbb";
    pub(crate) const C: &str = "s_cccccccccc";
    pub(crate) const LOCAL: &str = "s_localsess2";

    pub(crate) fn state(kind: StateKind) -> State {
        State {
            kind,
            since_ms: 0,
            detail: None,
            exit_code: None,
        }
    }

    pub(crate) fn run_json(id: &str) -> Value {
        json!({"id": id, "name": format!("run {id}"), "agent": "codex", "state": state(StateKind::Working)})
    }

    pub(crate) fn run_view(id: &str) -> RunView {
        serde_json::from_value(run_json(id)).unwrap()
    }

    pub(crate) fn push(session: &str) -> ScreenPush {
        ScreenPush {
            session: session.into(),
            full: false,
            snapshot: None,
            lines: vec![(
                0,
                Line {
                    spans: vec![Span {
                        a: Attr {
                            fg: Color::Default,
                            bg: Color::Default,
                            flags: 0,
                        },
                        t: "hi".into(),
                    }],
                },
            )],
            cursor: None,
            modes: None,
            scroll_offset: 0,
            scroll_max: 0,
        }
    }

    pub(crate) fn framed(bytes: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        write_frame(&mut v, bytes, MAX_FRAME).unwrap();
        v
    }

    #[derive(Default)]
    pub(crate) struct FakeLink {
        pub(crate) dial: VecDeque<std::result::Result<(), DialErr>>,
        pub(crate) dials: Vec<Instant>,
        pub(crate) runs: Vec<Value>,
        pub(crate) list_err: Option<Kind>,
        pub(crate) hang: bool,
        pub(crate) dial_delay: Duration,
        pub(crate) bind: u32,
        pub(crate) unbound: bool,
        pub(crate) block_writes: bool,
        pub(crate) killed: Arc<AtomicBool>,
        pub(crate) halt_err: Option<Kind>,
        pub(crate) input: bool,
        pub(crate) calls: Vec<(String, Value)>,
        pub(crate) feeds: HashMap<String, Sender<Vec<u8>>>,
        pub(crate) written: HashMap<String, Arc<Mutex<Vec<u8>>>>,
    }

    #[derive(Default)]
    pub(crate) struct Fake(
        pub(crate) Mutex<HashMap<String, FakeLink>>,
        pub(crate) AtomicBool,
        pub(crate) std::sync::atomic::AtomicUsize,
    );

    impl Fake {
        pub(crate) fn with<R>(&self, link: &str, f: impl FnOnce(&mut FakeLink) -> R) -> R {
            f(lock(&self.0).entry(link.to_string()).or_default())
        }

        pub(crate) fn set_runs(&self, link: &str, ids: &[&str]) {
            self.with(link, |l| l.runs = ids.iter().map(|i| run_json(i)).collect());
        }

        pub(crate) fn calls(&self, link: &str, method: &str) -> Vec<Value> {
            self.with(link, |l| {
                l.calls
                    .iter()
                    .filter(|(m, _)| m == method)
                    .map(|(_, p)| p.clone())
                    .collect()
            })
        }

        pub(crate) fn feed(&self, link: &str, run: &str, frame: &[u8]) {
            let tx = self.with(link, |l| l.feeds.get(run).cloned()).unwrap();
            tx.send(framed(frame)).unwrap();
        }

        pub(crate) fn hang_up(&self, link: &str, run: &str) {
            self.with(link, |l| l.feeds.remove(run));
        }

        pub(crate) fn written(&self, link: &str, run: &str) -> Vec<Outgoing> {
            let buf = self
                .with(link, |l| l.written.get(run).cloned())
                .unwrap()
                .lock()
                .unwrap()
                .clone();
            let mut r = &buf[..];
            let mut out = Vec::new();
            while let Some(frame) = read_frame(&mut r, MAX_FRAME).unwrap() {
                out.push(tmgr_link::attach::parse_outgoing(&frame).unwrap());
            }
            out
        }
    }

    pub(crate) struct FakeDialer(pub(crate) Arc<Fake>);

    impl Dialer for FakeDialer {
        fn shutdown(&self) {
            self.0 .1.store(true, Ordering::SeqCst);
        }

        fn forget(&self) {
            self.0 .2.fetch_add(1, Ordering::SeqCst);
        }

        fn binding(&self, link: &str) -> Binding {
            self.0.with(link, |l| {
                if l.unbound {
                    Binding::Gone
                } else {
                    Binding::Is(l.bind.to_string())
                }
            })
        }

        fn dial(&self, link: &str) -> std::result::Result<Box<dyn LinkConn>, DialErr> {
            let delay = self.0.with(link, |l| l.dial_delay);
            thread::sleep(delay);
            self.0.with(link, |l| {
                l.dials.push(Instant::now());
                l.dial.pop_front().unwrap_or(Ok(()))
            })?;
            Ok(Box::new(FakeConn {
                fake: Arc::clone(&self.0),
                link: link.to_string(),
                pending: None,
            }))
        }
    }

    type Wire = (Receiver<Vec<u8>>, Arc<Mutex<Vec<u8>>>);

    struct FakeConn {
        fake: Arc<Fake>,
        link: String,
        pending: Option<Wire>,
    }

    struct ChanRead {
        rx: Receiver<Vec<u8>>,
        buf: Vec<u8>,
    }

    impl Read for ChanRead {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            if self.buf.is_empty() {
                match self.rx.recv() {
                    Ok(b) => self.buf = b,
                    Err(_) => return Ok(0),
                }
            }
            let n = out.len().min(self.buf.len());
            out[..n].copy_from_slice(&self.buf[..n]);
            self.buf.drain(..n);
            Ok(n)
        }
    }

    struct Sink(Arc<Mutex<Vec<u8>>>);

    struct Stuck(Arc<AtomicBool>);

    impl Write for Stuck {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            while !self.0.load(Ordering::SeqCst) {
                thread::sleep(Duration::from_millis(5));
            }
            Err(std::io::ErrorKind::BrokenPipe.into())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    impl Write for Sink {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(b);
            Ok(b.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    enum Plan {
        Hang,
        Fail(Kind),
        Ok(Value),
    }

    impl LinkConn for FakeConn {
        fn call(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
            let pending = &mut self.pending;
            let plan = self.fake.with(&self.link, |l| {
                l.calls.push((method.to_string(), params.clone()));
                match method {
                    "runs.list" if l.hang => Plan::Hang,
                    "runs.list" => match l.list_err {
                        Some(k) => Plan::Fail(k),
                        None => Plan::Ok(json!({"runs": l.runs})),
                    },
                    "runs.halt" => match l.halt_err {
                        Some(k) => Plan::Fail(k),
                        None => Plan::Ok(json!({})),
                    },
                    "attach.open" => {
                        let run = params["run"].as_str().unwrap().to_string();
                        let (tx, rx) = mpsc::channel();
                        let sink = Arc::new(Mutex::new(Vec::new()));
                        l.feeds.insert(run.clone(), tx);
                        l.written.insert(run.clone(), Arc::clone(&sink));
                        *pending = Some((rx, sink));
                        Plan::Ok(json!({"run": run, "input": l.input}))
                    }
                    _ => Plan::Fail(Kind::Usage),
                }
            });
            match plan {
                Plan::Hang => {
                    thread::sleep(timeout);
                    Err(Error::new(
                        Kind::Link,
                        "the follower did not answer in time",
                    ))
                }
                Plan::Fail(k) => Err(Error::new(k, "scripted failure")),
                Plan::Ok(v) => Ok(v),
            }
        }

        fn into_stream(self: Box<Self>) -> Result<(Pair, Kill)> {
            let (rx, sink) = self.pending.ok_or_else(|| Error::internal("no stream"))?;
            let (stuck, killed) = self
                .fake
                .with(&self.link, |l| (l.block_writes, Arc::clone(&l.killed)));
            let writer: Box<dyn Write + Send> = if stuck {
                Box::new(Stuck(Arc::clone(&killed)))
            } else {
                Box::new(Sink(sink))
            };
            Ok((
                (
                    Box::new(ChanRead {
                        rx,
                        buf: Vec::new(),
                    }),
                    writer,
                ),
                Box::new(move || killed.store(true, Ordering::SeqCst)),
            ))
        }
    }

    pub(crate) fn fast() -> Timing {
        Timing {
            poll: Duration::from_millis(20),
            call: Duration::from_millis(100),
            open: Duration::from_millis(500),
            halt: Duration::from_millis(500),
            backoff_min: Duration::from_millis(30),
            backoff_max: Duration::from_millis(120),
            reconnect_after: Duration::from_secs(3600),
            reconnect_requests: 100_000,
            heartbeat: Duration::from_secs(3600),
            silence: Duration::from_secs(3600),
            stream_renew: Duration::from_secs(3600),
            cooldown: Duration::from_millis(200),
            shutdown_wait: Duration::from_millis(500),
            kill_grace: Duration::from_millis(50),
            screen_gap: Duration::from_millis(33),
        }
    }

    struct Rig {
        fake: Arc<Fake>,
        hub: Hub,
        rx: Receiver<LinkMsg>,
        seen: Vec<String>,
    }

    fn rig(links: &[&str], timing: Timing, setup: impl FnOnce(&Fake)) -> Rig {
        let fake = Arc::new(Fake::default());
        setup(&fake);
        let (tx, rx) = mpsc::sync_channel(512);
        let hub = Hub::start_with(
            Arc::new(FakeDialer(Arc::clone(&fake))),
            links.iter().map(|l| l.to_string()).collect(),
            tx,
            timing,
        );
        Rig {
            fake,
            hub,
            rx,
            seen: Vec::new(),
        }
    }

    fn is_state(m: &LinkMsg, want: &LinkState) -> bool {
        matches!(m, LinkMsg::State { state, .. } if state == want)
    }

    impl Rig {
        fn until<T>(&mut self, mut pick: impl FnMut(&LinkMsg) -> Option<T>) -> T {
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                let msg = self
                    .rx
                    .recv_timeout(left)
                    .unwrap_or_else(|_| panic!("timed out; saw {:#?}", self.seen));
                self.seen.push(format!("{msg:?}"));
                if let Some(v) = pick(&msg) {
                    return v;
                }
            }
        }

        fn state_is(&mut self, link: &str, want: LinkState) {
            self.until(|m| {
                matches!(m, LinkMsg::State { link: l, state } if l == link && *state == want)
                    .then_some(())
            });
        }

        fn online(&mut self, link: &str) {
            self.until(|m| {
                matches!(m, LinkMsg::State { link: l, state: LinkState::Online { .. } } if l == link)
                    .then_some(())
            });
        }

        fn runs(&mut self, link: &str, n: usize) {
            self.until(|m| {
                matches!(m, LinkMsg::Runs { link: l, runs } if l == link && runs.len() == n)
                    .then_some(())
            });
        }

        fn open(&mut self, link: &str, run: &str) -> bool {
            let id = view_id(link, run);
            self.hub.open(&id, link, run, 80, 24).unwrap();
            self.until(|m| match m {
                LinkMsg::Opened { id: i, input } if *i == id => Some(*input),
                _ => None,
            })
        }

        fn drain(&mut self, wait: Duration) -> Vec<LinkMsg> {
            thread::sleep(wait);
            let mut v = Vec::new();
            while let Ok(m) = self.rx.try_recv() {
                v.push(m);
            }
            v
        }
    }

    #[test]
    fn rows_are_namespaced_per_link_and_bad_ids_and_overflow_are_dropped() {
        let mut st = RemoteState::new(&["spark".into(), "lab".into()]);
        let mut many: Vec<RunView> = vec![
            run_view(A),
            run_view(A),
            run_view("bad"),
            run_view("s_short"),
        ];
        let alphabet = b"abcdefghijklmnopqrstuvwxyz234567";
        for i in 0..250usize {
            let id = format!("s_{}{}", alphabet[i / 32] as char, "bbbbbbb")
                + &format!("{}{}", alphabet[i % 32] as char, "b");
            many.push(run_view(&id));
        }
        let kept = st.ingest_runs("spark", many);
        assert_eq!(kept.len(), 200);
        assert_eq!(kept.iter().filter(|r| r.id == A).count(), 1);
        st.ingest_runs("lab", vec![run_view(A), run_view(B)]);
        let rows = st.rows("lab");
        assert_eq!(
            rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>(),
            vec![format!("lab/{A}"), format!("lab/{B}")]
        );
        assert!(rows
            .iter()
            .all(|r| r.esc && r.you.is_none() && r.resumed_from.is_none()));
        assert!(st.rows("spark")[0].id.starts_with("spark/s_"));
        assert!(st.rows("nowhere").is_empty());
        assert!(st.ingest_runs("nowhere", vec![run_view(A)]).is_empty());
    }

    #[test]
    fn target_of_resolves_only_listed_remote_runs_and_valid_local_ids() {
        let mut st = RemoteState::new(&["spark".into(), "lab".into()]);
        st.ingest_runs("spark", vec![run_view(A)]);
        assert_eq!(st.target_of(LOCAL), Target::Local(LOCAL));
        assert_eq!(
            st.target_of(&format!("spark/{A}")),
            Target::Remote {
                link: "spark".into(),
                run: A.into()
            }
        );
        for bad in [
            format!("lab/{A}"),
            format!("spark/{B}"),
            format!("ghost/{A}"),
            format!("spark/../{A}"),
            format!("spark/{A}/{A}"),
            format!("spark//{A}"),
            format!("/{A}"),
            format!("spark/{A} "),
            "spark/".to_string(),
            "spark".to_string(),
            String::new(),
            format!("{A}/{A}"),
        ] {
            assert_eq!(st.target_of(&bad), Target::Unknown, "{bad:?}");
        }
        assert_eq!(
            split_local(&[LOCAL.into(), format!("spark/{A}"), "x".into()]),
            vec![LOCAL]
        );
    }

    #[test]
    fn ingest_rekeys_listed_runs_and_drops_everything_else() {
        let mut st = RemoteState::new(&["spark".into(), "lab".into()]);
        st.ingest_runs("spark", vec![run_view(A)]);
        st.ingest_runs("lab", vec![run_view(B)]);
        let screen = |s: &str| Incoming::Screen(Box::new(push(s)));
        let Some(Incoming::Screen(p)) = st.ingest("spark", screen(A)) else {
            panic!("listed run must pass");
        };
        assert_eq!(p.session, format!("spark/{A}"));
        assert!(st.ingest("spark", screen(B)).is_none());
        assert!(st.ingest("lab", screen(A)).is_none());
        assert!(st.ingest("lab", screen(&format!("spark/{A}"))).is_none());
        assert!(st.ingest("ghost", screen(A)).is_none());
        let ev = EventMsg {
            session: A.into(),
            state: state(StateKind::Waiting),
            task: None,
            origin: None,
        };
        let Some(Incoming::Event(e)) = st.ingest("spark", Incoming::Event(ev.clone())) else {
            panic!("listed run must pass");
        };
        assert_eq!(e.session, format!("spark/{A}"));
        assert_eq!(st.rows("spark")[0].state.kind, StateKind::Waiting);
        assert!(st.ingest("lab", Incoming::Event(ev)).is_none());
        st.remove_link("spark");
        assert!(st.ingest("spark", screen(A)).is_none());
        assert!(st.rows("spark").is_empty());
        assert_eq!(st.target_of(&format!("spark/{A}")), Target::Unknown);
    }

    #[test]
    fn stream_budget_is_two_per_link_and_closes_what_is_not_visible() {
        let mut st = RemoteState::new(&["spark".into(), "lab".into()]);
        st.ingest_runs("spark", vec![run_view(A), run_view(B), run_view(C)]);
        st.ingest_runs("lab", vec![run_view(A)]);
        let vis: Vec<String> = vec![
            format!("spark/{A}"),
            format!("spark/{B}"),
            format!("spark/{C}"),
            format!("lab/{A}"),
            LOCAL.to_string(),
            format!("spark/{A}"),
            "ghost/x".to_string(),
        ];
        let w = st.want_streams(&vis);
        assert_eq!(w.open.len(), 3);
        assert_eq!(w.over_budget, vec![format!("spark/{C}")]);
        assert!(w.close.is_empty());
        st.mark_open(&format!("spark/{A}"));
        st.mark_open(&format!("spark/{B}"));
        let w = st.want_streams(&[format!("spark/{B}"), format!("spark/{C}")]);
        assert_eq!(w.close, vec![format!("spark/{A}")]);
        assert_eq!(w.open.len(), 1);
        assert!(w.over_budget.is_empty());
    }

    #[test]
    fn dial_errors_are_classified_from_representative_texts() {
        let e = |k, m: &str| classify(&Error::new(k, m));
        assert_eq!(
            e(Kind::NotFound, "no follower named spark"),
            DialErr::Removed
        );
        assert_eq!(
            e(
                Kind::Link,
                "host key mismatch: the follower's key is not the pinned one"
            ),
            DialErr::HostKeyChanged
        );
        assert!(matches!(
            e(
                Kind::Link,
                "the follower refused this link key (not trusted there, or from= does not match)"
            ),
            DialErr::NoAccess(_)
        ));
        assert!(matches!(
            e(
                Kind::Link,
                "the follower refused the handshake: forbidden: revoked"
            ),
            DialErr::NoAccess(_)
        ));
        assert!(matches!(
            e(Kind::Forbidden, "refused"),
            DialErr::NoAccess(_)
        ));
        assert!(matches!(
            e(Kind::Link, "ssh failed: tmgr worker: refused"),
            DialErr::Refused(_)
        ));
        assert!(matches!(
            e(
                Kind::Link,
                "the follower refused the handshake: busy: slots"
            ),
            DialErr::Refused(_)
        ));
        for text in [
            "the follower did not answer in time",
            "ssh failed: ssh: connect to host x port 22: Connection refused",
            "the follower closed the connection",
            "could not start ssh",
        ] {
            assert!(matches!(e(Kind::Link, text), DialErr::Offline(_)), "{text}");
        }
        assert!(matches!(
            e(Kind::NotFound, "this machine has no link identity"),
            DialErr::Offline(_)
        ));
    }

    #[test]
    fn followers_come_from_the_store_sorted_and_validated() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let paths = Paths::under(d.path());
        assert!(followers(&paths).is_empty());
        Store::update(&paths, |s| {
            for name in ["spark", "lab", "Bad_Name"] {
                s.followers.insert(
                    name.into(),
                    tmgr_link::store::Follower {
                        user: "u".into(),
                        host: "example.com".into(),
                        port: 22,
                        host_key_fingerprint: "SHA256:x".into(),
                        added: "2026-01-01".into(),
                    },
                );
            }
            Ok(())
        })
        .unwrap();
        assert_eq!(
            followers(&paths),
            vec!["lab".to_string(), "spark".to_string()]
        );
        Store::update(&paths, |s| {
            let hop = s.followers["spark"].clone();
            s.followers.insert("hop".into(), hop);
            s.jumps.insert("lab".into(), "hop".into());
            Ok(())
        })
        .unwrap();
        assert_eq!(
            followers(&paths),
            vec!["lab".to_string(), "spark".to_string()]
        );
    }

    #[test]
    fn two_links_list_their_runs_and_halt_reaches_only_the_right_machine() {
        let mut r = rig(&["spark", "lab"], fast(), |f| {
            f.set_runs("spark", &[A, B]);
            f.set_runs("lab", &[A]);
        });
        r.runs("spark", 2);
        r.runs("lab", 1);
        assert_eq!(r.hub.state().rows("spark")[0].id, format!("spark/{A}"));
        assert_eq!(r.hub.state().rows("lab")[0].id, format!("lab/{A}"));
        r.hub.halt("spark", A).unwrap();
        let id = r.until(|m| match m {
            LinkMsg::Halted { id, result } => {
                assert!(result.is_ok());
                Some(id.clone())
            }
            _ => None,
        });
        assert_eq!(id, format!("spark/{A}"));
        assert_eq!(r.fake.calls("spark", "runs.halt"), vec![json!({"run": A})]);
        assert!(r.fake.calls("lab", "runs.halt").is_empty());
        assert!(r.hub.halt("spark", C).is_err());
        assert!(r.hub.halt("ghost", A).is_err());
        assert!(r.hub.halt("lab", B).is_err());
        r.hub.shutdown();
    }

    #[test]
    fn forbidden_on_halt_is_reported_and_the_link_stays_up() {
        let mut r = rig(&["spark"], fast(), |f| {
            f.set_runs("spark", &[A]);
            f.with("spark", |l| l.halt_err = Some(Kind::Forbidden));
        });
        r.runs("spark", 1);
        r.hub.halt("spark", A).unwrap();
        let result = r.until(|m| match m {
            LinkMsg::Halted { result, .. } => Some(result.clone()),
            _ => None,
        });
        assert!(result.is_err());
        r.runs("spark", 1);
    }

    #[test]
    fn a_follower_cannot_pass_off_a_local_id_or_another_links_data() {
        let mut r = rig(&["spark", "lab"], fast(), |f| {
            f.set_runs("spark", &[LOCAL]);
            f.set_runs("lab", &[A]);
            f.with("spark", |l| l.input = true);
            f.with("lab", |l| l.input = true);
        });
        r.runs("spark", 1);
        r.runs("lab", 1);
        assert!(r.open("spark", LOCAL));
        r.fake.feed("spark", LOCAL, &encode_screen(&push(LOCAL)));
        let session = r.until(|m| match m {
            LinkMsg::Screen(p, _) => Some(p.session.clone()),
            _ => None,
        });
        assert_eq!(session, format!("spark/{LOCAL}"));
        assert_ne!(session, LOCAL);
        r.fake.feed("spark", LOCAL, &encode_screen(&push(A)));
        r.fake
            .feed("spark", LOCAL, &encode_screen(&push(&format!("lab/{A}"))));
        r.fake.feed(
            "spark",
            LOCAL,
            &encode_event(&EventMsg {
                session: format!("lab/{A}"),
                state: state(StateKind::Exited),
                task: None,
                origin: None,
            }),
        );
        r.fake
            .feed("spark", LOCAL, &encode_notice("hello \u{1b}[2J"));
        r.until(|m| match m {
            LinkMsg::Notice { link, id, text } => {
                assert_eq!(link, "spark");
                assert_eq!(id.as_deref(), Some(format!("spark/{LOCAL}").as_str()));
                assert!(!text.contains('\u{1b}'));
                Some(())
            }
            LinkMsg::Screen(..) | LinkMsg::Event(..) => panic!("forged data got through: {m:?}"),
            _ => None,
        });
    }

    #[test]
    fn a_run_that_leaves_the_list_stops_producing_screens_and_closes_its_stream() {
        let mut r = rig(&["spark"], fast(), |f| {
            f.set_runs("spark", &[A]);
            f.with("spark", |l| l.input = true);
        });
        r.runs("spark", 1);
        r.open("spark", A);
        r.fake.set_runs("spark", &[B]);
        r.runs("spark", 1);
        assert_eq!(
            r.hub.input(&format!("spark/{A}"), b"x"),
            Err("not attached")
        );
        let tx = r.fake.with("spark", |l| l.feeds.get(A).cloned());
        if let Some(tx) = tx {
            let _ = tx.send(framed(&encode_screen(&push(A))));
        }
        let after = r.drain(Duration::from_millis(100));
        assert!(!after.iter().any(|m| matches!(m, LinkMsg::Screen(..))));
    }

    #[test]
    fn link_down_backs_off_then_recovers_with_online_and_runs() {
        let mut r = rig(&["spark"], fast(), |f| {
            f.set_runs("spark", &[A]);
            f.with("spark", |l| {
                l.dial = VecDeque::from([
                    Err(DialErr::Offline("down".into())),
                    Err(DialErr::Offline("down".into())),
                    Err(DialErr::Offline("down".into())),
                ])
            });
        });
        r.state_is("spark", LinkState::Offline);
        r.online("spark");
        r.runs("spark", 1);
        let dials = r.fake.with("spark", |l| l.dials.clone());
        let gap = |i: usize| dials[i + 1].duration_since(dials[i]);
        assert!(gap(0) >= Duration::from_millis(30), "{:?}", gap(0));
        assert!(gap(1) >= Duration::from_millis(60), "{:?}", gap(1));
        assert!(gap(2) >= Duration::from_millis(100), "{:?}", gap(2));
        let connecting = r.seen.iter().filter(|s| s.contains("Connecting")).count();
        assert_eq!(connecting, 1, "{:#?}", r.seen);
    }

    #[test]
    fn a_planned_reconnect_never_shows_offline() {
        let t = Timing {
            reconnect_after: Duration::from_millis(80),
            ..fast()
        };
        let mut r = rig(&["spark"], t, |f| f.set_runs("spark", &[A]));
        r.online("spark");
        let msgs = r.drain(Duration::from_millis(500));
        assert!(r.fake.with("spark", |l| l.dials.len()) >= 3);
        assert!(!msgs
            .iter()
            .any(|m| is_state(m, &LinkState::Offline) || is_state(m, &LinkState::Connecting)));
        assert_eq!(r.hub.state().rows("spark").len(), 1);
    }

    #[test]
    fn an_unlinked_machine_loses_its_rows_and_ids() {
        let mut r = rig(&["spark"], fast(), |f| f.set_runs("spark", &[A]));
        r.runs("spark", 1);
        r.fake.with("spark", |l| {
            l.dial.push_back(Err(DialErr::Removed));
            l.list_err = Some(Kind::Link);
        });
        r.state_is("spark", LinkState::Offline);
        r.state_is("spark", LinkState::Removed);
        r.runs("spark", 0);
        assert!(r.hub.state().rows("spark").is_empty());
        assert_eq!(
            r.hub.state().target_of(&format!("spark/{A}")),
            Target::Unknown
        );
        assert!(r
            .hub
            .open(&format!("spark/{A}"), "spark", A, 80, 24)
            .is_err());
    }

    #[test]
    fn forbidden_on_list_means_no_access_and_polling_stops_until_retry() {
        let mut r = rig(&["spark"], fast(), |f| {
            f.set_runs("spark", &[A]);
            f.with("spark", |l| l.list_err = Some(Kind::Forbidden));
        });
        r.state_is("spark", LinkState::NoAccess);
        thread::sleep(Duration::from_millis(150));
        let polls = r.fake.calls("spark", "runs.list").len();
        let dials = r.fake.with("spark", |l| l.dials.len());
        thread::sleep(Duration::from_millis(200));
        assert_eq!(r.fake.calls("spark", "runs.list").len(), polls);
        assert_eq!(r.fake.with("spark", |l| l.dials.len()), dials);
        r.fake.with("spark", |l| l.list_err = None);
        r.hub.retry_all();
        r.online("spark");
    }

    #[test]
    fn dial_refusals_map_to_states_and_hostkey_stops_until_retry() {
        let mut r = rig(&["spark", "lab"], fast(), |f| {
            f.with("spark", |l| {
                l.dial = VecDeque::from([Err(DialErr::Refused("slots".into()))])
            });
            f.with("lab", |l| {
                l.dial = VecDeque::from([Err(DialErr::HostKeyChanged)])
            });
        });
        let (mut busy, mut hostkey, mut online) = (false, false, false);
        r.until(|m| {
            match m {
                LinkMsg::State { link, state } if link == "spark" => match state {
                    LinkState::Busy => busy = true,
                    LinkState::Online { .. } => online = true,
                    _ => {}
                },
                LinkMsg::State { link, state } if link == "lab" => {
                    hostkey |= *state == LinkState::HostKeyChanged;
                }
                _ => {}
            }
            (busy && hostkey && online).then_some(())
        });
        thread::sleep(Duration::from_millis(150));
        assert_eq!(r.fake.with("lab", |l| l.dials.len()), 1);
        r.hub.retry_all();
        r.online("lab");
    }

    #[test]
    fn a_connection_that_never_answers_takes_the_link_offline_within_the_deadline() {
        let mut r = rig(&["spark"], fast(), |f| {
            f.set_runs("spark", &[A]);
            f.with("spark", |l| l.hang = true);
        });
        let began = Instant::now();
        r.state_is("spark", LinkState::Offline);
        assert!(began.elapsed() < Duration::from_secs(2));
        r.fake.with("spark", |l| l.hang = false);
        r.online("spark");
    }

    #[test]
    fn a_read_only_stream_never_gets_input_or_resize() {
        let mut r = rig(&["spark"], fast(), |f| f.set_runs("spark", &[A]));
        r.runs("spark", 1);
        assert!(!r.open("spark", A));
        let id = format!("spark/{A}");
        assert!(r.hub.input(&id, b"rm -rf\n").is_err());
        r.hub.resize(&id, 100, 30);
        r.hub.scroll(&id, 3);
        thread::sleep(Duration::from_millis(100));
        r.hub.close(&id);
        thread::sleep(Duration::from_millis(100));
        let written = r.fake.written("spark", A);
        assert_eq!(written, vec![Outgoing::Scroll(3), Outgoing::Detach]);
        assert_eq!(r.fake.calls("spark", "attach.open")[0]["read_only"], false);
    }

    #[test]
    fn an_interactive_stream_takes_text_input_and_resize_only_for_its_own_machine() {
        let mut r = rig(&["spark", "lab"], fast(), |f| {
            f.set_runs("spark", &[A]);
            f.set_runs("lab", &[A]);
            f.with("spark", |l| l.input = true);
            f.with("lab", |l| l.input = true);
        });
        r.runs("spark", 1);
        r.runs("lab", 1);
        assert!(r.open("spark", A));
        assert!(r.open("lab", A));
        let id = format!("spark/{A}");
        r.hub.input(&id, "ls é\n".as_bytes()).unwrap();
        r.hub.resize(&id, 100, 30);
        thread::sleep(Duration::from_millis(100));
        r.hub.close(&id);
        thread::sleep(Duration::from_millis(100));
        assert_eq!(
            r.fake.written("spark", A),
            vec![
                Outgoing::Input("ls é\n".into()),
                Outgoing::Resize {
                    cols: 100,
                    rows: 30
                },
                Outgoing::Detach
            ]
        );
        assert!(r.fake.written("lab", A).is_empty());
        assert!(r.hub.input(&id, b"x").is_err());
        assert!(r.hub.input(&format!("lab/{B}"), b"x").is_err());
        assert!(r.hub.input(LOCAL, b"x").is_err());
    }

    #[test]
    fn at_most_two_streams_per_link_are_opened() {
        let mut r = rig(&["spark", "lab"], fast(), |f| {
            f.set_runs("spark", &[A, B, C]);
            f.set_runs("lab", &[A]);
        });
        r.runs("spark", 3);
        r.runs("lab", 1);
        r.open("spark", A);
        r.open("spark", B);
        let c = format!("spark/{C}");
        assert_eq!(
            r.hub.open(&c, "spark", C, 80, 24),
            Err("too many remote panes on this machine")
        );
        r.open("lab", A);
        assert!(r.hub.open(&c, "lab", C, 80, 24).is_err());
        r.hub.close(&format!("spark/{A}"));
        r.hub.open(&c, "spark", C, 80, 24).unwrap();
    }

    #[test]
    fn stream_close_reasons_decide_whether_the_pane_reopens() {
        for (reason, reopen) in [
            ("idle", false),
            ("detached", false),
            ("this link is no longer trusted", false),
            ("the run is no longer yours", false),
            ("bad screen: more lines than rows", false),
            ("runner: gone", false),
        ] {
            let mut r = rig(&["spark"], fast(), |f| f.set_runs("spark", &[A]));
            r.runs("spark", 1);
            r.open("spark", A);
            r.fake.feed("spark", A, &encode_closed(reason));
            let got = r.until(|m| match m {
                LinkMsg::StreamClosed { id, why, reopen } => {
                    assert_eq!(*id, format!("spark/{A}"));
                    Some((why.clone(), *reopen))
                }
                _ => None,
            });
            assert_eq!(got, (reason.to_string(), reopen));
            let id = format!("spark/{A}");
            assert_eq!(
                r.hub.open(&id, "spark", A, 80, 24).is_ok(),
                reopen,
                "{reason}"
            );
        }
    }

    #[test]
    fn idle_and_detached_reopen_only_near_the_renewal_time() {
        let t = Timing {
            stream_renew: Duration::from_millis(50),
            ..fast()
        };
        for reason in ["idle", "detached"] {
            let mut r = rig(&["spark"], t, |f| f.set_runs("spark", &[A]));
            r.runs("spark", 1);
            r.open("spark", A);
            thread::sleep(Duration::from_millis(80));
            r.fake.feed("spark", A, &encode_closed(reason));
            let reopen = r.until(|m| match m {
                LinkMsg::StreamClosed { reopen, .. } => Some(*reopen),
                _ => None,
            });
            assert!(reopen, "{reason}");
        }
    }

    #[test]
    fn an_open_is_not_sent_for_a_run_that_left_the_list_during_the_dial() {
        let mut r = rig(&["spark"], fast(), |f| f.set_runs("spark", &[A]));
        r.runs("spark", 1);
        r.fake
            .with("spark", |l| l.dial_delay = Duration::from_millis(1500));
        r.hub
            .open(&view_id("spark", A), "spark", A, 80, 24)
            .unwrap();
        let opened = Instant::now();
        r.fake.set_runs("spark", &[B]);
        while r.hub.target_of(&view_id("spark", A)) != Target::Unknown {
            assert!(
                opened.elapsed() < Duration::from_millis(1400),
                "the list did not change in time"
            );
            thread::sleep(Duration::from_millis(10));
        }
        thread::sleep(Duration::from_millis(2000));
        assert!(r.fake.calls("spark", "attach.open").is_empty());
    }

    #[test]
    fn a_dropped_connection_is_a_notice_not_a_silent_reopen_unless_near_the_hour() {
        let mut r = rig(&["spark"], fast(), |f| f.set_runs("spark", &[A]));
        r.runs("spark", 1);
        r.open("spark", A);
        r.fake.hang_up("spark", A);
        let reopen = r.until(|m| match m {
            LinkMsg::StreamClosed { reopen, .. } => Some(*reopen),
            _ => None,
        });
        assert!(!reopen);

        let t = Timing {
            stream_renew: Duration::from_millis(50),
            ..fast()
        };
        let mut r = rig(&["spark"], t, |f| f.set_runs("spark", &[A]));
        r.runs("spark", 1);
        r.open("spark", A);
        thread::sleep(Duration::from_millis(80));
        r.fake.hang_up("spark", A);
        let reopen = r.until(|m| match m {
            LinkMsg::StreamClosed { reopen, .. } => Some(*reopen),
            _ => None,
        });
        assert!(reopen);
    }

    #[test]
    fn an_offline_machine_refuses_new_streams() {
        let mut r = rig(&["spark"], fast(), |f| {
            f.set_runs("spark", &[A]);
            f.with("spark", |l| l.hang = true);
        });
        r.state_is("spark", LinkState::Offline);
        assert!(r
            .hub
            .open(&format!("spark/{A}"), "spark", A, 80, 24)
            .is_err());
    }

    #[test]
    fn shutdown_stops_the_control_threads_promptly() {
        let r = rig(&["spark", "lab"], fast(), |_| {});
        let began = Instant::now();
        r.hub.shutdown();
        assert!(began.elapsed() < Duration::from_secs(1));
        assert!(r
            .hub
            .open(&view_id("spark", A), "spark", A, 80, 24)
            .is_err());
    }

    #[test]
    fn long_follower_strings_are_cut_before_they_reach_the_sidebar() {
        let mut st = RemoteState::new(&["spark".into()]);
        let long = "x".repeat(5000);
        let mut run = run_view(A);
        run.name = long.clone();
        run.agent = Some(long.clone());
        run.task = Some(long.clone());
        run.state.detail = Some(long.clone());
        let kept = st.ingest_runs("spark", vec![run]);
        assert!(kept[0].name.chars().count() <= MAX_NAME + 1);
        assert!(kept[0].agent.as_deref().unwrap().chars().count() <= MAX_NAME + 1);
        assert!(kept[0].task.as_deref().unwrap().chars().count() <= MAX_NAME + 1);
        assert!(kept[0].state.detail.as_deref().unwrap().chars().count() <= MAX_DETAIL + 1);
        let ev = EventMsg {
            session: A.into(),
            state: State {
                detail: Some(long),
                ..state(StateKind::Waiting)
            },
            task: None,
            origin: None,
        };
        let Some(Incoming::Event(e)) = st.ingest("spark", Incoming::Event(ev)) else {
            panic!("listed run must pass");
        };
        assert!(e.state.detail.unwrap().chars().count() <= MAX_DETAIL + 1);
        assert!(st.rows("spark")[0].name.chars().count() <= MAX_NAME + 1);
    }

    #[test]
    fn shutdown_and_drop_both_release_the_dialer() {
        let r = rig(&["spark"], fast(), |_| {});
        let fake = Arc::clone(&r.fake);
        r.hub.shutdown();
        assert!(fake.1.load(Ordering::SeqCst));
        let r = rig(&["spark"], fast(), |_| {});
        let fake = Arc::clone(&r.fake);
        drop(r);
        assert!(fake.1.load(Ordering::SeqCst));
    }

    struct CountingKey {
        loads: Arc<std::sync::atomic::AtomicUsize>,
        fail: Arc<AtomicBool>,
    }

    impl KeySource for CountingKey {
        fn load(&self) -> Result<Secret> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            if self.fail.load(Ordering::SeqCst) {
                return Err(Error::not_found("this machine has no link identity"));
            }
            Ok(Secret::new("pem"))
        }
    }

    type Loads = Arc<std::sync::atomic::AtomicUsize>;

    fn ssh_dialer() -> (SshDialer, tempfile::TempDir, Loads, Arc<AtomicBool>) {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let loads = Loads::default();
        let fail = Arc::new(AtomicBool::new(false));
        let root = d.path().join("keys");
        std::fs::create_dir(&root).unwrap();
        let dialer = SshDialer::with_source(
            Paths::under(d.path()),
            Box::new(CountingKey {
                loads: Arc::clone(&loads),
                fail: Arc::clone(&fail),
            }),
            root,
        );
        (dialer, d, loads, fail)
    }

    #[test]
    fn the_ssh_dialer_reads_the_key_on_the_first_dial_and_writes_a_fresh_file_each_time() {
        let (dialer, _d, loads, _) = ssh_dialer();
        assert_eq!(loads.load(Ordering::SeqCst), 0);
        let first = dialer.fresh_key().unwrap();
        let second = dialer.fresh_key().unwrap();
        assert_eq!(loads.load(Ordering::SeqCst), 1);
        assert_ne!(first.path, second.path);
        assert_eq!(std::fs::read_to_string(&first.path).unwrap(), "pem");
        let (p1, p2) = (first.path.clone(), second.path.clone());
        drop(first);
        assert!(!p1.exists());
        assert!(p2.exists());
        drop(second);
        assert!(!p2.exists());
    }

    struct SlowKey(Mutex<Option<std::sync::mpsc::Receiver<()>>>);

    impl KeySource for SlowKey {
        fn load(&self) -> Result<Secret> {
            let rx = lock(&self.0).take().unwrap();
            let _ = rx.recv();
            Ok(Secret::new("pem"))
        }
    }

    #[test]
    fn shutdown_does_not_wait_for_a_key_prompt_and_nothing_is_kept_afterwards() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        std::fs::set_permissions(d.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let (release, gate) = mpsc::channel();
        let dialer = Arc::new(SshDialer::with_source(
            Paths::under(d.path()),
            Box::new(SlowKey(Mutex::new(Some(gate)))),
            d.path().to_path_buf(),
        ));
        let dialling = {
            let dialer = Arc::clone(&dialer);
            thread::spawn(move || dialer.fresh_key().err())
        };
        thread::sleep(Duration::from_millis(100));
        let began = Instant::now();
        dialer.shutdown();
        assert!(began.elapsed() < Duration::from_millis(100));
        release.send(()).unwrap();
        assert!(matches!(
            dialling.join().unwrap(),
            Some(DialErr::Offline(_))
        ));
        assert!(lock(&dialer.held).is_none());
    }

    #[test]
    fn a_dial_with_no_follower_reports_removed_without_reading_the_key() {
        let (dialer, _d, loads, _) = ssh_dialer();
        assert_eq!(dialer.dial("spark").err(), Some(DialErr::Removed));
        assert_eq!(loads.load(Ordering::SeqCst), 0);
    }

    fn add_follower(dialer: &SshDialer, name: &str, host: &str) {
        let (name, host) = (name.to_string(), host.to_string());
        Store::update(&dialer.paths, |s| {
            s.followers.insert(
                name,
                tmgr_link::store::Follower {
                    user: "u".into(),
                    host,
                    port: 22,
                    host_key_fingerprint: "SHA256:x".into(),
                    added: "2026-01-01".into(),
                },
            );
            Ok(())
        })
        .unwrap();
    }

    #[test]
    fn an_unreadable_key_is_unavailable_and_read_again_on_the_next_dial() {
        let (dialer, _d, loads, fail) = ssh_dialer();
        add_follower(&dialer, "spark", "one.example.com");
        fail.store(true, Ordering::SeqCst);
        assert!(matches!(
            dialer.dial("spark"),
            Err(DialErr::Unavailable(m)) if m.contains("no link identity")
        ));
        fail.store(false, Ordering::SeqCst);
        assert!(dialer.fresh_key().is_ok());
        assert_eq!(loads.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn the_ssh_dialer_forgets_the_key_on_shutdown_and_refuses_later_dials() {
        let (dialer, _d, loads, _) = ssh_dialer();
        add_follower(&dialer, "spark", "one.example.com");
        drop(dialer.fresh_key().unwrap());
        dialer.shutdown();
        assert!(matches!(dialer.dial("spark"), Err(DialErr::Offline(_))));
        assert!(dialer.fresh_key().is_err());
        assert_eq!(loads.load(Ordering::SeqCst), 1);
    }

    fn key_dirs(root: &std::path::Path) -> usize {
        std::fs::read_dir(root)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("tmgr-link-")
            })
            .count()
    }

    #[test]
    fn shutdown_deletes_the_key_file_of_a_dial_still_in_flight() {
        let (dialer, _d, _, _) = ssh_dialer();
        let key = dialer.fresh_key().unwrap();
        let path = key.path.clone();
        assert!(path.exists());
        dialer.shutdown();
        assert!(!path.exists());
        assert_eq!(key_dirs(&dialer.root), 0);
        drop(key);
    }

    #[test]
    fn no_key_file_is_written_after_shutdown() {
        let (dialer, _d, _, _) = ssh_dialer();
        dialer.shutdown();
        assert!(dialer.fresh_key().is_err());
        assert_eq!(key_dirs(&dialer.root), 0);
    }

    #[test]
    fn keys_written_while_shutdown_races_never_survive_it() {
        let (dialer, _d, _, _) = ssh_dialer();
        let dialer = Arc::new(dialer);
        drop(dialer.fresh_key().unwrap());
        let workers: Vec<_> = (0..8)
            .map(|_| {
                let dialer = Arc::clone(&dialer);
                thread::spawn(move || {
                    let mut held = Vec::new();
                    for _ in 0..50 {
                        if let Ok(k) = dialer.fresh_key() {
                            held.push(k);
                        }
                    }
                    held
                })
            })
            .collect();
        thread::sleep(Duration::from_millis(2));
        dialer.shutdown();
        let held: Vec<_> = workers
            .into_iter()
            .flat_map(|w| w.join().unwrap())
            .collect();
        assert!(held.iter().all(|k| !k.path.exists()));
        assert_eq!(key_dirs(&dialer.root), 0);
    }

    #[test]
    fn forgetting_makes_the_ssh_dialer_read_the_key_again() {
        let (dialer, _d, loads, _) = ssh_dialer();
        drop(dialer.fresh_key().unwrap());
        dialer.forget();
        drop(dialer.fresh_key().unwrap());
        assert_eq!(loads.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn the_ssh_dialer_binding_follows_the_follower_entry_and_its_jump() {
        let (dialer, _d, _, _) = ssh_dialer();
        assert_eq!(dialer.binding("spark"), Binding::Gone);
        let add = |name: &str, host: &str| add_follower(&dialer, name, host);
        add("spark", "one.example.com");
        let first = dialer.binding("spark");
        assert!(matches!(first, Binding::Is(_)));
        assert_eq!(dialer.binding("spark"), first);
        add("spark", "two.example.com");
        let second = dialer.binding("spark");
        assert_ne!(second, first);
        add("hop", "hop.example.com");
        Store::update(&dialer.paths, |s| {
            s.jumps.insert("spark".into(), "hop".into());
            Ok(())
        })
        .unwrap();
        let third = dialer.binding("spark");
        assert_ne!(third, second);
        add("hop", "other.example.com");
        assert_ne!(dialer.binding("spark"), third);
        Store::update(&dialer.paths, |s| {
            s.followers.remove("spark");
            Ok(())
        })
        .unwrap();
        assert_eq!(dialer.binding("spark"), Binding::Gone);
    }

    #[test]
    fn an_unreadable_key_stops_the_machine_with_one_notice_until_retry() {
        let mut r = rig(&["spark", "lab"], fast(), |f| {
            f.set_runs("spark", &[A]);
            f.set_runs("lab", &[A]);
            for l in ["spark", "lab"] {
                f.with(l, |l| {
                    l.dial = VecDeque::from([Err(DialErr::Unavailable("keychain locked".into()))])
                });
            }
        });
        let mut denied = 0;
        r.until(|m| {
            if matches!(
                m,
                LinkMsg::State {
                    state: LinkState::NoAccess,
                    ..
                }
            ) {
                denied += 1;
            }
            (denied == 2).then_some(())
        });
        let after = r.drain(Duration::from_millis(200));
        let notices = r
            .seen
            .iter()
            .cloned()
            .chain(after.iter().map(|m| format!("{m:?}")))
            .filter(|m| m.contains("keychain locked"))
            .count();
        assert_eq!(notices, 1, "{:#?}", r.seen);
        assert_eq!(r.fake.with("spark", |l| l.dials.len()), 1);
        r.hub.retry_all();
        let mut online = BTreeSet::new();
        r.until(|m| {
            if let LinkMsg::State {
                link,
                state: LinkState::Online { .. },
            } = m
            {
                online.insert(link.clone());
            }
            (online.len() == 2).then_some(())
        });
    }

    #[test]
    fn a_follower_that_stops_reading_cannot_pin_the_stream_threads_or_ssh() {
        let mut r = rig(&["spark"], fast(), |f| {
            f.set_runs("spark", &[A]);
            f.with("spark", |l| {
                l.input = true;
                l.block_writes = true;
            });
        });
        r.runs("spark", 1);
        r.open("spark", A);
        let id = view_id("spark", A);
        r.hub.input(&id, b"x").unwrap();
        thread::sleep(Duration::from_millis(60));
        let killed = |r: &Rig| r.fake.with("spark", |l| l.killed.load(Ordering::SeqCst));
        assert!(!killed(&r));
        r.hub.close(&id);
        let deadline = Instant::now() + Duration::from_secs(2);
        while !killed(&r) {
            assert!(Instant::now() < deadline, "the connection was never killed");
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_stream_that_the_follower_ends_is_killed_too() {
        let mut r = rig(&["spark"], fast(), |f| {
            f.set_runs("spark", &[A]);
            f.with("spark", |l| {
                l.input = true;
                l.block_writes = true;
            });
        });
        r.runs("spark", 1);
        r.open("spark", A);
        r.fake.feed("spark", A, &encode_closed("runner: gone"));
        let deadline = Instant::now() + Duration::from_secs(2);
        while !r.fake.with("spark", |l| l.killed.load(Ordering::SeqCst)) {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_relinked_machine_mid_session_ends_the_old_connection_and_its_streams() {
        let t = Timing {
            poll: Duration::from_millis(300),
            ..fast()
        };
        let mut r = rig(&["spark"], t, |f| {
            f.set_runs("spark", &[A]);
            f.with("spark", |l| l.input = true);
        });
        r.runs("spark", 1);
        r.open("spark", A);
        let id = view_id("spark", A);
        let dials = r.fake.with("spark", |l| l.dials.len());
        r.fake.with("spark", |l| l.bind = 7);
        r.hub.halt("spark", A).unwrap();
        let result = r.until(|m| match m {
            LinkMsg::Halted { result, .. } => Some(result.clone()),
            _ => None,
        });
        assert!(result.is_err());
        assert!(r.fake.calls("spark", "runs.halt").is_empty());
        let deadline = Instant::now() + Duration::from_secs(3);
        while r.hub.input(&id, b"x") != Err("not attached") {
            assert!(Instant::now() < deadline, "the old stream stayed attached");
            thread::sleep(Duration::from_millis(10));
        }
        while r.fake.with("spark", |l| l.dials.len()) <= dials {
            assert!(Instant::now() < deadline, "no fresh connection");
            thread::sleep(Duration::from_millis(10));
        }
        assert!(r.fake.calls("spark", "runs.halt").is_empty());
        r.runs("spark", 1);
    }

    #[test]
    fn a_machine_unlinked_mid_session_stops_polling_and_loses_its_rows_and_streams() {
        let mut r = rig(&["spark"], fast(), |f| {
            f.set_runs("spark", &[A]);
            f.with("spark", |l| l.input = true);
        });
        r.runs("spark", 1);
        r.open("spark", A);
        r.fake.with("spark", |l| l.unbound = true);
        r.state_is("spark", LinkState::Removed);
        r.runs("spark", 0);
        assert!(r.hub.rows("spark").is_empty());
        assert_eq!(r.hub.input(&view_id("spark", A), b"x"), Err("not attached"));
        thread::sleep(Duration::from_millis(60));
        let polls = r.fake.calls("spark", "runs.list").len();
        thread::sleep(Duration::from_millis(150));
        assert_eq!(r.fake.calls("spark", "runs.list").len(), polls);
        r.fake.with("spark", |l| l.unbound = false);
        r.hub.retry_all();
        r.runs("spark", 1);
    }

    #[test]
    fn retry_makes_the_dialer_forget_the_key() {
        let r = rig(&["spark"], fast(), |_| {});
        r.hub.retry_all();
        assert_eq!(r.fake.2.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_dial_that_ends_in_no_access_makes_the_dialer_forget_the_key() {
        let mut r = rig(&["spark"], fast(), |f| {
            f.with("spark", |l| {
                l.dial = VecDeque::from([Err(DialErr::NoAccess("revoked".into()))])
            })
        });
        r.state_is("spark", LinkState::NoAccess);
        assert_eq!(r.fake.2.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_forbidden_list_makes_the_dialer_forget_the_key() {
        let mut r = rig(&["spark"], fast(), |f| {
            f.with("spark", |l| l.list_err = Some(Kind::Forbidden))
        });
        r.state_is("spark", LinkState::NoAccess);
        assert_eq!(r.fake.2.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn a_relinked_machine_loses_its_rows_until_the_new_session_lists() {
        let mut r = rig(&["spark"], fast(), |f| f.set_runs("spark", &[A]));
        r.runs("spark", 1);
        r.fake.with("spark", |l| {
            l.dial_delay = Duration::from_millis(1500);
            l.bind = 7;
        });
        r.runs("spark", 0);
        assert!(r.hub.rows("spark").is_empty());
        assert_eq!(r.hub.target_of(&view_id("spark", A)), Target::Unknown);
        assert!(r
            .hub
            .open(&view_id("spark", A), "spark", A, 80, 24)
            .is_err());
        r.runs("spark", 1);
        assert_eq!(r.hub.rows("spark").len(), 1);
    }

    #[test]
    fn a_stream_opened_after_a_silent_relink_is_refused_before_it_dials() {
        let t = Timing {
            poll: Duration::from_millis(500),
            ..fast()
        };
        let mut r = rig(&["spark"], t, |f| f.set_runs("spark", &[A]));
        r.runs("spark", 1);
        let dials = r.fake.with("spark", |l| l.dials.len());
        r.fake.with("spark", |l| l.bind = 7);
        let id = view_id("spark", A);
        r.hub.open(&id, "spark", A, 80, 24).unwrap();
        let why = r.until(|m| match m {
            LinkMsg::StreamClosed { id: i, why, .. } if *i == id => Some(why.clone()),
            _ => None,
        });
        assert!(why.contains("changed"), "{why}");
        assert!(r.fake.calls("spark", "attach.open").is_empty());
        assert_eq!(r.fake.with("spark", |l| l.dials.len()), dials);
    }

    #[test]
    fn a_halt_stamped_for_another_binding_never_reaches_the_machine() {
        let mut r = rig(&["spark"], fast(), |f| f.set_runs("spark", &[A]));
        r.runs("spark", 1);
        r.hub.ctl["spark"]
            .send(Cmd::Halt(A.into(), Some(Binding::Is("stale".into()))))
            .unwrap();
        let result = r.until(|m| match m {
            LinkMsg::Halted { result, .. } => Some(result.clone()),
            _ => None,
        });
        assert!(result.is_err());
        assert!(r.fake.calls("spark", "runs.halt").is_empty());
    }

    #[test]
    fn a_halt_for_a_run_that_is_no_longer_listed_never_reaches_the_machine() {
        let mut r = rig(&["spark"], fast(), |f| f.set_runs("spark", &[A]));
        r.runs("spark", 1);
        r.hub.ctl["spark"]
            .send(Cmd::Halt(B.into(), Some(Binding::Is("0".into()))))
            .unwrap();
        let result = r.until(|m| match m {
            LinkMsg::Halted { result, .. } => Some(result.clone()),
            _ => None,
        });
        assert!(result.is_err());
        assert!(r.fake.calls("spark", "runs.halt").is_empty());
    }

    #[test]
    fn the_latest_notice_of_a_burst_is_delivered_when_the_window_opens() {
        let mut r = rig(&["spark"], fast(), |f| f.set_runs("spark", &[A]));
        r.runs("spark", 1);
        r.open("spark", A);
        for i in 0..4 {
            r.fake.feed("spark", A, &encode_notice(&format!("n{i}")));
        }
        thread::sleep(Duration::from_millis(400));
        let texts: Vec<String> = r
            .drain(Duration::ZERO)
            .into_iter()
            .filter_map(|m| match m {
                LinkMsg::Notice { text, .. } => Some(text),
                _ => None,
            })
            .collect();
        assert_eq!(texts.last().map(String::as_str), Some("n3"), "{texts:?}");
        assert_eq!(texts.len(), 2, "{texts:?}");
    }

    #[test]
    fn a_noisy_stream_gets_at_most_a_notice_every_so_often() {
        let mut r = rig(&["spark"], fast(), |f| f.set_runs("spark", &[A]));
        r.runs("spark", 1);
        r.open("spark", A);
        for i in 0..300 {
            r.fake.feed("spark", A, &encode_notice(&format!("n{i}")));
        }
        thread::sleep(Duration::from_millis(200));
        let notices = r
            .drain(Duration::ZERO)
            .iter()
            .filter(|m| matches!(m, LinkMsg::Notice { .. }))
            .count();
        assert!((1..=3).contains(&notices), "{notices}");
    }

    fn pushes_agree(base: &ScreenPush, seq: &[ScreenPush]) {
        use tmgr_runner::client::ScreenCache;
        let mut one = ScreenCache::new();
        one.apply_push(base.clone());
        let mut merged = seq[0].clone();
        for p in &seq[1..] {
            one.apply_push(p.clone());
            merged = merge_screens(merged, p.clone());
        }
        one.apply_push(seq[0].clone());
        let mut seqcache = ScreenCache::new();
        seqcache.apply_push(base.clone());
        for p in seq {
            seqcache.apply_push(p.clone());
        }
        let mut mergedcache = ScreenCache::new();
        mergedcache.apply_push(base.clone());
        mergedcache.apply_push(merged);
        assert_eq!(
            format!("{:?}", mergedcache.snapshot(&base.session)),
            format!("{:?}", seqcache.snapshot(&base.session))
        );
    }

    #[test]
    fn merging_pending_screens_never_loses_a_line() {
        let base = full_push("x", 0);
        let mut n = 1;
        let mut next = |full: bool| {
            n += 1;
            if full {
                full_push("x", n)
            } else {
                diff_push("x", n)
            }
        };
        let diffs: Vec<ScreenPush> = (0..5).map(|_| next(false)).collect();
        pushes_agree(&base, &diffs);
        let mixed = vec![
            next(false),
            next(false),
            next(true),
            next(false),
            next(false),
        ];
        pushes_agree(&base, &mixed);
        let full_first = vec![next(true), next(false), next(false), next(false)];
        pushes_agree(&base, &full_first);
        let mut lcg = 12345u64;
        for _ in 0..200 {
            let len = 2 + (lcg % 6) as usize;
            let seq: Vec<ScreenPush> = (0..len)
                .map(|_| {
                    lcg = lcg
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1442695040888963407);
                    next((lcg >> 33).is_multiple_of(5))
                })
                .collect();
            pushes_agree(&base, &seq);
        }
    }

    #[test]
    fn a_stream_that_goes_silent_is_closed_and_frees_its_slot() {
        let t = Timing {
            heartbeat: Duration::from_millis(20),
            silence: Duration::from_millis(120),
            ..fast()
        };
        let mut r = rig(&["spark"], t, |f| f.set_runs("spark", &[A]));
        r.runs("spark", 1);
        r.open("spark", A);
        let (why, reopen) = r.until(|m| match m {
            LinkMsg::StreamClosed { why, reopen, .. } => Some((why.clone(), *reopen)),
            _ => None,
        });
        assert!(why.contains("no answer"), "{why}");
        assert!(!reopen);
        assert!(r.fake.written("spark", A).contains(&Outgoing::Ping));
        assert_eq!(r.hub.want_streams(&[]).close, Vec::<String>::new());
    }

    #[test]
    fn a_stream_that_keeps_hearing_back_stays_open() {
        let t = Timing {
            heartbeat: Duration::from_millis(20),
            silence: Duration::from_millis(600),
            ..fast()
        };
        let mut r = rig(&["spark"], t, |f| f.set_runs("spark", &[A]));
        r.runs("spark", 1);
        r.open("spark", A);
        for _ in 0..50 {
            r.fake.feed("spark", A, br#"{"type":"pong"}"#);
            thread::sleep(Duration::from_millis(30));
        }
        let msgs = r.drain(Duration::ZERO);
        assert!(!msgs
            .iter()
            .any(|m| matches!(m, LinkMsg::StreamClosed { .. })));
    }

    #[test]
    fn a_relinked_machine_gets_its_rows_back() {
        let mut r = rig(&["spark"], fast(), |f| {
            f.set_runs("spark", &[A]);
            f.with("spark", |l| {
                l.dial = VecDeque::from([Err(DialErr::Removed)])
            });
        });
        r.state_is("spark", LinkState::Removed);
        assert!(r.hub.rows("spark").is_empty());
        r.hub.retry_all();
        r.runs("spark", 1);
        assert_eq!(r.hub.rows("spark").len(), 1);
        assert_eq!(
            r.hub.target_of(&format!("spark/{A}")),
            Target::Remote {
                link: "spark".into(),
                run: A.into()
            }
        );
    }

    fn text_line(t: &str) -> Line {
        Line {
            spans: vec![Span {
                a: Attr {
                    fg: Color::Default,
                    bg: Color::Default,
                    flags: 0,
                },
                t: t.into(),
            }],
        }
    }

    fn full_push(session: &str, n: usize) -> ScreenPush {
        ScreenPush {
            session: session.into(),
            full: true,
            snapshot: Some(tmgr_ipc::api::Snapshot {
                rows: 4,
                cols: 20,
                lines: (0..4).map(|i| text_line(&format!("full{n}-{i}"))).collect(),
                cursor: None,
                modes: tmgr_ipc::api::Modes {
                    app_cursor: false,
                    bracketed_paste: false,
                    alt_screen: false,
                    hide_cursor: false,
                    mouse: Default::default(),
                },
                scroll_offset: 0,
                scroll_max: 0,
            }),
            lines: Vec::new(),
            cursor: None,
            modes: None,
            scroll_offset: 0,
            scroll_max: 0,
        }
    }

    fn diff_push(session: &str, n: usize) -> ScreenPush {
        ScreenPush {
            session: session.into(),
            full: false,
            snapshot: None,
            lines: vec![
                (n % 4, text_line(&format!("d{n}"))),
                ((n + 1) % 4, text_line(&format!("e{n}"))),
            ],
            cursor: Some(((n % 20) as u16, (n % 4) as u16)),
            modes: n.is_multiple_of(3).then(|| tmgr_ipc::api::Modes {
                app_cursor: n.is_multiple_of(2),
                bracketed_paste: false,
                alt_screen: false,
                hide_cursor: false,
                mouse: Default::default(),
            }),
            scroll_offset: n % 7,
            scroll_max: 10,
        }
    }

    #[test]
    fn a_flooding_follower_cannot_fill_the_app_queue_and_the_last_screen_wins() {
        use tmgr_runner::client::ScreenCache;
        let fake = Arc::new(Fake::default());
        fake.set_runs("spark", &[A]);
        let (tx, rx) = mpsc::sync_channel(8);
        let local = tx.clone();
        let hub = Hub::start_with(
            Arc::new(FakeDialer(Arc::clone(&fake))),
            vec!["spark".into()],
            tx,
            Timing {
                poll: Duration::from_secs(30),
                ..fast()
            },
        );
        let id = view_id("spark", A);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !matches!(hub.target_of(&id), Target::Remote { .. }) {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        while rx.try_recv().is_ok() {}
        hub.open(&id, "spark", A, 80, 24).unwrap();
        loop {
            match rx.recv_timeout(Duration::from_secs(5)).unwrap() {
                LinkMsg::Opened { .. } => break,
                _ => continue,
            }
        }
        let mut reference = ScreenCache::new();
        for n in 0..10_000 {
            let p = if n % 1000 == 0 {
                full_push(A, n)
            } else {
                diff_push(A, n)
            };
            fake.feed("spark", A, &encode_screen(&p));
            let mut r = p;
            r.session = id.clone();
            reference.apply_push(r);
        }
        thread::sleep(Duration::from_millis(400));
        local
            .try_send(LinkMsg::Notice {
                link: "local".into(),
                id: None,
                text: "still room".into(),
            })
            .expect("local messages must find room");
        let mut cache = ScreenCache::new();
        let mut screens = 0;
        let mut queued = 0;
        while let Ok(m) = rx.try_recv() {
            if let LinkMsg::Screen(p, ..) = m {
                queued += 1;
                screens += 1;
                cache.apply_push(*p);
            }
        }
        assert!(queued <= 2, "{queued} screens were queued");
        while let Ok(m) = rx.recv_timeout(Duration::from_millis(400)) {
            if let LinkMsg::Screen(p, ..) = m {
                screens += 1;
                cache.apply_push(*p);
            }
        }
        assert!(screens < 100, "{screens} screens were delivered");
        assert_eq!(
            format!("{:?}", cache.snapshot(&id)),
            format!("{:?}", reference.snapshot(&id))
        );
        assert!(cache.snapshot(&id).is_some());
    }
}
