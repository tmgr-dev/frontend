fn unsafe_char(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
}

pub fn sanitize(input: &str) -> String {
    input
        .chars()
        .filter(|c| !unsafe_char(*c) || *c == '\n' || *c == '\t')
        .collect()
}

pub fn sanitize_line(input: &str) -> String {
    input
        .chars()
        .map(|c| if unsafe_char(c) { ' ' } else { c })
        .collect()
}

pub fn truncate_chars(input: &str, max: usize) -> (String, bool) {
    match input.char_indices().nth(max) {
        Some((idx, _)) => (input[..idx].to_string(), true),
        None => (input.to_string(), false),
    }
}

pub fn truncate_bytes(input: &str, max: usize) -> (String, bool) {
    if input.len() <= max {
        return (input.to_string(), false);
    }
    let mut end = max;
    while !input.is_char_boundary(end) {
        end -= 1;
    }
    (input[..end].to_string(), true)
}

pub fn parse_size(input: &str) -> Option<usize> {
    let s = input.trim().to_ascii_lowercase();
    let (num, mult) = match s.strip_suffix('k') {
        Some(n) => (n, 1024),
        None => (s.as_str(), 1),
    };
    num.parse::<usize>().ok()?.checked_mul(mult)
}

pub fn format_duration(seconds: i64) -> String {
    let s = seconds.max(0);
    let (h, m) = (s / 3600, (s % 3600) / 60);
    if h > 0 {
        format!("{h}h {m:02}m")
    } else if m > 0 {
        format!("{m}m")
    } else {
        format!("{s}s")
    }
}

pub fn markdown_to_text(md: &str) -> String {
    let mut out = Vec::new();
    let mut in_fence = false;
    for line in sanitize(md).lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            out.push(format!("    {line}"));
            continue;
        }
        let mut l = if trimmed.starts_with('#') {
            trimmed.trim_start_matches('#').trim_start().to_string()
        } else if let Some(rest) = trimmed.strip_prefix("> ") {
            rest.to_string()
        } else {
            line.to_string()
        };
        l = strip_links(&l);
        l = l.replace("**", "").replace("__", "").replace('`', "");
        out.push(l);
    }
    out.join("\n")
}

fn strip_links(line: &str) -> String {
    let mut out = String::new();
    let mut rest = line;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find("](") else { break };
        let Some(end) = after[close + 2..].find(')') else {
            break;
        };
        let text = &after[..close];
        let url = &after[close + 2..close + 2 + end];
        let is_image = rest[..open].ends_with('!');
        out.push_str(if is_image {
            &rest[..open - 1]
        } else {
            &rest[..open]
        });
        if is_image || text == url || text.is_empty() {
            out.push_str(if text.is_empty() { url } else { text });
        } else {
            out.push_str(&format!("{text} ({url})"));
        }
        rest = &after[close + 2 + end + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_escape_sequences_but_keeps_newlines() {
        assert_eq!(sanitize("a\u{1b}[31mb\nc\td\u{7}"), "a[31mb\nc\td");
        assert_eq!(sanitize_line("a\nb\u{1b}c"), "a b c");
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        assert_eq!(truncate_chars("héllo", 2), ("hé".to_string(), true));
        assert_eq!(truncate_chars("hi", 5), ("hi".to_string(), false));
        assert_eq!(truncate_bytes("héllo", 2), ("h".to_string(), true));
    }

    #[test]
    fn sizes_and_durations() {
        assert_eq!(parse_size("64k"), Some(65536));
        assert_eq!(parse_size("500"), Some(500));
        assert_eq!(parse_size("x"), None);
        assert_eq!(format_duration(4320), "1h 12m");
        assert_eq!(format_duration(125), "2m");
        assert_eq!(format_duration(5), "5s");
    }

    #[test]
    fn markdown_becomes_plain_text() {
        let md = "# Title\nSome **bold** and `code` with [a link](https://x.y).\n```\nlet a = 1;\n```\n> quote";
        let text = markdown_to_text(md);
        assert_eq!(
            text,
            "Title\nSome bold and code with a link (https://x.y).\n    let a = 1;\nquote"
        );
    }
}
