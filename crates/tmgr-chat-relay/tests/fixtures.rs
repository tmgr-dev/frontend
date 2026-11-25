use std::collections::{BTreeMap, BTreeSet};

use p256::ecdsa::Signature;
use serde_json::{json, Value};
use tmgr_chat_relay::keys::DeviceKeys;
use tmgr_chat_relay::trust::{
    decode_join_code, encode_join_code, join_mac, restrict_to_before, sign_intro, sign_revoke,
    sign_spawnkey, trusted_set, trusted_set_by_kind, verify_intro, verify_join_mac, verify_revoke,
    verify_spawnkey, DirEntry, IntroStatement, JoinCode, JoinMacInput, KeyPurpose, KindCtx,
    RevokeStatement, SpawnStatement, Trusted,
};
use tmgr_chat_relay::wire::{
    chat_ctx, msg_id_from, open_message, seal_message_unpadded_with, seal_message_with,
    signed_string, Body, Inner, Peer, SealParams,
};
use tmgr_chat_relay::Error;
use tmgr_phone::{
    b64, b64_decode, h, hex, open_ctx, public_key, seal_ctx_with, sign, Entropy, Sealed,
};

const USER: u64 = 42;
const NOW: u64 = 1_800_000_000_000;
const SENT_AT: u64 = 1_799_999_990_000;
const MAX_AGE_MS: u64 = 31 * 24 * 3600 * 1000;
const NEG_MSG_ID: &str = "m_00000000000000000000000000000002";
const IDS: [&str; 5] = [
    "00000000-0000-4000-8000-00000000000a",
    "00000000-0000-4000-8000-00000000000b",
    "00000000-0000-4000-8000-00000000000c",
    "00000000-0000-4000-8000-00000000000d",
    "00000000-0000-4000-8000-00000000000e",
];

fn dir_path(name: &str) -> String {
    format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"))
}

fn keys(n: u8) -> DeviceKeys {
    DeviceKeys::from_secrets(&[n; 32], &[n + 100; 32]).unwrap()
}

fn key_json(k: &DeviceKeys) -> Value {
    json!({
        "sign_secret_hex": hex(k.sign_secret()),
        "kex_secret_hex": hex(k.kex_secret()),
        "sign_pub": k.sign_pub_b64(),
        "kex_pub": k.kex_pub_b64(),
        "fp": k.fingerprint(),
    })
}

fn pin(id: &str, k: &DeviceKeys) -> Trusted {
    Trusted::new(id, &k.sign_pub_b64(), &k.kex_pub_b64()).unwrap()
}

fn text(v: &Value) -> String {
    let mut t = serde_json::to_string_pretty(v).unwrap();
    t.push('\n');
    t
}

fn message_fixture(description: &str, kind: &str, msg_id: &str, seq: u64, payload: Value) -> Value {
    let sender = keys(1);
    let recipient = keys(2);
    let ent = Entropy {
        eph_secret: [7u8; 32],
        nonce: [8u8; 12],
    };
    let (sp, kp) = (recipient.sign_pub_b64(), recipient.kex_pub_b64());
    let params = SealParams {
        user_id: USER,
        from_device_id: IDS[0],
        to: Peer {
            device_id: IDS[1],
            sign_pub_b64: &sp,
            kex_pub_b64: &kp,
        },
        kind,
        msg_id,
        sent_at_ms: SENT_AT,
        seq,
        payload: payload.clone(),
    };
    let body = seal_message_with(&sender, &params, &ent).unwrap();
    let body_unpadded = seal_message_unpadded_with(&sender, &params, &ent).unwrap();
    let inner = Inner {
        v: 1,
        kind: kind.into(),
        id: msg_id.into(),
        sent_at: SENT_AT,
        seq,
        payload,
    };
    let inner_json = serde_json::to_string(&inner).unwrap();
    let ctx = chat_ctx(USER, IDS[0], IDS[1], kind, msg_id).unwrap();
    let signed = signed_string(
        USER,
        &sender.fingerprint(),
        &recipient.fingerprint(),
        IDS[0],
        IDS[1],
        kind,
        msg_id,
        SENT_AT,
        &inner_json,
    )
    .unwrap();
    let plaintext = |b: &str| {
        let parsed: Body = serde_json::from_str(b).unwrap();
        let sealed = Sealed {
            v: parsed.v,
            kind: parsed.kind,
            id: parsed.id,
            eph: parsed.eph,
            nonce: parsed.nonce,
            ct: parsed.ct,
        };
        String::from_utf8(open_ctx(recipient.kex_secret(), &ctx, &sealed).unwrap()).unwrap()
    };
    let plain = plaintext(&body);
    let plain_unpadded = plaintext(&body_unpadded);
    let sig = serde_json::from_str::<Value>(&plain).unwrap()["sig"]
        .as_str()
        .unwrap()
        .to_string();
    let opened = open_message(
        &recipient,
        USER,
        IDS[1],
        IDS[0],
        msg_id,
        &body,
        NOW,
        &[pin(IDS[0], &sender)],
        |_| None,
    )
    .unwrap();
    json!({
        "description": description,
        "inputs": {
            "user_id": USER,
            "now_ms": NOW,
            "sender": {"device_id": IDS[0], "keys": key_json(&sender)},
            "recipient": {"device_id": IDS[1], "keys": key_json(&recipient)},
            "kind": kind,
            "msg_id": msg_id,
            "sent_at_ms": SENT_AT,
            "seq": seq,
            "payload": inner.payload,
            "entropy": {"eph_secret_hex": hex(&ent.eph_secret), "nonce_hex": hex(&ent.nonce)},
        },
        "outputs": {
            "ctx": ctx,
            "inner": inner_json,
            "inner_sha256_hex": h(&inner_json),
            "signed_string": signed,
            "sig": sig,
            "plaintext": plain,
            "plaintext_len": plain.len(),
            "body": body,
            "plaintext_unpadded": plain_unpadded,
            "body_unpadded": body_unpadded,
            "open": {
                "from": opened.from,
                "from_fp": opened.from_fp,
                "kind": opened.kind,
                "id": opened.id,
                "seq": opened.seq,
                "sent_at": opened.sent_at,
                "payload": opened.payload,
                "trusted": opened.trusted,
            },
        },
    })
}

fn chat_fixture() -> Value {
    message_fixture(
        "chat message sealed from sender to recipient with fixed keys and entropy; padded body and unpadded body",
        "chat",
        "m_00000000000000000000000000000001",
        3,
        json!({"text": "hello", "n": 1}),
    )
}

fn blob_fixture() -> Value {
    let data = b"fixture blob bytes";
    let id = msg_id_from("blob", &[0x11; 16]).unwrap();
    message_fixture(
        "blob message; msg_id is the blob prefix plus 32 hex of fixed entropy",
        "blob",
        &id,
        1,
        json!({"sha256": h(std::str::from_utf8(data).unwrap()), "mime": "text/plain", "data": b64(data)}),
    )
}

fn intro_statement(
    signer: &DeviceKeys,
    target: &DeviceKeys,
    target_id: &str,
    user: u64,
) -> IntroStatement {
    IntroStatement {
        version: 1,
        user_id: user,
        introducer_fp: signer.fingerprint(),
        target_id: target_id.into(),
        target_fp: target.fingerprint(),
        target_kind: "mobile".into(),
        issued_at_ms: NOW,
    }
}

