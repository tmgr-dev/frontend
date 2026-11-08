use p256::ecdsa::Signature;
use serde_json::{json, Value};
use tmgr_phone::*;

const FIXTURE_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/v1.json");
const HOST: &str = "tmgr.dev";
const USER_ID: i64 = 42;
const MAILBOX: &str = "0b9f6c1e-3d52-4c47-9a6e-5f1d2a7c8e90";
const PAIR_SECRET: &str = "a1a2a3a4a5a6a7a8a9aaabacadaeafb0b1b2b3b4b5b6b7b8b9babbbcbdbebfc0";
const NOW: i64 = 1_790_000_000_000;

struct Party {
    sign_secret: String,
    kex_secret: String,
    sign_pub: Vec<u8>,
    kex_pub: Vec<u8>,
    fp: String,
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
        .collect()
}

fn party(sign_hex: &str, kex_hex: &str) -> Party {
    let sign_pub = public_key(&unhex(sign_hex)).unwrap();
    let kex_pub = public_key(&unhex(kex_hex)).unwrap();
    Party {
        fp: fingerprint(&sign_pub, &kex_pub),
        sign_secret: sign_hex.into(),
        kex_secret: kex_hex.into(),
        sign_pub,
        kex_pub,
    }
}

fn desktop() -> Party {
    party(&"11".repeat(32), &"22".repeat(32))
}

fn phone() -> Party {
    party(&"33".repeat(32), &"44".repeat(32))
}

fn ent(n: u8) -> Entropy {
    Entropy {
        eph_secret: [n; 32],
        nonce: [n; 12],
    }
}

fn pairing<'a>(d: &'a Party, p: &'a Party) -> Pairing<'a> {
    Pairing {
        mailbox: MAILBOX,
        desktop_fp: &d.fp,
        phone_fp: &p.fp,
    }
}

fn hello_ctx(d: &Party) -> HelloCtx<'_> {
    HelloCtx {
        host: HOST,
        user_id: USER_ID,
        mailbox: MAILBOX,
        desktop_fp: &d.fp,
    }
}

const ANSWER_ID: &str = "00112233445566778899aabbccddeeff";
const CARD_PROJECTION: &str = "{\"p\":1,\"kind\":\"question\",\"title\":\"Ship it?\",\"rev\":\"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\"}";

fn answer_body() -> AnswerBody {
    AnswerBody {
        card_id: "card-7".into(),
        card_digest: digest(CARD_PROJECTION),
        session: "runner:abc".into(),
        answer_id: ANSWER_ID.into(),
        issued_at: NOW,
        action: Action::Answer,
        choice: Some("yes".into()),
        text: Some("go ahead".into()),
    }
}

fn snapshot_body() -> SnapshotBody {
    SnapshotBody {
        v: 1,
        seq: NOW + 1,
        generated_at: NOW,
        desktop_name: "Studio Mac".into(),
        cards: vec![SnapshotCard {
            id: "card-7".into(),
            digest: digest(CARD_PROJECTION),
            state: "open".into(),
            session_gone: false,
            projection: CARD_PROJECTION.into(),
        }],
        more: 0,
    }
}

fn ack_body() -> AckBody {
    AckBody {
        answer_id: ANSWER_ID.into(),
        result: AckResult::Applied,
        at: NOW + 1000,
        winner: None,
        label: Some("Yes".into()),
    }
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or_else(|| panic!("missing {k}"))
}

fn args_for(
    recipient: &Party,
    signer_pub: &[u8],
    sealed: &str,
    mailbox: &str,
    d: &Party,
    p: &Party,
) -> Value {
    json!({
        "recipient_kex_secret_hex": recipient.kex_secret,
        "signer_sign_pub": b64(signer_pub),
        "mailbox": mailbox,
        "desktop_fp": d.fp,
        "phone_fp": p.fp,
        "host": HOST,
        "user_id": USER_ID,
        "pair_secret_hex": PAIR_SECRET,
        "sealed": sealed,
    })
}

