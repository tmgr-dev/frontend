use crate::chat::Chat;
use crate::client::Client;
use crate::handoff_store::HandoffStore;
use std::time::{Duration, Instant};
use tmgr_ipc::api::limits::MAX_RUN_PROMPT;
use tmgr_ipc::api::{SessionInfo, StateKind};
use tmgr_ipc::chat::{valid_message_id, AuthorKind, MessageKind};
use tmgr_session::packet::{
    cut_bytes, render_prompt, ChatKind, ChatMessage, HandoffPacket, Limits, PacketSource, PacketTo,
};
pub use tmgr_session::switch::*;

const CHAT_MESSAGES: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Carry {
    pub mode: String,
    pub fell_back: bool,
    pub bypass_dropped: bool,
}

fn carried_mode(old: &SessionInfo, keep_bypass: bool) -> &'static str {
    match old.permission_mode.as_deref() {
        Some("accept-edits") => "accept-edits",
        Some("plan") => "plan",
        Some("bypass") if keep_bypass => "bypass",
        _ => "ask",
    }
}

pub fn carry(
    old: &SessionInfo,
    target: Target,
    keep_bypass: bool,
    supports: impl Fn(&str, &str) -> bool,
) -> Result<Carry, SwitchError> {
    let wanted = carried_mode(old, keep_bypass);
    let bypass_dropped =
        !keep_bypass && (old.permission_mode.as_deref() == Some("bypass") || old.no_sandbox);
    if supports(target.profile(), wanted) {
        return Ok(Carry {
            mode: wanted.into(),
            fell_back: false,
            bypass_dropped,
        });
    }
    if wanted == "bypass" {
        return Err(SwitchError::Usage(format!(
            "{} cannot run in bypass mode",
            target.profile()
        )));
    }
    Ok(Carry {
        mode: "ask".into(),
        fell_back: true,
        bypass_dropped,
    })
}

#[derive(Debug, Clone)]
pub struct Handoff {
    pub packet_id: String,
    pub prompt: String,
    pub packet: HandoffPacket,
}

fn chat_history(chat: &Chat, old: &SessionInfo) -> Vec<ChatMessage> {
    let room = chat.session_room(&old.id);
    let mut out: Vec<ChatMessage> = chat
        .history(&room, None, None)
        .unwrap_or_default()
        .into_iter()
        .filter(|m| m.kind == MessageKind::Text)
        .map(|m| ChatMessage {
            role: match m.author.kind {
                AuthorKind::Owner => "owner",
                AuthorKind::Manager => "manager",
                AuthorKind::Session => "assistant",
                AuthorKind::System => "system",
            }
            .into(),
            kind: ChatKind::Text,
            text: m.body_md,
            ts_ms: Some(m.ts_ms),
        })
        .collect();
    out.drain(..out.len().saturating_sub(CHAT_MESSAGES));
    out
}

