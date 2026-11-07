use super::memory::{self, DialogMemory, Kind, Memory};
use super::shared::{Cell, RefCell};
use super::state::*;
use super::view;
use super::{settle, wanted, Entry, Failures, MAX_POLL_ERRORS};
use crate::cmd::board::theme::ColorLevel;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::ffi::OsString;

struct Fake {
    rows: RefCell<Vec<Row>>,
    hints: Cell<bool>,
    ranked: Option<Vec<String>>,
    allowed: Cell<u32>,
    refuse: bool,
    pending: RefCell<Option<(Vec<Row>, Hints)>>,
}

impl Source for Fake {
    fn load(&self, _profile: &str) {
        *self.pending.borrow_mut() = Some((
            self.rows.borrow().clone(),
            Hints {
                reading_off: self.hints.get(),
            },
        ));
    }

    fn poll(&self) -> Option<(Vec<Row>, Hints)> {
        self.pending.borrow_mut().take()
    }

    fn search(&self, _query: &str) -> Option<Vec<String>> {
        self.ranked.clone()
    }

    fn allow_reading(&self) -> Result<(), String> {
        self.allowed.set(self.allowed.get() + 1);
        if self.refuse {
            return Err("could not turn reading on".into());
        }
        for r in self.rows.borrow_mut().iter_mut() {
            r.readable = true;
        }
        self.hints.set(false);
        Ok(())
    }
}

fn row(id: &str, kind: RowKind, sandboxed: bool, project: &str, text: &str) -> Row {
    Row {
        id: id.into(),
        kind,
        sandboxed,
        project: project.into(),
        when: "2h ago".into(),
        name: None,
        text: text.into(),
        live: false,
        readable: true,
        folder_gone: false,
        bypassed: false,
        maybe_live: false,
    }
}

fn sample_rows() -> Vec<Row> {
    vec![
        row(
            "ext-1",
            RowKind::External,
            false,
            "tmgr.dev",
            "fix the menu",
        ),
        row("tm-2", RowKind::Tmgr, true, "tmgr.dev", "add sessions list"),
        row("tm-3", RowKind::Tmgr, false, "blog", "write the post"),
    ]
}

fn agents(installed: [bool; 2]) -> Vec<AgentOpt> {
    vec![
        AgentOpt {
            id: "claude-code".into(),
            name: "Claude Code".into(),
            installed: installed[0],
            bypass: true,
        },
        AgentOpt {
            id: "codex".into(),
            name: "Codex".into(),
            installed: installed[1],
            bypass: true,
        },
    ]
}

fn fake(rows: Vec<Row>) -> Fake {
    Fake {
        rows: RefCell::new(rows),
        hints: Cell::new(false),
        ranked: None,
        allowed: Cell::new(0),
        refuse: false,
        pending: RefCell::new(None),
    }
}

fn state_with(source: Fake, memory: &Memory) -> State {
    State::new(
        agents([true, true]),
        "/work/app".into(),
        memory,
        Box::new(source),
    )
}

fn state(rows: Vec<Row>) -> State {
    state_with(fake(rows), &Memory::default())
}

fn press(s: &mut State, keys: &[Key]) -> Step {
    let mut last = Step::Stay;
    for k in keys {
        last = s.on_key(*k);
    }
    last
}

fn typed(s: &mut State, text: &str) {
    for c in text.chars() {
        s.on_key(Key::Char(c));
    }
}

fn to_session(s: &mut State) {
    press(s, &[Key::Enter]);
    assert_eq!(s.screen, Screen::Session);
}

fn open_row(s: &mut State, index: usize, kind: char) {
    to_session(s);
    for _ in 0..index {
        s.on_key(Key::Down);
    }
    s.on_key(Key::Enter);
    s.on_key(Key::Char(kind));
    assert_eq!(s.screen, Screen::Flags);
}

fn after_delay() {
    std::thread::sleep(std::time::Duration::from_millis(350));
}

fn words(v: &[String]) -> String {
    v.join(" ")
}

#[test]
fn agent_screen_skips_agents_that_are_not_installed() {
    let mut s = State::new(
        agents([false, true]),
        String::new(),
        &Memory::default(),
        Box::new(fake(vec![])),
    );
    assert_eq!(s.agent_cursor, 1);
    s.on_key(Key::Up);
    s.on_key(Key::Down);
    assert_eq!(s.agent_cursor, 1);
    to_session(&mut s);
    assert_eq!(s.agent().id, "codex");
}

#[test]
fn nothing_installed_means_enter_does_nothing() {
    let mut s = State::new(
        agents([false, false]),
        String::new(),
        &Memory::default(),
        Box::new(fake(vec![])),
    );
    assert_eq!(s.on_key(Key::Enter), Step::Stay);
    assert_eq!(s.screen, Screen::Agent);
}

#[test]
fn q_esc_and_ctrl_c_leave_the_first_screen_without_an_argv() {
    for key in [Key::Char('q'), Key::Esc, Key::CtrlC] {
        assert_eq!(state(vec![]).on_key(key), Step::Quit);
    }
    let mut s = state(vec![]);
    to_session(&mut s);
    assert_eq!(s.on_key(Key::CtrlC), Step::Quit);
    assert_eq!(s.on_key(Key::Esc), Step::Stay);
    assert_eq!(s.screen, Screen::Agent);
}

#[test]
fn new_session_argv_for_each_toggle_combination() {
    let cases = [
        (false, false, "tmgr run --agent claude-code --cwd /work/app"),
        (
            true,
            false,
            "tmgr run --agent claude-code --cwd /work/app --mode bypass",
        ),
        (
            false,
            true,
            "tmgr run --agent claude-code --cwd /work/app --no-sandbox",
        ),
        (
            true,
            true,
            "tmgr run --agent claude-code --cwd /work/app --mode bypass --no-sandbox",
        ),
    ];
    for (bypass, no_sandbox, expected) in cases {
        let mut s = state(sample_rows());
        to_session(&mut s);
        s.on_key(Key::Enter);
        assert_eq!(s.screen, Screen::Flags);
        s.on_key(Key::Down);
        if bypass {
            s.on_key(Key::Char(' '));
        }
        s.on_key(Key::Down);
        if no_sandbox {
            s.on_key(Key::Char(' '));
        }
        let Step::Start(argv) = s.on_key(Key::Enter) else {
            panic!("no start");
        };
        assert_eq!(words(&argv), expected);
        assert_eq!(s.command(), expected);
        assert_eq!(s.needs_confirmation(), bypass || no_sandbox);
    }
}

#[test]
fn folder_is_editable_and_quoted_in_the_command() {
    let mut s = state(vec![]);
    to_session(&mut s);
    s.on_key(Key::Enter);
    assert_eq!(s.current_field(), Some(Field::Folder));
    for _ in 0..4 {
        s.on_key(Key::Backspace);
    }
    typed(&mut s, "/my dir");
    s.on_paste("\n/x");
    let Step::Start(argv) = s.on_key(Key::Enter) else {
        panic!("no start");
    };
    assert_eq!(argv[5], "/work/my dir/x");
    assert_eq!(
        s.command(),
        "tmgr run --agent claude-code --cwd '/work/my dir/x'"
    );
}

