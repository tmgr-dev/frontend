use crate::resume::valid_resume_id;
use serde_json::Value;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub const TITLE_MAX: usize = 120;
const PASS_CAP: u64 = 8 * 1024 * 1024;
const MAX_PASSES: usize = 4;
const LINE_CAP: usize = 1024 * 1024;
const PID_FILE_CAP: u64 = 16 * 1024;
const DIR_CAP: usize = 512;
const ELLIPSIS: char = '…';

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentTitle {
    pub text: String,
    pub auto: bool,
    pub base: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Claude,
    Codex,
}

impl Format {
    pub fn of(profile: &str) -> Option<Format> {
        match profile {
            "claude-code" => Some(Format::Claude),
            "codex" => Some(Format::Codex),
            _ => None,
        }
    }
}

pub fn clean_title(raw: &str) -> String {
    let mut out = String::new();
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => match chars.next() {
                Some('[') => {
                    for n in chars.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&n) {
                            break;
                        }
                    }
                }
                Some(']' | 'P' | 'X' | '^' | '_') => {
                    while let Some(n) = chars.next() {
                        if n == '\u{7}' {
                            break;
                        }
                        if n == '\u{1b}' {
                            chars.next();
                            break;
                        }
                    }
                }
                _ => {}
            },
            '\n' | '\r' | '\t' => out.push(' '),
            '\u{0}'..='\u{1f}'
            | '\u{7f}'..='\u{9f}'
            | '\u{61c}'
            | '\u{200b}'..='\u{200f}'
            | '\u{2028}'..='\u{202e}'
            | '\u{2060}'..='\u{2069}'
            | '\u{feff}'
            | '\u{e0000}'..='\u{e007f}' => {}
            c => out.push(c),
        }
        if out.len() > TITLE_MAX * 8 {
            break;
        }
    }
    let joined = out.split_whitespace().collect::<Vec<_>>().join(" ");
    joined.chars().take(TITLE_MAX).collect()
}

pub fn clip(s: &str, width: usize) -> String {
    if s.width() <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = c.width().unwrap_or(0);
        if used + w + 1 > width {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push(ELLIPSIS);
    out
}

pub fn encode_cwd(cwd: &str) -> String {
    cwd.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn open_under(root: &Path, path: &Path) -> Option<(File, std::fs::Metadata)> {
    let names: Vec<&std::ffi::OsStr> = path
        .strip_prefix(root)
        .ok()?
        .components()
        .map(|c| match c {
            Component::Normal(n) => Some(n),
            _ => None,
        })
        .collect::<Option<_>>()?;
    let (last, dirs) = names.split_last()?;
    let mut dir = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
        .open(root)
        .ok()?;
    for name in dirs {
        dir = openat(&dir, name, libc::O_DIRECTORY)?;
    }
    let file = openat(&dir, last, libc::O_NONBLOCK)?;
    let meta = file.metadata().ok()?;
    meta.is_file().then_some((file, meta))
}

fn openat(dir: &File, name: &std::ffi::OsStr, flags: libc::c_int) -> Option<File> {
    let name = std::ffi::CString::new(name.as_bytes()).ok()?;
    let fd = unsafe {
        libc::openat(
            dir.as_raw_fd(),
            name.as_ptr(),
            libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC | flags,
        )
    };
    (fd >= 0).then(|| unsafe { File::from_raw_fd(fd) })
}

fn read_small(root: &Path, path: &Path, cap: u64) -> Option<Vec<u8>> {
    let (file, _) = open_under(root, path)?;
    let mut buf = Vec::new();
    file.take(cap).read_to_end(&mut buf).ok()?;
    Some(buf)
}

fn pid_file_id(home: &Path, pid: u32) -> Option<String> {
    if pid == 0 {
        return None;
    }
    let raw = read_small(
        home,
        &home
            .join(".claude")
            .join("sessions")
            .join(format!("{pid}.json")),
        PID_FILE_CAP,
    )?;
    let first = serde_json::Deserializer::from_slice(&raw)
        .into_iter::<Value>()
        .next()?
        .ok()?;
    if first
        .get("pid")
        .and_then(Value::as_u64)
        .is_some_and(|p| p != u64::from(pid))
    {
        return None;
    }
    let id = first.get("sessionId")?.as_str()?;
    valid_resume_id(id).then(|| id.to_string())
}

fn newest_jsonl(dir: &Path) -> Option<PathBuf> {
    let mut best: Option<(std::time::SystemTime, PathBuf)> = None;
    for entry in std::fs::read_dir(dir).ok()?.take(DIR_CAP).flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let Ok(mtime) = meta.modified() else {
            continue;
        };
        if best.as_ref().is_none_or(|(t, _)| mtime > *t) {
            best = Some((mtime, path));
        }
    }
    best.map(|(_, p)| p)
}

fn project_dirs(home: &Path, cwd: &str) -> Vec<PathBuf> {
    let projects = home.join(".claude").join("projects");
    let mut names = vec![encode_cwd(cwd)];
    if let Ok(real) = std::fs::canonicalize(cwd) {
        let real = encode_cwd(&real.to_string_lossy());
        if !names.contains(&real) {
            names.push(real);
        }
    }
    names.into_iter().map(|n| projects.join(n)).collect()
}

pub struct Source<'a> {
    pub home: &'a Path,
    pub format: Format,
    pub cwd: &'a str,
    pub pid: u32,
    pub log_id: Option<&'a str>,
}

