use super::mouse::{Kind, MouseEvent};
use super::prompts::clean;
use super::render::{button_style, dim, put};
use super::view::{contains, MouseOut, ViewState};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, BorderType, Clear, Widget};
use serde::Deserialize;
use std::collections::HashMap;
use unicode_width::UnicodeWidthStr;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pr {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub state: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum PrLookup {
    #[default]
    Unavailable,
    NoPr,
    Found(Pr),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GitInfo {
    pub root: String,
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub changed: u32,
    pub untracked: u32,
    pub added: u64,
    pub deleted: u64,
    pub base: Option<String>,
    pub ahead_base: Option<u32>,
    pub pr: PrLookup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GitAction {
    Diff,
    OpenPr,
    Close,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct StatusInfo {
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    pub changed: u32,
    pub untracked: u32,
}

pub fn parse_status(text: &str) -> StatusInfo {
    let mut out = StatusInfo::default();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("# branch.head ") {
            out.branch = clean(rest, 100);
        } else if let Some(rest) = line.strip_prefix("# branch.upstream ") {
            out.upstream = Some(clean(rest, 100)).filter(|u| !u.is_empty());
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            let mut it = rest.split_whitespace();
            let num = |s: Option<&str>, sign: char| {
                s.and_then(|s| s.strip_prefix(sign))
                    .and_then(|n| n.parse().ok())
                    .unwrap_or(0)
            };
            out.ahead = num(it.next(), '+');
            out.behind = num(it.next(), '-');
        } else if line.starts_with("1 ") || line.starts_with("2 ") || line.starts_with("u ") {
            out.changed += 1;
        } else if line.starts_with("? ") {
            out.untracked += 1;
        }
    }
    out
}

pub fn parse_numstat(text: &str) -> (u64, u64) {
    text.lines().fold((0, 0), |(a, d), line| {
        let mut it = line.split('\t');
        let add = it.next().and_then(|n| n.parse::<u64>().ok()).unwrap_or(0);
        let del = it.next().and_then(|n| n.parse::<u64>().ok()).unwrap_or(0);
        (a + add, d + del)
    })
}

pub fn valid_pr_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://github.com/") else {
        return false;
    };
    let parts: Vec<&str> = rest.split('/').collect();
    let name = |s: &str| {
        !s.is_empty()
            && s != "."
            && s != ".."
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
    };
    parts.len() == 4
        && name(parts[0])
        && name(parts[1])
        && parts[2] == "pull"
        && !parts[3].is_empty()
        && parts[3].bytes().all(|b| b.is_ascii_digit())
}

#[derive(Deserialize)]
struct RawPr {
    number: u64,
    #[serde(default)]
    title: String,
    url: String,
    #[serde(default)]
    state: String,
}

pub fn parse_pr(json: &str) -> Option<Pr> {
    let raw: RawPr = serde_json::from_str(json).ok()?;
    valid_pr_url(&raw.url).then(|| Pr {
        number: raw.number,
        title: clean(&raw.title, 120),
        url: raw.url,
        state: clean(&raw.state, 12).to_ascii_lowercase(),
    })
}

impl GitInfo {
    pub fn dirty(&self) -> u32 {
        self.changed + self.untracked
    }

    pub fn badge(&self, room: usize) -> Option<String> {
        let mut tail = String::new();
        if self.dirty() > 0 {
            tail.push_str(&format!(" ±{}", self.dirty()));
        }
        if let Some(n) = self.ahead_base.filter(|n| *n > 0) {
            tail.push_str(&format!(" ↑{n}"));
        }
        let fixed = 5 + tail.width();
        let branch_room = room.checked_sub(fixed).filter(|r| *r >= 4)?;
        let branch: String = if self.branch.width() > branch_room {
            let mut s: String = self.branch.chars().take(branch_room - 1).collect();
            s.push('…');
            s
        } else {
            self.branch.clone()
        };
        Some(format!(" · ⎇ {branch}{tail}"))
    }
}

const PANEL_W: u16 = 66;
const PANEL_H: u16 = 11;

fn panel_rect(area: Rect) -> Rect {
    let w = PANEL_W.min(area.width);
    let h = PANEL_H.min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

fn buttons(rect: Rect, can_pr: bool) -> [(Rect, GitAction, &'static str, bool); 3] {
    let y = rect.bottom().saturating_sub(3);
    let items = [
        (GitAction::Diff, "[ d  Diff ]", true),
        (GitAction::OpenPr, "[ p  Open PR ]", can_pr),
        (GitAction::Close, "[ Esc  Close ]", true),
    ];
    let mut x = rect.x + 2;
    items.map(|(action, label, enabled)| {
        let w = label.width() as u16;
        let r = Rect::new(x, y, w.min(rect.right().saturating_sub(x)), 1);
        x += w + 2;
        (r, action, label, enabled)
    })
}

impl ViewState {
    pub fn set_git(&mut self, found: HashMap<String, GitInfo>) -> bool {
        if found == self.git {
            return false;
        }
        self.git = found;
        if self.git_open && self.focused_git().is_none() {
            self.git_open = false;
        }
        true
    }

    pub fn focused_git(&self) -> Option<&GitInfo> {
        self.git.get(self.focused_session()?)
    }

    pub fn toggle_git(&mut self) {
        if self.git_open {
            self.git_open = false;
            return;
        }
        if self.focused_git().is_some() {
            self.git_open = true;
            self.help = false;
            self.grab = None;
        } else {
            self.notice = Some("no git repository for this session".into());
        }
    }

    fn can_open_pr(&self) -> bool {
        matches!(self.focused_git().map(|g| &g.pr), Some(PrLookup::Found(_)))
    }

    pub fn edit_git(&mut self, data: &[u8]) -> Option<GitAction> {
        let action = match data {
            b"d" | b"D" => Some(GitAction::Diff),
            b"p" | b"P" | b"\r" if self.can_open_pr() => Some(GitAction::OpenPr),
            b"G" | b"q" | b"\x1b" => Some(GitAction::Close),
            _ => None,
        };
        if action == Some(GitAction::Close) {
            self.git_open = false;
        }
        action
    }

    pub(crate) fn git_mouse(&mut self, area: Rect, ev: MouseEvent, mut out: MouseOut) -> MouseOut {
        let rect = panel_rect(area);
        match ev.kind() {
            Kind::Press => {
                self.grab = Some(String::new());
                if !ev.is_left_press() {
                    return out;
                }
                out.redraw = true;
                let hit = buttons(rect, self.can_open_pr())
                    .into_iter()
                    .find(|(r, _, _, on)| *on && contains(*r, ev.col, ev.row))
                    .map(|(_, a, _, _)| a);
                if let Some(action) = hit {
                    if action == GitAction::Close {
                        self.git_open = false;
                    }
                    out.git = Some(action);
                } else if !contains(rect, ev.col, ev.row) {
                    self.git_open = false;
                }
            }
            Kind::Release if self.grab.as_deref() == Some("") => self.grab = None,
            _ => {}
        }
        out
    }
}

pub(crate) fn draw_git(buf: &mut Buffer, area: Rect, st: &ViewState) {
    let Some(g) = st.focused_git() else {
        return;
    };
    let rect = panel_rect(area);
    if rect.is_empty() {
        return;
    }
    Clear.render(rect, buf);
    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" Git ")
        .render(rect, buf);
    let inner = rect.inner(ratatui::layout::Margin::new(2, 1));
    if inner.is_empty() {
        return;
    }
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let track = match (&g.upstream, g.ahead, g.behind) {
        (Some(u), a, b) => format!("{} (tracks {u}, ↑{a} ↓{b})", g.branch),
        (None, ..) => format!("{} (no upstream)", g.branch),
    };
    let base = match (&g.base, g.ahead_base) {
        (Some(b), Some(n)) => format!("{n} commit{} ahead of {b}", if n == 1 { "" } else { "s" }),
        _ => "no base branch found".to_string(),
    };
    let changes = if g.dirty() == 0 {
        "clean".to_string()
    } else {
        format!(
            "{} changed, {} untracked  +{} -{}",
            g.changed, g.untracked, g.added, g.deleted
        )
    };
    let pr = match &g.pr {
        PrLookup::Found(p) => format!("#{} {} ({})", p.number, p.title, p.state),
        PrLookup::NoPr => "no pull request for this branch".to_string(),
        PrLookup::Unavailable => "unknown (gh is not available)".to_string(),
    };
    let root = g.root.rsplit('/').next().unwrap_or("");
    let rows = [
        ("Repo", root.to_string()),
        ("Branch", track),
        ("Base", base),
        ("Changes", changes),
        ("PR", pr),
    ];
    for (i, (label, value)) in rows.iter().enumerate() {
        let y = inner.y + i as u16;
        if y >= inner.bottom() {
            break;
        }
        put(buf, inner.x, y, label, bold, inner.width);
        let x = inner.x + 9;
        if x < inner.right() {
            put(buf, x, y, value, Style::new(), inner.right() - x);
        }
    }
    let hover = st.pointer;
    for (r, _, label, enabled) in buttons(rect, st.can_open_pr()) {
        if r.is_empty() {
            continue;
        }
        let over = hover.is_some_and(|(c, row)| contains(r, c, row));
        put(buf, r.x, r.y, label, button_style(over, enabled), r.width);
    }
    put(
        buf,
        inner.x,
        inner.bottom().saturating_sub(1),
        "read-only: nothing is committed, pushed or checked out",
        dim(),
        inner.width,
    );
}
