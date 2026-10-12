mod common;
use common::*;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

struct Stub {
    script: PathBuf,
    log: PathBuf,
}

fn stub(env: &Env, body: &str) -> Stub {
    let dir = env.tmp.path().join("bin");
    let log = env.tmp.path().join("editor.log");
    let script = write_script(
        &dir,
        "ed",
        &format!(
            "for a; do last=\"$a\"; done\n{{ echo \"argc=$#\"; for a; do echo \"arg=$a\"; done; echo \"file=$last\"; ls -ld \"$(dirname \"$last\")\" | cut -c1-10 | sed 's/^/dir=/'; ls -l \"$last\" | cut -c1-10 | sed 's/^/mode=/'; echo \"initial=$(cat \"$last\" | tr '\\n' '|')\"; }} > '{}'\n{body}",
            log.display()
        ),
    );
    Stub { script, log }
}

fn log_value(s: &Stub, key: &str) -> String {
    std::fs::read_to_string(&s.log)
        .unwrap()
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{key}=")).map(str::to_string))
        .unwrap_or_default()
}

async fn fixtures(env: &Env) {
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_json(
        "/api/tasks/by-key/TM-512",
        json!({"data": task_json(8974, "TM-512: Checkout")}),
    )
    .await;
    Mock::given(method("POST"))
        .and(path("/api/pages"))
        .and(header("x-workspace-id", "1"))
        .and(header("authorization", format!("Bearer {ACCESS}").as_str()))
        .respond_with(ResponseTemplate::new(201).set_body_json(json!({"data": {
            "id": 77, "title": "Deploy checklist", "version": 1, "body": "x"
        }})))
        .mount(&env.server)
        .await;
}

