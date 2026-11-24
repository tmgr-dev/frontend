use p256::ecdsa::signature::Signer;
use p256::ecdsa::{Signature, SigningKey};
use serde_json::{json, Value};
use tmgr_spawn_auth::*;

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/permit-v1.json");
const ISSUED: u64 = 1_795_000_000_000;
const NOW: u64 = ISSUED + 30_000;

struct Trust(TrustStatus);
impl TrustView for Trust {
    fn status(&self, _: &str, _: &str) -> TrustStatus {
        self.0
    }
}

fn key(byte: u8) -> SigningKey {
    SigningKey::from_slice(&[byte; 32]).unwrap()
}

fn pub_bytes(k: &SigningKey) -> Vec<u8> {
    k.verifying_key()
        .to_encoded_point(false)
        .as_bytes()
        .to_vec()
}

fn trust_name(t: TrustStatus) -> &'static str {
    match t {
        TrustStatus::Trusted => "trusted",
        TrustStatus::NotTrusted => "not_trusted",
        TrustStatus::Revoked => "revoked",
        TrustStatus::Unknown => "unknown",
    }
}

fn trust_from(name: &str) -> TrustStatus {
    [
        TrustStatus::Trusted,
        TrustStatus::NotTrusted,
        TrustStatus::Revoked,
        TrustStatus::Unknown,
    ]
    .into_iter()
    .find(|t| trust_name(*t) == name)
    .unwrap()
}

fn screen() -> PermitScreen {
    PermitScreen {
        agent: "claude-code".into(),
        tool: "Bash".into(),
        text: "rm -rf build \u{43f}\u{440}\u{438}\u{432}\u{435}\u{442}".into(),
        path: Some("/work/app".into()),
        reason: Some("clean the build".into()),
        from: None,
        warning: Some("deletes files".into()),
        danger: vec!["deletes files".into(), "not undoable".into()],
        always_label: Some("Always allow rm in /work/app".into()),
    }
}

fn record() -> PermitRecord {
    PermitRecord {
        user_id: "42".into(),
        runner_id: "mx_0123456789abcdef".into(),
        runner_fp: "b".repeat(64),
        card_id: "card-0001".into(),
        session: "s_abcdefghij".into(),
        nonce: "0123456789abcdef0123456789abcdef".into(),
        issued_ms: ISSUED,
        exp_ms: ISSUED + MAX_PERMIT_TTL_MS,
        screen: screen(),
    }
}

fn pinned() -> PinnedKey {
    PinnedKey {
        user_id: "42".into(),
        device_id: "device-uuid-1".into(),
        device_fp: "a".repeat(64),
        spawn_pub: pub_bytes(&key(3)),
    }
}

fn base_wire(choice: PermitChoice, confirmed: u8) -> SignedPermit {
    let (r, k) = (record(), pinned());
    let mut w = SignedPermit {
        v: 1,
        user_id: r.user_id.clone(),
        device_fp: k.device_fp.clone(),
        runner_id: r.runner_id.clone(),
        runner_fp: r.runner_fp.clone(),
        card_id: r.card_id.clone(),
        session: r.session.clone(),
        choice,
        confirmed,
        nonce: r.nonce.clone(),
        ts_ms: ISSUED + 10_000,
        exp_ms: r.exp_ms,
        req_sha256: r.screen.req_sha256(),
        alg: "es256".into(),
        sig: String::new(),
    };
    resign(&mut w);
    w
}

fn resign(w: &mut SignedPermit) {
    let s: Signature = key(3).sign(permit_signed_string(&record(), &pinned(), w).as_bytes());
    let s = s.normalize_s().unwrap_or(s);
    w.sig = b64_encode(s.to_der().as_bytes());
}

struct Case<'a> {
    name: &'a str,
    wire: &'a SignedPermit,
    now: u64,
    trust: TrustStatus,
    owner: bool,
    sender_fp: Option<String>,
    expect: &'a str,
}

fn case<'a>(name: &'a str, wire: &'a SignedPermit, expect: &'a str) -> Case<'a> {
    Case {
        name,
        wire,
        now: NOW,
        trust: TrustStatus::Trusted,
        owner: true,
        sender_fp: None,
        expect,
    }
}