fn intro_fixture() -> Value {
    let (p, n) = (keys(1), keys(2));
    let st = intro_statement(&p, &n, IDS[1], USER);
    let statement = st.to_statement().unwrap();
    let json_str = sign_intro(&p, IDS[0], &st).unwrap();
    verify_intro(&json_str, USER, IDS[0], &p.sign_pub_b64(), &p.kex_pub_b64()).unwrap();
    let sig = serde_json::from_str::<Value>(&json_str).unwrap()["sig"].clone();
    json!({
        "description": "intro signed by P for N; uploaded as the intros string",
        "inputs": {
            "user_id": USER,
            "introducer": {"device_id": IDS[0], "keys": key_json(&p)},
            "target": {"device_id": IDS[1], "kind": "mobile", "keys": key_json(&n)},
            "issued_at_ms": NOW,
        },
        "outputs": {"statement": statement, "sig": sig, "intro_json": json_str},
    })
}

fn intro_v2_fixture() -> Value {
    let (p, n) = (keys(1), keys(2));
    let st = IntroStatement {
        version: 2,
        ..intro_statement(&p, &n, IDS[1], USER)
    };
    let me = IntroStatement {
        version: 2,
        user_id: USER,
        introducer_fp: p.fingerprint(),
        target_id: IDS[0].into(),
        target_fp: p.fingerprint(),
        target_kind: "desktop".into(),
        issued_at_ms: NOW,
    };
    let section = |st: &IntroStatement, by: &DeviceKeys| {
        let json_str = sign_intro(by, IDS[0], st).unwrap();
        verify_intro(
            &json_str,
            USER,
            IDS[0],
            &by.sign_pub_b64(),
            &by.kex_pub_b64(),
        )
        .unwrap();
        let sig = serde_json::from_str::<Value>(&json_str).unwrap()["sig"].clone();
        json!({"statement": st.to_statement().unwrap(), "sig": sig, "intro_json": json_str})
    };
    json!({
        "description": "intro v2 signed by P for N (statement tag v2, envelope v 2), and P's self-intro (target is P itself; used only as the upgrade marker, its kind is never taken)",
        "inputs": {
            "user_id": USER,
            "introducer": {"device_id": IDS[0], "kind": "desktop", "keys": key_json(&p)},
            "target": {"device_id": IDS[1], "kind": "mobile", "keys": key_json(&n)},
            "issued_at_ms": NOW,
        },
        "outputs": {
            "intro": section(&st, &p),
            "self_intro": section(&me, &p),
        },
    })
}

fn revoke_fixture() -> Value {
    let (r, t) = (keys(1), keys(3));
    let st = RevokeStatement {
        user_id: USER,
        revoker_fp: r.fingerprint(),
        target_id: IDS[2].into(),
        target_fp: t.fingerprint(),
        issued_at_ms: NOW,
    };
    let statement = st.to_statement().unwrap();
    let json_str = sign_revoke(&r, &st).unwrap();
    verify_revoke(&json_str, USER, &r.sign_pub_b64(), &r.kex_pub_b64()).unwrap();
    let sig = serde_json::from_str::<Value>(&json_str).unwrap()["sig"].clone();
    json!({
        "description": "revoke signed by R for T; the json string is the POST /revocations statement",
        "inputs": {
            "user_id": USER,
            "revoker": {"device_id": IDS[0], "keys": key_json(&r)},
            "target": {"device_id": IDS[2], "keys": key_json(&t)},
            "issued_at_ms": NOW,
        },
        "outputs": {"statement": statement, "sig": sig, "revoke_json": json_str},
    })
}

fn join_section(label: &str) -> Value {
    join_section_v(label, 1)
}

fn join_section_v(label: &str, version: u8) -> Value {
    let (p, n) = (keys(1), keys(2));
    let secret = [0x5au8; 32];
    let code = JoinCode {
        version,
        user_id: USER,
        device_id: IDS[0].into(),
        fp: p.fingerprint(),
        kind: (version == 2).then(|| "desktop".to_owned()),
        secret,
        exp_ms: NOW + 600_000,
    };
    let encoded = encode_join_code(&code).unwrap();
    assert_eq!(decode_join_code(&encoded).unwrap(), code);
    let (pf, nf) = (p.fingerprint(), n.fingerprint());
    let input = JoinMacInput {
        version,
        user_id: USER,
        p_fp: &pf,
        n_id: IDS[1],
        n_fp: &nf,
        n_kind: "mobile",
        label,
    };
    let mac = join_mac(&secret, &input).unwrap();
    verify_join_mac(&secret, &input, &mac).unwrap();
    let raw = encoded
        .strip_prefix(&format!("tmgr-chat-join:v{version}:"))
        .unwrap();
    let mut code_json = json!({
        "user_id": USER,
        "device_id": IDS[0],
        "fp": pf,
        "secret_hex": hex(&secret),
        "secret_b64": b64(&secret),
        "exp_ms": NOW + 600_000,
    });
    if version == 2 {
        code_json["kind"] = json!("desktop");
    }
    json!({
        "inputs": {
            "code": code_json,
            "joiner": {"device_id": IDS[1], "kind": "mobile", "label": label, "fp": nf},
        },
        "outputs": {
            "code_string": encoded,
            "code_base64url": raw,
            "mac_transcript": format!(
                "tmgr-chat-join:v{version}:{USER}:{pf}:{}:{nf}:mobile:{}",
                IDS[1],
                h(label)
            ),
            "mac_hex": mac,
        },
    })
}

fn join_fixture() -> Value {
    let mut v = join_section("Phone");
    v["description"] = json!("join code, MAC key is the 32 decoded secret bytes");
    v
}

fn join_v2_fixture() -> Value {
    let mut v = join_section_v("Phone", 2);
    v["description"] = json!("join code v2 (prefix tmgr-chat-join:v2:, JSON v 2, kind after fp = the approver's kind) and the v2 MAC transcript; MAC key is the 32 decoded secret bytes");
    v
}

fn unicode_fixture() -> Value {
    json!({
        "description": "non-ASCII positive vectors: a chat payload and a join label; hashes are over the UTF-8 bytes, no \\u escapes in the inner string",
        "chat": message_fixture(
            "chat with non-ASCII payload",
            "chat",
            "m_00000000000000000000000000000003",
            1,
            json!({"text": "Привет, мир ✓ 日本語 😀", "n": 1}),
        ),
        "join": join_section("Телефон ✓ 日本語"),
    })
}

fn spawnkey_fixture() -> Value {
    let dev = keys(1);
    let spawn_secret = [0x77u8; 32];
    let spawn_pub = b64(&public_key(&spawn_secret).unwrap());
    let st = SpawnStatement {
        purpose: KeyPurpose::Spawn,
        user_id: USER,
        device_fp: dev.fingerprint(),
        device_id: IDS[0].into(),
        spawn_pub_b64: spawn_pub.clone(),
        issued_at_ms: NOW,
    };
    let k = sign_spawnkey(&dev, &spawn_secret, &st).unwrap();
    let payload = serde_json::to_string(&k).unwrap();
    verify_spawnkey(
        &payload,
        USER,
        IDS[0],
        &dev.sign_pub_b64(),
        &dev.kex_pub_b64(),
    )
    .unwrap();
    json!({
        "description": "spawn key statement signed by the device chat sign key and by the spawn key",
        "inputs": {
            "user_id": USER,
            "device": {"device_id": IDS[0], "keys": key_json(&dev)},
            "spawn_secret_hex": hex(&spawn_secret),
            "spawn_pub": spawn_pub,
            "issued_at_ms": NOW,
        },
        "outputs": {
            "statement": k.statement,
            "sig": k.sig,
            "spawn_sig": k.spawn_sig,
            "payload": serde_json::from_str::<Value>(&payload).unwrap(),
        },
    })
}

