use std::path::{Path, PathBuf};
use tmgr_usage::Tokens;

pub const BASIS_NOTE: &str = "cost is API-equivalent (list prices), not your subscription bill";

pub fn money(v: f64) -> String {
    if v > 0.0 && v < 0.01 {
        format!("${v:.4}")
    } else {
        format!("${v:.2}")
    }
}

pub fn round6(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}

pub fn count(n: u64) -> String {
    match n {
        0..=9_999 => n.to_string(),
        10_000..=999_999 => format!("{:.1}K", n as f64 / 1_000.0),
        1_000_000..=999_999_999 => format!("{:.1}M", n as f64 / 1_000_000.0),
        _ => format!("{:.2}B", n as f64 / 1_000_000_000.0),
    }
}

pub fn total_tokens(t: &Tokens) -> u64 {
    t.input + t.output + t.cache_read + t.cache_write_5m + t.cache_write_1h
}

pub fn span(secs: i64) -> String {
    let s = secs.max(0);
    let (d, h, m) = (s / 86_400, s % 86_400 / 3_600, s % 3_600 / 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else {
        format!("{}m", m.max(if s > 0 { 1 } else { 0 }))
    }
}

pub fn tilde(path: &Path) -> String {
    let text = path.display().to_string();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && home != "/" => match text.strip_prefix(&home) {
            Some(rest) if rest.is_empty() || rest.starts_with('/') => format!("~{rest}"),
            _ => text,
        },
        _ => text,
    }
}

pub fn display_path(path: &Path) -> String {
    if let Some(home) = std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|h| h.as_os_str().len() > 1)
    {
        let canonical = std::fs::canonicalize(&home).unwrap_or_else(|_| home.clone());
        for base in [home, canonical] {
            if let Ok(rest) = path.strip_prefix(&base) {
                return if rest.as_os_str().is_empty() {
                    "~".to_string()
                } else {
                    format!("~/{}", rest.display())
                };
            }
        }
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
    format!("<external>/{}", name.unwrap_or_default())
}

pub fn scrub(text: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if home.len() > 1 => text.replace(&home, "~"),
        _ => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_keeps_precision_for_small_amounts() {
        assert_eq!(money(0.0), "$0.00");
        assert_eq!(money(0.0034), "$0.0034");
        assert_eq!(money(12.345), "$12.35");
    }

    #[test]
    fn counts_and_spans_are_short() {
        assert_eq!(count(9_999), "9999");
        assert_eq!(count(12_345), "12.3K");
        assert_eq!(count(3_400_000), "3.4M");
        assert_eq!(span(0), "0m");
        assert_eq!(span(59), "1m");
        assert_eq!(span(3 * 3600 + 120), "3h 2m");
        assert_eq!(span(90_000), "1d 1h");
    }
}
