//! Desktop approvals for dangerous permission cards: a Secure Enclave key that signs only after the
//! owner proves presence with Touch ID or the login password. One proof stays usable for a short
//! window that ends early on sleep, screen lock or a session switch.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub const WINDOW: Duration = Duration::from_secs(600);
pub const NOT_SET_UP: &str = "Set up desktop approvals first";
pub const SETUP_AGAIN: &str = "Set up desktop approvals again";
pub const EXPIRED: &str = "The Touch ID approval has ended; answer again";
pub const ANSWER_REASON: &str = "approve a dangerous command from the Manager";
pub const SETUP_REASON: &str = "set up desktop approvals for dangerous commands";
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const KEY_FILE: &str = "presence.key";

pub trait Enclave: Send + Sync {
  fn available(&self) -> bool;
  /// A new key as (opaque blob, uncompressed SEC1 public key).
  fn create(&self) -> Result<(Vec<u8>, Vec<u8>), String>;
  /// None when the blob no longer opens a key on this Mac.
  fn public_key(&self, blob: &[u8]) -> Option<Vec<u8>>;
  /// Prompts the owner; Ok is an evaluated context that signs without prompting again.
  fn authenticate(&self, reason: &str) -> Result<usize, String>;
  /// Never prompts: a context that no longer works fails.
  fn sign(&self, ctx: usize, blob: &[u8], msg: &[u8]) -> Result<Vec<u8>, String>;
  fn release(&self, ctx: usize);
  /// Bumped on sleep, screen lock and session switch.
  fn epoch(&self) -> u64;
}

pub type Now = Box<dyn Fn() -> (Instant, u64) + Send + Sync>;

#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub fn system_now() -> (Instant, u64) {
  let wall = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
  (Instant::now(), wall)
}

struct Unlock {
  ctx: usize,
  mono: Instant,
  wall: u64,
  epoch: u64,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq, Eq)]
pub struct PresenceStatus {
  pub available: bool,
  pub set_up: bool,
  pub needs_setup_again: bool,
  pub key_fp: Option<String>,
  pub unlocked_secs: Option<u64>,
}

pub struct Presence {
  enclave: Box<dyn Enclave>,
  path: PathBuf,
  now: Now,
  unlock: Mutex<Option<Unlock>>,
  prompting: Mutex<()>,
}

impl Presence {
  #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
  pub fn new(enclave: Box<dyn Enclave>, dir: PathBuf, now: Now) -> Self {
    Self { enclave, path: dir.join(KEY_FILE), now, unlock: Mutex::new(None), prompting: Mutex::new(()) }
  }

  fn blob(&self) -> Option<Vec<u8>> {
    fs::read(&self.path).ok().filter(|b| !b.is_empty())
  }

  /// Ok(None) before setup; Err when the stored key stopped working.
  pub fn public_key(&self) -> Result<Option<Vec<u8>>, String> {
    match self.blob() {
      None => Ok(None),
      Some(blob) => self.enclave.public_key(&blob).map(Some).ok_or_else(|| SETUP_AGAIN.to_owned()),
    }
  }

  pub fn setup(&self) -> Result<Vec<u8>, String> {
    if !self.enclave.available() {
      return Err("Desktop approvals need a Mac with a Secure Enclave".into());
    }
    let (blob, public) = self.enclave.create()?;
    if let Some(dir) = self.path.parent() {
      fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let tmp = self.path.with_extension("tmp");
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    let mut f = opts.open(&tmp).map_err(|e| e.to_string())?;
    f.write_all(&blob).and_then(|_| f.sync_all()).map_err(|e| e.to_string())?;
    fs::rename(&tmp, &self.path).map_err(|e| e.to_string())?;
    self.lock_now();
    Ok(public)
  }

  fn lock(&self) -> std::sync::MutexGuard<'_, Option<Unlock>> {
    self.unlock.lock().unwrap_or_else(|e| e.into_inner())
  }

  fn live_secs(&self, slot: &mut Option<Unlock>) -> Option<u64> {
    let (mono, wall) = (self.now)();
    let left = slot.as_ref().and_then(|u| {
      let by_mono = WINDOW.checked_sub(mono.saturating_duration_since(u.mono))?;
      let since_wall = wall.checked_sub(u.wall)?;
      let by_wall = WINDOW.checked_sub(Duration::from_millis(since_wall))?;
      (u.epoch == self.enclave.epoch() && !by_mono.is_zero() && !by_wall.is_zero()).then(|| by_mono.min(by_wall).as_secs())
    });
    if left.is_none() {
      if let Some(u) = slot.take() {
        self.enclave.release(u.ctx);
      }
    }
    left
  }

