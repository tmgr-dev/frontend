use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rusqlite::types::{Value, ValueRef};
use rusqlite::{params_from_iter, Connection, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Number, Value as Json};
use tauri::{AppHandle, Runtime, State};

use crate::local_workspaces;

/// One connection per local workspace, opened lazily. SQLite serialises writers anyway; a single
/// connection also keeps per-connection pragmas (foreign_keys) reliable.
#[derive(Default)]
pub struct LocalDbs(Mutex<HashMap<String, (String, Arc<Mutex<Connection>>)>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteResult {
  rows_affected: usize,
  last_insert_id: i64,
}

/// Statements the local API never needs and that could reach outside the workspace file
/// (ATTACH a path, VACUUM INTO a path, pragmas that change safety settings).
pub fn check_statement(sql: &str) -> Result<(), String> {
  if let Some(first) = first_keyword(sql) {
    if ["BEGIN", "COMMIT", "END", "ROLLBACK", "SAVEPOINT", "RELEASE"].contains(&first.as_str()) {
      return Err(format!("{first} is not allowed: transactions are managed by local_db_batch"));
    }
  }
  let upper = sql.to_ascii_uppercase();
  let words: Vec<&str> = upper
    .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
    .filter(|w| !w.is_empty())
    .collect();
  for forbidden in ["ATTACH", "DETACH", "PRAGMA", "VACUUM", "LOAD_EXTENSION"] {
    if words.iter().any(|w| *w == forbidden) {
      return Err(format!("{forbidden} is not allowed in local workspace queries"));
    }
  }
  Ok(())
}

/// The leading keyword of a statement, after whitespace and SQL comments (trigger bodies contain BEGIN/END later on).
fn first_keyword(sql: &str) -> Option<String> {
  let mut rest = sql;
  loop {
    rest = rest.trim_start();
    if let Some(after) = rest.strip_prefix("--") {
      rest = after.split_once('\n').map_or("", |(_, tail)| tail);
    } else if let Some(after) = rest.strip_prefix("/*") {
      rest = after.split_once("*/").map_or("", |(_, tail)| tail);
    } else {
      break;
    }
  }
  let word: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
  if word.is_empty() { None } else { Some(word.to_ascii_uppercase()) }
}

fn to_sql(value: &Json) -> Value {
  match value {
    Json::Null => Value::Null,
    Json::Bool(b) => Value::Integer(*b as i64),
    Json::Number(n) => n
      .as_i64()
      .map(Value::Integer)
      .unwrap_or_else(|| Value::Real(n.as_f64().unwrap_or(0.0))),
    Json::String(s) => Value::Text(s.clone()),
    other => Value::Text(other.to_string()),
  }
}

fn to_json(value: ValueRef<'_>) -> Json {
  match value {
    ValueRef::Null => Json::Null,
    ValueRef::Integer(i) => Json::Number(i.into()),
    ValueRef::Real(f) => Number::from_f64(f).map(Json::Number).unwrap_or(Json::Null),
    ValueRef::Text(t) => Json::String(String::from_utf8_lossy(t).into_owned()),
    ValueRef::Blob(b) => Json::String(String::from_utf8_lossy(b).into_owned()),
  }
}

pub fn open(path: &str) -> Result<Connection, String> {
  let conn = Connection::open(path).map_err(|e| e.to_string())?;
  conn
    .execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA busy_timeout = 5000;")
    .map_err(|e| e.to_string())?;
  Ok(conn)
}

pub fn select(conn: &Connection, sql: &str, params: &[Json]) -> Result<Vec<Map<String, Json>>, String> {
  check_statement(sql)?;
  let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
  let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
  let mut rows = stmt
    .query(params_from_iter(params.iter().map(to_sql)))
    .map_err(|e| e.to_string())?;
  let mut out = Vec::new();
  while let Some(row) = rows.next().map_err(|e| e.to_string())? {
    let mut object = Map::new();
    for (i, name) in names.iter().enumerate() {
      object.insert(name.clone(), to_json(row.get_ref(i).map_err(|e| e.to_string())?));
    }
    out.push(object);
  }
  Ok(out)
}

pub fn execute(conn: &Connection, sql: &str, params: &[Json]) -> Result<ExecuteResult, String> {
  check_statement(sql)?;
  let rows_affected = conn
    .execute(sql, params_from_iter(params.iter().map(to_sql)))
    .map_err(|e| e.to_string())?;
  Ok(ExecuteResult { rows_affected, last_insert_id: conn.last_insert_rowid() })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchStatement {
  sql: String,
  #[serde(default)]
  params: Vec<Json>,
  /// A statement that must change a row (a version guard): changing none rolls the whole batch back.
  #[serde(default)]
  expect_changes: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResult {
  results: Vec<ExecuteResult>,
  failed_at: Option<usize>,
}

/// Runs the statements as one transaction while the caller holds the connection, so no other window's
/// statement can land inside it and a closing window cannot leave it open.
pub fn batch(conn: &Connection, statements: &[BatchStatement]) -> Result<BatchResult, String> {
  for statement in statements {
    check_statement(&statement.sql)?;
  }
  if !conn.is_autocommit() {
    return Err("the connection is already inside a transaction".to_string());
  }
  let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate).map_err(|e| e.to_string())?;
  let mut results = Vec::with_capacity(statements.len());
  for (index, statement) in statements.iter().enumerate() {
    let result = execute(&tx, &statement.sql, &statement.params)?;
    if tx.is_autocommit() {
      return Err("the transaction ended before the batch did".to_string());
    }
    let missed = statement.expect_changes && result.rows_affected == 0;
    results.push(result);
    if missed {
      tx.rollback().map_err(|e| e.to_string())?;
      return Ok(BatchResult { results: Vec::new(), failed_at: Some(index) });
    }
  }
  tx.commit().map_err(|e| e.to_string())?;
  Ok(BatchResult { results, failed_at: None })
}

fn connection<R: Runtime>(app: &AppHandle<R>, dbs: &LocalDbs, code: &str) -> Result<Arc<Mutex<Connection>>, String> {
  if let Some((path, conn)) = dbs.0.lock().map_err(|e| e.to_string())?.get(code) {
    // A stat per query instead of a folder scan; a removed folder falls through to a fresh lookup.
    if std::path::Path::new(path).exists() {
      return Ok(conn.clone());
    }
  }
  let workspace = local_workspaces::find(app, code)?;
  let conn = Arc::new(Mutex::new(open(&workspace.database)?));
  dbs.0
    .lock()
    .map_err(|e| e.to_string())?
    .insert(code.to_string(), (workspace.database, conn.clone()));
  Ok(conn)
}

/// SQLite calls block (busy_timeout up to 5 s), so they run off the async runtime.
async fn with_connection<R: Runtime, T: Send + 'static>(
  app: AppHandle<R>,
  dbs: &LocalDbs,
  code: String,
  work: impl FnOnce(&Connection) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
  let conn = connection(&app, dbs, &code)?;
  tauri::async_runtime::spawn_blocking(move || {
    let guard = conn.lock().map_err(|e| e.to_string())?;
    work(&guard)
  })
  .await
  .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn local_db_select<R: Runtime>(
  app: AppHandle<R>,
  dbs: State<'_, LocalDbs>,
  code: String,
  sql: String,
  params: Vec<Json>,
) -> Result<Vec<Map<String, Json>>, String> {
  with_connection(app, &dbs, code, move |conn| select(conn, &sql, &params)).await
}

#[tauri::command]
pub async fn local_db_execute<R: Runtime>(
  app: AppHandle<R>,
  dbs: State<'_, LocalDbs>,
  code: String,
  sql: String,
  params: Vec<Json>,
) -> Result<ExecuteResult, String> {
  with_connection(app, &dbs, code, move |conn| execute(conn, &sql, &params)).await
}

#[tauri::command]
pub async fn local_db_batch<R: Runtime>(
  app: AppHandle<R>,
  dbs: State<'_, LocalDbs>,
  code: String,
  statements: Vec<BatchStatement>,
) -> Result<BatchResult, String> {
  with_connection(app, &dbs, code, move |conn| batch(conn, &statements)).await
}

/// Consistent snapshot of the workspace database before a schema migration (VACUUM INTO also
/// captures what still sits in the WAL, which a plain file copy would miss).
#[tauri::command]
pub async fn local_db_backup<R: Runtime>(
  app: AppHandle<R>,
  dbs: State<'_, LocalDbs>,
  code: String,
) -> Result<String, String> {
  let workspace = local_workspaces::find(&app, &code)?;
  let target = std::path::Path::new(&workspace.path)
    .join("backups")
    .join(format!("workspace-v{}-{}.db", workspace.manifest.schema_version, crate::tray::now_secs()));
  std::fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
  let path = target.to_string_lossy().into_owned();
  with_connection(app, &dbs, code.clone(), move |conn| {
    conn.execute("VACUUM INTO ?", [path.as_str()]).map(|_| ()).map_err(|e| e.to_string())
  })
  .await?;
  log::info!("[local] backed up {code} before migration");
  Ok(target.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  #[test]
  fn statements_that_reach_outside_the_workspace_are_refused() {
    assert!(check_statement("ATTACH DATABASE '/etc/x' AS x").is_err());
    assert!(check_statement("vacuum into '/tmp/copy.db'").is_err());
    assert!(check_statement("pragma writable_schema = on").is_err());
    assert!(check_statement("SELECT load_extension('x')").is_err());
    assert!(check_statement("SELECT * FROM tasks WHERE title = 'attachment'").is_ok());
  }

  #[test]
  fn transaction_control_statements_are_refused_by_their_first_keyword() {
    for sql in [
      "BEGIN",
      "begin immediate",
      "COMMIT",
      "END TRANSACTION",
      "ROLLBACK TO x",
      "SAVEPOINT x",
      "RELEASE x",
      "  \n\tcommit",
      "/* x */ COMMIT",
      "-- c\nBEGIN",
      "/* a */ -- b\n /* c */ ROLLBACK",
    ] {
      assert!(check_statement(sql).is_err(), "{sql}");
    }
    assert!(check_statement("SELECT 'BEGIN', 'COMMIT' FROM t WHERE x = 'END'").is_ok());
    assert!(check_statement("INSERT INTO t (v) VALUES ('rollback')").is_ok());
    assert!(check_statement("CREATE TRIGGER x AFTER INSERT ON t BEGIN SELECT 1; END").is_ok());
    assert!(check_statement("-- note\nSELECT 1").is_ok());
  }

  #[test]
  fn batch_refuses_a_connection_that_is_already_in_a_transaction() {
    let conn = open(":memory:").unwrap();
    conn.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY); BEGIN").unwrap();
    let out = batch(&conn, &[statement("INSERT INTO t DEFAULT VALUES", vec![], false)]);
    assert!(out.is_err());
  }

  #[test]
  fn select_and_execute_round_trip_json_values() {
    let conn = open(":memory:").unwrap();
    execute(&conn, "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT, n REAL, flag INTEGER)", &[]).unwrap();
    let inserted = execute(&conn, "INSERT INTO t (name, n, flag) VALUES (?, ?, ?)", &[json!("a"), json!(1.5), json!(true)]).unwrap();
    assert_eq!(inserted.rows_affected, 1);
    assert_eq!(inserted.last_insert_id, 1);

    let rows = select(&conn, "SELECT * FROM t WHERE id = ?", &[json!(1)]).unwrap();
    assert_eq!(Json::Object(rows[0].clone()), json!({"id": 1, "name": "a", "n": 1.5, "flag": 1}));
  }

  #[test]
  fn fts5_virtual_table_and_sync_triggers_run_as_single_statements() {
    let conn = open(":memory:").unwrap();
    execute(&conn, "CREATE TABLE pages (id INTEGER PRIMARY KEY, title TEXT NOT NULL, body TEXT NOT NULL)", &[]).unwrap();
    execute(
      &conn,
      "CREATE VIRTUAL TABLE IF NOT EXISTS pages_fts USING fts5(title, body, content='pages', content_rowid='id')",
      &[],
    )
    .unwrap();
    execute(
      &conn,
      "CREATE TRIGGER IF NOT EXISTS pages_fts_ai AFTER INSERT ON pages BEGIN
        INSERT INTO pages_fts(rowid, title, body) VALUES (new.id, new.title, new.body);
      END",
      &[],
    )
    .unwrap();
    execute(
      &conn,
      "CREATE TRIGGER IF NOT EXISTS pages_fts_au AFTER UPDATE OF title, body ON pages BEGIN
        INSERT INTO pages_fts(pages_fts, rowid, title, body) VALUES ('delete', old.id, old.title, old.body);
        INSERT INTO pages_fts(rowid, title, body) VALUES (new.id, new.title, new.body);
      END",
      &[],
    )
    .unwrap();
    execute(&conn, "INSERT INTO pages (title, body) VALUES (?, ?)", &[json!("Заметки"), json!("Привет мир")]).unwrap();
    let found = select(&conn, "SELECT rowid FROM pages_fts WHERE pages_fts MATCH ?", &[json!("\"привет\"*")]).unwrap();
    assert_eq!(found.len(), 1);
    execute(&conn, "UPDATE pages SET body = ? WHERE id = 1", &[json!("другой текст")]).unwrap();
    assert!(select(&conn, "SELECT rowid FROM pages_fts WHERE pages_fts MATCH ?", &[json!("\"привет\"*")]).unwrap().is_empty());
    assert_eq!(select(&conn, "SELECT rowid FROM pages_fts WHERE pages_fts MATCH ?", &[json!("\"текст\"*")]).unwrap().len(), 1);
  }

  fn statement(sql: &str, params: Vec<Json>, expect_changes: bool) -> BatchStatement {
    BatchStatement { sql: sql.to_string(), params, expect_changes }
  }

  fn rows(conn: &Connection) -> i64 {
    select(conn, "SELECT COUNT(*) AS n FROM t", &[]).unwrap()[0]["n"].as_i64().unwrap()
  }

  #[test]
  fn batch_commits_every_statement_together() {
    let conn = open(":memory:").unwrap();
    execute(&conn, "CREATE TABLE t (id INTEGER PRIMARY KEY, v INTEGER NOT NULL)", &[]).unwrap();
    let out = batch(
      &conn,
      &[
        statement("INSERT INTO t (v) VALUES (?)", vec![json!(1)], false),
        statement("UPDATE t SET v = v + 1 WHERE id = ? AND v = ?", vec![json!(1), json!(1)], true),
      ],
    )
    .unwrap();
    assert_eq!(out.failed_at, None);
    assert_eq!(out.results.len(), 2);
    assert_eq!(select(&conn, "SELECT v FROM t", &[]).unwrap()[0]["v"], json!(2));
  }

  #[test]
  fn batch_rolls_back_everything_when_a_guarded_statement_changes_nothing() {
    let conn = open(":memory:").unwrap();
    execute(&conn, "CREATE TABLE t (id INTEGER PRIMARY KEY, v INTEGER NOT NULL)", &[]).unwrap();
    let out = batch(
      &conn,
      &[
        statement("INSERT INTO t (v) VALUES (?)", vec![json!(1)], false),
        statement("UPDATE t SET v = 9 WHERE v = ?", vec![json!(42)], true),
        statement("INSERT INTO t (v) VALUES (?)", vec![json!(3)], false),
      ],
    )
    .unwrap();
    assert_eq!(out.failed_at, Some(1));
    assert_eq!(rows(&conn), 0);
  }

  #[test]
  fn batch_rolls_back_on_an_error_and_leaves_the_connection_usable() {
    let conn = open(":memory:").unwrap();
    execute(&conn, "CREATE TABLE t (id INTEGER PRIMARY KEY, v INTEGER NOT NULL UNIQUE)", &[]).unwrap();
    let failed = batch(
      &conn,
      &[
        statement("INSERT INTO t (v) VALUES (?)", vec![json!(1)], false),
        statement("INSERT INTO t (v) VALUES (?)", vec![json!(1)], false),
      ],
    );
    assert!(failed.is_err());
    assert_eq!(rows(&conn), 0);
    assert!(batch(&conn, &[statement("INSERT INTO t (v) VALUES (?)", vec![json!(1)], false)]).is_ok());
    assert_eq!(rows(&conn), 1);
  }

  #[test]
  fn batch_refuses_forbidden_or_chained_statements_before_running_any() {
    let conn = open(":memory:").unwrap();
    execute(&conn, "CREATE TABLE t (id INTEGER PRIMARY KEY, v INTEGER)", &[]).unwrap();
    assert!(batch(
      &conn,
      &[statement("INSERT INTO t (v) VALUES (1)", vec![], false), statement("PRAGMA writable_schema = on", vec![], false)]
    )
    .is_err());
    assert!(batch(&conn, &[statement("INSERT INTO t (v) VALUES (1); DROP TABLE t", vec![], false)]).is_err());
    assert_eq!(rows(&conn), 0);
  }

  #[test]
  fn several_statements_in_one_call_are_rejected() {
    let conn = open(":memory:").unwrap();
    execute(&conn, "CREATE TABLE t (id INTEGER)", &[]).unwrap();
    assert!(execute(&conn, "INSERT INTO t VALUES (1); DROP TABLE t", &[]).is_err());
    assert_eq!(select(&conn, "SELECT COUNT(*) AS n FROM t", &[]).unwrap()[0]["n"], json!(0));
  }
}
