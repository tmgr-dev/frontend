use tmgr_ipc::api::Modes;

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

pub fn focus_on() -> &'static [u8] {
    b"\x1b[?1004h"
}

pub fn reset_modes() -> Vec<u8> {
    let mut out = app_cursor(false).to_vec();
    out.extend_from_slice(bracketed_paste(false));
    out.extend_from_slice(b"\x1b[?1004l");
    out.extend_from_slice(mouse_off());
    out
}

pub fn enter_mouse() -> &'static [u8] {
    b"\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h"
}

fn mouse_off() -> &'static [u8] {
    b"\x1b[?1006l\x1b[?1003l\x1b[?1002l\x1b[?1000l"
}
