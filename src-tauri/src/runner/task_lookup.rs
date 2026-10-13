use std::path::Path;

use rusqlite::{Connection, OpenFlags, OptionalExtension};

use super::bundle::MAX_COMMENTS;
use crate::local_workspaces::list_in;

pub const MAX_TITLE: i64 = 1000;
pub const MAX_DESCRIPTION: usize = 256 << 10;
pub const MAX_COMMENT: usize = 64 << 10;
pub const MAX_TOTAL: usize = 900 << 10;

pub struct CommentInfo {
  pub author_kind: String,
  pub author_id: Option<String>,
  pub created_at: String,
  pub message: String,
}

pub struct TaskInfo {
  pub title: String,
  pub description: String,
  pub comments: Vec<CommentInfo>,
}

fn too_large(what: &str) -> String {
  format!("The task is too large to send to an agent ({what})")
}

fn db(_: rusqlite::Error) -> String {
  "Could not read the task from the local workspace".to_owned()
}

fn read_task(conn: &Connection, task_id: i64) -> Result<Option<TaskInfo>, String> {
  let tx = conn.unchecked_transaction().map_err(db)?;
  let sizes: Option<(i64, i64)> = tx
    .query_row("SELECT length(title), COALESCE(length(CAST(description AS BLOB)), 0) FROM tasks WHERE id = ?1", [task_id], |r| {
      Ok((r.get(0)?, r.get(1)?))
    })
    .optional()
    .map_err(db)?;
  let Some((title_len, description_len)) = sizes else { return Ok(None) };
  if title_len > MAX_TITLE {
    return Err(too_large(&format!("the title is over {MAX_TITLE} characters")));
  }
  if description_len as usize > MAX_DESCRIPTION {
    return Err(too_large(&format!("the description is over {} KiB", MAX_DESCRIPTION >> 10)));
  }
  let mut total = description_len as usize;
  let mut ids = Vec::new();
  {
    let mut stmt = tx
      .prepare(
        "SELECT id, length(CAST(message AS BLOB)) FROM comments WHERE task_id = ?1 AND deleted_at IS NULL ORDER BY created_at, id LIMIT ?2",
      )
      .map_err(db)?;
    let mut rows = stmt.query(rusqlite::params![task_id, (MAX_COMMENTS + 1) as i64]).map_err(db)?;
    while let Some(row) = rows.next().map_err(db)? {
      let (id, len): (i64, i64) = (row.get(0).map_err(db)?, row.get(1).map_err(db)?);
      if ids.len() == MAX_COMMENTS {
        return Err(too_large(&format!("more than {MAX_COMMENTS} comments")));
      }
      if len as usize > MAX_COMMENT {
        return Err(too_large(&format!("a comment is over {} KiB", MAX_COMMENT >> 10)));
      }
      total += len as usize;
      if total > MAX_TOTAL {
        return Err(too_large(&format!("the description and comments exceed {} KiB", MAX_TOTAL >> 10)));
      }
      ids.push(id);
    }
  }
  let (title, description): (String, Option<String>) =
    tx.query_row("SELECT title, description FROM tasks WHERE id = ?1", [task_id], |r| Ok((r.get(0)?, r.get(1)?))).map_err(db)?;
  let mut stmt = tx
    .prepare("SELECT substr(author_kind, 1, 32), substr(author_id, 1, 65), substr(created_at, 1, 64), message FROM comments WHERE id = ?1")
    .map_err(db)?;
  let comments = ids
    .into_iter()
    .map(|id| {
      stmt.query_row([id], |r| Ok(CommentInfo { author_kind: r.get(0)?, author_id: r.get(1)?, created_at: r.get(2)?, message: r.get(3)? })).map_err(db)
    })
    .collect::<Result<Vec<_>, _>>()?;
  drop(stmt);
  Ok(Some(TaskInfo { title, description: description.unwrap_or_default(), comments }))
}

pub fn lookup_in(root: &Path, workspace_id: i64, task_id: i64) -> Result<TaskInfo, String> {
  let not_found = || "task not found".to_owned();
  let ws = list_in(root).into_iter().find(|w| w.manifest.id == workspace_id).ok_or_else(not_found)?;
  let conn = Connection::open_with_flags(&ws.database, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
    .map_err(|_| not_found())?;
  read_task(&conn, task_id)?.ok_or_else(not_found)
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
    assert_eq!(info.title, "Real title");
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

  fn exec(root: &tempfile::TempDir, sql: &str, params: impl rusqlite::Params) {
    let ws = list_in(root.path()).remove(0);
    Connection::open(&ws.database).unwrap().execute(sql, params).unwrap();
  }

  fn add_comments(root: &tempfile::TempDir, n: usize, size: usize) {
    let ws = list_in(root.path()).remove(0);
    let conn = Connection::open(&ws.database).unwrap();
    let mut stmt = conn.prepare("INSERT INTO comments (task_id, message, created_at) VALUES (8, ?1, '2026-10-13T09:30:00.000Z')").unwrap();
    for _ in 0..n {
      stmt.execute([&"c".repeat(size)]).unwrap();
    }
  }

  #[test]
  fn a_long_title_is_refused() {
    let (root, id) = seeded();
    exec(&root, "UPDATE tasks SET title = ?1 WHERE id = 8", [&"t".repeat(1000)]);
    assert_eq!(lookup_in(root.path(), id, 8).unwrap().title.len(), 1000);
    exec(&root, "UPDATE tasks SET title = ?1 WHERE id = 8", [&"t".repeat(1001)]);
    assert!(lookup_in(root.path(), id, 8).err().unwrap().starts_with("The task is too large to send to an agent (the title"));
  }

  #[test]
  fn a_large_description_is_refused_by_bytes() {
    let (root, id) = seeded();
    exec(&root, "UPDATE tasks SET description = ?1 WHERE id = 8", [&"d".repeat(MAX_DESCRIPTION)]);
    assert_eq!(lookup_in(root.path(), id, 8).unwrap().description.len(), MAX_DESCRIPTION);
    exec(&root, "UPDATE tasks SET description = ?1 WHERE id = 8", [&"é".repeat(MAX_DESCRIPTION / 2 + 1)]);
    assert!(lookup_in(root.path(), id, 8).err().unwrap().contains("description"));
  }

  #[test]
  fn more_than_500_comments_are_refused() {
    let (root, id) = seeded();
    add_comments(&root, 499, 1);
    assert_eq!(lookup_in(root.path(), id, 8).unwrap().comments.len(), 500);
    add_comments(&root, 1, 1);
    assert!(lookup_in(root.path(), id, 8).err().unwrap().contains("more than 500 comments"));
  }

  #[test]
  fn an_oversized_comment_is_refused() {
    let (root, id) = seeded();
    add_comments(&root, 1, MAX_COMMENT);
    assert!(lookup_in(root.path(), id, 8).is_ok());
    add_comments(&root, 1, MAX_COMMENT + 1);
    assert!(lookup_in(root.path(), id, 8).err().unwrap().contains("a comment is over 64 KiB"));
  }

  #[test]
  fn description_and_comments_share_one_budget() {
    let (root, id) = seeded();
    add_comments(&root, 14, 60 * 1024);
    exec(&root, "UPDATE tasks SET description = ?1 WHERE id = 8", [&"d".repeat(50 * 1024)]);
    assert_eq!(lookup_in(root.path(), id, 8).unwrap().comments.len(), 15);
    add_comments(&root, 1, 60 * 1024);
    assert!(lookup_in(root.path(), id, 8).err().unwrap().contains("exceed 900 KiB"));
  }
}
