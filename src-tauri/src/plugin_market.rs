use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime, Url};

const MAX_BUNDLE_BYTES: usize = 3 * 1024 * 1024;
const BUNDLE_ASSET: &str = "tmgr-plugin.json";
const GITHUB_HOSTS: [&str; 4] =
  ["api.github.com", "github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com"];

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Release {
  repo: String,
  tag: String,
  sha256: String,
  bundle: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct InstalledPlugin {
  id: String,
  repo: String,
  tag: String,
  sha256: String,
  bundle: String,
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

/// The latest release of a GitHub repository and its `tmgr-plugin.json`. Nothing is installed here.
#[tauri::command]
pub async fn plugin_github_release(repo: String) -> Result<Release, String> {
  let repo = parse_repo(&repo)?;
  let client = client()?;
  let meta = client
    .get(format!("https://api.github.com/repos/{repo}/releases/latest"))
    .header("Accept", "application/vnd.github+json")
    .send()
    .await
    .map_err(|e| e.to_string())?;
  let release: GithubRelease =
    serde_json::from_slice(&read_limited(meta, 512 * 1024).await?).map_err(|e| e.to_string())?;
  let asset = release
    .assets
    .iter()
    .find(|asset| asset.name == BUNDLE_ASSET)
    .ok_or_else(|| format!("the latest release of {repo} has no {BUNDLE_ASSET}"))?;
  let url: Url = asset.browser_download_url.parse().map_err(|e: url::ParseError| e.to_string())?;
  if !github_host(&url) {
    return Err("the plugin bundle is not hosted on GitHub".into());
  }
  let bytes = read_limited(client.get(url).send().await.map_err(|e| e.to_string())?, MAX_BUNDLE_BYTES).await?;
  let bundle = String::from_utf8(bytes).map_err(|_| "the plugin bundle is not UTF-8 text")?;
  Ok(Release { repo, tag: release.tag_name.chars().take(60).collect(), sha256: sha256_hex(bundle.as_bytes()), bundle })
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

pub fn install_into(root: &Path, plugin: &InstalledPlugin) -> Result<(), String> {
  if !valid_id(&plugin.id) {
    return Err("bad plugin id".into());
  }
  if sha256_hex(plugin.bundle.as_bytes()) != plugin.sha256 {
    return Err("the bundle does not match its checksum".into());
  }
  let dir = root.join(&plugin.id);
  fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
  let json = serde_json::to_string(plugin).map_err(|e| e.to_string())?;
  let tmp = dir.join("plugin.json.tmp");
  fs::write(&tmp, json).map_err(|e| e.to_string())?;
  fs::rename(&tmp, dir.join("plugin.json")).map_err(|e| e.to_string())
}

pub fn list_in(root: &Path) -> Vec<InstalledPlugin> {
  let Ok(entries) = fs::read_dir(root) else { return Vec::new() };
  let mut plugins: Vec<InstalledPlugin> = entries
    .flatten()
    .filter_map(|entry| {
      let raw = fs::read_to_string(entry.path().join("plugin.json")).ok()?;
      let plugin: InstalledPlugin = serde_json::from_str(&raw).ok()?;
      // A file edited on disk no longer matches what the user agreed to install.
      (valid_id(&plugin.id) && sha256_hex(plugin.bundle.as_bytes()) == plugin.sha256).then_some(plugin)
    })
    .collect();
  plugins.sort_by(|a, b| a.id.cmp(&b.id));
  plugins
}

/// Called only after the user confirmed the plugin's permissions in the app.
#[tauri::command]
pub fn plugin_install<R: Runtime>(app: AppHandle<R>, plugin: InstalledPlugin) -> Result<(), String> {
  install_into(&installed_root(&app)?, &plugin)
}

#[tauri::command]
pub fn plugins_installed_list<R: Runtime>(app: AppHandle<R>) -> Result<Vec<InstalledPlugin>, String> {
  Ok(list_in(&installed_root(&app)?))
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

  #[test]
  fn installs_only_what_matches_the_checksum() {
    let root = std::env::temp_dir().join(format!("tmgr-installed-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let bundle = "{\"manifest\":{}}".to_string();
    let good = InstalledPlugin {
      id: "acme.plugin".into(),
      repo: "acme/plugin".into(),
      tag: "v1".into(),
      sha256: sha256_hex(bundle.as_bytes()),
      bundle: bundle.clone(),
    };
    install_into(&root, &good).unwrap();
    assert!(install_into(&root, &InstalledPlugin { sha256: "0".repeat(64), ..good.clone() }).is_err());
    assert!(install_into(&root, &InstalledPlugin { id: "../evil".into(), ..good.clone() }).is_err());
    assert_eq!(list_in(&root).len(), 1);
    fs::write(root.join("acme.plugin/plugin.json"), serde_json::to_string(&InstalledPlugin { bundle: "tampered".into(), ..good }).unwrap()).unwrap();
    assert!(list_in(&root).is_empty());
    let _ = fs::remove_dir_all(&root);
  }
}

#[cfg(test)]
mod network {
  #[test]
  #[ignore = "reaches api.github.com"]
  fn reads_a_real_release() {
    let result = tauri::async_runtime::block_on(super::plugin_github_release("tauri-apps/tauri".into()));
    assert!(result.unwrap_err().contains("has no tmgr-plugin.json"));
  }
}
