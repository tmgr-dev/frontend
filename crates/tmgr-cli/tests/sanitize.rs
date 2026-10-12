mod common;
use common::*;
use serde_json::{json, Value};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

const EVIL: &str = "\u{1b}]0;pwn\u{7}\u{1b}[31mX\u{9b}2J\u{202e}Z\u{85}";

fn evil(label: &str) -> String {
    format!("{label}{EVIL}")
}

fn assert_clean(what: &str, out: &std::process::Output) {
    let all = format!("{}{}", text(&out.stdout), text(&out.stderr));
    for bad in ['\u{1b}', '\u{7}', '\u{9b}', '\u{202e}', '\u{85}'] {
        assert!(
            !all.contains(bad),
            "{what}: output contains {bad:?}\n{all:?}"
        );
    }
}

async fn auth_get(env: &Env, p: &str, body: Value) {
    Mock::given(method("GET"))
        .and(path(p))
        .and(header("authorization", format!("Bearer {ACCESS}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&env.server)
        .await;
}

async fn fixtures(env: &Env) {
    env.login_seed();
    auth_get(
        env,
        "/api/workspaces",
        json!({"data": [{"id": 1, "name": evil("ws"), "code": "personal", "is_default": true}]}),
    )
    .await;
    auth_get(
        env,
        "/api/workspaces/1/statuses",
        json!([{"id": 11, "name": evil("st"), "type": evil("ty")}, {"id": 10, "name": "Backlog", "type": "default"}]),
    )
    .await;
    let mut t = task_json(8974, &format!("TM-512: {}", evil("title")));
    t["description"] = json!(evil("desc"));
    t["category"] = json!({"id": 5, "title": evil("cat"), "code": "TM"});
    t["assignees"] = json!([{"id": 3, "name": evil("who")}]);
    t["created_at"] = json!(evil("created"));
    auth_get(env, "/api/tasks/8974", json!({"data": t.clone()})).await;
    auth_get(
        env,
        "/api/tasks/8974/comments",
        json!([{"id": 1, "message": evil("msg"), "created_at": evil("when"), "user": {"id": 3, "name": evil("author")}}]),
    )
    .await;
    auth_get(
        env,
        "/api/tasks",
        json!({"data": [t], "meta": {"current_page": 1, "last_page": 1}}),
    )
    .await;
    auth_get(
        env,
        "/api/auth/devices/current",
        json!({"data": {"device_id": 1, "user": {"id": 1, "name": evil("name"), "email": evil("mail")},
            "scopes": [evil("scope")], "expires_at": evil("exp")}}),
    )
    .await;
    auth_get(
        env,
        "/api/workspaces/1/time-summary",
        json!({"data": {"total_s": 60, "items": []}}),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn no_server_string_can_inject_terminal_sequences() {
    let env = Env::new().await;
    fixtures(&env).await;
    let runs: Vec<Vec<&str>> = vec![
        vec!["whoami"],
        vec!["ws", "list"],
        vec![],
        vec!["task", "show", "8974"],
        vec!["task", "list"],
        vec!["task", "list", "--status-type", "active"],
    ];
    for args in runs {
        let out = env.cmd().args(&args).output().unwrap();
        assert!(
            out.status.success() || args.is_empty(),
            "{args:?}: {}",
            text(&out.stderr)
        );
        assert_clean(&format!("{args:?}"), &out);
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn login_and_add_output_is_clean_too() {
    let env = Env::new().await;
    fixtures(&env).await;
    Mock::given(method("POST"))
        .and(path("/api/tasks"))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"data": {
            "id": 9, "title": format!("TM-1: {}", evil("t")), "status_id": 11, "workspace_id": 1}})))
        .mount(&env.server)
        .await;
    let out = env.cmd().args(["add", "x"]).output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_clean("add", &out);

    let fresh = Env::new().await;
    Mock::given(method("GET"))
        .and(path("/api/auth/devices/current"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "user": {"name": evil("n")}, "scopes": [evil("s")]}})))
        .mount(&fresh.server)
        .await;
    let out = fresh
        .cmd()
        .args(["login", "--with-token"])
        .write_stdin("tok\n")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_clean("login", &out);
}

#[tokio::test(flavor = "multi_thread")]
async fn error_messages_and_warnings_are_clean() {
    let env = Env::new().await;
    env.login_seed();
    Mock::given(method("GET"))
        .and(path("/api/workspaces"))
        .respond_with(ResponseTemplate::new(404).set_body_json(json!({"message": evil("gone")})))
        .mount(&env.server)
        .await;
    let out = env.cmd().args(["ws", "list"]).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
    assert_clean("error", &out);
    let out = env
        .cmd()
        .args(["task", "show", &evil("x")])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert_clean("usage", &out);
}
