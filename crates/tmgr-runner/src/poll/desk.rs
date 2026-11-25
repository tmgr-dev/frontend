use super::{advance, matches_card, sanitize_other, Next, Screen};
use crate::attach::status::{AskPoll, AskQuestion};
use crate::permit::digest;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};
use tmgr_ipc::chat::{PollAnswer, PollMsg, PollOption, PollPick, PollQuestion};

pub const MAX_OPEN: usize = 32;
const CLOSED_RING: usize = 128;
const MAX_STEPS: usize = 64;
const PICKED_MAX: usize = 300;
const RECOMMENDED: &str = "(Recommended)";
pub const WHY_NOT: &str = "open the pane to answer";
const HEADER_MAX: usize = 60;
const QUESTION_MAX: usize = 500;
const LABEL_MAX: usize = 200;
const DESCRIPTION_MAX: usize = 500;

pub fn card_id(session: &str, tool_use_id: &str) -> String {
    format!("poll_{}", &digest(&[session, tool_use_id])[..32])
}

fn clip(s: &str, max: usize) -> String {
    let mut end = s.len().min(max);
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    s[..end].to_string()
}

pub fn shown_label(raw: &str) -> (String, bool) {
    match raw.trim_end().strip_suffix(RECOMMENDED) {
        Some(rest) if !rest.trim().is_empty() => (rest.trim_end().to_string(), true),
        _ => (raw.to_string(), false),
    }
}

