//! Chat log and state in AES-256-GCM files under app data; the file key and the identity stay in the keychain.

use std::collections::BTreeSet;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM};
use sha2::{Digest, Sha256};
use tmgr_phone::{b64, b64_decode, hex};

use super::ports::{KeyStore, Slot};

const MAGIC: &[u8; 6] = b"TMGRCV";
const VERSION: u8 = 1;
const HEADER: usize = MAGIC.len() + 1;
const NONCE: usize = 12;
const KEY: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
  WriteTmp,
  Rename,
  ReadBack,
  Marker,
}

pub trait Fs: Send + Sync {
  fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>>;
  fn write_atomic(&self, path: &Path, data: &[u8]) -> io::Result<()>;
  fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
  fn remove(&self, path: &Path) -> io::Result<()>;
  fn exists(&self, path: &Path) -> bool {
    path.exists()
  }
  fn fail(&self, _step: Step) -> io::Result<()> {
    Ok(())
  }
}

pub struct DiskFs;

fn tmp_of(path: &Path) -> PathBuf {
  let mut p = path.as_os_str().to_owned();
  p.push(".tmp");
  PathBuf::from(p)
}

#[cfg(target_vendor = "apple")]
fn full_sync(f: &fs::File) -> io::Result<()> {
  use std::os::fd::AsRawFd;
  if unsafe { libc::fcntl(f.as_raw_fd(), libc::F_FULLFSYNC) } == -1 {
    return f.sync_all();
  }
  Ok(())
}

#[cfg(not(target_vendor = "apple"))]
fn full_sync(f: &fs::File) -> io::Result<()> {
  f.sync_all()
}

fn sync_dir(dir: &Path) -> io::Result<()> {
  #[cfg(unix)]
  {
    full_sync(&fs::File::open(dir)?)
  }
  #[cfg(not(unix))]
  {
    let _ = dir;
    Ok(())
  }
}

fn create_private(path: &Path) -> io::Result<fs::File> {
  let mut o = fs::OpenOptions::new();
  o.write(true).create(true).truncate(true);
  #[cfg(unix)]
  {
    use std::os::unix::fs::OpenOptionsExt;
    o.mode(0o600);
  }
  o.open(path)
}

impl DiskFs {
  fn write_steps(&self, path: &Path, data: &[u8], fail: &dyn Fn(Step) -> io::Result<()>) -> io::Result<()> {
    let dir = path.parent().ok_or_else(|| io::Error::other("no parent dir"))?;
    fs::create_dir_all(dir)?;
    let tmp = tmp_of(path);
    let mut f = create_private(&tmp)?;
    f.write_all(data)?;
    full_sync(&f)?;
    drop(f);
    fail(Step::WriteTmp)?;
    fs::rename(&tmp, path)?;
    fail(Step::Rename)?;
    sync_dir(dir)
  }
}

impl Fs for DiskFs {
  fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::read(path) {
      Ok(b) => Ok(Some(b)),
      Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
      Err(e) => Err(e),
    }
  }

  fn write_atomic(&self, path: &Path, data: &[u8]) -> io::Result<()> {
    self.write_steps(path, data, &|_| Ok(()))
  }

  fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
    fs::rename(from, to)?;
    if let Some(dir) = to.parent() {
      sync_dir(dir)?;
    }
    Ok(())
  }

  fn remove(&self, path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
      Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
      _ => Ok(()),
    }
  }
}

enum Read {
  Missing,
  Plain(String),
  Unreadable,
}

pub struct FileVault<K: KeyStore + 'static, F: Fs + 'static = DiskFs> {
  inner: Arc<K>,
  fs: Arc<F>,
  dir: PathBuf,
  stem: String,
  key: Mutex<Option<[u8; KEY]>>,
  keychain_mode: Mutex<BTreeSet<&'static str>>,
  reset: Mutex<bool>,
  sweep_inline: bool,
}

fn is_file_slot(slot: Slot) -> bool {
  matches!(slot, Slot::Log | Slot::State)
}

fn sha(s: &str) -> [u8; 32] {
  Sha256::digest(s.as_bytes()).into()
}

fn entries(s: &str) -> Option<usize> {
  serde_json::from_str::<Vec<serde::de::IgnoredAny>>(s).ok().map(|v| v.len())
}

impl<K: KeyStore + 'static> FileVault<K, DiskFs> {
  pub fn new(inner: K, dir: PathBuf, namespace: &str) -> Self {
    Self::with_fs(inner, DiskFs, dir, namespace)
  }
}

impl<K: KeyStore + 'static, F: Fs + 'static> FileVault<K, F> {
  pub fn with_fs(inner: K, fs: F, dir: PathBuf, namespace: &str) -> Self {
    let stem = hex(&Sha256::digest(namespace.as_bytes()))[..32].to_owned();
    Self { inner: Arc::new(inner), fs: Arc::new(fs), dir, stem, key: Mutex::new(None), keychain_mode: Mutex::new(BTreeSet::new()), reset: Mutex::new(false), sweep_inline: false }
  }

