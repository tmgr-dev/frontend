use crate::store::Paths;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tmgr_core::config::{ensure_private_dir, write_atomic};
use tmgr_core::error::Result;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attached {
    pub run: String,
    pub input: bool,
    pub remote: bool,
    pub bypass: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct Rec {
    pid: u32,
    start: String,
    enrollment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    attached: Option<Attached>,
}

fn dir(paths: &Paths) -> PathBuf {
    paths.link_dir().join("workers")
}

fn file(paths: &Paths, name: &str, pid: u32) -> PathBuf {
    dir(paths).join(format!("{name}.{pid}.json"))
}

#[doc(hidden)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Probe {
    Running { start: String, zombie: bool },
    Gone,
    Unknown,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Life {
    Alive,
    Gone,
    Unknown,
}

fn after_failed_query(pid: u32) -> Probe {
    if pid == 0 || pid > i32::MAX as u32 {
        return Probe::Unknown;
    }
    let r = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if r == -1 && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
        Probe::Gone
    } else {
        Probe::Unknown
    }
}

#[cfg(target_os = "linux")]
fn query(pid: u32) -> Probe {
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(text) => parse_stat(&text).unwrap_or(Probe::Unknown),
        Err(_) => after_failed_query(pid),
    }
}

#[cfg(target_os = "linux")]
fn parse_stat(text: &str) -> Option<Probe> {
    let rest = text.get(text.rfind(')')? + 2..)?;
    let fields: Vec<&str> = rest.split(' ').collect();
    Some(Probe::Running {
        start: fields.get(19)?.to_string(),
        zombie: fields.first() == Some(&"Z"),
    })
}

#[cfg(target_os = "macos")]
fn query(pid: u32) -> Probe {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    let got = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            1,
            &mut info as *mut _ as *mut libc::c_void,
            size,
        )
    };
    if got == size {
        Probe::Running {
            start: format!("{}.{}", info.pbi_start_tvsec, info.pbi_start_tvusec),
            zombie: info.pbi_status == 5,
        }
    } else if got == 0 {
        after_failed_query(pid)
    } else {
        Probe::Unknown
    }
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn query(pid: u32) -> Probe {
    after_failed_query(pid)
}

pub fn process_start(pid: u32) -> Option<String> {
    match query(pid) {
        Probe::Running { start, .. } => Some(start),
        _ => None,
    }
}

fn life(probe: &dyn Fn(u32) -> Probe, pid: u32, start: &str) -> Life {
    match probe(pid) {
        Probe::Running {
            start: s,
            zombie: false,
        } if s == start => Life::Alive,
        Probe::Running { .. } | Probe::Gone => Life::Gone,
        Probe::Unknown => Life::Unknown,
    }
}

fn alive(pid: u32, start: &str) -> bool {
    life(&query, pid, start) == Life::Alive
}

pub struct WorkerGuard {
    path: PathBuf,
}

impl Drop for WorkerGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

pub fn register(paths: &Paths, name: &str, enrollment: &str) -> Result<WorkerGuard> {
    ensure_private_dir(&paths.config)?;
    ensure_private_dir(&paths.link_dir())?;
    ensure_private_dir(&dir(paths))?;
    let pid = std::process::id();
    let start = process_start(pid).unwrap_or_default();
    let rec = Rec {
        pid,
        start,
        enrollment: enrollment.to_string(),
        attached: None,
    };
    let path = file(paths, name, pid);
    write_rec(&path, &rec)?;
    Ok(WorkerGuard { path })
}

fn write_rec(path: &std::path::Path, rec: &Rec) -> Result<()> {
    let bytes = serde_json::to_vec(rec)
        .map_err(|_| tmgr_core::error::Error::internal("cannot serialize a worker record"))?;
    write_atomic(path, &bytes, 0o600)
}

