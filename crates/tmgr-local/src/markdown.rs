//! Port of the TS `pages/markdown.ts`: section markers, headings, links and task keys outside code,
//! and the body edits of append and set-section. Offsets are byte offsets; every marker is ASCII.

use std::sync::OnceLock;

use regex::Regex;

pub(crate) const MAX_BODY_BYTES: usize = 1_048_576;
pub(crate) const MAX_TASK_KEYS: usize = 50;
pub(crate) const MAX_LINKS: usize = 1000;
pub(crate) const AGENTS_OWNER: &str = "agents";
pub(crate) const SYSTEM_OWNER: &str = "system";
pub(crate) const SECTION_CLOSE: &str = "<!-- /tmgr:section -->";
pub(crate) const PLAIN: &str = "plain";
pub(crate) const CONTEXT: &str = "context";
pub(crate) const PERSON: &str = "person";
pub(crate) const MEETING: &str = "meeting";
pub(crate) const PROMISES_SECTION: &str = "promises";
pub(crate) const CHRONICLE_HEADING: &str = "Timeline";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Section {
    pub id: String,
    pub owner: String,
    pub heading: Option<String>,
    pub start: usize,
    pub inner_start: usize,
    pub inner_end: usize,
    pub end: usize,
}

struct Heading {
    level: usize,
    text: String,
    start: usize,
    end: usize,
}

struct Parsed {
    sections: Vec<Section>,
    opens: Vec<(String, String)>,
    error: Option<String>,
}

fn res() -> &'static [Regex; 5] {
    static RE: OnceLock<[Regex; 5]> = OnceLock::new();
    RE.get_or_init(|| {
        [
            Regex::new(r#"<!--\s*tmgr:section\s+id="([^"]+)"\s+owner="([^"]+)"\s*-->"#).unwrap(),
            Regex::new(r"<!--\s*/tmgr:section\s*-->").unwrap(),
            Regex::new(r"tmgr://(page|task|category|user|persona)/(\d+)").unwrap(),
            Regex::new(r"(?:https?|tmgr)://[^\s)>\]]+").unwrap(),
            Regex::new(r"^\s*(?:[-*+]|\d+[.)])\s+\S.*$").unwrap(),
        ]
    })
}

pub(crate) fn exceeds_limit(body: &str) -> bool {
    body.len() > MAX_BODY_BYTES
}

