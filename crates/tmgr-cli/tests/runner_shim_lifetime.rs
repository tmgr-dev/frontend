#![cfg(feature = "runner")]

mod common;

use common::{path_with, Env};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
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

fn cwd_of(pid: i32) -> String {
    #[cfg(target_os = "macos")]
    {
        let out = std::process::Command::new("/usr/sbin/lsof")
            .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        let found = text.lines().find_map(|l| l.strip_prefix('n'));
        found.unwrap().to_string()
    }
    #[cfg(not(target_os = "macos"))]
    std::fs::read_link(format!("/proc/{pid}/cwd"))
        .unwrap()
        .display()
        .to_string()
}

fn wait_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + limit;
    while Instant::now() < end {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    done()
}

struct Fx {
    env: Env,
    _work: tempfile::TempDir,
    root: PathBuf,
    runner: PathBuf,
    shim: i32,
}

impl Fx {
    async fn start(agent: &str) -> Fx {
        let env = Env::new().await;
        let work = tempfile::Builder::new()
            .prefix("slt")
            .tempdir_in("/tmp")
            .unwrap();
        let root = std::fs::canonicalize(work.path()).unwrap();
        let runner = root.join("run/r");
        let cwd = root.join("code");
        std::fs::create_dir_all(&cwd).unwrap();
        let status = env
            .std_cmd()
            .env("PATH", path_with(None))
            .env("TMGR_RUNNER_DIR", &runner)
            .env("TMGR_RUNNER_HOMES", root.join("h"))
            .args(["run", "--agent-cmd", agent])
            .arg("--cwd")
            .arg(&cwd)
            .arg("--no-attach")
            .stdout(Stdio::null())
            .stderr(std::fs::File::create(root.join("run.err")).unwrap())
            .status()
            .unwrap();
        let mut fx = Fx {
            env,
            _work: work,
            root,
            runner,
            shim: 0,
        };
        let mut pid = None;
        wait_until(Duration::from_secs(15), || {
            pid = find_shim_pid(&fx.runner)
                .and_then(|p| std::fs::read_to_string(p).ok())
                .and_then(|s| s.trim().parse::<i32>().ok());
            pid.is_some()
        });
        let Some(pid) = pid else {
            fx.stop();
            let err = std::fs::read_to_string(fx.root.join("run.err")).unwrap_or_default();
            panic!("the runner could not start a session ({status:?}): {err}");
        };
        fx.shim = pid;
        fx
    }

    fn stop(&self) {
        let Ok(mut child) = self
            .env
            .std_cmd()
            .env("PATH", path_with(None))
            .env("TMGR_RUNNER_DIR", &self.runner)
            .env("TMGR_RUNNER_HOMES", self.root.join("h"))
            .args(["runner", "stop"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return;
        };
        let limit = Instant::now() + Duration::from_secs(30);
        while child.try_wait().ok().flatten().is_none() && Instant::now() < limit {
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
    }

    fn remove_run_dir(&self) {
        std::fs::remove_dir_all(self.root.join("run")).unwrap();
    }
}

impl Drop for Fx {
    fn drop(&mut self) {
        let _ = std::fs::set_permissions(
            self.root.join("run"),
            std::fs::Permissions::from_mode(0o755),
        );
        self.stop();
        if self.shim > 1 {
            unsafe {
                let pgid = libc::getpgid(self.shim);
                if pgid > 1 && pgid != libc::getpgrp() {
                    libc::killpg(pgid, libc::SIGKILL);
                }
                libc::kill(self.shim, libc::SIGKILL);
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_finished_session_shim_exits_once_the_run_dir_is_removed() {
    let fx = Fx::start("/usr/bin/true").await;
    fx.stop();
    fx.remove_run_dir();
    assert!(
        wait_until(Duration::from_secs(10), || !alive(fx.shim)),
        "the shim of a finished session outlived its removed run dir"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_shim_does_not_stay_in_the_folder_it_launched_the_agent_in() {
    let fx = Fx::start("/bin/sleep 5").await;
    let cwd = cwd_of(fx.shim);
    let folder = fx.root.join("code");
    assert!(
        !Path::new(&cwd).starts_with(&folder),
        "the shim works in {cwd}, so `tmgr rm` can see it as using the folder"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_finished_session_shim_stays_while_the_run_dir_exists() {
    let fx = Fx::start("/usr/bin/true").await;
    fx.stop();
    std::thread::sleep(Duration::from_secs(3));
    assert!(alive(fx.shim), "the shim left although the run dir exists");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_running_agent_keeps_its_shim_when_the_run_dir_is_removed() {
    let fx = Fx::start("/bin/sleep 5").await;
    let sdir = find_shim_pid(&fx.runner)
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(sdir.join("meta.json")).unwrap()).unwrap();
    let agent = meta["child_pid"].as_i64().unwrap() as i32;
    let log = std::fs::read_to_string(sdir.join("output.log")).unwrap_or_default();
    assert!(alive(agent), "the agent is not running: {log}");
    fx.stop();
    fx.remove_run_dir();
    std::thread::sleep(Duration::from_secs(3));
    let (shim_up, agent_up) = (alive(fx.shim), alive(agent));
    assert!(shim_up, "the shim left while its agent was running");
    assert!(
        agent_up,
        "the agent was killed while its run dir was removed"
    );
    assert!(
        wait_until(Duration::from_secs(15), || !alive(fx.shim)),
        "the shim stayed after the agent exited and the run dir was gone"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unreadable_run_dir_does_not_end_the_shim() {
    assert_ne!(
        unsafe { libc::geteuid() },
        0,
        "this test needs a non-root user"
    );
    let fx = Fx::start("/usr/bin/true").await;
    fx.stop();
    let parent = fx.root.join("run");
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o000)).unwrap();
    std::thread::sleep(Duration::from_secs(3));
    let up = alive(fx.shim);
    std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        up,
        "the shim left on a permission error instead of NotFound"
    );
}
