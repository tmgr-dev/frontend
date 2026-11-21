mod common;
use common::{git, init_repo, path_with, seed_remote, text, Env};
use serde_json::{json, Value};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;
use tmgr_ipc::decisions::{
    prepare_server_dir, read_line_limited, socket_endpoint, token_ok, write_line, Event, Op,
    Request, Response,
};

type Handler = Box<dyn Fn(&Request) -> Response + Send + Sync>;

struct Stand {
    home: TempDir,
    dir: TempDir,
}

struct Fake {
    requests: Arc<Mutex<Vec<Request>>>,
}

impl Fake {
    fn created(&self) -> Vec<Value> {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter_map(|r| match &r.op {
                Op::Create { card, .. } => Some(serde_json::to_value(&**card).unwrap()),
                _ => None,
            })
            .collect()
    }
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

fn desktop(capabilities: &[&str], final_event: Value) -> Handler {
    let caps: Vec<String> = capabilities.iter().map(|c| c.to_string()).collect();
    Box::new(move |req| match &req.op {
        Op::Ping => Response {
            ok: true,
            capabilities: caps.clone(),
            ..Response::default()
        },
        Op::Create { card, .. } => Response {
            ok: true,
            card_id: card.id.clone(),
            ..Response::default()
        },
        Op::Wait { card_id, .. } => {
            let mut v = final_event.clone();
            v["card_id"] = json!(card_id);
            v["type"] = json!("answer");
            v["seq"] = json!(1);
            Response {
                ok: true,
                event: Some(serde_json::from_value::<Event>(v).unwrap()),
                ..Response::default()
            }
        }
        _ => Response::ok(),
    })
}

fn approved_ok() -> Value {
    json!({"choice": "approve", "result": {"ok": true, "url": "https://github.com/acme/widgets/pull/7"}})
}

fn tmgr(stand: &Stand, server: &str, args: &[&str]) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_tmgr"));
    c.env_clear()
        .env("HOME", stand.home.path())
        .env("XDG_CONFIG_HOME", stand.home.path().join("xdg"))
        .env("XDG_DATA_HOME", stand.home.path().join("data"))
        .env("TMGR_CREDENTIAL_STORE", "file")
        .env("TMGR_DECISIONS_DIR", stand.dir.path())
        .env("TMGR_SERVER", server)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("NO_COLOR", "1")
        .env("PATH", path_with(None))
        .args(args);
    c
}

fn run(stand: &Stand, args: &[&str]) -> Output {
    tmgr(stand, "http://127.0.0.1:9", args).output().unwrap()
}

struct Repo {
    _tmp: TempDir,
    work: PathBuf,
    bare: PathBuf,
}

const GITHUB: &str = "https://github.com/acme/widgets.git";

fn repo() -> Repo {
    let tmp = tempfile::tempdir().unwrap();
    let bare = tmp.path().join("remote.git");
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "--bare", "-q", "-b", "main", "."]);
    let work = init_repo(&tmp.path().join("work"));
    seed_remote(&work, &bare);
    git(&work, &["remote", "add", "origin", GITHUB]);
    git(
        &work,
        &[
            "config",
            &format!("url.{}.insteadOf", bare.display()),
            GITHUB,
        ],
    );
    git(&work, &["fetch", "-q", "origin"]);
    git(&work, &["switch", "-q", "-c", "feat"]);
    std::fs::write(work.join("a.txt"), "a\n").unwrap();
    git(&work, &["add", "a.txt"]);
    git(&work, &["commit", "-q", "-m", "add a"]);
    git(&work, &["push", "-q", "origin", "feat"]);
    git(&work, &["fetch", "-q", "origin"]);
    Repo {
        _tmp: tmp,
        work,
        bare,
    }
}

fn pr_args(repo: &Path) -> Vec<&str> {
    vec![
        "propose",
        "pr",
        "--repo-dir",
        repo.to_str().unwrap(),
        "--title",
        "Add a",
        "--body",
        "why",
    ]
}

