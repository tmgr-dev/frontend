pub mod haiku;
pub mod rules;
mod store;
pub mod tail;

pub use store::{
    question_id, Entry, Origin, Question, ScanLock, SessionFile, Settings, Status, Store,
    MAX_PER_SESSION,
};

use crate::attach::{locate, Format, Source};
use rules::{candidates, clip_head, clip_tail, collapse};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tail::{locate_codex_rollout, Tailer, Turn};

const HAIKU_BACKOFF: Duration = Duration::from_secs(600);
const HAIKU_PER_ROUND: usize = 3;
const MAX_TAILS: usize = 64;

pub struct SessionSrc {
    pub id: String,
    pub format: Format,
    pub home: PathBuf,
    pub cwd: String,
    pub pid: u32,
    pub log_id: Option<String>,
    pub settled: bool,
}

pub struct Scanner {
    store: Store,
    claude: Option<PathBuf>,
    workdir: PathBuf,
    timeout: Duration,
    tails: HashMap<String, Tailer>,
    haiku_off_until: Option<Instant>,
}

impl Scanner {
    pub fn new(store: Store, claude: Option<PathBuf>, workdir: PathBuf) -> Scanner {
        Scanner {
            store,
            claude,
            workdir,
            timeout: haiku::TIMEOUT,
            tails: HashMap::new(),
            haiku_off_until: None,
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Scanner {
        self.timeout = timeout;
        self
    }

    pub fn store(&self) -> &Store {
        &self.store
    }

    pub fn scan(&mut self, sources: &[SessionSrc], now_s: u64) -> bool {
        if self.tails.len() > MAX_TAILS {
            self.tails
                .retain(|id, _| sources.iter().any(|s| &s.id == id));
        }
        let mut budget = HAIKU_PER_ROUND;
        let mut changed = false;
        for src in sources {
            changed |= self.scan_one(src, now_s, &mut budget);
        }
        changed
    }

    fn scan_one(&mut self, src: &SessionSrc, now_s: u64, budget: &mut usize) -> bool {
        let Some(path) = transcript(src) else {
            return false;
        };
        let mut file = self.store.load(&src.id);
        let shown = path.to_string_lossy();
        let fresh = self.tails.get(&src.id).is_none_or(|t| !t.is_for(&path));
        if fresh {
            let resume = (file.transcript == shown).then_some(file.offset);
            self.tails.insert(
                src.id.clone(),
                Tailer::new(src.format, src.home.clone(), path.clone(), resume),
            );
        }
        let Some(tail) = self.tails.get_mut(&src.id) else {
            return false;
        };
        let turn = tail.poll(src.settled);
        let offset = tail.resume_offset();
        let mut dirty = file.transcript != shown || file.offset != offset;
        let mut added = false;
        if let Some(turn) = turn {
            let found = self.classify(&turn, budget);
            let marks = self.store.entries(Some(&src.id));
            for (text, origin) in found {
                let id = question_id(&src.id, turn.start, &text);
                let known = file.questions.iter().any(|q| q.id == id)
                    || marks
                        .iter()
                        .any(|e| e.status == Status::Open && e.q.text == text);
                if known {
                    continue;
                }
                file.questions.push(Question {
                    id,
                    session: src.id.clone(),
                    text,
                    asked_s: now_s,
                    origin,
                    user: turn.user.clone(),
                    reply: turn.text.clone(),
                });
                dirty = true;
                added = true;
            }
        }
        if dirty {
            file.transcript = shown.into_owned();
            file.offset = offset;
            let _ = self.store.save(&src.id, &file, now_s);
        }
        added
    }

    fn classify(&mut self, turn: &Turn, budget: &mut usize) -> Vec<(String, Origin)> {
        let mut out: Vec<(String, Origin)> = turn
            .asked
            .iter()
            .map(|q| (q.clone(), Origin::Rule))
            .collect();
        let rule = candidates(&turn.text);
        if rule.is_empty() {
            return out;
        }
        let from_rule = |out: &mut Vec<(String, Origin)>| {
            for q in &rule {
                if !out.iter().any(|(t, _)| t == q) {
                    out.push((q.clone(), Origin::Rule));
                }
            }
        };
        let usable = self.store.settings().haiku
            && *budget > 0
            && self.haiku_off_until.is_none_or(|t| Instant::now() >= t);
        let Some(bin) = self.claude.clone().filter(|_| usable) else {
            from_rule(&mut out);
            return out;
        };
        *budget -= 1;
        let input = clip_tail(&turn.text, haiku::INPUT_CAP);
        match haiku::classify(&bin, &self.workdir, &input, self.timeout) {
            Ok(list) => {
                for q in list {
                    if !out.iter().any(|(t, _)| *t == q) {
                        out.push((q, Origin::Haiku));
                    }
                }
            }
            Err(_) => {
                self.haiku_off_until = Some(Instant::now() + HAIKU_BACKOFF);
                from_rule(&mut out);
            }
        }
        out
    }
}

fn transcript(src: &SessionSrc) -> Option<PathBuf> {
    match src.format {
        Format::Claude => {
            let source = Source {
                home: &src.home,
                format: src.format,
                cwd: &src.cwd,
                pid: src.pid,
                log_id: src.log_id.as_deref(),
            };
            locate(&source).map(|(p, _)| p)
        }
        Format::Codex => locate_codex_rollout(&src.home, src.log_id.as_deref(), &src.cwd),
    }
}

pub fn ago(now_s: u64, then_s: u64) -> String {
    let secs = now_s.saturating_sub(then_s);
    match secs {
        0..=59 => "just now".to_string(),
        60..=3599 => format!("{} min ago", secs / 60),
        3600..=86399 => format!("{} h ago", secs / 3600),
        _ => format!("{} d ago", secs / 86400),
    }
}

pub fn short_age(now_s: u64, then_s: u64) -> String {
    let secs = now_s.saturating_sub(then_s);
    match secs {
        0..=59 => "now".to_string(),
        60..=3599 => format!("{}m", secs / 60),
        3600..=86399 => format!("{}h", secs / 3600),
        _ => format!("{}d", secs / 86400),
    }
}

pub fn context_lines(reply: &str, question: &str) -> Vec<String> {
    let paras: Vec<&str> = reply
        .split("\n\n")
        .filter(|p| !p.trim().is_empty())
        .collect();
    if paras.is_empty() {
        return Vec::new();
    }
    let probe: String = collapse(question)
        .chars()
        .take(30)
        .collect::<String>()
        .to_lowercase();
    let at = paras
        .iter()
        .position(|p| collapse(p).to_lowercase().contains(&probe))
        .unwrap_or(paras.len() - 1);
    let from = at.saturating_sub(1);
    let to = (at + 2).min(paras.len());
    let mut lines = Vec::new();
    for (i, p) in paras[from..to].iter().enumerate() {
        if i > 0 {
            lines.push(String::new());
        }
        lines.extend(p.lines().map(collapse));
    }
    lines
}

pub fn paste_text(q: &Question, now_s: u64) -> String {
    let question = collapse(&q.text);
    let context = context_lines(&q.reply, &q.text)
        .into_iter()
        .find(|l| {
            l.to_lowercase()
                .contains(&question.chars().take(30).collect::<String>().to_lowercase())
        })
        .map(|l| clip_head(&l, 160))
        .filter(|l| *l != question);
    let mut out = format!(
        "> Earlier you asked ({}): \"{}\"\n",
        ago(now_s, q.asked_s),
        question
    );
    if let Some(c) = context {
        out.push_str(&format!("> {c}\n"));
    }
    out.push_str("Answer: ");
    out
}

#[cfg(test)]
mod tests;
