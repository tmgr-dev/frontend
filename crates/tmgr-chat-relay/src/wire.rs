use crate::fields::{check_device_id, check_fp, check_kind, check_msg_id, parse_strict};
use crate::keys::{fingerprint, DeviceKeys};
use crate::trust::Trusted;
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tmgr_phone::{b64_decode, h, hex, open_ctx, seal_ctx_with, sign, verify_der, Entropy, Sealed};

const MAX_AGE_MS: u64 = 31 * 24 * 3600 * 1000;
const MAX_SKEW_MS: u64 = 5 * 60 * 1000;
const PAD_BUCKETS: [usize; 6] = [1 << 10, 4 << 10, 16 << 10, 64 << 10, 256 << 10, 1 << 20];

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Body {
    pub v: u32,
    pub kind: String,
    pub id: String,
    pub from: String,
    pub eph: String,
    pub nonce: String,
    pub ct: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Inner {
    pub v: u32,
    pub kind: String,
    pub id: String,
    pub sent_at: u64,
    pub seq: u64,
    pub payload: Value,
}

#[derive(Serialize, Deserialize)]
struct Plain {
    body: String,
    sig: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pad: Option<String>,
}

pub struct Peer<'a> {
    pub device_id: &'a str,
    pub sign_pub_b64: &'a str,
    pub kex_pub_b64: &'a str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Opened {
    pub from: String,
    pub from_fp: String,
    pub kind: String,
    pub id: String,
    pub seq: u64,
    pub sent_at: u64,
    pub payload: Value,
    pub trusted: bool,
}

pub fn chat_ctx(
    user_id: u64,
    from_id: &str,
    to_id: &str,
    kind: &str,
    msg_id: &str,
) -> Result<String> {
    check_device_id(from_id)?;
    check_device_id(to_id)?;
    check_kind(kind)?;
    check_msg_id(msg_id)?;
    Ok(format!(
        "tmgr-chat:v1:{user_id}:{from_id}:{to_id}:{kind}:{msg_id}"
    ))
}

#[allow(clippy::too_many_arguments)]
pub fn signed_string(
    user_id: u64,
    from_fp: &str,
    to_fp: &str,
    from_id: &str,
    to_id: &str,
    kind: &str,
    msg_id: &str,
    sent_at_ms: u64,
    inner_json: &str,
) -> Result<String> {
    check_fp(from_fp)?;
    check_fp(to_fp)?;
    check_device_id(from_id)?;
    check_device_id(to_id)?;
    check_kind(kind)?;
    check_msg_id(msg_id)?;
    Ok(format!(
        "tmgr-chat-msg:v1:{user_id}:{from_fp}:{to_fp}:{from_id}:{to_id}:{kind}:{msg_id}:{sent_at_ms}:{}",
        h(inner_json)
    ))
}

pub fn msg_id_from(kind: &str, random: &[u8; 16]) -> Result<String> {
    let prefix = match kind {
        "blob" => "b",
        "receipt" => "r",
        "join" => "j",
        "join_ok" => "o",
        "revoke" => "x",
        "spawnkey" => "k",
        "mgr_req" => "g",
        "mgr_res" => "h",
        _ => return Err(Error::UnexpectedKind),
    };
    Ok(format!("{prefix}_{}", hex(random)))
}

pub fn random_msg_id(kind: &str) -> Result<String> {
    let mut r = [0u8; 16];
    getrandom::fill(&mut r).map_err(|_| Error::Random)?;
    msg_id_from(kind, &r)
}

pub struct SealParams<'a> {
    pub user_id: u64,
    pub from_device_id: &'a str,
    pub to: Peer<'a>,
    pub kind: &'a str,
    pub msg_id: &'a str,
    pub sent_at_ms: u64,
    pub seq: u64,
    pub payload: Value,
}

fn build_plain(body: String, sig: String, pad: bool) -> Result<Vec<u8>> {
    let ser = |pad: Option<String>| {
        serde_json::to_vec(&Plain {
            body: body.clone(),
            sig: sig.clone(),
            pad,
        })
        .map_err(|_| Error::BadJson)
    };
    let bare = ser(None)?;
    if !pad {
        return Ok(bare);
    }
    let with_empty_pad = ser(Some(String::new()))?.len();
    match PAD_BUCKETS.iter().find(|b| **b >= with_empty_pad) {
        Some(bucket) => ser(Some(" ".repeat(bucket - with_empty_pad))),
        None => Ok(bare),
    }
}

