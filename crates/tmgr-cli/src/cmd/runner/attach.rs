use super::{connect, rerr};
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::cursor::{Hide, Show};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::layout::Rect;
use ratatui::Terminal;
use serde_json::Value;
use std::collections::HashMap;
use std::io::{self, IsTerminal, Read, Write};
use std::panic;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender, SyncSender};
use std::thread;
use std::time::{Duration, Instant};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::{Modes, SessionInfo, Snapshot};
use tmgr_ipc::proto::ServerMsg;
use tmgr_runner::attach::{
    cursor, layout, mode_changes, render, reset_modes, Action, Cmd, InputSplitter, LayoutPrefs,
    ScreenSource, SessionRow, ViewState,
};
use tmgr_runner::client::{parse_event, AttachWriter, ScreenCache};
use tmgr_runner::daemon::RunnerConfig;
use tmgr_runner::sanitize::sanitize_text;
use tmgr_runner::wire::now_ms;

const TICK: Duration = Duration::from_millis(16);
const FRAME: Duration = Duration::from_millis(33);
const LIST_EVERY: Duration = Duration::from_secs(1);
const LAYOUT_DEBOUNCE: Duration = Duration::from_millis(300);
const NOTICE_FOR: Duration = Duration::from_secs(3);
const QUEUE: usize = 256;
const RPC_TIMEOUT: Duration = Duration::from_secs(5);
const CLEANUP_WAIT: Duration = Duration::from_secs(1);

static ACTIVE: AtomicBool = AtomicBool::new(false);
static SIGNAL: AtomicI32 = AtomicI32::new(0);

extern "C" fn on_signal(sig: libc::c_int) {
    SIGNAL.store(sig, Ordering::SeqCst);
}

pub(super) fn pending_signal() -> i32 {
    SIGNAL.load(Ordering::SeqCst)
}

fn restore() {
    if ACTIVE.swap(false, Ordering::SeqCst) {
        let mut out = io::stdout();
        let _ = out.write_all(&reset_modes());
        let _ = execute!(out, Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
        let _ = out.flush();
    }
}

pub(super) struct Guard;

impl Guard {
    pub(super) fn enter() -> io::Result<Guard> {
        enable_raw_mode()?;
        ACTIVE.store(true, Ordering::SeqCst);
        let guard = Guard;
        execute!(io::stdout(), EnterAlternateScreen, Hide)?;
        let previous = panic::take_hook();
        panic::set_hook(Box::new(move |info| {
            restore();
            previous(info);
        }));
        for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            unsafe { libc::signal(sig, on_signal as *const () as libc::sighandler_t) };
        }
        Ok(guard)
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        restore();
    }
}

enum Msg {
    Stdin(Vec<u8>),
    StdinClosed,
    Server(ServerMsg),
    Down(String),
    Sessions(Vec<SessionInfo>),
}

enum Exit {
    Detached,
    Signal(i32),
    Lost(String),
}

struct Screens<'a>(&'a ScreenCache);

impl ScreenSource for Screens<'_> {
    fn snapshot(&self, session: &str) -> Option<&Snapshot> {
        self.0.snapshot(session)
    }
}

fn short_age(now: u64, since: u64) -> String {
    let s = now.saturating_sub(since) / 1000;
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m", s / 60),
        3600..=86399 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86400),
    }
}

fn rows(list: &[SessionInfo]) -> Vec<SessionRow> {
    let now = now_ms();
    list.iter()
        .map(|s| SessionRow {
            id: s.id.clone(),
            name: s.name.clone(),
            state: s.state.clone(),
            age: short_age(now, s.state.since_ms),
        })
        .collect()
}

struct App {
    view: ViewState,
    cache: ScreenCache,
    splitter: InputSplitter,
    scroll_mode: bool,
    writer: AttachWriter,
    prefs_tx: Sender<Value>,
    dirty: bool,
    size: (u16, u16),
    sent_sizes: HashMap<String, (u16, u16)>,
    watched: Vec<String>,
    prev_modes: Option<Modes>,
    last_prefs: Option<LayoutPrefs>,
    scrolled: Option<(String, u32)>,
    notice_until: Option<Instant>,
}

