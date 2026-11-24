use super::probe::{machine_names, name_runners};
use super::*;
use tmgr_chat_relay::api::Request;
use tmgr_chat_relay::engine::MemKeyStore;
use tmgr_runner::chat::relay::store::RelayConfig;

const LOCAL: &str = "mx_00000000000000aa";
const PEER: &str = "mx_00000000000000bb";

#[derive(Clone, Default)]
struct Offline(Arc<std::sync::Mutex<Vec<serde_json::Value>>>);

impl Transport for Offline {
    fn call(&self, req: &Request, _: Option<&str>) -> std::result::Result<String, String> {
        self.0
            .lock()
            .unwrap()
            .push(req.body.clone().unwrap_or_default());
        Err("offline".into())
    }
}

fn cfg() -> RelayConfig {
    RelayConfig {
        server: "https://tmgr.dev".into(),
        user_id: 7,
    }
}

fn device() -> AttachDevice<Offline, MemKeyStore> {
    build(
        Offline::default(),
        MemKeyStore::default(),
        RoomCache::memory(),
        &cfg(),
        LOCAL,
    )
    .unwrap()
}

#[test]
fn the_label_names_the_machine_and_never_exceeds_the_relay_limit() {
    assert_eq!(device_label("Studio"), "tmgr attach (Studio)");
    for machine in ["", "x", &"é".repeat(64), &"y".repeat(500)] {
        let l = device_label(machine);
        assert!(l.chars().count() <= MAX_LABEL_CHARS, "{l}");
        assert!(l.starts_with("tmgr attach (") && l.ends_with(')'));
    }
}

#[test]
fn the_engine_registers_as_a_terminal_with_the_attach_label() {
    let net = Offline::default();
    let d = build(
        net.clone(),
        MemKeyStore::default(),
        RoomCache::memory(),
        &cfg(),
        LOCAL,
    )
    .unwrap();
    assert!(!d.is_paired().unwrap());
    assert_eq!(d.engine.own_device(), None);
    let _ = d.send(&RoomRef::manager(PEER), "hi");
    let sent = net.0.lock().unwrap();
    assert_eq!(sent[0]["kind"], "terminal");
    assert_eq!(sent[0]["label"], device_label(&host_label()));
    assert!(sent[0]["label"]
        .as_str()
        .unwrap()
        .starts_with("tmgr attach ("));
}

#[test]
fn nothing_is_paired_without_the_attach_dir_or_a_config() {
    let t = tempfile::tempdir().unwrap();
    assert!(load_parts(t.path()).unwrap().is_none());
    assert!(open(t.path(), LOCAL).unwrap().is_none());
    assert!(!t.path().join(DIR).exists());
    let dir = config_dir(t.path()).unwrap();
    assert!(load_parts(t.path()).unwrap().is_none());
    dir.save_config(&cfg()).unwrap();
    assert_eq!(load_parts(t.path()).unwrap().unwrap().cfg, cfg());
}

#[test]
fn the_attach_dir_never_touches_the_runners_relay_dir() {
    let t = tempfile::tempdir().unwrap();
    config_dir(t.path()).unwrap().save_config(&cfg()).unwrap();
    assert!(!t.path().join("relay").exists());
}

#[test]
fn an_owner_message_is_valid_and_carries_no_peer_claim() {
    let room = RoomRef::session(PEER, "s_abc");
    let m = owner_message(&room, "hello", 5);
    m.validate().unwrap();
    assert_eq!(m.author.kind, AuthorKind::Owner);
    assert_eq!(m.author.id, "owner");
    assert!(m.author.device.is_none() && m.author.machine.is_none());
    assert_eq!((m.room, m.body_md.as_str(), m.ts_ms), (room, "hello", 5));
}

