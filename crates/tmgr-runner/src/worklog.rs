use crate::sanitize::sanitize_text;
use crate::wire::now_ms;
use crate::worktree::GitEnv;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use tmgr_ipc::api::{SessionInfo, State, StateKind, TaskRef, WorkCommit, WorkExit, WorkSummary};

const FILE: &str = "worklog.jsonl";
const MAX_LINES: usize = 10_000;
const MAX_BYTES: usize = 1 << 20;
const LIFECYCLE_RESERVE: usize = 384 << 10;
const MAX_COMMITS: usize = 200;
const MAX_SUBJECT: usize = 200;
const MAX_RULE: usize = 64;

#[derive(Serialize, Deserialize)]
#[serde(tag = "ev", rename_all = "snake_case")]
enum Ev {
    Created {
        ts: u64,
        profile: Option<String>,
        argv0_basename: String,
        task: Option<TaskRef>,
        bundle_sha256: Option<String>,
        cwd_branch: Option<String>,
        cwd_head: Option<String>,
    },
    State {
        ts: u64,
        kind: StateKind,
        detail: Option<String>,
    },
    Exited {
        ts: u64,
        code: Option<i32>,
        signal: Option<i32>,
    },
    Commits {
        ts: u64,
        branch: Option<String>,
        list: Vec<WorkCommit>,
    },
    Truncated {
        ts: u64,
    },
}

#[derive(Default)]
struct St {
    lines: usize,
    bytes: usize,
    kind: Option<StateKind>,
    created: bool,
    exited: bool,
    truncated: bool,
    broken_tail: bool,
    head: Option<String>,
    branch: Option<String>,
}

