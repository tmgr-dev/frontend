use super::mouse::{Kind, MouseEvent};
use super::palette::{esc_key, Key};
use super::render::{button_style, dim, put};
use super::theme::colors;
use super::title::clip;
use super::view::{contains, header_button, Control, MouseOut, ViewState};
use crate::questions::{context_lines, short_age, Entry, Status};
use crate::sanitize::sanitize_text;
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, BorderType, Clear, Widget};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const MAX_WIDTH: u16 = 92;
const MAX_ROWS: usize = 6;
const MAX_CONTEXT: usize = 9;
const BUTTONS_MIN: u16 = 58;
const WIDE_BUTTONS_MIN: u16 = 80;
const MARK_MAX_ROWS: usize = 8;
const MAX_ENTRIES: usize = 400;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QMenu {
    pub scope: Option<String>,
    pub ignored: bool,
    pub sel: usize,
    pub context: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QButton {
    Goto,
    Fork,
    Context,
    Answer,
    Ignore,
}

impl QButton {
    fn label(self, ignored: bool) -> &'static str {
        match (self, ignored) {
            (QButton::Goto, _) => "Go to",
            (QButton::Fork, _) => "Fork",
            (QButton::Context, _) => "Context",
            (QButton::Answer, _) => "Answer",
            (QButton::Ignore, false) => "Ignore",
            (QButton::Ignore, true) => "Restore",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GotoMark {
    pub session: String,
    pub bottom: usize,
    pub rows: usize,
}

pub fn find_question(lines: &[String], question: &str) -> Option<(usize, usize)> {
    let norm = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let mut hay = String::new();
    let mut starts = Vec::with_capacity(lines.len());
    for line in lines {
        starts.push(hay.len());
        hay.push_str(&norm(line));
        hay.push(' ');
    }
    let needle = norm(question);
    if needle.is_empty() {
        return None;
    }
    let chars: Vec<char> = needle.chars().collect();
    let head: String = chars.iter().take(40).collect();
    let tail: String = chars[chars.len().saturating_sub(40)..].iter().collect();
    let at = [needle.clone(), head, tail].into_iter().find_map(|n| {
        let s = hay.rfind(&n)?;
        Some((s, s + n.len()))
    })?;
    let line = |b: usize| starts.partition_point(|&s| s <= b).saturating_sub(1);
    Some((line(at.0), line(at.1.saturating_sub(1))))
}

pub fn goto_target(
    len: usize,
    screen_rows: usize,
    scroll_max: usize,
    (first, last): (usize, usize),
    session: &str,
) -> (usize, GotoMark) {
    let rows = (last + 1 - first).min(MARK_MAX_ROWS);
    let first = last + 1 - rows;
    let start = first.saturating_sub(1);
    let offset = len
        .saturating_sub(screen_rows)
        .saturating_sub(start)
        .min(scroll_max);
    let mark = GotoMark {
        session: session.to_string(),
        bottom: len.saturating_sub(1).saturating_sub(last),
        rows,
    };
    (offset, mark)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QEdit {
    Editing,
    Answer(String),
    Goto(String),
    Fork(String),
    Ignore(String),
    Restore(String),
    Close,
}

pub(crate) struct QRowGeom {
    pub idx: usize,
    pub y: u16,
    pub text_x: u16,
    pub text_w: u16,
    pub age_x: u16,
    pub buttons: Vec<(QButton, u16, u16)>,
}

pub(crate) struct QGeom {
    pub rect: Rect,
    pub inner: Rect,
    pub tabs: [(bool, u16, u16); 2],
    pub rows: Vec<QRowGeom>,
    pub context: Vec<(u16, String)>,
    pub hint_y: u16,
    pub empty_y: Option<u16>,
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    let mut w = 0;
    for word in text.split_whitespace() {
        let ww = word.width();
        if w > 0 && w + 1 + ww > width {
            lines.push(std::mem::take(&mut cur));
            w = 0;
        }
        if w > 0 {
            cur.push(' ');
            w += 1;
        }
        for c in word.chars() {
            let cw = c.width().unwrap_or(0);
            if w + cw > width && w > 0 {
                lines.push(std::mem::take(&mut cur));
                w = 0;
            }
            cur.push(c);
            w += cw;
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

fn context_block(e: &Entry, width: usize) -> Vec<String> {
    let width = width.max(8);
    let mut out = Vec::new();
    if !e.q.user.is_empty() {
        let mut user = wrap(&format!("You: {}", e.q.user), width);
        if user.len() > 2 {
            user.truncate(2);
            if let Some(last) = user.last_mut() {
                last.push('…');
            }
        }
        out.extend(user);
        out.push(String::new());
    }
    out.push("Agent:".to_string());
    let lines = if e.q.item.is_empty() {
        context_lines(&e.q.reply, &e.q.text)
    } else {
        vec![e.q.item.clone()]
    };
    for line in lines {
        if line.is_empty() {
            out.push(String::new());
        } else {
            out.extend(wrap(&line, width));
        }
    }
    if out.len() > MAX_CONTEXT {
        out.truncate(MAX_CONTEXT);
        out.push("…".to_string());
    }
    out
}

pub(crate) fn badge_text(n: usize) -> String {
    format!("?{n}")
}

impl ViewState {
    pub(crate) fn header_badge(&self, outer: Rect, session: &str) -> Option<(u16, u16)> {
        let n = self.open_in(session);
        if n == 0 {
            return None;
        }
        let base = header_button(outer, Control::Interrupt)?.checked_sub(1)?;
        let w = badge_text(n).width() as u16;
        Some((base.checked_sub(w)?, w))
    }

    pub(crate) fn sidebar_badge(
        &self,
        sidebar: Rect,
        session: &str,
        off: u16,
    ) -> Option<(u16, u16)> {
        let n = self.open_in(session);
        (n > 0).then(|| (sidebar.x + 3 + off, badge_text(n).width() as u16))
    }

    fn question_known(&self, session: &str) -> bool {
        self.sessions.iter().any(|s| s.id == session)
    }

    pub fn open_in(&self, session: &str) -> usize {
        self.questions
            .iter()
            .filter(|e| e.status == Status::Open && e.q.session == session)
            .count()
    }

    pub fn open_total(&self) -> usize {
        self.questions
            .iter()
            .filter(|e| e.status == Status::Open && self.question_known(&e.q.session))
            .count()
    }

    pub fn qmenu_rows(&self, menu: &QMenu) -> Vec<&Entry> {
        let want = if menu.ignored {
            Status::Ignored
        } else {
            Status::Open
        };
        self.questions
            .iter()
            .filter(|e| e.status == want)
            .filter(|e| self.question_known(&e.q.session))
            .filter(|e| menu.scope.as_deref().is_none_or(|s| e.q.session == s))
            .collect()
    }

    fn qmenu_ignored_count(&self, menu: &QMenu) -> usize {
        let other = QMenu {
            ignored: !menu.ignored,
            ..menu.clone()
        };
        self.qmenu_rows(&other).len()
    }

    pub fn set_questions(&mut self, mut entries: Vec<Entry>) -> bool {
        entries.truncate(MAX_ENTRIES);
        if entries == self.questions {
            return false;
        }
        let keep = self.qmenu.as_ref().and_then(|_| self.qmenu_selected());
        self.questions = entries;
        if let (Some(id), Some(menu)) = (keep, self.qmenu.clone()) {
            let at = self.qmenu_rows(&menu).iter().position(|e| e.q.id == id);
            if let (Some(at), Some(m)) = (at, self.qmenu.as_mut()) {
                m.sel = at;
            }
        }
        true
    }

    pub fn set_question_status(&mut self, id: &str, status: Status) {
        for e in self.questions.iter_mut().filter(|e| e.q.id == id) {
            e.status = status;
        }
    }

    pub fn question(&self, id: &str) -> Option<&Entry> {
        self.questions.iter().find(|e| e.q.id == id)
    }

    pub fn show_context(&mut self, session: &str, id: &str) {
        if !self.open_questions(Some(session.to_string())) {
            return;
        }
        let Some(menu) = self.qmenu.clone() else {
            return;
        };
        let at = self.qmenu_rows(&menu).iter().position(|e| e.q.id == id);
        if let (Some(at), Some(m)) = (at, self.qmenu.as_mut()) {
            m.sel = at;
            m.context = true;
        }
    }

    pub fn open_questions(&mut self, scope: Option<String>) -> bool {
        let known = |e: &&Entry| self.question_known(&e.q.session);
        let has = self
            .questions
            .iter()
            .filter(known)
            .any(|e| e.status != Status::Answered);
        if !has {
            self.notice = Some("no unanswered questions".into());
            return false;
        }
        let scope = scope.filter(|s| {
            self.questions
                .iter()
                .any(|e| &e.q.session == s && e.status != Status::Answered)
        });
        let mut menu = QMenu {
            scope,
            ..QMenu::default()
        };
        if self.qmenu_rows(&menu).is_empty() {
            menu.ignored = true;
            if self.qmenu_rows(&menu).is_empty() {
                menu.ignored = false;
            }
        }
        self.new_session = None;
        self.palette = None;
        self.help = false;
        self.grab = None;
        self.qmenu = Some(menu);
        true
    }

    pub fn toggle_questions(&mut self) {
        if self.qmenu.take().is_some() {
            return;
        }
        let scope = self.focused_session().map(str::to_string);
        self.open_questions(scope);
    }

    fn qmenu_len(&self) -> usize {
        self.qmenu.as_ref().map_or(0, |m| self.qmenu_rows(m).len())
    }

    fn qmenu_selected(&self) -> Option<String> {
        let menu = self.qmenu.as_ref()?;
        let rows = self.qmenu_rows(menu);
        rows.get(menu.sel.min(rows.len().checked_sub(1)?))
            .map(|e| e.q.id.clone())
    }

    fn qmenu_move(&mut self, down: bool) {
        let last = self.qmenu_len().saturating_sub(1);
        if let Some(m) = self.qmenu.as_mut() {
            let cur = m.sel.min(last);
            let next = if down {
                (cur + 1).min(last)
            } else {
                cur.saturating_sub(1)
            };
            if next != cur {
                m.context = false;
            }
            m.sel = next;
        }
    }

    fn qmenu_tab(&mut self, ignored: bool) {
        if let Some(m) = self.qmenu.as_mut() {
            if m.ignored != ignored {
                m.ignored = ignored;
                m.sel = 0;
                m.context = false;
            }
        }
    }

    fn qmenu_act(&mut self, button: QButton) -> QEdit {
        let Some(id) = self.qmenu_selected() else {
            return QEdit::Editing;
        };
        let ignored = self.qmenu.as_ref().is_some_and(|m| m.ignored);
        match button {
            QButton::Context => {
                if let Some(m) = self.qmenu.as_mut() {
                    m.context = !m.context;
                }
                QEdit::Editing
            }
            QButton::Answer => {
                self.qmenu = None;
                QEdit::Answer(id)
            }
            QButton::Goto => {
                self.qmenu = None;
                QEdit::Goto(id)
            }
            QButton::Fork => {
                self.qmenu = None;
                QEdit::Fork(id)
            }
            QButton::Ignore if ignored => QEdit::Restore(id),
            QButton::Ignore => QEdit::Ignore(id),
        }
    }

    pub fn edit_qmenu(&mut self, data: &[u8]) -> QEdit {
        if self.qmenu.is_none() {
            return QEdit::Close;
        }
        if !data.is_empty() && data.iter().all(|&b| b == 0x1b) {
            self.qmenu = None;
            return QEdit::Close;
        }
        let mut i = 0;
        while i < data.len() && self.qmenu.is_some() {
            let b = data[i];
            i += 1;
            let edit = match b {
                0x1b => {
                    let (key, used) = esc_key(&data[i - 1..]);
                    i += used - 1;
                    match key {
                        Key::Up => self.qmenu_move(false),
                        Key::Down => self.qmenu_move(true),
                        Key::Other => {}
                    }
                    QEdit::Editing
                }
                b'j' | 0x0e => {
                    self.qmenu_move(true);
                    QEdit::Editing
                }
                b'k' | 0x10 => {
                    self.qmenu_move(false);
                    QEdit::Editing
                }
                b'q' => {
                    self.qmenu = None;
                    QEdit::Close
                }
                b'c' | b' ' => self.qmenu_act(QButton::Context),
                b'a' | b'\r' | b'\n' => self.qmenu_act(QButton::Answer),
                b'g' => self.qmenu_act(QButton::Goto),
                b'f' => self.qmenu_act(QButton::Fork),
                b'i' => self.qmenu_act(QButton::Ignore),
                b'\t' => {
                    let ignored = self.qmenu.as_ref().is_some_and(|m| m.ignored);
                    self.qmenu_tab(!ignored);
                    QEdit::Editing
                }
                _ => QEdit::Editing,
            };
            if edit != QEdit::Editing {
                return edit;
            }
        }
        QEdit::Editing
    }

    pub(crate) fn qmenu_mouse(
        &mut self,
        area: Rect,
        ev: MouseEvent,
        mut out: MouseOut,
    ) -> MouseOut {
        let geom = qgeom(area, self);
        match ev.kind() {
            kind @ (Kind::WheelUp | Kind::WheelDown) => {
                self.qmenu_move(kind == Kind::WheelDown);
                out.redraw = true;
            }
            Kind::Press => {
                self.grab = Some(String::new());
                if !ev.is_left_press() {
                    return out;
                }
                out.redraw = true;
                let Some(g) = geom.filter(|g| contains(g.rect, ev.col, ev.row)) else {
                    self.qmenu = None;
                    out.question = Some(QEdit::Close);
                    return out;
                };
                if let Some((ignored, ..)) = g
                    .tabs
                    .iter()
                    .find(|(_, x, w)| ev.row == g.inner.y && ev.col >= *x && ev.col < x + w)
                {
                    self.qmenu_tab(*ignored);
                    return out;
                }
                let Some(row) = g.rows.iter().find(|r| r.y == ev.row) else {
                    return out;
                };
                if let Some(m) = self.qmenu.as_mut() {
                    if m.sel != row.idx {
                        m.context = false;
                    }
                    m.sel = row.idx;
                }
                if let Some((button, ..)) = row
                    .buttons
                    .iter()
                    .find(|(_, x, w)| ev.col >= *x && ev.col < x + w)
                {
                    out.question = Some(self.qmenu_act(*button));
                }
            }
            Kind::Release if self.grab.as_deref() == Some("") => self.grab = None,
            _ => {}
        }
        out
    }
}

pub(crate) fn qgeom(area: Rect, st: &ViewState) -> Option<QGeom> {
    let menu = st.qmenu.as_ref()?;
    let rows = st.qmenu_rows(menu);
    let n = rows.len();
    let w = MAX_WIDTH.min(area.width);
    if w < 24 || area.height < 8 {
        return None;
    }
    let inner_w = w.saturating_sub(4);
    let sel = menu.sel.min(n.saturating_sub(1));
    let context: Vec<String> = match (menu.context, rows.get(sel)) {
        (true, Some(e)) => context_block(e, usize::from(inner_w.saturating_sub(2))),
        _ => Vec::new(),
    };
    let wanted_rows = n.clamp(1, MAX_ROWS);
    let want_h = 2 + 1 + wanted_rows + context.len() + 1 + 1;
    let h = u16::try_from(want_h).unwrap_or(u16::MAX).min(area.height);
    let fixed = 5;
    let visible = wanted_rows.min(usize::from(h).saturating_sub(fixed).max(1));
    let mut context = context;
    context.truncate(usize::from(h).saturating_sub(fixed + visible));
    let rect = Rect::new(
        area.x + (area.width - w) / 2,
        area.y + (area.height - h) / 2,
        w,
        h,
    );
    let inner = rect.inner(Margin::new(2, 1));
    let top = (sel + 1).saturating_sub(visible);
    let list_y = inner.y + 1;
    let hint_y = inner.bottom().saturating_sub(1);
    let open_label = format!(
        "Open ({})",
        if menu.ignored {
            st.qmenu_ignored_count(menu)
        } else {
            n
        }
    );
    let ign_label = format!(
        "Ignored ({})",
        if menu.ignored {
            n
        } else {
            st.qmenu_ignored_count(menu)
        }
    );
    let open_w = open_label.width() as u16;
    let tabs = [
        (false, inner.x, open_w),
        (true, inner.x + open_w + 2, ign_label.width() as u16),
    ];
    let mut geom_rows = Vec::new();
    let mut ctx_lines = Vec::new();
    let mut y = list_y;
    let show_buttons = inner.width >= BUTTONS_MIN;
    for (i, e) in rows.iter().enumerate().skip(top).take(visible) {
        if y >= hint_y {
            break;
        }
        let age = short_age(st.now_s, e.q.asked_s);
        let age_w = age.width() as u16;
        let mut buttons = Vec::new();
        let mut right = inner.right();
        if show_buttons {
            let mut order = vec![QButton::Ignore, QButton::Answer, QButton::Context];
            if inner.width >= WIDE_BUTTONS_MIN {
                if st.forkable.contains(&e.q.session) {
                    order.push(QButton::Fork);
                }
                order.push(QButton::Goto);
            }
            for b in order {
                let bw = b.label(menu.ignored).width() as u16 + 2;
                right = right.saturating_sub(bw);
                buttons.push((b, right, bw));
                right = right.saturating_sub(1);
            }
        }
        let age_x = right.saturating_sub(age_w + 1).max(inner.x + 4);
        geom_rows.push(QRowGeom {
            idx: i,
            y,
            text_x: inner.x + 2,
            text_w: age_x.saturating_sub(inner.x + 3),
            age_x,
            buttons,
        });
        y += 1;
        if i == sel {
            for line in &context {
                if y >= hint_y {
                    break;
                }
                ctx_lines.push((y, line.clone()));
                y += 1;
            }
        }
    }
    Some(QGeom {
        rect,
        inner,
        tabs,
        rows: geom_rows,
        context: ctx_lines,
        hint_y,
        empty_y: (n == 0).then_some(list_y),
    })
}

pub(crate) fn draw_qmenu(buf: &mut Buffer, area: Rect, st: &ViewState) {
    let (Some(menu), Some(g)) = (st.qmenu.as_ref(), qgeom(area, st)) else {
        return;
    };
    let rows = st.qmenu_rows(menu);
    let sel = menu.sel.min(rows.len().saturating_sub(1));
    Clear.render(g.rect, buf);
    let title = match menu
        .scope
        .as_deref()
        .and_then(|s| st.sessions.iter().find(|r| r.id == s))
    {
        Some(r) => format!(" Questions · {} ", sanitize_text(&r.name)),
        None => " Questions ".to_string(),
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(title)
        .render(g.rect, buf);
    let counts = [
        format!(
            "Open ({})",
            if menu.ignored {
                st.qmenu_ignored_count(menu)
            } else {
                rows.len()
            }
        ),
        format!(
            "Ignored ({})",
            if menu.ignored {
                rows.len()
            } else {
                st.qmenu_ignored_count(menu)
            }
        ),
    ];
    for ((ignored, x, w), label) in g.tabs.iter().zip(&counts) {
        let style = if *ignored == menu.ignored {
            Style::new().add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            dim()
        };
        put(buf, *x, g.inner.y, label, style, *w);
    }
    if let Some(y) = g.empty_y {
        let msg = if menu.ignored {
            "Nothing ignored"
        } else {
            "No open questions"
        };
        put(
            buf,
            g.inner.x + 2,
            y,
            msg,
            dim(),
            g.inner.width.saturating_sub(2),
        );
    }
    for r in &g.rows {
        let Some(e) = rows.get(r.idx) else {
            continue;
        };
        let selected = r.idx == sel;
        if selected {
            buf.set_style(
                Rect::new(g.inner.x, r.y, g.inner.width, 1),
                Style::new().bg(colors().sidebar_bg),
            );
        }
        put(
            buf,
            g.inner.x,
            r.y,
            if selected { "▸" } else { " " },
            Style::new(),
            1,
        );
        let text = match (
            menu.scope.as_deref(),
            st.sessions.iter().find(|s| s.id == e.q.session),
        ) {
            (None, Some(row)) => format!("{}: {}", row.name, e.q.text),
            _ => e.q.text.clone(),
        };
        put(
            buf,
            r.text_x,
            r.y,
            &clip(&text, usize::from(r.text_w)),
            Style::new(),
            r.text_w,
        );
        put(
            buf,
            r.age_x,
            r.y,
            &short_age(st.now_s, e.q.asked_s),
            dim(),
            g.inner.right().saturating_sub(r.age_x),
        );
        for (b, x, w) in &r.buttons {
            let on = st
                .pointer
                .is_some_and(|(px, py)| py == r.y && px >= *x && px < x + w);
            let label = format!("[{}]", b.label(menu.ignored));
            put(buf, *x, r.y, &label, button_style(on, true), *w);
        }
    }
    for (y, line) in &g.context {
        put(
            buf,
            g.inner.x + 4,
            *y,
            line,
            dim(),
            g.inner.width.saturating_sub(4),
        );
    }
    let verb = if menu.ignored { "restore" } else { "ignore" };
    let long = format!("a answer · g go to · f fork · c context · i {verb} · Tab list · Esc close");
    let hint = if long.width() <= usize::from(g.inner.width) {
        long
    } else {
        format!("a answer · g go to · f fork · c context · i {verb}")
    };
    put(buf, g.inner.x, g.hint_y, &hint, dim(), g.inner.width);
}
