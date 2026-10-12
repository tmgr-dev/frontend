use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value as Json};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Runtime, State};

use crate::local_backups::{rfc3339, stamp, verify};
use crate::local_db::{live_connection, with_connection, LocalDbs};
use crate::local_files::{content_type, file_path, parse_target};
use crate::local_workspaces::{self, LocalWorkspace};

pub const MOVING: &str = "workspace is moving";
const BUNDLE_SCHEMA: u32 = 1;
const PUT_BODY_LIMIT: usize = 2048;

const ENTITIES: [&str; 17] = [
  "statuses",
  "categories",
  "tasks",
  "task_relations",
  "comments",
  "comment_reactions",
  "files",
  "pages",
  "page_versions",
  "routines",
  "routine_patterns",
  "routine_instances",
  "agent_work_runs",
  "personas",
  "workspace_personas",
  "task_persona_assignees",
  "meta",
];

pub const BUSY: &str = "another move or removal is in progress";

#[derive(Default)]
struct SlotState {
  owner: Option<u64>,
  writers: usize,
}

#[derive(Default)]
struct Slot {
  state: Mutex<SlotState>,
  drained: Condvar,
}

impl Slot {
  fn lock(&self) -> MutexGuard<'_, SlotState> {
    self.state.lock().unwrap_or_else(|e| e.into_inner())
  }
}

/// Per-workspace operation lock. One owned token at a time (a move from freeze to unfreeze, an
/// archive, a removal); only the holder of the token can release it. Writers take a permit for the
/// whole write and are refused while a token is held; taking the token waits for permits already out.
/// In memory only: a crash or restart starts unlocked.
#[derive(Clone, Default)]
pub struct Moving {
  slots: Arc<Mutex<HashMap<String, Arc<Slot>>>>,
  next: Arc<AtomicU64>,
}

pub struct Operation {
  moving: Moving,
  code: String,
  token: u64,
  armed: bool,
}

impl Operation {
  /// Keeps the lock past this value: the caller now owns the token and releases it explicitly.
  pub fn keep(mut self) -> u64 {
    self.armed = false;
    self.token
  }
}

impl Drop for Operation {
  fn drop(&mut self) {
    if self.armed {
      self.moving.release(&self.code, self.token);
    }
  }
}

pub struct Permit {
  slot: Arc<Slot>,
}

impl Drop for Permit {
  fn drop(&mut self) {
    let mut state = self.slot.lock();
    state.writers = state.writers.saturating_sub(1);
    if state.writers == 0 {
      self.slot.drained.notify_all();
    }
  }
}

impl Moving {
  fn slot(&self, code: &str) -> Arc<Slot> {
    let mut slots = self.slots.lock().unwrap_or_else(|e| e.into_inner());
    slots.entry(code.to_string()).or_default().clone()
  }

  pub fn is_set(&self, code: &str) -> bool {
    self.slot(code).lock().owner.is_some()
  }

  pub fn check(&self, code: &str) -> Result<(), String> {
    if self.is_set(code) {
      Err(MOVING.to_string())
    } else {
      Ok(())
    }
  }

  /// Takes the operation lock, then waits until every write already in flight has finished.
  /// Blocks while draining, so call it off the async runtime.
  pub fn begin(&self, code: &str) -> Result<Operation, String> {
    let slot = self.slot(code);
    let mut state = slot.lock();
    if state.owner.is_some() {
      return Err(BUSY.to_string());
    }
    let token = self.next.fetch_add(1, Ordering::Relaxed) + 1;
    state.owner = Some(token);
    while state.writers > 0 {
      state = slot.drained.wait(state).unwrap_or_else(|e| e.into_inner());
    }
    Ok(Operation { moving: self.clone(), code: code.to_string(), token, armed: true })
  }

  /// Held for the whole write, I/O included. Never blocks: while an operation owns the workspace it
  /// refuses.
  pub fn permit(&self, code: &str) -> Result<Permit, String> {
    let slot = self.slot(code);
    {
      let mut state = slot.lock();
      if state.owner.is_some() {
        return Err(MOVING.to_string());
      }
      state.writers += 1;
    }
    Ok(Permit { slot })
  }

  pub fn owns(&self, code: &str, token: u64) -> bool {
    self.slot(code).lock().owner == Some(token)
  }

  pub fn release(&self, code: &str, token: u64) -> bool {
    let slot = self.slot(code);
    let mut state = slot.lock();
    if state.owner == Some(token) {
      state.owner = None;
      true
    } else {
      false
    }
  }
}

fn hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn sha256_hex(data: &[u8]) -> String {
  hex(&Sha256::digest(data))
}

fn sha256_file(path: &Path) -> Result<(u64, String), String> {
  let mut file = File::open(path).map_err(|e| e.to_string())?;
  let mut hasher = Sha256::new();
  let mut buffer = vec![0u8; 64 * 1024];
  let mut size = 0u64;
  loop {
    let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
    if n == 0 {
      break;
    }
    hasher.update(&buffer[..n]);
    size += n as u64;
  }
  Ok((size, hex(&hasher.finalize())))
}

fn quote(identifier: &str) -> String {
  format!("\"{}\"", identifier.replace('"', "\"\""))
}

fn counted_table(name: &str) -> bool {
  !(name == "activity_log" || name.starts_with("plugin_") || name.starts_with("pages_fts") || name.starts_with("sqlite_"))
}

fn open_read_only(path: &Path) -> Result<Connection, String> {
  Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| e.to_string())
}

fn feed_len(hasher: &mut Sha256, bytes: &[u8]) {
  hasher.update((bytes.len() as u64).to_le_bytes());
  hasher.update(bytes);
}

fn table_names(conn: &Connection) -> Result<Vec<(String, String)>, String> {
  conn
    .prepare("SELECT name, COALESCE(sql, '') FROM sqlite_master WHERE type = 'table' ORDER BY name")
    .map_err(|e| e.to_string())?
    .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
    .map_err(|e| e.to_string())?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| e.to_string())
}

/// Every real table: no SQLite internals, no virtual table and none of its shadow tables.
fn full_digest_tables(conn: &Connection) -> Result<Vec<String>, String> {
  let tables = table_names(conn)?;
  let virtual_tables: Vec<&str> = tables
    .iter()
    .filter(|(_, sql)| sql.trim_start().to_ascii_uppercase().starts_with("CREATE VIRTUAL"))
    .map(|(name, _)| name.as_str())
    .collect();
  Ok(
    tables
      .iter()
      .map(|(name, _)| name.as_str())
      .filter(|name| !name.starts_with("sqlite_"))
      .filter(|name| !virtual_tables.iter().any(|vt| name == vt || name.starts_with(&format!("{vt}_"))))
      .map(str::to_string)
      .collect(),
  )
}

fn hash_tables(conn: &Connection, names: Vec<String>) -> Result<String, String> {
  let mut hasher = Sha256::new();
  for table in names {
    let quoted = quote(&table);
    let mut columns: Vec<(String, i64)> = conn
      .prepare(&format!("PRAGMA table_info({quoted})"))
      .map_err(|e| e.to_string())?
      .query_map([], |row| Ok((row.get::<_, String>(1)?, row.get::<_, i64>(5)?)))
      .map_err(|e| e.to_string())?
      .collect::<Result<_, _>>()
      .map_err(|e| e.to_string())?;
    hasher.update(b"T");
    feed_len(&mut hasher, table.as_bytes());
    hasher.update((columns.len() as u64).to_le_bytes());
    for (name, _) in &columns {
      feed_len(&mut hasher, name.as_bytes());
    }
    let mut keys: Vec<(String, i64)> = columns.iter().filter(|(_, pk)| *pk > 0).cloned().collect();
    keys.sort_by_key(|(_, pk)| *pk);
    columns.clear();
    let order = if keys.is_empty() {
      "rowid".to_string()
    } else {
      keys.iter().map(|(name, _)| quote(name)).collect::<Vec<_>>().join(", ")
    };
    let mut stmt = conn
      .prepare(&format!("SELECT * FROM {quoted} ORDER BY {order}"))
      .map_err(|e| e.to_string())?;
    let width = stmt.column_count();
    let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
    while let Some(row) = rows.next().map_err(|e| e.to_string())? {
      hasher.update(b"R");
      for i in 0..width {
        match row.get_ref(i).map_err(|e| e.to_string())? {
          ValueRef::Null => hasher.update(b"N"),
          ValueRef::Integer(v) => {
            hasher.update(b"I");
            hasher.update(v.to_le_bytes());
          }
          ValueRef::Real(v) => {
            hasher.update(b"F");
            hasher.update(v.to_bits().to_le_bytes());
          }
          ValueRef::Text(v) => {
            hasher.update(b"T");
            feed_len(&mut hasher, v);
          }
          ValueRef::Blob(v) => {
            hasher.update(b"B");
            feed_len(&mut hasher, v);
          }
        }
      }
    }
  }
  Ok(hex(&hasher.finalize()))
}

/// Exact content hash over every migrated table: each value carries its SQLite type tag, so a
/// changed row, type or deleted row changes it. Independent of rowids and of the file layout.
pub fn revision_of(conn: &Connection) -> Result<String, String> {
  let mut names: Vec<String> = table_names(conn)?.into_iter().map(|(name, _)| name).collect();
  names.retain(|name| counted_table(name));
  hash_tables(conn, names)
}

