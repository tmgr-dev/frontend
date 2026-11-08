use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};
use std::time::{Duration, Instant};

use tmgr_ipc::decisions::{drain_spool, CardIn, Event, Found, Inbox, LineComment, PatchIn, PendingDelivery};

use super::store::{AnswerIn, CardView, Created, MessageIn, PendingTaskComment, PhoneOutcome, PhoneReq, Recorded, Routing, Store};

pub enum Change {
  Created { title: String, body: String },
  Updated,
}

type Hit = (usize, Event);

pub type Sink = Box<dyn Fn(&Change, usize) + Send + Sync>;

const LISTEN_GRACE: Duration = Duration::from_secs(5);
const ALIVE_STEP: Duration = Duration::from_secs(1);

#[derive(Default)]
struct Listen {
  waiters: usize,
  last: Option<Instant>,
  idle_pending: bool,
  delivered: u64,
}

impl Listen {
  fn active(&self, grace: Duration) -> bool {
    self.waiters > 0 || self.last.is_some_and(|t| t.elapsed() < grace)
  }
}

pub struct Hub {
  store: Mutex<Store>,
  cv: Condvar,
  sink: Sink,
  spool: Option<PathBuf>,
  listen: Mutex<HashMap<String, Listen>>,
  grace: Duration,
}

struct Waiting<'a> {
  hub: &'a Arc<Hub>,
  id: String,
}

impl Drop for Waiting<'_> {
  fn drop(&mut self) {
    self.hub.leave(&self.id);
  }
}

fn body_of(card: &CardIn) -> String {
  format!("{} needs you ({})", card.source.agent, card.kind)
}

impl Hub {
  pub fn new(store: Store, sink: Sink) -> Hub {
    Hub {
      store: Mutex::new(store),
      cv: Condvar::new(),
      sink,
      spool: None,
      listen: Mutex::new(HashMap::new()),
      grace: LISTEN_GRACE,
    }
  }

  pub fn with_spool(mut self, dir: PathBuf) -> Hub {
    self.spool = Some(dir);
    self
  }

  #[cfg(test)]
  fn with_grace(mut self, grace: Duration) -> Hub {
    self.grace = grace;
    self
  }

