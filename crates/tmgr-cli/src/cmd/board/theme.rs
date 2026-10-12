use ratatui::style::{Color, Modifier, Style};
use ratatui::symbols::border;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorLevel {
    None,
    Basic,
    Ansi256,
    True,
}

impl ColorLevel {
    pub fn from_env(off: bool, colorterm: &str, term: &str) -> ColorLevel {
        if off {
            ColorLevel::None
        } else if colorterm == "truecolor" || colorterm == "24bit" {
            ColorLevel::True
        } else if term.contains("256color") {
            ColorLevel::Ansi256
        } else {
            ColorLevel::Basic
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub level: ColorLevel,
    pub ascii: bool,
}

const ASCII_BORDER: border::Set = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

impl Theme {
    pub fn new(level: ColorLevel, ascii: bool) -> Theme {
        Theme { level, ascii }
    }

    fn pick(&self, rgb: (u8, u8, u8), idx: u8, basic: Color) -> Option<Color> {
        match self.level {
            ColorLevel::None => None,
            ColorLevel::True => Some(Color::Rgb(rgb.0, rgb.1, rgb.2)),
            ColorLevel::Ansi256 => Some(Color::Indexed(idx)),
            ColorLevel::Basic => Some(basic),
        }
    }

    fn fg(&self, rgb: (u8, u8, u8), idx: u8, basic: Color) -> Style {
        match self.pick(rgb, idx, basic) {
            Some(c) => Style::new().fg(c),
            None => Style::new(),
        }
    }

    pub fn accent(&self) -> Style {
        self.fg((232, 104, 90), 203, Color::Red)
    }

    pub fn key(&self) -> Style {
        self.accent().add_modifier(Modifier::BOLD)
    }

    pub fn muted(&self) -> Style {
        match self.level {
            ColorLevel::None => Style::new().add_modifier(Modifier::DIM),
            _ => self.fg((140, 140, 152), 245, Color::DarkGray),
        }
    }

    pub fn good(&self) -> Style {
        self.fg((98, 190, 120), 114, Color::Green)
    }

    pub fn warn(&self) -> Style {
        self.fg((232, 190, 90), 179, Color::Yellow)
    }

    pub fn bold(&self) -> Style {
        Style::new().add_modifier(Modifier::BOLD)
    }

    pub fn selected(&self) -> Style {
        match self.pick((62, 36, 34), 52, Color::Reset) {
            Some(Color::Reset) | None => Style::new().add_modifier(Modifier::REVERSED),
            Some(c) => Style::new().bg(c),
        }
    }

    pub fn border_set(&self) -> border::Set<'static> {
        if self.ascii {
            ASCII_BORDER
        } else {
            border::ROUNDED
        }
    }

    pub fn dot(&self) -> &'static str {
        if self.ascii {
            "*"
        } else {
            "\u{25cf}"
        }
    }

    pub fn ellipsis(&self) -> &'static str {
        if self.ascii {
            "..."
        } else {
            "\u{2026}"
        }
    }

    pub fn sep(&self) -> &'static str {
        if self.ascii {
            " - "
        } else {
            " \u{b7} "
        }
    }

    pub fn cursor(&self) -> &'static str {
        if self.ascii {
            "_"
        } else {
            "\u{2588}"
        }
    }
}
