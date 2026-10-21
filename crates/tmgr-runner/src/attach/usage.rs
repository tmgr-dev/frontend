use super::view::ViewState;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    pub pct: f64,
    pub stale: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cost {
    pub usd: f64,
    pub stale: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct UsageBar {
    pub claude_5h: Option<Reading>,
    pub claude_week: Option<Reading>,
    pub codex_week: Option<Reading>,
    pub costs: HashMap<String, Cost>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Plain,
    Stale,
    Warn,
    Hot,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Part {
    pub text: String,
    pub tone: Tone,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionMeta {
    pub parts: Vec<Part>,
}

impl SessionMeta {
    pub fn text(&self) -> String {
        self.parts
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

fn percent(label: &str, r: Option<Reading>) -> Part {
    let Some(r) = r else {
        return Part {
            text: format!("{label} ?"),
            tone: Tone::Stale,
        };
    };
    let pct = r.pct.round().clamp(0.0, 999.0) as u32;
    let tone = match (r.stale, pct) {
        (true, _) => Tone::Stale,
        (false, 95..) => Tone::Hot,
        (false, 80..) => Tone::Warn,
        _ => Tone::Plain,
    };
    let mark = if r.stale { "~" } else { "" };
    Part {
        text: format!("{label} {mark}{pct}%"),
        tone,
    }
}

fn dollars(c: Option<Cost>) -> Part {
    match c {
        None => Part {
            text: "$?".into(),
            tone: Tone::Stale,
        },
        Some(c) => Part {
            text: format!("${}{:.2}", if c.stale { "~" } else { "" }, c.usd.max(0.0)),
            tone: if c.stale { Tone::Stale } else { Tone::Plain },
        },
    }
}

impl UsageBar {
    fn is_blank(&self) -> bool {
        self.claude_5h.is_none()
            && self.claude_week.is_none()
            && self.codex_week.is_none()
            && self.costs.is_empty()
    }

    pub fn parts(&self, focused: Option<&str>, claude_focused: bool) -> Vec<Part> {
        let mut out = vec![
            percent("5h", self.claude_5h),
            percent("wk", self.claude_week),
            percent("cx", self.codex_week),
        ];
        if let Some(id) = focused {
            let cost = self.costs.get(id).copied();
            if cost.is_some() || claude_focused {
                out.push(dollars(cost));
            }
        }
        out
    }
}

impl ViewState {
    pub fn set_usage(&mut self, bar: UsageBar) -> bool {
        if self.usage.as_ref() == Some(&bar) {
            return false;
        }
        self.usage = Some(bar);
        true
    }

    pub fn usage_parts(&self) -> Vec<Part> {
        let Some(bar) = self.usage.as_ref().filter(|b| !b.is_blank()) else {
            return Vec::new();
        };
        let focused = self.focused_session();
        let claude = focused.is_some_and(|id| self.status.contains_key(id));
        bar.parts(focused, claude)
    }

    pub fn session_meta(&self, id: &str) -> Option<SessionMeta> {
        let st = self.status.get(id)?;
        if st.model.is_none() && st.ctx.is_none() && st.mode.is_none() {
            return None;
        }
        let model = Part {
            text: st.model.clone().unwrap_or_else(|| "?".into()),
            tone: Tone::Plain,
        };
        let ctx = match st.ctx {
            Some(p) => Part {
                text: format!("ctx {p}%"),
                tone: match p {
                    95.. => Tone::Hot,
                    80.. => Tone::Warn,
                    _ => Tone::Plain,
                },
            },
            None => Part {
                text: "ctx ?".into(),
                tone: Tone::Stale,
            },
        };
        let mode = match st.mode.as_deref() {
            Some("bypass") => Part {
                text: "bypass".into(),
                tone: Tone::Hot,
            },
            Some(m) => Part {
                text: m.to_string(),
                tone: Tone::Plain,
            },
            None => Part {
                text: "?".into(),
                tone: Tone::Stale,
            },
        };
        Some(SessionMeta {
            parts: vec![model, ctx, mode],
        })
    }
}
