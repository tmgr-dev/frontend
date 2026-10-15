use crate::wire::write_atomic;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tmgr_ipc::api::TrustedFolder;

const FILE: &str = ".trusted-folders.json";
const MAX_FILE: u64 = 1 << 20;
const MAX_FOLDERS: usize = 1000;
const MAX_ANSWER_FILE: u64 = 8 << 20;

pub const HOW_COMMAND: &str = "command";
pub const HOW_ANSWERED: &str = "answered";

static LOCK: Mutex<()> = Mutex::new(());

pub fn store(homes: &Path) -> PathBuf {
    homes.join(FILE)
}

fn read_capped(path: &Path, max: u64) -> Option<Vec<u8>> {
    let f = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let m = f.metadata().ok()?;
    if !m.is_file() || m.nlink() != 1 || m.len() > max {
        return None;
    }
    let mut bytes = Vec::new();
    f.take(max + 1).read_to_end(&mut bytes).ok()?;
    (bytes.len() as u64 <= max).then_some(bytes)
}

pub fn list(homes: &Path) -> Vec<TrustedFolder> {
    read_capped(&store(homes), MAX_FILE)
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save(homes: &Path, folders: &[TrustedFolder]) -> Result<(), String> {
    let bytes = serde_json::to_vec_pretty(folders).map_err(|e| e.to_string())?;
    write_atomic(&store(homes), &bytes).map_err(|e| format!("cannot save trusted folders: {e}"))
}

fn identify(path: &Path) -> Result<(PathBuf, u64, u64), String> {
    let canon = std::fs::canonicalize(path)
        .map_err(|_| format!("{} is not an existing folder", path.display()))?;
    let m = std::fs::metadata(&canon).map_err(|e| e.to_string())?;
    if !m.is_dir() {
        return Err(format!("{} is not a folder", canon.display()));
    }
    Ok((canon, m.dev(), m.ino()))
}

fn insert(homes: &Path, folder: TrustedFolder) -> Result<TrustedFolder, String> {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut all = list(homes);
    all.retain(|f| f.path != folder.path);
    if all.len() >= MAX_FOLDERS {
        return Err(format!("at most {MAX_FOLDERS} trusted folders"));
    }
    all.push(folder.clone());
    save(homes, &all)?;
    Ok(folder)
}

pub fn add(homes: &Path, path: &Path, now_ms: u64) -> Result<TrustedFolder, String> {
    if !path.is_absolute() {
        return Err("the folder must be an absolute path".into());
    }
    let (canon, dev, ino) = identify(path)?;
    let path = canon
        .to_str()
        .ok_or("the folder path is not valid UTF-8")?
        .to_string();
    insert(
        homes,
        TrustedFolder {
            path,
            dev,
            ino,
            added_ms: now_ms,
            how: HOW_COMMAND.into(),
        },
    )
}

pub fn remove(homes: &Path, path: &str) -> Result<bool, String> {
    let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let canon = std::fs::canonicalize(path)
        .ok()
        .and_then(|p| p.to_str().map(str::to_string));
    let mut all = list(homes);
    let before = all.len();
    all.retain(|f| f.path != path && Some(&f.path) != canon.as_ref());
    if all.len() == before {
        return Ok(false);
    }
    save(homes, &all)?;
    Ok(true)
}

pub fn is_trusted(homes: &Path, path: &Path, id: (u64, u64)) -> bool {
    let Some(path) = path.to_str() else {
        return false;
    };
    list(homes)
        .iter()
        .any(|f| f.path == path && (f.dev, f.ino) == id)
}

/// The agent's own record that a person accepted its folder-trust question, read from the run HOME
/// the runner gave it. Only called for runs whose trust the runner did not answer itself.
pub fn answered(home: &Path, file: &str, path: &[String], cwd: &str) -> bool {
    let Some(bytes) = read_capped(&home.join(file), MAX_ANSWER_FILE) else {
        return false;
    };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return false;
    };
    let mut cur = &v;
    for seg in path {
        let key = if seg == "{cwd}" { cwd } else { seg.as_str() };
        match cur.get(key) {
            Some(next) => cur = next,
            None => return false,
        }
    }
    cur.as_bool() == Some(true)
}

