mod app;
mod data;
mod filters;
mod theme;
mod view;

use crate::cli::BoardArgs;
use crate::ctx::Ctx;
use crate::out::Output;
use app::{App, Effect, Failure, Msg, Ws};
use filters::{Assignee, Filters};
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

struct CloudConn {
    server: String,
    dir: PathBuf,
    account: String,
    preferred: Option<Backend>,
}

impl CloudConn {
    fn connect(&self) -> Result<Client> {
        let store = CredentialStore::new(&self.dir, &self.account, self.preferred);
        let (tokens, _) = store.load()?.ok_or_else(Error::not_logged_in)?;
        Client::new(
            &self.server,
            &Ctx::user_agent(),
            Some(tokens),
            Some(store),
            false,
        )
    }
}

/// How the background worker reaches the board's workspace: the logged-in cloud, or the same
/// local workspace and identity as the board itself.
enum Conn {
    Cloud(CloudConn),
    Local {
        workspace: tmgr_core::local::LocalWorkspace,
        user: tmgr_core::local::User,
    },
}

impl Conn {
    fn connect(&self) -> Result<Client> {
        match self {
            Conn::Cloud(cloud) => cloud.connect(),
            Conn::Local { workspace, user } => Client::for_local(
                tmgr_core::local::Local::new(workspace.clone(), user.clone()),
                &Ctx::user_agent(),
                false,
            ),
        }
    }
}

/// Everything the worker needs to open another workspace or remember filters.
struct Reach {
    cloud: Option<CloudConn>,
    local_root: Option<PathBuf>,
    local_user_id: Option<i64>,
    state: Option<PathBuf>,
}

impl Reach {
    fn connect_local(&self, ws: &Ws) -> Result<Client> {
        let root = self
            .local_root
            .as_deref()
            .ok_or_else(|| Error::internal("cannot locate local workspaces"))?;
        let code = ws
            .code
            .strip_prefix(tmgr_core::local::PREFIX)
            .unwrap_or(&ws.code);
        let workspace = tmgr_core::local::find_at(root, code)?;
        let user = tmgr_core::local::identity(root, self.local_user_id);
        Client::for_local(
            tmgr_core::local::Local::new(workspace, user),
            &Ctx::user_agent(),
            false,
        )
    }

    fn connect_cloud(&self) -> Result<Client> {
        self.cloud
            .as_ref()
            .ok_or_else(Error::not_logged_in)?
            .connect()
    }

    fn workspaces(&self, active: Option<&Client>, cache: &mut Option<Client>) -> Result<Vec<Ws>> {
        let locals: Vec<Ws> = self
            .local_root
            .as_deref()
            .map(tmgr_core::local::list_at)
            .unwrap_or_default()
            .iter()
            .map(|w| Ws::from_workspace(&crate::ctx::local_entry(w)))
            .collect();
        let listed = match active {
            Some(c) => c.workspaces(),
            None => {
                if cache.is_none() {
                    match self.connect_cloud() {
                        Ok(c) => *cache = Some(c),
                        Err(e) => {
                            return if locals.is_empty() {
                                Err(e)
                            } else {
                                Ok(locals)
                            };
                        }
                    }
                }
                cache.as_ref().map_or(Ok(Vec::new()), |c| c.workspaces())
            }
        };
        let mut all = Vec::new();
        match listed {
            Ok(list) => all.extend(list.iter().map(Ws::from_workspace)),
            Err(e) if locals.is_empty() => return Err(e),
            Err(_) => {}
        }
        all.extend(locals);
        Ok(all)
    }
}

