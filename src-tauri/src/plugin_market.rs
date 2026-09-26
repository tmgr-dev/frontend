use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, Url};

use crate::plugin_catalog::Catalog;

const MAX_BUNDLE_BYTES: usize = 3 * 1024 * 1024;
const BUNDLE_ASSET: &str = "tmgr-plugin.json";
const SIGNATURE_ASSET: &str = "tmgr-plugin.json.minisig";
const KEY_ASSET: &str = "tmgr-plugin.pub";
const GITHUB_HOSTS: [&str; 4] =
  ["api.github.com", "github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com"];

/// A downloaded release. `verified` says the catalog vouches for its publisher key; nothing is installed yet.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Release {
  repo: String,
  tag: String,
  sha256: String,
  bundle: String,
  signature: String,
  public_key: String,
  verified: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InstalledPlugin {
  id: String,
  repo: String,
  tag: String,
  sha256: String,
  bundle: String,
  #[serde(default)]
  signature: String,
  #[serde(default)]
  public_key: String,
  /// Filled in when listing, never trusted from the caller.
  #[serde(default)]
  verified: bool,
  #[serde(default)]
  blocked: Option<String>,
}

/// Checks a minisign signature (prehashed Ed25519) over the exact bundle text.
pub fn verify_signature(data: &str, signature: &str, public_key: &str) -> Result<(), String> {
  if signature.len() > 4096 {
    return Err("the signature is too large".into());
  }
  let key = minisign_verify::PublicKey::from_base64(public_key.trim()).map_err(|_| "bad publisher key")?;
  let signature = minisign_verify::Signature::decode(signature).map_err(|_| "bad signature file")?;
  key.verify(data.as_bytes(), &signature, false).map_err(|_| "the signature does not match".to_string())
}

/// The key line of a minisign public key file (or a bare key line).
fn key_line(file: &str) -> String {
  file.lines().map(str::trim).filter(|line| !line.is_empty() && !line.starts_with("untrusted comment:")).last()
    .unwrap_or_default().chars().take(100).collect()
}

/// `owner/repo` from a GitHub link or the short form; anything else is refused.
pub fn parse_repo(input: &str) -> Result<String, String> {
  let trimmed = input.trim().trim_end_matches('/').trim_end_matches(".git");
  let path = trimmed
    .strip_prefix("https://github.com/")
    .or_else(|| trimmed.strip_prefix("github.com/"))
    .unwrap_or(trimmed);
  let mut parts = path.split('/');
  let (Some(owner), Some(repo), None) = (parts.next(), parts.next(), parts.next()) else {
    return Err("expected a GitHub repository like owner/name".into());
  };
  let valid = |s: &str| {
    !s.is_empty() && s.len() <= 100 && s != "." && s != ".." && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
  };
  if !valid(owner) || !valid(repo) {
    return Err("expected a GitHub repository like owner/name".into());
  }
  Ok(format!("{owner}/{repo}"))
}

