mod common;
use common::*;
use serde_json::{json, Value};
use std::os::unix::fs::PermissionsExt;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

const MINTED: &str = "tmgrn_minted_SECRET0001";
const ENV_TOKEN: &str = "tmgrn_envtoken_SECRET0002";
const LEGACY_TOKEN: &str = "tmgrn_legacy_SECRET0003";

fn leaks(out: &std::process::Output) {
    assert_no_secrets(out);
    let all = format!("{}{}", text(&out.stdout), text(&out.stderr));
    for s in [MINTED, ENV_TOKEN, LEGACY_TOKEN] {
        assert!(!all.contains(s), "output leaked {s}");
    }
}

async fn mock_push(env: &Env, token: &str) {
    Mock::given(method("POST"))
        .and(path("/api/notifications/push"))
        .and(header("authorization", format!("Bearer {token}").as_str()))
        .respond_with(
            ResponseTemplate::new(202)
                .set_body_json(json!({"data": {"status": "sent", "channels": ["mobile"]}})),
        )
        .mount(&env.server)
        .await;
}

async fn mock_mint(env: &Env) {
    Mock::given(method("POST"))
        .and(path("/api/notify-tokens"))
        .and(header("authorization", format!("Bearer {ACCESS}").as_str()))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"data": {"id": 7, "label": "tmgr cli", "prefix": "tmgrn_mint", "token": MINTED}})))
        .mount(&env.server)
        .await;
}

