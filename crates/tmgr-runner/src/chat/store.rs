use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use tmgr_ipc::chat::{limits, ChatMessage, RoomKind, RoomRef};

pub const ROOM_FILE_CAP: u64 = 16 << 20;
const LAST_WINDOW: u64 = 1 << 20;
const SEARCH_BYTES: u64 = 64 << 20;
const DEFAULT_SEARCH: u32 = 50;

pub struct Store {
    dir: PathBuf,
    cap: u64,
}

pub fn room_key(room: &RoomRef) -> Result<String, String> {
    room.validate()?;
    Ok(match (room.kind, room.session.as_deref()) {
        (RoomKind::Session, Some(s)) => format!("session-{s}"),
        _ => "manager".into(),
    })
}

fn well_formed_key(name: &str) -> bool {
    name == "manager"
        || name.strip_prefix("session-").is_some_and(|t| {
            !t.is_empty()
                && t.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        })
}

impl Store {
    pub fn open(runner_dir: &Path) -> io::Result<Store> {
        Store::open_with_cap(runner_dir, ROOM_FILE_CAP)
    }

    pub fn open_with_cap(runner_dir: &Path, cap: u64) -> io::Result<Store> {
        let dir = runner_dir.join("chat");
        fs::DirBuilder::new()
            .mode(0o700)
            .recursive(true)
            .create(&dir)?;
        Ok(Store { dir, cap })
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.jsonl"))
    }

    fn older(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{key}.jsonl.1"))
    }

    pub fn append(&self, msg: &ChatMessage) -> Result<(), String> {
        msg.validate()?;
        let key = room_key(&msg.room)?;
        let mut line = serde_json::to_vec(msg).map_err(|e| e.to_string())?;
        line.push(b'\n');
        self.write_line(&key, &line)
            .map_err(|e| format!("chat store: {e}"))
    }

