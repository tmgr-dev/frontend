use crate::scope::Scope;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tmgr_core::config::{ensure_private_dir, write_atomic};
use tmgr_core::error::{Error, Result};

#[derive(Debug, Clone)]
pub struct Paths {
    pub config: PathBuf,
    pub state: PathBuf,
    pub home: PathBuf,
}

impl Paths {
    pub fn from_env() -> Result<Paths> {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .ok_or_else(|| Error::internal("HOME is not set"))?;
        let state = tmgr_core::active::state_dir()
            .ok_or_else(|| Error::internal("cannot locate the state directory"))?;
        Ok(Paths {
            config: tmgr_core::config::config_dir()?,
            state,
            home,
        })
    }

    pub fn under(root: &Path) -> Paths {
        Paths {
            config: root.join("config"),
            state: root.join("state"),
            home: root.join("home"),
        }
    }

    pub fn links_toml(&self) -> PathBuf {
        self.config.join("links.toml")
    }

    pub fn known_hosts(&self) -> PathBuf {
        self.config.join("link_known_hosts")
    }

    pub fn public_key(&self) -> PathBuf {
        self.config.join("link_key.pub")
    }

    pub fn pending_marker(&self, name: &str) -> PathBuf {
        self.config.join("link-pending").join(name)
    }

    pub fn link_dir(&self) -> PathBuf {
        self.config.join("link")
    }

    pub fn audit_dir(&self) -> PathBuf {
        self.state.join("link-audit")
    }

