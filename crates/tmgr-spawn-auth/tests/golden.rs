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
    trusted_device_id: String,
    revoked: bool,
}

impl TrustView for Trust {
    fn is_trusted(&self, device_id: &str, _: &str) -> bool {
        self.trusted && device_id == self.trusted_device_id
    }
    fn is_revoked(&self, _: &str) -> bool {
        self.revoked
    }
}

const DEVICE_ID: &str = "device-uuid-1";

#[derive(Clone)]
struct Case {
    name: &'static str,
    wire: SignedSpawn,
    record: ChallengeRecord,
    key: PinnedKey,
    now_ms: u64,
    trusted: bool,
    trusted_device_id: String,
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
        user_id: "42".into(),
        target_id: "mx_0123456789abcdef".into(),
        target_fp: "b".repeat(64),
        card_id: "card-0001".into(),
        nonce: "0123456789abcdef0123456789abcdef".into(),
        issued_ms: ISSUED,
        exp_ms: ISSUED + MAX_TTL_MS,
        proposal: proposal(),
    };
    let pinned = PinnedKey {
        user_id: "42".into(),
        device_id: DEVICE_ID.into(),
        device_fp: "a".repeat(64),
        spawn_pub: pub_bytes(&key(3)),
    };
    (record, pinned)
}

fn wire_with_sig(r: &ChallengeRecord, k: &PinnedKey, ts_ms: u64, sig: String) -> SignedSpawn {
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
        req_sha256: r.proposal.req_sha256(),
        alg: "es256".into(),
        sig,
    }
}

fn wire_for(r: &ChallengeRecord, k: &PinnedKey, ts_ms: u64, signer: &SigningKey) -> SignedSpawn {
    let sig = der_b64(&sign_low(signer, &signed_string(r, k, ts_ms)));
    wire_with_sig(r, k, ts_ms, sig)
}

fn sig_over_other_segment(
    r: &ChallengeRecord,
    k: &PinnedKey,
    ts_ms: u64,
    index: usize,
    alt: &str,
) -> String {
    let msg = signed_string(r, k, ts_ms);
    let mut parts: Vec<&str> = msg.split(':').collect();
    parts[index] = alt;
    der_b64(&sign_low(&key(3), &parts.join(":")))
}

fn url_safe_variant(std: &str) -> String {
    std.replace('+', "-")
        .replace('/', "_")
        .trim_end_matches('=')
        .to_string()
}

