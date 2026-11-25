//! Swaps the relay between "not set up" and a live engine as the web UI signs in and out, and polls the live one.

use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::{Duration, Instant};

use super::auth::{AuthCell, BoundLogin, RelayAuth};
use super::engine::{retry_after_secs, EngineConfig, RelayEngine};
use super::http::{HttpTransport, LoginToken};
use super::port::{NoRelay, Receipt, RelayPort, TrustState};
use super::ports::{KeyStore, KeychainStore};
use super::super::presence::Presence;
use crate::manager::backend::{ChatMessage, HistoryParams, RoomInfo, RoomRef, SearchParams, SendParams};

const LABEL: &str = "Desktop";
const CADENCE: Duration = Duration::from_secs(5);
const CADENCE_VISIBLE: Duration = Duration::from_secs(1);
const WARN_EVERY: Duration = Duration::from_secs(60);
const BACKOFF_START: Duration = Duration::from_secs(5);
const BACKOFF_CAP: Duration = Duration::from_secs(300);
const RETRY_AFTER_CAP: u64 = 3600;

pub struct EngineSpec {
  pub user_id: u64,
  pub api_base: String,
  pub local_machine: String,
  pub login: Arc<dyn LoginToken>,
}

pub type Builder = Box<dyn Fn(EngineSpec) -> Result<Arc<dyn RelayPort>, String> + Send + Sync>;
pub type MachineSource = Box<dyn Fn() -> Result<Option<String>, String> + Send + Sync>;

pub fn build_engine(spec: EngineSpec, store: impl KeyStore + 'static, presence: Option<Arc<Presence>>) -> Result<Arc<dyn RelayPort>, String> {
  let transport = HttpTransport::new(&spec.api_base, spec.login)?;
  let cfg = EngineConfig { user_id: spec.user_id, label: LABEL.to_owned(), local_machine: spec.local_machine };
  Ok(Arc::new(RelayEngine::new(transport, store, cfg)?.with_presence(presence)))
}

pub fn keychain_builder(presence: Option<Arc<Presence>>) -> Builder {
  Box::new(move |spec| {
    let store = KeychainStore::new(format!("u{}@{}", spec.user_id, spec.api_base));
    build_engine(spec, store, presence.clone())
  })
}

struct Slot {
  relay: Arc<dyn RelayPort>,
  built_for: Option<(u64, String)>,
  machine_known: bool,
}

pub struct SwitchRelay {
  enabled: bool,
  auth: Arc<AuthCell>,
  slot: RwLock<Slot>,
  gate: Mutex<bool>,
  build: Builder,
  machine: MachineSource,
}

impl SwitchRelay {
  pub fn new(enabled: bool, build: Builder, machine: MachineSource) -> Self {
    Self {
      enabled,
      auth: Arc::new(AuthCell::default()),
      slot: RwLock::new(Slot { relay: Arc::new(NoRelay), built_for: None, machine_known: false }),
      gate: Mutex::new(false),
      build,
      machine,
    }
  }

  pub fn enabled(&self) -> bool {
    self.enabled
  }

  fn cur(&self) -> Arc<dyn RelayPort> {
    self.slot.read().unwrap_or_else(|p| p.into_inner()).relay.clone()
  }

  fn put(&self, relay: Arc<dyn RelayPort>, built_for: Option<(u64, String)>, machine_known: bool) {
    *self.slot.write().unwrap_or_else(|p| p.into_inner()) = Slot { relay, built_for, machine_known };
  }

  #[cfg(test)]
  pub fn is_active(&self) -> bool {
    self.slot.read().unwrap_or_else(|p| p.into_inner()).built_for.is_some()
  }

  pub fn set_auth(&self, auth: RelayAuth) {
    let mut gate = self.gate.lock().unwrap_or_else(|p| p.into_inner());
    self.auth.set(auth);
    self.ensure_locked(&mut gate);
  }

  pub fn clear(&self) {
    let _gate = self.gate.lock().unwrap_or_else(|p| p.into_inner());
    self.auth.clear();
    self.put(Arc::new(NoRelay), None, false);
  }

  pub fn ensure_engine(&self) -> bool {
    let mut gate = self.gate.lock().unwrap_or_else(|p| p.into_inner());
    self.ensure_locked(&mut gate)
  }

