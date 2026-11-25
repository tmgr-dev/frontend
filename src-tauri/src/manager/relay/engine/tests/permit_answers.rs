use super::*;
use crate::manager::backend::DecisionRef;
use crate::manager::relay::permit::{screen_of, PermitAnswer, MAX_PERMITS};
use tmgr_ipc::chat::PermitCardMsg;
use tmgr_spawn_auth::{verify_permit, DesktopPolicy, PermitChoice, PermitRecord, PermitRefusal, PermitSender, PermitSenderKind, PermittedAnswer, PinnedKey, SignedPermit, TrustStatus, TrustView};

const CARD: &str = "card-abc";
const EXP: u64 = T0 + 60_000;
const UNKNOWN: &str = "Unknown or closed permission card";
const CONFIRM: &str = "This answer needs an explicit confirmation";

struct Trusting;
impl TrustView for Trusting {
  fn status(&self, _: &str, _: &str) -> TrustStatus {
    TrustStatus::Trusted
  }
}

struct Rig {
  fake: Fake,
  clock: Clock,
  a: Engine,
  r: Engine,
  rid: String,
}

fn rig() -> Rig {
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A");
  let r = join_as(&fake, &clock, &a, "Runner", "runner");
  let rid = my_id(&r);
  Rig { fake, clock, a, r, rid }
}

fn card(danger: bool, desktop_ok: bool) -> PermitCardMsg {
  let mut c = PermitCardMsg {
    session: "s_abcdefghij".into(),
    runner_id: "mx_00aa".into(),
    runner_fp: "f".repeat(64),
    nonce: "ab".repeat(16),
    exp_ms: EXP,
    req_sha256: String::new(),
    reissue: 0,
    agent: "claude-code".into(),
    tool: "Bash".into(),
    text: "ls -la".into(),
    path: Some("/work/app".into()),
    reason: None,
    from: None,
    warning: danger.then(|| "deletes files".to_owned()),
    danger: if danger { vec!["destructive".to_owned()] } else { vec![] },
    always_label: Some("Always allow ls".into()),
    desktop_ok,
    presence_fps: vec![],
  };
  c.req_sha256 = screen_of(&c).req_sha256();
  c
}

fn card_msg(n: u8, card_id: &str, permit: Option<PermitCardMsg>, closed: Option<&str>) -> ChatMessage {
  let mut m = msg_n(n, "manager");
  m.kind = "decision".into();
  m.author.machine = Some("mx_remote".into());
  m.decision = Some(DecisionRef { card_id: card_id.into(), spawn: None, permit, closed: closed.map(str::to_owned), poster: None, live: false });
  m
}

fn deliver(rig: &Rig, from: &Engine, msg: &ChatMessage) {
  post_chat(from, &my_id(&rig.a), msg);
  rig.a.poll_once().unwrap();
}

fn post_card(rig: &Rig, n: u8, c: PermitCardMsg) {
  deliver(rig, &rig.r, &card_msg(n, CARD, Some(c), None));
}

fn to_runner(rig: &Rig) -> Vec<(String, String, String)> {
  let s = rig.fake.0.lock().unwrap();
  s.msgs.iter().filter(|m| m.to == rig.rid).map(|m| (m.msg_id.clone(), m.from.clone(), m.body.clone())).collect()
}

fn sent_to_runner(rig: &Rig, before: usize) -> Value {
  let all = to_runner(rig);
  assert_eq!(all.len(), before + 1, "exactly one new message for the runner");
  let (msg_id, from, body) = all.last().unwrap().clone();
  let g = rig.r.lock();
  let id = g.identity.as_ref().unwrap().device_id.clone();
  let trusted: Vec<_> = g.st.trusted.iter().map(to_trusted).collect();
  let opened = open_message_checked(g.keys.as_ref().unwrap(), USER, &id, &from, &msg_id, &body, T0, &trusted, |_| None).unwrap();
  assert_eq!(opened.kind, "permitok");
  assert!(msg_id.starts_with("p_"));
  opened.payload
}