fn creates(reqs: Vec<wiremock::Request>) -> Vec<Value> {
    reqs.into_iter()
        .filter(|r| r.method.as_str() == "POST")
        .map(|r| serde_json::from_slice(&r.body).unwrap())
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn note_creates_a_page_linked_to_the_task_from_a_private_temp_file() {
    let env = Env::new().await;
    fixtures(&env).await;
    let s = stub(
        &env,
        "printf '# Deploy checklist\\n\\n- [x] migrations\\n' > \"$last\"",
    );
    let marker = env.tmp.path().join("PWNED");
    let editor = format!(
        "{} --wait 'two words' \";touch {}\" $(touch {})",
        s.script.display(),
        marker.display(),
        marker.display()
    );
    let out = env
        .cmd()
        .env("EDITOR", &editor)
        .args(["note", "--task", "TM-512", "--parent", "5", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_valid("page_ref", &v);
    assert_eq!(v["data"]["page_id"], 77);
    assert_eq!(v["data"]["task"], "TM-512");
    assert!(
        !marker.exists(),
        "the editor command line must never reach a shell"
    );
    assert_eq!(log_value(&s, "argc"), "6");
    let log = std::fs::read_to_string(&s.log).unwrap();
    assert!(log.contains("arg=--wait\narg=two words\n"), "{log}");
    assert!(
        log.contains(&format!("arg=;touch {}\n", marker.display())),
        "{log}"
    );
    assert!(log.contains("arg=$(touch"), "{log}");
    assert_eq!(log_value(&s, "dir"), "drwx------");
    assert_eq!(log_value(&s, "mode"), "-rw-------");
    let file = log_value(&s, "file");
    assert!(!Path::new(&file).exists(), "temp file must be removed");
    assert!(
        !Path::new(&file).parent().unwrap().exists(),
        "temp dir must be removed"
    );
    let sent = creates(env.requests_to("/api/pages").await);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0]["title"], "Deploy checklist");
    assert_eq!(sent[0]["parent_id"], 5);
    let body = sent[0]["body"].as_str().unwrap();
    assert!(
        body.starts_with("# Deploy checklist\n\n- [x] migrations\n"),
        "{body}"
    );
    assert!(body.contains("[TM-512](tmgr://task/8974)"), "{body}");
    assert_no_secrets(&out);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_positional_title_seeds_the_file_and_visual_wins_over_editor() {
    let env = Env::new().await;
    fixtures(&env).await;
    let s = stub(&env, "printf -- '- more\\n' >> \"$last\"");
    let out = env
        .cmd()
        .env("VISUAL", s.script.display().to_string())
        .env("EDITOR", "/nonexistent/editor")
        .args(["note", "deploy \u{1b}[31mchecklist"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(log_value(&s, "initial"), "# deploy  [31mchecklist||");
    assert!(log_value(&s, "file").ends_with("/deploy-31mchecklist.md"));
    let sent = creates(env.requests_to("/api/pages").await);
    assert_eq!(sent[0]["title"], "deploy  [31mchecklist");
    assert!(sent[0].get("parent_id").is_none());
    let human = text(&out.stdout);
    assert!(
        human.contains("saved as page") && human.contains("(id 77)"),
        "{human}"
    );
    assert!(!human.contains('\u{1b}'));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_empty_or_untouched_note_is_cancelled_without_a_request() {
    let env = Env::new().await;
    fixtures(&env).await;
    let empty = stub(&env, ": > \"$last\"");
    let out = env
        .cmd()
        .env("EDITOR", empty.script.display().to_string())
        .args(["note"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(130), "{}", text(&out.stderr));
    let untouched = stub(&env, "true");
    let out = env
        .cmd()
        .env("EDITOR", untouched.script.display().to_string())
        .args(["note", "--title", "Only a title"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(130));
    assert!(creates(env.requests_to("/api/pages").await).is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_editor_saves_nothing() {
    let env = Env::new().await;
    fixtures(&env).await;
    let s = stub(&env, "echo content > \"$last\"; exit 3");
    let out = env
        .cmd()
        .env("EDITOR", s.script.display().to_string())
        .args(["note"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(creates(env.requests_to("/api/pages").await).is_empty());
    assert!(!Path::new(&log_value(&s, "file")).exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failed_save_keeps_the_note_and_says_where() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    Mock::given(method("POST"))
        .and(path("/api/pages"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&env.server)
        .await;
    let s = stub(&env, "echo 'precious thought' > \"$last\"");
    let out = env
        .cmd()
        .env("EDITOR", s.script.display().to_string())
        .args(["note", "--title", "Keep me", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(7));
    let v = stdout_json(&out);
    let hint = v["error"]["hint"].as_str().unwrap();
    let kept = hint.strip_prefix("your note is kept at ").unwrap();
    assert!(std::fs::read_to_string(kept)
        .unwrap()
        .contains("precious thought"));
    std::fs::remove_dir_all(Path::new(kept).parent().unwrap()).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_arguments_fail_before_the_editor_starts() {
    let env = Env::new().await;
    fixtures(&env).await;
    let s = stub(&env, "echo x > \"$last\"");
    for (args, code) in [
        (vec!["note", "--parent", "abc"], 2),
        (vec!["note", "--parent", "0"], 2),
        (vec!["note", "--task", "../x"], 2),
        (vec!["note", "--task", "TM-9999"], 3),
        (vec!["note", "a", "--title", "b"], 2),
    ] {
        let out = env
            .cmd()
            .env("EDITOR", s.script.display().to_string())
            .args(&args)
            .output()
            .unwrap();
        assert_eq!(
            out.status.code(),
            Some(code),
            "{args:?} {}",
            text(&out.stderr)
        );
    }
    assert!(!s.log.exists(), "the editor must not have run");
}

#[tokio::test(flavor = "multi_thread")]
async fn an_unbalanced_editor_command_is_a_usage_error() {
    let env = Env::new().await;
    fixtures(&env).await;
    let out = env
        .cmd()
        .env("EDITOR", "vim 'unterminated")
        .args(["note"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}
