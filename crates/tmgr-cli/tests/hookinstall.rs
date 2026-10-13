mod common;
use common::*;
use serde_json::Value;

fn read_json(p: &std::path::Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}

fn commands(v: &Value, event: &str) -> Vec<String> {
    v["hooks"][event]
        .as_array()
        .map(|groups| {
            groups
                .iter()
                .flat_map(|g| g["hooks"].as_array().cloned().unwrap_or_default())
                .filter_map(|h| h["command"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn exe() -> String {
    std::fs::canonicalize(std::path::PathBuf::from(env!("CARGO_BIN_EXE_tmgr")))
        .unwrap()
        .display()
        .to_string()
}

fn same_binary(command: &str, sub: &str) -> bool {
    let (path, rest) = command.split_once(" hook claude ").unwrap();
    rest == sub
        && std::fs::canonicalize(path.trim_matches('\''))
            .unwrap()
            .display()
            .to_string()
            == exe()
}

const LEGACY: &str = r#"{
  "model": "opus",
  "hooks": {
    "Notification": [{"hooks": [{"type": "command", "command": "npx -y @tmgr/notify hook notification"}]}],
    "UserPromptSubmit": [{"hooks": [{"type": "command", "command": "npx -y @tmgr/notify hook prompt"}]}],
    "Stop": [
      {"hooks": [{"type": "command", "command": "echo mine"}]},
      {"hooks": [{"type": "command", "command": "npx -y @tmgr/notify hook stop"}]}
    ]
  }
}
"#;

#[tokio::test(flavor = "multi_thread")]
async fn print_shows_the_snippet_and_writes_nothing() {
    let env = Env::new().await;
    let out = env
        .cmd()
        .args(["hook", "install", "claude", "--print", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    let snippet = v["data"]["snippet"].as_str().unwrap();
    assert!(snippet.contains("\"Stop\"") && snippet.contains("hook claude stop"));
    assert!(!env.tmp.path().join(".claude").exists());
    let out = env
        .cmd()
        .args(["hook", "install", "codex", "--print"])
        .output()
        .unwrap();
    assert!(text(&out.stdout).starts_with("notify = ["));
    assert!(text(&out.stdout).contains("\"hook\", \"codex\", \"notify\""));
}

#[tokio::test(flavor = "multi_thread")]
async fn user_level_install_needs_a_terminal_and_ignores_yes() {
    let env = Env::new().await;
    let claude = env.tmp.path().join(".claude/settings.json");
    std::fs::create_dir_all(claude.parent().unwrap()).unwrap();
    std::fs::write(&claude, LEGACY).unwrap();
    for extra in [vec![], vec!["--yes"]] {
        let mut args = vec!["hook", "install", "claude"];
        args.extend(extra);
        let out = env.cmd().args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(10), "{}", text(&out.stderr));
        assert_eq!(std::fs::read_to_string(&claude).unwrap(), LEGACY);
    }
    let out = env
        .cmd()
        .args(["hook", "install", "codex", "--yes"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10));
    assert!(!env.tmp.path().join(".codex/config.toml").exists());
    let diff = text(
        &env.cmd()
            .args(["hook", "install", "claude"])
            .output()
            .unwrap()
            .stderr,
    );
    assert!(
        diff.contains("- ") || diff.contains("+ "),
        "the diff is shown before asking: {diff}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn project_install_migrates_backs_up_is_idempotent_and_reverts() {
    let env = Env::new().await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let settings = repo.join(".claude/settings.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    std::fs::write(&settings, LEGACY).unwrap();

    let out = env
        .tmgr_in(&repo, None)
        .args(["hook", "install", "claude", "--project", "--yes", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert_eq!(v["data"]["changed"], true);
    assert_eq!(v["warnings"][0]["code"], "project_hook_path");
    let backup = v["data"]["backup"].as_str().unwrap();
    assert!(backup.contains(".tmgr-backup-"));
    assert_eq!(std::fs::read_to_string(backup).unwrap(), LEGACY);

    let installed = read_json(&settings);
    assert_eq!(installed["model"], "opus");
    assert!(!std::fs::read_to_string(&settings).unwrap().contains("npx"));
    assert_eq!(commands(&installed, "Stop").len(), 2);
    assert_eq!(commands(&installed, "Stop")[0], "echo mine");
    assert!(same_binary(&commands(&installed, "Stop")[1], "stop"));
    assert!(same_binary(
        &commands(&installed, "Notification")[0],
        "notification"
    ));
    assert!(same_binary(
        &commands(&installed, "UserPromptSubmit")[0],
        "user-prompt-submit"
    ));

    let again = env
        .tmgr_in(&repo, None)
        .args(["hook", "install", "claude", "--project", "--yes", "--json"])
        .output()
        .unwrap();
    assert_eq!(stdout_json(&again)["data"]["changed"], false);

    let gone = env
        .tmgr_in(&repo, None)
        .args([
            "hook",
            "uninstall",
            "claude",
            "--project",
            "--yes",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(gone.status.success(), "{}", text(&gone.stderr));
    let reverted = read_json(&settings);
    assert_eq!(reverted["model"], "opus");
    assert_eq!(commands(&reverted, "Stop"), vec!["echo mine".to_string()]);
    assert!(reverted["hooks"].get("Notification").is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn broken_settings_are_refused_and_left_alone() {
    let env = Env::new().await;
    let repo = init_repo(&env.tmp.path().join("repo"));
    let settings = repo.join(".claude/settings.json");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    std::fs::write(&settings, "{ \"hooks\": ").unwrap();
    let out = env
        .tmgr_in(&repo, None)
        .args(["hook", "install", "claude", "--project", "--yes", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(std::fs::read_to_string(&settings).unwrap(), "{ \"hooks\": ");
}

#[tokio::test(flavor = "multi_thread")]
async fn codex_has_no_project_scope_and_never_overwrites_a_foreign_notify() {
    let env = Env::new().await;
    let out = env
        .cmd()
        .args(["hook", "install", "codex", "--project", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let cfg = env.tmp.path().join(".codex/config.toml");
    std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    std::fs::write(&cfg, "notify = [\"/usr/bin/other\"]\n").unwrap();
    let out = env
        .cmd()
        .args(["hook", "install", "codex", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(6), "{}", text(&out.stdout));
    assert_eq!(
        std::fs::read_to_string(&cfg).unwrap(),
        "notify = [\"/usr/bin/other\"]\n"
    );
    assert!(stdout_json(&out)["error"]["hint"]
        .as_str()
        .unwrap()
        .contains("hook"));
}

#[tokio::test(flavor = "multi_thread")]
async fn codex_migration_is_planned_as_a_diff_when_printing_uninstall() {
    let env = Env::new().await;
    let cfg = env.tmp.path().join(".codex/config.toml");
    std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    let old =
        "model = \"gpt\"\nnotify = [\"npx\", \"-y\", \"@tmgr/notify\", \"hook\", \"codex\"]\n";
    std::fs::write(&cfg, old).unwrap();
    let out = env
        .cmd()
        .args(["hook", "uninstall", "codex", "--print", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    let v = stdout_json(&out);
    assert!(v["data"]["diff"].as_str().unwrap().contains("- notify"));
    assert_eq!(std::fs::read_to_string(&cfg).unwrap(), old);
}