  fn ensure_locked(&self, logged: &mut bool) -> bool {
    if !self.enabled {
      return false;
    }
    let Some(key) = self.auth.key() else {
      return false;
    };
    let (built, machine_known) = {
      let slot = self.slot.read().unwrap_or_else(|p| p.into_inner());
      (slot.built_for.as_ref() == Some(&key), slot.machine_known)
    };
    if built {
      if !machine_known {
        if let Ok(Some(machine)) = (self.machine)() {
          self.cur().set_local_machine(&machine);
          self.slot.write().unwrap_or_else(|p| p.into_inner()).machine_known = true;
        }
      }
      return true;
    }
    self.put(Arc::new(NoRelay), None, false);
    let machine = match (self.machine)() {
      Ok(machine) => machine,
      Err(e) => {
        if !*logged {
          log::warn!("[manager] chat relay unavailable: {e}");
          *logged = true;
        }
        return false;
      }
    };
    let known = machine.is_some();
    let spec = EngineSpec { user_id: key.0, api_base: key.1.clone(), local_machine: machine.unwrap_or_default(), login: Arc::new(BoundLogin::new(self.auth.clone(), key.clone())) };
    match (self.build)(spec) {
      Ok(relay) => {
        *logged = false;
        self.put(relay, Some(key), known);
        true
      }
      Err(e) => {
        if !*logged {
          log::warn!("[manager] chat relay unavailable: {e}");
          *logged = true;
        }
        false
      }
    }
  }
}

impl RelayPort for SwitchRelay {
  fn rooms(&self) -> Result<Vec<RoomInfo>, String> {
    self.cur().rooms()
  }
  fn history(&self, params: &HistoryParams) -> Result<Vec<ChatMessage>, String> {
    self.cur().history(params)
  }
  fn search(&self, params: &SearchParams) -> Result<Vec<ChatMessage>, String> {
    self.cur().search(params)
  }
  fn send(&self, params: SendParams) -> Result<ChatMessage, String> {
    let relay = self.cur();
    let sent = relay.send(params);
    if sent.is_ok() {
      let spawned = std::thread::Builder::new().name("manager-relay-flush".into()).spawn(move || relay.flush());
      if let Err(e) = spawned {
        log::warn!("[manager] chat store flush did not start: {e}");
      }
    }
    sent
  }
  fn owns(&self, room: &RoomRef) -> bool {
    self.cur().owns(room)
  }
  fn trust_state(&self) -> Result<TrustState, String> {
    self.cur().trust_state()
  }
  fn decide_join(&self, device_id: &str, accept: bool) -> Result<(), String> {
    self.cur().decide_join(device_id, accept)
  }
  fn revoke(&self, device_id: &str) -> Result<(), String> {
    self.cur().revoke(device_id)
  }
  fn confirm_device_kind(&self, device_id: &str, kind: &str) -> Result<(), String> {
    self.cur().confirm_device_kind(device_id, kind)
  }
  fn decide_revoke(&self, statement_id: &str, accept: bool) -> Result<(), String> {
    self.cur().decide_revoke(statement_id, accept)
  }
  fn open_join_code(&self) -> Result<String, String> {
    self.cur().open_join_code()
  }
  fn join_with_code(&self, code: &str) -> Result<(), String> {
    self.cur().join_with_code(code)
  }
  fn close_join_code(&self, expires_at_ms: u64) -> Result<(), String> {
    self.cur().close_join_code(expires_at_ms)
  }
  fn expect_join(&self, device_id: &str, fp: &str) -> Result<(), String> {
    self.cur().expect_join(device_id, fp)
  }
  fn machine_indexes(&self) -> Vec<super::rooms::MachineIndex> {
    self.cur().machine_indexes()
  }
  fn set_local_machine(&self, machine: &str) {
    self.cur().set_local_machine(machine)
  }
  fn reset_identity(&self) -> Result<(), String> {
    self.cur().reset_identity()
  }
  fn answer_permit(&self, card_id: &str, answer: super::permit::PermitAnswer, confirmed: bool) -> Result<(), String> {
    self.cur().answer_permit(card_id, answer, confirmed)
  }
  fn refresh_permit(&self, card_id: &str) -> Result<(), String> {
    self.cur().refresh_permit(card_id)
  }
  fn publish_presence(&self) -> Result<usize, String> {
    self.cur().publish_presence()
  }
  fn poll_once(&self) -> Result<Vec<ChatMessage>, String> {
    self.cur().poll_once()
  }
  fn take_receipts(&self) -> Vec<Receipt> {
    self.cur().take_receipts()
  }
  fn flush(&self) {
    self.cur().flush()
  }
  fn poll_with_receipts(&self, on_receipt: &dyn Fn(Receipt)) -> Result<Vec<ChatMessage>, String> {
    self.cur().poll_with_receipts(on_receipt)
  }
}

