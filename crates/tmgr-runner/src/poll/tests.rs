use super::sim::*;
use super::*;

const SINGLE: &str = include_str!("../../profiles/screens/claude-askuser-single.txt");
const DOWN1: &str = include_str!("../../profiles/screens/claude-askuser-single-down1.txt");
const OTHER_ROW: &str = include_str!("../../profiles/screens/claude-askuser-single-other-row.txt");
const OTHER_TYPED: &str =
    include_str!("../../profiles/screens/claude-askuser-single-other-typed.txt");
const CHAT_ROW: &str = include_str!("../../profiles/screens/claude-askuser-single-chat-row.txt");
const SHORT: &str = include_str!("../../profiles/screens/claude-askuser-single-short.txt");
const NARROW: &str = include_str!("../../profiles/screens/claude-askuser-narrow80.txt");
const MULTI_OPEN: &str = include_str!("../../profiles/screens/claude-askuser-multi-open.txt");
const MULTI_CHECKED: &str = include_str!("../../profiles/screens/claude-askuser-multi-checked.txt");
const MULTI_NEXT: &str = include_str!("../../profiles/screens/claude-askuser-multi-next-row.txt");
const MULTI_Q2: &str = include_str!("../../profiles/screens/claude-askuser-multi-q2.txt");
const MULTI_REVIEW: &str = include_str!("../../profiles/screens/claude-askuser-multi-review.txt");

fn rows(s: &str) -> Vec<String> {
    s.lines().map(String::from).collect()
}

fn picker(s: &str) -> Picker {
    parse_picker(&rows(s)).expect("picker")
}

#[test]
fn parses_single_select() {
    let p = picker(SINGLE);
    assert_eq!(p.header, "Scope");
    assert!(p.tabs.is_empty());
    assert!(!p.multi);
    assert_eq!(p.cursor, Cursor::Option(0));
    assert_eq!(
        p.question,
        "Which scope should the rework cover in the hub service, where one service calls all external sources and serves data to everyone?"
    );
    let labels: Vec<&str> = p.options.iter().map(|o| o.label.as_str()).collect();
    assert_eq!(
        labels,
        [
            "Jira+GitLab+Confluence (Recommended)",
            "All sources at once",
            "Only Jira+GitLab",
            "Type something."
        ]
    );
    assert!(p.options[3].is_text_row);
    assert!(p.options.iter().take(3).all(|o| !o.is_text_row));
    assert!(p.options.iter().all(|o| o.checked.is_none()));
    assert_eq!(p.options[3].number, 4);
}

#[test]
fn cursor_positions() {
    assert_eq!(picker(DOWN1).cursor, Cursor::Option(1));
    assert_eq!(picker(OTHER_ROW).cursor, Cursor::Option(3));
    assert_eq!(picker(CHAT_ROW).cursor, Cursor::Chat);
}

#[test]
fn typed_text_shows_in_text_row() {
    let p = picker(OTHER_TYPED);
    assert_eq!(p.options[3].label, "hello");
    assert!(p.options[3].is_text_row);
    assert_eq!(p.cursor, Cursor::Option(3));
    assert!(text_row_has(&p, "hello"));
}

#[test]
fn parses_short_and_narrow() {
    let p = picker(SHORT);
    assert_eq!(p.header, "Pick");
    assert_eq!(p.question, "Which color?");
    assert_eq!(p.options.len(), 3);
    let n = picker(NARROW);
    assert_eq!(n.header, "Narrow");
    assert_eq!(
        n.question,
        "A fairly long question to wrap on a narrow terminal screen, does it wrap?"
    );
    assert_eq!(n.options.len(), 3);
    assert_eq!(n.options[1].label, "No thanks");
}

#[test]
fn parses_multi_and_tabs() {
    let p = picker(MULTI_OPEN);
    assert!(p.multi);
    assert_eq!(p.cursor, Cursor::Option(0));
    assert_eq!(
        p.tabs,
        vec![
            TabInfo {
                label: "Sources".into(),
                done: false
            },
            TabInfo {
                label: "Order".into(),
                done: false
            }
        ]
    );
    assert_eq!(p.header, "");
    assert_eq!(p.question, "Which sources should be migrated?");
    assert_eq!(p.options.len(), 4);
    assert!(p.options.iter().all(|o| o.checked == Some(false)));
    assert_eq!(p.options[3].label, "Type something");
    assert!(p.options[3].is_text_row);

    let c = picker(MULTI_CHECKED);
    assert_eq!(c.options[0].checked, Some(true));
    assert_eq!(c.options[0].label, "Jira");
    assert!(c.tabs[0].done);

    let n = picker(MULTI_NEXT);
    assert_eq!(n.cursor, Cursor::Next);
    assert_eq!(n.options[2].checked, Some(true));
}

