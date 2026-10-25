mod common;
use common::*;
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const TOKEN: &str = "tmgrn_hooktoken_SECRET0009";

fn hook_with_budget(env: &Env, args: &[&str], stdin: &[u8]) -> (std::process::Output, Duration) {
    run_hook(env, args, stdin, Some("5000"))
}

fn hook(env: &Env, args: &[&str], stdin: &[u8]) -> (std::process::Output, Duration) {
    run_hook(env, args, stdin, None)
}

fn run_hook(
    env: &Env,
    args: &[&str],
    stdin: &[u8],
    budget_ms: Option<&str>,
) -> (std::process::Output, Duration) {
    warm_up();
    let started = Instant::now();
    let mut cmd = env.cmd();
    cmd.env("PATH", path_with(None))
        .env("TMGR_HOOK_DEBUG", "1")
        .env("TMGR_NOTIFY_TOKEN", TOKEN)
        .env("TMGR_URL", env.server.uri());
    if let Some(ms) = budget_ms {
        cmd.env("TMGR_HOOK_BUDGET_MS", ms);
    }
    let out = cmd.args(args).write_stdin(stdin.to_vec()).output().unwrap();
    (out, started.elapsed())
}

fn warm_up() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let home = tempfile::tempdir().unwrap();
        let _ = std::process::Command::new(std::path::PathBuf::from(env!("CARGO_BIN_EXE_tmgr")))
            .env_clear()
            .env("HOME", home.path())
            .arg("version")
            .output();
    });
}

fn within_deadline(took: Duration) {
    assert!(took < Duration::from_secs(2), "took {took:?}");
}

fn quiet(out: &std::process::Output) {
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(
        out.stdout.is_empty(),
        "hooks never write stdout: {}",
        text(&out.stdout)
    );
    let all = format!("{}{}", text(&out.stdout), text(&out.stderr));
    assert!(!all.contains(TOKEN));
}

async fn mock_push(env: &Env) {
    Mock::given(method("POST"))
        .and(path("/api/notifications/push"))
        .respond_with(
            ResponseTemplate::new(202)
                .set_body_json(json!({"data": {"status": "sent", "channels": ["mobile"]}})),
        )
        .mount(&env.server)
        .await;
}

