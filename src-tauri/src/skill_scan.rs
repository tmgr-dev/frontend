use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_dialog::DialogExt;

use crate::skill_files as sf;

const MAX_SKILLS: usize = 1000;
const MAX_LISTED_FILES: usize = 500;
const MAX_CONTENT_BYTES: u64 = 64 * 1024;
const MAX_HASH_BYTES: u64 = 256 * 1024;
const MAX_INSTALLED_BYTES: u64 = 4 * 1024 * 1024;
const PLUGIN_MAX_DEPTH: usize = 4;
const FOLDER_SUBDIRS: [&str; 7] = [
  ".claude/skills",
  ".codex/skills",
  ".agents/skills",
  "skills",
  ".cursor/skills",
  ".gemini/skills",
  ".opencode/skills",
];

#[derive(Default)]
pub struct PickedFolders(Mutex<HashMap<String, PathBuf>>);

#[derive(Serialize, Debug)]
pub struct PickedFolder {
  id: String,
  label: String,
}

#[derive(Default)]
pub struct ScannedSkills(Mutex<HashMap<String, SkillPreview>>);

#[derive(Clone, Debug)]
pub struct SkillPreview {
  folder: PathBuf,
  files: Vec<PreviewFile>,
}

#[derive(Clone, Debug)]
struct PreviewFile {
  path: String,
  size: u64,
  sha256: String,
}