#[test]
fn pr_card_carries_the_resolved_payload_and_the_result_url_is_printed() {
    let st = stand();
    let r = repo();
    let fake = serve(&st, desktop(&["actions"], approved_ok()));
    let out = run(&st, &pr_args(&r.work));
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("https://github.com/acme/widgets/pull/7"));
    let cards = fake.created();
    assert_eq!(cards.len(), 1);
    let c = &cards[0];
    assert_eq!(c["title"], "PR: Add a");
    assert_eq!(c["action"]["kind"], "pr");
    let p = &c["action"]["payload"];
    let head = git(&r.work, &["rev-parse", "feat"]);
    let base = git(&r.work, &["rev-parse", "main"]);
    assert_eq!(p["repo"], "acme/widgets");
    assert_eq!(p["branch"], "feat");
    assert_eq!(p["base"], "main");
    assert_eq!(p["head_sha"], head.trim());
    assert_eq!(p["base_sha"], base.trim());
    assert_eq!(p["commits"][0]["subject"], "add a");
    assert_eq!(p["commits"].as_array().unwrap().len(), 1);
    assert_eq!(c["diff"]["repo"], r.work.to_str().unwrap());
    assert_eq!(c["diff"]["base"], base.trim());
    assert_eq!(c["diff"]["head"], head.trim());
}

#[test]
fn an_unpushed_branch_is_refused_before_anything_is_sent() {
    let st = stand();
    let r = repo();
    std::fs::write(r.work.join("b.txt"), "b\n").unwrap();
    git(&r.work, &["add", "b.txt"]);
    git(&r.work, &["commit", "-q", "-m", "more"]);
    let fake = serve(&st, desktop(&["actions"], approved_ok()));
    let out = run(&st, &pr_args(&r.work));
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("push the branch first"));
    assert!(fake.created().is_empty());
}

#[test]
fn a_non_github_origin_is_refused() {
    let st = stand();
    let r = repo();
    git(
        &r.work,
        &[
            "remote",
            "set-url",
            "origin",
            "https://gitlab.com/acme/widgets.git",
        ],
    );
    let fake = serve(&st, desktop(&["actions"], approved_ok()));
    let out = run(&st, &pr_args(&r.work));
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("github.com"));
    assert!(fake.created().is_empty());
}

#[test]
fn a_desktop_without_the_actions_capability_is_refused_and_gets_no_card() {
    let st = stand();
    let r = repo();
    let fake = serve(&st, desktop(&[], approved_ok()));
    let out = run(&st, &pr_args(&r.work));
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("update the TMGR desktop app to approve actions"));
    assert!(fake.created().is_empty());
}

#[test]
fn without_a_desktop_the_command_fails_and_nothing_is_spooled() {
    let st = stand();
    let r = repo();
    let out = run(&st, &pr_args(&r.work));
    assert_eq!(out.status.code(), Some(7));
    assert!(text(&out.stderr).contains("never queued"));
    let leftovers: Vec<_> = std::fs::read_dir(st.dir.path())
        .map(|d| d.flatten().map(|e| e.file_name()).collect())
        .unwrap_or_default();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    let spool = st.home.path().join("data");
    assert!(!spool.exists(), "{spool:?}");
}

#[test]
fn a_rejection_prints_the_reason_and_exits_11() {
    let st = stand();
    let r = repo();
    serve(
        &st,
        desktop(
            &["actions"],
            json!({"choice": "reject", "note": "too early"}),
        ),
    );
    let out = run(&st, &pr_args(&r.work));
    assert_eq!(out.status.code(), Some(11));
    assert!(text(&out.stdout).contains("rejected: too early"));
}

#[test]
fn an_approved_action_that_failed_exits_12_with_the_error() {
    let st = stand();
    let r = repo();
    serve(
        &st,
        desktop(
            &["actions"],
            json!({"choice": "approve", "result": {"ok": false, "error": "branch moved since approval"}}),
        ),
    );
    let out = run(&st, &pr_args(&r.work));
    assert_eq!(out.status.code(), Some(12));
    assert!(text(&out.stdout).contains("branch moved since approval"));
}

#[test]
fn json_prints_the_final_event_with_approved_and_result() {
    let st = stand();
    let r = repo();
    serve(&st, desktop(&["actions"], approved_ok()));
    let mut args = pr_args(&r.work);
    args.push("--json");
    let out = run(&st, &args);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["result"]["ok"], true);
    assert_eq!(v["data"]["choice"], "approve");
}

