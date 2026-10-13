use serde_json::{json, Value};
use tmgr_ipc::api::limits::{MAX_COLS, MAX_PUSH, MAX_ROWS};
use tmgr_ipc::api::{EventMsg, Line, ScreenPush, Snapshot, Span};

pub const MAX_INPUT_TEXT: usize = 4096;
pub const MAX_SPANS_PER_LINE: usize = 512;
pub const MAX_SCROLL: usize = 1_000_000;
const MAX_TEXT: usize = 512;
const FLAGS: u8 = 0b1_1111;

fn unsafe_char(c: char) -> bool {
    matches!(c,
        '\u{0}'..='\u{1f}'
        | '\u{7f}'
        | '\u{80}'..='\u{9f}'
        | '\u{200e}' | '\u{200f}'
        | '\u{202a}'..='\u{202e}'
        | '\u{2066}'..='\u{2069}'
        | '\u{feff}')
}

pub fn clean_text(s: &str) -> String {
    s.chars()
        .map(|c| if unsafe_char(c) { '?' } else { c })
        .collect()
}

fn clean_short(s: &str, max: usize) -> String {
    clean_text(&tmgr_core::text::truncate_chars(s, max).0)
}

fn guard_line(line: Line, cols: usize) -> Line {
    let mut budget = cols.saturating_mul(4).max(16);
    let mut spans = Vec::new();
    for mut span in line.spans.into_iter().take(MAX_SPANS_PER_LINE) {
        if budget == 0 {
            break;
        }
        let text: String = span.t.chars().take(budget).collect();
        budget -= text.chars().count();
        span.t = clean_text(&text);
        span.a.flags &= FLAGS;
        spans.push(Span {
            a: span.a,
            t: span.t,
        });
    }
    Line { spans }
}

fn guard_cursor(cursor: Option<(u16, u16)>, rows: usize, cols: usize) -> Option<(u16, u16)> {
    cursor.filter(|(r, c)| (*r as usize) < rows && (*c as usize) <= cols)
}

pub fn guard_screen(push: ScreenPush, session: &str) -> Result<Option<ScreenPush>, &'static str> {
    if push.session != session {
        return Ok(None);
    }
    let mut known_rows = MAX_ROWS as usize;
    let mut known_cols = MAX_COLS as usize;
    let snapshot = if push.full {
        let snap = push.snapshot.ok_or("a full screen without a snapshot")?;
        if snap.rows == 0 || snap.cols == 0 || snap.rows > MAX_ROWS || snap.cols > MAX_COLS {
            return Err("screen size out of bounds");
        }
        known_rows = snap.rows as usize;
        known_cols = snap.cols as usize;
        if snap.lines.len() > known_rows {
            return Err("more lines than rows");
        }
        Some(Snapshot {
            rows: snap.rows,
            cols: snap.cols,
            lines: snap
                .lines
                .into_iter()
                .map(|l| guard_line(l, known_cols))
                .collect(),
            cursor: guard_cursor(snap.cursor, known_rows, known_cols),
            modes: snap.modes,
            scroll_offset: snap.scroll_offset.min(MAX_SCROLL),
            scroll_max: snap.scroll_max.min(MAX_SCROLL),
        })
    } else {
        None
    };
    if push.lines.len() > MAX_ROWS as usize {
        return Err("too many changed lines");
    }
    let lines: Vec<(usize, Line)> = push
        .lines
        .into_iter()
        .filter(|(i, _)| *i < MAX_ROWS as usize)
        .map(|(i, l)| (i, guard_line(l, known_cols)))
        .collect();
    let out = ScreenPush {
        session: push.session,
        full: push.full,
        snapshot,
        lines,
        cursor: guard_cursor(push.cursor, known_rows, known_cols),
        modes: push.modes,
        scroll_offset: push.scroll_offset.min(MAX_SCROLL),
        scroll_max: push.scroll_max.min(MAX_SCROLL),
    };
    match serde_json::to_vec(&out) {
        Ok(bytes) if bytes.len() <= MAX_PUSH => Ok(Some(out)),
        _ => Err("screen update too large"),
    }
}

pub fn guard_event(mut ev: EventMsg, session: &str) -> Option<EventMsg> {
    if ev.session != session {
        return None;
    }
    ev.state.detail = ev.state.detail.map(|d| clean_short(&d, MAX_TEXT));
    Some(ev)
}

pub fn encode_screen(push: &ScreenPush) -> Vec<u8> {
    let mut v = serde_json::to_value(push).unwrap_or(Value::Null);
    if let Value::Object(m) = &mut v {
        m.insert("type".into(), json!("screen"));
    }
    serde_json::to_vec(&v).unwrap_or_default()
}

