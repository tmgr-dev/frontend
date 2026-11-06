use super::render::{dim, put};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use std::path::Path;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub const PICK_H: u16 = 7;
const LIST_H: usize = 5;
const MAX_PINS: usize = 50;
const MAX_ROWS: usize = 200;
const MAX_RECENTS: usize = 500;
const MAX_MARKS: usize = 40;
const MAX_SCAN: usize = 3000;
const MAX_QUERY: usize = 64;
const MAX_PATH: usize = 512;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RecentDir {
    pub path: String,
    pub last_ms: u64,
    pub count: u32,
    pub live: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FolderData {
    pub recents: Vec<RecentDir>,
    pub pins: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FolderMark {
    pub branch: String,
    pub dirty: bool,
    pub merged: bool,
}

fn odd_char(c: char) -> bool {
    c.is_control()
        || matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
}

fn usable_path(p: &str) -> bool {
    p.starts_with('/') && p.chars().count() <= MAX_PATH && !p.chars().any(odd_char)
}

impl FolderData {
    pub fn cleaned(mut self) -> FolderData {
        self.recents.retain(|r| usable_path(&r.path));
        self.recents.truncate(MAX_RECENTS);
        self.pins.retain(|p| usable_path(p));
        let mut seen = std::collections::HashSet::new();
        self.pins.retain(|p| seen.insert(p.clone()));
        self.pins.truncate(MAX_PINS);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PickRow {
    pub path: String,
    pub pinned: bool,
    pub live: bool,
    pub last_ms: u64,
    pub up: bool,
}

fn recent_score(r: &RecentDir, now_ms: u64) -> f64 {
    let days = now_ms.saturating_sub(r.last_ms) as f64 / 86_400_000.0;
    let live = if r.live { 15.0 } else { 0.0 };
    f64::from(r.count).ln_1p() * 10.0 + 50.0 * 0.5f64.powf(days / 3.0) + live
}

fn row_of(data: &FolderData, path: &str) -> PickRow {
    let found = data.recents.iter().find(|r| r.path == path);
    PickRow {
        path: path.to_string(),
        pinned: data.pins.iter().any(|p| p == path),
        live: found.is_some_and(|r| r.live),
        last_ms: found.map_or(0, |r| r.last_ms),
        up: false,
    }
}

pub fn recent_rows(data: &FolderData, now_ms: u64) -> Vec<PickRow> {
    let mut out: Vec<PickRow> = data.pins.iter().map(|p| row_of(data, p)).collect();
    let mut rest: Vec<&RecentDir> = data
        .recents
        .iter()
        .filter(|r| !data.pins.contains(&r.path))
        .collect();
    rest.sort_by(|a, b| {
        recent_score(b, now_ms)
            .total_cmp(&recent_score(a, now_ms))
            .then_with(|| a.path.cmp(&b.path))
    });
    let mut seen: Vec<&str> = Vec::new();
    for r in rest {
        if seen.contains(&r.path.as_str()) {
            continue;
        }
        seen.push(&r.path);
        out.push(row_of(data, &r.path));
    }
    out.truncate(MAX_ROWS);
    out
}

pub fn fuzzy_score(query: &str, path: &str) -> Option<i64> {
    let q: Vec<char> = query.to_lowercase().chars().collect();
    if q.is_empty() {
        return Some(0);
    }
    let lower = path.to_lowercase();
    let mut qi = 0;
    let mut score = 0i64;
    let mut prev_hit = false;
    let mut prev = '/';
    for c in lower.chars() {
        if qi < q.len() && c == q[qi] {
            score += 10;
            if prev_hit {
                score += 15;
            }
            if matches!(prev, '/' | '-' | '_' | '.' | ' ') {
                score += 20;
            }
            prev_hit = true;
            qi += 1;
        } else {
            prev_hit = false;
            score -= 1;
        }
        prev = c;
    }
    if qi < q.len() {
        return None;
    }
    let base = lower.rsplit('/').next().unwrap_or(&lower);
    let needle: String = q.iter().collect();
    if base.contains(&needle) {
        score += 60;
    }
    if base.starts_with(&needle) {
        score += 20;
    }
    Some(score)
}

pub fn search_rows(data: &FolderData, query: &str, now_ms: u64) -> Vec<PickRow> {
    if query.is_empty() {
        return recent_rows(data, now_ms);
    }
    let mut scored: Vec<(f64, PickRow)> = recent_rows(data, now_ms)
        .into_iter()
        .filter_map(|row| {
            let fz = fuzzy_score(query, &row.path)?;
            let boost = data
                .recents
                .iter()
                .find(|r| r.path == row.path)
                .map_or(0.0, |r| recent_score(r, now_ms).min(40.0));
            let pin = if row.pinned { 50.0 } else { 0.0 };
            Some((fz as f64 + boost + pin, row))
        })
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.path.cmp(&b.1.path)));
    scored.into_iter().map(|(_, r)| r).collect()
}

pub fn browse_dirs(path: &Path) -> Vec<String> {
    let Ok(rd) = std::fs::read_dir(path) else {
        return Vec::new();
    };
    let mut names: Vec<String> = rd
        .take(MAX_SCAN)
        .filter_map(Result::ok)
        .filter(|e| match e.file_type() {
            Ok(t) if t.is_dir() => true,
            Ok(t) if t.is_symlink() => std::fs::metadata(e.path()).is_ok_and(|m| m.is_dir()),
            _ => false,
        })
        .filter_map(|e| e.file_name().to_str().map(str::to_string))
        .filter(|n| !n.is_empty() && !n.chars().any(odd_char))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

fn join(dir: &str, name: &str) -> String {
    if dir == "/" {
        format!("/{name}")
    } else {
        format!("{dir}/{name}")
    }
}

fn parent_of(dir: &str) -> String {
    match dir.rfind('/') {
        Some(0) | None => "/".into(),
        Some(i) => dir[..i].to_string(),
    }
}

fn norm(dir: &str) -> String {
    let mut d = dir.to_string();
    while d.len() > 1 && d.ends_with('/') {
        d.pop();
    }
    d
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Recent,
    Browse,
    Search,
    Path,
}

#[derive(Clone, Debug)]
pub struct Picker {
    pub mode: Mode,
    pub sel: usize,
    pub moved: bool,
    pub query: String,
    pub dir: String,
    pub entries: Vec<String>,
    pub hidden: bool,
    pub saved: String,
    pub want_marks: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PKey {
    Up,
    Down,
    Left,
    Right,
    Back,
    Enter,
    Esc,
    Clear,
    Pin,
    Char(char),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Act {
    Done,
    Submit,
    Fall,
    Cancel,
    Pins(Vec<String>),
}

pub struct Env<'a> {
    pub folder: &'a mut String,
    pub data: &'a mut FolderData,
    pub home: &'a Path,
    pub list: fn(&Path) -> Vec<String>,
    pub now: u64,
}

impl Default for Picker {
    fn default() -> Self {
        Picker::new()
    }
}

impl Picker {
    pub fn new() -> Picker {
        Picker {
            mode: Mode::Recent,
            sel: 0,
            moved: false,
            query: String::new(),
            dir: String::new(),
            entries: Vec::new(),
            hidden: false,
            saved: String::new(),
            want_marks: Vec::new(),
        }
    }

    pub fn live(&self) -> bool {
        self.mode != Mode::Path
    }

    pub fn rows(&self, data: &FolderData, now_ms: u64) -> Vec<PickRow> {
        match self.mode {
            Mode::Recent => recent_rows(data, now_ms),
            Mode::Search => search_rows(data, &self.query, now_ms),
            Mode::Path => Vec::new(),
            Mode::Browse => {
                let mut out = vec![PickRow {
                    path: self.dir.clone(),
                    pinned: false,
                    live: false,
                    last_ms: 0,
                    up: true,
                }];
                for n in &self.entries {
                    if !self.hidden && n.starts_with('.') {
                        continue;
                    }
                    out.push(row_of(data, &join(&self.dir, n)));
                }
                out
            }
        }
    }

    fn reset(&mut self, mode: Mode) {
        self.mode = mode;
        self.sel = 0;
        self.moved = false;
        self.query.clear();
    }

    fn start_path_edit(&mut self, folder: &mut String, c: Option<char>) {
        self.saved = folder.clone();
        match c {
            Some(c) => *folder = c.to_string(),
            None => {
                folder.pop();
            }
        }
        self.mode = Mode::Path;
    }

    fn open_browse(&mut self, dir: &str, e: &Env, select: Option<&str>) {
        self.dir = norm(dir);
        self.entries = (e.list)(Path::new(&self.dir));
        self.mode = Mode::Browse;
        self.moved = true;
        self.query.clear();
        self.sel = 0;
        let rows = self.rows(e.data, e.now);
        if let Some(name) = select {
            let want = join(&self.dir, name);
            self.sel = rows.iter().position(|r| r.path == want).unwrap_or(0);
        }
        self.want_marks = rows
            .iter()
            .filter(|r| !r.up)
            .take(MAX_MARKS)
            .map(|r| r.path.clone())
            .collect();
    }

    fn up(&mut self, e: &Env) {
        let from = self.dir.rsplit('/').next().unwrap_or("").to_string();
        let parent = parent_of(&self.dir);
        if parent == self.dir {
            return;
        }
        self.open_browse(&parent, e, Some(&from));
    }

    pub fn key(&mut self, k: PKey, e: &mut Env) -> Act {
        let rows = self.rows(e.data, e.now);
        let cur = rows.get(self.sel).cloned();
        match k {
            PKey::Up => {
                if self.sel == 0 {
                    return Act::Fall;
                }
                self.sel -= 1;
                self.moved = true;
            }
            PKey::Down => {
                if self.sel + 1 >= rows.len() {
                    return Act::Fall;
                }
                self.sel += 1;
                self.moved = true;
            }
            PKey::Right => {
                if let Some(row) = cur {
                    if row.up {
                        self.up(e);
                    } else {
                        self.open_browse(&row.path, e, None);
                    }
                }
            }
            PKey::Left => {
                if self.mode == Mode::Browse {
                    self.up(e);
                }
            }
            PKey::Back => match self.mode {
                Mode::Recent => self.start_path_edit(e.folder, None),
                Mode::Search => {
                    self.query.pop();
                    self.sel = 0;
                    if self.query.is_empty() {
                        self.reset(Mode::Recent);
                    }
                }
                Mode::Browse => self.up(e),
                Mode::Path => {}
            },
            PKey::Clear => {
                if self.mode == Mode::Search {
                    self.reset(Mode::Recent);
                }
            }
            PKey::Esc => match self.mode {
                Mode::Recent => return Act::Cancel,
                Mode::Path => {
                    *e.folder = std::mem::take(&mut self.saved);
                    self.reset(Mode::Recent);
                }
                _ => self.reset(Mode::Recent),
            },
            PKey::Pin => {
                if let Some(row) = cur {
                    let pins = &mut e.data.pins;
                    if let Some(i) = pins.iter().position(|p| *p == row.path) {
                        pins.remove(i);
                    } else if pins.len() < MAX_PINS {
                        pins.push(row.path.clone());
                    }
                    return Act::Pins(pins.clone());
                }
            }
            PKey::Enter => {
                if self.mode == Mode::Search && cur.is_none() {
                    return Act::Done;
                }
                if self.moved {
                    if let Some(row) = cur {
                        *e.folder = row.path;
                    }
                }
                return Act::Submit;
            }
            PKey::Char(c) => self.typed(c, cur.is_some(), &rows, e),
        }
        Act::Done
    }

    fn typed(&mut self, c: char, _has_row: bool, rows: &[PickRow], e: &mut Env) {
        match self.mode {
            Mode::Recent => {
                if matches!(c, '/' | '~' | '.') {
                    self.start_path_edit(e.folder, Some(c));
                } else {
                    self.reset(Mode::Search);
                    self.query.push(c);
                    self.moved = true;
                }
            }
            Mode::Search => {
                if self.query.is_empty() && matches!(c, '/' | '~' | '.') {
                    self.start_path_edit(e.folder, Some(c));
                } else if self.query.chars().count() < MAX_QUERY {
                    self.query.push(c);
                    self.sel = 0;
                    self.moved = true;
                }
            }
            Mode::Browse => match c {
                '.' => {
                    self.hidden = !self.hidden;
                    self.sel = 0;
                    let rows = self.rows(e.data, e.now);
                    self.want_marks = rows
                        .iter()
                        .filter(|r| !r.up)
                        .take(MAX_MARKS)
                        .map(|r| r.path.clone())
                        .collect();
                }
                '~' => {
                    let home = e.home.to_string_lossy().into_owned();
                    if usable_path(&home) {
                        self.open_browse(&home, e, None);
                    }
                }
                '/' => self.reset(Mode::Search),
                c => {
                    let lc = c.to_lowercase().next().unwrap_or(c);
                    let at = rows.iter().position(|r| {
                        !r.up
                            && r.path
                                .rsplit('/')
                                .next()
                                .and_then(|n| n.chars().next())
                                .is_some_and(|f| f.to_lowercase().next() == Some(lc))
                    });
                    if let Some(i) = at {
                        self.sel = i;
                    }
                }
            },
            Mode::Path => {}
        }
    }
}

pub fn age_label(now_ms: u64, last_ms: u64) -> String {
    if last_ms == 0 {
        return String::new();
    }
    let s = now_ms.saturating_sub(last_ms) / 1000;
    match s {
        0..=59 => "now".into(),
        60..=3599 => format!("{}m", s / 60),
        3600..=172_799 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86_400),
    }
}

fn tilde(path: &str, home: &Path) -> String {
    let h = home.to_string_lossy();
    if h.len() > 1 {
        if path == h {
            return "~".into();
        }
        if let Some(rest) = path.strip_prefix(&format!("{h}/")) {
            return format!("~/{rest}");
        }
    }
    path.to_string()
}

fn shorten(s: &str, max: usize) -> String {
    if s.width() <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let mut tail: Vec<char> = Vec::new();
    let mut w = 1;
    for c in s.chars().rev() {
        let cw = c.width().unwrap_or(0);
        if w + cw > max {
            break;
        }
        w += cw;
        tail.push(c);
    }
    tail.reverse();
    format!("…{}", tail.into_iter().collect::<String>())
}

fn help(mode: Mode) -> &'static str {
    match mode {
        Mode::Recent => "↑↓ pick  Enter start  → open  type to search  ^P pin  ⌫ edit",
        Mode::Search => "↑↓ pick  Enter start  → open  Esc back  ^P pin",
        Mode::Browse => "←→ up/into  Enter start  . hidden  ~ home  / search  ^P pin",
        Mode::Path => "",
    }
}

pub struct DrawCtx<'a> {
    pub data: &'a FolderData,
    pub marks: &'a std::collections::HashMap<String, Option<FolderMark>>,
    pub home: &'a Path,
    pub now: u64,
}

pub fn draw(buf: &mut Buffer, rect: Rect, p: &Picker, c: &DrawCtx) {
    if rect.height < PICK_H || rect.width < 20 {
        return;
    }
    let w = rect.width;
    let title = match p.mode {
        Mode::Recent => "Recent".to_string(),
        Mode::Search => format!("Search: {}", p.query),
        Mode::Browse => format!("{}/", tilde(&p.dir, c.home).trim_end_matches('/')),
        Mode::Path => String::new(),
    };
    put(
        buf,
        rect.x,
        rect.y,
        &shorten(&title, usize::from(w)),
        Style::new().add_modifier(Modifier::BOLD),
        w,
    );
    let rows = p.rows(c.data, c.now);
    let start = (p.sel + 1).saturating_sub(LIST_H);
    if rows.is_empty() {
        put(buf, rect.x, rect.y + 1, "  (no match)", dim(), w);
    }
    for (i, row) in rows.iter().skip(start).take(LIST_H).enumerate() {
        let y = rect.y + 1 + i as u16;
        let selected = start + i == p.sel;
        let mut meta: Vec<String> = Vec::new();
        let left = if p.mode == Mode::Browse {
            if row.up {
                "..  (up)".to_string()
            } else {
                let name = row.path.rsplit('/').next().unwrap_or("");
                if let Some(Some(m)) = c.marks.get(&row.path) {
                    meta.push(format!("git:{}", m.branch));
                    if m.dirty {
                        meta.push("●dirty".into());
                    }
                    if m.merged {
                        meta.push("merged".into());
                    }
                }
                format!("{name}/")
            }
        } else {
            tilde(&row.path, c.home)
        };
        if row.live {
            meta.push("live".into());
        }
        let age = age_label(c.now, row.last_ms);
        if !age.is_empty() && p.mode != Mode::Browse {
            meta.push(age);
        }
        let meta = meta.join(" ");
        let meta_w = meta.width();
        let room = usize::from(w).saturating_sub(4 + meta_w + 1);
        let left = shorten(&left, room);
        let style = if selected {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        let line = format!(
            "{}{}{}",
            if selected { "▸ " } else { "  " },
            if row.pinned { "★ " } else { "  " },
            left
        );
        put(buf, rect.x, y, &" ".repeat(usize::from(w)), style, w);
        put(buf, rect.x, y, &line, style, w);
        if meta_w > 0 {
            let mx = rect.x + w.saturating_sub(meta_w as u16);
            put(buf, mx, y, &meta, if selected { style } else { dim() }, w);
        }
    }
    put(buf, rect.x, rect.y + PICK_H - 1, help(p.mode), dim(), w);
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_000_000_000_000;
    const MIN: u64 = 60_000;
    const DAY: u64 = 86_400_000;

    fn rd(path: &str, ago: u64, count: u32, live: bool) -> RecentDir {
        RecentDir {
            path: path.into(),
            last_ms: NOW - ago,
            count,
            live,
        }
    }

    fn data() -> FolderData {
        FolderData {
            recents: vec![
                rd("/w/old", 30 * DAY, 40, false),
                rd("/w/fresh", 2 * MIN, 1, false),
                rd("/w/busy", DAY, 12, true),
                rd("/w/pinned", 90 * DAY, 1, false),
            ],
            pins: vec!["/w/pinned".into()],
        }
    }

    fn fake(path: &Path) -> Vec<String> {
        match path.to_str().unwrap() {
            "/" => vec!["w".into(), "etc".into()],
            "/w" => vec![".git".into(), "alpha".into(), "beta".into()],
            "/w/alpha" => vec!["src".into()],
            "/home/u" => vec!["code".into()],
            _ => Vec::new(),
        }
    }

    fn run(p: &mut Picker, d: &mut FolderData, f: &mut String, k: PKey) -> Act {
        let mut e = Env {
            folder: f,
            data: d,
            home: Path::new("/home/u"),
            list: fake,
            now: NOW,
        };
        p.key(k, &mut e)
    }

    fn paths(rows: &[PickRow]) -> Vec<&str> {
        rows.iter().map(|r| r.path.as_str()).collect()
    }

    #[test]
    fn recent_puts_pins_first_then_frequency_and_recency() {
        let rows = recent_rows(&data(), NOW);
        assert_eq!(paths(&rows), ["/w/pinned", "/w/busy", "/w/fresh", "/w/old"]);
        assert!(rows[0].pinned);
        assert!(rows[1].live);
    }

    #[test]
    fn fuzzy_matches_subsequences_and_prefers_basename() {
        assert!(fuzzy_score("zz", "/w/alpha").is_none());
        let a = fuzzy_score("alp", "/w/alpha").unwrap();
        let b = fuzzy_score("alp", "/alpha/w/x").unwrap();
        assert!(a > b);
        assert!(fuzzy_score("tmui", "/x/task-manager-ui").is_some());
    }

    #[test]
    fn search_ranks_recent_higher_on_equal_match() {
        let d = FolderData {
            recents: vec![
                rd("/a/proj", 60 * DAY, 1, false),
                rd("/b/proj", MIN, 5, false),
            ],
            pins: vec![],
        };
        assert_eq!(paths(&search_rows(&d, "proj", NOW)), ["/b/proj", "/a/proj"]);
        assert!(search_rows(&d, "nomatch", NOW).is_empty());
    }

    #[test]
    fn enter_keeps_default_until_selection_moves() {
        let mut d = data();
        let mut p = Picker::new();
        let mut f = "/w/default".to_string();
        assert_eq!(run(&mut p, &mut d, &mut f, PKey::Enter), Act::Submit);
        assert_eq!(f, "/w/default");
        assert_eq!(run(&mut p, &mut d, &mut f, PKey::Down), Act::Done);
        assert_eq!(run(&mut p, &mut d, &mut f, PKey::Enter), Act::Submit);
        assert_eq!(f, "/w/busy");
    }

    #[test]
    fn arrows_fall_through_at_the_edges() {
        let mut d = data();
        let mut p = Picker::new();
        let mut f = String::new();
        assert_eq!(run(&mut p, &mut d, &mut f, PKey::Up), Act::Fall);
        for _ in 0..3 {
            run(&mut p, &mut d, &mut f, PKey::Down);
        }
        assert_eq!(run(&mut p, &mut d, &mut f, PKey::Down), Act::Fall);
    }

    #[test]
    fn browse_goes_into_and_up_hides_dotfiles_and_selects_origin() {
        let mut d = data();
        let mut p = Picker::new();
        let mut f = String::new();
        p.open_browse(
            "/w",
            &Env {
                folder: &mut f,
                data: &mut d,
                home: Path::new("/home/u"),
                list: fake,
                now: NOW,
            },
            None,
        );
        let rows = p.rows(&d, NOW);
        assert_eq!(paths(&rows), ["/w", "/w/alpha", "/w/beta"]);
        run(&mut p, &mut d, &mut f, PKey::Char('.'));
        assert_eq!(p.rows(&d, NOW).len(), 4);
        run(&mut p, &mut d, &mut f, PKey::Char('.'));
        run(&mut p, &mut d, &mut f, PKey::Down);
        run(&mut p, &mut d, &mut f, PKey::Right);
        assert_eq!(p.dir, "/w/alpha");
        run(&mut p, &mut d, &mut f, PKey::Left);
        assert_eq!(p.dir, "/w");
        assert_eq!(p.sel, 1);
        run(&mut p, &mut d, &mut f, PKey::Left);
        assert_eq!(p.dir, "/");
        run(&mut p, &mut d, &mut f, PKey::Left);
        assert_eq!(p.dir, "/");
        run(&mut p, &mut d, &mut f, PKey::Char('~'));
        assert_eq!(p.dir, "/home/u");
        run(&mut p, &mut d, &mut f, PKey::Enter);
        assert_eq!(f, "/home/u");
    }

    #[test]
    fn enter_on_dotdot_starts_in_the_browsed_folder() {
        let mut d = data();
        let mut p = Picker::new();
        let mut f = String::new();
        run(&mut p, &mut d, &mut f, PKey::Down);
        run(&mut p, &mut d, &mut f, PKey::Right);
        run(&mut p, &mut d, &mut f, PKey::Up);
        run(&mut p, &mut d, &mut f, PKey::Enter);
        assert_eq!(f, "/w/busy");
    }

    #[test]
    fn typing_searches_and_slash_tilde_dot_switch_to_path_mode() {
        let mut d = data();
        let mut p = Picker::new();
        let mut f = "/w/default".to_string();
        run(&mut p, &mut d, &mut f, PKey::Char('f'));
        assert_eq!(p.mode, Mode::Search);
        run(&mut p, &mut d, &mut f, PKey::Char('r'));
        assert_eq!(paths(&p.rows(&d, NOW)), ["/w/fresh"]);
        run(&mut p, &mut d, &mut f, PKey::Enter);
        assert_eq!(f, "/w/fresh");

        for c in ['/', '~', '.'] {
            let mut p = Picker::new();
            let mut f = "/w/default".to_string();
            run(&mut p, &mut d, &mut f, PKey::Char(c));
            assert_eq!(p.mode, Mode::Path);
            assert_eq!(f, c.to_string());
            assert!(!p.live());
            run(&mut p, &mut d, &mut f, PKey::Esc);
            assert_eq!((p.mode, f.as_str()), (Mode::Recent, "/w/default"));
        }
    }

    #[test]
    fn backspace_in_recent_edits_the_path_and_esc_steps_back() {
        let mut d = data();
        let mut p = Picker::new();
        let mut f = "/w/default".to_string();
        run(&mut p, &mut d, &mut f, PKey::Back);
        assert_eq!((p.mode, f.as_str()), (Mode::Path, "/w/defaul"));
        let mut p = Picker::new();
        run(&mut p, &mut d, &mut f, PKey::Char('x'));
        assert_eq!(run(&mut p, &mut d, &mut f, PKey::Esc), Act::Done);
        assert_eq!(p.mode, Mode::Recent);
        assert_eq!(run(&mut p, &mut d, &mut f, PKey::Esc), Act::Cancel);
    }

    #[test]
    fn pin_toggles_and_returns_the_new_list() {
        let mut d = data();
        let mut p = Picker::new();
        let mut f = String::new();
        run(&mut p, &mut d, &mut f, PKey::Down);
        assert_eq!(
            run(&mut p, &mut d, &mut f, PKey::Pin),
            Act::Pins(vec!["/w/pinned".into(), "/w/busy".into()])
        );
        assert_eq!(
            run(&mut p, &mut d, &mut f, PKey::Pin),
            Act::Pins(vec!["/w/pinned".into()])
        );
    }

    #[test]
    fn cleaned_drops_relative_and_control_paths() {
        let d = FolderData {
            recents: vec![
                rd("rel", 0, 1, false),
                rd("/a\x1b[2J", 0, 1, false),
                rd("/ok", 0, 1, false),
            ],
            pins: vec!["/p\u{202e}x".into(), "/q".into()],
        }
        .cleaned();
        assert_eq!(d.recents.len(), 1);
        assert_eq!(d.pins, ["/q"]);
    }

    #[test]
    fn browse_dirs_lists_only_folders_sorted() {
        let t = tempfile::tempdir().unwrap();
        for n in ["b", "A", ".h"] {
            std::fs::create_dir(t.path().join(n)).unwrap();
        }
        std::fs::write(t.path().join("file"), "x").unwrap();
        assert_eq!(browse_dirs(t.path()), [".h", "A", "b"]);
        assert!(browse_dirs(&t.path().join("none")).is_empty());
    }

    #[test]
    fn ages_are_short() {
        assert_eq!(age_label(NOW, NOW - 2 * MIN), "2m");
        assert_eq!(age_label(NOW, NOW - 3 * 3_600_000), "3h");
        assert_eq!(age_label(NOW, NOW - 5 * DAY), "5d");
        assert_eq!(age_label(NOW, 0), "");
    }
}
