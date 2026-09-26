use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use rusqlite::types::{Value, ValueRef};
use rusqlite::{params_from_iter, Connection};
use serde::Serialize;
use serde_json::{Map, Number, Value as Json};
use tauri::{AppHandle, Runtime, State};

use crate::local_workspaces;

/// One connection per local workspace, opened lazily. SQLite serialises writers anyway; a single
/// connection also keeps per-connection pragmas (foreign_keys) reliable.
#[derive(Default)]
pub struct LocalDbs(Mutex<HashMap<String, Arc<Mutex<Connection>>>>);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteResult {
  rows_affected: usize,
  last_insert_id: i64,
}

/// Statements the local API never needs and that could reach outside the workspace file
/// (ATTACH a path, VACUUM INTO a path, pragmas that change safety settings).
pub fn check_statement(sql: &str) -> Result<(), String> {
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

fn connection<R: Runtime>(app: &AppHandle<R>, dbs: &LocalDbs, code: &str) -> Result<Arc<Mutex<Connection>>, String> {
  let mut map = dbs.0.lock().unwrap();
  if let Some(conn) = map.get(code) {
    return Ok(conn.clone());
  }
  let workspace = local_workspaces::find(app, code)?;
  let conn = Arc::new(Mutex::new(open(&workspace.database)?));
  map.insert(code.to_string(), conn.clone());
  Ok(conn)
}

#[tauri::command]
pub async fn local_db_select<R: Runtime>(
  app: AppHandle<R>,
  dbs: State<'_, LocalDbs>,
  code: String,
  sql: String,
  params: Vec<Json>,
) -> Result<Vec<Map<String, Json>>, String> {
  let conn = connection(&app, &dbs, &code)?;
  let guard = conn.lock().unwrap();
  select(&guard, &sql, &params)
}

#[tauri::command]
pub async fn local_db_execute<R: Runtime>(
  app: AppHandle<R>,
  dbs: State<'_, LocalDbs>,
  code: String,
  sql: String,
  params: Vec<Json>,
) -> Result<ExecuteResult, String> {
  let conn = connection(&app, &dbs, &code)?;
  let guard = conn.lock().unwrap();
  execute(&guard, &sql, &params)
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
  let conn = connection(&app, &dbs, &code)?;
  let guard = conn.lock().unwrap();
  guard
    .execute("VACUUM INTO ?", [target.to_string_lossy().as_ref()])
    .map_err(|e| e.to_string())?;
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
  fn several_statements_in_one_call_are_rejected() {
    let conn = open(":memory:").unwrap();
    execute(&conn, "CREATE TABLE t (id INTEGER)", &[]).unwrap();
    assert!(execute(&conn, "INSERT INTO t VALUES (1); DROP TABLE t", &[]).is_err());
    assert_eq!(select(&conn, "SELECT COUNT(*) AS n FROM t", &[]).unwrap()[0]["n"], json!(0));
  }
}