/// JavaScript `substring`: clamped to the text and with the bounds swapped when reversed, so
/// overlapping markers never panic.
pub(crate) fn substring(text: &str, a: usize, b: usize) -> &str {
    let (a, b) = (a.min(text.len()), b.min(text.len()));
    let (from, to) = if a <= b { (a, b) } else { (b, a) };
    text.get(from..to).unwrap_or("")
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

fn mask_comments(body: &str, skip: &mut [bool]) {
    let mut i = 0;
    while let Some(at) = body[i..].find("<!--").map(|p| i + p) {
        let Some(end) = body[at + 4..].find("-->").map(|p| at + 4 + p) else {
            return;
        };
        if !skip[at] {
            fill(skip, at, end + 3);
        }
        i = end + 3;
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

fn next_of(bytes: &[u8], from: usize, ch: u8) -> Option<usize> {
    bytes
        .get(from..)?
        .iter()
        .position(|b| *b == ch)
        .map(|p| from + p)
}

/// For each position, the next position at or after it holding `ch` (precomputed, like the TS `nextOf`).
fn next_table(bytes: &[u8], ch: u8) -> Vec<Option<usize>> {
    let mut next = vec![None; bytes.len() + 2];
    for i in (0..bytes.len()).rev() {
        next[i] = if bytes[i] == ch { Some(i) } else { next[i + 1] };
    }
    next
}

fn mask_markdown_links(body: &str, skip: &mut [bool]) {
    let bytes = body.as_bytes();
    if !bytes.contains(&b'[') {
        return;
    }
    let n = bytes.len();
    let newline = next_table(bytes, b'\n');
    let bracket = next_table(bytes, b']');
    let paren = next_table(bytes, b')');
    let mut i = 0;
    while i < n {
        if bytes[i] != b'[' {
            i += 1;
            continue;
        }
        let Some(close) = bracket[i + 1] else {
            i += 1;
            continue;
        };
        let newline_before = newline[i + 1].is_some_and(|nl| nl < close);
        if newline_before || close + 1 >= n || bytes[close + 1] != b'(' {
            i += 1;
            continue;
        }
        let Some(end) = paren[close + 2] else {
            i += 1;
            continue;
        };
        if newline[close + 2].is_some_and(|nl| nl < end) {
            i += 1;
            continue;
        }
        let start = if i > 0 && bytes[i - 1] == b'!' {
            i - 1
        } else {
            i
        };
        if !skip[start] {
            fill(skip, start, end + 1);
        }
        i = end + 1;
    }
}

fn mask_angle_autolinks(body: &str, skip: &mut [bool]) {
    let bytes = body.as_bytes();
    let n = bytes.len();
    let mut from = 0;
    while let Some(i) = next_of(bytes, from, b'<') {
        from = i + 1;
        let mut j = i + 1;
        if j >= n || !bytes[j].is_ascii_alphabetic() {
            continue;
        }
        while j < n && (bytes[j].is_ascii_alphanumeric() || b"+.-".contains(&bytes[j])) {
            j += 1;
        }
        if j >= n || bytes[j] != b':' {
            continue;
        }
        let mut k = j + 1;
        while k < n && bytes[k] != b'>' && bytes[k] != b'<' && !bytes[k].is_ascii_whitespace() {
            k += 1;
        }
        if k < n && bytes[k] == b'>' && !skip[i] {
            fill(skip, i, k + 1);
        }
    }
}

fn mask(body: &str, include_links: bool) -> Vec<bool> {
    let mut skip = vec![false; body.len() + 1];
    mask_fences(body, &mut skip);
    mask_comments(body, &mut skip);
    mask_inline_code(body, &mut skip);
    if include_links {
        mask_markdown_links(body, &mut skip);
        mask_angle_autolinks(body, &mut skip);
        for m in res()[3].find_iter(body) {
            if !skip[m.start()] {
                fill(&mut skip, m.start(), m.end());
            }
        }
    }
    skip
}

/// `tmgr://kind/id` links outside code and comments, first occurrence of each, in order.
/// `tmgr://kind/id` links outside code and comments, first occurrence of each, in order; at most
/// `MAX_LINKS`, the number a page write considers.
pub(crate) fn extract_links(body: &str) -> Vec<(String, i64)> {
    let skip = mask(body, false);
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<(String, i64)> = Vec::new();
    for c in res()[2].captures_iter(body) {
        if out.len() >= MAX_LINKS {
            break;
        }
        let m = c.get(0).unwrap();
        if skip[m.start()] {
            continue;
        }
        let Ok(id) = c[2].parse::<i64>() else {
            continue;
        };
        if id > 9_007_199_254_740_991 {
            continue;
        }
        let link = (c[1].to_string(), id);
        if seen.insert(link.clone()) {
            out.push(link);
        }
    }
    out
}

fn is_key_neighbour(c: char, dash_or_slash: bool) -> bool {
    c.is_alphanumeric() || c == '_' || c == '-' || (dash_or_slash && c == '/')
}

/// Matches of `(?<![\p{L}\p{N}_/-])([A-Z][A-Z0-9]+-\d+)(?![\p{L}\p{N}_-])` as (start, end).
fn task_key_matches(body: &str) -> Vec<(usize, usize)> {
    let bytes = body.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let found = (|| {
            if !bytes[i].is_ascii_uppercase() {
                return None;
            }
            if body[..i]
                .chars()
                .next_back()
                .is_some_and(|c| is_key_neighbour(c, true))
            {
                return None;
            }
            let mut j = i + 1;
            while j < bytes.len() && (bytes[j].is_ascii_uppercase() || bytes[j].is_ascii_digit()) {
                j += 1;
            }
            if j == i + 1 || bytes.get(j) != Some(&b'-') {
                return None;
            }
            let digits = bytes[j + 1..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
            if digits == 0 {
                return None;
            }
            let end = j + 1 + digits;
            if body[end..]
                .chars()
                .next()
                .is_some_and(|c| is_key_neighbour(c, false))
            {
                return None;
            }
            Some(end)
        })();
        match found {
            Some(end) => {
                out.push((i, end));
                i = end;
            }
            None => i += body[i..].chars().next().map_or(1, char::len_utf8),
        }
    }
    out
}

/// Bare task keys (TM-212) outside code, comments and existing links, in order of appearance.
pub(crate) fn task_key_candidates(body: &str) -> Vec<String> {
    let skip = mask(body, true);
    let mut out: Vec<String> = Vec::new();
    for (start, end) in task_key_matches(body) {
        if out.len() >= MAX_TASK_KEYS {
            break;
        }
        let key = body[start..end].to_string();
        if !skip[start] && !out.contains(&key) {
            out.push(key);
        }
    }
    out
}

pub(crate) fn autolink_task_keys(body: &str, ids: &[(String, i64)]) -> String {
    if ids.is_empty() {
        return body.to_string();
    }
    let skip = mask(body, true);
    let mut out = String::new();
    let mut last = 0;
    for (start, end) in task_key_matches(body) {
        let key = &body[start..end];
        let Some((_, id)) = ids.iter().find(|(k, _)| k == key).filter(|_| !skip[start]) else {
            continue;
        };
        out.push_str(&body[last..start]);
        out.push_str(&format!("[{key}](tmgr://task/{id})"));
        last = end;
    }
    out.push_str(substring(body, last, body.len()));
    out
}

fn parse_heading(line: &str, start: usize, end: usize) -> Option<Heading> {
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
    Some(Heading {
        level: hashes,
        text: text.to_string(),
        start,
        end,
    })
}

fn headings(body: &str) -> Vec<Heading> {
    let mut out = Vec::new();
    let mut pos = 0;
    let mut open: Option<(u8, usize)> = None;
    loop {
        let eol = body[pos..].find('\n').map(|i| pos + i);
        let end = eol.unwrap_or(body.len());
        let line = &body[pos..end];
        let fence = fence_of(line);
        match (open, fence) {
            (None, Some(f)) => open = Some((f.as_bytes()[0], f.len())),
            (None, None) => out.extend(parse_heading(line, pos, end)),
            (Some((ch, len)), Some(f)) if is_fence_close(line, f, ch, len) => open = None,
            _ => {}
        }
        let Some(eol) = eol else {
            return out;
        };
        pos = eol + 1;
    }
}

/// Start, end, and `(id, owner)` for an opening marker.
type Marker = (usize, usize, Option<(String, String)>);

fn parse(body: &str) -> Parsed {
    let mut skip = vec![false; body.len() + 1];
    mask_fences(body, &mut skip);
    mask_inline_code(body, &mut skip);
    let [open_re, close_re, ..] = res();
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
    let mut sections: Vec<Section> = Vec::new();
    let mut opens = Vec::new();
    let mut error: Option<String> = None;
    let mut pending: Option<(usize, usize, String, String)> = None;
    for (start, end, open) in marks {
        match open {
            Some((id, owner)) => {
                opens.push((id.clone(), owner.clone()));
                if let Some((_, _, pid, _)) = &pending {
                    error.get_or_insert(format!(
                        "Section '{pid}' is not closed before the next section starts"
                    ));
                }
                pending = Some((start, end, id, owner));
            }
            None => match pending.take() {
                None => {
                    error.get_or_insert("A section closing marker has no opening marker".into());
                }
                Some((open_start, inner_start, id, owner)) => {
                    if sections.iter().any(|s| s.id == id) {
                        error.get_or_insert(format!("Section id '{id}' is used more than once"));
                    }
                    sections.push(Section {
                        heading: headings(substring(body, inner_start, start))
                            .into_iter()
                            .next()
                            .map(|h| h.text),
                        id,
                        owner,
                        start: open_start,
                        inner_start,
                        inner_end: start,
                        end,
                    });
                }
            },
        }
    }
    if let Some((_, _, id, _)) = pending {
        error.get_or_insert(format!("Section '{id}' is never closed"));
    }
    Parsed {
        sections,
        opens,
        error,
    }
}

/// Well-formed `<!-- tmgr:section -->` blocks of a page body, in order, with their first heading.
pub(crate) fn sections(body: &str) -> Vec<Section> {
    parse(body).sections
}

/// Why the section markers of this body are not well formed: duplicate ids, nesting, strays.
pub(crate) fn structure_error(body: &str) -> Option<String> {
    parse(body).error
}

pub(crate) fn find_section(body: &str, id: &str) -> Option<Section> {
    sections(body).into_iter().find(|s| s.id == id)
}

/// Whether agent-supplied text carries a section marker of any kind.
pub(crate) fn contains_section_marker(text: &str) -> bool {
    let mut from = 0;
    while let Some(i) = text[from..].find("<!--").map(|p| from + p) {
        let rest = text[i + 4..].trim_start();
        if rest.starts_with("tmgr:section") || rest.starts_with("/tmgr:section") {
            return true;
        }
        from = i + 1;
    }
    false
}

/// A persona may write the shared `agents` owner or its own section, a plugin only its own; on a context page only personas, only `agents`.
pub(crate) fn writable_by(owner: &str, kind: &str, reference: &str, context_page: bool) -> bool {
    if owner == AGENTS_OWNER {
        return kind == "persona";
    }
    !context_page && owner == format!("{kind}:{reference}")
}

fn first_line(text: &str) -> &str {
    text.split(['\r', '\n']).next().unwrap_or("")
}

/// Replaces what is between the markers, keeping the section heading when the new text brings none.
pub(crate) fn replace_section(body: &str, section: &Section, markdown: &str) -> String {
    let inner = substring(body, section.inner_start, section.inner_end);
    let found = headings(inner);
    let kept = found
        .first()
        .filter(|h| inner[..h.start].trim().is_empty())
        .map(|h| &inner[h.start..h.end]);
    let text = markdown.trim();
    let brings_heading = !text.is_empty() && !headings(first_line(text)).is_empty();
    let mut next = String::from("\n");
    if let Some(heading) = kept.filter(|_| !brings_heading) {
        next.push_str(heading);
        next.push('\n');
        if !text.is_empty() {
            next.push('\n');
        }
    }
    if !text.is_empty() {
        next.push_str(text);
        next.push('\n');
    }
    format!(
        "{}{next}{}",
        substring(body, 0, section.inner_start),
        substring(body, section.inner_end, body.len())
    )
}

fn insert_at(body: &str, at: usize, markdown: &str) -> String {
    let before = body[..at].trim_end();
    let after = &body[at..];
    let text = markdown.trim();
    if before.is_empty() {
        return if after.is_empty() {
            format!("{text}\n")
        } else {
            format!("{text}\n\n{after}")
        };
    }
    let last_line = &before[before.rfind('\n').map_or(0, |i| i + 1)..];
    let list = &res()[4];
    let sep = if list.is_match(last_line) && list.is_match(first_line(text)) {
        "\n"
    } else {
        "\n\n"
    };
    if after.is_empty() {
        return format!("{before}{sep}{text}\n");
    }
    let gap = if res()[1].find(after).is_some_and(|m| m.start() == 0) {
        "\n"
    } else {
        "\n\n"
    };
    format!("{before}{sep}{text}{gap}{after}")
}

pub(crate) fn append_to_end(body: &str, markdown: &str) -> String {
    insert_at(body, body.len(), markdown)
}

pub(crate) fn append_in_section(body: &str, section: &Section, markdown: &str) -> String {
    insert_at(body, section.inner_end, markdown)
}

fn normalize_heading(heading: &str) -> String {
    heading.trim().trim_start_matches('#').trim().to_string()
}

const HEADING_GROUPS: [&[&str]; 12] = [
    &["timeline", "хронология"],
    &["summary", "кратко"],
    &["promises", "обещания"],
    &["what i know", "что я знаю"],
    &["insights", "инсайты"],
    &["agenda", "повестка"],
    &["outcomes", "итоги"],
    &["decisions", "решения"],
    &["action items", "действия"],
    &["how we work", "как мы работаем"],
    &["architecture", "архитектура"],
    &["agent notes", "заметки агентов"],
];

fn normalize_alias(heading: &str) -> String {
    heading
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub(crate) fn headings_match(a: &str, b: &str) -> bool {
    let key = normalize_alias(a);
    let wanted = normalize_alias(b);
    match HEADING_GROUPS.iter().find(|g| g.contains(&key.as_str())) {
        Some(group) => group.contains(&wanted.as_str()),
        None => key == wanted,
    }
}

pub(crate) fn is_chronicle_heading(heading: Option<&str>) -> bool {
    headings_match(&normalize_heading(heading.unwrap_or("")), CHRONICLE_HEADING)
}

/// Appends inside the first `##` section called `heading`; `None` when there is none.
pub(crate) fn append_under_heading(body: &str, heading: &str, markdown: &str) -> Option<String> {
    let wanted = normalize_heading(heading);
    let all = headings(body);
    let index = all
        .iter()
        .position(|h| h.level == 2 && headings_match(&h.text, &wanted))?;
    let target = &all[index];
    let mut boundary = all[index + 1..]
        .iter()
        .find(|h| h.level <= 2)
        .map_or(body.len(), |h| h.start);
    for s in sections(body) {
        for marker in [s.start, s.inner_end] {
            if marker > target.end && marker < boundary {
                boundary = marker;
            }
        }
    }
    Some(insert_at(body, boundary, markdown))
}

pub(crate) fn append_new_heading(body: &str, heading: &str, markdown: &str) -> String {
    let title = normalize_heading(heading);
    let prefix = if body.trim().is_empty() {
        String::new()
    } else {
        format!("{}\n\n", body.trim_end())
    };
    format!("{prefix}## {title}\n\n{}\n", markdown.trim())
}

pub(crate) fn append_new_section(
    body: &str,
    id: &str,
    owner: &str,
    heading: &str,
    markdown: &str,
) -> String {
    let block = format!(
        "<!-- tmgr:section id=\"{id}\" owner=\"{owner}\" -->\n## {heading}\n\n{}\n{SECTION_CLOSE}\n",
        markdown.trim()
    );
    let head = body.trim_end();
    if head.is_empty() {
        block
    } else {
        format!("{head}\n\n{block}")
    }
}

pub(crate) fn valid_section_id(id: &str) -> bool {
    let b = id.as_bytes();
    let ok = |c: &u8| c.is_ascii_lowercase() || c.is_ascii_digit();
    !b.is_empty() && b.len() <= 64 && ok(&b[0]) && b[1..].iter().all(|c| ok(c) || *c == b'-')
}

fn mask_writable(
    body: &str,
    list: &[Section],
    kind: &str,
    reference: &str,
    context_page: bool,
) -> String {
    let mut out = String::new();
    let mut last = 0;
    for s in list {
        if s.start < last || !writable_by(&s.owner, kind, reference, context_page) {
            continue;
        }
        out.push_str(substring(body, last, s.inner_start));
        out.push('\u{0}');
        last = s.inner_end;
    }
    out.push_str(&body[last..]);
    out
}

/// Why a persona or plugin may not turn `old` into `new`: it may only write inside its own (or `agents`) sections.
pub(crate) fn non_human_violation(
    old: &str,
    new: &str,
    context_page: bool,
    kind: &str,
    reference: &str,
    new_section_id: Option<&str>,
) -> Option<String> {
    let mut old_keys = parse(old).opens;
    if let Some(id) = new_section_id {
        old_keys.push((id.to_string(), format!("{kind}:{reference}")));
    }
    if old_keys != parse(new).opens {
        return Some("Section markers cannot be added, removed or changed".into());
    }
    let before = sections(old);
    let after = sections(new);
    let after_by_id = |id: &str| after.iter().find(|s| s.id == id);
    for s in &before {
        let writable = writable_by(&s.owner, kind, reference, context_page);
        let Some(now) = after_by_id(&s.id) else {
            return Some(format!("Section '{}' cannot be removed", s.id));
        };
        if now.owner != s.owner {
            return Some(format!("Section '{}' owner cannot be changed", s.id));
        }
        if !writable && substring(old, s.start, s.end) != substring(new, now.start, now.end) {
            return Some(format!("Section '{}' is managed by {}", s.id, s.owner));
        }
    }
    for s in &after {
        if !before.iter().any(|b| b.id == s.id)
            && !writable_by(&s.owner, kind, reference, context_page)
        {
            return Some(format!("Section '{}' would be owned by {}", s.id, s.owner));
        }
    }
    if context_page
        && mask_writable(old, &before, kind, reference, context_page)
            != mask_writable(new, &after, kind, reference, context_page)
    {
        return Some("A context page may only be changed inside its agent sections".into());
    }
    None
}

/// Why a write by anyone but the system may not stand: it changes or removes a system section.
pub(crate) fn system_section_violation(old: &str, new: &str) -> Option<String> {
    let after = sections(new);
    for s in sections(old) {
        if s.owner != SYSTEM_OWNER {
            continue;
        }
        let same = after
            .iter()
            .find(|n| n.id == s.id)
            .is_some_and(|n| substring(old, s.start, s.end) == substring(new, n.start, n.end));
        if !same {
            return Some(format!("Section '{}' is managed by the system", s.id));
        }
    }
    None
}

pub(crate) fn is_known_type(kind: &str) -> bool {
    [PLAIN, CONTEXT, PERSON, MEETING].contains(&kind)
}

const CONTEXT_BODY: &str = "## How we work\n\n## Architecture\n\n<!-- tmgr:section id=\"agent-notes\" owner=\"agents\" -->\n## Agent notes\n<!-- /tmgr:section -->\n";
const MEETING_BODY: &str = "## Agenda\n\n## Outcomes\n\n## Decisions\n\n## Action items\n";
const PERSON_HEAD: &str = "## Summary\n\n<!-- tmgr:section id=\"promises\" owner=\"system\" -->\n## Promises\n<!-- /tmgr:section -->\n\n## Timeline\n\n## What I know\n\n";

/// For a person, `analyst` (when set) owns the "Insights" section.
pub(crate) fn template_body(kind: &str, analyst: Option<&str>) -> String {
    match kind {
        CONTEXT => CONTEXT_BODY.into(),
        MEETING => MEETING_BODY.into(),
        PERSON => {
            let insights = match analyst.map(str::trim).filter(|u| !u.is_empty()) {
                Some(uuid) => format!(
                    "<!-- tmgr:section id=\"insights\" owner=\"persona:{uuid}\" -->\n## Insights\n<!-- /tmgr:section -->\n"
                ),
                None => "## Insights\n".into(),
            };
            format!("{PERSON_HEAD}{insights}")
        }
        _ => String::new(),
    }
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
        assert_eq!(structure_error(body), None);
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
    fn reports_malformed_structure() {
        let body = "<!-- tmgr:section id=\"a\" owner=\"o\" -->\n<!-- tmgr:section id=\"b\" owner=\"o\" -->\n# B\n<!-- /tmgr:section -->";
        assert_eq!(ids(body), [("b".to_string(), Some("B".to_string()))]);
        assert_eq!(
            structure_error(body).as_deref(),
            Some("Section 'a' is not closed before the next section starts")
        );
        assert_eq!(
            structure_error("<!-- /tmgr:section -->").as_deref(),
            Some("A section closing marker has no opening marker")
        );
    }

    #[test]
    fn headings_follow_the_atx_rules() {
        let text = |l: &str| parse_heading(l, 0, l.len()).map(|h| h.text);
        assert_eq!(text("### Title ###"), Some("Title".into()));
        assert_eq!(text("# C#"), Some("C#".into()));
        assert_eq!(text("#NoSpace"), None);
        assert_eq!(text("    # four spaces"), None);
        assert_eq!(text("####### seven"), None);
    }

    #[test]
    fn links_and_task_keys_skip_code_and_existing_links() {
        let body = "See tmgr://page/3 and `tmgr://task/9` <!-- tmgr://task/8 --> tmgr://page/3 TM-12, x-TM-1 [TM-5](tmgr://task/5) TM-7a AB-3.";
        assert_eq!(
            extract_links(body),
            [("page".to_string(), 3), ("task".to_string(), 5)]
        );
        assert_eq!(task_key_candidates(body), ["TM-12", "AB-3"]);
        assert_eq!(
            autolink_task_keys("Do TM-12 now", &[("TM-12".into(), 4)]),
            "Do [TM-12](tmgr://task/4) now"
        );
    }

    #[test]
    fn replace_and_append_keep_the_layout() {
        let body = "Intro\n<!-- tmgr:section id=\"s\" owner=\"agents\" -->\n## Log\n- one\n<!-- /tmgr:section -->\n";
        let s = find_section(body, "s").unwrap();
        assert_eq!(
            replace_section(body, &s, "- two"),
            "Intro\n<!-- tmgr:section id=\"s\" owner=\"agents\" -->\n## Log\n\n- two\n<!-- /tmgr:section -->\n"
        );
        assert_eq!(
            append_in_section(body, &s, "- two"),
            "Intro\n<!-- tmgr:section id=\"s\" owner=\"agents\" -->\n## Log\n- one\n- two\n<!-- /tmgr:section -->\n"
        );
        assert_eq!(
            append_to_end("Hello", "\n\nAppended line"),
            "Hello\n\nAppended line\n"
        );
        assert_eq!(append_to_end("", "x"), "x\n");
        assert_eq!(
            append_under_heading("## Timeline\n- a\n## Next\n", "хронология", "- b").as_deref(),
            Some("## Timeline\n- a\n- b\n\n## Next\n")
        );
    }

    #[test]
    fn agents_stay_inside_their_sections() {
        let old = "Text\n<!-- tmgr:section id=\"a\" owner=\"agents\" -->\nx\n<!-- /tmgr:section -->\n<!-- tmgr:section id=\"p\" owner=\"system\" -->\ny\n<!-- /tmgr:section -->\n";
        let inside = old.replace("\nx\n", "\nx2\n");
        assert_eq!(
            non_human_violation(old, &inside, false, "persona", "u", None),
            None
        );
        let managed = old.replace("\ny\n", "\ny2\n");
        assert_eq!(
            non_human_violation(old, &managed, false, "persona", "u", None).as_deref(),
            Some("Section 'p' is managed by system")
        );
        assert_eq!(
            system_section_violation(old, &managed).as_deref(),
            Some("Section 'p' is managed by the system")
        );
        let outside = old.replace("Text", "Changed");
        assert!(non_human_violation(old, &outside, true, "persona", "u", None).is_some());
        assert!(contains_section_marker("a <!--  /tmgr:section -->"));
    }

    #[test]
    fn overlapping_markers_never_panic() {
        let body = "<!-- tmgr:section id=\"a<!-- /tmgr:section -->\" owner=\"agents\" -->\n# H\n<!-- /tmgr:section -->";
        for s in sections(body) {
            let _ = replace_section(body, &s, "x");
            let _ = append_in_section(body, &s, "x");
        }
        let _ = non_human_violation(body, "", true, "persona", "u", None);
        let _ = system_section_violation(body, "");
        assert_eq!(substring("abcdef", 4, 1), "bcd");
        assert_eq!(substring("abc", 2, 99), "c");
    }

    #[test]
    fn pathological_bodies_stay_linear() {
        let brackets = "[".repeat(200_000);
        let started = std::time::Instant::now();
        assert!(task_key_candidates(&brackets).is_empty());
        let links: String = (0..5000).map(|i| format!("tmgr://task/{i} ")).collect();
        assert_eq!(extract_links(&links).len(), MAX_LINKS);
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }
}
