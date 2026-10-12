use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskRef {
    Key(String),
    Id(u64),
}

pub fn parse_task_ref(input: &str) -> Result<TaskRef> {
    if let Some(key) = parse_key(input) {
        return Ok(TaskRef::Key(key.to_string()));
    }
    let digits = input.strip_prefix('#').unwrap_or(input);
    if (1..=9).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit()) {
        if let Ok(id) = digits.parse::<u64>() {
            return Ok(TaskRef::Id(id));
        }
    }
    Err(
        Error::usage(format!("invalid task reference {:?}", printable(input)))
            .hint("use TM-512 or #8974, see `tmgr help refs`"),
    )
}

pub fn parse_key(input: &str) -> Option<&str> {
    let (code, seq) = input.split_once('-')?;
    let mut chars = code.chars();
    let first = chars.next()?;
    let code_ok = first.is_ascii_uppercase()
        && code.len() <= 10
        && code
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit());
    let seq_ok = (1..=7).contains(&seq.len()) && seq.bytes().all(|b| b.is_ascii_digit());
    (code_ok && seq_ok).then_some(input)
}

pub fn is_category_code(input: &str) -> bool {
    let len = input.len();
    (1..=10).contains(&len)
        && input.as_bytes()[0].is_ascii_uppercase()
        && input
            .bytes()
            .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
}

pub fn is_workspace_code(input: &str) -> bool {
    (1..=64).contains(&input.len())
        && input
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn printable(input: &str) -> String {
    input
        .chars()
        .take(40)
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_keys_and_ids() {
        assert_eq!(
            parse_task_ref("TM-512").unwrap(),
            TaskRef::Key("TM-512".into())
        );
        assert_eq!(
            parse_task_ref("A1B2C3D4E5-1234567").unwrap(),
            TaskRef::Key("A1B2C3D4E5-1234567".into())
        );
        assert_eq!(parse_task_ref("#8974").unwrap(), TaskRef::Id(8974));
        assert_eq!(parse_task_ref("512").unwrap(), TaskRef::Id(512));
        assert_eq!(
            parse_task_ref("999999999").unwrap(),
            TaskRef::Id(999_999_999)
        );
    }

    #[test]
    fn rejects_everything_else() {
        for bad in [
            "",
            "TM",
            "TM-",
            "-1",
            "-rf",
            "--help",
            "tm-1",
            "1TM-1",
            "TM-12345678",
            "ABCDEFGHIJK-1",
            "TM-1;rm",
            "TM-1 ",
            " TM-1",
            "TM-1\n",
            "../x",
            "../../etc/passwd",
            "TM-1/../2",
            "#",
            "##1",
            "#-1",
            "1234567890",
            "12a",
            "TM-1%0a",
            "TM_1",
            "TМ-1",
        ] {
            let err = parse_task_ref(bad).unwrap_err();
            assert_eq!(err.exit_code(), 2, "{bad:?} must be a usage error");
        }
    }

    #[test]
    fn invalid_ref_message_has_no_control_characters() {
        let err = parse_task_ref("x\u{1b}[31m").unwrap_err();
        assert!(!err.message.contains('\u{1b}'));
    }

    #[test]
    fn category_and_workspace_codes() {
        assert!(is_category_code("TM"));
        assert!(!is_category_code("tm"));
        assert!(!is_category_code("T M"));
        assert!(!is_category_code(""));
        assert!(is_workspace_code("my-ws_1"));
        assert!(!is_workspace_code("a/b"));
        assert!(!is_workspace_code("local:x"));
    }
}
