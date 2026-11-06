use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::Serialize;
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_dialog::DialogExt;

static TMP_COUNTER: AtomicU64 = AtomicU64::new(0);
const MAX_FILE_BYTES: u64 = 2_500_000;
const MAX_SKILLS: usize = 100;
const MAX_MARKDOWN_BYTES: usize = 16_384;
const MAX_ATTACHMENTS_PER_PERSONA: usize = 20;
const MAX_LINKS: usize = 1000;
const MAX_LINK_BYTES: usize = 4096;
const MAX_FIELD_BYTES: usize = 1024;

fn library_path<R: Runtime>(app: &AppHandle<R>, user_id: i64) -> Result<PathBuf, String> {
  let root = app.path().app_data_dir().map_err(|e| e.to_string())?.join("skill-library");
  library_file(&root, user_id)
}

fn library_file(root: &Path, user_id: i64) -> Result<PathBuf, String> {
  if user_id != -1 {
    crate::local_access::tokens::validate_user_id(user_id)?;
  }
  Ok(root.join(format!("{user_id}.json")))
}

fn valid_slug(slug: &str) -> bool {
  !slug.is_empty() && slug.len() <= 60 && slug.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}

fn valid_uuid(uuid: &str) -> bool {
  !uuid.is_empty() && uuid.len() <= 64 && uuid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
}

fn text_field<'a>(obj: &'a Map<String, Value>, key: &str) -> Result<&'a str, String> {
  match obj.get(key) {
    Some(Value::String(s)) if s.len() <= MAX_FIELD_BYTES => Ok(s),
    _ => Err(format!("skill {key} is missing or too long")),
  }
}

fn validate_skills(skills: &Value) -> Result<(), String> {
  let list = skills.as_array().ok_or("skills must be a list")?;
  if list.len() > MAX_SKILLS {
    return Err(format!("at most {MAX_SKILLS} skills are allowed"));
  }
  let mut seen = std::collections::HashSet::new();
  for skill in list {
    let obj = skill.as_object().ok_or("a skill must be an object")?;
    let slug = text_field(obj, "slug")?;
    if !valid_slug(slug) {
      return Err("invalid skill slug".into());
    }
    if !seen.insert(slug.to_string()) {
      return Err("duplicate skill slug".into());
    }
    match obj.get("markdown") {
      Some(Value::String(s)) if s.len() <= MAX_MARKDOWN_BYTES => {}
      _ => return Err("skill markdown is missing or too large".into()),
    }
    for key in ["title", "when", "updated_at"] {
      if let Some(v) = obj.get(key) {
        if !matches!(v, Value::String(s) if s.len() <= MAX_FIELD_BYTES) {
          return Err(format!("skill {key} is invalid"));
        }
      }
    }
    if let Some(v) = obj.get("version") {
      if !v.is_u64() {
        return Err("skill version is invalid".into());
      }
    }
  }
  Ok(())
}

fn validate_library(data: &Value) -> Result<(), String> {
  let obj = data.as_object().ok_or("library must be an object")?;
  if obj.get("version").and_then(Value::as_u64) != Some(1) {
    return Err("unsupported library version".into());
  }
  validate_skills(obj.get("skills").ok_or("skills are missing")?)?;
  if let Some(links) = obj.get("links") {
    let links = links.as_object().ok_or("links must be an object")?;
    if links.len() > MAX_LINKS {
      return Err("too many links".into());
    }
    for (key, value) in links {
      if key.len() > MAX_FIELD_BYTES || value.to_string().len() > MAX_LINK_BYTES {
        return Err("a link is too large".into());
      }
    }
  }
  if let Some(attachments) = obj.get("attachments") {
    let attachments = attachments.as_array().ok_or("attachments must be a list")?;
    let mut per_persona: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for item in attachments {
      let item = item.as_object().ok_or("an attachment must be an object")?;
      let uuid = text_field(item, "persona_uuid")?;
      let slug = text_field(item, "slug")?;
      if !valid_uuid(uuid) || !valid_slug(slug) {
        return Err("invalid attachment".into());
      }
      let count = per_persona.entry(uuid).or_insert(0);
      *count += 1;
      if *count > MAX_ATTACHMENTS_PER_PERSONA {
        return Err(format!("at most {MAX_ATTACHMENTS_PER_PERSONA} skills per persona"));
      }
    }
  }
  Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
  let dir = path.parent().ok_or("invalid path")?;
  fs::create_dir_all(dir).map_err(|e| e.to_string())?;
  let name = path.file_name().and_then(|n| n.to_str()).ok_or("invalid path")?;
  let tmp = dir.join(format!(".{name}.{}.{}.tmp", std::process::id(), TMP_COUNTER.fetch_add(1, Ordering::Relaxed)));
  let result = (|| -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&tmp, path)
  })();
  if result.is_err() {
    let _ = fs::remove_file(&tmp);
  }
  result.map_err(|e| e.to_string())
}