fn verify(rig: &Rig, payload: &Value, c: &PermitCardMsg, policy: DesktopPolicy) -> Result<PermittedAnswer, PermitRefusal> {
  let signed: SignedPermit = serde_json::from_value(payload.clone()).unwrap();
  let (dev_id, dev_fp, sign_pub) = {
    let g = rig.a.lock();
    let keys = g.keys.as_ref().unwrap();
    (g.identity.as_ref().unwrap().device_id.clone(), keys.fingerprint(), b64_decode(&keys.sign_pub_b64()).unwrap())
  };
  let record = PermitRecord {
    user_id: USER.to_string(),
    runner_id: c.runner_id.clone(),
    runner_fp: c.runner_fp.clone(),
    card_id: CARD.into(),
    session: c.session.clone(),
    nonce: c.nonce.clone(),
    issued_ms: c.exp_ms - 60_000,
    exp_ms: c.exp_ms,
    screen: screen_of(c),
  };
  let key = PinnedKey { user_id: USER.to_string(), device_id: dev_id, device_fp: dev_fp.clone(), spawn_pub: sign_pub };
  let sender = PermitSender { device_fp: &dev_fp, owner_device: true, kind: PermitSenderKind::Desktop, desktop: policy, presence: None };
  verify_permit(&signed, &record, &key, sender, &Trusting, T0)
}

fn answer(rig: &Rig, a: PermitAnswer, confirmed: bool) -> Result<(), String> {
  rig.a.answer_permit(CARD, a, confirmed)
}

fn counts(rig: &Rig) -> (usize, usize) {
  (to_runner(rig).len(), rig.fake.posts())
}

#[test]
fn a_signature_built_from_the_stored_card_passes_the_runner_verifier() {
  let rig = rig();
  let c = card(false, true);
  post_card(&rig, 2, c.clone());
  let before = to_runner(&rig).len();
  answer(&rig, PermitAnswer::Once, false).unwrap();
  let payload = sent_to_runner(&rig, before);
  let ok = verify(&rig, &payload, &c, DesktopPolicy::Safe).unwrap();
  assert_eq!(ok.choice(), PermitChoice::Yes);
  assert_eq!(ok.card_id(), CARD);
}

#[test]
fn deny_and_confirmed_always_verify_with_their_own_choice() {
  for (a, confirmed, want) in [(PermitAnswer::Deny, false, PermitChoice::No), (PermitAnswer::Always, true, PermitChoice::Always)] {
    let rig = rig();
    let c = card(false, true);
    post_card(&rig, 2, c.clone());
    let before = to_runner(&rig).len();
    answer(&rig, a, confirmed).unwrap();
    let payload = sent_to_runner(&rig, before);
    assert_eq!(verify(&rig, &payload, &c, DesktopPolicy::Safe).unwrap().choice(), want);
  }
}

#[test]
fn an_unknown_card_is_refused_and_nothing_is_sent() {
  let rig = rig();
  let before = counts(&rig);
  assert_eq!(answer(&rig, PermitAnswer::Once, false).unwrap_err(), UNKNOWN);
  assert_eq!(counts(&rig), before);
}

#[test]
fn a_closed_card_is_refused() {
  let rig = rig();
  post_card(&rig, 2, card(false, true));
  deliver(&rig, &rig.r, &card_msg(3, CARD, None, Some("answered on Phone")));
  let before = counts(&rig);
  assert_eq!(answer(&rig, PermitAnswer::Once, false).unwrap_err(), UNKNOWN);
  assert_eq!(counts(&rig), before);
}

#[test]
fn an_expired_card_is_refused() {
  let rig = rig();
  post_card(&rig, 2, card(false, true));
  rig.clock.set(EXP + 1);
  let before = counts(&rig);
  assert_eq!(answer(&rig, PermitAnswer::Once, false).unwrap_err(), "This permission card has expired");
  assert_eq!(counts(&rig), before);
}

#[test]
fn a_card_the_desktop_may_not_answer_is_refused_without_signing_or_sending() {
  let rig = rig();
  post_card(&rig, 2, card(true, false));
  let before = counts(&rig);
  for a in [PermitAnswer::Once, PermitAnswer::Always, PermitAnswer::Deny] {
    assert_eq!(answer(&rig, a, true).unwrap_err(), "Answer on your phone or in tmgr attach");
  }
  assert_eq!(counts(&rig), before);
}

#[test]
fn a_danger_card_needs_confirm_for_once_but_not_for_deny() {
  let rig = rig();
  post_card(&rig, 2, card(true, true));
  let before = counts(&rig);
  assert_eq!(answer(&rig, PermitAnswer::Once, false).unwrap_err(), CONFIRM);
  assert_eq!(counts(&rig), before);
  answer(&rig, PermitAnswer::Deny, false).unwrap();
  assert_eq!(to_runner(&rig).len(), before.0 + 1);
}

