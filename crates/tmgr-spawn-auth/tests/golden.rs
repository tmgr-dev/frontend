use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use serde_json::{json, Value};
use tmgr_spawn_auth::*;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/spawn-v1.json");
const NOW: u64 = 1_795_000_100_000;
const ISSUED: u64 = 1_795_000_000_000;

fn key(byte: u8) -> SigningKey {
    SigningKey::from_slice(&[byte; 32]).unwrap()
}

fn pub_bytes(k: &SigningKey) -> Vec<u8> {
    k.verifying_key()
        .to_encoded_point(false)
        .as_bytes()
        .to_vec()
}

fn pub_b64(k: &SigningKey) -> String {
    b64_encode(&pub_bytes(k))
}

fn sign_low(k: &SigningKey, msg: &str) -> Signature {
    let s: Signature = k.sign(msg.as_bytes());
    s.normalize_s().unwrap_or(s)
}

fn high_of(s: &Signature) -> Signature {
    Signature::from_scalars(s.r().to_bytes(), (-*s.s().as_ref()).to_bytes()).unwrap()
}

fn der_b64(s: &Signature) -> String {
    b64_encode(s.to_der().as_bytes())
}

struct Trust {
    trusted: bool,
    revoked: bool,
}

impl TrustView for Trust {
    fn is_trusted(&self, _: &str) -> bool {
        self.trusted
    }
    fn is_revoked(&self, _: &str) -> bool {
        self.revoked
    }
}

#[derive(Clone)]
struct Case {
    name: &'static str,
    wire: SignedSpawn,
    record: ChallengeRecord,
    key: PinnedKey,
    now_ms: u64,
    trusted: bool,
    revoked: bool,
    expect: &'static str,
}

fn proposal() -> SpawnProposal {
    SpawnProposal {
        v: 1,
        profile: "claude".into(),
        cwd: "/work/app".into(),
        mode: SpawnMode::AcceptEdits,
        prompt_sha256: Some(sha256_hex(b"fix the login bug")),
        name: Some("fix \"login\" \u{43f}\u{440}\u{438}\u{432}\u{435}\u{442}".into()),
        task: None,
        target_id: "mx_0123456789abcdef".into(),
    }
}

fn base_record() -> (ChallengeRecord, PinnedKey) {
    let record = ChallengeRecord {
        user_id: "user-1".into(),
        target_id: "mx_0123456789abcdef".into(),
        target_fp: "b".repeat(64),
        card_id: "card-0001".into(),
        nonce: "0123456789abcdef0123456789abcdef".into(),
        issued_ms: ISSUED,
        exp_ms: ISSUED + MAX_TTL_MS,
        req_sha256: proposal().req_sha256(),
    };
    let pinned = PinnedKey {
        user_id: "user-1".into(),
        device_fp: "a".repeat(64),
        spawn_pub: pub_bytes(&key(3)),
    };
    (record, pinned)
}

fn wire_for(r: &ChallengeRecord, k: &PinnedKey, ts_ms: u64, signer: &SigningKey) -> SignedSpawn {
    let sig = sign_low(signer, &signed_string(r, k, ts_ms));
    SignedSpawn {
        v: 1,
        user_id: r.user_id.clone(),
        device_fp: k.device_fp.clone(),
        target_id: r.target_id.clone(),
        target_fp: r.target_fp.clone(),
        card_id: r.card_id.clone(),
        nonce: r.nonce.clone(),
        ts_ms,
        exp_ms: r.exp_ms,
        req_sha256: r.req_sha256.clone(),
        alg: "es256".into(),
        sig: der_b64(&sig),
    }
}

