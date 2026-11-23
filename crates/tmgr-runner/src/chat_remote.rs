use crate::manager_remote::{plain, ready, remote_result, Link};
use serde_json::{json, Value};
use tmgr_ipc::chat::{
    limits, Author, AuthorKind, ChatMessage, MessageKind, RoomInfo, RoomKind, RoomRef, CHAT_V,
};
use tmgr_ipc::manager::limits as rpc_limits;
use tmgr_ipc::mgr_rpc::Op;

const OTHER_MACHINE: &str = "the room belongs to another machine";

pub fn rooms(link: &dyn Link) -> Vec<RoomInfo> {
    if !link.top() {
        return Vec::new();
    }
    link.machines()
        .rows()
        .into_iter()
        .map(|(id, label)| RoomInfo {
            room: RoomRef::manager(id),
            title: if label.is_empty() { id } else { label }.into(),
            agent: None,
            state: None,
            context_pct: None,
            context_tokens: None,
            model: None,
            last: None,
            unrestricted: false,
            mention: None,
        })
        .collect()
}

fn target<'a>(link: &dyn Link, room: &'a RoomRef) -> Result<&'a str, String> {
    if !link.top() {
        return Err(OTHER_MACHINE.into());
    }
    if room.kind != RoomKind::Manager {
        return Err("session rooms of other machines are not available".into());
    }
    ready(link, &room.machine)?;
    Ok(&room.machine)
}

pub fn history(
    link: &dyn Link,
    room: &RoomRef,
    before: Option<&str>,
    limit: Option<u32>,
) -> Result<Vec<ChatMessage>, String> {
    let machine = target(link, room)?;
    if before.is_some() {
        return Ok(Vec::new());
    }
    let max = rpc_limits::MAX_ROOM_LIMIT;
    let limit = limit.unwrap_or(max).clamp(1, max);
    let res = link
        .call(machine, Op::Room, json!({"limit": limit}))
        .map_err(|e| plain(&e))?;
    let rows = remote_result(machine, Op::Room, res)?;
    let Value::Array(rows) = rows else {
        return Err("the remote machine sent an unreadable room".into());
    };
    let mut all: Vec<ChatMessage> = rows
        .into_iter()
        .filter_map(|v| serde_json::from_value::<ChatMessage>(v).ok())
        .filter(|m| m.room == *room && m.validate().is_ok())
        .collect();
    all.drain(..all.len().saturating_sub(limit as usize));
    Ok(all)
}