#[test]
fn a_confirmed_answer_to_a_danger_card_verifies_under_the_all_policy_only() {
  let rig = rig();
  let c = card(true, true);
  post_card(&rig, 2, c.clone());
  let before = to_runner(&rig).len();
  answer(&rig, PermitAnswer::Once, true).unwrap();
  let payload = sent_to_runner(&rig, before);
  assert_eq!(verify(&rig, &payload, &c, DesktopPolicy::Safe).unwrap_err(), PermitRefusal::DesktopNotAllowed);
  assert!(verify(&rig, &payload, &c, DesktopPolicy::All).is_ok());
}

#[test]
fn always_needs_a_confirm_and_an_offered_label() {
  let rig = rig();
  post_card(&rig, 2, card(false, true));
  assert_eq!(answer(&rig, PermitAnswer::Always, false).unwrap_err(), CONFIRM);
  let rig = self::rig();
  let mut c = card(false, true);
  c.always_label = None;
  c.req_sha256 = screen_of(&c).req_sha256();
  post_card(&rig, 2, c);
  assert_eq!(answer(&rig, PermitAnswer::Always, true).unwrap_err(), "Always is not offered for this card");
}

#[test]
fn a_card_whose_hash_does_not_match_its_screen_is_not_stored() {
  let rig = rig();
  let mut c = card(false, true);
  c.text = "something else".into();
  post_card(&rig, 2, c);
  assert_eq!(answer(&rig, PermitAnswer::Once, false).unwrap_err(), UNKNOWN);
}

#[test]
fn a_card_from_a_device_that_is_not_a_runner_is_not_stored() {
  let rig = rig();
  let d2 = join_as(&rig.fake, &rig.clock, &rig.a, "Desktop 2", "desktop");
  let mut m = card_msg(2, CARD, Some(card(false, true)), None);
  m.author.kind = "owner".into();
  m.author.machine = None;
  deliver(&rig, &d2, &m);
  assert_eq!(answer(&rig, PermitAnswer::Once, false).unwrap_err(), UNKNOWN);
}

#[test]
fn a_second_runner_cannot_take_over_or_close_a_card() {
  let rig = rig();
  let r2 = join_as(&rig.fake, &rig.clock, &rig.a, "Runner 2", "runner");
  let r2id = my_id(&r2);
  post_card(&rig, 2, card(false, true));
  let mut m = card_msg(3, CARD, Some(card(false, true)), None);
  m.room.machine = "mx_other".into();
  m.author.machine = Some("mx_other".into());
  deliver(&rig, &r2, &m);
  let mut close = card_msg(4, CARD, None, Some("gone"));
  close.room.machine = "mx_other".into();
  close.author.machine = Some("mx_other".into());
  deliver(&rig, &r2, &close);
  let before = (to_runner(&rig).len(), rig.fake.queue_len(&r2id));
  answer(&rig, PermitAnswer::Once, false).unwrap();
  assert_eq!((to_runner(&rig).len(), rig.fake.queue_len(&r2id)), (before.0 + 1, before.1));
}

#[test]
fn the_answer_goes_to_the_posting_runner_only() {
  let rig = rig();
  let d2 = join_as(&rig.fake, &rig.clock, &rig.a, "Desktop 2", "desktop");
  let r2 = join_as(&rig.fake, &rig.clock, &rig.a, "Runner 2", "runner");
  post_card(&rig, 2, card(false, true));
  let q = |rig: &Rig| (rig.fake.queue_len(&rig.rid), rig.fake.queue_len(&my_id(&d2)), rig.fake.queue_len(&my_id(&r2)));
  let before = q(&rig);
  answer(&rig, PermitAnswer::Once, false).unwrap();
  assert_eq!(q(&rig), (before.0 + 1, before.1, before.2));
}

#[test]
fn a_card_is_answered_once() {
  let rig = rig();
  post_card(&rig, 2, card(false, true));
  answer(&rig, PermitAnswer::Once, false).unwrap();
  assert_eq!(answer(&rig, PermitAnswer::Deny, false).unwrap_err(), UNKNOWN);
}

#[test]
fn the_command_takes_only_a_card_a_choice_and_a_confirm() {
  let _: fn(&Engine, &str, PermitAnswer, bool) -> Result<(), String> = Engine::answer_permit;
}

