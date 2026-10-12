mod common;
use common::*;
use serde_json::{json, Value};

fn run(id: u64, task_id: u64, key: &str, start: &str, end: Option<&str>, secs: i64) -> Value {
    json!({
        "id": id, "task_id": task_id, "workspace_id": 1, "user_id": 3, "agent": "codex",
        "model": "gpt-5", "branch": "b", "status": if end.is_some() { "succeeded" } else { "running" },
        "started_at": start, "ended_at": end, "duration_seconds": secs, "commits": [], "version": 1,
        "task": {"id": task_id, "key": key, "title": format!("{key}: Agent task")}
    })
}

fn daily(items: Value, total: i64) -> Value {
    json!({"data": {"from": "2026-10-12", "to": "2026-10-12", "tz": "UTC", "days": [
        {"date": "2026-10-12", "total_s": total, "items": items}
    ]}})
}

async fn fixtures(env: &Env) {
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_statuses().await;
    env.mock_json(
        "/api/workspaces/1/time/daily",
        daily(
            json!([{
                "task": {"id": 8974, "key": "TM-512", "title": "TM-512: Fix login\u{1b}[31m loop", "url": "https://tmgr.dev/tmgrdev/tasks/8974"},
                "status": {"id": 11, "name": "In progress\u{1b}[31m", "type": "active"},
                "category": {"id": 2, "code": "TM", "name": "Tmgr\u{1b}]0;pwn\u{7}"},
                "seconds": 7200,
                "users": [{"id": 3, "name": "Ann\u{1b}[31m Lee", "seconds": 7200}]
            }]),
            7200,
        ),
    )
    .await;
    env.mock_json(
        "/api/workspaces/1/agent-work",
        json!({"data": [
            run(1, 8974, "TM-512", "2026-10-12T09:00:00Z", Some("2026-10-12T10:00:00Z"), 3600),
            run(2, 9000, "TM-518", "2026-10-11T23:30:00Z", Some("2026-10-12T00:30:00Z"), 3600),
            run(3, 9001, "TM-519", "2026-10-10T09:00:00Z", Some("2026-10-10T10:00:00Z"), 3600)
        ]}),
    )
    .await;
    env.mock_json(
        "/api/daily-routines/expand",
        json!([
            {"task_id": 31, "title": "Morning plan", "date": "2026-10-12", "time": "09:00", "status": "completed", "completed": true, "instance_id": 501, "virtual": false},
            {"task_id": 32, "title": "Standup\u{1b}[31m notes", "date": "2026-10-12", "time": "17:30", "status": "pending", "completed": false, "instance_id": null, "virtual": true}
        ]),
    )
    .await;
}

fn today_cmd(env: &Env) -> assert_cmd::Command {
    let mut c = env.cmd();
    c.env("TZ", "UTC");
    c
}

#[tokio::test(flavor = "multi_thread")]
async fn today_json_combines_time_agents_and_routines() {
    let env = Env::new().await;
    fixtures(&env).await;
    let out = today_cmd(&env)
        .args(["today", "--date", "2026-10-12", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("today", &v);
    let d = &v["data"];
    assert_eq!(d["date"], "2026-10-12");
    assert_eq!(d["totals"]["human_s"], 7200);
    assert_eq!(d["totals"]["agent_s"], 5400);
    let tasks = d["tasks"].as_array().unwrap();
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0]["ref"], "TM-512");
    assert_eq!(tasks[0]["human_s"], 7200);
    assert_eq!(tasks[0]["agent_s"], 3600);
    assert_eq!(tasks[0]["status"], "In progress [31m");
    assert_eq!(tasks[0]["project"], "Tmgr ]0;pwn ");
    assert_eq!(tasks[0]["url"], "https://tmgr.dev/tmgrdev/tasks/8974");
    assert_eq!(tasks[0]["users"][0]["name"], "Ann [31m Lee");
    assert_eq!(tasks[0]["users"][0]["human_s"], 7200);
    assert_eq!(d["tz"], "UTC");
    assert_eq!(tasks[1]["ref"], "TM-518");
    assert_eq!(tasks[1]["agent_s"], 1800);
    assert_eq!(tasks[1]["human_s"], 0);
    assert_eq!(d["routines"].as_array().unwrap().len(), 2);
    assert_eq!(d["routines"][0]["title"], "Morning plan");
    assert_eq!(d["routines"][0]["done"], true);
    assert_eq!(d["routines"][1]["done"], false);
    let q = env.requests_to("/api/workspaces/1/time/daily").await[0]
        .url
        .query()
        .unwrap()
        .to_string();
    assert!(
        q.contains("from=2026-10-12")
            && q.contains("to=2026-10-12")
            && q.contains("tz=UTC")
            && q.contains("mine=true"),
        "{q}"
    );
    assert!(env
        .requests_to("/api/workspaces/1/time-summary")
        .await
        .is_empty());
    assert!(env.requests_to("/api/tasks/8974").await.is_empty());
    let q = env.requests_to("/api/workspaces/1/agent-work").await[0]
        .url
        .query()
        .unwrap()
        .to_string();
    assert!(
        q.contains("since=2026-10-11T00%3A00%3A00Z")
            && q.contains("until=2026-10-13T00%3A00%3A00Z"),
        "{q}"
    );
    assert_no_secrets(&out);
}

