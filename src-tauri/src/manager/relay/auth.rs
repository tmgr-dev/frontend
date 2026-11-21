//! The login token, user id and API base the web UI hands to the desktop. Held in memory only.

use std::fmt;
use std::sync::Mutex;

use super::http::{checked_origin, LoginToken};

const TOKEN_LIMIT: usize = 8 << 10;

#[derive(Clone)]
pub struct RelayAuth {
  token: String,
  user_id: u64,
  api_base: String,
}

impl fmt::Debug for RelayAuth {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("RelayAuth").field("token", &"<redacted>").field("user_id", &self.user_id).field("api_base", &self.api_base).finish()
  }
}

impl RelayAuth {
  pub fn new(token: String, user_id: u64, api_base: &str) -> Result<Self, String> {
    if token.trim().is_empty() || token.len() > TOKEN_LIMIT {
      return Err("the login token is empty or too long".to_owned());
    }
    if user_id == 0 {
      return Err("the user id is not valid".to_owned());
    }
    let url = url::Url::parse(api_base).map_err(|_| "the relay address is not valid".to_owned())?;
    if url.query().is_some() || url.fragment().is_some() {
      return Err("the relay address is not one this app trusts".to_owned());
    }
    Ok(Self { token, user_id, api_base: checked_origin(api_base)? })
  }

  pub fn key(&self) -> (u64, String) {
    (self.user_id, self.api_base.clone())
  }
}

#[derive(Default)]
pub struct AuthCell(Mutex<Option<RelayAuth>>);

impl AuthCell {
  fn lock(&self) -> std::sync::MutexGuard<'_, Option<RelayAuth>> {
    self.0.lock().unwrap_or_else(|p| p.into_inner())
  }

  pub fn set(&self, auth: RelayAuth) {
    *self.lock() = Some(auth);
  }

  pub fn clear(&self) {
    *self.lock() = None;
  }

  pub fn key(&self) -> Option<(u64, String)> {
    self.lock().as_ref().map(RelayAuth::key)
  }
}

impl LoginToken for AuthCell {
  fn token(&self) -> Option<String> {
    self.lock().as_ref().map(|a| a.token.clone())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const SECRET: &str = "tok_SECRET_123";

  fn auth(base: &str) -> Result<RelayAuth, String> {
    RelayAuth::new(SECRET.into(), 7, base)
  }

  #[test]
  fn debug_redacts_the_token() {
    let a = auth("https://api.example.com/api/").unwrap();
    let text = format!("{a:?}");
    assert!(!text.contains(SECRET));
    assert!(text.contains("<redacted>"));
  }

  #[test]
  fn invalid_inputs_are_rejected_without_echoing_the_token() {
    let bad = [
      RelayAuth::new(String::new(), 7, "https://a.example"),
      RelayAuth::new("  ".into(), 7, "https://a.example"),
      RelayAuth::new(SECRET.repeat(TOKEN_LIMIT), 7, "https://a.example"),
      RelayAuth::new(SECRET.into(), 0, "https://a.example"),
      auth("http://api.example.com"),
      auth("ftp://localhost"),
      auth("not a url"),
      auth("https://user:pw@api.example.com"),
      auth("https://api.example.com/?x=1"),
      auth("https://api.example.com/#f"),
      auth(&format!("https://u:{SECRET}@api.example.com")),
      auth(&format!("https://api.example.com/?t={SECRET}")),
    ];
    for r in bad {
      let e = r.unwrap_err();
      assert!(!e.contains(SECRET), "{e}");
    }
  }

  #[test]
  fn loopback_http_is_allowed() {
    assert!(auth("http://127.0.0.1:8080/api/").is_ok());
    assert!(auth("http://localhost:8080/api/").is_ok());
    assert!(auth("http://evil.example").is_err());
  }

  #[test]
  fn cell_serves_the_latest_token_and_forgets_on_clear() {
    let cell = AuthCell::default();
    assert_eq!(cell.token(), None);
    cell.set(auth("https://api.example.com/api/").unwrap());
    assert_eq!(cell.token().as_deref(), Some(SECRET));
    assert_eq!(cell.key(), Some((7, "https://api.example.com".to_owned())));
    cell.set(RelayAuth::new("fresh".into(), 7, "https://api.example.com").unwrap());
    assert_eq!(cell.token().as_deref(), Some("fresh"));
    cell.clear();
    assert_eq!(cell.token(), None);
    assert_eq!(cell.key(), None);
  }
}
