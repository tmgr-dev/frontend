use std::collections::HashMap;
use std::sync::Arc;

use super::*;
use crate::manager::relay::ports::{Cells, ChunkedStore, KEYCHAIN_LOG_CAP};

const NS: &str = "u42@https://api.example.com/api/";

#[derive(Default)]
struct MemCells {
  map: Mutex<HashMap<String, String>>,
  fail_get: Mutex<Option<String>>,
  fail_delete: Mutex<Option<String>>,
  hold_delete: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl MemCells {
  fn keys(&self) -> Vec<String> {
    let mut k: Vec<String> = self.map.lock().unwrap().keys().cloned().collect();
    k.sort();
    k
  }
  fn has(&self, account: &str) -> bool {
    self.map.lock().unwrap().contains_key(account)
  }
  fn data_chunks(&self, slot: &str) -> usize {
    let prefix = format!("{NS}/{slot}.v");
    self.keys().iter().filter(|k| k.starts_with(&prefix)).count()
  }
}

fn matches(rule: &Mutex<Option<String>>, account: &str) -> bool {
  rule.lock().unwrap().as_deref().is_some_and(|r| account.contains(r))
}

impl Cells for Arc<MemCells> {
  fn get(&self, account: &str) -> Result<Option<String>, String> {
    if matches(&self.fail_get, account) {
      return Err("keychain locked".into());
    }
    Ok(self.map.lock().unwrap().get(account).cloned())
  }
  fn set(&self, account: &str, value: &str) -> Result<(), String> {
    self.map.lock().unwrap().insert(account.to_owned(), value.to_owned());
    Ok(())
  }
  fn delete(&self, account: &str) -> Result<(), String> {
    if matches(&self.fail_delete, account) {
      return Err("keychain denied".into());
    }
    if account.contains("/log.v") {
      if let Some(gate) = self.hold_delete.lock().unwrap().as_ref() {
        let _ = gate.recv();
      }
    }
    self.map.lock().unwrap().remove(account);
    Ok(())
  }
}

#[derive(Default)]
struct FailFs {
  at: Mutex<Option<Step>>,
}

impl FailFs {
  fn at(step: Step) -> Self {
    Self { at: Mutex::new(Some(step)) }
  }
}

impl Fs for FailFs {
  fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>> {
    DiskFs.read(path)
  }
  fn write_atomic(&self, path: &Path, data: &[u8]) -> io::Result<()> {
    DiskFs.write_steps(path, data, &|s| self.fail(s))
  }
  fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
    DiskFs.rename(from, to)
  }
  fn remove(&self, path: &Path) -> io::Result<()> {
    DiskFs.remove(path)
  }
  fn fail(&self, step: Step) -> io::Result<()> {
    if *self.at.lock().unwrap() == Some(step) {
      return Err(io::Error::other(format!("failpoint {step:?}")));
    }
    Ok(())
  }
}

type Store = ChunkedStore<Arc<MemCells>>;

fn store(cells: &Arc<MemCells>) -> Store {
  ChunkedStore::with_cells(cells.clone(), NS.to_owned())
}

fn vault(cells: &Arc<MemCells>, dir: &Path) -> FileVault<Store, FailFs> {
  vault_fs(cells, dir, FailFs::default())
}

fn vault_fs(cells: &Arc<MemCells>, dir: &Path, fs: FailFs) -> FileVault<Store, FailFs> {
  FileVault::with_fs(store(cells), fs, dir.to_path_buf(), NS).sweeping_inline()
}

