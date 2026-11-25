use super::desk::*;
use super::sim::*;
use super::*;
use crate::attach::status::{AskPoll, AskQuestion};
use std::sync::Mutex;
use std::time::Duration;
use tmgr_ipc::chat::{PollAnswer, PollMsg, PollPick};

struct Fake {
    sim: Mutex<Sim>,
    send_ok: bool,
    stuck_open: bool,
    allowed: std::sync::atomic::AtomicBool,
    drift_before_send: bool,
}

impl Fake {
    fn new(sim: Sim) -> Fake {
        Fake {
            sim: Mutex::new(sim),
            send_ok: true,
            stuck_open: false,
            allowed: true.into(),
            drift_before_send: false,
        }
    }

    fn keys(&self) -> Vec<Vec<u8>> {
        self.sim.lock().unwrap().keys.clone()
    }
}

impl Terminal for Fake {
    fn screen(&self) -> Option<Screen> {
        parse_screen(&self.sim.lock().unwrap().render())
    }

    fn send_checked(&self, keys: &[u8], check: &dyn Fn(Option<&Screen>) -> bool) -> Sent {
        let mut sim = self.sim.lock().unwrap();
        if self.drift_before_send {
            sim.press(b"\x1b[B");
        }
        if !check(parse_screen(&sim.render()).as_ref()) {
            return Sent::Changed;
        }
        if !self.send_ok {
            return Sent::Refused;
        }
        sim.press(keys);
        Sent::Ok
    }

