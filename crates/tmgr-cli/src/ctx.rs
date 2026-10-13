use crate::cli::{Cli, VERSION};
use crate::out::{stderr_style, stdout_style, Style};
use std::cell::{Cell, OnceCell, RefCell};
use std::path::PathBuf;
use tmgr_core::api::{Client, Warning};
use tmgr_core::config::{self, Config, DEFAULT_PROFILE};
use tmgr_core::creds::{Backend, CredentialStore};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::local;
use tmgr_core::models::Workspace;
use tmgr_core::refs::is_workspace_code;

pub struct Ctx {
    pub json: bool,
    pub quiet: bool,
    pub yes: bool,
    pub verbose: bool,
    pub style: Style,
    pub err_style: Style,
    pub ws: Option<String>,
    pub dir: PathBuf,
    pub cfg: Config,
    server_override: Option<String>,
    warnings: RefCell<Vec<Warning>>,
    client: OnceCell<Client>,
    cloud: OnceCell<Client>,
    backend: Cell<Option<Backend>>,
    workspaces: OnceCell<Vec<Workspace>>,
    switched: RefCell<Option<Workspace>>,
}

impl Ctx {
    pub fn new(cli: &Cli) -> Result<Ctx> {
        let dir = config::config_dir()?;
        let cfg = Config::load(&dir)?;
        Ok(Ctx {
            json: cli.json,
            quiet: cli.quiet,
            yes: cli.yes,
            verbose: cli.verbose,
            style: stdout_style(cli.no_color),
            err_style: stderr_style(cli.no_color),
            ws: cli.ws.clone(),
            dir,
            cfg,
            server_override: cli.server.clone(),
            warnings: RefCell::new(Vec::new()),
            client: OnceCell::new(),
            cloud: OnceCell::new(),
            backend: Cell::new(None),
            workspaces: OnceCell::new(),
            switched: RefCell::new(None),
        })
    }

    pub fn user_agent() -> String {
        format!("tmgr/{VERSION}")
    }

    pub fn warn(&self, code: &str, message: impl Into<String>) {
        self.warnings.borrow_mut().push(Warning::new(code, message));
    }

    pub fn take_warnings(&self) -> Vec<Warning> {
        let mut all = std::mem::take(&mut *self.warnings.borrow_mut());
        for c in [self.client.get(), self.cloud.get()].into_iter().flatten() {
            all.extend(c.take_warnings());
        }
        all
    }

    pub fn server(&self) -> Result<String> {
        self.cfg.effective_server(self.server_override.as_deref())
    }

    pub fn server_is_overridden(&self) -> bool {
        self.server_override.is_some()
    }

    pub fn store(&self) -> Result<CredentialStore> {
        let account = config::account_for(&self.server()?, DEFAULT_PROFILE);
        let preferred = self
            .cfg
            .credential_store
            .as_deref()
            .and_then(Backend::parse);
        Ok(CredentialStore::new(&self.dir, &account, preferred))
    }

    /// The client for the selected workspace: a local one for `local:<code>` (no login), else the cloud.
    pub fn client(&self) -> Result<&Client> {
        if let Some(c) = self.client.get() {
            return Ok(c);
        }
        if let Some(code) = self
            .wanted_workspace()?
            .as_deref()
            .and_then(local::local_code)
        {
            let workspace = local::find(code)?;
            let user = local::identity(&local::root()?, self.cfg.local_user_id);
            let client = Client::for_local(
                local::Local::new(workspace, user),
                &Self::user_agent(),
                self.verbose,
            )?;
            return Ok(self.client.get_or_init(|| client));
        }
        self.cloud_client()
    }

    /// `local:<code>` when the selected workspace is local.
    pub fn local_tag(&self) -> Result<Option<String>> {
        Ok(self
            .wanted_workspace()?
            .filter(|w| local::local_code(w).is_some()))
    }

