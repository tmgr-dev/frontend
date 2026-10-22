use super::export::Stage;
use super::view::{MAX_SIDEBAR_WIDTH, MIN_SIDEBAR_WIDTH, SIDEBAR_WIDTH};
use super::*;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::Terminal;
use std::collections::HashMap;
use tmgr_ipc::api::{Attr, Color, Line, Modes, Snapshot, Span, BOLD};
use tmgr_ipc::api::{MouseEncoding, MouseLevel, MouseMode, State, StateKind};

fn forward(b: &[u8]) -> Action {
    Action::Forward(b.to_vec())
}

#[test]
fn plain_bytes_are_forwarded_in_one_run() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"hello\r"), vec![forward(b"hello\r")]);
    assert_eq!(s.feed(b""), vec![]);
}

#[test]
fn prefix_commands_map_to_cmds() {
    let table: &[(u8, Cmd)] = &[
        (b'd', Cmd::Detach),
        (b'n', Cmd::NextSession),
        (b'p', Cmd::PrevSession),
        (b's', Cmd::Split),
        (b'\t', Cmd::FocusOther),
        (b'z', Cmd::Zoom),
        (b'b', Cmd::Sidebar),
        (b'?', Cmd::Help),
        (b'w', Cmd::Picker),
        (b'\r', Cmd::Resume),
        (b'R', Cmd::Resume),
    ];
    for (byte, cmd) in table {
        let mut s = InputSplitter::new();
        assert_eq!(s.feed(&[0x1d, *byte]), vec![Action::Cmd(*cmd)]);
    }
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(&[0x1d, b'[']), vec![Action::Cmd(Cmd::ScrollMode)]);
}

#[test]
fn text_around_a_command_is_split_into_runs() {
    let mut s = InputSplitter::new();
    assert_eq!(
        s.feed(b"ab\x1dnc\x1b[A"),
        vec![
            forward(b"ab"),
            Action::Cmd(Cmd::NextSession),
            forward(b"c\x1b[A")
        ]
    );
}

#[test]
fn prefix_split_across_feeds() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"x\x1d"), vec![forward(b"x")]);
    assert_eq!(s.feed(b"d"), vec![Action::Cmd(Cmd::Detach)]);
    assert_eq!(s.feed(b"d"), vec![forward(b"d")]);
}

#[test]
fn prefix_twice_sends_literal_ctrl_bracket() {
    let mut s = InputSplitter::new();
    assert_eq!(
        s.feed(&[b'a', 0x1d, 0x1d, b'b']),
        vec![forward(&[b'a', 0x1d, b'b'])]
    );
}

#[test]
fn unknown_byte_after_prefix_is_dropped() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(&[0x1d, b'k', b'y']), vec![forward(b"y")]);
    assert_eq!(s.feed(&[0x1d, 0x1b, b'x']), vec![]);
}

#[test]
fn large_bracketed_paste_is_byte_exact() {
    let mut data = b"\x1b[200~".to_vec();
    data.extend((0..4096u32).map(|i| (i % 251) as u8).filter(|b| *b != 0x1d));
    data.extend_from_slice(b"\x1b[201~");
    let mut s = InputSplitter::new();
    assert_eq!(
        s.feed(&data),
        vec![Action::Paste {
            data: data.clone(),
            start: true,
            end: true
        }]
    );
}

#[test]
fn escape_sequences_pass_through_unchanged() {
    let seqs: &[&[u8]] = &[
        b"\x1b[1;5C",
        b"\x1bb",
        b"\x1b[97;5u",
        b"\x1b[?u",
        "héllo 世".as_bytes(),
    ];
    for seq in seqs {
        let mut s = InputSplitter::new();
        assert_eq!(s.feed(seq), vec![forward(seq)]);
    }
}

fn scroll(bytes: &[u8]) -> Vec<Action> {
    let mut s = InputSplitter::new();
    s.set_scroll_mode(true);
    s.feed(bytes)
}

#[test]
fn scroll_mode_keys() {
    let k = |key| vec![Action::Scroll(key)];
    assert_eq!(scroll(b"\x1b[5~"), k(ScrollKey::PageUp));
    assert_eq!(scroll(b"\x1b[6~"), k(ScrollKey::PageDown));
    assert_eq!(scroll(b"\x1b[A"), k(ScrollKey::Up));
    assert_eq!(scroll(b"\x1b[B"), k(ScrollKey::Down));
    assert_eq!(scroll(b"\x1bOA"), k(ScrollKey::Up));
    assert_eq!(scroll(b"\x1bOB"), k(ScrollKey::Down));
    assert_eq!(scroll(b"g"), k(ScrollKey::Top));
    assert_eq!(scroll(b"G"), k(ScrollKey::Bottom));
    assert_eq!(scroll(b"q"), k(ScrollKey::Exit));
}

#[test]
fn scroll_mode_forwards_nothing() {
    assert_eq!(scroll(b"fopRx\x1b[1;5Czz"), vec![]);
    assert_eq!(
        scroll(b"a\x1b[5~b\x1b[6~"),
        vec![
            Action::Scroll(ScrollKey::PageUp),
            Action::Scroll(ScrollKey::PageDown)
        ]
    );
}

#[test]
fn escape_at_end_of_feed_is_a_lone_escape_and_exits_scroll() {
    let later = || std::time::Instant::now() + std::time::Duration::from_millis(50);
    let mut s = InputSplitter::new();
    s.set_scroll_mode(true);
    assert_eq!(s.feed(b"\x1b"), vec![]);
    assert_eq!(
        s.flush_stale(later()),
        vec![Action::Scroll(ScrollKey::Exit)]
    );
    s.set_scroll_mode(true);
    let mut got = s.feed(b"\x1b[5~\x1b");
    got.extend(s.flush_stale(later()));
    assert_eq!(
        got,
        vec![
            Action::Scroll(ScrollKey::PageUp),
            Action::Scroll(ScrollKey::Exit)
        ]
    );
}

#[test]
fn q_exits_scroll_and_input_flows_again() {
    let mut s = InputSplitter::new();
    s.set_scroll_mode(true);
    assert_eq!(
        s.feed(b"qhi"),
        vec![Action::Scroll(ScrollKey::Exit), forward(b"hi")]
    );
    assert_eq!(s.feed(b"x"), vec![forward(b"x")]);
}

#[test]
fn scroll_command_switches_the_splitter_to_scroll_keys() {
    let mut s = InputSplitter::new();
    assert_eq!(
        s.feed(b"\x1d[\x1b[5~"),
        vec![
            Action::Cmd(Cmd::ScrollMode),
            Action::Scroll(ScrollKey::PageUp)
        ]
    );
}

fn st(kind: StateKind, code: Option<i32>) -> State {
    State {
        kind,
        since_ms: 0,
        detail: None,
        exit_code: code,
    }
}

fn row(id: &str, name: &str, kind: StateKind, age: &str) -> SessionRow {
    let code = (kind == StateKind::Exited).then_some(0);
    SessionRow {
        id: id.into(),
        name: name.into(),
        state: st(kind, code),
        age: age.into(),
        you: None,
        esc: false,
        resumed_from: None,
    }
}

fn four() -> Vec<SessionRow> {
    vec![
        row("a", "claude", StateKind::Working, "18m"),
        row("b", "dev-server", StateKind::Waiting, "12m"),
        row("c", "codex", StateKind::Blocked, "3m"),
        row("d", "tests", StateKind::Exited, "52m"),
    ]
}

fn ids(v: &ViewState) -> Vec<&str> {
    v.panes().iter().map(String::as_str).collect()
}

#[test]
fn next_prev_cycle_and_skip_other_pane() {
    let mut v = ViewState::new(four(), "local");
    assert_eq!(ids(&v), ["a"]);
    v.apply(Cmd::NextSession);
    assert_eq!(ids(&v), ["b"]);
    v.apply(Cmd::PrevSession);
    v.apply(Cmd::PrevSession);
    assert_eq!(ids(&v), ["d"]);
    v.apply(Cmd::NextSession);
    assert_eq!(ids(&v), ["a"]);

    v.apply(Cmd::Split);
    assert_eq!(ids(&v), ["a", "b"]);
    assert_eq!(v.focus(), 1);
    v.apply(Cmd::NextSession);
    assert_eq!(ids(&v), ["a", "c"]);
    v.apply(Cmd::FocusOther);
    v.apply(Cmd::PrevSession);
    assert_eq!(ids(&v), ["d", "c"]);
    v.apply(Cmd::NextSession);
    assert_eq!(ids(&v), ["a", "c"]);
    v.apply(Cmd::NextSession);
    assert_eq!(ids(&v), ["b", "c"]);
}

#[test]
fn split_toggles_and_keeps_focused_session() {
    let mut v = ViewState::new(four(), "");
    v.apply(Cmd::Split);
    assert_eq!(v.focused_session(), Some("b"));
    v.apply(Cmd::Split);
    assert_eq!(ids(&v), ["b"]);
    assert_eq!(v.focus(), 0);
}

#[test]
fn split_with_one_session_sets_notice() {
    let mut v = ViewState::new(vec![row("a", "x", StateKind::Working, "1m")], "");
    v.apply(Cmd::Split);
    assert_eq!(ids(&v), ["a"]);
    assert!(v.notice.is_some());
}

#[test]
fn next_with_two_sessions_in_two_panes_does_nothing() {
    let rows = four()[..2].to_vec();
    let mut v = ViewState::new(rows, "");
    v.apply(Cmd::Split);
    v.apply(Cmd::NextSession);
    assert_eq!(ids(&v), ["a", "b"]);
}

#[test]
fn toggles() {
    let mut v = ViewState::new(four(), "");
    v.width = 160;
    v.apply(Cmd::Zoom);
    assert!(v.zoom);
    v.apply(Cmd::Help);
    assert!(v.help);
    v.apply(Cmd::Sidebar);
    assert_eq!(v.sidebar, Some(false));
    v.width = 80;
    v.sidebar = None;
    v.apply(Cmd::Sidebar);
    assert_eq!(v.sidebar, Some(true));
    v.apply(Cmd::ScrollMode);
    assert!(v.scroll.is_some());
    v.apply(Cmd::NextSession);
    assert!(v.scroll.is_none());
}

#[test]
fn scroll_keys_clamp() {
    let mut v = ViewState::new(four(), "");
    v.apply(Cmd::ScrollMode);
    v.scroll.as_mut().unwrap().max = 30;
    v.scroll_key(ScrollKey::PageUp, 20);
    v.scroll_key(ScrollKey::PageUp, 20);
    assert_eq!(v.scroll.unwrap().offset, 30);
    v.scroll_key(ScrollKey::Down, 20);
    assert_eq!(v.scroll.unwrap().offset, 29);
    v.scroll_key(ScrollKey::Bottom, 20);
    assert_eq!(v.scroll.unwrap().offset, 0);
    v.scroll_key(ScrollKey::Exit, 20);
    assert!(v.scroll.is_none());
}

#[test]
fn vanished_sessions_repair_panes() {
    let mut v = ViewState::new(four(), "");
    v.apply(Cmd::Split);
    v.sessions_changed(AREA, four().into_iter().filter(|r| r.id != "b").collect());
    assert_eq!(ids(&v), ["a"]);
    assert_eq!(v.focus(), 0);

    v.sessions_changed(
        AREA,
        four()
            .into_iter()
            .filter(|r| r.id == "c" || r.id == "d")
            .collect(),
    );
    assert_eq!(ids(&v), ["c"]);

    v.sessions_changed(AREA, vec![]);
    assert!(v.panes().is_empty());
    assert_eq!(v.focused_session(), None);
    v.apply(Cmd::NextSession);
    v.apply(Cmd::Split);
}

#[test]
fn prefs_round_trip_ignores_unknown_ids() {
    let mut v = ViewState::new(four(), "");
    v.apply(Cmd::Split);
    v.apply(Cmd::Zoom);
    v.sidebar = Some(false);
    let json = serde_json::to_string(&v.prefs()).unwrap();
    let prefs: LayoutPrefs = serde_json::from_str(&json).unwrap();

    let mut fresh = ViewState::new(four(), "");
    fresh.apply_prefs(&prefs);
    assert_eq!(ids(&fresh), ["a", "b"]);
    assert_eq!(
        (fresh.focus(), fresh.zoom, fresh.sidebar),
        (1, true, Some(false))
    );

    let mut gone = ViewState::new(four(), "");
    gone.apply_prefs(&LayoutPrefs {
        panes: vec!["zzz".into(), "d".into(), "d".into(), "qqq".into()],
        focus: 5,
        ..Default::default()
    });
    assert_eq!(ids(&gone), ["d"]);
    assert_eq!(gone.focus(), 0);

    gone.apply_prefs(&LayoutPrefs {
        panes: vec!["zzz".into()],
        ..Default::default()
    });
    assert_eq!(ids(&gone), ["d"]);
}

fn two_pane(width: u16) -> ViewState {
    let mut v = ViewState::new(four(), "local");
    v.width = width;
    v.apply(Cmd::Split);
    v
}

fn r(x: u16, y: u16, w: u16, h: u16) -> Rect {
    Rect::new(x, y, w, h)
}

#[test]
fn layout_wide_sidebar_and_two_panes_side_by_side() {
    let v = two_pane(160);
    let l = layout(r(0, 0, 160, 45), &v);
    assert_eq!(l.sidebar, Some(r(0, 0, 24, 44)));
    assert_eq!(l.status, r(0, 44, 160, 1));
    assert_eq!(l.panes.len(), 2);
    assert_eq!(l.panes[0].outer, r(24, 0, 68, 44));
    assert_eq!(l.panes[1].outer, r(92, 0, 68, 44));
    assert_eq!(l.panes[0].inner, r(25, 1, 66, 42));
    assert_eq!(l.panes[0].session, "a");
}

#[test]
fn layout_wide_enough_for_side_by_side_without_sidebar() {
    let mut v = two_pane(110);
    v.sidebar = Some(false);
    let l = layout(r(0, 0, 110, 30), &v);
    assert_eq!(l.sidebar, None);
    assert_eq!(l.panes[0].outer, r(0, 0, 55, 29));
    assert_eq!(l.panes[1].outer, r(55, 0, 55, 29));
}

#[test]
fn layout_narrow_has_no_sidebar_and_stacks_panes() {
    let v = two_pane(90);
    let l = layout(r(0, 0, 90, 30), &v);
    assert_eq!(l.sidebar, None);
    assert_eq!(l.panes[0].outer, r(0, 0, 90, 14));
    assert_eq!(l.panes[1].outer, r(0, 14, 90, 15));
}

#[test]
fn layout_wide_with_sidebar_keeps_side_by_side_above_row_minimum() {
    let v = two_pane(110);
    let l = layout(r(0, 0, 110, 30), &v);
    assert_eq!(l.sidebar, Some(r(0, 0, 24, 29)));
    assert_eq!(l.panes[0].outer, r(24, 0, 43, 29));
    assert_eq!(l.panes[1].outer, r(67, 0, 43, 29));
}

#[test]
fn layout_zoom_shows_only_focused_pane_without_sidebar() {
    let mut v = two_pane(160);
    v.apply(Cmd::Zoom);
    let l = layout(r(0, 0, 160, 45), &v);
    assert_eq!(l.sidebar, None);
    assert_eq!(l.panes.len(), 1);
    assert_eq!(l.panes[0].session, "b");
    assert_eq!(l.panes[0].outer, r(0, 0, 160, 44));
}

#[test]
fn layout_tiny_terminals_keep_inner_at_least_one_cell() {
    for (w, h) in [(10, 5), (1, 1), (0, 0), (3, 2), (30, 3), (100, 1)] {
        for zoom in [false, true] {
            for sidebar in [None, Some(true), Some(false)] {
                let mut v = two_pane(w);
                v.zoom = zoom;
                v.sidebar = sidebar;
                let l = layout(r(0, 0, w, h), &v);
                for p in &l.panes {
                    assert!(p.inner.width >= 1 && p.inner.height >= 1, "{w}x{h} {p:?}");
                }
                let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
                term.draw(|f| render(f, &v, &Screens::default())).unwrap();
            }
        }
    }
}

#[derive(Default)]
struct Screens(HashMap<String, Snapshot>);

impl ScreenSource for Screens {
    fn snapshot(&self, session: &str) -> Option<&Snapshot> {
        self.0.get(session)
    }
}

fn attr(fg: Color, flags: u8) -> Attr {
    Attr {
        fg,
        bg: Color::Default,
        flags,
    }
}

fn snap(rows: u16, cols: u16, lines: &[Vec<(&str, Attr)>], cursor: Option<(u16, u16)>) -> Snapshot {
    Snapshot {
        rows,
        cols,
        lines: lines
            .iter()
            .map(|l| Line {
                spans: l
                    .iter()
                    .map(|(t, a)| Span {
                        a: *a,
                        t: (*t).into(),
                    })
                    .collect(),
            })
            .collect(),
        cursor,
        modes: Modes {
            app_cursor: false,
            bracketed_paste: false,
            alt_screen: false,
            hide_cursor: false,
            mouse: Default::default(),
        },
        scroll_offset: 0,
        scroll_max: 0,
    }
}

fn plain() -> Attr {
    attr(Color::Default, 0)
}

fn screens() -> Screens {
    let mut s = Screens::default();
    s.0.insert(
        "a".into(),
        snap(
            10,
            40,
            &[
                vec![("● ", attr(Color::Idx(3), 0)), ("Reading src/checkout/Form.vue", plain())],
                vec![("I will add an Apple Pay button next to the card form, shown only when the session exists.", plain())],
                vec![("✓ Tests 14 passed", attr(Color::Rgb(0, 200, 0), BOLD))],
                vec![("> ", plain())],
            ],
            Some((3, 2)),
        ),
    );
    s.0.insert(
        "b".into(),
        snap(
            4,
            30,
            &[
                vec![("$ pnpm dev", plain())],
                vec![("  VITE ready in 412 ms", attr(Color::Idx(2), 0))],
            ],
            None,
        ),
    );
    s
}

fn text(term: &Terminal<TestBackend>) -> String {
    let buf = term.backend().buffer();
    let w = buf.area.width as usize;
    buf.content()
        .chunks(w)
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

fn draw(v: &ViewState, s: &Screens, w: u16, h: u16) -> String {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| render(f, v, s)).unwrap();
    text(&term)
}

fn assert_snapshot(name: &str, actual: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src/attach/snapshots")
        .join(format!("{name}.txt"));
    if std::env::var_os("TMGR_UPDATE_SNAPSHOTS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("{actual}\n")).unwrap();
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_default();
    assert_eq!(expected.trim_end_matches('\n'), actual, "snapshot {name}");
}

#[test]
fn snapshot_wide_two_panes() {
    let v = two_pane(140);
    assert_snapshot("wide", &draw(&v, &screens(), 140, 22));
}

#[test]
fn snapshot_narrow() {
    let v = two_pane(90);
    assert_snapshot("narrow", &draw(&v, &screens(), 90, 16));
}

#[test]
fn snapshot_zoom() {
    let mut v = two_pane(140);
    v.apply(Cmd::Zoom);
    assert_snapshot("zoom", &draw(&v, &screens(), 140, 10));
}

#[test]
fn snapshot_help_overlay() {
    let mut v = two_pane(140);
    v.apply(Cmd::Help);
    assert_snapshot("help", &draw(&v, &screens(), 140, 31));
}

#[test]
fn snapshot_blocked_and_connecting() {
    let mut v = ViewState::new(four(), "spark");
    v.apply(Cmd::NextSession);
    v.apply(Cmd::NextSession);
    v.notice = Some("only one session, nothing to split".into());
    assert_snapshot("blocked", &draw(&v, &Screens::default(), 120, 12));
}

#[test]
fn scroll_mode_title_shows_offset() {
    let mut v = ViewState::new(four(), "");
    v.apply(Cmd::ScrollMode);
    *v.scroll.as_mut().unwrap() = ScrollState {
        offset: 12,
        max: 340,
    };
    assert!(draw(&v, &screens(), 120, 10).contains("scroll -12/340"));
}

fn term_for(v: &ViewState, s: &Screens, w: u16, h: u16) -> Terminal<TestBackend> {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| render(f, v, s)).unwrap();
    term
}

#[test]
fn colours_and_modifiers_are_mapped() {
    use ratatui::style::{Color as C, Modifier};
    let v = ViewState::new(four(), "");
    let term = term_for(&v, &screens(), 140, 22);
    let l = layout(r(0, 0, 140, 22), &v);
    let inner = l.panes[0].inner;
    let buf = term.backend().buffer();
    assert_eq!(buf[(inner.x, inner.y)].fg, C::Indexed(3));
    assert_eq!(buf[(inner.x + 2, inner.y)].fg, C::Reset);
    let tests = &buf[(inner.x, inner.y + 2)];
    assert_eq!(tests.fg, C::Rgb(0, 200, 0));
    assert!(tests.modifier.contains(Modifier::BOLD));
}

#[test]
fn blocked_pane_gets_accent_border_and_sidebar_lists_it_first() {
    use ratatui::style::Color as C;
    let mut v = ViewState::new(four(), "");
    v.apply(Cmd::Split);
    v.apply(Cmd::NextSession);
    assert_eq!(ids(&v), ["a", "c"]);
    let term = term_for(&v, &screens(), 140, 22);
    let l = layout(r(0, 0, 140, 22), &v);
    let buf = term.backend().buffer();
    let blocked = l.panes[1].outer;
    assert_eq!(buf[(blocked.x, blocked.y + 3)].fg, C::LightRed);
    let calm = l.panes[0].outer;
    assert_eq!(buf[(calm.x, calm.y + 3)].fg, C::DarkGray);
    let rows = text(&term);
    let side: Vec<&str> = rows.lines().take(4).collect();
    assert!(side[1].contains("▾ claude"), "{side:?}");
    assert!(side[3].contains("◆ codex"), "{side:?}");
}

#[test]
fn adversarial_snapshot_text_never_reaches_the_buffer() {
    let evil = "\x1b]52;c;AAAA\x07\u{9b}31m\u{202e}rtl \x01\u{301}x \u{2066}\u{feff}";
    let mut s = Screens::default();
    for id in ["a", "b"] {
        s.0.insert(
            id.into(),
            snap(
                5,
                60,
                &[
                    vec![(evil, plain())],
                    vec![(evil, attr(Color::Idx(1), BOLD))],
                ],
                Some((0, 0)),
            ),
        );
    }
    let mut v = two_pane(140);
    v.sessions[0].name = "na\x1b[31mme\u{202e}".into();
    v.notice = Some("n\x1b]52;c;x\x07".into());
    for (w, h) in [(140, 22), (60, 12)] {
        for help in [false, true] {
            v.help = help;
            let term = term_for(&v, &s, w, h);
            for cell in term.backend().buffer().content() {
                for c in cell.symbol().chars() {
                    assert!(
                        c != '\u{1b}'
                            && !('\u{80}'..='\u{9f}').contains(&c)
                            && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}' | '\u{feff}')
                            && !c.is_control(),
                        "bad char {c:?} in {:?}",
                        cell.symbol()
                    );
                }
            }
            assert!(text(&term).contains("AAAA") || help);
        }
    }
}

#[test]
fn cursor_maps_into_focused_pane_inner_rect() {
    let mut v = two_pane(140);
    v.apply(Cmd::FocusOther);
    let s = screens();
    let l = layout(r(0, 0, 140, 22), &v);
    assert_eq!(v.focused_session(), Some("a"));
    let inner = l.panes[0].inner;
    assert_eq!(cursor(&v, &l, &s), Some((inner.x + 2, inner.y + 3)));

    v.apply(Cmd::FocusOther);
    assert_eq!(cursor(&v, &l, &s), None);
}

#[test]
fn cursor_is_none_when_scrolled_hidden_clipped_or_missing() {
    let s = screens();
    let mut v = ViewState::new(four(), "");
    let l = layout(r(0, 0, 140, 22), &v);
    assert!(cursor(&v, &l, &s).is_some());
    assert_eq!(cursor(&v, &l, &Screens::default()), None);
    let small = layout(r(0, 0, 140, 5), &v);
    assert_eq!(cursor(&v, &small, &s), None);
    v.apply(Cmd::ScrollMode);
    assert_eq!(cursor(&v, &l, &s), None);
}

#[test]
fn mode_transitions_emit_only_the_four_sequences() {
    let off = Modes {
        app_cursor: false,
        bracketed_paste: false,
        alt_screen: true,
        hide_cursor: true,
        mouse: Default::default(),
    };
    let on = Modes {
        app_cursor: true,
        bracketed_paste: true,
        ..off
    };
    assert_eq!(mode_changes(Some(off), off), b"");
    assert_eq!(mode_changes(Some(off), on), b"\x1b[?1h\x1b[?2004h");
    assert_eq!(mode_changes(Some(on), off), b"\x1b[?1l\x1b[?2004l");
    assert_eq!(
        mode_changes(
            Some(off),
            Modes {
                app_cursor: true,
                ..off
            }
        ),
        b"\x1b[?1h"
    );
    assert_eq!(
        mode_changes(
            Some(on),
            Modes {
                bracketed_paste: false,
                ..on
            }
        ),
        b"\x1b[?2004l"
    );
    assert_eq!(mode_changes(None, off), b"\x1b[?1l\x1b[?2004l");
    assert_eq!(mode_changes(None, on), b"\x1b[?1h\x1b[?2004h");
    assert_eq!(
        reset_modes(),
        b"\x1b[?1l\x1b[?2004l\x1b[?1004l\x1b[?1006l\x1b[?1003l\x1b[?1002l\x1b[?1000l".as_slice()
    );
}

fn sgr(cb: u16, col: u16, row: u16, release: bool) -> Vec<u8> {
    format!(
        "\x1b[<{};{};{}{}",
        cb,
        col + 1,
        row + 1,
        if release { 'm' } else { 'M' }
    )
    .into_bytes()
}

fn ev(cb: u16, col: u16, row: u16, release: bool) -> MouseEvent {
    MouseEvent {
        cb,
        col,
        row,
        release,
    }
}

fn mouse_info(mode: MouseMode, scroll_max: usize) -> impl Fn(&str) -> PaneInfo {
    move |_| PaneInfo {
        mouse: mode,
        scroll_max,
        alt_screen: false,
        app_cursor: false,
    }
}

const AREA: Rect = Rect {
    x: 0,
    y: 0,
    width: 140,
    height: 40,
};

fn sgr_mode(level: MouseLevel) -> MouseMode {
    MouseMode {
        level,
        encoding: MouseEncoding::Sgr,
    }
}

#[test]
fn sgr_mouse_is_parsed_even_when_split_across_reads() {
    let mut s = InputSplitter::new();
    let bytes = sgr(0, 9, 4, false);
    assert_eq!(s.feed(&bytes), vec![Action::Mouse(ev(0, 9, 4, false))]);
    assert_eq!(s.feed(&bytes[..5]), vec![]);
    assert_eq!(s.feed(&bytes[5..]), vec![Action::Mouse(ev(0, 9, 4, false))]);
    assert_eq!(
        s.feed(b"a\x1b[<0;1;1mb"),
        vec![
            forward(b"a"),
            Action::Mouse(ev(0, 0, 0, true)),
            forward(b"b")
        ]
    );
}

#[test]
fn stale_held_escape_is_flushed_to_the_agent() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"\x1b[<0;1"), vec![]);
    let later = std::time::Instant::now() + std::time::Duration::from_millis(50);
    assert_eq!(s.flush_stale(later), vec![forward(b"\x1b[<0;1")]);
}

#[test]
fn paste_is_one_action_and_ignores_prefix_and_mouse_look_alikes() {
    let mut s = InputSplitter::new();
    let data = b"\x1b[200~/tmp/a b\x1d\x1b[<0;1;1M.png\x1b[201~".to_vec();
    assert_eq!(
        s.feed(&data),
        vec![Action::Paste {
            data: data.clone(),
            start: true,
            end: true
        }]
    );
}