#[test]
fn send_refuses_local_rooms_empty_and_oversized_bodies_without_a_call() {
    let d = device();
    let local = RoomRef::manager(LOCAL);
    assert!(d.send(&local, "x").unwrap_err().contains("this machine"));
    let room = RoomRef::manager(PEER);
    assert!(d.send(&room, "  ").unwrap_err().contains("empty"));
    let big = "a".repeat(limits::MAX_BODY + 1);
    assert!(d.send(&room, &big).unwrap_err().contains("at most"));
}

#[test]
fn a_failed_send_stores_nothing_in_the_history() {
    let d = device();
    let room = RoomRef::session(PEER, "s_abc");
    assert_eq!(d.send(&room, "hi").unwrap_err(), "offline");
    assert!(d.history(&room, 10).is_empty());
    assert!(d.rooms().is_empty());
}

#[test]
fn status_names_the_three_states() {
    let none = TrustState::default();
    assert_eq!(status_data(&none, false, false).0, "not_paired");
    let dev = |is_self| tmgr_chat_relay::engine::TrustDevice {
        id: "d".into(),
        kind: "desktop".into(),
        label: "l".into(),
        fp: "ab".repeat(32),
        state: DeviceState::Trusted,
        is_self,
        kind_verified: true,
        can_confirm_kind: false,
    };
    let me = TrustState {
        registered: true,
        devices: vec![dev(true)],
        ..TrustState::default()
    };
    assert_eq!(status_data(&me, true, true).0, "pending");
    let both = TrustState {
        registered: true,
        devices: vec![dev(true), dev(false)],
        ..TrustState::default()
    };
    assert_eq!(status_data(&both, true, true).0, "paired");
    assert_eq!(status_data(&both, true, false).0, "not_paired");
    assert!(!status_data(&both, true, true).1.contains(&"ab".repeat(32)));
}

fn info(room: RoomRef, title: &str) -> RoomInfo {
    RoomInfo {
        room,
        title: title.into(),
        agent: None,
        state: None,
        context_pct: None,
        context_tokens: None,
        model: None,
        last: None,
        unrestricted: false,
        mention: None,
    }
}

#[test]
fn only_session_rooms_of_other_machines_go_through_the_device() {
    let d = device();
    assert!(d.handles(&RoomRef::session(PEER, "s_1")));
    assert!(!d.handles(&RoomRef::manager(PEER)));
    assert!(!d.handles(&RoomRef::session(LOCAL, "s_1")));
    assert!(!d.handles(&RoomRef::manager(LOCAL)));
}

#[test]
fn a_remote_session_send_uses_the_device_and_a_remote_manager_send_does_not() {
    let net = Offline::default();
    let d = build(
        net.clone(),
        MemKeyStore::default(),
        RoomCache::memory(),
        &cfg(),
        LOCAL,
    )
    .unwrap();
    let session = RoomRef::session(PEER, "s_1");
    assert!(d.handles(&session));
    assert_eq!(d.send(&session, "hi").unwrap_err(), "offline");
    assert!(!net.0.lock().unwrap().is_empty());
}

#[test]
fn merging_adds_remote_sessions_once_and_leaves_everything_else_as_it_was() {
    let runner = vec![
        info(RoomRef::manager(LOCAL), "Manager"),
        info(RoomRef::session(LOCAL, "s_l"), "local"),
        info(RoomRef::manager(PEER), "laptop"),
    ];
    assert_eq!(merge_rooms(runner.clone(), Vec::new(), LOCAL), runner);
    let device = vec![
        info(RoomRef::manager(PEER), "dup manager"),
        info(RoomRef::session(PEER, "s_r"), "remote"),
        info(RoomRef::session(PEER, "s_r"), "again"),
        info(RoomRef::session(LOCAL, "s_x"), "own machine"),
    ];
    let merged = merge_rooms(runner.clone(), device, LOCAL);
    assert_eq!(merged[..3], runner[..]);
    assert_eq!(merged.len(), 4);
    assert_eq!(merged[3].title, "remote");
}

