use crate::wire::write_atomic;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use tmgr_ipc::api::TaskRef;
use tmgr_ipc::dirs::ensure_private_dir;

const CHUNK_MS: u64 = 60_000;
const TYPING_HOLD_MS: u64 = 10_000;
const SLEEP_SLACK_MS: u64 = 5_000;
const KEEP_S: u64 = 45 * 24 * 3600;
const MAX_FILE: u64 = 64 << 20;
const MAX_PENDING: usize = 10_000;
pub const IDLE_MIN_S: u64 = 30;
pub const IDLE_MAX_S: u64 = 3600;
pub const IDLE_DEFAULT_S: u64 = 120;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub session: String,
    pub name: String,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskRef>,
    pub start_s: u64,
    pub end_s: u64,
    pub typing_s: u64,
    pub reading_s: u64,
}

pub type Closed = Record;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SentRecord {
    pub id: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub counted_s: Option<u64>,
    pub at_s: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub sync: bool,
    pub idle_s: u64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            sync: true,
            idle_s: IDLE_DEFAULT_S,
        }
    }
}

impl Settings {
    pub fn load(runner_dir: &Path) -> Settings {
        let mut s: Settings = read_file(&time_dir(runner_dir).join("settings.json"))
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        s.idle_s = s.idle_s.clamp(IDLE_MIN_S, IDLE_MAX_S);
        s
    }

    pub fn save(&self, runner_dir: &Path) -> std::io::Result<()> {
        let dir = time_dir(runner_dir);
        ensure_private_dir(&dir)?;
        let clamped = Settings {
            idle_s: self.idle_s.clamp(IDLE_MIN_S, IDLE_MAX_S),
            ..*self
        };
        let bytes = serde_json::to_vec(&clamped).map_err(std::io::Error::other)?;
        write_atomic(&dir.join("settings.json"), &bytes)
    }
}

pub fn time_dir(runner_dir: &Path) -> PathBuf {
    tmgr_ipc::dirs::time_dir(runner_dir)
}

fn read_file(path: &Path) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .and_then(|f| f.take(MAX_FILE).read_to_end(&mut buf))
        .ok()?;
    Some(buf)
}

fn read_lines<T: for<'a> Deserialize<'a>>(path: &Path) -> Vec<T> {
    let Some(buf) = read_file(path) else {
        return Vec::new();
    };
    buf.split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .filter_map(|l| serde_json::from_slice(l).ok())
        .collect()
}

fn append_lines<T: Serialize>(runner_dir: &Path, name: &str, items: &[T]) -> std::io::Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    let dir = time_dir(runner_dir);
    ensure_private_dir(&dir)?;
    let mut out = Vec::new();
    for i in items {
        serde_json::to_writer(&mut out, i).map_err(std::io::Error::other)?;
        out.push(b'\n');
    }
    let mut f = OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(dir.join(name))?;
    let len = f.metadata()?.len();
    if len > 0 {
        let mut last = [0u8; 1];
        f.read_exact_at(&mut last, len - 1)?;
        if last[0] != b'\n' {
            out.insert(0, b'\n');
        }
    }
    f.write_all(&out)
}

pub fn read_records(runner_dir: &Path) -> Vec<Record> {
    read_lines(&time_dir(runner_dir).join("intervals.jsonl"))
}

pub fn append_records(runner_dir: &Path, records: &[Record]) -> std::io::Result<()> {
    append_lines(runner_dir, "intervals.jsonl", records)
}

pub fn read_sent(runner_dir: &Path) -> HashMap<String, SentRecord> {
    read_lines::<SentRecord>(&time_dir(runner_dir).join("sent.jsonl"))
        .into_iter()
        .map(|r| (r.id.clone(), r))
        .collect()
}

pub fn append_sent(runner_dir: &Path, records: &[SentRecord]) -> std::io::Result<()> {
    append_lines(runner_dir, "sent.jsonl", records)
}

