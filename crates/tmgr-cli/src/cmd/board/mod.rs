mod app;
mod data;
mod theme;
mod view;

use crate::cli::BoardArgs;
use crate::ctx::Ctx;
use crate::out::Output;
use app::{App, Effect, Failure, Msg};
use ratatui::backend::CrosstermBackend;
use ratatui::crossterm::cursor::Show;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind,
};
use ratatui::crossterm::execute;
use ratatui::crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::Terminal;
use serde_json::json;
use std::io::{stdin, stdout, IsTerminal};
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;
use theme::{ColorLevel, Theme};
use tmgr_core::api::Client;
use tmgr_core::config::{self, DEFAULT_PROFILE};
use tmgr_core::creds::{Backend, CredentialStore};
use tmgr_core::error::{Error, Result};
use tmgr_core::refs::is_category_code;

struct Conn {
    server: String,
    dir: PathBuf,
    account: String,
    preferred: Option<Backend>,
    verbose: bool,
}

impl Conn {
    fn connect(&self) -> Result<Client> {
        let store = CredentialStore::new(&self.dir, &self.account, self.preferred);
        let (tokens, _) = store.load()?.ok_or_else(Error::not_logged_in)?;
        Client::new(
            &self.server,
            &Ctx::user_agent(),
            Some(tokens),
            Some(store),
            self.verbose,
        )
    }
}

pub fn run(ctx: &Ctx, args: &BoardArgs) -> Result<Output> {
    if ctx.json || !stdin().is_terminal() || !stdout().is_terminal() {
        return Err(Error::usage("tmgr board needs an interactive terminal").hint("tmgr task list"));
    }
    let project = match &args.project {
        Some(code) => {
            let code = code.to_ascii_uppercase();
            if !is_category_code(&code) {
                return Err(Error::usage("invalid project code"));
            }
            Some(code)
        }
        None => None,
    };
    let client = ctx.client()?;
    let ws = ctx.workspace(client)?;
    let category = match &project {
        Some(code) => Some(
            client
                .categories(ws.id)?
                .into_iter()
                .find(|c| {
                    c.code
                        .as_deref()
                        .is_some_and(|x| x.eq_ignore_ascii_case(code))
                })
                .map(|c| c.id)
                .ok_or_else(|| {
                    Error::not_found(format!("project {code} not found")).hint("tmgr ws list")
                })?,
        ),
        None => None,
    };
    let me = if args.mine {
        client.current_device()?.user.id
    } else {
        None
    };
    let snapshot = data::fetch(client, ws.id, category)?;
    let now = chrono::Utc::now().timestamp();
    let mut app = App::new(&ws.name, project.as_deref(), args.mine, me, now);
    app.apply_snapshot(snapshot);

    let server = ctx.server()?;
    let conn = Conn {
        account: config::account_for(&server, DEFAULT_PROFILE),
        server,
        dir: ctx.dir.clone(),
        preferred: ctx.cfg.credential_store.as_deref().and_then(Backend::parse),
        verbose: false,
    };
    let level = ColorLevel::from_env(
        ctx.style.is_off(),
        &std::env::var("COLORTERM").unwrap_or_default(),
        &std::env::var("TERM").unwrap_or_default(),
    );
    let theme = Theme::new(level, args.ascii);
    event_loop(app, &theme, conn, ws.id, category)?;
    Ok(Output::new(json!({}), ""))
}

fn restore() {
    let _ = disable_raw_mode();
    let _ = execute!(stdout(), DisableMouseCapture, LeaveAlternateScreen, Show);
}

struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> Result<TerminalGuard> {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            previous(info);
        }));
        enable_raw_mode()?;
        if let Err(e) = execute!(stdout(), EnterAlternateScreen, EnableMouseCapture) {
            restore();
            return Err(e.into());
        }
        Ok(TerminalGuard)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        restore();
    }
}

fn spawn_worker(
    conn: Conn,
    ws_id: i64,
    category: Option<u64>,
    effects: mpsc::Receiver<Effect>,
    results: mpsc::Sender<Msg>,
) {
    std::thread::spawn(move || {
        let client = conn.connect();
        for effect in effects {
            let msg = match &client {
                Ok(c) => data::exec(c, ws_id, category, effect),
                Err(e) => failed(effect, Failure::new(e.kind, &e.message)),
            };
            if results.send(msg).is_err() {
                break;
            }
        }
    });
}

fn failed(effect: Effect, f: Failure) -> Msg {
    match effect {
        Effect::Refresh => Msg::Loaded(Err(f)),
        Effect::LoadDetail(id) => Msg::Detail(id, Box::new(Err(f))),
        Effect::Move { id, .. } => Msg::MoveDone(id, Err(f)),
        Effect::Timer { id, start } => Msg::TimerDone(id, start, Err(f)),
        Effect::Comment { id, .. } => Msg::CommentDone(id, Err(f)),
    }
}

fn event_loop(
    mut app: App,
    theme: &Theme,
    conn: Conn,
    ws_id: i64,
    category: Option<u64>,
) -> Result<()> {
    let (eff_tx, eff_rx) = mpsc::channel();
    let (msg_tx, msg_rx) = mpsc::channel();
    spawn_worker(conn, ws_id, category, eff_rx, msg_tx);
    let guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let width = terminal.size()?.width;
    let mut pending = app.update(Msg::Resize(width));
    pending.extend(app.update(Msg::Tick(chrono::Utc::now().timestamp())));
    while !app.quit {
        for effect in pending.drain(..) {
            if eff_tx.send(effect).is_err() {
                app.fatal = Some(Failure::new(
                    tmgr_core::error::Kind::Internal,
                    "background worker stopped",
                ));
                app.quit = true;
            }
        }
        terminal.draw(|f| view::draw(f, &app, theme))?;
        if event::poll(Duration::from_millis(250))? {
            loop {
                let msg = match event::read()? {
                    Event::Key(k) if k.kind == KeyEventKind::Press => Some(Msg::Key(k)),
                    Event::Mouse(m) => Some(Msg::Mouse(m)),
                    Event::Resize(w, _) => Some(Msg::Resize(w)),
                    _ => None,
                };
                if let Some(m) = msg {
                    pending.extend(app.update(m));
                }
                if !event::poll(Duration::ZERO)? {
                    break;
                }
            }
        }
        while let Ok(m) = msg_rx.try_recv() {
            pending.extend(app.update(m));
        }
        pending.extend(app.update(Msg::Tick(chrono::Utc::now().timestamp())));
    }
    drop(terminal);
    drop(guard);
    match app.fatal {
        Some(f) => {
            let mut e = Error::new(f.kind, &f.message);
            if f.kind == tmgr_core::error::Kind::Auth {
                e = e.hint("tmgr login");
            }
            Err(e)
        }
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests;
