use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use rusqlite::types::{Value as Sql, ValueRef};
use rusqlite::Connection;
use serde_json::{json, Value};
use tmgr_local::{dispatch, Actor, Context, LocalWorkspace, Manifest, User, WriteOrigin};

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
        let columns: Vec<String> = stmt.column_names().iter().map(|c| c.to_string()).collect();
        let rows: Vec<Value> = stmt
            .query_map([], |r| {
                let mut row = serde_json::Map::new();
                for (i, column) in columns.iter().enumerate() {
                    let value = match r.get_ref(i).unwrap() {
                        ValueRef::Null => Value::Null,
                        ValueRef::Integer(n) => json!(n),
                        ValueRef::Real(f) if f.fract() == 0.0 && f.abs() < 9e15 => json!(f as i64),
                        ValueRef::Real(f) => json!(f),
                        ValueRef::Text(t) | ValueRef::Blob(t) => json!(String::from_utf8_lossy(t)),
                    };
                    row.insert(column.clone(), value);
                }
                Ok(Value::Object(row))
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
        actor: actor(&scenario["actor"]),
        origin: WriteOrigin::DesktopUi,
    }
}

fn actor(value: &Value) -> Option<Actor> {
    let a = value.as_object()?;
    let text = |k: &str| a.get(k).and_then(Value::as_str).map(str::to_string);
    Some(Actor {
        kind: text("kind")?,
        id: text("id")?,
        name: text("name").unwrap_or_default(),
        owner_id: text("ownerId"),
    })
}

/// Routes only the Rust crate has; the TS golden answers them with "no route".
fn rust_only(url: &str) -> bool {
    matches!(
        segments(url).as_slice(),
        ["workspaces"] | ["tasks", "by-key", _] | ["workspaces", _, "time-summary" | "agent-work"]
    )
}

/// TS routes the crate does not answer yet (`:x` any segment, `#` digits only). Any other request
/// without an answer is a failure.
const NOT_PORTED: &[(&str, &str)] = &[
    ("POST", "tasks/#/related-to/#/with/#"),
    ("DELETE", "tasks/#/related-to/#/with/#"),
    ("POST", "files/presign-upload"),
    ("GET", "tasks/#/files"),
    ("POST", "tasks/#/files"),
    ("GET", "files/#"),
    ("GET", "files/#/signed-url"),
    ("GET", "files/#/content"),
    ("DELETE", "files/#"),
    ("GET", "workspaces/:x/files"),
    ("GET", "plugins/:x/storage"),
    ("GET", "plugins/:x/storage/:x"),
    ("PUT", "plugins/:x/storage/:x"),
    ("DELETE", "plugins/:x/storage/:x"),
    ("GET", "plugins/:x/tasks/#/data/:x"),
    ("PUT", "plugins/:x/tasks/#/data/:x"),
    ("DELETE", "plugins/:x/tasks/#/data/:x"),
    ("POST", "plugins/:x/task-data/query"),
    ("GET", "workspaces/:x/modules"),
    ("PUT", "workspaces/:x/modules"),
    ("PUT", "workspaces/:x/modules/hidden"),
    ("GET", "daily-routines/workspace"),
    ("GET", "daily-routines/tasks/count"),
    ("GET", "daily-routines/tasks/archived/count"),
    ("GET", "daily-routines/tasks/completed/count"),
    ("POST", "daily-routines/tasks"),
    ("POST", "daily-routines/tasks/recurring"),
    ("POST", "daily-routines/tasks/quick"),
    ("GET", "daily-routines/tasks/upcoming"),
    ("GET", "daily-routines/tasks"),
    ("GET", "daily-routines/tasks/#"),
    ("PUT", "daily-routines/tasks/#"),
    ("DELETE", "daily-routines/tasks/#"),
    ("POST", "daily-routines/tasks/#/archive"),
    ("POST", "daily-routines/tasks/#/convert"),
    ("GET", "daily-routines/tasks/#/instances"),
    ("DELETE", "daily-routines/tasks/#/instances/#"),
    ("PATCH", "daily-routines/tasks/#/instances/:x"),
    ("PUT", "daily-routines/tasks/#/pattern"),
    ("GET", "daily-routines/tasks/#/stats"),
    ("GET", "daily-routines/expand/stats"),
    ("GET", "workspaces/:x/settings"),
    ("PUT", "workspaces/:x/settings"),
    ("GET", "workspaces/context"),
    ("GET", "pages"),
    ("GET", "pages/trash"),
    ("GET", "pages/search"),
    ("POST", "pages/:x/versions/#/restore"),
    ("GET", "pages/:x/files"),
    ("POST", "pages/:x/files"),
    ("POST", "pages/:x/task-from-selection"),
    ("POST", "pages/:x/follow"),
    ("DELETE", "pages/:x/follow"),
    ("GET", "tasks/#/pages"),
    ("GET", "plugins/:x/pages/#/data/:x"),
    ("PUT", "plugins/:x/pages/#/data/:x"),
    ("DELETE", "plugins/:x/pages/#/data/:x"),
    ("POST", "plugins/:x/page-data/query"),
    ("GET", "graph/related"),
    ("GET", "graph/path"),
    ("GET", "graph/hubs"),
    ("GET", "graph/orphans"),
    ("GET", "graph/map"),
];

