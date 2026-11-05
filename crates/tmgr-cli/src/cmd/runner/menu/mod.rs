mod indexer;
mod memory;
mod rank;
mod signal;
mod smart;
mod state;
mod view;

#[cfg(test)]
mod shared;
#[cfg(test)]
mod smart_tests;
#[cfg(test)]
mod tests;

use super::attach_consent::Reads;
use super::catalog::{index, Agent, Live as Liveness};
use super::ops::span;
use super::session_list;
use crate::cmd::board::theme::ColorLevel;
use crate::cmd::ui::{enter_screen, leave_screen, Restore};
use indexer::{Indexer, Searcher};
pub(crate) use memory::{dialog_path, load_dialog, save_dialog, DialogMemory};
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::Terminal;
use state::{AgentOpt, Consent, Hints, Picks, Row, RowKind, Source};
pub(super) use state::{Key, Pick, State, Step};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{stdin, stdout, IsTerminal};
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;
use tmgr_runner::profile::{self, ProfileSet};
use tmgr_usage::paths::Env;

pub enum Entry {
    Skip,
    Argv(Vec<String>),
    Exit(i32),
}

pub fn entry() -> Entry {
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let tty = stdin().is_terminal() && stdout().is_terminal();
    if !wanted(&args, tty, super::attach_new::in_agent()) {
        return Entry::Skip;
    }
    let source = Box::new(Live::start());
    let memory_path = memory::default_path();
    let remembered = memory_path.as_deref().map(memory::load).unwrap_or_default();
    let mut state = State::new(agents(), current_folder(), &remembered, source);
    let step = drive(&mut state);
    match signal::pending() {
        0 => settle(&state, step, memory_path.as_deref()),
        sig => Entry::Exit(128 + sig),
    }
}

fn current_folder() -> String {
    std::env::current_dir()
        .ok()
        .and_then(|p| p.to_str().map(str::to_string))
        .filter(|p| !p.chars().any(char::is_control))
        .unwrap_or_default()
}

fn settle(state: &State, step: Option<Step>, memory_path: Option<&std::path::Path>) -> Entry {
    match step {
        Some(Step::Start(argv)) => {
            if let Some(path) = memory_path {
                let _ = memory::save(path, &state.memory());
            }
            Entry::Argv(argv)
        }
        Some(_) => Entry::Exit(0),
        None => Entry::Skip,
    }
}

fn wanted(args: &[OsString], tty: bool, in_agent: bool) -> bool {
    args == ["run"] && tty && !in_agent
}

type Loaded = (Vec<Row>, Hints, HashMap<String, String>);

const ROW_LIMIT: usize = 1000;
const HIT_LIMIT: usize = 500;
const RUNNER_WAIT: Duration = Duration::from_secs(2);

#[derive(Clone)]
struct Recent {
    row_id: String,
    index_id: String,
    label: String,
}

#[derive(Default)]
struct Live {
    runner: Option<PathBuf>,
    reads: Cell<Reads>,
    recent: RefCell<Vec<Recent>>,
    job: RefCell<Option<smart::Job>>,
    indexer: RefCell<Option<Indexer>>,
    searcher: RefCell<Option<Searcher>>,
    loading: RefCell<Option<Receiver<Loaded>>>,
    tmgr: RefCell<Vec<(String, String)>>,
    owners: RefCell<HashMap<String, String>>,
}

impl Live {
    fn start() -> Live {
        let live = Live {
            runner: smart::find_claude(std::env::var_os("PATH")),
            ..Live::default()
        };
        live.ensure_indexer();
        live
    }

    fn ensure_indexer(&self) {
        let mut slot = self.indexer.borrow_mut();
        if slot.is_some() {
            return;
        }
        let env = Env::from_process();
        if !Reads::of(&env).any() {
            return;
        }
        if let Some(path) = session_list::index_file() {
            *slot = Some(Indexer::start(path, env));
        }
    }

    fn translate(&self, ids: Vec<String>, query: &str) -> Vec<String> {
        let owners = self.owners.borrow();
        let mut out: Vec<String> = Vec::new();
        for id in ids {
            let id = owners.get(&id).cloned().unwrap_or(id);
            if !out.contains(&id) {
                out.push(id);
            }
        }
        let needle = query.to_lowercase();
        for (id, haystack) in self.tmgr.borrow().iter() {
            if haystack.contains(&needle) && !out.contains(id) {
                out.push(id.clone());
            }
        }
        out
    }
}

