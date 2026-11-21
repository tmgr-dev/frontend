use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffPrepareReq {
    pub session: String,
    pub to: String,
    #[serde(default)]
    pub keep_bypass: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffPrepareResp {
    pub packet_id: String,
    pub prompt: String,
    pub mode: String,
    pub fell_back: bool,
    pub bypass_dropped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffFinishReq {
    pub packet_id: String,
    pub new_session: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HandoffFinishResp {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}
