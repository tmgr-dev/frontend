use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Receiver;
use tmgr_core::active::{self, ActiveState};
use tmgr_core::runstate::{self, Run, Runs};
use tmgr_ipc::api::TaskRef;
use tmgr_runner::attach::TaskChip;

pub(super) struct ChipJob {
    pub id: String,
    pub task: Option<TaskRef>,
    pub branch: Option<String>,
    pub created_s: u64,
    pub sandbox_home: PathBuf,
}

pub(super) trait Opener: Send {
    fn open(&mut self, desktop: &str, web: &str) -> bool;
}

pub(super) struct SystemOpener;

fn launcher() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("open")
    } else if std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some()
    {
        Some("xdg-open")
    } else {
        None
    }
}

fn launch(program: &str, url: &str) -> bool {
    Command::new(program)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

impl Opener for SystemOpener {
    fn open(&mut self, desktop: &str, web: &str) -> bool {
        let Some(program) = launcher() else {
            return false;
        };
        let (desktop, web) = (desktop.to_string(), web.to_string());
        std::thread::spawn(move || {
            if !launch(program, &desktop) {
                launch(program, &web);
            }
        });
        true
    }
}

fn key_of_branch(branch: &str, known: &HashSet<String>) -> Option<String> {
    let (code, rest) = branch.split_once('-')?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    if digits.is_empty() || digits.len() > 7 {
        return None;
    }
    if rest[digits.len()..]
        .chars()
        .next()
        .is_some_and(|c| !matches!(c, '-' | '_' | '/' | '.'))
    {
        return None;
    }
    let code = code.to_ascii_uppercase();
    known.contains(&code).then(|| format!("{code}-{digits}"))
}

fn workspace_of(active: &ActiveState, id: u64) -> Option<String> {
    if active.task.id != id
        || active
            .workspace
            .as_deref()
            .is_some_and(|w| w.starts_with("local:"))
    {
        return None;
    }
    let url = active.task.url.as_deref()?;
    let path = url.split_once("://").map_or(url, |(_, r)| r);
    let mut parts = path.split('/').skip(1);
    let (ws, kind, task) = (parts.next()?, parts.next()?, parts.next()?);
    (kind == "tasks"
        && task.parse::<u64>().ok() == Some(id)
        && tmgr_core::refs::is_workspace_code(ws))
    .then(|| ws.to_string())
}

fn started_s(run: &Run) -> Option<u64> {
    let t = chrono::DateTime::parse_from_rfc3339(&run.started_at).ok()?;
    u64::try_from(t.timestamp()).ok()
}

const MAX_STATE_FILE: u64 = 256 * 1024;

fn real_dir(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|m| m.is_dir())
}

fn read_state(path: &Path, root: Option<&Path>) -> Option<Vec<u8>> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    if let Some(root) = root {
        let mut dir = root.to_path_buf();
        if !real_dir(&dir) {
            return None;
        }
        for part in path.strip_prefix(root).ok()?.parent()?.components() {
            dir.push(part);
            if !real_dir(&dir) {
                return None;
            }
        }
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut buf = Vec::new();
    file.take(MAX_STATE_FILE + 1).read_to_end(&mut buf).ok()?;
    (buf.len() as u64 <= MAX_STATE_FILE).then_some(buf)
}

#[derive(Default)]
pub(super) struct Records {
    runs: Runs,
    active: Vec<ActiveState>,
}

fn load_dir(dir: &Path, root: Option<&Path>) -> Records {
    let runs = read_state(&dir.join("runs.json"), root)
        .map(|b| runstate::parse(&b))
        .unwrap_or_default();
    let active = read_state(&dir.join("active.json"), root)
        .and_then(|b| active::parse(&b))
        .into_iter()
        .collect();
    Records { runs, active }
}

pub(super) fn user_records() -> Records {
    active::state_dir().map_or_else(Records::default, |d| load_dir(&d, None))
}

fn sandbox_records(home: &Path) -> Records {
    load_dir(&home.join(".local").join("state").join("tmgr"), Some(home))
}

