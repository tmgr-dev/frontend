use crate::ctx::Ctx;
use serde_json::Value;
use std::io::IsTerminal;
use tmgr_core::error::Error;
use tmgr_core::{Envelope, ErrorEnvelope};

pub struct Output {
    pub data: Value,
    pub human: String,
}

impl Output {
    pub fn new(data: impl serde::Serialize, human: impl Into<String>) -> Self {
        Self {
            data: serde_json::to_value(data).expect("output serializes"),
            human: human.into(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    None,
    Basic,
    Ansi256,
    True,
}

#[derive(Clone, Copy)]
pub struct Style(Level);

impl Style {
    pub fn detect(no_color_flag: bool, is_tty: bool) -> Style {
        let no_color_env = std::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty());
        let dumb = std::env::var("TERM").is_ok_and(|t| t == "dumb");
        if no_color_flag || no_color_env || dumb || !is_tty {
            return Style(Level::None);
        }
        let colorterm = std::env::var("COLORTERM").unwrap_or_default();
        let term = std::env::var("TERM").unwrap_or_default();
        Style(if colorterm == "truecolor" || colorterm == "24bit" {
            Level::True
        } else if term.contains("256color") {
            Level::Ansi256
        } else {
            Level::Basic
        })
    }

    pub fn off() -> Style {
        Style(Level::None)
    }

    fn paint(&self, code: &str, s: &str) -> String {
        if self.0 == Level::None {
            s.to_string()
        } else {
            format!("\x1b[{code}m{s}\x1b[0m")
        }
    }

    pub fn accent(&self, s: &str) -> String {
        let code = match self.0 {
            Level::True => "38;2;232;104;90",
            Level::Ansi256 => "38;5;203",
            _ => "31",
        };
        self.paint(code, s)
    }

    pub fn muted(&self, s: &str) -> String {
        self.paint(if self.0 == Level::Basic { "90" } else { "2" }, s)
    }

    pub fn good(&self, s: &str) -> String {
        self.paint("32", s)
    }

    pub fn bold(&self, s: &str) -> String {
        self.paint("1", s)
    }
}

pub fn stdout_style(no_color: bool) -> Style {
    Style::detect(no_color, std::io::stdout().is_terminal())
}

pub fn stderr_style(no_color: bool) -> Style {
    Style::detect(no_color, std::io::stderr().is_terminal())
}

pub fn emit(ctx: &Ctx, out: Output) -> i32 {
    let warnings = ctx.take_warnings();
    if ctx.json {
        let env = Envelope::with_warnings(out.data, warnings);
        println!(
            "{}",
            serde_json::to_string(&env).expect("envelope serializes")
        );
        return 0;
    }
    if !out.human.is_empty() {
        println!("{}", out.human.trim_end_matches('\n'));
    }
    if !ctx.quiet {
        for w in warnings {
            eprintln!(
                "{} {}",
                ctx.err_style.muted("warning:"),
                tmgr_core::text::sanitize_line(&w.message)
            );
        }
    }
    0
}

pub fn print_error(json: bool, verbose: bool, style: Style, err: &Error) -> i32 {
    if json {
        println!(
            "{}",
            serde_json::to_string(&ErrorEnvelope::new(err)).expect("error envelope serializes")
        );
    } else {
        eprintln!(
            "{} {}",
            style.accent("error:"),
            tmgr_core::text::sanitize_line(&err.message)
        );
        if let Some(h) = &err.hint {
            eprintln!("{} {}", style.muted("hint:"), h);
        }
        if verbose {
            eprintln!(
                "{}",
                style.muted(&format!(
                    "code: {} (exit {})",
                    err.kind.name(),
                    err.exit_code()
                ))
            );
        }
    }
    err.exit_code()
}

pub fn table(rows: &[Vec<String>]) -> String {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    let mut widths = vec![0usize; cols];
    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.chars().count());
        }
    }
    rows.iter()
        .map(|row| {
            let last = row.len().saturating_sub(1);
            row.iter()
                .enumerate()
                .map(|(i, cell)| {
                    if i == last {
                        cell.clone()
                    } else {
                        format!("{cell:<w$}", w = widths[i])
                    }
                })
                .collect::<Vec<_>>()
                .join("  ")
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_is_off_for_the_flag_and_for_non_tty() {
        assert_eq!(Style::detect(true, true).accent("x"), "x");
        assert_eq!(Style::detect(false, false).bold("x"), "x");
    }

    #[test]
    fn colour_levels_paint_ansi_sequences() {
        assert_eq!(Style(Level::Basic).accent("x"), "\x1b[31mx\x1b[0m");
        assert_eq!(Style(Level::Ansi256).accent("x"), "\x1b[38;5;203mx\x1b[0m");
        assert_eq!(
            Style(Level::True).accent("x"),
            "\x1b[38;2;232;104;90mx\x1b[0m"
        );
        assert_eq!(Style(Level::Basic).muted("x"), "\x1b[90mx\x1b[0m");
    }

    #[test]
    fn tables_pad_all_but_the_last_column() {
        let rows = vec![
            vec!["a".to_string(), "bb".to_string(), "t1".to_string()],
            vec!["ccc".to_string(), "d".to_string(), "t2".to_string()],
        ];
        assert_eq!(table(&rows), "a    bb  t1\nccc  d   t2");
    }
}
