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
        let local = !crate::cmd::setup_terminal::remote_session() && cfg!(target_os = "macos");
        if local {
            pbcopy(clean);
        }
        copy_notice(n, osc, local)
    }
}

const OSA_MAX: usize = 2 * tmgr_link::paste::MAX_IMAGE + 4096;

fn osascript(script: &str) -> Option<String> {
    use std::io::Read;
    let mut child = Command::new("/usr/bin/osascript")
        .args(["-e", script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut out = Vec::new();
    let read = child
        .stdout
        .take()?
        .take(OSA_MAX as u64 + 1)
        .read_to_end(&mut out);
    if out.len() > OSA_MAX {
        let _ = child.kill();
    }
    let status = child.wait().ok()?;
    (read.is_ok() && status.success() && out.len() <= OSA_MAX)
        .then(|| String::from_utf8_lossy(&out).into_owned())
}

pub(super) fn data_class(reply: &str, tag: &str) -> Option<Vec<u8>> {
    let hex = reply
        .trim()
        .strip_prefix(&format!("\u{ab}data {tag}"))?
        .strip_suffix('\u{bb}')?;
    if hex.len() % 2 != 0 || hex.len() / 2 > tmgr_link::paste::MAX_IMAGE {
        return None;
    }
    hex.as_bytes()
        .chunks(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).ok()?, 16).ok())
        .collect()
}

pub(super) fn image_only(info: &str) -> Option<&'static str> {
    let text = ["utf8", "string", "ut16", "furl"]
        .iter()
        .any(|t| info.contains(t));
    if text {
        return None;
    }
    ["PNGf", "JPEG"].into_iter().find(|t| info.contains(t))
}

#[cfg(target_os = "macos")]
pub(super) fn clipboard_image() -> Option<Vec<u8>> {
    if crate::cmd::setup_terminal::remote_session() {
        return None;
    }
    let tag = image_only(&osascript("clipboard info")?)?;
    let reply = osascript(&format!("the clipboard as \u{ab}class {tag}\u{bb}"))?;
    let bytes = data_class(&reply, tag)?;
    tmgr_link::paste::image_ext(&bytes).map(|_| bytes)
}

#[cfg(not(target_os = "macos"))]
pub(super) fn clipboard_image() -> Option<Vec<u8>> {
    None
}

fn copy_notice(n: usize, osc: bool, local: bool) -> String {
    let chars = format!("{n} character{}", if n == 1 { "" } else { "s" });
    if local {
        format!("copied {chars}")
    } else if osc {
        format!(
            "sent {chars} to the terminal clipboard (empty? run `tmgr setup terminal` on your computer)"
        )
    } else {
        format!("{chars} too long for the terminal clipboard")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_local_copy_says_copied_because_a_terminal_never_confirms() {
        assert_eq!(copy_notice(5, true, true), "copied 5 characters");
        assert_eq!(copy_notice(5, false, true), "copied 5 characters");
        assert_eq!(
            copy_notice(1, true, false),
            "sent 1 character to the terminal clipboard (empty? run `tmgr setup terminal` on your computer)"
        );
        assert_eq!(
            copy_notice(200_000, false, false),
            "200000 characters too long for the terminal clipboard"
        );
    }

    #[test]
    fn copied_text_is_plain_with_newlines_only() {
        assert_eq!(plain("a\x1b[31mb\u{7}\tc\r\nd\0"), "a[31mbc\nd");
    }

    #[test]
    fn the_clipboard_reply_is_decoded_only_for_the_asked_type() {
        assert_eq!(
            data_class("\u{ab}data PNGf89504E47\u{bb}\n", "PNGf"),
            Some(vec![0x89, 0x50, 0x4e, 0x47])
        );
        assert_eq!(data_class("\u{ab}data JPEGff\u{bb}", "PNGf"), None);
        assert_eq!(data_class("\u{ab}data PNGf89504\u{bb}", "PNGf"), None);
        assert_eq!(data_class("\u{ab}data PNGfzz\u{bb}", "PNGf"), None);
        assert_eq!(data_class("hello", "PNGf"), None);
    }

    #[test]
    fn an_image_counts_only_when_the_clipboard_holds_no_text() {
        assert_eq!(
            image_only("{{\u{ab}class PNGf\u{bb}, 100}, {\u{ab}class TIFF\u{bb}, 200}}"),
            Some("PNGf")
        );
        assert_eq!(image_only("{{\u{ab}class JPEG\u{bb}, 100}}"), Some("JPEG"));
        assert_eq!(
            image_only("{{\u{ab}class PNGf\u{bb}, 100}, {string, 5}, {\u{ab}class utf8\u{bb}, 5}}"),
            None
        );
        assert_eq!(image_only("{{string, 5}}"), None);
        assert_eq!(image_only("{}"), None);
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