pub fn locate(src: &Source<'_>) -> Option<(PathBuf, Option<String>)> {
    match src.format {
        Format::Claude => locate_claude(src),
        Format::Codex => {
            let id = src
                .log_id
                .filter(|i| valid_resume_id(i))
                .map(str::to_string);
            Some((src.home.join(".codex").join("session_index.jsonl"), id))
        }
    }
}

fn locate_claude(src: &Source<'_>) -> Option<(PathBuf, Option<String>)> {
    let id = pid_file_id(src.home, src.pid).or_else(|| {
        src.log_id
            .filter(|i| valid_resume_id(i))
            .map(str::to_string)
    });
    let dirs = project_dirs(src.home, src.cwd);
    if let Some(id) = id {
        let file = format!("{id}.jsonl");
        for dir in &dirs {
            let p = dir.join(&file);
            if std::fs::symlink_metadata(&p).is_ok_and(|m| m.is_file()) {
                return Some((p, Some(id)));
            }
        }
        let projects = src.home.join(".claude").join("projects");
        for entry in std::fs::read_dir(projects).ok()?.take(DIR_CAP).flatten() {
            let p = entry.path().join(&file);
            if std::fs::symlink_metadata(&p).is_ok_and(|m| m.is_file()) {
                return Some((p, Some(id)));
            }
        }
        return None;
    }
    dirs.iter().find_map(|d| newest_jsonl(d)).map(|p| (p, None))
}

#[derive(Debug)]
pub struct TitleScan {
    format: Format,
    root: PathBuf,
    path: PathBuf,
    id: Option<String>,
    ident: Option<(u64, u64, i64, i64)>,
    offset: u64,
    carry: Vec<u8>,
    skipping: bool,
    custom: Option<String>,
    auto: Option<String>,
}

impl TitleScan {
    pub fn new(format: Format, root: PathBuf, path: PathBuf, id: Option<String>) -> TitleScan {
        TitleScan {
            format,
            root,
            path,
            id,
            ident: None,
            offset: 0,
            carry: Vec::new(),
            skipping: false,
            custom: None,
            auto: None,
        }
    }

    pub fn is_for(&self, path: &Path, id: Option<&str>) -> bool {
        self.path == path && self.id.as_deref() == id
    }

    fn reset(&mut self) {
        self.offset = 0;
        self.carry.clear();
        self.skipping = false;
        self.custom = None;
        self.auto = None;
    }

    pub fn poll(&mut self) {
        for _ in 0..MAX_PASSES {
            if !self.pass() {
                return;
            }
        }
    }