pub fn set_attached(
    paths: &Paths,
    name: &str,
    enrollment: &str,
    attached: Option<Attached>,
) -> Result<()> {
    ensure_private_dir(&paths.config)?;
    ensure_private_dir(&paths.link_dir())?;
    ensure_private_dir(&dir(paths))?;
    let pid = std::process::id();
    write_rec(
        &file(paths, name, pid),
        &Rec {
            pid,
            start: process_start(pid).unwrap_or_default(),
            enrollment: enrollment.to_string(),
            attached,
        },
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttachedLeader {
    pub link: String,
    pub input: bool,
}

pub fn attached_leaders(paths: &Paths) -> std::collections::BTreeMap<String, Vec<AttachedLeader>> {
    let mut out: std::collections::BTreeMap<String, Vec<AttachedLeader>> = Default::default();
    let Ok(entries) = std::fs::read_dir(dir(paths)) else {
        return out;
    };
    for e in entries.flatten() {
        let fname = e.file_name().to_string_lossy().to_string();
        let Some(link) = fname
            .strip_suffix(".json")
            .and_then(|f| f.rsplit_once('.'))
            .map(|(link, _)| link.to_string())
        else {
            continue;
        };
        let rec: Option<Rec> = std::fs::read(e.path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
        let Some(rec) = rec else { continue };
        let Some(a) = &rec.attached else { continue };
        if !rec.start.is_empty() && alive(rec.pid, &rec.start) {
            out.entry(a.run.clone()).or_default().push(AttachedLeader {
                link,
                input: a.input,
            });
        }
    }
    for leaders in out.values_mut() {
        leaders.sort_by(|a, b| (&a.link, a.input).cmp(&(&b.link, b.input)));
        leaders.dedup();
    }
    out
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Terminated {
    pub signalled: usize,
    pub stale: usize,
    pub survivors: usize,
    pub unknown: usize,
}

fn wait_gone(probe: &dyn Fn(u32) -> Probe, pid: u32, start: &str, limit: Duration) -> Life {
    let end = Instant::now() + limit;
    loop {
        let l = life(probe, pid, start);
        if l == Life::Gone || Instant::now() >= end {
            return l;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

pub fn terminate(
    paths: &Paths,
    name: &str,
    grace: Duration,
    select: &dyn Fn(&str) -> bool,
) -> Result<Terminated> {
    terminate_where(paths, name, grace, &|e, _| select(e))
}

const LIST_PASSES: usize = 3;

fn record_files(paths: &Paths, name: &str) -> Result<(Vec<PathBuf>, usize)> {
    let d = dir(paths);
    collect_records(name, &mut || {
        std::fs::read_dir(&d).map(|rd| rd.map(|e| e.map(|e| e.path())).collect())
    })
}

fn collect_records(
    name: &str,
    list: &mut dyn FnMut() -> std::io::Result<Vec<std::io::Result<PathBuf>>>,
) -> Result<(Vec<PathBuf>, usize)> {
    let prefix = format!("{name}.");
    let mut files: Vec<PathBuf> = Vec::new();
    let mut unreadable = 0;
    for pass in 0..LIST_PASSES {
        let entries = match list() {
            Ok(entries) => entries,
            Err(e) if pass == 0 => {
                return match e.kind() {
                    std::io::ErrorKind::NotFound => Ok((Vec::new(), 0)),
                    _ => Err(e.into()),
                }
            }
            Err(_) => {
                unreadable = 1;
                continue;
            }
        };
        unreadable = 0;
        for entry in entries {
            let Ok(path) = entry else {
                unreadable += 1;
                continue;
            };
            let ours = path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .as_deref()
                .and_then(|f| f.strip_prefix(&prefix))
                .and_then(|r| r.strip_suffix(".json"))
                .is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()));
            if ours && !files.contains(&path) {
                files.push(path);
            }
        }
        if unreadable == 0 {
            break;
        }
    }
    Ok((files, unreadable))
}

enum Read {
    Gone,
    Unknown,
    Rec(Rec),
}

fn read_rec(path: &std::path::Path) -> Read {
    match std::fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Read::Gone,
        Err(_) => Read::Unknown,
        Ok(bytes) => match serde_json::from_slice::<Rec>(&bytes) {
            Ok(r) if !r.start.is_empty() => Read::Rec(r),
            _ => Read::Unknown,
        },
    }
}

pub fn any_attached(paths: &Paths, name: &str, select: &dyn Fn(&Attached) -> bool) -> Result<bool> {
    any_attached_with(paths, name, select, &query)
}

fn any_attached_with(
    paths: &Paths,
    name: &str,
    select: &dyn Fn(&Attached) -> bool,
    probe: &dyn Fn(u32) -> Probe,
) -> Result<bool> {
    let (files, unreadable) = record_files(paths, name)?;
    Ok(unreadable > 0
        || files.iter().any(|path| match read_rec(path) {
            Read::Gone => false,
            Read::Unknown => true,
            Read::Rec(r) => {
                r.attached.as_ref().is_some_and(select)
                    && life(probe, r.pid, &r.start) != Life::Gone
            }
        }))
}

pub fn terminate_where(
    paths: &Paths,
    name: &str,
    grace: Duration,
    select: &dyn Fn(&str, Option<&Attached>) -> bool,
) -> Result<Terminated> {
    terminate_where_with(paths, name, grace, select, &query)
}

#[doc(hidden)]
pub fn terminate_where_with(
    paths: &Paths,
    name: &str,
    grace: Duration,
    select: &dyn Fn(&str, Option<&Attached>) -> bool,
    probe: &dyn Fn(u32) -> Probe,
) -> Result<Terminated> {
    let (files, unreadable) = record_files(paths, name)?;
    Ok(terminate_files(files, unreadable, grace, select, probe))
}

fn terminate_files(
    files: Vec<PathBuf>,
    unreadable: usize,
    grace: Duration,
    select: &dyn Fn(&str, Option<&Attached>) -> bool,
    probe: &dyn Fn(u32) -> Probe,
) -> Terminated {
    let mut out = Terminated {
        unknown: unreadable,
        ..Terminated::default()
    };
    let mut live: Vec<(PathBuf, Rec)> = Vec::new();
    for path in files {
        match read_rec(&path) {
            Read::Gone => {}
            Read::Unknown => out.unknown += 1,
            Read::Rec(r) if !select(&r.enrollment, r.attached.as_ref()) => {}
            Read::Rec(r) => match life(probe, r.pid, &r.start) {
                Life::Alive => live.push((path, r)),
                Life::Gone => {
                    out.stale += 1;
                    let _ = std::fs::remove_file(path);
                }
                Life::Unknown => out.unknown += 1,
            },
        }
    }
    for (_, r) in &live {
        if life(probe, r.pid, &r.start) == Life::Alive {
            unsafe { libc::kill(r.pid as libc::pid_t, libc::SIGTERM) };
            out.signalled += 1;
        }
    }
    for (path, r) in &live {
        let mut state = wait_gone(probe, r.pid, &r.start, grace);
        if state == Life::Alive {
            if life(probe, r.pid, &r.start) == Life::Alive {
                unsafe { libc::kill(r.pid as libc::pid_t, libc::SIGKILL) };
            }
            state = wait_gone(probe, r.pid, &r.start, Duration::from_secs(2));
        }
        match state {
            Life::Gone => {
                let _ = std::fs::remove_file(path);
            }
            Life::Alive => out.survivors += 1,
            Life::Unknown => out.unknown += 1,
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::process::{Command, Stdio};

    fn registry_dir(paths: &Paths) {
        ensure_private_dir(&paths.config).unwrap();
        ensure_private_dir(&paths.link_dir()).unwrap();
        ensure_private_dir(&dir(paths)).unwrap();
    }

    #[test]
    fn an_unparsable_record_is_unknown_and_kept() {
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        registry_dir(&paths);
        let bad = file(&paths, "laptop", 4242);
        std::fs::write(&bad, b"not json").unwrap();
        let t = terminate(&paths, "laptop", Duration::from_millis(10), &|_| true).unwrap();
        assert_eq!((t.unknown, t.stale), (1, 0));
        assert!(bad.exists());
        assert!(any_attached(&paths, "laptop", &|_| false).unwrap());
    }

    #[test]
    fn a_record_without_a_start_marker_is_unknown_and_kept() {
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        registry_dir(&paths);
        let rec = file(&paths, "laptop", 4242);
        let body = json!({"pid": 4242, "start": "", "enrollment": "e"}).to_string();
        std::fs::write(&rec, body).unwrap();
        let t = terminate(&paths, "laptop", Duration::from_millis(10), &|_| true).unwrap();
        assert_eq!((t.unknown, t.stale), (1, 0));
        assert!(rec.exists());
    }

    #[test]
    fn a_record_whose_process_is_gone_is_still_stale() {
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        registry_dir(&paths);
        let mut child = Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        let rec = file(&paths, "laptop", pid);
        let body = json!({"pid": pid, "start": "1.1", "enrollment": "e"}).to_string();
        std::fs::write(&rec, body).unwrap();
        let t = terminate(&paths, "laptop", Duration::from_millis(10), &|_| true).unwrap();
        assert_eq!((t.unknown, t.stale), (0, 1));
        assert!(!rec.exists());
    }

    #[test]
    fn a_record_removed_after_listing_is_not_unknown() {
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        registry_dir(&paths);
        assert!(matches!(
            read_rec(&file(&paths, "laptop", 4242)),
            Read::Gone
        ));
    }

    #[test]
    fn own_process_has_a_start_marker() {
        assert!(process_start(std::process::id()).is_some_and(|s| !s.is_empty()));
        assert!(process_start(u32::MAX - 1).is_none());
    }

    fn fake_worker(paths: &Paths, name: &str, child: &std::process::Child, start: String) {
        ensure_private_dir(&paths.config).unwrap();
        ensure_private_dir(&paths.link_dir()).unwrap();
        ensure_private_dir(&dir(paths)).unwrap();
        let rec = Rec {
            pid: child.id(),
            start,
            enrollment: "e".into(),
            attached: None,
        };
        write_atomic(
            &file(paths, name, child.id()),
            &serde_json::to_vec(&rec).unwrap(),
            0o600,
        )
        .unwrap();
    }

    fn sleeper() -> std::process::Child {
        Command::new("sleep")
            .arg("60")
            .stdin(Stdio::null())
            .spawn()
            .unwrap()
    }

    #[test]
    fn terminate_kills_registered_workers_of_that_link_only() {
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        let mut mine = sleeper();
        let mut other = sleeper();
        fake_worker(&paths, "laptop", &mine, process_start(mine.id()).unwrap());
        fake_worker(&paths, "phone", &other, process_start(other.id()).unwrap());
        let reaper = std::thread::spawn(move || {
            let st = mine.wait().unwrap();
            (mine, st)
        });
        let t = terminate(&paths, "laptop", Duration::from_secs(5), &|_| true).unwrap();
        assert_eq!(t.signalled, 1);
        assert_eq!(t.survivors, 0);
        let (_mine, status) = reaper.join().unwrap();
        assert!(!status.success());
        assert!(other.try_wait().unwrap().is_none());
        other.kill().unwrap();
        other.wait().unwrap();
    }

    #[test]
    fn contain_kills_workers_even_when_the_runs_file_is_damaged() {
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        ensure_private_dir(&paths.config).unwrap();
        ensure_private_dir(&paths.link_dir()).unwrap();
        write_atomic(&paths.link_dir().join("runs.json"), b"\x00 not json", 0o600).unwrap();
        let mut worker = sleeper();
        fake_worker(
            &paths,
            "laptop",
            &worker,
            process_start(worker.id()).unwrap(),
        );
        crate::tombstone::record(&paths, "laptop", "e", 1).unwrap();
        let reaper = std::thread::spawn(move || worker.wait().unwrap());
        let out = crate::follower::contain(&paths, "laptop", None, false).unwrap();
        assert!(out.runs_unreadable);
        assert_eq!(out.workers.signalled, 1);
        assert!(!reaper.join().unwrap().success());
    }

    #[test]
    fn only_the_selected_enrollments_are_terminated() {
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        let mut old = sleeper();
        let mut current = sleeper();
        for (child, enrollment) in [(&old, "old"), (&current, "new")] {
            ensure_private_dir(&paths.config).unwrap();
            ensure_private_dir(&paths.link_dir()).unwrap();
            ensure_private_dir(&dir(&paths)).unwrap();
            let rec = Rec {
                pid: child.id(),
                start: process_start(child.id()).unwrap(),
                enrollment: enrollment.into(),
                attached: None,
            };
            write_atomic(
                &file(&paths, "laptop", child.id()),
                &serde_json::to_vec(&rec).unwrap(),
                0o600,
            )
            .unwrap();
        }
        let reaper = std::thread::spawn(move || {
            old.wait().unwrap();
        });
        let t = terminate(&paths, "laptop", Duration::from_secs(5), &|e| e == "old").unwrap();
        assert_eq!(t.signalled, 1);
        reaper.join().unwrap();
        assert!(current.try_wait().unwrap().is_none());
        current.kill().unwrap();
        current.wait().unwrap();
    }

    #[test]
    fn a_recycled_pid_is_never_signalled() {
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        let mut bystander = sleeper();
        fake_worker(&paths, "laptop", &bystander, "not-its-start-time".into());
        let t = terminate(&paths, "laptop", Duration::from_millis(200), &|_| true).unwrap();
        assert_eq!(t.signalled, 0);
        assert_eq!(t.stale, 1);
        assert!(bystander.try_wait().unwrap().is_none());
        bystander.kill().unwrap();
        bystander.wait().unwrap();
        assert!(!file(&paths, "laptop", bystander.id()).exists());
    }

    #[test]
    fn registration_is_removed_on_drop_and_private() {
        use std::os::unix::fs::PermissionsExt;
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        let guard = register(&paths, "laptop", "e").unwrap();
        let p = file(&paths, "laptop", std::process::id());
        assert_eq!(
            std::fs::metadata(&p).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(guard);
        assert!(!p.exists());
    }

    #[test]
    fn the_attached_marker_lists_the_link_and_clears() {
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        assert!(attached_leaders(&paths).is_empty());
        let a = Attached {
            run: "s_abc".into(),
            input: true,
            remote: true,
            bypass: false,
        };
        set_attached(&paths, "laptop", "e", Some(a)).unwrap();
        let got = attached_leaders(&paths);
        assert_eq!(
            got["s_abc"],
            [AttachedLeader {
                link: "laptop".into(),
                input: true
            }]
        );
        set_attached(&paths, "laptop", "e", None).unwrap();
        assert!(attached_leaders(&paths).is_empty());
    }

    #[test]
    fn terminate_where_selects_on_the_attached_record() {
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        let mut plain = sleeper();
        let mut marked = sleeper();
        fake_worker(&paths, "laptop", &plain, process_start(plain.id()).unwrap());
        fake_worker(
            &paths,
            "laptop",
            &marked,
            process_start(marked.id()).unwrap(),
        );
        let rec = Rec {
            pid: marked.id(),
            start: process_start(marked.id()).unwrap(),
            enrollment: "e".into(),
            attached: Some(Attached {
                run: "s_abc".into(),
                input: false,
                remote: true,
                bypass: true,
            }),
        };
        write_atomic(
            &file(&paths, "laptop", marked.id()),
            &serde_json::to_vec(&rec).unwrap(),
            0o600,
        )
        .unwrap();
        let t = terminate_where(&paths, "laptop", Duration::from_secs(5), &|_, a| {
            a.is_some_and(|a| a.remote && a.bypass)
        })
        .unwrap();
        assert_eq!(t.signalled, 1);
        let end = Instant::now() + Duration::from_secs(5);
        while marked.try_wait().unwrap().is_none() && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(marked.try_wait().unwrap().is_some());
        assert!(plain.try_wait().unwrap().is_none());
        plain.kill().unwrap();
        plain.wait().unwrap();
    }

    fn all(_: &str, _: Option<&Attached>) -> bool {
        true
    }

    fn stop(mut child: std::process::Child) {
        child.kill().unwrap();
        child.wait().unwrap();
    }

    fn marked_worker(paths: &Paths, child: &std::process::Child) {
        let rec = Rec {
            pid: child.id(),
            start: process_start(child.id()).unwrap(),
            enrollment: "e".into(),
            attached: Some(Attached {
                run: "s_abc".into(),
                input: false,
                remote: true,
                bypass: false,
            }),
        };
        registry_dir(paths);
        write_atomic(
            &file(paths, "laptop", child.id()),
            &serde_json::to_vec(&rec).unwrap(),
            0o600,
        )
        .unwrap();
    }

    #[test]
    fn a_failed_process_query_keeps_the_record_and_signals_nothing() {
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        let mut worker = sleeper();
        fake_worker(
            &paths,
            "laptop",
            &worker,
            process_start(worker.id()).unwrap(),
        );
        let t = terminate_where_with(&paths, "laptop", Duration::from_millis(50), &all, &|_| {
            Probe::Unknown
        })
        .unwrap();
        assert_eq!((t.unknown, t.stale, t.signalled), (1, 0, 0));
        assert!(file(&paths, "laptop", worker.id()).exists());
        assert!(worker.try_wait().unwrap().is_none());
        stop(worker);
    }

    #[test]
    fn a_query_that_starts_failing_midway_is_unknown_not_a_survivor() {
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        let mut worker = sleeper();
        let start = process_start(worker.id()).unwrap();
        fake_worker(&paths, "laptop", &worker, start.clone());
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let probe = |_: u32| {
            if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                Probe::Running {
                    start: start.clone(),
                    zombie: false,
                }
            } else {
                Probe::Unknown
            }
        };
        let t = terminate_where_with(&paths, "laptop", Duration::from_millis(100), &all, &probe)
            .unwrap();
        assert_eq!((t.signalled, t.unknown, t.survivors, t.stale), (0, 1, 0, 0));
        assert!(file(&paths, "laptop", worker.id()).exists());
        assert!(worker.try_wait().unwrap().is_none());
        stop(worker);
    }

    #[test]
    fn an_unknown_attached_record_counts_as_attached_when_selected() {
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        let mut worker = sleeper();
        marked_worker(&paths, &worker);
        let unknown = |_: u32| Probe::Unknown;
        let gone = |_: u32| Probe::Gone;
        assert!(any_attached_with(&paths, "laptop", &|_| true, &unknown).unwrap());
        assert!(!any_attached_with(&paths, "laptop", &|_| false, &unknown).unwrap());
        assert!(!any_attached_with(&paths, "laptop", &|_| true, &gone).unwrap());
        assert!(any_attached_with(&paths, "laptop", &|_| true, &query).unwrap());
        worker.kill().unwrap();
        worker.wait().unwrap();
    }

    #[test]
    fn after_a_failed_query_only_a_missing_process_is_gone() {
        let mut child = Command::new("true").spawn().unwrap();
        let pid = child.id();
        child.wait().unwrap();
        assert_eq!(after_failed_query(pid), Probe::Gone);
        assert_eq!(after_failed_query(std::process::id()), Probe::Unknown);
        assert_eq!(after_failed_query(0), Probe::Unknown);
        assert_eq!(after_failed_query(u32::MAX - 1), Probe::Unknown);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_live_process_of_another_user_is_unknown_not_gone() {
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        assert_eq!(query(1), Probe::Unknown);
    }

    #[test]
    fn an_exited_unreaped_process_reads_as_gone() {
        let mut child = Command::new("true").spawn().unwrap();
        let start = process_start(child.id());
        let end = Instant::now() + Duration::from_secs(5);
        let state = loop {
            let p = query(child.id());
            if !matches!(p, Probe::Running { zombie: false, .. }) || Instant::now() >= end {
                break p;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        assert!(matches!(state, Probe::Running { zombie: true, .. }));
        let alive_now = life(&query, child.id(), &start.unwrap());
        assert!(alive_now == Life::Gone);
        child.wait().unwrap();
    }

    #[test]
    fn an_unparsable_record_does_not_stop_the_live_worker_being_signalled() {
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        let worker = sleeper();
        fake_worker(
            &paths,
            "laptop",
            &worker,
            process_start(worker.id()).unwrap(),
        );
        let bad = file(&paths, "laptop", 4242);
        std::fs::write(&bad, b"not json").unwrap();
        let mut worker = worker;
        let reaper = std::thread::spawn(move || worker.wait().unwrap());
        let t = terminate(&paths, "laptop", Duration::from_secs(5), &|_| true).unwrap();
        assert_eq!((t.signalled, t.unknown, t.survivors), (1, 1, 0));
        assert!(!reaper.join().unwrap().success());
        assert!(bad.exists());
    }

    #[test]
    fn a_worker_with_a_failed_query_does_not_stop_a_live_one_being_signalled() {
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        let mut live = sleeper();
        let other = sleeper();
        let other_pid = other.id();
        fake_worker(&paths, "laptop", &live, process_start(live.id()).unwrap());
        fake_worker(&paths, "laptop", &other, process_start(other_pid).unwrap());
        let reaper = std::thread::spawn(move || live.wait().unwrap());
        let probe = |pid: u32| {
            if pid == other_pid {
                Probe::Unknown
            } else {
                query(pid)
            }
        };
        let t =
            terminate_where_with(&paths, "laptop", Duration::from_secs(5), &all, &probe).unwrap();
        assert_eq!((t.signalled, t.unknown, t.survivors), (1, 1, 0));
        assert!(!reaper.join().unwrap().success());
        assert!(file(&paths, "laptop", other_pid).exists());
        stop(other);
    }

    #[test]
    fn containment_stays_incomplete_while_a_record_is_unknown_but_still_stops_the_worker() {
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        let mut worker = sleeper();
        fake_worker(
            &paths,
            "laptop",
            &worker,
            process_start(worker.id()).unwrap(),
        );
        let bad = file(&paths, "laptop", 4242);
        std::fs::write(&bad, b"not json").unwrap();
        crate::tombstone::record(&paths, "laptop", "e", 1).unwrap();
        let reaper = std::thread::spawn(move || worker.wait().unwrap());
        let out = crate::follower::contain(&paths, "laptop", None, false).unwrap();
        assert_eq!((out.workers.signalled, out.workers.unknown), (1, 1));
        assert!(out.incomplete());
        assert!(!reaper.join().unwrap().success());
        assert!(!crate::tombstone::for_link(&paths, "laptop")
            .unwrap()
            .is_empty());
    }

    fn pass(
        items: Vec<std::io::Result<PathBuf>>,
    ) -> std::io::Result<Vec<std::io::Result<PathBuf>>> {
        Ok(items)
    }

    fn rec_path(pid: u32) -> PathBuf {
        PathBuf::from(format!("/w/laptop.{pid}.json"))
    }

    fn broken() -> std::io::Result<PathBuf> {
        Err(std::io::Error::other("entry"))
    }

    #[test]
    fn a_listing_cut_short_by_an_entry_error_is_repeated_and_merged() {
        let mut passes = vec![
            pass(vec![Ok(rec_path(1)), broken()]),
            pass(vec![Ok(rec_path(1)), Ok(rec_path(2))]),
        ]
        .into_iter();
        let (files, unreadable) =
            collect_records("laptop", &mut || passes.next().unwrap()).unwrap();
        assert_eq!(files, [rec_path(1), rec_path(2)]);
        assert_eq!(unreadable, 0);
    }

    #[test]
    fn a_listing_that_keeps_failing_is_bounded_and_reports_the_errors() {
        let mut calls = 0;
        let (files, unreadable) = collect_records("laptop", &mut || {
            calls += 1;
            pass(vec![Ok(rec_path(1)), broken()])
        })
        .unwrap();
        assert_eq!(calls, 3);
        assert_eq!(files, [rec_path(1)]);
        assert_eq!(unreadable, 1);
    }

    #[test]
    fn a_failed_reopen_counts_as_one_unreadable_entry() {
        let mut passes = vec![
            pass(vec![Ok(rec_path(1)), broken()]),
            Err(std::io::Error::other("reopen")),
            Err(std::io::Error::other("reopen")),
        ]
        .into_iter();
        let (files, unreadable) =
            collect_records("laptop", &mut || passes.next().unwrap()).unwrap();
        assert_eq!((files, unreadable), (vec![rec_path(1)], 1));
    }

    #[test]
    fn listing_filters_by_link_and_stops_at_the_first_clean_pass() {
        let mut calls = 0;
        let (files, unreadable) = collect_records("laptop", &mut || {
            calls += 1;
            pass(vec![
                Ok(rec_path(7)),
                Ok(PathBuf::from("/w/phone.8.json")),
                Ok(PathBuf::from("/w/laptop.x.json")),
                Ok(PathBuf::from("/w/laptop.9.txt")),
            ])
        })
        .unwrap();
        assert_eq!((calls, unreadable), (1, 0));
        assert_eq!(files, [rec_path(7)]);
    }

    #[test]
    fn a_missing_directory_lists_empty_and_another_open_error_fails() {
        let missing = collect_records("laptop", &mut || {
            Err(std::io::Error::from(std::io::ErrorKind::NotFound))
        })
        .unwrap();
        assert_eq!(missing, (Vec::new(), 0));
        assert!(collect_records("laptop", &mut || {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        })
        .is_err());
    }

    #[test]
    fn a_record_listed_only_after_an_entry_error_is_still_terminated() {
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        let worker = sleeper();
        fake_worker(
            &paths,
            "laptop",
            &worker,
            process_start(worker.id()).unwrap(),
        );
        let target = file(&paths, "laptop", worker.id());
        let mut passes = vec![pass(vec![broken()]), pass(vec![Ok(target.clone())])].into_iter();
        let (files, unreadable) =
            collect_records("laptop", &mut || passes.next().unwrap()).unwrap();
        let mut worker = worker;
        let reaper = std::thread::spawn(move || worker.wait().unwrap());
        let t = terminate_files(files, unreadable, Duration::from_secs(5), &all, &query);
        assert_eq!((t.signalled, t.unknown, t.survivors), (1, 0, 0));
        assert!(!reaper.join().unwrap().success());
        assert!(!target.exists());
    }
}
