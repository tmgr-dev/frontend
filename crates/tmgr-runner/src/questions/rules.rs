use regex::Regex;
use std::sync::OnceLock;

pub const MAX_LEN: usize = 300;
const MIN_LEN: usize = 8;
const MAX_CANDIDATES: usize = 8;
pub const MAX_ITEM: usize = 600;

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

fn label_of(raw: &str) -> Option<String> {
    let text = collapse(strip_marks(raw).trim_end_matches(['*', '_']));
    let text = text.trim_matches(['*', '_', ' ']).to_string();
    if text.chars().count() < MIN_LEN || !text.contains(' ') {
        return None;
    }
    Some(clip_tail(&text, MAX_LEN))
}

fn marker_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^(\s*)(?:[-*•]|\d{1,3}[.)])\s+\S").expect("marker compiles"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub pos: usize,
    pub label: String,
    pub item: String,
}

struct Block {
    pos: usize,
    text: String,
    mask: Vec<bool>,
}

fn blocks(chars: &[char], mask: &[bool]) -> Vec<Block> {
    struct Line {
        start: usize,
        end: usize,
        indent: usize,
        blank: bool,
        marker: bool,
    }
    let mut lines = Vec::new();
    let mut pos = 0;
    for line in chars.split(|c| *c == '\n') {
        let end = pos + line.len();
        let text: String = line.iter().collect();
        let blank = text.trim().is_empty();
        let masked = mask[pos..end].iter().all(|m| *m) && !blank;
        let marker = !blank && !masked && marker_re().is_match(&text);
        lines.push(Line {
            start: pos,
            end,
            indent: text.chars().take_while(|c| c.is_whitespace()).count(),
            blank,
            marker,
        });
        pos = end + 1;
    }
    let mut spans: Vec<(usize, usize, bool, usize)> = Vec::new();
    let mut open: Option<usize> = None;
    let mut after_blank = false;
    for line in &lines {
        if line.blank {
            open = None;
            after_blank = true;
            continue;
        }
        let list_indent = |k: usize| spans[k].3;
        match open {
            Some(k) if line.marker && line.indent > list_indent(k) && spans[k].2 => {
                spans[k].1 = line.end;
            }
            Some(k) if !line.marker => spans[k].1 = line.end,
            _ => {
                let prev = spans.len().checked_sub(1);
                match prev {
                    Some(k)
                        if after_blank
                            && !line.marker
                            && spans[k].2
                            && line.indent > spans[k].3 =>
                    {
                        spans[k].1 = line.end;
                        open = Some(k);
                    }
                    _ => {
                        spans.push((line.start, line.end, line.marker, line.indent));
                        open = Some(spans.len() - 1);
                    }
                }
            }
        }
        after_blank = false;
    }
    spans
        .into_iter()
        .map(|(a, b, ..)| Block {
            pos: a,
            text: chars[a..b].iter().collect(),
            mask: mask[a..b].to_vec(),
        })
        .collect()
}

fn bold_lead(block: &str) -> Option<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^\s*(?:(?:[-*•]|\d{1,3}[.)])\s+)?").expect("lead pattern compiles")
    });
    let rest = &block[re.find(block).map_or(0, |m| m.end())..];
    let rest = rest.strip_prefix("**")?;
    let end = rest.find("**")?;
    label_of(rest[..end].trim_end_matches(':'))
}

fn question_sentences(b: &Block) -> Vec<String> {
    let chars: Vec<char> = b.text.chars().collect();
    let mut out = Vec::new();
    let mut start = 0;
    for i in 0..chars.len() {
        if b.mask[i] {
            continue;
        }
        let c = chars[i];
        let boundary = c == '\n'
            || (matches!(c, '.' | '!' | '。')
                && chars.get(i + 1).is_some_and(|n| n.is_whitespace()));
        if boundary {
            start = i + 1;
        } else if is_end(&chars, i) {
            out.push(chars[start..=i].iter().collect());
            start = i + 1;
        }
    }
    out
}

fn has_slot(b: &Block) -> bool {
    let mut at = 0;
    for line in b.text.split('\n') {
        let n = line.chars().count();
        let masked = n > 0 && b.mask[at..at + n].iter().all(|m| *m);
        at += n + 1;
        if !masked && slot_re().is_match(line) {
            return true;
        }
    }
    false
}

fn group_of(b: &Block, need_question: bool) -> Option<Group> {
    let sentences = question_sentences(b);
    let slot = has_slot(b);
    if need_question && sentences.is_empty() && !slot {
        return None;
    }
    let label = bold_lead(&b.text)
        .or_else(|| sentences.iter().find_map(|s| label_of(s)))
        .or_else(|| {
            b.text
                .lines()
                .find(|l| !l.trim().is_empty() && !slot_re().is_match(l))
                .and_then(label_of)
        })?;
    let body: Vec<&str> = b.text.lines().filter(|l| !slot_re().is_match(l)).collect();
    let item = collapse(strip_marks(&body.join("\n")))
        .replace("**", "")
        .replace('`', "");
    let item = clip_head(&item, MAX_ITEM);
    let item = if item == label { String::new() } else { item };
    Some(Group {
        pos: b.pos,
        label,
        item,
    })
}

fn dedupe(groups: Vec<Group>) -> Vec<Group> {
    let mut out: Vec<Group> = Vec::new();
    for g in groups {
        if out.len() < MAX_CANDIDATES && !out.iter().any(|o| o.label.eq_ignore_ascii_case(&g.label))
        {
            out.push(g);
        }
    }
    out
}

pub fn groups(text: &str) -> Vec<Group> {
    let chars: Vec<char> = text.chars().collect();
    let mask = noise_mask(&chars);
    dedupe(
        blocks(&chars, &mask)
            .iter()
            .filter_map(|b| group_of(b, true))
            .collect(),
    )
}

pub fn groups_for_spans(text: &str, spans: &[String]) -> Vec<Group> {
    let chars: Vec<char> = text.chars().collect();
    let mask = noise_mask(&chars);
    let all = blocks(&chars, &mask);
    let norm: Vec<String> = all
        .iter()
        .map(|b| collapse(&b.text).to_lowercase())
        .collect();
    let mut out = Vec::new();
    for span in spans {
        let probe = collapse(strip_marks(span)).to_lowercase();
        let head: String = probe.chars().take(30).collect();
        let hit = norm
            .iter()
            .position(|n| n.contains(&probe))
            .or_else(|| norm.iter().position(|n| n.contains(&head)));
        match hit.and_then(|k| group_of(&all[k], false).map(|g| (k, g))) {
            Some((k, mut g)) => {
                if bold_lead(&all[k].text).is_none() {
                    let picked = label_of(span).filter(|l| {
                        question_sentences(&all[k])
                            .iter()
                            .any(|s| label_of(s).is_some_and(|x| x.eq_ignore_ascii_case(l)))
                    });
                    if let Some(l) = picked {
                        g.label = l;
                    }
                }
                out.push(g);
            }
            None => {
                if let Some(label) = label_of(span) {
                    out.push(Group {
                        pos: usize::MAX,
                        label,
                        item: String::new(),
                    });
                }
            }
        }
    }
    dedupe(out)
}

pub fn candidates(text: &str) -> Vec<String> {
    groups(text).into_iter().map(|g| g.label).collect()
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
