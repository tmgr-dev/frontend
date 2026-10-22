use super::mouse::{Kind, MouseEvent};
use super::render::{button_style, dim, put};
use super::summary::{cut, mask_pairs, scrub, short_path, texts, wrap};
use super::title::{open_under, LINE_CAP};
use super::view::{contains, MouseOut, ViewState};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, BorderType, Clear, Widget};
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use unicode_width::UnicodeWidthStr;

pub const TAIL: u64 = 8 * 1024 * 1024;
const MAX_LINES: usize = 20_000;
const MAX_LINE: usize = 2000;
const TOOL_ARG: usize = 120;
const MAX_BYTES: usize = 1024 * 1024;

fn md_line(raw: &str) -> String {
    let line: String = raw
        .chars()
        .filter(|c| !crate::sanitize::is_unsafe(*c))
        .collect();
    let line = line.trim_end().to_string();
    let line = if line.trim_start().starts_with('#') {
        format!("\\{line}")
    } else {
        line
    };
    if line.chars().count() > MAX_LINE {
        let mut s: String = line.chars().take(MAX_LINE - 1).collect();
        s.push('…');
        s
    } else {
        line
    }
}

fn tool_line(name: &str, input: Option<&Value>, cwd: &str, home: &str) -> String {
    let field = |k: &str| input.and_then(|i| i.get(k)).and_then(Value::as_str);
    let name: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | ':'))
        .take(40)
        .collect();
    let arg = match name.as_str() {
        "Bash" => field("command").map(|c| cut(c.lines().next().unwrap_or(""), TOOL_ARG)),
        "Edit" | "Write" | "MultiEdit" | "Read" | "NotebookEdit" => field("file_path")
            .or_else(|| field("notebook_path"))
            .map(|p| short_path(&cut(p, 300), cwd, home)),
        "Grep" | "Glob" => field("pattern").map(|p| cut(p, TOOL_ARG)),
        "WebFetch" => field("url").map(|u| cut(u.split(['?', '#']).next().unwrap_or(""), TOOL_ARG)),
        "WebSearch" => field("query").map(|p| cut(p, TOOL_ARG)),
        _ => None,
    };
    let arg = arg.unwrap_or_default().replace('`', "'");
    if arg.is_empty() {
        format!("- tool: {name}")
    } else {
        format!("- tool: {name} `{arg}`")
    }
}

pub fn markdown_lines<'a>(
    lines: impl Iterator<Item = &'a str>,
    title: &str,
    cwd: &str,
    home: &str,
) -> Vec<String> {
    let mut out = vec![
        format!("# Session: {}", md_line(title).trim_start_matches('\\')),
        String::new(),
    ];
    let mut bytes: usize;
    'outer: for line in lines.filter(|l| l.len() <= LINE_CAP) {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v.get("isSidechain").and_then(Value::as_bool) == Some(true)
            || v.get("isMeta").and_then(Value::as_bool) == Some(true)
        {
            continue;
        }
        let Some(content) = v.get("message").and_then(|m| m.get("content")) else {
            continue;
        };
        let role = match v.get("type").and_then(Value::as_str) {
            Some("user") => "User",
            Some("assistant") => "Assistant",
            _ => continue,
        };
        let mut block: Vec<String> = Vec::new();
        for t in texts(content) {
            let t = t.trim();
            if t.is_empty()
                || (role == "User" && (t.starts_with('<') || t.starts_with("[Request interrupted")))
            {
                continue;
            }
            block.extend(t.lines().map(md_line));
        }
        if role == "Assistant" {
            if let Some(items) = content.as_array() {
                for b in items
                    .iter()
                    .filter(|b| b.get("type").and_then(Value::as_str) == Some("tool_use"))
                {
                    let name = b.get("name").and_then(Value::as_str).unwrap_or("");
                    block.push(tool_line(name, b.get("input"), cwd, home));
                }
            }
        }
        if block.is_empty() {
            continue;
        }
        out.push(format!("## {role}"));
        out.extend(block);
        out.push(String::new());
        bytes = out.iter().map(|l| l.len() + 1).sum();
        if out.len() >= MAX_LINES || bytes > MAX_BYTES {
            out.push("(the rest of the session was left out: too long)".into());
            break 'outer;
        }
    }
    out
}

