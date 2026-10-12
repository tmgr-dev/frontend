mod common;
use common::*;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use wiremock::matchers::{body_partial_json, method, path};
use wiremock::{Mock, ResponseTemplate};

async fn mock_device_code(env: &Env, interval: u64) {
    Mock::given(method("POST"))
        .and(path("/api/auth/device/code"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "device_code": "dev-code-SECRET",
            "user_code": "HJQK-7PXM",
            "verification_uri": "http://127.0.0.1:1/device",
            "verification_uri_complete": "http://127.0.0.1:1/device?code=HJQK-7PXM",
            "expires_in": 600,
            "interval": interval
        }})))
        .mount(&env.server)
        .await;
}

async fn mock_current(env: &Env, access: &str) {
    Mock::given(method("GET"))
        .and(path("/api/auth/devices/current"))
        .and(wiremock::matchers::header(
            "authorization",
            format!("Bearer {access}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "device_id": 9,
            "user": {"id": 1, "name": "Ann", "email": "ann@example.com"},
            "scopes": ["workspaces:read", "tasks:read"],
            "expires_at": "2026-11-10T00:00:00Z"
        }})))
        .mount(&env.server)
        .await;
}

fn grant(body: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(400).set_body_json(body)
}

#[tokio::test(flavor = "multi_thread")]
async fn device_login_polls_through_pending_and_slow_down() {
    let env = Env::new().await;
    mock_device_code(&env, 0).await;
    Mock::given(method("POST"))
        .and(path("/api/auth/device/token"))
        .and(body_partial_json(json!({"device_code": "dev-code-SECRET"})))
        .respond_with(grant(json!({"error": "authorization_pending"})))
        .up_to_n_times(1)
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/auth/device/token"))
        .respond_with(grant(json!({"error": "slow_down"})))
        .up_to_n_times(1)
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/auth/device/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "access_token": ACCESS, "refresh_token": REFRESH, "token_type": "Bearer",
            "expires_in": 3600, "scopes": ["workspaces:read", "tasks:read"], "device_id": 9
        }})))
        .mount(&env.server)
        .await;
    mock_current(&env, ACCESS).await;

    let out = env
        .cmd()
        .args(["login", "--no-browser", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("whoami", &v);
    assert_eq!(v["data"]["user"]["name"], "Ann");
    assert_eq!(v["data"]["credential_store"], "file");
    assert_no_secrets(&out);

    let stderr = text(&out.stderr);
    assert!(stderr.contains("HJQK-7PXM"), "{stderr}");
    assert!(stderr.contains("http://127.0.0.1:1/device?code=HJQK-7PXM"));

    let req = &env.requests_to("/api/auth/device/code").await[0];
    let body: serde_json::Value = serde_json::from_slice(&req.body).unwrap();
    assert_eq!(
        body["client_name"],
        format!("tmgr {}", env!("CARGO_PKG_VERSION"))
    );
    assert_eq!(body["host_label"], "tmgr cli");
    assert!(body["scopes"].as_array().unwrap().iter().all(|s| s != "*"));
    assert_eq!(body["scopes"][0], "workspaces:read");
    assert_eq!(
        req.headers.get("user-agent").unwrap().to_str().unwrap(),
        format!("tmgr/{}", env!("CARGO_PKG_VERSION"))
    );

    assert_eq!(env.requests_to("/api/auth/device/token").await.len(), 3);
    let mode = std::fs::metadata(env.creds_path())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o600);
    let dir_mode = std::fs::metadata(env.config_dir())
        .unwrap()
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(dir_mode, 0o700);
    assert!(env.creds_text().contains(ACCESS));
}

