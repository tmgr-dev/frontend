use std::io::{self, Read, Write};

use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;
use subtle::ConstantTimeEq;

pub const PROTO_VERSION: u64 = 1;
pub const MAX_JSON: usize = 1 << 20;
pub const MAX_BIN: usize = 4 << 20;
pub const TAG_INPUT: u8 = 3;
pub const MAX_INPUT: usize = 64 << 10;
const KIND_JSON: u8 = 0;
const KIND_BIN: u8 = 1;
const SERVER: &str = "tmgr-runner-server";
const CLIENT: &str = "tmgr-runner-client";

#[derive(Debug, Clone, PartialEq)]
pub struct BinFrame {
  pub tag: u8,
  pub session: String,
  pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
  Json(Value),
  Bin(BinFrame),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConnError {
  Absent(String),
  Incompatible(String),
  Failed(String),
}

impl ConnError {
  pub fn message(&self) -> &str {
    match self {
      ConnError::Absent(m) | ConnError::Incompatible(m) | ConnError::Failed(m) => m,
    }
  }
}

impl From<io::Error> for ConnError {
  fn from(e: io::Error) -> Self {
    match e.kind() {
      io::ErrorKind::NotFound | io::ErrorKind::ConnectionRefused => ConnError::Absent("Agent runner is not running".to_owned()),
      io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock => ConnError::Failed("Agent runner did not answer in time".to_owned()),
      _ => ConnError::Failed(e.to_string()),
    }
  }
}

pub fn valid_session_id(s: &str) -> bool {
  match s.strip_prefix("s_") {
    Some(rest) => rest.len() == 10 && rest.bytes().all(|c| matches!(c, b'a'..=b'z' | b'2'..=b'7')),
    None => false,
  }
}

fn bad(msg: impl Into<String>) -> io::Error {
  io::Error::new(io::ErrorKind::InvalidData, msg.into())
}

fn limit(kind: u8) -> io::Result<usize> {
  match kind {
    KIND_JSON => Ok(MAX_JSON),
    KIND_BIN => Ok(MAX_BIN),
    k => Err(bad(format!("unknown frame kind {k}"))),
  }
}

fn encode(kind: u8, payload: &[u8]) -> io::Result<Vec<u8>> {
  if payload.len() > limit(kind)? {
    return Err(bad("frame too large"));
  }
  let mut out = Vec::with_capacity(payload.len() + 5);
  out.extend_from_slice(&((payload.len() + 1) as u32).to_be_bytes());
  out.push(kind);
  out.extend_from_slice(payload);
  Ok(out)
}

pub fn encode_frame(frame: &Frame) -> io::Result<Vec<u8>> {
  match frame {
    Frame::Json(v) => encode(KIND_JSON, &serde_json::to_vec(v).map_err(io::Error::other)?),
    Frame::Bin(b) => {
      if !b.session.is_empty() && !valid_session_id(&b.session) {
        return Err(bad("bad session id"));
      }
      let mut p = Vec::with_capacity(b.data.len() + b.session.len() + 2);
      p.push(b.tag);
      p.push(b.session.len() as u8);
      p.extend_from_slice(b.session.as_bytes());
      p.extend_from_slice(&b.data);
      encode(KIND_BIN, &p)
    }
  }
}

pub fn write_frame<W: Write>(w: &mut W, frame: &Frame) -> io::Result<()> {
  w.write_all(&encode_frame(frame)?)
}

pub fn write_json<W: Write>(w: &mut W, value: &Value) -> io::Result<()> {
  w.write_all(&encode(KIND_JSON, &serde_json::to_vec(value).map_err(io::Error::other)?)?)
}

fn decode(kind: u8, payload: &[u8]) -> io::Result<Frame> {
  match kind {
    KIND_JSON => serde_json::from_slice(payload).map(Frame::Json).map_err(|e| bad(format!("bad json: {e}"))),
    KIND_BIN => {
      let [tag, sid_len, rest @ ..] = payload else {
        return Err(bad("short binary frame"));
      };
      let n = *sid_len as usize;
      if rest.len() < n {
        return Err(bad("short binary frame"));
      }
      let (sid, data) = rest.split_at(n);
      let sid = std::str::from_utf8(sid).map_err(|_| bad("bad session id"))?;
      if !sid.is_empty() && !valid_session_id(sid) {
        return Err(bad("bad session id"));
      }
      Ok(Frame::Bin(BinFrame { tag: *tag, session: sid.to_owned(), data: data.to_vec() }))
    }
    k => Err(bad(format!("unknown frame kind {k}"))),
  }
}

pub fn read_frame<R: Read>(r: &mut R) -> io::Result<Option<Frame>> {
  let mut hdr = [0u8; 4];
  let mut got = 0;
  while got < hdr.len() {
    match r.read(&mut hdr[got..]) {
      Ok(0) if got == 0 => return Ok(None),
      Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
      Ok(n) => got += n,
      Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
      Err(e) => return Err(e),
    }
  }
  let len = u32::from_be_bytes(hdr) as usize;
  if len == 0 || len - 1 > MAX_BIN {
    return Err(bad("bad frame length"));
  }
  let mut kind = [0u8; 1];
  r.read_exact(&mut kind)?;
  if len - 1 > limit(kind[0])? {
    return Err(bad("frame too large"));
  }
  let mut payload = vec![0u8; len - 1];
  r.read_exact(&mut payload)?;
  decode(kind[0], &payload).map(Some)
}

fn hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn proof(secret: &str, label: &str, first: &str, second: &str) -> String {
  let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(secret.as_bytes()).expect("hmac accepts any key length");
  mac.update(label.as_bytes());
  mac.update(&[0]);
  mac.update(first.as_bytes());
  mac.update(second.as_bytes());
  hex(&mac.finalize().into_bytes())
}

fn proof_eq(a: &str, b: &str) -> bool {
  a.as_bytes().ct_eq(b.as_bytes()).into()
}

fn nonce() -> Result<String, ConnError> {
  let mut b = [0u8; 32];
  getrandom::fill(&mut b).map_err(|e| ConnError::Failed(e.to_string()))?;
  Ok(hex(&b))
}

fn read_json<S: Read>(s: &mut S) -> Result<Value, ConnError> {
  match read_frame(s)? {
    Some(Frame::Json(v)) => Ok(v),
    Some(Frame::Bin(_)) => Err(ConnError::Failed("unexpected binary frame".to_owned())),
    None => Err(ConnError::Failed("Agent runner closed the connection".to_owned())),
  }
}

fn remote_error(v: &Value) -> ConnError {
  let code = v.get("code").and_then(Value::as_str).unwrap_or("error");
  let message = super::sanitize::text(v.get("message").and_then(Value::as_str).unwrap_or(""), 200);
  if code == "proto_mismatch" {
    ConnError::Incompatible(format!("Agent runner speaks another protocol version ({message})"))
  } else {
    ConnError::Failed(format!("{code}: {message}"))
  }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Welcome {
  pub runner_version: String,
  pub caps: Vec<String>,
}

#[derive(Debug)]
pub struct Conn<S> {
  pub stream: S,
  next_id: u64,
}

impl<S: Read + Write> Conn<S> {
  pub fn handshake(mut stream: S, secret: &str, client: &str) -> Result<(Conn<S>, Welcome), ConnError> {
    let nonce_c = nonce()?;
    let hello = json!({
      "type": "hello",
      "proto": PROTO_VERSION,
      "client": client,
      "version": env!("CARGO_PKG_VERSION"),
      "pid": std::process::id(),
      "nonce_c": nonce_c,
    });
    write_json(&mut stream, &hello)?;
    let challenge = read_json(&mut stream)?;
    match challenge.get("type").and_then(Value::as_str) {
      Some("challenge") => {}
      Some("error") => return Err(remote_error(&challenge)),
      _ => return Err(ConnError::Failed("unexpected handshake message".to_owned())),
    }
    let nonce_s = challenge.get("nonce_s").and_then(Value::as_str).unwrap_or_default();
    let proof_s = challenge.get("proof_s").and_then(Value::as_str).unwrap_or_default();
    if nonce_s.is_empty() || !proof_eq(&proof(secret, SERVER, &nonce_c, nonce_s), proof_s) {
      return Err(ConnError::Failed("runner identity check failed (stale or foreign socket)".to_owned()));
    }
    let proof_c = proof(secret, CLIENT, nonce_s, &nonce_c);
    write_json(&mut stream, &json!({"type": "auth", "proof_c": proof_c}))?;
    let reply = read_json(&mut stream)?;
    match reply.get("type").and_then(Value::as_str) {
      Some("welcome") => {
        if reply.get("proto").and_then(Value::as_u64) != Some(PROTO_VERSION) {
          return Err(ConnError::Incompatible("Agent runner speaks another protocol version".to_owned()));
        }
        let welcome = Welcome {
          runner_version: super::sanitize::text(reply.get("runner_version").and_then(Value::as_str).unwrap_or(""), 40),
          caps: reply
            .get("caps")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(Value::as_str).map(|c| super::sanitize::text(c, 40)).collect())
            .unwrap_or_default(),
        };
        Ok((Conn { stream, next_id: 1 }, welcome))
      }
      Some("error") => Err(remote_error(&reply)),
      _ => Err(ConnError::Failed("unexpected handshake message".to_owned())),
    }
  }

  pub fn call(&mut self, method: &str, params: Value) -> Result<Value, ConnError> {
    let id = self.next_id;
    self.next_id += 1;
    write_json(&mut self.stream, &json!({"type": "request", "id": id, "method": method, "params": params}))?;
    loop {
      let Some(frame) = read_frame(&mut self.stream)? else {
        return Err(ConnError::Failed("Agent runner closed the connection".to_owned()));
      };
      let Frame::Json(v) = frame else { continue };
      match v.get("type").and_then(Value::as_str) {
        Some("response") if v.get("id").and_then(Value::as_u64) == Some(id) => {
          if let Some(err) = v.get("err").filter(|e| !e.is_null()) {
            let code = err.get("code").and_then(Value::as_str).unwrap_or("error");
            let message = super::sanitize::text(err.get("message").and_then(Value::as_str).unwrap_or(""), 200);
            return Err(ConnError::Failed(format!("{code}: {message}")));
          }
          return Ok(v.get("ok").cloned().unwrap_or(Value::Null));
        }
        Some("closed") => {
          let reason = super::sanitize::text(v.get("reason").and_then(Value::as_str).unwrap_or("closed"), 200);
          return Err(ConnError::Failed(reason));
        }
        _ => {}
      }
    }
  }
}

pub fn input_frame(session: &str, data: &[u8]) -> Frame {
  Frame::Bin(BinFrame { tag: TAG_INPUT, session: session.to_owned(), data: data.to_vec() })
}

#[cfg(all(test, unix))]
mod tests {
  use super::*;
  use crate::runner::mock::{self, Mock};
  use std::os::unix::net::UnixStream;
  use std::time::Duration;

