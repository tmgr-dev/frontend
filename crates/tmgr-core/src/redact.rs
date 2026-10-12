use std::fmt;
use zeroize::Zeroize;

pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl Clone for Secret {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[redacted]")
    }
}

pub fn redact_secrets(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = find_bearer(rest) {
        let (head, tail) = rest.split_at(pos);
        out.push_str(head);
        let after = &tail["bearer".len()..];
        let ws = after.len() - after.trim_start().len();
        let token_part = &after[ws..];
        let end = token_part
            .find(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == ',' || c == ';')
            .unwrap_or(token_part.len());
        out.push_str(&tail[.."bearer".len()]);
        out.push_str(&after[..ws]);
        out.push_str("[redacted]");
        rest = &token_part[end..];
    }
    out.push_str(rest);
    out
}

fn find_bearer(s: &str) -> Option<usize> {
    let lower = s.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find("bearer") {
        let pos = from + i;
        let after = &s[pos + "bearer".len()..];
        if after.starts_with(char::is_whitespace) && !after.trim_start().is_empty() {
            return Some(pos);
        }
        from = pos + "bearer".len();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_of_secret_is_redacted() {
        let s = Secret::new("super-secret-token");
        assert_eq!(format!("{s:?}"), "[redacted]");
        assert_eq!(s.expose(), "super-secret-token");
    }

    #[test]
    fn bearer_values_are_replaced() {
        assert_eq!(
            redact_secrets("Authorization: Bearer abc123 and more"),
            "Authorization: Bearer [redacted] and more"
        );
        assert_eq!(redact_secrets("bearer  xyz"), "bearer  [redacted]");
        assert_eq!(redact_secrets("no token here"), "no token here");
    }
}
