use std::path::Path;
use tmgr_chat_relay::engine::{KeyStore, Slot};
use tmgr_runner::chat::relay::store::RelayDir;

pub const SERVICE: &str = "dev.tmgr.attach.chat-relay";
const CHUNK: usize = 1800;
const TERMINAL_SUFFIX: &str = "#terminal";

pub struct DynStore(Box<dyn KeyStore>);

impl KeyStore for DynStore {
    fn load(&self, slot: Slot) -> Result<Option<String>, String> {
        self.0.load(slot)
    }

    fn save(&self, slot: Slot, value: &str) -> Result<(), String> {
        self.0.save(slot, value)
    }

    fn clear(&self) -> Result<(), String> {
        self.0.clear()
    }
}

#[cfg(target_os = "macos")]
pub fn open_store(dir: &Path, namespace: &str) -> Result<DynStore, String> {
    let _ = dir;
    Ok(DynStore(Box::new(KeychainStore::new(
        KeyringCells,
        namespace.to_owned(),
    ))))
}

#[cfg(not(target_os = "macos"))]
pub fn open_store(dir: &Path, _namespace: &str) -> Result<DynStore, String> {
    Ok(DynStore(Box::new(RelayDir::open_named(dir, super::DIR)?)))
}

pub fn namespace(server: &str, user_id: u64) -> String {
    format!("u{user_id}@{server}")
}

pub fn device_namespace(server: &str, user_id: u64) -> String {
    format!("{}{TERMINAL_SUFFIX}", namespace(server, user_id))
}

pub fn config_dir(runner_dir: &Path) -> Result<RelayDir, String> {
    RelayDir::open_named(runner_dir, super::DIR)
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub trait Cells: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn set(&self, account: &str, value: &str) -> Result<(), String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

#[cfg(target_os = "macos")]
struct KeyringCells;

#[cfg(target_os = "macos")]
impl KeyringCells {
    fn entry(account: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())
    }
}

