use crate::error::Result;
use crate::store::Store;
use chrono::{DateTime, SecondsFormat, Utc};
use rusqlite::{params, OptionalExtension};

pub const CONSENT_KEY: &str = "sync_consent_at";
pub const AUTO_KEY: &str = "sync_auto_at";
pub const LAST_RUN_KEY: &str = "sync_last_run";

#[derive(Debug, Clone, PartialEq)]
pub struct LedgerEntry {
    pub workspace: String,
    pub task_ref: String,
    pub task_id: Option<u64>,
    pub generated_at: String,
    pub events: u64,
    pub cost_usd: f64,
    pub digest: String,
    pub first_event_at: Option<String>,
    pub last_event_at: Option<String>,
}

impl LedgerEntry {
    pub fn key(&self) -> String {
        ledger_key(&self.workspace, &self.task_ref)
    }
}

pub fn ledger_key(workspace: &str, task_ref: &str) -> String {
    format!("{workspace}/{}", task_ref.to_ascii_uppercase())
}

impl Store {
    pub fn sync_consent(&self) -> Result<Option<DateTime<Utc>>> {
        Ok(self
            .setting(CONSENT_KEY)?
            .and_then(|v| DateTime::parse_from_rfc3339(&v).ok())
            .map(|t| t.with_timezone(&Utc)))
    }

    pub fn set_sync_consent(&self, at: DateTime<Utc>) -> Result<()> {
        self.set_setting(CONSENT_KEY, &at.to_rfc3339_opts(SecondsFormat::Secs, true))
    }

    pub fn clear_sync_consent(&self) -> Result<()> {
        self.conn.execute(
            "DELETE FROM settings WHERE key IN (?1, ?2)",
            [CONSENT_KEY, AUTO_KEY],
        )?;
        Ok(())
    }

    pub fn sync_auto(&self) -> Result<Option<DateTime<Utc>>> {
        Ok(self
            .setting(AUTO_KEY)?
            .and_then(|v| DateTime::parse_from_rfc3339(&v).ok())
            .map(|t| t.with_timezone(&Utc)))
    }

    pub fn set_sync_auto(&self, at: DateTime<Utc>) -> Result<()> {
        self.set_setting(AUTO_KEY, &at.to_rfc3339_opts(SecondsFormat::Secs, true))
    }

    pub fn clear_sync_auto(&self) -> Result<()> {
        self.conn
            .execute("DELETE FROM settings WHERE key = ?1", [AUTO_KEY])?;
        Ok(())
    }

    pub fn sync_auto_active(&self) -> Result<bool> {
        Ok(self.sync_consent()?.is_some() && self.sync_auto()?.is_some())
    }

    pub fn sync_last_run(&self) -> Result<Option<String>> {
        self.setting(LAST_RUN_KEY)
    }

    pub fn set_sync_last_run(&self, json: &str) -> Result<()> {
        self.set_setting(LAST_RUN_KEY, json)
    }

    pub fn ledger_get(&self, workspace: &str, task_ref: &str) -> Result<Option<LedgerEntry>> {
        Ok(self
            .conn
            .query_row(
                "SELECT workspace, task_ref, task_id, generated_at, events, cost_usd, digest,
                        first_event_at, last_event_at
                 FROM usage_sync_ledger WHERE key = ?1",
                [ledger_key(workspace, task_ref)],
                |r| {
                    Ok(LedgerEntry {
                        workspace: r.get(0)?,
                        task_ref: r.get(1)?,
                        task_id: r.get::<_, Option<i64>>(2)?.map(|v| v as u64),
                        generated_at: r.get(3)?,
                        events: r.get::<_, i64>(4)? as u64,
                        cost_usd: r.get(5)?,
                        digest: r.get(6)?,
                        first_event_at: r.get(7)?,
                        last_event_at: r.get(8)?,
                    })
                },
            )
            .optional()?)
    }

