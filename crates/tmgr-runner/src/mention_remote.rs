use crate::manager_remote::{plain, ready, remote_result, Link};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use tmgr_ipc::chat::{AuthorKind, ChatMessage, RoomKind, RoomRef};
use tmgr_ipc::manager::limits as rpc_limits;
use tmgr_ipc::mgr_rpc::{ErrorCode, Op};

pub const POLL_EVERY_MS: u64 = 10_000;
pub const BACKOFF_MAX_MS: u64 = 60_000;
pub const QUIET_MS: u64 = 20_000;
pub const BUDGET_PER_MIN: usize = 10;
const WINDOW_MS: u64 = 60_000;
const REMOTE_AUTHOR: &str = "remote";

pub const NOT_LOCAL: &str = "phone mentions reach only this machine";
pub const OLD_CLI: &str = "the follower runner needs a newer CLI (TM-1065 follower asks)";

pub fn target_ok(link: &dyn Link, room: &RoomRef) -> Result<(), String> {
    if !link.top() {
        return Err("mentions reach only sessions on this machine".into());
    }
    if room.kind != RoomKind::Manager {
        return Err("session rooms of other machines are not available".into());
    }
    ready(link, &room.machine)
}

pub fn ask(link: &dyn Link, machine: &str, question: &str) -> Result<String, String> {
    let res = link
        .call(machine, Op::Ask, json!({"text": question}))
        .map_err(|e| plain(&e))?;
    let answer = remote_result(machine, Op::Ask, res)?;
    if answer.get("queued") != Some(&Value::Bool(true)) {
        return Err(OLD_CLI.into());
    }
    answer
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| tmgr_ipc::chat::valid_message_id(id))
        .map(String::from)
        .ok_or_else(|| "the remote machine sent an unreadable answer".into())
}

pub enum Fetch {
    Rows(Vec<ChatMessage>),
    Limited,
    Failed,
}

pub fn fetch(link: &dyn Link, machine: &str) -> Fetch {
    let args = json!({"limit": rpc_limits::MAX_ROOM_LIMIT});
    let Ok(res) = link.call(machine, Op::Room, args) else {
        return Fetch::Failed;
    };
    if res
        .error
        .as_ref()
        .is_some_and(|e| e.code == ErrorCode::RateLimited)
    {
        return Fetch::Limited;
    }
    match remote_result(machine, Op::Room, res) {
        Ok(Value::Array(rows)) => Fetch::Rows(
            rows.into_iter()
                .filter_map(|v| serde_json::from_value::<ChatMessage>(v).ok())
                .collect(),
        ),
        _ => Fetch::Failed,
    }
}

#[derive(Debug, Clone)]
pub struct Track {
    pub mention: String,
    pub asked: String,
    replies: Vec<(String, String)>,
    last_change: u64,
    polled: u64,
    found: bool,
    gone: bool,
}

impl Track {
    pub fn new(mention: &str, asked: &str, now: u64) -> Track {
        Track {
            mention: mention.into(),
            asked: asked.into(),
            replies: Vec::new(),
            last_change: now,
            polled: now,
            found: false,
            gone: false,
        }
    }
}

pub fn fold(t: &mut Track, rows: &[ChatMessage], now: u64) {
    let Some(at) = rows.iter().position(|m| m.id == t.asked) else {
        t.polled = now;
        t.gone = t.found;
        return;
    };
    t.polled = now;
    t.found = true;
    t.gone = false;
    for m in &rows[at + 1..] {
        let reply = m.author.kind == AuthorKind::Session && m.author.id != REMOTE_AUTHOR;
        if reply && !m.body_md.trim().is_empty() && !t.replies.iter().any(|(id, _)| *id == m.id) {
            t.replies.push((m.id.clone(), m.body_md.clone()));
            t.last_change = now;
        }
    }
}