fn not_ported_yet(method: &str, url: &str) -> bool {
    let parts = segments(url);
    NOT_PORTED.iter().any(|(m, pattern)| {
        let pattern: Vec<&str> = pattern.split('/').collect();
        *m == method
            && pattern.len() == parts.len()
            && pattern.iter().zip(&parts).all(|(p, s)| match *p {
                ":x" => !s.is_empty(),
                "#" => !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()),
                lit => lit == *s,
            })
    })
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
    tables_matched: usize,
    tables_skipped: Vec<String>,
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
    let base = context(&scenario);
    let mut skipped = false;
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
        let mut ctx = base.clone();
        if let Some(now) = request.get("now").and_then(Value::as_str) {
            ctx.now = DateTime::parse_from_rfc3339(now)
                .unwrap()
                .with_timezone(&Utc);
        }
        if let Some(a) = request.get("actor") {
            ctx.actor = actor(a);
        }
        let got = match dispatch(&conn, &ctx, method, url, request.get("body")) {
            Ok(Some(r)) => json!({ "status": r.status, "data": r.data }),
            Ok(None) if *want != not_found && not_ported_yet(method, url) => {
                outcome.unported.push(label);
                skipped |= method != "GET";
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
    let after = dump(&conn);
    let mut changed = serde_json::Map::new();
    for (table, rows) in after.as_object().unwrap() {
        if before.get(table) != Some(rows) {
            changed.insert(table.clone(), rows.clone());
        }
    }
    let changed = Value::Object(changed);
    let all_get = requests
        .iter()
        .all(|r| r["method"].as_str().unwrap().eq_ignore_ascii_case("GET"));
    if all_get {
        assert_eq!(changed, json!({}), "{name}: a read changed the database");
    }
    let want = golden.get("tables").cloned().unwrap_or_else(|| json!({}));
    if skipped {
        outcome.tables_skipped.push(name.to_string());
    } else if changed == want {
        outcome.tables_matched += 1;
    } else {
        outcome.failed.push(table_diff(name, &want, &changed));
    }
}

fn table_diff(name: &str, want: &Value, got: &Value) -> String {
    let mut lines = vec![format!("{name}: tables differ")];
    let empty = serde_json::Map::new();
    let w = want.as_object().unwrap_or(&empty);
    let g = got.as_object().unwrap_or(&empty);
    let mut names: Vec<&String> = w.keys().chain(g.keys()).collect();
    names.sort();
    names.dedup();
    for t in names {
        if w.get(t) != g.get(t) {
            lines.push(format!(
                "  {t}\n    want {}\n    got  {}",
                w.get(t).map_or("-".into(), |v| v.to_string()),
                g.get(t).map_or("-".into(), |v| v.to_string())
            ));
        }
    }
    lines.join("\n")
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
        "parity: {} requests match, {} on routes not ported yet; table state matches in {} scenarios, not compared in {:?}",
        outcome.passed,
        outcome.unported.len(),
        outcome.tables_matched,
        outcome.tables_skipped
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
