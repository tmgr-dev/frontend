use crate::out::Output;
use serde_json::json;
use tmgr_chat_relay::api::Request;
use tmgr_chat_relay::engine::{DeviceState, EngineConfig, RelayEngine, Transport, TrustState};
use tmgr_core::error::{Error, Result};
use tmgr_runner::chat::relay::store::{RelayConfig, RelayDir};

struct Offline;

impl Transport for Offline {
    fn call(&self, _: &Request, _: Option<&str>) -> std::result::Result<String, String> {
        Err("status does not use the network".into())
    }
}

struct NoSink;

impl tmgr_chat_relay::engine::ChatSink for NoSink {
    fn deliver(&self, _: &str, _: tmgr_ipc::chat::ChatMessage) -> std::result::Result<(), String> {
        Err("status does not take chat".into())
    }
}

pub fn render(st: &TrustState, joined: bool, cfg: Option<&RelayConfig>) -> Output {
    let trusted = st
        .devices
        .iter()
        .filter(|d| d.state == DeviceState::Trusted)
        .count();
    let device_id = st.devices.iter().find(|d| d.is_self).map(|d| d.id.clone());
    let (state, human) = match (st.registered, joined, cfg.is_some()) {
        (false, _, _) => ("not_set_up", "Not paired. Run: tmgr chat join"),
        (true, false, _) => (
            "pending",
            "Waiting for a trusted device to approve the join. Run: tmgr chat join",
        ),
        (true, true, false) => (
            "unfinished",
            "Joined, but not finished. Run: tmgr chat join",
        ),
        (true, true, true) => ("joined", "Paired."),
    };
    let rows = format!(
        "{human}\nrelay device: {}\ntrusted devices: {trusted}",
        device_id.as_deref().unwrap_or("-")
    );
    Output::new(
        json!({"state": state, "device_id": device_id, "trusted_devices": trusted}),
        rows,
    )
}

pub fn run() -> Result<Output> {
    let dir =
        tmgr_ipc::dirs::runner_dir().map_err(|e| Error::internal(format!("runner dir: {e}")))?;
    if !dir.join("relay").exists() {
        return Ok(render(&TrustState::default(), false, None));
    }
    let relay_dir = RelayDir::open(&dir).map_err(Error::internal)?;
    let cfg = relay_dir.load_config().map_err(Error::internal)?;
    let engine_cfg = EngineConfig {
        user_id: cfg.as_ref().map_or(0, |c| c.user_id),
        label: String::new(),
        local_machine: String::new(),
    };
    let engine =
        RelayEngine::new(Offline, relay_dir, NoSink, engine_cfg).map_err(Error::internal)?;
    let st = engine.trust_state().map_err(Error::internal)?;
    Ok(render(&st, engine.is_joined(), cfg.as_ref()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmgr_chat_relay::engine::TrustDevice;

    fn dev(id: &str, is_self: bool, state: DeviceState) -> TrustDevice {
        TrustDevice {
            id: id.into(),
            kind: "runner".into(),
            label: "l".into(),
            fp: "ab".repeat(32),
            state,
            is_self,
            kind_verified: true,
            can_confirm_kind: false,
        }
    }

    fn cfg() -> RelayConfig {
        RelayConfig {
            server: "https://tmgr.dev".into(),
            user_id: 1,
        }
    }

    #[test]
    fn not_registered_says_to_join() {
        let out = render(&TrustState::default(), false, None);
        assert_eq!(out.data["state"], "not_set_up");
        assert!(out.human.contains("tmgr chat join"));
    }

    #[test]
    fn joined_shows_id_and_trusted_count_but_never_fingerprints() {
        let st = TrustState {
            registered: true,
            devices: vec![
                dev("me", true, DeviceState::Trusted),
                dev("a", false, DeviceState::Trusted),
                dev("b", false, DeviceState::Revoked),
            ],
            ..TrustState::default()
        };
        let out = render(&st, true, Some(&cfg()));
        assert_eq!(out.data["state"], "joined");
        assert_eq!(out.data["device_id"], "me");
        assert_eq!(out.data["trusted_devices"], 2);
        let all = format!("{} {}", out.human, out.data);
        assert!(!all.contains(&"ab".repeat(32)));
    }

    #[test]
    fn registered_but_not_joined_is_pending() {
        let st = TrustState {
            registered: true,
            devices: vec![dev("me", true, DeviceState::Trusted)],
            ..TrustState::default()
        };
        assert_eq!(render(&st, false, None).data["state"], "pending");
        assert_eq!(render(&st, true, None).data["state"], "unfinished");
    }
}
