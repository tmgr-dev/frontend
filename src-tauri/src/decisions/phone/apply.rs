use std::sync::atomic::Ordering;

use tmgr_ipc::decisions::{CardIn, Event};
use tmgr_phone::{AckBody, AckResult, Pairing};

use super::super::store::{PhoneOutcome, PhoneReq};
use super::pins::Pin;
use super::projection::{delivery_target, session_of};
use super::relay::{self, RelayMsg};
use super::Phones;

const ALERT_EVERY_MS: i64 = 10 * 60 * 1000;
const FAILED_IDS_MAX: usize = 1024;
const OTHER_PHONE: &str = "another phone";

impl Phones {
  fn pairing_of<'a>(pin: &'a Pin) -> Pairing<'a> {
    Pairing { mailbox: &pin.mailbox, desktop_fp: &pin.desktop_fp, phone_fp: &pin.phone_fp }
  }

  pub(super) fn send_ack(&self, pin: &Pin, answer_id: &str, ack: AckBody) -> Result<(), String> {
    let keys = pin.keys()?;
    let sealed = tmgr_phone::seal_ack(&keys.phone_kex_pub, &Self::pairing_of(pin), &keys.sign_secret, &ack)
      .map_err(|e| e.to_string())?;
    self.relay.post_message(&pin.conn(), "ack", answer_id, &sealed).map_err(|e| e.to_string())
  }

  fn ack_body(&self, answer_id: &str, result: AckResult, winner: Option<String>, label: Option<String>) -> AckBody {
    AckBody { answer_id: answer_id.to_owned(), result, at: (self.now)(), winner, label }
  }

  fn winner_name(&self, pin: &Pin, winner: Option<String>) -> Option<String> {
    let w = winner?;
    if w == "desktop" {
      return Some(pin.desktop_name.clone());
    }
    let pins = self.pins.list().unwrap_or_default();
    Some(
      pins.iter().find(|p| p.phone_fp.starts_with(&w)).map_or_else(|| OTHER_PHONE.to_owned(), |p| p.phone_name.clone()),
    )
  }

  fn first_failure(&self, id: &str) -> bool {
    let mut seen = self.failed_ids.lock().unwrap_or_else(|p| p.into_inner());
    if seen.len() >= FAILED_IDS_MAX {
      seen.clear();
    }
    seen.insert(id.to_owned())
  }

  pub(super) fn verification_failed(&self, pin: &Pin, msg: &RelayMsg) {
    let now = (self.now)();
    self.failures.fetch_add(1, Ordering::SeqCst);
    if self.first_failure(&msg.id) {
      self.audit.append(now, "verify_failed", &[("mailbox", &pin.mailbox), ("message", &msg.id)]);
    }
    let last = self.last_alert.load(Ordering::SeqCst);
    if now.saturating_sub(last) >= ALERT_EVERY_MS {
      self.last_alert.store(now, Ordering::SeqCst);
      self.host.notify("TMGR", "A message from a paired iPhone failed verification");
    }
  }

  fn touch(&self, mailbox: &str, now: i64) {
    let _ = self.pins.update(mailbox, |p| p.last_used_ms = now);
  }

  pub(super) fn handle_answer(&self, listed: &Pin, msg: &RelayMsg) -> Result<(), String> {
    let now = (self.now)();
    let conn = listed.conn();
    let Some(pin) = self.pins.get(&listed.mailbox)?.filter(|p| !p.expired(now)) else {
      if tmgr_phone::valid_answer_id(&msg.reference) {
        let _ = self.send_ack(listed, &msg.reference, self.ack_body(&msg.reference, AckResult::Revoked, None, None));
      }
      let _ = self.relay.delete_message(&conn, &msg.id);
      return Ok(());
    };
    let keys = pin.keys()?;
    let opened = tmgr_phone::open_answer(&keys.kex_secret, &Self::pairing_of(&pin), &keys.phone_sign_pub, &msg.body);
    let Ok(body) = opened else {
      self.verification_failed(&pin, msg);
      let _ = self.relay.delete_message(&conn, &msg.id);
      return Ok(());
    };
    let req = PhoneReq {
      answer_id: body.answer_id.clone(),
      phone_fp: pin.phone_fp.clone(),
      card_id: body.card_id,
      card_digest: body.card_digest,
      session: body.session,
      action: body.action,
      choice: body.choice,
      text: body.text,
      issued_at: body.issued_at,
    };
    let out = self.hub.answer_from_phone(&req, now)?;
    if out.result == AckResult::Applied {
      self.touch(&pin.mailbox, now);
    }
    self.audit.append(
      now,
      "answer",
      &[("answer_id", &req.answer_id), ("result", out.result.as_str()), ("phone_fp", &pin.phone_fp)],
    );
    self.after_apply(&req.card_id, &out);
    let ack = self.ack_body(&req.answer_id, out.result, self.winner_name(&pin, out.winner.clone()), out.label.clone());
    self.send_ack(&pin, &req.answer_id, ack)?;
    let _ = self.relay.delete_message(&conn, &msg.id);
    Ok(())
  }

  fn after_apply(&self, card_id: &str, out: &PhoneOutcome) {
    let Some((event, card)) = &out.applied else {
      return;
    };
    let label = out.label.as_deref().unwrap_or("Text reply");
    self.host.notify("TMGR", &format!("Answered from iPhone: {label}"));
    self.deliver_bound(card_id, card, event, &out.target);
  }

  fn deliver_bound(&self, card_id: &str, card: &CardIn, event: &Event, bound: &str) {
    if bound.is_empty() || delivery_target(card) != bound {
      return;
    }
    let unchanged = self.hub.get(card_id).is_ok_and(|v| session_of(&v) == bound);
    if unchanged {
      self.host.deliver(card, event, bound);
    }
  }

  pub fn cleanup_revoked(&self, pin: &Pin) {
    let conn = pin.conn();
    if let Ok(msgs) = self.relay.list_messages(&conn) {
      for m in relay::usable(msgs).filter(|m| m.kind == "answer" && tmgr_phone::valid_answer_id(&m.reference)) {
        let _ = self.send_ack(pin, &m.reference, self.ack_body(&m.reference, AckResult::Revoked, None, None));
      }
    }
    let _ = self.relay.delete_mailbox(&conn);
  }

  pub(super) fn expire_pin(&self, pin: &Pin) {
    let _ = self.pins.delete(&pin.mailbox);
    self.synced.lock().unwrap_or_else(|p| p.into_inner()).remove(&pin.mailbox);
    self.audit.append((self.now)(), "expired", &[("mailbox", &pin.mailbox), ("phone_fp", &pin.phone_fp)]);
    self.cleanup_revoked(pin);
    self.host.changed();
  }
}
