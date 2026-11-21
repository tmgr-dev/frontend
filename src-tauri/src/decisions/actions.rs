use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use tmgr_ipc::actions::{ActionIn, ActionResult, PrPayload, TagPayload};

pub const STATE_DONE: &str = "done";
pub const STATE_FAILED: &str = "failed";
pub const STATE_PARTIAL: &str = "partial";
pub const STATE_UNKNOWN: &str = "unknown";

const GH_PATH: &str = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin";

pub struct Outcome {
  pub state: &'static str,
  pub result: ActionResult,
}

impl Outcome {
  pub fn failed(error: impl Into<String>) -> Outcome {
    Outcome { state: STATE_FAILED, result: ActionResult { ok: false, error: Some(error.into()), ..Default::default() } }
  }

  fn done(url: String, sha: &str) -> Outcome {
    Outcome {
      state: STATE_DONE,
      result: ActionResult { ok: true, url: Some(url), sha: Some(sha.to_owned()), error: None },
    }
  }
}

#[derive(Debug, Clone)]
pub struct Gh {
  pub bin: PathBuf,
  pub home: OsString,
  pub path: String,
  pub cwd: PathBuf,
}

impl Gh {
  pub fn resolve(cwd: &Path) -> Result<Gh, String> {
    let home = std::env::var_os("HOME").filter(|h| !h.is_empty()).ok_or_else(|| "HOME is not set".to_owned())?;
    let bin = GH_PATH
      .split(':')
      .map(|d| Path::new(d).join("gh"))
      .find(|c| c.is_file())
      .ok_or_else(|| "the GitHub CLI (gh) was not found; install it and run gh auth login".to_owned())?;
    Ok(Gh { bin, home, path: GH_PATH.to_owned(), cwd: cwd.to_owned() })
  }

