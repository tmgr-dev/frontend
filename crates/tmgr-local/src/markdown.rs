//! The read side of the TS `pages/markdown.ts`: section markers and headings, outside code.

use std::sync::OnceLock;

use regex::Regex;

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Section {
    pub id: String,
    pub owner: String,
    pub heading: Option<String>,
}

fn fence_of(line: &str) -> Option<&str> {
    let spaces = line.bytes().take(3).take_while(|b| *b == b' ').count();
    let rest = &line[spaces..];
    let ch = *rest.as_bytes().first()?;
    if ch != b'`' && ch != b'~' {
        return None;
    }
    let run = rest.bytes().take_while(|b| *b == ch).count();
    (run >= 3).then(|| &rest[..run])
}

fn is_fence_close(line: &str, fence: &str, ch: u8, len: usize) -> bool {
    fence.as_bytes()[0] == ch && fence.len() >= len && line.trim().len() == fence.len()
}

fn fill(skip: &mut [bool], from: usize, to: usize) {
    let to = to.min(skip.len());
    if from < to {
        skip[from..to].fill(true);
    }
}

fn mask_fences(body: &str, skip: &mut [bool]) {
    let mut pos = 0;
    let mut open: Option<(u8, usize, usize)> = None;
    while pos < body.len() {
        let end = body[pos..].find('\n').map_or(body.len(), |i| pos + i);
        let line = &body[pos..end];
        let fence = fence_of(line);
        match (open, fence) {
            (None, Some(f)) => open = Some((f.as_bytes()[0], f.len(), pos)),
            (Some((ch, len, start)), Some(f)) if is_fence_close(line, f, ch, len) => {
                fill(skip, start, end);
                open = None;
            }
            _ => {}
        }
        pos = end + 1;
    }
    if let Some((_, _, start)) = open {
        fill(skip, start, body.len());
    }
}

fn mask_inline_code(body: &str, skip: &mut [bool]) {
    let bytes = body.as_bytes();
    let mut runs: Vec<(usize, usize)> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let start = i;
        while i < bytes.len() && bytes[i] == b'`' {
            i += 1;
        }
        runs.push((start, i - start));
    }
    if runs.len() < 2 {
        return;
    }
    let mut next_same = vec![None; runs.len()];
    let mut last: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for k in (0..runs.len()).rev() {
        next_same[k] = last.get(&runs[k].1).copied();
        last.insert(runs[k].1, k);
    }
    let blanks: Vec<usize> = body.match_indices("\n\n").map(|(i, _)| i).collect();
    let mut b = 0;
    let mut k = 0;
    while k < runs.len() {
        let (start, _) = runs[k];
        let Some(j) = next_same[k].filter(|_| !skip[start]) else {
            k += 1;
            continue;
        };
        let close_end = runs[j].0 + runs[j].1;
        while b < blanks.len() && blanks[b] < start {
            b += 1;
        }
        if b < blanks.len() && blanks[b] + 2 <= close_end {
            k += 1;
            continue;
        }
        fill(skip, start, close_end);
        k = j + 1;
    }
}

fn parse_heading(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i < 3 && i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }
    let hashes = bytes[i..].iter().take_while(|b| **b == b'#').count();
    let p = i + hashes;
    if !(1..=6).contains(&hashes) || p >= bytes.len() || (bytes[p] != b' ' && bytes[p] != b'\t') {
        return None;
    }
    let mut text = line[p..].trim();
    if text.ends_with('#') {
        let r = text.trim_end_matches('#').len();
        let before = text.as_bytes().get(r.wrapping_sub(1));
        if r == 0 || matches!(before, Some(b' ' | b'\t')) {
            text = text[..r].trim();
        }
    }
    Some(text.to_string())
}

fn first_heading(body: &str) -> Option<String> {
    let mut pos = 0;
    let mut open: Option<(u8, usize)> = None;
    loop {
        let eol = body[pos..].find('\n').map(|i| pos + i);
        let end = eol.unwrap_or(body.len());
        let line = &body[pos..end];
        let fence = fence_of(line);
        match (open, fence) {
            (None, Some(f)) => open = Some((f.as_bytes()[0], f.len())),
            (None, None) => {
                if let Some(h) = parse_heading(line) {
                    return Some(h);
                }
            }
            (Some((ch, len)), Some(f)) if is_fence_close(line, f, ch, len) => open = None,
            _ => {}
        }
        let eol = eol?;
        pos = eol + 1;
    }
}

