use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_dialog::DialogExt;

const MAX_SKILLS: usize = 1000;
const MAX_SUPPORTING: usize = 20;
const MAX_CONTENT_BYTES: u64 = 64 * 1024;
const MAX_HASH_BYTES: u64 = 256 * 1024;
const MAX_INSTALLED_BYTES: u64 = 4 * 1024 * 1024;
const PLUGIN_MAX_DEPTH: usize = 4;
const FOLDER_SUBDIRS: [&str; 4] = [".claude/skills", ".codex/skills", ".agents/skills", "skills"];

#[derive(Default)]
pub struct PickedFolders(Mutex<HashMap<String, PathBuf>>);

#[derive(Serialize, Debug)]
pub struct PickedFolder {
  id: String,
  label: String,
}

#[derive(Serialize, Debug, Default)]
pub struct ScanResult {
  sources: Vec<Source>,
  skipped: Vec<Skipped>,
}

#[derive(Serialize, Debug)]
pub struct Skipped {
  path: String,
  reason: String,
}

#[derive(Serialize, Debug)]
pub struct Source {
  id: &'static str,
  label: &'static str,
  packages: Vec<Package>,
}

#[derive(Serialize, Debug)]
pub struct Package {
  id: String,
  name: String,
  version: Option<String>,
  skills: Vec<ScanSkill>,
}

#[derive(Serialize, Debug)]
pub struct ScanSkill {
  key: String,
  dir_name: String,
  path: String,
  content: String,
  hash: String,
  size: u64,
  too_large: bool,
  secret: bool,
  secret_reason: Option<String>,
  symlink_target: Option<String>,
  supporting: Vec<Supporting>,
  supporting_more: usize,
}

#[derive(Serialize, Debug)]
pub struct Supporting {
  name: String,
  size: u64,
}

struct Install {
  marketplace: String,
  plugin: String,
  version: Option<String>,
  path: PathBuf,
  user_scope: bool,
}

struct Scan {
  home: Option<PathBuf>,
  canon_home: Option<PathBuf>,
  visited: HashSet<PathBuf>,
  skipped: Vec<Skipped>,
  count: usize,
  limit_reported: bool,
}

impl Scan {
  fn new(home: Option<&Path>) -> Self {
    Scan {
      home: home.map(Path::to_path_buf),
      canon_home: home.and_then(|h| fs::canonicalize(h).ok()),
      visited: HashSet::new(),
      skipped: Vec::new(),
      count: 0,
      limit_reported: false,
    }
  }

  fn show(&self, path: &Path) -> String {
    for home in [self.home.as_deref(), self.canon_home.as_deref()].into_iter().flatten() {
      if let Ok(rest) = path.strip_prefix(home) {
        return if rest.as_os_str().is_empty() {
          "~".to_owned()
        } else {
          Path::new("~").join(rest).to_string_lossy().into_owned()
        };
      }
    }
    path.to_string_lossy().into_owned()
  }

  fn skip(&mut self, path: &Path, reason: impl Into<String>) {
    let path = self.show(path);
    self.skipped.push(Skipped { path, reason: reason.into() });
  }

