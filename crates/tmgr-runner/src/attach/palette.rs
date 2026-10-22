use super::actions::ACTIONS;
use super::keys::Cmd;
use super::mouse::{Kind, MouseEvent};
use super::prompts::{clean, Prompt, PromptForm, MAX_NAME, MAX_TEXT};
use super::render::state_label;
use super::view::{contains, MouseOut, ViewState};
use ratatui::layout::{Margin, Rect};
use std::ops::Not;
use unicode_width::UnicodeWidthStr;

const MAX_QUERY: usize = 64;
const MAX_WIDTH: u16 = 64;
const PROMPT_WIDTH: usize = 2;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum PaletteMode {
    #[default]
    Commands,
    Prompts,
    Form(PromptForm),
    Delete(usize),
}

#[derive(Clone, Debug, Default)]
pub struct Palette {
    pub query: String,
    pub sel: usize,
    pub mode: PaletteMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteEntry {
    Cmd(Cmd),
    Session(String),
    Prompt(usize),
    NewPrompt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PaletteEdit {
    Editing,
    Run(PaletteEntry),
    PromptsChanged,
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowView {
    pub label: String,
    pub right: String,
    pub hits: Vec<usize>,
    pub entry: PaletteEntry,
}

struct Item {
    entry: PaletteEntry,
    label: String,
    search: String,
    right: String,
}

fn aliases(cmd: Cmd) -> &'static str {
    match cmd {
        Cmd::Detach => "quit exit",
        Cmd::Stop => "kill",
        Cmd::Remove => "delete",
        Cmd::Help => "keys",
        Cmd::ScrollMode => "history",
        Cmd::Split => "split pane",
        Cmd::SplitRight => "split vertical",
        Cmd::SplitDown => "split horizontal",
        Cmd::NewSession => "create start chat agent",
        Cmd::Questions => "asked pocket answer ignore reply",
        Cmd::OpenTask => "ticket issue browser desktop chip",
        Cmd::Search => "find grep history scrollback",
        Cmd::CopyMode => "clipboard yank select text",
        Cmd::Broadcast => "all panes same prompt send many fan out",
        Cmd::BroadcastMark => "select target pane broadcast",
        Cmd::NeedsYou => "attention waiting permission question alert",
        Cmd::Subagents => "agents tasks background peek",
        _ => "",
    }
}

pub(crate) fn palette_rect(area: Rect, rows: usize) -> Rect {
    let w = MAX_WIDTH.min(area.width);
    let h = u16::try_from(rows.saturating_add(4))
        .unwrap_or(u16::MAX)
        .min(area.height);
    Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    )
}

pub(crate) fn palette_inner(rect: Rect) -> Rect {
    rect.inner(Margin::new(2, 1))
}

pub(crate) fn palette_list(rect: Rect) -> Rect {
    let inner = palette_inner(rect);
    Rect::new(
        inner.x,
        inner.y.saturating_add(2),
        inner.width,
        inner.height.saturating_sub(2),
    )
}

pub(crate) fn palette_top(sel: usize, visible: usize) -> usize {
    (sel + 1).saturating_sub(visible)
}

pub(crate) fn query_tail(query: &str, avail: usize) -> &str {
    let mut start = 0;
    for (i, c) in query.char_indices() {
        if query[start..].width() <= avail {
            break;
        }
        start = i + c.len_utf8();
    }
    &query[start..]
}

pub(crate) fn query_avail(inner: Rect) -> usize {
    usize::from(inner.width).saturating_sub(PROMPT_WIDTH + 1)
}

pub(crate) enum Key {
    Up,
    Down,
    Other,
}

pub(crate) fn esc_key(b: &[u8]) -> (Key, usize) {
    match b.get(1) {
        Some(b'[') => {
            let mut j = 2;
            while j < b.len() && (0x20..=0x3f).contains(&b[j]) {
                j += 1;
            }
            if j >= b.len() {
                return (Key::Other, b.len());
            }
            let key = match (j, b[j]) {
                (2, b'A') => Key::Up,
                (2, b'B') => Key::Down,
                _ => Key::Other,
            };
            (key, j + 1)
        }
        Some(b'O') => match b.get(2) {
            Some(b'A') => (Key::Up, 3),
            Some(b'B') => (Key::Down, 3),
            Some(_) => (Key::Other, 3),
            None => (Key::Other, 2),
        },
        Some(0x1b) | None => (Key::Other, 1),
        Some(_) => (Key::Other, 2),
    }
}

pub(crate) fn next_char(b: &[u8]) -> (Option<char>, usize) {
    let len = match b[0] {
        0..=0x7f => 1,
        0xc2..=0xdf => 2,
        0xe0..=0xef => 3,
        0xf0..=0xf4 => 4,
        _ => return (None, 1),
    };
    match b
        .get(..len)
        .and_then(|s| std::str::from_utf8(s).ok())
        .and_then(|s| s.chars().next())
    {
        Some(c) => (Some(c), len),
        None => (None, 1),
    }
}

impl ViewState {
    pub fn entries(&self) -> Vec<RowView> {
        let Some(p) = &self.palette else {
            return Vec::new();
        };
        match p.mode {
            PaletteMode::Commands => {}
            PaletteMode::Prompts => return self.prompt_entries(&p.query),
            PaletteMode::Form(_) | PaletteMode::Delete(_) => return Vec::new(),
        }
        let items: Vec<Item> = ACTIONS
            .iter()
            .filter(|a| a.palette)
            .map(|a| Item {
                entry: PaletteEntry::Cmd(a.cmd),
                label: a.label.to_string(),
                search: format!("{} {}", aliases(a.cmd), a.label),
                right: format!("Ctrl-] {}", a.hotkey),
            })
            .chain(self.sidebar_rows().into_iter().map(|s| Item {
                entry: PaletteEntry::Session(s.id.clone()),
                label: s.name.clone(),
                search: s.name.clone(),
                right: state_label(s).to_string(),
            }))
            .collect();
        tmgr_fuzzy::rank(&p.query, items, |i| i.search.clone())
            .into_iter()
            .map(|i| RowView {
                hits: if p.query.is_empty() {
                    Vec::new()
                } else {
                    tmgr_fuzzy::matched(&p.query, &i.label)
                },
                label: i.label,
                right: i.right,
                entry: i.entry,
            })
            .collect()
    }

