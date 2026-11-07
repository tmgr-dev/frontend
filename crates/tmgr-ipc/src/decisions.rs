use crate::dirs::{check_private_dir, ensure_private_dir};
use crate::endpoint::{self, Endpoint};
use crate::secret;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::Shutdown;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

pub const PROTO: u32 = 1;
pub const SOCK: &str = "decisions.sock";
pub const TOKEN: &str = "decisions.token";
pub const DB: &str = "decisions.db";
pub const INBOX: &str = "inbox";
pub const MAX_LINE: usize = 4 << 20;
const DRAIN_MAX: usize = 64 << 20;
pub const MAX_DIFFS: usize = 8;
pub const MAX_PATCHES: usize = 8;
pub const MAX_PATCH_BYTES: usize = 1 << 20;
pub const MAX_LINKS: usize = 50;
pub const OUTCOME_CREATED: &str = "created";
pub const OUTCOME_UPDATED: &str = "updated";
pub const OUTCOME_FINAL: &str = "final";
pub const OUTCOME_SIMILAR: &str = "similar";
pub const UNAUTHORIZED: &str = "unauthorized";
pub const BAD_REQUEST: &str = "bad request";
pub const MAX_WAIT_MS: u64 = 3_600_000;
pub const MAX_WAIT_CARDS: usize = 64;
pub const BUSY_LINE: &str = r#"{"busy":true,"error":"busy: too many connections"}"#;
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
const IO_GRACE: Duration = Duration::from_secs(10);
const UNAUTHORIZED_ATTEMPTS: u32 = 5;
const UNAUTHORIZED_PAUSE: Duration = Duration::from_millis(200);

pub fn decisions_dir() -> io::Result<PathBuf> {
    if let Some(p) = std::env::var_os("TMGR_DECISIONS_DIR").filter(|v| !v.is_empty()) {
        return Ok(PathBuf::from(p));
    }
    let home = std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "HOME is not set"))?;
    #[cfg(target_os = "macos")]
    {
        Ok(home.join("Library/Application Support/dev.tmgr.desktop/decisions"))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let data = std::env::var_os("XDG_DATA_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"));
        Ok(data.join("dev.tmgr.desktop/decisions"))
    }
}

