use crate::active::{valid_key, ActiveState};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Plain,
    Zsh,
    Bash,
    Fish,
    Starship,
    Tmux,
    Claude,
}

impl Format {
    pub fn parse(name: &str) -> Option<Format> {
        Some(match name {
            "plain" => Format::Plain,
            "zsh" => Format::Zsh,
            "bash" => Format::Bash,
            "fish" => Format::Fish,
            "starship" => Format::Starship,
            "tmux" => Format::Tmux,
            "claude" => Format::Claude,
            _ => return None,
        })
    }

    pub fn is_shell_prompt(self) -> bool {
        matches!(self, Format::Zsh | Format::Bash | Format::Fish)
    }
}

pub fn compact_duration(seconds: i64) -> String {
    let s = seconds.max(0);
    let (h, m) = (s / 3600, (s % 3600) / 60);
    if h > 0 {
        format!("{h}h{m:02}m")
    } else {
        format!("{m}m")
    }
}

pub fn segment(state: &ActiveState, now: i64) -> Option<String> {
    if !valid_key(&state.task.key) {
        return None;
    }
    Some(match state.elapsed_s(now) {
        Some(s) => format!("\u{23f1} {} {}", state.task.key, compact_duration(s)),
        None => format!("\u{23f8} {}", state.task.key),
    })
}

pub fn render(state: Option<&ActiveState>, now: i64, format: Format, color: bool) -> String {
    let Some(text) = state.and_then(|s| segment(s, now)) else {
        return String::new();
    };
    let colored = color
        && matches!(
            format,
            Format::Zsh | Format::Bash | Format::Fish | Format::Tmux
        );
    let escaped = match format {
        Format::Zsh => text.replace('%', "%%"),
        Format::Tmux => text.replace('#', "##"),
        _ => text,
    };
    let body = if colored {
        match format {
            Format::Zsh => format!("%{{\x1b[38;5;203m%}}{escaped}%{{\x1b[0m%}}"),
            Format::Bash => format!("\\[\x1b[38;5;203m\\]{escaped}\\[\x1b[0m\\]"),
            Format::Fish => format!("\x1b[38;5;203m{escaped}\x1b[0m"),
            _ => format!("#[fg=colour203]{escaped}#[default]"),
        }
    } else {
        escaped
    };
    if format.is_shell_prompt() {
        format!("{body} ")
    } else {
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::active::ActiveTask;

    fn state(running: bool) -> ActiveState {
        let mut s = ActiveState::new(ActiveTask {
            id: 1,
            key: "TM-509".into(),
            title: "t".into(),
            url: None,
        });
        if running {
            s.timer_started_at = Some("2026-10-12T09:00:00Z".into());
        }
        s
    }

    const T0: i64 = 1_791_795_600;

    #[test]
    fn durations_are_compact() {
        assert_eq!(compact_duration(0), "0m");
        assert_eq!(compact_duration(59), "0m");
        assert_eq!(compact_duration(300), "5m");
        assert_eq!(compact_duration(4320), "1h12m");
        assert_eq!(compact_duration(3600), "1h00m");
        assert_eq!(compact_duration(-5), "0m");
    }

    #[test]
    fn plain_running_and_stopped() {
        let s = state(true);
        assert_eq!(
            render(Some(&s), T0 + 4320, Format::Plain, true),
            "\u{23f1} TM-509 1h12m"
        );
        let s = state(false);
        assert_eq!(render(Some(&s), T0, Format::Plain, true), "\u{23f8} TM-509");
        assert_eq!(render(None, T0, Format::Plain, true), "");
    }

    #[test]
    fn shells_wrap_colour_codes_so_the_cursor_math_stays_right() {
        let s = state(true);
        let zsh = render(Some(&s), T0 + 300, Format::Zsh, true);
        assert_eq!(zsh, "%{\x1b[38;5;203m%}\u{23f1} TM-509 5m%{\x1b[0m%} ");
        let bash = render(Some(&s), T0 + 300, Format::Bash, true);
        assert_eq!(bash, "\\[\x1b[38;5;203m\\]\u{23f1} TM-509 5m\\[\x1b[0m\\] ");
        let fish = render(Some(&s), T0 + 300, Format::Fish, true);
        assert_eq!(fish, "\x1b[38;5;203m\u{23f1} TM-509 5m\x1b[0m ");
        let tmux = render(Some(&s), T0 + 300, Format::Tmux, true);
        assert_eq!(tmux, "#[fg=colour203]\u{23f1} TM-509 5m#[default]");
    }

    #[test]
    fn no_colour_means_no_escape_sequences_anywhere() {
        let s = state(true);
        for f in [Format::Zsh, Format::Bash, Format::Fish, Format::Tmux] {
            let out = render(Some(&s), T0 + 60, f, false);
            assert!(
                !out.contains('\x1b')
                    && !out.contains("%{")
                    && !out.contains("\\[")
                    && !out.contains("#["),
                "{out:?}"
            );
        }
        for f in [Format::Starship, Format::Claude, Format::Plain] {
            assert!(!render(Some(&s), T0, f, true).contains('\x1b'));
        }
    }

    #[test]
    fn hash_ids_are_escaped_for_tmux_and_percent_for_zsh() {
        let mut s = state(false);
        s.task.key = "#8974".into();
        assert_eq!(render(Some(&s), T0, Format::Tmux, false), "\u{23f8} ##8974");
        assert_eq!(render(Some(&s), T0, Format::Plain, false), "\u{23f8} #8974");
    }

    #[test]
    fn hostile_keys_render_nothing() {
        let mut s = state(true);
        s.task.key = "TM-1\u{1b}[31m".into();
        assert_eq!(render(Some(&s), T0, Format::Plain, true), "");
    }

    #[test]
    fn shell_prompts_end_with_a_space_others_do_not() {
        let s = state(false);
        assert!(render(Some(&s), T0, Format::Zsh, false).ends_with(' '));
        assert!(!render(Some(&s), T0, Format::Starship, false).ends_with(' '));
    }

    #[test]
    fn formats_parse_by_name() {
        assert_eq!(Format::parse("starship"), Some(Format::Starship));
        assert_eq!(Format::parse("nope"), None);
    }
}
