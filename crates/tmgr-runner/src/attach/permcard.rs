use super::mouse::{Kind, MouseEvent};
use super::render::{dim, put};
use super::theme::colors;
use super::view::{contains, layout, MouseOut, SidebarLine, ViewState};
use crate::sanitize::sanitize_text;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, BorderType, Clear, Widget};
use std::collections::{HashMap, HashSet};
use tmgr_ipc::api::{PermissionAnswer, PermissionCard, PermissionChoice};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const REARM_MS: u64 = 300;
const MIN_CARD_W: usize = 12;
const COMPACT_ROWS: usize = 4;
const MAX_OVERLAY_W: u16 = 100;
const WHEEL_ROWS: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermBtn {
    Allow,
    Deny,
    Always,
    Confirm,
    Cancel,
    ShowAll,
    OpenPane,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Title,
    Plain,
    Dim,
    Danger,
    Btn(bool),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Seg {
    text: String,
    role: Role,
    btn: Option<PermBtn>,
}

type Row = Vec<Seg>;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Overlay {
    card: String,
    scroll: usize,
    seen_end: bool,
}

#[derive(Clone, Copy)]
enum Mode {
    Sidebar { fits: bool },
    Overlay { end: bool },
}

#[derive(Clone, Debug, Default)]
pub struct PermUi {
    pub now_ms: u64,
    ticked: u64,
    on: HashSet<String>,
    order: Vec<String>,
    arm: HashMap<String, u64>,
    sent: HashSet<String>,
    confirm: Option<(String, u64)>,
    overlay: Option<Overlay>,
}

fn clean(s: &str) -> String {
    sanitize_text(&s.replace('\t', "    ")).into_owned()
}

pub(crate) fn wrap_line(line: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let cells: Vec<(char, usize)> = line.chars().map(|c| (c, c.width().unwrap_or(0))).collect();
    let text = |from: usize, to: usize| cells[from..to].iter().map(|(c, _)| *c).collect::<String>();
    let mut out = Vec::new();
    let (mut start, mut used, mut brk) = (0, 0, None::<usize>);
    let mut i = 0;
    while i < cells.len() {
        let (c, w) = cells[i];
        if used + w > width && i > start {
            let cut = brk.filter(|b| *b > start).unwrap_or(i);
            out.push(text(start, cut));
            start = cut;
            used = cells[start..i].iter().map(|(_, w)| w).sum();
            brk = cells[start..i]
                .iter()
                .rposition(|(c, _)| *c == ' ')
                .map(|p| start + p + 1)
                .filter(|b| *b > start);
            continue;
        }
        used += w;
        if c == ' ' {
            brk = Some(i + 1);
        }
        i += 1;
    }
    out.push(text(start, cells.len()));
    out
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    text.split('\n')
        .flat_map(|l| wrap_line(&clean(l.strip_suffix('\r').unwrap_or(l)), width))
        .collect()
}

fn seg(text: impl Into<String>, role: Role) -> Seg {
    Seg {
        text: text.into(),
        role,
        btn: None,
    }
}

fn plain_rows(lines: Vec<String>, role: Role) -> Vec<Row> {
    lines.into_iter().map(|l| vec![seg(l, role)]).collect()
}

fn button(label: &str, btn: PermBtn, live: bool) -> Seg {
    Seg {
        text: format!(" {label} "),
        role: Role::Btn(live),
        btn: Some(btn),
    }
}

fn long_button(key: &str, label: &str, btn: PermBtn, live: bool, w: usize) -> Vec<Row> {
    let lead = format!(" {key} ");
    let pad = " ".repeat(lead.width());
    wrap_text(label, w.saturating_sub(lead.width() + 1).max(1))
        .into_iter()
        .enumerate()
        .map(|(i, line)| {
            vec![Seg {
                text: format!("{}{line} ", if i == 0 { &lead } else { &pad }),
                role: Role::Btn(live),
                btn: Some(btn),
            }]
        })
        .collect()
}

fn flow(segs: Vec<Seg>, width: usize) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    let mut used = 0;
    for s in segs {
        let w = s.text.width();
        if !rows.is_empty() && used > 0 && used + 1 + w <= width {
            rows.last_mut().unwrap().push(seg(" ", Role::Plain));
            used += 1;
        } else {
            rows.push(Vec::new());
            used = 0;
        }
        rows.last_mut().unwrap().push(s);
        used += w;
    }
    rows
}

fn btn_at(row: &Row, x: u16) -> Option<(PermBtn, bool)> {
    let mut at = 0usize;
    for s in row {
        let w = s.text.width();
        if usize::from(x) >= at && usize::from(x) < at + w {
            return match (s.btn, s.role) {
                (Some(b), Role::Btn(live)) => Some((b, live)),
                _ => None,
            };
        }
        at += w;
    }
    None
}

fn live_in(rows: &[Row], btn: PermBtn) -> bool {
    rows.iter()
        .flatten()
        .any(|s| s.btn == Some(btn) && s.role == Role::Btn(true))
}

