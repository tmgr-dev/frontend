use serde::{Deserialize, Serialize};
use tmgr_ipc::chat::RoomRef;
use tmgr_ipc::mgr_rpc::valid_machine_id;

pub const ROOMS_V: u8 = 1;
pub const MAX_ROOMS: usize = 200;
pub const MAX_LABEL_CHARS: usize = 64;
pub const MAX_FOLLOWERS: usize = 200;
pub const MAX_AGENT_CHARS: usize = 64;

pub fn valid_agent(agent: &str) -> bool {
    (1..=MAX_AGENT_CHARS).contains(&agent.chars().count()) && !agent.chars().any(char::is_control)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Top,
    Sub,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomEntry {
    pub room: RoomRef,
    pub title: String,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub context_pct: Option<u8>,
    #[serde(default)]
    pub last_ts: Option<u64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub mention_ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoomsIndex {
    pub v: u8,
    pub machine: String,
    pub machine_label: String,
    pub role: Role,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub followers: Option<Vec<String>>,
    pub rooms: Vec<RoomEntry>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub activity: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub blob: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub terminal_input: bool,
}

pub fn valid_index_msg_id(id: &str) -> bool {
    id.len() == 34
        && id.starts_with("i_")
        && id[2..]
            .bytes()
            .all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

impl RoomsIndex {
    pub fn validate(&self) -> Result<(), String> {
        if self.v != ROOMS_V {
            return Err("unsupported rooms version".into());
        }
        if !valid_machine_id(&self.machine) {
            return Err("invalid machine id".into());
        }
        let label = self.machine_label.chars().count();
        if label == 0 || label > MAX_LABEL_CHARS || self.machine_label.chars().any(char::is_control)
        {
            return Err("invalid machine label".into());
        }
        match (self.role, &self.followers) {
            (Role::Sub, Some(_)) => return Err("a sub runner has no followers".into()),
            (Role::Top, Some(f))
                if f.len() > MAX_FOLLOWERS || !f.iter().all(|m| valid_machine_id(m)) =>
            {
                return Err("invalid followers".into());
            }
            _ => {}
        }
        if self.rooms.len() > MAX_ROOMS {
            return Err("too many rooms".into());
        }
        for r in &self.rooms {
            r.room.validate()?;
            if r.room.machine != self.machine {
                return Err("a room belongs to another machine".into());
            }
            if r.agent.as_deref().is_some_and(|a| !valid_agent(a)) {
                return Err("invalid agent".into());
            }
        }
        Ok(())
    }

    pub fn parse(value: serde_json::Value) -> Result<RoomsIndex, String> {
        let idx: RoomsIndex = serde_json::from_value(value).map_err(|e| e.to_string())?;
        idx.validate()?;
        Ok(idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    const FIXTURE: &str = include_str!("../tests/fixtures/rooms_v1.json");

    fn fixture() -> Value {
        serde_json::from_str(FIXTURE).unwrap()
    }

    fn with(f: impl FnOnce(&mut Value)) -> Value {
        let mut v = fixture();
        f(&mut v);
        v
    }

    #[test]
    fn the_golden_fixture_parses_validates_and_round_trips() {
        let idx = RoomsIndex::parse(fixture()).unwrap();
        assert_eq!(idx.role, Role::Top);
        assert_eq!(idx.rooms.len(), 2);
        assert_eq!(serde_json::to_value(&idx).unwrap(), fixture());
    }

    #[test]
    fn unknown_fields_are_ignored_for_forward_compat() {
        let v = with(|v| {
            v["future"] = json!(1);
            v["rooms"][0]["extra"] = json!("x");
        });
        assert!(RoomsIndex::parse(v).is_ok());
    }

    #[test]
    fn nullable_fields_may_be_absent() {
        let v = with(|v| {
            v["rooms"][1].as_object_mut().unwrap().remove("state");
            v["rooms"][1].as_object_mut().unwrap().remove("last_ts");
        });
        assert!(RoomsIndex::parse(v).is_ok());
    }

    #[test]
    fn invalid_indexes_are_refused() {
        let long = "x".repeat(65);
        let many: Vec<Value> = (0..201)
            .map(|i| json!({"room": {"kind": "session", "machine": "mx_0123456789abcdef", "session": format!("s{i}")}, "title": "t"}))
            .collect();
        let cases = [
            with(|v| v["v"] = json!(2)),
            with(|v| v["machine"] = json!("mx_nothex")),
            with(|v| v["machine_label"] = json!("")),
            with(|v| v["machine_label"] = json!(long)),
            with(|v| v["machine_label"] = json!("a\nb")),
            with(|v| v["role"] = json!("middle")),
            with(|v| v["role"] = json!("sub")),
            with(|v| v["followers"] = json!(["nope"])),
            with(|v| v["rooms"][0]["room"]["machine"] = json!("mx_aaaaaaaaaaaaaaaa")),
            with(|v| v["rooms"][1]["room"]["session"] = json!("bad id")),
            with(|v| v["rooms"] = json!(many)),
            with(|v| v["rooms"][0]["context_pct"] = json!(300)),
        ];
        for (i, c) in cases.into_iter().enumerate() {
            assert!(RoomsIndex::parse(c).is_err(), "case {i}");
        }
    }

    #[test]
    fn agent_and_context_tokens_are_optional_and_round_trip_when_set() {
        let idx = RoomsIndex::parse(fixture()).unwrap();
        assert_eq!(
            (idx.rooms[0].agent.clone(), idx.rooms[0].context_tokens),
            (None, None)
        );
        let on = with(|v| {
            v["rooms"][1]["agent"] = json!("claude-code");
            v["rooms"][1]["context_tokens"] = json!(64_600);
        });
        let idx = RoomsIndex::parse(on.clone()).unwrap();
        assert_eq!(idx.rooms[1].agent.as_deref(), Some("claude-code"));
        assert_eq!(idx.rooms[1].context_tokens, Some(64_600));
        assert_eq!(serde_json::to_value(&idx).unwrap(), on);
    }

    #[test]
    fn a_long_or_control_agent_is_refused() {
        for agent in ["x".repeat(65), "a\nb".into(), String::new()] {
            let v = with(|v| v["rooms"][1]["agent"] = json!(agent));
            assert!(RoomsIndex::parse(v).is_err(), "{agent:?}");
        }
    }

    #[test]
    fn a_sub_without_followers_is_valid_and_a_top_without_followers_too() {
        let sub = with(|v| {
            v["role"] = json!("sub");
            v.as_object_mut().unwrap().remove("followers");
        });
        assert!(RoomsIndex::parse(sub).is_ok());
        let top = with(|v| {
            v.as_object_mut().unwrap().remove("followers");
        });
        assert!(RoomsIndex::parse(top).is_ok());
    }

    #[test]
    fn the_activity_flag_defaults_to_false_and_round_trips_when_set() {
        let idx = RoomsIndex::parse(fixture()).unwrap();
        assert!(!idx.activity);
        assert!(fixture().get("activity").is_none());
        let on = with(|v| v["activity"] = json!(true));
        let idx = RoomsIndex::parse(on.clone()).unwrap();
        assert!(idx.activity);
        assert_eq!(serde_json::to_value(&idx).unwrap(), on);
    }

    #[test]
    fn the_blob_flag_defaults_to_false_and_round_trips_when_set() {
        let idx = RoomsIndex::parse(fixture()).unwrap();
        assert!(!idx.blob);
        assert!(fixture().get("blob").is_none());
        let on = with(|v| v["blob"] = json!(true));
        let idx = RoomsIndex::parse(on.clone()).unwrap();
        assert!(idx.blob);
        assert_eq!(serde_json::to_value(&idx).unwrap(), on);
    }

    #[test]
    fn the_terminal_input_flag_defaults_to_false_and_round_trips_when_set() {
        let idx = RoomsIndex::parse(fixture()).unwrap();
        assert!(!idx.terminal_input);
        assert!(fixture().get("terminal_input").is_none());
        let on = with(|v| v["terminal_input"] = json!(true));
        let idx = RoomsIndex::parse(on.clone()).unwrap();
        assert!(idx.terminal_input);
        assert_eq!(serde_json::to_value(&idx).unwrap(), on);
    }
}
