mod common;
use common::{init_repo, path_with, text};
use serde_json::{json, Value};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tmgr_ipc::decisions::{
    prepare_server_dir, read_line_limited, socket_endpoint, token_ok, write_line, Event, Op,
    Request, Response,
};

type Handler = Box<dyn Fn(&Request) -> Response + Send + Sync>;

struct Fake {
    requests: Arc<Mutex<Vec<Request>>>,
}

impl Fake {
    fn ops(&self) -> Vec<Op> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.op.clone())
            .collect()
    }

    fn created(&self) -> Vec<Value> {
        self.ops()
            .into_iter()
            .filter_map(|op| match op {
                Op::Create { card } => Some(serde_json::to_value(&*card).unwrap()),
                _ => None,
            })
            .collect()
    }

    fn waits(&self) -> Vec<u64> {
        self.ops()
            .into_iter()
            .filter_map(|op| match op {
                Op::Wait { since, .. } => Some(since),
                _ => None,
            })
            .collect()
    }
}

struct Stand {
    home: TempDir,
    dir: TempDir,
}

fn stand() -> Stand {
    let dir = tempfile::Builder::new()
        .prefix("d")
        .tempdir_in("/tmp")
        .unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    Stand {
        home: tempfile::tempdir().unwrap(),
        dir,
    }
}

fn serve(stand: &Stand, handler: Handler) -> Fake {
    let token = prepare_server_dir(stand.dir.path()).unwrap();
    let listener = match socket_endpoint(stand.dir.path()) {
        tmgr_ipc::endpoint::Endpoint::Path(p) => UnixListener::bind(p).unwrap(),
        #[allow(unreachable_patterns)]
        _ => unreachable!(),
    };
    let requests = Arc::new(Mutex::new(Vec::new()));
    let log = requests.clone();
    let handler = Arc::new(handler);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let log = log.clone();
            let handler = handler.clone();
            let token = token.clone();
            std::thread::spawn(move || {
                let Ok(Some(line)) = read_line_limited(&stream) else {
                    return;
                };
                let req: Request = serde_json::from_slice(&line).unwrap();
                if !token_ok(&token, &req.token) {
                    let _ = write_line(&mut stream, &Response::err("unauthorized"));
                    return;
                }
                log.lock().unwrap().push(req.clone());
                let _ = write_line(&mut stream, &handler(&req));
            });
        }
    });
    Fake { requests }
}

fn event(card_id: &str, kind: &str, seq: u64) -> Event {
    Event {
        card_id: card_id.into(),
        kind: kind.into(),
        seq,
        ..Event::default()
    }
}

fn answering(choice: &'static str) -> Handler {
    Box::new(move |req| match &req.op {
        Op::Create { card } => Response {
            ok: true,
            card_id: card.id.clone(),
            ..Response::default()
        },
        Op::Wait { card_id, .. } => Response {
            ok: true,
            event: Some(Event {
                choice: Some(choice.into()),
                choice_label: Some(format!("option {choice}")),
                ..event(card_id, "answer", 1)
            }),
            ..Response::default()
        },
        _ => Response::ok(),
    })
}

fn silent() -> Handler {
    Box::new(|req| match &req.op {
        Op::Create { card } => Response {
            ok: true,
            card_id: card.id.clone(),
            ..Response::default()
        },
        _ => Response::ok(),
    })
}

fn base(stand: &Stand, program_args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_tmgr"));
    c.env_clear()
        .env("HOME", stand.home.path())
        .env("XDG_CONFIG_HOME", stand.home.path().join("xdg"))
        .env("XDG_DATA_HOME", stand.home.path().join("data"))
        .env("TMGR_CREDENTIAL_STORE", "file")
        .env("TMGR_DECISIONS_DIR", stand.dir.path())
        .env("TMGR_SERVER", "http://127.0.0.1:9")
        .env("NO_COLOR", "1")
        .env("PATH", path_with(None))
        .args(program_args);
    c
}