#[test]
fn the_book_is_bounded() {
  let rig = rig();
  for n in 0..(MAX_PERMITS + 10) {
    let id = format!("card-{n}");
    let mut m = card_msg(2, &id, Some(card(false, true)), None);
    m.id = format!("m_{n:032x}");
    deliver(&rig, &rig.r, &m);
  }
  assert_eq!(rig.a.lock().permits.len(), MAX_PERMITS);
}

fn shown(rig: &Rig) -> Vec<DecisionRef> {
  let h = rig.a.history(&HistoryParams { room: card_msg(2, CARD, None, None).room, before: None, limit: None }).unwrap();
  h.into_iter().filter_map(|m| m.decision).collect()
}

#[test]
fn history_marks_a_card_live_only_while_it_is_in_the_book() {
  let rig = rig();
  post_card(&rig, 2, card(false, true));
  let d = shown(&rig);
  assert!(d[0].live);
  assert_eq!(d[0].poster.as_deref(), Some(rig.rid.as_str()));
  rig.a.lock().permits = Default::default();
  assert!(!shown(&rig)[0].live);
}

#[test]
fn only_the_message_matching_the_booked_nonce_is_live() {
  let rig = rig();
  post_card(&rig, 2, card(false, true));
  let mut newer = card(false, true);
  newer.nonce = "cd".repeat(16);
  deliver(&rig, &rig.r, &card_msg(3, CARD, Some(newer), None));
  let live: Vec<bool> = shown(&rig).iter().map(|d| d.live).collect();
  assert_eq!(live, vec![false, true]);
}

#[test]
fn an_answered_or_closed_card_is_not_live() {
  let rig = rig();
  post_card(&rig, 2, card(false, true));
  answer(&rig, PermitAnswer::Once, false).unwrap();
  assert!(!shown(&rig)[0].live);
}

#[test]
fn a_remote_cannot_preset_live_or_poster() {
  let rig = rig();
  let mut m = card_msg(2, CARD, Some(card(false, true)), None);
  let d = m.decision.as_mut().unwrap();
  d.live = true;
  d.poster = Some("someone-else".into());
  let mut bad = card(false, true);
  bad.req_sha256 = "0".repeat(64);
  d.permit = Some(bad);
  deliver(&rig, &rig.r, &m);
  let d = &shown(&rig)[0];
  assert!(!d.live);
  assert_eq!(d.poster.as_deref(), Some(rig.rid.as_str()));
}

#[test]
fn a_refresh_request_is_the_exact_payload_sealed_only_to_the_poster() {
  let rig = rig();
  let c = card(false, true);
  post_card(&rig, 2, c.clone());
  let before = counts(&rig);
  rig.a.refresh_permit(CARD).unwrap();
  let payload = sent_to_runner(&rig, before.0);
  assert_eq!(payload, serde_json::json!({"v": 1, "refresh": CARD, "nonce": c.nonce}));
  assert_eq!(rig.fake.posts(), before.1 + 1);
}

#[test]
fn a_refresh_is_sent_for_an_expired_card_and_one_the_desktop_cannot_answer() {
  let rig = rig();
  post_card(&rig, 2, card(true, false));
  rig.clock.set(EXP + 1);
  let before = to_runner(&rig).len();
  rig.a.refresh_permit(CARD).unwrap();
  assert_eq!(to_runner(&rig).len(), before + 1);
}

#[test]
fn a_refresh_keeps_the_card_so_the_updated_one_can_still_be_answered() {
  let rig = rig();
  post_card(&rig, 2, card(false, true));
  rig.a.refresh_permit(CARD).unwrap();
  rig.a.refresh_permit(CARD).unwrap();
  answer(&rig, PermitAnswer::Deny, false).unwrap();
}

#[test]
fn a_refresh_for_an_unknown_or_closed_card_is_refused_and_nothing_is_sent() {
  let rig = rig();
  let before = counts(&rig);
  assert_eq!(rig.a.refresh_permit(CARD).unwrap_err(), UNKNOWN);
  assert_eq!(counts(&rig), before);
  post_card(&rig, 2, card(false, true));
  deliver(&rig, &rig.r, &card_msg(3, CARD, None, Some("answered on Phone")));
  let before2 = counts(&rig);
  assert_eq!(rig.a.refresh_permit(CARD).unwrap_err(), UNKNOWN);
  assert_eq!(counts(&rig), before2);
}

struct Touch {
  rig: Rig,
  e: crate::manager::presence::soft::SoftEnclave,
  p: Arc<crate::manager::presence::Presence>,
  _dir: tempfile::TempDir,
}