pub trait Pollee {
  fn alive(&self) -> bool;
  fn active(&self) -> bool;
  fn poll(&self) -> Result<Vec<ChatMessage>, String>;
  fn take_receipts(&self) -> Vec<Receipt> {
    vec![]
  }
  fn poll_with_receipts(&self, _on_receipt: &dyn Fn(Receipt)) -> Result<Vec<ChatMessage>, String> {
    self.poll()
  }
}

impl Pollee for Weak<SwitchRelay> {
  fn alive(&self) -> bool {
    self.strong_count() > 0
  }
  fn active(&self) -> bool {
    self.upgrade().is_some_and(|s| s.ensure_engine())
  }
  fn poll(&self) -> Result<Vec<ChatMessage>, String> {
    match self.upgrade() {
      Some(s) => s.poll_once(),
      None => Ok(vec![]),
    }
  }
  fn take_receipts(&self) -> Vec<Receipt> {
    self.upgrade().map(|s| s.take_receipts()).unwrap_or_default()
  }
  fn poll_with_receipts(&self, on_receipt: &dyn Fn(Receipt)) -> Result<Vec<ChatMessage>, String> {
    match self.upgrade() {
      Some(s) => s.poll_with_receipts(on_receipt),
      None => Ok(vec![]),
    }
  }
}

fn retry_after_of(e: &str) -> Option<u64> {
  retry_after_secs(e).or_else(|| serde_json::from_str::<serde_json::Value>(e).ok()?["retry_after"].as_u64())
}

pub fn cadence(visible: bool) -> Duration {
  if visible {
    CADENCE_VISIBLE
  } else {
    CADENCE
  }
}

#[derive(Default)]
struct WarnGate(Option<(String, Instant)>);

impl WarnGate {
  fn allow(&mut self, message: &str, now: Instant) -> bool {
    if self.0.as_ref().is_some_and(|(m, at)| m == message && now.saturating_duration_since(*at) < WARN_EVERY) {
      return false;
    }
    self.0 = Some((message.to_owned(), now));
    true
  }
}

pub fn run_poll(src: &dyn Pollee, emit: &dyn Fn(ChatMessage), emit_receipt: &dyn Fn(Receipt), visible: &dyn Fn() -> bool, sleep: &dyn Fn(Duration)) {
  let mut backoff = BACKOFF_START;
  let mut gate = WarnGate::default();
  while src.alive() {
    if !src.active() {
      backoff = BACKOFF_START;
      sleep(cadence(visible()));
      continue;
    }
    let polled = src.poll_with_receipts(emit_receipt);
    src.take_receipts().into_iter().for_each(emit_receipt);
    match polled {
      Ok(messages) => {
        backoff = BACKOFF_START;
        messages.into_iter().for_each(emit);
        sleep(cadence(visible()));
      }
      Err(e) => {
        if gate.allow(&e, Instant::now()) {
          log::warn!("[manager] relay poll failed: {}", e.chars().take(200).collect::<String>());
        }
        let hinted = Duration::from_secs(retry_after_of(&e).unwrap_or(0).min(RETRY_AFTER_CAP));
        sleep(backoff.max(hinted));
        backoff = (backoff * 2).min(BACKOFF_CAP);
      }
    }
  }
}

struct TrustWatch<P, S, N> {
  inner: P,
  snapshot: S,
  notify: N,
  last: Mutex<Option<TrustState>>,
}

