fn unsafe_char(c: char) -> bool {
  c.is_control() || matches!(c, '\u{200B}'..='\u{200F}' | '\u{2028}'..='\u{202E}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
}

pub fn text(s: &str, max: usize) -> String {
  s.chars().filter(|c| !unsafe_char(*c)).take(max).collect::<String>().trim().to_owned()
}

#[cfg(test)]
mod tests {
  use super::text;

  #[test]
  fn strips_controls_and_caps_length() {
    assert_eq!(text("a\x1b]52;c;x\x07b\u{9b}31m\n\tc", 100), "a]52;c;xb31mc");
    assert_eq!(text(&"x".repeat(300), 120).chars().count(), 120);
    assert_eq!(text("  hi \u{202E}there ", 50), "hi there");
  }
}
