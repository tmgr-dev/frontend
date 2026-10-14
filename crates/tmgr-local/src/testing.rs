use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde_json::Value;

use crate::context::{Context, User};
use crate::workspace::{LocalWorkspace, Manifest};

const SCHEMA_GOLDEN: &str = include_str!("../tests/parity/golden/schema.json");

/// The v13 schema and default statuses from the parity schema golden.
pub(crate) fn schema() -> String {
    build_schema(13)
}

/// The same database as the desktop left it before the edit journal existed.
pub(crate) fn schema12() -> String {
    build_schema(12)
}

fn build_schema(version: u32) -> String {
    let golden: Value = serde_json::from_str(SCHEMA_GOLDEN).unwrap();
    let mut sql = String::new();
    for object in golden["sqlite_master"].as_array().unwrap() {
        if version < 13 && object["tbl_name"] == "edit_journal" {
            continue;
        }
        if let Some(ddl) = object["sql"].as_str() {
            sql.push_str(ddl);
            sql.push_str(";\n");
        }
    }
    sql.push_str(&format!(
        "INSERT INTO meta (key, value) VALUES ('schema_version', '{version}');\n"
    ));
    for s in golden["statuses"].as_array().unwrap() {
        sql.push_str(&format!(
            "INSERT INTO statuses (id, name, type, color, sort_order, created_at, updated_at) VALUES ({}, {}, {}, {}, {}, 'x', 'x');\n",
            s["id"], quote(&s["name"]), quote(&s["type"]), quote(&s["color"]), s["sort_order"]
        ));
    }
    sql
}

fn quote(v: &Value) -> String {
    format!("'{}'", v.as_str().unwrap().replace('\'', "''"))
}

pub(crate) fn db12(seed: &str) -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(&schema12()).unwrap();
    conn.execute_batch(seed).unwrap();
    conn.execute_batch("PRAGMA query_only = ON").unwrap();
    conn
}

pub(crate) fn db(seed: &str) -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(&schema()).unwrap();
    conn.execute_batch(seed).unwrap();
    conn.execute_batch("PRAGMA query_only = ON").unwrap();
    conn
}

pub(crate) fn ctx(now: &str) -> Context {
    Context {
        workspace: LocalWorkspace {
            manifest: Manifest {
                id: -42,
                name: "Test".into(),
                code: "test".into(),
                schema_version: 12,
                created_at: "2026-01-01T00:00:00Z".into(),
            },
            path: "/nonexistent/test".into(),
            database: "/nonexistent/test/workspace.db".into(),
        },
        user: User {
            id: 1,
            name: "Owner".into(),
            email: "owner@example.com".into(),
        },
        now: DateTime::parse_from_rfc3339(now)
            .unwrap()
            .with_timezone(&Utc),
        local_offset: chrono::FixedOffset::east_opt(0).unwrap(),
        actor: None,
        origin: crate::journal::WriteOrigin::default(),
    }
}

pub(crate) fn get(conn: &Connection, ctx: &Context, url: &str) -> (u16, Value) {
    let r = crate::dispatch(conn, ctx, "GET", url, None)
        .unwrap()
        .unwrap_or_else(|| panic!("no route for {url}"));
    (r.status, r.data)
}
