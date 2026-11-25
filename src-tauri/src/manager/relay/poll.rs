//! Remote poll cards received from a runner, and the unsigned answers this desktop sends for them.
//! The webview names a card and the picks; everything sent is checked against the stored card.

use tmgr_ipc::chat::{limits, PollAnswer, PollMsg, PollPick};
use tmgr_spawn_auth::is_token;

use super::super::backend::RoomRef;

pub const MAX_POLLS: usize = 64;

#[derive(Clone, Debug)]
pub struct StoredPoll {
  pub card_id: String,
  pub from: String,
  pub room: RoomRef,
  pub poll: PollMsg,
}

#[derive(Default)]
pub struct PollBook {
  cards: Vec<StoredPoll>,
}

fn well_formed(poll: &PollMsg) -> bool {
  is_token(&poll.card_id)
    && poll.closed.is_none()
    && (1..=limits::MAX_POLL_QUESTIONS).contains(&poll.questions.len())
    && poll.questions.iter().all(|q| (1..=limits::MAX_POLL_OPTIONS).contains(&q.options.len()))
}

impl PollBook {
  pub fn accept(&mut self, from: &str, room: &RoomRef, poll: &PollMsg) -> bool {
    if !well_formed(poll) {
      return false;
    }
    let stored = StoredPoll { card_id: poll.card_id.clone(), from: from.to_owned(), room: room.clone(), poll: poll.clone() };
    if let Some(i) = self.cards.iter().position(|c| c.card_id == poll.card_id) {
      if self.cards[i].from != from {
        return false;
      }
      self.cards[i] = stored;
      return true;
    }
    if self.cards.len() >= MAX_POLLS {
      self.cards.remove(0);
    }
    self.cards.push(stored);
    true
  }

  pub fn close(&mut self, card_id: &str, from: &str) {
    self.cards.retain(|c| !(c.card_id == card_id && c.from == from));
  }

  pub fn get(&self, card_id: &str) -> Option<&StoredPoll> {
    self.cards.iter().find(|c| c.card_id == card_id)
  }

  #[cfg(test)]
  pub fn len(&self) -> usize {
    self.cards.len()
  }
}

pub fn build_answer(stored: &StoredPoll, picks: &[PollPick]) -> Result<PollAnswer, String> {
  let poll = &stored.poll;
  if !poll.can_answer {
    return Err("Answer in the terminal".into());
  }
  if picks.len() != poll.questions.len() {
    return Err("Answer every question".into());
  }
  let mut answers: Vec<PollPick> = Vec::with_capacity(picks.len());
  for (idx, q) in poll.questions.iter().enumerate() {
    let mut matching = picks.iter().filter(|p| usize::from(p.question_idx) == idx);
    let pick = matching.next().ok_or_else(|| "Answer every question".to_owned())?;
    if matching.next().is_some() {
      return Err("Answer each question once".into());
    }
    if pick.choices.iter().any(|c| usize::from(*c) >= q.options.len()) {
      return Err("Unknown option".into());
    }
    let mut seen = pick.choices.clone();
    seen.sort_unstable();
    seen.dedup();
    if seen.len() != pick.choices.len() {
      return Err("Pick each option once".into());
    }
    if !q.multi_select && pick.choices.len() > 1 {
      return Err("Pick one option".into());
    }
    match pick.other.as_deref() {
      Some(_) if q.multi_select => return Err("Free text is not offered for this question".into()),
      Some(_) if !pick.choices.is_empty() => return Err("Pick an option or write text, not both".into()),
      Some("") => return Err("The text is empty".into()),
      Some(t) if t.len() > limits::MAX_POLL_OTHER => return Err("The text is too long".into()),
      Some(t) if t.chars().any(char::is_control) => return Err("The text has control characters".into()),
      None if pick.choices.is_empty() => return Err("Pick an option".into()),
      _ => {}
    }
    answers.push(pick.clone());
  }
  let answer = PollAnswer { v: 1, card_id: stored.card_id.clone(), answers };
  answer.validate()?;
  Ok(answer)
}