    pub fn ledger_put(&self, e: &LedgerEntry) -> Result<()> {
        self.conn.execute(
            "INSERT INTO usage_sync_ledger
               (key, workspace, task_ref, task_id, generated_at, events, cost_usd, digest,
                first_event_at, last_event_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(key) DO UPDATE SET
               workspace = excluded.workspace, task_ref = excluded.task_ref,
               task_id = excluded.task_id, generated_at = excluded.generated_at,
               events = excluded.events, cost_usd = excluded.cost_usd, digest = excluded.digest,
               first_event_at = excluded.first_event_at, last_event_at = excluded.last_event_at",
            params![
                e.key(),
                e.workspace,
                e.task_ref,
                e.task_id.map(|v| v as i64),
                e.generated_at,
                e.events as i64,
                e.cost_usd,
                e.digest,
                e.first_event_at,
                e.last_event_at,
            ],
        )?;
        Ok(())
    }

    pub fn ledger_clear(&self) -> Result<()> {
        self.conn.execute("DELETE FROM usage_sync_ledger", [])?;
        Ok(())
    }

    pub fn ledger_forget_task(&self, task_ref: Option<&str>, task_id: Option<u64>) -> Result<()> {
        self.conn.execute(
            "DELETE FROM usage_sync_ledger WHERE upper(task_ref) = upper(?1) OR task_id = ?2",
            params![task_ref.unwrap_or(""), task_id.map(|v| v as i64)],
        )?;
        Ok(())
    }

    pub fn ledger_count(&self) -> Result<u64> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM usage_sync_ledger", [], |r| {
                r.get::<_, i64>(0)
            })? as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(ws: &str, r: &str, events: u64) -> LedgerEntry {
        LedgerEntry {
            workspace: ws.into(),
            task_ref: r.into(),
            task_id: Some(7),
            generated_at: "2026-10-16T12:00:00Z".into(),
            events,
            cost_usd: 1.5,
            digest: "d".into(),
            first_event_at: None,
            last_event_at: None,
        }
    }

    #[test]
    fn consent_round_trips_and_clears() {
        let s = Store::open_in_memory().unwrap();
        assert!(s.sync_consent().unwrap().is_none());
        let at: DateTime<Utc> = "2026-10-16T12:00:00Z".parse().unwrap();
        s.set_sync_consent(at).unwrap();
        assert_eq!(s.sync_consent().unwrap(), Some(at));
        s.clear_sync_consent().unwrap();
        assert!(s.sync_consent().unwrap().is_none());
    }

    #[test]
    fn auto_needs_consent_and_goes_away_with_it() {
        let s = Store::open_in_memory().unwrap();
        let at: DateTime<Utc> = "2026-10-16T12:00:00Z".parse().unwrap();
        s.set_sync_auto(at).unwrap();
        assert_eq!(s.sync_auto().unwrap(), Some(at));
        assert!(!s.sync_auto_active().unwrap());
        s.set_sync_consent(at).unwrap();
        assert!(s.sync_auto_active().unwrap());
        s.clear_sync_auto().unwrap();
        assert!(s.sync_consent().unwrap().is_some());
        assert!(!s.sync_auto_active().unwrap());
        s.set_sync_auto(at).unwrap();
        s.clear_sync_consent().unwrap();
        assert!(s.sync_auto().unwrap().is_none());
    }

    #[test]
    fn ledger_upserts_per_task_and_forgets() {
        let s = Store::open_in_memory().unwrap();
        s.ledger_put(&entry("ws", "TM-1", 3)).unwrap();
        s.ledger_put(&entry("ws", "tm-1", 5)).unwrap();
        s.ledger_put(&entry("ws", "TM-2", 1)).unwrap();
        assert_eq!(s.ledger_count().unwrap(), 2);
        assert_eq!(s.ledger_get("ws", "TM-1").unwrap().unwrap().events, 5);
        s.ledger_forget_task(Some("TM-1"), None).unwrap();
        assert!(s.ledger_get("ws", "TM-1").unwrap().is_none());
        s.ledger_clear().unwrap();
        assert_eq!(s.ledger_count().unwrap(), 0);
    }

    #[test]
    fn delete_all_clears_consent_ledger_and_last_run() {
        let mut s = Store::open_in_memory().unwrap();
        s.set_sync_consent(Utc::now()).unwrap();
        s.set_sync_auto(Utc::now()).unwrap();
        s.set_sync_last_run("{}").unwrap();
        s.ledger_put(&entry("ws", "TM-1", 3)).unwrap();
        s.delete_all().unwrap();
        assert!(s.sync_consent().unwrap().is_none());
        assert!(s.sync_auto().unwrap().is_none());
        assert!(s.sync_last_run().unwrap().is_none());
        assert_eq!(s.ledger_count().unwrap(), 0);
    }
}
