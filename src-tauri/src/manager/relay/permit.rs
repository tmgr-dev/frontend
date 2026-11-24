//! Remote permission cards received from a runner, and the answers this desktop signs for them.
//! The webview names a card and a choice; everything that gets signed comes from the stored card.

use serde::Deserialize;
use tmgr_ipc::chat::PermitCardMsg;
use tmgr_spawn_auth::{is_hex_lower, is_token, permit_signed_string, PermitChoice, PermitRecord, PermitScreen, PinnedKey, SignedPermit};

use super::super::backend::RoomRef;

pub const MAX_PERMITS: usize = 64;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PermitAnswer {
  Once,
  Always,
  Deny,
}

#[derive(Clone, Debug)]
pub struct StoredPermit {
  pub card_id: String,
  pub from: String,
  pub room: RoomRef,
  pub card: PermitCardMsg,
}

#[derive(Default)]
pub struct PermitBook {
  cards: Vec<StoredPermit>,
}

pub fn screen_of(card: &PermitCardMsg) -> PermitScreen {
  PermitScreen {
    agent: card.agent.clone(),
    tool: card.tool.clone(),
    text: card.text.clone(),
    path: card.path.clone(),
    reason: card.reason.clone(),
    from: card.from.clone(),
    warning: card.warning.clone(),
    danger: card.danger.clone(),
    always_label: card.always_label.clone(),
  }
}

fn well_formed(card_id: &str, card: &PermitCardMsg) -> bool {
  let screen = screen_of(card);
  is_token(card_id)
    && is_hex_lower(&card.nonce, 32)
    && is_hex_lower(&card.runner_fp, 64)
    && is_hex_lower(&card.req_sha256, 64)
    && screen.validate().is_ok()
    && screen.req_sha256() == card.req_sha256
}

impl PermitBook {
  pub fn accept(&mut self, card_id: &str, from: &str, room: &RoomRef, card: &PermitCardMsg, now: u64) -> bool {
    if !well_formed(card_id, card) {
      return false;
    }
    let stored = StoredPermit { card_id: card_id.to_owned(), from: from.to_owned(), room: room.clone(), card: card.clone() };
    if let Some(i) = self.cards.iter().position(|c| c.card_id == card_id) {
      if self.cards[i].from != from {
        return false;
      }
      self.cards[i] = stored;
      return true;
    }
    if self.cards.len() >= MAX_PERMITS {
      let victim = self.cards.iter().position(|c| c.card.exp_ms < now).or_else(|| self.cards.iter().enumerate().min_by_key(|(_, c)| c.card.exp_ms).map(|(i, _)| i));
      if let Some(i) = victim {
        self.cards.remove(i);
      }
    }
    self.cards.push(stored);
    true
  }

  pub fn close(&mut self, card_id: &str, from: &str) {
    self.cards.retain(|c| !(c.card_id == card_id && c.from == from));
  }

  pub fn get(&self, card_id: &str) -> Option<&StoredPermit> {
    self.cards.iter().find(|c| c.card_id == card_id)
  }

  pub fn remove(&mut self, card_id: &str) {
    self.cards.retain(|c| c.card_id != card_id);
  }

  #[cfg(test)]
  pub fn len(&self) -> usize {
    self.cards.len()
  }
}

pub fn sign_answer(stored: &StoredPermit, answer: PermitAnswer, confirmed: bool, user_id: &str, own_fp: &str, now: u64, sign_secret: &[u8]) -> Result<SignedPermit, String> {
  let card = &stored.card;
  if !card.desktop_ok {
    return Err("Answer on your phone or in tmgr attach".into());
  }
  if now > card.exp_ms {
    return Err("This permission card has expired".into());
  }
  let screen = screen_of(card);
  if answer == PermitAnswer::Always && !screen.always_offered() {
    return Err("Always is not offered for this card".into());
  }
  let needs_confirm = answer == PermitAnswer::Always || (screen.is_danger() && answer == PermitAnswer::Once);
  if needs_confirm && !confirmed {
    return Err("This answer needs an explicit confirmation".into());
  }
  let choice = match answer {
    PermitAnswer::Once => PermitChoice::Yes,
    PermitAnswer::Always => PermitChoice::Always,
    PermitAnswer::Deny => PermitChoice::No,
  };
  let record = PermitRecord {
    user_id: user_id.to_owned(),
    runner_id: card.runner_id.clone(),
    runner_fp: card.runner_fp.clone(),
    card_id: stored.card_id.clone(),
    session: card.session.clone(),
    nonce: card.nonce.clone(),
    issued_ms: 0,
    exp_ms: card.exp_ms,
    screen: screen.clone(),
  };
  let key = PinnedKey { user_id: user_id.to_owned(), device_id: String::new(), device_fp: own_fp.to_owned(), spawn_pub: Vec::new() };
  let mut signed = SignedPermit {
    v: 1,
    user_id: user_id.to_owned(),
    device_fp: own_fp.to_owned(),
    runner_id: card.runner_id.clone(),
    runner_fp: card.runner_fp.clone(),
    card_id: stored.card_id.clone(),
    session: card.session.clone(),
    choice,
    confirmed: u8::from(confirmed),
    nonce: card.nonce.clone(),
    ts_ms: now,
    exp_ms: card.exp_ms,
    req_sha256: screen.req_sha256(),
    alg: "es256".into(),
    sig: String::new(),
  };
  let message = permit_signed_string(&record, &key, &signed);
  signed.sig = tmgr_phone::sign(sign_secret, &message).map_err(|_| "Could not sign the answer".to_owned())?;
  Ok(signed)
}
