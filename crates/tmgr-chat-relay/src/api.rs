use crate::fields::{check_device_id, check_device_kind, check_fp, check_msg_id};
use crate::{Error, Result};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Map, Value};

pub const BASE_PATH: &str = "/api/chat-relay";
pub const MAX_LABEL_CHARS: usize = 64;
pub const MAX_PUSH_TOKEN_CHARS: usize = 255;
pub const MAX_INTRO_BYTES: usize = 2048;
pub const MAX_STATEMENT_BYTES: usize = 2048;
pub const MAX_ITEMS: usize = 20;
pub const MAX_BODY_BYTES: usize = 2 << 20;
pub const MAX_REQUEST_BYTES: usize = 8 << 20;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Put,
    Delete,
}

impl Method {
    pub fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
            Method::Put => "PUT",
            Method::Delete => "DELETE",
        }
    }
}

pub const KINDS_HEADER: &str = "X-Chat-Relay-Kinds";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Auth {
    Login,
    Device,
    DeviceOrLogin,
    LoginOrOwnDevice,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub method: Method,
    pub path: String,
    pub auth: Auth,
    pub body: Option<Value>,
    pub headers: Vec<(&'static str, &'static str)>,
}

fn req(method: Method, path: &str, auth: Auth, body: Option<Value>) -> Request {
    Request {
        method,
        path: format!("{BASE_PATH}{path}"),
        auth,
        body,
        headers: Vec::new(),
    }
}

fn ensure(ok: bool) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(Error::BadFormat)
    }
}

fn check_pub(p: &str) -> Result<()> {
    let raw = tmgr_phone::b64_decode(p)?;
    ensure(raw.len() == 65 && raw[0] == 4)
}

pub fn register_device(
    kind: &str,
    label: &str,
    sign_pub_b64: &str,
    kex_pub_b64: &str,
    push_token: Option<&str>,
) -> Result<Request> {
    check_device_kind(kind)?;
    ensure(label.chars().count() <= MAX_LABEL_CHARS)?;
    check_pub(sign_pub_b64)?;
    check_pub(kex_pub_b64)?;
    let mut body = Map::new();
    body.insert("kind".into(), json!(kind));
    body.insert("label".into(), json!(label));
    body.insert("sign_pub".into(), json!(sign_pub_b64));
    body.insert("kex_pub".into(), json!(kex_pub_b64));
    if let Some(t) = push_token {
        ensure(t.chars().count() <= MAX_PUSH_TOKEN_CHARS)?;
        body.insert("push_token".into(), json!(t));
    }
    Ok(req(
        Method::Post,
        "/devices",
        Auth::Login,
        Some(Value::Object(body)),
    ))
}

pub fn list_devices() -> Request {
    let mut r = req(Method::Get, "/devices", Auth::DeviceOrLogin, None);
    r.headers.push((KINDS_HEADER, "terminal"));
    r
}

pub fn put_intro(target_device_id: &str, intro_json: &str) -> Result<Request> {
    check_device_id(target_device_id)?;
    ensure(intro_json.len() <= MAX_INTRO_BYTES)?;
    Ok(req(
        Method::Put,
        &format!("/devices/{target_device_id}/intros/self"),
        Auth::Device,
        Some(json!({ "intro": intro_json })),
    ))
}

pub fn put_push_token(push_token: Option<&str>) -> Result<Request> {
    if let Some(t) = push_token {
        ensure(t.chars().count() <= MAX_PUSH_TOKEN_CHARS)?;
    }
    Ok(req(
        Method::Put,
        "/devices/self/push-token",
        Auth::Device,
        Some(json!({ "push_token": push_token })),
    ))
}

pub fn delete_device(device_id: &str) -> Result<Request> {
    check_device_id(device_id)?;
    Ok(req(
        Method::Delete,
        &format!("/devices/{device_id}"),
        Auth::LoginOrOwnDevice,
        None,
    ))
}

