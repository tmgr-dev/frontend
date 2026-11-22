use super::*;
use tmgr_ipc::chat::{Author, AuthorKind, ChatMessage, MessageKind, RoomInfo, RoomKind, RoomRef};

const MACHINE: &str = "mx_0123456789abcdef";
const T0: u64 = 1_700_000_000_000;

fn manager_room() -> RoomRef {
    RoomRef {
        kind: RoomKind::Manager,
        machine: MACHINE.into(),
        session: None,
    }
}

fn session_room(id: &str) -> RoomRef {
    RoomRef {
        kind: RoomKind::Session,
        machine: MACHINE.into(),
        session: Some(id.into()),
    }
}

fn info(room: RoomRef, title: &str, agent: Option<&str>, pct: Option<u8>) -> RoomInfo {
    RoomInfo {
        room,
        title: title.into(),
        agent: agent.map(str::to_string),
        state: Some("working".into()),
        context_pct: pct,
        context_tokens: None,
        model: None,
        last: None,
        unrestricted: false,
    }
}

fn rooms() -> Vec<RoomInfo> {
    vec![
        info(
            session_room("s_aaaaaaaaaa"),
            "api-fix",
            Some("claude-code"),
            Some(42),
        ),
        info(session_room("s_bbbbbbbbbb"), "docs", Some("codex"), None),
        info(manager_room(), "Manager", None, None),
    ]
}

fn msg(n: u64, room: RoomRef, author: AuthorKind, kind: MessageKind, body: &str) -> ChatMessage {
    ChatMessage {
        v: 1,
        id: format!("m_{n:012x}{:020x}", 0),
        room,
        author: Author {
            kind: author,
            id: "x".into(),
            machine: None,
            device: None,
            agent: (author == AuthorKind::Session).then(|| "codex".to_string()),
        },
        ts_ms: T0 + n * 60_000,
        kind,
        body_md: body.into(),
        attachments: Vec::new(),
        reply_to: None,
        decision: None,
        session_meta: None,
        user_turn: false,
    }
}

fn ready() -> ChatState {
    let mut c = ChatState::new().with_offset(0);
    c.set_rooms(rooms());
    c
}

fn view_with(chat: ChatState) -> ViewState {
    let mut v = two_pane(140);
    v.open_chat(chat);
    v
}

fn type_keys(v: &mut ViewState, keys: &[&[u8]]) -> Vec<ChatAction> {
    keys.iter().flat_map(|k| v.edit_chat(k)).collect()
}

#[test]
fn the_chat_key_is_free_and_help_lists_it() {
    let keys: Vec<_> = ACTIONS.iter().filter(|a| a.keys.contains(&b'j')).collect();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].cmd, Cmd::Chat);
    assert!(keys[0].palette);
    assert!(super::render::HELP
        .iter()
        .any(|(k, _)| matches!(k, super::render::HelpKey::Cmds(c) if c.contains(&Cmd::Chat))));
}

#[test]
fn the_manager_room_is_pinned_first_and_loads_its_history() {
    let mut c = ChatState::new().with_offset(0);
    let action = c.set_rooms(rooms());
    assert_eq!(c.rooms[0].info.room, manager_room());
    assert_eq!(c.rooms[1].info.title, "api-fix");
    assert_eq!(c.selected(), Some(&manager_room()));
    assert_eq!(
        action,
        Some(ChatAction::Load {
            room: manager_room(),
            before: None
        })
    );
}

#[test]
fn snapshot_room_list_with_manager_first() {
    let mut c = ready();
    c.history(&manager_room(), None, Ok(Vec::new()));
    let v = view_with(c);
    assert_snapshot("chat_rooms", &draw(&v, &screens(), 100, 16));
}

#[test]
fn snapshot_messages_of_every_author_kind_and_wrap() {
    let mut c = ready();
    let room = manager_room();
    let long = "word ".repeat(30);
    c.history(
        &room,
        None,
        Ok(vec![
            msg(
                1,
                room.clone(),
                AuthorKind::Owner,
                MessageKind::Text,
                "start the api-fix run",
            ),
            msg(
                2,
                room.clone(),
                AuthorKind::Manager,
                MessageKind::Text,
                "On it.",
            ),
            msg(
                3,
                room.clone(),
                AuthorKind::Session,
                MessageKind::Text,
                &long,
            ),
            msg(
                4,
                room.clone(),
                AuthorKind::System,
                MessageKind::System,
                "session s_aaaaaaaaaa exited",
            ),
            msg(
                5,
                room.clone(),
                AuthorKind::Manager,
                MessageKind::Other,
                "future kind",
            ),
        ]),
    );
    let v = view_with(c);
    assert_snapshot("chat_messages", &draw(&v, &screens(), 100, 24));
}

