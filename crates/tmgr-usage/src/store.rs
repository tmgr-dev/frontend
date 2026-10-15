use chrono::{DateTime, Duration, Offset, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::BTreeMap;
use std::path::Path;

use crate::aggregate::Tz;
use crate::error::{Result, UsageError};
use crate::limits::{LimitSnapshot, LimitSource, LimitWindow};
use crate::model::{CostBasis, Tool};
use crate::pricing::{PriceBook, PriceEntry};
use crate::schema::{CONTEXT_MIGRATIONS, MIGRATIONS};

const ROLLUP_FLOOR_KEY: &str = "rollup_floor_ms";
const CONTEXT_MIN_READER_KEY: &str = "context_min_reader";
/// The oldest context version that can still use the database. Only a migration that old readers
/// would misread raises it; additive ones leave it alone.
const CONTEXT_MIN_READER: i64 = 1;
const ROLLUP_KEEP_DAYS: i64 = 730;
const LIMIT_KEEP_DAYS: i64 = 30;
const SAME_WINDOW_S: i64 = 120;

#[derive(Default)]
struct Fold {
    name: String,
    tokens: crate::model::Tokens,
    cost: f64,
    events: u64,
    unpriced: u64,
}

pub struct Store {
    pub(crate) conn: Connection,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PruneReport {
    pub events_folded: u64,
    pub rollup_rows_dropped: u64,
}

pub fn ms(ts: DateTime<Utc>) -> i64 {
    ts.timestamp_millis()
}

pub fn from_ms(v: i64) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(v).single().unwrap_or_default()
}

impl Store {
    pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;
    pub const CONTEXT_VERSION: i64 = CONTEXT_MIGRATIONS.len() as i64;

    pub fn open(path: &Path) -> Result<Store> {
        let existed = path.exists();
        if !existed {
            create_private_file(path)?;
        }
        Self::from_connection(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Store> {
        Self::from_connection(Connection::open_in_memory()?)
    }

    fn from_connection(conn: Connection) -> Result<Store> {
        conn.busy_timeout(std::time::Duration::from_millis(30_000))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&conn, MIGRATIONS.len())?;
        migrate_context(&conn)?;
        Ok(Store { conn })
    }

    pub fn schema_version(&self) -> Result<i64> {
        current_version(&self.conn)
    }

    pub fn context_version(&self) -> Result<i64> {
        context_version(&self.conn)
    }

    pub(crate) fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub(crate) fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    pub fn take_notice(&self, key: &str, times: u32) -> Result<bool> {
        Ok(self
            .conn
            .query_row(
                "INSERT INTO settings (key, value) VALUES (?1, '1')
                 ON CONFLICT(key) DO UPDATE SET value = CAST(value AS INTEGER) + 1
                 WHERE CAST(value AS INTEGER) < ?2
                 RETURNING value",
                params![format!("notice_shown:{key}"), times],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub(crate) fn fingerprint_salt(&self) -> Result<String> {
        if let Some(s) = self.setting("fp_salt")? {
            return Ok(s);
        }
        let salt: String = self
            .conn
            .query_row("SELECT hex(randomblob(16))", [], |r| r.get(0))?;
        self.set_setting("fp_salt", &salt)?;
        Ok(salt)
    }

    pub(crate) fn rollup_floor_ms(&self) -> Result<i64> {
        Ok(self
            .setting(ROLLUP_FLOOR_KEY)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(i64::MIN))
    }

    pub fn event_count(&self) -> Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM usage_event", [], |r| r.get(0))?)
    }

    pub fn record_limit(&self, snap: &LimitSnapshot) -> Result<()> {
        record_limit_conn(&self.conn, snap)
    }

    pub fn current_limits(&self, now: DateTime<Utc>) -> Result<Vec<LimitSnapshot>> {
        let mut stmt = self.conn.prepare(
            "SELECT tool, account, tool_session_id, window, used_pct, resets_at, plan, observed_at, source
             FROM usage_limit_snapshot
             WHERE resets_at IS NULL OR resets_at > ?1
             ORDER BY observed_at DESC, id DESC",
        )?;
        let rows = stmt.query_map([ms(now)], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, f64>(4)?,
                r.get::<_, Option<i64>>(5)?,
                r.get::<_, Option<String>>(6)?,
                r.get::<_, i64>(7)?,
                r.get::<_, String>(8)?,
            ))
        })?;
        let mut out: Vec<LimitSnapshot> = Vec::new();
        for row in rows {
            let (tool, account, session, window, used, resets, plan, observed, source) = row?;
            let (Ok(tool), Some(window), Some(source)) = (
                Tool::parse(&tool),
                LimitWindow::from_key(&window),
                LimitSource::from_key(&source),
            ) else {
                continue;
            };
            let snap = LimitSnapshot {
                tool,
                account,
                window,
                used_pct: used,
                resets_at: resets.map(from_ms),
                plan,
                observed_at: from_ms(observed),
                source,
                tool_session_id: session,
            };
            match out.iter_mut().find(|s| {
                s.tool == snap.tool && s.account == snap.account && s.window == snap.window
            }) {
                Some(best) if fresher(&snap, best) => *best = snap,
                Some(_) => {}
                None => out.push(snap),
            }
        }
        out.sort_by(|a, b| (a.tool, &a.account, &a.window).cmp(&(b.tool, &b.account, &b.window)));
        Ok(out)
    }

    pub fn save_prices(&self, book: &PriceBook, source: &str, now: DateTime<Utc>) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("DELETE FROM price_book", [])?;
        {
            let mut ins = tx.prepare(
                "INSERT INTO price_book (model_key, entry, source, fetched_at) VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (key, entry) in book.entries() {
                ins.execute(params![key, serde_json::to_string(entry)?, source, ms(now)])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn load_prices(&self) -> Result<Option<(PriceBook, DateTime<Utc>)>> {
        let mut stmt = self
            .conn
            .prepare("SELECT model_key, entry, fetched_at FROM price_book")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?;
        let mut entries = std::collections::BTreeMap::new();
        let mut fetched = 0;
        for row in rows {
            let (key, entry, at) = row?;
            let entry: PriceEntry = serde_json::from_str(&entry)?;
            entries.insert(key, entry);
            fetched = fetched.max(at);
        }
        if entries.is_empty() {
            return Ok(None);
        }
        Ok(Some((PriceBook::with_entries(entries), from_ms(fetched))))
    }

    pub fn reprice(&mut self, book: &PriceBook) -> Result<u64> {
        let changes = {
            let mut stmt = self.conn.prepare(
                "SELECT id, model, input, output, cache_read, cache_write_5m, cache_write_1h, service_tier, cost_usd, speed
                 FROM usage_event",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    crate::model::Tokens {
                        input: r.get::<_, i64>(2)? as u64,
                        output: r.get::<_, i64>(3)? as u64,
                        cache_read: r.get::<_, i64>(4)? as u64,
                        cache_write_5m: r.get::<_, i64>(5)? as u64,
                        cache_write_1h: r.get::<_, i64>(6)? as u64,
                        reasoning: 0,
                    },
                    r.get::<_, Option<String>>(7)?,
                    r.get::<_, Option<f64>>(8)?,
                    r.get::<_, Option<String>>(9)?,
                ))
            })?;
            let mut changes = Vec::new();
            for row in rows {
                let (id, model, tokens, tier, old, speed) = row?;
                let new = book.cost_at(&model, &tokens, tier.as_deref(), speed.as_deref());
                if new != old {
                    changes.push((id, new));
                }
            }
            changes
        };
        let tx = self.conn.transaction()?;
        {
            let mut upd =
                tx.prepare("UPDATE usage_event SET cost_usd = ?2, cost_basis = ?3 WHERE id = ?1")?;
            for (id, cost) in &changes {
                let basis = if cost.is_some() {
                    CostBasis::Computed
                } else {
                    CostBasis::Unpriced
                };
                upd.execute(params![id, cost, basis.as_str()])?;
            }
        }
        tx.commit()?;
        Ok(changes.len() as u64)
    }

    pub fn history_floor(&self) -> Result<Option<DateTime<Utc>>> {
        let floor = self.rollup_floor_ms()?;
        Ok((floor != i64::MIN).then(|| from_ms(floor)))
    }

    pub fn prune(&mut self, retention_days: u32, now: DateTime<Utc>) -> Result<PruneReport> {
        self.prune_with_tz(retention_days, now, Tz::Fixed(Utc.fix()))
    }

    pub fn prune_with_tz(
        &mut self,
        retention_days: u32,
        now: DateTime<Utc>,
        tz: Tz,
    ) -> Result<PruneReport> {
        let cutoff_day = (now - Duration::days(retention_days as i64)).date_naive();
        let cutoff =
            ms(Utc.from_utc_datetime(&cutoff_day.and_hms_opt(0, 0, 0).unwrap_or_default()));
        let floor = self.rollup_floor_ms()?.max(cutoff);
        let offset = tz.offset_min(now);
        let tx = self.conn.transaction()?;
        let mut folded: BTreeMap<(String, String, String, String, String), Fold> = BTreeMap::new();
        let mut events_folded = 0u64;
        {
            let mut stmt = tx.prepare(
                "SELECT ts, tool, model, account, COALESCE(project_hash, ''), COALESCE(project_name, ''),
                        input, output, cache_read, cache_write_5m, cache_write_1h, reasoning,
                        cost_usd, cost_basis
                 FROM usage_event WHERE ts < ?1",
            )?;
            let mut rows = stmt.query([floor])?;
            while let Some(r) = rows.next()? {
                events_folded += 1;
                let day = tz.day(from_ms(r.get(0)?)).format("%Y-%m-%d").to_string();
                let key = (day, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?);
                let f = folded.entry(key).or_default();
                let name: String = r.get(5)?;
                if f.name.is_empty() {
                    f.name = name;
                }
                f.tokens.add(&crate::model::Tokens {
                    input: r.get::<_, i64>(6)? as u64,
                    output: r.get::<_, i64>(7)? as u64,
                    cache_read: r.get::<_, i64>(8)? as u64,
                    cache_write_5m: r.get::<_, i64>(9)? as u64,
                    cache_write_1h: r.get::<_, i64>(10)? as u64,
                    reasoning: r.get::<_, i64>(11)? as u64,
                });
                f.cost += r.get::<_, Option<f64>>(12)?.unwrap_or(0.0);
                f.events += 1;
                f.unpriced += u64::from(r.get::<_, String>(13)? == "unpriced");
            }
        }
        {
            let mut ins = tx.prepare(
                "INSERT INTO usage_day_rollup
                   (day, tool, model, account, project_hash, project_name, input, output, cache_read,
                    cache_write_5m, cache_write_1h, reasoning, cost_usd, events, unpriced_events,
                    tz_offset_min)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
                 ON CONFLICT(day, tool, model, account, project_hash) DO UPDATE SET
                   input = input + excluded.input,
                   output = output + excluded.output,
                   cache_read = cache_read + excluded.cache_read,
                   cache_write_5m = cache_write_5m + excluded.cache_write_5m,
                   cache_write_1h = cache_write_1h + excluded.cache_write_1h,
                   reasoning = reasoning + excluded.reasoning,
                   cost_usd = cost_usd + excluded.cost_usd,
                   events = events + excluded.events,
                   unpriced_events = unpriced_events + excluded.unpriced_events",
            )?;
            for ((day, tool, model, account, project), f) in &folded {
                ins.execute(params![
                    day,
                    tool,
                    model,
                    account,
                    project,
                    f.name,
                    f.tokens.input as i64,
                    f.tokens.output as i64,
                    f.tokens.cache_read as i64,
                    f.tokens.cache_write_5m as i64,
                    f.tokens.cache_write_1h as i64,
                    f.tokens.reasoning as i64,
                    f.cost,
                    f.events as i64,
                    f.unpriced as i64,
                    offset,
                ])?;
            }
        }
        tx.execute("DELETE FROM usage_event WHERE ts < ?1", [floor])?;
        tx.execute("DELETE FROM usage_fact WHERE ts < ?1", [floor])?;
        tx.execute(
            "DELETE FROM usage_session WHERE parent_session_id IS NULL
               AND id NOT IN (SELECT DISTINCT session_id FROM usage_event)",
            [],
        )?;
        tx.execute(
            "DELETE FROM usage_agent WHERE NOT EXISTS (
               SELECT 1 FROM usage_fact f WHERE f.tool = usage_agent.tool
                 AND f.account = usage_agent.account AND f.tool_session_id = usage_agent.tool_session_id)
             AND NOT EXISTS (
               SELECT 1 FROM usage_session s WHERE s.tool = usage_agent.tool
                 AND s.account = usage_agent.account AND s.tool_session_id = usage_agent.tool_session_id)",
            [],
        )?;
        let old_day = (now - Duration::days(ROLLUP_KEEP_DAYS))
            .format("%Y-%m-%d")
            .to_string();
        let dropped = tx.execute("DELETE FROM usage_day_rollup WHERE day < ?1", [old_day])?;
        tx.execute(
            "DELETE FROM usage_limit_snapshot WHERE observed_at < ?1",
            [ms(now - Duration::days(LIMIT_KEEP_DAYS))],
        )?;
        tx.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![ROLLUP_FLOOR_KEY, floor.to_string()],
        )?;
        tx.commit()?;
        Ok(PruneReport {
            events_folded,
            rollup_rows_dropped: dropped as u64,
        })
    }

    pub fn delete_all(&mut self) -> Result<()> {
        let tx = self.conn.transaction()?;
        for table in [
            "usage_event",
            "usage_session",
            "usage_file_state",
            "usage_source",
            "usage_day_rollup",
            "usage_limit_snapshot",
            "usage_fact",
            "usage_agent",
        ] {
            tx.execute(&format!("DELETE FROM {table}"), [])?;
        }
        tx.execute("DELETE FROM settings WHERE key = ?1", [ROLLUP_FLOOR_KEY])?;
        tx.commit()?;
        self.conn
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        self.conn.execute_batch("VACUUM")?;
        Ok(())
    }
}

