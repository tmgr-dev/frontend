use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension};

use crate::local_workspaces::list_in;

pub struct TaskInfo {
  pub workspace: String,
  pub title: String,
}

pub fn lookup_in(root: &Path, workspace_id: i64, task_id: i64) -> Result<TaskInfo, String> {
  let not_found = || "task not found".to_owned();
  let ws = list_in(root).into_iter().find(|w| w.manifest.id == workspace_id).ok_or_else(not_found)?;
  let conn = Connection::open_with_flags(&ws.database, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
    .map_err(|_| not_found())?;
  let title: Option<String> = conn
    .query_row("SELECT title FROM tasks WHERE id = ?1", [task_id], |r| r.get(0))
    .optional()
    .map_err(|_| not_found())?;
  Ok(TaskInfo { workspace: ws.manifest.name, title: title.ok_or_else(not_found)? })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::local_workspaces::create_in;

  fn seeded() -> (tempfile::TempDir, i64) {
    let root = tempfile::tempdir().unwrap();
    let ws = create_in(root.path(), "Personal", "now").unwrap();
    let conn = Connection::open(&ws.database).unwrap();
    conn.execute_batch("CREATE TABLE tasks (id INTEGER PRIMARY KEY, title TEXT NOT NULL); INSERT INTO tasks VALUES (7, 'Real title');").unwrap();
    (root, ws.manifest.id)
  }

  #[test]
  fn title_comes_from_the_workspace_database() {
    let (root, id) = seeded();
    let info = lookup_in(root.path(), id, 7).unwrap();
    assert_eq!((info.workspace.as_str(), info.title.as_str()), ("Personal", "Real title"));
  }

  #[test]
  fn unknown_workspace_or_task_is_not_found() {
    let (root, id) = seeded();
    assert_eq!(lookup_in(root.path(), id, 8).err().unwrap(), "task not found");
    assert_eq!(lookup_in(root.path(), id - 1, 7).err().unwrap(), "task not found");
    assert_eq!(lookup_in(&root.path().join("none"), id, 7).err().unwrap(), "task not found");
  }

  #[test]
  fn lookup_cannot_write() {
    let (root, id) = seeded();
    lookup_in(root.path(), id, 7).unwrap();
    let ws = list_in(root.path()).remove(0);
    let conn = Connection::open_with_flags(&ws.database, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    assert!(conn.execute("DELETE FROM tasks", []).is_err());
  }
}