  #[cfg(test)]
  pub fn sweeping_inline(mut self) -> Self {
    self.sweep_inline = true;
    self
  }

  pub fn path(&self, slot: Slot) -> PathBuf {
    self.dir.join(format!("{}.{}.bin", self.stem, slot.name()))
  }

  fn marker(&self, slot: Slot) -> PathBuf {
    self.dir.join(format!("{}.{}.sweep", self.stem, slot.name()))
  }

  fn modes(&self) -> MutexGuard<'_, BTreeSet<&'static str>> {
    self.keychain_mode.lock().unwrap_or_else(|p| p.into_inner())
  }

  fn aad(&self, slot: Slot) -> Vec<u8> {
    let mut a = Vec::with_capacity(HEADER + 16 + self.stem.len());
    a.extend_from_slice(MAGIC);
    a.push(VERSION);
    a.extend_from_slice(slot.name().as_bytes());
    a.push(0);
    a.extend_from_slice(self.stem.as_bytes());
    a
  }

  fn cached_key(&self, create: bool) -> Result<Option<[u8; KEY]>, String> {
    let mut cached = self.key.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(k) = *cached {
      return Ok(Some(k));
    }
    let parse = |raw: &str| -> Result<[u8; KEY], String> { b64_decode(raw).ok().and_then(|b| <[u8; KEY]>::try_from(b.as_slice()).ok()).ok_or_else(|| "the chat file key is unreadable".to_owned()) };
    let key = match self.inner.load(Slot::VaultKey)? {
      Some(raw) => parse(&raw)?,
      None if create => {
        let mut k = [0u8; KEY];
        getrandom::fill(&mut k).map_err(|_| "no randomness for the chat file key".to_owned())?;
        self.inner.save(Slot::VaultKey, &b64(&k))?;
        let back = self.inner.load(Slot::VaultKey)?.ok_or_else(|| "the chat file key was not stored".to_owned())?;
        if parse(&back)? != k {
          return Err("the chat file key was not stored".into());
        }
        k
      }
      None => return Ok(None),
    };
    *cached = Some(key);
    Ok(Some(key))
  }

  fn cipher(key: &[u8; KEY]) -> Option<LessSafeKey> {
    UnboundKey::new(&AES_256_GCM, key).ok().map(LessSafeKey::new)
  }

  fn seal(&self, slot: Slot, key: &[u8; KEY], plain: &str) -> Result<Vec<u8>, String> {
    let mut nonce = [0u8; NONCE];
    getrandom::fill(&mut nonce).map_err(|_| "no randomness for the chat file".to_owned())?;
    let cipher = Self::cipher(key).ok_or_else(|| "bad chat file key".to_owned())?;
    let mut out = Vec::with_capacity(HEADER + NONCE + plain.len() + AES_256_GCM.tag_len());
    out.extend_from_slice(MAGIC);
    out.push(VERSION);
    out.extend_from_slice(&nonce);
    let mut body = plain.as_bytes().to_vec();
    cipher.seal_in_place_append_tag(Nonce::assume_unique_for_key(nonce), Aad::from(self.aad(slot)), &mut body).map_err(|_| "the chat file could not be encrypted".to_owned())?;
    out.extend_from_slice(&body);
    Ok(out)
  }

  fn open(&self, slot: Slot, key: &[u8; KEY], data: &[u8]) -> Option<String> {
    if data.len() < HEADER + NONCE || &data[..MAGIC.len()] != MAGIC || data[MAGIC.len()] != VERSION {
      return None;
    }
    let nonce = Nonce::try_assume_unique_for_key(&data[HEADER..HEADER + NONCE]).ok()?;
    let mut body = data[HEADER + NONCE..].to_vec();
    let plain = Self::cipher(key)?.open_in_place(nonce, Aad::from(self.aad(slot)), &mut body).ok()?;
    let len = plain.len();
    body.truncate(len);
    String::from_utf8(body).ok()
  }

  fn read_file(&self, slot: Slot) -> Result<Read, String> {
    let Some(data) = self.fs.read(&self.path(slot)).map_err(|e| format!("the chat file could not be read: {}", e.kind()))? else {
      return Ok(Read::Missing);
    };
    let Some(key) = self.cached_key(false)? else {
      return Ok(Read::Unreadable);
    };
    Ok(self.open(slot, &key, &data).map_or(Read::Unreadable, Read::Plain))
  }

  fn write_file(&self, slot: Slot, value: &str) -> Result<(), String> {
    let key = self.cached_key(true)?.ok_or_else(|| "no chat file key".to_owned())?;
    let sealed = self.seal(slot, &key, value)?;
    let path = self.path(slot);
    self.fs.write_atomic(&path, &sealed).map_err(|e| format!("the chat file could not be written: {}", e.kind()))
  }

