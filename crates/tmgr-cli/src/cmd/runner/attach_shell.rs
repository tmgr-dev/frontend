use ratatui::layout::Rect;
use std::collections::{HashMap, HashSet};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tmgr_ipc::api::StateKind;
use tmgr_runner::attach::{SessionRow, SplitDir, MAX_LEAVES};

pub(super) const PENDING_TTL: Duration = Duration::from_secs(5);
const FALLBACK_SHELL: &str = "/bin/sh";
const MIN_COLS: u16 = 10;
const MIN_ROWS: u16 = 3;

fn is_executable_file(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

pub(super) fn pick_shell(env: Option<&str>, executable: impl Fn(&Path) -> bool) -> String {
    env.filter(|s| s.starts_with('/') && executable(Path::new(s)))
        .unwrap_or(FALLBACK_SHELL)
        .to_string()
}

pub(super) fn shell_argv() -> Vec<String> {
    let env = std::env::var("SHELL").ok();
    vec![
        pick_shell(env.as_deref(), is_executable_file),
        "-l".to_string(),
    ]
}

pub(super) fn next_shell_name<'a>(names: impl Iterator<Item = &'a str>) -> String {
    let used: HashSet<&str> = names.collect();
    (1u32..)
        .map(|n| format!("sh-{n}"))
        .find(|n| !used.contains(n.as_str()))
        .unwrap_or_else(|| "sh-0".to_string())
}

pub(super) fn shell_name(
    sessions: &[SessionRow],
    creating: &HashMap<String, String>,
    planned: Option<&str>,
) -> String {
    next_shell_name(
        sessions
            .iter()
            .map(|r| r.name.as_str())
            .chain(creating.values().map(String::as_str))
            .chain(planned),
    )
}

pub(super) const TYPED_CAP: usize = 4096;
pub(super) type Ladder = &'static [(&'static str, u64)];
pub(super) const STOP_LADDER: Ladder = &[("int", 10), ("term", 5), ("kill", 3)];
pub(super) const KILL_ONLY: Ladder = &[("kill", 2)];

pub(super) fn ladder_for(shown: bool) -> Ladder {
    if shown {
        STOP_LADDER
    } else {
        KILL_ONLY
    }
}

pub(super) fn can_split(panes: usize) -> bool {
    panes < MAX_LEAVES
}

pub(super) fn focus_new_pane(focused: Option<&str>, target: &str) -> bool {
    focused == Some(target)
}

pub(super) fn push_typed(buf: &mut Vec<u8>, data: &[u8]) {
    let room = TYPED_CAP.saturating_sub(buf.len());
    buf.extend_from_slice(&data[..data.len().min(room)]);
}

