use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

/// The TMGR root key. Only a catalog signed with it is trusted; its secret half never leaves the owner.
pub const ROOT_KEY: &str = "RWSzWunavl3M39tsL66KniHgqcCax8peIbIBAHUVG5KbCkrG58LbZz6H";
pub const CATALOG_REPO: &str = "tmgr-dev/tmgr-plugins";
const CATALOG_FILE: &str = "catalog.json";
const MAX_CATALOG_BYTES: usize = 1024 * 1024;

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Entry {
  pub id: String,
  pub repo: String,
  pub public_key: String,
  #[serde(default)]
  pub name: Option<String>,
  #[serde(default)]
  pub description: Option<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Block {
  #[serde(default)]
  pub id: Option<String>,
  #[serde(default)]
  pub repo: Option<String>,
  #[serde(default)]
  pub sha256: Option<String>,
  pub reason: String,
}

/// Verified publishers and the blocklist. `serial` only grows, so an old catalog cannot be replayed.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Catalog {
  pub serial: u64,
  #[serde(default)]
  pub plugins: Vec<Entry>,
  #[serde(default)]
  pub blocked: Vec<Block>,
}

impl Catalog {
  pub fn entry_for_repo(&self, repo: &str) -> Option<&Entry> {
    self.plugins.iter().find(|entry| entry.repo.eq_ignore_ascii_case(repo))
  }

  pub fn entry_for_id(&self, id: &str) -> Option<&Entry> {
    self.plugins.iter().find(|entry| entry.id == id)
  }

  /// Why a plugin must not run, if it is on the blocklist by id, repository or exact release.
  pub fn blocked_reason(&self, id: &str, repo: Option<&str>, sha256: Option<&str>) -> Option<String> {
    self
      .blocked
      .iter()
      .find(|block| {
        block.id.as_deref() == Some(id)
          || block.repo.as_deref().zip(repo).is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
          || block.sha256.as_deref().zip(sha256).is_some_and(|(a, b)| a == b)
      })
      .map(|block| block.reason.chars().take(200).collect())
  }
}

pub fn verify_catalog(json: &str, signature: &str, key: &str) -> Result<Catalog, String> {
  crate::plugin_market::verify_signature(json, signature, key).map_err(|e| format!("catalog: {e}"))?;
  serde_json::from_str(json).map_err(|e| format!("catalog: {e}"))
}

fn read_pair(dir: &Path) -> Option<(String, String)> {
  let json = fs::read_to_string(dir.join(CATALOG_FILE)).ok()?;
  let signature = fs::read_to_string(dir.join(format!("{CATALOG_FILE}.minisig"))).ok()?;
  Some((json, signature))
}

/// The cached catalog, re-verified on every read; an empty one when there is none or it does not verify.
pub fn cached(dir: &Path, key: &str) -> Catalog {
  read_pair(dir).and_then(|(json, signature)| verify_catalog(&json, &signature, key).ok()).unwrap_or_default()
}

/// Verifies a downloaded catalog and caches it, unless it is older than the one already cached.
pub fn accept(dir: &Path, json: &str, signature: &str, key: &str) -> Result<Catalog, String> {
  let catalog = verify_catalog(json, signature, key)?;
  let current = cached(dir, key);
  if catalog.serial < current.serial {
    return Err(format!("catalog {} is older than the cached {}", catalog.serial, current.serial));
  }
  fs::create_dir_all(dir).map_err(|e| e.to_string())?;
  for (name, content) in [(format!("{CATALOG_FILE}.minisig"), signature), (CATALOG_FILE.to_string(), json)] {
    let tmp = dir.join(format!("{name}.tmp"));
    fs::write(&tmp, content).map_err(|e| e.to_string())?;
    fs::rename(&tmp, dir.join(name)).map_err(|e| e.to_string())?;
  }
  Ok(catalog)
}

pub fn catalog_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
  let home = app.path().home_dir().map_err(|e| e.to_string())?;
  Ok(home.join(".tmgr.dev").join("plugins"))
}

pub fn current<R: Runtime>(app: &AppHandle<R>) -> Catalog {
  catalog_dir(app).map(|dir| cached(&dir, ROOT_KEY)).unwrap_or_default()
}

