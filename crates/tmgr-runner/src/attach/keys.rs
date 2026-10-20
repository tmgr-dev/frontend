use super::mouse::{parse_sgr, MouseEvent};
use std::time::{Duration, Instant};

const PREFIX: u8 = 0x1d;
const PASTE_BEGIN: &[u8] = b"\x1b[200~";
const PASTE_END: &[u8] = b"\x1b[201~";
const MAX_HELD: usize = 32;
const HELD_FOR: Duration = Duration::from_millis(20);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    Detach,
    NextSession,
    PrevSession,
    Split,
    FocusOther,
    Zoom,
    Sidebar,
    ScrollMode,
    Help,
    Picker,
    Stop,
    Remove,
    Rename,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollKey {
    PageUp,
    PageDown,
    Up,
    Down,
    Top,
    Bottom,
    Exit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Forward(Vec<u8>),
    Cmd(Cmd),
    Scroll(ScrollKey),
    Mouse(MouseEvent),
    Drop(Vec<u8>),
    Paste {
        data: Vec<u8>,
        start: bool,
        end: bool,
    },
}

#[derive(Default)]
pub struct InputSplitter {
    prefix: bool,
    scroll: bool,
    paste: Option<bool>,
    held: Vec<u8>,
    held_at: Option<Instant>,
}

fn command(b: u8) -> Option<Cmd> {
    Some(match b {
        b'd' => Cmd::Detach,
        b'n' => Cmd::NextSession,
        b'p' => Cmd::PrevSession,
        b's' => Cmd::Split,
        b'\t' => Cmd::FocusOther,
        b'z' => Cmd::Zoom,
        b'b' => Cmd::Sidebar,
        b'[' => Cmd::ScrollMode,
        b'?' => Cmd::Help,
        b'w' => Cmd::Picker,
        b'x' => Cmd::Stop,
        b'X' => Cmd::Remove,
        b'r' => Cmd::Rename,
        _ => return None,
    })
}

impl InputSplitter {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_scroll_mode(&mut self, on: bool) {
        self.scroll = on;
        self.prefix = false;
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Vec<Action> {
        let joined;
        self.held_at = None;
        let bytes = if self.held.is_empty() {
            bytes
        } else {
            joined = [std::mem::take(&mut self.held).as_slice(), bytes].concat();
            &joined
        };
        let mut out = Vec::new();
        let mut run: Vec<u8> = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if let Some(first) = self.paste {
                i = self.paste_chunk(bytes, i, first, &mut out);
                continue;
            }
            if bytes[i] == 0x1b {
                match mouse_at(&bytes[i..]) {
                    Seq::Mouse(ev, used) => {
                        flush(&mut run, &mut out);
                        out.push(Action::Mouse(ev));
                        i += used;
                        continue;
                    }
                    Seq::Skip(used) => {
                        flush(&mut run, &mut out);
                        i += used;
                        continue;
                    }
                    Seq::Partial => {
                        flush(&mut run, &mut out);
                        self.hold(&bytes[i..]);
                        break;
                    }
                    Seq::None => {}
                }
                let rest = &bytes[i..];
                if rest.starts_with(PASTE_BEGIN) {
                    flush(&mut run, &mut out);
                    self.paste = Some(true);
                    self.scroll = false;
                    continue;
                }
                if rest.len() >= 3
                    && rest.len() < PASTE_BEGIN.len()
                    && PASTE_BEGIN.starts_with(rest)
                {
                    flush(&mut run, &mut out);
                    self.hold(rest);
                    break;
                }
            }
            if self.scroll {
                flush(&mut run, &mut out);
                let (key, used) = scroll_key(&bytes[i..]);
                i += used;
                if let Some(key) = key {
                    out.push(Action::Scroll(key));
                    if key == ScrollKey::Exit {
                        self.scroll = false;
                    }
                }
                continue;
            }
            let b = bytes[i];
            i += 1;
            if self.prefix {
                self.prefix = false;
                if b == PREFIX {
                    run.push(PREFIX);
                } else if let Some(cmd) = command(b) {
                    flush(&mut run, &mut out);
                    out.push(Action::Cmd(cmd));
                    if cmd == Cmd::ScrollMode {
                        self.scroll = true;
                    }
                }
            } else if b == PREFIX {
                self.prefix = true;
            } else {
                run.push(b);
            }
        }
        flush(&mut run, &mut out);
        if let [Action::Forward(data)] = out.as_slice() {
            if looks_like_drop(data) {
                return vec![Action::Drop(out.remove(0).into_forward())];
            }
        }
        out
    }

    fn hold(&mut self, rest: &[u8]) {
        self.held = rest.to_vec();
        self.held_at = (!rest.is_empty()).then(Instant::now);
    }

