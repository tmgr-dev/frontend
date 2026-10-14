use super::app::*;
use super::theme::{ColorLevel, Theme};
use super::view;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::Terminal;
use serde_json::{json, Value};
use tmgr_core::error::Kind;
use tmgr_core::models::{ApiTask, Status};

pub const NOW: i64 = 1_800_000_000;

fn status(id: u64, name: &str, kind: &str) -> Status {
    serde_json::from_value(json!({"id": id, "name": name, "type": kind})).unwrap()
}

pub fn task(id: u64, title: &str, status_id: u64, extra: Value) -> ApiTask {
    let mut v = json!({"id": id, "title": title, "status_id": status_id, "description": null});
    for (k, val) in extra.as_object().cloned().unwrap_or_default() {
        v[k] = val;
    }
    serde_json::from_value(v).unwrap()
}

pub fn snapshot() -> Snapshot {
    Snapshot {
        statuses: vec![
            status(1, "Backlog", "default"),
            status(2, "In progress", "active"),
            status(3, "Review", "active"),
            status(4, "Done", "completed"),
            status(9, "Parked", "hidden"),
            status(10, "Archive", "archived"),
        ],
        tasks: vec![
            task(
                101,
                "TM-101: Pricing page copy review",
                1,
                json!({"assignees": [{"id": 7, "name": "Ann"}]}),
            ),
            task(
                102,
                "TM-102: Fix flaky upload test",
                1,
                json!({"common_time": 7200}),
            ),
            task(
                103,
                "TM-103: Fix login redirect loop",
                2,
                json!({"start_time": NOW - 4320, "assignees": [{"id": 7, "name": "Ann"}]}),
            ),
            task(104, "TM-104: Invoice export bug", 3, json!({})),
            task(105, "TM-105: Footer links audit", 4, json!({})),
            task(106, "TM-106: Old thing", 10, json!({})),
        ],
        more: false,
    }
}

pub fn app() -> App {
    let mut app = App::new("Website relaunch", Some("TM"), false, Some(7), NOW);
    app.apply_snapshot(snapshot());
    app
}

