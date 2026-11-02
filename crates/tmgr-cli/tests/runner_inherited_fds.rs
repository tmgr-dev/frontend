#![cfg(feature = "runner")]

mod common;

use common::{path_with, write_script, Env};
use std::io::Read;
use std::os::fd::FromRawFd;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::mpsc;
use std::time::{Duration, Instant};

fn find_shim_pid(dir: &Path) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(found) = find_shim_pid(&p) {
                return Some(found);
            }
        } else if p.file_name().is_some_and(|n| n == "shim.pid") {
            return Some(p);
        }
    }
    None
}

fn alive(pid: i32) -> bool {
    unsafe { libc::kill(pid, 0) == 0 }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn runner_processes_do_not_keep_inherited_fds() {
    let env = Env::new().await;
    let work = tempfile::Builder::new()
        .prefix("ifd")
        .tempdir_in("/tmp")
        .unwrap();
    let root = std::fs::canonicalize(work.path()).unwrap();
    let runner = root.join("r");
    let cwd = root.join("code");
    std::fs::create_dir_all(&cwd).unwrap();
    let script = write_script(&root.join("agent"), "agent.sh", "exec /bin/sleep 300");

    let mut fds = [0 as libc::c_int; 2];
    assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
    let [read_fd, write_fd] = fds;
    unsafe {
        libc::fcntl(read_fd, libc::F_SETFD, libc::FD_CLOEXEC);
        libc::fcntl(write_fd, libc::F_SETFD, libc::FD_CLOEXEC);
    }

    let mut cmd = env.std_cmd();
    cmd.env("PATH", path_with(None))
        .env("TMGR_RUNNER_DIR", &runner)
        .env("TMGR_RUNNER_HOMES", root.join("h"))
        .args(["run", "--agent-cmd"])
        .arg(&script)
        .arg("--cwd")
        .arg(&cwd)
        .arg("--no-attach")
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(root.join("run.err")).unwrap());
    unsafe {
        cmd.pre_exec(move || {
            libc::fcntl(write_fd, libc::F_SETFD, 0);
            Ok(())
        });
    }
    let started = cmd.status().unwrap();
    unsafe { libc::close(write_fd) };

    let stop = |env: &Env| {
        let _ = env
            .std_cmd()
            .env("PATH", path_with(None))
            .env("TMGR_RUNNER_DIR", &runner)
            .env("TMGR_RUNNER_HOMES", root.join("h"))
            .args(["runner", "stop"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    };

    let deadline = Instant::now() + Duration::from_secs(15);
    let shim = loop {
        if let Some(pid) = find_shim_pid(&runner)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| s.trim().parse::<i32>().ok())
        {
            break Some(pid);
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(Duration::from_millis(100));
    };

    let Some(shim) = shim else {
        stop(&env);
        unsafe { libc::close(read_fd) };
        let err = std::fs::read_to_string(root.join("run.err")).unwrap_or_default();
        panic!("the runner could not start a session ({started:?}): {err}");
    };
    std::thread::sleep(Duration::from_millis(800));
    if !alive(shim) {
        stop(&env);
        unsafe { libc::close(read_fd) };
        let err = std::fs::read_to_string(root.join("run.err")).unwrap_or_default();
        panic!("the session shim exited early ({started:?}): {err}");
    }

    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut pipe = unsafe { std::fs::File::from_raw_fd(read_fd) };
        let mut b = Vec::new();
        let _ = tx.send(pipe.read_to_end(&mut b).map(|_| b.len()));
    });
    let eof = rx.recv_timeout(Duration::from_secs(5));
    let shim_alive = alive(shim);

    stop(&env);
    unsafe {
        let pgid = libc::getpgid(shim);
        if pgid > 1 && pgid != libc::getpgrp() {
            libc::killpg(pgid, libc::SIGKILL);
        }
        libc::kill(shim, libc::SIGKILL);
    }

    assert!(
        shim_alive,
        "the shim must still be running during the check"
    );
    assert!(
        eof.is_ok(),
        "the test's pipe never reached EOF: a runner process still holds an inherited write end"
    );
}
