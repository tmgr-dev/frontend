use super::normalize;
use std::collections::{HashMap, VecDeque};
use tmgr_ipc::chat::{MentionState, MentionTarget, RoomRef};

pub const QUEUE_TTL_MS: u64 = 120_000;
pub const ANSWER_TIMEOUT_MS: u64 = 600_000;
pub const HOP: u8 = 1;
pub const HOP_MAX: u8 = 2;
pub const PER_ORIGIN_PER_MIN: usize = 6;
pub const TOTAL_PER_MIN: usize = 20;
pub const REPLY_MAX: usize = 8 << 10;
const WINDOW_MS: u64 = 60_000;
const KEEP_DONE: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Via {
    Local { client: String },
    Relay { device: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub id: String,
    pub origin: RoomRef,
    pub target: RoomRef,
    pub title: String,
    pub hop: u8,
    pub question: String,
    pub via: Via,
    pub confirmed_risky: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Phase {
    Queued,
    Sent(Sent),
    Done(Option<String>),
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Sent {
    typed: String,
    delivered_ms: u64,
    work_base: u64,
    gen: Option<u64>,
    anchored: bool,
    answering: bool,
    waiting_seen: bool,
    replies: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub spec: Spec,
    phase: Phase,
    work_base: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Transition {
    pub spec: Spec,
    pub state: MentionState,
    pub reason: Option<String>,
    pub reply: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Probe {
    pub exited: bool,
    pub waiting: bool,
    pub working_seen: u64,
    pub input_gen: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueEnd {
    Expired,
    Cancelled,
    Refused(String),
    Gone(String),
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Facts {
    pub link_origin: bool,
    pub worker: bool,
    pub exited: bool,
    pub no_reader: bool,
    pub no_tail: bool,
    pub risky: bool,
}

pub fn target_ok(f: &Facts) -> MentionTarget {
    let refuse = |why: &str| MentionTarget {
        ok: false,
        reason: Some(why.into()),
        risky: f.risky,
    };
    if f.exited {
        return refuse("Exited");
    }
    if f.link_origin {
        return refuse("Linked session");
    }
    if f.worker {
        return refuse("Team worker");
    }
    if f.no_reader || f.no_tail {
        return refuse("No transcript");
    }
    MentionTarget {
        ok: true,
        reason: f.risky.then(|| "Needs confirmation".to_string()),
        risky: f.risky,
    }
}

pub fn hop_ok(hop: u8) -> bool {
    (1..=HOP_MAX).contains(&hop)
}

pub fn question_of(body: &str) -> Option<String> {
    let body = body.trim();
    let rest = match body.strip_prefix('@') {
        Some(after) => after.split_once(char::is_whitespace).map_or("", |(_, r)| r),
        None => body,
    };
    let rest = rest.trim();
    (!rest.is_empty()).then(|| rest.to_string())
}

pub fn clip_reply(text: &str) -> String {
    if text.len() <= REPLY_MAX {
        return text.to_string();
    }
    let mut end = REPLY_MAX;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text[..end].to_string()
}

#[derive(Default)]
pub struct Mentions {
    entries: HashMap<String, Entry>,
    order: VecDeque<String>,
    window: VecDeque<(u64, RoomRef)>,
}

fn tr(
    spec: &Spec,
    state: MentionState,
    reason: Option<String>,
    reply: Option<String>,
) -> Transition {
    Transition {
        spec: spec.clone(),
        state,
        reason,
        reply,
    }
}

impl Mentions {
    pub fn admit(&mut self, now: u64, origin: &RoomRef) -> Result<(), MentionState> {
        self.window
            .retain(|(at, _)| now.saturating_sub(*at) < WINDOW_MS);
        let mine = self.window.iter().filter(|(_, o)| o == origin).count();
        if mine >= PER_ORIGIN_PER_MIN || self.window.len() >= TOTAL_PER_MIN {
            return Err(MentionState::RateLimited);
        }
        self.window.push_back((now, origin.clone()));
        Ok(())
    }

    pub fn target_busy(&self, target: &RoomRef) -> bool {
        self.entries
            .values()
            .any(|e| e.spec.target == *target && matches!(e.phase, Phase::Queued | Phase::Sent(_)))
    }

    pub fn open(&mut self, spec: Spec) {
        let phase = Phase::Queued;
        self.order.push_back(spec.id.clone());
        self.entries.insert(
            spec.id.clone(),
            Entry {
                spec,
                phase,
                work_base: 0,
            },
        );
        self.trim();
    }

    fn trim(&mut self) {
        let done = |m: &Mentions, id: &String| {
            m.entries
                .get(id)
                .is_none_or(|e| matches!(e.phase, Phase::Done(_)))
        };
        while self.order.len() > KEEP_DONE {
            let Some(pos) = self.order.iter().position(|id| done(self, id)) else {
                break;
            };
            if let Some(id) = self.order.remove(pos) {
                self.entries.remove(&id);
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.get(id)
    }

    pub fn is_open(&self, id: &str) -> bool {
        self.entries
            .get(id)
            .is_some_and(|e| !matches!(e.phase, Phase::Done(_)))
    }

    pub fn finish(
        &mut self,
        id: &str,
        state: MentionState,
        reason: Option<String>,
    ) -> Option<Transition> {
        let e = self.entries.get_mut(id)?;
        if matches!(e.phase, Phase::Done(_)) {
            return None;
        }
        e.phase = Phase::Done(None);
        Some(tr(&e.spec, state, reason, None))
    }

    pub fn queue_end(&mut self, id: &str, end: QueueEnd) -> Option<Transition> {
        if !matches!(self.entries.get(id)?.phase, Phase::Queued) {
            return None;
        }
        match end {
            QueueEnd::Expired => self.finish(id, MentionState::Busy, None),
            QueueEnd::Cancelled => self.finish(id, MentionState::Cancelled, None),
            QueueEnd::Refused(why) => self.finish(id, MentionState::Refused, Some(why)),
            QueueEnd::Gone(why) => self.finish(id, MentionState::Offline, Some(why)),
        }
    }

    pub fn prepare(&mut self, id: &str, work_base: u64) {
        if let Some(e) = self.entries.get_mut(id) {
            e.work_base = work_base;
        }
    }

    pub fn delivered(&mut self, id: &str, typed: &str, now: u64) -> Option<Transition> {
        let e = self.entries.get_mut(id)?;
        if !matches!(e.phase, Phase::Queued) {
            return None;
        }
        e.phase = Phase::Sent(Sent {
            typed: normalize(typed),
            delivered_ms: now,
            work_base: e.work_base,
            ..Sent::default()
        });
        Some(tr(&e.spec, MentionState::Sent, None, None))
    }

    pub fn set_gen(&mut self, id: &str, gen: u64) {
        if let Some(Entry {
            phase: Phase::Sent(s),
            ..
        }) = self.entries.get_mut(id)
        {
            s.gen = Some(gen);
        }
    }

    pub fn observe(&mut self, target: &RoomRef, user: bool, text: &str) {
        let text = normalize(text);
        for e in self.entries.values_mut() {
            let Phase::Sent(s) = &mut e.phase else {
                continue;
            };
            if e.spec.target != *target {
                continue;
            }
            if user {
                if !s.anchored && s.typed == text {
                    s.anchored = true;
                }
            } else if s.anchored && !text.is_empty() && !s.replies.contains(&text) {
                s.replies.push(text.clone());
            }
        }
    }

    pub fn pass(&mut self, target: &RoomRef, caught_up: bool) -> Vec<Transition> {
        if !caught_up {
            return Vec::new();
        }
        let ids: Vec<String> = self
            .entries
            .iter()
            .filter(|(_, e)| {
                e.spec.target == *target
                    && matches!(&e.phase, Phase::Sent(s)
                        if s.anchored && s.waiting_seen && !s.replies.is_empty())
            })
            .map(|(id, _)| id.clone())
            .collect();
        ids.into_iter().filter_map(|id| self.answer(&id)).collect()
    }

    fn answer(&mut self, id: &str) -> Option<Transition> {
        let e = self.entries.get_mut(id)?;
        let Phase::Sent(s) = &e.phase else {
            return None;
        };
        let text = clip_reply(&s.replies.join("\n\n"));
        e.phase = Phase::Done(Some(text.clone()));
        Some(tr(&e.spec, MentionState::Answered, None, Some(text)))
    }

    pub fn reply(&self, id: &str, origin: &RoomRef) -> Option<(&Spec, &str)> {
        let e = self.entries.get(id)?;
        match &e.phase {
            Phase::Done(Some(text)) if e.spec.origin == *origin => Some((&e.spec, text)),
            _ => None,
        }
    }

    pub fn tick(&mut self, now: u64, probe: &dyn Fn(&RoomRef) -> Option<Probe>) -> Vec<Transition> {
        let mut out = Vec::new();
        let ids: Vec<String> = self.order.iter().cloned().collect();
        for id in ids {
            let Some(e) = self.entries.get_mut(&id) else {
                continue;
            };
            if let Phase::Sent(s) = &mut e.phase {
                {
                    let spec = e.spec.clone();
                    let p = probe(&spec.target);
                    let verdict = match p {
                        None => Some((MentionState::Offline, None)),
                        Some(p) if p.exited => Some((MentionState::Offline, None)),
                        Some(p) if s.gen.is_some_and(|g| p.input_gen > g) => {
                            Some((MentionState::Interrupted, None))
                        }
                        Some(_) if now.saturating_sub(s.delivered_ms) >= ANSWER_TIMEOUT_MS => {
                            Some((MentionState::Timeout, None))
                        }
                        Some(p) => {
                            let worked = p.working_seen > s.work_base;
                            if worked && !s.answering {
                                s.answering = true;
                                out.push(tr(&spec, MentionState::Answering, None, None));
                            }
                            if worked && p.waiting && s.anchored {
                                s.waiting_seen = true;
                            }
                            None
                        }
                    };
                    if let Some((state, reason)) = verdict {
                        out.extend(self.finish(&id, state, reason));
                    }
                }
            }
        }
        out
    }

    pub fn open_specs(&self) -> Vec<Spec> {
        self.order
            .iter()
            .filter_map(|id| self.entries.get(id))
            .filter(|e| !matches!(e.phase, Phase::Done(_)))
            .map(|e| e.spec.clone())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: u64 = 1_800_000_000_000;

    fn room(s: &str) -> RoomRef {
        RoomRef::session("mx_0000000000000001", s)
    }

    fn spec(id: &str, via: Via) -> Spec {
        Spec {
            id: id.into(),
            origin: room("s_origin0001"),
            target: room("s_target0001"),
            title: "build-fix".into(),
            hop: HOP,
            question: "status?".into(),
            via,
            confirmed_risky: false,
        }
    }

    fn local() -> Via {
        Via::Local {
            client: "desktop".into(),
        }
    }

    fn probe(waiting: bool, worked: u64, gen: u64) -> Option<Probe> {
        Some(Probe {
            exited: false,
            waiting,
            working_seen: worked,
            input_gen: gen,
        })
    }

    fn sent(m: &mut Mentions, id: &str) {
        m.open(spec(id, local()));
        m.prepare(id, 5);
        let t = m.delivered(id, "status?", T0 + 10).unwrap();
        assert_eq!(t.state, MentionState::Sent);
        m.set_gen(id, 7);
    }

    #[test]
    fn a_reply_is_collected_after_the_anchor_and_finished_on_waiting_plus_a_caught_up_pass() {
        let mut m = Mentions::default();
        sent(&mut m, "m1");
        m.observe(
            &room("s_target0001"),
            false,
            "stale answer before the anchor",
        );
        let t = m.tick(T0 + 100, &|_| probe(false, 6, 7));
        assert_eq!(t[0].state, MentionState::Answering);
        m.observe(&room("s_target0001"), true, "status?\n");
        m.observe(&room("s_target0001"), false, "all green");
        assert!(m.tick(T0 + 200, &|_| probe(false, 6, 7)).is_empty());
        assert!(m.pass(&room("s_target0001"), true).is_empty());
        assert!(m.tick(T0 + 300, &|_| probe(true, 6, 7)).is_empty());
        assert!(m.pass(&room("s_target0001"), false).is_empty());
        let t = m.pass(&room("s_target0001"), true);
        assert_eq!(t[0].state, MentionState::Answered);
        assert_eq!(t[0].reply.as_deref(), Some("all green"));
        assert!(m.pass(&room("s_target0001"), true).is_empty());
        assert!(m.reply("m1", &room("s_origin0001")).is_some());
        assert!(m.reply("m1", &room("s_other00001")).is_none());
    }

    #[test]
    fn waiting_without_having_worked_after_delivery_does_not_finish() {
        let mut m = Mentions::default();
        sent(&mut m, "m1");
        m.observe(&room("s_target0001"), true, "status?");
        m.observe(
            &room("s_target0001"),
            false,
            "old answer to an earlier turn",
        );
        m.tick(T0 + 100, &|_| probe(true, 5, 7));
        assert!(m.pass(&room("s_target0001"), true).is_empty());
    }

    #[test]
    fn a_short_working_phase_between_polls_is_latched_by_the_counter() {
        let mut m = Mentions::default();
        sent(&mut m, "m1");
        m.observe(&room("s_target0001"), true, "status?");
        m.observe(&room("s_target0001"), false, "done");
        let t = m.tick(T0 + 100, &|_| probe(true, 6, 7));
        assert_eq!(t[0].state, MentionState::Answering);
        assert_eq!(
            m.pass(&room("s_target0001"), true)[0].state,
            MentionState::Answered
        );
    }

    #[test]
    fn typing_into_the_target_after_the_router_interrupts() {
        let mut m = Mentions::default();
        sent(&mut m, "m1");
        let t = m.tick(T0 + 100, &|_| probe(false, 5, 8));
        assert_eq!(t[0].state, MentionState::Interrupted);
        assert!(m.tick(T0 + 200, &|_| probe(false, 5, 9)).is_empty());
    }

    #[test]
    fn the_input_generation_is_not_checked_before_the_router_recorded_its_own_write() {
        let mut m = Mentions::default();
        m.open(spec("m1", local()));
        m.delivered("m1", "status?", T0).unwrap();
        assert!(m.tick(T0 + 1, &|_| probe(false, 0, 50)).is_empty());
    }

    #[test]
    fn no_answer_in_ten_minutes_times_out_and_an_exited_target_is_offline() {
        let mut m = Mentions::default();
        sent(&mut m, "m1");
        let t = m.tick(T0 + 10 + ANSWER_TIMEOUT_MS, &|_| probe(false, 5, 7));
        assert_eq!(t[0].state, MentionState::Timeout);
        sent(&mut m, "m2");
        let t = m.tick(T0 + 100, &|_| {
            Some(Probe {
                exited: true,
                waiting: false,
                working_seen: 5,
                input_gen: 7,
            })
        });
        assert_eq!(t[0].state, MentionState::Offline);
        sent(&mut m, "m3");
        let t = m.tick(T0 + 100, &|_| None);
        assert_eq!(t[0].state, MentionState::Offline);
    }

    #[test]
    fn queue_expiry_cancel_refusal_and_gone_map_to_states_once() {
        let mut m = Mentions::default();
        for (id, end, want) in [
            ("m1", QueueEnd::Expired, MentionState::Busy),
            ("m2", QueueEnd::Cancelled, MentionState::Cancelled),
            ("m3", QueueEnd::Refused("no".into()), MentionState::Refused),
            ("m4", QueueEnd::Gone("gone".into()), MentionState::Offline),
        ] {
            m.open(spec(id, local()));
            assert_eq!(m.queue_end(id, end.clone()).unwrap().state, want);
            assert!(m.queue_end(id, end).is_none());
        }
    }

    #[test]
    fn rate_windows_limit_per_origin_and_in_total_and_slide() {
        let mut m = Mentions::default();
        let a = room("s_origin0001");
        for i in 0..PER_ORIGIN_PER_MIN {
            m.admit(T0 + i as u64, &a).unwrap();
        }
        assert_eq!(m.admit(T0 + 10, &a), Err(MentionState::RateLimited));
        m.admit(T0 + 10, &room("s_origin0002")).unwrap();
        m.admit(T0 + WINDOW_MS + 10, &a).unwrap();
        let mut m = Mentions::default();
        for i in 0..TOTAL_PER_MIN {
            m.admit(T0, &room(&format!("s_origin{i:04}"))).unwrap();
        }
        assert_eq!(
            m.admit(T0, &room("s_originzzzz")),
            Err(MentionState::RateLimited)
        );
    }

    #[test]
    fn a_target_with_a_queued_or_sent_mention_is_busy() {
        let mut m = Mentions::default();
        assert!(!m.target_busy(&room("s_target0001")));
        m.open(spec("m1", local()));
        assert!(m.target_busy(&room("s_target0001")));
        m.finish("m1", MentionState::Busy, None);
        assert!(!m.target_busy(&room("s_target0001")));
    }

    #[test]
    fn hop_over_two_is_refused() {
        assert!(hop_ok(1) && hop_ok(2));
        assert!(!hop_ok(3) && !hop_ok(0));
    }

    #[test]
    fn target_rules_follow_send_to_session() {
        let ok = Facts::default();
        assert!(target_ok(&ok).ok);
        for (f, why) in [
            (Facts { exited: true, ..ok }, "Exited"),
            (
                Facts {
                    link_origin: true,
                    ..ok
                },
                "Linked session",
            ),
            (Facts { worker: true, ..ok }, "Team worker"),
            (
                Facts {
                    no_reader: true,
                    ..ok
                },
                "No transcript",
            ),
            (
                Facts {
                    no_tail: true,
                    ..ok
                },
                "No transcript",
            ),
        ] {
            let t = target_ok(&f);
            assert!(!t.ok);
            assert_eq!(t.reason.as_deref(), Some(why));
        }
        let risky = target_ok(&Facts { risky: true, ..ok });
        assert!(risky.ok && risky.risky);
        assert_eq!(risky.reason.as_deref(), Some("Needs confirmation"));
    }

    #[test]
    fn the_question_drops_the_leading_mention_token_only() {
        assert_eq!(
            question_of("@build-fix what now?").as_deref(),
            Some("what now?")
        );
        assert_eq!(
            question_of("  @Build-Fix\nwhat\nnow").as_deref(),
            Some("what\nnow")
        );
        assert_eq!(question_of("@build-fix"), None);
        assert_eq!(
            question_of("plain question").as_deref(),
            Some("plain question")
        );
        assert_eq!(question_of("hi @x there").as_deref(), Some("hi @x there"));
    }

    #[test]
    fn replies_are_clipped_on_a_char_boundary() {
        let long = "é".repeat(REPLY_MAX);
        let c = clip_reply(&long);
        assert!(c.len() <= REPLY_MAX);
        assert!(long.starts_with(&c));
    }

    #[test]
    fn done_entries_are_trimmed_but_open_ones_are_kept() {
        let mut m = Mentions::default();
        m.open(spec("open", local()));
        for i in 0..KEEP_DONE + 5 {
            let id = format!("d{i}");
            m.open(spec(&id, local()));
            m.finish(&id, MentionState::Busy, None);
        }
        assert!(m.is_open("open"));
        assert!(m.entries.len() <= KEEP_DONE + 1);
    }
}
