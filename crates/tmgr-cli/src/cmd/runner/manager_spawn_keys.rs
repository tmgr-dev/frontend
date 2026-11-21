use super::{connect, emit, rerr, runner_config, Opts};
use chrono::DateTime;
use clap::Subcommand;
use serde_json::{json, Value};
use tmgr_core::error::Result;
use tmgr_ipc::api::{
    method, SpawnKeyInfo, SpawnKeysApproveParams, SpawnKeysApproveResult, SpawnKeysList,
    SpawnKeysRevokeParams, SpawnKeysRevokeResult,
};

#[derive(Subcommand)]
pub enum SpawnKeysCmd {
    /// [experimental] List the spawn keys published by your phones
    List,
    /// [experimental] Trust a pending spawn key after comparing its fingerprint on the phone
    Approve {
        /// First 8 or more characters of the device fingerprint
        device_fp: String,
        /// First 16 or more hex characters of the spawn key fingerprint
        spawn_key_fp: String,
    },
    /// [experimental] Stop trusting a device's spawn key and refuse that key from now on
    Revoke {
        /// First 8 or more characters of the device fingerprint
        device_fp: String,
    },
}

pub(super) fn run(o: &Opts, cmd: &SpawnKeysCmd) -> Result<i32> {
    let cfg = runner_config()?;
    let mut c = connect(&cfg)?;
    match cmd {
        SpawnKeysCmd::List => {
            let v = c.call(method::SPAWN_KEYS_LIST, Value::Null).map_err(rerr)?;
            let list: SpawnKeysList = serde_json::from_value(v).map_err(bad_reply)?;
            let human = format_list(&list.keys);
            Ok(emit(o, list, human))
        }
        SpawnKeysCmd::Approve {
            device_fp,
            spawn_key_fp,
        } => {
            let params = SpawnKeysApproveParams {
                device_fp: device_fp.clone(),
                spawn_key_fp: spawn_key_fp.clone(),
            };
            let v = c
                .call(method::SPAWN_KEYS_APPROVE, json!(params))
                .map_err(rerr)?;
            let r: SpawnKeysApproveResult = serde_json::from_value(v).map_err(bad_reply)?;
            let human = approved_text(&r);
            Ok(emit(o, r, human))
        }
        SpawnKeysCmd::Revoke { device_fp } => {
            let params = SpawnKeysRevokeParams {
                device_fp: device_fp.clone(),
            };
            let v = c
                .call(method::SPAWN_KEYS_REVOKE, json!(params))
                .map_err(rerr)?;
            let r: SpawnKeysRevokeResult = serde_json::from_value(v).map_err(bad_reply)?;
            let human = revoked_text(&r);
            Ok(emit(o, r, human))
        }
    }
}

fn bad_reply(e: serde_json::Error) -> tmgr_core::error::Error {
    tmgr_core::error::Error::internal(format!("bad reply from the runner: {e}"))
}

fn groups(hex: &str) -> String {
    let head: Vec<char> = hex.chars().take(16).collect();
    head.chunks(4)
        .map(|c| c.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join(" ")
}

fn when(ms: u64) -> String {
    i64::try_from(ms)
        .ok()
        .and_then(DateTime::from_timestamp_millis)
        .map_or_else(
            || "unknown".to_string(),
            |t| t.format("%Y-%m-%d %H:%M UTC").to_string(),
        )
}

fn short(fp: &str) -> String {
    fp.chars().take(8).collect()
}

fn format_list(keys: &[SpawnKeyInfo]) -> String {
    if keys.is_empty() {
        return "No spawn keys. Publish one from your phone first.".into();
    }
    keys.iter()
        .map(|k| {
            let fp: String = k.device_fp.chars().take(16).collect();
            format!(
                "{fp}  {}  {}  {}  {}",
                k.device_id,
                groups(&k.spawn_key_fp),
                k.state,
                when(k.issued_at_ms)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn approved_text(r: &SpawnKeysApproveResult) -> String {
    format!("Approved the spawn key of device {}…", short(&r.device_fp))
}

fn revoked_text(r: &SpawnKeysRevokeResult) -> String {
    format!(
        "Revoked the {} spawn key {} of device {}… ({}). That key is refused from now on; a new key from the phone arrives as pending.",
        r.state,
        groups(&r.spawn_key_fp),
        short(&r.device_fp),
        r.device_id
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct W {
        #[command(subcommand)]
        c: SpawnKeysCmd,
    }

    fn parse(args: &[&str]) -> std::result::Result<SpawnKeysCmd, clap::Error> {
        W::try_parse_from(std::iter::once("t").chain(args.iter().copied())).map(|w| w.c)
    }

    fn spawn_fp() -> String {
        ["0123", "4567", "89ab", "cdef"].concat().repeat(4)
    }

    fn key(state: &str) -> SpawnKeyInfo {
        SpawnKeyInfo {
            device_fp: "ab".repeat(32),
            device_id: "phone-1".into(),
            spawn_key_fp: spawn_fp(),
            state: state.into(),
            issued_at_ms: 1_795_000_000_000,
            replaced_at_ms: None,
        }
    }

    #[test]
    fn subcommands_take_the_documented_arguments() {
        assert!(matches!(parse(&["list"]).unwrap(), SpawnKeysCmd::List));
        assert!(matches!(
            parse(&["approve", "abcdef01", "0123456789abcdef"]).unwrap(),
            SpawnKeysCmd::Approve { .. }
        ));
        assert!(matches!(
            parse(&["revoke", "abcdef01"]).unwrap(),
            SpawnKeysCmd::Revoke { .. }
        ));
        assert!(parse(&["approve", "abcdef01"]).is_err());
        assert!(parse(&["revoke"]).is_err());
    }

    #[test]
    fn list_shows_fingerprints_in_groups_state_and_time() {
        let text = format_list(&[key("pending"), key("approved")]);
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("abababababababab  phone-1  0123 4567 89ab cdef  pending  "));
        assert!(lines[0].ends_with("UTC"));
        assert!(lines[1].contains("  approved  "));
        assert_eq!(
            format_list(&[]),
            "No spawn keys. Publish one from your phone first."
        );
    }

    #[test]
    fn approve_and_revoke_texts_name_the_device() {
        let a = approved_text(&SpawnKeysApproveResult {
            device_fp: "ab".repeat(32),
        });
        assert_eq!(a, "Approved the spawn key of device abababab…");
        let r = revoked_text(&SpawnKeysRevokeResult {
            device_fp: "ab".repeat(32),
            device_id: "phone-1".into(),
            spawn_key_fp: spawn_fp(),
            state: "approved".into(),
        });
        assert!(r.starts_with(
            "Revoked the approved spawn key 0123 4567 89ab cdef of device abababab… (phone-1)."
        ));
    }
}