pub fn github_host(url: &Url) -> bool {
  url.scheme() == "https" && url.host_str().is_some_and(|host| GITHUB_HOSTS.contains(&host))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
  use sha2::{Digest, Sha256};
  Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn client() -> Result<reqwest::Client, String> {
  if rustls::crypto::CryptoProvider::get_default().is_none() {
    let _ = rustls::crypto::ring::default_provider().install_default();
  }
  reqwest::Client::builder()
    .https_only(true)
    .timeout(Duration::from_secs(30))
    .user_agent("TMGR-desktop-plugin-installer")
    // Release assets redirect to GitHub's storage; anywhere else is refused.
    .redirect(reqwest::redirect::Policy::custom(|attempt| {
      if attempt.previous().len() < 5 && github_host(attempt.url()) {
        attempt.follow()
      } else {
        attempt.stop()
      }
    }))
    .build()
    .map_err(|e| e.to_string())
}

async fn read_limited(response: reqwest::Response, limit: usize) -> Result<Vec<u8>, String> {
  let mut response = response.error_for_status().map_err(|e| e.to_string())?;
  let mut bytes = Vec::new();
  while let Some(chunk) = response.chunk().await.map_err(|e| e.to_string())? {
    if bytes.len() + chunk.len() > limit {
      return Err("the plugin bundle is larger than 3 MB".into());
    }
    bytes.extend_from_slice(&chunk);
  }
  Ok(bytes)
}

#[derive(Deserialize)]
struct GithubRelease {
  tag_name: String,
  assets: Vec<GithubAsset>,
}

#[derive(Deserialize)]
struct GithubAsset {
  name: String,
  browser_download_url: String,
}

pub struct Assets {
  pub tag: String,
  /// In the order they were asked for.
  pub files: Vec<String>,
}

pub fn valid_tag(tag: &str) -> bool {
  !tag.is_empty() && tag.len() <= 60 && tag.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
}

/// Named text assets of a repository's latest release, each at most `limit` bytes, from GitHub hosts only.
pub async fn latest_assets(repo: &str, names: &[&str], limit: usize) -> Result<Assets, String> {
  release_assets(repo, None, names, limit).await
}

/// The same for one tagged release, or the latest when `tag` is None.
pub async fn release_assets(repo: &str, tag: Option<&str>, names: &[&str], limit: usize) -> Result<Assets, String> {
  let which = match tag {
    Some(tag) if valid_tag(tag) => format!("tags/{tag}"),
    Some(_) => return Err("bad release tag".into()),
    None => "latest".into(),
  };
  let client = client()?;
  let meta = client
    .get(format!("https://api.github.com/repos/{repo}/releases/{which}"))
    .header("Accept", "application/vnd.github+json")
    .send()
    .await
    .map_err(|e| e.to_string())?;
  let release: GithubRelease =
    serde_json::from_slice(&read_limited(meta, 512 * 1024).await?).map_err(|e| e.to_string())?;
  let mut files = Vec::new();
  for name in names {
    let asset = release
      .assets
      .iter()
      .find(|asset| asset.name == *name)
      .ok_or_else(|| format!("the {} release of {repo} has no {name}", tag.unwrap_or("latest")))?;
    let url: Url = asset.browser_download_url.parse().map_err(|e: url::ParseError| e.to_string())?;
    if !github_host(&url) {
      return Err(format!("{name} is not hosted on GitHub"));
    }
    let bytes = read_limited(client.get(url).send().await.map_err(|e| e.to_string())?, limit).await?;
    files.push(String::from_utf8(bytes).map_err(|_| format!("{name} is not UTF-8 text"))?);
  }
  Ok(Assets { tag: release.tag_name.chars().take(60).collect(), files })
}

/// A signed release of a GitHub repository: the latest, or the tag a shared workspace pinned. The signature is
/// checked here and again on install.
#[tauri::command]
pub async fn plugin_github_release<R: Runtime>(
  app: AppHandle<R>,
  repo: String,
  tag: Option<String>,
) -> Result<Release, String> {
  let repo = parse_repo(&repo)?;
  let assets =
    release_assets(&repo, tag.as_deref(), &[BUNDLE_ASSET, SIGNATURE_ASSET, KEY_ASSET], MAX_BUNDLE_BYTES).await?;
  let [bundle, signature, key_file] = <[String; 3]>::try_from(assets.files).map_err(|_| "missing assets")?;
  let catalog = crate::plugin_catalog::current(&app);
  if let Some(reason) = catalog.blocked_reason("", Some(&repo), Some(&sha256_hex(bundle.as_bytes()))) {
    return Err(format!("this plugin is blocked: {reason}"));
  }
  let entry = catalog.entry_for_repo(&repo);
  let public_key = entry.map(|entry| entry.public_key.clone()).unwrap_or_else(|| key_line(&key_file));
  verify_signature(&bundle, &signature, &public_key)?;
  Ok(Release {
    repo,
    tag: assets.tag,
    sha256: sha256_hex(bundle.as_bytes()),
    bundle,
    signature,
    public_key,
    verified: entry.is_some(),
  })
}

fn installed_root<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
  let home = app.path().home_dir().map_err(|e| e.to_string())?;
  Ok(home.join(".tmgr.dev").join("plugins").join("installed"))
}

