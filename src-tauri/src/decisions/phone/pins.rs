#[cfg(test)]
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

pub const SERVICE: &str = "dev.tmgr.desktop.phone-relay";
const INDEX: &str = "index";
pub const MAX_PHONES: usize = 3;
pub const EXPIRY_MS: i64 = 30 * 24 * 3600 * 1000;

pub trait PinStore: Send + Sync {
  fn get(&self, account: &str) -> Result<Option<String>, String>;
  fn set(&self, account: &str, value: &str) -> Result<(), String>;
  fn delete(&self, account: &str) -> Result<(), String>;
}

pub struct Keychain;

impl Keychain {
  fn entry(account: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(SERVICE, account).map_err(|e| e.to_string())
  }
}

impl PinStore for Keychain {
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

#[cfg(test)]
#[derive(Default)]
pub struct MemStore(Mutex<HashMap<String, String>>);

#[cfg(test)]
impl PinStore for MemStore {
  fn get(&self, account: &str) -> Result<Option<String>, String> {
    Ok(self.0.lock().unwrap_or_else(|p| p.into_inner()).get(account).cloned())
  }

  fn set(&self, account: &str, value: &str) -> Result<(), String> {
    self.0.lock().unwrap_or_else(|p| p.into_inner()).insert(account.to_owned(), value.to_owned());
    Ok(())
  }

  fn delete(&self, account: &str) -> Result<(), String> {
    self.0.lock().unwrap_or_else(|p| p.into_inner()).remove(account);
    Ok(())
  }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Pin {
  pub mailbox: String,
  pub api_base: String,
  pub user_id: i64,
  pub desktop_sign_secret: String,
  pub desktop_kex_secret: String,
  pub desktop_fp: String,
  pub desktop_token: String,
  pub desktop_name: String,
  pub phone_sign_pub: String,
  pub phone_kex_pub: String,
  pub phone_fp: String,
  pub phone_name: String,
  pub last_seq: i64,
  pub paired_ms: i64,
  pub last_used_ms: i64,
}

impl fmt::Debug for Pin {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Pin")
      .field("mailbox", &self.mailbox)
      .field("phone_fp", &self.phone_fp)
      .field("keys", &"<redacted>")
      .field("desktop_token", &"<redacted>")
      .finish_non_exhaustive()
  }
}

pub struct Keys {
  pub sign_secret: Vec<u8>,
  pub kex_secret: Vec<u8>,
  pub phone_sign_pub: Vec<u8>,
  pub phone_kex_pub: Vec<u8>,
}

impl Pin {
  pub fn expired(&self, now_ms: i64) -> bool {
    now_ms.saturating_sub(self.last_used_ms.max(self.paired_ms)) > EXPIRY_MS
  }

  pub fn keys(&self) -> Result<Keys, String> {
    let dec = |s: &str| tmgr_phone::b64_decode(s).map_err(|e| e.to_string());
    Ok(Keys {
      sign_secret: dec(&self.desktop_sign_secret)?,
      kex_secret: dec(&self.desktop_kex_secret)?,
      phone_sign_pub: dec(&self.phone_sign_pub)?,
      phone_kex_pub: dec(&self.phone_kex_pub)?,
    })
  }

  pub fn conn(&self) -> super::relay::Conn {
    super::relay::Conn {
      api_base: self.api_base.clone(),
      mailbox: self.mailbox.clone(),
      token: self.desktop_token.clone(),
    }
  }
}

pub struct Pins {
  store: Arc<dyn PinStore>,
  gate: Mutex<()>,
}

impl Pins {
  pub fn new(store: Arc<dyn PinStore>) -> Pins {
    Pins { store, gate: Mutex::new(()) }
  }

  fn index(&self) -> Result<Vec<String>, String> {
    match self.store.get(INDEX)? {
      Some(raw) => serde_json::from_str(&raw).map_err(|_| "the phone index is unreadable".to_owned()),
      None => Ok(Vec::new()),
    }
  }

  fn write_index(&self, ids: &[String]) -> Result<(), String> {
    self.store.set(INDEX, &serde_json::to_string(ids).map_err(|e| e.to_string())?)
  }

  pub fn get(&self, mailbox: &str) -> Result<Option<Pin>, String> {
    let _g = self.gate.lock().unwrap_or_else(|p| p.into_inner());
    self.read(mailbox)
  }

  fn read(&self, mailbox: &str) -> Result<Option<Pin>, String> {
    match self.store.get(mailbox)? {
      Some(raw) => Ok(serde_json::from_str(&raw).ok()),
      None => Ok(None),
    }
  }

  pub fn list(&self) -> Result<Vec<Pin>, String> {
    let _g = self.gate.lock().unwrap_or_else(|p| p.into_inner());
    let mut out = Vec::new();
    for id in self.index()? {
      if let Some(pin) = self.read(&id)? {
        out.push(pin);
      }
    }
    Ok(out)
  }

  pub fn put(&self, pin: &Pin) -> Result<(), String> {
    let _g = self.gate.lock().unwrap_or_else(|p| p.into_inner());
    let mut ids = self.index()?;
    self.store.set(&pin.mailbox, &serde_json::to_string(pin).map_err(|e| e.to_string())?)?;
    if !ids.contains(&pin.mailbox) {
      ids.push(pin.mailbox.clone());
      self.write_index(&ids)?;
    }
    Ok(())
  }

  pub fn delete(&self, mailbox: &str) -> Result<(), String> {
    let _g = self.gate.lock().unwrap_or_else(|p| p.into_inner());
    self.store.delete(mailbox)?;
    let mut ids = self.index()?;
    if ids.iter().any(|i| i == mailbox) {
      ids.retain(|i| i != mailbox);
      self.write_index(&ids)?;
    }
    Ok(())
  }
}
