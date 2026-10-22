use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WatchState {
    Running,
    Ok,
    Fail,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchStatus {
    pub state: WatchState,
    #[serde(default)]
    pub code: Option<i32>,
    #[serde(default)]
    pub ms: u64,
    #[serde(default)]
    pub runs: u32,
}

pub fn duration_label(ms: u64) -> String {
    let secs = ms / 1000;
    if secs >= 60 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else {
        format!("{}.{}s", secs, (ms % 1000) / 100)
    }
}

impl WatchStatus {
    pub fn glyph(&self) -> &'static str {
        match self.state {
            WatchState::Running => "…",
            WatchState::Ok => "✓",
            WatchState::Fail => "✗",
        }
    }

    pub fn badge(&self, room: usize) -> Option<String> {
        let text = match (self.state, self.code) {
            (WatchState::Running, _) => format!(" · {} running", self.glyph()),
            (WatchState::Ok, _) => format!(" · {} {}", self.glyph(), duration_label(self.ms)),
            (WatchState::Fail, Some(c)) => {
                format!(" · {} {} exit {c}", self.glyph(), duration_label(self.ms))
            }
            (WatchState::Fail, None) => {
                format!(" · {} {}", self.glyph(), duration_label(self.ms))
            }
        };
        let short = format!(" · {} {}", self.glyph(), duration_label(self.ms));
        [text, short]
            .into_iter()
            .find(|t| unicode_width::UnicodeWidthStr::width(t.as_str()) <= room)
    }
}