impl<P: Pollee, S: Fn() -> Option<TrustState>, N: Fn()> Pollee for TrustWatch<P, S, N> {
  fn alive(&self) -> bool {
    self.inner.alive()
  }
  fn active(&self) -> bool {
    self.inner.active()
  }
  fn poll(&self) -> Result<Vec<ChatMessage>, String> {
    self.after(self.inner.poll())
  }
  fn poll_with_receipts(&self, on_receipt: &dyn Fn(Receipt)) -> Result<Vec<ChatMessage>, String> {
    self.after(self.inner.poll_with_receipts(on_receipt))
  }
  fn take_receipts(&self) -> Vec<Receipt> {
    self.inner.take_receipts()
  }
}

impl<P, S: Fn() -> Option<TrustState>, N: Fn()> TrustWatch<P, S, N> {
  fn after(&self, polled: Result<Vec<ChatMessage>, String>) -> Result<Vec<ChatMessage>, String> {
    if let Some(now) = (self.snapshot)() {
      let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
      if last.as_ref() != Some(&now) {
        *last = Some(now);
        drop(last);
        (self.notify)();
      }
    }
    polled
  }
}

pub fn spawn_poll(
  switch: &Arc<SwitchRelay>,
  emit: impl Fn(ChatMessage) + Send + 'static,
  emit_receipt: impl Fn(Receipt) + Send + 'static,
  on_trust: impl Fn() + Send + 'static,
  visible: impl Fn() -> bool + Send + 'static,
) {
  let weak = Arc::downgrade(switch);
  let snap = weak.clone();
  let watch = TrustWatch { inner: weak, snapshot: move || snap.upgrade()?.trust_state().ok(), notify: on_trust, last: Mutex::new(None) };
  let spawned = std::thread::Builder::new().name("manager-relay-poll".into()).spawn(move || run_poll(&watch, &emit, &emit_receipt, &visible, &std::thread::sleep));
  if let Err(e) = spawned {
    log::error!("[manager] relay polling did not start: {e}");
  }
}

#[cfg(test)]
mod tests {
  use std::cell::{Cell, RefCell};
  use std::sync::atomic::{AtomicUsize, Ordering};

  use super::*;
  use crate::manager::backend::{Author, RoomKind};
  use crate::manager::relay::ports::MemKeyStore;

  const SECRET: &str = "tok_SECRET_123";
  const NOT_SET_UP: &str = "chat sync is not set up";

  struct Live;

  impl RelayPort for Live {
    fn rooms(&self) -> Result<Vec<RoomInfo>, String> {
      Ok(vec![])
    }
    fn history(&self, _: &HistoryParams) -> Result<Vec<ChatMessage>, String> {
      Ok(vec![])
    }
    fn search(&self, _: &SearchParams) -> Result<Vec<ChatMessage>, String> {
      Ok(vec![])
    }
    fn send(&self, _: SendParams) -> Result<ChatMessage, String> {
      Err("live".into())
    }
    fn owns(&self, _: &RoomRef) -> bool {
      true
    }
    fn trust_state(&self) -> Result<TrustState, String> {
      Ok(TrustState::default())
    }
    fn decide_join(&self, _: &str, _: bool) -> Result<(), String> {
      Ok(())
    }
    fn revoke(&self, _: &str) -> Result<(), String> {
      Ok(())
    }
    fn reset_identity(&self) -> Result<(), String> {
      Ok(())
    }
    fn poll_once(&self) -> Result<Vec<ChatMessage>, String> {
      Ok(vec![])
    }
  }

  fn auth(user: u64) -> RelayAuth {
    RelayAuth::new(SECRET.into(), user, "https://api.tmgr.dev/api/").unwrap()
  }

  fn live_builder(count: Arc<AtomicUsize>) -> Builder {
    Box::new(move |_| {
      count.fetch_add(1, Ordering::SeqCst);
      Ok(Arc::new(Live))
    })
  }

  fn machine_ok() -> MachineSource {
    Box::new(|| Ok(Some("mx_a".to_owned())))
  }

  fn switch(enabled: bool, count: &Arc<AtomicUsize>) -> SwitchRelay {
    SwitchRelay::new(enabled, live_builder(count.clone()), machine_ok())
  }

  fn is_no_relay(s: &SwitchRelay) -> bool {
    s.reset_identity().is_err_and(|e| e == NOT_SET_UP)
  }

