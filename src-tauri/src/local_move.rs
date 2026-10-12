use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{Number, Value as Json};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Runtime, State};

use crate::local_backups::{rfc3339, stamp, verify};
use crate::local_db::{with_connection, LocalDbs};
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

/// Workspaces whose writers are held back while their content is copied to the cloud. In memory
/// only: a crash or restart starts unfrozen.
#[derive(Clone, Default)]
pub struct Moving(Arc<Mutex<HashSet<String>>>);

impl Moving {
  pub fn is_set(&self, code: &str) -> bool {
    self.0.lock().map(|set| set.contains(code)).unwrap_or(true)
  }

  pub fn check(&self, code: &str) -> Result<(), String> {
    if self.is_set(code) {
      Err(MOVING.to_string())
    } else {
      Ok(())
    }
  }

  pub(crate) fn set(&self, code: &str) -> bool {
    self.0.lock().map(|mut set| set.insert(code.to_string())).unwrap_or(false)
  }

  pub(crate) fn clear(&self, code: &str) {
    if let Ok(mut set) = self.0.lock() {
      set.remove(code);
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

/// Exact content hash over every migrated table: each value carries its SQLite type tag, so a
/// changed row, type or deleted row changes it. Independent of rowids and of the file layout.
pub fn revision_of(conn: &Connection) -> Result<String, String> {
  let mut names = conn
    .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
    .map_err(|e| e.to_string())?
    .query_map([], |row| row.get::<_, String>(0))
    .map_err(|e| e.to_string())?
    .collect::<Result<Vec<_>, _>>()
    .map_err(|e| e.to_string())?;
  names.retain(|name| counted_table(name));

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
pub struct ArchiveFile {
  pub key: String,
  pub size: u64,
  pub sha256: String,
  pub missing: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ArchiveRecord {
  pub revision: String,
  pub created_at: String,
  pub files: Vec<ArchiveFile>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ArchiveResult {
  pub path: String,
  pub revision: String,
  pub entries: usize,
  pub verified: bool,
}

const ARCHIVE_DB: &str = "workspace.db";
const ARCHIVE_META: &str = "archive.json";

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
  loop {
    let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
    if n == 0 {
      break;
    }
    hasher.update(&buffer[..n]);
    zip.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
  }
  Ok((size, hex(&hasher.finalize())))
}

fn write_archive(workspace_dir: &Path, db_copy: &Path, now: i64) -> Result<ArchiveResult, String> {
  let backups = workspace_dir.join("backups");
  let tmp = backups.join(format!(".archive-{}.zip.tmp", stamp(now)));
  let built = (|| {
    let copy = open_read_only(db_copy)?;
    let revision = revision_of(&copy)?;
    let rows = file_rows(&copy)?;
    drop(copy);
    let mut zip = zip::ZipWriter::new(BufWriter::new(File::create(&tmp).map_err(|e| e.to_string())?));
    add_entry(&mut zip, ARCHIVE_DB, db_copy)?;
    let mut files = Vec::new();
    for (_, key, _) in rows {
      let source = file_path(workspace_dir, &key);
      if safe_key(&key) && source.is_file() {
        let (size, sha256) = add_entry(&mut zip, &format!("files/{key}"), &source)?;
        files.push(ArchiveFile { key, size, sha256, missing: false });
      } else {
        files.push(ArchiveFile { key, size: 0, sha256: String::new(), missing: true });
      }
    }
    let record = ArchiveRecord { revision, created_at: rfc3339(now), files };
    zip.start_file(ARCHIVE_META, zip_options(0)).map_err(|e| e.to_string())?;
    zip.write_all(serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?.as_bytes()).map_err(|e| e.to_string())?;
    zip.finish().and_then(|mut w| w.flush().map_err(zip::result::ZipError::Io)).map_err(|e| e.to_string())?;
    let verified = verify_archive(&tmp)?;
    let target = unique_path(backups.join(format!("archive-{}.zip", stamp(now))));
    fs::rename(&tmp, &target).map_err(|e| e.to_string())?;
    Ok(ArchiveResult {
      path: target.to_string_lossy().into_owned(),
      revision: verified.0.revision,
      entries: verified.1,
      verified: true,
    })
  })();
  if built.is_err() {
    let _ = fs::remove_file(&tmp);
  }
  built
}

/// Re-reads the whole archive: every entry's CRC, every file's hash against the recorded one, then the
/// database copy itself (integrity check and revision). Returns the record and the entry count.
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
  let mut seen: HashMap<String, String> = HashMap::new();
  let mut db_tmp: Option<PathBuf> = None;
  let outcome = (|| {
    for index in 0..entries {
      let mut entry = archive.by_index(index).map_err(|e| e.to_string())?;
      let name = entry.name().map_err(|e| e.to_string())?.to_string();
      if name.ends_with('/') {
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
      loop {
        let n = entry.read(&mut buffer).map_err(|e| format!("{name}: {e}"))?;
        if n == 0 {
          break;
        }
        hasher.update(&buffer[..n]);
      }
      seen.insert(name, hex(&hasher.finalize()));
    }
    for file in record.files.iter().filter(|f| !f.missing) {
      match seen.get(&format!("files/{}", file.key)) {
        Some(sha) if *sha == file.sha256 => {}
        Some(_) => return Err(format!("files/{} does not match its recorded hash", file.key)),
        None => return Err(format!("files/{} is missing", file.key)),
      }
    }
    let tmp = db_tmp.as_ref().ok_or_else(|| format!("{ARCHIVE_DB} is missing"))?;
    verify(tmp)?;
    let revision = revision_of(&open_read_only(tmp)?)?;
    if revision != record.revision {
      return Err("database revision does not match the archive record".to_string());
    }
    Ok(())
  })();
  if let Some(tmp) = db_tmp {
    let _ = fs::remove_file(tmp);
  }
  outcome.map(|_| (record, entries))
}

fn removed_root(workspaces_root: &Path) -> PathBuf {
  workspaces_root.parent().unwrap_or(workspaces_root).join("removed")
}

/// Moves a workspace folder to `removed/<code>-<stamp>/` and deletes its database and files there;
/// `backups/` and `workspace.json` stay. Every precondition has its own error.
pub fn remove_in(
  workspaces_root: &Path,
  code: &str,
  typed_name: &str,
  expected_revision: &str,
  live_revision: &str,
  archive_path: &Path,
  now: i64,
) -> Result<PathBuf, String> {
  let workspace = local_workspaces::list_in(workspaces_root)
    .into_iter()
    .find(|w| w.manifest.code == code)
    .ok_or_else(|| format!("local workspace {code} not found"))?;
  if typed_name != workspace.manifest.name {
    return Err("typed name does not match the workspace name".to_string());
  }
  if live_revision != expected_revision {
    return Err("the workspace changed since the move snapshot".to_string());
  }
  let workspace_dir = PathBuf::from(&workspace.path);
  if !archive_path.exists() {
    return Err("archive file not found".to_string());
  }
  if !inside(&workspace_dir.join("backups"), archive_path) {
    return Err("archive is not inside this workspace's backups folder".to_string());
  }
  let (record, _) = verify_archive(archive_path).map_err(|e| format!("archive failed verification: {e}"))?;
  if record.revision != expected_revision {
    return Err("archive was made from a different revision".to_string());
  }

  let removed = removed_root(workspaces_root);
  fs::create_dir_all(&removed).map_err(|e| e.to_string())?;
  let target = unique_path(removed.join(format!("{code}-{}", stamp(now))));
  fs::rename(&workspace_dir, &target).map_err(|e| e.to_string())?;
  for suffix in ["", "-wal", "-shm", "-journal"] {
    let _ = fs::remove_file(target.join(format!("workspace.db{suffix}")));
  }
  let _ = fs::remove_dir_all(target.join("files"));
  Ok(target)
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

#[tauri::command]
pub async fn local_move_freeze<R: Runtime>(
  app: AppHandle<R>,
  dbs: State<'_, LocalDbs>,
  moving: State<'_, Moving>,
  code: String,
) -> Result<usize, String> {
  let newly = moving.set(&code);
  let now = crate::tray::now_secs();
  let result = with_connection(app, &dbs, code.clone(), move |conn| stop_timers(conn, now)).await;
  if result.is_err() && newly {
    moving.clear(&code);
  }
  if let Ok(stopped) = &result {
    log::info!("[local] froze {code} for a cloud move, stopped {stopped} timers");
  }
  result
}

#[tauri::command]
pub fn local_move_unfreeze(moving: State<'_, Moving>, code: String) {
  moving.clear(&code);
}

#[tauri::command]
pub async fn local_source_revision<R: Runtime>(
  app: AppHandle<R>,
  dbs: State<'_, LocalDbs>,
  code: String,
) -> Result<String, String> {
  with_connection(app, &dbs, code, |conn| revision_of(conn)).await
}

#[tauri::command]
pub async fn local_move_snapshot<R: Runtime>(
  app: AppHandle<R>,
  dbs: State<'_, LocalDbs>,
  moving: State<'_, Moving>,
  code: String,
) -> Result<Snapshot, String> {
  if !moving.is_set(&code) {
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
  code: String,
) -> Result<ArchiveResult, String> {
  let workspace = local_workspaces::find(&app, &code)?;
  let workspace_dir = PathBuf::from(&workspace.path);
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
pub async fn local_workspace_remove<R: Runtime>(
  app: AppHandle<R>,
  dbs: State<'_, LocalDbs>,
  moving: State<'_, Moving>,
  code: String,
  typed_name: String,
  expected_revision: String,
  archive_path: String,
) -> Result<(), String> {
  let root = local_workspaces::root(&app)?;
  let newly = moving.set(&code);
  let live = with_connection(app, &dbs, code.clone(), |conn| revision_of(conn)).await;
  let removed = match live {
    Ok(live_revision) => {
      dbs.forget(&code);
      let code = code.clone();
      let now = crate::tray::now_secs();
      tauri::async_runtime::spawn_blocking(move || {
        remove_in(&root, &code, &typed_name, &expected_revision, &live_revision, Path::new(&archive_path), now)
      })
      .await
      .map_err(|e| e.to_string())?
    }
    Err(error) => Err(error),
  };
  match &removed {
    Ok(target) => {
      moving.clear(&code);
      log::info!("[local] removed local workspace {code} to {}", target.display());
    }
    Err(_) if newly => moving.clear(&code),
    Err(_) => {}
  }
  removed.map(|_| ())
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
  fn freeze_flag_is_per_code_and_clears() {
    let moving = Moving::default();
    assert!(moving.check("a").is_ok());
    assert!(moving.set("a"));
    assert!(!moving.set("a"));
    assert_eq!(moving.check("a").unwrap_err(), "workspace is moving");
    assert!(moving.check("b").is_ok());
    moving.clear("a");
    assert!(moving.check("a").is_ok());
    assert!(!Moving::default().is_set("a"));
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

  fn archived(name: &str) -> (PathBuf, PathBuf, ArchiveResult, PathBuf) {
    let root = temp(name);
    let dir = workspace(&root.join("workspaces"), "Arch");
    let live = open_workspace(&dir);
    let copy = dir.join("backups").join(".copy.db");
    vacuum_into(&live, &copy).unwrap();
    let result = write_archive(&dir, &copy, 1_800_000_000).unwrap();
    (root, dir, result, copy)
  }

  #[test]
  fn archive_contains_database_files_and_record_and_verifies() {
    let (_, dir, result, _) = archived("archive");
    assert!(result.verified);
    assert_eq!(result.entries, 5);
    assert_eq!(result.revision, revision_of(&open_workspace(&dir)).unwrap());
    assert!(Path::new(&result.path).starts_with(dir.join("backups")));
    assert!(result.path.ends_with("archive-20270115-080000.zip"));

    let (record, entries) = verify_archive(Path::new(&result.path)).unwrap();
    assert_eq!(entries, 5);
    assert_eq!(record.files.len(), 4);
    assert!(record.files[3].missing);
    let mut zip = zip::ZipArchive::new(File::open(&result.path).unwrap()).unwrap();
    let mut names: Vec<String> = (0..zip.len()).map(|i| zip.by_index(i).unwrap().name().unwrap().to_string()).collect();
    names.sort();
    assert_eq!(names, ["archive.json", "files/u1/a.png", "files/u2/big.png", "files/u3/n.txt", "workspace.db"]);
    let mut png = Vec::new();
    zip.by_name("files/u1/a.png").unwrap().read_to_end(&mut png).unwrap();
    assert_eq!(png, b"png");
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
    let mut zip = zip::ZipArchive::new(File::open(&result.path).unwrap()).unwrap();
    let mut out = zip::ZipWriter::new(File::create(&forged).unwrap());
    for i in 0..zip.len() {
      let mut entry = zip.by_index(i).unwrap();
      let name = entry.name().unwrap().to_string();
      let mut content = Vec::new();
      entry.read_to_end(&mut content).unwrap();
      if name == ARCHIVE_META {
        let mut record: ArchiveRecord = serde_json::from_slice(&content).unwrap();
        record.revision = "0".repeat(64);
        content = serde_json::to_vec(&record).unwrap();
      }
      out.start_file(name, zip_options(0)).unwrap();
      out.write_all(&content).unwrap();
    }
    out.finish().unwrap();
    assert!(verify_archive(Path::new(&forged)).unwrap_err().contains("revision"));
  }

  struct Removal {
    root: PathBuf,
    dir: PathBuf,
    archive: PathBuf,
    revision: String,
  }

  fn removal(name: &str) -> Removal {
    let (root, dir, result, _) = archived(name);
    let revision = result.revision.clone();
    Removal { root: root.join("workspaces"), dir, archive: PathBuf::from(result.path), revision }
  }

  fn attempt(r: &Removal, typed: &str, expected: &str, live: &str, archive: &Path) -> Result<PathBuf, String> {
    remove_in(&r.root, "arch", typed, expected, live, archive, 1_800_000_100)
  }

  #[test]
  fn removal_refuses_each_unmet_precondition_with_its_own_error() {
    let r = removal("refusals");
    let code = r.dir.file_name().unwrap().to_string_lossy().to_string();
    assert_eq!(code, "arch");
    let ok = |typed: &str, expected: &str, live: &str, archive: &Path| attempt(&r, typed, expected, live, archive);

    assert_eq!(ok("arch", &r.revision, &r.revision, &r.archive).unwrap_err(), "typed name does not match the workspace name");
    assert_eq!(ok("Arch ", &r.revision, &r.revision, &r.archive).unwrap_err(), "typed name does not match the workspace name");
    assert_eq!(ok("Arch", &r.revision, "changed", &r.archive).unwrap_err(), "the workspace changed since the move snapshot");
    assert_eq!(ok("Arch", "stale", &r.revision, &r.archive).unwrap_err(), "the workspace changed since the move snapshot");
    assert_eq!(ok("Arch", &r.revision, &r.revision, &r.dir.join("backups/archive-none.zip")).unwrap_err(), "archive file not found");

    let elsewhere = temp("elsewhere").join("archive.zip");
    fs::copy(&r.archive, &elsewhere).unwrap();
    assert_eq!(ok("Arch", &r.revision, &r.revision, &elsewhere).unwrap_err(), "archive is not inside this workspace's backups folder");

    let garbage = r.dir.join("backups/archive-garbage.zip");
    fs::write(&garbage, b"not a zip").unwrap();
    assert!(ok("Arch", &r.revision, &r.revision, &garbage).unwrap_err().starts_with("archive failed verification"));

    let live = open_workspace(&r.dir);
    live.execute_batch("UPDATE meta SET value = 'x' WHERE key = 'pinned'").unwrap();
    let new_revision = revision_of(&live).unwrap();
    assert_eq!(
      ok("Arch", &new_revision, &new_revision, &r.archive).unwrap_err(),
      "archive was made from a different revision"
    );

    assert_eq!(attempt(&r, "Arch", "x", "x", &r.archive).unwrap_err(), "archive was made from a different revision");
    assert!(r.dir.join("workspace.db").exists());
    assert!(r.dir.join("files").exists());
    assert!(!r.root.parent().unwrap().join("removed").exists());
    assert!(remove_in(&r.root, "missing", "Arch", "x", "x", &r.archive, 0).unwrap_err().contains("not found"));
  }

  #[test]
  fn removal_moves_the_folder_drops_data_and_keeps_backups() {
    let r = removal("success");
    let snapshot = {
      let live = open_workspace(&r.dir);
      let dir = snapshot_db(&live, &r.dir, 1_800_000_050).unwrap();
      snapshot_files(&dir, &r.dir, 1_800_000_050).unwrap();
      dir
    };
    fs::write(r.dir.join("workspace.db-wal"), b"").unwrap();

    let moved = attempt(&r, "Arch", &r.revision, &r.revision, &r.archive).unwrap();
    assert_eq!(moved, r.root.parent().unwrap().join("removed").join("arch-20270115-080140"));
    assert!(!r.dir.exists());
    assert!(local_workspaces::list_in(&r.root).is_empty());
    assert!(!moved.join("workspace.db").exists());
    assert!(!moved.join("workspace.db-wal").exists());
    assert!(!moved.join("files").exists());
    assert!(moved.join("workspace.json").exists());
    assert!(moved.join("backups").join(r.archive.file_name().unwrap()).exists());
    let kept = moved.join("backups").join(snapshot.file_name().unwrap());
    assert!(kept.join("workspace.db").exists());
    assert_eq!(fs::read(kept.join("files/u1/a.png")).unwrap(), b"png");
    verify_archive(&moved.join("backups").join(r.archive.file_name().unwrap())).unwrap();
  }

  #[test]
  fn two_removals_of_the_same_code_in_one_second_do_not_collide() {
    let r = removal("twice");
    let first = attempt(&r, "Arch", &r.revision, &r.revision, &r.archive).unwrap();
    let again = local_workspaces::create_in(&r.root, "Arch", "2026-01-01T00:00:00Z").unwrap();
    assert_eq!(again.manifest.code, "arch");
    let conn = Connection::open(&again.database).unwrap();
    conn.execute_batch(SCHEMA).unwrap();
    let rev = revision_of(&conn).unwrap();
    let copy = Path::new(&again.path).join("backups/.c.db");
    vacuum_into(&conn, &copy).unwrap();
    let archive = write_archive(Path::new(&again.path), &copy, 1_800_000_000).unwrap();
    let second = remove_in(&r.root, "arch", "Arch", &rev, &rev, Path::new(&archive.path), 1_800_000_100).unwrap();
    assert_ne!(first, second);
    assert!(second.exists());
  }
}
