use tmgr_runner::profile::{Matcher, ProfileSet};

fn bottom(screen: &str, n: usize) -> Vec<String> {
    let lines: Vec<String> = screen
        .lines()
        .map(|l| l.trim_end().to_string())
        .filter(|l| !l.is_empty())
        .collect();
    lines[lines.len().saturating_sub(n)..].to_vec()
}

fn matcher(id: &str) -> Matcher {
    let (set, _) = ProfileSet::load(None);
    Matcher::new(set.get(id).unwrap()).unwrap()
}

fn blocked_lines(id: &str, lines: &[String]) -> Option<String> {
    matcher(id)
        .check(lines)
        .filter(|(rule, _)| rule.starts_with("blocked"))
        .map(|(_, line)| line)
}

fn blocked(id: &str, screen: &str) -> Option<String> {
    blocked_lines(id, &bottom(screen, 10))
}

fn literal(id: &str, line: &str) -> bool {
    blocked_lines(id, &[line.to_string()]).is_some()
}

#[test]
fn agy_login_choice_is_blocked() {
    let line = blocked("agy", include_str!("../profiles/screens/agy-login.txt")).unwrap();
    assert!(
        line.contains("not signed in") || line.contains("login method"),
        "{line}"
    );
}

#[test]
fn agy_oauth_code_prompt_is_blocked() {
    assert!(blocked("agy", include_str!("../profiles/screens/agy-oauth.txt")).is_some());
}

#[test]
fn agy_confirmation_lines_are_blocked() {
    for line in [
        "Do you want to proceed?",
        "Allow access to this file?",
        "Allow creation of this file?",
        "Allow sandbox bypass for command execution?",
        "Requesting permission for:",
        "> Yes, I trust this folder",
    ] {
        assert!(literal("agy", line), "{line}");
    }
    assert!(!literal("agy", "Fix the login form validation"));
}

#[test]
fn opencode_idle_home_is_not_blocked() {
    assert_eq!(
        blocked(
            "opencode",
            include_str!("../profiles/screens/opencode-idle.txt")
        ),
        None
    );
}

fn rows(screen: &str, n: usize) -> Vec<String> {
    let mut lines: Vec<String> = screen.lines().map(|l| l.trim_end().to_string()).collect();
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.split_off(lines.len().saturating_sub(n))
}

#[test]
fn opencode_session_waiting_for_input_is_idle() {
    let screen = include_str!("../profiles/screens/opencode-session-idle.txt");
    let (rule, _) = matcher("opencode").check(&rows(screen, 10)).unwrap();
    assert!(rule.starts_with("idle"), "{rule}");
}

#[test]
fn opencode_recorded_permission_prompts_are_blocked() {
    for screen in [
        include_str!("../profiles/screens/opencode-permission.txt"),
        include_str!("../profiles/screens/opencode-always-allow.txt"),
    ] {
        assert!(blocked("opencode", screen).is_some(), "{screen}");
    }
}

#[test]
fn opencode_permission_lines_are_blocked() {
    for line in [
        "△ Permission required",
        "Allow once   Allow always   Reject",
    ] {
        assert!(literal("opencode", line), "{line}");
    }
}

#[test]
fn opencode_ordinary_text_is_not_blocked() {
    assert!(!literal(
        "opencode",
        "Ask anything… \"Fix a TODO in the codebase\""
    ));
}

#[test]
fn cursor_login_screen_is_not_blocked() {
    assert_eq!(
        blocked(
            "cursor-agent",
            include_str!("../profiles/screens/cursor-login.txt")
        ),
        None
    );
}

#[test]
fn cursor_prompt_lines_are_blocked() {
    for line in [
        "⚠ Workspace Trust Required",
        "[a] Trust this workspace",
        "Run (once) (y)",
        "Run outside sandbox (once) (y)",
        "Add Shell(ls) to allowlist? (tab)",
    ] {
        assert!(literal("cursor-agent", line), "{line}");
    }
}

#[test]
fn aider_gitignore_question_is_blocked() {
    let line = blocked(
        "aider",
        include_str!("../profiles/screens/aider-gitignore.txt"),
    )
    .unwrap();
    assert!(line.contains("(Y)es/(N)o"), "{line}");
}

#[test]
fn aider_login_question_is_blocked() {
    assert!(blocked("aider", include_str!("../profiles/screens/aider-login.txt")).is_some());
}

#[test]
fn aider_idle_prompt_is_not_blocked() {
    assert_eq!(
        blocked("aider", include_str!("../profiles/screens/aider-idle.txt")),
        None
    );
}

fn argv(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

#[test]
fn every_builtin_has_a_matcher_and_valid_launch_fields() {
    let (set, warnings) = ProfileSet::load(None);
    assert!(warnings.is_empty(), "{warnings:?}");
    let ids: Vec<_> = set.list().iter().map(|p| p.id.as_str()).collect();
    for id in [
        "aider",
        "claude-code",
        "codex",
        "cursor-agent",
        "agy",
        "opencode",
    ] {
        assert!(ids.contains(&id), "{id} missing from {ids:?}");
    }
    for p in set.list() {
        assert!(Matcher::new(p).is_ok(), "{}", p.id);
        assert!(!p.status.blocked.is_empty(), "{}", p.id);
        assert!(
            p.secrets.file.is_none() || p.id == "claude-code",
            "{}",
            p.id
        );
    }
}

#[test]
fn resume_argv_matches_each_builtin() {
    let (set, _) = ProfileSet::load(None);
    let resume =
        |id: &str, model: Option<&str>| set.get(id).unwrap().resume_argv(model).unwrap().unwrap();
    assert_eq!(resume("claude-code", None), argv(&["claude", "--continue"]));
    assert_eq!(
        resume("codex", Some("m1")),
        argv(&["codex", "resume", "--last", "--no-daemon", "--model", "m1"])
    );
    assert_eq!(resume("opencode", None), argv(&["opencode", "--continue"]));
    assert!(!set.list().iter().any(|p| p.id == "gemini"));
    assert_eq!(
        resume("agy", Some("m1")),
        argv(&["agy", "--model", "m1", "--continue"])
    );
    assert_eq!(
        resume("cursor-agent", None),
        argv(&["cursor-agent", "--continue"])
    );
    assert_eq!(
        resume("aider", None),
        argv(&[
            "aider",
            "--no-check-update",
            "--analytics-disable",
            "--restore-chat-history"
        ])
    );
}
