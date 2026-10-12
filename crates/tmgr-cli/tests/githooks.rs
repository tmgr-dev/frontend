mod common;
use common::*;

fn install(env: &Env, repo: &std::path::Path) -> serde_json::Value {
    let out = env
        .tmgr_in(repo, None)
        .args(["git", "install-hooks", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    stdout_json(&out)
}

fn commit(repo: &std::path::Path, msg: &str) -> String {
    std::fs::write(repo.join("f.txt"), format!("{msg}\n")).unwrap();
    git(repo, &["add", "f.txt"]);
    git(repo, &["commit", "-q", "-m", msg]);
    git(repo, &["log", "-1", "--format=%B"])
}

#[tokio::test(flavor = "multi_thread")]
async fn the_hook_appends_a_refs_trailer_in_a_real_commit() {
    let env = Env::new().await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let v = install(&env, &repo);
    assert_eq!(v["data"]["unchanged"], false);
    assert_eq!(v["data"]["chained"], false);
    git(&repo, &["switch", "-c", "tm-512-fix-login"]);
    assert_eq!(
        commit(&repo, "fix: stop redirect loop"),
        "fix: stop redirect loop\n\nRefs: TM-512"
    );
    git(&repo, &["switch", "main"]);
    assert_eq!(commit(&repo, "chore: on main"), "chore: on main");
    git(&repo, &["switch", "tm-512-fix-login"]);
    let again = commit(&repo, "fix: more\n\nRefs: TM-9");
    assert_eq!(again.matches("Refs:").count(), 1, "{again}");
    assert!(env.server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn install_chains_an_existing_hook_and_is_idempotent() {
    let env = Env::new().await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let hooks = repo.join(".git/hooks");
    let marker = env.tmp.path().join("old-hook-ran");
    write_script(
        &hooks,
        "commit-msg",
        &format!("echo \"$1\" > '{}'\nexit 0", marker.display()),
    );
    let v = install(&env, &repo);
    assert_eq!(v["data"]["chained"], true);
    let backup = v["data"]["backup"].as_str().unwrap().to_string();
    assert!(backup.ends_with("commit-msg.tmgr-prev"), "{backup}");
    assert!(std::fs::read_to_string(&backup)
        .unwrap()
        .contains("old-hook-ran"));
    git(&repo, &["switch", "-c", "tm-7-x"]);
    assert!(commit(&repo, "feat: y").ends_with("Refs: TM-7"));
    assert!(marker.exists());

    let v = install(&env, &repo);
    assert_eq!(v["data"]["unchanged"], true);
    assert!(std::fs::read_to_string(&backup)
        .unwrap()
        .contains("old-hook-ran"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_failing_previous_hook_still_blocks_the_commit() {
    let env = Env::new().await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    write_script(
        &repo.join(".git/hooks"),
        "commit-msg",
        "echo no >&2\nexit 1",
    );
    install(&env, &repo);
    std::fs::write(repo.join("f.txt"), "x\n").unwrap();
    git(&repo, &["add", "f.txt"]);
    let out = std::process::Command::new("git")
        .args(["commit", "-q", "-m", "x"])
        .current_dir(&repo)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "T")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "T")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap();
    assert!(!out.status.success());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_foreign_hook_is_not_clobbered_when_a_backup_already_exists() {
    let env = Env::new().await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let hooks = repo.join(".git/hooks");
    write_script(&hooks, "commit-msg", "exit 0");
    write_script(&hooks, "commit-msg.tmgr-prev", "exit 0");
    let out = env
        .tmgr_in(&repo, None)
        .args(["git", "install-hooks", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(6));
    assert!(!std::fs::read_to_string(hooks.join("commit-msg"))
        .unwrap()
        .contains("tmgr-managed"));
}

#[tokio::test(flavor = "multi_thread")]
async fn install_respects_a_local_core_hooks_path() {
    let env = Env::new().await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    git(&repo, &["config", "core.hooksPath", ".githooks"]);
    let v = install(&env, &repo);
    let hook = v["data"]["hook"].as_str().unwrap();
    assert_eq!(
        hook,
        repo.join(".githooks/commit-msg").display().to_string()
    );
    assert!(std::path::Path::new(hook).exists());
    assert!(!repo.join(".git/hooks/commit-msg").exists());
    git(&repo, &["switch", "-c", "tm-3-z"]);
    assert!(commit(&repo, "fix: q").ends_with("Refs: TM-3"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hooks_path_from_the_global_config_counts_as_global_and_needs_a_terminal() {
    let env = Env::new().await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let shared = env.tmp.path().join("shared-hooks");
    let gitconfig = env.tmp.path().join("gitconfig");
    std::fs::write(
        &gitconfig,
        format!("[core]\n\thooksPath = {}\n", shared.display()),
    )
    .unwrap();
    let out = env
        .tmgr_in(&repo, None)
        .env("GIT_CONFIG_GLOBAL", &gitconfig)
        .args(["git", "install-hooks", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10), "{}", text(&out.stdout));
    assert!(!shared.exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_repository_config_pointing_outside_the_repository_needs_a_terminal() {
    let env = Env::new().await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let elsewhere = env.tmp.path().join("elsewhere");
    git(
        &repo,
        &["config", "core.hooksPath", elsewhere.to_str().unwrap()],
    );
    let out = env
        .tmgr_in(&repo, None)
        .args(["git", "install-hooks", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10), "{}", text(&out.stdout));
    assert!(!elsewhere.exists());
    git(&repo, &["config", "core.hooksPath", "../escape"]);
    let out = env
        .tmgr_in(&repo, None)
        .args(["git", "install-hooks"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10));
    assert!(!env.tmp.path().join("escape").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn global_install_needs_a_terminal_even_with_yes() {
    let env = Env::new().await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let out = env
        .tmgr_in(&repo, None)
        .args(["git", "install-hooks", "--global", "--yes", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10), "{}", text(&out.stdout));
    assert_eq!(stdout_json(&out)["error"]["code"], "needs_confirmation");
    assert!(!env.config_dir().join("hooks").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn install_outside_a_repository_is_a_usage_error() {
    let env = Env::new().await;
    let plain = env.tmp.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let out = env
        .tmgr_in(&plain, None)
        .args(["git", "install-hooks"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn the_hook_command_never_fails_and_never_uses_the_network() {
    let env = Env::new().await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    git(&repo, &["switch", "-c", "tm-512-x"]);
    let missing = env
        .tmgr_in(&repo, None)
        .args(["git", "hook", "commit-msg", "/nonexistent/COMMIT_EDITMSG"])
        .output()
        .unwrap();
    assert_eq!(missing.status.code(), Some(0));
    assert!(text(&missing.stderr).contains("commit-msg hook"));

    let bin = env.tmp.path().join("msg");
    std::fs::write(&bin, [0xff, 0xfe, 0x00]).unwrap();
    let out = env
        .tmgr_in(&repo, None)
        .args(["git", "hook", "commit-msg"])
        .arg(&bin)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(std::fs::read(&bin).unwrap(), vec![0xff, 0xfe, 0x00]);

    let plain = env.tmp.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    let msg = plain.join("msg");
    std::fs::write(&msg, "subject\n").unwrap();
    let out = env
        .tmgr_in(&plain, None)
        .args(["git", "hook", "commit-msg"])
        .arg(&msg)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(std::fs::read_to_string(&msg).unwrap(), "subject\n");

    let msg = repo.join("msg");
    std::fs::write(&msg, "subject\n\n# comment\n").unwrap();
    let out = env
        .cmd_for(&dead_server())
        .current_dir(&repo)
        .env("PATH", path_with(None))
        .args(["git", "hook", "commit-msg"])
        .arg(&msg)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        std::fs::read_to_string(&msg).unwrap(),
        "subject\n\nRefs: TM-512\n\n# comment\n"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_custom_template_branch_gets_its_key_from_the_active_task() {
    let env = Env::new().await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    install(&env, &repo);
    git(&repo, &["switch", "-c", "my-weird-branch"]);
    assert_eq!(commit(&repo, "fix: a"), "fix: a");
    env.seed_active(serde_json::json!({
        "schema": "tmgr.active.v1",
        "task": {"id": 1, "key": "TM-77", "title": "x"},
        "branch": "my-weird-branch",
    }));
    std::fs::write(repo.join("g.txt"), "b\n").unwrap();
    git(&repo, &["add", "g.txt"]);
    let out = std::process::Command::new("git")
        .args(["commit", "-q", "-m", "fix: b"])
        .current_dir(&repo)
        .env("HOME", env.tmp.path())
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "T")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "T")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(git(&repo, &["log", "-1", "--format=%B"]).ends_with("Refs: TM-77"));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hooks_directory_inside_the_working_tree_is_called_out() {
    let env = Env::new().await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    git(&repo, &["config", "core.hooksPath", ".githooks"]);
    let out = env
        .tmgr_in(&repo, None)
        .args(["git", "install-hooks"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("inside the working tree"),
        "{}",
        text(&out.stderr)
    );
}