fn run_case(c: &Case) -> &'static str {
    let t = Trust {
        trusted: c.trusted,
        trusted_device_id: c.trusted_device_id.clone(),
        revoked: c.revoked,
    };
    match verify(&c.wire, &c.record, &c.key, &t, c.now_ms) {
        Ok(_) => "ok",
        Err(e) => e.code(),
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
        trusted_device_id: DEVICE_ID.into(),
        revoked: false,
        expect,
    };
    let custom = |name, edit: fn(&mut ChallengeRecord, &mut PinnedKey), expect| {
        let (mut rec, mut pk) = (r.clone(), k.clone());
        edit(&mut rec, &mut pk);
        let w = wire_for(&rec, &pk, ts, &key(3));
        let mut c = mk(name, w, rec, expect);
        c.key = pk;
        c
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
    let sig_over = |name, index, alt: &str| {
        let mut w = good.clone();
        w.sig = sig_over_other_segment(&r, &k, ts, index, alt);
        mk(name, w, r.clone(), "bad_sig")
    };

    let low = sign_low(&key(3), &signed_string(&r, &k, ts));
    let der = low.to_der();
    let raw = b64_encode(&low.to_bytes());
    let truncated = b64_encode(&der.as_bytes()[..der.as_bytes().len() - 1]);
    let std_sig = der_b64(&low);
    let unpadded = std_sig.trim_end_matches('=').to_string();
    let mid = std_sig.len() / 2;
    let inner_space = format!("{} {}", &std_sig[..mid], &std_sig[mid..]);
    let (url_ts, url_sig) = (0..)
        .map(|i| {
            let t = ts + i;
            let s = der_b64(&sign_low(&key(3), &signed_string(&r, &k, t)));
            (t, s)
        })
        .find(|(_, s)| url_safe_variant(s) != *s)
        .unwrap();

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
        wire_edit("other_user", |w| w.user_id = "43".into(), "mismatch"),
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
        custom(
            "pinned_key_other_user",
            |_, k| k.user_id = "43".into(),
            "mismatch",
        ),
        custom(
            "proposal_target_id_differs_from_record",
            |r, _| r.proposal.target_id = "mx_ffffffffffffffff".into(),
            "mismatch",
        ),
        mk(
            "record_proposal_changed_after_signing",
            good.clone(),
            {
                let mut rec = r.clone();
                rec.proposal.mode = SpawnMode::Bypass;
                rec
            },
            "mismatch",
        ),
        custom(
            "bad_token_card_id_space",
            |r, _| r.card_id = "bad card".into(),
            "bad_token",
        ),
        custom(
            "bad_token_card_id_colon",
            |r, _| r.card_id = "a:b".into(),
            "bad_token",
        ),
        custom(
            "bad_token_card_id_65_chars",
            |r, _| r.card_id = "a".repeat(65),
            "bad_token",
        ),
        custom(
            "bad_token_user_id_colon",
            |r, k| {
                r.user_id = "4:2".into();
                k.user_id = "4:2".into();
            },
            "bad_token",
        ),
        custom(
            "bad_token_nonce_uppercase",
            |r, _| r.nonce = "A".repeat(32),
            "bad_token",
        ),
        custom(
            "bad_token_nonce_31_hex",
            |r, _| r.nonce = "a".repeat(31),
            "bad_token",
        ),
        custom(
            "bad_token_target_fp_uppercase",
            |r, _| r.target_fp = "B".repeat(64),
            "bad_token",
        ),
        custom(
            "bad_token_target_fp_63_hex",
            |r, _| r.target_fp = "b".repeat(63),
            "bad_token",
        ),
        custom(
            "bad_token_device_fp_uppercase",
            |_, k| k.device_fp = "A".repeat(64),
            "bad_token",
        ),
        custom(
            "bad_token_device_fp_65_hex",
            |_, k| k.device_fp = "a".repeat(65),
            "bad_token",
        ),
        custom(
            "bad_token_device_id_colon",
            |_, k| k.device_id = "a:b".into(),
            "bad_token",
        ),
        custom(
            "bad_token_target_id_not_mx_hex",
            |r, _| {
                r.target_id = "mx_XYZ".into();
                r.proposal.target_id = "mx_XYZ".into();
            },
            "bad_token",
        ),
        custom(
            "bad_token_proposal_cwd_control_char",
            |r, _| r.proposal.cwd = "/work/\u{1}app".into(),
            "bad_token",
        ),
        custom(
            "bad_token_proposal_name_bidi_override",
            |r, _| r.proposal.name = Some("safe\u{202e}evil".into()),
            "bad_token",
        ),
        custom(
            "bad_token_proposal_name_zero_width",
            |r, _| r.proposal.name = Some("a\u{200b}b".into()),
            "bad_token",
        ),
        custom(
            "bad_token_proposal_name_129_bytes",
            |r, _| r.proposal.name = Some("a".repeat(129)),
            "bad_token",
        ),
        custom(
            "bad_token_proposal_profile_space",
            |r, _| r.proposal.profile = "cl aude".into(),
            "bad_token",
        ),
        custom(
            "bad_token_proposal_prompt_hash_uppercase",
            |r, _| r.proposal.prompt_sha256 = Some("A".repeat(64)),
            "bad_token",
        ),
        custom(
            "bad_token_proposal_version",
            |r, _| r.proposal.v = 2,
            "bad_token",
        ),
        custom(
            "bad_ttl_over_5_min",
            |r, _| r.exp_ms = r.issued_ms + MAX_TTL_MS + 1,
            "bad_ttl",
        ),
        custom(
            "bad_ttl_exp_equals_issued",
            |r, _| r.exp_ms = r.issued_ms,
            "bad_ttl",
        ),
        custom(
            "bad_ttl_exp_before_issued",
            |r, _| r.exp_ms = r.issued_ms - 1,
            "bad_ttl",
        ),
        sig_edit(
            "bad_sig_by_non_pinned_key",
            der_b64(&sign_low(&key(9), &signed_string(&r, &k, ts))),
            "bad_sig",
        ),
        wire_edit("bad_sig_ts_ms_only_tampered", |w| w.ts_ms += 1, "bad_sig"),
        sig_over("bad_sig_signed_other_version_prefix", 1, "v2"),
        sig_over("bad_sig_signed_other_user_id", 2, "43"),
        sig_over("bad_sig_signed_other_device_fp", 3, &"d".repeat(64)),
        sig_over("bad_sig_signed_other_target_id", 4, "mx_ffffffffffffffff"),
        sig_over("bad_sig_signed_other_target_fp", 5, &"c".repeat(64)),
        sig_over("bad_sig_signed_other_card_id", 6, "card-0002"),
        sig_over("bad_sig_signed_other_nonce", 7, &"f".repeat(32)),
        sig_over("bad_sig_signed_other_ts_ms", 8, &(ts + 1).to_string()),
        sig_over(
            "bad_sig_signed_other_exp_ms",
            9,
            &(r.exp_ms - 1).to_string(),
        ),
        sig_over("bad_sig_signed_other_req_sha256", 10, &"e".repeat(64)),
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
            format!("{std_sig}\n"),
            "bad_sig",
        ),
        sig_edit(
            "bad_sig_base64_leading_space",
            format!(" {std_sig}"),
            "bad_sig",
        ),
        sig_edit("bad_sig_base64_inner_space", inner_space, "bad_sig"),
        sig_edit("bad_sig_over_96_chars", format!("{std_sig}AAAA"), "bad_sig"),
    ];

    let mut url = mk(
        "bad_sig_base64url",
        wire_with_sig(&r, &k, url_ts, url_safe_variant(&url_sig)),
        r.clone(),
        "bad_sig",
    );
    url.now_ms = NOW;
    v.push(url);

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

    v.push(mk(
        "ts_in_future_beyond_5_min",
        wire_for(&r, &k, NOW + MAX_SKEW_MS + 1, &key(3)),
        r.clone(),
        "clock_skew",
    ));
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
    v.push({
        let mut rec = r.clone();
        rec.issued_ms = NOW + MAX_SKEW_MS + 1;
        rec.exp_ms = rec.issued_ms + MAX_TTL_MS;
        mk(
            "challenge_issued_in_future_beyond_5_min",
            wire_for(&rec, &k, NOW, &key(3)),
            rec,
            "clock_skew",
        )
    });
    v.push({
        let mut rec = r.clone();
        rec.issued_ms = NOW + MAX_SKEW_MS;
        rec.exp_ms = rec.issued_ms + MAX_TTL_MS;
        mk(
            "ok_challenge_issued_in_future_exactly_5_min",
            wire_for(&rec, &k, NOW, &key(3)),
            rec,
            "ok",
        )
    });

    let mut untrusted = mk("device_not_trusted", good.clone(), r.clone(), "not_trusted");
    untrusted.trusted = false;
    v.push(untrusted);
    let mut other_id = mk(
        "trusted_under_other_device_id",
        good.clone(),
        r.clone(),
        "not_trusted",
    );
    other_id.trusted_device_id = "device-uuid-2".into();
    v.push(other_id);
    let mut revoked = mk("device_revoked", good.clone(), r.clone(), "not_trusted");
    revoked.revoked = true;
    v.push(revoked);

    let spawn_key_case = |name, bytes: Vec<u8>| {
        let mut c = mk(name, good.clone(), r.clone(), "unknown_key");
        c.key.spawn_pub = bytes;
        c
    };
    let full = pub_bytes(&key(3));
    let compressed = key(3)
        .verifying_key()
        .to_encoded_point(true)
        .as_bytes()
        .to_vec();
    v.push(spawn_key_case("unknown_key_64_bytes", full[..64].to_vec()));
    v.push(spawn_key_case("unknown_key_off_curve", vec![4; 65]));
    v.push(spawn_key_case(
        "unknown_key_compressed_33_bytes",
        compressed,
    ));
    v.push(spawn_key_case("unknown_key_empty", Vec::new()));
    v.push(spawn_key_case("unknown_key_hybrid_tag_06", {
        let mut b = full.clone();
        b[0] = 6;
        b
    }));

    for c in &v {
        assert_eq!(run_case(c), c.expect, "case {}", c.name);
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
    now_ms: u64,
    trusted: bool,
    revoked: bool,
    expect: &'static str,
}