fn case_json(c: Case) -> Value {
    json!({
        "name": c.name,
        "wire": serde_json::to_value(c.wire).unwrap(),
        "now_ms": c.now,
        "trust": trust_name(c.trust),
        "owner_device": c.owner,
        "sender_fp": c.sender_fp.unwrap_or_else(|| c.wire.device_fp.clone()),
        "expect": c.expect,
    })
}

fn cases() -> Vec<Value> {
    let exp = ISSUED + MAX_PERMIT_TTL_MS;
    let yes1 = base_wire(PermitChoice::Yes, 1);
    let no0 = base_wire(PermitChoice::No, 0);
    let always1 = base_wire(PermitChoice::Always, 1);
    let yes0 = base_wire(PermitChoice::Yes, 0);
    let always0 = base_wire(PermitChoice::Always, 0);
    let yes2 = base_wire(PermitChoice::Yes, 2);

    let mut stale_nonce = yes1.clone();
    stale_nonce.nonce = "f".repeat(32);
    let mut other_card = yes1.clone();
    other_card.card_id = "card-0002".into();
    let mut screen_mismatch = yes1.clone();
    screen_mismatch.req_sha256 = sha256_hex(b"other screen");
    let mut changed = yes1.clone();
    changed.choice = PermitChoice::No;
    let mut bad_version = yes1.clone();
    bad_version.v = 2;
    let mut bad_alg = yes1.clone();
    bad_alg.alg = "es384".into();
    let mut other_key = yes1.clone();
    let s: Signature =
        key(4).sign(permit_signed_string(&record(), &pinned(), &other_key).as_bytes());
    other_key.sig = b64_encode(s.to_der().as_bytes());
    let mut high_s = yes1.clone();
    let low = Signature::from_der(&b64_decode_strict(&high_s.sig).unwrap()).unwrap();
    let high =
        Signature::from_scalars(low.r().to_bytes(), (-*low.s().as_ref()).to_bytes()).unwrap();
    high_s.sig = b64_encode(high.to_der().as_bytes());

    let mut at_exp = case("ok_at_exp", &yes1, "ok");
    at_exp.now = exp;
    let mut expired = case("expired", &yes1, "expired");
    expired.now = exp + 1;
    let mut not_owner = case("not_owner_device", &yes1, "not_owner_device");
    not_owner.owner = false;
    let mut revoked = case("revoked", &yes1, "not_trusted");
    revoked.trust = TrustStatus::Revoked;
    let mut mismatch = case("sender_mismatch", &yes1, "sender_mismatch");
    mismatch.sender_fp = Some("c".repeat(64));

    [
        case("ok_yes_confirmed", &yes1, "ok"),
        case("ok_no_unconfirmed", &no0, "ok"),
        case("ok_always_confirmed", &always1, "ok"),
        at_exp,
        expired,
        case("danger_yes_unconfirmed", &yes0, "needs_confirm"),
        case("always_unconfirmed", &always0, "needs_confirm"),
        case("confirmed_two", &yes2, "bad_token"),
        not_owner,
        revoked,
        mismatch,
        case("stale_nonce", &stale_nonce, "stale"),
        case("other_card", &other_card, "stale"),
        case("screen_mismatch", &screen_mismatch, "screen_mismatch"),
        case("choice_changed_after_signing", &changed, "bad_sig"),
        case("unsupported_version", &bad_version, "unsupported_version"),
        case("unsupported_alg", &bad_alg, "unsupported_alg"),
        case("signed_by_other_key", &other_key, "bad_sig"),
        case("ok_high_s", &high_s, "ok"),
    ]
    .into_iter()
    .map(case_json)
    .collect()
}