#[derive(Serialize, Debug, Default)]
pub struct ScanResult {
  sources: Vec<Source>,
  skipped: Vec<Skipped>,
  #[serde(skip)]
  previews: HashMap<String, SkillPreview>,
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
  files: Vec<SkillFile>,
  files_sha256: String,
  files_blocked: Option<String>,
  files_truncated: bool,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
pub struct SkillFile {
  path: String,
  size: u64,
  status: &'static str,
  reason: Option<String>,
  sha256: Option<String>,
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
  previews: HashMap<String, SkillPreview>,
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
      previews: HashMap::new(),
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
    if !self.visited.insert(canonical.clone()) {
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
    let listing = list_files(&canonical);
    let key = format!("{key_prefix}{dir_name}");
    self.previews.insert(
      key.clone(),
      SkillPreview {
        folder: canonical,
        files: listing
          .files
          .iter()
          .filter(|f| f.status == "ok")
          .filter_map(|f| Some(PreviewFile { path: f.path.clone(), size: f.size, sha256: f.sha256.clone()? }))
          .collect(),
      },
    );
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
      files: listing.files,
      files_sha256: listing.sha256,
      files_blocked: listing.blocked,
      files_truncated: listing.truncated,
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

  fn cursor(&mut self, home: &Path) -> Vec<Package> {
    let skills = self.skills_in(&home.join(".cursor").join("skills"), "cursor:personal:", &[]);
    personal(skills)
  }

  fn gemini(&mut self, home: &Path) -> Vec<Package> {
    let skills = self.skills_in(&home.join(".gemini").join("skills"), "gemini:personal:", &[]);
    personal(skills)
  }

  fn opencode(&mut self, home: &Path) -> Vec<Package> {
    let skills = self.skills_in(&home.join(".config").join("opencode").join("skills"), "opencode:personal:", &[]);
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
    let mut skills = Vec::new();
    for entry in entries {
      let parent = entry.parent().and_then(|p| p.strip_prefix(&root).ok()).map(|p| p.to_string_lossy().replace('\\', "/"));
      let prefix = format!("claude-plugin:{name}:{}/", parent.unwrap_or_default());
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
    let cursor = scan.cursor(home);
    push_source(&mut sources, "cursor", "Cursor", cursor);
    let gemini = scan.gemini(home);
    push_source(&mut sources, "gemini", "Gemini CLI", gemini);
    let opencode = scan.opencode(home);
    push_source(&mut sources, "opencode", "opencode", opencode);
  }
  let folders = scan.folders(folder_ids, picked);
  push_source(&mut sources, "folder", "Picked folders", folders);
  ScanResult { sources, skipped: scan.skipped, previews: scan.previews }
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

struct Listing {
  files: Vec<SkillFile>,
  sha256: String,
  blocked: Option<String>,
  truncated: bool,
}

fn entry(path: &str, size: u64, status: &'static str, reason: Option<&str>) -> SkillFile {
  SkillFile { path: printable(path), size, status, reason: reason.map(str::to_owned), sha256: None }
}

fn printable(text: &str) -> String {
  text.chars().map(|c| if c.is_control() { '?' } else { c }).collect()
}

#[cfg(unix)]
fn open_nofollow(path: &Path) -> std::io::Result<fs::File> {
  use std::os::unix::fs::OpenOptionsExt;
  fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(path)
}

#[cfg(not(unix))]
fn open_nofollow(path: &Path) -> std::io::Result<fs::File> {
  fs::File::open(path)
}

fn not_regular() -> std::io::Error {
  std::io::Error::new(std::io::ErrorKind::InvalidInput, "not a regular file")
}

fn read_regular(path: &Path, limit: u64) -> std::io::Result<Option<Vec<u8>>> {
  let before = fs::symlink_metadata(path)?;
  if !before.is_file() {
    return Err(not_regular());
  }
  let file = open_nofollow(path)?;
  let meta = file.metadata()?;
  if !meta.is_file() {
    return Err(not_regular());
  }
  #[cfg(unix)]
  {
    use std::os::unix::fs::MetadataExt;
    if meta.dev() != before.dev() || meta.ino() != before.ino() {
      return Err(not_regular());
    }
  }
  let mut bytes = Vec::new();
  file.take(limit + 1).read_to_end(&mut bytes)?;
  Ok((bytes.len() as u64 <= limit).then_some(bytes))
}

fn classify_file(path: &Path, rel: &str, name: &str, size: u64) -> SkillFile {
  if looks_secret(name) {
    return entry(rel, size, "secret", Some("file name looks like a secret"));
  }
  if size > sf::MAX_FILE_BYTES as u64 {
    return entry(rel, size, "too_large", Some("larger than 64 KiB"));
  }
  let bytes = match read_regular(path, sf::MAX_FILE_BYTES as u64) {
    Ok(Some(bytes)) => bytes,
    Ok(None) => return entry(rel, size, "too_large", Some("larger than 64 KiB")),
    Err(e) => return entry(rel, size, "unreadable", Some(&e.to_string())),
  };
  let Some(text) = sf::text_of(&bytes) else {
    return entry(rel, size, "binary", Some("not UTF-8 text"));
  };
  if let Some(reason) = find_secret(text) {
    return entry(rel, size, "secret", Some(reason));
  }
  SkillFile {
    path: rel.to_owned(),
    size: bytes.len() as u64,
    status: "ok",
    reason: None,
    sha256: Some(sf::sha256_hex(&bytes)),
  }
}

fn walk(dir: &Path, prefix: &str, segments: usize, files: &mut Vec<SkillFile>, truncated: &mut bool) {
  for child in sorted_children(dir) {
    if files.len() >= MAX_LISTED_FILES {
      *truncated = true;
      return;
    }
    let name = name_of(&child);
    let rel = format!("{prefix}{name}");
    let Ok(meta) = fs::symlink_metadata(&child) else { continue };
    if prefix.is_empty() && name == "SKILL.md" {
      continue;
    }
    let size = if meta.is_file() { meta.len() } else { 0 };
    if name.starts_with('.') || name == "node_modules" {
      files.push(entry(&rel, size, "skipped_hidden", Some("hidden file or folder")));
    } else if meta.file_type().is_symlink() {
      files.push(entry(&rel, 0, "symlink", Some("symlinks are not followed")));
    } else if !sf::valid_segment(&name) {
      files.push(entry(&rel, size, "unsupported_name", Some("name is not supported")));
    } else if meta.is_dir() {
      if segments + 1 >= sf::MAX_SEGMENTS {
        files.push(entry(&rel, 0, "too_deep", Some("deeper than 5 levels")));
      } else {
        walk(&child, &format!("{rel}/"), segments + 1, files, truncated);
      }
    } else if meta.is_file() {
      if !sf::valid_path(&rel) {
        files.push(entry(&rel, size, "unsupported_name", Some("path is too long")));
      } else {
        files.push(classify_file(&child, &rel, &name, size));
      }
    }
  }
}

fn list_files(folder: &Path) -> Listing {
  let mut files = Vec::new();
  let mut truncated = false;
  walk(folder, "", 0, &mut files, &mut truncated);
  files.sort_by(|a, b| a.path.as_bytes().cmp(b.path.as_bytes()));

  let mut seen: HashMap<String, usize> = HashMap::new();
  for file in files.iter().filter(|f| f.status == "ok") {
    *seen.entry(sf::path_key(&file.path)).or_insert(0) += 1;
  }
  for file in &mut files {
    if file.status != "ok" {
      continue;
    }
    if !file.path.contains('/') && sf::is_reserved(&file.path) {
      file.status = "case_conflict";
      file.reason = Some("differs from SKILL.md only by case".into());
    } else if seen[&sf::path_key(&file.path)] > 1 {
      file.status = "case_conflict";
      file.reason = Some("another file differs only by case".into());
    }
    if file.status != "ok" {
      file.sha256 = None;
    }
  }

  let ok: Vec<&SkillFile> = files.iter().filter(|f| f.status == "ok").collect();
  let total: u64 = ok.iter().map(|f| f.size).sum();
  let blocked = if let Some(secret) = files.iter().find(|f| f.status == "secret") {
    Some(format!("{} looks like it contains a secret", secret.path))
  } else if truncated {
    Some(format!("more than {MAX_LISTED_FILES} files"))
  } else if ok.len() > sf::MAX_FILES {
    Some(format!("more than {} files", sf::MAX_FILES))
  } else if total > sf::MAX_SKILL_BYTES as u64 {
    Some(format!("files total more than {} KiB", sf::MAX_SKILL_BYTES / 1024))
  } else {
    None
  };
  let sha256 = sf::files_sha256(ok.iter().filter_map(|f| Some((f.path.as_str(), f.sha256.as_deref()?))));
  Listing { files, sha256, blocked, truncated }
}

pub(crate) fn looks_secret(name: &str) -> bool {
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

pub(crate) fn find_secret(text: &str) -> Option<&'static str> {
  if text.lines().any(|line| line.contains("-----BEGIN") && line.contains("PRIVATE KEY")) {
    return Some("contains a private key block");
  }
  if secret_run(text, "AKIA", 16, |c| c.is_ascii_uppercase() || c.is_ascii_digit()) {
    return Some("contains an AWS access key id");
  }
  if ["ghp_", "gho_", "ghu_", "ghs_", "ghr_"].iter().any(|p| secret_run(text, p, 36, |c| c.is_ascii_alphanumeric())) {
    return Some("contains a GitHub token");
  }
  if secret_run(text, "github_pat_", 20, |c| c.is_ascii_alphanumeric() || c == '_') {
    return Some("contains a GitHub token");
  }
  if ["xoxa-", "xoxb-", "xoxp-", "xoxs-", "xoxr-", "xapp-"].iter().any(|p| secret_run(text, p, 10, |c| c.is_ascii_alphanumeric() || c == '-')) {
    return Some("contains a Slack token");
  }
  if ["sk_live_", "rk_live_"].iter().any(|p| secret_run(text, p, 16, |c| c.is_ascii_alphanumeric()))
    || secret_run(text, "glpat-", 20, |c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    || secret_run(text, "AIza", 35, |c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
  {
    return Some("contains an API token");
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
  scanned: State<'_, ScannedSkills>,
  folder_ids: Vec<String>,
) -> Result<ScanResult, String> {
  let home = app.path().home_dir().ok();
  let picked = state.0.lock().map_err(|e| e.to_string())?.clone();
  let mut result = tauri::async_runtime::spawn_blocking(move || scan_roots(home.as_deref(), &picked, &folder_ids))
    .await
    .map_err(|e| e.to_string())?;
  scanned.0.lock().map_err(|e| e.to_string())?.extend(std::mem::take(&mut result.previews));
  Ok(result)
}

const CHANGED: &str = "changed since preview, rescan";

#[derive(Serialize, Debug)]
pub struct FileContent {
  path: String,
  content: String,
  size: u64,
  sha256: String,
}

#[derive(Serialize, Debug)]
pub struct SkillFilesRead {
  key: String,
  ok: bool,
  error: Option<String>,
  files: Vec<FileContent>,
  files_sha256: String,
}

fn read_preview(preview: &SkillPreview) -> Result<Vec<FileContent>, ()> {
  if fs::canonicalize(&preview.folder).map_err(|_| ())? != preview.folder {
    return Err(());
  }
  let total: u64 = preview.files.iter().map(|f| f.size).sum();
  if preview.files.len() > sf::MAX_FILES || total > sf::MAX_SKILL_BYTES as u64 {
    return Err(());
  }
  let mut out = Vec::new();
  for file in &preview.files {
    if !sf::valid_path(&file.path) || sf::is_reserved(&file.path) {
      return Err(());
    }
    let target = preview.folder.join(&file.path);
    if fs::canonicalize(&target).map_err(|_| ())? != target {
      return Err(());
    }
    let bytes = read_regular(&target, file.size).map_err(|_| ())?.ok_or(())?;
    if bytes.len() as u64 != file.size || sf::sha256_hex(&bytes) != file.sha256 {
      return Err(());
    }
    let text = sf::text_of(&bytes).ok_or(())?;
    if find_secret(text).is_some() {
      return Err(());
    }
    out.push(FileContent {
      path: file.path.clone(),
      content: text.to_owned(),
      size: file.size,
      sha256: file.sha256.clone(),
    });
  }
  Ok(out)
}

pub fn read_skill_files(previews: &HashMap<String, SkillPreview>, keys: &[String]) -> Vec<SkillFilesRead> {
  keys
    .iter()
    .map(|key| {
      let read = previews.get(key).ok_or(()).and_then(read_preview);
      match read {
        Ok(files) => {
          let files_sha256 = sf::files_sha256(previews[key].files.iter().map(|f| (f.path.as_str(), f.sha256.as_str())));
          SkillFilesRead { key: key.clone(), ok: true, error: None, files, files_sha256 }
        }
        Err(()) => SkillFilesRead {
          key: key.clone(),
          ok: false,
          error: Some(CHANGED.to_owned()),
          files: Vec::new(),
          files_sha256: String::new(),
        },
      }
    })
    .collect()
}

#[tauri::command]
pub async fn skills_read_files(
  scanned: State<'_, ScannedSkills>,
  keys: Vec<String>,
) -> Result<Vec<SkillFilesRead>, String> {
  let previews = scanned.0.lock().map_err(|e| e.to_string())?.clone();
  tauri::async_runtime::spawn_blocking(move || read_skill_files(&previews, &keys)).await.map_err(|e| e.to_string())
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
  fn cursor_gemini_and_opencode_layouts_use_their_own_keys() {
    let t = Tmp::new("new-agent-layouts");
    skill(t.path(), ".cursor/skills/x", "x");
    skill(t.path(), ".gemini/skills/y", "y");
    skill(t.path(), ".config/opencode/skills/z", "z");
    let result = scan_roots(Some(t.path()), &HashMap::new(), &[]);
    let found: Vec<(&str, &str, &str, &str)> = result
      .sources
      .iter()
      .map(|s| (s.id, s.label, s.packages[0].name.as_str(), s.packages[0].skills[0].path.as_str()))
      .collect();
    assert_eq!(
      found,
      [
        ("cursor", "Cursor", "Personal skills", "~/.cursor/skills/x/SKILL.md"),
        ("gemini", "Gemini CLI", "Personal skills", "~/.gemini/skills/y/SKILL.md"),
        ("opencode", "opencode", "Personal skills", "~/.config/opencode/skills/z/SKILL.md"),
      ]
    );
    assert_eq!(keys(&result), ["cursor:personal:x", "gemini:personal:y", "opencode:personal:z"]);
  }

  #[cfg(unix)]
  #[test]
  fn skill_reachable_from_agents_and_cursor_is_listed_once_under_agents() {
    let t = Tmp::new("agents-cursor-link");
    let home = t.path();
    skill(home, ".agents/skills/shared", "shared");
    link(&home.join(".agents/skills/shared"), &home.join(".cursor/skills/shared"));
    let result = scan_roots(Some(home), &HashMap::new(), &[]);
    assert_eq!(keys(&result), ["agents:personal:shared"]);
    assert!(result.skipped.iter().any(|s| s.path == "~/.cursor/skills/shared" && s.reason.contains("already listed")));
  }

  #[test]
  fn sources_come_in_fixed_order_and_empty_ones_are_omitted() {
    let t = Tmp::new("source-order");
    let folder = t.path().join("project");
    skill(&folder, "tool-x", "x");
    skill(t.path(), ".agents/skills/a", "a");
    skill(t.path(), ".codex/skills/b", "b");
    skill(t.path(), ".claude/skills/c", "c");
    skill(t.path(), ".cursor/skills/d", "d");
    skill(t.path(), ".gemini/skills/e", "e");
    skill(t.path(), ".config/opencode/skills/f", "f");
    let mut picked = HashMap::new();
    let id = pick(&mut picked, &folder);
    let result = scan_roots(Some(t.path()), &picked, &[id]);
    let ids: Vec<&str> = result.sources.iter().map(|s| s.id).collect();
    assert_eq!(ids, ["claude", "codex", "agents", "cursor", "gemini", "opencode", "folder"]);
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
    assert_eq!(keys(&result), ["claude-plugin:market/tool:skills/s1", "claude-plugin:market/tool:nested/skills/s2"]);
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
    assert_eq!(keys(&missing), ["claude-plugin:market/tool:skills/new"]);
    assert_eq!(missing.sources[0].packages[0].version.as_deref(), Some("2.0.0"));

    write(&home.join(".claude/plugins/installed_plugins.json"), "{ not json");
    let broken = scan_roots(Some(home), &HashMap::new(), &[]);
    assert_eq!(keys(&broken), ["claude-plugin:market/tool:skills/new"]);
  }

  #[test]
  fn plugin_walk_stops_at_depth_limit() {
    let t = Tmp::new("plugin-depth");
    let home = t.path();
    let install = home.join(".claude/plugins/cache/m/p/1");
    skill(&install, "a/b/c/d/skills/deep", "deep");
    skill(&install, "a/b/c/d/e/skills/too-deep", "too deep");
    let result = scan_roots(Some(home), &HashMap::new(), &[]);
    assert_eq!(keys(&result), ["claude-plugin:m/p:a/b/c/d/skills/deep"]);
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

  fn files_of(result: &ScanResult, dir: &str) -> Vec<(String, &'static str)> {
    let skill = all_skills(result).into_iter().find(|s| s.dir_name == dir).unwrap();
    skill.files.iter().map(|f| (f.path.clone(), f.status)).collect()
  }

  fn status_of(result: &ScanResult, dir: &str, path: &str) -> &'static str {
    files_of(result, dir).into_iter().find(|(p, _)| p == path).map(|(_, s)| s).unwrap_or("absent")
  }

  fn one(home: &Path) -> ScanResult {
    scan_roots(Some(home), &HashMap::new(), &[])
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
    let result = one(home);
    assert_eq!(all_skills(&result).len(), 1);
    assert_eq!(
      files_of(&result, "gamma"),
      [("leak.txt".to_owned(), "symlink"), ("linkdir".to_owned(), "symlink"), ("readme.md".to_owned(), "ok")]
    );
  }

  #[test]
  fn listing_is_recursive_with_sizes_hashes_and_the_root_skill_md_left_out() {
    let t = Tmp::new("recursive");
    let home = t.path();
    skill(home, ".claude/skills/kit", "kit");
    write(&home.join(".claude/skills/kit/references/guide.md"), "guide");
    write(&home.join(".claude/skills/kit/scripts/run.sh"), "echo hi\n");
    write(&home.join(".claude/skills/kit/references/SKILL.md"), "nested");
    let result = one(home);
    let kit = &all_skills(&result)[0];
    let paths: Vec<&str> = kit.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["references/SKILL.md", "references/guide.md", "scripts/run.sh"]);
    assert!(kit.files.iter().all(|f| f.status == "ok"));
    let guide = &kit.files[1];
    assert_eq!(guide.size, 5);
    assert_eq!(guide.sha256.as_deref(), Some(sf::sha256_hex(b"guide").as_str()));
    assert_eq!(
      kit.files_sha256,
      sf::files_sha256(kit.files.iter().map(|f| (f.path.as_str(), f.sha256.as_deref().unwrap())))
    );
    assert!(kit.files_blocked.is_none());
  }

  #[test]
  fn hidden_files_dirs_node_modules_and_git_are_listed_as_skipped_and_never_entered() {
    let t = Tmp::new("hidden");
    let home = t.path();
    skill(home, ".claude/skills/kit", "kit");
    write(&home.join(".claude/skills/kit/.hidden"), "x");
    write(&home.join(".claude/skills/kit/.git/config"), "x");
    write(&home.join(".claude/skills/kit/node_modules/pkg/index.js"), "x");
    write(&home.join(".claude/skills/kit/notes.md"), "n");
    let result = one(home);
    assert_eq!(
      files_of(&result, "kit"),
      [
        (".git".to_owned(), "skipped_hidden"),
        (".hidden".to_owned(), "skipped_hidden"),
        ("node_modules".to_owned(), "skipped_hidden"),
        ("notes.md".to_owned(), "ok"),
      ]
    );
  }

  #[test]
  fn binary_and_too_large_files_are_listed_with_status_and_not_hashed() {
    let t = Tmp::new("binary");
    let home = t.path();
    skill(home, ".claude/skills/kit", "kit");
    fs::write(home.join(".claude/skills/kit/logo.png"), [0x89, 0x50, 0x00, 0x47]).unwrap();
    fs::write(home.join(".claude/skills/kit/latin1.txt"), [0xff, 0xfe, 0x41]).unwrap();
    fs::write(home.join(".claude/skills/kit/big.txt"), vec![b'a'; 64 * 1024 + 1]).unwrap();
    fs::write(home.join(".claude/skills/kit/edge.txt"), vec![b'a'; 64 * 1024]).unwrap();
    let result = one(home);
    assert_eq!(status_of(&result, "kit", "logo.png"), "binary");
    assert_eq!(status_of(&result, "kit", "latin1.txt"), "binary");
    assert_eq!(status_of(&result, "kit", "big.txt"), "too_large");
    assert_eq!(status_of(&result, "kit", "edge.txt"), "ok");
    let kit = &all_skills(&result)[0];
    assert!(kit.files.iter().filter(|f| f.status != "ok").all(|f| f.sha256.is_none()));
    assert!(kit.files_blocked.is_none());
  }

  #[test]
  fn secret_files_block_the_skill_files_by_name_and_by_content() {
    let t = Tmp::new("secret-files");
    let home = t.path();
    skill(home, ".claude/skills/named", "kit");
    write(&home.join(".claude/skills/named/credentials.json"), "{}");
    write(&home.join(".claude/skills/named/api-token.txt"), "x");
    write(&home.join(".claude/skills/named/ok.md"), "fine");
    skill(home, ".claude/skills/content", "kit");
    write(&home.join(".claude/skills/content/deploy.sh"), &format!("{}\nabc\n", pem_header("RSA")));
    let result = one(home);
    assert_eq!(status_of(&result, "named", "credentials.json"), "secret");
    assert_eq!(status_of(&result, "named", "api-token.txt"), "secret");
    assert_eq!(status_of(&result, "named", "ok.md"), "ok");
    assert_eq!(status_of(&result, "content", "deploy.sh"), "secret");
    for dir in ["named", "content"] {
      let skill = all_skills(&result).into_iter().find(|s| s.dir_name == dir).unwrap();
      assert!(skill.files_blocked.as_deref().unwrap().contains("secret"));
      let shown = serde_json::to_string(skill).unwrap();
      assert!(!shown.contains("PRIVATE KEY"));
    }
  }

  #[test]
  fn depth_counts_from_the_skill_folder_and_five_parts_is_the_limit() {
    let t = Tmp::new("depth");
    let home = t.path();
    skill(home, ".claude/skills/kit", "kit");
    write(&home.join(".claude/skills/kit/a/b/c/d/ok.md"), "x");
    write(&home.join(".claude/skills/kit/a/b/c/d/e/deep.md"), "x");
    let result = one(home);
    assert_eq!(status_of(&result, "kit", "a/b/c/d/ok.md"), "ok");
    assert_eq!(status_of(&result, "kit", "a/b/c/d/e"), "too_deep");
    assert_eq!(status_of(&result, "kit", "a/b/c/d/e/deep.md"), "absent");
  }

  #[cfg(unix)]
  #[test]
  fn files_differing_only_by_case_are_both_rejected() {
    let t = Tmp::new("case");
    let home = t.path();
    skill(home, ".claude/skills/kit", "kit");
    write(&home.join(".claude/skills/kit/A.md"), "a");
    write(&home.join(".claude/skills/kit/a.md"), "b");
    write(&home.join(".claude/skills/kit/refs/Same.md"), "1");
    write(&home.join(".claude/skills/kit/refs/other.md"), "1");
    write(&home.join(".claude/skills/kit/skill.md"), "lower");
    let result = one(home);
    let names = files_of(&result, "kit");
    if names.iter().filter(|(p, _)| p.eq_ignore_ascii_case("a.md")).count() == 2 {
      assert_eq!(status_of(&result, "kit", "A.md"), "case_conflict");
      assert_eq!(status_of(&result, "kit", "a.md"), "case_conflict");
    }
    if names.iter().any(|(p, _)| p == "skill.md") && !names.iter().any(|(p, _)| p == "SKILL.md") {
      assert_eq!(status_of(&result, "kit", "skill.md"), "case_conflict");
    }
    assert_eq!(status_of(&result, "kit", "refs/other.md"), "ok");
  }

  #[test]
  fn unsupported_names_are_listed_and_never_uploaded() {
    let t = Tmp::new("names");
    let home = t.path();
    skill(home, ".claude/skills/kit", "kit");
    write(&home.join(".claude/skills/kit/my notes.md"), "x");
    write(&home.join(".claude/skills/kit/naïve.md"), "x");
    write(&home.join(".claude/skills/kit/-dash.md"), "x");
    write(&home.join(".claude/skills/kit/bad dir/inner.md"), "x");
    write(&home.join(".claude/skills/kit/fine.md"), "x");
    let result = one(home);
    for path in ["my notes.md", "naïve.md", "-dash.md", "bad dir"] {
      assert_eq!(status_of(&result, "kit", path), "unsupported_name", "{path}");
    }
    assert_eq!(status_of(&result, "kit", "bad dir/inner.md"), "absent");
    assert_eq!(status_of(&result, "kit", "fine.md"), "ok");
  }

  #[test]
  fn more_than_fifty_files_or_256_kib_block_the_skill_files() {
    let t = Tmp::new("limits");
    let home = t.path();
    skill(home, ".claude/skills/many", "many");
    for i in 0..51 {
      write(&home.join(format!(".claude/skills/many/f{i:02}.txt")), "x");
    }
    skill(home, ".claude/skills/fifty", "fifty");
    for i in 0..50 {
      write(&home.join(format!(".claude/skills/fifty/f{i:02}.txt")), "x");
    }
    skill(home, ".claude/skills/heavy", "heavy");
    for i in 0..5 {
      fs::write(home.join(format!(".claude/skills/heavy/h{i}.txt")), vec![b'a'; 60 * 1024]).unwrap();
    }
    let result = one(home);
    let blocked = |dir: &str| all_skills(&result).into_iter().find(|s| s.dir_name == dir).unwrap().files_blocked.clone();
    assert!(blocked("many").unwrap().contains("50 files"));
    assert!(blocked("fifty").is_none());
    assert!(blocked("heavy").unwrap().contains("256 KiB"));
  }

  fn read_one(result: &ScanResult, dir: &str) -> SkillFilesRead {
    let key = all_skills(result).into_iter().find(|s| s.dir_name == dir).unwrap().key.clone();
    read_skill_files(&result.previews, &[key]).remove(0)
  }

  fn kit_with_files(name: &str) -> (Tmp, ScanResult) {
    let t = Tmp::new(name);
    let home = t.path().to_path_buf();
    skill(&home, ".claude/skills/kit", "kit");
    write(&home.join(".claude/skills/kit/references/guide.md"), "guide");
    write(&home.join(".claude/skills/kit/run.sh"), "echo hi");
    let result = one(&home);
    (t, result)
  }

  #[test]
  fn import_read_returns_the_previewed_files_and_hash() {
    let (_t, result) = kit_with_files("read-ok");
    let read = read_one(&result, "kit");
    assert!(read.ok, "{:?}", read.error);
    let got: Vec<(&str, &str)> = read.files.iter().map(|f| (f.path.as_str(), f.content.as_str())).collect();
    assert_eq!(got, [("references/guide.md", "guide"), ("run.sh", "echo hi")]);
    assert_eq!(read.files_sha256, all_skills(&result)[0].files_sha256);
  }

  #[test]
  fn import_read_fails_the_whole_skill_when_a_file_changed_or_vanished() {
    let (t, result) = kit_with_files("read-changed");
    let dir = t.path().join(".claude/skills/kit");
    fs::write(dir.join("run.sh"), "echo bye").unwrap();
    let read = read_one(&result, "kit");
    assert!(!read.ok);
    assert!(read.files.is_empty());
    assert_eq!(read.error.as_deref(), Some("changed since preview, rescan"));
    fs::write(dir.join("run.sh"), "echo hi").unwrap();
    assert!(read_one(&result, "kit").ok);
    fs::remove_file(dir.join("references/guide.md")).unwrap();
    assert!(!read_one(&result, "kit").ok);
  }

  #[test]
  fn import_read_rejects_same_size_changed_content() {
    let (t, result) = kit_with_files("read-secret");
    let dir = t.path().join(".claude/skills/kit");
    fs::write(dir.join("run.sh"), "echo ho").unwrap();
    assert!(!read_one(&result, "kit").ok);
  }

  #[cfg(unix)]
  #[test]
  fn import_read_refuses_a_symlink_swapped_in_for_a_file_or_a_folder() {
    let (t, result) = kit_with_files("read-symlink");
    let dir = t.path().join(".claude/skills/kit");
    write(&t.path().join("outside.txt"), "echo hi");
    fs::remove_file(dir.join("run.sh")).unwrap();
    link(&t.path().join("outside.txt"), &dir.join("run.sh"));
    assert!(!read_one(&result, "kit").ok);
    fs::remove_file(dir.join("run.sh")).unwrap();
    write(&dir.join("run.sh"), "echo hi");
    assert!(read_one(&result, "kit").ok);
    write(&t.path().join("elsewhere/guide.md"), "guide");
    fs::remove_dir_all(dir.join("references")).unwrap();
    link(&t.path().join("elsewhere"), &dir.join("references"));
    assert!(!read_one(&result, "kit").ok);
  }

  #[test]
  fn import_read_rejects_unknown_keys_and_escaping_paths() {
    let (_t, result) = kit_with_files("read-escape");
    let unknown = read_skill_files(&result.previews, &["nope".to_owned()]).remove(0);
    assert!(!unknown.ok);
    let mut previews = result.previews.clone();
    let key = all_skills(&result)[0].key.clone();
    for bad in ["../outside.txt", "/etc/passwd", "a/./b", "SKILL.md"] {
      let mut tampered = previews[&key].clone();
      tampered.files = vec![PreviewFile { path: bad.to_owned(), size: 1, sha256: "x".into() }];
      previews.insert(key.clone(), tampered);
      assert!(!read_skill_files(&previews, std::slice::from_ref(&key))[0].ok, "{bad}");
    }
  }

  #[test]
  fn each_secret_pattern_is_flagged_and_similar_text_is_not() {
    let flagged = [
      pem_header("RSA"),
      pem_header("OPENSSH"),
      pem_header("PGP"),
      format!("gho_{}", "a".repeat(36)),
      format!("sk_live_{}", "a".repeat(16)),
      format!("glpat-{}", "a".repeat(20)),
      format!("AIza{}", "a".repeat(35)),
      "xapp-1234567890".to_owned(),
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
    skill(&project, ".cursor/skills/tool-f", "f");
    skill(&project, ".gemini/skills/tool-g", "g");
    skill(&project, ".opencode/skills/tool-h", "h");
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
        format!("folder:{id}:.cursor/skills/tool-f"),
        format!("folder:{id}:.gemini/skills/tool-g"),
        format!("folder:{id}:.opencode/skills/tool-h"),
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
