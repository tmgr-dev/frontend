use std::path::Path;

use rusqlite::types::{Value as Sql, ValueRef};
use rusqlite::{Connection, OpenFlags};
use serde_json::{Map, Value};

use crate::error::{Error, Result};

pub const SCHEMA_VERSION: u32 = 12;

pub(crate) type Row = Map<String, Value>;

/// A read-only handle next to a running desktop: same pragmas, plus `query_only`, and never a migration.
pub fn open_read(database: &Path) -> Result<Connection> {
    if !database.is_file() {
        return Err(Error::NotFound(format!("database {}", database.display())));
    }
    let conn = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.execute_batch(
        "PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000; PRAGMA query_only = ON;",
    )?;
    check_schema(&conn)?;
    Ok(conn)
}

/// A write handle with the desktop's pragmas. Every write request runs in one `BEGIN IMMEDIATE` transaction.
pub fn open_write(database: &Path) -> Result<Connection> {
    open_write_with(database, 5000)
}

pub(crate) fn open_write_with(database: &Path, busy_ms: u32) -> Result<Connection> {
    if !database.is_file() {
        return Err(Error::NotFound(format!("database {}", database.display())));
    }
    let conn = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.execute_batch(&format!(
        "PRAGMA foreign_keys = ON; PRAGMA busy_timeout = {busy_ms};"
    ))?;
    check_schema(&conn)?;
    let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
    Ok(conn)
}

pub(crate) struct Executed {
    pub changes: usize,
    pub last_id: i64,
}

pub(crate) fn execute(conn: &Connection, sql: &str, params: &[Sql]) -> Result<Executed> {
    let mut stmt = conn.prepare_cached(sql)?;
    let changes = stmt.execute(rusqlite::params_from_iter(params.iter()))?;
    Ok(Executed {
        changes,
        last_id: conn.last_insert_rowid(),
    })
}

pub(crate) fn schema_version(conn: &Connection) -> Result<u32> {
    let has_meta: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'meta'",
        [],
        |r| r.get(0),
    )?;
    if has_meta == 0 {
        return Ok(0);
    }
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| r.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            e => Err(e),
        })?;
    Ok(raw.and_then(|v| v.trim().parse().ok()).unwrap_or(0))
}

pub(crate) fn check_schema(conn: &Connection) -> Result<()> {
    match schema_version(conn)? {
        SCHEMA_VERSION => Ok(()),
        v if v > SCHEMA_VERSION => Err(Error::SchemaTooNew(v)),
        v => Err(Error::SchemaTooOld(v)),
    }
}

fn to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::Number(i.into()),
        ValueRef::Real(f) => crate::js::float_json(f),
        ValueRef::Text(t) | ValueRef::Blob(t) => {
            Value::String(String::from_utf8_lossy(t).into_owned())
        }
    }
}

pub(crate) fn select(conn: &Connection, sql: &str, params: &[Sql]) -> Result<Vec<Row>> {
    let mut stmt = conn.prepare_cached(sql)?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let mut rows = stmt.query(rusqlite::params_from_iter(params.iter()))?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let mut map = Map::new();
        for (i, name) in names.iter().enumerate() {
            map.insert(name.clone(), to_json(row.get_ref(i)?));
        }
        out.push(map);
    }
    Ok(out)
}

pub(crate) fn select_one(conn: &Connection, sql: &str, params: &[Sql]) -> Result<Option<Row>> {
    Ok(select(conn, sql, params)?.into_iter().next())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file_db(sql: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(sql).unwrap();
        (dir, path)
    }

    #[test]
    fn opens_the_current_schema_read_only() {
        let (_dir, path) = file_db(&crate::testing::schema());
        let conn = open_read(&path).unwrap();
        let err = conn
            .execute("DELETE FROM statuses", [])
            .map_err(Error::from)
            .unwrap_err();
        assert!(matches!(err, Error::Sql(_)), "{err:?}");
    }

    #[test]
    fn refuses_newer_and_older_schemas() {
        let (_d1, newer) = file_db(
            "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT); INSERT INTO meta VALUES ('schema_version', '13');",
        );
        assert_eq!(open_read(&newer).unwrap_err(), Error::SchemaTooNew(13));
        let (_d2, older) = file_db(
            "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT); INSERT INTO meta VALUES ('schema_version', '11');",
        );
        assert_eq!(open_read(&older).unwrap_err(), Error::SchemaTooOld(11));
        let (_d3, empty) = file_db("CREATE TABLE x (a)");
        assert_eq!(open_read(&empty).unwrap_err(), Error::SchemaTooOld(0));
    }

    #[test]
    fn a_write_waits_for_a_desktop_transaction_then_reports_busy_without_writing() {
        let (_dir, path) = file_db(&crate::testing::schema());
        let desktop = open_write(&path).unwrap();
        desktop
            .execute_batch("BEGIN IMMEDIATE; INSERT INTO meta VALUES ('desktop', '1');")
            .unwrap();
        let cli = open_write_with(&path, 50).unwrap();
        let ctx = crate::testing::ctx("2026-10-13T12:00:00.000Z");
        let err = crate::dispatch(&cli, &ctx, "POST", "tasks/1/countdown", None).unwrap_err();
        assert_eq!(err, Error::Busy);
        desktop.execute_batch("COMMIT").unwrap();
        let journal: String = cli
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(journal, "wal");
        assert!(cli.is_autocommit());
    }

    #[test]
    fn a_refused_database_is_left_untouched() {
        let (_dir, path) = file_db(
            "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT); INSERT INTO meta VALUES ('schema_version', '13');",
        );
        assert_eq!(open_write(&path).unwrap_err(), Error::SchemaTooNew(13));
        let mode: String = Connection::open(&path)
            .unwrap()
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "delete");
    }

    #[test]
    fn a_missing_file_is_not_created() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.db");
        assert!(matches!(open_read(&path), Err(Error::NotFound(_))));
        assert!(!path.exists());
    }
}