pub fn key(c: KeyCode) -> Msg {
    Msg::Key(KeyEvent {
        code: c,
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

pub fn ch(c: char) -> Msg {
    key(KeyCode::Char(c))
}

fn ctrl(c: char) -> Msg {
    Msg::Key(KeyEvent {
        code: KeyCode::Char(c),
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    })
}

fn press(app: &mut App, keys: &str) -> Vec<Effect> {
    keys.chars().flat_map(|c| app.update(ch(c))).collect()
}

fn titles(app: &App, col: usize) -> Vec<String> {
    app.visible(col).iter().map(|c| c.key.clone()).collect()
}

fn fail(kind: Kind) -> Failure {
    Failure::new(kind, "boom")
}

#[test]
fn hidden_and_archived_statuses_are_not_columns() {
    let app = app();
    let names: Vec<_> = app.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["Backlog", "In progress", "Review", "Done"]);
    assert_eq!(app.visible_count(), 5);
}

#[test]
fn navigation_is_clamped() {
    let mut app = app();
    press(&mut app, "hkk");
    assert_eq!((app.col, app.row(0)), (0, 0));
    press(&mut app, "jjj");
    assert_eq!(app.row(0), 1);
    press(&mut app, "llll");
    assert_eq!(app.col, 3);
    assert_eq!(app.selected_id(), Some(105));
}

#[test]
fn moving_a_card_is_optimistic_and_follows_the_card() {
    let mut app = app();
    let effects = press(&mut app, "L");
    assert_eq!(effects, vec![Effect::Move { id: 101, to: 2 }]);
    assert_eq!(titles(&app, 0), ["TM-102"]);
    assert_eq!(titles(&app, 1), ["TM-101", "TM-103"].map(String::from));
    assert_eq!((app.col, app.selected_id()), (1, Some(101)));
}

#[test]
fn moving_past_the_edge_does_nothing() {
    let mut app = app();
    assert!(press(&mut app, "H").is_empty());
    press(&mut app, "lll");
    assert!(press(&mut app, "L").is_empty());
}

#[test]
fn a_failed_move_rolls_back_to_the_old_column_and_card() {
    let mut app = app();
    press(&mut app, "j");
    press(&mut app, "L");
    assert_eq!(titles(&app, 0), ["TM-101"]);
    app.update(Msg::MoveDone(102, Err(fail(Kind::Offline))));
    assert_eq!(titles(&app, 0), ["TM-101", "TM-102"].map(String::from));
    assert_eq!(app.selected_id(), Some(102));
    assert!(app.notice.as_ref().is_some_and(|n| n.error));
    assert!(app.offline);
}

#[test]
fn a_poll_that_lands_before_the_move_is_acknowledged_does_not_undo_it() {
    let mut app = app();
    press(&mut app, "L");
    app.update(Msg::Loaded(Ok(snapshot())));
    assert_eq!(titles(&app, 1), ["TM-101", "TM-103"].map(String::from));
    let after = app.update(Msg::MoveDone(101, Ok(())));
    assert_eq!(after, vec![Effect::Refresh]);
}

#[test]
fn selection_follows_the_task_across_refreshes() {
    let mut app = app();
    press(&mut app, "lj");
    assert_eq!(app.col, 1);
    let mut snap = snapshot();
    snap.tasks.reverse();
    snap.tasks.push(task(110, "TM-110: New", 2, json!({})));
    snap.tasks
        .insert(0, task(111, "TM-111: Newer", 2, json!({})));
    app.update(Msg::Loaded(Ok(snap)));
    assert_eq!(app.selected_id(), Some(103));
}

#[test]
fn filter_matches_key_and_title_and_q_types_instead_of_quitting() {
    let mut app = app();
    app.update(ch('/'));
    assert_eq!(app.mode, Mode::Filter);
    press(&mut app, "q");
    assert!(!app.quit);
    assert_eq!(app.visible_count(), 0);
    app.update(key(KeyCode::Backspace));
    press(&mut app, "tm-10");
    assert_eq!(app.visible_count(), 5);
    app.update(key(KeyCode::Esc));
    assert_eq!(app.visible_count(), 5);
    app.update(ch('/'));
    press(&mut app, "login");
    app.update(key(KeyCode::Enter));
    assert_eq!(app.mode, Mode::Normal);
    assert_eq!(app.visible_count(), 1);
    assert_eq!(app.selected_id(), Some(103));
    app.update(ch('/'));
    app.update(key(KeyCode::Esc));
    assert_eq!(app.visible_count(), 5);
}

#[test]
fn ctrl_c_and_esc_quit() {
    let mut app = app();
    app.update(ch('c'));
    assert_eq!(app.mode, Mode::Comment);
    app.update(ctrl('c'));
    assert!(app.quit);
    let mut app = self::app();
    app.update(key(KeyCode::Esc));
    assert!(app.quit);
}

#[test]
fn comment_flow_sends_trimmed_text_and_ignores_empty() {
    let mut app = app();
    app.update(ch('c'));
    assert!(app.update(key(KeyCode::Enter)).is_empty());
    app.update(ch('c'));
    press(&mut app, " ship it ");
    let effects = app.update(key(KeyCode::Enter));
    assert_eq!(
        effects,
        vec![Effect::Comment {
            id: 101,
            text: "ship it".into()
        }]
    );
    assert_eq!(app.mode, Mode::Normal);
}

#[test]
fn timer_keys_are_optimistic_and_roll_back() {
    let mut app = app();
    assert_eq!(
        press(&mut app, "s"),
        vec![Effect::Timer {
            id: 101,
            start: true
        }]
    );
    assert!(app.selected().unwrap().running);
    assert!(press(&mut app, "s").is_empty());
    app.update(Msg::TimerDone(101, true, Err(fail(Kind::Forbidden))));
    assert!(!app.selected().unwrap().running);
    press(&mut app, "l");
    assert_eq!(
        press(&mut app, "S"),
        vec![Effect::Timer {
            id: 103,
            start: false
        }]
    );
    assert!(!app.selected().unwrap().running);
}

#[test]
fn polling_runs_every_thirty_seconds_and_not_while_loading() {
    let mut app = app();
    assert!(app.update(Msg::Tick(NOW + 29)).is_empty());
    assert_eq!(app.update(Msg::Tick(NOW + 30)), vec![Effect::Refresh]);
    assert!(app.update(Msg::Tick(NOW + 90)).is_empty());
    app.update(Msg::Loaded(Ok(snapshot())));
    assert!(app.update(Msg::Tick(NOW + 100)).is_empty());
    assert_eq!(app.update(Msg::Tick(NOW + 121)), vec![Effect::Refresh]);
}

#[test]
fn details_load_once_per_card_and_after_comments() {
    let mut app = app();
    assert_eq!(
        app.update(key(KeyCode::Enter)),
        vec![Effect::LoadDetail(101)]
    );
    app.update(Msg::Detail(101, Box::new(Ok(Detail::default()))));
    assert!(app.update(ch('k')).is_empty());
    assert_eq!(app.update(ch('j')), vec![Effect::LoadDetail(102)]);
    let after = app.update(Msg::CommentDone(102, Ok(())));
    assert_eq!(after, vec![Effect::LoadDetail(102)]);
}

#[test]
fn mine_keeps_only_tasks_assigned_to_me() {
    let mut app = App::new("W", None, true, Some(7), NOW);
    app.apply_snapshot(snapshot());
    assert_eq!(app.visible_count(), 2);
}

#[test]
fn an_auth_failure_ends_the_session() {
    let mut app = app();
    app.update(Msg::Loaded(Err(fail(Kind::Auth))));
    assert!(app.quit);
    assert_eq!(app.fatal.unwrap().kind, Kind::Auth);
}

#[test]
fn flat_mode_walks_across_columns() {
    let mut app = app();
    app.update(Msg::Resize(50));
    press(&mut app, "jj");
    assert_eq!(app.selected_id(), Some(103));
    press(&mut app, "k");
    assert_eq!(app.selected_id(), Some(102));
    press(&mut app, "k");
    assert_eq!(app.selected_id(), Some(101));
}

pub fn render(app: &App, w: u16, h: u16, theme: &Theme) -> String {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| view::draw(f, app, theme)).unwrap();
    let buf = term.backend().buffer().clone();
    (0..h)
        .map(|y| {
            (0..w)
                .map(|x| buf[(x, y)].symbol().to_string())
                .collect::<String>()
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn plain() -> Theme {
    Theme::new(ColorLevel::None, false)
}

fn assert_snapshot(name: &str, actual: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/cmd/board/snapshots")
        .join(format!("{name}.txt"));
    if std::env::var_os("TMGR_UPDATE_SNAPSHOTS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("{actual}\n")).unwrap();
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(expected.trim_end_matches('\n'), actual, "snapshot {name}");
}

#[test]
fn snapshot_wide() {
    let mut app = app();
    app.update(Msg::Resize(120));
    assert_snapshot("wide", &render(&app, 120, 14, &plain()));
}

#[test]
fn snapshot_narrow_tabs() {
    let mut app = app();
    app.update(Msg::Resize(80));
    app.update(ch('l'));
    assert_snapshot("narrow", &render(&app, 80, 12, &plain()));
}

#[test]
fn snapshot_very_narrow_list() {
    let mut app = app();
    app.update(Msg::Resize(50));
    assert_snapshot("very_narrow", &render(&app, 50, 14, &plain()));
}

#[test]
fn snapshot_ascii() {
    let mut app = app();
    app.update(Msg::Resize(120));
    assert_snapshot(
        "ascii",
        &render(&app, 120, 10, &Theme::new(ColorLevel::None, true)),
    );
}

#[test]
fn snapshot_details_pane() {
    let mut app = app();
    app.update(Msg::Resize(120));
    app.update(key(KeyCode::Enter));
    let d = Detail {
        description: Some("Reproduce with an expired cookie.\nSecond line".into()),
        comments: vec![CommentLine {
            author: "Ann".into(),
            text: "looks good".into(),
        }],
        runs: vec![Run {
            agent: "claude-code".into(),
            model: "opus".into(),
            status: "succeeded".into(),
            seconds: 600,
            branch: "tm-101-x".into(),
            pr: String::new(),
            passed: Some(56),
            failed: Some(0),
        }],
        runs_available: true,
        error: None,
    };
    app.update(Msg::Detail(101, Box::new(Ok(d))));
    assert_snapshot("details", &render(&app, 120, 26, &plain()));
}

#[test]
fn snapshot_help_overlay() {
    let mut app = app();
    app.update(Msg::Resize(80));
    app.update(ch('?'));
    assert_snapshot("help", &render(&app, 80, 20, &plain()));
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> Msg {
    Msg::Mouse(MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    })
}

fn find(out: &str, needle: &str) -> (u16, u16) {
    for (y, line) in out.lines().enumerate() {
        if let Some(x) = line.find(needle) {
            return (line[..x].chars().count() as u16, y as u16);
        }
    }
    panic!("{needle} not on screen");
}

#[test]
fn clicking_a_card_selects_it_and_the_wheel_moves() {
    let mut app = app();
    app.update(Msg::Resize(120));
    let out = render(&app, 120, 16, &plain());
    let (x, y) = find(&out, "TM-104");
    app.update(mouse(MouseEventKind::Down(MouseButton::Left), x + 2, y));
    assert_eq!(app.selected_id(), Some(104));
    let (x, y) = find(&out, "TM-102");
    app.update(mouse(MouseEventKind::Down(MouseButton::Left), x, y + 1));
    assert_eq!(app.selected_id(), Some(102));
    app.update(mouse(MouseEventKind::ScrollUp, 0, 0));
    assert_eq!(app.selected_id(), Some(101));
    app.update(mouse(MouseEventKind::ScrollDown, 0, 0));
    assert_eq!(app.selected_id(), Some(102));
}

#[test]
fn clicking_a_tab_switches_the_column() {
    let mut app = app();
    app.update(Msg::Resize(80));
    let out = render(&app, 80, 12, &plain());
    let (x, y) = find(&out, "Review");
    app.update(mouse(MouseEventKind::Down(MouseButton::Left), x, y));
    assert_eq!(app.col, 2);
}

#[test]
fn hostile_server_text_never_reaches_the_screen() {
    let evil = "\u{1b}[31mRED\u{1b}]0;pwn\u{7}\u{202e}rtl\tTab";
    let mut snap = snapshot();
    snap.statuses[1].name = format!("In{evil}");
    snap.tasks.push(task(
        120,
        &format!("TM-120: {evil}"),
        2,
        json!({"description": evil, "assignees": [{"id": 1, "name": evil}]}),
    ));
    let mut app = App::new(evil, Some(evil), false, None, NOW);
    app.apply_snapshot(snap);
    app.update(Msg::Resize(120));
    app.update(ch('l'));
    app.update(ch('j'));
    app.update(ch('j'));
    app.update(key(KeyCode::Enter));
    let d = Detail {
        description: Some(plain_text(evil)),
        comments: vec![CommentLine {
            author: Failure::new(Kind::Internal, evil).message,
            text: Failure::new(Kind::Internal, evil).message,
        }],
        runs: vec![],
        runs_available: false,
        error: None,
    };
    app.update(Msg::Detail(120, Box::new(Ok(d))));
    app.update(Msg::MoveDone(120, Err(Failure::new(Kind::Internal, evil))));
    let theme = plain();
    for (w, h) in [(120, 24), (80, 14), (50, 14)] {
        let out = render(&app, w, h, &theme);
        assert!(out.contains("RED"), "{w}: {out}");
        assert!(
            !out.chars().any(|c| c.is_control() && c != '\n') && !out.contains('\u{202e}'),
            "{w}: {out:?}"
        );
    }
}

#[test]
fn the_worker_of_a_local_board_writes_to_the_local_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let (workspace, user) = super::data::local_fixture(dir.path());
    let ws_id = workspace.manifest.id;
    let db = workspace.database.clone();
    let (eff_tx, eff_rx) = std::sync::mpsc::channel();
    let (msg_tx, msg_rx) = std::sync::mpsc::channel();
    super::spawn_worker(
        super::Conn::Local { workspace, user },
        ws_id,
        None,
        eff_rx,
        msg_tx,
    );
    eff_tx.send(Effect::Move { id: 1, to: 2 }).unwrap();
    let msg = msg_rx
        .recv_timeout(std::time::Duration::from_secs(10))
        .unwrap();
    assert!(matches!(msg, Msg::MoveDone(1, Ok(()))));
    let status: i64 = rusqlite::Connection::open(db)
        .unwrap()
        .query_row("SELECT status_id FROM tasks WHERE id = 1", [], |r| r.get(0))
        .unwrap();
    assert_eq!(status, 2);
}