  fn read_skill(&mut self, entry: &Path, key_prefix: &str) -> Option<ScanSkill> {
    let dir_name = entry.file_name()?.to_string_lossy().into_owned();
    let meta = fs::symlink_metadata(entry).ok()?;
    let (folder, symlink_target) = if meta.file_type().is_symlink() {
      match fs::canonicalize(entry) {
        Ok(target) if target.is_dir() => {
          let shown = self.show(&target);
          (target, Some(shown))
        }
        Ok(_) => {
          self.skip(entry, "symlink does not point to a folder");
          return None;
        }
        Err(e) => {
          self.skip(entry, format!("broken symlink: {e}"));
          return None;
        }
      }
    } else if meta.is_dir() {
      (entry.to_path_buf(), None)
    } else {
      return None;
    };

    let md = folder.join("SKILL.md");
    let display_md = entry.join("SKILL.md");
    let md_meta = fs::symlink_metadata(&md).ok()?;
    if md_meta.file_type().is_symlink() {
      self.skip(&display_md, "SKILL.md is a symlink, not followed");
      return None;
    }
    if !md_meta.is_file() {
      self.skip(&display_md, "SKILL.md is not a regular file");
      return None;
    }

    let canonical = fs::canonicalize(&folder).unwrap_or_else(|_| folder.clone());
    if !self.visited.insert(canonical) {
      self.skip(entry, "already listed from another location");
      return None;
    }
    if self.count >= MAX_SKILLS {
      if !self.limit_reported {
        self.limit_reported = true;
        self.skip(entry, format!("more than {MAX_SKILLS} skills; the rest are not listed"));
      }
      return None;
    }

    let bytes = match read_bounded(&md, MAX_HASH_BYTES) {
      Ok(Some(bytes)) => bytes,
      Ok(None) => {
        self.skip(&display_md, "SKILL.md is larger than 256 KiB");
        return None;
      }
      Err(e) => {
        self.skip(&display_md, format!("cannot read SKILL.md: {e}"));
        return None;
      }
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
      self.skip(&display_md, "SKILL.md is not valid UTF-8");
      return None;
    };

    let secret = find_secret(text);
    let too_large = bytes.len() as u64 > MAX_CONTENT_BYTES;
    let content = if too_large || secret.is_some() { String::new() } else { text.to_owned() };
    let mut supporting = supporting_files(&folder);
    let supporting_more = supporting.len().saturating_sub(MAX_SUPPORTING);
    supporting.truncate(MAX_SUPPORTING);
    let key = format!("{key_prefix}{dir_name}");
    let path = self.show(&display_md);
    self.count += 1;

    Some(ScanSkill {
      key,
      dir_name,
      path,
      content,
      hash: hex(&Sha256::digest(&bytes)),
      size: bytes.len() as u64,
      too_large,
      secret: secret.is_some(),
      secret_reason: secret.map(str::to_owned),
      symlink_target,
      supporting,
      supporting_more,
    })
  }

  fn skills_in(&mut self, root: &Path, key_prefix: &str, exclude: &[&str]) -> Vec<ScanSkill> {
    let mut out = Vec::new();
    for child in sorted_children(root) {
      let name = child.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
      if exclude.contains(&name.as_str()) {
        continue;
      }
      out.extend(self.read_skill(&child, key_prefix));
    }
    out
  }

  fn claude(&mut self, home: &Path) -> Vec<Package> {
    let skills = self.skills_in(&home.join(".claude").join("skills"), "claude:personal:", &[]);
    personal(skills)
  }

  fn codex(&mut self, home: &Path) -> Vec<Package> {
    let root = home.join(".codex").join("skills");
    let mut packages = personal(self.skills_in(&root, "codex:personal:", &[".system"]));
    let system = self.skills_in(&root.join(".system"), "codex:system:", &[]);
    if !system.is_empty() {
      packages.push(package("system", "System", None, system));
    }
    packages
  }

  fn agents(&mut self, home: &Path) -> Vec<Package> {
    let skills = self.skills_in(&home.join(".agents").join("skills"), "agents:personal:", &[]);
    personal(skills)
  }

  fn plugins(&mut self, home: &Path) -> Vec<Package> {
    let plugins_dir = home.join(".claude").join("plugins");
    let installs = read_installed(&plugins_dir.join("installed_plugins.json"))
      .unwrap_or_else(|| cache_installs(&plugins_dir.join("cache")));
    let mut by_name: BTreeMap<String, Vec<Install>> = BTreeMap::new();
    for install in installs {
      let name = format!("{}/{}", install.marketplace, install.plugin);
      by_name.entry(name).or_default().push(install);
    }
    let mut packages = Vec::new();
    for (name, mut list) in by_name {
      list.sort_by_key(|install| !install.user_scope);
      let mut list = list.into_iter();
      let Some(chosen) = list.next() else { continue };
      for other in list {
        self.skip(&other.path, "another install of this plugin is listed");
      }
      packages.extend(self.plugin_package(&name, chosen));
    }
    packages
  }

  fn plugin_package(&mut self, name: &str, install: Install) -> Option<Package> {
    let root = match fs::canonicalize(&install.path) {
      Ok(root) if root.is_dir() => root,
      _ => {
        self.skip(&install.path, "plugin folder not found");
        return None;
      }
    };
    let mut entries = Vec::new();
    plugin_skill_dirs(&root, 0, &mut entries);
    let prefix = format!("claude-plugin:{name}:");
    let mut skills = Vec::new();
    for entry in entries {
      skills.extend(self.read_skill(&entry, &prefix));
    }
    (!skills.is_empty()).then(|| package(name, name, install.version, skills))
  }

  fn folders(&mut self, folder_ids: &[String], picked: &HashMap<String, PathBuf>) -> Vec<Package> {
    let mut seen = HashSet::new();
    let mut packages = Vec::new();
    for id in folder_ids {
      if !seen.insert(id.as_str()) {
        continue;
      }
      match picked.get(id) {
        Some(root) => packages.extend(self.folder_package(id, root)),
        None => self.skip(Path::new(id), "folder was not picked in this session"),
      }
    }
    packages
  }

