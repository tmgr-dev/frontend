mod common;
use common::{init_repo, path_with, text};
use serde_json::{json, Value};
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tmgr_ipc::decisions::{
    prepare_server_dir, read_line_limited, socket_endpoint, token_ok, write_line, Event, Op,
    PendingDelivery, Request, Response,
};

type Handler = Box<dyn Fn(&Request) -> Response + Send + Sync>;

struct Fake {
    requests: Arc<Mutex<Vec<Request>>>,
    rejected: Arc<std::sync::atomic::AtomicUsize>,
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
                Op::Create { card, .. } => Some(serde_json::to_value(&*card).unwrap()),
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
    serve_as(stand, token, handler)
}

fn serve_stale(stand: &Stand, handler: Handler) -> (Fake, String) {
    prepare_server_dir(stand.dir.path()).unwrap();
    let token = tmgr_ipc::secret::generate();
    (serve_as(stand, token.clone(), handler), token)
}

fn publish_token(dir: &Path, token: String, after: Duration) -> std::thread::JoinHandle<()> {
    let file = dir.join("decisions.token");
    std::thread::spawn(move || {
        std::thread::sleep(after);
        std::fs::remove_file(&file).unwrap();
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&file)
            .unwrap();
        f.write_all(token.as_bytes()).unwrap();
    })
}

fn serve_as(stand: &Stand, token: String, handler: Handler) -> Fake {
    let listener = match socket_endpoint(stand.dir.path()) {
        tmgr_ipc::endpoint::Endpoint::Path(p) => UnixListener::bind(p).unwrap(),
        #[allow(unreachable_patterns)]
        _ => unreachable!(),
    };
    let requests = Arc::new(Mutex::new(Vec::new()));
    let rejected = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let log = requests.clone();
    let refusals = rejected.clone();
    let handler = Arc::new(handler);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let log = log.clone();
            let handler = handler.clone();
            let token = token.clone();
            let refusals = refusals.clone();
            std::thread::spawn(move || {
                let Ok(Some(line)) = read_line_limited(&stream) else {
                    return;
                };
                let req: Request = serde_json::from_slice(&line).unwrap();
                if !token_ok(&token, &req.token) {
                    refusals.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    let _ = write_line(&mut stream, &Response::err("unauthorized"));
                    return;
                }
                log.lock().unwrap().push(req.clone());
                let _ = write_line(&mut stream, &handler(&req));
            });
        }
    });
    Fake { requests, rejected }
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
        Op::Create { card, .. } => Response {
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
        Op::Create { card, .. } => Response {
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
    let ids: Vec<u64> = (0..calls.len() as u64).map(|i| i + 10).collect();
    let all = responses(&mut child, stdin, &ids);
    (0..calls.len())
        .map(|i| {
            all.iter()
                .find(|r| r["id"] == i + 10)
                .unwrap_or_else(|| panic!("no response {i}: {all:?}"))
                .clone()
        })
        .collect()
}

fn responses(
    child: &mut std::process::Child,
    stdin: std::process::ChildStdin,
    ids: &[u64],
) -> Vec<Value> {
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::BufRead::lines(std::io::BufReader::new(stdout)) {
            let Ok(line) = line else { break };
            if tx
                .send(serde_json::from_str::<Value>(&line).unwrap())
                .is_err()
            {
                break;
            }
        }
    });
    let mut all = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(60);
    while !ids
        .iter()
        .all(|i| all.iter().any(|r: &Value| r["id"] == *i))
    {
        let left = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(left) {
            Ok(v) => all.push(v),
            Err(_) => panic!("missing responses {ids:?}: {all:?}"),
        }
    }
    drop(stdin);
    child.wait().unwrap();
    all.extend(rx.try_iter());
    all
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
            Op::Create { card, .. } => Response {
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
    let all = responses(&mut child, stdin, &[1, 2, 3]);
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
            Op::Create { card, .. } => Response {
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
    let waited = ask(&st, &["--title", "T", "--wait", "--timeout", "1"]);
    assert_eq!(waited.status.code(), Some(8));

    let blocker = st.home.path().join("file");
    std::fs::write(&blocker, "x").unwrap();
    let mut cmd = base(&st, &["ask", "--title", "T"]);
    cmd.env("TMGR_DECISIONS_DIR", blocker.join("sub"));
    let out = cmd.output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
}

fn id_of(out: &Output) -> String {
    text(&out.stdout).trim().to_string()
}

fn ask_in(stand: &Stand, args: &[&str], stdin: &[u8]) -> Output {
    let mut full = vec!["ask"];
    full.extend_from_slice(args);
    let mut child = base(stand, &full)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin).unwrap();
    child.wait_with_output().unwrap()
}

fn waits_on(fake: &Fake) -> Vec<String> {
    fake.ops()
        .into_iter()
        .filter_map(|op| match op {
            Op::Wait { card_id, .. } => Some(card_id),
            _ => None,
        })
        .collect()
}

