mod mention;

use super::mouse::{Kind, MouseEvent};
use super::render::{dim, put};
use super::theme::colors;
use super::view::{MouseOut, ViewState};
use ratatui::buffer::Buffer;
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, BorderType, Clear, Widget};
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use tmgr_ipc::chat::{
    Author, AuthorKind, ChatMessage, MentionReq, MentionState, MessageKind, RoomInfo, RoomKind,
    RoomRef,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub const HISTORY_LIMIT: u32 = 200;
pub const PAIR_HINT: &str =
    "Pair this terminal to write into this machine's sessions: tmgr device pair";
pub const NEEDS_NEWER: &str = "Chat needs a newer runner";
pub const BUSY_STATUS: &str = "Session is busy, try again when it is at its prompt";
const MAX_INPUT: usize = 4000;
const LIST_W: u16 = 30;
const MIN_WIDE: u16 = 56;
const POLL_MS: u64 = 15_000;
const POLL_ERR_MS: u64 = 60_000;
const LOCAL_GROUP: &str = "This machine";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatPhase {
    NoCap,
    Loading,
    Failed(String),
    Ready,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatAction {
    Load {
        room: RoomRef,
        before: Option<String>,
    },
    Send {
        room: RoomRef,
        body: String,
        mention: Option<MentionReq>,
    },
    Inject {
        room: RoomRef,
        mention_id: String,
        confirmed_risky: bool,
    },
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoomEntry {
    pub info: RoomInfo,
    pub unread: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Thread {
    msgs: Vec<ChatMessage>,
    loaded: bool,
    loading: bool,
    more: bool,
    scroll: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatState {
    pub phase: ChatPhase,
    pub rooms: Vec<RoomEntry>,
    pub sel: usize,
    threads: HashMap<RoomRef, Thread>,
    local: Option<String>,
    polled: Option<(RoomRef, u64)>,
    input: Vec<char>,
    cur: usize,
    status: String,
    status_bad: bool,
    sending: bool,
    offset_secs: i32,
    metrics: Cell<(usize, usize)>,
    mention_cap: bool,
    labels: HashMap<String, String>,
    pair_hint: bool,
    agent_session: bool,
    pending: Option<mention::Pending>,
    menu_sel: usize,
    now_ms: u64,
}

fn is_manager(info: &RoomInfo) -> bool {
    info.room.kind == RoomKind::Manager
}

fn room_label(info: &RoomInfo) -> &str {
    if is_manager(info) {
        "Manager"
    } else {
        &info.title
    }
}

fn hhmm(ts_ms: u64, offset_secs: i32) -> String {
    let utc = i64::try_from(ts_ms)
        .ok()
        .and_then(chrono::DateTime::from_timestamp_millis);
    let zone =
        chrono::FixedOffset::east_opt(offset_secs).or_else(|| chrono::FixedOffset::east_opt(0));
    match (utc, zone) {
        (Some(t), Some(z)) => t.with_timezone(&z).format("%H:%M").to_string(),
        _ => "--:--".into(),
    }
}

fn system_like(m: &ChatMessage) -> bool {
    m.author.kind == AuthorKind::System
        || matches!(m.kind, MessageKind::System | MessageKind::Other)
}

fn author_label(a: &Author) -> String {
    match a.kind {
        AuthorKind::Owner => "Owner".into(),
        AuthorKind::Manager => "Manager".into(),
        AuthorKind::Session => a.agent.clone().unwrap_or_else(|| "Agent".into()),
        AuthorKind::System => "System".into(),
    }
}

impl ChatState {
    pub fn new() -> ChatState {
        let offset = {
            use chrono::Offset;
            chrono::Local::now().offset().fix().local_minus_utc()
        };
        ChatState {
            phase: ChatPhase::Loading,
            rooms: Vec::new(),
            sel: 0,
            threads: HashMap::new(),
            local: None,
            polled: None,
            input: Vec::new(),
            cur: 0,
            status: String::new(),
            status_bad: false,
            sending: false,
            offset_secs: offset,
            metrics: Cell::new((0, 0)),
            mention_cap: false,
            labels: HashMap::new(),
            pair_hint: false,
            agent_session: false,
            pending: None,
            menu_sel: 0,
            now_ms: 0,
        }
    }

    pub fn with_pair_hint(mut self, on: bool) -> ChatState {
        self.pair_hint = on;
        self
    }

    pub fn set_machine_labels(&mut self, labels: HashMap<String, String>) {
        self.labels = labels;
    }

    pub fn with_offset(mut self, secs: i32) -> ChatState {
        self.offset_secs = secs;
        self
    }

    pub fn unsupported() -> ChatState {
        ChatState {
            phase: ChatPhase::NoCap,
            ..ChatState::new()
        }
    }

    pub fn input_text(&self) -> String {
        self.input.iter().collect()
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn is_unread(&self, room: &RoomRef) -> bool {
        self.rooms.iter().any(|r| &r.info.room == room && r.unread)
    }

    pub fn selected(&self) -> Option<&RoomRef> {
        self.rooms.get(self.sel).map(|r| &r.info.room)
    }

    pub fn messages(&self, room: &RoomRef) -> &[ChatMessage] {
        self.threads.get(room).map_or(&[], |t| t.msgs.as_slice())
    }

    fn is_remote(&self, room: &RoomRef) -> bool {
        self.local.as_ref().is_some_and(|l| &room.machine != l)
    }

    fn grouped(&self) -> bool {
        self.rooms.iter().any(|r| self.is_remote(&r.info.room))
    }

    pub fn poll(&mut self, now_ms: u64) -> Option<ChatAction> {
        self.now_ms = now_ms;
        self.expire_pending();
        if self.phase != ChatPhase::Ready {
            return None;
        }
        let room = self.selected()?.clone();
        if !self.is_remote(&room) {
            self.polled = None;
            return None;
        }
        let due = match &self.polled {
            Some((r, at)) if *r == room => now_ms.saturating_sub(*at) >= POLL_MS,
            _ => {
                self.polled = Some((room.clone(), now_ms));
                false
            }
        };
        let th = self.thread(&room);
        if !due || th.loading || !th.loaded {
            return None;
        }
        th.loading = true;
        self.polled = Some((room.clone(), now_ms));
        Some(ChatAction::Load { room, before: None })
    }

    pub fn history_done(&mut self, room: &RoomRef, ok: bool, now_ms: u64) {
        if !self.is_remote(room) {
            return;
        }
        if let Some((r, at)) = self.polled.as_mut() {
            if r == room {
                *at = if ok {
                    now_ms
                } else {
                    now_ms + (POLL_ERR_MS - POLL_MS)
                };
            }
        }
    }

    fn say(&mut self, text: &str, bad: bool) {
        self.status = text.to_string();
        self.status_bad = bad;
    }

    fn thread(&mut self, room: &RoomRef) -> &mut Thread {
        self.threads.entry(room.clone()).or_default()
    }

    pub fn set_rooms(&mut self, rooms: Vec<RoomInfo>) -> Option<ChatAction> {
        let keep = self.selected().cloned();
        self.local = rooms.first().map(|r| r.room.machine.clone());
        let unread: HashSet<RoomRef> = self
            .rooms
            .iter()
            .filter(|r| r.unread)
            .map(|r| r.info.room.clone())
            .collect();
        let mut entries: Vec<RoomEntry> = rooms
            .into_iter()
            .map(|info| RoomEntry {
                unread: unread.contains(&info.room),
                info,
            })
            .collect();
        let local = self.local.clone();
        let mut remotes: Vec<String> = Vec::new();
        for e in &entries {
            let m = &e.info.room.machine;
            if Some(m) != local.as_ref() && !remotes.contains(m) {
                remotes.push(m.clone());
            }
        }
        entries.sort_by_key(|r| {
            let m = &r.info.room.machine;
            let group = remotes.iter().position(|x| x == m).map_or(0, |i| i + 1);
            (group, !is_manager(&r.info))
        });
        self.rooms = entries;
        self.sel = keep
            .and_then(|k| self.rooms.iter().position(|r| r.info.room == k))
            .unwrap_or(0);
        self.phase = ChatPhase::Ready;
        self.enter_selected()
    }

    pub fn rooms_failed(&mut self, message: &str) {
        self.phase = ChatPhase::Failed(message.to_string());
    }

    fn enter_selected(&mut self) -> Option<ChatAction> {
        let room = self.selected()?.clone();
        if let Some(entry) = self.rooms.get_mut(self.sel) {
            entry.unread = false;
        }
        self.say("", false);
        let th = self.thread(&room);
        if th.loaded || th.loading {
            return None;
        }
        th.loading = true;
        Some(ChatAction::Load { room, before: None })
    }

    fn select(&mut self, idx: usize) -> Option<ChatAction> {
        if idx == self.sel || idx >= self.rooms.len() {
            return None;
        }
        self.sel = idx;
        self.enter_selected()
    }

    pub fn history(
        &mut self,
        room: &RoomRef,
        before: Option<&str>,
        result: Result<Vec<ChatMessage>, String>,
    ) {
        let th = self.thread(room);
        th.loading = false;
        match result {
            Ok(list) => {
                th.more = list.len() >= HISTORY_LIMIT as usize;
                if before.is_some() {
                    let known: HashSet<String> = th.msgs.iter().map(|m| m.id.clone()).collect();
                    let mut merged: Vec<ChatMessage> = list
                        .into_iter()
                        .filter(|m| !known.contains(&m.id))
                        .collect();
                    merged.append(&mut th.msgs);
                    th.msgs = merged;
                } else {
                    let ids: HashSet<String> = list.iter().map(|m| m.id.clone()).collect();
                    let mut merged = list;
                    merged.extend(th.msgs.drain(..).filter(|m| !ids.contains(&m.id)));
                    th.msgs = merged;
                    th.loaded = true;
                }
            }
            Err(e) => {
                let selected = self.selected() == Some(room);
                if selected {
                    self.say(&format!("Cannot load history: {e}"), true);
                }
            }
        }
    }

    fn append(&mut self, m: ChatMessage) -> bool {
        let th = self.thread(&m.room.clone());
        if th.msgs.iter().any(|x| x.id == m.id) {
            return false;
        }
        th.msgs.push(m);
        true
    }

    pub fn push(&mut self, m: ChatMessage) {
        let room = m.room.clone();
        let author_agent = m.author.agent.clone();
        if !self.append(m) {
            return;
        }
        if self.phase != ChatPhase::Ready {
            return;
        }
        if !self.rooms.iter().any(|r| r.info.room == room) {
            self.rooms.push(RoomEntry {
                info: RoomInfo {
                    title: room.session.clone().unwrap_or_else(|| "Session".into()),
                    agent: author_agent,
                    state: None,
                    context_pct: None,
                    context_tokens: None,
                    model: None,
                    last: None,
                    unrestricted: false,
                    mention: None,
                    room: room.clone(),
                },
                unread: false,
            });
        }
        if self.selected() != Some(&room) {
            if let Some(r) = self.rooms.iter_mut().find(|r| r.info.room == room) {
                r.unread = true;
            }
        }
    }

    pub fn sent(&mut self, room: &RoomRef, result: Result<ChatMessage, (bool, String)>) {
        self.sending = false;
        match result {
            Ok(m) => {
                self.append(m);
                self.input.clear();
                self.cur = 0;
                self.thread(room).scroll = 0;
                self.say("", false);
            }
            Err((true, _)) => self.say(BUSY_STATUS, true),
            Err((false, e)) => self.say(&e, true),
        }
    }

    fn insert_text(&mut self, text: &str) {
        for ch in text.chars() {
            let ch = if matches!(ch, '\n' | '\r' | '\t') {
                ' '
            } else {
                ch
            };
            if crate::sanitize::is_unsafe(ch) || self.input.len() >= MAX_INPUT {
                continue;
            }
            self.input.insert(self.cur, ch);
            self.cur += 1;
        }
    }

    fn page_up(&mut self) -> Option<ChatAction> {
        let room = self.selected()?.clone();
        if self.is_remote(&room) {
            return None;
        }
        let (total, height) = self.metrics.get();
        let page = height.saturating_sub(1).max(1);
        let max = total.saturating_sub(height);
        let th = self.thread(&room);
        if th.scroll >= max {
            if th.more && th.loaded && !th.loading {
                let before = th.msgs.first().map(|m| m.id.clone())?;
                th.loading = true;
                return Some(ChatAction::Load {
                    room,
                    before: Some(before),
                });
            }
        } else {
            th.scroll = (th.scroll + page).min(max);
        }
        None
    }

    fn page_down(&mut self) {
        let Some(room) = self.selected().cloned() else {
            return;
        };
        let page = self.metrics.get().1.saturating_sub(1).max(1);
        let th = self.thread(&room);
        th.scroll = th.scroll.saturating_sub(page);
    }

    fn submit(&mut self) -> Option<ChatAction> {
        let room = self.selected()?.clone();
        let body = self.input_text().trim().to_string();
        if body.is_empty() {
            return None;
        }
        if let Some(n) = mention::parse_add(&body) {
            return self.submit_add(room, n);
        }
        if self.sending {
            self.say("Still sending the previous message", false);
            return None;
        }
        if body.starts_with('@') {
            if let Some(handled) = self.submit_mention(room.clone(), &body) {
                return handled;
            }
        }
        self.sending = true;
        self.say("Sending...", false);
        Some(ChatAction::Send {
            room,
            body,
            mention: None,
        })
    }

    fn key(&mut self, key: &[u8]) -> Option<ChatAction> {
        if self.phase != ChatPhase::Ready {
            return None;
        }
        if self.pending.is_some() {
            return self.confirm_key(key);
        }
        if self.menu_open() {
            match key {
                b"\x1b[A" | b"\x1bOA" => {
                    self.menu_move(false);
                    return None;
                }
                b"\x1b[B" | b"\x1bOB" => {
                    self.menu_move(true);
                    return None;
                }
                b"\t" | b"\r" | b"\n" => {
                    self.menu_accept();
                    return None;
                }
                _ => {}
            }
        }
        match key {
            b"\x1b[A" | b"\x1bOA" => return self.select(self.sel.saturating_sub(1)),
            b"\x1b[B" | b"\x1bOB" => return self.select(self.sel + 1),
            b"\x1b[5~" => return self.page_up(),
            b"\x1b[6~" => self.page_down(),
            b"\r" | b"\n" => return self.submit(),
            b"\x1b[D" | b"\x1bOD" => self.cur = self.cur.saturating_sub(1),
            b"\x1b[C" | b"\x1bOC" => self.cur = (self.cur + 1).min(self.input.len()),
            b"\x1b[H" | b"\x1bOH" | b"\x1b[1~" | b"\x01" => self.cur = 0,
            b"\x1b[F" | b"\x1bOF" | b"\x1b[4~" | b"\x05" => self.cur = self.input.len(),
            b"\x7f" | b"\x08" => {
                if self.cur > 0 {
                    self.cur -= 1;
                    self.input.remove(self.cur);
                }
            }
            b"\x1b[3~" | b"\x04" => {
                if self.cur < self.input.len() {
                    self.input.remove(self.cur);
                }
            }
            b"\x15" => {
                self.input.clear();
                self.cur = 0;
            }
            other if other.first() == Some(&0x1b) => {}
            other => {
                let text = String::from_utf8_lossy(other).into_owned();
                self.insert_text(&text);
            }
        }
        None
    }
}

impl Default for ChatState {
    fn default() -> Self {
        ChatState::new()
    }
}

fn split_chat_keys(data: &[u8]) -> Vec<Vec<u8>> {
    super::notes::split_keys(data)
}

impl ViewState {
    pub fn open_chat(&mut self, state: ChatState) {
        self.chat = Some(state);
        self.help = false;
        self.grab = None;
    }

    pub fn edit_chat(&mut self, data: &[u8]) -> Vec<ChatAction> {
        let now = self.chat.as_ref().map_or(0, |c| c.now_ms);
        self.edit_chat_at(data, now)
    }

    pub fn edit_chat_at(&mut self, data: &[u8], now_ms: u64) -> Vec<ChatAction> {
        if let Some(c) = self.chat.as_mut() {
            c.now_ms = c.now_ms.max(now_ms);
        }
        let mut out = Vec::new();
        for key in split_chat_keys(data) {
            let Some(chat) = self.chat.as_mut() else {
                break;
            };
            if key == b"\x1b" {
                self.chat = None;
                out.push(ChatAction::Close);
                break;
            }
            out.extend(chat.key(&key));
        }
        out
    }

    pub fn chat_poll(&mut self, now_ms: u64) -> Vec<ChatAction> {
        self.chat
            .as_mut()
            .and_then(|c| c.poll(now_ms))
            .into_iter()
            .collect()
    }

    pub fn insert_chat_text(&mut self, data: &[u8]) {
        let Some(chat) = self.chat.as_mut() else {
            return;
        };
        if chat.phase != ChatPhase::Ready {
            return;
        }
        let text = String::from_utf8_lossy(data)
            .replace("\x1b[200~", "")
            .replace("\x1b[201~", "");
        chat.insert_text(&text);
    }

    pub(crate) fn chat_mouse(&mut self, ev: MouseEvent, mut out: MouseOut) -> MouseOut {
        match ev.kind() {
            Kind::Press => {
                self.grab = Some(String::new());
                out.redraw = ev.is_left_press();
            }
            Kind::Release if self.grab.as_deref() == Some("") => self.grab = None,
            _ => {}
        }
        out
    }
}

struct Frame {
    panel: Rect,
    list: Option<Rect>,
    msgs: Rect,
    status: Rect,
    input: Rect,
}

fn frame(area: Rect) -> Option<Frame> {
    let panel = Rect::new(area.x, area.y, area.width, area.height.saturating_sub(1));
    if panel.width < 20 || panel.height < 7 {
        return None;
    }
    let inner = panel.inner(Margin::new(1, 1));
    let wide = inner.width >= MIN_WIDE;
    let list_w = if wide { LIST_W } else { 0 };
    let right_x = inner.x + if wide { list_w + 1 } else { 0 };
    let right_w = inner.right().saturating_sub(right_x);
    let msgs_h = inner.height.saturating_sub(2);
    Some(Frame {
        panel,
        list: wide.then(|| Rect::new(inner.x, inner.y, list_w, inner.height)),
        msgs: Rect::new(right_x, inner.y, right_w, msgs_h),
        status: Rect::new(right_x, inner.y + msgs_h, right_w, 1),
        input: Rect::new(right_x, inner.y + msgs_h + 1, right_w, 1),
    })
}

type Row = Vec<(String, Style)>;

fn author_style(m: &ChatMessage) -> Style {
    let c = colors();
    if system_like(m) {
        return dim();
    }
    match m.author.kind {
        AuthorKind::Owner => Style::new().fg(c.strong).add_modifier(Modifier::BOLD),
        _ if m.user_turn => Style::new().fg(c.strong).add_modifier(Modifier::BOLD),
        AuthorKind::Manager => Style::new().fg(c.accent).add_modifier(Modifier::BOLD),
        _ => Style::new().fg(c.waiting).add_modifier(Modifier::BOLD),
    }
}

pub(super) fn message_rows(chat: &ChatState, msgs: &[ChatMessage], width: usize) -> Vec<Row> {
    let mut rows = Vec::new();
    let latest = mention::latest_states(msgs);
    let questions: HashSet<&str> = msgs
        .iter()
        .filter(|m| mention::is_question(m))
        .map(|m| m.id.as_str())
        .collect();
    for m in msgs {
        if let Some(r) = m.mention.as_ref().filter(|_| !mention::is_question(m)) {
            if r.state != MentionState::Answered && questions.contains(r.id.as_str()) {
                continue;
            }
        }
        let sys = system_like(m);
        let label = if sys {
            "System".to_string()
        } else if m.user_turn {
            "You".to_string()
        } else {
            author_label(&m.author)
        };
        rows.push(vec![
            (label, author_style(m)),
            (format!("  {}", hhmm(m.ts_ms, chat.offset_secs)), dim()),
        ]);
        let body_style = if sys { dim() } else { Style::new() };
        let body = m.body_md.replace('\r', "").replace('\t', "    ");
        for line in wrap_words(&body, width) {
            rows.push(vec![(line, body_style)]);
        }
        if let Some(r) = m.mention.as_ref().filter(|_| mention::is_question(m)) {
            let state = latest.get(r.id.as_str()).copied().unwrap_or(r.state);
            rows.push(vec![(
                format!(
                    "\u{21b3} @{}: {}",
                    r.target_title,
                    mention::state_label(state)
                ),
                dim(),
            )]);
        }
        rows.push(Vec::new());
    }
    rows.pop();
    rows
}

pub(super) fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let width = width.max(4);
    let mut out = Vec::new();
    for line in text.split('\n') {
        let mut cur = String::new();
        let mut w = 0;
        let mut last_space: Option<usize> = None;
        for ch in line.chars() {
            let cw = ch.width().unwrap_or(0);
            if w + cw > width {
                if let Some(at) = last_space.filter(|_| ch != ' ') {
                    let rest = cur.split_off(at);
                    out.push(cur.trim_end().to_string());
                    cur = rest.trim_start().to_string();
                    w = cur.width();
                } else {
                    out.push(std::mem::take(&mut cur));
                    w = 0;
                }
                last_space = None;
                if ch == ' ' && w == 0 {
                    continue;
                }
            }
            if ch == ' ' {
                last_space = Some(cur.len());
            }
            cur.push(ch);
            w += cw;
        }
        out.push(cur);
    }
    out
}

fn draw_rows(buf: &mut Buffer, rect: Rect, rows: &[Row], start: usize) {
    for (i, row) in rows
        .iter()
        .skip(start)
        .take(usize::from(rect.height))
        .enumerate()
    {
        let y = rect.y + i as u16;
        let mut x = rect.x;
        for (text, style) in row {
            x = put(buf, x, y, text, *style, rect.right().saturating_sub(x));
        }
    }
}

fn room_meta(info: &RoomInfo) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !is_manager(info) {
        parts.extend(info.agent.clone());
    }
    if let Some(p) = info.context_pct {
        parts.push(format!("{p}%"));
    }
    parts.extend(info.state.clone());
    parts.join(" \u{b7} ")
}

enum ListItem<'a> {
    Header(&'a str),
    Room(usize),
    Hint,
}

fn list_items(chat: &ChatState) -> Vec<ListItem<'_>> {
    if !chat.grouped() {
        return (0..chat.rooms.len()).map(ListItem::Room).collect();
    }
    let mut items = Vec::new();
    let mut group: Option<&str> = None;
    let mut remote = false;
    for (i, e) in chat.rooms.iter().enumerate() {
        let m = e.info.room.machine.as_str();
        if group != Some(m) {
            if chat.pair_hint && remote {
                items.push(ListItem::Hint);
            }
            group = Some(m);
            remote = chat.is_remote(&e.info.room);
            items.push(ListItem::Header(if !chat.is_remote(&e.info.room) {
                LOCAL_GROUP
            } else if is_manager(&e.info) {
                e.info.title.as_str()
            } else {
                chat.labels.get(m).map_or(m, String::as_str)
            }));
        }
        items.push(ListItem::Room(i));
    }
    if chat.pair_hint && remote {
        items.push(ListItem::Hint);
    }
    items
}

fn draw_list(buf: &mut Buffer, rect: Rect, chat: &ChatState) {
    let items = list_items(chat);
    let hint_rows = wrap_words(PAIR_HINT, rect.width.saturating_sub(2).max(1).into()).len() as u16;
    let height = |it: &ListItem| match it {
        ListItem::Header(_) => 1u16,
        ListItem::Room(_) => 2,
        ListItem::Hint => hint_rows,
    };
    let sel_item = items
        .iter()
        .position(|it| matches!(it, ListItem::Room(i) if *i == chat.sel))
        .unwrap_or(0);
    let mut first = sel_item;
    let mut used = items.get(sel_item).map_or(0, height);
    while first > 0 && used + height(&items[first - 1]) <= rect.height {
        first -= 1;
        used += height(&items[first]);
    }
    let mut y = rect.y;
    for item in items.iter().skip(first) {
        if y + height(item) > rect.bottom() && y > rect.y {
            break;
        }
        match item {
            ListItem::Header(title) => {
                put(
                    buf,
                    rect.x,
                    y,
                    title,
                    dim().add_modifier(Modifier::BOLD),
                    rect.width,
                );
            }
            ListItem::Room(i) => draw_room(buf, rect, y, chat, *i),
            ListItem::Hint => {
                for (dy, line) in wrap_words(PAIR_HINT, rect.width.saturating_sub(2).max(1).into())
                    .iter()
                    .enumerate()
                {
                    put(buf, rect.x + 2, y + dy as u16, line, dim(), rect.width);
                }
            }
        }
        y += height(item);
    }
}

fn draw_room(buf: &mut Buffer, rect: Rect, y: u16, chat: &ChatState, i: usize) {
    let per = 2u16;
    let entry = &chat.rooms[i];
    let selected = i == chat.sel;
    let base = if selected {
        Style::new().add_modifier(Modifier::REVERSED)
    } else {
        Style::new()
    };
    let dot = if entry.unread { "\u{25cf} " } else { "  " };
    let dot_style = if entry.unread {
        base.fg(colors().accent)
    } else {
        base
    };
    for dy in 0..per {
        put(
            buf,
            rect.x,
            y + dy,
            &" ".repeat(rect.width.into()),
            base,
            rect.width,
        );
    }
    let x = put(buf, rect.x, y, dot, dot_style, rect.width);
    put(
        buf,
        x,
        y,
        room_label(&entry.info),
        base.add_modifier(Modifier::BOLD),
        rect.right().saturating_sub(x),
    );
    let meta_style = if selected { base } else { dim() };
    put(
        buf,
        rect.x + 2,
        y + 1,
        &room_meta(&entry.info),
        meta_style,
        rect.width.saturating_sub(2),
    );
}

fn draw_input(buf: &mut Buffer, rect: Rect, chat: &ChatState) {
    if rect.width < 3 {
        return;
    }
    put(
        buf,
        rect.x,
        rect.y,
        "> ",
        Style::new().fg(colors().accent),
        rect.width,
    );
    let avail = usize::from(rect.width - 2);
    let mut start = 0;
    while start < chat.cur
        && chat.input[start..chat.cur]
            .iter()
            .map(|c| c.width().unwrap_or(0))
            .sum::<usize>()
            >= avail.max(2)
    {
        start += 1;
    }
    let shown: String = chat.input[start..].iter().collect();
    put(
        buf,
        rect.x + 2,
        rect.y,
        &shown,
        Style::new(),
        rect.width - 2,
    );
    let before: String = chat.input[start..chat.cur].iter().collect();
    let x = rect.x + 2 + before.width() as u16;
    if x < rect.right() {
        let under = chat.input.get(chat.cur).copied().unwrap_or(' ').to_string();
        put(
            buf,
            x,
            rect.y,
            &under,
            Style::new().add_modifier(Modifier::REVERSED),
            rect.right() - x,
        );
    }
}

pub(crate) fn draw_chat(buf: &mut Buffer, area: Rect, st: &ViewState) {
    let Some(chat) = st.chat.as_ref() else {
        return;
    };
    let Some(f) = frame(area) else {
        return;
    };
    Clear.render(f.panel, buf);
    let title = match (&chat.phase, f.list, chat.rooms.get(chat.sel)) {
        (ChatPhase::Ready, None, Some(r)) => format!(" Chat \u{b7} {} ", room_label(&r.info)),
        _ => " Chat ".to_string(),
    };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .title(title)
        .title_bottom(" Esc close \u{b7} Up/Down room \u{b7} PgUp older \u{b7} Enter send ")
        .render(f.panel, buf);
    let inner = f.panel.inner(Margin::new(2, 1));
    match &chat.phase {
        ChatPhase::NoCap => {
            put(
                buf,
                inner.x,
                inner.y,
                NEEDS_NEWER,
                Style::new(),
                inner.width,
            );
        }
        ChatPhase::Loading => {
            put(
                buf,
                inner.x,
                inner.y,
                "Loading rooms...",
                dim(),
                inner.width,
            );
        }
        ChatPhase::Failed(e) => {
            let text = format!("Cannot load chat: {e}");
            put(
                buf,
                inner.x,
                inner.y,
                &text,
                Style::new().fg(colors().warn),
                inner.width,
            );
        }
        ChatPhase::Ready => draw_ready(buf, &f, chat),
    }
}

fn draw_ready(buf: &mut Buffer, f: &Frame, chat: &ChatState) {
    if let Some(list) = f.list {
        draw_list(buf, list, chat);
        for y in list.y..list.bottom() {
            put(buf, list.right(), y, "\u{2502}", dim(), 1);
        }
    }
    let Some(room) = chat.selected() else {
        put(buf, f.msgs.x, f.msgs.y, "No rooms yet", dim(), f.msgs.width);
        return;
    };
    let th = chat.threads.get(room);
    let msgs = th.map_or(&[][..], |t| t.msgs.as_slice());
    if msgs.is_empty() {
        let text = if th.is_some_and(|t| t.loading) || !th.is_some_and(|t| t.loaded) {
            "Loading..."
        } else {
            "No messages yet"
        };
        put(buf, f.msgs.x, f.msgs.y, text, dim(), f.msgs.width);
    } else {
        let rows = message_rows(chat, msgs, usize::from(f.msgs.width));
        let height = usize::from(f.msgs.height);
        chat.metrics.set((rows.len(), height));
        let scroll = th
            .map_or(0, |t| t.scroll)
            .min(rows.len().saturating_sub(height));
        let end = rows.len() - scroll;
        draw_rows(buf, f.msgs, &rows, end.saturating_sub(height));
    }
    let style = if chat.status_bad {
        Style::new().fg(colors().warn)
    } else {
        dim()
    };
    draw_menu(buf, f.msgs, chat);
    put(
        buf,
        f.status.x,
        f.status.y,
        &chat.status,
        style,
        f.status.width,
    );
    draw_input(buf, f.input, chat);
}

fn draw_menu(buf: &mut Buffer, area: Rect, chat: &ChatState) {
    let items = chat.menu_items();
    let n = items.len().min(usize::from(area.height));
    if n == 0 {
        return;
    }
    let sel = chat.menu_sel.min(items.len() - 1);
    for (i, idx) in items.iter().take(n).enumerate() {
        let y = area.bottom() - (n - i) as u16;
        let style = if i == sel {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        put(
            buf,
            area.x,
            y,
            &" ".repeat(area.width.into()),
            style,
            area.width,
        );
        put(
            buf,
            area.x,
            y,
            &format!(" @{}", chat.menu_line(*idx)),
            style,
            area.width,
        );
    }
}
