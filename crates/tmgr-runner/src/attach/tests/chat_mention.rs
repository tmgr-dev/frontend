use super::*;
use tmgr_ipc::chat::{
    Author, AuthorKind, ChatMessage, MentionReq, MentionState, MentionTarget, MessageKind,
    RoomInfo, RoomKind, RoomRef,
};

const MACHINE: &str = "mx_0123456789abcdef";
const A: &str = "s_aaaaaaaaaa";
const B: &str = "s_bbbbbbbbbb";
const C: &str = "s_cccccccccc";
const R: &str = "s_rrrrrrrrrr";

fn room(id: &str) -> RoomRef {
    RoomRef {
        kind: RoomKind::Session,
        machine: MACHINE.into(),
        session: Some(id.into()),
    }
}

fn manager() -> RoomRef {
    RoomRef {
        kind: RoomKind::Manager,
        machine: MACHINE.into(),
        session: None,
    }
}

fn verdict(ok: bool, risky: bool) -> Option<MentionTarget> {
    Some(MentionTarget {
        ok,
        reason: (!ok).then(|| "Exited".to_string()),
        risky,
    })
}

fn info(r: RoomRef, title: &str, agent: &str, m: Option<MentionTarget>) -> RoomInfo {
    RoomInfo {
        room: r,
        title: title.into(),
        agent: Some(agent.into()),
        state: None,
        context_pct: None,
        context_tokens: None,
        model: None,
        last: None,
        unrestricted: false,
        mention: m,
    }
}

fn rooms() -> Vec<RoomInfo> {
    vec![
        info(manager(), "Manager", "manager", verdict(true, false)),
        info(room(A), "api-fix", "claude-code", verdict(true, false)),
        info(room(B), "Build Fix", "codex", verdict(true, false)),
        info(room(C), "build fix", "claude-code", verdict(true, false)),
        info(room(R), "danger", "claude-code", verdict(true, true)),
        info(room("s_dddddddddd"), "gone", "codex", verdict(false, false)),
        info(room("s_eeeeeeeeee"), "docs", "codex", verdict(true, false)),
    ]
}

fn ready(cap: bool, agent: bool) -> ChatState {
    let mut c = ChatState::new()
        .with_offset(0)
        .with_mention_cap(cap)
        .with_agent_session(agent);
    c.set_rooms(rooms());
    c.sel = c.rooms.iter().position(|r| r.info.room == room(A)).unwrap();
    c
}

fn view(c: ChatState) -> ViewState {
    let mut v = two_pane(140);
    v.open_chat(c);
    v
}

fn type_and_enter(v: &mut ViewState, text: &str) -> Vec<ChatAction> {
    for ch in text.chars() {
        v.edit_chat(ch.to_string().as_bytes());
    }
    v.edit_chat(b"\r")
}

fn status(v: &ViewState) -> String {
    v.chat.as_ref().unwrap().status().to_string()
}

fn msg(id: &str, r: RoomRef, body: &str, mention: Option<(&str, MentionState)>) -> ChatMessage {
    ChatMessage {
        v: 1,
        id: id.into(),
        room: r,
        author: Author {
            kind: AuthorKind::Owner,
            id: "o".into(),
            machine: None,
            device: None,
            agent: None,
        },
        ts_ms: 1_700_000_000_000,
        kind: MessageKind::Text,
        body_md: body.into(),
        attachments: Vec::new(),
        reply_to: None,
        decision: None,
        session_meta: None,
        user_turn: false,
        mention: mention.map(|(mid, state)| tmgr_ipc::chat::MentionRef {
            id: mid.into(),
            target: room(B),
            target_title: "docs".into(),
            state,
            hop: 0,
        }),
    }
}

fn mention_send(target: RoomRef, risky: bool) -> Option<MentionReq> {
    Some(MentionReq {
        target,
        confirmed_risky: risky,
    })
}

fn row_text(c: &ChatState, msgs: &[ChatMessage]) -> Vec<String> {
    crate::attach::chat::message_rows(c, msgs, 60)
        .iter()
        .map(|r| r.iter().map(|(t, _)| t.as_str()).collect::<String>())
        .collect()
}

#[test]
fn a_mention_resolves_case_insensitively_by_session_id_and_drops_the_name() {
    let mut v = view(ready(true, false));
    let actions = type_and_enter(&mut v, "@API-FIX  why is it red?");
    assert_eq!(
        actions,
        vec![ChatAction::Send {
            room: room(A),
            body: "why is it red?".into(),
            mention: mention_send(room(A), false),
        }]
    );
    let mut v = view(ready(true, false));
    let actions = type_and_enter(&mut v, &format!("@{B} hello"));
    assert_eq!(
        actions,
        vec![ChatAction::Send {
            room: room(A),
            body: "hello".into(),
            mention: mention_send(room(B), false),
        }]
    );
}