  fn folder_package(&mut self, id: &str, root: &Path) -> Option<Package> {
    let label = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| id.to_owned());
    let mut skills = self.skills_in(root, &format!("folder:{id}:"), &[]);
    for sub in FOLDER_SUBDIRS {
      skills.extend(self.skills_in(&root.join(sub), &format!("folder:{id}:{sub}/"), &[]));
    }
    (!skills.is_empty()).then(|| package(&format!("folder:{id}"), &label, None, skills))
  }
}

fn personal(skills: Vec<ScanSkill>) -> Vec<Package> {
  if skills.is_empty() {
    Vec::new()
  } else {
    vec![package("personal", "Personal skills", None, skills)]
  }
}

fn package(id: &str, name: &str, version: Option<String>, skills: Vec<ScanSkill>) -> Package {
  Package { id: id.to_owned(), name: name.to_owned(), version, skills }
}

fn push_source(sources: &mut Vec<Source>, id: &'static str, label: &'static str, mut packages: Vec<Package>) {
  packages.sort_by(|a, b| a.name.cmp(&b.name));
  for package in &mut packages {
    package.skills.sort_by(|a, b| a.dir_name.cmp(&b.dir_name));
  }
  if !packages.is_empty() {
    sources.push(Source { id, label, packages });
  }
}

pub fn scan_roots(home: Option<&Path>, picked: &HashMap<String, PathBuf>, folder_ids: &[String]) -> ScanResult {
  let mut scan = Scan::new(home);
  let mut sources = Vec::new();
  if let Some(home) = home {
    let claude = scan.claude(home);
    push_source(&mut sources, "claude", "Claude Code", claude);
    let plugins = scan.plugins(home);
    push_source(&mut sources, "claude-plugin", "Claude Code plugins", plugins);
    let codex = scan.codex(home);
    push_source(&mut sources, "codex", "Codex", codex);
    let agents = scan.agents(home);
    push_source(&mut sources, "agents", "Shared agents", agents);
  }
  let folders = scan.folders(folder_ids, picked);
  push_source(&mut sources, "folder", "Picked folders", folders);
  ScanResult { sources, skipped: scan.skipped }
}

fn read_bounded(path: &Path, limit: u64) -> std::io::Result<Option<Vec<u8>>> {
  let mut bytes = Vec::new();
  fs::File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
  Ok((bytes.len() as u64 <= limit).then_some(bytes))
}

fn sorted_children(dir: &Path) -> Vec<PathBuf> {
  let Ok(read) = fs::read_dir(dir) else { return Vec::new() };
  let mut paths: Vec<PathBuf> = read.filter_map(|entry| entry.ok().map(|e| e.path())).collect();
  paths.sort();
  paths
}

fn dirs_in(dir: &Path) -> Vec<PathBuf> {
  sorted_children(dir).into_iter().filter(|p| fs::symlink_metadata(p).is_ok_and(|m| m.is_dir())).collect()
}