fn merge(user: &Records, sandbox: Records, trust_sandbox: bool) -> Records {
    let mut all = Records::default();
    all.runs.runs.extend(user.runs.runs.iter().cloned());
    all.active.extend(user.active.iter().cloned());
    if trust_sandbox {
        all.runs.runs.splice(0..0, sandbox.runs.runs);
        all.active.splice(0..0, sandbox.active);
    }
    all
}

fn known_categories(user: &Records) -> HashSet<String> {
    user.runs
        .runs
        .iter()
        .map(|r| r.task_key.as_str())
        .chain(user.active.iter().map(|a| a.task.key.as_str()))
        .filter_map(|k| k.split_once('-').map(|(c, _)| c.to_string()))
        .collect()
}

pub(super) fn resolve(job: &ChipJob, user: &Records, sandbox: Records) -> Option<TaskChip> {
    let known = known_categories(user);
    let branch_key = job.branch.as_deref().and_then(|b| key_of_branch(b, &known));
    let rec = merge(user, sandbox, job.task.is_some());
    let run =
        match (&job.task, &branch_key) {
            (Some(t), _) => {
                let mut hits = rec
                    .runs
                    .runs
                    .iter()
                    .rev()
                    .filter(|r| i64::try_from(r.task_id) == Ok(t.task_id));
                let first = hits.next();
                let cloud =
                    rec.runs.runs.iter().rev().find(|r| {
                        i64::try_from(r.task_id) == Ok(t.task_id) && r.workspace.is_none()
                    });
                cloud.or(first)
            }
            (None, Some(k)) => rec.runs.runs.iter().rev().find(|r| &r.task_key == k),
            _ => None,
        };
    let task_id = job
        .task
        .map(|t| t.task_id as u64)
        .or(run.map(|r| r.task_id))
        .or_else(|| {
            let k = branch_key.as_ref()?;
            rec.active
                .iter()
                .find(|a| &a.task.key == k)
                .map(|a| a.task.id)
        });
    let key = run
        .map(|r| r.task_key.clone())
        .or(branch_key)
        .or_else(|| {
            let id = task_id?;
            rec.active
                .iter()
                .find(|a| a.task.id == id)
                .map(|a| a.task.key.clone())
        })
        .or_else(|| task_id.map(|id| format!("#{id}")))?;
    if !(key.starts_with('#') || active::valid_key(&key)) {
        return None;
    }
    let local = run.is_some_and(|r| {
        r.workspace
            .as_deref()
            .is_some_and(|w| w.starts_with("local:"))
    });
    let workspace = task_id
        .filter(|_| !local)
        .and_then(|id| rec.active.iter().find_map(|a| workspace_of(a, id)));
    let started_s = match (run.and_then(started_s), job.task.is_some()) {
        (Some(s), true) if s + 120 >= job.created_s => Some(s),
        (Some(s), false) => Some(s),
        (_, true) => Some(job.created_s),
        _ => None,
    };
    let branch = job
        .branch
        .clone()
        .or_else(|| run.and_then(|r| r.branch.clone()))
        .map(|b| tmgr_core::text::truncate_chars(&tmgr_core::text::sanitize_line(&b), 80).0)
        .filter(|b| !b.is_empty());
    Some(TaskChip {
        key,
        branch,
        started_s,
        workspace,
        task_id,
    })
}