#[test]
fn a_session_message_shows_the_agent_and_a_system_like_kind_shows_system() {
    let c = ready();
    let room = manager_room();
    let agent = msg(
        1,
        room.clone(),
        AuthorKind::Session,
        MessageKind::Text,
        "hi",
    );
    let other = msg(2, room, AuthorKind::Manager, MessageKind::Other, "x");
    let rows = crate::attach::chat::message_rows(&c, &[agent, other], 40);
    assert_eq!(rows[0][0].0, "codex");
    assert_eq!(rows[0][1].0, "  22:14");
    let sys = rows
        .iter()
        .find(|r| r.first().is_some_and(|s| s.0 == "System"));
    assert!(sys.is_some());
}

#[test]
fn typing_and_editing_build_the_send_request() {
    let mut v = view_with(ready());
    let actions = type_keys(
        &mut v,
        &[b"helo", b"\x1b[D", b"l", b"\x1b[F", b"!", b"\x7f", b"\x7f"],
    );
    assert!(actions.is_empty());
    assert_eq!(v.chat.as_ref().unwrap().input_text(), "hell");
    let actions = v.edit_chat(b"\r");
    assert_eq!(
        actions,
        vec![ChatAction::Send {
            room: manager_room(),
            body: "hell".into()
        }]
    );
    assert_eq!(v.chat.as_ref().unwrap().status(), "Sending...");
    assert!(v.edit_chat(b"\r").is_empty());
}

#[test]
fn an_empty_input_sends_nothing_and_paste_is_flattened() {
    let mut v = view_with(ready());
    assert!(v.edit_chat(b"\r").is_empty());
    v.insert_chat_text(b"\x1b[200~one\ntwo\x1b[201~");
    assert_eq!(v.chat.as_ref().unwrap().input_text(), "one two");
}

#[test]
fn a_successful_send_clears_the_input_and_shows_the_message_once() {
    let mut v = view_with(ready());
    type_keys(&mut v, &[b"ping"]);
    v.edit_chat(b"\r");
    let sent = msg(
        9,
        manager_room(),
        AuthorKind::Owner,
        MessageKind::Text,
        "ping",
    );
    let c = v.chat.as_mut().unwrap();
    c.sent(&manager_room(), Ok(sent.clone()));
    c.push(sent);
    assert_eq!(c.input_text(), "");
    assert_eq!(c.messages(&manager_room()).len(), 1);
}

#[test]
fn a_busy_error_keeps_the_input_and_shows_the_inline_status() {
    let mut v = view_with(ready());
    type_keys(&mut v, &[b"ping"]);
    v.edit_chat(b"\r");
    v.chat
        .as_mut()
        .unwrap()
        .sent(&manager_room(), Err((true, "busy: x".into())));
    let c = v.chat.as_ref().unwrap();
    assert_eq!(
        c.status(),
        "Session is busy, try again when it is at its prompt"
    );
    assert_eq!(c.input_text(), "ping");
    assert_snapshot("chat_busy", &draw(&v, &screens(), 100, 12));
    let c = v.chat.as_mut().unwrap();
    c.sent(&manager_room(), Err((false, "invalid params".into())));
    assert_eq!(c.status(), "invalid params");
}

#[test]
fn a_push_for_another_room_marks_it_unread_and_selecting_clears_it() {
    let mut v = view_with(ready());
    let other = session_room("s_aaaaaaaaaa");
    let c = v.chat.as_mut().unwrap();
    c.push(msg(
        3,
        other.clone(),
        AuthorKind::Session,
        MessageKind::Text,
        "done",
    ));
    assert!(c.is_unread(&other));
    assert!(!c.is_unread(&manager_room()));
    c.push(msg(
        4,
        manager_room(),
        AuthorKind::Manager,
        MessageKind::Text,
        "hello",
    ));
    assert!(!c.is_unread(&manager_room()));
    assert_eq!(c.messages(&manager_room()).len(), 1);
    assert_snapshot("chat_unread", &draw(&v, &screens(), 100, 14));
    let actions = v.edit_chat(b"\x1b[B");
    assert_eq!(
        actions,
        vec![ChatAction::Load {
            room: other.clone(),
            before: None
        }]
    );
    assert!(!v.chat.as_ref().unwrap().is_unread(&other));
}

