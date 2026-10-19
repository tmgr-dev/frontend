use std::fs::File;
use std::io::Read;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

const MAX_BYTES: u64 = 1024 * 1024;

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CategoryRepositories {
  pub category_id: String,
  pub names: Vec<String>,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AllowedEntry {
  pub workspace_code: String,
  pub workspace_id: i64,
  pub expires_at: Option<String>,
  pub expired: bool,
  pub keys: usize,
  pub repos: usize,
  pub repositories: Vec<CategoryRepositories>,
}

#[derive(Serialize, Debug, PartialEq, Default)]
pub struct AllowedList {
  pub entries: Vec<AllowedEntry>,
  pub error: Option<String>,
}

#[derive(Deserialize)]
struct RawFile {
  entries: Vec<Value>,
}

fn names_of(binding: &Value) -> Option<Vec<String>> {
  match binding {
    Value::String(path) => {
      let name = std::path::Path::new(path).file_name()?.to_str()?;
      Some(vec![name.to_owned()])
    }
    Value::Object(set) => {
      if set.values().any(|path| !path.is_string()) {
        return None;
      }
      let mut names: Vec<String> = set.keys().cloned().collect();
      names.sort();
      Some(names)
    }
    _ => None,
  }
}

fn entry_from(value: &Value, now: DateTime<Utc>) -> Option<AllowedEntry> {
  let object = value.as_object()?;
  let workspace_code = object.get("workspace_code")?.as_str()?.to_owned();
  let workspace_id = object.get("workspace_id")?.as_i64()?;
  let (expires_at, expired) = match object.get("expires_at") {
    None | Some(Value::Null) => (None, false),
    Some(Value::String(text)) => {
      let at = DateTime::parse_from_rfc3339(text).ok()?;
      (Some(text.clone()), at.with_timezone(&Utc) <= now)
    }
    Some(_) => return None,
  };
  let keys = object.get("keys").and_then(Value::as_array).map_or(0, Vec::len);
  let bound = object.get("repos").and_then(Value::as_object);
  let repos = bound.map_or(0, |m| m.len());
  let mut repositories = Vec::new();
  for (category_id, binding) in bound.into_iter().flatten() {
    repositories.push(CategoryRepositories { category_id: category_id.clone(), names: names_of(binding)? });
  }
  Some(AllowedEntry { workspace_code, workspace_id, expires_at, expired, keys, repos, repositories })
}

pub fn parse_allowed(bytes: &[u8], now: DateTime<Utc>) -> AllowedList {
  let raw: RawFile = match serde_json::from_slice(bytes) {
    Ok(raw) => raw,
    Err(_) => return AllowedList { entries: Vec::new(), error: Some("runner-allow.json is not valid".to_owned()) },
  };
  let mut entries = Vec::new();
  for value in &raw.entries {
    match entry_from(value, now) {
      Some(entry) => entries.push(entry),
      None => return AllowedList { entries: Vec::new(), error: Some("runner-allow.json is not valid".to_owned()) },
    }
  }
  AllowedList { entries, error: None }
}

fn allow_path(xdg: Option<std::ffi::OsString>, home: Option<std::ffi::OsString>) -> Option<PathBuf> {
  let base = xdg
    .map(PathBuf::from)
    .filter(|p| p.is_absolute())
    .or_else(|| home.map(PathBuf::from).filter(|p| p.is_absolute()).map(|h| h.join(".config")))?;
  Some(base.join("tmgr").join("runner-allow.json"))
}

pub fn read_allowed(now: DateTime<Utc>) -> AllowedList {
  let Some(path) = allow_path(std::env::var_os("XDG_CONFIG_HOME"), std::env::var_os("HOME")) else {
    return AllowedList::default();
  };
  let file = match File::open(&path) {
    Ok(file) => file,
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return AllowedList::default(),
    Err(_) => return AllowedList { entries: Vec::new(), error: Some("cannot read runner-allow.json".to_owned()) },
  };
  let mut bytes = Vec::new();
  if file.take(MAX_BYTES + 1).read_to_end(&mut bytes).is_err() || bytes.len() as u64 > MAX_BYTES {
    return AllowedList { entries: Vec::new(), error: Some("runner-allow.json is too large or unreadable".to_owned()) };
  }
  parse_allowed(&bytes, now)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-10-19T12:00:00Z").unwrap().with_timezone(&Utc)
  }

  const SAMPLE: &str = r#"{"version":1,"entries":[
    {"server":"api.tmgr.dev","user_id":1,"workspace_id":56,"workspace_code":"tmgrdev","allowed_at":"2026-10-01T00:00:00Z","expires_at":"2026-10-20T00:00:00Z",
     "keys":[{"key_id":"a","public_key":"b","device_name":"c"},{"key_id":"d","public_key":"e","device_name":"f"}],"repos":{"127":"/r"}},
    {"server":"api.tmgr.dev","user_id":1,"workspace_id":57,"workspace_code":"old","expires_at":"2026-10-01T00:00:00Z","keys":[],"repos":{}},
    {"server":"api.tmgr.dev","user_id":1,"workspace_id":58,"workspace_code":"forever","expires_at":null,"keys":[{}],"repos":{}}]}"#;

  #[test]
  fn parses_live_expired_and_forever() {
    let list = parse_allowed(SAMPLE.as_bytes(), now());
    assert_eq!(list.error, None);
    assert_eq!(list.entries.len(), 3);
    assert_eq!(list.entries[0].keys, 2);
    assert_eq!(list.entries[0].repos, 1);
    assert!(!list.entries[0].expired);
    assert!(list.entries[1].expired);
    assert_eq!(list.entries[2].expires_at, None);
    assert!(!list.entries[2].expired);
  }

  #[test]
  fn lists_repository_names_for_both_shapes() {
    let sample = r#"{"version":2,"entries":[
      {"workspace_code":"a","workspace_id":1,"keys":[],"repos":{"5":"/home/x/projects/web","7":{"tmgr-dev/frontend":"/f","acme/api":"/a"}}}]}"#;
    let list = parse_allowed(sample.as_bytes(), now());
    assert_eq!(list.error, None);
    let entry = &list.entries[0];
    assert_eq!(entry.repos, 2);
    assert_eq!(
      entry.repositories,
      vec![
        CategoryRepositories { category_id: "5".into(), names: vec!["web".into()] },
        CategoryRepositories { category_id: "7".into(), names: vec!["acme/api".into(), "tmgr-dev/frontend".into()] },
      ]
    );
    let json = serde_json::to_value(entry).unwrap();
    assert_eq!(json["repositories"][0]["categoryId"], "5");
  }

  #[test]
  fn other_repo_binding_shapes_are_invalid() {
    for bad in [r#"1"#, r#"null"#, r#"["/r"]"#, r#"{"a/b":1}"#, r#""/""#] {
      let text = format!(r#"{{"entries":[{{"workspace_code":"x","workspace_id":1,"repos":{{"5":{bad}}}}}]}}"#);
      let list = parse_allowed(text.as_bytes(), now());
      assert!(list.entries.is_empty(), "{bad}");
      assert!(list.error.is_some(), "{bad}");
    }
  }

  #[test]
  fn malformed_is_empty_with_error() {
    for bad in ["not json", "{}", r#"{"entries":[{"workspace_code":"x"}]}"#, r#"{"entries":[{"workspace_code":"x","workspace_id":1,"expires_at":"nope"}]}"#] {
      let list = parse_allowed(bad.as_bytes(), now());
      assert!(list.entries.is_empty(), "{bad}");
      assert!(list.error.is_some(), "{bad}");
    }
  }

  #[test]
  fn path_prefers_absolute_xdg_then_home() {
    let p = allow_path(Some("/x".into()), Some("/h".into())).unwrap();
    assert_eq!(p, PathBuf::from("/x/tmgr/runner-allow.json"));
    let p = allow_path(Some("rel".into()), Some("/h".into())).unwrap();
    assert_eq!(p, PathBuf::from("/h/.config/tmgr/runner-allow.json"));
    assert!(allow_path(None, None).is_none());
  }
}
