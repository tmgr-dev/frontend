use std::path::Path;
use tmgr_chat_relay::engine::{KeyStore, Slot};
use tmgr_core::creds::{classify, Failure};
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

pub const NO_SECURE_STORE: &str = "No secure key store available on this machine (Secret Service on Linux). Pairing the terminal needs one.";

pub fn namespace(server: &str, user_id: u64) -> String {
    format!("u{user_id}@{server}")
}

pub fn device_namespace(server: &str, user_id: u64) -> String {
    format!("{}{TERMINAL_SUFFIX}", namespace(server, user_id))
}

pub fn config_dir(runner_dir: &Path) -> Result<RelayDir, String> {
    RelayDir::open_named(runner_dir, super::DIR)
}

pub trait Cells: Send + Sync {
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn set(&self, account: &str, value: &str) -> Result<(), String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

struct KeyringCells;

fn cell_error(e: keyring::Error) -> String {
    match classify(&e, cfg!(target_os = "linux")) {
        Failure::StoreAbsent => NO_SECURE_STORE.to_owned(),
        _ => e.to_string(),
    }
}

impl KeyringCells {
    fn entry(account: &str) -> Result<keyring::Entry, String> {
        keyring::Entry::new(SERVICE, account).map_err(cell_error)
    }
}

impl Cells for KeyringCells {
    fn get(&self, account: &str) -> Result<Option<String>, String> {
        match Self::entry(account)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(cell_error(e)),
        }
    }

    fn set(&self, account: &str, value: &str) -> Result<(), String> {
        Self::entry(account)?
            .set_password(value)
            .map_err(cell_error)
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(cell_error(e)),
        }
    }
}

pub fn open_store(namespace: &str) -> Result<DynStore, String> {
    open_with_cells(KeyringCells, namespace)
}

pub fn open_with_cells<C: Cells + 'static>(cells: C, namespace: &str) -> Result<DynStore, String> {
    cells.get(&format!("{namespace}/probe"))?;
    Ok(DynStore(Box::new(KeychainStore::new(
        cells,
        namespace.to_owned(),
    ))))
}

pub struct KeychainStore<C: Cells> {
    cells: C,
    namespace: String,
}

impl<C: Cells> KeychainStore<C> {
    pub fn new(cells: C, namespace: String) -> Self {
        Self { cells, namespace }
    }

    fn account(&self, slot: Slot) -> String {
        format!("{}/{}", self.namespace, slot.name())
    }
}

fn unreadable() -> String {
    "the attach device store is unreadable".to_owned()
}

fn chunk_name(name: &str, version: u64, i: usize) -> String {
    format!("{name}.v{version}.{i}")
}

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

fn drop_chunks(cells: &dyn Cells, name: &str, version: u64) -> Result<(), String> {
    let mut i = 0;
    while cells.get(&chunk_name(name, version, i))?.is_some() {
        cells.delete(&chunk_name(name, version, i))?;
        i += 1;
    }
    Ok(())
}

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

    struct AbsentCells;

    impl Cells for AbsentCells {
        fn get(&self, _: &str) -> Result<Option<String>, String> {
            Err(NO_SECURE_STORE.to_owned())
        }

        fn set(&self, _: &str, _: &str) -> Result<(), String> {
            panic!("keys must never be written when no secure store exists")
        }

        fn delete(&self, _: &str) -> Result<(), String> {
            Err(NO_SECURE_STORE.to_owned())
        }
    }

    #[test]
    fn an_absent_secure_store_is_refused_with_a_clear_message_and_nothing_is_written() {
        let e = open_with_cells(AbsentCells, "ns").err().expect("refused");
        assert!(
            e.contains("No secure key store available") && e.contains("Secret Service on Linux"),
            "{e}"
        );
    }

    #[test]
    fn a_present_store_is_opened_after_a_probe_that_writes_nothing() {
        let cells = MemCells::default();
        let store = open_with_cells(cells, "ns").unwrap();
        store.save(Slot::Identity, "k").unwrap();
        assert_eq!(store.load(Slot::Identity).unwrap().as_deref(), Some("k"));
    }

    #[test]
    fn linux_store_absence_is_classified_like_the_login_credentials() {
        assert_eq!(cell_error(keyring::Error::NoDefaultStore), NO_SECURE_STORE);
        let denied = keyring::Error::NoStorageAccess(std::io::Error::other("locked").into());
        assert_ne!(cell_error(denied), NO_SECURE_STORE);
    }
}