fn cases() -> Vec<Case> {
    let (r, k) = base_record();
    let ts = NOW - 1_000;
    let good = wire_for(&r, &k, ts, &key(3));
    let mk = |name, wire: SignedSpawn, record: ChallengeRecord, expect| Case {
        name,
        wire,
        record,
        key: k.clone(),
        now_ms: NOW,
        trusted: true,
        revoked: false,
        expect,
    };
    let resigned = |name, edit: fn(&mut ChallengeRecord), expect| {
        let mut rec = r.clone();
        edit(&mut rec);
        let w = wire_for(&rec, &k, ts, &key(3));
        mk(name, w, rec, expect)
    };
    let wire_edit = |name, edit: fn(&mut SignedSpawn), expect| {
        let mut w = good.clone();
        edit(&mut w);
        mk(name, w, r.clone(), expect)
    };
    let sig_edit = |name, sig: String, expect| {
        let mut w = good.clone();
        w.sig = sig;
        mk(name, w, r.clone(), expect)
    };

    let low = sign_low(&key(3), &signed_string(&r, &k, ts));
    let der = low.to_der();
    let raw = b64_encode(&low.to_bytes());
    let truncated = b64_encode(&der.as_bytes()[..der.as_bytes().len() - 1]);
    let unpadded = der_b64(&low).trim_end_matches('=').to_string();

    let mut v = vec![
        mk("ok_low_s", good.clone(), r.clone(), "ok"),
        sig_edit("ok_high_s", der_b64(&high_of(&low)), "ok"),
        wire_edit(
            "wrong_field_card_id",
            |w| w.card_id = "card-0002".into(),
            "mismatch",
        ),
        wire_edit(
            "wrong_field_nonce",
            |w| w.nonce = "f".repeat(32),
            "mismatch",
        ),
        wire_edit(
            "wrong_target_id",
            |w| w.target_id = "mx_ffffffffffffffff".into(),
            "mismatch",
        ),
        wire_edit(
            "wrong_target_fp",
            |w| w.target_fp = "c".repeat(64),
            "mismatch",
        ),
        wire_edit("other_user", |w| w.user_id = "user-2".into(), "mismatch"),
        wire_edit(
            "other_device_fp",
            |w| w.device_fp = "d".repeat(64),
            "mismatch",
        ),
        wire_edit(
            "wrong_req_sha256",
            |w| w.req_sha256 = "e".repeat(64),
            "mismatch",
        ),
        wire_edit("wrong_exp_ms", |w| w.exp_ms += 1, "mismatch"),
        wire_edit("unsupported_version", |w| w.v = 2, "unsupported_version"),
        wire_edit(
            "unsupported_alg",
            |w| w.alg = "es384".into(),
            "unsupported_alg",
        ),
        resigned(
            "bad_token_card_id_space",
            |r| r.card_id = "bad card".into(),
            "bad_token",
        ),
        resigned(
            "bad_token_card_id_colon",
            |r| r.card_id = "a:b".into(),
            "bad_token",
        ),
        resigned(
            "bad_token_card_id_65_chars",
            |r| r.card_id = "a".repeat(65),
            "bad_token",
        ),
        resigned(
            "bad_token_nonce_uppercase",
            |r| r.nonce = "A".repeat(32),
            "bad_token",
        ),
        resigned(
            "bad_token_nonce_31_hex",
            |r| r.nonce = "a".repeat(31),
            "bad_token",
        ),
        resigned(
            "bad_token_hash_63_hex",
            |r| r.req_sha256 = "a".repeat(63),
            "bad_token",
        ),
        resigned(
            "bad_token_hash_65_hex",
            |r| r.req_sha256 = "a".repeat(65),
            "bad_token",
        ),
        resigned(
            "bad_token_hash_uppercase",
            |r| r.req_sha256 = "A".repeat(64),
            "bad_token",
        ),
        resigned(
            "bad_ttl_over_5_min",
            |r| r.exp_ms = r.issued_ms + MAX_TTL_MS + 1,
            "bad_ttl",
        ),
        sig_edit(
            "bad_sig_by_non_pinned_key",
            der_b64(&sign_low(&key(9), &signed_string(&r, &k, ts))),
            "bad_sig",
        ),
        sig_edit("bad_sig_non_der_raw_r_s", raw, "bad_sig"),
        sig_edit("bad_sig_non_der_garbage", b64_encode(&[1u8; 70]), "bad_sig"),
        sig_edit("bad_sig_truncated_der", truncated, "bad_sig"),
        sig_edit(
            "bad_sig_wrong_length_8_bytes",
            b64_encode(&[0x30; 8]),
            "bad_sig",
        ),
        sig_edit("bad_sig_empty", String::new(), "bad_sig"),
        sig_edit("bad_sig_base64_unpadded", unpadded, "bad_sig"),
        sig_edit(
            "bad_sig_base64_trailing_newline",
            format!("{}\n", der_b64(&low)),
            "bad_sig",
        ),
        sig_edit(
            "bad_sig_base64_inner_space",
            format!(" {}", der_b64(&low)),
            "bad_sig",
        ),
    ];

    let mut expired = mk("expired", good.clone(), r.clone(), "expired");
    expired.now_ms = r.exp_ms + 1;
    v.push(expired);
    let mut edge = mk(
        "ok_now_equals_exp",
        wire_for(&r, &k, r.exp_ms - 1_000, &key(3)),
        r.clone(),
        "ok",
    );
    edge.now_ms = r.exp_ms;
    v.push(edge);

    let mut future = mk(
        "ts_in_future_beyond_5_min",
        wire_for(&r, &k, NOW + MAX_SKEW_MS + 1, &key(3)),
        r.clone(),
        "clock_skew",
    );
    future.now_ms = NOW;
    v.push(future);
    v.push(mk(
        "ok_ts_in_future_exactly_5_min",
        wire_for(&r, &k, NOW + MAX_SKEW_MS, &key(3)),
        r.clone(),
        "ok",
    ));
    v.push(mk(
        "ts_in_past_beyond_5_min",
        wire_for(&r, &k, NOW - MAX_SKEW_MS - 1, &key(3)),
        r.clone(),
        "clock_skew",
    ));

    let mut untrusted = mk("device_not_trusted", good.clone(), r.clone(), "not_trusted");
    untrusted.trusted = false;
    v.push(untrusted);
    let mut revoked = mk("device_revoked", good.clone(), r.clone(), "not_trusted");
    revoked.revoked = true;
    v.push(revoked);

    let mut short_key = mk(
        "unknown_key_64_bytes",
        good.clone(),
        r.clone(),
        "unknown_key",
    );
    short_key.key.spawn_pub.truncate(64);
    v.push(short_key);
    let mut off_curve = mk(
        "unknown_key_off_curve",
        good.clone(),
        r.clone(),
        "unknown_key",
    );
    off_curve.key.spawn_pub = vec![4; 65];
    v.push(off_curve);

    for c in &v {
        let t = Trust {
            trusted: c.trusted,
            revoked: c.revoked,
        };
        let got = match verify(&c.wire, &c.record, &c.key, &t, c.now_ms) {
            Ok(_) => "ok",
            Err(e) => e.code(),
        };
        assert_eq!(got, c.expect, "case {}", c.name);
    }
    v
}

