use crate::config::{ensure_private_dir, write_atomic};
use crate::error::{Error, Result};
use crate::redact::Secret;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const SERVICE: &str = "dev.tmgr.cli";
pub const STORE_ENV: &str = "TMGR_CREDENTIAL_STORE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backend {
    Keychain,
    File,
}

impl Backend {
    pub fn as_str(self) -> &'static str {
        match self {
            Backend::Keychain => "keychain",
            Backend::File => "file",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Backend::Keychain => "OS credential store",
            Backend::File => "0600 file",
        }
    }

    pub fn parse(s: &str) -> Option<Backend> {
        match s {
            "keychain" => Some(Backend::Keychain),
            "file" => Some(Backend::File),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Tokens {
    pub access: Secret,
    pub refresh: Option<Secret>,
    pub expires_at: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    access_token: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expires_at: Option<String>,
}

impl Stored {
    fn from_tokens(t: &Tokens) -> Self {
        Stored {
            access_token: t.access.expose().to_string(),
            refresh_token: t.refresh.as_ref().map(|r| r.expose().to_string()),
            expires_at: t.expires_at.clone(),
        }
    }

    fn into_tokens(self) -> Tokens {
        Tokens {
            access: Secret::new(self.access_token),
            refresh: self.refresh_token.map(Secret::new),
            expires_at: self.expires_at,
        }
    }
}

pub struct Saved {
    pub backend: Backend,
    pub warning: Option<String>,
}

pub struct CredentialStore {
    dir: PathBuf,
    account: String,
    forced_file: bool,
    preferred: Option<Backend>,
}

impl CredentialStore {
    pub fn new(dir: &Path, account: &str, preferred: Option<Backend>) -> Self {
        Self {
            dir: dir.to_path_buf(),
            account: account.to_string(),
            forced_file: std::env::var(STORE_ENV).as_deref() == Ok("file"),
            preferred,
        }
    }

    pub fn file_path(&self) -> PathBuf {
        self.dir.join("credentials.json")
    }

    pub fn load(&self) -> Result<Option<(Tokens, Backend)>> {
        if !self.forced_file && self.preferred != Some(Backend::File) {
            if let Some(t) = self.keychain_load() {
                return Ok(Some((t, Backend::Keychain)));
            }
        }
        Ok(self.file_load()?.map(|t| (t, Backend::File)))
    }

    pub fn save(&self, tokens: &Tokens) -> Result<Saved> {
        if self.forced_file {
            self.file_save(tokens)?;
            return Ok(Saved {
                backend: Backend::File,
                warning: None,
            });
        }
        match self.keychain_save(tokens) {
            Ok(()) => {
                let _ = self.file_remove();
                Ok(Saved {
                    backend: Backend::Keychain,
                    warning: None,
                })
            }
            Err(_) => {
                self.file_save(tokens)?;
                Ok(Saved {
                    backend: Backend::File,
                    warning: Some(format!(
                        "no OS credential store available; token stored in {} (mode 0600)",
                        self.file_path().display()
                    )),
                })
            }
        }
    }

    pub fn delete(&self) -> Result<()> {
        if !self.forced_file {
            if let Ok(entry) = keyring::Entry::new(SERVICE, &self.account) {
                let _ = entry.delete_credential();
            }
        }
        self.file_remove()
    }

    fn keychain_load(&self) -> Option<Tokens> {
        let entry = keyring::Entry::new(SERVICE, &self.account).ok()?;
        let blob = entry.get_password().ok()?;
        serde_json::from_str::<Stored>(&blob)
            .ok()
            .map(Stored::into_tokens)
    }

    fn keychain_save(&self, tokens: &Tokens) -> std::result::Result<(), ()> {
        let entry = keyring::Entry::new(SERVICE, &self.account).map_err(|_| ())?;
        let blob = serde_json::to_string(&Stored::from_tokens(tokens)).map_err(|_| ())?;
        entry.set_password(&blob).map_err(|_| ())
    }

    fn read_file_map(&self) -> Result<BTreeMap<String, Stored>> {
        let path = self.file_path();
        let meta = match fs::metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(e) => return Err(e.into()),
        };
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if meta.permissions().mode() & 0o077 != 0 {
                return Err(Error::internal(format!(
                    "{} is accessible by other users, refusing to read it",
                    path.display()
                ))
                .hint(format!("chmod 600 {}", path.display())));
            }
        }
        let _ = meta;
        let text = fs::read_to_string(&path)?;
        serde_json::from_str(&text).map_err(|_| {
            Error::internal(format!("{} is not valid", path.display()))
                .hint("remove it and run `tmgr login`")
        })
    }

    fn file_load(&self) -> Result<Option<Tokens>> {
        Ok(self
            .read_file_map()?
            .remove(&self.account)
            .map(Stored::into_tokens))
    }

    fn file_save(&self, tokens: &Tokens) -> Result<()> {
        ensure_private_dir(&self.dir)?;
        let mut map = self.read_file_map()?;
        map.insert(self.account.clone(), Stored::from_tokens(tokens));
        let text = serde_json::to_string_pretty(&map)
            .map_err(|_| Error::internal("cannot serialize credentials"))?;
        write_atomic(&self.file_path(), text.as_bytes(), 0o600)
    }

    fn file_remove(&self) -> Result<()> {
        if !self.file_path().exists() {
            return Ok(());
        }
        let mut map = self.read_file_map()?;
        if map.remove(&self.account).is_none() {
            return Ok(());
        }
        if map.is_empty() {
            fs::remove_file(self.file_path())?;
            return Ok(());
        }
        let text = serde_json::to_string_pretty(&map)
            .map_err(|_| Error::internal("cannot serialize credentials"))?;
        write_atomic(&self.file_path(), text.as_bytes(), 0o600)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forced(dir: &Path, account: &str) -> CredentialStore {
        CredentialStore {
            dir: dir.join("tmgr"),
            account: account.to_string(),
            forced_file: true,
            preferred: None,
        }
    }

    fn tokens(a: &str, r: Option<&str>) -> Tokens {
        Tokens {
            access: Secret::new(a),
            refresh: r.map(Secret::new),
            expires_at: Some("2026-11-10T00:00:00Z".into()),
        }
    }

    #[test]
    fn file_store_round_trip_with_private_modes() {
        let tmp = tempfile::tempdir().unwrap();
        let store = forced(tmp.path(), "h:default");
        assert!(store.load().unwrap().is_none());
        let saved = store.save(&tokens("acc", Some("ref"))).unwrap();
        assert_eq!(saved.backend, Backend::File);
        let (t, b) = store.load().unwrap().unwrap();
        assert_eq!(b, Backend::File);
        assert_eq!(t.access.expose(), "acc");
        assert_eq!(t.refresh.as_ref().unwrap().expose(), "ref");

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let dir_mode = fs::metadata(tmp.path().join("tmgr"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            let file_mode = fs::metadata(store.file_path())
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(dir_mode, 0o700);
            assert_eq!(file_mode, 0o600);
        }
    }

    #[test]
    fn accounts_are_independent_and_delete_removes_the_file_when_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let a = forced(tmp.path(), "a:default");
        let b = forced(tmp.path(), "b:default");
        a.save(&tokens("A", None)).unwrap();
        b.save(&tokens("B", None)).unwrap();
        a.delete().unwrap();
        assert!(a.load().unwrap().is_none());
        assert_eq!(b.load().unwrap().unwrap().0.access.expose(), "B");
        b.delete().unwrap();
        assert!(!b.file_path().exists());
    }

    #[cfg(unix)]
    #[test]
    fn world_readable_credentials_are_refused() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let store = forced(tmp.path(), "h:default");
        store.save(&tokens("acc", None)).unwrap();
        fs::set_permissions(store.file_path(), fs::Permissions::from_mode(0o644)).unwrap();
        let err = store.load().unwrap_err();
        assert!(err.message.contains("accessible by other users"));
        assert!(err.hint.unwrap().starts_with("chmod 600"));
    }

    #[test]
    fn tokens_never_show_in_debug_output() {
        let t = tokens("topsecret", Some("topsecret2"));
        let dbg = format!("{t:?}");
        assert!(!dbg.contains("topsecret"));
    }
}
