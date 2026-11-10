use serde_json::{json, Value};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tmgr_ipc::decisions::{
    prepare_server_dir, read_line_limited, socket_endpoint, token_ok, write_line, Op, Request,
    Response,
};

fn var(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("{name} is required"))
}

fn serve(dir: &std::path::Path) -> Arc<Mutex<Vec<Request>>> {
    let token = prepare_server_dir(dir).unwrap();
    let listener = match socket_endpoint(dir) {
        tmgr_ipc::endpoint::Endpoint::Path(p) => UnixListener::bind(p).unwrap(),
        #[allow(unreachable_patterns)]
        _ => unreachable!(),
    };
    let seen = Arc::new(Mutex::new(Vec::new()));
    let log = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let (log, token) = (log.clone(), token.clone());
            std::thread::spawn(move || {
                let Ok(Some(line)) = read_line_limited(&stream) else {
                    return;
                };
                let req: Request = serde_json::from_slice(&line).unwrap();
                if !token_ok(&token, &req.token) {
                    return;
                }
                log.lock().unwrap().push(req.clone());
                let reply = match &req.op {
                    Op::Create { card, .. } => Response {
                        ok: true,
                        card_id: card.id.clone(),
                        ..Response::default()
                    },
                    _ => Response::ok(),
                };
                let _ = write_line(&mut stream, &reply);
            });
        }
    });
    seen
}

fn ask(meta: Option<&str>) -> Value {
    let dir = tempfile::Builder::new()
        .prefix("d")
        .tempdir_in("/tmp")
        .unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let home = tempfile::tempdir().unwrap();
    let seen = serve(dir.path());
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_tmgr"));
    cmd.arg("mcp")
        .env_clear()
        .current_dir(var("TMGR_E2E_CWD"))
        .env("HOME", home.path())
        .env("TMGR_CREDENTIAL_STORE", "file")
        .env("TMGR_DECISIONS_DIR", dir.path())
        .env("TMGR_SERVER", "http://127.0.0.1:9")
        .env("TMGR_DECISIONS_WAIT_MS", "300")
        .env("PATH", std::env::var("PATH").unwrap())
        .env("OPENCODE", "1")
        .env("OPENCODE_PID", var("TMGR_E2E_PID"))
        .env("TMGR_OPENCODE_URL", var("TMGR_E2E_URL"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null());
    if let Ok(p) = std::env::var("TMGR_E2E_PASSWORD") {
        cmd.env("OPENCODE_SERVER_PASSWORD", p);
    }
    let mut child = cmd.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut params = json!({"name": "ask_owner", "arguments": {"title": "e2e"}});
    if let Some(m) = meta {
        params["_meta"] = json!({"ai.opencode/sessionID": m});
    }
    for m in [
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "opencode", "version": "1"}}}),
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": params}),
    ] {
        writeln!(stdin, "{m}").unwrap();
    }
    std::thread::sleep(Duration::from_secs(3));
    drop(stdin);
    child.wait().unwrap();
    let reqs = seen.lock().unwrap();
    let create = reqs
        .iter()
        .find_map(|r| match &r.op {
            Op::Create { inbox, .. } => Some(serde_json::to_value(inbox).unwrap()),
            _ => None,
        })
        .expect("no Create captured");
    create
}

#[test]
#[ignore]
fn the_busy_session_in_the_cwd_becomes_the_inbox() {
    let inbox = ask(None);
    assert_eq!(inbox["kind"], "opencode");
    assert_eq!(inbox["endpoint"], var("TMGR_E2E_URL"));
    assert_eq!(inbox["session"], var("TMGR_E2E_SESSION"));
    assert_eq!(
        inbox["token"].is_null(),
        std::env::var("TMGR_E2E_PASSWORD").is_err()
    );
}

#[test]
#[ignore]
fn a_meta_session_id_is_used_when_it_belongs_to_a_busy_root() {
    let session = var("TMGR_E2E_SESSION");
    assert_eq!(ask(Some(&session))["session"], session);
}
