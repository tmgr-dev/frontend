use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime, State};
use tauri_plugin_opener::OpenerExt;

use crate::local_db::{with_connection, LocalDbs};
use crate::local_workspaces::{self, civil_from_days, LocalWorkspace};

const DAY: i64 = 86_400;
const AUTO_EVERY: i64 = DAY;
const CHECK_EVERY: Duration = Duration::from_secs(3600);
const KEEP_DAYS: usize = 7;
const KEEP_WEEKS: usize = 4;
const KEEP_MONTHS: usize = 3;
const FUTURE_SLACK: i64 = DAY;
const STALE_TMP: i64 = 3600;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BackupInfo {
  pub file: String,
  pub path: String,
  pub size: u64,
  pub created_at: String,
  pub kind: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct BackupStatus {
  pub code: String,
  pub name: String,
  pub backups_dir: String,
  pub last_backup_at: Option<String>,
  pub last_error: Option<String>,
}

#[derive(Default)]
pub struct BackupErrors(Mutex<HashMap<String, String>>);

impl BackupErrors {
  fn set(&self, code: &str, error: Option<String>) {
    if let Ok(mut map) = self.0.lock() {
      match error {
        Some(e) => map.insert(code.to_string(), e),
        None => map.remove(code),
      };
    }
  }

  fn get(&self, code: &str) -> Option<String> {
    self.0.lock().ok()?.get(code).cloned()
  }
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
  let y = if m <= 2 { y - 1 } else { y };
  let era = y.div_euclid(400);
  let yoe = y.rem_euclid(400);
  let mp = (m as i64 + 9) % 12;
  let doy = (153 * mp + 2) / 5 + d as i64 - 1;
  let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
  era * 146_097 + doe - 719_468
}

fn stamp(secs: i64) -> String {
  let (y, m, d) = civil_from_days(secs.div_euclid(DAY));
  let rem = secs.rem_euclid(DAY);
  format!("{y:04}{m:02}{d:02}-{:02}{:02}{:02}", rem / 3600, (rem % 3600) / 60, rem % 60)
}

fn rfc3339(secs: i64) -> String {
  let (y, m, d) = civil_from_days(secs.div_euclid(DAY));
  let rem = secs.rem_euclid(DAY);
  format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z", rem / 3600, (rem % 3600) / 60, rem % 60)
}

fn parse_stamp(s: &str) -> Option<i64> {
  let b = s.as_bytes();
  if b.len() != 15 || b[8] != b'-' || !b.iter().enumerate().all(|(i, c)| i == 8 || c.is_ascii_digit()) {
    return None;
  }
  let n = |r: std::ops::Range<usize>| s[r].parse::<i64>().ok();
  let (y, mo, d, h, mi, se) = (n(0..4)?, n(4..6)?, n(6..8)?, n(9..11)?, n(11..13)?, n(13..15)?);
  if !(1..=12).contains(&mo) || !(1..=31).contains(&d) || h > 23 || mi > 59 || se > 59 {
    return None;
  }
  let secs = days_from_civil(y, mo as u32, d as u32) * DAY + h * 3600 + mi * 60 + se;
  (stamp(secs) == s).then_some(secs)
}

fn exact_time(name: &str, prefix: &str) -> Option<i64> {
  parse_stamp(name.strip_prefix(prefix)?.strip_suffix(".db")?)
}

pub fn auto_time(name: &str) -> Option<i64> {
  exact_time(name, "auto-")
}

fn migration_time(name: &str) -> Option<i64> {
  let rest = name.strip_prefix("workspace-v")?.strip_suffix(".db")?;
  let (version, secs) = rest.split_once('-')?;
  version.parse::<u32>().ok()?;
  secs.parse().ok()
}

fn month_key(secs: i64) -> (i64, u32) {
  let (y, m, _) = civil_from_days(secs.div_euclid(DAY));
  (y, m)
}

fn week_key(secs: i64) -> i64 {
  (secs.div_euclid(DAY) + 3).div_euclid(7)
}

/// Names of automatic backups to delete. Keeps the newest file of each of the latest 7 days, 4 weeks
/// and 3 months that have backups; only exact `auto-YYYYMMDD-HHMMSS.db` names are ever considered,
/// and files dated in the future (a clock that jumped) are left alone.
pub fn files_to_delete(files: &[(String, i64)], now: i64) -> Vec<String> {
  let mut autos: Vec<(&String, i64)> = files
    .iter()
    .filter(|(name, secs)| auto_time(name) == Some(*secs) && *secs <= now + FUTURE_SLACK)
    .map(|(name, secs)| (name, *secs))
    .collect();
  autos.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| b.0.cmp(a.0)));

  let mut keep: HashSet<&String> = HashSet::new();
  if let Some((name, _)) = autos.first() {
    keep.insert(name);
  }
  fn newest_per<'a, K: std::hash::Hash + Eq>(
    autos: &[(&'a String, i64)],
    limit: usize,
    key: impl Fn(i64) -> K,
    keep: &mut HashSet<&'a String>,
  ) {
    let mut seen = HashSet::new();
    for (name, secs) in autos {
      if seen.len() == limit {
        break;
      }
      if seen.insert(key(*secs)) {
        keep.insert(name);
      }
    }
  }
  newest_per(&autos, KEEP_DAYS, |s| s.div_euclid(DAY), &mut keep);
  newest_per(&autos, KEEP_WEEKS, week_key, &mut keep);
  newest_per(&autos, KEEP_MONTHS, month_key, &mut keep);

  autos
    .iter()
    .filter(|(name, _)| !keep.contains(name))
    .map(|(name, _)| (*name).clone())
    .collect()
}

