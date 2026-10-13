use std::io::{self, Read};
use std::time::{Duration, Instant};

use serde_json::Value;
use tmgr_ipc::api::{caps, limits, ClientMsg, ErrorCode, Request, Response, ServerMsg, PROTO_VERSION};

use super::dir::Sock;
use super::sanitize;

#[cfg(unix)]
pub use tmgr_ipc::frame::{read_frame, write_frame, write_json, BinFrame, Frame, TAG_INPUT};
#[cfg(not(unix))]
pub use self::no_frames::*;

pub use limits::MAX_INPUT;

pub const CALL_DEADLINE: Duration = Duration::from_secs(10);
pub const IO_TIMEOUT: Duration = Duration::from_secs(5);
pub const MAX_UNRELATED_FRAMES: usize = 256;
pub const UPDATE_RUNNER: &str = "Update the TMGR runner";

#[cfg(not(unix))]
mod no_frames {
  use std::io::{self, Read, Write};

  pub const TAG_INPUT: u8 = 3;

  #[derive(Debug, Clone, PartialEq)]
  pub struct BinFrame {
    pub tag: u8,
    pub session: String,
    pub data: Vec<u8>,
  }

  #[derive(Debug, Clone, PartialEq)]
  pub enum Frame {
    Json(serde_json::Value),
    Bin(BinFrame),
  }

  fn unsupported() -> io::Error {
    io::ErrorKind::Unsupported.into()
  }

  pub fn read_frame<R: Read>(_: &mut R) -> io::Result<Option<Frame>> {
    Err(unsupported())
  }

  pub fn write_frame<W: Write>(_: &mut W, _: &Frame) -> io::Result<()> {
    Err(unsupported())
  }

  pub fn write_json<W: Write, T: serde::Serialize>(_: &mut W, _: &T) -> io::Result<()> {
    Err(unsupported())
  }
}

struct Deadline<'a> {
  stream: &'a mut Sock,
  end: Instant,
}

impl Read for Deadline<'_> {
  fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
    let left = self.end.saturating_duration_since(Instant::now());
    if left.is_zero() {
      return Err(io::ErrorKind::TimedOut.into());
    }
    let _ = self.stream.set_read_timeout(Some(left)); // macOS rejects setsockopt once the peer has closed
    self.stream.read(buf)
  }
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

pub use tmgr_ipc::proto::valid_session_id;

#[derive(Debug, Clone, PartialEq)]
pub struct Welcome {
  pub runner_version: String,
  pub caps: Vec<String>,
}

impl Welcome {
  pub fn can_create_runs(&self) -> bool {
    self.caps.iter().any(|c| c == caps::RUNS_CREATE_V1)
  }
}

#[derive(Debug)]
pub struct Conn {
  pub stream: Sock,
  next_id: u64,
  call_deadline: Duration,
}

#[cfg(unix)]
fn handshake_error(e: io::Error) -> ConnError {
  let text = e.to_string();
  if let Some(rest) = text.strip_prefix(ErrorCode::ProtoMismatch.as_str()) {
    let message = sanitize::text(rest.trim_start_matches(':'), 200);
    return ConnError::Incompatible(format!("Agent runner speaks another protocol version ({message})"));
  }
  match e.kind() {
    io::ErrorKind::InvalidData | io::ErrorKind::PermissionDenied => ConnError::Failed(sanitize::text(&text, 200)),
    _ => e.into(),
  }
}

impl Conn {
  #[cfg(unix)]
  pub fn handshake(stream: Sock, secret: &str, client: &str) -> Result<(Conn, Welcome), ConnError> {
    let w = tmgr_ipc::server::handshake_client(&stream, client, secret).map_err(handshake_error)?;
    if w.proto != PROTO_VERSION {
      return Err(ConnError::Incompatible("Agent runner speaks another protocol version".to_owned()));
    }
    let welcome = Welcome {
      runner_version: sanitize::text(&w.runner_version, 40),
      caps: w.caps.iter().map(|c| sanitize::text(c, 40)).collect(),
    };
    stream.set_read_timeout(Some(IO_TIMEOUT))?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    Ok((Conn { stream, next_id: 1, call_deadline: CALL_DEADLINE }, welcome))
  }