pub fn quiet_reply(t: &Track, now: u64) -> Option<String> {
    if t.replies.is_empty()
        || (!t.gone && now.min(t.polled).saturating_sub(t.last_change) < QUIET_MS)
    {
        return None;
    }
    let joined: Vec<&str> = t.replies.iter().map(|(_, b)| b.as_str()).collect();
    Some(crate::chat::mention::clip_reply(&joined.join("\n\n")))
}

#[derive(Debug, Default)]
struct Pace {
    next_at: u64,
    interval: u64,
    calls: VecDeque<u64>,
}

impl Pace {
    fn admit(&mut self, now: u64) -> bool {
        self.calls.retain(|at| now.saturating_sub(*at) < WINDOW_MS);
        if now < self.next_at || self.calls.len() >= BUDGET_PER_MIN {
            return false;
        }
        self.calls.push_back(now);
        self.next_at = now + self.interval.max(POLL_EVERY_MS);
        true
    }

    fn limited(&mut self, now: u64) {
        self.interval = (self.interval.max(POLL_EVERY_MS) * 2).min(BACKOFF_MAX_MS);
        self.next_at = now + self.interval;
    }

    fn ok(&mut self) {
        self.interval = POLL_EVERY_MS;
    }
}

#[derive(Debug, Default)]
pub struct Followers {
    tracks: HashMap<String, Track>,
    pace: HashMap<String, Pace>,
}

impl Followers {
    pub fn track(&mut self, machine: &str, t: Track) {
        self.tracks.insert(machine.into(), t);
    }

    pub fn admit(&mut self, machine: &str, now: u64) -> bool {
        self.pace.entry(machine.into()).or_default().admit(now)
    }

    pub fn keep_open(&mut self, is_open: impl Fn(&str) -> bool) {
        self.tracks.retain(|_, t| is_open(&t.mention));
    }

    pub fn due(&mut self, now: u64) -> Vec<String> {
        let mut out = Vec::new();
        for machine in self.tracks.keys() {
            if self.pace.entry(machine.clone()).or_default().admit(now) {
                out.push(machine.clone());
            }
        }
        out
    }

    pub fn fed(&mut self, machine: &str, fetched: Fetch, now: u64) {
        let pace = self.pace.entry(machine.into()).or_default();
        match fetched {
            Fetch::Limited => pace.limited(now),
            Fetch::Failed => {}
            Fetch::Rows(rows) => {
                pace.ok();
                if let Some(t) = self.tracks.get_mut(machine) {
                    fold(t, &rows, now);
                }
            }
        }
    }

