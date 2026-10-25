use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;

use crate::claude::ClaudeCollector;
use crate::codex::CodexCollector;
use crate::collector::{Collector, LogFile, ParserState};
use crate::error::{Result, UsageError};
use crate::events::{purge_session_events, upsert_event, upsert_session_meta, Upsert};
use crate::facts::{end_fact, upsert_agent, upsert_fact};
use crate::model::{hex, Account, ParsedRecord, Tool};
use crate::pricing::PriceBook;
use crate::store::{ms, record_limit_conn, Store};

const HEAD_BYTES: u64 = 256;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScanReport {
    pub files: u64,
    pub files_unchanged: u64,
    pub bytes: u64,
    pub events_new: u64,
    pub events_updated: u64,
    pub skipped_oversized: u64,
    pub skipped_before_floor: u64,
    pub bad_lines: u64,
    pub errors: u64,
    pub rescans_deferred: u64,
}

struct FileStat {
    dev: u64,
    ino: u64,
    size: u64,
    mtime_ns: i64,
}

struct FileRow {
    dev: u64,
    ino: u64,
    size: u64,
    mtime_ns: i64,
    offset: u64,
    head_len: u64,
    head_tag: String,
    parser_state: Option<String>,
}

impl FileRow {
    /// What another scan of the same file would have moved.
    fn position(&self) -> (u64, u64, u64, i64, u64) {
        (self.dev, self.ino, self.size, self.mtime_ns, self.offset)
    }
}

/// New lines read and stored per transaction (plus at most one long line past it).
pub const CHUNK_BYTES: u64 = 8 * 1024 * 1024;
const UNFINISHED_MTIME: i64 = -1;

enum FileEnd {
    Done,
    /// Reading this tool's logs was turned off: stop the scan of this account.
    Denied,
}

#[derive(Default)]
struct Tally {
    new: u64,
    updated: u64,
    skipped_before_floor: u64,
    last_ts: Option<i64>,
}

#[derive(Default)]
struct Reparse {
    seen: HashSet<String>,
    purged: HashSet<String>,
    file_seen: Vec<String>,
    file_purged: Vec<String>,
}

impl Reparse {
    fn begin_file(&mut self) {
        self.file_seen.clear();
        self.file_purged.clear();
    }

    fn rollback_file(&mut self) {
        for k in self.file_seen.drain(..) {
            self.seen.remove(&k);
        }
        for k in self.file_purged.drain(..) {
            self.purged.remove(&k);
        }
    }

    fn mark_purged(&mut self, session: &str) -> bool {
        let first = self.purged.insert(session.to_string());
        if first {
            self.file_purged.push(session.to_string());
        }
        first
    }

    fn mark_seen(&mut self, key: String) -> bool {
        let first = self.seen.insert(key.clone());
        if first {
            self.file_seen.push(key);
        }
        first
    }
}

struct Source {
    id: i64,
    reparse: bool,
}

pub fn scan_account(
    store: &mut Store,
    account: &Account,
    pricebook: &PriceBook,
    now: DateTime<Utc>,
) -> Result<ScanReport> {
    match account.tool {
        Tool::Claude => scan_with(store, &ClaudeCollector, account, pricebook, now),
        Tool::Codex => scan_with(store, &CodexCollector, account, pricebook, now),
    }
}

pub fn scan_with(
    store: &mut Store,
    collector: &dyn Collector,
    account: &Account,
    pricebook: &PriceBook,
    now: DateTime<Utc>,
) -> Result<ScanReport> {
    scan_in_chunks(store, collector, account, pricebook, now, CHUNK_BYTES)
}