  #[cfg(not(unix))]
  pub fn handshake(_stream: Sock, _secret: &str, _client: &str) -> Result<(Conn, Welcome), ConnError> {
    let _ = PROTO_VERSION;
    Err(ConnError::Absent("Agent runner is not available on this platform".to_owned()))
  }

  pub fn call(&mut self, method: &str, params: Value) -> Result<Value, ConnError> {
    let id = self.next_id;
    self.next_id += 1;
    let end = Instant::now() + self.call_deadline;
    write_json(&mut self.stream, &ClientMsg::Request(Request { id, method: method.to_owned(), params }))?;
    let mut unrelated = 0;
    loop {
      let Some(frame) = read_frame(&mut Deadline { stream: &mut self.stream, end })? else {
        return Err(ConnError::Failed("Agent runner closed the connection".to_owned()));
      };
      if let Frame::Json(v) = frame {
        match serde_json::from_value::<ServerMsg>(v) {
          Ok(ServerMsg::Response(r)) if r.id == id => return response_value(r),
          Ok(ServerMsg::Closed { reason }) => return Err(ConnError::Failed(sanitize::text(&reason, 200))),
          _ => {}
        }
      }
      unrelated += 1;
      if unrelated > MAX_UNRELATED_FRAMES {
        return Err(ConnError::Failed("Agent runner sent too many unrelated messages".to_owned()));
      }
    }
  }
}

fn response_value(r: Response) -> Result<Value, ConnError> {
  match r.err {
    Some(e) if e.code == ErrorCode::Busy => Err(ConnError::Failed("busy".to_owned())),
    Some(e) => Err(ConnError::Failed(format!("{}: {}", e.code, sanitize::text(&e.message, 200)))),
    None => Ok(r.ok.unwrap_or(Value::Null)),
  }
}

pub fn input_frame(session: &str, data: &[u8]) -> Frame {
  Frame::Bin(BinFrame { tag: TAG_INPUT, session: session.to_owned(), data: data.to_vec() })
}

#[cfg(all(test, unix))]
mod tests {
  use super::*;
  use crate::runner::mock::{self, Mock};
  use serde_json::json;
  use std::io::Write;
  use std::os::unix::net::UnixStream;

  const SID: &str = "s_abcdefghij";

  fn pair() -> (UnixStream, UnixStream) {
    let (a, b) = UnixStream::pair().unwrap();
    a.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    a.set_write_timeout(Some(Duration::from_secs(5))).unwrap();
    (a, b)
  }

  #[test]
  fn handshake_succeeds_and_calls_work() {
    let (c, s) = pair();
    let server = mock::spawn(s, Mock::new("secret"));
    let (mut conn, welcome) = Conn::handshake(c, "secret", "desktop").unwrap();
    assert_eq!(welcome.runner_version, "0.1.0");
    assert!(welcome.can_create_runs());
    assert_eq!(conn.call("runner.status", Value::Null).unwrap()["proto"], 1);
    let err = conn.call("nope.method", Value::Null).unwrap_err();
    assert!(err.message().contains("unknown_method"));
    drop(conn);
    let log = server.join().unwrap();
    assert_eq!(log.hello["client"], "desktop");
    assert_eq!(log.requests.len(), 2);
  }