struct StatementCase {
    name: &'static str,
    user_id: &'static str,
    statement: String,
    sig: String,
    spawn_sig: String,
    device_id: &'static str,
    trusted: bool,
    revoked: bool,
    expect: &'static str,
}

fn statement_cases() -> (Value, Vec<StatementCase>) {
    let dev = key(5);
    let spawn = key(3);
    let kex = key(7);
    let sign_pub = pub_b64(&dev);
    let kex_pub = pub_b64(&kex);
    let fp = device_fp(&sign_pub, &kex_pub);
    let spawn_pub = pub_b64(&spawn);
    let stmt = build_spawn_key_statement(
        "user-1",
        &fp,
        "device-uuid-1",
        &spawn_pub,
        1_795_000_000_000,
    );
    let sig = |k: &SigningKey, m: &str| der_b64(&sign_low(k, m));
    let mk = |name, statement: String, sig_b: String, spawn_sig: String, expect| StatementCase {
        name,
        user_id: "user-1",
        statement,
        sig: sig_b,
        spawn_sig,
        device_id: "device-uuid-1",
        trusted: true,
        revoked: false,
        expect,
    };
    let both = |s: &str| (sig(&dev, s), sig(&spawn, s));
    let (a, b) = both(&stmt);
    let other_user = stmt.replacen(":user-1:", ":user-2:", 1);
    let (ou_a, ou_b) = both(&other_user);
    let leading = stmt.replace(":1795000000000", ":01795000000000");
    let (l_a, l_b) = both(&leading);
    let mut v = vec![
        mk("ok", stmt.clone(), a.clone(), b.clone(), "ok"),
        mk("wrong_user", other_user, ou_a, ou_b, "mismatch"),
        mk("leading_zero_issued_at", leading, l_a, l_b, "bad_token"),
        mk(
            "spawn_sig_by_other_key",
            stmt.clone(),
            a.clone(),
            sig(&key(9), &stmt),
            "bad_sig",
        ),
        mk(
            "device_sig_by_other_key",
            stmt.clone(),
            sig(&key(9), &stmt),
            b.clone(),
            "bad_sig",
        ),
        mk(
            "device_sig_missing_proof_of_possession",
            stmt.clone(),
            a.clone(),
            a.clone(),
            "bad_sig",
        ),
    ];
    let mut other_device = mk(
        "wrong_device_id",
        stmt.clone(),
        a.clone(),
        b.clone(),
        "mismatch",
    );
    other_device.device_id = "device-uuid-2";
    v.push(other_device);
    let mut untrusted = mk(
        "device_not_trusted",
        stmt.clone(),
        a.clone(),
        b.clone(),
        "not_trusted",
    );
    untrusted.trusted = false;
    v.push(untrusted);
    let mut revoked = mk("device_revoked", stmt.clone(), a, b, "not_trusted");
    revoked.revoked = true;
    v.push(revoked);

    for c in &v {
        let d = DeviceKeys {
            device_id: c.device_id,
            sign_pub_b64: &sign_pub,
            kex_pub_b64: &kex_pub,
        };
        let t = Trust {
            trusted: c.trusted,
            revoked: c.revoked,
        };
        let got =
            match verify_spawn_key_statement(c.user_id, &c.statement, &c.sig, &c.spawn_sig, &d, &t)
            {
                Ok(_) => "ok",
                Err(e) => e.code(),
            };
        assert_eq!(got, c.expect, "statement case {}", c.name);
    }
    let head = json!({
        "test_device_sign_secret_hex": hex::encode([5u8; 32]),
        "test_device_sign_pub_b64": sign_pub,
        "test_device_kex_secret_hex": hex::encode([7u8; 32]),
        "test_device_kex_pub_b64": kex_pub,
        "device_id": "device-uuid-1",
        "device_fp": fp,
        "test_spawn_secret_hex": hex::encode([3u8; 32]),
        "test_spawn_pub_b64": spawn_pub,
        "fp_input": format!("tmgr-chat-fp:v1:{}:{}", pub_b64(&dev), pub_b64(&kex)),
    });
    (head, v)
}

