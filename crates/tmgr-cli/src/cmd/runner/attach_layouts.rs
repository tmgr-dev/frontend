use std::path::{Path, PathBuf};
use tmgr_core::config::{config_dir, ensure_private_dir, write_atomic};
use tmgr_runner::attach::{parse_layouts, render_layouts, SavedLayout, MAX_LAYOUTS};

const FILE: &str = "layouts.json";

pub(super) fn dir() -> Option<PathBuf> {
    config_dir().ok()
}

pub(super) fn load(dir: &Path) -> (Vec<SavedLayout>, Option<String>) {
    match std::fs::read_to_string(dir.join(FILE)) {
        Ok(text) => match parse_layouts(&text) {
            Ok(list) => (list, None),
            Err(_) => (
                Vec::new(),
                Some("layouts.json is invalid and was ignored; saving replaces it".into()),
            ),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Vec::new(), None),
        Err(_) => (Vec::new(), Some("cannot read layouts.json".into())),
    }
}

pub(super) fn names(layouts: &[SavedLayout]) -> Vec<String> {
    layouts.iter().map(|l| l.name.to_lowercase()).collect()
}

pub(super) fn save(
    dir: &Path,
    layouts: &[SavedLayout],
    seen: &[String],
) -> std::io::Result<Vec<SavedLayout>> {
    ensure_private_dir(dir).map_err(std::io::Error::other)?;
    let path = dir.join(FILE);
    let (disk, unreadable) = match std::fs::read_to_string(&path) {
        Ok(text) => match parse_layouts(&text) {
            Ok(list) => (list, false),
            Err(_) => (Vec::new(), true),
        },
        Err(_) => (Vec::new(), false),
    };
    if unreadable {
        let _ = std::fs::rename(&path, dir.join("layouts.json.bak"));
    }
    let mut merged = layouts.to_vec();
    for other in disk {
        let key = other.name.to_lowercase();
        if merged.len() < MAX_LAYOUTS
            && !merged.iter().any(|l| l.name.to_lowercase() == key)
            && !seen.contains(&key)
        {
            merged.push(other);
        }
    }
    write_atomic(&path, render_layouts(&merged).as_bytes(), 0o600)
        .map_err(std::io::Error::other)?;
    Ok(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmgr_runner::attach::{LayoutPrefs, Member};

    fn layout(name: &str) -> SavedLayout {
        SavedLayout {
            name: name.into(),
            prefs: LayoutPrefs::default(),
            members: vec![Member {
                id: "a".into(),
                name: "claude".into(),
                kind: Some("agent:claude".into()),
                dir: "/work/x".into(),
            }],
        }
    }

    #[test]
    fn a_missing_file_is_empty_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (list, warn) = load(dir.path());
        assert!(list.is_empty() && warn.is_none());
        assert!(!dir.path().join(FILE).exists());
    }

    #[test]
    fn layouts_round_trip_through_a_private_file() {
        let dir = tempfile::tempdir().unwrap();
        let list = vec![layout("tmgr"), layout("backend")];
        save(dir.path(), &list, &[]).unwrap();
        assert_eq!(load(dir.path()).0, list);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join(FILE))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn an_invalid_file_is_ignored_with_a_warning_and_kept_as_a_backup_on_save() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE), "{ nope").unwrap();
        let (list, warn) = load(dir.path());
        assert!(list.is_empty() && warn.is_some());
        assert_eq!(
            std::fs::read_to_string(dir.path().join(FILE)).unwrap(),
            "{ nope"
        );
        save(dir.path(), &[layout("x")], &[]).unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("layouts.json.bak")).unwrap(),
            "{ nope"
        );
        assert_eq!(load(dir.path()).0.len(), 1);
    }

    #[test]
    fn a_save_keeps_layouts_another_window_added_but_not_ones_we_deleted() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), &[layout("one"), layout("two")], &[]).unwrap();
        let seen = names(&[layout("one"), layout("two")]);
        save(
            dir.path(),
            &[layout("one"), layout("two"), layout("other")],
            &[],
        )
        .unwrap();
        let merged = save(dir.path(), &[layout("one")], &seen).unwrap();
        let got: Vec<&str> = merged.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(got, ["one", "other"]);
        assert_eq!(load(dir.path()).0.len(), 2);
    }
}
