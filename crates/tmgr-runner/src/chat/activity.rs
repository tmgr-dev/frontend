use super::lock;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;
use tmgr_ipc::chat::ChatActivity;

const MAX_ROOMS: usize = 256;

#[derive(Default)]
pub(super) struct Slot {
    latest: Mutex<Vec<ChatActivity>>,
    wake: Condvar,
}

impl Slot {
    pub(super) fn put(&self, event: &ChatActivity) {
        let mut latest = lock(&self.latest);
        match latest.iter_mut().find(|e| e.room == event.room) {
            Some(old) => *old = event.clone(),
            None => {
                if latest.len() >= MAX_ROOMS {
                    latest.remove(0);
                }
                latest.push(event.clone());
            }
        }
        self.wake.notify_one();
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
        std::mem::take(&mut *latest)
    }
}
