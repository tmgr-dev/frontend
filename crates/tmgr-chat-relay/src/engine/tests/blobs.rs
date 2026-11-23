use super::*;
use crate::engine::blob::{BlobPart, MAX_RELAY_BYTES, PART_BYTES};
use tmgr_phone::b64;

fn payload(data: &[u8], size: u64, part: u32, parts: u32) -> Value {
    json!({
        "v": 1,
        "sha256": "ab".repeat(32),
        "size": size,
        "part": part,
        "parts": parts,
        "data": b64(data),
    })
}

fn small() -> Value {
    payload(b"hello", 5, 0, 1)
}

fn blob_world(kind: &str) -> (Fake, Clock, Engine, Engine) {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Phone").with_kind(kind);
    let r = join_as(&fake, &clock, &a, "Runner", "runner");
    r.poll_once().unwrap();
    (fake, clock, a, r)
}

fn post_blob(from: &Engine, to: &Engine, payload: &Value) {
    raw_post(from, &my_id(to), "blob", &rpc_id("blob"), payload);
    to.poll_once().unwrap();
}

#[test]
fn a_blob_from_a_trusted_mobile_reaches_the_sink_and_is_never_a_chat() {
    let (_f, _c, runner, _desktop, mobile, _other) = rooms_world();
    let before = runner.lock().st.seen.len();
    raw_post(&mobile, &my_id(&runner), "blob", &rpc_id("blob"), &small());
    let chats = runner.poll_once().unwrap();
    assert!(chats.is_empty());
    let got = runner.s.blobs.lock().unwrap().clone();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].0, my_id(&mobile));
    assert_eq!(got[0].1.data, b"hello");
    assert_eq!((got[0].1.part, got[0].1.parts, got[0].1.size), (0, 1, 5));
    assert!(runner.s.bodies().is_empty());
    assert_eq!(runner.lock().st.seen.len(), before);
}

#[test]
fn a_blob_from_a_trusted_desktop_is_accepted() {
    let (_f, _c, runner, desktop, _mobile, _other) = rooms_world();
    post_blob(&desktop, &runner, &small());
    assert_eq!(runner.s.blobs.lock().unwrap().len(), 1);
}

#[test]
fn a_blob_from_a_runner_or_an_unpinned_sender_is_dropped() {
    let (_f, _c, runner, _desktop, _mobile, other) = rooms_world();
    post_blob(&other, &runner, &small());
    assert!(runner.s.blobs.lock().unwrap().is_empty());
    assert_eq!(
        runner.s.drops(),
        ["blob from a sender that may not send files"]
    );
    let (_f, _c, a, r) = blob_world("mobile");
    drop_pin(&r, &my_id(&a));
    post_blob(&a, &r, &small());
    assert!(r.s.blobs.lock().unwrap().is_empty());
}

#[test]
fn a_blob_before_this_device_has_joined_is_dropped() {
    let (_f, _c, runner, desktop, _mobile, _other) = rooms_world();
    runner.lock().st.joined = false;
    post_blob(&desktop, &runner, &small());
    assert!(runner.s.blobs.lock().unwrap().is_empty());
    assert_eq!(runner.s.drops().len(), 1);
}

#[test]
fn bad_payloads_are_dropped_with_a_reason() {
    let (_f, _c, runner, desktop, _mobile, _other) = rooms_world();
    post_blob(&desktop, &runner, &json!({"v": 1}));
    post_blob(&desktop, &runner, &payload(b"hello", 6, 0, 1));
    assert!(runner.s.blobs.lock().unwrap().is_empty());
    assert_eq!(
        runner.s.drops(),
        ["malformed blob payload", "malformed blob payload"]
    );
}

#[test]
fn a_failing_sink_blocks_the_cursor_and_the_blob_is_redelivered() {
    let (_f, _c, runner, desktop, _mobile, _other) = rooms_world();
    runner.s.set_failing(true);
    raw_post(&desktop, &my_id(&runner), "blob", &rpc_id("blob"), &small());
    assert!(runner.poll_once().is_err());
    assert!(runner.s.blobs.lock().unwrap().is_empty());
    runner.s.set_failing(false);
    runner.poll_once().unwrap();
    assert_eq!(runner.s.blobs.lock().unwrap().len(), 1);
}