#[test]
fn no_wait_prints_the_card_id_and_returns() {
    let st = stand();
    let r = repo();
    let fake = serve(&st, desktop(&["actions"], approved_ok()));
    let mut args = pr_args(&r.work);
    args.push("--no-wait");
    let out = run(&st, &args);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let id = fake.created()[0]["id"].as_str().unwrap().to_string();
    assert!(text(&out.stdout).contains(&id));
}

#[test]
fn tag_card_resolves_the_target_sha_and_checks_the_remote() {
    let st = stand();
    let r = repo();
    let fake = serve(
        &st,
        desktop(
            &["actions"],
            json!({"choice": "approve", "result": {"ok": true, "sha": "abc"}}),
        ),
    );
    let dir = r.work.to_str().unwrap();
    let out = run(
        &st,
        &[
            "propose",
            "tag",
            "--repo-dir",
            dir,
            "--name",
            "v1.0.0",
            "--target",
            "feat",
        ],
    );
    assert!(out.status.success(), "{}", text(&out.stderr));
    let head = git(&r.work, &["rev-parse", "feat"]);
    let p = &fake.created()[0]["action"]["payload"];
    assert_eq!(fake.created()[0]["action"]["kind"], "tag");
    assert_eq!(p["repo"], "acme/widgets");
    assert_eq!(p["name"], "v1.0.0");
    assert_eq!(p["target_sha"], head.trim());
}

#[test]
fn tag_on_an_unpushed_commit_or_an_existing_name_is_refused() {
    let st = stand();
    let r = repo();
    std::fs::write(r.work.join("c.txt"), "c\n").unwrap();
    git(&r.work, &["add", "c.txt"]);
    git(&r.work, &["commit", "-q", "-m", "local only"]);
    let fake = serve(&st, desktop(&["actions"], approved_ok()));
    let dir = r.work.to_str().unwrap();
    let out = run(&st, &["propose", "tag", "--repo-dir", dir, "--name", "v1"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("push first"));

    git(&r.work, &["tag", "v0.9", "origin/feat"]);
    git(&r.work, &["push", "-q", "origin", "v0.9"]);
    let out = run(
        &st,
        &[
            "propose",
            "tag",
            "--repo-dir",
            dir,
            "--name",
            "v0.9",
            "--target",
            "origin/feat",
        ],
    );
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("already exists"),
        "{}",
        text(&out.stderr)
    );
    assert!(fake.created().is_empty());
    let _ = &r.bare;
}

#[tokio::test(flavor = "multi_thread")]
async fn comment_card_binds_origin_user_and_task() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_json(
        "/api/tasks/by-key/TM-12",
        json!({"data": common::task_json(8974, "TM-12: Fix the thing")}),
    )
    .await;
    env.mock_json(
        "/api/auth/devices/current",
        json!({"data": {"user": {"id": 42, "name": "Ann"}, "scopes": ["tasks:write"]}}),
    )
    .await;
    let st = stand();
    let fake = serve(
        &st,
        desktop(
            &["actions"],
            json!({"choice": "approve", "result": {"ok": true, "url": "https://x/c"}}),
        ),
    );
    let mut cmd = tmgr(
        &st,
        &env.server.uri(),
        &[
            "propose",
            "comment",
            "--task",
            "TM-12",
            "--text",
            "looks done",
        ],
    );
    cmd.env("HOME", env.tmp.path())
        .env("XDG_CONFIG_HOME", env.xdg());
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let c = &fake.created()[0];
    assert_eq!(c["action"]["kind"], "comment");
    let p = &c["action"]["payload"];
    assert_eq!(p["origin"], env.server.uri());
    assert_eq!(p["user_id"], 42);
    assert_eq!(p["task_id"], 8974);
    assert_eq!(p["task_label"], "TM-12: Fix the thing");
    assert_eq!(p["text"], "looks done");
}

