use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::Connection;
use serde_json::{json, Value};
use tmgr_local::{
    confirm_task, dispatch, provenance, verify_chain_full, Actor, Context, LocalWorkspace,
    Manifest, User, WriteOrigin,
};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/journal")
}

fn schema() -> String {
    let golden: Value = serde_json::from_str(
        &fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parity/golden/schema.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut sql = String::new();
    for object in golden["sqlite_master"].as_array().unwrap() {
        if let Some(ddl) = object["sql"].as_str() {
            sql.push_str(ddl);
            sql.push_str(";\n");
        }
    }
    sql.push_str("INSERT INTO meta (key, value) VALUES ('schema_version', '13');\n");
    for s in golden["statuses"].as_array().unwrap() {
        sql.push_str(&format!(
            "INSERT INTO statuses (id, name, type, color, sort_order, created_at, updated_at) VALUES ({}, '{}', '{}', '{}', {}, 'x', 'x');\n",
            s["id"],
            s["name"].as_str().unwrap().replace('\'', "''"),
            s["type"].as_str().unwrap(),
            s["color"].as_str().unwrap(),
            s["sort_order"]
        ));
    }
    sql
}

fn database(seed: &Value) -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
    conn.execute_batch(&schema()).unwrap();
    for statement in seed.as_array().unwrap() {
        conn.execute_batch(statement.as_str().unwrap()).unwrap();
    }
    conn
}

fn context(op: &Value) -> Context {
    let actor = op.get("actor").and_then(|a| {
        let text = |k: &str| a.get(k).and_then(Value::as_str).map(str::to_string);
        Some(Actor {
            kind: text("kind")?,
            id: text("id")?,
            name: text("name").unwrap_or_default(),
            owner_id: text("ownerId"),
        })
    });
    let origin = match op.get("origin").and_then(Value::as_str) {
        Some("cli") => WriteOrigin::Cli,
        _ => WriteOrigin::DesktopUi,
    };
    Context {
        workspace: LocalWorkspace {
            manifest: Manifest {
                id: -1,
                name: "Fixture".into(),
                code: "fixture".into(),
                schema_version: 13,
                created_at: String::new(),
            },
            path: "/nonexistent".into(),
            database: "/nonexistent/workspace.db".into(),
        },
        user: User {
            id: 7,
            name: "Owner".into(),
            email: "owner@example.com".into(),
        },
        now: DateTime::parse_from_rfc3339(op["at"].as_str().unwrap())
            .unwrap()
            .with_timezone(&Utc),
        local_offset: chrono::FixedOffset::east_opt(0).unwrap(),
        actor,
        origin,
    }
}

fn journal(conn: &Connection) -> Vec<Value> {
    let mut stmt = conn
        .prepare("SELECT seq, at, entity, entity_id, task_id, field, author_kind, source, actor_id, content_sha256, prev_hash, hash FROM edit_journal ORDER BY seq")
        .unwrap();
    let names: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
    stmt.query_map([], |r| {
        let mut row = serde_json::Map::new();
        for (i, name) in names.iter().enumerate() {
            row.insert(
                name.clone(),
                match r.get_ref(i).unwrap() {
                    rusqlite::types::ValueRef::Integer(n) => json!(n),
                    rusqlite::types::ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                    _ => Value::Null,
                },
            );
        }
        Ok(Value::Object(row))
    })
    .unwrap()
    .map(Result::unwrap)
    .collect()
}

fn run(fixture: &mut Value, update: bool) -> Value {
    let conn = database(&fixture["seed"]);
    let ops = fixture["ops"].as_array().cloned().unwrap();
    let mut updated_ops = Vec::new();
    for mut op in ops {
        let before = journal(&conn).len();
        let ctx = context(&op);
        if let Some(task) = op.get("confirm").and_then(Value::as_i64) {
            confirm_task(&conn, &ctx, task, None).unwrap();
            if op.get("writer").and_then(Value::as_str) == Some("rust") {
                let appended: Vec<Value> = journal(&conn).split_off(before);
                if update {
                    op["rows"] = Value::Array(appended);
                } else {
                    assert_eq!(Value::Array(appended), op["rows"], "rows of confirm {task}");
                }
            }
            updated_ops.push(op);
            continue;
        }
        let answer = dispatch(
            &conn,
            &ctx,
            op["method"].as_str().unwrap(),
            op["url"].as_str().unwrap(),
            op.get("body"),
        )
        .unwrap()
        .unwrap_or_else(|| panic!("no route for {}", op["url"]));
        let wanted = op.get("status").and_then(Value::as_u64).unwrap_or(0);
        if wanted != 0 {
            assert_eq!(answer.status as u64, wanted, "{}", op["url"]);
        } else {
            assert!(
                answer.status < 400,
                "{} answered {}",
                op["url"],
                answer.status
            );
        }
        if op.get("writer").and_then(Value::as_str) == Some("rust") {
            let appended: Vec<Value> = journal(&conn).split_off(before);
            if update {
                op["rows"] = Value::Array(appended);
            } else {
                assert_eq!(Value::Array(appended), op["rows"], "rows of {}", op["url"]);
            }
        }
        updated_ops.push(op);
    }
    fixture["ops"] = Value::Array(updated_ops);
    let mut provenance_map = serde_json::Map::new();
    for id in fixture["provenance_for"].as_array().unwrap() {
        let id = id.as_i64().unwrap();
        provenance_map.insert(
            id.to_string(),
            serde_json::to_value(provenance(&conn, id).unwrap()).unwrap(),
        );
    }
    let chain = verify_chain_full(&conn).unwrap();
    json!({
        "journal": journal(&conn),
        "chain": { "ok": chain.ok, "broken_at": chain.broken_at, "rows": chain.rows },
        "provenance": provenance_map,
    })
}

#[test]
fn the_fixtures_produce_the_recorded_chains() {
    let update = std::env::var("UPDATE_JOURNAL_FIXTURES").as_deref() == Ok("1");
    let mut seen = 0;
    let mut files: Vec<PathBuf> = fs::read_dir(fixtures())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    files.sort();
    for file in files {
        let mut fixture: Value = serde_json::from_str(&fs::read_to_string(&file).unwrap()).unwrap();
        let actual = run(&mut fixture, update);
        if update {
            fixture["expect"] = actual;
            let mut out = Vec::new();
            let formatter = serde_json::ser::PrettyFormatter::with_indent(b"\t");
            let mut serializer = serde_json::Serializer::with_formatter(&mut out, formatter);
            serde::Serialize::serialize(&fixture, &mut serializer).unwrap();
            out.push(b'\n');
            fs::write(&file, out).unwrap();
        } else {
            assert_eq!(actual, fixture["expect"], "{}", file.display());
            assert_eq!(actual["chain"]["ok"], json!(true));
        }
        seen += 1;
    }
    assert!(seen >= 4);
}