#[test]
fn empty_folder_leaves_out_cwd() {
    let mut s = State::new(
        agents([true, true]),
        String::new(),
        &Memory::default(),
        Box::new(fake(vec![])),
    );
    to_session(&mut s);
    s.on_key(Key::Enter);
    assert_eq!(words(&s.argv()), "tmgr run --agent claude-code");
}

#[test]
fn external_rows_lock_no_sandbox_on() {
    let mut s = state(sample_rows());
    open_row(&mut s, 1, 'r');
    assert_eq!(s.no_sandbox_lock(), Some((true, SANDBOX_LOCKED_ON)));
    assert_eq!(words(&s.argv()), "tmgr resume ext-1 --no-sandbox");
    s.on_key(Key::Down);
    s.on_key(Key::Char(' '));
    assert!(s.no_sandbox());
    s.on_key(Key::Down);
    s.on_key(Key::Char(' '));
    assert!(s.no_sandbox());
}

#[test]
fn sandboxed_tmgr_rows_lock_no_sandbox_off() {
    let mut s = state_with(
        fake(sample_rows()),
        &Memory {
            no_sandbox: true,
            ..Memory::default()
        },
    );
    open_row(&mut s, 2, 'r');
    assert_eq!(s.no_sandbox_lock(), Some((false, SANDBOX_LOCKED_OFF)));
    assert!(!s.no_sandbox());
    assert_eq!(words(&s.argv()), "tmgr resume tm-2 --attach");
    s.on_key(Key::Down);
    s.on_key(Key::Char(' '));
    s.on_key(Key::Down);
    s.on_key(Key::Char(' '));
    assert!(!s.no_sandbox());
}

#[test]
fn unsandboxed_tmgr_rows_lock_no_sandbox_on_with_a_reason() {
    let mut s = state(sample_rows());
    open_row(&mut s, 3, 'r');
    assert_eq!(s.no_sandbox_lock(), Some((true, SANDBOX_REQUIRED)));
    assert_eq!(words(&s.argv()), "tmgr resume tm-3 --attach --no-sandbox");
    s.on_key(Key::Down);
    s.on_key(Key::Char(' '));
    assert!(s.no_sandbox());
    s.on_key(Key::Up);
    s.on_key(Key::Char(' '));
    assert_eq!(
        words(&s.argv()),
        "tmgr resume tm-3 --attach --mode bypass --no-sandbox"
    );
    assert!(render(&s, 100, 20).contains("ran without the sandbox"));
}

#[test]
fn a_tmgr_row_that_ran_in_bypass_locks_bypass_on_with_a_reason() {
    let mut rows = sample_rows();
    rows[1].bypassed = true;
    let mut s = state(rows);
    open_row(&mut s, 2, 'r');
    assert_eq!(s.bypass_lock(), Some((true, BYPASS_REQUIRED)));
    assert!(s.bypass());
    assert_eq!(words(&s.argv()), "tmgr resume tm-2 --attach --mode bypass");
    s.on_key(Key::Char(' '));
    assert!(s.bypass());
    assert!(render(&s, 100, 20).contains("ran in bypass mode"));
}

#[test]
fn a_possibly_running_row_says_it_may_still_be_running_and_stays_resumable() {
    let mut rows = sample_rows();
    rows[0].maybe_live = true;
    let mut s = state(rows);
    to_session(&mut s);
    assert!(render(&s, 120, 12).contains("(may still be running)"));
    press(&mut s, &[Key::Down, Key::Enter]);
    assert_eq!(s.choice, Some(Kind::Resume));
}

#[test]
fn fork_of_an_external_row_keeps_no_sandbox_only() {
    let mut s = state(sample_rows());
    open_row(&mut s, 1, 'f');
    assert_eq!(words(&s.argv()), "tmgr resume ext-1 --fork --no-sandbox");
}

#[test]
fn enter_on_a_row_asks_resume_or_fork_and_tab_switches() {
    let mut s = state(sample_rows());
    to_session(&mut s);
    press(&mut s, &[Key::Down, Key::Enter]);
    assert_eq!(s.choice, Some(Kind::Resume));
    s.on_key(Key::Tab);
    assert_eq!(s.choice, Some(Kind::Fork));
    s.on_key(Key::Esc);
    assert_eq!(s.choice, None);
    assert_eq!(s.screen, Screen::Session);
    press(&mut s, &[Key::Enter, Key::Tab, Key::Tab, Key::Enter]);
    assert_eq!(s.screen, Screen::Flags);
    assert_eq!(s.kind(), Kind::Resume);
}

#[test]
fn live_rows_can_only_be_forked() {
    let mut rows = sample_rows();
    rows[1].live = true;
    let mut s = state(rows);
    to_session(&mut s);
    press(&mut s, &[Key::Down, Key::Down]);
    assert_eq!(s.reason(), Some(LIVE_REASON));
    s.on_key(Key::Enter);
    assert_eq!(s.choice, Some(Kind::Fork));
    s.on_key(Key::Tab);
    assert_eq!(s.choice, Some(Kind::Fork));
    s.on_key(Key::Char('r'));
    assert_eq!(s.screen, Screen::Session);
    assert_eq!(s.notice.as_deref(), Some(LIVE_REASON));
    s.on_key(Key::Enter);
    assert_eq!(s.screen, Screen::Flags);
    assert_eq!(s.kind(), Kind::Fork);
}

#[test]
fn unreadable_and_folder_gone_rows_cannot_be_selected() {
    let mut rows = sample_rows();
    rows[0].readable = false;
    rows[1].folder_gone = true;
    let mut s = state(rows);
    to_session(&mut s);
    for steps in [1, 1] {
        for _ in 0..steps {
            s.on_key(Key::Down);
        }
        assert_eq!(s.on_key(Key::Enter), Step::Stay);
        assert_eq!(s.choice, None);
        assert_eq!(s.screen, Screen::Session);
    }
    assert_eq!(s.reason(), Some(GONE_REASON));
    assert!(s.reading_hint());
}

#[test]
fn rows_with_a_control_character_in_the_id_cannot_be_selected() {
    let mut rows = sample_rows();
    rows[0].id = "bad\nid".into();
    let mut s = state(rows);
    to_session(&mut s);
    press(&mut s, &[Key::Down, Key::Enter]);
    assert_eq!(s.choice, None);
}

#[test]
fn a_grants_reading_and_reloads_the_rows() {
    let mut rows = sample_rows();
    rows[0].readable = false;
    let src = fake(rows);
    let allowed = src.allowed.clone();
    let mut s = state_with(src, &Memory::default());
    to_session(&mut s);
    assert!(s.reading_hint());
    s.on_key(Key::Tab);
    assert_eq!(allowed.get(), 0);
    assert_eq!(s.notice.as_deref(), Some(ALLOW_CONFIRM));
    after_delay();
    s.on_key(Key::Tab);
    assert_eq!(allowed.get(), 1);
    assert!(!s.reading_hint());
    assert!(s.rows.iter().all(|r| r.readable));
    assert_eq!(s.filter, "");
}

