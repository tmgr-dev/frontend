use regex::Regex;
use std::fs::OpenOptions;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::OnceLock;

const TAIL: u64 = 64 * 1024;
const ALT_SCREEN_OFF: &[u8] = b"\x1b[?1049l";
const ID_LEN: usize = 36;

pub fn valid_resume_id(id: &str) -> bool {
    id.len() == ID_LEN
        && id.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => matches!(b, b'0'..=b'9' | b'a'..=b'f'),
        })
}

pub fn read_capped(path: &Path, cap: u64) -> io::Result<Vec<u8>> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let meta = f.metadata()?;
    if !meta.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "not a regular file",
        ));
    }
    f.seek(SeekFrom::Start(meta.len().saturating_sub(cap)))?;
    let mut buf = Vec::new();
    f.take(cap).read_to_end(&mut buf)?;
    Ok(buf)
}

fn after_alt_screen(raw: &[u8]) -> &[u8] {
    match raw
        .windows(ALT_SCREEN_OFF.len())
        .rposition(|w| w == ALT_SCREEN_OFF)
    {
        Some(i) => &raw[i + ALT_SCREEN_OFF.len()..],
        None => raw,
    }
}

fn strip_controls(raw: &[u8]) -> String {
    let mut out = Vec::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let b = raw[i];
        i += 1;
        match b {
            0x1b => match raw.get(i) {
                Some(b'[') => {
                    i += 1;
                    while i < raw.len() && !(0x40..=0x7e).contains(&raw[i]) {
                        i += 1;
                    }
                    i += 1;
                }
                Some(b']' | b'P' | b'X' | b'^' | b'_') => {
                    i += 1;
                    while i < raw.len() {
                        if raw[i] == 0x07 {
                            i += 1;
                            break;
                        }
                        if raw[i] == 0x1b {
                            i += raw.get(i + 1).map_or(1, |_| 2);
                            break;
                        }
                        i += 1;
                    }
                }
                Some(_) => i += 1,
                None => {}
            },
            b'\n' | b'\r' | b'\t' => out.push(b),
            0..=0x1f | 0x7f => {}
            _ => out.push(b),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

const UUID: &str = "([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})";

fn matcher(profile: &str) -> Option<&'static Regex> {
    static CLAUDE: OnceLock<Regex> = OnceLock::new();
    static CODEX: OnceLock<Regex> = OnceLock::new();
    match profile {
        "claude-code" => Some(CLAUDE.get_or_init(|| {
            Regex::new(&format!(r#"claude\s+--resume\s+"?{UUID}"#))
                .expect("resume pattern compiles")
        })),
        "codex" => Some(CODEX.get_or_init(|| {
            Regex::new(&format!(r#"codex\s+resume\s+{UUID}"#)).expect("resume pattern compiles")
        })),
        _ => None,
    }
}

pub fn extract_id(raw: &[u8], profile: &str) -> Option<String> {
    let re = matcher(profile)?;
    let text = strip_controls(after_alt_screen(raw));
    re.captures_iter(&text)
        .filter_map(|c| c.get(1))
        .filter(|m| {
            !text[m.end()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        })
        .map(|m| m.as_str().to_string())
        .filter(|id| valid_resume_id(id))
        .last()
}

pub fn id_from_log(path: &Path, profile: &str) -> Option<String> {
    extract_id(&read_capped(path, TAIL).ok()?, profile)
}

#[cfg(test)]
mod tests {
    use super::*;

    const U: &str = "2f0e1c3a-5b7d-4e8f-9a1b-3c4d5e6f7a8b";
    const CLAUDE: &str = "claude-code";
    const V: &str = "0199a213-81c0-7800-8aa1-bbab2a035a53";

    #[test]
    fn validates_ids() {
        assert!(valid_resume_id(U));
        for bad in [
            "",
            "2F0E1C3A-5B7D-4E8F-9A1B-3C4D5E6F7A8B",
            "-f0e1c3a-5b7d-4e8f-9a1b-3c4d5e6f7a8b",
            "2f0e1c3a5b7d4e8f9a1b3c4d5e6f7a8b0000",
            "2f0e1c3a-5b7d-4e8f-9a1b-3c4d5e6f7a8b0",
            "2f0e1c3a-5b7d-4e8f-9a1b-3c4d5e6f7a8",
            "../../etc/passwd-aaaaaaaaaaaaaaaaaaaa",
        ] {
            assert!(!valid_resume_id(bad), "{bad}");
        }
    }

    #[test]
    fn claude_exit_screen_with_colors_and_cursor_moves() {
        let raw = format!(
            "\x1b[?1049h...work...\x1b[?1049l\x1b[2K\r\n\x1b[1m\x1b[38;5;246mResume this session with:\x1b[0m\r\n\x1b[38;2;215;119;87mclaude --resume {U}\x1b[0m\r\n\x1b]0;shell\x07\x1b[?25h"
        );
        assert_eq!(extract_id(raw.as_bytes(), CLAUDE).as_deref(), Some(U));
    }

    #[test]
    fn claude_quoted_id_and_wrapped_lines() {
        let raw = format!("Resume this session\r\nwith:\r\nclaude\r\n--resume\r\n\"{U}\"\r\n");
        assert_eq!(extract_id(raw.as_bytes(), CLAUDE).as_deref(), Some(U));
    }

    #[test]
    fn codex_exit_screen() {
        let raw = format!(
            "\x1b[0mToken usage: total=1,204 input=980 output=224\r\nTo continue this session, run codex resume {V}\r\n"
        );
        assert_eq!(extract_id(raw.as_bytes(), "codex").as_deref(), Some(V));
    }

    #[test]
    fn the_last_match_wins() {
        let raw = format!("claude --resume {V}\r\nmore\r\nclaude --resume {U}\r\n");
        assert_eq!(extract_id(raw.as_bytes(), CLAUDE).as_deref(), Some(U));
    }

    #[test]
    fn text_before_the_last_alt_screen_exit_is_ignored() {
        let raw = format!("claude --resume {V}\x1b[?1049l\x1b[?1049hrepaint\x1b[?1049lbye\r\n");
        assert_eq!(extract_id(raw.as_bytes(), CLAUDE), None);
        let raw = format!("claude --resume {V}\x1b[?1049lbye\r\nclaude --resume {U}");
        assert_eq!(extract_id(raw.as_bytes(), CLAUDE).as_deref(), Some(U));
    }

    #[test]
    fn malformed_ids_are_rejected() {
        for raw in [
            format!("claude --resume {}", U.to_uppercase()),
            format!("claude --resume -{}", &U[1..]),
            format!("claude --resume {U}0"),
            format!("claude --resume {U}-extra"),
            "claude --resume 2f0e1c3a-5b7d-4e8f-9a1b".to_string(),
            "claude --resume $(reboot)".to_string(),
            format!("codex resume {}", U.to_uppercase()),
            format!("claude --resume{U}"),
        ] {
            assert_eq!(extract_id(raw.as_bytes(), CLAUDE), None, "{raw}");
        }
    }

    #[test]
    fn escape_sequences_inside_the_text_are_dropped() {
        let raw = format!("claude --resume \x1b[0m{U}");
        assert_eq!(extract_id(raw.as_bytes(), CLAUDE).as_deref(), Some(U));
        assert_eq!(strip_controls(b"a\x1b]0;t\x1b\\b\x07c\x00d"), "abcd");
    }

    #[test]
    fn only_a_regular_file_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("output.log");
        std::fs::write(&real, format!("x\r\nclaude --resume {U}\r\n")).unwrap();
        assert_eq!(id_from_log(&real, CLAUDE).as_deref(), Some(U));
        let link = dir.path().join("link.log");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert_eq!(id_from_log(&link, CLAUDE), None);
        assert_eq!(id_from_log(dir.path(), CLAUDE), None);
    }

    #[test]
    fn only_the_tail_is_read() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("output.log");
        let mut body = format!("claude --resume {V}\r\n").into_bytes();
        body.extend(vec![b'x'; 70 * 1024]);
        std::fs::write(&p, &body).unwrap();
        assert_eq!(id_from_log(&p, CLAUDE), None);
    }

    #[test]
    fn only_the_profiles_own_pattern_counts() {
        let claude = format!("claude --resume {U}\r\n");
        let codex = format!("codex resume {V}\r\n");
        assert_eq!(extract_id(claude.as_bytes(), "codex"), None);
        assert_eq!(extract_id(codex.as_bytes(), CLAUDE), None);
        assert_eq!(extract_id(codex.as_bytes(), "codex").as_deref(), Some(V));
        let mixed = format!("{codex}{claude}");
        assert_eq!(extract_id(mixed.as_bytes(), CLAUDE).as_deref(), Some(U));
        assert_eq!(extract_id(mixed.as_bytes(), "codex").as_deref(), Some(V));
    }

    #[test]
    fn an_unknown_profile_gets_no_id() {
        let raw = format!("claude --resume {U}\r\ncodex resume {V}\r\n");
        for profile in ["fake", "", "claude", "Codex"] {
            assert_eq!(extract_id(raw.as_bytes(), profile), None, "{profile}");
        }
    }
}
