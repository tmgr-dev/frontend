use serde::Deserialize;
use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkScope {
    pub repos: Vec<String>,
    pub agents: Vec<String>,
    pub models: Vec<String>,
    pub max: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkTrust {
    pub scope: LinkScope,
    pub generation: u64,
}

#[derive(Deserialize, Default)]
struct ScopeRec {
    #[serde(default)]
    repos: Vec<String>,
    #[serde(default)]
    agents: Vec<String>,
    #[serde(default)]
    models: Vec<String>,
    #[serde(default)]
    max: u32,
}

#[derive(Deserialize)]
struct LeaderRec {
    #[serde(default)]
    scope: ScopeRec,
}

#[derive(Deserialize, Default)]
struct StoreRec {
    #[serde(default)]
    leaders: BTreeMap<String, LeaderRec>,
    #[serde(default)]
    generations: BTreeMap<String, u64>,
}

pub fn store_dir() -> Result<PathBuf, String> {
    let abs = |k: &str| {
        std::env::var_os(k)
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
    };
    let dir = match abs("XDG_CONFIG_HOME") {
        Some(x) => x.join("tmgr"),
        None => abs("HOME")
            .ok_or("cannot locate the trust store (HOME is not set)")?
            .join(".config")
            .join("tmgr"),
    };
    Ok(dir)
}

fn links_toml() -> Result<PathBuf, String> {
    Ok(store_dir()?.join("links.toml"))
}

pub fn read(link: &str) -> Result<Option<LinkTrust>, String> {
    let path = links_toml()?;
    let meta = match std::fs::symlink_metadata(&path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read the trust store: {e}")),
    };
    if !meta.is_file() || meta.permissions().mode() & 0o077 != 0 {
        return Err("the trust store is not a private regular file".into());
    }
    let text =
        std::fs::read_to_string(&path).map_err(|e| format!("cannot read the trust store: {e}"))?;
    let store: StoreRec =
        toml::from_str(&text).map_err(|_| "the trust store is not valid".to_string())?;
    Ok(store.leaders.get(link).map(|l| LinkTrust {
        scope: LinkScope {
            repos: l.scope.repos.clone(),
            agents: l.scope.agents.clone(),
            models: l.scope.models.clone(),
            max: l.scope.max,
        },
        generation: store.generations.get(link).copied().unwrap_or(0),
    }))
}
