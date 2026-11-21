//! Ports of the relay engine: HTTP transport and secret/state storage.

use std::collections::HashMap;
use std::sync::Mutex;

use tmgr_chat_relay::api::Request;

pub trait Transport: Send + Sync {
  fn call(&self, req: &Request, device_token: Option<&str>) -> Result<String, String>;
}

// STUB: backend chat-relay (Liquibase 230) is not shipped yet
pub struct StubTransport;

impl Transport for StubTransport {
  fn call(&self, _: &Request, _: Option<&str>) -> Result<String, String> {
    Err("chat sync is not available yet".into())
  }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Slot {
  Identity,
  State,
  Log,
}

impl Slot {
  fn name(self) -> &'static str {
    match self {
      Slot::Identity => "identity",
      Slot::State => "state",
      Slot::Log => "log",
    }
  }

  const ALL: [Slot; 3] = [Slot::Identity, Slot::State, Slot::Log];
}

pub trait KeyStore: Send + Sync {
  fn load(&self, slot: Slot) -> Result<Option<String>, String>;
  fn save(&self, slot: Slot, value: &str) -> Result<(), String>;
  fn clear(&self) -> Result<(), String>;
}

#[derive(Default)]
pub struct MemKeyStore(Mutex<HashMap<Slot, String>>);

impl KeyStore for MemKeyStore {
  fn load(&self, slot: Slot) -> Result<Option<String>, String> {
    Ok(self.0.lock().unwrap_or_else(|p| p.into_inner()).get(&slot).cloned())
  }

  fn save(&self, slot: Slot, value: &str) -> Result<(), String> {
    self.0.lock().unwrap_or_else(|p| p.into_inner()).insert(slot, value.to_owned());
    Ok(())
  }

  fn clear(&self) -> Result<(), String> {
    self.0.lock().unwrap_or_else(|p| p.into_inner()).clear();
    Ok(())
  }
}

pub const SERVICE: &str = "dev.tmgr.desktop.chat-relay";
const CHUNK: usize = 1800;

trait Cells {
  fn get(&self, account: &str) -> Result<Option<String>, String>;
  fn set(&self, account: &str, value: &str) -> Result<(), String>;
  fn delete(&self, account: &str) -> Result<(), String>;
}

struct KeyringCells;

impl KeyringCells {
  fn entry(account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())
  }
}

impl Cells for KeyringCells {
  fn get(&self, account: &str) -> Result<Option<String>, String> {
    match Self::entry(account)?.get_password() {
      Ok(v) => Ok(Some(v)),
      Err(keyring::Error::NoEntry) => Ok(None),
      Err(e) => Err(e.to_string()),
    }
  }

  fn set(&self, account: &str, value: &str) -> Result<(), String> {
    Self::entry(account)?.set_password(value).map_err(|e| e.to_string())
  }

  fn delete(&self, account: &str) -> Result<(), String> {
    match Self::entry(account)?.delete_credential() {
      Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
      Err(e) => Err(e.to_string()),
    }
  }
}

pub struct KeychainStore {
  namespace: String,
}

impl KeychainStore {
  pub fn new(namespace: String) -> Self {
    Self { namespace }
  }

  fn account(&self, slot: Slot) -> String {
    scoped(&self.namespace, slot.name())
  }
}

fn scoped(namespace: &str, name: &str) -> String {
  format!("{namespace}/{name}")
}

fn unreadable() -> String {
  "the chat store is unreadable".to_owned()
}

fn chunk_name(name: &str, version: Option<u64>, i: usize) -> String {
  match version {
    Some(v) => format!("{name}.v{v}.{i}"),
    None => format!("{name}.{i}"),
  }
}

/// The pointer `<name>.n` is `"<version>:<count>"`; a bare `"<count>"` is the unversioned legacy layout.
fn pointer(cells: &dyn Cells, name: &str) -> Result<Option<(Option<u64>, usize)>, String> {
  let Some(raw) = cells.get(&format!("{name}.n"))? else { return Ok(None) };
  match raw.split_once(':') {
    Some((v, n)) => Ok(Some((Some(v.parse().map_err(|_| unreadable())?), n.parse().map_err(|_| unreadable())?))),
    None => Ok(Some((None, raw.parse().map_err(|_| unreadable())?))),
  }
}

fn drop_chunks(cells: &dyn Cells, name: &str, version: Option<u64>) -> Result<(), String> {
  let mut i = 0;
  while cells.get(&chunk_name(name, version, i))?.is_some() {
    cells.delete(&chunk_name(name, version, i))?;
    i += 1;
  }
  Ok(())
}

// Values are split into chunks because a Windows credential holds only ~2.5 KB. A save writes a new chunk set
// and only then flips the pointer, so a crash leaves either the old value or the new one, never a mix.
fn load_chunked(cells: &dyn Cells, name: &str) -> Result<Option<String>, String> {
  let Some((version, count)) = pointer(cells, name)? else { return Ok(None) };
  let mut out = String::new();
  for i in 0..count {
    out.push_str(&cells.get(&chunk_name(name, version, i))?.ok_or_else(unreadable)?);
  }
  Ok(Some(out))
}