fn initial_filters(remembered: Option<Filters>, project: Option<String>, mine: bool) -> Filters {
    let mut filters = remembered.unwrap_or_default();
    if project.is_some() {
        filters.project = project;
    }
    if mine {
        filters.assignee = Assignee::Me;
    }
    filters
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
    let ws = Ws::from_workspace(&ctx.workspace(client)?);
    let state = filters::path(&ctx.dir);
    let saved = filters::load(&state);
    let strict = project.is_some();
    let mut active = initial_filters(
        saved.workspaces.get(&filters::key(&ws.code)).cloned(),
        project,
        args.mine,
    );
    let opened = data::open(client, &ws, active.project.as_deref(), strict)?;
    active.project = opened.project.clone();
    let now = chrono::Utc::now().timestamp();
    let mut app = App::new(opened.ws.clone(), active, saved, opened.me, now);
    app.projects = opened.projects;
    app.apply_snapshot(opened.snapshot);

    let cloud = ctx.server().ok().map(|server| CloudConn {
        account: config::account_for(&server, DEFAULT_PROFILE),
        server,
        dir: ctx.dir.clone(),
        preferred: ctx.cfg.credential_store.as_deref().and_then(Backend::parse),
    });
    let conn = match client.local() {
        Some(local) => Conn::Local {
            workspace: local.workspace().clone(),
            user: local.user().clone(),
        },
        None => match ctx.server() {
            Ok(server) => Conn::Cloud(CloudConn {
                account: config::account_for(&server, DEFAULT_PROFILE),
                server,
                dir: ctx.dir.clone(),
                preferred: ctx.cfg.credential_store.as_deref().and_then(Backend::parse),
            }),
            Err(e) => return Err(e),
        },
    };
    let reach = Reach {
        cloud,
        local_root: tmgr_core::local::root().ok(),
        local_user_id: ctx.cfg.local_user_id,
        state: Some(state),
    };
    let level = ColorLevel::from_env(
        ctx.style.is_off(),
        &std::env::var("COLORTERM").unwrap_or_default(),
        &std::env::var("TERM").unwrap_or_default(),
    );
    let theme = Theme::new(level, args.ascii);
    event_loop(app, &theme, conn, reach, opened.ws.id, opened.category)?;
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
    reach: Reach,
    effects: mpsc::Receiver<Effect>,
    results: mpsc::Sender<Msg>,
) {
    std::thread::spawn(move || {
        let mut ws_id = ws_id;
        let mut category = category;
        let mut local_active = matches!(conn, Conn::Local { .. });
        let mut client = conn.connect();
        let mut cloud: Option<Client> = None;
        for effect in effects {
            let msg = match effect {
                Effect::Open { ws, project } => {
                    let res: Result<(Option<Client>, app::Opened)> = (|| {
                        if ws.local {
                            let c = reach.connect_local(&ws)?;
                            let o = data::open(&c, &ws, project.as_deref(), false)?;
                            Ok((Some(c), o))
                        } else if !local_active {
                            let c = client
                                .as_ref()
                                .map_err(|e| Error::new(e.kind, &e.message))?;
                            Ok((None, data::open(c, &ws, project.as_deref(), false)?))
                        } else {
                            if cloud.is_none() {
                                cloud = Some(reach.connect_cloud()?);
                            }
                            let c = cloud.as_ref().ok_or_else(Error::not_logged_in)?;
                            Ok((None, data::open(c, &ws, project.as_deref(), false)?))
                        }
                    })();
                    match res {
                        Ok((new_local, o)) => {
                            ws_id = ws.id;
                            category = o.category;
                            match new_local {
                                Some(c) => {
                                    if !local_active {
                                        cloud = client.ok();
                                    }
                                    client = Ok(c);
                                    local_active = true;
                                }
                                None if local_active => {
                                    if let Some(c) = cloud.take() {
                                        client = Ok(c);
                                        local_active = false;
                                    }
                                }
                                None => {}
                            }
                            Msg::Opened(Box::new(Ok(o)))
                        }
                        Err(e) => Msg::Opened(Box::new(Err(Failure::new(e.kind, &e.message)))),
                    }
                }
                Effect::Workspaces => {
                    let active = if local_active {
                        None
                    } else {
                        client.as_ref().ok()
                    };
                    Msg::Workspaces(
                        reach
                            .workspaces(active, &mut cloud)
                            .map_err(|e| Failure::new(e.kind, &e.message)),
                    )
                }
                Effect::Save { ws, filters: f } => {
                    if let Some(file) = &reach.state {
                        filters::save(file, &ws, &f);
                    }
                    continue;
                }
                effect => match &client {
                    Ok(c) => data::exec(c, ws_id, category, effect),
                    Err(e) => failed(effect, Failure::new(e.kind, &e.message)),
                },
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
        Effect::Open { .. } => Msg::Opened(Box::new(Err(f))),
        Effect::Workspaces => Msg::Workspaces(Err(f)),
        Effect::Save { .. } => Msg::Loaded(Err(f)),
    }
}

fn event_loop(
    mut app: App,
    theme: &Theme,
    conn: Conn,
    reach: Reach,
    ws_id: i64,
    category: Option<u64>,
) -> Result<()> {
    let (eff_tx, eff_rx) = mpsc::channel();
    let (msg_tx, msg_rx) = mpsc::channel();
    spawn_worker(conn, ws_id, category, reach, eff_rx, msg_tx);
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
