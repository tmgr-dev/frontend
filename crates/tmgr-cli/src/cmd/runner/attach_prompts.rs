use std::path::{Path, PathBuf};
use tmgr_core::config::{config_dir, ensure_private_dir, write_atomic};
use tmgr_runner::attach::{default_prompts, parse_prompts, render_prompts, Prompt};

const FILE: &str = "prompts.toml";

pub(super) fn dir() -> Option<PathBuf> {
    config_dir().ok()
}

pub(super) fn load(dir: &Path) -> (Vec<Prompt>, Option<String>) {
    match std::fs::read_to_string(dir.join(FILE)) {
        Ok(text) => match parse_prompts(&text) {
            Ok(list) => (list, None),
            Err(_) => (
                default_prompts(),
                Some("prompts.toml is invalid, showing defaults; saving replaces it".into()),
            ),
        },
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (default_prompts(), None),
        Err(_) => (default_prompts(), Some("cannot read prompts.toml".into())),
    }
}

pub(super) fn save(dir: &Path, prompts: &[Prompt]) -> std::io::Result<()> {
    ensure_private_dir(dir).map_err(std::io::Error::other)?;
    let path = dir.join(FILE);
    let unreadable = std::fs::read_to_string(&path)
        .map(|text| parse_prompts(&text).is_err())
        .unwrap_or_else(|e| e.kind() != std::io::ErrorKind::NotFound);
    if unreadable {
        let _ = std::fs::rename(&path, dir.join("prompts.toml.bak"));
    }
    write_atomic(&path, render_prompts(prompts).as_bytes(), 0o600).map_err(std::io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str, text: &str) -> Prompt {
        Prompt {
            name: name.into(),
            text: text.into(),
        }
    }

    #[test]
    fn a_missing_file_gives_the_defaults_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let (list, warn) = load(dir.path());
        assert_eq!(list, default_prompts());
        assert!(warn.is_none());
        assert!(!dir.path().join(FILE).exists());
    }

    #[test]
    fn prompts_round_trip_through_a_private_file() {
        let dir = tempfile::tempdir().unwrap();
        let list = vec![p("a", "one \"quoted\" line"), p("b", "two")];
        save(dir.path(), &list).unwrap();
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
    fn deleting_every_prompt_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        save(dir.path(), &[]).unwrap();
        assert!(load(dir.path()).0.is_empty());
    }

    #[test]
    fn an_invalid_file_falls_back_with_a_warning_and_is_untouched() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(FILE), "prompt = [[[").unwrap();
        let (list, warn) = load(dir.path());
        assert_eq!(list, default_prompts());
        assert!(warn.is_some());
        assert_eq!(
            std::fs::read_to_string(dir.path().join(FILE)).unwrap(),
            "prompt = [[["
        );
    }

    #[test]
    fn control_characters_in_the_file_are_cleaned_on_load() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(FILE),
            "[[prompt]]\nname = \"x\"\ntext = \"a\\u001b[201~b\\nc\"\n",
        )
        .unwrap();
        assert_eq!(load(dir.path()).0, vec![p("x", "a[201~b c")]);
    }
}
