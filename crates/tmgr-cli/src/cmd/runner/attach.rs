use super::attach_chip::{chipper, ChipJob, Opener, SystemOpener};
use super::attach_clip::{Clipboard, SystemClipboard};
use super::attach_export;
use super::attach_git::{diff_argv, git_worker, GitJob};
use super::attach_goto::{fork_call, fork_name, goto_call, text_call, Found, FORK_PASTE_WAIT};
use super::attach_layouts;
use super::attach_new::{
    default_folder, dialog_for, execute_task, in_agent, installed_kinds, kind_label, place_session,
    prepare_task, registers_shell, route, spawn_size, start_session, NewFlow, Route, SpawnErr,
    TaskReview,
};
use super::attach_prompts;
use super::attach_questions::{claude_binary, questioner, QuestionJob};
use super::attach_shell::{
    can_split, cwd_candidates, focus_new_pane, is_shell_session, ladder_for, merge_pending,
    process_cwd, push_typed, reap_ids, shell_argv, shell_name, size_hint, unique_name, watch_ids,
    Ladder, KILL_ONLY,
};
use super::attach_signal::{minute_of_day, settings as signal_settings, SystemNotifier};
use super::attach_status::{peek_worker, status_worker, PeekJob};
use super::attach_summary::{claim, post_real, summarizer, unclaim, Generated, SummaryJob};
use super::attach_tasks;
use super::attach_templates;
use super::attach_title::{agent_src, titler, AgentSrc, TitleJob};
use super::attach_usage::{usage_source, usage_worker};
use super::run_task::Prepared;
use super::watch as watcher;
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
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender, SyncSender};
use std::thread;
use std::time::{Duration, Instant};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::{
    caps, CreateReq, Modes, RestoredRun, SessionInfo, Snapshot, StateKind, TaskRef,
};
use tmgr_ipc::proto::ServerMsg;
use tmgr_runner::attach::{
    cursor, deliver, enter_mouse, focus_on, kind_for_background, layout, mode_changes, open_urls,
    parse_osc11, paste_bytes, peer_bytes, probe, render, reset_modes, valid_pr_url, Action,
    AgentTitle, AttentionTracker, Cmd, Confirm, Control, GitAction, GitInfo, Hit, InputSplitter,
    LayoutPrefs, Loc, NewEdit, NewRequest, NewSession, Notifier, PaletteEdit, PaletteEntry,
    PaletteMode, PaneInfo, Place, PostTarget, PrLookup, QEdit, RenameEdit, ScreenSource, ScrollKey,
    ScrollState, ScrollText, SearchEdit, SessionRow, SessionStatus, SidebarEdit, SignalSettings,
    SignalTracker, SplitDir, SummaryAction, SummaryDraft, TaskChip, TaskOpt, ThemeKind, UsageBar,
    ViewState, OSC11_QUERY,
};
use tmgr_runner::client::{parse_event, AttachWriter, ScreenCache};
use tmgr_runner::daemon::RunnerConfig;
use tmgr_runner::questions::{self, Entry, Status, Store};
use tmgr_runner::sanitize::sanitize_text;
use tmgr_runner::wire::now_ms;

