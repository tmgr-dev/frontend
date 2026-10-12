const PREFIX: u8 = 0x1d;

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
}

#[derive(Default)]
pub struct InputSplitter {
    prefix: bool,
    scroll: bool,
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
        let mut out = Vec::new();
        let mut run: Vec<u8> = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
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
        out
    }
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
