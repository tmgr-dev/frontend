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
    pub remote_control: bool,
    pub allow_bypass: bool,
    pub allow_no_sandbox: bool,
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
    #[serde(default)]
    remote_control: bool,
    #[serde(default)]
    allow_bypass: bool,
    #[serde(default)]
    allow_no_sandbox: bool,
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

const FENCE_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

// Held shared through the spawn so a trust change cannot land between the generation check and the spawn.
pub fn fence_shared() -> Result<std::fs::File, String> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let dir = store_dir()?.join("link");
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .map_err(|e| format!("cannot create the trust fence directory: {e}"))?;
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(dir.join("links.lock"))
        .map_err(|e| format!("cannot open the trust fence: {e}"))?;
    let fd = std::os::fd::AsRawFd::as_raw_fd(&f);
    let deadline = std::time::Instant::now() + FENCE_WAIT;
    loop {
        if unsafe { libc::flock(fd, libc::LOCK_SH | libc::LOCK_NB) } == 0 {
            return Ok(f);
        }
        if std::time::Instant::now() >= deadline {
            return Err("the link trust store is being changed; retry".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
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
    parse(&text, link)
}

fn parse(text: &str, link: &str) -> Result<Option<LinkTrust>, String> {
    let store: StoreRec =
        toml::from_str(text).map_err(|_| "the trust store is not valid".to_string())?;
    Ok(store.leaders.get(link).map(|l| LinkTrust {
        scope: LinkScope {
            repos: l.scope.repos.clone(),
            agents: l.scope.agents.clone(),
            models: l.scope.models.clone(),
            max: l.scope.max,
            remote_control: l.scope.remote_control,
            allow_bypass: l.scope.allow_bypass,
            allow_no_sandbox: l.scope.allow_no_sandbox,
        },
        generation: store.generations.get(link).copied().unwrap_or(0),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    const OLD: &str = "[leaders.laptop.scope]\nrepos = [\"/r\"]\nagents = [\"claude-code\"]\nmax = 2\n\n[generations]\nlaptop = 3\n";

    #[test]
    fn a_scope_without_the_privilege_fields_has_them_all_off() {
        let t = parse(OLD, "laptop").unwrap().unwrap();
        assert_eq!(t.generation, 3);
        assert_eq!(t.scope.max, 2);
        assert!(!t.scope.remote_control && !t.scope.allow_bypass && !t.scope.allow_no_sandbox);
    }

    #[test]
    fn the_privilege_fields_are_read_when_true() {
        let text = "[leaders.laptop.scope]\nremote_control = true\nallow_bypass = true\nallow_no_sandbox = true\n";
        let s = parse(text, "laptop").unwrap().unwrap().scope;
        assert!(s.remote_control && s.allow_bypass && s.allow_no_sandbox);
        let only = "[leaders.laptop.scope]\nallow_bypass = true\n";
        let s = parse(only, "laptop").unwrap().unwrap().scope;
        assert!(s.allow_bypass && !s.remote_control && !s.allow_no_sandbox);
        assert!(parse(text, "other").unwrap().is_none());
    }
}
