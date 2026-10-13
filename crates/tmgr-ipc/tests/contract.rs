use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use std::fmt::Debug;
use tmgr_ipc::api::*;

fn fixture(name: &str) -> Value {
    let path = format!("{}/tests/fixtures/{name}.json", env!("CARGO_MANIFEST_DIR"));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap()
}

fn golden<T: Serialize + DeserializeOwned + PartialEq + Debug>(name: &str, value: &T) {
    let want = fixture(name);
    assert_eq!(
        &serde_json::to_value(value).unwrap(),
        &want,
        "{name}: encode"
    );
    let back: T = serde_json::from_value(want).unwrap();
    assert_eq!(&back, value, "{name}: decode");
}

const SID: &str = "s_abcdefghij";
const TASK: TaskRef = TaskRef {
    workspace_id: -3,
    task_id: 412,
};

fn state(kind: StateKind, detail: Option<&str>, exit_code: Option<i32>, since_ms: u64) -> State {
    State {
        kind,
        since_ms,
        detail: detail.map(String::from),
        exit_code,
    }
}

fn plain(text: &str) -> Span {
    Span {
        a: Attr {
            fg: Color::Default,
            bg: Color::Default,
            flags: 0,
        },
        t: text.into(),
    }
}

fn modes(ac: bool, bp: bool, alt: bool, hc: bool) -> Modes {
    Modes {
        app_cursor: ac,
        bracketed_paste: bp,
        alt_screen: alt,
        hide_cursor: hc,
    }
}

fn snapshot() -> Snapshot {
    Snapshot {
        rows: 2,
        cols: 4,
        lines: vec![
            Line {
                spans: vec![
                    plain("ab"),
                    Span {
                        a: Attr {
                            fg: Color::Idx(1),
                            bg: Color::Rgb(10, 20, 30),
                            flags: BOLD | ITALIC,
                        },
                        t: "cd".into(),
                    },
                ],
            },
            Line { spans: vec![] },
        ],
        cursor: Some((0, 1)),
        modes: modes(false, true, false, false),
        scroll_offset: 0,
        scroll_max: 12,
    }
}

fn session_info() -> SessionInfo {
    SessionInfo {
        id: SID.into(),
        name: "fix login".into(),
        argv: vec![
            "/usr/local/bin/codex".into(),
            "--model".into(),
            "gpt-5".into(),
        ],
        cwd: "/work/project".into(),
        profile: Some("codex".into()),
        state: state(StateKind::Working, None, None, 1_760_000_000_000),
        created_ms: 1_759_999_990_000,
        cols: 120,
        rows: 40,
        shim_pid: 501,
        child_pid: 502,
        task: Some(TASK),
    }
}

#[test]
fn handshake_messages() {
    golden(
        "hello",
        &Handshake::Hello(Hello {
            proto: 1,
            client: "desktop".into(),
            version: "0.3.0".into(),
            pid: 4242,
            nonce_c: "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff".into(),
        }),
    );
    golden(
        "challenge",
        &Handshake::Challenge(Challenge {
            nonce_s: "ffeeddccbbaa99887766554433221100ffeeddccbbaa99887766554433221100".into(),
            proof_s: "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7".into(),
        }),
    );
    golden(
        "auth",
        &Handshake::Auth(Auth {
            proof_c: "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843".into(),
        }),
    );
    golden(
        "welcome",
        &Handshake::Welcome(Welcome {
            proto: 1,
            runner_version: "0.3.0".into(),
            caps: caps::ALL.map(String::from).to_vec(),
        }),
    );
    golden(
        "handshake_error",
        &Handshake::Error(ErrorBody {
            code: ErrorCode::ProtoMismatch,
            message: "server speaks protocol 1, client 2".into(),
        }),
    );
}