#[test]
fn ask_flags_become_cards_with_groups_and_links() {
    let st = stand();
    let fake = serve(&st, silent());
    let patch = st.home.path().join("fix.diff");
    std::fs::write(&patch, "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n").unwrap();
    let eq_patch = st.home.path().join("a=b.diff");
    std::fs::write(&eq_patch, "--- a/y\n+++ b/y\n").unwrap();
    let replaces = tmgr_ipc::decisions::new_card_id();
    let out = ask(
        &st,
        &[
            "--title",
            "T",
            "--repo",
            "/w/r",
            "--base",
            "main",
            "--diff",
            "/w/a=v1..v2",
            "--diff",
            "/w/b=main",
            "--diff",
            "/w/c",
            "--patch",
            patch.to_str().unwrap(),
            "--patch",
            &format!("Named={}", patch.display()),
            "--patch",
            eq_patch.to_str().unwrap(),
            "--pr",
            "https://github.com/o/r/pull/1",
            "--pr",
            "o/r#2",
            "--commit",
            "o/r@ABCDEF1234",
            "--commit",
            "https://github.com/o/r/commit/0123456789abcdef",
            "--tag",
            "o/r@v1.2.0",
            "--tag",
            "https://github.com/o/r/releases/tag/v1",
            "--link",
            "https://example.com/a?x=y",
            "--link",
            "Docs=https://example.com/d",
            "--task",
            "https://tracker.example.com/T-1",
            "--replaces",
            &replaces,
            "--key",
            "round-1",
        ],
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let c = &fake.created()[0];
    assert_eq!(c["diff"]["repo"], "/w/r");
    assert_eq!(c["diff"]["base"], "main");
    assert_eq!(c["diffs"][0]["repo"], "/w/a");
    assert_eq!(c["diffs"][0]["base"], "v1");
    assert_eq!(c["diffs"][0]["head"], "v2");
    assert_eq!(c["diffs"][1]["base"], "main");
    assert!(c["diffs"][1].get("head").is_none());
    assert_eq!(c["diffs"][2]["repo"], "/w/c");
    assert_eq!(c["patches"][0]["label"], "fix.diff");
    assert_eq!(c["patches"][1]["label"], "Named");
    assert_eq!(c["patches"][2]["label"], "a=b.diff");
    assert!(c["patches"][0]["text"].as_str().unwrap().contains("+b"));
    assert_eq!(c["replaces"], replaces);
    assert_eq!(c["key"], "round-1");
    assert_eq!(c["links"]["pr"], "https://github.com/o/r/pull/1");
    assert_eq!(c["links"]["task"], "https://tracker.example.com/T-1");
    let items = c["links"]["items"].as_array().unwrap();
    let got: Vec<(&str, &str, Option<&str>)> = items
        .iter()
        .map(|i| {
            (
                i["kind"].as_str().unwrap(),
                i["url"].as_str().unwrap(),
                i["label"].as_str(),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            ("pr", "https://github.com/o/r/pull/1", None),
            ("pr", "https://github.com/o/r/pull/2", None),
            (
                "commit",
                "https://github.com/o/r/commit/ABCDEF1234",
                Some("abcdef1")
            ),
            (
                "commit",
                "https://github.com/o/r/commit/0123456789abcdef",
                Some("0123456")
            ),
            ("tag", "https://github.com/o/r/releases/tag/v1.2.0", None),
            ("tag", "https://github.com/o/r/releases/tag/v1", None),
            ("url", "https://example.com/a?x=y", None),
            ("url", "https://example.com/d", Some("Docs")),
            ("task", "https://tracker.example.com/T-1", None),
        ]
    );
}

#[test]
fn a_diff_alone_is_the_first_group() {
    let st = stand();
    let fake = serve(&st, silent());
    let out = ask(&st, &["--title", "T", "--diff", "/w/a=main..HEAD"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let c = &fake.created()[0];
    assert_eq!(c["diff"]["repo"], "/w/a");
    assert_eq!(c["diff"]["head"], "HEAD");
    assert!(c.get("diffs").is_none());
}

#[test]
fn bad_forms_are_usage_errors_and_send_nothing() {
    let st = stand();
    let fake = serve(&st, silent());
    let id = tmgr_ipc::decisions::new_card_id();
    let cases: Vec<Vec<&str>> = vec![
        vec!["--commit", "o/r@abc12"],
        vec!["--commit", "o/r@zzzzzzzz"],
        vec!["--commit", "file:///etc/passwd"],
        vec!["--tag", "o/r@"],
        vec!["--tag", "o/r@a b"],
        vec!["--tag", "o/r@a..b"],
        vec!["--tag", "o/r@v1%2F"],
        vec!["--tag", "o/r@-x"],
        vec!["--tag", "o@v1"],
        vec!["--link", "Docs=ftp://x/y"],
        vec!["--link", "=https://x.example"],
        vec!["--link", "not a url"],
        vec!["--link", "ftp://x.example/a=b"],
        vec!["--diff", ""],
        vec!["--diff", "=main"],
        vec!["--diff", "/w/a="],
        vec!["--diff", "/w/a=a..b..c"],
        vec!["--diff", "/w/a=..b"],
        vec!["--diff", "/w/a=a.."],
        vec!["--patch", "/nonexistent/file.diff"],
        vec!["--replaces", "nope"],
        vec!["--key", ""],
        vec!["--task", "https://ok.example", "--link", "x"],
    ];
    for extra in cases {
        let mut args = vec!["--title", "T"];
        args.extend(extra.iter());
        let out = ask(&st, &args);
        assert_eq!(
            out.status.code(),
            Some(2),
            "{extra:?}: {}",
            text(&out.stderr)
        );
    }
    let out = ask(&st, &["--title", "T", "--replaces", &id, "--key", "k"]);
    assert!(out.status.success());
    assert_eq!(fake.created().len(), 1);
}

#[test]
fn patches_come_from_stdin_once() {
    let st = stand();
    let fake = serve(&st, silent());
    let out = ask_in(
        &st,
        &["--title", "T", "--patch", "-"],
        b"--- a\n+++ b\n\xff\n",
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let out = ask_in(
        &st,
        &["--title", "T", "--patch", "Piped=-"],
        b"--- a\n+++ b\n",
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let cards = fake.created();
    assert_eq!(cards[0]["patches"][0]["label"], "patch");
    assert!(cards[0]["patches"][0]["text"]
        .as_str()
        .unwrap()
        .contains('\u{fffd}'));
    assert_eq!(cards[1]["patches"][0]["label"], "Piped");
    let out = ask_in(
        &st,
        &["--title", "T", "--patch", "-", "--patch", "B=-"],
        b"x",
    );
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("stdin"));
    assert_eq!(fake.created().len(), 2);
}

#[test]
fn limits_are_refused_before_sending() {
    let st = stand();
    let fake = serve(&st, silent());
    let mut many = vec!["--title".to_string(), "T".to_string()];
    for i in 0..9 {
        many.push("--diff".into());
        many.push(format!("/w/r{i}"));
    }
    let args: Vec<&str> = many.iter().map(String::as_str).collect();
    assert_eq!(ask(&st, &args).status.code(), Some(2));
    let mut ok = args.clone();
    ok.truncate(2 + 16);
    assert!(ask(&st, &ok).status.success());

    let small = st.home.path().join("s.diff");
    std::fs::write(&small, "--- a\n+++ b\n").unwrap();
    let mut args = vec!["--title", "T"];
    for _ in 0..9 {
        args.extend(["--patch", small.to_str().unwrap()]);
    }
    assert_eq!(ask(&st, &args).status.code(), Some(2));

    let big = st.home.path().join("big.diff");
    std::fs::write(&big, vec![b'x'; (1 << 20) + 1]).unwrap();
    assert_eq!(
        ask(&st, &["--title", "T", "--patch", big.to_str().unwrap()])
            .status
            .code(),
        Some(2)
    );
    let half = st.home.path().join("half.diff");
    std::fs::write(&half, vec![b'x'; 600 * 1024]).unwrap();
    let h = half.to_str().unwrap();
    assert_eq!(
        ask(&st, &["--title", "T", "--patch", h, "--patch", h])
            .status
            .code(),
        Some(2)
    );

    let mut links = vec!["--title".to_string(), "T".to_string()];
    for i in 0..51 {
        links.push("--link".into());
        links.push(format!("https://example.com/{i}"));
    }
    let args: Vec<&str> = links.iter().map(String::as_str).collect();
    assert_eq!(ask(&st, &args).status.code(), Some(2));

    let ctl = st.home.path().join("ctl.diff");
    std::fs::write(&ctl, vec![1u8; 1 << 20]).unwrap();
    let out = ask(&st, &["--title", "T", "--patch", ctl.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("too large"));
    assert_eq!(fake.created().len(), 1);
}

#[test]
fn old_flags_alone_give_the_same_card() {
    let st = stand();
    let fake = serve(&st, silent());
    let out = ask(
        &st,
        &[
            "--title", "T", "--repo", "/w/r", "--base", "main", "--task", "TM-9",
        ],
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let c = &fake.created()[0];
    for k in ["diffs", "patches", "replaces", "key"] {
        assert!(c.get(k).is_none(), "{k}");
    }
    assert!(c["links"].get("items").is_none());
    assert!(c["links"].get("pr").is_none());
    assert_eq!(c["links"]["task"], "TM-9");
    assert_eq!(c["diff"]["repo"], "/w/r");
}

fn outcome_handler(outcome: &'static str, canonical: String) -> Handler {
    Box::new(move |req| match &req.op {
        Op::Create { .. } => Response {
            ok: true,
            card_id: Some(canonical.clone()),
            outcome: Some(outcome.into()),
            ..Response::default()
        },
        Op::Wait { card_id, .. } => Response {
            ok: true,
            event: Some(Event {
                choice: Some("1".into()),
                choice_label: Some("Ship".into()),
                ..event(card_id, "answer", 3)
            }),
            ..Response::default()
        },
        _ => Response::ok(),
    })
}

#[test]
fn the_outcome_sets_the_status_and_the_canonical_id() {
    for (outcome, status) in [
        ("created", "created"),
        ("updated", "updated"),
        ("final", "already_answered"),
    ] {
        let st = stand();
        let canon = tmgr_ipc::decisions::new_card_id();
        let fake = serve(&st, outcome_handler(outcome, canon.clone()));
        let out = ask(&st, &["--title", "T", "--key", "k", "--json"]);
        assert!(out.status.success(), "{}", text(&out.stderr));
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(v["data"]["card_id"], canon.as_str());
        assert_eq!(v["data"]["status"], status);
        assert_ne!(fake.created()[0]["id"], canon.as_str());
        let plain = ask(&st, &["--title", "T"]);
        assert_eq!(id_of(&plain), canon);
    }
}

#[test]
fn a_final_outcome_with_wait_returns_the_existing_answer_at_once() {
    let st = stand();
    let canon = tmgr_ipc::decisions::new_card_id();
    let fake = serve(&st, outcome_handler("final", canon.clone()));
    let started = Instant::now();
    let out = ask(&st, &["--title", "T", "--key", "k", "--wait", "--json"]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(started.elapsed() < Duration::from_secs(5));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["status"], "answered");
    assert_eq!(v["data"]["card_id"], canon.as_str());
    assert_eq!(waits_on(&fake), [canon]);
}

#[test]
fn ask_wait_without_the_desktop_waits_until_it_appears() {
    let st = stand();
    let mut child = base(&st, &["ask", "--title", "T", "--wait", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(1500));
    assert!(child.try_wait().unwrap().is_none(), "exited early");
    let fake = serve(&st, answering("1"));
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("not running"));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["status"], "answered");
    let spooled = std::fs::read_dir(st.dir.path().join("inbox"))
        .unwrap()
        .count();
    assert_eq!(spooled, 1);
    assert_eq!(fake.waits(), [0]);
}

#[test]
fn ask_wait_timeout_without_the_desktop_exits_8_at_the_deadline() {
    let st = stand();
    let started = Instant::now();
    let out = ask(&st, &["--title", "T", "--wait", "--timeout", "1"]);
    let took = started.elapsed();
    assert_eq!(out.status.code(), Some(8), "{}", text(&out.stderr));
    assert!(took >= Duration::from_millis(900), "{took:?}");
    assert!(took < Duration::from_secs(6), "{took:?}");
    assert!(tmgr_ipc::decisions::valid_card_id(&id_of(&out)));
    assert!(text(&out.stderr).contains("not running"));
}

#[test]
fn an_unknown_card_in_the_spool_window_keeps_waiting() {
    let st = stand();
    let child = base(&st, &["ask", "--title", "T", "--wait", "--json"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    std::thread::sleep(Duration::from_millis(800));
    let seen = Arc::new(Mutex::new(0usize));
    let counter = seen.clone();
    let fake = serve(
        &st,
        Box::new(move |req| match &req.op {
            Op::Wait { card_id, .. } => {
                let mut n = counter.lock().unwrap();
                *n += 1;
                if *n < 3 {
                    Response::err("unknown card")
                } else {
                    Response {
                        ok: true,
                        event: Some(Event {
                            choice: Some("1".into()),
                            ..event(card_id, "answer", 1)
                        }),
                        ..Response::default()
                    }
                }
            }
            _ => Response::ok(),
        }),
    );
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert_eq!(fake.waits().len(), 3);
}

#[test]
fn an_unknown_card_for_a_live_card_is_still_an_error() {
    let st = stand();
    let _fake = serve(
        &st,
        Box::new(|req| match &req.op {
            Op::Create { card, .. } => Response {
                ok: true,
                card_id: card.id.clone(),
                ..Response::default()
            },
            _ => Response::err("unknown card"),
        }),
    );
    let out = ask(&st, &["--title", "T", "--wait"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out.stderr));
}

#[test]
fn ask_owner_takes_the_new_fields() {
    let st = stand();
    let canon = tmgr_ipc::decisions::new_card_id();
    let fake = serve(&st, outcome_handler("updated", canon.clone()));
    let replaces = tmgr_ipc::decisions::new_card_id();
    let r = mcp(
        &st,
        "3000",
        "claude-code",
        &[
            (
                "ask_owner",
                json!({
                    "title": "T",
                    "repo": "/w/main", "base": "main",
                    "repos": [{"repo": "/w/b", "base": "v1", "head": "v2"}, {"repo": "/w/c"}],
                    "patches": [{"label": "fix", "text": "--- a\n+++ b\n"}, {"text": "--- c\n+++ d\n"}],
                    "pr": "https://github.com/o/r/pull/9",
                    "prs": ["o/r#2"],
                    "commits": ["o/r@abcdef1"],
                    "tags": ["o/r@v2"],
                    "links": ["Docs=https://example.com/d"],
                    "task": "TM-3",
                    "replaces": replaces,
                    "key": "k1"
                }),
            ),
            ("get_answer", json!({"card_id": canon, "waitSeconds": 0})),
        ],
    );
    assert_eq!(payload(&r[0])["type"], "answer");
    let c = &fake.created()[0];
    assert_eq!(c["diff"]["repo"], "/w/main");
    assert_eq!(c["diffs"][0]["repo"], "/w/b");
    assert_eq!(c["diffs"][0]["head"], "v2");
    assert_eq!(c["diffs"][1]["repo"], "/w/c");
    assert_eq!(c["patches"][0]["label"], "fix");
    assert_eq!(c["patches"][1]["label"], "patch");
    assert_eq!(c["replaces"], replaces);
    assert_eq!(c["key"], "k1");
    assert_eq!(c["supersede"], Value::Null);
    assert_eq!(c["links"]["pr"], "https://github.com/o/r/pull/9");
    assert_eq!(c["links"]["task"], "TM-3");
    let kinds: Vec<&str> = c["links"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["kind"].as_str().unwrap())
        .collect();
    assert_eq!(kinds, ["pr", "pr", "commit", "tag", "url"]);
    assert_eq!(waits_on(&fake), [canon.clone(), canon.clone()]);
    let since: Vec<u64> = fake.waits();
    assert_eq!(since, [0, 3]);
}

#[test]
fn ask_owner_refuses_bad_new_fields() {
    let st = stand();
    let fake = serve(&st, silent());
    let nine: Vec<Value> = (0..9).map(|i| json!({"repo": format!("/w/{i}")})).collect();
    let r = mcp(
        &st,
        "1000",
        "claude-code",
        &[
            ("ask_owner", json!({"title": "T", "repos": nine})),
            ("ask_owner", json!({"title": "T", "repos": [{"base": "x"}]})),
            (
                "ask_owner",
                json!({"title": "T", "patches": [{"label": "x"}]}),
            ),
            (
                "ask_owner",
                json!({"title": "T", "patches": [{"text": ""}]}),
            ),
            ("ask_owner", json!({"title": "T", "prs": [1]})),
            ("ask_owner", json!({"title": "T", "commits": [1]})),
            ("ask_owner", json!({"title": "T", "tags": "v1"})),
            (
                "ask_owner",
                json!({"title": "T", "links": ["javascript:x"]}),
            ),
            ("ask_owner", json!({"title": "T", "replaces": "nope"})),
            ("ask_owner", json!({"title": "T", "key": ""})),
        ],
    );
    for (i, resp) in r.iter().enumerate() {
        assert!(is_error(resp), "{i}: {resp}");
    }
    assert!(fake.created().is_empty());
}

#[test]
fn ask_owner_without_the_desktop_is_pending_and_the_old_fields_work() {
    let st = stand();
    let r = mcp(
        &st,
        "1000",
        "claude-code",
        &[(
            "ask_owner",
            json!({"title": "T", "repo": "/w/r", "pr": "https://example.com/p/1"}),
        )],
    );
    let p = payload(&r[0]);
    assert_eq!(p["status"], "pending");
    let id = p["card_id"].as_str().unwrap();
    let spooled: Value = serde_json::from_slice(
        &std::fs::read(st.dir.path().join("inbox").join(format!("{id}.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(spooled["links"]["pr"], "https://example.com/p/1");
    assert!(spooled.get("diffs").is_none());
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
    std::fs::write(repo.join("README.md"), "changed\n").unwrap();
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
    assert_eq!(kinds, ["permission", "question", "question"]);
    assert_eq!(cards[0]["question"], "Claude needs you");
    assert_eq!(cards[0]["supersede"], Value::Null);
}

#[test]
fn stock_and_idle_notifications_make_no_card() {
    let st = stand();
    let fake = serve(&st, silent());
    let cwd = project_dir(&st, "p");
    for (t, m) in [
        ("idle_prompt", "Claude is waiting for your input"),
        ("agent_needs_input", "Claude is waiting for your input"),
        ("elicitation_dialog", ""),
    ] {
        let input = json!({"session_id": "s", "cwd": cwd, "hook_event_name": "Notification", "notification_type": t, "message": m});
        quiet(&hook(&st, "notification", input.to_string().as_bytes(), &[]).0);
    }
    assert!(fake.created().is_empty());
}

#[test]
fn stop_without_text_makes_a_card_only_when_the_tree_is_dirty() {
    let st = stand();
    let fake = serve(&st, silent());
    let plain = project_dir(&st, "plain");
    quiet(&hook(&st, "stop", &stop_input(&plain, ""), &[]).0);
    let repo = init_repo(&st.home.path().join("proj"));
    quiet(&hook(&st, "stop", &stop_input(&repo, "  "), &[]).0);
    assert!(fake.created().is_empty());
    std::fs::write(repo.join("new.txt"), "x").unwrap();
    quiet(&hook(&st, "stop", &stop_input(&repo, ""), &[]).0);
    let cards = fake.created();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0]["kind"], "stopped");
    assert_eq!(cards[0]["diff"]["repo"], repo.display().to_string());
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

#[test]
fn get_answer_follows_a_token_that_changes_mid_call() {
    let st = stand();
    let (fake, token) = serve_stale(&st, answering("2"));
    let id = tmgr_ipc::decisions::new_card_id();
    let publisher = publish_token(st.dir.path(), token, Duration::from_millis(300));
    let r = mcp(
        &st,
        "5000",
        "claude-code",
        &[("get_answer", json!({"card_id": id}))],
    );
    publisher.join().unwrap();
    assert!(!is_error(&r[0]), "{}", r[0]);
    assert_eq!(payload(&r[0])["choice"], "2");
    assert_eq!(fake.waits(), [0]);
}

#[test]
fn get_answer_rides_out_a_token_file_that_is_briefly_missing() {
    let st = stand();
    let (fake, token) = serve_stale(&st, answering("2"));
    let id = tmgr_ipc::decisions::new_card_id();
    let file = st.dir.path().join("decisions.token");
    let rejected = fake.rejected.clone();
    let publisher = std::thread::spawn(move || {
        let end = std::time::Instant::now() + Duration::from_secs(5);
        while rejected.load(std::sync::atomic::Ordering::SeqCst) == 0 {
            assert!(
                std::time::Instant::now() < end,
                "the token was never refused"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        std::fs::remove_file(&file).unwrap();
        std::thread::sleep(Duration::from_millis(350));
        std::fs::write(&file, token).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
    });
    let r = mcp(
        &st,
        "5000",
        "claude-code",
        &[("get_answer", json!({"card_id": id}))],
    );
    publisher.join().unwrap();
    assert!(!is_error(&r[0]), "{}", r[0]);
    assert_eq!(payload(&r[0])["choice"], "2");
}

#[test]
fn ask_wait_follows_a_token_that_changes_mid_call() {
    let st = stand();
    let (fake, token) = serve_stale(&st, answering("1"));
    let publisher = publish_token(st.dir.path(), token, Duration::from_millis(300));
    let out = ask(
        &st,
        &["--title", "T", "--wait", "--timeout", "20", "--json"],
    );
    publisher.join().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["status"], "answered");
    assert!(!fake.waits().is_empty());
}

#[test]
fn a_server_that_never_accepts_the_token_gives_pending_not_an_error() {
    let st = stand();
    let (fake, _token) = serve_stale(&st, answering("1"));
    let id = tmgr_ipc::decisions::new_card_id();
    let r = mcp(
        &st,
        "1500",
        "claude-code",
        &[("get_answer", json!({"card_id": id}))],
    );
    assert!(!is_error(&r[0]), "{}", r[0]);
    assert_eq!(payload(&r[0])["status"], "pending");
    assert_eq!(payload(&r[0])["card_id"], id);

    let out = ask(&st, &["--title", "T", "--wait", "--timeout", "2"]);
    assert_eq!(out.status.code(), Some(8), "{}", text(&out.stderr));
    assert!(
        !text(&out.stderr).contains("error"),
        "{}",
        text(&out.stderr)
    );
    assert!(fake.ops().is_empty());
}

#[test]
fn cards_are_spooled_when_the_token_is_never_accepted() {
    let st = stand();
    let (fake, _token) = serve_stale(&st, silent());
    let out = ask(&st, &["--title", "T", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["status"], "spooled");
    let r = mcp(
        &st,
        "300",
        "claude-code",
        &[("ask_owner", json!({"title": "T2"}))],
    );
    assert!(!is_error(&r[0]), "{}", r[0]);
    assert_eq!(payload(&r[0])["status"], "pending");
    assert_eq!(
        std::fs::read_dir(st.dir.path().join("inbox"))
            .unwrap()
            .count(),
        2
    );
    assert!(fake.ops().is_empty());
}

#[test]
fn reply_owner_reports_not_delivered_when_the_token_is_never_accepted() {
    let st = stand();
    let (fake, _token) = serve_stale(&st, silent());
    let id = tmgr_ipc::decisions::new_card_id();
    let r = mcp(
        &st,
        "300",
        "claude-code",
        &[("reply_owner", json!({"card_id": id, "text": "hi"}))],
    );
    assert!(is_error(&r[0]), "{}", r[0]);
    let msg = r[0]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(
        msg.contains("not delivered") && msg.contains("unauthorized"),
        "{msg}"
    );
    assert!(fake.ops().is_empty());
}

#[test]
fn pr_values_that_are_not_urls_or_references_pass_through_to_links_pr() {
    let st = stand();
    let fake = serve(&st, silent());
    for (given, kept) in [
        ("PR-77", "PR-77"),
        ("TM-9", "TM-9"),
        ("o/r#x", "o/r#x"),
        ("https://example.com/a b", "https://example.com/a b"),
    ] {
        let out = ask(&st, &["--title", "T", "--pr", given]);
        assert_eq!(out.status.code(), Some(0), "{given}: {}", text(&out.stderr));
        let c = fake.created().pop().unwrap();
        assert_eq!(c["links"]["pr"], kept);
        assert!(c["links"].get("items").is_none(), "{given}: {c}");
    }
    let out = ask(&st, &["--title", "T", "--pr", "o/r#5", "--pr", "PR-77"]);
    assert_eq!(out.status.code(), Some(0));
    let c = fake.created().pop().unwrap();
    assert_eq!(c["links"]["pr"], "https://github.com/o/r/pull/5");
    assert_eq!(c["links"]["items"].as_array().unwrap().len(), 1);
}

#[test]
fn values_with_a_foreign_scheme_are_refused_on_pr_and_task() {
    let st = stand();
    let fake = serve(&st, silent());
    for bad in [
        "javascript:alert(1)",
        "JAVASCRIPT:alert(1)",
        "java\tscript:alert(1)",
        "data:text/html,x",
        "file:///etc/passwd",
        "vbscript:x",
    ] {
        for flag in ["--pr", "--task"] {
            let out = ask(&st, &["--title", "T", flag, bad]);
            assert_ne!(out.status.code(), Some(0), "{flag} {bad:?}");
            assert!(
                text(&out.stderr).contains("only http and https"),
                "{flag} {bad:?}: {}",
                text(&out.stderr)
            );
        }
    }
    let out = ask(
        &st,
        &["--title", "T", "--pr", "o/r#5", "--pr", "javascript:x"],
    );
    assert_ne!(out.status.code(), Some(0));
    assert!(fake.created().is_empty());
}

#[test]
fn mcp_refuses_foreign_schemes_on_pr_prs_task_and_fact_links() {
    let st = stand();
    let fake = serve(&st, silent());
    let r = mcp(
        &st,
        "100",
        "claude-code",
        &[
            ("ask_owner", json!({"title": "T", "pr": "javascript:x"})),
            (
                "ask_owner",
                json!({"title": "T", "prs": ["data:text/html,x"]}),
            ),
            ("ask_owner", json!({"title": "T", "task": "file:///x"})),
            (
                "ask_owner",
                json!({"title": "T", "facts": [{"state": "ok", "text": "t", "link": "javascript:x"}]}),
            ),
            (
                "ask_owner",
                json!({"title": "T", "facts": [{"state": "ok", "text": "t", "link": "TM-9"}], "task": "TM-9", "pr": "PR-77"}),
            ),
        ],
    );
    for resp in &r[..4] {
        assert!(is_error(resp), "{resp}");
    }
    assert!(!is_error(&r[4]), "{}", r[4]);
    let cards = fake.created();
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0]["facts"][0]["link"], "TM-9");
}

#[test]
fn mcp_pr_and_prs_share_the_same_rules() {
    let st = stand();
    let fake = serve(&st, silent());
    let r = mcp(
        &st,
        "100",
        "claude-code",
        &[
            ("ask_owner", json!({"title": "T", "pr": "PR-77"})),
            (
                "ask_owner",
                json!({"title": "T", "pr": "PR-78", "prs": ["o/r#5"]}),
            ),
            ("ask_owner", json!({"title": "T", "prs": ["o/r#x"]})),
        ],
    );
    for resp in &r {
        assert!(!is_error(resp), "{resp}");
    }
    let cards = fake.created();
    assert_eq!(cards[0]["links"]["pr"], "PR-77");
    assert!(cards[0]["links"].get("items").is_none());
    assert_eq!(cards[1]["links"]["pr"], "PR-78");
    let items = cards[1]["links"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["url"], "https://github.com/o/r/pull/5");
    assert_eq!(cards[2]["links"]["pr"], "o/r#x");
    assert!(cards[2]["links"].get("items").is_none());
}

#[test]
fn task_references_that_are_not_urls_stay_accepted() {
    let st = stand();
    let fake = serve(&st, silent());
    let out = ask(&st, &["--title", "T", "--task", "TM-123"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let r = mcp(
        &st,
        "100",
        "claude-code",
        &[
            ("ask_owner", json!({"title": "T", "task": "TM-123"})),
            (
                "ask_owner",
                json!({"title": "T", "task": "https://tmgr.example/ws/tasks/1"}),
            ),
        ],
    );
    for resp in &r {
        assert!(!is_error(resp), "{resp}");
    }
    let cards = fake.created();
    assert_eq!(cards[0]["links"]["task"], "TM-123");
    assert!(cards[0]["links"].get("items").is_none());
    assert_eq!(cards[1]["links"]["task"], "TM-123");
    assert!(cards[1]["links"].get("items").is_none());
    assert_eq!(cards[2]["links"]["items"][0]["kind"], "task");
}

fn dropping(stand: &Stand) -> Arc<std::sync::atomic::AtomicUsize> {
    prepare_server_dir(stand.dir.path()).unwrap();
    let listener = match socket_endpoint(stand.dir.path()) {
        tmgr_ipc::endpoint::Endpoint::Path(p) => UnixListener::bind(p).unwrap(),
        #[allow(unreachable_patterns)]
        _ => unreachable!(),
    };
    let accepts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = accepts.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            drop(stream);
        }
    });
    accepts
}

#[test]
fn get_answer_backs_off_while_the_inbox_drops_connections() {
    let st = stand();
    let accepts = dropping(&st);
    let id = tmgr_ipc::decisions::new_card_id();
    let started = Instant::now();
    let r = mcp(
        &st,
        "600000",
        "claude-code",
        &[("get_answer", json!({"card_id": id, "waitSeconds": 3}))],
    );
    let took = started.elapsed();
    let v = payload(&r[0]);
    assert_eq!(v["status"], "pending");
    assert!(v["note"].as_str().unwrap().contains("not running"));
    assert!(took >= Duration::from_millis(2900), "{took:?}");
    let n = accepts.load(std::sync::atomic::Ordering::SeqCst);
    assert!((2..=10).contains(&n), "{n} connections");
}

#[test]
fn closing_stdin_cancels_a_pending_get_answer() {
    let st = stand();
    let fake = serve(
        &st,
        Box::new(|req| {
            if matches!(req.op, Op::Wait { .. }) {
                std::thread::sleep(Duration::from_secs(20));
            }
            Response::ok()
        }),
    );
    let mut child = base(&st, &["mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let id = tmgr_ipc::decisions::new_card_id();
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {"name": "get_answer", "arguments": {"card_id": id, "waitSeconds": 3600}}})
    )
    .unwrap();
    let started = Instant::now();
    while fake.waits().is_empty() {
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "no wait arrived"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let closed = Instant::now();
    drop(stdin);
    child.wait().unwrap();
    assert!(
        closed.elapsed() < Duration::from_secs(1),
        "{:?}",
        closed.elapsed()
    );
}

fn listener_in(stand: &Stand) -> UnixListener {
    prepare_server_dir(stand.dir.path()).unwrap();
    match socket_endpoint(stand.dir.path()) {
        tmgr_ipc::endpoint::Endpoint::Path(p) => UnixListener::bind(p).unwrap(),
        #[allow(unreachable_patterns)]
        _ => unreachable!(),
    }
}

#[test]
fn get_answer_keeps_backing_off_while_the_inbox_is_busy() {
    let st = stand();
    let listener = listener_in(&st);
    let accepts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let seen = accepts.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let _ = tmgr_ipc::decisions::write_busy(&mut stream);
        }
    });
    let id = tmgr_ipc::decisions::new_card_id();
    let r = mcp(
        &st,
        "600000",
        "claude-code",
        &[("get_answer", json!({"card_id": id, "waitSeconds": 3}))],
    );
    assert_eq!(payload(&r[0])["status"], "pending");
    let n = accepts.load(std::sync::atomic::Ordering::SeqCst);
    assert!((2..=10).contains(&n), "{n} connections");
}

struct Holding {
    held: Arc<std::sync::atomic::AtomicUsize>,
    closed: Arc<Mutex<Option<Instant>>>,
}

fn holding(stand: &Stand) -> Holding {
    let listener = listener_in(stand);
    let held = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let closed = Arc::new(Mutex::new(None));
    let (h, c) = (held.clone(), closed.clone());
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { continue };
            let (h, c) = (h.clone(), c.clone());
            std::thread::spawn(move || {
                let _ = read_line_limited(&stream);
                h.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                let mut buf = [0u8; 64];
                while std::io::Read::read(&mut &stream, &mut buf).is_ok_and(|n| n > 0) {}
                *c.lock().unwrap() = Some(Instant::now());
            });
        }
    });
    Holding { held, closed }
}

fn spawn_mcp(
    stand: &Stand,
) -> (
    std::process::Child,
    std::process::ChildStdin,
    Arc<Mutex<Vec<Value>>>,
) {
    let mut child = base(stand, &["mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdin = child.stdin.take().unwrap();
    let stdout = child.stdout.take().unwrap();
    let lines = Arc::new(Mutex::new(Vec::new()));
    let sink = lines.clone();
    std::thread::spawn(move || {
        for line in std::io::BufRead::lines(std::io::BufReader::new(stdout)) {
            let Ok(line) = line else { break };
            sink.lock()
                .unwrap()
                .push(serde_json::from_str(&line).unwrap());
        }
    });
    (child, stdin, lines)
}

fn until(what: &str, limit: Duration, mut done: impl FnMut() -> bool) {
    let started = Instant::now();
    while !done() {
        assert!(started.elapsed() < limit, "timed out: {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_cancelled_get_answer_drops_its_connection_and_sends_no_response() {
    let st = stand();
    let server = holding(&st);
    let (mut child, mut stdin, lines) = spawn_mcp(&st);
    let id = tmgr_ipc::decisions::new_card_id();
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {"name": "get_answer", "arguments": {"card_id": id, "waitSeconds": 600}}})
    )
    .unwrap();
    until(
        "the wait reaches the server",
        Duration::from_secs(5),
        || server.held.load(std::sync::atomic::Ordering::SeqCst) == 1,
    );
    let cancelled_at = Instant::now();
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 7, "reason": "test"}})
    )
    .unwrap();
    until("the connection closes", Duration::from_secs(1), || {
        server.closed.lock().unwrap().is_some()
    });
    assert!(cancelled_at.elapsed() < Duration::from_secs(1));
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc": "2.0", "id": 8, "method": "ping"})
    )
    .unwrap();
    until("the ping reply", Duration::from_secs(2), || {
        lines.lock().unwrap().iter().any(|r| r["id"] == 8)
    });
    let closing = Instant::now();
    drop(stdin);
    until("the process exits", Duration::from_secs(5), || {
        child.try_wait().unwrap().is_some()
    });
    assert!(closing.elapsed() < Duration::from_secs(5));
    std::thread::sleep(Duration::from_millis(100));
    assert!(lines.lock().unwrap().iter().all(|r| r["id"] != 7));
    assert_eq!(server.held.load(std::sync::atomic::Ordering::SeqCst), 1);
}