#[test]
fn titles_equal_ignoring_case_are_ambiguous_until_a_session_id_is_used() {
    let mut v = view(ready(true, false));
    let actions = type_and_enter(&mut v, "@build fix go");
    assert!(actions.is_empty());
    let s = status(&v);
    assert!(s.contains(B) && s.contains(C) && s.contains("codex"), "{s}");
    assert_eq!(v.chat.as_ref().unwrap().input_text(), "@build fix go");
    v.edit_chat(b"\x15");
    let actions = type_and_enter(&mut v, &format!("@{C} go"));
    assert_eq!(
        actions,
        vec![ChatAction::Send {
            room: room(A),
            body: "go".into(),
            mention: mention_send(room(C), false),
        }]
    );
}

#[test]
fn unknown_unavailable_and_empty_mentions_send_nothing() {
    let mut v = view(ready(true, false));
    assert!(type_and_enter(&mut v, "@nobody hi").is_empty());
    assert_eq!(status(&v), "No session named @nobody");
    v.edit_chat(b"\x15");
    assert!(type_and_enter(&mut v, "@gone hi").is_empty());
    assert_eq!(status(&v), "Cannot ask @gone: Exited");
    v.edit_chat(b"\x15");
    v.edit_chat(b"@api-fix");
    v.edit_chat(b"\r");
    assert!(v.edit_chat(b"\r").is_empty());
    assert_eq!(status(&v), "Write a question after the name");
}

#[test]
fn a_runner_without_the_capability_gets_no_mention() {
    let mut v = view(ready(false, false));
    assert!(type_and_enter(&mut v, "@api-fix hi").is_empty());
    assert_eq!(status(&v), "Runner too old for mentions");
    v.edit_chat(b"\x15");
    assert!(type_and_enter(&mut v, ":add").is_empty());
    assert_eq!(status(&v), "Runner too old for mentions");
}

#[test]
fn a_plain_message_is_not_a_mention() {
    let mut v = view(ready(true, false));
    let actions = type_and_enter(&mut v, "hello @api-fix");
    assert_eq!(
        actions,
        vec![ChatAction::Send {
            room: room(A),
            body: "hello @api-fix".into(),
            mention: None,
        }]
    );
}

#[test]
fn a_risky_target_needs_y_inside_the_window() {
    let mut c = ready(true, false);
    c.poll(1_000);
    let mut v = view(c);
    assert!(type_and_enter(&mut v, "@danger run it").is_empty());
    assert!(v.chat.as_ref().unwrap().awaiting_confirm());
    let actions = v.edit_chat(b"y");
    assert_eq!(
        actions,
        vec![ChatAction::Send {
            room: room(A),
            body: "run it".into(),
            mention: mention_send(room(R), true),
        }]
    );
}

#[test]
fn a_risky_confirmation_is_cancelled_by_another_key_and_expires_after_30_seconds() {
    let mut c = ready(true, false);
    c.poll(1_000);
    let mut v = view(c);
    type_and_enter(&mut v, "@danger run it");
    assert!(v.edit_chat(b"n").is_empty());
    assert_eq!(status(&v), "Cancelled");
    assert_eq!(v.chat.as_ref().unwrap().input_text(), "@danger run it");

    v.edit_chat(b"\r");
    assert!(v.chat_poll(1_000 + 30_001).is_empty());
    assert!(!v.chat.as_ref().unwrap().awaiting_confirm());
    assert_eq!(status(&v), "Confirmation expired");

    v.chat_poll(40_000);
    v.edit_chat(b"\r");
    assert!(v.chat_poll(40_000 + 29_000).is_empty());
    assert_eq!(v.edit_chat(b"y").len(), 1);
}

#[test]
fn inside_an_agent_session_risky_mentions_and_add_are_refused_but_plain_ones_pass() {
    let mut v = view(ready(true, true));
    assert!(type_and_enter(&mut v, "@danger run it").is_empty());
    assert!(!v.chat.as_ref().unwrap().awaiting_confirm());
    assert!(status(&v).contains("TMGR_SESSION_ID"));
    v.edit_chat(b"\x15");
    assert_eq!(type_and_enter(&mut v, "@docs hi").len(), 1);

    let mut c = ready(true, true);
    c.history(
        &room(A),
        None,
        Ok(vec![msg(
            "a1",
            room(A),
            "r",
            Some(("q1", MentionState::Answered)),
        )]),
    );
    let mut v = view(c);
    assert!(type_and_enter(&mut v, ":add").is_empty());
    assert!(status(&v).contains("TMGR_SESSION_ID"));
}

#[test]
fn the_latest_state_per_mention_id_is_shown_next_to_the_question() {
    let c = ready(true, false);
    let msgs = vec![
        msg("q1", room(A), "what?", Some(("q1", MentionState::Queued))),
        msg(
            "s1",
            room(A),
            "Sent to @docs",
            Some(("q1", MentionState::Sent)),
        ),
        msg(
            "s2",
            room(A),
            "@docs is answering",
            Some(("q1", MentionState::Answering)),
        ),
        msg("q2", room(A), "again?", Some(("q2", MentionState::Queued))),
        msg(
            "s3",
            room(A),
            "@docs is busy",
            Some(("q2", MentionState::Busy)),
        ),
    ];
    let text = row_text(&c, &msgs);
    assert!(
        text.contains(&"\u{21b3} @docs: answering".to_string()),
        "{text:?}"
    );
    assert!(
        text.contains(&"\u{21b3} @docs: busy".to_string()),
        "{text:?}"
    );
    assert!(!text.iter().any(|l| l.contains("Sent to")), "{text:?}");
    assert!(!text.iter().any(|l| l.contains("is busy")), "{text:?}");
}