/// The same hash over all tables, plugin and activity data included.
pub fn db_digest_of(conn: &Connection) -> Result<String, String> {
  hash_tables(conn, full_digest_tables(conn)?)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct InventoryFile {
  pub path: String,
  pub size: u64,
  pub sha256: String,
}

/// Every regular file under `files/`, relative path with `/`, sorted. A symlink is an error: it could
/// point outside the workspace and cannot be archived faithfully.
fn walk_files(workspace_dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
  let root = workspace_dir.join("files");
  let mut out = Vec::new();
  if !root.exists() {
    return Ok(out);
  }
  let mut stack = vec![(root, String::new())];
  while let Some((dir, prefix)) = stack.pop() {
    for entry in fs::read_dir(&dir).map_err(|e| e.to_string())? {
      let entry = entry.map_err(|e| e.to_string())?;
      let name = entry.file_name().into_string().map_err(|_| "a file name is not valid UTF-8".to_string())?;
      let relative = if prefix.is_empty() { name } else { format!("{prefix}/{name}") };
      let kind = entry.file_type().map_err(|e| e.to_string())?;
      if kind.is_symlink() {
        return Err(format!("files/{relative} is a symbolic link"));
      } else if kind.is_dir() {
        stack.push((entry.path(), relative));
      } else if kind.is_file() {
        out.push((relative, entry.path()));
      }
    }
  }
  out.sort_by(|a, b| a.0.cmp(&b.0));
  Ok(out)
}

pub fn files_inventory(workspace_dir: &Path) -> Result<Vec<InventoryFile>, String> {
  walk_files(workspace_dir)?
    .into_iter()
    .map(|(path, full)| sha256_file(&full).map(|(size, sha256)| InventoryFile { path, size, sha256 }))
    .collect()
}

pub fn digest_of(db_digest: &str, files: &[InventoryFile]) -> String {
  let mut sorted: Vec<&InventoryFile> = files.iter().collect();
  sorted.sort_by(|a, b| a.path.cmp(&b.path));
  let mut hasher = Sha256::new();
  hasher.update(b"FULL");
  feed_len(&mut hasher, db_digest.as_bytes());
  hasher.update((sorted.len() as u64).to_le_bytes());
  for file in sorted {
    feed_len(&mut hasher, file.path.as_bytes());
    hasher.update(file.size.to_le_bytes());
    feed_len(&mut hasher, file.sha256.as_bytes());
  }
  hex(&hasher.finalize())
}

/// What removal compares: every table plus every file under `files/`.
pub fn full_digest(conn: &Connection, workspace_dir: &Path) -> Result<String, String> {
  Ok(digest_of(&db_digest_of(conn)?, &files_inventory(workspace_dir)?))
}

/// Stops every running timer the way the app does (`start_time` is epoch seconds).
fn stop_timers(conn: &Connection, now: i64) -> Result<usize, String> {
  let tx = rusqlite::Transaction::new_unchecked(conn, TransactionBehavior::Immediate).map_err(|e| e.to_string())?;
  let stopped = tx
    .execute(
      "UPDATE tasks SET common_time = common_time + (?1 - start_time), start_time = 0, end_time = ?1 WHERE start_time > 0",
      [now],
    )
    .map_err(|e| e.to_string())?;
  tx.commit().map_err(|e| e.to_string())?;
  Ok(stopped)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SnapshotFile {
  pub id: i64,
  pub key: String,
  pub size: u64,
  pub sha256: String,
  pub mime: String,
  pub missing: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct SnapshotRecord {
  pub revision: String,
  pub created_at: String,
  pub files: Vec<SnapshotFile>,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct Snapshot {
  pub dir: String,
  #[serde(flatten)]
  pub record: SnapshotRecord,
}

fn base_mime(mime: &str) -> String {
  mime.split(';').next().unwrap_or("").trim().to_ascii_lowercase()
}

fn resolved_mime(row_mime: Option<&str>, key: &str) -> String {
  match row_mime.map(base_mime) {
    Some(m) if !m.is_empty() => m,
    _ => base_mime(content_type(key)),
  }
}

fn safe_key(key: &str) -> bool {
  parse_target(&format!("x/{key}")).is_some_and(|(_, parsed)| parsed == key)
}

fn unique_path(base: PathBuf) -> PathBuf {
  if !base.exists() {
    return base;
  }
  let name = base.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
  (2..)
    .map(|i| match name.rsplit_once('.') {
      Some((stem, ext)) => base.with_file_name(format!("{stem}-{i}.{ext}")),
      None => base.with_file_name(format!("{name}-{i}")),
    })
    .find(|p| !p.exists())
    .unwrap()
}

fn vacuum_into(conn: &Connection, target: &Path) -> Result<(), String> {
  conn
    .execute("VACUUM INTO ?", [target.to_string_lossy().as_ref()])
    .map(|_| ())
    .map_err(|e| e.to_string())
}

fn file_rows(conn: &Connection) -> Result<Vec<(i64, String, Option<String>)>, String> {
  conn
    .prepare("SELECT id, file_path, mime_type FROM files ORDER BY id")
    .map_err(|e| e.to_string())?
    .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
    .map_err(|e| e.to_string())?
    .collect::<Result<_, _>>()
    .map_err(|e| e.to_string())
}

fn link_or_copy(from: &Path, to: &Path) -> io::Result<()> {
  if let Some(parent) = to.parent() {
    fs::create_dir_all(parent)?;
  }
  fs::hard_link(from, to).or_else(|_| fs::copy(from, to).map(|_| ()))
}

/// The part that needs the workspace connection: a consistent copy of the database, verified.
fn snapshot_db(conn: &Connection, workspace_dir: &Path, now: i64) -> Result<PathBuf, String> {
  let dir = unique_path(workspace_dir.join("backups").join(format!("cloud-move-{}", stamp(now))));
  fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
  let db = dir.join("workspace.db");
  if let Err(error) = vacuum_into(conn, &db).and_then(|_| verify(&db)) {
    let _ = fs::remove_dir_all(&dir);
    return Err(error);
  }
  Ok(dir)
}

/// Copies every referenced file next to the snapshot database and records what it found.
fn snapshot_files(dir: &Path, workspace_dir: &Path, now: i64) -> Result<Snapshot, String> {
  let result = (|| {
    let snapshot = open_read_only(&dir.join("workspace.db"))?;
    let revision = revision_of(&snapshot)?;
    let mut files = Vec::new();
    for (id, key, mime) in file_rows(&snapshot)? {
      let mime = resolved_mime(mime.as_deref(), &key);
      let source = file_path(workspace_dir, &key);
      let target = file_path(dir, &key);
      let copied = safe_key(&key) && source.is_file() && link_or_copy(&source, &target).is_ok();
      let (size, sha256) = if copied { sha256_file(&target)? } else { (0, String::new()) };
      files.push(SnapshotFile { id, key, size, sha256, mime, missing: !copied });
    }
    let record = SnapshotRecord { revision, created_at: rfc3339(now), files };
    let json = serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?;
    fs::write(dir.join("snapshot.json"), json).map_err(|e| e.to_string())?;
    Ok(Snapshot { dir: dir.to_string_lossy().into_owned(), record })
  })();
  if result.is_err() {
    let _ = fs::remove_dir_all(dir);
  }
  result
}

#[derive(Clone, Debug, PartialEq)]
struct FileInfo {
  sha256: Option<String>,
  excluded: Option<&'static str>,
}

fn mime_allowed(mime: &str, allowlist: &[String]) -> bool {
  allowlist.is_empty()
    || allowlist.iter().any(|allowed| match allowed.strip_suffix('*') {
      Some(prefix) => mime.starts_with(prefix),
      None => allowed == mime,
    })
}

fn file_infos(
  conn: &Connection,
  dir: &Path,
  max_file_bytes: u64,
  allowlist: &[String],
) -> Result<HashMap<i64, FileInfo>, String> {
  let allowlist: Vec<String> = allowlist.iter().map(|m| base_mime(m)).collect();
  let mut infos = HashMap::new();
  for (id, key, mime) in file_rows(conn)? {
    let path = file_path(dir, &key);
    let info = if !safe_key(&key) || !path.is_file() {
      FileInfo { sha256: None, excluded: Some("missing") }
    } else {
      let (size, sha256) = sha256_file(&path)?;
      let excluded = if size > max_file_bytes {
        Some("too_large")
      } else if !mime_allowed(&resolved_mime(mime.as_deref(), &key), &allowlist) {
        Some("mime")
      } else {
        None
      };
      FileInfo { sha256: Some(sha256), excluded }
    };
    infos.insert(id, info);
  }
  Ok(infos)
}

fn json_text(value: &impl Serialize) -> String {
  serde_json::to_string(value).unwrap_or_else(|_| "null".to_string())
}

fn value_json(value: ValueRef<'_>) -> Json {
  match value {
    ValueRef::Null => Json::Null,
    ValueRef::Integer(i) => Json::Number(i.into()),
    ValueRef::Real(f) => Number::from_f64(f).map(Json::Number).unwrap_or(Json::Null),
    ValueRef::Text(t) => Json::String(String::from_utf8_lossy(t).into_owned()),
    ValueRef::Blob(b) => {
      use base64::Engine;
      Json::String(base64::engine::general_purpose::STANDARD.encode(b))
    }
  }
}

/// Parents first: breadth-first from the roots, ids ascending; rows caught in a cycle follow last.
fn tree_order(conn: &Connection, table: &str) -> Result<Vec<i64>, String> {
  let pairs: Vec<(i64, Option<i64>)> = conn
    .prepare(&format!("SELECT id, parent_id FROM {} ORDER BY id", quote(table)))
    .map_err(|e| e.to_string())?
    .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
    .map_err(|e| e.to_string())?
    .collect::<Result<_, _>>()
    .map_err(|e| e.to_string())?;
  let known: HashSet<i64> = pairs.iter().map(|(id, _)| *id).collect();
  let mut children: HashMap<i64, Vec<i64>> = HashMap::new();
  let mut queue: VecDeque<i64> = VecDeque::new();
  for (id, parent) in &pairs {
    match parent {
      Some(p) if known.contains(p) && p != id => children.entry(*p).or_default().push(*id),
      _ => queue.push_back(*id),
    }
  }
  let mut order = Vec::with_capacity(pairs.len());
  let mut seen = HashSet::new();
  while let Some(id) = queue.pop_front() {
    if seen.insert(id) {
      order.push(id);
      if let Some(kids) = children.get(&id) {
        queue.extend(kids.iter().copied());
      }
    }
  }
  for (id, _) in &pairs {
    if seen.insert(*id) {
      order.push(*id);
    }
  }
  Ok(order)
}

fn row_line(
  entity: &str,
  names: &[String],
  row: &rusqlite::Row<'_>,
  extra: &[(&str, Json)],
) -> Result<String, String> {
  let mut out = format!("{{\"t\":{},\"r\":{{", json_text(&entity));
  for (i, name) in names.iter().enumerate() {
    if i > 0 {
      out.push(',');
    }
    out.push_str(&json_text(name));
    out.push(':');
    out.push_str(&json_text(&value_json(row.get_ref(i).map_err(|e| e.to_string())?)));
  }
  for (name, value) in extra {
    out.push(',');
    out.push_str(&json_text(name));
    out.push(':');
    out.push_str(&json_text(value));
  }
  out.push_str("}}");
  Ok(out)
}

fn for_each_line(
  conn: &Connection,
  entity: &str,
  files: &HashMap<i64, FileInfo>,
  sink: &mut dyn FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
  let table = quote(entity);
  let tree = matches!(entity, "categories" | "pages");
  let (filter, order) = match entity {
    "page_versions" => ("", "page_id, version"),
    "task_persona_assignees" => ("", "task_id, persona_uuid"),
    "personas" => ("", "uuid"),
    "workspace_personas" => ("", "persona_uuid"),
    "meta" => ("WHERE substr(key, 1, 8) = 'feature.'", "key"),
    _ => ("", "id"),
  };
  let extras = |names: &[String], row: &rusqlite::Row<'_>| -> Result<Vec<(&'static str, Json)>, String> {
    if entity != "files" {
      return Ok(Vec::new());
    }
    let index = names.iter().position(|n| n == "id").ok_or("files has no id column")?;
    let id: i64 = row.get(index).map_err(|e| e.to_string())?;
    let info = files.get(&id);
    let mut extra = Vec::new();
    if let Some(sha) = info.and_then(|i| i.sha256.clone()) {
      extra.push(("sha256", Json::String(sha)));
    }
    if let Some(reason) = info.and_then(|i| i.excluded) {
      extra.push(("excluded", Json::String(reason.to_string())));
    }
    Ok(extra)
  };

  if tree {
    let mut stmt = conn
      .prepare(&format!("SELECT * FROM {table} WHERE id = ?1"))
      .map_err(|e| e.to_string())?;
    let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    for id in tree_order(conn, entity)? {
      let mut rows = stmt.query([id]).map_err(|e| e.to_string())?;
      if let Some(row) = rows.next().map_err(|e| e.to_string())? {
        sink(&row_line(entity, &names, row, &extras(&names, row)?)?)?;
      }
    }
    return Ok(());
  }
  let mut stmt = conn
    .prepare(&format!("SELECT * FROM {table} {filter} ORDER BY {order}"))
    .map_err(|e| e.to_string())?;
  let names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
  let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
  while let Some(row) = rows.next().map_err(|e| e.to_string())? {
    sink(&row_line(entity, &names, row, &extras(&names, row)?)?)?;
  }
  Ok(())
}

#[derive(Debug, Serialize)]
pub struct BundleResult {
  pub manifest: Json,
  pub bytes: u64,
  pub path: String,
}

pub struct BundleInput<'a> {
  pub code: &'a str,
  pub name: &'a str,
  pub profile_uuid: &'a str,
  pub max_file_bytes: u64,
  pub mime_allowlist: &'a [String],
}

pub fn fingerprint(profile_uuid: &str, code: &str, revision: &str) -> String {
  sha256_hex(format!("{profile_uuid}{code}{revision}").as_bytes())
}

/// Writes `bundle.ndjson.gz` from the snapshot database and the files copied next to it.
pub fn build_bundle(dir: &Path, input: &BundleInput<'_>) -> Result<BundleResult, String> {
  let conn = open_read_only(&dir.join("workspace.db"))?;
  let revision = revision_of(&conn)?;
  let local_schema: u32 = conn
    .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |row| row.get::<_, String>(0))
    .map_err(|e| e.to_string())?
    .parse()
    .map_err(|_| "schema_version is not a number".to_string())?;
  let snapshot_at = fs::read_to_string(dir.join("snapshot.json"))
    .ok()
    .and_then(|raw| serde_json::from_str::<SnapshotRecord>(&raw).ok())
    .map(|record| record.created_at)
    .unwrap_or_else(|| rfc3339(crate::tray::now_secs()));
  let files = file_infos(&conn, dir, input.max_file_bytes, input.mime_allowlist)?;

  let mut counts = Vec::new();
  let mut hashes = Vec::new();
  for entity in ENTITIES {
    let mut hasher = Sha256::new();
    let mut count = 0usize;
    for_each_line(&conn, entity, &files, &mut |line| {
      if count > 0 {
        hasher.update(b"\n");
      }
      hasher.update(line.as_bytes());
      count += 1;
      Ok(())
    })?;
    counts.push(count);
    hashes.push(hex(&hasher.finalize()));
  }

  let pairs = |values: Vec<String>| {
    ENTITIES
      .iter()
      .zip(values)
      .map(|(entity, value)| format!("{}:{value}", json_text(entity)))
      .collect::<Vec<_>>()
      .join(",")
  };
  let manifest_line = format!(
    "{{\"t\":\"manifest\",\"bundle_schema\":{BUNDLE_SCHEMA},\"local_schema\":{local_schema},\"source_revision\":{},\"fingerprint\":{},\"workspace\":{{\"code\":{},\"name\":{}}},\"profile_uuid\":{},\"snapshot_at\":{},\"counts\":{{{}}},\"hashes\":{{{}}}}}",
    json_text(&revision),
    json_text(&fingerprint(input.profile_uuid, input.code, &revision)),
    json_text(&input.code),
    json_text(&input.name),
    json_text(&input.profile_uuid),
    json_text(&snapshot_at),
    pairs(counts.iter().map(|c| c.to_string()).collect()),
    pairs(hashes.iter().map(|h| json_text(h)).collect()),
  );

  let mut excluded: Vec<ExcludedRecord> = file_rows(&conn)?
    .into_iter()
    .filter_map(|(id, key, _)| {
      files.get(&id).and_then(|info| info.excluded).map(|reason| ExcludedRecord { key, reason: reason.to_string() })
    })
    .collect();
  if let Ok(previous) = read_json::<BundleRecord>(&dir.join(BUNDLE_RECORD), "bundle record") {
    for entry in previous.excluded.into_iter().filter(|e| e.reason == "refused") {
      if !excluded.iter().any(|e| e.key == entry.key) {
        excluded.push(entry);
      }
    }
  }
  write_json(&dir.join(BUNDLE_RECORD), &BundleRecord { excluded })?;

  let target = dir.join("bundle.ndjson.gz");
  let tmp = dir.join(".bundle.ndjson.gz.tmp");
  let written = (|| {
    let file = File::create(&tmp).map_err(|e| e.to_string())?;
    let mut gz = flate2::write::GzEncoder::new(BufWriter::new(file), flate2::Compression::default());
    writeln!(gz, "{manifest_line}").map_err(|e| e.to_string())?;
    for entity in ENTITIES {
      for_each_line(&conn, entity, &files, &mut |line| writeln!(gz, "{line}").map_err(|e| e.to_string()))?;
    }
    gz.finish().and_then(|mut w| w.flush()).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &target).map_err(|e| e.to_string())
  })();
  if let Err(error) = written {
    let _ = fs::remove_file(&tmp);
    return Err(error);
  }
  Ok(BundleResult {
    manifest: serde_json::from_str(&manifest_line).map_err(|e| e.to_string())?,
    bytes: fs::metadata(&target).map_err(|e| e.to_string())?.len(),
    path: target.to_string_lossy().into_owned(),
  })
}

#[derive(Debug, Serialize, PartialEq)]
pub struct PutResult {
  pub status: u16,
  pub body: String,
}

pub async fn put_file(url: &str, headers: &HashMap<String, String>, path: &Path) -> Result<PutResult, String> {
  if rustls::crypto::CryptoProvider::get_default().is_none() {
    let _ = rustls::crypto::ring::default_provider().install_default();
  }
  let file = tokio::fs::File::open(path).await.map_err(|e| e.to_string())?;
  let length = file.metadata().await.map_err(|e| e.to_string())?.len();
  let client = reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none())
    .connect_timeout(std::time::Duration::from_secs(20))
    .build()
    .map_err(|e| e.to_string())?;
  let mut request = client.put(url).header(reqwest::header::CONTENT_LENGTH, length);
  for (name, value) in headers {
    if !name.eq_ignore_ascii_case("content-length") {
      request = request.header(name, value);
    }
  }
  let mut response = request.body(reqwest::Body::from(file)).send().await.map_err(|e| e.to_string())?;
  let status = response.status().as_u16();
  let mut body = Vec::new();
  while body.len() < PUT_BODY_LIMIT {
    match response.chunk().await.map_err(|e| e.to_string())? {
      Some(chunk) => body.extend_from_slice(&chunk),
      None => break,
    }
  }
  body.truncate(PUT_BODY_LIMIT);
  Ok(PutResult { status, body: String::from_utf8_lossy(&body).into_owned() })
}