  /// Prompts only when no window is open. Call it outside every other lock.
  pub fn unlock(&self, reason: &str) -> Result<(), String> {
    let _one = self.prompting.lock().unwrap_or_else(|e| e.into_inner());
    if self.public_key()?.is_none() {
      return Err(NOT_SET_UP.into());
    }
    if self.live_secs(&mut self.lock()).is_some() {
      return Ok(());
    }
    let epoch = self.enclave.epoch();
    let ctx = self.enclave.authenticate(reason)?;
    let (mono, wall) = (self.now)();
    let old = self.lock().replace(Unlock { ctx, mono, wall, epoch });
    if let Some(u) = old {
      self.enclave.release(u.ctx);
    }
    Ok(())
  }

  /// Signs `msg` with the window that `unlock` opened, never prompting. Returns a low-S DER signature in base64.
  pub fn sign_unlocked(&self, msg: &str) -> Result<String, String> {
    let blob = self.blob().ok_or_else(|| NOT_SET_UP.to_owned())?;
    let public = self.enclave.public_key(&blob).ok_or_else(|| SETUP_AGAIN.to_owned())?;
    let mut slot = self.lock();
    if self.live_secs(&mut slot).is_none() {
      return Err(EXPIRED.into());
    }
    let ctx = slot.as_ref().map(|u| u.ctx).ok_or_else(|| EXPIRED.to_owned())?;
    let der = match self.enclave.sign(ctx, &blob, msg.as_bytes()) {
      Ok(der) => der,
      Err(_) => {
        if let Some(u) = slot.take() {
          self.enclave.release(u.ctx);
        }
        return Err(EXPIRED.into());
      }
    };
    if self.live_secs(&mut slot).is_none() {
      return Err(EXPIRED.into());
    }
    drop(slot);
    let bad = || "Touch ID returned an invalid signature".to_owned();
    let sig = Signature::from_der(&der).map_err(|_| bad())?;
    let sig = sig.normalize_s().unwrap_or(sig);
    let key = VerifyingKey::from_sec1_bytes(&public).map_err(|_| bad())?;
    key.verify(msg.as_bytes(), &sig).map_err(|_| bad())?;
    Ok(tmgr_phone::b64(sig.to_der().as_bytes()))
  }

  pub fn lock_now(&self) {
    if let Some(u) = self.lock().take() {
      self.enclave.release(u.ctx);
    }
  }

  pub fn status(&self) -> PresenceStatus {
    let available = self.enclave.available();
    let (set_up, again, key_fp) = match self.public_key() {
      Ok(Some(k)) => (true, false, Some(tmgr_phone::hex(&Sha256::digest(&k)))),
      Ok(None) => (false, false, None),
      Err(_) => (false, true, None),
    };
    let unlocked_secs = if set_up { self.live_secs(&mut self.lock()) } else { None };
    PresenceStatus { available, set_up, needs_setup_again: again, key_fp, unlocked_secs }
  }
}

#[cfg(target_os = "macos")]
pub mod macos {
  use std::ffi::{c_char, c_void, CString};

  extern "C" {
    fn tmgr_presence_available() -> i32;
    fn tmgr_presence_watch();
    fn tmgr_presence_create(blob: *mut u8, cap: usize, blob_len: *mut usize, public: *mut u8) -> i32;
    fn tmgr_presence_public(blob: *const u8, len: usize, public: *mut u8) -> i32;
    fn tmgr_presence_evaluate(reason: *const c_char, ctx: *mut *mut c_void) -> i32;
    fn tmgr_presence_sign(ctx: *mut c_void, blob: *const u8, blob_len: usize, msg: *const u8, msg_len: usize, der: *mut u8, cap: usize, der_len: *mut usize) -> i32;
    fn tmgr_presence_release(ctx: *mut c_void);
    fn tmgr_presence_epoch() -> u64;
  }

  const CANCELLED: i32 = -4;
  const BLOB_CAP: usize = 4096;

  pub struct SecureEnclave;

  impl SecureEnclave {
    /// Call once on the main thread so sleep and lock notifications arrive.
    pub fn watch() {
      unsafe { tmgr_presence_watch() }
    }
  }