fn backups_dir(workspace: &LocalWorkspace) -> PathBuf {
  Path::new(&workspace.path).join("backups")
}

fn verify(path: &Path) -> Result<(), String> {
  let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| e.to_string())?;
  let result: String = conn
    .query_row("PRAGMA integrity_check", [], |row| row.get(0))
    .map_err(|e| e.to_string())?;
  if result == "ok" {
    Ok(())
  } else {
    Err(format!("integrity check failed: {result}"))
  }
}

fn info(dir: &Path, file: &str, kind: &str, secs: i64) -> BackupInfo {
  let path = dir.join(file);
  BackupInfo {
    file: file.to_string(),
    size: fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
    path: path.to_string_lossy().into_owned(),
    created_at: rfc3339(secs),
    kind: kind.to_string(),
  }
}

fn backup_names(kind: &str, secs: i64) -> String {
  format!("{kind}-{}.db", stamp(secs))
}

/// The only step that needs the workspace connection.
fn snapshot(conn: &Connection, dir: &Path, kind: &str, secs: i64) -> Result<PathBuf, String> {
  fs::create_dir_all(dir).map_err(|e| e.to_string())?;
  let tmp = dir.join(format!(".{}.tmp", backup_names(kind, secs)));
  let _ = fs::remove_file(&tmp);
  match conn.execute("VACUUM INTO ?", [tmp.to_string_lossy().as_ref()]) {
    Ok(_) => Ok(tmp),
    Err(e) => {
      let _ = fs::remove_file(&tmp);
      Err(e.to_string())
    }
  }
}

/// Verifies the snapshot and only then gives it its final name.
fn finalize(tmp: &Path, dir: &Path, kind: &str, secs: i64) -> Result<BackupInfo, String> {
  let file = backup_names(kind, secs);
  let result = verify(tmp).and_then(|_| fs::rename(tmp, dir.join(&file)).map_err(|e| e.to_string()));
  if let Err(error) = result {
    let _ = fs::remove_file(tmp);
    return Err(error);
  }
  Ok(info(dir, &file, kind, secs))
}

fn existing(dir: &Path, kind: &str, secs: i64) -> Option<BackupInfo> {
  let file = backup_names(kind, secs);
  dir.join(&file).exists().then(|| info(dir, &file, kind, secs))
}

