use super::{claude, codex, open_regular, Agent, Entry};
use rusqlite::{params, Connection, OpenFlags, Transaction, TransactionBehavior};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fmt;
use std::fs::{DirBuilder, OpenOptions};
use std::io::{self, BufRead, BufReader, Seek, SeekFrom};
use std::os::unix::fs::{DirBuilderExt, FileExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::Duration;

const SESSION_CAP: usize = 1024 * 1024;
const TOOL_CAP: usize = 4 * 1024;
const TEXT_CAP: usize = 64 * 1024;
const LINE_CAP: usize = 8 * 1024 * 1024;
const ANCHOR_LEN: u64 = 4096;
const BLOB_WORD: usize = 200;
const BUSY_MS: u64 = 2000;

const SCHEMA: i64 = 2;

pub fn drop_index(path: &Path) -> io::Result<()> {
    tmgr_usage::consent::remove_index_files(path)
}

#[derive(Debug)]
pub enum IndexError {
    Io(io::Error),
    Sql(rusqlite::Error),
}

impl fmt::Display for IndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IndexError::Io(e) => write!(f, "{e}"),
            IndexError::Sql(e) => write!(f, "{e}"),
        }
    }
}

impl From<io::Error> for IndexError {
    fn from(e: io::Error) -> Self {
        IndexError::Io(e)
    }
}

impl From<rusqlite::Error> for IndexError {
    fn from(e: rusqlite::Error) -> Self {
        IndexError::Sql(e)
    }
}

impl IndexError {
    pub fn is_busy(&self) -> bool {
        matches!(
            self,
            IndexError::Sql(rusqlite::Error::SqliteFailure(e, _))
                if matches!(e.code, rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked)
        )
    }
}

pub struct Piece {
    pub text: String,
    pub tool: bool,
}

impl Piece {
    pub fn text(s: &str) -> Piece {
        Piece {
            text: s.to_string(),
            tool: false,
        }
    }

    pub fn tool(s: &str) -> Piece {
        Piece {
            text: s.to_string(),
            tool: true,
        }
    }
}

#[derive(Debug)]
pub struct Hit {
    pub session_id: String,
    pub snippet: String,
    pub rank: f64,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub indexed: usize,
    pub skipped: usize,
    pub removed: usize,
    pub busy: bool,
}

pub struct Index {
    pub(super) conn: Connection,
}

struct Row {
    size: u64,
    mtime_ms: u64,
    indexed_bytes: u64,
    truncated: bool,
    anchor: Vec<u8>,
    text_bytes: usize,
    session_id: String,
}