  impl super::Enclave for SecureEnclave {
    fn available(&self) -> bool {
      unsafe { tmgr_presence_available() == 1 }
    }

    fn create(&self) -> Result<(Vec<u8>, Vec<u8>), String> {
      let mut blob = vec![0u8; BLOB_CAP];
      let mut len = 0usize;
      let mut public = vec![0u8; 65];
      let rc = unsafe { tmgr_presence_create(blob.as_mut_ptr(), blob.len(), &mut len, public.as_mut_ptr()) };
      if rc != 0 || len == 0 || len > BLOB_CAP {
        return Err("Could not create the desktop approval key".into());
      }
      blob.truncate(len);
      Ok((blob, public))
    }

    fn public_key(&self, blob: &[u8]) -> Option<Vec<u8>> {
      let mut public = vec![0u8; 65];
      let rc = unsafe { tmgr_presence_public(blob.as_ptr(), blob.len(), public.as_mut_ptr()) };
      (rc == 0).then_some(public)
    }

    fn authenticate(&self, reason: &str) -> Result<usize, String> {
      let reason = CString::new(reason).map_err(|_| "bad prompt text".to_owned())?;
      let mut ctx: *mut c_void = std::ptr::null_mut();
      match unsafe { tmgr_presence_evaluate(reason.as_ptr(), &mut ctx) } {
        0 if !ctx.is_null() => Ok(ctx as usize),
        CANCELLED => Err("Touch ID was cancelled".into()),
        _ => Err("Touch ID did not confirm it is you".into()),
      }
    }

    fn sign(&self, ctx: usize, blob: &[u8], msg: &[u8]) -> Result<Vec<u8>, String> {
      let mut der = vec![0u8; 128];
      let mut len = 0usize;
      let rc = unsafe { tmgr_presence_sign(ctx as *mut c_void, blob.as_ptr(), blob.len(), msg.as_ptr(), msg.len(), der.as_mut_ptr(), der.len(), &mut len) };
      if rc != 0 || len == 0 || len > der.len() {
        return Err("signing failed".into());
      }
      der.truncate(len);
      Ok(der)
    }

    fn release(&self, ctx: usize) {
      if ctx != 0 {
        unsafe { tmgr_presence_release(ctx as *mut c_void) }
      }
    }

    fn epoch(&self) -> u64 {
      unsafe { tmgr_presence_epoch() }
    }
  }
}

#[cfg(test)]
pub mod soft {
  use std::collections::BTreeSet;
  use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
  use std::sync::{mpsc, Arc, Mutex};

  use p256::ecdsa::signature::Signer;
  use p256::ecdsa::{Signature, SigningKey};

  /// A software stand-in: the blob is the secret key, and `broken` blobs no longer open.
  #[derive(Clone, Default)]
  pub struct SoftEnclave(pub Arc<SoftState>);

  #[derive(Default)]
  pub struct SoftState {
    pub prompts: AtomicUsize,
    pub refuse: Mutex<Option<String>>,
    pub epoch: AtomicU64,
    pub broken: Mutex<bool>,
    pub high_s: Mutex<bool>,
    pub lock_while_signing: AtomicBool,
    pub live: Mutex<BTreeSet<usize>>,
    pub hold: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>,
    next: AtomicUsize,
  }

  impl SoftEnclave {
    pub fn prompts(&self) -> usize {
      self.0.prompts.load(Ordering::SeqCst)
    }
    pub fn live(&self) -> usize {
      self.0.live.lock().unwrap().len()
    }
  }