#[test]
fn parses_second_tab_single_select() {
    let p = picker(MULTI_Q2);
    assert!(!p.multi);
    assert_eq!(p.question, "Which goes first?");
    assert_eq!(p.tabs.len(), 2);
    assert_eq!(p.options.len(), 3);
}

#[test]
fn parses_review() {
    let r = parse_review(&rows(MULTI_REVIEW)).expect("review");
    assert!(r.cursor_on_submit);
    assert_eq!(r.items.len(), 2);
    assert_eq!(r.items[0].question, "Which sources should be migrated?");
    assert_eq!(r.items[0].answer, "Jira, Confluence");
    assert_eq!(r.items[1].answer, "GitLab");
    assert!(parse_picker(&rows(MULTI_REVIEW)).is_none());
    assert!(matches!(
        parse_screen(&rows(MULTI_REVIEW)),
        Some(Screen::Review(_))
    ));
}

#[test]
fn review_cursor_on_cancel_is_not_submit() {
    let s = MULTI_REVIEW
        .replace("❯ 1. Submit answers", "  1. Submit answers")
        .replace("  2. Cancel", "❯ 2. Cancel");
    assert!(!parse_review(&rows(&s)).unwrap().cursor_on_submit);
}

#[test]
fn unknown_screens_are_none() {
    assert!(parse_picker(&[]).is_none());
    assert!(parse_picker(&rows("hello\nworld\n")).is_none());
    assert!(parse_screen(&rows("❯ 1. Yes\n  2. No\nEsc to cancel\n")).is_none());
    let no_footer = SINGLE.replace("Enter to select", "Something else");
    assert!(parse_picker(&rows(&no_footer)).is_none());
    let broken = SINGLE.replace("  3. Only Jira+GitLab", "  7. Only Jira+GitLab");
    assert!(parse_picker(&rows(&broken)).is_none());
    let two_cursors = SINGLE.replace("  2. All", "❯ 2. All");
    assert!(parse_picker(&rows(&two_cursors)).is_none());
    let no_cursor = SINGLE.replace("❯ 1.", "  1.");
    assert!(parse_picker(&rows(&no_cursor)).is_none());
}

#[test]
fn matches_the_card_identity() {
    assert!(matches_card(&picker(SINGLE), &scope_q()));
    assert!(matches_card(&picker(DOWN1), &scope_q()));
    assert!(matches_card(&picker(MULTI_OPEN), &sources_q()));
    assert!(matches_card(&picker(MULTI_Q2), &order_q()));
    assert!(matches_card(&picker(NARROW), &narrow_q()));
}

#[test]
fn rejects_a_different_card() {
    let p = picker(SINGLE);
    let mut q = scope_q();
    q.question.push('!');
    assert!(!matches_card(&p, &q));
    let mut q = scope_q();
    q.options[1].label = "Other".into();
    assert!(!matches_card(&p, &q));
    let mut q = scope_q();
    q.header = "Else".into();
    assert!(!matches_card(&p, &q));
    let mut q = scope_q();
    q.multi_select = true;
    assert!(!matches_card(&p, &q));
    let mut q = scope_q();
    q.options.pop();
    assert!(!matches_card(&p, &q));
    assert!(!matches_card(&p, &sources_q()));
}

#[test]
fn matching_tolerates_whitespace_and_clipped_card_text() {
    let p = picker(SINGLE);
    let mut q = scope_q();
    q.question = q.question.replace(' ', "  ");
    assert!(matches_card(&p, &q));
    let mut long = scope_q();
    long.question = "x".repeat(499) + " tail";
    let mut shown = picker(SINGLE);
    shown.question = format!("{} more words", "x".repeat(499) + " tail");
    assert!(!matches_card(&shown, &long) || long.question.chars().count() >= QUESTION_MAX);
    let mut clipped = scope_q();
    clipped.question = "y".repeat(QUESTION_MAX);
    shown.question = "y".repeat(QUESTION_MAX) + " and more";
    assert!(matches_card(&shown, &clipped));
}

#[test]
fn sanitizes_other_text() {
    assert_eq!(sanitize_other("a\x1b[Bb\r\nc\x03\x15d"), "a[Bbcd");
    assert_eq!(sanitize_other("  hi  "), "hi");
    assert_eq!(sanitize_other(&"é".repeat(800)).len(), 1000);
}

