use std::io::Write;
use std::process::{Command, Stdio};
use tmgr_core::config::{parse_quiet_hours, AttachConfig};
use tmgr_core::redact::redact_secrets;
use tmgr_runner::attach::{Notifier, SignalSettings};

pub(super) fn settings(cfg: &AttachConfig) -> SignalSettings {
    let base = SignalSettings::default();
    SignalSettings {
        permission: cfg.signal_permission.unwrap_or(base.permission),
        question: cfg.signal_question.unwrap_or(base.question),
        input: cfg.signal_input.unwrap_or(base.input),
        quiet: cfg.signal_quiet.as_deref().and_then(parse_quiet_hours),
    }
}

pub(super) fn minute_of_day() -> u16 {
    use chrono::Timelike;
    let now = chrono::Local::now();
    (now.hour() * 60 + now.minute()) as u16
}

pub(super) fn osascript_argv(text: &str) -> Vec<String> {
    [
        "-e",
        "on run argv",
        "-e",
        "display notification (item 2 of argv) with title (item 1 of argv)",
        "-e",
        "end run",
        "--",
        "tmgr attach",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .chain(std::iter::once(redact_secrets(text)))
    .collect()
}

pub(super) struct SystemNotifier;

impl Notifier for SystemNotifier {
    fn bell(&mut self) {
        let mut out = std::io::stdout();
        let _ = out.write_all(b"\x07");
        let _ = out.flush();
    }

    fn notify(&mut self, text: &str) {
        if !cfg!(target_os = "macos") {
            return;
        }
        let argv = osascript_argv(text);
        std::thread::spawn(move || {
            let _ = Command::new("osascript")
                .args(argv)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_to_permission_and_questions_only() {
        let s = settings(&AttachConfig::default());
        assert!(s.permission && s.question && !s.input);
        assert_eq!(s.quiet, None);
    }

    #[test]
    fn settings_follow_the_config() {
        let cfg = AttachConfig {
            signal_permission: Some(false),
            signal_input: Some(true),
            signal_quiet: Some("22:00-08:00".into()),
            ..AttachConfig::default()
        };
        let s = settings(&cfg);
        assert!(!s.permission && s.question && s.input);
        assert_eq!(s.quiet, Some((22 * 60, 8 * 60)));
    }

    #[test]
    fn the_notification_text_is_one_argv_item_never_script_source() {
        let text = "a\" & do shell script \"x\" needs you: Bearer abc123";
        let argv = osascript_argv(text);
        assert_eq!(argv[0], "-e");
        let last = argv.last().unwrap();
        assert!(last.contains("do shell script"));
        assert!(!last.contains("abc123"));
        assert!(!argv[..argv.len() - 1]
            .iter()
            .any(|a| a.contains("do shell script")));
    }
}
