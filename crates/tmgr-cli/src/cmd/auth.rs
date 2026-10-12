use crate::cli::{LoginArgs, VERSION};
use crate::ctx::Ctx;
use crate::out::{table, Output};
use serde_json::json;
use std::io::{IsTerminal, Read};
use std::time::{Duration, Instant};
use tmgr_core::api::{Client, DevicePoll, DEFAULT_SCOPES};
use tmgr_core::config::Config;
use tmgr_core::creds::{Backend, Tokens};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::models::DeviceInfo;
use tmgr_core::redact::Secret;
use tmgr_core::text::sanitize_line;

const MAX_TOKEN_LEN: usize = 4096;

pub fn login(ctx: &Ctx, args: &LoginArgs) -> Result<Output> {
    let server = ctx.server()?;
    let (client, saved_backend, warning) = if args.with_token {
        login_with_token(ctx, &server)?
    } else {
        login_device(ctx, args, &server)?
    };
    let info = client.current_device()?;
    persist_login_settings(ctx, saved_backend, &server)?;
    if let Some(w) = warning {
        ctx.warn("credential_store_fallback", w);
    }
    let name = sanitize_line(&info.user.name);
    let human = format!(
        "{} Logged in as {} ({})\n{}",
        ctx.style.good("\u{2713}"),
        name,
        info.scopes.join(", "),
        ctx.style
            .muted(&format!("token stored in: {}", saved_backend.label()))
    );
    Ok(Output::new(
        whoami_data(&server, &info, saved_backend, None),
        human,
    ))
}

fn persist_login_settings(ctx: &Ctx, backend: Backend, server: &str) -> Result<()> {
    let mut cfg: Config = Config::load(&ctx.dir)?;
    let mut changed = false;
    if cfg.credential_store.as_deref() != Some(backend.as_str()) {
        cfg.credential_store = Some(backend.as_str().to_string());
        changed = true;
    }
    if ctx.server_is_overridden() && cfg.server.as_deref() != Some(server) {
        cfg.server = Some(server.to_string());
        changed = true;
    }
    if changed {
        cfg.save(&ctx.dir)?;
    }
    Ok(())
}

fn login_with_token(ctx: &Ctx, server: &str) -> Result<(Client, Backend, Option<String>)> {
    let token = read_token()?;
    let tokens = Tokens {
        access: Secret::new(token),
        refresh: None,
        expires_at: None,
    };
    let client = Client::new(
        server,
        &Ctx::user_agent(),
        Some(tokens.clone()),
        None,
        ctx.verbose,
    )?;
    client.current_device()?;
    let saved = ctx.store()?.save(&tokens)?;
    Ok((client, saved.backend, saved.warning))
}

fn read_token() -> Result<String> {
    let raw = if std::io::stdin().is_terminal() {
        rpassword::prompt_password("Token: ")?
    } else {
        let mut buf = String::new();
        std::io::stdin()
            .lock()
            .take(MAX_TOKEN_LEN as u64 * 2)
            .read_to_string(&mut buf)
            .map_err(|_| Error::usage("could not read a token from stdin"))?;
        buf
    };
    let token = raw.trim().to_string();
    if token.is_empty()
        || token.len() > MAX_TOKEN_LEN
        || !token.chars().all(|c| c.is_ascii_graphic())
    {
        return Err(Error::usage("the token on stdin is empty or malformed"));
    }
    Ok(token)
}

fn login_device(
    ctx: &Ctx,
    args: &LoginArgs,
    server: &str,
) -> Result<(Client, Backend, Option<String>)> {
    let scopes = parse_scopes(args.scopes.as_deref())?;
    let label = parse_label(&args.label)?;
    let _ = ctrlc::set_handler(|| {
        eprintln!();
        std::process::exit(130);
    });
    let client = ctx.anonymous_client()?;
    let dc = client.device_code(&format!("tmgr {VERSION}"), &label, &scopes)?;
    let url = verification_url(
        &dc.verification_uri_complete
            .clone()
            .unwrap_or_else(|| dc.verification_uri.clone()),
    )?;
    let code = user_code(&dc.user_code)?;
    eprintln!("Open this URL to sign in:  {url}");
    eprintln!(
        "Confirm the code:          {} (expires in {} min)",
        ctx.err_style.bold(&code),
        (dc.expires_in / 60).max(1)
    );
    if !args.no_browser {
        open_browser(&url);
    }
    eprintln!(
        "{}",
        ctx.err_style
            .muted("Waiting for approval... (Ctrl-C to cancel)")
    );
    let deadline = Instant::now() + Duration::from_secs(dc.expires_in);
    let mut interval = dc.interval;
    let (tokens, _granted) = loop {
        if Instant::now() >= deadline {
            return Err(expired());
        }
        std::thread::sleep(Duration::from_secs(interval));
        match client.device_token(&dc.device_code)? {
            DevicePoll::Pending => {}
            DevicePoll::SlowDown => interval += 5,
            DevicePoll::Expired => return Err(expired()),
            DevicePoll::Denied => {
                return Err(Error::new(Kind::Forbidden, "the login request was denied"))
            }
            DevicePoll::Granted(t, s) => break (t, s),
        }
    };
    let saved = ctx.store()?.save(&tokens)?;
    let authed = Client::new(server, &Ctx::user_agent(), Some(tokens), None, ctx.verbose)?;
    Ok((authed, saved.backend, saved.warning))
}

fn expired() -> Error {
    Error::auth("the login code expired").hint("tmgr login")
}

