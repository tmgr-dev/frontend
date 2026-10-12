use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct Workspace {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub is_default: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Status {
    pub id: u64,
    #[serde(default)]
    pub name: String,
    #[serde(default, rename = "type")]
    pub kind: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Category {
    pub id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub code: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Person {
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiTask {
    pub id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub status_id: Option<u64>,
    #[serde(default)]
    pub project_category_id: Option<u64>,
    #[serde(default)]
    pub category_tasks_sequence_id: Option<u64>,
    #[serde(default)]
    pub category: Option<Category>,
    #[serde(default)]
    pub workspace_id: Option<u64>,
    #[serde(default)]
    pub common_time: Option<i64>,
    #[serde(default)]
    pub approximately_time: Option<i64>,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub assignees: Vec<Person>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiComment {
    pub id: u64,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub user: Option<Person>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    #[serde(default)]
    pub verification_uri_complete: Option<String>,
    #[serde(default = "default_expires")]
    pub expires_in: u64,
    #[serde(default = "default_interval")]
    pub interval: u64,
}

fn default_expires() -> u64 {
    600
}

fn default_interval() -> u64 {
    5
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceUser {
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DeviceInfo {
    #[serde(default)]
    pub device_id: Option<serde_json::Value>,
    pub user: DeviceUser,
    #[serde(default)]
    pub scopes: Vec<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
}
