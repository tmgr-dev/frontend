use serde::Deserialize;
use unicode_width::UnicodeWidthChar;

pub const MAX_ROWS: usize = 200;
pub const MAX_COLS: usize = 500;
pub const MAX_SPANS: usize = 500;
pub const MAX_LINE_CHARS: usize = 500;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Color {
  Idx(u8),
  Rgb(u8, u8, u8),
  Default(Option<()>),
  Named(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Attr {
  pub f: Color,
  pub b: Color,
  pub x: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Span {
  pub a: Attr,
  pub t: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct Line {
  pub spans: Vec<Span>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct Modes {
  pub ac: bool,
  pub bp: bool,
  pub alt: bool,
  pub hc: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct Snapshot {
  pub r: u16,
  pub c: u16,
  pub l: Vec<Line>,
  pub cu: Option<(u16, u16)>,
  pub m: Modes,
  #[serde(default)]
  pub so: usize,
  #[serde(default)]
  pub sm: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ScreenPush {
  pub full: bool,
  #[serde(default)]
  pub snapshot: Option<Snapshot>,
  #[serde(default)]
  pub lines: Vec<(usize, Line)>,
  #[serde(default)]
  pub cursor: Option<(u16, u16)>,
  #[serde(default)]
  pub modes: Option<Modes>,
  #[serde(default)]
  pub scroll_offset: usize,
  #[serde(default)]
  pub scroll_max: usize,
}

fn allowed(c: char) -> bool {
  !c.is_control() && !matches!(c, '\u{200E}' | '\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
}

pub fn cap_line(l: &Line) -> Line {
  let mut left = MAX_LINE_CHARS;
  let mut spans = Vec::new();
  for span in l.spans.iter().take(MAX_SPANS) {
    if left == 0 {
      break;
    }
    let t: String = span.t.chars().take(left).collect();
    left -= t.chars().count();
    spans.push(Span { a: span.a.clone(), t });
  }
  Line { spans }
}

pub fn cap_snapshot(s: &Snapshot) -> Snapshot {
  let rows = (s.r as usize).min(MAX_ROWS);
  Snapshot { r: s.r.min(MAX_ROWS as u16), c: s.c, l: s.l.iter().take(rows).map(cap_line).collect(), cu: s.cu, m: s.m, so: s.so, sm: s.sm }
}

fn color(out: &mut String, c: &Color, base: u8) {
  match c {
    Color::Idx(n) => out.push_str(&format!(";{};5;{n}", base + 8)),
    Color::Rgb(r, g, b) => out.push_str(&format!(";{};2;{r};{g};{b}", base + 8)),
    Color::Default(_) | Color::Named(_) => {}
  }
}

fn sgr(a: &Attr) -> String {
  let mut s = String::from("\x1b[0");
  for (bit, code) in [(1u8, 1u8), (2, 2), (4, 3), (8, 4), (16, 7)] {
    if a.x & bit != 0 {
      s.push_str(&format!(";{code}"));
    }
  }
  color(&mut s, &a.f, 30);
  color(&mut s, &a.b, 40);
  s.push('m');
  s
}

fn line(out: &mut String, l: &Line, cols: usize) {
  let mut left = cols;
  for span in &l.spans {
    if left == 0 {
      break;
    }
    let mut text = String::new();
    let mut used = 0;
    let mut clipped = false;
    for c in span.t.chars().filter(|c| allowed(*c)) {
      let w = c.width().unwrap_or(0);
      if used + w > left {
        clipped = true;
        break;
      }
      used += w;
      text.push(c);
    }
    if text.is_empty() {
      if clipped {
        break;
      }
      continue;
    }
    left -= used;
    if clipped {
      left = 0;
    }
    out.push_str(&sgr(&span.a));
    out.push_str(&text);
  }
  out.push_str("\x1b[0m");
}

fn tail(out: &mut String, cursor: Option<(u16, u16)>, modes: Option<Modes>, rows: usize, cols: usize) {
  if let Some(m) = modes {
    for (on, code) in [(m.ac, 1), (m.bp, 2004)] {
      out.push_str(&format!("\x1b[?{code}{}", if on { 'h' } else { 'l' }));
    }
  }
  let visible = cursor.is_some() && modes.map_or(true, |m| !m.hc);
  if let Some((r, c)) = cursor {
    let r = (r as usize).min(rows.saturating_sub(1)) + 1;
    let c = (c as usize).min(cols.saturating_sub(1)) + 1;
    out.push_str(&format!("\x1b[{r};{c}H"));
  }
  out.push_str(if visible { "\x1b[?25h" } else { "\x1b[?25l" });
}

pub fn render_full(s: &Snapshot) -> String {
  let rows = (s.r as usize).min(MAX_ROWS);
  let cols = (s.c as usize).min(MAX_COLS);
  let mut out = String::from("\x1b[H\x1b[2J");
  for (i, l) in s.l.iter().take(rows).enumerate() {
    out.push_str(&format!("\x1b[{};1H", i + 1));
    line(&mut out, l, cols);
  }
  tail(&mut out, s.cu, Some(s.m), rows, cols);
  out
}

pub fn render_diff(lines: &[(usize, Line)], cursor: Option<(u16, u16)>, modes: Option<Modes>, rows: usize, cols: usize) -> String {
  let rows = rows.min(MAX_ROWS);
  let cols = cols.min(MAX_COLS);
  let mut out = String::new();
  for (i, l) in lines {
    if *i >= rows {
      continue;
    }
    out.push_str(&format!("\x1b[{};1H\x1b[2K", i + 1));
    line(&mut out, l, cols);
  }
  tail(&mut out, cursor, modes, rows, cols);
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  fn attr() -> Attr {
    Attr { f: Color::Default(None), b: Color::Default(None), x: 0 }
  }

  fn span(t: &str) -> Span {
    Span { a: attr(), t: t.into() }
  }

  fn snap(lines: Vec<Vec<Span>>) -> Snapshot {
    Snapshot {
      r: lines.len() as u16,
      c: 80,
      l: lines.into_iter().map(|spans| Line { spans }).collect(),
      cu: Some((0, 2)),
      m: Modes::default(),
      so: 0,
      sm: 0,
    }
  }

  fn escapes(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
      if c == '\x1b' {
        let mut e = String::from("\x1b");
        while let Some(&n) = it.peek() {
          e.push(n);
          it.next();
          if n.is_ascii_alphabetic() {
            break;
          }
        }
        out.push(e);
      }
    }
    out
  }

  fn only_allowed_sequences(s: &str) {
    for e in escapes(s) {
      let ok = e.starts_with("\x1b[")
        && e[2..].chars().all(|c| c.is_ascii_digit() || matches!(c, ';' | '?' | 'H' | 'J' | 'K' | 'm' | 'h' | 'l'));
      assert!(ok, "unexpected sequence {e:?} in {s:?}");
    }
    assert!(!s.chars().any(|c| c.is_control() && c != '\x1b'), "control char in {s:?}");
  }

  #[test]
  fn hostile_span_text_never_survives() {
    let hostile = [
      "a\x1b]52;c;ZWNobw==\x07b",
      "\u{9b}31mred",
      "lone\x1bESC",
      "nul\0bel\x07del\x7f",
      "c1\u{80}\u{85}\u{9f}x",
      "\x1b[2J",
      "\x1bP1$r\x1b\\",
    ];
    for h in hostile {
      let out = render_full(&snap(vec![vec![span(h)]]));
      only_allowed_sequences(&out);
      assert!(!out.contains("\x1b]"), "{out:?}");
    }
    let out = render_full(&snap(vec![vec![span("\x1b]52;c;ZWNobw==\x07")]]));
    assert!(out.contains("]52;c;ZWNobw=="));
    assert!(!out.contains('\x07'));
  }

  #[test]
  fn fragmented_escape_across_spans_is_harmless() {
    let out = render_full(&snap(vec![vec![span("x\x1b"), span("[2J"), span("\x1b"), span("]0;t\x07")]]));
    only_allowed_sequences(&out);
    let body = out.split("\x1b[1;1H").nth(1).unwrap();
    assert_eq!(escapes(body).iter().filter(|e| e.ends_with('J')).count(), 0, "{body:?}");
  }

  #[test]
  fn full_render_clears_then_draws_every_line_and_cursor() {
    let mut s = snap(vec![vec![span("one")], vec![Span { a: Attr { f: Color::Idx(1), b: Color::Rgb(1, 2, 3), x: 1 | 16 }, t: "two".into() }]]);
    s.m = Modes { ac: true, bp: false, alt: false, hc: false };
    s.cu = Some((1, 4));
    let out = render_full(&s);
    assert!(out.starts_with("\x1b[H\x1b[2J"));
    assert!(out.contains("\x1b[1;1H\x1b[0mone\x1b[0m"));
    assert!(out.contains("\x1b[2;1H\x1b[0;1;7;38;5;1;48;2;1;2;3mtwo"));
    assert!(out.contains("\x1b[?1h") && out.contains("\x1b[?2004l"));
    assert!(out.ends_with("\x1b[2;5H\x1b[?25h"));
  }

  #[test]
  fn diff_render_touches_only_given_rows() {
    let lines = vec![(3usize, Line { spans: vec![span("new")] })];
    let out = render_diff(&lines, Some((3, 0)), Some(Modes { hc: true, ..Modes::default() }), 24, 80);
    assert!(!out.contains("\x1b[2J"));
    assert!(out.starts_with("\x1b[4;1H\x1b[2K"));
    assert!(out.contains("new"));
    assert!(out.ends_with("\x1b[?25l"));
    only_allowed_sequences(&out);
  }

  #[test]
  fn limits_are_clamped() {
    let long = "x".repeat(2000);
    let mut s = snap((0..300).map(|_| vec![span(&long)]).collect());
    s.r = 300;
    s.c = 9999;
    s.cu = Some((999, 9999));
    let out = render_full(&s);
    assert!(!out.contains("\x1b[201;1H"));
    assert!(out.contains("\x1b[200;1H"));
    assert!(!out.contains(&"x".repeat(501)));
    assert!(out.ends_with("\x1b[200;500H\x1b[?25h"));
    let diff = render_diff(&[(500, Line::default())], None, None, 24, 80);
    assert_eq!(diff, "\x1b[?25l");
  }

  #[test]
  fn wide_chars_are_clipped_by_cell_width() {
    let mut s = snap(vec![vec![span(&"界".repeat(10))]]);
    s.c = 10;
    assert_eq!(render_full(&s).matches('界').count(), 5);
    s.c = 9;
    assert_eq!(render_full(&s).matches('界').count(), 4);
    s.c = 1;
    assert_eq!(render_full(&s).matches('界').count(), 0);
    s.c = 3;
    let mixed = snap(vec![vec![span("a界"), span("b"), span("c")]]);
    let out = render_diff(&[(0, mixed.l[0].clone())], None, None, 1, 3);
    assert!(out.contains("a界") && !out.contains('b'), "{out:?}");
    let zero = render_diff(&[(0, Line { spans: vec![span("a\u{200b}\u{301}b")] })], None, None, 1, 2);
    assert!(zero.contains("a\u{200b}\u{301}b"), "{zero:?}");
  }

  #[test]
  fn bidi_controls_are_stripped() {
    let out = render_full(&snap(vec![vec![span("a\u{202e}b\u{200f}c\u{2066}d\u{2069}e\u{200e}f\u{202a}g")]]));
    assert!(out.contains("abcdefg"), "{out:?}");
  }

  #[test]
  fn lines_are_capped_before_retention() {
    let long = "x".repeat(900 * 1024);
    let capped = cap_line(&Line { spans: vec![span(&long), span(&long)] });
    assert_eq!(capped.spans.iter().map(|s| s.t.chars().count()).sum::<usize>(), MAX_LINE_CHARS);
    let many = cap_line(&Line { spans: (0..2000).map(|_| span("a")).collect() });
    assert_eq!(many.spans.len(), MAX_SPANS);
  }

  #[test]
  fn color_json_forms_parse() {
    let l: Line = serde_json::from_str(
      r#"[{"a":{"f":null,"b":3,"x":0},"t":"a"},{"a":{"f":[1,2,3],"b":"Default","x":5},"t":"b"}]"#,
    )
    .unwrap();
    assert_eq!(l.spans[0].a.f, Color::Default(None));
    assert_eq!(l.spans[0].a.b, Color::Idx(3));
    assert_eq!(l.spans[1].a.f, Color::Rgb(1, 2, 3));
    assert!(matches!(l.spans[1].a.b, Color::Named(_)));
  }
}
