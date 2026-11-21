//! Cross-machine manager RPC payloads. JSON schema: `schema/mgr-rpc.v1.json`.
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MGR_RPC_V: u32 = 1;
pub const KIND_REQ: &str = "mgr_req";
pub const KIND_RES: &str = "mgr_res";
pub const LETTER_REQ: char = 'g';
pub const LETTER_RES: char = 'h';

pub mod limits {
    pub const MAX_REQ_BYTES: usize = 96 * 1024;
    pub const MAX_RES_BYTES: usize = 1024 * 1024;
    pub const FRESH_MS: u64 = 300_000;
    pub const SEEN_MARGIN_MS: u64 = 60_000;
    pub const RPC_PER_MINUTE: usize = 30;
    pub const MAX_SEEN_PER_SENDER: usize = 512;
    pub const SWITCHES_PER_MINUTE: usize = 2;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Op {
    List,
    Read,
    Send,
    Ask,
    SpawnChallenge,
    Spawn,
    Switch,
}

impl Op {
    pub const ALL: [Op; 7] = [
        Op::List,
        Op::Read,
        Op::Send,
        Op::Ask,
        Op::SpawnChallenge,
        Op::Spawn,
        Op::Switch,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Op::List => "list",
            Op::Read => "read",
            Op::Send => "send",
            Op::Ask => "ask",
            Op::SpawnChallenge => "spawn_challenge",
            Op::Spawn => "spawn",
            Op::Switch => "switch",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    BadRequest,
    NotAllowed,
    Stale,
    Depth,
    RateLimited,
    OpDenied,
    Unavailable,
    Refused,
}

impl ErrorCode {
    pub const ALL: [ErrorCode; 8] = [
        ErrorCode::BadRequest,
        ErrorCode::NotAllowed,
        ErrorCode::Stale,
        ErrorCode::Depth,
        ErrorCode::RateLimited,
        ErrorCode::OpDenied,
        ErrorCode::Unavailable,
        ErrorCode::Refused,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::BadRequest => "bad_request",
            ErrorCode::NotAllowed => "not_allowed",
            ErrorCode::Stale => "stale",
            ErrorCode::Depth => "depth",
            ErrorCode::RateLimited => "rate_limited",
            ErrorCode::OpDenied => "op_denied",
            ErrorCode::Unavailable => "unavailable",
            ErrorCode::Refused => "refused",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcReq {
    pub v: u32,
    pub rpc: String,
    pub op: Op,
    pub args: Value,
    pub chain: Vec<String>,
    pub ts_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcError {
    pub code: ErrorCode,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RpcRes {
    pub v: u32,
    pub rpc: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl RpcRes {
    pub fn ok(rpc: &str, result: Value) -> RpcRes {
        RpcRes {
            v: MGR_RPC_V,
            rpc: rpc.into(),
            ok: true,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(rpc: &str, code: ErrorCode, message: impl Into<String>) -> RpcRes {
        RpcRes {
            v: MGR_RPC_V,
            rpc: rpc.into(),
            ok: false,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
            }),
        }
    }
}

pub fn valid_machine_id(s: &str) -> bool {
    s.strip_prefix("mx_")
        .is_some_and(|r| r.len() == 16 && r.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chat::{Author, AuthorKind, ChatMessage, MessageKind, RoomRef};
    use serde_json::{json, Value};

    const SCHEMA: &str = include_str!("../schema/mgr-rpc.v1.json");

    fn req_json() -> Value {
        json!({"v":1,"rpc":"r-1","op":"read","args":{"session":"s_abcdefghij"},"chain":[],"ts_ms":5})
    }

    #[test]
    fn a_request_round_trips() {
        let r: RpcReq = serde_json::from_value(req_json()).unwrap();
        assert_eq!(r.op, Op::Read);
        assert_eq!(serde_json::to_value(&r).unwrap(), req_json());
    }

    #[test]
    fn requests_with_unknown_fields_or_ops_do_not_parse() {
        let mut extra = req_json();
        extra["approval"] = json!({});
        assert!(serde_json::from_value::<RpcReq>(extra).is_err());
        let mut op = req_json();
        op["op"] = json!("rm");
        assert!(serde_json::from_value::<RpcReq>(op).is_err());
        let mut missing = req_json();
        missing.as_object_mut().unwrap().remove("chain");
        assert!(serde_json::from_value::<RpcReq>(missing).is_err());
    }

    #[test]
    fn responses_omit_the_unused_side() {
        let ok = serde_json::to_value(RpcRes::ok("r", json!({"x":1}))).unwrap();
        assert_eq!(ok, json!({"v":1,"rpc":"r","ok":true,"result":{"x":1}}));
        let err = serde_json::to_value(RpcRes::err("r", ErrorCode::Stale, "old")).unwrap();
        assert_eq!(
            err,
            json!({"v":1,"rpc":"r","ok":false,"error":{"code":"stale","message":"old"}})
        );
    }

    #[test]
    fn machine_ids_are_mx_and_sixteen_lowercase_hex() {
        assert!(valid_machine_id("mx_0123456789abcdef"));
        for bad in [
            "",
            "mx_",
            "mx_0123456789ABCDEF",
            "mx_0123456789abcde",
            "mx_0123456789abcdef0",
            "my_0123456789abcdef",
            "mx_0123456789abcdeg",
        ] {
            assert!(!valid_machine_id(bad), "{bad}");
        }
    }

    #[test]
    fn schema_matches_the_types() {
        let s: Value = serde_json::from_str(SCHEMA).unwrap();
        let keys = |v: &Value| -> Vec<String> {
            let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
            k.sort();
            k
        };
        let req: RpcReq = serde_json::from_value(req_json()).unwrap();
        assert_eq!(
            keys(&s["$defs"]["req"]["properties"]),
            keys(&serde_json::to_value(&req).unwrap())
        );
        let mut required: Vec<&str> = s["$defs"]["req"]["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        required.sort();
        assert_eq!(required, ["args", "chain", "op", "rpc", "ts_ms", "v"]);
        let res = RpcRes::err("r", ErrorCode::Refused, "m");
        let mut res_keys = keys(&serde_json::to_value(&res).unwrap());
        res_keys.push("result".into());
        res_keys.sort();
        assert_eq!(keys(&s["$defs"]["res"]["properties"]), res_keys);
        let ops: Vec<&str> = s["$defs"]["req"]["properties"]["op"]["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(ops, Op::ALL.map(Op::as_str));
        let codes: Vec<&str> = s["$defs"]["res"]["properties"]["error"]["properties"]["code"]
            ["enum"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(codes, ErrorCode::ALL.map(ErrorCode::as_str));
        for op in Op::ALL {
            assert_eq!(serde_json::to_value(op).unwrap(), json!(op.as_str()));
        }
        for c in ErrorCode::ALL {
            assert_eq!(serde_json::to_value(c).unwrap(), json!(c.as_str()));
        }
    }

    #[test]
    fn relay_kinds_and_letters_are_fixed() {
        assert_eq!((KIND_REQ, KIND_RES), ("mgr_req", "mgr_res"));
        assert_eq!((LETTER_REQ, LETTER_RES), ('g', 'h'));
        assert_eq!(MGR_RPC_V, 1);
    }

    #[test]
    fn a_remote_ask_message_validates_with_a_foreign_author_machine() {
        let m = ChatMessage {
            v: crate::chat::CHAT_V,
            id: crate::chat::new_message_id(),
            room: RoomRef::manager("mx_00000000000000aa"),
            author: Author {
                kind: AuthorKind::Manager,
                id: "manager".into(),
                machine: Some("mx_00000000000000bb".into()),
                device: None,
                agent: None,
            },
            ts_ms: 1,
            kind: MessageKind::Text,
            body_md: "hi".into(),
            attachments: Vec::new(),
            reply_to: None,
            decision: None,
            session_meta: None,
        };
        m.validate().unwrap();
    }
}