#[test]
fn a_push_for_an_unknown_session_adds_an_unread_room() {
    let mut v = view_with(ready());
    let fresh = session_room("s_cccccccccc");
    let c = v.chat.as_mut().unwrap();
    c.push(msg(
        5,
        fresh.clone(),
        AuthorKind::Session,
        MessageKind::Text,
        "hi",
    ));
    assert_eq!(c.rooms.len(), 4);
    assert!(c.is_unread(&fresh));
}

#[test]
fn paging_up_loads_older_messages_before_the_oldest_id() {
    let mut v = view_with(ready());
    let room = manager_room();
    let page: Vec<ChatMessage> = (1..=200)
        .map(|n| msg(n, room.clone(), AuthorKind::Manager, MessageKind::Text, "m"))
        .collect();
    let oldest = page[0].id.clone();
    v.chat.as_mut().unwrap().history(&room, None, Ok(page));
    draw(&v, &screens(), 100, 16);
    let mut guard = 0;
    let action = loop {
        let out = v.edit_chat(b"\x1b[5~");
        if let Some(a) = out.into_iter().next() {
            break a;
        }
        guard += 1;
        assert!(guard < 200);
        draw(&v, &screens(), 100, 16);
    };
    assert_eq!(
        action,
        ChatAction::Load {
            room: room.clone(),
            before: Some(oldest)
        }
    );
    assert!(v.edit_chat(b"\x1b[5~").is_empty());
    let older: Vec<ChatMessage> = (300..303)
        .map(|n| {
            msg(
                n,
                room.clone(),
                AuthorKind::Manager,
                MessageKind::Text,
                "old",
            )
        })
        .collect();
    v.chat
        .as_mut()
        .unwrap()
        .history(&room, Some("x"), Ok(older.clone()));
    let c = v.chat.as_ref().unwrap();
    assert_eq!(c.messages(&room).len(), 203);
    assert_eq!(c.messages(&room)[0].id, older[0].id);
}

#[test]
fn escape_closes_the_view_and_reports_it() {
    let mut v = view_with(ready());
    assert_eq!(v.edit_chat(b"\x1b"), vec![ChatAction::Close]);
    assert!(v.chat.is_none());
}

#[test]
fn snapshot_chat_needs_a_newer_runner_without_the_cap() {
    let v = view_with(ChatState::unsupported());
    assert_snapshot("chat_no_cap", &draw(&v, &screens(), 100, 10));
    let mut v = v;
    assert!(v.edit_chat(b"hello").is_empty());
    assert_eq!(v.chat.as_ref().unwrap().input_text(), "");
    assert_eq!(v.edit_chat(b"\x1b"), vec![ChatAction::Close]);
}

#[test]
fn chat_renders_at_every_small_size() {
    let mut c = ready();
    let room = manager_room();
    c.history(
        &room,
        None,
        Ok(vec![msg(
            1,
            room.clone(),
            AuthorKind::Owner,
            MessageKind::Text,
            "hello there",
        )]),
    );
    let v = view_with(c);
    for w in [1u16, 10, 20, 40, 56, 80] {
        for h in [1u16, 4, 8, 12] {
            draw(&v, &screens(), w, h);
        }
    }
}

#[test]
fn long_lines_wrap_at_words_and_break_unbroken_tokens() {
    use crate::attach::chat::wrap_words;
    assert_eq!(
        wrap_words("alpha beta gamma delta", 11),
        vec!["alpha beta", "gamma delta"]
    );
    assert_eq!(wrap_words("abcdefghij", 4), vec!["abcd", "efgh", "ij"]);
    assert_eq!(wrap_words("a\n\nb", 10), vec!["a", "", "b"]);
}

const PEER_A: &str = "mx_aaaaaaaaaaaaaaaa";
const PEER_B: &str = "mx_bbbbbbbbbbbbbbbb";

fn remote_manager(machine: &str) -> RoomRef {
    RoomRef {
        kind: RoomKind::Manager,
        machine: machine.into(),
        session: None,
    }
}

fn grouped_rooms() -> Vec<RoomInfo> {
    vec![
        info(manager_room(), "Manager", None, None),
        info(
            session_room("s_aaaaaaaaaa"),
            "api-fix",
            Some("claude-code"),
            Some(42),
        ),
        info(remote_manager(PEER_A), "laptop", None, None),
        info(remote_manager(PEER_B), "build box", None, None),
    ]
}

fn grouped() -> ChatState {
    let mut c = ChatState::new().with_offset(0);
    c.set_rooms(grouped_rooms());
    c
}

