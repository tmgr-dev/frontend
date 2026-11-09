use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};
use tauri_plugin_dialog::DialogExt;

const NOT_A_TASK_KEY: [&str; 18] = [
  "release", "hotfix", "bugfix", "feature", "feat", "fix", "chore", "docs", "rc", "v", "version",
  "sprint", "week", "epoch", "step", "phase", "stage", "node",
];
const POLL_SECS: u64 = 3;
const MAX_FOLDERS: usize = 20;
const MAX_PATH_LEN: usize = 1024;
const HEAD_MAX_BYTES: u64 = 512;
const GITFILE_MAX_BYTES: u64 = 4096;
const EVENT: &str = "git-track://branch";

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BranchEvent {
  pub key: String,
}

#[derive(Default)]
pub struct GitTrackStore {
  pub folders: Mutex<Vec<PathBuf>>,
  ticket: AtomicU64,
  written: Mutex<u64>,
}

#[derive(Default)]
pub struct BranchWatcher {
  folders: HashMap<String, FolderState>,
}

#[derive(Default)]
struct FolderState {
  seen: Option<String>,
  pending: Option<String>,
  emitted: Option<String>,
}

impl BranchWatcher {
  pub fn tick(&mut self, observed: &[(String, Option<String>)]) -> Vec<String> {
    self.folders.retain(|folder, _| observed.iter().any(|(f, _)| f == folder));
    let mut keys = Vec::new();
    for (folder, branch) in observed {
      let state = self.folders.entry(folder.clone()).or_default();
      let Some(branch) = branch else {
        state.pending = None;
        continue;
      };
      if state.seen.is_none() {
        state.seen = Some(branch.clone());
      } else if state.seen.as_ref() == Some(branch) {
        state.pending = None;
      } else if state.pending.as_ref() == Some(branch) {
        state.seen = Some(branch.clone());
        state.pending = None;
        if let Some(key) = key_from_branch(branch) {
          if state.emitted.as_ref() != Some(&key) {
            state.emitted = Some(key.clone());
            keys.push(key);
          }
        }
      } else {
        state.pending = Some(branch.clone());
      }
    }
    keys
  }
}

pub fn key_from_branch(branch: &str) -> Option<String> {
  let last = branch.rsplit('/').next()?;
  let (code, rest) = last.split_once('-')?;
  let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
  let after = &rest[digits.len()..];
  if !(after.is_empty() || after.starts_with('-')) || digits.is_empty() {
    return None;
  }
  if NOT_A_TASK_KEY.contains(&code) || !code.bytes().next()?.is_ascii_alphabetic() {
    return None;
  }
  let key = format!("{}-{}", code.to_ascii_uppercase(), digits);
  parse_key(&key).map(str::to_string)
}

fn parse_key(input: &str) -> Option<&str> {
  let (code, seq) = input.split_once('-')?;
  let mut chars = code.chars();
  let first = chars.next()?;
  let code_ok = first.is_ascii_uppercase()
    && code.len() <= 10
    && code.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
  let seq_ok = (1..=7).contains(&seq.len()) && seq.bytes().all(|b| b.is_ascii_digit());
  (code_ok && seq_ok).then_some(input)
}

pub fn parse_head(content: &str) -> Option<String> {
  let branch = content.lines().next()?.trim().strip_prefix("ref: refs/heads/")?;
  (!branch.is_empty()).then(|| branch.to_string())
}

pub fn head_path(dir: &Path) -> Option<PathBuf> {
  let dot_git = dir.join(".git");
  if dot_git.is_dir() {
    return Some(dot_git.join("HEAD"));
  }
  let content = read_capped(&dot_git, GITFILE_MAX_BYTES)?;
  let target = content.lines().next()?.trim().strip_prefix("gitdir:")?.trim();
  if target.is_empty() {
    return None;
  }
  let gitdir = Path::new(target);
  let gitdir = if gitdir.is_absolute() { gitdir.to_path_buf() } else { dir.join(gitdir) };
  Some(gitdir.join("HEAD"))
}

fn read_capped(path: &Path, max: u64) -> Option<String> {
  if !path.is_file() {
    return None;
  }
  let mut content = String::new();
  fs::File::open(path).ok()?.take(max).read_to_string(&mut content).ok()?;
  Some(content)
}

pub(crate) fn read_branch(folder: &Path) -> Option<String> {
  let path = head_path(folder)?;
  parse_head(&read_capped(&path, HEAD_MAX_BYTES)?)
}

fn sanitize(folders: Vec<String>) -> Vec<PathBuf> {
  let mut kept: Vec<PathBuf> = Vec::new();
  for raw in folders {
    if raw.len() > MAX_PATH_LEN || !Path::new(&raw).is_absolute() {
      continue;
    }
    let Ok(canonical) = fs::canonicalize(&raw) else { continue };
    if !canonical.is_dir() || kept.contains(&canonical) {
      continue;
    }
    kept.push(canonical);
    if kept.len() == MAX_FOLDERS {
      break;
    }
  }
  kept
}