fn run_op(op: &str, a: &Value) -> Result<Value> {
    let secret = unhex(s(a, "recipient_kex_secret_hex"));
    let signer = b64_decode(s(a, "signer_sign_pub")).unwrap();
    let sealed = s(a, "sealed");
    let p = Pairing {
        mailbox: s(a, "mailbox"),
        desktop_fp: s(a, "desktop_fp"),
        phone_fp: s(a, "phone_fp"),
    };
    let to = |r: Result<_>| r.map(|v: Value| v);
    match op {
        "open_answer" => {
            to(open_answer(&secret, &p, &signer, sealed).map(|b| serde_json::to_value(b).unwrap()))
        }
        "open_snapshot" => {
            to(open_snapshot(&secret, &p, &signer, sealed)
                .map(|b| serde_json::to_value(b).unwrap()))
        }
        "open_ack" => {
            to(open_ack(&secret, &p, &signer, sealed).map(|b| serde_json::to_value(b).unwrap()))
        }
        "open_pair_ok" => to(
            open_pair_ok(&secret, &p, &signer, sealed).map(|b| serde_json::to_value(b).unwrap())
        ),
        "open_hello" => {
            let c = HelloCtx {
                host: s(a, "host"),
                user_id: a["user_id"].as_i64().unwrap(),
                mailbox: s(a, "mailbox"),
                desktop_fp: s(a, "desktop_fp"),
            };
            open_hello(&secret, &c, &unhex(s(a, "pair_secret_hex")), sealed)
                .map(|o| json!({"body": o.body, "phone_fp": o.phone_fp, "code": o.code}))
        }
        other => panic!("unknown op {other}"),
    }
}

fn high_s_der(msg: &str, secret: &[u8]) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let der = b64_decode(&sign(secret, msg).unwrap()).unwrap();
    let sig = Signature::from_der(&der).unwrap();
    let (r, sv) = sig.split_scalars();
    let high = Signature::from_scalars(r.to_bytes(), (-*sv).to_bytes()).unwrap();
    assert!(high.normalize_s().is_some());
    (
        sig.to_bytes().to_vec(),
        der,
        high.to_der().as_bytes().to_vec(),
    )
}

fn flip(sealed: &str, field: &str) -> String {
    let mut v: Value = serde_json::from_str(sealed).unwrap();
    let mut ct = b64_decode(s(&v, field)).unwrap();
    ct[0] ^= 1;
    v[field] = json!(b64(&ct));
    v.to_string()
}

fn set(sealed: &str, field: &str, val: &str) -> String {
    let mut v: Value = serde_json::from_str(sealed).unwrap();
    v[field] = json!(val);
    v.to_string()
}

fn forged_answer_card_swap(d: &Party, p: &Party) -> String {
    let pr = pairing(d, p);
    let original = answer_body();
    let sig = sign(&unhex(&p.sign_secret), &answer_signed(&pr, &original)).unwrap();
    let mut swapped = original.clone();
    swapped.card_id = "card-8".into();
    let plaintext = json!({"body": serde_json::to_string(&swapped).unwrap(), "sig": sig});
    let sealed = seal_with(
        &d.kex_pub,
        MAILBOX,
        Dir::P2d,
        Kind::Answer,
        ANSWER_ID,
        plaintext.to_string().as_bytes(),
        &ent(9),
    )
    .unwrap();
    serde_json::to_string(&sealed).unwrap()
}

