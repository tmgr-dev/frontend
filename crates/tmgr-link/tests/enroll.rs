use serde_json::{json, Value};
use std::cell::RefCell;
use std::io::Cursor;
use tmgr_link::audit::Audit;
use tmgr_link::enroll::{Enroller, Identity};
use tmgr_link::proto::{self, Framed, MAX_FRAME};
use tmgr_link::scope::Scope;
use tmgr_link::store::{Leader, Paths, Store};
use tmgr_link::taskref::LocalRefs;
use tmgr_link::worker::{serve_with, Env, Limits, NoTitles, Runtime};

const ENR: &str = "0123456789abcdef0123456789abcdef";
const LEADER: &str = "mx_0123456789abcdef";
const SECRET_CODE: &str = "tmgr-chat-join:v2:SECRETSECRETSECRET";

struct Fake {
    calls: RefCell<Vec<(String, String)>>,
    result: Result<Identity, String>,
}

impl Fake {
    fn ok() -> Fake {
        Fake {
            calls: RefCell::default(),
            result: Ok(Identity {
                id: "mx_fedcba9876543210".into(),
                device_id: "11111111-2222-3333-4444-555555555555".into(),
                fp: "ab".repeat(32),
            }),
        }
    }
}

impl Enroller for Fake {
    fn enroll(&self, code: &str, leader_machine: &str) -> Result<Identity, String> {
        self.calls
            .borrow_mut()
            .push((code.into(), leader_machine.into()));
        self.result.clone()
    }
}

struct World {
    _d: tempfile::TempDir,
    paths: Paths,
}

fn world(chat_enroll: bool) -> World {
    let d = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(d.path()).unwrap();
    let paths = Paths::under(&root);
    Store::update(&paths, |s| {
        s.leaders.insert(
            "laptop".into(),
            Leader {
                key: "AAAA".into(),
                fingerprint: "SHA256:laptop".into(),
                added: "2026-11-22T00:00:00Z".into(),
                scope: Scope {
                    chat_enroll,
                    ..Default::default()
                },
                enrollment: ENR.into(),
                authorized_keys: String::new(),
                chat_leader_machine: None,
            },
        );
        Ok(())
    })
    .unwrap();
    World { _d: d, paths }
}

fn frames(items: &[Value]) -> Vec<u8> {
    let mut out = Vec::new();
    for v in items {
        let b = serde_json::to_vec(v).unwrap();
        out.extend_from_slice(&(b.len() as u32).to_be_bytes());
        out.extend_from_slice(&b);
    }
    out
}

fn call(w: &World, fake: Option<&Fake>, params: Value) -> Value {
    let mut out = Vec::new();
    {
        let connect = |_d: Option<std::time::Instant>| {
            Err(tmgr_link::runner::BackendError::Unavailable("none".into()))
        };
        let rt = Runtime {
            connect: &connect,
            resolver: &LocalRefs,
            titles: &NoTitles,
            enroll: fake.map(|f| f as &dyn Enroller),
        };
        let mut chan = Framed::new(
            Cursor::new(frames(&[
                json!({"id":1,"method":"chat.enroll","params":params}),
            ])),
            &mut out,
        );
        let env = Env {
            ssh_connection: Some("192.0.2.5 5555 192.0.2.1 22".into()),
            original_command: None,
        };
        let _ = serve_with(
            &w.paths,
            "laptop",
            ENR,
            &env,
            &mut chan,
            &Limits::default(),
            &rt,
        );
    }
    let mut cur = Cursor::new(out);
    let frame = proto::read_frame(&mut cur, MAX_FRAME).unwrap().unwrap();
    serde_json::from_slice(&frame).unwrap()
}

fn audit_text(w: &World) -> String {
    serde_json::to_string(&Audit::new(&w.paths.audit_dir()).tail(100).unwrap()).unwrap()
}

fn good() -> Value {
    json!({"code": SECRET_CODE, "leader_machine": LEADER})
}

#[test]
fn an_allowed_enroll_replies_with_the_identity_and_audits_no_code() {
    let w = world(true);
    let fake = Fake::ok();
    let r = call(&w, Some(&fake), good());
    assert_eq!(
        r["result"],
        json!({"id": "mx_fedcba9876543210", "device_id": "11111111-2222-3333-4444-555555555555", "fp": "ab".repeat(32)})
    );
    assert_eq!(
        *fake.calls.borrow(),
        [(SECRET_CODE.to_string(), LEADER.to_string())]
    );
    let log = audit_text(&w);
    assert!(log.contains("chat.enroll") && log.contains("accepted") && log.contains(LEADER));
    assert!(!log.contains("SECRET"));
}

#[test]
fn without_the_scope_flag_it_is_refused_audited_and_nothing_runs() {
    let w = world(false);
    let fake = Fake::ok();
    let r = call(&w, Some(&fake), good());
    assert_eq!(r["error"], "forbidden");
    assert!(fake.calls.borrow().is_empty());
    let log = audit_text(&w);
    assert!(log.contains("chat.enroll") && log.contains("rejected"));
    assert!(!log.contains("SECRET"));
}