#[test]
fn requests_and_responses() {
    let run = RunCreateReq {
        profile: "codex".into(),
        task: Some(TASK),
        cwd: "/work/project".into(),
        name: Some("fix login".into()),
        model: Some("gpt-5".into()),
        cols: 120,
        rows: 40,
    };
    golden("run_create_req", &run);
    golden(
        "request_runs_create",
        &ClientMsg::Request(Request {
            id: 7,
            method: method::RUNS_CREATE.into(),
            params: serde_json::to_value(&run).unwrap(),
        }),
    );
    golden(
        "response_ok",
        &ServerMsg::Response(Response {
            id: 7,
            ok: Some(json!({"pid": 100})),
            err: None,
        }),
    );
    golden(
        "response_err",
        &ServerMsg::Response(Response {
            id: 8,
            ok: None,
            err: Some(ErrorBody {
                code: ErrorCode::UnknownProfile,
                message: "unknown agent profile \"nope\"".into(),
            }),
        }),
    );
    golden(
        "response_err_future_code",
        &ServerMsg::Response(Response {
            id: 9,
            ok: None,
            err: Some(ErrorBody {
                code: ErrorCode::Other("rate_limited".into()),
                message: "slow down".into(),
            }),
        }),
    );
}

#[test]
fn method_params_and_results() {
    golden(
        "session_create_req",
        &CreateReq {
            argv: vec!["/bin/sh".into(), "-c".into(), "true".into()],
            cwd: "/work/project".into(),
            name: Some("scratch".into()),
            profile: Some("codex".into()),
            cols: 80,
            rows: 24,
            env_pass: vec!["FOO".into()],
            task: Some(TASK),
        },
    );
    golden("session_info", &session_info());
    golden(
        "session_info_notask",
        &SessionInfo {
            id: SID.into(),
            name: "sh".into(),
            argv: vec!["/bin/sh".into()],
            cwd: "/work/project".into(),
            profile: None,
            state: state(StateKind::Exited, None, Some(0), 1_760_000_005_000),
            created_ms: 1_759_999_990_000,
            cols: 80,
            rows: 24,
            shim_pid: 501,
            child_pid: 502,
            task: None,
        },
    );
    golden(
        "status_info",
        &StatusInfo {
            pid: 321,
            version: "0.3.0".into(),
            proto: 1,
            uptime_s: 3600,
            sessions: 2,
        },
    );
    golden(
        "attach_params",
        &AttachParams {
            sessions: vec![SID.into()],
            sizes: [(SID.to_string(), (100, 30))].into(),
        },
    );
    golden(
        "kill_params",
        &KillParams {
            id: SID.into(),
            signal: "term".into(),
        },
    );
    golden(
        "send_params",
        &SendParams {
            id: SID.into(),
            text: "hello\r".into(),
        },
    );
    golden("id_params", &IdParams { id: SID.into() });
    golden(
        "layout_set_params",
        &LayoutSetParams {
            layout: json!({"panes": [SID]}),
        },
    );
}

#[test]
fn pushes_and_stream_messages() {
    let event = EventMsg {
        session: SID.into(),
        state: state(
            StateKind::Blocked,
            Some("Allow command?"),
            None,
            1_760_000_001_000,
        ),
        task: Some(TASK),
    };
    golden("event_msg", &event);
    golden(
        "event_msg_notask",
        &EventMsg {
            session: SID.into(),
            state: state(StateKind::Waiting, None, None, 1_760_000_001_000),
            task: None,
        },
    );
    golden("snapshot", &snapshot());
    golden(
        "screen_push_full",
        &ScreenPush {
            session: SID.into(),
            full: true,
            snapshot: Some(Snapshot {
                rows: 1,
                cols: 4,
                lines: vec![Line {
                    spans: vec![plain("hi")],
                }],
                cursor: Some((0, 2)),
                modes: modes(false, false, false, false),
                scroll_offset: 0,
                scroll_max: 0,
            }),
            lines: vec![],
            cursor: Some((0, 2)),
            modes: Some(modes(false, false, false, false)),
            scroll_offset: 0,
            scroll_max: 0,
        },
    );
    golden(
        "screen_push_diff",
        &ScreenPush {
            session: SID.into(),
            full: false,
            snapshot: None,
            lines: vec![(
                3,
                Line {
                    spans: vec![Span {
                        a: Attr {
                            fg: Color::Idx(2),
                            bg: Color::Default,
                            flags: BOLD,
                        },
                        t: "x".into(),
                    }],
                },
            )],
            cursor: None,
            modes: Some(modes(true, false, true, true)),
            scroll_offset: 2,
            scroll_max: 9,
        },
    );
    golden(
        "server_event",
        &ServerMsg::Event(
            serde_json::to_value(EventMsg {
                session: SID.into(),
                state: state(StateKind::Starting, None, None, 1_760_000_000_000),
                task: None,
            })
            .unwrap(),
        ),
    );
    golden(
        "server_closed",
        &ServerMsg::Closed {
            reason: "client too slow".into(),
        },
    );
    golden(
        "client_resize",
        &ClientMsg::Resize(Resize {
            session: SID.into(),
            cols: 100,
            rows: 30,
        }),
    );
    golden(
        "client_scroll",
        &ClientMsg::Scroll(Scroll {
            session: SID.into(),
            offset: 15,
        }),
    );
    golden(
        "client_focus",
        &ClientMsg::Focus(Focus {
            session: SID.into(),
        }),
    );
    golden("client_detach", &ClientMsg::Detach);

    let screen: ServerMsg = serde_json::from_value(fixture("server_screen")).unwrap();
    let push = screen.screen().expect("typed screen push");
    assert_eq!(push.cursor, Some((1, 1)));
    assert!(push.modes.is_none());
    let ev = ServerMsg::Event(serde_json::to_value(&event).unwrap());
    assert_eq!(ev.event(), Some(event));
}