#[test]
fn paste_split_across_reads_keeps_one_target() {
    let mut s = InputSplitter::new();
    let first = s.feed(b"\x1b[200~/tmp/a");
    let second = s.feed(b" b.png\x1b[20");
    let third = s.feed(b"1~");
    let all: Vec<u8> = [first, second, third]
        .into_iter()
        .flatten()
        .flat_map(|a| match a {
            Action::Paste { data, .. } => data,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(all, b"\x1b[200~/tmp/a b.png\x1b[201~");
}

#[test]
fn bare_dropped_path_is_a_drop() {
    let mut s = InputSplitter::new();
    assert_eq!(
        s.feed(b"/tmp/my\\ file.png "),
        vec![Action::Drop(b"/tmp/my\\ file.png ".to_vec())]
    );
    assert_eq!(s.feed(b"/ls"), vec![forward(b"/ls")]);
}

#[test]
fn click_on_sidebar_row_switches_session() {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    let out = v.on_mouse(
        AREA,
        ev(0, 3, 1, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert!(out.redraw);
    assert_eq!(ids(&v), vec!["c"]);
    v.on_mouse(
        AREA,
        ev(0, 3, 3, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert_eq!(ids(&v), vec!["b"]);
    v.on_mouse(
        AREA,
        ev(0, 3, 0, false),
        &mouse_info(MouseMode::default(), 0),
    );
    v.on_mouse(
        AREA,
        ev(0, 3, 30, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert_eq!(ids(&v), vec!["b"]);
}

#[test]
fn hidden_sidebar_and_narrow_layout_have_no_sidebar_hits() {
    let mut v = ViewState::new(four(), "local");
    v.width = 90;
    let area = Rect::new(0, 0, 90, 30);
    assert!(matches!(
        v.hit(area, 3, 1),
        Hit::Pane {
            idx: 0,
            at: Some(_)
        }
    ));
    v.width = 140;
    v.apply(Cmd::Sidebar);
    assert!(matches!(
        v.hit(AREA, 3, 5),
        Hit::Pane {
            idx: 0,
            at: Some(_)
        }
    ));
}

#[test]
fn click_on_other_pane_moves_focus_in_wide_and_narrow_split() {
    let mut v = two_pane(140);
    assert_eq!(v.focus(), 1);
    v.on_mouse(
        AREA,
        ev(0, 40, 5, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert_eq!(v.focus(), 0);
    let mut n = two_pane(90);
    let area = Rect::new(0, 0, 90, 40);
    n.on_mouse(
        area,
        ev(0, 40, 5, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert_eq!(n.focus(), 0);
    n.on_mouse(
        area,
        ev(0, 40, 30, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert_eq!(n.focus(), 1);
}

#[test]
fn zoom_hit_only_sees_the_focused_pane() {
    let mut v = two_pane(140);
    v.apply(Cmd::Zoom);
    assert!(matches!(v.hit(AREA, 40, 5), Hit::Pane { idx: 1, .. }));
}

#[test]
fn clicks_are_forwarded_with_pane_relative_coordinates() {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    v.apply(Cmd::Sidebar);
    let out = v.on_mouse(
        AREA,
        ev(0, 10, 7, false),
        &mouse_info(sgr_mode(MouseLevel::Press), 0),
    );
    assert_eq!(out.forward, Some(("a".into(), b"\x1b[<0;10;7M".to_vec())));
    let out = v.on_mouse(
        AREA,
        ev(0, 12, 9, true),
        &mouse_info(sgr_mode(MouseLevel::Press), 0),
    );
    assert_eq!(out.forward, Some(("a".into(), b"\x1b[<0;12;9m".to_vec())));
}

#[test]
fn clicks_are_not_forwarded_when_the_agent_did_not_ask() {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    let out = v.on_mouse(
        AREA,
        ev(0, 40, 7, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert_eq!(out.forward, None);
}

#[test]
fn motion_needs_any_motion_and_drag_follows_the_grab() {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    v.apply(Cmd::Sidebar);
    let press = sgr_mode(MouseLevel::Drag);
    assert_eq!(
        v.on_mouse(AREA, ev(35, 5, 5, false), &mouse_info(press, 0))
            .forward,
        None
    );
    v.on_mouse(AREA, ev(0, 5, 5, false), &mouse_info(press, 0));
    let out = v.on_mouse(AREA, ev(32, 400, 400, false), &mouse_info(press, 0));
    assert_eq!(out.forward.map(|f| f.0), Some("a".to_string()));
    let out = v.on_mouse(AREA, ev(0, 400, 400, true), &mouse_info(press, 0));
    assert!(out.forward.is_some());
    assert_eq!(v.grab, None);
}

#[test]
fn x10_and_utf8_encodings_translate_coordinates() {
    let e = ev(0, 0, 0, false);
    assert_eq!(encode(&e, 2, 3, MouseEncoding::Default), b"\x1b[M #$");
    assert_eq!(encode(&e, 300, 3, MouseEncoding::Default), b"");
    assert_eq!(
        encode(&e, 300, 3, MouseEncoding::Utf8),
        "\x1b[M \u{14d}$".as_bytes()
    );
}

#[test]
fn wheel_scrolls_locally_and_returns_to_live() {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    let info = mouse_info(MouseMode::default(), 10);
    v.on_mouse(AREA, ev(64, 40, 5, false), &info);
    assert_eq!(v.scroll.map(|s| s.offset), Some(3));
    v.on_mouse(AREA, ev(64, 40, 5, false), &info);
    assert_eq!(v.scroll.map(|s| s.offset), Some(6));
    v.on_mouse(AREA, ev(65, 40, 5, false), &info);
    v.on_mouse(AREA, ev(65, 40, 5, false), &info);
    assert_eq!(v.scroll, None);
}

#[test]
fn wheel_does_nothing_without_history_and_forwards_to_mouse_aware_agents() {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    v.on_mouse(
        AREA,
        ev(64, 40, 5, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert_eq!(v.scroll, None);
    let out = v.on_mouse(
        AREA,
        ev(64, 40, 5, false),
        &mouse_info(sgr_mode(MouseLevel::Press), 10),
    );
    assert!(out.forward.is_some());
    assert_eq!(v.scroll, None);
}

#[test]
fn wheel_on_alt_screen_without_mouse_sends_arrows() {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    let info = |_: &str| PaneInfo {
        mouse: MouseMode::default(),
        scroll_max: 0,
        alt_screen: true,
        app_cursor: true,
    };
    let out = v.on_mouse(AREA, ev(65, 40, 5, false), &info);
    assert_eq!(
        out.forward,
        Some(("a".into(), b"\x1bOB\x1bOB\x1bOB".to_vec()))
    );
}

#[test]
fn wheel_over_other_pane_scrolls_it_and_moves_focus() {
    let mut v = two_pane(140);
    v.on_mouse(
        AREA,
        ev(64, 40, 5, false),
        &mouse_info(MouseMode::default(), 10),
    );
    assert_eq!(v.focus(), 0);
    assert!(v.scroll.is_some());
}

#[test]
fn paste_target_follows_the_pointer_and_falls_back_to_focus() {
    let mut v = two_pane(140);
    assert_eq!(v.paste_target(AREA), 1);
    v.on_mouse(
        AREA,
        ev(35, 40, 5, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert_eq!(v.paste_target(AREA), 0);
    v.on_mouse(
        AREA,
        ev(35, 3, 1, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert_eq!(v.paste_target(AREA), v.focus());
}

#[test]
fn stop_asks_only_for_running_sessions() {
    let mut v = ViewState::new(four(), "local");
    v.apply(Cmd::Stop);
    assert_eq!(v.confirm.as_ref().map(|c| c.session.as_str()), Some("a"));
    let mut e = ViewState::new(four(), "local");
    e.select("d");
    e.apply(Cmd::Stop);
    assert_eq!(e.confirm, None);
    assert!(e.notice.is_some());
}

#[test]
fn remove_needs_an_exited_session_and_focus_moves_to_a_neighbour() {
    let mut v = ViewState::new(four(), "local");
    v.apply(Cmd::Remove);
    assert_eq!(v.sessions.len(), 4);
    assert!(v.notice.is_some());
    v.select("d");
    v.apply(Cmd::Remove);
    assert_eq!(v.sessions.len(), 3);
    assert_eq!(ids(&v), vec!["c"]);
}

#[test]
fn removing_the_last_session_empties_the_panes() {
    let mut v = ViewState::new(vec![row("x", "one", StateKind::Exited, "1m")], "local");
    v.apply(Cmd::Remove);
    assert!(v.sessions.is_empty());
    assert!(v.panes().is_empty());
}

#[test]
fn mouse_mode_bytes_are_enabled_and_always_reset() {
    assert_eq!(
        enter_mouse(),
        b"\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h"
    );
    assert!(reset_modes().ends_with(b"\x1b[?1006l\x1b[?1003l\x1b[?1002l\x1b[?1000l"));
}

#[test]
fn rename_prompt_edits_saves_and_cancels() {
    let mut v = ViewState::new(four(), "local");
    v.apply(Cmd::Rename);
    assert_eq!(v.rename.as_ref().map(|r| r.text.as_str()), Some("claude"));
    assert_eq!(v.edit_rename(b"\x7f\x7f"), RenameEdit::Editing);
    assert_eq!(v.edit_rename("é!".as_bytes()), RenameEdit::Editing);
    assert_eq!(
        v.edit_rename(b"\r"),
        RenameEdit::Submit("a".into(), "claué!".into())
    );
    assert_eq!(v.rename, None);
    v.apply(Cmd::Rename);
    assert_eq!(v.edit_rename(b"\x1b"), RenameEdit::Cancel);
    assert_eq!(v.rename, None);
}

#[test]
fn rename_done_updates_the_sidebar_name() {
    let mut v = ViewState::new(four(), "local");
    v.rename_done("b", "web");
    assert_eq!(v.sessions[1].name, "web");
}

#[test]
fn a_late_focus_report_final_byte_is_not_split_from_its_prefix() {
    let ms = |n| std::time::Instant::now() + std::time::Duration::from_millis(n);
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"\x1b["), vec![]);
    assert_eq!(s.flush_stale(ms(50)), vec![]);
    assert_eq!(s.feed(b"I"), vec![Action::FocusIn]);
}

#[test]
fn a_held_csi_prefix_is_forwarded_after_the_longer_wait() {
    let ms = |n| std::time::Instant::now() + std::time::Duration::from_millis(n);
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"\x1b["), vec![]);
    assert_eq!(s.flush_stale(ms(50)), vec![]);
    assert_eq!(s.flush_stale(ms(2000)), vec![forward(b"\x1b[")]);
}

#[test]
fn lone_escape_is_forwarded_after_the_hold_time() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"\x1b"), vec![]);
    let later = std::time::Instant::now() + std::time::Duration::from_millis(50);
    assert_eq!(s.flush_stale(later), vec![forward(b"\x1b")]);
}

#[test]
fn mouse_split_after_the_escape_is_not_typed() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"\x1b"), vec![]);
    assert_eq!(s.feed(b"[<0;1;1M"), vec![Action::Mouse(ev(0, 0, 0, false))]);
    assert_eq!(s.feed(b"\x1b["), vec![]);
    assert_eq!(s.feed(b"A"), vec![forward(b"\x1b[A")]);
}

#[test]
fn mouse_movement_between_prefix_and_command_keeps_the_prefix() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(&[0x1d]), vec![]);
    assert_eq!(
        s.feed(b"\x1b[<35;10;5M"),
        vec![Action::Mouse(ev(35, 9, 4, false))]
    );
    assert_eq!(s.feed(b"d"), vec![Action::Cmd(Cmd::Detach)]);
}

#[test]
fn partial_paste_end_is_never_flushed_by_the_timer() {
    let mut s = InputSplitter::new();
    s.feed(b"\x1b[200~abc\x1b[20");
    let later = std::time::Instant::now() + std::time::Duration::from_millis(50);
    assert_eq!(s.flush_stale(later), vec![]);
    assert_eq!(
        s.feed(b"1~"),
        vec![Action::Paste {
            data: b"\x1b[201~".to_vec(),
            start: false,
            end: true
        }]
    );
}

#[test]
fn paste_in_scroll_mode_is_a_paste() {
    let mut s = InputSplitter::new();
    s.set_scroll_mode(true);
    let got = s.feed(b"\x1b[200~q\x1b[201~");
    assert!(matches!(got.as_slice(), [Action::Paste { .. }]));
    assert_eq!(s.feed(b"x"), vec![forward(b"x")]);
}

#[test]
fn header_buttons_are_hit_tested_on_the_top_border() {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    v.apply(Cmd::Sidebar);
    let lay = layout(AREA, &v);
    let outer = lay.panes[0].outer;
    let x = |c| header_button(outer, c).unwrap();
    assert_eq!(
        v.hit(AREA, x(Control::Interrupt), 0),
        Hit::Button("a".into(), Control::Interrupt)
    );
    assert_eq!(
        v.hit(AREA, x(Control::Stop), 0),
        Hit::Button("a".into(), Control::Stop)
    );
    assert_eq!(
        v.hit(AREA, x(Control::Close), 0),
        Hit::Button("a".into(), Control::Close)
    );
    assert!(!matches!(
        v.hit(AREA, x(Control::Close), 1),
        Hit::Button(..)
    ));
    let out = v.on_mouse(
        AREA,
        ev(0, x(Control::Stop), 0, false),
        &mouse_info(sgr_mode(MouseLevel::Any), 0),
    );
    assert_eq!(out.control, Some(("a".into(), Control::Stop)));
    assert_eq!(out.forward, None);
}

#[test]
fn header_buttons_hide_in_a_narrow_pane() {
    let mut v = two_pane(90);
    let area = Rect::new(0, 0, 90, 40);
    let lay = layout(area, &v);
    assert!(lay.panes[0].outer.width >= 40);
    let mut w = ViewState::new(four(), "local");
    w.width = 140;
    let narrow = Rect::new(0, 0, 38, 20);
    let lay = layout(narrow, &w);
    assert_eq!(header_button(lay.panes[0].outer, Control::Close), None);
    v.apply(Cmd::Split);
    assert!(!matches!(w.hit(narrow, 30, 0), Hit::Button(..)));
}

#[test]
fn sidebar_icons_are_hit_tested_and_the_row_still_switches() {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    let lay = layout(AREA, &v);
    let sb = lay.sidebar.unwrap();
    let stop = sidebar_button(sb, Control::Stop).unwrap();
    let close = sidebar_button(sb, Control::Close).unwrap();
    assert_eq!(v.hit(AREA, stop, 2), Hit::Button("a".into(), Control::Stop));
    assert_eq!(
        v.hit(AREA, close, 2),
        Hit::Button("a".into(), Control::Close)
    );
    assert_eq!(v.hit(AREA, 3, 2), Hit::Session("a".into()));
    assert_eq!(v.hit(AREA, stop, 0), Hit::Filter);
}

#[test]
fn only_a_busy_session_can_be_interrupted_and_agents_use_escape() {
    let mut r = row("x", "x", StateKind::Working, "1s");
    assert!(r.can_interrupt());
    assert_eq!(r.interrupt_bytes(), b"\x03");
    r.esc = true;
    assert_eq!(r.interrupt_bytes(), b"\x1b");
    for kind in [StateKind::Waiting, StateKind::Exited, StateKind::Starting] {
        assert!(!row("x", "x", kind, "1s").can_interrupt());
    }
    assert!(!row("x", "x", StateKind::Blocked, "1s").can_interrupt());
}

#[test]
fn buttons_are_drawn_in_the_border_and_hovered_sidebar_row_shows_icons() {
    let mut v = two_pane(140);
    let plain = draw(&v, &screens(), 140, 22);
    assert!(plain.lines().next().unwrap().contains("⏸ ■ ✕"));
    v.pointer = Some((3, 3));
    let hovered = draw(&v, &screens(), 140, 22);
    let line = hovered.lines().nth(3).unwrap();
    assert!(line.contains("■ ✕"), "{line}");
    assert!(!line.contains("18m"), "{line}");
}

#[test]
fn hover_changes_ask_for_a_redraw_and_a_stray_click_drops_the_confirm() {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    let info = mouse_info(MouseMode::default(), 0);
    assert!(v.on_mouse(AREA, ev(35, 3, 2, false), &info).redraw);
    assert!(!v.on_mouse(AREA, ev(35, 3, 2, false), &info).redraw);
    assert!(v.on_mouse(AREA, ev(35, 40, 10, false), &info).redraw);
    v.confirm = Some(Confirm {
        session: "a".into(),
        name: "claude".into(),
        remove: false,
    });
    v.on_mouse(AREA, ev(0, 40, 10, false), &info);
    assert_eq!(v.confirm, None);
}

#[test]
fn releasing_after_a_button_press_is_not_forwarded() {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    v.apply(Cmd::Sidebar);
    let outer = layout(AREA, &v).panes[0].outer;
    let x = header_button(outer, Control::Stop).unwrap();
    let info = mouse_info(sgr_mode(MouseLevel::Any), 0);
    v.on_mouse(AREA, ev(0, x, 0, false), &info);
    let out = v.on_mouse(AREA, ev(0, 40, 10, true), &info);
    assert_eq!(out.forward, None);
    assert_eq!(v.grab, None);
}

fn leaf(id: &str) -> Node {
    Node::leaf(id)
}

fn node(dir: SplitDir, ratio: u16, a: Node, b: Node) -> Node {
    Node::Split {
        dir,
        ratio,
        a: Box::new(a),
        b: Box::new(b),
    }
}

fn names(n: &Node) -> Vec<&str> {
    n.leaves()
}

fn grid(width: u16) -> ViewState {
    let rows: Vec<SessionRow> = ["a", "b", "c", "d", "e", "f"]
        .iter()
        .map(|id| row(id, id, StateKind::Working, "1m"))
        .collect();
    let mut v = ViewState::new(rows, "local");
    v.width = width;
    assert!(v.split_focused(SplitDir::Right, "b"));
    v.focus_pane(0);
    assert!(v.split_focused(SplitDir::Down, "c"));
    v.focus_pane(2);
    assert!(v.split_focused(SplitDir::Down, "d"));
    v
}

#[test]
fn tree_split_leaf_makes_the_new_leaf_the_second_child() {
    let mut t = leaf("a");
    assert!(t.split_leaf("a", SplitDir::Right, "b"));
    assert_eq!(t, node(SplitDir::Right, 500, leaf("a"), leaf("b")));
    assert!(t.split_leaf("b", SplitDir::Down, "c"));
    assert_eq!(names(&t), ["a", "b", "c"]);
    assert!(!t.split_leaf("a", SplitDir::Down, "c"));
    assert!(!t.split_leaf("zzz", SplitDir::Down, "q"));
    assert!(t.contains("c") && !t.contains("q"));
}

#[test]
fn tree_remove_lets_the_sibling_take_the_space() {
    let mut t = Some(node(
        SplitDir::Right,
        300,
        leaf("a"),
        node(SplitDir::Down, 500, leaf("b"), leaf("c")),
    ));
    assert!(Node::remove(&mut t, "b"));
    assert_eq!(t, Some(node(SplitDir::Right, 300, leaf("a"), leaf("c"))));
    assert!(!Node::remove(&mut t, "b"));
    assert!(Node::remove(&mut t, "a"));
    assert_eq!(t, Some(leaf("c")));
    assert!(Node::remove(&mut t, "c"));
    assert_eq!(t, None);
    assert!(!Node::remove(&mut t, "c"));
}

#[test]
fn tree_rename_refuses_an_id_already_present() {
    let mut t = node(SplitDir::Right, 500, leaf("a"), leaf("b"));
    assert!(!t.rename_leaf("a", "b"));
    assert!(!t.rename_leaf("x", "y"));
    assert!(t.rename_leaf("a", "z"));
    assert_eq!(names(&t), ["z", "b"]);
}

#[test]
fn tree_resize_moves_the_nearest_split_on_that_axis_and_clamps() {
    let mut t = node(
        SplitDir::Right,
        500,
        leaf("a"),
        node(SplitDir::Down, 500, leaf("b"), leaf("c")),
    );
    assert!(t.resize(AREA, "b", Nav::Right, 50));
    assert!(t.resize(AREA, "c", Nav::Down, 50));
    let Node::Split { ratio, b, .. } = &t else {
        panic!()
    };
    assert_eq!(*ratio, 550);
    let Node::Split { ratio: inner, .. } = &**b else {
        panic!()
    };
    assert_eq!(*inner, 550);
    assert!(!t.resize(AREA, "a", Nav::Up, 50));
    assert!(t.resize(AREA, "a", Nav::Left, 50));
    for _ in 0..40 {
        t.resize(AREA, "a", Nav::Left, 50);
    }
    let Node::Split { ratio, .. } = &t else {
        panic!()
    };
    assert_eq!(*ratio, 100);
    assert!(!t.resize(AREA, "a", Nav::Left, 50));
    for _ in 0..40 {
        t.resize(AREA, "a", Nav::Right, 50);
    }
    let Node::Split { ratio, .. } = &t else {
        panic!()
    };
    assert_eq!(*ratio, 900);
    assert!(!t.resize(AREA, "nope", Nav::Right, 50));
}

fn deep(depth: usize) -> Node {
    let mut t = leaf("n0");
    let mut last = "n0".to_string();
    for i in 1..=depth {
        let id = format!("n{i}");
        let dir = if i % 2 == 0 {
            SplitDir::Right
        } else {
            SplitDir::Down
        };
        assert!(t.split_leaf(&last, dir, &id));
        last = id;
    }
    t
}

#[test]
fn tree_layout_sums_exactly_for_odd_sizes_and_deep_trees() {
    for (w, h) in [(101, 77), (157, 63), (200, 201), (99, 99)] {
        let t = deep(5);
        let rects = t.layout(Rect::new(3, 2, w, h)).unwrap();
        assert_eq!(rects.len(), 6);
        let area: u32 = rects
            .iter()
            .map(|(_, r)| u32::from(r.width) * u32::from(r.height))
            .sum();
        assert_eq!(area, u32::from(w) * u32::from(h));
        for (i, (_, a)) in rects.iter().enumerate() {
            assert!(a.width >= 3 && a.height >= 3, "{a:?}");
            for (_, b) in &rects[i + 1..] {
                assert!(!a.intersects(*b), "{a:?} {b:?}");
            }
        }
    }
    let flat = node(SplitDir::Right, 500, leaf("a"), leaf("b"));
    let rects = flat.layout(Rect::new(0, 0, 101, 10)).unwrap();
    assert_eq!(rects[0].1.width + rects[1].1.width, 101);
}

#[test]
fn tree_layout_gives_up_when_the_minimum_does_not_fit() {
    let t = node(SplitDir::Down, 500, leaf("a"), leaf("b"));
    assert!(t.layout(Rect::new(0, 0, 80, 5)).is_none());
    assert!(t.layout(Rect::new(0, 0, 80, 6)).is_some());
    let skewed = node(SplitDir::Down, 100, leaf("a"), leaf("b"));
    let rects = skewed.layout(Rect::new(0, 0, 80, 20)).unwrap();
    assert_eq!(rects[0].1.height, 3);
    assert_eq!(leaf("x").layout(Rect::new(0, 0, 1, 1)).unwrap().len(), 1);
}

#[test]
fn tree_layout_stacks_a_row_split_narrower_than_fifty() {
    let t = node(SplitDir::Right, 500, leaf("a"), leaf("b"));
    let narrow = t.layout(Rect::new(0, 0, 49, 20)).unwrap();
    assert_eq!(narrow[1].1, Rect::new(0, 10, 49, 10));
    let wide = t.layout(Rect::new(0, 0, 50, 20)).unwrap();
    assert_eq!(wide[1].1, Rect::new(25, 0, 25, 20));
}

#[test]
fn view_falls_back_to_the_focused_pane_when_nothing_fits() {
    let v = grid(140);
    let l = layout(r(0, 0, 140, 5), &v);
    assert_eq!(l.panes.len(), 1);
    assert_eq!(l.panes[0].session, "d");
    assert_eq!(l.panes[0].outer, r(24, 0, 116, 4));
}

#[test]
fn neighbour_walks_a_two_by_two_grid() {
    let v = grid(140);
    assert_eq!(v.panes(), ["a", "c", "b", "d"]);
    let rects: Vec<(String, Rect)> = layout(AREA, &v)
        .panes
        .into_iter()
        .map(|p| (p.session, p.outer))
        .collect();
    let go = |from: &str, nav| neighbour(&rects, from, nav);
    assert_eq!(go("a", Nav::Right), Some("b".into()));
    assert_eq!(go("a", Nav::Down), Some("c".into()));
    assert_eq!(go("d", Nav::Left), Some("c".into()));
    assert_eq!(go("d", Nav::Up), Some("b".into()));
    assert_eq!(go("a", Nav::Left), None);
    assert_eq!(go("a", Nav::Up), None);
    assert_eq!(go("d", Nav::Right), None);
    assert_eq!(go("d", Nav::Down), None);
    assert_eq!(go("zzz", Nav::Down), None);
}

#[test]
fn neighbour_prefers_the_pane_with_most_overlap_on_uneven_layouts() {
    let t = node(
        SplitDir::Right,
        500,
        leaf("a"),
        node(SplitDir::Down, 200, leaf("b"), leaf("c")),
    );
    let rects = t.layout(Rect::new(0, 0, 100, 40)).unwrap();
    assert_eq!(neighbour(&rects, "a", Nav::Right), Some("c".into()));
    assert_eq!(neighbour(&rects, "b", Nav::Left), Some("a".into()));
    assert_eq!(neighbour(&rects, "c", Nav::Up), Some("b".into()));
    let even = node(
        SplitDir::Right,
        500,
        leaf("a"),
        node(SplitDir::Down, 500, leaf("b"), leaf("c")),
    );
    let rects = even.layout(Rect::new(0, 0, 100, 40)).unwrap();
    assert_eq!(neighbour(&rects, "a", Nav::Right), Some("b".into()));
}

#[test]
fn focus_nav_and_cycling_cover_every_pane() {
    let mut v = grid(140);
    assert_eq!(v.focused_session(), Some("d"));
    v.focus_nav(AREA, Nav::Up);
    assert_eq!(v.focused_session(), Some("b"));
    v.focus_nav(AREA, Nav::Left);
    assert_eq!(v.focused_session(), Some("a"));
    v.focus_nav(AREA, Nav::Left);
    assert_eq!(v.focused_session(), Some("a"));
    v.apply(Cmd::FocusOther);
    assert_eq!(v.focused_session(), Some("c"));
    v.apply(Cmd::FocusOther);
    v.apply(Cmd::FocusOther);
    v.apply(Cmd::FocusOther);
    assert_eq!(v.focused_session(), Some("a"));
    v.apply(Cmd::FocusPrev);
    assert_eq!(v.focused_session(), Some("d"));
}

#[test]
fn split_focused_focuses_the_new_pane_and_rejects_duplicates() {
    let mut v = ViewState::new(four(), "local");
    v.apply(Cmd::Zoom);
    v.apply(Cmd::ScrollMode);
    assert!(v.split_focused(SplitDir::Down, "b"));
    assert!(!v.zoom);
    assert!(v.scroll.is_none());
    assert_eq!(v.focused_session(), Some("b"));
    assert!(!v.split_focused(SplitDir::Down, "a"));
    assert_eq!(ids(&v), ["a", "b"]);
}

#[test]
fn close_pane_hands_focus_to_the_sibling() {
    let mut v = grid(140);
    v.close_pane("d");
    assert_eq!(v.panes(), ["a", "c", "b"]);
    assert_eq!(v.focused_session(), Some("b"));
    v.close_pane("a");
    assert_eq!(v.panes(), ["c", "b"]);
    assert_eq!(v.focused_session(), Some("b"));
    v.focus_pane(0);
    v.close_pane("c");
    assert_eq!(v.focused_session(), Some("b"));
    assert_eq!(v.panes(), ["b"]);
}

#[test]
fn closing_the_only_pane_swaps_in_another_session() {
    let mut v = ViewState::new(four(), "local");
    v.close_pane("a");
    assert_eq!(ids(&v), ["b"]);
}

#[test]
fn resize_focused_changes_the_layout() {
    let mut v = grid(140);
    v.focus_pane(0);
    let before = layout(AREA, &v).panes[0].outer;
    v.resize_focused(AREA, Nav::Down);
    let after = layout(AREA, &v).panes[0].outer;
    assert!(after.height > before.height);
}

#[test]
fn removing_a_shell_pane_closes_it_but_other_panes_get_a_free_session() {
    let mut v = grid(140);
    v.shells.push("d".into());
    v.remove_session("d");
    assert_eq!(v.panes(), ["a", "c", "b"]);
    assert!(v.shells.is_empty());
    v.remove_session("c");
    assert_eq!(v.panes(), ["a", "e", "b"]);
}

#[test]
fn next_prev_and_select_never_duplicate_a_pane() {
    let mut v = grid(140);
    for _ in 0..8 {
        v.apply(Cmd::NextSession);
        let mut seen = v.panes().to_vec();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), v.panes().len());
    }
    for _ in 0..8 {
        v.apply(Cmd::PrevSession);
        let mut seen = v.panes().to_vec();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), v.panes().len());
    }
    let n = v.panes().len();
    v.select("a");
    assert_eq!(v.panes().len(), n);
    assert_eq!(v.focused_session(), Some("a"));
    v.select("f");
    assert!(v.panes().contains(&"f".to_string()));
    assert_eq!(v.panes().len(), n);
}

#[test]
fn next_does_nothing_when_every_session_is_shown() {
    let rows: Vec<SessionRow> = ["a", "b", "c"]
        .iter()
        .map(|id| row(id, id, StateKind::Working, "1m"))
        .collect();
    let mut v = ViewState::new(rows, "");
    v.split_focused(SplitDir::Right, "b");
    v.split_focused(SplitDir::Right, "c");
    let before = v.panes().to_vec();
    v.apply(Cmd::NextSession);
    v.apply(Cmd::PrevSession);
    assert_eq!(v.panes(), before);
}

#[test]
fn split_command_collapses_many_panes_to_the_focused_one() {
    let mut v = grid(140);
    v.shells.push("b".into());
    v.apply(Cmd::Split);
    assert_eq!(ids(&v), ["d"]);
    assert!(v.notice.as_deref().is_some_and(|n| n.contains("shells")));
    assert_eq!(v.shells, ["b"]);
    let mut plain = grid(140);
    plain.apply(Cmd::Split);
    assert_eq!(plain.notice, None);
}

#[test]
fn split_command_direction_follows_terminal_width() {
    let mut wide = ViewState::new(four(), "");
    wide.width = 160;
    wide.apply(Cmd::Split);
    let l = layout(r(0, 0, 160, 40), &wide);
    assert_eq!(l.panes[0].outer.y, l.panes[1].outer.y);
    let mut narrow = ViewState::new(four(), "");
    narrow.width = 80;
    narrow.apply(Cmd::Split);
    let l = layout(r(0, 0, 80, 40), &narrow);
    assert_eq!(l.panes[0].outer.x, l.panes[1].outer.x);
}

#[test]
fn set_initial_focuses_or_replaces_without_duplicating() {
    let mut v = ViewState::new(four(), "");
    v.apply(Cmd::Split);
    v.set_initial("a");
    assert_eq!(ids(&v), ["a", "b"]);
    assert_eq!(v.focused_session(), Some("a"));
    v.set_initial("c");
    assert_eq!(ids(&v), ["c"]);
    assert_eq!(v.focused_session(), Some("c"));
    v.set_initial("b");
    assert_eq!(ids(&v), ["a", "b"]);
    assert_eq!(v.focused_session(), Some("b"));
    let mut empty = ViewState::new(vec![], "");
    empty.set_initial("x");
    assert_eq!(ids(&empty), ["x"]);
}

#[test]
fn vanished_sessions_collapse_the_tree_and_move_focus() {
    let mut v = grid(140);
    v.sessions_changed(
        AREA,
        grid(140)
            .sessions
            .into_iter()
            .filter(|s| s.id != "d" && s.id != "a")
            .collect(),
    );
    assert_eq!(v.panes(), ["c", "b"]);
    assert_eq!(v.focused_session(), Some("b"));
    v.shells.push("c".into());
    v.sessions_changed(AREA, vec![row("b", "b", StateKind::Working, "1m")]);
    assert_eq!(v.panes(), ["b"]);
    assert!(v.shells.is_empty());
}

#[test]
fn prefs_round_trip_keeps_the_tree_and_shells() {
    let mut v = grid(140);
    v.shells.push("d".into());
    v.resize_focused(AREA, Nav::Right);
    v.focus_pane(1);
    let json = serde_json::to_string(&v.prefs()).unwrap();
    let prefs: LayoutPrefs = serde_json::from_str(&json).unwrap();
    let mut fresh = ViewState::new(grid(140).sessions, "");
    fresh.width = 140;
    fresh.apply_prefs(&prefs);
    assert_eq!(fresh.panes(), v.panes());
    assert_eq!(fresh.focused_session(), Some("c"));
    assert_eq!(fresh.shells, ["d"]);
    assert_eq!(layout(AREA, &fresh), layout(AREA, &v));
}

#[test]
fn prefs_without_a_tree_still_load() {
    let old = r#"{"panes":["a","b"],"focus":1,"zoom":true,"sidebar":false}"#;
    let prefs: LayoutPrefs = serde_json::from_str(old).unwrap();
    assert_eq!(prefs.tree, None);
    assert!(prefs.shells.is_empty());
    let mut v = ViewState::new(four(), "");
    v.width = 160;
    v.apply_prefs(&prefs);
    assert_eq!(ids(&v), ["a", "b"]);
    assert_eq!(v.focus(), 1);
    assert!(v.zoom);
    assert_eq!(v.sidebar, Some(false));
    v.zoom = false;
    let l = layout(r(0, 0, 160, 40), &v);
    assert_eq!(l.panes[0].outer.y, l.panes[1].outer.y);
}

#[test]
fn prefs_with_more_than_two_panes_build_a_chain() {
    let prefs = LayoutPrefs {
        panes: vec!["a".into(), "b".into(), "c".into()],
        focus: 2,
        ..Default::default()
    };
    let mut v = ViewState::new(four(), "");
    v.width = 150;
    v.apply_prefs(&prefs);
    assert_eq!(ids(&v), ["a", "b", "c"]);
    let l = layout(r(0, 0, 150, 40), &v);
    let widths: Vec<u16> = l.panes.iter().map(|p| p.outer.width).collect();
    assert_eq!(
        widths.iter().sum::<u16>() + l.sidebar.map_or(0, |s| s.width),
        150
    );
    assert!(widths.iter().all(|w| *w >= 30), "{widths:?}");
}

#[test]
fn prefs_drop_missing_sessions_and_reject_bad_trees() {
    let mut tree = node(
        SplitDir::Right,
        5000,
        leaf("a"),
        node(SplitDir::Down, 3, leaf("zzz"), leaf("c")),
    );
    let mut v = ViewState::new(four(), "");
    v.apply_prefs(&LayoutPrefs {
        tree: Some(tree.clone()),
        focus: 9,
        shells: vec!["zzz".into(), "c".into()],
        ..Default::default()
    });
    assert_eq!(ids(&v), ["a", "c"]);
    assert_eq!(v.focused_session(), Some("c"));
    assert_eq!(v.shells, ["c"]);
    let rects = layout(AREA, &v).panes;
    assert!(rects.iter().all(|p| p.outer.width >= 3));

    tree = node(SplitDir::Right, 500, leaf("a"), leaf("a"));
    let mut dup = ViewState::new(four(), "");
    dup.apply_prefs(&LayoutPrefs {
        tree: Some(tree),
        panes: vec!["b".into()],
        ..Default::default()
    });
    assert_eq!(ids(&dup), ["b"]);

    let mut big = leaf("s0");
    for i in 1..20 {
        big.split_leaf(&format!("s{}", i - 1), SplitDir::Right, &format!("s{i}"));
    }
    let mut capped = ViewState::new(four(), "");
    capped.apply_prefs(&LayoutPrefs {
        tree: Some(big),
        panes: vec!["c".into()],
        ..Default::default()
    });
    assert_eq!(ids(&capped), ["c"]);
}

fn cmds(bytes: &[&[u8]]) -> Vec<Action> {
    let mut s = InputSplitter::new();
    bytes.iter().flat_map(|b| s.feed(b)).collect()
}

#[test]
fn prefix_split_and_navigation_keys() {
    let table: &[(&[u8], Cmd)] = &[
        (b"|", Cmd::SplitRight),
        (b"v", Cmd::SplitRight),
        (b"-", Cmd::SplitDown),
        (b"h", Cmd::SplitDown),
        (b"\x1b[A", Cmd::Focus(Nav::Up)),
        (b"\x1b[B", Cmd::Focus(Nav::Down)),
        (b"\x1b[C", Cmd::Focus(Nav::Right)),
        (b"\x1b[D", Cmd::Focus(Nav::Left)),
        (b"\x1bOA", Cmd::Focus(Nav::Up)),
        (b"\x1bOD", Cmd::Focus(Nav::Left)),
        (b"\x1b[1;2A", Cmd::Resize(Nav::Up)),
        (b"\x1b[1;2D", Cmd::Resize(Nav::Left)),
        (b"\x1b[Z", Cmd::FocusPrev),
        (b"\t", Cmd::FocusOther),
    ];
    for (keys, cmd) in table {
        let mut s = InputSplitter::new();
        let mut input = vec![0x1d];
        input.extend_from_slice(keys);
        assert_eq!(s.feed(&input), vec![Action::Cmd(*cmd)], "{keys:?}");
        assert_eq!(s.feed(b"x"), vec![forward(b"x")]);
    }
}

#[test]
fn prefix_arrows_survive_being_split_across_reads() {
    let full = b"\x1b[1;2C";
    for cut in 1..full.len() {
        let mut s = InputSplitter::new();
        assert_eq!(s.feed(b"\x1d"), vec![]);
        assert_eq!(s.feed(&full[..cut]), vec![], "cut {cut}");
        assert_eq!(
            s.feed(&full[cut..]),
            vec![Action::Cmd(Cmd::Resize(Nav::Right))],
            "cut {cut}"
        );
        assert_eq!(s.feed(b"k"), vec![forward(b"k")]);
    }
    let mut s = InputSplitter::new();
    let acts = cmds(&[b"\x1d", b"\x1b", b"O", b"B", b"z"]);
    assert_eq!(
        acts,
        vec![Action::Cmd(Cmd::Focus(Nav::Down)), forward(b"z")]
    );
    assert_eq!(
        s.feed(b"\x1d\x1b[B"),
        vec![Action::Cmd(Cmd::Focus(Nav::Down))]
    );
}

#[test]
fn other_sequences_after_the_prefix_are_swallowed() {
    for seq in [
        &b"\x1b[1;5A"[..],
        b"\x1b[3~",
        b"\x1b[97;5u",
        b"\x1bOP",
        b"\x1b[1;2P",
    ] {
        let mut s = InputSplitter::new();
        let mut input = vec![0x1d];
        input.extend_from_slice(seq);
        assert_eq!(s.feed(&input), vec![], "{seq:?}");
        assert_eq!(s.feed(b"n"), vec![forward(b"n")], "{seq:?}");
    }
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"\x1d"), vec![]);
    assert_eq!(s.feed(b"\x1b[1;"), vec![]);
    assert_eq!(s.feed(b"5A"), vec![]);
    assert_eq!(s.feed(b"n"), vec![forward(b"n")]);
}

#[test]
fn overlong_sequence_after_the_prefix_is_dropped_not_forwarded() {
    let mut s = InputSplitter::new();
    let out = s.feed(b"\x1d\x1b[1;1;1;1;1;1;1;1;1");
    assert!(out
        .iter()
        .all(|a| !matches!(a, Action::Forward(d) if d.contains(&0x1b))));
}

#[test]
fn lone_escape_after_the_prefix_is_never_forwarded() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"\x1d\x1b"), vec![]);
    let later = std::time::Instant::now() + std::time::Duration::from_millis(50);
    assert_eq!(s.flush_stale(later), vec![]);
    assert_eq!(s.feed(b"n"), vec![forward(b"n")]);

    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"\x1d\x1b[1;2"), vec![]);
    assert_eq!(s.flush_stale(later), vec![]);
    assert_eq!(s.feed(b"n"), vec![forward(b"n")]);
}

#[test]
fn escape_without_the_prefix_still_flushes_to_the_agent() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"\x1b"), vec![]);
    let later = std::time::Instant::now() + std::time::Duration::from_millis(50);
    assert_eq!(s.flush_stale(later), vec![forward(b"\x1b")]);
}

#[test]
fn mouse_between_prefix_and_key_keeps_the_prefix() {
    let mut s = InputSplitter::new();
    let mut input = vec![0x1d];
    input.extend(sgr(35, 3, 3, false));
    input.extend_from_slice(b"\x1b[B");
    assert_eq!(
        s.feed(&input),
        vec![
            Action::Mouse(ev(35, 3, 3, false)),
            Action::Cmd(Cmd::Focus(Nav::Down))
        ]
    );
}

#[test]
fn bare_keys_and_arrows_without_the_prefix_pass_through() {
    for seq in [
        &b"\x1b[A"[..],
        b"\x1bOB",
        b"\x1b[1;2C",
        b"\x1b[Z",
        b"\t",
        b"\x04",
    ] {
        let mut s = InputSplitter::new();
        assert_eq!(s.feed(seq), vec![forward(seq)], "{seq:?}");
    }
}

#[test]
fn clicks_and_hits_land_in_the_right_pane_of_a_grid() {
    let mut v = grid(140);
    let lay = layout(AREA, &v);
    let third = &lay.panes[2];
    assert_eq!(third.session, "b");
    let (c, rw) = (third.inner.x + 2, third.inner.y + 2);
    assert!(matches!(v.hit(AREA, c, rw), Hit::Pane { idx: 2, .. }));
    v.on_mouse(
        AREA,
        ev(0, c, rw, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert_eq!(v.focused_session(), Some("b"));
}

#[test]
fn drops_and_pastes_route_to_the_pane_under_the_pointer_in_a_grid() {
    let mut v = grid(140);
    assert_eq!(v.paste_target(AREA), v.focus());
    let lay = layout(AREA, &v);
    let second = &lay.panes[1];
    v.on_mouse(
        AREA,
        ev(35, second.inner.x + 1, second.inner.y + 1, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert_eq!(v.paste_target(AREA), 1);
    assert_eq!(v.panes()[v.paste_target(AREA)], "c");
}

#[test]
fn drag_from_one_pane_stays_clamped_to_it_when_over_another() {
    let mut v = grid(140);
    let lay = layout(AREA, &v);
    let first = lay.panes[0].clone();
    let second = lay.panes[2].clone();
    let info = mouse_info(sgr_mode(MouseLevel::Drag), 0);
    let out = v.on_mouse(
        AREA,
        ev(0, first.inner.x + 1, first.inner.y + 1, false),
        &info,
    );
    assert_eq!(out.forward.as_ref().map(|f| f.0.as_str()), Some("a"));
    let out = v.on_mouse(
        AREA,
        ev(32, second.inner.x + 3, second.inner.y + 3, false),
        &info,
    );
    let (id, bytes) = out.forward.unwrap();
    assert_eq!(id, "a");
    let want = format!(
        "\x1b[<32;{};{}M",
        (first.inner.width - 1).min(second.inner.x + 3 - first.inner.x) + 1,
        (first.inner.height - 1).min(second.inner.y + 3 - first.inner.y) + 1,
    );
    assert_eq!(String::from_utf8(bytes).unwrap(), want);
    v.on_mouse(
        AREA,
        ev(0, second.inner.x + 3, second.inner.y + 3, true),
        &info,
    );
    assert_eq!(v.grab, None);
}

#[test]
fn wheel_over_an_unfocused_grid_pane_focuses_and_scrolls_it() {
    let mut v = grid(140);
    let lay = layout(AREA, &v);
    let p = &lay.panes[1];
    v.on_mouse(
        AREA,
        ev(64, p.inner.x + 1, p.inner.y + 1, false),
        &mouse_info(MouseMode::default(), 10),
    );
    assert_eq!(v.focused_session(), Some("c"));
    assert!(v.scroll.is_some());
}

#[test]
fn header_buttons_work_in_every_grid_pane() {
    let v = grid(140);
    let lay = layout(AREA, &v);
    for p in &lay.panes {
        let x = header_button(p.outer, Control::Stop).unwrap();
        assert_eq!(
            v.hit(AREA, x, p.outer.y),
            Hit::Button(p.session.clone(), Control::Stop)
        );
    }
}

#[test]
fn grab_is_cleared_when_its_session_leaves_the_panes() {
    let mut v = grid(140);
    let p = layout(AREA, &v).panes[0].clone();
    v.on_mouse(
        AREA,
        ev(0, p.inner.x + 1, p.inner.y + 1, false),
        &mouse_info(sgr_mode(MouseLevel::Drag), 0),
    );
    assert_eq!(v.grab.as_deref(), Some("a"));
    v.close_pane("a");
    assert_eq!(v.grab, None);
}

#[test]
fn scroll_resets_when_focus_changes_between_panes() {
    let mut v = grid(140);
    v.apply(Cmd::ScrollMode);
    v.apply(Cmd::FocusOther);
    assert!(v.scroll.is_none());
}

#[test]
fn help_overlay_fits_a_24_row_terminal() {
    let mut v = two_pane(100);
    v.help = true;
    let text = draw(&v, &screens(), 100, 24);
    for key in [
        "split right",
        "split down",
        "focus next pane",
        "move focus",
        "resize",
    ] {
        assert!(text.contains(key), "{key}");
    }
    assert_snapshot("help_short", &text);
}

#[test]
fn grid_renders_without_panicking_at_many_sizes() {
    for (w, h) in [(160, 45), (100, 24), (80, 24), (40, 10), (20, 6), (5, 3)] {
        let mut v = grid(w);
        v.help = true;
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| render(f, &v, &Screens::default())).unwrap();
    }
}

#[test]
fn ctrl_arrow_alt_key_after_prefix_is_dropped() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"\x1d\x1bx"), vec![]);
    assert_eq!(s.feed(b"n"), vec![forward(b"n")]);
}

#[test]
fn double_escape_after_prefix_is_dropped_and_recurses() {
    let mut s = InputSplitter::new();
    assert_eq!(
        s.feed(b"\x1d\x1b\x1b[A"),
        vec![Action::Cmd(Cmd::Focus(Nav::Up))]
    );
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"\x1d\x1b\x1bx"), vec![]);
    assert_eq!(s.feed(b"n"), vec![forward(b"n")]);
}

#[test]
fn overflowing_sequence_is_swallowed_across_reads() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"\x1d\x1b[1;1;1;1;1;1;1;1;1"), vec![]);
    assert_eq!(s.feed(b";1;1m"), vec![]);
    assert_eq!(s.feed(b"n"), vec![forward(b"n")]);
}

#[test]
fn swallow_stops_at_a_control_byte() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"\x1d\x1b[1;1;1;1;1;1;1;1;1"), vec![]);
    assert_eq!(s.feed(b"\r"), vec![forward(b"\r")]);
}

#[test]
fn sessions_appearing_later_refill_an_empty_view() {
    let mut v = ViewState::new(vec![], "local");
    assert!(v.panes().is_empty());
    v.sessions_changed(AREA, four());
    assert_eq!(ids(&v), ["a"]);
    assert_eq!(v.focused_session(), Some("a"));
}

#[test]
fn split_at_keeps_focus_unless_asked() {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    assert!(v.split_at("a", SplitDir::Right, "b", false));
    assert_eq!(ids(&v), ["a", "b"]);
    assert_eq!(v.focused_session(), Some("a"));
    assert!(v.split_at("a", SplitDir::Down, "c", true));
    assert_eq!(v.focused_session(), Some("c"));
    assert!(!v.split_at("zz", SplitDir::Down, "d", true));
    assert_eq!(v.focused_session(), Some("c"));
}

