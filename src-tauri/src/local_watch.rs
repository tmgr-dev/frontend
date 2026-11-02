use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::Connection;
use serde::Serialize;
use tauri::{AppHandle, Emitter, EventTarget, Manager, Runtime};

use crate::local_db::LocalDbs;

pub const EVENT: &str = "tmgr://local-db-changed";
const TICK: Duration = Duration::from_secs(1);

#[derive(Clone, Serialize)]
struct Changed {
  code: String,
}

#[derive(Default)]
pub struct ChangeWatch(HashMap<String, i64>);

impl ChangeWatch {
  pub fn seed(&mut self, key: String, version: i64) {
    self.0.insert(key, version);
  }

  pub fn observe(&mut self, key: &str, version: i64) -> bool {
    match self.0.insert(key.to_string(), version) {
      Some(previous) => previous != version,
      None => false,
    }
  }

  #[cfg(test)]
  pub fn len(&self) -> usize {
    self.0.len()
  }

  pub fn retain(&mut self, live: &HashSet<String>) {
    self.0.retain(|key, _| live.contains(key));
  }
}

pub fn watch_key(code: &str, conn: &Arc<Mutex<Connection>>) -> String {
  format!("{code}#{:p}", Arc::as_ptr(conn))
}

pub fn data_version(conn: &Connection) -> Option<i64> {
  conn.query_row("PRAGMA data_version", [], |row| row.get(0)).ok()
}

pub fn poll_once(dbs: &LocalDbs, mut emit: impl FnMut(&str)) {
  let mut changed = Vec::new();
  for (code, conn) in dbs.snapshot_for_watch() {
    let key = watch_key(&code, &conn);
    let version = match conn.try_lock() {
      Ok(guard) => data_version(&guard),
      Err(_) => None,
    };
    drop(conn);
    if let Some(version) = version {
      if dbs.observe_version(&key, version) {
        changed.push(code);
      }
    }
  }
  for code in changed {
    emit(&code);
  }
}

fn is_ui_window(target: &EventTarget) -> bool {
  match target {
    EventTarget::WebviewWindow { label } | EventTarget::Webview { label } => {
      label == "main" || label.starts_with("task-")
    }
    _ => false,
  }
}

pub fn start<R: Runtime>(app: &AppHandle<R>) {
  let handle = app.clone();
  std::thread::spawn(move || loop {
    std::thread::sleep(TICK);
    let dbs = handle.state::<LocalDbs>();
    poll_once(&dbs, |code| {
      let _ = handle.emit_filter(EVENT, Changed { code: code.to_string() }, is_ui_window);
    });
  });
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::local_db::open;

  fn setup() -> (tempfile::TempDir, String, LocalDbs, Arc<Mutex<Connection>>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("workspace.db").to_string_lossy().into_owned();
    let app = open(&path).unwrap();
    app.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)").unwrap();
    let dbs = LocalDbs::default();
    let conn = Arc::new(Mutex::new(app));
    dbs.register("ws", path.clone(), conn.clone());
    (dir, path, dbs, conn)
  }

  fn count(dbs: &LocalDbs) -> Vec<String> {
    let mut out = Vec::new();
    poll_once(dbs, |code| out.push(code.to_string()));
    out
  }

  #[test]
  fn external_commit_emits_once_and_idle_polls_stay_quiet() {
    let (_dir, path, dbs, _conn) = setup();
    assert!(count(&dbs).is_empty());
    let cli = open(&path).unwrap();
    cli.execute("INSERT INTO t (v) VALUES ('a')", []).unwrap();
    assert_eq!(count(&dbs), vec!["ws".to_string()]);
    assert!(count(&dbs).is_empty());
    assert!(count(&dbs).is_empty());
  }

  #[test]
  fn own_commit_does_not_emit() {
    let (_dir, _path, dbs, conn) = setup();
    conn.lock().unwrap().execute("INSERT INTO t (v) VALUES ('own')", []).unwrap();
    assert!(count(&dbs).is_empty());
  }

  #[test]
  fn burst_of_external_commits_emits_once() {
    let (_dir, path, dbs, _conn) = setup();
    let cli = open(&path).unwrap();
    for i in 0..50 {
      cli.execute("INSERT INTO t (v) VALUES (?1)", [i]).unwrap();
    }
    assert_eq!(count(&dbs).len(), 1);
    assert!(count(&dbs).is_empty());
  }

  #[test]
  fn write_between_open_and_first_poll_is_not_lost() {
    let (_dir, path, dbs, _conn) = setup();
    let cli = open(&path).unwrap();
    cli.execute("INSERT INTO t (v) VALUES ('x')", []).unwrap();
    let late = Arc::new(Mutex::new(open(&path).unwrap()));
    dbs.register("late", path.clone(), late);
    cli.execute("INSERT INTO t (v) VALUES ('y')", []).unwrap();
    let mut codes = count(&dbs);
    codes.sort();
    assert_eq!(codes, vec!["late".to_string(), "ws".to_string()]);
  }

  #[test]
  fn reopened_connection_does_not_emit_spuriously() {
    let (_dir, path, dbs, _conn) = setup();
    let cli = open(&path).unwrap();
    cli.execute("INSERT INTO t (v) VALUES ('a')", []).unwrap();
    dbs.register("ws", path.clone(), Arc::new(Mutex::new(open(&path).unwrap())));
    assert!(count(&dbs).is_empty());
    cli.execute("INSERT INTO t (v) VALUES ('b')", []).unwrap();
    assert_eq!(count(&dbs).len(), 1);
  }

  #[test]
  fn busy_connection_is_skipped_and_forgotten_ones_dropped() {
    let (_dir, path, dbs, conn) = setup();
    let cli = open(&path).unwrap();
    cli.execute("INSERT INTO t (v) VALUES ('a')", []).unwrap();
    {
      let _held = conn.lock().unwrap();
      assert!(count(&dbs).is_empty());
    }
    assert_eq!(count(&dbs).len(), 1);
    dbs.forget("ws");
    drop(conn);
    cli.execute("INSERT INTO t (v) VALUES ('b')", []).unwrap();
    assert!(count(&dbs).is_empty());
    assert!(dbs.watch_len() == 0);
  }
}