async fn pushes(env: &Env) -> Vec<Value> {
    env.requests_to("/api/notifications/push")
        .await
        .iter()
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_permission_prompt_pushes_a_high_priority_needs_you() {
    let env = Env::new().await;
    mock_push(&env).await;
    let payload = json!({"session_id": "s1", "cwd": "/work/api", "message": "Claude needs your permission to use Bash\nsecond line"});
    let (out, _) = hook(
        &env,
        &["hook", "claude", "notification"],
        payload.to_string().as_bytes(),
    );
    quiet(&out);
    let sent = pushes(&env).await;
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["title"], "Claude Code · api · needs you");
    assert_eq!(sent[0]["body"], "Claude needs your permission to use Bash");
    assert_eq!(sent[0]["priority"], "high");
    assert_eq!(sent[0]["source"], "claude-code:api");

    let quiet_type = json!({"cwd": "/work/api", "notification_type": "auth_success"});
    let (out, _) = hook(
        &env,
        &["hook", "claude", "notification"],
        quiet_type.to_string().as_bytes(),
    );
    quiet(&out);
    assert_eq!(pushes(&env).await.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_pushes_only_after_a_long_turn() {
    let env = Env::new().await;
    mock_push(&env).await;
    let prompt = json!({"session_id": "s2", "cwd": "/work/api"}).to_string();
    let stop = json!({"session_id": "s2", "cwd": "/work/api", "last_assistant_message": "All done.\nDetails"}).to_string();

    let (out, _) = hook(
        &env,
        &["hook", "claude", "user-prompt-submit"],
        prompt.as_bytes(),
    );
    quiet(&out);
    let (out, _) = hook(&env, &["hook", "claude", "stop"], stop.as_bytes());
    quiet(&out);
    assert!(pushes(&env).await.is_empty(), "a short turn stays silent");

    let (out, _) = hook(
        &env,
        &["hook", "claude", "user-prompt-submit"],
        prompt.as_bytes(),
    );
    quiet(&out);
    let out = env
        .cmd()
        .env("TMGR_NOTIFY_TOKEN", TOKEN)
        .env("TMGR_URL", env.server.uri())
        .env("TMGR_NOTIFY_STOP_MIN_MINUTES", "0")
        .args(["hook", "claude", "stop"])
        .write_stdin(stop.clone())
        .output()
        .unwrap();
    quiet(&out);
    let sent = pushes(&env).await;
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["title"], "Claude Code · api · done");
    assert_eq!(sent[0]["body"], "All done.");
    assert_eq!(sent[0]["priority"], "normal");

    let looping =
        json!({"session_id": "s2", "cwd": "/work/api", "stop_hook_active": true}).to_string();
    let (out, _) = hook(&env, &["hook", "claude", "stop"], looping.as_bytes());
    quiet(&out);
    assert_eq!(pushes(&env).await.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_transcript_gives_the_turn_start_when_no_prompt_hook_ran() {
    let env = Env::new().await;
    mock_push(&env).await;
    let transcript = env.tmp.path().join("t.jsonl");
    std::fs::write(
        &transcript,
        "{\"type\":\"user\",\"timestamp\":\"2020-01-01T00:00:00Z\",\"message\":{\"role\":\"user\",\"content\":\"go\"}}\n",
    )
    .unwrap();
    let stop = json!({"session_id": "s3", "cwd": "/work/api", "transcript_path": transcript.display().to_string()}).to_string();
    let (out, _) = hook(&env, &["hook", "claude", "stop"], stop.as_bytes());
    quiet(&out);
    assert_eq!(
        pushes(&env).await.len(),
        0,
        "the transcript is not read before Claude reading is allowed"
    );
    assert!(!env.tmp.path().join(".local/share/tmgr/usage").exists());
    let allow = env
        .cmd()
        .args(["usage", "consent", "allow"])
        .output()
        .unwrap();
    assert!(allow.status.success(), "{}", text(&allow.stderr));
    let (out, _) = hook(&env, &["hook", "claude", "stop"], stop.as_bytes());
    quiet(&out);
    assert_eq!(pushes(&env).await.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn codex_reads_the_event_from_the_last_argument() {
    let env = Env::new().await;
    mock_push(&env).await;
    let event = json!({"type": "agent-turn-complete", "cwd": "/work/app", "last-assistant-message": "Fixed it."}).to_string();
    let (out, _) = hook(&env, &["hook", "codex", "notify", &event], b"");
    quiet(&out);
    let sent = pushes(&env).await;
    assert_eq!(sent[0]["title"], "Codex · app");
    assert_eq!(sent[0]["body"], "Fixed it.");
    let other = json!({"type": "something-else"}).to_string();
    let (out, _) = hook(&env, &["hook", "codex", "notify", &other], b"");
    quiet(&out);
    assert_eq!(pushes(&env).await.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn garbage_oversized_or_missing_input_still_exits_zero_quietly() {
    let env = Env::new().await;
    mock_push(&env).await;
    let big = vec![b'a'; 600 * 1024];
    let mut big_json = br#"{"message":""#.to_vec();
    big_json.extend_from_slice(&big);
    big_json.extend_from_slice(br#""}"#);
    let cases: Vec<(&[&str], Vec<u8>)> = vec![
        (&["hook", "claude", "notification"], b"{not json".to_vec()),
        (&["hook", "claude", "notification"], b"[1,2,3]".to_vec()),
        (&["hook", "claude", "notification"], b"".to_vec()),
        (&["hook", "claude", "notification"], vec![0xff, 0xfe, 0x00]),
        (&["hook", "claude", "notification"], big_json),
        (&["hook", "claude", "stop"], big),
        (&["hook", "claude", "no-such-event"], b"{}".to_vec()),
        (&["hook", "claude"], b"{}".to_vec()),
        (&["hook", "claude", "stop", "--bogus-flag"], b"{}".to_vec()),
        (&["hook", "codex", "notify", "{not json"], b"".to_vec()),
        (&["hook", "codex"], b"".to_vec()),
    ];
    for (args, stdin) in cases {
        let (out, took) = hook(&env, args, &stdin);
        quiet(&out);
        within_deadline(took);
    }
    assert!(pushes(&env).await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_broken_config_or_no_token_does_not_fail_the_hook() {
    let env = Env::new().await;
    std::fs::create_dir_all(env.config_dir()).unwrap();
    std::fs::write(env.config_dir().join("config.toml"), "server = [").unwrap();
    let payload = json!({"cwd": "/work/api", "message": "m"}).to_string();
    let (out, _) = hook(
        &env,
        &["hook", "claude", "notification"],
        payload.as_bytes(),
    );
    quiet(&out);

    let env = Env::new().await;
    let out = env
        .cmd()
        .args(["hook", "claude", "notification"])
        .write_stdin(payload)
        .output()
        .unwrap();
    quiet(&out);
    assert!(env.server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_slow_server_cannot_hold_the_agent_for_more_than_two_seconds() {
    let env = Env::new().await;
    Mock::given(method("POST"))
        .and(path("/api/notifications/push"))
        .respond_with(ResponseTemplate::new(202).set_delay(Duration::from_secs(8)))
        .mount(&env.server)
        .await;
    let payload = json!({"cwd": "/work/api", "message": "m"}).to_string();
    let (out, took) = hook(
        &env,
        &["hook", "claude", "notification"],
        payload.as_bytes(),
    );
    quiet(&out);
    within_deadline(took);
    assert!(
        took >= Duration::from_millis(1000),
        "gave up too early: {took:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_stdin_that_never_closes_is_cut_off_at_the_deadline() {
    use std::io::Write;
    warm_up();
    let env = Env::new().await;
    let mut cmd = env.std_cmd();
    let started = Instant::now();
    let mut child = cmd
        .env("TMGR_NOTIFY_TOKEN", TOKEN)
        .args(["hook", "claude", "stop"])
        .stdin(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"{\"cwd\":").unwrap();
    let status = child.wait().unwrap();
    assert_eq!(status.code(), Some(0));
    within_deadline(started.elapsed());
    drop(stdin);
}

async fn seed_run(
    env: &Env,
    repo: &std::path::Path,
    branch: &str,
    start_sha: &str,
    last_sent: i64,
) {
    let runs = json!({"schema": "tmgr.runs.v1", "runs": [{
        "run_id": 41, "task_id": 8974, "task_key": "TM-512", "branch": branch,
        "repo": repo.display().to_string(), "start_sha": start_sha, "agent": "claude-code",
        "started_at": "2026-10-12T09:00:00Z", "commits": ["abc1234 earlier"], "last_sent": last_sent
    }]});
    std::fs::create_dir_all(env.state_dir()).unwrap();
    std::fs::write(env.state_dir().join("runs.json"), runs.to_string()).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn stop_adds_the_new_commits_to_the_active_run_and_heartbeats() {
    let env = Env::new().await;
    env.login_seed();
    mock_push(&env).await;
    Mock::given(method("PATCH"))
        .and(path("/api/agent-work/41"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"id": 41, "status": "running"})),
        )
        .mount(&env.server)
        .await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    git(&repo, &["checkout", "-q", "-b", "tm-512-x"]);
    let start = git(&repo, &["rev-parse", "HEAD"]);
    std::fs::write(repo.join("a.txt"), "a").unwrap();
    git(&repo, &["add", "a.txt"]);
    git(&repo, &["commit", "-q", "-m", "feat: first change"]);
    std::fs::write(repo.join("b.txt"), "b").unwrap();
    git(&repo, &["add", "b.txt"]);
    git(
        &repo,
        &["commit", "-q", "-m", "fix: second change; $(touch pwned)"],
    );
    seed_run(&env, &repo, "tm-512-x", &start, 0).await;

    let stop = json!({"session_id": "s9", "cwd": repo.display().to_string()}).to_string();
    let (out, _) = hook_with_budget(&env, &["hook", "claude", "stop"], stop.as_bytes());
    quiet(&out);

    let patches = env.requests_to("/api/agent-work/41").await;
    assert_eq!(patches.len(), 1, "hook stderr: {}", text(&out.stderr));
    let body: Value = serde_json::from_slice(&patches[0].body).unwrap();
    let commits = body["commits"].as_array().unwrap();
    assert_eq!(commits.len(), 3);
    assert_eq!(commits[0]["sha"], "abc1234");
    assert_eq!(commits[1]["message"], "feat: first change");
    assert_eq!(commits[2]["message"], "fix: second change; $(touch pwned)");
    assert!(!repo.join("pwned").exists());
    let state: Value =
        serde_json::from_str(&std::fs::read_to_string(env.state_dir().join("runs.json")).unwrap())
            .unwrap();
    assert_eq!(state["runs"][0]["commits"].as_array().unwrap().len(), 3);
    assert!(state["runs"][0]["last_sent"].as_i64().unwrap() > 1_700_000_000);
}

#[tokio::test(flavor = "multi_thread")]
async fn heartbeats_are_throttled_and_a_finished_run_is_forgotten() {
    let env = Env::new().await;
    env.login_seed();
    Mock::given(method("PATCH"))
        .and(path("/api/agent-work/41"))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({"message": "finished"})))
        .mount(&env.server)
        .await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    git(&repo, &["checkout", "-q", "-b", "tm-512-x"]);
    let start = git(&repo, &["rev-parse", "HEAD"]);
    let payload = json!({"cwd": repo.display().to_string()}).to_string();

    seed_run(&env, &repo, "tm-512-x", &start, now_s()).await;
    let (out, _) = hook_with_budget(
        &env,
        &["hook", "claude", "pre-tool-use"],
        payload.as_bytes(),
    );
    quiet(&out);
    assert!(
        env.requests_to("/api/agent-work/41").await.is_empty(),
        "recent heartbeat"
    );

    seed_run(&env, &repo, "tm-512-x", &start, now_s() - 1000).await;
    let (out, _) = hook_with_budget(
        &env,
        &["hook", "claude", "pre-tool-use"],
        payload.as_bytes(),
    );
    quiet(&out);
    assert_eq!(
        env.requests_to("/api/agent-work/41").await.len(),
        1,
        "hook stderr: {}",
        text(&out.stderr)
    );
    let state: Value =
        serde_json::from_str(&std::fs::read_to_string(env.state_dir().join("runs.json")).unwrap())
            .unwrap();
    assert!(
        state["runs"].as_array().unwrap().is_empty(),
        "409 means the run is over"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn an_expired_login_is_skipped_without_refreshing() {
    let env = Env::new().await;
    env.login_seed();
    Mock::given(method("PATCH"))
        .and(path("/api/agent-work/41"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&env.server)
        .await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    git(&repo, &["checkout", "-q", "-b", "tm-512-x"]);
    let start = git(&repo, &["rev-parse", "HEAD"]);
    seed_run(&env, &repo, "tm-512-x", &start, 0).await;
    let payload = json!({"cwd": repo.display().to_string()}).to_string();
    let (out, _) = hook(&env, &["hook", "claude", "stop"], payload.as_bytes());
    quiet(&out);
    assert!(env.requests_to("/api/auth/refresh").await.is_empty());
    let state: Value =
        serde_json::from_str(&std::fs::read_to_string(env.state_dir().join("runs.json")).unwrap())
            .unwrap();
    assert_eq!(state["runs"].as_array().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn help_is_still_printed_for_the_hook_commands() {
    let env = Env::new().await;
    let out = env
        .cmd()
        .args(["hook", "claude", "--help"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(text(&out.stdout).contains("Claude Code hook entry point"));
}

const T0: i64 = 1_800_000_000;

fn iso(at: i64) -> String {
    chrono::DateTime::from_timestamp(at, 0)
        .unwrap()
        .to_rfc3339()
}

fn mcp_run_response(body: Value) -> Value {
    json!({"content": [{"type": "text", "text": body.to_string()}]})
}

fn tool_run(status: &str) -> Value {
    mcp_run_response(json!({
        "id": 77, "task_id": 8974, "status": status,
        "started_at": iso(T0), "ended_at": iso(T0 + 1000)
    }))
}

async fn mock_run_endpoints(env: &Env) {
    for (verb, p) in [
        ("PATCH", "/api/agent-work/77"),
        ("POST", "/api/agent-work/77/commits"),
    ] {
        Mock::given(method(verb))
            .and(path(p))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"id": 77, "status": "running"})),
            )
            .mount(&env.server)
            .await;
    }
}

async fn mock_server_run(env: &Env, status: &str, started: i64, extra: Value) {
    let mut run = json!({
        "id": 77, "task_id": 8974, "status": status,
        "started_at": iso(started), "ended_at": iso(T0 + 1000), "commits": []
    });
    for (k, v) in extra.as_object().cloned().unwrap_or_default() {
        run[k] = v;
    }
    Mock::given(method("GET"))
        .and(path("/api/tasks/8974/agent-work"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"runs": [run]})))
        .mount(&env.server)
        .await;
}

fn post_tool_use(tool: &str, repo: &std::path::Path, response: Value, extra: Value) -> Vec<u8> {
    let mut v = json!({
        "session_id": "5f0c-sess", "cwd": repo.display().to_string(),
        "hook_event_name": "PostToolUse", "tool_name": tool,
        "tool_input": {}, "tool_response": response
    });
    for (k, val) in extra.as_object().cloned().unwrap_or_default() {
        v[k] = val;
    }
    v.to_string().into_bytes()
}

fn finish_input(repo: &std::path::Path, response: Value) -> Vec<u8> {
    post_tool_use("mcp__tmgr__finish_agent_work", repo, response, json!({}))
}

async fn writes(env: &Env) -> usize {
    env.requests_to("/api/agent-work/77").await.len()
        + env.requests_to("/api/agent-work/77/commits").await.len()
}

#[tokio::test(flavor = "multi_thread")]
async fn starting_a_run_in_the_main_session_binds_the_session_id() {
    let env = Env::new().await;
    env.login_seed();
    mock_run_endpoints(&env).await;
    mock_server_run(&env, "running", T0, json!({})).await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let input = post_tool_use(
        "mcp__tmgr__start_agent_work",
        &repo,
        tool_run("running"),
        json!({}),
    );
    let (out, _) = hook_with_budget(&env, &["hook", "claude", "post-tool-use"], &input);
    quiet(&out);
    let patches = env.requests_to("/api/agent-work/77").await;
    assert_eq!(patches.len(), 1, "stderr: {}", text(&out.stderr));
    let body: Value = serde_json::from_slice(&patches[0].body).unwrap();
    assert_eq!(body, json!({"session_id": "5f0c-sess"}));
}

#[tokio::test(flavor = "multi_thread")]
async fn starting_a_run_in_a_subagent_binds_the_node_and_its_parent() {
    let env = Env::new().await;
    env.login_seed();
    mock_run_endpoints(&env).await;
    mock_server_run(&env, "running", T0 + 1, json!({})).await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let input = post_tool_use(
        "mcp__plugin_tmgr_tmgr__start_agent_work",
        &repo,
        tool_run("running"),
        json!({"agent_id": "ag-12", "agent_type": "worker"}),
    );
    let (out, _) = hook_with_budget(&env, &["hook", "claude", "post-tool-use"], &input);
    quiet(&out);
    let patches = env.requests_to("/api/agent-work/77").await;
    let body: Value = serde_json::from_slice(&patches[0].body).unwrap();
    assert_eq!(
        body,
        json!({"session_id": "5f0c-sess/ag-12", "parent_session_id": "5f0c-sess"})
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_conflict_unparseable_responses_and_other_tools_change_nothing() {
    let env = Env::new().await;
    env.login_seed();
    mock_server_run(&env, "running", T0, json!({})).await;
    Mock::given(method("PATCH"))
        .and(path("/api/agent-work/77"))
        .respond_with(ResponseTemplate::new(409).set_body_json(json!({"error": "conflict"})))
        .mount(&env.server)
        .await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let input = post_tool_use(
        "mcp__tmgr__start_agent_work",
        &repo,
        tool_run("running"),
        json!({}),
    );
    let (out, _) = hook_with_budget(&env, &["hook", "claude", "post-tool-use"], &input);
    quiet(&out);
    let cases = [
        (
            "mcp__tmgr__start_agent_work",
            json!("Agent work is disabled"),
        ),
        ("mcp__tmgr__start_agent_work", json!({"weird": [1, 2]})),
        ("mcp__tmgr__update_agent_work", tool_run("running")),
        ("Bash", json!({"stdout": "{\"id\": 77}"})),
    ];
    for (tool, resp) in cases {
        let input = post_tool_use(tool, &repo, resp, json!({}));
        let (out, _) = hook_with_budget(&env, &["hook", "claude", "post-tool-use"], &input);
        quiet(&out);
    }
    assert_eq!(env.requests_to("/api/agent-work/77").await.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_run_the_server_does_not_confirm_is_never_written_to() {
    let env = Env::new().await;
    env.login_seed();
    mock_run_endpoints(&env).await;
    mock_server_run(&env, "running", T0, json!({})).await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    git(&repo, &["config", "user.email", "me@example.com"]);
    commit_at(&repo, "mine", T0 + 100, "me@example.com");
    let foreign_task = mcp_run_response(json!({
        "id": 77, "task_id": 1234, "status": "running", "started_at": iso(T0)
    }));
    let other_start = mcp_run_response(json!({
        "id": 77, "task_id": 8974, "status": "running", "started_at": iso(T0 + 60)
    }));
    let no_start = mcp_run_response(json!({"id": 77, "task_id": 8974}));
    for resp in [foreign_task, other_start, no_start] {
        let input = post_tool_use(
            "mcp__tmgr__start_agent_work",
            &repo,
            resp.clone(),
            json!({}),
        );
        let (out, _) = hook_with_budget(&env, &["hook", "claude", "post-tool-use"], &input);
        quiet(&out);
        let (out, _) = hook_with_budget(
            &env,
            &["hook", "claude", "post-tool-use"],
            &finish_input(&repo, resp),
        );
        quiet(&out);
    }
    let finishing_a_running_run = finish_input(&repo, tool_run("succeeded"));
    let (out, _) = hook_with_budget(
        &env,
        &["hook", "claude", "post-tool-use"],
        &finishing_a_running_run,
    );
    quiet(&out);
    assert_eq!(writes(&env).await, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_start_for_a_run_that_already_finished_on_the_server_is_not_bound() {
    let env = Env::new().await;
    env.login_seed();
    mock_run_endpoints(&env).await;
    mock_server_run(&env, "succeeded", T0, json!({})).await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let input = post_tool_use(
        "mcp__tmgr__start_agent_work",
        &repo,
        tool_run("running"),
        json!({}),
    );
    let (out, _) = hook_with_budget(&env, &["hook", "claude", "post-tool-use"], &input);
    quiet(&out);
    assert_eq!(writes(&env).await, 0);
}

async fn commit_fixture(env: &Env) -> (std::path::PathBuf, String, String, String) {
    let repo = init_repo(&env.tmp.path().join("repo"));
    git(&repo, &["config", "user.email", "me@example.com"]);
    git(
        &repo,
        &["remote", "add", "origin", "git@github.com:acme/tool.git"],
    );
    commit_at(&repo, "before", T0 - 3600, "me@example.com");
    let reported = commit_at(&repo, "reported", T0 + 50, "me@example.com");
    let main_one = commit_at(&repo, "mine-main", T0 + 100, "me@example.com");
    commit_at(&repo, "someone-else", T0 + 120, "you@example.com");
    git(&repo, &["checkout", "-q", "-b", "side"]);
    let side_one = commit_at(&repo, "mine-side", T0 + 200, "me@example.com");
    git(&repo, &["checkout", "-q", "main"]);
    commit_at(&repo, "after", T0 + 5000, "me@example.com");
    (repo, reported, main_one, side_one)
}

#[tokio::test(flavor = "multi_thread")]
async fn finishing_a_run_posts_my_commits_from_the_servers_run_branch() {
    let env = Env::new().await;
    env.login_seed();
    mock_run_endpoints(&env).await;
    let (repo, reported, main_one, side_one) = commit_fixture(&env).await;
    mock_server_run(
        &env,
        "succeeded",
        T0,
        json!({"branch": "side", "commits": [{"sha": &reported[..7], "message": "reported"}]}),
    )
    .await;
    let lying = mcp_run_response(json!({
        "id": 77, "task_id": 8974, "status": "succeeded",
        "started_at": iso(T0), "ended_at": iso(T0 - 100000)
    }));
    let (out, _) = hook_with_budget(
        &env,
        &["hook", "claude", "post-tool-use"],
        &finish_input(&repo, lying),
    );
    quiet(&out);
    let posts = env.requests_to("/api/agent-work/77/commits").await;
    assert_eq!(posts.len(), 1, "stderr: {}", text(&out.stderr));
    let body: Value = serde_json::from_slice(&posts[0].body).unwrap();
    let got: Vec<&str> = body["commits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["sha"].as_str().unwrap())
        .collect();
    assert_eq!(got, [main_one.as_str(), side_one.as_str()]);
    assert_eq!(body["commits"][0]["repo"], "acme/tool");
    assert_eq!(body["commits"][0]["branch"], "side");
    assert!(env.requests_to("/api/agent-work/77").await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn the_git_commits_opt_out_stops_the_hook_from_posting() {
    let env = Env::new().await;
    env.login_seed();
    mock_run_endpoints(&env).await;
    let (repo, ..) = commit_fixture(&env).await;
    mock_server_run(&env, "succeeded", T0, json!({"branch": "side"})).await;
    let cfg = env
        .cmd()
        .args(["config", "set", "hook_git_commits", "false"])
        .output()
        .unwrap();
    assert!(cfg.status.success(), "{}", text(&cfg.stderr));
    let (out, _) = hook_with_budget(
        &env,
        &["hook", "claude", "post-tool-use"],
        &finish_input(&repo, tool_run("succeeded")),
    );
    quiet(&out);
    assert_eq!(writes(&env).await, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn finishing_with_no_new_commits_or_no_times_posts_nothing() {
    let env = Env::new().await;
    env.login_seed();
    mock_run_endpoints(&env).await;
    mock_server_run(&env, "succeeded", T0, json!({})).await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    git(&repo, &["config", "user.email", "me@example.com"]);
    let no_times = mcp_run_response(json!({"id": 77, "task_id": 8974}));
    let (out, _) = hook_with_budget(
        &env,
        &["hook", "claude", "post-tool-use"],
        &finish_input(&repo, no_times),
    );
    quiet(&out);
    let (out, _) = hook_with_budget(
        &env,
        &["hook", "claude", "post-tool-use"],
        &finish_input(&repo, tool_run("succeeded")),
    );
    quiet(&out);
    assert_eq!(writes(&env).await, 0);
}