fn mcp(stand: &Stand, wait_ms: &str, client: &str, calls: &[(&str, Value)]) -> Vec<Value> {
    let mut cmd = base(stand, &["mcp"]);
    cmd.env("TMGR_DECISIONS_WAIT_MS", wait_ms)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": client, "version": "1"}}})
    )
    .unwrap();
    for (i, (name, args)) in calls.iter().enumerate() {
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": i + 10, "method": "tools/call", "params": {"name": name, "arguments": args}})
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(400));
    }
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let all: Vec<Value> = text(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    (0..calls.len())
        .map(|i| {
            all.iter()
                .find(|r| r["id"] == i + 10)
                .unwrap_or_else(|| panic!("no response {i}: {all:?}"))
                .clone()
        })
        .collect()
}

fn payload(resp: &Value) -> Value {
    serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap()
}

fn is_error(resp: &Value) -> bool {
    resp["result"]["isError"] == true
}

#[test]
fn ask_owner_creates_a_card_and_returns_the_answer() {
    let st = stand();
    let fake = serve(&st, answering("2"));
    let r = mcp(
        &st,
        "5000",
        "claude-code",
        &[(
            "ask_owner",
            json!({
                "title": "Ship 1.2?",
                "question": "Release now?",
                "kind": "release",
                "options": ["Ship", {"label": "Wait", "recommended": true}],
                "facts": ["ok:CI green", {"state": "bad", "text": "one flake"}],
                "checks": ["todo:smoke"],
                "repo": "/w/repo", "base": "main", "head": "feat",
                "task": "TM-1", "pr": "https://example.com/pr/1"
            }),
        )],
    );
    let ans = payload(&r[0]);
    assert_eq!(ans["type"], "answer");
    assert_eq!(ans["choice"], "2");
    let cards = fake.created();
    assert_eq!(cards.len(), 1);
    let c = &cards[0];
    assert_eq!(c["title"], "Ship 1.2?");
    assert_eq!(c["kind"], "release");
    assert_eq!(c["blocking"], true);
    assert_eq!(c["reply_via"], "mcp");
    assert_eq!(c["source"]["agent"], "claude-code");
    assert!(c["source"]["pid"].as_u64().unwrap() > 0);
    assert!(c["source"]["cwd"].is_string());
    assert_eq!(c["options"][0]["id"], "1");
    assert_eq!(c["options"][1]["recommended"], true);
    assert_eq!(c["facts"][1]["state"], "bad");
    assert_eq!(c["checks"][0]["text"], "smoke");
    assert_eq!(c["diff"]["repo"], "/w/repo");
    assert_eq!(c["diff"]["head"], "feat");
    assert_eq!(c["links"]["task"], "TM-1");
    assert!(tmgr_ipc::decisions::valid_card_id(
        c["id"].as_str().unwrap()
    ));
}

#[test]
fn the_agent_name_comes_from_the_client() {
    let st = stand();
    let fake = serve(&st, answering("1"));
    mcp(
        &st,
        "3000",
        "codex-mcp-client",
        &[("ask_owner", json!({"title": "t"}))],
    );
    mcp(
        &st,
        "3000",
        "some-ide",
        &[("ask_owner", json!({"title": "t"}))],
    );
    let agents: Vec<String> = fake
        .created()
        .iter()
        .map(|c| c["source"]["agent"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(agents, ["codex", "other"]);
}

#[test]
fn without_the_desktop_the_card_is_spooled_and_pending() {
    let st = stand();
    let missing = st.dir.path().join("none");
    let mut cmd = base(&st, &["mcp"]);
    cmd.env("TMGR_DECISIONS_DIR", &missing)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "ask_owner", "arguments": {"title": "Need a call"}}})
    )
    .unwrap();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let resp: Value = serde_json::from_str(text(&out.stdout).lines().next().unwrap()).unwrap();
    let v = payload(&resp);
    assert_eq!(v["status"], "pending");
    assert!(v["note"].as_str().unwrap().contains("not running"));
    let id = v["card_id"].as_str().unwrap();
    let file = missing.join("inbox").join(format!("{id}.json"));
    let meta = std::fs::metadata(&file).unwrap();
    assert_eq!(meta.permissions().mode() & 0o777, 0o600);
    let card: Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(card["title"], "Need a call");
    assert_eq!(card["id"], id);
}

