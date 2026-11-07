mod common;
use common::*;
use serde_json::json;
use wiremock::matchers::{body_json, header, method, path};
use wiremock::{Mock, ResponseTemplate};

const PERSONA: &str = "0f3a9c1e-1b2c-4d5e-8f90-abcdef012345";
const SKILL_MD: &str = "---\nslug: triage\ntitle: Triage\nwhen: use it\n---\nBody\n";

fn skill(body: bool) -> serde_json::Value {
    let mut v = json!({
        "slug": "triage", "title": "Tri\u{1b}[31mage", "when": "use it", "actions": [], "version": 2,
        "updated_at": "2026-11-01T10:00:00Z",
        "attached_personas": [
            {"id": PERSONA, "name": "Reviewer", "overridden": false},
            {"id": "x", "name": "Writer", "overridden": true}
        ]
    });
    if body {
        v["body"] = json!(SKILL_MD);
    }
    v
}

async fn mount(env: &Env, m: &str, p: &str, status: u16, body: serde_json::Value) {
    Mock::given(method(m))
        .and(path(p))
        .and(header("authorization", format!("Bearer {ACCESS}").as_str()))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&env.server)
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn list_prints_skills_and_attached_personas() {
    let env = Env::new().await;
    env.login_seed();
    mount(
        &env,
        "GET",
        "/api/shared-skills",
        200,
        json!({"data": [skill(false)]}),
    )
    .await;

    let out = env
        .cmd()
        .args(["skills", "list", "--no-color"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let s = text(&out.stdout);
    assert!(s.contains("triage") && s.contains("v2"), "{s}");
    assert!(s.contains("Reviewer, Writer (overridden)"), "{s}");
    assert!(!s.contains('\u{1b}'));

    let out = env
        .cmd()
        .args(["skills", "list", "--json"])
        .output()
        .unwrap();
    let v = stdout_json(&out);
    assert_eq!(v["data"]["skills"][0]["slug"], "triage");
    assert_no_secrets(&out);
}

#[tokio::test(flavor = "multi_thread")]
async fn show_prints_the_markdown_and_quiet_prints_only_the_body() {
    let env = Env::new().await;
    env.login_seed();
    mount(
        &env,
        "GET",
        "/api/shared-skills/triage",
        200,
        json!({"data": skill(true)}),
    )
    .await;

    let out = env
        .cmd()
        .args(["skills", "show", "triage", "--no-color"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("slug: triage\ntitle: Triage"));

    let out = env
        .cmd()
        .args(["skills", "show", "triage", "-q"])
        .output()
        .unwrap();
    assert_eq!(text(&out.stdout).trim_end(), SKILL_MD.trim_end());

    let out = env
        .cmd()
        .args(["skills", "show", "triage", "--json"])
        .output()
        .unwrap();
    assert_eq!(stdout_json(&out)["data"]["skill"]["version"], 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn show_lists_the_files_without_their_content() {
    let env = Env::new().await;
    env.login_seed();
    mount(
        &env,
        "GET",
        "/api/shared-skills/triage",
        200,
        json!({"data": skill(true)}),
    )
    .await;
    mount(
        &env,
        "GET",
        "/api/shared-skills/triage/files",
        200,
        json!({"data": {"files_sha256": "abc", "files": [
            {"path": "scripts/run.sh", "size": 42, "sha256": "x", "updated_at": "2026-11-01T10:00:00Z"}
        ]}}),
    )
    .await;

    let out = env
        .cmd()
        .args(["skills", "show", "triage", "--no-color"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let s = text(&out.stdout);
    assert!(
        s.contains("Files") && s.contains("scripts/run.sh") && s.contains("42 B"),
        "{s}"
    );

    let out = env
        .cmd()
        .args(["skills", "show", "triage", "--json"])
        .output()
        .unwrap();
    assert_eq!(
        stdout_json(&out)["data"]["files"][0]["path"],
        "scripts/run.sh"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn show_on_an_old_server_without_the_files_route_prints_no_files() {
    let env = Env::new().await;
    env.login_seed();
    mount(
        &env,
        "GET",
        "/api/shared-skills/triage",
        200,
        json!({"data": skill(true)}),
    )
    .await;
    mount(
        &env,
        "GET",
        "/api/shared-skills/triage/files",
        404,
        json!({"message": "Not Found"}),
    )
    .await;

    let out = env
        .cmd()
        .args(["skills", "show", "triage", "--no-color"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(!text(&out.stdout).contains("Files"));

    let out = env
        .cmd()
        .args(["skills", "show", "triage", "--json"])
        .output()
        .unwrap();
    assert!(stdout_json(&out)["data"].get("files").is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn put_sends_the_file_as_the_body_and_reads_stdin() {
    let env = Env::new().await;
    env.login_seed();
    Mock::given(method("PUT"))
        .and(path("/api/shared-skills/triage"))
        .and(body_json(json!({"body": SKILL_MD})))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": skill(true)})))
        .expect(2)
        .mount(&env.server)
        .await;
    let file = env.xdg().join("triage.md");
    std::fs::write(&file, SKILL_MD).unwrap();

    let out = env
        .cmd()
        .args(["skills", "put", "triage", "--file"])
        .arg(&file)
        .arg("--json")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(stdout_json(&out)["data"]["skill"]["slug"], "triage");

    let out = env
        .cmd()
        .args(["skills", "put", "triage", "--file", "-"])
        .write_stdin(SKILL_MD)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("saved"));
}

#[tokio::test(flavor = "multi_thread")]
async fn put_422_is_a_usage_error_with_the_server_message() {
    let env = Env::new().await;
    env.login_seed();
    mount(
        &env,
        "PUT",
        "/api/shared-skills/triage",
        422,
        json!({"message": "The given data was invalid.", "errors": {"slug": ["must match the URL slug"]}}),
    )
    .await;
    let out = env
        .cmd()
        .args(["skills", "put", "triage", "--file", "-", "--json"])
        .write_stdin(SKILL_MD)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("slug must match the URL slug")
            || text(&out.stdout).contains("slug must match the URL slug")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_needs_confirmation_and_reports_detached_personas() {
    let env = Env::new().await;
    env.login_seed();
    mount(
        &env,
        "DELETE",
        "/api/shared-skills/triage",
        200,
        json!({"data": {"detached_personas": [{"id": PERSONA, "name": "Reviewer"}]}}),
    )
    .await;

    let out = env
        .cmd()
        .args(["skills", "delete", "triage"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10));
    assert!(env
        .requests_to("/api/shared-skills/triage")
        .await
        .is_empty());

    let out = env
        .cmd()
        .args(["skills", "delete", "triage", "-y", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_eq!(v["data"]["detached_personas"][0]["name"], "Reviewer");
}

#[tokio::test(flavor = "multi_thread")]
async fn attach_and_detach_use_the_persona_routes() {
    let env = Env::new().await;
    env.login_seed();
    let route = format!("/api/personas/{PERSONA}/shared-skills/triage");
    mount(
        &env,
        "PUT",
        &route,
        200,
        json!({"data": {"slug": "triage", "title": "Triage", "version": 2, "overridden": true}}),
    )
    .await;
    Mock::given(method("DELETE"))
        .and(path(route.as_str()))
        .respond_with(ResponseTemplate::new(204))
        .mount(&env.server)
        .await;

    let out = env
        .cmd()
        .args(["skills", "attach", &PERSONA.to_uppercase(), "triage"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(text(&out.stdout).contains("own skill with this slug wins"));

    let out = env
        .cmd()
        .args(["skills", "detach", PERSONA, "triage", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(stdout_json(&out)["data"]["detached"], true);
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_arguments_fail_before_any_request() {
    let env = Env::new().await;
    env.login_seed();
    for args in [
        vec!["skills", "show", "../etc"],
        vec!["skills", "attach", "Reviewer", "triage"],
        vec!["skills", "detach", PERSONA, "a/b"],
        vec!["skills", "show", "Triage"],
        vec!["skills", "put", "triage"],
    ] {
        let out = env.cmd().args(&args).output().unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
    }
    assert!(env.server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn http_errors_map_to_exit_codes() {
    let env = Env::new().await;
    env.login_seed();
    for (slug, status, exit) in [("nf", 404, 3), ("fb", 403, 5), ("cf", 409, 6)] {
        mount(
            &env,
            "GET",
            &format!("/api/shared-skills/{slug}"),
            status,
            json!({"message": "nope"}),
        )
        .await;
        let out = env.cmd().args(["skills", "show", slug]).output().unwrap();
        assert_eq!(out.status.code(), Some(exit), "{status}");
    }
    Mock::given(method("GET"))
        .and(path("/api/shared-skills/scope"))
        .respond_with(
            ResponseTemplate::new(403).set_body_json(json!({"error": "insufficient_scope"})),
        )
        .mount(&env.server)
        .await;
    let out = env
        .cmd()
        .args(["skills", "show", "scope", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(5));
    let hint = stdout_json(&out)["error"]["hint"]
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        hint.contains("tmgr login")
            && hint.contains("skills:read,skills:write")
            && !hint.contains("--with-token"),
        "{hint}"
    );

    let env2 = Env::new().await;
    env2.login_seed();
    mount(&env2, "GET", "/api/shared-skills", 401, json!({})).await;
    Mock::given(method("POST"))
        .and(path("/api/auth/refresh"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&env2.server)
        .await;
    let out = env2.cmd().args(["skills", "list"]).output().unwrap();
    assert_eq!(out.status.code(), Some(4));
}
