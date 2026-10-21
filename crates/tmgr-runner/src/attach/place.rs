use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const HEAD_CAP: u64 = 4096;
const SEP: &str = " · ";
const ELLIPSIS: char = '…';

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loc {
    pub root: String,
    pub dir: String,
    pub branch: Option<String>,
}

pub fn probe(cwd: &Path) -> Loc {
    let real = std::fs::canonicalize(cwd);
    let cwd = real.as_deref().unwrap_or(cwd);
    let dir = cwd.to_string_lossy().into_owned();
    for ancestor in cwd.ancestors() {
        if let Some(head) = head_path(&ancestor.join(".git")) {
            return Loc {
                root: ancestor.to_string_lossy().into_owned(),
                dir,
                branch: read_head(&head).and_then(|h| parse_head(&h)),
            };
        }
    }
    Loc {
        root: dir.clone(),
        dir,
        branch: None,
    }
}

fn head_path(dot_git: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(dot_git).ok()?;
    if meta.is_dir() {
        return Some(dot_git.join("HEAD"));
    }
    if !meta.is_file() {
        return None;
    }
    let text = read_capped(dot_git)?;
    let target = text.lines().next()?.strip_prefix("gitdir:")?.trim();
    if target.is_empty() {
        return None;
    }
    let target = Path::new(target);
    let base = dot_git.parent()?;
    Some(base.join(target).join("HEAD"))
}

fn read_capped(path: &Path) -> Option<String> {
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let mut buf = Vec::new();
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .ok()?
        .take(HEAD_CAP)
        .read_to_end(&mut buf)
        .ok()?;
    Some(String::from_utf8_lossy(&buf).into_owned())
}

fn read_head(path: &Path) -> Option<String> {
    read_capped(path)
}

fn parse_head(raw: &str) -> Option<String> {
    let text = raw.trim();
    if let Some(r) = text.strip_prefix("ref:") {
        let r = r.trim();
        let name = match r.strip_prefix("refs/heads/") {
            Some(n) => n,
            None => r.strip_prefix("refs/")?,
        };
        return (!name.is_empty()).then(|| name.to_string());
    }
    let is_hex = text
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
    if is_hex && (text.len() == 40 || text.len() == 64) {
        return Some(text[..7].to_string());
    }
    None
}

pub fn tail2(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return if path.is_empty() {
            String::new()
        } else {
            "/".into()
        };
    }
    let parts: Vec<&str> = trimmed.split('/').filter(|p| !p.is_empty()).collect();
    match parts.as_slice() {
        [] | [_] => trimmed.to_string(),
        [.., a, b] => format!("{a}/{b}"),
    }
}

pub fn fit(path: &str, branch: Option<&str>, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let Some(branch) = branch else {
        return trunc_mid(path, width);
    };
    let sep_w = SEP.width();
    if path.width() + sep_w + branch.width() <= width {
        return format!("{path}{SEP}{branch}");
    }
    let b = trunc_mid(branch, width / 2);
    let Some(path_max) = width.checked_sub(b.width() + sep_w).filter(|m| *m > 0) else {
        return trunc_mid(path, width);
    };
    format!("{}{SEP}{b}", trunc_mid(path, path_max))
}

