pub mod desk;
#[cfg(test)]
mod desk_tests;
mod picker;
#[cfg(test)]
pub(crate) mod sim;
#[cfg(test)]
mod tests;

use crate::attach::status::AskQuestion;
use crate::sanitize::is_unsafe;
use tmgr_ipc::chat::PollPick;

pub use picker::{
    collapse, parse_picker, parse_review, parse_screen, Cursor, Picker, PickerOption, Review,
    ReviewItem, Screen, TabInfo,
};

const KEY_DOWN: &[u8] = b"\x1b[B";
const KEY_SPACE: &[u8] = b" ";
const KEY_ENTER: &[u8] = b"\r";
const OTHER_MAX_BYTES: usize = 1000;
const HEADER_MAX: usize = 60;
const QUESTION_MAX: usize = 500;
const LABEL_MAX: usize = 200;
const TEXT_PREFIX: usize = 40;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expect {
    Cursor(usize),
    CursorNext,
    Checked(usize, bool),
    TextRow(String),
    Left { question: usize },
    Gone,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub keys: Vec<u8>,
    pub expect: Expect,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Next {
    Step(Step),
    Done,
    Refuse(&'static str),
}

pub fn sanitize_other(text: &str) -> String {
    let clean: String = text.chars().filter(|&c| !is_unsafe(c)).collect();
    let mut end = clean.len().min(OTHER_MAX_BYTES);
    while !clean.is_char_boundary(end) {
        end -= 1;
    }
    clean[..end].trim().to_string()
}

fn same(screen: &str, card: &str, limit: usize) -> bool {
    let s = collapse(screen);
    let c = collapse(card);
    s == c || (c.chars().count() >= limit && s.starts_with(&c))
}

pub fn matches_card(picker: &Picker, q: &AskQuestion) -> bool {
    if picker.options.len() != q.options.len() + 1 || picker.multi != q.multi_select {
        return false;
    }
    let header_ok = if picker.tabs.is_empty() {
        same(&picker.header, &q.header, HEADER_MAX)
    } else {
        q.header.is_empty()
            || picker
                .tabs
                .iter()
                .any(|t| same(&t.label, &q.header, HEADER_MAX))
    };
    header_ok
        && same(&picker.question, &q.question, QUESTION_MAX)
        && picker
            .options
            .iter()
            .zip(&q.options)
            .all(|(s, c)| same(&s.label, &c.label, LABEL_MAX))
}

fn text_row_has(picker: &Picker, text: &str) -> bool {
    let Some(row) = picker.options.last() else {
        return false;
    };
    let label = collapse(&row.label);
    let want = collapse(text);
    !label.is_empty()
        && (label == want
            || (want.chars().count() > TEXT_PREFIX
                && label.starts_with(&want.chars().take(TEXT_PREFIX).collect::<String>())))
}

impl Expect {
    pub fn holds(&self, questions: &[AskQuestion], screen: Option<&Screen>) -> bool {
        match (self, screen) {
            (Expect::Gone, None) => true,
            (Expect::Left { .. }, None) | (Expect::Left { .. }, Some(Screen::Review(_))) => true,
            (Expect::Left { question }, Some(Screen::Picker(p))) => questions
                .get(*question)
                .is_some_and(|q| !matches_card(p, q)),
            (Expect::Cursor(i), Some(Screen::Picker(p))) => p.cursor == Cursor::Option(*i),
            (Expect::CursorNext, Some(Screen::Picker(p))) => p.cursor == Cursor::Next,
            (Expect::Checked(i, want), Some(Screen::Picker(p))) => {
                p.options.get(*i).and_then(|o| o.checked) == Some(*want)
            }
            (Expect::TextRow(t), Some(Screen::Picker(p))) => text_row_has(p, t),
            _ => false,
        }
    }
}

fn step(keys: &[u8], expect: Expect) -> Next {
    Next::Step(Step {
        keys: keys.to_vec(),
        expect,
    })
}

fn valid_picks(questions: &[AskQuestion], picks: &[PollPick]) -> bool {
    let mut seen = vec![false; questions.len()];
    picks.iter().all(|p| {
        let qi = usize::from(p.question_idx);
        let Some(q) = questions.get(qi) else {
            return false;
        };
        if std::mem::replace(&mut seen[qi], true) {
            return false;
        }
        p.choices.iter().all(|&c| usize::from(c) < q.options.len())
    })
}

fn goto_row(cursor: Cursor, target: usize) -> Next {
    match cursor {
        Cursor::Option(k) if k < target => step(KEY_DOWN, Expect::Cursor(k + 1)),
        Cursor::Option(k) if k == target => Next::Refuse("already on target"),
        _ => Next::Refuse("cursor past target"),
    }
}

fn goto_next_row(p: &Picker) -> Next {
    match p.cursor {
        Cursor::Option(k) if k + 1 == p.options.len() => step(KEY_DOWN, Expect::CursorNext),
        Cursor::Option(k) => step(KEY_DOWN, Expect::Cursor(k + 1)),
        _ => Next::Refuse("cursor past target"),
    }
}

pub fn advance(questions: &[AskQuestion], picks: &[PollPick], screen: Option<&Screen>) -> Next {
    let Some(screen) = screen else {
        return Next::Done;
    };
    if !valid_picks(questions, picks) {
        return Next::Refuse("invalid answer");
    }
    match screen {
        Screen::Review(r) => advance_review(questions, picks, r),
        Screen::Picker(p) => advance_picker(questions, picks, p),
    }
}

fn advance_picker(questions: &[AskQuestion], picks: &[PollPick], p: &Picker) -> Next {
    let mut hits = questions
        .iter()
        .enumerate()
        .filter(|(_, q)| matches_card(p, q));
    let Some((qi, _)) = hits.next() else {
        return Next::Refuse("question does not match");
    };
    if hits.next().is_some() {
        return Next::Refuse("ambiguous question");
    }
    let Some(pick) = picks.iter().find(|a| usize::from(a.question_idx) == qi) else {
        return Next::Refuse("no answer for question");
    };
    let text_row = p.options.len() - 1;
    let other = pick
        .other
        .as_deref()
        .map(sanitize_other)
        .filter(|t| !t.is_empty());
    if pick.other.is_some() && other.is_none() {
        return Next::Refuse("empty text");
    }
    if p.multi {
        if other.is_some() {
            return Next::Refuse("other is not supported for multi select");
        }
        return advance_multi(p, pick, qi);
    }
    match (pick.choices.as_slice(), other) {
        ([c], None) => {
            let c = usize::from(*c);
            if p.cursor == Cursor::Option(c) {
                step(KEY_ENTER, Expect::Left { question: qi })
            } else {
                goto_row(p.cursor, c)
            }
        }
        ([], Some(text)) => {
            if p.cursor != Cursor::Option(text_row) {
                return goto_row(p.cursor, text_row);
            }
            if text_row_has(p, &text) {
                step(KEY_ENTER, Expect::Left { question: qi })
            } else if p.options[text_row].label == "Type something." {
                let keys = text.clone().into_bytes();
                step(&keys, Expect::TextRow(text))
            } else {
                Next::Refuse("text row not empty")
            }
        }
        _ => Next::Refuse("invalid answer"),
    }
}

fn advance_multi(p: &Picker, pick: &PollPick, qi: usize) -> Next {
    let wanted = |i: usize| pick.choices.iter().any(|&c| usize::from(c) == i);
    let real = p.options.len() - 1;
    let mismatch = (0..real).find(|&i| p.options[i].checked != Some(wanted(i)));
    match mismatch {
        Some(i) => match p.cursor {
            Cursor::Option(k) if k == i => step(KEY_SPACE, Expect::Checked(i, wanted(i))),
            c => goto_row(c, i),
        },
        None => match p.cursor {
            Cursor::Next => step(KEY_ENTER, Expect::Left { question: qi }),
            _ => goto_next_row(p),
        },
    }
}

fn expected_answer(q: &AskQuestion, pick: &PollPick) -> String {
    if let Some(t) = pick.other.as_deref() {
        return sanitize_other(t);
    }
    let mut idx: Vec<usize> = pick.choices.iter().map(|&c| usize::from(c)).collect();
    idx.sort_unstable();
    idx.dedup();
    idx.iter()
        .map(|&i| q.options[i].label.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn advance_review(questions: &[AskQuestion], picks: &[PollPick], r: &Review) -> Next {
    if r.items.len() != questions.len() || picks.len() != questions.len() {
        return Next::Refuse("review does not match");
    }
    for (i, (q, item)) in questions.iter().zip(&r.items).enumerate() {
        let Some(pick) = picks.iter().find(|a| usize::from(a.question_idx) == i) else {
            return Next::Refuse("review does not match");
        };
        if !same(&item.question, &q.question, QUESTION_MAX)
            || !same(&item.answer, &expected_answer(q, pick), usize::MAX)
        {
            return Next::Refuse("review does not match");
        }
    }
    if r.cursor_on_submit {
        step(KEY_ENTER, Expect::Gone)
    } else {
        Next::Refuse("review cursor not on submit")
    }
}
