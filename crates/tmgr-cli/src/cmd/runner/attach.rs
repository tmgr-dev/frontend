use super::attach_new::{
    default_folder, dialog_for, installed_kinds, kind_label, place_session, registers_shell, route,
    spawn_size, start_session, NewFlow, Route, SpawnErr,
};
use super::attach_shell::{
    can_split, cwd_candidates, focus_new_pane, is_shell_session, ladder_for, merge_pending,
    process_cwd, push_typed, reap_ids, shell_argv, shell_name, size_hint, watch_ids, Ladder,
    KILL_ONLY,
};
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
use std::collections::{HashMap, HashSet};
use std::io::{self, IsTerminal, Read, Write};
use std::panic;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender, SyncSender};
use std::thread;
use std::time::{Duration, Instant};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::{caps, CreateReq, Modes, RestoredRun, SessionInfo, Snapshot, StateKind};
use tmgr_ipc::proto::ServerMsg;
use tmgr_runner::attach::{
    cursor, enter_mouse, layout, mode_changes, probe, render, reset_modes, Action, Cmd, Confirm,
    Control, InputSplitter, LayoutPrefs, Loc, NewEdit, NewRequest, NewSession, PaletteEdit,
    PaletteEntry, PaneInfo, Place, RenameEdit, ScreenSource, SessionRow, SidebarEdit, SplitDir,
    ViewState,
};
use tmgr_runner::client::{parse_event, AttachWriter, ScreenCache};
use tmgr_runner::daemon::RunnerConfig;
use tmgr_runner::sanitize::sanitize_text;
use tmgr_runner::wire::now_ms;

const TICK: Duration = Duration::from_millis(16);
const FRAME: Duration = Duration::from_millis(33);
const LIST_EVERY: Duration = Duration::from_secs(1);
const PROBE_EVERY: Duration = Duration::from_millis(2500);
const PROBE_RETRY: Duration = Duration::from_millis(250);
const LAYOUT_DEBOUNCE: Duration = Duration::from_millis(300);
const NOTICE_FOR: Duration = Duration::from_secs(3);
const QUEUE: usize = 256;
const RPC_TIMEOUT: Duration = Duration::from_secs(5);
const CLEANUP_WAIT: Duration = Duration::from_secs(1);
const CREATE_WAIT: Duration = Duration::from_secs(2);
const RESUME_WAIT: Duration = Duration::from_secs(30);
const SHELL_CLOSED: &str = "shell closed before it started";

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

#[allow(clippy::large_enum_variant)]
enum Msg {
    Stdin(Vec<u8>),
    StdinClosed,
    Server(ServerMsg),
    Down(String),
    Sessions(Vec<SessionInfo>),
    Locs(Vec<(String, Loc)>),
    Created {
        result: std::result::Result<SessionInfo, String>,
        dir: SplitDir,
        target: String,
    },
    Spawned {
        id: u64,
        result: std::result::Result<SessionInfo, SpawnErr>,
        req: NewRequest,
        target: String,
    },
    Resumed {
        old: String,
        result: std::result::Result<RestoredRun, String>,
    },
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
            resumed_from: s.resumed_from.clone(),
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
    tx: SyncSender<Msg>,
    starts: HashMap<String, (String, u32)>,
    pending: HashMap<String, Instant>,
    creating: HashMap<String, String>,
    new_flow: NewFlow,
    typed: Option<(String, Vec<u8>)>,
    removing: HashSet<String>,
    stopped: HashSet<String>,
    resuming: HashMap<String, Instant>,
    shell_ids: HashSet<String>,
    paste_to: Option<String>,
    last_interrupt: Option<(String, Instant)>,
    dirty: bool,
    size: (u16, u16),
    sent_sizes: HashMap<String, (u16, u16)>,
    watched: Option<Vec<String>>,
    prev_modes: Option<Modes>,
    last_prefs: Option<LayoutPrefs>,
    scrolled: Option<(String, u32)>,
    notice_until: Option<Instant>,
    probe_tx: SyncSender<Vec<ProbeJob>>,
    probe_pending: bool,
    last_probe: Instant,
}

type ProbeJob = (String, u32, String);