    fn allowed(&self) -> bool {
        self.allowed.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn poll_open(&self) -> bool {
        self.stuck_open || !self.sim.lock().unwrap().gone
    }
}

const FAST: Pace = Pace {
    poll: Duration::from_millis(1),
    step: Duration::from_millis(30),
    confirm: Duration::from_millis(40),
    deadline: Duration::from_secs(5),
};

fn answering(qs: Vec<AskQuestion>, picks: Vec<PollPick>) -> Answering {
    Answering {
        card_id: "poll_x".into(),
        session: "s_aaaaaaaaaa".into(),
        tool_use_id: "t".into(),
        questions: qs,
        picks,
    }
}

fn run(t: &Fake, qs: Vec<AskQuestion>, picks: Vec<PollPick>) -> Outcome {
    run_answer_with(t, &answering(qs, picks), &FAST)
}

#[test]
fn executor_single() {
    let t = Fake::new(Sim::new(vec![scope_q()], 100));
    let out = run(&t, vec![scope_q()], vec![pick(0, &[1], None)]);
    assert_eq!(
        out,
        Outcome::Applied {
            picked: "All sources at once".into()
        }
    );
    assert_eq!(t.sim.lock().unwrap().answers, ["All sources at once"]);
}

#[test]
fn executor_multi_and_review() {
    let qs = vec![sources_q(), order_q()];
    let t = Fake::new(Sim::new(qs.clone(), 100));
    let picks = vec![pick(0, &[0, 2], None), pick(1, &[1], None)];
    let out = run(&t, qs, picks);
    assert_eq!(
        out,
        Outcome::Applied {
            picked: "Jira, Confluence; GitLab".into()
        }
    );
    assert!(t.sim.lock().unwrap().submitted);
}

#[test]
fn executor_other_text() {
    let t = Fake::new(Sim::new(vec![scope_q()], 100));
    let out = run(&t, vec![scope_q()], vec![pick(0, &[], Some("my own plan"))]);
    assert_eq!(
        out,
        Outcome::Applied {
            picked: "my own plan".into()
        }
    );
}

#[test]
fn executor_summary_drops_the_recommended_suffix() {
    let t = Fake::new(Sim::new(vec![scope_q()], 100));
    let out = run(&t, vec![scope_q()], vec![pick(0, &[0], None)]);
    assert_eq!(
        out,
        Outcome::Applied {
            picked: "Jira+GitLab+Confluence".into()
        }
    );
}

#[test]
fn executor_stops_when_the_picker_disappears_midway() {
    let mut sim = Sim::new(vec![scope_q()], 100);
    sim.vanish_after = Some(1);
    let t = Fake::new(sim);
    assert_eq!(
        run(&t, vec![scope_q()], vec![pick(0, &[2], None)]),
        Outcome::Stale
    );
    assert_eq!(t.keys().len(), 1);
}

#[test]
fn executor_fails_when_the_screen_does_not_follow() {
    let mut sim = Sim::new(vec![scope_q()], 100);
    sim.ignore = 1;
    let t = Fake::new(sim);
    assert_eq!(
        run(&t, vec![scope_q()], vec![pick(0, &[2], None)]),
        Outcome::Failed("the screen did not follow".into())
    );
    assert_eq!(t.keys().len(), 1);
}

#[test]
fn executor_types_nothing_for_a_different_question() {
    let t = Fake::new(Sim::new(vec![scope_q()], 100));
    assert_eq!(
        run(&t, vec![sources_q()], vec![pick(0, &[0], None)]),
        Outcome::Stale
    );
    assert!(t.keys().is_empty());
}

#[test]
fn executor_types_nothing_when_the_picker_is_already_gone() {
    let mut sim = Sim::new(vec![scope_q()], 100);
    sim.gone = true;
    let t = Fake::new(sim);
    assert_eq!(
        run(&t, vec![scope_q()], vec![pick(0, &[0], None)]),
        Outcome::Stale
    );
    assert!(t.keys().is_empty());
}

#[test]
fn executor_fails_when_a_key_is_not_accepted() {
    let mut t = Fake::new(Sim::new(vec![scope_q()], 100));
    t.send_ok = false;
    assert_eq!(
        run(&t, vec![scope_q()], vec![pick(0, &[1], None)]),
        Outcome::Failed("the agent is not reading its input".into())
    );
}

#[test]
fn executor_sends_nothing_when_the_screen_changes_before_the_key() {
    let mut t = Fake::new(Sim::new(vec![scope_q()], 100));
    t.drift_before_send = true;
    assert_eq!(
        run(&t, vec![scope_q()], vec![pick(0, &[2], None)]),
        Outcome::Stale
    );
    assert_eq!(t.keys(), [b"\x1b[B".to_vec()]);
}

#[test]
fn executor_stops_when_consent_is_turned_off() {
    let t = Fake::new(Sim::new(vec![scope_q()], 100));
    t.allowed.store(false, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        run(&t, vec![scope_q()], vec![pick(0, &[2], None)]),
        Outcome::Failed("reading consent was turned off".into())
    );
    assert!(t.keys().is_empty());
}

#[test]
fn executor_needs_the_agent_to_confirm() {
    let mut t = Fake::new(Sim::new(vec![scope_q()], 100));
    t.stuck_open = true;
    assert_eq!(
        run(&t, vec![scope_q()], vec![pick(0, &[0], None)]),
        Outcome::Failed("the agent did not confirm".into())
    );
}

fn poll(tool: &str, qs: Vec<AskQuestion>) -> AskPoll {
    AskPoll {
        tool_use_id: tool.into(),
        questions: qs,
    }
}

fn shown(qs: Vec<AskQuestion>) -> Option<Screen> {
    parse_screen(&Sim::new(qs, 100).render())
}

const SID: &str = "s_aaaaaaaaaa";

fn answer(id: &str, picks: Vec<PollPick>) -> PollAnswer {
    PollAnswer {
        v: 1,
        card_id: id.into(),
        answers: picks,
    }
}

fn posted(desk: &mut PollDesk, tool: &str, fg: bool) -> (String, PollMsg) {
    let p = poll(tool, vec![scope_q()]);
    match desk
        .observe(SID, Some(&p), shown(vec![scope_q()]).as_ref(), fg)
        .pop()
    {
        Some(Action::Post { card_id, poll }) => (card_id, poll),
        other => panic!("{other:?}"),
    }
}

#[test]
fn card_ids_are_stable_and_scoped() {
    let q = [scope_q()];
    let a = card_id(SID, "tool1", &q);
    assert!(a.starts_with("poll_") && a.len() == 37);
    assert_eq!(a, card_id(SID, "tool1", &q));
    assert_ne!(a, card_id(SID, "tool2", &q));
    assert_ne!(a, card_id("s_bbbbbbbbbb", "tool1", &q));
    assert_ne!(a, card_id(SID, "tool1", &[sources_q()]));
    let mut other = scope_q();
    other.options[1].label = "Else".into();
    assert_ne!(a, card_id(SID, "tool1", &[other]));
}

#[test]
fn a_card_is_posted_once_and_can_be_answered() {
    let mut d = PollDesk::default();
    let (id, msg) = posted(&mut d, "t1", true);
    assert!(msg.can_answer && msg.why_not.is_none());
    assert_eq!(msg.questions[0].options[0].label, "Jira+GitLab+Confluence");
    assert!(msg.questions[0].options[0].recommended);
    assert!(!msg.questions[0].options[1].recommended);
    let p = poll("t1", vec![scope_q()]);
    assert!(d
        .observe(SID, Some(&p), shown(vec![scope_q()]).as_ref(), true)
        .is_empty());
    let a = d
        .begin_answer(&id, &answer(&id, vec![pick(0, &[0], None)]), "dev")
        .unwrap();
    assert_eq!(a.questions[0].options[0].label, scope_q().options[0].label);
}

#[test]
fn an_unanswerable_card_is_reposted_once_when_it_becomes_answerable() {
    let mut d = PollDesk::default();
    let (id, msg) = posted(&mut d, "t1", false);
    assert!(!msg.can_answer);
    assert_eq!(msg.why_not.as_deref(), Some("open the pane to answer"));
    assert_eq!(
        d.begin_answer(&id, &answer(&id, vec![pick(0, &[0], None)]), "dev"),
        Err(Reject::NotAnswerable)
    );
    let p = poll("t1", vec![scope_q()]);
    let screen = shown(vec![scope_q()]);
    let again = d.observe(SID, Some(&p), screen.as_ref(), true);
    assert!(
        matches!(&again[..], [Action::Post { card_id, poll }] if *card_id == id && poll.can_answer)
    );
    assert!(d.observe(SID, Some(&p), screen.as_ref(), true).is_empty());
    assert!(d
        .begin_answer(&id, &answer(&id, vec![pick(0, &[0], None)]), "dev")
        .is_ok());
}

#[test]
fn an_unknown_screen_is_not_answerable() {
    let mut d = PollDesk::default();
    let p = poll("t1", vec![scope_q()]);
    let out = d.observe(SID, Some(&p), None, true);
    assert!(matches!(&out[..], [Action::Post { poll, .. }] if !poll.can_answer));
}

#[test]
fn a_vanished_card_closes_as_terminal_and_a_gone_session_as_gone() {
    let mut d = PollDesk::default();
    let (id, _) = posted(&mut d, "t1", true);
    let out = d.observe(SID, None, None, true);
    assert_eq!(
        out,
        [Action::Close {
            card_id: id.clone(),
            session: SID.into(),
            closed: "terminal",
            picked: None,
            why: None
        }]
    );
    assert!(d.observe(SID, None, None, true).is_empty());
    let p = poll("t1", vec![scope_q()]);
    assert!(d
        .observe(SID, Some(&p), shown(vec![scope_q()]).as_ref(), true)
        .is_empty());

    let (id2, _) = posted(&mut d, "t2", true);
    let out = d.gone(SID);
    assert!(matches!(&out[..], [Action::Close { card_id, closed: "gone", .. }] if *card_id == id2));
    assert!(d.is_empty());
}

#[test]
fn an_answering_card_is_left_to_the_worker() {
    let mut d = PollDesk::default();
    let (id, _) = posted(&mut d, "t1", true);
    d.begin_answer(&id, &answer(&id, vec![pick(0, &[1], None)]), "dev")
        .unwrap();
    assert!(d.observe(SID, None, None, true).is_empty());
    let fin = d.finish(
        &id,
        Outcome::Applied {
            picked: "All sources at once".into(),
        },
    );
    assert!(matches!(
        fin,
        Some(Action::Close {
            closed: "applied",
            picked: Some(_),
            ..
        })
    ));
    assert_eq!(d.finish(&id, Outcome::Stale), None);
    assert!(d.gone(SID).is_empty());
}

#[test]
fn finish_reports_failed_with_a_reason() {
    let mut d = PollDesk::default();
    let (id, _) = posted(&mut d, "t1", true);
    d.begin_answer(&id, &answer(&id, vec![pick(0, &[1], None)]), "dev")
        .unwrap();
    match d.finish(&id, Outcome::Failed("the screen did not follow".into())) {
        Some(Action::Close { closed, why, .. }) => {
            assert_eq!(closed, "failed");
            assert_eq!(why.as_deref(), Some("the screen did not follow"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_second_answer_is_a_duplicate_and_a_closed_card_never_types() {
    let mut d = PollDesk::default();
    let (id, _) = posted(&mut d, "t1", true);
    let a = answer(&id, vec![pick(0, &[1], None)]);
    d.begin_answer(&id, &a, "dev").unwrap();
    assert_eq!(d.begin_answer(&id, &a, "dev"), Err(Reject::Duplicate));
    assert_eq!(d.begin_answer(&id, &a, "other"), Err(Reject::Duplicate));
    d.finish(&id, Outcome::Stale);
    assert_eq!(d.begin_answer(&id, &a, "dev"), Err(Reject::Duplicate));
    assert_eq!(d.begin_answer("poll_nope", &a, "dev"), Err(Reject::Unknown));
}

#[test]
fn invalid_answers_are_rejected_and_the_card_stays_open() {
    let mut d = PollDesk::default();
    let p = poll("t1", vec![sources_q(), scope_q()]);
    let screen = shown(vec![sources_q(), scope_q()]);
    let id = match d.observe(SID, Some(&p), screen.as_ref(), true).pop() {
        Some(Action::Post { card_id, .. }) => card_id,
        other => panic!("{other:?}"),
    };
    let cases: Vec<Vec<PollPick>> = vec![
        vec![pick(0, &[0], None)],
        vec![pick(0, &[0], None), pick(0, &[1], None)],
        vec![pick(0, &[9], None), pick(1, &[0], None)],
        vec![pick(0, &[], Some("x")), pick(1, &[0], None)],
        vec![pick(0, &[0], None), pick(1, &[0, 1], None)],
        vec![pick(0, &[0], None), pick(1, &[0], Some("t"))],
        vec![pick(0, &[0], None), pick(1, &[], Some("\u{7}\u{3}"))],
        vec![pick(0, &[0], None), pick(5, &[0], None)],
    ];
    for picks in cases {
        let r = d.begin_answer(&id, &answer(&id, picks.clone()), "dev");
        assert!(matches!(r, Err(Reject::Invalid(_))), "{picks:?} {r:?}");
    }
    let wrong = answer("poll_other", vec![pick(0, &[0], None), pick(1, &[0], None)]);
    assert!(matches!(
        d.begin_answer(&id, &wrong, "dev"),
        Err(Reject::Invalid(_))
    ));
    let ok = answer(
        &id,
        vec![pick(0, &[0, 2], None), pick(1, &[], Some("  ab "))],
    );
    let a = d.begin_answer(&id, &ok, "dev").unwrap();
    assert_eq!(a.picks[1].other.as_deref(), Some("ab"));
}

#[test]
fn open_cards_and_the_closed_ring_are_bounded() {
    let mut d = PollDesk::default();
    let screen = shown(vec![scope_q()]);
    for n in 0..MAX_OPEN + 3 {
        let sid = format!("s_{n:010}");
        let p = poll("t", vec![scope_q()]);
        d.observe(&sid, Some(&p), screen.as_ref(), true);
    }
    assert_eq!(d.sessions().len(), MAX_OPEN);
    for n in 0..300 {
        let p = poll(&format!("t{n}"), vec![scope_q()]);
        d.observe("s_ring", Some(&p), screen.as_ref(), true);
        d.observe("s_ring", None, None, true);
    }
    assert!(d.closed.len() <= 128);
}

#[test]
fn a_changed_question_under_the_same_tool_id_replaces_the_card() {
    let mut d = PollDesk::default();
    let (old, _) = posted(&mut d, "t1", true);
    let p = poll("t1", vec![sources_q()]);
    let out = d.observe(SID, Some(&p), shown(vec![sources_q()]).as_ref(), true);
    assert_eq!(out.len(), 2);
    assert!(
        matches!(&out[0], Action::Close { card_id, closed: "terminal", .. } if *card_id == old)
    );
    assert!(matches!(&out[1], Action::Post { card_id, .. } if *card_id != old));
}

#[test]
fn displayed_fields_are_stripped_and_capped() {
    let mut q = scope_q();
    q.header = "He\u{202e}ad\u{1b}[31m".into();
    q.question = "Wh\u{202e}ich\u{7}?".into();
    q.options[1].label = "Opt\u{202e}ion\u{1b}".into();
    q.options[1].description = "de\u{202e}sc\n\u{3}".into();
    let extra: Vec<AskQuestion> = (0..6).map(|_| q.clone()).collect();
    let mut d = PollDesk::default();
    let p = poll("t1", extra);
    let out = d.observe(SID, Some(&p), None, true);
    let Some(Action::Post { poll, .. }) = out.first() else {
        panic!("{out:?}")
    };
    assert_eq!(poll.questions.len(), 4);
    let q = &poll.questions[0];
    assert_eq!(q.header, "Head[31m");
    assert_eq!(q.question, "Which?");
    assert_eq!(q.options[1].label, "Option");
    assert_eq!(q.options[1].description, "desc");
}

#[test]
fn a_closed_summary_and_reason_are_stripped() {
    let mut d = PollDesk::default();
    let (id, _) = posted(&mut d, "t1", true);
    d.begin_answer(&id, &answer(&id, vec![pick(0, &[1], None)]), "dev")
        .unwrap();
    match d.finish(&id, Outcome::Failed("bad\u{202e}\u{1b}[0m".into())) {
        Some(Action::Close { why, .. }) => assert_eq!(why.as_deref(), Some("bad[0m")),
        other => panic!("{other:?}"),
    }
}

#[test]
fn fail_open_closes_only_an_open_card() {
    let mut d = PollDesk::default();
    let (id, _) = posted(&mut d, "t1", true);
    d.begin_answer(&id, &answer(&id, vec![pick(0, &[1], None)]), "dev")
        .unwrap();
    assert_eq!(d.fail_open(&id, "busy"), None);
    let (id2, _) = posted(&mut d, "t2", true);
    assert!(matches!(
        d.fail_open(&id2, "busy"),
        Some(Action::Close { closed: "failed", why: Some(w), .. }) if w == "busy"
    ));
    assert_eq!(d.fail_open(&id2, "busy"), None);
}