#[test]
fn narrow_stacked_right_split_resizes_on_the_vertical_axis() {
    let rows: Vec<SessionRow> = ["a", "b"]
        .iter()
        .map(|id| row(id, id, StateKind::Working, "1m"))
        .collect();
    let mut v = ViewState::new(rows, "local");
    v.sidebar = Some(false);
    assert!(v.split_focused(SplitDir::Right, "b"));
    v.focus_pane(0);
    let narrow = Rect::new(0, 0, 40, 30);
    let before = layout(narrow, &v).panes[0].outer;
    assert_eq!(before.width, 40);
    v.resize_focused(narrow, Nav::Right);
    assert_eq!(layout(narrow, &v).panes[0].outer.height, before.height);
    v.resize_focused(narrow, Nav::Down);
    assert!(layout(narrow, &v).panes[0].outer.height > before.height);
}

#[test]
fn wide_right_split_still_resizes_on_the_horizontal_axis() {
    let rows: Vec<SessionRow> = ["a", "b"]
        .iter()
        .map(|id| row(id, id, StateKind::Working, "1m"))
        .collect();
    let mut v = ViewState::new(rows, "local");
    v.sidebar = Some(false);
    assert!(v.split_focused(SplitDir::Right, "b"));
    v.focus_pane(0);
    let wide = Rect::new(0, 0, 120, 30);
    let before = layout(wide, &v).panes[0].outer;
    v.resize_focused(wide, Nav::Right);
    assert!(layout(wide, &v).panes[0].outer.width > before.width);
    v.resize_focused(wide, Nav::Down);
}

#[test]
fn help_shows_the_mouse_section_at_34_rows() {
    let mut v = two_pane(140);
    v.help = true;
    let text = draw(&v, &screens(), 140, 34);
    assert!(text.contains("pick a tab, a session or a pane"));
    assert!(text.contains("Shift-drag selects"));
    assert!(text.contains("? again closes this box"));
}

#[test]
fn help_explains_the_status_bar_when_there_is_room() {
    let mut v = two_pane(140);
    v.help = true;
    let tall = draw(&v, &screens(), 140, 40);
    assert!(
        tall.contains("5h/wk Claude, cx Codex, $ cost; ~ old, ? n/a"),
        "{tall}"
    );
    assert!(tall.contains("? again closes this box"));
    let short = draw(&v, &screens(), 140, 32);
    assert!(!short.contains("Status bar"));
}

#[test]
fn v1_prefs_convert_with_the_real_terminal_width() {
    let prefs = LayoutPrefs {
        panes: vec!["a".into(), "b".into()],
        focus: 0,
        ..LayoutPrefs::default()
    };
    let area = Rect::new(0, 0, 60, 30);
    let mut narrow = ViewState::new(four(), "local");
    narrow.width = 60;
    narrow.apply_prefs(&prefs);
    let rects = layout(area, &narrow).panes;
    assert_eq!(rects[0].outer.y, 0);
    assert!(rects[1].outer.y > 0);
    let mut wide = ViewState::new(four(), "local");
    wide.width = 160;
    wide.apply_prefs(&prefs);
    let rects = layout(Rect::new(0, 0, 160, 30), &wide).panes;
    assert!(rects[1].outer.x > 0);
}

#[test]
fn closing_the_focused_pane_leaves_scroll_mode() {
    let mut v = grid(140);
    v.apply(Cmd::ScrollMode);
    assert!(v.scroll.is_some());
    let id = v.focused_session().unwrap().to_string();
    v.close_pane(&id);
    assert!(v.scroll.is_none());
}

fn focus_exited() -> ViewState {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    v.select("d");
    v
}

#[test]
fn plain_enter_and_r_stay_plain_input_without_the_prefix() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"R\r"), vec![forward(b"R\r")]);
    assert_eq!(s.feed(&[0x1d, b'r']), vec![Action::Cmd(Cmd::Rename)]);
}

#[test]
fn resume_replaces_pause_in_the_header_of_an_exited_pane_only() {
    let v = focus_exited();
    let outer = layout(AREA, &v).panes[0].outer;
    let slot = header_button(outer, Control::Resume).unwrap();
    assert_eq!(Some(slot), header_button(outer, Control::Interrupt));
    assert_eq!(
        v.hit(AREA, slot, 0),
        Hit::Button("d".into(), Control::Resume)
    );
    assert_eq!(
        v.hit(AREA, header_button(outer, Control::Stop).unwrap(), 0),
        Hit::Button("d".into(), Control::Stop)
    );
    let running = ViewState::new(four(), "local");
    let outer = layout(AREA, &running).panes[0].outer;
    assert_eq!(
        running.hit(AREA, slot_of(outer), 0),
        Hit::Button("a".into(), Control::Interrupt)
    );
}

fn slot_of(outer: Rect) -> u16 {
    header_button(outer, Control::Interrupt).unwrap()
}

#[test]
fn resume_replaces_stop_in_the_sidebar_row_of_an_exited_session_only() {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    let sb = layout(AREA, &v).sidebar.unwrap();
    let slot = sidebar_button(sb, Control::Resume).unwrap();
    assert_eq!(Some(slot), sidebar_button(sb, Control::Stop));
    assert_eq!(
        v.hit(AREA, slot, 4),
        Hit::Button("d".into(), Control::Resume)
    );
    assert_eq!(
        v.hit(AREA, sidebar_button(sb, Control::Close).unwrap(), 4),
        Hit::Button("d".into(), Control::Close)
    );
    assert_eq!(v.hit(AREA, slot, 2), Hit::Button("a".into(), Control::Stop));
    assert_eq!(v.hit(AREA, slot, 3), Hit::Button("b".into(), Control::Stop));
}

#[test]
fn clicking_the_play_glyph_reports_a_resume_control() {
    let mut v = focus_exited();
    let outer = layout(AREA, &v).panes[0].outer;
    let out = v.on_mouse(
        AREA,
        ev(0, slot_of(outer), 0, false),
        &mouse_info(sgr_mode(MouseLevel::Any), 0),
    );
    assert_eq!(out.control, Some(("d".into(), Control::Resume)));
}

#[test]
fn exited_pane_header_draws_play_and_running_draws_pause() {
    let v = focus_exited();
    let text = draw(&v, &screens(), 140, 10);
    let first = text.lines().next().unwrap();
    assert!(first.contains('▶') && !first.contains('⏸'), "{first}");
    let running = ViewState::new(four(), "local");
    let first = draw(&running, &screens(), 140, 10);
    let first = first.lines().next().unwrap();
    assert!(first.contains('⏸') && !first.contains('▶'), "{first}");
}

#[test]
fn hovered_sidebar_row_of_an_exited_session_shows_play() {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    v.pointer = Some((3, 4));
    let text = draw(&v, &screens(), 140, 10);
    let line = text.lines().nth(4).unwrap();
    assert!(line.contains('▶') && !line.contains('■'), "{line}");
    v.pointer = Some((3, 2));
    let text = draw(&v, &screens(), 140, 10);
    let line = text.lines().nth(2).unwrap();
    assert!(line.contains('■') && !line.contains('▶'), "{line}");
}

#[test]
fn snapshot_exited_header_with_play_glyph() {
    let v = focus_exited();
    assert_snapshot("exited_play", &draw(&v, &screens(), 140, 8));
}

fn resumed(id: &str, name: &str, from: &str) -> SessionRow {
    SessionRow {
        resumed_from: Some(from.into()),
        ..row(id, name, StateKind::Working, "1s")
    }
}

#[test]
fn replace_session_keeps_focus_tree_shape_and_shells() {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    v.apply(Cmd::Split);
    v.shells.push("b".into());
    let tree_before = v.prefs().tree.clone();
    v.zoom = true;
    v.scroll = Some(ScrollState::default());
    v.sessions
        .push(row("b2", "dev-server", StateKind::Working, "1s"));
    v.replace_session("b", "b2");
    assert_eq!(ids(&v), ["a", "b2"]);
    assert_eq!(v.focused_session(), Some("b2"));
    assert_eq!(v.shells, vec!["b2".to_string()]);
    assert!(v.sessions.iter().all(|s| s.id != "b"));
    assert!(v.zoom);
    assert!(v.scroll.is_none());
    let renamed = v.prefs().tree.unwrap().leaves().join(",");
    assert_eq!(
        renamed,
        tree_before.unwrap().leaves().join(",").replace('b', "b2")
    );
}

#[test]
fn replace_session_of_a_background_pane_keeps_focus_and_scroll() {
    let mut v = two_pane(140);
    v.focus_pane(0);
    v.scroll = Some(ScrollState::default());
    v.sessions
        .push(row("b2", "dev-server", StateKind::Working, "1s"));
    v.replace_session("b", "b2");
    assert_eq!(ids(&v), ["a", "b2"]);
    assert_eq!(v.focused_session(), Some("a"));
    assert!(v.scroll.is_some());
}

#[test]
fn sessions_changed_renames_a_vanished_pane_to_its_resumed_copy() {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    v.select("d");
    v.apply(Cmd::Split);
    v.shells.push("d".into());
    let mut rows = four();
    rows.retain(|r| r.id != "d");
    rows.push(resumed("d2", "tests", "d"));
    v.sessions_changed(AREA, rows);
    assert_eq!(ids(&v), ["d2", "a"]);
    assert!(v.panes().iter().all(|p| p != "d"));
    assert_eq!(v.shells, vec!["d2".to_string()]);
}

#[test]
fn list_before_reply_and_reply_before_list_end_up_the_same() {
    let mut list_first = ViewState::new(four(), "local");
    list_first.select("d");
    let mut rows = four();
    rows.retain(|r| r.id != "d");
    rows.push(resumed("d2", "tests", "d"));
    list_first.sessions_changed(AREA, rows.clone());
    list_first.replace_session("d", "d2");

    let mut reply_first = ViewState::new(four(), "local");
    reply_first.select("d");
    reply_first.sessions.push(resumed("d2", "tests", "d"));
    reply_first.replace_session("d", "d2");
    reply_first.sessions_changed(AREA, rows);

    assert_eq!(ids(&list_first), ["d2"]);
    assert_eq!(ids(&reply_first), ["d2"]);
    assert_eq!(list_first.focused_session(), reply_first.focused_session());
}

#[test]
fn resumed_from_does_not_steal_a_pane_whose_original_still_exists() {
    let mut v = ViewState::new(four(), "local");
    let mut rows = four();
    rows.push(resumed("a2", "claude", "a"));
    v.sessions_changed(AREA, rows);
    assert_eq!(ids(&v), ["a"]);
}

fn open_palette(v: &mut ViewState) {
    v.apply(Cmd::Palette);
    assert!(v.palette.is_some());
}

fn query(v: &mut ViewState, q: &str) {
    open_palette(v);
    v.insert_palette_text(q.as_bytes());
}

fn first(v: &ViewState) -> PaletteEntry {
    v.entries().remove(0).entry
}

#[test]
fn every_action_key_reaches_its_cmd() {
    let table: &[(u8, Cmd)] = &[
        (b'd', Cmd::Detach),
        (b'n', Cmd::NextSession),
        (b'p', Cmd::PrevSession),
        (b's', Cmd::Split),
        (b'|', Cmd::SplitRight),
        (b'v', Cmd::SplitRight),
        (b'-', Cmd::SplitDown),
        (b'h', Cmd::SplitDown),
        (b'\t', Cmd::FocusOther),
        (b'z', Cmd::Zoom),
        (b'b', Cmd::Sidebar),
        (b'[', Cmd::ScrollMode),
        (b'?', Cmd::Help),
        (b'w', Cmd::Picker),
        (b'x', Cmd::Stop),
        (b'X', Cmd::Remove),
        (b'r', Cmd::Rename),
        (b'\r', Cmd::Resume),
        (b'R', Cmd::Resume),
        (b':', Cmd::Palette),
    ];
    for (byte, cmd) in table {
        let mut s = InputSplitter::new();
        assert_eq!(
            s.feed(&[0x1d, *byte]),
            vec![Action::Cmd(*cmd)],
            "{cmd:?} via {byte}"
        );
    }
}

#[test]
fn escape_bindings_stay_outside_the_table() {
    let table: &[(&[u8], Cmd)] = &[
        (b"\x1b[Z", Cmd::FocusPrev),
        (b"\x1b[A", Cmd::Focus(Nav::Up)),
        (b"\x1bOD", Cmd::Focus(Nav::Left)),
        (b"\x1b[1;2C", Cmd::Resize(Nav::Right)),
    ];
    for (seq, cmd) in table {
        let mut s = InputSplitter::new();
        assert_eq!(
            s.feed(&[b"\x1d".as_slice(), seq].concat()),
            vec![Action::Cmd(*cmd)]
        );
    }
    for cmd in [Cmd::FocusPrev, Cmd::Focus(Nav::Left)] {
        assert!(ACTIONS.iter().any(|d| d.cmd == cmd && d.keys.is_empty()));
    }
}

#[test]
fn help_rows_resolve_in_the_action_table() {
    for (key, _) in render::HELP {
        if let render::HelpKey::Cmds(cmds) = key {
            for c in *cmds {
                assert!(ACTIONS.iter().any(|d| d.cmd == *c), "{c:?}");
            }
        }
    }
}

#[test]
fn palette_entries_are_exactly_the_palette_actions() {
    let mut v = ViewState::new(Vec::new(), "");
    open_palette(&mut v);
    let cmds: Vec<Cmd> = v
        .entries()
        .into_iter()
        .map(|r| match r.entry {
            PaletteEntry::Cmd(c) => c,
            _ => panic!("no sessions"),
        })
        .collect();
    let table: Vec<Cmd> = ACTIONS
        .iter()
        .filter(|d| d.palette)
        .map(|d| d.cmd)
        .collect();
    assert_eq!(cmds, table);
    assert!(!table.contains(&Cmd::Palette) && !table.contains(&Cmd::Picker));
}

#[test]
fn ctrl_bracket_colon_opens_the_palette_and_toggles() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"\x1d:"), vec![Action::Cmd(Cmd::Palette)]);
    let mut v = ViewState::new(four(), "");
    v.help = true;
    v.apply(Cmd::Palette);
    assert!(v.palette.is_some());
    assert!(!v.help);
    v.apply(Cmd::Palette);
    assert!(v.palette.is_none());
}

#[test]
fn empty_query_lists_actions_in_table_order_then_sessions_in_sidebar_order() {
    let mut v = ViewState::new(four(), "");
    open_palette(&mut v);
    let rows = v.entries();
    let n = ACTIONS.iter().filter(|d| d.palette).count();
    assert_eq!(rows.len(), n + 4);
    assert_eq!(rows[0].entry, PaletteEntry::Cmd(Cmd::Detach));
    assert_eq!(rows[0].right, "Ctrl-] d");
    let tail: Vec<_> = rows[n..].iter().map(|r| r.entry.clone()).collect();
    let want: Vec<_> = ["c", "a", "b", "d"]
        .iter()
        .map(|i| PaletteEntry::Session((*i).into()))
        .collect();
    assert_eq!(tail, want);
    assert_eq!(rows[n].right, "blocked");
}

#[test]
fn typing_spl_puts_the_split_entries_first() {
    let mut v = ViewState::new(four(), "");
    query(&mut v, "spl");
    let top: Vec<_> = v.entries().into_iter().take(3).map(|r| r.entry).collect();
    for want in [Cmd::Split, Cmd::SplitRight, Cmd::SplitDown] {
        assert!(
            top.contains(&PaletteEntry::Cmd(want)),
            "{want:?} in {top:?}"
        );
    }
}

#[test]
fn a_session_name_beats_an_action_when_it_matches_better() {
    let mut v = ViewState::new(four(), "");
    query(&mut v, "tests");
    assert_eq!(first(&v), PaletteEntry::Session("d".into()));
    let hits = &v.entries()[0].hits;
    assert_eq!(hits, &vec![0, 1, 2, 3, 4]);
    let mut v = ViewState::new(four(), "");
    query(&mut v, "kill");
    assert_eq!(first(&v), PaletteEntry::Cmd(Cmd::Stop));
}

#[test]
fn query_with_no_match_is_empty_and_enter_does_nothing() {
    let mut v = ViewState::new(four(), "");
    query(&mut v, "zzzqq");
    assert!(v.entries().is_empty());
    assert_eq!(v.edit_palette(b"\r"), PaletteEdit::Editing);
    assert!(v.palette.is_some());
}

#[test]
fn palette_editing_keys() {
    let mut v = ViewState::new(four(), "");
    open_palette(&mut v);
    assert_eq!(v.edit_palette(b"ab\xc3\xa9"), PaletteEdit::Editing);
    assert_eq!(v.palette.as_ref().unwrap().query, "abé");
    v.edit_palette(b"\x7f");
    v.edit_palette(b"\x08");
    assert_eq!(v.palette.as_ref().unwrap().query, "a");
    v.edit_palette(b"x\x00\x1d\x01y");
    assert_eq!(v.palette.as_ref().unwrap().query, "axy");
    v.edit_palette(b"\x15");
    assert_eq!(v.palette.as_ref().unwrap().query, "");
    v.edit_palette(&[b'q'; 100]);
    assert_eq!(v.palette.as_ref().unwrap().query.len(), 64);
}

#[test]
fn palette_arrows_move_the_selection_and_never_close() {
    let mut v = ViewState::new(four(), "");
    open_palette(&mut v);
    for (keys, sel) in [
        (&b"\x1b[B"[..], 1),
        (b"\x1bOB", 2),
        (b"\x0e", 3),
        (b"\x10", 2),
        (b"\x1b[A", 1),
        (b"\x1bOA", 0),
        (b"\x1b[A", 0),
    ] {
        assert_eq!(v.edit_palette(keys), PaletteEdit::Editing);
        assert_eq!(v.palette.as_ref().unwrap().sel, sel);
    }
    for other in [
        &b"\x1b[200~"[..],
        b"\x1b[1;5C",
        b"\x1b[3~",
        b"\x1bOP",
        b"\x1bx",
    ] {
        assert_eq!(v.edit_palette(other), PaletteEdit::Editing);
    }
    assert_eq!(v.palette.as_ref().unwrap().query, "");
}

#[test]
fn selection_clamps_to_the_last_row_and_resets_on_typing() {
    let mut v = ViewState::new(four(), "");
    open_palette(&mut v);
    let last = v.entries().len() - 1;
    for _ in 0..(last + 5) {
        v.edit_palette(b"\x1b[B");
    }
    assert_eq!(v.palette.as_ref().unwrap().sel, last);
    v.edit_palette(b"s");
    assert_eq!(v.palette.as_ref().unwrap().sel, 0);
}

#[test]
fn lone_escape_closes_the_palette() {
    let mut v = ViewState::new(four(), "");
    open_palette(&mut v);
    assert_eq!(v.edit_palette(b"\x1b"), PaletteEdit::Close);
    assert!(v.palette.is_none());
}

#[test]
fn mixed_chunk_types_then_moves() {
    let mut v = ViewState::new(four(), "");
    open_palette(&mut v);
    assert_eq!(v.edit_palette(b"c\x1b[B\x1b[B"), PaletteEdit::Editing);
    let p = v.palette.as_ref().unwrap();
    assert_eq!(p.query, "c");
    assert_eq!(p.sel, 2);
}

#[test]
fn enter_runs_the_selected_entry_and_ignores_the_rest_of_the_chunk() {
    let mut v = ViewState::new(four(), "");
    open_palette(&mut v);
    assert_eq!(
        v.edit_palette(b"\x1b[B\rzz"),
        PaletteEdit::Run(PaletteEntry::Cmd(Cmd::NextSession))
    );
    assert!(v.palette.is_none());
    query(&mut v, "tests");
    assert_eq!(
        v.edit_palette(b"\n"),
        PaletteEdit::Run(PaletteEntry::Session("d".into()))
    );
    assert!(v.palette.is_none());
}

#[test]
fn pasted_text_lands_in_the_query_without_markers_or_controls() {
    let mut v = ViewState::new(four(), "");
    open_palette(&mut v);
    v.insert_palette_text(b"\x1b[200~sp\nl\x1b[201~");
    assert_eq!(v.palette.as_ref().unwrap().query, "spl");
    let mut closed = ViewState::new(four(), "");
    closed.insert_palette_text(b"x");
    assert!(closed.palette.is_none());
}

fn palette_geometry(v: &ViewState) -> (Rect, Rect) {
    let rect = palette::palette_rect(AREA, v.entries().len().max(1));
    (rect, palette::palette_list(rect))
}

#[test]
fn clicking_a_palette_row_returns_the_entry_and_closes() {
    let mut v = ViewState::new(four(), "");
    open_palette(&mut v);
    let (_, list) = palette_geometry(&v);
    let info = mouse_info(sgr_mode(MouseLevel::Any), 0);
    let out = v.on_mouse(AREA, ev(0, list.x + 3, list.y + 1, false), &info);
    assert_eq!(out.palette, Some(PaletteEntry::Cmd(Cmd::NextSession)));
    assert!(out.redraw);
    assert!(v.palette.is_none());
    let out = v.on_mouse(AREA, ev(0, list.x + 3, list.y + 1, true), &info);
    assert_eq!(out.forward, None);
    assert_eq!(v.grab, None);
}

#[test]
fn clicking_outside_the_palette_closes_it_and_swallows_the_release() {
    let mut v = ViewState::new(four(), "");
    open_palette(&mut v);
    let info = mouse_info(sgr_mode(MouseLevel::Any), 0);
    let out = v.on_mouse(AREA, ev(0, 1, 1, false), &info);
    assert_eq!(out.palette, None);
    assert!(v.palette.is_none());
    let out = v.on_mouse(AREA, ev(0, 1, 1, true), &info);
    assert_eq!(out.forward, None);
    assert_eq!(v.grab, None);
}

#[test]
fn clicking_the_query_line_keeps_the_palette_and_wheel_moves_the_selection() {
    let mut v = ViewState::new(four(), "");
    open_palette(&mut v);
    let (rect, _) = palette_geometry(&v);
    let info = mouse_info(sgr_mode(MouseLevel::Any), 0);
    let out = v.on_mouse(AREA, ev(0, rect.x + 5, rect.y + 1, false), &info);
    assert_eq!(out, MouseOut::default());
    assert!(v.palette.is_some());
    v.on_mouse(AREA, ev(0, rect.x + 5, rect.y + 1, true), &info);
    assert_eq!(v.grab, None);
    let out = v.on_mouse(AREA, ev(65, 3, 3, false), &info);
    assert!(out.redraw);
    assert_eq!(v.palette.as_ref().unwrap().sel, 1);
    assert_eq!(out.forward, None);
    v.on_mouse(AREA, ev(64, 3, 3, false), &info);
    assert_eq!(v.palette.as_ref().unwrap().sel, 0);
}

fn palette_view() -> ViewState {
    let mut v = two_pane(140);
    open_palette(&mut v);
    v
}

#[test]
fn snapshot_palette_empty_query() {
    let v = palette_view();
    assert_snapshot("palette", &draw(&v, &screens(), 100, 24));
}

#[test]
fn snapshot_palette_filtered() {
    let mut v = palette_view();
    v.insert_palette_text(b"re");
    v.edit_palette(b"\x1b[B");
    assert_snapshot("palette_filtered", &draw(&v, &screens(), 100, 24));
}

#[test]
fn palette_with_no_match_says_so() {
    let mut v = palette_view();
    v.insert_palette_text(b"zzzqq");
    assert!(draw(&v, &screens(), 100, 24).contains("no match"));
}

#[test]
fn palette_renders_without_panicking_at_tiny_sizes() {
    for (w, h) in [(5, 3), (20, 6), (40, 10), (1, 1), (100, 24)] {
        let mut v = grid(w);
        open_palette(&mut v);
        v.insert_palette_text("é字x".as_bytes());
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| render(f, &v, &Screens::default())).unwrap();
        let lay = layout(Rect::new(0, 0, w, h), &v);
        let _ = cursor(&v, &lay, &Screens::default());
    }
}

#[test]
fn palette_cursor_sits_at_the_end_of_the_query() {
    let mut v = palette_view();
    let area = Rect::new(0, 0, 100, 24);
    let lay = layout(area, &v);
    let rect = palette::palette_rect(area, v.entries().len().max(1));
    let inner = palette::palette_inner(rect);
    assert_eq!(cursor(&v, &lay, &screens()), Some((inner.x + 2, inner.y)));
    v.insert_palette_text(b"ab");
    let rect = palette::palette_rect(area, v.entries().len().max(1));
    let inner = palette::palette_inner(rect);
    assert_eq!(cursor(&v, &lay, &screens()), Some((inner.x + 4, inner.y)));
    v.palette = None;
    assert_ne!(cursor(&v, &lay, &screens()), Some((inner.x + 4, inner.y)));
}

#[test]
fn session_list_change_resets_the_palette_selection() {
    let mut v = ViewState::new(four(), "");
    query(&mut v, "s");
    v.edit_palette(b"\x1b[B");
    assert_eq!(v.palette.as_ref().unwrap().sel, 1);
    v.sessions_changed(AREA, four()[..2].to_vec());
    assert_eq!(v.palette.as_ref().unwrap().sel, 0);
}

#[test]
fn repeated_escape_closes_the_palette_but_sequences_do_not() {
    for esc in [&b"\x1b\x1b"[..], b"\x1b\x1b\x1b"] {
        let mut v = palette_view();
        assert_eq!(v.edit_palette(esc), PaletteEdit::Close);
        assert!(v.palette.is_none());
    }
    for seq in [&b"\x1b[A"[..], b"\x1bOA", b"\x1b[1;2C"] {
        let mut v = palette_view();
        assert_eq!(v.edit_palette(seq), PaletteEdit::Editing);
        assert!(v.palette.is_some());
    }
}

#[test]
fn drag_and_release_around_the_palette_never_reach_a_pane() {
    let mut v = ViewState::new(four(), "");
    open_palette(&mut v);
    let (rect, _) = palette_geometry(&v);
    let info = mouse_info(sgr_mode(MouseLevel::Any), 0);
    v.on_mouse(AREA, ev(0, rect.x + 5, rect.y + 1, false), &info);
    assert_eq!(v.grab.as_deref(), Some(""));
    let out = v.on_mouse(AREA, ev(32, 60, 1, false), &info);
    assert_eq!(out.forward, None);
    let out = v.on_mouse(AREA, ev(0, 60, 1, true), &info);
    assert_eq!(out.forward, None);
    assert_eq!(v.grab, None);
}

#[test]
fn opening_the_palette_drops_a_pane_drag_grab() {
    let mut v = ViewState::new(four(), "");
    v.grab = Some("a".into());
    open_palette(&mut v);
    assert_eq!(v.grab, None);
}

#[test]
fn palette_with_a_very_long_query_keeps_the_caret_inside_the_box() {
    for w in [8u16, 12, 20, 30] {
        let mut v = grid(w);
        open_palette(&mut v);
        v.insert_palette_text("字x".repeat(40).as_bytes());
        let area = Rect::new(0, 0, w, 10);
        let mut term = Terminal::new(TestBackend::new(w, 10)).unwrap();
        term.draw(|f| render(f, &v, &Screens::default())).unwrap();
        let lay = layout(area, &v);
        let rect = palette::palette_rect(
            Rect::new(lay.status.x, 0, lay.status.width, lay.status.bottom()),
            v.entries().len().max(1),
        );
        let inner = palette::palette_inner(rect);
        if let Some((x, y)) = cursor(&v, &lay, &Screens::default()) {
            assert!(
                x >= inner.x && x < inner.right() && y == inner.y,
                "w={w} x={x}"
            );
            let tail = palette::query_tail(
                &v.palette.as_ref().unwrap().query,
                palette::query_avail(inner),
            );
            assert!(
                unicode_width::UnicodeWidthStr::width(tail) + 2 < usize::from(inner.width).max(3)
            );
        }
    }
}

fn wide_view() -> ViewState {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    v
}

fn press(v: &mut ViewState, col: u16, row: u16) -> MouseOut {
    v.on_mouse(
        AREA,
        ev(0, col, row, false),
        &mouse_info(MouseMode::default(), 0),
    )
}

#[test]
fn ctrl_bracket_c_is_new_session_and_action_keys_are_unique() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(&[0x1d, b'c']), vec![Action::Cmd(Cmd::NewSession)]);
    for (i, a) in ACTIONS.iter().enumerate() {
        for b in &ACTIONS[i + 1..] {
            let clash = !a.keys.is_empty()
                && a.keys.iter().any(|k| b.keys.contains(k))
                && !(a.cmd == b.cmd);
            assert!(!clash, "{:?} vs {:?}", a.cmd, b.cmd);
        }
    }
}

#[test]
fn sidebar_layout_pins_the_new_row_to_the_last_line() {
    let v = wide_view();
    let sl = v.sidebar_layout(r(0, 0, 24, 20));
    assert_eq!(sl.new_row, Some(19));
    let ys: Vec<u16> = sl.rows().iter().map(|(_, y)| *y).collect();
    assert_eq!(ys, [1, 2, 3, 4]);
    assert_eq!(sl.rows()[0].0, "c");
    let small = v.sidebar_layout(r(0, 0, 24, 5));
    assert_eq!(small.new_row, Some(4));
    assert_eq!(small.rows().len(), 3);
    let tiny = v.sidebar_layout(r(0, 0, 24, 3));
    assert_eq!(tiny.new_row, None);
    assert_eq!(tiny.rows().len(), 2);
}

#[test]
fn click_on_the_new_row_asks_for_a_new_session() {
    let mut v = wide_view();
    let sb = layout(AREA, &v).sidebar.unwrap();
    let y = sb.bottom() - 1;
    assert_eq!(v.hit(AREA, 3, y), Hit::NewSession);
    let out = press(&mut v, 3, y);
    assert!(out.new_session);
    assert!(!press(&mut v, 3, 1).new_session);
    assert_eq!(ids(&v), ["c"]);
}

