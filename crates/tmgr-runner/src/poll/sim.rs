use crate::attach::status::AskOption;
use crate::attach::status::AskQuestion;
use tmgr_ipc::chat::PollPick;

pub(crate) fn question(header: &str, text: &str, multi: bool, labels: &[&str]) -> AskQuestion {
    AskQuestion {
        header: header.into(),
        question: text.into(),
        multi_select: multi,
        options: labels
            .iter()
            .map(|l| AskOption {
                label: (*l).into(),
                description: format!("about {l}"),
            })
            .collect(),
    }
}

pub(crate) fn scope_q() -> AskQuestion {
    question(
        "Scope",
        "Which scope should the rework cover in the hub service, where one service calls all external sources and serves data to everyone?",
        false,
        &[
            "Jira+GitLab+Confluence (Recommended)",
            "All sources at once",
            "Only Jira+GitLab",
        ],
    )
}

pub(crate) fn sources_q() -> AskQuestion {
    question(
        "Sources",
        "Which sources should be migrated?",
        true,
        &["Jira", "GitLab", "Confluence"],
    )
}

pub(crate) fn order_q() -> AskQuestion {
    question("Order", "Which goes first?", false, &["Jira", "GitLab"])
}

pub(crate) fn pick(idx: u8, choices: &[u8], other: Option<&str>) -> PollPick {
    PollPick {
        question_idx: idx,
        choices: choices.to_vec(),
        other: other.map(String::from),
    }
}

pub(crate) fn narrow_q() -> AskQuestion {
    question(
        "Narrow",
        "A fairly long question to wrap on a narrow terminal screen, does it wrap?",
        false,
        &["Yes please", "No thanks"],
    )
}

pub(crate) struct Sim {
    pub(crate) qs: Vec<AskQuestion>,
    pub(crate) width: usize,
    pub(crate) tab: usize,
    pub(crate) cur: usize,
    pub(crate) checked: Vec<Vec<bool>>,
    pub(crate) typed: String,
    pub(crate) answers: Vec<String>,
    pub(crate) review: bool,
    pub(crate) review_cur: usize,
    pub(crate) gone: bool,
    pub(crate) submitted: bool,
    pub(crate) single_goes_to_review: bool,
    pub(crate) keys: Vec<Vec<u8>>,
    pub(crate) ignore: usize,
    pub(crate) vanish_after: Option<usize>,
}

impl Sim {
    pub(crate) fn new(qs: Vec<AskQuestion>, width: usize) -> Sim {
        let checked = qs.iter().map(|q| vec![false; q.options.len()]).collect();
        Sim {
            qs,
            width,
            tab: 0,
            cur: 0,
            checked,
            typed: String::new(),
            answers: Vec::new(),
            review: false,
            review_cur: 0,
            gone: false,
            submitted: false,
            single_goes_to_review: false,
            keys: Vec::new(),
            ignore: 0,
            vanish_after: None,
        }
    }

    pub(crate) fn n(&self) -> usize {
        self.qs[self.tab].options.len()
    }

    pub(crate) fn multi(&self) -> bool {
        self.qs[self.tab].multi_select
    }

    pub(crate) fn nrows(&self) -> usize {
        self.n() + if self.multi() { 3 } else { 2 }
    }

    pub(crate) fn chat_row(&self) -> usize {
        self.nrows() - 1
    }

    pub(crate) fn advance_tab(&mut self) {
        self.tab += 1;
        self.cur = 0;
        self.typed.clear();
        if self.tab == self.qs.len() {
            if self.qs.len() > 1 || self.single_goes_to_review {
                self.review = true;
                self.tab -= 1;
            } else {
                self.gone = true;
            }
        }
    }

    pub(crate) fn press(&mut self, keys: &[u8]) {
        self.keys.push(keys.to_vec());
        if self.gone {
            panic!("key after the picker is gone");
        }
        if self.ignore > 0 {
            self.ignore -= 1;
            return;
        }
        self.apply(keys);
        if self.vanish_after.is_some_and(|n| self.keys.len() >= n) {
            self.gone = true;
        }
    }

