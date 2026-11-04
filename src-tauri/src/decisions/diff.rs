use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;
use tmgr_ipc::decisions::{DiffSpec, PatchIn};

const MAX_BYTES: usize = 2 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(10);
const REF_MAX: usize = 200;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DiffLine {
  pub kind: &'static str,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub old: Option<u32>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub new: Option<u32>,
  pub text: String,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Hunk {
  pub header: String,
  pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DiffFile {
  pub path: String,
  pub added: u32,
  pub removed: u32,
  pub hunks: Vec<Hunk>,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct DiffView {
  pub files: Vec<DiffFile>,
  pub truncated: bool,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct DiffGroup {
  pub key: String,
  pub label: String,
  pub kind: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub base: Option<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub head: Option<String>,
  pub view: DiffView,
}

#[derive(Debug, Clone, Default, Serialize, PartialEq)]
pub struct DiffGroups {
  pub groups: Vec<DiffGroup>,
}

fn failed(msg: impl Into<String>) -> DiffView {
  DiffView { error: Some(msg.into()), ..Default::default() }
}

pub fn valid_ref(s: &str) -> bool {
  !s.is_empty()
    && s.len() <= REF_MAX
    && !s.starts_with('-')
    && !s.contains("..")
    && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '~' | '^' | '@' | '-'))
}

pub fn git_args(spec: &DiffSpec) -> Result<Vec<String>, String> {
  let base = spec.base.as_deref().unwrap_or("HEAD");
  if !valid_ref(base) {
    return Err("invalid base ref".into());
  }
  let mut args: Vec<String> =
    ["diff", "--no-ext-diff", "--no-textconv", "--no-color", "--no-renames"].map(String::from).to_vec();
  match spec.head.as_deref() {
    Some(head) => {
      if !valid_ref(head) {
        return Err("invalid head ref".into());
      }
      args.push(format!("{base}...{head}"));
    }
    None => {
      args.push("--merge-base".into());
      args.push(base.to_owned());
    }
  }
  args.push("--".into());
  Ok(args)
}

pub fn run_diff(spec: &DiffSpec) -> DiffView {
  let args = match git_args(spec) {
    Ok(a) => a,
    Err(e) => return failed(e),
  };
  if !std::path::Path::new(&spec.repo).is_dir() {
    return failed("repository folder not found");
  }
  let mut cmd = Command::new("git");
  cmd
    .args(["-c", "core.fsmonitor=false", "-c", "diff.noprefix=false", "-c", "diff.mnemonicPrefix=false"])
    .arg("-C")
    .arg(&spec.repo)
    .args(&args)
    .env("GIT_TERMINAL_PROMPT", "0")
    .env("GIT_OPTIONAL_LOCKS", "0")
    .env("GIT_PAGER", "cat")
    .env_remove("GIT_DIR")
    .env_remove("GIT_WORK_TREE")
    .env_remove("GIT_INDEX_FILE")
    .env_remove("GIT_EXTERNAL_DIFF")
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());
  let mut child = match cmd.spawn() {
    Ok(c) => c,
    Err(e) => return failed(format!("git is not available: {e}")),
  };
  let (mut out, mut err) = (child.stdout.take().unwrap(), child.stderr.take().unwrap());
  let reader = std::thread::spawn(move || {
    let mut buf = Vec::new();
    let _ = (&mut out).take(MAX_BYTES as u64 + 1).read_to_end(&mut buf);
    buf
  });
  let stderr = std::thread::spawn(move || {
    let mut buf = Vec::new();
    let _ = (&mut err).take(4096).read_to_end(&mut buf);
    buf
  });
  let start = Instant::now();
  let mut timed_out = false;
  while !reader.is_finished() {
    if start.elapsed() > TIMEOUT {
      timed_out = true;
      break;
    }
    std::thread::sleep(Duration::from_millis(10));
  }
  let _ = child.kill();
  let status = child.wait().ok();
  let bytes = if timed_out { Vec::new() } else { reader.join().unwrap_or_default() };
  let stderr = String::from_utf8_lossy(&stderr.join().unwrap_or_default()).trim().to_owned();
  if timed_out {
    return failed("git diff timed out. If this repo is on another disk, macOS may be waiting for you to allow access: allow it, then press Refresh.");
  }
  let truncated = bytes.len() > MAX_BYTES;
  if !truncated && bytes.is_empty() && status.is_some_and(|s| !s.success()) && !stderr.is_empty() {
    return failed(stderr.lines().next().unwrap_or("git failed").to_owned());
  }
  let mut text = String::from_utf8_lossy(&bytes[..bytes.len().min(MAX_BYTES)]).into_owned();
  if truncated {
    if let Some(i) = text.rfind('\n') {
      text.truncate(i);
    }
  }
  DiffView { files: parse(&text), truncated, error: None }
}

fn unquote(p: &str) -> &str {
  p.strip_prefix('"').and_then(|p| p.strip_suffix('"')).unwrap_or(p)
}

fn header_path(line: &str) -> String {
  let rest = line.strip_prefix("diff --git ").unwrap_or(line);
  let path = rest.rfind(" b/").map(|i| &rest[i + 3..]).unwrap_or(rest);
  unquote(path).to_owned()
}

struct HunkRange {
  old_start: u32,
  old_len: u32,
  new_start: u32,
  new_len: u32,
}

fn hunk_range(header: &str) -> HunkRange {
  let mut parts = header.split_whitespace().skip(1).take(2).map(|part| {
    let mut nums = part.trim_start_matches(['-', '+']).split(',');
    let start = nums.next().and_then(|n| n.parse::<u32>().ok());
    let len = match nums.next() {
      None => start.map(|_| 1),
      Some(n) => n.parse::<u32>().ok(),
    };
    (start.unwrap_or(1), len.unwrap_or(u32::MAX))
  });
  let (old_start, old_len) = parts.next().unwrap_or((1, u32::MAX));
  let (new_start, new_len) = parts.next().unwrap_or((1, u32::MAX));
  HunkRange { old_start, old_len, new_start, new_len }
}

fn patch_path(line: &str, prefix: &str) -> Option<String> {
  let rest = line.strip_prefix(prefix)?;
  let rest = rest.split('\t').next().unwrap_or(rest).trim();
  let path = unquote(rest);
  let path = path.strip_prefix("a/").or_else(|| path.strip_prefix("b/")).unwrap_or(path);
  (!path.is_empty()).then(|| path.to_owned())
}

fn headerless_path(old: &str, new: &str) -> String {
  patch_path(new, "+++ ")
    .filter(|p| p != "/dev/null")
    .or_else(|| patch_path(old, "--- ").filter(|p| p != "/dev/null"))
    .unwrap_or_else(|| "patch".to_owned())
}

pub fn parse(text: &str) -> Vec<DiffFile> {
  let lines: Vec<&str> = text.lines().collect();
  let mut files: Vec<DiffFile> = Vec::new();
  let (mut old, mut new) = (0u32, 0u32);
  let (mut left_old, mut left_new) = (0u32, 0u32);
  let mut in_hunk = false;
  let mut in_header = false;
  let mut i = 0;
  while i < lines.len() {
    let line = lines[i];
    i += 1;
    if line.starts_with("diff --git ") {
      files.push(DiffFile { path: header_path(line), added: 0, removed: 0, hunks: Vec::new() });
      in_hunk = false;
      in_header = true;
      continue;
    }
    if line.starts_with("@@") {
      let Some(file) = files.last_mut() else { continue };
      let range = hunk_range(line);
      (old, new, left_old, left_new) = (range.old_start, range.new_start, range.old_len, range.new_len);
      file.hunks.push(Hunk { header: line.to_owned(), lines: Vec::new() });
      in_hunk = left_old > 0 || left_new > 0;
      in_header = false;
      continue;
    }
    if !in_hunk {
      if in_header {
        if let Some(p) = line.strip_prefix("+++ b/") {
          files.last_mut().unwrap().path = unquote(p).to_owned();
        } else if let Some(p) = line.strip_prefix("--- a/") {
          files.last_mut().unwrap().path = unquote(p).to_owned();
        }
      } else if line.starts_with("--- ") && lines.get(i).is_some_and(|l| l.starts_with("+++ ")) {
        files.push(DiffFile {
          path: headerless_path(line, lines[i]),
          added: 0,
          removed: 0,
          hunks: Vec::new(),
        });
        i += 1;
      }
      continue;
    }
    let file = files.last_mut().unwrap();
    let hunk = file.hunks.last_mut().unwrap();
    let (kind, rest) = match line.chars().next() {
      Some('+') => ("add", &line[1..]),
      Some('-') => ("del", &line[1..]),
      Some(' ') => ("ctx", &line[1..]),
      Some('\\') => continue,
      None => ("ctx", ""),
      Some(_) => continue,
    };
    let entry = match kind {
      "add" => {
        file.added += 1;
        new += 1;
        left_new = left_new.saturating_sub(1);
        DiffLine { kind, old: None, new: Some(new - 1), text: rest.to_owned() }
      }
      "del" => {
        file.removed += 1;
        old += 1;
        left_old = left_old.saturating_sub(1);
        DiffLine { kind, old: Some(old - 1), new: None, text: rest.to_owned() }
      }
      _ => {
        old += 1;
        new += 1;
        left_old = left_old.saturating_sub(1);
        left_new = left_new.saturating_sub(1);
        DiffLine { kind, old: Some(old - 1), new: Some(new - 1), text: rest.to_owned() }
      }
    };
    hunk.lines.push(entry);
    in_hunk = left_old > 0 || left_new > 0;
  }
  files
}

fn repo_label(spec: &DiffSpec) -> String {
  let trimmed = spec.repo.trim_end_matches('/');
  let name = trimmed.rsplit('/').next().unwrap_or(trimmed);
  if name.is_empty() {
    "repo".to_owned()
  } else {
    name.to_owned()
  }
}

fn unique_label(base: String, used: &mut Vec<String>) -> String {
  let mut label = base.clone();
  let mut n = 2;
  while used.contains(&label) {
    label = format!("{base}-{n}");
    n += 1;
  }
  used.push(label.clone());
  label
}

fn patch_view(text: &str) -> DiffView {
  let files = parse(text);
  if files.is_empty() {
    return failed("No file changes found in the patch");
  }
  DiffView { files, truncated: false, error: None }
}

pub fn groups(repos: &[DiffSpec], patches: &[PatchIn]) -> DiffGroups {
  let mut used = Vec::new();
  let labels: Vec<String> = repos.iter().map(|r| unique_label(repo_label(r), &mut used)).collect();
  let views: Vec<DiffView> = std::thread::scope(|scope| {
    let handles: Vec<_> = repos.iter().map(|spec| scope.spawn(move || run_diff(spec))).collect();
    handles.into_iter().map(|h| h.join().unwrap_or_else(|_| failed("the diff could not be built"))).collect()
  });
  let mut groups: Vec<DiffGroup> = repos
    .iter()
    .zip(labels)
    .zip(views)
    .map(|((spec, label), view)| DiffGroup {
      key: format!("repo:{label}"),
      label,
      kind: "repo".into(),
      base: spec.base.clone(),
      head: spec.head.clone(),
      view,
    })
    .collect();
  for p in patches {
    let base = if p.label.is_empty() { "patch".to_owned() } else { p.label.clone() };
    let label = unique_label(base, &mut used);
    groups.push(DiffGroup {
      key: format!("patch:{label}"),
      label,
      kind: "patch".into(),
      base: None,
      head: None,
      view: patch_view(&p.text),
    });
  }
  DiffGroups { groups }
}

#[cfg(test)]
mod tests {
  use super::*;

  const SAMPLE: &str = "diff --git a/src/a b.rs b/src/a b.rs\nindex 1..2 100644\n--- a/src/a b.rs\n+++ b/src/a b.rs\n@@ -1,3 +1,4 @@ fn x\n ctx\n-gone\n+new1\n+new2\n tail\n\\ No newline at end of file\ndiff --git a/bin.png b/bin.png\nBinary files a/bin.png and b/bin.png differ\ndiff --git a/c.txt b/c.txt\nnew file mode 100644\n--- /dev/null\n+++ b/c.txt\n@@ -0,0 +1 @@\n+--- not a header\n";

  #[test]
  fn ref_validation() {
    for ok in ["HEAD", "main", "feature/x-1", "HEAD~2", "v1.0^", "origin/main", "@"] {
      assert!(valid_ref(ok), "{ok}");
    }
    for bad in ["", "-x", "--output=/tmp/x", "a b", "a;b", "a..b", "$(x)", "a\nb", "a:b", "@{u}", &"a".repeat(201)] {
      assert!(!valid_ref(bad), "{bad}");
    }
  }

  #[test]
  fn git_args_shapes() {
    let spec = |base: Option<&str>, head: Option<&str>| DiffSpec {
      repo: "/r".into(),
      base: base.map(Into::into),
      head: head.map(Into::into),
    };
    let tail = |a: Vec<String>| a[5..].to_vec();
    assert_eq!(tail(git_args(&spec(None, None)).unwrap()), ["--merge-base", "HEAD", "--"]);
    assert_eq!(tail(git_args(&spec(Some("main"), None)).unwrap()), ["--merge-base", "main", "--"]);
    assert_eq!(tail(git_args(&spec(Some("main"), Some("topic"))).unwrap()), ["main...topic", "--"]);
    assert!(git_args(&spec(Some("--evil"), None)).is_err());
    assert!(git_args(&spec(Some("main"), Some("-x"))).is_err());
  }

  #[test]
  fn parser_numbers_lines_and_counts() {
    let files = parse(SAMPLE);
    assert_eq!(files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["src/a b.rs", "bin.png", "c.txt"]);
    let f = &files[0];
    assert_eq!((f.added, f.removed), (2, 1));
    let lines = &f.hunks[0].lines;
    assert_eq!(lines.len(), 5);
    assert_eq!((lines[0].kind, lines[0].old, lines[0].new), ("ctx", Some(1), Some(1)));
    assert_eq!((lines[1].kind, lines[1].old, lines[1].new), ("del", Some(2), None));
    assert_eq!((lines[2].kind, lines[2].old, lines[2].new), ("add", None, Some(2)));
    assert_eq!((lines[3].old, lines[3].new), (None, Some(3)));
    assert_eq!((lines[4].old, lines[4].new), (Some(3), Some(4)));
    assert!(files[1].hunks.is_empty());
    assert_eq!(files[2].hunks[0].lines[0].text, "--- not a header");
    assert_eq!(files[2].added, 1);
  }

  fn git(dir: &std::path::Path, args: &[&str]) {
    let ok = Command::new("git")
      .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
      .arg("-C")
      .arg(dir)
      .args(args)
      .env("GIT_CONFIG_GLOBAL", "/dev/null")
      .env("GIT_CONFIG_NOSYSTEM", "1")
      .status()
      .unwrap()
      .success();
    assert!(ok, "{args:?}");
  }

  #[test]
  fn real_repo_diff_against_head_and_between_refs() {
    let tmp = tempfile::Builder::new().prefix("dg").tempdir_in("/tmp").unwrap();
    let d = tmp.path();
    git(d, &["init", "-q", "-b", "main"]);
    std::fs::write(d.join("a.txt"), "one\ntwo\n").unwrap();
    git(d, &["add", "."]);
    git(d, &["commit", "-qm", "init"]);
    git(d, &["checkout", "-qb", "topic"]);
    std::fs::write(d.join("a.txt"), "one\ntwo\nthree\n").unwrap();
    git(d, &["commit", "-qam", "more"]);
    std::fs::write(d.join("a.txt"), "one\ntwo\nthree\nfour\n").unwrap();
    let repo = d.to_string_lossy().into_owned();

    let work = run_diff(&DiffSpec { repo: repo.clone(), base: None, head: None });
    assert_eq!(work.error, None);
    assert_eq!((work.files.len(), work.files[0].added), (1, 1));
    let between = run_diff(&DiffSpec { repo: repo.clone(), base: Some("main".into()), head: Some("topic".into()) });
    assert_eq!(between.files[0].path, "a.txt");
    assert_eq!(between.files[0].added, 1);
    let merge_base = run_diff(&DiffSpec { repo: repo.clone(), base: Some("main".into()), head: None });
    assert_eq!(merge_base.files[0].added, 2);
    let evil = run_diff(&DiffSpec { repo: repo.clone(), base: Some("--output=/tmp/x".into()), head: None });
    assert!(evil.error.is_some());
    let missing = run_diff(&DiffSpec { repo: "/nonexistent-dir".into(), base: None, head: None });
    assert!(missing.error.is_some());
    let notrepo = tempfile::Builder::new().prefix("dn").tempdir_in("/tmp").unwrap();
    let nr = run_diff(&DiffSpec { repo: notrepo.path().to_string_lossy().into_owned(), base: None, head: None });
    assert!(nr.error.is_some(), "{nr:?}");
  }

  const HEADERLESS: &str = "--- a/one.txt\t2024-01-01 10:00:00\n+++ b/one.txt\t2024-01-01 10:00:01\n@@ -1,2 +1,2 @@\n keep\n-old\n+new\n--- /dev/null\n+++ b/two.txt\n@@ -0,0 +1 @@\n+hello\n--- a/three.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-bye\n";

  #[test]
  fn parses_patches_without_a_git_header() {
    let files = parse(HEADERLESS);
    assert_eq!(files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["one.txt", "two.txt", "three.txt"]);
    assert_eq!(files.iter().map(|f| (f.added, f.removed)).collect::<Vec<_>>(), [(1, 1), (1, 0), (0, 1)]);
    assert_eq!(files[0].hunks[0].lines.len(), 3);
    assert_eq!(files[1].hunks[0].lines[0].new, Some(1));
  }

  #[test]
  fn git_header_files_keep_their_path_from_the_header_block() {
    let files = parse("diff --git a/x.rs b/x.rs\nindex 1..2 100644\n--- a/x.rs\n+++ b/x.rs\n@@ -1 +1 @@\n-a\n+b\n");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "x.rs");
  }

  #[test]
  fn removed_lines_that_look_like_file_headers_stay_in_the_hunk() {
    let body = "@@ -1,3 +1,3 @@\n keep\n--- sql comment\n+++ other comment\n tail\n";
    let headerless = parse(&format!("--- a/q.sql\n+++ b/q.sql\n{body}--- a/next\n+++ b/next\n@@ -1 +1 @@\n-x\n+y\n"));
    assert_eq!(headerless.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["q.sql", "next"]);
    let lines = &headerless[0].hunks[0].lines;
    assert_eq!(lines.len(), 4);
    assert_eq!((lines[1].kind, lines[1].text.as_str()), ("del", "-- sql comment"));
    assert_eq!((lines[2].kind, lines[2].text.as_str()), ("add", "++ other comment"));
    let git = parse(&format!("diff --git a/q.sql b/q.sql\n--- a/q.sql\n+++ b/q.sql\n{body}"));
    assert_eq!((git.len(), git[0].added, git[0].removed), (1, 1, 1));
  }

  #[test]
  fn malformed_patches_do_not_panic() {
    for text in ["", "garbage\nmore", "@@ -1 +1 @@\n+x\n", "--- a\n", "--- a\n+++ b\n@@ nonsense\n+x\n-y\n z", "+++ only\n--- reversed\n"] {
      let _ = parse(text);
    }
    assert!(parse("garbage").is_empty());
    let view = patch_view("not a patch");
    assert!(view.files.is_empty() && view.error.is_some());
  }

  fn commit_repo(dir: &std::path::Path, text: &str) {
    std::fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("f.txt"), "one\n").unwrap();
    git(dir, &["add", "."]);
    git(dir, &["commit", "-qm", "init"]);
    std::fs::write(dir.join("f.txt"), text).unwrap();
  }

  fn spec_of(dir: &std::path::Path) -> DiffSpec {
    DiffSpec { repo: dir.to_string_lossy().into_owned(), base: None, head: None }
  }

  #[test]
  fn two_repos_and_a_patch_make_three_groups() {
    let tmp = tempfile::Builder::new().prefix("dg").tempdir_in("/tmp").unwrap();
    let (a, b) = (tmp.path().join("x/app"), tmp.path().join("y/app"));
    commit_repo(&a, "one\ntwo\n");
    commit_repo(&b, "one\ntwo\nthree\n");
    let patches = [PatchIn { label: String::new(), text: HEADERLESS.into() }];
    let g = groups(&[spec_of(&a), spec_of(&b)], &patches).groups;
    assert_eq!(
      g.iter().map(|g| (g.key.as_str(), g.label.as_str(), g.kind.as_str())).collect::<Vec<_>>(),
      [("repo:app", "app", "repo"), ("repo:app-2", "app-2", "repo"), ("patch:patch", "patch", "patch")]
    );
    assert_eq!(g.iter().map(|g| g.view.files.len()).collect::<Vec<_>>(), [1, 1, 3]);
    assert_eq!((g[0].view.files[0].added, g[1].view.files[0].added), (1, 2));
    assert!(g.iter().all(|g| g.view.error.is_none()));
  }

  #[test]
  fn one_bad_repo_leaves_the_others_rendering() {
    let tmp = tempfile::Builder::new().prefix("dg").tempdir_in("/tmp").unwrap();
    let ok = tmp.path().join("good");
    commit_repo(&ok, "one\ntwo\n");
    let missing = DiffSpec { repo: "/nonexistent-dir/bad".into(), base: None, head: None };
    let evil = DiffSpec { repo: ok.to_string_lossy().into_owned(), base: Some("--output=/tmp/x".into()), head: None };
    let g = groups(&[missing, spec_of(&ok), evil], &[]).groups;
    assert_eq!(g.len(), 3);
    assert!(g[0].view.error.is_some() && g[2].view.error.is_some());
    assert_eq!((g[1].view.error.clone(), g[1].view.files.len()), (None, 1));
    assert_eq!((g[1].label.as_str(), g[2].label.as_str()), ("good", "good-2"));
  }

  #[test]
  fn patch_labels_dedupe_against_repos_and_each_other() {
    let p = |l: &str| PatchIn { label: l.into(), text: HEADERLESS.into() };
    let repo = DiffSpec { repo: "/nonexistent-dir/shared/".into(), base: None, head: None };
    let g = groups(&[repo], &[p("shared"), p("shared"), p(""), p("")]).groups;
    assert_eq!(
      g.iter().map(|g| g.label.as_str()).collect::<Vec<_>>(),
      ["shared", "shared-2", "shared-3", "patch", "patch-2"]
    );
    assert_eq!(g[1].key, "patch:shared-2");
    assert!(groups(&[], &[]).groups.is_empty());
  }
}