#[test]
fn hovering_the_new_row_redraws() {
    let mut v = wide_view();
    let y = layout(AREA, &v).sidebar.unwrap().bottom() - 1;
    let out = v.on_mouse(
        AREA,
        ev(35, 3, y, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert!(out.redraw);
}

#[test]
fn session_hits_are_limited_to_the_visible_rows() {
    let mut v = wide_view();
    let area = Rect::new(0, 0, 140, 5);
    assert_eq!(v.hit(area, 3, 1), Hit::Session("c".into()));
    assert_eq!(v.hit(area, 3, 2), Hit::Session("a".into()));
    assert_eq!(v.hit(area, 3, 3), Hit::NewSession);
    v.allow_new = false;
    assert_eq!(v.hit(area, 3, 3), Hit::Session("b".into()));
    assert_eq!(v.hit(area, 3, 4), Hit::None);
}

#[test]
fn remote_attach_has_no_new_row() {
    let mut v = wide_view();
    v.allow_new = false;
    let sb = layout(AREA, &v).sidebar.unwrap();
    assert_eq!(v.sidebar_layout(sb).new_row, None);
    assert_eq!(v.hit(AREA, 3, sb.bottom() - 1), Hit::None);
    assert!(!draw(&v, &screens(), 140, 22).contains("+ New session"));
    assert!(!press(&mut v, 3, sb.bottom() - 1).new_session);
}

#[test]
fn sidebar_focus_starts_on_the_focused_session_from_the_leftmost_pane() {
    let mut v = two_pane(140);
    v.focus_pane(0);
    assert_eq!(v.focused_session(), Some("a"));
    assert!(!v.focus_nav(AREA, Nav::Left));
    assert_eq!(v.sidebar_focus, Some(SidebarItem::Session("a".into())));
}

#[test]
fn focus_moves_report_true_and_do_not_enter_the_sidebar() {
    let mut w = two_pane(140);
    assert_eq!(w.focused_session(), Some("b"));
    assert!(w.focus_nav(AREA, Nav::Left));
    assert_eq!(w.focused_session(), Some("a"));
    assert_eq!(w.sidebar_focus, None);
}

#[test]
fn sidebar_focus_is_not_entered_when_hidden_or_zoomed() {
    let mut v = two_pane(140);
    v.focus_pane(0);
    v.apply(Cmd::Zoom);
    v.focus_nav(AREA, Nav::Left);
    assert_eq!(v.sidebar_focus, None);
    v.apply(Cmd::Zoom);
    v.apply(Cmd::Sidebar);
    v.focus_nav(AREA, Nav::Left);
    assert_eq!(v.sidebar_focus, None);
}

#[test]
fn sidebar_focus_starts_on_new_without_sessions() {
    let mut v = ViewState::new(Vec::new(), "local");
    v.width = 140;
    assert!(v.enter_sidebar(AREA));
    assert_eq!(v.sidebar_focus, Some(SidebarItem::New));
    v.allow_new = false;
    assert!(!v.enter_sidebar(AREA));
    assert_eq!(v.sidebar_focus, None);
}

fn focused_sidebar() -> ViewState {
    let mut v = wide_view();
    v.set_initial("a");
    v.focus_nav(AREA, Nav::Left);
    v
}

#[test]
fn sidebar_arrows_move_in_display_order_and_clamp() {
    let mut v = focused_sidebar();
    assert_eq!(v.sidebar_focus, Some(SidebarItem::Session("a".into())));
    assert_eq!(v.edit_sidebar(AREA, b"\x1b[A"), SidebarEdit::Editing);
    assert_eq!(v.sidebar_focus, Some(SidebarItem::Session("c".into())));
    v.edit_sidebar(AREA, b"\x1b[A");
    assert_eq!(v.sidebar_focus, Some(SidebarItem::Session("c".into())));
    for keys in [&b"\x1b[B"[..], b"\x1bOB", b"\x0e"] {
        v.edit_sidebar(AREA, keys);
    }
    assert_eq!(v.sidebar_focus, Some(SidebarItem::Session("d".into())));
    v.edit_sidebar(AREA, b"j");
    assert_eq!(v.sidebar_focus, Some(SidebarItem::New));
    v.edit_sidebar(AREA, b"\x1b[B");
    assert_eq!(v.sidebar_focus, Some(SidebarItem::New));
    for keys in [&b"\x1b[A"[..], b"\x1bOA", b"\x10", b"k"] {
        v.edit_sidebar(AREA, keys);
    }
    assert_eq!(v.sidebar_focus, Some(SidebarItem::Session("c".into())));
}

#[test]
fn enter_on_a_session_selects_it_and_clears_the_focus() {
    let mut v = focused_sidebar();
    v.edit_sidebar(AREA, b"\x1b[B");
    assert_eq!(v.edit_sidebar(AREA, b"\r"), SidebarEdit::Select("b".into()));
    assert_eq!(v.sidebar_focus, None);
    assert_eq!(ids(&v), ["b"]);
}

#[test]
fn enter_on_new_asks_for_a_new_session() {
    let mut v = focused_sidebar();
    assert_eq!(
        v.edit_sidebar(AREA, b"\x1b[B\x1b[B\x1b[B\x1b[B\n"),
        SidebarEdit::New
    );
    assert_eq!(v.sidebar_focus, None);
}

#[test]
fn several_keys_in_one_chunk_are_all_handled() {
    let mut v = focused_sidebar();
    assert_eq!(
        v.edit_sidebar(AREA, b"\x1b[B\x1b[B\r"),
        SidebarEdit::Select("d".into())
    );
}

#[test]
fn escape_and_right_leave_the_sidebar() {
    for keys in [&b"\x1b"[..], b"\x1b\x1b", b"\x1b[C", b"\x1bOC"] {
        let mut v = focused_sidebar();
        assert_eq!(v.edit_sidebar(AREA, keys), SidebarEdit::Leave);
        assert_eq!(v.sidebar_focus, None);
    }
}

#[test]
fn other_bytes_are_swallowed() {
    let mut v = focused_sidebar();
    assert_eq!(
        v.edit_sidebar(AREA, b"hello\x03\x7f\x1b[1;2D\x1b[5~"),
        SidebarEdit::Editing
    );
    assert_eq!(v.sidebar_focus, Some(SidebarItem::Session("a".into())));
    assert_eq!(v.edit_sidebar(AREA, b"\x1bO"), SidebarEdit::Editing);
    assert_eq!(v.edit_sidebar(AREA, b"A"), SidebarEdit::Editing);
    assert_eq!(v.sidebar_focus, Some(SidebarItem::Session("a".into())));
}

#[test]
fn sidebar_focus_follows_the_item_not_the_index() {
    let mut v = focused_sidebar();
    v.edit_sidebar(AREA, b"\x1b[B");
    assert_eq!(v.sidebar_focus, Some(SidebarItem::Session("b".into())));
    let mut rows = four();
    rows[1].state = st(StateKind::Blocked, None);
    v.sessions_changed(AREA, rows);
    assert_eq!(v.sidebar_focus, Some(SidebarItem::Session("b".into())));
}

#[test]
fn sidebar_focus_falls_to_a_neighbour_when_its_session_vanishes() {
    let mut v = focused_sidebar();
    v.edit_sidebar(AREA, b"\x1b[B");
    let rows: Vec<SessionRow> = four().into_iter().filter(|r| r.id != "a").collect();
    v.sessions_changed(AREA, rows);
    assert_eq!(v.sidebar_focus, Some(SidebarItem::Session("b".into())));
    v.sessions_changed(AREA, Vec::new());
    assert_eq!(v.sidebar_focus, Some(SidebarItem::New));
    v.allow_new = false;
    v.sessions_changed(AREA, Vec::new());
    assert_eq!(v.sidebar_focus, None);
}

#[test]
fn sidebar_focus_clears_when_the_sidebar_goes_away() {
    let mut v = focused_sidebar();
    v.apply(Cmd::Zoom);
    assert_eq!(v.sidebar_focus, None);
    let mut w = focused_sidebar();
    w.sidebar = Some(false);
    w.sessions_changed(AREA, four());
    assert_eq!(w.sidebar_focus, None);
}

#[test]
fn a_left_press_clears_the_sidebar_focus_but_the_wheel_does_not() {
    let mut v = focused_sidebar();
    v.on_mouse(
        AREA,
        ev(64, 3, 2, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert!(v.sidebar_focus.is_some());
    let out = press(&mut v, 60, 5);
    assert_eq!(v.sidebar_focus, None);
    assert!(out.redraw);
}

#[test]
fn focused_sidebar_hides_the_cursor_and_shows_a_hint() {
    let mut v = focused_sidebar();
    let s = screens();
    let lay = layout(AREA, &v);
    v.sidebar_focus = None;
    assert!(cursor(&v, &lay, &s).is_some());
    v.focus_nav(AREA, Nav::Left);
    assert_eq!(cursor(&v, &lay, &s), None);
    assert!(draw(&v, &s, 140, 22).contains("sidebar: Up/Down move, Enter opens, Esc/Right back"));
}

#[test]
fn snapshot_sidebar_new_row() {
    let v = two_pane(140);
    assert_snapshot("sidebar_new_row", &draw(&v, &screens(), 140, 12));
}

#[test]
fn snapshot_sidebar_focus_new() {
    let mut v = focused_sidebar();
    v.edit_sidebar(AREA, b"\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B");
    assert_eq!(v.sidebar_focus, Some(SidebarItem::New));
    assert_snapshot("sidebar_focus_new", &draw(&v, &screens(), 140, 12));
}

#[test]
fn palette_lists_and_ranks_new_session() {
    let mut v = ViewState::new(Vec::new(), "");
    open_palette(&mut v);
    assert!(v
        .entries()
        .iter()
        .any(|r| r.entry == PaletteEntry::Cmd(Cmd::NewSession)));
    let mut w = ViewState::new(Vec::new(), "");
    query(&mut w, "new session");
    assert_eq!(first(&w), PaletteEntry::Cmd(Cmd::NewSession));
}

fn many_sessions(n: usize) -> Vec<SessionRow> {
    (0..n)
        .map(|i| {
            let id = format!("s{i}");
            row(&id, &id, StateKind::Working, "1m")
        })
        .collect()
}

#[test]
fn keyboard_navigation_never_lands_on_an_undrawn_row() {
    let mut v = ViewState::new(many_sessions(30), "local");
    v.width = 140;
    let area = r(0, 0, 140, 8);
    let drawn = v.sidebar_layout(layout(area, &v).sidebar.unwrap());
    assert!(drawn.rows().len() < 30);
    assert!(v.enter_sidebar(area));
    for _ in 0..50 {
        v.edit_sidebar(area, b"\x1b[B");
        match v.sidebar_focus.clone().unwrap() {
            SidebarItem::Group(_) | SidebarItem::Tab(_) => unreachable!(),
            SidebarItem::Session(id) => assert!(drawn.rows().iter().any(|(r, _)| *r == id)),
            SidebarItem::New => assert!(drawn.new_row.is_some()),
        }
    }
    assert_eq!(v.sidebar_focus, Some(SidebarItem::New));
}

#[test]
fn a_tiny_sidebar_cannot_focus_new() {
    let mut v = ViewState::new(many_sessions(2), "local");
    v.width = 140;
    let area = r(0, 0, 140, 4);
    assert!(layout(area, &v).sidebar.unwrap().height < 4);
    assert!(v.enter_sidebar(area));
    for _ in 0..5 {
        v.edit_sidebar(area, b"j");
        assert_ne!(v.sidebar_focus, Some(SidebarItem::New));
    }
    assert!(!v.sidebar_items(area).contains(&SidebarItem::New));
    assert_eq!(
        v.edit_sidebar(area, b"\r"),
        SidebarEdit::Select("s1".into())
    );
}

fn loc(root: &str, dir: &str, branch: Option<&str>) -> Loc {
    Loc {
        root: root.into(),
        dir: dir.into(),
        branch: branch.map(Into::into),
    }
}

fn six() -> Vec<SessionRow> {
    ["a", "b", "c", "d", "e", "f"]
        .iter()
        .map(|id| row(id, id, StateKind::Working, "1m"))
        .collect()
}

fn grouped_view() -> ViewState {
    let mut v = ViewState::new(six(), "local");
    v.width = 140;
    v.grouped = true;
    v.set_locs(
        AREA,
        vec![
            ("a".into(), loc("/w/app", "/w/app", Some("main"))),
            ("b".into(), loc("/w/app", "/w/app/src", Some("main"))),
            ("c".into(), loc("/w/app-wt", "/w/app-wt", Some("feat/x"))),
            ("d".into(), loc("/w/lib", "/w/lib", None)),
            ("e".into(), loc("/w/lib", "/w/lib", None)),
        ],
    );
    v
}

fn shape(v: &ViewState) -> Vec<String> {
    let sb = layout(AREA, v).sidebar.unwrap();
    v.sidebar_layout(sb)
        .lines
        .into_iter()
        .map(|(l, _)| match l {
            SidebarLine::Header {
                key,
                count,
                collapsed,
            } => format!("H {key} {count} {collapsed}"),
            SidebarLine::Tab { id, count } => format!("T {id} {count}"),
            SidebarLine::Session { id, indent } => {
                format!("{}{id}", if indent { "  " } else { "" })
            }
            SidebarLine::Detail(id) => format!("~{id}"),
            SidebarLine::Activity(id) => format!("^{id}"),
            SidebarLine::Sub { id, .. } => format!("*{id}"),
        })
        .collect()
}

fn sidebar_y(v: &ViewState, want: &SidebarLine) -> u16 {
    let sb = layout(AREA, v).sidebar.unwrap();
    v.sidebar_layout(sb)
        .lines
        .into_iter()
        .find(|(l, _)| l == want)
        .unwrap()
        .1
}

#[test]
fn sessions_in_one_repo_share_a_header_and_singles_get_a_detail_line() {
    let v = grouped_view();
    assert_eq!(
        shape(&v),
        vec![
            "H /w/app 2 false",
            "  a",
            "  b",
            "c",
            "~c",
            "H /w/lib 2 false",
            "  d",
            "  e",
            "f",
        ]
    );
}

#[test]
fn a_session_changing_folder_moves_groups_without_reordering_them() {
    let mut v = grouped_view();
    v.set_locs(AREA, vec![("a".into(), loc("/w/lib", "/w/lib", None))]);
    assert_eq!(
        shape(&v),
        vec![
            "b",
            "~b",
            "c",
            "~c",
            "H /w/lib 3 false",
            "  a",
            "  d",
            "  e",
            "f",
        ],
    );
}

#[test]
fn an_emptied_group_keeps_its_place_when_it_fills_again() {
    let mut v = grouped_view();
    v.set_locs(
        AREA,
        vec![
            ("a".into(), loc("/w/lib", "/w/lib", None)),
            ("b".into(), loc("/w/lib", "/w/lib", None)),
        ],
    );
    v.set_locs(
        AREA,
        vec![("a".into(), loc("/w/app", "/w/app", Some("main")))],
    );
    v.set_locs(
        AREA,
        vec![("b".into(), loc("/w/app", "/w/app", Some("main")))],
    );
    assert_eq!(shape(&v)[0], "H /w/app 2 false");
}

#[test]
fn enter_on_a_header_folds_it_and_keeps_the_focus_there() {
    let mut v = grouped_view();
    assert!(v.enter_sidebar(AREA));
    v.sidebar_focus = Some(SidebarItem::Group("/w/app".into()));
    assert_eq!(v.edit_sidebar(AREA, b"\r"), SidebarEdit::Editing);
    assert_eq!(v.sidebar_focus, Some(SidebarItem::Group("/w/app".into())));
    assert_eq!(shape(&v)[0], "H /w/app 2 true");
    assert_eq!(shape(&v)[1], "c");
    v.edit_sidebar(AREA, b"\r");
    assert_eq!(shape(&v)[0], "H /w/app 2 false");
}

#[test]
fn clicking_a_header_folds_and_unfolds_it() {
    let mut v = grouped_view();
    let y = sidebar_y(
        &v,
        &SidebarLine::Header {
            key: "/w/app".into(),
            count: 2,
            collapsed: false,
        },
    );
    let out = v.on_mouse(
        AREA,
        ev(0, 3, y, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert!(out.redraw);
    assert_eq!(shape(&v)[0], "H /w/app 2 true");
    v.on_mouse(
        AREA,
        ev(0, 3, y, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert_eq!(shape(&v)[0], "H /w/app 2 false");
}

#[test]
fn the_group_toggle_flattens_and_restores_the_list() {
    let mut v = grouped_view();
    v.apply(Cmd::Group);
    assert!(shape(&v).iter().all(|l| !l.starts_with('H')));
    v.apply(Cmd::Group);
    assert!(shape(&v)[0].starts_with('H'));
}

#[test]
fn next_and_previous_follow_the_grouped_order_and_skip_folded_groups() {
    let mut v = grouped_view();
    v.set_locs(AREA, vec![("a".into(), loc("/w/lib", "/w/lib", None))]);
    v.collapsed.insert("/w/lib".into());
    v.set_initial("b");
    v.apply(Cmd::NextSession);
    assert_eq!(ids(&v), vec!["c"]);
    v.apply(Cmd::NextSession);
    assert_eq!(ids(&v), vec!["f"]);
    v.apply(Cmd::PrevSession);
    assert_eq!(ids(&v), vec!["c"]);
}

#[test]
fn next_session_reaches_folded_sessions_when_nothing_else_is_free() {
    let mut v = ViewState::new(six()[..2].to_vec(), "local");
    v.grouped = true;
    v.width = 140;
    let same = loc("/w/app", "/w/app", None);
    v.set_locs(AREA, vec![("a".into(), same.clone()), ("b".into(), same)]);
    v.collapsed.insert("/w/app".into());
    v.set_initial("a");
    v.apply(Cmd::NextSession);
    assert_eq!(ids(&v), vec!["b"]);
}

#[test]
fn focus_moves_on_when_its_header_disappears() {
    let mut v = grouped_view();
    v.sidebar_focus = Some(SidebarItem::Group("/w/lib".into()));
    v.set_locs(AREA, vec![("e".into(), loc("/w/other", "/w/other", None))]);
    assert_eq!(v.sidebar_focus, Some(SidebarItem::Session("d".into())));
}

#[test]
fn a_resumed_session_inherits_the_folder_of_the_one_it_replaces() {
    let mut v = grouped_view();
    let mut rows = six();
    rows.retain(|r| r.id != "a");
    let mut resumed = row("a2", "a", StateKind::Working, "1s");
    resumed.resumed_from = Some("a".into());
    rows.insert(0, resumed);
    v.sessions_changed(AREA, rows);
    assert_eq!(v.locs.get("a2").map(|l| l.root.as_str()), Some("/w/app"));
    assert_eq!(shape(&v)[0], "H /w/app 2 false");
}

#[test]
fn details_are_dropped_before_rows_when_the_sidebar_is_short() {
    let mut v = grouped_view();
    v.allow_new = false;
    let area = r(0, 0, 140, 8);
    let sb = layout(area, &v).sidebar.unwrap();
    let lines = v.sidebar_layout(sb).lines;
    assert!(lines
        .iter()
        .all(|(l, _)| !matches!(l, SidebarLine::Detail(_))));
}

#[test]
fn snapshot_grouped_sidebar() {
    let v = grouped_view();
    assert_snapshot("grouped", &draw(&v, &screens(), 140, 22));
}

#[test]
fn snapshot_grouped_sidebar_folded() {
    let mut v = grouped_view();
    v.collapsed.insert("/w/app".into());
    assert_snapshot("grouped_folded", &draw(&v, &screens(), 140, 22));
}

#[test]
fn crlf_enter_on_a_header_folds_it_once() {
    let mut v = grouped_view();
    v.sidebar_focus = Some(SidebarItem::Group("/w/app".into()));
    v.edit_sidebar(AREA, b"\r\n");
    assert_eq!(shape(&v)[0], "H /w/app 2 true");
}

#[test]
fn folding_a_header_with_the_click_moves_focus_off_its_hidden_rows() {
    let mut v = grouped_view();
    v.sidebar_focus = Some(SidebarItem::Session("a".into()));
    let y = sidebar_y(
        &v,
        &SidebarLine::Header {
            key: "/w/app".into(),
            count: 2,
            collapsed: false,
        },
    );
    v.on_mouse(
        AREA,
        ev(0, 3, y, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert!(
        v.sidebar_focus.is_none()
            || v.sidebar_items(AREA)
                .contains(v.sidebar_focus.as_ref().unwrap())
    );
}

fn titled(id: &str, text: &str, auto: bool, base: &str) -> (String, AgentTitle) {
    (
        id.into(),
        AgentTitle {
            text: text.into(),
            auto,
            base: base.into(),
        },
    )
}

fn grab(v: &ViewState, w: u16, h: u16) -> (String, ratatui::buffer::Buffer) {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| render(f, v, &Screens::default())).unwrap();
    (text(&term), term.backend().buffer().clone())
}

#[test]
fn agent_title_replaces_the_name_in_sidebar_and_pane_header() {
    let mut v = two_pane(140);
    v.titles
        .extend([titled("a", "Fix checkout", false, "claude")]);
    let (out, _) = grab(&v, 140, 12);
    let lines: Vec<&str> = out.lines().collect();
    assert!(
        lines[0].contains("1  Fix checkout · working"),
        "{}",
        lines[0]
    );
    assert!(lines[3].contains("Fix check…"), "{}", lines[3]);
    assert!(!lines[3].contains("claude"), "{}", lines[3]);
    assert!(lines[3].contains("wo… 18m"), "{}", lines[3]);
    assert!(out.contains("tmgr runner · local · dev-server · waiting"));
}

#[test]
fn auto_titles_are_dim_and_user_titles_are_not() {
    let mut v = ViewState::new(four(), "local");
    v.width = 100;
    v.titles.extend([titled("a", "Auto title", true, "claude")]);
    let (_, buf) = grab(&v, 100, 12);
    let sidebar_row = (0..12u16)
        .find(|y| {
            (0..24u16)
                .map(|x| buf[(x, *y)].symbol())
                .collect::<String>()
                .contains("Auto")
        })
        .unwrap();
    let x = (0..24u16)
        .find(|x| buf[(*x, sidebar_row)].symbol() == "A")
        .unwrap();
    assert_eq!(buf[(x, sidebar_row)].fg, ratatui::style::Color::DarkGray);
    v.titles.extend([titled("a", "Mine", false, "claude")]);
    let (_, buf) = grab(&v, 100, 12);
    let x = (0..24u16)
        .find(|x| buf[(*x, sidebar_row)].symbol() == "M")
        .unwrap();
    assert_ne!(buf[(x, sidebar_row)].fg, ratatui::style::Color::DarkGray);
}

#[test]
fn a_renamed_session_ignores_the_agent_title() {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    v.titles
        .extend([titled("a", "Agent says", false, "claude")]);
    v.sessions[0].name = "pinned".into();
    let (out, _) = grab(&v, 140, 12);
    assert!(out.contains("pinned"));
    assert!(!out.contains("Agent says"));
}

#[test]
fn set_titles_reports_changes_and_prunes_vanished_sessions() {
    let mut v = ViewState::new(four(), "local");
    let t = titled("a", "x", false, "claude").1;
    assert!(v.set_titles(vec![("a".into(), Some(t.clone()))]));
    assert!(!v.set_titles(vec![("a".into(), Some(t))]));
    assert!(!v.set_titles(vec![("zz".into(), None)]));
    assert!(v.set_titles(vec![("a".into(), None)]));
    assert!(v.titles.is_empty());
    v.titles.extend([titled("a", "x", false, "claude")]);
    v.sessions_changed(AREA, vec![row("b", "dev-server", StateKind::Waiting, "1m")]);
    assert!(v.titles.is_empty());
}

#[test]
fn long_agent_title_stays_clear_of_the_header_buttons() {
    let mut v = two_pane(140);
    let long =
        "Codex Companion Task: <task> Adversarial security review read-only do NOT modify files";
    v.titles.extend([titled("a", long, false, "claude")]);
    let (out, _) = grab(&v, 140, 12);
    let head = out.lines().next().unwrap();
    assert!(head.contains("… · working ─"), "{head}");
    assert!(head.contains("⏸ ■ ✕ ╮"), "{head}");
}

#[test]
fn sidebar_keeps_the_age_and_a_readable_name_at_24_columns() {
    let mut v = ViewState::new(vec![row("a", "claude", StateKind::Working, "15s")], "local");
    v.width = 100;
    v.titles
        .extend([titled("a", "Refactor the billing module", false, "claude")]);
    let (out, _) = grab(&v, 100, 6);
    let line = out.lines().nth(1).unwrap();
    assert!(line.starts_with(" ● Refactor … work… 15s"), "{line}");
    v.sessions[0].age = "15s".into();
    v.sessions[0].state.kind = StateKind::Blocked;
    let (out, _) = grab(&v, 100, 6);
    assert!(out.lines().nth(1).unwrap().contains("bloc… 15s"));
}

#[test]
fn scroll_indicator_keeps_the_header_buttons_clear_of_a_long_title() {
    let mut v = two_pane(140);
    let long = "Codex Companion Task: Adversarial security review read-only do NOT modify files";
    v.titles.extend([titled("b", long, false, "dev-server")]);
    v.scroll = Some(ScrollState {
        offset: 120,
        max: 9000,
    });
    let (out, _) = grab(&v, 140, 12);
    let head = out.lines().next().unwrap();
    assert!(head.contains("… · waiting · scroll -120/9000 ─"), "{head}");
    assert!(head.contains("⏸ ■ ✕ ╮"), "{head}");
}

#[test]
fn focus_reports_are_actions_and_never_forwarded() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"\x1b[I"), vec![Action::FocusIn]);
    assert_eq!(s.feed(b"\x1b[O"), vec![Action::FocusOut]);
    assert_eq!(
        s.feed(b"ab\x1b[Icd\x1b[O"),
        vec![
            Action::Forward(b"ab".to_vec()),
            Action::FocusIn,
            Action::Forward(b"cd".to_vec()),
            Action::FocusOut
        ]
    );
}

#[test]
fn focus_reports_split_across_reads_are_held() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"\x1b"), vec![]);
    assert_eq!(s.feed(b"["), vec![]);
    assert_eq!(s.feed(b"O"), vec![Action::FocusOut]);
    assert_eq!(s.feed(b"x\x1b["), vec![Action::Forward(b"x".to_vec())]);
    assert_eq!(s.feed(b"I"), vec![Action::FocusIn]);
}

#[test]
fn focus_reports_do_not_disturb_the_prefix_or_mouse() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"\x1d\x1b[I"), vec![Action::FocusIn]);
    assert_eq!(s.feed(b"d"), vec![Action::Cmd(Cmd::Detach)]);
    assert_eq!(s.feed(b"\x1b[<64;3;4M\x1b[I").len(), 2);
}

#[test]
fn focus_bytes_inside_a_paste_stay_paste() {
    let mut s = InputSplitter::new();
    let got = s.feed(b"\x1b[200~a\x1b[Ib\x1b[201~");
    assert_eq!(
        got,
        vec![Action::Paste {
            data: b"\x1b[200~a\x1b[Ib\x1b[201~".to_vec(),
            start: true,
            end: true
        }]
    );
}

#[test]
fn focus_reporting_is_enabled_and_always_reset() {
    assert_eq!(focus_on(), b"\x1b[?1004h");
    let reset = reset_modes();
    assert!(reset.windows(8).any(|w| w == b"\x1b[?1004l"));
}

fn you_rows(you: Option<u64>) -> ViewState {
    let mut r = row("a", "a", StateKind::Waiting, "2s");
    r.you = you;
    let mut v = ViewState::new(vec![r], "");
    v.width = 140;
    v
}

fn sidebar_row(v: &ViewState) -> String {
    draw(v, &Screens::default(), 140, 6)
        .lines()
        .nth(1)
        .unwrap()
        .chars()
        .take(24)
        .collect()
}

#[test]
fn sidebar_row_shows_human_time_before_the_state() {
    let line = sidebar_row(&you_rows(Some(12 * 60 + 5)));
    assert!(line.contains("you 12m waiting 2s"), "{line}");
}

#[test]
fn sidebar_row_formats_hours() {
    assert_eq!(render::you_label(3900).as_deref(), Some("you 1h 5m"));
    assert_eq!(render::you_label(60).as_deref(), Some("you 1m"));
    assert_eq!(render::you_label(59), None);
    assert!(sidebar_row(&you_rows(Some(7200))).contains("you 2h waiting 2s"));
}

#[test]
fn sidebar_row_hides_human_time_under_a_minute_or_when_absent() {
    let plain = sidebar_row(&you_rows(None));
    assert!(!plain.contains("you"));
    assert_eq!(sidebar_row(&you_rows(Some(59))), plain);
}

fn named_row(name: &str, age: &str, you: Option<u64>) -> String {
    let mut r = row("a", name, StateKind::Waiting, age);
    r.you = you;
    let mut v = ViewState::new(vec![r], "");
    v.width = 140;
    sidebar_row(&v)
}

#[test]
fn sidebar_row_keeps_human_time_and_shortens_the_state_to_fit() {
    let line = named_row("api", "2s", Some(720));
    assert!(line.contains("you 12m wait… 2s"), "{line}");
    let line = named_row("dev", "12m", Some(720));
    assert!(line.contains("you 12m wai… 12m"), "{line}");
    let line = named_row("my-agent", "2s", Some(720));
    assert!(line.contains("you 12m 2s"), "{line}");
}

#[test]
fn sidebar_row_shows_everything_when_it_all_fits() {
    let line = named_row("a", "2s", Some(720));
    assert!(line.contains("you 12m waiting 2s"), "{line}");
}

#[test]
fn sidebar_row_drops_human_time_when_the_age_would_not_fit() {
    let plain = named_row("dev-server", "12m", None);
    assert_eq!(named_row("dev-server", "12m", Some(720)), plain);
    assert!(plain.contains("waiting 12m") || plain.contains("12m"));
    let line = named_row("dev-server", "2s", Some(720));
    assert!(line.contains("2s"), "{line}");
}

#[test]
fn sidebar_row_never_cuts_the_age() {
    for name in ["a", "api", "dev", "dev-server", "a-very-long-session-name"] {
        for age in ["2s", "12m", "3h"] {
            let line = named_row(name, age, Some(720));
            assert!(line.trim_end().ends_with(age), "{name} {age}: {line}");
        }
    }
}

fn tab_id(v: &ViewState, session: &str) -> u64 {
    match v.item_of(session) {
        TabItem::Tab(t) => t,
        TabItem::Session(s) => panic!("{s} is not in a tab"),
    }
}

fn tab_line(v: &ViewState, session: &str) -> SidebarLine {
    SidebarLine::Tab {
        id: tab_id(v, session),
        count: v.tab_members(tab_id(v, session)).len(),
    }
}

fn session_line(id: &str) -> SidebarLine {
    SidebarLine::Session {
        id: id.into(),
        indent: true,
    }
}

fn click_line(v: &mut ViewState, line: &SidebarLine) {
    let y = sidebar_y(v, line);
    press(v, 3, y);
}

fn tabbed() -> ViewState {
    let mut v = ViewState::new(six(), "local");
    v.width = 140;
    v.apply(Cmd::Split);
    v.open("c");
    v.apply(Cmd::Split);
    v
}

fn flat(n: usize) -> ViewState {
    let mut v = ViewState::new(six()[..n].to_vec(), "local");
    v.width = 140;
    v
}

#[test]
fn a_split_makes_a_tab_header_with_indented_members() {
    let v = two_pane(140);
    let t = tab_id(&v, "a");
    assert_eq!(v.item_of("b"), TabItem::Tab(t));
    assert_eq!(v.item_of("d"), TabItem::Session("d".into()));
    assert_eq!(v.tab_count(), 1);
    assert_eq!(
        shape(&v),
        vec![
            "c".to_string(),
            format!("T {t} 2"),
            "  a".into(),
            "  b".into(),
            "d".into()
        ]
    );
}

#[test]
fn one_pane_sessions_are_plain_rows() {
    let v = flat(3);
    assert_eq!(shape(&v), vec!["a", "b", "c"]);
    assert_eq!(v.tab_count(), 0);
    assert_eq!(v.active_tab(), None);
}

#[test]
fn tabs_view_lists_the_stashed_tab_the_active_tab_and_free_sessions() {
    let v = tabbed();
    let (t0, t1) = (tab_id(&v, "a"), tab_id(&v, "c"));
    assert_ne!(t0, t1);
    assert_eq!(v.tab_count(), 2);
    assert_eq!(
        shape(&v),
        vec![
            format!("T {t0} 2"),
            "  a".into(),
            "  b".into(),
            format!("T {t1} 2"),
            "  c".into(),
            "  d".into(),
            "e".into(),
            "f".into(),
        ]
    );
}

#[test]
fn clicking_a_free_session_shows_it_alone_and_stashes_the_split_tab() {
    let mut v = tabbed();
    click_line(
        &mut v,
        &SidebarLine::Session {
            id: "e".into(),
            indent: false,
        },
    );
    assert_eq!(ids(&v), ["e"]);
    assert_eq!(v.active_tab(), None);
    assert_eq!(v.tab_count(), 2);
    assert_eq!(v.tab_members(tab_id(&v, "c")), ["c", "d"]);
    assert_eq!(v.tab_members(tab_id(&v, "a")), ["a", "b"]);
}

#[test]
fn clicking_a_tab_header_restores_its_layout_and_focus() {
    let mut v = tabbed();
    v.focus_pane(0);
    assert_eq!(v.focused_session(), Some("c"));
    click_line(
        &mut v,
        &SidebarLine::Session {
            id: "e".into(),
            indent: false,
        },
    );
    let header = tab_line(&v, "c");
    click_line(&mut v, &header);
    assert_eq!(ids(&v), ["c", "d"]);
    assert_eq!(v.focused_session(), Some("c"));
    assert_eq!(v.active_tab(), Some(tab_id(&v, "c")));
}

#[test]
fn clicking_a_session_inside_a_stashed_tab_switches_and_focuses_that_pane() {
    let mut v = tabbed();
    click_line(&mut v, &session_line("b"));
    assert_eq!(ids(&v), ["a", "b"]);
    assert_eq!(v.focused_session(), Some("b"));
    assert_eq!(v.tab_members(tab_id(&v, "c")), ["c", "d"]);
    click_line(&mut v, &session_line("c"));
    assert_eq!(ids(&v), ["c", "d"]);
    assert_eq!(v.focused_session(), Some("c"));
}

#[test]
fn clicking_a_session_of_the_active_tab_only_moves_focus() {
    let mut v = tabbed();
    click_line(&mut v, &session_line("c"));
    assert_eq!(ids(&v), ["c", "d"]);
    assert_eq!(v.focused_session(), Some("c"));
    assert_eq!(v.tab_count(), 2);
}

#[test]
fn next_and_previous_session_never_pull_a_session_out_of_a_stashed_tab() {
    let mut v = tabbed();
    for _ in 0..4 {
        v.apply(Cmd::NextSession);
        assert!(
            v.panes().iter().all(|p| p != "a" && p != "b"),
            "{:?}",
            ids(&v)
        );
    }
    assert_eq!(v.tab_members(tab_id(&v, "a")), ["a", "b"]);
    let mut one = tabbed();
    one.apply(Cmd::Split);
    one.apply(Cmd::Split);
    assert!(one.panes().iter().all(|p| p != "a" && p != "b"));
    assert_eq!(one.tab_members(tab_id(&one, "a")), ["a", "b"]);
}

#[test]
fn split_command_skips_sessions_of_a_stashed_tab() {
    let mut v = tabbed();
    click_line(
        &mut v,
        &SidebarLine::Session {
            id: "e".into(),
            indent: false,
        },
    );
    v.apply(Cmd::Split);
    assert_eq!(ids(&v), ["e", "f"]);
    assert_eq!(v.tab_count(), 3);
}

#[test]
fn split_at_a_stashed_session_removes_it_from_the_stash_and_dissolves_the_rest() {
    let mut v = tabbed();
    let stashed = tab_id(&v, "a");
    assert!(v.split_at("c", SplitDir::Right, "a", true));
    assert_eq!(ids(&v), ["c", "a", "d"]);
    assert_eq!(v.item_of("b"), TabItem::Session("b".into()));
    assert_eq!(v.tab_count(), 1);
    assert!(v.tab_members(stashed).is_empty());
    assert_eq!(shape(&v)[0].split(' ').next(), Some("T"));
}

#[test]
fn select_of_a_stashed_session_takes_it_out_of_the_stash_and_dissolves_the_leftover() {
    let mut v = tabbed();
    v.select("a");
    assert_eq!(v.focused_session(), Some("a"));
    assert!(v.panes().contains(&"a".to_string()));
    assert_eq!(v.item_of("b"), TabItem::Session("b".into()));
    assert_eq!(v.tab_count(), 1);
}

#[test]
fn next_tab_and_previous_tab_cycle_in_sidebar_order_and_wrap() {
    let mut v = tabbed();
    assert_eq!(v.focused_session(), Some("d"));
    v.apply(Cmd::NextTab);
    assert_eq!(ids(&v), ["e"]);
    v.apply(Cmd::NextTab);
    assert_eq!(ids(&v), ["f"]);
    v.apply(Cmd::NextTab);
    assert_eq!(ids(&v), ["a", "b"]);
    v.apply(Cmd::NextTab);
    assert_eq!(ids(&v), ["c", "d"]);
    v.apply(Cmd::PrevTab);
    assert_eq!(ids(&v), ["a", "b"]);
    v.apply(Cmd::PrevTab);
    assert_eq!(ids(&v), ["f"]);
    v.apply(Cmd::PrevTab);
    assert_eq!(ids(&v), ["e"]);
}

#[test]
fn tab_stepping_with_one_item_does_nothing() {
    let mut v = flat(1);
    v.apply(Cmd::NextTab);
    v.apply(Cmd::PrevTab);
    assert_eq!(ids(&v), ["a"]);
    let mut empty = ViewState::new(Vec::new(), "");
    empty.apply(Cmd::NextTab);
    empty.apply(Cmd::PrevTab);
    assert!(empty.panes().is_empty());
}

#[test]
fn close_tab_keeps_the_sessions_and_leaves_the_focused_one_alone() {
    let mut v = tabbed();
    v.focus_pane(0);
    v.apply(Cmd::CloseTab);
    assert_eq!(ids(&v), ["c"]);
    assert_eq!(v.focused_session(), Some("c"));
    assert!(v.notice.is_some());
    assert_eq!(v.sessions.len(), 6);
    assert_eq!(v.item_of("d"), TabItem::Session("d".into()));
    assert_eq!(v.tab_count(), 1);
    assert_eq!(v.item_of("a"), TabItem::Tab(tab_id(&v, "a")));
}

#[test]
fn closing_a_tab_drops_its_custom_name() {
    let mut v = two_pane(140);
    v.apply(Cmd::RenameTab);
    v.edit_rename(b"work\r");
    v.apply(Cmd::CloseTab);
    v.apply(Cmd::Split);
    assert_ne!(v.tab_title(v.active_tab().unwrap()), "work");
}

#[test]
fn close_tab_on_a_one_pane_tab_only_sets_a_notice() {
    let mut v = flat(3);
    v.apply(Cmd::CloseTab);
    assert_eq!(ids(&v), ["a"]);
    assert!(v.notice.is_some());
    assert_eq!(v.sessions.len(), 3);
}

#[test]
fn rename_tab_on_a_one_pane_tab_sets_a_notice_and_no_prompt() {
    let mut v = flat(3);
    v.apply(Cmd::RenameTab);
    assert!(v.rename.is_none());
    assert!(v.notice.is_some());
}

#[test]
fn rename_tab_prompts_and_enter_names_the_tab() {
    let mut v = two_pane(140);
    v.apply(Cmd::RenameTab);
    assert!(v.rename.as_ref().unwrap().tab);
    assert_eq!(v.edit_rename(b"x"), RenameEdit::Editing);
    for _ in 0..30 {
        v.edit_rename(b"\x7f");
    }
    assert_eq!(v.edit_rename(b"deploy\r"), RenameEdit::Done);
    assert!(v.rename.is_none());
    let t = tab_id(&v, "a");
    assert_eq!(v.tab_title(t), "deploy");
    assert!(draw(&v, &screens(), 140, 22).contains("deploy"));
}

#[test]
fn renaming_a_tab_does_not_rename_the_session() {
    let mut v = two_pane(140);
    v.apply(Cmd::RenameTab);
    for _ in 0..30 {
        v.edit_rename(b"\x7f");
    }
    v.edit_rename(b"deploy\r");
    assert!(v
        .sessions
        .iter()
        .any(|s| s.id == "b" && s.name == "dev-server"));
    assert!(v.sessions.iter().any(|s| s.id == "a" && s.name == "claude"));
}

#[test]
fn an_empty_tab_name_falls_back_to_the_default_title() {
    let mut v = two_pane(140);
    let t = tab_id(&v, "a");
    let default = v.tab_title(t);
    v.apply(Cmd::RenameTab);
    for _ in 0..30 {
        v.edit_rename(b"\x7f");
    }
    assert_eq!(v.edit_rename(b"\r"), RenameEdit::Done);
    assert_eq!(v.tab_title(t), default);
    v.apply(Cmd::RenameTab);
    for _ in 0..30 {
        v.edit_rename(b"\x7f");
    }
    v.edit_rename(b"   \r");
    assert_eq!(v.tab_title(t), default);
}

#[test]
fn tab_names_strip_control_characters_and_cap_the_length() {
    let mut v = two_pane(140);
    let t = tab_id(&v, "a");
    v.apply(Cmd::RenameTab);
    for _ in 0..30 {
        v.edit_rename(b"\x7f");
    }
    let long = "x".repeat(200);
    v.edit_rename(long.as_bytes());
    v.edit_rename(b"\r");
    assert_eq!(v.tab_title(t).chars().count(), 64);

    let prefs = LayoutPrefs {
        tab_name: Some("a\u{1b}[31mb\u{7}c".into()),
        ..v.prefs()
    };
    let mut fresh = ViewState::new(four(), "local");
    fresh.apply_prefs(&prefs);
    assert_eq!(fresh.tab_title(tab_id(&fresh, "a")), "a[31mbc");
}

#[test]
fn keyboard_sidebar_enter_on_a_tab_header_switches_to_that_tab() {
    let mut v = tabbed();
    v.set_initial("c");
    v.focus_nav(AREA, Nav::Left);
    assert!(v.sidebar_focus.is_some());
    let target = SidebarItem::Tab(tab_id(&v, "a"));
    let mut guard = 0;
    while v.sidebar_focus != Some(target.clone()) && guard < 20 {
        v.edit_sidebar(AREA, b"\x1b[A");
        guard += 1;
    }
    assert_eq!(v.sidebar_focus, Some(target));
    assert_eq!(v.edit_sidebar(AREA, b"\r"), SidebarEdit::Select("b".into()));
    assert_eq!(ids(&v), ["a", "b"]);
    assert_eq!(v.focused_session(), Some("b"));
    assert_eq!(v.sidebar_focus, None);
}

#[test]
fn keyboard_sidebar_visits_tab_headers_between_sessions() {
    let mut v = tabbed();
    v.set_initial("e");
    v.focus_nav(AREA, Nav::Left);
    let mut seen = Vec::new();
    for _ in 0..8 {
        v.edit_sidebar(AREA, b"\x1b[A");
        seen.push(v.sidebar_focus.clone());
    }
    assert!(seen.contains(&Some(SidebarItem::Tab(tab_id(&v, "c")))));
    assert!(seen.contains(&Some(SidebarItem::Tab(tab_id(&v, "a")))));
}

#[test]
fn the_group_command_switches_between_tab_and_repo_grouping() {
    let mut v = ViewState::new(six(), "local");
    v.width = 140;
    v.set_locs(
        AREA,
        vec![
            ("a".into(), loc("/w/app", "/w/app", Some("main"))),
            ("b".into(), loc("/w/app", "/w/app/src", Some("main"))),
            ("c".into(), loc("/w/lib", "/w/lib", None)),
            ("d".into(), loc("/w/lib", "/w/lib", None)),
        ],
    );
    v.apply(Cmd::Split);
    assert!(!v.grouped);
    let by_tab = shape(&v);
    assert!(by_tab[0].starts_with("T "), "{by_tab:?}");
    assert!(by_tab.iter().all(|l| !l.starts_with("H ")));
    v.apply(Cmd::Group);
    assert!(v.grouped);
    let by_repo = shape(&v);
    assert!(by_repo[0].starts_with("H /w/app 2"), "{by_repo:?}");
    assert!(by_repo.iter().all(|l| !l.starts_with("T ")));
    v.apply(Cmd::Group);
    assert_eq!(shape(&v), by_tab);
}

fn tab_prefs(tree: Node, focus: &str, name: Option<&str>) -> TabPrefs {
    TabPrefs {
        tree,
        focus: focus.into(),
        name: name.map(Into::into),
    }
}

fn pair(a: &str, b: &str) -> Node {
    node(SplitDir::Right, 500, leaf(a), leaf(b))
}

#[test]
fn a_legacy_layout_without_tabs_loads_as_one_active_tab() {
    let tree = serde_json::to_string(&pair("a", "b")).unwrap();
    let old = format!(
        r#"{{"panes":["a","b"],"focus":0,"zoom":false,"sidebar":true,"tree":{tree},"shells":["b"]}}"#
    );
    let prefs: LayoutPrefs = serde_json::from_str(&old).unwrap();
    assert!(prefs.tabs.is_empty());
    assert_eq!(prefs.tab_name, None);
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    v.apply_prefs(&prefs);
    assert_eq!(ids(&v), ["a", "b"]);
    assert_eq!(v.focused_session(), Some("a"));
    assert_eq!(v.tab_count(), 1);
    assert_eq!(v.active_tab(), Some(tab_id(&v, "a")));
    assert_eq!(v.shells, ["b"]);
}

