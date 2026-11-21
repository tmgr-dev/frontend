use serde_json::{json, Value};
use tmgr_chat_relay::keys::DeviceKeys;
use tmgr_chat_relay::trust::{
    decode_join_code, encode_join_code, join_mac, sign_intro, sign_revoke, sign_spawnkey,
    trusted_set, verify_intro, verify_join_mac, verify_revoke, verify_spawnkey, DirEntry,
    IntroStatement, JoinCode, JoinMacInput, RevokeStatement, SpawnStatement,
};
use tmgr_chat_relay::wire::{
    chat_ctx, msg_id_from, open_message, seal_message_unpadded_with, seal_message_with,
    signed_string, Body, Inner, Peer, SealParams,
};
use tmgr_phone::{b64, h, hex, open_ctx, public_key, Entropy, Sealed};

const USER: u64 = 42;
const NOW: u64 = 1_800_000_000_000;
const SENT_AT: u64 = 1_799_999_990_000;
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
    let (sender_sign, sender_kex) = (sender.sign_pub_b64(), sender.kex_pub_b64());
    let opened = open_message(&recipient, USER, IDS[1], IDS[0], msg_id, &body, NOW, |_| {
        Some(Peer {
            device_id: IDS[0],
            sign_pub_b64: &sender_sign,
            kex_pub_b64: &sender_kex,
        })
    })
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

fn intro_statement(signer: &DeviceKeys, target: &DeviceKeys, target_id: &str) -> IntroStatement {
    IntroStatement {
        user_id: USER,
        introducer_fp: signer.fingerprint(),
        target_id: target_id.into(),
        target_fp: target.fingerprint(),
        target_kind: "mobile".into(),
        issued_at_ms: NOW,
    }
}