fn build_fixture() -> Value {
    let d = desktop();
    let p = phone();
    let pr = pairing(&d, &p);
    let hc = hello_ctx(&d);
    let pair_secret = unhex(PAIR_SECRET);

    let party_json = |x: &Party| {
        json!({
            "sign_secret_hex": x.sign_secret,
            "kex_secret_hex": x.kex_secret,
            "sign_pub": b64(&x.sign_pub),
            "kex_pub": b64(&x.kex_pub),
            "fp": x.fp,
        })
    };

    let seal_plain = r#"{"hello":"world"}"#;
    let sealed_vec = seal_with(
        &p.kex_pub,
        MAILBOX,
        Dir::D2p,
        Kind::Snapshot,
        "12",
        seal_plain.as_bytes(),
        &ent(5),
    )
    .unwrap();

    let hello = HelloBody::build(
        &hc,
        &pair_secret,
        &p.sign_pub,
        &p.kex_pub,
        "Alice iPhone",
        "tok-next-0123456789",
    )
    .unwrap();
    let hello_text = serde_json::to_string(&hello).unwrap();
    let transcript = pair_transcript(
        &hc,
        &hello.sign_pub,
        &hello.kex_pub,
        &hello.name,
        &hello.next_token,
    );
    let hello_sealed =
        seal_hello_with(&d.kex_pub, &hc, &unhex(&p.sign_secret), &hello, &ent(1)).unwrap();
    let bad_mac_hello = {
        let other = HelloBody::build(
            &hc,
            &unhex(&"ee".repeat(32)),
            &p.sign_pub,
            &p.kex_pub,
            "Alice iPhone",
            "tok-next-0123456789",
        )
        .unwrap();
        seal_hello_with(&d.kex_pub, &hc, &unhex(&p.sign_secret), &other, &ent(1)).unwrap()
    };

    let pair_ok = PairOkBody {
        desktop_name: "Studio Mac".into(),
        paired_at: NOW,
    };
    let pair_ok_text = serde_json::to_string(&pair_ok).unwrap();
    let pair_ok_sealed =
        seal_pair_ok_with(&p.kex_pub, &pr, &unhex(&d.sign_secret), &pair_ok, &ent(2)).unwrap();

    let answer = answer_body();
    let answer_text = serde_json::to_string(&answer).unwrap();
    let answer_sealed =
        seal_answer_with(&d.kex_pub, &pr, &unhex(&p.sign_secret), &answer, &ent(3)).unwrap();

    let snap = snapshot_body();
    let snap_text = serde_json::to_string(&snap).unwrap();
    let snap_sealed =
        seal_snapshot_with(&p.kex_pub, &pr, &unhex(&d.sign_secret), &snap, &ent(4)).unwrap();

    let ack = ack_body();
    let ack_text = serde_json::to_string(&ack).unwrap();
    let ack_sealed = seal_ack_with(&p.kex_pub, &pr, &unhex(&d.sign_secret), &ack, &ent(6)).unwrap();

    let sig_msg = "tmgr-phone signature vector";
    let (raw, der, high) = high_s_der(sig_msg, &unhex(&p.sign_secret));
    let sig_ok = json!({"sign_pub": b64(&p.sign_pub), "message": sig_msg});

    let op_answer = |recipient: &Party, signer: &[u8], sealed: &str, mailbox: &str| {
        args_for(recipient, signer, sealed, mailbox, &d, &p)
    };
    let neg = |name: &str, why: &str, op: &str, args: Value, err: &str| json!({"name": name, "description": why, "op": op, "args": args, "expect_error": err});
    let valid_answer_args = |sealed: &str| op_answer(&d, &p.sign_pub, sealed, MAILBOX);

    let negative = vec![
        neg(
            "wrong_recipient_key",
            "answer opened with the phone's key-exchange secret instead of the desktop's",
            "open_answer",
            op_answer(&p, &p.sign_pub, &answer_sealed, MAILBOX),
            "open_failed",
        ),
        neg(
            "tampered_ciphertext",
            "first byte of ct flipped",
            "open_answer",
            valid_answer_args(&flip(&answer_sealed, "ct")),
            "open_failed",
        ),
        neg(
            "swapped_outer_id",
            "outer id replaced by another well-formed answer id; ctx and AAD no longer match",
            "open_answer",
            valid_answer_args(&set(&answer_sealed, "id", &"ab".repeat(16))),
            "open_failed",
        ),
        neg(
            "swapped_outer_kind",
            "outer kind replaced by snapshot, which the receiver does not accept in p2d",
            "open_answer",
            valid_answer_args(&set(&answer_sealed, "kind", "snapshot")),
            "unexpected_kind",
        ),
        neg(
            "wrong_mailbox_ctx",
            "receiver's own mailbox differs from the one the message was sealed for",
            "open_answer",
            op_answer(
                &d,
                &p.sign_pub,
                &answer_sealed,
                "ffffffff-3d52-4c47-9a6e-5f1d2a7c8e90",
            ),
            "open_failed",
        ),
        neg(
            "wrong_signer_key",
            "signature verified against the desktop's sign key instead of the pinned phone key",
            "open_answer",
            op_answer(&d, &d.sign_pub, &answer_sealed, MAILBOX),
            "bad_signature",
        ),
        neg(
            "answer_for_swapped_card_id",
            "body names card-8 but the signature covers card-7; outer id is intact",
            "open_answer",
            valid_answer_args(&forged_answer_card_swap(&d, &p)),
            "bad_signature",
        ),
        neg(
            "bad_pair_mac",
            "hello whose mac was computed with a different pair secret; self-signature is valid",
            "open_hello",
            args_for(&d, &p.sign_pub, &bad_mac_hello, MAILBOX, &d, &p),
            "bad_mac",
        ),
    ];

    let pos = |name: &str, op: &str, args: Value, expect: Value| json!({"name": name, "op": op, "args": args, "expect": expect});
    let positive = vec![
        pos(
            "hello",
            "open_hello",
            args_for(&d, &p.sign_pub, &hello_sealed, MAILBOX, &d, &p),
            json!({"body": hello, "phone_fp": p.fp, "code": pair_code(&transcript)}),
        ),
        pos(
            "pair_ok",
            "open_pair_ok",
            args_for(&p, &d.sign_pub, &pair_ok_sealed, MAILBOX, &d, &p),
            serde_json::to_value(&pair_ok).unwrap(),
        ),
        pos(
            "snapshot",
            "open_snapshot",
            args_for(&p, &d.sign_pub, &snap_sealed, MAILBOX, &d, &p),
            serde_json::to_value(&snap).unwrap(),
        ),
        pos(
            "answer",
            "open_answer",
            valid_answer_args(&answer_sealed),
            serde_json::to_value(&answer).unwrap(),
        ),
        pos(
            "ack",
            "open_ack",
            args_for(&p, &d.sign_pub, &ack_sealed, MAILBOX, &d, &p),
            serde_json::to_value(&ack).unwrap(),
        ),
    ];

    json!({
        "protocol": "tmgr-phone v1",
        "about": "Shared vectors for crates/tmgr-phone and the mobile JS mirror. Binary values are standard base64 with padding, hex is lowercase, H(x) is lowercase hex SHA-256 of UTF-8. Every `op` vector in positive/negative takes `args` and either returns `expect` (positive) or fails with the stable error code in `expect_error` (negative).",
        "context": {
            "host": HOST,
            "user_id": USER_ID,
            "mailbox": MAILBOX,
            "pair_secret_hex": PAIR_SECRET,
            "now_ms": NOW,
        },
        "desktop": party_json(&d),
        "phone": party_json(&p),
        "fingerprint": {
            "description": "fp = H(\"tmgr-phone-fp:v1:\" + b64(sign_pub) + \":\" + b64(kex_pub))",
            "sign_pub": b64(&d.sign_pub),
            "kex_pub": b64(&d.kex_pub),
            "expected": d.fp,
        },
        "ctx": {
            "mailbox": MAILBOX, "dir": "p2d", "kind": "answer", "id": ANSWER_ID,
            "expected": ctx(MAILBOX, Dir::P2d, "answer", ANSWER_ID),
        },
        "seal": {
            "description": "seal_with(recipient_kex_pub, ...) with fixed ephemeral secret and nonce gives exactly `expected_sealed`; opening it with `recipient_kex_secret_hex` returns `plaintext`",
            "recipient_kex_pub": b64(&p.kex_pub),
            "recipient_kex_secret_hex": p.kex_secret,
            "mailbox": MAILBOX, "dir": "d2p", "kind": "snapshot", "id": "12",
            "plaintext": seal_plain,
            "eph_secret_hex": hex(&[5u8; 32]),
            "nonce_hex": hex(&[5u8; 12]),
            "expected_sealed": serde_json::from_str::<Value>(&serde_json::to_string(&sealed_vec).unwrap()).unwrap(),
        },
        "signed_strings": {
            "hello": {"body": hello_text, "expected": hello_signed(&hc, &hello_text)},
            "pair_ok": {"body": pair_ok_text, "expected": pair_ok_signed(&pr, &pair_ok_text)},
            "snapshot": {"body": snap_text, "seq": snap.seq, "generated_at": snap.generated_at,
                "expected": snapshot_signed(&pr, snap.seq, snap.generated_at, &snap_text)},
            "answer": {"body": answer_text, "expected": answer_signed(&pr, &answer)},
            "ack": {"body": ack_text, "answer_id": ack.answer_id, "result": "applied",
                "expected": ack_signed(&pr, &ack.answer_id, ack.result, &ack_text)},
        },
        "pair": {
            "description": "mac = hex HMAC-SHA256(pair_secret, transcript); code = first 4 bytes of SHA-256(\"tmgr-phone-code:v1:\" + transcript) as big-endian u32 mod 1000000, zero-padded to 6",
            "transcript": transcript,
            "expected_mac": hello.mac,
            "expected_code": pair_code(&transcript),
        },
        "signatures": {
            "description": "All three verify against `sign_pub` for `message`; the DER one is the signer's deterministic output, high_s_der has s above n/2 and must still verify",
            "sign_pub": sig_ok["sign_pub"],
            "message": sig_ok["message"],
            "signer_secret_hex": p.sign_secret,
            "der_b64": b64(&der),
            "raw_rs_b64": b64(&raw),
            "high_s_der_b64": b64(&high),
        },
        "checks": {
            "now_ms": NOW,
            "issued_at_ok": [NOW - 86_400_000, NOW, NOW + 300_000],
            "issued_at_rejected": [NOW - 86_400_001, NOW + 300_001],
            "text_cap_chars": 1000,
            "answer_id_valid": [ANSWER_ID],
            "answer_id_invalid": ["00112233445566778899AABBCCDDEEFF", "0011", "zz112233445566778899aabbccddeeff"],
        },
        "positive": positive,
        "negative": negative,
    })
}

