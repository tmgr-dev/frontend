use super::attach::Msg;
use super::connect;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Arc, Mutex};
use std::thread;
use tmgr_ipc::chat::{ChatMessage, MentionReq, RoomRef};
use tmgr_runner::attach::HISTORY_LIMIT;
use tmgr_runner::client::ChatCloser;
use tmgr_runner::daemon::RunnerConfig;
use tmgr_runner::sanitize::sanitize_text;

pub(super) struct ChatSub {
    stop: Arc<AtomicBool>,
    closer: Arc<Mutex<Option<ChatCloser>>>,
}

impl ChatSub {
    pub(super) fn start(cfg: &RunnerConfig, tx: SyncSender<Msg>) -> ChatSub {
        let sub = ChatSub {
            stop: Arc::new(AtomicBool::new(false)),
            closer: Arc::new(Mutex::new(None)),
        };
        let cfg = cfg.clone();
        let stop = sub.stop.clone();
        let slot = sub.closer.clone();
        thread::spawn(move || listen(&cfg, &tx, &stop, &slot));
        sub
    }

    pub(super) fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(slot) = self.closer.lock() {
            if let Some(closer) = slot.as_ref() {
                closer.close();
            }
        }
    }
}

fn clean(e: &dyn std::fmt::Display) -> String {
    sanitize_text(&e.to_string()).into_owned()
}

fn listen(
    cfg: &RunnerConfig,
    tx: &SyncSender<Msg>,
    stop: &AtomicBool,
    slot: &Mutex<Option<ChatCloser>>,
) {
    let stream = connect(cfg)
        .map_err(|e| clean(&e))
        .and_then(|c| c.chat_subscribe().map_err(|e| clean(&e)));
    let mut stream = match stream {
        Ok(s) => s,
        Err(e) => {
            if !stop.load(Ordering::SeqCst) {
                let _ = tx.send(Msg::ChatDown(e));
            }
            return;
        }
    };
    if let Ok(closer) = stream.closer() {
        if let Ok(mut s) = slot.lock() {
            *s = Some(closer);
        }
    }
    while !stop.load(Ordering::SeqCst) {
        match stream.recv() {
            Ok(Some(m)) => {
                if stop.load(Ordering::SeqCst) || tx.send(Msg::ChatPush(Box::new(m))).is_err() {
                    return;
                }
            }
            Ok(None) | Err(_) => break,
        }
    }
    if !stop.load(Ordering::SeqCst) {
        let _ = tx.send(Msg::ChatDown("the live stream closed".into()));
    }
}

pub(super) fn load_rooms(cfg: RunnerConfig, tx: SyncSender<Msg>) {
    thread::spawn(move || {
        let result = connect(&cfg)
            .map_err(|e| clean(&e))
            .and_then(|mut c| c.chat_rooms().map_err(|e| clean(&e)));
        let _ = tx.send(Msg::ChatRooms(result));
    });
}

pub(super) fn load_history(
    cfg: RunnerConfig,
    tx: SyncSender<Msg>,
    room: RoomRef,
    before: Option<String>,
) {
    thread::spawn(move || {
        let result = connect(&cfg).map_err(|e| clean(&e)).and_then(|mut c| {
            c.chat_history(&room, before.as_deref(), Some(HISTORY_LIMIT))
                .map_err(|e| clean(&e))
        });
        let _ = tx.send(Msg::ChatHistory {
            room,
            before,
            result,
        });
    });
}

pub(super) fn send(
    cfg: RunnerConfig,
    tx: SyncSender<Msg>,
    room: RoomRef,
    body: String,
    mention: Option<MentionReq>,
) {
    thread::spawn(move || {
        let result: Result<ChatMessage, (bool, String)> = connect(&cfg)
            .map_err(|e| (false, clean(&e)))
            .and_then(|mut c| {
                c.chat_send_with(&room, &body, None, mention)
                    .map_err(|e| (e.to_string().starts_with("busy: "), clean(&e)))
            });
        let _ = tx.send(Msg::ChatSent { room, result });
    });
}

pub(super) fn inject(
    cfg: RunnerConfig,
    tx: SyncSender<Msg>,
    room: RoomRef,
    mention_id: String,
    confirmed_risky: bool,
) {
    thread::spawn(move || {
        let result = connect(&cfg).map_err(|e| clean(&e)).and_then(|mut c| {
            c.chat_mention_inject(&room, &mention_id, confirmed_risky)
                .map_err(|e| clean(&e))
        });
        let _ = tx.send(Msg::ChatInjected {
            room,
            mention_id,
            result,
        });
    });
}