#[test]
fn error_codes_use_todays_strings_and_keep_unknown_ones() {
    let known = [
        (ErrorCode::InvalidParams, "invalid_params"),
        (ErrorCode::UnknownMethod, "unknown_method"),
        (ErrorCode::UnknownProfile, "unknown_profile"),
        (ErrorCode::NotFound, "not_found"),
        (ErrorCode::NotRunning, "not_running"),
        (ErrorCode::Busy, "busy"),
        (ErrorCode::Limit, "limit"),
        (ErrorCode::Sandbox, "sandbox"),
        (ErrorCode::Unauthorized, "unauthorized"),
        (ErrorCode::ProtoMismatch, "proto_mismatch"),
        (ErrorCode::Internal, "internal"),
        (ErrorCode::Forbidden, "forbidden"),
    ];
    for (code, text) in known {
        assert_eq!(serde_json::to_value(&code).unwrap(), json!(text));
        assert_eq!(
            serde_json::from_value::<ErrorCode>(json!(text)).unwrap(),
            code
        );
        assert_eq!(code.to_string(), text);
    }
    let future: ErrorCode = serde_json::from_value(json!("quota_exceeded")).unwrap();
    assert_eq!(future, ErrorCode::Other("quota_exceeded".into()));
    assert_eq!(
        serde_json::to_value(&future).unwrap(),
        json!("quota_exceeded")
    );
}

#[test]
fn constants_are_pinned() {
    assert_eq!(PROTO_VERSION, 1);
    assert_eq!(
        caps::ALL,
        [
            "sessions.v1",
            "attach.v1",
            "runs.create.v1",
            "task_ref.v1",
            "layout.v1"
        ]
    );
    let methods = [
        method::RUNNER_STATUS,
        method::RUNNER_STOP,
        method::SESSIONS_LIST,
        method::SESSIONS_CREATE,
        method::SESSIONS_KILL,
        method::SESSIONS_SEND,
        method::SESSIONS_REMOVE,
        method::RUNS_CREATE,
        method::LAYOUT_GET,
        method::LAYOUT_SET,
        method::ATTACH_OPEN,
        method::ATTACH_SET,
    ];
    assert_eq!(
        methods,
        [
            "runner.status",
            "runner.stop",
            "sessions.list",
            "sessions.create",
            "sessions.kill",
            "sessions.send",
            "sessions.remove",
            "runs.create",
            "layout.get",
            "layout.set",
            "attach.open",
            "attach.set",
        ]
    );
    assert_eq!(CLIENT_CLI, "cli");
}

#[test]
fn unknown_fields_are_ignored_everywhere() {
    let mut info = fixture("session_info");
    info["future_field"] = json!({"a": 1});
    info["state"]["future"] = json!(true);
    info["task"]["future"] = json!("x");
    assert_eq!(
        serde_json::from_value::<SessionInfo>(info).unwrap(),
        session_info()
    );

    let mut welcome = fixture("welcome");
    welcome["extra"] = json!([1, 2]);
    let Handshake::Welcome(w) = serde_json::from_value(welcome).unwrap() else {
        panic!("welcome expected");
    };
    assert_eq!(w.caps.len(), 5);

    let mut push = fixture("screen_push_diff");
    push["future"] = json!(1);
    serde_json::from_value::<ScreenPush>(push).unwrap();

    let mut req = fixture("run_create_req");
    req["future"] = json!(1);
    serde_json::from_value::<RunCreateReq>(req).unwrap();

    let mut err = fixture("response_err");
    err["err"]["hint"] = json!("later");
    serde_json::from_value::<ServerMsg>(err).unwrap();
}