impl Index {
    pub fn open(path: &Path) -> Result<Index, IndexError> {
        if let Some(dir) = path.parent() {
            DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
        }
        OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)?
            .set_permissions(std::fs::Permissions::from_mode(0o600))?;
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(Duration::from_millis(BUSY_MS))?;
        conn.pragma_update(None, "secure_delete", "ON")?;
        conn.query_row("PRAGMA journal_mode=WAL", [], |_| Ok(()))?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != SCHEMA {
            conn.execute_batch("DROP TABLE IF EXISTS chunks; DROP TABLE IF EXISTS files;")?;
        }
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS files(
                path TEXT PRIMARY KEY, agent TEXT NOT NULL, session_id TEXT NOT NULL,
                size INTEGER NOT NULL, mtime_ms INTEGER NOT NULL, indexed_bytes INTEGER NOT NULL,
                truncated INTEGER NOT NULL, anchor BLOB NOT NULL, text_bytes INTEGER NOT NULL);
             CREATE VIRTUAL TABLE IF NOT EXISTS chunks USING fts5(
                session_id UNINDEXED, path UNINDEXED, text, tokenize='unicode61');",
        )?;
        if version != SCHEMA {
            conn.execute(
                "INSERT INTO chunks(chunks, rank) VALUES('secure-delete', 1)",
                [],
            )?;
            conn.pragma_update(None, "user_version", SCHEMA)?;
        }
        Ok(Index { conn })
    }

    pub fn open_read_only(path: &Path) -> Result<Index, IndexError> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(Duration::from_millis(20))?;
        Ok(Index { conn })
    }

    pub fn update(
        &self,
        entries: &[Entry],
        progress: impl FnMut(usize, usize),
    ) -> Result<Outcome, IndexError> {
        self.update_while(entries, progress, |_| true)
    }

    pub fn update_while(
        &self,
        entries: &[Entry],
        mut progress: impl FnMut(usize, usize),
        may_index: impl Fn(&Entry) -> bool,
    ) -> Result<Outcome, IndexError> {
        let todo: Vec<&Entry> = entries.iter().filter(|e| e.readable).collect();
        let mut out = Outcome::default();
        match self.remove_missing(&todo) {
            Ok(n) => out.removed = n,
            Err(e) if e.is_busy() => {
                out.busy = true;
                return Ok(out);
            }
            Err(e) => return Err(e),
        }
        let total = todo.len();
        for (i, e) in todo.iter().enumerate() {
            if !may_index(e) {
                out.skipped += 1;
                progress(i + 1, total);
                continue;
            }
            match self.index_file(e) {
                Ok(true) => out.indexed += 1,
                Ok(false) => out.skipped += 1,
                Err(IndexError::Io(_)) => out.skipped += 1,
                Err(e) if e.is_busy() => {
                    out.busy = true;
                    break;
                }
                Err(e) => return Err(e),
            }
            progress(i + 1, total);
        }
        Ok(out)
    }

    fn remove_missing(&self, keep: &[&Entry]) -> Result<usize, IndexError> {
        let keep: HashSet<String> = keep
            .iter()
            .map(|e| e.path.to_string_lossy().into_owned())
            .collect();
        let mut stmt = self.conn.prepare("SELECT path FROM files")?;
        let gone: Vec<String> = stmt
            .query_map([], |r| r.get(0))?
            .filter_map(Result::ok)
            .filter(|p| !keep.contains(p))
            .collect();
        drop(stmt);
        if gone.is_empty() {
            return Ok(0);
        }
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        for path in &gone {
            tx.execute("DELETE FROM chunks WHERE path=?1", [path])?;
            tx.execute("DELETE FROM files WHERE path=?1", [path])?;
        }
        tx.commit()?;
        Ok(gone.len())
    }

    fn row(&self, path: &str) -> Result<Option<Row>, IndexError> {
        let mut stmt = self.conn.prepare_cached(
            "SELECT size, mtime_ms, indexed_bytes, truncated, anchor, text_bytes, session_id FROM files WHERE path=?1",
        )?;
        let mut rows = stmt.query([path])?;
        let Some(r) = rows.next()? else {
            return Ok(None);
        };
        Ok(Some(Row {
            size: r.get::<_, i64>(0)? as u64,
            mtime_ms: r.get::<_, i64>(1)? as u64,
            indexed_bytes: r.get::<_, i64>(2)? as u64,
            truncated: r.get::<_, i64>(3)? != 0,
            anchor: r.get(4)?,
            text_bytes: r.get::<_, i64>(5)? as usize,
            session_id: r.get(6)?,
        }))
    }

    fn index_file(&self, e: &Entry) -> Result<bool, IndexError> {
        let mut f = open_regular(&e.path)?;
        let meta = f.metadata()?;
        let (size, mtime) = (meta.len(), super::mtime_ms(&meta));
        let path = e.path.to_string_lossy().into_owned();
        let prev = self.row(&path)?;
        if prev
            .as_ref()
            .is_some_and(|r| r.size == size && r.mtime_ms == mtime)
        {
            return Ok(false);
        }
        let appending = match &prev {
            Some(r) if size >= r.size => anchor(&f, r.indexed_bytes)? == r.anchor,
            _ => false,
        };
        let tx = Transaction::new_unchecked(&self.conn, TransactionBehavior::Immediate)?;
        let (start, mut text_bytes, mut truncated, session_id) = match (&prev, appending) {
            (Some(r), true) => (
                r.indexed_bytes,
                r.text_bytes,
                r.truncated,
                r.session_id.clone(),
            ),
            _ => {
                if prev.is_some() {
                    tx.execute("DELETE FROM chunks WHERE path=?1", [&path])?;
                }
                (0, 0, false, e.id.clone())
            }
        };
        let mut pos = start;
        if !truncated {
            f.seek(SeekFrom::Start(start))?;
            let mut reader = BufReader::with_capacity(64 * 1024, &f);
            let mut insert =
                tx.prepare_cached("INSERT INTO chunks(session_id, path, text) VALUES(?1, ?2, ?3)")?;
            let mut first = start == 0;
            let mut line = Vec::new();
            'lines: while let Some((n, complete)) = read_line(&mut reader, &mut line)? {
                if !complete {
                    break;
                }
                pos += n as u64;
                let Some(v) = serde_json::from_slice::<Value>(&line).ok() else {
                    first = false;
                    continue;
                };
                if first && e.agent == Agent::Codex && v["type"] == "session_meta" {
                    if codex::is_subagent(&v) {
                        truncated = true;
                        break;
                    }
                    if v["payload"]["id"].as_str().is_some_and(|i| i != e.id) {
                        truncated = true;
                        break;
                    }
                }
                first = false;
                let mut pieces = Vec::new();
                match e.agent {
                    Agent::Claude => claude::pieces(&v, &mut pieces),
                    Agent::Codex => codex::pieces(&v, &mut pieces),
                }
                for p in pieces {
                    let cap = if p.tool { TOOL_CAP } else { TEXT_CAP };
                    let mut text = strip_blobs(&p.text);
                    cut(&mut text, cap);
                    let room = SESSION_CAP - text_bytes;
                    if text.len() > room {
                        cut(&mut text, room);
                        truncated = true;
                    }
                    if !text.trim().is_empty() {
                        insert.execute(params![session_id, path, text])?;
                        text_bytes += text.len();
                    }
                    if truncated {
                        break 'lines;
                    }
                }
            }
        } else {
            pos = start;
        }
        let new_anchor = anchor(&f, pos)?;
        tx.execute(
            "INSERT OR REPLACE INTO files(path, agent, session_id, size, mtime_ms, indexed_bytes, truncated, anchor, text_bytes)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                path,
                e.agent.profile(),
                session_id,
                size as i64,
                mtime as i64,
                pos as i64,
                i64::from(truncated),
                new_anchor,
                text_bytes as i64
            ],
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn search(&self, q: &str, limit: usize) -> Result<Vec<Hit>, IndexError> {
        let Some(m) = match_query(q) else {
            return Ok(Vec::new());
        };
        let mut stmt = self.conn.prepare_cached(
            "SELECT session_id, snippet(chunks, 2, '', '', '…', 16), bm25(chunks) FROM chunks
             WHERE chunks MATCH ?1 ORDER BY bm25(chunks) LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![m, (limit * 20 + 100) as i64], |r| {
            Ok(Hit {
                session_id: r.get(0)?,
                snippet: r.get(1)?,
                rank: r.get(2)?,
            })
        })?;
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        for hit in rows {
            let hit = hit?;
            if out.len() < limit && seen.insert(hit.session_id.clone()) {
                out.push(hit);
            }
        }
        Ok(out)
    }
}