fn index(machine: &str, label: &str, session: &str) -> tmgr_chat_relay::rooms::RoomsIndex {
    tmgr_chat_relay::rooms::RoomsIndex::parse(serde_json::json!({
        "v": 1,
        "machine": machine,
        "machine_label": label,
        "role": "top",
        "rooms": [{"room": {"kind": "session", "machine": machine, "session": session}, "title": session}]
    }))
    .unwrap()
}

#[test]
fn merging_uses_the_devices_own_machine_even_when_the_first_runner_room_is_remote() {
    let d = device();
    d.cache.set_index(index(PEER, "Studio", "s_r")).unwrap();
    let runner = vec![
        info(RoomRef::manager(PEER), "laptop"),
        info(RoomRef::manager(LOCAL), "Manager"),
    ];
    let merged = d.merge(runner);
    assert_eq!(merged.len(), 3);
    assert_eq!(merged[2].room, RoomRef::session(PEER, "s_r"));
}

#[test]
fn machines_sharing_a_label_get_an_id_suffix() {
    let d = device();
    let other = "mx_00000000000000cc";
    d.cache.set_index(index(PEER, "Studio", "s_1")).unwrap();
    d.cache.set_index(index(other, "Studio", "s_2")).unwrap();
    let labels: std::collections::BTreeMap<_, _> = d.machines().into_iter().collect();
    assert_ne!(labels[PEER], labels[other]);
    assert!(labels[PEER].starts_with("Studio") && labels[PEER].ends_with("00bb)"));
    assert!(labels[other].ends_with("00cc)"));
    let d = device();
    d.cache.set_index(index(PEER, "Studio", "s_1")).unwrap();
    assert_eq!(d.machines(), vec![(PEER.to_owned(), "Studio".to_owned())]);
}

#[test]
fn only_one_attach_holds_the_device_lock() {
    let t = tempfile::tempdir().unwrap();
    config_dir(t.path()).unwrap().save_config(&cfg()).unwrap();
    let first = lock::acquire(t.path()).unwrap().expect("first lock");
    assert!(lock::acquire(t.path()).unwrap().is_none());
    drop(first);
    assert!(lock::acquire(t.path()).unwrap().is_some());
}

#[test]
fn the_lock_file_is_never_followed_through_a_symlink() {
    let t = tempfile::tempdir().unwrap();
    config_dir(t.path()).unwrap().save_config(&cfg()).unwrap();
    let target = t.path().join("elsewhere");
    std::fs::write(&target, "keep").unwrap();
    std::os::unix::fs::symlink(&target, t.path().join(DIR).join(lock::FILE)).unwrap();
    assert!(lock::acquire(t.path()).is_err());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "keep");
}

#[test]
fn a_held_lock_means_no_device_and_no_store_access() {
    let t = tempfile::tempdir().unwrap();
    config_dir(t.path()).unwrap().save_config(&cfg()).unwrap();
    let _held = lock::acquire(t.path()).unwrap().unwrap();
    let opened = open_with::<Offline, MemKeyStore>(t.path(), LOCAL, |_, _| {
        panic!("the store must not be opened while another attach holds the lock")
    });
    assert!(opened.unwrap().is_none());
}

#[test]
fn an_agent_session_never_opens_the_device() {
    let t = tempfile::tempdir().unwrap();
    config_dir(t.path()).unwrap().save_config(&cfg()).unwrap();
    assert!(super::super::attach_chat::open_in(true, t.path()).is_none());
    assert!(!t.path().join(DIR).join(lock::FILE).exists());
}

#[test]
fn unpair_refuses_while_an_attach_holds_the_lock() {
    let t = tempfile::tempdir().unwrap();
    config_dir(t.path()).unwrap().save_config(&cfg()).unwrap();
    let _held = lock::acquire(t.path()).unwrap().unwrap();
    let e = unpair_with::<Offline, MemKeyStore>(t.path(), |_| {
        panic!("the store must not be opened while another attach holds the lock")
    })
    .unwrap_err();
    assert!(e.to_string().contains("Close tmgr attach first"), "{e}");
}