  fn quarantine(&self, slot: Slot) -> Result<(), String> {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis());
    let base = self.path(slot);
    let mut n = 0u32;
    let target = loop {
      let suffix = if n == 0 { format!(".corrupt-{now}") } else { format!(".corrupt-{now}-{n}") };
      let mut p = base.as_os_str().to_owned();
      p.push(suffix);
      let p = PathBuf::from(p);
      if !self.fs.exists(&p) {
        break p;
      }
      n += 1;
    };
    self.fs.rename(&base, &target).map_err(|e| format!("the unreadable chat file could not be moved aside: {}", e.kind()))
  }

  fn sweep_later(&self, slot: Slot, pointer: String) {
    let (inner, fs, marker) = (self.inner.clone(), self.fs.clone(), self.marker(slot));
    let run = move || {
      let t = Instant::now();
      match inner.sweep(slot, &pointer) {
        Ok(()) => {
          let _ = fs.remove(&marker);
          log::info!("[chat-relay] removed the old keychain chunks of the chat {} in {} ms", slot.name(), t.elapsed().as_millis());
        }
        Err(e) => log::warn!("[chat-relay] old keychain chunks of the chat {} are not removed yet: {e}", slot.name()),
      }
    };
    if self.sweep_inline {
      run();
    } else if let Err(e) = std::thread::Builder::new().name("chat-relay-sweep".into()).spawn(run) {
      log::warn!("[chat-relay] keychain cleanup did not start: {e}");
    }
  }

  fn resume_sweep(&self, slot: Slot) {
    if let Ok(Some(raw)) = self.fs.read(&self.marker(slot)) {
      if let Ok(pointer) = String::from_utf8(raw) {
        self.sweep_later(slot, pointer);
      }
    }
  }

  fn migrate(&self, slot: Slot, pointer: &str, value: &str) -> Result<(), String> {
    let t = Instant::now();
    self.write_file(slot, value)?;
    self.fs.fail(Step::ReadBack).map_err(|e| e.to_string())?;
    let Read::Plain(back) = self.read_file(slot)? else {
      return Err("the chat file did not read back".into());
    };
    if sha(&back) != sha(value) || entries(&back) != entries(value) {
      return Err("the chat file read back different content".into());
    }
    self.fs.fail(Step::Marker).map_err(|e| e.to_string())?;
    self.fs.write_atomic(&self.marker(slot), pointer.as_bytes()).map_err(|e| format!("the sweep marker could not be written: {}", e.kind()))?;
    self.inner.drop_pointer(slot)?;
    log::info!(
      "[chat-relay] moved the chat {} from the keychain to the encrypted file: {} B, {} entries, {} ms",
      slot.name(),
      value.len(),
      entries(value).map_or_else(|| "-".to_owned(), |n| n.to_string()),
      t.elapsed().as_millis()
    );
    self.sweep_later(slot, pointer.to_owned());
    Ok(())
  }

  fn load_file_slot(&self, slot: Slot) -> Result<Option<String>, String> {
    if let Some(pointer) = self.inner.legacy_pointer(slot)? {
      let t = Instant::now();
      let value = self.inner.load(slot)?.ok_or_else(|| "the chat store is unreadable".to_owned())?;
      log::info!("[chat-relay] read the chat {} from the keychain: {} B in {} ms", slot.name(), value.len(), t.elapsed().as_millis());
      if let Err(e) = self.migrate(slot, &pointer, &value) {
        log::warn!("[chat-relay] the chat {} stays in the keychain for now: {e}", slot.name());
        self.modes().insert(slot.name());
      }
      return Ok(Some(value));
    }
    self.resume_sweep(slot);
    match self.read_file(slot)? {
      Read::Missing => Ok(None),
      Read::Plain(s) => Ok(Some(s)),
      Read::Unreadable if slot == Slot::Log => {
        self.quarantine(slot)?;
        *self.reset.lock().unwrap_or_else(|p| p.into_inner()) = true;
        log::warn!("[chat-relay] the chat log file is unreadable; it was moved aside and the history starts empty");
        Ok(None)
      }
      Read::Unreadable => Err("the chat state is unreadable".into()),
    }
  }
}

impl<K: KeyStore + 'static, F: Fs + 'static> KeyStore for FileVault<K, F> {
  fn load(&self, slot: Slot) -> Result<Option<String>, String> {
    if is_file_slot(slot) {
      self.load_file_slot(slot)
    } else {
      self.inner.load(slot)
    }
  }

  fn save(&self, slot: Slot, value: &str) -> Result<(), String> {
    if is_file_slot(slot) && !self.modes().contains(slot.name()) {
      self.write_file(slot, value)
    } else {
      self.inner.save(slot, value)
    }
  }

  fn clear(&self) -> Result<(), String> {
    self.inner.clear()?;
    *self.key.lock().unwrap_or_else(|p| p.into_inner()) = None;
    self.modes().clear();
    for slot in [Slot::Log, Slot::State] {
      for p in [self.path(slot), self.marker(slot), tmp_of(&self.path(slot))] {
        self.fs.remove(&p).map_err(|e| format!("the chat file could not be removed: {}", e.kind()))?;
      }
    }
    Ok(())
  }

  fn log_reset(&self) -> bool {
    std::mem::take(&mut *self.reset.lock().unwrap_or_else(|p| p.into_inner()))
  }
}

#[cfg(test)]
mod tests;