pub struct Log {
    path: PathBuf,
    cwd: PathBuf,
    st: Mutex<St>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn read_all(path: &Path) -> (Vec<Ev>, usize, usize, bool) {
    let mut buf = Vec::new();
    let opened = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .and_then(|f| f.take(MAX_BYTES as u64 + 1).read_to_end(&mut buf));
    if opened.is_err() {
        return (Vec::new(), 0, 0, false);
    }
    let broken = buf.last().is_some_and(|b| *b != b'\n');
    let lines: Vec<&[u8]> = buf.split(|b| *b == b'\n').collect();
    let used = lines.len() - 1 + usize::from(broken);
    let events = lines
        .iter()
        .filter_map(|l| serde_json::from_slice::<Ev>(l).ok())
        .collect();
    (events, used, buf.len(), broken)
}

fn is_sha(s: &str) -> bool {
    matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

fn clip(s: &str, max: usize) -> String {
    sanitize_text(s).chars().take(max).collect()
}

fn git_facts(cwd: &Path) -> Option<(GitEnv, Option<String>, Option<String>)> {
    let git = GitEnv::open_facts(cwd).ok()?;
    if git.run(&["rev-parse", "--is-inside-work-tree"]).ok()? != "true" {
        return None;
    }
    let branch = git
        .run(&["symbolic-ref", "--short", "-q", "HEAD"])
        .ok()
        .filter(|b| !b.is_empty())
        .map(|b| clip(&b, MAX_SUBJECT));
    let head = git
        .run(&["rev-parse", "--verify", "-q", "HEAD^{commit}"])
        .ok()
        .filter(|h| is_sha(h));
    Some((git, branch, head))
}

impl Log {
    pub fn open(dir: &Path, cwd: PathBuf) -> Log {
        let path = dir.join(FILE);
        let (events, lines, bytes, broken_tail) = read_all(&path);
        let mut st = St {
            lines,
            bytes,
            broken_tail,
            ..St::default()
        };
        for ev in events {
            match ev {
                Ev::Created {
                    cwd_head,
                    cwd_branch,
                    ..
                } => {
                    st.created = true;
                    st.head = cwd_head;
                    st.branch = cwd_branch;
                }
                Ev::State { kind, .. } => st.kind = Some(kind),
                Ev::Exited { .. } => st.exited = true,
                Ev::Truncated { .. } => st.truncated = true,
                Ev::Commits { .. } => {}
            }
        }
        Log {
            path,
            cwd,
            st: Mutex::new(st),
        }
    }

    fn append(&self, ev: &Ev, lifecycle: bool) {
        let Ok(line) = serde_json::to_vec(ev) else {
            return;
        };
        let mut st = lock(&self.st);
        let lines_left = MAX_LINES.saturating_sub(st.lines);
        let bytes_left = MAX_BYTES.saturating_sub(st.bytes + usize::from(st.broken_tail));
        let reserve = if lifecycle { 0 } else { LIFECYCLE_RESERVE };
        let tail = if lifecycle { 1 } else { 8 };
        if lines_left <= tail || bytes_left < line.len() + 1 + reserve {
            if !st.truncated && lines_left > 0 {
                st.truncated = true;
                let marker =
                    serde_json::to_vec(&Ev::Truncated { ts: now_ms() }).unwrap_or_default();
                self.write_line(&mut st, marker);
            }
            return;
        }
        self.write_line(&mut st, line);
    }

    fn write_line(&self, st: &mut St, mut line: Vec<u8>) {
        line.push(b'\n');
        if st.broken_tail {
            line.insert(0, b'\n');
        }
        let file = OpenOptions::new()
            .append(true)
            .create(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&self.path);
        if file.and_then(|mut f| f.write_all(&line)).is_ok() {
            st.lines += 1 + usize::from(st.broken_tail);
            st.bytes += line.len();
            st.broken_tail = false;
        }
    }

    pub fn created(&self, info: &SessionInfo) {
        {
            let mut st = lock(&self.st);
            if st.created {
                return;
            }
            st.created = true;
        }
        let (branch, head) = git_facts(&self.cwd).map_or((None, None), |(_, b, h)| (b, h));
        {
            let mut st = lock(&self.st);
            st.head = head.clone();
            st.branch = branch.clone();
        }
        let argv0 = info.argv.first().map_or("", String::as_str);
        let base = Path::new(argv0)
            .file_name()
            .map_or_else(String::new, |n| clip(&n.to_string_lossy(), MAX_RULE));
        self.append(
            &Ev::Created {
                ts: info.created_ms,
                profile: info.profile.clone(),
                argv0_basename: base,
                task: info.task,
                bundle_sha256: info.bundle_sha256.clone(),
                cwd_branch: branch,
                cwd_head: head,
            },
            true,
        );
    }

    pub fn record(&self, state: &State) {
        if state.kind == StateKind::Exited {
            self.exited(state.since_ms, state.exit_code, None);
            return;
        }
        {
            let mut st = lock(&self.st);
            if st.exited || st.kind.unwrap_or(StateKind::Starting) == state.kind {
                return;
            }
            st.kind = Some(state.kind);
        }
        let detail = match (state.kind, state.detail.as_deref()) {
            (StateKind::Waiting, Some("bell")) => Some("bell".to_string()),
            (StateKind::Blocked, Some(d)) => {
                Some(clip(d.split(':').next().unwrap_or_default(), MAX_RULE))
            }
            _ => None,
        };
        self.append(
            &Ev::State {
                ts: state.since_ms,
                kind: state.kind,
                detail,
            },
            false,
        );
    }

    pub fn exited(&self, ts: u64, code: Option<i32>, signal: Option<i32>) {
        let (head, branch) = {
            let mut st = lock(&self.st);
            if st.exited {
                return;
            }
            st.exited = true;
            (st.head.clone(), st.branch.clone())
        };
        if let Some((git, now_branch, _)) = git_facts(&self.cwd) {
            let range = match (&head, &branch) {
                (Some(h), _) => Some(format!("{h}..HEAD")),
                (None, Some(_)) => Some("HEAD".to_string()),
                _ => None,
            };
            let list = range.map_or_else(Vec::new, |r| commits(&git, &r));
            self.append(
                &Ev::Commits {
                    ts,
                    branch: now_branch,
                    list,
                },
                true,
            );
        }
        self.append(&Ev::Exited { ts, code, signal }, true);
    }
}

fn commits(git: &GitEnv, range: &str) -> Vec<WorkCommit> {
    let max = format!("--max-count={MAX_COMMITS}");
    let Ok(out) = git.run(&["log", "--reverse", &max, "--format=%H%x1f%s", range]) else {
        return Vec::new();
    };
    out.lines()
        .filter_map(|l| {
            let (sha, subject) = l.split_once('\u{1f}')?;
            is_sha(sha).then(|| WorkCommit {
                sha: sha.to_string(),
                subject: clip(subject, MAX_SUBJECT),
            })
        })
        .take(MAX_COMMITS)
        .collect()
}

pub fn summary(dir: &Path) -> WorkSummary {
    summary_at(dir, now_ms())
}

pub fn summary_at(dir: &Path, now: u64) -> WorkSummary {
    let (events, ..) = read_all(&dir.join(FILE));
    let mut out = WorkSummary::default();
    let mut started = None;
    let mut cur = (StateKind::Starting, 0u64);
    let mut ms = [0u64; 3];
    let mut branch = None;
    let mut ended = None;
    let spent = |ms: &mut [u64; 3], kind: StateKind, from: u64, to: u64| {
        let slot = match kind {
            StateKind::Working => 0,
            StateKind::Waiting => 1,
            StateKind::Blocked => 2,
            _ => return,
        };
        ms[slot] += to.saturating_sub(from);
    };
    for ev in events {
        if ended.is_some() && !matches!(ev, Ev::Commits { .. }) {
            continue;
        }
        match ev {
            Ev::Created {
                ts,
                profile,
                argv0_basename,
                task,
                cwd_branch,
                ..
            } => {
                if started.is_none() {
                    started = Some(ts);
                    cur = (StateKind::Starting, ts);
                    out.agent = profile
                        .clone()
                        .or(Some(argv0_basename).filter(|a| !a.is_empty()));
                    out.profile = profile;
                    out.task = task;
                    branch = cwd_branch;
                }
            }
            Ev::State { ts, kind, .. } => {
                if started.is_none() {
                    started = Some(ts);
                    cur.1 = ts;
                }
                let ts = ts.max(cur.1);
                spent(&mut ms, cur.0, cur.1, ts);
                cur = (kind, ts);
            }
            Ev::Exited { ts, code, signal } => {
                if started.is_none() {
                    started = Some(ts);
                    cur.1 = ts;
                }
                ended = Some(ts.max(cur.1));
                out.exit = Some(WorkExit { code, signal });
            }
            Ev::Commits {
                branch: b, list, ..
            } => {
                branch = b.or(branch);
                out.commits = list;
            }
            Ev::Truncated { .. } => out.truncated = true,
        }
    }
    let Some(start) = started else {
        return out;
    };
    let end = ended.unwrap_or_else(|| now.max(cur.1));
    spent(&mut ms, cur.0, cur.1, end);
    out.started_ms = start;
    out.ended_ms = ended;
    out.duration_s = end.saturating_sub(start) / 1000;
    [out.working_s, out.waiting_s, out.blocked_s] = ms.map(|m| m / 1000);
    out.branch = branch;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, lines: &[&str]) {
        std::fs::write(dir.join(FILE), lines.join("\n") + "\n").unwrap();
    }

    fn created(ts: u64) -> String {
        format!(
            r#"{{"ev":"created","ts":{ts},"profile":null,"argv0_basename":"sh","task":null,"bundle_sha256":null,"cwd_branch":"main","cwd_head":null}}"#
        )
    }

    fn state(ts: u64, kind: &str) -> String {
        format!(r#"{{"ev":"state","ts":{ts},"kind":"{kind}","detail":null}}"#)
    }

    fn st(kind: StateKind, since_ms: u64, detail: Option<&str>) -> State {
        State {
            kind,
            since_ms,
            detail: detail.map(String::from),
            exit_code: None,
        }
    }

    #[test]
    fn durations_come_from_state_changes() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            &[
                &created(1000),
                &state(2000, "working"),
                &state(12_000, "waiting"),
                &state(15_000, "blocked"),
                &state(16_000, "working"),
                r#"{"ev":"exited","ts":21000,"code":0,"signal":null}"#,
            ],
        );
        let s = summary_at(d.path(), 99_999);
        assert_eq!(s.started_ms, 1000);
        assert_eq!(s.ended_ms, Some(21_000));
        assert_eq!(s.duration_s, 20);
        assert_eq!((s.working_s, s.waiting_s, s.blocked_s), (15, 3, 1));
        assert_eq!(
            s.exit,
            Some(WorkExit {
                code: Some(0),
                signal: None
            })
        );
        assert_eq!(s.agent.as_deref(), Some("sh"));
        assert_eq!(s.branch.as_deref(), Some("main"));
    }

