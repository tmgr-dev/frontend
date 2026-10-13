use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension};

use crate::local_workspaces::list_in;

pub struct CommentInfo {
  pub author_kind: String,
  pub author_id: Option<String>,
  pub created_at: String,
  pub message: String,
}

pub struct TaskInfo {
  pub workspace: String,
  pub title: String,
  pub description: String,
  pub comments: Vec<CommentInfo>,
}

fn read_task(conn: &Connection, workspace: String, task_id: i64) -> rusqlite::Result<Option<TaskInfo>> {
  let tx = conn.unchecked_transaction()?;
  let head: Option<(String, Option<String>)> = tx
    .query_row("SELECT title, description FROM tasks WHERE id = ?1", [task_id], |r| Ok((r.get(0)?, r.get(1)?)))
    .optional()?;
  let Some((title, description)) = head else { return Ok(None) };
  let mut stmt = tx.prepare(
    "SELECT author_kind, author_id, created_at, message FROM comments WHERE task_id = ?1 AND deleted_at IS NULL ORDER BY created_at, id",
  )?;
  let comments = stmt
    .query_map([task_id], |r| {
      Ok(CommentInfo { author_kind: r.get(0)?, author_id: r.get(1)?, created_at: r.get(2)?, message: r.get(3)? })
    })?
    .collect::<rusqlite::Result<Vec<_>>>()?;
  drop(stmt);
  Ok(Some(TaskInfo { workspace, title, description: description.unwrap_or_default(), comments }))
}

pub fn lookup_in(root: &Path, workspace_id: i64, task_id: i64) -> Result<TaskInfo, String> {
  let not_found = || "task not found".to_owned();
  let ws = list_in(root).into_iter().find(|w| w.manifest.id == workspace_id).ok_or_else(not_found)?;
  let conn = Connection::open_with_flags(&ws.database, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
    .map_err(|_| not_found())?;
  read_task(&conn, ws.manifest.name, task_id)
    .map_err(|_| "Could not read the task from the local workspace".to_owned())?
    .ok_or_else(not_found)
}

#[cfg(test)]
pub(crate) const TEST_SCHEMA: &str = "
  CREATE TABLE tasks (id INTEGER PRIMARY KEY, title TEXT NOT NULL, description TEXT);
  CREATE TABLE comments (id INTEGER PRIMARY KEY, task_id INTEGER NOT NULL, message TEXT NOT NULL, created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL DEFAULT '', deleted_at TEXT, author_kind TEXT NOT NULL DEFAULT 'user', author_id TEXT, author_name TEXT);
  INSERT INTO tasks VALUES (7, 'Real title', 'Real body');
  INSERT INTO tasks VALUES (8, 'Other', NULL);
  INSERT INTO comments (task_id, message, created_at, author_kind, author_id) VALUES
  (7, 'second', '2026-10-13T09:31:00.000Z', 'persona', 'p-1'),
  (7, 'first', '2026-10-13T09:30:00.000Z', 'user', '1'),
  (7, 'gone', '2026-10-13T09:32:00.000Z', 'user', '1'),
  (8, 'elsewhere', '2026-10-13T09:30:00.000Z', 'user', '1');
  UPDATE comments SET deleted_at = '2026-10-13T10:00:00.000Z' WHERE message = 'gone';
";

#[cfg(test)]
mod tests {
  use super::*;
  use crate::local_workspaces::create_in;

  fn seeded() -> (tempfile::TempDir, i64) {
    let root = tempfile::tempdir().unwrap();
    let ws = create_in(root.path(), "Personal", "now").unwrap();
    let conn = Connection::open(&ws.database).unwrap();
    conn.execute_batch(TEST_SCHEMA).unwrap();
    (root, ws.manifest.id)
  }

  #[test]
  fn title_comes_from_the_workspace_database() {
    let (root, id) = seeded();
    let info = lookup_in(root.path(), id, 7).unwrap();
    assert_eq!((info.workspace.as_str(), info.title.as_str()), ("Personal", "Real title"));
  }

  #[test]
  fn body_and_comments_come_from_one_read() {
    let (root, id) = seeded();
    let info = lookup_in(root.path(), id, 7).unwrap();
    assert_eq!(info.description, "Real body");
    let got: Vec<_> = info.comments.iter().map(|c| (c.message.as_str(), c.author_kind.as_str(), c.author_id.as_deref())).collect();
    assert_eq!(got, [("first", "user", Some("1")), ("second", "persona", Some("p-1"))]);
    let other = lookup_in(root.path(), id, 8).unwrap();
    assert_eq!((other.description.as_str(), other.comments.len()), ("", 1));
  }

  #[test]
  fn unknown_workspace_or_task_is_not_found() {
    let (root, id) = seeded();
    assert_eq!(lookup_in(root.path(), id, 99).err().unwrap(), "task not found");
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
