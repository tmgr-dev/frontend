use tmgr_ipc::api::{State, StateKind};

pub const DEFAULT_QUIET_MS: u64 = 5000;
const QUIET_MIN: u64 = 100;
const QUIET_MAX: u64 = 600_000;

fn parse_quiet(raw: Option<&str>) -> u64 {
    raw.and_then(|v| v.trim().parse::<u64>().ok())
        .map_or(DEFAULT_QUIET_MS, |v| v.clamp(QUIET_MIN, QUIET_MAX))
}

pub fn quiet_ms_from_env() -> u64 {
    parse_quiet(std::env::var("TMGR_T0_QUIET_MS").ok().as_deref())
}

pub struct Detector {
    quiet_ms: u64,
    state: State,
    last_output: Option<u64>,
    last_input: Option<u64>,
    bell: bool,
    blocked: Option<String>,
    tty_waiting: Option<bool>,
    cpu_samples: Vec<(u64, u64)>,
    exited: bool,
}

impl Detector {
    pub fn new(quiet_ms: u64, now_ms: u64) -> Self {
        Self {
            quiet_ms,
            state: State {
                kind: StateKind::Starting,
                since_ms: now_ms,
                detail: None,
                exit_code: None,
            },
            last_output: None,
            last_input: None,
            bell: false,
            blocked: None,
            tty_waiting: None,
            cpu_samples: Vec::new(),
            exited: false,
        }
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn on_output(&mut self, now: u64) -> Option<State> {
        self.last_output = Some(now);
        self.bell = false;
        self.tick(now)
    }

    pub fn on_input(&mut self, now: u64) -> Option<State> {
        self.last_input = Some(now);
        self.tick(now)
    }

    pub fn on_bell(&mut self, now: u64) -> Option<State> {
        self.bell = true;
        self.tick(now)
    }

    pub fn on_stat(
        &mut self,
        now: u64,
        fg_cpu_ns: Option<u64>,
        fg_waiting_tty: Option<bool>,
    ) -> Option<State> {
        self.tty_waiting = fg_waiting_tty;
        if let Some(cpu) = fg_cpu_ns {
            self.cpu_samples.push((now, cpu));
        }
        let floor = now.saturating_sub(self.quiet_ms);
        self.cpu_samples.retain(|(t, _)| *t >= floor);
        self.tick(now)
    }

    pub fn on_blocked(&mut self, now: u64, rule_id: &str, line: &str) -> Option<State> {
        self.blocked = Some(format!("{rule_id}: {line}"));
        self.tick(now)
    }

    pub fn on_unblocked(&mut self, now: u64) -> Option<State> {
        self.blocked = None;
        self.tick(now)
    }

    pub fn on_exit(&mut self, now: u64, code: Option<i32>, signal: Option<i32>) -> Option<State> {
        if self.exited {
            return None;
        }
        self.exited = true;
        let exit_code = code.or(signal.map(|s| 128 + s));
        let detail = if code == Some(0) { "done" } else { "failed" };
        self.set(StateKind::Exited, Some(detail.to_string()), exit_code, now)
    }

    pub fn tick(&mut self, now: u64) -> Option<State> {
        if self.exited {
            return None;
        }
        let (kind, detail) = self.desired(now);
        self.set(kind, detail, None, now)
    }

    fn set(
        &mut self,
        kind: StateKind,
        detail: Option<String>,
        exit_code: Option<i32>,
        now: u64,
    ) -> Option<State> {
        if self.state.kind == kind && self.state.detail == detail {
            return None;
        }
        self.state = State {
            kind,
            since_ms: now,
            detail,
            exit_code,
        };
        Some(self.state.clone())
    }

    fn desired(&self, now: u64) -> (StateKind, Option<String>) {
        if let Some(d) = &self.blocked {
            return (StateKind::Blocked, Some(d.clone()));
        }
        if self.bell {
            return (StateKind::Waiting, Some("bell".to_string()));
        }
        let Some(out) = self.last_output else {
            return (StateKind::Starting, None);
        };
        let activity = self.last_input.map_or(out, |i| i.max(out));
        if now.saturating_sub(activity) < self.quiet_ms {
            return (StateKind::Working, None);
        }
        let waiting = match self.tty_waiting {
            Some(w) => w,
            None => !self.cpu_growing(now),
        };
        if waiting {
            (StateKind::Waiting, None)
        } else {
            (StateKind::Working, None)
        }
    }

    fn cpu_growing(&self, now: u64) -> bool {
        let floor = now.saturating_sub(self.quiet_ms);
        let mut window = self.cpu_samples.iter().filter(|(t, _)| *t >= floor);
        match (window.next(), window.next_back()) {
            (Some((_, first)), Some((_, last))) => last > first,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(d: &Detector) -> StateKind {
        d.state().kind
    }

    #[test]
    fn scripted_working_waiting_working_exited() {
        let mut d = Detector::new(5000, 0);
        assert_eq!(kinds(&d), StateKind::Starting);
        assert_eq!(d.tick(4000), None);

        let s = d.on_output(100).unwrap();
        assert_eq!((s.kind, s.since_ms), (StateKind::Working, 100));
        assert_eq!(d.tick(5000), None);

        let s = d.tick(5100).unwrap();
        assert_eq!((s.kind, s.since_ms), (StateKind::Waiting, 5100));
        assert_eq!(d.tick(9000), None);

        let s = d.on_output(9500).unwrap();
        assert_eq!(s.kind, StateKind::Working);

        let s = d.on_exit(9600, Some(0), None).unwrap();
        assert_eq!(s.kind, StateKind::Exited);
        assert_eq!(s.detail.as_deref(), Some("done"));
        assert_eq!(s.exit_code, Some(0));
        assert_eq!(d.tick(99999), None);
        assert_eq!(d.on_output(100000), None);
        assert_eq!(kinds(&d), StateKind::Exited);
    }

    #[test]
    fn failed_exit_codes() {
        let mut d = Detector::new(1000, 0);
        let s = d.on_exit(1, Some(3), None).unwrap();
        assert_eq!(
            (s.detail.as_deref(), s.exit_code),
            (Some("failed"), Some(3))
        );
        let mut d = Detector::new(1000, 0);
        let s = d.on_exit(1, None, Some(9)).unwrap();
        assert_eq!(
            (s.detail.as_deref(), s.exit_code),
            (Some("failed"), Some(137))
        );
    }

    #[test]
    fn bell_waits_until_next_output() {
        let mut d = Detector::new(5000, 0);
        d.on_output(10);
        let s = d.on_bell(20).unwrap();
        assert_eq!(s.kind, StateKind::Waiting);
        assert_eq!(s.detail.as_deref(), Some("bell"));
        assert_eq!(d.on_input(30), None);
        let s = d.on_output(40).unwrap();
        assert_eq!(s.kind, StateKind::Working);
    }

    #[test]
    fn blocked_overrides_and_restores() {
        let mut d = Detector::new(5000, 0);
        d.on_output(10);
        let s = d
            .on_blocked(20, "blocked[0]", "Do you want to proceed?")
            .unwrap();
        assert_eq!(s.kind, StateKind::Blocked);
        assert_eq!(
            s.detail.as_deref(),
            Some("blocked[0]: Do you want to proceed?")
        );
        assert_eq!(d.on_output(30), None);
        assert_eq!(d.tick(20000), None);
        let s = d.on_unblocked(20001).unwrap();
        assert_eq!(s.kind, StateKind::Waiting);
    }

    #[test]
    fn tty_wait_hint_decides() {
        let mut d = Detector::new(1000, 0);
        d.on_output(0);
        assert_eq!(d.on_stat(1500, None, Some(false)), None);
        assert_eq!(kinds(&d), StateKind::Working);
        let s = d.on_stat(1600, None, Some(true)).unwrap();
        assert_eq!(s.kind, StateKind::Waiting);
    }

    #[test]
    fn cpu_growth_keeps_working() {
        let mut d = Detector::new(1000, 0);
        d.on_output(0);
        d.on_stat(1100, Some(100), None);
        d.on_stat(1500, Some(900), None);
        assert_eq!(kinds(&d), StateKind::Working);
        d.on_stat(2000, Some(900), None);
        d.on_stat(2600, Some(900), None);
        assert_eq!(kinds(&d), StateKind::Waiting);
    }

    #[test]
    fn input_resets_quiet_window() {
        let mut d = Detector::new(1000, 0);
        d.on_output(0);
        d.tick(1500);
        assert_eq!(kinds(&d), StateKind::Waiting);
        let s = d.on_input(1600).unwrap();
        assert_eq!(s.kind, StateKind::Working);
    }

    #[test]
    fn quiet_env_bounds() {
        assert_eq!(parse_quiet(None), 5000);
        assert_eq!(parse_quiet(Some("junk")), 5000);
        assert_eq!(parse_quiet(Some("1")), 100);
        assert_eq!(parse_quiet(Some("99999999")), 600_000);
        assert_eq!(parse_quiet(Some(" 2500 ")), 2500);
    }

    #[test]
    fn state_serde_snake_case() {
        let s = serde_json::to_string(&StateKind::Waiting).unwrap();
        assert_eq!(s, "\"waiting\"");
    }
}