fn dir_json(entries: &[(&str, &DeviceKeys)]) -> Value {
    Value::Array(
        entries
            .iter()
            .map(|(id, k)| json!({"device_id": id, "sign_pub": k.sign_pub_b64(), "kex_pub": k.kex_pub_b64()}))
            .collect(),
    )
}

fn trust_case(
    name: &str,
    note: &str,
    pins: &[(&str, &DeviceKeys)],
    revoked: &[&DeviceKeys],
    intros: Vec<String>,
    directory: &[(&str, &DeviceKeys)],
) -> Value {
    let pin_list: Vec<Trusted> = pins.iter().map(|(i, k)| pin(i, k)).collect();
    let revoked_fps: Vec<String> = revoked.iter().map(|k| k.fingerprint()).collect();
    let dir: Vec<DirEntry> = directory
        .iter()
        .map(|(i, k)| DirEntry {
            device_id: i.to_string(),
            sign_pub_b64: k.sign_pub_b64(),
            kex_pub_b64: k.kex_pub_b64(),
        })
        .collect();
    let set = trusted_set(USER, &pin_list, &revoked_fps, &intros, &dir);
    json!({
        "name": name,
        "note": note,
        "inputs": {
            "pins": pin_list,
            "revoked": revoked_fps,
            "intros": intros,
            "directory": dir_json(directory),
        },
        "outputs": {"trusted": set.trusted, "mismatches": set.mismatches},
    })
}

fn intro_by(by: (&str, &DeviceKeys), target: (&str, &DeviceKeys)) -> String {
    sign_intro(by.1, by.0, &intro_statement(by.1, target.1, target.0, USER)).unwrap()
}

fn trust_fixture() -> Value {
    let (a, b, c, evil) = (keys(1), keys(2), keys(3), keys(66));
    let d4 = keys(4);
    let full = [(IDS[0], &a), (IDS[1], &b), (IDS[2], &c)];
    let pin_a = [(IDS[0], &a)];
    let cases = vec![
        trust_case(
            "chain",
            "A pinned; A introduces B; B introduces C; intros listed child first",
            &pin_a,
            &[],
            vec![
                intro_by((IDS[1], &b), (IDS[2], &c)),
                intro_by((IDS[0], &a), (IDS[1], &b)),
            ],
            &full,
        ),
        trust_case(
            "revoked_hub_cascade",
            "B revoked; C only had an intro from B, so C drops out",
            &pin_a,
            &[&b],
            vec![
                intro_by((IDS[0], &a), (IDS[1], &b)),
                intro_by((IDS[1], &b), (IDS[2], &c)),
            ],
            &full,
        ),
        trust_case(
            "revoked_hub_dense_graph",
            "B revoked; C also has a direct intro from A, so C stays",
            &pin_a,
            &[&b],
            vec![
                intro_by((IDS[0], &a), (IDS[1], &b)),
                intro_by((IDS[1], &b), (IDS[2], &c)),
                intro_by((IDS[0], &a), (IDS[2], &c)),
            ],
            &full,
        ),
        trust_case(
            "revoked_pin",
            "A and B pinned, A revoked: A is dropped and its intros are not used",
            &[(IDS[0], &a), (IDS[1], &b)],
            &[&a],
            vec![intro_by((IDS[0], &a), (IDS[2], &c))],
            &full,
        ),
        trust_case(
            "revoked_target_reentry",
            "B revoked; A signs an intro for the same keys under a new device id: the revoked fp stays out",
            &pin_a,
            &[&b],
            vec![intro_by((IDS[0], &a), (IDS[3], &b))],
            &[(IDS[0], &a), (IDS[3], &b)],
        ),
        trust_case(
            "pinned_key_mismatch_reported",
            "directory serves other keys for pinned A: A keeps its pinned keys, its intro for B still verifies, and the mismatch is reported",
            &pin_a,
            &[],
            vec![intro_by((IDS[0], &a), (IDS[1], &b))],
            &[(IDS[0], &evil), (IDS[1], &b)],
        ),
        trust_case(
            "intro_signed_by_directory_impostor",
            "an intro signed with the keys the directory serves for A is not accepted: A's pinned keys verify",
            &pin_a,
            &[],
            vec![intro_by((IDS[0], &evil), (IDS[3], &d4))],
            &[(IDS[0], &evil), (IDS[3], &d4)],
        ),
        trust_case(
            "intro_target_keys_must_match_fp",
            "directory serves other keys for N than the intro's N_fp: N is not added",
            &pin_a,
            &[],
            vec![intro_by((IDS[0], &a), (IDS[1], &b))],
            &[(IDS[0], &a), (IDS[1], &evil)],
        ),
        trust_case(
            "by_mismatch",
            "intro signed by A but its by field names another id: ignored",
            &pin_a,
            &[],
            vec![sign_intro(&a, IDS[3], &intro_statement(&a, &b, IDS[1], USER)).unwrap()],
            &full,
        ),
        trust_case(
            "untrusted_introducer",
            "C is not trusted, so its intro for D does not count",
            &pin_a,
            &[],
            vec![intro_by((IDS[2], &c), (IDS[3], &d4))],
            &[(IDS[0], &a), (IDS[2], &c), (IDS[3], &d4)],
        ),
        trust_case(
            "wrong_user",
            "intro statement carries user 43 while the set is computed for user 42: ignored",
            &pin_a,
            &[],
            vec![sign_intro(&a, IDS[0], &intro_statement(&a, &b, IDS[1], 43)).unwrap()],
            &full,
        ),
        trust_case(
            "withheld_intros_cached",
            "server serves no intros; locally cached intros keep B and C trusted",
            &pin_a,
            &[],
            vec![
                intro_by((IDS[0], &a), (IDS[1], &b)),
                intro_by((IDS[1], &b), (IDS[2], &c)),
            ],
            &full,
        ),
        trust_case(
            "withheld_intros_nothing_cached",
            "no intros at all: only the pin",
            &pin_a,
            &[],
            vec![],
            &full,
        ),
        trust_case(
            "pinned_id_not_overridden",
            "intro for a pinned id with a different fp is ignored",
            &[(IDS[0], &a), (IDS[1], &b)],
            &[],
            vec![intro_by((IDS[0], &a), (IDS[1], &keys(7)))],
            &full,
        ),
    ];
    json!({
        "description": "trusted_set(user_id 42, pins, revoked, intros, directory); every intro is a signed intro json string; trusted is sorted by (device_id, fp); mismatches lists trusted ids whose directory keys give another fp",
        "user_id": USER,
        "cases": cases,
    })
}

struct NegWorld {
    sender: DeviceKeys,
    recipient: DeviceKeys,
    ent: Entropy,
}

impl NegWorld {
    fn new() -> NegWorld {
        NegWorld {
            sender: keys(1),
            recipient: keys(2),
            ent: Entropy {
                eph_secret: [7u8; 32],
                nonce: [8u8; 12],
            },
        }
    }

    fn valid_body(&self, kind: &str, id: &str) -> String {
        let (sp, kp) = (self.recipient.sign_pub_b64(), self.recipient.kex_pub_b64());
        seal_message_with(
            &self.sender,
            &SealParams {
                user_id: USER,
                from_device_id: IDS[0],
                to: Peer {
                    device_id: IDS[1],
                    sign_pub_b64: &sp,
                    kex_pub_b64: &kp,
                },
                kind,
                msg_id: id,
                sent_at_ms: SENT_AT,
                seq: 1,
                payload: json!({"text": "neg"}),
            },
            &self.ent,
        )
        .unwrap()
    }