    pub fn authorized_keys(&self) -> PathBuf {
        self.home.join(".ssh").join("authorized_keys")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Leader {
    pub key: String,
    pub fingerprint: String,
    pub added: String,
    #[serde(default)]
    pub scope: Scope,
    #[serde(default)]
    pub enrollment: String,
    #[serde(default)]
    pub authorized_keys: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Follower {
    pub user: String,
    pub host: String,
    pub port: u16,
    pub host_key_fingerprint: String,
    pub added: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenEnrollment {
    pub generation: u64,
    pub enrollment: String,
}

const MAX_HISTORY: usize = 64;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Store {
    #[serde(default)]
    pub version: u64,
    #[serde(default)]
    pub generation_seq: u64,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub generations: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub generation_enrollments: BTreeMap<String, Vec<GenEnrollment>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub leaders: BTreeMap<String, Leader>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub followers: BTreeMap<String, Follower>,
}

#[cfg(unix)]
fn check_private(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.is_file() {
        return Err(crate::forbidden(format!(
            "{} is not a regular file",
            path.display()
        )));
    }
    if meta.permissions().mode() & 0o077 != 0 {
        return Err(crate::forbidden(format!(
            "{} is accessible by other users, refusing to use it",
            path.display()
        ))
        .hint(format!("chmod 600 {}", path.display())));
    }
    Ok(())
}

#[cfg(not(unix))]
fn check_private(_path: &Path) -> Result<()> {
    Ok(())
}

impl Store {
    pub fn load(paths: &Paths) -> Result<Store> {
        let path = paths.links_toml();
        match std::fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Store::default()),
            Err(e) => return Err(e.into()),
        }
        check_private(&path)?;
        let text = std::fs::read_to_string(&path)?;
        toml::from_str(&text).map_err(|e| {
            Error::internal(format!("invalid links.toml: {}", e.message()))
                .hint(format!("fix or remove {}", path.display()))
        })
    }

    pub fn update<T>(paths: &Paths, change: impl FnOnce(&mut Store) -> Result<T>) -> Result<T> {
        let _lock = lock_links(paths)?;
        let mut store = Store::load(paths)?;
        let out = change(&mut store)?;
        store.save(paths)?;
        Ok(out)
    }

    pub(crate) fn bump_generation(&mut self, link: &str, enrollment: &str) -> u64 {
        self.generation_seq += 1;
        self.generations
            .insert(link.to_string(), self.generation_seq);
        let history = self
            .generation_enrollments
            .entry(link.to_string())
            .or_default();
        history.push(GenEnrollment {
            generation: self.generation_seq,
            enrollment: enrollment.to_string(),
        });
        if history.len() > MAX_HISTORY {
            let drop = history.len() - MAX_HISTORY;
            history.drain(..drop);
        }
        self.generation_seq
    }

    pub fn enrollment_of(&self, link: &str, generation: u64) -> Option<String> {
        if let Some(e) = self
            .generation_enrollments
            .get(link)
            .and_then(|h| h.iter().find(|g| g.generation == generation))
        {
            return Some(e.enrollment.clone());
        }
        (generation == self.generation(link))
            .then(|| self.leaders.get(link).map(|l| l.enrollment.clone()))
            .flatten()
    }

    pub fn generation(&self, link: &str) -> u64 {
        self.generations.get(link).copied().unwrap_or(0)
    }

    pub(crate) fn save(&mut self, paths: &Paths) -> Result<()> {
        self.version += 1;
        let text = toml::to_string_pretty(self)
            .map_err(|_| Error::internal("cannot serialize links.toml"))?;
        write_atomic(&paths.links_toml(), text.as_bytes(), 0o600)
    }
}

pub(crate) fn lock_links(paths: &Paths) -> Result<Lock> {
    ensure_private_dir(&paths.config)?;
    let old = paths.config.join("links.lock");
    if std::fs::symlink_metadata(&old).is_ok_and(|m| m.is_file()) {
        let _ = std::fs::remove_file(&old);
    }
    ensure_private_dir(&paths.link_dir())?;
    lock(&crate::fence::path(paths))
}

pub(crate) struct Lock(#[allow(dead_code)] std::fs::File);

pub(crate) fn lock(path: &Path) -> Result<Lock> {
    use std::os::unix::fs::OpenOptionsExt;
    let f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;
    f.lock()?;
    Ok(Lock(f))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leader() -> Leader {
        Leader {
            key: "AAAA".into(),
            fingerprint: "SHA256:x".into(),
            added: "2026-10-13T00:00:00Z".into(),
            scope: Scope {
                repos: vec!["/srv/a".into()],
                agents: vec!["codex".into()],
                max: 2,
                from: vec!["192.0.2.0/24".into()],
                allow_attach_input: false,
                models: vec![],
            },
            enrollment: "ab".repeat(16),
            authorized_keys: "/home/u/.ssh/authorized_keys".into(),
        }
    }

    #[test]
    fn store_round_trips_with_private_modes() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::under(d.path());
        assert_eq!(Store::load(&paths).unwrap(), Store::default());
        Store::update(&paths, |s| {
            s.leaders.insert("laptop".into(), leader());
            s.followers.insert(
                "spark".into(),
                Follower {
                    user: "deploy".into(),
                    host: "example.com".into(),
                    port: 2222,
                    host_key_fingerprint: "SHA256:y".into(),
                    added: "2026-10-13T00:00:00Z".into(),
                },
            );
            Ok(())
        })
        .unwrap();
        let loaded = Store::load(&paths).unwrap();
        assert_eq!(loaded.leaders["laptop"], leader());
        assert_eq!(loaded.version, 1);
        assert_eq!(loaded.followers["spark"].port, 2222);
        let file = std::fs::metadata(paths.links_toml()).unwrap();
        assert_eq!(file.permissions().mode() & 0o777, 0o600);
        let dir = std::fs::metadata(&paths.config).unwrap();
        assert_eq!(dir.permissions().mode() & 0o777, 0o700);
    }

    #[test]
    fn the_runner_reads_these_keys() {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::under(d.path());
        Store::update(&paths, |s| {
            s.leaders.insert("laptop".into(), leader());
            s.bump_generation("laptop", "e1");
            Ok(())
        })
        .unwrap();
        let v: toml::Value =
            toml::from_str(&std::fs::read_to_string(paths.links_toml()).unwrap()).unwrap();
        assert_eq!(v["generations"]["laptop"].as_integer(), Some(1));
        let scope = &v["leaders"]["laptop"]["scope"];
        assert_eq!(scope["repos"][0].as_str(), Some("/srv/a"));
        assert_eq!(scope["agents"][0].as_str(), Some("codex"));
        assert_eq!(scope["max"].as_integer(), Some(2));
        assert!(scope["models"].as_array().unwrap().is_empty());
    }

    #[test]
    fn a_failing_change_writes_nothing() {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::under(d.path());
        let r: Result<()> = Store::update(&paths, |s| {
            s.leaders.insert("x".into(), leader());
            Err(Error::internal("nope"))
        });
        assert!(r.is_err());
        assert!(!paths.links_toml().exists());
    }

    #[test]
    fn a_group_readable_store_is_refused() {
        use std::os::unix::fs::PermissionsExt;
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::under(d.path());
        Store::update(&paths, |_| Ok(())).unwrap();
        std::fs::set_permissions(paths.links_toml(), std::fs::Permissions::from_mode(0o644))
            .unwrap();
        assert_eq!(Store::load(&paths).unwrap_err().exit_code(), 5);
    }

    #[test]
    fn unknown_fields_and_garbage_are_refused() {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths::under(d.path());
        Store::update(&paths, |_| Ok(())).unwrap();
        tmgr_core::config::write_atomic(&paths.links_toml(), b"bogus = 1\n", 0o600).unwrap();
        assert!(Store::load(&paths).is_err());
        tmgr_core::config::write_atomic(&paths.links_toml(), b"\x00\x01 not toml", 0o600).unwrap();
        assert!(Store::load(&paths).is_err());
    }
}