fn trunc_mid(s: &str, max: usize) -> String {
    if s.width() <= max {
        return s.to_string();
    }
    match max {
        0 => return String::new(),
        1 => return ELLIPSIS.to_string(),
        _ => {}
    }
    let budget = max - 1;
    let left_w = budget.div_ceil(2);
    let right_w = budget - left_w;
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > left_w {
            break;
        }
        used += w;
        out.push(c);
    }
    out.push(ELLIPSIS);
    let mut tail: Vec<char> = Vec::new();
    let mut used = 0;
    for c in s.chars().rev() {
        let w = c.width().unwrap_or(0);
        if used + w > right_w {
            break;
        }
        used += w;
        tail.push(c);
    }
    out.extend(tail.into_iter().rev());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    const SHA40: &str = "0123456789abcdef0123456789abcdef01234567";

    fn repo(head: &str) -> TempDir {
        let t = TempDir::new().unwrap();
        fs::create_dir(t.path().join(".git")).unwrap();
        fs::write(t.path().join(".git/HEAD"), head).unwrap();
        t
    }

    fn s(p: &Path) -> String {
        std::fs::canonicalize(p)
            .unwrap_or_else(|_| p.to_path_buf())
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn probe_reads_branch_name() {
        let t = repo("ref: refs/heads/main\n");
        let loc = probe(t.path());
        assert_eq!(loc.branch.as_deref(), Some("main"));
        assert_eq!(loc.root, s(t.path()));
        assert_eq!(loc.dir, s(t.path()));
    }

    #[test]
    fn probe_keeps_slashes_in_branch() {
        let t = repo("ref: refs/heads/feature/login/v2\n");
        assert_eq!(probe(t.path()).branch.as_deref(), Some("feature/login/v2"));
    }

    #[test]
    fn probe_non_heads_ref_drops_refs_prefix() {
        let t = repo("ref: refs/remotes/origin/main\n");
        assert_eq!(
            probe(t.path()).branch.as_deref(),
            Some("remotes/origin/main")
        );
    }

    #[test]
    fn probe_detached_head_shows_short_sha() {
        let t = repo(&format!("{SHA40}\n"));
        assert_eq!(probe(t.path()).branch.as_deref(), Some("0123456"));
        let t = repo(&format!("{SHA40}{}\n", &SHA40[..24]));
        assert_eq!(probe(t.path()).branch.as_deref(), Some("0123456"));
    }

    #[test]
    fn probe_worktree_file_with_relative_gitdir() {
        let t = TempDir::new().unwrap();
        let real = t.path().join("main/.git/worktrees/wt");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("HEAD"), "ref: refs/heads/wt-branch\n").unwrap();
        let wt = t.path().join("wt");
        fs::create_dir(&wt).unwrap();
        fs::write(wt.join(".git"), "gitdir: ../main/.git/worktrees/wt\n").unwrap();
        let loc = probe(&wt);
        assert_eq!(loc.branch.as_deref(), Some("wt-branch"));
        assert_eq!(loc.root, s(&wt));
    }

    #[test]
    fn probe_worktree_file_with_absolute_gitdir() {
        let t = TempDir::new().unwrap();
        let real = t.path().join("main/.git/worktrees/wt");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("HEAD"), "ref: refs/heads/abs\n").unwrap();
        let wt = t.path().join("wt");
        fs::create_dir(&wt).unwrap();
        fs::write(wt.join(".git"), format!("gitdir: {}\n", real.display())).unwrap();
        let loc = probe(&wt);
        assert_eq!(loc.branch.as_deref(), Some("abs"));
        assert_eq!(loc.root, s(&wt));
    }

    #[test]
    fn probe_walks_up_from_subfolder() {
        let t = repo("ref: refs/heads/dev\n");
        let sub = t.path().join("a/b");
        fs::create_dir_all(&sub).unwrap();
        let loc = probe(&sub);
        assert_eq!(loc.root, s(t.path()));
        assert_eq!(loc.dir, s(&sub));
        assert_eq!(loc.branch.as_deref(), Some("dev"));
    }

    #[test]
    fn probe_non_git_folder_uses_cwd_as_root() {
        let t = TempDir::new().unwrap();
        let loc = probe(t.path());
        if loc.branch.is_none() && loc.root == s(t.path()) {
            assert_eq!(loc.dir, s(t.path()));
        }
        let missing = t.path().join("nope");
        let loc = probe(&missing);
        assert_eq!(loc.dir, s(&missing));
    }

    #[test]
    fn probe_garbage_head_keeps_root_without_branch() {
        for head in [
            "",
            "hello world\n",
            "ref:\n",
            "ref: refs/heads/\n",
            "ABCDEF\n",
        ] {
            let t = repo(head);
            let loc = probe(t.path());
            assert_eq!(loc.root, s(t.path()), "head {head:?}");
            assert_eq!(loc.branch, None, "head {head:?}");
        }
    }

    #[test]
    fn probe_missing_head_keeps_root_without_branch() {
        let t = TempDir::new().unwrap();
        fs::create_dir(t.path().join(".git")).unwrap();
        let loc = probe(t.path());
        assert_eq!(loc.root, s(t.path()));
        assert_eq!(loc.branch, None);
    }

    #[test]
    fn probe_oversized_head_does_not_panic() {
        let t = repo(&"x".repeat(100_000));
        assert_eq!(probe(t.path()).branch, None);
    }

    #[test]
    fn tail2_takes_last_two_components() {
        assert_eq!(tail2("/a/b/c/d"), "c/d");
        assert_eq!(tail2("/a/b"), "a/b");
        assert_eq!(tail2("a/b/c"), "b/c");
    }

    #[test]
    fn tail2_short_and_root_paths() {
        assert_eq!(tail2("/a"), "/a");
        assert_eq!(tail2("/"), "/");
        assert_eq!(tail2(""), "");
        assert_eq!(tail2("a"), "a");
    }

    #[test]
    fn tail2_ignores_trailing_slash() {
        assert_eq!(tail2("/a/b/c/d/"), "c/d");
        assert_eq!(tail2("/a/"), "/a");
        assert_eq!(tail2("//"), "/");
    }

    #[test]
    fn fit_returns_everything_when_it_fits() {
        assert_eq!(fit("proj/app", Some("main"), 40), "proj/app · main");
        assert_eq!(fit("proj/app", Some("main"), 15), "proj/app · main");
    }

    #[test]
    fn fit_without_branch_is_just_path() {
        assert_eq!(fit("proj/app", None, 40), "proj/app");
        assert_eq!(fit("abcdefghij", None, 6), "abc…ij");
    }

    #[test]
    fn fit_long_path_truncates_in_the_middle() {
        let out = fit("very/long/path/segment", Some("main"), 18);
        assert_eq!(out, "very/…gment · main");
        assert!(out.width() <= 18);
        assert!(out.ends_with(" · main"));
        assert!(out.contains('…'));
    }

    #[test]
    fn fit_long_branch_gets_at_most_half() {
        let out = fit("app", Some("feature/long-branch-name"), 20);
        assert!(out.width() <= 20);
        assert!(out.starts_with("app · "));
        let branch = out.strip_prefix("app · ").unwrap();
        assert!(branch.width() <= 10);
        assert!(branch.contains('…'));
    }

    #[test]
    fn fit_both_long_truncates_both() {
        let out = fit(
            "very/long/path/segment",
            Some("feature/long-branch-name"),
            24,
        );
        assert!(out.width() <= 24);
        let (p, b) = out.split_once(" · ").unwrap();
        assert!(p.contains('…') && b.contains('…'));
        assert!(b.width() <= 12);
    }

    #[test]
    fn fit_middle_ellipsis_example() {
        assert_eq!(fit("p", Some("feature/long-name"), 14), "p · fea…ame");
    }

    #[test]
    fn fit_tiny_width_never_exceeds() {
        assert_eq!(fit("abc", Some("main"), 0), "");
        for w in 1..12 {
            let out = fit("some/path", Some("branch"), w);
            assert!(out.width() <= w, "width {w}: {out:?}");
            let out = fit("some/path", None, w);
            assert!(out.width() <= w, "width {w}: {out:?}");
        }
        assert_eq!(fit("abc", None, 1), "…");
    }

    #[test]
    fn fit_wide_chars_are_not_split() {
        let out = fit("日本語のパス", Some("ブランチ"), 11);
        assert!(out.width() <= 11, "{out:?}");
        let out = fit("日本語のパス", None, 8);
        assert!(out.width() <= 8, "{out:?}");
        assert!(out.contains('…'));
        let out = fit("日本語", None, 4);
        assert_eq!(out, "日…");
    }
}