impl Source for Live {
    fn load(&self, profile: &str) {
        let (tx, rx) = mpsc::channel();
        let profile = profile.to_string();
        std::thread::spawn(move || {
            let _ = tx.send(load_rows(&profile));
        });
        *self.loading.borrow_mut() = Some(rx);
    }

    fn poll(&self) -> Option<(Vec<Row>, Hints)> {
        let received = self.loading.borrow().as_ref()?.try_recv();
        let (rows, hints, owners) = match received {
            Ok(loaded) => loaded,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Default::default(),
        };
        *self.loading.borrow_mut() = None;
        *self.tmgr.borrow_mut() = rows
            .iter()
            .filter(|r| r.kind == RowKind::Tmgr)
            .map(|r| {
                (
                    r.id.clone(),
                    format!("{} {}", r.project, r.text).to_lowercase(),
                )
            })
            .collect();
        *self.recent.borrow_mut() = rows
            .iter()
            .filter(|r| r.readable)
            .map(|r| Recent {
                row_id: r.id.clone(),
                index_id: owners
                    .iter()
                    .find(|(_, row)| **row == r.id)
                    .map_or_else(|| r.id.clone(), |(session, _)| session.clone()),
                label: format!("{} - {}", r.project, r.text),
            })
            .collect();
        self.reads.set(Reads::of(&Env::from_process()));
        *self.owners.borrow_mut() = owners;
        Some((rows, hints))
    }

    fn search(&self, query: &str) -> Option<Vec<String>> {
        let guard = self.indexer.borrow();
        let indexer = guard.as_ref().filter(|i| i.ready())?;
        let stamp = indexer.stamp();
        let mut slot = self.searcher.borrow_mut();
        let searcher = slot.get_or_insert_with(|| {
            Searcher::start(indexer.path().to_path_buf(), Env::from_process())
        });
        searcher.ask(query, stamp, indexer.complete());
        let answer = searcher.latest().filter(|a| a.query == query)?;
        answer.ids.map(|ids| self.translate(ids, query))
    }

    fn allow_reading(&self) -> Result<(), String> {
        allow_log_reading()?;
        self.indexer.borrow_mut().take();
        self.searcher.borrow_mut().take();
        self.ensure_indexer();
        Ok(())
    }

    fn status(&self) -> Option<String> {
        self.indexer.borrow().as_ref()?.status()
    }

    fn smart_agent(&self, browsed: &str) -> Option<String> {
        self.runner.as_ref()?;
        let reads = self.reads.get();
        let allowed = match browsed {
            "claude-code" => reads.claude,
            "codex" => reads.codex,
            _ => false,
        };
        allowed.then(|| "Claude Code".to_string())
    }

    fn smart_consent(&self, browsed: &str) -> Consent {
        let from = match browsed {
            "claude-code" => return Consent::Allowed,
            "codex" => "Codex",
            _ => return Consent::Declined,
        };
        match memory::sends_path().and_then(|p| memory::load_send(&p, "codex_to_claude")) {
            Some(true) => Consent::Allowed,
            Some(false) => Consent::Declined,
            None => Consent::Ask {
                from,
                to: "Claude Code",
                reset: memory::sends_path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
            },
        }
    }

    fn smart_remember(&self, browsed: &str, allow: bool) {
        if browsed == "codex" {
            if let Some(path) = memory::sends_path() {
                let _ = memory::save_send(&path, "codex_to_claude", allow);
            }
        }
    }

    fn smart_start(&self, browsed: &str, question: &str) -> Result<(), String> {
        let bin = self
            .runner
            .clone()
            .ok_or("No agent is available for smart search.")?;
        let path = {
            let guard = self.indexer.borrow();
            let indexer = guard
                .as_ref()
                .filter(|i| i.ready())
                .ok_or("The search index is not ready yet. Try again in a moment.")?;
            indexer.path().to_path_buf()
        };
        let (browsed, question) = (browsed.to_string(), question.to_string());
        let recent = self.recent.borrow().clone();
        self.job.borrow_mut().take();
        let job = smart::spawn(bin, smart::TIMEOUT, true, move || {
            prepare(&Env::from_process(), &path, &browsed, &question, &recent)
        })?;
        *self.job.borrow_mut() = Some(job);
        Ok(())
    }