pub fn transcript(
    root: &Path,
    path: &Path,
    title: &str,
    cwd: &str,
    home: &str,
) -> Option<Vec<String>> {
    let (mut file, meta) = open_under(root, path)?;
    let start = meta.len().saturating_sub(TAIL);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::new();
    file.take(TAIL).read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf);
    let mut lines = text.lines();
    if start > 0 {
        lines.next();
    }
    let mut out = markdown_lines(lines, title, cwd, home);
    if start > 0 {
        out.insert(2, "(the beginning of a long session was left out)".into());
    }
    Some(out)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub line: usize,
    pub kind: &'static str,
    pub keepable: bool,
    pub original: String,
    pub cleaned: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Pending,
    Remove,
    Keep,
}

fn env_like(line: &str) -> Option<String> {
    let t = line.trim_start();
    let t = t.strip_prefix("export ").unwrap_or(t).trim_start();
    let (name, value) = t.split_once('=')?;
    let ok = name.len() >= 3
        && name.chars().next().is_some_and(|c| c.is_ascii_uppercase())
        && name
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
    (ok && value.trim().len() >= 4).then(|| format!("{name}=…"))
}

fn user_dirs(line: &str, home: &str) -> String {
    let mut s = line.to_string();
    let home = home.trim_end_matches('/');
    if home.len() > 1 {
        s = s.replace(home, "~");
    }
    for base in ["/Users/", "/home/"] {
        let mut out = String::new();
        let mut rest = s.as_str();
        while let Some(at) = rest.find(base) {
            let after = &rest[at + base.len()..];
            let end = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.')))
                .unwrap_or(after.len());
            out.push_str(&rest[..at + base.len()]);
            if end > 0 && &after[..end] != "<user>" {
                out.push_str("<user>");
            } else {
                out.push_str(&after[..end]);
            }
            rest = &after[end..];
        }
        out.push_str(rest);
        s = out;
    }
    s
}

pub fn scan(lines: &[String], home: &str) -> Vec<Finding> {
    let mut found = Vec::new();
    let mut in_key = false;
    let mut key_len = 0usize;
    for (i, line) in lines.iter().enumerate() {
        let mut kind = "";
        let mut keepable = true;
        let mut cleaned = line.clone();
        let t = line.trim();
        if !in_key && t.starts_with("-----BEGIN ") && t.ends_with("-----") {
            in_key = true;
            key_len = 0;
        }
        if in_key {
            key_len += 1;
            if key_len > 200 {
                in_key = false;
            }
        }
        if in_key {
            kind = "private key";
            keepable = false;
            cleaned = "…".into();
            if t.starts_with("-----END ") {
                in_key = false;
            }
        } else {
            let masked = mask_pairs(&scrub(line));
            if masked != *line {
                kind = "secret";
                cleaned = masked;
            }
            let t = line.trim();
            if t.len() >= 60
                && t.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '-' | '_'))
            {
                kind = "key material";
                keepable = false;
                cleaned = "…".into();
            }
            if line.split_whitespace().any(super::summary::token_literal) {
                keepable = false;
            }
            if let Some(env) = env_like(&cleaned) {
                if env != cleaned {
                    kind = if kind.is_empty() { ".env value" } else { kind };
                    cleaned = env;
                }
            }
            let homed = user_dirs(&cleaned, home);
            if homed != cleaned {
                kind = if kind.is_empty() { "home path" } else { kind };
                cleaned = homed;
            }
        }
        if cleaned != *line {
            found.push(Finding {
                line: i,
                kind,
                keepable,
                original: line.clone(),
                cleaned,
            });
        }
    }
    found
}