  fn api(&self, args: &[String]) -> Result<String, String> {
    let out = Command::new(&self.bin)
      .env_clear()
      .env("HOME", &self.home)
      .env("PATH", &self.path)
      .current_dir(&self.cwd)
      .args(["api", "--hostname", "github.com"])
      .args(args)
      .stdin(Stdio::null())
      .output()
      .map_err(|e| format!("gh could not start: {e}"))?;
    if out.status.success() {
      return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let err = String::from_utf8_lossy(&out.stderr);
    let msg: String = err.trim().chars().take(400).collect();
    Err(if msg.is_empty() { format!("gh failed ({})", out.status) } else { msg })
  }
}

fn strs(items: &[&str]) -> Vec<String> {
  items.iter().map(|s| (*s).to_owned()).collect()
}

fn field(flag: &str, key: &str, value: &str) -> [String; 2] {
  [flag.to_owned(), format!("{key}={value}")]
}

pub fn execute(gh: &Gh, action: &ActionIn) -> Outcome {
  match action {
    ActionIn::Tag(p) => tag(gh, p),
    ActionIn::Pr(p) => pr(gh, p),
    ActionIn::Comment(_) | ActionIn::Task(_) => Outcome::failed("this action runs in the main window"),
  }
}

fn tag(gh: &Gh, p: &TagPayload) -> Outcome {
  let mut args = strs(&["-X", "POST"]);
  args.push(format!("repos/{}/git/refs", p.repo));
  args.extend(field("-f", "ref", &format!("refs/tags/{}", p.name)));
  args.extend(field("-f", "sha", &p.target_sha));
  match gh.api(&args) {
    Ok(_) => Outcome::done(format!("https://github.com/{}/releases/tag/{}", p.repo, p.name), &p.target_sha),
    Err(e) => Outcome::failed(e),
  }
}

fn pr(gh: &Gh, p: &PrPayload) -> Outcome {
  let head: serde_json::Value = match gh.api(&[format!("repos/{}/git/ref/heads/{}", p.repo, p.branch)]) {
    Ok(t) => match serde_json::from_str(&t) {
      Ok(v) => v,
      Err(_) => return Outcome::failed("GitHub answered with something unreadable when reading the branch"),
    },
    Err(e) => return Outcome::failed(format!("could not read the branch: {e}")),
  };
  if head["object"]["sha"].as_str() != Some(p.head_sha.as_str()) {
    return Outcome::failed("branch moved since approval");
  }
  let mut args = strs(&["-X", "POST"]);
  args.push(format!("repos/{}/pulls", p.repo));
  args.extend(field("-f", "head", &p.branch));
  args.extend(field("-f", "base", &p.base));
  args.extend(field("-f", "title", &p.title));
  args.extend(field("-f", "body", &p.body));
  let created: serde_json::Value = match gh.api(&args) {
    Ok(t) => match serde_json::from_str(&t) {
      Ok(v) => v,
      Err(_) => return Outcome::failed("GitHub answered with something unreadable; check whether the PR exists"),
    },
    Err(e) => return Outcome::failed(e),
  };
  let Some(url) = created["html_url"].as_str().map(str::to_owned) else {
    return Outcome::failed("GitHub did not return the PR address; check whether the PR exists");
  };
  if created["head"]["sha"].as_str() != Some(p.head_sha.as_str()) {
    return Outcome {
      state: STATE_PARTIAL,
      result: ActionResult {
        ok: false,
        url: Some(url),
        sha: None,
        error: Some("head moved during creation; review the PR".into()),
      },
    };
  }
  Outcome::done(url, &p.head_sha)
}

pub fn verify_pr_commits(p: &PrPayload, repo: &Path) -> Result<(), String> {
  if !repo.is_dir() {
    return Err("the repository folder for this PR was not found".into());
  }
  let out = Command::new("git")
    .args(["-c", "core.fsmonitor=false", "-c", "log.showSignature=false"])
    .arg("-C")
    .arg(repo)
    .args(["log", "--no-color", "--format=%H%x09%s"])
    .arg(format!("{}..{}", p.base_sha, p.head_sha))
    .env("GIT_TERMINAL_PROMPT", "0")
    .env("GIT_OPTIONAL_LOCKS", "0")
    .env("GIT_PAGER", "cat")
    .env_remove("GIT_DIR")
    .env_remove("GIT_WORK_TREE")
    .env_remove("GIT_INDEX_FILE")
    .stdin(Stdio::null())
    .output()
    .map_err(|e| format!("git is not available: {e}"))?;
  if !out.status.success() {
    return Err("could not read the commit list from the local repository".into());
  }
  let text = String::from_utf8_lossy(&out.stdout);
  let actual: Vec<(&str, &str)> = text.lines().filter_map(|l| l.split_once('\t').or(Some((l, "")))).collect();
  let same = actual.len() == p.commits.len()
    && actual.iter().zip(&p.commits).all(|((sha, subject), c)| *sha == c.sha && *subject == c.subject);
  if same {
    Ok(())
  } else {
    Err("the commit list does not match the local repository".into())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::fs;
  use tmgr_ipc::actions::Commit;

  const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
  const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

  struct Fake {
    dir: tempfile::TempDir,
    gh: Gh,
  }

  impl Fake {
    fn new() -> Fake {
      let dir = tempfile::tempdir().unwrap();
      let script = "#!/bin/sh\nd=\"$(dirname \"$0\")\"\n{ printf '%s\\n' \"$@\"; echo ---; } >> \"$d/argv.log\"\nenv > \"$d/env.log\"\npwd > \"$d/cwd.log\"\ncase \"$*\" in\n  *git/ref/heads*) cat \"$d/get.json\" ;;\n  *git/refs*) if [ -f \"$d/tag.fail\" ]; then echo 'Reference already exists' >&2; exit 1; fi; echo '{}' ;;\n  *pulls*) cat \"$d/pr.json\" ;;\nesac\n";
      let bin = dir.path().join("gh");
      fs::write(&bin, script).unwrap();
      fs::set_permissions(&bin, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
      let cwd = dir.path().join("cwd");
      fs::create_dir(&cwd).unwrap();
      let gh = Gh { bin, home: "/nonexistent-home".into(), path: GH_PATH.into(), cwd };
      Fake { dir, gh }
    }

    fn put(&self, name: &str, text: &str) {
      fs::write(self.dir.path().join(name), text).unwrap();
    }

    fn read(&self, name: &str) -> String {
      fs::read_to_string(self.dir.path().join(name)).unwrap_or_default()
    }
  }

  fn tag_payload() -> TagPayload {
    TagPayload { repo: "tmgr-dev/frontend".into(), name: "cli-v1.0.140".into(), target_sha: A.into() }
  }

  fn pr_payload() -> PrPayload {
    PrPayload {
      repo: "tmgr-dev/frontend".into(),
      branch: "tm-1012".into(),
      base: "master".into(),
      base_sha: A.into(),
      head_sha: B.into(),
      title: "feat: @x".into(),
      body: "body 42".into(),
      commits: vec![],
    }
  }

  #[test]
  fn tag_posts_the_ref_with_exact_args_and_a_cleared_env() {
    std::env::set_var("GH_TOKEN", "leak-token");
    std::env::set_var("TMGR_TEST_LEAK", "1");
    let f = Fake::new();
    let out = execute(&f.gh, &ActionIn::Tag(tag_payload()));
    assert_eq!(out.state, STATE_DONE);
    assert!(out.result.ok);
    assert_eq!(out.result.url.as_deref(), Some("https://github.com/tmgr-dev/frontend/releases/tag/cli-v1.0.140"));
    assert_eq!(out.result.sha.as_deref(), Some(A));
    let argv: Vec<String> = f.read("argv.log").lines().map(str::to_owned).collect();
    assert_eq!(
      argv,
      [
        "api",
        "--hostname",
        "github.com",
        "-X",
        "POST",
        "repos/tmgr-dev/frontend/git/refs",
        "-f",
        "ref=refs/tags/cli-v1.0.140",
        "-f",
        &format!("sha={A}"),
        "---"
      ]
    );
    let env = f.read("env.log");
    assert!(!env.contains("GH_TOKEN") && !env.contains("TMGR_TEST_LEAK"), "{env}");
    assert!(env.contains("HOME=/nonexistent-home"));
    assert!(env.contains(&format!("PATH={GH_PATH}")));
    let cwd = f.read("cwd.log");
    assert!(cwd.trim().ends_with("/cwd"), "{cwd}");
  }

  #[test]
  fn an_existing_tag_fails_once_without_retry() {
    let f = Fake::new();
    f.put("tag.fail", "");
    let out = execute(&f.gh, &ActionIn::Tag(tag_payload()));
    assert_eq!(out.state, STATE_FAILED);
    assert!(!out.result.ok);
    assert!(out.result.error.unwrap().contains("already exists"));
    assert_eq!(f.read("argv.log").matches("---").count(), 1);
  }

  #[test]
  fn pr_checks_the_branch_then_posts_with_literal_fields() {
    let f = Fake::new();
    f.put("get.json", &format!(r#"{{"object":{{"sha":"{B}"}}}}"#));
    f.put(
      "pr.json",
      &format!(r#"{{"html_url":"https://github.com/tmgr-dev/frontend/pull/9","head":{{"sha":"{B}"}}}}"#),
    );
    let out = execute(&f.gh, &ActionIn::Pr(pr_payload()));
    assert_eq!(out.state, STATE_DONE);
    assert_eq!(out.result.url.as_deref(), Some("https://github.com/tmgr-dev/frontend/pull/9"));
    assert_eq!(out.result.sha.as_deref(), Some(B));
    let calls: Vec<Vec<String>> = f
      .read("argv.log")
      .split("---\n")
      .filter(|c| !c.is_empty())
      .map(|c| c.lines().map(str::to_owned).collect())
      .collect();
    assert_eq!(calls[0], ["api", "--hostname", "github.com", "repos/tmgr-dev/frontend/git/ref/heads/tm-1012"]);
    assert_eq!(
      calls[1],
      [
        "api",
        "--hostname",
        "github.com",
        "-X",
        "POST",
        "repos/tmgr-dev/frontend/pulls",
        "-f",
        "head=tm-1012",
        "-f",
        "base=master",
        "-f",
        "title=feat: @x",
        "-f",
        "body=body 42"
      ]
    );
  }

  #[test]
  fn a_moved_branch_stops_before_creating_the_pr() {
    let f = Fake::new();
    f.put("get.json", &format!(r#"{{"object":{{"sha":"{A}"}}}}"#));
    let out = execute(&f.gh, &ActionIn::Pr(pr_payload()));
    assert_eq!(out.state, STATE_FAILED);
    assert_eq!(out.result.error.as_deref(), Some("branch moved since approval"));
    assert_eq!(f.read("argv.log").matches("---").count(), 1);
  }

  #[test]
  fn a_head_that_moved_during_creation_is_partial_with_the_url() {
    let f = Fake::new();
    f.put("get.json", &format!(r#"{{"object":{{"sha":"{B}"}}}}"#));
    f.put(
      "pr.json",
      &format!(r#"{{"html_url":"https://github.com/tmgr-dev/frontend/pull/9","head":{{"sha":"{A}"}}}}"#),
    );
    let out = execute(&f.gh, &ActionIn::Pr(pr_payload()));
    assert_eq!(out.state, STATE_PARTIAL);
    assert!(!out.result.ok);
    assert_eq!(out.result.url.as_deref(), Some("https://github.com/tmgr-dev/frontend/pull/9"));
    assert_eq!(out.result.error.as_deref(), Some("head moved during creation; review the PR"));
  }

  fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
      .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
      .arg("-C")
      .arg(dir)
      .args(args)
      .env("GIT_CONFIG_GLOBAL", "/dev/null")
      .env("GIT_CONFIG_NOSYSTEM", "1")
      .output()
      .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
  }

  #[test]
  fn commit_list_is_recomputed_from_the_local_repo() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "master"]);
    git(dir.path(), &["commit", "-q", "--allow-empty", "-m", "base"]);
    let base = git(dir.path(), &["rev-parse", "HEAD"]);
    git(dir.path(), &["commit", "-q", "--allow-empty", "-m", "one"]);
    let one = git(dir.path(), &["rev-parse", "HEAD"]);
    git(dir.path(), &["commit", "-q", "--allow-empty", "-m", "two"]);
    let two = git(dir.path(), &["rev-parse", "HEAD"]);
    let mut p = pr_payload();
    p.base_sha = base;
    p.head_sha = two.clone();
    p.commits =
      vec![Commit { sha: two.clone(), subject: "two".into() }, Commit { sha: one.clone(), subject: "one".into() }];
    assert_eq!(verify_pr_commits(&p, dir.path()), Ok(()));
    let mut lie = p.clone();
    lie.commits[0].subject = "harmless".into();
    assert!(verify_pr_commits(&lie, dir.path()).is_err());
    let mut short = p.clone();
    short.commits.pop();
    assert!(verify_pr_commits(&short, dir.path()).is_err());
    let mut unknown = p.clone();
    unknown.head_sha = B.into();
    assert!(verify_pr_commits(&unknown, dir.path()).is_err());
    assert!(verify_pr_commits(&p, &dir.path().join("missing")).is_err());
  }
}