    fn smart_poll(&self) -> Option<Result<Picks, String>> {
        let done = self.job.borrow().as_ref()?.poll()?;
        *self.job.borrow_mut() = None;
        let (out, allowed) = match done {
            Ok(a) => a,
            Err(e) => return Some(Err(e)),
        };
        let picks = match smart::parse(&out, &allowed) {
            Ok(p) => p,
            Err(e) => return Some(Err(e)),
        };
        let recent = self.recent.borrow();
        Some(Ok(picks
            .into_iter()
            .filter_map(|p| {
                let row = recent.iter().find(|r| r.index_id == p.id)?;
                Some((row.row_id.clone(), p.reason))
            })
            .collect()))
    }

    fn smart_cancel(&self) {
        *self.job.borrow_mut() = None;
    }
}

fn runner_rows_within(agent: Agent) -> Vec<session_list::Row> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(session_list::runner_rows(Some(agent), &mut Vec::new()));
    });
    rx.recv_timeout(RUNNER_WAIT).unwrap_or_default()
}

pub fn load_rows(profile: &str) -> Loaded {
    let agent = match profile {
        "claude-code" => Agent::Claude,
        "codex" => Agent::Codex,
        _ => return Default::default(),
    };
    let runner = runner_rows_within(agent);
    let (rows, hints) = session_list::recent(&Env::from_process(), Some(agent), ROW_LIMIT, runner);
    let owners = rows
        .iter()
        .filter(|r| r.source == "tmgr")
        .filter_map(|r| Some((r.agent_session.clone()?, r.id.clone())))
        .collect();
    let now = super::catalog::now_ms();
    let rows = rows
        .iter()
        .map(|r| Row {
            id: r.id.clone(),
            kind: if r.source == "tmgr" {
                RowKind::Tmgr
            } else {
                RowKind::External
            },
            sandboxed: r.sandboxed,
            project: r.project_label(),
            when: span(now.saturating_sub(r.when_ms) / 1000),
            name: r.title().map(str::to_string),
            text: r.text(),
            live: r.live == Liveness::Yes,
            readable: r.readable,
            folder_gone: r.folder_gone,
            bypassed: r.bypassed,
            maybe_live: r.live == Liveness::Recent,
        })
        .collect();
    (
        rows,
        Hints {
            reading_off: !hints.is_empty(),
        },
        owners,
    )
}

const CONSENT_OFF: &str = "Reading session logs is off for this agent, so nothing was sent.";

fn reads_on(env: &Env, browsed: &str) -> Result<(), String> {
    let reads = Reads::of(env);
    let on = match browsed {
        "claude-code" => reads.claude,
        "codex" => reads.codex,
        _ => false,
    };
    on.then_some(()).ok_or_else(|| CONSENT_OFF.to_string())
}

fn prepare(
    env: &Env,
    path: &std::path::Path,
    browsed: &str,
    question: &str,
    recent: &[Recent],
) -> Result<smart::Prepared, String> {
    reads_on(env, browsed)?;
    let ix = index::Index::open_read_only(path)
        .map_err(|_| "The search index is unavailable.".to_string())?;
    let found = ix
        .fragments(
            browsed,
            &smart::keywords(question),
            smart::CANDIDATES,
            smart::PER_SESSION,
            smart::FRAGMENT_CHARS,
        )
        .unwrap_or_default();
    let mut candidates: Vec<smart::Candidate> = found
        .into_iter()
        .filter_map(|(id, fragments)| {
            let label = recent.iter().find(|r| r.index_id == id)?.label.clone();
            Some(smart::Candidate {
                id,
                label,
                fragments,
            })
        })
        .collect();
    for r in recent {
        if candidates.len() >= smart::FLOOR {
            break;
        }
        if !candidates.iter().any(|c| c.id == r.index_id) {
            candidates.push(smart::Candidate {
                id: r.index_id.clone(),
                label: r.label.clone(),
                fragments: Vec::new(),
            });
        }
    }
    if candidates.is_empty() {
        return Err("There are no sessions to search.".into());
    }
    let prepared = smart::build_prompt(question, &candidates, &smart::nonce());
    reads_on(env, browsed)?;
    Ok(prepared)
}

