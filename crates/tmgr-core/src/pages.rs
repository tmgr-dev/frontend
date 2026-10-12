use crate::error::{Error, Result};
use crate::refs::is_workspace_code;
use rustix::fs::{AtFlags, Mode, OFlags};
use rustix::io::Errno;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::fd::OwnedFd;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};

pub const INDEX_FILE: &str = ".tmgr-pages.json";
pub const INDEX_SCHEMA: &str = "tmgr.pages.v1";
pub const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_SLUG: usize = 60;
const MAX_INDEX_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub struct ApiPage {
    pub id: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default, rename = "type")]
    pub kind: Option<String>,
    #[serde(default)]
    pub parent_id: Option<u64>,
    #[serde(default)]
    pub updated_at: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
    #[serde(default)]
    pub version: Option<i64>,
}

pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut dash = true;
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
            dash = false;
        } else if !dash {
            out.push('-');
            dash = true;
        }
    }
    let mut out: String = out.trim_matches('-').chars().take(MAX_SLUG).collect();
    while out.ends_with('-') {
        out.pop();
    }
    if out.is_empty() {
        "page".to_string()
    } else {
        out
    }
}

fn slug_ok(s: &str) -> bool {
    (1..=MAX_SLUG).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub fn is_page_file(name: &str) -> bool {
    name.strip_suffix(".md").is_some_and(slug_ok)
}

pub fn remote_name(file: &str) -> Option<String> {
    let slug = file.strip_suffix(".md").filter(|s| slug_ok(s))?;
    Some(format!("{slug}.remote.md"))
}

pub fn choose_file_name(title: &str, id: u64, taken: &dyn Fn(&str) -> bool) -> Option<String> {
    let base = slugify(title);
    let plain = format!("{base}.md");
    if !taken(&plain) {
        return Some(plain);
    }
    let suffix = format!("-{id}");
    let keep = MAX_SLUG.saturating_sub(suffix.len());
    let mut stem: String = base.chars().take(keep).collect();
    while stem.ends_with('-') {
        stem.pop();
    }
    let with_id = format!("{stem}{suffix}.md");
    (!taken(&with_id) && is_page_file(&with_id)).then_some(with_id)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrontMatter {
    pub page_id: u64,
    pub version: i64,
    pub updated_at: Option<String>,
    pub workspace: String,
}

pub fn render(fm: &FrontMatter, body: &str) -> String {
    let mut out = format!(
        "---\ntmgr:\n  page_id: {}\n  version: {}\n",
        fm.page_id, fm.version
    );
    if let Some(u) = fm.updated_at.as_deref().filter(|u| safe_value(u)) {
        out.push_str(&format!("  updated_at: {u}\n"));
    }
    out.push_str(&format!("  workspace: {}\n---\n", fm.workspace));
    out.push_str(body);
    out
}

fn safe_value(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 64
        && v.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.' | b'+'))
}

fn bad_front_matter(why: &str) -> Error {
    Error::usage(format!("invalid front matter: {why}"))
        .hint("restore the `tmgr:` block written by `tmgr page pull`")
}

pub fn split(content: &str) -> Result<(FrontMatter, &str)> {
    let rest = content
        .strip_prefix("---\n")
        .ok_or_else(|| bad_front_matter("the file does not start with ---"))?;
    let mut offset = 0;
    let mut block = Vec::new();
    let mut closed = false;
    for line in rest.split_inclusive('\n') {
        offset += line.len();
        if line.trim_end_matches('\n') == "---" {
            closed = true;
            break;
        }
        block.push(line.trim_end_matches('\n'));
        if block.len() > 16 {
            break;
        }
    }
    if !closed {
        return Err(bad_front_matter("the block is not closed with ---"));
    }
    let body = &rest[offset..];
    if block.first() != Some(&"tmgr:") {
        return Err(bad_front_matter("missing the tmgr: block"));
    }
    let (mut page_id, mut version, mut updated_at, mut workspace) = (None, None, None, None);
    for line in &block[1..] {
        let (key, value) = line
            .strip_prefix("  ")
            .and_then(|l| l.split_once(": "))
            .ok_or_else(|| bad_front_matter("unexpected line in the tmgr: block"))?;
        match key {
            "page_id" => page_id = value.parse::<u64>().ok().filter(|v| *v > 0),
            "version" => version = value.parse::<i64>().ok().filter(|v| *v >= 0),
            "updated_at" if safe_value(value) => updated_at = Some(value.to_string()),
            "workspace" if is_workspace_code(value) => workspace = Some(value.to_string()),
            _ => {}
        }
    }
    Ok((
        FrontMatter {
            page_id: page_id.ok_or_else(|| bad_front_matter("page_id"))?,
            version: version.ok_or_else(|| bad_front_matter("version"))?,
            updated_at,
            workspace: workspace.ok_or_else(|| bad_front_matter("workspace"))?,
        },
        body,
    ))
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IndexEntry {
    pub page_id: u64,
    pub version: i64,
    #[serde(default)]
    pub updated_at: Option<String>,
    pub body_sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Index {
    pub schema: String,
    pub workspace: String,
    pub pages: BTreeMap<String, IndexEntry>,
}

impl Index {
    pub fn new(workspace: &str) -> Self {
        Self {
            schema: INDEX_SCHEMA.to_string(),
            workspace: workspace.to_string(),
            pages: BTreeMap::new(),
        }
    }

    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let bad = || Error::usage(format!("{INDEX_FILE} is damaged"));
        let index: Index = serde_json::from_slice(bytes).map_err(|_| bad())?;
        if index.schema != INDEX_SCHEMA
            || !is_workspace_code(&index.workspace)
            || index
                .pages
                .iter()
                .any(|(name, e)| !is_page_file(name) || e.page_id == 0)
        {
            return Err(bad());
        }
        Ok(index)
    }

    pub fn file_for(&self, page_id: u64) -> Option<&str> {
        self.pages
            .iter()
            .find(|(_, e)| e.page_id == page_id)
            .map(|(n, _)| n.as_str())
    }
}

fn errno(what: &str, name: &str, e: Errno) -> Error {
    if e == Errno::LOOP || e == Errno::MLINK {
        return Error::usage(format!("{name} is a symbolic link; refusing to follow it"));
    }
    Error::internal(format!("cannot {what} {name}: {}", e.kind_name()))
}

trait KindName {
    fn kind_name(&self) -> String;
}

impl KindName for Errno {
    fn kind_name(&self) -> String {
        std::io::Error::from_raw_os_error(self.raw_os_error())
            .kind()
            .to_string()
    }
}

pub struct PageDir {
    fd: OwnedFd,
}

static TMP_COUNTER: AtomicU32 = AtomicU32::new(0);

impl PageDir {
    pub fn open(path: &Path, create: bool) -> Result<Self> {
        if create {
            std::fs::create_dir_all(path)?;
        }
        let fd = rustix::fs::open(
            path,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map_err(|e| {
            if e == Errno::NOENT {
                Error::not_found("directory not found").hint("tmgr page pull <dir>")
            } else {
                errno("open", "the directory", e)
            }
        })?;
        Ok(Self { fd })
    }

    pub fn read(&self, name: &str, max: u64) -> Result<Option<Vec<u8>>> {
        ensure_plain(name)?;
        let fd = match rustix::fs::openat(
            &self.fd,
            name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        ) {
            Ok(fd) => fd,
            Err(Errno::NOENT) => return Ok(None),
            Err(e) => return Err(errno("open", name, e)),
        };
        let stat = rustix::fs::fstat(&fd).map_err(|e| errno("stat", name, e))?;
        if rustix::fs::FileType::from_raw_mode(stat.st_mode) != rustix::fs::FileType::RegularFile {
            return Err(Error::usage(format!("{name} is not a regular file")));
        }
        let mut buf = Vec::new();
        std::fs::File::from(fd)
            .take(max + 1)
            .read_to_end(&mut buf)
            .map_err(|_| Error::internal(format!("cannot read {name}")))?;
        if buf.len() as u64 > max {
            return Err(Error::usage(format!("{name} is too large")));
        }
        Ok(Some(buf))
    }

    pub fn exists(&self, name: &str) -> Result<bool> {
        ensure_plain(name)?;
        match rustix::fs::statat(&self.fd, name, AtFlags::SYMLINK_NOFOLLOW) {
            Ok(_) => Ok(true),
            Err(Errno::NOENT) => Ok(false),
            Err(e) => Err(errno("stat", name, e)),
        }
    }

    pub fn write_atomic(&self, name: &str, bytes: &[u8]) -> Result<()> {
        ensure_plain(name)?;
        let tmp = format!(
            ".{name}.tmgr-tmp-{}-{}",
            std::process::id(),
            TMP_COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let fd = rustix::fs::openat(
            &self.fd,
            tmp.as_str(),
            OFlags::WRONLY | OFlags::CREATE | OFlags::EXCL | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::from_raw_mode(0o666),
        )
        .map_err(|e| errno("create", name, e))?;
        let mut file = std::fs::File::from(fd);
        let written = file.write_all(bytes).and_then(|_| file.sync_all());
        drop(file);
        if written.is_err() {
            let _ = rustix::fs::unlinkat(&self.fd, tmp.as_str(), AtFlags::empty());
            return Err(Error::internal(format!("cannot write {name}")));
        }
        if let Err(e) = rustix::fs::renameat(&self.fd, tmp.as_str(), &self.fd, name) {
            let _ = rustix::fs::unlinkat(&self.fd, tmp.as_str(), AtFlags::empty());
            return Err(errno("replace", name, e));
        }
        Ok(())
    }

    pub fn load_index(&self) -> Result<Option<Index>> {
        self.read(INDEX_FILE, MAX_INDEX_BYTES)?
            .map(|b| Index::parse(&b))
            .transpose()
    }

    pub fn save_index(&self, index: &Index) -> Result<()> {
        let mut text = serde_json::to_string_pretty(index)
            .map_err(|_| Error::internal("cannot serialize the page index"))?;
        text.push('\n');
        self.write_atomic(INDEX_FILE, text.as_bytes())
    }
}

fn ensure_plain(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 255
        || name.contains('/')
        || name.contains('\0')
        || name == "."
        || name == ".."
    {
        return Err(Error::usage("invalid file name"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn slugs_follow_the_strict_grammar() {
        assert_eq!(slugify("Deploy checklist"), "deploy-checklist");
        assert_eq!(slugify("../../etc/passwd"), "etc-passwd");
        assert_eq!(slugify("\u{1b}[31mRed\u{1b}[0m"), "31mred-0m");
        assert_eq!(slugify("  --  "), "page");
        assert_eq!(
            slugify("\u{41f}\u{440}\u{438}\u{432}\u{435}\u{442}"),
            "page"
        );
        let long = slugify(&"ab ".repeat(100));
        assert!(long.len() <= 60 && !long.ends_with('-'));
        assert!(is_page_file(&format!("{long}.md")));
    }

    #[test]
    fn file_names_get_an_id_suffix_on_collision() {
        let none = |_: &str| false;
        assert_eq!(choose_file_name("Notes", 5, &none).unwrap(), "notes.md");
        let taken = |n: &str| n == "notes.md";
        assert_eq!(choose_file_name("Notes", 5, &taken).unwrap(), "notes-5.md");
        let all = |_: &str| true;
        assert!(choose_file_name("Notes", 5, &all).is_none());
        let long = "x".repeat(100);
        let full = format!("{}.md", "x".repeat(60));
        let taken = |n: &str| n == full;
        let name = choose_file_name(&long, 123456, &taken).unwrap();
        assert!(
            is_page_file(&name) && name.ends_with("-123456.md"),
            "{name}"
        );
    }

    #[test]
    fn page_file_grammar_rejects_paths_and_dots() {
        for ok in ["a.md", "deploy-checklist.md"] {
            assert!(is_page_file(ok), "{ok}");
        }
        for bad in [
            "",
            ".md",
            "A.md",
            "a b.md",
            "../a.md",
            "a/b.md",
            "a.remote.md",
            ".hidden.md",
            "a.txt",
        ] {
            assert!(!is_page_file(bad), "{bad:?}");
        }
        assert_eq!(remote_name("a.md").unwrap(), "a.remote.md");
        assert!(remote_name("../a.md").is_none());
    }

    fn fm() -> FrontMatter {
        FrontMatter {
            page_id: 12,
            version: 3,
            updated_at: Some("2026-10-12T10:00:00Z".into()),
            workspace: "personal".into(),
        }
    }

    #[test]
    fn front_matter_round_trips_and_leaves_the_body_untouched() {
        let body = "# T\n\n---\nnot front matter\n";
        let text = render(&fm(), body);
        let (parsed, rest) = split(&text).unwrap();
        assert_eq!(parsed, fm());
        assert_eq!(rest, body);
        let rendered = render(&fm(), "");
        let (_, empty) = split(&rendered).unwrap();
        assert_eq!(empty, "");
    }

    #[test]
    fn front_matter_rejects_garbage() {
        for bad in [
            "",
            "# no front matter",
            "---\ntmgr:\n  page_id: 1\n  version: 1\n  workspace: w\n",
            "---\ntmgr:\n  page_id: 0\n  version: 1\n  workspace: w\n---\n",
            "---\ntmgr:\n  page_id: 1\n  version: x\n  workspace: w\n---\n",
            "---\ntmgr:\n  page_id: 1\n  version: 1\n  workspace: a/b\n---\n",
            "---\nother:\n  page_id: 1\n---\n",
            "---\ntmgr:\n  page_id: 1\n  version: 1\n  workspace: w\n  $(x)\n---\n",
        ] {
            assert!(split(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn hashes_are_sha256() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn atomic_writes_replace_files_and_leave_no_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let d = PageDir::open(dir.path(), false).unwrap();
        d.write_atomic("a.md", b"one").unwrap();
        d.write_atomic("a.md", b"two").unwrap();
        assert_eq!(d.read("a.md", 100).unwrap().unwrap(), b"two");
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["a.md"]);
        assert!(d.read("missing.md", 10).unwrap().is_none());
        assert!(d.read("a.md", 2).is_err());
    }

    #[test]
    fn symlinks_are_never_followed_for_reads_or_writes() {
        let dir = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let target = outside.path().join("secret.txt");
        std::fs::write(&target, "secret").unwrap();
        symlink(&target, dir.path().join("a.md")).unwrap();
        let d = PageDir::open(dir.path(), false).unwrap();
        let e = d.read("a.md", 100).unwrap_err();
        assert_eq!(e.exit_code(), 2);
        assert!(d.exists("a.md").unwrap());
        d.write_atomic("a.md", b"new").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "secret");
        assert_eq!(std::fs::read(dir.path().join("a.md")).unwrap(), b"new");
        assert!(!std::fs::symlink_metadata(dir.path().join("a.md"))
            .unwrap()
            .is_symlink());
    }

    #[test]
    fn names_with_separators_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let d = PageDir::open(dir.path(), false).unwrap();
        for bad in ["../x.md", "a/b.md", "", "..", "."] {
            assert!(d.write_atomic(bad, b"x").is_err(), "{bad:?}");
            assert!(d.read(bad, 10).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn index_round_trips_and_rejects_foreign_names() {
        let dir = tempfile::tempdir().unwrap();
        let d = PageDir::open(dir.path(), false).unwrap();
        assert!(d.load_index().unwrap().is_none());
        let mut idx = Index::new("personal");
        idx.pages.insert(
            "a.md".into(),
            IndexEntry {
                page_id: 1,
                version: 2,
                updated_at: None,
                body_sha256: "x".into(),
            },
        );
        d.save_index(&idx).unwrap();
        assert_eq!(d.load_index().unwrap().unwrap().file_for(1), Some("a.md"));
        let mut bad = idx.clone();
        bad.pages
            .insert("../evil.md".into(), idx.pages["a.md"].clone());
        d.write_atomic(INDEX_FILE, serde_json::to_string(&bad).unwrap().as_bytes())
            .unwrap();
        assert!(d.load_index().is_err());
    }
}