  #[test]
  fn without_auth_it_is_no_relay_and_never_builds() {
    let count = Arc::new(AtomicUsize::new(0));
    let s = switch(true, &count);
    assert!(is_no_relay(&s));
    assert!(!s.ensure_engine());
    assert_eq!(count.load(Ordering::SeqCst), 0);
  }

  #[test]
  fn auth_swaps_in_the_engine_and_clear_swaps_it_out() {
    let count = Arc::new(AtomicUsize::new(0));
    let s = switch(true, &count);
    s.set_auth(auth(7));
    assert!(s.is_active());
    assert!(!is_no_relay(&s));
    s.clear();
    assert!(!s.is_active());
    assert!(is_no_relay(&s));
    assert!(!s.ensure_engine());
  }

  #[test]
  fn disabled_relay_stays_no_relay_even_with_auth() {
    let count = Arc::new(AtomicUsize::new(0));
    let s = switch(false, &count);
    s.set_auth(auth(7));
    assert!(is_no_relay(&s));
    assert!(!s.is_active());
    assert_eq!(count.load(Ordering::SeqCst), 0);
  }

  #[test]
  fn a_refreshed_token_does_not_rebuild_but_another_user_does() {
    let count = Arc::new(AtomicUsize::new(0));
    let s = switch(true, &count);
    s.set_auth(auth(7));
    s.set_auth(RelayAuth::new("fresh".into(), 7, "https://api.tmgr.dev").unwrap());
    assert_eq!(count.load(Ordering::SeqCst), 1);
    s.set_auth(auth(8));
    assert_eq!(count.load(Ordering::SeqCst), 2);
  }

  #[test]
  fn an_engine_built_for_the_old_account_never_sends_the_new_token() {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    let build: Builder = Box::new(move |spec| {
      sink.lock().unwrap().push(spec.login.clone());
      Ok(Arc::new(Live))
    });
    let s = SwitchRelay::new(true, build, machine_ok());
    s.set_auth(RelayAuth::new("old".into(), 7, "https://api.tmgr.dev").unwrap());
    s.set_auth(RelayAuth::new("new".into(), 8, "https://api.tmgr.dev").unwrap());
    let logins = seen.lock().unwrap();
    assert_eq!(logins.len(), 2);
    assert_eq!(logins[0].token(), None);
    assert_eq!(logins[1].token().as_deref(), Some("new"));
  }

  #[test]
  fn builder_sees_the_live_token_and_local_machine() {
    let seen = Arc::new(Mutex::new(None));
    let sink = seen.clone();
    let build: Builder = Box::new(move |spec| {
      *sink.lock().unwrap() = Some((spec.user_id, spec.api_base.clone(), spec.local_machine.clone(), spec.login.clone()));
      Ok(Arc::new(Live))
    });
    let s = SwitchRelay::new(true, build, machine_ok());
    s.set_auth(auth(7));
    let (user, base, machine, login) = seen.lock().unwrap().take().unwrap();
    assert_eq!((user, base.as_str(), machine.as_str()), (7, "https://api.tmgr.dev", "mx_a"));
    assert_eq!(login.token().as_deref(), Some(SECRET));
    s.set_auth(RelayAuth::new("fresh".into(), 7, "https://api.tmgr.dev").unwrap());
    assert_eq!(login.token().as_deref(), Some("fresh"));
    s.clear();
    assert_eq!(login.token(), None);
  }

  struct Machined(Arc<Mutex<Vec<String>>>);

  impl RelayPort for Machined {
    fn rooms(&self) -> Result<Vec<RoomInfo>, String> {
      Ok(vec![])
    }
    fn history(&self, _: &HistoryParams) -> Result<Vec<ChatMessage>, String> {
      Ok(vec![])
    }
    fn search(&self, _: &SearchParams) -> Result<Vec<ChatMessage>, String> {
      Ok(vec![])
    }
    fn send(&self, _: SendParams) -> Result<ChatMessage, String> {
      Err("live".into())
    }
    fn owns(&self, _: &RoomRef) -> bool {
      true
    }
    fn trust_state(&self) -> Result<TrustState, String> {
      Ok(TrustState::default())
    }
    fn decide_join(&self, _: &str, _: bool) -> Result<(), String> {
      Ok(())
    }
    fn revoke(&self, _: &str) -> Result<(), String> {
      Ok(())
    }
    fn reset_identity(&self) -> Result<(), String> {
      Ok(())
    }
    fn open_join_code(&self) -> Result<String, String> {
      Ok("code".into())
    }
    fn set_local_machine(&self, machine: &str) {
      self.0.lock().unwrap().push(machine.to_owned());
    }
    fn poll_once(&self) -> Result<Vec<ChatMessage>, String> {
      Ok(vec![])
    }
  }

