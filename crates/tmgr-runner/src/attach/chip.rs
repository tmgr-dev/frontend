use super::title::clip;
use super::view::ViewState;
use unicode_width::UnicodeWidthStr;

const SEP: &str = " · ";
const MIN_BRANCH: usize = 6;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskChip {
    pub key: String,
    pub branch: Option<String>,
    pub started_s: Option<u64>,
    pub workspace: Option<String>,
    pub task_id: Option<u64>,
}

fn clock(secs: u64) -> String {
    match secs {
        0..=3599 => format!("{}:{:02}", secs / 60, secs % 60),
        _ => format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60),
    }
}

pub fn chip_text(chip: &TaskChip, now_s: u64, room: usize) -> Option<String> {
    let timer = chip
        .started_s
        .filter(|s| *s > 0 && now_s > 0)
        .map(|s| clock(now_s.saturating_sub(s)));
    let head = |branch: Option<&str>| {
        let mut parts = vec![chip.key.as_str()];
        parts.extend(branch);
        parts.extend(timer.as_deref());
        format!("{SEP}{}", parts.join(SEP))
    };
    let full = head(chip.branch.as_deref());
    if full.width() <= room {
        return Some(full);
    }
    if let Some(branch) = chip.branch.as_deref() {
        let without = head(None).width();
        let left = room.saturating_sub(without + SEP.width());
        if left >= MIN_BRANCH {
            return Some(head(Some(&clip(branch, left))));
        }
    }
    let short = head(None);
    if short.width() <= room {
        return Some(short);
    }
    let key_only = format!("{SEP}{}", chip.key);
    (key_only.width() <= room).then_some(key_only)
}

pub fn open_urls(web_base: &str, chip: &TaskChip) -> Option<(String, String)> {
    let ws = chip.workspace.as_deref()?;
    let id = chip.task_id?;
    let valid = !ws.is_empty()
        && ws.len() <= 64
        && ws
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if !valid || id == 0 || id > 999_999_999_999 {
        return None;
    }
    let desktop = format!("tmgr://open?path=%2F{ws}%2Ftasks%2F{id}");
    let web = format!("{}/{ws}/tasks/{id}", web_base.trim_end_matches('/'));
    Some((desktop, web))
}

impl ViewState {
    pub fn set_chips(&mut self, mut found: std::collections::HashMap<String, TaskChip>) -> bool {
        found.retain(|id, _| self.sessions.iter().any(|r| &r.id == id));
        if found == self.chips {
            return false;
        }
        self.chips = found;
        true
    }

    pub fn chip_of(&self, session: &str) -> Option<&TaskChip> {
        self.chips.get(session)
    }
}