fn fixture_text() -> String {
    let mut t = serde_json::to_string_pretty(&build_fixture()).unwrap();
    t.push('\n');
    t
}

#[test]
fn fixture_file_is_current() {
    let on_disk =
        std::fs::read_to_string(FIXTURE_PATH).expect("fixture missing; run write_fixture");
    assert_eq!(on_disk, fixture_text());
}

#[test]
#[ignore]
fn write_fixture() {
    std::fs::write(FIXTURE_PATH, fixture_text()).unwrap();
}

fn committed() -> Value {
    serde_json::from_str(&std::fs::read_to_string(FIXTURE_PATH).unwrap()).unwrap()
}

#[test]
fn positive_vectors_open() {
    for v in committed()["positive"].as_array().unwrap() {
        let got =
            run_op(s(v, "op"), &v["args"]).unwrap_or_else(|e| panic!("{}: {e}", s(v, "name")));
        assert_eq!(got, v["expect"], "{}", s(v, "name"));
    }
}

#[test]
fn negative_vectors_fail_with_expected_code() {
    let f = committed();
    let list = f["negative"].as_array().unwrap();
    assert!(list.len() >= 8);
    for v in list {
        let err = run_op(s(v, "op"), &v["args"]).expect_err(s(v, "name"));
        assert_eq!(err.code(), s(v, "expect_error"), "{}", s(v, "name"));
    }
}