#[tokio::test(flavor = "multi_thread")]
async fn login_honours_label_and_scopes() {
    let env = Env::new().await;
    mock_device_code(&env, 0).await;
    Mock::given(method("POST"))
        .and(path("/api/auth/device/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "access_token": ACCESS, "refresh_token": REFRESH, "expires_in": 3600, "scopes": ["tasks:read"]
        }})))
        .mount(&env.server)
        .await;
    mock_current(&env, ACCESS).await;
    let out = env
        .cmd()
        .args([
            "login",
            "--no-browser",
            "--label",
            "spark box",
            "--scopes",
            "tasks:read",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let body: serde_json::Value =
        serde_json::from_slice(&env.requests_to("/api/auth/device/code").await[0].body).unwrap();
    assert_eq!(body["host_label"], "spark box");
    assert_eq!(body["scopes"], json!(["tasks:read"]));
    assert!(text(&out.stdout).contains("Logged in as Ann"));
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_scopes_are_rejected_before_any_request() {
    let env = Env::new().await;
    let out = env
        .cmd()
        .args(["login", "--no-browser", "--scopes", "*"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(env.server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn denied_login_exits_5_and_stores_nothing() {
    let env = Env::new().await;
    mock_device_code(&env, 0).await;
    Mock::given(method("POST"))
        .and(path("/api/auth/device/token"))
        .respond_with(grant(json!({"error": "access_denied"})))
        .mount(&env.server)
        .await;
    let out = env
        .cmd()
        .args(["login", "--no-browser", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    let v = stdout_json(&out);
    assert_valid("error", &v);
    assert_eq!(v["error"]["exit"], 5);
    assert!(!env.creds_path().exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn expired_login_exits_4() {
    let env = Env::new().await;
    mock_device_code(&env, 0).await;
    Mock::given(method("POST"))
        .and(path("/api/auth/device/token"))
        .respond_with(grant(json!({"error": "expired_token"})))
        .mount(&env.server)
        .await;
    let out = env.cmd().args(["login", "--no-browser"]).output().unwrap();
    assert_eq!(out.status.code(), Some(4));
    assert!(text(&out.stderr).contains("error: the login code expired"));
    assert!(text(&out.stderr).contains("hint: tmgr login"));
}

#[tokio::test(flavor = "multi_thread")]
async fn with_token_reads_stdin_validates_and_stores() {
    let env = Env::new().await;
    mock_current(&env, "tok-from-stdin-9999").await;
    let out = env
        .cmd()
        .args(["login", "--with-token"])
        .write_stdin("  tok-from-stdin-9999 \n")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(!text(&out.stdout).contains("tok-from-stdin-9999"));
    assert!(env.creds_text().contains("tok-from-stdin-9999"));
}

#[tokio::test(flavor = "multi_thread")]
async fn with_token_rejects_garbage_and_invalid_tokens() {
    let env = Env::new().await;
    let out = env
        .cmd()
        .args(["login", "--with-token"])
        .write_stdin("two words\n")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(env.server.received_requests().await.unwrap().is_empty());

    Mock::given(method("GET"))
        .and(path("/api/auth/devices/current"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&env.server)
        .await;
    let out = env
        .cmd()
        .args(["login", "--with-token"])
        .write_stdin("bad-token\n")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(4));
    assert!(!env.creds_path().exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn whoami_prints_user_scopes_and_never_a_token() {
    let env = Env::new().await;
    env.login_seed();
    mock_current(&env, ACCESS).await;
    let out = env.cmd().arg("whoami").output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let s = text(&out.stdout);
    assert!(s.contains("Ann (ann@example.com)"), "{s}");
    assert!(s.contains("workspaces:read, tasks:read"));
    assert!(s.contains("0600 file"));
    assert_no_secrets(&out);

    let out = env
        .cmd()
        .args(["auth", "status", "--json", "-v"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v = stdout_json(&out);
    assert_valid("whoami", &v);
    assert_no_secrets(&out);
}

#[tokio::test(flavor = "multi_thread")]
async fn whoami_without_login_is_exit_4() {
    let env = Env::new().await;
    let out = env.cmd().args(["whoami", "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(4));
    let v = stdout_json(&out);
    assert_valid("error", &v);
    assert_eq!(v["error"]["hint"], "tmgr login");
}

#[tokio::test(flavor = "multi_thread")]
async fn logout_revokes_remotely_and_deletes_local_credentials() {
    let env = Env::new().await;
    env.login_seed();
    Mock::given(method("POST"))
        .and(path("/api/auth/devices/current/logout"))
        .and(wiremock::matchers::header(
            "authorization",
            format!("Bearer {ACCESS}").as_str(),
        ))
        .respond_with(ResponseTemplate::new(204))
        .mount(&env.server)
        .await;
    let out = env.cmd().arg("logout").output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        env.requests_to("/api/auth/devices/current/logout")
            .await
            .len(),
        1
    );
    assert!(!env.creds_path().exists());
    assert_no_secrets(&out);
}

#[tokio::test(flavor = "multi_thread")]
async fn logout_survives_an_unreachable_server_with_a_warning() {
    let env = Env::new().await;
    let dead = dead_server();
    env.seed_account(
        &format!("{}:default", dead.trim_start_matches("http://")),
        ACCESS,
        Some(REFRESH),
    );
    let out = env.cmd_for(&dead).arg("logout").output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stderr).contains("warning:"));
}