#[cfg(test)]
fn write_backup(conn: &Connection, dir: &Path, kind: &str, secs: i64) -> Result<BackupInfo, String> {
  if let Some(found) = existing(dir, kind, secs) {
    return Ok(found);
  }
  finalize(&snapshot(conn, dir, kind, secs)?, dir, kind, secs)
}

fn remove_stale_temp_files(dir: &Path, kind: &str, now: i64) {
  let Ok(entries) = fs::read_dir(dir) else {
    return;
  };
  let prefix = format!(".{kind}-");
  for entry in entries.filter_map(Result::ok) {
    let name = entry.file_name().to_string_lossy().into_owned();
    if !name.starts_with(&prefix) || !name.ends_with(".db.tmp") {
      continue;
    }
    let age = entry
      .metadata()
      .and_then(|m| m.modified())
      .ok()
      .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
      .map(|d| now - d.as_secs() as i64);
    if age.is_some_and(|a| a > STALE_TMP) {
      let _ = fs::remove_file(entry.path());
    }
  }
}

fn list_with_secs(dir: &Path) -> Vec<(i64, BackupInfo)> {
  let Ok(entries) = fs::read_dir(dir) else {
    return Vec::new();
  };
  let mut backups: Vec<(i64, BackupInfo)> = entries
    .filter_map(Result::ok)
    .filter_map(|entry| {
      let file = entry.file_name().to_string_lossy().into_owned();
      if file.starts_with('.') || !file.ends_with(".db") {
        return None;
      }
      let kind = if file.starts_with("auto-") {
        "auto"
      } else if file.starts_with("manual-") {
        "manual"
      } else if file.starts_with("workspace-v") {
        "migration"
      } else {
        return None;
      };
      let from_name = match kind {
        "auto" | "manual" => exact_time(&file, &format!("{kind}-")),
        _ => migration_time(&file),
      };
      let secs = from_name.or_else(|| {
        let modified = entry.metadata().ok()?.modified().ok()?;
        modified.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64)
      })?;
      Some((secs, info(dir, &file, kind, secs)))
    })
    .collect();
  backups.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.file.cmp(&a.1.file)));
  backups
}

pub fn list_in(dir: &Path) -> Vec<BackupInfo> {
  list_with_secs(dir).into_iter().map(|(_, b)| b).collect()
}

fn newest_secs(dir: &Path, now: i64) -> Option<i64> {
  list_with_secs(dir).into_iter().map(|(s, _)| s).find(|s| *s <= now + FUTURE_SLACK)
}

fn newest_auto_secs(dir: &Path, now: i64) -> Option<i64> {
  list_with_secs(dir)
    .into_iter()
    .filter(|(_, b)| b.kind == "auto")
    .map(|(s, _)| s)
    .find(|s| *s <= now + FUTURE_SLACK)
}

pub fn is_due(dir: &Path, now: i64) -> bool {
  newest_auto_secs(dir, now).map_or(true, |newest| now - newest >= AUTO_EVERY)
}

fn rotate(dir: &Path, now: i64) {
  let files: Vec<(String, i64)> = list_in(dir)
    .into_iter()
    .filter(|b| b.kind == "auto")
    .filter_map(|b| auto_time(&b.file).map(|s| (b.file, s)))
    .collect();
  for name in files_to_delete(&files, now) {
    if let Err(error) = fs::remove_file(dir.join(&name)) {
      log::warn!("[local] could not rotate backup {name}: {error}");
    }
  }
}

const NOTHING_TO_BACK_UP: &str = "Nothing to back up yet";

