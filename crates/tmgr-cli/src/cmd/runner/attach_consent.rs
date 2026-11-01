use tmgr_runner::attach::Format;
use tmgr_usage::consent::granted_read_only;
use tmgr_usage::paths::Env;
use tmgr_usage::Tool;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Reads {
    pub claude: bool,
    pub codex: bool,
}

const BOTH_OFF: &str = "Titles and questions are off: run tmgr usage consent allow";
const CLAUDE_OFF: &str =
    "Claude Code titles and questions are off: turn it on in TMGR desktop (AI Usage)";
const CODEX_OFF: &str = "Codex titles and questions are off: turn it on in TMGR desktop (AI Usage)";

impl Reads {
    pub fn of(env: &Env) -> Reads {
        Reads {
            claude: granted_read_only(env, Tool::Claude),
            codex: granted_read_only(env, Tool::Codex),
        }
    }

    pub fn current() -> Reads {
        Reads::of(&Env::from_process())
    }

    pub fn allows(self, format: Format) -> bool {
        match format {
            Format::Claude => self.claude,
            Format::Codex => self.codex,
        }
    }

    pub fn any(self) -> bool {
        self.claude || self.codex
    }

    pub fn denial(self, format: Format) -> Option<&'static str> {
        (!self.allows(format)).then(|| self.hint()).flatten()
    }

    pub fn hint(self) -> Option<&'static str> {
        match (self.claude, self.codex) {
            (true, true) => None,
            (false, false) => Some(BOTH_OFF),
            (false, true) => Some(CLAUDE_OFF),
            (true, false) => Some(CODEX_OFF),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::HashMap;
    use tmgr_usage::consent::{ConsentChange, ReadLogs};
    use tmgr_usage::paths::db_path;
    use tmgr_usage::Store;

    fn env_in(dir: &std::path::Path) -> Env {
        Env {
            home: None,
            vars: HashMap::from([("TMGR_USAGE_DIR".to_string(), dir.display().to_string())]),
        }
    }

    fn set(dir: &std::path::Path, ch: ConsentChange) {
        let store = Store::open(&db_path(dir)).unwrap();
        store.change_log_consent(&ch, Utc::now()).unwrap();
    }

    fn change(state: Option<ReadLogs>, claude: Option<bool>, codex: Option<bool>) -> ConsentChange {
        ConsentChange {
            state,
            claude,
            codex,
        }
    }

    #[test]
    fn the_default_reads_nothing() {
        assert_eq!(
            Reads::default(),
            Reads {
                claude: false,
                codex: false
            }
        );
        assert!(!Reads::default().any());
    }

    #[test]
    fn a_missing_database_reads_nothing_and_creates_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("usage");
        assert_eq!(Reads::of(&env_in(&dir)), Reads::default());
        assert!(!dir.exists());
    }

    #[test]
    fn a_garbage_database_reads_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(db_path(tmp.path()), b"this is not sqlite at all").unwrap();
        assert_eq!(Reads::of(&env_in(tmp.path())), Reads::default());
    }

    #[test]
    fn an_undecided_choice_reads_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        Store::open(&db_path(tmp.path())).unwrap();
        assert_eq!(Reads::of(&env_in(tmp.path())), Reads::default());
    }

    #[test]
    fn a_denied_choice_reads_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        set(tmp.path(), change(Some(ReadLogs::Denied), None, None));
        assert_eq!(Reads::of(&env_in(tmp.path())), Reads::default());
    }

    #[test]
    fn a_granted_choice_reads_both_tools() {
        let tmp = tempfile::tempdir().unwrap();
        set(tmp.path(), change(Some(ReadLogs::Granted), None, None));
        let r = Reads::of(&env_in(tmp.path()));
        assert_eq!(
            r,
            Reads {
                claude: true,
                codex: true
            }
        );
        assert!(r.allows(Format::Claude) && r.allows(Format::Codex));
        assert_eq!(r.hint(), None);
    }

    #[test]
    fn a_granted_choice_with_one_tool_off_reads_only_the_other() {
        let tmp = tempfile::tempdir().unwrap();
        set(
            tmp.path(),
            change(Some(ReadLogs::Granted), None, Some(false)),
        );
        let r = Reads::of(&env_in(tmp.path()));
        assert_eq!(
            r,
            Reads {
                claude: true,
                codex: false
            }
        );
        assert!(r.allows(Format::Claude) && !r.allows(Format::Codex));
        assert!(r.any());
    }

    #[test]
    fn hints_name_the_tool_that_is_off() {
        let on = |claude, codex| Reads { claude, codex }.hint();
        assert_eq!(on(true, true), None);
        assert!(on(false, false)
            .unwrap()
            .starts_with("Titles and questions are off"));
        assert!(on(false, true).unwrap().starts_with("Claude Code titles"));
        assert!(on(true, false).unwrap().starts_with("Codex titles"));
        assert!(on(false, false)
            .unwrap()
            .contains("tmgr usage consent allow"));
        assert!(!on(true, false).unwrap().contains("consent allow"));
    }
}