pub(super) fn is_shell_session(argv: &[String], name: &str) -> bool {
    argv.len() == 2
        && argv[1] == "-l"
        && name
            .strip_prefix("sh-")
            .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

pub(super) fn cwd_candidates(live: Option<PathBuf>, start: Option<&str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let live = live.map(|p| p.to_string_lossy().into_owned());
    for c in live.into_iter().chain(start.map(str::to_string)) {
        if !c.is_empty() && !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

pub(super) fn size_hint(content: Rect, dir: SplitDir) -> (u16, u16) {
    let (cols, rows) = match dir {
        SplitDir::Right => ((content.width / 2).saturating_sub(1), content.height),
        SplitDir::Down => (content.width, (content.height / 2).saturating_sub(1)),
    };
    (cols.max(MIN_COLS), rows.max(MIN_ROWS))
}

pub(super) fn merge_pending(
    mut new: Vec<SessionRow>,
    old: &[SessionRow],
    pending: &mut HashMap<String, Instant>,
    now: Instant,
) -> Vec<SessionRow> {
    pending.retain(|id, since| {
        now.saturating_duration_since(*since) < PENDING_TTL && !new.iter().any(|r| &r.id == id)
    });
    for id in pending.keys() {
        if let Some(row) = old.iter().find(|r| &r.id == id) {
            new.push(row.clone());
        }
    }
    new
}

pub(super) fn watch_ids(panes: &[String], sessions: &[SessionRow]) -> Vec<String> {
    panes
        .iter()
        .filter(|id| sessions.iter().any(|s| &s.id == *id))
        .cloned()
        .collect()
}

pub(super) fn reap_ids(
    sessions: &[SessionRow],
    is_shell: impl Fn(&str) -> bool,
    removing: &HashSet<String>,
    stopped: &HashSet<String>,
) -> Vec<String> {
    sessions
        .iter()
        .filter(|r| r.state.kind == StateKind::Exited)
        .filter(|r| is_shell(&r.id) && !removing.contains(&r.id) && !stopped.contains(&r.id))
        .map(|r| r.id.clone())
        .collect()
}

#[cfg(target_os = "linux")]
pub(super) fn process_cwd(pid: u32) -> Option<PathBuf> {
    if pid == 0 {
        return None;
    }
    std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
}

#[cfg(target_os = "macos")]
pub(super) fn process_cwd(pid: u32) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    if pid == 0 {
        return None;
    }
    let mut info: libc::proc_vnodepathinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_vnodepathinfo>() as libc::c_int;
    let got = unsafe {
        libc::proc_pidinfo(
            i32::try_from(pid).ok()?,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            std::ptr::addr_of_mut!(info).cast::<libc::c_void>(),
            size,
        )
    };
    if got != size {
        return None;
    }
    let bytes: Vec<u8> = info
        .pvi_cdir
        .vip_path
        .iter()
        .flatten()
        .map(|&c| c as u8)
        .take_while(|&b| b != 0)
        .collect();
    if bytes.is_empty() {
        return None;
    }
    Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub(super) fn process_cwd(_pid: u32) -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmgr_ipc::api::State;

    fn row(id: &str, name: &str, kind: StateKind) -> SessionRow {
        SessionRow {
            id: id.into(),
            name: name.into(),
            state: State {
                kind,
                since_ms: 0,
                detail: None,
                exit_code: None,
            },
            age: String::new(),
            esc: false,
            resumed_from: None,
        }
    }

    #[test]
    fn shell_path_falls_back_unless_env_is_an_absolute_executable() {
        let yes = |_: &Path| true;
        let no = |_: &Path| false;
        assert_eq!(pick_shell(Some("/bin/zsh"), yes), "/bin/zsh");
        assert_eq!(pick_shell(Some("zsh"), yes), "/bin/sh");
        assert_eq!(pick_shell(Some("./zsh"), yes), "/bin/sh");
        assert_eq!(pick_shell(None, yes), "/bin/sh");
        assert_eq!(pick_shell(Some("/bin/zsh"), no), "/bin/sh");
        assert_eq!(pick_shell(Some(""), yes), "/bin/sh");
    }

    #[test]
    fn shell_path_checks_the_real_file_mode() {
        let dir = tempfile::tempdir().unwrap();
        let plain = dir.path().join("plain");
        std::fs::write(&plain, "x").unwrap();
        assert!(!is_executable_file(&plain));
        std::fs::set_permissions(&plain, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(is_executable_file(&plain));
        assert!(!is_executable_file(dir.path()));
        assert!(!is_executable_file(&dir.path().join("missing")));
    }

    #[test]
    fn argv_is_a_login_shell() {
        let argv = shell_argv();
        assert_eq!(argv.len(), 2);
        assert!(argv[0].starts_with('/'));
        assert_eq!(argv[1], "-l");
    }

    #[test]
    fn name_is_the_smallest_unused_among_all_sessions() {
        assert_eq!(next_shell_name([].into_iter()), "sh-1");
        assert_eq!(next_shell_name(["sh-1", "sh-2"].into_iter()), "sh-3");
        assert_eq!(next_shell_name(["sh-1", "sh-3"].into_iter()), "sh-2");
        assert_eq!(next_shell_name(["sh-2", "alpha"].into_iter()), "sh-1");
        assert_eq!(next_shell_name(["sh-x", "sh-"].into_iter()), "sh-1");
    }

    #[test]
    fn concurrent_splits_reserve_distinct_names() {
        let sessions = vec![row("a", "alpha", StateKind::Working)];
        let mut creating = HashMap::new();
        let first = shell_name(&sessions, &creating, None);
        creating.insert("a".to_string(), first.clone());
        let second = shell_name(&sessions, &creating, None);
        assert_eq!(first, "sh-1");
        assert_eq!(second, "sh-2");
        assert_eq!(shell_name(&sessions, &creating, Some("sh-2")), "sh-3");
    }

    #[test]
    fn never_shown_shells_are_killed_directly() {
        assert_eq!(ladder_for(false), KILL_ONLY);
        assert_eq!(ladder_for(true), STOP_LADDER);
        assert_eq!(KILL_ONLY.len(), 1);
        assert_eq!(KILL_ONLY[0].0, "kill");
    }

    #[test]
    fn splitting_is_refused_at_the_leaf_cap() {
        assert!(can_split(MAX_LEAVES - 1));
        assert!(!can_split(MAX_LEAVES));
    }

    #[test]
    fn focus_moves_to_the_new_pane_only_from_the_target() {
        assert!(focus_new_pane(Some("a"), "a"));
        assert!(!focus_new_pane(Some("b"), "a"));
        assert!(!focus_new_pane(None, "a"));
    }

    #[test]
    fn typed_bytes_are_capped() {
        let mut buf = Vec::new();
        push_typed(&mut buf, b"ls");
        assert_eq!(buf, b"ls");
        push_typed(&mut buf, &vec![b'x'; TYPED_CAP]);
        assert_eq!(buf.len(), TYPED_CAP);
        push_typed(&mut buf, b"more");
        assert_eq!(buf.len(), TYPED_CAP);
    }

    #[test]
    fn shell_sessions_are_recognised_by_argv_and_name() {
        let sh = vec!["/bin/zsh".to_string(), "-l".to_string()];
        assert!(is_shell_session(&sh, "sh-1"));
        assert!(is_shell_session(&sh, "sh-42"));
        assert!(!is_shell_session(&sh, "sh-"));
        assert!(!is_shell_session(&sh, "sh-1a"));
        assert!(!is_shell_session(&sh, "build"));
        assert!(!is_shell_session(&sh[..1], "sh-1"));
        let other = vec!["/bin/zsh".to_string(), "-c".to_string()];
        assert!(!is_shell_session(&other, "sh-1"));
        let long = vec!["a".into(), "-l".into(), "b".into()];
        assert!(!is_shell_session(&long, "sh-1"));
    }

    #[test]
    fn cwd_prefers_the_live_directory_then_the_start_one() {
        assert_eq!(
            cwd_candidates(Some("/live".into()), Some("/start")),
            vec!["/live", "/start"]
        );
        assert_eq!(cwd_candidates(None, Some("/start")), vec!["/start"]);
        assert_eq!(
            cwd_candidates(Some("/same".into()), Some("/same")),
            vec!["/same"]
        );
        assert_eq!(cwd_candidates(Some("/live".into()), None), vec!["/live"]);
        assert!(cwd_candidates(None, Some("")).is_empty());
    }

    #[test]
    fn size_hint_halves_the_split_axis_and_keeps_a_floor() {
        let r = Rect::new(0, 0, 80, 40);
        assert_eq!(size_hint(r, SplitDir::Right), (39, 40));
        assert_eq!(size_hint(r, SplitDir::Down), (80, 19));
        assert_eq!(size_hint(Rect::new(0, 0, 4, 2), SplitDir::Right), (10, 3));
        assert_eq!(size_hint(Rect::new(0, 0, 0, 0), SplitDir::Down), (10, 3));
    }

    #[test]
    fn process_cwd_of_this_process_matches_current_dir() {
        let want = std::env::current_dir().unwrap().canonicalize().unwrap();
        let got = process_cwd(std::process::id())
            .unwrap()
            .canonicalize()
            .unwrap();
        assert_eq!(got, want);
        assert_eq!(process_cwd(0), None);
    }

    #[test]
    fn pending_rows_survive_a_stale_list_until_seen_or_expired() {
        let now = Instant::now();
        let old = vec![
            row("a", "alpha", StateKind::Working),
            row("n", "sh-1", StateKind::Working),
        ];
        let mut pending = HashMap::from([("n".to_string(), now)]);
        let merged = merge_pending(vec![old[0].clone()], &old, &mut pending, now);
        assert_eq!(merged.len(), 2);
        assert!(pending.contains_key("n"));

        let merged = merge_pending(old.clone(), &old, &mut pending, now);
        assert_eq!(merged.len(), 2);
        assert!(pending.is_empty());

        let mut pending = HashMap::from([("n".to_string(), now)]);
        let later = now + PENDING_TTL + Duration::from_millis(1);
        let merged = merge_pending(vec![old[0].clone()], &old, &mut pending, later);
        assert_eq!(merged.len(), 1);
        assert!(pending.is_empty());

        let mut pending = HashMap::from([("ghost".to_string(), now)]);
        let merged = merge_pending(vec![old[0].clone()], &old, &mut pending, now);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn watch_list_only_has_known_sessions() {
        let sessions = vec![row("a", "alpha", StateKind::Working)];
        let panes = vec!["a".to_string(), "gone".to_string()];
        assert_eq!(watch_ids(&panes, &sessions), vec!["a"]);
        assert!(watch_ids(&[], &sessions).is_empty());
    }

    #[test]
    fn only_exited_shells_not_already_being_removed_are_reaped() {
        let sessions = vec![
            row("s1", "sh-1", StateKind::Exited),
            row("s2", "sh-2", StateKind::Working),
            row("x", "build", StateKind::Exited),
            row("s3", "sh-3", StateKind::Exited),
        ];
        let is_shell = |id: &str| matches!(id, "s1" | "s2" | "s3");
        let mut removing = HashSet::new();
        let mut stopped = HashSet::new();
        let reaped = |removing: &HashSet<String>, stopped: &HashSet<String>| {
            reap_ids(&sessions, is_shell, removing, stopped)
        };
        assert_eq!(reaped(&removing, &stopped), vec!["s1", "s3"]);
        removing.insert("s1".to_string());
        assert_eq!(reaped(&removing, &stopped), vec!["s3"]);
        stopped.insert("s3".to_string());
        assert!(reaped(&removing, &stopped).is_empty());
    }
}