#[test]
fn optional_fields_may_be_absent() {
    let req: RunCreateReq =
        serde_json::from_value(json!({"profile": "codex", "cwd": "/work"})).unwrap();
    assert_eq!(
        (req.cols, req.rows, req.task, req.name, req.model),
        (80, 24, None, None, None)
    );
    let mut info = fixture("session_info");
    info.as_object_mut().unwrap().remove("task");
    assert_eq!(
        serde_json::from_value::<SessionInfo>(info).unwrap().task,
        None
    );
    let hello: Handshake = serde_json::from_value(json!({"type": "hello", "proto": 2})).unwrap();
    assert!(matches!(hello, Handshake::Hello(h) if h.proto == 2));
    let attach: AttachParams = serde_json::from_value(json!({})).unwrap();
    assert_eq!(attach, AttachParams::default());
}

#[test]
fn unknown_push_types_surface_as_unknown() {
    let msg: ServerMsg =
        serde_json::from_value(json!({"type": "hologram", "data": {"x": 1}})).unwrap();
    assert_eq!(msg, ServerMsg::Unknown);
    assert!(msg.event().is_none() && msg.screen().is_none());
}

#[cfg(unix)]
mod handshake {
    use super::*;
    use std::os::unix::net::UnixStream;
    use std::thread;
    use tmgr_ipc::frame::{read_frame, write_json, Frame};
    use tmgr_ipc::{auth, server};

    const SECRET: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn known_answer_proofs() {
        let (nc, ns) = ("aa".repeat(32), "bb".repeat(32));
        assert_eq!(
            auth::proof(SECRET, "tmgr-runner-server", &nc, &ns),
            "a490348a2f8a10158c9911d731984007d11ac37d6c5f900e3d6de242b09156fb"
        );
        assert_eq!(
            auth::proof(SECRET, "tmgr-runner-client", &ns, &nc),
            "9067d3e30032ebc623682f4022e80aee006ecc41efae880905b3e3e34eac0bbb"
        );
    }

    #[test]
    fn welcome_carries_every_capability() {
        let (a, b) = UnixStream::pair().unwrap();
        let srv = thread::spawn(move || server::handshake_server(&b, SECRET, "9.9.9").unwrap());
        let welcome = server::handshake_client(&a, "desktop", SECRET).unwrap();
        assert_eq!(welcome.proto, PROTO_VERSION);
        assert_eq!(welcome.runner_version, "9.9.9");
        assert_eq!(welcome.caps, caps::ALL);
        assert_eq!(srv.join().unwrap().client, "desktop");
    }

    #[test]
    fn major_mismatch_is_reported_before_anything_else() {
        let (a, b) = UnixStream::pair().unwrap();
        let srv = thread::spawn(move || server::handshake_server(&b, SECRET, "9.9.9"));
        write_json(
            &mut &a,
            &json!({"type": "hello", "proto": 2, "client": "desktop", "novel": [1]}),
        )
        .unwrap();
        let Some(Frame::Json(v)) = read_frame(&mut &a).unwrap() else {
            panic!("json frame expected");
        };
        assert_eq!(v["type"], "error");
        assert_eq!(v["code"], "proto_mismatch");
        assert!(srv.join().unwrap().is_err());
        assert!(read_frame(&mut &a).unwrap().is_none());
    }

    #[test]
    fn wrong_secret_is_unauthorized() {
        let (a, b) = UnixStream::pair().unwrap();
        let srv = thread::spawn(move || server::handshake_server(&b, SECRET, "9.9.9"));
        let wrong = "1".repeat(64);
        let e = server::handshake_client(&a, "cli", &wrong).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::PermissionDenied);
        drop(a);
        assert!(srv.join().unwrap().is_err());
    }
}