impl App {
    fn on_msg(&mut self, msg: Msg) -> io::Result<Option<Exit>> {
        match msg {
            Msg::Stdin(bytes) => return self.on_stdin(&bytes),
            Msg::StdinClosed => return Ok(Some(Exit::Detached)),
            Msg::Down(why) => return Ok(Some(Exit::Lost(why))),
            Msg::Sessions(list) => {
                let new = rows(&list);
                if new != self.view.sessions {
                    for old in &self.view.sessions {
                        if !new.iter().any(|r| r.id == old.id) {
                            self.cache.remove(&old.id);
                        }
                    }
                    self.view.sessions_changed(new);
                    self.dirty = true;
                }
            }
            Msg::Server(ServerMsg::Closed { reason }) => return Ok(Some(Exit::Lost(reason))),
            Msg::Server(msg) => {
                if self.cache.apply(&msg).is_some() {
                    self.dirty = true;
                }
                if let Some(ev) = parse_event(&msg) {
                    if let Some(row) = self.view.sessions.iter_mut().find(|r| r.id == ev.session) {
                        row.age = short_age(now_ms(), ev.state.since_ms);
                        row.state = ev.state;
                        self.dirty = true;
                    }
                }
            }
        }
        Ok(None)
    }

    fn page(&self) -> usize {
        let area = Rect::new(0, 0, self.size.0, self.size.1);
        let lay = layout(area, &self.view);
        let focused = self.view.focused_session();
        lay.panes
            .iter()
            .find(|p| Some(p.session.as_str()) == focused)
            .map_or(1, |p| usize::from(p.inner.height.saturating_sub(1)).max(1))
    }

    fn sync_scroll_max(&mut self) {
        let max = self
            .view
            .focused_session()
            .and_then(|id| self.cache.snapshot(id))
            .map(|s| s.scroll_max);
        if let (Some(sc), Some(max)) = (self.view.scroll.as_mut(), max) {
            sc.max = max;
        }
    }

    fn notify(&mut self, text: &str) {
        self.view.notice = Some(text.to_string());
        self.notice_until = Some(Instant::now() + NOTICE_FOR);
    }

    fn on_stdin(&mut self, bytes: &[u8]) -> io::Result<Option<Exit>> {
        for action in self.splitter.feed(bytes) {
            self.dirty = true;
            match action {
                Action::Forward(data) => {
                    if std::mem::take(&mut self.view.help) {
                        continue;
                    }
                    if let Some(id) = self.view.focused_session() {
                        self.writer.input(id, &data)?;
                    }
                }
                Action::Cmd(Cmd::Detach) => return Ok(Some(Exit::Detached)),
                Action::Cmd(Cmd::Picker) => {
                    self.notify("switch sessions with Ctrl-] n and Ctrl-] p")
                }
                Action::Cmd(cmd) => {
                    self.view.apply(cmd);
                    if cmd == Cmd::ScrollMode {
                        self.scroll_mode = self.view.scroll.is_some();
                        self.splitter.set_scroll_mode(self.scroll_mode);
                    }
                    if let Some(n) = self.view.notice.as_ref() {
                        let n = n.clone();
                        self.notify(&n);
                    }
                }
                Action::Scroll(key) => {
                    self.sync_scroll_max();
                    let page = self.page();
                    self.view.scroll_key(key, page);
                }
            }
        }
        Ok(None)
    }