pub fn prune_records(runner_dir: &Path, now_s: u64) -> std::io::Result<()> {
    let all = read_records(runner_dir);
    let keep: Vec<&Record> = all
        .iter()
        .filter(|r| r.end_s.saturating_add(KEEP_S) >= now_s)
        .collect();
    if keep.len() == all.len() {
        return Ok(());
    }
    let mut out = Vec::new();
    for r in keep {
        serde_json::to_writer(&mut out, r).map_err(std::io::Error::other)?;
        out.push(b'\n');
    }
    write_atomic(&time_dir(runner_dir).join("intervals.jsonl"), &out)
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Meta {
    pub name: String,
    pub cwd: String,
    pub task: Option<TaskRef>,
}

struct ConnState {
    session: Option<String>,
    window: bool,
    last_ms: u64,
    typing_until_ms: u64,
}

struct Chunk {
    start_ms: u64,
    end_ms: u64,
    typing_ms: u64,
    reading_ms: u64,
    meta: Meta,
}

pub struct Clock {
    idle_ms: u64,
    conns: HashMap<u64, ConnState>,
    open: HashMap<String, Chunk>,
    metas: HashMap<String, Meta>,
    totals: HashMap<String, u64>,
    closed: Vec<Record>,
    floors: HashMap<String, u64>,
    ids: HashSet<String>,
    at: Option<u64>,
    prev_tick: Option<(u64, u64)>,
}

fn round_s(ms: u64) -> u64 {
    (ms + 500) / 1000
}

impl Clock {
    pub fn new(idle_s: u64) -> Clock {
        Clock {
            idle_ms: idle_s.clamp(IDLE_MIN_S, IDLE_MAX_S) * 1000,
            conns: HashMap::new(),
            open: HashMap::new(),
            metas: HashMap::new(),
            totals: HashMap::new(),
            closed: Vec::new(),
            floors: HashMap::new(),
            ids: HashSet::new(),
            at: None,
            prev_tick: None,
        }
    }

    pub fn load_totals(&mut self, records: &[Record]) {
        for r in records {
            *self.totals.entry(r.session.clone()).or_default() += r.typing_s + r.reading_s;
            let floor = self.floors.entry(r.session.clone()).or_default();
            *floor = (*floor).max(r.end_s.saturating_mul(1000));
            self.ids.insert(r.id.clone());
        }
    }

    pub fn requeue(&mut self, mut records: Vec<Record>) {
        records.append(&mut self.closed);
        let excess = records.len().saturating_sub(MAX_PENDING);
        records.drain(..excess);
        self.closed = records;
    }

    pub fn set_idle_s(&mut self, now: u64, idle_s: u64) {
        let now = self.clamp(now);
        self.advance(now);
        let old = self.idle_ms;
        let new = idle_s.clamp(IDLE_MIN_S, IDLE_MAX_S) * 1000;
        if new > old {
            for c in self.conns.values_mut() {
                if c.last_ms + old <= now {
                    c.last_ms = c.last_ms.saturating_sub(new - old);
                }
            }
        }
        self.idle_ms = new;
    }

    pub fn set_meta(&mut self, session: &str, meta: Meta) {
        self.metas.insert(session.to_string(), meta);
    }

    pub fn attend(&mut self, conn: u64, now: u64, session: Option<&str>, window: bool) {
        let now = self.clamp(now);
        self.advance(now);
        let c = self.conns.entry(conn).or_insert(ConnState {
            session: None,
            window,
            last_ms: now,
            typing_until_ms: 0,
        });
        if c.session.as_deref() != session {
            c.typing_until_ms = 0;
        }
        c.session = session.map(String::from);
        c.window = window;
        c.last_ms = now;
        self.close_inactive(now);
    }

    pub fn input(&mut self, conn: u64, now: u64, session: &str) {
        let now = self.clamp(now);
        self.advance(now);
        if let Some(c) = self.conns.get_mut(&conn) {
            if c.session.as_deref() == Some(session) {
                c.last_ms = now;
                c.typing_until_ms = now + TYPING_HOLD_MS;
            }
        }
    }

    pub fn scroll(&mut self, conn: u64, now: u64, session: &str) {
        let now = self.clamp(now);
        self.advance(now);
        if let Some(c) = self.conns.get_mut(&conn) {
            if c.session.as_deref() == Some(session) {
                c.last_ms = now;
            }
        }
    }

    pub fn drop_conn(&mut self, conn: u64, now: u64) {
        let now = self.clamp(now);
        self.advance(now);
        self.conns.remove(&conn);
        self.close_inactive(now);
        self.forget_idle_metas();
    }

    pub fn observe(&mut self, now_wall: u64, now_mono: u64) {
        if let Some((pw, pm)) = self.prev_tick {
            let wall = now_wall.saturating_sub(pw);
            let mono = now_mono.saturating_sub(pm);
            if wall > mono + SLEEP_SLACK_MS {
                let keys: Vec<String> = self.open.keys().cloned().collect();
                for k in keys {
                    self.close(&k);
                }
                self.at = Some(now_wall);
            }
        }
        self.prev_tick = Some((now_wall, now_mono));
    }

    pub fn tick(&mut self, now_wall: u64, now_mono: u64) {
        self.observe(now_wall, now_mono);
        let now = self.clamp(now_wall);
        self.advance(now);
        self.close_inactive(now);
        self.forget_idle_metas();
    }

    pub fn shutdown(&mut self, now: u64) {
        let now = self.clamp(now);
        self.advance(now);
        self.conns.clear();
        let keys: Vec<String> = self.open.keys().cloned().collect();
        for k in keys {
            self.close(&k);
        }
    }

    pub fn drain_closed(&mut self) -> Vec<Closed> {
        std::mem::take(&mut self.closed)
    }

    pub fn open_ms(&self, session: &str) -> u64 {
        self.open
            .get(session)
            .map_or(0, |c| c.typing_ms + c.reading_ms)
    }

    pub fn closed_s(&self, session: &str) -> u64 {
        self.totals.get(session).copied().unwrap_or(0)
    }

    pub fn human_s(&self, session: &str) -> u64 {
        self.closed_s(session) + round_s(self.open_ms(session))
    }

    fn clamp(&self, now: u64) -> u64 {
        self.at.map_or(now, |a| a.max(now))
    }

    fn advance(&mut self, now: u64) {
        let Some(from) = self.at else {
            self.at = Some(now);
            return;
        };
        if now <= from {
            return;
        }
        let mut cuts: Vec<u64> = Vec::new();
        for c in self.conns.values() {
            for t in [c.last_ms + self.idle_ms, c.typing_until_ms] {
                if t > from && t < now {
                    cuts.push(t);
                }
            }
        }
        cuts.sort_unstable();
        cuts.dedup();
        cuts.push(now);
        let mut a = from;
        for b in cuts {
            self.segment(a, b);
            a = b;
        }
        self.at = Some(now);
    }

    fn active_at(&self, t: u64) -> HashMap<String, bool> {
        let mut active: HashMap<String, bool> = HashMap::new();
        for c in self.conns.values() {
            if let (true, Some(s)) = (c.window, &c.session) {
                if t < c.last_ms + self.idle_ms {
                    *active.entry(s.clone()).or_default() |= t < c.typing_until_ms;
                }
            }
        }
        active
    }

    fn segment(&mut self, a: u64, b: u64) {
        let active = self.active_at(a);
        let gone: Vec<String> = self
            .open
            .keys()
            .filter(|k| !active.contains_key(*k))
            .cloned()
            .collect();
        for k in gone {
            self.close(&k);
        }
        for (s, typing) in active {
            self.add(&s, a, b, typing);
        }
    }

    fn close_inactive(&mut self, now: u64) {
        let active = self.active_at(now);
        let gone: Vec<String> = self
            .open
            .keys()
            .filter(|k| !active.contains_key(*k))
            .cloned()
            .collect();
        for k in gone {
            self.close(&k);
        }
    }

    fn forget_idle_metas(&mut self) {
        let open = &self.open;
        let conns = &self.conns;
        self.metas.retain(|k, _| {
            open.contains_key(k) || conns.values().any(|c| c.session.as_deref() == Some(k))
        });
    }

    fn add(&mut self, session: &str, mut a: u64, b: u64, typing: bool) {
        if let Some(floor) = self.floors.get(session).copied() {
            if a < floor && !self.open.contains_key(session) {
                a = floor.min(b);
            }
        }
        while a < b {
            if self.open.get(session).is_some_and(|c| c.end_ms != a) {
                self.close(session);
            }
            if !self.open.contains_key(session) {
                let meta = self.metas.get(session).cloned().unwrap_or_else(|| Meta {
                    name: session.to_string(),
                    ..Meta::default()
                });
                self.open.insert(
                    session.to_string(),
                    Chunk {
                        start_ms: a,
                        end_ms: a,
                        typing_ms: 0,
                        reading_ms: 0,
                        meta,
                    },
                );
            }
            let Some(c) = self.open.get_mut(session) else {
                return;
            };
            let take = (b - a).min(CHUNK_MS - (c.end_ms - c.start_ms));
            if typing {
                c.typing_ms += take;
            } else {
                c.reading_ms += take;
            }
            c.end_ms += take;
            a += take;
            if c.end_ms - c.start_ms >= CHUNK_MS {
                self.close(session);
            }
        }
    }

    fn close(&mut self, session: &str) {
        let Some(c) = self.open.remove(session) else {
            return;
        };
        let (start_s, end_s) = (c.start_ms / 1000, c.end_ms / 1000);
        let seconds = round_s(c.typing_ms + c.reading_ms).min(end_s.saturating_sub(start_s));
        if seconds == 0 {
            return;
        }
        let typing_s = round_s(c.typing_ms).min(seconds);
        *self.totals.entry(session.to_string()).or_default() += seconds;
        let mut id = format!("{session}-{start_s}");
        let mut n = 2;
        while self.ids.contains(&id) {
            id = format!("{session}-{start_s}-{n}");
            n += 1;
        }
        self.ids.insert(id.clone());
        self.closed.push(Record {
            id,
            session: session.to_string(),
            name: c.meta.name,
            cwd: c.meta.cwd,
            task: c.meta.task,
            start_s,
            end_s,
            typing_s,
            reading_s: seconds - typing_s,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    const S1: &str = "s_aaaaaaaaaa";
    const S2: &str = "s_bbbbbbbbbb";
    const T0: u64 = 1_000_000;

    fn clock() -> Clock {
        Clock::new(120)
    }

    fn secs(c: &mut Clock, session: &str) -> u64 {
        c.drain_closed()
            .iter()
            .filter(|r| r.session == session)
            .map(|r| r.typing_s + r.reading_s)
            .sum()
    }

    #[test]
    fn a_focused_attending_session_counts_until_blur() {
        let mut c = clock();
        c.attend(1, T0, Some(S1), true);
        c.attend(1, T0 + 20_000, Some(S1), false);
        c.tick(T0 + 40_000, 40_000);
        c.shutdown(T0 + 40_000);
        assert_eq!(secs(&mut c, S1), 20);
    }

    #[test]
    fn window_blur_then_refocus_resumes() {
        let mut c = clock();
        c.attend(1, T0, Some(S1), true);
        c.attend(1, T0 + 10_000, Some(S1), false);
        c.attend(1, T0 + 30_000, Some(S1), true);
        c.attend(1, T0 + 40_000, Some(S1), true);
        c.shutdown(T0 + 40_000);
        assert_eq!(secs(&mut c, S1), 20);
    }

    #[test]
    fn silence_counts_exactly_the_idle_cap() {
        let mut c = clock();
        c.attend(1, T0, Some(S1), true);
        c.tick(T0 + 119_000, 119_000);
        c.tick(T0 + 600_000, 600_000);
        c.shutdown(T0 + 600_000);
        assert_eq!(secs(&mut c, S1), 120);
    }

    #[test]
    fn two_connections_on_one_session_count_once() {
        let mut c = clock();
        c.attend(1, T0, Some(S1), true);
        c.attend(2, T0, Some(S1), true);
        c.attend(1, T0 + 10_000, Some(S1), true);
        c.attend(2, T0 + 10_000, Some(S1), true);
        c.shutdown(T0 + 10_000);
        assert_eq!(secs(&mut c, S1), 10);
    }

    #[test]
    fn a_connection_on_another_session_does_not_count() {
        let mut c = clock();
        c.attend(1, T0, Some(S2), true);
        c.attend(1, T0 + 10_000, Some(S2), true);
        c.attend(2, T0 + 10_000, None, true);
        c.shutdown(T0 + 20_000);
        assert_eq!(secs(&mut c, S1), 0);
        assert_eq!(secs(&mut c, S2), 0);
    }

    #[test]
    fn unattended_connections_never_count() {
        let mut c = clock();
        c.input(1, T0, S1);
        c.scroll(1, T0 + 1_000, S1);
        c.tick(T0 + 30_000, 30_000);
        c.shutdown(T0 + 30_000);
        assert!(c.drain_closed().is_empty());
        assert_eq!(c.human_s(S1), 0);
    }

    #[test]
    fn input_for_another_session_than_the_focused_one_does_not_count() {
        let mut c = clock();
        c.attend(1, T0, Some(S1), true);
        c.input(1, T0 + 100_000, S2);
        c.shutdown(T0 + 200_000);
        assert_eq!(secs(&mut c, S1), 120);
        assert_eq!(secs(&mut c, S2), 0);
    }

    #[test]
    fn typing_is_held_for_ten_seconds_then_reading() {
        let mut c = clock();
        c.attend(1, T0, Some(S1), true);
        c.input(1, T0 + 5_000, S1);
        c.shutdown(T0 + 30_000);
        let r = c.drain_closed();
        assert_eq!(r.len(), 1);
        assert_eq!((r[0].typing_s, r[0].reading_s), (10, 20));
    }

    #[test]
    fn typing_from_one_connection_marks_the_union_as_typing() {
        let mut c = clock();
        c.attend(1, T0, Some(S1), true);
        c.attend(2, T0, Some(S1), true);
        c.input(2, T0 + 2_000, S1);
        c.shutdown(T0 + 12_000);
        let r = c.drain_closed();
        assert_eq!((r[0].typing_s, r[0].reading_s), (10, 2));
    }

    #[test]
    fn long_activity_rolls_into_contiguous_chunks() {
        let mut c = clock();
        let mut t = T0;
        c.attend(1, t, Some(S1), true);
        while t < T0 + 150_000 {
            t += 5_000;
            c.attend(1, t, Some(S1), true);
        }
        c.shutdown(t);
        let r = c.drain_closed();
        assert_eq!(r.len(), 3);
        assert_eq!(r.iter().map(|x| x.typing_s + x.reading_s).sum::<u64>(), 150);
        for w in r.windows(2) {
            assert_eq!(w[0].end_s, w[1].start_s);
        }
        for x in &r {
            assert_eq!(x.id, format!("{S1}-{}", x.start_s));
            assert!(x.typing_s + x.reading_s <= x.end_s - x.start_s);
            assert!(x.id.len() >= 8 && x.id.len() <= 64);
        }
    }

    #[test]
    fn a_sleep_gap_is_not_counted() {
        let mut c = clock();
        c.attend(1, T0, Some(S1), true);
        c.tick(T0 + 5_000, 5_000);
        c.tick(T0 + 15_000, 5_500);
        c.attend(1, T0 + 15_000, Some(S1), true);
        c.shutdown(T0 + 16_000);
        let r = c.drain_closed();
        let total: u64 = r.iter().map(|x| x.typing_s + x.reading_s).sum();
        assert_eq!(total, 5 + 1, "{r:?}");
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn dropping_a_connection_closes_its_chunk() {
        let mut c = clock();
        c.attend(1, T0, Some(S1), true);
        c.attend(1, T0 + 8_000, Some(S1), true);
        c.drop_conn(1, T0 + 8_000);
        let r = c.drain_closed();
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].reading_s, 8);
        assert_eq!(c.open_ms(S1), 0);
        assert_eq!(c.human_s(S1), 8);
    }

    #[test]
    fn dropping_one_of_two_connections_keeps_the_session_open() {
        let mut c = clock();
        c.attend(1, T0, Some(S1), true);
        c.attend(2, T0, Some(S1), true);
        c.drop_conn(1, T0 + 5_000);
        assert!(c.drain_closed().is_empty());
        assert_eq!(c.open_ms(S1), 5_000);
    }

    #[test]
    fn chunks_under_one_second_are_dropped() {
        let mut c = clock();
        c.attend(1, T0, Some(S1), true);
        c.attend(1, T0 + 400, None, true);
        c.shutdown(T0 + 1_000);
        assert!(c.drain_closed().is_empty());
        assert_eq!(c.human_s(S1), 0);
    }

    #[test]
    fn seconds_never_exceed_the_whole_second_span() {
        let mut c = clock();
        c.attend(1, T0 + 100, Some(S1), true);
        c.attend(1, T0 + 700, None, true);
        c.shutdown(T0 + 2_000);
        assert!(c.drain_closed().is_empty());
    }

    #[test]
    fn meta_is_captured_when_the_chunk_opens() {
        let mut c = clock();
        let task = TaskRef {
            workspace_id: 5,
            task_id: 9,
        };
        c.set_meta(
            S1,
            Meta {
                name: "one".into(),
                cwd: "/w".into(),
                task: Some(task),
            },
        );
        c.attend(1, T0, Some(S1), true);
        c.attend(1, T0 + 1_000, Some(S1), true);
        c.set_meta(
            S1,
            Meta {
                name: "two".into(),
                ..Meta::default()
            },
        );
        c.shutdown(T0 + 5_000);
        let r = c.drain_closed();
        assert_eq!((r[0].name.as_str(), r[0].cwd.as_str()), ("one", "/w"));
        assert_eq!(r[0].task, Some(task));
    }

    #[test]
    fn totals_include_loaded_records_and_the_open_chunk() {
        let mut c = clock();
        c.load_totals(&[Record {
            id: "x".into(),
            session: S1.into(),
            name: String::new(),
            cwd: String::new(),
            task: None,
            start_s: 1,
            end_s: 9,
            typing_s: 3,
            reading_s: 4,
        }]);
        c.attend(1, T0, Some(S1), true);
        c.tick(T0 + 5_000, 5_000);
        assert_eq!(c.human_s(S1), 12);
    }

    #[test]
    fn settings_default_clamp_and_roundtrip() {
        let t = tempfile::tempdir().unwrap();
        std::fs::set_permissions(t.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(
            Settings::load(t.path()),
            Settings {
                sync: true,
                idle_s: 120
            }
        );
        Settings {
            sync: false,
            idle_s: 5,
        }
        .save(t.path())
        .unwrap();
        assert_eq!(
            Settings::load(t.path()),
            Settings {
                sync: false,
                idle_s: 30
            }
        );
        std::fs::write(
            time_dir(t.path()).join("settings.json"),
            r#"{"idle_s":99999}"#,
        )
        .unwrap();
        assert_eq!(
            Settings::load(t.path()),
            Settings {
                sync: true,
                idle_s: 3600
            }
        );
        std::fs::write(time_dir(t.path()).join("settings.json"), "nope").unwrap();
        assert_eq!(Settings::load(t.path()), Settings::default());
        let mode = std::fs::metadata(time_dir(t.path()))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    fn rec(id: &str, end_s: u64) -> Record {
        Record {
            id: id.into(),
            session: S1.into(),
            name: "n".into(),
            cwd: "/w".into(),
            task: Some(TaskRef {
                workspace_id: 1,
                task_id: 2,
            }),
            start_s: end_s - 5,
            end_s,
            typing_s: 2,
            reading_s: 3,
        }
    }

    #[test]
    fn records_roundtrip_skip_bad_lines_and_prune() {
        let t = tempfile::tempdir().unwrap();
        std::fs::set_permissions(t.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let now = 100 * 86_400;
        let old = rec("old-record", now - 46 * 86_400);
        let new = rec("new-record", now - 10);
        append_records(t.path(), &[old.clone(), new.clone()]).unwrap();
        let path = time_dir(t.path()).join("intervals.jsonl");
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"garbage\n{\"id\":1}\n").unwrap();
        assert_eq!(read_records(t.path()), vec![old, new.clone()]);
        prune_records(t.path(), now).unwrap();
        assert_eq!(read_records(t.path()), vec![new]);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }

    #[test]
    fn sent_records_roundtrip() {
        let t = tempfile::tempdir().unwrap();
        std::fs::set_permissions(t.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let a = SentRecord {
            id: "a-1".into(),
            status: "created".into(),
            counted_s: Some(5),
            at_s: 9,
        };
        let b = SentRecord {
            id: "b-2".into(),
            status: "local".into(),
            counted_s: None,
            at_s: 10,
        };
        append_sent(t.path(), &[a.clone(), b.clone()]).unwrap();
        let got = read_sent(t.path());
        assert_eq!(got.get("a-1"), Some(&a));
        assert_eq!(got.get("b-2"), Some(&b));
    }

    #[test]
    fn appending_does_not_follow_a_planted_symlink() {
        let t = tempfile::tempdir().unwrap();
        std::fs::set_permissions(t.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let victim = t.path().join("victim");
        std::fs::write(&victim, "keep").unwrap();
        std::fs::create_dir(time_dir(t.path())).unwrap();
        std::fs::set_permissions(time_dir(t.path()), std::fs::Permissions::from_mode(0o700))
            .unwrap();
        std::os::unix::fs::symlink(&victim, time_dir(t.path()).join("intervals.jsonl")).unwrap();
        assert!(append_records(t.path(), &[rec("some-record", 50)]).is_err());
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "keep");
    }

    #[test]
    fn a_wall_jump_without_progress_is_not_counted_even_if_input_comes_first() {
        let mut c = clock();
        c.observe(T0, 0);
        c.attend(1, T0, Some(S1), true);
        c.tick(T0 + 5_000, 5_000);
        let wake = T0 + 7_200_000;
        c.observe(wake, 5_100);
        c.input(1, wake, S1);
        c.shutdown(wake);
        assert_eq!(secs(&mut c, S1), 5);
    }

    #[test]
    fn a_wall_jump_before_set_idle_is_not_counted() {
        let mut c = clock();
        c.observe(T0, 0);
        c.attend(1, T0, Some(S1), true);
        c.tick(T0 + 5_000, 5_000);
        let wake = T0 + 60_000;
        c.observe(wake, 5_100);
        c.set_idle_s(wake, 3600);
        c.shutdown(wake);
        assert_eq!(secs(&mut c, S1), 5);
    }

    #[test]
    fn requeued_records_come_back_first_and_are_bounded() {
        let mut c = clock();
        c.attend(1, T0, Some(S1), true);
        c.shutdown(T0 + 5_000);
        let first = c.drain_closed();
        assert_eq!(first.len(), 1);
        c.requeue(first.clone());
        assert_eq!(c.drain_closed(), first);
        let many: Vec<Record> = (0..MAX_PENDING + 5)
            .map(|i| rec(&format!("rec-{i:08}"), 100))
            .collect();
        c.requeue(many);
        let got = c.drain_closed();
        assert_eq!(got.len(), MAX_PENDING);
        assert_eq!(got[0].id, "rec-00000005");
    }

    #[test]
    fn raising_the_idle_cap_does_not_revive_idle_connections() {
        let t0 = 10 * T0;
        let mut c = clock();
        c.attend(1, t0, Some(S1), true);
        c.tick(t0 + 200_000, 200_000);
        c.set_idle_s(t0 + 200_000, 3600);
        c.tick(t0 + 400_000, 400_000);
        c.shutdown(t0 + 400_000);
        assert_eq!(secs(&mut c, S1), 120);
    }

    #[test]
    fn raising_the_idle_cap_keeps_active_connections_active() {
        let mut c = clock();
        c.attend(1, T0, Some(S1), true);
        c.tick(T0 + 60_000, 60_000);
        c.set_idle_s(T0 + 60_000, 600);
        c.tick(T0 + 800_000, 800_000);
        c.shutdown(T0 + 800_000);
        assert_eq!(secs(&mut c, S1), 600);
    }

    #[test]
    fn restart_never_starts_a_chunk_before_recorded_time() {
        let mut c = clock();
        let mut r = rec("old-record", 1000);
        r.start_s = 995;
        c.load_totals(&[r]);
        c.attend(1, 990_000, Some(S1), true);
        c.shutdown(1_004_000);
        let got = c.drain_closed();
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].start_s, got[0].end_s), (1000, 1004));
        assert_eq!(got[0].reading_s, 4);
    }

    #[test]
    fn colliding_ids_get_a_suffix() {
        let mut c = clock();
        let mut r = rec(&format!("{S1}-1000"), 1000);
        r.start_s = 995;
        c.load_totals(&[r]);
        c.attend(1, 1_000_000, Some(S1), true);
        c.shutdown(1_005_000);
        let got = c.drain_closed();
        assert_eq!(got[0].id, format!("{S1}-1000-2"));
        assert!(got[0].id.len() <= 64);
    }

    #[test]
    fn scrolling_another_session_is_not_activity() {
        let mut c = clock();
        c.attend(1, T0, Some(S1), true);
        c.scroll(1, T0 + 100_000, S2);
        c.shutdown(T0 + 300_000);
        assert_eq!(secs(&mut c, S1), 120);
    }

    #[test]
    fn scrolling_the_attended_session_is_activity() {
        let mut c = clock();
        c.attend(1, T0, Some(S1), true);
        c.scroll(1, T0 + 100_000, S1);
        c.shutdown(T0 + 300_000);
        assert_eq!(secs(&mut c, S1), 220);
    }

    #[test]
    fn appending_after_a_partial_line_starts_a_new_line() {
        let t = tempfile::tempdir().unwrap();
        std::fs::set_permissions(t.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        append_records(t.path(), &[rec("first-record", 50)]).unwrap();
        let path = time_dir(t.path()).join("intervals.jsonl");
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"{\"id\":\"cut").unwrap();
        append_records(t.path(), &[rec("second-record", 60)]).unwrap();
        let got = read_records(t.path());
        assert_eq!(
            got.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
            ["first-record", "second-record"]
        );
    }
}