fn log_json(n: usize, tag: &str) -> String {
  let items: Vec<String> = (0..n).map(|i| format!(r#"{{"id":"m{i}","body":"{tag} message number {i} with some padding text"}}"#)).collect();
  format!("[{}]", items.join(","))
}

fn files(dir: &Path) -> Vec<String> {
  let mut v: Vec<String> = fs::read_dir(dir).map(|d| d.filter_map(|e| e.ok()).map(|e| e.file_name().to_string_lossy().into_owned()).collect()).unwrap_or_default();
  v.sort();
  v
}

fn pointer_key(slot: &str) -> String {
  format!("{NS}/{slot}.n")
}

#[test]
fn file_round_trips_without_plaintext_and_survives_restart() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  let v = vault(&cells, tmp.path());
  let log = log_json(3, "secret-body-xyz");
  v.save(Slot::Log, &log).unwrap();
  v.save(Slot::State, r#"{"seen":[]}"#).unwrap();
  let raw = fs::read(v.path(Slot::Log)).unwrap();
  assert!(!raw.windows(15).any(|w| w == b"secret-body-xyz"));
  assert_eq!(&raw[..7], b"TMGRCV\x01");
  assert!(!cells.has(&pointer_key("log")));
  assert!(cells.has(&pointer_key("vault-key")));
  let again = vault(&cells, tmp.path());
  assert_eq!(again.load(Slot::Log).unwrap().as_deref(), Some(log.as_str()));
  assert_eq!(again.load(Slot::State).unwrap().as_deref(), Some(r#"{"seen":[]}"#));
  assert!(!files(tmp.path()).iter().any(|f| f.contains("42") || f.contains("example")));
}

#[cfg(unix)]
#[test]
fn file_is_private_to_the_user() {
  use std::os::unix::fs::PermissionsExt;
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  let v = vault(&cells, tmp.path());
  v.save(Slot::Log, "[]").unwrap();
  assert_eq!(fs::metadata(v.path(Slot::Log)).unwrap().permissions().mode() & 0o777, 0o600);
}

#[test]
fn every_write_uses_a_fresh_nonce() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  let v = vault(&cells, tmp.path());
  v.save(Slot::Log, "[1,2,3]").unwrap();
  let a = fs::read(v.path(Slot::Log)).unwrap();
  v.save(Slot::Log, "[1,2,3]").unwrap();
  let b = fs::read(v.path(Slot::Log)).unwrap();
  assert_eq!(a.len(), b.len());
  assert_ne!(a[HEADER..HEADER + NONCE], b[HEADER..HEADER + NONCE]);
  assert_ne!(a[HEADER + NONCE..], b[HEADER + NONCE..]);
}

#[test]
fn a_log_file_does_not_open_as_state() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  let v = vault(&cells, tmp.path());
  v.save(Slot::Log, "[]").unwrap();
  fs::copy(v.path(Slot::Log), v.path(Slot::State)).unwrap();
  assert_eq!(vault(&cells, tmp.path()).load(Slot::State).unwrap(), None);
  assert!(!v.path(Slot::State).exists());
  assert_eq!(v.load(Slot::Log).unwrap(), Some("[]".into()));
}

#[test]
fn identity_stays_in_the_keychain() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  let v = vault(&cells, tmp.path());
  v.save(Slot::Identity, r#"{"device":"d1"}"#).unwrap();
  assert!(cells.has(&pointer_key("identity")));
  assert_eq!(store(&cells).load(Slot::Identity).unwrap().as_deref(), Some(r#"{"device":"d1"}"#));
  assert!(files(tmp.path()).is_empty());
}

#[test]
fn migration_moves_chunks_into_the_file_and_sweeps_them() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  let log = log_json(400, "old");
  let state = r#"{"seen":[{"from":"a","id":"b","at":1}]}"#;
  store(&cells).save(Slot::Log, &log).unwrap();
  store(&cells).save(Slot::State, state).unwrap();
  store(&cells).save(Slot::Identity, "{}").unwrap();
  assert!(cells.data_chunks("log") > 10);

  let v = vault(&cells, tmp.path());
  assert_eq!(v.load(Slot::Log).unwrap().as_deref(), Some(log.as_str()));
  assert_eq!(v.load(Slot::State).unwrap().as_deref(), Some(state));

  assert!(!cells.has(&pointer_key("log")) && !cells.has(&pointer_key("state")));
  assert_eq!(cells.data_chunks("log"), 0);
  assert_eq!(cells.data_chunks("state"), 0);
  assert!(cells.has(&pointer_key("identity")));
  assert!(!files(tmp.path()).iter().any(|f| f.ends_with(".sweep") || f.ends_with(".tmp")));

  let again = vault(&cells, tmp.path());
  assert_eq!(again.load(Slot::Log).unwrap().as_deref(), Some(log.as_str()));
  assert_eq!(again.load(Slot::State).unwrap().as_deref(), Some(state));
}

#[test]
fn failed_file_write_keeps_chunks_and_saves_to_them_until_restart() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  store(&cells).save(Slot::Log, &log_json(50, "old")).unwrap();

  let v = vault_fs(&cells, tmp.path(), FailFs::at(Step::WriteTmp));
  assert_eq!(v.load(Slot::Log).unwrap(), Some(log_json(50, "old")));
  assert!(cells.has(&pointer_key("log")));
  assert!(!v.path(Slot::Log).exists());

  v.save(Slot::Log, &log_json(51, "newer")).unwrap();
  assert_eq!(store(&cells).load(Slot::Log).unwrap(), Some(log_json(51, "newer")));

  let after = vault(&cells, tmp.path());
  assert_eq!(after.load(Slot::Log).unwrap(), Some(log_json(51, "newer")));
  assert!(!cells.has(&pointer_key("log")));
  assert_eq!(vault(&cells, tmp.path()).load(Slot::Log).unwrap(), Some(log_json(51, "newer")));
}

#[test]
fn failed_read_back_keeps_chunks() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  store(&cells).save(Slot::State, r#"{"a":1}"#).unwrap();
  let v = vault_fs(&cells, tmp.path(), FailFs::at(Step::ReadBack));
  assert_eq!(v.load(Slot::State).unwrap().as_deref(), Some(r#"{"a":1}"#));
  assert!(cells.has(&pointer_key("state")));
  assert!(cells.data_chunks("state") > 0);
}

#[test]
fn crash_after_tmp_before_rename_recovers_on_next_start() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  store(&cells).save(Slot::Log, &log_json(20, "old")).unwrap();
  let crashed = vault_fs(&cells, tmp.path(), FailFs::at(Step::WriteTmp));
  crashed.load(Slot::Log).unwrap();
  assert!(files(tmp.path()).iter().any(|f| f.ends_with(".tmp")));
  drop(crashed);

  let v = vault(&cells, tmp.path());
  assert_eq!(v.load(Slot::Log).unwrap(), Some(log_json(20, "old")));
  assert!(!cells.has(&pointer_key("log")));
  v.clear().unwrap();
  assert!(!files(tmp.path()).iter().any(|f| f.ends_with(".tmp")));
}

#[test]
fn writers_never_share_a_tmp_file() {
  let p = Path::new("/x/a.log.bin");
  assert_ne!(tmp_of(p), tmp_of(p));
}

#[test]
fn crash_after_file_write_before_pointer_delete_takes_newer_chunks() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  store(&cells).save(Slot::Log, &log_json(20, "old")).unwrap();
  let crashed = vault_fs(&cells, tmp.path(), FailFs::at(Step::Marker));
  crashed.load(Slot::Log).unwrap();
  assert!(crashed.path(Slot::Log).exists());
  assert!(cells.has(&pointer_key("log")));
  crashed.save(Slot::Log, &log_json(21, "newer")).unwrap();
  drop(crashed);

  let v = vault(&cells, tmp.path());
  assert_eq!(v.load(Slot::Log).unwrap(), Some(log_json(21, "newer")));
  assert_eq!(vault(&cells, tmp.path()).load(Slot::Log).unwrap(), Some(log_json(21, "newer")));
}

#[test]
fn crash_after_pointer_delete_sweeps_on_next_start() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  store(&cells).save(Slot::Log, &log_json(100, "old")).unwrap();
  *cells.fail_delete.lock().unwrap() = Some(".v1.".into());
  let v = vault(&cells, tmp.path());
  assert_eq!(v.load(Slot::Log).unwrap(), Some(log_json(100, "old")));
  assert!(!cells.has(&pointer_key("log")));
  assert!(cells.data_chunks("log") > 0);
  assert!(v.marker(Slot::Log).exists());
  drop(v);

  *cells.fail_delete.lock().unwrap() = None;
  let v = vault(&cells, tmp.path());
  assert_eq!(v.load(Slot::Log).unwrap(), Some(log_json(100, "old")));
  assert_eq!(cells.data_chunks("log"), 0);
  assert!(!v.marker(Slot::Log).exists());
}

#[test]
fn corrupt_file_with_chunks_uses_the_chunks() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  store(&cells).save(Slot::Log, &log_json(5, "old")).unwrap();
  let v = vault(&cells, tmp.path());
  fs::create_dir_all(tmp.path()).unwrap();
  fs::write(v.path(Slot::Log), b"garbage").unwrap();
  assert_eq!(v.load(Slot::Log).unwrap(), Some(log_json(5, "old")));
  assert_eq!(vault(&cells, tmp.path()).load(Slot::Log).unwrap(), Some(log_json(5, "old")));
}