impl App {
    fn on_msg(&mut self, msg: Msg) -> io::Result<Option<Exit>> {
        match msg {
            Msg::Stdin(bytes) => return self.on_stdin(&bytes),
            Msg::StdinClosed => return Ok(Some(Exit::Detached)),
            Msg::Down(why) => return Ok(Some(Exit::Lost(why))),
            Msg::Sessions(list) => self.on_sessions(&list),
            Msg::Locs(found) => {
                self.probe_pending = false;
                if self.view.set_locs(self.area(), found) {
                    self.dirty = true;
                }
            }
            Msg::Created {
                result,
                dir,
                target,
            } => self.on_created(result, dir, &target),
            Msg::Spawned {
                id,
                result,
                req,
                target,
            } => self.on_spawned(id, result, &req, &target),
            Msg::Resumed { old, result } => self.on_resumed(&old, result),
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

    fn on_sessions(&mut self, list: &[SessionInfo]) {
        let new = merge_pending(
            rows(list),
            &self.view.sessions,
            &mut self.pending,
            Instant::now(),
        );
        self.starts
            .retain(|id, _| list.iter().any(|s| &s.id == id) || self.pending.contains_key(id));
        for s in list {
            self.starts
                .insert(s.id.clone(), (s.cwd.clone(), s.child_pid));
        }
        self.shell_ids = list
            .iter()
            .filter(|s| is_shell_session(&s.argv, &s.name))
            .map(|s| s.id.clone())
            .collect();
        self.removing.retain(|id| new.iter().any(|r| &r.id == id));
        self.stopped.retain(|id| new.iter().any(|r| &r.id == id));
        if new == self.view.sessions {
            return;
        }
        for old in &self.view.sessions {
            if !new.iter().any(|r| r.id == old.id) {
                self.cache.remove(&old.id);
            }
        }
        let before = self.view.panes().to_vec();
        self.view.sessions_changed(self.area(), new);
        let fresh: Vec<(String, Loc)> = list
            .iter()
            .filter(|s| !s.cwd.is_empty() && !self.view.locs.contains_key(&s.id))
            .map(|s| (s.id.clone(), probe(Path::new(&s.cwd))))
            .collect();
        self.view.set_locs(self.area(), fresh);
        if self.view.panes() != before {
            self.watched = None;
        }
        self.dirty = true;
    }

    fn probe_places(&mut self) {
        if self.probe_pending || !self.view.sidebar_shown() {
            return;
        }
        let jobs: Vec<ProbeJob> = self
            .view
            .visible_sessions(self.area())
            .into_iter()
            .filter_map(|id| {
                let row = self.view.sessions.iter().find(|r| r.id == id)?;
                let known = self.view.locs.contains_key(&id);
                if row.exited() && known {
                    return None;
                }
                let (start, pid) = self.starts.get(&id).cloned().unwrap_or_default();
                let pid = if row.exited() { 0 } else { pid };
                Some((id, pid, start))
            })
            .collect();
        let missing = jobs.iter().any(|(id, ..)| !self.view.locs.contains_key(id));
        let every = if missing { PROBE_RETRY } else { PROBE_EVERY };
        if jobs.is_empty() || self.last_probe.elapsed() < every {
            return;
        }
        self.last_probe = Instant::now();
        self.probe_pending = self.probe_tx.try_send(jobs).is_ok();
    }

    fn is_shell(&self, id: &str) -> bool {
        self.view.shells.iter().any(|s| s == id) || self.shell_ids.contains(id)
    }

    fn request_split(&mut self, dir: SplitDir) {
        let Some(target) = self.view.focused_session().map(str::to_string) else {
            return;
        };
        if self.creating.contains_key(&target) {
            return;
        }
        if !can_split(self.view.panes().len()) {
            self.notify("no room for another pane");
            return;
        }
        let (start, pid) = match self.starts.get(&target) {
            Some((cwd, pid)) => (Some(cwd.clone()), *pid),
            None => (None, 0),
        };
        let mut cwds = cwd_candidates(process_cwd(pid), start.as_deref());
        if cwds.is_empty() {
            cwds.extend(
                std::env::current_dir()
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned()),
            );
        }
        let name = shell_name(
            &self.view.sessions,
            &self.creating,
            self.new_flow.planned_name(),
        );
        let content = layout(self.area(), &self.view)
            .panes
            .iter()
            .find(|p| p.session == target)
            .map(|p| p.content())
            .unwrap_or_default();
        let (cols, rows) = size_hint(content, dir);
        let cfg = self.cfg.clone();
        let tx = self.tx.clone();
        self.creating.insert(target.clone(), name.clone());
        self.notify("starting shell…");
        thread::spawn(move || {
            let result = create_shell(&cfg, &cwds, &name, cols, rows);
            let sent = tx.send(Msg::Created {
                result,
                dir,
                target,
            });
            if let Err(mpsc::SendError(Msg::Created {
                result: Ok(info), ..
            })) = sent
            {
                run_ladder(&cfg, &info.id, true, KILL_ONLY);
            }
        });
    }

    fn take_typed(&mut self, target: &str) -> Vec<u8> {
        match self.typed.take() {
            Some((t, buf)) if t == target => buf,
            other => {
                self.typed = other;
                Vec::new()
            }
        }
    }

    fn hold_typed(&mut self, target: &str, data: &[u8]) {
        let entry = self
            .typed
            .get_or_insert_with(|| (target.to_string(), Vec::new()));
        if entry.0 != target {
            *entry = (target.to_string(), Vec::new());
        }
        push_typed(&mut entry.1, data);
        self.notify("starting shell...");
    }

    fn on_created(
        &mut self,
        result: std::result::Result<SessionInfo, String>,
        dir: SplitDir,
        target: &str,
    ) {
        self.creating.remove(target);
        self.dirty = true;
        let typed = self.take_typed(target);
        let lost = |typed: &[u8], default: &str| {
            if typed.is_empty() {
                default.to_string()
            } else {
                SHELL_CLOSED.to_string()
            }
        };
        let info = match result {
            Ok(info) => info,
            Err(e) => return self.notify(&lost(&typed, &e)),
        };
        if !self.view.panes().iter().any(|p| p == target) {
            self.discard_session(&info.id);
            return self.notify(&lost(&typed, "pane closed before the shell started"));
        }
        if !self.view.sessions.iter().any(|r| r.id == info.id) {
            self.view.sessions.extend(rows(std::slice::from_ref(&info)));
        }
        self.pending.insert(info.id.clone(), Instant::now());
        self.starts
            .insert(info.id.clone(), (info.cwd.clone(), info.child_pid));
        let focus_new = focus_new_pane(self.view.focused_session(), target);
        if !self.view.split_at(target, dir, &info.id, focus_new) {
            self.pending.remove(&info.id);
            self.view.sessions.retain(|r| r.id != info.id);
            self.discard_session(&info.id);
            return self.notify(&lost(&typed, "no room for another pane"));
        }
        if !typed.is_empty() {
            let _ = self.writer.input(&info.id, &typed);
        }
        self.view.shells.push(info.id.clone());
        self.shell_ids.insert(info.id);
    }

    fn open_new_session(&mut self) {
        self.dirty = true;
        if self.new_flow.busy(Instant::now()) {
            return self.notify("a session is still starting");
        }
        self.leave_overlays();
        self.view.confirm = None;
        self.view.rename = None;
        if let Some(mut saved) = self.new_flow.take_failed() {
            saved.taken_names = self.view.sessions.iter().map(|s| s.name.clone()).collect();
            self.view.reopen_new_session(saved);
            return;
        }
        let home = std::env::var_os("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| "/".into());
        let (start, pid) = match self
            .view
            .focused_session()
            .and_then(|id| self.starts.get(id))
        {
            Some((cwd, pid)) => (Some(cwd.as_str()), *pid),
            None => (None, 0),
        };
        let folder = default_folder(process_cwd(pid), start, std::env::current_dir().ok(), &home);
        self.view.open_new_session(folder, installed_kinds(), home);
    }

    fn begin_spawn(&mut self, before: Option<NewSession>, req: NewRequest) {
        let Some(base) = before else { return };
        let target = self
            .view
            .focused_session()
            .map(str::to_string)
            .unwrap_or_default();
        let content = layout(self.area(), &self.view)
            .panes
            .iter()
            .find(|p| p.session == target)
            .map(|p| p.content())
            .unwrap_or_default();
        let (cols, rows) = spawn_size(content, req.place);
        let mut run_req = req.clone();
        let planned = registers_shell(&req).then(|| {
            shell_name(
                &self.view.sessions,
                &self.creating,
                self.new_flow.planned_name(),
            )
        });
        if planned.is_some() {
            run_req.name = planned.clone();
        }
        self.notify(&format!("starting {}...", kind_label(&req.kind)));
        let id = self
            .new_flow
            .begin(dialog_for(&base, &req), planned, Instant::now());
        let cfg = self.cfg.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = start_session(&cfg, &run_req, cols, rows);
            let _ = tx.send(Msg::Spawned {
                id,
                result,
                req,
                target,
            });
        });
    }

