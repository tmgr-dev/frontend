use crate::ctx::Ctx;
use std::path::PathBuf;
use tmgr_core::api::{map_error, Client, Warning};
use tmgr_core::config::{self, Config, DEFAULT_PROFILE};
use tmgr_core::creds::{Backend, CredentialStore};
use tmgr_core::error::{Error, Result};
use tmgr_core::notify::{
    self, Fail, LegacyEnv, NotifyCred, NotifyHttp, ENV_STOP_MIN, ENV_TOKEN, ENV_URL, NOTIFY_SCOPE,
};
use tmgr_core::redact::Secret;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Env,
    Legacy,
    Keychain,
}

impl Source {
    pub fn hint(self) -> &'static str {
        match self {
            Source::Env | Source::Legacy => {
                "check TMGR_NOTIFY_TOKEN, or unset it and run `tmgr login`"
            }
            Source::Keychain => "run the command again to mint a fresh notify token",
        }
    }
}

pub struct Resolved {
    pub token: Secret,
    pub server: String,
    pub source: Source,
}

pub struct Session {
    pub http: NotifyHttp,
    pub source: Source,
}

impl Session {
    pub fn error(&self, fail: Fail) -> Error {
        fail.into_error(self.source.hint())
    }
}

#[derive(Clone)]
pub struct NotifyEnv {
    pub dir: PathBuf,
    pub cfg: Config,
    pub server_override: Option<String>,
    pub verbose: bool,
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

fn plausible_token(t: &str) -> bool {
    t.len() <= 256 && t.bytes().all(|b| b.is_ascii_graphic())
}

impl NotifyEnv {
    pub fn from_ctx(ctx: &Ctx) -> NotifyEnv {
        NotifyEnv {
            dir: ctx.dir.clone(),
            cfg: ctx.cfg.clone(),
            server_override: ctx
                .server_is_overridden()
                .then(|| ctx.server().ok())
                .flatten(),
            verbose: ctx.verbose,
        }
    }

    pub fn load(server_override: Option<String>, verbose: bool) -> Result<NotifyEnv> {
        let dir = config::config_dir()?;
        let cfg = Config::load(&dir)?;
        Ok(NotifyEnv {
            dir,
            cfg,
            server_override,
            verbose,
        })
    }

    pub fn server(&self) -> Result<String> {
        self.cfg.effective_server(self.server_override.as_deref())
    }

    fn preferred(&self) -> Option<Backend> {
        self.cfg
            .credential_store
            .as_deref()
            .and_then(Backend::parse)
    }

    pub fn login_store(&self, server: &str) -> CredentialStore {
        let account = config::account_for(server, DEFAULT_PROFILE);
        CredentialStore::new(&self.dir, &account, self.preferred())
    }

    pub fn notify_store(&self, server: &str) -> CredentialStore {
        let account = notify::notify_account(&config::account_for(server, DEFAULT_PROFILE));
        CredentialStore::new(&self.dir, &account, self.preferred())
    }

    pub fn stop_min_minutes(&self) -> f64 {
        let legacy = self.legacy_values(&mut Vec::new());
        let raw = env_nonempty(ENV_STOP_MIN)
            .or_else(|| legacy.and_then(|m| m.get(ENV_STOP_MIN).cloned()));
        notify::stop_min_minutes(raw.as_deref())
    }

    fn legacy_values(
        &self,
        warnings: &mut Vec<Warning>,
    ) -> Option<std::collections::BTreeMap<String, String>> {
        let path = notify::legacy_env_path()?;
        match notify::read_legacy_env(&path) {
            LegacyEnv::Values(v) => Some(v),
            LegacyEnv::Missing => None,
            LegacyEnv::Insecure => {
                warnings.push(Warning::new(
                    "legacy_env_ignored",
                    "~/.config/tmgr-notify/env was ignored: it must be a regular file, mode 0600, owned by you",
                ));
                None
            }
        }
    }

    fn foreign_server(
        &self,
        legacy: Option<&std::collections::BTreeMap<String, String>>,
    ) -> Result<String> {
        match env_nonempty(ENV_URL).or_else(|| legacy.and_then(|m| m.get(ENV_URL).cloned())) {
            Some(url) => notify::server_from_env_url(&url),
            None => self.server(),
        }
    }

    pub fn find_existing(&self, warnings: &mut Vec<Warning>) -> Result<Option<Resolved>> {
        let env_token = env_nonempty(ENV_TOKEN);
        let legacy = if env_token.is_some() && env_nonempty(ENV_URL).is_some() {
            None
        } else {
            self.legacy_values(warnings)
        };
        if let Some(t) = env_token {
            if !plausible_token(&t) {
                return Err(Error::usage("TMGR_NOTIFY_TOKEN is malformed"));
            }
            return Ok(Some(Resolved {
                token: Secret::new(t),
                server: self.foreign_server(legacy.as_ref())?,
                source: Source::Env,
            }));
        }
        if let Some(t) = legacy
            .as_ref()
            .and_then(|m| m.get(ENV_TOKEN))
            .filter(|t| !t.is_empty())
        {
            if !plausible_token(t) {
                return Err(Error::usage(
                    "the token in ~/.config/tmgr-notify/env is malformed",
                ));
            }
            return Ok(Some(Resolved {
                token: Secret::new(t.clone()),
                server: self.foreign_server(legacy.as_ref())?,
                source: Source::Legacy,
            }));
        }
        let server = self.server()?;
        Ok(self
            .notify_store(&server)
            .load()?
            .map(|(tokens, _)| Resolved {
                token: NotifyCred::from_tokens(&tokens).token,
                server,
                source: Source::Keychain,
            }))
    }