#[test]
fn derived_values_match_fixture() {
    let f = committed();
    let d = desktop();
    assert_eq!(f["fingerprint"]["expected"], d.fp);
    assert_eq!(
        fingerprint(
            &b64_decode(s(&f["fingerprint"], "sign_pub")).unwrap(),
            &b64_decode(s(&f["fingerprint"], "kex_pub")).unwrap()
        ),
        d.fp
    );
    let c = &f["ctx"];
    assert_eq!(
        ctx(s(c, "mailbox"), Dir::P2d, s(c, "kind"), s(c, "id")),
        s(c, "expected")
    );
    let pair = &f["pair"];
    let t = s(pair, "transcript");
    assert_eq!(
        pair_mac(&unhex(PAIR_SECRET), t).unwrap(),
        s(pair, "expected_mac")
    );
    assert_eq!(pair_code(t), s(pair, "expected_code"));
    assert_eq!(pair_code(t).len(), 6);
}

#[test]
fn seal_vector_is_exact_and_opens() {
    let f = committed();
    let v = &f["seal"];
    let e = Entropy {
        eph_secret: unhex(s(v, "eph_secret_hex")).try_into().unwrap(),
        nonce: unhex(s(v, "nonce_hex")).try_into().unwrap(),
    };
    let sealed = seal_with(
        &b64_decode(s(v, "recipient_kex_pub")).unwrap(),
        MAILBOX,
        Dir::D2p,
        Kind::Snapshot,
        "12",
        s(v, "plaintext").as_bytes(),
        &e,
    )
    .unwrap();
    let json = serde_json::to_value(&sealed).unwrap();
    assert_eq!(json, v["expected_sealed"]);
    let opened = open(
        &unhex(s(v, "recipient_kex_secret_hex")),
        MAILBOX,
        Dir::D2p,
        &json.to_string(),
        &[Kind::Snapshot],
    )
    .unwrap();
    assert_eq!(opened.plaintext, s(v, "plaintext").as_bytes());
    assert_eq!(opened.id, "12");
}

