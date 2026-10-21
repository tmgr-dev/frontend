use super::mouse::MouseEvent;
use super::palette::{esc_key, next_char, query_tail, Key};
use super::render::{dim, put};
use super::theme::colors;
use super::view::{contains, MouseOut, ViewState};
use crate::sanitize::sanitize_text;
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, BorderType, Clear, Widget};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Place {
    #[default]
    Right,
    Below,
    Replace,
    Tab,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpawnKind {
    Agent { profile: String },
    Shell,
    Custom { line: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindOpt {
    pub id: String,
    pub label: String,
    pub spawn: SpawnKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewRequest {
    pub kind: SpawnKind,
    pub folder: String,
    pub name: Option<String>,
    pub place: Place,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewEdit {
    Editing,
    Submit(NewRequest),
    Cancel,
}

const MAX_FOLDER: usize = 512;
const MAX_COMMAND: usize = 512;
const MAX_NAME: usize = 64;
const MAX_ENTRIES: usize = 500;
const MAX_WIDTH: u16 = 72;
const LABEL_W: u16 = 10;
const START_LABEL: &str = "[ Start ]";

impl Place {
    const ALL: [Place; 4] = [Place::Tab, Place::Right, Place::Below, Place::Replace];

    fn label(self) -> &'static str {
        match self {
            Place::Right => "Right",
            Place::Below => "Below",
            Place::Replace => "Replace",
            Place::Tab => "New tab",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewField {
    Kind,
    Command,
    Folder,
    Name,
    Where,
    Start,
}

#[derive(Clone, Debug)]
pub struct NewSession {
    pub kinds: Vec<KindOpt>,
    pub kind: usize,
    pub folder: String,
    pub command: String,
    pub name: String,
    pub place: Place,
    pub error: Option<String>,
    pub error_hint: Option<String>,
    pub focus: NewField,
    pub taken_names: Vec<String>,
    pub home: PathBuf,
    pub list_dir: fn(&Path) -> Vec<String>,
}

fn strip_control(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).collect()
}

fn list_dirs(path: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    rd.take(MAX_ENTRIES)
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .map(|e| strip_control(&e.file_name().to_string_lossy()))
        .collect()
}

fn expand_home(s: &str, home: &Path) -> PathBuf {
    if s == "~" {
        home.to_path_buf()
    } else if let Some(rest) = s.strip_prefix("~/") {
        home.join(rest)
    } else {
        PathBuf::from(s)
    }
}

fn is_reserved_shell_name(name: &str) -> bool {
    name.strip_prefix("sh-")
        .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
}

fn common_prefix(names: &[&String]) -> String {
    let Some((first, rest)) = names.split_first() else {
        return String::new();
    };
    let mut prefix: Vec<char> = first.chars().collect();
    for n in rest {
        let keep = prefix
            .iter()
            .zip(n.chars())
            .take_while(|(a, b)| **a == *b)
            .count();
        prefix.truncate(keep);
    }
    prefix.into_iter().collect()
}

impl NewSession {
    pub fn with_error(mut self, msg: impl Into<String>, hint: Option<String>) -> Self {
        self.error = Some(msg.into());
        self.error_hint = hint;
        self
    }

    fn is_custom(&self) -> bool {
        self.kinds
            .get(self.kind)
            .is_some_and(|k| matches!(k.spawn, SpawnKind::Custom { .. }))
    }

    fn fields(&self) -> Vec<NewField> {
        let mut f = vec![NewField::Kind];
        if self.is_custom() {
            f.push(NewField::Command);
        }
        f.extend([
            NewField::Folder,
            NewField::Name,
            NewField::Where,
            NewField::Start,
        ]);
        f
    }

    fn clear_error(&mut self) {
        self.error = None;
        self.error_hint = None;
    }

    fn text_field(&mut self) -> Option<(&mut String, usize)> {
        match self.focus {
            NewField::Command => Some((&mut self.command, MAX_COMMAND)),
            NewField::Folder => Some((&mut self.folder, MAX_FOLDER)),
            NewField::Name => Some((&mut self.name, MAX_NAME)),
            _ => None,
        }
    }

    fn push_char(&mut self, c: char) {
        if let Some((s, cap)) = self.text_field() {
            if s.chars().count() < cap {
                s.push(c);
                self.clear_error();
            }
        }
    }

    fn step(&mut self, down: bool) {
        let fields = self.fields();
        let at = fields.iter().position(|f| *f == self.focus).unwrap_or(0);
        let n = fields.len();
        let next = if down { (at + 1) % n } else { (at + n - 1) % n };
        self.focus = fields[next];
    }

    fn cycle(&mut self, forward: bool) {
        match self.focus {
            NewField::Kind => {
                let n = self.kinds.len();
                if n > 0 {
                    self.kind = if forward {
                        (self.kind + 1) % n
                    } else {
                        (self.kind + n - 1) % n
                    };
                    self.clear_error();
                }
            }
            NewField::Where => {
                let at = Place::ALL
                    .iter()
                    .position(|p| *p == self.place)
                    .unwrap_or(0);
                let n = Place::ALL.len();
                let next = if forward {
                    (at + 1) % n
                } else {
                    (at + n - 1) % n
                };
                self.place = Place::ALL[next];
                self.clear_error();
            }
            _ => {}
        }
    }

    fn complete_folder(&mut self) -> bool {
        let Some(slash) = self.folder.rfind('/') else {
            if self.folder == "~" {
                self.folder.push('/');
                return true;
            }
            return false;
        };
        let (dir_part, prefix) = self.folder.split_at(slash + 1);
        let dir = expand_home(dir_part, &self.home);
        if !dir.is_absolute() {
            return false;
        }
        let names: Vec<String> = (self.list_dir)(&dir)
            .iter()
            .map(|n| strip_control(n))
            .filter(|n| !n.is_empty() && n.starts_with(prefix))
            .filter(|n| prefix.starts_with('.') || !n.starts_with('.'))
            .collect();
        let refs: Vec<&String> = names.iter().collect();
        let done = match names.len() {
            0 => return false,
            1 => format!("{dir_part}{}/", names[0]),
            _ => format!("{dir_part}{}", common_prefix(&refs)),
        };
        if done.chars().count() <= MAX_FOLDER && done.len() > self.folder.len() {
            self.folder = done;
            self.clear_error();
            return true;
        }
        false
    }

    fn submit(&mut self) -> Result<NewRequest, ()> {
        match self.validate() {
            Ok(req) => Ok(req),
            Err((field, msg)) => {
                self.focus = field;
                self.error = Some(msg.to_string());
                self.error_hint = None;
                Err(())
            }
        }
    }

    fn validate(&self) -> Result<NewRequest, (NewField, String)> {
        let folder = self.folder.trim();
        if folder.is_empty() {
            return Err((NewField::Folder, "folder is required".into()));
        }
        let path = expand_home(folder, &self.home);
        if !path.is_absolute() {
            return Err((
                NewField::Folder,
                "folder must be an absolute path or start with ~".into(),
            ));
        }
        if !path.is_dir() {
            return Err((NewField::Folder, "no such folder".into()));
        }
        let mut folder = path.to_string_lossy().into_owned();
        while folder.len() > 1 && folder.ends_with('/') {
            folder.pop();
        }
        let kind = self
            .kinds
            .get(self.kind)
            .ok_or((NewField::Kind, "no session kind available".to_string()))?;
        let name = self.name.trim();
        if name.chars().count() > MAX_NAME {
            return Err((
                NewField::Name,
                format!("name is longer than {MAX_NAME} characters"),
            ));
        }
        if name.chars().any(char::is_control) {
            return Err((NewField::Name, "name has control characters".into()));
        }
        if !name.is_empty() {
            if self.taken_names.iter().any(|n| n == name) {
                return Err((
                    NewField::Name,
                    format!("a session is already named \"{name}\""),
                ));
            }
            if is_reserved_shell_name(name) && !matches!(kind.spawn, SpawnKind::Shell) {
                return Err((NewField::Name, "sh-N names are reserved for shells".into()));
            }
        }
        let spawn = match &kind.spawn {
            SpawnKind::Custom { .. } => {
                let line = self.command.trim();
                if line.is_empty() {
                    return Err((NewField::Command, "command is required".into()));
                }
                match shell_words::split(line) {
                    Ok(words) if !words.is_empty() => SpawnKind::Custom {
                        line: line.to_string(),
                    },
                    _ => return Err((NewField::Command, "command has unbalanced quotes".into())),
                }
            }
            other => other.clone(),
        };
        Ok(NewRequest {
            kind: spawn,
            folder,
            name: (!name.is_empty()).then(|| name.to_string()),
            place: self.place,
        })
    }
}

struct Notes {
    error: Vec<String>,
    hint: Vec<String>,
}

impl Notes {
    fn error_h(&self) -> usize {
        self.error.len().max(1)
    }

    fn hint_h(&self) -> usize {
        self.hint.len().max(1)
    }
}

fn wrap(text: &str, width: usize, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut cw = 0usize;
    for word in text.split(' ') {
        let ww = word.width();
        if cw + usize::from(!cur.is_empty()) + ww <= width {
            if !cur.is_empty() {
                cur.push(' ');
                cw += 1;
            }
            cur.push_str(word);
            cw += ww;
            continue;
        }
        if !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
        }
        cw = 0;
        for c in word.chars() {
            let w = c.width().unwrap_or(0);
            if cw + w > width {
                lines.push(std::mem::take(&mut cur));
                cw = 0;
            }
            cur.push(c);
            cw += w;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
        if let Some(last) = lines.last_mut() {
            while last.width() + 1 > width {
                last.pop();
            }
            last.push('\u{2026}');
        }
    }
    lines
}

fn notes(ns: &NewSession, area_width: u16) -> Notes {
    let width = usize::from(MAX_WIDTH.min(area_width))
        .saturating_sub(4)
        .max(1);
    let clean = |s: &Option<String>, max| {
        s.as_deref()
            .map(|t| wrap(&strip_control(&sanitize_text(t)), width, max))
            .unwrap_or_default()
    };
    Notes {
        error: clean(&ns.error, 2),
        hint: clean(&ns.error_hint, 3),
    }
}

pub(crate) fn new_session_rect(area: Rect, ns: &NewSession) -> Rect {
    let w = MAX_WIDTH.min(area.width);
    let n = notes(ns, area.width);
    let h = u16::try_from(ns.fields().len() + 6 + n.error_h() + n.hint_h())
        .unwrap_or(u16::MAX)
        .min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

fn inner(rect: Rect) -> Rect {
    rect.inner(Margin::new(2, 1))
}

fn field_rows(ns: &NewSession, inner: Rect) -> Vec<(NewField, Rect)> {
    ns.fields()
        .into_iter()
        .zip(0u16..)
        .map(|(f, k)| {
            (
                f,
                Rect::new(inner.x, inner.y.saturating_add(k), inner.width, 1),
            )
        })
        .filter(|(_, r)| r.y < inner.bottom())
        .collect()
}

fn value_x(row: Rect) -> u16 {
    row.x.saturating_add(LABEL_W)
}

fn value_avail(row: Rect) -> usize {
    usize::from(row.width.saturating_sub(LABEL_W))
}

fn option_spans(labels: &[String], sel: usize, avail: usize) -> Vec<(usize, u16, u16)> {
    let widths: Vec<usize> = labels.iter().map(|l| l.width() + 2).collect();
    let mut start = sel.min(labels.len().saturating_sub(1));
    while start > 0 && widths[start - 1..=sel].iter().sum::<usize>() + (sel - start + 1) <= avail {
        start -= 1;
    }
    let mut out = Vec::new();
    let mut x = 0usize;
    for (i, w) in widths.iter().enumerate().skip(start) {
        if x + w > avail {
            break;
        }
        out.push((i, x as u16, *w as u16));
        x += w + 1;
    }
    out
}

fn option_labels(ns: &NewSession, field: NewField) -> (Vec<String>, usize) {
    match field {
        NewField::Kind => (
            ns.kinds.iter().map(|k| strip_control(&k.label)).collect(),
            ns.kind,
        ),
        _ => (
            Place::ALL.iter().map(|p| p.label().to_string()).collect(),
            Place::ALL.iter().position(|p| *p == ns.place).unwrap_or(0),
        ),
    }
}

fn field_label(f: NewField) -> &'static str {
    match f {
        NewField::Kind => "Kind",
        NewField::Command => "Command",
        NewField::Folder => "Folder",
        NewField::Name => "Name",
        NewField::Where => "Where",
        NewField::Start => "",
    }
}

fn field_text(ns: &NewSession, f: NewField) -> Option<&str> {
    match f {
        NewField::Command => Some(&ns.command),
        NewField::Folder => Some(&ns.folder),
        NewField::Name => Some(&ns.name),
        _ => None,
    }
}

pub(crate) fn draw_new_session(buf: &mut Buffer, area: Rect, st: &ViewState) {
    let Some(ns) = &st.new_session else {
        return;
    };
    let rect = new_session_rect(area, ns);
    if rect.is_empty() {
        return;
    }
    Clear.render(rect, buf);
    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(" New session ")
        .render(rect, buf);
    let inner = inner(rect);
    if inner.is_empty() {
        return;
    }
    let rows = field_rows(ns, inner);
    for (f, row) in &rows {
        let focused = *f == ns.focus;
        let marker = if focused { "> " } else { "  " };
        let label_style = if focused {
            Style::new().add_modifier(Modifier::BOLD)
        } else {
            dim()
        };
        put(buf, row.x, row.y, marker, label_style, row.width);
        put(
            buf,
            row.x + 2,
            row.y,
            field_label(*f),
            label_style,
            row.width.saturating_sub(2),
        );
        let x = value_x(*row);
        let avail = value_avail(*row);
        match f {
            NewField::Kind | NewField::Where => {
                let (labels, sel) = option_labels(ns, *f);
                for (i, off, w) in option_spans(&labels, sel, avail) {
                    let style = if i == sel {
                        Style::new().add_modifier(Modifier::REVERSED)
                    } else {
                        Style::new()
                    };
                    put(buf, x + off, row.y, &format!(" {} ", labels[i]), style, w);
                }
            }
            NewField::Start => {
                let style = if focused {
                    Style::new().add_modifier(Modifier::REVERSED)
                } else {
                    Style::new()
                };
                put(
                    buf,
                    x,
                    row.y,
                    START_LABEL,
                    style,
                    row.right().saturating_sub(x),
                );
            }
            _ => {
                let text = strip_control(field_text(ns, *f).unwrap_or(""));
                let tail = query_tail(&text, avail.saturating_sub(1));
                put(
                    buf,
                    x,
                    row.y,
                    tail,
                    Style::new(),
                    row.right().saturating_sub(x),
                );
            }
        }
    }
    let base = u16::try_from(ns.fields().len()).unwrap_or(u16::MAX);
    let line = |k: usize| {
        inner
            .y
            .saturating_add(base)
            .saturating_add(u16::try_from(k).unwrap_or(u16::MAX))
    };
    let bottom = inner.bottom();
    let n = notes(ns, area.width);
    for (i, text) in n.error.iter().enumerate() {
        if line(1 + i) < bottom {
            put(
                buf,
                inner.x,
                line(1 + i),
                text,
                Style::new().fg(colors().bad),
                inner.width,
            );
        }
    }
    let hint_at = 1 + n.error_h();
    for (i, text) in n.hint.iter().enumerate() {
        if line(hint_at + i) < bottom {
            put(buf, inner.x, line(hint_at + i), text, dim(), inner.width);
        }
    }
    let help_at = hint_at + n.hint_h();
    if line(help_at) < bottom {
        put(
            buf,
            inner.x,
            line(help_at),
            "Enter starts, Esc cancels",
            dim(),
            inner.width,
        );
    }
}

pub(crate) fn new_session_cursor(ns: &NewSession, area: Rect) -> Option<(u16, u16)> {
    let rect = new_session_rect(area, ns);
    let inner = inner(rect);
    if rect.is_empty() || inner.is_empty() {
        return None;
    }
    let (_, row) = field_rows(ns, inner)
        .into_iter()
        .find(|(f, _)| *f == ns.focus)?;
    let text = strip_control(field_text(ns, ns.focus)?);
    let avail = value_avail(row);
    let w = query_tail(&text, avail.saturating_sub(1)).width();
    let x = value_x(row).saturating_add(w as u16);
    Some((x.min(row.right().saturating_sub(1)), row.y))
}

impl ViewState {
    pub fn open_new_session(&mut self, folder: String, kinds: Vec<KindOpt>, home: PathBuf) {
        let kind = self
            .new_kind
            .as_ref()
            .and_then(|id| kinds.iter().position(|k| &k.id == id))
            .unwrap_or(0);
        self.palette = None;
        self.help = false;
        self.confirm = None;
        self.rename = None;
        self.sidebar_focus = None;
        self.new_session = Some(NewSession {
            kinds,
            kind,
            folder,
            command: String::new(),
            name: String::new(),
            place: self.new_place.unwrap_or_default(),
            error: None,
            error_hint: None,
            focus: NewField::Kind,
            taken_names: self.sessions.iter().map(|s| s.name.clone()).collect(),
            home,
            list_dir: list_dirs,
        });
    }

    pub fn reopen_new_session(&mut self, ns: NewSession) {
        self.new_session = Some(ns);
    }

    pub fn remember_new_session(&mut self, req: &NewRequest, kind_id: &str) {
        self.new_kind = Some(kind_id.to_string());
        self.new_place = Some(req.place);
    }

    pub fn edit_new_session(&mut self, data: &[u8]) -> NewEdit {
        if self.new_session.is_none() {
            return NewEdit::Cancel;
        }
        if !data.is_empty() && data.iter().all(|&b| b == 0x1b) {
            self.new_session = None;
            return NewEdit::Cancel;
        }
        let mut i = 0;
        while i < data.len() {
            let Some(ns) = self.new_session.as_mut() else {
                break;
            };
            match data[i] {
                0x1b => {
                    let (key, used) = esc_key(&data[i..]);
                    let seq = &data[i..i + used];
                    i += used;
                    match key {
                        Key::Up => ns.step(false),
                        Key::Down => ns.step(true),
                        Key::Other => match seq {
                            b"\x1b[C" | b"\x1bOC" => ns.cycle(true),
                            b"\x1b[D" | b"\x1bOD" => ns.cycle(false),
                            b"\x1b[Z" => ns.step(false),
                            _ => {}
                        },
                    }
                }
                b'\t' => {
                    i += 1;
                    if ns.focus != NewField::Folder || !ns.complete_folder() {
                        ns.step(true);
                    }
                }
                b'\r' | b'\n' => {
                    i += 1;
                    if let Ok(req) = ns.submit() {
                        self.new_session = None;
                        return NewEdit::Submit(req);
                    }
                }
                0x7f | 0x08 => {
                    i += 1;
                    if let Some((s, _)) = ns.text_field() {
                        s.pop();
                        ns.clear_error();
                    }
                }
                0x15 => {
                    i += 1;
                    if let Some((s, _)) = ns.text_field() {
                        s.clear();
                        ns.clear_error();
                    }
                }
                _ => {
                    let (c, used) = next_char(&data[i..]);
                    i += used;
                    if let Some(c) = c.filter(|c| !c.is_control()) {
                        ns.push_char(c);
                    }
                }
            }
        }
        if self.new_session.is_some() {
            NewEdit::Editing
        } else {
            NewEdit::Cancel
        }
    }

    pub fn insert_new_session_text(&mut self, data: &[u8]) {
        let Some(ns) = self.new_session.as_mut() else {
            return;
        };
        let text = String::from_utf8_lossy(data)
            .replace("\x1b[200~", "")
            .replace("\x1b[201~", "");
        for c in text.chars().filter(|c| !c.is_control()) {
            ns.push_char(c);
        }
    }

    pub(crate) fn new_session_mouse(
        &mut self,
        area: Rect,
        ev: MouseEvent,
        mut out: MouseOut,
    ) -> MouseOut {
        let Some(ns) = self.new_session.as_mut() else {
            return out;
        };
        if !ev.is_left_press() {
            return out;
        }
        out.redraw = true;
        let rect = new_session_rect(area, ns);
        if !contains(rect, ev.col, ev.row) {
            self.new_session = None;
            out.new_edit = Some(NewEdit::Cancel);
            return out;
        }
        let Some((field, row)) = field_rows(ns, inner(rect))
            .into_iter()
            .find(|(_, r)| r.y == ev.row)
        else {
            return out;
        };
        ns.focus = field;
        let x = value_x(row);
        let avail = value_avail(row);
        match field {
            NewField::Kind | NewField::Where => {
                let (labels, sel) = option_labels(ns, field);
                let hit = option_spans(&labels, sel, avail)
                    .into_iter()
                    .find(|(_, off, w)| ev.col >= x + off && ev.col < x + off + w);
                if let Some((i, _, _)) = hit {
                    if field == NewField::Kind {
                        ns.kind = i;
                    } else {
                        ns.place = Place::ALL[i];
                    }
                    ns.clear_error();
                }
            }
            NewField::Start => {
                let w = START_LABEL.width() as u16;
                if ev.col >= x && ev.col < x + w {
                    out.new_edit = Some(match ns.submit() {
                        Ok(req) => {
                            self.new_session = None;
                            NewEdit::Submit(req)
                        }
                        Err(()) => NewEdit::Editing,
                    });
                }
            }
            _ => {}
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attach::view::SessionRow;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tmgr_ipc::api::{State, StateKind};

    fn kinds() -> Vec<KindOpt> {
        let k = |id: &str, spawn| KindOpt {
            id: id.into(),
            label: id.into(),
            spawn,
        };
        vec![
            k(
                "claude",
                SpawnKind::Agent {
                    profile: "claude".into(),
                },
            ),
            k(
                "codex",
                SpawnKind::Agent {
                    profile: "codex".into(),
                },
            ),
            k("shell", SpawnKind::Shell),
            k(
                "custom",
                SpawnKind::Custom {
                    line: String::new(),
                },
            ),
        ]
    }

    fn open(folder: &str, home: &Path) -> ViewState {
        let mut v = ViewState::new(Vec::new(), "r");
        v.open_new_session(folder.into(), kinds(), home.to_path_buf());
        v
    }

    fn ns(v: &mut ViewState) -> &mut NewSession {
        v.new_session.as_mut().unwrap()
    }

    fn submit_err(v: &mut ViewState) -> String {
        assert_eq!(v.edit_new_session(b"\r"), NewEdit::Editing);
        v.new_session.as_ref().unwrap().error.clone().unwrap()
    }

    fn tmp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn press(col: u16, row: u16) -> MouseEvent {
        MouseEvent {
            cb: 0,
            col,
            row,
            release: false,
        }
    }

    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 100,
        height: 30,
    };

    fn click(v: &mut ViewState, col: u16, row: u16) -> MouseOut {
        v.on_mouse(AREA, press(col, row), &|_| Default::default())
    }

    fn row_of(v: &mut ViewState, f: NewField) -> Rect {
        let ns = v.new_session.as_ref().unwrap();
        let rect = new_session_rect(AREA, ns);
        field_rows(ns, inner(rect))
            .into_iter()
            .find(|(g, _)| *g == f)
            .unwrap()
            .1
    }

    fn draw(v: &ViewState, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| {
            let area = f.area();
            draw_new_session(f.buffer_mut(), area, v)
        })
        .unwrap();
        let buf = t.backend().buffer().clone();
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn defaults_and_remembered_choices() {
        let h = tmp();
        let mut v = open("~/x", h.path());
        assert_eq!(ns(&mut v).kind, 0);
        assert_eq!(ns(&mut v).place, Place::Right);
        assert_eq!(ns(&mut v).folder, "~/x");
        let req = NewRequest {
            kind: SpawnKind::Shell,
            folder: "/".into(),
            name: None,
            place: Place::Below,
        };
        v.remember_new_session(&req, "shell");
        let p = v.prefs();
        assert_eq!(p.new_kind.as_deref(), Some("shell"));
        let mut w = ViewState::new(Vec::new(), "r");
        w.apply_prefs(&p);
        w.open_new_session("/".into(), kinds(), h.path().to_path_buf());
        assert_eq!(ns(&mut w).kind, 2);
        assert_eq!(ns(&mut w).place, Place::Below);
        w.new_kind = Some("gone".into());
        w.open_new_session("/".into(), kinds(), h.path().to_path_buf());
        assert_eq!(ns(&mut w).kind, 0);
    }

    #[test]
    fn taken_names_come_from_sessions_and_dialog_clears_other_modals() {
        let mut v = ViewState::new(
            vec![SessionRow {
                id: "a".into(),
                name: "alpha".into(),
                state: State {
                    kind: StateKind::Working,
                    since_ms: 0,
                    detail: None,
                    exit_code: None,
                },
                age: String::new(),
                you: None,
                esc: false,
                resumed_from: None,
            }],
            "r",
        );
        v.help = true;
        v.palette = Some(Default::default());
        v.open_new_session("/".into(), kinds(), PathBuf::from("/"));
        assert!(!v.help && v.palette.is_none());
        assert_eq!(ns(&mut v).taken_names, vec!["alpha".to_string()]);
    }

    #[test]
    fn kind_cycling_shows_command_only_for_custom() {
        let h = tmp();
        let mut v = open("/", h.path());
        assert!(!ns(&mut v).fields().contains(&NewField::Command));
        v.edit_new_session(b"\x1b[D");
        assert_eq!(ns(&mut v).kind, 3);
        assert!(ns(&mut v).fields().contains(&NewField::Command));
        v.edit_new_session(b"\x1b[B");
        assert_eq!(ns(&mut v).focus, NewField::Command);
        v.edit_new_session(b"\x1b[A\x1b[C");
        assert_eq!(ns(&mut v).kind, 0);
        v.edit_new_session(b"\x1b[B");
        assert_eq!(ns(&mut v).focus, NewField::Folder);
    }

    #[test]
    fn where_cycles_and_tab_moves_between_fields() {
        let h = tmp();
        let mut v = open("/", h.path());
        v.edit_new_session(b"\x1b[B\x1b[B\x1b[B");
        assert_eq!(ns(&mut v).focus, NewField::Where);
        v.edit_new_session(b"\x1b[C");
        assert_eq!(ns(&mut v).place, Place::Below);
        v.edit_new_session(b"\x1b[D\x1b[D");
        assert_eq!(ns(&mut v).place, Place::Tab);
        v.edit_new_session(b"\x1b[D");
        assert_eq!(ns(&mut v).place, Place::Replace);
        v.edit_new_session(b"\t");
        assert_eq!(ns(&mut v).focus, NewField::Start);
        v.edit_new_session(b"\x1b[Z");
        assert_eq!(ns(&mut v).focus, NewField::Where);
    }

    #[test]
    fn folder_validation() {
        let h = tmp();
        let file = h.path().join("f");
        std::fs::write(&file, "x").unwrap();
        let mut v = open("", h.path());
        assert_eq!(submit_err(&mut v), "folder is required");
        assert_eq!(ns(&mut v).focus, NewField::Folder);
        ns(&mut v).folder = "relative/dir".into();
        assert_eq!(
            submit_err(&mut v),
            "folder must be an absolute path or start with ~"
        );
        ns(&mut v).folder = h.path().join("absent").to_string_lossy().into_owned();
        assert_eq!(submit_err(&mut v), "no such folder");
        ns(&mut v).folder = file.to_string_lossy().into_owned();
        assert_eq!(submit_err(&mut v), "no such folder");
    }

    #[test]
    fn tilde_expands_with_injected_home_and_submit_closes() {
        let h = tmp();
        std::fs::create_dir(h.path().join("proj")).unwrap();
        let mut v = open("~/proj/", h.path());
        ns(&mut v).name = "  work ".into();
        v.edit_new_session(b"\x1b[B\x1b[B\x1b[B\x1b[C");
        let NewEdit::Submit(req) = v.edit_new_session(b"\r") else {
            panic!("expected submit");
        };
        assert_eq!(req.folder, h.path().join("proj").to_string_lossy());
        assert_eq!(req.name.as_deref(), Some("work"));
        assert_eq!(req.place, Place::Below);
        assert_eq!(
            req.kind,
            SpawnKind::Agent {
                profile: "claude".into()
            }
        );
        assert!(v.new_session.is_none());
        let mut v = open("~", h.path());
        let NewEdit::Submit(req) = v.edit_new_session(b"\r") else {
            panic!("expected submit");
        };
        assert_eq!(req.folder, h.path().to_string_lossy());
        assert_eq!(req.name, None);
    }

    #[test]
    fn name_validation() {
        let h = tmp();
        let mut v = open("~", h.path());
        ns(&mut v).taken_names = vec!["dup".into()];
        ns(&mut v).name = "x".repeat(65);
        assert!(submit_err(&mut v).contains("longer than 64"));
        ns(&mut v).name = "dup".into();
        assert_eq!(submit_err(&mut v), "a session is already named \"dup\"");
        ns(&mut v).name = "sh-12".into();
        assert_eq!(submit_err(&mut v), "sh-N names are reserved for shells");
        ns(&mut v).name = "sh-x".into();
        assert!(matches!(v.edit_new_session(b"\r"), NewEdit::Submit(_)));
        let mut v = open("~", h.path());
        ns(&mut v).kind = 2;
        ns(&mut v).name = "sh-12".into();
        assert!(matches!(v.edit_new_session(b"\r"), NewEdit::Submit(_)));
    }

    #[test]
    fn custom_command_validation() {
        let h = tmp();
        let mut v = open("~", h.path());
        ns(&mut v).kind = 3;
        assert_eq!(submit_err(&mut v), "command is required");
        assert_eq!(ns(&mut v).focus, NewField::Command);
        ns(&mut v).command = "echo 'oops".into();
        assert_eq!(submit_err(&mut v), "command has unbalanced quotes");
        ns(&mut v).command = " htop -d 5 ".into();
        let NewEdit::Submit(req) = v.edit_new_session(b"\r") else {
            panic!("expected submit");
        };
        assert_eq!(
            req.kind,
            SpawnKind::Custom {
                line: "htop -d 5".into()
            }
        );
    }

    #[test]
    fn typing_editing_caps_and_error_reset() {
        let h = tmp();
        let mut v = open("", h.path());
        submit_err(&mut v);
        v.edit_new_session("é\u{7f}ab".as_bytes());
        assert_eq!(ns(&mut v).folder, "ab");
        assert!(ns(&mut v).error.is_none());
        v.edit_new_session(b"\x15");
        assert_eq!(ns(&mut v).folder, "");
        v.edit_new_session("я\u{8}я\x07".as_bytes());
        assert_eq!(ns(&mut v).folder, "я");
        v.edit_new_session("z".repeat(600).as_bytes());
        assert_eq!(ns(&mut v).folder.chars().count(), MAX_FOLDER);
        ns(&mut v).focus = NewField::Name;
        v.edit_new_session("n".repeat(100).as_bytes());
        assert_eq!(ns(&mut v).name.chars().count(), MAX_NAME);
        ns(&mut v).focus = NewField::Kind;
        v.edit_new_session(b"qq");
        assert_eq!(ns(&mut v).kind, 0);
    }

    #[test]
    fn several_keys_in_one_chunk_and_escape_handling() {
        let h = tmp();
        let mut v = open("~", h.path());
        let out = v.edit_new_session(b"\x1b[B\x1b[B\x1b[B\x1b[B\r");
        assert!(matches!(out, NewEdit::Submit(_)));
        let mut v = open("~", h.path());
        v.edit_new_session(b"\x1b[B\x1b[B\x1bOAx");
        assert_eq!(ns(&mut v).focus, NewField::Folder);
        assert_eq!(ns(&mut v).folder, "~x");
        let mut v = open("~", h.path());
        v.edit_new_session(b"\x1bOA");
        assert_eq!(ns(&mut v).folder, "~");
        assert_eq!(v.edit_new_session(b"\x1b"), NewEdit::Cancel);
        assert!(v.new_session.is_none());
        let mut v = open("~", h.path());
        assert_eq!(v.edit_new_session(b"\x1b\x1b"), NewEdit::Cancel);
    }

    #[test]
    fn paste_never_submits_and_strips_controls() {
        let h = tmp();
        let mut v = open("", h.path());
        ns(&mut v).focus = NewField::Folder;
        v.insert_new_session_text(b"\x1b[200~/a/b\r\nc\x07d\x1b[201~");
        assert_eq!(ns(&mut v).folder, "/a/bcd");
        assert!(v.new_session.is_some());
        ns(&mut v).focus = NewField::Where;
        v.insert_new_session_text(b"zzz");
        assert_eq!(ns(&mut v).folder, "/a/bcd");
    }

    fn fake_dirs(_: &Path) -> Vec<String> {
        vec![
            "alpha".into(),
            "alps".into(),
            "beta".into(),
            ".hidden".into(),
            "x\u{7}y".into(),
        ]
    }

    fn tab_in_folder(v: &mut ViewState, folder: &str) {
        ns(v).folder = folder.into();
        ns(v).focus = NewField::Folder;
        v.edit_new_session(b"\t");
    }

    #[test]
    fn tab_completion_single_multiple_none() {
        let h = tmp();
        let mut v = open("/r/b", h.path());
        ns(&mut v).list_dir = fake_dirs;
        tab_in_folder(&mut v, "/r/b");
        assert_eq!(ns(&mut v).folder, "/r/beta/");
        assert_eq!(ns(&mut v).focus, NewField::Folder);
        tab_in_folder(&mut v, "/r/a");
        assert_eq!(ns(&mut v).folder, "/r/alp");
        assert_eq!(ns(&mut v).focus, NewField::Folder);
        tab_in_folder(&mut v, "/r/.h");
        assert_eq!(ns(&mut v).folder, "/r/.hidden/");
        tab_in_folder(&mut v, "/r/x");
        assert_eq!(ns(&mut v).folder, "/r/xy/");
        tab_in_folder(&mut v, "~");
        assert_eq!(ns(&mut v).folder, "~/");
        assert_eq!(ns(&mut v).focus, NewField::Folder);
    }

    #[test]
    fn tab_moves_on_when_completion_changes_nothing() {
        let h = tmp();
        let mut v = open("/r", h.path());
        ns(&mut v).list_dir = fake_dirs;
        tab_in_folder(&mut v, "/r/zz");
        assert_eq!(ns(&mut v).folder, "/r/zz");
        assert_eq!(ns(&mut v).focus, NewField::Name);
        tab_in_folder(&mut v, "/r/alp");
        assert_eq!(ns(&mut v).focus, NewField::Name);
        tab_in_folder(&mut v, "relative");
        assert_eq!(ns(&mut v).focus, NewField::Name);
        tab_in_folder(&mut v, "");
        assert_eq!(ns(&mut v).focus, NewField::Name);
    }

    #[test]
    fn tab_completion_expands_home_and_lists_real_dirs() {
        let h = tmp();
        std::fs::create_dir(h.path().join("projects")).unwrap();
        std::fs::write(h.path().join("pfile"), "x").unwrap();
        let mut v = open("~/p", h.path());
        ns(&mut v).focus = NewField::Folder;
        v.edit_new_session(b"\t");
        assert_eq!(ns(&mut v).folder, "~/projects/");
    }

    #[test]
    fn mouse_focus_select_start_and_outside() {
        let h = tmp();
        let mut v = open("~", h.path());
        let kind = row_of(&mut v, NewField::Kind);
        let (labels, sel) = option_labels(v.new_session.as_ref().unwrap(), NewField::Kind);
        let spans = option_spans(&labels, sel, value_avail(kind));
        let (_, off, _) = spans[2];
        let out = click(&mut v, value_x(kind) + off + 1, kind.y);
        assert!(out.redraw && out.new_edit.is_none());
        assert_eq!(ns(&mut v).kind, 2);
        assert_eq!(ns(&mut v).focus, NewField::Kind);
        let folder = row_of(&mut v, NewField::Folder);
        click(&mut v, folder.x + 15, folder.y);
        assert_eq!(ns(&mut v).focus, NewField::Folder);
        let wh = row_of(&mut v, NewField::Where);
        let (labels, sel) = option_labels(v.new_session.as_ref().unwrap(), NewField::Where);
        let (_, off, _) = option_spans(&labels, sel, value_avail(wh))[2];
        click(&mut v, value_x(wh) + off + 1, wh.y);
        assert_eq!(ns(&mut v).place, Place::Below);
        let start = row_of(&mut v, NewField::Start);
        let out = click(&mut v, value_x(start) + 2, start.y);
        let Some(NewEdit::Submit(req)) = out.new_edit else {
            panic!("expected submit");
        };
        assert_eq!(req.kind, SpawnKind::Shell);
        assert_eq!(req.place, Place::Below);
        assert!(v.new_session.is_none());
    }

    #[test]
    fn mouse_start_with_error_stays_and_outside_cancels() {
        let h = tmp();
        let mut v = open("", h.path());
        let start = row_of(&mut v, NewField::Start);
        let out = click(&mut v, value_x(start) + 1, start.y);
        assert_eq!(out.new_edit, Some(NewEdit::Editing));
        assert!(ns(&mut v).error.is_some());
        let rect = new_session_rect(AREA, v.new_session.as_ref().unwrap());
        let out = click(&mut v, rect.x + 1, rect.y + 1);
        assert!(out.new_edit.is_none() && out.forward.is_none() && out.control.is_none());
        assert!(v.new_session.is_some());
        let wheel = MouseEvent {
            cb: 64,
            col: 0,
            row: 0,
            release: false,
        };
        let out = v.on_mouse(AREA, wheel, &|_| Default::default());
        assert!(out.new_edit.is_none() && v.new_session.is_some());
        let out = click(&mut v, 0, 0);
        assert_eq!(out.new_edit, Some(NewEdit::Cancel));
        assert!(v.new_session.is_none());
    }

    #[test]
    fn render_shows_fields_error_and_hint() {
        let h = tmp();
        let mut v = open("~/code", h.path());
        let s = draw(&v, 100, 30);
        for want in [
            "New session",
            "Kind",
            "claude",
            "Folder",
            "~/code",
            "Name",
            "Where",
            "Right",
            "[ Start ]",
            "Enter starts, Esc cancels",
        ] {
            assert!(s.contains(want), "missing {want}\n{s}");
        }
        assert!(!s.contains("Command"));
        ns(&mut v).kind = 3;
        let s = draw(&v, 100, 30);
        assert!(s.contains("Command"));
        let ns0 = v.new_session.take().unwrap();
        v.reopen_new_session(ns0.with_error("boom", Some("Run it yourself: tmgr run x".into())));
        let s = draw(&v, 100, 30);
        assert!(s.contains("boom") && s.contains("Run it yourself: tmgr run x"));
        let s = draw(&v, 30, 8);
        assert!(s.contains("New session"));
    }

    #[test]
    fn cursor_follows_the_active_text_field() {
        let h = tmp();
        let mut v = open("~/ab", h.path());
        assert_eq!(
            new_session_cursor(v.new_session.as_ref().unwrap(), AREA),
            None
        );
        ns(&mut v).focus = NewField::Folder;
        let row = row_of(&mut v, NewField::Folder);
        assert_eq!(
            new_session_cursor(v.new_session.as_ref().unwrap(), AREA),
            Some((value_x(row) + 4, row.y))
        );
        ns(&mut v).folder = "x".repeat(300);
        let (x, _) = new_session_cursor(v.new_session.as_ref().unwrap(), AREA).unwrap();
        assert!(x < row.right());
    }

    fn long_hint() -> String {
        "tmgr run --agent-cmd 'some-tool --flag value' --cwd '/very/long/path/to/some/project/folder' --name 'my agent'".into()
    }

    #[test]
    fn wrap_breaks_at_spaces_hard_splits_and_truncates() {
        assert_eq!(wrap("aa bb cc", 5, 3), ["aa bb", "cc"]);
        assert_eq!(wrap("abcdefgh", 3, 5), ["abc", "def", "gh"]);
        let cut = wrap("aaaa bbbb cccc dddd", 4, 2);
        assert_eq!(cut.len(), 2);
        assert!(cut[1].ends_with('\u{2026}') && cut[1].width() <= 4);
        assert!(wrap("", 4, 2).is_empty());
        for l in wrap(&long_hint(), 68, 3) {
            assert!(l.width() <= 68);
        }
    }

    #[test]
    fn a_long_hint_wraps_over_lines_and_stays_exact() {
        let h = tmp();
        let mut v = open("~/code", h.path());
        let ns0 = v.new_session.take().unwrap();
        v.reopen_new_session(ns0.with_error("boom", Some(long_hint())));
        let n = notes(v.new_session.as_ref().unwrap(), 100);
        assert!(n.hint.len() >= 2 && n.hint.len() <= 3);
        assert_eq!(n.hint.join(" "), long_hint());
        let s = draw(&v, 100, 30);
        assert!(s.contains("Enter starts, Esc cancels"));
        for l in &n.hint {
            assert!(s.contains(l.as_str()), "missing {l}\n{s}");
        }
    }

    #[test]
    fn field_rows_and_mouse_hits_stay_put_with_a_long_hint() {
        let h = tmp();
        let mut v = open("~/code", h.path());
        let (before_rect, before) = {
            let ns = v.new_session.as_ref().unwrap();
            let rect = new_session_rect(AREA, ns);
            (rect, field_rows(ns, inner(rect)))
        };
        let ns0 = v.new_session.take().unwrap();
        v.reopen_new_session(ns0.with_error("e ".repeat(80), Some(long_hint())));
        let ns = v.new_session.as_ref().unwrap();
        let rect = new_session_rect(AREA, ns);
        let after = field_rows(ns, inner(rect));
        assert_eq!(after.len(), before.len());
        assert!(rect.height > 12);
        for ((f, a), (g, b)) in after.iter().zip(&before) {
            assert_eq!(f, g);
            assert_eq!(a.width, b.width);
            assert_eq!(a.y - rect.y, b.y - before_rect.y);
        }
        let folder = row_of(&mut v, NewField::Folder);
        click(&mut v, folder.x + 15, folder.y);
        assert_eq!(ns_of(&v).focus, NewField::Folder);
        let start = row_of(&mut v, NewField::Start);
        let out = click(&mut v, value_x(start) + 2, start.y);
        assert!(matches!(out.new_edit, Some(NewEdit::Submit(_))) || v.new_session.is_some());
        let below = rect.bottom() - 2;
        let out = click(&mut v, rect.x + 5, below);
        assert!(out.new_edit.is_none() && v.new_session.is_some());
    }

    fn ns_of(v: &ViewState) -> &NewSession {
        v.new_session.as_ref().unwrap()
    }

    #[test]
    fn tiny_sizes_never_panic() {
        let h = tmp();
        let mut v = open("~/code", h.path());
        let ns0 = v.new_session.take().unwrap();
        v.reopen_new_session(ns0.with_error("boom\x1b[31m", Some(long_hint())));
        for (w, hh) in [(1u16, 1u16), (10, 3), (12, 4), (30, 5)] {
            let area = Rect::new(0, 0, w, hh);
            draw(&v, w, hh);
            let _ = new_session_cursor(ns_of(&v), area);
            for (c, r) in [
                (0, 0),
                (w / 2, hh / 2),
                (w.saturating_sub(1), hh.saturating_sub(1)),
            ] {
                let mut vv = v.clone();
                vv.on_mouse(area, press(c, r), &|_| Default::default());
            }
        }
    }
}
