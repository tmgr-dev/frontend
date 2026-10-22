use super::signal::{detect, NeedKind, SignalSettings};
use super::view::ViewState;
use std::collections::{HashMap, HashSet};
use tmgr_ipc::api::StateKind;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PushKind {
    LongTurn,
    Error,
    Blocked,
}

impl PushKind {
    pub fn label(self) -> &'static str {
        match self {
            PushKind::LongTurn => "turn",
            PushKind::Error => "error",
            PushKind::Blocked => "blocked",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushSettings {
    pub long_min: Option<u32>,
    pub error: bool,
    pub blocked_min: Option<u32>,
    pub quiet: Option<(u16, u16)>,
    pub debounce_min: u32,
}

impl Default for PushSettings {
    fn default() -> Self {
        PushSettings {
            long_min: None,
            error: false,
            blocked_min: None,
            quiet: None,
            debounce_min: 10,
        }
    }
}

impl PushSettings {
    pub fn enabled(&self) -> bool {
        self.long_min.is_some() || self.error || self.blocked_min.is_some()
    }

    fn is_quiet(&self, minute: u16) -> bool {
        SignalSettings {
            quiet: self.quiet,
            ..SignalSettings::default()
        }
        .is_quiet(minute)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushEvent {
    pub session: String,
    pub kind: PushKind,
    pub reason: String,
    pub key: String,
}

struct Seen {
    kind: StateKind,
    working_since: Option<u64>,
}

#[derive(Default)]
pub struct PushTracker {
    seen: HashMap<String, Seen>,
    last: HashMap<(String, PushKind), u64>,
    fired: HashSet<(String, u64)>,
    asks: HashMap<String, (String, u64)>,
}

fn mins(ms: u64) -> u64 {
    ms / 60_000
}

impl PushTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn update(
        &mut self,
        view: &ViewState,
        s: &PushSettings,
        now_ms: u64,
        minute: u16,
        user_stopped: &dyn Fn(&str) -> bool,
    ) -> Vec<PushEvent> {
        let mut out = Vec::new();
        let any = SignalSettings {
            permission: true,
            question: true,
            input: false,
            quiet: None,
        };
        for row in &view.sessions {
            let id = row.id.as_str();
            let since = row.state.since_ms;
            let prev = self.seen.get(id);
            let mut candidates: Vec<(PushKind, String, u64)> = Vec::new();
            match row.state.kind {
                StateKind::Waiting => {
                    if let (Some(p), Some(min)) = (prev, s.long_min) {
                        if matches!(p.kind, StateKind::Working | StateKind::Blocked) {
                            if let Some(start) = p.working_since {
                                let ran = since.saturating_sub(start);
                                if ran >= u64::from(min) * 60_000 {
                                    candidates.push((
                                        PushKind::LongTurn,
                                        format!("finished a turn after {} min", mins(ran)),
                                        since,
                                    ));
                                }
                            }
                        }
                    }
                }
                StateKind::Exited => {
                    if let Some(p) = prev {
                        if s.error
                            && p.kind != StateKind::Exited
                            && !user_stopped(id)
                            && row.state.exit_code.is_some_and(|c| (1..128).contains(&c))
                        {
                            let code = row.state.exit_code.unwrap_or_default();
                            candidates.push((
                                PushKind::Error,
                                format!("exited with code {code}"),
                                since,
                            ));
                        }
                    }
                }
                _ => {}
            }
            if let Some(min) = s.blocked_min {
                let ask = view.status.get(id).and_then(|st| st.ask.as_deref());
                if let Some(need) = detect(row, ask, view.open_in(id), &any) {
                    let mut started = since;
                    if need.kind == NeedKind::Question {
                        if let Some(text) = ask {
                            let entry = self
                                .asks
                                .entry(id.to_string())
                                .or_insert((text.to_string(), now_ms));
                            if entry.0 != text {
                                *entry = (text.to_string(), now_ms);
                            }
                            started = entry.1;
                        }
                    }
                    let waited = now_ms.saturating_sub(started);
                    if need.kind != NeedKind::Input && waited >= u64::from(min) * 60_000 {
                        let what = match need.kind {
                            NeedKind::Permission => "is waiting for permission",
                            _ => "asked a question",
                        };
                        candidates.push((
                            PushKind::Blocked,
                            format!("{what}, {} min ago", mins(waited)),
                            started,
                        ));
                    }
                }
            }
            for (kind, reason, at) in candidates {
                let key = (id.to_string(), at);
                if self.fired.contains(&key) && kind == PushKind::Blocked {
                    continue;
                }
                if s.is_quiet(minute) {
                    continue;
                }
                let recent = self.last.get(&(id.to_string(), kind)).is_some_and(|t| {
                    now_ms.saturating_sub(*t) < u64::from(s.debounce_min) * 60_000
                });
                if recent {
                    continue;
                }
                if kind == PushKind::Blocked {
                    self.fired.insert(key);
                }
                self.last.insert((id.to_string(), kind), now_ms);
                out.push(PushEvent {
                    session: id.to_string(),
                    kind,
                    reason,
                    key: format!("{id}-{}-{at}", kind.label()),
                });
            }
            let working_since = match (row.state.kind, prev) {
                (StateKind::Working | StateKind::Blocked, Some(p))
                    if matches!(p.kind, StateKind::Working | StateKind::Blocked) =>
                {
                    p.working_since
                }
                (StateKind::Working, _) => Some(since),
                _ => None,
            };
            self.seen.insert(
                id.to_string(),
                Seen {
                    kind: row.state.kind,
                    working_since,
                },
            );
        }
        let alive: HashSet<&str> = view.sessions.iter().map(|r| r.id.as_str()).collect();
        self.seen.retain(|id, _| alive.contains(id.as_str()));
        self.last.retain(|(id, _), _| alive.contains(id.as_str()));
        self.fired.retain(|(id, _)| alive.contains(id.as_str()));
        self.asks.retain(|id, _| alive.contains(id.as_str()));
        out
    }
}