fn card_w(width: u16) -> usize {
    usize::from(width.saturating_sub(4))
}

fn seg_style(s: &Seg, danger: bool) -> Style {
    let c = colors();
    match s.role {
        Role::Title => Style::new().fg(c.strong).add_modifier(Modifier::BOLD),
        Role::Plain if danger => Style::new().fg(c.bad),
        Role::Plain => Style::new(),
        Role::Dim => dim(),
        Role::Danger => Style::new().fg(c.bad).add_modifier(Modifier::BOLD),
        Role::Btn(false) => dim().add_modifier(Modifier::DIM),
        Role::Btn(true) => {
            let fg = match s.btn {
                Some(PermBtn::Allow | PermBtn::Always) => c.ok,
                Some(PermBtn::Confirm) => c.bad,
                Some(PermBtn::Deny) => c.warn,
                _ => c.strong,
            };
            Style::new()
                .fg(fg)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED)
        }
    }
}

fn draw_segs(buf: &mut Buffer, x: u16, y: u16, right: u16, row: &Row, danger: bool) {
    let mut x = x;
    for s in row {
        if x >= right {
            break;
        }
        x = put(buf, x, y, &s.text, seg_style(s, danger), right - x);
    }
}

impl ViewState {
    pub fn cards_on(&self, session: &str) -> bool {
        self.perm.on.contains(session)
    }

    pub fn wants_perm_poll(&self) -> bool {
        !self.perm.on.is_empty()
    }

    pub fn perm_overlay_open(&self) -> bool {
        self.perm.overlay.is_some()
    }

    pub fn perm_clock(&mut self, now_ms: u64) {
        self.perm.now_ms = now_ms;
    }

    pub fn set_cards_on(&mut self, on: HashSet<String>) -> bool {
        if on == self.perm.on {
            return false;
        }
        self.perm.on = on;
        let cards = std::mem::take(&mut self.perm_cards);
        self.set_perm_cards(cards);
        true
    }

    pub fn set_perm_cards(&mut self, cards: Vec<PermissionCard>) -> bool {
        let now = self.perm.now_ms;
        let mut seen = HashSet::new();
        let cards: Vec<PermissionCard> = cards
            .into_iter()
            .filter(|c| {
                self.perm.on.contains(&c.session)
                    && self.is_local(&c.session)
                    && self.sessions.iter().any(|r| r.id == c.session)
                    && seen.insert(c.session.clone())
            })
            .collect();
        let ids: Vec<String> = cards.iter().map(|c| c.id.clone()).collect();
        for c in &cards {
            self.perm
                .arm
                .entry(c.id.clone())
                .or_insert(now + c.armed_in_ms);
        }
        if ids != self.perm.order {
            for id in &ids {
                if let Some(d) = self.perm.arm.get_mut(id) {
                    *d = (*d).max(now + REARM_MS);
                }
            }
        }
        self.perm.arm.retain(|id, _| ids.contains(id));
        self.perm.sent.retain(|id| ids.contains(id));
        if self
            .perm
            .confirm
            .as_ref()
            .is_some_and(|(id, _)| !ids.contains(id))
        {
            self.perm.confirm = None;
        }
        if self
            .perm
            .overlay
            .as_ref()
            .is_some_and(|o| !ids.contains(&o.card))
        {
            self.perm.overlay = None;
        }
        self.perm.order = ids;
        let changed = cards != self.perm_cards;
        self.perm_cards = cards;
        changed
    }

    pub fn perm_tick(&mut self) -> bool {
        let (from, now) = (self.perm.ticked, self.perm.now_ms);
        self.perm.ticked = now;
        self.perm
            .arm
            .values()
            .copied()
            .chain(self.perm.confirm.as_ref().map(|(_, d)| *d))
            .any(|d| d > from && d <= now)
    }

    pub fn perm_refused(&mut self, card: &str) {
        self.perm.sent.remove(card);
    }

    fn perm_card(&self, id: &str) -> Option<&PermissionCard> {
        self.perm_cards.iter().find(|c| c.id == id)
    }

    fn perm_card_of(&self, session: &str) -> Option<&PermissionCard> {
        self.perm_cards.iter().find(|c| c.session == session)
    }

    fn perm_armed(&self, id: &str) -> bool {
        self.perm.arm.get(id).is_some_and(|d| *d > self.perm.now_ms)
    }

    fn perm_confirming(&self, id: &str) -> bool {
        self.perm.confirm.as_ref().is_some_and(|(c, _)| c == id)
    }

    pub(crate) fn perm_line(&self, session: &str, out: &mut Vec<SidebarLine>) {
        if self.perm_card_of(session).is_some() {
            out.push(SidebarLine::Perm {
                session: session.to_string(),
                row: 0,
                full: false,
            });
        }
    }