#[tauri::command]
pub async fn git_track_update(store: State<'_, GitTrackStore>, folders: Vec<String>) -> Result<(), String> {
  let ticket = store.ticket.fetch_add(1, Ordering::SeqCst) + 1;
  let folders: Vec<String> = folders.into_iter().take(MAX_FOLDERS * 2).collect();
  let kept = tauri::async_runtime::spawn_blocking(move || sanitize(folders)).await.map_err(|e| e.to_string())?;
  let mut written = store.written.lock().map_err(|e| e.to_string())?;
  if ticket > *written {
    *written = ticket;
    *store.folders.lock().map_err(|e| e.to_string())? = kept;
  }
  Ok(())
}

#[tauri::command]
pub async fn git_track_pick_folder<R: Runtime>(app: AppHandle<R>) -> Result<Option<String>, String> {
  let picked = tauri::async_runtime::spawn_blocking(move || {
    app.dialog().file().set_title("Choose a git repository").blocking_pick_folder()
  })
  .await
  .map_err(|e| e.to_string())?;
  let Some(picked) = picked else { return Ok(None) };
  let path = picked.into_path().map_err(|e| e.to_string())?;
  tauri::async_runtime::spawn_blocking(move || {
    let root = fs::canonicalize(&path).map_err(|e| e.to_string())?;
    if !root.join(".git").exists() {
      return Err("Not a git repository".to_string());
    }
    Ok(Some(root.to_string_lossy().into_owned()))
  })
  .await
  .map_err(|e| e.to_string())?
}

