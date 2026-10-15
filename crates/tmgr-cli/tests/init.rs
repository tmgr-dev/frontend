mod common;
use common::*;

#[tokio::test(flavor = "multi_thread")]
async fn init_prints_a_snippet_and_touches_nothing() {
    let env = Env::new().await;
    std::fs::write(env.tmp.path().join(".zshrc"), "export A=1\n").unwrap();
    for (shell, needle) in [
        ("zsh", "tmgr prompt --format zsh"),
        ("bash", "tmgr prompt --format bash"),
        ("fish", "tmgr prompt --format fish"),
        ("starship", "[custom.tmgr]"),
        ("tmux", "#(tmgr prompt --format tmux)"),
        ("claude", "\"command\": \"tmgr statusline claude\""),
    ] {
        let out = env.cmd().args(["init", shell]).output().unwrap();
        assert!(out.status.success(), "{shell}: {}", text(&out.stderr));
        assert!(
            text(&out.stdout).contains(needle),
            "{shell}: {}",
            text(&out.stdout)
        );
    }
    assert_eq!(
        std::fs::read_to_string(env.tmp.path().join(".zshrc")).unwrap(),
        "export A=1\n"
    );
    let out = env.cmd().args(["init", "tmux", "--json"]).output().unwrap();
    assert!(stdout_json(&out)["data"]["snippet"]
        .as_str()
        .unwrap()
        .starts_with("# >>> tmgr init >>>"));
}

#[tokio::test(flavor = "multi_thread")]
async fn install_needs_a_terminal_and_leaves_the_file_alone() {
    let env = Env::new().await;
    let rc = env.tmp.path().join(".zshrc");
    std::fs::write(&rc, "export A=1\n").unwrap();
    let out = env
        .cmd()
        .args(["init", "zsh", "--install", "--yes", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10), "{}", text(&out.stdout));
    assert_eq!(std::fs::read_to_string(&rc).unwrap(), "export A=1\n");
    let leftovers: Vec<_> = std::fs::read_dir(env.tmp.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains("tmgr-backup"))
        .collect();
    assert!(leftovers.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn install_shows_the_diff_on_stderr_before_asking() {
    let env = Env::new().await;
    std::fs::write(env.tmp.path().join(".zshrc"), "export A=1\n").unwrap();
    let out = env
        .cmd()
        .args(["init", "zsh", "--install"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10));
    let err = text(&out.stderr);
    assert!(err.contains("+ # >>> tmgr init >>>"), "{err}");
    assert!(err.contains("+ # <<< tmgr init <<<"), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn claude_settings_are_never_edited() {
    let env = Env::new().await;
    let out = env
        .cmd()
        .args(["init", "claude", "--install"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(
        text(&out.stderr).contains("tmgr usage statusline install --install"),
        "{}",
        text(&out.stderr)
    );
    let out = env
        .cmd()
        .args(["init", "zsh", "--install", "--uninstall"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn an_already_installed_block_is_a_no_op_without_a_terminal() {
    let env = Env::new().await;
    let rc = env.tmp.path().join(".bashrc");
    let block = "# >>> tmgr init >>>\nif command -v tmgr >/dev/null 2>&1; then\n  __TMGR_PS1_BASE=\"$PS1\"\n  __tmgr_ps1() { PS1=\"$(tmgr prompt --format bash)${__TMGR_PS1_BASE}\"; }\n  PROMPT_COMMAND=\"__tmgr_ps1${PROMPT_COMMAND:+;$PROMPT_COMMAND}\"\nfi\n# <<< tmgr init <<<\n";
    std::fs::write(&rc, format!("export A=1\n\n{block}")).unwrap();
    let out = env
        .cmd()
        .args(["init", "bash", "--install", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(stdout_json(&out)["data"]["changed"], false);
    let out = env
        .cmd()
        .args(["init", "zsh", "--uninstall", "--json"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert_eq!(stdout_json(&out)["data"]["changed"], false);
}

#[tokio::test(flavor = "multi_thread")]
async fn init_claude_points_at_the_statusline_installer_for_limits() {
    let env = Env::new().await;
    let out = env.cmd().args(["init", "claude"]).output().unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        text(&out.stderr).contains("tmgr usage statusline install"),
        "{}",
        text(&out.stderr)
    );
    assert!(!text(&out.stdout).contains("statusline install"));
    let out = env.cmd().args(["init", "zsh"]).output().unwrap();
    assert!(!text(&out.stderr).contains("statusline install"));
    let out = env
        .cmd()
        .args(["init", "claude", "--json"])
        .output()
        .unwrap();
    let v = stdout_json(&out);
    assert!(v["data"]["note"]
        .as_str()
        .unwrap()
        .contains("tmgr usage statusline install"));
    assert!(text(&out.stderr).is_empty(), "{}", text(&out.stderr));
}