fn save_chunked(cells: &dyn Cells, name: &str, value: &str) -> Result<(), String> {
  let old = pointer(cells, name)?;
  let next = old.and_then(|(v, _)| v).map_or(1, |v| v + 1);
  let chunks = chunks(value);
  for (i, c) in chunks.iter().enumerate() {
    cells.set(&chunk_name(name, Some(next), i), c)?;
  }
  cells.set(&format!("{name}.n"), &format!("{next}:{}", chunks.len()))?;
  for i in chunks.len().. {
    let stale = chunk_name(name, Some(next), i);
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
  let current = pointer(cells, name)?.and_then(|(v, _)| v);
  cells.delete(&format!("{name}.n"))?;
  drop_chunks(cells, name, None)?;
  drop_chunks(cells, name, current)?;
  drop_chunks(cells, name, Some(current.map_or(1, |v| v + 1)))
}

impl KeyStore for KeychainStore {
  fn load(&self, slot: Slot) -> Result<Option<String>, String> {
    load_chunked(&KeyringCells, &self.account(slot))
  }

  fn save(&self, slot: Slot, value: &str) -> Result<(), String> {
    save_chunked(&KeyringCells, &self.account(slot), value)
  }

  fn clear(&self) -> Result<(), String> {
    Slot::ALL.into_iter().try_for_each(|slot| clear_chunked(&KeyringCells, &self.account(slot)))
  }
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn mem_store_round_trip_and_clear() {
    let s = MemKeyStore::default();
    assert_eq!(s.load(Slot::State).unwrap(), None);
    s.save(Slot::State, "x").unwrap();
    assert_eq!(s.load(Slot::State).unwrap().as_deref(), Some("x"));
    s.clear().unwrap();
    assert_eq!(s.load(Slot::State).unwrap(), None);
  }

  #[test]
  fn keychain_accounts_differ_per_user_and_origin() {
    let a = KeychainStore::new("u7@https://api.tmgr.dev".into());
    let b = KeychainStore::new("u8@https://api.tmgr.dev".into());
    let c = KeychainStore::new("u7@http://127.0.0.1:8080".into());
    assert_ne!(a.account(Slot::Identity), b.account(Slot::Identity));
    assert_ne!(a.account(Slot::Identity), c.account(Slot::Identity));
    assert_ne!(a.account(Slot::Identity), a.account(Slot::State));
  }

  #[test]
  fn stub_transport_says_not_available() {
    let req = tmgr_chat_relay::api::list_devices();
    assert_eq!(StubTransport.call(&req, None).unwrap_err(), "chat sync is not available yet");
  }

  #[derive(Default)]
  struct FlakyCells {
    map: Mutex<HashMap<String, String>>,
    sets_left: Mutex<Option<usize>>,
  }

  impl Cells for FlakyCells {
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
      self.map.lock().unwrap().insert(account.to_owned(), value.to_owned());
      Ok(())
    }
    fn delete(&self, account: &str) -> Result<(), String> {
      self.map.lock().unwrap().remove(account);
      Ok(())
    }
  }

  #[test]
  fn chunked_save_survives_a_crash_between_chunks_and_pointer() {
    let cells = FlakyCells::default();
    let old = "a".repeat(CHUNK * 3 + 5);
    let new = "b".repeat(CHUNK * 4 + 9);
    save_chunked(&cells, "identity", &old).unwrap();
    for allowed in 0..5 {
      *cells.sets_left.lock().unwrap() = Some(allowed);
      assert!(save_chunked(&cells, "identity", &new).is_err());
      assert_eq!(load_chunked(&cells, "identity").unwrap().as_deref(), Some(old.as_str()));
    }
    *cells.sets_left.lock().unwrap() = None;
    save_chunked(&cells, "identity", &new).unwrap();
    assert_eq!(load_chunked(&cells, "identity").unwrap().as_deref(), Some(new.as_str()));
    save_chunked(&cells, "identity", "short").unwrap();
    assert_eq!(load_chunked(&cells, "identity").unwrap().as_deref(), Some("short"));
    assert_eq!(cells.map.lock().unwrap().len(), 2);
  }

  #[test]
  fn chunked_load_reads_the_legacy_layout_and_clear_leaves_nothing() {
    let cells = FlakyCells::default();
    cells.set("log.0", "he").unwrap();
    cells.set("log.1", "llo").unwrap();
    cells.set("log.n", "2").unwrap();
    assert_eq!(load_chunked(&cells, "log").unwrap().as_deref(), Some("hello"));
    save_chunked(&cells, "log", "world").unwrap();
    assert_eq!(load_chunked(&cells, "log").unwrap().as_deref(), Some("world"));
    assert!(cells.get("log.0").unwrap().is_none());
    cells.set("log.v2.0", "orphan").unwrap();
    clear_chunked(&cells, "log").unwrap();
    assert!(cells.map.lock().unwrap().is_empty());
  }
}