    fn head_rows(&self, card: &PermissionCard, w: usize) -> Vec<Row> {
        let name = self
            .sessions
            .iter()
            .find(|r| r.id == card.session)
            .map_or(card.session.as_str(), |r| r.name.as_str());
        let mut rows = plain_rows(
            wrap_line(&clean(&format!("{name} · {}", card.tool)), w),
            Role::Title,
        );
        if !card.danger.is_empty() {
            let line = clean(&format!("Dangerous: {}", card.danger.join(", ")));
            rows.extend(plain_rows(wrap_line(&line, w), Role::Danger));
        }
        rows
    }

    fn body_rows(card: &PermissionCard, w: usize) -> Vec<Row> {
        let mut rows = plain_rows(wrap_text(&card.text, w), Role::Plain);
        if let Some(reason) = &card.reason {
            rows.extend(plain_rows(
                wrap_text(&format!("Reason: {reason}"), w),
                Role::Dim,
            ));
        }
        if let Some(path) = card.path.as_ref().filter(|p| !card.text.contains(*p)) {
            rows.extend(plain_rows(
                wrap_text(&format!("Path: {path}"), w),
                Role::Dim,
            ));
        }
        rows
    }

    fn foot_rows(&self, card: &PermissionCard, w: usize, mode: Mode) -> Vec<Row> {
        if self.perm.sent.contains(&card.id) {
            return vec![vec![seg("Sent", Role::Dim)]];
        }
        let sidebar_cut = matches!(mode, Mode::Sidebar { fits: false });
        let reached = match mode {
            Mode::Sidebar { fits } => fits,
            Mode::Overlay { end } => end,
        };
        let base = card.can_answer && !self.perm_armed(&card.id) && reached;
        let mut segs = Vec::new();
        let mut rows = Vec::new();
        if sidebar_cut {
            segs.push(button("Show all", PermBtn::ShowAll, true));
        }
        if !card.can_answer {
            let why = card.why_not.as_deref().unwrap_or("view only");
            rows.extend(plain_rows(wrap_text(why, w), Role::Dim));
            segs.push(button("Open pane", PermBtn::OpenPane, true));
            rows.extend(flow(segs, w));
            return rows;
        }
        if self.perm_confirming(&card.id) {
            let ready = base
                && self
                    .perm
                    .confirm
                    .as_ref()
                    .is_some_and(|(_, d)| *d <= self.perm.now_ms);
            rows.extend(flow(vec![button("n Cancel", PermBtn::Cancel, true)], w));
            rows.extend(long_button(
                "c",
                "Confirm: allow this",
                PermBtn::Confirm,
                ready,
                w,
            ));
            return rows;
        }
        if sidebar_cut {
            return flow(segs, w);
        }
        segs.push(button("y Allow", PermBtn::Allow, base));
        segs.push(button("n Deny", PermBtn::Deny, base));
        rows.extend(flow(segs, w));
        if let Some(label) = card
            .always_label
            .as_ref()
            .filter(|_| card.danger.is_empty())
        {
            rows.extend(long_button("a", label, PermBtn::Always, base, w));
        }
        rows
    }

    fn perm_rows(&self, card: &PermissionCard, w: usize, full: bool) -> Vec<Row> {
        let mut rows = self.head_rows(card, w);
        if full {
            rows.extend(Self::body_rows(card, w));
        } else {
            let n = Self::body_rows(card, w).len();
            rows.push(vec![seg(
                format!("Too long for the sidebar ({n} lines)"),
                Role::Dim,
            )]);
        }
        rows.extend(self.foot_rows(card, w, Mode::Sidebar { fits: full }));
        rows
    }

    pub(crate) fn perm_expand(
        &self,
        plan: Vec<SidebarLine>,
        room: usize,
        width: u16,
    ) -> Vec<SidebarLine> {
        let slots = plan
            .iter()
            .filter(|l| matches!(l, SidebarLine::Perm { .. }))
            .count();
        if slots == 0 {
            return plan;
        }
        let w = card_w(width);
        let mut avail = room.saturating_sub(plan.len() - slots);
        let mut left = slots;
        let mut out = Vec::new();
        for line in plan {
            let SidebarLine::Perm { session, .. } = line else {
                out.push(line);
                continue;
            };
            left -= 1;
            let Some(card) = self.perm_card_of(&session) else {
                continue;
            };
            let need = self.perm_rows(card, w, true).len();
            let full = w >= MIN_CARD_W && need + left * COMPACT_ROWS <= avail;
            let n = if full {
                need
            } else {
                self.perm_rows(card, w, false).len()
            };
            avail = avail.saturating_sub(n);
            for row in 0..n {
                out.push(SidebarLine::Perm {
                    session: session.clone(),
                    row,
                    full,
                });
            }
        }
        out
    }

    fn perm_send(
        &mut self,
        card: &PermissionCard,
        choice: PermissionChoice,
        confirmed: bool,
    ) -> Option<PermissionAnswer> {
        self.perm.sent.insert(card.id.clone());
        self.perm.confirm = None;
        self.perm.overlay = None;
        Some(PermissionAnswer {
            session: card.session.clone(),
            card: card.id.clone(),
            choice,
            confirmed,
        })
    }