pub fn new_card_id() -> String {
    static LAST_MS: Mutex<u64> = Mutex::new(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let ms = {
        let mut last = LAST_MS.lock().unwrap_or_else(|p| p.into_inner());
        *last = now.max(*last + 1);
        *last & 0xffff_ffff_ffff
    };
    let mut r = [0u8; 10];
    getrandom::fill(&mut r).expect("system random source");
    format!("c_{ms:012x}{}", secret::hex(&r))
}

pub fn valid_card_id(s: &str) -> bool {
    s.strip_prefix("c_")
        .is_some_and(|r| r.len() == 32 && r.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')))
}

pub fn has_foreign_scheme(s: &str) -> bool {
    let s: String = s
        .trim_start_matches(|c: char| c <= ' ')
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let Some((scheme, rest)) = s.split_once(':') else {
        return false;
    };
    let mut chars = scheme.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !valid {
        return false;
    }
    let scheme = scheme.to_ascii_lowercase();
    if matches!(scheme.as_str(), "http" | "https") {
        return false;
    }
    let dangerous = matches!(
        scheme.as_str(),
        "javascript" | "data" | "vbscript" | "file" | "blob"
    );
    let known = matches!(
        scheme.as_str(),
        "ftp"
            | "mailto"
            | "tel"
            | "sms"
            | "vscode"
            | "tauri"
            | "filesystem"
            | "ms-msdt"
            | "ssh"
            | "git"
            | "ws"
            | "wss"
            | "intent"
            | "itms"
    );
    dangerous || known || rest.starts_with("//")
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Source {
    #[serde(default)]
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runner_session: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fact {
    pub state: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Opt {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub recommended: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DiffSpec {
    pub repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Check {
    pub state: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Links {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<LinkItem>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LinkItem {
    pub kind: String,
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchIn {
    #[serde(default)]
    pub label: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CardIn {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default)]
    pub source: Source,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub blocking: bool,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub facts: Vec<Fact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<Opt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<DiffSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub diffs: Vec<DiffSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patches: Vec<PatchIn>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub checks: Vec<Check>,
    #[serde(default)]
    pub links: Links,
    #[serde(default)]
    pub reply_via: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub supersede: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force_new: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Found {
    pub card_id: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question: Option<String>,
    pub state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dismissed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<String>,
    pub created_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineComment {
    pub file: String,
    pub line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadRef {
    pub file: String,
    pub line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub side: Option<String>,
}

/// What the agent receives. `type` is `answer` (final unless `deferred`) or `message` (interim).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub card_id: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub seq: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub choice_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_text: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub line_comments: Vec<LineComment>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub deferred: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub dismissed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread: Option<ThreadRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
}

impl Event {
    pub fn is_final(&self) -> bool {
        self.kind == "answer" && !self.deferred
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("<redacted>")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Inbox {
    pub kind: String,
    pub endpoint: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<Secret>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    Ping,
    Create {
        card: Box<CardIn>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        inbox: Option<Inbox>,
    },
    /// The final answer at once when the card has one; else the first event with `seq > since`
    /// (with `final_only`, only a final answer), waiting up to `timeout_ms`.
    Wait {
        card_id: String,
        #[serde(default)]
        since: u64,
        timeout_ms: u64,
        #[serde(default)]
        final_only: bool,
    },
    Reply {
        card_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        line_replies: Vec<LineComment>,
    },
    CloseSession {
        agent: String,
        session: String,
    },
    /// Like `Wait`, over several cards at once: the first card with an event wins.
    WaitAny {
        cards: Vec<WaitCard>,
        timeout_ms: u64,
        #[serde(default)]
        final_only: bool,
    },
    Find {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        task: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        query: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u32>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WaitCard {
    pub card_id: String,
    #[serde(default)]
    pub since: u64,
}

pub fn write_busy<W: Write>(w: &mut W) -> io::Result<()> {
    w.write_all(format!("{BUSY_LINE}\n").as_bytes())?;
    w.flush()
}

/// `BUSY_LINE` has no `ok`, so clients older than it fail to parse it and treat it like a closed connection.
pub fn is_busy(line: &[u8]) -> bool {
    #[derive(Deserialize)]
    struct Busy {
        busy: bool,
    }
    serde_json::from_slice::<Busy>(line).is_ok_and(|b| b.busy)
}

#[derive(Default)]
struct CancelState {
    cancelled: bool,
    stream: Option<UnixStream>,
}

/// Ends a blocked `Client` call from another thread: the call returns `ErrorKind::Interrupted`.
#[derive(Default)]
pub struct Cancel {
    state: Mutex<CancelState>,
    cv: Condvar,
}

impl Cancel {
    fn lock(&self) -> std::sync::MutexGuard<'_, CancelState> {
        self.state.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn cancel(&self) {
        let mut s = self.lock();
        s.cancelled = true;
        if let Some(stream) = s.stream.take() {
            let _ = stream.shutdown(Shutdown::Both);
        }
        self.cv.notify_all();
    }

    pub fn is_cancelled(&self) -> bool {
        self.lock().cancelled
    }

    /// Sleeps up to `d`; true when cancelled.
    pub fn sleep(&self, d: Duration) -> bool {
        let s = self.lock();
        let (s, _) = self
            .cv
            .wait_timeout_while(s, d, |s| !s.cancelled)
            .unwrap_or_else(|p| p.into_inner());
        s.cancelled
    }

    fn attach(&self, stream: &UnixStream) -> io::Result<()> {
        let mut s = self.lock();
        if s.cancelled {
            return Err(cancelled());
        }
        s.stream = Some(stream.try_clone()?);
        Ok(())
    }

    fn detach(&self) -> bool {
        let mut s = self.lock();
        s.stream = None;
        s.cancelled
    }
}

fn busy() -> io::Error {
    io::Error::new(io::ErrorKind::ResourceBusy, "the decisions inbox is busy")
}

fn cancelled() -> io::Error {
    io::Error::new(io::ErrorKind::Interrupted, "cancelled")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub v: u32,
    pub token: String,
    #[serde(flatten)]
    pub op: Op,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<Event>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub found: Vec<Found>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub similar: Vec<Found>,
}

impl Response {
    pub fn ok() -> Self {
        Response {
            ok: true,
            ..Default::default()
        }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Response {
            ok: false,
            error: Some(msg.into()),
            ..Default::default()
        }
    }
}

pub fn socket_endpoint(dir: &Path) -> Endpoint {
    Endpoint::Path(dir.join(SOCK))
}

/// Server side: create the private directory and a fresh token for this launch.
pub fn prepare_server_dir(dir: &Path) -> io::Result<String> {
    ensure_private_dir(dir)?;
    let path = dir.join(TOKEN);
    let staged = dir.join(format!("{TOKEN}.new"));
    match fs::remove_file(&staged) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    let token = secret::write_new(&staged)?;
    fs::rename(&staged, &path)?;
    Ok(token)
}

pub fn token_ok(expected: &str, given: &str) -> bool {
    secret::eq(expected, given)
}

pub fn read_line_limited<R: Read>(r: R) -> io::Result<Option<Vec<u8>>> {
    let mut reader = BufReader::new(r.take(MAX_LINE as u64 + 1));
    let mut buf = Vec::new();
    reader.read_until(b'\n', &mut buf)?;
    if buf.is_empty() {
        return Ok(None);
    }
    if buf.len() > MAX_LINE {
        if buf.last() != Some(&b'\n') {
            drain_line(reader.into_inner().into_inner());
        }
        return Err(io::Error::new(
            io::ErrorKind::FileTooLarge,
            format!("request too large (limit {MAX_LINE} bytes)"),
        ));
    }
    if buf.last() != Some(&b'\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "request line is truncated",
        ));
    }
    buf.pop();
    Ok(Some(buf))
}

fn drain_line<R: Read>(mut r: R) {
    let mut left = DRAIN_MAX;
    let mut chunk = [0u8; 64 * 1024];
    while left > 0 {
        match r.read(&mut chunk) {
            Ok(n) if n > 0 => {
                if chunk[..n].contains(&b'\n') {
                    return;
                }
                left = left.saturating_sub(n);
            }
            _ => return,
        }
    }
}

pub fn write_line<W: Write, T: Serialize>(w: &mut W, value: &T) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    if bytes.len() >= MAX_LINE {
        return Err(io::Error::other(format!(
            "request too large (limit {MAX_LINE} bytes)"
        )));
    }
    bytes.push(b'\n');
    w.write_all(&bytes)?;
    w.flush()
}

pub struct Client {
    dir: PathBuf,
    token: Mutex<String>,
}

impl Client {
    pub fn open() -> io::Result<Client> {
        Client::open_in(&decisions_dir()?)
    }

    pub fn open_in(dir: &Path) -> io::Result<Client> {
        check_private_dir(dir)?;
        let token = secret::read_secret(&dir.join(TOKEN))?;
        Ok(Client {
            dir: dir.to_path_buf(),
            token: Mutex::new(token),
        })
    }

    pub fn call(&self, op: Op) -> io::Result<Response> {
        self.call_with(op, None)
    }

    fn call_with(&self, op: Op, cancel: Option<&Cancel>) -> io::Result<Response> {
        let mut unreadable = None;
        for attempt in 1..=UNAUTHORIZED_ATTEMPTS {
            let token = self.token.lock().unwrap_or_else(|p| p.into_inner()).clone();
            let resp = self.send(op.clone(), token, cancel)?;
            if resp.ok || resp.error.as_deref() != Some(UNAUTHORIZED) {
                return Ok(resp);
            }
            if attempt < UNAUTHORIZED_ATTEMPTS {
                std::thread::sleep(UNAUTHORIZED_PAUSE);
                match secret::read_secret(&self.dir.join(TOKEN)) {
                    Ok(fresh) => {
                        *self.token.lock().unwrap_or_else(|p| p.into_inner()) = fresh;
                        unreadable = None;
                    }
                    Err(e) => unreadable = Some(e),
                }
            }
        }
        Err(unreadable
            .unwrap_or_else(|| io::Error::new(io::ErrorKind::PermissionDenied, UNAUTHORIZED)))
    }

    fn send(&self, op: Op, token: String, cancel: Option<&Cancel>) -> io::Result<Response> {
        let wait = match &op {
            Op::Wait { timeout_ms, .. } | Op::WaitAny { timeout_ms, .. } => {
                Duration::from_millis((*timeout_ms).min(MAX_WAIT_MS))
            }
            _ => Duration::ZERO,
        };
        let mut stream: UnixStream =
            endpoint::connect_within(&socket_endpoint(&self.dir), CONNECT_TIMEOUT)?;
        let timeouts = stream
            .set_write_timeout(Some(IO_GRACE))
            .and_then(|()| stream.set_read_timeout(Some(wait + IO_GRACE)));
        if let Err(e) = timeouts {
            let _ = stream.set_nonblocking(true);
            return match read_line_limited(&stream) {
                Ok(Some(line)) if is_busy(&line) => Err(busy()),
                _ => Err(e),
            };
        }
        if let Some(c) = cancel {
            c.attach(&stream)?;
        }
        let read = self.exchange(&mut stream, op, token);
        if cancel.is_some_and(Cancel::detach) {
            return Err(cancelled());
        }
        let line = read?;
        if is_busy(&line) {
            return Err(busy());
        }
        serde_json::from_slice(&line).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn exchange(&self, stream: &mut UnixStream, op: Op, token: String) -> io::Result<Vec<u8>> {
        let req = Request {
            v: PROTO,
            token,
            op,
        };
        if let Err(e) = write_line(stream, &req) {
            return match read_line_limited(&*stream) {
                Ok(Some(line)) if is_busy(&line) => Ok(line),
                _ => Err(e),
            };
        }
        read_line_limited(&*stream)?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the decisions inbox closed the connection",
            )
        })
    }

    fn checked(&self, op: Op) -> io::Result<Response> {
        self.checked_with(op, None)
    }

    fn checked_with(&self, op: Op, cancel: Option<&Cancel>) -> io::Result<Response> {
        let r = self.call_with(op, cancel)?;
        if r.ok {
            Ok(r)
        } else {
            Err(io::Error::other(
                r.error.unwrap_or_else(|| "refused".into()),
            ))
        }
    }

    pub fn ping(&self) -> io::Result<Response> {
        self.checked(Op::Ping)
    }

    pub fn create(&self, card: CardIn) -> io::Result<String> {
        self.create_outcome(card).map(|(id, _)| id)
    }

    pub fn create_outcome(&self, card: CardIn) -> io::Result<(String, Option<String>)> {
        let response = self.create_full(card)?;
        Ok((response.card_id.unwrap_or_default(), response.outcome))
    }

    pub fn create_full(&self, card: CardIn) -> io::Result<Response> {
        self.create_with(card, None)
    }

    pub fn create_with(&self, card: CardIn, inbox: Option<Inbox>) -> io::Result<Response> {
        let response = self.checked(Op::Create {
            card: Box::new(card),
            inbox,
        })?;
        if response.card_id.is_none() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "no card id in the answer",
            ));
        }
        Ok(response)
    }

    pub fn find(
        &self,
        task: Option<&str>,
        key: Option<&str>,
        query: Option<&str>,
        limit: Option<u32>,
    ) -> io::Result<Vec<Found>> {
        let r = self.call(Op::Find {
            task: task.map(str::to_owned),
            key: key.map(str::to_owned),
            query: query.map(str::to_owned),
            limit,
        })?;
        if r.ok {
            return Ok(r.found);
        }
        let msg = r.error.unwrap_or_else(|| "refused".into());
        if msg == BAD_REQUEST {
            return Err(io::Error::new(io::ErrorKind::Unsupported, msg));
        }
        Err(io::Error::other(msg))
    }

    pub fn wait(
        &self,
        card_id: &str,
        since: u64,
        timeout: Duration,
        final_only: bool,
    ) -> io::Result<Option<Event>> {
        self.wait_cancellable(card_id, since, timeout, final_only, None)
    }

    pub fn wait_cancellable(
        &self,
        card_id: &str,
        since: u64,
        timeout: Duration,
        final_only: bool,
        cancel: Option<&Cancel>,
    ) -> io::Result<Option<Event>> {
        Ok(self
            .checked_with(
                Op::Wait {
                    card_id: card_id.to_string(),
                    since,
                    timeout_ms: timeout_ms(timeout),
                    final_only,
                },
                cancel,
            )?
            .event)
    }

    /// The first event over `cards`, keyed by the card id as given (the event carries the
    /// canonical one). A desktop without `wait_any` answers `ErrorKind::Unsupported`.
    pub fn wait_any(
        &self,
        cards: &[WaitCard],
        timeout: Duration,
        final_only: bool,
        cancel: Option<&Cancel>,
    ) -> io::Result<Option<(String, Event)>> {
        let op = Op::WaitAny {
            cards: cards.to_vec(),
            timeout_ms: timeout_ms(timeout),
            final_only,
        };
        let r = self.call_with(op, cancel)?;
        if r.ok {
            return Ok(r
                .event
                .map(|e| (r.card_id.unwrap_or_else(|| e.card_id.clone()), e)));
        }
        let msg = r.error.unwrap_or_else(|| "refused".into());
        if msg == BAD_REQUEST {
            return Err(io::Error::new(io::ErrorKind::Unsupported, msg));
        }
        Err(io::Error::other(msg))
    }

    pub fn reply(
        &self,
        card_id: &str,
        text: Option<String>,
        line_replies: Vec<LineComment>,
    ) -> io::Result<()> {
        self.checked(Op::Reply {
            card_id: card_id.to_string(),
            text,
            line_replies,
        })
        .map(|_| ())
    }

    pub fn close_session(&self, agent: &str, session: &str) -> io::Result<u64> {
        Ok(self
            .checked(Op::CloseSession {
                agent: agent.to_string(),
                session: session.to_string(),
            })?
            .closed
            .unwrap_or(0))
    }
}

fn timeout_ms(timeout: Duration) -> u64 {
    timeout.as_millis().min(MAX_WAIT_MS as u128) as u64
}

/// Queue a card while the desktop is not running; the desktop takes it in on its next start.
pub fn spool(dir: &Path, card: &CardIn) -> io::Result<String> {
    let id = match card.id.as_deref() {
        Some(id) if valid_card_id(id) => id.to_string(),
        Some(_) => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid card id",
            ))
        }
        None => new_card_id(),
    };
    let mut card = card.clone();
    card.id = Some(id.clone());
    let inbox = dir.join(INBOX);
    ensure_private_dir(dir)?;
    ensure_private_dir(&inbox)?;
    let tmp = inbox.join(format!(".{id}.tmp"));
    let bytes = serde_json::to_vec(&card).map_err(io::Error::other)?;
    if bytes.len() > MAX_LINE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "card is too large",
        ));
    }
    {
        let mut f = OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW)
            .mode(0o600)
            .open(&tmp)?;
        f.write_all(&bytes)?;
        f.sync_all()?;
    }
    if let Err(e) = fs::rename(&tmp, inbox.join(format!("{id}.json"))) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(id)
}