    fn inner(&self, extra: &str) -> String {
        format!(
            "{{\"v\":1,\"kind\":\"chat\",\"id\":\"{NEG_MSG_ID}\",\"sent_at\":{SENT_AT},\"seq\":1,{extra}}}"
        )
    }

    fn sig(&self, inner: &str) -> Vec<u8> {
        let s = signed_string(
            USER,
            &self.sender.fingerprint(),
            &self.recipient.fingerprint(),
            IDS[0],
            IDS[1],
            "chat",
            NEG_MSG_ID,
            SENT_AT,
            inner,
        )
        .unwrap();
        b64_decode(&sign(self.sender.sign_secret(), &s).unwrap()).unwrap()
    }

    fn seal_plain(&self, plain: &str, to: &DeviceKeys) -> String {
        let ctx = chat_ctx(USER, IDS[0], IDS[1], "chat", NEG_MSG_ID).unwrap();
        let s = seal_ctx_with(
            &b64_decode(&to.kex_pub_b64()).unwrap(),
            &ctx,
            "chat",
            NEG_MSG_ID,
            plain.as_bytes(),
            &self.ent,
        )
        .unwrap();
        serde_json::to_string(&Body {
            v: 1,
            kind: s.kind,
            id: s.id,
            from: IDS[0].into(),
            eph: s.eph,
            nonce: s.nonce,
            ct: s.ct,
        })
        .unwrap()
    }

    fn body_for(&self, inner: &str, sig: &[u8]) -> String {
        let plain = format!(
            "{{\"body\":{},\"sig\":\"{}\"}}",
            serde_json::to_string(inner).unwrap(),
            b64(sig)
        );
        self.seal_plain(&plain, &self.recipient)
    }

    fn signed_body(&self, inner: &str) -> String {
        self.body_for(inner, &self.sig(inner))
    }
}

const PAYLOAD: &str = "\"payload\":{\"text\":\"neg\"}";

fn neg_msg(name: &str, class: &str, body: String, expect: Value) -> Value {
    json!({"name": name, "class": class, "body": body, "expect": expect})
}

fn neg_msg_with(name: &str, class: &str, body: String, expect: Value, extra: Value) -> Value {
    let mut v = neg_msg(name, class, body, expect);
    for (k, x) in extra.as_object().unwrap() {
        v[k] = x.clone();
    }
    v
}

fn err(code: Error) -> Value {
    json!({"error": code.code()})
}

fn ok_open(trusted: bool) -> Value {
    json!({"ok": {"seq": 1, "trusted": trusted}})
}

fn high_s(der: &[u8]) -> Vec<u8> {
    let sig = Signature::from_der(der).unwrap();
    let (r, s) = sig.split_scalars();
    let neg = -*s.as_ref();
    let hs = Signature::from_scalars(r, p256::NonZeroScalar::new(neg).unwrap()).unwrap();
    assert!(hs.normalize_s().is_some());
    hs.to_der().as_bytes().to_vec()
}

fn raw64(der: &[u8]) -> Vec<u8> {
    Signature::from_der(der).unwrap().to_bytes().to_vec()
}

fn b64_alphabet_index(c: char) -> usize {
    "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        .find(c)
        .unwrap()
}

fn negative_fixture() -> Value {
    let w = NegWorld::new();
    let inner = w.inner(PAYLOAD);
    let der = w.sig(&inner);
    let valid = w.valid_body("chat", NEG_MSG_ID);
    let mut vectors = vec![];

    vectors.push(neg_msg(
        "valid_baseline",
        "positive",
        valid.clone(),
        ok_open(true),
    ));

    vectors.push(neg_msg(
        "duplicate_key_in_body",
        "duplicate_keys",
        valid.replacen("{\"v\":1,", "{\"v\":1,\"v\":1,", 1),
        err(Error::BadJson),
    ));

    let plain_dup = format!(
        "{{\"body\":{},\"sig\":\"{}\",\"sig\":\"{}\"}}",
        serde_json::to_string(&inner).unwrap(),
        b64(&der),
        b64(&der)
    );
    vectors.push(neg_msg(
        "duplicate_key_in_plaintext",
        "duplicate_keys",
        w.seal_plain(&plain_dup, &w.recipient),
        err(Error::BadJson),
    ));

    vectors.push(neg_msg(
        "duplicate_key_in_inner",
        "duplicate_keys",
        w.signed_body(&w.inner(&format!("\"seq\":1,{PAYLOAD}"))),
        err(Error::BadJson),
    ));

    vectors.push(neg_msg(
        "duplicate_key_in_payload",
        "duplicate_keys",
        w.signed_body(&w.inner("\"payload\":{\"a\":1,\"a\":2}")),
        err(Error::BadJson),
    ));

    let parsed: Body = serde_json::from_str(&valid).unwrap();
    let with_eph = |eph: String| {
        serde_json::to_string(&Body {
            eph,
            ..parsed.clone()
        })
        .unwrap()
    };
    vectors.push(neg_msg(
        "unpadded_base64_eph",
        "base64",
        with_eph(parsed.eph.trim_end_matches('=').to_string()),
        err(Error::BadFormat),
    ));

    let mut chars: Vec<char> = parsed.eph.chars().collect();
    let pad_at = chars.iter().position(|c| *c == '=').unwrap();
    let last = chars[pad_at - 1];
    let idx = b64_alphabet_index(last);
    assert_eq!(idx & 3, 0, "fixture eph tail must have zero trailing bits");
    chars[pad_at - 1] = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
        .chars()
        .nth(idx | 1)
        .unwrap();
    vectors.push(neg_msg(
        "trailing_bits_base64_eph",
        "base64",
        with_eph(chars.into_iter().collect()),
        err(Error::BadFormat),
    ));

    vectors.push(neg_msg(
        "whitespace_in_base64_ct",
        "base64",
        serde_json::to_string(&Body {
            ct: format!("{}\n", parsed.ct),
            ..parsed.clone()
        })
        .unwrap(),
        err(Error::BadFormat),
    ));

    let exp_inner = format!(
        "{{\"v\":1,\"kind\":\"chat\",\"id\":\"{NEG_MSG_ID}\",\"sent_at\":1.79999999e12,\"seq\":1,{PAYLOAD}}}"
    );
    vectors.push(neg_msg(
        "exponent_integer_sent_at",
        "integer",
        w.signed_body(&exp_inner),
        err(Error::BadJson),
    ));
    let float_seq = w.inner(PAYLOAD).replace("\"seq\":1,", "\"seq\":1.0,");
    vectors.push(neg_msg(
        "float_seq",
        "integer",
        w.signed_body(&float_seq),
        err(Error::BadJson),
    ));

    vectors.push(neg_msg(
        "lone_surrogate_escape_in_payload",
        "utf8",
        w.signed_body(&w.inner("\"payload\":{\"t\":\"\\ud800\"}")),
        err(Error::BadJson),
    ));

    let carol = keys(3);
    vectors.push(neg_msg(
        "sealed_to_other_recipient",
        "sealing",
        w.seal_plain(
            &format!(
                "{{\"body\":{},\"sig\":\"{}\"}}",
                serde_json::to_string(&inner).unwrap(),
                b64(&der)
            ),
            &carol,
        ),
        err(Error::OpenFailed),
    ));

    vectors.push(neg_msg_with(
        "body_for_different_user",
        "sealing",
        valid.clone(),
        err(Error::OpenFailed),
        json!({"user_id": 43}),
    ));

    vectors.push(neg_msg_with(
        "untrusted_sender_chat",
        "trust",
        valid.clone(),
        err(Error::BadKey),
        json!({"sender_trusted": false}),
    ));

    vectors.push(neg_msg_with(
        "untrusted_sender_join_from_directory",
        "trust",
        w.valid_body("join", "j_00000000000000000000000000000001"),
        ok_open(false),
        json!({"sender_trusted": false, "msg_id": "j_00000000000000000000000000000001"}),
    ));

    let high = high_s(&der);
    for (name, sig, expect) in [
        ("signature_high_s_der_accepted", high.clone(), ok_open(true)),
        (
            "signature_raw64_low_s_rejected",
            raw64(&der),
            err(Error::BadSignature),
        ),
        (
            "signature_raw64_high_s_rejected",
            raw64(&high),
            err(Error::BadSignature),
        ),
    ] {
        vectors.push(neg_msg(name, "signature", w.body_for(&inner, &sig), expect));
    }

    for (name, now, expect) in [
        (
            "window_oldest_accepted",
            SENT_AT + MAX_AGE_MS,
            ok_open(true),
        ),
        (
            "window_too_old",
            SENT_AT + MAX_AGE_MS + 1,
            err(Error::BadWindow),
        ),
        (
            "window_future_edge_accepted",
            SENT_AT - MAX_AGE_MS,
            ok_open(true),
        ),
        (
            "window_too_far_in_future",
            SENT_AT - MAX_AGE_MS - 1,
            err(Error::BadWindow),
        ),
    ] {
        vectors.push(neg_msg_with(
            name,
            "window",
            valid.clone(),
            expect,
            json!({"now_ms": now}),
        ));
    }

    let (p, n) = (keys(1), keys(2));
    let good_intro = sign_intro(&p, IDS[0], &intro_statement(&p, &n, IDS[1], USER)).unwrap();
    let signer =
        json!({"device_id": IDS[0], "sign_pub": p.sign_pub_b64(), "kex_pub": p.kex_pub_b64()});
    let other = keys(9);
    let intro_vec = |name: &str, json_str: &str, user: u64, signer: Value, expect: Value| json!({"name": name, "intro_json": json_str, "user_id": user, "signer": signer, "expect": expect});
    let intro_vectors = vec![
        intro_vec(
            "valid",
            &good_intro,
            USER,
            signer.clone(),
            json!({"ok": true}),
        ),
        intro_vec(
            "by_mismatch",
            &sign_intro(&p, IDS[3], &intro_statement(&p, &n, IDS[1], USER)).unwrap(),
            USER,
            signer.clone(),
            err(Error::IdMismatch),
        ),
        intro_vec(
            "wrong_user",
            &sign_intro(&p, IDS[0], &intro_statement(&p, &n, IDS[1], 43)).unwrap(),
            USER,
            signer.clone(),
            err(Error::IdMismatch),
        ),
        intro_vec(
            "signer_keys_do_not_match_introducer_fp",
            &good_intro,
            USER,
            json!({"device_id": IDS[0], "sign_pub": other.sign_pub_b64(), "kex_pub": other.kex_pub_b64()}),
            err(Error::BadKey),
        ),
        intro_vec(
            "duplicate_key_in_intro_json",
            &good_intro.replacen("{\"v\":1,", "{\"v\":1,\"v\":1,", 1),
            USER,
            signer.clone(),
            err(Error::BadJson),
        ),
    ];

    json!({
        "description": "negative and edge vectors; message vectors are opened with context.recipient, context.sender pinned (unless sender_trusted is false) and per-vector overrides user_id, now_ms, msg_id; expect is {error: code} or {ok: ...}",
        "context": {
            "user_id": USER,
            "now_ms": NOW,
            "msg_id": NEG_MSG_ID,
            "recipient": {"device_id": IDS[1], "keys": key_json(&w.recipient)},
            "sender": {"device_id": IDS[0], "keys": key_json(&w.sender)},
        },
        "message_vectors": vectors,
        "intro_vectors": intro_vectors,
    })
}