    fn perm_dispatch(&mut self, card: &PermissionCard, btn: PermBtn) -> Option<PermissionAnswer> {
        match btn {
            PermBtn::Allow if !card.danger.is_empty() => {
                self.perm.confirm = Some((card.id.clone(), self.perm.now_ms + REARM_MS));
                None
            }
            PermBtn::Allow => self.perm_send(card, PermissionChoice::Yes, false),
            PermBtn::Deny => self.perm_send(card, PermissionChoice::No, false),
            PermBtn::Always => self.perm_send(card, PermissionChoice::Always, false),
            PermBtn::Confirm if self.perm_confirming(&card.id) => {
                self.perm_send(card, PermissionChoice::Yes, true)
            }
            PermBtn::Confirm => None,
            PermBtn::Cancel => {
                self.perm.confirm = None;
                None
            }
            PermBtn::ShowAll => {
                self.perm.overlay = Some(Overlay {
                    card: card.id.clone(),
                    scroll: 0,
                    seen_end: false,
                });
                None
            }
            PermBtn::OpenPane => {
                self.perm.overlay = None;
                self.open(&card.session);
                None
            }
        }
    }

    pub fn open_permissions(&mut self) {
        if self.perm.overlay.take().is_some() {
            return;
        }
        let Some(id) = self.perm_cards.first().map(|c| c.id.clone()) else {
            self.notice = Some("no permission requests".into());
            return;
        };
        self.palette = None;
        self.help = false;
        self.grab = None;
        self.perm.overlay = Some(Overlay {
            card: id,
            scroll: 0,
            seen_end: false,
        });
    }

    fn overlay_geom(&self, area: Rect) -> Option<OGeom> {
        let ov = self.perm.overlay.as_ref()?;
        let card = self.perm_card(&ov.card)?;
        let w = MAX_OVERLAY_W.min(area.width);
        if w < 24 || area.height < 9 {
            return None;
        }
        let iw = usize::from(w - 4);
        let head = self.head_rows(card, iw);
        let body = Self::body_rows(card, iw);
        let foot_len = self.foot_rows(card, iw, Mode::Overlay { end: false }).len();
        let chrome = 2 + head.len() + 1 + foot_len;
        let h = (chrome + body.len()).min(usize::from(area.height));
        let view_h = h.checked_sub(chrome).filter(|v| *v > 0)?;
        let max_scroll = body.len().saturating_sub(view_h);
        let end = ov.seen_end || ov.scroll >= max_scroll;
        let foot = self.foot_rows(card, iw, Mode::Overlay { end });
        let h = h as u16;
        let rect = Rect::new(
            area.x + (area.width - w) / 2,
            area.y + (area.height - h) / 2,
            w,
            h,
        );
        Some(OGeom {
            rect,
            head,
            body,
            foot,
            view_h,
            max_scroll,
            scroll: ov.scroll.min(max_scroll),
            end,
        })
    }

    fn overlay_scroll(&mut self, area: Rect, delta: isize) {
        let Some(g) = self.overlay_geom(area) else {
            return;
        };
        let next = g.scroll.saturating_add_signed(delta).min(g.max_scroll);
        if let Some(ov) = self.perm.overlay.as_mut() {
            ov.scroll = next;
            ov.seen_end |= next >= g.max_scroll;
        }
    }

    fn overlay_key(&mut self, area: Rect, btn: PermBtn) -> Option<PermissionAnswer> {
        let g = self.overlay_geom(area)?;
        if !live_in(&g.foot, btn) {
            return None;
        }
        let card = self.perm_card(&self.perm.overlay.as_ref()?.card)?.clone();
        self.perm_dispatch(&card, btn)
    }

    pub fn edit_perm(&mut self, area: Rect, data: &[u8]) -> Option<PermissionAnswer> {
        let mut i = 0;
        while i < data.len() && self.perm.overlay.is_some() {
            let b = data[i];
            i += 1;
            let confirming = self
                .perm
                .overlay
                .as_ref()
                .is_some_and(|o| self.perm_confirming(&o.card));
            let page =
                isize::try_from(self.overlay_geom(area).map_or(1, |g| g.view_h)).unwrap_or(1);
            let ans = match b {
                0x1b => {
                    let rest = &data[i..];
                    let (delta, used) = match rest {
                        [b'[' | b'O', b'A', ..] => (Some(-1), 2),
                        [b'[' | b'O', b'B', ..] => (Some(1), 2),
                        [b'[', b'5', b'~', ..] => (Some(-page), 3),
                        [b'[', b'6', b'~', ..] => (Some(page), 3),
                        [b'[', tail @ ..] => (
                            None,
                            1 + tail
                                .iter()
                                .take_while(|c| !(0x40..=0x7e).contains(*c))
                                .count()
                                + 1,
                        ),
                        _ => {
                            self.perm.overlay = None;
                            self.perm.confirm = None;
                            return None;
                        }
                    };
                    i += used.min(rest.len());
                    if let Some(d) = delta {
                        self.overlay_scroll(area, d);
                    }
                    None
                }
                b'y' | b'Y' => self.overlay_key(area, PermBtn::Allow),
                b'n' | b'N' if confirming => self.overlay_key(area, PermBtn::Cancel),
                b'n' | b'N' => self.overlay_key(area, PermBtn::Deny),
                b'a' | b'A' => self.overlay_key(area, PermBtn::Always),
                b'c' | b'C' => self.overlay_key(area, PermBtn::Confirm),
                _ => None,
            };
            if ans.is_some() {
                return ans;
            }
        }
        None
    }

