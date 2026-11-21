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

pub struct KeychainStore;

impl KeychainStore {
  fn entry(account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())
  }

  fn get(account: &str) -> Result<Option<String>, String> {
    match Self::entry(account)?.get_password() {
      Ok(v) => Ok(Some(v)),
      Err(keyring::Error::NoEntry) => Ok(None),
      Err(e) => Err(e.to_string()),
    }
  }

  fn set(account: &str, value: &str) -> Result<(), String> {
    Self::entry(account)?.set_password(value).map_err(|e| e.to_string())
  }

  fn delete(account: &str) -> Result<(), String> {
    match Self::entry(account)?.delete_credential() {
      Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
      Err(e) => Err(e.to_string()),
    }
  }

  fn drop_chunks(name: &str, from: usize) -> Result<(), String> {
    let count = Self::get(&format!("{name}.n"))?.and_then(|n| n.parse::<usize>().ok()).unwrap_or(0);
    for i in from..count {
      Self::delete(&format!("{name}.{i}"))?;
    }
    Ok(())
  }
}

// Values are split into chunks because a Windows credential holds only ~2.5 KB.
impl KeyStore for KeychainStore {
  fn load(&self, slot: Slot) -> Result<Option<String>, String> {
    let name = slot.name();
    let Some(count) = Self::get(&format!("{name}.n"))? else { return Ok(None) };
    let count: usize = count.parse().map_err(|_| "the chat store is unreadable".to_owned())?;
    let mut out = String::new();
    for i in 0..count {
      out.push_str(&Self::get(&format!("{name}.{i}"))?.ok_or_else(|| "the chat store is unreadable".to_owned())?);
    }
    Ok(Some(out))
  }

  fn save(&self, slot: Slot, value: &str) -> Result<(), String> {
    let name = slot.name();
    let chunks: Vec<String> = chunks(value);
    for (i, c) in chunks.iter().enumerate() {
      Self::set(&format!("{name}.{i}"), c)?;
    }
    Self::set(&format!("{name}.n"), &chunks.len().to_string())?;
    Self::drop_chunks(name, chunks.len())
  }

  fn clear(&self) -> Result<(), String> {
    for slot in Slot::ALL {
      let name = slot.name();
      Self::drop_chunks(name, 0)?;
      Self::delete(&format!("{name}.n"))?;
    }
    Ok(())
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
  fn stub_transport_says_not_available() {
    let req = tmgr_chat_relay::api::list_devices();
    assert_eq!(StubTransport.call(&req, None).unwrap_err(), "chat sync is not available yet");
  }
}
