use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::Path;

use crate::claude::ClaudeCollector;
use crate::codex::CodexCollector;
use crate::collector::{Collector, LogFile, ParserState};
use crate::error::{Result, UsageError};
use crate::events::{upsert_event, upsert_session_meta, Upsert};
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
    pub errors: u64,
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

#[derive(Default)]
struct Tally {
    new: u64,
    updated: u64,
    last_ts: Option<i64>,
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
    let tool = collector.tool();
    let source_id = prepare_source(store, tool, &account.label, collector.parser_version())?;
    let salt = store.fingerprint_salt()?;
    let floor = store.rollup_floor_ms()?;
    let mut report = ScanReport::default();
    for file in collector.discover(account) {
        if scan_file(
            store,
            collector,
            source_id,
            &file,
            pricebook,
            &salt,
            floor,
            &mut report,
        )
        .is_err()
        {
            report.errors += 1;
        }
    }
    store.conn.execute(
        "UPDATE usage_source SET last_scan_at = ?2 WHERE id = ?1",
        params![source_id, ms(now)],
    )?;
    Ok(report)
}

fn prepare_source(store: &Store, tool: Tool, label: &str, version: u32) -> Result<i64> {
    store.conn.execute(
        "INSERT OR IGNORE INTO usage_source (tool, account) VALUES (?1, ?2)",
        params![tool.as_str(), label],
    )?;
    let (id, old): (i64, i64) = store.conn.query_row(
        "SELECT id, parser_version FROM usage_source WHERE tool = ?1 AND account = ?2",
        params![tool.as_str(), label],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if old != version as i64 {
        store
            .conn
            .execute("DELETE FROM usage_file_state WHERE source_id = ?1", [id])?;
        store.conn.execute(
            "UPDATE usage_source SET parser_version = ?2 WHERE id = ?1",
            params![id, version],
        )?;
    }
    Ok(id)
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
    report: &mut ScanReport,
) -> Result<()> {
    let st = stat_file(&file.path)?;
    let row = load_row(&store.conn, source_id, &file.rel)?;
    let (from, mut pstate) = match row {
        None => (0, ParserState::default()),
        Some(r) => {
            if (r.dev, r.ino, r.size, r.mtime_ns) == (st.dev, st.ino, st.size, st.mtime_ns) {
                report.files_unchanged += 1;
                return Ok(());
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
    let tx = store.conn.transaction()?;
    let mut tally = Tally::default();
    let mut failure: Option<UsageError> = None;
    let outcome = collector.parse(file, from, &mut pstate, &mut |rec| {
        if failure.is_some() {
            return;
        }
        if let Err(e) = apply(&tx, tool, &file.account, pricebook, floor, rec, &mut tally) {
            failure = Some(e);
        }
    });
    if let Some(e) = failure {
        return Err(e);
    }
    let outcome = outcome?;

    let head_len = outcome.next_offset.min(HEAD_BYTES);
    let tag = if head_len > 0 {
        head_tag(&file.path, salt, head_len)?
    } else {
        String::new()
    };
    let state_text = match &pstate.0 {
        serde_json::Value::Null => None,
        v => Some(serde_json::to_string(v)?),
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
            st.size as i64,
            st.mtime_ns,
            outcome.next_offset as i64,
            head_len as i64,
            tag,
            state_text,
            tally.last_ts,
        ],
    )?;
    tx.commit()?;

    report.files += 1;
    report.bytes += outcome.next_offset.saturating_sub(from);
    report.events_new += tally.new;
    report.events_updated += tally.updated;
    report.skipped_oversized += outcome.oversized;
    report.errors += outcome.bad_lines;
    Ok(())
}

fn apply(
    conn: &Connection,
    tool: Tool,
    account: &str,
    pricebook: &PriceBook,
    floor: i64,
    rec: ParsedRecord,
    tally: &mut Tally,
) -> Result<()> {
    match rec {
        ParsedRecord::Event(mut ev) => {
            ev.account = account.to_string();
            let (cost, _) = pricebook.price(&ev);
            match upsert_event(conn, tool, &ev, cost, floor)? {
                Upsert::New => tally.new += 1,
                Upsert::Updated => tally.updated += 1,
                Upsert::Unchanged => {}
            }
            let t = ms(ev.ts);
            tally.last_ts = Some(tally.last_ts.map_or(t, |l| l.max(t)));
        }
        ParsedRecord::Session(mut meta) => {
            meta.account = account.to_string();
            upsert_session_meta(conn, tool, &meta)?;
        }
        ParsedRecord::Limit(mut snap) => {
            snap.account = account.to_string();
            record_limit_conn(conn, &snap)?;
        }
    }
    Ok(())
}