mod parsing {
    use crate::cli::{Cli, Command};
    use crate::cmd::runner::{AttachArgs, DeviceCmd, RunnerCommands};
    use clap::Parser;

    fn attach(args: &[&str]) -> Result<AttachArgs, clap::Error> {
        let argv = ["tmgr", "attach"].iter().chain(args);
        match Cli::try_parse_from(argv)?.command {
            Some(Command::Runner(RunnerCommands::Attach(a))) => Ok(a),
            _ => panic!("not attach"),
        }
    }

    fn device(args: &[&str]) -> Result<DeviceCmd, clap::Error> {
        let argv = ["tmgr", "device"].iter().chain(args);
        match Cli::try_parse_from(argv)?.command {
            Some(Command::Device { command }) => Ok(command),
            _ => panic!("not device"),
        }
    }

    #[test]
    fn a_session_named_like_a_device_command_still_attaches() {
        for name in ["pair", "unpair", "device", "status"] {
            let a = attach(&[name]).unwrap();
            assert_eq!(a.session.as_deref(), Some(name));
        }
    }

    #[test]
    fn the_existing_attach_forms_parse_as_before() {
        let a = attach(&[]).unwrap();
        assert!(a.session.is_none() && a.on.is_none());
        let a = attach(&["s_abc", "--no-mouse", "--no-links"]).unwrap();
        assert!(a.no_mouse && a.no_links);
        let a = attach(&["--on", "link", "--read-only"]).unwrap();
        assert!(a.read_only);
        assert!(attach(&["--read-only"]).is_err());
    }

    #[test]
    fn the_device_commands_parse() {
        assert!(matches!(device(&["pair"]).unwrap(), DeviceCmd::Pair));
        assert!(matches!(device(&["unpair"]).unwrap(), DeviceCmd::Unpair));
        assert!(matches!(device(&["status"]).unwrap(), DeviceCmd::Status));
        assert!(device(&["pair", "extra"]).is_err());
        assert!(device(&[]).is_err());
    }
}

fn runner_dev(id: &str, kind: &str, state: DeviceState) -> tmgr_chat_relay::engine::TrustDevice {
    tmgr_chat_relay::engine::TrustDevice {
        id: id.into(),
        kind: kind.into(),
        label: format!("label {id}"),
        fp: id.repeat(32),
        state,
        is_self: false,
        kind_verified: true,
        can_confirm_kind: false,
    }
}

fn note(cache: &RoomCache, device: &str, machine: &str, capable: bool) {
    use tmgr_chat_relay::rooms::{Role, ROOMS_V};
    let index = tmgr_chat_relay::rooms::RoomsIndex {
        v: ROOMS_V,
        machine: machine.into(),
        machine_label: "Laptop".into(),
        role: Role::Top,
        followers: None,
        rooms: Vec::new(),
        activity: false,
        blob: false,
        terminal_input: capable,
    };
    cache.note_runner(device, &index).unwrap();
}

fn terminal_device(net: &Offline) -> AttachDevice<Offline, MemKeyStore> {
    build(
        net.clone(),
        MemKeyStore::default(),
        RoomCache::memory(),
        &cfg(),
        LOCAL,
    )
    .unwrap()
}

fn runner(device: &str, machine: &str, capable: bool) -> RunnerCap {
    RunnerCap {
        device_id: device.into(),
        machine: machine.into(),
        machine_label: "Laptop".into(),
        capable,
    }
}

#[test]
fn the_refusal_text_is_exact() {
    assert_eq!(
        terminal_refusal("Laptop"),
        "Runner on Laptop does not accept terminal input. Update tmgr there"
    );
}

