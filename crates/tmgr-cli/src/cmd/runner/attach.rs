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
use tmgr_ipc::api::{Modes, SessionInfo, Snapshot, StateKind};
use tmgr_ipc::proto::ServerMsg;
use tmgr_runner::attach::{
    cursor, enter_mouse, layout, mode_changes, render, reset_modes, Action, Cmd, Confirm, Control,
    InputSplitter, LayoutPrefs, PaneInfo, RenameEdit, ScreenSource, SessionRow, ViewState,
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
            esc: s.argv.iter().any(|a| {
                let n = a.rsplit('/').next().unwrap_or(a);
                n.contains("claude") || n.contains("codex")
            }),
        })
        .collect()
}

struct App {
    view: ViewState,
    mouse: bool,
    cache: ScreenCache,
    splitter: InputSplitter,
    scroll_mode: bool,
    writer: AttachWriter,
    prefs_tx: Sender<Value>,
    cfg: RunnerConfig,
    paste_to: Option<String>,
    last_interrupt: Option<(String, Instant)>,
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
        let actions = self.splitter.feed(bytes);
        self.handle(actions)
    }

    fn area(&self) -> Rect {
        Rect::new(0, 0, self.size.0, self.size.1)
    }

    fn leave_overlays(&mut self) {
        self.view.help = false;
        if self.view.scroll.take().is_some() {
            self.scroll_mode = false;
            self.splitter.set_scroll_mode(false);
        }
    }

    fn send_to(&mut self, session: &str, data: &[u8]) -> io::Result<()> {
        if data.is_empty() {
            return Ok(());
        }
        self.writer.input(session, data)
    }

    fn bracketed(&self, session: &str) -> bool {
        self.cache
            .snapshot(session)
            .is_some_and(|s| s.modes.bracketed_paste)
    }

    fn confirm_key(&mut self, data: &[u8]) {
        let Some(c) = self.view.confirm.take() else {
            return;
        };
        self.dirty = true;
        if matches!(data.first(), Some(b'y' | b'Y')) {
            self.stop_session(&c.session, c.remove);
            self.notify(&format!("stopping {}", c.name));
        }
    }

    fn control(&mut self, id: &str, ctl: Control) -> io::Result<()> {
        let Some(row) = self.view.sessions.iter().find(|r| r.id == id).cloned() else {
            return Ok(());
        };
        let exited = row.state.kind == StateKind::Exited;
        self.dirty = true;
        self.view.confirm = None;
        if ctl != Control::Interrupt {
            self.leave_overlays();
            self.view.rename = None;
        }
        match ctl {
            Control::Interrupt => {
                let recent = self
                    .last_interrupt
                    .as_ref()
                    .is_some_and(|(s, t)| s == id && t.elapsed() < Duration::from_secs(1));
                if row.can_interrupt() && !recent {
                    self.send_to(id, row.interrupt_bytes())?;
                    self.last_interrupt = Some((id.to_string(), Instant::now()));
                }
            }
            Control::Stop if exited => self.notify("already exited"),
            Control::Stop => {
                self.view.confirm = Some(Confirm {
                    session: id.to_string(),
                    name: row.name,
                    remove: false,
                })
            }
            Control::Close if exited => {
                self.view.remove_session(id);
                self.remove_session(id);
            }
            Control::Close => {
                self.view.confirm = Some(Confirm {
                    session: id.to_string(),
                    name: row.name,
                    remove: true,
                })
            }
        }
        Ok(())
    }

    fn stop_session(&self, id: &str, remove: bool) {
        let cfg = self.cfg.clone();
        let id = id.to_string();
        thread::spawn(move || {
            let Ok(mut c) = connect(&cfg) else { return };
            for (signal, wait) in [("int", 10), ("term", 5), ("kill", 3)] {
                let _ = c.kill(&id, signal);
                let end = Instant::now() + Duration::from_secs(wait);
                while Instant::now() < end {
                    let gone = c.list().ok().map(|l| {
                        l.iter()
                            .find(|s| s.id == id)
                            .is_none_or(|s| s.state.kind == StateKind::Exited)
                    });
                    if gone == Some(true) {
                        if remove {
                            let _ = c.remove(&id);
                        }
                        return;
                    }
                    thread::sleep(Duration::from_millis(200));
                }
            }
        });
    }

    fn edit_rename_text(&mut self, data: &[u8]) {
        let text: String = String::from_utf8_lossy(data)
            .replace("\x1b[200~", "")
            .replace("\x1b[201~", "")
            .chars()
            .filter(|c| !c.is_control())
            .collect();
        self.view.edit_rename(text.as_bytes());
    }

    fn rename_session(&mut self, id: &str, name: &str) {
        let result = connect(&self.cfg).and_then(|mut c| c.rename(id, name).map_err(rerr));
        match result {
            Ok(()) => self.view.rename_done(id, name),
            Err(e) => self.notify(&e.to_string()),
        }
    }

    fn remove_session(&self, id: &str) {
        let cfg = self.cfg.clone();
        let id = id.to_string();
        thread::spawn(move || {
            if let Ok(mut c) = connect(&cfg) {
                let _ = c.remove(&id);
            }
        });
    }

    fn handle(&mut self, actions: Vec<Action>) -> io::Result<Option<Exit>> {
        for action in actions {
            if !matches!(action, Action::Mouse(_)) {
                self.dirty = true;
            }
            match action {
                Action::Forward(data) if self.view.rename.is_some() => {
                    match self.view.edit_rename(&data) {
                        RenameEdit::Submit(id, name) => self.rename_session(&id, &name),
                        RenameEdit::Editing | RenameEdit::Cancel => {}
                    }
                }
                Action::Forward(data) => {
                    self.view.pointer = None;
                    if self.view.scroll.is_some() && !self.scroll_mode {
                        self.view.scroll = None;
                    }
                    if self.view.confirm.is_some() {
                        self.confirm_key(&data);
                        continue;
                    }
                    if std::mem::take(&mut self.view.help) {
                        continue;
                    }
                    if let Some(id) = self.view.focused_session().map(str::to_string) {
                        self.send_to(&id, &data)?;
                    }
                }
                Action::Drop(data) if self.view.rename.is_some() => {
                    self.edit_rename_text(&data);
                }
                Action::Drop(data) if !dropped_path_exists(&data) => {
                    self.handle(vec![Action::Forward(data)])?;
                }
                Action::Paste { data, start, end } if self.view.rename.is_some() => {
                    let _ = (start, end);
                    self.edit_rename_text(&data);
                }
                Action::Drop(data) => {
                    self.view.confirm = None;
                    self.leave_overlays();
                    let idx = self.view.paste_target(self.area());
                    self.view.focus_pane(idx);
                    if let Some(id) = self.view.focused_session().map(str::to_string) {
                        let mut out = Vec::new();
                        let wrap = self.bracketed(&id);
                        if wrap {
                            out.extend_from_slice(b"\x1b[200~");
                        }
                        out.extend_from_slice(&data);
                        if wrap {
                            out.extend_from_slice(b"\x1b[201~");
                        }
                        self.send_to(&id, &out)?;
                    }
                }
                Action::Paste { data, start, end } => {
                    if start {
                        self.view.confirm = None;
                        self.leave_overlays();
                        let idx = self.view.paste_target(self.area());
                        self.view.focus_pane(idx);
                        self.paste_to = self.view.focused_session().map(str::to_string);
                    }
                    let Some(id) = self.paste_to.clone() else {
                        continue;
                    };
                    let mut body: &[u8] = &data;
                    if !self.bracketed(&id) {
                        if start {
                            body = &body[body.len().min(6)..];
                        }
                        if end {
                            body = &body[..body.len().saturating_sub(6)];
                        }
                    }
                    self.send_to(&id, body)?;
                    if end {
                        self.paste_to = None;
                    }
                }
                Action::Mouse(ev) => {
                    if !self.mouse {
                        continue;
                    }
                    let area = self.area();
                    let cache = &self.cache;
                    let info = |id: &str| {
                        cache
                            .snapshot(id)
                            .map(|s| PaneInfo {
                                mouse: s.modes.mouse,
                                scroll_max: s.scroll_max,
                                alt_screen: s.modes.alt_screen,
                                app_cursor: s.modes.app_cursor,
                            })
                            .unwrap_or_default()
                    };
                    let out = self.view.on_mouse(area, ev, &info);
                    if out.redraw {
                        self.dirty = true;
                    }
                    if let Some((id, bytes)) = out.forward {
                        self.send_to(&id, &bytes)?;
                    }
                    if let Some((id, ctl)) = out.control {
                        self.control(&id, ctl)?;
                    }
                }
                Action::Cmd(Cmd::Detach) => return Ok(Some(Exit::Detached)),
                Action::Cmd(Cmd::Picker) => {
                    self.notify("switch sessions with Ctrl-] n and Ctrl-] p")
                }
                Action::Cmd(cmd) => {
                    self.view.pointer = None;
                    self.view.confirm = None;
                    self.view.rename = None;
                    let removing = (cmd == Cmd::Remove)
                        .then(|| self.view.focused_row())
                        .flatten()
                        .filter(|r| r.state.kind == StateKind::Exited)
                        .map(|r| r.id.clone());
                    self.view.apply(cmd);
                    if let Some(id) = removing {
                        self.remove_session(&id);
                    }
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
        let stale = self.splitter.flush_stale(Instant::now());
        if !stale.is_empty() {
            if let Some(exit) = self.handle(stale)? {
                let _ = exit;
            }
        }
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
        if let Some(modes) = modes.map(|m| Modes {
            bracketed_paste: true,
            ..m
        }) {
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
    run_with(cfg, initial, true)
}

pub fn run_with(cfg: &RunnerConfig, initial: &str, mouse: bool) -> Result<i32> {
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
    if mouse {
        let mut out = io::stdout();
        let _ = out.write_all(enter_mouse());
        let _ = out.flush();
    }
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
        mouse,
        cache: ScreenCache::new(),
        splitter: InputSplitter::new(),
        scroll_mode: false,
        writer,
        prefs_tx,
        cfg: cfg.clone(),
        paste_to: None,
        last_interrupt: None,
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

fn dropped_path_exists(data: &[u8]) -> bool {
    let text = String::from_utf8_lossy(data);
    let mut path = String::new();
    let mut chars = text.trim_end().chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(n) = chars.next() {
                path.push(n);
            }
        } else {
            path.push(c);
        }
    }
    let path = match path.strip_prefix("~/") {
        Some(rest) => match std::env::var_os("HOME") {
            Some(home) => std::path::Path::new(&home).join(rest),
            None => return false,
        },
        None => std::path::PathBuf::from(path),
    };
    path.exists()
}