#[test]
fn a_reports_a_refusal_and_keeps_the_hint() {
    let mut src = fake(sample_rows());
    src.hints.set(true);
    src.refuse = true;
    let mut s = state_with(src, &Memory::default());
    to_session(&mut s);
    s.on_key(Key::Tab);
    after_delay();
    s.on_key(Key::Tab);
    assert_eq!(s.notice.as_deref(), Some("could not turn reading on"));
    assert!(s.reading_hint());
}

#[test]
fn typing_a_letter_never_grants_reading_and_a_stray_key_cancels_the_confirmation() {
    let src = fake(sample_rows());
    src.hints.set(true);
    let allowed = src.allowed.clone();
    let mut s = state_with(src, &Memory::default());
    to_session(&mut s);
    typed(&mut s, "a");
    assert_eq!(s.filter, "a");
    assert_eq!(allowed.get(), 0);
    s.on_key(Key::Tab);
    after_delay();
    press(&mut s, &[Key::Char('x'), Key::Tab]);
    assert_eq!(allowed.get(), 0);
    after_delay();
    s.on_key(Key::Tab);
    assert_eq!(allowed.get(), 1);
}

#[test]
fn typing_filters_by_project_and_text_without_an_index() {
    let mut s = state(sample_rows());
    to_session(&mut s);
    typed(&mut s, "BLOG");
    assert_eq!(s.visible(), vec![2]);
    s.on_key(Key::Backspace);
    s.on_key(Key::Backspace);
    s.on_key(Key::Backspace);
    s.on_key(Key::Backspace);
    typed(&mut s, "menu");
    assert_eq!(s.visible(), vec![0]);
    s.on_key(Key::Down);
    assert_eq!(s.selected_row().unwrap().id, "ext-1");
}

#[test]
fn a_wired_search_decides_the_order_and_the_set() {
    let mut src = fake(sample_rows());
    src.ranked = Some(vec!["tm-3".into(), "missing".into(), "ext-1".into()]);
    let mut s = state_with(src, &Memory::default());
    to_session(&mut s);
    typed(&mut s, "anything");
    assert_eq!(s.visible(), vec![2, 0]);
}

fn named(id: &str, kind: RowKind, name: &str, text: &str) -> Row {
    Row {
        name: Some(name.into()),
        ..row(id, kind, false, "proj", text)
    }
}

fn search_state(rows: Vec<Row>, ranked: Option<Vec<&str>>, query: &str) -> State {
    let mut src = fake(rows);
    src.ranked = ranked.map(|v| v.into_iter().map(String::from).collect());
    let mut s = state_with(src, &Memory::default());
    to_session(&mut s);
    typed(&mut s, query);
    s
}

#[test]
fn a_name_only_row_missing_from_the_hits_still_shows_first() {
    let rows = vec![
        row("a", RowKind::External, false, "p", "content about deploy"),
        named("b", RowKind::External, "deploy notes", "x"),
    ];
    let s = search_state(rows, Some(vec!["a"]), "deploy");
    assert_eq!(s.visible(), vec![1, 0]);
}

#[test]
fn a_name_match_ranks_first_when_the_index_is_silent() {
    let rows = vec![
        row("a", RowKind::External, false, "p", "content about deploy"),
        named("b", RowKind::External, "deploy notes", "x"),
    ];
    let s = search_state(rows, None, "deploy");
    assert_eq!(s.visible(), vec![1, 0]);
}

#[test]
fn exact_name_beats_prefix_beats_substring_and_each_tier_keeps_row_order() {
    let rows = vec![
        named("sub1", RowKind::External, "my deploy", "x"),
        named("pre1", RowKind::External, "deploy now", "x"),
        named("exact", RowKind::External, "deploy", "x"),
        named("sub2", RowKind::External, "redeploy", "x"),
        named("pre2", RowKind::External, "deployment", "x"),
    ];
    let s = search_state(rows, Some(vec![]), "deploy");
    assert_eq!(s.visible(), vec![2, 1, 4, 0, 3]);
}

#[test]
fn a_tmgr_row_matched_by_name_beats_a_content_hit() {
    let rows = vec![
        row("hit", RowKind::External, false, "p", "talks about billing"),
        named("tm-9", RowKind::Tmgr, "Billing", "x"),
    ];
    let s = search_state(rows, Some(vec!["hit"]), "billing");
    assert_eq!(s.visible(), vec![1, 0]);
}

#[test]
fn name_matching_ignores_case_and_surrounding_spaces() {
    let rows = vec![
        row("a", RowKind::External, false, "p", "x"),
        named("b", RowKind::External, "Release Plan", "x"),
    ];
    let s = search_state(rows, Some(vec![]), "  RELEASE plan ");
    assert_eq!(s.visible(), vec![1]);
}

#[test]
fn a_row_that_is_both_a_name_match_and_a_hit_appears_once() {
    let rows = vec![
        row("a", RowKind::External, false, "p", "x"),
        named("b", RowKind::External, "deploy", "x"),
    ];
    let s = search_state(rows, Some(vec!["b", "a", "b"]), "deploy");
    assert_eq!(s.visible(), vec![1, 0]);
}

#[test]
fn rows_without_a_name_are_never_name_matches() {
    let rows = vec![row("a", RowKind::External, false, "p", "x")];
    let s = search_state(rows, Some(vec![]), "a");
    assert_eq!(s.visible(), Vec::<usize>::new());
}

#[test]
fn esc_walks_back_one_screen_at_a_time() {
    let mut s = state(sample_rows());
    open_row(&mut s, 1, 'r');
    s.on_key(Key::Esc);
    assert_eq!(s.screen, Screen::Session);
    s.on_key(Key::Esc);
    assert_eq!(s.screen, Screen::Agent);
}

#[test]
fn memory_preselects_agent_toggles_and_choice() {
    let remembered = Memory {
        agent: "codex".into(),
        kind: Kind::Fork,
        bypass: true,
        no_sandbox: true,
    };
    let mut s = state_with(fake(sample_rows()), &remembered);
    assert_eq!(s.agent_cursor, 1);
    to_session(&mut s);
    assert_eq!(s.screen, Screen::Session);
    press(&mut s, &[Key::Down, Key::Enter]);
    assert_eq!(s.choice, Some(Kind::Fork));
    s.on_key(Key::Enter);
    assert_eq!(s.screen, Screen::Flags);
    assert!(s.bypass());
    assert!(s.no_sandbox());
    assert!(s.needs_confirmation());
}

#[test]
fn memory_of_an_uninstalled_agent_falls_back_to_the_first_installed() {
    let remembered = Memory {
        agent: "codex".into(),
        ..Memory::default()
    };
    let s = State::new(
        agents([true, false]),
        String::new(),
        &remembered,
        Box::new(fake(vec![])),
    );
    assert_eq!(s.agent_cursor, 0);
}

#[test]
fn locks_never_leak_into_the_remembered_choice() {
    let mut s = state(sample_rows());
    open_row(&mut s, 1, 'r');
    assert!(s.no_sandbox());
    assert!(!s.memory().no_sandbox);
    s.on_key(Key::Esc);
    s.on_key(Key::Esc);
    s.on_key(Key::Enter);
    s.on_key(Key::Enter);
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.on_key(Key::Char(' '));
    assert!(s.memory().no_sandbox);
}

