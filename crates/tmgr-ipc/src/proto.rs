use crate::api::{ErrorCode, EventMsg, ScreenPush};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const PROTO_VERSION: u32 = 1;

const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";

pub fn new_session_id() -> String {
    let mut b = [0u8; 10];
    getrandom::fill(&mut b).expect("system random source");
    let mut s = String::from("s_");
    s.extend(b.iter().map(|x| ALPHABET[(x & 31) as usize] as char));
    s
}

pub fn valid_session_id(s: &str) -> bool {
    match s.strip_prefix("s_") {
        Some(rest) => rest.len() == 10 && rest.bytes().all(|c| ALPHABET.contains(&c)),
        None => false,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hello {
    pub proto: u32,
    #[serde(default)]
    pub client: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub pid: u32,
    #[serde(default)]
    pub nonce_c: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Challenge {
    pub nonce_s: String,
    pub proof_s: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Auth {
    pub proof_c: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Welcome {
    pub proto: u32,
    pub runner_version: String,
    #[serde(default)]
    pub caps: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
}

/// Handshake messages on the main socket, in both directions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Handshake {
    Hello(Hello),
    Challenge(Challenge),
    Auth(Auth),
    Welcome(Welcome),
    Error(ErrorBody),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ok: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub err: Option<ErrorBody>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Resize {
    pub session: String,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scroll {
    pub session: String,
    pub offset: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Focus {
    pub session: String,
}

/// Client to server JSON messages after the handshake. Input travels as a binary
/// frame with `TAG_INPUT`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMsg {
    Request(Request),
    Resize(Resize),
    Scroll(Scroll),
    Focus(Focus),
    Detach,
}

/// Server to client JSON messages. `Screen` and `Event` carry JSON objects
/// (`ScreenPush`, `EventMsg`); a `type` this build does not know decodes to
/// `Unknown` and must be ignored.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMsg {
    Response(Response),
    Screen(Value),
    Event(Value),
    Closed {
        reason: String,
    },
    #[serde(other)]
    Unknown,
}

impl ServerMsg {
    pub fn event(&self) -> Option<EventMsg> {
        match self {
            ServerMsg::Event(v) => serde_json::from_value(v.clone()).ok(),
            _ => None,
        }
    }

    pub fn screen(&self) -> Option<ScreenPush> {
        match self {
            ServerMsg::Screen(v) => serde_json::from_value(v.clone()).ok(),
            _ => None,
        }
    }
}

/// Runner to shim JSON messages. Input is a binary frame with `TAG_INPUT`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToShim {
    Adopt { nonce_r: String },
    AdoptAuth { proof_r: String },
    Resize { cols: u16, rows: u16 },
    Signal { sig: i32 },
    Reap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExitInfo {
    pub code: Option<i32>,
    pub signal: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShimHello {
    pub pid: u32,
    pub child_pid: u32,
    pub started_at_ms: u64,
    #[serde(default)]
    pub exit: Option<ExitInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stat {
    pub fg_pgid: Option<i32>,
    pub fg_cpu_ns: Option<u64>,
    pub fg_waiting_tty: Option<bool>,
}

/// Shim to runner JSON messages. Output is a binary frame with `TAG_OUTPUT`,
/// the ring replay one with `TAG_REPLAY`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FromShim {
    AdoptChallenge { nonce_sh: String, proof_sh: String },
    Hello(ShimHello),
    Stat(Stat),
    Exited(ExitInfo),
    InputDropped { bytes: u64 },
    SignalAck { sig: i32, delivered: bool },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn generated_ids_are_valid_and_distinct() {
        let a = new_session_id();
        assert!(valid_session_id(&a), "{a}");
        assert_ne!(a, new_session_id());
    }

    #[test]
    fn id_validation() {
        for ok in ["s_abcdefghij", "s_2345672345"] {
            assert!(valid_session_id(ok), "{ok}");
        }
        for bad in [
            "",
            "s_",
            "s_abcdefghi",
            "s_abcdefghijk",
            "S_abcdefghij",
            "s_ABCDEFGHIJ",
            "s_abcdefgh10",
            "s_abcdefgh1j",
            "../abcdefghij",
            "s_../..//..",
            "x_abcdefghij",
            "s_abcdefghi\n",
        ] {
            assert!(!valid_session_id(bad), "{bad}");
        }
    }

    #[test]
    fn wire_shapes() {
        let v = serde_json::to_value(ClientMsg::Detach).unwrap();
        assert_eq!(v, json!({"type": "detach"}));
        let v = serde_json::to_value(ServerMsg::Response(Response {
            id: 3,
            ok: Some(json!({"a": 1})),
            err: None,
        }))
        .unwrap();
        assert_eq!(v, json!({"type": "response", "id": 3, "ok": {"a": 1}}));
        let m: ToShim = serde_json::from_value(json!({"type": "signal", "sig": 15})).unwrap();
        assert_eq!(m, ToShim::Signal { sig: 15 });
        let h = FromShim::Hello(ShimHello {
            pid: 1,
            child_pid: 2,
            started_at_ms: 3,
            exit: None,
        });
        let back: FromShim = serde_json::from_value(serde_json::to_value(&h).unwrap()).unwrap();
        assert_eq!(back, h);
        let e = ServerMsg::Event(json!({"session": "s_abcdefghij", "state": "working"}));
        let back: ServerMsg = serde_json::from_value(serde_json::to_value(&e).unwrap()).unwrap();
        assert_eq!(back, e);
    }
}