#[test]
fn prefs_round_trip_restores_the_active_tab_stashed_tabs_names_and_focus() {
    let mut v = tabbed();
    v.apply(Cmd::RenameTab);
    for _ in 0..30 {
        v.edit_rename(b"\x7f");
    }
    v.edit_rename(b"live\r");
    v.focus_pane(0);
    click_line(&mut v, &session_line("b"));
    v.apply(Cmd::RenameTab);
    for _ in 0..30 {
        v.edit_rename(b"\x7f");
    }
    v.edit_rename(b"first\r");
    click_line(&mut v, &session_line("d"));
    assert_eq!(v.focused_session(), Some("d"));

    let json = serde_json::to_string(&v.prefs()).unwrap();
    let prefs: LayoutPrefs = serde_json::from_str(&json).unwrap();
    let mut fresh = ViewState::new(six(), "local");
    fresh.width = 140;
    fresh.apply_prefs(&prefs);

    assert_eq!(ids(&fresh), ["c", "d"]);
    assert_eq!(fresh.focused_session(), Some("d"));
    assert_eq!(fresh.tab_title(tab_id(&fresh, "c")), "live");
    assert_eq!(fresh.tab_count(), 2);
    assert_eq!(fresh.tab_members(tab_id(&fresh, "a")), ["a", "b"]);
    assert_eq!(fresh.tab_title(tab_id(&fresh, "a")), "first");
    fresh.open("a");
    assert_eq!(fresh.focused_session(), Some("a"));
    fresh.apply(Cmd::NextTab);
    assert_eq!(ids(&fresh), ["c", "d"]);
    assert_eq!(fresh.focused_session(), Some("d"));
}

fn loaded(prefs: LayoutPrefs) -> ViewState {
    let mut v = ViewState::new(six(), "local");
    v.width = 140;
    v.apply_prefs(&prefs);
    v
}

fn active_ab() -> LayoutPrefs {
    LayoutPrefs {
        panes: vec!["a".into(), "b".into()],
        tree: Some(pair("a", "b")),
        ..Default::default()
    }
}

#[test]
fn saved_tabs_drop_unknown_sessions_and_keep_the_rest() {
    let v = loaded(LayoutPrefs {
        tabs: vec![tab_prefs(
            node(SplitDir::Right, 500, pair("c", "zzz"), leaf("d")),
            "d",
            None,
        )],
        ..active_ab()
    });
    assert_eq!(v.tab_members(tab_id(&v, "c")), ["c", "d"]);
    assert_eq!(v.item_of("zzz"), TabItem::Session("zzz".into()));
}

#[test]
fn saved_tabs_never_claim_a_session_twice() {
    let v = loaded(LayoutPrefs {
        tabs: vec![
            tab_prefs(pair("a", "c"), "c", None),
            tab_prefs(pair("c", "d"), "d", None),
            tab_prefs(pair("c", "e"), "e", None),
            tab_prefs(
                node(SplitDir::Right, 500, pair("d", "e"), leaf("f")),
                "f",
                None,
            ),
        ],
        ..active_ab()
    });
    assert_eq!(ids(&v), ["a", "b"]);
    assert_eq!(v.tab_members(tab_id(&v, "c")), ["c", "d"]);
    assert_eq!(v.tab_members(tab_id(&v, "e")), ["e", "f"]);
    assert_eq!(v.tab_count(), 3);
}

#[test]
fn saved_tabs_left_with_fewer_than_two_known_panes_are_dropped() {
    let v = loaded(LayoutPrefs {
        tabs: vec![
            tab_prefs(pair("c", "zzz"), "c", None),
            tab_prefs(pair("a", "b"), "a", None),
            tab_prefs(pair("d", "e"), "d", None),
        ],
        ..active_ab()
    });
    assert_eq!(v.tab_count(), 2);
    assert_eq!(v.item_of("c"), TabItem::Session("c".into()));
    assert_eq!(v.tab_members(tab_id(&v, "d")), ["d", "e"]);
}

#[test]
fn a_saved_tab_with_an_unknown_focus_falls_back_to_its_first_pane() {
    let mut v = loaded(LayoutPrefs {
        tabs: vec![tab_prefs(pair("c", "d"), "nope", None)],
        ..active_ab()
    });
    v.open("c");
    assert_eq!(v.focused_session(), Some("c"));
}

#[test]
fn more_than_thirty_two_saved_tabs_are_capped() {
    let rows: Vec<SessionRow> = (0..80)
        .map(|i| row(&format!("s{i}"), "x", StateKind::Working, "1m"))
        .collect();
    let tabs = (1..40)
        .map(|i| {
            tab_prefs(
                pair(&format!("s{}", 2 * i), &format!("s{}", 2 * i + 1)),
                "",
                None,
            )
        })
        .collect();
    let mut v = ViewState::new(rows, "local");
    v.apply_prefs(&LayoutPrefs {
        panes: vec!["s0".into(), "s1".into()],
        tree: Some(pair("s0", "s1")),
        tabs,
        ..Default::default()
    });
    assert_eq!(v.tab_count(), 33);
}

#[test]
fn set_initial_after_prefs_switches_to_the_tab_holding_the_session() {
    let mut v = loaded(LayoutPrefs {
        tabs: vec![tab_prefs(pair("c", "d"), "c", None)],
        ..active_ab()
    });
    v.set_initial("d");
    assert_eq!(ids(&v), ["c", "d"]);
    assert_eq!(v.focused_session(), Some("d"));
    assert_eq!(v.tab_members(tab_id(&v, "a")), ["a", "b"]);
}

#[test]
fn set_initial_after_prefs_with_a_free_session_keeps_the_restored_tabs_stashed() {
    let mut v = loaded(LayoutPrefs {
        tabs: vec![tab_prefs(pair("c", "d"), "c", None)],
        ..active_ab()
    });
    v.set_initial("f");
    assert_eq!(ids(&v), ["f"]);
    assert_eq!(v.tab_count(), 2);
    assert_eq!(v.tab_members(tab_id(&v, "a")), ["a", "b"]);
    assert_eq!(v.tab_members(tab_id(&v, "c")), ["c", "d"]);
}

#[test]
fn a_vanished_session_of_a_stashed_tab_is_renamed_to_its_resumed_copy() {
    let mut v = tabbed();
    let mut rows = six();
    rows.retain(|r| r.id != "a");
    rows.push(resumed("a2", "claude", "a"));
    v.sessions_changed(AREA, rows);
    assert_eq!(v.tab_members(tab_id(&v, "b")), ["a2", "b"]);
    assert_eq!(v.item_of("a"), TabItem::Session("a".into()));
    v.open("a2");
    assert_eq!(ids(&v), ["a2", "b"]);
}

#[test]
fn the_focus_of_a_stashed_tab_follows_a_resumed_copy() {
    let mut v = tabbed();
    click_line(&mut v, &session_line("a"));
    click_line(&mut v, &session_line("c"));
    let mut rows = six();
    rows.retain(|r| r.id != "a");
    rows.push(resumed("a2", "claude", "a"));
    v.sessions_changed(AREA, rows);
    let header = tab_line(&v, "b");
    click_line(&mut v, &header);
    assert_eq!(v.focused_session(), Some("a2"));
}

#[test]
fn a_vanished_session_dissolves_a_stashed_tab_left_with_one_pane() {
    let mut v = tabbed();
    let mut rows = six();
    rows.retain(|r| r.id != "a");
    v.sessions_changed(AREA, rows);
    assert_eq!(v.item_of("b"), TabItem::Session("b".into()));
    assert_eq!(v.tab_count(), 1);
    assert_eq!(ids(&v), ["c", "d"]);
}

#[test]
fn when_every_pane_of_the_active_tab_vanishes_the_view_falls_back_to_the_first_session_tab() {
    let mut v = tabbed();
    let rows: Vec<SessionRow> = six()
        .into_iter()
        .filter(|r| r.id != "c" && r.id != "d")
        .collect();
    v.sessions_changed(AREA, rows);
    assert_eq!(ids(&v), ["a", "b"]);
    assert_eq!(v.tab_count(), 1);
}

#[test]
fn when_the_active_tab_vanishes_and_the_first_session_is_free_it_is_shown_alone() {
    let mut v = tabbed();
    let rows: Vec<SessionRow> = six()
        .into_iter()
        .filter(|r| !matches!(r.id.as_str(), "a" | "b" | "c" | "d"))
        .collect();
    v.sessions_changed(AREA, rows);
    assert_eq!(ids(&v), ["e"]);
    assert_eq!(v.tab_count(), 0);
}

#[test]
fn prefix_tab_keys_map_to_cmds() {
    let table: &[(u8, Cmd)] = &[
        (b'N', Cmd::NextTab),
        (b'P', Cmd::PrevTab),
        (b',', Cmd::RenameTab),
        (b'&', Cmd::CloseTab),
    ];
    for (byte, cmd) in table {
        let mut s = InputSplitter::new();
        assert_eq!(s.feed(&[0x1d, *byte]), vec![Action::Cmd(*cmd)]);
    }
}

#[test]
fn tab_and_group_actions_appear_in_the_help() {
    let listed: Vec<Cmd> = render::HELP
        .iter()
        .filter_map(|(k, _)| match k {
            render::HelpKey::Cmds(c) => Some(c.iter().copied()),
            render::HelpKey::Lit(_) => None,
        })
        .flatten()
        .collect();
    for cmd in [
        Cmd::NextTab,
        Cmd::PrevTab,
        Cmd::RenameTab,
        Cmd::CloseTab,
        Cmd::Group,
    ] {
        assert!(listed.contains(&cmd), "{cmd:?} missing from the help");
        assert!(ACTIONS.iter().any(|d| d.cmd == cmd && d.palette));
    }
}

#[test]
fn snapshot_tabs_sidebar() {
    let v = tabbed();
    assert_snapshot("tabs", &draw(&v, &screens(), 140, 22));
}

#[test]
fn removing_a_pane_never_takes_a_session_from_a_stashed_tab() {
    let mut v = tabbed();
    v.remove_session("e");
    v.remove_session("f");
    v.remove_session("d");
    assert_eq!(ids(&v), ["c"]);
    let t = tab_id(&v, "a");
    assert_eq!(v.tab_members(t), ["a", "b"]);
}

#[test]
fn a_tab_name_does_not_come_back_after_the_tab_shrinks_to_one_pane() {
    let mut v = two_pane(140);
    v.apply(Cmd::RenameTab);
    v.rename.as_mut().unwrap().text = "work".into();
    assert_eq!(v.edit_rename(b"\r"), RenameEdit::Done);
    assert_eq!(v.tab_title(tab_id(&v, "a")), "work");
    v.close_pane("b");
    v.apply(Cmd::Split);
    assert_eq!(v.tab_title(tab_id(&v, "a")), "claude");
}

#[test]
fn a_new_tab_choice_is_not_written_to_the_saved_layout() {
    let mut v = two_pane(140);
    v.new_place = Some(Place::Tab);
    assert_eq!(v.prefs().new_place, None);
    v.new_place = Some(Place::Below);
    assert_eq!(v.prefs().new_place, Some(Place::Below));
}

#[test]
fn removing_the_only_shown_session_falls_back_to_a_tab_not_a_blank_view() {
    let mut v = tabbed();
    v.open("e");
    v.remove_session("f");
    v.remove_session("e");
    assert_eq!(ids(&v).len(), 2);
    let t = tab_id(&v, "a");
    assert_eq!(v.tab_members(t), ["a", "b"]);
}

#[test]
fn switching_tabs_cancels_a_pending_tab_rename() {
    let mut v = tabbed();
    v.apply(Cmd::RenameTab);
    assert!(v.rename.as_ref().is_some_and(|r| r.tab));
    v.open("a");
    assert!(v.rename.is_none());
}

#[test]
fn a_broken_saved_tab_is_skipped_and_the_rest_of_the_layout_loads() {
    let json = r#"{"panes":["a"],"focus":0,"zoom":false,"sidebar":null,
        "tabs":[{"tree":"nope"},{"tree":{"Split":{"dir":"Right","ratio":500,
        "a":{"Leaf":"b"},"b":{"Leaf":"c"}}},"focus":"c"}]}"#;
    let prefs: LayoutPrefs = serde_json::from_str(json).unwrap();
    assert_eq!(prefs.tabs.len(), 1);
    let mut v = ViewState::new(four(), "local");
    v.apply_prefs(&prefs);
    assert_eq!(ids(&v), ["a"]);
    assert_eq!(v.tab_members(tab_id(&v, "b")), ["b", "c"]);
}

fn border_col(v: &ViewState) -> u16 {
    layout(AREA, v).sidebar.unwrap().right()
}

fn drag(v: &mut ViewState, cb: u16, col: u16, release: bool) -> MouseOut {
    v.on_mouse(
        AREA,
        ev(cb, col, 5, release),
        &mouse_info(MouseMode::default(), 0),
    )
}

#[test]
fn sidebar_keys_resize_within_bounds() {
    let mut s = InputSplitter::new();
    assert_eq!(
        s.feed(&[0x1d, b'<']),
        vec![Action::Cmd(Cmd::SidebarNarrower)]
    );
    assert_eq!(s.feed(&[0x1d, b'>']), vec![Action::Cmd(Cmd::SidebarWider)]);
    let mut v = wide_view();
    assert_eq!(v.sidebar_width(), SIDEBAR_WIDTH);
    v.apply(Cmd::SidebarWider);
    assert_eq!(v.sidebar_width(), SIDEBAR_WIDTH + 2);
    for _ in 0..40 {
        v.apply(Cmd::SidebarWider);
    }
    assert_eq!(v.sidebar_width(), MAX_SIDEBAR_WIDTH);
    for _ in 0..40 {
        v.apply(Cmd::SidebarNarrower);
    }
    assert_eq!(v.sidebar_width(), MIN_SIDEBAR_WIDTH);
    assert_eq!(layout(AREA, &v).sidebar.unwrap().width, MIN_SIDEBAR_WIDTH);
}

#[test]
fn sidebar_never_squeezes_panes_below_the_minimum_on_a_narrow_terminal() {
    let mut v = wide_view();
    v.sidebar_w = Some(MAX_SIDEBAR_WIDTH);
    let area = Rect::new(0, 0, 70, 20);
    let lay = layout(area, &v);
    assert_eq!(lay.sidebar.unwrap().width, 60);
    assert!(lay.panes[0].outer.width >= 10);
    let area = Rect::new(0, 0, 40, 20);
    let lay = layout(area, &v);
    assert!(lay.panes[0].outer.width >= 10);
}

#[test]
fn dragging_the_border_resizes_and_release_stops() {
    let mut v = wide_view();
    let edge = border_col(&v);
    assert!(!drag(&mut v, 0, edge, false).redraw);
    let out = drag(&mut v, 32, 33, false);
    assert!(out.redraw);
    assert_eq!(v.sidebar_width(), 34);
    drag(&mut v, 32, 200, false);
    assert_eq!(v.sidebar_width(), MAX_SIDEBAR_WIDTH);
    drag(&mut v, 32, 2, false);
    assert_eq!(v.sidebar_width(), MIN_SIDEBAR_WIDTH);
    drag(&mut v, 0, 30, true);
    let w = v.sidebar_width();
    drag(&mut v, 32, 50, false);
    assert_eq!(v.sidebar_width(), w);
}

#[test]
fn only_the_border_starts_a_drag() {
    let mut v = wide_view();
    press(&mut v, 10, 5);
    drag(&mut v, 32, 40, false);
    assert_eq!(v.sidebar_width(), SIDEBAR_WIDTH);
}