fn log_names(inputs: &Inputs) -> Vec<String> {
    inputs
        .exports
        .iter()
        .filter_map(|(_, p)| p.file_name().and_then(|n| n.to_str()).map(str::to_string))
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub fn build_handoff(
    store: &HandoffStore,
    chat: &Chat,
    old: &SessionInfo,
    inputs: &Inputs,
    target: Target,
    carry: &Carry,
    now_ms: u64,
    random: [u8; 10],
) -> Result<Handoff, SwitchError> {
    let finish = |packet: HandoffPacket| Handoff {
        packet_id: packet.id.clone(),
        prompt: cut_bytes(&render_prompt(&packet), MAX_RUN_PROMPT),
        packet,
    };
    if let Some((_, value)) = store.find_for(&old.id, target.name(), old.state.since_ms) {
        if let Ok(packet) = serde_json::from_value::<HandoffPacket>(value) {
            return Ok(finish(packet));
        }
    }
    let src = PacketSource {
        machine: chat.machine().to_string(),
        session: old.id.clone(),
        name: old.name.clone(),
        agent: inputs.source,
        profile: old.profile.clone(),
        cwd: old.cwd.clone(),
        permission_mode: old.permission_mode.clone(),
        no_sandbox: old.no_sandbox,
        carried_mode: Some(carry.mode.clone()),
        fell_back: carry.fell_back,
        bypass_dropped: carry.bypass_dropped,
        logs: log_names(inputs),
        summary_override: None,
        created_ms: now_ms,
        random,
        limits: Limits::default(),
    };
    let to = PacketTo {
        agent: target.name(),
        profile: target.profile().to_string(),
    };
    let packet = packet_for(inputs, &src, &chat_history(chat, old), &to)?;
    let value = serde_json::to_value(&packet).map_err(|e| SwitchError::Internal(e.to_string()))?;
    store
        .save(&packet.id, &value)
        .map_err(|e| SwitchError::Internal(format!("the handoff packet was not stored: {e}")))?;
    Ok(finish(packet))
}

fn message_ids(packet_id: &str) -> Option<(String, String)> {
    let rest = packet_id.strip_prefix("h_")?;
    let old = format!("m_{rest}");
    if !valid_message_id(&old) {
        return None;
    }
    let last = rest.chars().last()?.to_digit(16)?;
    let new = format!("m_{}{:x}", &rest[..rest.len() - 1], last ^ 1);
    Some((old, new))
}

pub fn announce(
    chat: &Chat,
    old: &SessionInfo,
    new_id: &str,
    packet: &HandoffPacket,
    target: Target,
) -> Result<(), SwitchError> {
    let (old_msg, new_msg) = message_ids(&packet.id)
        .ok_or_else(|| SwitchError::Internal("invalid handoff packet id".into()))?;
    let io = |e: String| SwitchError::Internal(e);
    chat.system_once(
        &chat.session_room(&old.id),
        &old_msg,
        packet.created_ms,
        &format!(
            "Switched to {}. Handoff {}. Continues in {new_id}.",
            target.name(),
            packet.id
        ),
    )
    .map_err(io)?;
    let files = packet.files.read.len() + packet.files.edited.len();
    chat.system_once(
        &chat.session_room(new_id),
        &new_msg,
        packet.created_ms,
        &format!(
            "Started from {} session {}. Handoff {}: {} messages, {} todos, {} files.",
            packet.from.agent,
            old.id,
            packet.id,
            packet.messages.len(),
            packet.todos.len(),
            files
        ),
    )
    .map_err(io)?;
    Ok(())
}

pub fn stop(client: &mut Client, s: &SessionInfo, target: Target) -> Result<(), SwitchError> {
    if s.state.kind == StateKind::Exited {
        return Ok(());
    }
    let reason = format!("handed to {}", target.name());
    let io = |e: std::io::Error| SwitchError::Internal(e.to_string());
    client
        .kill_with_reason(&s.id, "int", Some(&reason))
        .map_err(io)?;
    if wait_exit(client, &s.id, Duration::from_secs(10))? {
        return Ok(());
    }
    client
        .kill_with_reason(&s.id, "term", Some(&reason))
        .map_err(io)?;
    if wait_exit(client, &s.id, Duration::from_secs(5))? {
        return Ok(());
    }
    Err(SwitchError::Conflict(format!(
        "{} is still running",
        s.name
    )))
}

fn wait_exit(client: &mut Client, id: &str, limit: Duration) -> Result<bool, SwitchError> {
    let end = Instant::now() + limit;
    loop {
        let list = client
            .list()
            .map_err(|e| SwitchError::Internal(e.to_string()))?;
        if list
            .iter()
            .any(|s| s.id == id && s.state.kind == StateKind::Exited)
        {
            return Ok(true);
        }
        if Instant::now() >= end {
            return Ok(false);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmgr_ipc::api::{State, StateKind};
    use tmgr_session::handoff::Opts as NoteOpts;

    const SECRET: &str = "sk-abcdefghijklmnopqrstuvwx1234";

    fn old(mode: Option<&str>, no_sandbox: bool) -> SessionInfo {
        SessionInfo {
            id: "old1".into(),
            name: "n".into(),
            argv: vec![],
            cwd: "/w".into(),
            profile: Some("opencode".into()),
            state: State {
                kind: StateKind::Exited,
                since_ms: 500,
                detail: None,
                exit_code: Some(0),
            },
            created_ms: 0,
            cols: 80,
            rows: 24,
            shim_pid: 1,
            child_pid: 2,
            task: None,
            bundle_sha256: None,
            origin: None,
            permission_mode: mode.map(str::to_string),
            resumed_from: None,
            human_s: None,
            no_sandbox,
            checkpoints: None,
            permission_cards: false,
            team: None,
            agent_session_id: None,
            limit: None,
            handoff_from: None,
            managed: false,
            manager: false,
        }
    }

    fn all(_: &str, _: &str) -> bool {
        true
    }

    fn inputs(screen: Vec<String>) -> Inputs {
        Inputs {
            sessions: Vec::new(),
            opts: NoteOpts {
                target: "codex",
                task: None,
                previous: Vec::new(),
                git: None,
                rules_files: Vec::new(),
                lookup_hint: None,
            },
            exports: Vec::new(),
            screen,
            source: "opencode",
            task_ref: None,
        }
    }

    fn kit() -> (tempfile::TempDir, HandoffStore, Chat) {
        let t = tempfile::tempdir().unwrap();
        std::fs::set_permissions(
            t.path(),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )
        .unwrap();
        let store = HandoffStore::open(t.path()).unwrap();
        let chat = Chat::open(t.path());
        (t, store, chat)
    }

    fn carry_of(mode: &str) -> Carry {
        Carry {
            mode: mode.into(),
            fell_back: false,
            bypass_dropped: false,
        }
    }

    #[test]
    fn ordinary_modes_carry_over_silently() {
        for mode in ["accept-edits", "plan", "ask"] {
            let c = carry(&old(Some(mode), false), Target::Codex, false, all).unwrap();
            assert_eq!(c, carry_of(mode));
        }
        for odd in [None, Some("odd")] {
            let c = carry(&old(odd, false), Target::Codex, true, all).unwrap();
            assert_eq!(c, carry_of("ask"));
        }
    }

    #[test]
    fn bypass_carries_only_with_the_explicit_flag() {
        let b = old(Some("bypass"), false);
        let c = carry(&b, Target::Codex, false, all).unwrap();
        assert_eq!((c.mode.as_str(), c.bypass_dropped), ("ask", true));
        let c = carry(&b, Target::Codex, true, all).unwrap();
        assert_eq!((c.mode.as_str(), c.bypass_dropped), ("bypass", false));
        let c = carry(&old(Some("ask"), true), Target::Codex, false, all).unwrap();
        assert!(c.bypass_dropped);
        let c = carry(&old(Some("ask"), true), Target::Codex, true, all).unwrap();
        assert!(!c.bypass_dropped);
        assert!(
            !carry(&old(Some("plan"), false), Target::Codex, false, all)
                .unwrap()
                .bypass_dropped
        );
    }

    #[test]
    fn a_target_that_cannot_run_bypass_is_an_error() {
        let b = old(Some("bypass"), false);
        let no_bypass = |_: &str, m: &str| m != "bypass";
        let e = carry(&b, Target::Agy, true, no_bypass).unwrap_err();
        assert!(matches!(&e, SwitchError::Usage(m) if m.contains("agy cannot run in bypass")));
        let c = carry(&b, Target::Agy, false, no_bypass).unwrap();
        assert_eq!(c.mode, "ask");
        assert!(!c.fell_back);
    }

    #[test]
    fn an_unsupported_mode_falls_back_to_ask() {
        let no_plan = |_: &str, m: &str| m != "plan";
        let c = carry(&old(Some("plan"), false), Target::Codex, false, no_plan).unwrap();
        assert_eq!((c.mode.as_str(), c.fell_back), ("ask", true));
        let c = carry(
            &old(Some("accept-edits"), false),
            Target::Codex,
            false,
            no_plan,
        )
        .unwrap();
        assert!(!c.fell_back);
    }

    #[test]
    fn the_target_profile_is_asked_about_the_wanted_mode() {
        let seen = std::cell::RefCell::new(Vec::new());
        let probe = |p: &str, m: &str| {
            seen.borrow_mut().push((p.to_string(), m.to_string()));
            true
        };
        carry(&old(Some("plan"), false), Target::ClaudeCode, false, probe).unwrap();
        assert_eq!(
            seen.into_inner(),
            vec![("claude-code".to_string(), "plan".to_string())]
        );
    }

    #[test]
    fn a_rerun_reuses_the_stored_packet() {
        let (_t, store, chat) = kit();
        let o = old(None, false);
        let i = inputs(vec!["line".into()]);
        let a = build_handoff(
            &store,
            &chat,
            &o,
            &i,
            Target::Codex,
            &carry_of("ask"),
            1000,
            [1; 10],
        )
        .unwrap();
        let b = build_handoff(
            &store,
            &chat,
            &o,
            &i,
            Target::Codex,
            &carry_of("ask"),
            2000,
            [2; 10],
        )
        .unwrap();
        assert_eq!(a.packet_id, b.packet_id);
        assert_eq!(a.prompt, b.prompt);
        assert!(store.get(&a.packet_id).unwrap().is_some());
    }

    #[test]
    fn a_packet_from_before_the_session_stopped_is_not_reused() {
        let (_t, store, chat) = kit();
        let mut o = old(None, false);
        let i = inputs(vec!["line".into()]);
        let a = build_handoff(
            &store,
            &chat,
            &o,
            &i,
            Target::Codex,
            &carry_of("ask"),
            1000,
            [1; 10],
        )
        .unwrap();
        o.state.since_ms = 2000;
        let b = build_handoff(
            &store,
            &chat,
            &o,
            &i,
            Target::Codex,
            &carry_of("ask"),
            3000,
            [2; 10],
        )
        .unwrap();
        assert_ne!(a.packet_id, b.packet_id);
        let c = build_handoff(
            &store,
            &chat,
            &o,
            &i,
            Target::ClaudeCode,
            &carry_of("ask"),
            3000,
            [3; 10],
        )
        .unwrap();
        assert_ne!(b.packet_id, c.packet_id);
    }

    #[test]
    fn the_packet_carries_the_carry_result_and_the_chat_of_the_old_room() {
        let (_t, store, chat) = kit();
        let o = old(Some("plan"), false);
        chat.post(
            chat.session_room(&o.id),
            Chat::owner(),
            "please keep going".into(),
            None,
            Vec::new(),
        )
        .unwrap();
        let c = Carry {
            mode: "ask".into(),
            fell_back: true,
            bypass_dropped: false,
        };
        let h = build_handoff(
            &store,
            &chat,
            &o,
            &inputs(vec![]),
            Target::Codex,
            &c,
            1000,
            [1; 10],
        )
        .unwrap();
        assert_eq!(h.packet.permission.carried_mode.as_deref(), Some("ask"));
        assert!(h.packet.permission.fell_back);
        assert_eq!(h.packet.from.session, "old1");
        assert_eq!(h.packet.to.agent, "codex");
        assert!(h
            .packet
            .messages
            .iter()
            .any(|m| m.role == "owner" && m.text == "please keep going"));
    }

    #[test]
    fn a_secret_in_the_chat_history_never_reaches_the_prompt_or_the_packet() {
        let (t, store, chat) = kit();
        let o = old(None, false);
        chat.post(
            chat.session_room(&o.id),
            Chat::owner(),
            format!("use this key {SECRET} now"),
            None,
            Vec::new(),
        )
        .unwrap();
        let h = build_handoff(
            &store,
            &chat,
            &o,
            &inputs(vec![]),
            Target::Codex,
            &carry_of("ask"),
            1000,
            [1; 10],
        )
        .unwrap();
        assert!(h
            .packet
            .messages
            .iter()
            .any(|m| m.text.contains("use this key")));
        assert!(!h.prompt.contains(SECRET));
        assert!(!serde_json::to_string(&h.packet).unwrap().contains(SECRET));
        let stored = std::fs::read_to_string(
            t.path()
                .join("handoff")
                .join(format!("{}.json", h.packet_id)),
        )
        .unwrap();
        assert!(!stored.contains(SECRET));
    }

    #[test]
    fn the_prompt_fits_the_run_limit_with_three_byte_characters() {
        let (_t, store, chat) = kit();
        let o = old(None, false);
        let wide = "€".repeat(900);
        for _ in 0..60 {
            chat.post(
                chat.session_room(&o.id),
                Chat::owner(),
                wide.clone(),
                None,
                Vec::new(),
            )
            .unwrap();
        }
        let screen: Vec<String> = (0..400).map(|_| "日".repeat(400)).collect();
        let h = build_handoff(
            &store,
            &chat,
            &o,
            &inputs(screen),
            Target::Codex,
            &carry_of("ask"),
            1000,
            [1; 10],
        )
        .unwrap();
        assert!(h.prompt.len() <= MAX_RUN_PROMPT);
        assert!(h.prompt.contains('€') || h.prompt.contains('日'));
    }

    #[test]
    fn announce_writes_one_message_in_each_room_and_only_once() {
        let (_t, store, chat) = kit();
        let o = old(None, false);
        let i = inputs(vec!["line".into()]);
        let h = build_handoff(
            &store,
            &chat,
            &o,
            &i,
            Target::Codex,
            &carry_of("ask"),
            1000,
            [1; 10],
        )
        .unwrap();
        announce(&chat, &o, "new1", &h.packet, Target::Codex).unwrap();
        announce(&chat, &o, "new1", &h.packet, Target::Codex).unwrap();
        let old_room = chat
            .history(&chat.session_room("old1"), None, None)
            .unwrap();
        let new_room = chat
            .history(&chat.session_room("new1"), None, None)
            .unwrap();
        assert_eq!(old_room.len(), 1);
        assert_eq!(new_room.len(), 1);
        assert_eq!(old_room[0].kind, MessageKind::System);
        assert_eq!(
            old_room[0].body_md,
            format!(
                "Switched to codex. Handoff {}. Continues in new1.",
                h.packet_id
            )
        );
        assert_eq!(
            new_room[0].body_md,
            format!(
                "Started from opencode session old1. Handoff {}: 0 messages, 0 todos, 0 files.",
                h.packet_id
            )
        );
        assert_ne!(old_room[0].id, new_room[0].id);
        assert!(valid_message_id(&old_room[0].id) && valid_message_id(&new_room[0].id));
    }

    #[test]
    fn message_ids_are_deterministic_distinct_and_valid() {
        let id = "h_0123456789ab0123456789abcdef01234567";
        assert_eq!(message_ids(id), None);
        let id = "h_0123456789ab0123456789abcdef0123";
        let (a, b) = message_ids(id).unwrap();
        assert_eq!(a, "m_0123456789ab0123456789abcdef0123");
        assert_eq!(b, "m_0123456789ab0123456789abcdef0122");
        assert_eq!(message_ids(id).unwrap(), (a, b));
        assert_eq!(message_ids("h_zz"), None);
        assert_eq!(message_ids("x_0123456789ab0123456789abcdef0123"), None);
    }
}