fn card(n: u8) -> String {
    format!("c_{:032x}", n)
}

#[test]
fn get_answer_with_card_ids_returns_the_first_event_from_any_card() {
    let st = stand();
    let (c1, c2, c3) = (card(1), card(2), card(3));
    let second = c2.clone();
    let fake = serve(
        &st,
        Box::new(move |req| match &req.op {
            Op::WaitAny { cards, .. } => Response {
                ok: true,
                event: Some(Event {
                    choice: Some("b".into()),
                    ..event(&cards[1].card_id, "answer", 4)
                }),
                ..Response::default()
            },
            _ => Response::err("bad request"),
        }),
    );
    let r = mcp(
        &st,
        "600000",
        "claude-code",
        &[(
            "get_answer",
            json!({"card_ids": [c1, c2, c3], "waitSeconds": 5}),
        )],
    );
    let v = payload(&r[0]);
    assert_eq!(v["card_id"], second);
    assert_eq!(v["type"], "answer");
    assert_eq!(v["choice"], "b");
    let any: Vec<_> = fake
        .ops()
        .into_iter()
        .filter(|op| matches!(op, Op::WaitAny { cards, .. } if cards.len() == 3))
        .collect();
    assert_eq!(any.len(), 1);
}

#[test]
fn get_answer_on_an_alias_moves_past_the_event_it_returned() {
    let st = stand();
    let (alias, canon) = (card(1), card(9));
    let fake = serve(
        &st,
        Box::new(move |req| match &req.op {
            Op::Wait { since: 0, .. } => Response {
                ok: true,
                event: Some(event(&canon, "message", 3)),
                ..Response::default()
            },
            Op::Wait { .. } => Response::ok(),
            _ => Response::err("bad request"),
        }),
    );
    let (mut child, mut stdin, lines) = spawn_mcp(&st);
    for id in [10, 11] {
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": "get_answer", "arguments": {"card_id": alias, "waitSeconds": 1}}})
        )
        .unwrap();
        until("the get_answer reply", Duration::from_secs(10), || {
            lines.lock().unwrap().iter().any(|r| r["id"] == id)
        });
    }
    drop(stdin);
    let _ = child.wait();
    let got = lines.lock().unwrap().clone();
    let reply = |id: u64| payload(got.iter().find(|r| r["id"] == id).unwrap());
    assert_eq!(reply(10)["type"], "message");
    assert_eq!(reply(11)["status"], "pending");
    let sinces: Vec<u64> = fake
        .ops()
        .into_iter()
        .filter_map(|op| match op {
            Op::Wait { since, .. } => Some(since),
            _ => None,
        })
        .collect();
    assert_eq!(sinces.first(), Some(&0));
    assert!(sinces[1..].iter().all(|s| *s == 3), "{sinces:?}");
}