    fn tick(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
        last_draw: &mut Instant,
    ) -> io::Result<()> {
        if let Ok(size) = ratatui::crossterm::terminal::size() {
            if size != self.size {
                self.size = size;
                self.dirty = true;
            }
        }
        self.view.width = self.size.0;
        if self.notice_until.is_some_and(|t| Instant::now() >= t) {
            self.notice_until = None;
            self.view.notice = None;
            self.dirty = true;
        }
        if self.scroll_mode && self.view.scroll.is_none() {
            self.scroll_mode = false;
            self.splitter.set_scroll_mode(false);
        }
        self.sync_scroll_max();
        if self.view.panes != self.watched {
            self.writer.set_sessions(&self.view.panes)?;
            self.watched = self.view.panes.clone();
        }
        let lay = layout(Rect::new(0, 0, self.size.0, self.size.1), &self.view);
        for p in &lay.panes {
            let want = (p.inner.width, p.inner.height);
            if self.sent_sizes.get(&p.session) != Some(&want) {
                self.writer.resize(&p.session, want.0, want.1)?;
                self.sent_sizes.insert(p.session.clone(), want);
            }
        }
        self.sync_server_scroll()?;
        let modes = self
            .view
            .focused_session()
            .and_then(|id| self.cache.snapshot(id))
            .map(|s| s.modes);
        if let Some(modes) = modes {
            let bytes = mode_changes(self.prev_modes, modes);
            if !bytes.is_empty() {
                let out = terminal.backend_mut();
                out.write_all(&bytes)?;
                out.flush()?;
            }
            self.prev_modes = Some(modes);
        }
        self.save_prefs();
        if self.dirty && last_draw.elapsed() >= FRAME {
            terminal.draw(|f| {
                let screens = Screens(&self.cache);
                render(f, &self.view, &screens);
                let lay = layout(f.area(), &self.view);
                if let Some(pos) = cursor(&self.view, &lay, &screens) {
                    f.set_cursor_position(pos);
                }
            })?;
            self.dirty = false;
            *last_draw = Instant::now();
        }
        Ok(())
    }

    fn sync_server_scroll(&mut self) -> io::Result<()> {
        let desired = self
            .view
            .scroll
            .zip(self.view.focused_session())
            .map(|(s, id)| (id.to_string(), s.offset as u32));
        if desired == self.scrolled {
            return Ok(());
        }
        if let Some((old, _)) = &self.scrolled {
            if desired.as_ref().map(|d| &d.0) != Some(old) {
                self.writer.scroll(old, 0)?;
            }
        }
        if let Some((id, offset)) = &desired {
            self.writer.scroll(id, *offset)?;
        }
        self.scrolled = desired;
        Ok(())
    }

    fn save_prefs(&mut self) {
        let prefs = self.view.prefs();
        if self.last_prefs.as_ref() != Some(&prefs) {
            if let Ok(v) = serde_json::to_value(&prefs) {
                let _ = self.prefs_tx.send(v);
            }
            self.last_prefs = Some(prefs);
        }
    }
}

fn event_loop(
    app: &mut App,
    rx: &mpsc::Receiver<Msg>,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
) -> Exit {
    let mut last_draw = Instant::now() - FRAME;
    loop {
        let sig = SIGNAL.load(Ordering::SeqCst);
        if sig != 0 {
            return Exit::Signal(sig);
        }
        let mut batch = Vec::new();
        match rx.recv_timeout(TICK) {
            Ok(m) => batch.push(m),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Exit::Lost("internal error".into()),
        }
        while batch.len() < 256 {
            match rx.try_recv() {
                Ok(m) => batch.push(m),
                Err(_) => break,
            }
        }
        for msg in batch {
            match app.on_msg(msg) {
                Ok(None) => {}
                Ok(Some(exit)) => return exit,
                Err(e) => return Exit::Lost(e.to_string()),
            }
        }
        if let Err(e) = app.tick(terminal, &mut last_draw) {
            return Exit::Lost(e.to_string());
        }
    }
}

