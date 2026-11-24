use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use tmgr_chat_relay::engine::retry_after_secs;

pub const PERIOD: Duration = Duration::from_secs(15);
pub const CAP: Duration = Duration::from_secs(300);
const TICK: Duration = Duration::from_millis(250);

fn retry_hint(e: &str) -> Option<u64> {
    retry_after_secs(e).or_else(|| {
        serde_json::from_str::<serde_json::Value>(e)
            .ok()?
            .get("retry_after")?
            .as_u64()
    })
}

pub fn next_wait(fails: u32, err: Option<&str>) -> Duration {
    if fails == 0 {
        return PERIOD;
    }
    if let Some(secs) = err.and_then(retry_hint) {
        return Duration::from_secs(secs).clamp(PERIOD, CAP);
    }
    PERIOD
        .saturating_mul(1u32.checked_shl(fails - 1).unwrap_or(u32::MAX))
        .min(CAP)
}

pub fn run(
    stop: &AtomicBool,
    step: &mut dyn FnMut() -> Result<(), String>,
    sleep: &dyn Fn(Duration),
) {
    let mut fails = 0u32;
    while !stop.load(Ordering::SeqCst) {
        let wait = match step() {
            Ok(()) => {
                fails = 0;
                next_wait(0, None)
            }
            Err(e) => {
                fails = fails.saturating_add(1);
                next_wait(fails, Some(&e))
            }
        };
        if stop.load(Ordering::SeqCst) {
            break;
        }
        sleep(wait);
    }
}

pub fn sleep_unless_stopped(stop: &AtomicBool, total: Duration) {
    let mut left = total;
    while !left.is_zero() && !stop.load(Ordering::SeqCst) {
        let d = left.min(TICK);
        std::thread::sleep(d);
        left -= d;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn a_healthy_poll_runs_every_period_which_keeps_the_device_inside_the_owner_idle_window() {
        assert_eq!(next_wait(0, None), PERIOD);
        assert!(PERIOD * 4 < Duration::from_secs(3 * 60));
    }

    #[test]
    fn failures_back_off_exponentially_up_to_the_cap() {
        let waits: Vec<u64> = (1..=7)
            .map(|n| next_wait(n, Some("boom")).as_secs())
            .collect();
        assert_eq!(waits, [15, 30, 60, 120, 240, 300, 300]);
        assert_eq!(next_wait(u32::MAX, Some("boom")), CAP);
    }

    #[test]
    fn a_429_retry_after_wins_within_bounds() {
        let e = r#"{"error":"rate_limited","retry_after":90}"#;
        assert_eq!(next_wait(1, Some(e)).as_secs(), 90);
        assert_eq!(next_wait(5, Some(e)).as_secs(), 90);
        assert_eq!(next_wait(1, Some(r#"{"retry_after":1}"#)), PERIOD);
        assert_eq!(next_wait(1, Some(r#"{"retry_after":99999}"#)), CAP);
        assert_eq!(
            next_wait(1, Some("relay is busy, retry in 45s")).as_secs(),
            45
        );
    }

    #[test]
    fn the_loop_backs_off_resets_on_success_and_stops_on_request() {
        let stop = AtomicBool::new(false);
        let mut script = vec![
            Err("x".to_owned()),
            Err(r#"{"retry_after":100}"#.to_owned()),
            Ok(()),
            Err("y".to_owned()),
        ]
        .into_iter();
        let waits = RefCell::new(Vec::new());
        let mut step = || script.next().unwrap_or(Ok(()));
        let sleep = |d: Duration| {
            waits.borrow_mut().push(d.as_secs());
            if waits.borrow().len() == 4 {
                stop.store(true, Ordering::SeqCst);
            }
        };
        run(&stop, &mut step, &sleep);
        assert_eq!(*waits.borrow(), [15, 100, 15, 15]);
    }

    #[test]
    fn a_stop_before_the_first_step_runs_nothing() {
        let stop = AtomicBool::new(true);
        let mut called = false;
        run(
            &stop,
            &mut || {
                called = true;
                Ok(())
            },
            &|_| {},
        );
        assert!(!called);
    }
}
