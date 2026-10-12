use crate::screen::Modes;

fn app_cursor(on: bool) -> &'static [u8] {
    if on {
        b"\x1b[?1h"
    } else {
        b"\x1b[?1l"
    }
}

fn bracketed_paste(on: bool) -> &'static [u8] {
    if on {
        b"\x1b[?2004h"
    } else {
        b"\x1b[?2004l"
    }
}

pub fn mode_changes(prev: Option<Modes>, next: Modes) -> Vec<u8> {
    let mut out = Vec::new();
    if prev.map(|p| p.app_cursor) != Some(next.app_cursor) {
        out.extend_from_slice(app_cursor(next.app_cursor));
    }
    if prev.map(|p| p.bracketed_paste) != Some(next.bracketed_paste) {
        out.extend_from_slice(bracketed_paste(next.bracketed_paste));
    }
    out
}

pub fn reset_modes() -> Vec<u8> {
    let mut out = app_cursor(false).to_vec();
    out.extend_from_slice(bracketed_paste(false));
    out
}