  impl super::Enclave for SoftEnclave {
    fn available(&self) -> bool {
      true
    }
    fn create(&self) -> Result<(Vec<u8>, Vec<u8>), String> {
      let mut secret = [0u8; 32];
      getrandom::fill(&mut secret).unwrap();
      let key = SigningKey::from_slice(&secret).unwrap();
      *self.0.broken.lock().unwrap() = false;
      Ok((secret.to_vec(), key.verifying_key().to_encoded_point(false).as_bytes().to_vec()))
    }
    fn public_key(&self, blob: &[u8]) -> Option<Vec<u8>> {
      if *self.0.broken.lock().unwrap() {
        return None;
      }
      let key = SigningKey::from_slice(blob).ok()?;
      Some(key.verifying_key().to_encoded_point(false).as_bytes().to_vec())
    }
    fn authenticate(&self, _reason: &str) -> Result<usize, String> {
      self.0.prompts.fetch_add(1, Ordering::SeqCst);
      if let Some((entered, release)) = self.0.hold.lock().unwrap().take() {
        entered.send(()).unwrap();
        release.recv().unwrap();
      }
      if let Some(e) = self.0.refuse.lock().unwrap().clone() {
        return Err(e);
      }
      let ctx = self.0.next.fetch_add(1, Ordering::SeqCst) + 1;
      self.0.live.lock().unwrap().insert(ctx);
      Ok(ctx)
    }
    fn sign(&self, ctx: usize, blob: &[u8], msg: &[u8]) -> Result<Vec<u8>, String> {
      if !self.0.live.lock().unwrap().contains(&ctx) {
        return Err("context invalidated".into());
      }
      let key = SigningKey::from_slice(blob).map_err(|_| "bad key".to_owned())?;
      let sig: Signature = key.sign(msg);
      if self.0.lock_while_signing.load(Ordering::SeqCst) {
        self.0.epoch.fetch_add(1, Ordering::SeqCst);
      }
      let sig = sig.normalize_s().unwrap_or(sig);
      let sig = if *self.0.high_s.lock().unwrap() { Signature::from_scalars(sig.r(), -*sig.s()).unwrap() } else { sig };
      Ok(sig.to_der().as_bytes().to_vec())
    }
    fn release(&self, ctx: usize) {
      self.0.live.lock().unwrap().remove(&ctx);
    }
    fn epoch(&self) -> u64 {
      self.0.epoch.load(Ordering::SeqCst)
    }
  }
}

#[cfg(test)]
mod tests {
  use std::sync::atomic::Ordering;
  use std::sync::{Arc, Mutex};

  use super::soft::SoftEnclave;
  use super::*;

  struct Rig {
    p: Presence,
    e: SoftEnclave,
    clock: Arc<Mutex<(Instant, u64)>>,
    _dir: tempfile::TempDir,
  }

  fn rig() -> Rig {
    let dir = tempfile::tempdir().unwrap();
    let e = SoftEnclave::default();
    let clock = Arc::new(Mutex::new((Instant::now(), 1_000_000u64)));
    let c = clock.clone();
    let p = Presence::new(Box::new(e.clone()), dir.path().to_path_buf(), Box::new(move || *c.lock().unwrap()));
    Rig { p, e, clock, _dir: dir }
  }

  impl Rig {
    fn tick(&self, mono: Duration, wall_ms: i64) {
      let mut c = self.clock.lock().unwrap();
      c.0 += mono;
      c.1 = (c.1 as i64 + wall_ms) as u64;
    }
  }

  fn verifies(public: &[u8], msg: &str, b64: &str) -> bool {
    let der = tmgr_phone::b64_decode(b64).unwrap();
    let sig = Signature::from_der(&der).unwrap();
    sig.normalize_s().is_none() && VerifyingKey::from_sec1_bytes(public).unwrap().verify(msg.as_bytes(), &sig).is_ok()
  }

  #[test]
  fn nothing_signs_before_setup() {
    let r = rig();
    assert_eq!(r.p.public_key().unwrap(), None);
    assert_eq!(r.p.unlock("x").unwrap_err(), NOT_SET_UP);
    assert_eq!(r.p.sign_unlocked("m").unwrap_err(), NOT_SET_UP);
    assert_eq!(r.e.prompts(), 0);
    assert!(!r.p.status().set_up);
  }

  #[test]
  fn setup_stores_a_private_key_file_and_reports_its_fingerprint() {
    let r = rig();
    let public = r.p.setup().unwrap();
    assert_eq!(r.p.public_key().unwrap().as_deref(), Some(&public[..]));
    #[cfg(unix)]
    {
      use std::os::unix::fs::PermissionsExt;
      assert_eq!(fs::metadata(&r.p.path).unwrap().permissions().mode() & 0o777, 0o600);
    }
    let st = r.p.status();
    assert!(st.set_up && st.available && !st.needs_setup_again);
    assert_eq!(st.key_fp, Some(tmgr_phone::hex(&Sha256::digest(&public))));
    assert_eq!(st.unlocked_secs, None);
  }

  #[test]
  fn one_prompt_covers_answers_inside_the_window() {
    let r = rig();
    let public = r.p.setup().unwrap();
    r.p.unlock("x").unwrap();
    let a = r.p.sign_unlocked("first").unwrap();
    r.tick(Duration::from_secs(300), 300_000);
    r.p.unlock("x").unwrap();
    let b = r.p.sign_unlocked("second").unwrap();
    assert_eq!(r.e.prompts(), 1);
    assert!(verifies(&public, "first", &a) && verifies(&public, "second", &b));
    assert_eq!(r.p.status().unlocked_secs, Some(300));
  }

