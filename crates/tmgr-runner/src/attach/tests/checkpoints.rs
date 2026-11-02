use super::*;
use std::time::{Duration, Instant};
use tmgr_ipc::api::{
    CheckpointDiff, CheckpointItem, CheckpointKind, CheckpointList, CheckpointRestored,
    CheckpointStatus,
};

fn status(count: u32) -> CheckpointStatus {
    CheckpointStatus {
        available: true,
        note: None,
        count,
    }
}

fn item(n: u64, kind: CheckpointKind, turn: u64, files: u32) -> CheckpointItem {
    CheckpointItem {
        n,
        kind,
        turn,
        files,
        skipped: 0,
        at_ms: 1_700_000_000_000 + n * 60_000,
        restored_from: None,
    }
}

fn clock(it: &CheckpointItem) -> String {
    use chrono::TimeZone;
    chrono::Local
        .timestamp_millis_opt(it.at_ms as i64)
        .unwrap()
        .format("%H:%M")
        .to_string()
}

fn items() -> Vec<CheckpointItem> {
    let mut restored = item(6, CheckpointKind::Restored, 5, 0);
    restored.restored_from = Some(4);
    let mut skipped = item(4, CheckpointKind::Turn, 3, 1);
    skipped.skipped = 2;
    vec![
        item(1, CheckpointKind::Start, 0, 0),
        item(2, CheckpointKind::Turn, 1, 3),
        skipped,
        item(5, CheckpointKind::BeforeRestore, 3, 0),
        restored,
    ]
}

fn list() -> CheckpointList {
    CheckpointList {
        status: status(5),
        items: items(),
    }
}

fn cp_view(width: u16) -> ViewState {
    let mut v = two_pane(width);
    let id = v.focused_session().unwrap().to_string();
    v.set_checkpoints(HashMap::from([(id, status(5))]));
    v
}

fn opened() -> ViewState {
    let mut v = cp_view(140);
    v.apply(Cmd::Checkpoints);
    let id = v.focused_session().unwrap().to_string();
    assert_eq!(v.cp_due(Instant::now()), vec![CpFetch::List(id.clone())]);
    v.cp_msg(CpMsg::List(id, Ok(list())));
    v
}

fn diff(modified: u32, added: u32, deleted: u32) -> CheckpointDiff {
    CheckpointDiff {
        n: 0,
        text: String::new(),
        truncated: false,
        added,
        modified,
        deleted,
    }
}

fn sid(v: &ViewState) -> String {
    v.focused_session().unwrap().to_string()
}

fn shown(v: &ViewState) -> String {
    draw(v, &screens(), 140, 40)
}

#[test]
fn the_checkpoints_key_is_free_and_subagents_keep_theirs() {
    let c: Vec<_> = ACTIONS.iter().filter(|a| a.keys.contains(&b'C')).collect();
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].cmd, Cmd::Checkpoints);
    assert!(c[0].palette);
    let u: Vec<_> = ACTIONS.iter().filter(|a| a.keys.contains(&b'u')).collect();
    assert_eq!(u.len(), 1);
    assert_eq!(u[0].cmd, Cmd::Subagents);
}

#[test]
fn sessions_without_checkpoints_render_exactly_as_before() {
    let v = two_pane(140);
    let plain = draw(&v, &screens(), 140, 22);
    assert_snapshot("wide", &plain);
    let mut other = two_pane(140);
    other.set_checkpoints(HashMap::from([("zzz".to_string(), status(3))]));
    assert_eq!(draw(&other, &screens(), 140, 22), plain);
    assert!(!plain.contains('↶'));
}

#[test]
fn the_header_shows_the_count_or_no_git() {
    let mut v = cp_view(140);
    assert!(shown(&v).contains("↶ 5"));
    let id = sid(&v);
    v.set_checkpoints(HashMap::from([(
        id,
        CheckpointStatus {
            available: false,
            note: Some("not a git repository".into()),
            count: 0,
        },
    )]));
    let out = shown(&v);
    assert!(out.contains("↶ no git"));
    assert!(!out.contains("↶ 0"));
}

#[test]
fn clicking_the_header_control_opens_the_panel() {
    let mut v = cp_view(140);
    let info = mouse_info(sgr_mode(MouseLevel::Any), 0);
    let lay = layout(AREA, &v);
    let p = lay
        .panes
        .iter()
        .find(|p| Some(p.session.as_str()) == v.focused_session())
        .unwrap();
    let (x, w) = super::super::render::header_checkpoints(&v, p).unwrap();
    assert!(w > 2);
    v.on_mouse(AREA, ev(0, x + 1, p.outer.y, false), &info);
    assert!(v.cp.is_open());
}

