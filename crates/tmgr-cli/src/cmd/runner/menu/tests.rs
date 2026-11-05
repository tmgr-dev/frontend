use super::memory::{self, Kind, Memory};
use super::state::*;
use super::view;
use super::{settle, wanted, Entry, Failures, MAX_POLL_ERRORS};
use crate::cmd::board::theme::ColorLevel;
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use std::cell::{Cell, RefCell};
use std::ffi::OsString;
use std::rc::Rc;

struct Fake {
    rows: Rc<RefCell<Vec<Row>>>,
    hints: Rc<Cell<bool>>,
    ranked: Option<Vec<String>>,
    allowed: Rc<Cell<u32>>,
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
        rows: Rc::new(RefCell::new(rows)),
        hints: Rc::new(Cell::new(false)),
        ranked: None,
        allowed: Rc::new(Cell::new(0)),
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
    struct Slow(Rc<Cell<bool>>);
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
    let ready = Rc::new(Cell::new(false));
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
    struct Reorder(Rc<Cell<bool>>);
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
    let changed = Rc::new(Cell::new(false));
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
