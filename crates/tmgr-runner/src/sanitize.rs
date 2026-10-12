use std::borrow::Cow;

fn is_unsafe(c: char) -> bool {
    matches!(c,
        '\u{0}'..='\u{1f}'
        | '\u{7f}'
        | '\u{80}'..='\u{9f}'
        | '\u{200e}' | '\u{200f}'
        | '\u{202a}'..='\u{202e}'
        | '\u{2066}'..='\u{2069}'
        | '\u{feff}')
}

pub fn sanitize_text(s: &str) -> Cow<'_, str> {
    if !s.chars().any(is_unsafe) {
        return Cow::Borrowed(s);
    }
    Cow::Owned(
        s.chars()
            .map(|c| if is_unsafe(c) { '?' } else { c })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_text_is_borrowed() {
        assert!(matches!(sanitize_text("héllo 世界"), Cow::Borrowed(_)));
    }

    #[test]
    fn replaces_controls_c1_bidi_bom() {
        let input = "a\u{1b}b\u{7f}c\u{9b}d\u{202e}e\u{2066}f\u{feff}g\u{200f}h\0";
        assert_eq!(sanitize_text(input), "a?b?c?d?e?f?g?h?");
    }

    #[test]
    fn keeps_combining_marks() {
        assert_eq!(sanitize_text("e\u{301}\u{1}"), "e\u{301}?");
    }
}