pub fn search_ids(ix: &index::Index, query: &str, complete: bool) -> Option<Vec<String>> {
    index::match_query(query)?;
    let hits = ix.search(query, HIT_LIMIT).ok()?;
    (complete || !hits.is_empty()).then(|| hits.into_iter().map(|h| h.session_id).collect())
}

pub fn allow_log_reading() -> Result<(), String> {
    use tmgr_usage::consent::{ConsentChange, ReadLogs};
    let usage = crate::cmd::usage::Usage::open().map_err(|e| e.message)?;
    let change = ConsentChange {
        state: Some(ReadLogs::Granted),
        ..Default::default()
    };
    usage
        .store
        .change_log_consent(&change, chrono::Utc::now())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn on_path(program: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let runnable = |p: &std::path::Path| {
        std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    };
    if program.contains('/') {
        return runnable(std::path::Path::new(program));
    }
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|d| runnable(&d.join(program))))
}

fn agents() -> Vec<AgentOpt> {
    let (set, _) = ProfileSet::load(profile::user_dir().as_deref());
    [("claude-code", "Claude Code"), ("codex", "Codex")]
        .into_iter()
        .map(|(id, name)| AgentOpt {
            id: id.into(),
            name: name.into(),
            installed: set
                .get(id)
                .and_then(|p| p.command.first())
                .is_some_and(|c| on_path(c)),
            bypass: set.get(id).is_some_and(|p| p.bypass_args().is_some()),
        })
        .collect()
}

fn key_of(code: KeyCode, mods: KeyModifiers) -> Option<Key> {
    let ctrl = mods.contains(KeyModifiers::CONTROL);
    Some(match code {
        KeyCode::Char('c') if ctrl => Key::CtrlC,
        KeyCode::Char('a') if ctrl => Key::CtrlA,
        KeyCode::Char(_) if ctrl => return None,
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Tab => Key::Tab,
        KeyCode::Backspace => Key::Backspace,
        _ => return None,
    })
}

pub(super) enum Fed {
    Stay,
    Close,
    Pick(Option<Pick>),
}

fn overlay(
    agents: Vec<AgentOpt>,
    folder: String,
    kind: Option<&str>,
    source: Box<dyn Source>,
) -> State {
    let remembered = memory::Memory {
        agent: kind.unwrap_or_default().to_string(),
        ..memory::Memory::default()
    };
    let mut state = State::new(agents, folder, &remembered, source);
    state.on_key(Key::Enter);
    state
}

pub(super) fn open_in_attach(kind: Option<&str>) -> State {
    overlay(agents(), current_folder(), kind, Box::new(Live::start()))
}

fn utf8_len(lead: u8) -> usize {
    match lead {
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => 1,
    }
}

fn escape_key(data: &[u8]) -> (Option<Key>, usize) {
    match data.get(1) {
        None | Some(0x1b) => (Some(Key::Esc), 1),
        Some(b'[') => {
            let mut i = 2;
            while data.get(i).is_some_and(|b| (0x30..=0x3f).contains(b)) {
                i += 1;
            }
            while data.get(i).is_some_and(|b| (0x20..=0x2f).contains(b)) {
                i += 1;
            }
            match data.get(i) {
                Some(b'A') => (Some(Key::Up), i + 1),
                Some(b'B') => (Some(Key::Down), i + 1),
                Some(b) if (0x40..=0x7e).contains(b) => (None, i + 1),
                _ => (None, data.len()),
            }
        }
        Some(b'O') => match data.get(2) {
            Some(b'A') => (Some(Key::Up), 3),
            Some(b'B') => (Some(Key::Down), 3),
            _ => (None, data.len().min(3)),
        },
        Some(b']' | b'P' | b'_' | b'^' | b'X') => {
            let end = (2..data.len()).find_map(|i| match data[i] {
                0x07 => Some(i + 1),
                0x1b if data.get(i + 1) == Some(&b'\\') => Some(i + 2),
                _ => None,
            });
            (None, end.unwrap_or(data.len()))
        }
        Some(_) => (None, 2),
    }
}