    fn pass(&mut self) -> bool {
        let Some((mut file, meta)) = open_under(&self.root, &self.path) else {
            return false;
        };
        let ident = (meta.dev(), meta.ino(), meta.mtime(), meta.mtime_nsec());
        let rewritten = self.ident.is_some_and(|i| (i.0, i.1) != (ident.0, ident.1))
            || meta.len() < self.offset
            || (meta.len() == self.offset && self.ident != Some(ident));
        if rewritten {
            self.reset();
        }
        self.ident = Some(ident);
        if meta.len() == self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return false;
        }
        let mut buf = Vec::new();
        if file.take(PASS_CAP).read_to_end(&mut buf).is_err() {
            return false;
        }
        self.offset += buf.len() as u64;
        self.feed(&buf);
        buf.len() as u64 == PASS_CAP
    }

    fn feed(&mut self, bytes: &[u8]) {
        let mut rest = bytes;
        while let Some(nl) = rest.iter().position(|b| *b == b'\n') {
            let (head, tail) = rest.split_at(nl);
            rest = &tail[1..];
            if self.skipping {
                self.skipping = false;
                self.carry.clear();
                continue;
            }
            if self.carry.len() + head.len() <= LINE_CAP {
                let mut line = std::mem::take(&mut self.carry);
                line.extend_from_slice(head);
                self.line(&line);
            }
            self.carry.clear();
        }
        if self.skipping {
            return;
        }
        if self.carry.len() + rest.len() > LINE_CAP {
            self.skipping = true;
            self.carry.clear();
        } else {
            self.carry.extend_from_slice(rest);
        }
    }

    fn line(&mut self, line: &[u8]) {
        match self.format {
            Format::Claude => self.claude_line(line),
            Format::Codex => self.codex_line(line),
        }
    }

    fn claude_line(&mut self, line: &[u8]) {
        let custom = contains(line, b"custom-title");
        if !custom && !contains(line, b"ai-title") {
            return;
        }
        let Ok(v) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        let (kind, field) = match v.get("type").and_then(Value::as_str) {
            Some("custom-title") => ("custom", "customTitle"),
            Some("ai-title") => ("ai", "aiTitle"),
            _ => return,
        };
        let Some(raw) = v.get(field).and_then(Value::as_str) else {
            return;
        };
        let text = Some(clean_title(raw)).filter(|t| !t.is_empty());
        if kind == "custom" {
            self.custom = text;
        } else if text.is_some() {
            self.auto = text;
        }
    }

    fn codex_line(&mut self, line: &[u8]) {
        if !contains(line, b"thread_name") {
            return;
        }
        let Ok(v) = serde_json::from_slice::<Value>(line) else {
            return;
        };
        if let Some(id) = &self.id {
            if v.get("id").and_then(Value::as_str) != Some(id.as_str()) {
                return;
            }
        }
        if let Some(raw) = v.get("thread_name").and_then(Value::as_str) {
            self.custom = Some(clean_title(raw)).filter(|t| !t.is_empty());
        }
    }

    pub fn title(&self, base: &str) -> Option<AgentTitle> {
        let (text, auto) = match (&self.custom, &self.auto) {
            (Some(t), _) => (t.clone(), false),
            (None, Some(t)) => (t.clone(), true),
            _ => return None,
        };
        Some(AgentTitle {
            text,
            auto,
            base: base.to_string(),
        })
    }
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