const TRUST_V2_DEVICES: [&str; 5] = ["A", "B", "X", "Y", "Z"];

fn tkeys() -> Vec<DeviceKeys> {
    (1..=5).map(keys).collect()
}

fn tkey(k: &[DeviceKeys], i: usize) -> String {
    format!("{}:{}", IDS[i], k[i].fingerprint())
}

fn tintro(k: &[DeviceKeys], ver: u8, by: usize, target: usize, kind: &str) -> String {
    let st = IntroStatement {
        version: ver,
        user_id: USER,
        introducer_fp: k[by].fingerprint(),
        target_id: IDS[target].into(),
        target_fp: k[target].fingerprint(),
        target_kind: kind.into(),
        issued_at_ms: NOW,
    };
    sign_intro(&k[by], IDS[by], &st).unwrap()
}

struct TrustV2Case<'a> {
    name: &'a str,
    note: &'a str,
    start: &'a [usize],
    roots: &'a [usize],
    known: &'a [(usize, &'a str)],
    own: Option<usize>,
    max_passes: Option<usize>,
    intros: Vec<String>,
    directory: &'a [usize],
    before: Option<&'a [usize]>,
    same_outputs_as: Option<&'a str>,
}

fn trust_v2_run(user: u64, inputs: &Value) -> Value {
    let list = |v: &Value| -> Vec<Value> { v.as_array().unwrap().clone() };
    let pins: Vec<Trusted> = list(&inputs["start"])
        .iter()
        .map(|p| {
            Trusted::new(
                p["device_id"].as_str().unwrap(),
                p["sign_pub"].as_str().unwrap(),
                p["kex_pub"].as_str().unwrap(),
            )
            .unwrap()
        })
        .collect();
    let strs = |v: &Value| -> Vec<String> {
        list(v)
            .iter()
            .map(|x| x.as_str().unwrap().to_owned())
            .collect()
    };
    let revoked = strs(&inputs["revoked"]);
    let intros = strs(&inputs["intros"]);
    let dir: Vec<DirEntry> = list(&inputs["directory"])
        .iter()
        .map(|d| DirEntry {
            device_id: d["device_id"].as_str().unwrap().into(),
            sign_pub_b64: d["sign_pub"].as_str().unwrap().into(),
            kex_pub_b64: d["kex_pub"].as_str().unwrap().into(),
        })
        .collect();
    let known: BTreeMap<String, String> = inputs["known"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap().to_owned()))
        .collect();
    let assumed: BTreeSet<String> = strs(&inputs["roots"])
        .into_iter()
        .filter(|k| !known.contains_key(k))
        .collect();
    let mut set = trusted_set_by_kind(
        user,
        &pins,
        &revoked,
        &intros,
        &dir,
        Some(&KindCtx {
            known: &known,
            own: inputs["own"].as_str(),
            assumed: &assumed,
            max_passes: inputs["max_passes"].as_u64().map(|n| n as usize),
        }),
    );
    if let Some(r) = inputs["revoke_from_base"].as_object() {
        let before: BTreeSet<String> = strs(&r["trusted_before"]).into_iter().collect();
        set = restrict_to_before(set, &before, &known);
    }
    let mut trusted: Vec<Value> = set
        .trusted
        .iter()
        .map(|t| json!({"device_id": t.device_id, "fp": t.fp}))
        .collect();
    trusted.sort_by_key(|v| v.to_string());
    let mut kinds: Vec<Value> = set
        .kinds
        .iter()
        .map(|(id, fp, kind)| json!({"device_id": id, "fp": fp, "kind": kind}))
        .collect();
    kinds.sort_by_key(|v| v.to_string());
    let mut conflicts: Vec<Value> = set
        .conflicts
        .iter()
        .map(|(id, fp)| json!({"device_id": id, "fp": fp}))
        .collect();
    conflicts.sort_by_key(|v| v.to_string());
    let unstable: BTreeSet<String> = set.unstable.iter().cloned().collect();
    json!({
        "trusted": trusted,
        "kinds": kinds,
        "conflicts": conflicts,
        "unstable": unstable,
        "passes": set.passes,
    })
}

