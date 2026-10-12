mod common;
use common::*;
use serde_json::json;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Mock, ResponseTemplate};

async fn mock_show_fixtures(env: &Env) {
    env.mock_workspaces().await;
    env.mock_statuses().await;
    env.mock_json(
        "/api/tasks/by-key/TM-512",
        json!({"data": task_json(8974, "TM-512: Checkout: Apple Pay button")}),
    )
    .await;
    env.mock_json(
        "/api/tasks/8974",
        json!({"data": task_json(8974, "TM-512: Checkout: Apple Pay button")}),
    )
    .await;
    env.mock_json(
        "/api/tasks/8974/comments",
        json!([
            {"id": 1, "message": "first", "created_at": "2026-10-12T09:00:00Z", "user": {"id": 3, "name": "Ann"}},
            {"id": 2, "message": "second\u{1b}[31m line", "created_at": "2026-10-12T09:05:00Z", "user": {"id": 4, "name": "Bob"}},
            {"id": 3, "message": "third", "created_at": "2026-10-12T09:10:00Z", "user": {"id": 3, "name": "Ann"}}
        ]),
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn task_show_by_key_json_matches_the_schema() {
    let env = Env::new().await;
    env.login_seed();
    mock_show_fixtures(&env).await;
    let out = env
        .cmd()
        .args(["task", "show", "TM-512", "--json", "--comments", "2"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("task", &v);
    assert_eq!(v["data"]["task"]["ref"], "TM-512");
    assert_eq!(v["data"]["task"]["title"], "Checkout: Apple Pay button");
    assert_eq!(v["data"]["task"]["status"]["name"], "In progress");
    assert_eq!(v["data"]["task"]["status"]["type"], "active");
    assert_eq!(v["data"]["task"]["category"]["code"], "TM");
    assert_eq!(
        v["data"]["task"]["url"],
        "http://localhost:5173/personal/tasks/8974"
    );
    assert_eq!(v["data"]["task"]["tracked_s"], 4320);
    assert_eq!(v["data"]["comments"].as_array().unwrap().len(), 2);
    assert_eq!(v["data"]["comments"][0]["message"], "second\u{1b}[31m line");
    assert_eq!(v["data"]["comments_total"], 3);
    assert!(!text(&out.stdout).contains("ann@example.com"));
    let key_req = &env.requests_to("/api/tasks/by-key/TM-512").await[0];
    assert_eq!(key_req.url.query(), Some("workspace_id=1"));
}

#[tokio::test(flavor = "multi_thread")]
async fn task_show_human_output_is_plain_text_without_control_characters() {
    let env = Env::new().await;
    env.login_seed();
    mock_show_fixtures(&env).await;
    let out = env.cmd().args(["task", "show", "#8974"]).output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let s = text(&out.stdout);
    assert!(s.contains("TM-512  Checkout: Apple Pay button"), "{s}");
    assert!(s.contains("In progress (active)"));
    assert!(
        s.contains("Plan\nDo it with docs (https://example.com/d)"),
        "{s}"
    );
    assert!(s.contains("Comments (last 3 of 3)"));
    assert!(s.contains("1h 12m tracked, 4h 00m estimated"));
    assert!(!s.contains('\u{1b}'));
    assert!(s.contains("http://localhost:5173/personal/tasks/8974"));
}

#[tokio::test(flavor = "multi_thread")]
async fn bare_number_is_a_global_task_id() {
    let env = Env::new().await;
    env.login_seed();
    mock_show_fixtures(&env).await;
    let out = env
        .cmd()
        .args(["task", "show", "8974", "--comments", "0", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(env.requests_to("/api/tasks/8974/comments").await.is_empty());
    assert_eq!(stdout_json(&out)["data"]["task"]["id"], 8974);
}

#[tokio::test(flavor = "multi_thread")]
async fn max_desc_truncates_the_description() {
    let env = Env::new().await;
    env.login_seed();
    mock_show_fixtures(&env).await;
    let out = env
        .cmd()
        .args([
            "task",
            "show",
            "8974",
            "--max-desc",
            "6",
            "--comments",
            "0",
            "--json",
        ])
        .output()
        .unwrap();
    let v = stdout_json(&out);
    assert_eq!(v["data"]["task"]["description"], "# Plan");
    assert_eq!(v["data"]["task"]["description_truncated"], true);
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_refs_never_reach_the_server() {
    let env = Env::new().await;
    env.login_seed();
    for bad in ["TM-1;rm", "../x", "-rf", "tm-1", "TM-"] {
        let out = env
            .cmd()
            .args(["task", "show", "--json", "--", bad])
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{bad}");
        let v = stdout_json(&out);
        assert_valid("error", &v);
        assert_eq!(v["error"]["code"], "usage");
    }
    assert!(env.server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn missing_task_is_exit_3_with_a_hint() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    Mock::given(method("GET"))
        .and(path("/api/tasks/by-key/TM-999"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json!({"message": "Task not found"})),
        )
        .mount(&env.server)
        .await;
    let out = env
        .cmd()
        .args(["task", "show", "TM-999", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    let v = stdout_json(&out);
    assert_valid("error", &v);
    assert_eq!(v["error"]["message"], "Task TM-999 not found");
    assert!(v["error"]["hint"].is_string());
    let human = env.cmd().args(["task", "show", "TM-999"]).output().unwrap();
    assert_eq!(human.status.code(), Some(3));
    assert!(text(&human.stderr).starts_with("error: Task TM-999 not found\nhint: "));
    assert!(human.stdout.is_empty());
}

fn list_page(items: Vec<serde_json::Value>, page: u64, last: u64) -> serde_json::Value {
    json!({"data": items, "links": {}, "meta": {"current_page": page, "last_page": last, "per_page": 2, "total": 5}})
}

fn light(id: u64, title: &str, status_id: u64) -> serde_json::Value {
    let mut t = task_json(id, title);
    t["status_id"] = json!(status_id);
    t["description"] = json!(null);
    t
}

#[tokio::test(flavor = "multi_thread")]
async fn task_list_json_paginates_with_an_opaque_cursor() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_statuses().await;
    for (page, items) in [
        (
            "1",
            vec![light(1, "TM-1: one", 10), light(2, "TM-2: two", 11)],
        ),
        ("2", vec![light(3, "TM-3: three", 12)]),
    ] {
        Mock::given(method("GET"))
            .and(path("/api/tasks"))
            .and(query_param("page", page))
            .and(query_param("per_page", "2"))
            .and(query_param("workspace_id", "1"))
            .and(header("authorization", format!("Bearer {ACCESS}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(list_page(
                items,
                page.parse().unwrap(),
                2,
            )))
            .mount(&env.server)
            .await;
    }
    let out = env
        .cmd()
        .args(["task", "list", "--limit", "2", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("task_list", &v);
    assert_eq!(v["data"]["items"].as_array().unwrap().len(), 2);
    assert_eq!(v["data"]["items"][1]["status"]["name"], "In progress");
    assert_eq!(v["data"]["next_cursor"], "2");

    let out = env
        .cmd()
        .args(["task", "list", "--limit", "2", "--cursor", "2", "--json"])
        .output()
        .unwrap();
    let v = stdout_json(&out);
    assert_valid("task_list", &v);
    assert_eq!(v["data"]["items"][0]["ref"], "TM-3");
    assert!(v["data"]["next_cursor"].is_null());

    let human = env
        .cmd()
        .args(["task", "list", "--limit", "2"])
        .output()
        .unwrap();
    let s = text(&human.stdout);
    assert!(s.contains("TM-1  Backlog      one"), "{s}");
    assert!(s.contains("more: tmgr task list --cursor 2"));
    let first = &env.requests_to("/api/tasks").await[0];
    assert!(first.url.query().unwrap().contains("fields=card"));
}

#[tokio::test(flavor = "multi_thread")]
async fn task_list_filters_by_project_and_status_type_on_the_server() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_statuses().await;
    env.mock_categories().await;
    Mock::given(method("GET"))
        .and(path("/api/tasks"))
        .and(query_param("project_category_id", "5"))
        .and(query_param("status_id", "11"))
        .respond_with(ResponseTemplate::new(200).set_body_json(list_page(
            vec![light(2, "TM-2: two", 11)],
            1,
            1,
        )))
        .mount(&env.server)
        .await;
    let out = env
        .cmd()
        .args([
            "task",
            "list",
            "-p",
            "tm",
            "--status-type",
            "active",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("task_list", &v);
    assert_eq!(v["data"]["items"][0]["project"], "TM");
    assert!(v["data"]["next_cursor"].is_null());
}

#[tokio::test(flavor = "multi_thread")]
async fn task_list_filters_client_side_when_a_type_has_several_statuses() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_json(
        "/api/workspaces/1/statuses",
        json!([
            {"id": 11, "name": "In progress", "type": "active"},
            {"id": 13, "name": "Review", "type": "active"},
            {"id": 12, "name": "Done", "type": "completed"}
        ]),
    )
    .await;
    Mock::given(method("GET"))
        .and(path("/api/tasks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(list_page(
            vec![
                light(1, "TM-1: a", 12),
                light(2, "TM-2: b", 13),
                light(3, "TM-3: c", 11),
            ],
            1,
            1,
        )))
        .mount(&env.server)
        .await;
    let out = env
        .cmd()
        .args([
            "task",
            "list",
            "--status-type",
            "active",
            "--limit",
            "1",
            "--json",
        ])
        .output()
        .unwrap();
    let v = stdout_json(&out);
    assert_eq!(v["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(v["data"]["items"][0]["ref"], "TM-2");
    assert_eq!(v["data"]["next_cursor"], "1.2");
    assert!(env.requests_to("/api/tasks").await[0]
        .url
        .query()
        .unwrap()
        .contains("per_page=1"));
}

#[tokio::test(flavor = "multi_thread")]
async fn empty_list_is_not_an_error() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_statuses().await;
    Mock::given(method("GET"))
        .and(path("/api/tasks"))
        .respond_with(ResponseTemplate::new(200).set_body_json(list_page(vec![], 1, 1)))
        .mount(&env.server)
        .await;
    let out = env.cmd().args(["task", "list"]).output().unwrap();
    assert!(out.status.success());
    assert!(text(&out.stdout).contains("No tasks found. Add one: tmgr add"));
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_list_flags_are_usage_errors() {
    let env = Env::new().await;
    env.login_seed();
    for args in [
        vec!["task", "list", "--status-type", "bogus"],
        vec!["task", "list", "--limit", "0"],
        vec!["task", "list", "--cursor", "x"],
        vec!["task", "list", "-p", "a b"],
    ] {
        let out = env.cmd().args(&args).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
    assert!(env.server.received_requests().await.unwrap().is_empty());
}
