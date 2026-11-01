use unicode_width::UnicodeWidthStr;

const WRAP_MARGIN: usize = 12;

struct Line {
    eff: usize,
    col: usize,
    marker: bool,
    fragment: bool,
    ruled: bool,
    indent: usize,
    item: Option<usize>,
    text: String,
}

const MARKERS: [char; 18] = [
    '⏺', '⎿', '❯', '✻', '✳', '✶', '✽', '✢', '·', '●', '○', '◐', '◑', '◒', '◓', '▪', '▸', '›',
];

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CopyContext {
    pub lead: String,
    pub cols: usize,
}

pub fn clean_copy(text: &str) -> String {
    clean_selection(text, &CopyContext::default())
}

pub fn clean_selection(text: &str, ctx: &CopyContext) -> String {
    let stripped = strip_ansi(text);
    let (body, trailing_newline) = match stripped.strip_suffix('\n') {
        Some(body) => (body, true),
        None => (stripped.as_str(), false),
    };
    let lead = clean_line(&ctx.lead);
    let lead_is_decoration = lead.text.is_empty();
    let source = if lead_is_decoration {
        let gap = if ctx.lead.is_empty() || ctx.lead.ends_with([' ', '\t']) {
            ""
        } else {
            " "
        };
        format!("{}{gap}{body}", ctx.lead)
    } else {
        body.to_string()
    };
    let mut lines: Vec<Line> = source.split('\n').map(clean_line).collect();
    if !lead_is_decoration {
        if let Some(first) = lines.first_mut() {
            first.fragment = true;
            first.col += ctx.lead.width();
            first.indent = lead.col;
        }
    }
    mark_list_items(&mut lines, &lead);
    let dedent = lines
        .iter()
        .filter(|l| !l.text.is_empty() && !l.fragment)
        .map(|l| l.eff)
        .min()
        .unwrap_or(0);
    let wrap_width = wrap_width(&lines, ctx.cols);
    let mut out = String::new();
    for (i, l) in lines.iter().enumerate() {
        if i > 0 {
            let joined =
                wrap_width.is_some_and(|w| continues(&lines[i - 1], l, w, ctx.cols, dedent));
            out.push(if joined { ' ' } else { '\n' });
            if joined {
                out.push_str(&l.text);
                continue;
            }
        }
        if !l.text.is_empty() {
            if !l.marker && !l.fragment {
                out.push_str(&" ".repeat(l.eff.saturating_sub(dedent)));
            }
            out.push_str(&l.text);
        }
    }
    if trailing_newline {
        out.push('\n');
    }
    out
}

fn row_width(l: &Line) -> usize {
    l.col + l.text.width()
}

fn wrap_width(lines: &[Line], cols: usize) -> Option<usize> {
    lines
        .iter()
        .map(row_width)
        .filter(|w| *w <= cols)
        .max()
        .filter(|w| w + WRAP_MARGIN >= cols)
}

fn mark_list_items(lines: &mut [Line], lead: &Line) {
    let mut item = None;
    for l in lines.iter_mut() {
        let prefix = if l.fragment {
            list_prefix(&lead.text).map(|p| lead.col + p)
        } else {
            list_prefix(&l.text).map(|p| l.col + p)
        };
        item = prefix.or(item.filter(|hang| *hang == l.col));
        l.item = item;
    }
}

fn continues(a: &Line, b: &Line, wrap_width: usize, cols: usize, dedent: usize) -> bool {
    if a.text.is_empty() || b.text.is_empty() || b.marker || row_width(a) > cols {
        return false;
    }
    if a.ruled || b.ruled {
        return false;
    }
    if a.text.ends_with([';', '{', '}', ':', '\\']) || starts_a_block(&b.text) {
        return false;
    }
    if !a.fragment
        && (!a.text.contains(' ') || a.text.starts_with('#') || a.text.starts_with("```"))
    {
        return false;
    }
    let body = b.col == a.indent && a.indent <= dedent;
    if !body && a.item != Some(b.col) {
        return false;
    }
    let first_a = a.text.split(' ').next().unwrap_or("");
    let first_b = b.text.split(' ').next().unwrap_or("");
    if first_a == first_b || starts_a_record(first_b) {
        return false;
    }
    row_width(a) + 1 + first_b.width() > wrap_width
}

