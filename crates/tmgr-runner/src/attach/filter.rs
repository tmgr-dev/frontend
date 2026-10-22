use super::palette::{esc_key, next_char};
use super::view::ViewState;

const MAX_FILTER: usize = 64;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Filter {
    pub query: String,
    pub input: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterEdit {
    Editing,
    Keep,
    Clear,
}

impl ViewState {
    pub fn open_filter(&mut self) {
        self.sidebar = Some(true);
        self.zoom = false;
        self.rename = None;
        self.confirm = None;
        self.layout_offer = None;
        self.sidebar_focus = None;
        self.palette = None;
        self.clear_find();
        self.scroll = None;
        let query = self.filter.take().map(|f| f.query).unwrap_or_default();
        self.filter = Some(Filter { query, input: true });
        self.help = false;
    }

    pub fn end_filter_input(&mut self) {
        if !self.filter_input() {
            return;
        }
        if self.filter_active() {
            if let Some(f) = self.filter.as_mut() {
                f.input = false;
            }
        } else {
            self.filter = None;
        }
    }

    pub fn clear_filter(&mut self) {
        self.filter = None;
    }

    pub fn filter_input(&self) -> bool {
        self.filter.as_ref().is_some_and(|f| f.input)
    }

    pub fn edit_filter(&mut self, data: &[u8]) -> FilterEdit {
        let Some(f) = self.filter.as_mut() else {
            return FilterEdit::Clear;
        };
        if !data.is_empty() && data.iter().all(|&b| b == 0x1b) {
            self.filter = None;
            return FilterEdit::Clear;
        }
        let mut i = 0;
        while i < data.len() {
            match data[i] {
                0x1b => {
                    let (_, used) = esc_key(&data[i..]);
                    i += used;
                }
                b'\r' | b'\n' => {
                    if f.query.trim().is_empty() {
                        self.filter = None;
                        return FilterEdit::Clear;
                    }
                    f.input = false;
                    return FilterEdit::Keep;
                }
                0x7f | 0x08 => {
                    i += 1;
                    f.query.pop();
                }
                0x15 => {
                    i += 1;
                    f.query.clear();
                }
                _ => {
                    let (c, used) = next_char(&data[i..]);
                    i += used;
                    if let Some(c) = c.filter(|c| !c.is_control()) {
                        if f.query.chars().count() < MAX_FILTER {
                            f.query.push(c);
                        }
                    }
                }
            }
        }
        FilterEdit::Editing
    }

    pub fn insert_filter_text(&mut self, data: &[u8]) {
        let Some(f) = self.filter.as_mut().filter(|f| f.input) else {
            return;
        };
        let text = String::from_utf8_lossy(data)
            .replace("\x1b[200~", "")
            .replace("\x1b[201~", "");
        for c in text.chars().filter(|c| !c.is_control()) {
            if f.query.chars().count() >= MAX_FILTER {
                break;
            }
            f.query.push(c);
        }
    }

    pub fn filter_active(&self) -> bool {
        self.filter
            .as_ref()
            .is_some_and(|f| !f.query.trim().is_empty())
    }

    pub(crate) fn passes_filter(&self, id: &str) -> bool {
        let Some(f) = self.filter.as_ref().filter(|_| self.filter_active()) else {
            return true;
        };
        let Some(row) = self.sessions.iter().find(|r| r.id == id) else {
            return false;
        };
        let mut hay = row.name.to_lowercase();
        if let Some(t) = self.agent_title(row) {
            hay.push(' ');
            hay.push_str(&t.text.to_lowercase());
        }
        if let Some(l) = self.locs.get(id) {
            for part in [
                Some(l.root.as_str()),
                Some(l.dir.as_str()),
                l.branch.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                hay.push(' ');
                hay.push_str(&part.to_lowercase());
            }
        }
        f.query
            .to_lowercase()
            .split_whitespace()
            .all(|term| hay.contains(term))
    }
}