async fn run_backup<R: Runtime>(
  app: &AppHandle<R>,
  code: &str,
  kind: &'static str,
) -> Result<BackupInfo, String> {
  let workspace = local_workspaces::find(app, code)?;
  if !Path::new(&workspace.database).exists() {
    return Err(NOTHING_TO_BACK_UP.into());
  }
  let dir = backups_dir(&workspace);
  let secs = crate::tray::now_secs();
  if let Some(found) = existing(&dir, kind, secs) {
    return Ok(found);
  }
  remove_stale_temp_files(&dir, kind, secs);
  let errors = app.state::<BackupErrors>();
  let dbs = app.state::<LocalDbs>();
  let target = dir.clone();
  let result = match with_connection(app.clone(), &dbs, code.to_string(), move |conn| {
    snapshot(conn, &target, kind, secs)
  })
  .await
  {
    Ok(tmp) => {
      let dir = dir.clone();
      tauri::async_runtime::spawn_blocking(move || finalize(&tmp, &dir, kind, secs))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r)
    }
    Err(error) => Err(error),
  };
  match &result {
    Ok(_) => {
      errors.set(code, None);
      if kind == "auto" {
        rotate(&dir, secs);
      }
      log::info!("[local] {kind} backup of {code} written");
    }
    Err(error) => {
      errors.set(code, Some(error.clone()));
      log::error!("[local] {kind} backup of {code} failed: {error}");
    }
  }
  result
}

async fn backup_due<R: Runtime>(app: &AppHandle<R>) {
  let Ok(root) = local_workspaces::root(app) else {
    return;
  };
  for workspace in local_workspaces::list_in(&root) {
    if !Path::new(&workspace.database).exists() || !is_due(&backups_dir(&workspace), crate::tray::now_secs()) {
      continue;
    }
    let _ = run_backup(app, &workspace.manifest.code, "auto").await;
  }
}

pub fn start<R: Runtime>(app: &AppHandle<R>) {
  let app = app.clone();
  tauri::async_runtime::spawn(async move {
    loop {
      backup_due(&app).await;
      tokio::time::sleep(CHECK_EVERY).await;
    }
  });
}

#[tauri::command]
pub async fn local_backup_now<R: Runtime>(app: AppHandle<R>, code: String) -> Result<BackupInfo, String> {
  run_backup(&app, &code, "manual").await
}

#[tauri::command]
pub fn local_backup_list<R: Runtime>(app: AppHandle<R>, code: String) -> Result<Vec<BackupInfo>, String> {
  let workspace = local_workspaces::find(&app, &code)?;
  Ok(list_in(&backups_dir(&workspace)))
}

#[tauri::command]
pub fn local_backup_status<R: Runtime>(
  app: AppHandle<R>,
  errors: State<'_, BackupErrors>,
) -> Result<Vec<BackupStatus>, String> {
  Ok(
    local_workspaces::list_in(&local_workspaces::root(&app)?)
      .into_iter()
      .map(|w| {
        let dir = backups_dir(&w);
        BackupStatus {
          last_backup_at: newest_secs(&dir, crate::tray::now_secs()).map(rfc3339),
          last_error: errors.get(&w.manifest.code),
          backups_dir: dir.to_string_lossy().into_owned(),
          code: w.manifest.code,
          name: w.manifest.name,
        }
      })
      .collect(),
  )
}