fn read_capped(path: &Path) -> Result<Option<Vec<u8>>, String> {
  let file = match fs::File::open(path) {
    Ok(f) => f,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
    Err(e) => return Err(e.to_string()),
  };
  if !file.metadata().map_err(|e| e.to_string())?.is_file() {
    return Err("not a regular file".into());
  }
  let mut bytes = Vec::new();
  file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
  if bytes.len() as u64 > MAX_FILE_BYTES {
    return Err("file is too large".into());
  }
  Ok(Some(bytes))
}

fn read_library(path: &Path) -> Result<Option<Value>, String> {
  match read_capped(path)? {
    None => Ok(None),
    Some(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(|e| e.to_string()),
  }
}

fn write_library(path: &Path, data: &Value) -> Result<(), String> {
  validate_library(data)?;
  let bytes = serde_json::to_vec(data).map_err(|e| e.to_string())?;
  if bytes.len() as u64 > MAX_FILE_BYTES {
    return Err("library is too large".into());
  }
  atomic_write(path, &bytes)
}

fn export_bytes(path: &Path) -> Result<Vec<u8>, String> {
  let stored = read_library(path)?.ok_or("nothing to export")?;
  validate_library(&stored)?;
  let skills = stored.get("skills").cloned().unwrap_or(Value::Array(vec![]));
  serde_json::to_vec_pretty(&json!({ "version": 1, "skills": skills })).map_err(|e| e.to_string())
}

fn parse_import(bytes: &[u8]) -> Result<Value, String> {
  if bytes.len() as u64 > MAX_FILE_BYTES {
    return Err("file is too large".into());
  }
  let parsed: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
  let obj = parsed.as_object().ok_or("not a skills file")?;
  if obj.get("version").and_then(Value::as_u64) != Some(1) {
    return Err("unsupported skills file version".into());
  }
  let skills = obj.get("skills").ok_or("skills are missing")?;
  validate_skills(skills)?;
  Ok(json!({ "version": 1, "skills": skills }))
}

fn require_main<R: Runtime>(window: &tauri::Window<R>, command: &str) -> Result<(), String> {
  if window.label() != "main" {
    return Err(format!("{command} may only be called from the main window"));
  }
  Ok(())
}

#[tauri::command]
pub async fn skill_library_read<R: Runtime>(
  app: AppHandle<R>,
  window: tauri::Window<R>,
  user_id: i64,
) -> Result<Option<Value>, String> {
  require_main(&window, "skill_library_read")?;
  let path = library_path(&app, user_id)?;
  tauri::async_runtime::spawn_blocking(move || read_library(&path)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn skill_library_write<R: Runtime>(
  app: AppHandle<R>,
  window: tauri::Window<R>,
  user_id: i64,
  data: Value,
) -> Result<(), String> {
  require_main(&window, "skill_library_write")?;
  let path = library_path(&app, user_id)?;
  tauri::async_runtime::spawn_blocking(move || write_library(&path, &data)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn skill_library_export<R: Runtime>(
  app: AppHandle<R>,
  window: tauri::Window<R>,
  user_id: i64,
) -> Result<bool, String> {
  require_main(&window, "skill_library_export")?;
  let source = library_path(&app, user_id)?;
  let bytes = tauri::async_runtime::spawn_blocking(move || export_bytes(&source))
    .await
    .map_err(|e| e.to_string())??;
  let picked = tauri::async_runtime::spawn_blocking(move || {
    app
      .dialog()
      .file()
      .set_title("Export skills")
      .set_file_name("skills.json")
      .add_filter("JSON", &["json"])
      .blocking_save_file()
  })
  .await
  .map_err(|e| e.to_string())?;
  let Some(picked) = picked else { return Ok(false) };
  let target = picked.into_path().map_err(|e| e.to_string())?;
  tauri::async_runtime::spawn_blocking(move || atomic_write(&target, &bytes))
    .await
    .map_err(|e| e.to_string())??;
  Ok(true)
}

#[tauri::command]
pub async fn skill_library_import_pick<R: Runtime>(
  app: AppHandle<R>,
  window: tauri::Window<R>,
) -> Result<Option<Value>, String> {
  require_main(&window, "skill_library_import_pick")?;
  let picked = tauri::async_runtime::spawn_blocking(move || {
    app
      .dialog()
      .file()
      .set_title("Import skills")
      .add_filter("JSON", &["json"])
      .blocking_pick_file()
  })
  .await
  .map_err(|e| e.to_string())?;
  let Some(picked) = picked else { return Ok(None) };
  let path = picked.into_path().map_err(|e| e.to_string())?;
  tauri::async_runtime::spawn_blocking(move || {
    let bytes = read_capped(&path)?.ok_or("file not found")?;
    parse_import(&bytes).map(Some)
  })
  .await
  .map_err(|e| e.to_string())?
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
pub struct LibraryMigrateOut {
  pub moved: u32,
  pub conflicts: u32,
}

fn migrate_file(src: &Path, dst: &Path) -> Result<LibraryMigrateOut, String> {
  let mut out = LibraryMigrateOut::default();
  if !src.is_file() {
    return Ok(out);
  }
  if dst.exists() {
    out.conflicts = 1;
    return Ok(out);
  }
  fs::create_dir_all(dst.parent().ok_or("invalid path")?).map_err(|e| e.to_string())?;
  fs::rename(src, dst).map_err(|e| e.to_string())?;
  out.moved = 1;
  Ok(out)
}

#[tauri::command]
pub fn skill_library_migrate_local<R: Runtime>(
  app: AppHandle<R>,
  window: tauri::Window<R>,
  to_user_id: i64,
) -> Result<LibraryMigrateOut, String> {
  require_main(&window, "skill_library_migrate_local")?;
  let to = crate::local_access::tokens::validate_user_id(to_user_id)?;
  crate::local_profile::require_claim(&app, to)?;
  migrate_file(&library_path(&app, -1)?, &library_path(&app, to)?)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn skill(slug: &str) -> Value {
    json!({ "slug": slug, "title": "T", "when": "W", "markdown": "---\nname: x\n---\nbody", "version": 1, "updated_at": "2026-11-06T00:00:00Z" })
  }

  fn library(skills: Vec<Value>) -> Value {
    json!({ "version": 1, "skills": skills, "links": {}, "attachments": [] })
  }

  #[test]
  fn user_id_rule_allows_minus_one_and_positive_only() {
    let root = Path::new("/r");
    assert_eq!(library_file(root, -1).unwrap(), root.join("-1.json"));
    assert_eq!(library_file(root, 7).unwrap(), root.join("7.json"));
    assert!(library_file(root, 0).is_err());
    assert!(library_file(root, -2).is_err());
    assert!(library_file(root, 9_007_199_254_740_992).is_err());
  }

  #[test]
  fn accepts_a_valid_library() {
    assert!(validate_library(&library(vec![skill("a-1")])).is_ok());
  }

  #[test]
  fn rejects_bad_slugs() {
    for slug in ["", "A", "a_b", "../x", "a b", &"a".repeat(61)] {
      assert!(validate_library(&library(vec![skill(slug)])).is_err(), "{slug}");
    }
    assert!(validate_library(&library(vec![skill(&"a".repeat(60))])).is_ok());
  }

  #[test]
  fn rejects_duplicate_slugs_and_too_many_skills() {
    assert!(validate_library(&library(vec![skill("a"), skill("a")])).is_err());
    let many: Vec<Value> = (0..101).map(|i| skill(&format!("s{i}"))).collect();
    assert!(validate_library(&library(many)).is_err());
    let max: Vec<Value> = (0..100).map(|i| skill(&format!("s{i}"))).collect();
    assert!(validate_library(&library(max)).is_ok());
  }

  #[test]
  fn rejects_oversized_markdown() {
    let mut big = skill("a");
    big["markdown"] = json!("x".repeat(16_385));
    assert!(validate_library(&library(vec![big])).is_err());
    let mut ok = skill("a");
    ok["markdown"] = json!("x".repeat(16_384));
    assert!(validate_library(&library(vec![ok])).is_ok());
  }

  #[test]
  fn rejects_wrong_version_and_shape() {
    assert!(validate_library(&json!({ "version": 2, "skills": [] })).is_err());
    assert!(validate_library(&json!({ "version": 1 })).is_err());
    assert!(validate_library(&json!([])).is_err());
  }

  #[test]
  fn caps_attachments_per_persona() {
    let mut lib = library(vec![]);
    let items = |n: usize| -> Vec<Value> {
      (0..n).map(|i| json!({ "persona_uuid": "p-1", "slug": format!("s{i}") })).collect()
    };
    lib["attachments"] = json!(items(20));
    assert!(validate_library(&lib).is_ok());
    lib["attachments"] = json!(items(21));
    assert!(validate_library(&lib).is_err());
    lib["attachments"] = json!([{ "persona_uuid": "../x", "slug": "a" }]);
    assert!(validate_library(&lib).is_err());
  }

  #[test]
  fn caps_links_count_and_size() {
    let mut lib = library(vec![]);
    let mut links = Map::new();
    for i in 0..1001 {
      links.insert(format!("k{i}"), json!({}));
    }
    lib["links"] = Value::Object(links);
    assert!(validate_library(&lib).is_err());
    lib["links"] = json!({ "k": { "v": "x".repeat(5000) } });
    assert!(validate_library(&lib).is_err());
  }

  #[test]
  fn write_then_read_round_trips_and_leaves_no_tmp() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("skill-library").join("-1.json");
    assert_eq!(read_library(&path).unwrap(), None);
    let lib = library(vec![skill("a")]);
    write_library(&path, &lib).unwrap();
    assert_eq!(read_library(&path).unwrap(), Some(lib));
    let names: Vec<_> = fs::read_dir(path.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, vec![std::ffi::OsString::from("-1.json")]);
  }

  #[test]
  fn invalid_write_keeps_the_previous_file() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("7.json");
    let good = library(vec![skill("a")]);
    write_library(&path, &good).unwrap();
    assert!(write_library(&path, &library(vec![skill("BAD")])).is_err());
    assert_eq!(read_library(&path).unwrap(), Some(good));
  }

  #[test]
  fn atomic_write_replaces_an_existing_file() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("a.json");
    atomic_write(&path, b"one").unwrap();
    atomic_write(&path, b"two").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "two");
    let names: Vec<_> = fs::read_dir(tmp.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(names, vec![std::ffi::OsString::from("a.json")]);
  }

  #[test]
  fn atomic_write_does_not_touch_a_user_file_named_like_the_old_temp() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("skills.json");
    let user = tmp.path().join("skills.json.tmp");
    fs::write(&user, "mine").unwrap();
    atomic_write(&path, b"x").unwrap();
    assert_eq!(fs::read_to_string(&user).unwrap(), "mine");
  }

  #[test]
  fn read_refuses_a_directory() {
    let tmp = tempfile::tempdir().unwrap();
    assert!(read_capped(tmp.path()).is_err());
  }

  #[test]
  fn read_refuses_an_oversized_file() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("7.json");
    fs::write(&path, vec![b' '; (MAX_FILE_BYTES + 1) as usize]).unwrap();
    assert!(read_library(&path).is_err());
  }

  #[test]
  fn export_keeps_only_version_and_skills() {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("7.json");
    let mut lib = library(vec![skill("a")]);
    lib["links"] = json!({ "k": { "path": "/secret" } });
    lib["attachments"] = json!([{ "persona_uuid": "p", "slug": "a" }]);
    write_library(&path, &lib).unwrap();
    let out: Value = serde_json::from_slice(&export_bytes(&path).unwrap()).unwrap();
    assert_eq!(out, json!({ "version": 1, "skills": [skill("a")] }));
    assert!(export_bytes(&tmp.path().join("none.json")).is_err());
  }

  #[test]
  fn import_checks_size_before_parsing_and_validates() {
    assert!(parse_import(&vec![b'{'; (MAX_FILE_BYTES + 1) as usize]).unwrap_err().contains("too large"));
    assert!(parse_import(b"not json").is_err());
    assert!(parse_import(br#"{"version":2,"skills":[]}"#).is_err());
    assert!(parse_import(br#"{"version":1,"skills":[{"slug":"Bad","markdown":"x"}]}"#).is_err());
    let ok = parse_import(br#"{"version":1,"skills":[{"slug":"a","markdown":"x"}],"links":{"k":1}}"#).unwrap();
    assert_eq!(ok, json!({ "version": 1, "skills": [{ "slug": "a", "markdown": "x" }] }));
  }

  #[test]
  fn migrate_moves_when_target_is_free() {
    let tmp = tempfile::tempdir().unwrap();
    let (src, dst) = (tmp.path().join("-1.json"), tmp.path().join("sub").join("7.json"));
    fs::write(&src, "A").unwrap();
    assert_eq!(migrate_file(&src, &dst).unwrap(), LibraryMigrateOut { moved: 1, conflicts: 0 });
    assert_eq!(fs::read_to_string(&dst).unwrap(), "A");
    assert!(!src.exists());
  }

  #[test]
  fn migrate_never_overwrites_an_existing_target() {
    let tmp = tempfile::tempdir().unwrap();
    let (src, dst) = (tmp.path().join("-1.json"), tmp.path().join("7.json"));
    fs::write(&src, "A").unwrap();
    fs::write(&dst, "OLD").unwrap();
    assert_eq!(migrate_file(&src, &dst).unwrap(), LibraryMigrateOut { moved: 0, conflicts: 1 });
    assert_eq!(fs::read_to_string(&dst).unwrap(), "OLD");
    assert_eq!(fs::read_to_string(&src).unwrap(), "A");
  }

  #[test]
  fn migrate_missing_source_is_a_noop() {
    let tmp = tempfile::tempdir().unwrap();
    let (src, dst) = (tmp.path().join("-1.json"), tmp.path().join("7.json"));
    assert_eq!(migrate_file(&src, &dst).unwrap(), LibraryMigrateOut::default());
    assert!(!dst.exists());
  }
}