    fn apply(&mut self, keys: &[u8]) {
        if self.review {
            match keys {
                b"\x1b[B" => self.review_cur = (self.review_cur + 1) % 2,
                b"\r" if self.review_cur == 0 => {
                    self.gone = true;
                    self.submitted = true;
                }
                other => panic!("bad review key {other:?}"),
            }
            return;
        }
        match keys {
            b"\x1b[B" => self.cur = (self.cur + 1) % self.nrows(),
            b" " => {
                assert!(self.multi() && self.cur < self.n(), "space off a checkbox");
                let t = self.tab;
                let c = self.cur;
                self.checked[t][c] = !self.checked[t][c];
            }
            b"\r" => self.enter(),
            text => {
                assert!(self.on_text() && text[0] != 0x1b, "bad key {text:?}");
                self.typed.push_str(std::str::from_utf8(text).unwrap());
            }
        }
    }

    pub(crate) fn on_text(&self) -> bool {
        self.cur == self.n()
    }

    pub(crate) fn enter(&mut self) {
        let t = self.tab;
        let n = self.n();
        if self.multi() {
            if self.cur == n + 1 {
                let picked: Vec<&str> = (0..n)
                    .filter(|&i| self.checked[t][i])
                    .map(|i| self.qs[t].options[i].label.as_str())
                    .collect();
                self.answers.push(picked.join(", "));
                self.advance_tab();
            }
            return;
        }
        if self.cur < n {
            self.answers
                .push(self.qs[t].options[self.cur].label.clone());
            self.advance_tab();
        } else if self.cur == n && !self.typed.is_empty() {
            self.answers.push(self.typed.clone());
            self.advance_tab();
        } else if self.cur == self.chat_row() {
            panic!("chat row picked");
        }
    }

    pub(crate) fn render(&self) -> Vec<String> {
        if self.gone {
            return Vec::new();
        }
        let rule = "─".repeat(self.width);
        let mut out = vec![rule.clone()];
        let tabbed = self.qs.len() > 1;
        if tabbed {
            let mut bar = String::from("←  ");
            for (i, q) in self.qs.iter().enumerate() {
                let mark = if i < self.answers.len() { '☒' } else { '☐' };
                bar.push_str(&format!("{mark} {}  ", q.header));
            }
            bar.push_str("✔ Submit  →");
            out.push(bar);
        } else {
            out.push(format!(" ☐ {}", self.qs[0].header));
        }
        out.push(String::new());
        if self.review {
            out.push("Review your answers".into());
            out.push(String::new());
            for (q, a) in self.qs.iter().zip(&self.answers) {
                out.push(format!(" ● {}", q.question));
                out.push(format!("   → {a}"));
            }
            out.push(String::new());
            let m = |i: usize| if self.review_cur == i { "❯" } else { " " };
            out.push(format!("{} 1. Submit answers", m(0)));
            out.push(format!("{} 2. Cancel", m(1)));
            return out;
        }
        let q = &self.qs[self.tab];
        if q.question.len() > 40 {
            let mid = q.question[..q.question.len() / 2]
                .rfind(' ')
                .unwrap_or(q.question.len() / 2);
            out.push(format!("│ {}", &q.question[..mid]));
            out.push(format!("│ {}", q.question[mid..].trim_start()));
        } else {
            out.push(q.question.clone());
        }
        out.push(String::new());
        let m = |i: usize| if self.cur == i { "❯" } else { " " };
        let n = self.n();
        for (i, o) in q.options.iter().enumerate() {
            let box_ = if q.multi_select {
                if self.checked[self.tab][i] {
                    "[✔] "
                } else {
                    "[ ] "
                }
            } else {
                ""
            };
            out.push(format!("{} {}. {box_}{}", m(i), i + 1, o.label));
            out.push(format!("      {}", o.description));
        }
        let text = if !self.typed.is_empty() {
            self.typed.clone()
        } else if q.multi_select {
            "Type something".into()
        } else {
            "Type something.".into()
        };
        let box_ = if q.multi_select { "[ ] " } else { "" };
        out.push(format!("{} {}. {box_}{text}", m(n), n + 1));
        if q.multi_select {
            out.push(format!("{}    Next", m(n + 1)));
        }
        out.push(rule);
        out.push(format!("{} {}. Chat about this", m(self.chat_row()), n + 2));
        out.push(String::new());
        out.push(
            if tabbed {
                "Enter to select · Tab/Arrow keys to navigate · Esc to cancel"
            } else {
                "Enter to select · ↑/↓ to navigate · Esc to cancel"
            }
            .into(),
        );
        out
    }
}