    pub fn flush_stale(&mut self, now: Instant) -> Vec<Action> {
        let stale = self.paste.is_none()
            && self
                .held_at
                .is_some_and(|t| now.saturating_duration_since(t) >= HELD_FOR);
        if !stale {
            return Vec::new();
        }
        self.held_at = None;
        let data = std::mem::take(&mut self.held);
        if self.scroll {
            if data == b"\x1b" {
                self.scroll = false;
                return vec![Action::Scroll(ScrollKey::Exit)];
            }
            return Vec::new();
        }
        vec![Action::Forward(data)]
    }

    fn paste_chunk(
        &mut self,
        bytes: &[u8],
        from: usize,
        first: bool,
        out: &mut Vec<Action>,
    ) -> usize {
        let skip = if first { PASTE_BEGIN.len() } else { 0 };
        let body = &bytes[from + skip..];
        if let Some(p) = find(body, PASTE_END) {
            let to = from + skip + p + PASTE_END.len();
            out.push(Action::Paste {
                data: bytes[from..to].to_vec(),
                start: first,
                end: true,
            });
            self.paste = None;
            return to;
        }
        let keep = (1..PASTE_END.len().min(body.len() + 1))
            .rev()
            .find(|n| body.ends_with(&PASTE_END[..*n]))
            .unwrap_or(0);
        let to = bytes.len() - keep;
        if to > from {
            out.push(Action::Paste {
                data: bytes[from..to].to_vec(),
                start: first,
                end: false,
            });
            self.paste = Some(false);
        }
        self.hold(&bytes[to..]);
        bytes.len()
    }
}

impl Action {
    fn into_forward(self) -> Vec<u8> {
        match self {
            Action::Forward(d) => d,
            _ => Vec::new(),
        }
    }
}

pub fn looks_like_drop(data: &[u8]) -> bool {
    data.len() >= 4
        && (data.starts_with(b"/") || data.starts_with(b"~/"))
        && data.ends_with(b" ")
        && data[1..data.len() - 1].contains(&b'/')
        && data.iter().all(|b| *b >= 0x20 && *b != 0x7f)
}

enum Seq {
    Mouse(MouseEvent, usize),
    Partial,
    Skip(usize),
    None,
}

fn mouse_at(b: &[u8]) -> Seq {
    if b == [0x1b] || b == [0x1b, b'['] {
        return Seq::Partial;
    }
    if b.len() < 3 {
        return Seq::None;
    }
    if b[1] == b'[' && b[2] == b'M' {
        return if b.len() >= 6 {
            Seq::Skip(6)
        } else {
            Seq::Partial
        };
    }
    if b[1] != b'[' || b[2] != b'<' {
        return Seq::None;
    }
    let mut j = 3;
    while j < b.len() && (b[j].is_ascii_digit() || b[j] == b';') {
        j += 1;
    }
    if j == b.len() {
        return if j <= MAX_HELD {
            Seq::Partial
        } else {
            Seq::None
        };
    }
    let release = match b[j] {
        b'M' => false,
        b'm' => true,
        _ => return Seq::None,
    };
    match parse_sgr(&b[3..j], release) {
        Some(ev) => Seq::Mouse(ev, j + 1),
        None => Seq::None,
    }
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn flush(run: &mut Vec<u8>, out: &mut Vec<Action>) {
    if !run.is_empty() {
        out.push(Action::Forward(std::mem::take(run)));
    }
}

fn scroll_key(b: &[u8]) -> (Option<ScrollKey>, usize) {
    match b[0] {
        b'g' => (Some(ScrollKey::Top), 1),
        b'G' => (Some(ScrollKey::Bottom), 1),
        b'q' => (Some(ScrollKey::Exit), 1),
        0x1b if b.len() == 1 => (Some(ScrollKey::Exit), 1),
        0x1b => match b[1] {
            b'O' => match b.get(2) {
                Some(b'A') => (Some(ScrollKey::Up), 3),
                Some(b'B') => (Some(ScrollKey::Down), 3),
                Some(_) => (None, 3),
                None => (None, 2),
            },
            b'[' => {
                let end = b[2..]
                    .iter()
                    .position(|c| (0x40..=0x7e).contains(c))
                    .map(|p| p + 3);
                let Some(end) = end else {
                    return (None, b.len());
                };
                let key = match &b[2..end] {
                    b"5~" => Some(ScrollKey::PageUp),
                    b"6~" => Some(ScrollKey::PageDown),
                    b"A" => Some(ScrollKey::Up),
                    b"B" => Some(ScrollKey::Down),
                    _ => None,
                };
                (key, end)
            }
            _ => (None, 1),
        },
        _ => (None, 1),
    }
}