/// `scan_with` with another chunk size (tests use small ones).
pub fn scan_in_chunks(
    store: &mut Store,
    collector: &dyn Collector,
    account: &Account,
    pricebook: &PriceBook,
    now: DateTime<Utc>,
    chunk_bytes: u64,
) -> Result<ScanReport> {
    let tool = collector.tool();
    let source = prepare_source(store, tool, &account.label, collector.parser_version())?;
    let salt = store.fingerprint_salt()?;
    let floor = store.rollup_floor_ms()?;
    let mut report = ScanReport::default();
    let mut reparse = source.reparse.then(Reparse::default);
    let (files, discover_errors) = collector.discover_checked(account);
    let mut unreadable = discover_errors;
    let mut stopped = false;
    for file in files {
        match scan_file(
            store,
            collector,
            source.id,
            &file,
            pricebook,
            &salt,
            floor,
            &mut reparse,
            &mut report,
            chunk_bytes,
        ) {
            Ok(FileEnd::Done) => {}
            Ok(FileEnd::Denied) => {
                stopped = true;
                break;
            }
            Err(_) => unreadable += 1,
        }
    }
    report.errors += unreadable;
    if source.reparse && !stopped && unreadable == 0 && report.rescans_deferred == 0 {
        store.conn.execute(
            "UPDATE usage_source SET parser_version = ?2 WHERE id = ?1 AND parser_version < ?2",
            params![source.id, collector.parser_version()],
        )?;
    }
    store.conn.execute(
        "UPDATE usage_source SET last_scan_at = ?2 WHERE id = ?1",
        params![source.id, ms(now)],
    )?;
    Ok(report)
}

fn prepare_source(store: &Store, tool: Tool, label: &str, version: u32) -> Result<Source> {
    store.conn.execute(
        "INSERT OR IGNORE INTO usage_source (tool, account) VALUES (?1, ?2)",
        params![tool.as_str(), label],
    )?;
    let (id, stored): (i64, i64) = store.conn.query_row(
        "SELECT id, parser_version FROM usage_source WHERE tool = ?1 AND account = ?2",
        params![tool.as_str(), label],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let reparse = stored < version as i64;
    Ok(Source { id, reparse })
}

fn stat_file(path: &Path) -> Result<FileStat> {
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.is_file() {
        return Err(UsageError::Invalid("not a regular file".into()));
    }
    let mtime_ns = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0);
    #[cfg(unix)]
    let (dev, ino) = {
        use std::os::unix::fs::MetadataExt;
        (meta.dev(), meta.ino())
    };
    #[cfg(not(unix))]
    let (dev, ino) = (0, 0);
    Ok(FileStat {
        dev,
        ino,
        size: meta.len(),
        mtime_ns,
    })
}

fn load_row(conn: &Connection, source_id: i64, key: &str) -> Result<Option<FileRow>> {
    Ok(conn
        .query_row(
            "SELECT dev, inode, size, mtime_ns, byte_offset, head_len, head_tag, parser_state
             FROM usage_file_state WHERE source_id = ?1 AND file_key = ?2",
            params![source_id, key],
            |r| {
                Ok(FileRow {
                    dev: r.get::<_, i64>(0)? as u64,
                    ino: r.get::<_, i64>(1)? as u64,
                    size: r.get::<_, i64>(2)? as u64,
                    mtime_ns: r.get(3)?,
                    offset: r.get::<_, i64>(4)? as u64,
                    head_len: r.get::<_, i64>(5)? as u64,
                    head_tag: r.get(6)?,
                    parser_state: r.get(7)?,
                })
            },
        )
        .optional()?)
}

/// True when a binary that does not collect context facts moved the token read position of a
/// file past the context one, so the file is read again from the start for its facts.
fn context_gap(conn: &Connection, source_id: i64, key: &str) -> Result<bool> {
    let tokens: Option<i64> = conn
        .query_row(
            "SELECT byte_offset FROM usage_file_state WHERE source_id = ?1 AND file_key = ?2",
            params![source_id, key],
            |r| r.get(0),
        )
        .optional()?;
    let Some(tokens) = tokens else {
        return Ok(false);
    };
    let context: Option<i64> = conn
        .query_row(
            "SELECT byte_offset FROM usage_context_file_state WHERE source_id = ?1 AND file_key = ?2",
            params![source_id, key],
            |r| r.get(0),
        )
        .optional()?;
    Ok(context != Some(tokens))
}

fn head_tag(path: &Path, salt: &str, len: u64) -> Result<String> {
    let mut buf = vec![0u8; len as usize];
    std::fs::File::open(path)?.read_exact(&mut buf)?;
    let mut h = Sha256::new();
    h.update(salt.as_bytes());
    h.update(&buf);
    Ok(hex(&h.finalize()))
}