pub fn post_messages(msg_id: &str, push: bool, items: &[(&str, &str)]) -> Result<Request> {
    check_msg_id(msg_id)?;
    ensure((1..=MAX_ITEMS).contains(&items.len()))?;
    let mut total = 0usize;
    let mut arr = Vec::with_capacity(items.len());
    for (i, (to, body)) in items.iter().enumerate() {
        check_device_id(to)?;
        ensure(!items[..i].iter().any(|(t, _)| t == to))?;
        ensure(body.len() <= MAX_BODY_BYTES)?;
        total += body.len();
        arr.push(json!({ "to": to, "body": body }));
    }
    ensure(total <= MAX_REQUEST_BYTES)?;
    let mut body = Map::new();
    body.insert("msg_id".into(), json!(msg_id));
    if push {
        body.insert("push".into(), json!(true));
    }
    body.insert("items".into(), Value::Array(arr));
    Ok(req(
        Method::Post,
        "/messages",
        Auth::Device,
        Some(Value::Object(body)),
    ))
}

pub fn get_messages(after: u64, limit: u32) -> Result<Request> {
    ensure((1..=200).contains(&limit))?;
    ensure(after <= i64::MAX as u64)?;
    Ok(req(
        Method::Get,
        &format!("/messages?after={after}&limit={limit}"),
        Auth::Device,
        None,
    ))
}

pub fn ack_messages(through: u64) -> Result<Request> {
    ensure(through <= i64::MAX as u64)?;
    Ok(req(
        Method::Post,
        "/messages/ack",
        Auth::Device,
        Some(json!({ "through": through })),
    ))
}

pub fn post_revocation(target_fp: &str, statement_json: &str) -> Result<Request> {
    check_fp(target_fp)?;
    ensure(statement_json.len() <= MAX_STATEMENT_BYTES)?;
    Ok(req(
        Method::Post,
        "/revocations",
        Auth::Device,
        Some(json!({ "target_fp": target_fp, "statement": statement_json })),
    ))
}

fn unwrap_data(v: Value) -> Value {
    match v {
        Value::Object(mut m) if m.len() == 1 && m.contains_key("data") => {
            m.remove("data").unwrap_or(Value::Null)
        }
        other => other,
    }
}

fn parse<T: DeserializeOwned>(body: &str) -> Result<T> {
    let v: Value = crate::fields::parse_strict(body)?;
    serde_json::from_value(unwrap_data(v)).map_err(|_| Error::BadJson)
}

#[derive(Deserialize, Clone, PartialEq)]
pub struct Registered {
    pub device_id: String,
    pub device_token: String,
}