#[cfg(target_os = "macos")]
impl Cells for KeyringCells {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        match Self::entry(account)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        Self::entry(account)?
            .set_password(value)
            .map_err(|e| e.to_string())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub struct KeychainStore<C: Cells> {
    cells: C,
    namespace: String,
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
impl<C: Cells> KeychainStore<C> {
    pub fn new(cells: C, namespace: String) -> Self {
        Self { cells, namespace }
    }

    fn account(&self, slot: Slot) -> String {
        format!("{}/{}", self.namespace, slot.name())
    }
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn unreadable() -> String {
    "the attach device store is unreadable".to_owned()
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn chunk_name(name: &str, version: u64, i: usize) -> String {
    format!("{name}.v{version}.{i}")
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn pointer(cells: &dyn Cells, name: &str) -> Result<Option<(u64, usize)>, String> {
    let Some(raw) = cells.get(&format!("{name}.n"))? else {
        return Ok(None);
    };
    let (v, n) = raw.split_once(':').ok_or_else(unreadable)?;
    Ok(Some((
        v.parse().map_err(|_| unreadable())?,
        n.parse().map_err(|_| unreadable())?,
    )))
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn drop_chunks(cells: &dyn Cells, name: &str, version: u64) -> Result<(), String> {
    let mut i = 0;
    while cells.get(&chunk_name(name, version, i))?.is_some() {
        cells.delete(&chunk_name(name, version, i))?;
        i += 1;
    }
    Ok(())
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn chunks(value: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    for c in value.chars() {
        if out.last().is_some_and(|l| l.len() + c.len_utf8() > CHUNK) {
            out.push(String::new());
        }
        if let Some(l) = out.last_mut() {
            l.push(c);
        }
    }
    out
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn load_chunked(cells: &dyn Cells, name: &str) -> Result<Option<String>, String> {
    let Some((version, count)) = pointer(cells, name)? else {
        return Ok(None);
    };
    let mut out = String::new();
    for i in 0..count {
        out.push_str(
            &cells
                .get(&chunk_name(name, version, i))?
                .ok_or_else(unreadable)?,
        );
    }
    Ok(Some(out))
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn save_chunked(cells: &dyn Cells, name: &str, value: &str) -> Result<(), String> {
    let old = pointer(cells, name)?;
    let next = old.map_or(1, |(v, _)| v + 1);
    let parts = chunks(value);
    for (i, c) in parts.iter().enumerate() {
        cells.set(&chunk_name(name, next, i), c)?;
    }
    cells.set(&format!("{name}.n"), &format!("{next}:{}", parts.len()))?;
    for i in parts.len().. {
        let stale = chunk_name(name, next, i);
        if cells.get(&stale)?.is_none() {
            break;
        }
        cells.delete(&stale)?;
    }
    if let Some((v, _)) = old {
        drop_chunks(cells, name, v)?;
    }
    Ok(())
}

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn clear_chunked(cells: &dyn Cells, name: &str) -> Result<(), String> {
    let current = pointer(cells, name)?.map(|(v, _)| v);
    cells.delete(&format!("{name}.n"))?;
    if let Some(v) = current {
        drop_chunks(cells, name, v)?;
        drop_chunks(cells, name, v + 1)?;
    }
    Ok(())
}

impl<C: Cells> KeyStore for KeychainStore<C> {
    fn load(&self, slot: Slot) -> Result<Option<String>, String> {
        load_chunked(&self.cells, &self.account(slot))
    }

    fn save(&self, slot: Slot, value: &str) -> Result<(), String> {
        save_chunked(&self.cells, &self.account(slot), value)
    }

    fn clear(&self) -> Result<(), String> {
        Slot::ALL
            .into_iter()
            .try_for_each(|slot| clear_chunked(&self.cells, &self.account(slot)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct MemCells {
        map: Mutex<HashMap<String, String>>,
        sets_left: Mutex<Option<usize>>,
    }

    impl MemCells {
        fn keys(&self) -> Vec<String> {
            let mut k: Vec<String> = self.map.lock().unwrap().keys().cloned().collect();
            k.sort();
            k
        }
    }

    impl Cells for MemCells {
        fn get(&self, account: &str) -> Result<Option<String>, String> {
            Ok(self.map.lock().unwrap().get(account).cloned())
        }

        fn set(&self, account: &str, value: &str) -> Result<(), String> {
            let mut left = self.sets_left.lock().unwrap();
            if let Some(n) = left.as_mut() {
                if *n == 0 {
                    return Err("keychain write failed".into());
                }
                *n -= 1;
            }
            self.map
                .lock()
                .unwrap()
                .insert(account.to_owned(), value.to_owned());
            Ok(())
        }

        fn delete(&self, account: &str) -> Result<(), String> {
            self.map.lock().unwrap().remove(account);
            Ok(())
        }
    }

    fn store() -> KeychainStore<MemCells> {
        KeychainStore::new(MemCells::default(), "u7@https://tmgr.dev".into())
    }

    #[test]
    fn the_service_is_distinct_from_the_desktop_and_the_cli() {
        assert_eq!(SERVICE, "dev.tmgr.attach.chat-relay");
        assert_ne!(SERVICE, "dev.tmgr.desktop.chat-relay");
        assert_ne!(SERVICE, tmgr_core::creds::SERVICE);
    }

    #[test]
    fn values_round_trip_across_chunks_and_clear_removes_everything() {
        let s = store();
        let big = "é".repeat(CHUNK * 2);
        assert_eq!(s.load(Slot::Identity).unwrap(), None);
        s.save(Slot::Identity, &big).unwrap();
        s.save(Slot::State, "state").unwrap();
        assert_eq!(
            s.load(Slot::Identity).unwrap().as_deref(),
            Some(big.as_str())
        );
        assert_eq!(s.load(Slot::State).unwrap().as_deref(), Some("state"));
        s.clear().unwrap();
        assert_eq!(s.load(Slot::Identity).unwrap(), None);
        assert!(s.cells.keys().is_empty());
    }

    #[test]
    fn a_failed_save_keeps_the_old_value() {
        let s = store();
        let old = "a".repeat(CHUNK * 3 + 5);
        let new = "b".repeat(CHUNK * 4 + 9);
        s.save(Slot::Identity, &old).unwrap();
        for allowed in 0..5 {
            *s.cells.sets_left.lock().unwrap() = Some(allowed);
            assert!(s.save(Slot::Identity, &new).is_err());
            assert_eq!(
                s.load(Slot::Identity).unwrap().as_deref(),
                Some(old.as_str())
            );
        }
        *s.cells.sets_left.lock().unwrap() = None;
        s.save(Slot::Identity, "short").unwrap();
        assert_eq!(s.load(Slot::Identity).unwrap().as_deref(), Some("short"));
        assert_eq!(s.cells.keys().len(), 2);
    }

    #[test]
    fn the_device_namespace_is_not_the_bare_user_namespace() {
        let ns = device_namespace("https://tmgr.dev", 7);
        assert_ne!(ns, namespace("https://tmgr.dev", 7));
        assert!(ns.ends_with("#terminal"));
        let a = KeychainStore::new(MemCells::default(), ns);
        let b = KeychainStore::new(MemCells::default(), namespace("https://tmgr.dev", 7));
        assert_ne!(a.account(Slot::Identity), b.account(Slot::Identity));
    }

    #[test]
    fn accounts_differ_per_user_origin_and_slot() {
        let a = store();
        let b = KeychainStore::new(MemCells::default(), namespace("https://tmgr.dev", 8));
        assert_ne!(a.account(Slot::Identity), b.account(Slot::Identity));
        assert_ne!(a.account(Slot::Identity), a.account(Slot::State));
    }

    #[test]
    fn an_unreadable_pointer_is_an_error_not_a_panic() {
        let s = store();
        s.cells
            .set(&format!("{}.n", s.account(Slot::State)), "junk")
            .unwrap();
        assert!(s.load(Slot::State).is_err());
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn off_macos_keys_live_in_a_private_file_under_the_attach_dir_only() {
        use std::os::unix::fs::PermissionsExt;
        let t = tempfile::tempdir().unwrap();
        let s = open_store(t.path(), "ns").unwrap();
        s.save(Slot::Identity, "secret").unwrap();
        let f = t.path().join(super::super::DIR).join("identity");
        assert_eq!(
            std::fs::metadata(f).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(!t.path().join("relay").exists());
    }
}
