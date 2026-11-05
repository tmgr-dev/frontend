use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::sync::Arc;
use std::time::Duration;

static PENDING: AtomicI32 = AtomicI32::new(0);
const SIGNALS: [libc::c_int; 2] = [libc::SIGTERM, libc::SIGHUP];
const CHECK: Duration = Duration::from_millis(200);
const GRACE: Duration = Duration::from_secs(2);

extern "C" fn on_signal(sig: libc::c_int) {
    PENDING.store(sig, Ordering::SeqCst);
    super::smart::kill_running();
}

pub fn pending() -> i32 {
    PENDING.load(Ordering::SeqCst)
}

fn terminal_gone(fd: libc::c_int) -> bool {
    let mut pfd = libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    };
    let ready = unsafe { libc::poll(&mut pfd, 1, 0) };
    ready > 0 && pfd.revents & (libc::POLLHUP | libc::POLLNVAL) != 0
}

fn exit_now(sig: libc::c_int) -> ! {
    unsafe { libc::_exit(128 + sig) }
}

// crossterm's poll never returns once the terminal reads EOF, so the input loop cannot end the process itself.
fn watch(stop: Arc<AtomicBool>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(CHECK);
        if stop.load(Ordering::SeqCst) {
            return;
        }
        if terminal_gone(libc::STDIN_FILENO) {
            exit_now(libc::SIGHUP);
        }
        let sig = pending();
        if sig != 0 {
            std::thread::sleep(GRACE);
            if !stop.load(Ordering::SeqCst) {
                exit_now(sig);
            }
            return;
        }
    });
}

pub struct Guard {
    previous: [libc::sighandler_t; 2],
    stop: Arc<AtomicBool>,
}

impl Guard {
    pub fn install() -> Guard {
        let mut previous = [libc::SIG_DFL; 2];
        for (slot, sig) in previous.iter_mut().zip(SIGNALS) {
            *slot = unsafe { libc::signal(sig, on_signal as *const () as libc::sighandler_t) };
        }
        let stop = Arc::new(AtomicBool::new(false));
        watch(stop.clone());
        Guard { previous, stop }
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        for (prev, sig) in self.previous.iter().zip(SIGNALS) {
            unsafe { libc::signal(sig, *prev) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_terminal_counts_as_gone_only_after_its_other_end_closes() {
        let (mut master, mut slave) = (-1, -1);
        let opened = unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        assert_eq!(opened, 0);
        assert!(!terminal_gone(slave));
        unsafe { libc::close(master) };
        assert!(terminal_gone(slave));
        unsafe { libc::close(slave) };
    }
}