fn touch(set_up: bool) -> Touch {
  use crate::manager::presence::{soft::SoftEnclave, Presence};
  let dir = tempfile::tempdir().unwrap();
  let e = SoftEnclave::default();
  let p = Arc::new(Presence::new(Box::new(e.clone()), dir.path().to_path_buf(), Box::new(crate::manager::presence::system_now)));
  if set_up {
    p.setup().unwrap();
  }
  let (fake, clock) = (Fake::default(), Clock::new(T0));
  let a = engine_with(&fake, &clock, "Desktop A").with_presence(Some(p.clone()));
  let r = join_as(&fake, &clock, &a, "Runner", "runner");
  let rid = my_id(&r);
  Touch { rig: Rig { fake, clock, a, r, rid }, e, p, _dir: dir }
}

fn own_fp(rig: &Rig) -> String {
  rig.a.lock().keys.as_ref().unwrap().fingerprint()
}

fn presence_card(rig: &Rig) -> PermitCardMsg {
  let mut c = card(true, false);
  c.presence_fps = vec![own_fp(rig)];
  c
}

fn verify_presence(t: &Touch, payload: &Value, c: &PermitCardMsg) -> Result<PermittedAnswer, PermitRefusal> {
  let signed: SignedPermit = serde_json::from_value(payload.clone()).unwrap();
  let (dev_id, dev_fp) = {
    let g = t.rig.a.lock();
    (g.identity.as_ref().unwrap().device_id.clone(), g.keys.as_ref().unwrap().fingerprint())
  };
  let record = PermitRecord {
    user_id: USER.to_string(),
    runner_id: c.runner_id.clone(),
    runner_fp: c.runner_fp.clone(),
    card_id: CARD.into(),
    session: c.session.clone(),
    nonce: c.nonce.clone(),
    issued_ms: c.exp_ms - 60_000,
    exp_ms: c.exp_ms,
    screen: screen_of(c),
  };
  let chat = PinnedKey { user_id: USER.to_string(), device_id: dev_id.clone(), device_fp: dev_fp.clone(), spawn_pub: vec![] };
  let presence = PinnedKey { user_id: USER.to_string(), device_id: dev_id, device_fp: dev_fp.clone(), spawn_pub: t.p.public_key().unwrap().unwrap() };
  let sender = PermitSender { device_fp: &dev_fp, owner_device: true, kind: PermitSenderKind::Desktop, desktop: DesktopPolicy::Presence, presence: Some(&presence) };
  verify_permit(&signed, &record, &chat, sender, &Trusting, T0)
}

#[test]
fn a_touch_id_answer_on_a_danger_card_passes_the_runner_verifier() {
  let t = touch(true);
  let c = presence_card(&t.rig);
  post_card(&t.rig, 2, c.clone());
  let before = to_runner(&t.rig).len();
  answer(&t.rig, PermitAnswer::Once, true).unwrap();
  let payload = sent_to_runner(&t.rig, before);
  assert_eq!(payload["k"], "presence");
  assert_eq!(verify_presence(&t, &payload, &c).unwrap().choice(), PermitChoice::Yes);
  assert_eq!(t.e.prompts(), 1);
}

#[test]
fn a_danger_card_without_desktop_approvals_set_up_is_refused_before_any_prompt() {
  let t = touch(false);
  post_card(&t.rig, 2, presence_card(&t.rig));
  let before = counts(&t.rig);
  assert_eq!(answer(&t.rig, PermitAnswer::Deny, false).unwrap_err(), crate::manager::presence::NOT_SET_UP);
  assert_eq!(counts(&t.rig), before);
  assert_eq!(t.e.prompts(), 0);

  let rig = rig();
  post_card(&rig, 2, presence_card(&rig));
  assert_eq!(answer(&rig, PermitAnswer::Deny, false).unwrap_err(), crate::manager::presence::NOT_SET_UP);
}

#[test]
fn a_cancelled_touch_id_prompt_sends_nothing_and_the_card_stays_answerable() {
  let t = touch(true);
  let c = presence_card(&t.rig);
  post_card(&t.rig, 2, c.clone());
  *t.e.0.refuse.lock().unwrap() = Some("cancelled".into());
  let before = counts(&t.rig);
  assert_eq!(answer(&t.rig, PermitAnswer::Deny, false).unwrap_err(), "cancelled");
  assert_eq!(counts(&t.rig), before);
  *t.e.0.refuse.lock().unwrap() = None;
  answer(&t.rig, PermitAnswer::Deny, false).unwrap();
  let payload = sent_to_runner(&t.rig, before.0);
  assert_eq!(verify_presence(&t, &payload, &c).unwrap().choice(), PermitChoice::No);
}