#[test]
fn dragged_width_is_saved_only_after_release_and_round_trips() {
    let mut v = wide_view();
    let edge = border_col(&v);
    drag(&mut v, 0, edge, false);
    drag(&mut v, 32, 40, false);
    assert_eq!(v.prefs().sidebar_width, None);
    drag(&mut v, 0, 40, true);
    let prefs = v.prefs();
    assert_eq!(prefs.sidebar_width, Some(41));
    let json = serde_json::to_value(&prefs).unwrap();
    let back: LayoutPrefs = serde_json::from_value(json).unwrap();
    let mut other = wide_view();
    other.sidebar_w = Some(30);
    other.apply_prefs(&back);
    assert_eq!(other.sidebar_width(), 41);
    let mut old = wide_view();
    old.sidebar_w = Some(30);
    let legacy: LayoutPrefs =
        serde_json::from_str(r#"{"panes":["a"],"focus":0,"zoom":false,"sidebar":null}"#).unwrap();
    old.apply_prefs(&legacy);
    assert_eq!(old.sidebar_width(), 30);
}

#[test]
fn wider_sidebar_snapshot_and_help_lists_the_keys() {
    let mut v = two_pane(140);
    v.sidebar_w = Some(40);
    assert_snapshot("sidebar_wide_40", &draw(&v, &screens(), 140, 22));
    v.help = true;
    let help = draw(&v, &screens(), 140, 40);
    assert!(help.contains("sidebar width"));
    assert!(ACTIONS
        .iter()
        .any(|a| a.cmd == Cmd::SidebarWider && a.palette));
}

#[test]
fn config_default_is_not_saved_into_the_layout() {
    let mut v = wide_view();
    v.sidebar_default = 30;
    assert_eq!(v.sidebar_width(), 30);
    assert_eq!(v.prefs().sidebar_width, None);
    v.apply(Cmd::SidebarWider);
    assert_eq!(v.prefs().sidebar_width, Some(32));
}

#[test]
fn a_lost_release_does_not_leave_the_drag_stuck() {
    let mut v = wide_view();
    let edge = border_col(&v);
    drag(&mut v, 0, edge, false);
    drag(&mut v, 35, 50, false);
    let w = v.sidebar_width();
    drag(&mut v, 32, 20, false);
    assert_eq!(v.sidebar_width(), w);
    let mut v = wide_view();
    drag(&mut v, 0, edge, false);
    v.help = true;
    drag(&mut v, 0, 10, true);
    v.help = false;
    drag(&mut v, 32, 50, false);
    assert_eq!(v.sidebar_width(), SIDEBAR_WIDTH);
}

#[test]
fn dragged_width_is_capped_to_what_the_terminal_can_show() {
    let mut v = wide_view();
    v.width = 50;
    v.sidebar = Some(true);
    let area = Rect::new(0, 0, 50, 20);
    let edge = layout(area, &v).sidebar.unwrap().right();
    v.on_mouse(
        area,
        ev(0, edge, 5, false),
        &mouse_info(MouseMode::default(), 0),
    );
    v.on_mouse(
        area,
        ev(32, 49, 5, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert_eq!(v.sidebar_width(), 40);
    v.apply(Cmd::SidebarNarrower);
    assert_eq!(v.sidebar_width(), 38);
}

fn fg_of(term: &Terminal<TestBackend>, glyph: &str) -> Option<ratatui::style::Color> {
    term.backend()
        .buffer()
        .content()
        .iter()
        .enumerate()
        .find(|(i, c)| i % 140 < 24 && c.symbol() == glyph)
        .map(|(_, c)| c.fg)
}

fn themed(kind: ThemeKind) -> Terminal<TestBackend> {
    let mut v = two_pane(140);
    v.theme = kind;
    let mut term = Terminal::new(TestBackend::new(140, 22)).unwrap();
    let s = screens();
    term.draw(|f| render(f, &v, &s)).unwrap();
    term
}

#[test]
fn theme_palettes_snapshot() {
    let mut out = String::new();
    for kind in [ThemeKind::Dark, ThemeKind::Light, ThemeKind::HighContrast] {
        out.push_str(&format!("{}\n{:#?}\n", kind.name(), kind.colors()));
    }
    assert_snapshot("theme_palettes", out.trim_end());
}

#[test]
fn state_badges_take_their_colours_from_the_theme() {
    for kind in [ThemeKind::Dark, ThemeKind::Light, ThemeKind::HighContrast] {
        let term = themed(kind);
        let c = kind.colors();
        assert_eq!(fg_of(&term, "◆"), Some(c.accent), "{kind:?} blocked");
        assert_eq!(fg_of(&term, "●"), Some(c.working), "{kind:?} working");
        assert_eq!(fg_of(&term, "◌"), Some(c.waiting), "{kind:?} waiting");
        assert_eq!(fg_of(&term, "✓"), Some(c.ok), "{kind:?} ok");
    }
}

#[test]
fn light_theme_never_paints_white_text_and_dark_never_black() {
    use ratatui::style::Color as C;
    let light = themed(ThemeKind::Light);
    assert!(light
        .backend()
        .buffer()
        .content()
        .iter()
        .all(|c| c.fg != C::White && c.bg != C::Indexed(237)));
    let dark = themed(ThemeKind::Dark);
    assert!(dark
        .backend()
        .buffer()
        .content()
        .iter()
        .all(|c| c.fg != C::Black));
}

#[test]
fn theme_does_not_change_the_text_layout() {
    let dark = text(&themed(ThemeKind::Dark));
    assert_eq!(dark, text(&themed(ThemeKind::Light)));
    assert_eq!(dark, text(&themed(ThemeKind::HighContrast)));
    assert_snapshot("wide", &dark);
}

#[test]
fn theme_names_parse() {
    assert_eq!(ThemeKind::parse("Light"), Some(ThemeKind::Light));
    assert_eq!(
        ThemeKind::parse(" high-contrast "),
        Some(ThemeKind::HighContrast)
    );
    assert_eq!(ThemeKind::parse("dark"), Some(ThemeKind::Dark));
    assert_eq!(ThemeKind::parse("auto"), None);
    assert_eq!(ThemeKind::parse("neon"), None);
}

#[test]
fn osc11_replies_parse_in_every_width_and_terminator() {
    assert_eq!(
        parse_osc11(b"\x1b]11;rgb:ffff/ffff/ffff\x07"),
        Some((255, 255, 255))
    );
    assert_eq!(
        parse_osc11(b"\x1b]11;rgb:0000/0000/0000\x1b\\"),
        Some((0, 0, 0))
    );
    assert_eq!(
        parse_osc11(b"\x1b]11;rgb:1e1e/1e1e/2e2e\x07"),
        Some((30, 30, 46))
    );
    assert_eq!(
        parse_osc11(b"\x1b]11;rgb:ff/80/00\x07"),
        Some((255, 128, 0))
    );
    assert_eq!(parse_osc11(b"\x1b]11;rgb:f/0/8\x07"), Some((255, 0, 136)));
    assert_eq!(parse_osc11(b"\x1b]11;rgb:ffff/ffff\x07"), None);
    assert_eq!(parse_osc11(b"\x1b]11;rgb:gg/00/00\x07"), None);
    assert_eq!(parse_osc11(b"\x1b]11;rgb:00/00/00/00\x07"), None);
    assert_eq!(parse_osc11(b"\x1b]11;#ffffff\x07"), None);
    assert_eq!(
        parse_osc11(b"\x1b]11;rgba:ffff/0000/0000/ffff\x07"),
        Some((255, 0, 0))
    );
    assert_eq!(
        parse_osc11(b"junk\xff\x1b]11;rgb:00/00/00\x07x"),
        Some((0, 0, 0))
    );
    assert_eq!(parse_osc11(b""), None);
    assert_eq!(parse_osc11(&[0xff, 0xfe]), None);
}

#[test]
fn background_brightness_picks_the_theme() {
    assert_eq!(kind_for_background((255, 255, 255)), ThemeKind::Light);
    assert_eq!(kind_for_background((253, 246, 227)), ThemeKind::Light);
    assert_eq!(kind_for_background((0, 0, 0)), ThemeKind::Dark);
    assert_eq!(kind_for_background((30, 30, 46)), ThemeKind::Dark);
    assert_eq!(kind_for_background((0, 0, 255)), ThemeKind::Dark);
}

fn at(v: &mut ViewState, cb: u16, col: u16, row: u16, release: bool) -> MouseOut {
    v.on_mouse(
        AREA,
        ev(cb, col, row, release),
        &mouse_info(MouseMode::default(), 0),
    )
}

fn two_panes(dir: SplitDir) -> ViewState {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    assert!(v.split_focused(dir, "b"));
    v
}

fn body_of(v: &ViewState) -> Rect {
    let sb = layout(AREA, v).sidebar.map_or(0, |r| r.width);
    Rect::new(sb, 0, AREA.width - sb, AREA.height - 1)
}

fn outer(v: &ViewState, i: usize) -> Rect {
    layout(AREA, v).panes[i].outer
}

#[test]
fn dragging_the_vertical_line_between_panes_moves_it() {
    let mut v = two_panes(SplitDir::Right);
    let edge = outer(&v, 1).x;
    let row = outer(&v, 0).y + 5;
    at(&mut v, 0, edge, row, false);
    let out = at(&mut v, 32, edge + 10, row, false);
    assert!(out.redraw);
    assert_eq!(outer(&v, 1).x, edge + 10);
    at(&mut v, 32, 0, row, false);
    assert!(outer(&v, 0).width >= 3);
    at(&mut v, 0, 60, row, true);
    let x = outer(&v, 1).x;
    at(&mut v, 32, x + 5, row, false);
    assert_eq!(outer(&v, 1).x, x);
}

#[test]
fn grabbing_the_left_panes_edge_column_keeps_the_line_under_the_pointer() {
    let mut v = two_panes(SplitDir::Right);
    let edge = outer(&v, 1).x;
    let row = outer(&v, 0).y + 5;
    at(&mut v, 0, edge - 1, row, false);
    at(&mut v, 32, edge + 7, row, false);
    assert_eq!(outer(&v, 1).x, edge + 8);
}

#[test]
fn dragging_the_header_row_of_the_bottom_pane_moves_the_horizontal_line() {
    let mut v = two_panes(SplitDir::Down);
    let top = outer(&v, 0);
    let header = outer(&v, 1).y;
    let x = outer(&v, 1).x + 3;
    assert_eq!(header, top.bottom());
    at(&mut v, 0, x, header, false);
    at(&mut v, 32, x, header + 6, false);
    assert_eq!(outer(&v, 1).y, header + 6);
    at(&mut v, 32, x, 0, false);
    assert!(outer(&v, 0).height >= 3);
    at(&mut v, 0, x, 10, true);
}

#[test]
fn header_buttons_still_click_and_do_not_start_a_drag() {
    let mut v = two_panes(SplitDir::Down);
    let o = outer(&v, 1);
    let col = header_button(o, Control::Stop).unwrap();
    let out = at(&mut v, 0, col, o.y, false);
    assert!(out.control.is_some());
    at(&mut v, 0, col, o.y, true);
    at(&mut v, 32, col, o.y + 5, false);
    assert_eq!(outer(&v, 1).y, o.y);
}

#[test]
fn nested_drag_moves_the_split_that_owns_the_border() {
    let mut v = grid(140);
    let before: Vec<Rect> = layout(AREA, &v).panes.iter().map(|p| p.outer).collect();
    let tree = v.prefs().tree.unwrap();
    let body = body_of(&v);
    let borders = tree.borders(body);
    assert_eq!(borders.len(), 3);
    let inner = &borders[0];
    assert!(inner.path.len() > borders[2].path.len());
    let (col, row) = match inner.dir {
        SplitDir::Right => (inner.edge, inner.rect.y + 2),
        SplitDir::Down => (inner.rect.x + 2, inner.edge),
    };
    at(&mut v, 0, col, row, false);
    match inner.dir {
        SplitDir::Right => at(&mut v, 32, col + 4, row, false),
        SplitDir::Down => at(&mut v, 32, col, row + 2, false),
    };
    at(&mut v, 0, col, row, true);
    let after: Vec<Rect> = layout(AREA, &v).panes.iter().map(|p| p.outer).collect();
    assert_ne!(before, after);
    let root = tree.borders(body).pop().unwrap();
    let now = v.prefs().tree.unwrap().borders(body).pop().unwrap();
    assert_eq!(root.edge, now.edge);
}

#[test]
fn border_hit_testing_ignores_cells_outside_the_split() {
    let v = two_panes(SplitDir::Right);
    let body = body_of(&v);
    let tree = v.prefs().tree.unwrap();
    let edge = outer(&v, 1).x;
    assert!(tree.border_at(body, edge, body.y).is_some());
    assert!(tree.border_at(body, edge - 1, body.y).is_some());
    assert!(tree.border_at(body, edge + 1, body.y).is_none());
    assert!(tree.border_at(body, edge, body.bottom()).is_none());
    assert!(leaf("a").border_at(body, edge, 3).is_none());
}

#[test]
fn drag_ratio_is_clamped_on_a_narrow_terminal() {
    let mut t = node(SplitDir::Right, 500, leaf("a"), leaf("b"));
    let rect = Rect::new(0, 0, 60, 20);
    let b = t.borders(rect).pop().unwrap();
    assert!(t.drag_border(&b, 1000));
    let rects = t.layout(rect).unwrap();
    assert!(rects[1].1.width >= 3);
    assert!(t.drag_border(&b, 0));
    let rects = t.layout(rect).unwrap();
    assert!(rects[0].1.width >= 3);
}

#[test]
fn dragged_ratio_survives_prefs_round_trip() {
    let mut v = two_panes(SplitDir::Right);
    let edge = outer(&v, 1).x;
    at(&mut v, 0, edge, 5, false);
    at(&mut v, 32, edge + 12, 5, false);
    at(&mut v, 0, edge + 12, 5, true);
    let json = serde_json::to_value(v.prefs()).unwrap();
    let back: LayoutPrefs = serde_json::from_value(json).unwrap();
    let mut other = two_panes(SplitDir::Right);
    other.apply_prefs(&back);
    assert_eq!(outer(&other, 1).x, edge + 12);
}

#[test]
fn drag_state_is_dropped_when_the_tree_changes_and_not_saved_midway() {
    let mut v = two_panes(SplitDir::Right);
    let saved = v.prefs().tree;
    let edge = outer(&v, 1).x;
    at(&mut v, 0, edge, 5, false);
    at(&mut v, 32, edge + 6, 5, false);
    assert_eq!(v.prefs().tree, saved);
    v.close_pane("b");
    let out = at(&mut v, 32, edge + 9, 5, false);
    assert!(out.redraw);
    assert_eq!(layout(AREA, &v).panes.len(), 1);
}

#[test]
fn sidebar_border_wins_over_a_pane_line_at_the_shared_column() {
    let mut v = two_panes(SplitDir::Down);
    let sb = layout(AREA, &v).sidebar.unwrap();
    let header = outer(&v, 1).y;
    let before = outer(&v, 1).y;
    at(&mut v, 0, sb.right(), header, false);
    at(&mut v, 32, sb.right() + 4, header + 5, false);
    assert_eq!(outer(&v, 1).y, before);
    assert!(v.sidebar_width() > sb.width);
}

mod questions_ui {
    use super::*;
    use crate::attach::qmenu::qgeom;
    use crate::questions::{Entry, Origin, Question, Status};

    const NOW: u64 = 1_000_000;

    fn entry(id: &str, session: &str, text: &str, status: Status, ago_s: u64) -> Entry {
        Entry {
            q: Question {
                id: id.into(),
                session: session.into(),
                text: text.into(),
                asked_s: NOW - ago_s,
                origin: Origin::Rule,
                user: "Add Apple Pay to the checkout form".into(),
                reply: "I added the button.\n\nThe session check is still missing. Should I also gate it behind the feature flag?\nOtherwise it shows for everyone.\n\nDone otherwise.".into(),
                item: String::new(),
                pos: 0,
                turn: 0,
            },
            status,
        }
    }

    fn with_questions(width: u16) -> ViewState {
        let mut v = two_pane(width);
        v.now_s = NOW;
        v.set_questions(vec![
            entry(
                "q1",
                "a",
                "Should I also gate it behind the feature flag?",
                Status::Open,
                720,
            ),
            entry(
                "q2",
                "a",
                "Which currency should the button default to?",
                Status::Open,
                7200,
            ),
            entry("q3", "a", "Keep the old API?", Status::Ignored, 90_000),
            entry("q4", "b", "Restart the dev server now?", Status::Open, 30),
            entry("q5", "b", "Already handled?", Status::Answered, 30),
            entry("q6", "gone", "Orphan?", Status::Open, 30),
        ]);
        v
    }

    #[test]
    fn counts_only_open_questions_of_known_sessions() {
        let v = with_questions(140);
        assert_eq!(v.open_in("a"), 2);
        assert_eq!(v.open_in("b"), 1);
        assert_eq!(v.open_total(), 3);
    }

    #[test]
    fn snapshot_badges_in_header_sidebar_and_status() {
        let v = with_questions(140);
        let text = draw(&v, &screens(), 140, 22);
        assert!(text.contains("?2"));
        assert_snapshot("questions_badges", &text);
    }

    #[test]
    fn narrow_status_line_carries_the_total() {
        let v = with_questions(90);
        let text = draw(&v, &screens(), 90, 22);
        assert!(text.lines().last().unwrap().contains("?3"), "{text}");
    }

    #[test]
    fn no_badge_without_open_questions() {
        let mut v = with_questions(140);
        v.set_questions(Vec::new());
        let text = draw(&v, &screens(), 140, 22);
        let chars: Vec<char> = text.chars().collect();
        assert!(
            !chars
                .windows(2)
                .any(|w| w[0] == '?' && w[1].is_ascii_digit()),
            "{text}"
        );
    }

    #[test]
    fn snapshot_menu_for_the_focused_session() {
        let mut v = with_questions(140);
        v.open_questions(Some("a".into()));
        assert!(v.qmenu.is_some());
        assert_snapshot("questions_menu", &draw(&v, &screens(), 140, 30));
    }

    #[test]
    fn snapshot_menu_with_context() {
        let mut v = with_questions(140);
        v.open_questions(Some("a".into()));
        v.edit_qmenu(b"c");
        assert_snapshot("questions_menu_context", &draw(&v, &screens(), 140, 30));
    }

    #[test]
    fn snapshot_menu_on_a_narrow_terminal_falls_back_to_keys() {
        let mut v = with_questions(60);
        v.open_questions(Some("a".into()));
        let text = draw(&v, &screens(), 60, 24);
        assert!(!text.contains("[Answer]"));
        assert!(text.contains("a answer"));
        assert_snapshot("questions_menu_narrow", &text);
    }

    #[test]
    fn menu_without_a_session_scope_lists_all_with_names() {
        let mut v = with_questions(140);
        v.open_questions(None);
        let text = draw(&v, &screens(), 140, 30);
        assert!(text.contains("claude: Should I also gate"));
        assert!(text.contains("dev-server: Restart"));
        assert!(!text.contains("Orphan"));
    }

    fn lines(text: &[&str]) -> Vec<String> {
        text.iter().map(|l| l.to_string()).collect()
    }

    #[test]
    fn find_question_matches_across_wrapped_lines_and_takes_the_latest() {
        use crate::attach::find_question;
        let l = lines(&[
            "Should I gate it?",
            "ok",
            "I added the button. Should I also gate",
            "  it behind the feature flag?",
            "> ",
        ]);
        assert_eq!(
            find_question(&l, "Should I also gate it behind the feature flag?"),
            Some((2, 3))
        );
        assert_eq!(find_question(&l, "should i gate it?"), Some((0, 0)));
        assert_eq!(find_question(&l, "Something else entirely?"), None);
        assert_eq!(find_question(&l, "   "), None);
        let twice = lines(&["Keep it?", "x", "Keep it?"]);
        assert_eq!(find_question(&twice, "Keep it?"), Some((2, 2)));
    }

    #[test]
    fn find_question_uses_the_head_or_tail_when_the_middle_is_mangled() {
        use crate::attach::find_question;
        let q = "Should I also gate the Apple Pay button behind the feature flag or leave it on for everyone?";
        let l = lines(&[
            "Should I also gate the Apple Pay button behind",
            "| the feature flag or leave",
            "it on for everyone?",
        ]);
        assert_eq!(find_question(&l, q), Some((0, 0)));
    }

    #[test]
    fn goto_target_puts_the_match_near_the_top_and_clamps() {
        use crate::attach::goto_target;
        let (off, mark) = goto_target(100, 20, 80, (30, 31), "a");
        assert_eq!(off, 100 - 20 - 29);
        assert_eq!((mark.bottom, mark.rows), (100 - 1 - 31, 2));
        let (off, _) = goto_target(100, 20, 80, (0, 0), "a");
        assert_eq!(off, 80);
        let (off, _) = goto_target(100, 20, 80, (95, 95), "a");
        assert_eq!(off, 0);
        let (_, mark) = goto_target(100, 20, 80, (10, 40), "a");
        assert_eq!((mark.rows, mark.bottom), (8, 59));
    }

    #[test]
    fn the_goto_highlight_marks_only_the_matched_rows_while_scrolled() {
        use crate::attach::{GotoMark, ScrollState};
        let mut v = with_questions(140);
        let n = screens().0["a"].lines.len();
        v.goto = Some(GotoMark {
            session: "a".into(),
            bottom: n - 2,
            rows: 1,
        });
        let marked = |v: &ViewState| {
            let mut term = Terminal::new(TestBackend::new(140, 30)).unwrap();
            term.draw(|f| render(f, v, &screens())).unwrap();
            let buf = term.backend().buffer();
            let w = buf.area.width as usize;
            buf.content()
                .chunks(w)
                .filter(|row| {
                    row.iter()
                        .any(|c| c.modifier.contains(ratatui::style::Modifier::REVERSED))
                })
                .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
                .filter(|t| t.contains("I will add an Apple Pay"))
                .count()
        };
        assert_eq!(marked(&v), 0);
        v.scroll = Some(ScrollState::default());
        assert_eq!(marked(&v), 1);
        v.goto.as_mut().unwrap().session = "b".into();
        assert_eq!(marked(&v), 0);
    }

    #[test]
    fn go_to_and_fork_keys_and_buttons() {
        let mut v = with_questions(140);
        v.open_questions(Some("a".into()));
        assert_eq!(v.edit_qmenu(b"g"), QEdit::Goto("q1".into()));
        assert!(v.qmenu.is_none());
        v.open_questions(Some("a".into()));
        assert_eq!(v.edit_qmenu(b"f"), QEdit::Fork("q1".into()));
        assert!(v.qmenu.is_none());

        v.open_questions(Some("a".into()));
        let text = draw(&v, &screens(), 140, 30);
        assert!(text.contains("[Go to]"), "{text}");
        assert!(!text.contains("[Fork]"), "{text}");
        v.forkable = vec!["a".into()];
        assert!(draw(&v, &screens(), 140, 30).contains("[Fork]"));
        v.forkable.clear();

        v.forkable = vec!["a".into()];
        let g = qgeom(AREA, &v).unwrap();
        let (_, x, _) = g.rows[0]
            .buttons
            .iter()
            .copied()
            .find(|(b, ..)| *b == crate::attach::QButton::Fork)
            .unwrap();
        let y = g.rows[0].y;
        let out = v.on_mouse(
            AREA,
            ev(0, x + 1, y, false),
            &mouse_info(MouseMode::default(), 0),
        );
        assert_eq!(out.question, Some(QEdit::Fork("q1".into())));
    }

    #[test]
    fn show_context_selects_the_question_and_opens_its_context() {
        let mut v = with_questions(140);
        v.show_context("a", "q2");
        let m = v.qmenu.as_ref().unwrap();
        assert!(m.context);
        assert_eq!(m.sel, 1);
    }

    #[test]
    fn toggle_and_empty_notice() {
        let mut v = with_questions(140);
        v.apply(Cmd::Questions);
        assert!(v.qmenu.is_some());
        v.apply(Cmd::Questions);
        assert!(v.qmenu.is_none());
        v.set_questions(Vec::new());
        v.apply(Cmd::Questions);
        assert!(v.qmenu.is_none());
        assert_eq!(v.notice.as_deref(), Some("no unanswered questions"));
    }

    #[test]
    fn keys_move_toggle_context_answer_and_ignore() {
        let mut v = with_questions(140);
        v.open_questions(Some("a".into()));
        assert_eq!(v.edit_qmenu(b"j"), QEdit::Editing);
        assert_eq!(v.qmenu.as_ref().unwrap().sel, 1);
        assert_eq!(v.edit_qmenu(b"\x1b[A"), QEdit::Editing);
        assert_eq!(v.qmenu.as_ref().unwrap().sel, 0);
        v.edit_qmenu(b"c");
        assert!(v.qmenu.as_ref().unwrap().context);
        v.edit_qmenu(b"\x1b[B");
        assert!(!v.qmenu.as_ref().unwrap().context);
        assert_eq!(v.edit_qmenu(b"i"), QEdit::Ignore("q2".into()));
        assert!(v.qmenu.is_some());
        assert_eq!(v.edit_qmenu(b"\r"), QEdit::Answer("q2".into()));
        assert!(v.qmenu.is_none());
    }

    #[test]
    fn ignored_tab_restores() {
        let mut v = with_questions(140);
        v.open_questions(Some("a".into()));
        v.edit_qmenu(b"\t");
        assert!(v.qmenu.as_ref().unwrap().ignored);
        assert_eq!(v.edit_qmenu(b"i"), QEdit::Restore("q3".into()));
        v.set_question_status("q3", Status::Open);
        assert_eq!(v.open_in("a"), 3);
        let text = draw(&v, &screens(), 140, 30);
        assert!(text.contains("Nothing ignored"));
    }

    #[test]
    fn escape_and_q_close_the_menu() {
        for key in [&b"\x1b"[..], b"q"] {
            let mut v = with_questions(140);
            v.open_questions(Some("a".into()));
            assert_eq!(v.edit_qmenu(key), QEdit::Close);
            assert!(v.qmenu.is_none());
        }
    }

    #[test]
    fn click_on_the_header_badge_opens_the_menu_for_that_session() {
        let mut v = with_questions(140);
        let lay = layout(AREA, &v);
        let pane = lay.panes.iter().find(|p| p.session == "a").unwrap().clone();
        let (x, _) = v.header_badge(pane.outer, "a").unwrap();
        v.on_mouse(
            AREA,
            ev(0, x, pane.outer.y, false),
            &mouse_info(MouseMode::default(), 0),
        );
        assert_eq!(v.qmenu.as_ref().unwrap().scope.as_deref(), Some("a"));
    }

    #[test]
    fn click_on_the_sidebar_badge_opens_the_menu_and_focuses_the_session() {
        let mut v = with_questions(140);
        let sb = layout(AREA, &v).sidebar.unwrap();
        let row = v
            .sidebar_layout(sb)
            .rows()
            .into_iter()
            .find(|(id, _)| id == "b")
            .unwrap()
            .1;
        let (x, _) = v.sidebar_badge(sb, "b", 2).unwrap();
        v.on_mouse(
            AREA,
            ev(0, x, row, false),
            &mouse_info(MouseMode::default(), 0),
        );
        assert_eq!(v.qmenu.as_ref().unwrap().scope.as_deref(), Some("b"));
    }

    #[test]
    fn menu_buttons_answer_ignore_context_and_outside_click() {
        let mut v = with_questions(140);
        v.open_questions(Some("a".into()));
        let info = mouse_info(MouseMode::default(), 0);
        let click = |v: &mut ViewState, x: u16, y: u16| {
            let out = v.on_mouse(AREA, ev(0, x, y, false), &info);
            v.on_mouse(AREA, ev(0, x, y, true), &info);
            out
        };
        let g = qgeom(AREA, &v).unwrap();
        let second = &g.rows[1];
        let find = |b: crate::attach::QButton| {
            second
                .buttons
                .iter()
                .find(|(k, ..)| *k == b)
                .map(|(_, x, _)| *x)
                .unwrap()
        };
        let out = click(&mut v, find(crate::attach::QButton::Context), second.y);
        assert_eq!(out.question, Some(QEdit::Editing));
        assert!(v.qmenu.as_ref().unwrap().context);
        let g = qgeom(AREA, &v).unwrap();
        let second = &g.rows[1];
        let ignore = second
            .buttons
            .iter()
            .find(|(k, ..)| *k == crate::attach::QButton::Ignore)
            .unwrap()
            .1;
        let out = click(&mut v, ignore, second.y);
        assert_eq!(out.question, Some(QEdit::Ignore("q2".into())));
        let answer = second
            .buttons
            .iter()
            .find(|(k, ..)| *k == crate::attach::QButton::Answer)
            .unwrap()
            .1;
        let out = click(&mut v, answer, second.y);
        assert_eq!(out.question, Some(QEdit::Answer("q2".into())));
        assert!(v.qmenu.is_none());
        v.open_questions(Some("a".into()));
        let out = click(&mut v, 0, 39);
        assert_eq!(out.question, Some(QEdit::Close));
        assert!(v.qmenu.is_none());
    }

    #[test]
    fn menu_tabs_are_clickable() {
        let mut v = with_questions(140);
        v.open_questions(Some("a".into()));
        let g = qgeom(AREA, &v).unwrap();
        let (_, x, _) = g.tabs[1];
        v.on_mouse(
            AREA,
            ev(0, x, g.inner.y, false),
            &mouse_info(MouseMode::default(), 0),
        );
        assert!(v.qmenu.as_ref().unwrap().ignored);
    }

    #[test]
    fn ctrl_bracket_q_is_the_questions_command_and_is_listed_everywhere() {
        let mut s = InputSplitter::new();
        assert_eq!(s.feed(&[0x1d, b'q']), vec![Action::Cmd(Cmd::Questions)]);
        let mut v = with_questions(140);
        v.help = true;
        assert!(draw(&v, &screens(), 140, 40).contains("agent questions"));
        v.help = false;
        v.apply(Cmd::Palette);
        v.palette.as_mut().unwrap().query = "unanswered".into();
        assert!(v
            .entries()
            .iter()
            .any(|e| e.entry == PaletteEntry::Cmd(Cmd::Questions)));
    }

    #[test]
    fn the_cursor_is_hidden_while_the_menu_is_open() {
        let mut v = with_questions(140);
        v.focus_pane(0);
        let lay = layout(AREA, &v);
        assert!(cursor(&v, &lay, &screens()).is_some());
        v.open_questions(Some("a".into()));
        assert!(cursor(&v, &lay, &screens()).is_none());
    }

    #[test]
    fn long_questions_and_terminal_controls_stay_inside_the_box() {
        let mut v = with_questions(140);
        v.set_questions(vec![entry(
            "q9",
            "a",
            &format!("{}\u{1b}[2J?", "very long ".repeat(40)),
            Status::Open,
            5,
        )]);
        v.open_questions(Some("a".into()));
        let text = draw(&v, &screens(), 140, 30);
        assert!(!text.contains('\u{1b}'));
        for line in text.lines() {
            assert!(line.chars().count() <= 140);
        }
    }

    #[test]
    fn tiny_terminals_do_not_panic() {
        for (w, h) in [(160, 45), (80, 24), (40, 10), (20, 6), (5, 3)] {
            let mut v = with_questions(w);
            v.open_questions(Some("a".into()));
            v.edit_qmenu(b"c");
            let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
            term.draw(|f| render(f, &v, &Screens::default())).unwrap();
        }
    }

    #[test]
    fn the_selection_follows_the_question_when_new_ones_arrive() {
        let mut v = with_questions(140);
        v.open_questions(Some("a".into()));
        v.edit_qmenu(b"j");
        assert_eq!(v.edit_qmenu(b"\r"), QEdit::Answer("q2".into()));
        v.open_questions(Some("a".into()));
        v.edit_qmenu(b"j");
        let mut next = v.questions.clone();
        next.insert(0, entry("q0", "a", "Brand new?", Status::Open, 1));
        v.set_questions(next);
        assert_eq!(v.edit_qmenu(b"i"), QEdit::Ignore("q2".into()));
    }

    #[test]
    fn the_selected_row_stays_visible_on_a_short_terminal() {
        let mut v = with_questions(140);
        let mut many = v.questions.clone();
        for i in 0..10 {
            many.push(entry(
                &format!("n{i}"),
                "a",
                &format!("Extra question {i}?"),
                Status::Open,
                10 + i,
            ));
        }
        v.set_questions(many);
        v.open_questions(Some("a".into()));
        for _ in 0..8 {
            v.edit_qmenu(b"j");
        }
        let area = Rect::new(0, 0, 140, 10);
        let g = qgeom(area, &v).unwrap();
        let sel = v.qmenu.as_ref().unwrap().sel;
        assert!(g.rows.iter().any(|r| r.idx == sel));
        assert!(g.rows.iter().all(|r| r.y < g.hint_y));
    }
}

fn with_status(width: u16) -> ViewState {
    let mut v = two_pane(width);
    v.sessions[0].state.since_ms = 58_000;
    v.now_s = 100;
    let mut found = std::collections::HashMap::new();
    found.insert(
        "a".to_string(),
        SessionStatus {
            activity: Some("Bash: cargo test".into()),
            todo: Some((3, 7)),
            ..Default::default()
        },
    );
    found.insert(
        "b".to_string(),
        SessionStatus {
            activity: Some("Edit: api.rs".into()),
            todo: None,
            ..Default::default()
        },
    );
    assert!(v.set_status(found));
    v
}

#[test]
fn status_line_shows_todo_activity_and_turn_clock_while_working() {
    let v = with_status(140);
    let line = v.status_line("a").unwrap();
    assert_eq!(line.text, "3/7 · Bash: cargo test");
    assert_eq!(line.clock.as_deref(), Some("0:42"));
}

#[test]
fn status_line_hides_activity_when_not_working() {
    let v = with_status(140);
    assert_eq!(v.status_line("b"), None);
}

#[test]
fn status_line_keeps_open_todos_when_waiting() {
    let mut v = with_status(140);
    v.sessions[0].state.kind = StateKind::Waiting;
    let line = v.status_line("a").unwrap();
    assert_eq!(line.text, "3/7");
    assert_eq!(line.clock, None);
}

#[test]
fn set_status_reports_changes_and_drops_unknown_sessions() {
    let mut v = with_status(140);
    let same = v.status.clone();
    assert!(!v.set_status(same));
    let mut ghost = std::collections::HashMap::new();
    ghost.insert(
        "zzz".to_string(),
        SessionStatus {
            activity: Some("Bash: ls".into()),
            todo: None,
            ..Default::default()
        },
    );
    assert!(v.set_status(ghost));
    assert!(v.status.is_empty());
}

#[test]
fn without_status_the_sidebar_has_no_activity_lines() {
    let v = two_pane(140);
    let sl = v.sidebar_layout(AREA);
    assert!(!sl
        .lines
        .iter()
        .any(|(l, _)| matches!(l, SidebarLine::Activity(_))));
}

#[test]
fn clicking_an_activity_line_selects_its_session() {
    let v = with_status(140);
    let sb = layout(AREA, &v).sidebar.unwrap();
    let sl = v.sidebar_layout(sb);
    let (_, y) = sl
        .lines
        .iter()
        .find(|(l, _)| matches!(l, SidebarLine::Activity(id) if id == "a"))
        .unwrap();
    assert_eq!(v.hit(AREA, sb.x + 4, *y), Hit::Session("a".into()));
}

#[test]
fn snapshot_wide_with_status() {
    let v = with_status(140);
    assert_snapshot("status_wide", &draw(&v, &screens(), 140, 22));
}

#[test]
fn snapshot_narrow_with_status() {
    let v = with_status(90);
    assert_snapshot("status_narrow", &draw(&v, &screens(), 90, 16));
}

fn with_meter(width: u16) -> ViewState {
    let mut v = with_status(width);
    let mut found = v.status.clone();
    for (id, model, ctx, mode) in [("a", "opus", 68, "bypass"), ("b", "sonnet", 12, "plan")] {
        let st = found.get_mut(id).unwrap();
        st.model = Some(model.into());
        st.ctx = Some(ctx);
        st.mode = Some(mode.into());
    }
    assert!(v.set_status(found));
    let mut bar = UsageBar {
        claude_5h: Some(Reading {
            pct: 34.4,
            stale: false,
        }),
        claude_week: Some(Reading {
            pct: 82.0,
            stale: false,
        }),
        codex_week: Some(Reading {
            pct: 8.0,
            stale: true,
        }),
        ..UsageBar::default()
    };
    bar.costs.insert(
        "b".into(),
        Cost {
            usd: 1.234,
            stale: false,
        },
    );
    assert!(v.set_usage(bar));
    v
}

fn texts(parts: &[Part]) -> Vec<String> {
    parts.iter().map(|p| p.text.clone()).collect()
}

#[test]
fn usage_parts_format_limits_cost_and_unknowns() {
    let v = with_meter(140);
    let parts = v.usage_parts();
    assert_eq!(texts(&parts), ["5h 34%", "wk 82%", "cx ~8%", "$1.23"]);
    assert_eq!(parts[1].tone, Tone::Warn);
    assert_eq!(parts[2].tone, Tone::Stale);
    let mut v = v;
    v.usage = Some(UsageBar::default());
    assert!(v.usage_parts().is_empty());
    v.usage.as_mut().unwrap().codex_week = Some(Reading {
        pct: 3.0,
        stale: false,
    });
    assert_eq!(texts(&v.usage_parts()), ["5h ?", "wk ?", "cx 3%", "$?"]);
    v.status.clear();
    assert_eq!(texts(&v.usage_parts()), ["5h ?", "wk ?", "cx 3%"]);
    v.usage = None;
    assert!(v.usage_parts().is_empty());
}

#[test]
fn usage_cost_is_marked_when_data_lags() {
    let mut v = with_meter(140);
    v.usage.as_mut().unwrap().costs.insert(
        "b".into(),
        Cost {
            usd: 0.5,
            stale: true,
        },
    );
    let parts = v.usage_parts();
    assert_eq!(parts[3].text, "$~0.50");
    assert_eq!(parts[3].tone, Tone::Stale);
}

#[test]
fn session_meta_highlights_bypass_and_flags_unknowns() {
    let v = with_meter(140);
    let a = v.session_meta("a").unwrap();
    assert_eq!(a.text(), "opus · ctx 68% · bypass");
    assert_eq!(a.parts[2].tone, Tone::Hot);
    let b = v.session_meta("b").unwrap();
    assert_eq!(b.text(), "sonnet · ctx 12% · plan");
    assert_eq!(b.parts[2].tone, Tone::Plain);
    let mut v = v;
    v.status.get_mut("b").unwrap().ctx = Some(96);
    v.status.get_mut("b").unwrap().mode = None;
    let b = v.session_meta("b").unwrap();
    assert_eq!(b.text(), "sonnet · ctx 96% · ?");
    assert_eq!(b.parts[1].tone, Tone::Hot);
    assert!(v.session_meta("c").is_none());
}

#[test]
fn bypass_is_drawn_in_the_alert_colour() {
    let v = with_meter(140);
    let term = term_for(&v, &screens(), 140, 22);
    let buf = term.backend().buffer();
    let found = (0..22u16)
        .flat_map(|y| (0..134u16).map(move |x| (x, y)))
        .find(|(x, y)| {
            (0..6u16)
                .map(|i| buf[(x + i, *y)].symbol())
                .collect::<String>()
                == "bypass"
        })
        .expect("bypass is drawn");
    let cell = &buf[found];
    assert_eq!(cell.fg, super::theme::colors().bad);
    assert!(cell.modifier.contains(ratatui::style::Modifier::BOLD));
}

#[test]
fn snapshot_wide_with_usage_and_meta() {
    let v = with_meter(140);
    assert_snapshot("usage_wide", &draw(&v, &screens(), 140, 22));
}

#[test]
fn snapshot_narrow_with_usage_and_meta() {
    let v = with_meter(90);
    assert_snapshot("usage_narrow", &draw(&v, &screens(), 90, 16));
}

#[test]
fn a_tight_header_keeps_bypass_and_drops_the_rest() {
    let v = with_meter(140);
    let parts = super::render::header_meta(&v, "a", 12).unwrap();
    assert_eq!(texts(&parts), ["bypass"]);
    assert!(super::render::header_meta(&v, "a", 5).is_none());
}

#[test]
fn a_working_session_between_tools_shows_thinking_and_the_clock() {
    let mut v = with_status(140);
    v.status.insert("a".into(), SessionStatus::default());
    let line = v.status_line("a").unwrap();
    assert_eq!(line.text, "thinking");
    assert_eq!(line.clock.as_deref(), Some("0:42"));
}

#[test]
fn a_short_sidebar_drops_activity_lines_before_detail_lines() {
    let mut v = with_status(140);
    v.locs.insert(
        "a".into(),
        Loc {
            dir: "/w/app".into(),
            root: "/w/app".into(),
            branch: Some("main".into()),
        },
    );
    let tall = v.sidebar_layout(Rect::new(0, 0, 30, 20));
    assert!(tall
        .lines
        .iter()
        .any(|(l, _)| matches!(l, SidebarLine::Activity(_))));
    let short = v.sidebar_layout(Rect::new(0, 0, 30, 7));
    assert!(!short
        .lines
        .iter()
        .any(|(l, _)| matches!(l, SidebarLine::Activity(_))));
}

fn with_subs(width: u16) -> ViewState {
    let mut v = with_status(width);
    let mut found = v.status.clone();
    found.get_mut("a").unwrap().subs = vec![
        SubAgent {
            id: "t1".into(),
            label: "Explore: find auth routes".into(),
            started_s: 88,
            agent: None,
        },
        SubAgent {
            id: "t2".into(),
            label: "Plan: review the diff".into(),
            started_s: 95,
            agent: None,
        },
    ];
    assert!(v.set_status(found));
    v
}

fn find_hit(v: &ViewState, pick: impl Fn(&SidebarLine) -> bool) -> (u16, u16) {
    let sb = layout(AREA, v).sidebar.unwrap();
    let sl = v.sidebar_layout(sb);
    let (_, y) = sl.lines.iter().find(|(l, _)| pick(l)).unwrap();
    (sb.x + 6, *y)
}

#[test]
fn the_sidebar_row_counts_running_subagents() {
    let v = with_subs(140);
    assert!(draw(&v, &screens(), 140, 22).contains("claude (+2)"));
}

#[test]
fn clicking_the_activity_line_expands_and_collapses_the_subagent_rows() {
    let mut v = with_subs(140);
    let (x, y) = find_hit(&v, |l| matches!(l, SidebarLine::Activity(id) if id == "a"));
    assert_eq!(v.hit(AREA, x, y), Hit::Subs("a".into()));
    v.apply(Cmd::Subagents);
    assert!(!v.subs_open.contains("a"));
    v.subs_open.insert("a".into());
    let rows = v
        .sidebar_layout(layout(AREA, &v).sidebar.unwrap())
        .lines
        .iter()
        .filter(|(l, _)| matches!(l, SidebarLine::Sub { .. }))
        .count();
    assert_eq!(rows, 2);
    v.toggle_subs("a");
    assert!(v.subs_open.is_empty());
}

#[test]
fn clicking_a_subagent_row_opens_the_read_only_peek() {
    let mut v = with_subs(140);
    v.subs_open.insert("a".into());
    let (x, y) = find_hit(
        &v,
        |l| matches!(l, SidebarLine::Sub { id, .. } if id == "t2"),
    );
    assert_eq!(v.hit(AREA, x, y), Hit::Sub("a".into(), "t2".into()));
    v.open_peek("a", "t2");
    let peek = v.peek.clone().unwrap();
    assert_eq!(peek.title, "Plan: review the diff");
    assert!(v.set_peek("a", "t2", vec!["prompt: x".into()]));
    assert!(!v.set_peek("a", "t2", vec!["prompt: x".into()]));
    assert!(!v.set_peek("a", "other", vec!["y".into()]));
}

#[test]
fn peek_keys_scroll_and_close() {
    let mut v = with_subs(140);
    v.open_peek("a", "t1");
    v.set_peek("a", "t1", (0..30).map(|i| format!("l{i}")).collect());
    assert_eq!(v.edit_peek(b"\x1b[A"), PeekEdit::Keep);
    assert_eq!(v.peek.as_ref().unwrap().back, 1);
    v.edit_peek(b"\x1b[5~");
    assert_eq!(v.peek.as_ref().unwrap().back, 11);
    v.edit_peek(b"G");
    assert_eq!(v.peek.as_ref().unwrap().back, 0);
    assert_eq!(v.edit_peek(b"\x1b"), PeekEdit::Close);
    assert!(v.peek.is_none());
}

#[test]
fn a_session_with_only_background_agents_keeps_an_activity_line() {
    let mut v = with_subs(140);
    v.sessions[0].state.kind = StateKind::Waiting;
    let mut found = v.status.clone();
    found.get_mut("a").unwrap().todo = None;
    found.get_mut("a").unwrap().subs[0].agent = Some("ag".into());
    v.set_status(found);
    assert_eq!(v.status_line("a").unwrap().text, "subagents running");
}

#[test]
fn snapshot_wide_with_expanded_subagents() {
    let mut v = with_subs(140);
    v.subs_open.insert("a".into());
    assert_snapshot("subagents_wide", &draw(&v, &screens(), 140, 22));
}

#[test]
fn snapshot_peek_overlay() {
    let mut v = with_subs(140);
    v.open_peek("a", "t1");
    v.set_peek(
        "a",
        "t1",
        vec![
            "prompt: find the auth routes".into(),
            "Looking at the router.".into(),
            "> Grep: route".into(),
            "Found 3 handlers.".into(),
        ],
    );
    assert_snapshot("subagents_peek", &draw(&v, &screens(), 140, 22));
}

#[test]
fn peek_keys_cap_at_the_top_and_handle_batched_input() {
    let mut v = with_subs(140);
    v.open_peek("a", "t1");
    v.set_peek("a", "t1", (0..30).map(|i| format!("l{i}")).collect());
    v.edit_peek(b"g");
    assert_eq!(v.peek.as_ref().unwrap().back, 29);
    v.edit_peek(b"\x1b[B\x1b[B\x1b[B");
    assert_eq!(v.peek.as_ref().unwrap().back, 26);
    v.edit_peek(b"kkk");
    assert_eq!(v.peek.as_ref().unwrap().back, 29);
    assert_eq!(v.edit_peek(b"\x1b\x1b"), PeekEdit::Close);
}

#[test]
fn scrolled_to_the_top_the_peek_still_shows_a_full_page() {
    let mut v = with_subs(140);
    v.open_peek("a", "t1");
    v.set_peek("a", "t1", (0..30).map(|i| format!("line {i}")).collect());
    v.edit_peek(b"g");
    assert!(draw(&v, &screens(), 140, 22).contains("line 0"));
}

#[test]
fn foreground_subagents_are_hidden_once_the_session_stops_working() {
    let mut v = with_subs(140);
    let mut found = v.status.clone();
    found.get_mut("a").unwrap().subs[1].agent = Some("ag".into());
    v.set_status(found);
    assert_eq!(v.subs_of("a").len(), 2);
    v.sessions[0].state.kind = StateKind::Waiting;
    let left: Vec<_> = v.subs_of("a").iter().map(|s| s.id.clone()).collect();
    assert_eq!(left, vec!["t2".to_string()]);
}

#[test]
fn the_peek_closes_when_its_session_disappears() {
    let mut v = with_subs(140);
    v.open_peek("a", "t1");
    v.sessions.retain(|r| r.id != "a");
    v.set_status(HashMap::new());
    assert!(v.peek.is_none());
}

#[test]
fn any_command_closes_the_peek() {
    let mut v = with_subs(140);
    v.open_peek("a", "t1");
    v.apply(Cmd::Help);
    assert!(v.peek.is_none());
}

struct FakeNotifier {
    bells: usize,
    texts: Vec<String>,
}

impl Notifier for FakeNotifier {
    fn bell(&mut self) {
        self.bells += 1;
    }

    fn notify(&mut self, text: &str) {
        self.texts.push(text.to_string());
    }
}

fn fake() -> FakeNotifier {
    FakeNotifier {
        bells: 0,
        texts: Vec::new(),
    }
}

fn blocked_view() -> ViewState {
    let mut v = ViewState::new(four(), "spark");
    v.sessions[2].state.detail = Some("blocked[0]: Do you want to proceed?".into());
    v
}

fn all_kinds() -> SignalSettings {
    SignalSettings {
        input: true,
        ..SignalSettings::default()
    }
}

#[test]
fn a_permission_prompt_is_a_need_with_the_prompt_line_as_reason() {
    let v = blocked_view();
    let need = detect(&v.sessions[2], None, 0, &SignalSettings::default()).unwrap();
    assert_eq!(need.kind, NeedKind::Permission);
    assert_eq!(need.reason, "Do you want to proceed?");
}

#[test]
fn waiting_is_a_need_only_when_the_input_kind_is_on() {
    let v = blocked_view();
    assert_eq!(
        detect(&v.sessions[1], None, 0, &SignalSettings::default()),
        None
    );
    let need = detect(&v.sessions[1], None, 0, &all_kinds()).unwrap();
    assert_eq!(
        (need.kind, need.reason.as_str()),
        (NeedKind::Input, "waiting for input")
    );
}

#[test]
fn a_pending_ask_user_question_or_an_open_question_is_a_question_need() {
    let v = blocked_view();
    let asked = detect(
        &v.sessions[0],
        Some("Which database?"),
        0,
        &SignalSettings::default(),
    );
    assert_eq!(asked.unwrap().kind, NeedKind::Question);
    let open = detect(&v.sessions[1], None, 2, &SignalSettings::default()).unwrap();
    assert_eq!(
        (open.kind, open.reason.as_str()),
        (NeedKind::Question, "asked 2 questions")
    );
}

#[test]
fn working_and_exited_sessions_never_need_you() {
    let v = blocked_view();
    assert_eq!(detect(&v.sessions[0], None, 0, &all_kinds()), None);
    let done = row("z", "z", StateKind::Exited, "1m");
    assert_eq!(detect(&done, None, 5, &all_kinds()), None);
}

#[test]
fn one_signal_per_wait_and_a_new_wait_signals_again() {
    let mut v = blocked_view();
    let mut t = SignalTracker::new();
    let s = SignalSettings::default();
    v.sessions[2].state.kind = StateKind::Working;
    t.update(&v, &s);
    v.sessions[2].state.kind = StateKind::Blocked;
    v.sessions[2].state.since_ms = 1;
    assert_eq!(t.update(&v, &s).len(), 1);
    assert_eq!(t.update(&v, &s).len(), 0);
    assert_eq!(t.update(&v, &s).len(), 0);
    assert!(t.current().contains_key("c"));
    v.sessions[2].state.kind = StateKind::Working;
    assert_eq!(t.update(&v, &s).len(), 0);
    assert!(t.current().is_empty());
    v.sessions[2].state.kind = StateKind::Blocked;
    v.sessions[2].state.since_ms = 2;
    assert_eq!(t.update(&v, &s).len(), 1);
}

#[test]
fn a_wait_that_escalates_signals_again_once() {
    let mut v = blocked_view();
    let mut t = SignalTracker::new();
    v.sessions[1].state.kind = StateKind::Working;
    t.update(&v, &all_kinds());
    v.sessions[1].state.kind = StateKind::Waiting;
    v.sessions[1].state.since_ms = 3;
    let first = t.update(&v, &all_kinds());
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].1.kind, NeedKind::Input);
    v.status.insert(
        "b".into(),
        SessionStatus {
            ask: Some("Which?".into()),
            ..Default::default()
        },
    );
    let fresh = t.update(&v, &all_kinds());
    assert_eq!(fresh.len(), 1);
    assert_eq!(fresh[0].1.kind, NeedKind::Question);
    assert!(t.update(&v, &all_kinds()).is_empty());
}

#[test]
fn a_disabled_kind_neither_signals_nor_highlights() {
    let v = blocked_view();
    let mut t = SignalTracker::new();
    let off = SignalSettings {
        permission: false,
        ..SignalSettings::default()
    };
    assert!(t.update(&v, &off).is_empty());
    assert!(t.current().is_empty());
}

#[test]
fn delivery_rings_one_bell_and_names_the_session() {
    let v = blocked_view();
    let mut t = SignalTracker::new();
    let s = SignalSettings::default();
    t.update(&ViewState::new(Vec::new(), ""), &s);
    let mut v2 = v.clone();
    v2.sessions[2].state.since_ms = 9;
    t.update(&ViewState::new(four(), ""), &s);
    let fresh = t.update(&v2, &s);
    let mut n = fake();
    let name = |id: &str| {
        v2.sessions
            .iter()
            .find(|r| r.id == id)
            .unwrap()
            .name
            .clone()
    };
    assert_eq!(deliver(&fresh, name, |_| false, &s, 12 * 60, &mut n), 1);
    assert_eq!(n.bells, 1);
    assert_eq!(
        n.texts,
        vec!["codex needs you: Do you want to proceed?".to_string()]
    );
}

#[test]
fn several_new_signals_make_one_notification_and_one_bell() {
    let v = blocked_view();
    let mut n = fake();
    let need = |r: &str| Need {
        kind: NeedKind::Input,
        reason: r.into(),
    };
    let fresh = vec![("a".to_string(), need("x")), ("b".to_string(), need("y"))];
    let name = |id: &str| v.sessions.iter().find(|r| r.id == id).unwrap().name.clone();
    let s = SignalSettings::default();
    assert_eq!(deliver(&fresh, name, |_| false, &s, 600, &mut n), 2);
    assert_eq!((n.bells, n.texts.len()), (1, 1));
    assert_eq!(n.texts[0], "2 sessions need you: claude, dev-server");
}

#[test]
fn sessions_already_waiting_at_attach_do_not_signal() {
    let v = blocked_view();
    let mut t = SignalTracker::new();
    assert!(t.update(&v, &SignalSettings::default()).is_empty());
    assert!(t.current().contains_key("c"));
}

#[test]
fn a_wait_is_identified_by_the_runner_state_not_by_what_was_visible() {
    let mut v = blocked_view();
    let mut t = SignalTracker::new();
    let s = all_kinds();
    t.update(&v, &s);
    v.sessions[1].state.since_ms = 5;
    assert_eq!(t.update(&v, &s).len(), 1);
    let ask = SessionStatus {
        ask: Some("Which?".into()),
        ..Default::default()
    };
    v.status.insert("b".into(), ask);
    assert!(t.update(&v, &s).len() <= 1);
    v.status.clear();
    assert!(t.update(&v, &s).is_empty());
    v.sessions[1].state.since_ms = 6;
    assert_eq!(t.update(&v, &s).len(), 1);
}

#[test]
fn the_reason_hides_secret_looking_values() {
    let mut v = blocked_view();
    v.sessions[2].state.detail = Some("blocked[0]: Allow API_KEY=abc123 curl?".into());
    let need = detect(&v.sessions[2], None, 0, &SignalSettings::default()).unwrap();
    assert!(!need.reason.contains("abc123"), "{}", need.reason);
}

#[test]
fn the_watched_session_and_quiet_hours_stay_silent() {
    let mut s = SignalSettings::default();
    let fresh = vec![(
        "c".to_string(),
        Need {
            kind: NeedKind::Permission,
            reason: "ok?".into(),
        },
    )];
    let mut n = fake();
    assert_eq!(
        deliver(&fresh, |i| i.into(), |id| id == "c", &s, 600, &mut n),
        0
    );
    assert_eq!((n.bells, n.texts.len()), (0, 0));
    s.quiet = Some((22 * 60, 8 * 60));
    assert_eq!(
        deliver(&fresh, |i| i.into(), |_| false, &s, 23 * 60, &mut n),
        0
    );
    assert_eq!(
        deliver(&fresh, |i| i.into(), |_| false, &s, 7 * 60, &mut n),
        0
    );
    assert_eq!(
        deliver(&fresh, |i| i.into(), |_| false, &s, 9 * 60, &mut n),
        1
    );
    s.quiet = Some((9 * 60, 17 * 60));
    assert!(s.is_quiet(12 * 60) && !s.is_quiet(8 * 60));
}

#[test]
fn notification_text_is_sanitised_and_clipped() {
    let need = Need {
        kind: NeedKind::Permission,
        reason: "run \x1b[31mrm\x07 now".into(),
    };
    let mut n = fake();
    deliver(
        &[("x".into(), need)],
        |_| format!("bad\x1b]0;t\x07{}", "n".repeat(100)),
        |_| false,
        &SignalSettings::default(),
        0,
        &mut n,
    );
    assert!(!n.texts[0].contains('\x1b') && !n.texts[0].contains('\x07'));
    assert!(n.texts[0].chars().count() < 100);
}

#[test]
fn the_jump_key_cycles_through_sessions_that_need_you() {
    let mut v = blocked_view();
    let mut t = SignalTracker::new();
    t.update(&v, &all_kinds());
    v.needs = t.current().clone();
    v.apply(Cmd::NeedsYou);
    let first = v.focused_session().unwrap().to_string();
    assert!(v.needs.contains_key(&first));
    v.apply(Cmd::NeedsYou);
    let second = v.focused_session().unwrap().to_string();
    assert_ne!(first, second);
    v.needs.clear();
    v.apply(Cmd::NeedsYou);
    assert_eq!(v.notice.as_deref(), Some("nothing needs you"));
}

#[test]
fn snapshot_sidebar_highlights_a_session_that_needs_you() {
    let mut v = two_pane(140);
    v.sessions[1].state.kind = StateKind::Waiting;
    let mut t = SignalTracker::new();
    t.update(&v, &all_kinds());
    v.needs = t.current().clone();
    assert!(!v.needs.is_empty());
    assert_snapshot("needs_you", &draw(&v, &screens(), 140, 22));
    let (buf, _) = (draw(&v, &screens(), 140, 22), 0);
    assert!(buf.contains("dev-server"));
}

fn chip() -> TaskChip {
    TaskChip {
        key: "TM-644".into(),
        branch: Some("tm-644-open-desktop".into()),
        started_s: Some(58),
        workspace: Some("tmgrdev".into()),
        task_id: Some(644),
        run_id: None,
        run_ws: None,
    }
}

fn with_chip(width: u16) -> ViewState {
    let mut v = two_pane(width);
    v.now_s = 100;
    let mut found = HashMap::new();
    found.insert("a".to_string(), chip());
    assert!(v.set_chips(found));
    v
}

#[test]
fn chip_text_shrinks_branch_first_then_the_clock_then_vanishes() {
    let c = chip();
    assert_eq!(
        chip_text(&c, 100, 60).unwrap(),
        " · TM-644 · tm-644-open-desktop · 0:42"
    );
    let mid = chip_text(&c, 100, 30).unwrap();
    assert!(
        mid.starts_with(" · TM-644 · tm-6") && mid.ends_with("· 0:42"),
        "{mid}"
    );
    assert_eq!(chip_text(&c, 100, 16).unwrap(), " · TM-644 · 0:42");
    assert_eq!(chip_text(&c, 100, 9).unwrap(), " · TM-644");
    assert_eq!(chip_text(&c, 100, 8), None);
}

#[test]
fn chip_text_omits_missing_parts() {
    let mut c = chip();
    c.branch = None;
    c.started_s = None;
    assert_eq!(chip_text(&c, 100, 40).unwrap(), " · TM-644");
}

#[test]
fn open_urls_use_the_validated_workspace_and_id_only() {
    let c = chip();
    let (desktop, web) = open_urls("https://tmgr.dev/", &c).unwrap();
    assert_eq!(desktop, "tmgr://open?path=%2Ftmgrdev%2Ftasks%2F644");
    assert_eq!(web, "https://tmgr.dev/tmgrdev/tasks/644");
    for bad in ["a b", "x/../y", "", "ws?x=1", "w\\u{1b}"] {
        let mut c = chip();
        c.workspace = Some(bad.into());
        assert_eq!(open_urls("https://tmgr.dev", &c), None, "{bad}");
    }
    let mut c = chip();
    c.task_id = None;
    assert_eq!(open_urls("https://tmgr.dev", &c), None);
    c = chip();
    c.workspace = None;
    assert_eq!(open_urls("https://tmgr.dev", &c), None);
}

#[test]
fn the_wide_header_shows_the_chip_and_a_click_on_it_opens_the_task() {
    let v = with_chip(140);
    let text = draw(&v, &screens(), 140, 22);
    let first = text.lines().next().unwrap();
    assert!(
        first.contains("claude · working · TM-644 · tm-644-") && first.contains("· 0:42"),
        "{first}"
    );
    let lay = layout(AREA, &v);
    let pane = lay.panes.iter().find(|p| p.session == "a").unwrap();
    let (x, w) = render::header_chip(&v, pane).unwrap();
    let col = first.chars().take_while(|_| true).collect::<Vec<_>>();
    let drawn = col
        .windows(6)
        .position(|win| win.iter().collect::<String>() == "TM-644")
        .unwrap();
    assert_eq!(usize::from(x), drawn);
    assert_eq!(v.hit(AREA, x, pane.outer.y), Hit::Chip("a".into()));
    assert_eq!(v.hit(AREA, x + w - 1, pane.outer.y), Hit::Chip("a".into()));
    assert_ne!(v.hit(AREA, x + w, pane.outer.y), Hit::Chip("a".into()));
    assert_ne!(v.hit(AREA, x - 1, pane.outer.y), Hit::Chip("a".into()));
}

#[test]
fn a_pane_without_a_task_has_no_chip() {
    let v = with_chip(140);
    let lay = layout(AREA, &v);
    let other = lay.panes.iter().find(|p| p.session == "b").unwrap();
    assert_eq!(render::header_chip(&v, other), None);
}

#[test]
fn a_narrow_header_drops_the_chip_before_the_title_and_the_state() {
    let v = with_chip(90);
    let text = draw(&v, &screens(), 90, 16);
    let first = text.lines().next().unwrap();
    assert!(first.contains("claude · working"), "{first}");
    let tiny = draw(&v, &screens(), 40, 16);
    assert!(tiny.lines().next().unwrap().contains("claude"));
    assert!(!tiny.lines().next().unwrap().contains("tm-644"));
}

#[test]
fn the_open_task_key_is_free_and_listed() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"\x1dt"), vec![Action::Cmd(Cmd::OpenTask)]);
    assert!(ACTIONS.iter().any(|d| d.cmd == Cmd::OpenTask && d.palette));
}

