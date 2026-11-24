use super::attach::Msg;
use super::attach_device::{self, poll, Paired};
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

pub(super) fn open_device() -> Option<Arc<Paired>> {
    open_in(
        tmgr_link::privileged::in_agent_session(),
        &tmgr_ipc::dirs::runner_dir().ok()?,
    )
}

pub(super) fn open_in(agent_session: bool, dir: &std::path::Path) -> Option<Arc<Paired>> {
    if agent_session {
        return None;
    }
    let machine = tmgr_runner::chat::machine_id(dir).ok()?;
    attach_device::open(dir, &machine)
        .ok()
        .flatten()
        .map(Arc::new)
}

pub(super) struct DevicePoller {
    stop: Arc<AtomicBool>,
}

impl DevicePoller {
    pub(super) fn start(dev: Arc<Paired>, cfg: RunnerConfig, tx: SyncSender<Msg>) -> DevicePoller {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        thread::spawn(move || {
            let mut shape = (usize::MAX, usize::MAX);
            let mut step = || {
                let fresh = dev.poll()?;
                for m in fresh {
                    if tx.send(Msg::ChatPush(Box::new(m))).is_err() {
                        flag.store(true, Ordering::SeqCst);
                    }
                }
                for note in dev.receipt_notes() {
                    let _ = tx.send(Msg::ChatReceipt(clean(&note)));
                }
                let now = (dev.rooms().len(), dev.machines().len());
                if now != shape {
                    shape = now;
                    load_rooms(cfg.clone(), tx.clone(), Some(dev.clone()));
                }
                Ok(())
            };
            poll::run(&flag.clone(), &mut step, &|d| {
                poll::sleep_unless_stopped(&flag, d)
            });
        });
        DevicePoller { stop }
    }
}

impl Drop for DevicePoller {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

pub(super) fn load_rooms(cfg: RunnerConfig, tx: SyncSender<Msg>, device: Option<Arc<Paired>>) {
    thread::spawn(move || {
        let result = connect(&cfg)
            .map_err(|e| clean(&e))
            .and_then(|mut c| c.chat_rooms().map_err(|e| clean(&e)));
        let result = match (&device, result) {
            (Some(dev), Ok(rooms)) => {
                let _ = tx.send(Msg::ChatMachines(dev.machines().into_iter().collect()));
                Ok(dev.merge(rooms))
            }
            (_, result) => result,
        };
        let _ = tx.send(Msg::ChatRooms(result));
    });
}

pub(super) fn load_history(
    cfg: RunnerConfig,
    tx: SyncSender<Msg>,
    room: RoomRef,
    before: Option<String>,
    device: Option<Arc<Paired>>,
) {
    if let Some(dev) = device.filter(|d| d.handles(&room)) {
        thread::spawn(move || {
            let result = Ok(if before.is_some() {
                Vec::new()
            } else {
                dev.history(&room, HISTORY_LIMIT as usize)
            });
            let _ = tx.send(Msg::ChatHistory {
                room,
                before,
                result,
            });
        });
        return;
    }
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
    device: Option<Arc<Paired>>,
) {
    if let Some(dev) = device.filter(|d| d.handles(&room)) {
        thread::spawn(move || {
            let result = if mention.is_some() {
                Err((
                    false,
                    "mentions are not supported in rooms of other machines".to_owned(),
                ))
            } else {
                dev.send(&room, &body).map_err(|e| (false, clean(&e)))
            };
            let _ = tx.send(Msg::ChatSent { room, result });
        });
        return;
    }
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