fn trust_v2_case(c: TrustV2Case) -> Value {
    let k = tkeys();
    let devices: Vec<Value> = (0..5)
        .map(|i| json!({"name": TRUST_V2_DEVICES[i], "device_id": IDS[i], "keys": key_json(&k[i])}))
        .collect();
    let known: BTreeMap<String, String> = c
        .known
        .iter()
        .map(|(i, kind)| (tkey(&k, *i), (*kind).to_owned()))
        .collect();
    let mut inputs = json!({
        "own": c.own.map(|i| tkey(&k, i)),
        "start": c.start.iter().map(|i| pin(IDS[*i], &k[*i])).collect::<Vec<_>>(),
        "roots": c.roots.iter().map(|i| tkey(&k, *i)).collect::<Vec<_>>(),
        "known": known,
        "revoked": Vec::<String>::new(),
        "max_passes": c.max_passes,
        "intros": c.intros,
        "directory": dir_json(&c.directory.iter().map(|i| (IDS[*i], &k[*i])).collect::<Vec<_>>()),
        "revoke_from_base": c.before.map(|b| json!({"trusted_before": b.iter().map(|i| tkey(&k, *i)).collect::<Vec<_>>()})),
    });
    inputs["devices"] = Value::Array(devices);
    let outputs = trust_v2_run(USER, &inputs);
    let mut case = json!({"name": c.name, "note": c.note, "inputs": inputs, "outputs": outputs});
    if let Some(o) = c.same_outputs_as {
        case["same_outputs_as"] = json!(o);
    }
    case
}

fn trust_v2_fixture() -> Value {
    let k = tkeys();
    let (a, b, x, y, z) = (0, 1, 2, 3, 4);
    let i = |ver: u8, by: usize, target: usize, kind: &str| tintro(&k, ver, by, target, kind);
    let base = |name, note, intros: Vec<String>| TrustV2Case {
        name,
        note,
        start: &[0],
        roots: &[],
        known: &[(0, "desktop")],
        own: Some(0),
        max_passes: None,
        intros,
        directory: &[1, 2, 3, 4],
        before: None,
        same_outputs_as: None,
    };
    let chain = || {
        vec![
            i(2, a, x, "desktop"),
            i(2, x, y, "desktop"),
            i(2, y, z, "desktop"),
        ]
    };
    let mut chain_rev = chain();
    chain_rev.reverse();
    let cases = vec![
        trust_v2_case(base(
            "runner_wins_desktop_first",
            "A vouches X as desktop and as runner; the result is runner whatever the order, with a conflict",
            vec![i(2, a, x, "desktop"), i(2, a, x, "runner")],
        )),
        trust_v2_case(TrustV2Case {
            same_outputs_as: Some("runner_wins_desktop_first"),
            ..base(
                "runner_wins_runner_first",
                "same intros in the reverse order",
                vec![i(2, a, x, "runner"), i(2, a, x, "desktop")],
            )
        }),
        trust_v2_case(TrustV2Case {
            start: &[0, 1],
            known: &[(0, "desktop"), (1, "desktop")],
            ..base(
                "demotion_rerun_drops_what_the_demoted_vouched_for",
                "A vouches X as desktop, X vouches Y, B vouches X as runner: X is demoted and Y falls out",
                vec![i(2, a, x, "desktop"), i(2, x, y, "desktop"), i(2, b, x, "runner")],
            )
        }),
        trust_v2_case(TrustV2Case {
            start: &[0, 1, 2, 3],
            known: &[(0, "desktop"), (1, "desktop"), (2, "desktop")],
            ..base(
                "m3_a_candidate_from_a_demoted_signer_does_not_survive",
                "X (pinned, desktop) names pinned Y desktop, B names X runner: Y keeps no kind after X is demoted",
                vec![i(2, b, x, "runner"), i(2, x, y, "desktop")],
            )
        }),
        trust_v2_case(base(
            "chain_settles_in_four_passes",
            "A -> X -> Y -> Z, each step needs the kind learned in the previous pass",
            chain(),
        )),
        trust_v2_case(TrustV2Case {
            max_passes: Some(1),
            ..base(
                "cap_reached_raises_trust_unstable_and_keeps_only_own",
                "the same chain with max_passes 1: unstable, only the own key stays trusted, no kinds",
                chain(),
            )
        }),
        trust_v2_case(TrustV2Case {
            same_outputs_as: Some("chain_settles_in_four_passes"),
            ..base(
                "order_independence_reversed_chain",
                "the chain intros in reverse order give the same output",
                chain_rev,
            )
        }),
        trust_v2_case(TrustV2Case {
            start: &[0, 2],
            ..base(
                "self_intro_yields_no_kind",
                "self-intros (X says mobile, A says mobile) never give a kind, not even to their signer",
                vec![i(2, x, x, "mobile"), i(2, a, a, "mobile")],
            )
        }),
        trust_v2_case(TrustV2Case {
            known: &[(0, "desktop"), (2, "desktop")],
            ..base(
                "v1_counts_for_a_target_with_a_known_kind",
                "gate closed: A's v1 intro admits X whose kind is known, and gives no kind",
                vec![i(1, a, x, "mobile")],
            )
        }),
        trust_v2_case(base(
            "v1_ignored_for_a_target_of_unknown_kind",
            "A's v1 intro for X is ignored because X has no known kind",
            vec![i(1, a, x, "mobile")],
        )),
        trust_v2_case(TrustV2Case {
            known: &[(0, "desktop"), (2, "desktop")],
            own: None,
            ..base(
                "v1_ignored_once_the_gate_is_open",
                "A and X both show a counted v2 self-intro, so the gate opens and the v1 intro for X no longer counts",
                vec![i(1, a, x, "mobile"), i(2, a, a, "desktop"), i(2, x, x, "desktop")],
            )
        }),
        trust_v2_case(TrustV2Case {
            start: &[0, 1],
            known: &[(0, "desktop"), (1, "desktop")],
            ..base(
                "tie_desktop_vs_mobile_without_an_existing_kind",
                "A says X desktop, B says X mobile, X has no kind: the kind stays unknown and a conflict is raised",
                vec![i(2, a, x, "desktop"), i(2, b, x, "mobile")],
            )
        }),
        trust_v2_case(TrustV2Case {
            start: &[0, 1],
            known: &[(0, "desktop"), (1, "desktop"), (2, "desktop")],
            ..base(
                "tie_desktop_vs_mobile_keeps_the_existing_kind",
                "X is already desktop; a mobile claim only raises a conflict",
                vec![i(2, a, x, "mobile"), i(2, b, x, "desktop")],
            )
        }),
        trust_v2_case(TrustV2Case {
            start: &[0],
            roots: &[0],
            known: &[],
            own: None,
            directory: &[1],
            before: Some(&[0]),
            ..base(
                "m1_revoke_from_base_never_trusts_what_was_untrusted_before",
                "root A has no known kind and vouches D as desktop; D was not trusted before the revoke: D stays out and no kind is stored",
                vec![i(2, a, b, "desktop")],
            )
        }),
        trust_v2_case(TrustV2Case {
            start: &[0],
            roots: &[0],
            known: &[],
            own: None,
            directory: &[1],
            before: Some(&[0, 1]),
            ..base(
                "m1_revoke_from_base_keeps_what_was_trusted_before_without_learning_its_kind",
                "same intro, D was trusted before: D stays, but the kind the unknown-kind root vouched is not stored",
                vec![i(2, a, b, "desktop")],
            )
        }),
        trust_v2_case(TrustV2Case {
            start: &[0, 1],
            roots: &[],
            known: &[(0, "desktop"), (1, "desktop"), (2, "desktop")],
            before: Some(&[0, 1, 2]),
            ..base(
                "m1_revoke_from_base_may_lower_a_known_kind",
                "X was desktop; B vouches X as runner: X stays and is lowered to runner",
                vec![i(2, a, x, "desktop"), i(2, b, x, "runner")],
            )
        }),
    ];
    let _ = (y, z);
    json!({
        "description": "trust evaluation with device kinds (intro v2): trusted_set_by_kind plus the revoke-from-base restriction. See README.md for the case schema",
        "user_id": USER,
        "cases": cases,
    })
}