    fn prompt_entries(&self, query: &str) -> Vec<RowView> {
        let items: Vec<(usize, &Prompt)> = self.prompts.iter().enumerate().collect();
        let mut rows: Vec<RowView> =
            tmgr_fuzzy::rank(query, items, |(_, p)| format!("{} {}", p.name, p.text))
                .into_iter()
                .map(|(i, p)| RowView {
                    hits: if query.is_empty() {
                        Vec::new()
                    } else {
                        tmgr_fuzzy::matched(query, &p.name)
                    },
                    label: p.name.clone(),
                    right: p.text.chars().take(28).collect(),
                    entry: PaletteEntry::Prompt(i),
                })
                .collect();
        rows.push(RowView {
            label: "+ Add prompt".into(),
            right: "Ctrl-A".into(),
            hits: Vec::new(),
            entry: PaletteEntry::NewPrompt,
        });
        rows
    }

    pub fn open_prompts(&mut self) {
        self.palette = Some(Palette {
            mode: PaletteMode::Prompts,
            ..Palette::default()
        });
        self.help = false;
        self.grab = None;
    }

    fn open_form(&mut self, index: Option<usize>) {
        let (name, text) = index
            .and_then(|i| self.prompts.get(i))
            .map(|p| (p.name.clone(), p.text.clone()))
            .unwrap_or_default();
        if let Some(p) = self.palette.as_mut() {
            p.mode = PaletteMode::Form(PromptForm {
                index,
                name,
                text,
                field: 0,
            });
        }
    }

    fn activate(&mut self, entry: PaletteEntry) -> Option<PaletteEntry> {
        if entry == PaletteEntry::NewPrompt {
            self.open_form(None);
            return None;
        }
        self.palette = None;
        Some(entry)
    }