#[tauri::command]
pub fn local_backup_reveal<R: Runtime>(app: AppHandle<R>, code: String) -> Result<(), String> {
  let workspace = local_workspaces::find(&app, &code)?;
  let dir = backups_dir(&workspace);
  fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
  app.opener().reveal_item_in_dir(&dir).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;

  fn at(y: i64, m: u32, d: u32, h: i64) -> i64 {
    days_from_civil(y, m, d) * DAY + h * 3600
  }

  fn auto(secs: i64) -> (String, i64) {
    (format!("auto-{}.db", stamp(secs)), secs)
  }

  fn deleted(files: &[(String, i64)], now: i64) -> Vec<String> {
    let mut d = files_to_delete(files, now);
    d.sort();
    d
  }

  fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("tmgr-backups-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
  }

  #[test]
  fn stamps_round_trip_and_reject_garbage() {
    let secs = at(2026, 10, 11, 13) + 125;
    assert_eq!(stamp(secs), "20261011-130205");
    assert_eq!(parse_stamp("20261011-130205"), Some(secs));
    assert_eq!(rfc3339(secs), "2026-10-11T13:02:05Z");
    assert_eq!(parse_stamp("20261301-130205"), None);
    assert_eq!(parse_stamp("20260231-000000"), None);
    assert_eq!(parse_stamp("2026101-1302055"), None);
    assert_eq!(auto_time("auto-20261011-130205.db.bak"), None);
    assert_eq!(auto_time("manual-20261011-130205.db"), None);
  }

  #[test]
  fn a_single_file_is_kept() {
    let files = vec![auto(at(2026, 10, 1, 3))];
    assert!(deleted(&files, at(2026, 10, 11, 0)).is_empty());
    assert!(deleted(&[], 0).is_empty());
  }

  #[test]
  fn sixty_daily_files_are_thinned_to_days_weeks_and_months() {
    let now = at(2026, 10, 11, 12);
    let files: Vec<_> = (0..60).map(|i| auto(at(2026, 10, 11, 3) - i * DAY)).collect();
    let kept: HashSet<String> = files
      .iter()
      .map(|f| f.0.clone())
      .filter(|n| !files_to_delete(&files, now).contains(n))
      .collect();
    for i in 0..7 {
      assert!(kept.contains(&auto(at(2026, 10, 11, 3) - i * DAY).0));
    }
    assert!(kept.len() >= 7 && kept.len() <= 14, "{}", kept.len());
    let weeks: HashSet<_> = files.iter().filter(|f| kept.contains(&f.0)).map(|f| week_key(f.1)).collect();
    assert!(weeks.len() >= 4);
    assert!(!kept.contains(&auto(at(2026, 10, 11, 3) - 59 * DAY).0));
  }

  #[test]
  fn same_day_duplicates_keep_only_the_newest() {
    let files = vec![auto(at(2026, 10, 11, 1)), auto(at(2026, 10, 11, 9)), auto(at(2026, 10, 11, 5))];
    assert_eq!(deleted(&files, at(2026, 10, 12, 0)), vec![auto(at(2026, 10, 11, 1)).0, auto(at(2026, 10, 11, 5)).0]);
  }

  #[test]
  fn gaps_do_not_shrink_the_window() {
    let files: Vec<_> = (0..10).map(|i| auto(at(2026, 6, 1, 3) + i * 3 * DAY)).collect();
    let del = files_to_delete(&files, at(2026, 10, 11, 0));
    assert!(!del.contains(&files[9].0));
    assert!(!del.contains(&files[3].0));
    assert!(del.contains(&files[0].0));
  }

  #[test]
  fn the_newest_backup_always_survives_and_foreign_names_are_untouched() {
    let mut files = vec![auto(at(2020, 1, 1, 3))];
    files.push(("manual-20200101-030000.db".into(), at(2020, 1, 1, 3)));
    files.push(("workspace-v3-1700000000.db".into(), 1_700_000_000));
    files.push(("auto-2020.db".into(), 0));
    files.push(("notes.txt".into(), 0));
    let del = files_to_delete(&files, at(2026, 10, 11, 0));
    assert!(del.is_empty(), "{del:?}");
  }

  #[test]
  fn a_name_whose_time_does_not_match_is_ignored() {
    let (name, _) = auto(at(2026, 10, 1, 3));
    assert!(files_to_delete(&[(name, 5)], 0).is_empty());
  }

  #[test]
  fn backup_is_verified_then_renamed_and_listed() {
    let dir = temp_dir("write");
    let db = Connection::open(dir.join("src.db")).unwrap();
    db.execute_batch("CREATE TABLE t(a); INSERT INTO t VALUES (1);").unwrap();
    let backups = dir.join("backups");
    let secs = at(2026, 10, 11, 12);

    let made = write_backup(&db, &backups, "manual", secs).unwrap();
    assert_eq!(made.file, "manual-20261011-120000.db");
    assert!(made.size > 0);
    assert!(fs::read_dir(&backups).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with('.')));
    let copy = Connection::open(&made.path).unwrap();
    assert_eq!(copy.query_row("SELECT a FROM t", [], |r| r.get::<_, i64>(0)).unwrap(), 1);
    assert_eq!(write_backup(&db, &backups, "manual", secs).unwrap(), made);

    write_backup(&db, &backups, "auto", secs + 60).unwrap();
    fs::write(backups.join("workspace-v2-1700000000.db"), b"x").unwrap();
    fs::write(backups.join("random.db"), b"x").unwrap();
    let listed = list_in(&backups);
    let kinds: Vec<_> = listed.iter().map(|b| b.kind.as_str()).collect();
    assert_eq!(kinds, vec!["auto", "manual", "migration"]);
    assert_eq!(listed[2].created_at, "2023-11-14T22:13:20Z");
    let _ = fs::remove_dir_all(&dir);
  }

  #[test]
  fn a_failed_backup_leaves_no_files() {
    let dir = temp_dir("fail");
    let db = Connection::open_in_memory().unwrap();
    let backups = dir.join("backups");
    fs::create_dir_all(backups.join(".manual-20261011-120000.db.tmp")).unwrap();
    assert!(write_backup(&db, &backups, "manual", at(2026, 10, 11, 12)).is_err());
    assert!(!backups.join("manual-20261011-120000.db").exists());
    let _ = fs::remove_dir_all(&dir);
  }

  #[test]
  fn a_corrupt_file_fails_verification() {
    let dir = temp_dir("verify");
    fs::write(dir.join("bad.db"), vec![7u8; 4096]).unwrap();
    assert!(verify(&dir.join("bad.db")).is_err());
    let _ = fs::remove_dir_all(&dir);
  }

  #[test]
  fn due_when_missing_or_older_than_a_day() {
    let dir = temp_dir("due");
    let now = at(2026, 10, 11, 12);
    assert!(is_due(&dir, now));
    fs::write(dir.join(auto(now - 3600).0), b"x").unwrap();
    assert!(!is_due(&dir, now));
    assert!(is_due(&dir, now + DAY));
    let _ = fs::remove_dir_all(&dir);
  }

  #[test]
  fn future_dated_files_do_not_block_backups_or_take_keep_slots() {
    let dir = temp_dir("future");
    let now = at(2026, 10, 11, 12);
    fs::write(dir.join(auto(now + 30 * DAY).0), b"x").unwrap();
    assert!(is_due(&dir, now));
    assert_eq!(newest_secs(&dir, now), None);
    fs::write(dir.join(auto(now - 2 * DAY).0), b"x").unwrap();
    assert_eq!(newest_secs(&dir, now), Some(now - 2 * DAY));

    let future = auto(now + 30 * DAY);
    let mut files: Vec<_> = (0..10).map(|i| auto(now - i * DAY)).collect();
    files.push(future.clone());
    let del = files_to_delete(&files, now);
    assert!(!del.contains(&future.0));
    assert!(!del.contains(&files[0].0));
    assert!(!del.contains(&files[6].0));
    let _ = fs::remove_dir_all(&dir);
  }

  #[test]
  fn only_old_temp_files_of_the_kind_are_removed() {
    let dir = temp_dir("stale");
    let stale = dir.join(".auto-20200101-000000.db.tmp");
    let other_kind = dir.join(".manual-20200101-000000.db.tmp");
    let real = dir.join("auto-20200101-000000.db");
    for f in [&stale, &other_kind, &real] {
      fs::write(f, b"x").unwrap();
    }
    let now = crate::tray::now_secs();
    let old = std::time::UNIX_EPOCH + Duration::from_secs((now - 2 * STALE_TMP) as u64);
    for f in [&stale, &other_kind] {
      fs::File::options().write(true).open(f).unwrap().set_modified(old).unwrap();
    }
    remove_stale_temp_files(&dir, "auto", now);
    assert!(!stale.exists() && other_kind.exists() && real.exists());

    let fresh = dir.join(".auto-20261011-120000.db.tmp");
    fs::write(&fresh, b"x").unwrap();
    remove_stale_temp_files(&dir, "auto", now);
    assert!(fresh.exists());
    let _ = fs::remove_dir_all(&dir);
  }
}