fn run_statement(c: &StatementCase, sign_pub: &str, kex_pub: &str) -> &'static str {
    let d = DeviceKeys {
        device_id: c.device_id,
        sign_pub_b64: sign_pub,
        kex_pub_b64: kex_pub,
    };
    let t = Trust {
        trusted: c.trusted,
        trusted_device_id: DEVICE_ID.into(),
        revoked: c.revoked,
    };
    match verify_spawn_key_statement(
        c.user_id,
        &c.statement,
        &c.sig,
        &c.spawn_sig,
        &d,
        &t,
        c.now_ms,
    ) {
        Ok(_) => "ok",
        Err(e) => e.code(),
    }
}

fn statement_cases() -> (Value, Vec<StatementCase>) {
    let dev = key(5);
    let spawn = key(3);
    let kex = key(7);
    let sign_pub = pub_b64(&dev);
    let kex_pub = pub_b64(&kex);
    let fp = device_fp(&sign_pub, &kex_pub);
    let spawn_pub = pub_b64(&spawn);
    let stmt = build_spawn_key_statement("42", &fp, DEVICE_ID, &spawn_pub, ISSUED);
    let sig = |k: &SigningKey, m: &str| der_b64(&sign_low(k, m));
    let mk = |name, statement: String, sig_b: String, spawn_sig: String, expect| StatementCase {
        name,
        user_id: "42",
        statement,
        sig: sig_b,
        spawn_sig,
        device_id: DEVICE_ID,
        now_ms: NOW,
        trusted: true,
        revoked: false,
        expect,
    };
    let both = |s: &str| (sig(&dev, s), sig(&spawn, s));
    let signed_variant = |name, s: String, expect| {
        let (x, y) = both(&s);
        mk(name, s, x, y, expect)
    };
    let (a, b) = both(&stmt);
    let compressed = b64_encode(spawn.verifying_key().to_encoded_point(true).as_bytes());
    let mut v = vec![
        mk("ok", stmt.clone(), a.clone(), b.clone(), "ok"),
        mk(
            "ok_high_s_both",
            stmt.clone(),
            der_b64(&high_of(&sign_low(&dev, &stmt))),
            der_b64(&high_of(&sign_low(&spawn, &stmt))),
            "ok",
        ),
        signed_variant("wrong_user", stmt.replacen(":42:", ":43:", 1), "mismatch"),
        signed_variant(
            "leading_zero_issued_at",
            stmt.replace(&format!(":{ISSUED}"), &format!(":0{ISSUED}")),
            "bad_token",
        ),
        signed_variant(
            "plus_sign_issued_at",
            stmt.replace(&format!(":{ISSUED}"), &format!(":+{ISSUED}")),
            "bad_token",
        ),
        signed_variant("extra_colon_segment", format!("{stmt}:x"), "bad_token"),
        signed_variant(
            "issued_at_future_beyond_5_min",
            stmt.replace(
                &format!(":{ISSUED}"),
                &format!(":{}", NOW + MAX_SKEW_MS + 1),
            ),
            "clock_skew",
        ),
        signed_variant(
            "ok_issued_at_future_exactly_5_min",
            stmt.replace(&format!(":{ISSUED}"), &format!(":{}", NOW + MAX_SKEW_MS)),
            "ok",
        ),
        signed_variant(
            "issued_at_2_pow_53",
            stmt.replace(&format!(":{ISSUED}"), ":9007199254740992"),
            "bad_token",
        ),
        signed_variant(
            "statement_over_512_bytes",
            format!("{stmt}{}", "0".repeat(512)),
            "bad_token",
        ),
        signed_variant(
            "spawn_key_compressed",
            stmt.replace(&spawn_pub, &compressed),
            "unknown_key",
        ),
        mk(
            "spawn_key_equals_device_sign_key",
            stmt.replace(&spawn_pub, &sign_pub),
            sig(&dev, &stmt.replace(&spawn_pub, &sign_pub)),
            sig(&dev, &stmt.replace(&spawn_pub, &sign_pub)),
            "unknown_key",
        ),
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
        mk(
            "spawn_sig_over_96_chars",
            stmt.clone(),
            a.clone(),
            format!("{b}AAAA"),
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
        assert_eq!(
            run_statement(c, &sign_pub, &kex_pub),
            c.expect,
            "statement case {}",
            c.name
        );
    }
    let head = json!({
        "test_device_sign_secret_hex": hex::encode([5u8; 32]),
        "test_device_sign_pub_b64": sign_pub,
        "test_device_kex_secret_hex": hex::encode([7u8; 32]),
        "test_device_kex_pub_b64": kex_pub,
        "device_id": DEVICE_ID,
        "device_fp": fp,
        "test_spawn_secret_hex": hex::encode([3u8; 32]),
        "test_spawn_pub_b64": spawn_pub,
        "fp_input": format!("tmgr-chat-fp:v1:{sign_pub}:{kex_pub}"),
    });
    (head, v)
}

fn statement_of(user: &str, fp: &str, issued: u64, pub_bytes: Vec<u8>) -> SpawnKeyStatement {
    SpawnKeyStatement {
        user_id: user.into(),
        device_fp: fp.into(),
        device_id: DEVICE_ID.into(),
        spawn_pub: pub_bytes,
        issued_at_ms: issued,
    }
}

fn supersedes_cases() -> Vec<(&'static str, SpawnKeyStatement, SpawnKeyStatement, bool)> {
    let fp = "a".repeat(64);
    let (k1, k2) = (pub_bytes(&key(3)), pub_bytes(&key(4)));
    let old = statement_of("42", &fp, ISSUED, k1.clone());
    let v = vec![
        (
            "strictly_newer_other_key",
            statement_of("42", &fp, ISSUED + 1, k2.clone()),
            old.clone(),
            true,
        ),
        (
            "older_replayed_over_newer",
            old.clone(),
            statement_of("42", &fp, ISSUED + 1, k2.clone()),
            false,
        ),
        ("equal_time_same_key", old.clone(), old.clone(), false),
        (
            "equal_time_different_key",
            statement_of("42", &fp, ISSUED, k2.clone()),
            old.clone(),
            false,
        ),
        (
            "newer_but_other_device",
            statement_of("42", &"b".repeat(64), ISSUED + 1, k2.clone()),
            old.clone(),
            false,
        ),
        (
            "newer_but_other_user",
            statement_of("43", &fp, ISSUED + 1, k2),
            old,
            false,
        ),
    ];
    for (name, new, old, want) in &v {
        assert_eq!(supersedes(new, old), *want, "supersedes case {name}");
    }
    v
}

fn canonical_proposals() -> Vec<(&'static str, SpawnProposal)> {
    let with_name = |s: &str| {
        let mut p = proposal();
        p.name = Some(s.into());
        p
    };
    let mut full = proposal();
    full.task = Some("TM-1017".into());
    let mut bare = proposal();
    bare.prompt_sha256 = None;
    bare.name = None;
    vec![
        ("sample", proposal()),
        ("all_optional_fields", full),
        ("no_optional_fields", bare),
        ("name_control_u0001", with_name("a\u{1}b")),
        ("name_tab", with_name("a\tb")),
        ("name_newline_cr_ff_bs", with_name("a\nb\rc\u{c}d\u{8}e")),
        ("name_u001f_and_del", with_name("a\u{1f}b\u{7f}c")),
        ("name_c1_u0085", with_name("a\u{85}b")),
        (
            "name_line_separator_u2028",
            with_name("a\u{2028}b\u{2029}c"),
        ),
        ("name_bidi_override_u202e", with_name("a\u{202e}b")),
        ("name_zero_width_space", with_name("a\u{200b}b")),
        ("name_astral_emoji", with_name("ship \u{1f680} now")),
        ("name_backslash", with_name("a\\b")),
        ("name_slash", with_name("a/b</script>")),
        ("name_double_quote", with_name("say \"hi\"")),
        (
            "name_cyrillic",
            with_name("\u{43f}\u{440}\u{438}\u{432}\u{435}\u{442}"),
        ),
    ]
}

fn proposal_validates(p: &SpawnProposal) -> &'static str {
    match p.validate() {
        Ok(()) => "ok",
        Err(e) => e.code(),
    }
}

