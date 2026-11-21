use super::SessionInfo;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SwitchReq {
    pub session: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SwitchResp {
    pub new_session: SessionInfo,
    pub packet_id: String,
    #[serde(default)]
    pub warnings: Vec<String>,
}
