use super::cmdline::Values;
use super::model::Node;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const LIMIT: usize = 40;
const FILE: &str = "ui-history.json";
const NEVER_RECORDED: &[&[&str]] = &[
    &["login"],
    &["auth"],
    &["config"],
    &["link"],
    &["notify"],
    &["alarm"],
    &["agents", "login"],
    &["agents", "logout"],
    &["agents", "sync"],
    &["usage", "accounts"],
    &["usage", "sync"],
    &["mcp"],
];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recent {
    pub path: Vec<String>,
    pub values: Values,
}

pub fn recordable(node: &Node) -> bool {
    !NEVER_RECORDED
        .iter()
        .any(|n| node.path.len() >= n.len() && n.iter().zip(&node.path).all(|(a, b)| a == b))
}

pub fn scrub(node: &Node, values: &Values) -> Values {
    values
        .iter()
        .filter(|(id, _)| {
            node.fields
                .iter()
                .find(|f| &f.id == *id)
                .is_some_and(|f| !f.sensitive)
        })
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

pub fn default_path() -> Option<PathBuf> {
    tmgr_core::config::config_dir().ok().map(|d| d.join(FILE))
}

pub fn load(path: &Path) -> Vec<Recent> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn record(path: &Path, node: &Node, values: &Values) {
    if !recordable(node) {
        return;
    }
    let entry = Recent {
        path: node.path.clone(),
        values: scrub(node, values),
    };
    let mut all = load(path);
    all.retain(|r| *r != entry);
    all.insert(0, entry);
    all.truncate(LIMIT);
    let _ = save(path, &all);
}

fn save(path: &Path, all: &[Recent]) -> std::io::Result<()> {
    use std::io::Write;
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let _ = tmgr_core::config::ensure_private_dir(dir);
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(serde_json::to_string(all)?.as_bytes())?;
    tmp.as_file().sync_all()?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}