#[test]
fn the_relay_stays_usable_while_the_touch_id_prompt_is_open() {
  let t = touch(true);
  post_card(&t.rig, 2, presence_card(&t.rig));
  let (entered_tx, entered_rx) = std::sync::mpsc::channel();
  let (release_tx, release_rx) = std::sync::mpsc::channel();
  *t.e.0.hold.lock().unwrap() = Some((entered_tx, release_rx));
  std::thread::scope(|s| {
    let answering = s.spawn(|| answer(&t.rig, PermitAnswer::Deny, false));
    entered_rx.recv().unwrap();
    assert!(t.rig.a.core.try_lock().is_ok(), "the relay lock is free during the prompt");
    t.rig.a.poll_once().unwrap();
    release_tx.send(()).unwrap();
    answering.join().unwrap().unwrap();
  });
}

#[test]
fn one_touch_id_window_covers_the_next_card() {
  let t = touch(true);
  post_card(&t.rig, 2, presence_card(&t.rig));
  answer(&t.rig, PermitAnswer::Deny, false).unwrap();
  post_card(&t.rig, 3, presence_card(&t.rig));
  answer(&t.rig, PermitAnswer::Deny, false).unwrap();
  assert_eq!(t.e.prompts(), 1);
}

#[test]
fn a_card_without_this_desktop_in_its_presence_list_keeps_the_chat_key() {
  let t = touch(true);
  let c = card(false, true);
  post_card(&t.rig, 2, c.clone());
  let before = to_runner(&t.rig).len();
  answer(&t.rig, PermitAnswer::Once, false).unwrap();
  let payload = sent_to_runner(&t.rig, before);
  assert!(payload.get("k").is_none(), "a chat answer keeps the old wire form");
  assert_eq!(verify(&t.rig, &payload, &c, DesktopPolicy::Presence).unwrap().choice(), PermitChoice::Yes);
  assert_eq!(t.e.prompts(), 0);

  let mut other = card(true, false);
  other.presence_fps = vec!["e".repeat(64)];
  post_card(&t.rig, 3, other);
  assert_eq!(answer(&t.rig, PermitAnswer::Deny, false).unwrap_err(), "Answer on your phone or in tmgr attach");
}

#[test]
fn publishing_sends_a_double_signed_presence_key_to_each_runner() {
  use tmgr_chat_relay::trust::{KeyPurpose, SpawnKey, SpawnStatement};
  let t = touch(true);
  let before = to_runner(&t.rig).len();
  assert_eq!(t.rig.a.publish_presence().unwrap(), 1);
  let all = to_runner(&t.rig);
  assert_eq!(all.len(), before + 1);
  let (msg_id, from, body) = all.last().unwrap().clone();
  let opened = {
    let g = t.rig.r.lock();
    let id = g.identity.as_ref().unwrap().device_id.clone();
    let trusted: Vec<_> = g.st.trusted.iter().map(to_trusted).collect();
    open_message_checked(g.keys.as_ref().unwrap(), USER, &id, &from, &msg_id, &body, T0, &trusted, |_| None).unwrap()
  };
  assert_eq!(opened.kind, "spawnkey");
  let key: SpawnKey = serde_json::from_value(opened.payload).unwrap();
  let st = SpawnStatement::parse(&key.statement).unwrap();
  let public = t.p.public_key().unwrap().unwrap();
  assert_eq!(st.purpose, KeyPurpose::Presence);
  assert_eq!(st.device_fp, own_fp(&t.rig));
  assert_eq!(b64_decode(&st.spawn_pub_b64).unwrap(), public);
  let sign_pub = b64_decode(&t.rig.a.lock().keys.as_ref().unwrap().sign_pub_b64()).unwrap();
  tmgr_phone::verify(&sign_pub, &key.statement, &key.sig).unwrap();
  tmgr_phone::verify(&public, &key.statement, &key.spawn_sig).unwrap();
}

#[test]
fn publishing_without_setup_or_a_runner_is_refused() {
  assert_eq!(touch(false).rig.a.publish_presence().unwrap_err(), crate::manager::presence::NOT_SET_UP);
  assert_eq!(rig().a.publish_presence().unwrap_err(), crate::manager::presence::NOT_SET_UP);
}
