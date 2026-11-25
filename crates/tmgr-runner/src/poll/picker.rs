use crate::sanitize::is_unsafe;

const FOOTER: &str = "Enter to select";
const REVIEW_TITLE: &str = "Review your answers";
const MIN_RULE: usize = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TabInfo {
    pub label: String,
    pub done: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickerOption {
    pub number: u32,
    pub label: String,
    pub checked: Option<bool>,
    pub is_text_row: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cursor {
    Option(usize),
    Next,
    Chat,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picker {
    pub header: String,
    pub tabs: Vec<TabInfo>,
    pub question: String,
    pub options: Vec<PickerOption>,
    pub multi: bool,
    pub cursor: Cursor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewItem {
    pub question: String,
    pub answer: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Review {
    pub cursor_on_submit: bool,
    pub items: Vec<ReviewItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Screen {
    Picker(Picker),
    Review(Review),
}

pub fn parse_screen(rows: &[String]) -> Option<Screen> {
    parse_picker(rows)
        .map(Screen::Picker)
        .or_else(|| parse_review(rows).map(Screen::Review))
}

pub fn collapse(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .filter(|&c| !is_unsafe(c))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn blank(r: &str) -> bool {
    r.trim().is_empty()
}

fn ruleish(r: &str) -> bool {
    let t = r.trim();
    t.chars().count() >= MIN_RULE && t.chars().all(|c| matches!(c, '─' | '╌' | '═' | '━'))
}

fn option_row(row: &str) -> Option<(bool, u32, String)> {
    let mut ch = row.chars();
    let cursor = match ch.next()? {
        '❯' => true,
        ' ' => false,
        _ => return None,
    };
    let rest = ch.as_str().strip_prefix(' ')?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() || digits.len() > 2 {
        return None;
    }
    let after = rest[digits.len()..].strip_prefix(". ")?;
    Some((cursor, digits.parse().ok()?, after.trim().to_string()))
}

fn next_row(row: &str) -> Option<bool> {
    let mut ch = row.chars();
    let cursor = match ch.next()? {
        '❯' => true,
        ' ' => false,
        _ => return None,
    };
    (ch.as_str().trim() == "Next").then_some(cursor)
}

enum Head {
    Chip(String),
    Tabs(Vec<TabInfo>),
}

fn head(line: &str) -> Option<Head> {
    let t = line.trim();
    if let Some(inner) = t.strip_prefix('←') {
        let inner = inner.trim_end().strip_suffix('→')?;
        let mut tabs: Vec<(char, String)> = Vec::new();
        for c in inner.chars() {
            if matches!(c, '☐' | '☒' | '✔') {
                tabs.push((c, String::new()));
            } else if let Some(last) = tabs.last_mut() {
                last.1.push(c);
            } else if !c.is_whitespace() {
                return None;
            }
        }
        let (mark, label) = tabs.pop()?;
        if mark != '✔' || label.trim() != "Submit" || tabs.is_empty() {
            return None;
        }
        return Some(Head::Tabs(
            tabs.into_iter()
                .map(|(m, l)| TabInfo {
                    label: collapse(&l),
                    done: m == '☒',
                })
                .collect(),
        ));
    }
    let mut ch = t.chars();
    matches!(ch.next()?, '☐' | '☒' | '✔').then_some(())?;
    let label = collapse(ch.as_str());
    (!label.is_empty()).then_some(Head::Chip(label))
}

fn question_line(r: &str) -> &str {
    let t = r.trim();
    t.strip_prefix('│').unwrap_or(t).trim()
}

pub fn parse_picker(rows: &[String]) -> Option<Picker> {
    let f = rows.iter().rposition(|r| r.trim().starts_with(FOOTER))?;
    let mut end = f;
    while end > 0 && blank(&rows[end - 1]) {
        end -= 1;
    }
    let chat_idx = end.checked_sub(1)?;
    let (chat_cursor, chat_n, chat_label) = option_row(&rows[chat_idx])?;
    if chat_label != "Chat about this" {
        return None;
    }
    let mid = chat_idx.checked_sub(1)?;
    if !ruleish(&rows[mid]) {
        return None;
    }
    let top = rows[..mid].iter().rposition(|r| ruleish(r))?;
    let region = &rows[top + 1..mid];

    let head = head(region.first()?)?;
    if !blank(region.get(1)?) {
        return None;
    }
    let q_len = region[2..].iter().take_while(|r| !blank(r)).count();
    if q_len == 0 {
        return None;
    }
    let question = collapse(
        &region[2..2 + q_len]
            .iter()
            .map(|r| question_line(r))
            .collect::<Vec<_>>()
            .join(" "),
    );
    let after = &region[2 + q_len..];
    if !after.first().is_some_and(|r| blank(r)) {
        return None;
    }
    let mut opt_rows = &after[1..];
    while opt_rows.last().is_some_and(|r| blank(r)) {
        opt_rows = &opt_rows[..opt_rows.len() - 1];
    }

    let mut next_cursor = None;
    if let Some((last, rest)) = opt_rows.split_last() {
        if let Some(c) = next_row(last) {
            next_cursor = Some(c);
            opt_rows = rest;
        }
    }

    let mut options: Vec<PickerOption> = Vec::new();
    let mut cursor: Option<Cursor> = None;
    let mut last_option_row = None;
    for (i, row) in opt_rows.iter().enumerate() {
        let Some((cur, number, label)) = option_row(row) else {
            if options.is_empty() || blank(row) || !row.starts_with("     ") {
                return None;
            }
            continue;
        };
        if number as usize != options.len() + 1 {
            return None;
        }
        let (checked, label) = if let Some(l) = label.strip_prefix("[ ] ") {
            (Some(false), l.trim().to_string())
        } else if let Some(l) = label.strip_prefix("[✔] ") {
            (Some(true), l.trim().to_string())
        } else {
            (None, label)
        };
        if cur && cursor.replace(Cursor::Option(options.len())).is_some() {
            return None;
        }
        options.push(PickerOption {
            number,
            label,
            checked,
            is_text_row: false,
        });
        last_option_row = Some(i);
    }
    if options.len() < 2 || last_option_row != Some(opt_rows.len().checked_sub(1)?) {
        return None;
    }
    let multi = options[0].checked.is_some();
    if options.iter().any(|o| o.checked.is_some() != multi) || multi != next_cursor.is_some() {
        return None;
    }
    if chat_n as usize != options.len() + 1 {
        return None;
    }
    options.last_mut()?.is_text_row = true;
    if let Some(true) = next_cursor {
        if cursor.replace(Cursor::Next).is_some() {
            return None;
        }
    }
    if chat_cursor && cursor.replace(Cursor::Chat).is_some() {
        return None;
    }

    let (header, tabs) = match head {
        Head::Chip(h) => (h, Vec::new()),
        Head::Tabs(t) => (String::new(), t),
    };
    Some(Picker {
        header,
        tabs,
        question,
        options,
        multi,
        cursor: cursor?,
    })
}

pub fn parse_review(rows: &[String]) -> Option<Review> {
    let title = rows.iter().rposition(|r| r.trim() == REVIEW_TITLE)?;
    let below = &rows[title + 1..];
    let submit = below
        .iter()
        .position(|r| option_row(r).is_some_and(|(_, n, l)| n == 1 && l == "Submit answers"))?;
    let (submit_cursor, _, _) = option_row(&below[submit])?;
    let (cancel_cursor, n, l) = option_row(below.get(submit + 1)?)?;
    if n != 2 || l != "Cancel" || (submit_cursor == cancel_cursor) {
        return None;
    }
    let mut items: Vec<ReviewItem> = Vec::new();
    for row in &below[..submit] {
        let t = row.trim();
        if t.is_empty() {
            continue;
        }
        if let Some(q) = t.strip_prefix('●') {
            items.push(ReviewItem {
                question: collapse(q),
                answer: String::new(),
            });
        } else {
            let last = items.last_mut()?;
            let piece = t.strip_prefix('→').unwrap_or(t);
            last.answer = collapse(&format!("{} {}", last.answer, piece));
        }
    }
    Some(Review {
        cursor_on_submit: submit_cursor,
        items,
    })
}
