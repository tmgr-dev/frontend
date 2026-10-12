mod common;
use common::*;
use serde_json::{json, Value};
use std::io::{Read, Write};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

const TOKEN: &str = "tmgrn_mcptoken_SECRET0010";

fn session(env: &Env, lines: &[Value]) -> (String, String) {
    let mut cmd = env.std_cmd();
    cmd.env("TMGR_NOTIFY_TOKEN", TOKEN)
        .env("TMGR_URL", env.server.uri())
        .arg("mcp")
        .stdin(std::process::Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    for l in lines {
        writeln!(stdin, "{l}").unwrap();
    }
    drop(stdin);
    let mut stdout = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    let mut stderr = String::new();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut stderr)
        .unwrap();
    assert!(child.wait().unwrap().success());
    (stdout, stderr)
}

fn responses(stdout: &str) -> Vec<Value> {
    stdout
        .lines()
        .map(|l| {
            serde_json::from_str(l)
                .unwrap_or_else(|e| panic!("stdout must be JSON-RPC only: {l:?} ({e})"))
        })
        .collect()
}

fn by_id(all: &[Value], id: i64) -> &Value {
    all.iter()
        .find(|r| r["id"] == id)
        .unwrap_or_else(|| panic!("no response for {id}: {all:?}"))
}

fn init(version: &str) -> Vec<Value> {
    vec![
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": version, "capabilities": {}, "clientInfo": {"name": "test", "version": "1"}}}),
        json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
    ]
}

#[tokio::test(flavor = "multi_thread")]
async fn handshake_lists_the_tools_and_negotiates_the_version() {
    let env = Env::new().await;
    let mut lines = init("2025-03-26");
    lines.push(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    lines.push(json!({"jsonrpc": "2.0", "id": 3, "method": "ping"}));
    lines.push(json!({"jsonrpc": "2.0", "id": 4, "method": "bogus/method"}));
    let (stdout, stderr) = session(&env, &lines);
    assert!(stderr.is_empty() || !stderr.contains(TOKEN));
    let all = responses(&stdout);
    assert_eq!(all.len(), 4, "the initialized notification gets no reply");
    assert_eq!(by_id(&all, 1)["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(by_id(&all, 1)["result"]["serverInfo"]["name"], "tmgr");
    assert!(by_id(&all, 1)["result"]["capabilities"]["tools"].is_object());
    let names: Vec<&str> = by_id(&all, 2)["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["notify_user", "alarm", "alarm_status", "tmgr_cli"]);
    assert_eq!(by_id(&all, 3)["result"], json!({}));
    assert_eq!(by_id(&all, 4)["error"]["code"], -32601);

    let (stdout, _) = session(&env, &init("2031-01-01"));
    assert_eq!(
        by_id(&responses(&stdout), 1)["result"]["protocolVersion"],
        "2025-11-25"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn tools_call_reaches_the_server_and_never_returns_the_token() {
    let env = Env::new().await;
    Mock::given(method("POST"))
        .and(path("/api/notifications/push"))
        .respond_with(
            ResponseTemplate::new(202)
                .set_body_json(json!({"data": {"status": "sent", "channels": ["mobile"]}})),
        )
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/alarms"))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(json!({"data": {"id": 12, "status": "pending"}})),
        )
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/alarms/12"))
        .and(query_param("waitSeconds", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {"id": 12, "status": "acknowledged", "ackChannel": "app", "callAttempts": 3, "callAttemptsMade": 0}})))
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/alarms/12"))
        .and(query_param("waitSeconds", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({"data": {"id": 12, "status": "calling", "callStatus": "ringing"}}),
        ))
        .mount(&env.server)
        .await;
    let call = |id: i64, name: &str, args: Value, meta: Value| json!({"jsonrpc": "2.0", "id": id, "method": "tools/call", "params": {"name": name, "arguments": args, "_meta": meta}});
    let mut lines = init("2025-06-18");
    lines.push(call(
        2,
        "notify_user",
        json!({"title": "Done", "body": "Build ok", "priority": "low"}),
        json!({}),
    ));
    lines.push(call(
        3,
        "alarm",
        json!({"title": "DB", "message": "down", "callAttempts": 2}),
        json!({"progressToken": "p1"}),
    ));
    lines.push(call(4, "alarm_status", json!({"id": "12"}), json!({})));
    lines.push(call(5, "tmgr_cli", json!({}), json!({})));
    lines.push(call(6, "notify_user", json!({"title": ""}), json!({})));
    lines.push(call(7, "alarm_status", json!({"id": "../x"}), json!({})));
    lines.push(call(8, "nope", json!({}), json!({})));
    let (stdout, stderr) = session(&env, &lines);
    assert!(!stdout.contains(TOKEN) && !stderr.contains(TOKEN));
    let all = responses(&stdout);
    let text_of = |id| {
        by_id(&all, id)["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(text_of(2), "sent");
    assert!(by_id(&all, 2)["result"].get("isError").is_none());
    assert_eq!(
        text_of(3),
        "alarm 12: acknowledged, channel: app, call: none, attempts: 0/3"
    );
    assert_eq!(text_of(4), "alarm 12: calling, call: ringing");
    assert!(text_of(5).contains("tmgr agent start"));
    for id in [6, 7, 8] {
        assert_eq!(by_id(&all, id)["result"]["isError"], true, "{id}");
    }
    let progress: Vec<&Value> = all
        .iter()
        .filter(|r| r["method"] == "notifications/progress")
        .collect();
    assert!(!progress.is_empty());
    assert_eq!(progress[0]["params"]["progressToken"], "p1");
    let push: Value =
        serde_json::from_slice(&env.requests_to("/api/notifications/push").await[0].body).unwrap();
    assert_eq!(
        push,
        json!({"title": "Done", "body": "Build ok", "priority": "low"})
    );
    let alarm: Value =
        serde_json::from_slice(&env.requests_to("/api/alarms").await[0].body).unwrap();
    assert_eq!(
        alarm,
        json!({"title": "DB", "message": "down", "callAttempts": 2})
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn server_errors_become_tool_errors_in_the_typescript_wording() {
    let env = Env::new().await;
    Mock::given(method("POST"))
        .and(path("/api/notifications/push"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "7"))
        .mount(&env.server)
        .await;
    let mut lines = init("2025-06-18");
    lines.push(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "notify_user", "arguments": {"title": "x"}}}));
    let (stdout, _) = session(&env, &lines);
    let all = responses(&stdout);
    let r = &by_id(&all, 2)["result"];
    assert_eq!(r["isError"], true);
    assert_eq!(r["content"][0]["text"], "rate limited (retry after 7s)");
}

#[tokio::test(flavor = "multi_thread")]
async fn garbage_lines_get_a_parse_error_and_the_server_keeps_going() {
    let env = Env::new().await;
    let mut cmd = env.std_cmd();
    cmd.arg("mcp").stdin(std::process::Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"{oops\n[1]\n").unwrap();
    writeln!(
        stdin,
        "{}",
        json!({"jsonrpc": "2.0", "id": 9, "method": "ping"})
    )
    .unwrap();
    drop(stdin);
    let mut stdout = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut stdout)
        .unwrap();
    child.wait().unwrap();
    let all = responses(&stdout);
    assert_eq!(all[0]["error"]["code"], -32700);
    assert_eq!(all[1]["error"]["code"], -32600);
    assert_eq!(by_id(&all, 9)["result"], json!({}));
}