#[allow(clippy::too_many_arguments)]
fn scan_file(
    store: &mut Store,
    collector: &dyn Collector,
    source_id: i64,
    file: &LogFile,
    pricebook: &PriceBook,
    salt: &str,
    floor: i64,
    reparse: &mut Option<Reparse>,
    report: &mut ScanReport,
    chunk_bytes: u64,
) -> Result<FileEnd> {
    if !store.log_consent()?.allows(collector.tool(), true) {
        return Ok(FileEnd::Denied);
    }
    let st = stat_file(&file.path)?;
    let mut behind = (reparse.is_none() && context_gap(&store.conn, source_id, &file.rel)?)
        .then(Reparse::default);
    let reparse = if behind.is_some() {
        &mut behind
    } else {
        reparse
    };
    if let Some(r) = load_row(&store.conn, source_id, &file.rel)? {
        if reparse.is_none()
            && (r.dev, r.ino, r.size, r.mtime_ns) == (st.dev, st.ino, st.size, st.mtime_ns)
        {
            report.files_unchanged += 1;
            return Ok(FileEnd::Done);
        }
    }
    let row = load_row(&store.conn, source_id, &file.rel)?;
    let mut seen = row.as_ref().map(FileRow::position);
    let previous = row.as_ref().map(|r| (r.size, r.offset));
    if let Some(rp) = reparse.as_mut() {
        rp.begin_file();
    }
    let (from, mut pstate) = match row {
        None => (0, ParserState::default()),
        Some(_) if reparse.is_some() => (0, ParserState::default()),
        Some(r) => {
            if (r.dev, r.ino, r.size, r.mtime_ns) == (st.dev, st.ino, st.size, st.mtime_ns) {
                report.files_unchanged += 1;
                return Ok(FileEnd::Done);
            }
            let rotated = (r.dev, r.ino) != (st.dev, st.ino)
                || st.size < r.size
                || st.size < r.offset
                || (r.head_len > 0
                    && head_tag(&file.path, salt, r.head_len).ok().as_deref()
                        != Some(r.head_tag.as_str()));
            if rotated {
                (0, ParserState::default())
            } else {
                let state = r
                    .parser_state
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .map(ParserState)
                    .unwrap_or_default();
                (r.offset, state)
            }
        }
    };

    let tool = collector.tool();
    if reparse.is_some() && tool == Tool::Codex {
        // A Codex reparse is all or nothing per file: check the whole file first, storing nothing.
        let mut probe = pstate.clone();
        let o = collector.parse(file, from, &mut probe, &mut |_| {})?;
        let whole = o.bad_lines == 0
            && o.oversized == 0
            && previous.is_none_or(|(size, offset)| st.size >= size && o.next_offset >= offset);
        if !whole {
            report.rescans_deferred += 1;
            return Ok(FileEnd::Done);
        }
    }

    // Each chunk is parsed without a lock, then stored in its own short transaction that first
    // re-checks the file position (another scan may have stored it) and the reading choice, so
    // saving "off" never waits behind parsing or a long write (D-027, D-030).
    let mut at = from;
    let mut last_ts = None;
    loop {
        if !store.log_consent()?.allows(tool, true) {
            return Ok(FileEnd::Denied);
        }
        if let Some(rp) = reparse.as_mut() {
            rp.begin_file();
        }
        let mut tally = Tally::default();
        let mut records = Vec::new();
        let mut next_state = pstate.clone();
        let parsed = collector.parse_until(
            file,
            at,
            at.saturating_add(chunk_bytes),
            &mut next_state,
            &mut |rec| records.push(rec),
        );
        let outcome = match parsed {
            Ok(o) => o,
            Err(e) => {
                if let Some(rp) = reparse.as_mut() {
                    rp.rollback_file();
                }
                return Err(e);
            }
        };
        let done =
            outcome.next_offset <= at || outcome.next_offset < at.saturating_add(chunk_bytes);
        let tx = store
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let moved = load_row(&tx, source_id, &file.rel)?
            .as_ref()
            .map(FileRow::position)
            != seen;
        // Inside the write lock the choice cannot change; `true`: any denial or tool switch-off
        // stops (background scans only start when granted).
        let off = !crate::consent::read_consent(&tx)?.allows(tool, true);
        if moved || off {
            if let Some(rp) = reparse.as_mut() {
                rp.rollback_file();
            }
            drop(tx);
            if off {
                return Ok(FileEnd::Denied);
            }
            report.files_unchanged += 1;
            return Ok(FileEnd::Done);
        }
        for rec in records {
            if let Err(e) = apply(
                &tx,
                tool,
                &file.account,
                pricebook,
                floor,
                reparse.as_mut(),
                rec,
                &mut tally,
            ) {
                if let Some(rp) = reparse.as_mut() {
                    rp.rollback_file();
                }
                return Err(e);
            }
        }
        last_ts = last_ts.max(tally.last_ts);
        let head_len = outcome.next_offset.min(HEAD_BYTES);
        let tag = if head_len > 0 {
            head_tag(&file.path, salt, head_len)?
        } else {
            String::new()
        };
        let state_text = match &next_state.0 {
            serde_json::Value::Null => None,
            v => Some(serde_json::to_string(v)?),
        };
        // An unfinished file is stored with its read position as size and no mtime, so the next
        // scan never takes it for unchanged and resumes from the last stored chunk.
        let (size, mtime_ns) = if done {
            (st.size, st.mtime_ns)
        } else {
            (outcome.next_offset, UNFINISHED_MTIME)
        };
        tx.execute(
            "INSERT OR REPLACE INTO usage_file_state
               (source_id, file_key, dev, inode, size, mtime_ns, byte_offset, head_len, head_tag,
                parser_state, last_event_ts)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                source_id,
                file.rel,
                st.dev as i64,
                st.ino as i64,
                size as i64,
                mtime_ns,
                outcome.next_offset as i64,
                head_len as i64,
                tag,
                state_text,
                last_ts,
            ],
        )?;
        tx.execute(
            "INSERT OR REPLACE INTO usage_context_file_state (source_id, file_key, byte_offset)
             VALUES (?1, ?2, ?3)",
            params![source_id, file.rel, outcome.next_offset as i64],
        )?;
        tx.commit()?;

        report.bytes += outcome.next_offset.saturating_sub(at);
        report.events_new += tally.new;
        report.events_updated += tally.updated;
        report.skipped_before_floor += tally.skipped_before_floor;
        report.skipped_oversized += outcome.oversized;
        report.bad_lines += outcome.bad_lines;
        seen = Some((st.dev, st.ino, size, mtime_ns, outcome.next_offset));
        if done {
            report.files += 1;
            return Ok(FileEnd::Done);
        }
        at = outcome.next_offset;
        pstate = next_state;
    }
}