pub fn encode_event(ev: &EventMsg) -> Vec<u8> {
    let mut v = serde_json::to_value(ev).unwrap_or(Value::Null);
    if let Value::Object(m) = &mut v {
        m.insert("type".into(), json!("event"));
    }
    serde_json::to_vec(&v).unwrap_or_default()
}

pub fn encode_notice(text: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"type": "notice", "text": clean_short(text, 200)}))
        .unwrap_or_default()
}

pub fn encode_closed(reason: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"type": "closed", "reason": clean_short(reason, 200)}))
        .unwrap_or_default()
}

pub fn encode_pong() -> Vec<u8> {
    br#"{"type":"pong"}"#.to_vec()
}

#[derive(Debug, PartialEq)]
pub enum Incoming {
    Screen(Box<ScreenPush>),
    Event(EventMsg),
    Notice(String),
    Pong,
    Closed(String),
}

pub fn decode_incoming(bytes: &[u8], session: &str) -> Result<Option<Incoming>, String> {
    let v: Value = serde_json::from_slice(bytes).map_err(|_| "the follower sent invalid JSON")?;
    let kind = v.get("type").and_then(Value::as_str).unwrap_or("");
    match kind {
        "screen" => {
            let push: ScreenPush =
                serde_json::from_value(v).map_err(|_| "the follower sent a malformed screen")?;
            Ok(guard_screen(push, session)?.map(|p| Incoming::Screen(Box::new(p))))
        }
        "event" => {
            let ev: EventMsg =
                serde_json::from_value(v).map_err(|_| "the follower sent a malformed event")?;
            Ok(guard_event(ev, session).map(Incoming::Event))
        }
        "notice" => Ok(Some(Incoming::Notice(clean_short(
            v.get("text").and_then(Value::as_str).unwrap_or(""),
            200,
        )))),
        "closed" => Ok(Some(Incoming::Closed(clean_short(
            v.get("reason").and_then(Value::as_str).unwrap_or(""),
            200,
        )))),
        "pong" => Ok(Some(Incoming::Pong)),
        _ => Ok(None),
    }
}