/// Start, end, and `(id, owner)` for an opening marker.
type Marker = (usize, usize, Option<(String, String)>);

fn markers() -> &'static (Regex, Regex) {
    static RE: OnceLock<(Regex, Regex)> = OnceLock::new();
    RE.get_or_init(|| {
        (
            Regex::new(r#"<!--\s*tmgr:section\s+id="([^"]+)"\s+owner="([^"]+)"\s*-->"#).unwrap(),
            Regex::new(r"<!--\s*/tmgr:section\s*-->").unwrap(),
        )
    })
}

/// Well-formed `<!-- tmgr:section -->` blocks of a page body, in order, with their first heading.
pub(crate) fn sections(body: &str) -> Vec<Section> {
    let mut skip = vec![false; body.len() + 1];
    mask_fences(body, &mut skip);
    mask_inline_code(body, &mut skip);
    let (open_re, close_re) = markers();
    let mut marks: Vec<Marker> = Vec::new();
    for c in open_re.captures_iter(body) {
        let m = c.get(0).unwrap();
        if !skip[m.start()] {
            marks.push((
                m.start(),
                m.end(),
                Some((c[1].to_string(), c[2].to_string())),
            ));
        }
    }
    for m in close_re.find_iter(body) {
        if !skip[m.start()] {
            marks.push((m.start(), m.end(), None));
        }
    }
    marks.sort_by_key(|m| m.0);
    let mut out = Vec::new();
    let mut pending: Option<(usize, String, String)> = None;
    for (start, end, open) in marks {
        match open {
            Some((id, owner)) => pending = Some((end, id, owner)),
            None => {
                if let Some((inner_start, id, owner)) = pending.take() {
                    out.push(Section {
                        id,
                        owner,
                        heading: first_heading(&body[inner_start..start]),
                    });
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(body: &str) -> Vec<(String, Option<String>)> {
        sections(body)
            .into_iter()
            .map(|s| (s.id, s.heading))
            .collect()
    }

    #[test]
    fn finds_sections_with_their_first_heading() {
        let body = "# Top\n<!-- tmgr:section id=\"a\" owner=\"agents\" -->\n## Notes ##\ntext\n<!-- /tmgr:section -->\n<!--tmgr:section id=\"b\" owner=\"system\"-->\nno heading\n<!-- /tmgr:section -->";
        let found = sections(body);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].owner, "agents");
        assert_eq!(found[0].heading.as_deref(), Some("Notes"));
        assert_eq!(found[1].heading, None);
    }

    #[test]
    fn ignores_markers_in_code() {
        let fenced =
            "```\n<!-- tmgr:section id=\"x\" owner=\"agents\" -->\n<!-- /tmgr:section -->\n```\n";
        assert!(ids(fenced).is_empty());
        let inline =
            "`<!-- tmgr:section id=\"x\" owner=\"agents\" -->` and `<!-- /tmgr:section -->`";
        assert!(ids(inline).is_empty());
    }

    #[test]
    fn an_unclosed_section_is_dropped_and_a_reopened_one_wins() {
        let body = "<!-- tmgr:section id=\"a\" owner=\"o\" -->\n<!-- tmgr:section id=\"b\" owner=\"o\" -->\n# B\n<!-- /tmgr:section -->";
        assert_eq!(ids(body), [("b".to_string(), Some("B".to_string()))]);
    }

    #[test]
    fn headings_follow_the_atx_rules() {
        assert_eq!(parse_heading("### Title ###"), Some("Title".into()));
        assert_eq!(parse_heading("# C#"), Some("C#".into()));
        assert_eq!(parse_heading("#NoSpace"), None);
        assert_eq!(parse_heading("    # four spaces"), None);
        assert_eq!(parse_heading("####### seven"), None);
    }
}