  #[test]
  fn a_runner_that_is_on_but_not_ready_leaves_no_relay_and_builds_later() {
    let count = Arc::new(AtomicUsize::new(0));
    let ready = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = ready.clone();
    let machine: MachineSource = Box::new(move || if flag.load(Ordering::SeqCst) { Ok(Some("mx_a".into())) } else { Err("no runner".into()) });
    let s = SwitchRelay::new(true, live_builder(count.clone()), machine);
    s.set_auth(auth(7));
    assert!(is_no_relay(&s));
    assert!(!s.ensure_engine());
    assert_eq!(count.load(Ordering::SeqCst), 0);
    ready.store(true, Ordering::SeqCst);
    assert!(s.ensure_engine());
    assert!(!is_no_relay(&s));
    assert_eq!(count.load(Ordering::SeqCst), 1);
  }

  #[test]
  fn with_the_runner_off_the_relay_still_builds_so_devices_can_pair() {
    let count = Arc::new(AtomicUsize::new(0));
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let ready = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = ready.clone();
    let machine: MachineSource = Box::new(move || Ok(flag.load(Ordering::SeqCst).then(|| "mx_a".to_owned())));
    let (c, sink) = (count.clone(), seen.clone());
    let build: Builder = Box::new(move |spec| {
      c.fetch_add(1, Ordering::SeqCst);
      sink.lock().unwrap().push(format!("built:{}", spec.local_machine));
      Ok(Arc::new(Machined(sink.clone())))
    });
    let s = SwitchRelay::new(true, build, machine);
    s.set_auth(auth(7));
    assert!(!is_no_relay(&s));
    assert!(s.ensure_engine());
    assert_eq!(s.open_join_code().unwrap(), "code");
    ready.store(true, Ordering::SeqCst);
    assert!(s.ensure_engine());
    assert!(s.ensure_engine());
    assert_eq!(count.load(Ordering::SeqCst), 1);
    assert_eq!(*seen.lock().unwrap(), vec!["built:".to_owned(), "mx_a".to_owned()]);
  }

  #[test]
  fn a_known_machine_is_passed_at_build_and_never_set_again() {
    let seen = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = seen.clone();
    let build: Builder = Box::new(move |spec| {
      sink.lock().unwrap().push(format!("built:{}", spec.local_machine));
      Ok(Arc::new(Machined(sink.clone())))
    });
    let s = SwitchRelay::new(true, build, machine_ok());
    s.set_auth(auth(7));
    assert!(s.ensure_engine());
    assert_eq!(*seen.lock().unwrap(), vec!["built:mx_a".to_owned()]);
  }

  #[test]
  fn build_failures_never_contain_the_token() {
    let spec = |base: &str, login: &str| EngineSpec {
      user_id: 7,
      api_base: base.to_owned(),
      local_machine: "mx".into(),
      login: Arc::new(TokenOnly(login.to_owned())),
    };
    let bad = [
      build_engine(spec(&format!("https://u:{SECRET}@api.example.com"), SECRET), MemKeyStore::default(), None),
      build_engine(spec(&format!("http://{SECRET}.example.com"), SECRET), MemKeyStore::default(), None),
      build_engine(spec("https://api.tmgr.dev", SECRET), BrokenStore, None),
    ];
    for r in bad {
      let e = r.err().expect("build must fail");
      assert!(!e.contains(SECRET), "{e}");
    }
    assert!(build_engine(spec("https://api.tmgr.dev", SECRET), MemKeyStore::default(), None).is_ok());
  }

  struct TokenOnly(String);

  impl LoginToken for TokenOnly {
    fn token(&self) -> Option<String> {
      Some(self.0.clone())
    }
  }

  struct BrokenStore;

