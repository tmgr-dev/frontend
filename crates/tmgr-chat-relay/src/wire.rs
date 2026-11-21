use crate::keys::{fingerprint, DeviceKeys};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tmgr_phone::{b64_decode, h, open_ctx, seal_ctx_with, sign, verify, Entropy, Sealed};

const MAX_AGE_MS: u64 = 31 * 24 * 3600 * 1000;
const MAX_SKEW_MS: u64 = 5 * 60 * 1000;

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
    pub payload: Value,
}

#[derive(Serialize, Deserialize)]
struct Plain {
    body: String,
    sig: String,
}

pub struct Peer<'a> {
    pub device_id: &'a str,
    pub sign_pub_b64: &'a str,
    pub kex_pub_b64: &'a str,
}

pub struct Opened {
    pub from: String,
    pub kind: String,
    pub id: String,
    pub sent_at: u64,
    pub payload: Value,
}

pub fn chat_ctx(user_id: u64, from_id: &str, to_id: &str, kind: &str, msg_id: &str) -> String {
    format!("tmgr-chat:v1:{user_id}:{from_id}:{to_id}:{kind}:{msg_id}")
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
) -> String {
    format!(
        "tmgr-chat-msg:v1:{user_id}:{from_fp}:{to_fp}:{from_id}:{to_id}:{kind}:{msg_id}:{sent_at_ms}:{}",
        h(inner_json)
    )
}

pub struct SealParams<'a> {
    pub user_id: u64,
    pub from_device_id: &'a str,
    pub to: Peer<'a>,
    pub kind: &'a str,
    pub msg_id: &'a str,
    pub sent_at_ms: u64,
    pub payload: Value,
}

pub fn seal_message(sender: &DeviceKeys, p: &SealParams) -> Result<String> {
    seal_message_with(sender, p, &Entropy::random()?)
}