    fn write_line(&self, key: &str, line: &[u8]) -> io::Result<()> {
        let path = self.path(key);
        if let Ok(meta) = fs::symlink_metadata(&path) {
            if meta.len() + line.len() as u64 > self.cap {
                fs::rename(&path, self.older(key))?;
            }
        }
        let mut f = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)?;
        let len = f.metadata()?.len();
        let mut buf = Vec::with_capacity(line.len() + 1);
        if len > 0 {
            let mut last = [0u8; 1];
            f.seek(SeekFrom::Start(len - 1))?;
            f.read_exact(&mut last)?;
            if last[0] != b'\n' {
                buf.push(b'\n');
            }
        }
        buf.extend_from_slice(line);
        f.write_all(&buf)
    }

    fn open_read(&self, path: &Path) -> Option<File> {
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .ok()
    }

    fn read_room(&self, key: &str) -> Vec<ChatMessage> {
        let mut out = Vec::new();
        for path in [self.older(key), self.path(key)] {
            if let Some(f) = self.open_read(&path) {
                out.extend(parse_lines(BufReader::new(f.take(self.cap + (1 << 20)))));
            }
        }
        out
    }

    pub fn history(
        &self,
        room: &RoomRef,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<ChatMessage>, String> {
        let key = room_key(room)?;
        let limit = limit
            .unwrap_or(limits::MAX_HISTORY)
            .clamp(1, limits::MAX_HISTORY) as usize;
        let all = self.read_room(&key);
        let eligible: Vec<ChatMessage> = match before {
            Some(b) => match all.iter().position(|m| m.id == b) {
                Some(i) => all.into_iter().take(i).collect(),
                None => Vec::new(),
            },
            None => all,
        };
        let skip = eligible.len().saturating_sub(limit);
        Ok(eligible.into_iter().skip(skip).collect())
    }

    pub fn exists(&self, room: &RoomRef) -> bool {
        room_key(room).is_ok_and(|k| self.path(&k).exists() || self.older(&k).exists())
    }

    pub fn get(&self, room: &RoomRef, id: &str) -> Result<Option<ChatMessage>, String> {
        let key = room_key(room)?;
        Ok(self.read_room(&key).into_iter().find(|m| m.id == id))
    }

    pub fn last(&self, room: &RoomRef) -> Option<ChatMessage> {
        let key = room_key(room).ok()?;
        for path in [self.path(&key), self.older(&key)] {
            let Some(mut f) = self.open_read(&path) else {
                continue;
            };
            let len = f.metadata().ok()?.len();
            let start = len.saturating_sub(LAST_WINDOW);
            f.seek(SeekFrom::Start(start)).ok()?;
            let mut buf = Vec::new();
            f.take(LAST_WINDOW).read_to_end(&mut buf).ok()?;
            let body = if start > 0 {
                buf.iter()
                    .position(|b| *b == b'\n')
                    .map_or(&buf[..0], |i| &buf[i + 1..])
            } else {
                &buf[..]
            };
            if let Some(m) = parse_lines(body).pop() {
                return Some(m);
            }
        }
        None
    }

    pub fn keys(&self) -> Vec<String> {
        let Ok(rd) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut keys: Vec<String> = rd
            .filter_map(|e| {
                let name = e.ok()?.file_name().into_string().ok()?;
                let key = name.strip_suffix(".jsonl")?;
                well_formed_key(key).then(|| key.to_string())
            })
            .collect();
        keys.sort();
        keys
    }

    pub fn search(
        &self,
        query: &str,
        room: Option<&RoomRef>,
        limit: Option<u32>,
    ) -> Result<Vec<ChatMessage>, String> {
        let query = query.trim();
        if query.is_empty() || query.len() > limits::MAX_QUERY {
            return Err("query must be 1 to 256 characters".into());
        }
        let needle = query.to_lowercase();
        let limit = limit
            .unwrap_or(DEFAULT_SEARCH)
            .clamp(1, limits::MAX_HISTORY) as usize;
        let keys = match room {
            Some(r) => vec![room_key(r)?],
            None => self.keys(),
        };
        let mut budget = SEARCH_BYTES;
        let mut hits: Vec<ChatMessage> = Vec::new();
        for key in keys {
            let mut found = 0usize;
            for path in [self.path(&key), self.older(&key)] {
                let Some(f) = self.open_read(&path) else {
                    continue;
                };
                let size = f.metadata().map_or(0, |m| m.len());
                if size > budget {
                    break;
                }
                budget -= size;
                let mut msgs = parse_lines(BufReader::new(f.take(self.cap + (1 << 20))));
                msgs.reverse();
                for m in msgs {
                    if m.body_md.to_lowercase().contains(&needle) {
                        hits.push(m);
                        found += 1;
                        if found >= limit {
                            break;
                        }
                    }
                }
                if found >= limit {
                    break;
                }
            }
        }
        hits.sort_by(|a, b| b.id.cmp(&a.id));
        hits.truncate(limit);
        Ok(hits)
    }
}