pub fn seal_message(sender: &DeviceKeys, p: &SealParams) -> Result<String> {
    seal_message_with(sender, p, &Entropy::random()?)
}

pub fn seal_message_with(sender: &DeviceKeys, p: &SealParams, ent: &Entropy) -> Result<String> {
    seal_inner(sender, p, ent, true)
}

pub fn seal_message_unpadded_with(
    sender: &DeviceKeys,
    p: &SealParams,
    ent: &Entropy,
) -> Result<String> {
    seal_inner(sender, p, ent, false)
}

fn seal_inner(sender: &DeviceKeys, p: &SealParams, ent: &Entropy, pad: bool) -> Result<String> {
    if p.seq == 0 {
        return Err(Error::BadFormat);
    }
    let ctx = chat_ctx(
        p.user_id,
        p.from_device_id,
        p.to.device_id,
        p.kind,
        p.msg_id,
    )?;
    let inner = Inner {
        v: 1,
        kind: p.kind.to_string(),
        id: p.msg_id.to_string(),
        sent_at: p.sent_at_ms,
        seq: p.seq,
        payload: p.payload.clone(),
    };
    let inner_json = serde_json::to_string(&inner).map_err(|_| Error::BadJson)?;
    let from_fp = sender.fingerprint();
    let to_fp = fingerprint(p.to.sign_pub_b64, p.to.kex_pub_b64);
    let sig = sign(
        sender.sign_secret(),
        &signed_string(
            p.user_id,
            &from_fp,
            &to_fp,
            p.from_device_id,
            p.to.device_id,
            p.kind,
            p.msg_id,
            p.sent_at_ms,
            &inner_json,
        )?,
    )?;
    let plain = build_plain(inner_json, sig, pad)?;
    let kex_pub = b64_decode(p.to.kex_pub_b64)?;
    let s = seal_ctx_with(&kex_pub, &ctx, p.kind, p.msg_id, &plain, ent)?;
    serde_json::to_string(&Body {
        v: 1,
        kind: s.kind,
        id: s.id,
        from: p.from_device_id.to_string(),
        eph: s.eph,
        nonce: s.nonce,
        ct: s.ct,
    })
    .map_err(|_| Error::BadJson)
}