pub fn learn(homes: &Path, cwd: &str, id: (u64, u64), now_ms: u64) -> Result<(), String> {
    let (canon, dev, ino) = identify(Path::new(cwd))?;
    if canon.to_str() != Some(cwd) || (dev, ino) != id {
        return Err("the folder changed since the run started".into());
    }
    insert(
        homes,
        TrustedFolder {
            path: cwd.to_string(),
            dev,
            ino,
            added_ms: now_ms,
            how: HOW_ANSWERED.into(),
        },
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn dirs() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let t = tempfile::tempdir().unwrap();
        let homes = t.path().join("homes");
        let work = t.path().join("work");
        std::fs::create_dir(&homes).unwrap();
        std::fs::create_dir(&work).unwrap();
        let work = std::fs::canonicalize(work).unwrap();
        (t, homes, work)
    }

    fn id(p: &Path) -> (u64, u64) {
        let m = std::fs::metadata(p).unwrap();
        (m.dev(), m.ino())
    }

    #[test]
    fn a_folder_is_trusted_by_path_and_identity_and_stored_private() {
        let (_t, homes, work) = dirs();
        let f = add(&homes, &work, 5).unwrap();
        assert_eq!(
            (f.path.as_str(), f.how.as_str()),
            (work.to_str().unwrap(), "command")
        );
        assert!(is_trusted(&homes, &work, id(&work)));
        assert!(!is_trusted(&homes, &work, (0, 0)));
        let sub = work.join("sub");
        std::fs::create_dir(&sub).unwrap();
        assert!(!is_trusted(&homes, &sub, id(&sub)));
        let mode = std::fs::metadata(store(&homes))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        add(&homes, &work, 6).unwrap();
        assert_eq!(list(&homes).len(), 1);
    }

    #[test]
    fn a_replaced_folder_at_the_same_path_is_not_trusted() {
        let (_t, homes, work) = dirs();
        add(&homes, &work, 1).unwrap();
        let old = id(&work);
        std::fs::rename(&work, work.with_extension("old")).unwrap();
        std::fs::create_dir(&work).unwrap();
        assert!(is_trusted(&homes, &work, old));
        assert!(!is_trusted(&homes, &work, id(&work)));
    }

    #[test]
    fn untrust_works_by_path_even_when_the_folder_is_gone() {
        let (_t, homes, work) = dirs();
        add(&homes, &work, 1).unwrap();
        std::fs::remove_dir(&work).unwrap();
        assert!(remove(&homes, work.to_str().unwrap()).unwrap());
        assert!(!remove(&homes, work.to_str().unwrap()).unwrap());
        assert!(list(&homes).is_empty());
    }

    #[test]
    fn relative_and_missing_folders_are_refused() {
        let (_t, homes, work) = dirs();
        assert!(add(&homes, Path::new("rel"), 1).is_err());
        assert!(add(&homes, &work.join("nope"), 1).is_err());
        std::fs::write(work.join("f"), "").unwrap();
        assert!(add(&homes, &work.join("f"), 1).is_err());
    }

    #[test]
    fn a_symlinked_or_linked_store_is_not_read() {
        let (_t, homes, work) = dirs();
        let other = work.join("other.json");
        std::fs::write(
            &other,
            r#"[{"path":"/x","dev":1,"ino":2,"added_ms":0,"how":"command"}]"#,
        )
        .unwrap();
        std::os::unix::fs::symlink(&other, store(&homes)).unwrap();
        assert!(list(&homes).is_empty());
    }

    #[test]
    fn the_answer_is_read_only_from_a_plain_file_for_this_folder() {
        let (_t, homes, work) = dirs();
        let home = homes.join("s_1");
        std::fs::create_dir(&home).unwrap();
        let path: Vec<String> = ["projects", "{cwd}", "hasTrustDialogAccepted"]
            .map(String::from)
            .to_vec();
        let cwd = work.to_str().unwrap();
        assert!(!answered(&home, ".claude.json", &path, cwd));
        let yes = serde_json::json!({"projects": {cwd: {"hasTrustDialogAccepted": true}}});
        std::fs::write(home.join("real.json"), yes.to_string()).unwrap();
        std::os::unix::fs::symlink(home.join("real.json"), home.join(".claude.json")).unwrap();
        assert!(!answered(&home, ".claude.json", &path, cwd));
        std::fs::remove_file(home.join(".claude.json")).unwrap();
        std::fs::hard_link(home.join("real.json"), home.join(".claude.json")).unwrap();
        assert!(!answered(&home, ".claude.json", &path, cwd));
        std::fs::remove_file(home.join("real.json")).unwrap();
        assert!(answered(&home, ".claude.json", &path, cwd));
        assert!(!answered(&home, ".claude.json", &path, "/elsewhere"));
        let no = serde_json::json!({"projects": {cwd: {"hasTrustDialogAccepted": "yes"}}});
        std::fs::write(home.join(".claude.json"), no.to_string()).unwrap();
        assert!(!answered(&home, ".claude.json", &path, cwd));
    }

    #[test]
    fn learning_needs_the_same_folder() {
        let (_t, homes, work) = dirs();
        let cwd = work.to_str().unwrap();
        assert!(learn(&homes, cwd, (0, 0), 1).is_err());
        learn(&homes, cwd, id(&work), 1).unwrap();
        assert_eq!(list(&homes)[0].how, "answered");
        assert!(is_trusted(&homes, &work, id(&work)));
    }
}