  impl KeyStore for BrokenStore {
    fn load(&self, _: crate::manager::relay::ports::Slot) -> Result<Option<String>, String> {
      Err("the chat store is unreadable".into())
    }
    fn save(&self, _: crate::manager::relay::ports::Slot, _: &str) -> Result<(), String> {
      Err("the chat store is unreadable".into())
    }
    fn clear(&self) -> Result<(), String> {
      Ok(())
    }
  }

  fn msg(id: &str) -> ChatMessage {
    ChatMessage {
      v: 1,
      id: id.into(),
      room: RoomRef { kind: RoomKind::Manager, machine: "m".into(), session: None },
      author: Author { kind: "owner".into(), id: "o".into(), machine: None, agent: None },
      ts_ms: 1,
      kind: "text".into(),
      body_md: "x".into(),
      attachments: vec![],
      reply_to: None,
      decision: None,
      mention: None,
    }
  }

  struct Fake {
    ticks: Cell<usize>,
    active: Cell<bool>,
    polls: Cell<usize>,
    script: RefCell<Vec<Result<Vec<ChatMessage>, String>>>,
    receipts: RefCell<Vec<Receipt>>,
  }

  impl Fake {
    fn new(ticks: usize, active: bool, script: Vec<Result<Vec<ChatMessage>, String>>) -> Self {
      Self { ticks: Cell::new(ticks), active: Cell::new(active), polls: Cell::new(0), script: RefCell::new(script), receipts: RefCell::default() }
    }
  }

  impl Pollee for Fake {
    fn alive(&self) -> bool {
      let left = self.ticks.get();
      self.ticks.set(left.saturating_sub(1));
      left > 0
    }
    fn active(&self) -> bool {
      self.active.get()
    }
    fn poll(&self) -> Result<Vec<ChatMessage>, String> {
      self.polls.set(self.polls.get() + 1);
      let mut s = self.script.borrow_mut();
      if s.is_empty() { Ok(vec![]) } else { s.remove(0) }
    }
    fn take_receipts(&self) -> Vec<Receipt> {
      std::mem::take(&mut self.receipts.borrow_mut())
    }
  }

  #[test]
  fn cadence_is_one_second_while_the_manager_window_is_visible_and_five_otherwise() {
    assert_eq!(cadence(true), Duration::from_secs(1));
    assert_eq!(cadence(false), Duration::from_secs(5));
  }

  #[test]
  fn the_poll_sleeps_by_window_visibility() {
    let sleeps = RefCell::new(vec![]);
    let shown = Cell::new(true);
    let fake = Fake::new(3, true, vec![]);
    run_poll(&fake, &|_| {}, &|_| {}, &|| shown.replace(false), &|d| sleeps.borrow_mut().push(d));
    assert_eq!(sleeps.into_inner(), [Duration::from_secs(1), Duration::from_secs(5), Duration::from_secs(5)]);
  }

  #[test]
  fn a_repeated_poll_error_is_logged_once_a_minute() {
    let mut gate = WarnGate::default();
    let t0 = Instant::now();
    assert!(gate.allow("relay unreachable", t0));
    assert!(!gate.allow("relay unreachable", t0 + Duration::from_secs(30)));
    assert!(gate.allow("other", t0 + Duration::from_secs(31)));
    assert!(gate.allow("relay unreachable", t0 + Duration::from_secs(32)));
    assert!(!gate.allow("relay unreachable", t0 + Duration::from_secs(60)));
    assert!(gate.allow("relay unreachable", t0 + Duration::from_secs(93)));
  }

  struct Streaming(Fake);

  impl Pollee for Streaming {
    fn alive(&self) -> bool {
      self.0.alive()
    }
    fn active(&self) -> bool {
      true
    }
    fn poll(&self) -> Result<Vec<ChatMessage>, String> {
      unreachable!("the streaming poll is used")
    }
    fn poll_with_receipts(&self, on_receipt: &dyn Fn(Receipt)) -> Result<Vec<ChatMessage>, String> {
      on_receipt(Receipt { id: "m_1".into(), state: crate::manager::relay::port::ReceiptState::Queued, reason: None });
      Err("still polling".into())
    }
  }

  #[test]
  fn receipts_reach_the_ui_while_the_poll_is_still_running() {
    let got = RefCell::new(vec![]);
    run_poll(&Streaming(Fake::new(1, true, vec![])), &|_| {}, &|r| got.borrow_mut().push(r.id), &|| false, &|_| {});
    assert_eq!(got.into_inner(), ["m_1"]);
  }

