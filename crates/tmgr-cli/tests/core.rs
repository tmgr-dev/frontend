mod common;
use common::*;
use serde_json::json;
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, ResponseTemplate};

#[tokio::test(flavor = "multi_thread")]
async fn a_401_triggers_one_refresh_and_a_retry_with_the_rotated_tokens() {
    let env = Env::new().await;
    env.login_seed();
    Mock::given(method("GET"))
        .and(path("/api/workspaces"))
        .and(header("authorization", format!("Bearer {ACCESS}").as_str()))
        .respond_with(ResponseTemplate::new(401))
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/auth/refresh"))
        .and(body_partial_json(json!({"refreshToken": REFRESH})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "token": "new-access-CCCC", "token_type": "Bearer", "expires_in": 3600,
            "refresh_token": "new-refresh-DDDD", "refresh_expires_in": 86400,
            "user": {"id": 1, "name": "Ann", "email": "ann@example.com"}
        }})))
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/workspaces"))
        .and(header("authorization", "Bearer new-access-CCCC"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [
            {"id": 1, "name": "Personal", "code": "personal", "is_default": true}
        ]})))
        .mount(&env.server)
        .await;

    let out = env.cmd().args(["ws", "list", "--json"]).output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(env.requests_to("/api/auth/refresh").await.len(), 1);
    assert_eq!(env.requests_to("/api/workspaces").await.len(), 2);
    let creds = env.creds_text();
    assert!(creds.contains("new-access-CCCC") && creds.contains("new-refresh-DDDD"));
    assert!(!creds.contains(REFRESH));
    assert_no_secrets(&out);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_refresh_is_exit_4_and_not_retried() {
    let env = Env::new().await;
    env.login_seed();
    Mock::given(method("GET"))
        .and(path("/api/workspaces"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/auth/refresh"))
        .respond_with(
            ResponseTemplate::new(401)
                .set_body_json(json!({"message": "The refresh token is invalid or has expired."})),
        )
        .mount(&env.server)
        .await;
    let out = env.cmd().args(["ws", "list", "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(4));
    let v = stdout_json(&out);
    assert_valid("error", &v);
    assert_eq!(v["error"]["hint"], "tmgr login");
    assert_eq!(env.requests_to("/api/auth/refresh").await.len(), 1);
    assert_eq!(env.requests_to("/api/workspaces").await.len(), 1);
    assert_no_secrets(&out);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_token_without_a_refresh_token_is_exit_4_on_401() {
    let env = Env::new().await;
    env.seed_tokens(ACCESS, None);
    Mock::given(method("GET"))
        .and(path("/api/workspaces"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&env.server)
        .await;
    let out = env.cmd().args(["ws", "list"]).output().unwrap();
    assert_eq!(out.status.code(), Some(4));
    assert!(env.requests_to("/api/auth/refresh").await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn insufficient_scope_is_exit_5_with_a_login_hint() {
    let env = Env::new().await;
    env.login_seed();
    Mock::given(method("GET"))
        .and(path("/api/workspaces"))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(json!({"error": "insufficient_scope"})),
        )
        .mount(&env.server)
        .await;
    let out = env.cmd().args(["ws", "list"]).output().unwrap();
    assert_eq!(out.status.code(), Some(5));
    let err = text(&out.stderr);
    assert!(
        err.contains("error: this login does not allow that (needs scope workspaces:read)"),
        "{err}"
    );
    assert!(
        err.contains("hint: tmgr login --scopes workspaces:read,"),
        "{err}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unreachable_server_is_exit_7() {
    let env = Env::new().await;
    let dead = dead_server();
    env.seed_account(
        &format!("{}:default", dead.trim_start_matches("http://")),
        ACCESS,
        Some(REFRESH),
    );
    let out = env
        .cmd_for(&dead)
        .args(["ws", "list", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(7));
    let v = stdout_json(&out);
    assert_valid("error", &v);
    assert_eq!(v["error"]["code"], "offline");
    assert_no_secrets(&out);
}

#[tokio::test(flavor = "multi_thread")]
async fn conflict_and_server_errors_map_to_their_exit_codes() {
    let env = Env::new().await;
    env.login_seed();
    Mock::given(method("GET"))
        .and(path("/api/workspaces"))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({"message": "busy"})))
        .mount(&env.server)
        .await;
    let out = env.cmd().args(["ws", "list"]).output().unwrap();
    assert_eq!(out.status.code(), Some(6));
}

#[tokio::test(flavor = "multi_thread")]
async fn redirects_are_never_followed_so_the_token_cannot_leak() {
    let env = Env::new().await;
    env.login_seed();
    Mock::given(method("GET"))
        .and(path("/api/workspaces"))
        .respond_with(
            ResponseTemplate::new(302).insert_header("location", "http://127.0.0.1:1/steal"),
        )
        .mount(&env.server)
        .await;
    let out = env.cmd().args(["ws", "list"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
}

#[tokio::test(flavor = "multi_thread")]
async fn verbose_logs_requests_but_never_tokens() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    let out = env.cmd().args(["ws", "list", "-v"]).output().unwrap();
    assert!(out.status.success());
    let err = text(&out.stderr);
    assert!(err.contains("> GET /api/workspaces") && err.contains("< 200"));
    assert_no_secrets(&out);
}

#[tokio::test(flavor = "multi_thread")]
async fn clap_usage_errors_are_exit_2_and_json_aware() {
    let env = Env::new().await;
    let out = env.cmd().args(["task", "show", "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v = stdout_json(&out);
    assert_valid("error", &v);
    assert_eq!(v["error"]["code"], "usage");
    let out = env.cmd().args(["task", "show"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("error:"));
    let out = env.cmd().arg("--help").output().unwrap();
    assert_eq!(out.status.code(), Some(0));
}

#[tokio::test(flavor = "multi_thread")]
async fn help_refs_documents_the_grammar() {
    let env = Env::new().await;
    let out = env.cmd().args(["help", "refs"]).output().unwrap();
    assert!(out.status.success());
    let s = text(&out.stdout);
    assert!(
        s.contains("^[A-Z][A-Z0-9]{0,9}-[0-9]{1,7}$") && s.contains("global task id"),
        "{s}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn http_servers_other_than_loopback_are_refused() {
    let env = Env::new().await;
    let out = env
        .cmd_for("http://example.com")
        .args(["whoami"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(text(&out.stderr).contains("plain http"));
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_processes_refresh_once_and_share_the_rotated_tokens() {
    let env = Env::new().await;
    env.login_seed();
    Mock::given(method("GET"))
        .and(path("/api/workspaces"))
        .and(header("authorization", format!("Bearer {ACCESS}").as_str()))
        .respond_with(ResponseTemplate::new(401))
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/auth/refresh"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(std::time::Duration::from_millis(600))
                .set_body_json(json!({"data": {
                    "token": "new-access-CCCC", "refresh_token": "new-refresh-DDDD", "expires_in": 3600}})),
        )
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/workspaces"))
        .and(header("authorization", "Bearer new-access-CCCC"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [
            {"id": 1, "name": "Personal", "code": "personal", "is_default": true}]})))
        .mount(&env.server)
        .await;
    let a = env.std_cmd().args(["ws", "list"]).spawn().unwrap();
    let b = env.std_cmd().args(["ws", "list"]).spawn().unwrap();
    let (oa, ob) = (a.wait_with_output().unwrap(), b.wait_with_output().unwrap());
    assert!(
        oa.status.success() && ob.status.success(),
        "{} {}",
        text(&oa.stderr),
        text(&ob.stderr)
    );
    assert_eq!(env.requests_to("/api/auth/refresh").await.len(), 1);
}