fn run(sim: &mut Sim, picks: &[PollPick]) -> Result<usize, String> {
    let qs = sim.qs.clone();
    for n in 0..200 {
        let screen = parse_screen(&sim.render());
        match advance(&qs, picks, screen.as_ref()) {
            Next::Done => return Ok(n),
            Next::Refuse(why) => return Err(why.to_string()),
            Next::Step(s) => {
                sim.press(&s.keys);
                let after = parse_screen(&sim.render());
                if !s.expect.holds(&qs, after.as_ref()) {
                    return Err(format!("expectation {:?} not met", s.expect));
                }
            }
        }
    }
    Err("no end".into())
}

#[test]
fn single_pick_option_two() {
    let mut sim = Sim::new(vec![scope_q()], 100);
    run(&mut sim, &[pick(0, &[1], None)]).unwrap();
    assert_eq!(sim.answers, ["All sources at once"]);
    assert_eq!(sim.keys, [b"\x1b[B".to_vec(), b"\r".to_vec()]);
}

#[test]
fn single_pick_first_needs_only_enter() {
    let mut sim = Sim::new(vec![scope_q()], 100);
    run(&mut sim, &[pick(0, &[0], None)]).unwrap();
    assert_eq!(sim.keys, [b"\r".to_vec()]);
    assert_eq!(sim.answers, ["Jira+GitLab+Confluence (Recommended)"]);
}

#[test]
fn single_pick_last_real_option() {
    let mut sim = Sim::new(vec![scope_q()], 100);
    run(&mut sim, &[pick(0, &[2], None)]).unwrap();
    assert_eq!(sim.answers, ["Only Jira+GitLab"]);
}

#[test]
fn single_other_text() {
    let mut sim = Sim::new(vec![scope_q()], 100);
    run(&mut sim, &[pick(0, &[], Some("my own\x1b plan"))]).unwrap();
    assert_eq!(sim.answers, ["my own plan"]);
    assert_eq!(sim.keys.len(), 5);
    assert_eq!(sim.keys[3], b"my own plan".to_vec());
}

#[test]
fn multi_toggles_then_next() {
    let mut sim = Sim::new(vec![sources_q(), order_q()], 100);
    let picks = [pick(0, &[0, 2], None), pick(1, &[1], None)];
    run(&mut sim, &picks).unwrap();
    assert_eq!(sim.answers, ["Jira, Confluence", "GitLab"]);
    assert!(sim.submitted);
}

#[test]
fn multi_alone_ends_after_next() {
    let mut sim = Sim::new(vec![sources_q()], 100);
    run(&mut sim, &[pick(0, &[0, 2], None)]).unwrap();
    assert_eq!(sim.answers, ["Jira, Confluence"]);
    assert!(!sim.submitted);
}

#[test]
fn single_question_review_is_submitted() {
    let mut sim = Sim::new(vec![scope_q()], 100);
    sim.single_goes_to_review = true;
    run(&mut sim, &[pick(0, &[0], None)]).unwrap();
    assert!(sim.submitted);
}

#[test]
fn narrow_screen() {
    let mut sim = Sim::new(vec![narrow_q()], 80);
    run(&mut sim, &[pick(0, &[1], None)]).unwrap();
    assert_eq!(sim.answers, ["No thanks"]);
}

#[test]
fn never_sends_forbidden_keys() {
    let mut sim = Sim::new(vec![sources_q(), scope_q()], 100);
    let picks = [pick(0, &[1, 2], None), pick(1, &[], Some("free text"))];
    run(&mut sim, &picks).unwrap();
    for k in &sim.keys {
        assert!(!k.contains(&0x15) && k.as_slice() != b"\x1b" && k.as_slice() != b"\x1b[A");
    }
    assert!(sim.submitted);
}

#[test]
fn refuses_unknown_screen() {
    let qs = [scope_q()];
    assert_eq!(
        advance(
            &qs,
            &[pick(0, &[0], None)],
            Some(&Screen::Review(Review {
                cursor_on_submit: true,
                items: Vec::new()
            }))
        ),
        Next::Refuse("review does not match")
    );
    assert_eq!(advance(&qs, &[pick(0, &[0], None)], None), Next::Done);
}

#[test]
fn refuses_mismatched_question() {
    let sim = Sim::new(vec![scope_q()], 100);
    let other = [sources_q()];
    let picks = [pick(0, &[0], None)];
    let screen = parse_screen(&sim.render());
    assert_eq!(
        advance(&other, &picks, screen.as_ref()),
        Next::Refuse("question does not match")
    );
    assert!(sim.keys.is_empty());
    let mut changed = scope_q();
    changed.options[0].label = "Different".into();
    assert_eq!(
        advance(&[changed], &picks, screen.as_ref()),
        Next::Refuse("question does not match")
    );
}