#[test]
fn chips_of_vanished_sessions_are_dropped() {
    let mut v = with_chip(140);
    let mut found = v.chips.clone();
    found.insert("ghost".into(), chip());
    assert!(!v.set_chips(found));
}

#[test]
fn snapshot_wide_with_a_task_chip() {
    let v = with_chip(140);
    assert_snapshot("task_chip_wide", &draw(&v, &screens(), 140, 22));
}

#[test]
fn the_chip_coexists_with_the_question_badge_and_activity() {
    let mut v = with_status(140);
    v.chips.insert("a".into(), chip());
    let text = draw(&v, &screens(), 140, 22);
    assert!(text.lines().next().unwrap().contains("TM-644"), "{text}");
}

fn three_panes() -> ViewState {
    let mut v = ViewState::new(four(), "local");
    v.width = 160;
    assert!(v.split_focused(SplitDir::Right, "b"));
    assert!(v.split_focused(SplitDir::Right, "c"));
    v.focus_pane(0);
    v
}

fn need(v: &mut ViewState, id: &str) {
    v.needs.insert(
        id.into(),
        Need {
            kind: NeedKind::Permission,
            reason: "Run rm".into(),
        },
    );
}

#[test]
fn broadcast_is_off_by_default_and_never_saved() {
    let mut v = three_panes();
    assert!(v.broadcast.is_none());
    assert!(v.broadcast_receivers().is_empty());
    let before = serde_json::to_string(&v.prefs()).unwrap();
    v.apply(Cmd::Broadcast);
    assert!(v.broadcast.is_some());
    assert_eq!(serde_json::to_string(&v.prefs()).unwrap(), before);
    let mut fresh = ViewState::new(four(), "local");
    fresh.apply_prefs(&v.prefs());
    assert!(fresh.broadcast.is_none());
    v.apply(Cmd::Broadcast);
    assert!(v.broadcast.is_none());
    assert_eq!(v.notice.as_deref(), Some("broadcast off"));
}

#[test]
fn broadcast_key_is_shift_b_and_mark_is_m() {
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(&[0x1d, b'B']), vec![Action::Cmd(Cmd::Broadcast)]);
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(&[0x1d, b'm']), vec![Action::Cmd(Cmd::BroadcastMark)]);
}

#[test]
fn all_visible_skips_blocked_exited_and_needs_you_panes() {
    let mut v = three_panes();
    v.apply(Cmd::Broadcast);
    assert_eq!(v.broadcast_receivers(), ["a", "b"]);
    assert_eq!(v.broadcast_peers("a"), ["b"]);
    need(&mut v, "b");
    assert_eq!(v.broadcast_receivers(), ["a"]);
    assert!(v.broadcast_peers("a").is_empty());
}

#[test]
fn a_hidden_pane_never_receives() {
    let mut v = ViewState::new(four(), "local");
    v.apply(Cmd::Broadcast);
    assert_eq!(v.broadcast_receivers(), ["a"]);
}

#[test]
fn picking_panes_starts_from_the_current_targets_and_can_include_a_needs_you_pane() {
    let mut v = three_panes();
    v.apply(Cmd::Broadcast);
    need(&mut v, "b");
    v.apply(Cmd::BroadcastMark);
    assert!(v.broadcast_receivers().is_empty());
    v.focus_pane(1);
    v.apply(Cmd::BroadcastMark);
    assert_eq!(v.broadcast_receivers(), ["b"]);
    v.focus_pane(2);
    v.apply(Cmd::BroadcastMark);
    assert_eq!(v.broadcast_receivers(), ["b", "c"]);
    assert_eq!(v.broadcast_peers("b"), ["c"]);
    assert!(v.broadcast_peers("a").is_empty());
}

#[test]
fn a_carried_pane_that_starts_asking_stops_receiving_after_a_pick() {
    let mut v = three_panes();
    v.apply(Cmd::Broadcast);
    v.apply(Cmd::BroadcastMark);
    v.apply(Cmd::BroadcastMark);
    assert_eq!(v.broadcast_receivers(), ["a", "b"]);
    need(&mut v, "b");
    assert_eq!(v.broadcast_receivers(), ["a"]);
}

#[test]
fn an_open_question_or_ask_blocks_even_when_signals_are_filtered_out() {
    let mut v = three_panes();
    v.apply(Cmd::Broadcast);
    v.status.insert(
        "b".into(),
        SessionStatus {
            ask: Some("Which one?".into()),
            ..Default::default()
        },
    );
    assert_eq!(v.broadcast_receivers(), ["a"]);
}

#[test]
fn plain_shells_are_left_out_unless_picked() {
    let mut v = three_panes();
    v.shells.push("b".into());
    v.apply(Cmd::Broadcast);
    assert_eq!(v.broadcast_receivers(), ["a"]);
    v.focus_pane(1);
    v.apply(Cmd::BroadcastMark);
    assert_eq!(v.broadcast_receivers(), ["a", "b"]);
}

#[test]
fn zoom_leaves_only_the_visible_pane() {
    let mut v = three_panes();
    v.apply(Cmd::Broadcast);
    v.apply(Cmd::Zoom);
    assert_eq!(v.broadcast_receivers(), ["a"]);
    assert!(v.broadcast_peers("a").is_empty());
}

#[test]
fn typing_in_an_unmarked_pane_goes_nowhere_else() {
    let mut v = three_panes();
    v.apply(Cmd::Broadcast);
    v.apply(Cmd::BroadcastMark);
    assert!(!v.broadcast_marked("a"));
    assert!(v.broadcast_peers("a").is_empty());
}

#[test]
fn a_resumed_session_keeps_its_pick() {
    let mut v = three_panes();
    v.apply(Cmd::Broadcast);
    v.apply(Cmd::BroadcastMark);
    v.replace_session("a", "z");
    assert!(v.broadcast.as_ref().unwrap().off.contains(&"z".to_string()));
}

#[test]
fn peers_only_get_text_enter_tab_and_backspace() {
    assert_eq!(peer_bytes(b"fix it\r"), b"fix it\r");
    assert_eq!(peer_bytes("héllo ✓".as_bytes()), "héllo ✓".as_bytes());
    assert_eq!(
        peer_bytes(b"\x03\x04\x1b\x1b[Zx\x1b[A\x1bOP\t\x7f"),
        b"x\t\x7f"
    );
    assert!(peer_bytes(b"\xff\xfe").is_empty());
}

#[test]
fn an_exited_pane_is_never_a_target_even_when_picked() {
    let mut v = three_panes();
    assert!(v.split_focused(SplitDir::Right, "d"));
    v.apply(Cmd::Broadcast);
    v.toggle_broadcast_pick("d");
    assert!(!v.broadcast_marked("d"));
}

#[test]
fn marking_while_off_does_nothing() {
    let mut v = three_panes();
    v.apply(Cmd::BroadcastMark);
    assert!(v.broadcast.is_none());
    assert_eq!(
        v.notice.as_deref(),
        Some("broadcast is off, Ctrl-] B turns it on")
    );
}

#[test]
fn clicking_the_marker_toggles_that_pane() {
    let mut v = three_panes();
    v.apply(Cmd::Broadcast);
    let lay = layout(AREA, &v);
    let p = &lay.panes[1];
    let r = super::broadcast::marker_rect(p, &v).unwrap();
    let out = at(&mut v, 0, r.x + 1, r.y, false);
    assert!(out.redraw);
    assert_eq!(v.broadcast_receivers(), ["a"]);
    at(&mut v, 0, r.x + 1, r.y, true);
    let out = at(&mut v, 0, r.x + 1, r.y, false);
    assert!(out.redraw);
    assert_eq!(v.broadcast_receivers(), ["a", "b"]);
}

#[test]
fn snapshot_broadcast_markers() {
    let mut v = two_pane(140);
    v.apply(Cmd::Broadcast);
    v.apply(Cmd::BroadcastMark);
    assert_snapshot("broadcast_wide", &draw(&v, &screens(), 140, 16));
}

#[test]
fn snapshot_broadcast_narrow_panes_use_short_markers() {
    let mut v = two_pane(60);
    v.apply(Cmd::Broadcast);
    v.apply(Cmd::BroadcastMark);
    assert_snapshot("broadcast_narrow", &draw(&v, &screens(), 60, 14));
}

fn git_info() -> GitInfo {
    GitInfo {
        root: "/work/shop".into(),
        branch: "feature/apple-pay".into(),
        upstream: Some("origin/feature/apple-pay".into()),
        ahead: 1,
        behind: 0,
        changed: 3,
        untracked: 1,
        added: 120,
        deleted: 34,
        base: Some("origin/main".into()),
        ahead_base: Some(2),
        pr: PrLookup::Found(Pr {
            number: 615,
            title: "Apple Pay button".into(),
            url: "https://github.com/acme/shop/pull/615".into(),
            state: "open".into(),
        }),
    }
}

fn git_view(width: u16) -> ViewState {
    let mut v = two_pane(width);
    let focused = v.focused_session().unwrap().to_string();
    let mut found = HashMap::new();
    found.insert(focused, git_info());
    v.set_git(found);
    v
}

#[test]
fn status_v2_is_parsed_into_branch_tracking_and_counts() {
    let text = "# branch.oid abc\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +2 -1\n1 .M N... 100644 100644 100644 a b src/a.rs\n2 R. N... 100644 100644 100644 a b R100 new.rs\told.rs\nu UU N... 1 1 1 1 a b c conflict.rs\n? scratch.txt\n? other.txt\n";
    assert_eq!(
        parse_status(text),
        StatusInfo {
            branch: "main".into(),
            upstream: Some("origin/main".into()),
            ahead: 2,
            behind: 1,
            changed: 3,
            untracked: 2,
        }
    );
    let detached = parse_status("# branch.head (detached)\n");
    assert_eq!(detached.branch, "(detached)");
    assert_eq!(detached.upstream, None);
    assert_eq!(parse_status(""), StatusInfo::default());
}

#[test]
fn numstat_sums_text_files_and_skips_binaries() {
    assert_eq!(
        parse_numstat("3\t1\ta.rs\n-\t-\timg.png\n10\t0\tb.rs\n"),
        (13, 1)
    );
    assert_eq!(parse_numstat(""), (0, 0));
}

#[test]
fn only_github_pull_urls_are_accepted() {
    assert!(valid_pr_url("https://github.com/acme/shop/pull/615"));
    for bad in [
        "http://github.com/acme/shop/pull/615",
        "https://github.com.evil.io/acme/shop/pull/1",
        "https://github.com/acme/shop/pull/1/files",
        "https://github.com/acme/shop/issues/1",
        "https://github.com/acme/shop/pull/x",
        "https://github.com/../shop/pull/1",
        "https://github.com/acme/sh op/pull/1",
        "file:///etc/passwd",
        "-a",
        "",
    ] {
        assert!(!valid_pr_url(bad), "{bad}");
    }
}

#[test]
fn a_gh_answer_is_parsed_and_a_bad_one_is_dropped() {
    let ok = r#"{"number":7,"title":"Fix \u001b[2J it","url":"https://github.com/a/b/pull/7","state":"OPEN"}"#;
    let pr = parse_pr(ok).unwrap();
    assert_eq!((pr.number, pr.state.as_str()), (7, "open"));
    assert!(!pr.title.contains('\u{1b}'));
    assert!(parse_pr(
        r#"{"number":7,"title":"x","url":"https://evil.example/a/b/pull/7","state":"OPEN"}"#
    )
    .is_none());
    assert!(parse_pr("not json").is_none());
}

#[test]
fn the_git_key_is_free_and_the_panel_is_hidden_without_a_repo() {
    let keys: Vec<_> = ACTIONS.iter().filter(|a| a.keys.contains(&b'G')).collect();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].cmd, Cmd::Git);
    let mut v = two_pane(140);
    v.apply(Cmd::Git);
    assert!(!v.git_open);
    assert!(v.notice.as_deref().unwrap().contains("no git repository"));
    let mut v = git_view(140);
    v.apply(Cmd::Git);
    assert!(v.git_open);
    v.apply(Cmd::Git);
    assert!(!v.git_open);
}

#[test]
fn the_panel_closes_when_the_repo_disappears() {
    let mut v = git_view(140);
    v.apply(Cmd::Git);
    assert!(v.set_git(HashMap::new()));
    assert!(!v.git_open);
}

#[test]
fn panel_keys_pick_actions_and_open_pr_needs_a_pr() {
    let mut v = git_view(140);
    v.apply(Cmd::Git);
    assert_eq!(v.edit_git(b"d"), Some(GitAction::Diff));
    assert_eq!(v.edit_git(b"p"), Some(GitAction::OpenPr));
    assert_eq!(v.edit_git(b"x"), None);
    assert!(v.git_open);
    assert_eq!(v.edit_git(b"\x1b"), Some(GitAction::Close));
    assert!(!v.git_open);
    let mut v = git_view(140);
    let id = v.focused_session().unwrap().to_string();
    v.git.get_mut(&id).unwrap().pr = PrLookup::NoPr;
    v.apply(Cmd::Git);
    assert_eq!(v.edit_git(b"p"), None);
}

#[test]
fn clicking_panel_buttons_returns_actions_and_outside_closes() {
    let mut v = git_view(140);
    v.apply(Cmd::Git);
    let info = mouse_info(sgr_mode(MouseLevel::Any), 0);
    let screen = draw(&v, &screens(), AREA.width, AREA.height);
    let (row, col) = screen
        .lines()
        .enumerate()
        .find_map(|(y, l)| {
            l.find("Open PR")
                .map(|x| (y as u16, l[..x].chars().count() as u16))
        })
        .unwrap();
    let out = v.on_mouse(AREA, ev(0, col, row, false), &info);
    assert_eq!(out.git, Some(GitAction::OpenPr));
    v.on_mouse(AREA, ev(0, col, row, true), &info);
    let out = v.on_mouse(AREA, ev(0, 1, 1, false), &info);
    assert_eq!(out.git, None);
    assert!(!v.git_open);
}

#[test]
fn badge_shows_branch_dirty_count_and_commits_ahead_and_shrinks() {
    let g = git_info();
    assert_eq!(g.badge(60).as_deref(), Some(" · ⎇ feature/apple-pay ±4 ↑2"));
    let short = g.badge(22).unwrap();
    assert!(short.contains('…') && short.contains("±4"), "{short}");
    assert_eq!(g.badge(8), None);
    let clean = GitInfo {
        changed: 0,
        untracked: 0,
        ahead_base: Some(0),
        ..git_info()
    };
    assert_eq!(clean.badge(60).as_deref(), Some(" · ⎇ feature/apple-pay"));
}

#[test]
fn clicking_the_header_badge_opens_the_panel() {
    let mut v = git_view(140);
    let info = mouse_info(sgr_mode(MouseLevel::Any), 0);
    let lay = layout(AREA, &v);
    let p = lay
        .panes
        .iter()
        .find(|p| Some(p.session.as_str()) == v.focused_session())
        .unwrap();
    let (x, w) = super::render::header_git(&v, p).unwrap();
    assert!(w > 4);
    v.on_mouse(AREA, ev(0, x + 1, p.outer.y, false), &info);
    assert!(v.git_open);
}

#[test]
fn snapshot_git_panel_and_header_badge() {
    let mut v = git_view(140);
    assert_snapshot("git_badge", &draw(&v, &screens(), 140, 16));
    v.apply(Cmd::Git);
    assert_snapshot("git_panel", &draw(&v, &screens(), 140, 22));
}

#[test]
fn snapshot_git_panel_narrow_without_pr() {
    let mut v = git_view(60);
    let id = v.focused_session().unwrap().to_string();
    let g = v.git.get_mut(&id).unwrap();
    g.pr = PrLookup::Unavailable;
    g.upstream = None;
    v.apply(Cmd::Git);
    assert_snapshot("git_panel_narrow", &draw(&v, &screens(), 60, 14));
}

fn prompt_view() -> ViewState {
    let mut v = two_pane(140);
    v.prompts = default_prompts();
    v.apply(Cmd::Prompts);
    v
}

fn prompt_names(v: &ViewState) -> Vec<String> {
    v.entries().into_iter().map(|r| r.label).collect()
}

#[test]
fn the_prompt_key_is_free_and_opens_the_prompt_list() {
    let keys: Vec<_> = ACTIONS.iter().filter(|a| a.keys.contains(&b'i')).collect();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].cmd, Cmd::Prompts);
    let v = prompt_view();
    assert_eq!(
        prompt_names(&v),
        [
            "run tests",
            "commit and open PR",
            "summarize what you did",
            "+ Add prompt"
        ]
    );
}

#[test]
fn prompts_are_filtered_by_fuzzy_query_and_keep_the_add_row() {
    let mut v = prompt_view();
    v.insert_palette_text(b"summ");
    assert_eq!(prompt_names(&v), ["summarize what you did", "+ Add prompt"]);
}

#[test]
fn enter_on_a_prompt_returns_it_and_closes() {
    let mut v = prompt_view();
    v.insert_palette_text(b"commit");
    assert_eq!(
        v.edit_palette(b"\r"),
        PaletteEdit::Run(PaletteEntry::Prompt(1))
    );
    assert!(v.palette.is_none());
}

#[test]
fn adding_a_prompt_through_the_form_saves_it() {
    let mut v = prompt_view();
    v.insert_palette_text(b"zzzz");
    assert_eq!(v.edit_palette(b"\r"), PaletteEdit::Editing);
    assert!(matches!(
        v.palette.as_ref().unwrap().mode,
        PaletteMode::Form(_)
    ));
    assert_eq!(v.edit_palette(b"\r"), PaletteEdit::Editing);
    v.edit_palette(b"lint");
    v.edit_palette(b"\t");
    v.edit_palette(b"Run the linter");
    assert_eq!(v.edit_palette(b"\r"), PaletteEdit::PromptsChanged);
    assert_eq!(
        v.prompts.last().unwrap(),
        &Prompt {
            name: "lint".into(),
            text: "Run the linter".into()
        }
    );
    assert_eq!(v.palette.as_ref().unwrap().mode, PaletteMode::Prompts);
}

#[test]
fn editing_replaces_in_place_and_escape_cancels_the_form() {
    let mut v = prompt_view();
    assert_eq!(v.edit_palette(b"\x05"), PaletteEdit::Editing);
    v.edit_palette(b"\x15new name");
    assert_eq!(v.edit_palette(b"\r"), PaletteEdit::PromptsChanged);
    assert_eq!(v.prompts[0].name, "new name");
    assert_eq!(v.prompts.len(), 3);
    v.edit_palette(b"\x05");
    v.edit_palette(b"zz");
    assert_eq!(v.edit_palette(b"\x1b"), PaletteEdit::Editing);
    assert_eq!(v.prompts[0].name, "new name");
    assert_eq!(v.palette.as_ref().unwrap().mode, PaletteMode::Prompts);
}

#[test]
fn an_empty_name_or_text_is_not_saved() {
    let mut v = prompt_view();
    v.edit_palette(b"\x01");
    v.edit_palette(b"only a name");
    assert_eq!(v.edit_palette(b"\r"), PaletteEdit::Editing);
    assert_eq!(v.prompts.len(), 3);
}

#[test]
fn delete_needs_a_confirmation() {
    let mut v = prompt_view();
    v.edit_palette(b"\x04");
    assert_eq!(v.edit_palette(b"n"), PaletteEdit::Editing);
    assert_eq!(v.prompts.len(), 3);
    v.edit_palette(b"\x04");
    assert_eq!(v.edit_palette(b"\x04"), PaletteEdit::Editing);
    assert_eq!(v.prompts.len(), 3);
    v.edit_palette(b"\x04");
    assert_eq!(v.edit_palette(b"y"), PaletteEdit::PromptsChanged);
    assert_eq!(v.prompts.len(), 2);
    assert_eq!(v.prompts[0].name, "commit and open PR");
}

#[test]
fn pasted_text_in_the_form_loses_control_characters() {
    let mut v = prompt_view();
    v.edit_palette(b"\x01");
    v.edit_palette(b"\t");
    v.insert_palette_text(b"\x1b[200~a\nb\x1b[201~");
    let PaletteMode::Form(f) = &v.palette.as_ref().unwrap().mode else {
        panic!()
    };
    assert_eq!(f.text, "a b");
}

#[test]
fn prompt_paste_bytes_are_bracketed_and_clean() {
    assert_eq!(
        paste_bytes("run\x1b[201~ it\n\x03\u{202e}x", true),
        b"\x1b[200~run[201~ it x\x1b[201~".to_vec()
    );
    assert_eq!(paste_bytes("hi", false), b"hi".to_vec());
}

#[test]
fn prompts_file_round_trips_and_drops_empty_entries() {
    let list = vec![Prompt {
        name: "a".into(),
        text: "say \"hi\"".into(),
    }];
    assert_eq!(parse_prompts(&render_prompts(&list)).unwrap(), list);
    let text = "[[prompt]]\nname=\"x\"\ntext=\"\"\n[[prompt]]\nname=\"y\"\ntext=\"t\"\n";
    assert_eq!(parse_prompts(text).unwrap().len(), 1);
    assert!(parse_prompts("prompt = [[[").is_err());
}

#[test]
fn snapshot_prompt_list() {
    let v = prompt_view();
    assert_snapshot("prompts_list", &draw(&v, &screens(), 140, 22));
}

#[test]
fn snapshot_prompt_form() {
    let mut v = prompt_view();
    v.edit_palette(b"\x05");
    assert_snapshot("prompts_form", &draw(&v, &screens(), 140, 22));
}

#[test]
fn snapshot_prompt_list_narrow() {
    let v = {
        let mut v = two_pane(60);
        v.prompts = default_prompts();
        v.apply(Cmd::Prompts);
        v
    };
    assert_snapshot("prompts_narrow", &draw(&v, &screens(), 60, 14));
}

fn lines_of(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn find_view(lines: &[&str], cols: usize) -> ViewState {
    let mut v = ViewState::new(four(), "local");
    v.width = 100;
    v.scroll = Some(ScrollState {
        offset: 0,
        max: 100,
    });
    v.set_scroll_text(
        ScrollText {
            session: "a".into(),
            lines: lines_of(lines),
            cols,
        },
        10,
    );
    v
}

fn type_search(v: &mut ViewState, text: &str) -> SearchEdit {
    v.open_search();
    v.edit_search(text.as_bytes(), 10)
}

#[test]
fn search_finds_every_hit_and_smart_case_decides_sensitivity() {
    let lines = lines_of(&["Error: build failed", "error again", "no match"]);
    let hits = super::find::find_hits(&lines, 40, "error");
    assert_eq!(hits.len(), 2);
    assert_eq!(super::find::find_hits(&lines, 40, "Error").len(), 1);
    assert!(super::find::find_hits(&lines, 40, "").is_empty());
}

#[test]
fn search_joins_wrapped_rows_but_not_separate_lines() {
    let lines = lines_of(&["0123456789", "abc def", "tail"]);
    let hits = super::find::find_hits(&lines, 10, "89ab");
    assert_eq!(hits.len(), 1);
    assert_eq!(
        (hits[0].l0, hits[0].c0, hits[0].l1, hits[0].c1),
        (0, 8, 1, 2)
    );
    assert!(super::find::find_hits(&lines, 10, "deftail").is_empty());
}

#[test]
fn typing_a_query_marks_hits_starts_at_the_newest_and_scrolls_to_it() {
    let mut lines: Vec<String> = (0..60).map(|i| format!("line {i}")).collect();
    lines[5] = "needle old".into();
    lines[40] = "needle new".into();
    let mut v = ViewState::new(four(), "local");
    v.scroll = Some(ScrollState {
        offset: 0,
        max: 100,
    });
    v.set_scroll_text(
        ScrollText {
            session: "a".into(),
            lines,
            cols: 40,
        },
        10,
    );
    assert_eq!(type_search(&mut v, "needle"), SearchEdit::Editing);
    let s = v.search.as_ref().unwrap();
    assert_eq!(s.hits.len(), 2);
    assert_eq!(s.current, Some(1));
    let off = v.scroll.unwrap().offset;
    assert!(off >= 14, "{off}");
    v.search_step(true, 10);
    assert_eq!(v.search.as_ref().unwrap().current, Some(0));
    assert!(v.scroll.unwrap().offset >= 45);
    v.search_step(true, 10);
    assert_eq!(v.search.as_ref().unwrap().current, Some(1));
    v.search_step(false, 10);
    assert_eq!(v.search.as_ref().unwrap().current, Some(0));
}

#[test]
fn enter_keeps_the_search_and_an_empty_result_or_esc_cancels() {
    let mut v = find_view(&["alpha", "beta"], 40);
    v.open_search();
    v.edit_search(b"alp", 10);
    assert_eq!(v.edit_search(b"\r", 10), SearchEdit::Keep);
    assert!(!v.search.as_ref().unwrap().input);
    let mut v = find_view(&["alpha"], 40);
    v.open_search();
    v.edit_search(b"zzz", 10);
    assert_eq!(v.edit_search(b"\r", 10), SearchEdit::Cancel);
    let mut v = find_view(&["alpha"], 40);
    v.open_search();
    assert_eq!(v.edit_search(b"\x1b", 10), SearchEdit::Cancel);
}

#[test]
fn backspace_and_ctrl_u_edit_the_query_and_hits_follow() {
    let mut v = find_view(&["alpha", "alpine"], 40);
    v.open_search();
    v.edit_search(b"alpi", 10);
    assert_eq!(v.search.as_ref().unwrap().hits.len(), 1);
    v.edit_search(b"\x7f", 10);
    assert_eq!(v.search.as_ref().unwrap().hits.len(), 2);
    v.edit_search(b"\x15", 10);
    assert!(v.search.as_ref().unwrap().hits.is_empty());
}

#[test]
fn n_without_a_search_says_so() {
    let mut v = find_view(&["alpha"], 40);
    v.scroll_key(ScrollKey::Next, 9);
    assert_eq!(v.notice.as_deref(), Some("no search yet, / starts one"));
}

#[test]
fn selection_text_joins_wrapped_rows_and_strips_controls() {
    let lines = lines_of(&["0123456789", "abc", "x\u{1b}[31my"]);
    assert_eq!(
        super::find::selection_text(&lines, 10, (0, 8), (1, 1)),
        "89ab"
    );
    assert_eq!(
        super::find::selection_text(&lines, 10, (1, 0), (2, 5)),
        "abc
x[31my"
    );
    assert_eq!(
        super::find::selection_text(&lines, 10, (1, 2), (1, 0)),
        "abc"
    );
}

#[test]
fn copy_mode_moves_selects_and_yields_the_text_without_pasting_anywhere() {
    let mut v = find_view(&["first line", "second line", "third"], 40);
    v.scroll_key(ScrollKey::CopyMode, 9);
    let c = v.copy.clone().unwrap();
    assert_eq!((c.line, c.col), (2, 0));
    v.scroll_key(ScrollKey::Up, 9);
    v.scroll_key(ScrollKey::Select, 9);
    v.scroll_key(ScrollKey::Right, 9);
    v.scroll_key(ScrollKey::Right, 9);
    v.scroll_key(ScrollKey::Down, 9);
    v.scroll_key(ScrollKey::Yank, 9);
    assert_eq!(v.take_copied().as_deref(), Some("second line\nthi"));
    assert!(v.copy.is_none() && v.scroll.is_none());
    assert!(v.take_copied().is_none());
}

#[test]
fn y_without_a_selection_copies_the_cursor_line_and_esc_leaves_in_steps() {
    let mut v = find_view(&["first", "second"], 40);
    v.scroll_key(ScrollKey::CopyMode, 9);
    v.scroll_key(ScrollKey::Yank, 9);
    assert_eq!(v.take_copied().as_deref(), Some("second"));

    let mut v = find_view(&["first", "second"], 40);
    v.scroll_key(ScrollKey::Select, 9);
    assert!(v.copy.as_ref().unwrap().anchor.is_some());
    v.scroll_key(ScrollKey::Exit, 9);
    assert!(v.copy.as_ref().unwrap().anchor.is_none() && v.scroll.is_some());
    v.scroll_key(ScrollKey::Exit, 9);
    assert!(v.copy.is_none() && v.scroll.is_none() && v.stext.is_none());
}

#[test]
fn copy_cursor_stays_inside_the_text_and_the_viewport_follows_it() {
    let lines: Vec<String> = (0..40).map(|i| format!("row {i}")).collect();
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let mut v = find_view(&refs, 40);
    v.scroll_key(ScrollKey::CopyMode, 9);
    for _ in 0..200 {
        v.scroll_key(ScrollKey::Up, 9);
    }
    assert_eq!(v.copy.as_ref().unwrap().line, 0);
    assert!(v.scroll.unwrap().offset >= 30);
    v.scroll_key(ScrollKey::LineEnd, 9);
    assert_eq!(v.copy.as_ref().unwrap().col, 4);
    v.scroll_key(ScrollKey::Bottom, 9);
    assert_eq!(v.copy.as_ref().unwrap().line, 39);
    assert_eq!(v.scroll.unwrap().offset, 0);
}

#[test]
fn copy_mode_keys_map_from_the_scroll_mode_splitter() {
    let mut s = InputSplitter::new();
    s.set_scroll_mode(true);
    let keys = s.feed(b"/nNcvyhjkl0$");
    let want = [
        ScrollKey::Search,
        ScrollKey::Next,
        ScrollKey::Prev,
        ScrollKey::CopyMode,
        ScrollKey::Select,
        ScrollKey::Yank,
        ScrollKey::Left,
        ScrollKey::Down,
        ScrollKey::Up,
        ScrollKey::Right,
        ScrollKey::LineStart,
        ScrollKey::LineEnd,
    ];
    assert_eq!(
        keys,
        want.iter().map(|k| Action::Scroll(*k)).collect::<Vec<_>>()
    );
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(&[0x1d, b'/']), vec![Action::Cmd(Cmd::Search)]);
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(&[0x1d, b'y']), vec![Action::Cmd(Cmd::CopyMode)]);
}

fn find_screens() -> Screens {
    let mut s = Screens::default();
    s.0.insert(
        "a".into(),
        snap(
            4,
            40,
            &[
                vec![("alpha beta", plain())],
                vec![("gamma", plain())],
                vec![("beta again beta", plain())],
                vec![("> ", plain())],
            ],
            None,
        ),
    );
    s
}

fn mods(term: &Terminal<TestBackend>, x: u16, y: u16) -> Modifier {
    term.backend().buffer()[(x, y)].modifier
}

#[test]
fn hits_are_highlighted_in_the_visible_rows_and_the_bar_shows_the_count() {
    let mut v = two_pane(140);
    v.focus_pane(0);
    v.scroll = Some(ScrollState { offset: 0, max: 0 });
    v.set_scroll_text(
        ScrollText {
            session: "a".into(),
            lines: lines_of(&["alpha beta", "gamma", "beta again beta", "> "]),
            cols: 40,
        },
        4,
    );
    v.open_search();
    v.edit_search(b"beta", 4);
    v.edit_search(b"\r", 4);
    let mut term = Terminal::new(TestBackend::new(140, 16)).unwrap();
    term.draw(|f| render(f, &v, &find_screens())).unwrap();
    let lay = layout(Rect::new(0, 0, 140, 16), &v);
    let inner = lay.panes[0].inner;
    assert!(mods(&term, inner.x + 6, inner.y).contains(Modifier::UNDERLINED));
    assert!(!mods(&term, inner.x, inner.y).contains(Modifier::UNDERLINED));
    assert!(mods(&term, inner.x + 11, inner.y + 2).contains(Modifier::REVERSED));
    let screen = text(&term);
    assert!(screen.contains("/beta  3/3"), "{screen}");
}

#[test]
fn copy_selection_is_reversed_and_the_bar_names_the_mode() {
    let mut v = two_pane(140);
    v.focus_pane(0);
    v.scroll = Some(ScrollState { offset: 0, max: 0 });
    v.set_scroll_text(
        ScrollText {
            session: "a".into(),
            lines: lines_of(&["alpha beta", "gamma", "beta again beta", "> "]),
            cols: 40,
        },
        4,
    );
    v.scroll_key(ScrollKey::CopyMode, 3);
    v.scroll_key(ScrollKey::Up, 3);
    v.scroll_key(ScrollKey::Select, 3);
    v.scroll_key(ScrollKey::Right, 3);
    v.scroll_key(ScrollKey::Right, 3);
    let mut term = Terminal::new(TestBackend::new(140, 16)).unwrap();
    term.draw(|f| render(f, &v, &find_screens())).unwrap();
    let lay = layout(Rect::new(0, 0, 140, 16), &v);
    let inner = lay.panes[0].inner;
    assert!(mods(&term, inner.x + 1, inner.y + 2).contains(Modifier::REVERSED));
    assert!(text(&term).contains("COPY"));
}

#[test]
fn clicking_in_copy_mode_places_the_cursor_without_forwarding() {
    let mut v = two_pane(140);
    v.focus_pane(0);
    v.scroll = Some(ScrollState { offset: 0, max: 0 });
    v.set_scroll_text(
        ScrollText {
            session: "a".into(),
            lines: lines_of(&["alpha beta", "gamma", "beta again beta", "> "]),
            cols: 40,
        },
        4,
    );
    v.scroll_key(ScrollKey::CopyMode, 3);
    let inner = layout(AREA, &v).panes[0].inner;
    let row = inner.y + inner.height - 1 - 3;
    let out = at(&mut v, 0, inner.x + 6, row, false);
    assert!(out.redraw && out.forward.is_none());
    let c = v.copy.as_ref().unwrap();
    assert_eq!((c.line, c.col), (0, 6));
}

#[test]
fn enter_before_the_text_has_loaded_keeps_editing() {
    let mut v = ViewState::new(four(), "local");
    v.scroll = Some(ScrollState { offset: 0, max: 5 });
    v.open_search();
    v.edit_search(b"abc", 10);
    assert_eq!(v.edit_search(b"\r", 10), SearchEdit::Editing);
}

#[test]
fn n_in_copy_mode_moves_the_cursor_to_the_hit() {
    let mut v = find_view(&["beta one", "gamma", "beta two", "> "], 40);
    v.open_search();
    v.edit_search(b"beta", 10);
    v.edit_search(b"\r", 10);
    v.scroll_key(ScrollKey::CopyMode, 4);
    assert_eq!(v.copy.as_ref().map(|c| (c.line, c.col)), Some((2, 0)));
    v.scroll_key(ScrollKey::Next, 4);
    assert_eq!(v.copy.as_ref().map(|c| (c.line, c.col)), Some((0, 0)));
    v.scroll_key(ScrollKey::Prev, 4);
    assert_eq!(v.copy.as_ref().map(|c| (c.line, c.col)), Some((2, 0)));
}

#[test]
fn escape_with_a_selection_then_y_still_yanks_and_never_forwards() {
    let mut v = find_view(&["alpha", "beta"], 40);
    v.scroll_key(ScrollKey::CopyMode, 4);
    v.scroll_key(ScrollKey::Select, 4);
    v.scroll_key(ScrollKey::Exit, 4);
    assert!(v.copy.as_ref().is_some_and(|c| c.anchor.is_none()));
    let mut s = InputSplitter::new();
    s.set_scroll_mode(true);
    assert_eq!(s.feed(b"y"), vec![Action::Scroll(ScrollKey::Yank)]);
}

#[test]
fn yanking_nothing_says_so() {
    let mut v = find_view(&[], 40);
    v.scroll_key(ScrollKey::CopyMode, 4);
    v.scroll_key(ScrollKey::Yank, 4);
    assert!(v.take_copied().is_none());
    assert_eq!(v.notice.as_deref(), Some("nothing to copy"));
}