#[test]
fn memory_round_trips_with_private_mode_and_no_stray_files() {
    use std::os::unix::fs::PermissionsExt;
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("cfg").join("run-menu.toml");
    let m = Memory {
        agent: "codex".into(),
        kind: Kind::Resume,
        bypass: true,
        no_sandbox: false,
    };
    memory::save(&path, &m).unwrap();
    assert_eq!(memory::load(&path), m);
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(names, vec![OsString::from("run-menu.toml")]);
}

#[test]
fn missing_or_corrupt_memory_means_defaults() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("run-menu.toml");
    assert_eq!(memory::load(&path), Memory::default());
    for junk in [
        "\u{0}\u{1}garbage",
        "agent = [",
        "agent = 5\nbypass = \"yes\"\nkind = 3",
    ] {
        std::fs::write(&path, junk).unwrap();
        assert_eq!(memory::load(&path), Memory::default(), "{junk}");
    }
}

#[test]
fn dialog_memory_round_trips_and_omits_unset_options() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("cfg").join("attach-new.toml");
    assert_eq!(memory::load_dialog(&path), DialogMemory::default());
    let m = DialogMemory {
        agent: "claude-code".into(),
        bypass: true,
        no_sandbox: true,
        worktree: Some(true),
        permission_cards: Some(false),
        checkpoints: None,
    };
    memory::save_dialog(&path, &m).unwrap();
    assert_eq!(memory::load_dialog(&path), m);
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("checkpoints"));
    let bare = DialogMemory {
        agent: "codex".into(),
        ..DialogMemory::default()
    };
    memory::save_dialog(&path, &bare).unwrap();
    assert_eq!(memory::load_dialog(&path), bare);
}

#[test]
fn missing_corrupt_or_wrong_typed_dialog_memory_means_defaults() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("attach-new.toml");
    assert_eq!(memory::load_dialog(&path), DialogMemory::default());
    for junk in ["\u{0}\u{1}garbage", "agent = ["] {
        std::fs::write(&path, junk).unwrap();
        assert_eq!(
            memory::load_dialog(&path),
            DialogMemory::default(),
            "{junk}"
        );
    }
    std::fs::write(
        &path,
        "agent = 5\nbypass = \"yes\"\nno_sandbox = 1\nworktree = \"on\"\npermission_cards = []\ncheckpoints = 2\n",
    )
    .unwrap();
    assert_eq!(memory::load_dialog(&path), DialogMemory::default());
    std::fs::write(
        &path,
        "agent = \"codex\"\nbypass = true\nworktree = \"on\"\n",
    )
    .unwrap();
    let m = memory::load_dialog(&path);
    assert_eq!(
        (m.agent.as_str(), m.bypass, m.worktree),
        ("codex", true, None)
    );
}

#[test]
fn the_dialog_file_and_the_run_menu_file_never_touch_each_other() {
    let home = tempfile::tempdir().unwrap();
    let menu_path = home.path().join("run-menu.toml");
    let dialog_path = home.path().join("attach-new.toml");
    let menu = Memory {
        agent: "codex".into(),
        kind: Kind::Fork,
        bypass: true,
        no_sandbox: false,
    };
    memory::save(&menu_path, &menu).unwrap();
    let before = std::fs::read(&menu_path).unwrap();
    let dialog = DialogMemory {
        agent: "claude-code".into(),
        no_sandbox: true,
        ..DialogMemory::default()
    };
    memory::save_dialog(&dialog_path, &dialog).unwrap();
    assert_eq!(std::fs::read(&menu_path).unwrap(), before);
    let dialog_before = std::fs::read(&dialog_path).unwrap();
    memory::save(&menu_path, &Memory::default()).unwrap();
    assert_eq!(std::fs::read(&dialog_path).unwrap(), dialog_before);
    assert_eq!(memory::load_dialog(&dialog_path), dialog);
    assert_eq!(memory::load(&menu_path), Memory::default());
}

#[test]
fn only_a_confirmed_start_writes_the_memory() {
    let home = tempfile::tempdir().unwrap();
    let path = home.path().join("cfg").join("run-menu.toml");
    let mut s = state(sample_rows());
    to_session(&mut s);
    s.on_key(Key::Enter);
    assert!(matches!(
        settle(&s, Some(Step::Quit), Some(&path)),
        Entry::Exit(0)
    ));
    assert!(matches!(settle(&s, None, Some(&path)), Entry::Skip));
    assert!(!path.exists());
    let step = s.on_key(Key::Enter);
    let Entry::Argv(argv) = settle(&s, Some(step), Some(&path)) else {
        panic!("no argv");
    };
    assert_eq!(argv, s.argv());
    assert_eq!(memory::load(&path).agent, "claude-code");
}

#[test]
fn the_menu_only_opens_for_a_bare_run_on_a_terminal_outside_an_agent() {
    let args = |v: &[&str]| v.iter().map(OsString::from).collect::<Vec<_>>();
    assert!(wanted(&args(&["run"]), true, false));
    assert!(!wanted(&args(&["run"]), false, false));
    assert!(!wanted(&args(&["run"]), true, true));
    assert!(!wanted(&args(&[]), true, false));
    assert!(!wanted(&args(&["--json", "run"]), true, false));
    assert!(!wanted(&args(&["run", "--no-attach"]), true, false));
    assert!(!wanted(&args(&["run", "--agent", "x"]), true, false));
    assert!(!wanted(&args(&["status"]), true, false));
}

