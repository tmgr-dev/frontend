#![cfg(all(unix, feature = "runner"))]
mod common;
use common::*;
use serde_json::{json, Value};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use wiremock::matchers::{method, path};
use wiremock::{Mock, ResponseTemplate};

const T0: i64 = 1_800_000_000;
const ME: &str = "me@example.com";

fn marker_script(dir: &Path, name: &str) -> (PathBuf, PathBuf) {
    let marker = dir.join(format!("{name}.ran"));
    let script = write_script(dir, name, &format!("touch '{}'\ncat", marker.display()));
    (script, marker)
}

fn signed_looking_commit(repo: &Path, epoch: i64, email: &str) -> String {
    let tree = git(repo, &["rev-parse", "HEAD^{tree}"]);
    let parent = git(repo, &["rev-parse", "HEAD"]);
    let body = format!(
        "tree {tree}\nparent {parent}\nauthor T <{email}> {epoch} +0000\ncommitter T <{email}> {epoch} +0000\ngpgsig -----BEGIN PGP SIGNATURE-----\n \n abcd\n -----END PGP SIGNATURE-----\n\nsigned\n"
    );
    let mut child = Command::new("git")
        .args([
            "hash-object",
            "-t",
            "commit",
            "-w",
            "--stdin",
            "--literally",
        ])
        .current_dir(repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(body.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    let sha = String::from_utf8(out.stdout).unwrap().trim().to_string();
    git(repo, &["update-ref", "HEAD", &sha]);
    sha
}

#[derive(Clone, Copy)]
enum Via {
    Direct,
    Include,
}

fn set_signature_program(repo: &Path, script: &Path, via: Via) {
    match via {
        Via::Direct => {
            git(repo, &["config", "log.showSignature", "true"]);
            git(repo, &["config", "gpg.program", script.to_str().unwrap()]);
        }
        Via::Include => {
            let extra = repo.join(".git/extra.cfg");
            std::fs::write(
                &extra,
                format!(
                    "[log]\n\tshowSignature = true\n[gpg]\n\tprogram = {}\n",
                    script.display()
                ),
            )
            .unwrap();
            git(repo, &["config", "include.path", "extra.cfg"]);
        }
    }
}

fn run_view(extra: Value) -> Value {
    let mut v = json!({
        "id": 41, "task_id": 8974, "workspace_id": 1, "user_id": 1, "agent": "claude-code",
        "model": "opus", "branch": "tm-512-x", "status": "running",
        "started_at": "2026-10-12T09:00:00Z", "duration_seconds": 12, "commits": [], "version": 1
    });
    for (k, val) in extra.as_object().cloned().unwrap_or_default() {
        v[k] = val;
    }
    v
}

async fn mock_post(env: &Env, verb: &str, p: &str, body: Value) {
    Mock::given(method(verb))
        .and(path(p))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&env.server)
        .await;
}

async fn agent_finish_reads_ignore_program_settings(via: Via) {
    let env = Env::new().await;
    env.login_seed();
    let started = chrono::DateTime::from_timestamp(T0, 0)
        .unwrap()
        .to_rfc3339();
    let ended = chrono::DateTime::from_timestamp(T0 + 600, 0)
        .unwrap()
        .to_rfc3339();
    mock_post(
        &env,
        "POST",
        "/api/agent-work/41/finish",
        run_view(json!({"status": "succeeded", "started_at": started, "ended_at": ended})),
    )
    .await;
    mock_post(
        &env,
        "POST",
        "/api/agent-work/41/commits",
        run_view(json!({})),
    )
    .await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    git(&repo, &["config", "user.email", ME]);
    let sha = signed_looking_commit(&repo, T0 + 60, ME);
    let (script, marker) = marker_script(&env.tmp.path().join("bin"), "gpg");
    set_signature_program(&repo, &script, via);
    let out = env
        .tmgr_in(&repo, None)
        .args([
            "agent",
            "finish",
            "--run",
            "41",
            "--status",
            "succeeded",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(!marker.exists(), "the signature program ran");
    let posts = env.requests_to("/api/agent-work/41/commits").await;
    assert_eq!(posts.len(), 1);
    let body: Value = serde_json::from_slice(&posts[0].body).unwrap();
    assert_eq!(body["commits"][0]["sha"], sha.as_str());
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_finish_reads_ignore_repository_program_settings() {
    agent_finish_reads_ignore_program_settings(Via::Direct).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_finish_reads_ignore_included_program_settings() {
    agent_finish_reads_ignore_program_settings(Via::Include).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn hook_stop_reads_ignore_repository_program_settings() {
    let env = Env::new().await;
    env.login_seed();
    mock_post(
        &env,
        "POST",
        "/api/notifications/push",
        json!({"data": {"status": "sent", "channels": ["mobile"]}}),
    )
    .await;
    mock_post(
        &env,
        "PATCH",
        "/api/agent-work/41",
        json!({"id": 41, "status": "running"}),
    )
    .await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    git(&repo, &["checkout", "-q", "-b", "tm-512-x"]);
    let start = git(&repo, &["rev-parse", "HEAD"]);
    signed_looking_commit(&repo, T0, ME);
    let (script, marker) = marker_script(&env.tmp.path().join("bin"), "gpg");
    set_signature_program(&repo, &script, Via::Direct);
    let runs = json!({"schema": "tmgr.runs.v1", "runs": [{
        "run_id": 41, "task_id": 8974, "task_key": "TM-512", "branch": "tm-512-x",
        "repo": repo.display().to_string(), "start_sha": start, "agent": "claude-code",
        "started_at": "2026-10-12T09:00:00Z", "commits": ["abc1234 earlier"], "last_sent": 0
    }]});
    std::fs::create_dir_all(env.state_dir()).unwrap();
    std::fs::write(env.state_dir().join("runs.json"), runs.to_string()).unwrap();
    let stdin = json!({"session_id": "s9", "cwd": repo.display().to_string()}).to_string();
    let out = env
        .cmd()
        .env("PATH", path_with(None))
        .env("TMGR_NOTIFY_TOKEN", "tmgrn_hooktoken_SECRET0009")
        .env("TMGR_URL", env.server.uri())
        .env("TMGR_HOOK_BUDGET_MS", "5000")
        .args(["hook", "claude", "stop"])
        .write_stdin(stdin.into_bytes())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    assert!(!marker.exists(), "the signature program ran");
    let patches = env.requests_to("/api/agent-work/41").await;
    assert_eq!(patches.len(), 1, "{}", text(&out.stderr));
    let body: Value = serde_json::from_slice(&patches[0].body).unwrap();
    assert_eq!(body["commits"].as_array().unwrap().len(), 2);
}

async fn start_env() -> (Env, PathBuf) {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_json(
        "/api/workspaces/1/statuses",
        json!([
            {"id": 10, "name": "Backlog", "type": "default"},
            {"id": 11, "name": "In progress", "type": "active"}
        ]),
    )
    .await;
    let mut t = task_json(8974, "TM-512: Checkout: Apple Pay button");
    t["status_id"] = json!(10);
    t["start_time"] = json!(0);
    env.mock_json("/api/tasks/by-key/TM-512", json!({"data": t.clone()}))
        .await;
    env.mock_any("POST", "/api/tasks/8974/countdown", json!({"data": t}))
        .await;
    env.mock_any("PUT", "/api/tasks/8974/11", json!({"data": {}}))
        .await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    (env, repo)
}

async fn server_untouched(env: &Env) {
    assert!(env
        .requests_to("/api/tasks/8974/countdown")
        .await
        .is_empty());
    assert!(env.requests_to("/api/tasks/8974/11").await.is_empty());
}

fn nothing_changed(repo: &Path) {
    assert_eq!(git(repo, &["branch", "--show-current"]), "main");
    assert_eq!(git(repo, &["stash", "list"]), "");
    assert_eq!(
        std::fs::read_to_string(repo.join("README.md")).unwrap(),
        "changed\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn start_refuses_a_repository_that_declares_filters() {
    let (env, repo) = start_env().await;
    let (script, marker) = marker_script(&env.tmp.path().join("bin"), "clean");
    git(
        &repo,
        &["config", "filter.x.clean", script.to_str().unwrap()],
    );
    std::fs::write(repo.join(".git/info/attributes"), "README.md filter=x\n").unwrap();
    std::fs::write(repo.join("README.md"), "changed\n").unwrap();
    let out = env
        .tmgr_in(&repo, None)
        .args(["start", "TM-512", "--stash", "--json"])
        .output()
        .unwrap();
    assert!(!marker.exists(), "a program from the repository config ran");
    assert_eq!(out.status.code(), Some(6), "{}", text(&out.stderr));
    assert_valid("error", &stdout_json(&out));
    nothing_changed(&repo);
    server_untouched(&env).await;
    assert!(env.read_active().is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn start_succeeds_without_running_hooks_from_a_configured_hooks_path() {
    let (env, repo) = start_env().await;
    let hooks = env.tmp.path().join("bin");
    let (_, checkout_marker) = marker_script(&hooks, "post-checkout");
    let (_, ref_marker) = marker_script(&hooks, "reference-transaction");
    git(
        &repo,
        &["config", "core.hooksPath", hooks.to_str().unwrap()],
    );
    std::fs::write(repo.join("README.md"), "changed\n").unwrap();
    let out = env
        .tmgr_in(&repo, None)
        .args(["start", "TM-512", "--stash", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(!checkout_marker.exists(), "a hook ran");
    assert!(!ref_marker.exists(), "a hook ran");
    assert_eq!(
        git(&repo, &["branch", "--show-current"]),
        "tm-512-checkout-apple-pay-button"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn start_refuses_a_repository_that_includes_other_config() {
    let (env, repo) = start_env().await;
    std::fs::write(repo.join(".git/extra.cfg"), "[core]\n\tpager = true\n").unwrap();
    git(&repo, &["config", "include.path", "extra.cfg"]);
    std::fs::write(repo.join("README.md"), "changed\n").unwrap();
    let out = env
        .tmgr_in(&repo, None)
        .args(["start", "TM-512", "--stash", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(6), "{}", text(&out.stderr));
    nothing_changed(&repo);
    server_untouched(&env).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn start_does_not_run_repository_hook_files() {
    let (env, repo) = start_env().await;
    let (script, marker) = marker_script(&env.tmp.path().join("bin"), "post-checkout");
    std::fs::copy(&script, repo.join(".git/hooks/post-checkout")).unwrap();
    let out = env
        .tmgr_in(&repo, None)
        .args(["start", "TM-512", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(!marker.exists(), "a hook from the repository ran");
    assert_eq!(
        git(&repo, &["branch", "--show-current"]),
        "tm-512-checkout-apple-pay-button"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn start_treats_files_of_a_global_filter_as_clean() {
    let (env, repo) = start_env().await;
    let gitconfig = env.tmp.path().join("gitconfig");
    std::fs::write(
        &gitconfig,
        "[filter \"upper\"]\n\tclean = tr a-z A-Z\n\tsmudge = cat\n",
    )
    .unwrap();
    std::fs::write(repo.join(".gitattributes"), "*.dat filter=upper\n").unwrap();
    std::fs::write(repo.join("a.dat"), "lower\n").unwrap();
    let with_global = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(&repo)
            .env("GIT_CONFIG_GLOBAL", &gitconfig)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "T")
            .env("GIT_AUTHOR_EMAIL", ME)
            .env("GIT_COMMITTER_NAME", "T")
            .env("GIT_COMMITTER_EMAIL", ME)
            .output()
            .unwrap();
        assert!(out.status.success(), "{args:?}");
    };
    with_global(&["add", "."]);
    with_global(&["commit", "-q", "-m", "filtered"]);
    let out = env
        .tmgr_in(&repo, None)
        .env("GIT_CONFIG_GLOBAL", &gitconfig)
        .args(["start", "TM-512", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        git(&repo, &["branch", "--show-current"]),
        "tm-512-checkout-apple-pay-button"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn start_still_stashes_and_switches_an_ordinary_repository() {
    let (env, repo) = start_env().await;
    std::fs::write(repo.join("README.md"), "changed\n").unwrap();
    std::fs::write(repo.join("new.txt"), "x\n").unwrap();
    let out = env
        .tmgr_in(&repo, None)
        .args(["start", "TM-512", "--stash", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(git(&repo, &["status", "--porcelain"]), "");
    assert!(git(&repo, &["stash", "list"]).contains("tmgr: before TM-512"));
}

#[tokio::test(flavor = "multi_thread")]
async fn start_refuses_a_repository_that_configures_a_transfer_program() {
    let (env, repo) = start_env().await;
    git(&repo, &["config", "lfs.customtransfer.x.path", "/bin/true"]);
    std::fs::write(repo.join("README.md"), "changed\n").unwrap();
    let out = env
        .tmgr_in(&repo, None)
        .args(["start", "TM-512", "--stash", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(6), "{}", text(&out.stderr));
    nothing_changed(&repo);
    server_untouched(&env).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn start_ignores_files_that_the_global_excludes_file_ignores() {
    let (env, repo) = start_env().await;
    let ignore = env.tmp.path().join("global-ignore");
    std::fs::write(&ignore, ".DS_Store\n").unwrap();
    let gitconfig = env.tmp.path().join("gitconfig");
    std::fs::write(
        &gitconfig,
        format!("[core]\n\texcludesFile = {}\n", ignore.display()),
    )
    .unwrap();
    std::fs::write(repo.join(".DS_Store"), "x").unwrap();
    let out = env
        .tmgr_in(&repo, None)
        .env("GIT_CONFIG_GLOBAL", &gitconfig)
        .args(["start", "TM-512", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        git(&repo, &["branch", "--show-current"]),
        "tm-512-checkout-apple-pay-button"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_start_records_the_repository_root_despite_a_work_tree_setting() {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_json(
        "/api/tasks/by-key/TM-512",
        json!({"data": task_json(8974, "TM-512: Checkout: Apple Pay button")}),
    )
    .await;
    mock_post(
        &env,
        "POST",
        "/api/tasks/8974/agent-work",
        run_view(json!({})),
    )
    .await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let decoy = env.tmp.path().join("decoy");
    std::fs::create_dir_all(&decoy).unwrap();
    git(&repo, &["config", "core.worktree", decoy.to_str().unwrap()]);
    let out = env
        .tmgr_in(&repo, None)
        .args([
            "agent",
            "start",
            "TM-512",
            "--agent",
            "claude-code",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let runs: Value =
        serde_json::from_str(&std::fs::read_to_string(env.state_dir().join("runs.json")).unwrap())
            .unwrap();
    assert_eq!(runs["runs"][0]["repo"], repo.display().to_string());
}

#[tokio::test(flavor = "multi_thread")]
async fn project_hook_install_uses_the_repository_root_despite_a_work_tree_setting() {
    let env = Env::new().await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let decoy = env.tmp.path().join("decoy");
    std::fs::create_dir_all(&decoy).unwrap();
    git(&repo, &["config", "core.worktree", decoy.to_str().unwrap()]);
    let out = env
        .tmgr_in(&repo, None)
        .args(["hook", "install", "claude", "--project", "--yes", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(!decoy.join(".claude").exists());
    assert!(repo.join(".claude/settings.json").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn global_hook_lookup_ignores_the_current_repository() {
    let env = Env::new().await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let extra = env.tmp.path().join("extra.cfg");
    std::fs::write(&extra, "[core]\n\thooksPath = relative-hooks\n").unwrap();
    let gitconfig = env.tmp.path().join("gitconfig");
    std::fs::write(
        &gitconfig,
        format!(
            "[includeIf \"gitdir:{}/\"]\n\tpath = {}\n",
            repo.display(),
            extra.display()
        ),
    )
    .unwrap();
    let out = env
        .tmgr_in(&repo, None)
        .env("GIT_CONFIG_GLOBAL", &gitconfig)
        .args(["git", "install-hooks", "--global", "--yes", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10), "{}", text(&out.stdout));
    assert_eq!(stdout_json(&out)["error"]["code"], "needs_confirmation");
}

#[derive(Clone, Copy)]
enum Layout {
    Linked,
    Subdir,
    PerWorktreeConfig,
    LinkedPerWorktreeConfig,
}

fn layout(repo: &Path, kind: Layout) -> (PathBuf, PathBuf) {
    let linked = |repo: &Path| {
        let wt = repo.parent().unwrap().join("wt");
        git(
            repo,
            &[
                "worktree",
                "add",
                "-q",
                "-b",
                "tm-512-x",
                wt.to_str().unwrap(),
            ],
        );
        std::fs::canonicalize(wt).unwrap()
    };
    match kind {
        Layout::Linked => {
            let wt = linked(repo);
            (wt.clone(), wt)
        }
        Layout::Subdir => {
            git(repo, &["checkout", "-q", "-b", "tm-512-x"]);
            let sub = repo.join("sub/deeper");
            std::fs::create_dir_all(&sub).unwrap();
            (repo.to_path_buf(), sub)
        }
        Layout::PerWorktreeConfig => {
            git(repo, &["config", "extensions.worktreeConfig", "true"]);
            git(repo, &["config", "--worktree", "color.ui", "never"]);
            git(repo, &["checkout", "-q", "-b", "tm-512-x"]);
            (repo.to_path_buf(), repo.to_path_buf())
        }
        Layout::LinkedPerWorktreeConfig => {
            git(repo, &["config", "extensions.worktreeConfig", "true"]);
            let wt = linked(repo);
            git(&wt, &["config", "--worktree", "color.ui", "never"]);
            (wt.clone(), wt)
        }
    }
}

async fn hook_stop_posts_new_commits(kind: Layout) {
    let env = Env::new().await;
    env.login_seed();
    mock_post(
        &env,
        "POST",
        "/api/notifications/push",
        json!({"data": {"status": "sent", "channels": ["mobile"]}}),
    )
    .await;
    mock_post(
        &env,
        "PATCH",
        "/api/agent-work/41",
        json!({"id": 41, "status": "running"}),
    )
    .await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let (root, cwd) = layout(&repo, kind);
    let start = git(&root, &["rev-parse", "HEAD"]);
    signed_looking_commit(&root, T0, ME);
    let runs = json!({"schema": "tmgr.runs.v1", "runs": [{
        "run_id": 41, "task_id": 8974, "task_key": "TM-512", "branch": "tm-512-x",
        "repo": root.display().to_string(), "start_sha": start, "agent": "claude-code",
        "started_at": "2026-10-12T09:00:00Z", "commits": ["abc1234 earlier"], "last_sent": 0
    }]});
    std::fs::create_dir_all(env.state_dir()).unwrap();
    std::fs::write(env.state_dir().join("runs.json"), runs.to_string()).unwrap();
    let stdin = json!({"session_id": "s9", "cwd": cwd.display().to_string()}).to_string();
    let out = env
        .cmd()
        .env("PATH", path_with(None))
        .env("TMGR_NOTIFY_TOKEN", "tmgrn_hooktoken_SECRET0009")
        .env("TMGR_URL", env.server.uri())
        .env("TMGR_HOOK_BUDGET_MS", "5000")
        .args(["hook", "claude", "stop"])
        .write_stdin(stdin.into_bytes())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
    let patches = env.requests_to("/api/agent-work/41").await;
    assert_eq!(patches.len(), 1, "{}", text(&out.stderr));
    let body: Value = serde_json::from_slice(&patches[0].body).unwrap();
    assert_eq!(body["commits"].as_array().unwrap().len(), 2);
}

async fn agent_start_records_root(kind: Layout) {
    let env = Env::new().await;
    env.login_seed();
    env.mock_workspaces().await;
    env.mock_json(
        "/api/tasks/by-key/TM-512",
        json!({"data": task_json(8974, "TM-512: Checkout: Apple Pay button")}),
    )
    .await;
    mock_post(
        &env,
        "POST",
        "/api/tasks/8974/agent-work",
        run_view(json!({})),
    )
    .await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let (root, cwd) = layout(&repo, kind);
    let out = env
        .tmgr_in(&cwd, None)
        .args([
            "agent",
            "start",
            "TM-512",
            "--agent",
            "claude-code",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let runs: Value =
        serde_json::from_str(&std::fs::read_to_string(env.state_dir().join("runs.json")).unwrap())
            .unwrap();
    assert_eq!(runs["runs"][0]["repo"], root.display().to_string());
}

async fn agent_finish_posts_commits(kind: Layout) {
    let env = Env::new().await;
    env.login_seed();
    let started = chrono::DateTime::from_timestamp(T0, 0)
        .unwrap()
        .to_rfc3339();
    let ended = chrono::DateTime::from_timestamp(T0 + 600, 0)
        .unwrap()
        .to_rfc3339();
    mock_post(
        &env,
        "POST",
        "/api/agent-work/41/finish",
        run_view(json!({"status": "succeeded", "started_at": started, "ended_at": ended})),
    )
    .await;
    mock_post(
        &env,
        "POST",
        "/api/agent-work/41/commits",
        run_view(json!({})),
    )
    .await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    git(&repo, &["config", "user.email", ME]);
    let (root, cwd) = layout(&repo, kind);
    let sha = signed_looking_commit(&root, T0 + 60, ME);
    let out = env
        .tmgr_in(&cwd, None)
        .args([
            "agent",
            "finish",
            "--run",
            "41",
            "--status",
            "succeeded",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let posts = env.requests_to("/api/agent-work/41/commits").await;
    assert_eq!(posts.len(), 1);
    let body: Value = serde_json::from_slice(&posts[0].body).unwrap();
    assert_eq!(body["commits"][0]["sha"], sha.as_str());
}

async fn start_switches_branch(kind: Layout) {
    let (env, repo) = start_env().await;
    let (root, cwd) = layout(&repo, kind);
    std::fs::write(root.join("README.md"), "changed\n").unwrap();
    let out = env
        .tmgr_in(&cwd, None)
        .args(["start", "TM-512", "--stash", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(
        git(&root, &["branch", "--show-current"]),
        "tm-512-checkout-apple-pay-button"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn hook_stop_posts_new_commits_from_a_linked_worktree() {
    hook_stop_posts_new_commits(Layout::Linked).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn hook_stop_posts_new_commits_from_a_subdirectory() {
    hook_stop_posts_new_commits(Layout::Subdir).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn hook_stop_posts_new_commits_with_per_worktree_config() {
    hook_stop_posts_new_commits(Layout::PerWorktreeConfig).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn hook_stop_posts_new_commits_from_a_linked_worktree_with_per_worktree_config() {
    hook_stop_posts_new_commits(Layout::LinkedPerWorktreeConfig).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_start_records_the_root_of_a_linked_worktree() {
    agent_start_records_root(Layout::Linked).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_start_records_the_root_from_a_subdirectory() {
    agent_start_records_root(Layout::Subdir).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_start_records_the_root_with_per_worktree_config() {
    agent_start_records_root(Layout::PerWorktreeConfig).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_start_records_the_root_of_a_linked_worktree_with_per_worktree_config() {
    agent_start_records_root(Layout::LinkedPerWorktreeConfig).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_finish_posts_commits_from_a_linked_worktree() {
    agent_finish_posts_commits(Layout::Linked).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_finish_posts_commits_from_a_subdirectory() {
    agent_finish_posts_commits(Layout::Subdir).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_finish_posts_commits_with_per_worktree_config() {
    agent_finish_posts_commits(Layout::PerWorktreeConfig).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_finish_posts_commits_from_a_linked_worktree_with_per_worktree_config() {
    agent_finish_posts_commits(Layout::LinkedPerWorktreeConfig).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn start_switches_the_branch_of_a_linked_worktree() {
    start_switches_branch(Layout::Linked).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn start_switches_the_branch_from_a_subdirectory() {
    start_switches_branch(Layout::Subdir).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn start_switches_the_branch_with_per_worktree_config() {
    start_switches_branch(Layout::PerWorktreeConfig).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn start_switches_the_branch_of_a_linked_worktree_with_per_worktree_config() {
    start_switches_branch(Layout::LinkedPerWorktreeConfig).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn start_in_a_linked_worktree_refuses_filters_from_the_common_config() {
    let (env, repo) = start_env().await;
    let (script, marker) = marker_script(&env.tmp.path().join("bin"), "clean");
    git(
        &repo,
        &["config", "filter.x.clean", script.to_str().unwrap()],
    );
    std::fs::write(repo.join(".git/info/attributes"), "README.md filter=x\n").unwrap();
    let (root, cwd) = layout(&repo, Layout::Linked);
    std::fs::write(root.join("README.md"), "changed\n").unwrap();
    let out = env
        .tmgr_in(&cwd, None)
        .args(["start", "TM-512", "--stash", "--json"])
        .output()
        .unwrap();
    assert!(!marker.exists(), "a program from the repository config ran");
    assert_eq!(out.status.code(), Some(6), "{}", text(&out.stderr));
    assert_eq!(git(&root, &["branch", "--show-current"]), "tm-512-x");
    assert_eq!(git(&root, &["stash", "list"]), "");
    server_untouched(&env).await;
}
