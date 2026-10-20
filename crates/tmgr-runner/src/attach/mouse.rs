use tmgr_ipc::api::{MouseEncoding, MouseLevel, MouseMode};

const SHIFT: u16 = 4;
const MOTION: u16 = 32;
const WHEEL: u16 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MouseEvent {
    pub cb: u16,
    pub col: u16,
    pub row: u16,
    pub release: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Press,
    Release,
    Drag,
    Move,
    WheelUp,
    WheelDown,
    WheelOther,
}

impl MouseEvent {
    pub fn kind(&self) -> Kind {
        if self.cb & WHEEL != 0 {
            return match self.cb & 3 {
                0 => Kind::WheelUp,
                1 => Kind::WheelDown,
                _ => Kind::WheelOther,
            };
        }
        let motion = self.cb & MOTION != 0;
        let no_button = self.cb & 3 == 3;
        match (motion, no_button, self.release) {
            (true, true, _) => Kind::Move,
            (true, false, _) => Kind::Drag,
            (false, _, true) => Kind::Release,
            (false, _, false) => Kind::Press,
        }
    }

    pub fn is_left_press(&self) -> bool {
        self.kind() == Kind::Press && self.cb & 3 == 0
    }

    pub fn shift(&self) -> bool {
        self.cb & SHIFT != 0
    }
}

pub fn parse_sgr(body: &[u8], release: bool) -> Option<MouseEvent> {
    let text = std::str::from_utf8(body).ok()?;
    let mut parts = text.split(';');
    let cb: u16 = parts.next()?.parse().ok()?;
    let x: u16 = parts.next()?.parse().ok()?;
    let y: u16 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || x == 0 || y == 0 {
        return None;
    }
    Some(MouseEvent {
        cb,
        col: x - 1,
        row: y - 1,
        release,
    })
}

pub fn wants(mode: MouseMode, kind: Kind) -> bool {
    if matches!(mode.level, MouseLevel::Off | MouseLevel::Unknown) {
        return false;
    }
    match kind {
        Kind::Press | Kind::Release | Kind::WheelUp | Kind::WheelDown | Kind::WheelOther => true,
        Kind::Drag => matches!(mode.level, MouseLevel::Drag | MouseLevel::Any),
        Kind::Move => mode.level == MouseLevel::Any,
    }
}

pub fn encode(ev: &MouseEvent, col: u16, row: u16, encoding: MouseEncoding) -> Vec<u8> {
    let (x, y) = (col + 1, row + 1);
    match encoding {
        MouseEncoding::Sgr => format!(
            "\x1b[<{};{};{}{}",
            ev.cb,
            x,
            y,
            if ev.release { 'm' } else { 'M' }
        )
        .into_bytes(),
        MouseEncoding::Default | MouseEncoding::Unknown => {
            let cb = if ev.release && ev.cb & WHEEL == 0 {
                (ev.cb & !3) | 3
            } else {
                ev.cb
            };
            let byte = |v: u16| u8::try_from(v + 32).unwrap_or(255);
            if x > 222 || y > 222 {
                return Vec::new();
            }
            vec![0x1b, b'[', b'M', byte(cb), byte(x), byte(y)]
        }
        MouseEncoding::Utf8 => {
            let cb = if ev.release && ev.cb & WHEEL == 0 {
                (ev.cb & !3) | 3
            } else {
                ev.cb
            };
            let mut out = b"\x1b[M".to_vec();
            for v in [cb, x, y] {
                let c = char::from_u32(u32::from(v) + 32).unwrap_or('\u{7ff}');
                let mut buf = [0u8; 4];
                out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
            }
            out
        }
    }
}