pub fn seal_message_with(sender: &DeviceKeys, p: &SealParams, ent: &Entropy) -> Result<String> {
    let inner = Inner {
        v: 1,
        kind: p.kind.to_string(),
        id: p.msg_id.to_string(),
        sent_at: p.sent_at_ms,
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
        ),
    )?;
    let plain = serde_json::to_vec(&Plain {
        body: inner_json,
        sig,
    })
    .map_err(|_| Error::BadJson)?;
    let ctx = chat_ctx(
        p.user_id,
        p.from_device_id,
        p.to.device_id,
        p.kind,
        p.msg_id,
    );
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
    sender_of: F,
) -> Result<Opened>
where
    F: FnOnce(&str) -> Option<Peer<'a>>,
{
    let body: Body = serde_json::from_str(body_json).map_err(|_| Error::BadJson)?;
    if body.v != 1 {
        return Err(Error::BadFormat);
    }
    if body.id != server_msg_id || body.from != server_from {
        return Err(Error::IdMismatch);
    }
    let ctx = chat_ctx(user_id, &body.from, own_device_id, &body.kind, &body.id);
    let sealed = Sealed {
        v: body.v,
        kind: body.kind.clone(),
        id: body.id.clone(),
        eph: body.eph,
        nonce: body.nonce,
        ct: body.ct,
    };
    let plain = open_ctx(me.kex_secret(), &ctx, &sealed)?;
    let plain: Plain = serde_json::from_slice(&plain).map_err(|_| Error::BadJson)?;
    let inner: Inner = serde_json::from_str(&plain.body).map_err(|_| Error::BadJson)?;
    if inner.v != 1 || inner.kind != body.kind || inner.id != body.id {
        return Err(Error::IdMismatch);
    }
    let sender = sender_of(&body.from).ok_or(Error::BadKey)?;
    let from_fp = fingerprint(sender.sign_pub_b64, sender.kex_pub_b64);
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
    );
    verify(&b64_decode(sender.sign_pub_b64)?, &signed, &plain.sig)?;
    if inner.sent_at < now_ms.saturating_sub(MAX_AGE_MS) || inner.sent_at > now_ms + MAX_SKEW_MS {
        return Err(Error::BadWindow);
    }
    Ok(Opened {
        from: body.from,
        kind: inner.kind,
        id: inner.id,
        sent_at: inner.sent_at,
        payload: inner.payload,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const NOW: u64 = 1_800_000_000_000;

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

    fn seal_to_bob(w: &World) -> String {
        let bob = pubs(&w.bob);
        seal_message_with(
            &w.alice,
            &SealParams {
                user_id: 42,
                from_device_id: "dev-a",
                to: peer("dev-b", &bob),
                kind: "chat",
                msg_id: "m_1",
                sent_at_ms: NOW,
                payload: json!({"text": "hi"}),
            },
            &ent(),
        )
        .unwrap()
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
        let a = pubs(&w.alice);
        let a_ref = &a;
        open_message(me, user, own_id, from, id, body, now, |f| {
            (f == "dev-a").then(|| peer("dev-a", a_ref))
        })
    }

    fn open_ok(w: &World, body: &str) -> Result<Opened> {
        open_as(w, &w.bob, "dev-b", 42, "dev-a", "m_1", body, NOW)
    }

    #[test]
    fn round_trip() {
        let w = world();
        let body = seal_to_bob(&w);
        let o = open_ok(&w, &body).unwrap();
        assert_eq!(o.from, "dev-a");
        assert_eq!(o.kind, "chat");
        assert_eq!(o.id, "m_1");
        assert_eq!(o.sent_at, NOW);
        assert_eq!(o.payload, json!({"text": "hi"}));
    }

    #[test]
    fn random_entropy_round_trips() {
        let w = world();
        let bob = pubs(&w.bob);
        let body = seal_message(
            &w.alice,
            &SealParams {
                user_id: 42,
                from_device_id: "dev-a",
                to: peer("dev-b", &bob),
                kind: "chat",
                msg_id: "m_1",
                sent_at_ms: NOW,
                payload: json!({}),
            },
        )
        .unwrap();
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
    fn strings_match_spec() {
        assert_eq!(
            chat_ctx(42, "a", "b", "chat", "m_1"),
            "tmgr-chat:v1:42:a:b:chat:m_1"
        );
        assert_eq!(
            signed_string(42, "ffp", "tfp", "a", "b", "chat", "m_1", 99, "{}"),
            format!("tmgr-chat-msg:v1:42:ffp:tfp:a:b:chat:m_1:99:{}", h("{}"))
        );
    }

    #[test]
    fn tamper_each_outer_field_is_rejected() {
        let w = world();
        let body = seal_to_bob(&w);
        for field in ["kind", "id", "from", "eph", "nonce", "ct"] {
            let mut v: Value = serde_json::from_str(&body).unwrap();
            let orig = v[field].as_str().unwrap().to_string();
            let mut chars: Vec<char> = orig.chars().collect();
            chars[0] = if chars[0] == 'A' { 'B' } else { 'A' };
            v[field] = Value::String(chars.into_iter().collect());
            let t = v.to_string();
            let id = if field == "id" {
                v["id"].as_str().unwrap()
            } else {
                "m_1"
            };
            let r = open_as(&w, &w.bob, "dev-b", 42, "dev-a", id, &t, NOW);
            assert!(r.is_err(), "{field} tamper accepted");
        }
    }

    #[test]
    fn server_envelope_mismatch_is_rejected() {
        let w = world();
        let body = seal_to_bob(&w);
        assert!(open_as(&w, &w.bob, "dev-b", 42, "dev-x", "m_1", &body, NOW).is_err());
        assert!(open_as(&w, &w.bob, "dev-b", 42, "dev-a", "m_2", &body, NOW).is_err());
    }

    #[test]
    fn wrong_user_or_recipient_id_is_rejected() {
        let w = world();
        let body = seal_to_bob(&w);
        assert!(open_as(&w, &w.bob, "dev-b", 43, "dev-a", "m_1", &body, NOW).is_err());
        assert!(open_as(&w, &w.bob, "dev-c", 42, "dev-a", "m_1", &body, NOW).is_err());
    }

    #[test]
    fn wrong_recipient_key_is_rejected() {
        let w = world();
        let body = seal_to_bob(&w);
        assert!(open_as(&w, &w.carol, "dev-b", 42, "dev-a", "m_1", &body, NOW).is_err());
    }

    #[test]
    fn wrong_sender_key_is_rejected() {
        let w = world();
        let body = seal_to_bob(&w);
        let c = pubs(&w.carol);
        let c_ref = &c;
        let r = open_message(&w.bob, 42, "dev-b", "dev-a", "m_1", &body, NOW, |_| {
            Some(peer("dev-a", c_ref))
        });
        assert_eq!(r.err(), Some(Error::BadSignature));
    }

    #[test]
    fn unknown_sender_is_rejected() {
        let w = world();
        let body = seal_to_bob(&w);
        let r = open_message(&w.bob, 42, "dev-b", "dev-a", "m_1", &body, NOW, |_| None);
        assert!(r.is_err());
    }

    #[test]
    fn signature_binds_sent_at_and_inner() {
        let w = world();
        let bob = pubs(&w.bob);
        let inner = Inner {
            v: 1,
            kind: "chat".into(),
            id: "m_1".into(),
            sent_at: NOW,
            payload: json!({"text": "hi"}),
        };
        let inner_json = serde_json::to_string(&inner).unwrap();
        let sig = sign(
            w.alice.sign_secret(),
            &signed_string(
                42,
                &w.alice.fingerprint(),
                &w.bob.fingerprint(),
                "dev-a",
                "dev-b",
                "chat",
                "m_1",
                NOW,
                &inner_json,
            ),
        )
        .unwrap();
        let forged_inner = inner_json.replace("hi", "yo");
        let plain = serde_json::to_vec(&Plain {
            body: forged_inner,
            sig,
        })
        .unwrap();
        let ctx = chat_ctx(42, "dev-a", "dev-b", "chat", "m_1");
        let s = seal_ctx_with(
            &b64_decode(&bob.1).unwrap(),
            &ctx,
            "chat",
            "m_1",
            &plain,
            &ent(),
        )
        .unwrap();
        let body = serde_json::to_string(&Body {
            v: 1,
            kind: s.kind,
            id: s.id,
            from: "dev-a".into(),
            eph: s.eph,
            nonce: s.nonce,
            ct: s.ct,
        })
        .unwrap();
        assert_eq!(open_ok(&w, &body).err(), Some(Error::BadSignature));
    }

    #[test]
    fn time_window_is_enforced() {
        let w = world();
        let body = seal_to_bob(&w);
        let day = 24 * 3600 * 1000;
        let r = open_as(
            &w,
            &w.bob,
            "dev-b",
            42,
            "dev-a",
            "m_1",
            &body,
            NOW + 32 * day,
        );
        assert_eq!(r.err(), Some(Error::BadWindow));
        let r = open_as(
            &w,
            &w.bob,
            "dev-b",
            42,
            "dev-a",
            "m_1",
            &body,
            NOW - 6 * 60 * 1000,
        );
        assert_eq!(r.err(), Some(Error::BadWindow));
    }
}