#[test]
fn get_answer_with_card_ids_falls_back_on_an_old_desktop() {
    let st = stand();
    let (c1, c2, c3) = (card(1), card(2), card(3));
    let wanted = c2.clone();
    let fake = serve(
        &st,
        Box::new(move |req| match &req.op {
            Op::Wait { card_id, .. } if *card_id == wanted => Response {
                ok: true,
                event: Some(event(card_id, "answer", 1)),
                ..Response::default()
            },
            Op::Wait { .. } => Response::ok(),
            _ => Response::err("bad request"),
        }),
    );
    let r = mcp(
        &st,
        "600000",
        "claude-code",
        &[
            (
                "get_answer",
                json!({"card_ids": [c1, c2.clone(), c3], "waitSeconds": 5}),
            ),
            ("get_answer", json!({"card_id": c2, "waitSeconds": 5})),
        ],
    );
    assert_eq!(payload(&r[0])["card_id"], c2);
    assert_eq!(payload(&r[0])["type"], "answer");
    assert_eq!(payload(&r[1])["card_id"], c2);
    let probes = fake
        .ops()
        .into_iter()
        .filter(|op| matches!(op, Op::WaitAny { .. }))
        .count();
    assert_eq!(probes, 1);
}

#[test]
fn get_answer_needs_a_card_id_or_card_ids() {
    let st = stand();
    let r = mcp(
        &st,
        "600000",
        "claude-code",
        &[("get_answer", json!({"waitSeconds": 0}))],
    );
    assert!(is_error(&r[0]));
    assert!(r[0]["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("card_id or card_ids is required"));
}

#[test]
fn cancel_ends_a_get_answer_that_is_backing_off() {
    let st = stand();
    let accepts = dropping(&st);
    let (mut child, mut stdin, lines) = spawn_mcp(&st);
    let id = tmgr_ipc::decisions::new_card_id();
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc": "2.0", "id": 7, "method": "tools/call", "params": {"name": "get_answer", "arguments": {"card_id": id, "waitSeconds": 600}}})
    )
    .unwrap();
    until("the first attempt", Duration::from_secs(5), || {
        accepts.load(std::sync::atomic::Ordering::SeqCst) >= 2
    });
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc": "2.0", "method": "notifications/cancelled", "params": {"requestId": 7}})
    )
    .unwrap();
    let cancelled_at = Instant::now();
    drop(stdin);
    until("the process exits", Duration::from_secs(5), || {
        child.try_wait().unwrap().is_some()
    });
    assert!(cancelled_at.elapsed() < Duration::from_secs(2));
    assert!(lines.lock().unwrap().iter().all(|r| r["id"] != 7));
}