pub(crate) fn record_limit_conn(conn: &Connection, snap: &LimitSnapshot) -> Result<()> {
    conn.execute(
        "INSERT INTO usage_limit_snapshot
           (tool, account, tool_session_id, window, used_pct, resets_at, plan, observed_at, source)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
         ON CONFLICT(tool, account, tool_session_id, window) DO UPDATE SET
           used_pct = excluded.used_pct, resets_at = excluded.resets_at, plan = excluded.plan,
           observed_at = excluded.observed_at, source = excluded.source
         WHERE excluded.observed_at >= usage_limit_snapshot.observed_at",
        params![
            snap.tool.as_str(),
            snap.account,
            snap.tool_session_id,
            snap.window.as_key(),
            snap.used_pct,
            snap.resets_at.map(ms),
            snap.plan,
            ms(snap.observed_at),
            snap.source.as_str(),
        ],
    )?;
    Ok(())
}

fn current_version(conn: &Connection) -> Result<i64> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS schema_version (version INTEGER NOT NULL)")?;
    let v: Option<i64> = conn
        .query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))
        .optional()?
        .flatten();
    Ok(v.unwrap_or(0))
}

pub(crate) fn migrate(conn: &Connection, target: usize) -> Result<()> {
    let found = current_version(conn)?;
    if found > MIGRATIONS.len() as i64 {
        return Err(UsageError::SchemaTooNew {
            found,
            supported: MIGRATIONS.len() as i64,
        });
    }
    for (idx, sql) in MIGRATIONS.iter().enumerate().take(target) {
        let version = idx as i64 + 1;
        if version <= found {
            continue;
        }
        conn.execute_batch(&format!(
            "BEGIN IMMEDIATE;
             {sql}
             INSERT INTO schema_version (version) VALUES ({version});
             COMMIT;"
        ))?;
    }
    Ok(())
}