#[test]
fn signed_strings_match_fixture() {
    let f = committed();
    let (d, p) = (desktop(), phone());
    let pr = pairing(&d, &p);
    let hc = hello_ctx(&d);
    let ss = &f["signed_strings"];
    assert_eq!(
        hello_signed(&hc, s(&ss["hello"], "body")),
        s(&ss["hello"], "expected")
    );
    assert_eq!(
        pair_ok_signed(&pr, s(&ss["pair_ok"], "body")),
        s(&ss["pair_ok"], "expected")
    );
    let sn = &ss["snapshot"];
    assert_eq!(
        snapshot_signed(
            &pr,
            sn["seq"].as_i64().unwrap(),
            sn["generated_at"].as_i64().unwrap(),
            s(sn, "body")
        ),
        s(sn, "expected")
    );
    let a: AnswerBody = serde_json::from_str(s(&ss["answer"], "body")).unwrap();
    assert_eq!(answer_signed(&pr, &a), s(&ss["answer"], "expected"));
    let k = &ss["ack"];
    assert_eq!(
        ack_signed(&pr, s(k, "answer_id"), AckResult::Applied, s(k, "body")),
        s(k, "expected")
    );
}

#[test]
fn signature_der_raw_and_high_s_all_verify() {
    let f = committed();
    let g = &f["signatures"];
    let key = b64_decode(s(g, "sign_pub")).unwrap();
    let msg = s(g, "message");
    for k in ["der_b64", "raw_rs_b64", "high_s_der_b64"] {
        verify(&key, msg, s(g, k)).unwrap_or_else(|e| panic!("{k}: {e}"));
        assert_eq!(
            verify(&key, "other", s(g, k)),
            Err(Error::BadSignature),
            "{k}"
        );
    }
    let high = b64_decode(s(g, "high_s_der_b64")).unwrap();
    assert!(Signature::from_der(&high).unwrap().normalize_s().is_some());
    assert_eq!(
        sign(&unhex(s(g, "signer_secret_hex")), msg).unwrap(),
        s(g, "der_b64")
    );
}

#[test]
fn verify_rejects_garbage_signature_and_wrong_key() {
    let (d, p) = (desktop(), phone());
    let sig = sign(&unhex(&p.sign_secret), "m").unwrap();
    assert_eq!(verify(&d.sign_pub, "m", &sig), Err(Error::BadSignature));
    assert_eq!(verify(&p.sign_pub, "m", "AAAA"), Err(Error::BadSignature));
    assert_eq!(verify(&p.sign_pub, "m", "***"), Err(Error::BadSignature));
}

#[test]
fn checks_window_text_and_answer_id() {
    let f = committed();
    let c = &f["checks"];
    let now = c["now_ms"].as_i64().unwrap();
    for t in c["issued_at_ok"].as_array().unwrap() {
        assert!(check_issued_at(t.as_i64().unwrap(), now).is_ok());
    }
    for t in c["issued_at_rejected"].as_array().unwrap() {
        assert_eq!(
            check_issued_at(t.as_i64().unwrap(), now),
            Err(Error::BadWindow)
        );
    }
    assert!(check_text(&"я".repeat(1000)).is_ok());
    assert_eq!(check_text(&"я".repeat(1001)), Err(Error::TextTooLong));
    for id in c["answer_id_valid"].as_array().unwrap() {
        assert!(valid_answer_id(id.as_str().unwrap()));
    }
    for id in c["answer_id_invalid"].as_array().unwrap() {
        assert!(!valid_answer_id(id.as_str().unwrap()));
    }
}

