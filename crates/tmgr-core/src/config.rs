use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

pub const DEFAULT_SERVER: &str = "https://api.tmgr.dev";
pub const DEFAULT_PROFILE: &str = "default";
pub const KEYS: [&str; 5] = [
    "server",
    "default_workspace",
    "web_url",
    "branch_template",
    "review_status",
];
pub const DEFAULT_REVIEW_STATUS: &str = "Review";

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq)]
pub struct Config {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_workspace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_template: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_store: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub projects: BTreeMap<String, String>,
}

pub fn config_dir() -> Result<PathBuf> {
    config_dir_from(
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )
}

fn config_dir_from(
    xdg: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
) -> Result<PathBuf> {
    if let Some(x) = xdg.map(PathBuf::from).filter(|p| p.is_absolute()) {
        return Ok(x.join("tmgr"));
    }
    let home = home
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .ok_or_else(|| Error::internal("cannot locate the config directory (HOME is not set)"))?;
    Ok(home.join(".config").join("tmgr"))
}

pub fn config_path(dir: &Path) -> PathBuf {
    dir.join("config.toml")
}

impl Config {
    pub fn load(dir: &Path) -> Result<Config> {
        let path = config_path(dir);
        match fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).map_err(|e| {
                Error::usage(format!("invalid config file: {}", e.message()))
                    .hint(format!("fix or remove {}", path.display()))
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, dir: &Path) -> Result<()> {
        ensure_private_dir(dir)?;
        let text = toml::to_string_pretty(self)
            .map_err(|_| Error::internal("cannot serialize the config"))?;
        write_atomic(&config_path(dir), text.as_bytes(), 0o644)
    }

    pub fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(match key {
            "server" => self.server.clone(),
            "default_workspace" => self.default_workspace.clone(),
            "web_url" => self.web_url.clone(),
            "branch_template" => self.branch_template.clone(),
            "review_status" => self.review_status.clone(),
            _ => return Err(unknown_key(key)),
        })
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<String> {
        match key {
            "server" => {
                let v = normalize_server(value)?;
                self.server = Some(v.clone());
                Ok(v)
            }
            "web_url" => {
                let v = normalize_server(value)?;
                self.web_url = Some(v.clone());
                Ok(v)
            }
            "default_workspace" => {
                if !crate::refs::is_workspace_code(value) {
                    return Err(Error::usage("invalid workspace code")
                        .hint("a workspace code has letters, digits, - and _"));
                }
                self.default_workspace = Some(value.to_string());
                Ok(value.to_string())
            }
            "branch_template" => {
                crate::branch::validate_template(value)?;
                self.branch_template = Some(value.to_string());
                Ok(value.to_string())
            }
            "review_status" => {
                let v = crate::text::sanitize_line(value).trim().to_string();
                if v.is_empty() || v.chars().count() > 64 {
                    return Err(Error::usage("invalid review_status")
                        .hint("a status name of up to 64 characters"));
                }
                self.review_status = Some(v.clone());
                Ok(v)
            }
            _ => Err(unknown_key(key)),
        }
    }

    pub fn branch_template(&self) -> &str {
        self.branch_template
            .as_deref()
            .unwrap_or(crate::branch::DEFAULT_TEMPLATE)
    }

    pub fn review_status(&self) -> &str {
        self.review_status
            .as_deref()
            .unwrap_or(DEFAULT_REVIEW_STATUS)
    }

    pub fn list(&self) -> Vec<(&'static str, Option<String>)> {
        KEYS.iter()
            .map(|k| (*k, self.get(k).ok().flatten()))
            .collect()
    }

    pub fn effective_server(&self, override_value: Option<&str>) -> Result<String> {
        let raw = override_value
            .or(self.server.as_deref())
            .unwrap_or(DEFAULT_SERVER);
        normalize_server(raw)
    }
}

fn unknown_key(key: &str) -> Error {
    Error::usage(format!(
        "unknown or read-only config key {:?}",
        key.chars().take(40).collect::<String>()
    ))
    .hint(format!("settable keys: {}", KEYS.join(", ")))
}

pub fn normalize_server(raw: &str) -> Result<String> {
    let bad = |why: &str| {
        Error::usage(format!("invalid server URL: {why}"))
            .hint("use https://host (http is allowed only for localhost)")
    };
    let url = reqwest::Url::parse(raw.trim()).map_err(|_| bad("not a URL"))?;
    let host = url.host_str().ok_or_else(|| bad("no host"))?.to_string();
    if !url.username().is_empty() || url.password().is_some() {
        return Err(bad("credentials in the URL are not allowed"));
    }
    if url.query().is_some() || url.fragment().is_some() || url.path() != "/" {
        return Err(bad("expected scheme://host[:port] only"));
    }
    match url.scheme() {
        "https" => {}
        "http" if is_loopback(&host) => {}
        _ => return Err(bad("plain http is only allowed for localhost")),
    }
    let mut origin = format!("{}://{}", url.scheme(), host);
    if let Some(port) = url.port() {
        origin.push_str(&format!(":{port}"));
    }
    Ok(origin)
}

pub fn is_loopback(host: &str) -> bool {
    matches!(host, "localhost" | "127.0.0.1" | "[::1]" | "::1")
}

pub fn account_for(server: &str, profile: &str) -> String {
    let authority = server.split_once("://").map(|(_, r)| r).unwrap_or(server);
    format!("{authority}:{profile}")
}

pub fn web_base(server: &str, web_url: Option<&str>) -> String {
    if let Some(w) = web_url {
        return w.trim_end_matches('/').to_string();
    }
    let (scheme, authority) = server.split_once("://").unwrap_or(("https", server));
    let host = authority.split(':').next().unwrap_or(authority);
    if is_loopback(host) {
        return "http://localhost:5173".to_string();
    }
    format!(
        "{scheme}://{}",
        authority.strip_prefix("api.").unwrap_or(authority)
    )
}

#[cfg(unix)]
pub fn ensure_private_dir(dir: &Path) -> Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    if !dir.exists() {
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    let mode = fs::metadata(dir)?.permissions().mode() & 0o777;
    if mode != 0o700 {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(not(unix))]
pub fn ensure_private_dir(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir)?;
    Ok(())
}

#[cfg(unix)]
pub fn write_atomic(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let dir = path
        .parent()
        .ok_or_else(|| Error::internal("invalid path"))?;
    let tmp = dir.join(format!(
        ".{}.{}.tmp",
        path.file_name().and_then(|n| n.to_str()).unwrap_or("file"),
        std::process::id()
    ));
    let _ = fs::remove_file(&tmp);
    let result = (|| -> std::io::Result<()> {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(Error::from)
}

#[cfg(not(unix))]
pub fn write_atomic(path: &Path, bytes: &[u8], _mode: u32) -> Result<()> {
    fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_server_is_the_production_api() {
        assert_eq!(
            Config::default().effective_server(None).unwrap(),
            "https://api.tmgr.dev"
        );
    }

    #[test]
    fn server_must_be_https_or_loopback() {
        assert_eq!(
            normalize_server("https://api.example.com/").unwrap(),
            "https://api.example.com"
        );
        assert_eq!(
            normalize_server("http://127.0.0.1:8080").unwrap(),
            "http://127.0.0.1:8080"
        );
        assert!(normalize_server("http://example.com").is_err());
        assert!(normalize_server("ftp://example.com").is_err());
        assert!(normalize_server("https://u:p@example.com").is_err());
        assert!(normalize_server("https://example.com/api").is_err());
        assert!(normalize_server("https://example.com/?x=1").is_err());
        assert!(normalize_server("nonsense").is_err());
    }

    #[test]
    fn config_dir_prefers_absolute_xdg_then_home() {
        let d = config_dir_from(Some("/x/cfg".into()), Some("/h".into())).unwrap();
        assert_eq!(d, PathBuf::from("/x/cfg/tmgr"));
        let d = config_dir_from(Some("rel".into()), Some("/h".into())).unwrap();
        assert_eq!(d, PathBuf::from("/h/.config/tmgr"));
        assert!(config_dir_from(None, None).is_err());
    }

    #[test]
    fn config_round_trips_and_validates_keys() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("tmgr");
        let mut c = Config::load(&dir).unwrap();
        assert_eq!(c, Config::default());
        c.set("default_workspace", "personal").unwrap();
        c.set("server", "https://api.example.com/").unwrap();
        c.save(&dir).unwrap();
        let back = Config::load(&dir).unwrap();
        assert_eq!(back.default_workspace.as_deref(), Some("personal"));
        assert_eq!(back.server.as_deref(), Some("https://api.example.com"));
        assert!(c.set("projects.TM", "/x").is_err());
        assert_eq!(c.branch_template(), "{key_lower}-{slug}");
        assert_eq!(c.review_status(), "Review");
        c.set("branch_template", "feat/{key_lower}-{slug}").unwrap();
        c.set("review_status", " In review ").unwrap();
        assert_eq!(
            c.get("review_status").unwrap().as_deref(),
            Some("In review")
        );
        assert!(c.set("branch_template", "-{slug}$").is_err());
        assert!(c.set("review_status", "  ").is_err());
        assert!(c.set("default_workspace", "a b").is_err());
        assert!(c.get("nope").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn config_dir_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("tmgr");
        Config::default().save(&dir).unwrap();
        assert_eq!(
            fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }

    #[test]
    fn broken_config_is_a_usage_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(config_path(tmp.path()), "server = [").unwrap();
        assert_eq!(Config::load(tmp.path()).unwrap_err().exit_code(), 2);
    }

    #[test]
    fn account_and_web_base() {
        assert_eq!(
            account_for("https://api.tmgr.dev", "default"),
            "api.tmgr.dev:default"
        );
        assert_eq!(
            account_for("http://127.0.0.1:9", "default"),
            "127.0.0.1:9:default"
        );
        assert_eq!(web_base("https://api.tmgr.dev", None), "https://tmgr.dev");
        assert_eq!(
            web_base("https://tasks.corp.example", None),
            "https://tasks.corp.example"
        );
        assert_eq!(
            web_base("http://127.0.0.1:8080", None),
            "http://localhost:5173"
        );
        assert_eq!(
            web_base("https://api.tmgr.dev", Some("https://w.example/")),
            "https://w.example"
        );
    }
}