pub(super) fn chipper(
    jobs: Receiver<Vec<ChipJob>>,
    mut send: impl FnMut(HashMap<String, TaskChip>) -> bool,
) {
    for batch in jobs {
        let mut found = HashMap::new();
        let user = user_records();
        for job in &batch {
            if let Some(chip) = resolve(job, &user, sandbox_records(&job.sandbox_home)) {
                found.insert(job.id.clone(), chip);
            }
        }
        if !send(found) {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmgr_core::active::ActiveTask;

    fn run(id: u64, key: &str, branch: &str) -> Run {
        Run {
            run_id: id,
            task_id: id * 10,
            task_key: key.into(),
            branch: Some(branch.into()),
            repo: None,
            start_sha: None,
            agent: "claude-code".into(),
            started_at: "2026-10-12T10:00:00Z".into(),
            commits: Vec::new(),
            last_sent: 0,
            workspace: None,
        }
    }

    fn rec(runs: Vec<Run>, active: Vec<ActiveState>) -> Records {
        Records {
            runs: Runs {
                schema: runstate::RUNS_SCHEMA.into(),
                runs,
            },
            active,
        }
    }

    fn resolve_user(j: &ChipJob, user: &Records) -> Option<TaskChip> {
        resolve(j, user, Records::default())
    }

    fn active_for(id: u64, key: &str, url: &str) -> ActiveState {
        ActiveState::new(ActiveTask {
            id,
            key: key.into(),
            title: "t".into(),
            url: Some(url.into()),
        })
    }

    fn job(task: Option<(i64, i64)>, branch: Option<&str>) -> ChipJob {
        ChipJob {
            id: "s".into(),
            task: task.map(|(w, t)| TaskRef {
                workspace_id: w,
                task_id: t,
            }),
            branch: branch.map(str::to_string),
            created_s: 1_000,
            sandbox_home: PathBuf::from("/nonexistent"),
        }
    }

    #[test]
    fn a_task_run_gives_the_key_branch_and_start_of_the_run_record() {
        let r = rec(vec![run(64, "TM-644", "tm-644-open-desktop")], Vec::new());
        let chip = resolve_user(&job(Some((1, 640)), Some("tm-644-open-desktop")), &r).unwrap();
        assert_eq!(chip.key, "TM-644");
        assert_eq!(chip.branch.as_deref(), Some("tm-644-open-desktop"));
        assert_eq!(chip.task_id, Some(640));
        assert_eq!(chip.started_s, Some(1_791_799_200));
    }

    #[test]
    fn a_branch_with_a_known_key_finds_the_run_without_a_task_ref() {
        let r = rec(vec![run(64, "TM-644", "x")], Vec::new());
        let chip = resolve_user(&job(None, Some("tm-644-open-desktop")), &r).unwrap();
        assert_eq!((chip.key.as_str(), chip.task_id), ("TM-644", Some(640)));
    }

    #[test]
    fn an_unknown_category_or_no_branch_means_no_chip() {
        let r = rec(vec![run(64, "TM-644", "x")], Vec::new());
        assert_eq!(resolve_user(&job(None, Some("zz-12-thing")), &r), None);
        assert_eq!(resolve_user(&job(None, Some("main")), &r), None);
        assert_eq!(resolve_user(&job(None, None), &r), None);
    }

    #[test]
    fn a_session_task_without_a_record_still_gets_a_chip_by_id() {
        let chip = resolve_user(&job(Some((1, 77)), None), &rec(Vec::new(), Vec::new())).unwrap();
        assert_eq!(chip.key, "#77");
        assert_eq!(chip.started_s, Some(1_000));
        assert_eq!(chip.workspace, None);
    }

    #[test]
    fn the_workspace_code_comes_only_from_a_matching_active_task_url() {
        let act = active_for(640, "TM-644", "https://tmgr.dev/tmgrdev/tasks/640");
        let r = rec(vec![run(64, "TM-644", "b")], vec![act]);
        let chip = resolve_user(&job(Some((1, 640)), None), &r).unwrap();
        assert_eq!(chip.workspace.as_deref(), Some("tmgrdev"));
        let other = active_for(5, "TM-9", "https://tmgr.dev/evil/tasks/5");
        let chip = resolve_user(
            &job(Some((1, 640)), None),
            &rec(vec![run(64, "TM-644", "b")], vec![other]),
        )
        .unwrap();
        assert_eq!(chip.workspace, None);
        let bad = active_for(640, "TM-644", "https://tmgr.dev/a b/tasks/640");
        let chip = resolve_user(&job(Some((1, 640)), None), &rec(Vec::new(), vec![bad])).unwrap();
        assert_eq!(chip.workspace, None);
    }

    #[test]
    fn a_key_that_is_not_a_task_key_is_dropped() {
        let mut r = run(64, "tm 644; rm", "b");
        r.task_id = 640;
        assert_eq!(
            resolve_user(&job(Some((1, 640)), None), &rec(vec![r], Vec::new())),
            None
        );
    }

    #[test]
    fn sandbox_records_are_ignored_when_the_session_has_no_task_ref() {
        let sandbox = rec(vec![run(64, "TM-644", "tm-644-x")], Vec::new());
        let chip = resolve(&job(None, Some("tm-644-x")), &Records::default(), sandbox);
        assert_eq!(chip, None);
    }

    #[test]
    fn a_session_task_ref_uses_sandbox_records_for_the_label_but_keeps_its_own_id() {
        let sandbox = rec(vec![run(64, "TM-644", "tm-644-x")], Vec::new());
        let chip = resolve(&job(Some((1, 640)), None), &Records::default(), sandbox).unwrap();
        assert_eq!((chip.key.as_str(), chip.task_id), ("TM-644", Some(640)));
    }

    #[test]
    fn a_local_workspace_run_has_no_link() {
        let mut r = run(64, "TM-644", "b");
        r.workspace = Some("local:team".into());
        let act = active_for(640, "TM-644", "https://tmgr.dev/team/tasks/640");
        let chip = resolve_user(&job(Some((1, 640)), None), &rec(vec![r], vec![act])).unwrap();
        assert_eq!(chip.workspace, None);
    }

    #[test]
    fn an_old_run_does_not_stretch_the_timer() {
        let r = run(64, "TM-644", "b");
        let mut j = job(Some((1, 640)), None);
        j.created_s = 1_900_000_000;
        let chip = resolve_user(&j, &rec(vec![r], Vec::new())).unwrap();
        assert_eq!(chip.started_s, Some(1_900_000_000));
    }

    #[test]
    fn branch_keys_need_a_boundary_after_the_digits() {
        let known: HashSet<String> = ["TM".to_string()].into();
        assert_eq!(
            key_of_branch("tm-644-open", &known).as_deref(),
            Some("TM-644")
        );
        assert_eq!(key_of_branch("tm-644", &known).as_deref(), Some("TM-644"));
        assert_eq!(key_of_branch("tm-644abc", &known), None);
        assert_eq!(key_of_branch("xx-1", &known), None);
    }

    #[test]
    fn the_branch_is_cleaned_for_display() {
        let r = rec(vec![run(64, "TM-644", "b")], Vec::new());
        let chip = resolve_user(
            &job(Some((1, 640)), Some("tm-644-a\u{202e}b\u{1b}[31m")),
            &r,
        )
        .unwrap();
        assert!(!chip.branch.unwrap().contains('\u{202e}'));
    }

    #[test]
    fn state_files_are_read_without_following_symlinks_or_fifos() {
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        let state = home.join(".local").join("state").join("tmgr");
        std::fs::create_dir_all(&state).unwrap();
        let real = state.join("runs.json");
        std::fs::write(&real, b"{}").unwrap();
        assert_eq!(read_state(&real, Some(&home)).as_deref(), Some(&b"{}"[..]));
        let outside = dir.path().join("secret.json");
        std::fs::write(&outside, b"{}").unwrap();
        let link = state.join("active.json");
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        assert_eq!(read_state(&link, Some(&home)), None);
        let elsewhere = dir.path().join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("runs.json"), b"{}").unwrap();
        let hijack = dir.path().join("home2");
        std::fs::create_dir_all(hijack.join(".local").join("state")).unwrap();
        std::os::unix::fs::symlink(&elsewhere, hijack.join(".local").join("state").join("tmgr"))
            .unwrap();
        let path = hijack
            .join(".local")
            .join("state")
            .join("tmgr")
            .join("runs.json");
        assert_eq!(read_state(&path, Some(&hijack)), None);
    }
}