#[test]
fn digest_is_sha256_hex_of_projection() {
    assert_eq!(
        digest("abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn wrong_direction_fails_to_open() {
    let (d, p) = (desktop(), phone());
    let pr = pairing(&d, &p);
    let sealed = seal_answer_with(
        &d.kex_pub,
        &pr,
        &unhex(&p.sign_secret),
        &answer_body(),
        &ent(3),
    )
    .unwrap();
    let opened = open(
        &unhex(&d.kex_secret),
        MAILBOX,
        Dir::D2p,
        &sealed,
        &[Kind::Answer],
    );
    assert_eq!(opened.err(), Some(Error::OpenFailed));
}

#[test]
fn swapped_kind_between_accepted_kinds_fails_ctx() {
    let (d, p) = (desktop(), phone());
    let pr = pairing(&d, &p);
    let sealed = seal_answer_with(
        &d.kex_pub,
        &pr,
        &unhex(&p.sign_secret),
        &answer_body(),
        &ent(3),
    )
    .unwrap();
    let swapped = set(&sealed, "kind", "ack");
    let opened = open(
        &unhex(&d.kex_secret),
        MAILBOX,
        Dir::P2d,
        &swapped,
        &[Kind::Answer, Kind::Ack],
    );
    assert_eq!(opened.err(), Some(Error::OpenFailed));
}

#[test]
fn inner_answer_id_must_equal_outer_id() {
    let (d, p) = (desktop(), phone());
    let pr = pairing(&d, &p);
    let mut body = answer_body();
    let outer = "ab".repeat(16);
    let sig = sign(&unhex(&p.sign_secret), &answer_signed(&pr, &body)).unwrap();
    body.answer_id = ANSWER_ID.into();
    let plaintext = json!({"body": serde_json::to_string(&body).unwrap(), "sig": sig});
    let sealed = seal(
        &d.kex_pub,
        MAILBOX,
        Dir::P2d,
        Kind::Answer,
        &outer,
        plaintext.to_string().as_bytes(),
    )
    .unwrap();
    let r = open_answer(
        &unhex(&d.kex_secret),
        &pr,
        &p.sign_pub,
        &serde_json::to_string(&sealed).unwrap(),
    );
    assert_eq!(r.err(), Some(Error::IdMismatch));
}

#[test]
fn random_seal_round_trips() {
    let (d, p) = (desktop(), phone());
    let pr = pairing(&d, &p);
    let a = seal_answer(&d.kex_pub, &pr, &unhex(&p.sign_secret), &answer_body()).unwrap();
    let b = seal_answer(&d.kex_pub, &pr, &unhex(&p.sign_secret), &answer_body()).unwrap();
    assert_ne!(a, b);
    assert_eq!(
        open_answer(&unhex(&d.kex_secret), &pr, &p.sign_pub, &a).unwrap(),
        answer_body()
    );
}

#[test]
fn snapshot_with_wrong_digest_is_refused() {
    let (d, p) = (desktop(), phone());
    let pr = pairing(&d, &p);
    let mut body = snapshot_body();
    body.cards[0].digest = "0".repeat(64);
    let sealed = seal_snapshot(&p.kex_pub, &pr, &unhex(&d.sign_secret), &body).unwrap();
    let r = open_snapshot(&unhex(&p.kex_secret), &pr, &d.sign_pub, &sealed);
    assert_eq!(r.err(), Some(Error::BadFormat));
}

#[test]
fn compressed_or_short_public_keys_are_refused() {
    let p = phone();
    let mut compressed = p.kex_pub[..33].to_vec();
    compressed[0] = 2;
    let r = seal(&compressed, MAILBOX, Dir::D2p, Kind::Snapshot, "1", b"x");
    assert_eq!(r.err(), Some(Error::BadKey));
}