fn render(s: &State, width: u16, height: u16) -> String {
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    let look = view::Look {
        level: ColorLevel::None,
    };
    term.draw(|f| view::draw(f, s, look)).unwrap();
    let buf = term.backend().buffer();
    let mut out = String::new();
    for row in buf.content.chunks(buf.area.width as usize) {
        let line: String = row.iter().map(|c| c.symbol()).collect();
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

fn check_snapshot(actual: &str, name: &str, expected: &str) {
    if std::env::var("UPDATE_SNAPSHOT").is_ok_and(|v| v == "1") {
        let path = format!(
            "{}/src/cmd/runner/menu/snapshots/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::write(&path, actual).unwrap();
        return;
    }
    assert_eq!(
        actual, expected,
        "snapshot {name} changed; rerun with UPDATE_SNAPSHOT=1 to regenerate"
    );
}

#[test]
fn agent_screen_snapshot() {
    let s = State::new(
        agents([true, false]),
        String::new(),
        &Memory::default(),
        Box::new(fake(vec![])),
    );
    check_snapshot(
        &render(&s, 100, 12),
        "render_agent.txt",
        include_str!("snapshots/render_agent.txt"),
    );
}

fn mixed_rows() -> Vec<Row> {
    let mut rows = sample_rows();
    rows.push(row(
        "tm-4",
        RowKind::Tmgr,
        false,
        "infra",
        "rotate the keys",
    ));
    rows.push(row(
        "ext-5",
        RowKind::External,
        false,
        "old-site",
        "port the css",
    ));
    rows[1].live = true;
    rows[2].folder_gone = true;
    rows[3].readable = false;
    rows
}

#[test]
fn session_screen_snapshot() {
    let mut s = state(mixed_rows());
    to_session(&mut s);
    press(&mut s, &[Key::Down, Key::Down]);
    check_snapshot(
        &render(&s, 100, 16),
        "render_session.txt",
        include_str!("snapshots/render_session.txt"),
    );
}

#[test]
fn flags_screen_snapshots_for_each_lock_case() {
    let mut s = state(mixed_rows());
    to_session(&mut s);
    s.on_key(Key::Enter);
    s.on_key(Key::Down);
    s.on_key(Key::Char(' '));
    check_snapshot(
        &render(&s, 100, 16),
        "render_flags_new.txt",
        include_str!("snapshots/render_flags_new.txt"),
    );

    let mut s = state(sample_rows());
    open_row(&mut s, 1, 'r');
    s.on_key(Key::Char(' '));
    check_snapshot(
        &render(&s, 100, 16),
        "render_flags_external.txt",
        include_str!("snapshots/render_flags_external.txt"),
    );

    let mut s = state(sample_rows());
    open_row(&mut s, 2, 'r');
    check_snapshot(
        &render(&s, 100, 16),
        "render_flags_sandboxed.txt",
        include_str!("snapshots/render_flags_sandboxed.txt"),
    );

    let mut s = state(sample_rows());
    open_row(&mut s, 1, 'f');
    s.on_key(Key::Down);
    s.on_key(Key::Char(' '));
    check_snapshot(
        &render(&s, 100, 16),
        "render_flags_fork.txt",
        include_str!("snapshots/render_flags_fork.txt"),
    );
}

#[test]
fn long_commands_wrap_instead_of_being_cut() {
    let mut s = state(vec![]);
    to_session(&mut s);
    s.on_key(Key::Enter);
    typed(&mut s, &"d".repeat(120));
    let screen = render(&s, 60, 20);
    let joined: String = screen
        .lines()
        .map(|l| l.trim_matches(|c| c == '\u{2502}' || c == ' '))
        .skip_while(|l| !l.contains("Command"))
        .skip(1)
        .take_while(|l| !l.is_empty())
        .collect();
    assert!(joined.ends_with(&"d".repeat(120)), "{joined}");
    assert!(
        joined.starts_with("tmgr run --agent claude-code --cwd"),
        "{joined}"
    );
}

#[test]
fn hostile_row_text_never_reaches_the_screen_as_control_characters() {
    let mut rows = sample_rows();
    rows[0].text = "a\u{1b}[2Jb\u{1b}]0;owned\u{7}c\u{202e}d\u{9b}e\nf".into();
    rows[0].project = "p\u{1b}[31mq\u{2066}r".into();
    rows[0].when = "\u{1b}[H".into();
    let mut s = state(rows);
    to_session(&mut s);
    let mut term = Terminal::new(TestBackend::new(100, 14)).unwrap();
    let look = view::Look {
        level: ColorLevel::None,
    };
    term.draw(|f| view::draw(f, &s, look)).unwrap();
    let buf = term.backend().buffer();
    for cell in buf.content() {
        assert!(
            !cell.symbol().chars().any(char::is_control),
            "{:?}",
            cell.symbol()
        );
    }
    let text = render(&s, 100, 14);
    assert!(text.contains("a?[2Jb?]0;owned?c?d?e?f"), "{text}");

    let mut parser = vt100::Parser::new(14, 100, 0);
    parser.process(text.replace('\n', "\r\n").as_bytes());
    assert!(parser.screen().contents().contains("a?[2Jb?]0;owned"));
}

#[test]
fn fit_truncates_by_display_width() {
    assert_eq!(view::fit("hello", 10), "hello");
    assert_eq!(view::fit("hello world", 6), "hello\u{2026}");
    assert_eq!(
        view::fit("\u{4e16}\u{754c}\u{4e16}\u{754c}", 5),
        "\u{4e16}\u{754c}\u{2026}"
    );
    assert_eq!(view::fit("a\u{1b}b", 5), "a?b");
}

#[test]
fn forking_a_sandboxed_claude_tmgr_row_offers_no_flags() {
    let mut s = state(sample_rows());
    open_row(&mut s, 2, 'f');
    assert!(s.fields().is_empty());
    assert_eq!(words(&s.argv()), "tmgr resume tm-2 --fork");
    press(&mut s, &[Key::Down, Key::Tab, Key::Up, Key::Char(' ')]);
    assert_eq!(words(&s.argv()), "tmgr resume tm-2 --fork");
    assert_eq!(s.on_key(Key::Enter), Step::Start(s.argv()));
}

#[test]
fn a_bypass_resume_of_a_tmgr_row_is_not_affected_by_fork_rules() {
    let mut s = state(sample_rows());
    open_row(&mut s, 2, 'r');
    press(&mut s, &[Key::Char(' ')]);
    assert_eq!(words(&s.argv()), "tmgr resume tm-2 --attach --mode bypass");
}

#[test]
fn forking_a_codex_tmgr_row_is_not_offered() {
    let mut s = state(sample_rows());
    s.agent_cursor = 1;
    to_session(&mut s);
    press(&mut s, &[Key::Down, Key::Down, Key::Enter]);
    assert_eq!(s.choice, Some(Kind::Resume));
    s.on_key(Key::Tab);
    assert_eq!(s.choice, Some(Kind::Resume));
    s.on_key(Key::Char('f'));
    assert_eq!(s.screen, Screen::Session);
    assert_eq!(s.notice.as_deref(), Some(FORK_CODEX));
}

#[test]
fn a_live_codex_tmgr_row_offers_neither_resume_nor_fork() {
    let mut rows = sample_rows();
    rows[1].live = true;
    let mut s = state(rows);
    s.agent_cursor = 1;
    to_session(&mut s);
    press(&mut s, &[Key::Down, Key::Down]);
    assert_eq!(s.reason(), Some(FORK_CODEX));
    s.on_key(Key::Enter);
    assert_eq!(s.choice, None);
    assert_eq!(s.screen, Screen::Session);
    assert_eq!(s.notice.as_deref(), Some(FORK_CODEX));
}

#[test]
fn an_unsandboxed_tmgr_row_cannot_be_forked_by_its_tmgr_id() {
    let mut s = state(sample_rows());
    to_session(&mut s);
    press(&mut s, &[Key::Down, Key::Down, Key::Down, Key::Enter]);
    s.on_key(Key::Char('f'));
    assert_eq!(s.screen, Screen::Session);
    assert_eq!(s.notice.as_deref(), Some(FORK_UNSANDBOXED));
}

#[test]
fn external_rows_can_always_be_forked_even_for_codex() {
    let mut s = state(sample_rows());
    s.agent_cursor = 1;
    open_row(&mut s, 1, 'f');
    assert_eq!(words(&s.argv()), "tmgr resume ext-1 --fork --no-sandbox");
}

#[test]
fn bypass_is_locked_off_for_an_agent_without_a_bypass_mode() {
    let mut s = State::new(
        vec![AgentOpt {
            id: "codex".into(),
            name: "Codex".into(),
            installed: true,
            bypass: false,
        }],
        "/work/app".into(),
        &Memory {
            bypass: true,
            ..Memory::default()
        },
        Box::new(fake(sample_rows())),
    );
    assert_eq!(s.bypass_lock(), Some((false, BYPASS_UNAVAILABLE)));
    press(&mut s, &[Key::Enter, Key::Enter, Key::Down, Key::Char(' ')]);
    assert!(!s.bypass());
    assert_eq!(words(&s.argv()), "tmgr run --agent codex --cwd /work/app");
    assert!(render(&s, 100, 20).contains("This agent has no bypass mode."));
}

#[test]
fn unreadable_rows_are_not_resumable_and_the_consent_key_is_shown() {
    let mut rows = sample_rows();
    rows[0].readable = false;
    let mut s = state(rows);
    to_session(&mut s);
    s.on_key(Key::Down);
    s.on_key(Key::Enter);
    assert_eq!(s.choice, None);
    assert!(s.reading_hint());
    assert!(render(&s, 120, 20).contains("Press Tab to allow it"));
}

#[test]
fn folder_gone_rows_show_the_reason_and_offer_nothing() {
    let mut rows = sample_rows();
    rows[1].folder_gone = true;
    let mut s = state(rows);
    to_session(&mut s);
    press(&mut s, &[Key::Down, Key::Down]);
    assert_eq!(s.reason(), Some(GONE_REASON));
    s.on_key(Key::Enter);
    assert_eq!(s.choice, None);
}

#[test]
fn the_row_that_starts_is_the_one_that_was_picked_even_if_the_results_change() {
    let mut src = fake(sample_rows());
    src.ranked = Some(vec!["ext-1".into(), "tm-2".into()]);
    let mut s = state_with(src, &Memory::default());
    to_session(&mut s);
    typed(&mut s, "x");
    press(&mut s, &[Key::Down, Key::Enter]);
    assert_eq!(s.choice, Some(Kind::Resume));
    s.rows.swap(0, 1);
    s.filter.clear();
    s.on_key(Key::Char('r'));
    assert_eq!(s.screen, Screen::Flags);
    assert_eq!(words(&s.argv()), "tmgr resume ext-1 --no-sandbox");
}

#[test]
fn rows_arrive_later_without_blocking_and_the_new_session_stays_available() {
    struct Slow(Cell<bool>);
    impl Source for Slow {
        fn load(&self, _: &str) {}
        fn poll(&self) -> Option<(Vec<Row>, Hints)> {
            self.0.get().then(|| (sample_rows(), Hints::default()))
        }
        fn search(&self, _: &str) -> Option<Vec<String>> {
            None
        }
        fn allow_reading(&self) -> Result<(), String> {
            Ok(())
        }
    }
    let ready = Cell::new(false);
    let mut s = State::new(
        agents([true, true]),
        "/work/app".into(),
        &Memory::default(),
        Box::new(Slow(ready.clone())),
    );
    to_session(&mut s);
    assert!(s.loading);
    assert!(render(&s, 100, 12).contains("loading"));
    assert!(s.rows.is_empty());
    ready.set(true);
    s.pump();
    assert!(!s.loading);
    assert_eq!(s.rows.len(), 3);
}

#[test]
fn an_unprintable_current_folder_is_left_out_of_the_command() {
    let mut s = State::new(
        agents([true, true]),
        String::new(),
        &Memory::default(),
        Box::new(fake(vec![])),
    );
    press(&mut s, &[Key::Enter, Key::Enter]);
    assert_eq!(words(&s.argv()), "tmgr run --agent claude-code");
    assert!(render(&s, 100, 12).contains("(current folder)"));
}

#[test]
fn a_tty_that_keeps_failing_ends_the_menu_after_a_few_errors_and_a_success_resets_it() {
    let mut f = Failures::default();
    for _ in 0..MAX_POLL_ERRORS - 1 {
        assert!(!f.exhausted());
    }
    f.reset();
    for _ in 0..MAX_POLL_ERRORS - 1 {
        assert!(!f.exhausted());
    }
    assert!(f.exhausted());
}

#[test]
fn a_second_tab_right_after_the_first_does_not_grant_reading() {
    let src = fake(sample_rows());
    src.hints.set(true);
    let allowed = src.allowed.clone();
    let mut s = state_with(src, &Memory::default());
    to_session(&mut s);
    press(&mut s, &[Key::Tab, Key::Tab, Key::Tab]);
    assert_eq!(allowed.get(), 0);
    after_delay();
    s.on_key(Key::Tab);
    assert_eq!(allowed.get(), 1);
}

#[test]
fn rows_replaced_before_the_flags_cannot_change_which_session_starts() {
    struct Reorder(Cell<bool>);
    impl Source for Reorder {
        fn load(&self, _: &str) {}
        fn poll(&self) -> Option<(Vec<Row>, Hints)> {
            let mut rows = sample_rows();
            if self.0.get() {
                rows.reverse();
                rows.retain(|r| r.id != "tm-3");
            }
            Some((rows, Hints::default()))
        }
        fn search(&self, _: &str) -> Option<Vec<String>> {
            None
        }
        fn allow_reading(&self) -> Result<(), String> {
            Ok(())
        }
    }
    let changed = Cell::new(false);
    let mut s = State::new(
        agents([true, true]),
        "/work/app".into(),
        &Memory::default(),
        Box::new(Reorder(changed.clone())),
    );
    to_session(&mut s);
    press(
        &mut s,
        &[Key::Down, Key::Down, Key::Down, Key::Enter, Key::Char('r')],
    );
    assert_eq!(s.screen, Screen::Flags);
    assert_eq!(words(&s.argv()), "tmgr resume tm-3 --attach --no-sandbox");
    changed.set(true);
    s.loading = true;
    s.pump();
    assert_eq!(s.screen, Screen::Session);
    assert_eq!(s.notice.as_deref(), Some(GONE_FROM_LIST));
}

#[test]
fn rows_replaced_before_the_flags_keep_the_same_session_when_it_is_still_listed() {
    let mut s = state(sample_rows());
    open_row(&mut s, 3, 'r');
    s.rows.reverse();
    s.rows.retain(|r| r.id != "ext-1");
    assert_eq!(words(&s.argv()), "tmgr resume tm-3 --attach --no-sandbox");
}

use super::{feed_keys, feed_paste, keys_from_bytes, overlay, Fed};

#[test]
fn raw_bytes_map_to_menu_keys() {
    let keys = |b: &[u8]| keys_from_bytes(b);
    assert_eq!(keys(b"\x1b[A"), [Key::Up]);
    assert_eq!(keys(b"\x1bOA"), [Key::Up]);
    assert_eq!(keys(b"\x1b[B"), [Key::Down]);
    assert_eq!(keys(b"\x1bOB"), [Key::Down]);
    assert_eq!(keys(b"\x1b[1;2A"), [Key::Up]);
    assert_eq!(keys(b"\r"), [Key::Enter]);
    assert_eq!(keys(b"\n"), [Key::Enter]);
    assert_eq!(keys(b"\x1b"), [Key::Esc]);
    assert_eq!(keys(b"\t"), [Key::Tab]);
    assert_eq!(keys(b"\x7f"), [Key::Backspace]);
    assert_eq!(keys(b"\x08"), [Key::Backspace]);
    assert_eq!(keys(b"\x03"), [Key::CtrlC]);
    assert_eq!(keys(b"\x01"), [Key::CtrlA]);
    assert_eq!(
        keys("a\u{e9}\u{4e2d}".as_bytes()),
        [Key::Char('a'), Key::Char('\u{e9}'), Key::Char('\u{4e2d}')]
    );
    assert_eq!(
        keys(b"ab\x1b[Bc"),
        [Key::Char('a'), Key::Char('b'), Key::Down, Key::Char('c')]
    );
}

#[test]
fn unknown_sequences_and_control_bytes_are_dropped_not_typed() {
    let keys = |b: &[u8]| keys_from_bytes(b);
    for raw in [
        &b"\x1b[C"[..],
        b"\x1b[D",
        b"\x1b[3~",
        b"\x1b[H",
        b"\x1bOH",
        b"\x1b[1;5C",
        b"\x1b[200~",
        b"\x1b[",
        b"\x1bx",
        b"\x1b]0;title\x07",
        b"\x02",
        b"\x12",
        b"\x00",
        b"\xff",
        b"\xc3",
        "\u{85}".as_bytes(),
    ] {
        assert_eq!(keys(raw), [], "{raw:?}");
    }
    assert_eq!(keys(b"\x1b[Cq"), [Key::Char('q')]);
    assert_eq!(keys(b"\xffq"), [Key::Char('q')]);
}

#[test]
fn a_double_escape_is_two_escapes() {
    assert_eq!(keys_from_bytes(b"\x1b\x1b"), [Key::Esc, Key::Esc]);
}

fn toggled(s: &mut State, bypass: bool, no_sandbox: bool) {
    let fields = s.fields();
    for (i, f) in fields.iter().enumerate() {
        let want = match f {
            Field::Bypass => bypass,
            Field::NoSandbox => no_sandbox,
            Field::Folder => false,
        };
        if want {
            s.flag_cursor = i;
            s.on_key(Key::Char(' '));
        }
    }
}

#[test]
fn pick_agrees_with_argv_for_every_row_kind_and_toggle_combination() {
    for index in 1..=3 {
        for kind in ['r', 'f'] {
            for (bypass, no_sandbox) in [(false, false), (true, false), (false, true), (true, true)]
            {
                let mut s = state(sample_rows());
                to_session(&mut s);
                for _ in 0..index {
                    s.on_key(Key::Down);
                }
                s.on_key(Key::Enter);
                s.on_key(Key::Char(kind));
                if s.screen != Screen::Flags {
                    continue;
                }
                toggled(&mut s, bypass, no_sandbox);
                let argv = s.argv();
                let pick = s.pick().expect("an existing row");
                let ctx = format!("{index} {kind} {bypass} {no_sandbox}: {argv:?}");
                assert_eq!(pick.id, argv[2], "{ctx}");
                assert_eq!(pick.fork, argv.contains(&"--fork".to_string()), "{ctx}");
                assert_eq!(
                    pick.no_sandbox,
                    argv.contains(&"--no-sandbox".to_string()),
                    "{ctx}"
                );
                assert_eq!(
                    pick.bypass,
                    argv.windows(2)
                        .any(|w| w[0] == "--mode" && w[1] == "bypass"),
                    "{ctx}"
                );
                assert_eq!(
                    pick.tmgr,
                    s.rows.iter().find(|r| r.id == pick.id).unwrap().kind == RowKind::Tmgr,
                    "{ctx}"
                );
            }
        }
    }
}

#[test]
fn the_new_row_has_no_pick() {
    let mut s = state(sample_rows());
    to_session(&mut s);
    s.on_key(Key::Enter);
    assert_eq!(s.screen, Screen::Flags);
    assert_eq!(s.pick(), None);
}

#[test]
fn a_bypass_pick_of_a_tmgr_row_carries_the_lock_not_the_preference() {
    let mut s = state(sample_rows());
    open_row(&mut s, 3, 'r');
    let pick = s.pick().unwrap();
    assert!(pick.tmgr && !pick.fork && pick.no_sandbox && !pick.bypass);
}

fn only(installed: [bool; 2]) -> State {
    overlay(
        agents(installed),
        "/work/app".into(),
        None,
        Box::new(fake(sample_rows())),
    )
}

#[test]
fn the_overlay_starts_on_the_session_screen_with_every_flag_off() {
    let mut s = overlay(
        agents([true, true]),
        "/work/app".into(),
        Some("codex"),
        Box::new(fake(sample_rows())),
    );
    assert_eq!(s.screen, Screen::Session);
    assert_eq!(s.agent().id, "codex");
    assert!(s.loading || !s.rows.is_empty());
    s.pump();
    assert_eq!(s.rows.len(), 3);
    s.on_key(Key::Down);
    s.on_key(Key::Down);
    s.on_key(Key::Enter);
    s.on_key(Key::Char('r'));
    assert!(!s.bypass());
    assert!(!s.memory().bypass);
    assert!(!s.memory().no_sandbox);
    assert_eq!(s.memory().kind, Kind::Resume);
}

#[test]
fn the_overlay_falls_back_to_the_first_installed_agent_for_an_unknown_kind() {
    for kind in [None, Some(""), Some("shell"), Some("claude")] {
        let s = overlay(
            agents([true, true]),
            String::new(),
            kind,
            Box::new(fake(vec![])),
        );
        assert_eq!(s.agent().id, "claude-code", "{kind:?}");
        assert_eq!(s.screen, Screen::Session);
    }
    let s = overlay(
        agents([false, true]),
        String::new(),
        Some("claude-code"),
        Box::new(fake(vec![])),
    );
    assert_eq!(s.agent().id, "codex");
    assert_eq!(only([false, false]).screen, Screen::Agent);
}

#[test]
fn escape_on_the_session_screen_goes_back_to_the_agent_screen_and_escape_there_closes() {
    let mut s = only([true, true]);
    assert!(matches!(feed_keys(&mut s, b"\x1b"), Fed::Stay));
    assert_eq!(s.screen, Screen::Agent);
    assert!(matches!(feed_keys(&mut s, b"\x1b"), Fed::Close));
}

#[test]
fn ctrl_c_closes_from_every_screen() {
    let mut s = only([true, true]);
    assert!(matches!(feed_keys(&mut s, b"\x03"), Fed::Close));
    let mut s = only([true, true]);
    s.pump();
    feed_keys(&mut s, b"\x1b[B\rr");
    assert_eq!(s.screen, Screen::Flags);
    assert!(matches!(feed_keys(&mut s, b"\x03"), Fed::Close));
}

#[test]
fn enter_on_the_new_row_picks_nothing_and_enter_on_a_row_picks_it_after_the_flags() {
    let mut s = only([true, true]);
    s.pump();
    assert!(matches!(feed_keys(&mut s, b"\r\r"), Fed::Pick(None)));
    let mut s = only([true, true]);
    s.pump();
    assert!(matches!(feed_keys(&mut s, b"\x1b[B\x1b[B\rr"), Fed::Stay));
    let Fed::Pick(Some(p)) = feed_keys(&mut s, b"\r") else {
        panic!("expected a pick");
    };
    assert_eq!(p.id, "tm-2");
    assert!(p.tmgr && !p.fork && !p.bypass && !p.no_sandbox);
}

#[test]
fn input_after_a_closing_key_is_not_applied() {
    let mut s = only([true, true]);
    s.pump();
    assert!(matches!(feed_keys(&mut s, b"\x03abc"), Fed::Close));
    assert!(s.filter.is_empty());
}

#[test]
fn a_paste_goes_to_the_filter_without_its_markers_or_control_bytes() {
    let mut s = only([true, true]);
    s.pump();
    feed_paste(&mut s, b"\x1b[200~fix\nthe menu\x1b[201~");
    assert_eq!(s.filter, "fixthe menu");
}

#[test]
fn rows_arriving_later_do_not_block_the_overlay() {
    struct Slow(Cell<bool>);
    impl Source for Slow {
        fn load(&self, _: &str) {}
        fn poll(&self) -> Option<(Vec<Row>, Hints)> {
            self.0.get().then(|| (sample_rows(), Hints::default()))
        }
        fn search(&self, _: &str) -> Option<Vec<String>> {
            None
        }
        fn allow_reading(&self) -> Result<(), String> {
            Ok(())
        }
    }
    let ready = Cell::new(false);
    let mut s = overlay(
        agents([true, true]),
        String::new(),
        None,
        Box::new(Slow(ready.clone())),
    );
    assert!(s.loading);
    assert!(render(&s, 100, 12).contains("loading"));
    assert!(matches!(feed_keys(&mut s, b"\x1b[B\x1b[A"), Fed::Stay));
    ready.set(true);
    s.pump();
    assert_eq!(s.rows.len(), 3);
    assert!(matches!(feed_keys(&mut s, b"\r"), Fed::Pick(None)));
    assert_eq!(s.pick(), None);
}

fn render_in(
    s: &State,
    host: view::Host,
    width: u16,
    height: u16,
    area: ratatui::layout::Rect,
) -> String {
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    let look = view::Look {
        level: ColorLevel::None,
    };
    term.draw(|f| view::draw_in(f, area, s, look, host))
        .unwrap();
    let buf = term.backend().buffer();
    buf.content
        .chunks(buf.area.width as usize)
        .map(|row| {
            row.iter()
                .map(|c| c.symbol())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_attach_host_names_attach_and_says_close_not_quit() {
    let mut s = only([true, true]);
    let area = ratatui::layout::Rect::new(0, 0, 100, 14);
    s.pump();
    let session = render_in(&s, view::ATTACH, 100, 14, area);
    assert!(
        session.contains(" tmgr attach \u{b7} session "),
        "{session}"
    );
    assert!(
        session.contains("Ctrl+C close") && !session.contains("quit"),
        "{session}"
    );
    feed_keys(&mut s, b"\x1b[B\rr");
    let flags = render_in(&s, view::ATTACH, 100, 14, area);
    assert!(flags.contains(" tmgr attach \u{b7} start "), "{flags}");
    assert!(
        flags.contains("Ctrl+C close") && !flags.contains("quit"),
        "{flags}"
    );
    feed_keys(&mut s, b"\x1b\x1b");
    let agent = render_in(&s, view::ATTACH, 100, 14, area);
    assert!(agent.contains(" tmgr attach \u{b7} agent "), "{agent}");
    assert!(
        agent.contains("q or Esc close") && !agent.contains("quit"),
        "{agent}"
    );
}

#[test]
fn the_run_host_through_draw_in_matches_draw_byte_for_byte() {
    let mut s = state(sample_rows());
    for step in 0..3 {
        let full = ratatui::layout::Rect::new(0, 0, 100, 14);
        assert_eq!(
            render(&s, 100, 14).trim_end(),
            render_in(&s, view::RUN, 100, 14, full).trim_end()
        );
        assert!(render(&s, 100, 14).contains("tmgr run"));
        if step == 0 {
            to_session(&mut s);
        } else {
            s.on_key(Key::Enter);
        }
    }
}

#[test]
fn the_overlay_is_centred_capped_and_clears_what_is_behind_it() {
    let area = ratatui::layout::Rect::new(0, 0, 200, 60);
    let r = view::overlay_rect(area);
    assert!(r.width <= 120 && r.height <= 32 && r.width >= 40 && r.height >= 12);
    assert_eq!(r.x, (area.width - r.width) / 2);
    assert_eq!(r.y, (area.height - r.height) / 2);
    let small = ratatui::layout::Rect::new(0, 0, 30, 8);
    let r = view::overlay_rect(small);
    assert!(r.right() <= small.right() && r.bottom() <= small.bottom() && !r.is_empty());
    let s = only([true, true]);
    let mut term = Terminal::new(TestBackend::new(200, 60)).unwrap();
    let look = view::Look {
        level: ColorLevel::None,
    };
    term.draw(|f| {
        for y in 0..60 {
            f.render_widget(
                ratatui::widgets::Paragraph::new("x".repeat(200)),
                ratatui::layout::Rect::new(0, y, 200, 1),
            );
        }
        view::draw_in(f, view::overlay_rect(f.area()), &s, look, view::ATTACH);
    })
    .unwrap();
    let buf = term.backend().buffer();
    let inside = buf[(r_mid(area).0, r_mid(area).1)].symbol().to_string();
    assert_ne!(inside, "x");
    assert_eq!(buf[(0, 0)].symbol(), "x");
}

fn r_mid(area: ratatui::layout::Rect) -> (u16, u16) {
    let r = view::overlay_rect(area);
    (r.x + r.width - 3, r.y + r.height - 2)
}

#[test]
fn dropping_the_overlay_state_drops_its_source() {
    struct Watched(Cell<bool>);
    impl Drop for Watched {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    impl Source for Watched {
        fn load(&self, _: &str) {}
        fn poll(&self) -> Option<(Vec<Row>, Hints)> {
            None
        }
        fn search(&self, _: &str) -> Option<Vec<String>> {
            None
        }
        fn allow_reading(&self) -> Result<(), String> {
            Ok(())
        }
    }
    let dropped = Cell::new(false);
    let s = overlay(
        agents([true, true]),
        String::new(),
        None,
        Box::new(Watched(dropped.clone())),
    );
    assert!(!dropped.get());
    drop(s);
    assert!(dropped.get());
}

#[test]
fn a_whitespace_query_adds_no_name_tiers_and_keeps_the_hits() {
    let rows = vec![
        named("a", RowKind::External, "alpha", "x"),
        named("b", RowKind::External, "beta", "x"),
    ];
    let hits = vec!["b".to_string()];
    assert_eq!(super::rank::rank(&rows, "  ", Some(&hits)), vec![1]);
    assert_eq!(super::rank::rank(&rows, "  ", None), vec![0, 1]);
}

#[test]
fn enter_on_the_new_row_in_the_overlay_skips_the_flags_screen() {
    let mut s = only([true, true]);
    s.pump();
    assert!(matches!(feed_keys(&mut s, b"\r"), Fed::Pick(None)));
    assert_eq!(s.screen, Screen::Session);
}

#[test]
fn the_run_menu_still_shows_the_flags_for_the_new_row() {
    let mut s = state(sample_rows());
    to_session(&mut s);
    s.on_key(Key::Enter);
    assert_eq!(s.screen, Screen::Flags);
}

#[test]
fn the_overlay_redraws_only_while_something_is_changing() {
    let mut s = only([true, true]);
    assert!(!super::refresh(&mut s));
    s.loading = true;
    assert!(super::refresh(&mut s));
    s.loading = false;
    assert!(!super::refresh(&mut s));
    s.smart = Smart::Running("q".into());
    assert!(super::refresh(&mut s));
}
