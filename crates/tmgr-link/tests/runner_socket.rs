use serde_json::{json, Value};
use std::io::Cursor;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tmgr_ipc::endpoint::{bind, Endpoint};
use tmgr_ipc::frame::{read_frame, write_json, Frame};
use tmgr_ipc::proto::{ClientMsg, ErrorBody, Response, ServerMsg};
use tmgr_ipc::{secret, server};
use tmgr_link::audit::Audit;
use tmgr_link::proto::{self, Framed, MAX_FRAME};
use tmgr_link::runner::{Backend, RunnerClient};
use tmgr_link::scope::Scope;
use tmgr_link::store::{Leader, Paths, Store};
use tmgr_link::taskref::LocalRefs;
use tmgr_link::worker::{serve_with, Env, Limits, Runtime};

const ENR: &str = "0123456789abcdef0123456789abcdef";
const SID: &str = "s_abcdefghij";

#[derive(Default)]
struct Log {
    hellos: Vec<String>,
    methods: Vec<String>,
    exited: bool,
}

struct Runner {
    _t: tempfile::TempDir,
    dir: PathBuf,
    log: Arc<Mutex<Log>>,
}

fn session(cwd: &str, exited: bool) -> Value {
    json!({
        "id": SID, "name": "n", "argv": ["/bin/true"], "cwd": cwd, "profile": "codex",
        "state": {"kind": if exited {"exited"} else {"working"}, "since_ms": 1, "detail": null, "exit_code": null},
        "created_ms": 1, "cols": 120, "rows": 40, "shim_pid": 1, "child_pid": 2,
        "task": {"workspace_id": -1, "task_id": 7}
    })
}

fn handle(stream: UnixStream, log: Arc<Mutex<Log>>, secret_text: String) {
    let Ok(hello) = server::handshake_server(&stream, &secret_text, "9.9.9") else {
        return;
    };
    log.lock().unwrap().hellos.push(hello.client);
    let mut s = &stream;
    while let Ok(Some(Frame::Json(v))) = read_frame(&mut s) {
        let Ok(msg) = serde_json::from_value::<ClientMsg>(v) else {
            return;
        };
        let ClientMsg::Request(req) = msg else {
            if matches!(msg, ClientMsg::Detach) {
                return;
            }
            continue;
        };
        log.lock().unwrap().methods.push(req.method.clone());
        let cwd = req
            .params
            .get("cwd")
            .and_then(Value::as_str)
            .unwrap_or("/")
            .to_string();
        let reply = match req.method.as_str() {
            "sessions.list" => Ok(json!([session("/", log.lock().unwrap().exited)])),
            "runs.create" => Ok(session(&cwd, false)),
            "sessions.kill" => {
                log.lock().unwrap().exited = true;
                Ok(Value::Null)
            }
            "sessions.send" | "attach.open" => Ok(Value::Null),
            "sessions.create" => Err((
                "forbidden",
                "raw commands are accepted only from the tmgr CLI",
            )),
            _ => Err(("unknown_method", "unknown")),
        };
        let resp = match reply {
            Ok(ok) => Response {
                id: req.id,
                ok: Some(ok),
                err: None,
            },
            Err((code, message)) => Response {
                id: req.id,
                ok: None,
                err: Some(ErrorBody {
                    code: code.into(),
                    message: message.into(),
                }),
            },
        };
        if write_json(&mut s, &ServerMsg::Response(resp)).is_err() {
            return;
        }
        if req.method == "attach.open" {
            let screen = json!({
                "session": SID, "full": true,
                "snapshot": {"r":1,"c":10,"l":[[{"a":{"f":null,"b":null,"x":0},"t":"ok\u{1b}[2J"}]],
                             "cu":null,"m":{"ac":false,"bp":false,"alt":false,"hc":false},"so":0,"sm":0},
                "lines": [], "cursor": null, "modes": null, "scroll_offset": 0, "scroll_max": 0
            });
            let _ = write_json(&mut s, &ServerMsg::Screen(screen));
        }
    }
}

fn runner(secret_in_file: &str, secret_served: &'static str) -> Runner {
    let t = tempfile::Builder::new()
        .prefix("tr")
        .tempdir_in("/tmp")
        .unwrap();
    std::fs::set_permissions(
        t.path(),
        <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700),
    )
    .unwrap();
    let dir = t.path().join("r");
    let ep = Endpoint::new_runner(&dir).unwrap();
    let listener = bind(&ep).unwrap();
    std::fs::write(dir.join("client.secret"), secret_in_file).unwrap();
    std::fs::set_permissions(
        dir.join("client.secret"),
        <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o600),
    )
    .unwrap();
    let log = Arc::new(Mutex::new(Log::default()));
    let l2 = log.clone();
    std::thread::spawn(move || {
        while let Ok((stream, _)) = listener.accept() {
            let log = l2.clone();
            std::thread::spawn(move || handle(stream, log, secret_served.to_string()));
        }
    });
    Runner { _t: t, dir, log }
}

struct World {
    _d: tempfile::TempDir,
    paths: Paths,
    repo: String,
}