#[tauri::command]
pub fn plugin_catalog<R: Runtime>(app: AppHandle<R>) -> Catalog {
  current(&app)
}

/// Fetches the latest catalog release; on any failure the cached catalog stays in force.
#[tauri::command]
pub async fn plugin_catalog_refresh<R: Runtime>(app: AppHandle<R>) -> Result<Catalog, String> {
  let assets = crate::plugin_market::latest_assets(
    CATALOG_REPO,
    &[CATALOG_FILE, &format!("{CATALOG_FILE}.minisig")],
    MAX_CATALOG_BYTES,
  )
  .await?;
  accept(&catalog_dir(&app)?, &assets.files[0], &assets.files[1], ROOT_KEY)
}

#[cfg(test)]
mod tests {
  use super::*;

  const JSON: &str = include_str!("../tests/fixtures/catalog.json");
  const SIGNATURE: &str = include_str!("../tests/fixtures/catalog.json.minisig");
  const KEY: &str = include_str!("../tests/fixtures/signed-bundle.pub");

  fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tmgr-catalog-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
  }

  #[test]
  fn only_a_signed_catalog_is_trusted() {
    let catalog = verify_catalog(JSON, SIGNATURE, KEY.trim()).unwrap();
    assert_eq!(catalog.serial, 2);
    assert!(verify_catalog(&JSON.replace("steals data", "fine"), SIGNATURE, KEY.trim()).is_err());
    assert!(verify_catalog(JSON, SIGNATURE, ROOT_KEY).is_err());
  }

  #[test]
  fn the_blocklist_matches_id_repo_or_release() {
    let catalog = verify_catalog(JSON, SIGNATURE, KEY.trim()).unwrap();
    assert_eq!(catalog.blocked_reason("evil.plugin", None, None).as_deref(), Some("steals data"));
    assert_eq!(catalog.blocked_reason("x.y", Some("Mallory/Tools"), None).as_deref(), Some("compromised"));
    assert_eq!(catalog.blocked_reason("x.y", Some("a/b"), Some(&"b".repeat(64))).as_deref(), Some("bad release"));
    assert_eq!(catalog.blocked_reason("acme.timer", Some("acme/timer"), Some(&"a".repeat(64))), None);
    assert_eq!(catalog.entry_for_repo("ACME/timer").map(|e| e.id.as_str()), Some("acme.timer"));
  }

  #[test]
  fn an_older_catalog_cannot_replace_a_newer_one() {
    let dir = temp("rollback");
    accept(&dir, JSON, SIGNATURE, KEY.trim()).unwrap();
    assert_eq!(cached(&dir, KEY.trim()).serial, 2);
    let old = include_str!("../tests/fixtures/catalog-old.json");
    let old_signature = include_str!("../tests/fixtures/catalog-old.json.minisig");
    assert!(accept(&dir, old, old_signature, KEY.trim()).unwrap_err().contains("older"));
    assert_eq!(cached(&dir, KEY.trim()).serial, 2);
    let newer = r#"{"serial":3,"plugins":[],"blocked":[]}"#;
    fs::write(dir.join("catalog.json"), newer).unwrap();
    assert_eq!(cached(&dir, KEY.trim()), Catalog::default(), "an edited cache does not verify");
    fs::write(dir.join("catalog.json"), JSON).unwrap();
    assert_eq!(cached(&dir, KEY.trim()).serial, 2);
    let _ = fs::remove_dir_all(&dir);
  }
}

#[cfg(test)]
mod network {
  #[test]
  #[ignore = "reaches api.github.com"]
  fn the_published_catalog_verifies_with_the_root_key() {
    let assets = tauri::async_runtime::block_on(crate::plugin_market::latest_assets(
      super::CATALOG_REPO,
      &["catalog.json", "catalog.json.minisig"],
      super::MAX_CATALOG_BYTES,
    ))
    .unwrap();
    let catalog = super::verify_catalog(&assets.files[0], &assets.files[1], super::ROOT_KEY).unwrap();
    assert!(catalog.serial >= 1);
  }
}

