use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tmgr_core::config;

pub const FILE: &str = "board-filters.json";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Assignee {
    #[default]
    All,
    Me,
    User {
        id: i64,
        name: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PersonaSel {
    #[default]
    Any,
    Mine,
    One {
        id: String,
        name: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Filters {
    pub project: Option<String>,
    pub assignee: Assignee,
    pub persona: PersonaSel,
    #[serde(skip)]
    pub search: String,
}

impl Filters {
    pub fn active(&self) -> usize {
        usize::from(self.project.is_some())
            + usize::from(self.assignee != Assignee::All)
            + usize::from(self.persona != PersonaSel::Any)
            + usize::from(!self.search.trim().is_empty())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Saved {
    pub workspaces: BTreeMap<String, Filters>,
}

pub fn key(code: &str) -> String {
    if code.starts_with(tmgr_core::local::PREFIX) {
        code.to_string()
    } else {
        code.to_ascii_lowercase()
    }
}

pub fn path(config_dir: &Path) -> PathBuf {
    config_dir.join(FILE)
}

pub fn load(file: &Path) -> Saved {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

pub fn save(file: &Path, code: &str, filters: &Filters) {
    let mut filters = filters.clone();
    filters.search.clear();
    let mut saved = match std::fs::read_to_string(file) {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_else(|_| {
            let _ = std::fs::rename(file, file.with_extension("json.bad"));
            Saved::default()
        }),
        Err(_) => Saved::default(),
    };
    if filters == Filters::default() {
        saved.workspaces.remove(&key(code));
    } else {
        saved.workspaces.insert(key(code), filters);
    }
    if let Ok(bytes) = serde_json::to_vec_pretty(&saved) {
        let _ = config::write_atomic(file, &bytes, 0o600);
    }
}