  const SID: &str = "s_abcdefghij";

  fn pair() -> (UnixStream, UnixStream) {
    let (a, b) = UnixStream::pair().unwrap();
    a.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    a.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
    (a, b)
  }

  fn roundtrip(frame: Frame) -> Frame {
    let bytes = encode_frame(&frame).unwrap();
    read_frame(&mut &bytes[..]).unwrap().unwrap()
  }

  #[test]
  fn json_frame_round_trips_with_documented_layout() {
    let f = Frame::Json(json!({"a": 1}));
    let bytes = encode_frame(&f).unwrap();
    let payload = br#"{"a":1}"#;
    assert_eq!(&bytes[..4], &((payload.len() + 1) as u32).to_be_bytes());
    assert_eq!(bytes[4], 0);
    assert_eq!(&bytes[5..], payload);
    assert_eq!(roundtrip(f.clone()), f);
  }

  #[test]
  fn binary_frame_round_trips_with_documented_layout() {
    let f = input_frame(SID, b"ls\r");
    let bytes = encode_frame(&f).unwrap();
    assert_eq!(bytes[4], 1);
    assert_eq!(&bytes[5..8], &[3, SID.len() as u8, b's']);
    assert_eq!(roundtrip(f.clone()), f);
  }

  #[test]
  fn limits_are_enforced_on_encode_and_decode() {
    let big_json = Frame::Json(Value::String("x".repeat(MAX_JSON)));
    assert!(encode_frame(&big_json).is_err());
    let big_bin = Frame::Bin(BinFrame { tag: 1, session: SID.into(), data: vec![0; MAX_BIN] });
    assert!(encode_frame(&big_bin).is_err());
    let mut hdr = ((MAX_JSON + 2) as u32).to_be_bytes().to_vec();
    hdr.push(0);
    assert!(read_frame(&mut &hdr[..]).is_err());
    assert!(read_frame(&mut &0u32.to_be_bytes()[..]).is_err());
    assert!(read_frame(&mut &[0, 0, 0, 2, 9, 0][..]).is_err());
    assert!(read_frame(&mut &[][..]).unwrap().is_none());
  }