#[test]
fn a_wait_that_times_out_is_pending_without_the_note() {
    let st = stand();
    let _fake = serve(&st, silent());
    let r = mcp(
        &st,
        "200",
        "claude-code",
        &[("ask_owner", json!({"title": "t"}))],
    );
    let v = payload(&r[0]);
    assert_eq!(v["status"], "pending");
    assert!(v.get("note").is_none());
    assert!(tmgr_ipc::decisions::valid_card_id(
        v["card_id"].as_str().unwrap()
    ));
}

#[test]
fn a_refused_card_is_a_tool_error() {
    let st = stand();
    let _fake = serve(&st, Box::new(|_| Response::err("title is too long")));
    let r = mcp(
        &st,
        "200",
        "claude-code",
        &[("ask_owner", json!({"title": "t"}))],
    );
    assert!(is_error(&r[0]));
    assert!(r[0]["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("title is too long"));
}

#[test]
fn bad_arguments_are_tool_errors() {
    let st = stand();
    let _fake = serve(&st, silent());
    let r = mcp(
        &st,
        "200",
        "claude-code",
        &[
            ("ask_owner", json!({})),
            ("ask_owner", json!({"title": "t", "kind": "nope"})),
            (
                "ask_owner",
                json!({"title": "t", "options": ["a"], "recommended": 2}),
            ),
            ("ask_owner", json!({"title": "t", "facts": ["maybe:x"]})),
            ("get_answer", json!({"card_id": "bad"})),
            (
                "reply_owner",
                json!({"card_id": "c_00000000000000000000000000000000"}),
            ),
        ],
    );
    for (i, resp) in r.iter().enumerate() {
        assert!(is_error(resp), "{i}: {resp}");
    }
}

#[test]
fn replies_are_sent_and_then_the_next_event_is_awaited() {
    let st = stand();
    let fake = serve(
        &st,
        Box::new(|req| match &req.op {
            Op::Create { card } => Response {
                ok: true,
                card_id: card.id.clone(),
                ..Response::default()
            },
            Op::Wait { card_id, since, .. } => {
                let e = match since {
                    0 => Event {
                        text: Some("why?".into()),
                        ..event(card_id, "message", 1)
                    },
                    1 => Event {
                        deferred: true,
                        ..event(card_id, "answer", 2)
                    },
                    _ => Event {
                        choice: Some("1".into()),
                        ..event(card_id, "answer", 3)
                    },
                };
                Response {
                    ok: true,
                    event: Some(e),
                    ..Response::default()
                }
            }
            _ => Response::ok(),
        }),
    );
    let mut cmd = base(&st, &["mcp"]);
    cmd.env("TMGR_DECISIONS_WAIT_MS", "3000")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let mut send = |id: u64, name: &str, args: Value| {
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": args}})
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(500));
    };
    send(1, "ask_owner", json!({"title": "t"}));
    let started = Instant::now();
    while fake.created().is_empty() && started.elapsed() < Duration::from_secs(5) {
        std::thread::sleep(Duration::from_millis(50));
    }
    let id = fake.created()[0]["id"].as_str().unwrap().to_string();
    send(
        2,
        "reply_owner",
        json!({"card_id": id, "text": "because", "line_replies": [{"file": "a.rs", "line": 3, "text": "done"}]}),
    );
    send(3, "get_answer", json!({"card_id": id}));
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let all: Vec<Value> = text(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let by = |i: u64| payload(all.iter().find(|r| r["id"] == i).unwrap());
    assert_eq!(by(1)["seq"], 1);
    assert_eq!(by(2)["seq"], 2);
    assert_eq!(by(2)["deferred"], true);
    assert_eq!(by(3)["seq"], 3);
    assert_eq!(by(3)["choice"], "1");
    assert_eq!(fake.waits(), [0, 1, 2]);
    let replies: Vec<Op> = fake
        .ops()
        .into_iter()
        .filter(|o| matches!(o, Op::Reply { .. }))
        .collect();
    match &replies[0] {
        Op::Reply {
            card_id,
            text,
            line_replies,
        } => {
            assert_eq!(card_id, &id);
            assert_eq!(text.as_deref(), Some("because"));
            assert_eq!(line_replies[0].line, 3);
        }
        _ => unreachable!(),
    }
}

fn ask(stand: &Stand, args: &[&str]) -> Output {
    let mut full = vec!["ask"];
    full.extend_from_slice(args);
    base(stand, &full).output().unwrap()
}

#[test]
fn ask_prints_the_card_id_and_sends_every_field() {
    let st = stand();
    let fake = serve(&st, silent());
    let out = ask(
        &st,
        &[
            "--title",
            "Pick one",
            "--question",
            "Which?",
            "--context",
            "# ctx",
            "--option",
            "A",
            "--option",
            "B",
            "--recommended",
            "2",
            "--kind",
            "review",
            "--repo",
            "/w/r",
            "--base",
            "main",
            "--fact",
            "ok:green",
            "--check",
            "todo:look",
            "--task",
            "TM-9",
            "--pr",
            "https://example.com/p/1",
        ],
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let c = &fake.created()[0];
    assert_eq!(text(&out.stdout).trim(), c["id"].as_str().unwrap());
    assert_eq!(c["reply_via"], "cli");
    assert_eq!(c["kind"], "review");
    assert_eq!(c["blocking"], false);
    assert_eq!(c["context"], "# ctx");
    assert_eq!(c["options"][1]["recommended"], true);
    assert_eq!(c["diff"]["base"], "main");
    assert_eq!(c["links"]["pr"], "https://example.com/p/1");
    assert!(c["source"].get("pid").is_none());
    let j = ask(&st, &["--title", "T", "--json"]);
    let v: Value = serde_json::from_slice(&j.stdout).unwrap();
    assert_eq!(v["data"]["status"], "created");
}

#[test]
fn ask_arguments_are_checked() {
    let st = stand();
    for args in [
        vec!["--title", "T", "--recommended", "1"],
        vec!["--title", "T", "--kind", "weird"],
        vec!["--title", "T", "--fact", "nope"],
        vec!["--title", "T", "--base", "main"],
        vec!["--title", "T", "--timeout", "5"],
        vec!["--question", "Q"],
    ] {
        let out = ask(&st, &args);
        assert_eq!(
            out.status.code(),
            Some(2),
            "{args:?}: {}",
            text(&out.stderr)
        );
    }
}

#[test]
fn ask_wait_prints_interim_messages_to_stderr_and_the_final_answer() {
    let st = stand();
    let fake = serve(
        &st,
        Box::new(|req| match &req.op {
            Op::Create { card } => Response {
                ok: true,
                card_id: card.id.clone(),
                ..Response::default()
            },
            Op::Wait { card_id, since, .. } => {
                let e = if *since == 0 {
                    Event {
                        text: Some("one more thing".into()),
                        ..event(card_id, "message", 1)
                    }
                } else {
                    Event {
                        choice: Some("1".into()),
                        choice_label: Some("Ship".into()),
                        note: Some("go".into()),
                        ..event(card_id, "answer", 2)
                    }
                };
                Response {
                    ok: true,
                    event: Some(e),
                    ..Response::default()
                }
            }
            _ => Response::ok(),
        }),
    );
    let out = ask(&st, &["--title", "T", "--option", "Ship", "--wait"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("choice: Ship"));
    assert!(text(&out.stdout).contains("note: go"));
    assert!(text(&out.stderr).contains("message: one more thing"));
    assert_eq!(fake.waits(), [0, 1]);
    assert_eq!(fake.created()[0]["blocking"], true);
    assert!(fake.created()[0]["source"]["pid"].as_u64().unwrap() > 0);
    let j = ask(&st, &["--title", "T", "--wait", "--json"]);
    let v: Value = serde_json::from_slice(&j.stdout).unwrap();
    assert_eq!(v["data"]["status"], "answered");
    assert_eq!(v["data"]["answer"]["type"], "answer");
}

#[test]
fn ask_wait_exits_8_when_no_answer_arrives() {
    let st = stand();
    let _fake = serve(&st, silent());
    let out = ask(&st, &["--title", "T", "--wait", "--timeout", "1"]);
    assert_eq!(out.status.code(), Some(8), "{}", text(&out.stderr));
    assert!(tmgr_ipc::decisions::valid_card_id(text(&out.stdout).trim()));
}

#[test]
fn ask_without_the_desktop_spools_and_fails_only_when_it_cannot() {
    let st = stand();
    let out = ask(&st, &["--title", "T"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let id = text(&out.stdout).trim().to_string();
    assert!(st
        .dir
        .path()
        .join("inbox")
        .join(format!("{id}.json"))
        .exists());
    assert!(text(&out.stderr).contains("not running"));
    let waited = ask(&st, &["--title", "T", "--wait"]);
    assert_eq!(waited.status.code(), Some(8));

    let blocker = st.home.path().join("file");
    std::fs::write(&blocker, "x").unwrap();
    let mut cmd = base(&st, &["ask", "--title", "T"]);
    cmd.env("TMGR_DECISIONS_DIR", blocker.join("sub"));
    let out = cmd.output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
}

fn hook(stand: &Stand, event: &str, stdin: &[u8], extra: &[(&str, &str)]) -> (Output, Duration) {
    let mut cmd = base(stand, &["hook", "decisions", event]);
    cmd.env("TMGR_HOOK_BUDGET_MS", "8000");
    for (k, v) in extra {
        cmd.env(k, v);
    }
    let started = Instant::now();
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    let out = child.wait_with_output().unwrap();
    (out, started.elapsed())
}

fn quiet(out: &Output) {
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(out.stdout.is_empty(), "stdout: {}", text(&out.stdout));
}

fn stop_input(cwd: &Path, message: &str) -> Vec<u8> {
    json!({
        "session_id": "sess-1", "cwd": cwd, "hook_event_name": "Stop",
        "transcript_path": "/nonexistent", "last_assistant_message": message
    })
    .to_string()
    .into_bytes()
}

fn project_dir(st: &Stand, name: &str) -> PathBuf {
    let p = st.home.path().join(name);
    std::fs::create_dir_all(&p).unwrap();
    std::fs::canonicalize(p).unwrap()
}

#[test]
fn stop_builds_a_stopped_card_without_the_message_when_consent_is_off() {
    let st = stand();
    let fake = serve(&st, silent());
    let repo = init_repo(&st.home.path().join("proj"));
    let (out, took) = hook(&st, "stop", &stop_input(&repo, "SECRET SUMMARY"), &[]);
    quiet(&out);
    assert!(took < Duration::from_secs(3), "{took:?}");
    let cards = fake.created();
    assert_eq!(cards.len(), 1);
    let c = &cards[0];
    assert_eq!(c["kind"], "stopped");
    assert_eq!(c["title"], "proj finished");
    assert!(c.get("context").is_none(), "{c}");
    assert_eq!(c["supersede"], true);
    assert_eq!(c["reply_via"], "hook");
    assert_eq!(c["blocking"], false);
    assert_eq!(c["source"]["agent"], "claude-code");
    assert_eq!(c["source"]["session"], "sess-1");
    assert!(c["source"].get("runner_session").is_none());
    assert_eq!(c["diff"]["repo"], repo.display().to_string());
    assert_eq!(c["diff"]["base"], "HEAD");
}

#[test]
fn stop_includes_the_message_only_when_reading_logs_is_granted() {
    let st = stand();
    let fake = serve(&st, silent());
    let plain = project_dir(&st, "plain");
    let allow = base(&st, &["usage", "consent", "allow"]).output().unwrap();
    assert!(allow.status.success(), "{}", text(&allow.stderr));
    let (out, _) = hook(&st, "stop", &stop_input(&plain, "all done here"), &[]);
    quiet(&out);
    let big = "y".repeat(40_000);
    let (out, _) = hook(&st, "stop", &stop_input(&plain, &big), &[]);
    quiet(&out);
    let cards = fake.created();
    assert_eq!(cards[0]["context"], "all done here");
    assert!(cards[0].get("diff").is_none(), "not a git repository");
    assert_eq!(cards[1]["context"].as_str().unwrap().len(), 16 * 1024);

    let deny = base(&st, &["usage", "consent", "deny"]).output().unwrap();
    assert!(deny.status.success());
    let (out, _) = hook(&st, "stop", &stop_input(&plain, "hidden again"), &[]);
    quiet(&out);
    assert!(fake.created()[2].get("context").is_none());
}

#[test]
fn the_runner_session_is_taken_from_the_environment_only_when_valid() {
    let st = stand();
    let fake = serve(&st, silent());
    let cwd = project_dir(&st, "p");
    let input = stop_input(&cwd, "m");
    quiet(&hook(&st, "stop", &input, &[("TMGR_SESSION_ID", "s_abcdefghjk")]).0);
    quiet(&hook(&st, "stop", &input, &[("TMGR_SESSION_ID", "not a session")]).0);
    let cards = fake.created();
    assert_eq!(cards[0]["source"]["runner_session"], "s_abcdefghjk");
    assert!(cards[1]["source"].get("runner_session").is_none());
}

#[test]
fn notifications_become_cards_by_type() {
    let st = stand();
    let fake = serve(&st, silent());
    let cwd = project_dir(&st, "p");
    for t in [
        "permission_prompt",
        "idle_prompt",
        "agent_needs_input",
        "elicitation_dialog",
        "auth_success",
    ] {
        let input = json!({"session_id": "s", "cwd": cwd, "hook_event_name": "Notification", "notification_type": t, "message": "Claude needs you", "title": "x"});
        quiet(&hook(&st, "notification", input.to_string().as_bytes(), &[]).0);
    }
    let cards = fake.created();
    let kinds: Vec<&str> = cards.iter().map(|c| c["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["permission", "question", "question", "question"]);
    assert_eq!(cards[0]["question"], "Claude needs you");
    assert_eq!(cards[0]["supersede"], true);
}

#[test]
fn permission_requests_become_permission_cards() {
    let st = stand();
    let fake = serve(&st, silent());
    let cwd = project_dir(&st, "p");
    let input = json!({"session_id": "s", "cwd": cwd, "hook_event_name": "PermissionRequest", "tool_name": "Bash", "tool_input": {"command": "cargo publish"}});
    quiet(&hook(&st, "permission-request", input.to_string().as_bytes(), &[]).0);
    let c = &fake.created()[0];
    assert_eq!(c["kind"], "permission");
    assert_eq!(c["title"], "p: permission for Bash");
    assert_eq!(c["question"], "cargo publish");
}

#[test]
fn a_new_prompt_closes_the_session_cards() {
    let st = stand();
    let fake = serve(&st, silent());
    let cwd = project_dir(&st, "p");
    let input = json!({"session_id": "sess-9", "cwd": cwd, "prompt": "next"});
    quiet(&hook(&st, "user-prompt-submit", input.to_string().as_bytes(), &[]).0);
    assert_eq!(
        fake.ops(),
        [Op::CloseSession {
            agent: "claude-code".into(),
            session: "sess-9".into()
        }]
    );
}

#[test]
fn hooks_exit_zero_quietly_without_the_desktop_and_on_bad_input() {
    let st = stand();
    let cwd = project_dir(&st, "p");
    for event in [
        "stop",
        "notification",
        "permission-request",
        "user-prompt-submit",
    ] {
        let (out, took) = hook(&st, event, &stop_input(&cwd, "m"), &[]);
        quiet(&out);
        assert!(took < Duration::from_secs(4), "{event}: {took:?}");
    }
    assert!(
        !st.dir.path().join("inbox").exists(),
        "hook cards are never spooled"
    );
    let _fake = serve(&st, silent());
    for event in ["stop", "bogus"] {
        quiet(&hook(&st, event, b"not json", &[]).0);
    }
    quiet(&hook(&st, "stop", b"[1,2]", &[]).0);
    let out = base(&st, &["hook", "decisions"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
}

#[test]
fn a_hung_desktop_cannot_hold_the_hook_past_its_budget() {
    let st = stand();
    let _fake = serve(
        &st,
        Box::new(|_| {
            std::thread::sleep(Duration::from_secs(20));
            Response::ok()
        }),
    );
    let cwd = project_dir(&st, "p");
    let (out, took) = hook(
        &st,
        "stop",
        &stop_input(&cwd, "m"),
        &[("TMGR_HOOK_BUDGET_MS", "1500")],
    );
    quiet(&out);
    assert!(took < Duration::from_secs(4), "{took:?}");
}