pub fn send(
    link: &dyn Link,
    me: &str,
    room: &RoomRef,
    body: &str,
    attachments: usize,
    reply_to: Option<&str>,
) -> Result<ChatMessage, String> {
    let machine = target(link, room)?;
    if reply_to.is_some() {
        return Err("replies are not supported in rooms of other machines".into());
    }
    if attachments > 0 {
        return Err("attachments are not supported in a remote room".into());
    }
    if body.trim().is_empty() {
        return Err("body_md is empty".into());
    }
    if body.len() > limits::MAX_BODY {
        return Err(format!(
            "body_md must be at most {} bytes",
            limits::MAX_BODY
        ));
    }
    let res = link
        .call(machine, Op::Ask, json!({"text": body}))
        .map_err(|e| plain(&e))?;
    let answer = remote_result(machine, Op::Ask, res)?;
    let id = answer
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| tmgr_ipc::chat::valid_message_id(id))
        .ok_or("the remote machine sent an unreadable answer")?;
    Ok(ChatMessage {
        v: CHAT_V,
        id: id.into(),
        room: room.clone(),
        author: Author {
            kind: AuthorKind::Manager,
            id: "manager".into(),
            machine: Some(me.into()),
            device: None,
            agent: None,
        },
        ts_ms: now_ms(),
        kind: MessageKind::Text,
        body_md: body.into(),
        attachments: Vec::new(),
        reply_to: None,
        decision: None,
        session_meta: None,
        user_turn: false,
        mention: None,
    })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mgr_rpc::machines::Machines;
    use std::sync::Mutex;
    use tmgr_ipc::mgr_rpc::{ErrorCode, RpcRes};

    const ME: &str = "mx_00000000000000aa";
    const PEER: &str = "mx_00000000000000bb";
    const STRANGER: &str = "mx_00000000000000cc";
    const DEV: &str = "66666666-7777-8888-9999-aaaaaaaaaaaa";
    const DEV2: &str = "66666666-7777-8888-9999-aaaaaaaaaaab";
    const FP2: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";
    const FP: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    struct Net {
        top: bool,
        ready: bool,
        machines: Machines,
        reply: Mutex<Result<RpcRes, String>>,
        calls: Mutex<Vec<(String, Op, Value)>>,
    }

    impl Net {
        fn top() -> Net {
            Net {
                top: true,
                ready: true,
                machines: Machines::parse(&format!(
                    r#"{{"machines":[{{"id":"{PEER}","label":"laptop","device_id":"{DEV}","fp":"{FP}"}},{{"id":"{STRANGER}","label":"","device_id":"{DEV2}","fp":"{FP2}"}}]}}"#
                ))
                .unwrap(),
                reply: Mutex::new(Ok(RpcRes::ok("r", json!([])))),
                calls: Mutex::default(),
            }
        }

        fn replying(self, r: Result<RpcRes, String>) -> Net {
            *self.reply.lock().unwrap() = r;
            self
        }

        fn calls(&self) -> Vec<(String, Op, Value)> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl Link for Net {
        fn top(&self) -> bool {
            self.top
        }
        fn machines(&self) -> Machines {
            self.machines.clone()
        }
        fn relay_ready(&self) -> bool {
            self.ready
        }
        fn call(&self, machine: &str, op: Op, args: Value) -> Result<RpcRes, String> {
            self.calls.lock().unwrap().push((machine.into(), op, args));
            self.reply.lock().unwrap().clone()
        }
    }

    fn msg(id: &str, room: RoomRef) -> ChatMessage {
        ChatMessage {
            v: CHAT_V,
            id: id.into(),
            room,
            author: Author {
                kind: AuthorKind::Owner,
                id: "owner".into(),
                machine: None,
                device: None,
                agent: None,
            },
            ts_ms: 1,
            kind: MessageKind::Text,
            body_md: "hello".into(),
            attachments: Vec::new(),
            reply_to: None,
            decision: None,
            session_meta: None,
            user_turn: false,
            mention: None,
        }
    }

    fn mid(n: u32) -> String {
        format!("m_{n:032x}")
    }

    #[test]
    fn rooms_is_empty_on_a_non_top() {
        let net = Net {
            top: false,
            ..Net::top()
        };
        assert!(rooms(&net).is_empty());
        assert!(net.calls().is_empty());
    }

    #[test]
    fn rooms_lists_one_manager_room_per_machine_without_a_relay_call() {
        let net = Net::top();
        let r = rooms(&net);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].room, RoomRef::manager(PEER));
        assert_eq!(r[0].title, "laptop");
        assert_eq!(r[1].room, RoomRef::manager(STRANGER));
        assert_eq!(r[1].title, STRANGER);
        assert!(r.iter().all(|i| i.last.is_none()));
        assert!(net.calls().is_empty());
    }

    #[test]
    fn history_of_another_machine_stays_refused_on_a_non_top() {
        let net = Net {
            top: false,
            ..Net::top()
        };
        let e = history(&net, &RoomRef::manager(PEER), None, None).unwrap_err();
        assert_eq!(e, OTHER_MACHINE);
        assert!(net.calls().is_empty());
    }

    #[test]
    fn send_to_another_machine_stays_refused_on_a_non_top() {
        let net = Net {
            top: false,
            ..Net::top()
        };
        let e = send(&net, ME, &RoomRef::manager(PEER), "hi", 0, None).unwrap_err();
        assert_eq!(e, OTHER_MACHINE);
        assert!(net.calls().is_empty());
    }

    #[test]
    fn a_session_room_of_another_machine_is_refused_even_on_top() {
        let net = Net::top();
        let room = RoomRef::session(PEER, "s1");
        assert!(history(&net, &room, None, None).is_err());
        assert!(send(&net, ME, &room, "hi", 0, None).is_err());
        assert!(net.calls().is_empty());
    }

    #[test]
    fn an_unknown_machine_names_the_known_ones() {
        let net = Net::top();
        let room = RoomRef::manager("mx_00000000000000dd");
        let want = format!("unknown machine mx_00000000000000dd; known: {PEER}, {STRANGER}");
        assert_eq!(history(&net, &room, None, None).unwrap_err(), want);
        assert_eq!(send(&net, ME, &room, "hi", 0, None).unwrap_err(), want);
        assert!(net.calls().is_empty());
    }

    #[test]
    fn relay_not_ready_is_reported_without_a_call() {
        let net = Net {
            ready: false,
            ..Net::top()
        };
        let room = RoomRef::manager(PEER);
        assert_eq!(
            history(&net, &room, None, None).unwrap_err(),
            "relay not ready"
        );
        assert_eq!(
            send(&net, ME, &room, "hi", 0, None).unwrap_err(),
            "relay not ready"
        );
        assert!(net.calls().is_empty());
    }

    #[test]
    fn op_denied_tells_the_owner_what_to_allow() {
        let denied = || {
            Ok(RpcRes::err(
                "r",
                ErrorCode::OpDenied,
                "operation is not allowed",
            ))
        };
        let room = RoomRef::manager(PEER);
        let net = Net::top().replying(denied());
        assert_eq!(
            history(&net, &room, None, None).unwrap_err(),
            format!("machine {PEER} refused room: allow \"room\" in manager-remote.toml on that machine")
        );
        let net = Net::top().replying(denied());
        assert_eq!(
            send(&net, ME, &room, "hi", 0, None).unwrap_err(),
            format!(
                "machine {PEER} refused ask: allow \"ask\" in manager-remote.toml on that machine"
            )
        );
    }

    #[test]
    fn a_remote_error_text_is_flattened_and_cut() {
        let long = format!("a\nb{}", "x".repeat(500));
        let net = Net::top().replying(Ok(RpcRes::err("r", ErrorCode::Refused, long)));
        let e = history(&net, &RoomRef::manager(PEER), None, None).unwrap_err();
        assert!(e.starts_with("refused: a b"), "{e}");
        assert!(!e.contains('\n'));
        assert!(e.chars().count() <= 200 + 10, "{e}");
    }

    #[test]
    fn a_transport_error_is_flattened_and_cut_like_a_remote_one() {
        let long = format!("a\nb{}", "x".repeat(500));
        let room = RoomRef::manager(PEER);
        let net = Net::top().replying(Err(long.clone()));
        let e = history(&net, &room, None, None).unwrap_err();
        assert!(e.starts_with("a b"), "{e}");
        assert!(!e.contains('\n'));
        assert_eq!(e.chars().count(), 200);
        let net = Net::top().replying(Err(long));
        let e = send(&net, ME, &room, "hi", 0, None).unwrap_err();
        assert!(!e.contains('\n'));
        assert_eq!(e.chars().count(), 200);
    }

    #[test]
    fn history_keeps_good_entries_when_one_is_malformed() {
        let room = RoomRef::manager(PEER);
        let rows = json!([
            msg(&mid(1), room.clone()),
            {"id": 5, "junk": true},
            "text",
            msg(&mid(2), room.clone()),
        ]);
        let net = Net::top().replying(Ok(RpcRes::ok("r", rows)));
        let got = history(&net, &room, None, None).unwrap();
        assert_eq!(
            got.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
            [mid(1), mid(2)]
        );
    }

    #[test]
    fn history_keeps_at_most_limit_of_the_last_entries() {
        let room = RoomRef::manager(PEER);
        let rows = json!([
            msg(&mid(1), room.clone()),
            msg(&mid(2), room.clone()),
            msg(&mid(3), room.clone()),
        ]);
        let net = Net::top().replying(Ok(RpcRes::ok("r", rows)));
        let got = history(&net, &room, None, Some(2)).unwrap();
        assert_eq!(
            got.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
            [mid(2), mid(3)]
        );
    }

    #[test]
    fn send_refuses_a_reply_to_without_a_call() {
        let net = Net::top();
        let e = send(&net, ME, &RoomRef::manager(PEER), "hi", 0, Some(&mid(1))).unwrap_err();
        assert_eq!(e, "replies are not supported in rooms of other machines");
        assert!(net.calls().is_empty());
    }

    #[test]
    fn a_transport_error_passes_through() {
        let net = Net::top().replying(Err("timed out".into()));
        let e = history(&net, &RoomRef::manager(PEER), None, None).unwrap_err();
        assert_eq!(e, "timed out");
    }

    #[test]
    fn history_asks_the_room_op_and_returns_the_messages() {
        let room = RoomRef::manager(PEER);
        let rows = json!([msg(&mid(1), room.clone()), msg(&mid(2), room.clone())]);
        let net = Net::top().replying(Ok(RpcRes::ok("r", rows)));
        let got = history(&net, &room, None, Some(30)).unwrap();
        assert_eq!(
            got.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
            [mid(1), mid(2)]
        );
        assert_eq!(net.calls(), [(PEER.into(), Op::Room, json!({"limit": 30}))]);
    }

    #[test]
    fn history_limit_is_clamped_to_the_room_op_range() {
        let room = RoomRef::manager(PEER);
        let net = Net::top();
        history(&net, &room, None, None).unwrap();
        history(&net, &room, None, Some(500)).unwrap();
        history(&net, &room, None, Some(0)).unwrap();
        let limits: Vec<u64> = net
            .calls()
            .iter()
            .map(|c| c.2["limit"].as_u64().unwrap())
            .collect();
        let max = u64::from(rpc_limits::MAX_ROOM_LIMIT);
        assert_eq!(limits, [max, max, 1]);
    }

    #[test]
    fn history_keeps_only_messages_of_the_requested_room() {
        let room = RoomRef::manager(PEER);
        let rows = json!([
            msg(&mid(1), room.clone()),
            msg(&mid(2), RoomRef::manager(ME)),
            msg(&mid(3), RoomRef::session(PEER, "s1")),
            msg(&mid(4), room.clone()),
        ]);
        let net = Net::top().replying(Ok(RpcRes::ok("r", rows)));
        let got = history(&net, &room, None, None).unwrap();
        assert_eq!(
            got.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
            [mid(1), mid(4)]
        );
    }

    #[test]
    fn history_drops_invalid_messages() {
        let room = RoomRef::manager(PEER);
        let mut bad = msg(&mid(2), room.clone());
        bad.id = "nope".into();
        let rows = json!([msg(&mid(1), room.clone()), bad]);
        let net = Net::top().replying(Ok(RpcRes::ok("r", rows)));
        assert_eq!(history(&net, &room, None, None).unwrap().len(), 1);
    }

    #[test]
    fn history_rejects_an_answer_that_is_not_a_message_list() {
        let net = Net::top().replying(Ok(RpcRes::ok("r", json!("text"))));
        let e = history(&net, &RoomRef::manager(PEER), None, None).unwrap_err();
        assert_eq!(e, "the remote machine sent an unreadable room");
    }

    #[test]
    fn history_before_is_empty_without_a_relay_call() {
        let net = Net::top();
        let got = history(&net, &RoomRef::manager(PEER), Some(&mid(1)), None).unwrap();
        assert!(got.is_empty());
        assert!(net.calls().is_empty());
    }

    #[test]
    fn send_asks_with_the_body_and_echoes_the_remote_id() {
        let net = Net::top().replying(Ok(RpcRes::ok("r", json!({"id": mid(9)}))));
        let room = RoomRef::manager(PEER);
        let echo = send(&net, ME, &room, "  do it ", 0, None).unwrap();
        assert_eq!(
            net.calls(),
            [(PEER.into(), Op::Ask, json!({"text": "  do it "}))]
        );
        assert_eq!(echo.id, mid(9));
        assert_eq!(echo.room, room);
        assert_eq!(echo.body_md, "  do it ");
        assert_eq!(echo.author.kind, AuthorKind::Manager);
        assert_eq!(echo.author.machine.as_deref(), Some(ME));
        assert_eq!(echo.kind, MessageKind::Text);
        echo.validate().unwrap();
    }

    #[test]
    fn send_refuses_attachments_empty_and_oversized_bodies_without_a_call() {
        let net = Net::top();
        let room = RoomRef::manager(PEER);
        assert!(send(&net, ME, &room, "hi", 1, None)
            .unwrap_err()
            .contains("attachments"));
        assert!(send(&net, ME, &room, "  ", 0, None).is_err());
        let big = "x".repeat(limits::MAX_BODY + 1);
        assert!(send(&net, ME, &room, &big, 0, None).is_err());
        assert!(net.calls().is_empty());
    }

    #[test]
    fn send_rejects_an_answer_without_a_valid_id() {
        for r in [json!({}), json!({"id": "bad"}), json!("x")] {
            let net = Net::top().replying(Ok(RpcRes::ok("r", r)));
            let e = send(&net, ME, &RoomRef::manager(PEER), "hi", 0, None).unwrap_err();
            assert_eq!(e, "the remote machine sent an unreadable answer");
        }
    }

    #[test]
    fn room_kind_is_manager_for_listed_rooms() {
        assert!(rooms(&Net::top())
            .iter()
            .all(|i| i.room.kind == RoomKind::Manager));
    }
}