impl std::fmt::Debug for Registered {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Registered")
            .field("device_id", &self.device_id)
            .field("device_token", &"<redacted>")
            .finish()
    }
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Device {
    pub device_id: String,
    pub kind: String,
    #[serde(default)]
    pub label: String,
    pub sign_pub: String,
    pub kex_pub: String,
    #[serde(default)]
    pub intros: Vec<String>,
    #[serde(default)]
    pub created_at: Value,
    #[serde(default)]
    pub last_seen_at: Value,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct ServerRevocation {
    pub target_fp: String,
    pub statement: String,
    #[serde(default)]
    pub created_at: Value,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Directory {
    pub devices: Vec<Device>,
    #[serde(default)]
    pub revocations: Vec<ServerRevocation>,
}

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Posted {
    pub stored: u64,
    pub duplicates: u64,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Msg {
    pub cursor: i64,
    pub msg_id: String,
    pub from: String,
    pub body: String,
    #[serde(default)]
    pub created_at: Value,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
pub struct Inbox {
    pub messages: Vec<Msg>,
    #[serde(default)]
    pub more: bool,
}

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Acked {
    pub deleted: u64,
}

#[derive(Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ApiError {
    #[serde(default)]
    pub message: String,
    pub error: String,
    #[serde(default)]
    pub unknown: Vec<String>,
}

pub fn parse_registered(body: &str) -> Result<Registered> {
    parse(body)
}

pub fn parse_directory(body: &str) -> Result<Directory> {
    parse(body)
}

pub fn parse_posted(body: &str) -> Result<Posted> {
    parse(body)
}

pub fn parse_inbox(body: &str) -> Result<Inbox> {
    let inbox: Inbox = parse(body)?;
    if inbox.messages.iter().any(|m| m.cursor <= 0) {
        return Err(Error::BadFormat);
    }
    Ok(inbox)
}

pub fn parse_acked(body: &str) -> Result<Acked> {
    parse(body)
}

pub fn parse_error(body: &str) -> Result<ApiError> {
    parse(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys::DeviceKeys;

    const D1: &str = "00000000-0000-4000-8000-000000000001";
    const D2: &str = "00000000-0000-4000-8000-000000000002";

    fn pubs() -> (String, String) {
        let k = DeviceKeys::from_secrets(&[1u8; 32], &[2u8; 32]).unwrap();
        (k.sign_pub_b64(), k.kex_pub_b64())
    }

    #[test]
    fn register_device_request() {
        let (s, k) = pubs();
        let r = register_device("mobile", "Phone", &s, &k, Some("ExponentPushToken[x]")).unwrap();
        assert_eq!(r.method, Method::Post);
        assert_eq!(r.path, "/api/chat-relay/devices");
        assert_eq!(r.auth, Auth::Login);
        let b = r.body.unwrap();
        let keys: Vec<_> = b.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["kind", "label", "sign_pub", "kex_pub", "push_token"]);
        let r = register_device("desktop", "D", &s, &k, None).unwrap();
        assert!(r.body.unwrap().get("push_token").is_none());
        assert!(register_device("laptop", "D", &s, &k, None).is_err());
        assert!(register_device("desktop", &"x".repeat(65), &s, &k, None).is_err());
        assert!(register_device("desktop", &"é".repeat(64), &s, &k, None).is_ok());
        assert!(register_device("desktop", "D", "AAAA", &k, None).is_err());
        assert!(register_device("desktop", "D", &s, &k, Some(&"t".repeat(256))).is_err());
    }

    #[test]
    fn simple_endpoints() {
        let r = list_devices();
        assert_eq!(
            (r.method, r.path.as_str()),
            (Method::Get, "/api/chat-relay/devices")
        );
        assert_eq!(r.auth, Auth::DeviceOrLogin);
        assert!(r.body.is_none());
        assert_eq!(r.headers, [("X-Chat-Relay-Kinds", "terminal")]);

        let r = put_intro(D1, "{\"v\":1}").unwrap();
        assert_eq!(r.method, Method::Put);
        assert_eq!(r.path, format!("/api/chat-relay/devices/{D1}/intros/self"));
        assert_eq!(r.body.unwrap(), json!({"intro": "{\"v\":1}"}));
        assert!(put_intro("nope", "x").is_err());
        assert!(put_intro(D1, &"x".repeat(2049)).is_err());

        let r = put_push_token(None).unwrap();
        assert_eq!(r.path, "/api/chat-relay/devices/self/push-token");
        assert_eq!(r.body.unwrap(), json!({"push_token": null}));
        assert_eq!(
            put_push_token(Some("t")).unwrap().body.unwrap(),
            json!({"push_token": "t"})
        );

        let r = delete_device(D2).unwrap();
        assert_eq!((r.method, r.auth), (Method::Delete, Auth::LoginOrOwnDevice));
        assert_eq!(r.path, format!("/api/chat-relay/devices/{D2}"));
        assert!(delete_device("../x").is_err());

        let r = ack_messages(7).unwrap();
        assert_eq!(r.path, "/api/chat-relay/messages/ack");
        assert_eq!(r.body.unwrap(), json!({"through": 7}));

        let fp = "a".repeat(64);
        let r = post_revocation(&fp, "{}").unwrap();
        assert_eq!(r.path, "/api/chat-relay/revocations");
        assert_eq!(r.body.unwrap(), json!({"target_fp": fp, "statement": "{}"}));
        assert!(post_revocation("short", "{}").is_err());
        assert!(post_revocation(&fp, &"x".repeat(2049)).is_err());
    }

    #[test]
    fn get_messages_query() {
        let r = get_messages(0, 100).unwrap();
        assert_eq!(r.path, "/api/chat-relay/messages?after=0&limit=100");
        assert_eq!(r.auth, Auth::Device);
        assert!(get_messages(0, 0).is_err());
        assert!(get_messages(0, 201).is_err());
    }

    #[test]
    fn post_messages_rules() {
        let r = post_messages("m_1", true, &[(D1, "b1"), (D2, "b2")]).unwrap();
        assert_eq!(r.path, "/api/chat-relay/messages");
        assert_eq!(
            r.body.unwrap(),
            json!({"msg_id": "m_1", "push": true, "items": [{"to": D1, "body": "b1"}, {"to": D2, "body": "b2"}]})
        );
        let r = post_messages("m_1", false, &[(D1, "b1")]).unwrap();
        assert!(r.body.unwrap().get("push").is_none());
        assert!(post_messages("m_1", false, &[]).is_err());
        assert!(post_messages("m:1", false, &[(D1, "b")]).is_err());
        assert!(post_messages("m_1", false, &[(D1, "a"), (D1, "b")]).is_err());
        assert!(post_messages("m_1", false, &[("x", "a")]).is_err());
        let ids: Vec<String> = (0..21)
            .map(|i| format!("00000000-0000-4000-8000-{i:012x}"))
            .collect();
        let many: Vec<(&str, &str)> = ids.iter().map(|i| (i.as_str(), "b")).collect();
        assert!(post_messages("m_1", false, &many[..20]).is_ok());
        assert!(post_messages("m_1", false, &many).is_err());
        let big = "x".repeat(MAX_BODY_BYTES);
        assert!(post_messages("m_1", false, &[(D1, &big)]).is_ok());
        assert!(post_messages("m_1", false, &[(D1, &format!("{big}x"))]).is_err());
        let four: Vec<(&str, &str)> = ids[..5]
            .iter()
            .map(|i| (i.as_str(), big.as_str()))
            .collect();
        assert!(post_messages("m_1", false, &four[..4]).is_ok());
        assert!(post_messages("m_1", false, &four).is_err());
    }

    #[test]
    fn parsers_accept_wrapped_and_bare() {
        for body in [
            r#"{"data":{"device_id":"d","device_token":"t"}}"#,
            r#"{"device_id":"d","device_token":"t"}"#,
        ] {
            let r = parse_registered(body).unwrap();
            assert_eq!((r.device_id.as_str(), r.device_token.as_str()), ("d", "t"));
        }
        for body in [
            r#"{"data":{"stored":2,"duplicates":1}}"#,
            r#"{"stored":2,"duplicates":1}"#,
        ] {
            assert_eq!(
                parse_posted(body).unwrap(),
                Posted {
                    stored: 2,
                    duplicates: 1
                }
            );
        }
        assert_eq!(parse_acked(r#"{"data":{"deleted":3}}"#).unwrap().deleted, 3);
    }

    #[test]
    fn registered_debug_redacts_token() {
        let r = parse_registered(r#"{"device_id":"d","device_token":"secret-token"}"#).unwrap();
        let d = format!("{r:?}");
        assert!(d.contains("<redacted>") && !d.contains("secret-token"));
    }

    #[test]
    fn directory_with_revocations() {
        let body = format!(
            r#"{{"data":{{"devices":[{{"device_id":"{D1}","kind":"mobile","label":"P","sign_pub":"s","kex_pub":"k","intros":["i1"],"created_at":"2026-01-01T00:00:00Z","last_seen_at":null}}],"revocations":[{{"target_fp":"f","statement":"st","created_at":5}}]}}}}"#
        );
        let d = parse_directory(&body).unwrap();
        assert_eq!(d.devices.len(), 1);
        assert_eq!(d.devices[0].intros, ["i1"]);
        assert_eq!(d.revocations[0].target_fp, "f");
        let old = format!(
            r#"{{"devices":[{{"device_id":"{D1}","kind":"mobile","sign_pub":"s","kex_pub":"k"}}]}}"#
        );
        let d = parse_directory(&old).unwrap();
        assert!(d.revocations.is_empty() && d.devices[0].intros.is_empty());
    }

    #[test]
    fn inbox_and_errors() {
        let body = r#"{"data":{"messages":[{"cursor":5,"msg_id":"m_1","from":"d","body":"b","created_at":"x"}],"more":true}}"#;
        let i = parse_inbox(body).unwrap();
        assert!(i.more);
        assert_eq!(i.messages[0].cursor, 5);
        assert!(parse_inbox(
            r#"{"messages":[{"cursor":0,"msg_id":"m","from":"d","body":"b"}],"more":false}"#
        )
        .is_err());
        let e =
            parse_error(r#"{"message":"m","error":"recipient_unknown","unknown":["a"]}"#).unwrap();
        assert_eq!(e.error, "recipient_unknown");
        assert_eq!(e.unknown, ["a"]);
        assert_eq!(
            parse_error(r#"{"message":"m","error":"device_limit"}"#)
                .unwrap()
                .unknown
                .len(),
            0
        );
        assert!(parse_directory("not json").is_err());
        assert!(parse_posted(r#"{"stored":1,"stored":2,"duplicates":0}"#).is_err());
    }
}