fn starts_a_record(word: &str) -> bool {
    let stamp = word.starts_with(|c: char| c.is_ascii_digit()) && word.contains([':', '-']);
    stamp || word.starts_with(['/', '~', '$', '-', '[']) || word.starts_with("./")
}

fn starts_a_block(text: &str) -> bool {
    list_prefix(text).is_some()
        || text.starts_with(['#', '>', '|', '}', ')', ']'])
        || text.starts_with("```")
}

fn list_prefix(text: &str) -> Option<usize> {
    let digits = text.chars().take_while(char::is_ascii_digit).count();
    let marker = if digits > 0 {
        text[digits..].starts_with(['.', ')']).then_some(digits + 1)
    } else {
        text.starts_with(['-', '*', '+', '•']).then_some(1)
    }?;
    let after = text.chars().nth(marker)?;
    (after == ' ').then_some(marker + 1)
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
    let (pre, body, marker) = match decoration(&s[k..]) {
        Some((bytes, chars, marker)) => (k + chars, &s[k + bytes..], marker),
        None => (0, s.as_str(), false),
    };
    let visible: String = body.chars().filter(|&c| !is_art(c)).collect();
    let body = visible.trim_end_matches([' ', '\t', '\r']);
    let lead = body.len() - body.trim_start_matches(' ').len();
    let col = pre + lead;
    Line {
        eff: if marker { pre } else { col },
        col,
        marker,
        fragment: false,
        ruled: s.chars().any(is_box),
        indent: col,
        item: None,
        text: body[lead..].to_string(),
    }
}

fn decoration(rest: &str) -> Option<(usize, usize, bool)> {
    let first = rest.chars().next()?;
    let (bytes, chars, marker) = if is_box(first) {
        let run = rest.find(|c: char| !is_box(c)).unwrap_or(rest.len());
        (run, rest[..run].chars().count(), false)
    } else if MARKERS.contains(&first) {
        let after = &rest[first.len_utf8()..];
        if !after.is_empty() && !after.starts_with([' ', '\t']) {
            return None;
        }
        (first.len_utf8(), 1, true)
    } else {
        return None;
    };
    let space = usize::from(rest[bytes..].starts_with([' ', '\t']));
    Some((bytes + space, chars + space, marker))
}

fn is_box(c: char) -> bool {
    ('\u{2500}'..='\u{257f}').contains(&c)
}

fn is_art(c: char) -> bool {
    ('\u{2500}'..='\u{259f}').contains(&c) || ('\u{2800}'..='\u{28ff}').contains(&c)
}

fn is_zero_width(c: char) -> bool {
    matches!(
        c,
        '\u{200b}'..='\u{200d}' | '\u{feff}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'
    )
}

#[cfg(test)]
mod tests {
    use super::{clean_copy, clean_selection, CopyContext};

    const ANSWER: &str = "TMGR.dev: таск-менеджер (tmgr.dev) с воркспейсами,\n  задачами, страницами и агентской работой.\n  Agent-команды репортят в него прогресс через MCP.\n  Описание собрано из CLAUDE.md и memory, код не\n  открывал.";
    const ANSWER_LINE: &str = "TMGR.dev: таск-менеджер (tmgr.dev) с воркспейсами, задачами, страницами и агентской работой. Agent-команды репортят в него прогресс через MCP. Описание собрано из CLAUDE.md и memory, код не открывал.";

    fn in_pane(text: &str, lead: &str, cols: usize) -> String {
        let ctx = CopyContext {
            lead: lead.into(),
            cols,
        };
        clean_selection(text, &ctx)
    }

    #[test]
    fn rows_that_only_look_wrapped_keep_their_breaks() {
        for (text, cols) in [
            ("docker exec a b c d\ndocker exec e", 20),
            ("curl https://example.com/x \\\n-H 'Accept: json'", 30),
            ("12:00:01 INFO started the worker\n12:00:02 INFO done", 34),
            ("src/a.rs was changed in this commit\n/tmp/other.rs", 36),
            ("## A long heading about the copy\nNext line here", 34),
            ("Do the following steps right now:\nopen the file", 34),
            ("https://example.com/very/long/pat\nh/here", 33),
            (
                "Intro\n    result = compute_the_thing(a)\n    value = compute_other(b)",
                34,
            ),
        ] {
            assert_eq!(in_pane(text, "", cols), text);
        }
    }