    pub fn resolve(&self, warnings: &mut Vec<Warning>) -> Result<Resolved> {
        match self.find_existing(warnings)? {
            Some(r) => Ok(r),
            None => self.mint(&self.server()?, warnings),
        }
    }

    pub fn mint(&self, server: &str, warnings: &mut Vec<Warning>) -> Result<Resolved> {
        let login = self.login_store(server);
        let (tokens, _) = login.load()?.ok_or_else(|| {
            Error::auth("no notify token and not logged in")
                .hint("run `tmgr login --scopes <defaults>,notify:send`, or set TMGR_NOTIFY_TOKEN")
        })?;
        let client = Client::new(
            server,
            &Ctx::user_agent(),
            Some(tokens),
            Some(self.login_store(server)),
            self.verbose,
        )?;
        let store = self.notify_store(server);
        for attempt in 0..2 {
            let guard = store.lock()?;
            if let Some((t, _)) = store.load()? {
                return Ok(Resolved {
                    token: NotifyCred::from_tokens(&t).token,
                    server: server.to_string(),
                    source: Source::Keychain,
                });
            }
            let (status, body) = client.call(
                "POST",
                "/api/notify-tokens",
                Some(&notify::mint_body()),
                false,
            )?;
            if status == 401 && attempt == 0 {
                drop(guard);
                client.current_device()?;
                continue;
            }
            if !(200..300).contains(&status) {
                return Err(map_error(status, &body, NOTIFY_SCOPE));
            }
            let cred = notify::parse_minted(&body)?;
            let saved = store.save_locked(&guard, &cred.to_tokens())?;
            if let Some(w) = saved.warning {
                warnings.push(Warning::new("credential_store_fallback", w));
            }
            return Ok(Resolved {
                token: cred.token,
                server: server.to_string(),
                source: Source::Keychain,
            });
        }
        Err(Error::auth("could not mint a notify token").hint("tmgr login"))
    }

    pub fn forget(&self, r: &Resolved) {
        if r.source == Source::Keychain {
            let _ = self.notify_store(&r.server).delete();
        }
    }

    pub fn with_http<T>(
        &self,
        warnings: &mut Vec<Warning>,
        f: impl FnMut(&NotifyHttp) -> std::result::Result<T, Fail>,
    ) -> Result<(T, Session)> {
        self.with_http_raw(warnings, f).map_err(|e| match e {
            Failure::Net(fail, source) => fail.into_error(source.hint()),
            Failure::Other(e) => e,
        })
    }

    pub fn with_http_raw<T>(
        &self,
        warnings: &mut Vec<Warning>,
        mut f: impl FnMut(&NotifyHttp) -> std::result::Result<T, Fail>,
    ) -> std::result::Result<(T, Session), Failure> {
        let mut r = self.resolve(warnings).map_err(Failure::Other)?;
        let mut retried = false;
        loop {
            let http = NotifyHttp::new(&r.server, r.token.expose(), &Ctx::user_agent())
                .map_err(Failure::Other)?;
            match f(&http) {
                Ok(v) => {
                    return Ok((
                        v,
                        Session {
                            http,
                            source: r.source,
                        },
                    ))
                }
                Err(Fail::Unauthorized) if r.source == Source::Keychain && !retried => {
                    retried = true;
                    self.forget(&r);
                    r = self
                        .mint(&r.server.clone(), warnings)
                        .map_err(Failure::Other)?;
                }
                Err(e) => return Err(Failure::Net(e, r.source)),
            }
        }
    }
}

pub enum Failure {
    Net(Fail, Source),
    Other(Error),
}

pub fn revoke_on_logout(ctx: &Ctx, client: Option<&Client>) {
    let env = NotifyEnv::from_ctx(ctx);
    let Ok(server) = env.server() else { return };
    let store = env.notify_store(&server);
    let Ok(Some((tokens, _))) = store.load() else {
        return;
    };
    let cred = NotifyCred::from_tokens(&tokens);
    if let (Some(client), Some(id)) = (client, cred.id.as_deref()) {
        let outcome = client.call("DELETE", &format!("/api/notify-tokens/{id}"), None, true);
        let ok = matches!(outcome, Ok((s, _)) if (200..300).contains(&s) || s == 404);
        if !ok {
            ctx.warn(
                "notify_token_not_revoked",
                "could not revoke the notify token on the server; revoke it under Agent notifications",
            );
        }
    }
    let _ = store.delete();
}

pub fn drop_stale_notify_token(ctx: &Ctx, server: &str) {
    let old = ctx
        .store()
        .ok()
        .and_then(|st| st.load().ok().flatten())
        .and_then(|(tokens, _)| {
            Client::new(
                server,
                &Ctx::user_agent(),
                Some(tokens),
                ctx.store().ok(),
                ctx.verbose,
            )
            .ok()
        });
    revoke_on_logout(ctx, old.as_ref());
}
