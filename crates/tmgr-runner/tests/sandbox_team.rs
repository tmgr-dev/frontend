#![cfg(any(target_os = "macos", target_os = "linux"))]
mod common;

use common::*;
use std::fs;
use std::path::PathBuf;

#[cfg(target_os = "linux")]
const PLATFORM: &str = "linux";
#[cfg(target_os = "macos")]
const PLATFORM: &str = "macos";

#[cfg(target_os = "linux")]
fn self_exe() -> &'static str {
    env!("CARGO_BIN_EXE_tmgr-sandbox-probe")
}

#[cfg(target_os = "macos")]
fn self_exe() -> &'static str {
    "/usr/bin/true"
}

const SIBLING_MARK: &str = "sibling-copy-content-4711";

struct Team {
    f: Fx,
    repo: PathBuf,
    pickup: PathBuf,
    outside: PathBuf,
}

fn team() -> Team {
    let mut f = fixture(self_exe());
    let repo = f.root.join("repo");
    let pickup = f.root.join("repo-worktrees/queue/p_1");
    let outside = f.root.join("elsewhere");
    for d in [
        repo.clone(),
        pickup.join("lead"),
        pickup.join("w1"),
        pickup.join("w2"),
        outside.clone(),
    ] {
        fs::create_dir_all(d).unwrap();
    }
    fs::write(pickup.join("w2/theirs.txt"), SIBLING_MARK).unwrap();
    f.cwd = pickup.join("w1");
    Team {
        f,
        repo,
        pickup,
        outside,
    }
}

fn usable(f: &Fx) -> bool {
    let o = f.run("exit 0");
    if o.code == Some(78) {
        eprintln!("SKIPPED: {}", o.stderr);
        return false;
    }
    assert!(o.ok(), "{} {}", o.stdout, o.stderr);
    true
}

#[test]
fn a_worker_writes_only_its_own_copy() {
    let t = team();
    if !usable(&t.f) {
        return;
    }
    let targets = [
        t.pickup.join("w2/x"),
        t.pickup.join("lead/x"),
        t.pickup.join("x"),
        t.pickup.parent().unwrap().join("x"),
        t.repo.join("x"),
        t.outside.join("x"),
        t.f.root.join("x"),
    ];
    for target in &targets {
        let o = t.f.run(&format!("echo x > {}", t.f.path(target)));
        assert!(!o.ok(), "{} was written", target.display());
        assert!(!target.exists(), "{} exists", target.display());
    }
    for rel in ["../w2/y", "../lead/y", "../y"] {
        let o = t.f.run(&format!("echo x > {rel}"));
        assert!(!o.ok(), "{rel} was written");
    }
    assert!(!t.pickup.join("w2/y").exists());
    assert!(!t.pickup.join("lead/y").exists());
    assert!(!t.pickup.join("y").exists());
    let o = t.f.run("echo mine > own.txt && echo h > $HOME/h.txt");
    assert!(o.ok(), "{}", o.stderr);
    assert_eq!(
        fs::read_to_string(t.pickup.join("w1/own.txt")).unwrap(),
        "mine\n"
    );
    assert!(t.f.home.join("h.txt").is_file());
}

#[test]
fn measure_whether_a_sibling_copy_can_be_read() {
    let t = team();
    if !usable(&t.f) {
        return;
    }
    let o = t.f.run("cat ../w2/theirs.txt");
    let verdict = if o.ok() && o.stdout.contains(SIBLING_MARK) {
        "allowed"
    } else {
        "denied"
    };
    let line = format!("TM606_SIBLING_READ {PLATFORM} {verdict}");
    eprintln!("{line}");
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("tm606_sibling_read.txt"), format!("{line}\n")).unwrap();
}
