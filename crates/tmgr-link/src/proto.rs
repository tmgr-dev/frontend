use serde_json::{json, Value};
use std::io::{Read, Write};

pub const MAX_FRAME: usize = 1 << 20;
pub const PROTOCOL: u32 = 1;
pub const METHODS: [&str; 5] = ["hello", "runs.list", "runs.get", "runs.create", "runs.halt"];

#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    Oversize(u64),
    Truncated,
    Empty,
    Timeout,
    Io(String),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::Oversize(n) => write!(f, "frame of {n} bytes exceeds the limit"),
            FrameError::Truncated => f.write_str("the stream ended inside a frame"),
            FrameError::Empty => f.write_str("empty frame"),
            FrameError::Timeout => f.write_str("timed out"),
            FrameError::Io(e) => write!(f, "I/O error: {e}"),
        }
    }
}

fn io_err(e: std::io::Error) -> FrameError {
    FrameError::Io(format!("{:?}", e.kind()))
}

pub trait Channel {
    fn recv(&mut self) -> Result<Option<Vec<u8>>, FrameError>;
    fn send(&mut self, payload: &[u8]) -> Result<(), FrameError>;
}

pub struct Framed<R, W> {
    reader: R,
    writer: W,
    max: usize,
}

impl<R: Read, W: Write> Framed<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        Self::with_max(reader, writer, MAX_FRAME)
    }

    pub fn with_max(reader: R, writer: W, max: usize) -> Self {
        Self {
            reader,
            writer,
            max,
        }
    }
}

impl<R: Read, W: Write> Channel for Framed<R, W> {
    fn recv(&mut self) -> Result<Option<Vec<u8>>, FrameError> {
        read_frame(&mut self.reader, self.max)
    }

    fn send(&mut self, payload: &[u8]) -> Result<(), FrameError> {
        write_frame(&mut self.writer, payload, self.max)
    }
}

type Inbox = std::sync::mpsc::Receiver<Result<Option<Vec<u8>>, FrameError>>;
type Outbox = std::sync::mpsc::SyncSender<Vec<u8>>;

pub struct Timed {
    inbox: Inbox,
    outbox: Outbox,
    acks: std::sync::mpsc::Receiver<Result<(), FrameError>>,
    idle: std::time::Duration,
    deadline: std::time::Instant,
}

impl Timed {
    pub fn spawn<R, W>(
        mut reader: R,
        mut writer: W,
        idle: std::time::Duration,
        session: std::time::Duration,
        max: usize,
    ) -> Timed
    where
        R: Read + Send + 'static,
        W: Write + Send + 'static,
    {
        let (tx, inbox) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || loop {
            let frame = read_frame(&mut reader, max);
            let last = !matches!(frame, Ok(Some(_)));
            if tx.send(frame).is_err() || last {
                break;
            }
        });
        let (outbox, out_rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(1);
        let (ack_tx, acks) = std::sync::mpsc::sync_channel(1);
        std::thread::spawn(move || {
            while let Ok(payload) = out_rx.recv() {
                if ack_tx
                    .send(write_frame(&mut writer, &payload, max))
                    .is_err()
                {
                    break;
                }
            }
        });
        Timed {
            inbox,
            outbox,
            acks,
            idle,
            deadline: std::time::Instant::now() + session,
        }
    }

    fn remaining(&self) -> Option<std::time::Duration> {
        self.deadline
            .checked_duration_since(std::time::Instant::now())
            .filter(|d| !d.is_zero())
    }
}

impl Channel for Timed {
    fn recv(&mut self) -> Result<Option<Vec<u8>>, FrameError> {
        let wait = self.remaining().ok_or(FrameError::Timeout)?.min(self.idle);
        match self.inbox.recv_timeout(wait) {
            Ok(frame) => frame,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(FrameError::Timeout),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Ok(None),
        }
    }

    fn send(&mut self, payload: &[u8]) -> Result<(), FrameError> {
        let wait = self.remaining().ok_or(FrameError::Timeout)?.min(self.idle);
        self.outbox
            .send(payload.to_vec())
            .map_err(|_| FrameError::Io("closed".into()))?;
        match self.acks.recv_timeout(wait) {
            Ok(r) => r,
            Err(_) => Err(FrameError::Timeout),
        }
    }
}

