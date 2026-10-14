use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

pub const ROOT_ENV: &str = "TMGR_LOCAL_ROOT";
const MANIFEST: &str = "workspace.json";
const DATABASE: &str = "workspace.db";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Manifest {
    pub id: i64,
    pub name: String,
    pub code: String,
    pub schema_version: u32,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LocalWorkspace {
    #[serde(flatten)]
    pub manifest: Manifest,
    pub path: PathBuf,
    pub database: PathBuf,
}

pub fn root() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(ROOT_ENV).filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    std::env::var_os("HOME")
        .filter(|v| !v.is_empty())
        .map(|home| PathBuf::from(home).join(".tmgr.dev").join("workspaces"))
}

/// Same FNV-1a derivation as the desktop, so both agree on a workspace's id.
pub fn local_id(code: &str) -> i64 {
    let hash = code.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| {
        (h ^ b as u64).wrapping_mul(0x0100_0000_01b3)
    });
    -((hash % 1_000_000_000) as i64 + 1)
}

fn read_workspace(dir: &Path) -> Option<LocalWorkspace> {
    let raw = fs::read_to_string(dir.join(MANIFEST)).ok()?;
    let mut manifest: Manifest = serde_json::from_str(&raw).ok()?;
    let code = dir.file_name()?.to_string_lossy().into_owned();
    manifest.id = local_id(&code);
    manifest.code = code;
    Some(LocalWorkspace {
        path: dir.to_path_buf(),
        database: dir.join(DATABASE),
        manifest,
    })
}

pub fn list_in(root: &Path) -> Vec<LocalWorkspace> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut workspaces: Vec<LocalWorkspace> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .filter_map(|e| read_workspace(&e.path()))
        .collect();
    workspaces.sort_by(|a, b| {
        a.manifest
            .name
            .to_lowercase()
            .cmp(&b.manifest.name.to_lowercase())
    });
    workspaces
}

pub fn find(root: &Path, code: &str) -> Result<LocalWorkspace> {
    list_in(root)
        .into_iter()
        .find(|w| w.manifest.code == code)
        .ok_or_else(|| Error::NotFound(format!("local workspace {code}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_id_matches_the_desktop_derivation() {
        let id = local_id("personal");
        assert!((-1_000_000_000..0).contains(&id));
        assert_eq!(id, local_id("personal"));
        assert_ne!(id, local_id("personal-2"));
    }

    #[test]
    fn lists_folders_with_a_manifest_and_takes_the_code_from_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let ws = dir.path().join("beta");
        fs::create_dir_all(&ws).unwrap();
        fs::write(
            ws.join(MANIFEST),
            r#"{"id":1,"name":"Beta","code":"other","schema_version":12,"created_at":"2026-01-01T00:00:00Z"}"#,
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("empty")).unwrap();
        let alpha = dir.path().join("alpha");
        fs::create_dir_all(&alpha).unwrap();
        fs::write(
            alpha.join(MANIFEST),
            r#"{"id":1,"name":"alpha","code":"alpha","schema_version":12,"created_at":"2026-01-01T00:00:00Z"}"#,
        )
        .unwrap();

        let list = list_in(dir.path());
        let codes: Vec<&str> = list.iter().map(|w| w.manifest.code.as_str()).collect();
        assert_eq!(codes, ["alpha", "beta"]);
        assert_eq!(list[1].manifest.id, local_id("beta"));
        assert_eq!(list[1].database, ws.join(DATABASE));
        assert!(find(dir.path(), "beta").is_ok());
        assert!(matches!(find(dir.path(), "other"), Err(Error::NotFound(_))));
    }
}