pub fn read_incoming<R: std::io::Read>(
    r: &mut R,
    session: &str,
) -> Result<Option<Incoming>, String> {
    loop {
        match crate::proto::read_frame(r, crate::proto::MAX_FRAME) {
            Ok(None) => return Ok(None),
            Ok(Some(bytes)) => {
                if let Some(msg) = decode_incoming(&bytes, session)? {
                    return Ok(Some(msg));
                }
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

pub fn write_outgoing<W: std::io::Write>(w: &mut W, m: &Outgoing) -> Result<(), String> {
    crate::proto::write_frame(w, &encode_outgoing(m), crate::proto::MAX_FRAME)
        .map_err(|e| e.to_string())
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outgoing {
    Input(String),
    Resize { cols: u16, rows: u16 },
    Scroll(u32),
    Ping,
    Detach,
}

pub fn encode_outgoing(m: &Outgoing) -> Vec<u8> {
    let v = match m {
        Outgoing::Input(t) => json!({"type": "input", "text": t}),
        Outgoing::Resize { cols, rows } => json!({"type": "resize", "cols": cols, "rows": rows}),
        Outgoing::Scroll(o) => json!({"type": "scroll", "offset": o}),
        Outgoing::Ping => json!({"type": "ping"}),
        Outgoing::Detach => json!({"type": "detach"}),
    };
    serde_json::to_vec(&v).unwrap_or_default()
}

pub fn parse_outgoing(bytes: &[u8]) -> Result<Outgoing, &'static str> {
    let v: Value = serde_json::from_slice(bytes).map_err(|_| "not valid JSON")?;
    let obj = v.as_object().ok_or("not an object")?;
    let kind = obj.get("type").and_then(Value::as_str).ok_or("no type")?;
    let allowed: &[&str] = match kind {
        "input" => &["type", "text"],
        "resize" => &["type", "cols", "rows"],
        "scroll" => &["type", "offset"],
        "ping" | "detach" => &["type"],
        _ => return Err("unknown attach message"),
    };
    if obj.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err("unknown field");
    }
    let num = |k: &str, max: u64| {
        obj.get(k)
            .and_then(Value::as_u64)
            .filter(|n| (1..=max).contains(n))
    };
    match kind {
        "input" => {
            let t = obj
                .get("text")
                .and_then(Value::as_str)
                .ok_or("text must be a string")?;
            if t.is_empty() || t.len() > MAX_INPUT_TEXT {
                return Err("input size out of bounds");
            }
            Ok(Outgoing::Input(t.to_string()))
        }
        "resize" => Ok(Outgoing::Resize {
            cols: num("cols", MAX_COLS as u64).ok_or("cols out of bounds")? as u16,
            rows: num("rows", MAX_ROWS as u64).ok_or("rows out of bounds")? as u16,
        }),
        "scroll" => Ok(Outgoing::Scroll(
            obj.get("offset")
                .and_then(Value::as_u64)
                .filter(|n| *n <= MAX_SCROLL as u64)
                .ok_or("offset out of bounds")? as u32,
        )),
        "ping" => Ok(Outgoing::Ping),
        _ => Ok(Outgoing::Detach),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmgr_ipc::api::{Attr, Color, Modes, State, StateKind};

    const SID: &str = "s_abcdefghij";

    fn attr() -> Attr {
        Attr {
            fg: Color::Default,
            bg: Color::Default,
            flags: 0,
        }
    }

    fn modes() -> Modes {
        Modes {
            app_cursor: false,
            bracketed_paste: false,
            alt_screen: false,
            hide_cursor: false,
        }
    }

    fn span(t: &str) -> Span {
        Span {
            a: attr(),
            t: t.into(),
        }
    }

    fn full(lines: Vec<Line>, rows: u16, cols: u16) -> ScreenPush {
        ScreenPush {
            session: SID.into(),
            full: true,
            snapshot: Some(Snapshot {
                rows,
                cols,
                lines,
                cursor: Some((0, 1)),
                modes: modes(),
                scroll_offset: 0,
                scroll_max: 0,
            }),
            lines: vec![],
            cursor: Some((0, 1)),
            modes: Some(modes()),
            scroll_offset: 0,
            scroll_max: 0,
        }
    }

    const HOSTILE: &[&str] = &[
        "\u{1b}[2J\u{1b}[H",
        "\u{1b}]52;c;aGVsbG8=\u{7}",
        "\u{1b}]0;owned\u{7}",
        "\u{1b}P1$r\u{1b}\\",
        "\u{9b}31m\u{9d}52;c;eA\u{9c}",
        "\u{1b}[6n\u{1b}[c",
        "a\u{202e}b\u{2066}c\u{feff}d\0e\u{7f}f\r\n",
        "\u{1b}]8;;http://example.com\u{1b}\\link",
    ];

    fn no_controls(s: &str) -> bool {
        !s.chars().any(unsafe_char)
    }

    #[test]
    fn every_hostile_payload_is_neutralised() {
        let lines = vec![Line {
            spans: HOSTILE.iter().map(|t| span(t)).collect(),
        }];
        let got = guard_screen(full(lines, 1, 80), SID).unwrap().unwrap();
        for s in &got.snapshot.unwrap().lines[0].spans {
            assert!(no_controls(&s.t), "{:?}", s.t);
        }
        let diff = ScreenPush {
            full: false,
            snapshot: None,
            lines: vec![(
                0,
                Line {
                    spans: HOSTILE.iter().map(|t| span(t)).collect(),
                },
            )],
            ..full(vec![], 1, 80)
        };
        let got = guard_screen(diff, SID).unwrap().unwrap();
        assert!(got.lines[0].1.spans.iter().all(|s| no_controls(&s.t)));
    }

    #[test]
    fn the_serialized_frame_has_no_escape_byte() {
        let lines = vec![Line {
            spans: HOSTILE.iter().map(|t| span(t)).collect(),
        }];
        let got = guard_screen(full(lines, 1, 80), SID).unwrap().unwrap();
        let wire = encode_screen(&got);
        let text = String::from_utf8(wire).unwrap();
        assert!(!text.contains('\u{1b}') && !text.contains("\\u001b"));
    }

    #[test]
    fn dimensions_indices_and_sizes_are_bounded() {
        assert!(guard_screen(full(vec![], 0, 80), SID).is_err());
        assert!(guard_screen(full(vec![], 24, 0), SID).is_err());
        assert!(guard_screen(full(vec![], 201, 80), SID).is_err());
        assert!(guard_screen(full(vec![], 24, 501), SID).is_err());
        let too_many = vec![Line { spans: vec![] }; 5];
        assert!(guard_screen(full(too_many, 4, 80), SID).is_err());
        let diff = ScreenPush {
            full: false,
            snapshot: None,
            lines: vec![
                (
                    999_999_999,
                    Line {
                        spans: vec![span("x")],
                    },
                ),
                (
                    3,
                    Line {
                        spans: vec![span("ok")],
                    },
                ),
            ],
            ..full(vec![], 4, 80)
        };
        let got = guard_screen(diff, SID).unwrap().unwrap();
        assert_eq!(got.lines.len(), 1);
        assert_eq!(got.lines[0].0, 3);
        let flood = ScreenPush {
            full: false,
            snapshot: None,
            lines: (0..201)
                .map(|i| (i % 200, Line { spans: vec![] }))
                .collect(),
            ..full(vec![], 4, 80)
        };
        assert!(guard_screen(flood, SID).is_err());
    }

    #[test]
    fn long_lines_and_spans_are_truncated() {
        let big = Line {
            spans: vec![span(&"x".repeat(2_000_000))],
        };
        let got = guard_screen(full(vec![big], 1, 80), SID).unwrap().unwrap();
        let total: usize = got.snapshot.unwrap().lines[0]
            .spans
            .iter()
            .map(|s| s.t.chars().count())
            .sum();
        assert!(total <= 320);
        let many = Line {
            spans: (0..10_000).map(|_| span("a")).collect(),
        };
        let got = guard_screen(full(vec![many], 1, 80), SID).unwrap().unwrap();
        assert!(got.snapshot.unwrap().lines[0].spans.len() <= MAX_SPANS_PER_LINE);
    }

    #[test]
    fn foreign_sessions_are_dropped_not_forwarded() {
        let mut p = full(vec![], 1, 80);
        p.session = "s_zzzzzzzzzz".into();
        assert_eq!(guard_screen(p, SID).unwrap(), None);
        let ev = EventMsg {
            session: "s_zzzzzzzzzz".into(),
            state: State {
                kind: StateKind::Working,
                since_ms: 1,
                detail: None,
                exit_code: None,
            },
            task: None,
        };
        assert!(guard_event(ev, SID).is_none());
    }

    #[test]
    fn flags_cursor_and_scroll_are_clamped() {
        let mut line = Line {
            spans: vec![span("a")],
        };
        line.spans[0].a.flags = 0xff;
        let mut p = full(vec![line], 2, 10);
        p.snapshot.as_mut().unwrap().cursor = Some((5, 5));
        p.snapshot.as_mut().unwrap().scroll_max = usize::MAX;
        let got = guard_screen(p, SID).unwrap().unwrap();
        let snap = got.snapshot.unwrap();
        assert_eq!(snap.lines[0].spans[0].a.flags, FLAGS);
        assert_eq!(snap.cursor, None);
        assert_eq!(snap.scroll_max, MAX_SCROLL);
    }

    #[test]
    fn event_detail_is_cleaned() {
        let ev = EventMsg {
            session: SID.into(),
            state: State {
                kind: StateKind::Blocked,
                since_ms: 1,
                detail: Some("Allow?\u{1b}[2J".into()),
                exit_code: None,
            },
            task: None,
        };
        let got = guard_event(ev, SID).unwrap();
        assert_eq!(got.state.detail.as_deref(), Some("Allow??[2J"));
    }

    #[test]
    fn leader_decoder_applies_the_guard_to_follower_bytes() {
        let hostile = full(
            vec![Line {
                spans: vec![span("\u{1b}]52;c;eA\u{7}hi")],
            }],
            1,
            80,
        );
        let mut v = serde_json::to_value(&hostile).unwrap();
        v["type"] = json!("screen");
        let got = decode_incoming(&serde_json::to_vec(&v).unwrap(), SID)
            .unwrap()
            .unwrap();
        match got {
            Incoming::Screen(p) => {
                assert!(no_controls(&p.snapshot.unwrap().lines[0].spans[0].t))
            }
            other => panic!("{other:?}"),
        }
        assert!(decode_incoming(b"nope", SID).is_err());
        assert_eq!(
            decode_incoming(br#"{"type":"mystery"}"#, SID).unwrap(),
            None
        );
        let n = decode_incoming(br#"{"type":"notice","text":"a\u001b[2Jb"}"#, SID)
            .unwrap()
            .unwrap();
        assert_eq!(n, Incoming::Notice("a?[2Jb".into()));
    }

    #[test]
    fn outgoing_messages_are_strict_and_bounded() {
        for m in [
            Outgoing::Input("ls\r".into()),
            Outgoing::Resize { cols: 80, rows: 24 },
            Outgoing::Scroll(7),
            Outgoing::Ping,
            Outgoing::Detach,
        ] {
            assert_eq!(parse_outgoing(&encode_outgoing(&m)).unwrap(), m);
        }
        for bad in [
            &br#"{"type":"input","text":""}"#[..],
            br#"{"type":"input"}"#,
            br#"{"type":"resize","cols":0,"rows":5}"#,
            br#"{"type":"resize","cols":501,"rows":5}"#,
            br#"{"type":"scroll","offset":99999999}"#,
            br#"{"type":"ping","x":1}"#,
            br#"{"type":"exec","cmd":"x"}"#,
            b"[]",
        ] {
            assert!(
                parse_outgoing(bad).is_err(),
                "{:?}",
                String::from_utf8_lossy(bad)
            );
        }
        let long = encode_outgoing(&Outgoing::Input("a".repeat(MAX_INPUT_TEXT + 1)));
        assert!(parse_outgoing(&long).is_err());
    }
}