fn context_version(conn: &Connection) -> Result<i64> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS usage_context_version (version INTEGER NOT NULL)",
    )?;
    let v: Option<i64> = conn
        .query_row("SELECT MAX(version) FROM usage_context_version", [], |r| {
            r.get(0)
        })
        .optional()?
        .flatten();
    Ok(v.unwrap_or(0))
}

fn migrate_context(conn: &Connection) -> Result<()> {
    let found = context_version(conn)?;
    let min_reader: i64 = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            [CONTEXT_MIN_READER_KEY],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    if min_reader > CONTEXT_MIGRATIONS.len() as i64 {
        return Err(UsageError::SchemaTooNew {
            found: min_reader,
            supported: CONTEXT_MIGRATIONS.len() as i64,
        });
    }
    for (idx, sql) in CONTEXT_MIGRATIONS.iter().enumerate() {
        let version = idx as i64 + 1;
        if version <= found {
            continue;
        }
        conn.execute_batch(&format!(
            "BEGIN IMMEDIATE;
             {sql}
             INSERT INTO usage_context_version (version) VALUES ({version});
             COMMIT;"
        ))?;
    }
    if min_reader < CONTEXT_MIN_READER {
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [CONTEXT_MIN_READER_KEY, &CONTEXT_MIN_READER.to_string()],
        )?;
    }
    Ok(())
}

fn create_private_file(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    opts.open(path)?;
    Ok(())
}

/// Every Claude Code session reports its own statusline copy of an account's limits, and an idle
/// one repeats what it saw last. Within a window usage only grows, so the window that resets last is
/// the current one and its highest reading is the freshest. Rollout readings carry the time of the
/// API response, so the newest of them wins.
fn fresher(candidate: &LimitSnapshot, best: &LimitSnapshot) -> bool {
    let statusline = |s: &LimitSnapshot| s.source == LimitSource::Statusline;
    match (candidate.resets_at, best.resets_at) {
        (Some(c), Some(b)) if statusline(candidate) && statusline(best) => {
            if (c - b).num_seconds().abs() > SAME_WINDOW_S {
                c > b
            } else {
                candidate.used_pct > best.used_pct
            }
        }
        _ => candidate.observed_at > best.observed_at,
    }
}
