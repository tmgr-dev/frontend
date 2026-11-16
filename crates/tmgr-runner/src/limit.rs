use regex::Regex;
use std::sync::OnceLock;

fn claude() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(concat!(
            r"(?i)^\W{0,4}(?:",
            r"(?:\d+-hour|weekly|session|opus weekly|sonnet weekly) limit reached",
            r"|claude (?:ai )?usage limit reached",
            r"|you(?:'|’)ve hit your (?:\w+ )?limit",
            r")",
        ))
        .expect("claude limit")
    })
}

fn codex() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(concat!(
            r"(?i)^\W{0,4}(?:",
            r"you(?:'|’)ve hit your usage limit",
            r"|you exceeded your current quota",
            r"|usage limit reached",
            r")",
        ))
        .expect("codex limit")
    })
}

pub fn detect(profile: &str, lines: &[String]) -> Option<String> {
    let re = match profile {
        "claude-code" => claude(),
        "codex" => codex(),
        _ => return None,
    };
    lines
        .iter()
        .rev()
        .find(|l| re.is_match(l.trim_start()))
        .map(|l| l.trim().chars().take(200).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(lines: &[&str]) -> Vec<String> {
        lines.iter().map(|l| (*l).to_string()).collect()
    }

    #[test]
    fn claude_limit_messages() {
        for line in [
            "5-hour limit reached ∙ resets 3pm",
            "  ⎿  5-hour limit reached ∙ resets 3pm",
            "Weekly limit reached ∙ resets Mon 9am",
            "Claude usage limit reached. Your limit will reset at 3pm.",
            "You've hit your limit · resets 5pm (Europe/Warsaw)",
            "You’ve hit your session limit · resets 5pm",
        ] {
            let found = detect("claude-code", &screen(&["> continue", line, ""]));
            assert!(found.is_some(), "{line}");
        }
    }

    #[test]
    fn codex_limit_messages() {
        for line in [
            "You've hit your usage limit. Upgrade to Pro or try again at 3:14 PM.",
            "■ You've hit your usage limit. Try again later.",
            "You exceeded your current quota, please check your plan.",
        ] {
            assert!(detect("codex", &screen(&[line])).is_some(), "{line}");
        }
    }

    #[test]
    fn prose_and_quotes_do_not_match() {
        for line in [
            "I added a detector for \"5-hour limit reached\" in limit.rs",
            "The error says: You've hit your limit",
            "- handle the usage limit reached case",
            "rate limit reached, retrying in 2s",
        ] {
            assert!(detect("claude-code", &screen(&[line])).is_none(), "{line}");
            assert!(detect("codex", &screen(&[line])).is_none(), "{line}");
        }
    }

    #[test]
    fn other_profiles_never_match() {
        assert!(detect("shell", &screen(&["5-hour limit reached"])).is_none());
    }

    #[test]
    fn claude_text_is_not_a_codex_limit() {
        assert!(detect("codex", &screen(&["5-hour limit reached ∙ resets 3pm"])).is_none());
    }
}
