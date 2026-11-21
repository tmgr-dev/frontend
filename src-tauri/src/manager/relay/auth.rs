//! The login token, user id and API base the web UI hands to the desktop. Held in memory only.

use std::fmt;
use std::sync::{Arc, Mutex};

use base64::Engine as _;

use super::http::{checked_origin, LoginToken};

const TOKEN_LIMIT: usize = 8 << 10;
const ALLOWED_ORIGIN: &str = match option_env!("TMGR_RELAY_ORIGIN") {
  Some(origin) => origin,
  None => "https://api.tmgr.dev",
};

fn pinned(origin: &str) -> bool {
  origin == ALLOWED_ORIGIN || (cfg!(debug_assertions) && (origin.starts_with("http://127.0.0.1") || origin.starts_with("http://localhost")))
}

fn subject_matches(token: &str, user_id: u64) -> bool {
  let mut parts = token.split('.');
  let (Some(_), Some(payload), Some(_), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else { return true };
  let Ok(bytes) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')) else { return true };
  let Ok(claims) = serde_json::from_slice::<serde_json::Value>(&bytes) else { return true };
  match claims.get("sub") {
    None => true,
    Some(serde_json::Value::String(sub)) => *sub == user_id.to_string(),
    Some(serde_json::Value::Number(sub)) => sub.as_u64() == Some(user_id),
    Some(_) => false,
  }
}

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
    let api_base = checked_origin(api_base)?;
    if !pinned(&api_base) {
      return Err("the relay address is not one this app trusts".to_owned());
    }
    if !subject_matches(&token, user_id) {
      return Err("the login token belongs to another user".to_owned());
    }
    Ok(Self { token, user_id, api_base })
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

pub struct BoundLogin {
  cell: Arc<AuthCell>,
  key: (u64, String),
}

impl BoundLogin {
  pub fn new(cell: Arc<AuthCell>, key: (u64, String)) -> Self {
    Self { cell, key }
  }
}

impl LoginToken for BoundLogin {
  fn token(&self) -> Option<String> {
    let guard = self.cell.lock();
    guard.as_ref().filter(|a| a.key() == self.key).map(|a| a.token.clone())
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
    let a = auth("https://api.tmgr.dev/api/").unwrap();
    let text = format!("{a:?}");
    assert!(!text.contains(SECRET));
    assert!(text.contains("<redacted>"));
  }

  #[test]
  fn invalid_inputs_are_rejected_without_echoing_the_token() {
    let bad = [
      RelayAuth::new(String::new(), 7, "https://api.tmgr.dev"),
      RelayAuth::new("  ".into(), 7, "https://api.tmgr.dev"),
      RelayAuth::new(SECRET.repeat(TOKEN_LIMIT), 7, "https://api.tmgr.dev"),
      RelayAuth::new(SECRET.into(), 0, "https://api.tmgr.dev"),
      auth("http://api.example.com"),
      auth("ftp://localhost"),
      auth("not a url"),
      auth("https://user:pw@api.example.com"),
      auth("https://api.tmgr.dev/?x=1"),
      auth("https://api.tmgr.dev/#f"),
      auth(&format!("https://u:{SECRET}@api.example.com")),
      auth(&format!("https://api.tmgr.dev/?t={SECRET}")),
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
    cell.set(auth("https://api.tmgr.dev/api/").unwrap());
    assert_eq!(cell.token().as_deref(), Some(SECRET));
    assert_eq!(cell.key(), Some((7, "https://api.tmgr.dev".to_owned())));
    cell.set(RelayAuth::new("fresh".into(), 7, "https://api.tmgr.dev").unwrap());
    assert_eq!(cell.token().as_deref(), Some("fresh"));
    cell.clear();
    assert_eq!(cell.token(), None);
    assert_eq!(cell.key(), None);
  }

  fn jwt(claims: &str) -> String {
    let enc = |v: &str| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(v);
    format!("{}.{}.sig", enc("{\"alg\":\"none\"}"), enc(claims))
  }

  #[test]
  fn only_the_pinned_origin_is_accepted() {
    assert!(RelayAuth::new("t".into(), 7, "https://api.tmgr.dev/api/").is_ok());
    for base in ["https://api.example.com", "https://tmgr.dev", "https://api.tmgr.dev.evil.com", "https://api.tmgr.dev:8443", "http://api.tmgr.dev"] {
      assert!(RelayAuth::new("t".into(), 7, base).is_err(), "{base}");
    }
  }

  #[test]
  fn jwt_subject_must_match_the_user_id() {
    let ok = |claims: &str, user| RelayAuth::new(jwt(claims), user, "https://api.tmgr.dev").is_ok();
    assert!(ok(r#"{"sub":"7"}"#, 7));
    assert!(ok(r#"{"sub":7}"#, 7));
    assert!(ok(r#"{"aud":"x"}"#, 7));
    assert!(!ok(r#"{"sub":"8"}"#, 7));
    assert!(!ok(r#"{"sub":8}"#, 7));
    assert!(!ok(r#"{"sub":null}"#, 7));
    assert!(RelayAuth::new("opaque-token".into(), 7, "https://api.tmgr.dev").is_ok());
    let e = RelayAuth::new(jwt(r#"{"sub":"8"}"#), 7, "https://api.tmgr.dev").unwrap_err();
    assert!(!e.contains("sig"), "{e}");
  }

  #[test]
  fn bound_login_dies_when_the_cell_moves_to_another_account() {
    let cell = Arc::new(AuthCell::default());
    cell.set(RelayAuth::new("one".into(), 7, "https://api.tmgr.dev").unwrap());
    let bound = BoundLogin::new(cell.clone(), cell.key().unwrap());
    assert_eq!(bound.token().as_deref(), Some("one"));
    cell.set(RelayAuth::new("one2".into(), 7, "https://api.tmgr.dev").unwrap());
    assert_eq!(bound.token().as_deref(), Some("one2"));
    cell.set(RelayAuth::new("two".into(), 8, "https://api.tmgr.dev").unwrap());
    assert_eq!(bound.token(), None);
    cell.clear();
    assert_eq!(bound.token(), None);
  }
}
