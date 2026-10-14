use chrono::{DateTime, Duration, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;

use crate::error::{Result, UsageError};
use crate::limits::{LimitSnapshot, LimitSource, LimitWindow};
use crate::model::{CostBasis, Tool};
use crate::pricing::{PriceBook, PriceEntry};
use crate::schema::MIGRATIONS;

const ROLLUP_FLOOR_KEY: &str = "rollup_floor_ms";
const ROLLUP_KEEP_DAYS: i64 = 730;
const LIMIT_KEEP_DAYS: i64 = 30;

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
        conn.busy_timeout(std::time::Duration::from_millis(5000))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&conn, MIGRATIONS.len())?;
        Ok(Store { conn })
    }

    pub fn schema_version(&self) -> Result<i64> {
        current_version(&self.conn)
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
            if out
                .iter()
                .any(|s| s.tool == tool && s.account == account && s.window == window)
            {
                continue;
            }
            out.push(LimitSnapshot {
                tool,
                account,
                window,
                used_pct: used,
                resets_at: resets.map(from_ms),
                plan,
                observed_at: from_ms(observed),
                source,
                tool_session_id: session,
            });
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
                "SELECT id, model, input, output, cache_read, cache_write_5m, cache_write_1h, service_tier, cost_usd, cost_basis
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
                ))
            })?;
            let mut changes = Vec::new();
            for row in rows {
                let (id, model, tokens, tier, old) = row?;
                let new = book.cost(&model, &tokens, tier.as_deref());
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

    pub fn prune(&mut self, retention_days: u32, now: DateTime<Utc>) -> Result<PruneReport> {
        let cutoff_day = (now - Duration::days(retention_days as i64)).date_naive();
        let cutoff =
            ms(Utc.from_utc_datetime(&cutoff_day.and_hms_opt(0, 0, 0).unwrap_or_default()));
        let floor = self.rollup_floor_ms()?.max(cutoff);
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO usage_day_rollup
               (day, tool, model, account, project_hash, project_name, input, output, cache_read,
                cache_write_5m, cache_write_1h, reasoning, cost_usd, events, unpriced_events)
             SELECT strftime('%Y-%m-%d', ts / 1000, 'unixepoch'), tool, model, account,
                    COALESCE(project_hash, ''), COALESCE(MAX(project_name), ''),
                    SUM(input), SUM(output), SUM(cache_read), SUM(cache_write_5m), SUM(cache_write_1h),
                    SUM(reasoning), COALESCE(SUM(cost_usd), 0), COUNT(*),
                    SUM(cost_basis = 'unpriced')
             FROM usage_event WHERE ts < ?1
             GROUP BY 1, tool, model, account, COALESCE(project_hash, '')
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
            [floor],
        )?;
        let events_folded: i64 = tx.query_row(
            "SELECT COUNT(*) FROM usage_event WHERE ts < ?1",
            [floor],
            |r| r.get(0),
        )?;
        tx.execute("DELETE FROM usage_event WHERE ts < ?1", [floor])?;
        tx.execute(
            "DELETE FROM usage_session WHERE id NOT IN (SELECT DISTINCT session_id FROM usage_event)",
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
            events_folded: events_folded as u64,
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
