use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, Runtime, State};

use crate::git_track::{key_from_branch, read_branch, GitTrackStore};
use crate::idle::system_idle_secs;
use crate::tray::now_secs;

const POLL_SECS: i64 = 10;
const SLEEP_GAP_SECS: i64 = 60;
const IDLE_LIMIT_SECS: f64 = 60.0;
const RETENTION_SECS: i64 = 14 * 86_400;
const MERGE_GAP_SECS: i64 = 300;
const MAX_INTERVAL_SECS: i64 = 8 * 3_600;
const MIN_SHOWN_SECS: i64 = 600;
const MAX_START_AGE_SECS: i64 = 29 * 86_400;
const FLUSH_SECS: i64 = 60;
const FILE_NAME: &str = "activity.json";

const ALLOWLIST: &[&str] = &[
  "com.apple.dt.xcode",
  "com.microsoft.vscode",
  "com.microsoft.vscodeinsiders",
  "com.todesktop.230313mzl4w4u92",
  "com.jetbrains.intellij",
  "com.jetbrains.intellij.ce",
  "com.jetbrains.webstorm",
  "com.jetbrains.pycharm",
  "com.jetbrains.pycharm.ce",
  "com.jetbrains.goland",
  "com.jetbrains.rustrover",
  "com.jetbrains.phpstorm",
  "com.jetbrains.clion",
  "com.jetbrains.rider",
  "com.jetbrains.datagrip",
  "com.apple.terminal",
  "com.googlecode.iterm2",
  "dev.warp.warp-stable",
  "com.mitchellh.ghostty",
  "net.kovidgoyal.kitty",
  "com.github.wez.wezterm",
  "org.alacritty",
  "dev.zed.zed",
  "code.exe",
  "cursor.exe",
  "devenv.exe",
  "idea64.exe",
  "pycharm64.exe",
  "webstorm64.exe",
  "goland64.exe",
  "rustrover64.exe",
  "phpstorm64.exe",
  "clion64.exe",
  "rider64.exe",
  "windowsterminal.exe",
  "wezterm-gui.exe",
  "alacritty.exe",
  "zed.exe",
];

fn id_string<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
  match serde_json::Value::deserialize(d)? {
    serde_json::Value::String(s) => Ok(s),
    serde_json::Value::Number(n) => Ok(n.to_string()),
    other => Err(serde::de::Error::custom(format!("expected string or number, got {other}"))),
  }
}

pub fn is_allowed_app(id: &str) -> bool {
  let id = id.to_ascii_lowercase();
  ALLOWLIST.contains(&id.as_str())
}

#[derive(Clone, Copy, Debug)]
pub struct SampleInputs<'a> {
  pub enabled: bool,
  pub idle_secs: f64,
  pub now: i64,
  pub last_poll: Option<i64>,
  pub timer_running: bool,
  pub app_allowed: bool,
  pub keys: &'a [String],
}

pub fn decide_sample(i: SampleInputs) -> Option<String> {
  if !i.enabled || i.timer_running || !i.app_allowed || i.idle_secs >= IDLE_LIMIT_SECS {
    return None;
  }
  if let Some(previous) = i.last_poll {
    if i.now - previous > POLL_SECS + SLEEP_GAP_SECS {
      return None;
    }
  }
  let distinct: BTreeSet<&String> = i.keys.iter().collect();
  match distinct.len() {
    1 => distinct.into_iter().next().cloned(),
    _ => None,
  }
}

type BucketKey = (String, String, i64, String);

#[derive(Default)]
pub struct ActivityState {
  enabled: bool,
  timer_running: bool,
  user_id: String,
  workspace_id: String,
  buckets: BTreeMap<BucketKey, u32>,
  dirty: bool,
}

impl ActivityState {
  pub fn new() -> Self {
    Self { timer_running: true, ..Self::default() }
  }

  pub fn update(&mut self, enabled: bool, user_id: String, workspace_id: String, timer_running: bool) {
    self.enabled = enabled;
    self.user_id = user_id;
    self.workspace_id = workspace_id;
    self.timer_running = timer_running;
    if !enabled {
      self.buckets.clear();
      self.dirty = false;
    }
  }

  pub fn add_sample(&mut self, now: i64, key: String) {
    let minute = now.div_euclid(60);
    let slot = self.buckets.entry((self.user_id.clone(), self.workspace_id.clone(), minute, key)).or_insert(0);
    *slot = (*slot + POLL_SECS as u32).min(60);
    self.dirty = true;
  }

