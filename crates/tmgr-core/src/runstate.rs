use crate::active::state_dir;
use crate::config::{ensure_private_dir, write_atomic};
use crate::error::{Error, Result};
use crate::text::{sanitize_line, truncate_chars};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const RUNS_SCHEMA: &str = "tmgr.runs.v1";
pub const MAX_COMMITS: usize = 200;
const MAX_FILE: u64 = 256 * 1024;
const MAX_RUNS: usize = 50;
const MAX_COMMIT_LINE: usize = 300;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Run {
    pub run_id: u64,
    pub task_id: u64,
    pub task_key: String,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub repo: Option<String>,
    #[serde(default)]
    pub start_sha: Option<String>,
    pub agent: String,
    pub started_at: String,
    #[serde(default)]
    pub commits: Vec<String>,
    #[serde(default)]
    pub last_sent: i64,
    /// `local:<code>` for a run in a local workspace; none for the cloud.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Runs {
    #[serde(default)]
    pub schema: String,
    #[serde(default)]
    pub runs: Vec<Run>,
}

impl Runs {
    /// The run of a branch: an exact repository match first, then a run whose repository is not
    /// recorded; else the only run of the task key (none when several runs carry it).
    pub fn find_branch(&self, repo: Option<&str>, branch: &str, key: Option<&str>) -> Option<&Run> {
        let on_branch = |r: &&Run| r.branch.as_deref() == Some(branch);
        let exact = repo.and_then(|repo| {
            self.runs
                .iter()
                .rev()
                .filter(on_branch)
                .find(|r| r.repo.as_deref() == Some(repo))
        });
        let same_repo = |r: &&Run| repo.is_none() || r.repo.is_none() || r.repo.as_deref() == repo;
        if let Some(run) =
            exact.or_else(|| self.runs.iter().rev().filter(on_branch).find(same_repo))
        {
            return Some(run);
        }
        let key = key?;
        let mut by_key = self
            .runs
            .iter()
            .filter(|r| r.task_key == key)
            .filter(same_repo);
        let only = by_key.next()?;
        by_key.next().is_none().then_some(only)
    }

    /// A run by id in exactly `workspace` (`local:<code>`, or none for the cloud).
    pub fn find_run(&self, run_id: u64, workspace: Option<&str>) -> Option<&Run> {
        self.runs
            .iter()
            .find(|r| r.run_id == run_id && r.workspace.as_deref() == workspace)
    }

    pub fn upsert(&mut self, run: Run) {
        self.runs.retain(|r| {
            r.workspace != run.workspace || (r.run_id != run.run_id && r.task_id != run.task_id)
        });
        self.runs.push(run);
        if self.runs.len() > MAX_RUNS {
            let drop = self.runs.len() - MAX_RUNS;
            self.runs.drain(..drop);
        }
    }

    pub fn remove(&mut self, run_id: u64, workspace: Option<&str>) {
        self.runs
            .retain(|r| r.run_id != run_id || r.workspace.as_deref() != workspace);
    }
}

pub fn runs_path() -> Option<PathBuf> {
    state_dir().map(|d| d.join("runs.json"))
}

pub fn load_from(path: &Path) -> Runs {
    let Ok(file) = fs::File::open(path) else {
        return Runs::default();
    };
    let mut buf = Vec::new();
    if file.take(MAX_FILE + 1).read_to_end(&mut buf).is_err() || buf.len() as u64 > MAX_FILE {
        return Runs::default();
    }
    parse(&buf)
}

pub fn parse(buf: &[u8]) -> Runs {
    if buf.len() as u64 > MAX_FILE {
        return Runs::default();
    }
    let Ok(mut runs) = serde_json::from_slice::<Runs>(buf) else {
        return Runs::default();
    };
    if runs.schema != RUNS_SCHEMA {
        return Runs::default();
    }
    runs.runs.retain(|r| r.run_id > 0 && r.task_id > 0);
    for r in &mut runs.runs {
        r.commits = r.commits.iter().filter_map(|c| clean_commit(c)).collect();
        if r.commits.len() > MAX_COMMITS {
            r.commits.drain(..r.commits.len() - MAX_COMMITS);
        }
        r.start_sha = r.start_sha.take().filter(|s| valid_sha(s));
    }
    runs
}

pub fn load() -> Runs {
    runs_path().map(|p| load_from(&p)).unwrap_or_default()
}

pub fn save_to(path: &Path, runs: &Runs) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| Error::internal("invalid state path"))?;
    ensure_private_dir(dir)?;
    let mut out = runs.clone();
    out.schema = RUNS_SCHEMA.to_string();
    let bytes = serde_json::to_vec_pretty(&out)
        .map_err(|_| Error::internal("cannot serialize the run state"))?;
    write_atomic(path, &bytes, 0o600)
}

pub fn modify<T>(path: &Path, wait: Duration, f: impl FnOnce(&mut Runs) -> T) -> Result<T> {
    use std::os::unix::fs::OpenOptionsExt;
    let dir = path
        .parent()
        .ok_or_else(|| Error::internal("invalid state path"))?;
    ensure_private_dir(dir)?;
    let lock = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(dir.join("runs.lock"))?;
    let deadline = Instant::now() + wait;
    loop {
        match lock.try_lock() {
            Ok(()) => break,
            Err(fs::TryLockError::WouldBlock) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return Err(Error::internal("the run state is busy")),
        }
    }
    let mut runs = load_from(path);
    let out = f(&mut runs);
    save_to(path, &runs)?;
    Ok(out)
}

