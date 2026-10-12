use super::*;
use crate::screen::{Attr, Color, Line, Modes, Snapshot, Span, BOLD};
use crate::t0::{State, StateKind};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use std::collections::HashMap;

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
    assert_eq!(s.feed(&[0x1d, b'x', b'y']), vec![forward(b"y")]);
    assert_eq!(s.feed(&[0x1d, 0x1b, b'[', b'A']), vec![forward(b"[A")]);
}

#[test]
fn large_bracketed_paste_is_byte_exact() {
    let mut data = b"\x1b[200~".to_vec();
    data.extend((0..4096u32).map(|i| (i % 251) as u8).filter(|b| *b != 0x1d));
    data.extend_from_slice(b"\x1b[201~");
    let mut s = InputSplitter::new();
    assert_eq!(s.feed(&data), vec![Action::Forward(data.clone())]);
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
    assert_snapshot("help", &draw(&v, &screens(), 140, 22));
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
    assert_eq!(reset_modes(), b"\x1b[?1l\x1b[?2004l");
}