    fn modal_input(&mut self, route: Route, action: Action) {
        match (route, action) {
            (Route::Dialog, Action::Forward(data)) => {
                let before = self.view.new_session.clone();
                if let NewEdit::Submit(req) = self.view.edit_new_session(&data) {
                    self.begin_spawn(before, req);
                }
            }
            (Route::Dialog, Action::Drop(data) | Action::Paste { data, .. }) => {
                self.view.insert_new_session_text(&data);
            }
            (Route::Sidebar, Action::Forward(data)) => {
                let area = self.area();
                let edit = self.view.edit_sidebar(area, &data);
                if edit == SidebarEdit::New {
                    self.open_new_session();
                }
            }
            _ => {}
        }
    }

    fn on_spawned(
        &mut self,
        id: u64,
        result: std::result::Result<SessionInfo, SpawnErr>,
        req: &NewRequest,
        target: &str,
    ) {
        self.dirty = true;
        let dialog = self.new_flow.finish(id);
        let info = match result {
            Ok(info) => info,
            Err(e) => {
                let label = kind_label(&req.kind);
                match dialog {
                    Some(d) => {
                        self.new_flow.fail(d, e);
                        self.notify(&format!("could not start {label}: Ctrl-] c shows why"));
                    }
                    None => {
                        let why = sanitize_text(&e.message).into_owned();
                        self.notify(&format!("could not start {label}: {why}"));
                    }
                }
                return;
            }
        };
        if !self.view.sessions.iter().any(|r| r.id == info.id) {
            self.view.sessions.extend(rows(std::slice::from_ref(&info)));
        }
        self.pending.insert(info.id.clone(), Instant::now());
        self.starts
            .insert(info.id.clone(), (info.cwd.clone(), info.child_pid));
        let split = place_session(&mut self.view, target, req.place, &info.id);
        if registers_shell(req) {
            self.view.shells.push(info.id.clone());
            self.shell_ids.insert(info.id.clone());
        }
        if let Some(d) = dialog {
            if let Some(kind) = d.kinds.get(d.kind) {
                self.view.remember_new_session(req, &kind.id);
            }
        }
        if req.place != Place::Replace && !split {
            self.notify("no room for another pane, replaced the focused one");
        } else {
            self.notify(&format!("started {}", sanitize_text(&info.name)));
        }
    }