#[test]
fn the_list_is_newest_first_with_kind_files_skipped_and_time() {
    let v = opened();
    let out = shown(&v);
    let rows: Vec<&str> = out.lines().filter(|l| l.contains('#')).collect();
    let find = |needle: &str| rows.iter().position(|l| l.contains(needle)).unwrap();
    let it = items();
    assert!(out.contains(&format!("#2 after turn 1 · 3 files · {}", clock(&it[1]))));
    assert!(out.contains(&format!(
        "#4 after turn 3 · 1 file · 2 skipped · {}",
        clock(&it[2])
    )));
    assert!(out.contains(&format!("#1 session start · {}", clock(&it[0]))));
    assert!(out.contains(&format!("#5 before restore · {}", clock(&it[3]))));
    assert!(out.contains(&format!("#6 restored #4 · {}", clock(&it[4]))));
    assert!(find("#6 restored") < find("#5 before") && find("#2 after") < find("#1 session"));
    assert!(!out.contains("#2 after turn 1 · 3 files · 2 skipped"));
}

#[test]
fn moving_the_selection_is_debounced_and_previews_are_cached() {
    let mut v = opened();
    let id = sid(&v);
    let now = Instant::now() + Duration::from_secs(1);
    assert_eq!(v.cp.selected_n(), Some(6));
    assert_eq!(v.cp_due(now), vec![CpFetch::Diff(id.clone(), 6)]);
    assert_eq!(v.cp_due(now), vec![]);
    v.cp_msg(CpMsg::Diff(id.clone(), 6, Ok(diff(1, 0, 0))));
    for _ in 0..3 {
        v.edit_cp(b"\x1b[B");
    }
    assert_eq!(v.cp.selected_n(), Some(2));
    assert_eq!(v.cp_due(Instant::now()), vec![]);
    let later = Instant::now() + Duration::from_secs(1);
    assert_eq!(v.cp_due(later), vec![CpFetch::Diff(id.clone(), 2)]);
    v.cp_msg(CpMsg::Diff(id.clone(), 2, Ok(diff(1, 0, 0))));
    v.edit_cp(b"\x1b[A");
    v.edit_cp(b"\x1b[B");
    assert_eq!(v.cp_due(later + Duration::from_secs(1)), vec![]);
}

#[test]
fn the_preview_shows_text_truncation_and_no_control_bytes() {
    let mut v = opened();
    let id = sid(&v);
    v.cp_due(Instant::now() + Duration::from_secs(1));
    let mut d = diff(1, 0, 0);
    d.text = "+++ b/a.rs\n+new\x1b[2J line\n-old\n".into();
    d.truncated = true;
    v.cp_msg(CpMsg::Diff(id, 6, Ok(d)));
    let out = shown(&v);
    assert!(out.contains("+new[2J line") || out.contains("+new line"));
    assert!(!out.contains('\x1b'));
    assert!(out.contains("diff truncated"));
}

#[test]
fn restore_asks_with_the_counts_and_only_y_runs_it() {
    let mut v = opened();
    let id = sid(&v);
    assert_eq!(v.edit_cp(b"r"), Some(CpAction::Check(id.clone(), 6)));
    v.cp_msg(CpMsg::Check(id.clone(), 6, Ok(diff(3, 2, 1))));
    let out = shown(&v);
    assert!(out.contains(
        "Restore to #6? 4 files change, 2 are removed. Current state is saved first. y/n"
    ));
    assert_eq!(v.edit_cp(b"y"), Some(CpAction::Restore(id.clone(), 6)));
    assert!(shown(&v).contains("Restoring..."));

    for key in [&b"n"[..], b"\r", b"Y", b"\x1b", b" ", b"x", b"\x1b[A"] {
        let mut v = opened();
        v.edit_cp(b"r");
        v.cp_msg(CpMsg::Check(id.clone(), 6, Ok(diff(1, 0, 0))));
        assert_eq!(v.edit_cp(key), None, "{key:?}");
        assert!(v.cp.is_open(), "{key:?}");
        assert!(!shown(&v).contains("Restoring..."), "{key:?}");
        assert!(!shown(&v).contains("y/n"), "{key:?}");
    }
}

#[test]
fn the_prompt_uses_singular_forms() {
    let mut v = opened();
    let id = sid(&v);
    v.edit_cp(b"r");
    v.cp_msg(CpMsg::Check(id, 6, Ok(diff(1, 1, 0))));
    assert!(shown(&v).contains("1 file changes, 1 is removed."));
}