pub fn age_label(word: &str, age: &str, max: usize) -> String {
    let full = format!("{word} {age}");
    if full.width() <= max {
        return full;
    }
    let room = max.saturating_sub(age.width() + 1);
    if room >= 2 {
        let cut = clip(word, room);
        if !cut.is_empty() {
            return format!("{cut} {age}");
        }
    }
    age.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const U: &str = "2f0e1c3a-5b7d-4e8f-9a1b-3c4d5e6f7a8b";

    fn scan(format: Format, id: Option<&str>) -> TitleScan {
        TitleScan::new(
            format,
            PathBuf::new(),
            PathBuf::new(),
            id.map(str::to_string),
        )
    }

    #[test]
    fn claude_latest_custom_and_auto_win() {
        let mut s = scan(Format::Claude, None);
        s.feed(
            br#"{"type":"user","message":"hi"}
{"type":"ai-title","aiTitle":"first auto"}
{"type":"custom-title","customTitle":"mine"}
{"type":"ai-title","aiTitle":"second auto"}
{"type":"custom-title","customTitle":"mine v2"}
"#,
        );
        let t = s.title("claude").unwrap();
        assert_eq!((t.text.as_str(), t.auto), ("mine v2", false));
        s.custom = None;
        let t = s.title("claude").unwrap();
        assert_eq!((t.text.as_str(), t.auto), ("second auto", true));
    }

    #[test]
    fn claude_auto_only_is_marked_auto() {
        let mut s = scan(Format::Claude, None);
        s.feed(b"{\"type\":\"ai-title\",\"aiTitle\":\"Fix the build\"}\n");
        assert!(s.title("c").unwrap().auto);
        assert!(scan(Format::Claude, None).title("c").is_none());
    }

    #[test]
    fn cleared_custom_title_falls_back_to_auto() {
        let mut s = scan(Format::Claude, None);
        s.feed(b"{\"type\":\"ai-title\",\"aiTitle\":\"auto\"}\n{\"type\":\"custom-title\",\"customTitle\":\"x\"}\n{\"type\":\"custom-title\",\"customTitle\":\"\"}\n");
        let t = s.title("c").unwrap();
        assert_eq!((t.text.as_str(), t.auto), ("auto", true));
    }

    #[test]
    fn partial_last_line_waits_for_its_newline() {
        let mut s = scan(Format::Claude, None);
        s.feed(b"{\"type\":\"custom-title\",\"customTitle\":\"one\"}\n{\"type\":\"custom-title\",\"custom");
        assert_eq!(s.title("c").unwrap().text, "one");
        s.feed(b"Title\":\"two\"}\n");
        assert_eq!(s.title("c").unwrap().text, "two");
    }

    #[test]
    fn garbage_unknown_and_wrong_typed_lines_are_ignored() {
        let mut s = scan(Format::Claude, None);
        s.feed(b"not json custom-title\n{\"type\":\"summary\",\"note\":\"ai-title\"}\n{\"type\":\"custom-title\",\"customTitle\":7}\n\n{\"type\":\"custom-title\"}\n");
        assert!(s.title("c").is_none());
    }

    #[test]
    fn oversized_line_is_skipped_without_losing_the_next() {
        let mut s = scan(Format::Claude, None);
        let big = vec![b'x'; LINE_CAP + 10];
        s.feed(&big);
        s.feed(&big);
        s.feed(b"tail\n{\"type\":\"custom-title\",\"customTitle\":\"ok\"}\n");
        assert_eq!(s.title("c").unwrap().text, "ok");
        assert!(s.carry.is_empty());
    }

    #[test]
    fn codex_latest_line_for_the_id_wins() {
        let other = "01a15ccd-f5c8-7fb1-a084-4cb65bea8b61";
        let mut s = scan(Format::Codex, Some(U));
        s.feed(
            format!(
                "{{\"id\":\"{U}\",\"thread_name\":\"old\"}}\n{{\"id\":\"{other}\",\"thread_name\":\"other\"}}\n{{\"id\":\"{U}\",\"thread_name\":\"new\"}}\n"
            )
            .as_bytes(),
        );
        let t = s.title("codex").unwrap();
        assert_eq!((t.text.as_str(), t.auto), ("new", false));
    }

    #[test]
    fn codex_without_an_id_takes_the_latest_line() {
        let mut s = scan(Format::Codex, None);
        s.feed(b"{\"id\":\"a\",\"thread_name\":\"first\"}\n{\"id\":\"b\",\"thread_name\":\"second\"}\n");
        assert_eq!(s.title("codex").unwrap().text, "second");
    }

    #[test]
    fn titles_are_sanitised() {
        assert_eq!(clean_title("\u{1b}[31mred\u{1b}[0m"), "red");
        assert_eq!(clean_title("a\u{1b}]0;evil\u{7}b"), "ab");
        assert_eq!(clean_title("a\u{1b}]0;evil\u{1b}\\b"), "ab");
        assert_eq!(clean_title("  two\nlines\t here  "), "two lines here");
        assert_eq!(
            clean_title("rtl\u{202e}txt\u{2066}\u{feff}\u{9b}"),
            "rtltxt"
        );
        assert_eq!(clean_title("\u{1b}"), "");
        assert_eq!(clean_title(&"я".repeat(500)).chars().count(), TITLE_MAX);
    }

    #[test]
    fn sanitised_title_through_json_escapes() {
        let mut s = scan(Format::Claude, None);
        s.feed(b"{\"type\":\"custom-title\",\"customTitle\":\"\\u001b[2Jwipe\\n\\u001b]52;c;eA==\\u0007ok\"}\n");
        assert_eq!(s.title("c").unwrap().text, "wipe ok");
    }

    #[test]
    fn clip_respects_display_width() {
        assert_eq!(clip("abcdef", 6), "abcdef");
        assert_eq!(clip("abcdef", 4), "abc…");
        assert_eq!(clip("世界世界", 5), "世界…");
        assert_eq!(clip("abc", 0), "");
    }

    #[test]
    fn age_label_truncates_the_word_before_the_age() {
        assert_eq!(age_label("waiting", "2s", 20), "waiting 2s");
        assert_eq!(age_label("waiting", "15s", 11), "waiting 15s");
        assert_eq!(age_label("waiting", "15s", 8), "wai… 15s");
        assert_eq!(age_label("working", "2s", 6), "wo… 2s");
        assert_eq!(age_label("working", "2s", 5), "w… 2s");
        assert_eq!(age_label("working", "2s", 4), "2s");
        assert_eq!(age_label("working", "15s", 3), "15s");
        assert_eq!(age_label("working", "15s", 0), "15s");
    }

    #[test]
    fn encodes_cwd_like_claude() {
        assert_eq!(
            encode_cwd("/Users/me/projects/own-projects/app.dev"),
            "-Users-me-projects-own-projects-app-dev"
        );
        assert_eq!(
            encode_cwd("/Users/me/.tmgr-local/board"),
            "-Users-me--tmgr-local-board"
        );
        assert_eq!(encode_cwd("/a b/c_d"), "-a-b-c-d");
    }

    fn home() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn write(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn src<'a>(home: &'a Path, cwd: &'a str, pid: u32) -> Source<'a> {
        Source {
            home,
            format: Format::Claude,
            cwd,
            pid,
            log_id: None,
        }
    }

    #[test]
    fn locate_uses_the_pid_file_session_id() {
        let h = home();
        let cwd = "/work/app.dev";
        let want = h
            .path()
            .join(format!(".claude/projects/-work-app-dev/{U}.jsonl"));
        write(&want, "");
        write(
            &h.path()
                .join(".claude/projects/-work-app-dev/aaaaaaaa-5b7d-4e8f-9a1b-3c4d5e6f7a8b.jsonl"),
            "",
        );
        write(
            &h.path().join(".claude/sessions/77.json"),
            &format!("{{\"pid\":77,\"sessionId\":\"{U}\"}}"),
        );
        let (path, id) = locate(&src(h.path(), cwd, 77)).unwrap();
        assert_eq!(path, want);
        assert_eq!(id.as_deref(), Some(U));
    }

    #[test]
    fn locate_rejects_a_pid_file_for_another_pid_or_a_bad_id() {
        let h = home();
        write(
            &h.path().join(".claude/sessions/77.json"),
            &format!("{{\"pid\":78,\"sessionId\":\"{U}\"}}"),
        );
        assert!(locate(&src(h.path(), "/w", 77)).is_none());
        write(
            &h.path().join(".claude/sessions/77.json"),
            "{\"pid\":77,\"sessionId\":\"../../etc/passwd\"}",
        );
        assert!(locate(&src(h.path(), "/w", 77)).is_none());
    }

    #[test]
    fn locate_guesses_the_newest_transcript_without_an_id() {
        let h = home();
        let dir = h.path().join(".claude/projects/-w");
        write(&dir.join("old.jsonl"), "");
        std::thread::sleep(std::time::Duration::from_millis(20));
        write(&dir.join("new.jsonl"), "");
        let (path, id) = locate(&src(h.path(), "/w", 0)).unwrap();
        assert_eq!(path, dir.join("new.jsonl"));
        assert_eq!(id, None);
    }

    #[test]
    fn locate_falls_back_to_the_log_id_and_other_project_dirs() {
        let h = home();
        let p = h
            .path()
            .join(format!(".claude/projects/-elsewhere/{U}.jsonl"));
        write(&p, "");
        let mut s = src(h.path(), "/w", 0);
        s.log_id = Some(U);
        assert_eq!(locate(&s).unwrap().0, p);
        s.log_id = Some("nope");
        assert!(locate(&s).is_none());
    }

    #[test]
    fn locate_codex_uses_the_index_and_a_valid_log_id() {
        let h = home();
        let mut s = src(h.path(), "/w", 0);
        s.format = Format::Codex;
        let (p, id) = locate(&s).unwrap();
        assert_eq!(p, h.path().join(".codex/session_index.jsonl"));
        assert_eq!(id, None);
        s.log_id = Some(U);
        assert_eq!(locate(&s).unwrap().1.as_deref(), Some(U));
        s.log_id = Some("../x");
        assert_eq!(locate(&s).unwrap().1, None);
    }

    #[test]
    fn poll_reads_incrementally_and_survives_truncation() {
        let h = home();
        let p = h.path().join("t.jsonl");
        write(&p, "{\"type\":\"ai-title\",\"aiTitle\":\"one\"}\n{\"type\":\"custom-title\",\"customTitle\":\"par");
        let mut s = TitleScan::new(Format::Claude, h.path().to_path_buf(), p.clone(), None);
        s.poll();
        assert_eq!(s.title("c").unwrap().text, "one");
        let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
        f.write_all(b"tial\"}\n").unwrap();
        drop(f);
        s.poll();
        assert_eq!(s.title("c").unwrap().text, "partial");
        std::fs::write(
            &p,
            "{\"type\":\"custom-title\",\"customTitle\":\"fresh\"}\n",
        )
        .unwrap();
        s.poll();
        assert_eq!(s.title("c").unwrap().text, "fresh");
        std::fs::remove_file(&p).unwrap();
        s.poll();
        assert_eq!(s.title("c").unwrap().text, "fresh");
    }

    #[test]
    fn poll_never_follows_a_symlink() {
        let h = home();
        let real = h.path().join("real.jsonl");
        write(
            &real,
            "{\"type\":\"custom-title\",\"customTitle\":\"secret\"}\n",
        );
        let link = h.path().join("link.jsonl");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let mut s = TitleScan::new(Format::Claude, h.path().to_path_buf(), link, None);
        s.poll();
        assert!(s.title("c").is_none());
    }

    #[test]
    fn poll_ignores_a_fifo_without_blocking() {
        let h = home();
        let fifo = h.path().join("f.jsonl");
        let c = std::ffi::CString::new(fifo.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
        let mut s = TitleScan::new(Format::Claude, h.path().to_path_buf(), fifo, None);
        s.poll();
        assert!(s.title("c").is_none());
    }

    #[test]
    fn poll_never_follows_a_symlinked_directory_out_of_the_root() {
        let h = home();
        let outside = home();
        write(
            &outside.path().join("session_index.jsonl"),
            "{\"id\":\"a\",\"thread_name\":\"host secret\"}\n",
        );
        std::os::unix::fs::symlink(outside.path(), h.path().join(".codex")).unwrap();
        let (path, _) = locate(&Source {
            home: h.path(),
            format: Format::Codex,
            cwd: "/w",
            pid: 0,
            log_id: None,
        })
        .unwrap();
        let mut s = TitleScan::new(Format::Codex, h.path().to_path_buf(), path, None);
        s.poll();
        assert!(s.title("codex").is_none());
    }

    #[test]
    fn poll_rereads_a_same_inode_rewrite_of_equal_length() {
        let h = home();
        let p = h.path().join("t.jsonl");
        write(&p, "{\"type\":\"custom-title\",\"customTitle\":\"aaaa\"}\n");
        let mut s = TitleScan::new(Format::Claude, h.path().to_path_buf(), p.clone(), None);
        s.poll();
        assert_eq!(s.title("c").unwrap().text, "aaaa");
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut f = std::fs::OpenOptions::new().write(true).open(&p).unwrap();
        f.write_all(b"{\"type\":\"custom-title\",\"customTitle\":\"bbbb\"}\n")
            .unwrap();
        drop(f);
        s.poll();
        assert_eq!(s.title("c").unwrap().text, "bbbb");
    }

    #[test]
    fn poll_catches_up_over_several_passes() {
        let h = home();
        let p = h.path().join("t.jsonl");
        let filler = format!("{{\"type\":\"user\",\"m\":\"{}\"}}\n", "x".repeat(1000));
        let mut body = String::from("{\"type\":\"custom-title\",\"customTitle\":\"early\"}\n");
        while (body.len() as u64) < PASS_CAP * 2 + 100 {
            body.push_str(&filler);
        }
        write(&p, &body);
        let mut s = TitleScan::new(Format::Claude, h.path().to_path_buf(), p, None);
        s.poll();
        assert_eq!(s.offset, body.len() as u64);
        assert_eq!(s.title("c").unwrap().text, "early");
    }

    #[test]
    fn invisible_and_tag_characters_are_stripped() {
        assert_eq!(
            clean_title("a\u{200b}b\u{2028}c\u{61c}d\u{2060}e\u{e0041}f"),
            "abcdef"
        );
    }
}