    fn reap_shells(&mut self) {
        let ids = reap_ids(
            &self.view.sessions,
            |id| self.is_shell(id),
            &self.removing,
            &self.stopped,
        );
        for id in ids {
            self.view.close_pane(&id);
            self.removing.insert(id.clone());
            self.remove_session(&id);
            self.dirty = true;
        }
    }

    fn request_resume(&mut self, id: &str) {
        let row = self.view.sessions.iter().find(|r| r.id == id);
        match resume_step(row, &self.resuming) {
            ResumeStep::Ignore => {}
            ResumeStep::Notice(text) => self.notify(text),
            ResumeStep::Start => {
                let (cols, rows) = layout(self.area(), &self.view)
                    .panes
                    .iter()
                    .find(|p| p.session == id)
                    .map(|p| (p.inner.width, p.inner.height))
                    .or_else(|| self.sent_sizes.get(id).copied())
                    .unwrap_or((80, 24));
                let old = id.to_string();
                self.resuming.insert(old.clone(), Instant::now());
                self.notify("resuming…");
                let cfg = self.cfg.clone();
                let tx = self.tx.clone();
                thread::spawn(move || {
                    let result = resume_call(&cfg, &old, cols, rows);
                    let _ = tx.send(Msg::Resumed { old, result });
                });
            }
        }
    }

