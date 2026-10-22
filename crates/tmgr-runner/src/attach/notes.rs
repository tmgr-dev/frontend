use super::mouse::{Kind, MouseEvent};
use super::render::{button_style, dim, put};
use super::summary::wrap;
use super::view::{contains, MouseOut, ViewState};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, BorderType, Clear, Widget};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const MAX_LINES: usize = 2000;
const MAX_LINE: usize = 1000;
const MAX_BYTES: usize = 64 * 1024;
const PANEL_W: u16 = 50;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotesTask {
    pub task_id: u64,
    pub workspace: Option<String>,
    pub key: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NotesState {
    pub session: String,
    pub name: String,
    pub lines: Vec<String>,
    pub row: usize,
    pub col: usize,
    pub task: Option<NotesTask>,
    pub preview: Option<String>,
    pub note: String,
    pub unsaved: bool,
    pub sending: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotesAction {
    Save { session: String, text: String },
    Append { task: NotesTask, text: String },
}

fn clean_name(name: &str) -> String {
    name.chars()
        .filter(|c| !crate::sanitize::is_unsafe(*c))
        .take(80)
        .collect()
}

impl NotesState {
    pub fn new(session: &str, name: &str, text: &str, task: Option<NotesTask>) -> NotesState {
        let mut state = NotesState {
            session: session.to_string(),
            name: clean_name(name),
            lines: vec![String::new()],
            row: 0,
            col: 0,
            task,
            preview: None,
            note: String::new(),
            unsaved: false,
            sending: false,
        };
        state.insert_text(text);
        state
    }

    pub fn text(&self) -> String {
        self.lines.join("\n").trim_end().to_string()
    }

    pub fn block(&self) -> String {
        format!("### Session notes: {}\n\n{}", self.name, self.text())
    }

    fn bytes(&self) -> usize {
        self.lines.iter().map(|l| l.len() + 1).sum()
    }

    fn byte_at(&self) -> usize {
        self.lines[self.row]
            .char_indices()
            .nth(self.col)
            .map_or(self.lines[self.row].len(), |(i, _)| i)
    }

    fn insert_text(&mut self, text: &str) -> bool {
        let mut changed = false;
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        for ch in text.chars() {
            if ch == '\n' {
                if self.lines.len() < MAX_LINES {
                    let at = self.byte_at();
                    let rest = self.lines[self.row].split_off(at);
                    self.lines.insert(self.row + 1, rest);
                    self.row += 1;
                    self.col = 0;
                    changed = true;
                }
                continue;
            }
            let ch = if ch == '\t' { ' ' } else { ch };
            if crate::sanitize::is_unsafe(ch)
                || self.lines[self.row].chars().count() >= MAX_LINE
                || self.bytes() + ch.len_utf8() > MAX_BYTES
            {
                continue;
            }
            let at = self.byte_at();
            self.lines[self.row].insert(at, ch);
            self.col += 1;
            changed = true;
        }
        changed
    }

    fn backspace(&mut self) -> bool {
        if self.col > 0 {
            self.col -= 1;
            let at = self.byte_at();
            self.lines[self.row].remove(at);
            return true;
        }
        if self.row == 0 {
            return false;
        }
        let tail = self.lines.remove(self.row);
        self.row -= 1;
        self.col = self.lines[self.row].chars().count();
        self.lines[self.row].push_str(&tail);
        true
    }

    fn delete(&mut self) -> bool {
        if self.col < self.lines[self.row].chars().count() {
            let at = self.byte_at();
            self.lines[self.row].remove(at);
            return true;
        }
        if self.row + 1 >= self.lines.len() {
            return false;
        }
        let tail = self.lines.remove(self.row + 1);
        self.lines[self.row].push_str(&tail);
        true
    }

    fn clamp(&mut self) {
        self.row = self.row.min(self.lines.len() - 1);
        self.col = self.col.min(self.lines[self.row].chars().count());
    }

    fn top(&self, height: usize) -> usize {
        (self.row + 1).saturating_sub(height.max(1))
    }

    fn start_col(&self, width: usize) -> usize {
        let line: Vec<char> = self.lines[self.row].chars().collect();
        let mut start = 0;
        while start < self.col
            && line[start..self.col.min(line.len())]
                .iter()
                .map(|c| c.width().unwrap_or(0))
                .sum::<usize>()
                >= width.max(2)
        {
            start += 1;
        }
        start
    }
}

fn split_keys(data: &[u8]) -> Vec<Vec<u8>> {
    let mut keys = Vec::new();
    let mut i = 0;
    while i < data.len() {
        let start = i;
        let b = data[i];
        if b == 0x1b {
            i += 1;
            match data.get(i) {
                Some(b'[') => {
                    i += 1;
                    while i < data.len() && !(0x40..=0x7e).contains(&data[i]) {
                        i += 1;
                    }
                    i = (i + 1).min(data.len());
                }
                Some(b'O') => i = (i + 2).min(data.len()),
                _ => {}
            }
        } else if b < 0x20 || b == 0x7f {
            i += 1;
        } else {
            while i < data.len() && data[i] >= 0x20 && data[i] != 0x7f {
                i += 1;
            }
        }
        keys.push(data[start..i].to_vec());
    }
    keys
}

fn panel_rect(area: Rect) -> Rect {
    let w = PANEL_W.min(area.width);
    let h = area.height.saturating_sub(2).max(area.height.min(10));
    Rect::new(
        area.x + area.width.saturating_sub(w),
        area.y + (area.height.saturating_sub(h)) / 2,
        w,
        h.min(area.height),
    )
}

fn body_rect(rect: Rect) -> Rect {
    let inner = rect.inner(ratatui::layout::Margin::new(1, 1));
    Rect::new(
        inner.x,
        inner.y,
        inner.width,
        inner.height.saturating_sub(3),
    )
}

fn buttons(rect: Rect, n: &NotesState) -> Vec<(Rect, &'static [u8], &'static str, bool)> {
    let y = rect.bottom().saturating_sub(3);
    let items: Vec<(&'static [u8], &'static str, bool)> = if n.preview.is_some() {
        vec![
            (b"\r", "[ Enter Send ]", true),
            (b"\x1b", "[ Esc Back ]", true),
        ]
    } else {
        vec![
            (b"\x14", "[ ^T To task ]", n.task.is_some()),
            (b"\x1b", "[ Esc Close ]", true),
        ]
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
    pub fn open_notes(&mut self, state: NotesState) {
        self.notes = Some(state);
        self.help = false;
        self.grab = None;
    }

    pub fn insert_notes_text(&mut self, data: &[u8]) -> Option<NotesAction> {
        let n = self.notes.as_mut()?;
        if n.preview.is_some() {
            return None;
        }
        let text = String::from_utf8_lossy(data).into_owned();
        n.note.clear();
        n.insert_text(&text).then(|| NotesAction::Save {
            session: n.session.clone(),
            text: n.text(),
        })
    }

    pub fn edit_notes(&mut self, data: &[u8]) -> Option<NotesAction> {
        let mut last = None;
        for key in split_keys(data) {
            if self.notes.is_none() {
                break;
            }
            let action = self.edit_note_key(&key);
            if matches!(action, Some(NotesAction::Append { .. })) {
                return action;
            }
            last = action.or(last);
        }
        last
    }

    fn edit_note_key(&mut self, data: &[u8]) -> Option<NotesAction> {
        let n = self.notes.as_mut()?;
        if n.preview.is_some() {
            match data {
                b"\r" | b"\n" => {
                    let task = n.task.clone()?;
                    let text = n.preview.take()?;
                    n.sending = true;
                    n.note = "sending to the task…".into();
                    return Some(NotesAction::Append { task, text });
                }
                b"\x1b" | b"n" | b"q" => n.preview = None,
                _ => {}
            }
            return None;
        }
        let save = |n: &NotesState| NotesAction::Save {
            session: n.session.clone(),
            text: n.text(),
        };
        let mut changed = false;
        n.note.clear();
        match data {
            b"\x1b" => {
                let action = n.unsaved.then(|| save(n));
                self.notes = None;
                return action;
            }
            b"\x14" => {
                if n.sending {
                    n.note = "still sending the previous notes".into();
                } else if n.task.is_none() {
                    n.note = "this session has no task".into();
                } else if n.text().is_empty() {
                    n.note = "nothing to send yet".into();
                } else {
                    n.preview = Some(n.block());
                }
            }
            b"\x1b[A" | b"\x1bOA" => {
                n.row = n.row.saturating_sub(1);
                n.clamp();
            }
            b"\x1b[B" | b"\x1bOB" => {
                n.row = (n.row + 1).min(n.lines.len() - 1);
                n.clamp();
            }
            b"\x1b[D" | b"\x1bOD" => {
                if n.col > 0 {
                    n.col -= 1;
                } else if n.row > 0 {
                    n.row -= 1;
                    n.col = n.lines[n.row].chars().count();
                }
            }
            b"\x1b[C" | b"\x1bOC" => {
                if n.col < n.lines[n.row].chars().count() {
                    n.col += 1;
                } else if n.row + 1 < n.lines.len() {
                    n.row += 1;
                    n.col = 0;
                }
            }
            b"\x1b[H" | b"\x1bOH" | b"\x1b[1~" | b"\x01" => n.col = 0,
            b"\x1b[F" | b"\x1bOF" | b"\x1b[4~" | b"\x05" => n.col = n.lines[n.row].chars().count(),
            b"\x7f" | b"\x08" => changed = n.backspace(),
            b"\x1b[3~" | b"\x04" => changed = n.delete(),
            other if other.first() == Some(&0x1b) => {}
            other => {
                let text = String::from_utf8_lossy(other).into_owned();
                changed = n.insert_text(&text);
            }
        }
        changed.then(|| save(n))
    }

    pub fn notes_saved(&mut self, ok: bool) {
        if let Some(n) = self.notes.as_mut() {
            n.unsaved = !ok;
            if !ok {
                n.note = "cannot save the notes file".into();
            }
        }
    }

    pub fn notes_sent(&mut self, ok: bool, message: &str) {
        if let Some(n) = self.notes.as_mut() {
            n.sending = false;
            n.note = if ok {
                message.to_string()
            } else {
                format!("not sent: {message}")
            };
        }
    }

    pub(crate) fn notes_mouse(
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
                let Some(n) = self.notes.as_mut() else {
                    return out;
                };
                let hit = buttons(rect, n)
                    .into_iter()
                    .find(|(r, _, _, on)| *on && contains(*r, ev.col, ev.row))
                    .map(|(_, k, _, _)| k);
                if let Some(key) = hit {
                    out.notes = self.edit_notes(key);
                    return out;
                }
                let body = body_rect(rect);
                if n.preview.is_none() && contains(body, ev.col, ev.row) {
                    let top = n.top(body.height as usize);
                    let row = (top + usize::from(ev.row - body.y)).min(n.lines.len() - 1);
                    let start = if row == n.row {
                        n.start_col(body.width as usize)
                    } else {
                        0
                    };
                    let target = usize::from(ev.col - body.x);
                    let mut width = 0;
                    let mut col = start;
                    for ch in n.lines[row].chars().skip(start) {
                        let w = ch.width().unwrap_or(0);
                        if width + w > target {
                            break;
                        }
                        width += w;
                        col += 1;
                    }
                    n.row = row;
                    n.col = col;
                }
            }
            Kind::Release if self.grab.as_deref() == Some("") => self.grab = None,
            _ => {}
        }
        out
    }
}

pub(crate) fn draw_notes(buf: &mut Buffer, area: Rect, st: &ViewState) {
    let Some(n) = st.notes.as_ref() else {
        return;
    };
    let rect = panel_rect(area);
    if rect.is_empty() {
        return;
    }
    Clear.render(rect, buf);
    let target = n
        .task
        .as_ref()
        .map_or("no task".to_string(), |t| t.key.clone());
    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(format!(" Notes · {} · {target} ", n.name))
        .render(rect, buf);
    let body = body_rect(rect);
    if body.is_empty() {
        return;
    }
    let bold = Style::new().add_modifier(Modifier::BOLD);
    if let Some(preview) = &n.preview {
        let key = n.task.as_ref().map_or("", |t| t.key.as_str());
        put(
            buf,
            body.x,
            body.y,
            &format!("Appended to the description of {key}:"),
            bold,
            body.width,
        );
        for (i, l) in wrap(preview, body.width as usize)
            .into_iter()
            .take(usize::from(body.height.saturating_sub(1)))
            .enumerate()
        {
            put(
                buf,
                body.x,
                body.y + 1 + i as u16,
                &l,
                Style::new(),
                body.width,
            );
        }
    } else {
        let top = n.top(body.height as usize);
        for i in 0..usize::from(body.height) {
            let Some(line) = n.lines.get(top + i) else {
                break;
            };
            let y = body.y + i as u16;
            let start = if top + i == n.row {
                n.start_col(body.width as usize)
            } else {
                0
            };
            let shown: String = line.chars().skip(start).collect();
            put(buf, body.x, y, &shown, Style::new(), body.width);
            if top + i == n.row {
                let before: String = line.chars().skip(start).take(n.col - start).collect();
                let x = body.x + before.width() as u16;
                if x < body.right() {
                    let under = line.chars().nth(n.col).unwrap_or(' ').to_string();
                    put(
                        buf,
                        x,
                        y,
                        &under,
                        Style::new().add_modifier(Modifier::REVERSED),
                        body.right() - x,
                    );
                }
            }
        }
    }
    let hover = st.pointer;
    for (r, _, label, enabled) in buttons(rect, n) {
        if r.is_empty() {
            continue;
        }
        let over = hover.is_some_and(|(c, row)| contains(r, c, row));
        put(buf, r.x, r.y, label, button_style(over, enabled), r.width);
    }
    let hint = if !n.note.is_empty() {
        n.note.clone()
    } else if n.preview.is_some() {
        "nothing was sent yet".to_string()
    } else {
        "autosaved here; never sent to the agent".to_string()
    };
    put(
        buf,
        rect.x + 2,
        rect.bottom().saturating_sub(2),
        &hint,
        dim(),
        rect.width.saturating_sub(4),
    );
}