#[test]
fn a_revoked_link_is_refused_before_the_method_runs() {
    let w = world(true);
    Store::update(&w.paths, |s| {
        s.leaders.clear();
        Ok(())
    })
    .unwrap();
    let fake = Fake::ok();
    let mut out = Vec::new();
    let connect = |_d: Option<std::time::Instant>| {
        Err(tmgr_link::runner::BackendError::Unavailable("n".into()))
    };
    let rt = Runtime {
        connect: &connect,
        resolver: &LocalRefs,
        titles: &NoTitles,
        enroll: Some(&fake),
    };
    let mut chan = Framed::new(
        Cursor::new(frames(&[
            json!({"id":1,"method":"chat.enroll","params":good()}),
        ])),
        &mut out,
    );
    let env = Env {
        ssh_connection: Some("192.0.2.5 5555 192.0.2.1 22".into()),
        original_command: None,
    };
    let r = serve_with(
        &w.paths,
        "laptop",
        ENR,
        &env,
        &mut chan,
        &Limits::default(),
        &rt,
    );
    assert!(r.is_err());
    assert!(fake.calls.borrow().is_empty());
}

#[test]
fn bad_params_are_refused_and_never_reach_the_enroller() {
    let w = world(true);
    let fake = Fake::ok();
    let bad = [
        json!({"code": SECRET_CODE}),
        json!({"leader_machine": LEADER}),
        json!({"code": SECRET_CODE, "leader_machine": "laptop"}),
        json!({"code": SECRET_CODE, "leader_machine": "mx_0123456789ABCDEF"}),
        json!({"code": "", "leader_machine": LEADER}),
        json!({"code": "a b", "leader_machine": LEADER}),
        json!({"code": "x".repeat(5000), "leader_machine": LEADER}),
        json!({"code": SECRET_CODE, "leader_machine": LEADER, "extra": 1}),
        json!({"code": 5, "leader_machine": LEADER}),
        json!("x"),
    ];
    for p in bad {
        let r = call(&w, Some(&fake), p.clone());
        assert_eq!(r["error"], "bad_params", "{p}");
    }
    assert!(fake.calls.borrow().is_empty());
    assert!(!audit_text(&w).contains("SECRET"));
}

#[test]
fn a_failed_join_is_refused_with_its_reason_and_audited_without_the_code() {
    let w = world(true);
    let fake = Fake {
        calls: RefCell::default(),
        result: Err("the follower is not logged in as the same user, or the code expired".into()),
    };
    let r = call(&w, Some(&fake), good());
    assert_eq!(r["error"], "refused");
    assert!(r["message"].as_str().unwrap().contains("not logged in"));
    let log = audit_text(&w);
    assert!(log.contains("failed") && !log.contains("SECRET"));
}

#[test]
fn a_build_without_an_enroller_says_not_available() {
    let w = world(true);
    let r = call(&w, None, good());
    assert_eq!(r["error"], "not_available");
}

#[test]
fn hello_lists_the_method() {
    assert!(proto::METHODS.contains(&"chat.enroll"));
}

const OTHER: &str = "mx_aaaaaaaaaaaaaaaa";

fn bound(w: &World) -> Option<String> {
    Store::load(&w.paths).unwrap().leaders["laptop"]
        .chat_leader_machine
        .clone()
}

#[test]
fn the_first_enroll_binds_the_leader_machine_and_the_same_machine_may_repeat() {
    let w = world(true);
    assert_eq!(bound(&w), None);
    let fake = Fake::ok();
    assert!(call(&w, Some(&fake), good()).get("result").is_some());
    assert_eq!(bound(&w).as_deref(), Some(LEADER));
    assert!(call(&w, Some(&fake), good()).get("result").is_some());
    assert_eq!(fake.calls.borrow().len(), 2);
}

#[test]
fn a_different_leader_machine_on_a_bound_link_is_refused_audited_and_never_enrolls() {
    let w = world(true);
    let fake = Fake::ok();
    call(&w, Some(&fake), good());
    let r = call(
        &w,
        Some(&fake),
        json!({"code": SECRET_CODE, "leader_machine": OTHER}),
    );
    assert_eq!(r["error"], "forbidden");
    assert!(r["message"].as_str().unwrap().contains("different leader"));
    assert_eq!(fake.calls.borrow().len(), 1);
    assert_eq!(bound(&w).as_deref(), Some(LEADER));
    let log = audit_text(&w);
    assert!(log.contains("rejected") && !log.contains("SECRET"));
}

#[test]
fn a_failed_first_enroll_does_not_bind() {
    let w = world(true);
    let fake = Fake {
        calls: RefCell::default(),
        result: Err("no".into()),
    };
    call(&w, Some(&fake), good());
    assert_eq!(bound(&w), None);
}

#[test]
fn an_old_record_without_the_field_loads_unbound() {
    let w = world(true);
    let text = std::fs::read_to_string(w.paths.links_toml()).unwrap();
    assert!(!text.contains("chat_leader_machine"));
    assert_eq!(bound(&w), None);
}