pub fn read_frame<R: Read>(r: &mut R, max: usize) -> Result<Option<Vec<u8>>, FrameError> {
    let mut header = [0u8; 4];
    let mut got = 0;
    while got < 4 {
        match r.read(&mut header[got..]) {
            Ok(0) if got == 0 => return Ok(None),
            Ok(0) => return Err(FrameError::Truncated),
            Ok(n) => got += n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(io_err(e)),
        }
    }
    let len = u32::from_be_bytes(header) as u64;
    if len == 0 {
        return Err(FrameError::Empty);
    }
    if len > max as u64 {
        return Err(FrameError::Oversize(len));
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body).map_err(|e| {
        if e.kind() == std::io::ErrorKind::UnexpectedEof {
            FrameError::Truncated
        } else {
            io_err(e)
        }
    })?;
    Ok(Some(body))
}

pub fn write_frame<W: Write>(w: &mut W, payload: &[u8], max: usize) -> Result<(), FrameError> {
    if payload.is_empty() {
        return Err(FrameError::Empty);
    }
    if payload.len() > max {
        return Err(FrameError::Oversize(payload.len() as u64));
    }
    let mut buf = Vec::with_capacity(payload.len() + 4);
    buf.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    buf.extend_from_slice(payload);
    w.write_all(&buf).map_err(io_err)?;
    w.flush().map_err(io_err)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Request {
    pub id: u64,
    pub method: String,
    pub params: Value,
}

pub fn parse_request(bytes: &[u8]) -> Result<Request, &'static str> {
    let v: Value = serde_json::from_slice(bytes).map_err(|_| "not valid JSON")?;
    let obj = v.as_object().ok_or("a request must be a JSON object")?;
    let id = obj
        .get("id")
        .and_then(Value::as_u64)
        .ok_or("id must be an unsigned integer")?;
    let method = obj
        .get("method")
        .and_then(Value::as_str)
        .ok_or("method must be a string")?;
    if method.len() > 64 {
        return Err("method name is too long");
    }
    let params = obj.get("params").cloned().unwrap_or(Value::Null);
    if obj
        .keys()
        .any(|k| !matches!(k.as_str(), "id" | "method" | "params"))
    {
        return Err("unknown request field");
    }
    Ok(Request {
        id,
        method: method.to_string(),
        params,
    })
}

pub fn request(id: u64, method: &str, params: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"id": id, "method": method, "params": params}))
        .expect("request serializes")
}

pub fn ok(id: Option<u64>, result: Value) -> Vec<u8> {
    serde_json::to_vec(&json!({"id": id, "result": result})).expect("response serializes")
}

pub fn error(id: Option<u64>, code: &str, message: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"id": id, "error": code, "message": message}))
        .expect("response serializes")
}

#[derive(Debug, PartialEq)]
pub enum Reply {
    Ok(Value),
    Err { code: String, message: String },
}

