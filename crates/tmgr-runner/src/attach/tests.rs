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
    assert_eq!(s.feed(&[0x1d, 0x1b, b'[', b'A']), vec![forward(b"[A")]);
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
        b"\x1b",
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
    assert_eq!(scroll(b"\x1b"), vec![Action::Scroll(ScrollKey::Exit)]);
    assert_eq!(
        scroll(b"\x1b[5~\x1b"),
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
    v.panes.iter().map(String::as_str).collect()
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
    assert_eq!(v.focus, 1);
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
    assert_eq!(v.focus, 0);
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
    v.sessions_changed(four().into_iter().filter(|r| r.id != "b").collect());
    assert_eq!(ids(&v), ["a"]);
    assert_eq!(v.focus, 0);

    v.sessions_changed(
        four()
            .into_iter()
            .filter(|r| r.id == "c" || r.id == "d")
            .collect(),
    );
    assert_eq!(ids(&v), ["c"]);

    v.sessions_changed(vec![]);
    assert!(v.panes.is_empty());
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
        (fresh.focus, fresh.zoom, fresh.sidebar),
        (1, true, Some(false))
    );

    let mut gone = ViewState::new(four(), "");
    gone.apply_prefs(&LayoutPrefs {
        panes: vec!["zzz".into(), "d".into(), "d".into(), "qqq".into()],
        focus: 5,
        zoom: false,
        sidebar: None,
    });
    assert_eq!(ids(&gone), ["d"]);
    assert_eq!(gone.focus, 0);

    gone.apply_prefs(&LayoutPrefs {
        panes: vec!["zzz".into()],
        focus: 0,
        zoom: false,
        sidebar: None,
    });
    assert_eq!(ids(&gone), ["d"]);
}

fn two_pane(width: u16) -> ViewState {
    let mut v = ViewState::new(four(), "local");
    v.apply(Cmd::Split);
    v.width = width;
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
fn layout_wide_but_remaining_width_narrow_stacks() {
    let v = two_pane(110);
    let l = layout(r(0, 0, 110, 30), &v);
    assert_eq!(l.sidebar, Some(r(0, 0, 24, 29)));
    assert_eq!(l.panes[0].outer, r(24, 0, 86, 14));
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
    assert_snapshot("help", &draw(&v, &screens(), 140, 26));
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
    let side: Vec<&str> = rows.lines().take(3).collect();
    assert!(side[1].contains("◆ codex"), "{side:?}");
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
        b"\x1b[?1l\x1b[?2004l\x1b[?1006l\x1b[?1003l\x1b[?1002l\x1b[?1000l".as_slice()
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
    assert_eq!(v.focus, 1);
    v.on_mouse(
        AREA,
        ev(0, 40, 5, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert_eq!(v.focus, 0);
    let mut n = two_pane(90);
    let area = Rect::new(0, 0, 90, 40);
    n.on_mouse(
        area,
        ev(0, 40, 5, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert_eq!(n.focus, 0);
    n.on_mouse(
        area,
        ev(0, 40, 30, false),
        &mouse_info(MouseMode::default(), 0),
    );
    assert_eq!(n.focus, 1);
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
    assert_eq!(v.focus, 0);
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
    assert_eq!(v.paste_target(AREA), v.focus);
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
    assert!(v.panes.is_empty());
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
