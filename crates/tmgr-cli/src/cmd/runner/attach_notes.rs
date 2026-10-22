use std::path::{Path, PathBuf};
use tmgr_core::api::Client;
use tmgr_core::config::{config_dir, ensure_private_dir, write_atomic};
use tmgr_runner::attach::NotesTask;

const DIR: &str = "notes";

pub(super) fn dir() -> Option<PathBuf> {
    config_dir().ok().map(|d| d.join(DIR))
}

fn file(dir: &Path, session: &str) -> Option<PathBuf> {
    let ok = !session.is_empty()
        && session.len() <= 64
        && session
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'));
    ok.then(|| dir.join(format!("{session}.md")))
}

pub(super) fn load(dir: &Path, session: &str) -> Result<String, String> {
    let path = file(dir, session).ok_or("this session cannot have notes")?;
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(e) => Err(format!("cannot read the notes file: {e}")),
    }
}

pub(super) fn save(dir: &Path, session: &str, text: &str) -> std::io::Result<()> {
    let path = file(dir, session).ok_or_else(|| std::io::Error::other("bad session id"))?;
    if text.trim().is_empty() {
        return match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    ensure_private_dir(dir).map_err(std::io::Error::other)?;
    write_atomic(&path, text.as_bytes(), 0o600).map_err(std::io::Error::other)
}

pub(super) struct Description {
    pub key: Option<String>,
    pub text: String,
    pub plain_markdown: bool,
}

pub(super) trait TaskDescription {
    fn get(&self, task_id: u64) -> Result<Description, String>;
    fn set(&self, task_id: u64, text: &str) -> Result<(), String>;
}

struct ClientDescription<'a>(&'a Client);

impl TaskDescription for ClientDescription<'_> {
    fn get(&self, task_id: u64) -> Result<Description, String> {
        let task = self.0.task_by_id(task_id).map_err(|e| e.to_string())?;
        let blocks = task.description_json.is_some_and(|v| !v.is_null());
        let key = task
            .category
            .as_ref()
            .and_then(|c| c.code.clone())
            .zip(task.category_tasks_sequence_id)
            .map(|(code, seq)| format!("{code}-{seq}"));
        Ok(Description {
            key,
            text: task.description.unwrap_or_default(),
            plain_markdown: !blocks,
        })
    }

    fn set(&self, task_id: u64, text: &str) -> Result<(), String> {
        self.0
            .set_task_description(task_id, text)
            .map_err(|e| e.to_string())
    }
}

pub(super) fn append(
    d: &dyn TaskDescription,
    task: &NotesTask,
    block: &str,
) -> Result<String, String> {
    if block.trim().is_empty() {
        return Err("the notes are empty".into());
    }
    let current = d
        .get(task.task_id)
        .map_err(|e| format!("cannot read the task: {e}"))?;
    if !task.key.starts_with('#')
        && !current
            .key
            .as_deref()
            .is_some_and(|k| k.eq_ignore_ascii_case(&task.key))
    {
        return Err("the task found is not the one this session works on".into());
    }
    if !current.plain_markdown {
        return Err("the task description uses the block editor, notes were not added".into());
    }
    let head = current.text.trim_end();
    let text = if head.is_empty() {
        block.to_string()
    } else {
        format!("{head}\n\n{block}")
    };
    d.set(task.task_id, &text)
        .map_err(|e| format!("cannot update the task: {e}"))?;
    Ok(format!("notes added to the description of {}", task.key))
}

