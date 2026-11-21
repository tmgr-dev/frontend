//! Manager chat contract (`chat.v1`): the message envelope shared by the runner,
//! the desktop app, the relay (as sealed plaintext) and the mobile app, plus the
//! local IPC request and push types. JSON schema: `schema/chat-message.v1.json`.
//!
//! Compatibility: readers ignore unknown fields; an unknown `kind` decodes to
//! `Other` and renders like `system`; an unknown room or author kind is refused.

use serde::{Deserialize, Serialize};

pub const CHAT_V: u32 = 1;
pub const PUSH_TYPE: &str = "chat";

pub mod method {
    pub const ROOMS: &str = "chat.rooms";
    pub const HISTORY: &str = "chat.history";
    pub const SEND: &str = "chat.send";
    pub const SEARCH: &str = "chat.search";
    pub const SUBSCRIBE: &str = "chat.subscribe";
    pub const RELAY_SELF: &str = "chat.relay_self";
}

pub mod limits {
    pub const MAX_BODY: usize = 64 << 10;
    pub const MAX_ATTACHMENTS: usize = 16;
    pub const MAX_NAME: usize = 255;
    pub const MAX_ID: usize = 64;
    pub const MAX_HISTORY: u32 = 500;
    pub const MAX_QUERY: usize = 256;
    pub const MAX_MODEL: usize = 128;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatMessage {
    pub v: u32,
    pub id: String,
    pub room: RoomRef,
    pub author: Author,
    pub ts_ms: u64,
    pub kind: MessageKind,
    pub body_md: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<DecisionRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_meta: Option<SessionMeta>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMeta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_pct: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

impl SessionMeta {
    pub fn validate(&self) -> Result<(), String> {
        if self.context_pct.is_some_and(|p| p > 100) {
            return Err("session_meta.context_pct must be 0 to 100".into());
        }
        if self.agent.as_deref().is_some_and(|a| !valid_token(a)) {
            return Err("invalid session_meta.agent".into());
        }
        if self.model.as_deref().is_some_and(|m| {
            m.is_empty() || m.len() > limits::MAX_MODEL || !m.bytes().all(|b| b.is_ascii_graphic())
        }) {
            return Err("invalid session_meta.model".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RoomRef {
    pub kind: RoomKind,
    pub machine: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomKind {
    Manager,
    Session,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Author {
    pub kind: AuthorKind,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub machine: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorKind {
    Owner,
    Manager,
    Session,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Text,
    Decision,
    System,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    pub name: String,
    pub mime: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionRef {
    pub card_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomInfo {
    pub room: RoomRef,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_pct: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<ChatMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryParams {
    pub room: RoomRef,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SendParams {
    pub room: RoomRef,
    pub body_md: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<Attachment>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ts_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelaySelf {
    pub id: String,
    pub device_id: String,
    pub fp: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchParams {
    pub query: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub room: Option<RoomRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatPush {
    pub message: ChatMessage,
}

pub fn new_message_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    message_id_at(now, rand10())
}

fn rand10() -> [u8; 10] {
    let mut r = [0u8; 10];
    getrandom::fill(&mut r).expect("system random source");
    r
}

pub fn message_id_at(ms: u64, random: [u8; 10]) -> String {
    let mut s = format!("m_{:012x}", ms & 0xffff_ffff_ffff);
    for b in random {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

pub fn valid_message_id(s: &str) -> bool {
    s.strip_prefix("m_")
        .is_some_and(|r| r.len() == 32 && r.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')))
}

fn valid_token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= limits::MAX_ID
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-' || b == b'.')
}

fn valid_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

impl RoomRef {
    pub fn manager(machine: &str) -> RoomRef {
        RoomRef {
            kind: RoomKind::Manager,
            machine: machine.into(),
            session: None,
        }
    }

    pub fn session(machine: &str, session: &str) -> RoomRef {
        RoomRef {
            kind: RoomKind::Session,
            machine: machine.into(),
            session: Some(session.into()),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if !valid_token(&self.machine) {
            return Err("room.machine must be 1 to 64 characters of A-Za-z0-9._-".into());
        }
        match (self.kind, self.session.as_deref()) {
            (RoomKind::Manager, None) => Ok(()),
            (RoomKind::Manager, Some(_)) => Err("a manager room has no session".into()),
            (RoomKind::Session, Some(s)) if valid_token(s) => Ok(()),
            (RoomKind::Session, _) => Err("a session room needs a valid session id".into()),
        }
    }
}

impl ChatMessage {
    pub fn validate(&self) -> Result<(), String> {
        if self.v != CHAT_V {
            return Err(format!("unsupported chat version {}", self.v));
        }
        if !valid_message_id(&self.id) {
            return Err("invalid message id".into());
        }
        self.room.validate()?;
        if !valid_token(&self.author.id) {
            return Err("invalid author id".into());
        }
        for t in [
            &self.author.machine,
            &self.author.device,
            &self.author.agent,
        ]
        .into_iter()
        .flatten()
        {
            if !valid_token(t) {
                return Err("invalid author machine, device or agent".into());
            }
        }
        if self.body_md.len() > limits::MAX_BODY {
            return Err("body_md is too large".into());
        }
        if self.attachments.len() > limits::MAX_ATTACHMENTS {
            return Err("too many attachments".into());
        }
        for a in &self.attachments {
            if a.name.is_empty()
                || a.name.len() > limits::MAX_NAME
                || a.name.contains(['/', '\\', '\0'])
            {
                return Err("invalid attachment name".into());
            }
            if a.mime.is_empty() || a.mime.len() > limits::MAX_NAME {
                return Err("invalid attachment mime".into());
            }
            if !valid_sha256(&a.sha256) {
                return Err("attachment sha256 must be 64 lowercase hex characters".into());
            }
        }
        if self
            .reply_to
            .as_deref()
            .is_some_and(|r| !valid_message_id(r))
        {
            return Err("invalid reply_to".into());
        }
        if let Some(meta) = &self.session_meta {
            meta.validate()?;
        }
        match (&self.kind, &self.decision) {
            (MessageKind::Decision, None) => {
                Err("a decision message needs decision.card_id".into())
            }
            (_, Some(d)) if !valid_token(&d.card_id) => Err("invalid decision card id".into()),
            _ => Ok(()),
        }
    }

    pub fn renders_as_system(&self) -> bool {
        matches!(self.kind, MessageKind::System | MessageKind::Other)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const SCHEMA: &str = include_str!("../schema/chat-message.v1.json");

    fn examples() -> Vec<Value> {
        vec![
            json!({
                "v": 1, "id": "m_0190a1b2c3d4aabbccddeeff00112233",
                "room": {"kind": "manager", "machine": "mx_7f3a"},
                "author": {"kind": "owner", "id": "owner", "device": "d_iphone1"},
                "ts_ms": 1795000000000u64, "kind": "text",
                "body_md": "Status of **TM-12**?"
            }),
            json!({
                "v": 1, "id": "m_0190a1b2c3d5aabbccddeeff00112233",
                "room": {"kind": "session", "machine": "mx_7f3a", "session": "s_abcdefghij"},
                "author": {"kind": "session", "id": "s_abcdefghij", "machine": "mx_7f3a", "agent": "codex"},
                "ts_ms": 1795000000100u64, "kind": "text", "body_md": "Done, tests green.",
                "session_meta": {"context_pct": 42, "context_tokens": 84000, "agent": "codex", "model": "gpt-5-codex"},
                "reply_to": "m_0190a1b2c3d4aabbccddeeff00112233",
                "attachments": [{"name": "diff.patch", "mime": "text/x-diff", "size": 120,
                    "sha256": "a".repeat(64)}]
            }),
            json!({
                "v": 1, "id": "m_0190a1b2c3d6aabbccddeeff00112233",
                "room": {"kind": "manager", "machine": "mx_7f3a"},
                "author": {"kind": "manager", "id": "s_manager01", "machine": "mx_7f3a", "agent": "claude-code"},
                "ts_ms": 1795000000200u64, "kind": "decision", "body_md": "Which database?",
                "decision": {"card_id": "c_0190a1b2c3d4aabbccddeeff00112233"}
            }),
            json!({
                "v": 1, "id": "m_0190a1b2c3d7aabbccddeeff00112233",
                "room": {"kind": "manager", "machine": "mx_7f3a"},
                "author": {"kind": "system", "id": "runner"},
                "ts_ms": 1795000000300u64, "kind": "system", "body_md": "Session s_abcdefghij exited",
                "future_field": {"x": 1}
            }),
        ]
    }

    #[test]
    fn the_examples_round_trip_and_validate() {
        for e in examples() {
            let m: ChatMessage = serde_json::from_value(e.clone()).unwrap();
            m.validate().unwrap();
            let back = serde_json::to_value(&m).unwrap();
            let mut expected = e.clone();
            expected.as_object_mut().unwrap().remove("future_field");
            assert_eq!(back, expected);
        }
    }

    #[test]
    fn the_schema_lists_the_same_fields_as_the_type() {
        let schema: Value = serde_json::from_str(SCHEMA).unwrap();
        let mut required: Vec<&str> = schema["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        required.sort();
        assert_eq!(
            required,
            ["author", "body_md", "id", "kind", "room", "ts_ms", "v"]
        );
        let mut props: Vec<&String> = schema["properties"].as_object().unwrap().keys().collect();
        props.sort();
        assert_eq!(
            props,
            [
                "attachments",
                "author",
                "body_md",
                "decision",
                "id",
                "kind",
                "reply_to",
                "room",
                "session_meta",
                "ts_ms",
                "v"
            ]
        );
        for e in examples() {
            let m: ChatMessage = serde_json::from_value(e).unwrap();
            let v = serde_json::to_value(&m).unwrap();
            for k in v.as_object().unwrap().keys() {
                assert!(
                    schema["properties"].get(k).is_some(),
                    "{k} missing in schema"
                );
            }
        }
    }

    #[test]
    fn an_unknown_kind_renders_as_system_and_unknown_rooms_are_refused() {
        let mut e = examples().remove(0);
        e["kind"] = json!("poll");
        let m: ChatMessage = serde_json::from_value(e.clone()).unwrap();
        assert!(m.renders_as_system());
        e["room"]["kind"] = json!("group");
        assert!(serde_json::from_value::<ChatMessage>(e).is_err());
    }

    #[test]
    fn invalid_messages_are_refused() {
        let base: ChatMessage = serde_json::from_value(examples().remove(0)).unwrap();
        let mut m = base.clone();
        m.room.session = Some("s_x".into());
        assert!(m.validate().is_err());
        let mut m = base.clone();
        m.room = RoomRef {
            kind: RoomKind::Session,
            machine: "mx".into(),
            session: None,
        };
        assert!(m.validate().is_err());
        let mut m = base.clone();
        m.kind = MessageKind::Decision;
        assert!(m.validate().is_err());
        let mut m = base.clone();
        m.body_md = "x".repeat(limits::MAX_BODY + 1);
        assert!(m.validate().is_err());
        let mut m = base.clone();
        m.room.machine = "../etc".into();
        assert!(m.validate().is_err());
        let mut m = base;
        m.attachments.push(Attachment {
            name: "a/b".into(),
            mime: "text/plain".into(),
            size: 1,
            sha256: "a".repeat(64),
        });
        assert!(m.validate().is_err());
    }

    #[test]
    fn invalid_session_meta_is_refused() {
        let base: ChatMessage = serde_json::from_value(examples().remove(1)).unwrap();
        let bad = [
            SessionMeta {
                context_pct: Some(101),
                ..SessionMeta::default()
            },
            SessionMeta {
                agent: Some("../x".into()),
                ..SessionMeta::default()
            },
            SessionMeta {
                model: Some(String::new()),
                ..SessionMeta::default()
            },
            SessionMeta {
                model: Some("a b".into()),
                ..SessionMeta::default()
            },
            SessionMeta {
                model: Some("m".repeat(limits::MAX_MODEL + 1)),
                ..SessionMeta::default()
            },
        ];
        for meta in bad {
            let mut m = base.clone();
            m.session_meta = Some(meta);
            assert!(m.validate().is_err());
        }
        let mut m = base;
        m.session_meta = Some(SessionMeta {
            model: Some("claude-opus-4-1[1m]".into()),
            ..SessionMeta::default()
        });
        m.validate().unwrap();
    }

    #[test]
    fn message_ids_sort_by_time_and_validate() {
        let a = message_id_at(1, [0; 10]);
        let b = message_id_at(2, [0xff; 10]);
        assert!(a < b);
        assert!(valid_message_id(&a) && valid_message_id(&new_message_id()));
        assert!(!valid_message_id("c_0190a1b2c3d4aabbccddeeff00112233"));
    }
}