    pub(crate) fn perm_mouse(&mut self, area: Rect, ev: MouseEvent, out: &mut MouseOut) -> bool {
        if self.perm.overlay.is_some() {
            self.overlay_mouse(area, ev, out);
            return true;
        }
        if !ev.is_left_press() || self.perm_cards.is_empty() {
            return false;
        }
        let Some(sb) = layout(area, self)
            .sidebar
            .filter(|sb| contains(*sb, ev.col, ev.row))
        else {
            return false;
        };
        let sl = self.sidebar_layout(sb);
        let Some((SidebarLine::Perm { session, row, full }, _)) =
            sl.lines.iter().find(|(_, y)| *y == ev.row)
        else {
            return false;
        };
        let Some(card) = self.perm_card_of(session).cloned() else {
            return false;
        };
        let rows = self.perm_rows(&card, card_w(sb.width), *full);
        let Some((btn, live)) = rows
            .get(*row)
            .zip(ev.col.checked_sub(sb.x + 3))
            .and_then(|(r, x)| btn_at(r, x))
        else {
            return false;
        };
        out.redraw = true;
        if live {
            out.perm = self.perm_dispatch(&card, btn);
        }
        true
    }

    fn overlay_mouse(&mut self, area: Rect, ev: MouseEvent, out: &mut MouseOut) {
        out.redraw = true;
        match ev.kind() {
            kind @ (Kind::WheelUp | Kind::WheelDown) => {
                let d = WHEEL_ROWS as isize;
                self.overlay_scroll(area, if kind == Kind::WheelUp { -d } else { d });
            }
            Kind::Press => {
                self.grab = Some(String::new());
                if !ev.is_left_press() {
                    return;
                }
                let Some(g) = self
                    .overlay_geom(area)
                    .filter(|g| contains(g.rect, ev.col, ev.row))
                else {
                    self.perm.overlay = None;
                    return;
                };
                let foot_y = g.rect.bottom() - 1 - g.foot.len() as u16;
                let x = ev.col.checked_sub(g.rect.x + 2);
                let hit = ev
                    .row
                    .checked_sub(foot_y)
                    .and_then(|r| g.foot.get(usize::from(r)))
                    .zip(x)
                    .and_then(|(r, x)| btn_at(r, x));
                let Some((btn, true)) = hit else {
                    return;
                };
                let Some(card) = self
                    .perm
                    .overlay
                    .as_ref()
                    .and_then(|o| self.perm_card(&o.card))
                    .cloned()
                else {
                    return;
                };
                out.perm = self.perm_dispatch(&card, btn);
            }
            Kind::Release if self.grab.as_deref() == Some("") => self.grab = None,
            _ => {}
        }
    }
}

struct OGeom {
    rect: Rect,
    head: Vec<Row>,
    body: Vec<Row>,
    foot: Vec<Row>,
    view_h: usize,
    max_scroll: usize,
    scroll: usize,
    end: bool,
}

pub(crate) fn draw_row(
    buf: &mut Buffer,
    area: Rect,
    st: &ViewState,
    session: &str,
    row: usize,
    full: bool,
    y: u16,
) {
    let Some(card) = st.perm_card_of(session) else {
        return;
    };
    let rows = st.perm_rows(card, card_w(area.width), full);
    let Some(r) = rows.get(row) else {
        return;
    };
    let danger = !card.danger.is_empty();
    let rail = if danger {
        Style::new().fg(colors().bad)
    } else {
        Style::new().fg(colors().warn)
    };
    put(buf, area.x + 1, y, "│", rail, 1);
    draw_segs(
        buf,
        area.x + 3,
        y,
        area.right().saturating_sub(1),
        r,
        danger,
    );
}

