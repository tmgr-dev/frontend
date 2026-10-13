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