fn intro_fixture() -> Value {
    let (p, n) = (keys(1), keys(2));
    let st = intro_statement(&p, &n, IDS[1]);
    let statement = st.to_statement().unwrap();
    let json_str = sign_intro(&p, IDS[0], &st).unwrap();
    verify_intro(&json_str, &p.sign_pub_b64()).unwrap();
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
    verify_revoke(&json_str, &r.sign_pub_b64()).unwrap();
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

fn join_fixture() -> Value {
    let (p, n) = (keys(1), keys(2));
    let secret = [0x5au8; 32];
    let code = JoinCode {
        user_id: USER,
        device_id: IDS[0].into(),
        fp: p.fingerprint(),
        secret,
        exp_ms: NOW + 600_000,
    };
    let encoded = encode_join_code(&code).unwrap();
    assert_eq!(decode_join_code(&encoded).unwrap(), code);
    let (pf, nf) = (p.fingerprint(), n.fingerprint());
    let input = JoinMacInput {
        user_id: USER,
        p_fp: &pf,
        n_id: IDS[1],
        n_fp: &nf,
        n_kind: "mobile",
        label: "Phone",
    };
    let mac = join_mac(&secret, &input).unwrap();
    verify_join_mac(&secret, &input, &mac).unwrap();
    let raw = encoded.strip_prefix("tmgr-chat-join:v1:").unwrap();
    json!({
        "description": "join code, MAC key is the 32 decoded secret bytes",
        "inputs": {
            "code": {
                "user_id": USER,
                "device_id": IDS[0],
                "fp": pf,
                "secret_hex": hex(&secret),
                "secret_b64": b64(&secret),
                "exp_ms": NOW + 600_000,
            },
            "joiner": {"device_id": IDS[1], "kind": "mobile", "label": "Phone", "fp": nf},
        },
        "outputs": {
            "code_string": encoded,
            "code_base64url": raw,
            "mac_transcript": format!(
                "tmgr-chat-join:v1:{USER}:{pf}:{}:{nf}:mobile:{}",
                IDS[1],
                h("Phone")
            ),
            "mac_hex": mac,
        },
    })
}

fn spawnkey_fixture() -> Value {
    let dev = keys(1);
    let spawn_secret = [0x77u8; 32];
    let spawn_pub = b64(&public_key(&spawn_secret).unwrap());
    let st = SpawnStatement {
        user_id: USER,
        device_fp: dev.fingerprint(),
        device_id: IDS[0].into(),
        spawn_pub_b64: spawn_pub.clone(),
        issued_at_ms: NOW,
    };
    let k = sign_spawnkey(&dev, &spawn_secret, &st).unwrap();
    let payload = serde_json::to_string(&k).unwrap();
    verify_spawnkey(&payload, IDS[0], &dev.sign_pub_b64(), &dev.kex_pub_b64()).unwrap();
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
    let pin_pairs: Vec<(String, String)> = pins
        .iter()
        .map(|(i, k)| (i.to_string(), k.fingerprint()))
        .collect();
    let revoked_fps: Vec<String> = revoked.iter().map(|k| k.fingerprint()).collect();
    let dir: Vec<DirEntry> = directory
        .iter()
        .map(|(i, k)| DirEntry {
            device_id: i.to_string(),
            sign_pub_b64: k.sign_pub_b64(),
            kex_pub_b64: k.kex_pub_b64(),
        })
        .collect();
    let expected = trusted_set(&pin_pairs, &revoked_fps, &intros, &dir);
    json!({
        "name": name,
        "note": note,
        "inputs": {
            "pins": pin_pairs,
            "revoked": revoked_fps,
            "intros": intros,
            "directory": dir_json(directory),
        },
        "outputs": {"trusted": expected},
    })
}

fn intro_by(by: (&str, &DeviceKeys), target: (&str, &DeviceKeys)) -> String {
    sign_intro(by.1, by.0, &intro_statement(by.1, target.1, target.0)).unwrap()
}

fn trust_fixture() -> Value {
    let (a, b, c, evil) = (keys(1), keys(2), keys(3), keys(66));
    let full = [(IDS[0], &a), (IDS[1], &b), (IDS[2], &c)];
    let pin_a = [(IDS[0], &a)];
    let cases = vec![
        trust_case(
            "chain",
            "A pinned; A introduces B; B introduces C; intros listed child first",
            &pin_a,
            &[],
            vec![intro_by((IDS[1], &b), (IDS[2], &c)), intro_by((IDS[0], &a), (IDS[1], &b))],
            &full,
        ),
        trust_case(
            "revoked_hub_cascade",
            "B revoked; C only had an intro from B, so C drops out",
            &pin_a,
            &[&b],
            vec![intro_by((IDS[0], &a), (IDS[1], &b)), intro_by((IDS[1], &b), (IDS[2], &c))],
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
            "mismatched_directory_keys",
            "directory serves other keys for A, so A's pinned fp does not match: A's intros are not verified",
            &pin_a,
            &[],
            vec![intro_by((IDS[0], &a), (IDS[1], &b)), intro_by((IDS[0], &evil), (IDS[3], &keys(4)))],
            &[(IDS[0], &evil), (IDS[1], &b)],
        ),
        trust_case(
            "withheld_intros_cached",
            "server serves no intros; locally cached intros keep B and C trusted",
            &pin_a,
            &[],
            vec![intro_by((IDS[0], &a), (IDS[1], &b)), intro_by((IDS[1], &b), (IDS[2], &c))],
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
        "description": "trusted_set fixpoint; every intro is a signed intro json string; result is sorted by (id, fp)",
        "cases": cases,
    })
}

fn all() -> Vec<(&'static str, Value)> {
    vec![
        ("chat", chat_fixture()),
        ("blob", blob_fixture()),
        ("intro", intro_fixture()),
        ("revoke", revoke_fixture()),
        ("join", join_fixture()),
        ("spawnkey", spawnkey_fixture()),
        ("trust", trust_fixture()),
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

#[test]
fn committed_chat_and_blob_bodies_open_from_inputs_only() {
    for name in ["chat", "blob"] {
        let f = read(name);
        let i = &f["inputs"];
        let hex_bytes = |s: &Value| -> Vec<u8> {
            let s = s.as_str().unwrap();
            (0..s.len())
                .step_by(2)
                .map(|n| u8::from_str_radix(&s[n..n + 2], 16).unwrap())
                .collect()
        };
        let rk = &i["recipient"]["keys"];
        let recipient = DeviceKeys::from_secrets(
            &hex_bytes(&rk["sign_secret_hex"]),
            &hex_bytes(&rk["kex_secret_hex"]),
        )
        .unwrap();
        let sk = &i["sender"]["keys"];
        let sign_pub = sk["sign_pub"].as_str().unwrap();
        let kex_pub = sk["kex_pub"].as_str().unwrap();
        for body_key in ["body", "body_unpadded"] {
            let o = open_message(
                &recipient,
                i["user_id"].as_u64().unwrap(),
                i["recipient"]["device_id"].as_str().unwrap(),
                i["sender"]["device_id"].as_str().unwrap(),
                i["msg_id"].as_str().unwrap(),
                f["outputs"][body_key].as_str().unwrap(),
                i["now_ms"].as_u64().unwrap(),
                |_| {
                    Some(Peer {
                        device_id: i["sender"]["device_id"].as_str().unwrap(),
                        sign_pub_b64: sign_pub,
                        kex_pub_b64: kex_pub,
                    })
                },
            )
            .unwrap();
            assert_eq!(o.seq, i["seq"].as_u64().unwrap());
            assert_eq!(o.payload, i["payload"]);
            assert_eq!(o.from_fp, sk["fp"].as_str().unwrap());
        }
        assert_eq!(f["outputs"]["plaintext_len"], 1024);
    }
}