pub(crate) fn draw_overlay(buf: &mut Buffer, area: Rect, st: &ViewState) {
    let Some(g) = st.overlay_geom(area) else {
        return;
    };
    let Some(card) = st.perm.overlay.as_ref().and_then(|o| st.perm_card(&o.card)) else {
        return;
    };
    let danger = !card.danger.is_empty();
    let border = if danger {
        Style::new().fg(colors().bad)
    } else {
        Style::new().fg(colors().warn)
    };
    Clear.render(g.rect, buf);
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title(" Permission request ")
        .render(g.rect, buf);
    let x = g.rect.x + 2;
    let right = g.rect.right() - 2;
    let mut y = g.rect.y + 1;
    for r in &g.head {
        draw_segs(buf, x, y, right, r, danger);
        y += 1;
    }
    for r in g.body.iter().skip(g.scroll).take(g.view_h) {
        draw_segs(buf, x, y, right, r, danger);
        y += 1;
    }
    let hint = if card.can_answer && !g.end && !st.perm.sent.contains(&card.id) {
        format!(
            "Scroll to the end to answer ({}/{})",
            g.scroll + 1,
            g.max_scroll + 1
        )
    } else {
        "Esc closes".to_string()
    };
    put(buf, x, y, &hint, dim(), right - x);
    y += 1;
    for r in &g.foot {
        draw_segs(buf, x, y, right, r, danger);
        y += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attach::{layout as view_layout, render, ScreenSource, SessionRow};
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use tmgr_ipc::api::{Snapshot, State, StateKind};

    const AREA: Rect = Rect {
        x: 0,
        y: 0,
        width: 140,
        height: 40,
    };

    struct NoScreens;

    impl ScreenSource for NoScreens {
        fn snapshot(&self, _: &str) -> Option<&Snapshot> {
            None
        }
    }

    fn session(id: &str) -> SessionRow {
        SessionRow {
            id: id.into(),
            name: format!("name-{id}"),
            state: State {
                kind: StateKind::Working,
                since_ms: 0,
                detail: None,
                exit_code: None,
            },
            age: "1m".into(),
            you: None,
            esc: false,
            resumed_from: None,
        }
    }

    fn view() -> ViewState {
        let mut v = ViewState::new(vec![session("a"), session("b")], "local");
        v.width = 140;
        v.set_cards_on(["a".to_string()].into());
        v
    }

    fn card(id: &str, text: &str) -> PermissionCard {
        PermissionCard {
            id: id.into(),
            session: "a".into(),
            agent: "claude".into(),
            tool: "Bash".into(),
            text: text.into(),
            can_answer: true,
            ..PermissionCard::default()
        }
    }

    fn show(v: &mut ViewState, cards: Vec<PermissionCard>) {
        v.perm_clock(10_000);
        v.set_perm_cards(cards);
        v.perm_clock(20_000);
    }

    fn dump(v: &ViewState, w: u16, h: u16) -> (String, Terminal<TestBackend>) {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| render(f, v, &NoScreens)).unwrap();
        let buf = term.backend().buffer();
        let text = buf
            .content()
            .chunks(usize::from(w))
            .map(|r| r.iter().map(|c| c.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        (text, term)
    }

    fn click(v: &mut ViewState, col: u16, row: u16) -> MouseOut {
        let info = |_: &str| crate::attach::PaneInfo {
            mouse: Default::default(),
            scroll_max: 0,
            alt_screen: false,
            app_cursor: false,
        };
        let ev = MouseEvent {
            cb: 0,
            col,
            row,
            release: false,
        };
        v.on_mouse(AREA, ev, &info)
    }

    fn find_text(v: &ViewState, needle: &str) -> Option<(u16, u16)> {
        let (text, _) = dump(v, AREA.width, AREA.height);
        text.lines().enumerate().find_map(|(y, l)| {
            let byte = l.find(needle)?;
            Some((l[..byte].chars().count() as u16, y as u16))
        })
    }

    fn click_text(v: &mut ViewState, needle: &str) -> MouseOut {
        let (x, y) = find_text(v, needle).unwrap_or_else(|| panic!("{needle} not on screen"));
        click(v, x + 1, y)
    }

    const LONG: &str =
        "cargo test --workspace --all-features\n  && echo done\n\nthen   keep   spacing";

    #[test]
    fn wrapping_keeps_every_character() {
        for w in 1..30 {
            for s in [
                "",
                "a b  c",
                "  lead",
                "trail  ",
                "averyveryverylongwordthatmustbreak x",
                "世界 世界 abc",
            ] {
                let rows = wrap_line(s, w);
                assert_eq!(rows.concat(), s, "w={w} {s:?}");
                assert!(rows.iter().all(|r| r.width() <= w.max(2)), "w={w} {rows:?}");
            }
        }
    }

    #[test]
    fn full_text_is_rendered_without_cuts() {
        let mut v = view();
        show(&mut v, vec![card("c1", LONG)]);
        let (text, _) = dump(&v, 140, 40);
        for word in LONG.split_whitespace() {
            assert!(text.contains(word), "{word} missing\n{text}");
        }
        assert!(text.contains("y Allow") && text.contains("n Deny"));
    }

    #[test]
    fn text_that_does_not_fit_hides_the_buttons_and_offers_show_all() {
        let mut v = view();
        let long = (0..200)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        show(&mut v, vec![card("c1", &long)]);
        let (text, _) = dump(&v, 140, 40);
        assert!(text.contains("Show all"));
        assert!(!text.contains("y Allow") && !text.contains("n Deny"));
        assert!(!text.contains("word3"));
        let out = click_text(&mut v, "Show all");
        assert!(out.perm.is_none());
        assert!(v.perm_overlay_open());
    }

    #[test]
    fn overlay_answers_only_after_the_end_is_reached() {
        let mut v = view();
        let long = (0..40)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        show(&mut v, vec![card("c1", &long)]);
        v.open_permissions();
        let area = Rect::new(0, 0, 80, 20);
        assert_eq!(v.edit_perm(area, b"y"), None);
        assert_eq!(v.edit_perm(area, b"n"), None);
        let (text, _) = dump(&v, 80, 20);
        assert!(text.contains("Scroll to the end"));
        for _ in 0..10 {
            v.edit_perm(area, b"\x1b[6~");
        }
        let ans = v.edit_perm(area, b"y").expect("answer");
        assert_eq!(ans.card, "c1");
        assert_eq!(ans.choice, PermissionChoice::Yes);
        assert!(!ans.confirmed);
        assert!(!v.perm_overlay_open());
    }

    #[test]
    fn overlay_keys_and_mouse_wheel_scroll_and_esc_closes() {
        let mut v = view();
        let long = (0..40)
            .map(|i| format!("line{i}"))
            .collect::<Vec<_>>()
            .join("\n");
        show(&mut v, vec![card("c1", &long)]);
        v.open_permissions();
        let area = Rect::new(0, 0, 80, 20);
        v.edit_perm(area, b"\x1b[B");
        assert_eq!(v.perm.overlay.as_ref().unwrap().scroll, 1);
        v.edit_perm(area, b"\x1b[A");
        assert_eq!(v.perm.overlay.as_ref().unwrap().scroll, 0);
        assert_eq!(v.edit_perm(area, b"\x1b"), None);
        assert!(!v.perm_overlay_open());
    }

    #[test]
    fn danger_needs_a_second_confirmation_tied_to_the_card() {
        let mut v = view();
        let mut c = card("c1", "rm -rf build");
        c.danger = vec!["rm".into()];
        show(&mut v, vec![c]);
        let (text, _) = dump(&v, 140, 40);
        assert!(text.contains("Dangerous: rm"));
        let out = click_text(&mut v, "y Allow");
        assert!(out.perm.is_none());
        let (text, _) = dump(&v, 140, 40);
        assert!(text.contains("Confirm:") && text.contains("Cancel"));
        v.perm_clock(30_000);
        let out = click_text(&mut v, "Confirm:");
        let ans = out.perm.expect("answer");
        assert_eq!((ans.card.as_str(), ans.confirmed), ("c1", true));
        assert_eq!(ans.choice, PermissionChoice::Yes);
    }

    #[test]
    fn danger_cards_draw_in_red() {
        let mut v = view();
        let mut c = card("c1", "rm -rf build");
        c.danger = vec!["rm".into()];
        show(&mut v, vec![c]);
        let (_, term) = dump(&v, 140, 40);
        let (x, y) = find_text(&v, "rm -rf").unwrap();
        let cell = &term.backend().buffer()[(x, y)];
        assert_eq!(cell.fg, crate::attach::theme::colors().bad);
    }

    #[test]
    fn a_changed_card_cancels_the_confirmation() {
        let mut v = view();
        let mut c = card("c1", "rm -rf build");
        c.danger = vec!["rm".into()];
        show(&mut v, vec![c.clone()]);
        click_text(&mut v, "y Allow");
        assert!(v.perm.confirm.is_some());
        let mut other = c;
        other.id = "c2".into();
        show(&mut v, vec![other]);
        assert!(v.perm.confirm.is_none());
        let out = click_text(&mut v, "y Allow");
        assert!(out.perm.is_none());
        assert!(v.perm.confirm.is_some());
        show(&mut v, vec![]);
        assert!(v.perm.confirm.is_none());
    }

    #[test]
    fn a_view_only_card_has_no_answer_buttons() {
        let mut v = view();
        let mut c = card("c1", "Write a file");
        c.can_answer = false;
        c.why_not = Some("prompt not recognised".into());
        show(&mut v, vec![c]);
        let (text, _) = dump(&v, 140, 40);
        assert!(text.contains("recognised"));
        assert!(text.contains("Open pane"));
        assert!(!text.contains("y Allow") && !text.contains("n Deny"));
        let out = click_text(&mut v, "Open pane");
        assert!(out.perm.is_none());
    }

    #[test]
    fn always_button_appears_only_with_a_label_and_keeps_it_verbatim() {
        let mut v = view();
        show(&mut v, vec![card("c1", "ls")]);
        let (text, _) = dump(&v, 140, 40);
        assert!(!text.contains(" a "));
        let mut c = card("c1", "ls");
        c.always_label = Some("Yes, and don't ask again for ls commands".into());
        show(&mut v, vec![c]);
        let (text, _) = dump(&v, 140, 40);
        assert!(text.contains("Yes, and"));
        assert!(text.contains("commands"));
        let out = click_text(&mut v, "Yes, and");
        assert_eq!(out.perm.expect("answer").choice, PermissionChoice::Always);
    }

    #[test]
    fn clicks_while_arming_do_nothing_and_then_answer_once() {
        let mut v = view();
        v.perm_clock(10_000);
        let mut c = card("c1", "ls");
        c.armed_in_ms = 300;
        v.set_perm_cards(vec![c]);
        let out = click_text(&mut v, "y Allow");
        assert!(out.perm.is_none());
        v.perm_clock(10_400);
        let out = click_text(&mut v, "y Allow");
        let ans = out.perm.expect("answer");
        assert_eq!(ans.choice, PermissionChoice::Yes);
        let out = click_text(&mut v, "Sent");
        assert!(out.perm.is_none());
        let (text, _) = dump(&v, 140, 40);
        assert!(text.contains("Sent") && !text.contains("y Allow"));
    }

    #[test]
    fn every_card_is_rearmed_when_the_list_changes() {
        let mut v = view();
        v.set_cards_on(["a".to_string(), "b".to_string()].into());
        let mut b = card("c2", "ls");
        b.session = "b".into();
        show(&mut v, vec![card("c1", "ls"), b.clone()]);
        assert!(!v.perm_armed("c2"));
        v.set_perm_cards(vec![b]);
        assert!(v.perm_armed("c2"));
    }

    #[test]
    fn a_refused_answer_lets_the_card_show_again() {
        let mut v = view();
        show(&mut v, vec![card("c1", "ls")]);
        click_text(&mut v, "n Deny");
        assert!(v.perm.sent.contains("c1"));
        v.perm_refused("c1");
        let (text, _) = dump(&v, 140, 40);
        assert!(text.contains("n Deny"));
    }

    #[test]
    fn typing_without_the_overlay_is_not_captured() {
        let mut v = view();
        show(&mut v, vec![card("c1", "ls")]);
        assert!(!v.perm_overlay_open());
        assert_eq!(v.edit_perm(AREA, b"y"), None);
        assert!(v.perm.sent.is_empty());
    }

    #[test]
    fn flag_off_means_no_marker_no_poll_and_no_card() {
        let mut v = ViewState::new(vec![session("a")], "local");
        v.width = 140;
        assert!(!v.wants_perm_poll());
        assert!(!v.cards_on("a"));
        assert!(!v.set_perm_cards(vec![card("c1", "ls")]));
        assert!(v.perm_cards.is_empty());
        let (text, _) = dump(&v, 140, 40);
        assert!(!text.contains("Allow"));
        v.set_cards_on(["a".to_string()].into());
        assert!(v.wants_perm_poll());
        v.set_cards_on(HashSet::new());
        assert!(!v.wants_perm_poll());
    }

    #[test]
    fn a_session_on_another_machine_gets_no_card() {
        let mut v = view();
        v.machines.push(crate::attach::Machine {
            name: "box".into(),
            state: crate::attach::MachineState::Connecting,
        });
        v.remote.insert("a".into(), "box".into());
        v.perm_clock(1);
        assert!(!v.set_perm_cards(vec![card("c1", "ls")]));
        assert!(v.perm_cards.is_empty());
    }

    #[test]
    fn a_vanished_card_closes_the_overlay() {
        let mut v = view();
        show(&mut v, vec![card("c1", "ls")]);
        v.open_permissions();
        assert!(v.perm_overlay_open());
        show(&mut v, vec![]);
        assert!(!v.perm_overlay_open());
        let (text, _) = dump(&v, 140, 40);
        assert!(!text.contains("Allow"));
    }

    #[test]
    fn control_characters_never_reach_the_terminal() {
        let mut v = view();
        show(
            &mut v,
            vec![card(
                "c1",
                "echo \x1b[31mred\x07\x1b]0;x\x07 \u{202e}tail\r\nnext\tcol",
            )],
        );
        let (text, _) = dump(&v, 140, 40);
        assert!(!text.chars().any(|c| c.is_control() && c != '\n'));
        assert!(text.contains("?[31mred"));
        assert!(text.contains("next    col"));
    }

    #[test]
    fn rendered_rows_rebuild_the_original_text() {
        let line = "grep -n  \"a  b\" src/some/very/long/path/to/a/file.rs | head -n 20";
        for w in [10, 14, 20, 33] {
            assert_eq!(wrap_text(line, w).concat(), line);
        }
    }

    #[test]
    fn the_cards_on_marker_shows_only_for_flagged_sessions() {
        let v = view();
        assert!(v.cards_on("a") && !v.cards_on("b"));
    }

    #[test]
    fn palette_command_opens_the_overlay_for_the_first_card() {
        let mut v = view();
        v.apply(crate::attach::Cmd::Permissions);
        assert!(!v.perm_overlay_open());
        show(&mut v, vec![card("c1", "ls")]);
        v.apply(crate::attach::Cmd::Permissions);
        assert!(v.perm_overlay_open());
        let (text, _) = dump(&v, 140, 40);
        assert!(text.contains("Permission request"));
        let _ = view_layout(AREA, &v);
    }
}