fn all() -> Vec<(&'static str, Value)> {
    vec![
        ("chat", chat_fixture()),
        ("blob", blob_fixture()),
        ("unicode", unicode_fixture()),
        ("intro", intro_fixture()),
        ("intro_v2", intro_v2_fixture()),
        ("revoke", revoke_fixture()),
        ("join", join_fixture()),
        ("join_v2", join_v2_fixture()),
        ("spawnkey", spawnkey_fixture()),
        ("trust", trust_fixture()),
        ("trust_v2", trust_v2_fixture()),
        ("negative", negative_fixture()),
    ]
}

#[test]
fn fixtures_are_current() {
    for (name, v) in all() {
        let on_disk = std::fs::read_to_string(dir_path(name))
            .unwrap_or_else(|_| panic!("{name}.json missing; run write_fixtures"));
        assert_eq!(
            on_disk,
            text(&v),
            "{name}.json is stale; run write_fixtures"
        );
    }
}

#[test]
#[ignore]
fn write_fixtures() {
    for (name, v) in all() {
        std::fs::write(dir_path(name), text(&v)).unwrap();
    }
}

fn read(name: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir_path(name)).unwrap()).unwrap()
}

fn from_hex(s: &Value) -> Vec<u8> {
    let s = s.as_str().unwrap();
    (0..s.len())
        .step_by(2)
        .map(|n| u8::from_str_radix(&s[n..n + 2], 16).unwrap())
        .collect()
}

fn device_keys(v: &Value) -> DeviceKeys {
    DeviceKeys::from_secrets(
        &from_hex(&v["sign_secret_hex"]),
        &from_hex(&v["kex_secret_hex"]),
    )
    .unwrap()
}

fn open_committed(f: &Value, body_key: &str) {
    let i = &f["inputs"];
    let recipient = device_keys(&i["recipient"]["keys"]);
    let sk = &i["sender"]["keys"];
    let sender = Trusted::new(
        i["sender"]["device_id"].as_str().unwrap(),
        sk["sign_pub"].as_str().unwrap(),
        sk["kex_pub"].as_str().unwrap(),
    )
    .unwrap();
    let o = open_message(
        &recipient,
        i["user_id"].as_u64().unwrap(),
        i["recipient"]["device_id"].as_str().unwrap(),
        i["sender"]["device_id"].as_str().unwrap(),
        i["msg_id"].as_str().unwrap(),
        f["outputs"][body_key].as_str().unwrap(),
        i["now_ms"].as_u64().unwrap(),
        &[sender],
        |_| None,
    )
    .unwrap();
    assert_eq!(o.seq, i["seq"].as_u64().unwrap());
    assert_eq!(o.payload, i["payload"]);
    assert_eq!(o.from_fp, sk["fp"].as_str().unwrap());
    assert!(o.trusted);
}

#[test]
fn committed_message_bodies_open_from_inputs_only() {
    for name in ["chat", "blob"] {
        let f = read(name);
        for body_key in ["body", "body_unpadded"] {
            open_committed(&f, body_key);
        }
        assert_eq!(f["outputs"]["plaintext_len"], 1024);
    }
    let u = read("unicode");
    open_committed(&u["chat"], "body");
}

#[test]
fn committed_negative_vectors_behave_as_declared() {
    let f = read("negative");
    let c = &f["context"];
    let recipient = device_keys(&c["recipient"]["keys"]);
    let sk = &c["sender"]["keys"];
    let sender_id = c["sender"]["device_id"].as_str().unwrap();
    let pinned = Trusted::new(
        sender_id,
        sk["sign_pub"].as_str().unwrap(),
        sk["kex_pub"].as_str().unwrap(),
    )
    .unwrap();
    for v in f["message_vectors"].as_array().unwrap() {
        let name = v["name"].as_str().unwrap();
        let user = v.get("user_id").unwrap_or(&c["user_id"]).as_u64().unwrap();
        let now = v.get("now_ms").unwrap_or(&c["now_ms"]).as_u64().unwrap();
        let msg_id = v.get("msg_id").unwrap_or(&c["msg_id"]).as_str().unwrap();
        let trusted_sender = v.get("sender_trusted").is_none_or(|b| b.as_bool().unwrap());
        let pins: Vec<Trusted> = if trusted_sender {
            vec![pinned.clone()]
        } else {
            vec![]
        };
        let (sp, kp) = (
            sk["sign_pub"].as_str().unwrap(),
            sk["kex_pub"].as_str().unwrap(),
        );
        let r = open_message(
            &recipient,
            user,
            c["recipient"]["device_id"].as_str().unwrap(),
            sender_id,
            msg_id,
            v["body"].as_str().unwrap(),
            now,
            &pins,
            |_| {
                Some(Peer {
                    device_id: sender_id,
                    sign_pub_b64: sp,
                    kex_pub_b64: kp,
                })
            },
        );
        match (&v["expect"]["error"], &v["expect"]["ok"]) {
            (Value::String(code), _) => {
                assert_eq!(r.err().map(|e| e.code()), Some(code.as_str()), "{name}")
            }
            (_, ok) => {
                let o = r.unwrap_or_else(|e| panic!("{name}: {e}"));
                assert_eq!(o.seq, ok["seq"].as_u64().unwrap(), "{name}");
                assert_eq!(o.trusted, ok["trusted"].as_bool().unwrap(), "{name}");
            }
        }
    }
    for v in f["intro_vectors"].as_array().unwrap() {
        let name = v["name"].as_str().unwrap();
        let s = &v["signer"];
        let r = verify_intro(
            v["intro_json"].as_str().unwrap(),
            v["user_id"].as_u64().unwrap(),
            s["device_id"].as_str().unwrap(),
            s["sign_pub"].as_str().unwrap(),
            s["kex_pub"].as_str().unwrap(),
        );
        match v["expect"]["error"].as_str() {
            Some(code) => assert_eq!(r.err().map(|e| e.code()), Some(code), "{name}"),
            None => assert!(r.is_ok(), "{name}"),
        }
    }
}

