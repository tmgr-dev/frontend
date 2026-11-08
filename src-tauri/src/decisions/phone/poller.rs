use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Weak};
use std::time::Duration;

use tmgr_phone::{Pairing, SnapshotBody, SnapshotCard};

use super::pins::Pin;
use super::relay;
use super::projection::{eligible, project, CARDS_MAX};
use super::Phones;

const BUSY_MS: i64 = 10_000;
const IDLE_MS: i64 = 5 * 60 * 1000;
const PAIR_MS: i64 = 3_000;
const DEBOUNCE_MS: i64 = 1_000;
const REFRESH_MS: i64 = 60_000;
const WAKE_JUMP_MS: i64 = 30_000;

impl Phones {
  pub fn start(self: &Arc<Self>) {
    let weak = Arc::downgrade(self);
    let spawned = std::thread::Builder::new().name("phone-relay".into()).spawn(move || run(weak));
    if spawned.is_err() {
      log::warn!("[phones] poller failed to start");
    }
  }

  pub fn poll_all(&self) {
    let now = (self.now)();
    let Ok(pins) = self.pins.list() else { return };
    for pin in pins {
      if pin.expired(now) {
        self.expire_pin(&pin);
        continue;
      }
      let msgs = match self.relay.list_messages(&pin.conn()) {
        Ok(m) => {
          self.poll_recovered(&pin.mailbox);
          m
        }
        Err(e) => {
          self.poll_failed(&pin.mailbox, "reading the messages of a paired phone failed", &e);
          continue;
        }
      };
      for msg in relay::usable(msgs) {
        if msg.kind == "answer" {
          if let Err(e) = self.handle_answer(&pin, &msg) {
            log::warn!("[phones] an answer could not be processed: {e}");
          }
        } else {
          let _ = self.relay.delete_message(&pin.conn(), &msg.id);
        }
      }
    }
  }

  pub(super) fn poll_failed(&self, key: &str, what: &str, e: &relay::RelayError) {
    let msg = e.to_string();
    let mut g = self.poll_errors.lock().unwrap_or_else(|p| p.into_inner());
    if g.get(key) != Some(&msg) {
      log::warn!("[phones] {what}: {msg}");
      g.insert(key.to_owned(), msg);
    }
  }

  pub(super) fn poll_recovered(&self, key: &str) {
    self.poll_errors.lock().unwrap_or_else(|p| p.into_inner()).remove(key);
  }

  pub fn refresh_snapshots(&self, force: bool) {
    let now = (self.now)();
    let Ok(pins) = self.pins.list() else { return };
    let pins: Vec<Pin> = pins.into_iter().filter(|p| !p.expired(now)).collect();
    if pins.is_empty() {
      return;
    }
    let Ok(views) = self.hub.list() else { return };
    let live: Vec<_> = views.iter().filter(|v| eligible(v)).collect();
    let mut cards = Vec::new();
    for v in &live {
      if cards.len() == CARDS_MAX {
        break;
      }
      if let Some(p) = project(v, &self.hub.delivery_target(&v.id)) {
        cards.push(SnapshotCard {
          id: v.id.clone(),
          digest: p.digest,
          state: v.state.clone(),
          session_gone: v.session_gone,
          projection: p.json,
        });
      }
    }
    let more = (live.len() - cards.len()) as u32;
    let sig = tmgr_phone::h(
      &cards
        .iter()
        .map(|c| format!("{}:{}:{}:{}", c.id, c.digest, c.state, c.session_gone))
        .collect::<Vec<_>>()
        .join("|"),
    ) + &more.to_string();
    for pin in pins {
      self.upload_snapshot(&pin, &cards, more, &sig, force, now);
    }
  }

  fn upload_snapshot(&self, pin: &Pin, cards: &[SnapshotCard], more: u32, sig: &str, force: bool, now: i64) {
    let (nudge, skip) = {
      let g = self.synced.lock().unwrap_or_else(|p| p.into_inner());
      match g.get(&pin.mailbox) {
        Some(s) => {
          let fresh = s.ids.as_ref().is_some_and(|ids| cards.iter().any(|c| c.state == "open" && !ids.contains(&c.id)));
          (fresh, !force && s.sig == sig && now - s.at < IDLE_MS)
        }
        None => (false, false),
      }
    };
    if skip {
      return;
    }
    let mut seq = 0;
    let Ok(Some(current)) = self.pins.update(&pin.mailbox, |p| {
      seq = (p.last_seq + 1).max(now);
      p.last_seq = seq;
    }) else {
      return;
    };
    let Ok(keys) = current.keys() else { return };
    let body = SnapshotBody {
      v: 1,
      seq,
      generated_at: now,
      desktop_name: current.desktop_name.clone(),
      cards: cards.to_vec(),
      more,
    };
    let pairing = Pairing { mailbox: &current.mailbox, desktop_fp: &current.desktop_fp, phone_fp: &current.phone_fp };
    let Ok(sealed) = tmgr_phone::seal_snapshot(&keys.phone_kex_pub, &pairing, &keys.sign_secret, &body) else {
      return;
    };
    match self.relay.put_snapshot(&current.conn(), &sealed, nudge) {
      Ok(()) => {
        let ids: HashSet<String> = cards.iter().map(|c| c.id.clone()).collect();
        self
          .synced
          .lock()
          .unwrap_or_else(|p| p.into_inner())
          .insert(current.mailbox.clone(), super::Synced { sig: sig.to_owned(), at: now, ids: Some(ids) });
      }
      Err(e) => log::warn!("[phones] snapshot upload failed: {e}"),
    }
  }

  fn take_kick(&self) -> bool {
    let (lock, cv) = &self.kick;
    let mut g = lock.lock().unwrap_or_else(|p| p.into_inner());
    if !*g {
      g = cv.wait_timeout(g, Duration::from_secs(1)).unwrap_or_else(|p| p.into_inner()).0;
    }
    std::mem::replace(&mut *g, false)
  }

  pub(super) fn tick(&self, st: &mut Ticker) {
    let now = (self.now)();
    let kicked = self.take_kick();
    let woke = now - st.last_tick > WAKE_JUMP_MS;
    st.last_tick = now;
    self.expire_pairing();
    if self.pairing_active() && (kicked || now - st.last_pair >= PAIR_MS) {
      st.last_pair = now;
      self.poll_pairing();
    }
    let interval = if self.hub.open_count() > 0 { BUSY_MS } else { IDLE_MS };
    if kicked || woke || now - st.last_poll >= interval {
      st.last_poll = now;
      self.poll_all();
    }
    let since = self.dirty_since.load(Ordering::SeqCst);
    let dirty = since != 0 && now - since >= DEBOUNCE_MS;
    if dirty || kicked || woke || now - st.last_refresh >= REFRESH_MS {
      if dirty {
        self.dirty_since.store(0, Ordering::SeqCst);
      }
      st.last_refresh = now;
      self.refresh_snapshots(false);
    }
  }
}

pub(super) struct Ticker {
  last_tick: i64,
  last_pair: i64,
  last_poll: i64,
  last_refresh: i64,
}

fn run(weak: Weak<Phones>) {
  let mut st = Ticker { last_tick: i64::MAX / 2, last_pair: 0, last_poll: 0, last_refresh: 0 };
  loop {
    let Some(p) = weak.upgrade() else { break };
    if st.last_tick == i64::MAX / 2 {
      st.last_tick = (p.now)();
    }
    p.tick(&mut st);
  }
}