pub fn render(lines: &[String], findings: &[Finding], decisions: &[Decision]) -> String {
    let mut out: Vec<&str> = lines.iter().map(String::as_str).collect();
    for (f, d) in findings.iter().zip(decisions) {
        if *d != Decision::Keep {
            out[f.line] = &f.cleaned;
        }
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

fn replace_ci(s: &str, needle: &str, with: &str) -> String {
    let lower = s.to_ascii_lowercase();
    let mut out = String::new();
    let mut at = 0;
    while let Some(i) = lower[at..].find(needle) {
        out.push_str(&s[at..at + i]);
        out.push_str(with);
        at += i + needle.len();
    }
    out.push_str(&s[at..]);
    out
}

pub fn escape_markdown(text: &str) -> String {
    let s = text
        .replace('<', "&lt;")
        .replace('@', "&#64;")
        .replace("[[", "[ [")
        .replace("](", "] (");
    replace_ci(&s, "javascript:", "javascript&#58;")
}

pub fn fmt_size(bytes: usize) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else {
        format!("{:.1} MB", bytes as f64 / 1024.0 / 1024.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Review,
    Choose,
    Confirm,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportState {
    pub title: String,
    pub lines: Vec<String>,
    pub findings: Vec<Finding>,
    pub decisions: Vec<Decision>,
    pub cursor: usize,
    pub stage: Stage,
    pub typed: String,
    pub ws_code: Option<String>,
    pub page_refusal: Option<String>,
    pub path: String,
    pub home: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportAction {
    File {
        text: String,
        path: String,
    },
    Page {
        text: String,
        title: String,
        code: String,
    },
}

impl ExportState {
    pub fn new(
        title: String,
        lines: Vec<String>,
        findings: Vec<Finding>,
        ws_code: Option<String>,
        page_refusal: Option<String>,
        path: String,
        home: String,
    ) -> ExportState {
        let stage = if findings.is_empty() {
            Stage::Choose
        } else {
            Stage::Review
        };
        ExportState {
            decisions: vec![Decision::Pending; findings.len()],
            title,
            lines,
            findings,
            cursor: 0,
            stage,
            typed: String::new(),
            ws_code,
            page_refusal,
            path,
            home,
        }
    }

    pub fn pending(&self) -> usize {
        self.decisions
            .iter()
            .filter(|d| **d == Decision::Pending)
            .count()
    }

    pub fn text(&self) -> String {
        render(&self.lines, &self.findings, &self.decisions)
    }

    pub fn ready_text(&self) -> Option<String> {
        if self.pending() > 0 {
            return None;
        }
        let text = self.text();
        let lines: Vec<String> = text.lines().map(str::to_string).collect();
        let again = scan(&lines, &self.home);
        let allowed = |f: &Finding| {
            self.findings.iter().zip(&self.decisions).any(|(o, d)| {
                *d == Decision::Keep && o.line == f.line && o.original == lines[f.line]
            })
        };
        again.iter().all(allowed).then(|| escape_markdown(&text))
    }

    fn decide(&mut self, d: Decision) {
        let keepable = self.findings.get(self.cursor).is_some_and(|f| f.keepable);
        if d == Decision::Keep && !keepable {
            return;
        }
        if let Some(slot) = self.decisions.get_mut(self.cursor) {
            *slot = d;
        }
        if self.cursor + 1 < self.findings.len() {
            self.cursor += 1;
        }
    }

    fn can_page(&self) -> bool {
        self.ws_code.is_some() && self.page_refusal.is_none()
    }
}

const PANEL_W: u16 = 78;
const PANEL_H: u16 = 17;

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

fn buttons(rect: Rect, e: &ExportState) -> Vec<(Rect, &'static [u8], &'static str, bool)> {
    let y = rect.bottom().saturating_sub(3);
    let items: Vec<(&'static [u8], &'static str, bool)> = match e.stage {
        Stage::Review => vec![
            (b"r", "[ r Remove ]", true),
            (b"k", "[ k Keep ]", true),
            (b"A", "[ A Remove all ]", true),
            (b"\r", "[ Enter Continue ]", e.pending() == 0),
            (b"\x1b", "[ Esc Cancel ]", true),
        ],
        Stage::Choose => vec![
            (b"f", "[ f File ]", !e.path.is_empty()),
            (b"g", "[ g Page ]", e.can_page()),
            (b"\x1b", "[ Esc Back ]", true),
        ],
        Stage::Confirm => vec![
            (b"\r", "[ Enter Send ]", true),
            (b"\x1b", "[ Esc Back ]", true),
        ],
    };
    let mut x = rect.x + 2;
    items
        .into_iter()
        .map(|(k, label, on)| {
            let w = label.width() as u16;
            let r = Rect::new(x, y, w.min(rect.right().saturating_sub(x)), 1);
            x += w + 1;
            (r, k, label, on)
        })
        .collect()
}

impl ViewState {
    pub fn open_export(&mut self, state: ExportState) {
        self.export = Some(state);
        self.help = false;
        self.grab = None;
    }

    pub fn edit_export(&mut self, data: &[u8]) -> Option<ExportAction> {
        let e = self.export.as_mut()?;
        match e.stage {
            Stage::Review => match data {
                b"r" | b"R" => e.decide(Decision::Remove),
                b"k" | b"K" => e.decide(Decision::Keep),
                b"A" => {
                    for d in e.decisions.iter_mut().filter(|d| **d == Decision::Pending) {
                        *d = Decision::Remove;
                    }
                }
                b"j" | b"n" | b"\t" | b"\x1b[B" | b"\x1bOB" => {
                    e.cursor = (e.cursor + 1).min(e.findings.len().saturating_sub(1))
                }
                b"p" | b"\x1b[A" | b"\x1bOA" => e.cursor = e.cursor.saturating_sub(1),
                b"\r" | b"c" if e.pending() == 0 => e.stage = Stage::Choose,
                b"\x1b" | b"q" => self.export = None,
                _ => {}
            },
            Stage::Choose => match data {
                b"f" | b"F" => {
                    if e.path.is_empty() {
                        return None;
                    }
                    let Some(text) = e.ready_text() else {
                        if e.findings.is_empty() {
                            self.export = None;
                        } else {
                            e.stage = Stage::Review;
                        }
                        return None;
                    };
                    let path = e.path.clone();
                    self.export = None;
                    return Some(ExportAction::File { text, path });
                }
                b"g" | b"G" if e.can_page() => {
                    e.typed.clear();
                    e.stage = Stage::Confirm;
                }
                b"\x1b" | b"q" => {
                    if e.findings.is_empty() {
                        self.export = None;
                    } else {
                        e.stage = Stage::Review;
                    }
                }
                _ => {}
            },
            Stage::Confirm => match data {
                b"\x1b" => {
                    e.typed.clear();
                    e.stage = Stage::Choose;
                }
                b"\x7f" | b"\x08" => {
                    e.typed.pop();
                }
                b"\r" => {
                    let want = e.ws_code.clone().unwrap_or_default();
                    if !want.is_empty() && e.typed.trim().eq_ignore_ascii_case(&want) {
                        let Some(text) = e.ready_text() else {
                            if e.findings.is_empty() {
                                self.export = None;
                            } else {
                                e.stage = Stage::Review;
                            }
                            return None;
                        };
                        let title = e.title.clone();
                        let code = want.clone();
                        self.export = None;
                        return Some(ExportAction::Page { text, title, code });
                    }
                }
                other if other.first() != Some(&0x1b) => {
                    if let Ok(s) = std::str::from_utf8(other) {
                        let s: String = s
                            .chars()
                            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
                            .collect();
                        let room = 64usize.saturating_sub(e.typed.len());
                        e.typed.extend(s.chars().take(room));
                    }
                }
                _ => {}
            },
        }
        None
    }

    pub(crate) fn export_mouse(
        &mut self,
        area: Rect,
        ev: MouseEvent,
        mut out: MouseOut,
    ) -> MouseOut {
        let rect = panel_rect(area);
        match ev.kind() {
            Kind::Press => {
                self.grab = Some(String::new());
                if !ev.is_left_press() {
                    return out;
                }
                out.redraw = true;
                let Some(e) = self.export.as_ref() else {
                    return out;
                };
                let hit = buttons(rect, e)
                    .into_iter()
                    .find(|(r, _, _, on)| *on && contains(*r, ev.col, ev.row))
                    .map(|(_, k, _, _)| k);
                if let Some(key) = hit {
                    out.export = self.edit_export(key);
                }
            }
            Kind::Release if self.grab.as_deref() == Some("") => self.grab = None,
            _ => {}
        }
        out
    }
}

pub(crate) fn draw_export(buf: &mut Buffer, area: Rect, st: &ViewState) {
    let Some(e) = st.export.as_ref() else {
        return;
    };
    let rect = panel_rect(area);
    if rect.is_empty() {
        return;
    }
    Clear.render(rect, buf);
    let size = fmt_size(e.text().len());
    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(format!(
            " Export session · {size} · {} findings ",
            e.findings.len()
        ))
        .render(rect, buf);
    let inner = rect.inner(ratatui::layout::Margin::new(2, 1));
    if inner.is_empty() {
        return;
    }
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let w = inner.width;
    let mut y = inner.y;
    let mut row = |buf: &mut Buffer, text: &str, style: Style| {
        if y < inner.bottom().saturating_sub(2) {
            put(buf, inner.x, y, text, style, w);
            y += 1;
        }
    };
    match e.stage {
        Stage::Review if e.findings.is_empty() => {}
        Stage::Review => {
            let f = &e.findings[e.cursor.min(e.findings.len() - 1)];
            let d = match e.decisions[e.cursor.min(e.findings.len() - 1)] {
                Decision::Pending => "undecided",
                Decision::Remove => "will be removed",
                Decision::Keep => "kept as is",
            };
            row(
                buf,
                &format!(
                    "Finding {}/{} · line {} · {} · {d}",
                    e.cursor + 1,
                    e.findings.len(),
                    f.line + 1,
                    f.kind
                ),
                bold,
            );
            row(buf, "", Style::new());
            row(buf, "In the text:", dim());
            for l in wrap(&f.original, w as usize).into_iter().take(3) {
                row(buf, &l, Style::new());
            }
            row(buf, "Exported as:", dim());
            for l in wrap(&f.cleaned, w as usize).into_iter().take(3) {
                row(buf, &l, Style::new());
            }
            row(buf, "", Style::new());
            let note = if f.keepable {
                ""
            } else {
                " · this kind cannot be kept"
            };
            row(
                buf,
                &format!("{} still undecided{note}", e.pending()),
                dim(),
            );
        }
        Stage::Choose => {
            let removed = e
                .decisions
                .iter()
                .filter(|d| **d == Decision::Remove)
                .count();
            let kept = e.decisions.iter().filter(|d| **d == Decision::Keep).count();
            row(
                buf,
                &format!(
                    "{} lines, {size}; {removed} findings removed, {kept} kept",
                    e.lines.len()
                ),
                bold,
            );
            row(buf, "", Style::new());
            let file = if e.path.is_empty() {
                "File: not available (the session runs in your home folder)".to_string()
            } else {
                format!("File: {}", e.path)
            };
            row(buf, &file, Style::new());
            let page = match (&e.page_refusal, &e.ws_code) {
                (Some(why), _) => format!("Page: not available ({why})"),
                (None, None) => "Page: no workspace is selected".to_string(),
                (None, Some(code)) => {
                    format!("Page: new page in workspace {code} (this sends the text to the TMGR server)")
                }
            };
            row(buf, &page, Style::new());
        }
        Stage::Confirm => {
            let code = e.ws_code.clone().unwrap_or_default();
            row(
                buf,
                &format!("This sends {size} to the TMGR server, workspace {code}."),
                bold,
            );
            row(buf, "", Style::new());
            row(
                buf,
                &format!("Type the workspace code ({code}) to confirm:"),
                Style::new(),
            );
            row(buf, &format!("> {}▏", e.typed), Style::new());
        }
    }
    let hover = st.pointer;
    for (r, _, label, enabled) in buttons(rect, e) {
        if r.is_empty() {
            continue;
        }
        let over = hover.is_some_and(|(c, row)| contains(r, c, row));
        put(buf, r.x, r.y, label, button_style(over, enabled), r.width);
    }
    let hint = match e.stage {
        Stage::Review => "n next, p previous: every finding needs a decision",
        Stage::Choose => "nothing is written until you choose",
        Stage::Confirm => "Esc goes back; nothing was sent yet",
    };
    put(
        buf,
        inner.x,
        inner.bottom().saturating_sub(1),
        hint,
        dim(),
        inner.width,
    );
}