fn name_of(path: &Path) -> String {
  path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn read_installed(path: &Path) -> Option<Vec<Install>> {
  let bytes = read_bounded(path, MAX_INSTALLED_BYTES).ok()??;
  let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
  let plugins = value.get("plugins").unwrap_or(&value).as_object()?;
  let mut out = Vec::new();
  for (key, entries) in plugins {
    let Some((plugin, marketplace)) = key.rsplit_once('@') else { continue };
    let entries: Vec<&serde_json::Value> = match entries {
      serde_json::Value::Array(items) => items.iter().collect(),
      other => vec![other],
    };
    for entry in entries {
      let Some(install_path) = entry.get("installPath").and_then(|v| v.as_str()) else { continue };
      out.push(Install {
        marketplace: marketplace.to_owned(),
        plugin: plugin.to_owned(),
        version: entry.get("version").and_then(|v| v.as_str()).map(str::to_owned),
        path: PathBuf::from(install_path),
        user_scope: entry.get("scope").and_then(|v| v.as_str()) == Some("user"),
      });
    }
  }
  (!out.is_empty()).then_some(out)
}

fn cache_installs(cache: &Path) -> Vec<Install> {
  let mut out = Vec::new();
  for market in dirs_in(cache) {
    for plugin in dirs_in(&market) {
      let newest = dirs_in(&plugin)
        .into_iter()
        .filter_map(|version| {
          let modified = fs::metadata(&version).ok()?.modified().ok()?;
          Some((modified, version))
        })
        .max_by_key(|(modified, _)| *modified);
      if let Some((_, path)) = newest {
        out.push(Install {
          marketplace: name_of(&market),
          plugin: name_of(&plugin),
          version: Some(name_of(&path)),
          path,
          user_scope: true,
        });
      }
    }
  }
  out
}

fn plugin_skill_dirs(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
  for child in sorted_children(dir) {
    if !fs::symlink_metadata(&child).is_ok_and(|m| m.is_dir()) {
      continue;
    }
    let name = name_of(&child);
    if name == "skills" {
      out.extend(sorted_children(&child));
    } else if depth < PLUGIN_MAX_DEPTH && name != "node_modules" && name != ".git" {
      plugin_skill_dirs(&child, depth + 1, out);
    }
  }
}

fn supporting_files(folder: &Path) -> Vec<Supporting> {
  let mut files: Vec<Supporting> = sorted_children(folder)
    .into_iter()
    .filter_map(|path| {
      let name = name_of(&path);
      if name == "SKILL.md" || looks_secret(&name) {
        return None;
      }
      let meta = fs::symlink_metadata(&path).ok()?;
      meta.is_file().then_some(Supporting { name, size: meta.len() })
    })
    .collect();
  files.sort_by(|a, b| a.name.cmp(&b.name));
  files
}

fn looks_secret(name: &str) -> bool {
  let lower = name.to_ascii_lowercase();
  lower.starts_with(".env")
    || [".pem", ".key", ".p12", ".pfx"].iter().any(|ext| lower.ends_with(ext))
    || lower.starts_with("id_rsa")
    || lower.starts_with("id_ed25519")
    || lower.starts_with("credentials")
    || lower.starts_with("secrets")
    || lower == ".npmrc"
    || lower.contains("token")
}

fn find_secret(text: &str) -> Option<&'static str> {
  if text.lines().any(|line| line.contains("-----BEGIN") && line.contains("PRIVATE KEY-----")) {
    return Some("contains a private key block");
  }
  if secret_run(text, "AKIA", 16, |c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
    return Some("contains an AWS access key id");
  }
  if secret_run(text, "ghp_", 36, |c| c.is_ascii_alphanumeric()) {
    return Some("contains a GitHub token");
  }
  if secret_run(text, "github_pat_", 20, |c| c.is_ascii_alphanumeric() || c == '_') {
    return Some("contains a GitHub token");
  }
  if ["xoxa-", "xoxb-", "xoxp-"].iter().any(|p| secret_run(text, p, 10, |c| c.is_ascii_alphanumeric() || c == '-')) {
    return Some("contains a Slack token");
  }
  if text.match_indices("sk-").any(|(at, _)| {
    text[..at].chars().next_back().is_none_or(|c| !c.is_alphanumeric())
      && text[at + 3..].chars().take_while(|&c| c.is_ascii_alphanumeric() || c == '_' || c == '-').take(20).count() == 20
  }) {
    return Some("contains an API key (sk-)");
  }
  None
}

fn secret_run(text: &str, prefix: &str, min: usize, ok: fn(char) -> bool) -> bool {
  text
    .match_indices(prefix)
    .any(|(at, _)| text[at + prefix.len()..].chars().take_while(|&c| ok(c)).take(min).count() == min)
}

fn hex(bytes: &[u8]) -> String {
  bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn folder_id(root: &Path) -> String {
  hex(&Sha256::digest(root.to_string_lossy().as_bytes()))[..12].to_owned()
}

fn canonical_folder(path: &Path) -> Result<PathBuf, String> {
  let root = fs::canonicalize(path).map_err(|e| e.to_string())?;
  if !root.is_dir() {
    return Err("not a folder".into());
  }
  Ok(root)
}

#[tauri::command]
pub async fn skills_pick_folder<R: Runtime>(
  app: AppHandle<R>,
  state: State<'_, PickedFolders>,
) -> Result<Option<PickedFolder>, String> {
  let picked = tauri::async_runtime::spawn_blocking(move || {
    app.dialog().file().set_title("Choose a folder with skills").blocking_pick_folder()
  })
  .await
  .map_err(|e| e.to_string())?;
  let Some(picked) = picked else { return Ok(None) };
  let path = picked.into_path().map_err(|e| e.to_string())?;
  let root = tauri::async_runtime::spawn_blocking(move || canonical_folder(&path))
    .await
    .map_err(|e| e.to_string())??;
  let id = folder_id(&root);
  let label = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| id.clone());
  state.0.lock().map_err(|e| e.to_string())?.insert(id.clone(), root);
  Ok(Some(PickedFolder { id, label }))
}

