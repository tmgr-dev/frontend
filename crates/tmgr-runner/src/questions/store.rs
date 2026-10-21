use crate::wire::write_atomic;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use tmgr_ipc::dirs::{ensure_private_dir, questions_dir};
use tmgr_ipc::proto::valid_session_id;

pub const MAX_PER_SESSION: usize = 200;
const MAX_FILES: usize = 512;
const MAX_FILE: u64 = 8 << 20;
const KEEP_S: u64 = 30 * 24 * 3600;
const MARKS: &str = "marks.jsonl";
const SETTINGS: &str = "settings.json";
const LOCK: &str = "scan.lock";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Open,
    Answered,
    Ignored,
}

impl Status {
    pub fn word(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::Answered => "answered",
            Status::Ignored => "ignored",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Rule,
    Haiku,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    pub id: String,
    pub session: String,
    pub text: String,
    pub asked_s: u64,
    pub origin: Origin,
    #[serde(default)]
    pub user: String,
    #[serde(default)]
    pub reply: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Entry {
    #[serde(flatten)]
    pub q: Question,
    pub status: Status,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionFile {
    pub transcript: String,
    pub offset: u64,
    pub questions: Vec<Question>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Mark {
    id: String,
    status: Status,
    at_s: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub haiku: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { haiku: true }
    }
}

#[derive(Clone, Debug)]
pub struct Store {
    dir: PathBuf,
}

pub fn question_id(session: &str, start: u64, text: &str) -> String {
    let mut h = Sha256::new();
    h.update(session.as_bytes());
    h.update([0]);
    h.update(start.to_le_bytes());
    h.update([0]);
    h.update(text.as_bytes());
    h.finalize()[..6]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn read_file(path: &Path) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .and_then(|f| f.take(MAX_FILE).read_to_end(&mut buf))
        .ok()?;
    Some(buf)
}

pub struct ScanLock {
    _file: File,
}

impl Store {
    pub fn new(runner_dir: &Path) -> Store {
        Store {
            dir: questions_dir(runner_dir),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, session: &str) -> Option<PathBuf> {
        valid_session_id(session).then(|| self.dir.join(format!("{session}.json")))
    }

    pub fn try_lock(&self) -> std::io::Result<Option<ScanLock>> {
        ensure_private_dir(&self.dir)?;
        let f = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(self.dir.join(LOCK))?;
        let rc = unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        Ok((rc == 0).then_some(ScanLock { _file: f }))
    }

    pub fn settings(&self) -> Settings {
        read_file(&self.dir.join(SETTINGS))
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save_settings(&self, s: Settings) -> std::io::Result<()> {
        ensure_private_dir(&self.dir)?;
        let bytes = serde_json::to_vec(&s).map_err(std::io::Error::other)?;
        write_atomic(&self.dir.join(SETTINGS), &bytes)
    }

    pub fn load(&self, session: &str) -> SessionFile {
        self.path(session)
            .and_then(|p| read_file(&p))
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, session: &str, file: &SessionFile, now_s: u64) -> std::io::Result<()> {
        let path = self
            .path(session)
            .ok_or_else(|| std::io::Error::other("invalid session id"))?;
        ensure_private_dir(&self.dir)?;
        let mut file = file.clone();
        file.questions
            .retain(|q| q.asked_s.saturating_add(KEEP_S) > now_s);
        let excess = file.questions.len().saturating_sub(MAX_PER_SESSION);
        file.questions.drain(..excess);
        let bytes = serde_json::to_vec(&file).map_err(std::io::Error::other)?;
        write_atomic(&path, &bytes)
    }

    pub fn mark(&self, id: &str, status: Status, now_s: u64) -> std::io::Result<()> {
        ensure_private_dir(&self.dir)?;
        let mut line = serde_json::to_vec(&Mark {
            id: id.to_string(),
            status,
            at_s: now_s,
        })
        .map_err(std::io::Error::other)?;
        line.insert(0, b'\n');
        line.push(b'\n');
        let mut f = OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(self.dir.join(MARKS))?;
        f.write_all(&line)
    }

    fn marks(&self) -> HashMap<String, Status> {
        let Some(buf) = read_file(&self.dir.join(MARKS)) else {
            return HashMap::new();
        };
        buf.split(|b| *b == b'\n')
            .filter_map(|l| serde_json::from_slice::<Mark>(l).ok())
            .map(|m| (m.id, m.status))
            .collect()
    }

    pub fn entries(&self, session: Option<&str>) -> Vec<Entry> {
        let marks = self.marks();
        let files: Vec<PathBuf> = match session {
            Some(s) => self.path(s).into_iter().collect(),
            None => std::fs::read_dir(&self.dir)
                .map(|rd| {
                    rd.flatten()
                        .map(|e| e.path())
                        .filter(|p| {
                            p.file_name()
                                .and_then(|n| n.to_str())
                                .is_some_and(|n| n.starts_with("s_") && n.ends_with(".json"))
                        })
                        .take(MAX_FILES)
                        .collect()
                })
                .unwrap_or_default(),
        };
        let mut out: Vec<Entry> = files
            .iter()
            .filter_map(|p| read_file(p))
            .filter_map(|b| serde_json::from_slice::<SessionFile>(&b).ok())
            .flat_map(|f| f.questions)
            .map(|q| Entry {
                status: marks.get(&q.id).copied().unwrap_or(Status::Open),
                q,
            })
            .collect();
        out.sort_by(|a, b| b.q.asked_s.cmp(&a.q.asked_s).then(a.q.id.cmp(&b.q.id)));
        out
    }

    pub fn find(&self, prefix: &str) -> Result<Entry, String> {
        if prefix.len() < 4 {
            return Err("give at least 4 characters of the question id".into());
        }
        let mut hits: Vec<Entry> = self
            .entries(None)
            .into_iter()
            .filter(|e| e.q.id.starts_with(prefix))
            .collect();
        match hits.len() {
            0 => Err(format!("no question with id {prefix}")),
            1 => Ok(hits.remove(0)),
            _ => Err(format!("{prefix} matches several questions")),
        }
    }
}