    #[test]
    fn a_running_session_counts_to_now() {
        let d = tempfile::tempdir().unwrap();
        write(d.path(), &[&created(0), &state(1000, "working")]);
        let s = summary_at(d.path(), 6000);
        assert_eq!(s.ended_ms, None);
        assert_eq!((s.duration_s, s.working_s), (6, 5));
    }

    #[test]
    fn malformed_lines_and_a_torn_tail_are_skipped() {
        let d = tempfile::tempdir().unwrap();
        let text = format!(
            "{}\nnot json\n{{\"ev\":\"nope\"}}\n{}\n{{\"ev\":\"sta",
            created(0),
            state(1000, "waiting")
        );
        std::fs::write(d.path().join(FILE), text).unwrap();
        let s = summary_at(d.path(), 4000);
        assert_eq!((s.waiting_s, s.duration_s), (3, 4));
        let log = Log::open(d.path(), d.path().to_path_buf());
        log.record(&st(StateKind::Working, 5000, None));
        assert_eq!(summary_at(d.path(), 6000).working_s, 1);
    }

    #[test]
    fn a_truncated_log_is_flagged_and_still_summarised() {
        let d = tempfile::tempdir().unwrap();
        write(
            d.path(),
            &[
                &created(0),
                &state(1000, "working"),
                r#"{"ev":"truncated","ts":2000}"#,
            ],
        );
        let s = summary_at(d.path(), 3000);
        assert!(s.truncated);
        assert_eq!(s.working_s, 2);
    }