#[test]
fn a_finished_restore_reports_counts_undo_and_left_as_is() {
    let mut v = opened();
    let id = sid(&v);
    v.edit_cp(b"r");
    v.cp_msg(CpMsg::Check(id.clone(), 6, Ok(diff(1, 0, 0))));
    v.edit_cp(b"y");
    v.cp_msg(CpMsg::Restored(
        id.clone(),
        6,
        Ok(CheckpointRestored {
            n: 6,
            before_n: 7,
            after_n: 8,
            written: 3,
            deleted: 1,
            left_as_is: Vec::new(),
        }),
    ));
    let out = shown(&v);
    assert!(out.contains("Restored #6. 3 written, 1 removed. Undo: pick #7."));
    assert!(!out.contains("left as is"));
    let fetch = v.cp_due(Instant::now());
    assert!(fetch.contains(&CpFetch::List(id.clone())));

    let mut v = opened();
    v.edit_cp(b"r");
    v.cp_msg(CpMsg::Check(id.clone(), 6, Ok(diff(1, 0, 0))));
    v.edit_cp(b"y");
    v.cp_msg(CpMsg::Restored(
        id,
        6,
        Ok(CheckpointRestored {
            n: 6,
            before_n: 7,
            after_n: 8,
            written: 3,
            deleted: 1,
            left_as_is: vec!["src/a.rs".into(), "b\x1b[31m.txt".into()],
        }),
    ));
    let out = shown(&v);
    assert!(out.contains("2 left as is (changed since)"));
    v.edit_cp(b"p");
    let out = shown(&v);
    assert!(out.contains("src/a.rs"));
    assert!(!out.contains('\x1b'));
}

#[test]
fn a_busy_daemon_reads_as_the_agent_is_working() {
    assert_eq!(
        checkpoints_error("busy: the session is still running"),
        "The agent is working. Try when it stops."
    );
    let mut v = opened();
    let id = sid(&v);
    v.edit_cp(b"r");
    v.cp_msg(CpMsg::Check(id.clone(), 6, Ok(diff(1, 0, 0))));
    v.edit_cp(b"y");
    v.cp_msg(CpMsg::Restored(
        id,
        6,
        Err(checkpoints_error("busy: the session is still running")),
    ));
    assert!(shown(&v).contains("The agent is working. Try when it stops."));
    assert_eq!(
        checkpoints_error("invalid_params: no such checkpoint"),
        "no such checkpoint"
    );
}

#[test]
fn an_off_session_says_how_to_turn_checkpoints_on() {
    let mut v = two_pane(140);
    v.apply(Cmd::Checkpoints);
    assert!(v.cp.is_open());
    let out = shown(&v).replace('\n', " ");
    let flat: String = out.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(flat.contains("Checkpoints are off for this session."));
    assert!(flat.contains("--checkpoints"));
    assert!(flat.contains("attach.checkpoints"));
    assert_eq!(v.cp_due(Instant::now()), vec![]);
    v.edit_cp(b"\x1b");
    assert!(!v.cp.is_open());
}

#[test]
fn an_unavailable_session_shows_the_note() {
    let mut v = two_pane(140);
    let id = sid(&v);
    v.set_checkpoints(HashMap::from([(
        id,
        CheckpointStatus {
            available: false,
            note: Some("the folder is not a git repository\x1b[2J".into()),
            count: 0,
        },
    )]));
    v.apply(Cmd::Checkpoints);
    let out = shown(&v);
    assert!(out.contains("the folder is not a git repository"));
    assert!(!out.contains('\x1b'));
    assert_eq!(v.cp_due(Instant::now()), vec![]);
}

#[test]
fn the_panel_closes_when_its_session_goes_away() {
    let mut v = opened();
    v.sessions_changed(AREA, vec![]);
    v.set_checkpoints(HashMap::new());
    assert!(!v.cp.is_open());
}

mod form {
    use super::*;
    use crate::attach::{KindOpt, NewEdit, NewField, SpawnKind};

    fn open(default: bool) -> ViewState {
        let mut v = ViewState::new(Vec::new(), "r");
        v.cp.form_default = default;
        let kinds = vec![KindOpt {
            id: "shell".into(),
            label: "shell".into(),
            spawn: SpawnKind::Shell,
        }];
        v.open_new_session("/".into(), kinds, "/".into());
        v
    }

    #[test]
    fn the_toggle_defaults_to_the_config_and_is_off_when_unset() {
        let v = open(false);
        assert!(!v.new_session.as_ref().unwrap().checkpoints);
        let v = open(true);
        assert!(v.new_session.as_ref().unwrap().checkpoints);
    }

    #[test]
    fn the_toggle_flips_and_reaches_the_request() {
        let mut v = open(false);
        v.new_session.as_mut().unwrap().focus = NewField::Checkpoints;
        v.edit_new_session(b"\x1b[C");
        assert!(v.new_session.as_ref().unwrap().checkpoints);
        v.edit_new_session(b"\x1b[D");
        assert!(!v.new_session.as_ref().unwrap().checkpoints);
        v.edit_new_session(b"\x1b[C");
        v.new_session.as_mut().unwrap().focus = NewField::Start;
        match v.edit_new_session(b"\r") {
            NewEdit::Submit(req) => assert_eq!(req.checkpoints, Some(true)),
            other => panic!("{other:?}"),
        }
    }
}
