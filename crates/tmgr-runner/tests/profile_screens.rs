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

fn blocked(id: &str, screen: &str) -> Option<String> {
    matcher(id)
        .check(&bottom(screen, 10))
        .filter(|(rule, _)| rule.starts_with("blocked"))
        .map(|(_, line)| line)
}

#[test]
fn codex_approval_prompt_is_blocked() {
    let line = blocked(
        "codex",
        include_str!("../profiles/screens/codex-approval.txt"),
    )
    .unwrap();
    assert!(
        line.contains("Allow running the requested command"),
        "{line}"
    );
}

#[test]
fn codex_approval_with_reworded_reason_is_blocked() {
    let screen = include_str!("../profiles/screens/codex-approval-reworded.txt");
    assert!(blocked("codex", screen).is_some());
}

#[test]
fn codex_folder_trust_is_blocked() {
    assert!(blocked("codex", include_str!("../profiles/screens/codex-trust.txt")).is_some());
}

#[test]
fn codex_idle_prompt_is_not_blocked() {
    assert_eq!(
        blocked("codex", include_str!("../profiles/screens/codex-idle.txt")),
        None
    );
}

#[test]
fn claude_folder_trust_is_blocked() {
    assert!(blocked(
        "claude-code",
        include_str!("../profiles/screens/claude-trust.txt")
    )
    .is_some());
}

#[test]
fn claude_idle_prompt_is_not_blocked() {
    assert_eq!(
        blocked(
            "claude-code",
            include_str!("../profiles/screens/claude-idle.txt")
        ),
        None
    );
}

fn idle(id: &str, screen: &str) -> bool {
    matcher(id)
        .check(&bottom(screen, 10))
        .is_some_and(|(rule, _)| rule.starts_with("idle"))
}

#[test]
fn claude_empty_prompt_is_idle_in_auto_and_manual_mode() {
    assert!(idle(
        "claude-code",
        include_str!("../profiles/screens/claude-idle.txt")
    ));
    assert!(idle(
        "claude-code",
        include_str!("../profiles/screens/claude-idle-manual.txt")
    ));
}

#[test]
fn claude_prompt_with_text_or_a_menu_is_not_idle() {
    assert!(!idle("claude-code", "❯ fix the failing test\n"));
    assert!(!idle(
        "claude-code",
        " ❯ No, exit\n   Yes, I trust this folder\n"
    ));
}

#[test]
fn codex_empty_prompt_is_idle() {
    assert!(idle(
        "codex",
        include_str!("../profiles/screens/codex-idle.txt")
    ));
}
