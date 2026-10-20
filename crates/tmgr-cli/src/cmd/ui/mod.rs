mod app;
mod cmdline;
mod fuzzy;
mod groups;
mod guard;
mod history;
mod model;
mod view;

#[cfg(test)]
mod tests;

use crate::cli::{Cli, Command};
use crate::cmd::board::theme::ColorLevel;
use crate::out::{self, Style};
use app::{Action, App, Done};
use clap::CommandFactory;
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::cursor::Show;
use ratatui::crossterm::event::{self, Event, KeyEventKind};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::Terminal;
use std::io::{stdin, stdout, IsTerminal, Write};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::Duration;
use tmgr_core::error::{Error, Result};

pub fn agent_session() -> bool {
    tmgr_core::local::agent_session_detected()
        || tmgr_link::privileged::RUN_ENV
            .iter()
            .any(|k| std::env::var_os(k).is_some())
}

pub fn all_tty() -> bool {
    stdin().is_terminal() && stdout().is_terminal() && std::io::stderr().is_terminal()
}

pub fn allowed(agent: bool, tty: bool) -> Result<()> {
    tmgr_link::privileged::check(agent, tty)
}

pub fn bare(args: usize) -> bool {
    args <= 1
}

pub fn entry(cli: &Cli) -> Option<i32> {
    let explicit = matches!(cli.command, Some(Command::Ui));
    let menu = cli.command.is_none()
        && bare(std::env::args_os().len())
        && allowed(agent_session(), all_tty()).is_ok();
    if !explicit && !menu {
        return None;
    }
    if explicit {
        if let Err(e) = allowed(agent_session(), all_tty()) {
            return Some(out::print_error(
                cli.json,
                cli.verbose,
                out::stderr_style(cli.no_color),
                &e,
            ));
        }
    }
    Some(match run(cli) {
        Ok(()) => 0,
        Err(e) => out::print_error(false, cli.verbose, Style::off(), &e),
    })
}

type Term = Terminal<CrosstermBackend<std::io::Stdout>>;

struct Restore;

impl Drop for Restore {
    fn drop(&mut self) {
        leave_screen();
    }
}

fn leave_screen() {
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), LeaveAlternateScreen, Show);
}

fn enter_screen() -> std::io::Result<()> {
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen)
}

fn look(cli: &Cli) -> view::Look {
    let off = cli.no_color || std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
    let colorterm = std::env::var("COLORTERM").unwrap_or_default();
    let term = std::env::var("TERM").unwrap_or_default();
    view::Look {
        level: ColorLevel::from_env(off, &colorterm, &term),
    }
}

fn header_probe() -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let out = std::process::Command::new(exe)
            .args(["status", "--no-color"])
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output();
        if let Ok(o) = out {
            if o.status.success() {
                let text = String::from_utf8_lossy(&o.stdout);
                if let Some(line) = text.lines().find(|l| !l.trim().is_empty()) {
                    let _ = tx.send(line.trim().to_string());
                }
            }
        }
    });
    rx
}

fn run(cli: &Cli) -> Result<()> {
    let root = model::build(&Cli::command());
    let hist = history::default_path();
    let recents = hist.as_deref().map(history::load).unwrap_or_default();
    let mut app = App::new(root, recents);
    app.header = format!("tmgr {}", crate::cli::VERSION);
    let probe = header_probe();
    let look = look(cli);
    let _ = ctrlc::set_handler(|| {});
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        leave_screen();
        previous(info);
    }));
    enter_screen().map_err(|e| Error::internal(format!("cannot open the terminal: {e}")))?;
    let _restore = Restore;
    let mut term: Term = Terminal::new(CrosstermBackend::new(stdout()))
        .map_err(|e| Error::internal(format!("cannot open the terminal: {e}")))?;
    loop {
        if let Ok(line) = probe.try_recv() {
            app.header = format!("tmgr {} \u{b7} {}", crate::cli::VERSION, line);
        }
        let _ = term.draw(|f| view::draw(f, &app, look));
        if !event::poll(Duration::from_millis(150)).unwrap_or(false) {
            continue;
        }
        let Ok(ev) = event::read() else { break };
        let Event::Key(key) = ev else { continue };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match app.on_key(key) {
            None => {}
            Some(Action::Quit) => break,
            Some(Action::Copy(text)) => {
                app.notice = Some(if copy(&text) {
                    "Command copied".into()
                } else {
                    "Could not copy: select the command with the mouse".into()
                });
            }
            Some(Action::Run { node, values, argv }) => {
                let done = run_child(&mut term, &app, node, values, argv, hist.as_deref());
                if let Some(path) = hist.as_deref() {
                    app.recents = history::load(path);
                }
                app.finish(done);
            }
        }
    }
    Ok(())
}

fn run_child(
    term: &mut Term,
    app: &App,
    node: model::Node,
    values: cmdline::Values,
    argv: Vec<String>,
    hist: Option<&std::path::Path>,
) -> Done {
    leave_screen();
    let shown = match cmdline::build_argv(&app.root, &node, &values, true) {
        Ok(masked) => cmdline::render(&masked),
        Err(_) => cmdline::render(&node.path),
    };
    println!("$ {shown}\n");
    let _ = stdout().flush();
    let code = std::env::current_exe()
        .ok()
        .and_then(|exe| std::process::Command::new(exe).args(&argv).status().ok())
        .map(|s| s.code().unwrap_or(1));
    if let Some(path) = hist {
        history::record(path, &node, &values);
    }
    println!(
        "\n[{}] Press Enter to return to the menu.",
        match code {
            Some(c) => format!("exit {c}"),
            None => "could not start".to_string(),
        }
    );
    let _ = stdout().flush();
    let mut line = String::new();
    let _ = stdin().read_line(&mut line);
    let _ = enter_screen();
    let _ = term.clear();
    Done {
        node,
        values,
        command: shown,
        code,
    }
}

fn copy(text: &str) -> bool {
    let tools: [(&str, &[&str]); 3] = [
        ("pbcopy", &[]),
        ("wl-copy", &[]),
        ("xclip", &["-selection", "clipboard"]),
    ];
    for (tool, args) in tools {
        let spawned = std::process::Command::new(tool)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(mut child) = spawned {
            if let Some(mut input) = child.stdin.take() {
                let _ = input.write_all(text.as_bytes());
            }
            if child.wait().is_ok_and(|s| s.success()) {
                return true;
            }
        }
    }
    use base64::Engine;
    let osc = format!(
        "\x1b]52;c;{}\x07",
        base64::engine::general_purpose::STANDARD.encode(text)
    );
    let mut out = stdout();
    out.write_all(osc.as_bytes()).is_ok() && out.flush().is_ok()
}
