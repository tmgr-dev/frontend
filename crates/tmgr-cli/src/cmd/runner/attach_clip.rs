use base64::Engine;
use std::io::Write;
use std::process::{Command, Stdio};

const OSC_MAX: usize = 100_000;

pub(super) trait Clipboard: Send {
    fn copy(&mut self, text: &str) -> String;
}

pub(super) fn plain(text: &str) -> String {
    text.chars()
        .filter(|c| !c.is_control() || *c == '\n')
        .collect()
}

pub(super) fn osc52(text: &str) -> Vec<u8> {
    let mut out = b"\x1b]52;c;".to_vec();
    out.extend(
        base64::engine::general_purpose::STANDARD
            .encode(text)
            .bytes(),
    );
    out.push(0x07);
    out
}

pub(super) struct SystemClipboard;

fn pbcopy(text: String) {
    std::thread::spawn(move || {
        let Ok(mut child) = Command::new("/usr/bin/pbcopy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return;
        };
        if let Some(mut pipe) = child.stdin.take() {
            let _ = pipe.write_all(text.as_bytes());
        }
        let _ = child.wait();
    });
}

fn remote_session() -> bool {
    std::env::var_os("SSH_CONNECTION").is_some() || std::env::var_os("SSH_TTY").is_some()
}

impl Clipboard for SystemClipboard {
    fn copy(&mut self, text: &str) -> String {
        let clean = plain(text);
        let n = clean.chars().count();
        let osc = clean.len() <= OSC_MAX;
        if osc {
            let mut out = std::io::stdout();
            let _ = out.write_all(&osc52(&clean));
            let _ = out.flush();
        }
        let local = !remote_session() && cfg!(target_os = "macos");
        if local {
            pbcopy(clean);
        }
        if osc || local {
            format!("copied {n} character{}", if n == 1 { "" } else { "s" })
        } else {
            format!("{n} characters are too long for the terminal clipboard")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copied_text_is_plain_with_newlines_only() {
        assert_eq!(plain("a\x1b[31mb\u{7}\tc\r\nd\0"), "a[31mbc\nd");
    }

    #[test]
    fn osc52_wraps_base64_and_cannot_carry_a_terminator() {
        let bytes = osc52("hi\x07there");
        assert!(bytes.starts_with(b"\x1b]52;c;"));
        assert!(bytes.ends_with(&[0x07]));
        assert_eq!(bytes.iter().filter(|b| **b == 0x07).count(), 1);
        assert_eq!(&bytes[7..bytes.len() - 1], b"aGkHdGhlcmU=");
    }
}