    fn on_resumed(&mut self, old: &str, result: std::result::Result<RestoredRun, String>) {
        self.resuming.remove(old);
        self.dirty = true;
        let run = match result {
            Ok(run) => run,
            Err(e) => return self.notify(&e),
        };
        let info = run.session;
        let new = info.id.clone();
        if !self.view.sessions.iter().any(|r| r.id == new) {
            let at = self.view.sessions.iter().position(|r| r.id == old);
            let row = rows(std::slice::from_ref(&info)).remove(0);
            match at {
                Some(i) => self.view.sessions.insert(i, row),
                None => self.view.sessions.push(row),
            }
        }
        self.pending.insert(new.clone(), Instant::now());
        self.starts.remove(old);
        self.starts
            .insert(new.clone(), (info.cwd.clone(), info.child_pid));
        self.view.replace_session(old, &new);
        self.cache.remove(old);
        if let Some(size) = self.sent_sizes.remove(old) {
            self.sent_sizes.insert(new.clone(), size);
        }
        self.stopped.remove(old);
        self.watched = None;
        self.notify(&resumed_notice(&info.name, run.via.as_deref()));
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
        self.view.palette = None;
        self.view.new_session = None;
        self.view.sidebar_focus = None;
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
        if ctl == Control::Resume && just_exited(&row, now_ms()) {
            return Ok(());
        }
        self.dirty = true;
        self.view.confirm = None;
        if blocked_by_resume(&self.resuming, id, ctl) {
            self.notify("resuming…");
            return Ok(());
        }
        if ctl != Control::Interrupt {
            self.leave_overlays();
            self.view.rename = None;
        }
        match ctl {
            Control::Resume => self.request_resume(id),
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

    fn stop_session(&mut self, id: &str, remove: bool) {
        self.stopped.insert(id.to_string());
        self.spawn_ladder(id, remove, ladder_for(true));
    }

    fn discard_session(&self, id: &str) {
        self.spawn_ladder(id, true, ladder_for(false));
    }

    fn spawn_ladder(&self, id: &str, remove: bool, ladder: Ladder) {
        let cfg = self.cfg.clone();
        let id = id.to_string();
        thread::spawn(move || run_ladder(&cfg, &id, remove, ladder));
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

    fn run_palette_entry(&mut self, entry: PaletteEntry) -> io::Result<Option<Exit>> {
        self.dirty = true;
        match entry {
            PaletteEntry::Cmd(cmd) => self.handle(vec![Action::Cmd(cmd)]),
            PaletteEntry::Session(id) => {
                self.view.select(&id);
                Ok(None)
            }
        }
    }

    fn handle(&mut self, actions: Vec<Action>) -> io::Result<Option<Exit>> {
        for action in actions {
            if !matches!(action, Action::Mouse(_)) {
                self.dirty = true;
            }
            let routed = route(&self.view, &action);
            if routed != Route::Other {
                self.modal_input(routed, action);
                continue;
            }
            match action {
                Action::Forward(data) if self.view.palette.is_some() => {
                    if let PaletteEdit::Run(entry) = self.view.edit_palette(&data) {
                        if let Some(exit) = self.run_palette_entry(entry)? {
                            return Ok(Some(exit));
                        }
                    }
                }
                Action::Drop(data) if self.view.palette.is_some() => {
                    self.view.insert_palette_text(&data);
                }
                Action::Paste { data, .. } if self.view.palette.is_some() => {
                    self.view.insert_palette_text(&data);
                }
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
                        if self.creating.contains_key(&id) {
                            self.hold_typed(&id, &data);
                        } else {
                            self.send_to(&id, &data)?;
                        }
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
                    let before = self.view.new_session.clone();
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
                    if let Some(entry) = out.palette {
                        if let Some(exit) = self.run_palette_entry(entry)? {
                            return Ok(Some(exit));
                        }
                    }
                    if let Some(NewEdit::Submit(req)) = out.new_edit {
                        self.begin_spawn(before, req);
                    }
                    if out.new_session {
                        self.open_new_session();
                    }
                }
                Action::Cmd(Cmd::Detach) => return Ok(Some(Exit::Detached)),
                Action::Cmd(Cmd::Picker) => {
                    self.view.palette = None;
                    self.notify("switch sessions with Ctrl-] n and Ctrl-] p")
                }
                Action::Cmd(Cmd::NewSession) => {
                    self.view.pointer = None;
                    if self.view.new_session.take().is_none() {
                        self.open_new_session();
                    }
                }
                Action::Cmd(cmd) => {
                    self.view.pointer = None;
                    self.view.confirm = None;
                    self.view.rename = None;
                    self.view.new_session = None;
                    self.view.sidebar_focus = None;
                    if self.view.palette.take().is_some() && cmd == Cmd::Palette {
                        continue;
                    }
                    let focused = self.view.focused_session().map(str::to_string);
                    if let (Some(ctl), Some(id)) = (control_of(cmd), focused) {
                        if blocked_by_resume(&self.resuming, &id, ctl) {
                            self.notify("resuming…");
                            continue;
                        }
                    }
                    let removing = (cmd == Cmd::Remove)
                        .then(|| self.view.focused_row())
                        .flatten()
                        .filter(|r| r.state.kind == StateKind::Exited)
                        .map(|r| r.id.clone());
                    match cmd {
                        Cmd::SplitRight => self.request_split(SplitDir::Right),
                        Cmd::SplitDown => self.request_split(SplitDir::Down),
                        Cmd::Focus(nav) => {
                            self.view.focus_nav(self.area(), nav);
                        }
                        Cmd::Resize(nav) => self.view.resize_focused(self.area(), nav),
                        Cmd::Resume => {
                            if let Some(id) = self.view.focused_session().map(str::to_string) {
                                self.request_resume(&id);
                            }
                        }
                        _ => self.view.apply(cmd),
                    }
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
        if !self.view.sidebar_shown() {
            self.view.sidebar_focus = None;
        }
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
        self.reap_shells();
        self.probe_places();
        if self.scroll_mode && self.view.scroll.is_none() {
            self.scroll_mode = false;
            self.splitter.set_scroll_mode(false);
        }
        self.sync_scroll_max();
        let want = watch_ids(self.view.panes(), &self.view.sessions);
        if self.watched.as_ref() != Some(&want) {
            self.writer.set_sessions(&want)?;
            self.watched = Some(want);
        }
        let panes = self.view.panes();
        self.sent_sizes.retain(|id, _| panes.contains(id));
        if self.paste_to.as_ref().is_some_and(|id| !panes.contains(id)) {
            self.paste_to = None;
        }
        if self
            .last_interrupt
            .as_ref()
            .is_some_and(|(id, _)| !panes.contains(id))
        {
            self.last_interrupt = None;
        }
        let lay = layout(Rect::new(0, 0, self.size.0, self.size.1), &self.view);
        for p in &lay.panes {
            if p.content().is_empty() {
                continue;
            }
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

fn prober(jobs: mpsc::Receiver<Vec<ProbeJob>>, tx: SyncSender<Msg>) {
    for batch in jobs {
        let found = batch
            .into_iter()
            .filter_map(|(id, pid, start)| {
                let live = process_cwd(pid)
                    .map(|p| p.to_string_lossy().into_owned())
                    .filter(|p| !p.is_empty());
                let cwd = live.or(Some(start).filter(|s| !s.is_empty()))?;
                Some((id, probe(Path::new(&cwd))))
            })
            .collect();
        if tx.send(Msg::Locs(found)).is_err() {
            return;
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
    if let Ok((cols, _)) = ratatui::crossterm::terminal::size() {
        view.width = cols;
    }
    if let Some(prefs) = ctl
        .layout_get()
        .ok()
        .and_then(|v| serde_json::from_value::<LayoutPrefs>(v).ok())
    {
        view.apply_prefs(&prefs);
    }
    view.set_initial(initial);
    view.grouped = true;
    let seeded = list
        .iter()
        .filter(|s| !s.cwd.is_empty())
        .map(|s| (s.id.clone(), probe(Path::new(&s.cwd))))
        .collect();
    let first = ratatui::crossterm::terminal::size().unwrap_or((view.width, 24));
    view.set_locs(Rect::new(0, 0, first.0, first.1), seeded);
    let (reader, writer) = connect(cfg)?.attach(view.panes()).map_err(rerr)?.split();
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
    let app_tx = tx.clone();
    let (probe_tx, probe_rx) = mpsc::sync_channel(1);
    let probe_out = tx.clone();
    thread::spawn(move || prober(probe_rx, probe_out));
    let poll = thread::spawn(move || poller(ctl, tx, prefs_rx));

    let mut app = App {
        watched: Some(view.panes().to_vec()),
        shell_ids: list
            .iter()
            .filter(|s| is_shell_session(&s.argv, &s.name))
            .map(|s| s.id.clone())
            .collect(),
        starts: list
            .iter()
            .map(|s| (s.id.clone(), (s.cwd.clone(), s.child_pid)))
            .collect(),
        view,
        mouse,
        cache: ScreenCache::new(),
        splitter: InputSplitter::new(),
        scroll_mode: false,
        writer,
        prefs_tx,
        cfg: cfg.clone(),
        tx: app_tx,
        pending: HashMap::new(),
        creating: HashMap::new(),
        new_flow: NewFlow::default(),
        typed: None,
        removing: HashSet::new(),
        stopped: HashSet::new(),
        resuming: HashMap::new(),
        paste_to: None,
        last_interrupt: None,
        dirty: true,
        size: (0, 0),
        sent_sizes: HashMap::new(),
        prev_modes: None,
        last_prefs: None,
        scrolled: None,
        notice_until: None,
        probe_tx,
        probe_pending: false,
        last_probe: Instant::now(),
    };
    let exit = event_loop(&mut app, &rx, &mut terminal);
    app.save_prefs();
    let cfg_now = app.cfg.clone();
    let mut spawning = app.new_flow.is_spawning();
    drain_creates(&mut app.creating, &mut spawning, &rx, CREATE_WAIT, |info| {
        run_ladder(&cfg_now, &info.id, true, KILL_ONLY)
    });
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

fn run_ladder(cfg: &RunnerConfig, id: &str, remove: bool, ladder: Ladder) {
    let Ok(mut c) = connect(cfg) else { return };
    for (signal, wait) in ladder {
        let _ = c.kill(id, signal);
        let end = Instant::now() + Duration::from_secs(*wait);
        while Instant::now() < end {
            let gone = c.list().ok().map(|l| {
                l.iter()
                    .find(|s| s.id == id)
                    .is_none_or(|s| s.state.kind == StateKind::Exited)
            });
            if gone == Some(true) {
                if remove {
                    let _ = c.remove(id);
                }
                return;
            }
            thread::sleep(Duration::from_millis(200));
        }
    }
}

fn drain_creates(
    creating: &mut HashMap<String, String>,
    spawning: &mut bool,
    rx: &mpsc::Receiver<Msg>,
    wait: Duration,
    mut discard: impl FnMut(SessionInfo),
) {
    let end = Instant::now() + wait;
    while !creating.is_empty() || *spawning {
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return;
        }
        match rx.recv_timeout(left) {
            Ok(Msg::Created { result, target, .. }) => {
                creating.remove(&target);
                if let Ok(info) = result {
                    discard(info);
                }
            }
            Ok(Msg::Spawned { result, req, .. }) => {
                *spawning = false;
                if let (Ok(info), true) = (result, registers_shell(&req)) {
                    discard(info);
                }
            }
            Ok(_) => {}
            Err(_) => return,
        }
    }
}

fn create_shell(
    cfg: &RunnerConfig,
    cwds: &[String],
    name: &str,
    cols: u16,
    rows: u16,
) -> std::result::Result<SessionInfo, String> {
    let mut client = connect(cfg).map_err(|e| sanitize_text(&e.to_string()).into_owned())?;
    let mut last = "no working directory for the new shell".to_string();
    for cwd in cwds {
        let req = CreateReq {
            argv: shell_argv(),
            cwd: cwd.clone(),
            name: Some(name.to_string()),
            profile: None,
            cols,
            rows,
            env_pass: Vec::new(),
            task: None,
        };
        match client.create(req) {
            Ok(info) => return Ok(info),
            Err(e) => last = sanitize_text(&rerr(e).to_string()).into_owned(),
        }
    }
    Err(last)
}

#[derive(Debug, PartialEq, Eq)]
enum ResumeStep {
    Start,
    Ignore,
    Notice(&'static str),
}

fn resume_step(row: Option<&SessionRow>, resuming: &HashMap<String, Instant>) -> ResumeStep {
    match row {
        None => ResumeStep::Ignore,
        Some(r) if resuming.contains_key(&r.id) => ResumeStep::Ignore,
        Some(r) if !r.exited() => ResumeStep::Notice("only an exited session can be resumed"),
        Some(_) => ResumeStep::Start,
    }
}

const CLICK_GRACE_MS: u64 = 500;

fn just_exited(row: &SessionRow, now: u64) -> bool {
    row.exited() && now.saturating_sub(row.state.since_ms) < CLICK_GRACE_MS
}

fn control_of(cmd: Cmd) -> Option<Control> {
    match cmd {
        Cmd::Stop => Some(Control::Stop),
        Cmd::Remove => Some(Control::Close),
        _ => None,
    }
}

fn blocked_by_resume(resuming: &HashMap<String, Instant>, id: &str, ctl: Control) -> bool {
    matches!(ctl, Control::Stop | Control::Close) && resuming.contains_key(id)
}

fn resumed_notice(name: &str, via: Option<&str>) -> String {
    match via {
        Some("fresh") => format!("resumed {name} (fresh)"),
        Some("rerun") => format!("resumed {name} (rerun)"),
        _ => format!("resumed {name}"),
    }
}

fn resume_call(
    cfg: &RunnerConfig,
    id: &str,
    cols: u16,
    rows: u16,
) -> std::result::Result<RestoredRun, String> {
    let clean = |s: String| sanitize_text(&s).into_owned();
    let mut client = connect(cfg).map_err(|e| clean(e.to_string()))?;
    if !client
        .welcome()
        .caps
        .iter()
        .any(|c| c == caps::SESSIONS_RESUME_V1)
    {
        return Err("this runner cannot resume sessions yet, restart it".into());
    }
    let _ = client.set_timeouts(RESUME_WAIT);
    client
        .resume(id, cols, rows)
        .map_err(|e| clean(rerr(e).to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use tmgr_ipc::api::State;
    use tmgr_runner::attach::SpawnKind;

    fn info(id: &str) -> SessionInfo {
        SessionInfo {
            id: id.into(),
            name: id.into(),
            argv: Vec::new(),
            cwd: String::new(),
            profile: None,
            state: State {
                kind: StateKind::Working,
                since_ms: 0,
                detail: None,
                exit_code: None,
            },
            created_ms: 0,
            cols: 80,
            rows: 24,
            shim_pid: 0,
            child_pid: 0,
            task: None,
            bundle_sha256: None,
            origin: None,
            permission_mode: None,
            resumed_from: None,
        }
    }

    fn created(id: &str, target: &str) -> Msg {
        Msg::Created {
            result: Ok(info(id)),
            dir: SplitDir::Right,
            target: target.into(),
        }
    }

    fn session_row(kind: StateKind) -> SessionRow {
        rows(&[SessionInfo {
            state: State {
                kind,
                since_ms: 0,
                detail: None,
                exit_code: None,
            },
            ..info("s1")
        }])
        .remove(0)
    }

    #[test]
    fn only_an_exited_session_starts_a_resume() {
        let none = HashMap::new();
        assert_eq!(
            resume_step(Some(&session_row(StateKind::Exited)), &none),
            ResumeStep::Start
        );
        assert_eq!(
            resume_step(Some(&session_row(StateKind::Working)), &none),
            ResumeStep::Notice("only an exited session can be resumed")
        );
        assert_eq!(resume_step(None, &none), ResumeStep::Ignore);
    }

    #[test]
    fn a_second_resume_of_the_same_session_is_ignored() {
        let busy = HashMap::from([("s1".to_string(), Instant::now())]);
        assert_eq!(
            resume_step(Some(&session_row(StateKind::Exited)), &busy),
            ResumeStep::Ignore
        );
    }

    #[test]
    fn stop_and_remove_are_ignored_while_a_resume_runs() {
        let busy = HashMap::from([("s1".to_string(), Instant::now())]);
        for ctl in [Control::Stop, Control::Close] {
            assert!(blocked_by_resume(&busy, "s1", ctl));
            assert!(!blocked_by_resume(&busy, "s2", ctl));
            assert!(!blocked_by_resume(&HashMap::new(), "s1", ctl));
        }
        for ctl in [Control::Resume, Control::Interrupt] {
            assert!(!blocked_by_resume(&busy, "s1", ctl));
        }
        assert_eq!(control_of(Cmd::Stop), Some(Control::Stop));
        assert_eq!(control_of(Cmd::Remove), Some(Control::Close));
        assert_eq!(control_of(Cmd::Resume), None);
    }

    #[test]
    fn a_click_right_after_the_exit_is_not_a_resume() {
        let mut exited = session_row(StateKind::Exited);
        exited.state.since_ms = 10_000;
        assert!(just_exited(&exited, 10_000));
        assert!(just_exited(&exited, 10_499));
        assert!(!just_exited(&exited, 10_500));
        assert!(!just_exited(&exited, 60_000));
        let mut running = session_row(StateKind::Working);
        running.state.since_ms = 10_000;
        assert!(!just_exited(&running, 10_100));
    }

    #[test]
    fn resumed_notice_names_the_fallback_used() {
        assert_eq!(resumed_notice("a", Some("id")), "resumed a");
        assert_eq!(resumed_notice("a", Some("continue")), "resumed a");
        assert_eq!(resumed_notice("a", Some("fresh")), "resumed a (fresh)");
        assert_eq!(resumed_notice("a", Some("rerun")), "resumed a (rerun)");
        assert_eq!(resumed_notice("a", None), "resumed a");
    }

    #[test]
    fn a_palette_stop_asks_before_acting_and_a_session_entry_focuses() {
        let mut view = ViewState::new(rows(&[info("a"), info("b")]), "t");
        view.select("a");
        view.apply(Cmd::Palette);
        view.insert_palette_text(b"stop");
        let PaletteEdit::Run(PaletteEntry::Cmd(cmd)) = view.edit_palette(b"\r") else {
            panic!("expected a command entry");
        };
        assert_eq!(cmd, Cmd::Stop);
        assert!(view.palette.is_none());
        view.apply(cmd);
        assert!(view.confirm.is_some());
        view.confirm = None;
        view.select("b");
        assert_eq!(view.focused_session(), Some("b"));
    }

    #[test]
    fn rows_carry_the_resumed_from_link() {
        let mut i = info("n");
        i.resumed_from = Some("o".into());
        assert_eq!(rows(&[i])[0].resumed_from.as_deref(), Some("o"));
    }

    #[test]
    fn drain_discards_every_in_flight_shell() {
        let (tx, rx) = mpsc::sync_channel(8);
        let mut creating = HashMap::from([
            ("a".to_string(), "sh-1".to_string()),
            ("b".to_string(), "sh-2".to_string()),
        ]);
        tx.send(created("s1", "a")).unwrap();
        tx.send(Msg::StdinClosed).unwrap();
        tx.send(Msg::Created {
            result: Err("boom".into()),
            dir: SplitDir::Down,
            target: "b".into(),
        })
        .unwrap();
        let mut killed = Vec::new();
        drain_creates(
            &mut creating,
            &mut false,
            &rx,
            Duration::from_secs(1),
            |i| killed.push(i.id),
        );
        assert!(creating.is_empty());
        assert_eq!(killed, vec!["s1"]);
    }

    #[test]
    fn drain_keeps_a_spawned_session_and_stops_waiting() {
        let (tx, rx) = mpsc::sync_channel(8);
        let mut creating = HashMap::new();
        let mut spawning = true;
        tx.send(Msg::Spawned {
            id: 1,
            result: Ok(info("s1")),
            req: NewRequest {
                kind: SpawnKind::Custom { line: "x".into() },
                folder: "/".into(),
                name: None,
                place: Place::Right,
            },
            target: "a".into(),
        })
        .unwrap();
        let mut killed = Vec::new();
        drain_creates(
            &mut creating,
            &mut spawning,
            &rx,
            Duration::from_secs(1),
            |i| killed.push(i.id),
        );
        assert!(!spawning);
        assert!(killed.is_empty());
    }

    #[test]
    fn drain_gives_up_on_a_spawn_that_never_answers() {
        let (_tx, rx) = mpsc::sync_channel(1);
        let mut spawning = true;
        let start = Instant::now();
        drain_creates(
            &mut HashMap::new(),
            &mut spawning,
            &rx,
            Duration::from_millis(100),
            |_| {},
        );
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(spawning);
    }

    #[test]
    fn drain_returns_at_once_when_nothing_is_in_flight() {
        let (_tx, rx) = mpsc::sync_channel(1);
        let mut creating = HashMap::new();
        let start = Instant::now();
        drain_creates(
            &mut creating,
            &mut false,
            &rx,
            Duration::from_secs(2),
            |_| {},
        );
        assert!(start.elapsed() < Duration::from_millis(200));
    }

    #[test]
    fn drain_gives_up_after_the_deadline() {
        let (_tx, rx) = mpsc::sync_channel(1);
        let mut creating = HashMap::from([("a".to_string(), "sh-1".to_string())]);
        let start = Instant::now();
        drain_creates(
            &mut creating,
            &mut false,
            &rx,
            Duration::from_millis(100),
            |_| {},
        );
        assert!(start.elapsed() < Duration::from_secs(1));
        assert_eq!(creating.len(), 1);
    }

    #[test]
    fn drain_discards_an_unnamed_dialog_shell_but_keeps_other_kinds() {
        let (tx, rx) = mpsc::sync_channel(8);
        let spawned = |id: &str, kind: SpawnKind, name: Option<&str>| Msg::Spawned {
            id: 1,
            result: Ok(info(id)),
            req: NewRequest {
                kind,
                folder: "/".into(),
                name: name.map(str::to_string),
                place: Place::Right,
            },
            target: "a".into(),
        };
        let mut killed = Vec::new();
        for (id, kind, name) in [
            ("shell", SpawnKind::Shell, None),
            ("named", SpawnKind::Shell, Some("mine")),
            ("custom", SpawnKind::Custom { line: "x".into() }, None),
            (
                "agent",
                SpawnKind::Agent {
                    profile: "codex".into(),
                },
                None,
            ),
        ] {
            tx.send(spawned(id, kind, name)).unwrap();
            let mut spawning = true;
            drain_creates(
                &mut HashMap::new(),
                &mut spawning,
                &rx,
                Duration::from_secs(1),
                |i| killed.push(i.id),
            );
        }
        assert_eq!(killed, vec!["shell"]);
    }
}
