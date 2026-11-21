//! Swaps the relay between "not set up" and a live engine as the web UI signs in and out, and polls the live one.

use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::Duration;

use super::auth::{AuthCell, BoundLogin, RelayAuth};
use super::engine::{retry_after_secs, EngineConfig, RelayEngine};
use super::http::{HttpTransport, LoginToken};
use super::port::{NoRelay, RelayPort, TrustState};
use super::ports::{KeyStore, KeychainStore};
use crate::manager::backend::{ChatMessage, HistoryParams, RoomInfo, RoomRef, SearchParams, SendParams};

const LABEL: &str = "Desktop";
const CADENCE: Duration = Duration::from_secs(5);
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
pub type MachineSource = Box<dyn Fn() -> Result<String, String> + Send + Sync>;

pub fn build_engine(spec: EngineSpec, store: impl KeyStore + 'static) -> Result<Arc<dyn RelayPort>, String> {
  let transport = HttpTransport::new(&spec.api_base, spec.login)?;
  let cfg = EngineConfig { user_id: spec.user_id, label: LABEL.to_owned(), local_machine: spec.local_machine };
  Ok(Arc::new(RelayEngine::new(transport, store, cfg)?))
}

pub fn keychain_builder() -> Builder {
  Box::new(|spec| {
    let store = KeychainStore::new(format!("u{}@{}", spec.user_id, spec.api_base));
    build_engine(spec, store)
  })
}

struct Slot {
  relay: Arc<dyn RelayPort>,
  built_for: Option<(u64, String)>,
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
      slot: RwLock::new(Slot { relay: Arc::new(NoRelay), built_for: None }),
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

  fn put(&self, relay: Arc<dyn RelayPort>, built_for: Option<(u64, String)>) {
    *self.slot.write().unwrap_or_else(|p| p.into_inner()) = Slot { relay, built_for };
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
    self.put(Arc::new(NoRelay), None);
  }

  pub fn ensure_engine(&self) -> bool {
    let mut gate = self.gate.lock().unwrap_or_else(|p| p.into_inner());
    self.ensure_locked(&mut gate)
  }

  fn ensure_locked(&self, logged: &mut bool) -> bool {
    if !self.enabled {
      return false;
    }
    let Some(key) = self.auth.key() else { return false };
    if self.slot.read().unwrap_or_else(|p| p.into_inner()).built_for.as_ref() == Some(&key) {
      return true;
    }
    self.put(Arc::new(NoRelay), None);
    let built = (self.machine)().and_then(|local_machine| {
      (self.build)(EngineSpec { user_id: key.0, api_base: key.1.clone(), local_machine, login: Arc::new(BoundLogin::new(self.auth.clone(), key.clone())) })
    });
    match built {
      Ok(relay) => {
        *logged = false;
        self.put(relay, Some(key));
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
    self.cur().send(params)
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
  fn decide_revoke(&self, statement_id: &str, accept: bool) -> Result<(), String> {
    self.cur().decide_revoke(statement_id, accept)
  }
  fn reset_identity(&self) -> Result<(), String> {
    self.cur().reset_identity()
  }
  fn poll_once(&self) -> Result<Vec<ChatMessage>, String> {
    self.cur().poll_once()
  }
}

pub trait Pollee {
  fn alive(&self) -> bool;
  fn active(&self) -> bool;
  fn poll(&self) -> Result<Vec<ChatMessage>, String>;
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
}

fn retry_after_of(e: &str) -> Option<u64> {
  retry_after_secs(e).or_else(|| serde_json::from_str::<serde_json::Value>(e).ok()?["retry_after"].as_u64())
}

pub fn run_poll(src: &dyn Pollee, emit: &dyn Fn(ChatMessage), sleep: &dyn Fn(Duration)) {
  let mut backoff = BACKOFF_START;
  while src.alive() {
    if !src.active() {
      backoff = BACKOFF_START;
      sleep(CADENCE);
      continue;
    }
    match src.poll() {
      Ok(messages) => {
        backoff = BACKOFF_START;
        messages.into_iter().for_each(emit);
        sleep(CADENCE);
      }
      Err(e) => {
        let hinted = Duration::from_secs(retry_after_of(&e).unwrap_or(0).min(RETRY_AFTER_CAP));
        sleep(backoff.max(hinted));
        backoff = (backoff * 2).min(BACKOFF_CAP);
      }
    }
  }
}

pub fn spawn_poll(switch: &Arc<SwitchRelay>, emit: impl Fn(ChatMessage) + Send + 'static) {
  let weak = Arc::downgrade(switch);
  let spawned = std::thread::Builder::new().name("manager-relay-poll".into()).spawn(move || run_poll(&weak, &emit, &std::thread::sleep));
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
    Box::new(|| Ok("mx_a".to_owned()))
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

  #[test]
  fn an_unknown_machine_leaves_no_relay_and_builds_later() {
    let count = Arc::new(AtomicUsize::new(0));
    let ready = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = ready.clone();
    let machine: MachineSource = Box::new(move || if flag.load(Ordering::SeqCst) { Ok("mx_a".into()) } else { Err("no runner".into()) });
    let s = SwitchRelay::new(true, live_builder(count.clone()), machine);
    s.set_auth(auth(7));
    assert!(is_no_relay(&s));
    assert!(!s.ensure_engine());
    ready.store(true, Ordering::SeqCst);
    assert!(s.ensure_engine());
    assert!(!is_no_relay(&s));
    assert_eq!(count.load(Ordering::SeqCst), 1);
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
      build_engine(spec(&format!("https://u:{SECRET}@api.example.com"), SECRET), MemKeyStore::default()),
      build_engine(spec(&format!("http://{SECRET}.example.com"), SECRET), MemKeyStore::default()),
      build_engine(spec("https://api.tmgr.dev", SECRET), BrokenStore),
    ];
    for r in bad {
      let e = r.err().expect("build must fail");
      assert!(!e.contains(SECRET), "{e}");
    }
    assert!(build_engine(spec("https://api.tmgr.dev", SECRET), MemKeyStore::default()).is_ok());
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
    }
  }

  struct Fake {
    ticks: Cell<usize>,
    active: Cell<bool>,
    polls: Cell<usize>,
    script: RefCell<Vec<Result<Vec<ChatMessage>, String>>>,
  }

  impl Fake {
    fn new(ticks: usize, active: bool, script: Vec<Result<Vec<ChatMessage>, String>>) -> Self {
      Self { ticks: Cell::new(ticks), active: Cell::new(active), polls: Cell::new(0), script: RefCell::new(script) }
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
  }

  fn run(fake: &Fake) -> (Vec<Duration>, Vec<String>) {
    let sleeps = RefCell::new(vec![]);
    let emitted = RefCell::new(vec![]);
    run_poll(fake, &|m| emitted.borrow_mut().push(m.id), &|d| sleeps.borrow_mut().push(d));
    (sleeps.into_inner(), emitted.into_inner())
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
    run_poll(&weak, &|_| panic!("no emit"), &|_| panic!("no sleep"));
  }
}