#[test]
fn a_full_part_seals_into_the_one_mib_bucket_and_the_body_stays_under_two_mib() {
    let (fake, _c, runner, desktop, _mobile, _other) = rooms_world();
    let data = vec![0x5a; PART_BYTES];
    let size = (PART_BYTES * 2) as u64;
    let sha = "cd".repeat(32);
    let pl =
        json!({"v": 1, "sha256": sha, "size": size, "part": 0, "parts": 2, "data": b64(&data)});
    raw_post(&desktop, &my_id(&runner), "blob", &rpc_id("blob"), &pl);
    let (_, _, body) = last_body_to(&fake, &my_id(&runner));
    assert!(body.len() < 2 << 20, "{}", body.len());
    let b: crate::wire::Body = serde_json::from_str(&body).unwrap();
    assert_eq!(tmgr_phone::b64_decode(&b.ct).unwrap().len() - 16, 1 << 20);
    runner.poll_once().unwrap();
    let got = runner.s.blobs.lock().unwrap().clone();
    assert_eq!(got[0].1.data.len(), PART_BYTES);
}

fn parse(v: &Value) -> Result<BlobPart, &'static str> {
    BlobPart::parse(v, MAX_RELAY_BYTES)
}

#[test]
fn parse_accepts_a_single_part_and_ignores_unknown_fields() {
    let mut v = small();
    v["extra"] = json!(1);
    let p = parse(&v).unwrap();
    assert_eq!(
        (p.size, p.part, p.parts, p.data.as_slice()),
        (5, 0, 1, &b"hello"[..])
    );
}

#[test]
fn parse_enforces_part_geometry() {
    let full = vec![1u8; PART_BYTES];
    let size = PART_BYTES as u64 + 10;
    assert!(parse(&payload(&full, size, 0, 2)).is_ok());
    assert!(parse(&payload(&[1u8; 10], size, 1, 2)).is_ok());
    assert!(parse(&payload(&[1u8; 11], size, 1, 2)).is_err());
    assert!(parse(&payload(&full[..PART_BYTES - 1], size, 0, 2)).is_err());
    assert!(parse(&payload(&full, size, 2, 2)).is_err());
    assert!(parse(&payload(&full, size, 0, 3)).is_err());
    assert!(parse(&payload(&full, size, 0, 1)).is_err());
    assert!(parse(&payload(&full, PART_BYTES as u64, 0, 1)).is_ok());
    assert!(parse(&payload(&[], 0, 0, 0)).is_err());
    assert!(parse(&payload(&[], PART_BYTES as u64 + 10, 1, 2)).is_err());
}

#[test]
fn parse_enforces_the_size_cap() {
    let full = vec![1u8; PART_BYTES];
    let max = MAX_RELAY_BYTES;
    let last = max - PART_BYTES as u64 * 8;
    let ok = payload(&vec![1u8; last as usize], max, 8, 9);
    assert!(parse(&ok).is_ok());
    let over = payload(&full, max + 1, 0, 10);
    assert!(parse(&over).is_err());
    let last_over = payload(&[1u8; 1], max + 1, 9, 10);
    assert!(parse(&last_over).is_err());
}

#[test]
fn parse_rejects_bad_shapes() {
    for edit in [
        |v: &mut Value| v["v"] = json!(2),
        |v: &mut Value| v["sha256"] = json!("AB".repeat(32)),
        |v: &mut Value| v["sha256"] = json!("ab".repeat(31)),
        |v: &mut Value| v["data"] = json!("not base64!"),
        |v: &mut Value| v["data"] = json!(5),
        |v: &mut Value| v["part"] = json!(-1),
        |v: &mut Value| v["parts"] = json!(4294967296u64),
        |v: &mut Value| {
            v.as_object_mut().unwrap().remove("size");
        },
    ] {
        let mut v = small();
        edit(&mut v);
        assert!(parse(&v).is_err(), "{v}");
    }
    assert!(parse(&json!([1])).is_err());
}
