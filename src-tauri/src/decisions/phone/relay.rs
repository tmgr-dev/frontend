use std::fmt;
use std::time::Duration;

use reqwest::Method;
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};

const BODY_LIMIT: usize = 4 << 20;

#[derive(Clone)]
pub struct Conn {
  pub api_base: String,
  pub mailbox: String,
  pub token: String,
}

impl fmt::Debug for Conn {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Conn").field("mailbox", &self.mailbox).field("token", &"<redacted>").finish_non_exhaustive()
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayError {
  Status(u16),
  Net(String),
  Bad(String),
}

impl fmt::Display for RelayError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      RelayError::Status(s) => write!(f, "the relay answered HTTP {s}"),
      RelayError::Net(m) | RelayError::Bad(m) => f.write_str(m),
    }
  }
}

#[derive(Clone, Deserialize)]
pub struct RelayMsg {
  #[serde(deserialize_with = "id_from_int_or_string")]
  pub id: String,
  pub kind: String,
  #[serde(rename = "ref", default)]
  pub reference: String,
  pub body: String,
}

fn id_from_int_or_string<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
  #[derive(Deserialize)]
  #[serde(untagged)]
  enum Id {
    Int(i64),
    Text(String),
  }
  Ok(match Id::deserialize(d)? {
    Id::Int(n) => n.to_string(),
    Id::Text(s) => s,
  })
}

impl fmt::Debug for RelayMsg {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("RelayMsg").field("id", &self.id).field("kind", &self.kind).finish_non_exhaustive()
  }
}

pub type RelayResult<T> = Result<T, RelayError>;

pub fn parse_messages(raw: &[u8]) -> RelayResult<Vec<RelayMsg>> {
  let unreadable = || RelayError::Bad("the relay answer is not readable".into());
  let v: Value = serde_json::from_slice(raw).map_err(|_| unreadable())?;
  let list = v.get("data").cloned().unwrap_or(v);
  serde_json::from_value(list).map_err(|_| unreadable())
}

pub fn usable(msgs: Vec<RelayMsg>) -> impl Iterator<Item = RelayMsg> {
  msgs.into_iter().filter(|m| safe_segment(&m.id)).take(MESSAGES_PER_POLL)
}

pub trait Relay: Send + Sync {
  fn put_snapshot(&self, c: &Conn, body: &str, nudge: bool) -> RelayResult<()>;
  fn put_phone_token(&self, c: &Conn, token: &str) -> RelayResult<()>;
  fn list_messages(&self, c: &Conn) -> RelayResult<Vec<RelayMsg>>;
  fn post_message(&self, c: &Conn, kind: &str, reference: &str, body: &str) -> RelayResult<()>;
  fn delete_message(&self, c: &Conn, id: &str) -> RelayResult<()>;
  fn delete_mailbox(&self, c: &Conn) -> RelayResult<()>;
}

pub struct HttpRelay;

pub const MESSAGES_PER_POLL: usize = 100;

pub fn safe_segment(s: &str) -> bool {
  !s.is_empty() && s.len() <= 64 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl HttpRelay {
  fn url(c: &Conn, tail: &str) -> RelayResult<String> {
    let base =
      crate::local_move::trusted_api_base(&c.api_base, crate::local_move::EXTRA_API).map_err(RelayError::Bad)?;
    if !safe_segment(&c.mailbox) {
      return Err(RelayError::Bad("the mailbox id is not valid".into()));
    }
    Ok(format!("{base}phone-relay/mailboxes/{}{tail}", c.mailbox))
  }

  fn call(c: &Conn, method: Method, tail: &str, body: Option<Value>) -> RelayResult<Vec<u8>> {
    let url = Self::url(c, tail)?;
    crate::local_move::ensure_tls_provider();
    tauri::async_runtime::block_on(async move {
      let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|e| RelayError::Net(e.without_url().to_string()))?;
      let mut req = client
        .request(method, url)
        .header(reqwest::header::AUTHORIZATION, format!("Bearer {}", c.token))
        .header(reqwest::header::ACCEPT, "application/json");
      if let Some(b) = body {
        req = req.header(reqwest::header::CONTENT_TYPE, "application/json").body(b.to_string());
      }
      let mut resp = req.send().await.map_err(|e| RelayError::Net(e.without_url().to_string()))?;
      let status = resp.status();
      if !status.is_success() {
        return Err(RelayError::Status(status.as_u16()));
      }
      let mut out = Vec::new();
      while out.len() < BODY_LIMIT {
        match resp.chunk().await.map_err(|e| RelayError::Net(e.without_url().to_string()))? {
          Some(chunk) => out.extend_from_slice(&chunk),
          None => break,
        }
      }
      Ok(out)
    })
  }
}

impl Relay for HttpRelay {
  fn put_snapshot(&self, c: &Conn, body: &str, nudge: bool) -> RelayResult<()> {
    Self::call(c, Method::PUT, "/snapshot", Some(json!({ "body": body, "nudge": nudge }))).map(|_| ())
  }

