mod common;
use common::Env;

#[tokio::test(flavor = "multi_thread")]
async fn board_without_a_terminal_is_exit_2_with_a_hint_and_no_requests() {
    let env = Env::new().await;
    env.login_seed();
    let out = env.cmd().args(["board"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.starts_with("error: tmgr board needs an interactive terminal\nhint: tmgr task list"),
        "{stderr}"
    );
    assert!(out.stdout.is_empty());
    assert!(env.server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn board_with_json_is_a_usage_error() {
    let env = Env::new().await;
    env.login_seed();
    let out = env.cmd().args(["board", "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["error"]["code"], "usage");
    assert_eq!(v["error"]["hint"], "tmgr task list");
    assert!(env.server.received_requests().await.unwrap().is_empty());
}