fn pending_rows() -> Handler {
    Box::new(|_| Response {
        ok: true,
        pending: vec![
            PendingDelivery {
                card_id: "c_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
                seq: 2,
                title: "Ship it?".into(),
                task: Some("https://tmgr.dev/ws/tasks/7".into()),
                answered_ms: 1_700_000_000_000,
                delivery_state: "none".into(),
                comment_skipped: Some("workspace mismatch".into()),
            },
            PendingDelivery {
                card_id: "c_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
                seq: 1,
                title: "Roll back?".into(),
                task: None,
                answered_ms: 1_700_000_100_000,
                delivery_state: "undelivered".into(),
                comment_skipped: None,
            },
        ],
        ..Response::ok()
    })
}

#[test]
fn pending_delivery_json_lists_the_rows() {
    let st = stand();
    let fake = serve(&st, pending_rows());
    let out = base(&st, &["decisions", "pending-delivery", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = v["data"]["pending"].as_array().unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["card_id"], "c_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    assert_eq!(rows[0]["delivery_state"], "none");
    assert_eq!(rows[1]["task"], Value::Null);
    assert_eq!(rows[0]["comment_skipped"], "workspace mismatch");
    assert!(rows[1]
        .as_object()
        .unwrap()
        .get("comment_skipped")
        .unwrap()
        .is_null());
    assert_eq!(fake.ops(), [Op::PendingDelivery]);
}

#[test]
fn pending_delivery_prints_a_table_and_a_clear_empty_line() {
    let st = stand();
    let _fake = serve(&st, pending_rows());
    let out = base(&st, &["decisions", "pending-delivery"])
        .output()
        .unwrap();
    let t = text(&out.stdout);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(t.contains("Ship it?") && t.contains("Roll back?"), "{t}");
    assert!(
        t.contains("undelivered") && t.contains("c_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"),
        "{t}"
    );
    assert!(t.contains("2023-11-14"), "{t}");
    assert!(t.contains("(skipped: workspace mismatch)"), "{t}");

    let st = stand();
    let _fake = serve(&st, Box::new(|_| Response::ok()));
    let out = base(&st, &["decisions", "pending-delivery"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(text(&out.stdout).contains("Every answer was picked up"));
}

#[test]
fn pending_delivery_on_an_old_desktop_says_so_and_fails() {
    let st = stand();
    let _fake = serve(&st, Box::new(|_| Response::err("bad request")));
    let out = base(&st, &["decisions", "pending-delivery"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    assert!(text(&out.stderr).contains("desktop too old, update TMGR desktop"));
    assert!(text(&out.stdout).is_empty());
}
