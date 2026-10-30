struct Line {
    eff: usize,
    lead: usize,
    text: String,
}

const MARKERS: [char; 18] = [
    '⏺', '⎿', '❯', '✻', '✳', '✶', '✽', '✢', '·', '●', '○', '◐', '◑', '◒', '◓', '▪', '▸', '›',
];

pub fn clean_copy(text: &str) -> String {
    let stripped = strip_ansi(text);
    let (body, trailing_newline) = match stripped.strip_suffix('\n') {
        Some(body) => (body, true),
        None => (stripped.as_str(), false),
    };
    let lines: Vec<Line> = body.split('\n').map(clean_line).collect();
    let dedent = lines
        .iter()
        .filter(|l| !l.text.is_empty())
        .map(|l| l.eff)
        .min()
        .unwrap_or(0);
    let mut out = lines
        .iter()
        .map(|l| {
            if l.text.is_empty() {
                String::new()
            } else {
                format!("{}{}", " ".repeat(l.lead.saturating_sub(dedent)), l.text)
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if trailing_newline {
        out.push('\n');
    }
    out
}

fn strip_ansi(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some('[') => {
                chars.next();
                while chars
                    .next_if(|c| ('\u{20}'..='\u{3f}').contains(c))
                    .is_some()
                {}
                chars.next_if(|c| ('\u{40}'..='\u{7e}').contains(c));
            }
            Some(']') => {
                chars.next();
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' {
                        chars.next_if_eq(&'\\');
                        break;
                    }
                }
            }
            Some(next) if next != '\n' => {
                chars.next();
            }
            _ => {}
        }
    }
    out
}

fn clean_line(line: &str) -> Line {
    let s: String = line.chars().filter(|&c| !is_zero_width(c)).collect();
    let k = s.len() - s.trim_start_matches(' ').len();
    let (pre, body) = match decoration(&s[k..]) {
        Some((bytes, chars)) => (k + chars, &s[k + bytes..]),
        None => (0, s.as_str()),
    };
    let visible: String = body.chars().filter(|&c| !is_art(c)).collect();
    let body = visible.trim_end_matches([' ', '\t', '\r']);
    let lead = body.len() - body.trim_start_matches(' ').len();
    Line {
        eff: pre + lead,
        lead,
        text: body[lead..].to_string(),
    }
}

fn decoration(rest: &str) -> Option<(usize, usize)> {
    let first = rest.chars().next()?;
    let (bytes, chars) = if is_box(first) {
        let run = rest.find(|c: char| !is_box(c)).unwrap_or(rest.len());
        (run, rest[..run].chars().count())
    } else if MARKERS.contains(&first) {
        let after = &rest[first.len_utf8()..];
        if !after.is_empty() && !after.starts_with([' ', '\t']) {
            return None;
        }
        (first.len_utf8(), 1)
    } else {
        return None;
    };
    let space = usize::from(rest[bytes..].starts_with([' ', '\t']));
    Some((bytes + space, chars + space))
}

fn is_box(c: char) -> bool {
    ('\u{2500}'..='\u{257f}').contains(&c)
}

fn is_art(c: char) -> bool {
    ('\u{2500}'..='\u{259f}').contains(&c) || ('\u{2800}'..='\u{28ff}').contains(&c)
}

fn is_zero_width(c: char) -> bool {
    matches!(c, '\u{200b}'..='\u{200d}' | '\u{feff}')
}

#[cfg(test)]
mod tests {
    use super::clean_copy;

    #[test]
    fn strips_csi_and_osc_sequences() {
        assert_eq!(clean_copy("\u{1b}[31mred\u{1b}[0m text"), "red text");
        assert_eq!(clean_copy("\u{1b}[>1u\u{1b}[200~paste\u{1b}[201~"), "paste");
        assert_eq!(clean_copy("\u{1b}]0;title\u{7}body"), "body");
        assert_eq!(clean_copy("\u{1b}]8;;http://x\u{1b}\\link"), "link");
        assert_eq!(clean_copy("ok\u{1b}"), "ok");
    }

    #[test]
    fn strips_box_frame() {
        assert_eq!(clean_copy("│ hello"), "hello");
        assert_eq!(clean_copy("│ hello │"), "hello");
        assert_eq!(clean_copy("  │ text"), "text");
        assert_eq!(clean_copy("│ a\n│ b │"), "a\nb");
    }

    #[test]
    fn border_line_becomes_empty() {
        assert_eq!(clean_copy("╭──────╮\n│ x │\n╰──────╯"), "\nx\n");
    }

    #[test]
    fn braille_spinner_keeps_space_then_dedents() {
        assert_eq!(clean_copy("⠋ Thinking"), "Thinking");
    }

    #[test]
    fn removes_zero_width_characters() {
        assert_eq!(clean_copy("a\u{200b}b\u{200c}c\u{200d}d\u{feff}"), "abcd");
    }

    #[test]
    fn trims_trailing_spaces_and_carriage_returns() {
        assert_eq!(clean_copy("hi  \t\nthere \r\n"), "hi\nthere\n");
    }

    #[test]
    fn claude_answer_loses_markers_and_frame_indent() {
        assert_eq!(
            clean_copy("⏺ First line\n  second line\n  ⎿  result\n"),
            "First line\nsecond line\nresult\n"
        );
    }

    #[test]
    fn keeps_relative_indent_after_dedent() {
        assert_eq!(clean_copy("  a\n    b\n"), "a\n  b\n");
    }

    #[test]
    fn plain_text_is_unchanged() {
        assert_eq!(
            clean_copy("plain text\nsecond line\n"),
            "plain text\nsecond line\n"
        );
        assert_eq!(clean_copy("no newline"), "no newline");
    }

    #[test]
    fn empty_and_newline_only_inputs() {
        assert_eq!(clean_copy(""), "");
        assert_eq!(clean_copy("\n"), "\n");
        assert_eq!(clean_copy("\n\n"), "\n\n");
    }

    #[test]
    fn is_idempotent_on_examples() {
        let examples = [
            "\u{1b}[31mred\u{1b}[0m text",
            "│ hello │",
            "╭──────╮\n│ x │\n╰──────╯",
            "⠋ Thinking",
            "⏺ First line\n  second line\n  ⎿  result\n",
            "  a\n    b\n",
            "plain text\n",
        ];
        for example in examples {
            let once = clean_copy(example);
            assert_eq!(clean_copy(&once), once, "input: {example:?}");
        }
    }
}