fn generate() -> Value {
    let (record, pinned) = base_record();
    let p = proposal();
    let canonical = p.canonical_bytes();
    let ts = NOW - 1_000;
    let msg = signed_string(&record, &pinned, ts);
    let low = sign_low(&key(3), &msg);
    let (stmt_head, stmt_cases) = statement_cases();
    let verify_cases: Vec<Value> = cases()
        .iter()
        .map(|c| {
            json!({
                "name": c.name,
                "now_ms": c.now_ms,
                "device_trusted": c.trusted,
                "device_revoked": c.revoked,
                "record": {
                    "user_id": c.record.user_id, "target_id": c.record.target_id,
                    "target_fp": c.record.target_fp, "card_id": c.record.card_id,
                    "nonce": c.record.nonce, "issued_ms": c.record.issued_ms,
                    "exp_ms": c.record.exp_ms, "req_sha256": c.record.req_sha256,
                },
                "pinned_key": {
                    "user_id": c.key.user_id, "device_fp": c.key.device_fp,
                    "spawn_pub_b64": b64_encode(&c.key.spawn_pub),
                },
                "wire": serde_json::to_value(&c.wire).unwrap(),
                "expect": c.expect,
            })
        })
        .collect();
    let statement_cases: Vec<Value> = stmt_cases
        .iter()
        .map(|c| {
            json!({
                "name": c.name, "expected_user_id": c.user_id, "device_id": c.device_id,
                "device_trusted": c.trusted, "device_revoked": c.revoked,
                "statement": c.statement, "sig": c.sig, "spawn_sig": c.spawn_sig,
                "expect": c.expect,
            })
        })
        .collect();
    json!({
        "doc": "SignedSpawn v1 golden vectors, regenerated by `cargo test -p tmgr-spawn-auth -- --ignored regenerate`. All keys are TEST-ONLY (test_* fields). Secrets are 32 bytes of one repeated byte; P-256; public keys are 65-byte uncompressed SEC1, base64 STANDARD padded. Signatures are DER ECDSA P-256 over SHA-256 of the UTF-8 message, base64 STANDARD padded and canonical (re-encoding must reproduce the string). A verifier must accept high-S. `proposal.fields` serialize to `proposal.canonical_utf8` in exactly the declared field order, no spaces, None omitted, serde_json escaping (only quote, backslash and control chars are escaped; non-ASCII stays raw UTF-8); `req_sha256` is lowercase hex SHA-256 of those bytes. `signed.message` is the exact signed string. `verify_cases[]`: given record (the machine's stored challenge), pinned_key, wire (the SignedSpawn JSON), now_ms and whether the device is trusted/revoked, verify must return `expect` (`ok` or a refusal code: mismatch, unsupported_version, unsupported_alg, bad_ttl, expired, clock_skew, bad_token, unknown_key, not_trusted, bad_sig). Check order: field mismatch, version, alg, ttl, expiry, skew, tokens, key, trust, signature. `statement_cases[]`: a tmgr-spawn-key:v1 statement must carry two valid signatures (device chat sign key `sig`, spawn key `spawn_sig`) and the device must be trusted and not revoked; `expect` as above.",
        "test_keys": {
            "test_spawn_secret_hex": hex::encode([3u8; 32]),
            "test_spawn_pub_b64": pub_b64(&key(3)),
            "test_other_secret_hex": hex::encode([9u8; 32]),
            "test_other_pub_b64": pub_b64(&key(9)),
        },
        "proposal": {
            "fields": serde_json::to_value(&p).unwrap(),
            "canonical_utf8": String::from_utf8(canonical.clone()).unwrap(),
            "canonical_b64": b64_encode(&canonical),
            "req_sha256": p.req_sha256(),
        },
        "signed": {
            "user_id": record.user_id, "device_fp": pinned.device_fp,
            "target_id": record.target_id, "target_fp": record.target_fp,
            "card_id": record.card_id, "nonce": record.nonce,
            "ts_ms": ts, "exp_ms": record.exp_ms, "req_sha256": record.req_sha256,
            "message": msg,
            "sig_low_s_b64": der_b64(&low),
            "sig_high_s_b64": der_b64(&high_of(&low)),
        },
        "statement": stmt_head,
        "statement_vector": {
            "statement": statement_cases[0]["statement"],
            "sig": statement_cases[0]["sig"],
            "spawn_sig": statement_cases[0]["spawn_sig"],
        },
        "verify_cases": verify_cases,
        "statement_cases": statement_cases,
    })
}