  #[test]
  fn bad_session_ids_are_rejected_in_binary_frames() {
    let f = Frame::Bin(BinFrame { tag: 3, session: "nope".into(), data: vec![] });
    assert!(encode_frame(&f).is_err());
    let mut raw = vec![0, 0, 0, 9, 1, 3, 4];
    raw.extend_from_slice(b"nope");
    raw.push(0);
    assert!(read_frame(&mut &raw[..]).is_err());
  }

  #[test]
  fn session_id_shape() {
    assert!(valid_session_id("s_abcdefghij"));
    assert!(valid_session_id("s_2345672345"));
    for bad in ["", "s_", "s_abcdefghi", "s_abcdefghijk", "S_abcdefghij", "s_ABCDEFGHIJ", "s_abcdefghi1", "x_abcdefghij"] {
      assert!(!valid_session_id(bad), "{bad}");
    }
  }

  #[test]
  fn proof_matches_the_runner_construction() {
    let expected = {
      let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(b"k").unwrap();
      mac.update(b"tmgr-runner-server\0ab");
      hex(&mac.finalize().into_bytes())
    };
    assert_eq!(proof("k", SERVER, "a", "b"), expected);
    assert_ne!(proof("k", SERVER, "a", "b"), proof("k", CLIENT, "a", "b"));
    assert_ne!(proof("k", SERVER, "a", "b"), proof("k", SERVER, "b", "a"));
  }