pub(super) fn keys_from_bytes(data: &[u8]) -> Vec<Key> {
    let mut keys = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let rest = &data[i..];
        let (key, used) = match rest[0] {
            0x1b => escape_key(rest),
            b'\r' | b'\n' => (Some(Key::Enter), 1),
            b'\t' => (Some(Key::Tab), 1),
            0x7f | 0x08 => (Some(Key::Backspace), 1),
            0x03 => (Some(Key::CtrlC), 1),
            0x01 => (Some(Key::CtrlA), 1),
            b if b < 0x20 => (None, 1),
            b => {
                let n = utf8_len(b).min(rest.len());
                match std::str::from_utf8(&rest[..n])
                    .ok()
                    .and_then(|t| t.chars().next())
                {
                    Some(c) if !c.is_control() => (Some(Key::Char(c)), n),
                    Some(_) => (None, n),
                    None => (None, 1),
                }
            }
        };
        keys.extend(key);
        i += used;
    }
    keys
}

pub(super) fn feed_keys(state: &mut State, data: &[u8]) -> Fed {
    let keys = keys_from_bytes(data);
    if state.confirming() && keys.len() > 1 {
        return Fed::Stay;
    }
    for key in keys {
        match state.on_key(key) {
            Step::Stay => {}
            Step::Quit => return Fed::Close,
            Step::Start(_) => return Fed::Pick(state.pick()),
        }
    }
    Fed::Stay
}

pub(super) fn feed_paste(state: &mut State, data: &[u8]) {
    let text = String::from_utf8_lossy(data)
        .replace("\x1b[200~", "")
        .replace("\x1b[201~", "");
    state.on_paste(&text);
}

pub(super) fn kill_smart() {
    smart::kill_running();
}

pub(super) fn draw_overlay(f: &mut ratatui::Frame, s: &State) {
    let area = view::overlay_rect(f.area());
    view::draw_in(f, area, s, look(), view::ATTACH);
}

fn look() -> view::Look {
    let off = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    let colorterm = std::env::var("COLORTERM").unwrap_or_default();
    let term = std::env::var("TERM").unwrap_or_default();
    view::Look {
        level: ColorLevel::from_env(off, &colorterm, &term),
    }
}

const MAX_POLL_ERRORS: u32 = 5;

#[derive(Default)]
struct Failures(u32);

impl Failures {
    fn reset(&mut self) {
        self.0 = 0;
    }

    fn exhausted(&mut self) -> bool {
        self.0 += 1;
        self.0 >= MAX_POLL_ERRORS
    }
}

fn drive(state: &mut State) -> Option<Step> {
    let look = look();
    let previous = std::sync::Arc::new(std::panic::take_hook());
    let chained = previous.clone();
    std::panic::set_hook(Box::new(move |info| {
        smart::kill_running();
        leave_screen();
        chained(info);
    }));
    let signals = signal::Guard::install();
    let outcome = {
        let _restore = Restore;
        enter_screen().ok().and_then(|()| {
            let mut term = Terminal::new(CrosstermBackend::new(stdout())).ok()?;
            let mut failures = Failures::default();
            loop {
                state.pump();
                state.tick();
                let _ = term.draw(|f| view::draw(f, state, look));
                state.mark_drawn();
                if signal::pending() != 0 {
                    return Some(Step::Quit);
                }
                match event::poll(Duration::from_millis(200)) {
                    Ok(false) => {
                        failures.reset();
                        continue;
                    }
                    Ok(true) => failures.reset(),
                    Err(_) if failures.exhausted() => return Some(Step::Quit),
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(50));
                        continue;
                    }
                }
                let Ok(ev) = event::read() else {
                    return Some(Step::Quit);
                };
                match ev {
                    Event::Key(k) if k.kind == KeyEventKind::Press => {
                        if state.confirming() && event::poll(Duration::ZERO).unwrap_or(false) {
                            while event::poll(Duration::ZERO).unwrap_or(false) {
                                let _ = event::read();
                            }
                            continue;
                        }
                        if let Some(key) = key_of(k.code, k.modifiers) {
                            match state.on_key(key) {
                                Step::Stay => {}
                                done => return Some(done),
                            }
                        }
                    }
                    Event::Paste(text) => state.on_paste(&text),
                    _ => {}
                }
            }
        })
    };
    drop(signals);
    std::panic::set_hook(Box::new(move |info| previous(info)));
    outcome
}