fn hex_of(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn generate() -> Value {
    let (r, k) = (record(), pinned());
    let w = base_wire(PermitChoice::Yes, 1);
    json!({
        "_comment": "Permit v1 golden vectors, regenerated by `cargo test -p tmgr-spawn-auth --test permit_golden -- --ignored regenerate`. All keys are TEST-ONLY. Secrets are 32 bytes of one repeated byte; P-256; public key is 65-byte uncompressed SEC1, base64 STANDARD padded. Signatures are DER ECDSA P-256 over SHA-256 of the UTF-8 signed string, base64 STANDARD padded; a verifier must accept high-S. req_sha256 is sha256 of canonical_bytes_hex (never of JSON): magic 'tmgr-permit-card:v1\\n', then agent, tool, text, path, reason, from, warning (each option is 0x00, or 0x01 + string), danger (u32 BE count, then each item), always_label (option); strings are u32 BE byte length + UTF-8.",
        "test_signing_seed": 3,
        "pub_b64": b64_encode(&k.spawn_pub),
        "record": {
            "user_id": r.user_id,
            "runner_id": r.runner_id,
            "runner_fp": r.runner_fp,
            "card_id": r.card_id,
            "session": r.session,
            "nonce": r.nonce,
            "issued_ms": r.issued_ms,
            "exp_ms": r.exp_ms,
            "screen": serde_json::to_value(&r.screen).unwrap(),
        },
        "key": {"user_id": k.user_id, "device_id": k.device_id, "device_fp": k.device_fp},
        "screen": {
            "canonical_bytes_hex": hex_of(&r.screen.canonical_bytes()),
            "req_sha256": r.screen.req_sha256(),
        },
        "signed_string": permit_signed_string(&r, &k, &w),
        "sig_b64": w.sig,
        "cases": cases(),
    })
}

fn load() -> Value {
    serde_json::from_str(&std::fs::read_to_string(FIXTURE).unwrap()).unwrap()
}

fn record_from(f: &Value) -> PermitRecord {
    let r = &f["record"];
    let s = |k: &str| r[k].as_str().unwrap().to_string();
    PermitRecord {
        user_id: s("user_id"),
        runner_id: s("runner_id"),
        runner_fp: s("runner_fp"),
        card_id: s("card_id"),
        session: s("session"),
        nonce: s("nonce"),
        issued_ms: r["issued_ms"].as_u64().unwrap(),
        exp_ms: r["exp_ms"].as_u64().unwrap(),
        screen: serde_json::from_value(r["screen"].clone()).unwrap(),
    }
}

fn key_from(f: &Value) -> PinnedKey {
    let k = &f["key"];
    let s = |n: &str| k[n].as_str().unwrap().to_string();
    PinnedKey {
        user_id: s("user_id"),
        device_id: s("device_id"),
        device_fp: s("device_fp"),
        spawn_pub: b64_decode_strict(f["pub_b64"].as_str().unwrap()).unwrap(),
    }
}

#[test]
fn fixture_matches_generator() {
    assert_eq!(
        load(),
        generate(),
        "run: cargo test -p tmgr-spawn-auth --test permit_golden -- --ignored regenerate"
    );
}

#[test]
fn fixture_hash_and_signed_string() {
    let f = load();
    let r = record_from(&f);
    let k = key_from(&f);
    assert_eq!(
        hex_of(&r.screen.canonical_bytes()),
        f["screen"]["canonical_bytes_hex"].as_str().unwrap()
    );
    assert_eq!(
        r.screen.req_sha256(),
        f["screen"]["req_sha256"].as_str().unwrap()
    );
    let w = base_wire(PermitChoice::Yes, 1);
    assert_eq!(
        permit_signed_string(&r, &k, &w),
        f["signed_string"].as_str().unwrap()
    );
    assert_eq!(w.sig, f["sig_b64"].as_str().unwrap());
}

#[test]
fn fixture_cases_verify() {
    let f = load();
    let (r, k) = (record_from(&f), key_from(&f));
    let cases = f["cases"].as_array().unwrap();
    assert!(cases.len() >= 15);
    for c in cases {
        let w: SignedPermit = serde_json::from_value(c["wire"].clone()).unwrap();
        let sender = PermitSender {
            device_fp: c["sender_fp"].as_str().unwrap(),
            owner_device: c["owner_device"].as_bool().unwrap(),
            kind: PermitSenderKind::Mobile,
            desktop: DesktopPolicy::Safe,
        };
        let trust = Trust(trust_from(c["trust"].as_str().unwrap()));
        let got = verify_permit(&w, &r, &k, sender, &trust, c["now_ms"].as_u64().unwrap());
        let code = match got {
            Ok(_) => "ok",
            Err(e) => e.code(),
        };
        assert_eq!(code, c["expect"].as_str().unwrap(), "{}", c["name"]);
    }
}

#[test]
#[ignore]
fn regenerate() {
    let mut text = serde_json::to_string_pretty(&generate()).unwrap();
    text.push('\n');
    std::fs::write(FIXTURE, text).unwrap();
}
