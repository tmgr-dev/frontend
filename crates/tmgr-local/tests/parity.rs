use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::types::{Value as Sql, ValueRef};
use rusqlite::Connection;
use serde_json::{json, Value};
use tmgr_local::{dispatch, Context, LocalWorkspace, Manifest, User};

fn dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/parity")
}

fn read(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap())
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn sql_value(v: &Value) -> Sql {
    match v {
        Value::Null => Sql::Null,
        Value::Bool(b) => Sql::Integer(*b as i64),
        Value::Number(n) => n
            .as_i64()
            .map(Sql::Integer)
            .unwrap_or_else(|| Sql::Real(n.as_f64().unwrap_or(0.0))),
        Value::String(s) => Sql::Text(s.clone()),
        other => Sql::Text(other.to_string()),
    }
}

fn insert(conn: &Connection, table: &str, row: &Value) {
    let row = row.as_object().unwrap();
    let cols: Vec<&str> = row.keys().map(String::as_str).collect();
    let marks = vec!["?"; cols.len()].join(", ");
    let params: Vec<Sql> = row.values().map(sql_value).collect();
    conn.execute(
        &format!("INSERT INTO {table} ({}) VALUES ({marks})", cols.join(", ")),
        rusqlite::params_from_iter(params),
    )
    .unwrap();
}

/// The v12 database the TS `migrate` builds, from the schema golden: DDL in rowid order, then its rows.
fn migrated(schema: &Value, now: &str) -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
    for object in schema["sqlite_master"].as_array().unwrap() {
        if let Some(ddl) = object["sql"].as_str() {
            conn.execute_batch(ddl)
                .unwrap_or_else(|e| panic!("{ddl}: {e}"));
        }
    }
    for row in schema["meta"].as_array().unwrap() {
        insert(&conn, "meta", row);
    }
    for row in schema["statuses"].as_array().unwrap() {
        let mut row = row.clone();
        row["created_at"] = json!(now);
        row["updated_at"] = json!(now);
        insert(&conn, "statuses", &row);
    }
    for row in schema["task_relation_types"].as_array().unwrap() {
        insert(&conn, "task_relation_types", row);
    }
    conn
}