  #[test]
  fn handshake_succeeds_and_calls_work() {
    let (c, s) = pair();
    let server = mock::spawn(s, Mock::new("secret"));
    let (mut conn, welcome) = Conn::handshake(c, "secret", "desktop").unwrap();
    assert_eq!(welcome.runner_version, "0.1.0");
    assert_eq!(conn.call("runner.status", Value::Null).unwrap()["proto"], 1);
    let err = conn.call("nope.method", Value::Null).unwrap_err();
    assert!(err.message().contains("unknown_method"));
    drop(conn);
    let log = server.join().unwrap();
    assert_eq!(log.hello["client"], "desktop");
    assert_eq!(log.requests.len(), 2);
  }

  #[test]
  fn server_with_a_different_secret_is_rejected() {
    let (c, s) = pair();
    let server = mock::spawn(s, Mock::new("other"));
    let err = Conn::handshake(c, "secret", "desktop").unwrap_err();
    assert!(matches!(err, ConnError::Failed(ref m) if m.contains("identity")), "{err:?}");
    let _ = server.join();
  }

  #[test]
  fn forged_server_proof_is_rejected() {
    let (c, s) = pair();
    let mut m = Mock::new("secret");
    m.bad_server_proof = true;
    let server = mock::spawn(s, m);
    assert!(matches!(Conn::handshake(c, "secret", "desktop"), Err(ConnError::Failed(_))));
    let _ = server.join();
  }

  #[test]
  fn proto_mismatch_error_is_incompatible() {
    let (c, s) = pair();
    let mut m = Mock::new("secret");
    m.reject_proto = true;
    let server = mock::spawn(s, m);
    assert!(matches!(Conn::handshake(c, "secret", "desktop"), Err(ConnError::Incompatible(_))));
    let _ = server.join();
  }

  #[test]
  fn welcome_with_other_proto_is_incompatible() {
    let (c, s) = pair();
    let mut m = Mock::new("secret");
    m.welcome_proto = 2;
    let server = mock::spawn(s, m);
    assert!(matches!(Conn::handshake(c, "secret", "desktop"), Err(ConnError::Incompatible(_))));
    let _ = server.join();
  }

  #[test]
  fn silent_server_times_out() {
    let (c, _s) = pair();
    c.set_read_timeout(Some(Duration::from_millis(100))).unwrap();
    let err = Conn::handshake(c, "secret", "desktop").unwrap_err();
    assert!(matches!(err, ConnError::Failed(ref m) if m.contains("in time")), "{err:?}");
  }
}