impl Index {
    pub fn fragments(
        &self,
        agent: &str,
        words: &[String],
        sessions: usize,
        per_session: usize,
        chars: usize,
    ) -> Result<Vec<(String, Vec<String>)>, IndexError> {
        if words.is_empty() {
            return Ok(Vec::new());
        }
        let m = words
            .iter()
            .map(|w| format!("\"{}\"", w.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" OR ");
        let mut stmt = self.conn.prepare_cached(
            "SELECT session_id, snippet(chunks, 2, '', '', '…', 48) FROM chunks
             WHERE chunks MATCH ?1 AND path IN (SELECT path FROM files WHERE agent=?2)
             ORDER BY bm25(chunks) LIMIT ?3",
        )?;
        let limit = (sessions * per_session * 10 + 100) as i64;
        let rows = stmt.query_map(params![m, agent, limit], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?;
        let mut out: Vec<(String, Vec<String>)> = Vec::new();
        for row in rows {
            let (id, mut text) = row?;
            cut(&mut text, chars);
            match out.iter().position(|(i, _)| *i == id) {
                Some(n) if out[n].1.len() < per_session => out[n].1.push(text),
                Some(_) => {}
                None if out.len() < sessions => out.push((id, vec![text])),
                None => {}
            }
        }
        Ok(out)
    }
}

pub fn enforce_consent(path: &Path, claude: bool, codex: bool) -> io::Result<()> {
    if !path.exists() {
        return Ok(());
    }
    if !claude && !codex {
        return drop_index(path);
    }
    let holds_denied = match Index::open(path) {
        Ok(ix) => ix
            .conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM files WHERE (agent=?1 AND ?2=0) OR (agent=?3 AND ?4=0))",
                params![
                    Agent::Claude.profile(),
                    i64::from(claude),
                    Agent::Codex.profile(),
                    i64::from(codex)
                ],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n != 0)
            .unwrap_or(true),
        Err(_) => true,
    };
    if holds_denied {
        drop_index(path)?;
    }
    Ok(())
}