pub fn parse_reply(bytes: &[u8], expect_id: u64) -> Result<Reply, &'static str> {
    let v: Value = serde_json::from_slice(bytes).map_err(|_| "the reply is not valid JSON")?;
    let obj = v.as_object().ok_or("the reply is not an object")?;
    match obj.get("id") {
        Some(Value::Number(n)) if n.as_u64() == Some(expect_id) => {}
        Some(Value::Null) | None if obj.contains_key("error") => {}
        _ => return Err("the reply id does not match the request"),
    }
    if let Some(code) = obj.get("error").and_then(Value::as_str) {
        let message = obj.get("message").and_then(Value::as_str).unwrap_or("");
        return Ok(Reply::Err {
            code: tmgr_core::text::sanitize_line(code),
            message: tmgr_core::text::sanitize_line(message),
        });
    }
    obj.get("result")
        .cloned()
        .map(Reply::Ok)
        .ok_or("the reply has neither result nor error")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::time::Duration;

    fn framed(payload: &[u8]) -> Vec<u8> {
        let mut v = (payload.len() as u32).to_be_bytes().to_vec();
        v.extend_from_slice(payload);
        v
    }

    #[test]
    fn frames_round_trip() {
        let mut buf = Vec::new();
        write_frame(&mut buf, b"{\"a\":1}", MAX_FRAME).unwrap();
        assert_eq!(&buf[..4], &[0, 0, 0, 7]);
        let got = read_frame(&mut Cursor::new(buf), MAX_FRAME).unwrap();
        assert_eq!(got.unwrap(), b"{\"a\":1}");
    }

    #[test]
    fn clean_eof_is_none_and_partial_header_is_truncated() {
        assert_eq!(read_frame(&mut Cursor::new(vec![]), 10).unwrap(), None);
        assert_eq!(
            read_frame(&mut Cursor::new(vec![0, 0]), 10),
            Err(FrameError::Truncated)
        );
        assert_eq!(
            read_frame(&mut Cursor::new(framed(b"abcd")[..6].to_vec()), 10),
            Err(FrameError::Truncated)
        );
    }

    #[test]
    fn oversize_is_rejected_before_allocating() {
        let header = u32::MAX.to_be_bytes().to_vec();
        assert_eq!(
            read_frame(&mut Cursor::new(header), MAX_FRAME),
            Err(FrameError::Oversize(u32::MAX as u64))
        );
        let mut sink = Vec::new();
        assert_eq!(
            write_frame(&mut sink, &[b'x'; 11], 10),
            Err(FrameError::Oversize(11))
        );
        assert!(sink.is_empty());
    }

    #[test]
    fn a_silent_peer_times_out() {
        let (r, _w) = std::io::pipe().unwrap();
        let (_r2, w2) = std::io::pipe().unwrap();
        let mut t = Timed::spawn(
            r,
            w2,
            Duration::from_millis(100),
            Duration::from_secs(5),
            MAX_FRAME,
        );
        let started = std::time::Instant::now();
        assert_eq!(t.recv(), Err(FrameError::Timeout));
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn a_half_sent_frame_times_out_and_whole_frames_pass() {
        let (r, mut w) = std::io::pipe().unwrap();
        let (_r2, w2) = std::io::pipe().unwrap();
        let mut t = Timed::spawn(
            r,
            w2,
            Duration::from_millis(150),
            Duration::from_secs(5),
            MAX_FRAME,
        );
        w.write_all(&framed(b"{}")).unwrap();
        assert_eq!(t.recv().unwrap().unwrap(), b"{}");
        w.write_all(&[0, 0, 0, 9, b'x']).unwrap();
        assert_eq!(t.recv(), Err(FrameError::Timeout));
    }

    #[test]
    fn the_session_deadline_ends_an_active_connection() {
        let (r, mut w) = std::io::pipe().unwrap();
        let (_r2, w2) = std::io::pipe().unwrap();
        let mut t = Timed::spawn(
            r,
            w2,
            Duration::from_secs(5),
            Duration::from_millis(300),
            MAX_FRAME,
        );
        let feeder = std::thread::spawn(move || {
            for _ in 0..20 {
                if w.write_all(&framed(b"{}")).is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        let mut got = 0;
        loop {
            match t.recv() {
                Ok(Some(_)) => got += 1,
                Err(FrameError::Timeout) => break,
                other => panic!("{other:?}"),
            }
        }
        assert!((2..20).contains(&got));
        drop(t);
        let _ = feeder.join();
    }

    #[test]
    fn a_writer_that_never_drains_times_out() {
        let (r, _w) = std::io::pipe().unwrap();
        let (_r2, w2) = std::io::pipe().unwrap();
        let mut t = Timed::spawn(
            r,
            w2,
            Duration::from_millis(200),
            Duration::from_secs(5),
            MAX_FRAME,
        );
        let big = vec![b'x'; 512 * 1024];
        let mut result = Ok(());
        for _ in 0..4 {
            result = t.send(&big);
            if result.is_err() {
                break;
            }
        }
        assert_eq!(result, Err(FrameError::Timeout));
    }

    #[test]
    fn empty_frames_are_rejected() {
        assert_eq!(
            read_frame(&mut Cursor::new(vec![0, 0, 0, 0]), 10),
            Err(FrameError::Empty)
        );
    }

    #[test]
    fn requests_are_strict() {
        let r = parse_request(br#"{"id":3,"method":"hello","params":{"protocol":1}}"#).unwrap();
        assert_eq!((r.id, r.method.as_str()), (3, "hello"));
        for bad in [
            &b"nope"[..],
            b"[]",
            br#"{"method":"hello"}"#,
            br#"{"id":-1,"method":"hello"}"#,
            br#"{"id":1,"method":5}"#,
            br#"{"id":1,"method":"hello","extra":1}"#,
        ] {
            assert!(parse_request(bad).is_err());
        }
    }

    #[test]
    fn replies_parse_and_strip_control_characters() {
        let e = error(Some(2), "not_available", "runner not installed\u{1b}[2J");
        match parse_reply(&e, 2).unwrap() {
            Reply::Err { code, message } => {
                assert_eq!(code, "not_available");
                assert!(!message.contains('\u{1b}'));
            }
            other => panic!("{other:?}"),
        }
        assert!(parse_reply(&ok(Some(9), json!({})), 2).is_err());
        assert_eq!(
            parse_reply(&ok(Some(2), json!({"x":1})), 2).unwrap(),
            Reply::Ok(json!({"x":1}))
        );
    }
}