  pub fn prune(&mut self, now: i64) {
    let before = self.buckets.len();
    self.buckets.retain(|(_, _, minute, _), _| minute * 60 >= now - RETENTION_SECS);
    if self.buckets.len() != before {
      self.dirty = true;
    }
  }

  pub fn consume(&mut self, user: &str, workspace: &str, key: &str, started: i64, ended: i64) {
    let before = self.buckets.len();
    self.buckets.retain(|(u, w, minute, k), _| {
      !(u == user && w == workspace && k == key && minute * 60 >= started && minute * 60 + 60 <= ended)
    });
    if self.buckets.len() != before {
      self.dirty = true;
    }
  }

  pub fn suggestions(&self, user: &str, workspace: &str, now: i64, day_of: impl Fn(i64) -> i64) -> Vec<Suggestion> {
    let mut by_key: BTreeMap<&str, Vec<(i64, u32)>> = BTreeMap::new();
    for ((u, w, minute, key), seconds) in &self.buckets {
      if u == user && w == workspace {
        by_key.entry(key).or_default().push((*minute, *seconds));
      }
    }
    let mut out = Vec::new();
    for (key, minutes) in by_key {
      let mut current: Option<(i64, i64, i64)> = None;
      let finish = |c: Option<(i64, i64, i64)>, out: &mut Vec<Suggestion>| {
        if let Some((start, end, secs)) = c {
          if secs >= MIN_SHOWN_SECS && start >= now - MAX_START_AGE_SECS {
            let seconds = secs.min(end - start);
            out.push(Suggestion { id: suggestion_id(user, workspace, key, start, end), key: key.to_string(), started_at: start, ended_at: end, seconds });
          }
        }
      };
      for (minute, seconds) in minutes {
        let (b_start, b_end) = (minute * 60, minute * 60 + 60);
        current = match current {
          Some((start, end, secs))
            if b_start - end <= MERGE_GAP_SECS
              && b_end - start <= MAX_INTERVAL_SECS
              && day_of(start) == day_of(b_start) =>
          {
            Some((start, b_end, secs + i64::from(seconds)))
          }
          other => {
            finish(other, &mut out);
            Some((b_start, b_end, i64::from(seconds)))
          }
        };
      }
      finish(current, &mut out);
    }
    out.sort_by_key(|s| (s.started_at, s.key.clone()));
    out
  }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Suggestion {
  pub id: String,
  pub key: String,
  pub started_at: i64,
  pub ended_at: i64,
  pub seconds: i64,
}

fn suggestion_id(user: &str, workspace: &str, key: &str, started: i64, ended: i64) -> String {
  let mut hasher = Sha256::new();
  for part in [user, workspace, key, &started.to_string(), &ended.to_string()] {
    hasher.update(part.as_bytes());
    hasher.update([0u8]);
  }
  let digest = hasher.finalize();
  let hex: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
  format!("act-{hex}")
}

fn local_day(ts: i64) -> i64 {
  use chrono::{Datelike, TimeZone};
  chrono::Local
    .timestamp_opt(ts, 0)
    .single()
    .map(|d| i64::from(d.date_naive().num_days_from_ce()))
    .unwrap_or(ts.div_euclid(86_400))
}

#[derive(Serialize, Deserialize)]
struct StoredBucket {
  u: String,
  w: String,
  m: i64,
  k: String,
  s: u32,
}

pub fn save(dir: &Path, state: &ActivityState) -> std::io::Result<()> {
  let rows: Vec<StoredBucket> = state
    .buckets
    .iter()
    .map(|((u, w, m, k), s)| StoredBucket { u: u.clone(), w: w.clone(), m: *m, k: k.clone(), s: *s })
    .collect();
  fs::create_dir_all(dir)?;
  let tmp = dir.join(format!("{FILE_NAME}.tmp"));
  let mut options = fs::OpenOptions::new();
  options.write(true).create(true).truncate(true);
  #[cfg(unix)]
  std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
  let mut file = options.open(&tmp)?;
  file.write_all(&serde_json::to_vec(&rows).map_err(std::io::Error::other)?)?;
  file.sync_all()?;
  fs::rename(&tmp, dir.join(FILE_NAME))
}

pub fn load(dir: &Path, now: i64) -> ActivityState {
  let mut state = ActivityState::new();
  let rows: Vec<StoredBucket> = fs::read(dir.join(FILE_NAME))
    .ok()
    .and_then(|bytes| serde_json::from_slice(&bytes).ok())
    .unwrap_or_default();
  for r in rows {
    state.buckets.insert((r.u, r.w, r.m, r.k), r.s.min(60));
  }
  state.prune(now);
  state
}

fn remove_file(dir: &Path) {
  let _ = fs::remove_file(dir.join(FILE_NAME));
}

pub struct ActivityStore {
  state: Mutex<ActivityState>,
  dir: Mutex<Option<PathBuf>>,
}

impl Default for ActivityStore {
  fn default() -> Self {
    Self { state: Mutex::new(ActivityState::new()), dir: Mutex::new(None) }
  }
}

impl ActivityStore {
  fn dir(&self) -> Option<PathBuf> {
    self.dir.lock().ok().and_then(|d| d.clone())
  }
}

#[cfg(target_os = "macos")]
fn foreground_app() -> Option<String> {
  use objc2_app_kit::NSWorkspace;
  let app = NSWorkspace::sharedWorkspace().frontmostApplication()?;
  Some(app.bundleIdentifier()?.to_string())
}

#[cfg(windows)]
fn foreground_app() -> Option<String> {
  use windows_sys::Win32::Foundation::CloseHandle;
  use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_QUERY_LIMITED_INFORMATION};
  use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
  unsafe {
    let hwnd = GetForegroundWindow();
    if hwnd.is_null() {
      return None;
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, &mut pid);
    let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
    if process.is_null() {
      return None;
    }
    let mut buf = [0u16; 1024];
    let mut len = buf.len() as u32;
    let ok = QueryFullProcessImageNameW(process, 0, buf.as_mut_ptr(), &mut len);
    CloseHandle(process);
    if ok == 0 {
      return None;
    }
    let path = String::from_utf16_lossy(&buf[..len as usize]);
    path.rsplit(['\\', '/']).next().map(str::to_lowercase)
  }
}