#[allow(clippy::too_many_arguments)]
pub fn open_message<'a, F>(
    me: &DeviceKeys,
    user_id: u64,
    own_device_id: &str,
    server_from: &str,
    server_msg_id: &str,
    body_json: &str,
    now_ms: u64,
    trusted: &[Trusted],
    join_sender: F,
) -> Result<Opened>
where
    F: FnOnce(&str) -> Option<Peer<'a>>,
{
    let body: Body = parse_strict(body_json)?;
    if body.v != 1 {
        return Err(Error::BadFormat);
    }
    let ctx = chat_ctx(user_id, &body.from, own_device_id, &body.kind, &body.id)?;
    if body.id != server_msg_id || body.from != server_from {
        return Err(Error::IdMismatch);
    }
    let known = trusted.iter().find(|t| t.device_id == body.from);
    if known.is_none() && body.kind != "join" {
        return Err(Error::BadKey);
    }
    let sealed = Sealed {
        v: body.v,
        kind: body.kind.clone(),
        id: body.id.clone(),
        eph: body.eph,
        nonce: body.nonce,
        ct: body.ct,
    };
    let plain = open_ctx(me.kex_secret(), &ctx, &sealed)?;
    let plain: Plain = parse_strict(std::str::from_utf8(&plain).map_err(|_| Error::BadJson)?)?;
    let inner: Inner = parse_strict(&plain.body)?;
    if inner.v != 1 || inner.kind != body.kind || inner.id != body.id || inner.seq == 0 {
        return Err(Error::IdMismatch);
    }
    let (sender_sign, sender_kex, is_trusted) = match known {
        Some(t) => (t.sign_pub.clone(), t.kex_pub.clone(), true),
        None => {
            let p = join_sender(&body.from).ok_or(Error::BadKey)?;
            if p.device_id != body.from {
                return Err(Error::BadKey);
            }
            (p.sign_pub_b64.to_string(), p.kex_pub_b64.to_string(), false)
        }
    };
    let from_fp = fingerprint(&sender_sign, &sender_kex);
    let signed = signed_string(
        user_id,
        &from_fp,
        &me.fingerprint(),
        &body.from,
        own_device_id,
        &body.kind,
        &body.id,
        inner.sent_at,
        &plain.body,
    )?;
    verify_der(&b64_decode(&sender_sign)?, &signed, &plain.sig)?;
    if inner.sent_at < now_ms.saturating_sub(MAX_AGE_MS) || inner.sent_at > now_ms + MAX_SKEW_MS {
        return Err(Error::BadWindow);
    }
    Ok(Opened {
        from: body.from,
        from_fp,
        kind: inner.kind,
        id: inner.id,
        seq: inner.seq,
        sent_at: inner.sent_at,
        payload: inner.payload,
        trusted: is_trusted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: u64 = 1_800_000_000_000;
    const A: &str = "aaaaaaaa-0000-4000-8000-000000000001";
    const B: &str = "bbbbbbbb-0000-4000-8000-000000000002";
    const C: &str = "cccccccc-0000-4000-8000-000000000003";
    const INNER: &str =
        r#"{"v":1,"kind":"chat","id":"m_1","sent_at":1800000000000,"seq":1,"payload":{}}"#;

    struct World {
        alice: DeviceKeys,
        bob: DeviceKeys,
        carol: DeviceKeys,
    }

    fn world() -> World {
        let k = |a: u8, b: u8| DeviceKeys::from_secrets(&[a; 32], &[b; 32]).unwrap();
        World {
            alice: k(1, 2),
            bob: k(3, 4),
            carol: k(5, 6),
        }
    }

    fn ent() -> Entropy {
        Entropy {
            eph_secret: [7u8; 32],
            nonce: [8u8; 12],
        }
    }

    struct Pubs(String, String);

    fn pubs(k: &DeviceKeys) -> Pubs {
        Pubs(k.sign_pub_b64(), k.kex_pub_b64())
    }

    fn peer<'a>(id: &'a str, p: &'a Pubs) -> Peer<'a> {
        Peer {
            device_id: id,
            sign_pub_b64: &p.0,
            kex_pub_b64: &p.1,
        }
    }

    fn params(bob: &Pubs, payload: Value) -> SealParams<'_> {
        SealParams {
            user_id: 42,
            from_device_id: A,
            to: peer(B, bob),
            kind: "chat",
            msg_id: "m_1",
            sent_at_ms: NOW,
            seq: 1,
            payload,
        }
    }

    fn seal_to_bob(w: &World) -> String {
        let bob = pubs(&w.bob);
        seal_message_with(&w.alice, &params(&bob, json!({"text": "hi"})), &ent()).unwrap()
    }

    #[allow(clippy::too_many_arguments)]
    fn open_as(
        w: &World,
        me: &DeviceKeys,
        own_id: &str,
        user: u64,
        from: &str,
        id: &str,
        body: &str,
        now: u64,
    ) -> Result<Opened> {
        let t = [trusted_alice(w)];
        open_message(me, user, own_id, from, id, body, now, &t, |_| None)
    }

    fn trusted_alice(w: &World) -> Trusted {
        Trusted::new(A, &w.alice.sign_pub_b64(), &w.alice.kex_pub_b64()).unwrap()
    }

    fn open_ok(w: &World, body: &str) -> Result<Opened> {
        open_as(w, &w.bob, B, 42, A, "m_1", body, NOW)
    }

    fn seal_raw_plain(w: &World, plain: &[u8]) -> String {
        let bob = pubs(&w.bob);
        let ctx = chat_ctx(42, A, B, "chat", "m_1").unwrap();
        let s = seal_ctx_with(
            &b64_decode(&bob.1).unwrap(),
            &ctx,
            "chat",
            "m_1",
            plain,
            &ent(),
        )
        .unwrap();
        serde_json::to_string(&Body {
            v: 1,
            kind: s.kind,
            id: s.id,
            from: A.into(),
            eph: s.eph,
            nonce: s.nonce,
            ct: s.ct,
        })
        .unwrap()
    }

    fn sig_for(w: &World, inner_json: &str) -> String {
        sign(
            w.alice.sign_secret(),
            &signed_string(
                42,
                &w.alice.fingerprint(),
                &w.bob.fingerprint(),
                A,
                B,
                "chat",
                "m_1",
                NOW,
                inner_json,
            )
            .unwrap(),
        )
        .unwrap()
    }

    fn plain_bytes(body: &str, sig: &str) -> Vec<u8> {
        serde_json::to_vec(&Plain {
            body: body.to_string(),
            sig: sig.to_string(),
            pad: None,
        })
        .unwrap()
    }

    fn open_inner(w: &World, inner: &str) -> Result<Opened> {
        let sig = sig_for(w, inner);
        open_ok(w, &seal_raw_plain(w, &plain_bytes(inner, &sig)))
    }

    #[test]
    fn round_trip_returns_verified_sender() {
        let w = world();
        let o = open_ok(&w, &seal_to_bob(&w)).unwrap();
        assert_eq!(o.from, A);
        assert_eq!(o.from_fp, w.alice.fingerprint());
        assert_eq!(o.kind, "chat");
        assert_eq!(o.id, "m_1");
        assert_eq!(o.seq, 1);
        assert_eq!(o.sent_at, NOW);
        assert!(o.trusted);
        assert_eq!(o.payload, json!({"text": "hi"}));
    }

    #[test]
    fn random_entropy_round_trips() {
        let w = world();
        let bob = pubs(&w.bob);
        let body = seal_message(&w.alice, &params(&bob, json!({}))).unwrap();
        open_ok(&w, &body).unwrap();
    }

    #[test]
    fn body_has_spec_fields() {
        let w = world();
        let v: Value = serde_json::from_str(&seal_to_bob(&w)).unwrap();
        let keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["v", "kind", "id", "from", "eph", "nonce", "ct"]);
        assert_eq!(v["v"], 1);
    }

    #[test]
    fn inner_field_order_is_fixed() {
        let inner = Inner {
            v: 1,
            kind: "chat".into(),
            id: "m_1".into(),
            sent_at: NOW,
            seq: 1,
            payload: json!({}),
        };
        assert_eq!(serde_json::to_string(&inner).unwrap(), INNER);
    }

    #[test]
    fn strings_match_spec() {
        assert_eq!(
            chat_ctx(42, A, B, "chat", "m_1").unwrap(),
            format!("tmgr-chat:v1:42:{A}:{B}:chat:m_1")
        );
        let fp = "a".repeat(64);
        let tp = "b".repeat(64);
        assert_eq!(
            signed_string(42, &fp, &tp, A, B, "chat", "m_1", 99, "{}").unwrap(),
            format!(
                "tmgr-chat-msg:v1:42:{fp}:{tp}:{A}:{B}:chat:m_1:99:{}",
                h("{}")
            )
        );
    }

    #[test]
    fn invalid_fields_are_rejected_not_escaped() {
        assert!(chat_ctx(1, "dev-a", B, "chat", "m_1").is_err());
        assert!(chat_ctx(1, A, B, "Chat", "m_1").is_err());
        assert!(chat_ctx(1, A, B, "chat", "m:1").is_err());
        assert!(chat_ctx(1, &A.to_uppercase(), B, "chat", "m_1").is_err());
        let fp = "a".repeat(64);
        assert!(signed_string(1, "short", &fp, A, B, "chat", "m_1", 1, "{}").is_err());
        let w = world();
        let bob = pubs(&w.bob);
        let mut p = params(&bob, json!({}));
        p.msg_id = "m:1";
        assert!(seal_message_with(&w.alice, &p, &ent()).is_err());
        let mut p = params(&bob, json!({}));
        p.kind = "a-b";
        assert!(seal_message_with(&w.alice, &p, &ent()).is_err());
        let mut p = params(&bob, json!({}));
        p.seq = 0;
        assert!(seal_message_with(&w.alice, &p, &ent()).is_err());
        let mut p = params(&bob, json!({}));
        p.from_device_id = "x";
        assert!(seal_message_with(&w.alice, &p, &ent()).is_err());
    }

    #[test]
    fn tamper_each_outer_field_is_rejected() {
        let w = world();
        let body = seal_to_bob(&w);
        for field in ["kind", "id", "from", "eph", "nonce", "ct"] {
            let mut v: Value = serde_json::from_str(&body).unwrap();
            let orig = v[field].as_str().unwrap().to_string();
            let mut chars: Vec<char> = orig.chars().collect();
            chars[0] = if chars[0] == 'a' { 'b' } else { 'a' };
            v[field] = Value::String(chars.into_iter().collect());
            let t = v.to_string();
            let id = if field == "id" {
                v["id"].as_str().unwrap()
            } else {
                "m_1"
            };
            let from = if field == "from" {
                v["from"].as_str().unwrap()
            } else {
                A
            };
            let r = open_as(&w, &w.bob, B, 42, from, id, &t, NOW);
            assert!(r.is_err(), "{field} tamper accepted");
        }
    }

    #[test]
    fn server_envelope_mismatch_is_rejected() {
        let w = world();
        let body = seal_to_bob(&w);
        assert_eq!(
            open_as(&w, &w.bob, B, 42, C, "m_1", &body, NOW).err(),
            Some(Error::IdMismatch)
        );
        assert_eq!(
            open_as(&w, &w.bob, B, 42, A, "m_2", &body, NOW).err(),
            Some(Error::IdMismatch)
        );
    }

    #[test]
    fn wrong_user_or_recipient_id_is_rejected() {
        let w = world();
        let body = seal_to_bob(&w);
        assert!(open_as(&w, &w.bob, B, 43, A, "m_1", &body, NOW).is_err());
        assert!(open_as(&w, &w.bob, C, 42, A, "m_1", &body, NOW).is_err());
    }

    #[test]
    fn wrong_recipient_key_is_rejected() {
        let w = world();
        let body = seal_to_bob(&w);
        assert!(open_as(&w, &w.carol, B, 42, A, "m_1", &body, NOW).is_err());
    }

    #[test]
    fn wrong_sender_key_is_rejected() {
        let w = world();
        let body = seal_to_bob(&w);
        let c = Trusted::new(A, &w.carol.sign_pub_b64(), &w.carol.kex_pub_b64()).unwrap();
        let r = open_message(&w.bob, 42, B, A, "m_1", &body, NOW, &[c], |_| None);
        assert_eq!(r.err(), Some(Error::BadSignature));
    }

    fn seal_kind(w: &World, kind: &str, id: &str) -> String {
        let bob = pubs(&w.bob);
        let mut p = params(&bob, json!({}));
        p.kind = kind;
        p.msg_id = id;
        seal_message_with(&w.alice, &p, &ent()).unwrap()
    }

    #[test]
    fn untrusted_sender_is_rejected_except_for_join() {
        let w = world();
        let body = seal_to_bob(&w);
        let dir_alice = pubs(&w.alice);
        let r = open_message(&w.bob, 42, B, A, "m_1", &body, NOW, &[], |_| {
            Some(peer(A, &dir_alice))
        });
        assert_eq!(r.err(), Some(Error::BadKey));

        let join = seal_kind(&w, "join", "j_1");
        let o = open_message(&w.bob, 42, B, A, "j_1", &join, NOW, &[], |_| {
            Some(peer(A, &dir_alice))
        })
        .unwrap();
        assert!(!o.trusted);
        assert_eq!(o.from_fp, w.alice.fingerprint());
        let r = open_message(&w.bob, 42, B, A, "j_1", &join, NOW, &[], |_| None);
        assert_eq!(r.err(), Some(Error::BadKey));
        let r = open_message(&w.bob, 42, B, A, "j_1", &join, NOW, &[], |_| {
            Some(peer(C, &dir_alice))
        });
        assert_eq!(r.err(), Some(Error::BadKey));
    }

    #[test]
    fn trusted_entry_keys_win_over_directory_for_join() {
        let w = world();
        let join = seal_kind(&w, "join", "j_1");
        let carol_dir = pubs(&w.carol);
        let t = [trusted_alice(&w)];
        let o = open_message(&w.bob, 42, B, A, "j_1", &join, NOW, &t, |_| {
            Some(peer(A, &carol_dir))
        })
        .unwrap();
        assert!(o.trusted);
    }

    #[test]
    fn unknown_json_fields_are_ignored() {
        let w = world();
        let inner = r#"{"v":1,"kind":"chat","id":"m_1","sent_at":1800000000000,"seq":1,"payload":{},"future":[1,{"x":2}]}"#;
        let sig = sig_for(&w, inner);
        let plain = format!(
            "{{\"body\":{},\"sig\":{sig:?},\"future\":true}}",
            serde_json::to_string(inner).unwrap(),
        );
        let body = seal_raw_plain(&w, plain.as_bytes());
        let body = body.replacen("{\"v\":1,", "{\"v\":1,\"future\":0,", 1);
        assert!(open_ok(&w, &body).is_ok());
    }

    #[test]
    fn signature_binds_inner_and_seq() {
        let w = world();
        let sig = sig_for(&w, INNER);
        for forged in [
            INNER.replace("\"payload\":{}", "\"payload\":{\"a\":1}"),
            INNER.replace("\"seq\":1", "\"seq\":2"),
        ] {
            let body = seal_raw_plain(&w, &plain_bytes(&forged, &sig));
            assert_eq!(open_ok(&w, &body).err(), Some(Error::BadSignature));
        }
        open_inner(&w, INNER).unwrap();
    }

    #[test]
    fn inner_without_seq_or_zero_seq_is_rejected() {
        let w = world();
        for inner in [
            r#"{"v":1,"kind":"chat","id":"m_1","sent_at":1800000000000,"payload":{}}"#,
            r#"{"v":1,"kind":"chat","id":"m_1","sent_at":1800000000000,"seq":0,"payload":{}}"#,
        ] {
            assert!(open_inner(&w, inner).is_err());
        }
    }

    #[test]
    fn duplicate_keys_in_body_plaintext_and_inner_are_rejected() {
        let w = world();
        let good = seal_to_bob(&w);
        let dup_body = good.replacen("{\"v\":1,", "{\"v\":1,\"v\":1,", 1);
        assert_eq!(
            open_ok(&w, &dup_body).err(),
            Some(Error::BadJson),
            "outer body"
        );

        let sig = sig_for(&w, INNER);
        let plain = format!(
            "{{\"body\":{},\"sig\":{sig:?},\"sig\":{sig:?}}}",
            serde_json::to_string(INNER).unwrap(),
        );
        assert_eq!(
            open_ok(&w, &seal_raw_plain(&w, plain.as_bytes())).err(),
            Some(Error::BadJson),
            "plaintext"
        );

        let dup_inner = r#"{"v":1,"kind":"chat","id":"m_1","sent_at":1800000000000,"seq":1,"seq":1,"payload":{}}"#;
        assert_eq!(
            open_inner(&w, dup_inner).err(),
            Some(Error::BadJson),
            "inner"
        );

        let dup_payload = r#"{"v":1,"kind":"chat","id":"m_1","sent_at":1800000000000,"seq":1,"payload":{"a":1,"a":2}}"#;
        assert_eq!(
            open_inner(&w, dup_payload).err(),
            Some(Error::BadJson),
            "nested payload"
        );
    }

    #[test]
    fn base64_is_strict() {
        let w = world();
        let body = seal_to_bob(&w);
        let v: Value = serde_json::from_str(&body).unwrap();
        for field in ["eph", "nonce", "ct"] {
            let s = v[field].as_str().unwrap().to_string();
            let trimmed = s.trim_end_matches('=').to_string();
            if trimmed != s {
                let mut t = v.clone();
                t[field] = Value::String(trimmed);
                assert!(
                    open_as(&w, &w.bob, B, 42, A, "m_1", &t.to_string(), NOW).is_err(),
                    "{field} unpadded accepted"
                );
            }
            let mut t = v.clone();
            t[field] = Value::String(format!("{s}\n"));
            assert!(
                open_as(&w, &w.bob, B, 42, A, "m_1", &t.to_string(), NOW).is_err(),
                "{field} whitespace accepted"
            );
        }
        assert!(b64_decode("QQ").is_err());
        assert!(b64_decode("QR==").is_err());
        assert!(b64_decode("QQ==").is_ok());
    }

    #[test]
    fn padding_rounds_plaintext_to_buckets() {
        let w = world();
        let bob = pubs(&w.bob);
        let pt_len = |payload: Value| {
            let body = seal_message_with(&w.alice, &params(&bob, payload), &ent()).unwrap();
            let b: Body = serde_json::from_str(&body).unwrap();
            b64_decode(&b.ct).unwrap().len() - 16
        };
        assert_eq!(pt_len(json!({"text": "hi"})), 1024);
        assert_eq!(pt_len(json!({"text": "x".repeat(300)})), 1024);
        assert_eq!(pt_len(json!({"text": "x".repeat(1000)})), 4096);
        assert_eq!(pt_len(json!({"text": "x".repeat(5000)})), 16 << 10);
        assert_eq!(pt_len(json!({"text": "x".repeat(20000)})), 64 << 10);
        assert_eq!(pt_len(json!({"text": "x".repeat(100_000)})), 256 << 10);
        assert_eq!(pt_len(json!({"text": "x".repeat(400_000)})), 1 << 20);
        let big = json!({"text": "x".repeat(1_100_000)});
        let unpadded =
            seal_message_unpadded_with(&w.alice, &params(&bob, big.clone()), &ent()).unwrap();
        let padded = seal_message_with(&w.alice, &params(&bob, big), &ent()).unwrap();
        assert_eq!(padded, unpadded);
    }

    #[test]
    fn padded_and_unpadded_both_open_and_pad_is_ignored() {
        let w = world();
        let bob = pubs(&w.bob);
        let p = params(&bob, json!({"text": "hi"}));
        let unpadded = seal_message_unpadded_with(&w.alice, &p, &ent()).unwrap();
        let padded = seal_message_with(&w.alice, &p, &ent()).unwrap();
        assert_ne!(padded, unpadded);
        assert_eq!(
            open_ok(&w, &padded).unwrap(),
            open_ok(&w, &unpadded).unwrap()
        );
    }

    #[test]
    fn non_string_pad_is_rejected() {
        let w = world();
        let sig = sig_for(&w, INNER);
        let plain = format!(
            "{{\"body\":{},\"sig\":{sig:?},\"pad\":5}}",
            serde_json::to_string(INNER).unwrap(),
        );
        assert!(open_ok(&w, &seal_raw_plain(&w, plain.as_bytes())).is_err());
    }

    #[test]
    fn time_window_is_enforced() {
        let w = world();
        let body = seal_to_bob(&w);
        let day = 24 * 3600 * 1000;
        let r = open_as(&w, &w.bob, B, 42, A, "m_1", &body, NOW + 32 * day);
        assert_eq!(r.err(), Some(Error::BadWindow));
        let r = open_as(&w, &w.bob, B, 42, A, "m_1", &body, NOW - 6 * 60 * 1000);
        assert_eq!(r.err(), Some(Error::BadWindow));
    }

    #[test]
    fn message_ids_per_kind() {
        let r = [0xab; 16];
        let hexs = "ab".repeat(16);
        for (kind, p) in [
            ("blob", "b"),
            ("receipt", "r"),
            ("join", "j"),
            ("join_ok", "o"),
            ("revoke", "x"),
            ("spawnkey", "k"),
            ("mgr_req", "g"),
            ("mgr_res", "h"),
        ] {
            assert_eq!(msg_id_from(kind, &r).unwrap(), format!("{p}_{hexs}"));
            let id = random_msg_id(kind).unwrap();
            assert_eq!(id.len(), 34);
            assert!(id.starts_with(&format!("{p}_")));
            check_msg_id(&id).unwrap();
        }
        assert!(msg_id_from("chat", &r).is_err());
        assert!(random_msg_id("nope").is_err());
        assert_ne!(
            random_msg_id("blob").unwrap(),
            random_msg_id("blob").unwrap()
        );
    }
}