fn on_peer_a() -> ChatState {
    let mut v = view_with(grouped());
    type_keys(&mut v, &[b"\x1b[B", b"\x1b[B"]);
    v.chat.take().unwrap()
}

#[test]
fn snapshot_room_list_grouped_by_machine() {
    let mut c = grouped();
    c.history(&manager_room(), None, Ok(Vec::new()));
    let v = view_with(c);
    assert_snapshot("chat_rooms_grouped", &draw(&v, &screens(), 100, 16));
}

#[test]
fn headers_are_skipped_by_the_arrow_keys() {
    let mut v = view_with(grouped());
    let mut seen = vec![v.chat.as_ref().unwrap().selected().cloned().unwrap()];
    for _ in 0..4 {
        v.edit_chat(b"\x1b[B");
        seen.push(v.chat.as_ref().unwrap().selected().cloned().unwrap());
    }
    assert_eq!(
        seen,
        vec![
            manager_room(),
            session_room("s_aaaaaaaaaa"),
            remote_manager(PEER_A),
            remote_manager(PEER_B),
            remote_manager(PEER_B),
        ]
    );
    for _ in 0..4 {
        v.edit_chat(b"\x1b[A");
    }
    assert_eq!(v.chat.as_ref().unwrap().selected(), Some(&manager_room()));
}

#[test]
fn without_remote_rooms_there_are_no_headers() {
    let mut c = ready();
    c.history(&manager_room(), None, Ok(Vec::new()));
    let text = draw(&view_with(c), &screens(), 100, 16);
    assert!(!text.contains("This machine"));
}

#[test]
fn a_selected_remote_room_is_polled_every_15_seconds() {
    let mut c = on_peer_a();
    c.history(&remote_manager(PEER_A), None, Ok(Vec::new()));
    assert_eq!(c.poll(100_000), None);
    assert_eq!(c.poll(114_999), None);
    assert_eq!(
        c.poll(115_000),
        Some(ChatAction::Load {
            room: remote_manager(PEER_A),
            before: None
        })
    );
    assert_eq!(c.poll(131_000), None);
    c.history(&remote_manager(PEER_A), None, Ok(Vec::new()));
    assert_eq!(c.poll(129_999), None);
    assert!(c.poll(130_000).is_some());
}

#[test]
fn the_next_poll_counts_from_the_completion_of_a_slow_load() {
    let room = remote_manager(PEER_A);
    let mut c = on_peer_a();
    c.history(&room, None, Ok(Vec::new()));
    assert_eq!(c.poll(100_000), None);
    assert!(c.poll(115_000).is_some());
    c.history(&room, None, Ok(Vec::new()));
    c.history_done(&room, true, 140_000);
    assert_eq!(c.poll(154_999), None);
    assert!(c.poll(155_000).is_some());
}

#[test]
fn after_a_failed_load_the_next_poll_waits_60_seconds() {
    let room = remote_manager(PEER_A);
    let mut c = on_peer_a();
    c.history(&room, None, Ok(Vec::new()));
    assert_eq!(c.poll(100_000), None);
    assert!(c.poll(115_000).is_some());
    c.history(&room, None, Err("timed out".into()));
    c.history_done(&room, false, 140_000);
    assert_eq!(c.poll(199_999), None);
    assert!(c.poll(200_000).is_some());
}

#[test]
fn a_local_room_is_never_polled() {
    let mut c = grouped();
    c.history(&manager_room(), None, Ok(Vec::new()));
    assert_eq!(c.poll(0), None);
    assert_eq!(c.poll(1_000_000), None);
}

#[test]
fn paging_up_in_a_remote_room_does_not_ask_for_older_messages() {
    let mut c = on_peer_a();
    let room = remote_manager(PEER_A);
    let list: Vec<ChatMessage> = (0..HISTORY_LIMIT as u64)
        .map(|n| msg(n, room.clone(), AuthorKind::Manager, MessageKind::Text, "x"))
        .collect();
    c.history(&room, None, Ok(list));
    let mut v = view_with(c);
    assert!(type_keys(&mut v, &[b"\x1b[5~", b"\x1b[5~"]).is_empty());
}

#[test]
fn grouped_chat_renders_at_every_small_size() {
    let mut c = grouped();
    c.history(&manager_room(), None, Ok(Vec::new()));
    let v = view_with(c);
    for w in [1u16, 10, 20, 40, 56, 80] {
        for h in [1u16, 4, 8, 12] {
            draw(&v, &screens(), w, h);
        }
    }
}