fn wire_json_cases() -> Vec<(&'static str, String, &'static str)> {
    let (r, k) = base_record();
    let ts = NOW - 1_000;
    let w = wire_for(&r, &k, ts, &key(3));
    let base = serde_json::to_string(&w).unwrap();
    let exp = format!("\"exp_ms\":{}", w.exp_ms);
    let tsf = format!("\"ts_ms\":{ts}");
    let swap = |from: &str, to: &str| {
        assert!(base.contains(from), "{from}");
        base.replace(from, to)
    };
    let v = vec![
        ("ok", base.clone(), "ok"),
        (
            "ok_ts_ms_max_safe_integer",
            swap(&tsf, "\"ts_ms\":9007199254740991"),
            "ok",
        ),
        (
            "unknown_field",
            format!("{},\"extra\":1}}", &base[..base.len() - 1]),
            "refused",
        ),
        (
            "duplicate_key",
            format!("{{\"v\":1,{}", &base[1..]),
            "refused",
        ),
        (
            "float_ts_ms",
            swap(&tsf, &format!("\"ts_ms\":{ts}.0")),
            "refused",
        ),
        (
            "exponent_ts_ms",
            swap(&tsf, "\"ts_ms\":1.795e12"),
            "refused",
        ),
        (
            "float_exp_ms",
            swap(&exp, &format!("\"exp_ms\":{}.0", w.exp_ms)),
            "refused",
        ),
        ("float_v", swap("\"v\":1", "\"v\":1.0"), "refused"),
        (
            "ts_ms_2_pow_53",
            swap(&tsf, "\"ts_ms\":9007199254740992"),
            "refused",
        ),
        (
            "ts_ms_2_pow_53_plus_1",
            swap(&tsf, "\"ts_ms\":9007199254740993"),
            "refused",
        ),
        (
            "ts_ms_u64_max",
            swap(&tsf, "\"ts_ms\":18446744073709551615"),
            "refused",
        ),
        (
            "exp_ms_2_pow_53",
            swap(&exp, "\"exp_ms\":9007199254740992"),
            "refused",
        ),
        ("negative_ts_ms", swap(&tsf, "\"ts_ms\":-1"), "refused"),
        ("negative_zero_ts_ms", swap(&tsf, "\"ts_ms\":-0"), "refused"),
        (
            "string_ts_ms",
            swap(&tsf, &format!("\"ts_ms\":\"{ts}\"")),
            "refused",
        ),
        (
            "missing_sig",
            swap(&format!(",\"sig\":\"{}\"", w.sig), ""),
            "refused",
        ),
    ];
    for (name, json, want) in &v {
        let got = if serde_json::from_str::<SignedSpawn>(json).is_ok() {
            "ok"
        } else {
            "refused"
        };
        assert_eq!(got, *want, "wire json case {name}");
    }
    v
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
                "trusted_device_id": c.trusted_device_id,
                "device_revoked": c.revoked,
                "record": {
                    "user_id": c.record.user_id, "target_id": c.record.target_id,
                    "target_fp": c.record.target_fp, "card_id": c.record.card_id,
                    "nonce": c.record.nonce, "issued_ms": c.record.issued_ms,
                    "exp_ms": c.record.exp_ms,
                    "proposal": serde_json::to_value(&c.record.proposal).unwrap(),
                },
                "pinned_key": {
                    "user_id": c.key.user_id, "device_id": c.key.device_id,
                    "device_fp": c.key.device_fp,
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
                "now_ms": c.now_ms,
                "device_trusted": c.trusted, "trusted_device_id": DEVICE_ID,
                "device_revoked": c.revoked,
                "statement": c.statement, "sig": c.sig, "spawn_sig": c.spawn_sig,
                "expect": c.expect,
            })
        })
        .collect();
    let supersedes: Vec<Value> = supersedes_cases()
        .iter()
        .map(|(name, new, old, want)| {
            let side = |s: &SpawnKeyStatement| {
                json!({
                    "user_id": s.user_id, "device_fp": s.device_fp,
                    "spawn_pub_b64": b64_encode(&s.spawn_pub), "issued_at_ms": s.issued_at_ms,
                })
            };
            json!({ "name": name, "new": side(new), "old": side(old), "expect": want })
        })
        .collect();
    let canonical_cases: Vec<Value> = canonical_proposals()
        .iter()
        .map(|(name, p)| {
            let bytes = p.canonical_bytes();
            json!({
                "name": name,
                "fields": serde_json::to_value(p).unwrap(),
                "canonical_utf8": String::from_utf8(bytes.clone()).unwrap(),
                "canonical_b64": b64_encode(&bytes),
                "req_sha256": p.req_sha256(),
                "validate": proposal_validates(p),
            })
        })
        .collect();
    let wire_json_cases: Vec<Value> = wire_json_cases()
        .iter()
        .map(|(name, json, expect)| json!({ "name": name, "json": json, "expect": expect }))
        .collect();
    json!({
        "doc": "SignedSpawn v1 golden vectors, regenerated by `cargo test -p tmgr-spawn-auth -- --ignored regenerate`. All keys are TEST-ONLY (test_* fields). Secrets are 32 bytes of one repeated byte; P-256; public keys are 65-byte uncompressed SEC1 (0x04 prefix; compressed or other lengths are refused), base64 STANDARD padded. Signatures are DER ECDSA P-256 over SHA-256 of the UTF-8 message, base64 STANDARD padded and canonical (re-encoding must reproduce the string), at most 96 chars; raw r||s, base64url and unpadded forms are refused. A verifier must accept high-S. `proposal.fields` serialize to `proposal.canonical_utf8` in exactly the declared field order, no spaces, None omitted, serde_json escaping (quote, backslash, and control chars below U+0020 are escaped: \\b \\t \\n \\f \\r short forms, others as lowercase \\u00xx; '/', DEL, C1, U+2028 and all non-ASCII stay raw UTF-8); `req_sha256` is lowercase hex SHA-256 of those bytes; `canonical_cases[]` pins this escaping and `validate` says whether SpawnProposal::validate accepts the fields (no C0/C1/bidi/zero-width/U+2028/U+2029 characters, cwd <= 4096 bytes, name/task <= 128 bytes, profile a token, prompt_sha256 64 lowercase hex, target_id mx_ + lowercase hex, v == 1). `signed.message` is the exact signed string. `verify_cases[]`: given record (the machine's stored challenge, with its proposal; req_sha256 is recomputed from the proposal, never stored), pinned_key, wire (the SignedSpawn JSON), now_ms and the trust view (a device is trusted only when device_trusted is true and pinned_key.device_id == trusted_device_id; device_revoked is by device_fp), verify must return `expect` (`ok` or a refusal code: mismatch, unsupported_version, unsupported_alg, bad_ttl, expired, clock_skew, bad_token, unknown_key, not_trusted, bad_sig). Check order: field mismatch (including proposal.target_id == record.target_id and the recomputed req_sha256), version, alg, ttl (0 < exp_ms - issued_ms <= 5 min), expiry, skew (issued_ms and ts_ms within 5 min of now_ms), tokens (fps are 64 lowercase hex, target_id mx_[0-9a-f]+, nonce 32 lowercase hex, other ids tokens [A-Za-z0-9._-]{1,64}, proposal valid), key, trust, signature. `wire_json_cases[]`: the raw wire JSON text must be accepted (`ok`) or refused (`refused`) by the parser: unknown fields, duplicate keys, non-integer numbers (a text with '.' or exponent, even 1.0), negative numbers (including -0), and ts_ms/exp_ms above 2^53-1 are refused; a port must inspect raw number tokens because JSON.parse loses the difference. `statement_cases[]`: a tmgr-spawn-key:v1 statement (at most 512 bytes, exactly five ':'-separated fields after the prefix, issued_at_ms canonical decimal <= 2^53-1 and not more than 5 min after now_ms) must carry two valid signatures (device chat sign key `sig`, spawn key `spawn_sig`), the spawn key must differ from the device sign key, and the device must be trusted and not revoked; `expect` as above. `supersedes_cases[]`: a new statement replaces the stored one only when same user and device_fp and issued_at_ms is strictly greater.",
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
            "ts_ms": ts, "exp_ms": record.exp_ms, "req_sha256": record.proposal.req_sha256(),
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
        "canonical_cases": canonical_cases,
        "wire_json_cases": wire_json_cases,
        "verify_cases": verify_cases,
        "statement_cases": statement_cases,
        "supersedes_cases": supersedes,
    })
}