  #[test]
  fn handshake_leaves_io_timeouts_on_the_stream() {
    let (c, s) = pair();
    let server = mock::spawn(s, Mock::new("secret"));
    let (conn, _) = Conn::handshake(c, "secret", "desktop").unwrap();
    assert_eq!(conn.stream.read_timeout().unwrap(), Some(IO_TIMEOUT));
    assert_eq!(conn.stream.write_timeout().unwrap(), Some(IO_TIMEOUT));
    drop(conn);
    server.join().unwrap();
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
  fn welcome_text_is_sanitized() {
    let (c, s) = pair();
    let mut m = Mock::new("secret");
    m.caps = Some(vec!["runs.create.v1\u{1b}[2J".into(), "x".repeat(100)]);
    let server = mock::spawn(s, m);
    let (_, welcome) = Conn::handshake(c, "secret", "desktop").unwrap();
    assert!(welcome.caps.iter().all(|c| !c.contains('\u{1b}') && c.chars().count() <= 40), "{:?}", welcome.caps);
    drop(server);
  }

  #[test]
  fn call_stops_after_the_unrelated_frame_budget() {
    let (c, s) = pair();
    let server = mock::spawn_scripted(s, "secret", |s| {
      let _ = read_frame(s);
      for _ in 0..1000 {
        if write_json(s, &json!({"type": "event", "session": SID})).is_err() {
          break;
        }
      }
    });
    let (mut conn, _) = Conn::handshake(c, "secret", "desktop").unwrap();
    let err = conn.call("runner.status", Value::Null).unwrap_err();
    assert!(matches!(err, ConnError::Failed(ref m) if m.contains("too many")), "{err:?}");
    drop(conn);
    server.join().unwrap();
  }

  #[test]
  fn call_deadline_is_absolute() {
    let (c, s) = pair();
    let server = mock::spawn_scripted(s, "secret", |s| {
      let _ = read_frame(s);
      for _ in 0..40 {
        if write_json(s, &json!({"type": "event"})).is_err() {
          break;
        }
        std::thread::sleep(Duration::from_millis(50));
      }
    });
    let (mut conn, _) = Conn::handshake(c, "secret", "desktop").unwrap();
    conn.call_deadline = Duration::from_millis(300);
    let started = Instant::now();
    let err = conn.call("runner.status", Value::Null).unwrap_err();
    assert!(started.elapsed() < Duration::from_millis(900), "{:?}", started.elapsed());
    assert!(matches!(err, ConnError::Failed(ref m) if m.contains("in time")), "{err:?}");
    drop(conn);
    server.join().unwrap();
  }

  #[test]
  fn unknown_pushes_and_extra_fields_are_ignored_while_waiting_for_the_response() {
    let (c, s) = pair();
    let server = mock::spawn_scripted(s, "secret", |s| {
      let Ok(Some(Frame::Json(req))) = read_frame(s) else { return };
      let _ = write_json(s, &json!({"type": "future_push", "x": [1, 2]}));
      let _ = write_json(s, &json!({"type": "response", "id": 999, "ok": "other"}));
      let _ = write_json(s, &json!({"type": "response", "id": req["id"], "ok": {"a": 1}, "extra": true}));
      let _ = s.flush();
    });
    let (mut conn, _) = Conn::handshake(c, "secret", "desktop").unwrap();
    assert_eq!(conn.call("runner.status", Value::Null).unwrap(), json!({"a": 1}));
    drop(conn);
    server.join().unwrap();
  }

  #[test]
  fn remote_error_text_is_sanitized_and_future_codes_are_kept() {
    let (c, s) = pair();
    let server = mock::spawn_scripted(s, "secret", |s| {
      let Ok(Some(Frame::Json(req))) = read_frame(s) else { return };
      let _ = write_json(s, &json!({"type": "response", "id": req["id"], "err": {"code": "rate_limited", "message": "slow\u{1b}[2J down"}}));
    });
    let (mut conn, _) = Conn::handshake(c, "secret", "desktop").unwrap();
    assert_eq!(conn.call("runner.status", Value::Null).unwrap_err().message(), "rate_limited: slow[2J down");
    drop(conn);
    server.join().unwrap();
  }

  #[test]
  fn input_frame_is_a_tag_3_binary_frame() {
    let Frame::Bin(b) = input_frame(SID, b"ls\r") else { panic!("not binary") };
    assert_eq!((b.tag, b.session.as_str(), b.data.as_slice()), (3, SID, &b"ls\r"[..]));
  }
}