fn write_legacy(env: &Env, mode: u32) {
    let dir = env.tmp.path().join(".config/tmgr-notify");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("env");
    std::fs::write(
        &file,
        format!(
            "TMGR_NOTIFY_TOKEN={LEGACY_TOKEN}\nTMGR_URL={}/api\n",
            env.server.uri()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&file, std::fs::Permissions::from_mode(mode)).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn notify_with_an_env_token_posts_the_payload_and_never_prints_it() {
    let env = Env::new().await;
    mock_push(&env, ENV_TOKEN).await;
    let out = env
        .cmd()
        .env("TMGR_NOTIFY_TOKEN", ENV_TOKEN)
        .env("TMGR_URL", env.server.uri())
        .args([
            "notify",
            "Build finished",
            "--title",
            "CI",
            "--priority",
            "high",
            "--link",
            "https://example.com/r/1",
            "--json",
            "-v",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("notify_result", &v);
    assert_eq!(v["data"], json!({"status": "sent", "channels": ["mobile"]}));
    let sent: Value =
        serde_json::from_slice(&env.requests_to("/api/notifications/push").await[0].body).unwrap();
    assert_eq!(
        sent,
        json!({"title": "CI", "body": "Build finished", "priority": "high", "link": "https://example.com/r/1", "source": "tmgr-cli"})
    );
    leaks(&out);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_legacy_env_file_works_only_when_private() {
    let env = Env::new().await;
    mock_push(&env, LEGACY_TOKEN).await;
    write_legacy(&env, 0o600);
    let out = env.cmd().args(["notify", "hi", "--json"]).output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_valid("notify_result", &stdout_json(&out));
    leaks(&out);

    let env = Env::new().await;
    mock_push(&env, LEGACY_TOKEN).await;
    write_legacy(&env, 0o644);
    let out = env.cmd().args(["notify", "hi", "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(4), "{}", text(&out.stdout));
    assert!(env.requests_to("/api/notifications/push").await.is_empty());
    leaks(&out);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_first_use_mints_a_token_and_later_uses_the_stored_one() {
    let env = Env::new().await;
    env.login_seed();
    mock_mint(&env).await;
    mock_push(&env, MINTED).await;
    for _ in 0..2 {
        let out = env
            .cmd()
            .args(["notify", "hello", "--json", "-v"])
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", text(&out.stderr));
        assert_valid("notify_result", &stdout_json(&out));
        leaks(&out);
    }
    assert_eq!(env.requests_to("/api/notify-tokens").await.len(), 1);
    assert_eq!(env.requests_to("/api/notifications/push").await.len(), 2);
    let creds: Value = serde_json::from_str(&env.creds_text()).unwrap();
    let key = format!("{}:notify", env.account());
    assert_eq!(creds[&key]["access_token"], MINTED);
    assert_eq!(creds[&key]["refresh_token"], "7");
    assert_eq!(creds[env.account().as_str()]["access_token"], ACCESS);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rejected_stored_token_is_replaced_once() {
    let env = Env::new().await;
    env.login_seed();
    mock_mint(&env).await;
    mock_push(&env, MINTED).await;
    let stale = "tmgrn_stale_SECRET0004";
    let creds = json!({
        env.account(): {"access_token": ACCESS, "refresh_token": REFRESH},
        format!("{}:notify", env.account()): {"access_token": stale, "refresh_token": "3"}
    });
    std::fs::create_dir_all(env.config_dir()).unwrap();
    std::fs::write(env.creds_path(), creds.to_string()).unwrap();
    std::fs::set_permissions(env.creds_path(), std::fs::Permissions::from_mode(0o600)).unwrap();
    Mock::given(method("POST"))
        .and(path("/api/notifications/push"))
        .and(header("authorization", format!("Bearer {stale}").as_str()))
        .respond_with(ResponseTemplate::new(401))
        .with_priority(1)
        .mount(&env.server)
        .await;
    let out = env
        .cmd()
        .args(["notify", "hello", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(env.requests_to("/api/notify-tokens").await.len(), 1);
    assert!(!text(&out.stdout).contains(stale));
}

#[tokio::test(flavor = "multi_thread")]
async fn minting_without_the_scope_is_exit_5_with_a_hint() {
    let env = Env::new().await;
    env.login_seed();
    Mock::given(method("POST"))
        .and(path("/api/notify-tokens"))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(json!({"error": "insufficient_scope"})),
        )
        .mount(&env.server)
        .await;
    let out = env
        .cmd()
        .args(["notify", "hello", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    let v = stdout_json(&out);
    assert!(v["error"]["hint"]
        .as_str()
        .unwrap()
        .ends_with(",notify:send"));
}

#[tokio::test(flavor = "multi_thread")]
async fn without_any_token_or_login_it_says_how_to_fix_it() {
    let env = Env::new().await;
    let out = env
        .cmd()
        .args(["notify", "hello", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(4));
    let v = stdout_json(&out);
    assert!(v["error"]["hint"]
        .as_str()
        .unwrap()
        .contains("TMGR_NOTIFY_TOKEN"));
}

#[tokio::test(flavor = "multi_thread")]
async fn logout_revokes_the_notify_token_and_forgets_it() {
    let env = Env::new().await;
    env.login_seed();
    mock_mint(&env).await;
    mock_push(&env, MINTED).await;
    Mock::given(method("DELETE"))
        .and(path("/api/notify-tokens/7"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&env.server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/auth/devices/current/logout"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&env.server)
        .await;
    assert!(env
        .cmd()
        .args(["notify", "hello"])
        .output()
        .unwrap()
        .status
        .success());
    let out = env.cmd().args(["logout", "--json"]).output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(env.requests_to("/api/notify-tokens/7").await.len(), 1);
    assert!(!env.creds_text().contains(MINTED));
    leaks(&out);
}

#[tokio::test(flavor = "multi_thread")]
async fn rate_limits_and_bad_input_have_clear_exit_codes() {
    let env = Env::new().await;
    Mock::given(method("POST"))
        .and(path("/api/notifications/push"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "30"))
        .mount(&env.server)
        .await;
    let out = env
        .cmd()
        .env("TMGR_NOTIFY_TOKEN", ENV_TOKEN)
        .env("TMGR_URL", env.server.uri())
        .args(["notify", "x", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout_json(&out)["error"]["message"]
        .as_str()
        .unwrap()
        .contains("30s"));
    let out = env
        .cmd()
        .args(["notify", "   ", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

async fn mock_alarm_create(env: &Env) {
    Mock::given(method("POST"))
        .and(path("/api/alarms"))
        .respond_with(
            ResponseTemplate::new(201)
                .set_body_json(json!({"data": {"id": 12, "status": "pending"}})),
        )
        .mount(&env.server)
        .await;
}

async fn mock_alarm_get(env: &Env, status: &str) {
    Mock::given(method("GET"))
        .and(path("/api/alarms/12"))
        .and(query_param("waitSeconds", "50"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {
            "id": 12, "status": status, "ackChannel": "app", "callStatus": null,
            "callAttempts": 3, "callAttemptsMade": 0, "createdAt": "2026-10-12T10:00:00Z"
        }})))
        .mount(&env.server)
        .await;
}

fn alarm_cmd(env: &Env) -> assert_cmd::Command {
    let mut c = env.cmd();
    c.env("TMGR_NOTIFY_TOKEN", ENV_TOKEN)
        .env("TMGR_URL", env.server.uri());
    c
}

#[tokio::test(flavor = "multi_thread")]
async fn an_acknowledged_alarm_exits_zero_and_sends_camel_case() {
    let env = Env::new().await;
    mock_alarm_create(&env).await;
    mock_alarm_get(&env, "acknowledged").await;
    let out = alarm_cmd(&env)
        .args([
            "alarm",
            "Prod is down",
            "--title",
            "DB",
            "--ack-timeout",
            "2m",
            "--call-attempts",
            "2",
            "--no-call",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("alarm", &v);
    assert_eq!(v["data"]["status"], "acknowledged");
    assert_eq!(v["data"]["ack_channel"], "app");
    assert_eq!(v["data"]["timed_out"], false);
    let sent: Value =
        serde_json::from_slice(&env.requests_to("/api/alarms").await[0].body).unwrap();
    assert_eq!(
        sent,
        json!({"title": "DB", "message": "Prod is down", "ackTimeoutSeconds": 120, "call": false, "callAttempts": 2})
    );
    leaks(&out);
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unanswered_alarm_exits_one_but_still_reports_it() {
    let env = Env::new().await;
    mock_alarm_create(&env).await;
    mock_alarm_get(&env, "no_answer").await;
    let out = alarm_cmd(&env)
        .args(["alarm", "Prod is down", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let v = stdout_json(&out);
    assert_valid("alarm", &v);
    assert_eq!(v["data"]["status"], "no_answer");
}

#[tokio::test(flavor = "multi_thread")]
async fn alarm_no_wait_and_status_use_the_last_alarm_id() {
    let env = Env::new().await;
    mock_alarm_create(&env).await;
    Mock::given(method("GET"))
        .and(path("/api/alarms/12"))
        .and(query_param("waitSeconds", "0"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {"id": 12, "status": "calling", "callStatus": "ringing", "callAttempts": 3, "callAttemptsMade": 1}})))
        .mount(&env.server)
        .await;
    let out = alarm_cmd(&env)
        .args(["alarm", "x", "--no-wait", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(stdout_json(&out)["data"]["status"], "pending");
    let out = alarm_cmd(&env)
        .args(["alarm", "status", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("alarm", &v);
    assert_eq!(v["data"]["call_status"], "ringing");
    let out = alarm_cmd(&env)
        .args(["alarm", "status", "1/../2", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn logging_in_again_drops_and_revokes_the_old_notify_token() {
    let env = Env::new().await;
    env.login_seed();
    mock_mint(&env).await;
    mock_push(&env, MINTED).await;
    Mock::given(method("DELETE"))
        .and(path("/api/notify-tokens/7"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&env.server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/auth/devices/current"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {"device_id": 9, "user": {"id": 2, "name": "Bob"}, "scopes": ["tasks:read"]}})))
        .mount(&env.server)
        .await;
    assert!(env
        .cmd()
        .args(["notify", "hello"])
        .output()
        .unwrap()
        .status
        .success());
    let out = env
        .cmd()
        .args(["login", "--with-token"])
        .write_stdin("new-token-EEEE5555\n")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(env.requests_to("/api/notify-tokens/7").await.len(), 1);
    assert!(!env.creds_text().contains(MINTED));
    leaks(&out);
}