#[test]
fn capable_runners_lists_only_trusted_runners_not_being_revoked_with_their_flag() {
    let cache = RoomCache::memory();
    note(&cache, "a", PEER, true);
    note(&cache, "b", PEER, false);
    note(&cache, "c", PEER, true);
    note(&cache, "d", PEER, true);
    let st = TrustState {
        registered: true,
        devices: vec![
            runner_dev("a", "runner", DeviceState::Trusted),
            runner_dev("b", "runner", DeviceState::Trusted),
            runner_dev("c", "runner", DeviceState::Revoked),
            runner_dev("d", "runner", DeviceState::Trusted),
            runner_dev("e", "runner", DeviceState::Trusted),
            runner_dev("f", "mobile", DeviceState::Trusted),
        ],
        pending_revokes: vec![tmgr_chat_relay::engine::PendingRevoke {
            statement_id: "s".into(),
            revoker_label: "r".into(),
            revoker_fp: "r".into(),
            target_id: "d".into(),
            target_label: "d".into(),
            target_fp: "d".into(),
        }],
        ..TrustState::default()
    };
    let got: Vec<_> = capable_runners(&st, &cache)
        .into_iter()
        .map(|r| (r.device_id, r.machine, r.machine_label, r.capable))
        .collect();
    assert_eq!(
        got,
        [
            ("a".into(), PEER.into(), "Laptop".into(), true),
            ("b".into(), PEER.into(), "Laptop".into(), false),
            ("e".into(), String::new(), "label e".into(), false),
        ]
    );
}

#[test]
fn a_terminal_attach_does_not_send_to_a_machine_whose_runners_cannot_take_terminal_input() {
    let net = Offline::default();
    let d = terminal_device(&net);
    let room = RoomRef::session(PEER, "s_1");
    let e = d
        .send_with(&room, "hi", &[runner("a", PEER, false)])
        .unwrap_err();
    assert_eq!(e, terminal_refusal("Laptop"));
    assert!(net.0.lock().unwrap().is_empty());
}

#[test]
fn a_refused_terminal_send_names_the_machine_even_without_a_label() {
    let net = Offline::default();
    let d = terminal_device(&net);
    let room = RoomRef::session(PEER, "s_1");
    let mut unlabelled = runner("a", PEER, false);
    unlabelled.machine_label = String::new();
    let e = d.send_with(&room, "hi", &[unlabelled]).unwrap_err();
    assert!(e.contains(PEER) && e.contains("Update tmgr there"), "{e}");
}

#[test]
fn a_terminal_attach_still_sends_to_capable_machines() {
    let net = Offline::default();
    let d = terminal_device(&net);
    let room = RoomRef::session(PEER, "s_1");
    let runners = [runner("a", PEER, false), runner("b", PEER, true)];
    assert_eq!(d.send_with(&room, "hi", &runners).unwrap_err(), "offline");
    let other = RoomRef::session("mx_00000000000000cc", "s_1");
    let runners = [
        runner("a", PEER, false),
        runner("c", "mx_00000000000000cc", true),
    ];
    assert_eq!(d.send_with(&other, "hi", &runners).unwrap_err(), "offline");
    assert_eq!(d.send_with(&room, "hi", &[]).unwrap_err(), "offline");
}

#[test]
fn a_failed_send_to_an_unconfirmed_machine_leaves_no_unconfirmed_note() {
    let net = Offline::default();
    let d = terminal_device(&net);
    let room = RoomRef::session(PEER, "s_1");
    assert_eq!(d.send_with(&room, "hi", &[]).unwrap_err(), "offline");
    assert!(d.receipt_notes().is_empty());
    let capable = [runner("a", PEER, true)];
    assert_eq!(d.send_with(&room, "hi", &capable).unwrap_err(), "offline");
    assert!(d.receipt_notes().is_empty());
}