const TICK: Duration = Duration::from_millis(16);
const FRAME: Duration = Duration::from_millis(33);
const LIST_EVERY: Duration = Duration::from_secs(1);
const PROBE_EVERY: Duration = Duration::from_millis(2500);
const PROBE_RETRY: Duration = Duration::from_millis(250);
const TITLE_EVERY: Duration = Duration::from_secs(3);
const QUESTIONS_EVERY: Duration = Duration::from_secs(4);
const STATUS_EVERY: Duration = Duration::from_secs(1);
const SEND_DELAY: Duration = Duration::from_millis(80);
const GIT_EVERY: Duration = Duration::from_secs(5);
const USAGE_EVERY: Duration = Duration::from_secs(10);
const CHIP_EVERY: Duration = Duration::from_secs(5);
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
    Titles(Vec<(String, Option<AgentTitle>)>),
    Status(HashMap<String, SessionStatus>),
    Usage(UsageBar),
    Git(HashMap<String, GitInfo>),
    Summary(Generated),
    Exported(std::result::Result<String, String>),
    Posted(std::result::Result<String, String>, Box<SummaryDraft>),
    Chips(HashMap<String, TaskChip>),
    Peek {
        session: String,
        sub: String,
        lines: Vec<String>,
    },
    Questions(Vec<Entry>),
    Goto {
        session: String,
        question: String,
        result: std::result::Result<Option<Found>, String>,
    },
    Forked {
        source: String,
        prefill: String,
        result: std::result::Result<RestoredRun, String>,
    },
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
    TemplateStarted {
        result: std::result::Result<SessionInfo, SpawnErr>,
        req: NewRequest,
        target: String,
        prompt: String,
    },
    LayoutStarted {
        saved: String,
        req: NewRequest,
        result: std::result::Result<SessionInfo, SpawnErr>,
        generation: u64,
    },
    Tasks(std::result::Result<Vec<TaskOpt>, String>),
    ScrollText {
        session: String,
        result: std::result::Result<ScrollText, String>,
    },
    TaskPrepared {
        id: u64,
        result: std::result::Result<Prepared, SpawnErr>,
        req: NewRequest,
        target: String,
        size: (u16, u16),
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

fn attention_target(view: &ViewState) -> Option<&str> {
    if view.sidebar_focus.is_some()
        || view.new_session.is_some()
        || view.palette.is_some()
        || view.qmenu.is_some()
        || view.confirm.is_some()
        || view.rename.is_some()
        || view.filter_input()
        || view.layout_offer.is_some()
        || view.help
    {
        return None;
    }
    view.focused_session()
}

fn is_user_action(view: &ViewState, action: &Action) -> bool {
    match action {
        Action::FocusIn | Action::FocusOut => false,
        Action::Forward(_) | Action::Drop(_) | Action::Paste { .. } => {
            attention_target(view).is_none()
        }
        _ => true,
    }
}

fn is_watcher_argv(argv: &[String]) -> bool {
    argv.windows(2).any(|w| w[0] == "runner" && w[1] == "watch")
        && argv.iter().any(|a| a == "--cmd")
}

fn offer_answer(data: &[u8]) -> bool {
    matches!(data, [b'y' | b'Y' | b'n' | b'N'] | [0x1b])
}

fn session_kind(s: &SessionInfo) -> Option<String> {
    if is_shell_session(&s.argv, &s.name) {
        Some("shell".to_string())
    } else {
        Some(format!("agent:{}", s.profile.as_ref()?))
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
            you: s.human_s,
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
    human_time: bool,
    window_focused: bool,
    focus_seen: bool,
    last_user: Instant,
    attention: AttentionTracker,
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
    paste_peers: Vec<String>,
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
    tried: HashSet<String>,
    last_probe: Instant,
    agents: HashMap<String, AgentSrc>,
    title_tx: SyncSender<Vec<TitleJob>>,
    title_pending: bool,
    titles_tried: HashSet<String>,
    exit_scanned: HashSet<String>,
    last_title: Instant,
    chip_tx: SyncSender<Vec<ChipJob>>,
    chip_pending: bool,
    last_chip: Instant,
    session_tasks: HashMap<String, (Option<TaskRef>, u64)>,
    web_base: String,
    opener: Box<dyn Opener>,
    clipboard: Box<dyn Clipboard>,
    text_pending: Option<String>,
    text_dirty: bool,
    text_at: Instant,
    signals: SignalTracker,
    signal_cfg: SignalSettings,
    prompt_send: bool,
    summary_mode: SummaryMode,
    summary_tx: SyncSender<SummaryJob>,
    summary_auto: HashMap<String, SummaryDraft>,
    summary_dir: PathBuf,
    running_seen: HashSet<String>,
    claude: Option<PathBuf>,
    send_enter: Option<(String, Instant)>,
    prompts_dir: Option<PathBuf>,
    layouts_dir: Option<PathBuf>,
    layouts_seen: Vec<String>,
    notifier: Box<dyn Notifier>,
    peek_tx: SyncSender<PeekJob>,
    peek_pending: bool,
    last_peek: Instant,
    status_tx: SyncSender<Vec<QuestionJob>>,
    status_pending: bool,
    last_status: Instant,
    usage_tx: SyncSender<Vec<QuestionJob>>,
    git_tx: SyncSender<Vec<GitJob>>,
    git_pending: bool,
    last_git: Instant,
    usage_pending: bool,
    last_usage: Instant,
    last_watch: Instant,
    question_tx: SyncSender<Vec<QuestionJob>>,
    question_pending: bool,
    last_question: Instant,
    question_exited: HashSet<String>,
    qstore: Store,
    forking: HashSet<String>,
    fork_paste: HashMap<String, (String, Instant)>,
    template_ids: HashMap<String, bool>,
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
            Msg::Titles(found) => {
                self.title_pending = false;
                if self.view.set_titles(found) {
                    self.dirty = true;
                }
            }
            Msg::Peek {
                session,
                sub,
                lines,
            } => {
                self.peek_pending = false;
                if self.view.set_peek(&session, &sub, lines) {
                    self.dirty = true;
                }
            }
            Msg::Chips(found) => {
                self.chip_pending = false;
                if self.view.set_chips(found) {
                    self.dirty = true;
                }
            }
            Msg::Status(found) => {
                self.status_pending = false;
                if self.view.set_status(found) {
                    self.dirty = true;
                }
            }
            Msg::Summary(done) => self.on_summary(done),
            Msg::Exported(result) => {
                self.dirty = true;
                match result {
                    Ok(text) => self.notify(&text),
                    Err(e) => self.notify(&format!("export failed: {e}")),
                }
            }
            Msg::Posted(result, draft) => {
                self.dirty = true;
                match result {
                    Ok(text) => self.notify(&text),
                    Err(e) => {
                        self.view.queue_summary(*draft);
                        self.notify(&format!("summary not posted: {e} (Ctrl-] S to retry)"));
                    }
                }
            }
            Msg::Git(found) => {
                self.git_pending = false;
                if self.view.set_git(found) {
                    self.dirty = true;
                }
            }
            Msg::Usage(bar) => {
                self.usage_pending = false;
                if self.view.set_usage(bar) {
                    self.dirty = true;
                }
            }
            Msg::Questions(entries) => {
                self.question_pending = false;
                if self.view.set_questions(entries) {
                    self.dirty = true;
                }
            }
            Msg::Goto {
                session,
                question,
                result,
            } => self.on_goto(&session, &question, result),
            Msg::Forked {
                source,
                prefill,
                result,
            } => self.on_forked(&source, prefill, result),
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
            Msg::TemplateStarted {
                result,
                req,
                target,
                prompt,
            } => self.on_template_started(result, &req, &target, prompt),
            Msg::LayoutStarted {
                saved,
                req,
                result,
                generation,
            } => self.on_layout_started(&saved, &req, result, generation),
            Msg::ScrollText { session, result } => {
                self.text_pending = None;
                self.text_at = Instant::now();
                self.dirty = true;
                let active = (self.view.search.is_some() || self.view.copy.is_some())
                    && self.view.scroll.is_some();
                match result {
                    Ok(text) if active && self.view.focused_session() == Some(session.as_str()) => {
                        let rows = self.page() + 1;
                        self.view.set_scroll_text(text, rows);
                    }
                    Ok(_) => {}
                    Err(e) => {
                        self.view.clear_find();
                        self.view.scroll = None;
                        self.sync_scroll_mode();
                        self.notify(&e);
                    }
                }
            }
            Msg::Tasks(result) => {
                match result {
                    Ok(tasks) => self.view.set_new_tasks(tasks, None),
                    Err(note) => self.view.set_new_tasks(Vec::new(), Some(note)),
                }
                self.dirty = true;
            }
            Msg::TaskPrepared {
                id,
                result,
                req,
                target,
                size,
            } => self.on_task_prepared(id, result, req, target, size),
            Msg::Server(ServerMsg::Closed { reason }) => return Ok(Some(Exit::Lost(reason))),
            Msg::Server(msg) => {
                if let Some(changed) = self.cache.apply(&msg) {
                    self.dirty = true;
                    if self.view.focused_session() == Some(changed.as_str()) {
                        self.text_dirty = true;
                    }
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
        self.view.watchers = list
            .iter()
            .filter(|s| is_watcher_argv(&s.argv))
            .map(|s| s.id.clone())
            .collect();
        self.view.kinds = list
            .iter()
            .filter_map(|s| Some((s.id.clone(), session_kind(s)?)))
            .collect();
        self.session_tasks = list
            .iter()
            .map(|s| (s.id.clone(), (s.task, s.created_ms / 1000)))
            .collect();
        self.agents = list
            .iter()
            .filter_map(|s| Some((s.id.clone(), agent_src(s)?)))
            .collect();
        self.shell_ids = list
            .iter()
            .filter(|s| is_shell_session(&s.argv, &s.name))
            .map(|s| s.id.clone())
            .collect();
        self.exit_scanned
            .retain(|id| new.iter().any(|r| &r.id == id && r.exited()));
        self.titles_tried
            .retain(|id| new.iter().any(|r| &r.id == id));
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
        if self.probe_pending
            || self.last_probe.elapsed() < PROBE_RETRY
            || !self.view.sidebar_shown()
        {
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
        let missing = jobs
            .iter()
            .any(|(id, ..)| !self.view.locs.contains_key(id) && !self.tried.contains(id));
        let every = if missing { PROBE_RETRY } else { PROBE_EVERY };
        if jobs.is_empty() || self.last_probe.elapsed() < every {
            return;
        }
        self.last_probe = Instant::now();
        self.tried.extend(jobs.iter().map(|(id, ..)| id.clone()));
        self.probe_pending = self.probe_tx.try_send(jobs).is_ok();
    }

    fn probe_titles(&mut self) {
        if self.title_pending || self.last_title.elapsed() < PROBE_RETRY {
            return;
        }
        let mut wanted: Vec<String> = self.view.panes().to_vec();
        if self.view.sidebar_shown() {
            wanted.extend(self.view.visible_sessions(self.area()));
        }
        let mut seen = HashSet::new();
        let jobs: Vec<TitleJob> = wanted
            .into_iter()
            .filter(|id| seen.insert(id.clone()))
            .filter_map(|id| {
                let row = self.view.sessions.iter().find(|r| r.id == id)?;
                let src = self.agents.get(&id)?;
                if row.name != src.default_name {
                    return None;
                }
                if row.exited() && !self.exit_scanned.insert(id.clone()) {
                    return None;
                }
                Some(TitleJob {
                    pid: if row.exited() { 0 } else { src.pid },
                    src: src.clone(),
                    sandbox_home: self.cfg.homes.join(&id),
                    log: self.cfg.dir.join("sessions").join(&id).join("output.log"),
                    id,
                })
            })
            .collect();
        let fresh = jobs.iter().any(|j| !self.titles_tried.contains(&j.id));
        let every = if fresh { PROBE_RETRY } else { TITLE_EVERY };
        if jobs.is_empty() || self.last_title.elapsed() < every {
            return;
        }
        self.last_title = Instant::now();
        self.titles_tried.extend(jobs.iter().map(|j| j.id.clone()));
        self.title_pending = self.title_tx.try_send(jobs).is_ok();
    }

    fn question_job(&self, row: &SessionRow, src: &AgentSrc) -> QuestionJob {
        QuestionJob {
            id: row.id.clone(),
            src: src.clone(),
            pid: src.pid,
            sandbox_home: self.cfg.homes.join(&row.id),
            log: self
                .cfg
                .dir
                .join("sessions")
                .join(&row.id)
                .join("output.log"),
            exited: row.exited(),
            settled: !matches!(row.state.kind, StateKind::Working | StateKind::Starting),
        }
    }

    fn probe_chips(&mut self) {
        if self.chip_pending || self.last_chip.elapsed() < CHIP_EVERY {
            return;
        }
        self.last_chip = Instant::now();
        let jobs: Vec<ChipJob> = self
            .view
            .sessions
            .iter()
            .filter(|r| self.agents.contains_key(&r.id))
            .map(|r| {
                let (task, created_s) = self.session_tasks.get(&r.id).copied().unwrap_or((None, 0));
                ChipJob {
                    id: r.id.clone(),
                    task,
                    branch: self.view.locs.get(&r.id).and_then(|l| l.branch.clone()),
                    created_s,
                    sandbox_home: self.cfg.homes.join(&r.id),
                }
            })
            .collect();
        if jobs.is_empty() && self.view.chips.is_empty() {
            return;
        }
        self.chip_pending = self.chip_tx.try_send(jobs).is_ok();
    }

    fn open_task(&mut self, id: &str) {
        let urls = self
            .view
            .chip_of(id)
            .and_then(|chip| open_urls(&self.web_base, chip));
        match urls {
            Some((desktop, web)) => {
                if self.opener.open(&desktop, &web) {
                    self.notify("opening the task…");
                } else {
                    self.notify(&format!("no browser here, open {web}"));
                }
            }
            None => self.notify("no task link for this session"),
        }
    }

    fn check_signals(&mut self) {
        let fresh = self.signals.update(&self.view, &self.signal_cfg);
        if self.signals.current() != &self.view.needs {
            self.view.needs = self.signals.current().clone();
            self.dirty = true;
        }
        if fresh.is_empty() {
            return;
        }
        let focused = self.view.focused_session().map(str::to_string);
        let window = if self.focus_seen {
            self.window_focused
        } else {
            self.last_user.elapsed() < Duration::from_secs(30)
        };
        let names: HashMap<&str, &str> = self
            .view
            .sessions
            .iter()
            .map(|r| (r.id.as_str(), r.name.as_str()))
            .collect();
        let name_of = |id: &str| {
            names
                .get(id)
                .map_or_else(|| id.to_string(), |n| (*n).to_string())
        };
        deliver(
            &fresh,
            name_of,
            |id| window && focused.as_deref() == Some(id),
            &self.signal_cfg,
            minute_of_day(),
            self.notifier.as_mut(),
        );
    }

    fn probe_peek(&mut self) {
        let Some(peek) = self.view.peek.as_ref() else {
            return;
        };
        let every = if peek.lines.is_empty() {
            Duration::from_millis(500)
        } else {
            STATUS_EVERY
        };
        if self.peek_pending || self.last_peek.elapsed() < every {
            return;
        }
        let (Some(row), Some(src)) = (
            self.view.sessions.iter().find(|r| r.id == peek.session),
            self.agents.get(&peek.session),
        ) else {
            return;
        };
        let job = PeekJob {
            job: self.question_job(row, src),
            sub: peek.sub.clone(),
            agent: peek.agent.clone(),
        };
        self.last_peek = Instant::now();
        self.peek_pending = self.peek_tx.try_send(job).is_ok();
    }

    fn probe_status(&mut self) {
        if self.status_pending || self.last_status.elapsed() < STATUS_EVERY {
            return;
        }
        let mut wanted: Vec<String> = self.view.panes().to_vec();
        if self.view.sidebar_shown() {
            wanted.extend(self.view.visible_sessions(self.area()));
        }
        let mut seen = HashSet::new();
        let jobs: Vec<QuestionJob> = wanted
            .into_iter()
            .filter(|id| seen.insert(id.clone()))
            .filter_map(|id| {
                let row = self.view.sessions.iter().find(|r| r.id == id)?;
                let src = self.agents.get(&id)?;
                (!row.exited()).then(|| self.question_job(row, src))
            })
            .collect();
        self.last_status = Instant::now();
        let stale = !self.view.status.is_empty();
        if jobs.is_empty() && !stale {
            return;
        }
        self.status_pending = self.status_tx.try_send(jobs).is_ok();
    }

    fn summary_job(&self, id: &str, suffix: u64) -> Option<(SummaryDraft, SummaryJob)> {
        let row = self.view.sessions.iter().find(|r| r.id == id)?;
        let src = self.agents.get(id)?;
        if src.format != tmgr_runner::attach::Format::Claude {
            return None;
        }
        let chip = self.view.chip_of(id);
        let target = chip.and_then(|c| {
            Some(PostTarget {
                task_id: c.task_id?,
                run_id: c.run_id?,
                workspace: c.run_ws.clone(),
                task_key: c.key.clone(),
            })
        });
        let key = format!(
            "{}-{}-{}",
            target.as_ref().map_or(0, |t| t.run_id),
            id,
            suffix
        );
        let mut job = self.question_job(row, src);
        job.settled = true;
        let draft = SummaryDraft {
            key: key.clone(),
            session: id.to_string(),
            name: row.name.clone(),
            target,
            text: None,
            editing: None,
            plain: false,
        };
        let workdir = self.cfg.dir.join("summary-cwd");
        let job = SummaryJob {
            key,
            job,
            claude: self
                .claude
                .clone()
                .filter(|_| tmgr_ipc::dirs::ensure_private_dir(&workdir).is_ok()),
            workdir,
        };
        Some((draft, job))
    }

    fn check_exits(&mut self) {
        let mut ended = Vec::new();
        for row in &self.view.sessions {
            if !row.exited() {
                self.running_seen.insert(row.id.clone());
            } else if self.running_seen.remove(&row.id) && !self.resuming.contains_key(&row.id) {
                ended.push((row.id.clone(), row.state.since_ms));
            }
        }
        self.running_seen
            .retain(|id| self.view.sessions.iter().any(|r| &r.id == id));
        if self.summary_mode == SummaryMode::Never {
            return;
        }
        for (id, since) in ended {
            let Some((draft, job)) = self.summary_job(&id, since) else {
                continue;
            };
            if draft.target.is_none() {
                continue;
            }
            let always = self.summary_mode == SummaryMode::Always;
            if self.summary_tx.try_send(job).is_err() {
                self.notify("too many summaries in flight: one was skipped, use Ctrl-] S on it");
                continue;
            }
            if always {
                self.summary_auto.insert(draft.key.clone(), draft);
            } else {
                self.view.queue_summary(draft);
            }
            self.dirty = true;
        }
    }

    fn export_command(&mut self) {
        self.dirty = true;
        if self.view.export.is_some() {
            return;
        }
        let Some(id) = self.view.focused_session().map(str::to_string) else {
            return;
        };
        let (Some(row), Some(src)) = (
            self.view.sessions.iter().find(|r| r.id == id),
            self.agents.get(&id),
        ) else {
            return self.notify("export is available for Claude sessions");
        };
        if src.format != tmgr_runner::attach::Format::Claude {
            return self.notify("export is available for Claude sessions");
        }
        let Some(home) = attach_export::home_dir() else {
            return self.notify("cannot find your home folder");
        };
        let job = self.question_job(row, src);
        let name = row.name.clone();
        let Some(loaded) = attach_export::load(&job, &name, &home.to_string_lossy()) else {
            return self.notify("the session transcript is not available");
        };
        let refusal =
            super::attach_new::in_agent().then(|| "refused inside agent sessions".to_string());
        let ws = tmgr_core::config::config_dir()
            .ok()
            .and_then(|d| tmgr_core::config::Config::load(&d).ok())
            .and_then(|c| c.default_workspace)
            .filter(|w| !w.starts_with("local:"));
        let in_cwd = std::fs::canonicalize(&src.cwd)
            .ok()
            .zip(std::fs::canonicalize(&home).ok())
            .is_some_and(|(cwd, h)| h.starts_with(cwd));
        let path = if in_cwd {
            PathBuf::new()
        } else {
            attach_export::default_path(&name, &home)
        };
        let state =
            attach_export::state_for(loaded, &name, ws, refusal, &path, &home.to_string_lossy());
        self.view.open_export(state);
    }

    fn export_action(&mut self, action: tmgr_runner::attach::ExportAction) {
        use tmgr_runner::attach::ExportAction;
        self.dirty = true;
        self.notify("exporting…");
        let tx = self.tx.clone();
        match action {
            ExportAction::File { text, path } => {
                let path = PathBuf::from(path);
                thread::spawn(move || {
                    let _ = tx.send(Msg::Exported(attach_export::write_export(&text, &path)));
                });
            }
            ExportAction::Page { text, title, code } => {
                thread::spawn(move || {
                    let _ = tx.send(Msg::Exported(attach_export::post_page(
                        &code, &title, &text,
                    )));
                });
            }
        }
    }

    fn summary_command(&mut self) {
        self.dirty = true;
        if self.view.open_summary() {
            return;
        }
        let Some(id) = self.view.focused_session().map(str::to_string) else {
            return;
        };
        let Some((draft, job)) = self.summary_job(&id, now_ms()) else {
            return self.notify("summaries are available for Claude sessions");
        };
        if self.summary_tx.try_send(job).is_err() {
            return self.notify("a summary is already being prepared, try again in a moment");
        }
        self.view.show_summary(draft);
    }

    fn on_summary(&mut self, done: Generated) {
        self.dirty = true;
        if let Some(mut draft) = self.summary_auto.remove(&done.key) {
            draft.text = Some(done.text);
            draft.plain = done.plain;
            if done.tainted || done.plain {
                self.view.queue_summary(draft);
                self.notify("a summary needs your review: Ctrl-] S");
            } else {
                self.post_now(draft);
            }
            return;
        }
        let shown = self
            .view
            .summary
            .as_ref()
            .is_some_and(|d| d.key == done.key);
        if self.view.set_summary_text(&done.key, done.text, done.plain) && !shown {
            self.notify("a session summary is ready: Ctrl-] S to review");
        }
    }

    fn summary_action(&mut self, action: SummaryAction) {
        self.dirty = true;
        match action {
            SummaryAction::Post(draft) => self.post_now(*draft),
            SummaryAction::Skip(key) => {
                self.summary_auto.remove(&key);
            }
        }
    }

    fn post_now(&mut self, draft: SummaryDraft) {
        let kept = Box::new(draft.clone());
        let (Some(target), Some(text)) = (draft.target, draft.text) else {
            return;
        };
        if !claim(&self.summary_dir, &draft.key) {
            return self.notify("this summary was already posted");
        }
        self.notify("posting the summary…");
        let tx = self.tx.clone();
        let dir = self.summary_dir.clone();
        let key = draft.key;
        thread::spawn(move || {
            let result = post_real(&target, &text);
            if result.is_err() {
                unclaim(&dir, &key);
            }
            let _ = tx.send(Msg::Posted(result, kept));
        });
    }

    fn probe_git(&mut self) {
        if self.git_pending || self.last_git.elapsed() < GIT_EVERY {
            return;
        }
        self.last_git = Instant::now();
        let jobs: Vec<GitJob> = self
            .view
            .panes()
            .iter()
            .filter_map(|id| {
                let row = self.view.sessions.iter().find(|r| &r.id == id)?;
                let (start, pid) = self.starts.get(id).cloned().unwrap_or_default();
                let start = (!start.is_empty()).then_some(start);
                let cwds: Vec<PathBuf> = cwd_candidates(process_cwd(pid), start.as_deref())
                    .into_iter()
                    .map(PathBuf::from)
                    .collect();
                (!row.exited() || !cwds.is_empty()).then(|| GitJob {
                    id: id.clone(),
                    cwds,
                })
            })
            .collect();
        self.git_pending = self.git_tx.try_send(jobs).is_ok();
    }

    fn git_action(&mut self, action: GitAction) {
        self.dirty = true;
        match action {
            GitAction::Close => {}
            GitAction::OpenPr => {
                let url = match self.view.focused_git().map(|g| &g.pr) {
                    Some(PrLookup::Found(pr)) if valid_pr_url(&pr.url) => pr.url.clone(),
                    _ => return self.notify("no pull request to open"),
                };
                if self.opener.open(&url, &url) {
                    self.notify("opening the pull request");
                } else {
                    self.notify("cannot open a browser here");
                }
            }
            GitAction::Diff => {
                let Some(root) = self.view.focused_git().map(|g| g.root.clone()) else {
                    return;
                };
                let pager = std::env::var("PAGER").ok();
                self.view.git_open = false;
                self.request_split_cmd(
                    SplitDir::Down,
                    Some((diff_argv(pager.as_deref()), root, "diff")),
                );
            }
        }
    }

    fn probe_usage(&mut self) {
        if self.usage_pending || self.last_usage.elapsed() < USAGE_EVERY {
            return;
        }
        self.last_usage = Instant::now();
        let jobs: Vec<QuestionJob> = self
            .view
            .panes()
            .iter()
            .filter_map(|id| {
                let row = self.view.sessions.iter().find(|r| &r.id == id)?;
                let src = self.agents.get(id)?;
                Some(self.question_job(row, src))
            })
            .collect();
        self.usage_pending = self.usage_tx.try_send(jobs).is_ok();
    }

    fn probe_questions(&mut self) {
        if self.question_pending || self.last_question.elapsed() < QUESTIONS_EVERY {
            return;
        }
        self.last_question = Instant::now();
        self.question_exited
            .retain(|id| self.view.sessions.iter().any(|r| &r.id == id && r.exited()));
        let mut jobs = Vec::new();
        for row in &self.view.sessions {
            let Some(src) = self.agents.get(&row.id) else {
                continue;
            };
            if row.exited() && !self.question_exited.insert(row.id.clone()) {
                continue;
            }
            jobs.push(self.question_job(row, src));
        }
        self.question_pending = self.question_tx.try_send(jobs).is_ok();
    }

    fn questions_input(&mut self, data: &[u8]) {
        self.dirty = true;
        let edit = self.view.edit_qmenu(data);
        self.question_edit(edit);
    }

    fn question_edit(&mut self, edit: QEdit) {
        self.dirty = true;
        let now_s = now_ms() / 1000;
        match edit {
            QEdit::Editing | QEdit::Close => {}
            QEdit::Answer(id) => {
                if let Err(e) = self.answer_question(&id, now_s) {
                    self.notify(&e.to_string());
                }
            }
            QEdit::Goto(id) => self.request_goto(&id),
            QEdit::Fork(id) => self.request_fork(&id, now_s),
            QEdit::Ignore(id) => self.mark_question(&id, Status::Ignored, now_s),
            QEdit::Restore(id) => self.mark_question(&id, Status::Open, now_s),
        }
    }

    fn mark_question(&mut self, id: &str, status: Status, now_s: u64) {
        match self.qstore.mark(id, status, now_s) {
            Ok(()) => self.view.set_question_status(id, status),
            Err(e) => self.notify(&format!("cannot save: {e}")),
        }
    }

    fn answer_question(&mut self, id: &str, now_s: u64) -> io::Result<()> {
        let Some(entry) = self.view.question(id).cloned() else {
            return Ok(());
        };
        let session = entry.q.session.clone();
        if !self
            .view
            .sessions
            .iter()
            .any(|r| r.id == session && !r.exited())
        {
            self.notify("the session is not running, resume it first");
            return Ok(());
        }
        self.view.open(&session);
        self.paste_into(&session, &questions::paste_text(&entry.q, now_s))?;
        self.mark_question(id, Status::Answered, now_s);
        self.notify("pasted, type your answer and press Enter");
        Ok(())
    }

    fn paste_into(&mut self, session: &str, text: &str) -> io::Result<()> {
        let body = if self.bracketed(session) {
            format!("\x1b[200~{text}\x1b[201~")
        } else {
            text.replace('\n', " ")
        };
        self.send_to(session, body.as_bytes())
    }

    fn request_goto(&mut self, id: &str) {
        let Some(entry) = self.view.question(id).cloned() else {
            return;
        };
        let session = entry.q.session.clone();
        let alt = self
            .cache
            .snapshot(&session)
            .is_some_and(|s| s.modes.alt_screen);
        if alt {
            self.notify("this agent uses the full screen, showing the context");
            return self.view.show_context(&session, id);
        }
        let cfg = self.cfg.clone();
        let tx = self.tx.clone();
        let question = if entry.q.item.is_empty() {
            entry.q.text.clone()
        } else {
            entry.q.item.clone()
        };
        let key = id.to_string();
        thread::spawn(move || {
            let result = goto_call(&cfg, &session, &question);
            let _ = tx.send(Msg::Goto {
                session,
                question: key,
                result,
            });
        });
    }

    fn on_goto(
        &mut self,
        session: &str,
        question: &str,
        result: std::result::Result<Option<Found>, String>,
    ) {
        self.dirty = true;
        match result {
            Ok(Some((offset, max, mark))) if self.view.sessions.iter().any(|r| r.id == session) => {
                self.view.open(session);
                self.view.scroll = Some(ScrollState { offset, max });
                self.view.goto = Some(mark);
                self.scroll_mode = true;
                self.splitter.set_scroll_mode(true);
                self.notify("Esc returns to the live screen");
            }
            other => {
                if let Err(e) = other {
                    self.notify(&e);
                } else {
                    self.notify("not in the scrollback any more, showing the context");
                }
                self.view.show_context(session, question);
            }
        }
    }

    fn request_fork(&mut self, id: &str, now_s: u64) {
        let Some(entry) = self.view.question(id).cloned() else {
            return;
        };
        let session = entry.q.session.clone();
        let Some((row, src)) = self
            .view
            .sessions
            .iter()
            .find(|r| r.id == session)
            .zip(self.agents.get(&session))
        else {
            return self.notify("this session is not ready to fork yet");
        };
        if src.profile != "claude-code" {
            return self.notify("fork works for Claude Code sessions only");
        }
        if !self.forking.insert(session.clone()) {
            return;
        }
        let job = self.question_job(row, src);
        let names: Vec<&str> = self.view.sessions.iter().map(|r| r.name.as_str()).collect();
        let name = fork_name(&names, &row.name);
        let content = layout(self.area(), &self.view)
            .panes
            .iter()
            .find(|p| p.session == session)
            .map(|p| p.content())
            .unwrap_or_default();
        let size = spawn_size(content, Place::Right);
        let prefill = questions::paste_text(&entry.q, now_s);
        self.notify("forking…");
        let cfg = self.cfg.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = fork_call(&cfg, &job, &name, size);
            let sent = tx.send(Msg::Forked {
                source: job.id,
                prefill,
                result,
            });
            if let Err(mpsc::SendError(Msg::Forked {
                result: Ok(run), ..
            })) = sent
            {
                run_ladder(&cfg, &run.session.id, true, KILL_ONLY);
            }
        });
    }

    fn on_forked(
        &mut self,
        source: &str,
        prefill: String,
        result: std::result::Result<RestoredRun, String>,
    ) {
        self.forking.remove(source);
        self.dirty = true;
        let info = match result {
            Ok(run) => run.session,
            Err(e) => return self.notify(&e),
        };
        if !self.view.sessions.iter().any(|r| r.id == info.id) {
            self.view.sessions.extend(rows(std::slice::from_ref(&info)));
        }
        self.pending.insert(info.id.clone(), Instant::now());
        self.starts
            .insert(info.id.clone(), (info.cwd.clone(), info.child_pid));
        let split = place_session(&mut self.view, source, Place::Right, &info.id);
        self.fork_paste
            .insert(info.id.clone(), (prefill, Instant::now()));
        let name = sanitize_text(&info.name).into_owned();
        if split {
            self.notify(&format!("forked into {name}"));
        } else {
            self.notify(&format!("forked into {name}, no room for another pane"));
        }
    }

    fn trust_prompt(&self, session: &str) -> bool {
        self.cache.snapshot(session).is_some_and(|s| {
            s.lines.iter().any(|l| {
                let text: String = l.spans.iter().map(|sp| sp.t.as_str()).collect();
                let text = text.to_lowercase();
                text.contains("do you trust") || text.contains("trust this folder")
            })
        })
    }

    fn paste_forks(&mut self) {
        let gone: Vec<String> = self
            .fork_paste
            .keys()
            .filter(|id| !self.view.sessions.iter().any(|r| &r.id == *id))
            .cloned()
            .collect();
        for id in gone {
            self.fork_paste.remove(&id);
            if self.template_ids.remove(&id).is_some() {
                self.notify("the session closed before the template prompt was pasted");
            }
        }
        let ready: Vec<String> = self
            .fork_paste
            .iter()
            .filter(|(id, (_, at))| {
                at.elapsed() > FORK_PASTE_WAIT
                    || self.view.sessions.iter().any(|r| {
                        &r.id == *id
                            && (r.state.kind == StateKind::Exited
                                || r.state.kind == StateKind::Waiting && !self.trust_prompt(id))
                    })
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in ready {
            let Some((text, at)) = self.fork_paste.remove(&id) else {
                continue;
            };
            let waiting = self
                .view
                .sessions
                .iter()
                .any(|r| r.id == id && r.state.kind == StateKind::Waiting);
            self.dirty = true;
            if let Some(agent) = self.template_ids.remove(&id) {
                if !waiting || at.elapsed() > FORK_PASTE_WAIT {
                    self.notify("started, the template prompt was not pasted");
                    continue;
                }
                let bytes = paste_bytes(&text, self.bracketed(&id));
                if self.send_to(&id, &bytes).is_err() {
                    self.notify("started, the template prompt was not pasted");
                } else if self.prompt_send && agent {
                    self.send_enter = Some((id, Instant::now() + SEND_DELAY));
                    self.notify("template prompt sent");
                } else {
                    self.notify("template prompt pasted, press Enter to send");
                }
                continue;
            }
            if !waiting || at.elapsed() > FORK_PASTE_WAIT {
                self.notify("forked, the question was not pasted");
            } else if self.paste_into(&id, &text).is_ok() {
                self.notify("forked, type your answer and press Enter");
            } else {
                self.notify("forked, the question was not pasted");
            }
        }
    }

    fn is_shell(&self, id: &str) -> bool {
        self.view.shells.iter().any(|s| s == id) || self.shell_ids.contains(id)
    }

    fn request_split(&mut self, dir: SplitDir) {
        self.request_split_cmd(dir, None);
    }

    fn request_split_cmd(
        &mut self,
        dir: SplitDir,
        cmd: Option<(Vec<String>, String, &'static str)>,
    ) {
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
        let (argv, mut cwds, label) = match cmd {
            Some((argv, root, label)) => (Some(argv), vec![root], label),
            None => (
                None,
                cwd_candidates(process_cwd(pid), start.as_deref()),
                "shell",
            ),
        };
        if cwds.is_empty() {
            cwds.extend(
                std::env::current_dir()
                    .ok()
                    .map(|p| p.to_string_lossy().into_owned()),
            );
        }
        let name = if argv.is_some() {
            unique_name(&self.view.sessions, label)
        } else {
            shell_name(
                &self.view.sessions,
                &self.creating,
                self.new_flow.planned_name(),
            )
        };
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
        self.notify(&if argv.is_some() {
            format!("opening the {label}…")
        } else {
            "starting shell…".to_string()
        });
        thread::spawn(move || {
            let result = create_shell(&cfg, &cwds, &name, argv, cols, rows);
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
        if !self.view.panes().iter().any(|p| p == target) && self.view.in_tab(target) {
            self.view.open(target);
        }
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
        self.new_flow.drop_review();
        self.view.open_new_session(folder, installed_kinds(), home);
        let tx = self.tx.clone();
        let agent = in_agent();
        thread::spawn(move || {
            let _ = tx.send(Msg::Tasks(attach_tasks::load(agent)));
        });
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
        if req.task.is_some() {
            return self.begin_prepare(base, req, target, (cols, rows));
        }
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

    fn begin_prepare(
        &mut self,
        base: NewSession,
        req: NewRequest,
        target: String,
        size: (u16, u16),
    ) {
        self.notify("checking the task...");
        let id = self
            .new_flow
            .begin(dialog_for(&base, &req), None, Instant::now());
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = prepare_task(&req, in_agent());
            let _ = tx.send(Msg::TaskPrepared {
                id,
                result,
                req,
                target,
                size,
            });
        });
    }

    fn on_task_prepared(
        &mut self,
        id: u64,
        result: std::result::Result<Prepared, SpawnErr>,
        req: NewRequest,
        target: String,
        size: (u16, u16),
    ) {
        self.dirty = true;
        let Some(dialog) = self.new_flow.finish(id) else {
            return;
        };
        match result {
            Ok(prepared) => {
                let lines = prepared.summary().lines().map(str::to_string).collect();
                self.new_flow.hold_review(TaskReview {
                    prepared,
                    req,
                    target,
                    size,
                });
                self.view.review_new_session(dialog, lines);
            }
            Err(e) => self
                .view
                .reopen_new_session(dialog.with_error(e.message, e.hint)),
        }
    }

    fn approve_task(&mut self) {
        if !self.view.review_fits(self.area()) {
            self.dirty = true;
            return self.notify("enlarge the terminal to read the whole run summary");
        }
        if self.new_flow.review_is_none() || self.view.new_session.is_none() {
            return;
        }
        let (Some(review), Some(mut dialog)) =
            (self.new_flow.take_review(), self.view.new_session.take())
        else {
            return;
        };
        dialog.review = None;
        self.dirty = true;
        self.notify("starting the task run...");
        let id = self
            .new_flow
            .begin(dialog_for(&dialog, &review.req), None, Instant::now());
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = execute_task(&review, in_agent());
            let _ = tx.send(Msg::Spawned {
                id,
                result,
                req: review.req,
                target: review.target,
            });
        });
    }

    fn sync_review(&mut self) {
        let shown = self
            .view
            .new_session
            .as_ref()
            .is_some_and(|n| n.review.is_some());
        self.new_flow.keep_review_if(shown);
    }

    fn modal_input(&mut self, route: Route, action: Action) {
        match (route, action) {
            (Route::Dialog, Action::Forward(data)) => {
                let before = self.view.new_session.clone();
                match self.view.edit_new_session(&data) {
                    NewEdit::Submit(req) => self.begin_spawn(before, req),
                    NewEdit::Approve => self.approve_task(),
                    _ => {}
                }
                self.sync_review();
            }
            (Route::Dialog, Action::Drop(data) | Action::Paste { data, .. }) => {
                self.view.insert_new_session_text(&data);
            }
            (Route::Git, Action::Forward(data)) => {
                if let Some(action) = self.view.edit_git(&data) {
                    self.git_action(action);
                }
                self.dirty = true;
            }
            (Route::Export, Action::Forward(data)) => {
                if let Some(action) = self.view.edit_export(&data) {
                    self.export_action(action);
                }
                self.dirty = true;
            }
            (Route::Summary, Action::Forward(data) | Action::Paste { data, .. }) => {
                if let Some(action) = self.view.edit_summary(&data) {
                    self.summary_action(action);
                }
                self.dirty = true;
            }
            (Route::Questions, Action::Forward(data)) => self.questions_input(&data),
            (Route::Peek, Action::Forward(data)) => {
                self.view.edit_peek(&data);
                self.dirty = true;
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

    fn begin_find(&mut self) -> Option<String> {
        let id = self.view.focused_session().map(str::to_string)?;
        self.view.confirm = None;
        self.view.rename = None;
        let saved = self.view.scroll;
        self.leave_overlays();
        match saved {
            Some(s) => self.view.scroll = Some(s),
            None => self.view.apply(Cmd::ScrollMode),
        }
        self.sync_scroll_max();
        self.view.scroll.is_some().then_some(id)
    }

    fn begin_search(&mut self) {
        self.dirty = true;
        if self.begin_find().is_none() {
            return;
        }
        self.view.copy = None;
        self.view.open_search();
        self.sync_scroll_mode();
        self.request_text();
    }

    fn sync_scroll_mode(&mut self) {
        let typing = self.view.search.as_ref().is_some_and(|s| s.input);
        self.scroll_mode = self.view.scroll.is_some() && !typing;
        self.splitter.set_scroll_mode(self.scroll_mode);
    }

    fn begin_copy(&mut self) {
        self.dirty = true;
        if self.begin_find().is_none() {
            return;
        }
        if let Some(s) = self.view.search.as_mut() {
            s.input = false;
        }
        let rows = self.page() + 1;
        self.view.enter_copy(false, rows);
        self.sync_scroll_mode();
        self.request_text();
    }

    fn after_find(&mut self) {
        self.dirty = true;
        self.sync_scroll_mode();
        if let Some(text) = self.view.take_copied() {
            let msg = self.clipboard.copy(&text);
            self.notify(&msg);
        }
        if (self.view.search.is_some() || self.view.copy.is_some()) && self.view.stext.is_none() {
            self.request_text();
        }
        if let Some(n) = self.view.notice.take() {
            self.notify(&n);
        }
    }

    fn request_text(&mut self) {
        let Some(session) = self.view.focused_session().map(str::to_string) else {
            return;
        };
        if self.text_pending.as_deref() == Some(session.as_str()) {
            return;
        }
        self.text_pending = Some(session.clone());
        self.text_dirty = false;
        let cfg = self.cfg.clone();
        let tx = self.tx.clone();
        thread::spawn(move || {
            let result = text_call(&cfg, &session);
            let _ = tx.send(Msg::ScrollText { session, result });
        });
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
        self.report_attention(false);
        self.writer.input(session, data)
    }

    fn broadcast_peers(&self, primary: &str) -> Vec<String> {
        self.view
            .broadcast_peers(primary)
            .into_iter()
            .filter(|id| !self.creating.contains_key(id))
            .collect()
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
        if ctl != Control::Interrupt {
            self.view.layout_offer = None;
            self.view.end_filter_input();
        }
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
                self.view.open(&id);
                Ok(None)
            }
            PaletteEntry::Prompt(idx) => {
                self.paste_prompt(idx)?;
                Ok(None)
            }
            PaletteEntry::NewPrompt => Ok(None),
            PaletteEntry::Layout(idx) => {
                self.switch_layout(idx);
                Ok(None)
            }
            PaletteEntry::Template(_) => Ok(None),
            PaletteEntry::RunWatch(folder, command) => {
                self.start_watch(&folder, &command);
                Ok(None)
            }
            PaletteEntry::RunTemplate(idx, values) => {
                self.start_template(idx, &values);
                Ok(None)
            }
            PaletteEntry::SaveLayout(name) => {
                match self.view.save_layout(&name) {
                    Ok(saved) => {
                        self.save_layouts();
                        self.notify(&format!("layout {saved} saved"));
                    }
                    Err(e) => self.notify(&e),
                }
                Ok(None)
            }
        }
    }

    fn save_layouts(&mut self) {
        let Some(dir) = self.layouts_dir.clone() else {
            self.notify("cannot locate the config directory");
            return;
        };
        match attach_layouts::save(&dir, &self.view.layouts, &self.layouts_seen) {
            Ok(merged) => {
                self.layouts_seen = attach_layouts::names(&merged);
                self.view.layouts = merged;
            }
            Err(_) => self.notify("cannot save layouts.json"),
        }
    }

    fn switch_layout(&mut self, idx: usize) {
        if self.view.switch_layout(idx) {
            self.watched = None;
            self.scroll_mode = false;
            self.splitter.set_scroll_mode(false);
        }
        self.dirty = true;
        if self.view.layout_offer.is_none() {
            if let Some(n) = self.view.notice.clone() {
                self.notify(&n);
            }
        }
    }

    fn start_template(&mut self, idx: usize, values: &[String]) {
        let Some(t) = self.view.templates.get(idx).cloned() else {
            return;
        };
        let launch = match attach_templates::launch(&t, values, &installed_kinds()) {
            Ok(l) => l,
            Err(e) => return self.notify(&e),
        };
        let base = self.template_folder();
        let folder = match attach_templates::resolve_folder(launch.folder.as_deref(), &base) {
            Ok(f) => f,
            Err(e) => return self.notify(&e),
        };
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
        let req = NewRequest {
            kind: launch.kind,
            folder,
            name: None,
            place: Place::Right,
            task: None,
        };
        let (cols, rows) = spawn_size(content, req.place);
        self.notify(&format!("starting {}...", sanitize_text(&t.name)));
        let cfg = self.cfg.clone();
        let tx = self.tx.clone();
        let prompt = launch.prompt;
        thread::spawn(move || {
            let result = start_session(&cfg, &req, cols, rows);
            let _ = tx.send(Msg::TemplateStarted {
                result,
                req,
                target,
                prompt,
            });
        });
    }

    fn begin_watch(&mut self) {
        let Some(id) = self.view.focused_session().map(str::to_string) else {
            return;
        };
        if self.view.watchers.contains(&id) {
            return self.rerun_watch(&id);
        }
        let folder = self
            .view
            .locs
            .get(&id)
            .map(|l| l.root.clone())
            .unwrap_or_else(|| self.template_folder());
        let command = watcher::detect(Path::new(&folder)).unwrap_or_default();
        self.view.open_watch_form(&folder, &command);
    }

    fn start_watch(&mut self, folder: &str, command: &str) {
        if !Path::new(folder).is_dir() {
            return self.notify("the folder to watch does not exist");
        }
        if command.trim().is_empty() {
            return self.notify("no command to run");
        }
        if watcher::too_broad(Path::new(folder)) {
            return self.notify("that folder is too big to watch, open the session in a project");
        }
        let argv = vec![
            self.cfg.self_exe.display().to_string(),
            "runner".into(),
            "watch".into(),
            "--dir".into(),
            folder.to_string(),
            "--cmd".into(),
            command.to_string(),
        ];
        self.request_split_cmd(SplitDir::Down, Some((argv, folder.to_string(), "watch")));
    }

    fn rerun_watch(&mut self, id: &str) {
        let alive = self.view.sessions.iter().any(|r| r.id == id && !r.exited());
        let pid = self.starts.get(id).map_or(0, |s| s.1);
        let sent = alive
            && pid != 0
            && watcher::status_dir().is_some_and(|d| watcher::request_rerun(&d, pid).is_ok());
        self.notify(if sent {
            "rerunning"
        } else {
            "cannot reach the watcher"
        });
    }

    fn poll_watchers(&mut self) {
        if self.last_watch.elapsed() < Duration::from_secs(1) {
            return;
        }
        self.last_watch = Instant::now();
        let Some(dir) = watcher::status_dir() else {
            return;
        };
        let mut changed = false;
        let ids: Vec<String> = self.view.watchers.iter().cloned().collect();
        for id in ids {
            let alive = self.view.sessions.iter().any(|r| r.id == id && !r.exited());
            let pid = self.starts.get(&id).map_or(0, |s| s.1);
            let status = if alive {
                watcher::read_status(&dir, pid)
            } else {
                None
            };
            match status {
                Some(s) if self.view.watch.get(&id) != Some(&s) => {
                    self.view.watch.insert(id, s);
                    changed = true;
                }
                Some(_) => {}
                None => changed |= self.view.watch.remove(&id).is_some(),
            }
        }
        let before = self.view.watch.len();
        self.view
            .watch
            .retain(|id, _| self.view.watchers.contains(id));
        if changed || before != self.view.watch.len() {
            self.dirty = true;
        }
    }

    fn template_folder(&self) -> String {
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
        default_folder(process_cwd(pid), start, std::env::current_dir().ok(), &home)
    }

    fn on_template_started(
        &mut self,
        result: std::result::Result<SessionInfo, SpawnErr>,
        req: &NewRequest,
        target: &str,
        prompt: String,
    ) {
        self.dirty = true;
        let info = match result {
            Ok(info) => info,
            Err(e) => {
                let why = sanitize_text(&e.message).into_owned();
                return self.notify(&format!("could not start {}: {why}", kind_label(&req.kind)));
            }
        };
        if !self.view.sessions.iter().any(|r| r.id == info.id) {
            self.view.sessions.extend(rows(std::slice::from_ref(&info)));
        }
        self.pending.insert(info.id.clone(), Instant::now());
        self.starts
            .insert(info.id.clone(), (info.cwd.clone(), info.child_pid));
        let split = place_session(&mut self.view, target, req.place, &info.id);
        if !prompt.trim().is_empty() {
            self.template_ids.insert(
                info.id.clone(),
                matches!(req.kind, tmgr_runner::attach::SpawnKind::Agent { .. }),
            );
            self.fork_paste
                .insert(info.id.clone(), (prompt, Instant::now()));
        }
        let name = sanitize_text(&info.name).into_owned();
        if split {
            self.notify(&format!("started {name}"));
        } else {
            self.notify(&format!("started {name}, no room for another pane"));
        }
    }

    fn offer_key(&mut self, data: &[u8]) {
        self.dirty = true;
        let Some(offer) = self.view.take_layout_offer() else {
            return;
        };
        if !matches!(data, [b'y' | b'Y']) {
            self.view.layout_restore = None;
            self.notify("left the missing sessions as they are");
            return;
        }
        for id in &offer.resume {
            self.request_resume(id);
        }
        if offer.start.is_empty() {
            self.view.layout_restore = None;
            return;
        }
        let generation = self
            .view
            .layout_restore
            .as_ref()
            .map_or(0, |r| r.generation);
        let mut taken: Vec<String> = Vec::new();
        for m in offer.start {
            let kind = match m.kind.as_deref() {
                Some("shell") => Some(tmgr_runner::attach::SpawnKind::Shell),
                Some(k) => {
                    k.strip_prefix("agent:")
                        .map(|profile| tmgr_runner::attach::SpawnKind::Agent {
                            profile: profile.to_string(),
                        })
                }
                None => None,
            };
            let Some(kind) = kind else {
                self.view.layout_started(&m.id, None, generation);
                continue;
            };
            if !Path::new(&m.dir).is_absolute() || !Path::new(&m.dir).is_dir() {
                self.notify(&format!("folder of {} is gone", sanitize_text(&m.name)));
                self.view.layout_started(&m.id, None, generation);
                continue;
            }
            let req = NewRequest {
                kind,
                folder: m.dir.clone(),
                name: None,
                place: Place::Right,
                task: None,
            };
            let mut run_req = req.clone();
            if registers_shell(&req) {
                let mut creating = self.creating.clone();
                for (i, t) in taken.iter().enumerate() {
                    creating.insert(format!("\0layout{i}"), t.clone());
                }
                let reusable = m
                    .name
                    .strip_prefix("sh-")
                    .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
                    && !taken.contains(&m.name)
                    && !self.view.sessions.iter().any(|r| r.name == m.name);
                let name = if reusable {
                    m.name.clone()
                } else {
                    shell_name(&self.view.sessions, &creating, None)
                };
                taken.push(name.clone());
                run_req.name = Some(name);
            }
            let cfg = self.cfg.clone();
            let tx = self.tx.clone();
            let saved = m.id.clone();
            thread::spawn(move || {
                let result = start_session(&cfg, &run_req, 80, 24);
                let _ = tx.send(Msg::LayoutStarted {
                    saved,
                    req,
                    result,
                    generation,
                });
            });
        }
        self.notify("restoring the layout...");
    }

    fn on_layout_started(
        &mut self,
        saved: &str,
        req: &NewRequest,
        result: std::result::Result<SessionInfo, SpawnErr>,
        generation: u64,
    ) {
        self.dirty = true;
        let info = match result {
            Ok(info) => info,
            Err(e) => {
                self.view.layout_started(saved, None, generation);
                let why = sanitize_text(&e.message).into_owned();
                return self.notify(&format!("could not start {}: {why}", kind_label(&req.kind)));
            }
        };
        if !self.view.sessions.iter().any(|r| r.id == info.id) {
            self.view.sessions.extend(rows(std::slice::from_ref(&info)));
        }
        self.pending.insert(info.id.clone(), Instant::now());
        self.starts
            .insert(info.id.clone(), (info.cwd.clone(), info.child_pid));
        if registers_shell(req) {
            self.view.shells.push(info.id.clone());
            self.shell_ids.insert(info.id.clone());
        }
        self.view.layout_started(saved, Some(&info.id), generation);
        self.watched = None;
        self.notify(&format!("started {}", sanitize_text(&info.name)));
    }

    fn paste_prompt(&mut self, idx: usize) -> io::Result<()> {
        let Some(text) = self.view.prompts.get(idx).map(|p| p.text.clone()) else {
            return Ok(());
        };
        let Some(row) = self.view.focused_row().filter(|r| !r.exited()) else {
            self.notify("no running session to paste into");
            return Ok(());
        };
        let id = row.id.clone();
        if self.creating.contains_key(&id) {
            self.notify("the session is starting, try again in a moment");
            return Ok(());
        }
        let bytes = paste_bytes(&text, self.bracketed(&id));
        self.send_to(&id, &bytes)?;
        if self.prompt_send {
            self.send_enter = Some((id, Instant::now() + SEND_DELAY));
            self.notify("prompt sent");
        } else {
            self.notify("prompt pasted, press Enter to send");
        }
        Ok(())
    }

    fn save_prompts(&mut self) {
        let Some(dir) = self.prompts_dir.clone() else {
            self.notify("cannot locate the config directory");
            return;
        };
        if attach_prompts::save(&dir, &self.view.prompts).is_err() {
            self.notify("cannot save prompts.toml");
        }
    }

    fn report_attention(&mut self, user_action: bool) {
        if !self.human_time {
            return;
        }
        let target = attention_target(&self.view).map(str::to_string);
        if let Some((session, window)) = self.attention.update(
            Instant::now(),
            target.as_deref(),
            self.window_focused,
            user_action,
        ) {
            let _ = self.writer.attention(session.as_deref(), window);
        }
    }

    fn handle(&mut self, actions: Vec<Action>) -> io::Result<Option<Exit>> {
        let user_action = actions.iter().any(|a| is_user_action(&self.view, a));
        if user_action {
            self.last_user = Instant::now();
        }
        for action in actions {
            if !matches!(action, Action::Mouse(_)) {
                self.dirty = true;
            }
            if self.view.layout_offer.is_some() && self.view.confirm.is_none() {
                let answer = matches!(&action, Action::Forward(d) if offer_answer(d));
                let dismiss = match &action {
                    Action::Forward(_) | Action::Drop(_) | Action::Paste { .. } => !answer,
                    Action::Mouse(ev) => ev.is_left_press(),
                    _ => false,
                };
                if dismiss {
                    self.view.layout_offer = None;
                    self.view.layout_restore = None;
                }
            }
            let routed = route(&self.view, &action);
            if routed != Route::Other {
                self.modal_input(routed, action);
                continue;
            }
            match action {
                Action::FocusIn => {
                    self.focus_seen = true;
                    self.window_focused = true;
                }
                Action::FocusOut => {
                    self.focus_seen = true;
                    self.window_focused = false;
                }
                Action::Forward(data) if self.view.palette.is_some() => {
                    match self.view.edit_palette(&data) {
                        PaletteEdit::Run(entry) => {
                            if let Some(exit) = self.run_palette_entry(entry)? {
                                return Ok(Some(exit));
                            }
                        }
                        PaletteEdit::PromptsChanged => self.save_prompts(),
                        PaletteEdit::LayoutsChanged => self.save_layouts(),
                        PaletteEdit::Editing | PaletteEdit::Close => {}
                    }
                }
                Action::Drop(data) if self.view.palette.is_some() => {
                    self.view.insert_palette_text(&data);
                }
                Action::Paste { data, .. } if self.view.palette.is_some() => {
                    self.view.insert_palette_text(&data);
                }
                Action::Forward(data) if self.view.search.as_ref().is_some_and(|s| s.input) => {
                    let rows = self.page() + 1;
                    match self.view.edit_search(&data, rows) {
                        SearchEdit::Editing => {}
                        SearchEdit::Keep => self.sync_scroll_mode(),
                        SearchEdit::Cancel => {
                            self.view.clear_find();
                            self.view.scroll = None;
                        }
                    }
                    self.dirty = true;
                }
                Action::Drop(_) | Action::Paste { .. }
                    if self.view.search.as_ref().is_some_and(|s| s.input) => {}
                Action::Forward(data) if self.view.rename.is_some() => {
                    match self.view.edit_rename(&data) {
                        RenameEdit::Submit(id, name) => self.rename_session(&id, &name),
                        RenameEdit::Editing | RenameEdit::Done | RenameEdit::Cancel => {}
                    }
                }
                Action::Forward(_) if self.view.copy.is_some() => {}
                Action::Forward(data) if self.view.layout_offer.is_some() => {
                    self.offer_key(&data);
                }
                Action::Forward(data) if self.view.filter_input() => {
                    self.view.edit_filter(&data);
                    self.dirty = true;
                }
                Action::Drop(data) | Action::Paste { data, .. } if self.view.filter_input() => {
                    self.view.insert_filter_text(&data);
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
                            let typed = peer_bytes(&data);
                            for peer in self.broadcast_peers(&id) {
                                self.send_to(&peer, &typed)?;
                            }
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
                        let mut ids = vec![id.clone()];
                        ids.extend(self.broadcast_peers(&id));
                        for id in ids {
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
                }
                Action::Paste { data, start, end } => {
                    if start {
                        self.view.confirm = None;
                        self.leave_overlays();
                        let idx = self.view.paste_target(self.area());
                        self.view.focus_pane(idx);
                        self.paste_to = self.view.focused_session().map(str::to_string);
                        self.paste_peers = match &self.paste_to {
                            Some(id) => self.broadcast_peers(id),
                            None => Vec::new(),
                        };
                    }
                    let Some(id) = self.paste_to.clone() else {
                        continue;
                    };
                    let live = self.broadcast_peers(&id);
                    let dropped: Vec<String> = self
                        .paste_peers
                        .iter()
                        .filter(|p| !live.contains(p))
                        .cloned()
                        .collect();
                    for peer in dropped {
                        self.paste_peers.retain(|p| *p != peer);
                        if self.bracketed(&peer) {
                            self.send_to(&peer, b"\x1b[201~")?;
                        }
                    }
                    let mut ids = vec![id];
                    ids.extend(self.paste_peers.clone());
                    for id in ids {
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
                    }
                    if end {
                        self.paste_to = None;
                        self.paste_peers.clear();
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
                    if ev.is_left_press() && self.view.hit(area, ev.col, ev.row) != Hit::Filter {
                        self.view.end_filter_input();
                    }
                    let out = self.view.on_mouse(area, ev, &info);
                    if self.view.filter_input() {
                        self.sync_scroll_mode();
                    }
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
                    self.sync_review();
                    if let Some(edit) = out.question {
                        self.question_edit(edit);
                    }
                    if out.new_session {
                        self.open_new_session();
                    }
                    if let Some(id) = out.watch {
                        self.rerun_watch(&id);
                    }
                    if out.templates {
                        if let Some(exit) = self.handle(vec![Action::Cmd(Cmd::Templates)])? {
                            return Ok(Some(exit));
                        }
                    }
                    if let Some(id) = out.open_task {
                        self.open_task(&id);
                    }
                    if let Some(action) = out.git {
                        self.git_action(action);
                    }
                    if let Some(action) = out.summary {
                        self.summary_action(action);
                    }
                    if let Some(action) = out.export {
                        self.export_action(action);
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
                    if cmd != Cmd::Git {
                        self.view.git_open = false;
                    }
                    self.view.sidebar_focus = None;
                    self.view.layout_offer = None;
                    if cmd != Cmd::Filter {
                        self.view.end_filter_input();
                    }
                    if let Some(old) = self.view.palette.take() {
                        if matches!(
                            (cmd, &old.mode),
                            (Cmd::Palette, PaletteMode::Commands)
                                | (Cmd::Prompts, PaletteMode::Prompts)
                                | (Cmd::Layouts, PaletteMode::Layouts)
                                | (Cmd::Templates, PaletteMode::Templates)
                        ) {
                            continue;
                        }
                    }
                    if self.view.qmenu.take().is_some() && cmd == Cmd::Questions {
                        continue;
                    }
                    if self.view.search.as_ref().is_some_and(|s| s.input)
                        && !matches!(cmd, Cmd::Search | Cmd::CopyMode)
                    {
                        self.view.clear_find();
                        self.view.scroll = None;
                        self.sync_scroll_mode();
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
                        Cmd::OpenTask => {
                            if let Some(id) = self.view.focused_session().map(str::to_string) {
                                self.open_task(&id);
                            }
                        }
                        Cmd::Summary => self.summary_command(),
                        Cmd::Export => self.export_command(),
                        Cmd::Resume => {
                            if let Some(id) = self.view.focused_session().map(str::to_string) {
                                self.request_resume(&id);
                            }
                        }
                        Cmd::Watch => self.begin_watch(),
                        Cmd::Search => self.begin_search(),
                        Cmd::CopyMode => self.begin_copy(),
                        _ => self.view.apply(cmd),
                    }
                    if let Some(id) = removing {
                        self.remove_session(&id);
                    }
                    if cmd == Cmd::Filter {
                        self.sync_scroll_mode();
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
                    if key == ScrollKey::Search {
                        self.begin_search();
                    } else {
                        self.view.scroll_key(key, page);
                    }
                    self.after_find();
                }
            }
        }
        self.report_attention(user_action);
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
        self.report_attention(false);
        if self
            .send_enter
            .as_ref()
            .is_some_and(|(_, at)| Instant::now() >= *at)
        {
            if let Some((id, _)) = self.send_enter.take() {
                self.send_to(&id, b"\r")?;
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
        self.probe_titles();
        self.probe_chips();
        self.check_signals();
        self.probe_status();
        self.probe_usage();
        self.poll_watchers();
        self.probe_git();
        self.check_exits();
        self.probe_peek();
        self.probe_questions();
        self.paste_forks();
        self.view.forkable = self
            .agents
            .iter()
            .filter(|(_, s)| s.profile == "claude-code")
            .map(|(id, _)| id.clone())
            .collect();
        if self.view.scroll.is_none() {
            self.view.goto = None;
        }
        let now_s = now_ms() / 1000;
        if self.view.now_s / 60 != now_s / 60 && self.view.qmenu.is_some() {
            self.dirty = true;
        }
        if self.view.now_s != now_s
            && (self
                .view
                .status
                .keys()
                .any(|id| self.view.status_line(id).is_some_and(|l| l.clock.is_some()))
                || self.view.chips.values().any(|c| c.started_s.is_some()))
        {
            self.dirty = true;
        }
        self.view.now_s = now_s;
        if self.scroll_mode && self.view.scroll.is_none() {
            self.scroll_mode = false;
            self.splitter.set_scroll_mode(false);
        }
        if self.view.scroll.is_none() {
            if self.view.search.is_some() || self.view.copy.is_some() || self.view.stext.is_some() {
                self.view.clear_find();
                self.dirty = true;
            }
        } else if (self.view.search.is_some() || self.view.copy.is_some())
            && self.text_pending.is_none()
            && self.text_dirty
            && self.text_at.elapsed() >= Duration::from_millis(500)
        {
            self.request_text();
        }
        self.sync_scroll_max();
        let want = watch_ids(self.view.panes(), &self.view.sessions);
        if self.watched.as_ref() != Some(&want) {
            self.writer.set_sessions(&want)?;
            self.watched = Some(want);
        }
        let panes = self.view.panes().to_vec();
        self.sent_sizes.retain(|id, _| panes.contains(id));
        if self.paste_to.as_ref().is_some_and(|id| !panes.contains(id)) {
            self.paste_to = None;
            for peer in std::mem::take(&mut self.paste_peers) {
                if self.bracketed(&peer) {
                    self.send_to(&peer, b"\x1b[201~")?;
                }
            }
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

const DA1_QUERY: &[u8] = b"\x1b[c";

fn take_span(buf: &mut Vec<u8>, start: &[u8], ends: &[&[u8]]) -> Option<Vec<u8>> {
    let from = buf.windows(start.len()).position(|w| w == start)?;
    let tail = &buf[from..];
    let len = ends
        .iter()
        .filter_map(|e| {
            tail.windows(e.len())
                .position(|w| w == *e)
                .map(|p| p + e.len())
        })
        .min()?;
    Some(buf.drain(from..from + len).collect())
}

fn query_background() -> (Option<(u8, u8, u8)>, Vec<u8>) {
    let mut out = io::stdout();
    if out.write_all(OSC11_QUERY).is_err()
        || out.write_all(DA1_QUERY).is_err()
        || out.flush().is_err()
    {
        return (None, Vec::new());
    }
    let deadline = Instant::now() + Duration::from_millis(500);
    let mut buf: Vec<u8> = Vec::new();
    while buf.len() < 512 {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        let mut fds = libc::pollfd {
            fd: libc::STDIN_FILENO,
            events: libc::POLLIN,
            revents: 0,
        };
        let ready = unsafe { libc::poll(&mut fds, 1, left.as_millis() as libc::c_int) };
        if ready < 0 && io::Error::last_os_error().kind() == io::ErrorKind::Interrupted {
            continue;
        }
        if ready <= 0 {
            break;
        }
        let mut chunk = [0u8; 128];
        let n = unsafe { libc::read(libc::STDIN_FILENO, chunk.as_mut_ptr().cast(), chunk.len()) };
        if n <= 0 {
            break;
        }
        buf.extend_from_slice(&chunk[..n as usize]);
        if take_span(&mut buf.clone(), b"\x1b[?", &[b"c"]).is_some() {
            break;
        }
    }
    let _ = take_span(&mut buf, b"\x1b[?", &[b"c"]);
    let osc = take_span(&mut buf, b"\x1b]11;", &[b"\x07", b"\x1b\\"]);
    (osc.as_deref().and_then(parse_osc11), buf)
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
    let attach_cfg = tmgr_core::config::config_dir()
        .ok()
        .and_then(|d| tmgr_core::config::Config::load(&d).ok())
        .map(|c| c.attach)
        .unwrap_or_default();
    view.sidebar_default = attach_cfg.sidebar_width.unwrap_or(view.sidebar_default);
    let layouts_dir = attach_layouts::dir();
    let prompts_dir = attach_prompts::dir();
    let mut notice_set = false;
    if let Some(dir) = &prompts_dir {
        let (list, warn) = attach_prompts::load(dir);
        view.prompts = list;
        view.notice = warn;
        notice_set = view.notice.is_some();
    }
    if let Some(dir) = &layouts_dir {
        let (list, warn) = attach_layouts::load(dir);
        view.layouts = list;
        if view.notice.is_none() {
            view.notice = warn;
            notice_set = view.notice.is_some();
        }
    }
    let layouts_seen = attach_layouts::names(&view.layouts);
    if let Some(dir) = attach_templates::dir() {
        let (list, warn) = attach_templates::load(&dir);
        view.templates = list;
        if view.notice.is_none() {
            view.notice = warn;
            notice_set = view.notice.is_some();
        }
    }
    let web_base = {
        let cfg = tmgr_core::config::config_dir()
            .ok()
            .and_then(|d| tmgr_core::config::Config::load(&d).ok())
            .unwrap_or_default();
        tmgr_core::config::web_base(
            cfg.server
                .as_deref()
                .unwrap_or(tmgr_core::config::DEFAULT_SERVER),
            cfg.web_url.as_deref(),
        )
    };
    if let Some(prefs) = ctl
        .layout_get()
        .ok()
        .and_then(|v| serde_json::from_value::<LayoutPrefs>(v).ok())
    {
        view.apply_prefs(&prefs);
    }
    view.set_initial(initial);
    let seeded = list
        .iter()
        .filter(|s| !s.cwd.is_empty())
        .map(|s| (s.id.clone(), probe(Path::new(&s.cwd))))
        .collect();
    let first = ratatui::crossterm::terminal::size().unwrap_or((view.width, 24));
    view.set_locs(Rect::new(0, 0, first.0, first.1), seeded);
    let attach_client = connect(cfg)?;
    let human_time = attach_client
        .welcome()
        .caps
        .iter()
        .any(|c| c == caps::HUMAN_TIME_V1);
    let (reader, writer) = attach_client.attach(view.panes()).map_err(rerr)?.split();
    let (tx, rx) = mpsc::sync_channel(QUEUE);
    let (prefs_tx, prefs_rx) = mpsc::channel();
    let guard = Guard::enter().map_err(|e| Error::internal(format!("terminal setup: {e}")))?;
    view.theme = match attach_cfg.theme.as_deref().and_then(ThemeKind::parse) {
        Some(kind) => kind,
        None => {
            let (rgb, typed) = query_background();
            if !typed.is_empty() {
                let _ = tx.send(Msg::Stdin(typed));
            }
            rgb.map_or(ThemeKind::Dark, kind_for_background)
        }
    };
    if mouse {
        let mut out = io::stdout();
        let _ = out.write_all(enter_mouse());
        let _ = out.flush();
    }
    if human_time {
        let mut out = io::stdout();
        let _ = out.write_all(focus_on());
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
    let (title_tx, title_rx) = mpsc::sync_channel(1);
    let title_out = tx.clone();
    thread::spawn(move || {
        titler(title_rx, move |found| {
            title_out.send(Msg::Titles(found)).is_ok()
        })
    });
    let (chip_tx, chip_rx) = mpsc::sync_channel(1);
    let chip_out = tx.clone();
    thread::spawn(move || {
        chipper(chip_rx, move |found| {
            chip_out.send(Msg::Chips(found)).is_ok()
        })
    });
    let (peek_tx, peek_rx) = mpsc::sync_channel(1);
    let peek_out = tx.clone();
    thread::spawn(move || {
        peek_worker(peek_rx, move |session, sub, lines| {
            peek_out
                .send(Msg::Peek {
                    session,
                    sub,
                    lines,
                })
                .is_ok()
        })
    });
    let (status_tx, status_rx) = mpsc::sync_channel(1);
    let status_out = tx.clone();
    thread::spawn(move || {
        status_worker(status_rx, move |found| {
            status_out.send(Msg::Status(found)).is_ok()
        })
    });
    let (summary_tx, summary_rx) = mpsc::sync_channel(4);
    let summary_out = tx.clone();
    thread::spawn(move || {
        summarizer(summary_rx, move |done| {
            summary_out.send(Msg::Summary(done)).is_ok()
        })
    });
    let (git_tx, git_rx) = mpsc::sync_channel(1);
    let git_out = tx.clone();
    thread::spawn(move || git_worker(git_rx, move |found| git_out.send(Msg::Git(found)).is_ok()));
    let (usage_tx, usage_rx) = mpsc::sync_channel(1);
    let usage_out = tx.clone();
    thread::spawn(move || {
        usage_worker(usage_source(), usage_rx, move |bar| {
            usage_out.send(Msg::Usage(bar)).is_ok()
        })
    });
    let (question_tx, question_rx) = mpsc::sync_channel(1);
    let question_out = tx.clone();
    let question_dir = cfg.dir.clone();
    thread::spawn(move || {
        questioner(question_rx, question_dir, claude_binary(), move |found| {
            question_out.send(Msg::Questions(found)).is_ok()
        })
    });
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
        human_time,
        window_focused: true,
        focus_seen: false,
        last_user: Instant::now(),
        attention: AttentionTracker::new(),
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
        paste_peers: Vec::new(),
        last_interrupt: None,
        dirty: true,
        size: (0, 0),
        sent_sizes: HashMap::new(),
        prev_modes: None,
        last_prefs: None,
        scrolled: None,
        notice_until: notice_set.then(|| Instant::now() + NOTICE_FOR),
        probe_tx,
        probe_pending: false,
        tried: HashSet::new(),
        last_probe: Instant::now(),
        agents: list
            .iter()
            .filter_map(|s| Some((s.id.clone(), agent_src(s)?)))
            .collect(),
        title_tx,
        title_pending: false,
        titles_tried: HashSet::new(),
        exit_scanned: HashSet::new(),
        last_title: Instant::now(),
        chip_tx,
        chip_pending: false,
        last_chip: Instant::now() - CHIP_EVERY,
        session_tasks: HashMap::new(),
        web_base,
        opener: Box::new(SystemOpener),
        clipboard: Box::new(SystemClipboard),
        text_pending: None,
        text_dirty: false,
        text_at: Instant::now(),
        signals: SignalTracker::new(),
        signal_cfg: signal_settings(&attach_cfg),
        prompt_send: attach_cfg.prompt_send.unwrap_or(false),
        summary_mode: SummaryMode::parse(attach_cfg.summary.as_deref()),
        summary_tx,
        summary_auto: HashMap::new(),
        summary_dir: cfg.dir.join("summaries"),
        running_seen: HashSet::new(),
        claude: claude_binary(),
        send_enter: None,
        prompts_dir,
        layouts_seen,
        layouts_dir,
        notifier: Box::new(SystemNotifier),
        peek_tx,
        peek_pending: false,
        last_peek: Instant::now() - STATUS_EVERY,
        status_tx,
        usage_tx,
        git_tx,
        git_pending: false,
        last_git: Instant::now() - GIT_EVERY,
        usage_pending: false,
        last_usage: Instant::now() - USAGE_EVERY,
        last_watch: Instant::now(),
        status_pending: false,
        last_status: Instant::now() - STATUS_EVERY,
        question_tx,
        question_pending: false,
        last_question: Instant::now() - QUESTIONS_EVERY,
        question_exited: HashSet::new(),
        qstore: Store::new(&cfg.dir),
        forking: HashSet::new(),
        fork_paste: HashMap::new(),
        template_ids: HashMap::new(),
    };
    let flusher = crate::cmd::time::send::Background::start(cfg.dir.clone());
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
    flusher.finish();

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
            Ok(Msg::TaskPrepared { .. }) => *spawning = false,
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum SummaryMode {
    Ask,
    Always,
    Never,
}

impl SummaryMode {
    fn parse(value: Option<&str>) -> SummaryMode {
        match value {
            Some("always") => SummaryMode::Always,
            Some("never") => SummaryMode::Never,
            _ => SummaryMode::Ask,
        }
    }
}

fn create_shell(
    cfg: &RunnerConfig,
    cwds: &[String],
    name: &str,
    argv: Option<Vec<String>>,
    cols: u16,
    rows: u16,
) -> std::result::Result<SessionInfo, String> {
    let mut client = connect(cfg).map_err(|e| sanitize_text(&e.to_string()).into_owned())?;
    let mut last = "no working directory for the new shell".to_string();
    for cwd in cwds {
        let req = CreateReq {
            argv: argv.clone().unwrap_or_else(shell_argv),
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
    #[test]
    fn background_reply_is_cut_out_and_typed_bytes_are_kept() {
        let mut buf = b"ab\x1b]11;rgb:ff/ff/ff\x07cd\x1b[?62;4cef".to_vec();
        assert!(super::take_span(&mut buf, b"\x1b[?", &[b"c"]).is_some());
        let osc = super::take_span(&mut buf, b"\x1b]11;", &[b"\x07", b"\x1b\\"]);
        assert_eq!(osc.unwrap(), b"\x1b]11;rgb:ff/ff/ff\x07");
        assert_eq!(buf, b"abcdef");
        let mut partial = b"\x1b]11;rgb:ff".to_vec();
        assert!(super::take_span(&mut partial, b"\x1b]11;", &[b"\x07"]).is_none());
        assert_eq!(partial, b"\x1b]11;rgb:ff");
    }

    use super::*;
    use tmgr_ipc::api::State;
    use tmgr_runner::attach::{ScrollKey, SidebarItem, SpawnKind};

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
            human_s: None,
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
        view.apply(Cmd::Palette);
        view.insert_palette_text(b"b");
        let PaletteEdit::Run(entry @ PaletteEntry::Session(_)) = view.edit_palette(b"\r") else {
            panic!("expected a session entry");
        };
        assert_eq!(entry, PaletteEntry::Session("b".into()));
        view.open("b");
        assert_eq!(view.focused_session(), Some("b"));
    }

    #[test]
    fn a_session_kind_is_a_shell_an_agent_profile_or_unknown() {
        let mut i = info("n");
        i.argv = vec!["/bin/zsh".into(), "-l".into()];
        i.name = "sh-3".into();
        assert_eq!(session_kind(&i).as_deref(), Some("shell"));
        let mut i = info("n");
        i.profile = Some("claude".into());
        assert_eq!(session_kind(&i).as_deref(), Some("agent:claude"));
        i.profile = None;
        assert_eq!(session_kind(&i), None);
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
                task: None,
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
                task: None,
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

    fn one_session_view() -> ViewState {
        let mut v = ViewState::new(rows(&[info("s1")]), "");
        v.set_initial("s1");
        v
    }

    fn typed() -> Action {
        Action::Forward(b"a".to_vec())
    }

    #[test]
    fn rows_carry_the_human_seconds() {
        let mut i = info("s1");
        i.human_s = Some(125);
        assert_eq!(rows(&[i]).remove(0).you, Some(125));
        assert_eq!(rows(&[info("s2")]).remove(0).you, None);
    }

    #[test]
    fn attention_follows_the_focused_session_and_drops_to_none_in_overlays() {
        let mut v = one_session_view();
        assert_eq!(attention_target(&v), Some("s1"));
        v.help = true;
        assert_eq!(attention_target(&v), None);
        v.help = false;
        v.sidebar_focus = Some(SidebarItem::Session("s1".into()));
        assert_eq!(attention_target(&v), None);
    }

    #[test]
    fn attention_leaves_the_panes_of_a_hidden_tab() {
        let mut v = ViewState::new(rows(&[info("a"), info("b"), info("c")]), "t");
        v.width = 140;
        v.apply(Cmd::Split);
        assert_eq!(v.panes(), ["a", "b"]);
        v.open("c");
        assert_eq!(attention_target(&v), Some("c"));
        v.apply(Cmd::NextTab);
        assert!(matches!(attention_target(&v), Some("a" | "b")));
    }

    #[test]
    fn input_forwarded_to_a_session_is_not_a_user_action() {
        let mut v = one_session_view();
        assert!(!is_user_action(&v, &typed()));
        assert!(!is_user_action(&v, &Action::FocusIn));
        assert!(!is_user_action(&v, &Action::FocusOut));
        assert!(is_user_action(&v, &Action::Cmd(Cmd::Palette)));
        assert!(is_user_action(&v, &Action::Scroll(ScrollKey::PageUp)));
        v.help = true;
        assert!(is_user_action(&v, &typed()));
    }
}
