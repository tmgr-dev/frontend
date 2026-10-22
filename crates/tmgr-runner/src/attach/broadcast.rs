use super::view::{PaneRect, ViewState};
use ratatui::layout::Rect;
use tmgr_ipc::api::StateKind;
use unicode_width::UnicodeWidthStr;

pub const LABEL: &str = " BROADCAST ";
pub const LABEL_SHORT: &str = " BC ";
pub const OFF_LABEL: &str = " not sent ";
pub const OFF_SHORT: &str = " -- ";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Broadcast {
    pub on: Vec<String>,
    pub off: Vec<String>,
}

pub const ONLY_HERE: &str = " only here ";
pub const ONLY_HERE_SHORT: &str = " 1 ";

pub fn peer_bytes(data: &[u8]) -> Vec<u8> {
    let Ok(text) = std::str::from_utf8(data) else {
        return Vec::new();
    };
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => match chars.peek().copied() {
                Some('\u{1b}') | None => {}
                Some('[') => {
                    chars.next();
                    for n in chars.by_ref() {
                        if ('@'..='~').contains(&n) {
                            break;
                        }
                    }
                }
                Some('O') => {
                    chars.next();
                    chars.next();
                }
                Some(_) => {
                    chars.next();
                }
            },
            '\r' | '\t' | '\u{7f}' => out.push(c),
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out.into_bytes()
}

impl ViewState {
    pub fn toggle_broadcast(&mut self) {
        if self.broadcast.take().is_some() {
            self.notice = Some("broadcast off".into());
            return;
        }
        self.broadcast = Some(Broadcast::default());
        let n = self.broadcast_receivers().len();
        self.notice = Some(format!(
            "broadcast on for {n} pane{}, Ctrl-] m picks panes, Ctrl-] B stops",
            if n == 1 { "" } else { "s" }
        ));
    }

    pub fn mark_broadcast(&mut self) {
        let Some(id) = self.focused_session().map(str::to_string) else {
            return;
        };
        self.toggle_broadcast_pick(&id);
    }

    pub fn toggle_broadcast_pick(&mut self, id: &str) {
        if self.broadcast.is_none() {
            self.notice = Some("broadcast is off, Ctrl-] B turns it on".into());
            return;
        }
        let receiving = self.broadcast_marked(id);
        let Some(b) = self.broadcast.as_mut() else {
            return;
        };
        b.on.retain(|p| p != id);
        b.off.retain(|p| p != id);
        if receiving {
            b.off.push(id.to_string());
        } else {
            b.on.push(id.to_string());
        }
    }

    pub fn broadcast_rename(&mut self, old: &str, new: &str) {
        if let Some(b) = self.broadcast.as_mut() {
            for p in b.on.iter_mut().chain(b.off.iter_mut()) {
                if p == old {
                    *p = new.to_string();
                }
            }
        }
    }

    fn broadcast_exited(&self, id: &str) -> bool {
        self.sessions
            .iter()
            .find(|s| s.id == id)
            .is_none_or(|s| s.state.kind == StateKind::Exited)
    }

    fn broadcast_asking(&self, id: &str) -> bool {
        self.needs.contains_key(id)
            || self.status.get(id).is_some_and(|s| s.ask.is_some())
            || self.open_in(id) > 0
            || self.sessions.iter().find(|s| s.id == id).is_none_or(|s| {
                s.state.kind != StateKind::Working && s.state.kind != StateKind::Waiting
            })
    }

    fn broadcast_drawn(&self) -> Vec<String> {
        if self.zoom {
            return self
                .focused_session()
                .map(str::to_string)
                .into_iter()
                .collect();
        }
        self.panes().to_vec()
    }

    pub fn broadcast_receivers(&self) -> Vec<String> {
        let Some(b) = &self.broadcast else {
            return Vec::new();
        };
        self.broadcast_drawn()
            .into_iter()
            .filter(|id| !self.broadcast_exited(id))
            .filter(|id| {
                b.on.contains(id)
                    || (!b.off.contains(id)
                        && !self.shells.contains(id)
                        && !self.broadcast_asking(id))
            })
            .collect()
    }

    pub fn broadcast_marked(&self, id: &str) -> bool {
        self.broadcast_receivers().iter().any(|p| p == id)
    }

    pub fn broadcast_peers(&self, primary: &str) -> Vec<String> {
        if !self.broadcast_marked(primary) {
            return Vec::new();
        }
        self.broadcast_receivers()
            .into_iter()
            .filter(|p| p != primary)
            .collect()
    }
}

pub(crate) fn marker(p: &PaneRect, st: &ViewState) -> Option<(String, bool)> {
    st.broadcast.as_ref()?;
    let room = usize::from(p.outer.width.saturating_sub(4));
    let on = st.broadcast_marked(&p.session);
    let here = st.focused_session() == Some(p.session.as_str());
    let (long, short) = if on {
        (LABEL, LABEL_SHORT)
    } else if here {
        (ONLY_HERE, ONLY_HERE_SHORT)
    } else {
        (OFF_LABEL, OFF_SHORT)
    };
    let text = if long.width() <= room {
        long
    } else if short.width() <= room {
        short
    } else {
        return None;
    };
    Some((text.to_string(), on))
}

pub(crate) fn marker_rect(p: &PaneRect, st: &ViewState) -> Option<Rect> {
    let (text, _) = marker(p, st)?;
    if p.outer.height < 2 {
        return None;
    }
    Some(Rect::new(
        p.outer.x + 1,
        p.outer.bottom() - 1,
        text.width() as u16,
        1,
    ))
}
