use super::status::mask_secrets;
use super::title::{clean_title, clip};
use super::view::{SessionRow, ViewState};
use std::collections::HashMap;
use tmgr_ipc::api::StateKind;

const REASON_MAX: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NeedKind {
    Input,
    Question,
    Permission,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Need {
    pub kind: NeedKind,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SignalSettings {
    pub permission: bool,
    pub question: bool,
    pub input: bool,
    pub quiet: Option<(u16, u16)>,
}

impl Default for SignalSettings {
    fn default() -> Self {
        SignalSettings {
            permission: true,
            question: true,
            input: false,
            quiet: None,
        }
    }
}

impl SignalSettings {
    fn allows(&self, kind: NeedKind) -> bool {
        match kind {
            NeedKind::Permission => self.permission,
            NeedKind::Question => self.question,
            NeedKind::Input => self.input,
        }
    }

    pub fn is_quiet(&self, minute_of_day: u16) -> bool {
        match self.quiet {
            Some((from, to)) if from < to => (from..to).contains(&minute_of_day),
            Some((from, to)) => minute_of_day >= from || minute_of_day < to,
            None => false,
        }
    }
}

pub trait Notifier: Send {
    fn bell(&mut self);
    fn notify(&mut self, text: &str);
}

fn reason(text: &str) -> String {
    clip(&mask_secrets(&clean_title(text)), REASON_MAX)
}

pub fn detect(
    row: &SessionRow,
    ask: Option<&str>,
    open_questions: usize,
    settings: &SignalSettings,
) -> Option<Need> {
    let need = match row.state.kind {
        StateKind::Blocked => {
            let line = row
                .state
                .detail
                .as_deref()
                .map(|d| d.split_once(": ").map_or(d, |(_, line)| line))
                .filter(|l| !l.trim().is_empty())
                .unwrap_or("permission prompt");
            Need {
                kind: NeedKind::Permission,
                reason: reason(line),
            }
        }
        StateKind::Working | StateKind::Waiting if ask.is_some() => Need {
            kind: NeedKind::Question,
            reason: reason(ask.unwrap_or_default()),
        },
        StateKind::Waiting if open_questions > 0 => Need {
            kind: NeedKind::Question,
            reason: match open_questions {
                1 => "asked a question".to_string(),
                n => format!("asked {n} questions"),
            },
        },
        StateKind::Waiting => Need {
            kind: NeedKind::Input,
            reason: "waiting for input".to_string(),
        },
        _ => return None,
    };
    settings.allows(need.kind).then_some(need)
}

#[derive(Default)]
pub struct SignalTracker {
    seen: HashMap<String, Wait>,
    current: HashMap<String, Need>,
    primed: bool,
}

struct Wait {
    since_ms: u64,
    kind: NeedKind,
    open: usize,
}

impl SignalTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current(&self) -> &HashMap<String, Need> {
        &self.current
    }

    pub fn update(&mut self, view: &ViewState, settings: &SignalSettings) -> Vec<(String, Need)> {
        let mut current = HashMap::new();
        let mut fresh = Vec::new();
        let mut alive = Vec::new();
        for row in &view.sessions {
            let ask = view.status.get(&row.id).and_then(|s| s.ask.as_deref());
            let open = view.open_in(&row.id);
            let need = detect(row, ask, open, settings);
            let since = row.state.since_ms;
            if matches!(row.state.kind, StateKind::Waiting | StateKind::Blocked) {
                alive.push(row.id.clone());
            }
            let Some(need) = need else {
                continue;
            };
            let due = match self.seen.get(&row.id).filter(|w| w.since_ms == since) {
                None => true,
                Some(w) => need.kind > w.kind || (need.kind == NeedKind::Question && open > w.open),
            };
            if due {
                fresh.push((row.id.clone(), need.clone()));
            }
            let kind = self
                .seen
                .get(&row.id)
                .filter(|w| w.since_ms == since)
                .map_or(need.kind, |w| w.kind.max(need.kind));
            self.seen.insert(
                row.id.clone(),
                Wait {
                    since_ms: since,
                    kind,
                    open,
                },
            );
            current.insert(row.id.clone(), need);
        }
        self.seen.retain(|id, _| alive.contains(id));
        self.current = current;
        if !self.primed && !view.sessions.is_empty() {
            self.primed = true;
            return Vec::new();
        }
        fresh
    }
}

pub fn deliver(
    fresh: &[(String, Need)],
    name_of: impl Fn(&str) -> String,
    watching: impl Fn(&str) -> bool,
    settings: &SignalSettings,
    minute_of_day: u16,
    notifier: &mut dyn Notifier,
) -> usize {
    if settings.is_quiet(minute_of_day) {
        return 0;
    }
    let due: Vec<(String, &Need)> = fresh
        .iter()
        .filter(|(id, _)| !watching(id))
        .map(|(id, need)| (clip(&clean_title(&name_of(id)), 40), need))
        .collect();
    match due.as_slice() {
        [] => return 0,
        [(name, need)] => {
            notifier.notify(&clean_title(&format!("{name} needs you: {}", need.reason)));
        }
        many => {
            let names: Vec<&str> = many.iter().map(|(n, _)| n.as_str()).collect();
            notifier.notify(&clean_title(&format!(
                "{} sessions need you: {}",
                many.len(),
                names.join(", ")
            )));
        }
    }
    notifier.bell();
    due.len()
}

impl ViewState {
    pub fn next_needing(&self) -> Option<String> {
        let ids: Vec<&str> = self.sessions.iter().map(|r| r.id.as_str()).collect();
        let from = self
            .focused_session()
            .and_then(|f| ids.iter().position(|id| *id == f))
            .map_or(0, |i| i + 1);
        (0..ids.len())
            .map(|k| ids[(from + k) % ids.len()])
            .find(|id| self.needs.contains_key(*id) && self.focused_session() != Some(*id))
            .map(str::to_string)
    }

    pub fn jump_to_needing(&mut self) {
        match self.next_needing() {
            Some(id) => self.open(&id),
            None => self.notice = Some("nothing needs you".into()),
        }
    }
}