#[tokio::test(flavor = "multi_thread")]
async fn task_card_resolves_the_workspace_code() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_json(
        "/api/auth/devices/current",
        json!({"data": {"user": {"id": 42, "name": "Ann"}, "scopes": []}}),
    )
    .await;
    let st = stand();
    let fake = serve(
        &st,
        desktop(
            &["actions"],
            json!({"choice": "approve", "result": {"ok": true, "url": "https://x/t"}}),
        ),
    );
    let mut cmd = tmgr(
        &st,
        &env.server.uri(),
        &[
            "propose",
            "task",
            "--workspace",
            "work",
            "--title",
            "New thing",
            "--category",
            "5",
        ],
    );
    cmd.env("HOME", env.tmp.path())
        .env("XDG_CONFIG_HOME", env.xdg());
    let out = tokio::task::spawn_blocking(move || cmd.output().unwrap())
        .await
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let p = &fake.created()[0]["action"]["payload"];
    assert_eq!(p["workspace_id"], 2);
    assert_eq!(p["category_id"], 5);
    assert_eq!(p["title"], "New thing");
    assert_eq!(p["user_id"], 42);
}

#[test]
fn an_over_limit_text_is_refused_not_cut() {
    let st = stand();
    let fake = serve(&st, desktop(&["actions"], approved_ok()));
    let long = "x".repeat(6001);
    let out = run(
        &st,
        &["propose", "task", "--workspace", "1", "--title", &long],
    );
    assert!(!out.status.success());
    assert!(fake.created().is_empty());
}

fn mcp(stand: &Stand, calls: &[(&str, Value)]) -> Vec<Value> {
    let mut cmd = tmgr(stand, "http://127.0.0.1:9", &["mcp"]);
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "claude-code", "version": "1"}}})
    )
    .unwrap();
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})
    )
    .unwrap();
    for (i, (name, args)) in calls.iter().enumerate() {
        writeln!(
            stdin,
            "{}",
            json!({"jsonrpc": "2.0", "id": i + 10, "method": "tools/call", "params": {"name": name, "arguments": args}})
        )
        .unwrap();
    }
    std::thread::sleep(Duration::from_millis(1500));
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    text(&out.stdout)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

fn by_id(all: &[Value], id: u64) -> &Value {
    all.iter().find(|r| r["id"] == id).unwrap()
}

#[test]
fn the_mcp_tool_is_listed_and_describes_the_approval_rules() {
    let st = stand();
    let all = mcp(&st, &[]);
    let tools = by_id(&all, 2)["result"]["tools"]
        .as_array()
        .unwrap()
        .clone();
    let t = tools
        .iter()
        .find(|t| t["name"] == "propose_action")
        .unwrap();
    let d = t["description"].as_str().unwrap();
    assert!(d.contains("only if the owner approves"));
    assert!(d.contains("may edit"));
    assert!(d.contains("reason"));
    assert_eq!(t["inputSchema"]["required"], json!(["kind"]));
    assert_eq!(
        t["inputSchema"]["properties"]["kind"]["enum"],
        json!(["comment", "task", "pr", "tag"])
    );
}

#[test]
fn the_mcp_tool_returns_the_card_id_without_waiting() {
    let st = stand();
    let r = repo();
    let fake = serve(&st, desktop(&["actions"], approved_ok()));
    let all = mcp(
        &st,
        &[(
            "propose_action",
            json!({"kind": "pr", "title": "Add a", "repo_dir": r.work.to_str().unwrap()}),
        )],
    );
    let resp = by_id(&all, 10);
    assert_ne!(resp["result"]["isError"], true, "{resp}");
    let v: Value =
        serde_json::from_str(resp["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(v["status"], "proposed");
    let id = fake.created()[0]["id"].as_str().unwrap().to_string();
    assert_eq!(v["card_id"], id);
    assert_eq!(fake.created()[0]["reply_via"], "mcp");
}

#[test]
fn the_mcp_tool_refuses_an_old_desktop_and_bad_kinds() {
    let st = stand();
    let r = repo();
    let fake = serve(&st, desktop(&[], approved_ok()));
    let all = mcp(
        &st,
        &[
            (
                "propose_action",
                json!({"kind": "pr", "title": "Add a", "repo_dir": r.work.to_str().unwrap()}),
            ),
            ("propose_action", json!({"kind": "merge"})),
        ],
    );
    let old = by_id(&all, 10);
    assert_eq!(old["result"]["isError"], true);
    assert!(old["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("update the TMGR desktop app"));
    assert_eq!(by_id(&all, 11)["result"]["isError"], true);
    assert!(fake.created().is_empty());
}