#[test]
fn the_bar_sits_on_the_border_row_and_the_caret_stays_off_it() {
    let mut v = two_pane(140);
    v.focus_pane(0);
    v.scroll = Some(ScrollState { offset: 0, max: 0 });
    v.set_scroll_text(
        ScrollText {
            session: "a".into(),
            lines: lines_of(&["alpha beta", "gamma", "beta again beta", "> "]),
            cols: 40,
        },
        4,
    );
    v.scroll_key(ScrollKey::CopyMode, 3);
    let mut term = Terminal::new(TestBackend::new(140, 16)).unwrap();
    term.draw(|f| render(f, &v, &find_screens())).unwrap();
    let lay = layout(Rect::new(0, 0, 140, 16), &v);
    let p = &lay.panes[0];
    let border: String = (p.outer.x..p.outer.right())
        .map(|x| {
            term.backend().buffer()[(x, p.outer.bottom() - 1)]
                .symbol()
                .to_string()
        })
        .collect();
    assert!(border.contains("COPY"), "{border}");
    let last: String = (p.inner.x..p.inner.right())
        .map(|x| {
            term.backend().buffer()[(x, p.inner.bottom() - 1)]
                .symbol()
                .to_string()
        })
        .collect();
    assert!(!last.contains("COPY"), "{last}");
}

fn draft(text: Option<&str>, target: bool) -> SummaryDraft {
    SummaryDraft {
        key: "9-a-1".into(),
        session: "a".into(),
        name: "claude".into(),
        target: target.then(|| PostTarget {
            task_id: 644,
            run_id: 9,
            workspace: None,
            task_key: "TM-644".into(),
        }),
        text: text.map(str::to_string),
        editing: None,
        plain: false,
    }
}

fn jsonl(lines: &[&str]) -> Vec<String> {
    lines.iter().map(|l| l.to_string()).collect()
}

#[test]
fn the_digest_keeps_prompts_edits_commits_and_prs_but_not_tool_output() {
    let lines = jsonl(&[
        r#"{"type":"user","message":{"role":"user","content":"fix the login bug"}}"#,
        r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"meta noise"}}"#,
        r#"{"type":"user","message":{"role":"user","content":"<system-reminder>x</system-reminder>"}}"#,
        r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"output of the tool"}]}}"#,
        r#"{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"side"}]}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Opened (https://github.com/o/r/pull/3)."},{"type":"tool_use","name":"Edit","input":{"file_path":"/work/repo/src/login.rs"}},{"type":"tool_use","name":"Write","input":{"file_path":"/home/me/notes/a.md"}},{"type":"tool_use","name":"Bash","input":{"command":"git commit -m \"fix login redirect\""}}]}}"#,
        "not json",
    ]);
    let d =
        super::summary::digest_lines(lines.iter().map(String::as_str), "/work/repo", "/home/me");
    assert_eq!(d.prompts, vec!["fix the login bug"]);
    assert_eq!(d.files, vec!["src/login.rs", "~/notes/a.md"]);
    assert_eq!(d.commits, vec!["fix login redirect"]);
    assert_eq!(d.prs, vec!["https://github.com/o/r/pull/3"]);
    let input = prompt_input(&d);
    assert!(input.starts_with("BEGIN SESSION") && input.trim_end().ends_with("END SESSION"));
    assert!(!input.contains("output of the tool") && !input.contains("side"));
}

#[test]
fn heredoc_commit_messages_give_their_first_line() {
    let cmd = "git commit -m \"$(cat <<'EOF'\nfeat: add x\n\nbody\nEOF\n)\"";
    let line = serde_json::json!({"type":"assistant","message":{"content":[
        {"type":"tool_use","name":"Bash","input":{"command":cmd}}]}})
    .to_string();
    let d = super::summary::digest_lines(std::iter::once(line.as_str()), "", "");
    assert_eq!(d.commits, vec!["feat: add x"]);
}

#[test]
fn absolute_paths_outside_cwd_and_home_do_not_leak_the_user_name() {
    let line = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Edit","input":{"file_path":"/srv/other/projects/x/a.rs"}}]}}"#;
    let d = super::summary::digest_lines(std::iter::once(line), "/work", "/home/me");
    assert_eq!(d.files, vec!["…/x/a.rs"]);
}

#[test]
fn the_digest_reads_only_the_tail_of_a_big_transcript() {
    let dir = std::env::temp_dir().join(format!("tmgr-sum-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.jsonl");
    let old = r#"{"type":"user","message":{"role":"user","content":"ancient request"}}"#;
    let new = r#"{"type":"user","message":{"role":"user","content":"recent request"}}"#;
    let mut body = String::new();
    while body.len() < 2 * 1024 * 1024 + 4096 {
        body.push_str(old);
        body.push('\n');
    }
    body.push_str(new);
    body.push('\n');
    std::fs::write(&path, body).unwrap();
    let d = digest(&dir, &path, "", "").unwrap();
    assert!(d.prompts.iter().any(|p| p == "recent request"));
    assert!(d.prompts.len() < 40_000);
    std::fs::remove_dir_all(&dir).unwrap();
}

fn fake_value() -> String {
    ["abcdefgh", "12345678"].concat()
}

#[test]
fn secrets_are_scrubbed_from_every_shape() {
    let pem = format!("-----{} PRIVATE KEY-----", "BEGIN");
    let pem_end = format!("-----{} PRIVATE KEY-----", "END");
    let cases = [
        format!("export {}={} now", "TOKEN", fake_value()),
        format!(
            "{{\"{}\": \"{}{}\"}}",
            "api_key", "sk-", "abcdefghijklmnop12345"
        ),
        format!("{}: Bearer abc.def.ghi", "Authorization"),
        format!("pasted {}{} here", "ghp_", "abcdefghijklmnopqrstuv"),
        format!("{}: {}", "password", "hunter2hunter2"),
        format!("{pem}\nMIIEvQIBADANBg\n{pem_end}"),
    ];
    for c in cases {
        let out = scrub(&c);
        let c = c.as_str();
        for leaked in [
            "abcdefgh12345678",
            "sk-abcdefghijklmnop12345",
            "abc.def.ghi",
            "ghp_abcdefghijklmnopqrstuv",
            "hunter2hunter2",
            "MIIEvQIBADANBg",
        ] {
            assert!(!out.contains(leaked), "{c:?} -> {out:?}");
        }
    }
    assert_eq!(scrub("plain words stay"), "plain words stay");
}

#[test]
fn a_haiku_reply_is_stripped_of_links_images_mentions_and_fences() {
    let (text, tainted) = clean_reply("```\n- fixed [login](https://evil.example/x) @bob <b>hi</b>\n![i](https://evil.example/y.png)\n```");
    assert!(tainted);
    for bad in ["evil.example", "@", "<", "```", "https"] {
        assert!(!text.contains(bad), "{text:?}");
    }
    assert!(text.contains("fixed login"));
    let (clean, tainted) = clean_reply("- did the thing\n- one more");
    assert!(!tainted);
    assert_eq!(clean, "- did the thing\n- one more");
}

#[test]
fn reference_links_and_bare_hosts_are_flagged_and_dropped() {
    for bad in [
        "[1]: //evil.example/p.png",
        "see www.evil.example now",
        "![a][1]",
    ] {
        let (text, tainted) = clean_reply(bad);
        assert!(tainted, "{bad}");
        assert!(!text.contains("evil.example"), "{text}");
    }
}

#[test]
fn editing_ignores_escape_sequences_and_caps_lines() {
    let mut v = two_pane(140);
    v.show_summary(draft(Some("x"), true));
    v.edit_summary(b"e");
    v.edit_summary(b"\x1b[D");
    for _ in 0..30 {
        v.edit_summary(b"\r");
    }
    let buf = v.summary.as_ref().unwrap().editing.clone().unwrap();
    assert_eq!(buf, format!("x{}", "\n".repeat(11)));
}

#[test]
fn the_composed_summary_is_prose_then_facts_within_ten_or_so_lines() {
    let d = Digest {
        files: (0..20).map(|i| format!("f{i}.rs")).collect(),
        commits: vec!["a".into(), "b".into(), "c".into(), "d".into(), "e".into()],
        prs: vec!["https://github.com/o/r/pull/3".into()],
        ..Digest::default()
    };
    let text = compose(Some("- one\n- two\n- three"), &d);
    let lines: Vec<&str> = text.lines().collect();
    assert!(lines.len() <= 12, "{text}");
    assert!(lines[3].starts_with("Files changed (20): f0.rs"));
    assert!(lines[3].contains("(+12 more)"));
    assert_eq!(lines[4], "Commit: c");
    assert!(text.ends_with("PR: https://github.com/o/r/pull/3"));
    assert_eq!(
        plain_summary(&Digest::default()),
        "No file changes, commits or pull requests were found in the session."
    );
}

#[test]
fn the_summary_key_is_free_and_s_opens_a_queued_draft_only_when_asked() {
    let keys: Vec<_> = ACTIONS.iter().filter(|a| a.keys.contains(&b'S')).collect();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].cmd, Cmd::Summary);
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(b"\x1dS"), vec![Action::Cmd(Cmd::Summary)]);
    let mut v = two_pane(140);
    assert!(v.queue_summary(draft(None, true)));
    assert!(!v.queue_summary(draft(None, true)));
    assert!(v.summary.is_none());
    assert!(v.set_summary_text("9-a-1", "- done".into(), false));
    assert!(v.open_summary());
    assert_eq!(v.summary.as_ref().unwrap().text.as_deref(), Some("- done"));
}

#[test]
fn preview_keys_post_edit_and_skip_and_nothing_posts_without_text_or_target() {
    let mut v = two_pane(140);
    v.show_summary(draft(None, true));
    assert_eq!(v.edit_summary(b"p"), None);
    assert_eq!(v.edit_summary(b"e"), None);
    assert!(v.set_summary_text("9-a-1", "- done".into(), false));
    assert_eq!(v.edit_summary(b"x"), None);
    assert!(
        matches!(v.edit_summary(b"p"), Some(SummaryAction::Post(d)) if d.text.as_deref() == Some("- done"))
    );
    assert!(v.summary.is_none());

    let mut v = two_pane(140);
    v.show_summary(draft(Some("- done"), false));
    assert_eq!(v.edit_summary(b"p"), None);
    assert_eq!(v.edit_summary(b"\r"), None);
    assert!(v.summary.is_some());
    assert_eq!(
        v.edit_summary(b"\x1b"),
        Some(SummaryAction::Skip("9-a-1".into()))
    );
    assert!(v.summary.is_none());
}

#[test]
fn editing_changes_the_text_tab_saves_and_escape_discards() {
    let mut v = two_pane(140);
    v.show_summary(draft(Some("- done"), true));
    v.edit_summary(b"e");
    v.edit_summary(b"\r");
    v.edit_summary("- more é".as_bytes());
    v.edit_summary(b"\x7f");
    v.edit_summary(b"\t");
    assert_eq!(
        v.summary.as_ref().unwrap().text.as_deref(),
        Some("- done\n- more")
    );
    v.edit_summary(b"e");
    v.edit_summary(b"zzz");
    v.edit_summary(b"\x1b");
    assert_eq!(
        v.summary.as_ref().unwrap().text.as_deref(),
        Some("- done\n- more")
    );
    v.edit_summary(b"e");
    v.edit_summary(format!("{}={}", "api_key", fake_value()).as_bytes());
    v.edit_summary(b"\x13");
    assert!(!v
        .summary
        .as_ref()
        .unwrap()
        .text
        .as_deref()
        .unwrap()
        .contains("abcdefgh12345678"));
}

#[test]
fn clicking_post_returns_the_action_and_outside_clicks_do_nothing() {
    let mut v = two_pane(140);
    v.show_summary(draft(Some("- done"), true));
    let info = mouse_info(sgr_mode(MouseLevel::Any), 0);
    let screen = draw(&v, &screens(), AREA.width, AREA.height);
    let (row, col) = screen
        .lines()
        .enumerate()
        .find_map(|(y, l)| {
            l.find("Post ]")
                .map(|x| (y as u16, l[..x].chars().count() as u16))
        })
        .unwrap();
    let out = v.on_mouse(AREA, ev(0, 1, 1, false), &info);
    assert_eq!(out.summary, None);
    assert!(v.summary.is_some());
    v.on_mouse(AREA, ev(0, 1, 1, true), &info);
    let out = v.on_mouse(AREA, ev(0, col, row, false), &info);
    assert!(matches!(out.summary, Some(SummaryAction::Post(_))));
    assert!(v.summary.is_none());
}

#[test]
fn the_next_queued_summary_waits_for_an_explicit_open() {
    let mut v = two_pane(140);
    v.show_summary(draft(Some("- one"), true));
    let mut second = draft(Some("- two"), true);
    second.key = "9-a-2".into();
    v.queue_summary(second);
    v.edit_summary(b"s");
    assert!(v.summary.is_none());
    assert_eq!(v.edit_summary(b"p"), None);
    assert!(v.open_summary());
    assert_eq!(v.summary.as_ref().unwrap().key, "9-a-2");
}

#[test]
fn snapshot_summary_preview_states() {
    let mut v = two_pane(140);
    v.show_summary(draft(None, true));
    assert_snapshot("summary_generating", &draw(&v, &screens(), 140, 24));
    v.set_summary_text("9-a-1", "- fixed the login redirect\n- tests still to run\nFiles changed (2): src/login.rs, src/auth.rs\nCommit: fix login redirect".into(), false);
    assert_snapshot("summary_ready", &draw(&v, &screens(), 140, 24));
    v.edit_summary(b"e");
    assert_snapshot("summary_editing", &draw(&v, &screens(), 140, 24));
    let mut v = two_pane(140);
    v.show_summary(draft(Some("Files changed (1): a.rs"), false));
    assert_snapshot("summary_no_task", &draw(&v, &screens(), 140, 24));
}

fn layout_view() -> ViewState {
    let mut v = two_panes(SplitDir::Right);
    v.kinds.insert("a".into(), "agent:claude".into());
    v.kinds.insert("b".into(), "shell".into());
    v.locs
        .insert("a".into(), loc("/w/app", "/w/app", Some("main")));
    v.locs
        .insert("b".into(), loc("/w/app", "/w/app/src", Some("main")));
    v
}

#[test]
fn layout_names_are_trimmed_cleaned_and_bounded() {
    assert_eq!(clean_layout_name("  tmgr  ").as_deref(), Some("tmgr"));
    assert_eq!(clean_layout_name("a\x1b[31mb\n").as_deref(), Some("a[31mb"));
    assert_eq!(clean_layout_name(" \t "), None);
    assert_eq!(
        clean_layout_name(&"x".repeat(200)).unwrap().chars().count(),
        40
    );
}

#[test]
fn saved_layouts_round_trip_and_bad_entries_are_skipped() {
    let mut v = layout_view();
    v.save_layout("Work").unwrap();
    let text = render_layouts(&v.layouts);
    assert_eq!(parse_layouts(&text).unwrap(), v.layouts);
    let broken = text.replace("\"layout\": [", "\"layout\": [{\"name\": 5}, ");
    assert_eq!(parse_layouts(&broken).unwrap().len(), 1);
    let dup = format!(
        "{{\"layout\":[{},{}]}}",
        serde_json::to_string(&v.layouts[0]).unwrap(),
        serde_json::to_string(&v.layouts[0])
            .unwrap()
            .replace("Work", "work")
    );
    assert_eq!(parse_layouts(&dup).unwrap().len(), 1);
    assert!(parse_layouts("not json").is_err());
}

#[test]
fn saving_needs_panes_and_replaces_a_layout_of_the_same_name() {
    let mut empty = ViewState::new(Vec::new(), "local");
    assert!(empty.save_layout("x").is_err());
    let mut v = layout_view();
    assert!(v.save_layout("  ").is_err());
    v.save_layout("Work").unwrap();
    v.open("c");
    v.save_layout("work").unwrap();
    assert_eq!(v.layouts.len(), 1);
    assert_eq!(v.layouts[0].prefs.panes, vec!["c".to_string()]);
}

#[test]
fn switching_restores_panes_and_keeps_the_sidebar_choice() {
    let mut v = layout_view();
    v.save_layout("work").unwrap();
    v.sidebar = Some(false);
    v.open("c");
    assert!(v.switch_layout(0));
    assert_eq!(ids(&v), vec!["a", "b"]);
    assert_eq!(v.sidebar, Some(false));
    assert!(v.layout_offer.is_none());
    assert_eq!(v.notice.as_deref(), Some("layout work"));
}

#[test]
fn exited_members_are_offered_a_resume() {
    let mut v = two_panes(SplitDir::Right);
    assert!(v.split_focused(SplitDir::Down, "d"));
    v.save_layout("work").unwrap();
    v.open("c");
    v.switch_layout(0);
    assert_eq!(ids(&v), vec!["a", "b", "d"]);
    let offer = v.layout_offer.clone().unwrap();
    assert_eq!(offer.resume, vec!["d".to_string()]);
    assert!(offer.start.is_empty());
}

#[test]
fn gone_members_are_offered_a_start_only_with_a_kind_and_folder() {
    let mut v = layout_view();
    assert!(v.split_focused(SplitDir::Down, "c"));
    v.save_layout("work").unwrap();
    let rows: Vec<SessionRow> = four().into_iter().filter(|r| r.id == "a").collect();
    v.sessions_changed(AREA, rows);
    v.switch_layout(0);
    let offer = v.layout_offer.clone().unwrap();
    assert_eq!(offer.start.len(), 1);
    assert_eq!(offer.start[0].id, "b");
    assert_eq!(offer.start[0].kind.as_deref(), Some("shell"));
    assert_eq!(offer.gone, 1);
    assert_eq!(ids(&v), vec!["a"]);
}

#[test]
fn a_started_member_takes_its_place_in_the_layout() {
    let mut v = layout_view();
    v.save_layout("work").unwrap();
    let rows: Vec<SessionRow> = four().into_iter().filter(|r| r.id == "a").collect();
    v.sessions_changed(AREA, rows.clone());
    v.switch_layout(0);
    let mut with_new = rows;
    with_new.push(row("b2", "sh-9", StateKind::Working, "1s"));
    v.sessions_changed(AREA, with_new);
    let g = v.layout_gen;
    v.layout_started("b", Some("b2"), g);
    assert_eq!(ids(&v), vec!["a", "b2"]);
    assert!(v.layout_restore.is_none());
}

#[test]
fn a_failed_start_ends_the_restore_without_changing_the_panes() {
    let mut v = layout_view();
    v.save_layout("work").unwrap();
    let rows: Vec<SessionRow> = four().into_iter().filter(|r| r.id == "a").collect();
    v.sessions_changed(AREA, rows);
    v.switch_layout(0);
    let g = v.layout_gen;
    v.layout_started("b", None, g);
    assert_eq!(ids(&v), vec!["a"]);
    assert!(v.layout_restore.is_none());
}

#[test]
fn a_resumed_session_stands_in_for_the_one_it_replaced() {
    let mut v = layout_view();
    v.save_layout("work").unwrap();
    let mut rows = four();
    rows.retain(|r| r.id != "a");
    let mut resumed = row("a2", "claude", StateKind::Working, "1s");
    resumed.resumed_from = Some("a".into());
    rows.insert(0, resumed);
    v.sessions_changed(AREA, rows);
    v.open("c");
    v.switch_layout(0);
    assert_eq!(ids(&v), vec!["a2", "b"]);
    assert!(v.layout_offer.is_none());
}

#[test]
fn the_layouts_palette_saves_under_a_typed_name_and_deletes_after_a_confirmation() {
    let mut v = layout_view();
    v.apply(Cmd::Layouts);
    assert_eq!(
        v.entries().last().map(|r| r.entry.clone()),
        Some(PaletteEntry::SaveLayout(String::new()))
    );
    assert_eq!(v.edit_palette(b"\r"), PaletteEdit::Editing);
    assert!(v.notice.is_some());
    v.edit_palette(b"work");
    assert_eq!(
        v.edit_palette(b"\r"),
        PaletteEdit::Run(PaletteEntry::SaveLayout("work".into()))
    );
    v.save_layout("work").unwrap();
    v.apply(Cmd::Layouts);
    assert_eq!(v.entries()[0].entry, PaletteEntry::Layout(0));
    assert_eq!(v.entries()[0].label, "work");
    v.edit_palette(b"\x04");
    assert_eq!(v.edit_palette(b"n"), PaletteEdit::Editing);
    assert_eq!(v.layouts.len(), 1);
    v.edit_palette(b"\x04");
    assert_eq!(v.edit_palette(b"y"), PaletteEdit::LayoutsChanged);
    assert!(v.layouts.is_empty());
}

#[test]
fn enter_on_a_saved_layout_runs_it() {
    let mut v = layout_view();
    v.save_layout("work").unwrap();
    v.apply(Cmd::Layouts);
    assert_eq!(
        v.edit_palette(b"\r"),
        PaletteEdit::Run(PaletteEntry::Layout(0))
    );
}

fn visible(v: &ViewState) -> Vec<String> {
    let sb = layout(AREA, v).sidebar.unwrap();
    v.sidebar_layout(sb)
        .rows()
        .into_iter()
        .map(|(id, _)| id)
        .collect()
}

#[test]
fn the_filter_matches_names_folders_and_branches() {
    let mut v = grouped_view();
    v.open_filter();
    v.edit_filter(b"feat");
    assert_eq!(visible(&v), vec!["c"]);
    v.edit_filter(b"\x15lib");
    assert_eq!(visible(&v), vec!["d", "e"]);
    v.edit_filter(b"\x15app main");
    assert_eq!(visible(&v), vec!["a", "b"]);
    v.edit_filter(b"\x15wt");
    assert_eq!(visible(&v), vec!["c"]);
    v.edit_filter(b"\x15zzz");
    assert!(visible(&v).is_empty());
}

#[test]
fn the_filter_works_in_the_tab_list_too_and_ignores_folded_groups() {
    let mut v = grouped_view();
    v.collapsed.insert("/w/lib".into());
    v.open_filter();
    v.edit_filter(b"lib");
    assert_eq!(visible(&v), vec!["d", "e"]);
    v.grouped = false;
    assert_eq!(visible(&v), vec!["d", "e"]);
}

#[test]
fn the_filter_keeps_on_enter_and_clears_on_escape() {
    let mut v = grouped_view();
    v.open_filter();
    assert!(v.filter_input());
    v.edit_filter(b"lib");
    assert_eq!(v.edit_filter(b"\r"), FilterEdit::Keep);
    assert!(!v.filter_input());
    assert_eq!(visible(&v), vec!["d", "e"]);
    v.open_filter();
    assert_eq!(v.filter.as_ref().unwrap().query, "lib");
    assert_eq!(v.edit_filter(b"\x1b"), FilterEdit::Clear);
    assert!(v.filter.is_none());
    assert_eq!(visible(&v).len(), 6);
    v.open_filter();
    assert_eq!(v.edit_filter(b"\r"), FilterEdit::Clear);
}

#[test]
fn the_filter_shows_a_hidden_or_zoomed_sidebar() {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    v.sidebar = Some(false);
    v.zoom = true;
    v.open_filter();
    assert!(v.sidebar_shown());
    assert!(v.filter_input());
}

#[test]
fn clicking_the_sidebar_title_opens_the_filter() {
    let mut v = ViewState::new(four(), "local");
    v.width = 140;
    assert_eq!(v.hit(AREA, 3, 0), Hit::Filter);
}

#[test]
fn snapshot_filtered_sidebar() {
    let mut v = grouped_view();
    v.open_filter();
    v.edit_filter(b"app");
    assert_snapshot("filter_sidebar", &draw(&v, &screens(), 140, 22));
    v.edit_filter(b"zzz");
    assert!(draw(&v, &screens(), 140, 22).contains("no matching session"));
}

#[test]
fn snapshot_layouts_palette() {
    let mut v = layout_view();
    v.save_layout("tmgr").unwrap();
    v.save_layout("backend").unwrap();
    v.apply(Cmd::Layouts);
    assert_snapshot("layouts_palette", &draw(&v, &screens(), 100, 24));
    v.edit_palette(b"back");
    assert_snapshot("layouts_palette_query", &draw(&v, &screens(), 100, 24));
}

#[test]
fn the_offer_names_what_it_will_do_in_the_status_line() {
    let mut v = two_panes(SplitDir::Right);
    assert!(v.split_focused(SplitDir::Down, "d"));
    v.save_layout("work").unwrap();
    v.open("c");
    v.switch_layout(0);
    let screen = draw(&v, &screens(), 140, 24);
    assert!(
        screen.contains("layout work: resume 1 exited? y/n"),
        "{screen}"
    );
}

#[test]
fn a_late_start_does_not_override_what_the_user_changed_meanwhile() {
    let mut v = layout_view();
    v.save_layout("work").unwrap();
    let rows: Vec<SessionRow> = four().into_iter().filter(|r| r.id == "a").collect();
    v.sessions_changed(AREA, rows.clone());
    v.switch_layout(0);
    let g = v.layout_gen;
    v.open("c");
    let mut with_new = rows;
    with_new.push(row("b2", "sh-9", StateKind::Working, "1s"));
    with_new.push(row("c", "codex", StateKind::Blocked, "3m"));
    v.sessions_changed(AREA, with_new);
    v.layout_started("b", Some("b2"), g);
    assert_eq!(ids(&v), vec!["c"]);
}

#[test]
fn a_start_from_an_older_switch_is_ignored() {
    let mut v = layout_view();
    v.save_layout("work").unwrap();
    let rows: Vec<SessionRow> = four().into_iter().filter(|r| r.id == "a").collect();
    v.sessions_changed(AREA, rows.clone());
    v.switch_layout(0);
    let old = v.layout_gen;
    v.switch_layout(0);
    let mut with_new = rows;
    with_new.push(row("b2", "sh-9", StateKind::Working, "1s"));
    v.sessions_changed(AREA, with_new);
    v.layout_started("b", Some("b2"), old);
    assert_eq!(ids(&v), vec!["a"]);
}

#[test]
fn a_resume_during_a_restore_keeps_its_slot_for_the_later_start() {
    let mut v = two_panes(SplitDir::Right);
    assert!(v.split_focused(SplitDir::Down, "d"));
    v.kinds.insert("b".into(), "shell".into());
    v.locs.insert("b".into(), loc("/w", "/w", None));
    v.save_layout("work").unwrap();
    let mut rows = four();
    rows.retain(|r| r.id != "b");
    v.sessions_changed(AREA, rows);
    v.switch_layout(0);
    let g = v.layout_gen;
    let mut resumed = row("d2", "tests", StateKind::Working, "1s");
    resumed.resumed_from = Some("d".into());
    v.replace_session("d", "d2");
    let mut rows = four();
    rows.retain(|r| r.id != "b" && r.id != "d");
    rows.push(resumed);
    rows.push(row("b2", "sh-9", StateKind::Working, "1s"));
    v.sessions_changed(AREA, rows);
    v.layout_started("b", Some("b2"), g);
    let mut got = ids(&v);
    got.sort_unstable();
    assert_eq!(got, vec!["a", "b2", "d2"]);
}

#[test]
fn switching_to_a_layout_of_only_gone_sessions_keeps_the_current_panes() {
    let mut v = layout_view();
    v.save_layout("work").unwrap();
    v.sessions_changed(AREA, four().into_iter().filter(|r| r.id == "c").collect());
    v.open("c");
    v.switch_layout(0);
    assert_eq!(ids(&v), vec!["c"]);
    assert!(v.layout_offer.is_some());
}

#[test]
fn opening_the_filter_ends_rename_confirm_and_scroll() {
    let mut v = layout_view();
    v.scroll = Some(ScrollState { offset: 0, max: 5 });
    v.confirm = Some(Confirm {
        session: "a".into(),
        name: "claude".into(),
        remove: false,
    });
    v.open_filter();
    assert!(v.confirm.is_none() && v.scroll.is_none());
}

fn fake_secret() -> String {
    ["abcdefgh", "12345678"].concat()
}

fn export_state(findings_lines: &[&str], ws: Option<&str>, refusal: Option<&str>) -> ExportState {
    let lines: Vec<String> = findings_lines.iter().map(|s| s.to_string()).collect();
    let found = scan_export(&lines, "/home/me");
    ExportState::new(
        "Session demo 2026-10-22 10:00".into(),
        lines,
        found,
        ws.map(str::to_string),
        refusal.map(str::to_string),
        "/home/me/tmgr-session-demo.md".into(),
        "/home/me".into(),
    )
}

#[test]
fn a_transcript_becomes_markdown_with_one_line_tool_calls_and_no_tool_output() {
    let lines = jsonl(&[
        r#"{"type":"user","message":{"role":"user","content":"fix <b>the</b> login\n# not a heading"}}"#,
        r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","content":"cat .env output"}]}}"#,
        r#"{"type":"user","isMeta":true,"message":{"role":"user","content":"meta"}}"#,
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Done, see [x](javascript:alert(1)) @bob"},{"type":"tool_use","name":"Edit","input":{"file_path":"/work/repo/src/a.rs","old_string":"SECRET OLD","new_string":"SECRET NEW"}},{"type":"tool_use","name":"Bash","input":{"command":"cargo test\nsecond line"}},{"type":"tool_use","name":"WebFetch","input":{"url":"https://example.com/p?token=zzz"}}]}}"#,
    ]);
    let md = markdown_lines(
        lines.iter().map(String::as_str),
        "demo",
        "/work/repo",
        "/home/me",
    )
    .join("\n");
    assert!(md.starts_with("# Session: demo\n\n## User\nfix <b>the</b> login\n\\# not a heading"));
    let esc = escape_markdown(&md);
    assert!(esc.contains("fix &lt;b>the&lt;/b> login"));
    assert!(esc.contains("## Assistant\nDone, see [x] (javascript&#58;alert(1)) &#64;bob"));
    assert!(escape_markdown("[a]: JavaScript:x").contains("&#58;"));
    assert!(md.contains("- tool: Edit `src/a.rs`"));
    assert!(md.contains("- tool: Bash `cargo test`"));
    assert!(md.contains("- tool: WebFetch `https://example.com/p`"));
    for gone in [
        "cat .env output",
        "SECRET OLD",
        "SECRET NEW",
        "second line",
        "meta",
        "token=zzz",
    ] {
        assert!(!md.contains(gone), "{gone}");
    }
}

#[test]
fn the_scan_flags_secrets_paths_env_lines_and_key_blocks_but_not_plain_text() {
    let secret_line = format!("export {}={}", "API_TOKEN", fake_secret());
    let json_line = format!("{{\"{}\": \"{}\"}}", "password", fake_secret());
    let pem = format!("-----{} PRIVATE KEY-----", "BEGIN");
    let pem_end = format!("-----{} PRIVATE KEY-----", "END");
    let lines: Vec<String> = vec![
        "plain sentence about tokens and keys".into(),
        secret_line,
        json_line,
        "see /home/me/work/a.rs and /Users/alice/x".into(),
        "DB_URL=postgres-host-name".into(),
        pem,
        "MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQC7".into(),
        pem_end,
        "x".repeat(70),
        "done".into(),
    ];
    let f = scan_export(&lines, "/home/me");
    let at: Vec<(usize, &str)> = f.iter().map(|x| (x.line, x.kind)).collect();
    assert_eq!(
        at,
        vec![
            (1, "secret"),
            (2, "secret"),
            (3, "home path"),
            (4, ".env value"),
            (5, "private key"),
            (6, "private key"),
            (7, "private key"),
            (8, "key material"),
        ]
    );
    assert_eq!(f[2].cleaned, "see ~/work/a.rs and /Users/<user>/x");
    assert!(f
        .iter()
        .filter(|x| x.kind.contains("key"))
        .all(|x| !x.keepable));
    assert!(f.iter().all(|x| !x.cleaned.contains(&fake_secret())));
}

#[test]
fn every_finding_needs_a_decision_and_unkeepable_ones_cannot_be_kept() {
    let key = format!("-----{} PRIVATE KEY-----", "BEGIN");
    let mut s = export_state(
        &["ok", "see /home/me/a", &key, "-----END PRIVATE KEY-----"],
        Some("acme"),
        None,
    );
    let mut v = two_pane(140);
    assert_eq!(s.findings.len(), 3);
    assert_eq!(s.stage, Stage::Review);
    v.open_export(s.clone());
    v.edit_export(b"\r");
    assert_eq!(v.export.as_ref().unwrap().stage, Stage::Review);
    v.edit_export(b"k");
    v.edit_export(b"k");
    assert_eq!(v.export.as_ref().unwrap().decisions[1], Decision::Pending);
    assert_eq!(v.export.as_ref().unwrap().pending(), 2);
    v.edit_export(b"A");
    assert_eq!(v.export.as_ref().unwrap().pending(), 0);
    v.edit_export(b"\r");
    assert_eq!(v.export.as_ref().unwrap().stage, Stage::Choose);
    s = v.export.clone().unwrap();
    let text = s.ready_text().unwrap();
    assert!(text.contains("see /home/me/a") && !text.contains("PRIVATE KEY"));
}

#[test]
fn a_kept_line_is_written_as_is_and_a_pending_one_blocks_the_export() {
    let mut s = export_state(&["a /home/me/x", "b /home/me/y"], None, None);
    assert!(s.ready_text().is_none());
    s.decisions = vec![Decision::Keep, Decision::Remove];
    assert_eq!(s.ready_text().unwrap(), "a /home/me/x\nb ~/y\n");
    s.decisions = vec![Decision::Keep, Decision::Keep];
    s.lines[1] = "b /home/me/changed".into();
    s.findings[1].original = "b /home/me/y".into();
    assert!(s.ready_text().is_none());
}

#[test]
fn choosing_a_file_returns_the_cleaned_text_and_closes() {
    let mut v = two_pane(140);
    v.open_export(export_state(&["hello"], Some("acme"), None));
    assert_eq!(v.export.as_ref().unwrap().stage, Stage::Choose);
    match v.edit_export(b"f") {
        Some(ExportAction::File { text, path }) => {
            assert_eq!(text, "hello\n");
            assert!(path.ends_with(".md"));
        }
        other => panic!("{other:?}"),
    }
    assert!(v.export.is_none());
}

#[test]
fn a_page_needs_the_typed_workspace_code_and_is_refused_in_agent_sessions() {
    let mut v = two_pane(140);
    v.open_export(export_state(&["hello"], Some("acme"), None));
    assert_eq!(v.edit_export(b"g"), None);
    assert_eq!(v.export.as_ref().unwrap().stage, Stage::Confirm);
    assert_eq!(v.edit_export(b"\r"), None);
    v.edit_export(b"nope");
    assert_eq!(v.edit_export(b"\r"), None);
    assert!(v.export.is_some());
    for _ in 0..4 {
        v.edit_export(b"\x7f");
    }
    v.edit_export(b"ACME");
    match v.edit_export(b"\r") {
        Some(ExportAction::Page { text, title, code }) => {
            assert_eq!(code, "acme");
            assert_eq!(text, "hello\n");
            assert!(title.starts_with("Session demo"));
        }
        other => panic!("{other:?}"),
    }
    let mut v = two_pane(140);
    v.open_export(export_state(
        &["hello"],
        Some("acme"),
        Some("refused inside agent sessions"),
    ));
    assert_eq!(v.edit_export(b"g"), None);
    assert_eq!(v.export.as_ref().unwrap().stage, Stage::Choose);
    let mut v = two_pane(140);
    v.open_export(export_state(&["hello"], None, None));
    assert_eq!(v.edit_export(b"g"), None);
    assert_eq!(v.export.as_ref().unwrap().stage, Stage::Choose);
}

#[test]
fn the_export_key_is_free_and_clicking_a_button_acts() {
    let keys: Vec<_> = ACTIONS.iter().filter(|a| a.keys.contains(&b'e')).collect();
    assert_eq!(keys.len(), 1);
    assert_eq!(keys[0].cmd, Cmd::Export);
    let mut v = two_pane(140);
    v.open_export(export_state(&["hello"], Some("acme"), None));
    let info = mouse_info(sgr_mode(MouseLevel::Any), 0);
    let screen = draw(&v, &screens(), AREA.width, AREA.height);
    let (row, col) = screen
        .lines()
        .enumerate()
        .find_map(|(y, l)| {
            l.find("f File")
                .map(|x| (y as u16, l[..x].chars().count() as u16))
        })
        .unwrap();
    let out = v.on_mouse(AREA, ev(0, 1, 1, false), &info);
    assert_eq!(out.export, None);
    v.on_mouse(AREA, ev(0, 1, 1, true), &info);
    let out = v.on_mouse(AREA, ev(0, col, row, false), &info);
    assert!(matches!(out.export, Some(ExportAction::File { .. })));
}

#[test]
fn snapshot_export_stages() {
    let mut v = two_pane(140);
    let f = format!("export {}={}", "API_TOKEN", "notarealvalue");
    v.open_export(export_state(
        &["intro", &f, "see /home/me/a.rs"],
        Some("acme"),
        None,
    ));
    assert_snapshot("export_review", &draw(&v, &screens(), 140, 24));
    v.edit_export(b"r");
    v.edit_export(b"k");
    v.edit_export(b"\r");
    assert_snapshot("export_choose", &draw(&v, &screens(), 140, 24));
    v.edit_export(b"g");
    v.edit_export(b"ac");
    assert_snapshot("export_confirm", &draw(&v, &screens(), 140, 24));
}

#[test]
fn passwords_with_special_characters_and_url_credentials_are_found_before_escaping() {
    let pw = format!("{}: P@ssw0rd12345", "password");
    let url = format!("postgres://app:{}@db.example/x", "S3cretValue");
    let lines = vec![pw, url, "mail me @team about <b>x</b>".to_string()];
    let found = scan_export(&lines, "/home/me");
    assert_eq!(found.iter().map(|f| f.line).collect::<Vec<_>>(), vec![0, 1]);
    assert!(found
        .iter()
        .all(|f| !f.cleaned.contains("P@ssw0rd") && !f.cleaned.contains("S3cret")));
}

#[test]
fn a_stray_begin_marker_does_not_blank_the_rest_of_the_export() {
    let mut lines: Vec<String> = vec!["- tool: Bash `grep -- '-----BEGIN' f`".into()];
    lines.extend((0..300).map(|i| format!("line {i}")));
    let f = scan_export(&lines, "/home/me");
    assert!(f.iter().all(|x| x.line == 0 && x.kind != "private key"));
    let mut block = vec![format!("-----{} PRIVATE KEY-----", "BEGIN")];
    block.extend((0..300).map(|i| format!("line {i}")));
    let found = scan_export(&block, "/home/me");
    assert!(found.len() <= 201);
}