fn load() -> Value {
    serde_json::from_str(&std::fs::read_to_string(FIXTURE).unwrap()).unwrap()
}

#[test]
fn fixture_matches_generator() {
    assert_eq!(
        load(),
        generate(),
        "run: cargo test -p tmgr-spawn-auth -- --ignored regenerate"
    );
}

#[test]
fn fixture_cases_verify() {
    let f = load();
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
            proposal: serde_json::from_value(rec["proposal"].clone()).unwrap(),
        };
        let pk = &c["pinned_key"];
        let pinned = PinnedKey {
            user_id: s(pk, "user_id"),
            device_id: s(pk, "device_id"),
            device_fp: s(pk, "device_fp"),
            spawn_pub: b64_decode_strict(pk["spawn_pub_b64"].as_str().unwrap()).unwrap(),
        };
        let wire: SignedSpawn = serde_json::from_value(c["wire"].clone()).unwrap();
        let case = Case {
            name: "fixture",
            wire,
            record,
            key: pinned,
            now_ms: n(c, "now_ms"),
            trusted: c["device_trusted"].as_bool().unwrap(),
            trusted_device_id: s(c, "trusted_device_id"),
            revoked: c["device_revoked"].as_bool().unwrap(),
            expect: "",
        };
        assert_eq!(
            run_case(&case),
            c["expect"].as_str().unwrap(),
            "{}",
            c["name"]
        );
        seen += 1;
    }
    assert!(seen >= 75);
}