  fn put_phone_token(&self, c: &Conn, token: &str) -> RelayResult<()> {
    Self::call(c, Method::PUT, "/phone-token", Some(json!({ "phone_token": token }))).map(|_| ())
  }

  fn list_messages(&self, c: &Conn) -> RelayResult<Vec<RelayMsg>> {
    parse_messages(&Self::call(c, Method::GET, "/messages", None)?)
  }

  fn post_message(&self, c: &Conn, kind: &str, reference: &str, body: &str) -> RelayResult<()> {
    Self::call(c, Method::POST, "/messages", Some(json!({ "kind": kind, "ref": reference, "body": body }))).map(|_| ())
  }

  fn delete_message(&self, c: &Conn, id: &str) -> RelayResult<()> {
    if !safe_segment(id) {
      return Err(RelayError::Bad("the message id is not valid".into()));
    }
    Self::call(c, Method::DELETE, &format!("/messages/{id}"), None).map(|_| ())
  }

  fn delete_mailbox(&self, c: &Conn) -> RelayResult<()> {
    Self::call(c, Method::DELETE, "", None).map(|_| ())
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const NUMERIC: &str = r#"{"id":42,"kind":"hello","ref":"hello","body":"x","created_at":"2026-11-08T16:00:00Z"}"#;

  #[test]
  fn numeric_id_in_data_wrapper_is_read() {
    let msgs = parse_messages(format!(r#"{{"data":[{NUMERIC}]}}"#).as_bytes()).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].id, "42");
    assert_eq!(msgs[0].kind, "hello");
  }

  #[test]
  fn numeric_id_in_bare_array_is_read() {
    let msgs = parse_messages(format!("[{NUMERIC}]").as_bytes()).unwrap();
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].id, "42");
  }

  #[test]
  fn string_id_is_still_read() {
    let raw = r#"{"data":[{"id":"abc-1","kind":"answer","ref":"r","body":"x"}]}"#;
    assert_eq!(parse_messages(raw.as_bytes()).unwrap()[0].id, "abc-1");
  }

  #[test]
  fn empty_list_is_read() {
    assert!(parse_messages(br#"{"data":[]}"#).unwrap().is_empty());
    assert!(parse_messages(b"[]").unwrap().is_empty());
  }

  #[test]
  fn garbage_is_refused() {
    assert!(parse_messages(b"not json").is_err());
    assert!(parse_messages(br#"{"data":[{"id":1.5,"kind":"k","body":"b"}]}"#).is_err());
  }
}

#[cfg(test)]
pub mod fake {
  use super::*;
  use std::sync::Mutex;

  #[derive(Default)]
  pub struct State {
    pub snapshots: Vec<(String, bool)>,
    pub inbox: Vec<RelayMsg>,
    pub posted: Vec<(String, String, String)>,
    pub deleted: Vec<String>,
    pub mailbox_deleted: bool,
    pub phone_token: Option<String>,
    pub fail_snapshot: bool,
    pub fail_post: bool,
    next: u64,
  }

  #[derive(Default)]
  pub struct FakeRelay(pub Mutex<State>);

  impl FakeRelay {
    pub fn push(&self, kind: &str, reference: &str, body: &str) -> String {
      let id = {
        let mut s = self.0.lock().unwrap();
        s.next += 1;
        format!("m{}", s.next)
      };
      self.push_with_id(&id, kind, reference, body)
    }

    pub fn push_with_id(&self, id: &str, kind: &str, reference: &str, body: &str) -> String {
      let mut s = self.0.lock().unwrap();
      s.inbox.push(RelayMsg { id: id.into(), kind: kind.into(), reference: reference.into(), body: body.into() });
      id.to_owned()
    }
  }

  impl Relay for FakeRelay {
    fn put_snapshot(&self, _: &Conn, body: &str, nudge: bool) -> RelayResult<()> {
      let mut s = self.0.lock().unwrap();
      if s.fail_snapshot {
        return Err(RelayError::Status(503));
      }
      s.snapshots.push((body.into(), nudge));
      Ok(())
    }

    fn put_phone_token(&self, _: &Conn, token: &str) -> RelayResult<()> {
      self.0.lock().unwrap().phone_token = Some(token.into());
      Ok(())
    }

    fn list_messages(&self, _: &Conn) -> RelayResult<Vec<RelayMsg>> {
      Ok(self.0.lock().unwrap().inbox.clone())
    }

    fn post_message(&self, _: &Conn, kind: &str, reference: &str, body: &str) -> RelayResult<()> {
      let mut s = self.0.lock().unwrap();
      if s.fail_post {
        return Err(RelayError::Status(503));
      }
      s.posted.push((kind.into(), reference.into(), body.into()));
      Ok(())
    }

    fn delete_message(&self, _: &Conn, id: &str) -> RelayResult<()> {
      let mut s = self.0.lock().unwrap();
      s.inbox.retain(|m| m.id != id);
      s.deleted.push(id.into());
      Ok(())
    }

    fn delete_mailbox(&self, _: &Conn) -> RelayResult<()> {
      self.0.lock().unwrap().mailbox_deleted = true;
      Ok(())
    }
  }
}