#[test]
fn corrupt_log_without_chunks_is_moved_aside_and_never_overwritten() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  let v = vault(&cells, tmp.path());
  v.save(Slot::Log, "[1]").unwrap();
  fs::write(v.path(Slot::Log), b"garbage one").unwrap();
  let v = vault(&cells, tmp.path());
  assert_eq!(v.load(Slot::Log).unwrap(), None);
  assert!(v.log_reset());
  assert!(!v.log_reset());

  v.save(Slot::Log, "[2]").unwrap();
  fs::write(v.path(Slot::Log), b"garbage two").unwrap();
  let v = vault(&cells, tmp.path());
  assert_eq!(v.load(Slot::Log).unwrap(), None);

  let corrupt: Vec<String> = files(tmp.path()).into_iter().filter(|f| f.contains(".log.bin.corrupt-")).collect();
  assert_eq!(corrupt.len(), 2);
  let contents: Vec<Vec<u8>> = corrupt.iter().map(|f| fs::read(tmp.path().join(f)).unwrap()).collect();
  assert!(contents.contains(&b"garbage one".to_vec()) && contents.contains(&b"garbage two".to_vec()));

  v.clear().unwrap();
  assert_eq!(files(tmp.path()).iter().filter(|f| f.contains(".corrupt-")).count(), 2);
}

