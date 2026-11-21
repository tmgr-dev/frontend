pub mod store;
pub mod tail;

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};
use store::Store;
use tmgr_ipc::api::{SessionInfo, StateKind};
use tmgr_ipc::chat::{
    message_id_at, Attachment, Author, AuthorKind, ChatMessage, ChatPush, MessageKind, RoomInfo,
    RoomRef, CHAT_V,
};

const MACHINE_FILE: &str = "machine.id";
const SUBSCRIBER_QUEUE: usize = 256;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn valid_machine_id(s: &str) -> bool {
    s.strip_prefix("mx_")
        .is_some_and(|r| r.len() == 16 && r.bytes().all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f')))
}

fn random_machine_id() -> String {
    let mut b = [0u8; 8];
    getrandom::fill(&mut b).expect("system random source");
    format!(
        "mx_{}",
        b.iter().map(|x| format!("{x:02x}")).collect::<String>()
    )
}

pub fn machine_id(runner_dir: &Path) -> io::Result<String> {
    let path = runner_dir.join(MACHINE_FILE);
    loop {
        match fs::read_to_string(&path) {
            Ok(text) => {
                let id = text.trim();
                return if valid_machine_id(id) {
                    Ok(id.to_string())
                } else {
                    Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "machine.id is malformed",
                    ))
                };
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        let id = random_machine_id();
        let tmp = runner_dir.join(format!("{MACHINE_FILE}.{}.tmp", std::process::id()));
        let mut f = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&tmp)?;
        f.write_all(id.as_bytes())?;
        f.sync_all()?;
        drop(f);
        let linked = fs::hard_link(&tmp, &path);
        let _ = fs::remove_file(&tmp);
        match linked {
            Ok(()) => return Ok(id),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
}

pub struct Chat {
    machine: String,
    store: Option<Store>,
    write: Mutex<()>,
    subs: Mutex<Vec<SyncSender<ChatPush>>>,
    seq: AtomicU16,
    tails: Mutex<HashMap<String, tail::Tail>>,
}

impl Chat {
    pub fn open(runner_dir: &Path) -> Chat {
        let machine = machine_id(runner_dir).unwrap_or_else(|e| {
            crate::daemon::log(&format!(
                "chat: machine id unavailable ({e}); using a temporary one"
            ));
            random_machine_id()
        });
        let store = Store::open(runner_dir)
            .map_err(|e| crate::daemon::log(&format!("chat: store unavailable: {e}")))
            .ok();
        Chat {
            machine,
            store,
            write: Mutex::new(()),
            subs: Mutex::new(Vec::new()),
            seq: AtomicU16::new(0),
            tails: Mutex::new(HashMap::new()),
        }
    }

    pub fn machine(&self) -> &str {
        &self.machine
    }

    pub fn manager_room(&self) -> RoomRef {
        RoomRef::manager(&self.machine)
    }

    pub fn session_room(&self, session: &str) -> RoomRef {
        RoomRef::session(&self.machine, session)
    }

    fn store(&self) -> Result<&Store, String> {
        self.store
            .as_ref()
            .ok_or_else(|| "the chat store is unavailable".to_string())
    }

    fn own(&self, room: &RoomRef) -> Result<(), String> {
        room.validate()?;
        if room.machine != self.machine {
            return Err("the room belongs to another machine".into());
        }
        Ok(())
    }

    pub fn mint_id(&self) -> String {
        let mut r = [0u8; 10];
        getrandom::fill(&mut r).expect("system random source");
        let n = self.seq.fetch_add(1, Ordering::SeqCst).to_be_bytes();
        r[..2].copy_from_slice(&n);
        message_id_at(now_ms(), r)
    }

    pub fn post(
        &self,
        room: RoomRef,
        author: Author,
        body_md: String,
        reply_to: Option<String>,
        attachments: Vec<Attachment>,
    ) -> Result<ChatMessage, String> {
        self.own(&room)?;
        let msg = ChatMessage {
            v: CHAT_V,
            id: self.mint_id(),
            room,
            author,
            ts_ms: now_ms(),
            kind: MessageKind::Text,
            body_md,
            attachments,
            reply_to,
            decision: None,
        };
        self.append(msg)
    }

    pub fn append(&self, msg: ChatMessage) -> Result<ChatMessage, String> {
        self.own(&msg.room)?;
        {
            let _g = lock(&self.write);
            self.store()?.append(&msg)?;
        }
        let push = ChatPush {
            message: msg.clone(),
        };
        lock(&self.subs).retain(|s| s.try_send(push.clone()).is_ok());
        Ok(msg)
    }

    pub fn owner() -> Author {
        Author {
            kind: AuthorKind::Owner,
            id: "owner".into(),
            machine: None,
            device: None,
            agent: None,
        }
    }

    pub fn subscribe(&self) -> Receiver<ChatPush> {
        let (tx, rx) = mpsc::sync_channel(SUBSCRIBER_QUEUE);
        lock(&self.subs).push(tx);
        rx
    }

    pub fn history(
        &self,
        room: &RoomRef,
        before: Option<&str>,
        limit: Option<u32>,
    ) -> Result<Vec<ChatMessage>, String> {
        self.own(room)?;
        self.store()?.history(room, before, limit)
    }

    pub fn search(
        &self,
        query: &str,
        room: Option<&RoomRef>,
        limit: Option<u32>,
    ) -> Result<Vec<ChatMessage>, String> {
        if let Some(r) = room {
            self.own(r)?;
        }
        self.store()?.search(query, room, limit)
    }

    pub fn rooms(&self, sessions: &[SessionInfo]) -> Vec<RoomInfo> {
        let last = |room: &RoomRef| self.store.as_ref().and_then(|s| s.last(room));
        let manager = self.manager_room();
        let mut out = vec![RoomInfo {
            last: last(&manager),
            room: manager,
            title: "Manager".into(),
            agent: None,
            state: None,
            context_pct: None,
            context_tokens: None,
        }];
        let mut sessions: Vec<&SessionInfo> = sessions.iter().collect();
        sessions.sort_by(|a, b| {
            a.created_ms
                .cmp(&b.created_ms)
                .then_with(|| a.id.cmp(&b.id))
        });
        for s in sessions {
            let room = self.session_room(&s.id);
            if room.validate().is_err() {
                continue;
            }
            out.push(RoomInfo {
                last: last(&room),
                room,
                title: s.name.clone(),
                agent: s.profile.clone(),
                state: Some(state_name(s.state.kind).into()),
                context_pct: None,
                context_tokens: None,
            });
        }
        out
    }
}

fn state_name(kind: StateKind) -> &'static str {
    match kind {
        StateKind::Starting => "starting",
        StateKind::Working => "working",
        StateKind::Waiting => "waiting",
        StateKind::Blocked => "blocked",
        StateKind::Exited => "exited",
    }
}

impl crate::manager::ChatPort for std::sync::Arc<Chat> {
    fn post(&self, msg: ChatMessage) -> Result<ChatMessage, String> {
        self.append(msg)
    }

    fn history(&self, room: &RoomRef, limit: u32) -> Result<Vec<ChatMessage>, String> {
        Chat::history(self, room, None, Some(limit))
    }
}

pub fn prompt_bytes(body: &str) -> Vec<u8> {
    let clean: String = body
        .chars()
        .filter(|c| *c == '\n' || *c == '\t' || !c.is_control())
        .collect();
    format!("\x1b[200~{clean}\x1b[201~\r").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn the_machine_id_is_created_once_private_and_stable() {
        let t = tempfile::tempdir().unwrap();
        let a = machine_id(t.path()).unwrap();
        assert!(valid_machine_id(&a), "{a}");
        assert_eq!(machine_id(t.path()).unwrap(), a);
        let mode = fs::metadata(t.path().join("machine.id"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        let other = tempfile::tempdir().unwrap();
        assert_ne!(machine_id(other.path()).unwrap(), a);
        assert_eq!(fs::read_dir(t.path()).unwrap().count(), 1);
    }

    #[test]
    fn a_malformed_machine_id_is_an_error_not_a_silent_rewrite() {
        let t = tempfile::tempdir().unwrap();
        fs::write(t.path().join("machine.id"), "my-laptop").unwrap();
        assert!(machine_id(t.path()).is_err());
        assert_eq!(
            fs::read_to_string(t.path().join("machine.id")).unwrap(),
            "my-laptop"
        );
    }

    #[test]
    fn a_restarted_chat_keeps_its_machine_and_history() {
        let t = tempfile::tempdir().unwrap();
        let a = Chat::open(t.path());
        let room = a.manager_room();
        a.post(room.clone(), Chat::owner(), "hello".into(), None, vec![])
            .unwrap();
        let b = Chat::open(t.path());
        assert_eq!(b.machine(), a.machine());
        assert_eq!(b.history(&room, None, None).unwrap()[0].body_md, "hello");
    }

    #[test]
    fn rooms_of_another_machine_are_refused() {
        let t = tempfile::tempdir().unwrap();
        let c = Chat::open(t.path());
        let foreign = RoomRef::manager("mx_ffffffffffffffff");
        assert!(c
            .post(foreign.clone(), Chat::owner(), "x".into(), None, vec![])
            .is_err());
        assert!(c.history(&foreign, None, None).is_err());
    }

    #[test]
    fn minted_ids_sort_in_creation_order_and_subscribers_get_every_message() {
        let t = tempfile::tempdir().unwrap();
        let c = Chat::open(t.path());
        let rx = c.subscribe();
        let room = c.session_room("s_abcdefghij");
        let ids: Vec<String> = (0..50)
            .map(|i| {
                c.post(room.clone(), Chat::owner(), format!("m{i}"), None, vec![])
                    .unwrap()
                    .id
            })
            .collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]));
        let pushed: Vec<String> = rx.try_iter().map(|p| p.message.id).collect();
        assert_eq!(pushed, ids);
    }

    #[test]
    fn a_dropped_subscriber_is_forgotten() {
        let t = tempfile::tempdir().unwrap();
        let c = Chat::open(t.path());
        drop(c.subscribe());
        c.post(c.manager_room(), Chat::owner(), "x".into(), None, vec![])
            .unwrap();
        assert!(lock(&c.subs).is_empty());
    }

    #[test]
    fn prompt_bytes_wrap_the_body_in_a_paste_and_drop_escapes() {
        let b = prompt_bytes("a\nb\x1b[201~c\x07");
        assert_eq!(b, b"\x1b[200~a\nb[201~c\x1b[201~\r");
    }
}
