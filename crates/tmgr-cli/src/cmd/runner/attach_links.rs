#![allow(dead_code)]

use super::remote::{self, RunView};
use super::remote_attach::{age, text_chunks};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{Read, Write};
use std::process::{ChildStdin, ChildStdout};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tmgr_core::config::{self, Config};
use tmgr_core::creds::Backend;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::{EventMsg, ScreenPush};
use tmgr_ipc::proto::valid_session_id;
use tmgr_link::attach::{read_incoming, write_outgoing, Incoming, Outgoing};
use tmgr_link::leader::{self, Client, KeyFile};
use tmgr_link::store::{Paths, Store};
use tmgr_runner::attach::SessionRow;

const MAX_RUNS_PER_LINK: usize = 200;
const STREAMS_PER_LINK: usize = 2;
const OUT_QUEUE: usize = 256;

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
    pub stream_renew: Duration,
    pub cooldown: Duration,
    pub shutdown_wait: Duration,
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
            stream_renew: Duration::from_secs(55 * 60),
            cooldown: Duration::from_secs(30),
            shutdown_wait: Duration::from_secs(1),
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
}

impl DialErr {
    fn why(&self, link: &str) -> String {
        match self {
            DialErr::Offline(_) => format!("{link} is offline"),
            DialErr::HostKeyChanged => format!("{link}: the host key changed"),
            DialErr::NoAccess(_) => format!("{link}: no access"),
            DialErr::Refused(_) => format!("{link} is busy: no free connection slot"),
            DialErr::Removed => format!("{link} is no longer linked"),
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

pub(super) trait Dialer: Send + Sync + 'static {
    fn dial(&self, link: &str) -> std::result::Result<Box<dyn LinkConn>, DialErr>;
}

pub(super) trait LinkConn: Send {
    fn call(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value>;
    fn into_stream(self: Box<Self>) -> Result<(Box<dyn Read + Send>, Box<dyn Write + Send>)>;
}

pub(super) struct SshDialer {
    paths: Paths,
    key: KeyFile,
}

impl SshDialer {
    pub(super) fn new() -> Result<SshDialer> {
        let paths = Paths::from_env()?;
        let cfg = Config::load(&config::config_dir()?)?;
        let preferred = cfg.credential_store.as_deref().and_then(Backend::parse);
        let key = leader::materialise(&leader::credential_store(&paths, preferred))?;
        Ok(SshDialer { paths, key })
    }
}

impl Dialer for SshDialer {
    fn dial(&self, link: &str) -> std::result::Result<Box<dyn LinkConn>, DialErr> {
        let mut client =
            Client::dial_with_key(&self.paths, link, &self.key).map_err(|e| classify(&e))?;
        client.hello().map_err(|e| classify(&e))?;
        client.disarm();
        Ok(Box::new(SshConn(client)))
    }
}

struct SshConn(Client);

impl LinkConn for SshConn {
    fn call(&mut self, method: &str, params: Value, timeout: Duration) -> Result<Value> {
        remote::check(self.0.call_within(method, params, timeout)?)
    }

    fn into_stream(mut self: Box<Self>) -> Result<(Box<dyn Read + Send>, Box<dyn Write + Send>)> {
        let (out, input) = self.0.take_stream()?;
        let owner = Arc::new(Mutex::new(self.0));
        Ok((
            Box::new(HeldRead(out, Arc::clone(&owner))),
            Box::new(HeldWrite(input, owner)),
        ))
    }
}

struct HeldRead(ChildStdout, #[allow(dead_code)] Arc<Mutex<Client>>);
struct HeldWrite(ChildStdin, #[allow(dead_code)] Arc<Mutex<Client>>);

impl Read for HeldRead {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
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

pub(super) fn followers(paths: &Paths) -> Vec<String> {
    Store::load(paths)
        .map(|s| {
            s.followers
                .keys()
                .filter(|n| tmgr_link::grammar::link_name(n).is_ok())
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
    Screen(Box<ScreenPush>),
    Event(EventMsg),
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
                kept.push(r);
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

enum Cmd {
    Halt(String),
    Retry,
    Shutdown,
}

struct Stream {
    gen: u64,
    link: String,
    out: Option<SyncSender<Outgoing>>,
    input: Option<bool>,
    utf8: Vec<u8>,
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
            streams.remove(id);
            state.mark_closed(id);
            true
        } else {
            false
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
        if let Some(out) = removed.and_then(|s| s.out) {
            let _ = out.try_send(Outgoing::Detach);
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

    pub(super) fn state(&self) -> MutexGuard<'_, RemoteState> {
        lock(&self.inner.state)
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
            .send(Cmd::Halt(run.to_string()))
            .map_err(|_| "the link is shutting down")
    }

    pub(super) fn retry_all(&self) {
        lock(&self.inner.cooldown).clear();
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
            Ok(Cmd::Halt(run)) => halt_refused(inner, link, &run, "the machine is not reachable"),
            Ok(Cmd::Shutdown) | Err(RecvTimeoutError::Disconnected) => return false,
        }
    }
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
                    DialErr::HostKeyChanged | DialErr::NoAccess(_) | DialErr::Removed => {
                        inner.lose_link(link);
                        let state = match e {
                            DialErr::HostKeyChanged => LinkState::HostKeyChanged,
                            DialErr::NoAccess(_) => LinkState::NoAccess,
                            _ => {
                                lock(&inner.state).remove_link(link);
                                LinkState::Removed
                            }
                        };
                        let removed = state == LinkState::Removed;
                        inner.set_state(link, state);
                        if removed {
                            inner.emit(LinkMsg::Runs {
                                link: link.to_string(),
                                runs: Vec::new(),
                            });
                        }
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
        backoff = t.backoff_min;
        match session(inner, link, conn, cmds) {
            Exit::Shutdown => return,
            Exit::Planned => quiet = true,
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
        }
    }
}

fn session(inner: &Inner, link: &str, mut conn: Box<dyn LinkConn>, cmds: &Receiver<Cmd>) -> Exit {
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
                inner.set_state(link, LinkState::NoAccess);
                return Exit::Denied;
            }
            Err(_) => return Exit::Lost,
        }
        match cmds.recv_timeout(t.poll) {
            Err(RecvTimeoutError::Timeout) => {}
            Ok(Cmd::Halt(run)) => {
                requests += 1;
                halt(inner, link, &run, conn.as_mut());
            }
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
            .map_err(|e| e.message)
    } else {
        Err("that run is not listed on this machine".to_string())
    };
    inner.emit(LinkMsg::Halted { id, result });
}

struct StreamSpec {
    id: String,
    link: String,
    run: String,
    cols: u16,
    rows: u16,
    gen: u64,
}

type Pair = (Box<dyn Read + Send>, Box<dyn Write + Send>);

fn open_stream(inner: &Inner, spec: &StreamSpec) -> std::result::Result<(bool, Pair), String> {
    let mut conn = inner
        .dialer
        .dial(&spec.link)
        .map_err(|e| e.why(&spec.link))?;
    let v = conn
        .call(
            "attach.open",
            json!({"run": spec.run, "read_only": false, "cols": spec.cols, "rows": spec.rows}),
            inner.timing.open,
        )
        .map_err(|e| format!("{}: {}", spec.link, e.message))?;
    let input = v.get("input").and_then(Value::as_bool).unwrap_or(false);
    let pair = conn
        .into_stream()
        .map_err(|e| format!("{}: {}", spec.link, e.message))?;
    Ok((input, pair))
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

fn stream_main(inner: &Inner, spec: &StreamSpec) {
    let t = inner.timing;
    let (input, (mut reader, writer)) = match open_stream(inner, spec) {
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
            }
            _ => {
                drop(streams);
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
    thread::spawn(move || writer_main(writer, &out_rx, t.heartbeat, t.stream_renew));
    loop {
        let got = read_incoming(&mut reader, &spec.run);
        if !inner.alive(&spec.id, spec.gen) {
            return;
        }
        let near_hour = born.elapsed() >= t.stream_renew;
        match got {
            Ok(Some(Incoming::Pong)) => {}
            Ok(Some(Incoming::Notice(text))) => {
                inner.emit(LinkMsg::Notice {
                    link: spec.link.clone(),
                    id: Some(spec.id.clone()),
                    text,
                });
            }
            Ok(Some(Incoming::Closed(why))) => {
                let reopen = reopens(&why);
                return close_stream(inner, spec, why, reopen);
            }
            Ok(Some(msg)) => {
                let rekeyed = lock(&inner.state).ingest(&spec.link, msg);
                match rekeyed {
                    Some(Incoming::Screen(push)) => {
                        inner.emit(LinkMsg::Screen(push));
                    }
                    Some(Incoming::Event(ev)) => {
                        inner.emit(LinkMsg::Event(ev));
                    }
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

fn writer_main(
    mut w: Box<dyn Write + Send>,
    rx: &Receiver<Outgoing>,
    heartbeat: Duration,
    max_life: Duration,
) {
    let born = Instant::now();
    loop {
        match rx.recv_timeout(heartbeat) {
            Ok(m) => {
                let last = m == Outgoing::Detach;
                if write_outgoing(&mut w, &m).is_err() || last {
                    return;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if born.elapsed() >= max_life {
                    let _ = write_outgoing(&mut w, &Outgoing::Detach);
                    return;
                }
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
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use tmgr_ipc::api::{Attr, Color, Line, Span, State, StateKind};
    use tmgr_link::attach::{encode_closed, encode_event, encode_notice, encode_screen};
    use tmgr_link::proto::{read_frame, write_frame, MAX_FRAME};

    const A: &str = "s_aaaaaaaaaa";
    const B: &str = "s_bbbbbbbbbb";
    const C: &str = "s_cccccccccc";
    const LOCAL: &str = "s_localsess2";

    fn state(kind: StateKind) -> State {
        State {
            kind,
            since_ms: 0,
            detail: None,
            exit_code: None,
        }
    }

    fn run_json(id: &str) -> Value {
        json!({"id": id, "name": format!("run {id}"), "agent": "codex", "state": state(StateKind::Working)})
    }

    fn run_view(id: &str) -> RunView {
        serde_json::from_value(run_json(id)).unwrap()
    }

    fn push(session: &str) -> ScreenPush {
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

    fn framed(bytes: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        write_frame(&mut v, bytes, MAX_FRAME).unwrap();
        v
    }

    #[derive(Default)]
    struct FakeLink {
        dial: VecDeque<std::result::Result<(), DialErr>>,
        dials: Vec<Instant>,
        runs: Vec<Value>,
        list_err: Option<Kind>,
        hang: bool,
        halt_err: Option<Kind>,
        input: bool,
        calls: Vec<(String, Value)>,
        feeds: HashMap<String, Sender<Vec<u8>>>,
        written: HashMap<String, Arc<Mutex<Vec<u8>>>>,
    }

    #[derive(Default)]
    struct Fake(Mutex<HashMap<String, FakeLink>>);

    impl Fake {
        fn with<R>(&self, link: &str, f: impl FnOnce(&mut FakeLink) -> R) -> R {
            f(lock(&self.0).entry(link.to_string()).or_default())
        }

        fn set_runs(&self, link: &str, ids: &[&str]) {
            self.with(link, |l| l.runs = ids.iter().map(|i| run_json(i)).collect());
        }

        fn calls(&self, link: &str, method: &str) -> Vec<Value> {
            self.with(link, |l| {
                l.calls
                    .iter()
                    .filter(|(m, _)| m == method)
                    .map(|(_, p)| p.clone())
                    .collect()
            })
        }

        fn feed(&self, link: &str, run: &str, frame: &[u8]) {
            let tx = self.with(link, |l| l.feeds.get(run).cloned()).unwrap();
            tx.send(framed(frame)).unwrap();
        }

        fn hang_up(&self, link: &str, run: &str) {
            self.with(link, |l| l.feeds.remove(run));
        }

        fn written(&self, link: &str, run: &str) -> Vec<Outgoing> {
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

    struct FakeDialer(Arc<Fake>);

    impl Dialer for FakeDialer {
        fn dial(&self, link: &str) -> std::result::Result<Box<dyn LinkConn>, DialErr> {
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

        fn into_stream(self: Box<Self>) -> Result<Pair> {
            let (rx, sink) = self.pending.ok_or_else(|| Error::internal("no stream"))?;
            Ok((
                Box::new(ChanRead {
                    rx,
                    buf: Vec::new(),
                }),
                Box::new(Sink(sink)),
            ))
        }
    }

    fn fast() -> Timing {
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
            stream_renew: Duration::from_secs(3600),
            cooldown: Duration::from_millis(200),
            shutdown_wait: Duration::from_millis(500),
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
            LinkMsg::Screen(p) => Some(p.session.clone()),
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
            LinkMsg::Screen(_) | LinkMsg::Event(_) => panic!("forged data got through: {m:?}"),
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
        assert!(!after.iter().any(|m| matches!(m, LinkMsg::Screen(_))));
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
            ("idle", true),
            ("detached", true),
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
}
