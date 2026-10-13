use crate::store::Paths;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tmgr_core::config::{ensure_private_dir, write_atomic};
use tmgr_core::error::Result;

#[derive(Debug, Serialize, Deserialize)]
struct Rec {
    pid: u32,
    start: String,
    enrollment: String,
}

fn dir(paths: &Paths) -> PathBuf {
    paths.link_dir().join("workers")
}

fn file(paths: &Paths, name: &str, pid: u32) -> PathBuf {
    dir(paths).join(format!("{name}.{pid}.json"))
}

#[cfg(target_os = "linux")]
fn process_state(pid: u32) -> Option<(String, bool)> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = &text[text.rfind(')')? + 2..];
    let fields: Vec<&str> = rest.split(' ').collect();
    Some((fields.get(19)?.to_string(), fields.first() == Some(&"Z")))
}

#[cfg(target_os = "macos")]
fn process_state(pid: u32) -> Option<(String, bool)> {
    let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_bsdinfo>() as libc::c_int;
    let got = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTBSDINFO,
            0,
            &mut info as *mut _ as *mut libc::c_void,
            size,
        )
    };
    if got != size {
        return None;
    }
    Some((
        format!("{}.{}", info.pbi_start_tvsec, info.pbi_start_tvusec),
        info.pbi_status == 5,
    ))
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn process_state(_pid: u32) -> Option<(String, bool)> {
    None
}

pub fn process_start(pid: u32) -> Option<String> {
    process_state(pid).map(|(start, _)| start)
}

fn alive(pid: u32, start: &str) -> bool {
    matches!(process_state(pid), Some((s, false)) if s == start)
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
    };
    let path = file(paths, name, pid);
    let bytes = serde_json::to_vec(&rec)
        .map_err(|_| tmgr_core::error::Error::internal("cannot serialize a worker record"))?;
    write_atomic(&path, &bytes, 0o600)?;
    Ok(WorkerGuard { path })
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Terminated {
    pub signalled: usize,
    pub stale: usize,
    pub survivors: usize,
}

fn wait_gone(pid: u32, start: &str, limit: Duration) -> bool {
    let end = Instant::now() + limit;
    while Instant::now() < end {
        if !alive(pid, start) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    !alive(pid, start)
}

pub fn terminate(
    paths: &Paths,
    name: &str,
    grace: Duration,
    select: &dyn Fn(&str) -> bool,
) -> Result<Terminated> {
    let mut out = Terminated::default();
    let Ok(entries) = std::fs::read_dir(dir(paths)) else {
        return Ok(out);
    };
    let prefix = format!("{name}.");
    let mut live: Vec<(PathBuf, Rec)> = Vec::new();
    for e in entries.flatten() {
        let fname = e.file_name().to_string_lossy().to_string();
        let ours = fname
            .strip_prefix(&prefix)
            .and_then(|r| r.strip_suffix(".json"))
            .is_some_and(|pid| !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()));
        if !ours {
            continue;
        }
        let rec: Option<Rec> = std::fs::read(e.path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
        if rec.as_ref().is_some_and(|r| !select(&r.enrollment)) {
            continue;
        }
        match rec {
            Some(r) if !r.start.is_empty() && alive(r.pid, &r.start) => live.push((e.path(), r)),
            _ => {
                out.stale += 1;
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    for (_, r) in &live {
        if alive(r.pid, &r.start) {
            unsafe { libc::kill(r.pid as libc::pid_t, libc::SIGTERM) };
            out.signalled += 1;
        }
    }
    for (path, r) in &live {
        if !wait_gone(r.pid, &r.start, grace) {
            if alive(r.pid, &r.start) {
                unsafe { libc::kill(r.pid as libc::pid_t, libc::SIGKILL) };
            }
            if !wait_gone(r.pid, &r.start, Duration::from_secs(2)) {
                out.survivors += 1;
                continue;
            }
        }
        let _ = std::fs::remove_file(path);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::{Command, Stdio};

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
        let d = tempfile::tempdir().unwrap();
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
        let d = tempfile::tempdir().unwrap();
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
        let d = tempfile::tempdir().unwrap();
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
        let d = tempfile::tempdir().unwrap();
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
        let d = tempfile::tempdir().unwrap();
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
}