#[test]
fn reply_rows_render_and_the_state_follows_the_answer() {
    let c = ready(true, false);
    let msgs = vec![
        msg("q1", room(A), "what?", Some(("q1", MentionState::Queued))),
        msg(
            "r1",
            room(A),
            "\u{21a9} @docs: because",
            Some(("q1", MentionState::Answered)),
        ),
    ];
    let text = row_text(&c, &msgs);
    assert!(
        text.contains(&"\u{21b3} @docs: answered".to_string()),
        "{text:?}"
    );
    assert!(
        text.contains(&"\u{21a9} @docs: because".to_string()),
        "{text:?}"
    );
}

fn with_answers(origin: RoomRef, ids: &[&str]) -> ChatState {
    let mut c = ready(true, false);
    let list = ids
        .iter()
        .enumerate()
        .flat_map(|(i, id)| {
            [
                msg(
                    &format!("q{i}"),
                    origin.clone(),
                    "?",
                    Some((id, MentionState::Queued)),
                ),
                msg(
                    &format!("r{i}"),
                    origin.clone(),
                    "ans",
                    Some((id, MentionState::Answered)),
                ),
            ]
        })
        .collect();
    c.history(&origin, None, Ok(list));
    c
}

#[test]
fn add_injects_the_latest_answered_mention_and_n_picks_an_older_one() {
    let mut v = view(with_answers(room(A), &["m1", "m2"]));
    assert_eq!(
        type_and_enter(&mut v, ":add"),
        vec![ChatAction::Inject {
            room: room(A),
            mention_id: "m2".into(),
            confirmed_risky: false,
        }]
    );
    assert_eq!(
        type_and_enter(&mut v, ":add 2"),
        vec![ChatAction::Inject {
            room: room(A),
            mention_id: "m1".into(),
            confirmed_risky: false,
        }]
    );
    assert!(type_and_enter(&mut v, ":add 3").is_empty());
    assert_eq!(status(&v), "Only 2 answered mention(s) in this room");
    v.edit_chat(b"\x15");
    assert!(type_and_enter(&mut v, ":add x").is_empty());
    assert_eq!(status(&v), "Usage: :add or :add <n>");
}

#[test]
fn add_without_an_answered_mention_says_so() {
    let mut v = view(ready(true, false));
    assert!(type_and_enter(&mut v, ":add").is_empty());
    assert_eq!(status(&v), "No answered mention in this room");
}

#[test]
fn add_into_a_manager_origin_asks_first() {
    let mut c = with_answers(manager(), &["m1"]);
    c.sel = c
        .rooms
        .iter()
        .position(|r| r.info.room == manager())
        .unwrap();
    c.poll(5_000);
    let mut v = view(c);
    assert!(type_and_enter(&mut v, ":add").is_empty());
    assert!(v.chat.as_ref().unwrap().awaiting_confirm());
    assert_eq!(
        v.edit_chat(b"y"),
        vec![ChatAction::Inject {
            room: manager(),
            mention_id: "m1".into(),
            confirmed_risky: true,
        }]
    );
}

#[test]
fn a_runner_demand_for_confirmation_turns_into_the_prompt() {
    let mut c = ready(true, false);
    c.injected(
        &room(A),
        "m1",
        Err("forbidden: this session needs confirmation before it takes the reply".into()),
    );
    assert!(c.awaiting_confirm());
    let mut c = ready(true, false);
    c.injected(&room(A), "m1", Err("forbidden: nope".into()));
    assert!(!c.awaiting_confirm());
}

#[test]
fn typing_at_offers_only_mentionable_local_rooms_and_tab_completes() {
    let mut v = view(ready(true, false));
    v.edit_chat(b"@dan");
    v.edit_chat(b"\t");
    assert_eq!(v.chat.as_ref().unwrap().input_text(), "@danger ");

    let mut v = view(ready(true, false));
    v.edit_chat(b"@bu");
    v.edit_chat(b"\t");
    assert_eq!(v.chat.as_ref().unwrap().input_text(), format!("@{B} "));

    let mut v = view(ready(true, false));
    v.edit_chat(b"@gon");
    v.edit_chat(b"\t");
    assert_eq!(v.chat.as_ref().unwrap().input_text(), "@gon ");
}

#[test]
fn the_menu_stays_closed_without_the_capability() {
    let mut v = view(ready(false, false));
    v.edit_chat(b"@d");
    v.edit_chat(b"\t");
    assert_eq!(v.chat.as_ref().unwrap().input_text(), "@d ");
}
