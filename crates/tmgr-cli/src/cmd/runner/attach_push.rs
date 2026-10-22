use std::path::Path;
use std::sync::mpsc::Receiver;
use std::time::Duration;
use tmgr_core::config::{parse_quiet_hours, AttachConfig};
use tmgr_core::notify::{build_push, PushPayload};
use tmgr_runner::attach::{mask_secrets, PushEvent, PushKind, PushSettings};
use tmgr_runner::sanitize::sanitize_text;

const PUSH_TIMEOUT: Duration = Duration::from_secs(5);
const KEEP_MARKERS: Duration = Duration::from_secs(7 * 24 * 3600);

pub(super) fn settings(cfg: &AttachConfig) -> PushSettings {
    let base = PushSettings::default();
    PushSettings {
        long_min: cfg.push_long,
        error: cfg.push_error.unwrap_or(base.error),
        blocked_min: cfg.push_blocked,
        quiet: cfg.push_quiet.as_deref().and_then(parse_quiet_hours),
        debounce_min: cfg.push_debounce.unwrap_or(base.debounce_min),
    }
}

pub(super) fn payload(
    event: &PushEvent,
    name: &str,
    task_key: Option<&str>,
    link: Option<&str>,
) -> PushPayload {
    let name: String = mask_secrets(&sanitize_text(name))
        .chars()
        .take(40)
        .collect();
    let title = format!("tmgr · {}", name.trim());
    let body = match task_key {
        Some(key) => format!("{key} · {}", event.reason),
        None => event.reason.clone(),
    };
    let priority = match event.kind {
        PushKind::LongTurn => "normal",
        PushKind::Error | PushKind::Blocked => "high",
    };
    build_push(
        &title,
        Some(&body),
        Some(priority),
        link,
        Some("tmgr-attach"),
    )
}

pub(super) fn prune(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > KEEP_MARKERS);
        if old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

pub(super) fn send_real(payload: &PushPayload) -> Result<(), String> {
    use clap::Parser;
    let cli = crate::cli::Cli::parse_from(["tmgr"]);
    let ctx = crate::ctx::Ctx::new(&cli).map_err(|e| e.to_string())?;
    let mut warnings = Vec::new();
    crate::cmd::notifyenv::NotifyEnv::from_ctx(&ctx)
        .with_http(&mut warnings, |http| http.push(payload, PUSH_TIMEOUT))
        .map(|_| ())
        .map_err(|e| e.to_string())
}

pub(super) fn pusher(
    jobs: Receiver<(String, PushPayload)>,
    dir: std::path::PathBuf,
    mut send: impl FnMut(Result<(), String>) -> bool,
) {
    for (key, payload) in jobs {
        let result = send_real(&payload);
        if result.is_err() {
            super::attach_summary::unclaim(&dir, &key);
        }
        if !send(result) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: PushKind, reason: &str) -> PushEvent {
        PushEvent {
            session: "s".into(),
            kind,
            reason: reason.into(),
            key: "s-blocked-1".into(),
        }
    }

    #[test]
    fn settings_are_all_off_by_default_and_follow_the_config() {
        assert!(!settings(&AttachConfig::default()).enabled());
        let cfg = AttachConfig {
            push_long: Some(15),
            push_error: Some(true),
            push_blocked: Some(5),
            push_quiet: Some("22:00-08:00".into()),
            push_debounce: Some(20),
            ..AttachConfig::default()
        };
        let s = settings(&cfg);
        assert_eq!(
            (s.long_min, s.error, s.blocked_min, s.debounce_min),
            (Some(15), true, Some(5), 20)
        );
        assert_eq!(s.quiet, Some((22 * 60, 8 * 60)));
    }

    #[test]
    fn the_payload_has_the_name_the_task_the_reason_and_the_link_only() {
        let e = event(PushKind::Blocked, "is waiting for permission, 6 min ago");
        let p = payload(
            &e,
            "api\u{1b}[31m worker",
            Some("TM-12"),
            Some("https://tmgr.dev/acme/tasks/12"),
        );
        assert_eq!(p.title, "tmgr · api?[31m worker");
        assert_eq!(
            p.body.as_deref(),
            Some("TM-12 · is waiting for permission, 6 min ago")
        );
        assert_eq!(p.priority.as_deref(), Some("high"));
        assert_eq!(p.link.as_deref(), Some("https://tmgr.dev/acme/tasks/12"));
        let json = serde_json::to_string(&p).unwrap();
        assert!(!json.contains("\u{1b}"));
    }

    #[test]
    fn a_session_without_a_task_still_pushes_without_a_link_and_bad_links_are_dropped() {
        let e = event(PushKind::LongTurn, "finished a turn after 25 min");
        let p = payload(&e, "claude", None, Some("javascript:alert(1)"));
        assert_eq!(p.body.as_deref(), Some("finished a turn after 25 min"));
        assert_eq!(p.priority.as_deref(), Some("normal"));
        assert_eq!(p.link, None);
    }

    #[test]
    fn only_old_markers_are_pruned() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("fresh"), "").unwrap();
        prune(d.path());
        assert!(d.path().join("fresh").exists());
    }
}