fn inside(parent: &Path, path: &Path) -> bool {
  match (fs::canonicalize(parent), fs::canonicalize(path)) {
    (Ok(parent), Ok(path)) => path.starts_with(parent),
    _ => false,
  }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ArchiveRecord {
  pub revision: String,
  pub full_digest: String,
  pub created_at: String,
  pub files: Vec<InventoryFile>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ArchiveResult {
  pub path: String,
  pub revision: String,
  pub full_digest: String,
  pub entries: usize,
  pub verified: bool,
}

const ARCHIVE_DB: &str = "workspace.db";
const ARCHIVE_META: &str = "archive.json";
const ARCHIVE_FILES: &str = "files/";

fn zip_options(size: u64) -> zip::write::SimpleFileOptions {
  zip::write::SimpleFileOptions::default()
    .compression_method(zip::CompressionMethod::Deflated)
    .large_file(size >= u32::MAX as u64)
}

fn add_entry(
  zip: &mut zip::ZipWriter<BufWriter<File>>,
  name: &str,
  source: &Path,
) -> Result<(u64, String), String> {
  let mut file = File::open(source).map_err(|e| e.to_string())?;
  let size = file.metadata().map_err(|e| e.to_string())?.len();
  zip.start_file(name, zip_options(size)).map_err(|e| e.to_string())?;
  let mut hasher = Sha256::new();
  let mut buffer = vec![0u8; 64 * 1024];
  let mut written = 0u64;
  loop {
    let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
    if n == 0 {
      break;
    }
    hasher.update(&buffer[..n]);
    zip.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
    written += n as u64;
  }
  Ok((written, hex(&hasher.finalize())))
}

/// Writes the database copy and every file under `files/` (not only the ones the database mentions)
/// and records what went in. The caller holds the workspace's operation lock.
fn write_archive(workspace_dir: &Path, db_copy: &Path, now: i64) -> Result<ArchiveResult, String> {
  let backups = workspace_dir.join("backups");
  let tmp = backups.join(format!(".archive-{}.zip.tmp", stamp(now)));
  let built = (|| {
    let copy = open_read_only(db_copy)?;
    let revision = revision_of(&copy)?;
    let db_digest = db_digest_of(&copy)?;
    drop(copy);
    let mut zip = zip::ZipWriter::new(BufWriter::new(File::create(&tmp).map_err(|e| e.to_string())?));
    add_entry(&mut zip, ARCHIVE_DB, db_copy)?;
    let mut files = Vec::new();
    for (path, source) in walk_files(workspace_dir)? {
      let (size, sha256) = add_entry(&mut zip, &format!("{ARCHIVE_FILES}{path}"), &source)?;
      files.push(InventoryFile { path, size, sha256 });
    }
    let full_digest = digest_of(&db_digest, &files);
    let record = ArchiveRecord { revision, full_digest, created_at: rfc3339(now), files };
    zip.start_file(ARCHIVE_META, zip_options(0)).map_err(|e| e.to_string())?;
    zip.write_all(serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| e.to_string())?;
    zip.finish().and_then(|mut w| w.flush().map_err(zip::result::ZipError::Io)).map_err(|e| e.to_string())?;
    let verified = verify_archive(&tmp)?;
    let target = unique_path(backups.join(format!("archive-{}.zip", stamp(now))));
    fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
    Ok(ArchiveResult {
      path: target.to_string_lossy().into_owned(),
      revision: verified.0.revision,
      full_digest: verified.0.full_digest,
      entries: verified.1,
      verified: true,
    })
  })();
  if built.is_err() {
    let _ = fs::remove_file(&tmp);
  }
  built
}

fn safe_entry_path(name: &str) -> bool {
  !name.is_empty()
    && !name.contains('\\')
    && name.split('/').all(|segment| !segment.is_empty() && segment != "." && segment != ".." && !segment.contains(':'))
}

/// Re-reads the whole archive: entry names (nothing but the database, the record and `files/...`),
/// every file against the recorded list (count, size, hash, nothing extra, nothing missing), then the
/// database copy (integrity, revision) and the full digest recomputed from the extracted content.
pub fn verify_archive(path: &Path) -> Result<(ArchiveRecord, usize), String> {
  let mut archive = zip::ZipArchive::new(File::open(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
  let entries = archive.len();
  let record: ArchiveRecord = {
    let mut raw = String::new();
    archive
      .by_name(ARCHIVE_META)
      .map_err(|_| format!("{ARCHIVE_META} is missing"))?
      .read_to_string(&mut raw)
      .map_err(|e| e.to_string())?;
    serde_json::from_str(&raw).map_err(|e| e.to_string())?
  };
  let mut recorded: HashMap<&str, &InventoryFile> = HashMap::new();
  for file in &record.files {
    if !safe_entry_path(&file.path) || recorded.insert(file.path.as_str(), file).is_some() {
      return Err(format!("the record lists an invalid or duplicate path: {}", file.path));
    }
  }
  let mut names: HashSet<String> = HashSet::new();
  let mut seen: HashMap<String, (u64, String)> = HashMap::new();
  let mut db_tmp: Option<PathBuf> = None;
  let outcome = (|| {
    for index in 0..entries {
      let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
      let name = entry.name().map_err(|e| e.to_string())?.to_string();
      if name.ends_with('/') {
        continue;
      }
      let relative = name.strip_prefix(ARCHIVE_FILES);
      let allowed = name == ARCHIVE_DB || name == ARCHIVE_META || relative.is_some_and(safe_entry_path);
      if !allowed {
        return Err(format!("unexpected entry {name}"));
      }
      if !names.insert(name.clone()) {
        return Err(format!("duplicate entry {name}"));
      }
      if name == ARCHIVE_META {
        continue;
      }
      if name == ARCHIVE_DB {
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        let tmp = dir.join(format!(".verify-{}-{}.db", std::process::id(), index));
        db_tmp = Some(tmp.clone());
        let mut out = File::create(&tmp).map_err(|e| e.to_string())?;
        io::copy(&mut entry, &mut out).map_err(|e| format!("{name}: {e}"))?;
        continue;
      }
      let mut hasher = Sha256::new();
      let mut buffer = vec![0u8; 64 * 1024];
      let mut size = 0u64;
      loop {
        let n = entry.read(&mut buffer).map_err(|e| format!("{name}: {e}"))?;
        if n == 0 {
          break;
        }
        hasher.update(&buffer[..n]);
        size += n as u64;
      }
      seen.insert(relative.unwrap_or_default().to_string(), (size, hex(&hasher.finalize())));
    }
    if seen.len() != record.files.len() {
      return Err(format!("the archive holds {} files but the record lists {}", seen.len(), record.files.len()));
    }
    for file in &record.files {
      match seen.get(&file.path) {
        Some((size, sha)) if *size == file.size && *sha == file.sha256 => {}
        Some(_) => return Err(format!("files/{} does not match its recorded size or hash", file.path)),
        None => return Err(format!("files/{} is missing", file.path)),
      }
    }
    let tmp = db_tmp.as_ref().ok_or_else(|| format!("{ARCHIVE_DB} is missing"))?;
    verify(tmp)?;
    let extracted = open_read_only(tmp)?;
    if revision_of(&extracted)? != record.revision {
      return Err("database revision does not match the archive record".to_string());
    }
    if digest_of(&db_digest_of(&extracted)?, &record.files) != record.full_digest {
      return Err("archive digest does not match the archive record".to_string());
    }
    Ok(())
  })();
  if let Some(tmp) = db_tmp {
    let _ = fs::remove_file(tmp);
  }
  outcome.map(|_| (record, entries))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ExcludedRecord {
  pub key: String,
  pub reason: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct BundleRecord {
  pub excluded: Vec<ExcludedRecord>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Decision {
  pub decision: String,
  pub size: u64,
  pub sha256: String,
}

type Decisions = BTreeMap<String, Decision>;

const BUNDLE_RECORD: &str = "bundle.json";
const DECISIONS: &str = "decisions.json";
const DECIDED: &str = "exported";
const LOST: &str = "lost";

static RECORD_LOCK: Mutex<()> = Mutex::new(());

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path, what: &str) -> Result<T, String> {
  let raw = fs::read_to_string(path).map_err(|_| format!("{what} not found"))?;
  serde_json::from_str(&raw).map_err(|e| format!("{what} is unreadable: {e}"))
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<(), String> {
  let tmp = path.with_extension("json.tmp");
  fs::write(&tmp, serde_json::to_string_pretty(value).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
  fs::rename(&tmp, path).map_err(|e| e.to_string())
}

fn read_decisions(dir: &Path) -> Result<Decisions, String> {
  if dir.join(DECISIONS).exists() {
    read_json(&dir.join(DECISIONS), "decisions record")
  } else {
    Ok(Decisions::new())
  }
}

fn change_decisions(dir: &Path, change: impl FnOnce(&mut Decisions) -> Result<(), String>) -> Result<(), String> {
  let _guard = RECORD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
  let mut decisions = read_decisions(dir)?;
  change(&mut decisions)?;
  write_json(&dir.join(DECISIONS), &decisions)
}

fn snapshot_record(dir: &Path) -> Result<SnapshotRecord, String> {
  read_json(&dir.join("snapshot.json"), "snapshot record")
}

fn snapshot_file(dir: &Path, key: &str) -> Result<SnapshotFile, String> {
  snapshot_record(dir)?
    .files
    .into_iter()
    .find(|file| file.key == key)
    .ok_or_else(|| format!("{key} is not part of this snapshot"))
}

fn add_excluded(dir: &Path, keys: &[String], reason: &str) -> Result<(), String> {
  let _guard = RECORD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
  let path = dir.join(BUNDLE_RECORD);
  let mut record: BundleRecord = read_json(&path, "bundle record")?;
  for key in keys {
    snapshot_file(dir, key)?;
    if !record.excluded.iter().any(|e| e.key == *key) {
      record.excluded.push(ExcludedRecord { key: key.clone(), reason: reason.to_string() });
    }
  }
  write_json(&path, &record)
}

/// Copies a snapshot file to the place the user chose and records `exported` only after the written
/// copy has the recorded size and hash.
pub fn export_file(workspace_dir: &Path, dir: &Path, key: &str, target: &Path, forbidden_root: &Path) -> Result<Decision, String> {
  let file = snapshot_file(dir, key)?;
  if file.missing {
    return Err("this file is missing from the workspace and cannot be exported; accept it as lost instead".to_string());
  }
  if !target.is_absolute() || target.is_dir() {
    return Err("choose a file path to export to".to_string());
  }
  let parent = target.parent().ok_or("choose a file path to export to")?;
  let parent = fs::canonicalize(parent).map_err(|_| "the chosen folder does not exist".to_string())?;
  if fs::canonicalize(forbidden_root).is_ok_and(|root| parent.starts_with(root)) {
    return Err("export the file outside the local workspaces folder".to_string());
  }
  let source = [file_path(dir, key), file_path(workspace_dir, key)]
    .into_iter()
    .find(|candidate| sha256_file(candidate).is_ok_and(|(size, sha)| size == file.size && sha == file.sha256))
    .ok_or_else(|| "the file no longer matches the snapshot".to_string())?;
  let target = parent.join(target.file_name().ok_or("choose a file path to export to")?);
  let copied = fs::copy(&source, &target)
    .map_err(|e| e.to_string())
    .and_then(|_| sha256_file(&target));
  match copied {
    Ok((size, sha)) if size == file.size && sha == file.sha256 => {}
    Ok(_) => {
      let _ = fs::remove_file(&target);
      return Err("the exported copy does not match the original".to_string());
    }
    Err(error) => {
      let _ = fs::remove_file(&target);
      return Err(error);
    }
  }
  let decision = Decision { decision: DECIDED.to_string(), size: file.size, sha256: file.sha256 };
  change_decisions(dir, |decisions| {
    decisions.insert(key.to_string(), decision.clone());
    Ok(())
  })?;
  Ok(decision)
}

pub fn accept_lost(dir: &Path, key: &str) -> Result<(), String> {
  let file = snapshot_file(dir, key)?;
  change_decisions(dir, |decisions| {
    decisions.insert(key.to_string(), Decision { decision: LOST.to_string(), size: file.size, sha256: file.sha256 });
    Ok(())
  })
}

pub fn clear_decision(dir: &Path, key: &str) -> Result<(), String> {
  change_decisions(dir, |decisions| {
    decisions.remove(key);
    Ok(())
  })
}

/// Every file the cloud copy lacks (missing from disk, excluded when the bundle was built, or refused
/// by the server) needs a decision before the local copy may go.
fn undecided_files(dir: &Path) -> Result<Vec<String>, String> {
  let snapshot = snapshot_record(dir)?;
  let bundle: BundleRecord = read_json(&dir.join(BUNDLE_RECORD), "bundle record")?;
  let decisions = read_decisions(dir)?;
  let mut needed: Vec<String> = snapshot.files.iter().filter(|f| f.missing).map(|f| f.key.clone()).collect();
  needed.extend(bundle.excluded.iter().map(|e| e.key.clone()));
  needed.sort();
  needed.dedup();
  Ok(needed.into_iter().filter(|key| !decisions.contains_key(key)).collect())
}

pub fn publication_verdict(body: &Json, expected_revision: &str) -> Result<(), String> {
  let data = body.get("data").unwrap_or(&Json::Null);
  if data.get("status").and_then(Json::as_str) != Some("published") {
    return Err("the cloud copy is not published".to_string());
  }
  if data.get("verified").and_then(Json::as_bool) != Some(true) {
    return Err("the cloud copy was not verified (it was published with accepted differences)".to_string());
  }
  if data.get("source_revision").and_then(Json::as_str) != Some(expected_revision) {
    return Err("the cloud copy was made from a different revision of this workspace".to_string());
  }
  Ok(())
}

const JOB_BODY_LIMIT: usize = 1024 * 1024;

fn ensure_tls_provider() {
  if rustls::crypto::CryptoProvider::get_default().is_none() {
    let _ = rustls::crypto::ring::default_provider().install_default();
  }
}

fn job_url(api_base: &str, job_uuid: &str) -> Result<String, String> {
  let local = ["http://127.0.0.1", "http://localhost", "http://[::1]"].iter().any(|prefix| api_base.starts_with(prefix));
  if !(api_base.starts_with("https://") || local) {
    return Err("the server address must use https".to_string());
  }
  if job_uuid.is_empty() || !job_uuid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
    return Err("the move id is not valid".to_string());
  }
  let base = if api_base.ends_with('/') { api_base.to_string() } else { format!("{api_base}/") };
  Ok(format!("{base}imports/{job_uuid}"))
}

/// Reads the job from the server with the caller's token. The token is only ever put in the request
/// header: never logged, stored or echoed in an error.
pub async fn fetch_job(api_base: &str, job_uuid: &str, token: &str) -> Result<Json, String> {
  let url = job_url(api_base, job_uuid)?;
  ensure_tls_provider();
  let client = reqwest::Client::builder()
    .redirect(reqwest::redirect::Policy::none())
    .timeout(std::time::Duration::from_secs(15))
    .build()
    .map_err(|e| e.without_url().to_string())?;
  let mut response = client
    .get(url)
    .header(reqwest::header::AUTHORIZATION, format!("Bearer {token}"))
    .header(reqwest::header::ACCEPT, "application/json")
    .send()
    .await
    .map_err(|e| format!("could not reach the server to check the move: {}", e.without_url()))?;
  let status = response.status();
  if !status.is_success() {
    return Err(format!("the server answered HTTP {} when checking the move", status.as_u16()));
  }
  let mut body = Vec::new();
  while body.len() < JOB_BODY_LIMIT {
    match response.chunk().await.map_err(|e| e.without_url().to_string())? {
      Some(chunk) => body.extend_from_slice(&chunk),
      None => break,
    }
  }
  serde_json::from_slice(&body).map_err(|_| "the server answer is not readable".to_string())
}

fn removed_root(workspaces_root: &Path) -> PathBuf {
  workspaces_root.parent().unwrap_or(workspaces_root).join("removed")
}

pub struct RemoveRequest<'a> {
  pub typed_name: &'a str,
  pub expected_revision: &'a str,
  pub archive_path: &'a Path,
  pub dir: &'a Path,
}

/// Every precondition, each with its own error. The live revision and full digest are computed here,
/// from the live data, while the caller holds the workspace's operation lock. Returns the workspace folder.
pub fn check_removal(
  workspaces_root: &Path,
  code: &str,
  request: &RemoveRequest<'_>,
  live: Option<&Connection>,
) -> Result<PathBuf, String> {
  let workspace = local_workspaces::list_in(workspaces_root)
    .into_iter()
    .find(|w| w.manifest.code == code)
    .ok_or_else(|| format!("local workspace {code} not found"))?;
  if request.typed_name != workspace.manifest.name {
    return Err("typed name does not match the workspace name".to_string());
  }
  let workspace_dir = PathBuf::from(&workspace.path);
  let owned;
  let conn = match live {
    Some(conn) => conn,
    None => {
      owned = open_read_only(&workspace_dir.join("workspace.db"))?;
      &owned
    }
  };
  if revision_of(conn)? != request.expected_revision {
    return Err("the workspace changed since the move snapshot".to_string());
  }
  if !request.archive_path.exists() {
    return Err("archive file not found".to_string());
  }
  if !inside(&workspace_dir.join("backups"), request.archive_path) {
    return Err("archive is not inside this workspace's backups folder".to_string());
  }
  let (record, _) = verify_archive(request.archive_path).map_err(|e| format!("archive failed verification: {e}"))?;
  if record.revision != request.expected_revision {
    return Err("archive was made from a different revision".to_string());
  }
  if full_digest(conn, &workspace_dir)? != record.full_digest {
    return Err("the workspace changed since the archive was made; create a new archive".to_string());
  }
  if !inside(&workspace_dir.join("backups"), request.dir) {
    return Err("snapshot folder is not inside this workspace's backups".to_string());
  }
  if snapshot_record(request.dir)?.revision != request.expected_revision {
    return Err("the snapshot folder belongs to a different revision".to_string());
  }
  let undecided = undecided_files(request.dir)?;
  if !undecided.is_empty() {
    return Err(format!(
      "{} file(s) that are not in the cloud copy are neither exported nor accepted as lost: {}",
      undecided.len(),
      undecided.join(", ")
    ));
  }
  Ok(workspace_dir)
}

/// Moves a workspace folder to `removed/<code>-<stamp>/` and deletes its database and files there;
/// `backups/` and `workspace.json` stay.
pub fn remove_folder(workspaces_root: &Path, code: &str, workspace_dir: &Path, now: i64) -> Result<PathBuf, String> {
  let removed = removed_root(workspaces_root);
  fs::create_dir_all(&removed).map_err(|e| e.to_string())?;
  let target = unique_path(removed.join(format!("{code}-{}", stamp(now))));
  fs::rename(workspace_dir, &target).map_err(|e| e.to_string())?;
  for suffix in ["", "-wal", "-shm", "-journal"] {
    let _ = fs::remove_file(target.join(format!("workspace.db{suffix}")));
  }
  let _ = fs::remove_dir_all(target.join("files"));
  Ok(target)
}

#[cfg(test)]
pub fn remove_in(
  workspaces_root: &Path,
  code: &str,
  request: &RemoveRequest<'_>,
  now: i64,
) -> Result<PathBuf, String> {
  let workspace_dir = check_removal(workspaces_root, code, request, None)?;
  remove_folder(workspaces_root, code, &workspace_dir, now)
}

fn workspace_and_dir<R: Runtime>(app: &AppHandle<R>, code: &str, dir: &str) -> Result<(LocalWorkspace, PathBuf), String> {
  let workspace = local_workspaces::find(app, code)?;
  let dir = PathBuf::from(dir);
  let is_move_dir = dir.file_name().is_some_and(|n| n.to_string_lossy().starts_with("cloud-move-"));
  if !is_move_dir || !inside(&Path::new(&workspace.path).join("backups"), &dir) {
    return Err("snapshot folder is not inside this workspace's backups".to_string());
  }
  Ok((workspace, dir))
}

#[derive(Debug, Serialize)]
pub struct Frozen {
  pub stopped: usize,
  pub token: String,
}

async fn begin_operation(moving: &Moving, code: &str) -> Result<Operation, String> {
  let moving = moving.clone();
  let code = code.to_string();
  tauri::async_runtime::spawn_blocking(move || moving.begin(&code))
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn local_move_freeze<R: Runtime>(
  app: AppHandle<R>,
  dbs: State<'_, LocalDbs>,
  moving: State<'_, Moving>,
  code: String,
) -> Result<Frozen, String> {
  let operation = begin_operation(&moving, &code).await?;
  let now = crate::tray::now_secs();
  let stopped = with_connection(app, &dbs, code.clone(), move |conn| stop_timers(conn, now)).await?;
  log::info!("[local] froze {code} for a cloud move, stopped {stopped} timers");
  Ok(Frozen { stopped, token: operation.keep().to_string() })
}

#[tauri::command]
pub fn local_move_unfreeze(moving: State<'_, Moving>, code: String, token: String) {
  if let Ok(token) = token.parse::<u64>() {
    moving.release(&code, token);
  }
}

#[tauri::command]
pub async fn local_source_revision<R: Runtime>(
  app: AppHandle<R>,
  dbs: State<'_, LocalDbs>,
  code: String,
) -> Result<String, String> {
  with_connection(app, &dbs, code, |conn| revision_of(conn)).await
}

#[derive(Debug, Serialize)]
pub struct SourceDigest {
  pub revision: String,
  pub full_digest: String,
}

#[tauri::command]
pub async fn local_source_digest<R: Runtime>(
  app: AppHandle<R>,
  dbs: State<'_, LocalDbs>,
  code: String,
) -> Result<SourceDigest, String> {
  let workspace_dir = PathBuf::from(local_workspaces::find(&app, &code)?.path);
  with_connection(app, &dbs, code, move |conn| {
    Ok(SourceDigest { revision: revision_of(conn)?, full_digest: full_digest(conn, &workspace_dir)? })
  })
  .await
}

#[tauri::command]
pub async fn local_move_snapshot<R: Runtime>(
  app: AppHandle<R>,
  dbs: State<'_, LocalDbs>,
  moving: State<'_, Moving>,
  code: String,
  token: String,
) -> Result<Snapshot, String> {
  if !token.parse::<u64>().is_ok_and(|token| moving.owns(&code, token)) {
    return Err("the workspace must be frozen before a snapshot".to_string());
  }
  let workspace = local_workspaces::find(&app, &code)?;
  let workspace_dir = PathBuf::from(&workspace.path);
  let now = crate::tray::now_secs();
  let target = workspace_dir.clone();
  let dir = with_connection(app, &dbs, code, move |conn| snapshot_db(conn, &target, now)).await?;
  tauri::async_runtime::spawn_blocking(move || snapshot_files(&dir, &workspace_dir, now))
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn local_move_build_bundle<R: Runtime>(
  app: AppHandle<R>,
  code: String,
  dir: String,
  profile_uuid: String,
  name: String,
  max_file_bytes: u64,
  mime_allowlist: Vec<String>,
) -> Result<BundleResult, String> {
  let (_, dir) = workspace_and_dir(&app, &code, &dir)?;
  tauri::async_runtime::spawn_blocking(move || {
    build_bundle(
      &dir,
      &BundleInput { code: &code, name: &name, profile_uuid: &profile_uuid, max_file_bytes, mime_allowlist: &mime_allowlist },
    )
  })
  .await
  .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn local_move_put<R: Runtime>(
  app: AppHandle<R>,
  url: String,
  headers: HashMap<String, String>,
  path: String,
) -> Result<PutResult, String> {
  let path = PathBuf::from(path);
  if !inside(&local_workspaces::root(&app)?, &path) {
    return Err("only files inside the local workspaces folder can be uploaded".to_string());
  }
  put_file(&url, &headers, &path).await
}

#[tauri::command]
pub async fn local_move_archive<R: Runtime>(
  app: AppHandle<R>,
  dbs: State<'_, LocalDbs>,
  moving: State<'_, Moving>,
  code: String,
) -> Result<ArchiveResult, String> {
  let workspace = local_workspaces::find(&app, &code)?;
  let workspace_dir = PathBuf::from(&workspace.path);
  let _operation = begin_operation(&moving, &code).await?;
  let now = crate::tray::now_secs();
  let backups = workspace_dir.join("backups");
  fs::create_dir_all(&backups).map_err(|e| e.to_string())?;
  let db_copy = backups.join(format!(".archive-{}.db.tmp", stamp(now)));
  let _ = fs::remove_file(&db_copy);
  let target = db_copy.clone();
  with_connection(app, &dbs, code, move |conn| vacuum_into(conn, &target)).await?;
  let result = tauri::async_runtime::spawn_blocking({
    let db_copy = db_copy.clone();
    move || write_archive(&workspace_dir, &db_copy, now)
  })
  .await
  .map_err(|e| e.to_string())?;
  let _ = fs::remove_file(&db_copy);
  result
}

#[tauri::command]
pub async fn local_move_export_file<R: Runtime>(
  app: AppHandle<R>,
  code: String,
  dir: String,
  key: String,
  target_path: String,
) -> Result<(), String> {
  let (workspace, dir) = workspace_and_dir(&app, &code, &dir)?;
  let root = local_workspaces::root(&app)?;
  tauri::async_runtime::spawn_blocking(move || {
    export_file(Path::new(&workspace.path), &dir, &key, Path::new(&target_path), &root).map(|_| ())
  })
  .await
  .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn local_move_pick_export_target<R: Runtime>(
  app: AppHandle<R>,
  file_name: String,
) -> Result<Option<String>, String> {
  use tauri_plugin_dialog::DialogExt;
  let name: String = file_name.chars().filter(|c| !matches!(c, '/' | '\\')).take(160).collect();
  let picked = tauri::async_runtime::spawn_blocking(move || app.dialog().file().set_file_name(name).blocking_save_file())
    .await
    .map_err(|e| e.to_string())?;
  match picked {
    Some(path) => Ok(Some(path.into_path().map_err(|e| e.to_string())?.to_string_lossy().into_owned())),
    None => Ok(None),
  }
}

#[tauri::command]
pub fn local_move_accept_lost<R: Runtime>(app: AppHandle<R>, code: String, dir: String, key: String) -> Result<(), String> {
  let (_, dir) = workspace_and_dir(&app, &code, &dir)?;
  accept_lost(&dir, &key)
}

#[tauri::command]
pub fn local_move_clear_decision<R: Runtime>(app: AppHandle<R>, code: String, dir: String, key: String) -> Result<(), String> {
  let (_, dir) = workspace_and_dir(&app, &code, &dir)?;
  clear_decision(&dir, &key)
}

#[tauri::command]
pub fn local_move_add_refused<R: Runtime>(app: AppHandle<R>, code: String, dir: String, keys: Vec<String>) -> Result<(), String> {
  let (_, dir) = workspace_and_dir(&app, &code, &dir)?;
  add_excluded(&dir, &keys, "refused")
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn local_workspace_remove<R: Runtime>(
  app: AppHandle<R>,
  dbs: State<'_, LocalDbs>,
  moving: State<'_, Moving>,
  code: String,
  typed_name: String,
  expected_revision: String,
  archive_path: String,
  dir: String,
  job_uuid: String,
  api_base: String,
  token: String,
) -> Result<(), String> {
  let root = local_workspaces::root(&app)?;
  let (_, dir) = workspace_and_dir(&app, &code, &dir)?;
  let evidence = fetch_job(&api_base, &job_uuid, &token).await?;
  publication_verdict(&evidence, &expected_revision)?;

  let _operation = begin_operation(&moving, &code).await?;
  let connection = live_connection(&app, &dbs, &code)?;
  let now = crate::tray::now_secs();
  let removed = {
    let code = code.clone();
    tauri::async_runtime::spawn_blocking(move || {
      let workspace_dir = {
        let conn = connection.lock().map_err(|e| e.to_string())?;
        check_removal(
          &root,
          &code,
          &RemoveRequest {
            typed_name: &typed_name,
            expected_revision: &expected_revision,
            archive_path: Path::new(&archive_path),
            dir: &dir,
          },
          Some(&conn),
        )?
      };
      Ok::<_, String>((root, workspace_dir, connection))
    })
    .await
    .map_err(|e| e.to_string())?
  };
  let (root, workspace_dir, connection) = removed?;
  dbs.forget(&code);
  drop(connection);
  let target = {
    let code = code.clone();
    tauri::async_runtime::spawn_blocking(move || remove_folder(&root, &code, &workspace_dir, now))
      .await
      .map_err(|e| e.to_string())??
  };
  log::info!("[local] removed local workspace {code} to {}", target.display());
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::atomic::{AtomicU64, Ordering};

  const SCHEMA: &str = "
    CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT);
    CREATE TABLE statuses (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, type TEXT NOT NULL DEFAULT 'active', color TEXT, sort_order INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
    CREATE TABLE categories (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, slug TEXT, code TEXT, parent_id INTEGER REFERENCES categories(id) ON DELETE SET NULL, settings TEXT NOT NULL DEFAULT '[]', created_at TEXT NOT NULL, updated_at TEXT NOT NULL, deleted_at TEXT);
    CREATE TABLE tasks (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, description TEXT, description_json TEXT, status_id INTEGER REFERENCES statuses(id) ON DELETE SET NULL, project_category_id INTEGER REFERENCES categories(id) ON DELETE SET NULL, category_tasks_sequence_id INTEGER, sort_order INTEGER, priority TEXT NOT NULL DEFAULT 'medium', common_time INTEGER NOT NULL DEFAULT 0, start_time INTEGER NOT NULL DEFAULT 0, end_time INTEGER, approximately_time INTEGER NOT NULL DEFAULT 0, checkpoints TEXT, settings TEXT NOT NULL DEFAULT '[]', expired_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, deleted_at TEXT);
    CREATE TABLE comments (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE, message TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, deleted_at TEXT, author_kind TEXT NOT NULL DEFAULT 'user', author_id TEXT, author_name TEXT);
    CREATE TABLE comment_reactions (id INTEGER PRIMARY KEY AUTOINCREMENT, comment_id INTEGER NOT NULL REFERENCES comments(id) ON DELETE CASCADE, emoji TEXT NOT NULL, actor_kind TEXT NOT NULL, actor_id TEXT NOT NULL, created_at TEXT NOT NULL);
    CREATE TABLE task_relation_types (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE);
    CREATE TABLE task_relations (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL, related_task_id INTEGER NOT NULL, relation_type_id INTEGER NOT NULL, created_at TEXT NOT NULL);
    CREATE TABLE files (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER, page_id INTEGER, name TEXT NOT NULL, file_path TEXT NOT NULL UNIQUE, mime_type TEXT, size INTEGER, created_at TEXT NOT NULL);
    CREATE TABLE pages (id INTEGER PRIMARY KEY AUTOINCREMENT, parent_id INTEGER, title TEXT NOT NULL, slug TEXT NOT NULL UNIQUE, type TEXT NOT NULL DEFAULT 'plain', body TEXT NOT NULL DEFAULT '', properties TEXT NOT NULL DEFAULT '{}', version INTEGER NOT NULL DEFAULT 1, author_id INTEGER NOT NULL, author_kind TEXT NOT NULL, author_ref TEXT NOT NULL, updated_by_id INTEGER NOT NULL, updated_by_kind TEXT NOT NULL, updated_by_ref TEXT NOT NULL, position INTEGER NOT NULL DEFAULT 0, pinned INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, deleted_at TEXT);
    CREATE TABLE page_versions (page_id INTEGER NOT NULL, version INTEGER NOT NULL, title TEXT NOT NULL, body TEXT NOT NULL, properties TEXT NOT NULL DEFAULT '{}', author_id INTEGER NOT NULL, author_kind TEXT NOT NULL, author_ref TEXT NOT NULL, summary TEXT, created_at TEXT NOT NULL, PRIMARY KEY (page_id, version));
    CREATE TABLE page_links (page_id INTEGER NOT NULL, target_kind TEXT NOT NULL, target_id INTEGER NOT NULL);
    CREATE VIRTUAL TABLE pages_fts USING fts5(title, body, content='pages', content_rowid='id');
    CREATE TRIGGER pages_fts_ai AFTER INSERT ON pages BEGIN INSERT INTO pages_fts(rowid, title, body) VALUES (new.id, new.title, new.body); END;
    CREATE TABLE routines (id INTEGER PRIMARY KEY, title TEXT NOT NULL, description TEXT, routine_category TEXT, priority TEXT, approximately_time INTEGER, scheduled_date TEXT, scheduled_time TEXT, settings TEXT NOT NULL DEFAULT '[]', archived_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL, deleted_at TEXT);
    CREATE TABLE routine_patterns (id INTEGER PRIMARY KEY AUTOINCREMENT, routine_id INTEGER NOT NULL UNIQUE, frequency TEXT, interval INTEGER NOT NULL DEFAULT 1, day_of_frequency INTEGER, month INTEGER, days_of_week TEXT, start_at TEXT, end_at TEXT, occurrences INTEGER, scheduled_time TEXT, duration_min INTEGER, reminder_min INTEGER, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
    CREATE TABLE routine_instances (id INTEGER PRIMARY KEY AUTOINCREMENT, routine_id INTEGER NOT NULL, scheduled_for TEXT NOT NULL, status TEXT NOT NULL DEFAULT 'PENDING', completed_at TEXT, skipped_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
    CREATE TABLE agent_work_runs (id INTEGER PRIMARY KEY AUTOINCREMENT, task_id INTEGER NOT NULL, agent TEXT NOT NULL, model TEXT, session_id TEXT, branch TEXT, status TEXT NOT NULL DEFAULT 'running', started_at TEXT NOT NULL, ended_at TEXT, duration_seconds INTEGER, summary TEXT, pr_url TEXT, commits TEXT, tests TEXT, actor_kind TEXT NOT NULL, actor_id TEXT NOT NULL, version INTEGER NOT NULL DEFAULT 1, created_at TEXT NOT NULL, updated_at TEXT NOT NULL);
    CREATE TABLE personas (uuid TEXT PRIMARY KEY, owner_user_id INTEGER NOT NULL, owner_name TEXT NOT NULL, name TEXT NOT NULL, description TEXT, avatar_file TEXT, synced_at TEXT NOT NULL, archived_at TEXT);
    CREATE TABLE workspace_personas (persona_uuid TEXT PRIMARY KEY, permissions TEXT NOT NULL, enabled_at TEXT NOT NULL, disabled_at TEXT);
    CREATE TABLE task_persona_assignees (task_id INTEGER NOT NULL, persona_uuid TEXT NOT NULL, owner_implied INTEGER NOT NULL DEFAULT 0, created_at TEXT NOT NULL, PRIMARY KEY (task_id, persona_uuid));
    CREATE TABLE activity_log (id INTEGER PRIMARY KEY AUTOINCREMENT, at TEXT NOT NULL, method TEXT NOT NULL);
    CREATE TABLE plugin_kv (plugin_id TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL, PRIMARY KEY (plugin_id, key));
  ";

  const DATA: &str = "
    INSERT INTO meta VALUES ('schema_version', '12'), ('feature.pages', 'true'), ('feature.board', 'false'), ('pinned', 'secret');
    INSERT INTO statuses (id, name, type, color, sort_order, created_at, updated_at) VALUES (1, 'Backlog', 'default', NULL, 0, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z'), (2, 'Done', 'completed', '#0a0', 1.5, '2026-01-01T00:00:00Z', '2026-01-02T00:00:00Z');
    INSERT INTO categories (id, title, slug, code, parent_id, created_at, updated_at) VALUES (1, 'Child', 'child', 'CH', 2, '2026-01-02T00:00:00Z', '2026-01-02T00:00:00Z'), (2, 'Root', 'root', 'RT', NULL, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z');
    INSERT INTO tasks (id, title, description, status_id, project_category_id, category_tasks_sequence_id, common_time, start_time, created_at, updated_at, deleted_at) VALUES (1, 'First \"quoted\" task', 'Привет', 1, 2, 1, 30, 0, '2026-01-03T00:00:00Z', '2026-01-03T00:00:00Z', NULL), (2, 'Archived', NULL, 2, 1, 1, 0, 0, '2026-01-03T00:00:00Z', '2026-01-04T00:00:00Z', '2026-01-05T00:00:00Z');
    INSERT INTO task_relation_types VALUES (1, 'blocks');
    INSERT INTO task_relations (id, task_id, related_task_id, relation_type_id, created_at) VALUES (1, 1, 2, 1, '2026-01-03T00:00:00Z');
    INSERT INTO comments (id, task_id, message, created_at, updated_at, author_kind, author_id, author_name) VALUES (1, 1, 'Looks good', '2026-01-03T01:00:00Z', '2026-01-03T01:00:00Z', 'persona', 'p-1', 'Reviewer');
    INSERT INTO comment_reactions (id, comment_id, emoji, actor_kind, actor_id, created_at) VALUES (1, 1, 'ok', 'user', '1', '2026-01-03T02:00:00Z');
    INSERT INTO pages (id, parent_id, title, slug, body, author_id, author_kind, author_ref, updated_by_id, updated_by_kind, updated_by_ref, created_at, updated_at) VALUES (1, 2, 'Leaf', 'leaf', 'see [[root]]', -1, 'user', 'me', -1, 'user', 'me', '2026-01-04T00:00:00Z', '2026-01-04T00:00:00Z'), (2, NULL, 'Top', 'top', 'body', -1, 'user', 'me', -1, 'user', 'me', '2026-01-04T00:00:00Z', '2026-01-04T00:00:00Z');
    INSERT INTO page_versions (page_id, version, title, body, author_id, author_kind, author_ref, created_at) VALUES (2, 1, 'Top', 'body', -1, 'user', 'me', '2026-01-04T00:00:00Z'), (1, 1, 'Leaf', 'v1', -1, 'user', 'me', '2026-01-04T00:00:00Z');
    INSERT INTO page_links VALUES (1, 'page', 2);
    INSERT INTO routines (id, title, created_at, updated_at) VALUES (7, 'Water plants', '2026-01-05T00:00:00Z', '2026-01-05T00:00:00Z');
    INSERT INTO routine_patterns (id, routine_id, frequency, created_at, updated_at) VALUES (1, 7, 'DAILY', '2026-01-05T00:00:00Z', '2026-01-05T00:00:00Z');
    INSERT INTO routine_instances (id, routine_id, scheduled_for, created_at, updated_at) VALUES (1, 7, '2026-01-06', '2026-01-05T00:00:00Z', '2026-01-05T00:00:00Z');
    INSERT INTO agent_work_runs (id, task_id, agent, status, started_at, actor_kind, actor_id, created_at, updated_at) VALUES (1, 1, 'claude-code', 'succeeded', '2026-01-03T03:00:00Z', 'persona', 'p-1', '2026-01-03T03:00:00Z', '2026-01-03T04:00:00Z');
    INSERT INTO personas (uuid, owner_user_id, owner_name, name, description, synced_at) VALUES ('p-1', -1, 'Me', 'Reviewer', 'reads', '2026-01-01T00:00:00Z');
    INSERT INTO workspace_personas VALUES ('p-1', '[\"tasks:read\"]', '2026-01-01T00:00:00Z', NULL);
    INSERT INTO task_persona_assignees VALUES (1, 'p-1', 0, '2026-01-03T00:00:00Z');
    INSERT INTO files (id, task_id, name, file_path, mime_type, size, created_at) VALUES (1, 1, 'a.png', 'u1/a.png', 'image/png', 3, '2026-01-03T00:00:00Z'), (2, 1, 'big.png', 'u2/big.png', 'image/png', 9, '2026-01-03T00:00:00Z'), (3, 1, 'n.txt', 'u3/n.txt', NULL, 2, '2026-01-03T00:00:00Z'), (4, 1, 'gone.png', 'u4/gone.png', 'image/png', 1, '2026-01-03T00:00:00Z');
    INSERT INTO activity_log (at, method) VALUES ('t', 'GET');
    INSERT INTO plugin_kv VALUES ('p', 'k', 'v');
  ";

  fn temp(name: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
      "tmgr-move-{name}-{}-{}",
      std::process::id(),
      COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
  }

  fn memory() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(SCHEMA).unwrap();
    conn.execute_batch(DATA).unwrap();
    conn
  }

  fn workspace(root: &Path, name: &str) -> PathBuf {
    let created = local_workspaces::create_in(root, name, "2026-01-01T00:00:00Z").unwrap();
    let dir = PathBuf::from(&created.path);
    let conn = Connection::open(&created.database).unwrap();
    conn.execute_batch(SCHEMA).unwrap();
    conn.execute_batch(DATA).unwrap();
    for (key, body) in [("u1/a.png", &b"png"[..]), ("u2/big.png", b"123456789"), ("u3/n.txt", b"hi")] {
      let path = file_path(&dir, key);
      fs::create_dir_all(path.parent().unwrap()).unwrap();
      fs::write(path, body).unwrap();
    }
    dir
  }

  fn open_workspace(dir: &Path) -> Connection {
    Connection::open(dir.join("workspace.db")).unwrap()
  }

  #[test]
  fn an_operation_owns_the_workspace_until_its_own_token_releases_it() {
    let moving = Moving::default();
    assert!(moving.check("a").is_ok());
    let first = moving.begin("a").unwrap();
    assert_eq!(moving.begin("a").err().unwrap(), "another move or removal is in progress");
    assert_eq!(moving.check("a").unwrap_err(), "workspace is moving");
    assert!(moving.check("b").is_ok());
    let token = first.keep();
    assert!(moving.owns("a", token));
    assert!(!moving.owns("a", token + 1));
    assert!(!moving.release("a", token + 1));
    assert!(moving.is_set("a"));
    assert!(moving.release("a", token));
    assert!(!moving.release("a", token));
    assert!(!moving.is_set("a"));
    drop(moving.begin("a").unwrap());
    assert!(!moving.is_set("a"));
  }

  #[test]
  fn a_removal_lock_cannot_be_cleared_by_a_move_token() {
    let moving = Moving::default();
    let move_token = moving.begin("ws").unwrap().keep();
    assert!(moving.release("ws", move_token));
    let removal = moving.begin("ws").unwrap();
    assert!(!moving.release("ws", move_token));
    assert!(!moving.owns("ws", move_token));
    assert!(moving.is_set("ws"));
    assert!(moving.begin("ws").is_err());
    drop(removal);
    assert!(!moving.is_set("ws"));
  }

  #[test]
  fn taking_the_lock_waits_for_a_slow_write_and_later_writes_are_refused() {
    let moving = Moving::default();
    let dir = temp("drain");
    let marker = dir.join("written");
    let permit = moving.permit("ws").unwrap();
    let writer = {
      let marker = marker.clone();
      std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(250));
        fs::write(marker, b"done").unwrap();
        drop(permit);
      })
    };
    let operation = moving.begin("ws").unwrap();
    assert!(marker.exists(), "begin returned before the in-flight write finished");
    assert_eq!(moving.permit("ws").err().unwrap(), "workspace is moving");
    writer.join().unwrap();
    drop(operation);
    assert!(moving.permit("ws").is_ok());
  }

  #[test]
  fn freezing_stops_running_timers_and_counts_them() {
    let conn = memory();
    conn
      .execute_batch(
        "UPDATE tasks SET start_time = 1000, common_time = 30 WHERE id = 1;
         UPDATE tasks SET start_time = 1500, common_time = 0, end_time = 5 WHERE id = 2;
         INSERT INTO tasks (id, title, common_time, created_at, updated_at) VALUES (3, 'idle', 7, 'x', 'x');",
      )
      .unwrap();
    assert_eq!(stop_timers(&conn, 1600).unwrap(), 2);
    let row = |id: i64| -> (i64, i64, Option<i64>) {
      conn
        .query_row("SELECT common_time, start_time, end_time FROM tasks WHERE id = ?", [id], |r| {
          Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .unwrap()
    };
    assert_eq!(row(1), (630, 0, Some(1600)));
    assert_eq!(row(2), (100, 0, Some(1600)));
    assert_eq!(row(3), (7, 0, None));
    assert_eq!(stop_timers(&conn, 1700).unwrap(), 0);
  }

  #[test]
  fn revision_is_stable_and_changes_with_any_row() {
    let conn = memory();
    let base = revision_of(&conn).unwrap();
    assert_eq!(base.len(), 64);
    assert_eq!(revision_of(&conn).unwrap(), base);

    let mut seen = HashSet::from([base.clone()]);
    for change in [
      "UPDATE meta SET value = 'false' WHERE key = 'feature.pages'",
      "UPDATE meta SET value = 'x' WHERE key = 'pinned'",
      "UPDATE workspace_personas SET permissions = '[]'",
      "UPDATE personas SET description = 'edited'",
      "UPDATE tasks SET title = 'Other' WHERE id = 2",
      "UPDATE tasks SET common_time = 31 WHERE id = 1",
      "UPDATE statuses SET sort_order = 2 WHERE id = 2",
      "UPDATE page_versions SET summary = 's' WHERE page_id = 1",
      "DELETE FROM task_persona_assignees",
      "INSERT INTO files (id, name, file_path, created_at) VALUES (9, 'n', 'u9/n', 'x')",
    ] {
      let fresh = memory();
      fresh.execute_batch(change).unwrap();
      assert!(seen.insert(revision_of(&fresh).unwrap()), "{change}");
    }
  }

  #[test]
  fn revision_ignores_activity_plugin_and_search_tables() {
    let conn = memory();
    let base = revision_of(&conn).unwrap();
    conn.execute_batch("INSERT INTO activity_log (at, method) VALUES ('later', 'POST'); UPDATE plugin_kv SET value = 'other'").unwrap();
    conn.execute_batch("INSERT INTO pages_fts(pages_fts) VALUES ('rebuild')").unwrap();
    assert_eq!(revision_of(&conn).unwrap(), base);
  }

  #[test]
  fn revision_separates_types_and_adjacent_values() {
    let a = Connection::open_in_memory().unwrap();
    a.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, x, y); INSERT INTO t VALUES (1, 'ab', 'c')").unwrap();
    let b = Connection::open_in_memory().unwrap();
    b.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, x, y); INSERT INTO t VALUES (1, 'a', 'bc')").unwrap();
    let c = Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, x, y); INSERT INTO t VALUES (1, 'ab', X'63')").unwrap();
    let d = Connection::open_in_memory().unwrap();
    d.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, x, y); INSERT INTO t VALUES (1, '1', 'c')").unwrap();
    let e = Connection::open_in_memory().unwrap();
    e.execute_batch("CREATE TABLE t (id INTEGER PRIMARY KEY, x, y); INSERT INTO t VALUES (1, 1, 'c')").unwrap();
    let all: HashSet<String> = [&a, &b, &c, &d, &e].iter().map(|conn| revision_of(conn).unwrap()).collect();
    assert_eq!(all.len(), 5);
  }

  #[test]
  fn revision_is_identical_for_the_live_database_and_its_vacuum_copy() {
    let root = temp("revision");
    let dir = workspace(&root, "Live");
    let live = open_workspace(&dir);
    live.execute_batch("DELETE FROM comments; INSERT INTO comments (task_id, message, created_at, updated_at) VALUES (1, 'later', 'x', 'x')").unwrap();
    let copy = root.join("copy.db");
    vacuum_into(&live, &copy).unwrap();
    assert_eq!(revision_of(&live).unwrap(), revision_of(&open_read_only(&copy).unwrap()).unwrap());
  }

  #[test]
  fn snapshot_copies_referenced_files_and_records_missing_ones() {
    let root = temp("snapshot");
    let dir = workspace(&root, "Snap");
    let live = open_workspace(&dir);
    let snap_dir = snapshot_db(&live, &dir, 1_800_000_000).unwrap();
    assert!(snap_dir.starts_with(dir.join("backups")));
    assert!(snap_dir.file_name().unwrap().to_string_lossy().starts_with("cloud-move-"));
    let snapshot = snapshot_files(&snap_dir, &dir, 1_800_000_000).unwrap();

    assert_eq!(snapshot.record.revision, revision_of(&live).unwrap());
    assert_eq!(fs::read(snap_dir.join("files/u1/a.png")).unwrap(), b"png");
    assert_eq!(fs::read(snap_dir.join("files/u3/n.txt")).unwrap(), b"hi");
    let files = &snapshot.record.files;
    assert_eq!(files.len(), 4);
    assert_eq!((files[0].key.as_str(), files[0].size, files[0].missing), ("u1/a.png", 3, false));
    assert_eq!(files[0].sha256, sha256_hex(b"png"));
    assert_eq!(files[0].mime, "image/png");
    assert_eq!(files[2].mime, "text/plain");
    assert!(files[3].missing && files[3].sha256.is_empty());
    let recorded: SnapshotRecord = serde_json::from_str(&fs::read_to_string(snap_dir.join("snapshot.json")).unwrap()).unwrap();
    assert_eq!(recorded, snapshot.record);

    let second = snapshot_db(&live, &dir, 1_800_000_000).unwrap();
    assert_ne!(second, snap_dir);
  }

  #[test]
  fn a_failed_snapshot_leaves_no_folder_behind() {
    let root = temp("snapfail");
    let dir = workspace(&root, "Fail");
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA query_only = ON").unwrap();
    fs::write(dir.join("backups").join("cloud-move-20270115-080000"), b"in the way").unwrap();
    let before = fs::read_dir(dir.join("backups")).unwrap().count();
    assert!(snapshot_db(&conn, &dir, 1_800_000_000).is_err());
    assert_eq!(fs::read_dir(dir.join("backups")).unwrap().count(), before);
  }

  fn built_bundle(name: &str) -> (PathBuf, BundleResult) {
    let root = temp(name);
    let dir = workspace(&root, "Golden");
    let live = open_workspace(&dir);
    let snap_dir = snapshot_db(&live, &dir, 1_800_000_000).unwrap();
    snapshot_files(&snap_dir, &dir, 1_800_000_000).unwrap();
    let allowlist = vec!["image/*".to_string(), "text/plain".to_string()];
    let bundle = build_bundle(
      &snap_dir,
      &BundleInput { code: "golden", name: "Golden", profile_uuid: "11111111-2222-3333-4444-555555555555", max_file_bytes: 5, mime_allowlist: &allowlist },
    )
    .unwrap();
    (snap_dir, bundle)
  }

  fn gunzip(path: &str) -> String {
    let mut text = String::new();
    flate2::read::GzDecoder::new(File::open(path).unwrap()).read_to_string(&mut text).unwrap();
    text
  }

  #[test]
  fn bundle_matches_the_golden_file() {
    let (_, bundle) = built_bundle("golden");
    let text = gunzip(&bundle.path);
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/move-bundle-v1.ndjson");
    if std::env::var("UPDATE_GOLDEN").is_ok() {
      let normalised = text.lines().map(|l| {
        if l.contains("\"t\":\"manifest\"") {
          let value: Json = serde_json::from_str(l).unwrap();
          let revision = value["source_revision"].as_str().unwrap();
          let fingerprint = value["fingerprint"].as_str().unwrap();
          l.replace(revision, "<revision>").replace(fingerprint, "<fingerprint>")
        } else {
          l.to_string()
        }
      });
      fs::write(&fixture, normalised.collect::<Vec<_>>().join("\n") + "\n").unwrap();
    }
    let expected = fs::read_to_string(&fixture).unwrap();
    let revision = bundle.manifest["source_revision"].as_str().unwrap().to_string();
    let fingerprint = bundle.manifest["fingerprint"].as_str().unwrap().to_string();
    let expected = expected.replace("<revision>", &revision).replace("<fingerprint>", &fingerprint);
    assert_eq!(text, expected);
  }

  #[test]
  fn bundle_manifest_hashes_counts_and_fingerprint_recompute_independently() {
    let (snap_dir, bundle) = built_bundle("hashes");
    let text = gunzip(&bundle.path);
    let mut lines = text.lines();
    let manifest: Json = serde_json::from_str(lines.next().unwrap()).unwrap();
    assert_eq!(manifest, bundle.manifest);
    assert_eq!(manifest["t"], "manifest");
    assert_eq!(manifest["bundle_schema"], 1);
    assert_eq!(manifest["local_schema"], 12);
    assert_eq!(manifest["snapshot_at"], rfc3339(1_800_000_000));
    assert_eq!(manifest["workspace"], serde_json::json!({"code": "golden", "name": "Golden"}));

    let revision = revision_of(&open_read_only(&snap_dir.join("workspace.db")).unwrap()).unwrap();
    assert_eq!(manifest["source_revision"], revision);
    let expected_fingerprint = sha256_hex(format!("11111111-2222-3333-4444-555555555555golden{revision}").as_bytes());
    assert_eq!(manifest["fingerprint"], expected_fingerprint);

    let mut by_entity: HashMap<String, Vec<&str>> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for line in lines {
      let value: Json = serde_json::from_str(line).unwrap();
      let entity = value["t"].as_str().unwrap().to_string();
      if order.last() != Some(&entity) {
        order.push(entity.clone());
      }
      by_entity.entry(entity).or_default().push(line);
    }
    let expected_order: Vec<String> = ENTITIES.iter().filter(|e| by_entity.contains_key(**e)).map(|e| e.to_string()).collect();
    assert_eq!(order, expected_order);
    for entity in ENTITIES {
      let lines = by_entity.get(entity).cloned().unwrap_or_default();
      assert_eq!(manifest["counts"][entity].as_u64().unwrap() as usize, lines.len(), "{entity}");
      assert_eq!(manifest["hashes"][entity], sha256_hex(lines.join("\n").as_bytes()), "{entity}");
    }
    assert_eq!(manifest["counts"].as_object().unwrap().len(), ENTITIES.len());
  }

  #[test]
  fn bundle_orders_parents_first_marks_excluded_files_and_leaves_secrets_out() {
    let (snap_dir, bundle) = built_bundle("rules");
    let text = gunzip(&bundle.path);
    assert!(!text.contains("secret"));
    assert!(!text.contains("later"));
    let rows = |entity: &str| -> Vec<Json> {
      text
        .lines()
        .skip(1)
        .map(|l| serde_json::from_str::<Json>(l).unwrap())
        .filter(|v| v["t"] == entity)
        .map(|v| v["r"].clone())
        .collect()
    };
    let titles = |entity: &str| rows(entity).iter().map(|r| r["title"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    assert_eq!(titles("categories"), ["Root", "Child"]);
    assert_eq!(titles("pages"), ["Top", "Leaf"]);
    let meta: Vec<String> = rows("meta").iter().map(|r| r["key"].as_str().unwrap().to_string()).collect();
    assert_eq!(meta, ["feature.board", "feature.pages"]);
    assert!(rows("personas")[0].get("token").is_none());

    let files = rows("files");
    let flag = |i: usize| files[i].get("excluded").and_then(|v| v.as_str()).map(String::from);
    assert_eq!(flag(0), None);
    assert_eq!(flag(1).as_deref(), Some("too_large"));
    assert_eq!(flag(2), None);
    assert_eq!(flag(3).as_deref(), Some("missing"));
    assert_eq!(files[0]["sha256"], sha256_hex(b"png"));
    assert!(files[3].get("sha256").is_none());
    assert_eq!(files[0]["file_path"], "u1/a.png");

    let strict = build_bundle(
      &snap_dir,
      &BundleInput { code: "golden", name: "Golden", profile_uuid: "u", max_file_bytes: 100, mime_allowlist: &["image/png".to_string()] },
    )
    .unwrap();
    let strict_text = gunzip(&strict.path);
    let mime_excluded = strict_text.lines().filter(|l| l.contains("\"excluded\":\"mime\"")).count();
    assert_eq!(mime_excluded, 1);
  }

  #[test]
  fn tree_order_survives_cycles_and_orphans() {
    let conn = Connection::open_in_memory().unwrap();
    conn
      .execute_batch(
        "CREATE TABLE c (id INTEGER PRIMARY KEY, parent_id INTEGER);
         INSERT INTO c VALUES (1, 2), (2, 1), (3, NULL), (4, 3), (5, 99), (6, 6);",
      )
      .unwrap();
    assert_eq!(tree_order(&conn, "c").unwrap(), [3, 5, 6, 4, 1, 2]);
  }

  async fn serve_once(response: &'static str) -> (String, tokio::sync::oneshot::Receiver<(Option<String>, Vec<u8>, Vec<(String, String)>)>) {
    use http_body_util::BodyExt;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/upload", listener.local_addr().unwrap());
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
      let (stream, _) = listener.accept().await.unwrap();
      let tx = std::sync::Mutex::new(Some(tx));
      let service = hyper::service::service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
        let sender = tx.lock().unwrap().take();
        async move {
          let length = req.headers().get("content-length").map(|v| v.to_str().unwrap().to_string());
          let custom: Vec<(String, String)> = req
            .headers()
            .iter()
            .filter(|(n, _)| n.as_str().starts_with("x-"))
            .map(|(n, v)| (n.to_string(), v.to_str().unwrap().to_string()))
            .collect();
          let body = req.into_body().collect().await.unwrap().to_bytes().to_vec();
          if let Some(sender) = sender {
            let _ = sender.send((length, body, custom));
          }
          Ok::<_, std::convert::Infallible>(hyper::Response::builder().status(201).body(http_body_util::Full::new(hyper::body::Bytes::from(response))).unwrap())
        }
      });
      let _ = hyper::server::conn::http1::Builder::new()
        .serve_connection(hyper_util::rt::TokioIo::new(stream), service)
        .await;
    });
    (url, rx)
  }

  #[tokio::test]
  async fn put_streams_the_file_with_its_length_and_headers() {
    let dir = temp("put");
    let path = dir.join("bundle.ndjson.gz");
    let payload: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
    fs::write(&path, &payload).unwrap();
    let (url, received) = serve_once("stored").await;
    let headers = HashMap::from([("x-token".to_string(), "abc".to_string()), ("Content-Length".to_string(), "5".to_string())]);
    let result = put_file(&url, &headers, &path).await.unwrap();
    assert_eq!(result, PutResult { status: 201, body: "stored".to_string() });
    let (length, body, custom) = received.await.unwrap();
    assert_eq!(length.as_deref(), Some("300000"));
    assert_eq!(body, payload);
    assert!(custom.contains(&("x-token".to_string(), "abc".to_string())));
  }

  #[tokio::test]
  async fn put_truncates_long_response_bodies() {
    let dir = temp("putlong");
    let path = dir.join("f");
    fs::write(&path, b"x").unwrap();
    let long: &'static str = Box::leak("e".repeat(10_000).into_boxed_str());
    let (url, _received) = serve_once(long).await;
    let result = put_file(&url, &HashMap::new(), &path).await.unwrap();
    assert_eq!(result.body.len(), 2048);
  }

  #[tokio::test]
  async fn put_reports_missing_files_and_unreachable_servers() {
    let dir = temp("puterr");
    assert!(put_file("http://127.0.0.1:9/x", &HashMap::new(), &dir.join("nope")).await.is_err());
    let path = dir.join("f");
    fs::write(&path, b"x").unwrap();
    assert!(put_file("http://127.0.0.1:9/x", &HashMap::new(), &path).await.is_err());
  }

  #[test]
  fn only_files_inside_the_root_may_be_uploaded() {
    let root = temp("inside");
    let other = temp("outside");
    fs::write(root.join("a"), b"x").unwrap();
    fs::write(other.join("a"), b"x").unwrap();
    assert!(inside(&root, &root.join("a")));
    assert!(!inside(&root, &other.join("a")));
    assert!(!inside(&root, &root.join("..").join(other.file_name().unwrap()).join("a")));
    assert!(!inside(&root, &root.join("missing")));
  }


  #[test]
  fn full_digest_covers_plugin_and_activity_tables_and_every_file_under_files() {
    let root = temp("digest");
    let dir = workspace(&root.join("workspaces"), "Dig");
    let live = open_workspace(&dir);
    let tables = full_digest_tables(&live).unwrap();
    assert!(tables.contains(&"plugin_kv".to_string()) && tables.contains(&"activity_log".to_string()));
    assert!(tables.iter().all(|t| !t.starts_with("sqlite_") && !t.starts_with("pages_fts")));

    let (revision, digest) = (revision_of(&live).unwrap(), full_digest(&live, &dir).unwrap());
    assert_eq!(full_digest(&live, &dir).unwrap(), digest);

    live.execute_batch("UPDATE plugin_kv SET value = 'changed'").unwrap();
    assert_eq!(revision_of(&live).unwrap(), revision);
    let after_plugin = full_digest(&live, &dir).unwrap();
    assert_ne!(after_plugin, digest);

    live.execute_batch("INSERT INTO activity_log (at, method) VALUES ('later', 'POST')").unwrap();
    let after_log = full_digest(&live, &dir).unwrap();
    assert_ne!(after_log, after_plugin);

    let orphan = dir.join("files/orphan/x.bin");
    fs::create_dir_all(orphan.parent().unwrap()).unwrap();
    fs::write(&orphan, b"abc").unwrap();
    let with_orphan = full_digest(&live, &dir).unwrap();
    assert_ne!(with_orphan, after_log);
    fs::write(&orphan, b"abd").unwrap();
    assert_ne!(full_digest(&live, &dir).unwrap(), with_orphan);
    fs::remove_file(&orphan).unwrap();
    assert_eq!(full_digest(&live, &dir).unwrap(), after_log);
  }

  #[test]
  fn full_digest_is_identical_for_the_live_database_and_its_vacuum_copy() {
    let root = temp("digest-copy");
    let dir = workspace(&root.join("workspaces"), "Dig");
    let live = open_workspace(&dir);
    let copy = dir.join("backups").join(".copy.db");
    vacuum_into(&live, &copy).unwrap();
    assert_eq!(db_digest_of(&live).unwrap(), db_digest_of(&open_read_only(&copy).unwrap()).unwrap());
    assert_eq!(revision_of(&live).unwrap(), revision_of(&open_read_only(&copy).unwrap()).unwrap());
  }

  #[cfg(unix)]
  #[test]
  fn a_symbolic_link_under_files_makes_the_inventory_fail() {
    let root = temp("symlink");
    let dir = workspace(&root.join("workspaces"), "Sym");
    std::os::unix::fs::symlink("/etc/hosts", dir.join("files/link")).unwrap();
    assert!(files_inventory(&dir).unwrap_err().contains("symbolic link"));
  }

  fn archived(name: &str) -> (PathBuf, PathBuf, ArchiveResult, PathBuf) {
    let root = temp(name);
    let dir = workspace(&root.join("workspaces"), "Arch");
    let orphan = dir.join("files/orphan/x.bin");
    fs::create_dir_all(orphan.parent().unwrap()).unwrap();
    fs::write(orphan, b"unreferenced").unwrap();
    let live = open_workspace(&dir);
    let copy = dir.join("backups").join(".copy.db");
    vacuum_into(&live, &copy).unwrap();
    let result = write_archive(&dir, &copy, 1_800_000_000).unwrap();
    (root, dir, result, copy)
  }

  #[test]
  fn archive_contains_the_database_and_every_file_under_files_and_verifies() {
    let (_, dir, result, _) = archived("archive");
    assert!(result.verified);
    assert_eq!(result.entries, 6);
    let live = open_workspace(&dir);
    assert_eq!(result.revision, revision_of(&live).unwrap());
    assert_eq!(result.full_digest, full_digest(&live, &dir).unwrap());
    assert!(Path::new(&result.path).starts_with(dir.join("backups")));
    assert!(result.path.ends_with("archive-20270115-080000.zip"));

    let (record, entries) = verify_archive(Path::new(&result.path)).unwrap();
    assert_eq!(entries, 6);
    assert_eq!(
      record.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
      ["orphan/x.bin", "u1/a.png", "u2/big.png", "u3/n.txt"]
    );
    assert_eq!(record.files[1].size, 3);
    assert_eq!(record.files[1].sha256, sha256_hex(b"png"));
    let mut zip = zip::ZipArchive::new(File::open(&result.path).unwrap()).unwrap();
    let mut names: Vec<String> = (0..zip.len()).map(|i| zip.by_index(i).unwrap().name().unwrap().to_string()).collect();
    names.sort();
    assert_eq!(
      names,
      ["archive.json", "files/orphan/x.bin", "files/u1/a.png", "files/u2/big.png", "files/u3/n.txt", "workspace.db"]
    );
    let mut png = Vec::new();
    zip.by_name("files/u1/a.png").unwrap().read_to_end(&mut png).unwrap();
    assert_eq!(png, b"png");
  }

  fn rewrite_zip(
    source: &str,
    target: &Path,
    edit: impl Fn(&str, Vec<u8>) -> Option<Vec<u8>>,
    extra: &[(&str, &[u8])],
  ) {
    let mut zip = zip::ZipArchive::new(File::open(source).unwrap()).unwrap();
    let mut out = zip::ZipWriter::new(File::create(target).unwrap());
    for i in 0..zip.len() {
      let mut entry = zip.by_index(i).unwrap();
      let name = entry.name().unwrap().to_string();
      let mut content = Vec::new();
      entry.read_to_end(&mut content).unwrap();
      if let Some(content) = edit(&name, content) {
        out.start_file(name, zip_options(0)).unwrap();
        out.write_all(&content).unwrap();
      }
    }
    for (name, content) in extra {
      out.start_file(*name, zip_options(0)).unwrap();
      out.write_all(content).unwrap();
    }
    out.finish().unwrap();
  }

  fn edit_record(change: impl Fn(&mut ArchiveRecord) + Copy) -> impl Fn(&str, Vec<u8>) -> Option<Vec<u8>> {
    move |name, content| {
      if name == ARCHIVE_META {
        let mut record: ArchiveRecord = serde_json::from_slice(&content).unwrap();
        change(&mut record);
        Some(serde_json::to_vec(&record).unwrap())
      } else {
        Some(content)
      }
    }
  }

  #[test]
  fn archive_verification_catches_corruption_and_a_mismatched_revision() {
    let (_, _, result, _) = archived("corrupt");
    let bytes = fs::read(&result.path).unwrap();

    let flipped = result.path.replace(".zip", "-flipped.zip");
    let mut damaged = bytes.clone();
    damaged[200] ^= 0xff;
    fs::write(&flipped, &damaged).unwrap();
    assert!(verify_archive(Path::new(&flipped)).is_err());

    let truncated = result.path.replace(".zip", "-cut.zip");
    fs::write(&truncated, &bytes[..bytes.len() / 2]).unwrap();
    assert!(verify_archive(Path::new(&truncated)).is_err());

    let forged = result.path.replace(".zip", "-forged.zip");
    rewrite_zip(&result.path, Path::new(&forged), edit_record(|r| r.revision = "0".repeat(64)), &[]);
    assert!(verify_archive(Path::new(&forged)).unwrap_err().contains("revision"));

    let digest = result.path.replace(".zip", "-digest.zip");
    rewrite_zip(&result.path, Path::new(&digest), edit_record(|r| r.full_digest = "0".repeat(64)), &[]);
    assert!(verify_archive(Path::new(&digest)).unwrap_err().contains("digest"));
  }

  #[test]
  fn archive_verification_reconciles_entries_against_the_recorded_file_list() {
    let (_, _, result, _) = archived("reconcile");
    let path = |suffix: &str| PathBuf::from(result.path.replace(".zip", &format!("-{suffix}.zip")));
    let check = |suffix: &str, edit: &dyn Fn(&str, Vec<u8>) -> Option<Vec<u8>>, extra: &[(&str, &[u8])]| {
      let target = path(suffix);
      rewrite_zip(&result.path, &target, edit, extra);
      verify_archive(&target).unwrap_err()
    };
    let keep = |_: &str, content: Vec<u8>| Some(content);

    let dropped = check("missing", &|name, c| (name != "files/u1/a.png").then_some(c), &[]);
    assert!(dropped.contains("holds 3 files but the record lists 4"), "{dropped}");

    let extra = check("extra", &keep, &[("files/extra.bin", b"x")]);
    assert!(extra.contains("holds 5 files but the record lists 4"), "{extra}");

    let swapped = check("swapped", &|name, c| if name == "files/u1/a.png" { Some(b"PNG".to_vec()) } else { Some(c) }, &[]);
    assert!(swapped.contains("does not match"), "{swapped}");

    let resized = check("resized", &|name, c| if name == "files/u1/a.png" { Some(b"pngpng".to_vec()) } else { Some(c) }, &[]);
    assert!(resized.contains("does not match"), "{resized}");

    let slip = check("slip", &keep, &[("files/../evil", b"x")]);
    assert!(slip.contains("unexpected entry"), "{slip}");
    let absolute = check("absolute", &keep, &[("/etc/evil", b"x")]);
    assert!(absolute.contains("unexpected entry"), "{absolute}");
    let backslash = check("backslash", &keep, &[("files\\..\\evil", b"x")]);
    assert!(backslash.contains("unexpected entry"), "{backslash}");
    let other = check("other", &keep, &[("notes.txt", b"x")]);
    assert!(other.contains("unexpected entry"), "{other}");

    let listed_twice = path("listed-twice");
    rewrite_zip(&result.path, &listed_twice, edit_record(|r| r.files.push(r.files[0].clone())), &[]);
    assert!(verify_archive(&listed_twice).unwrap_err().contains("invalid or duplicate path"));

    let hostile_record = path("hostile");
    rewrite_zip(&result.path, &hostile_record, edit_record(|r| r.files[0].path = "../x".to_string()), &[]);
    assert!(verify_archive(&hostile_record).is_err());
  }

  struct Removal {
    root: PathBuf,
    dir: PathBuf,
    archive: PathBuf,
    revision: String,
    snapshot: PathBuf,
  }

  fn snapshot_of(dir: &Path, code: &str) -> PathBuf {
    let live = open_workspace(dir);
    let snapshot = snapshot_db(&live, dir, 1_800_000_050).unwrap();
    snapshot_files(&snapshot, dir, 1_800_000_050).unwrap();
    build_bundle(
      &snapshot,
      &BundleInput { code, name: "Arch", profile_uuid: "profile", max_file_bytes: 5, mime_allowlist: &[] },
    )
    .unwrap();
    snapshot
  }

  fn removal(name: &str) -> Removal {
    let (root, dir, result, _) = archived(name);
    let snapshot = snapshot_of(&dir, "arch");
    Removal {
      root: root.join("workspaces"),
      dir,
      archive: PathBuf::from(&result.path),
      revision: result.revision.clone(),
      snapshot,
    }
  }

  fn decide_all(r: &Removal) {
    accept_lost(&r.snapshot, "u4/gone.png").unwrap();
    let out = temp("export").join("big.png");
    export_file(&r.dir, &r.snapshot, "u2/big.png", &out, &r.root).unwrap();
  }

  fn attempt(r: &Removal, typed: &str, expected: &str, archive: &Path) -> Result<PathBuf, String> {
    remove_in(
      &r.root,
      "arch",
      &RemoveRequest { typed_name: typed, expected_revision: expected, archive_path: archive, dir: &r.snapshot },
      1_800_000_100,
    )
  }

  fn untouched(r: &Removal) {
    assert!(r.dir.join("workspace.db").exists());
    assert!(r.dir.join("files").exists());
    assert!(!r.root.parent().unwrap().join("removed").exists());
  }

  #[test]
  fn removal_refuses_each_unmet_precondition_with_its_own_error() {
    let r = removal("refusals");
    decide_all(&r);
    let ok = |typed: &str, expected: &str, archive: &Path| attempt(&r, typed, expected, archive);

    assert_eq!(ok("arch", &r.revision, &r.archive).unwrap_err(), "typed name does not match the workspace name");
    assert_eq!(ok("Arch ", &r.revision, &r.archive).unwrap_err(), "typed name does not match the workspace name");
    assert_eq!(ok("Arch", "stale", &r.archive).unwrap_err(), "the workspace changed since the move snapshot");
    assert_eq!(ok("Arch", &r.revision, &r.dir.join("backups/archive-none.zip")).unwrap_err(), "archive file not found");

    let elsewhere = temp("elsewhere").join("archive.zip");
    fs::copy(&r.archive, &elsewhere).unwrap();
    assert_eq!(ok("Arch", &r.revision, &elsewhere).unwrap_err(), "archive is not inside this workspace's backups folder");

    let garbage = r.dir.join("backups/archive-garbage.zip");
    fs::write(&garbage, b"not a zip").unwrap();
    assert!(ok("Arch", &r.revision, &garbage).unwrap_err().starts_with("archive failed verification"));

    let outside = temp("outside-snapshot");
    let escaped = remove_in(
      &r.root,
      "arch",
      &RemoveRequest { typed_name: "Arch", expected_revision: &r.revision, archive_path: &r.archive, dir: &outside },
      0,
    );
    assert_eq!(escaped.unwrap_err(), "snapshot folder is not inside this workspace's backups");
    assert!(remove_in(
      &r.root,
      "missing",
      &RemoveRequest { typed_name: "Arch", expected_revision: &r.revision, archive_path: &r.archive, dir: &r.snapshot },
      0
    )
    .unwrap_err()
    .contains("not found"));
    untouched(&r);
  }

  #[test]
  fn removal_refuses_when_the_archive_no_longer_matches_the_live_data() {
    let r = removal("drift");
    decide_all(&r);
    let live = open_workspace(&r.dir);

    live.execute_batch("UPDATE plugin_kv SET value = 'changed'").unwrap();
    let message = attempt(&r, "Arch", &r.revision, &r.archive).unwrap_err();
    assert!(message.contains("changed since the archive"), "{message}");
    live.execute_batch("UPDATE plugin_kv SET value = 'v'").unwrap();

    fs::write(r.dir.join("files/orphan/late.bin"), b"late").unwrap();
    assert!(attempt(&r, "Arch", &r.revision, &r.archive).unwrap_err().contains("changed since the archive"));
    fs::remove_file(r.dir.join("files/orphan/late.bin")).unwrap();

    live.execute_batch("INSERT INTO activity_log (at, method) VALUES ('t2', 'POST')").unwrap();
    assert!(attempt(&r, "Arch", &r.revision, &r.archive).unwrap_err().contains("changed since the archive"));
    live.execute_batch("DELETE FROM activity_log WHERE at = 't2'").unwrap();

    live.execute_batch("UPDATE meta SET value = 'x' WHERE key = 'pinned'").unwrap();
    let new_revision = revision_of(&live).unwrap();
    assert_eq!(attempt(&r, "Arch", &new_revision, &r.archive).unwrap_err(), "archive was made from a different revision");
    assert_eq!(attempt(&r, "Arch", &r.revision, &r.archive).unwrap_err(), "the workspace changed since the move snapshot");
    untouched(&r);
  }

  #[test]
  fn removal_refuses_while_a_missing_or_excluded_file_has_no_decision() {
    let r = removal("decisions");
    let message = attempt(&r, "Arch", &r.revision, &r.archive).unwrap_err();
    assert!(message.contains("2 file(s)") && message.contains("u2/big.png") && message.contains("u4/gone.png"), "{message}");

    accept_lost(&r.snapshot, "u4/gone.png").unwrap();
    let message = attempt(&r, "Arch", &r.revision, &r.archive).unwrap_err();
    assert!(message.contains("1 file(s)") && message.contains("u2/big.png"), "{message}");

    accept_lost(&r.snapshot, "u2/big.png").unwrap();
    clear_decision(&r.snapshot, "u2/big.png").unwrap();
    assert!(attempt(&r, "Arch", &r.revision, &r.archive).unwrap_err().contains("u2/big.png"));

    decide_all(&r);
    add_excluded(&r.snapshot, &["u1/a.png".to_string()], "refused").unwrap();
    assert!(attempt(&r, "Arch", &r.revision, &r.archive).unwrap_err().contains("u1/a.png"));
    assert!(add_excluded(&r.snapshot, &["nope/x.png".to_string()], "refused").is_err());
    untouched(&r);
  }

  #[test]
  fn removal_refuses_a_snapshot_folder_from_another_revision() {
    let r = removal("other-snapshot");
    decide_all(&r);
    let mut record = snapshot_record(&r.snapshot).unwrap();
    record.revision = "stale".to_string();
    fs::write(r.snapshot.join("snapshot.json"), serde_json::to_string(&record).unwrap()).unwrap();
    assert_eq!(attempt(&r, "Arch", &r.revision, &r.archive).unwrap_err(), "the snapshot folder belongs to a different revision");
    untouched(&r);
  }

  #[test]
  fn removal_moves_the_folder_drops_data_and_keeps_backups() {
    let r = removal("success");
    decide_all(&r);
    fs::write(r.dir.join("workspace.db-wal"), b"").unwrap();

    let moved = attempt(&r, "Arch", &r.revision, &r.archive).unwrap();
    assert_eq!(moved, r.root.parent().unwrap().join("removed").join("arch-20270115-080140"));
    assert!(!r.dir.exists());
    assert!(local_workspaces::list_in(&r.root).is_empty());
    assert!(!moved.join("workspace.db").exists());
    assert!(!moved.join("workspace.db-wal").exists());
    assert!(!moved.join("files").exists());
    assert!(moved.join("workspace.json").exists());
    assert!(moved.join("backups").join(r.archive.file_name().unwrap()).exists());
    let kept = moved.join("backups").join(r.snapshot.file_name().unwrap());
    assert!(kept.join("workspace.db").exists());
    assert_eq!(fs::read(kept.join("files/u1/a.png")).unwrap(), b"png");
    verify_archive(&moved.join("backups").join(r.archive.file_name().unwrap())).unwrap();
  }

  #[test]
  fn two_removals_of_the_same_code_in_one_second_do_not_collide() {
    let r = removal("twice");
    decide_all(&r);
    let first = attempt(&r, "Arch", &r.revision, &r.archive).unwrap();

    let again = local_workspaces::create_in(&r.root, "Arch", "2026-01-01T00:00:00Z").unwrap();
    assert_eq!(again.manifest.code, "arch");
    let again_dir = PathBuf::from(&again.path);
    let conn = Connection::open(&again.database).unwrap();
    conn.execute_batch(SCHEMA).unwrap();
    conn.execute_batch("INSERT INTO meta VALUES ('schema_version', '12')").unwrap();
    let copy = again_dir.join("backups/.c.db");
    vacuum_into(&conn, &copy).unwrap();
    let archive = write_archive(&again_dir, &copy, 1_800_000_000).unwrap();
    let snapshot = snapshot_of(&again_dir, "arch");
    let second = remove_in(
      &r.root,
      "arch",
      &RemoveRequest {
        typed_name: "Arch",
        expected_revision: &archive.revision,
        archive_path: Path::new(&archive.path),
        dir: &snapshot,
      },
      1_800_000_100,
    )
    .unwrap();
    assert_ne!(first, second);
    assert!(second.exists());
  }

  #[test]
  fn the_bundle_records_which_files_it_left_out() {
    let r = removal("bundle-record");
    let record: BundleRecord = read_json(&r.snapshot.join(BUNDLE_RECORD), "bundle record").unwrap();
    assert_eq!(
      record.excluded,
      [
        ExcludedRecord { key: "u2/big.png".into(), reason: "too_large".into() },
        ExcludedRecord { key: "u4/gone.png".into(), reason: "missing".into() },
      ]
    );
  }

  #[test]
  fn exporting_copies_the_file_verifies_it_and_records_the_decision() {
    let r = removal("export");
    let out = temp("export-out");
    let target = out.join("big-copy.png");
    let decision = export_file(&r.dir, &r.snapshot, "u2/big.png", &target, &r.root).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"123456789");
    assert_eq!(decision, Decision { decision: "exported".into(), size: 9, sha256: sha256_hex(b"123456789") });
    assert_eq!(read_decisions(&r.snapshot).unwrap()["u2/big.png"], decision);

    assert!(export_file(&r.dir, &r.snapshot, "u4/gone.png", &out.join("g.png"), &r.root).unwrap_err().contains("missing"));
    assert!(export_file(&r.dir, &r.snapshot, "nope/x.png", &out.join("n.png"), &r.root).unwrap_err().contains("not part"));
    assert!(export_file(&r.dir, &r.snapshot, "u2/big.png", &r.root.join("inside.png"), &r.root).unwrap_err().contains("outside"));
    assert!(export_file(&r.dir, &r.snapshot, "u2/big.png", &out, &r.root).is_err());
    assert!(export_file(&r.dir, &r.snapshot, "u2/big.png", Path::new("relative.png"), &r.root).is_err());
    assert!(!r.root.join("inside.png").exists());
  }

  #[test]
  fn a_file_that_changed_since_the_snapshot_is_not_exported_or_recorded() {
    let r = removal("export-changed");
    fs::write(file_path(&r.snapshot, "u2/big.png"), b"tampered!").unwrap();
    fs::write(file_path(&r.dir, "u2/big.png"), b"tampered!").unwrap();
    let target = temp("export-changed-out").join("big.png");
    let error = export_file(&r.dir, &r.snapshot, "u2/big.png", &target, &r.root).unwrap_err();
    assert!(error.contains("no longer matches"), "{error}");
    assert!(!target.exists());
    assert!(read_decisions(&r.snapshot).unwrap().is_empty());
  }

  #[test]
  fn accepting_a_loss_and_taking_it_back_are_recorded_in_the_decisions_file() {
    let r = removal("lost");
    accept_lost(&r.snapshot, "u4/gone.png").unwrap();
    assert_eq!(read_decisions(&r.snapshot).unwrap()["u4/gone.png"].decision, "lost");
    assert!(accept_lost(&r.snapshot, "nope/x.png").is_err());
    clear_decision(&r.snapshot, "u4/gone.png").unwrap();
    assert!(read_decisions(&r.snapshot).unwrap().is_empty());
  }

  #[test]
  fn publication_needs_published_verified_and_the_same_revision() {
    let good = serde_json::json!({"data": {"status": "published", "verified": true, "source_revision": "rev"}});
    assert!(publication_verdict(&good, "rev").is_ok());
    assert!(publication_verdict(&good, "other").unwrap_err().contains("different revision"));
    for (body, needle) in [
      (serde_json::json!({"data": {"status": "verified", "verified": true, "source_revision": "rev"}}), "not published"),
      (serde_json::json!({"data": {"status": "published", "verified": false, "source_revision": "rev"}}), "not verified"),
      (serde_json::json!({"data": {"status": "published", "source_revision": "rev"}}), "not verified"),
      (serde_json::json!({"data": {"status": "published", "verified": true}}), "different revision"),
      (serde_json::json!({}), "not published"),
    ] {
      assert!(publication_verdict(&body, "rev").unwrap_err().contains(needle), "{body}");
    }
  }

  #[test]
  fn the_job_url_is_built_only_from_a_safe_id_and_a_secure_base() {
    assert_eq!(job_url("https://tmgr.dev/api/", "ab-12").unwrap(), "https://tmgr.dev/api/imports/ab-12");
    assert_eq!(job_url("https://tmgr.dev/api", "ab-12").unwrap(), "https://tmgr.dev/api/imports/ab-12");
    assert!(job_url("http://127.0.0.1:8080/api/", "ab").is_ok());
    assert!(job_url("http://example.com/api/", "ab").is_err());
    for id in ["", "../x", "a/b", "a?b", "a b"] {
      assert!(job_url("https://tmgr.dev/api/", id).is_err(), "{id}");
    }
  }

  async fn serve_job(
    status: u16,
    location: Option<&'static str>,
    body: &'static str,
  ) -> (String, tokio::sync::oneshot::Receiver<(String, Option<String>)>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}/api/", listener.local_addr().unwrap());
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
      let (stream, _) = listener.accept().await.unwrap();
      let tx = std::sync::Mutex::new(Some(tx));
      let service = hyper::service::service_fn(move |req: hyper::Request<hyper::body::Incoming>| {
        let sender = tx.lock().unwrap().take();
        async move {
          let auth = req.headers().get("authorization").map(|v| v.to_str().unwrap().to_string());
          if let Some(sender) = sender {
            let _ = sender.send((req.uri().path().to_string(), auth));
          }
          let mut response = hyper::Response::builder().status(status);
          if let Some(location) = location {
            response = response.header("location", location);
          }
          Ok::<_, std::convert::Infallible>(response.body(http_body_util::Full::new(hyper::body::Bytes::from(body))).unwrap())
        }
      });
      let _ = hyper::server::conn::http1::Builder::new()
        .serve_connection(hyper_util::rt::TokioIo::new(stream), service)
        .await;
    });
    (base, rx)
  }

  #[tokio::test]
  async fn the_job_is_read_from_the_server_with_the_bearer_token() {
    let (base, seen) = serve_job(200, None, r#"{"data":{"status":"published","verified":true,"source_revision":"rev"}}"#).await;
    let body = fetch_job(&base, "job-1", "secret-token").await.unwrap();
    publication_verdict(&body, "rev").unwrap();
    let (path, auth) = seen.await.unwrap();
    assert_eq!(path, "/api/imports/job-1");
    assert_eq!(auth.as_deref(), Some("Bearer secret-token"));
  }

  #[tokio::test]
  async fn a_server_refusal_redirect_or_junk_answer_is_an_error_without_the_token() {
    for (status, location, body) in [
      (401u16, None, "{}"),
      (404, None, "{}"),
      (302, Some("https://elsewhere.example/"), ""),
      (200, None, "<html>"),
    ] {
      let (base, _) = serve_job(status, location, body).await;
      let error = fetch_job(&base, "job-1", "secret-token").await.unwrap_err();
      assert!(!error.contains("secret-token"), "{error}");
      assert!(error.contains(&status.to_string()) || status == 200, "{error}");
    }
    let unreachable = fetch_job("http://127.0.0.1:9/api/", "job-1", "secret-token").await.unwrap_err();
    assert!(!unreachable.contains("secret-token"));
  }
}