#[test]
fn quarantine_picks_a_free_name() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  let v = vault(&cells, tmp.path());
  for i in 0..3 {
    fs::write(v.path(Slot::Log), format!("bad {i}")).unwrap();
    v.quarantine(Slot::Log, "corrupt").unwrap();
  }
  assert_eq!(files(tmp.path()).iter().filter(|f| f.contains(".corrupt-")).count(), 3);
}

#[test]
fn corrupt_state_without_chunks_is_moved_aside_and_starts_empty() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  let v = vault(&cells, tmp.path());
  v.save(Slot::State, "{}").unwrap();
  fs::write(v.path(Slot::State), b"garbage").unwrap();
  let v = vault(&cells, tmp.path());
  assert_eq!(v.load(Slot::State).unwrap(), None);
  assert!(v.log_reset());
  let corrupt: Vec<String> = files(tmp.path()).into_iter().filter(|f| f.contains(".state.bin.corrupt-")).collect();
  assert_eq!(corrupt.len(), 1);
  assert_eq!(fs::read(tmp.path().join(&corrupt[0])).unwrap(), b"garbage");
}

#[test]
fn a_leftover_marker_keeps_the_chunks_while_the_file_is_unreadable() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  store(&cells).save(Slot::Log, &log_json(100, "old")).unwrap();
  *cells.fail_delete.lock().unwrap() = Some(".v1.".into());
  let v = vault(&cells, tmp.path());
  v.load(Slot::Log).unwrap();
  *cells.fail_delete.lock().unwrap() = None;
  fs::write(v.path(Slot::Log), b"garbage").unwrap();
  drop(v);
  let v = vault(&cells, tmp.path());
  assert_eq!(v.load(Slot::Log).unwrap(), None);
  assert!(cells.data_chunks("log") > 0);
  assert!(v.marker(Slot::Log).exists());
}

#[test]
fn re_migration_sets_the_existing_file_aside() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  store(&cells).save(Slot::Log, &log_json(30, "first")).unwrap();
  vault(&cells, tmp.path()).load(Slot::Log).unwrap();
  store(&cells).save(Slot::Log, &log_json(2, "downgrade")).unwrap();
  let v = vault(&cells, tmp.path());
  assert_eq!(v.load(Slot::Log).unwrap(), Some(log_json(2, "downgrade")));
  assert_eq!(files(tmp.path()).iter().filter(|f| f.contains(".log.bin.pre-migrate-")).count(), 1);
}

#[test]
fn the_log_cap_follows_where_the_log_is_stored() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  assert_eq!(vault(&cells, tmp.path()).log_cap(), FILE_LOG_CAP);
  assert_eq!(store(&cells).log_cap(), KEYCHAIN_LOG_CAP);
  store(&cells).save(Slot::Log, &log_json(5, "old")).unwrap();
  let v = vault_fs(&cells, tmp.path(), FailFs::at(Step::WriteTmp));
  v.load(Slot::Log).unwrap();
  assert_eq!(v.log_cap(), KEYCHAIN_LOG_CAP);
}

#[test]
fn lost_key_moves_the_log_aside() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  let v = vault(&cells, tmp.path());
  v.save(Slot::Log, "[1]").unwrap();
  cells.map.lock().unwrap().retain(|k, _| !k.contains("vault-key"));
  let v = vault(&cells, tmp.path());
  assert_eq!(v.load(Slot::Log).unwrap(), None);
  assert!(v.log_reset());
  v.save(Slot::Log, "[2]").unwrap();
  assert_eq!(vault(&cells, tmp.path()).load(Slot::Log).unwrap().as_deref(), Some("[2]"));
}

