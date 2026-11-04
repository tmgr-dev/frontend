mod memory;
mod state;
mod view;

#[cfg(test)]
mod tests;

use crate::cmd::board::theme::ColorLevel;
use crate::cmd::ui::{enter_screen, leave_screen, Restore};
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::Terminal;
use state::{AgentOpt, Hints, Key, Row, Source, State, Step};
use std::ffi::OsString;
use std::io::{stdin, stdout, IsTerminal};
use tmgr_runner::profile::{self, ProfileSet};

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
    let source = Box::new(Live);
    let memory_path = memory::default_path();
    let remembered = memory_path.as_deref().map(memory::load).unwrap_or_default();
    let folder = std::env::current_dir()
        .map(|p| {
            p.to_string_lossy()
                .chars()
                .filter(|c| !c.is_control())
                .collect()
        })
        .unwrap_or_default();
    let mut state = State::new(agents(), folder, &remembered, source);
    let step = drive(&mut state);
    settle(&state, step, memory_path.as_deref())
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

struct Live;

impl Source for Live {
    fn rows(&self, profile: &str) -> (Vec<Row>, Hints) {
        load_rows(profile)
    }

    fn search(&self, query: &str) -> Option<Vec<String>> {
        search_ids(query)
    }

    fn allow_reading(&self) -> Result<(), String> {
        allow_log_reading()
    }
}

pub fn load_rows(_profile: &str) -> (Vec<Row>, Hints) {
    (Vec::new(), Hints::default())
}

pub fn search_ids(_query: &str) -> Option<Vec<String>> {
    None
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
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        leave_screen();
        previous(info);
    }));
    let outcome = {
        let _restore = Restore;
        enter_screen().ok().and_then(|()| {
            let mut term = Terminal::new(CrosstermBackend::new(stdout())).ok()?;
            loop {
                let _ = term.draw(|f| view::draw(f, state, look));
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
    let _ = std::panic::take_hook();
    outcome
}