    fn selected_prompt(&self) -> Option<usize> {
        match self.selected_entry()? {
            PaletteEntry::Prompt(i) => Some(i),
            _ => None,
        }
    }

    fn edit_prompt_form(&mut self, data: &[u8]) -> PaletteEdit {
        let mut i = 0;
        while i < data.len() {
            let Some(PaletteMode::Form(form)) = self.palette.as_mut().map(|p| &mut p.mode) else {
                break;
            };
            match data[i] {
                0x1b => {
                    let (_, used) = esc_key(&data[i..]);
                    if used == 1 && data.len() == i + 1 {
                        if let Some(p) = self.palette.as_mut() {
                            p.mode = PaletteMode::Prompts;
                        }
                        return PaletteEdit::Editing;
                    }
                    i += used;
                }
                b'\t' => {
                    i += 1;
                    form.field = 1 - form.field;
                }
                b'\r' | b'\n' => {
                    i += 1;
                    let name = clean(&form.name, MAX_NAME);
                    let text = clean(&form.text, MAX_TEXT);
                    if name.is_empty() || text.is_empty() {
                        form.field = usize::from(name.is_empty().not());
                        continue;
                    }
                    let index = form.index;
                    let prompt = Prompt { name, text };
                    match index.filter(|i| *i < self.prompts.len()) {
                        Some(i) => self.prompts[i] = prompt,
                        None => self.prompts.push(prompt),
                    }
                    if let Some(p) = self.palette.as_mut() {
                        p.mode = PaletteMode::Prompts;
                        p.query.clear();
                        p.sel = 0;
                    }
                    return PaletteEdit::PromptsChanged;
                }
                0x7f | 0x08 => {
                    i += 1;
                    form.active().0.pop();
                }
                0x15 => {
                    i += 1;
                    form.active().0.clear();
                }
                _ => {
                    let (c, used) = next_char(&data[i..]);
                    i += used;
                    if let Some(c) = c.filter(|c| !c.is_control()) {
                        let (field, max) = form.active();
                        if field.chars().count() < max {
                            field.push(c);
                        }
                    }
                }
            }
        }
        PaletteEdit::Editing
    }

    fn edit_prompt_delete(&mut self, data: &[u8], idx: usize) -> PaletteEdit {
        let confirm = matches!(data, [b'y'] | [b'Y']);
        if let Some(p) = self.palette.as_mut() {
            p.mode = PaletteMode::Prompts;
            p.sel = 0;
        }
        if confirm && idx < self.prompts.len() {
            self.prompts.remove(idx);
            return PaletteEdit::PromptsChanged;
        }
        PaletteEdit::Editing
    }

    fn move_palette_sel(&mut self, down: bool) -> bool {
        let last = self.entries().len().saturating_sub(1);
        let Some(p) = self.palette.as_mut() else {
            return false;
        };
        let cur = p.sel.min(last);
        let next = if down {
            (cur + 1).min(last)
        } else {
            cur.saturating_sub(1)
        };
        p.sel = next;
        next != cur
    }

    fn selected_entry(&self) -> Option<PaletteEntry> {
        let p = self.palette.as_ref()?;
        let rows = self.entries();
        let at = p.sel.min(rows.len().checked_sub(1)?);
        rows.into_iter().nth(at).map(|r| r.entry)
    }