    pub fn finished(&mut self, now: u64) -> Vec<(String, String)> {
        let done: Vec<String> = self
            .tracks
            .iter()
            .filter(|(_, t)| quiet_reply(t, now).is_some())
            .map(|(m, _)| m.clone())
            .collect();
        done.into_iter()
            .filter_map(|m| self.tracks.remove(&m))
            .filter_map(|t| quiet_reply(&t, now).map(|r| (t.mention, r)))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmgr_ipc::chat::{Author, MessageKind, CHAT_V};

    const M: &str = "mx_00000000000000bb";

    fn row(n: u32, kind: AuthorKind, author: &str, body: &str) -> ChatMessage {
        ChatMessage {
            v: CHAT_V,
            id: format!("m_{n:032x}"),
            room: RoomRef::manager(M),
            author: Author {
                kind,
                id: author.into(),
                machine: None,
                device: None,
                agent: None,
            },
            ts_ms: 1,
            kind: MessageKind::Text,
            body_md: body.into(),
            attachments: Vec::new(),
            reply_to: None,
            decision: None,
            session_meta: None,
            user_turn: false,
            mention: None,
        }
    }

    fn asked() -> String {
        format!("m_{:032x}", 1)
    }

    #[test]
    fn reply_assembly_keeps_session_rows_and_drops_remote_and_system_rows() {
        let mut t = Track::new("m_x", &asked(), 0);
        let rows = [
            row(1, AuthorKind::Manager, "remote", "the question"),
            row(2, AuthorKind::Session, "s_aaaaaaaaaa", "first"),
            row(3, AuthorKind::System, "system", "notice"),
            row(4, AuthorKind::Session, "remote", "forged"),
            row(5, AuthorKind::Session, "s_aaaaaaaaaa", "second"),
        ];
        fold(&mut t, &rows, 1_000);
        fold(&mut t, &rows, 1_000 + QUIET_MS);
        assert_eq!(
            quiet_reply(&t, 1_000 + QUIET_MS).unwrap(),
            "first\n\nsecond"
        );
    }

    #[test]
    fn rows_before_the_asked_id_are_ignored() {
        let mut t = Track::new("m_x", &asked(), 0);
        let rows = [
            row(0, AuthorKind::Session, "s_aaaaaaaaaa", "old"),
            row(1, AuthorKind::Manager, "remote", "q"),
            row(2, AuthorKind::Session, "s_aaaaaaaaaa", "new"),
        ];
        fold(&mut t, &rows, 5);
        fold(&mut t, &rows, 5 + QUIET_MS);
        assert_eq!(quiet_reply(&t, 5 + QUIET_MS).unwrap(), "new");
    }

    #[test]
    fn rows_without_the_asked_id_give_no_answer() {
        let mut t = Track::new("m_x", &asked(), 0);
        let rows = [
            row(7, AuthorKind::Session, "s_aaaaaaaaaa", "newest"),
            row(8, AuthorKind::Session, "s_aaaaaaaaaa", "newer"),
        ];
        fold(&mut t, &rows, 1_000);
        assert!(quiet_reply(&t, 1_000_000).is_none());
    }

    #[test]
    fn an_asked_row_that_scrolled_out_finishes_with_the_text_collected_so_far() {
        let mut t = Track::new("m_x", &asked(), 0);
        let a = row(2, AuthorKind::Session, "s_aaaaaaaaaa", "kept");
        fold(
            &mut t,
            &[row(1, AuthorKind::Manager, "remote", "q"), a],
            1_000,
        );
        assert!(quiet_reply(&t, 1_001).is_none());
        fold(
            &mut t,
            &[row(9, AuthorKind::Session, "s_aaaaaaaaaa", "unrelated")],
            2_000,
        );
        assert_eq!(quiet_reply(&t, 2_000).unwrap(), "kept");
    }

    #[test]
    fn a_scrolled_out_asked_row_with_no_text_keeps_waiting() {
        let mut t = Track::new("m_x", &asked(), 0);
        fold(&mut t, &[row(1, AuthorKind::Manager, "remote", "q")], 1_000);
        fold(
            &mut t,
            &[row(9, AuthorKind::Session, "s_aaaaaaaaaa", "x")],
            2_000,
        );
        assert!(quiet_reply(&t, 1_000_000).is_none());
    }

    #[test]
    fn completion_waits_for_a_quiet_period_after_the_last_new_row() {
        let mut t = Track::new("m_x", &asked(), 0);
        assert!(quiet_reply(&t, 1_000_000).is_none());
        let q = row(1, AuthorKind::Manager, "remote", "q");
        let a = row(2, AuthorKind::Session, "s_aaaaaaaaaa", "a");
        fold(&mut t, &[q.clone(), a.clone()], 1_000);
        assert!(quiet_reply(&t, 1_000 + QUIET_MS - 1).is_none());
        let b = row(3, AuthorKind::Session, "s_aaaaaaaaaa", "b");
        fold(&mut t, &[q.clone(), a.clone(), b], 15_000);
        assert!(quiet_reply(&t, 1_000 + QUIET_MS).is_none());
        fold(&mut t, &[q.clone(), a.clone()], 30_000);
        assert!(quiet_reply(&t, 15_000 + QUIET_MS).is_none());
        fold(&mut t, &[q, a], 15_000 + QUIET_MS);
        assert!(quiet_reply(&t, 15_000 + QUIET_MS).is_some());
    }

    #[test]
    fn the_budget_allows_ten_calls_a_minute_and_the_interval_spaces_them() {
        let mut f = Followers::default();
        f.track(M, Track::new("m_x", &asked(), 0));
        assert_eq!(f.due(0).len(), 1);
        assert!(f.due(POLL_EVERY_MS - 1).is_empty());
        assert_eq!(f.due(POLL_EVERY_MS).len(), 1);
        let p = f.pace.get_mut(M).unwrap();
        p.next_at = 0;
        p.calls = (0..BUDGET_PER_MIN as u64).map(|i| 50_000 + i).collect();
        assert!(f.due(59_999).is_empty());
        assert_eq!(f.due(110_001).len(), 1);
    }

    #[test]
    fn rate_limited_doubles_the_interval_up_to_a_minute() {
        let mut f = Followers::default();
        f.track(M, Track::new("m_x", &asked(), 0));
        let mut now = 0;
        let mut seen = Vec::new();
        for _ in 0..4 {
            assert_eq!(f.due(now).len(), 1);
            f.fed(M, Fetch::Limited, now);
            let next = f.pace[M].next_at;
            seen.push(next - now);
            now = next;
        }
        assert_eq!(seen, [20_000, 40_000, 60_000, 60_000]);
        assert_eq!(f.due(now).len(), 1);
        f.fed(M, Fetch::Rows(Vec::new()), now);
        assert_eq!(f.pace[M].interval, POLL_EVERY_MS);
    }

    #[test]
    fn a_reply_finishes_only_after_a_successful_poll_saw_the_quiet_period() {
        let mut f = Followers::default();
        f.track(M, Track::new("m_x", &asked(), 0));
        let rows = vec![
            row(1, AuthorKind::Manager, "remote", "q"),
            row(2, AuthorKind::Session, "s_aaaaaaaaaa", "partial"),
        ];
        f.fed(M, Fetch::Rows(rows.clone()), 100);
        f.fed(M, Fetch::Failed, 100 + QUIET_MS);
        assert!(f.finished(100 + 5 * QUIET_MS).is_empty());
        f.fed(M, Fetch::Rows(rows), 100 + 6 * QUIET_MS);
        assert_eq!(
            f.finished(100 + 6 * QUIET_MS),
            [("m_x".to_string(), "partial".to_string())]
        );
    }

    #[test]
    fn an_ask_is_charged_to_the_pace() {
        let mut f = Followers::default();
        assert!(f.admit(M, 0));
        f.track(M, Track::new("m_x", &asked(), 0));
        assert!(f.due(1).is_empty());
        assert_eq!(f.due(POLL_EVERY_MS).len(), 1);
        let p = f.pace.get_mut(M).unwrap();
        p.next_at = 0;
        p.calls = (0..BUDGET_PER_MIN as u64).map(|i| 50_000 + i).collect();
        assert!(!f.admit(M, 59_999));
    }

    #[test]
    fn finished_returns_each_quiet_mention_once() {
        let mut f = Followers::default();
        f.track(M, Track::new("m_x", &asked(), 0));
        let rows = vec![
            row(1, AuthorKind::Manager, "remote", "q"),
            row(2, AuthorKind::Session, "s_aaaaaaaaaa", "hi"),
        ];
        f.fed(M, Fetch::Rows(rows.clone()), 100);
        f.fed(M, Fetch::Rows(rows), 100 + QUIET_MS);
        assert!(f.finished(100 + QUIET_MS - 1).is_empty());
        assert_eq!(
            f.finished(100 + QUIET_MS),
            [("m_x".to_string(), "hi".to_string())]
        );
        assert!(f.finished(100 + QUIET_MS).is_empty());
    }
}