#[tokio::test(flavor = "multi_thread")]
async fn today_human_output_is_a_clean_table_without_escape_sequences() {
    let env = Env::new().await;
    fixtures(&env).await;
    let out = today_cmd(&env)
        .args(["today", "--date", "2026-10-12"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let s = text(&out.stdout);
    assert!(!s.contains('\u{1b}'), "{s}");
    assert!(s.contains("Mon 12 Oct"), "{s}");
    assert!(s.contains("tracked 3h 30m"), "{s}");
    assert!(s.contains("you 2h 00m"), "{s}");
    assert!(
        s.contains("agents 30m") || s.contains("agents 1h 30m"),
        "{s}"
    );
    assert!(s.contains("TM-512 Fix login [31m loop"), "{s}");
    assert!(
        s.contains("Tmgr ]0;pwn") && s.contains("In progress [31m"),
        "{s}"
    );
    assert!(
        s.contains("YOU") && s.contains("AGENTS") && s.contains("STATUS"),
        "{s}"
    );
    assert!(s.contains("time split"), "{s}");
    assert!(s.contains("Routines"), "{s}");
    assert!(s.contains("\u{2713} Morning plan"), "{s}");
    assert!(s.contains("\u{25cb} Standup [31m notes"), "{s}");
}

#[tokio::test(flavor = "multi_thread")]
async fn today_mine_keeps_only_my_agent_runs() {
    let env = Env::new().await;
    fixtures(&env).await;
    env.mock_json(
        "/api/auth/devices/current",
        json!({"data": {"device_id": 1, "user": {"id": 99, "name": "Me"}, "scopes": []}}),
    )
    .await;
    let out = today_cmd(&env)
        .args(["today", "--date", "2026-10-12", "--mine", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_eq!(v["data"]["totals"]["agent_s"], 0);
    assert!(v["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .all(|w| w["code"] != "human_time_workspace_wide"));
}

#[tokio::test(flavor = "multi_thread")]
async fn today_falls_back_to_the_time_summary_on_an_older_server() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_json(
        "/api/workspaces/1/time-summary",
        json!({"data": {"total_s": 900, "items": [
            {"task_id": 8974, "key": "TM-512", "title": "TM-512: Fix login", "seconds": 900}
        ]}}),
    )
    .await;
    env.mock_json("/api/daily-routines/expand", json!([])).await;
    env.mock_json("/api/workspaces/1/agent-work", json!({"data": []}))
        .await;
    let out = today_cmd(&env)
        .args(["today", "--date", "2026-10-12", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("today", &v);
    assert_eq!(v["data"]["totals"]["human_s"], 900);
    assert_eq!(v["data"]["tasks"][0]["ref"], "TM-512");
    assert_eq!(v["warnings"][0]["code"], "daily_time_unavailable");
}

#[tokio::test(flavor = "multi_thread")]
async fn today_with_nothing_prints_an_empty_state_and_survives_missing_modules() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_json("/api/workspaces/1/time/daily", daily(json!([]), 0))
        .await;
    env.mock_json("/api/daily-routines/expand", json!([])).await;
    wiremock::Mock::given(wiremock::matchers::method("GET"))
        .and(wiremock::matchers::path("/api/workspaces/1/agent-work"))
        .respond_with(wiremock::ResponseTemplate::new(403).set_body_json(json!({
            "error": "feature_disabled", "feature": "agent_work", "message": "off"
        })))
        .mount(&env.server)
        .await;
    let out = today_cmd(&env).args(["today", "--json"]).output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("today", &v);
    assert_eq!(v["data"]["tasks"].as_array().unwrap().len(), 0);
    assert_eq!(v["warnings"][0]["code"], "agent_work_disabled");
    let human = today_cmd(&env).args(["today"]).output().unwrap();
    assert!(text(&human.stdout).contains("Nothing tracked yet"));
}

#[tokio::test(flavor = "multi_thread")]
async fn today_rejects_bad_dates() {
    let env = Env::new().await;
    env.login_seed();
    for args in [
        vec!["today", "--date", "yesterday"],
        vec!["today", "--date", "2026-13-01"],
        vec!["today", "--date", "2026-10-1"],
        vec!["today", "--date", "2026-10-12", "--yesterday"],
    ] {
        let out = today_cmd(&env).args(&args).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
    assert!(env.server.received_requests().await.unwrap().is_empty());
}

fn local_today() -> String {
    chrono::Utc::now().format("%Y-%m-%d").to_string()
}

async fn routine_fixture(env: &Env, date: &str) {
    env.login_seed();
    env.mock_json(
        "/api/daily-routines/expand",
        json!([
            {"task_id": 31, "title": "Morning plan", "date": date, "time": "09:00", "status": "completed", "completed": true, "instance_id": 501},
            {"task_id": 32, "title": "Standup", "date": date, "time": "17:30", "status": "pending", "completed": false, "instance_id": 502},
            {"task_id": 33, "title": "Weekly\u{1b}[31m review", "date": date, "time": null, "status": "pending", "completed": false, "instance_id": null}
        ]),
    )
    .await;
    env.mock_any(
        "POST",
        "/api/daily-routines/tasks/32/instances/502/complete",
        json!({"id": 502, "status": "completed"}),
    )
    .await;
    env.mock_any(
        "POST",
        "/api/daily-routines/tasks/33/complete",
        json!({"id": 600, "status": "completed"}),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn routine_list_json_and_human() {
    let env = Env::new().await;
    routine_fixture(&env, "2026-10-12").await;
    let out = today_cmd(&env)
        .args(["routine", "list", "--date", "2026-10-12", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("routine_list", &v);
    assert_eq!(v["data"]["routines"].as_array().unwrap().len(), 3);
    let human = today_cmd(&env)
        .args(["routine", "list", "--date", "2026-10-12"])
        .output()
        .unwrap();
    let s = text(&human.stdout);
    assert!(
        !s.contains('\u{1b}') && s.contains("Weekly [31m review"),
        "{s}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn routine_done_uses_the_idempotent_instance_route_when_there_is_an_instance() {
    let env = Env::new().await;
    routine_fixture(&env, "2026-10-12").await;
    let out = today_cmd(&env)
        .args(["routine", "done", "32", "--date", "2026-10-12", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("routine_done", &v);
    assert_eq!(v["data"]["routine"]["done"], true);
    assert_eq!(v["data"]["already_done"], false);
    assert_eq!(
        env.requests_to("/api/daily-routines/tasks/32/instances/502/complete")
            .await
            .len(),
        1
    );
    assert!(env
        .requests_to("/api/daily-routines/tasks/32/complete")
        .await
        .is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn routine_done_never_toggles_a_finished_routine_back() {
    let env = Env::new().await;
    routine_fixture(&env, "2026-10-12").await;
    let out = today_cmd(&env)
        .args(["routine", "done", "31", "--date", "2026-10-12", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(stdout_json(&out)["data"]["already_done"], true);
    let posts: Vec<_> = env
        .server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.method.as_str() == "POST")
        .collect();
    assert!(posts.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn routine_done_for_today_without_an_instance_completes_it_once() {
    let env = Env::new().await;
    let today = local_today();
    routine_fixture(&env, &today).await;
    let out = today_cmd(&env)
        .args(["routine", "done", "33", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        env.requests_to("/api/daily-routines/tasks/33/complete")
            .await
            .len(),
        1
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn routine_done_refuses_virtual_instances_of_other_days_and_unknown_ids() {
    let env = Env::new().await;
    routine_fixture(&env, "2026-01-01").await;
    let out = today_cmd(&env)
        .args(["routine", "done", "33", "--date", "2026-01-01"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", text(&out.stderr));
    let out = today_cmd(&env)
        .args(["routine", "done", "999", "--date", "2026-01-01"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    for bad in ["abc", "0", "-1", "1;2", "1234567890123"] {
        let out = today_cmd(&env)
            .args(["routine", "done", bad])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{bad}");
    }
    let posts = env
        .server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .filter(|r| r.method.as_str() == "POST")
        .count();
    assert_eq!(posts, 0);
}