fn parse_lines(reader: impl BufRead) -> Vec<ChatMessage> {
    let mut out = Vec::new();
    for line in reader.split(b'\n') {
        let Ok(line) = line else { break };
        if line.is_empty() {
            continue;
        }
        if let Ok(m) = serde_json::from_slice::<ChatMessage>(&line) {
            if m.validate().is_ok() {
                out.push(m);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tmgr_ipc::chat::{message_id_at, Author, AuthorKind, MessageKind};

    const MX: &str = "mx_0123456789abcdef";

    fn msg(n: u64, room: &RoomRef, body: &str) -> ChatMessage {
        ChatMessage {
            v: 1,
            id: message_id_at(n, [0; 10]),
            room: room.clone(),
            author: Author {
                kind: AuthorKind::Owner,
                id: "owner".into(),
                machine: None,
                device: None,
                agent: None,
            },
            ts_ms: n,
            kind: MessageKind::Text,
            body_md: body.into(),
            attachments: Vec::new(),
            reply_to: None,
            decision: None,
        }
    }

    fn store() -> (tempfile::TempDir, Store) {
        let t = tempfile::tempdir().unwrap();
        let s = Store::open(t.path()).unwrap();
        (t, s)
    }

    #[test]
    fn append_then_history_returns_oldest_first() {
        let (_t, s) = store();
        let r = RoomRef::manager(MX);
        for n in 1..=3 {
            s.append(&msg(n, &r, &format!("m{n}"))).unwrap();
        }
        let h = s.history(&r, None, None).unwrap();
        assert_eq!(
            h.iter().map(|m| m.body_md.as_str()).collect::<Vec<_>>(),
            ["m1", "m2", "m3"]
        );
    }

    #[test]
    fn history_pages_backwards_with_before_and_limit() {
        let (_t, s) = store();
        let r = RoomRef::session(MX, "s_abcdefghij");
        for n in 1..=10 {
            s.append(&msg(n, &r, &format!("m{n}"))).unwrap();
        }
        let page = s.history(&r, None, Some(4)).unwrap();
        assert_eq!(
            page.iter().map(|m| m.body_md.as_str()).collect::<Vec<_>>(),
            ["m7", "m8", "m9", "m10"]
        );
        let prev = s.history(&r, Some(&page[0].id), Some(4)).unwrap();
        assert_eq!(
            prev.iter().map(|m| m.body_md.as_str()).collect::<Vec<_>>(),
            ["m3", "m4", "m5", "m6"]
        );
        assert_eq!(
            s.history(&r, Some(&prev[0].id), Some(500)).unwrap().len(),
            2
        );
        assert_eq!(s.history(&r, None, Some(9999)).unwrap().len(), 10);
    }

    #[test]
    fn get_finds_a_message_by_id_in_either_generation() {
        let t = tempfile::tempdir().unwrap();
        let s = Store::open_with_cap(t.path(), 2000).unwrap();
        let r = RoomRef::manager(MX);
        assert!(!s.exists(&r));
        for n in 1..=20 {
            s.append(&msg(n, &r, &format!("m{n}"))).unwrap();
        }
        assert!(s.exists(&r));
        let old = s.read_room("manager")[0].id.clone();
        assert!(s.get(&r, &old).unwrap().is_some());
        assert_eq!(
            s.get(&r, &message_id_at(20, [0; 10]))
                .unwrap()
                .unwrap()
                .body_md,
            "m20"
        );
        assert!(s.get(&r, &message_id_at(99, [0; 10])).unwrap().is_none());
    }

    #[test]
    fn paging_follows_append_order_even_when_an_old_id_is_in_the_middle() {
        let (_t, s) = store();
        let r = RoomRef::manager(MX);
        for n in [10, 11, 3, 12, 13] {
            s.append(&msg(n, &r, &format!("m{n}"))).unwrap();
        }
        let page = s.history(&r, None, Some(2)).unwrap();
        assert_eq!(
            page.iter().map(|m| m.body_md.as_str()).collect::<Vec<_>>(),
            ["m12", "m13"]
        );
        let prev = s.history(&r, Some(&page[0].id), Some(2)).unwrap();
        assert_eq!(
            prev.iter().map(|m| m.body_md.as_str()).collect::<Vec<_>>(),
            ["m11", "m3"]
        );
        let first = s.history(&r, Some(&prev[0].id), Some(2)).unwrap();
        assert_eq!(
            first.iter().map(|m| m.body_md.as_str()).collect::<Vec<_>>(),
            ["m10"]
        );
        assert!(s
            .history(&r, Some(&message_id_at(99, [0; 10])), None)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn rooms_are_separate_files() {
        let (_t, s) = store();
        let a = RoomRef::manager(MX);
        let b = RoomRef::session(MX, "s_abcdefghij");
        s.append(&msg(1, &a, "to manager")).unwrap();
        s.append(&msg(2, &b, "to session")).unwrap();
        assert_eq!(s.history(&a, None, None).unwrap().len(), 1);
        assert_eq!(s.history(&b, None, None).unwrap()[0].body_md, "to session");
        assert_eq!(s.keys(), ["manager", "session-s_abcdefghij"]);
        assert_eq!(s.last(&b).unwrap().body_md, "to session");
    }

    #[test]
    fn files_and_dir_are_private() {
        let (t, s) = store();
        s.append(&msg(1, &RoomRef::manager(MX), "x")).unwrap();
        let mode = |p: PathBuf| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(t.path().join("chat")), 0o700);
        assert_eq!(mode(t.path().join("chat/manager.jsonl")), 0o600);
    }

    #[test]
    fn corrupt_and_truncated_lines_are_skipped_and_append_recovers() {
        let (t, s) = store();
        let r = RoomRef::manager(MX);
        s.append(&msg(1, &r, "good1")).unwrap();
        let path = t.path().join("chat/manager.jsonl");
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"not json\n{\"v\":1,\"id\":\"m_bad\"}\n\xff\xfe\n{\"v\":1,\"id\":")
            .unwrap();
        drop(f);
        s.append(&msg(2, &r, "good2")).unwrap();
        let h = s.history(&r, None, None).unwrap();
        assert_eq!(
            h.iter().map(|m| m.body_md.as_str()).collect::<Vec<_>>(),
            ["good1", "good2"]
        );
        assert_eq!(s.last(&r).unwrap().body_md, "good2");
    }

    #[test]
    fn search_is_case_insensitive_newest_first_and_scoped() {
        let (_t, s) = store();
        let a = RoomRef::manager(MX);
        let b = RoomRef::session(MX, "s_abcdefghij");
        s.append(&msg(1, &a, "Deploy the Thing")).unwrap();
        s.append(&msg(2, &b, "nothing here")).unwrap();
        s.append(&msg(3, &b, "DEPLOY done")).unwrap();
        let all = s.search("deploy", None, None).unwrap();
        assert_eq!(
            all.iter().map(|m| m.body_md.as_str()).collect::<Vec<_>>(),
            ["DEPLOY done", "Deploy the Thing"]
        );
        assert_eq!(s.search("deploy", Some(&a), None).unwrap().len(), 1);
        assert_eq!(s.search("deploy", None, Some(1)).unwrap().len(), 1);
        assert!(s.search("zzz", None, None).unwrap().is_empty());
        assert!(s.search("  ", None, None).is_err());
        assert!(s.search(&"x".repeat(257), None, None).is_err());
    }

    #[test]
    fn unsafe_rooms_and_invalid_messages_are_refused() {
        let (t, s) = store();
        for session in ["../x", "a/b", "", "a b"] {
            let r = RoomRef::session(MX, session);
            assert!(room_key(&r).is_err(), "{session}");
            assert!(s.history(&r, None, None).is_err());
            assert!(s.append(&msg(1, &r, "x")).is_err());
        }
        let dotted = RoomRef::session(MX, "..");
        let key = room_key(&dotted).unwrap();
        assert_eq!(key, "session-..");
        assert!(t
            .path()
            .join("chat")
            .join(format!("{key}.jsonl"))
            .starts_with(t.path().join("chat")));
        let mut m = msg(1, &RoomRef::manager(MX), "x");
        m.body_md = "x".repeat(limits::MAX_BODY + 1);
        assert!(s.append(&m).is_err());
        assert!(s.keys().is_empty());
    }

    #[test]
    fn a_full_room_file_rotates_once_and_history_spans_both() {
        let t = tempfile::tempdir().unwrap();
        let s = Store::open_with_cap(t.path(), 2000).unwrap();
        let r = RoomRef::manager(MX);
        for n in 1..=20 {
            s.append(&msg(n, &r, &"y".repeat(100))).unwrap();
        }
        assert!(t.path().join("chat/manager.jsonl.1").exists());
        assert!(
            fs::metadata(t.path().join("chat/manager.jsonl"))
                .unwrap()
                .len()
                <= 2000
        );
        let h = s.history(&r, None, None).unwrap();
        assert!(h.len() > 6 && h.len() <= 20);
        assert_eq!(h.last().unwrap().id, message_id_at(20, [0; 10]));
        assert!(h.windows(2).all(|w| w[0].id < w[1].id));
        assert_eq!(s.keys(), ["manager"]);
    }

    #[test]
    fn a_symlinked_room_file_is_not_followed() {
        let (t, s) = store();
        let target = t.path().join("elsewhere");
        fs::write(&target, "").unwrap();
        std::os::unix::fs::symlink(&target, t.path().join("chat/manager.jsonl")).unwrap();
        assert!(s.append(&msg(1, &RoomRef::manager(MX), "x")).is_err());
        assert_eq!(fs::read(&target).unwrap().len(), 0);
    }
}