fn valid_id(id: &str) -> bool {
  let mut parts = id.split('.');
  let word = |s: Option<&str>| {
    s.is_some_and(|s| !s.is_empty() && s.len() <= 60 && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
  };
  word(parts.next()) && word(parts.next()) && parts.next().is_none()
}

/// Installs only a signed bundle. The publisher key is the catalog's for that repository, else the key the
/// first install pinned, else (first install of an unlisted plugin) the release's own key.
pub fn install_into(root: &Path, plugin: &InstalledPlugin, catalog: &Catalog) -> Result<(), String> {
  if !valid_id(&plugin.id) {
    return Err("bad plugin id".into());
  }
  if sha256_hex(plugin.bundle.as_bytes()) != plugin.sha256 {
    return Err("the bundle does not match its checksum".into());
  }
  if let Some(reason) = catalog.blocked_reason(&plugin.id, Some(&plugin.repo), Some(&plugin.sha256)) {
    return Err(format!("this plugin is blocked: {reason}"));
  }
  let dir = root.join(&plugin.id);
  let existing = fs::read_to_string(dir.join("plugin.json"))
    .ok()
    .and_then(|raw| serde_json::from_str::<InstalledPlugin>(&raw).ok());
  // An id installed from one repository cannot be taken over by a release from another.
  if let Some(existing) = &existing {
    if existing.repo != plugin.repo {
      return Err(format!("{} is already installed from github.com/{}", plugin.id, existing.repo));
    }
  }
  let expected = match (catalog.entry_for_repo(&plugin.repo), catalog.entry_for_id(&plugin.id)) {
    (Some(entry), _) if entry.id != plugin.id => {
      return Err(format!("github.com/{} publishes {}, not {}", plugin.repo, entry.id, plugin.id))
    }
    (None, Some(entry)) => return Err(format!("{} is published from github.com/{}", plugin.id, entry.repo)),
    (Some(entry), _) => entry.public_key.clone(),
    (None, None) => existing.as_ref().map(|e| e.public_key.clone()).unwrap_or_else(|| plugin.public_key.clone()),
  };
  if plugin.public_key != expected {
    return Err("the release is signed with a different key than this plugin's publisher".into());
  }
  verify_signature(&plugin.bundle, &plugin.signature, &plugin.public_key)?;
  fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
  let stored = InstalledPlugin { verified: false, blocked: None, ..plugin.clone() };
  let json = serde_json::to_string(&stored).map_err(|e| e.to_string())?;
  let tmp = dir.join("plugin.json.tmp");
  fs::write(&tmp, json).map_err(|e| e.to_string())?;
  fs::rename(&tmp, dir.join("plugin.json")).map_err(|e| e.to_string())
}

/// Installed plugins whose signature still verifies, each marked verified (catalog publisher) or blocked.
pub fn list_in(root: &Path, catalog: &Catalog) -> Vec<InstalledPlugin> {
  let Ok(entries) = fs::read_dir(root) else { return Vec::new() };
  let mut plugins: Vec<InstalledPlugin> = entries
    .flatten()
    .filter_map(|entry| {
      let raw = fs::read_to_string(entry.path().join("plugin.json")).ok()?;
      let plugin: InstalledPlugin = serde_json::from_str(&raw).ok()?;
      let sound = valid_id(&plugin.id)
        && sha256_hex(plugin.bundle.as_bytes()) == plugin.sha256
        && verify_signature(&plugin.bundle, &plugin.signature, &plugin.public_key).is_ok();
      if !sound {
        log::warn!("[plugins] {} failed its signature check and is not loaded", plugin.id);
        return None;
      }
      let listed = catalog.entry_for_repo(&plugin.repo);
      let key_changed = listed.is_some_and(|entry| entry.public_key != plugin.public_key || entry.id != plugin.id);
      let blocked = catalog
        .blocked_reason(&plugin.id, Some(&plugin.repo), Some(&plugin.sha256))
        .or_else(|| key_changed.then(|| "the publisher key no longer matches the catalog".to_string()));
      Some(InstalledPlugin { verified: listed.is_some() && !key_changed, blocked, ..plugin })
    })
    .collect();
  plugins.sort_by(|a, b| a.id.cmp(&b.id));
  plugins
}

/// Called only after the user confirmed the plugin's permissions in the app.
#[tauri::command]
pub fn plugin_install<R: Runtime>(app: AppHandle<R>, plugin: InstalledPlugin) -> Result<(), String> {
  install_into(&installed_root(&app)?, &plugin, &crate::plugin_catalog::current(&app))
}

#[tauri::command]
pub fn plugins_installed_list<R: Runtime>(app: AppHandle<R>) -> Result<Vec<InstalledPlugin>, String> {
  Ok(list_in(&installed_root(&app)?, &crate::plugin_catalog::current(&app)))
}

#[tauri::command]
pub fn plugin_uninstall<R: Runtime>(app: AppHandle<R>, id: String) -> Result<(), String> {
  if !valid_id(&id) {
    return Err("bad plugin id".into());
  }
  let dir = installed_root(&app)?.join(&id);
  if dir.exists() {
    fs::remove_dir_all(dir).map_err(|e| e.to_string())?;
  }
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  const SIGNED: &str = include_str!("../tests/fixtures/signed-bundle.json");
  const SIGNATURE: &str = include_str!("../tests/fixtures/signed-bundle.json.minisig");
  const PUBLIC_KEY: &str = include_str!("../tests/fixtures/signed-bundle.pub");

  #[test]
  fn verifies_what_the_node_signer_signed() {
    let key = PUBLIC_KEY.trim();
    verify_signature(SIGNED, SIGNATURE, key).unwrap();
    assert!(verify_signature(&SIGNED.replace("acme", "evil"), SIGNATURE, key).is_err());
    assert!(verify_signature(SIGNED, &SIGNATURE.replace("hashed", "hashed2"), key).is_err());
    assert!(verify_signature(SIGNED, SIGNATURE, "RWQf6LRCGA9i53mlYecO4IzT51TGPpvWucNSCh1CBM0QTaLn73Y7GFO3").is_err());
    assert!(verify_signature(SIGNED, "garbage", key).is_err());
  }

  #[test]
  fn accepts_github_links_and_short_names_only() {
    assert_eq!(parse_repo("https://github.com/tmgr-dev/estimate-plugin").unwrap(), "tmgr-dev/estimate-plugin");
    assert_eq!(parse_repo("github.com/acme/plugin.git/").unwrap(), "acme/plugin");
    assert_eq!(parse_repo("acme/plugin").unwrap(), "acme/plugin");
    assert!(parse_repo("https://gitlab.com/acme/plugin").is_err());
    assert!(parse_repo("acme/plugin/releases").is_err());
    assert!(parse_repo("../x").is_err());
    assert!(parse_repo("acme/pl ugin").is_err());
  }

  #[test]
  fn follows_only_github_hosts_over_https() {
    let url = |s: &str| s.parse::<Url>().unwrap();
    assert!(github_host(&url("https://objects.githubusercontent.com/x")));
    assert!(!github_host(&url("http://github.com/x")));
    assert!(!github_host(&url("https://github.com.evil.example/x")));
    assert!(!github_host(&url("https://example.com/x")));
  }

  const OTHER_KEY: &str = include_str!("../tests/fixtures/other-key.pub");
  const OTHER_SIGNATURE: &str = include_str!("../tests/fixtures/signed-bundle.other-key.minisig");

  fn temp(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("tmgr-installed-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    root
  }

  fn signed() -> InstalledPlugin {
    InstalledPlugin {
      id: "acme.timer".into(),
      repo: "acme/timer".into(),
      tag: "v1".into(),
      sha256: sha256_hex(SIGNED.as_bytes()),
      bundle: SIGNED.into(),
      signature: SIGNATURE.into(),
      public_key: PUBLIC_KEY.trim().into(),
      verified: false,
      blocked: None,
    }
  }

  fn resigned() -> InstalledPlugin {
    InstalledPlugin { signature: OTHER_SIGNATURE.into(), public_key: OTHER_KEY.trim().into(), ..signed() }
  }

  fn listing(id: &str, repo: &str, key: &str) -> Catalog {
    serde_json::from_str(&format!(
      r#"{{"serial":1,"plugins":[{{"id":"{id}","repo":"{repo}","public_key":"{}"}}]}}"#,
      key.trim()
    ))
    .unwrap()
  }

  #[test]
  fn installs_only_signed_bundles_that_match_their_checksum() {
    let root = temp("checksum");
    let none = Catalog::default();
    install_into(&root, &signed(), &none).unwrap();
    assert!(install_into(&root, &InstalledPlugin { sha256: "0".repeat(64), ..signed() }, &none).is_err());
    assert!(install_into(&root, &InstalledPlugin { id: "../evil".into(), ..signed() }, &none).is_err());
    assert!(install_into(&root, &InstalledPlugin { signature: "".into(), ..signed() }, &none).is_err());
    let other_repo = InstalledPlugin { repo: "mallory/plugin".into(), ..signed() };
    assert!(install_into(&root, &other_repo, &none).unwrap_err().contains("already installed from github.com/acme/timer"));
    assert_eq!(list_in(&root, &none).len(), 1);
    let edited = SIGNED.replace("acme", "evil");
    let tampered = InstalledPlugin { sha256: sha256_hex(edited.as_bytes()), bundle: edited, ..signed() };
    fs::write(root.join("acme.timer/plugin.json"), serde_json::to_string(&tampered).unwrap()).unwrap();
    assert!(list_in(&root, &none).is_empty(), "a bundle edited on disk no longer verifies");
    let _ = fs::remove_dir_all(&root);
  }

  #[test]
  fn updates_must_keep_the_pinned_publisher_key() {
    let root = temp("pin");
    let none = Catalog::default();
    install_into(&root, &signed(), &none).unwrap();
    assert!(install_into(&root, &resigned(), &none).unwrap_err().contains("different key"));
    let rotated = listing("acme.timer", "acme/timer", OTHER_KEY);
    install_into(&root, &resigned(), &rotated).unwrap();
    let listed = list_in(&root, &rotated);
    assert!(listed[0].verified);
    assert_eq!(listed[0].blocked, None);
    let _ = fs::remove_dir_all(&root);
  }

  #[test]
  fn the_catalog_decides_keys_ids_and_blocks() {
    let root = temp("catalog");
    let wrong_key = listing("acme.timer", "acme/timer", OTHER_KEY);
    assert!(install_into(&root, &signed(), &wrong_key).unwrap_err().contains("different key"));
    let wrong_id = listing("acme.other", "acme/timer", PUBLIC_KEY);
    assert!(install_into(&root, &signed(), &wrong_id).unwrap_err().contains("publishes acme.other"));
    let squatted = listing("acme.timer", "acme/real-timer", PUBLIC_KEY);
    assert!(install_into(&root, &signed(), &squatted).unwrap_err().contains("published from github.com/acme/real-timer"));
    let blocked: Catalog =
      serde_json::from_str(r#"{"serial":1,"blocked":[{"repo":"acme/timer","reason":"compromised"}]}"#).unwrap();
    assert!(install_into(&root, &signed(), &blocked).unwrap_err().contains("compromised"));
    install_into(&root, &signed(), &Catalog::default()).unwrap();
    let listed = list_in(&root, &blocked);
    assert_eq!(listed[0].blocked.as_deref(), Some("compromised"));
    assert!(!listed[0].verified);
    assert!(list_in(&root, &wrong_key)[0].blocked.as_deref().unwrap().contains("no longer matches"));
    let _ = fs::remove_dir_all(&root);
  }

  #[test]
  fn release_tags_are_plain_words() {
    assert!(valid_tag("v1.2.0") && valid_tag("1.0.0-beta+2"));
    assert!(!valid_tag("") && !valid_tag("../latest") && !valid_tag("v1?x=1") && !valid_tag(&"v".repeat(61)));
  }

  #[test]
  fn reads_the_key_line_of_a_public_key_file() {
    assert_eq!(key_line("untrusted comment: minisign public key ABC\nRWQkey\n"), "RWQkey");
    assert_eq!(key_line("RWQkey"), "RWQkey");
  }
}

#[cfg(test)]
mod network {
  #[test]
  #[ignore = "reaches api.github.com"]
  fn reads_a_real_release() {
    let result = tauri::async_runtime::block_on(super::latest_assets("tauri-apps/tauri", &["tmgr-plugin.json"], 1024));
    assert!(result.err().unwrap().contains("has no tmgr-plugin.json"));
  }
}
