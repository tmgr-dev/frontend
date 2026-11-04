use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;
use tmgr_ipc::decisions::DiffSpec;

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

fn hunk_starts(header: &str) -> (u32, u32) {
  let mut nums =
    header.split_whitespace().skip(1).take(2).map(|part| {
      part.trim_start_matches(['-', '+']).split(',').next().and_then(|n| n.parse::<u32>().ok()).unwrap_or(1)
    });
  (nums.next().unwrap_or(1), nums.next().unwrap_or(1))
}

pub fn parse(text: &str) -> Vec<DiffFile> {
  let mut files: Vec<DiffFile> = Vec::new();
  let (mut old, mut new) = (0u32, 0u32);
  let mut in_hunk = false;
  for line in text.lines() {
    if line.starts_with("diff --git ") {
      files.push(DiffFile { path: header_path(line), added: 0, removed: 0, hunks: Vec::new() });
      in_hunk = false;
      continue;
    }
    let Some(file) = files.last_mut() else { continue };
    if line.starts_with("@@") {
      (old, new) = hunk_starts(line);
      file.hunks.push(Hunk { header: line.to_owned(), lines: Vec::new() });
      in_hunk = true;
      continue;
    }
    if !in_hunk {
      if let Some(p) = line.strip_prefix("+++ b/") {
        file.path = unquote(p).to_owned();
      } else if let Some(p) = line.strip_prefix("--- a/") {
        file.path = unquote(p).to_owned();
      }
      continue;
    }
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
        DiffLine { kind, old: None, new: Some(new - 1), text: rest.to_owned() }
      }
      "del" => {
        file.removed += 1;
        old += 1;
        DiffLine { kind, old: Some(old - 1), new: None, text: rest.to_owned() }
      }
      _ => {
        old += 1;
        new += 1;
        DiffLine { kind, old: Some(old - 1), new: Some(new - 1), text: rest.to_owned() }
      }
    };
    hunk.lines.push(entry);
  }
  files
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
}
