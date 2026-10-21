use std::time::{Duration, Instant};

const HEARTBEAT: Duration = Duration::from_secs(5);

#[derive(Default)]
pub struct AttentionTracker {
    last: Option<(Option<String>, bool)>,
    sent_at: Option<Instant>,
}

impl AttentionTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(
        &mut self,
        now: Instant,
        session: Option<&str>,
        window: bool,
        user_action: bool,
    ) -> Option<(Option<String>, bool)> {
        let changed = match &self.last {
            Some((s, w)) => s.as_deref() != session || *w != window,
            None => true,
        };
        let due = user_action
            && self
                .sent_at
                .is_none_or(|t| now.saturating_duration_since(t) >= HEARTBEAT);
        if !changed && !due {
            return None;
        }
        let state = (session.map(str::to_string), window);
        self.last = Some(state.clone());
        self.sent_at = Some(now);
        Some(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t: Instant, s: u64) -> Instant {
        t + Duration::from_secs(s)
    }

    #[test]
    fn the_first_call_reports_and_an_unchanged_state_stays_quiet() {
        let t = Instant::now();
        let mut a = AttentionTracker::new();
        assert_eq!(
            a.update(t, Some("s1"), true, false),
            Some((Some("s1".into()), true))
        );
        assert_eq!(a.update(at(t, 60), Some("s1"), true, false), None);
    }

    #[test]
    fn a_change_of_session_or_window_reports_at_once() {
        let t = Instant::now();
        let mut a = AttentionTracker::new();
        a.update(t, Some("s1"), true, false);
        assert_eq!(
            a.update(at(t, 1), Some("s2"), true, false),
            Some((Some("s2".into()), true))
        );
        assert_eq!(
            a.update(at(t, 1), Some("s2"), false, false),
            Some((Some("s2".into()), false))
        );
        assert_eq!(a.update(at(t, 1), None, false, false), Some((None, false)));
    }

    #[test]
    fn user_actions_are_throttled_to_one_per_five_seconds() {
        let t = Instant::now();
        let mut a = AttentionTracker::new();
        a.update(t, Some("s1"), true, false);
        assert_eq!(a.update(at(t, 2), Some("s1"), true, true), None);
        assert!(a.update(at(t, 5), Some("s1"), true, true).is_some());
        assert_eq!(a.update(at(t, 9), Some("s1"), true, true), None);
        assert!(a.update(at(t, 10), Some("s1"), true, true).is_some());
    }

    #[test]
    fn a_change_resets_the_heartbeat_window() {
        let t = Instant::now();
        let mut a = AttentionTracker::new();
        a.update(t, Some("s1"), true, false);
        a.update(at(t, 4), Some("s2"), true, false);
        assert_eq!(a.update(at(t, 6), Some("s2"), true, true), None);
        assert!(a.update(at(t, 9), Some("s2"), true, true).is_some());
    }
}