  fn run(fake: &Fake) -> (Vec<Duration>, Vec<String>) {
    let sleeps = RefCell::new(vec![]);
    let emitted = RefCell::new(vec![]);
    run_poll(fake, &|m| emitted.borrow_mut().push(m.id), &|_| {}, &|| false, &|d| sleeps.borrow_mut().push(d));
    (sleeps.into_inner(), emitted.into_inner())
  }

  #[test]
  fn receipts_are_emitted_after_each_poll_even_when_the_poll_failed() {
    use crate::manager::relay::port::ReceiptState;
    let fake = Fake::new(2, true, vec![Err("down".into()), Ok(vec![])]);
    let receipt = Receipt { id: "m_1".into(), state: ReceiptState::Queued, reason: None };
    fake.receipts.borrow_mut().push(receipt.clone());
    let got = RefCell::new(vec![]);
    run_poll(&fake, &|_| {}, &|r| got.borrow_mut().push(r), &|| false, &|_| {});
    assert_eq!(got.into_inner(), [receipt]);
  }

  #[test]
  fn trust_watch_notifies_only_when_the_trust_state_changes() {
    let fake = Fake::new(4, true, vec![]);
    let states = RefCell::new(vec![TrustState::default(), TrustState::default(), TrustState { joining: true, ..TrustState::default() }]);
    let notified = Cell::new(0);
    let watch = TrustWatch {
      inner: fake,
      snapshot: || {
        let mut s = states.borrow_mut();
        if s.is_empty() { None } else { Some(s.remove(0)) }
      },
      notify: || notified.set(notified.get() + 1),
      last: Mutex::new(None),
    };
    for _ in 0..4 {
      watch.poll().unwrap();
    }
    assert_eq!(notified.get(), 2);
  }

  fn secs(v: &[u64]) -> Vec<Duration> {
    v.iter().map(|s| Duration::from_secs(*s)).collect()
  }

  #[test]
  fn errors_back_off_exponentially_up_to_the_cap_and_reset_on_success() {
    let mut script: Vec<_> = (0..9).map(|_| Err("relay unreachable".to_owned())).collect();
    script.push(Ok(vec![]));
    script.push(Err("relay unreachable".into()));
    let fake = Fake::new(11, true, script);
    let (sleeps, _) = run(&fake);
    assert_eq!(sleeps, secs(&[5, 10, 20, 40, 80, 160, 300, 300, 300, 5, 5]));
  }

  #[test]
  fn retry_after_sets_a_minimum_wait() {
    let fake = Fake::new(
      3,
      true,
      vec![Err("relay is busy, retry in 90s".into()), Err(r#"{"error":"rate_limited","retry_after":2}"#.into()), Err("relay is busy, retry in 9999999s".into())],
    );
    let (sleeps, _) = run(&fake);
    assert_eq!(sleeps, secs(&[90, 10, 3600]));
  }

  #[test]
  fn nothing_is_polled_while_the_relay_is_inactive() {
    let fake = Fake::new(3, false, vec![]);
    let (sleeps, emitted) = run(&fake);
    assert_eq!(fake.polls.get(), 0);
    assert_eq!(sleeps, secs(&[5, 5, 5]));
    assert!(emitted.is_empty());
  }

  #[test]
  fn each_new_message_is_emitted_in_order() {
    let fake = Fake::new(2, true, vec![Ok(vec![msg("a"), msg("b")]), Ok(vec![msg("c")])]);
    let (sleeps, emitted) = run(&fake);
    assert_eq!(emitted, ["a", "b", "c"]);
    assert_eq!(sleeps, secs(&[5, 5]));
  }

  #[test]
  fn polling_stops_after_clear_and_when_the_switch_is_dropped() {
    let count = Arc::new(AtomicUsize::new(0));
    let s = Arc::new(switch(true, &count));
    s.set_auth(auth(7));
    let weak = Arc::downgrade(&s);
    assert!(weak.alive() && weak.active());
    s.clear();
    assert!(!weak.active());
    drop(s);
    assert!(!weak.alive());
    run_poll(&weak, &|_| panic!("no emit"), &|_| panic!("no receipt"), &|| false, &|_| panic!("no sleep"));
  }
}