  fn lock_listen(&self) -> MutexGuard<'_, HashMap<String, Listen>> {
    self.listen.lock().unwrap_or_else(|p| p.into_inner())
  }

  fn mark(&self, store: &Store, mut view: CardView) -> CardView {
    let (listening, upto) = self.lock_listen().get(&view.id).map_or((false, 0), |l| (l.active(self.grace), l.delivered));
    view.listening = listening;
    let seqs = store.message_event_seqs(&view.id);
    for (k, m) in view.messages.iter_mut().filter(|m| m.author == "owner").enumerate() {
      m.delivered = seqs.get(k).is_some_and(|s| *s <= upto);
    }
    view
  }

  fn deliver(&self, id: &str, seq: u64) -> bool {
    let mut map = self.lock_listen();
    let l = map.entry(id.to_owned()).or_default();
    let rose = seq > l.delivered;
    l.delivered = l.delivered.max(seq);
    rose
  }

  fn enter<'a>(self: &'a Arc<Self>, id: &str) -> Waiting<'a> {
    let was = {
      let mut map = self.lock_listen();
      let l = map.entry(id.to_owned()).or_default();
      let was = l.active(self.grace);
      l.waiters += 1;
      was
    };
    if !was {
      self.changed(Change::Updated);
    }
    Waiting { hub: self, id: id.to_owned() }
  }

  fn leave(self: &Arc<Self>, id: &str) {
    let spawn = {
      let mut map = self.lock_listen();
      let Some(l) = map.get_mut(id) else { return };
      l.waiters = l.waiters.saturating_sub(1);
      l.last = Some(Instant::now());
      let spawn = l.waiters == 0 && !l.idle_pending;
      l.idle_pending |= spawn;
      spawn
    };
    if spawn {
      let (weak, id, grace) = (Arc::downgrade(self), id.to_owned(), self.grace);
      std::thread::spawn(move || announce_idle(weak, id, grace));
    }
  }

  fn settle(&self, id: &str) -> Option<Duration> {
    let mut map = self.lock_listen();
    let l = map.get_mut(id)?;
    if l.waiters > 0 {
      l.idle_pending = false;
      return None;
    }
    let left = l.last.map_or(Duration::ZERO, |t| self.grace.saturating_sub(t.elapsed()));
    if !left.is_zero() {
      return Some(left);
    }
    l.last = None;
    l.idle_pending = false;
    drop(map);
    self.changed(Change::Updated);
    None
  }

  fn lock(&self) -> MutexGuard<'_, Store> {
    self.store.lock().unwrap_or_else(|p| p.into_inner())
  }

  fn changed(&self, change: Change) {
    let count = self.lock().open_count();
    self.cv.notify_all();
    (self.sink)(&change, count);
  }

  pub fn open_count(&self) -> usize {
    self.lock().open_count()
  }

  #[cfg(test)]
  pub fn create(&self, card: CardIn) -> Result<Created, String> {
    self.create_with(card, None)
  }

  pub fn create_with(&self, card: CardIn, inbox: Option<Inbox>) -> Result<Created, String> {
    let created = self.lock().create_with(card, inbox)?;
    if created.fresh {
      self.changed(Change::Created { title: created.card.title.clone(), body: body_of(&created.card) });
    } else if created.changed {
      self.changed(Change::Updated);
    }
    Ok(created)
  }

  pub fn listening(&self, id: &str) -> bool {
    self.lock_listen().get(id).is_some_and(|l| l.active(self.grace))
  }

  pub fn routing(&self, id: &str) -> Result<Option<Routing>, String> {
    self.lock().routing(id)
  }

  pub fn delivery_target(&self, id: &str) -> String {
    self.lock().delivery_target(id).ok().flatten().unwrap_or_default()
  }

  pub fn drop_target(&self, id: &str, endpoint: &str) {
    self.lock().drop_target(id, endpoint);
    self.changed(Change::Updated);
  }

  pub fn record_delivery(&self, id: &str, seq: u64, state: &str, via: &str, detail: &str) {
    let recorded = self.lock().record_delivery(id, seq, state, via, detail).unwrap_or(Recorded::Same);
    if recorded != Recorded::Same {
      self.changed(Change::Updated);
    }
  }

  pub fn find(&self, task: Option<&str>, key: Option<&str>, query: Option<&str>, limit: usize) -> Result<Vec<Found>, String> {
    self.lock().find(task, key, query, limit)
  }

  pub fn ingest_spool(&self, dir: &Path) -> usize {
    let (mut added, mut changed) = (0, false);
    let _ = drain_spool(dir, |card| {
      if Store::check(&card).is_err() {
        return true;
      }
      match self.lock().create(CardIn { force_new: Some(true), ..card }) {
        Ok(created) => {
          added += usize::from(created.fresh);
          changed |= created.changed;
          true
        }
        Err(_) => false,
      }
    });
    if changed {
      self.changed(Change::Updated);
    }
    added
  }

  #[cfg(test)]
  pub fn wait(
    self: &Arc<Self>,
    id: &str,
    since: u64,
    timeout: Duration,
    final_only: bool,
  ) -> Result<(String, Option<Event>), String> {
    self.wait_while(id, since, timeout, final_only, &|| true)
  }

  pub fn wait_while(
    self: &Arc<Self>,
    id: &str,
    since: u64,
    timeout: Duration,
    final_only: bool,
    alive: &dyn Fn() -> bool,
  ) -> Result<(String, Option<Event>), String> {
    let (mut ids, hit) = self.wait_set(&[(id.to_owned(), since)], timeout, final_only, alive)?;
    Ok((ids.remove(0), hit.map(|(_, e)| e)))
  }

  pub fn wait_any_while(
    self: &Arc<Self>,
    cards: &[(String, u64)],
    timeout: Duration,
    final_only: bool,
    alive: &dyn Fn() -> bool,
  ) -> Result<Option<(String, Event)>, String> {
    let (_, hit) = self.wait_set(cards, timeout, final_only, alive)?;
    Ok(hit.map(|(i, e)| (cards[i].0.clone(), e)))
  }

  fn wait_set(
    self: &Arc<Self>,
    cards: &[(String, u64)],
    timeout: Duration,
    final_only: bool,
    alive: &dyn Fn() -> bool,
  ) -> Result<(Vec<String>, Option<Hit>), String> {
    let deadline = Instant::now() + timeout;
    let mut guard = self.lock();
    if cards.iter().any(|(id, _)| guard.canon(id).is_err()) {
      if let Some(dir) = &self.spool {
        drop(guard);
        self.ingest_spool(dir);
        guard = self.lock();
      }
    }
    let ids = cards.iter().map(|(id, _)| guard.canon(id)).collect::<Result<Vec<_>, _>>()?;
    if !alive() {
      return Ok((ids, None));
    }
    let mut rose = false;
    for (id, (_, since)) in ids.iter().zip(cards) {
      if !final_only && self.deliver(id, *since) {
        rose = true;
        let _ = guard.deliver_through(id, *since);
      }
    }
    if let Some((i, e)) = Self::first_event(&mut guard, &ids, cards, final_only)? {
      rose |= !final_only && !e.is_final() && self.deliver(&ids[i], e.seq);
      rose |= guard.record_delivery(&ids[i], e.seq, "delivered", "agent", "") == Ok(Recorded::Changed);
      drop(guard);
      if rose {
        self.changed(Change::Updated);
      }
      return Ok((ids, Some((i, e))));
    }
    drop(guard);
    if rose {
      self.changed(Change::Updated);
    }
    let _waiting: Vec<_> = if final_only { Vec::new() } else { ids.iter().map(|id| self.enter(id)).collect() };
    guard = self.lock();
    loop {
      if let Some((i, e)) = Self::first_event(&mut guard, &ids, cards, final_only)? {
        if !alive() {
          return Ok((ids, None));
        }
        let mut rose = !final_only && !e.is_final() && self.deliver(&ids[i], e.seq);
        rose |= guard.record_delivery(&ids[i], e.seq, "delivered", "agent", "") == Ok(Recorded::Changed);
        drop(guard);
        if rose {
          self.changed(Change::Updated);
        }
        return Ok((ids, Some((i, e))));
      }
      let left = deadline.saturating_duration_since(Instant::now());
      if left.is_zero() || !alive() {
        return Ok((ids, None));
      }
      guard = self.cv.wait_timeout(guard, left.min(ALIVE_STEP)).unwrap_or_else(|p| p.into_inner()).0;
    }
  }

  fn first_event(
    store: &mut Store,
    ids: &[String],
    cards: &[(String, u64)],
    final_only: bool,
  ) -> Result<Option<Hit>, String> {
    for (i, (id, (_, since))) in ids.iter().zip(cards).enumerate() {
      if let Some(e) = store.wait_event(id, *since, final_only)? {
        return Ok(Some((i, e)));
      }
    }
    Ok(None)
  }

  pub fn reply(&self, id: &str, text: Option<String>, lines: Vec<LineComment>) -> Result<String, String> {
    let id = self.lock().agent_message(id, text, lines)?;
    self.changed(Change::Updated);
    Ok(id)
  }

  pub fn close_session(&self, agent: &str, session: &str) -> Result<u64, String> {
    let n = self.lock().close_session(agent, session)?;
    if n > 0 {
      self.changed(Change::Updated);
    }
    Ok(n)
  }

  pub fn answer(&self, id: &str, a: AnswerIn) -> Result<(Event, CardIn), String> {
    let r = self.lock().answer(id, a)?;
    self.changed(Change::Updated);
    Ok(r)
  }

  pub fn answer_from_phone(&self, r: &PhoneReq, now: i64) -> Result<PhoneOutcome, String> {
    let out = self.lock().phone_answer(r, now)?;
    if out.applied.is_some() {
      self.changed(Change::Updated);
    }
    Ok(out)
  }

  #[cfg(test)]
  pub fn stored_phone_ack(&self, answer_id: &str) -> Option<String> {
    self.lock().stored_phone_ack(answer_id)
  }

  pub fn set_phone_ack_result(&self, answer_id: &str, result: tmgr_phone::AckResult) {
    let _ = self.lock().set_phone_ack_result(answer_id, result);
  }

  pub fn message(&self, id: &str, m: MessageIn) -> Result<Event, String> {
    let e = self.lock().owner_message(id, m)?;
    self.changed(Change::Updated);
    Ok(e)
  }

  pub fn get(&self, id: &str) -> Result<CardView, String> {
    let store = self.lock();
    let view = store.get(id)?.ok_or_else(|| "unknown card".to_owned())?;
    Ok(self.mark(&store, view))
  }

  pub fn get_with_patches(&self, id: &str) -> Result<(CardView, Vec<PatchIn>), String> {
    let store = self.lock();
    let view = store.get(id)?.ok_or_else(|| "unknown card".to_owned())?;
    let patches = store.patches(&view.id)?;
    Ok((self.mark(&store, view), patches))
  }

  pub fn list(&self) -> Result<Vec<CardView>, String> {
    let store = self.lock();
    Ok(store.list()?.into_iter().map(|v| self.mark(&store, v)).collect())
  }

  pub fn live_messages(&self, id: &str) -> Vec<tmgr_phone::LiveMessage> {
    self.lock().live_messages(id)
  }

  pub fn pending_task_comments(&self) -> Result<Vec<PendingTaskComment>, String> {
    self.lock().pending_task_comments()
  }

  pub fn skip_task_comment(&self, card_id: &str, seq: u64, reason: &str) -> Result<(), String> {
    self.lock().skip_task_comment(card_id, seq, reason)
  }

  pub fn mark_task_comment(&self, card_id: &str, seq: u64) -> Result<(), String> {
    self.lock().mark_task_comment(card_id, seq)
  }

  pub fn pending_delivery(&self) -> Result<Vec<PendingDelivery>, String> {
    self.lock().pending_delivery()
  }

  pub fn check_liveness(&self, alive: impl Fn(u32) -> bool) {
    {
      let mut store = self.lock();
      let gone: Vec<String> =
        store.live_pids().unwrap_or_default().into_iter().filter(|(_, pid)| !alive(*pid)).map(|(id, _)| id).collect();
      if gone.is_empty() || store.mark_gone(&gone).is_err() {
        return;
      }
    }
    self.changed(Change::Updated);
  }
}