#[test]
fn refuses_other_for_multi_select() {
    let mut sim = Sim::new(vec![sources_q(), order_q()], 100);
    let r = run(&mut sim, &[pick(0, &[], Some("x")), pick(1, &[0], None)]);
    assert_eq!(r, Err("other is not supported for multi select".into()));
    assert!(sim.keys.is_empty());
}

#[test]
fn refuses_invalid_answers() {
    let mut sim = Sim::new(vec![scope_q()], 100);
    assert_eq!(
        run(&mut sim, &[pick(0, &[9], None)]),
        Err("invalid answer".into())
    );
    assert_eq!(
        run(&mut sim, &[pick(0, &[0, 1], None)]),
        Err("invalid answer".into())
    );
    assert_eq!(
        run(&mut sim, &[pick(0, &[0], Some("t"))]),
        Err("invalid answer".into())
    );
    assert_eq!(
        run(&mut sim, &[pick(0, &[], Some("\x1b\x03"))]),
        Err("empty text".into())
    );
    assert_eq!(
        run(&mut sim, &[pick(3, &[0], None)]),
        Err("invalid answer".into())
    );
    assert_eq!(run(&mut sim, &[]), Err("no answer for question".into()));
    assert!(sim.keys.is_empty());
}

#[test]
fn never_moves_up_when_cursor_is_past_target() {
    let qs = [scope_q()];
    let screen = Screen::Picker(picker(DOWN1));
    assert_eq!(
        advance(&qs, &[pick(0, &[0], None)], Some(&screen)),
        Next::Refuse("cursor past target")
    );
    let chat = Screen::Picker(picker(CHAT_ROW));
    assert_eq!(
        advance(&qs, &[pick(0, &[0], None)], Some(&chat)),
        Next::Refuse("cursor past target")
    );
}

#[test]
fn text_row_with_foreign_text_is_refused() {
    let qs = [scope_q()];
    let typed = Screen::Picker(picker(OTHER_TYPED));
    assert_eq!(
        advance(&qs, &[pick(0, &[], Some("bye"))], Some(&typed)),
        Next::Refuse("text row not empty")
    );
    match advance(&qs, &[pick(0, &[], Some("hello"))], Some(&typed)) {
        Next::Step(s) => assert_eq!(s.keys, b"\r"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn review_must_match_the_answer() {
    let qs = [sources_q(), order_q()];
    let screen = parse_screen(&rows(MULTI_REVIEW));
    let good = [pick(0, &[0, 2], None), pick(1, &[1], None)];
    match advance(&qs, &good, screen.as_ref()) {
        Next::Step(s) => {
            assert_eq!(s.keys, b"\r");
            assert_eq!(s.expect, Expect::Gone);
        }
        other => panic!("{other:?}"),
    }
    let bad = [pick(0, &[0], None), pick(1, &[1], None)];
    assert_eq!(
        advance(&qs, &bad, screen.as_ref()),
        Next::Refuse("review does not match")
    );
}

#[test]
fn fixture_steps() {
    let qs = [scope_q()];
    let p = [pick(0, &[2], None)];
    let step = |s: &str| advance(&qs, &p, Some(&Screen::Picker(picker(s))));
    assert_eq!(
        step(SINGLE),
        Next::Step(Step {
            keys: KEY_DOWN.to_vec(),
            expect: Expect::Cursor(1)
        })
    );
    assert_eq!(
        step(DOWN1),
        Next::Step(Step {
            keys: KEY_DOWN.to_vec(),
            expect: Expect::Cursor(2)
        })
    );
    let q = [sources_q()];
    let pm = [pick(0, &[0, 2], None)];
    let m = |s: &str| advance(&q, &pm, Some(&Screen::Picker(picker(s))));
    assert_eq!(
        m(MULTI_OPEN),
        Next::Step(Step {
            keys: KEY_SPACE.to_vec(),
            expect: Expect::Checked(0, true)
        })
    );
    assert_eq!(
        m(MULTI_CHECKED),
        Next::Step(Step {
            keys: KEY_DOWN.to_vec(),
            expect: Expect::Cursor(1)
        })
    );
    assert_eq!(
        m(MULTI_NEXT),
        Next::Step(Step {
            keys: KEY_ENTER.to_vec(),
            expect: Expect::Left { question: 0 }
        })
    );
}
