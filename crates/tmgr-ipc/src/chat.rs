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
    pub const QUEUED: &str = "chat.queued";
    pub const QUEUED_WHY: &str = "chat.queued_why";
    pub const CANCEL: &str = "chat.cancel";
    pub const MENTION_INJECT: &str = "chat.mention_inject";
    pub const BLOB_PUT: &str = "chat.blob.put";
}

pub mod limits {
    pub const MAX_BODY: usize = 64 << 10;
    pub const MAX_ATTACHMENTS: usize = 16;
    pub const MAX_NAME: usize = 255;
    pub const MAX_ID: usize = 64;
    pub const MAX_HISTORY: u32 = 500;
    pub const MAX_QUERY: usize = 256;
    pub const MAX_MODEL: usize = 128;
    pub const MAX_SPAWN_REQUEST: usize = 16 << 10;
    pub const MAX_SPAWN_SUMMARY: usize = 2048;
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
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub user_turn: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mention: Option<MentionRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MentionRef {
    pub id: String,
    pub target: RoomRef,
    pub target_title: String,
    pub state: MentionState,
    pub hop: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MentionState {
    Queued,
    Sent,
    Answering,
    Answered,
    Busy,
    Offline,
    Timeout,
    Interrupted,
    Expired,
    Refused,
    RateLimited,
    Cancelled,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MentionReq {
    pub target: RoomRef,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirmed_risky: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MentionTarget {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub risky: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MentionInjectParams {
    pub room: RoomRef,
    pub mention_id: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub confirmed_risky: bool,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spawn: Option<SpawnCard>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permit: Option<PermitCardMsg>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub closed: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PermitCardMsg {
    pub session: String,
    pub runner_id: String,
    pub runner_fp: String,
    pub nonce: String,
    pub exp_ms: u64,
    pub req_sha256: String,
    pub reissue: u8,
    pub agent: String,
    pub tool: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub danger: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub always_label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnCard {
    pub summary: String,
    pub request: String,
    pub nonce: String,
    pub exp_ms: u64,
    pub target_id: String,
    pub target_fp: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposed_by: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
}

impl SpawnCard {
    fn validate(&self) -> Result<(), String> {
        let lower_hex = |s: &str, n: usize| {
            s.len() == n && s.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
        };
        if !lower_hex(&self.nonce, 32) {
            return Err("invalid spawn nonce".into());
        }
        if !valid_sha256(&self.target_fp) {
            return Err("invalid spawn target_fp".into());
        }
        if !valid_token(&self.target_id) {
            return Err("invalid spawn target_id".into());
        }
        if self.request.is_empty() || self.request.len() > limits::MAX_SPAWN_REQUEST {
            return Err("invalid spawn request".into());
        }
        if self.summary.len() > limits::MAX_SPAWN_SUMMARY {
            return Err("spawn summary too long".into());
        }
        if self.proposed_by.as_ref().is_some_and(|p| p.len() > 256)
            || self.prompt.as_ref().is_some_and(|p| p.len() > 1000)
        {
            return Err("spawn card detail too long".into());
        }
        Ok(())
    }
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
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<ChatMessage>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub unrestricted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mention: Option<MentionTarget>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mention: Option<MentionReq>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobPutParams {
    pub v: u32,
    pub sha256: String,
    pub size: u64,
    pub part: u32,
    pub parts: u32,
    pub data: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlobPutResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub have: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parts: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complete: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueuedParams {
    pub room: RoomRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelParams {
    pub room: RoomRef,
    pub id: String,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityState {
    Working,
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatActivity {
    pub v: u32,
    pub room: RoomRef,
    pub state: ActivityState,
    pub current: Option<String>,
    pub used: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub calls: Vec<ToolCall>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCall {
    #[serde(default)]
    pub n: u32,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub running: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub error: bool,
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
        if let Some(m) = &self.mention {
            m.target.validate()?;
            if !valid_message_id(&m.id) {
                return Err("invalid mention id".into());
            }
        }
        match (&self.kind, &self.decision) {
            (MessageKind::Decision, None) => {
                Err("a decision message needs decision.card_id".into())
            }
            (_, Some(d)) if !valid_token(&d.card_id) => Err("invalid decision card id".into()),
            (MessageKind::Decision, Some(d)) => {
                d.spawn.as_ref().map_or(Ok(()), SpawnCard::validate)
            }
            (_, Some(d)) if d.spawn.is_some() => Err("spawn is only valid on a decision".into()),
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
            json!({
                "v": 1, "id": "m_0190a1b2c3d8aabbccddeeff00112233",
                "room": {"kind": "session", "machine": "mx_7f3a", "session": "s_abcdefghij"},
                "author": {"kind": "system", "id": "runner"},
                "ts_ms": 1795000000400u64, "kind": "system", "body_md": "sent",
                "reply_to": "m_0190a1b2c3d4aabbccddeeff00112233",
                "mention": {"id": "m_0190a1b2c3d4aabbccddeeff00112233",
                    "target": {"kind": "session", "machine": "mx_7f3a", "session": "s_target0001"},
                    "target_title": "build-fix", "state": "sent", "hop": 1}
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
                "mention",
                "reply_to",
                "room",
                "session_meta",
                "ts_ms",
                "user_turn",
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
    fn spawn_card() -> Value {
        json!({
            "summary": "claude-code in ~/x",
            "request": "{\"v\":1}",
            "nonce": "ab".repeat(16),
            "exp_ms": 1795000900000u64,
            "target_id": "d_runner1",
            "target_fp": "cd".repeat(32)
        })
    }

    fn spawn_message(spawn: Value) -> Value {
        let mut e = examples().remove(2);
        e["decision"]["spawn"] = spawn;
        e
    }

    #[test]
    fn a_spawn_card_round_trips_and_validates() {
        let e = spawn_message(spawn_card());
        let m: ChatMessage = serde_json::from_value(e.clone()).unwrap();
        m.validate().unwrap();
        assert_eq!(
            m.decision.as_ref().unwrap().spawn.as_ref().unwrap().exp_ms,
            1795000900000
        );
        assert_eq!(serde_json::to_value(&m).unwrap(), e);
    }

    fn permit_card() -> Value {
        json!({
            "session": "s_abcdefghij", "runner_id": "mx_7f3a", "runner_fp": "ab".repeat(32),
            "nonce": "cd".repeat(16), "exp_ms": 1795000120000u64, "req_sha256": "ef".repeat(32),
            "reissue": 1, "agent": "claude-code", "tool": "Bash", "text": "ls",
            "danger": ["deletes files"], "always_label": "Always allow ls"
        })
    }

    #[test]
    fn a_permit_card_round_trips_on_a_decision() {
        let mut e = examples().remove(2);
        e["decision"]["permit"] = permit_card();
        e["decision"]["closed"] = json!("answered on Mac");
        let m: ChatMessage = serde_json::from_value(e.clone()).unwrap();
        m.validate().unwrap();
        let d = m.decision.as_ref().unwrap();
        assert_eq!(d.permit.as_ref().unwrap().reissue, 1);
        assert_eq!(d.closed.as_deref(), Some("answered on Mac"));
        assert_eq!(serde_json::to_value(&m).unwrap(), e);
    }

    #[test]
    fn an_old_decision_has_no_permit_or_closed_and_serializes_without_them() {
        let m: ChatMessage = serde_json::from_value(examples().remove(2)).unwrap();
        let d = m.decision.as_ref().unwrap();
        assert!(d.permit.is_none() && d.closed.is_none());
        let s = serde_json::to_string(&m).unwrap();
        assert!(!s.contains("permit") && !s.contains("closed"));
    }

    #[test]
    fn a_permit_card_without_danger_omits_it() {
        let mut c = permit_card();
        c.as_object_mut().unwrap().remove("danger");
        let p: PermitCardMsg = serde_json::from_value(c).unwrap();
        assert!(p.danger.is_empty());
        assert!(serde_json::to_value(&p).unwrap().get("danger").is_none());
    }

    #[test]
    fn a_decision_without_spawn_serializes_without_the_field() {
        let m: ChatMessage = serde_json::from_value(examples().remove(2)).unwrap();
        assert!(m.decision.as_ref().unwrap().spawn.is_none());
        assert!(!serde_json::to_string(&m).unwrap().contains("spawn"));
    }

    #[test]
    fn a_malformed_spawn_card_is_refused() {
        let mutate = |f: &dyn Fn(&mut Value)| {
            let mut c = spawn_card();
            f(&mut c);
            let m: ChatMessage = serde_json::from_value(spawn_message(c)).unwrap();
            m.validate()
        };
        assert!(mutate(&|c| c["nonce"] = json!("AB".repeat(16))).is_err());
        assert!(mutate(&|c| c["nonce"] = json!("ab")).is_err());
        assert!(mutate(&|c| c["target_fp"] = json!("CD".repeat(32))).is_err());
        assert!(mutate(&|c| c["target_fp"] = json!("cd")).is_err());
        assert!(mutate(&|c| c["request"] = json!("")).is_err());
        assert!(mutate(&|c| c["request"] = json!("x".repeat(16 * 1024 + 1))).is_err());
        assert!(mutate(&|c| c["summary"] = json!("x".repeat(2049))).is_err());
        assert!(mutate(&|c| c["target_id"] = json!("bad id")).is_err());
        assert!(mutate(&|_| ()).is_ok());
        assert!(mutate(&|c| c["request"] = json!("x".repeat(16 * 1024))).is_ok());
    }

    #[test]
    fn a_spawn_card_may_carry_the_proposer_and_the_prompt_and_limits_them() {
        let mut c = spawn_card();
        c["proposed_by"] = json!("top manager on laptop");
        c["prompt"] = json!("go");
        let e = spawn_message(c.clone());
        let m: ChatMessage = serde_json::from_value(e.clone()).unwrap();
        m.validate().unwrap();
        assert_eq!(serde_json::to_value(&m).unwrap(), e);
        let mut long = c.clone();
        long["prompt"] = json!("x".repeat(1001));
        let m: ChatMessage = serde_json::from_value(spawn_message(long)).unwrap();
        assert!(m.validate().is_err());
        let mut long = c;
        long["proposed_by"] = json!("x".repeat(257));
        let m: ChatMessage = serde_json::from_value(spawn_message(long)).unwrap();
        assert!(m.validate().is_err());
    }

    #[test]
    fn spawn_is_only_valid_on_a_decision_message() {
        let mut e = spawn_message(spawn_card());
        e["kind"] = json!("text");
        let m: ChatMessage = serde_json::from_value(e).unwrap();
        assert!(m.validate().is_err());
    }

    #[test]
    fn an_unknown_mention_state_decodes_to_other() {
        let r: MentionRef = serde_json::from_value(json!({
            "id": "m_0190a1b2c3d4aabbccddeeff00112233",
            "target": {"kind": "session", "machine": "mx", "session": "s1"},
            "target_title": "t", "state": "from_the_future", "hop": 1
        }))
        .unwrap();
        assert_eq!(r.state, MentionState::Other);
    }

    #[test]
    fn mention_fields_are_omitted_when_absent() {
        let p: SendParams = serde_json::from_value(json!({
            "room": {"kind": "session", "machine": "mx", "session": "s1"}, "body_md": "x"
        }))
        .unwrap();
        assert!(p.mention.is_none());
        assert!(serde_json::to_value(&p).unwrap().get("mention").is_none());
        let req: MentionReq = serde_json::from_value(json!({
            "target": {"kind": "session", "machine": "mx", "session": "s2"}
        }))
        .unwrap();
        assert!(!req.confirmed_risky);
    }
}