pub fn match_query(q: &str) -> Option<String> {
    let words: Vec<&str> = q
        .split_whitespace()
        .filter(|w| w.chars().any(char::is_alphanumeric))
        .collect();
    let last = words.len().checked_sub(1)?;
    Some(
        words
            .iter()
            .enumerate()
            .map(|(i, w)| {
                let star = if i == last { "*" } else { "" };
                format!("\"{}\"{star}", w.replace('"', "\"\""))
            })
            .collect::<Vec<_>>()
            .join(" AND "),
    )
}

fn anchor(f: &std::fs::File, end: u64) -> io::Result<Vec<u8>> {
    let from = end.saturating_sub(ANCHOR_LEN);
    let mut buf = vec![0u8; (end - from) as usize];
    f.read_exact_at(&mut buf, from)?;
    Ok(Sha256::digest(&buf).to_vec())
}

fn read_line<R: BufRead>(r: &mut R, line: &mut Vec<u8>) -> io::Result<Option<(usize, bool)>> {
    line.clear();
    let mut total = 0usize;
    let mut over = false;
    loop {
        let buf = r.fill_buf()?;
        if buf.is_empty() {
            return Ok((total > 0).then_some((total, false)));
        }
        let (n, found) = match buf.iter().position(|b| *b == b'\n') {
            Some(i) => (i + 1, true),
            None => (buf.len(), false),
        };
        if !over && total + n <= LINE_CAP {
            line.extend_from_slice(&buf[..n]);
        } else {
            over = true;
            line.clear();
        }
        total += n;
        r.consume(n);
        if found {
            return Ok(Some((total, true)));
        }
    }
}

fn strip_blobs(s: &str) -> String {
    if !s.split_whitespace().any(|w| w.len() > BLOB_WORD) {
        return s.to_string();
    }
    s.split_whitespace()
        .filter(|w| w.len() <= BLOB_WORD)
        .collect::<Vec<_>>()
        .join(" ")
}

fn cut(s: &mut String, max: usize) {
    if s.len() > max {
        let mut i = max;
        while !s.is_char_boundary(i) {
            i -= 1;
        }
        s.truncate(i);
    }
}