fn dump(conn: &Connection) -> Value {
    let mut stmt = conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE 'pages_fts%' ORDER BY name",
        )
        .unwrap();
    let names: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut out = serde_json::Map::new();
    for name in names {
        let mut stmt = conn
            .prepare(&format!("SELECT * FROM \"{name}\" ORDER BY rowid"))
            .unwrap();
        let width = stmt.column_count();
        let rows: Vec<Value> = stmt
            .query_map([], |r| {
                Ok(Value::Array(
                    (0..width)
                        .map(|i| match r.get_ref(i).unwrap() {
                            ValueRef::Null => Value::Null,
                            ValueRef::Integer(n) => json!(n),
                            ValueRef::Real(f) => json!(f),
                            ValueRef::Text(t) | ValueRef::Blob(t) => {
                                json!(String::from_utf8_lossy(t))
                            }
                        })
                        .collect(),
                ))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect();
        out.insert(name, Value::Array(rows));
    }
    Value::Object(out)
}

fn context(scenario: &Value) -> Context {
    let ws = &scenario["workspace"];
    let user = &scenario["user"];
    assert!(
        scenario["actor"].is_null(),
        "actors are not supported by the runner yet"
    );
    Context {
        workspace: LocalWorkspace {
            manifest: Manifest {
                id: ws["id"].as_i64().unwrap(),
                name: ws["name"].as_str().unwrap().into(),
                code: ws["code"].as_str().unwrap().into(),
                schema_version: 0,
                created_at: String::new(),
            },
            path: "/tmp/x".into(),
            database: "/tmp/x/workspace.db".into(),
        },
        user: User {
            id: user["id"].as_i64().unwrap(),
            name: user["name"].as_str().unwrap().into(),
            email: user["email"].as_str().unwrap().into(),
        },
        now: DateTime::parse_from_rfc3339(scenario["now"].as_str().unwrap())
            .unwrap()
            .with_timezone(&Utc),
        local_offset: chrono::FixedOffset::east_opt(0).unwrap(),
        actor: None,
    }
}

/// Routes only the Rust crate has; the TS golden answers them with "no route".
fn rust_only(url: &str) -> bool {
    matches!(
        segments(url).as_slice(),
        ["workspaces"] | ["tasks", "by-key", _] | ["workspaces", _, "time-summary" | "agent-work"]
    )
}

/// Route families the crate does not answer yet; any other unanswered request is a failure.
fn not_ported_yet(url: &str) -> bool {
    matches!(
        segments(url).as_slice(),
        ["pages", ..]
            | ["tasks", _, "pages" | "files"]
            | ["daily-routines", ..]
            | ["files", ..]
            | ["plugins", ..]
            | ["graph", ..]
            | ["workspaces", "context"]
            | ["workspaces", _, "modules" | "settings" | "files"]
    )
}

fn segments(url: &str) -> Vec<&str> {
    let path = url.split('?').next().unwrap_or("");
    let path = path.trim_start_matches('/').trim_start_matches("api/");
    path.trim_matches('/').split('/').collect()
}

#[derive(Default)]
struct Outcome {
    passed: usize,
    unported: Vec<String>,
    failed: Vec<String>,
}

fn run(name: &str, schema: &Value, outcome: &mut Outcome) {
    let scenario = read(&dir().join("scenarios").join(format!("{name}.json")));
    let golden = read(&dir().join("golden").join(format!("{name}.json")));
    let conn = migrated(schema, scenario["now"].as_str().unwrap());
    for seed in scenario["seed"].as_array().unwrap() {
        let params: Vec<Sql> = seed["params"]
            .as_array()
            .map(|p| p.iter().map(sql_value).collect())
            .unwrap_or_default();
        conn.execute(
            seed["sql"].as_str().unwrap(),
            rusqlite::params_from_iter(params),
        )
        .unwrap_or_else(|e| panic!("{name} seed: {e}"));
    }
    let before = dump(&conn);
    conn.execute_batch("PRAGMA query_only = ON").unwrap();
    let ctx = context(&scenario);
    let expected = golden["responses"].as_array().unwrap();
    let requests = scenario["requests"].as_array().unwrap();
    assert_eq!(requests.len(), expected.len(), "{name}: golden length");
    for (request, want) in requests.iter().zip(expected) {
        let method = request["method"].as_str().unwrap();
        let url = request["url"].as_str().unwrap();
        let label = format!("{name}: {method} {url}");
        let not_found = json!({ "status": 404, "data": null });
        if *want == not_found && rust_only(url) {
            continue;
        }
        let got = match dispatch(&conn, &ctx, method, url, request.get("body")) {
            Ok(Some(r)) => json!({ "status": r.status, "data": r.data }),
            Ok(None) if *want != not_found && not_ported_yet(url) => {
                outcome.unported.push(label);
                continue;
            }
            Ok(None) => not_found,
            Err(e) => json!({ "error": e.to_string() }),
        };
        if got == *want {
            outcome.passed += 1;
        } else {
            outcome.failed.push(format!(
                "{label}\n  want {}\n  got  {}",
                serde_json::to_string(want).unwrap(),
                serde_json::to_string(&got).unwrap()
            ));
        }
    }
    conn.execute_batch("PRAGMA query_only = OFF").unwrap();
    assert_eq!(dump(&conn), before, "{name}: a read changed the database");
}

#[test]
fn scenarios_match_the_ts_goldens() {
    let scenarios = dir().join("scenarios");
    let mut names: Vec<String> = fs::read_dir(&scenarios)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter_map(|e| {
                    e.file_name()
                        .to_str()?
                        .strip_suffix(".json")
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    let schema_file = dir().join("golden/schema.json");
    assert!(
        !names.is_empty(),
        "no parity scenarios in {}",
        scenarios.display()
    );
    assert!(schema_file.exists(), "missing {}", schema_file.display());
    let schema = read(&schema_file);
    assert_eq!(schema["schema_version"], json!(tmgr_local::SCHEMA_VERSION));
    let mut outcome = Outcome::default();
    for name in &names {
        run(name, &schema, &mut outcome);
    }
    eprintln!(
        "parity: {} requests match, {} on routes not ported yet",
        outcome.passed,
        outcome.unported.len()
    );
    for label in &outcome.unported {
        eprintln!("  not ported: {label}");
    }
    assert!(outcome.passed > 0, "no request was compared");
    assert!(
        outcome.failed.is_empty(),
        "{} parity mismatches:\n{}",
        outcome.failed.len(),
        outcome.failed.join("\n")
    );
}

#[test]
fn every_golden_has_a_scenario() {
    let names = |sub: &str| -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(dir().join(sub))
            .map(|e| {
                e.filter_map(Result::ok)
                    .filter_map(|e| {
                        e.file_name()
                            .to_str()?
                            .strip_suffix(".json")
                            .map(str::to_string)
                    })
                    .filter(|n| n != "schema")
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    };
    assert_eq!(names("golden"), names("scenarios"));
}