pub fn parse_scopes(raw: Option<&str>) -> Result<Vec<String>> {
    let Some(raw) = raw else {
        return Ok(DEFAULT_SCOPES.iter().map(|s| s.to_string()).collect());
    };
    let scopes: Vec<String> = raw
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    let valid = |s: &String| {
        s.len() <= 40
            && s.starts_with(|c: char| c.is_ascii_lowercase())
            && s.chars().all(|c| {
                c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, ':' | '_' | '-')
            })
    };
    if scopes.is_empty() || scopes.len() > 20 || !scopes.iter().all(valid) {
        return Err(Error::usage("invalid --scopes list")
            .hint("example: --scopes tasks:read,workspaces:read"));
    }
    Ok(scopes)
}

fn parse_label(raw: &str) -> Result<String> {
    let label = sanitize_line(raw.trim());
    if label.is_empty() || label.chars().count() > 64 {
        return Err(Error::usage("--label must be 1 to 64 characters"));
    }
    Ok(label)
}

fn verification_url(raw: &str) -> Result<String> {
    let bad = || Error::internal("the server returned an invalid verification URL");
    let url = tmgr_core::Url::parse(raw).map_err(|_| bad())?;
    let loopback = url.host_str().is_some_and(tmgr_core::config::is_loopback);
    let scheme_ok = url.scheme() == "https" || (url.scheme() == "http" && loopback);
    if !scheme_ok || url.host_str().is_none() || !url.username().is_empty() {
        return Err(bad());
    }
    Ok(url.to_string())
}

fn user_code(raw: &str) -> Result<String> {
    if (4..=16).contains(&raw.len()) && raw.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        Ok(raw.to_string())
    } else {
        Err(Error::internal("the server returned an invalid user code"))
    }
}

fn open_browser(url: &str) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else {
        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            eprintln!(
                "no browser found on this machine (headless); open the URL on another device"
            );
            return;
        }
        "xdg-open"
    };
    let _ = std::process::Command::new(opener)
        .arg(url)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

pub fn logout(ctx: &Ctx) -> Result<Output> {
    let store = ctx.store()?;
    let loaded = store.load()?;
    let mut remote_revoked = false;
    if let Some((tokens, _)) = loaded.as_ref() {
        let client = Client::new(
            &ctx.server()?,
            &Ctx::user_agent(),
            Some(tokens.clone()),
            None,
            ctx.verbose,
        )?;
        match client.logout() {
            Ok(()) => remote_revoked = true,
            Err(e) => ctx.warn(
                "remote_logout_failed",
                format!(
                    "could not revoke the token on the server ({}); revoke it under Signed-in devices",
                    e.message
                ),
            ),
        }
    }
    store.delete()?;
    let was_logged_in = loaded.is_some();
    let human = if was_logged_in {
        format!("{} Logged out", ctx.style.good("\u{2713}"))
    } else {
        ctx.style.muted("Not logged in.")
    };
    Ok(Output::new(
        json!({"logged_out": was_logged_in, "remote_revoked": remote_revoked}),
        human,
    ))
}

pub fn whoami(ctx: &Ctx) -> Result<Output> {
    let server = ctx.server()?;
    let client = ctx.client()?;
    let backend = ctx.backend().ok_or_else(Error::not_logged_in)?;
    let info = client.current_device()?;
    let expires = client.token_expires_at();
    let who = match &info.user.email {
        Some(e) => format!("{} ({})", sanitize_line(&info.user.name), sanitize_line(e)),
        None => sanitize_line(&info.user.name),
    };
    let rows = vec![
        vec!["user".to_string(), who],
        vec!["server".to_string(), server.clone()],
        vec!["scopes".to_string(), info.scopes.join(", ")],
        vec![
            "token expires".to_string(),
            info.expires_at
                .clone()
                .or(expires.clone())
                .map(|s| sanitize_line(&s))
                .unwrap_or_else(|| "unknown".into()),
        ],
        vec!["credential store".to_string(), backend.label().to_string()],
    ];
    Ok(Output::new(
        whoami_data(&server, &info, backend, expires),
        table(&rows),
    ))
}

fn whoami_data(
    server: &str,
    info: &DeviceInfo,
    backend: Backend,
    fallback_expiry: Option<String>,
) -> serde_json::Value {
    json!({
        "user": {"id": info.user.id, "name": sanitize_line(&info.user.name), "email": info.user.email},
        "server": server,
        "scopes": info.scopes,
        "expires_at": info.expires_at.clone().or(fallback_expiry),
        "credential_store": backend.as_str(),
        "device_id": info.device_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_are_validated() {
        assert_eq!(parse_scopes(None).unwrap().len(), 7);
        assert_eq!(
            parse_scopes(Some("tasks:read, workspaces:read")).unwrap(),
            vec!["tasks:read", "workspaces:read"]
        );
        for bad in ["*", "", "Tasks:read", "a b;c", "tasks:read,*", "../x"] {
            assert!(parse_scopes(Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn verification_urls_must_be_https_or_loopback() {
        assert!(verification_url("https://tmgr.dev/device?code=ABCD-EFGH").is_ok());
        assert!(verification_url("http://127.0.0.1:1/device").is_ok());
        assert!(verification_url("http://evil.example/device").is_err());
        assert!(verification_url("javascript:alert(1)").is_err());
        assert!(verification_url("-flag").is_err());
    }

    #[test]
    fn user_codes_are_validated() {
        assert!(user_code("HJQK-7PXM").is_ok());
        assert!(user_code("\u{1b}[31m").is_err());
        assert!(user_code("").is_err());
    }
}
