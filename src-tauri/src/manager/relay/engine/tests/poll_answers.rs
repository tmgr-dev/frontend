use super::*;
use crate::manager::relay::poll::MAX_POLLS;
use tmgr_ipc::chat::{PollAnswer, PollMsg, PollOption, PollPick, PollQuestion};

const CARD: &str = "poll_0123456789abcdef0123456789abcdef";
const UNKNOWN: &str = "Unknown or closed question";

struct Rig {
  fake: Fake,
  a: Engine,
  r: Engine,
  rid: String,
}

fn rig() -> Rig {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let r = join_as(&fake, &clock, &a, "Runner", "runner");
  let rid = my_id(&r);
  Rig { fake, a, r, rid }
}

fn opt(label: &str) -> PollOption {
  PollOption { label: label.into(), description: String::new(), recommended: false }
}

fn question(multi: bool) -> PollQuestion {
  PollQuestion { header: "DB".into(), question: "Which DB?".into(), multi_select: multi, options: vec![opt("Postgres"), opt("SQLite"), opt("MySQL")] }
}

fn open_poll(card_id: &str, questions: Vec<PollQuestion>) -> PollMsg {
  PollMsg { card_id: card_id.into(), session: "s_abcdefghij".into(), questions, closed: None, picked: None, can_answer: true, why_not: None }
}

fn closed_poll(card_id: &str, how: &str) -> PollMsg {
  PollMsg { card_id: card_id.into(), session: "s_abcdefghij".into(), questions: vec![], closed: Some(how.into()), picked: None, can_answer: true, why_not: None }
}

fn poll_msg(n: u8, poll: PollMsg) -> ChatMessage {
  let mut m = msg_n(n, "manager");
  m.author.machine = Some("mx_remote".into());
  m.poll = Some(poll);
  m
}

fn deliver(rig: &Rig, msg: &ChatMessage) {
  post_chat(&rig.r, &my_id(&rig.a), msg);
  rig.a.poll_once().unwrap();
}

fn post(rig: &Rig, n: u8, poll: PollMsg) {
  deliver(rig, &poll_msg(n, poll));
}

fn to_runner(rig: &Rig) -> Vec<(String, String, String)> {
  let s = rig.fake.0.lock().unwrap();
  s.msgs.iter().filter(|m| m.to == rig.rid).map(|m| (m.msg_id.clone(), m.from.clone(), m.body.clone())).collect()
}

fn counts(rig: &Rig) -> (usize, usize) {
  (to_runner(rig).len(), rig.fake.posts())
}

fn sent_to_runner(rig: &Rig, before: usize) -> (String, Value) {
  let all = to_runner(rig);
  assert_eq!(all.len(), before + 1, "exactly one new message for the runner");
  let (msg_id, from, body) = all.last().unwrap().clone();
  let g = rig.r.lock();
  let id = g.identity.as_ref().unwrap().device_id.clone();
  let trusted: Vec<_> = g.st.trusted.iter().map(to_trusted).collect();
  let opened = open_message_checked(g.keys.as_ref().unwrap(), USER, &id, &from, &msg_id, &body, T0, &trusted, |_| None).unwrap();
  assert_eq!(opened.kind, "permitok");
  (msg_id, opened.payload)
}

fn pick(idx: u8, choices: &[u8], other: Option<&str>) -> PollPick {
  PollPick { question_idx: idx, choices: choices.to_vec(), other: other.map(str::to_owned) }
}

fn answer(rig: &Rig, picks: Vec<PollPick>) -> Result<(), String> {
  rig.a.answer_poll(CARD, &picks)
}

#[test]
fn a_tap_sends_an_unsigned_poll_answer_to_the_posting_runner() {
  let rig = rig();
  post(&rig, 2, open_poll(CARD, vec![question(false)]));
  let before = to_runner(&rig).len();
  answer(&rig, vec![pick(0, &[1], None)]).unwrap();
  let (_, payload) = sent_to_runner(&rig, before);
  assert_eq!(payload, json!({"v": 1, "card_id": CARD, "answers": [{"question_idx": 0, "choices": [1]}]}));
  let parsed: PollAnswer = serde_json::from_value(payload).unwrap();
  parsed.validate().unwrap();
}

#[test]
fn several_questions_go_out_in_question_order_with_other_text() {
  let rig = rig();
  post(&rig, 2, open_poll(CARD, vec![question(true), question(false)]));
  let before = to_runner(&rig).len();
  answer(&rig, vec![pick(1, &[], Some("MariaDB")), pick(0, &[0, 2], None)]).unwrap();
  let (_, payload) = sent_to_runner(&rig, before);
  assert_eq!(payload["answers"], json!([{"question_idx": 0, "choices": [0, 2]}, {"question_idx": 1, "choices": [], "other": "MariaDB"}]));
}

#[test]
fn the_card_stays_answerable_after_a_send_until_the_runner_closes_it() {
  let rig = rig();
  post(&rig, 2, open_poll(CARD, vec![question(false)]));
  answer(&rig, vec![pick(0, &[0], None)]).unwrap();
  assert_eq!(rig.a.lock().polls.len(), 1);
  deliver(&rig, &poll_msg(3, closed_poll(CARD, "applied")));
  assert_eq!(rig.a.lock().polls.len(), 0);
}