#[allow(clippy::too_many_arguments)]
fn apply(
    conn: &Connection,
    tool: Tool,
    account: &str,
    pricebook: &PriceBook,
    floor: i64,
    mut reparse: Option<&mut Reparse>,
    rec: ParsedRecord,
    tally: &mut Tally,
) -> Result<()> {
    match rec {
        ParsedRecord::Event(mut ev) => {
            ev.account = account.to_string();
            if ms(ev.ts) < floor {
                tally.skipped_before_floor += 1;
                return Ok(());
            }
            let mut overwrite = false;
            if let Some(rp) = reparse.as_mut() {
                if tool == Tool::Codex && rp.mark_purged(&ev.tool_session_id) {
                    purge_session_events(conn, tool, account, &ev.tool_session_id)?;
                }
                overwrite = rp.mark_seen(format!("{}|{}", tool.as_str(), ev.event_key));
            }
            let (cost, _) = pricebook.price(&ev);
            match upsert_event(conn, tool, &ev, cost, floor, overwrite)? {
                Upsert::New => tally.new += 1,
                Upsert::Updated => tally.updated += 1,
                Upsert::Unchanged => {}
            }
            let t = ms(ev.ts);
            tally.last_ts = Some(tally.last_ts.map_or(t, |l| l.max(t)));
        }
        ParsedRecord::Session(mut meta) => {
            meta.account = account.to_string();
            if let Some(rp) = reparse.as_mut() {
                if tool == Tool::Codex && rp.mark_purged(&meta.tool_session_id) {
                    purge_session_events(conn, tool, account, &meta.tool_session_id)?;
                }
            }
            upsert_session_meta(conn, tool, &meta)?;
        }
        ParsedRecord::Limit(mut snap) => {
            snap.account = account.to_string();
            record_limit_conn(conn, &snap)?;
        }
        ParsedRecord::Fact(mut fact) => {
            fact.account = account.to_string();
            if ms(fact.ts) >= floor {
                upsert_fact(conn, tool, &fact)?;
            }
        }
        ParsedRecord::FactEnd(end) => end_fact(conn, tool, &end)?,
        ParsedRecord::Agent(mut agent) => {
            agent.account = account.to_string();
            upsert_agent(conn, tool, &agent)?;
        }
    }
    Ok(())
}
