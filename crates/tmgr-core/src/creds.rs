use crate::config::{ensure_private_dir, write_atomic};
use crate::error::{Error, Result};
use crate::redact::Secret;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub use keyring::Error as KeyringError;

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

#[derive(Debug)]
pub struct Saved {
    pub backend: Backend,
    pub warning: Option<String>,
}

pub trait Keychain {
    fn get(&self, account: &str) -> keyring::Result<String>;
    fn set(&self, account: &str, blob: &str) -> keyring::Result<()>;
    fn delete(&self, account: &str) -> keyring::Result<()>;
}

pub struct OsKeychain;

impl Keychain for OsKeychain {
    fn get(&self, account: &str) -> keyring::Result<String> {
        keyring::Entry::new(SERVICE, account)?.get_password()
    }

    fn set(&self, account: &str, blob: &str) -> keyring::Result<()> {
        keyring::Entry::new(SERVICE, account)?.set_password(blob)
    }

    fn delete(&self, account: &str) -> keyring::Result<()> {
        keyring::Entry::new(SERVICE, account)?.delete_credential()
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Failure {
    NoEntry,
    StoreAbsent,
    Refused,
}

pub fn classify(err: &keyring::Error, linux: bool) -> Failure {
    match err {
        keyring::Error::NoEntry => Failure::NoEntry,
        keyring::Error::NoDefaultStore | keyring::Error::NotSupportedByStore(_) => {
            Failure::StoreAbsent
        }
        keyring::Error::PlatformFailure(_) if linux => Failure::StoreAbsent,
        _ => Failure::Refused,
    }
}

fn refused() -> Error {
    Error::internal("the OS credential store refused access (locked or denied)").hint(format!(
        "unlock the keychain and retry, or opt in to a 0600 file with {STORE_ENV}=file"
    ))
}

pub struct Guard(#[allow(dead_code)] fs::File);

pub struct CredentialStore {
    dir: PathBuf,
    account: String,
    forced_file: bool,
    preferred: Option<Backend>,
    last_loaded: Cell<Option<Backend>>,
    keychain: Box<dyn Keychain>,
}

impl CredentialStore {
    pub fn new(dir: &Path, account: &str, preferred: Option<Backend>) -> Self {
        Self::with_keychain(dir, account, preferred, Box::new(OsKeychain))
    }

    pub fn with_keychain(
        dir: &Path,
        account: &str,
        preferred: Option<Backend>,
        keychain: Box<dyn Keychain>,
    ) -> Self {
        Self {
            dir: dir.to_path_buf(),
            account: account.to_string(),
            forced_file: std::env::var(STORE_ENV).as_deref() == Ok("file"),
            preferred,
            last_loaded: Cell::new(None),
            keychain,
        }
    }

    pub fn file_path(&self) -> PathBuf {
        self.dir.join("credentials.json")
    }

    fn effective(&self) -> Option<Backend> {
        if self.forced_file {
            Some(Backend::File)
        } else {
            self.preferred.or(self.last_loaded.get())
        }
    }

    pub fn lock(&self) -> Result<Guard> {
        use std::os::unix::fs::OpenOptionsExt;
        ensure_private_dir(&self.dir)?;
        let f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(self.dir.join("credentials.lock"))?;
        f.lock()?;
        Ok(Guard(f))
    }

    pub fn load(&self) -> Result<Option<(Tokens, Backend)>> {
        if self.effective() != Some(Backend::File) {
            if let Some(t) = self.keychain_load()? {
                self.last_loaded.set(Some(Backend::Keychain));
                return Ok(Some((t, Backend::Keychain)));
            }
        }
        let loaded = self.file_load()?.map(|t| (t, Backend::File));
        if loaded.is_some() {
            self.last_loaded.set(Some(Backend::File));
        }
        Ok(loaded)
    }

    pub fn save(&self, tokens: &Tokens) -> Result<Saved> {
        let guard = self.lock()?;
        self.save_locked(&guard, tokens)
    }

    pub fn save_locked(&self, _guard: &Guard, tokens: &Tokens) -> Result<Saved> {
        let effective = self.effective();
        if effective == Some(Backend::File) {
            self.file_save(tokens)?;
            return Ok(Saved {
                backend: Backend::File,
                warning: None,
            });
        }
        match self.keychain_save(tokens) {
            Ok(()) => Ok(Saved {
                backend: Backend::Keychain,
                warning: None,
            }),
            Err(Failure::StoreAbsent) if effective.is_none() => {
                self.file_save(tokens)?;
                Ok(Saved {
                    backend: Backend::File,
                    warning: Some(format!(
                        "no OS credential store available; token stored in {} (mode 0600)",
                        self.file_path().display()
                    )),
                })
            }
            Err(_) => Err(refused()),
        }
    }

    pub fn finalize(&self, backend: Backend) {
        if let Ok(_guard) = self.lock() {
            match backend {
                Backend::Keychain => {
                    let _ = self.file_remove();
                }
                Backend::File => {
                    let _ = self.keychain.delete(&self.account);
                }
            }
        }
    }

    pub fn delete(&self) -> Result<()> {
        let _guard = self.lock()?;
        let mut first: Option<Error> = None;
        if self.effective() != Some(Backend::File) {
            match self.keychain.delete(&self.account) {
                Ok(()) => {}
                Err(e) => {
                    if classify(&e, cfg!(target_os = "linux")) == Failure::Refused {
                        first = Some(refused());
                    }
                }
            }
        }
        if let Err(e) = self.file_remove() {
            first.get_or_insert(e);
        }
        first.map_or(Ok(()), Err)
    }

    fn keychain_load(&self) -> Result<Option<Tokens>> {
        match self.keychain.get(&self.account) {
            Ok(blob) => serde_json::from_str::<Stored>(&blob)
                .map(|s| Some(s.into_tokens()))
                .map_err(|_| {
                    Error::internal("the stored credential is corrupt").hint("tmgr login")
                }),
            Err(e) => match classify(&e, cfg!(target_os = "linux")) {
                Failure::NoEntry | Failure::StoreAbsent => Ok(None),
                Failure::Refused => Err(refused()),
            },
        }
    }

    fn keychain_save(&self, tokens: &Tokens) -> std::result::Result<(), Failure> {
        let blob =
            serde_json::to_string(&Stored::from_tokens(tokens)).map_err(|_| Failure::Refused)?;
        self.keychain
            .set(&self.account, &blob)
            .map_err(|e| classify(&e, cfg!(target_os = "linux")))
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
        let mut s = with_fake(dir, account, None, Mode::Locked);
        s.forced_file = true;
        s
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Mode {
        Works,
        Absent,
        Locked,
    }

    struct Fake {
        mode: Mode,
        blob: std::rc::Rc<std::cell::RefCell<Option<String>>>,
    }

    impl Fake {
        fn err(&self) -> Option<keyring::Error> {
            match self.mode {
                Mode::Works => None,
                Mode::Absent => Some(keyring::Error::NoDefaultStore),
                Mode::Locked => Some(keyring::Error::NoStorageAccess(
                    std::io::Error::other("locked").into(),
                )),
            }
        }
    }

    impl Keychain for Fake {
        fn get(&self, _: &str) -> keyring::Result<String> {
            if let Some(e) = self.err() {
                return Err(e);
            }
            self.blob.borrow().clone().ok_or(keyring::Error::NoEntry)
        }
        fn set(&self, _: &str, b: &str) -> keyring::Result<()> {
            if let Some(e) = self.err() {
                return Err(e);
            }
            *self.blob.borrow_mut() = Some(b.to_string());
            Ok(())
        }
        fn delete(&self, _: &str) -> keyring::Result<()> {
            if let Some(e) = self.err() {
                return Err(e);
            }
            self.blob
                .borrow_mut()
                .take()
                .map(|_| ())
                .ok_or(keyring::Error::NoEntry)
        }
    }

    fn with_fake(
        dir: &Path,
        account: &str,
        preferred: Option<Backend>,
        mode: Mode,
    ) -> CredentialStore {
        let fake = Fake {
            mode,
            blob: Default::default(),
        };
        CredentialStore::with_keychain(&dir.join("tmgr"), account, preferred, Box::new(fake))
    }

    #[test]
    fn typed_errors_are_classified_per_platform() {
        let denied = || keyring::Error::NoStorageAccess(std::io::Error::other("x").into());
        let platform = || keyring::Error::PlatformFailure(std::io::Error::other("x").into());
        assert_eq!(classify(&keyring::Error::NoEntry, true), Failure::NoEntry);
        assert_eq!(
            classify(&keyring::Error::NoDefaultStore, false),
            Failure::StoreAbsent
        );
        assert_eq!(classify(&denied(), true), Failure::Refused);
        assert_eq!(classify(&denied(), false), Failure::Refused);
        assert_eq!(classify(&platform(), true), Failure::StoreAbsent);
        assert_eq!(classify(&platform(), false), Failure::Refused);
    }

    #[test]
    fn missing_store_falls_back_to_the_file_with_a_warning() {
        let tmp = tempfile::tempdir().unwrap();
        let s = with_fake(tmp.path(), "h:default", None, Mode::Absent);
        let saved = s.save(&tokens("a", Some("r"))).unwrap();
        assert_eq!(saved.backend, Backend::File);
        assert!(saved.warning.unwrap().contains("no OS credential store"));
        assert_eq!(s.load().unwrap().unwrap().1, Backend::File);
    }

    #[test]
    fn a_locked_store_is_an_error_and_never_writes_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let s = with_fake(tmp.path(), "h:default", None, Mode::Locked);
        let err = s.save(&tokens("a", None)).unwrap_err();
        assert!(err.hint.unwrap().contains("TMGR_CREDENTIAL_STORE=file"));
        assert!(!s.file_path().exists());
        assert!(s.load().is_err());
    }

    #[test]
    fn an_established_keychain_never_silently_downgrades_to_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let s = with_fake(
            tmp.path(),
            "h:default",
            Some(Backend::Keychain),
            Mode::Absent,
        );
        assert!(s.save(&tokens("a", None)).is_err());
        assert!(!s.file_path().exists());
    }

    #[test]
    fn a_file_preference_is_honoured_on_save_and_load() {
        let tmp = tempfile::tempdir().unwrap();
        let s = with_fake(tmp.path(), "h:default", Some(Backend::File), Mode::Locked);
        let saved = s.save(&tokens("a", None)).unwrap();
        assert_eq!((saved.backend, saved.warning), (Backend::File, None));
        assert_eq!(s.load().unwrap().unwrap().0.access.expose(), "a");
    }

    #[test]
    fn missing_entries_are_not_confused_with_refused_access() {
        let tmp = tempfile::tempdir().unwrap();
        let s = with_fake(tmp.path(), "h:default", None, Mode::Works);
        assert!(s.load().unwrap().is_none());
        s.delete().unwrap();
        let locked = with_fake(tmp.path(), "h:default", None, Mode::Locked);
        assert!(locked.delete().is_err());
    }

    #[test]
    fn finalize_removes_the_other_copy_only_when_asked() {
        let tmp = tempfile::tempdir().unwrap();
        let mut f = with_fake(tmp.path(), "h:default", None, Mode::Works);
        f.forced_file = true;
        f.save(&tokens("old", None)).unwrap();
        f.forced_file = false;
        let saved = f.save(&tokens("new", None)).unwrap();
        assert_eq!(saved.backend, Backend::Keychain);
        assert!(f.file_path().exists());
        f.finalize(Backend::Keychain);
        assert!(!f.file_path().exists());
        assert_eq!(f.load().unwrap().unwrap().0.access.expose(), "new");
    }

    #[test]
    fn the_credentials_lock_serialises_writers() {
        let tmp = tempfile::tempdir().unwrap();
        let first = forced(tmp.path(), "h:default").lock().unwrap();
        let dir = tmp.path().to_path_buf();
        let (tx, rx) = std::sync::mpsc::channel();
        let h = std::thread::spawn(move || {
            let _g = forced(&dir, "h:default").lock().unwrap();
            tx.send(()).unwrap();
        });
        assert!(rx
            .recv_timeout(std::time::Duration::from_millis(300))
            .is_err());
        drop(first);
        assert!(rx.recv_timeout(std::time::Duration::from_secs(5)).is_ok());
        h.join().unwrap();
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