#[tauri::command]
pub async fn skills_scan<R: Runtime>(
  app: AppHandle<R>,
  state: State<'_, PickedFolders>,
  folder_ids: Vec<String>,
) -> Result<ScanResult, String> {
  let home = app.path().home_dir().ok();
  let picked = state.0.lock().map_err(|e| e.to_string())?.clone();
  tauri::async_runtime::spawn_blocking(move || scan_roots(home.as_deref(), &picked, &folder_ids))
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::time::{Duration, SystemTime};

  struct Tmp(PathBuf);

  impl Tmp {
    fn new(name: &str) -> Self {
      let dir = std::env::temp_dir().join(format!("tmgr-skill-scan-{}-{name}", std::process::id()));
      let _ = fs::remove_dir_all(&dir);
      fs::create_dir_all(&dir).unwrap();
      Tmp(dir)
    }

    fn path(&self) -> &Path {
      &self.0
    }
  }

  impl Drop for Tmp {
    fn drop(&mut self) {
      let _ = fs::remove_dir_all(&self.0);
    }
  }

  fn write(path: &Path, content: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
  }

  fn pem_header(kind: &str) -> String {
    format!("-----BEGIN {kind} PRIVATE KEY-----")
  }

  fn skill(home: &Path, rel: &str, body: &str) {
    write(&home.join(rel).join("SKILL.md"), body);
  }

  #[cfg(unix)]
  fn link(target: &Path, at: &Path) {
    fs::create_dir_all(at.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(target, at).unwrap();
  }

  fn all_skills(result: &ScanResult) -> Vec<&ScanSkill> {
    result.sources.iter().flat_map(|s| &s.packages).flat_map(|p| &p.skills).collect()
  }

  fn keys(result: &ScanResult) -> Vec<String> {
    all_skills(result).iter().map(|s| s.key.clone()).collect()
  }

  fn pick(picked: &mut HashMap<String, PathBuf>, root: &Path) -> String {
    let root = fs::canonicalize(root).unwrap();
    let id = folder_id(&root);
    picked.insert(id.clone(), root);
    id
  }

  #[test]
  fn lists_personal_claude_skills_with_home_shown_as_tilde() {
    let t = Tmp::new("claude-layout");
    let body = "---\nname: alpha\n---\nBody\n";
    skill(t.path(), ".claude/skills/alpha", body);
    let result = scan_roots(Some(t.path()), &HashMap::new(), &[]);
    assert_eq!(result.sources.len(), 1);
    let source = &result.sources[0];
    assert_eq!(source.id, "claude");
    assert_eq!(source.packages[0].id, "personal");
    assert_eq!(source.packages[0].name, "Personal skills");
    assert!(source.packages[0].version.is_none());
    let s = &source.packages[0].skills[0];
    assert_eq!(s.key, "claude:personal:alpha");
    assert_eq!(s.dir_name, "alpha");
    assert_eq!(s.path, "~/.claude/skills/alpha/SKILL.md");
    assert_eq!(s.content, body);
    assert_eq!(s.hash, hex(&Sha256::digest(body.as_bytes())));
    assert_eq!(s.size, body.len() as u64);
    assert!(!s.too_large && !s.secret && s.secret_reason.is_none() && s.symlink_target.is_none());
    assert!(result.skipped.is_empty());
  }

  #[test]
  fn codex_personal_and_system_packages_are_separate() {
    let t = Tmp::new("codex-layout");
    skill(t.path(), ".codex/skills/beta", "beta");
    skill(t.path(), ".codex/skills/.system/gamma", "gamma");
    let result = scan_roots(Some(t.path()), &HashMap::new(), &[]);
    let source = &result.sources[0];
    assert_eq!(source.id, "codex");
    let names: Vec<&str> = source.packages.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names, ["Personal skills", "System"]);
    assert_eq!(keys(&result), ["codex:personal:beta", "codex:system:gamma"]);
    assert_eq!(source.packages[1].skills[0].path, "~/.codex/skills/.system/gamma/SKILL.md");
  }

  #[test]
  fn agents_layout_uses_agents_keys() {
    let t = Tmp::new("agents-layout");
    skill(t.path(), ".agents/skills/delta", "delta");
    let result = scan_roots(Some(t.path()), &HashMap::new(), &[]);
    assert_eq!(result.sources[0].id, "agents");
    assert_eq!(keys(&result), ["agents:personal:delta"]);
  }

  #[test]
  fn sources_come_in_fixed_order_and_empty_ones_are_omitted() {
    let t = Tmp::new("source-order");
    let folder = t.path().join("project");
    skill(&folder, "tool-x", "x");
    skill(t.path(), ".agents/skills/a", "a");
    skill(t.path(), ".codex/skills/b", "b");
    skill(t.path(), ".claude/skills/c", "c");
    let mut picked = HashMap::new();
    let id = pick(&mut picked, &folder);
    let result = scan_roots(Some(t.path()), &picked, &[id]);
    let ids: Vec<&str> = result.sources.iter().map(|s| s.id).collect();
    assert_eq!(ids, ["claude", "codex", "agents", "folder"]);
  }

  #[test]
  fn packages_and_skills_are_sorted_by_name() {
    let t = Tmp::new("sorting");
    skill(t.path(), ".claude/skills/zeta", "z");
    skill(t.path(), ".claude/skills/alpha", "a");
    skill(t.path(), ".claude/skills/mid", "m");
    let result = scan_roots(Some(t.path()), &HashMap::new(), &[]);
    let dirs: Vec<&str> = result.sources[0].packages[0].skills.iter().map(|s| s.dir_name.as_str()).collect();
    assert_eq!(dirs, ["alpha", "mid", "zeta"]);
  }

  #[test]
  fn plugin_installed_manifest_gives_package_version_and_nested_skills() {
    let t = Tmp::new("plugin-manifest");
    let home = t.path();
    let install = home.join(".claude/plugins/cache/market/tool/1.2.0");
    skill(&install, "skills/s1", "one");
    skill(&install, "nested/skills/s2", "two");
    let manifest = serde_json::json!({
      "version": 2,
      "plugins": { "tool@market": [ { "scope": "user", "installPath": install.to_string_lossy(), "version": "1.2.0" } ] }
    });
    write(&home.join(".claude/plugins/installed_plugins.json"), &manifest.to_string());
    let result = scan_roots(Some(home), &HashMap::new(), &[]);
    let source = &result.sources[0];
    assert_eq!(source.id, "claude-plugin");
    assert_eq!(source.packages[0].id, "market/tool");
    assert_eq!(source.packages[0].name, "market/tool");
    assert_eq!(source.packages[0].version.as_deref(), Some("1.2.0"));
    assert_eq!(keys(&result), ["claude-plugin:market/tool:s1", "claude-plugin:market/tool:s2"]);
    assert_eq!(source.packages[0].skills[0].path, "~/.claude/plugins/cache/market/tool/1.2.0/skills/s1/SKILL.md");
  }

  #[test]
  fn plugin_falls_back_to_newest_cache_version_when_manifest_is_missing_or_broken() {
    let t = Tmp::new("plugin-fallback");
    let home = t.path();
    let cache = home.join(".claude/plugins/cache/market/tool");
    skill(&cache.join("1.0.0"), "skills/old", "old");
    skill(&cache.join("2.0.0"), "skills/new", "new");
    let old = fs::File::open(cache.join("1.0.0")).unwrap();
    old.set_modified(SystemTime::now() - Duration::from_secs(86_400)).unwrap();

    let missing = scan_roots(Some(home), &HashMap::new(), &[]);
    assert_eq!(keys(&missing), ["claude-plugin:market/tool:new"]);
    assert_eq!(missing.sources[0].packages[0].version.as_deref(), Some("2.0.0"));

    write(&home.join(".claude/plugins/installed_plugins.json"), "{ not json");
    let broken = scan_roots(Some(home), &HashMap::new(), &[]);
    assert_eq!(keys(&broken), ["claude-plugin:market/tool:new"]);
  }

  #[test]
  fn plugin_walk_stops_at_depth_limit() {
    let t = Tmp::new("plugin-depth");
    let home = t.path();
    let install = home.join(".claude/plugins/cache/m/p/1");
    skill(&install, "a/b/c/d/skills/deep", "deep");
    skill(&install, "a/b/c/d/e/skills/too-deep", "too deep");
    let result = scan_roots(Some(home), &HashMap::new(), &[]);
    assert_eq!(keys(&result), ["claude-plugin:m/p:deep"]);
  }

  #[test]
  fn symlinked_skill_md_is_skipped_with_reason() {
    let t = Tmp::new("symlink-md");
    let home = t.path();
    write(&home.join("outside.md"), "outside");
    fs::create_dir_all(home.join(".claude/skills/evil")).unwrap();
    link(&home.join("outside.md"), &home.join(".claude/skills/evil/SKILL.md"));
    let result = scan_roots(Some(home), &HashMap::new(), &[]);
    assert!(all_skills(&result).is_empty());
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].path, "~/.claude/skills/evil/SKILL.md");
    assert!(result.skipped[0].reason.contains("symlink"));
  }

  #[cfg(unix)]
  #[test]
  fn symlinked_skill_folder_is_followed_once_and_deduplicated() {
    let t = Tmp::new("symlink-folder");
    let home = t.path();
    skill(home, "store/real-skill", "real");
    link(&home.join("store/real-skill"), &home.join(".claude/skills/linked"));
    link(&home.join("store/real-skill"), &home.join(".claude/skills/linked-again"));
    let result = scan_roots(Some(home), &HashMap::new(), &[]);
    let found = all_skills(&result);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].key, "claude:personal:linked");
    assert_eq!(found[0].content, "real");
    assert_eq!(found[0].symlink_target.as_deref(), Some("~/store/real-skill"));
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].path, "~/.claude/skills/linked-again");
    assert!(result.skipped[0].reason.contains("already listed"));
  }

  #[cfg(unix)]
  #[test]
  fn nothing_inside_a_skill_folder_is_followed() {
    let t = Tmp::new("no-follow");
    let home = t.path();
    write(&home.join("outside-secret.txt"), "outside");
    skill(home, ".claude/skills/gamma", "gamma");
    write(&home.join(".claude/skills/gamma/readme.md"), "readme");
    link(&home.join("outside-secret.txt"), &home.join(".claude/skills/gamma/leak.txt"));
    link(&home.join("store"), &home.join(".claude/skills/gamma/linkdir"));
    let result = scan_roots(Some(home), &HashMap::new(), &[]);
    let found = all_skills(&result);
    assert_eq!(found.len(), 1);
    let gamma = found.iter().find(|s| s.dir_name == "gamma").unwrap();
    let names: Vec<&str> = gamma.supporting.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["readme.md"]);
    assert_eq!(gamma.supporting_more, 0);
  }

  #[test]
  fn secret_named_files_are_never_listed() {
    let t = Tmp::new("secret-names");
    let home = t.path();
    skill(home, ".claude/skills/kit", "kit");
    for name in [
      ".env",
      ".env.local",
      "id_rsa",
      "id_ed25519.pub",
      "server.pem",
      "tls.key",
      "cert.p12",
      "keys.pfx",
      "credentials.json",
      "secrets.yml",
      ".npmrc",
      "api-token.txt",
    ] {
      write(&home.join(".claude/skills/kit").join(name), "x");
    }
    write(&home.join(".claude/skills/kit/notes.md"), "n");
    write(&home.join(".claude/skills/kit/README.md"), "r");
    let result = scan_roots(Some(home), &HashMap::new(), &[]);
    let kit = &all_skills(&result)[0];
    let names: Vec<&str> = kit.supporting.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["README.md", "notes.md"]);
  }

  #[test]
  fn supporting_list_is_capped_at_twenty_with_a_count_of_the_rest() {
    let t = Tmp::new("supporting-cap");
    let home = t.path();
    skill(home, ".claude/skills/many", "many");
    for i in 0..25 {
      write(&home.join(format!(".claude/skills/many/f{i:02}.txt")), "x");
    }
    let result = scan_roots(Some(home), &HashMap::new(), &[]);
    let many = &all_skills(&result)[0];
    assert_eq!(many.supporting.len(), MAX_SUPPORTING);
    assert_eq!(many.supporting_more, 5);
  }

  #[test]
  fn each_secret_pattern_is_flagged_and_similar_text_is_not() {
    let flagged = [
      pem_header("RSA"),
      pem_header("OPENSSH"),
      "key AKIAIOSFODNN7EXAMPLE".to_owned(),
      format!("token ghp_{}", "a".repeat(36)),
      format!("github_pat_{}", "a".repeat(20)),
      "xoxb-1234567890".to_owned(),
      format!("sk-{}", "a".repeat(20)),
      format!("key: sk-ant-api03-{}", "a".repeat(20)),
    ];
    for text in &flagged {
      assert!(find_secret(text).is_some(), "expected a hit: {text}");
    }
    let clean = [
      "task-manager and risk-assessment-long-name-here".to_owned(),
      "password = foo".to_owned(),
      format!("ghp_{}", "a".repeat(35)),
      format!("AKIA{}", "A".repeat(15)),
      "-----BEGIN CERTIFICATE-----".to_owned(),
      "xoxb-short".to_owned(),
    ];
    for text in &clean {
      assert!(find_secret(text).is_none(), "unexpected hit: {text}");
    }
  }

  #[test]
  fn secret_content_is_withheld_but_the_skill_is_listed() {
    let t = Tmp::new("secret-content");
    let home = t.path();
    skill(home, ".claude/skills/leaky", &format!("# leaky\n{}\nabc\n", pem_header("RSA")));
    let result = scan_roots(Some(home), &HashMap::new(), &[]);
    let s = &all_skills(&result)[0];
    assert!(s.secret);
    assert_eq!(s.secret_reason.as_deref(), Some("contains a private key block"));
    assert!(s.content.is_empty());
    assert!(!s.hash.is_empty());
    assert!(result.skipped.is_empty());
  }

  #[test]
  fn large_skill_is_flagged_too_large_without_content() {
    let t = Tmp::new("too-large");
    let home = t.path();
    let body = format!("---\nname: big\n---\n{}", "x".repeat(70 * 1024));
    skill(home, ".claude/skills/big", &body);
    let result = scan_roots(Some(home), &HashMap::new(), &[]);
    let s = &all_skills(&result)[0];
    assert!(s.too_large);
    assert!(s.content.is_empty());
    assert_eq!(s.size, body.len() as u64);
    assert_eq!(s.hash, hex(&Sha256::digest(body.as_bytes())));
  }

  #[test]
  fn skill_over_256_kib_is_skipped_with_reason() {
    let t = Tmp::new("over-256");
    let home = t.path();
    skill(home, ".claude/skills/huge", &"x".repeat(300 * 1024));
    let result = scan_roots(Some(home), &HashMap::new(), &[]);
    assert!(all_skills(&result).is_empty());
    assert!(result.skipped[0].reason.contains("256 KiB"));
  }

  #[test]
  fn non_utf8_skill_is_skipped_with_reason() {
    let t = Tmp::new("non-utf8");
    let home = t.path();
    fs::create_dir_all(home.join(".claude/skills/binary")).unwrap();
    fs::write(home.join(".claude/skills/binary/SKILL.md"), [0xff_u8, 0xfe, 0x00]).unwrap();
    let result = scan_roots(Some(home), &HashMap::new(), &[]);
    assert!(all_skills(&result).is_empty());
    assert!(result.skipped[0].reason.contains("UTF-8"));
  }

  #[test]
  fn user_skill_count_is_capped_at_a_thousand() {
    let t = Tmp::new("count-cap");
    let home = t.path();
    for i in 0..(MAX_SKILLS + 1) {
      skill(home, &format!(".claude/skills/s{i:04}"), "x");
    }
    let result = scan_roots(Some(home), &HashMap::new(), &[]);
    assert_eq!(all_skills(&result).len(), MAX_SKILLS);
    assert!(result.skipped.iter().any(|s| s.reason.contains("more than 1000")));
  }

  #[test]
  fn picked_folder_layouts_use_folder_keys() {
    let t = Tmp::new("folder-layouts");
    let project = t.path().join("project");
    skill(&project, "tool-a", "a");
    skill(&project, ".claude/skills/tool-b", "b");
    skill(&project, ".codex/skills/tool-c", "c");
    skill(&project, ".agents/skills/tool-d", "d");
    skill(&project, "skills/tool-e", "e");
    let mut picked = HashMap::new();
    let id = pick(&mut picked, &project);
    let result = scan_roots(Some(t.path()), &picked, std::slice::from_ref(&id));
    let source = result.sources.iter().find(|s| s.id == "folder").unwrap();
    assert_eq!(source.packages.len(), 1);
    assert_eq!(source.packages[0].id, format!("folder:{id}"));
    assert_eq!(source.packages[0].name, "project");
    assert_eq!(
      keys(&result),
      [
        format!("folder:{id}:tool-a"),
        format!("folder:{id}:.claude/skills/tool-b"),
        format!("folder:{id}:.codex/skills/tool-c"),
        format!("folder:{id}:.agents/skills/tool-d"),
        format!("folder:{id}:skills/tool-e"),
      ]
    );
  }

  #[test]
  fn unknown_folder_id_is_rejected() {
    let t = Tmp::new("unknown-folder");
    let result = scan_roots(Some(t.path()), &HashMap::new(), &["feedface1234".to_owned()]);
    assert!(result.sources.is_empty());
    assert_eq!(result.skipped.len(), 1);
    assert_eq!(result.skipped[0].path, "feedface1234");
    assert!(result.skipped[0].reason.contains("not picked"));
  }

  #[test]
  fn folder_id_is_twelve_stable_hex_characters() {
    let id = folder_id(Path::new("/some/where"));
    assert_eq!(id.len(), 12);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    assert_eq!(id, folder_id(Path::new("/some/where")));
  }
}