#[test]
fn committed_trust_cases_recompute() {
    let f = read("trust");
    let user = f["user_id"].as_u64().unwrap();
    for c in f["cases"].as_array().unwrap() {
        let i = &c["inputs"];
        let pins: Vec<Trusted> = i["pins"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| {
                Trusted::new(
                    p["device_id"].as_str().unwrap(),
                    p["sign_pub"].as_str().unwrap(),
                    p["kex_pub"].as_str().unwrap(),
                )
                .unwrap()
            })
            .collect();
        let revoked: Vec<String> = i["revoked"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_string())
            .collect();
        let intros: Vec<String> = i["intros"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_string())
            .collect();
        let dir: Vec<DirEntry> = i["directory"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| DirEntry {
                device_id: d["device_id"].as_str().unwrap().into(),
                sign_pub_b64: d["sign_pub"].as_str().unwrap().into(),
                kex_pub_b64: d["kex_pub"].as_str().unwrap().into(),
            })
            .collect();
        let set = trusted_set(user, &pins, &revoked, &intros, &dir);
        assert_eq!(
            serde_json::to_value(&set.trusted).unwrap(),
            c["outputs"]["trusted"],
            "{}",
            c["name"]
        );
        assert_eq!(
            serde_json::to_value(&set.mismatches).unwrap(),
            c["outputs"]["mismatches"],
            "{}",
            c["name"]
        );
    }
}

#[test]
fn committed_v2_vectors_verify_from_inputs_only() {
    let f = read("intro_v2");
    let i = &f["inputs"];
    let by = device_keys(&i["introducer"]["keys"]);
    for key in ["intro", "self_intro"] {
        let o = &f["outputs"][key];
        let v = verify_intro(
            o["intro_json"].as_str().unwrap(),
            i["user_id"].as_u64().unwrap(),
            i["introducer"]["device_id"].as_str().unwrap(),
            &by.sign_pub_b64(),
            &by.kex_pub_b64(),
        )
        .unwrap();
        assert_eq!(v.parsed.version, 2);
        assert_eq!(v.statement, o["statement"].as_str().unwrap());
        let swapped = o["intro_json"]
            .as_str()
            .unwrap()
            .replace("tmgr-chat-intro:v2:", "tmgr-chat-intro:v1:");
        assert!(verify_intro(
            &swapped,
            USER,
            IDS[0],
            &by.sign_pub_b64(),
            &by.kex_pub_b64()
        )
        .is_err());
    }
    let j = read("join_v2");
    let code = decode_join_code(j["outputs"]["code_string"].as_str().unwrap()).unwrap();
    assert_eq!((code.version, code.kind.as_deref()), (2, Some("desktop")));
    let jc = &j["inputs"]["joiner"];
    let secret: [u8; 32] = from_hex(&j["inputs"]["code"]["secret_hex"])
        .try_into()
        .unwrap();
    let input = JoinMacInput {
        version: 2,
        user_id: USER,
        p_fp: &code.fp,
        n_id: jc["device_id"].as_str().unwrap(),
        n_fp: jc["fp"].as_str().unwrap(),
        n_kind: jc["kind"].as_str().unwrap(),
        label: jc["label"].as_str().unwrap(),
    };
    verify_join_mac(&secret, &input, j["outputs"]["mac_hex"].as_str().unwrap()).unwrap();
    let v1 = JoinMacInput {
        version: 1,
        ..input
    };
    assert!(verify_join_mac(&secret, &v1, j["outputs"]["mac_hex"].as_str().unwrap()).is_err());
}

#[test]
fn committed_trust_v2_cases_recompute() {
    let f = read("trust_v2");
    let user = f["user_id"].as_u64().unwrap();
    let cases = f["cases"].as_array().unwrap();
    for c in cases {
        assert_eq!(
            trust_v2_run(user, &c["inputs"]),
            c["outputs"],
            "{}",
            c["name"]
        );
        if let Some(other) = c["same_outputs_as"].as_str() {
            let o = cases.iter().find(|x| x["name"] == other).unwrap();
            assert_eq!(c["outputs"], o["outputs"], "{}", c["name"]);
        }
    }
}

#[test]
fn trust_v2_cases_pin_the_documented_semantics() {
    let f = read("trust_v2");
    let find = |name: &str| -> Value {
        f["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap_or_else(|| panic!("{name}"))["outputs"]
            .clone()
    };
    let ids = |v: &Value, key: &str| -> Vec<String> {
        let mut out: Vec<String> = v[key]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                IDS.iter()
                    .position(|i| *i == t["device_id"])
                    .map_or("?", |n| ["A", "B", "X", "Y", "Z"][n])
                    .to_owned()
            })
            .collect();
        out.sort();
        out
    };
    let kinds = |v: &Value| -> Vec<(String, String)> {
        v["kinds"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| {
                let n = IDS.iter().position(|i| *i == t["device_id"]).unwrap();
                (
                    ["A", "B", "X", "Y", "Z"][n].to_owned(),
                    t["kind"].as_str().unwrap().to_owned(),
                )
            })
            .collect()
    };
    let o = find("runner_wins_desktop_first");
    assert_eq!(ids(&o, "trusted"), ["A", "X"]);
    assert_eq!(kinds(&o), [("X".to_owned(), "runner".to_owned())]);
    assert_eq!(ids(&o, "conflicts"), ["X"]);
    let o = find("demotion_rerun_drops_what_the_demoted_vouched_for");
    assert_eq!(ids(&o, "trusted"), ["A", "B", "X"]);
    assert_eq!(kinds(&o), [("X".to_owned(), "runner".to_owned())]);
    let o = find("m3_a_candidate_from_a_demoted_signer_does_not_survive");
    assert_eq!(ids(&o, "trusted"), ["A", "B", "X", "Y"]);
    assert_eq!(kinds(&o), [("X".to_owned(), "runner".to_owned())]);
    let o = find("chain_settles_in_four_passes");
    assert_eq!(ids(&o, "trusted"), ["A", "X", "Y", "Z"]);
    assert_eq!(o["passes"], 4);
    let o = find("cap_reached_raises_trust_unstable_and_keeps_only_own");
    assert_eq!(ids(&o, "trusted"), ["A"]);
    assert!(!o["unstable"].as_array().unwrap().is_empty());
    assert!(o["kinds"].as_array().unwrap().is_empty());
    let o = find("self_intro_yields_no_kind");
    assert!(o["kinds"].as_array().unwrap().is_empty());
    assert!(o["conflicts"].as_array().unwrap().is_empty());
    assert_eq!(
        ids(&find("v1_counts_for_a_target_with_a_known_kind"), "trusted"),
        ["A", "X"]
    );
    assert_eq!(
        ids(&find("v1_ignored_for_a_target_of_unknown_kind"), "trusted"),
        ["A"]
    );
    assert_eq!(
        ids(&find("v1_ignored_once_the_gate_is_open"), "trusted"),
        ["A"]
    );
    let o = find("tie_desktop_vs_mobile_without_an_existing_kind");
    assert_eq!(ids(&o, "trusted"), ["A", "B", "X"]);
    assert!(o["kinds"].as_array().unwrap().is_empty());
    assert_eq!(ids(&o, "conflicts"), ["X"]);
    let o = find("tie_desktop_vs_mobile_keeps_the_existing_kind");
    assert!(o["kinds"].as_array().unwrap().is_empty());
    assert_eq!(ids(&o, "conflicts"), ["X"]);
    let o = find("m1_revoke_from_base_never_trusts_what_was_untrusted_before");
    assert_eq!(ids(&o, "trusted"), ["A"]);
    assert!(o["kinds"].as_array().unwrap().is_empty());
    let o = find("m1_revoke_from_base_keeps_what_was_trusted_before_without_learning_its_kind");
    assert_eq!(ids(&o, "trusted"), ["A", "B"]);
    assert!(o["kinds"].as_array().unwrap().is_empty());
    let o = find("m1_revoke_from_base_may_lower_a_known_kind");
    assert_eq!(ids(&o, "trusted"), ["A", "B", "X"]);
    assert_eq!(kinds(&o), [("X".to_owned(), "runner".to_owned())]);
}