#[test]
fn locked_keychain_is_an_error_and_moves_nothing() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  let v = vault(&cells, tmp.path());
  v.save(Slot::Log, "[1]").unwrap();
  *cells.fail_get.lock().unwrap() = Some("vault-key".into());
  assert_eq!(vault(&cells, tmp.path()).load(Slot::Log).unwrap_err(), "keychain locked");
  assert!(!files(tmp.path()).iter().any(|f| f.contains(".corrupt-")));
  *cells.fail_get.lock().unwrap() = None;
  assert_eq!(vault(&cells, tmp.path()).load(Slot::Log).unwrap().as_deref(), Some("[1]"));
}

#[test]
fn clear_removes_files_and_key() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  let v = vault(&cells, tmp.path());
  v.save(Slot::Log, "[1]").unwrap();
  v.save(Slot::State, "{}").unwrap();
  v.save(Slot::Identity, "{}").unwrap();
  v.clear().unwrap();
  assert!(files(tmp.path()).is_empty());
  assert!(cells.keys().is_empty());
  assert_eq!(v.load(Slot::Log).unwrap(), None);
  v.save(Slot::Log, "[3]").unwrap();
  assert_eq!(vault(&cells, tmp.path()).load(Slot::Log).unwrap().as_deref(), Some("[3]"));
}

#[test]
fn clear_sweeps_chunks_an_unfinished_migration_left() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  for i in 0..3 {
    store(&cells).save(Slot::Log, &log_json(100, &format!("old{i}"))).unwrap();
  }
  assert!(cells.get(&pointer_key("log")).unwrap().unwrap().starts_with("3:"));
  *cells.fail_delete.lock().unwrap() = Some(".v3.".into());
  let v = vault(&cells, tmp.path());
  v.load(Slot::Log).unwrap();
  assert!(cells.data_chunks("log") > 0);
  *cells.fail_delete.lock().unwrap() = None;
  v.clear().unwrap();
  assert!(cells.keys().is_empty());
  assert!(files(tmp.path()).is_empty());
}

#[test]
fn clear_finishes_when_old_chunks_cannot_be_swept() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  for i in 0..3 {
    store(&cells).save(Slot::Log, &log_json(10, &format!("old{i}"))).unwrap();
  }
  *cells.fail_delete.lock().unwrap() = Some(".v3.".into());
  let v = vault(&cells, tmp.path());
  v.load(Slot::Log).unwrap();
  v.clear().unwrap();
  assert!(files(tmp.path()).is_empty());
}

#[test]
fn a_slow_background_sweep_does_not_hold_up_reads_and_writes() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  store(&cells).save(Slot::Log, &log_json(50, "old")).unwrap();
  let (release, gate) = std::sync::mpsc::channel();
  *cells.hold_delete.lock().unwrap() = Some(gate);
  let v = FileVault::with_fs(store(&cells), FailFs::default(), tmp.path().to_path_buf(), NS);
  v.load(Slot::Log).unwrap();
  let t = Instant::now();
  v.save(Slot::Log, &log_json(51, "new")).unwrap();
  v.save(Slot::Identity, "{}").unwrap();
  assert_eq!(v.load(Slot::Log).unwrap(), Some(log_json(51, "new")));
  assert!(t.elapsed() < std::time::Duration::from_secs(1));
  assert!(cells.data_chunks("log") > 0);
  drop(release);
  let t = Instant::now();
  while cells.data_chunks("log") > 0 {
    assert!(t.elapsed() < std::time::Duration::from_secs(10));
    std::thread::sleep(std::time::Duration::from_millis(10));
  }
}

#[test]
fn migration_of_a_5_mb_log_is_fast() {
  let (cells, tmp) = (Arc::new(MemCells::default()), tempfile::tempdir().unwrap());
  let log = log_json(80_000, "big");
  assert!(log.len() > 5 * 1024 * 1024);
  store(&cells).save(Slot::Log, &log).unwrap();
  let v = vault(&cells, tmp.path());
  let t = Instant::now();
  assert_eq!(v.load(Slot::Log).unwrap().map(|s| s.len()), Some(log.len()));
  let migrate = t.elapsed();
  let t = Instant::now();
  v.save(Slot::Log, &log).unwrap();
  let save = t.elapsed();
  eprintln!("5 MB log: migrate {} ms (in-memory keychain), file save {} ms", migrate.as_millis(), save.as_millis());
  assert!(save.as_millis() < 1000);
}