    #[test]
    fn a_box_table_keeps_one_row_per_line() {
        let out = in_pane("│ name     │ type   │\n│ user_id  │ bigint │", "", 24);
        assert_eq!(out.lines().count(), 2);
    }

    #[test]
    fn an_indented_block_never_joins_a_partial_first_line() {
        assert_eq!(
            in_pane("please do this now\n    cargo build --release", "Run ", 26),
            "please do this now\ncargo build --release"
        );
    }

    #[test]
    fn a_marker_before_the_selection_never_enters_the_copy() {
        assert_eq!(in_pane("foo bar", "·", 10), "foo bar");
    }

    #[test]
    fn a_wrapped_list_item_after_a_partial_first_line_still_joins() {
        assert_eq!(
            in_pane(
                "first part of item\n  second part of it\n  third",
                "- x ",
                22
            ),
            "first part of item second part of it third"
        );
    }

    #[test]
    fn an_agent_paragraph_selected_after_its_marker_becomes_one_line() {
        assert_eq!(in_pane(ANSWER, "⏺ ", 58), ANSWER_LINE);
    }

    #[test]
    fn an_agent_paragraph_selected_with_its_marker_becomes_one_line() {
        assert_eq!(in_pane(&format!("⏺ {ANSWER}"), "", 58), ANSWER_LINE);
    }

    #[test]
    fn a_selection_from_the_middle_of_a_row_loses_the_indent_of_the_next_rows() {
        let text = "таск-менеджер (tmgr.dev) с воркспейсами,\n  задачами, страницами и агентской работой.\n    вложено";
        assert_eq!(
            in_pane(text, "TMGR.dev: ", 200),
            "таск-менеджер (tmgr.dev) с воркспейсами,\nзадачами, страницами и агентской работой.\n  вложено"
        );
    }

    #[test]
    fn a_wrapped_list_item_is_joined_and_the_next_item_stays_apart() {
        let text = "- первый пункт списка довольно длинный и он\n  переносится на вторую строку\n- второй пункт\n1. третий";
        assert_eq!(
            in_pane(text, "", 50),
            "- первый пункт списка довольно длинный и он переносится на вторую строку\n- второй пункт\n1. третий"
        );
    }

    #[test]
    fn a_blank_line_still_separates_paragraphs() {
        let text = "первая строка абзаца довольно длинная вот\nтак\n\nследующий абзац";
        assert_eq!(
            in_pane(text, "", 48),
            "первая строка абзаца довольно длинная вот так\n\nследующий абзац"
        );
    }

    #[test]
    fn short_lines_keep_their_breaks() {
        let text = "git status\ngit diff\nlet result = compute_something_rather_long(input);";
        assert_eq!(in_pane(text, "", 56), text);
    }

    #[test]
    fn long_code_lines_keep_their_breaks() {
        let text = "let first_value = compute_the_first_thing(input);\nlet second_value = compute_the_second(input);\nif second_value > first_value {\n    return;\n}";
        assert_eq!(in_pane(text, "", 52), text);
    }

    #[test]
    fn text_much_narrower_than_the_pane_keeps_its_breaks() {
        assert_eq!(
            in_pane("foo bar\nbazqux quux", "", 100),
            "foo bar\nbazqux quux"
        );
    }

    #[test]
    fn a_line_longer_than_the_pane_was_wrapped_by_the_terminal_and_stays_apart() {
        let long = "слово ".repeat(20);
        let text = format!("{}\nследующая строка команды", long.trim_end());
        assert_eq!(in_pane(&text, "", 60), text);
    }

    #[test]
    fn an_unknown_pane_width_never_joins_lines() {
        assert_eq!(in_pane("a b\nc", "", 0), "a b\nc");
    }

    #[test]
    fn a_trailing_newline_is_kept_after_joining() {
        let text = "первая строка абзаца довольно длинная вот\nтак\n";
        assert_eq!(
            in_pane(text, "", 48),
            "первая строка абзаца довольно длинная вот так\n"
        );
    }

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

    #[test]
    fn code_indent_inside_a_frame_is_kept() {
        assert_eq!(
            clean_copy("\u{2502} def f():\n\u{2502}     return 1"),
            "def f():\n    return 1"
        );
    }

    #[test]
    fn bidi_overrides_are_removed() {
        assert_eq!(clean_copy("a\u{202e}b"), "ab");
    }
}