pub fn start<R: Runtime>(app: &AppHandle<R>) {
  let app = app.clone();
  std::thread::spawn(move || {
    let mut watcher = BranchWatcher::default();
    loop {
      std::thread::sleep(Duration::from_secs(POLL_SECS));
      let folders = app.state::<GitTrackStore>().folders.lock().map(|list| list.clone()).unwrap_or_default();
      let observed: Vec<(String, Option<String>)> = folders
        .iter()
        .map(|folder| (folder.to_string_lossy().into_owned(), read_branch(folder)))
        .collect();
      for key in watcher.tick(&observed) {
        log::info!("[git-track] branch changed");
        let _ = app.emit_to("main", EVENT, BranchEvent { key });
      }
    }
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  fn poll(watcher: &mut BranchWatcher, folder: &str, branch: Option<&str>) -> Vec<String> {
    watcher.tick(&[(folder.to_string(), branch.map(str::to_string))])
  }

  #[test]
  fn parse_head_reads_branch_and_ignores_detached() {
    assert_eq!(parse_head("ref: refs/heads/main\n").as_deref(), Some("main"));
    assert_eq!(parse_head("ref: refs/heads/feat/tm-5-x\r\n").as_deref(), Some("feat/tm-5-x"));
    assert_eq!(parse_head("3f2a9c1d0e\n"), None);
    assert_eq!(parse_head("ref: refs/tags/v1\n"), None);
    assert_eq!(parse_head(""), None);
  }

  #[test]
  fn head_path_follows_plain_repo_and_worktree_gitfile() {
    let root = tempfile::tempdir().unwrap();
    let main = root.path().join("main");
    fs::create_dir_all(main.join(".git/worktrees/wt")).unwrap();
    fs::write(main.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    fs::write(main.join(".git/worktrees/wt/HEAD"), "ref: refs/heads/tm-9-wt\n").unwrap();
    let wt = root.path().join("wt");
    fs::create_dir_all(&wt).unwrap();
    fs::write(wt.join(".git"), "gitdir: ../main/.git/worktrees/wt\n").unwrap();

    assert_eq!(head_path(&main), Some(main.join(".git/HEAD")));
    assert_eq!(head_path(&wt), Some(wt.join("../main/.git/worktrees/wt/HEAD")));
    assert_eq!(read_branch(&main).as_deref(), Some("main"));
    assert_eq!(read_branch(&wt).as_deref(), Some("tm-9-wt"));
    assert_eq!(head_path(&root.path().join("plain")), None);
  }

  #[test]
  fn sanitize_keeps_existing_unique_absolute_folders_up_to_the_limit() {
    let root = tempfile::tempdir().unwrap();
    let a = root.path().join("a");
    fs::create_dir_all(&a).unwrap();
    let file = root.path().join("file");
    fs::write(&file, "x").unwrap();
    let mut input = vec![
      "relative/path".to_string(),
      root.path().join("missing").to_string_lossy().into_owned(),
      file.to_string_lossy().into_owned(),
      a.to_string_lossy().into_owned(),
      format!("{}/./a", root.path().display()),
    ];
    input.extend((0..30).map(|i| {
      let dir = root.path().join(format!("d{i}"));
      fs::create_dir_all(&dir).unwrap();
      dir.to_string_lossy().into_owned()
    }));
    let kept = sanitize(input);
    assert_eq!(kept.len(), MAX_FOLDERS);
    assert_eq!(kept[0], fs::canonicalize(&a).unwrap());
    assert_eq!(kept.iter().filter(|p| **p == kept[0]).count(), 1);
  }

  #[test]
  fn keys_come_from_branch_names() {
    assert_eq!(
      key_from_branch("tm-509-repo-prep").as_deref(),
      Some("TM-509")
    );
    assert_eq!(key_from_branch("tm-509").as_deref(), Some("TM-509"));
    assert_eq!(key_from_branch("feat/tm-12-x").as_deref(), Some("TM-12"));
    assert_eq!(key_from_branch("ab2-7-z").as_deref(), Some("AB2-7"));
    for none in [
      "main",
      "master",
      "release-2",
      "feature-12",
      "tm-x",
      "tm-509x",
      "tm-12345678",
      "1-2",
      "HEAD",
      "",
    ] {
      assert_eq!(key_from_branch(none), None, "{none}");
    }
  }

  #[test]
  fn first_observation_records_without_event() {
    let mut watcher = BranchWatcher::default();
    assert!(poll(&mut watcher, "/repo", Some("tm-1")).is_empty());
    assert!(poll(&mut watcher, "/repo", Some("tm-1")).is_empty());
  }

  #[test]
  fn a_change_needs_two_identical_polls() {
    let mut watcher = BranchWatcher::default();
    poll(&mut watcher, "/repo", Some("main"));
    assert!(poll(&mut watcher, "/repo", Some("tm-1")).is_empty());
    assert_eq!(poll(&mut watcher, "/repo", Some("tm-1")), vec!["TM-1"]);
    assert!(poll(&mut watcher, "/repo", Some("tm-1")).is_empty());
  }

  #[test]
  fn quick_back_and_forth_emits_nothing() {
    let mut watcher = BranchWatcher::default();
    poll(&mut watcher, "/repo", Some("main"));
    assert!(poll(&mut watcher, "/repo", Some("tm-1")).is_empty());
    assert!(poll(&mut watcher, "/repo", Some("main")).is_empty());
    assert!(poll(&mut watcher, "/repo", Some("main")).is_empty());
  }

  #[test]
  fn branch_detached_and_back_to_same_branch_emits_nothing() {
    let mut watcher = BranchWatcher::default();
    poll(&mut watcher, "/repo", Some("tm-1"));
    assert!(poll(&mut watcher, "/repo", None).is_empty());
    assert!(poll(&mut watcher, "/repo", Some("tm-1")).is_empty());
    assert!(poll(&mut watcher, "/repo", Some("tm-1")).is_empty());
  }

  #[test]
  fn folder_missing_then_back_on_same_branch_emits_nothing() {
    let mut watcher = BranchWatcher::default();
    poll(&mut watcher, "/repo", Some("tm-1"));
    assert!(poll(&mut watcher, "/repo", None).is_empty());
    assert!(poll(&mut watcher, "/repo", Some("tm-1")).is_empty());
    assert!(poll(&mut watcher, "/repo", Some("tm-1")).is_empty());
  }

  #[test]
  fn branch_without_key_emits_nothing() {
    let mut watcher = BranchWatcher::default();
    poll(&mut watcher, "/repo", Some("main"));
    assert!(poll(&mut watcher, "/repo", Some("release-2")).is_empty());
    assert!(poll(&mut watcher, "/repo", Some("release-2")).is_empty());
  }

  #[test]
  fn new_key_emits_once_per_folder() {
    let mut watcher = BranchWatcher::default();
    poll(&mut watcher, "/repo", Some("main"));
    assert!(poll(&mut watcher, "/repo", Some("tm-7")).is_empty());
    assert_eq!(poll(&mut watcher, "/repo", Some("tm-7")), vec!["TM-7"]);
    assert!(poll(&mut watcher, "/repo", Some("tm-7")).is_empty());
    assert!(poll(&mut watcher, "/repo", Some("tm-7-fix")).is_empty());
    assert!(poll(&mut watcher, "/repo", Some("tm-7-fix")).is_empty());
  }

  #[test]
  fn folders_are_tracked_independently() {
    let mut watcher = BranchWatcher::default();
    let both = |a: &str, b: &str| {
      vec![("/a".to_string(), Some(a.to_string())), ("/b".to_string(), Some(b.to_string()))]
    };
    watcher.tick(&both("main", "main"));
    watcher.tick(&both("tm-1", "main"));
    assert_eq!(watcher.tick(&both("tm-1", "main")), vec!["TM-1"]);
    watcher.tick(&both("tm-1", "tm-2"));
    assert_eq!(watcher.tick(&both("tm-1", "tm-2")), vec!["TM-2"]);
  }
}