fn poll_questions(questions: &[AskQuestion]) -> Vec<PollQuestion> {
    questions
        .iter()
        .map(|q| PollQuestion {
            header: clip(&q.header, HEADER_MAX),
            question: clip(&q.question, QUESTION_MAX),
            multi_select: q.multi_select,
            options: q
                .options
                .iter()
                .map(|o| {
                    let (label, recommended) = shown_label(&o.label);
                    PollOption {
                        label: clip(&label, LABEL_MAX),
                        description: clip(&o.description, DESCRIPTION_MAX),
                        recommended,
                    }
                })
                .collect(),
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Post {
        card_id: String,
        poll: PollMsg,
    },
    Close {
        card_id: String,
        session: String,
        closed: &'static str,
        picked: Option<String>,
        why: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    Applied { picked: String },
    Stale,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reject {
    Unknown,
    Duplicate,
    NotAnswerable,
    Invalid(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answering {
    pub card_id: String,
    pub session: String,
    pub questions: Vec<AskQuestion>,
    pub picks: Vec<PollPick>,
}

#[derive(Debug, PartialEq, Eq)]
enum State {
    Open,
    Answering(String),
}

#[derive(Debug)]
struct Card {
    session: String,
    questions: Vec<AskQuestion>,
    state: State,
    posted_answerable: bool,
}

#[derive(Default)]
pub struct PollDesk {
    cards: HashMap<String, Card>,
    pub(super) closed: VecDeque<String>,
}

fn answerable(questions: &[AskQuestion], screen: Option<&Screen>, foreground: bool) -> bool {
    foreground
        && matches!(screen, Some(Screen::Picker(p)) if questions.iter().any(|q| matches_card(p, q)))
}

fn post(card_id: &str, c: &Card) -> Action {
    Action::Post {
        card_id: card_id.to_string(),
        poll: PollMsg {
            card_id: card_id.to_string(),
            session: c.session.clone(),
            questions: poll_questions(&c.questions),
            closed: None,
            picked: None,
            can_answer: c.posted_answerable,
            why_not: (!c.posted_answerable).then(|| WHY_NOT.to_string()),
        },
    }
}

impl PollDesk {
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }

    pub fn sessions(&self) -> Vec<String> {
        let mut s: Vec<String> = self.cards.values().map(|c| c.session.clone()).collect();
        s.sort();
        s.dedup();
        s
    }

    fn close(&mut self, id: &str, closed: &'static str) -> Option<Action> {
        let c = self.cards.remove(id)?;
        self.closed.push_back(id.to_string());
        while self.closed.len() > CLOSED_RING {
            self.closed.pop_front();
        }
        Some(Action::Close {
            card_id: id.to_string(),
            session: c.session,
            closed,
            picked: None,
            why: None,
        })
    }

    pub fn observe(
        &mut self,
        session: &str,
        poll: Option<&AskPoll>,
        screen: Option<&Screen>,
        foreground: bool,
    ) -> Vec<Action> {
        let live = poll.map(|p| card_id(session, &p.tool_use_id));
        let vanished: Vec<String> = self
            .cards
            .iter()
            .filter(|(id, c)| {
                c.session == session && c.state == State::Open && Some(*id) != live.as_ref()
            })
            .map(|(id, _)| id.clone())
            .collect();
        let mut out: Vec<Action> = vanished
            .iter()
            .filter_map(|id| self.close(id, "terminal"))
            .collect();
        let (Some(p), Some(id)) = (poll, live) else {
            return out;
        };
        if self.closed.contains(&id) {
            return out;
        }
        let can = answerable(&p.questions, screen, foreground);
        match self.cards.get_mut(&id) {
            Some(c) => {
                if c.state == State::Open && can && !c.posted_answerable {
                    c.posted_answerable = true;
                    out.push(post(&id, c));
                }
            }
            None => {
                if self.cards.len() < MAX_OPEN {
                    let c = Card {
                        session: session.to_string(),
                        questions: p.questions.clone(),
                        state: State::Open,
                        posted_answerable: can,
                    };
                    out.push(post(&id, &c));
                    self.cards.insert(id, c);
                }
            }
        }
        out
    }

    pub fn gone(&mut self, session: &str) -> Vec<Action> {
        let ids: Vec<String> = self
            .cards
            .iter()
            .filter(|(_, c)| c.session == session)
            .map(|(id, _)| id.clone())
            .collect();
        ids.iter().filter_map(|id| self.close(id, "gone")).collect()
    }

    pub fn begin_answer(
        &mut self,
        card_id: &str,
        answer: &PollAnswer,
        from_device: &str,
    ) -> Result<Answering, Reject> {
        if self.closed.iter().any(|c| c == card_id) {
            return Err(Reject::Duplicate);
        }
        let c = self.cards.get_mut(card_id).ok_or(Reject::Unknown)?;
        if c.state != State::Open {
            return Err(Reject::Duplicate);
        }
        if !c.posted_answerable {
            return Err(Reject::NotAnswerable);
        }
        answer
            .validate()
            .map_err(|_| Reject::Invalid("malformed answer"))?;
        if answer.card_id != card_id {
            return Err(Reject::Invalid("wrong card"));
        }
        let picks = check_picks(&c.questions, answer)?;
        c.state = State::Answering(from_device.to_string());
        Ok(Answering {
            card_id: card_id.to_string(),
            session: c.session.clone(),
            questions: c.questions.clone(),
            picks,
        })
    }

    pub fn finish(&mut self, card_id: &str, outcome: Outcome) -> Option<Action> {
        if !matches!(self.cards.get(card_id)?.state, State::Answering(_)) {
            return None;
        }
        let (closed, picked, why) = match outcome {
            Outcome::Applied { picked } => ("applied", Some(picked), None),
            Outcome::Stale => ("stale", None, None),
            Outcome::Failed(why) => ("failed", None, Some(clip(&why, PICKED_MAX))),
        };
        match self.close(card_id, closed)? {
            Action::Close {
                card_id, session, ..
            } => Some(Action::Close {
                card_id,
                session,
                closed,
                picked,
                why,
            }),
            Action::Post { .. } => None,
        }
    }
}

fn check_picks(questions: &[AskQuestion], a: &PollAnswer) -> Result<Vec<PollPick>, Reject> {
    let bad = Reject::Invalid;
    if a.answers.len() != questions.len() {
        return Err(bad("one answer per question"));
    }
    let mut seen = vec![false; questions.len()];
    let mut picks = Vec::new();
    for p in &a.answers {
        let qi = usize::from(p.question_idx);
        let q = questions.get(qi).ok_or(bad("unknown question"))?;
        if std::mem::replace(&mut seen[qi], true) {
            return Err(bad("duplicate question"));
        }
        if p.choices.iter().any(|&c| usize::from(c) >= q.options.len()) {
            return Err(bad("unknown option"));
        }
        let other = match p.other.as_deref() {
            Some(text) => {
                if q.multi_select {
                    return Err(bad("other needs a single choice question"));
                }
                if !p.choices.is_empty() {
                    return Err(bad("an option and text together"));
                }
                let text = sanitize_other(text);
                if text.is_empty() {
                    return Err(bad("empty text"));
                }
                Some(text)
            }
            None => {
                if p.choices.is_empty() || (!q.multi_select && p.choices.len() != 1) {
                    return Err(bad("wrong number of choices"));
                }
                None
            }
        };
        picks.push(PollPick {
            question_idx: p.question_idx,
            choices: p.choices.clone(),
            other,
        });
    }
    Ok(picks)
}

fn summary(questions: &[AskQuestion], picks: &[PollPick]) -> String {
    let mut ordered: Vec<&PollPick> = picks.iter().collect();
    ordered.sort_by_key(|p| p.question_idx);
    let parts: Vec<String> = ordered
        .iter()
        .filter_map(|p| {
            let q = questions.get(usize::from(p.question_idx))?;
            if let Some(t) = &p.other {
                return Some(t.clone());
            }
            let mut idx: Vec<usize> = p.choices.iter().map(|&c| usize::from(c)).collect();
            idx.sort_unstable();
            idx.dedup();
            Some(
                idx.iter()
                    .filter_map(|&i| q.options.get(i))
                    .map(|o| shown_label(&o.label).0)
                    .collect::<Vec<_>>()
                    .join(", "),
            )
        })
        .collect();
    clip(&parts.join("; "), PICKED_MAX)
}

pub trait Terminal {
    fn screen(&self) -> Option<Screen>;
    fn send(&self, keys: &[u8]) -> bool;
    fn poll_open(&self) -> bool;
}

#[derive(Clone, Copy, Debug)]
pub struct Pace {
    pub poll: Duration,
    pub step: Duration,
    pub confirm: Duration,
    pub deadline: Duration,
}

impl Default for Pace {
    fn default() -> Pace {
        Pace {
            poll: Duration::from_millis(40),
            step: Duration::from_millis(400),
            confirm: Duration::from_secs(3),
            deadline: Duration::from_secs(15),
        }
    }
}

pub fn run_answer<T: Terminal>(t: &T, ans: &Answering) -> Outcome {
    run_answer_with(t, ans, &Pace::default())
}

pub fn run_answer_with<T: Terminal>(t: &T, ans: &Answering, pace: &Pace) -> Outcome {
    let (qs, picks) = (&ans.questions, &ans.picks);
    let start = Instant::now();
    let mut steps = 0usize;
    loop {
        if start.elapsed() > pace.deadline {
            return Outcome::Failed("the answer took too long".into());
        }
        let screen = t.screen();
        if steps == 0 && !matches!(screen, Some(Screen::Picker(_))) {
            return Outcome::Stale;
        }
        let step = match advance(qs, picks, screen.as_ref()) {
            Next::Done => return confirm(t, pace, summary(qs, picks)),
            Next::Refuse("question does not match") => return Outcome::Stale,
            Next::Refuse(why) => return Outcome::Failed(why.to_string()),
            Next::Step(s) => s,
        };
        if steps >= MAX_STEPS {
            return Outcome::Failed("too many steps".into());
        }
        steps += 1;
        if !t.send(&step.keys) {
            return Outcome::Failed("the agent is not reading its input".into());
        }
        let sent = Instant::now();
        let last = loop {
            std::thread::sleep(pace.poll);
            let now = t.screen();
            if step.expect.holds(qs, now.as_ref()) {
                break None;
            }
            if sent.elapsed() >= pace.step {
                break Some(now);
            }
        };
        if let Some(now) = last {
            return match now {
                Some(Screen::Picker(p)) if qs.iter().any(|q| matches_card(&p, q)) => {
                    Outcome::Failed("the screen did not follow".into())
                }
                _ => Outcome::Stale,
            };
        }
    }
}

fn confirm<T: Terminal>(t: &T, pace: &Pace, picked: String) -> Outcome {
    let start = Instant::now();
    loop {
        if !t.poll_open() {
            return Outcome::Applied { picked };
        }
        if start.elapsed() >= pace.confirm {
            return Outcome::Failed("the agent did not confirm".into());
        }
        std::thread::sleep(pace.poll);
    }
}
