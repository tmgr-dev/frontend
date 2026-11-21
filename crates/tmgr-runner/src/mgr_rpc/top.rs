use super::files::write_private_atomic;
use super::policy::read_private;
use std::path::Path;

pub const TOP_FILE: &str = "manager-top";
const MAX_TOP_BYTES: u64 = 16;

pub fn is_top(runner_dir: &Path) -> bool {
    match read_private(runner_dir, TOP_FILE, MAX_TOP_BYTES) {
        Ok(text) => text.trim() == "1",
        Err(e) => {
            if e.kind() != std::io::ErrorKind::NotFound {
                crate::daemon::log(&format!("manager top flag ignored: {e}"));
            }
            false
        }
    }
}

pub fn apply_top(runner_dir: &Path, choice: Option<bool>) -> std::io::Result<()> {
    match choice {
        None => Ok(()),
        Some(true) => write_private_atomic(runner_dir, TOP_FILE, b"1\n"),
        Some(false) => match std::fs::remove_file(runner_dir.join(TOP_FILE)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{symlink, PermissionsExt};

    fn dir() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("mt")
            .tempdir_in("/tmp")
            .unwrap()
    }

    fn put(d: &Path, text: &str, mode: u32) {
        let p = d.join(TOP_FILE);
        std::fs::write(&p, text).unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn a_private_one_is_top() {
        let d = dir();
        put(d.path(), "1\n", 0o600);
        assert!(is_top(d.path()));
    }

    #[test]
    fn missing_garbage_and_wrong_shapes_are_sub() {
        let d = dir();
        assert!(!is_top(d.path()));
        for text in ["", "0\n", "yes", "11", "1 1", "top"] {
            put(d.path(), text, 0o600);
            assert!(!is_top(d.path()), "{text:?}");
        }
        put(d.path(), &"1".repeat(64), 0o600);
        assert!(!is_top(d.path()));
        for mode in [0o620, 0o602, 0o666] {
            put(d.path(), "1\n", mode);
            assert!(!is_top(d.path()), "{mode:o}");
        }
        assert!(!is_top(Path::new("/tmp/mt-does-not-exist-x1")));
    }

    #[test]
    fn a_symlink_or_a_directory_is_sub() {
        let d = dir();
        let real = d.path().join("real");
        std::fs::write(&real, "1\n").unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&real, d.path().join(TOP_FILE)).unwrap();
        assert!(!is_top(d.path()));
        let d = dir();
        std::fs::create_dir(d.path().join(TOP_FILE)).unwrap();
        assert!(!is_top(d.path()));
    }

    #[test]
    fn setting_top_writes_a_private_file_that_reads_back() {
        let d = dir();
        apply_top(d.path(), Some(true)).unwrap();
        let mode = std::fs::metadata(d.path().join(TOP_FILE))
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
        assert!(is_top(d.path()));
        apply_top(d.path(), Some(true)).unwrap();
        assert!(is_top(d.path()));
        assert_eq!(std::fs::read_dir(d.path()).unwrap().count(), 1);
    }

    #[test]
    fn setting_top_over_a_loose_file_makes_it_private() {
        let d = dir();
        put(d.path(), "1\n", 0o666);
        apply_top(d.path(), Some(true)).unwrap();
        assert!(is_top(d.path()));
    }

    #[test]
    fn setting_top_replaces_a_symlink_without_writing_through_it() {
        let d = dir();
        let target = d.path().join("target");
        std::fs::write(&target, "keep").unwrap();
        symlink(&target, d.path().join(TOP_FILE)).unwrap();
        apply_top(d.path(), Some(true)).unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "keep");
        assert!(is_top(d.path()));
    }

    #[test]
    fn clearing_top_removes_the_file_and_is_idempotent() {
        let d = dir();
        apply_top(d.path(), Some(true)).unwrap();
        apply_top(d.path(), Some(false)).unwrap();
        assert!(!d.path().join(TOP_FILE).exists());
        apply_top(d.path(), Some(false)).unwrap();
        assert!(!is_top(d.path()));
    }

    #[test]
    fn no_choice_leaves_the_flag_as_it_is() {
        let d = dir();
        apply_top(d.path(), None).unwrap();
        assert!(!is_top(d.path()));
        apply_top(d.path(), Some(true)).unwrap();
        apply_top(d.path(), None).unwrap();
        assert!(is_top(d.path()));
    }

    #[test]
    fn clearing_top_over_a_directory_is_an_error() {
        let d = dir();
        std::fs::create_dir(d.path().join(TOP_FILE)).unwrap();
        assert!(apply_top(d.path(), Some(false)).is_err());
    }
}