#[cfg(not(any(target_os = "macos", windows)))]
fn foreground_app() -> Option<String> {
  None
}

fn foreground_allowed() -> bool {
  foreground_app().is_some_and(|app| is_allowed_app(&app))
}

fn watched_keys(folders: &[PathBuf]) -> Vec<String> {
  folders.iter().filter_map(|f| read_branch(f)).filter_map(|b| key_from_branch(&b)).collect()
}

#[tauri::command]
pub fn activity_supported() -> bool {
  cfg!(any(target_os = "macos", windows))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateArgs {
  enabled: bool,
  #[serde(deserialize_with = "id_string")]
  user_id: String,
  #[serde(deserialize_with = "id_string")]
  workspace_id: String,
  timer_running: bool,
}

#[tauri::command]
pub fn activity_update(store: State<'_, ActivityStore>, args: UpdateArgs) -> Result<(), String> {
  let enabled = args.enabled && activity_supported();
  let mut state = store.state.lock().map_err(|e| e.to_string())?;
  state.update(enabled, args.user_id, args.workspace_id, args.timer_running);
  if !enabled {
    if let Some(dir) = store.dir() {
      remove_file(&dir);
    }
  }
  Ok(())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeArgs {
  #[serde(deserialize_with = "id_string")]
  user_id: String,
  #[serde(deserialize_with = "id_string")]
  workspace_id: String,
}

#[tauri::command]
pub fn activity_suggestions(store: State<'_, ActivityStore>, args: ScopeArgs) -> Result<Vec<Suggestion>, String> {
  let state = store.state.lock().map_err(|e| e.to_string())?;
  Ok(state.suggestions(&args.user_id, &args.workspace_id, now_secs(), local_day))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConsumeArgs {
  #[serde(deserialize_with = "id_string")]
  user_id: String,
  #[serde(deserialize_with = "id_string")]
  workspace_id: String,
  key: String,
  started_at: i64,
  ended_at: i64,
}

#[tauri::command]
pub fn activity_consume(store: State<'_, ActivityStore>, args: ConsumeArgs) -> Result<(), String> {
  let mut state = store.state.lock().map_err(|e| e.to_string())?;
  state.consume(&args.user_id, &args.workspace_id, &args.key, args.started_at, args.ended_at);
  if let Some(dir) = store.dir() {
    save(&dir, &state).map_err(|e| e.to_string())?;
    state.dirty = false;
  }
  Ok(())
}

#[tauri::command]
pub fn activity_forget(store: State<'_, ActivityStore>) -> Result<(), String> {
  let mut state = store.state.lock().map_err(|e| e.to_string())?;
  state.buckets.clear();
  state.dirty = false;
  if let Some(dir) = store.dir() {
    remove_file(&dir);
  }
  Ok(())
}

pub fn start<R: Runtime>(app: &AppHandle<R>) {
  if !activity_supported() {
    return;
  }
  let dir = app.path().app_data_dir().ok().map(|d| d.join("activity"));
  let store = app.state::<ActivityStore>();
  if let Some(dir) = &dir {
    *store.state.lock().unwrap_or_else(|e| e.into_inner()) = load(dir, now_secs());
  }
  *store.dir.lock().unwrap_or_else(|e| e.into_inner()) = dir;
  let app = app.clone();
  std::thread::spawn(move || {
    let mut last_poll: Option<i64> = None;
    let mut last_flush = now_secs();
    loop {
      std::thread::sleep(Duration::from_secs(POLL_SECS as u64));
      let now = now_secs();
      let store = app.state::<ActivityStore>();
      let (enabled, timer_running) = {
        let s = store.state.lock().unwrap_or_else(|e| e.into_inner());
        (s.enabled, s.timer_running)
      };
      let previous = last_poll.replace(now);
      if !enabled {
        continue;
      }
      let folders = app.state::<GitTrackStore>().folders.lock().map(|l| l.clone()).unwrap_or_default();
      let keys = watched_keys(&folders);
      let sample = decide_sample(SampleInputs {
        enabled,
        idle_secs: system_idle_secs(),
        now,
        last_poll: previous,
        timer_running,
        app_allowed: foreground_allowed(),
        keys: &keys,
      });
      let mut s = store.state.lock().unwrap_or_else(|e| e.into_inner());
      if !s.enabled {
        continue;
      }
      if let Some(key) = sample {
        s.add_sample(now, key);
      }
      s.prune(now);
      if s.dirty && now - last_flush >= FLUSH_SECS {
        last_flush = now;
        if let Some(dir) = store.dir() {
          match save(&dir, &s) {
            Ok(()) => s.dirty = false,
            Err(_) => log::warn!("[activity] save failed"),
          }
        }
        log::info!("[activity] {} buckets", s.buckets.len());
      }
    }
  });
}

#[cfg(test)]
mod tests {
  use super::*;

  fn k(keys: &[&str]) -> Vec<String> {
    keys.iter().map(|s| s.to_string()).collect()
  }

  fn inputs<'a>(keys: &'a [String]) -> SampleInputs<'a> {
    SampleInputs { enabled: true, idle_secs: 1.0, now: 1_000, last_poll: Some(990), timer_running: false, app_allowed: true, keys }
  }

  fn day(ts: i64) -> i64 {
    ts.div_euclid(86_400)
  }

  fn state() -> ActivityState {
    let mut s = ActivityState::new();
    s.update(true, "u1".into(), "w1".into(), false);
    s
  }

  fn fill(s: &mut ActivityState, start_minute: i64, minutes: i64, key: &str) {
    for m in start_minute..start_minute + minutes {
      for i in 0..6 {
        s.add_sample(m * 60 + i * 10, key.to_string());
      }
    }
  }

  const NOW: i64 = 1_800_000_000;

  #[test]
  fn allowlist_matches_both_platforms_and_rejects_the_rest() {
    for ok in ["com.apple.dt.Xcode", "com.microsoft.VSCode", "com.jetbrains.WebStorm", "dev.warp.Warp-Stable", "dev.zed.Zed", "Code.exe", "WindowsTerminal.exe", "RIDER64.EXE"] {
      assert!(is_allowed_app(ok), "{ok}");
    }
    for no in ["com.google.Chrome", "com.apple.Safari", "com.jetbrains.fleet", "com.jetbrains.toolbox", "chrome.exe", "msedge.exe", "notepad.exe", "", "com.jetbrains."] {
      assert!(!is_allowed_app(no), "{no}");
    }
  }

  #[test]
  fn sampler_counts_only_when_every_condition_holds() {
    let one = k(&["TM-1"]);
    assert_eq!(decide_sample(inputs(&one)).as_deref(), Some("TM-1"));
    assert_eq!(decide_sample(SampleInputs { last_poll: None, ..inputs(&one) }).as_deref(), Some("TM-1"));
    assert_eq!(decide_sample(inputs(&k(&["TM-1", "TM-1"]))).as_deref(), Some("TM-1"));
    assert_eq!(decide_sample(SampleInputs { enabled: false, ..inputs(&one) }), None);
    assert_eq!(decide_sample(SampleInputs { idle_secs: 60.0, ..inputs(&one) }), None);
    assert_eq!(decide_sample(SampleInputs { timer_running: true, ..inputs(&one) }), None);
    assert_eq!(decide_sample(SampleInputs { app_allowed: false, ..inputs(&one) }), None);
    assert_eq!(decide_sample(SampleInputs { last_poll: Some(1_000 - 71), ..inputs(&one) }), None);
    assert_eq!(decide_sample(SampleInputs { last_poll: Some(1_000 - 70), ..inputs(&one) }).as_deref(), Some("TM-1"));
    assert_eq!(decide_sample(inputs(&k(&[]))), None);
    assert_eq!(decide_sample(inputs(&k(&["TM-1", "TM-2"]))), None);
  }

  #[test]
  fn starts_disabled_and_treats_timer_as_running() {
    let s = ActivityState::new();
    assert!(!s.enabled);
    assert!(s.timer_running);
    assert!(!ActivityStore::default().state.lock().unwrap().enabled);
  }

  #[test]
  fn disabling_drops_buckets() {
    let mut s = state();
    fill(&mut s, 100, 5, "TM-1");
    assert!(!s.buckets.is_empty());
    s.update(false, "u1".into(), "w1".into(), false);
    assert!(s.buckets.is_empty());
  }

  #[test]
  fn a_sample_is_a_fixed_ten_seconds_and_a_minute_caps_at_sixty() {
    let mut s = state();
    for _ in 0..9 {
      s.add_sample(600, "TM-1".into());
    }
    assert_eq!(s.buckets.values().copied().collect::<Vec<_>>(), vec![60]);
  }

  #[test]
  fn consecutive_minutes_merge_and_short_runs_are_hidden() {
    let mut s = state();
    let base = NOW / 60 - 1_000;
    fill(&mut s, base, 10, "TM-1");
    fill(&mut s, base + 200, 9, "TM-2");
    let got = s.suggestions("u1", "w1", NOW, day);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].key, "TM-1");
    assert_eq!(got[0].started_at, base * 60);
    assert_eq!(got[0].ended_at, (base + 10) * 60);
    assert_eq!(got[0].seconds, 600);
    assert!(got[0].id.starts_with("act-") && got[0].id.len() == 20);
  }

  #[test]
  fn gaps_up_to_five_minutes_merge_and_longer_split() {
    let base = NOW / 60 - 1_000;
    let mut s = state();
    fill(&mut s, base, 6, "TM-1");
    fill(&mut s, base + 6 + 5, 6, "TM-1");
    let got = s.suggestions("u1", "w1", NOW, day);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].seconds, 720);
    assert!(got[0].seconds <= got[0].ended_at - got[0].started_at);

    let mut s = state();
    fill(&mut s, base, 10, "TM-1");
    fill(&mut s, base + 10 + 6, 10, "TM-1");
    assert_eq!(s.suggestions("u1", "w1", NOW, day).len(), 2);
  }

  #[test]
  fn intervals_split_at_the_day_boundary_and_at_eight_hours() {
    let midnight_minute = (NOW / 86_400) * 1_440;
    let mut s = state();
    fill(&mut s, midnight_minute - 10, 20, "TM-1");
    let got = s.suggestions("u1", "w1", NOW, day);
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].ended_at, midnight_minute * 60);
    assert_eq!(got[1].started_at, midnight_minute * 60);

    let mut s = state();
    let base = NOW / 60 - 3_000;
    fill(&mut s, base, 9 * 60, "TM-1");
    let flat = |_: i64| 0;
    for g in s.suggestions("u1", "w1", NOW, flat) {
      assert!(g.ended_at - g.started_at <= MAX_INTERVAL_SECS);
    }
    assert_eq!(s.suggestions("u1", "w1", NOW, flat).len(), 2);
  }

  #[test]
  fn old_intervals_and_other_scopes_are_hidden() {
    let mut s = state();
    fill(&mut s, (NOW - 30 * 86_400) / 60, 12, "TM-1");
    assert!(s.suggestions("u1", "w1", NOW, day).is_empty());
    let mut s = state();
    fill(&mut s, NOW / 60 - 100, 12, "TM-1");
    assert_eq!(s.suggestions("u1", "w1", NOW, day).len(), 1);
    assert!(s.suggestions("u2", "w1", NOW, day).is_empty());
    assert!(s.suggestions("u1", "w2", NOW, day).is_empty());
  }

  #[test]
  fn ids_are_stable_and_depend_on_every_field() {
    let a = suggestion_id("u", "w", "TM-1", 10, 20);
    assert_eq!(a, suggestion_id("u", "w", "TM-1", 10, 20));
    for other in [
      suggestion_id("x", "w", "TM-1", 10, 20),
      suggestion_id("u", "x", "TM-1", 10, 20),
      suggestion_id("u", "w", "TM-2", 10, 20),
      suggestion_id("u", "w", "TM-1", 11, 20),
      suggestion_id("u", "w", "TM-1", 10, 21),
    ] {
      assert_ne!(a, other);
    }
  }

  #[test]
  fn consume_removes_only_that_key_inside_the_interval() {
    let base = NOW / 60 - 500;
    let mut s = state();
    fill(&mut s, base, 12, "TM-1");
    fill(&mut s, base, 12, "TM-2");
    s.consume("u1", "w1", "TM-1", base * 60, (base + 12) * 60);
    let got = s.suggestions("u1", "w1", NOW, day);
    assert_eq!(got.iter().map(|g| g.key.as_str()).collect::<Vec<_>>(), vec!["TM-2"]);

    let mut s = state();
    fill(&mut s, base, 12, "TM-1");
    s.consume("u1", "w1", "TM-1", base * 60, (base + 6) * 60);
    assert_eq!(s.buckets.len(), 6);
    s.consume("u2", "w1", "TM-1", 0, i64::MAX);
    assert_eq!(s.buckets.len(), 6);
  }

  #[test]
  fn retention_prunes_on_load_and_round_trip_keeps_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = state();
    fill(&mut s, (NOW - 15 * 86_400) / 60, 3, "TM-1");
    fill(&mut s, NOW / 60 - 50, 3, "TM-2");
    save(dir.path(), &s).unwrap();
    let loaded = load(dir.path(), NOW);
    assert_eq!(loaded.buckets.len(), 3);
    assert!(loaded.buckets.keys().all(|(_, _, _, key)| key == "TM-2"));
    assert!(!loaded.enabled);
    assert!(loaded.timer_running);
    assert!(!dir.path().join("activity.json.tmp").exists());
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      let mode = fs::metadata(dir.path().join("activity.json")).unwrap().permissions().mode();
      assert_eq!(mode & 0o777, 0o600);
    }
  }

  #[test]
  fn stored_file_holds_only_ids_minutes_keys_and_seconds() {
    let dir = tempfile::tempdir().unwrap();
    let mut s = state();
    fill(&mut s, NOW / 60, 1, "TM-1");
    save(dir.path(), &s).unwrap();
    let text = fs::read_to_string(dir.path().join("activity.json")).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    let mut fields: Vec<&str> = v[0].as_object().unwrap().keys().map(String::as_str).collect();
    fields.sort();
    assert_eq!(fields, vec!["k", "m", "s", "u", "w"]);
  }

  #[test]
  fn corrupt_file_loads_as_empty() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("activity.json"), "not json").unwrap();
    assert!(load(dir.path(), NOW).buckets.is_empty());
  }

  #[test]
  fn source_never_reads_window_titles() {
    let src = include_str!("activity.rs");
    for needle in [concat!("GetWindow", "Text"), concat!("kCGWindow", "Name"), concat!("CGWindow", "List")] {
      assert_eq!(src.matches(needle).count(), 0, "{needle}");
    }
  }

  #[test]
  fn ids_accept_strings_and_numbers() {
    let a: ScopeArgs = serde_json::from_str(r#"{"userId":7,"workspaceId":"w"}"#).unwrap();
    assert_eq!((a.user_id.as_str(), a.workspace_id.as_str()), ("7", "w"));
  }
}
