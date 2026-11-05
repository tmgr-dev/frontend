mod indexer;
mod memory;
mod signal;
mod state;
mod view;

#[cfg(test)]
mod tests;

use super::attach_consent::Reads;
use super::catalog::{index, Agent, Live as Liveness};
use super::ops::span;
use super::session_list;
use crate::cmd::board::theme::ColorLevel;
use crate::cmd::ui::{enter_screen, leave_screen, Restore};
use indexer::{Indexer, Searcher};
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::Terminal;
use state::{AgentOpt, Hints, Key, Row, RowKind, Source, State, Step};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{stdin, stdout, IsTerminal};
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

#[derive(Default)]
struct Live {
    indexer: RefCell<Option<Indexer>>,
    searcher: RefCell<Option<Searcher>>,
    loading: RefCell<Option<Receiver<Loaded>>>,
    tmgr: RefCell<Vec<(String, String)>>,
    owners: RefCell<HashMap<String, String>>,
}

impl Live {
    fn start() -> Live {
        let live = Live::default();
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

fn look() -> view::Look {
    let off = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    let colorterm = std::env::var("COLORTERM").unwrap_or_default();
    let term = std::env::var("TERM").unwrap_or_default();
    view::Look {
        level: ColorLevel::from_env(off, &colorterm, &term),
    }
}

fn drive(state: &mut State) -> Option<Step> {
    let look = look();
    let previous = std::sync::Arc::new(std::panic::take_hook());
    let chained = previous.clone();
    std::panic::set_hook(Box::new(move |info| {
        leave_screen();
        chained(info);
    }));
    let signals = signal::Guard::install();
    let outcome = {
        let _restore = Restore;
        enter_screen().ok().and_then(|()| {
            let mut term = Terminal::new(CrosstermBackend::new(stdout())).ok()?;
            loop {
                state.pump();
                let _ = term.draw(|f| view::draw(f, state, look));
                if signal::pending() != 0 {
                    return Some(Step::Quit);
                }
                if !event::poll(Duration::from_millis(200)).unwrap_or(false) {
                    continue;
                }
                match event::read().ok()? {
                    Event::Key(k) if k.kind == KeyEventKind::Press => {
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