fn announce_idle(hub: Weak<Hub>, id: String, mut delay: Duration) {
  loop {
    std::thread::sleep(delay);
    let Some(hub) = hub.upgrade() else { return };
    match hub.settle(&id) {
      Some(left) => delay = left,
      None => return,
    }
  }
}

pub fn pid_alive(pid: u32) -> bool {
  let Ok(pid) = i32::try_from(pid) else { return true };
  if pid <= 0 {
    return true;
  }
  let rc = unsafe { libc::kill(pid, 0) };
  rc == 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::atomic::{AtomicUsize, Ordering};
  use tmgr_ipc::decisions::Source;

  const GRACE: Duration = Duration::from_millis(300);

  fn rig() -> (Arc<Hub>, Arc<AtomicUsize>, String) {
    let updates = Arc::new(AtomicUsize::new(0));
    let seen = updates.clone();
    let sink: Sink = Box::new(move |c, _| {
      if matches!(c, Change::Updated) {
        seen.fetch_add(1, Ordering::SeqCst);
      }
    });
    let store = Store::open(Path::new(":memory:")).unwrap();
    let hub = Arc::new(Hub::new(store, sink).with_grace(GRACE));
    let card = CardIn {
      title: "Q".into(),
      source: Source { agent: "claude".into(), ..Default::default() },
      ..Default::default()
    };
    let id = hub.create(card).unwrap().id;
    updates.store(0, Ordering::SeqCst);
    (hub, updates, id)
  }

  fn listening(hub: &Hub, id: &str) -> bool {
    hub.get(id).unwrap().listening
  }

  fn until(what: &str, f: impl Fn() -> bool) {
    let t = Instant::now();
    while !f() {
      assert!(t.elapsed() < Duration::from_secs(5), "timed out: {what}");
      std::thread::sleep(Duration::from_millis(10));
    }
  }

  #[test]
  fn listening_is_true_during_a_wait_and_for_the_grace_after_it() {
    let (hub, updates, id) = rig();
    assert!(!listening(&hub, &id));
    let h = hub.clone();
    let id2 = id.clone();
    let t = std::thread::spawn(move || h.wait(&id2, 0, Duration::from_millis(400), false).unwrap());
    until("listening", || listening(&hub, &id));
    assert_eq!(updates.load(Ordering::SeqCst), 1);
    assert!(hub.list().unwrap()[0].listening);
    assert!(hub.get_with_patches(&id).unwrap().0.listening);
    t.join().unwrap();
    assert!(listening(&hub, &id));
    until("idle", || !listening(&hub, &id));
    until("idle emit", || updates.load(Ordering::SeqCst) == 2);
  }

  #[test]
  fn a_reconnect_inside_the_grace_period_does_not_flicker() {
    let (hub, updates, id) = rig();
    hub.wait(&id, 0, Duration::ZERO, false).unwrap();
    std::thread::sleep(GRACE / 3);
    assert!(listening(&hub, &id));
    hub.wait(&id, 0, Duration::ZERO, false).unwrap();
    std::thread::sleep(GRACE / 3 * 2);
    assert!(listening(&hub, &id));
    assert_eq!(updates.load(Ordering::SeqCst), 1);
    until("idle", || !listening(&hub, &id));
    until("idle emit", || updates.load(Ordering::SeqCst) == 2);
    std::thread::sleep(GRACE);
    assert_eq!(updates.load(Ordering::SeqCst), 2);
  }

  #[test]
  fn an_alias_waiter_marks_the_canonical_card() {
    let (hub, _, id) = rig();
    let card = CardIn {
      title: "Q".into(),
      replaces: Some(id.clone()),
      source: Source { agent: "claude".into(), ..Default::default() },
      ..Default::default()
    };
    let new_id = hub.create(card).unwrap().id;
    let canon = hub.get(&new_id).unwrap().id;
    hub.wait(&new_id, 0, Duration::ZERO, false).unwrap();
    assert!(listening(&hub, &new_id));
    assert!(listening(&hub, &canon));
    assert!(listening(&hub, &id));
  }

  #[test]
  fn the_waiter_is_released_on_answer_and_on_error() {
    let (hub, _, id) = rig();
    let (h, id2) = (hub.clone(), id.clone());
    let t = std::thread::spawn(move || h.wait(&id2, 0, Duration::from_secs(10), false).unwrap());
    until("listening", || listening(&hub, &id));
    hub.answer(&id, AnswerIn { dismissed: true, ..Default::default() }).unwrap();
    assert!(t.join().unwrap().1.is_some());
    until("idle", || !listening(&hub, &id));
    assert_eq!(hub.lock_listen().get(&id).map(|l| l.waiters).unwrap_or(0), 0);
    assert!(hub.wait("c_00000000000000000000000000000000", 0, Duration::ZERO, false).is_err());
  }

  #[test]
  fn finished_cards_are_not_marked_by_a_repeated_wait() {
    let (hub, updates, id) = rig();
    hub.answer(&id, AnswerIn { dismissed: true, ..Default::default() }).unwrap();
    updates.store(0, Ordering::SeqCst);
    assert!(hub.wait(&id, 0, Duration::ZERO, false).unwrap().1.is_some());
    assert!(!listening(&hub, &id));
    assert_eq!(updates.load(Ordering::SeqCst), 0);
  }

  #[test]
  fn dropping_the_hub_does_not_wait_for_the_idle_thread() {
    let (hub, _, id) = rig();
    hub.wait(&id, 0, Duration::ZERO, false).unwrap();
    let weak = Arc::downgrade(&hub);
    drop(hub);
    assert!(weak.upgrade().is_none());
  }

  fn say(hub: &Hub, id: &str, text: &str) {
    hub.message(id, MessageIn { text: text.into(), ..Default::default() }).unwrap();
  }

  fn delivered(hub: &Hub, id: &str) -> Vec<bool> {
    hub.get(id).unwrap().messages.iter().filter(|m| m.author == "owner").map(|m| m.delivered).collect()
  }

  #[test]
  fn a_returned_message_stays_delivered_after_the_grace() {
    let (hub, _, id) = rig();
    say(&hub, &id, "one");
    assert_eq!(delivered(&hub, &id), [false]);
    assert_eq!(hub.wait(&id, 0, Duration::ZERO, false).unwrap().1.unwrap().seq, 1);
    assert_eq!(delivered(&hub, &id), [true]);
    until("idle", || !listening(&hub, &id));
    assert_eq!(delivered(&hub, &id), [true]);
    assert!(hub.list().unwrap()[0].messages[0].delivered);
    assert!(hub.get_with_patches(&id).unwrap().0.messages[0].delivered);
  }

  #[test]
  fn delivery_follows_message_events_not_event_seqs() {
    let (hub, updates, id) = rig();
    hub.answer(&id, AnswerIn { deferred: true, ..Default::default() }).unwrap();
    say(&hub, &id, "one");
    say(&hub, &id, "two");
    let first = hub.wait(&id, 0, Duration::ZERO, false).unwrap().1.unwrap();
    assert_eq!(first.seq, 1);
    assert_eq!(delivered(&hub, &id), [false, false]);
    let second = hub.wait(&id, first.seq, Duration::ZERO, false).unwrap().1.unwrap();
    assert_eq!(second.seq, 2);
    assert_eq!(delivered(&hub, &id), [true, false]);
    until("idle", || !listening(&hub, &id));
    updates.store(0, Ordering::SeqCst);
    say(&hub, &id, "three");
    assert_eq!(delivered(&hub, &id), [true, false, false]);
    hub.wait(&id, second.seq, Duration::ZERO, false).unwrap();
    assert_eq!(delivered(&hub, &id), [true, true, false]);
    hub.wait(&id, 3, Duration::ZERO, false).unwrap();
    assert_eq!(delivered(&hub, &id), [true, true, true]);
  }

  #[test]
  fn a_cursor_past_a_message_marks_it_delivered_without_returning_it() {
    let (hub, updates, id) = rig();
    say(&hub, &id, "one");
    updates.store(0, Ordering::SeqCst);
    assert!(hub.wait(&id, 1, Duration::ZERO, false).unwrap().1.is_none());
    assert_eq!(delivered(&hub, &id), [true]);
    assert!(updates.load(Ordering::SeqCst) >= 1);
  }

  #[test]
  fn a_final_only_wait_neither_listens_nor_delivers() {
    let (hub, _, id) = rig();
    say(&hub, &id, "one");
    let (h, id2) = (hub.clone(), id.clone());
    let t = std::thread::spawn(move || h.wait(&id2, 5, Duration::from_millis(300), true).unwrap());
    std::thread::sleep(Duration::from_millis(100));
    assert!(!listening(&hub, &id));
    assert_eq!(delivered(&hub, &id), [false]);
    assert!(t.join().unwrap().1.is_none());
    assert!(!listening(&hub, &id));
    assert_eq!(delivered(&hub, &id), [false]);
  }

  #[test]
  fn waits_do_not_deadlock_with_concurrent_reads() {
    let (hub, _, id) = rig();
    say(&hub, &id, "one");
    let (tx, rx) = std::sync::mpsc::channel();
    let mut handles = Vec::new();
    for final_only in [true, false, true, false] {
      let (h, id2, tx) = (hub.clone(), id.clone(), tx.clone());
      handles.push(std::thread::spawn(move || {
        for _ in 0..20 {
          h.wait(&id2, 0, Duration::from_millis(5), final_only).unwrap();
          h.get(&id2).unwrap();
          h.list().unwrap();
        }
        tx.send(()).unwrap();
      }));
    }
    for _ in 0..4 {
      rx.recv_timeout(Duration::from_secs(10)).expect("deadlock");
    }
    for t in handles {
      t.join().unwrap();
    }
  }

  #[test]
  fn a_returned_event_is_recorded_as_delivered_by_the_agent_final_ones_included() {
    let (hub, _, id) = rig();
    hub.record_delivery(&id, 1, "sent", "claude", "");
    say(&hub, &id, "one");
    hub.record_delivery(&id, 1, "sent", "claude", "");
    hub.wait(&id, 0, Duration::ZERO, false).unwrap();
    let d = hub.get(&id).unwrap().delivery.unwrap();
    assert_eq!((d.state.as_str(), d.via.as_str()), ("delivered", "agent"));
    hub.answer(&id, AnswerIn { dismissed: true, ..Default::default() }).unwrap();
    hub.record_delivery(&id, 2, "sent", "claude", "");
    hub.wait(&id, 0, Duration::ZERO, true).unwrap();
    let d = hub.get(&id).unwrap().delivery.unwrap();
    assert_eq!((d.state.as_str(), d.via.as_str()), ("delivered", "agent"));
    assert_eq!(hub.routing(&id).unwrap().unwrap().latest_seq, 2);
  }

  #[test]
  fn pid_probe() {
    assert!(pid_alive(std::process::id()));
    let mut child = std::process::Command::new("true").spawn().unwrap();
    let pid = child.id();
    child.wait().unwrap();
    assert!(!pid_alive(pid));
    assert!(pid_alive(0));
    assert!(pid_alive(u32::MAX));
  }

  #[test]
  fn a_cursor_past_a_message_writes_a_delivered_row_that_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("decisions.db");
    let card = CardIn { title: "Q".into(), source: Source { agent: "claude".into(), ..Default::default() }, ..Default::default() };
    let hub = Arc::new(Hub::new(Store::open(&path).unwrap(), Box::new(|_, _| {})));
    let id = hub.create(card).unwrap().id;
    say(&hub, &id, "one");
    assert!(hub.get(&id).unwrap().delivery.is_none());
    assert!(hub.wait(&id, 1, Duration::ZERO, false).unwrap().1.is_none());
    let d = hub.get(&id).unwrap().delivery.unwrap();
    assert_eq!((d.state.as_str(), d.via.as_str()), ("delivered", "agent"));
    drop(hub);
    let again = Arc::new(Hub::new(Store::open(&path).unwrap(), Box::new(|_, _| {})));
    let d = again.get(&id).unwrap().delivery.unwrap();
    assert_eq!((d.state.as_str(), d.via.as_str()), ("delivered", "agent"));
    assert!(again.live_messages(&id)[0].delivered_ms.is_some());
  }
}