#[test]
fn only_failed_receipts_become_notes_with_their_reason() {
    use tmgr_chat_relay::engine::{Receipt, ReceiptState};
    let r = |state, reason: Option<&str>| Receipt {
        id: "m_x".into(),
        state,
        reason: reason.map(Into::into),
    };
    assert_eq!(
        receipt_note(&r(ReceiptState::Failed, Some("older than 10 minutes."))).as_deref(),
        Some("remote send failed: older than 10 minutes.")
    );
    assert_eq!(
        receipt_note(&r(ReceiptState::Failed, None)).as_deref(),
        Some("remote send failed")
    );
    assert_eq!(receipt_note(&r(ReceiptState::Delivered, None)), None);
    assert_eq!(receipt_note(&r(ReceiptState::Queued, Some("turn"))), None);
}

#[test]
fn runner_controlled_text_is_stripped_of_terminal_escapes() {
    let evil = "Box\u{1b}[2J\u{7}x";
    for text in [
        terminal_refusal(evil),
        name_runners(&machine_names(&["r".to_owned()], |_| Some(evil.to_owned()))),
        receipt_note(&Receipt {
            id: "m".into(),
            state: ReceiptState::Failed,
            reason: Some(evil.into()),
        })
        .unwrap(),
    ] {
        assert!(
            !text.contains('\u{1b}') && !text.contains('\u{7}'),
            "{text:?}"
        );
    }
}

#[test]
fn only_a_remote_session_room_of_a_paired_device_leaves_the_1_0_187_paths() {
    let rooms = [
        (RoomRef::manager(LOCAL), Route::Daemon, Route::Daemon),
        (RoomRef::session(LOCAL, "s_1"), Route::Daemon, Route::Daemon),
        (RoomRef::manager(PEER), Route::Daemon, Route::Daemon),
        (RoomRef::session(PEER, "s_1"), Route::Daemon, Route::Device),
    ];
    for (room, unpaired, paired) in rooms {
        assert_eq!(route(LOCAL, false, &room), unpaired, "{room:?} unpaired");
        assert_eq!(route(LOCAL, true, &room), paired, "{room:?} paired");
    }
}

#[test]
fn a_non_capable_runner_never_diverts_a_remote_manager_room_from_the_old_path() {
    let net = Offline::default();
    let d = terminal_device(&net);
    note(&d.cache, "a", PEER, false);
    assert!(!d.handles(&RoomRef::manager(PEER)));
    assert!(!d.handles(&RoomRef::manager(LOCAL)));
    assert!(!d.handles(&RoomRef::session(LOCAL, "s_1")));
    assert!(d.handles(&RoomRef::session(PEER, "s_1")));
    assert!(net.0.lock().unwrap().is_empty());
}

#[test]
fn a_403_on_register_names_the_scope_and_the_login_command_that_grants_it() {
    let forbidden =
        crate::cmd::chat::transport::failure(403, None, r#"{"error":"forbidden"}"#.into());
    let e = pair_error(&forbidden);
    assert_eq!(e.exit_code(), 5);
    let hint = e.hint.clone().unwrap_or_default();
    assert!(e.message.contains("chat:devices"), "{}", e.message);
    assert!(
        hint.starts_with("tmgr login --scopes ") && hint.ends_with(",chat:devices"),
        "{hint}"
    );
    assert!(hint.contains("tasks:read"), "{hint}");
}

#[test]
fn a_401_on_register_asks_to_log_in_and_other_errors_hint_a_fresh_code() {
    let unauthorized = crate::cmd::chat::transport::failure(401, None, String::new());
    let e = pair_error(&unauthorized);
    assert_eq!(e.exit_code(), 4);
    assert_eq!(e.hint.as_deref(), Some("tmgr login"));
    let other = pair_error("relay unreachable");
    assert_eq!(other.exit_code(), 1);
    assert!(other.hint.unwrap_or_default().contains("fresh join code"));
}

#[test]
fn a_forged_status_in_a_body_does_not_pass_for_a_403() {
    let forged = crate::cmd::chat::transport::failure(
        500,
        None,
        r#"{"error":"x","http_status":403}"#.into(),
    );
    assert_eq!(pair_error(&forged).exit_code(), 1);
}