    /// The client for a run recorded in `workspace` (`local:<code>`, or none for the cloud), which
    /// need not be the selected workspace.
    pub fn client_for_run(&self, workspace: Option<&str>) -> Result<RunClient<'_>> {
        if workspace == self.local_tag()?.as_deref() {
            return self.client().map(RunClient::Shared);
        }
        match workspace.and_then(local::local_code) {
            Some(code) => Ok(RunClient::Own(Box::new(local::client(
                code,
                self.cfg.local_user_id,
                &Self::user_agent(),
                self.verbose,
            )?))),
            None => self.cloud_client().map(RunClient::Shared),
        }
    }

    /// The logged-in cloud client, whatever workspace is selected (auth and notifications).
    pub fn cloud_client(&self) -> Result<&Client> {
        if let Some(c) = self.cloud.get() {
            return Ok(c);
        }
        let store = self.store()?;
        let (tokens, backend) = store.load()?.ok_or_else(Error::not_logged_in)?;
        self.backend.set(Some(backend));
        let client = Client::new(
            &self.server()?,
            &Self::user_agent(),
            Some(tokens),
            Some(store),
            self.verbose,
        )?;
        Ok(self.cloud.get_or_init(|| client))
    }

    pub fn backend(&self) -> Option<Backend> {
        self.backend.get()
    }

    pub fn anonymous_client(&self) -> Result<Client> {
        Client::new(
            &self.server()?,
            &Self::user_agent(),
            None,
            None,
            self.verbose,
        )
    }

    pub fn web_base(&self) -> Result<String> {
        Ok(config::web_base(
            &self.server()?,
            self.cfg.web_url.as_deref(),
        ))
    }

    pub fn workspaces(&self, client: &Client) -> Result<&[Workspace]> {
        if let Some(w) = self.workspaces.get() {
            return Ok(w);
        }
        let list = match client.local() {
            Some(_) => local_workspaces()?,
            None => client.workspaces()?,
        };
        Ok(self.workspaces.get_or_init(|| list))
    }

    /// The web link of a task; local workspaces have none.
    pub fn task_link(&self, workspace_code: &str, id: u64) -> Result<Option<String>> {
        if workspace_code.starts_with(local::PREFIX) {
            return Ok(None);
        }
        Ok(Some(tmgr_core::view::task_url(
            &self.web_base()?,
            workspace_code,
            id,
        )))
    }

    pub fn wanted_workspace(&self) -> Result<Option<String>> {
        match self.ws.as_deref().or(self.cfg.default_workspace.as_deref()) {
            None => Ok(None),
            Some(code) if is_workspace_code(code) => Ok(Some(code.to_string())),
            Some(code) if local::local_code(code).is_some() => Ok(Some(code.to_string())),
            Some(_) => Err(Error::usage("invalid workspace code")
                .hint("a workspace code has letters, digits, - and _")),
        }
    }

    pub fn switch_workspace(&self, ws: Workspace) {
        *self.switched.borrow_mut() = Some(ws);
    }

    pub fn workspace(&self, client: &Client) -> Result<Workspace> {
        if let Some(ws) = self.switched.borrow().clone() {
            return Ok(ws);
        }
        let list = self.workspaces(client)?;
        match self.wanted_workspace()? {
            Some(code) => list
                .iter()
                .find(|w| w.code.eq_ignore_ascii_case(&code))
                .cloned()
                .ok_or_else(|| {
                    Error::not_found(format!("workspace {code} not found")).hint("tmgr ws list")
                }),
            None => list
                .iter()
                .find(|w| w.is_default && !w.local)
                .or_else(|| list.first())
                .cloned()
                .ok_or_else(|| Error::not_found("no workspaces available")),
        }
    }

    pub fn soft<T>(&self, result: Result<T>, code: &str, what: &str) -> Result<Option<T>> {
        match result {
            Ok(v) => Ok(Some(v)),
            Err(e) if matches!(e.kind, Kind::Forbidden | Kind::NotFound) => {
                self.warn(code, format!("{what} is not available ({})", e.message));
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }
}

/// Local workspaces as `local:<code>` entries of the workspace list.
pub fn local_workspaces() -> Result<Vec<Workspace>> {
    Ok(local::list()?
        .into_iter()
        .map(|w| Workspace {
            id: w.manifest.id,
            name: w.manifest.name,
            code: format!("{}{}", local::PREFIX, w.manifest.code),
            is_default: false,
            local: true,
        })
        .collect())
}

pub enum RunClient<'a> {
    Shared(&'a Client),
    Own(Box<Client>),
}

impl std::ops::Deref for RunClient<'_> {
    type Target = Client;

    fn deref(&self) -> &Client {
        match self {
            RunClient::Shared(c) => c,
            RunClient::Own(c) => c,
        }
    }
}