/// Desktop side: hand each spooled card to `persist`; the file is deleted only when it returns true.
/// Bad files are dropped.
pub fn drain_spool(dir: &Path, mut persist: impl FnMut(CardIn) -> bool) -> io::Result<()> {
    let inbox = dir.join(INBOX);
    let entries = match fs::read_dir(&inbox) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e),
    };
    check_private_dir(&inbox)?;
    let mut entries: Vec<_> = entries
        .flatten()
        .map(|e| {
            let modified = e.metadata().and_then(|m| m.modified()).ok();
            (modified, e.file_name(), e.path())
        })
        .collect();
    entries.sort_by(|a, b| (&a.0, &a.1).cmp(&(&b.0, &b.1)));
    for (_, name, path) in entries {
        let name = name.to_string_lossy().into_owned();
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        let parsed = (|| -> Option<CardIn> {
            let meta = fs::symlink_metadata(&path).ok()?;
            if !meta.is_file() || meta.len() as usize > MAX_LINE {
                return None;
            }
            let card: CardIn = serde_json::from_slice(&fs::read(&path).ok()?).ok()?;
            (valid_card_id(id) && card.id.as_deref() == Some(id)).then_some(card)
        })();
        if parsed.is_none_or(&mut persist) {
            let _ = fs::remove_file(&path);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn side_is_optional_on_the_wire_both_ways() {
        let legacy: LineComment =
            serde_json::from_str(r#"{"file":"a.rs","line":3,"text":"x"}"#).unwrap();
        assert_eq!(legacy.side, None);
        let t: ThreadRef = serde_json::from_str(r#"{"file":"a.rs","line":3}"#).unwrap();
        assert_eq!(t.side, None);
        assert!(!serde_json::to_string(&legacy).unwrap().contains("side"));
        let old = LineComment {
            side: Some("old".into()),
            ..legacy
        };
        assert!(serde_json::to_string(&old)
            .unwrap()
            .contains(r#""side":"old""#));
    }

    const TOKEN_VALUE: &str = "s3cr3t-token-value";

    fn inbox_with_token() -> Inbox {
        Inbox {
            kind: "claude".into(),
            endpoint: "/tmp/claude.sock".into(),
            token: Some(Secret::new(TOKEN_VALUE)),
            session: Some("sess-1".into()),
        }
    }

    fn titled(title: &str) -> CardIn {
        CardIn {
            title: title.into(),
            ..Default::default()
        }
    }

    #[test]
    fn secret_debug_is_redacted_and_wire_is_plain() {
        let s = Secret::new(TOKEN_VALUE);
        assert_eq!(format!("{s:?}"), "<redacted>");
        assert_eq!(s.expose(), TOKEN_VALUE);
        assert_eq!(
            serde_json::to_string(&s).unwrap(),
            format!("\"{TOKEN_VALUE}\"")
        );
        let back: Secret = serde_json::from_str(&format!("\"{TOKEN_VALUE}\"")).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn debug_of_request_and_card_never_shows_the_token() {
        let request = Request {
            v: PROTO,
            token: "t".into(),
            op: Op::Create {
                card: Box::new(titled("Ship?")),
                inbox: Some(inbox_with_token()),
            },
        };
        assert!(!format!("{request:?}").contains(TOKEN_VALUE));
        assert!(!serde_json::to_string(&titled("Ship?"))
            .unwrap()
            .contains(TOKEN_VALUE));
    }

    #[test]
    fn create_round_trips_with_and_without_inbox() {
        for inbox in [Some(inbox_with_token()), None] {
            let request = Request {
                v: PROTO,
                token: "t".into(),
                op: Op::Create {
                    card: Box::new(titled("Ship?")),
                    inbox: inbox.clone(),
                },
            };
            let json = serde_json::to_string(&request).unwrap();
            assert_eq!(json.contains("inbox"), inbox.is_some());
            let back: Request = serde_json::from_str(&json).unwrap();
            assert_eq!(back, request);
        }
    }

    #[test]
    fn create_without_inbox_field_still_parses() {
        let r: Request =
            serde_json::from_str(r#"{"v":1,"token":"t","op":"create","card":{"title":"x"}}"#)
                .unwrap();
        assert!(matches!(r.op, Op::Create { inbox: None, .. }));
    }

    #[test]
    fn spool_file_never_holds_a_token() {
        let t = private_tmp();
        let dir = t.path().join("d");
        spool(&dir, &titled("Ship?")).unwrap();
        for entry in std::fs::read_dir(dir.join(INBOX)).unwrap() {
            let body = std::fs::read_to_string(entry.unwrap().path()).unwrap();
            assert!(!body.contains(TOKEN_VALUE));
            assert!(!body.contains("inbox"));
        }
    }

    fn private_tmp() -> tempfile::TempDir {
        let t = tempfile::Builder::new()
            .prefix("dq")
            .tempdir_in("/tmp")
            .unwrap();
        std::fs::set_permissions(
            t.path(),
            <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700),
        )
        .unwrap();
        t
    }

    #[test]
    fn card_ids_round_trip() {
        let id = new_card_id();
        assert!(valid_card_id(&id));
        assert!(!valid_card_id("c_../x"));
        assert!(!valid_card_id(&id.to_uppercase()));
    }

    #[test]
    fn request_wire_shape_is_flat() {
        let r = Request {
            v: PROTO,
            token: "t".into(),
            op: Op::Wait {
                card_id: "c_1".into(),
                since: 2,
                timeout_ms: 5,
                final_only: true,
            },
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["op"], "wait");
        assert_eq!(v["card_id"], "c_1");
        assert_eq!(serde_json::from_value::<Request>(v).unwrap(), r);
    }

    #[test]
    fn card_ids_strictly_increase() {
        let ids: Vec<String> = (0..2000).map(|_| new_card_id()).collect();
        assert!(ids.iter().all(|i| valid_card_id(i)));
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
    }

    #[test]
    fn drain_follows_creation_order_when_mtimes_tie() {
        let t = private_tmp();
        let dir = t.path().join("d");
        let mut made = Vec::new();
        for n in 0..20 {
            let card = CardIn {
                title: format!("card {n}"),
                ..Default::default()
            };
            made.push(spool(&dir, &card).unwrap());
        }
        let same = std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        for id in &made {
            let f = fs::File::options()
                .write(true)
                .open(dir.join(INBOX).join(format!("{id}.json")))
                .unwrap();
            f.set_modified(same).unwrap();
        }
        let mut got = Vec::new();
        drain_spool(&dir, |c| {
            got.push(c.id.unwrap());
            true
        })
        .unwrap();
        assert_eq!(got, made);
    }

    #[test]
    fn spool_round_trips_and_drains_once() {
        let t = private_tmp();
        let dir = t.path().join("d");
        let card = CardIn {
            title: "Ship?".into(),
            ..Default::default()
        };
        let id = spool(&dir, &card).unwrap();
        std::fs::write(dir.join(INBOX).join("junk.json"), b"{").unwrap();
        let mut got = Vec::new();
        drain_spool(&dir, |c| {
            got.push(c);
            true
        })
        .unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].id.as_deref(), Some(id.as_str()));
        let mut again = 0;
        drain_spool(&dir, |_| {
            again += 1;
            true
        })
        .unwrap();
        assert_eq!(again, 0);
        assert!(!dir.join(INBOX).join("junk.json").exists());
    }

    #[test]
    fn spool_drains_oldest_first() {
        let t = private_tmp();
        let dir = t.path().join("d");
        let older = spool(
            &dir,
            &CardIn {
                title: "older".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let newer = spool(
            &dir,
            &CardIn {
                title: "newer".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let stamp = |id: &str, secs: u64| {
            let file = fs::File::options()
                .write(true)
                .open(dir.join(INBOX).join(format!("{id}.json")))
                .unwrap();
            file.set_modified(std::time::UNIX_EPOCH + Duration::from_secs(secs))
                .unwrap();
        };
        stamp(&older, 1_000);
        stamp(&newer, 2_000);
        let mut got = Vec::new();
        drain_spool(&dir, |c| {
            got.push(c.title);
            true
        })
        .unwrap();
        assert_eq!(got, ["older", "newer"]);
    }

    #[test]
    fn foreign_schemes_are_detected_the_way_a_browser_reads_them() {
        for bad in [
            "javascript:alert(1)",
            "JAVASCRIPT:alert(1)",
            "java\tscript:alert(1)",
            "java\nscript:alert(1)",
            "\u{1}\u{2} javascript:alert(1)",
            "  data:text/html,x",
            "file:///etc/passwd",
            "vbscript:x",
            "javascript:1/alert(1)",
            "ftp://example.com/a",
            "mailto:a@b.c",
            "vscode:1/x",
            "ms-msdt:1/x",
            "tauri:1/x",
            "filesystem:1/x",
            "tel:12345",
        ] {
            assert!(has_foreign_scheme(bad), "{bad:?}");
        }
        for ok in [
            "https://example.com/a",
            "HTTP://example.com/a",
            "http://example.com/a b",
            "PR-77",
            "TM-9",
            "o/r#5",
            "o/r#x",
            "#8974",
            "localhost:3000",
            "example.com:8080/path",
            "PR 77: fix",
            "TM-9: fix login",
            "PR-77: title",
            "TM-839:x",
            "Task: TM-9",
            "localhost:3000?x=1",
            "example.com:8080#a",
            "",
        ] {
            assert!(!has_foreign_scheme(ok), "{ok:?}");
        }
    }

    #[test]
    fn plain_card_wire_format_is_unchanged() {
        let card: CardIn = serde_json::from_str(r#"{"title":"Ship?","links":{"pr":"u"}}"#).unwrap();
        assert!(card.diffs.is_empty() && card.patches.is_empty() && card.links.items.is_empty());
        assert_eq!(card.replaces, None);
        assert_eq!(card.key, None);
        let v = serde_json::to_value(&card).unwrap();
        for field in ["diffs", "patches", "replaces", "key"] {
            assert!(v.get(field).is_none(), "{field}");
        }
        assert!(v["links"].get("items").is_none());
        let r = Response {
            ok: true,
            ..Default::default()
        };
        assert!(serde_json::to_value(&r).unwrap().get("outcome").is_none());
    }

    #[test]
    fn spool_file_stays_until_persisted() {
        let t = private_tmp();
        let dir = t.path().join("d");
        let id = spool(
            &dir,
            &CardIn {
                title: "Ship?".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let file = dir.join(INBOX).join(format!("{id}.json"));
        drain_spool(&dir, |_| false).unwrap();
        assert!(file.exists());
        drain_spool(&dir, |_| true).unwrap();
        assert!(!file.exists());
    }

    #[test]
    fn long_lines_are_refused() {
        let big = vec![b'a'; MAX_LINE + 2];
        assert!(read_line_limited(&big[..]).is_err());
        assert_eq!(read_line_limited(&b"{}\n"[..]).unwrap().unwrap(), b"{}");
        assert!(read_line_limited(&b"{}"[..]).is_err());
    }

    #[test]
    fn too_long_is_told_apart_from_truncated() {
        let big = vec![b'a'; MAX_LINE + 2];
        let e = read_line_limited(&big[..]).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::FileTooLarge);
        assert!(e.to_string().contains("request too large"), "{e}");
        let e = read_line_limited(&b"{}"[..]).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn an_oversized_request_is_refused_on_the_client_side() {
        let card = CardIn {
            title: "x".into(),
            context: Some("a".repeat(MAX_LINE)),
            ..Default::default()
        };
        let mut sink = Vec::new();
        let e = write_line(&mut sink, &card).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::Other);
        assert!(e.to_string().contains("too large"), "{e}");
        assert!(sink.is_empty());
    }

    #[test]
    fn client_needs_a_private_token() {
        let t = private_tmp();
        let dir = t.path().join("d");
        assert!(Client::open_in(&dir).is_err());
        let tok = prepare_server_dir(&dir).unwrap();
        assert!(secret::valid_secret(&tok));
        assert!(Client::open_in(&dir).is_ok());
        let again = prepare_server_dir(&dir).unwrap();
        assert_ne!(tok, again);
    }

    fn serve(dir: &Path, answer: impl Fn(&Request) -> Response + Send + 'static) {
        let listener = std::os::unix::net::UnixListener::bind(dir.join(SOCK)).unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let Ok(Some(line)) = read_line_limited(&stream) else {
                    continue;
                };
                let req: Request = serde_json::from_slice(&line).unwrap();
                let _ = write_line(&mut stream, &answer(&req));
            }
        });
    }

    #[test]
    fn a_held_client_follows_a_rotated_token() {
        let t = private_tmp();
        let dir = t.path().join("d");
        let first = prepare_server_dir(&dir).unwrap();
        let expected = std::sync::Arc::new(Mutex::new(first));
        let seen = expected.clone();
        serve(&dir, move |req| {
            if token_ok(&seen.lock().unwrap(), &req.token) {
                Response::ok()
            } else {
                Response::err(UNAUTHORIZED)
            }
        });
        let client = Client::open_in(&dir).unwrap();
        client.ping().unwrap();
        *expected.lock().unwrap() = prepare_server_dir(&dir).unwrap();
        client.ping().unwrap();
        client.ping().unwrap();
    }

    #[test]
    fn retries_stop_with_a_non_final_error() {
        let t = private_tmp();
        let dir = t.path().join("d");
        prepare_server_dir(&dir).unwrap();
        serve(&dir, |_| Response::err(UNAUTHORIZED));
        let client = Client::open_in(&dir).unwrap();
        let started = std::time::Instant::now();
        let err = client.ping().unwrap_err();
        let took = started.elapsed();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(err.to_string(), UNAUTHORIZED);
        assert!(took >= Duration::from_millis(700), "{took:?}");
        assert!(took < Duration::from_secs(5), "{took:?}");
    }

    #[test]
    fn a_missing_token_during_retries_is_the_plain_io_error() {
        let t = private_tmp();
        let dir = t.path().join("d");
        prepare_server_dir(&dir).unwrap();
        serve(&dir, |_| Response::err(UNAUTHORIZED));
        let client = Client::open_in(&dir).unwrap();
        fs::remove_file(dir.join(TOKEN)).unwrap();
        assert_eq!(client.ping().unwrap_err().kind(), io::ErrorKind::NotFound);
    }

    fn waiting_card() -> Op {
        Op::Wait {
            card_id: "c_1".into(),
            since: 0,
            timeout_ms: 1,
            final_only: false,
        }
    }

    #[test]
    fn busy_is_its_own_line_that_old_clients_cannot_parse() {
        assert!(is_busy(BUSY_LINE.as_bytes()));
        let ok = serde_json::to_vec(&Response::ok()).unwrap();
        assert!(!is_busy(&ok));
        assert!(!is_busy(
            &serde_json::to_vec(&Response::err("busy")).unwrap()
        ));
        assert!(serde_json::from_str::<Response>(BUSY_LINE).is_err());
        let mut out = Vec::new();
        write_busy(&mut out).unwrap();
        assert_eq!(
            read_line_limited(&out[..]).unwrap().unwrap(),
            BUSY_LINE.as_bytes()
        );
    }

    #[test]
    fn a_busy_reply_is_resource_busy_even_before_the_request_is_read() {
        let t = private_tmp();
        let dir = t.path().join("d");
        prepare_server_dir(&dir).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(dir.join(SOCK)).unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let _ = write_busy(&mut stream);
            }
        });
        let client = Client::open_in(&dir).unwrap();
        for _ in 0..20 {
            let err = client.call(waiting_card()).unwrap_err();
            assert_eq!(err.kind(), io::ErrorKind::ResourceBusy, "{err}");
        }
    }

    #[test]
    fn cancel_ends_a_blocked_wait_at_once() {
        let t = private_tmp();
        let dir = t.path().join("d");
        prepare_server_dir(&dir).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(dir.join(SOCK)).unwrap();
        let (held_tx, held_rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let _ = read_line_limited(&stream);
                let _ = held_tx.send(stream);
            }
        });
        let client = Client::open_in(&dir).unwrap();
        let cancel = std::sync::Arc::new(Cancel::default());
        let c = cancel.clone();
        let waiter = std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let r = client.wait_cancellable("c_1", 0, Duration::from_secs(60), false, Some(&c));
            (r, started.elapsed())
        });
        let held = held_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        std::thread::sleep(Duration::from_millis(50));
        let fired = std::time::Instant::now();
        cancel.cancel();
        let (r, _) = waiter.join().unwrap();
        assert!(
            fired.elapsed() < Duration::from_millis(500),
            "{:?}",
            fired.elapsed()
        );
        assert_eq!(r.unwrap_err().kind(), io::ErrorKind::Interrupted);
        let mut byte = [0u8; 1];
        assert_eq!((&held).read(&mut byte).unwrap(), 0);
        assert!(cancel.sleep(Duration::from_secs(5)));
        assert_eq!(
            Client::open_in(&dir)
                .unwrap()
                .wait_cancellable("c_1", 0, Duration::from_secs(1), false, Some(&cancel))
                .unwrap_err()
                .kind(),
            io::ErrorKind::Interrupted
        );
    }

    #[test]
    fn an_uncancelled_sleep_runs_its_time() {
        let c = Cancel::default();
        let started = std::time::Instant::now();
        assert!(!c.sleep(Duration::from_millis(60)));
        assert!(started.elapsed() >= Duration::from_millis(55));
    }

    #[test]
    fn wait_any_on_a_desktop_without_it_is_unsupported() {
        let t = private_tmp();
        let dir = t.path().join("d");
        prepare_server_dir(&dir).unwrap();
        let listener = std::os::unix::net::UnixListener::bind(dir.join(SOCK)).unwrap();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                let Ok(Some(line)) = read_line_limited(&stream) else {
                    continue;
                };
                #[derive(Deserialize)]
                #[serde(tag = "op", rename_all = "snake_case")]
                enum OldOp {
                    Wait {},
                }
                let resp = match serde_json::from_slice::<OldOp>(&line) {
                    Ok(_) => Response::ok(),
                    Err(_) => Response::err(BAD_REQUEST),
                };
                let _ = write_line(&mut stream, &resp);
            }
        });
        let client = Client::open_in(&dir).unwrap();
        let cards = [WaitCard {
            card_id: "c_1".into(),
            since: 0,
        }];
        let err = client
            .wait_any(&cards, Duration::from_millis(10), false, None)
            .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Unsupported);
        assert!(client
            .wait("c_1", 0, Duration::from_millis(10), false)
            .unwrap()
            .is_none());
    }

    #[test]
    fn wait_any_wire_shape() {
        let r = Request {
            v: PROTO,
            token: "t".into(),
            op: Op::WaitAny {
                cards: vec![WaitCard {
                    card_id: "c_1".into(),
                    since: 3,
                }],
                timeout_ms: 5,
                final_only: false,
            },
        };
        let v: serde_json::Value = serde_json::to_value(&r).unwrap();
        assert_eq!(v["op"], "wait_any");
        assert_eq!(v["cards"][0]["card_id"], "c_1");
        assert_eq!(v["cards"][0]["since"], 3);
        assert_eq!(serde_json::from_value::<Request>(v).unwrap(), r);
    }

    #[test]
    fn find_wire_shape() {
        let r = Request {
            v: PROTO,
            token: "t".into(),
            op: Op::Find {
                task: Some("https://tmgr.dev/ws/tasks/211".into()),
                key: None,
                query: Some("ship it".into()),
                limit: Some(5),
            },
        };
        let v: serde_json::Value = serde_json::to_value(&r).unwrap();
        assert_eq!(v["op"], "find");
        assert_eq!(v["task"], "https://tmgr.dev/ws/tasks/211");
        assert_eq!(v["query"], "ship it");
        assert_eq!(v["limit"], 5);
        assert!(v.get("key").is_none());
        assert_eq!(serde_json::from_value::<Request>(v).unwrap(), r);
        let bare: Request = serde_json::from_str(r#"{"v":1,"token":"t","op":"find"}"#).unwrap();
        assert_eq!(
            bare.op,
            Op::Find {
                task: None,
                key: None,
                query: None,
                limit: None
            }
        );
    }

    #[test]
    fn found_and_force_new_are_optional_on_the_wire() {
        let card: CardIn = serde_json::from_str(r#"{"title":"Ship?"}"#).unwrap();
        assert_eq!(card.force_new, None);
        assert!(serde_json::to_value(&card)
            .unwrap()
            .get("force_new")
            .is_none());
        let found: Found = serde_json::from_str(
            r#"{"card_id":"c_1","title":"Ship?","state":"answered","created_ms":5}"#,
        )
        .unwrap();
        assert!(!found.dismissed && found.task.is_none() && found.choice_label.is_none());
        let v = serde_json::to_value(&found).unwrap();
        assert!(v.get("dismissed").is_none() && v.get("note").is_none());
        let r = Response {
            ok: true,
            ..Default::default()
        };
        let v = serde_json::to_value(&r).unwrap();
        assert!(v.get("found").is_none() && v.get("similar").is_none());
    }
}