fn world() -> World {
    let d = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(d.path()).unwrap();
    let paths = Paths::under(&root);
    let repo = root.join("repo");
    std::fs::create_dir(&repo).unwrap();
    let repo = repo.to_str().unwrap().to_string();
    Store::update(&paths, |s| {
        s.leaders.insert(
            "laptop".into(),
            Leader {
                key: "AAAA".into(),
                fingerprint: "SHA256:x".into(),
                added: "2026-10-13T00:00:00Z".into(),
                scope: Scope {
                    repos: vec![repo.clone()],
                    agents: vec!["codex".into()],
                    models: vec![],
                    max: 2,
                    from: vec![],
                    allow_attach_input: true,
                },
                enrollment: ENR.into(),
                authorized_keys: String::new(),
            },
        );
        Ok(())
    })
    .unwrap();
    World { _d: d, paths, repo }
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

#[test]
fn the_worker_speaks_to_the_runner_as_worker_and_never_calls_sessions_create() {
    let r = runner("the-secret", "the-secret");
    let w = world();
    let dir = r.dir.clone();
    let connect = move || RunnerClient::connect(&dir).map(|c| Box::new(c) as Box<dyn Backend>);
    let rt = Runtime {
        connect: &connect,
        resolver: &LocalRefs,
    };
    let items = [
        json!({"id":1,"method":"runs.create","params":{"task":"w1:7","repo":w.repo,"agent":"codex"}}),
        json!({"id":2,"method":"runs.list"}),
        json!({"id":3,"method":"runs.get","params":{"run":SID}}),
        json!({"id":4,"method":"runs.halt","params":{"run":SID}}),
        json!({"id":5,"method":"sessions.create","params":{"argv":["/bin/sh"],"cwd":w.repo}}),
        json!({"id":6,"method":"runs.create","params":{"task":"w1:7","repo":w.repo,"agent":"codex","argv":["/bin/sh"]}}),
    ];
    let mut out = Vec::new();
    {
        let env = Env {
            ssh_connection: Some("192.0.2.5 5555 192.0.2.1 22".into()),
            original_command: None,
        };
        let mut chan = Framed::new(Cursor::new(frames(&items)), &mut out);
        serve_with(
            &w.paths,
            "laptop",
            ENR,
            &env,
            &mut chan,
            &Limits::default(),
            &rt,
        )
        .unwrap();
    }
    let mut replies = Vec::new();
    let mut cur = Cursor::new(out);
    while let Ok(Some(f)) = proto::read_frame(&mut cur, MAX_FRAME) {
        replies.push(serde_json::from_slice::<Value>(&f).unwrap());
    }
    assert_eq!(replies[0]["result"]["id"], SID, "{:?}", replies[0]);
    assert_eq!(replies[1]["result"]["runs"].as_array().unwrap().len(), 1);
    assert_eq!(replies[3]["result"]["signal"], "int");
    assert_eq!(replies[4]["error"], "unknown_method");
    assert_eq!(replies[5]["error"], "bad_params");
    let log = r.log.lock().unwrap();
    assert!(log.hellos.iter().all(|c| c == "worker"), "{:?}", log.hellos);
    assert!(!log.hellos.is_empty());
    assert!(
        !log.methods.contains(&"sessions.create".to_string()),
        "{:?}",
        log.methods
    );
    for m in &log.methods {
        assert!(
            ["sessions.list", "runs.create", "sessions.kill"].contains(&m.as_str()),
            "{m}"
        );
    }
    assert!(Audit::new(&w.paths.audit_dir()).verify().unwrap().is_ok());
}

#[test]
fn a_runner_that_cannot_prove_the_secret_is_not_trusted() {
    let r = runner("the-secret", "someone-elses-secret");
    assert!(RunnerClient::connect(&r.dir).is_err());
    assert!(r.log.lock().unwrap().methods.is_empty());
}

#[test]
fn a_secret_file_open_to_others_is_refused() {
    let r = runner("the-secret", "the-secret");
    std::fs::set_permissions(
        r.dir.join("client.secret"),
        <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o644),
    )
    .unwrap();
    assert!(RunnerClient::connect(&r.dir).is_err());
    let _ = secret::eq("a", "b");
}

#[test]
fn attach_goes_through_attach_open_and_the_screen_is_cleaned_by_the_worker() {
    let r = runner("the-secret", "the-secret");
    let mut c = RunnerClient::connect(&r.dir).unwrap();
    let mut a = c.attach(SID, Some((80, 24))).unwrap();
    let got = loop {
        match a.recv(std::time::Duration::from_secs(5)) {
            tmgr_link::runner::AttachRecv::Msg(m) => break *m,
            tmgr_link::runner::AttachRecv::Idle => continue,
            tmgr_link::runner::AttachRecv::Closed => panic!("closed"),
        }
    };
    let ServerMsg::Screen(v) = got else {
        panic!("not a screen")
    };
    let push: tmgr_ipc::api::ScreenPush = serde_json::from_value(v).unwrap();
    let cleaned = tmgr_link::attach::guard_screen(push, SID).unwrap().unwrap();
    let text = &cleaned.snapshot.unwrap().lines[0].spans[0].t;
    assert_eq!(text, "ok?[2J");
    a.detach();
    assert!(r
        .log
        .lock()
        .unwrap()
        .methods
        .contains(&"attach.open".to_string()));
}