#[test]
fn an_unknown_or_closed_card_is_refused_and_nothing_is_sent() {
  let rig = rig();
  assert_eq!(answer(&rig, vec![pick(0, &[0], None)]).unwrap_err(), UNKNOWN);
  post(&rig, 2, open_poll(CARD, vec![question(false)]));
  deliver(&rig, &poll_msg(3, closed_poll(CARD, "terminal")));
  let before = counts(&rig);
  assert_eq!(answer(&rig, vec![pick(0, &[0], None)]).unwrap_err(), UNKNOWN);
  assert_eq!(counts(&rig), before);
}

#[test]
fn a_card_that_cannot_be_answered_is_refused() {
  let rig = rig();
  let mut p = open_poll(CARD, vec![question(false)]);
  p.can_answer = false;
  p.why_not = Some("The picker is not on screen".into());
  post(&rig, 2, p);
  let before = counts(&rig);
  assert_eq!(answer(&rig, vec![pick(0, &[0], None)]).unwrap_err(), "Answer in the terminal");
  assert_eq!(counts(&rig), before);
}

#[test]
fn invalid_picks_are_refused_against_the_stored_card() {
  let rig = rig();
  post(&rig, 2, open_poll(CARD, vec![question(false), question(true)]));
  let ok1 = pick(0, &[0], None);
  let ok2 = pick(1, &[1], None);
  let long = "x".repeat(1001);
  let cases: Vec<(Vec<PollPick>, &str)> = vec![
    (vec![ok1.clone()], "Answer every question"),
    (vec![ok1.clone(), ok2.clone(), pick(1, &[0], None)], "Answer every question"),
    (vec![ok1.clone(), pick(0, &[1], None)], "Answer each question once"),
    (vec![pick(0, &[3], None), ok2.clone()], "Unknown option"),
    (vec![pick(0, &[0, 1], None), ok2.clone()], "Pick one option"),
    (vec![ok1.clone(), pick(1, &[1, 1], None)], "Pick each option once"),
    (vec![ok1.clone(), pick(1, &[], Some("text"))], "Free text is not offered for this question"),
    (vec![pick(0, &[0], Some("text")), ok2.clone()], "Pick an option or write text, not both"),
    (vec![pick(0, &[], Some("")), ok2.clone()], "The text is empty"),
    (vec![pick(0, &[], Some(&long)), ok2.clone()], "The text is too long"),
    (vec![pick(0, &[], Some("a\nb")), ok2.clone()], "The text has control characters"),
    (vec![pick(0, &[], None), ok2.clone()], "Pick an option"),
    (vec![ok1.clone(), pick(1, &[], None)], "Pick an option"),
  ];
  let before = counts(&rig);
  for (picks, why) in cases {
    assert_eq!(answer(&rig, picks.clone()).unwrap_err(), why, "{picks:?}");
  }
  assert_eq!(counts(&rig), before);
}

#[test]
fn a_card_from_a_device_that_is_not_the_bound_runner_is_not_stored() {
  let rig = rig();
  let mut m = poll_msg(2, open_poll(CARD, vec![question(false)]));
  m.author.kind = "owner".into();
  deliver(&rig, &m);
  assert_eq!(rig.a.lock().polls.len(), 0);
}

#[test]
fn malformed_cards_are_not_stored_and_a_close_without_questions_is_not_a_card() {
  let rig = rig();
  post(&rig, 2, open_poll(CARD, vec![]));
  let mut many = question(false);
  many.options = (0..9).map(|i| opt(&format!("o{i}"))).collect();
  post(&rig, 3, open_poll(CARD, vec![many]));
  post(&rig, 4, open_poll("bad id", vec![question(false)]));
  assert_eq!(rig.a.lock().polls.len(), 0);
}

#[test]
fn a_repost_with_the_same_id_replaces_the_stored_card() {
  let rig = rig();
  post(&rig, 2, open_poll(CARD, vec![question(false)]));
  post(&rig, 3, open_poll(CARD, vec![question(false), question(false)]));
  assert_eq!(rig.a.lock().polls.len(), 1);
  assert_eq!(answer(&rig, vec![pick(0, &[0], None)]).unwrap_err(), "Answer every question");
}

#[test]
fn the_book_is_bounded() {
  let rig = rig();
  for n in 0..(MAX_POLLS + 5) {
    let id = format!("poll_{n:032x}");
    let m = poll_msg(2, open_poll(&id, vec![question(false)]));
    let mut m = m;
    m.id = format!("m_{n:032x}");
    deliver(&rig, &m);
  }
  assert_eq!(rig.a.lock().polls.len(), MAX_POLLS);
}

#[test]
fn an_answer_with_no_paired_runner_reports_the_unknown_card() {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  assert_eq!(a.answer_poll(CARD, &[pick(0, &[0], None)]).unwrap_err(), UNKNOWN);
}
