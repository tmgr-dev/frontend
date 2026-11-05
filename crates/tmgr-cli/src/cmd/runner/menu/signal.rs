use std::sync::atomic::{AtomicI32, Ordering};

static PENDING: AtomicI32 = AtomicI32::new(0);
const SIGNALS: [libc::c_int; 2] = [libc::SIGTERM, libc::SIGHUP];

extern "C" fn on_signal(sig: libc::c_int) {
    PENDING.store(sig, Ordering::SeqCst);
}

pub fn pending() -> i32 {
    PENDING.load(Ordering::SeqCst)
}

pub struct Guard([libc::sighandler_t; 2]);

impl Guard {
    pub fn install() -> Guard {
        let mut previous = [libc::SIG_DFL; 2];
        for (slot, sig) in previous.iter_mut().zip(SIGNALS) {
            *slot = unsafe { libc::signal(sig, on_signal as *const () as libc::sighandler_t) };
        }
        Guard(previous)
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        for (prev, sig) in self.0.iter().zip(SIGNALS) {
            unsafe { libc::signal(sig, *prev) };
        }
    }
}
