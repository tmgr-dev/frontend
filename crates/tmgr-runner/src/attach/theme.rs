use ratatui::style::Color;
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeKind {
    #[default]
    Dark,
    Light,
    HighContrast,
}

impl ThemeKind {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "dark" => Some(Self::Dark),
            "light" => Some(Self::Light),
            "high-contrast" | "high_contrast" | "hc" => Some(Self::HighContrast),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
            Self::HighContrast => "high-contrast",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Colors {
    pub accent: Color,
    pub sidebar_bg: Color,
    pub dim: Color,
    pub strong: Color,
    pub working: Color,
    pub waiting: Color,
    pub ok: Color,
    pub bad: Color,
    pub warn: Color,
}

impl ThemeKind {
    pub fn colors(self) -> Colors {
        match self {
            Self::Dark => Colors {
                accent: Color::LightRed,
                sidebar_bg: Color::Indexed(237),
                dim: Color::DarkGray,
                strong: Color::White,
                working: Color::Yellow,
                waiting: Color::Cyan,
                ok: Color::Green,
                bad: Color::Red,
                warn: Color::Yellow,
            },
            Self::Light => Colors {
                accent: Color::Indexed(160),
                sidebar_bg: Color::Indexed(254),
                dim: Color::Indexed(243),
                strong: Color::Black,
                working: Color::Indexed(130),
                waiting: Color::Indexed(30),
                ok: Color::Indexed(28),
                bad: Color::Indexed(124),
                warn: Color::Indexed(130),
            },
            Self::HighContrast => Colors {
                accent: Color::LightMagenta,
                sidebar_bg: Color::Indexed(236),
                dim: Color::Gray,
                strong: Color::White,
                working: Color::LightYellow,
                waiting: Color::LightCyan,
                ok: Color::LightGreen,
                bad: Color::LightRed,
                warn: Color::LightYellow,
            },
        }
    }
}

thread_local! {
    static CURRENT: Cell<ThemeKind> = const { Cell::new(ThemeKind::Dark) };
}

pub(crate) fn colors() -> Colors {
    CURRENT.with(Cell::get).colors()
}

pub(crate) fn with_theme<R>(kind: ThemeKind, f: impl FnOnce() -> R) -> R {
    let prev = CURRENT.with(|c| c.replace(kind));
    let out = f();
    CURRENT.with(|c| c.set(prev));
    out
}

pub fn parse_osc11(reply: &[u8]) -> Option<(u8, u8, u8)> {
    let at = reply
        .windows(4)
        .position(|w| w == b"rgb:" || w == b"gba:")?;
    let rest = &reply[at + 4..];
    let end = rest
        .iter()
        .position(|&b| b == 0x07 || b == 0x1b)
        .unwrap_or(rest.len());
    let body = std::str::from_utf8(&rest[..end]).ok()?;
    let mut parts = body.split('/');
    let mut channel = || -> Option<u8> {
        let p = parts.next()?;
        if p.is_empty() || p.len() > 4 || !p.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let v = u32::from_str_radix(p, 16).ok()?;
        let max = (1u32 << (4 * p.len())) - 1;
        u8::try_from((v * 255 + max / 2) / max).ok()
    };
    let rgb = (channel()?, channel()?, channel()?);
    let alpha_ok = match parts.next() {
        None => true,
        Some(a) => reply[at] == b'g' && parts.next().is_none() && !a.is_empty(),
    };
    alpha_ok.then_some(rgb)
}

pub fn kind_for_background((r, g, b): (u8, u8, u8)) -> ThemeKind {
    let luma = 299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b);
    if luma >= 128_000 {
        ThemeKind::Light
    } else {
        ThemeKind::Dark
    }
}

pub const OSC11_QUERY: &[u8] = b"\x1b]11;?\x07";
