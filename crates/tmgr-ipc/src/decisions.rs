use crate::dirs::{check_private_dir, ensure_private_dir};
use crate::endpoint::{self, Endpoint};
use crate::secret;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const PROTO: u32 = 1;
pub const SOCK: &str = "decisions.sock";
pub const TOKEN: &str = "decisions.token";
pub const DB: &str = "decisions.db";
pub const INBOX: &str = "inbox";
pub const MAX_LINE: usize = 4 << 20;
pub const MAX_DIFFS: usize = 8;
pub const MAX_PATCHES: usize = 8;
pub const MAX_PATCH_BYTES: usize = 1 << 20;
pub const MAX_LINKS: usize = 50;
pub const OUTCOME_CREATED: &str = "created";
pub const OUTCOME_UPDATED: &str = "updated";
pub const OUTCOME_FINAL: &str = "final";
pub const MAX_WAIT_MS: u64 = 3_600_000;
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
const IO_GRACE: Duration = Duration::from_secs(10);

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
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("system random source");
    format!("c_{}", secret::hex(&b))
}

pub fn valid_card_id(s: &str) -> bool {
    s.strip_prefix("c_")
        .is_some_and(|r| r.len() == 32 && r.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')))
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
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LineComment {
    pub file: String,
    pub line: u32,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThreadRef {
    pub file: String,
    pub line: u32,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    Ping,
    Create {
        card: Box<CardIn>,
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
    match fs::remove_file(&path) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    secret::write_new(&path)
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
    if buf.len() > MAX_LINE || buf.last() != Some(&b'\n') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "request line is too long or truncated",
        ));
    }
    buf.pop();
    Ok(Some(buf))
}

pub fn write_line<W: Write, T: Serialize>(w: &mut W, value: &T) -> io::Result<()> {
    let mut bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    bytes.push(b'\n');
    w.write_all(&bytes)?;
    w.flush()
}

pub struct Client {
    dir: PathBuf,
    token: String,
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
            token,
        })
    }

    pub fn call(&self, op: Op) -> io::Result<Response> {
        let wait = match &op {
            Op::Wait { timeout_ms, .. } => Duration::from_millis((*timeout_ms).min(MAX_WAIT_MS)),
            _ => Duration::ZERO,
        };
        let mut stream: UnixStream =
            endpoint::connect_within(&socket_endpoint(&self.dir), CONNECT_TIMEOUT)?;
        stream.set_write_timeout(Some(IO_GRACE))?;
        stream.set_read_timeout(Some(wait + IO_GRACE))?;
        let req = Request {
            v: PROTO,
            token: self.token.clone(),
            op,
        };
        write_line(&mut stream, &req)?;
        let line = read_line_limited(&stream)?.ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "the decisions inbox closed the connection",
            )
        })?;
        serde_json::from_slice(&line).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn checked(&self, op: Op) -> io::Result<Response> {
        let r = self.call(op)?;
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
        self.checked(Op::Create {
            card: Box::new(card),
        })?
        .card_id
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no card id in the answer"))
    }

    pub fn wait(
        &self,
        card_id: &str,
        since: u64,
        timeout: Duration,
        final_only: bool,
    ) -> io::Result<Option<Event>> {
        Ok(self
            .checked(Op::Wait {
                card_id: card_id.to_string(),
                since,
                timeout_ms: timeout.as_millis().min(MAX_WAIT_MS as u128) as u64,
                final_only,
            })?
            .event)
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
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
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
}