#[test]
fn fixture_wire_json_and_canonical_cases() {
    let f = load();
    for c in f["wire_json_cases"].as_array().unwrap() {
        let got = if serde_json::from_str::<SignedSpawn>(c["json"].as_str().unwrap()).is_ok() {
            "ok"
        } else {
            "refused"
        };
        assert_eq!(got, c["expect"].as_str().unwrap(), "{}", c["name"]);
    }
    for c in f["canonical_cases"].as_array().unwrap() {
        let p: SpawnProposal = serde_json::from_value(c["fields"].clone()).unwrap();
        assert_eq!(
            String::from_utf8(p.canonical_bytes()).unwrap(),
            c["canonical_utf8"].as_str().unwrap(),
            "{}",
            c["name"]
        );
        assert_eq!(p.req_sha256(), c["req_sha256"].as_str().unwrap());
        assert_eq!(proposal_validates(&p), c["validate"].as_str().unwrap());
    }
}

#[test]
fn canonical_escapes_are_pinned() {
    let by_name = |name: &str| {
        canonical_proposals()
            .into_iter()
            .find(|(n, _)| *n == name)
            .map(|(_, p)| String::from_utf8(p.canonical_bytes()).unwrap())
            .unwrap()
    };
    let has = |name: &str, frag: &str| {
        let s = by_name(name);
        assert!(s.contains(frag), "{name}: {s}");
    };
    has("name_control_u0001", r#""name":"a\u0001b""#);
    has("name_tab", r#""name":"a\tb""#);
    has("name_newline_cr_ff_bs", r#""name":"a\nb\rc\fd\be""#);
    has("name_u001f_and_del", "\"name\":\"a\\u001fb\u{7f}c\"");
    has("name_c1_u0085", "\"name\":\"a\u{85}b\"");
    has(
        "name_line_separator_u2028",
        "\"name\":\"a\u{2028}b\u{2029}c\"",
    );
    has("name_astral_emoji", "\"name\":\"ship \u{1f680} now\"");
    has("name_backslash", r#""name":"a\\b""#);
    has("name_slash", r#""name":"a/b</script>""#);
    has("name_double_quote", r#""name":"say \"hi\"""#);
}

#[test]
fn fixture_signed_string_and_hash() {
    let f = load();
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