    #[test]
    fn a_missing_log_is_an_empty_summary() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(summary_at(d.path(), 5), WorkSummary::default());
    }

    #[test]
    fn the_log_is_bounded_and_keeps_room_for_the_exit() {
        let d = tempfile::tempdir().unwrap();
        let log = Log::open(d.path(), d.path().to_path_buf());
        for i in 0..MAX_LINES + 50 {
            let kind = if i % 2 == 0 {
                StateKind::Working
            } else {
                StateKind::Waiting
            };
            log.record(&st(kind, i as u64, None));
        }
        log.exited(99_999, Some(0), None);
        let text = std::fs::read_to_string(d.path().join(FILE)).unwrap();
        let n = text.lines().count();
        assert!(n <= MAX_LINES, "{n} lines");
        assert!(text.len() <= MAX_BYTES);
        assert_eq!(text.matches("\"truncated\"").count(), 1);
        let s = summary_at(d.path(), 0);
        assert!(s.truncated);
        assert_eq!(s.exit.map(|e| e.code), Some(Some(0)));
        let reopened = Log::open(d.path(), d.path().to_path_buf());
        reopened.record(&st(StateKind::Blocked, 1, None));
        assert_eq!(std::fs::read_to_string(d.path().join(FILE)).unwrap(), text);
    }

    #[test]
    fn only_kind_changes_are_logged_and_details_carry_no_screen_text() {
        let d = tempfile::tempdir().unwrap();
        let log = Log::open(d.path(), d.path().to_path_buf());
        log.record(&st(StateKind::Starting, 1, None));
        log.record(&st(StateKind::Working, 1, None));
        log.record(&st(StateKind::Working, 1, Some("x")));
        log.record(&st(
            StateKind::Blocked,
            1,
            Some("perm: Allow rm -rf /secret? (y/n)"),
        ));
        log.record(&st(StateKind::Blocked, 1, Some("other: more text")));
        let text = std::fs::read_to_string(d.path().join(FILE)).unwrap();
        assert_eq!(text.lines().count(), 2, "{text}");
        assert!(text.contains(r#""detail":"perm""#));
        assert!(!text.contains("secret"));
    }

    #[test]
    fn a_huge_commit_subject_leaves_the_facts_absent() {
        let d = tempfile::tempdir().unwrap();
        let repo = d.path().join("repo");
        let logs = d.path().join("logs");
        std::fs::create_dir(&repo).unwrap();
        std::fs::create_dir(&logs).unwrap();
        let g = GitEnv::bare(&repo)
            .env("GIT_AUTHOR_NAME", "t")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_NAME", "t")
            .env("GIT_COMMITTER_EMAIL", "t@example.com");
        g.run(&["init", "-b", "main"]).unwrap();
        g.run(&["commit", "--allow-empty", "-m", "first"]).unwrap();
        let log = Log::open(&logs, repo.clone());
        std::fs::write(d.path().join("msg"), "x".repeat(5 * 1024 * 1024)).unwrap();
        let msg = d.path().join("msg");
        g.run(&["commit", "--allow-empty", "-F", msg.to_str().unwrap()])
            .unwrap();
        let t = std::time::Instant::now();
        log.exited(9, Some(0), None);
        assert!(t.elapsed() < std::time::Duration::from_secs(10));
        let text = std::fs::read_to_string(logs.join(FILE)).unwrap();
        assert!(text.len() < 64 * 1024, "{}", text.len());
        assert!(text.contains(r#""ev":"commits""#), "{text}");
        assert!(text.contains(r#""list":[]"#), "{text}");
    }

    #[test]
    fn a_planted_symlink_is_not_followed() {
        let d = tempfile::tempdir().unwrap();
        let target = d.path().join("target");
        std::fs::write(&target, "keep").unwrap();
        let sdir = d.path().join("s");
        std::fs::create_dir(&sdir).unwrap();
        std::os::unix::fs::symlink(&target, sdir.join(FILE)).unwrap();
        let log = Log::open(&sdir, sdir.clone());
        log.record(&st(StateKind::Working, 1, None));
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "keep");
    }
}