#[test]
fn fixture_matches_generator() {
    let on_disk: Value = serde_json::from_str(&std::fs::read_to_string(FIXTURE).unwrap()).unwrap();
    assert_eq!(
        on_disk,
        generate(),
        "run: cargo test -p tmgr-spawn-auth -- --ignored regenerate"
    );
}

#[test]
fn fixture_cases_verify() {
    let f: Value = serde_json::from_str(&std::fs::read_to_string(FIXTURE).unwrap()).unwrap();
    let s = |v: &Value, k: &str| v[k].as_str().unwrap().to_string();
    let n = |v: &Value, k: &str| v[k].as_u64().unwrap();
    let mut seen = 0;
    for c in f["verify_cases"].as_array().unwrap() {
        let rec = &c["record"];
        let record = ChallengeRecord {
            user_id: s(rec, "user_id"),
            target_id: s(rec, "target_id"),
            target_fp: s(rec, "target_fp"),
            card_id: s(rec, "card_id"),
            nonce: s(rec, "nonce"),
            issued_ms: n(rec, "issued_ms"),
            exp_ms: n(rec, "exp_ms"),
            req_sha256: s(rec, "req_sha256"),
        };
        let pk = &c["pinned_key"];
        let pinned = PinnedKey {
            user_id: s(pk, "user_id"),
            device_fp: s(pk, "device_fp"),
            spawn_pub: b64_decode_strict(pk["spawn_pub_b64"].as_str().unwrap()).unwrap(),
        };
        let wire: SignedSpawn = serde_json::from_value(c["wire"].clone()).unwrap();
        let t = Trust {
            trusted: c["device_trusted"].as_bool().unwrap(),
            revoked: c["device_revoked"].as_bool().unwrap(),
        };
        let got = match verify(&wire, &record, &pinned, &t, n(c, "now_ms")) {
            Ok(_) => "ok",
            Err(e) => e.code(),
        };
        assert_eq!(got, c["expect"].as_str().unwrap(), "{}", c["name"]);
        seen += 1;
    }
    assert!(seen >= 30);
}

#[test]
fn fixture_signed_string_and_hash() {
    let f: Value = serde_json::from_str(&std::fs::read_to_string(FIXTURE).unwrap()).unwrap();
    let p: SpawnProposal = serde_json::from_value(f["proposal"]["fields"].clone()).unwrap();
    assert_eq!(
        p.canonical_bytes(),
        b64_decode_strict(f["proposal"]["canonical_b64"].as_str().unwrap()).unwrap()
    );
    assert_eq!(
        p.req_sha256(),
        f["proposal"]["req_sha256"].as_str().unwrap()
    );
    assert_eq!(f["signed"]["req_sha256"], f["proposal"]["req_sha256"]);
}

#[test]
#[ignore]
fn regenerate() {
    let mut text = serde_json::to_string_pretty(&generate()).unwrap();
    text.push('\n');
    std::fs::write(FIXTURE, text).unwrap();
}
