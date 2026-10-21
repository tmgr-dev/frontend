use super::*;
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
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
    assert_eq!(scroll(b"hello\x1b[1;5Czz"), vec![]);
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
    assert_eq!(v.hit(AREA, stop, 0), Hit::None);
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
fn help_shows_the_mouse_section_at_32_rows() {
    let mut v = two_pane(140);
    v.help = true;
    let text = draw(&v, &screens(), 140, 32);
    assert!(text.contains("pick a tab, a session or a pane"));
    assert!(text.contains("Shift-drag selects"));
    assert!(text.contains("? again closes this box"));
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
            PaletteEntry::Session(_) => panic!("no sessions"),
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
    let out = v.on_mouse(AREA, ev(0, 60, 1, false), &info);
    assert_eq!(out.palette, None);
    assert!(v.palette.is_none());
    let out = v.on_mouse(AREA, ev(0, 60, 1, true), &info);
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
