use super::{is_manager, room_label, ChatAction, ChatMessage, ChatState, RoomEntry, RoomRef};
use std::collections::HashMap;
use tmgr_ipc::chat::{MentionReq, MentionState};

pub const CONFIRM_WINDOW_MS: u64 = 30_000;
pub const TOO_OLD: &str = "Runner too old for mentions";
pub const AGENT_REFUSAL: &str = "Refused: TMGR_SESSION_ID is set, run this from your own terminal";
const MENU_ROWS: usize = 5;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Confirm {
    Send {
        room: RoomRef,
        target: RoomRef,
        title: String,
        body: String,
    },
    Add {
        room: RoomRef,
        mention_id: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Pending {
    pub what: Confirm,
    pub at_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Resolved {
    Target { idx: usize, question: String },
    NoQuestion,
    Ambiguous(Vec<usize>),
    Refused { title: String, reason: String },
    Unknown(String),
}

fn at_token_end(after: &str, len: usize) -> bool {
    after[len..].chars().next().is_none_or(char::is_whitespace)
}

fn name_matches(after: &str, name: &str, exact: bool) -> bool {
    if name.is_empty() {
        return false;
    }
    let Some(pre) = after.get(..name.len()) else {
        return false;
    };
    let same = if exact {
        pre == name
    } else {
        pre.to_lowercase() == name.to_lowercase()
    };
    same && at_token_end(after, name.len())
}

pub(super) fn resolve(rooms: &[RoomEntry], local: Option<&str>, body: &str) -> Option<Resolved> {
    let after = body.strip_prefix('@')?;
    let locals: Vec<(usize, &RoomEntry)> = rooms
        .iter()
        .enumerate()
        .filter(|(_, e)| local.is_none_or(|l| e.info.room.machine == l))
        .collect();
    let by_id = locals.iter().find(|(_, e)| {
        e.info
            .room
            .session
            .as_deref()
            .is_some_and(|s| name_matches(after, s, true))
    });
    let (idx, len) = if let Some((i, e)) = by_id {
        (*i, e.info.room.session.as_deref().map_or(0, str::len))
    } else {
        let hits: Vec<(usize, usize)> = locals
            .iter()
            .filter(|(_, e)| name_matches(after, room_label(&e.info), false))
            .map(|(i, e)| (*i, room_label(&e.info).len()))
            .collect();
        let Some(longest) = hits.iter().map(|h| h.1).max() else {
            let word = after.split_whitespace().next().unwrap_or("").to_string();
            return Some(Resolved::Unknown(word));
        };
        let best: Vec<(usize, usize)> = hits.into_iter().filter(|h| h.1 == longest).collect();
        if best.len() > 1 {
            return Some(Resolved::Ambiguous(best.iter().map(|h| h.0).collect()));
        }
        best[0]
    };
    let info = &rooms[idx].info;
    let verdict = info.mention.as_ref();
    if !verdict.is_some_and(|m| m.ok) {
        let reason = verdict
            .and_then(|m| m.reason.clone())
            .unwrap_or_else(|| "not available".into());
        return Some(Resolved::Refused {
            title: room_label(info).to_string(),
            reason,
        });
    }
    let question = after[len..].trim().to_string();
    if question.is_empty() {
        return Some(Resolved::NoQuestion);
    }
    Some(Resolved::Target { idx, question })
}

pub(super) fn latest_states(msgs: &[ChatMessage]) -> HashMap<&str, MentionState> {
    let mut out = HashMap::new();
    for m in msgs {
        if let Some(r) = &m.mention {
            out.insert(r.id.as_str(), r.state);
        }
    }
    out
}

pub(super) fn state_label(s: MentionState) -> &'static str {
    match s {
        MentionState::Queued => "queued",
        MentionState::Sent => "sent",
        MentionState::Answering => "answering",
        MentionState::Answered => "answered",
        MentionState::Busy => "busy",
        MentionState::Offline => "offline",
        MentionState::Timeout => "timeout",
        MentionState::Interrupted => "interrupted",
        MentionState::Expired => "expired",
        MentionState::Refused => "refused",
        MentionState::RateLimited => "rate limited",
        MentionState::Cancelled => "cancelled",
        MentionState::Other => "updated",
    }
}

pub(super) fn is_question(m: &ChatMessage) -> bool {
    m.mention.as_ref().is_some_and(|r| r.id == m.id)
}

pub(super) fn answered_ids(msgs: &[ChatMessage]) -> Vec<String> {
    let latest = latest_states(msgs);
    let mut out: Vec<String> = Vec::new();
    for m in msgs {
        let Some(r) = &m.mention else { continue };
        if latest.get(r.id.as_str()) == Some(&MentionState::Answered)
            && r.state == MentionState::Answered
            && !out.contains(&r.id)
        {
            out.push(r.id.clone());
        }
    }
    out.reverse();
    out
}

pub(super) fn parse_add(body: &str) -> Option<Result<usize, ()>> {
    let rest = body.strip_prefix(":add")?;
    if rest.is_empty() {
        return Some(Ok(1));
    }
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    Some(match rest.trim().parse::<usize>() {
        Ok(n) if n >= 1 => Ok(n),
        _ => Err(()),
    })
}

impl ChatState {
    pub fn with_mention_cap(mut self, cap: bool) -> ChatState {
        self.mention_cap = cap;
        self
    }

    pub fn with_agent_session(mut self, inside_agent: bool) -> ChatState {
        self.agent_session = inside_agent;
        self
    }

    pub fn awaiting_confirm(&self) -> bool {
        self.pending.is_some()
    }

    fn room_title(&self, room: &RoomRef) -> String {
        self.rooms
            .iter()
            .find(|e| &e.info.room == room)
            .map_or_else(
                || "This session".into(),
                |e| room_label(&e.info).to_string(),
            )
    }

    fn local_room(&self, room: &RoomRef) -> bool {
        !self.is_remote(room)
    }

    fn origin_risky(&self, room: &RoomRef) -> bool {
        self.rooms.iter().any(|e| {
            &e.info.room == room
                && (is_manager(&e.info)
                    || e.info.unrestricted
                    || e.info.mention.as_ref().is_some_and(|m| m.risky))
        })
    }

    fn dup_title(&self, idx: usize) -> bool {
        let label = room_label(&self.rooms[idx].info).to_lowercase();
        self.rooms.iter().enumerate().any(|(i, e)| {
            i != idx && self.local_room(&e.info.room) && room_label(&e.info).to_lowercase() == label
        })
    }

    pub(super) fn menu_items(&self) -> Vec<usize> {
        if self.phase != super::ChatPhase::Ready || !self.mention_cap {
            return Vec::new();
        }
        let Some(first) = self.input.first() else {
            return Vec::new();
        };
        if *first != '@' || self.cur == 0 {
            return Vec::new();
        }
        let typed: String = self.input[1..].iter().collect::<String>().to_lowercase();
        self.rooms
            .iter()
            .enumerate()
            .filter(|(_, e)| {
                self.local_room(&e.info.room)
                    && e.info.mention.as_ref().is_some_and(|m| m.ok)
                    && room_label(&e.info).to_lowercase().starts_with(&typed)
            })
            .map(|(i, _)| i)
            .take(MENU_ROWS)
            .collect()
    }

    pub(super) fn menu_line(&self, idx: usize) -> String {
        let info = &self.rooms[idx].info;
        let mut line = room_label(info).to_string();
        let mut extra: Vec<String> = Vec::new();
        if !is_manager(info) {
            extra.extend(info.agent.clone());
        }
        if self.dup_title(idx) {
            extra.extend(info.room.session.clone());
        }
        if info.mention.as_ref().is_some_and(|m| m.risky) {
            extra.push("risky".into());
        }
        if !extra.is_empty() {
            line = format!("{line}  {}", extra.join(" \u{b7} "));
        }
        line
    }

    pub(super) fn menu_open(&self) -> bool {
        !self.menu_items().is_empty()
    }

    pub(super) fn menu_move(&mut self, down: bool) {
        let n = self.menu_items().len();
        if n == 0 {
            return;
        }
        let cur = self.menu_sel.min(n - 1);
        self.menu_sel = if down {
            (cur + 1).min(n - 1)
        } else {
            cur.saturating_sub(1)
        };
    }

    pub(super) fn menu_accept(&mut self) {
        let items = self.menu_items();
        let Some(&idx) = items.get(self.menu_sel.min(items.len().saturating_sub(1))) else {
            return;
        };
        let insert = if self.dup_title(idx) {
            self.rooms[idx]
                .info
                .room
                .session
                .clone()
                .unwrap_or_else(|| room_label(&self.rooms[idx].info).to_string())
        } else {
            room_label(&self.rooms[idx].info).to_string()
        };
        let tail: Vec<char> = format!("@{insert} ").chars().collect();
        self.cur = tail.len();
        self.input = tail;
        self.menu_sel = 0;
    }

    pub(super) fn submit_mention(
        &mut self,
        room: RoomRef,
        body: &str,
    ) -> Option<Option<ChatAction>> {
        let resolved = resolve(&self.rooms, self.local.as_deref(), body)?;
        if matches!(resolved, Resolved::Unknown(_)) {
            return None;
        }
        if !self.mention_cap {
            self.say(TOO_OLD, true);
            return Some(None);
        }
        if !self.local_room(&room) {
            self.say("Mentions work only in rooms on this machine", true);
            return Some(None);
        }
        Some(match resolved {
            Resolved::Unknown(_) => None,
            Resolved::NoQuestion => {
                self.say("Write a question after the name", true);
                None
            }
            Resolved::Refused { title, reason } => {
                self.say(&format!("Cannot ask @{title}: {reason}"), true);
                None
            }
            Resolved::Ambiguous(idx) => {
                let list: Vec<String> = idx
                    .iter()
                    .map(|&i| {
                        let info = &self.rooms[i].info;
                        format!(
                            "{} ({})",
                            info.room.session.as_deref().unwrap_or("manager"),
                            info.agent.as_deref().unwrap_or("agent")
                        )
                    })
                    .collect();
                self.say(
                    &format!(
                        "Several sessions match, use @<session id>: {}",
                        list.join(", ")
                    ),
                    true,
                );
                None
            }
            Resolved::Target { idx, question } => {
                let info = self.rooms[idx].info.clone();
                let risky = info.mention.as_ref().is_some_and(|m| m.risky);
                if risky && self.agent_session {
                    self.say(AGENT_REFUSAL, true);
                    return Some(None);
                }
                let title = room_label(&info).to_string();
                if risky {
                    self.pending = Some(Pending {
                        what: Confirm::Send {
                            room,
                            target: info.room,
                            title: title.clone(),
                            body: question,
                        },
                        at_ms: self.now_ms,
                    });
                    self.say(
                        &format!("@{title} runs in bypass or without a sandbox. Press y within 30 s to send, any other key cancels"),
                        true,
                    );
                    return Some(None);
                }
                Some(self.start_send(room, question, info.room, false))
            }
        })
    }

    fn start_send(
        &mut self,
        room: RoomRef,
        body: String,
        target: RoomRef,
        confirmed: bool,
    ) -> ChatAction {
        self.sending = true;
        self.say("Sending...", false);
        ChatAction::Send {
            room,
            body,
            mention: Some(MentionReq {
                target,
                confirmed_risky: confirmed,
            }),
        }
    }

    pub(super) fn submit_add(&mut self, room: RoomRef, n: Result<usize, ()>) -> Option<ChatAction> {
        if self.agent_session {
            self.say(AGENT_REFUSAL, true);
            return None;
        }
        if !self.mention_cap {
            self.say(TOO_OLD, true);
            return None;
        }
        if !self.local_room(&room) {
            self.say("Add works only in rooms on this machine", true);
            return None;
        }
        let Ok(n) = n else {
            self.say("Usage: :add or :add <n>", true);
            return None;
        };
        let ids = answered_ids(self.messages(&room));
        let Some(id) = ids.get(n - 1).cloned() else {
            let text = if ids.is_empty() {
                "No answered mention in this room".to_string()
            } else {
                format!("Only {} answered mention(s) in this room", ids.len())
            };
            self.say(&text, true);
            return None;
        };
        if self.origin_risky(&room) {
            let title = self.room_title(&room);
            self.pending = Some(Pending {
                what: Confirm::Add {
                    room,
                    mention_id: id,
                },
                at_ms: self.now_ms,
            });
            self.say(
                &format!("{title} is a manager or runs in bypass or without a sandbox. Press y within 30 s to add the reply, any other key cancels"),
                true,
            );
            return None;
        }
        self.input.clear();
        self.cur = 0;
        self.say("Adding the reply...", false);
        Some(ChatAction::Inject {
            room,
            mention_id: id,
            confirmed_risky: false,
        })
    }

    pub(super) fn confirm_key(&mut self, key: &[u8]) -> Option<ChatAction> {
        let p = self.pending.take()?;
        if !matches!(key, b"y" | b"Y") {
            self.say("Cancelled", false);
            return None;
        }
        if self.now_ms.saturating_sub(p.at_ms) > CONFIRM_WINDOW_MS {
            self.say("Confirmation expired", true);
            return None;
        }
        let origin = match &p.what {
            Confirm::Send { room, .. } | Confirm::Add { room, .. } => room,
        };
        if self.selected() != Some(origin) {
            self.say("Confirmation dropped: the room changed", true);
            return None;
        }
        match p.what {
            Confirm::Send {
                room, target, body, ..
            } => Some(self.start_send(room, body, target, true)),
            Confirm::Add { room, mention_id } => {
                self.input.clear();
                self.cur = 0;
                self.say("Adding the reply...", false);
                Some(ChatAction::Inject {
                    room,
                    mention_id,
                    confirmed_risky: true,
                })
            }
        }
    }

    pub(super) fn expire_pending(&mut self) {
        if self
            .pending
            .as_ref()
            .is_some_and(|p| self.now_ms.saturating_sub(p.at_ms) > CONFIRM_WINDOW_MS)
        {
            self.pending = None;
            self.say("Confirmation expired", true);
        }
    }

    pub fn injected(&mut self, room: &RoomRef, mention_id: &str, result: Result<(), String>) {
        match result {
            Ok(()) => self.say("Added the reply to the session context", false),
            Err(e) if e.contains("needs confirmation") && !self.agent_session => {
                if self.selected() != Some(room) {
                    self.say(
                        "A session needs confirmation, run :add again in its room",
                        true,
                    );
                    return;
                }
                let title = self.room_title(room);
                self.pending = Some(Pending {
                    what: Confirm::Add {
                        room: room.clone(),
                        mention_id: mention_id.to_string(),
                    },
                    at_ms: self.now_ms,
                });
                self.say(
                    &format!("{title} needs confirmation. Press y within 30 s to add the reply, any other key cancels"),
                    true,
                );
            }
            Err(e) => self.say(&e, true),
        }
    }
}