pub fn valid_sha(s: &str) -> bool {
    (7..=64).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

pub fn clean_commit(raw: &str) -> Option<String> {
    let line = truncate_chars(sanitize_line(raw).trim(), MAX_COMMIT_LINE).0;
    let (sha, _) = split_commit(&line);
    valid_sha(sha).then_some(line)
}

pub fn split_commit(line: &str) -> (&str, &str) {
    match line.split_once(' ') {
        Some((sha, msg)) => (sha, msg.trim()),
        None => (line, ""),
    }
}

pub fn merge_commits(existing: &[String], new: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in existing.iter().chain(new) {
        let sha = split_commit(line).0;
        match out.iter().position(|c| split_commit(c).0 == sha) {
            Some(i) => out[i] = line.clone(),
            None => out.push(line.clone()),
        }
    }
    if out.len() > MAX_COMMITS {
        out.drain(..out.len() - MAX_COMMITS);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(id: u64, task: u64, branch: &str) -> Run {
        Run {
            run_id: id,
            task_id: task,
            task_key: format!("TM-{task}"),
            branch: Some(branch.to_string()),
            repo: Some("/work/api".into()),
            start_sha: Some("abcdef1".into()),
            agent: "claude-code".into(),
            started_at: "2026-10-12T10:00:00Z".into(),
            commits: vec![],
            last_sent: 0,
            workspace: None,
        }
    }

    #[test]
    fn local_and_cloud_runs_with_the_same_id_are_kept_apart() {
        let mut runs = Runs::default();
        runs.upsert(run(1, 10, "a"));
        let mut local = run(1, 10, "b");
        local.workspace = Some("local:personal".into());
        runs.upsert(local);
        assert_eq!(runs.runs.len(), 2);
        assert_eq!(
            runs.find_run(1, Some("local:personal"))
                .unwrap()
                .branch
                .as_deref(),
            Some("b")
        );
        assert_eq!(runs.find_run(1, None).unwrap().branch.as_deref(), Some("a"));
        assert!(runs.find_run(1, Some("local:other")).is_none());
        runs.remove(1, Some("local:personal"));
        assert_eq!(runs.runs.len(), 1);
    }

    #[test]
    fn commits_merge_by_sha_and_keep_order() {
        let a = vec!["aaaaaaa one".to_string(), "bbbbbbb two".to_string()];
        let b = vec!["bbbbbbb two".to_string(), "ccccccc three".to_string()];
        assert_eq!(
            merge_commits(&a, &b),
            vec!["aaaaaaa one", "bbbbbbb two", "ccccccc three"]
        );
    }

    #[test]
    fn merged_commits_keep_the_newest_two_hundred() {
        let many: Vec<String> = (0..250).map(|i| format!("{i:07x} c{i}")).collect();
        let merged = merge_commits(&[], &many);
        assert_eq!(merged.len(), MAX_COMMITS);
        assert!(merged[0].starts_with("0000032"));
    }

    #[test]
    fn commit_lines_are_validated_and_bounded() {
        assert_eq!(
            clean_commit("abc1234 fix: it"),
            Some("abc1234 fix: it".into())
        );
        assert_eq!(clean_commit("zzzzzzz nope"), None);
        assert_eq!(clean_commit("abc"), None);
        let long = clean_commit(&format!("abc1234 {}", "x".repeat(1000))).unwrap();
        assert!(long.chars().count() <= MAX_COMMIT_LINE);
        assert!(!clean_commit("abc1234 a\u{1b}[31mb")
            .unwrap()
            .contains('\u{1b}'));
    }

    #[test]
    fn runs_round_trip_through_a_private_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tmgr").join("runs.json");
        modify(&path, Duration::from_secs(1), |r| {
            r.upsert(run(1, 10, "tm-10-x"));
            r.upsert(run(2, 10, "tm-10-x"));
            r.upsert(run(3, 11, "tm-11-y"));
        })
        .unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let runs = load_from(&path);
        assert_eq!(runs.runs.len(), 2);
        assert!(runs.find_run(1, None).is_none());
        assert_eq!(
            runs.find_branch(Some("/work/api"), "tm-11-y", None)
                .unwrap()
                .run_id,
            3
        );
        assert_eq!(
            runs.find_branch(None, "other", Some("TM-10"))
                .unwrap()
                .run_id,
            2
        );
        assert!(runs.find_branch(None, "main", None).is_none());
    }

    #[test]
    fn garbage_or_foreign_files_load_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("runs.json");
        fs::write(&p, b"{not json").unwrap();
        assert!(load_from(&p).runs.is_empty());
        fs::write(&p, br#"{"schema":"other","runs":[]}"#).unwrap();
        assert!(load_from(&p).runs.is_empty());
        fs::write(&p, vec![b' '; 300_000]).unwrap();
        assert!(load_from(&p).runs.is_empty());
        assert!(load_from(&dir.path().join("missing.json")).runs.is_empty());
    }

    #[test]
    fn a_key_fallback_never_guesses_between_runs() {
        let mut runs = Runs::default();
        runs.runs.push(run(1, 10, "tm-10-a"));
        let mut local = run(2, 10, "tm-10-b");
        local.workspace = Some("local:personal".into());
        runs.runs.push(local);
        assert!(runs
            .find_branch(Some("/work/api"), "main", Some("TM-10"))
            .is_none());
        assert_eq!(
            runs.find_branch(Some("/work/api"), "tm-10-b", Some("TM-10"))
                .unwrap()
                .run_id,
            2
        );
        let mut exact = run(3, 11, "shared");
        exact.repo = Some("/work/exact".into());
        runs.runs.insert(0, exact);
        let mut loose = run(4, 12, "shared");
        loose.repo = None;
        runs.runs.push(loose);
        assert_eq!(
            runs.find_branch(Some("/work/exact"), "shared", None)
                .unwrap()
                .run_id,
            3
        );
    }
}
