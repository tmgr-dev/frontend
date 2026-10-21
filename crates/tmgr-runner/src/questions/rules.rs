use regex::Regex;
use std::sync::OnceLock;

pub const MAX_LEN: usize = 300;
const MIN_LEN: usize = 8;
const MAX_CANDIDATES: usize = 8;

pub fn collapse(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for c in raw.chars() {
        let c = if c.is_whitespace() { ' ' } else { c };
        if c.is_control()
            || matches!(c, '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{feff}')
        {
            continue;
        }
        if c == ' ' && (out.is_empty() || out.ends_with(' ')) {
            continue;
        }
        out.push(c);
    }
    out.truncate(out.trim_end().len());
    out
}

pub fn clip_tail(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return s.to_string();
    }
    let tail: String = s.chars().skip(n - (max - 1)).collect();
    let tail = tail.trim_start();
    format!("…{tail}")
}

pub fn clip_head(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return s.to_string();
    }
    let head: String = s.chars().take(max - 1).collect();
    format!("{}…", head.trim_end())
}

fn slot_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\s*(?:[-*]\s*)?(?:\*\*)?(?:Ответ|Answer)(?:\*\*)?\s*:(?:\*\*)?\s*(?:…|\.\.\.|_+|\[\s*\])?\s*$")
            .expect("slot pattern compiles")
    })
}

fn noise_mask(chars: &[char]) -> Vec<bool> {
    let mut mask = vec![false; chars.len()];
    let mut fenced = false;
    let mut pos = 0;
    for line in chars.split(|c| *c == '\n') {
        let end = pos + line.len();
        let text: String = line.iter().collect();
        let trimmed = text.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            fenced = !fenced;
            mask[pos..end].fill(true);
        } else if fenced || trimmed.starts_with('>') {
            mask[pos..end].fill(true);
        } else {
            mark_spans(line, &mut mask[pos..end]);
        }
        pos = end + 1;
    }
    mask
}

fn mark_spans(line: &[char], mask: &mut [bool]) {
    let pairs = [('`', '`'), ('"', '"'), ('“', '”'), ('«', '»')];
    let mut i = 0;
    while i < line.len() {
        let Some(&(_, close)) = pairs.iter().find(|(open, _)| *open == line[i]) else {
            i += 1;
            continue;
        };
        match line[i + 1..].iter().position(|c| *c == close) {
            Some(rel) => {
                mask[i..=i + 1 + rel].fill(true);
                i += rel + 2;
            }
            None => i += 1,
        }
    }
}

fn is_end(chars: &[char], i: usize) -> bool {
    matches!(chars[i], '?' | '？')
        && chars.get(i + 1).is_none_or(|n| {
            n.is_whitespace()
                || matches!(
                    n,
                    ')' | ']' | '}' | '*' | '_' | '"' | '\'' | '”' | '»' | '`'
                )
        })
}

fn strip_marks(s: &str) -> &str {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^(?:\s|[-*•#>_]|\d{1,3}[.)])+").expect("marker pattern compiles")
    });
    match re.find(s) {
        Some(m) => &s[m.end()..],
        None => s,
    }
}

fn push(out: &mut Vec<String>, raw: &str) {
    let text = collapse(strip_marks(raw).trim_end_matches(['*', '_']));
    let text = text.trim_matches(['*', '_', ' ']).to_string();
    if text.chars().count() < MIN_LEN || !text.contains(' ') {
        return;
    }
    let text = clip_tail(&text, MAX_LEN);
    if out.iter().any(|t| t.eq_ignore_ascii_case(&text)) || out.len() >= MAX_CANDIDATES {
        return;
    }
    out.push(text);
}

pub fn candidates(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let mask = noise_mask(&chars);
    let mut out = Vec::new();
    let mut start = 0;
    for i in 0..chars.len() {
        if mask[i] {
            continue;
        }
        let c = chars[i];
        let boundary = c == '\n'
            || (matches!(c, '.' | '!' | '。')
                && chars.get(i + 1).is_some_and(|n| n.is_whitespace()));
        if boundary {
            start = i + 1;
        } else if is_end(&chars, i) {
            let sentence: String = chars[start..=i].iter().collect();
            push(&mut out, &sentence);
            start = i + 1;
        }
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut offset = 0;
    for (n, line) in lines.iter().enumerate() {
        let masked = mask[offset..offset + line.chars().count()]
            .iter()
            .all(|m| *m);
        offset += line.chars().count() + 1;
        if masked || !slot_re().is_match(line) {
            continue;
        }
        if let Some(prev) = lines[..n]
            .iter()
            .rev()
            .find(|l| !l.trim().is_empty() && !slot_re().is_match(l))
        {
            push(&mut out, prev);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_question() {
        assert_eq!(
            candidates("I changed the parser. Should I also update the docs?"),
            vec!["Should I also update the docs?"]
        );
    }

    #[test]
    fn no_question_means_no_candidates() {
        assert!(candidates("All done. Tests pass.").is_empty());
        assert!(candidates("").is_empty());
    }

    #[test]
    fn code_fences_inline_code_and_quote_lines_are_skipped() {
        let t = "Fixed.\n```\nlet x = a ? b : c;\nwhat now?\n```\nThe `a ? b` form is used.\n> Did you mean this one?\nDone";
        assert!(candidates(t).is_empty(), "{:?}", candidates(t));
    }

    #[test]
    fn quoted_question_marks_do_not_count_but_the_sentence_keeps_its_quotes() {
        assert!(candidates("The user wrote \"why is it slow?\" in the issue.").is_empty());
        assert_eq!(
            candidates("Use \"foo\" or \"bar\"?"),
            vec!["Use \"foo\" or \"bar\"?"]
        );
    }

    #[test]
    fn urls_with_query_strings_are_not_questions() {
        assert!(candidates("See https://example.com/a?b=c for details").is_empty());
    }

    #[test]
    fn list_markers_and_bold_are_stripped() {
        assert_eq!(
            candidates("- **Which database do you want to use?**\n2. Do we keep the old API?"),
            vec![
                "Which database do you want to use?",
                "Do we keep the old API?"
            ]
        );
    }

    #[test]
    fn answer_slots_pick_up_the_line_before() {
        let t = "1. Which port should the server use\nОтвет: …\n\n2. Keep the old flag\nAnswer:\n";
        assert_eq!(
            candidates(t),
            vec!["Which port should the server use", "Keep the old flag"]
        );
    }

    #[test]
    fn russian_and_dedupe() {
        assert_eq!(
            candidates("Нужно ли обновить документацию? Нужно ли обновить документацию?"),
            vec!["Нужно ли обновить документацию?"]
        );
    }

    #[test]
    fn long_sentences_are_clipped_and_controls_removed() {
        let long = format!("{} really?", "word ".repeat(120));
        let got = candidates(&long);
        assert_eq!(got.len(), 1);
        assert!(got[0].chars().count() <= MAX_LEN && got[0].starts_with('…'));
        let got = candidates("Run\u{1b}[2J now \u{202e}ok?");
        assert_eq!(got, vec!["Run[2J now ok?"]);
    }

    #[test]
    fn single_words_are_not_questions() {
        assert!(candidates("Why?\nOK?").is_empty());
    }
}
