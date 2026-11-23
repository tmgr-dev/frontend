use super::lock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use tmgr_ipc::chat::{ActivityState, ChatActivity, RoomRef};

const MAX_ROOMS: usize = 256;

pub(crate) fn blank_idle(room: &RoomRef) -> ChatActivity {
    ChatActivity {
        v: 1,
        room: room.clone(),
        state: ActivityState::Idle,
        current: None,
        used: Vec::new(),
        calls: Vec::new(),
    }
}

fn carries_nothing(e: &ChatActivity) -> bool {
    e.state == ActivityState::Idle && e.current.is_none() && e.used.is_empty() && e.calls.is_empty()
}

pub(super) struct Slot {
    latest: Mutex<Vec<ChatActivity>>,
    wake: Condvar,
    on: Arc<AtomicBool>,
}

impl Slot {
    pub(super) fn new(on: Arc<AtomicBool>) -> Slot {
        Slot {
            latest: Mutex::new(Vec::new()),
            wake: Condvar::new(),
            on,
        }
    }

    pub(super) fn put(&self, event: &ChatActivity) {
        if !self.on.load(Ordering::SeqCst) {
            return;
        }
        self.store(event);
    }

    pub(super) fn put_blank_idle(&self, room: &RoomRef) {
        self.store(&blank_idle(room));
    }

    fn store(&self, event: &ChatActivity) {
        let mut latest = lock(&self.latest);
        latest.retain(|e| e.room != event.room);
        if latest.len() >= MAX_ROOMS {
            latest.remove(0);
        }
        latest.push(event.clone());
        self.wake.notify_one();
    }

    pub(super) fn clear(&self) {
        lock(&self.latest).retain(carries_nothing);
    }

    pub(super) fn forget(&self, room: &RoomRef) {
        lock(&self.latest).retain(|e| e.room != *room);
    }
}

pub struct ActivityRx(pub(super) Arc<Slot>);

impl ActivityRx {
    pub fn take(&self, wait: Duration) -> Vec<ChatActivity> {
        let mut latest = lock(&self.0.latest);
        if latest.is_empty() && !wait.is_zero() {
            latest = self
                .0
                .wake
                .wait_timeout(latest, wait)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
        let mut taken = std::mem::take(&mut *latest);
        if !self.0.on.load(Ordering::SeqCst) {
            taken.retain(carries_nothing);
        }
        taken
    }
}