    pub fn edit_palette(&mut self, data: &[u8]) -> PaletteEdit {
        if self.palette.is_none() {
            return PaletteEdit::Close;
        }
        match self.palette.as_ref().map(|p| p.mode.clone()) {
            Some(PaletteMode::Form(_)) => return self.edit_prompt_form(data),
            Some(PaletteMode::Delete(idx)) => return self.edit_prompt_delete(data, idx),
            _ => {}
        }
        if !data.is_empty() && data.iter().all(|&b| b == 0x1b) {
            self.palette = None;
            return PaletteEdit::Close;
        }
        let mut i = 0;
        while i < data.len() {
            let Some(p) = self.palette.as_mut() else {
                break;
            };
            match data[i] {
                0x1b => {
                    let (key, used) = esc_key(&data[i..]);
                    i += used;
                    match key {
                        Key::Up => {
                            self.move_palette_sel(false);
                        }
                        Key::Down => {
                            self.move_palette_sel(true);
                        }
                        Key::Other => {}
                    }
                }
                b'\r' | b'\n' => {
                    i += 1;
                    if let Some(entry) = self.selected_entry() {
                        if let Some(entry) = self.activate(entry) {
                            return PaletteEdit::Run(entry);
                        }
                        return PaletteEdit::Editing;
                    }
                }
                0x01 if p.mode == PaletteMode::Prompts => {
                    self.open_form(None);
                    return PaletteEdit::Editing;
                }
                0x05 if p.mode == PaletteMode::Prompts => {
                    if let Some(idx) = self.selected_prompt() {
                        self.open_form(Some(idx));
                    }
                    return PaletteEdit::Editing;
                }
                0x04 if p.mode == PaletteMode::Prompts => {
                    if let Some(idx) = self.selected_prompt() {
                        if let Some(p) = self.palette.as_mut() {
                            p.mode = PaletteMode::Delete(idx);
                        }
                    }
                    return PaletteEdit::Editing;
                }
                0x7f | 0x08 => {
                    i += 1;
                    p.query.pop();
                    p.sel = 0;
                }
                0x15 => {
                    i += 1;
                    p.query.clear();
                    p.sel = 0;
                }
                0x0e => {
                    i += 1;
                    self.move_palette_sel(true);
                }
                0x10 => {
                    i += 1;
                    self.move_palette_sel(false);
                }
                _ => {
                    let (c, used) = next_char(&data[i..]);
                    i += used;
                    if let Some(c) = c.filter(|c| !c.is_control()) {
                        if p.query.chars().count() < MAX_QUERY {
                            p.query.push(c);
                            p.sel = 0;
                        }
                    }
                }
            }
        }
        PaletteEdit::Editing
    }

    pub fn insert_palette_text(&mut self, data: &[u8]) {
        let Some(p) = self.palette.as_mut() else {
            return;
        };
        if let PaletteMode::Form(form) = &mut p.mode {
            let text = String::from_utf8_lossy(data)
                .replace("\x1b[200~", "")
                .replace("\x1b[201~", "");
            let (field, max) = form.active();
            for c in text.chars().map(|c| if c.is_control() { ' ' } else { c }) {
                if field.chars().count() < max {
                    field.push(c);
                }
            }
            return;
        }
        if matches!(p.mode, PaletteMode::Delete(_)) {
            return;
        }
        let text = String::from_utf8_lossy(data)
            .replace("\x1b[200~", "")
            .replace("\x1b[201~", "");
        for c in text.chars().filter(|c| !c.is_control()) {
            if p.query.chars().count() >= MAX_QUERY {
                break;
            }
            p.query.push(c);
            p.sel = 0;
        }
    }

    pub(crate) fn palette_mouse(
        &mut self,
        area: Rect,
        ev: MouseEvent,
        mut out: MouseOut,
    ) -> MouseOut {
        let rows = self.entries();
        let rect = palette_rect(area, rows.len().max(1));
        let list = palette_list(rect);
        match ev.kind() {
            kind @ (Kind::WheelUp | Kind::WheelDown) => {
                if self.move_palette_sel(kind == Kind::WheelDown) {
                    out.redraw = true;
                }
            }
            Kind::Press => {
                self.grab = Some(String::new());
                if !ev.is_left_press() {
                    return out;
                }
                if contains(list, ev.col, ev.row) {
                    let sel = self.palette.as_ref().map_or(0, |p| p.sel);
                    let top = palette_top(
                        sel.min(rows.len().saturating_sub(1)),
                        usize::from(list.height),
                    );
                    let idx = top + usize::from(ev.row - list.y);
                    if let Some(r) = rows.get(idx) {
                        out.palette = self.activate(r.entry.clone());
                        out.redraw = true;
                    }
                } else if !contains(rect, ev.col, ev.row) {
                    self.palette = None;
                    out.redraw = true;
                }
            }
            Kind::Release if self.grab.as_deref() == Some("") => self.grab = None,
            _ => {}
        }
        out
    }
}