pub(super) fn post_real(task: &NotesTask, block: &str) -> Result<String, String> {
    use clap::Parser;
    if super::attach_new::in_agent() {
        return Err("refused inside an agent session".into());
    }
    let cli = crate::cli::Cli::parse_from(["tmgr"]);
    let ctx = crate::ctx::Ctx::new(&cli).map_err(|e| e.to_string())?;
    let client = ctx
        .client_for_run(task.workspace.as_deref())
        .map_err(|e| e.to_string())?;
    append(&ClientDescription(&client), task, block)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake {
        text: RefCell<String>,
        plain: bool,
        key: Option<String>,
        fail_set: bool,
        sets: RefCell<u32>,
    }

    impl Fake {
        fn new(text: &str) -> Fake {
            Fake {
                text: RefCell::new(text.into()),
                plain: true,
                key: Some("TM-7".into()),
                fail_set: false,
                sets: RefCell::new(0),
            }
        }
    }

    impl TaskDescription for Fake {
        fn get(&self, _: u64) -> Result<Description, String> {
            Ok(Description {
                key: self.key.clone(),
                text: self.text.borrow().clone(),
                plain_markdown: self.plain,
            })
        }

        fn set(&self, _: u64, text: &str) -> Result<(), String> {
            *self.sets.borrow_mut() += 1;
            if self.fail_set {
                return Err("503".into());
            }
            *self.text.borrow_mut() = text.into();
            Ok(())
        }
    }

    fn task() -> NotesTask {
        NotesTask {
            task_id: 7,
            workspace: None,
            key: "TM-7".into(),
        }
    }

    #[test]
    fn notes_are_saved_reloaded_and_removed_when_emptied() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("notes");
        assert_eq!(load(&dir, "s_abc").unwrap(), "");
        save(&dir, "s_abc", "first\nsecond").unwrap();
        assert_eq!(load(&dir, "s_abc").unwrap(), "first\nsecond");
        assert_eq!(load(&dir, "s_other").unwrap(), "");
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(file(&dir, "s_abc").unwrap())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
        save(&dir, "s_abc", "  \n").unwrap();
        assert_eq!(load(&dir, "s_abc").unwrap(), "");
        save(&dir, "s_abc", "").unwrap();
    }

    #[test]
    fn a_session_id_cannot_leave_the_notes_folder() {
        assert_eq!(file(Path::new("/n"), "../../etc/passwd"), None);
        assert_eq!(file(Path::new("/n"), ""), None);
        assert_eq!(
            file(Path::new("/n"), "s_ab-1"),
            Some(PathBuf::from("/n/s_ab-1.md"))
        );
        assert!(save(Path::new("/n"), "../x", "t").is_err());
    }

    #[test]
    fn the_block_is_appended_after_the_existing_description() {
        let d = Fake::new("Existing text\n\n");
        let msg = append(&d, &task(), "### Session notes: x\n\nhello").unwrap();
        assert_eq!(msg, "notes added to the description of TM-7");
        assert_eq!(
            d.text.borrow().as_str(),
            "Existing text\n\n### Session notes: x\n\nhello"
        );
    }

    #[test]
    fn an_empty_description_gets_just_the_block() {
        let d = Fake::new("");
        append(&d, &task(), "### Session notes: x\n\nhello").unwrap();
        assert_eq!(d.text.borrow().as_str(), "### Session notes: x\n\nhello");
    }

    #[test]
    fn a_block_editor_description_and_empty_notes_are_never_written() {
        let mut d = Fake::new("old");
        d.plain = false;
        assert!(append(&d, &task(), "note")
            .unwrap_err()
            .contains("block editor"));
        let d = Fake::new("old");
        assert!(append(&d, &task(), " \n").is_err());
        assert_eq!(*d.sets.borrow(), 0);
    }

    #[test]
    fn an_unreadable_notes_file_is_an_error_not_an_empty_note() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("s_bad.md"), [0xff, 0xfe, 0x41]).unwrap();
        assert!(load(tmp.path(), "s_bad")
            .unwrap_err()
            .contains("cannot read"));
    }

    #[test]
    fn notes_are_refused_for_a_different_task_than_the_session_works_on() {
        let mut d = Fake::new("old");
        d.key = Some("OTHER-7".into());
        let err = append(&d, &task(), "note").unwrap_err();
        assert!(err.contains("not the one"), "{err}");
        assert_eq!(*d.sets.borrow(), 0);
        let mut d = Fake::new("old");
        d.key = None;
        assert!(append(&d, &task(), "note").is_err());
        let by_id = NotesTask {
            key: "#7".into(),
            ..task()
        };
        d.key = None;
        assert!(append(&d, &by_id, "note").is_ok());
    }

    #[test]
    fn a_failed_update_is_reported_and_keeps_the_description() {
        let mut d = Fake::new("old");
        d.fail_set = true;
        let err = append(&d, &task(), "note").unwrap_err();
        assert!(err.contains("cannot update the task"), "{err}");
        assert_eq!(d.text.borrow().as_str(), "old");
    }
}