fn poller(
    mut ctl: tmgr_runner::client::Client,
    tx: SyncSender<Msg>,
    layouts: mpsc::Receiver<Value>,
) {
    let mut next_list = Instant::now() + LIST_EVERY;
    loop {
        match layouts.recv_timeout(next_list.saturating_duration_since(Instant::now())) {
            Ok(mut latest) => {
                while let Ok(newer) = layouts.recv_timeout(LAYOUT_DEBOUNCE) {
                    latest = newer;
                }
                let _ = ctl.layout_set(latest);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        if Instant::now() >= next_list {
            next_list = Instant::now() + LIST_EVERY;
            if let Ok(list) = ctl.list() {
                if tx.send(Msg::Sessions(list)).is_err() {
                    break;
                }
            }
        }
    }
}

pub fn run(cfg: &RunnerConfig, initial: &str) -> Result<i32> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(Error::usage("attach needs an interactive terminal"));
    }
    let mut ctl = connect(cfg)?;
    let status = ctl.status().map_err(rerr)?;
    let list = ctl.list().map_err(rerr)?;
    let mut view = ViewState::new(rows(&list), &format!("pid {}", status.pid));
    view.panes = vec![initial.to_string()];
    if let Some(prefs) = ctl
        .layout_get()
        .ok()
        .and_then(|v| serde_json::from_value::<LayoutPrefs>(v).ok())
    {
        view.apply_prefs(&prefs);
        if !view.panes.iter().any(|p| p == initial) {
            match view.panes.get_mut(view.focus) {
                Some(slot) => *slot = initial.to_string(),
                None => view.panes.push(initial.to_string()),
            }
        }
        view.focus = view.panes.iter().position(|p| p == initial).unwrap_or(0);
    }
    let (reader, writer) = connect(cfg)?.attach(&view.panes).map_err(rerr)?.split();
    let (tx, rx) = mpsc::sync_channel(QUEUE);
    let (prefs_tx, prefs_rx) = mpsc::channel();
    let guard = Guard::enter().map_err(|e| Error::internal(format!("terminal setup: {e}")))?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))
        .map_err(|e| Error::internal(format!("terminal setup: {e}")))?;
    let _ = terminal.clear();

    let stdin_tx = tx.clone();
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        let mut stdin = io::stdin().lock();
        loop {
            match stdin.read(&mut buf) {
                Ok(0) | Err(_) => {
                    let _ = stdin_tx.send(Msg::StdinClosed);
                    return;
                }
                Ok(n) => {
                    if stdin_tx.send(Msg::Stdin(buf[..n].to_vec())).is_err() {
                        return;
                    }
                }
            }
        }
    });
    let server_tx = tx.clone();
    thread::spawn(move || {
        let mut reader = reader;
        loop {
            let msg = match reader.recv() {
                Ok(Some(m)) => Msg::Server(m),
                Ok(None) => Msg::Down("the runner closed the connection".into()),
                Err(e) => Msg::Down(e.to_string()),
            };
            let stop = matches!(msg, Msg::Down(_));
            if server_tx.send(msg).is_err() || stop {
                return;
            }
        }
    });
    let _ = ctl.set_timeouts(RPC_TIMEOUT);
    let poll = thread::spawn(move || poller(ctl, tx, prefs_rx));

    let mut app = App {
        watched: view.panes.clone(),
        view,
        cache: ScreenCache::new(),
        splitter: InputSplitter::new(),
        scroll_mode: false,
        writer,
        prefs_tx,
        dirty: true,
        size: (0, 0),
        sent_sizes: HashMap::new(),
        prev_modes: None,
        last_prefs: None,
        scrolled: None,
        notice_until: None,
    };
    let exit = event_loop(&mut app, &rx, &mut terminal);
    app.save_prefs();
    let target = app
        .view
        .focused_row()
        .map(|r| (r.name.clone(), r.id.clone()));
    drop(terminal);
    drop(guard);
    drop(rx);
    let (done_tx, done_rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = app.writer.detach();
        drop(app);
        let _ = poll.join();
        let _ = done_tx.send(());
    });
    let _ = done_rx.recv_timeout(CLEANUP_WAIT);

    match exit {
        Exit::Detached => {
            let (name, id) = target.unwrap_or_else(|| (initial.to_string(), initial.to_string()));
            let (name, id) = (sanitize_text(&name), sanitize_text(&id));
            let bin = super::bin();
            let _ = writeln!(
                io::stdout(),
                "detached from {name} ({id}) — it keeps running; reattach with: {bin} attach {id}"
            );
            Ok(0)
        }
        Exit::Signal(sig) => Ok(128 + sig),
        Exit::Lost(why) => Err(Error::new(
            Kind::Offline,
            format!("lost the connection to the runner: {why}"),
        )
        .hint(format!("{} runner status", super::bin()))),
    }
}