  #[test]
  fn the_window_ends_after_ten_minutes_on_either_clock() {
    for (mono, wall) in [(600, 1), (1, 600_000), (1, -1)] {
      let r = rig();
      r.p.setup().unwrap();
      r.p.unlock("x").unwrap();
      r.tick(Duration::from_secs(mono), wall);
      assert_eq!(r.p.sign_unlocked("m").unwrap_err(), EXPIRED, "{mono} {wall}");
      assert_eq!(r.e.live(), 0);
      assert_eq!(r.p.status().unlocked_secs, None);
      r.p.unlock("x").unwrap();
      assert_eq!(r.e.prompts(), 2);
      r.p.sign_unlocked("m").unwrap();
    }
  }

  #[test]
  fn sleep_or_screen_lock_ends_the_window() {
    let r = rig();
    r.p.setup().unwrap();
    r.p.unlock("x").unwrap();
    r.e.0.epoch.fetch_add(1, Ordering::SeqCst);
    assert_eq!(r.p.sign_unlocked("m").unwrap_err(), EXPIRED);
    r.p.unlock("x").unwrap();
    assert_eq!(r.e.prompts(), 2);
    r.p.sign_unlocked("m").unwrap();
  }

  #[test]
  fn a_screen_lock_during_signing_discards_the_signature() {
    let r = rig();
    r.p.setup().unwrap();
    r.p.unlock("x").unwrap();
    r.e.0.lock_while_signing.store(true, Ordering::SeqCst);
    assert_eq!(r.p.sign_unlocked("m").unwrap_err(), EXPIRED);
    assert_eq!(r.e.live(), 0);
  }

  #[test]
  fn a_cancelled_prompt_opens_no_window() {
    let r = rig();
    r.p.setup().unwrap();
    *r.e.0.refuse.lock().unwrap() = Some("Touch ID was cancelled".into());
    assert_eq!(r.p.unlock("x").unwrap_err(), "Touch ID was cancelled");
    assert_eq!(r.p.sign_unlocked("m").unwrap_err(), EXPIRED);
    *r.e.0.refuse.lock().unwrap() = None;
    r.p.unlock("x").unwrap();
    r.p.sign_unlocked("m").unwrap();
  }

  #[test]
  fn a_key_that_stopped_working_asks_for_setup_again() {
    let r = rig();
    r.p.setup().unwrap();
    r.p.unlock("x").unwrap();
    *r.e.0.broken.lock().unwrap() = true;
    assert_eq!(r.p.sign_unlocked("m").unwrap_err(), SETUP_AGAIN);
    assert_eq!(r.p.unlock("x").unwrap_err(), SETUP_AGAIN);
    let st = r.p.status();
    assert!(!st.set_up && st.needs_setup_again);
    r.p.setup().unwrap();
    assert_eq!(r.e.live(), 0);
    r.p.unlock("x").unwrap();
    r.p.sign_unlocked("m").unwrap();
  }

  #[test]
  fn a_context_the_enclave_dropped_closes_the_window_without_a_prompt() {
    let r = rig();
    r.p.setup().unwrap();
    r.p.unlock("x").unwrap();
    r.e.0.live.lock().unwrap().clear();
    assert_eq!(r.p.sign_unlocked("m").unwrap_err(), EXPIRED);
    assert_eq!(r.e.prompts(), 1);
    assert_eq!(r.p.status().unlocked_secs, None);
  }

  #[test]
  fn a_high_s_signature_is_normalized_and_checked() {
    let r = rig();
    let public = r.p.setup().unwrap();
    *r.e.0.high_s.lock().unwrap() = true;
    r.p.unlock("x").unwrap();
    assert!(verifies(&public, "m", &r.p.sign_unlocked("m").unwrap()));
  }

  #[test]
  fn lock_now_releases_the_context() {
    let r = rig();
    r.p.setup().unwrap();
    r.p.unlock("x").unwrap();
    assert_eq!(r.e.live(), 1);
    r.p.lock_now();
    assert_eq!(r.e.live(), 0);
    assert_eq!(r.p.sign_unlocked("m").unwrap_err(), EXPIRED);
  }
}
