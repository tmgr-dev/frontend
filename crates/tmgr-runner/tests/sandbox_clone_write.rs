#![cfg(any(target_os = "macos", target_os = "linux"))]
mod common;

use common::*;
use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(target_os = "linux")]
fn self_exe() -> &'static str {
    env!("CARGO_BIN_EXE_tmgr-sandbox-probe")
}

#[cfg(target_os = "macos")]
fn self_exe() -> &'static str {
    "/usr/bin/true"
}

const MARK: &str = "tm786-tamper";

fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn emit(case: &str, fields: &str) {
    let _ = std::io::Write::write_all(
        &mut std::io::stderr(),
        format!("TM786_WRITE platform={} case={case} {fields}\n", platform()).as_bytes(),
    );
}

fn git(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git")
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(args)
        .output()
        .unwrap();
    assert!(o.status.success(), "{args:?}: {o:?}");
    String::from_utf8_lossy(&o.stdout).trim().to_string()
}

fn object_files(objects: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut todo = vec![objects.to_path_buf()];
    while let Some(d) = todo.pop() {
        for e in fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                todo.push(p);
            } else if !p.starts_with(objects.join("info")) {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn source_repo(f: &Fx) -> PathBuf {
    let repo = f.root.join("source");
    fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    fs::write(repo.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "a"]);
    git(&repo, &["gc", "-q"]);
    fs::write(repo.join("b.txt"), "b\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "b"]);
    repo
}

fn usable(case: &str, f: &Fx) -> bool {
    let o = f.run("exit 0");
    if o.code == Some(78) {
        skip_without_landlock(&o.stderr);
    }
    if !o.ok() {
        emit(case, &format!("skipped=1 sandbox_exit={:?}", o.code));
    }
    o.ok()
}

fn unlock(files: &[PathBuf]) {
    for p in files {
        fs::set_permissions(p, fs::Permissions::from_mode(0o644)).unwrap();
    }
}

fn inodes(files: &[PathBuf]) -> HashSet<(u64, u64)> {
    files
        .iter()
        .map(|p| {
            let m = p.metadata().unwrap();
            (m.dev(), m.ino())
        })
        .collect()
}

fn tampered(path: &Path) -> bool {
    fs::read(path)
        .map(|b| b.windows(MARK.len()).any(|w| w == MARK.as_bytes()))
        .unwrap_or(false)
}

fn any_tampered(objects: &Path) -> usize {
    object_files(objects).iter().filter(|p| tampered(p)).count()
}

fn out_line(o: &Out, key: &str) -> String {
    o.stdout
        .lines()
        .find_map(|l| l.strip_prefix(&format!("{key}=")))
        .unwrap_or("none")
        .replace(' ', "_")
}

fn append(target: &Path, key: &str) -> String {
    format!("echo {MARK} >> '{}'; echo \"{key}=$?\";", target.display())
}

fn control() -> &'static str {
    "echo ctl > ./tm786-ctl; echo \"ctl=$?\";"
}

fn run(f: &Fx, objects: Option<&Path>, script: &str) -> Out {
    match objects {
        Some(o) => f.run_with_objects(o, script),
        None => f.run(script),
    }
}

fn measure_clone_write(case: &str, f: &Fx, source_objects: &Path, objects: Option<&Path>) {
    let clone_objects = f.cwd.join(".git/objects");
    let files = object_files(&clone_objects);
    let source_files = object_files(source_objects);
    unlock(&files);
    unlock(&source_files);
    let source_inodes = inodes(&source_files);
    let shared = files
        .iter()
        .filter(|p| {
            let m = p.metadata().unwrap();
            source_inodes.contains(&(m.dev(), m.ino()))
        })
        .count();
    let target = files
        .iter()
        .find(|p| {
            let m = p.metadata().unwrap();
            source_inodes.contains(&(m.dev(), m.ino()))
        })
        .or(files.first())
        .unwrap()
        .clone();
    let o = run(
        f,
        objects,
        &format!("{}{}", control(), append(&target, "write")),
    );
    emit(
        case,
        &format!(
            "sandbox_exit={:?} clone_files={} shared_inodes={shared} ctl_rc={} write_rc={} clone_tampered={} source_tampered={}",
            o.code,
            files.len(),
            out_line(&o, "ctl"),
            out_line(&o, "write"),
            any_tampered(&clone_objects),
            any_tampered(source_objects),
        ),
    );
}

#[test]
fn measure_prefix_clone_object_write() {
    let f = fixture(self_exe());
    if !usable("prefix_clone_write", &f) {
        return;
    }
    let repo = source_repo(&f);
    fs::remove_dir(&f.cwd).unwrap();
    git(
        &f.root,
        &[
            "clone",
            "-q",
            "--reference",
            repo.to_str().unwrap(),
            repo.to_str().unwrap(),
            f.cwd.to_str().unwrap(),
        ],
    );
    let source_objects = repo.join(".git/objects");
    measure_clone_write(
        "prefix_clone_write",
        &f,
        &source_objects,
        Some(&source_objects),
    );
}

#[test]
fn measure_fixed_clone_object_write() {
    let f = fixture(self_exe());
    if !usable("fixed_clone_write", &f) {
        return;
    }
    f.self_contained_clone();
    let origin_objects = f.root.join("origin/.git/objects");
    measure_clone_write("fixed_clone_write", &f, &origin_objects, None);
}

#[test]
fn measure_source_objects_write_through_alternates() {
    let f = fixture(self_exe());
    if !usable("alternates_write", &f) {
        return;
    }
    let objects = f.borrowing_clone();
    let alt = fs::read_to_string(f.cwd.join(".git/objects/info/alternates")).unwrap();
    let alt = PathBuf::from(alt.trim());
    let files = object_files(&alt);
    unlock(&files);
    let existing = files[0].clone();
    let mode_before = existing.metadata().unwrap().mode();
    let sdir = f.runner_dir.join("sessions").join(SID);
    if cfg!(target_os = "linux") {
        fs::create_dir_all(&sdir).unwrap();
        tmgr_runner::objectmodes::save(&sdir, std::slice::from_ref(&alt)).unwrap();
    }
    let fresh = alt.join("tm786-new-file");
    let script = format!(
        "{}echo {MARK} > '{}'; echo \"create=$?\"; {}chmod a+x '{}'; echo \"chmod=$?\"; mkdir '{}/tm786-dir'; echo \"mkdir=$?\"; ln '{}' ./tm786-link; echo \"link=$?\"; [ -e ./tm786-link ] && {} true",
        control(),
        fresh.display(),
        append(&existing, "append"),
        existing.display(),
        alt.display(),
        existing.display(),
        append(Path::new("./tm786-link"), "linkwrite"),
    );
    let o = f.run_with_objects(&objects, &script);
    let mode_changed = existing.metadata().unwrap().mode() != mode_before;
    emit(
        "alternates_write",
        &format!(
            "sandbox_exit={:?} ctl_rc={} create_rc={} append_rc={} chmod_rc={} mkdir_rc={} link_rc={} linkwrite_rc={} source_new_file={} source_tampered={} source_mode_changed={} source_new_dir={} link_shares_source_inode={}",
            o.code,
            out_line(&o, "ctl"),
            out_line(&o, "create"),
            out_line(&o, "append"),
            out_line(&o, "chmod"),
            out_line(&o, "mkdir"),
            out_line(&o, "link"),
            out_line(&o, "linkwrite"),
            fresh.exists(),
            any_tampered(&alt),
            mode_changed,
            alt.join("tm786-dir").exists(),
            f.cwd
                .join("tm786-link")
                .metadata()
                .map(|m| m.ino() == existing.metadata().unwrap().ino())
                .unwrap_or(false),
        ),
    );
    assert_eq!(out_line(&o, "ctl"), "0", "{} {}", o.stdout, o.stderr);
    assert!(
        out_line(&o, "chmod").parse::<i32>().is_ok(),
        "{} {}",
        o.stdout,
        o.stderr
    );
    assert_source_mode_kept(&sdir, &o, mode_changed);
    assert_eq!(existing.metadata().unwrap().mode(), mode_before);
}

#[cfg(target_os = "macos")]
fn assert_source_mode_kept(_: &Path, o: &Out, mode_changed: bool) {
    assert_ne!(out_line(o, "chmod"), "0", "{} {}", o.stdout, o.stderr);
    assert!(!mode_changed, "{} {}", o.stdout, o.stderr);
}

#[cfg(target_os = "linux")]
fn assert_source_mode_kept(sdir: &Path, o: &Out, mode_changed: bool) {
    let record = fs::read_to_string(sdir.join("objects_ro")).unwrap();
    if record == "namespace" {
        assert_ne!(out_line(o, "chmod"), "0", "{} {}", o.stdout, o.stderr);
        assert!(!mode_changed, "{} {}", o.stdout, o.stderr);
    } else {
        assert!(record.starts_with("fallback: "), "{record}");
        assert!(
            std::env::var("TMGR_TEST_REQUIRE_RO_MOUNT").as_deref() != Ok("1"),
            "TMGR_TEST_REQUIRE_RO_MOUNT=1 requires the read-only mount: {record}"
        );
        eprintln!("NAMESPACE PATH NOT TESTED: {record}");
    }
    tmgr_runner::objectmodes::finish(sdir);
}

#[test]
fn measure_git_env_write_to_source() {
    let f = fixture(self_exe());
    if !usable("git_env_write", &f) {
        return;
    }
    let objects = f.borrowing_clone();
    let source_git = objects.parent().unwrap().to_path_buf();
    let source_work = source_git.parent().unwrap().to_path_buf();
    let sha_of = |body: &str| {
        use std::io::Write;
        let mut c = Command::new("git")
            .current_dir(&source_work)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args(["hash-object", "--stdin"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        c.stdin.take().unwrap().write_all(body.as_bytes()).unwrap();
        let o = c.wait_with_output().unwrap();
        assert!(o.status.success());
        let sha = String::from_utf8_lossy(&o.stdout).trim().to_string();
        assert_eq!(sha.len(), 40);
        sha
    };
    for (case, salt, prefix) in [
        (
            "git_object_directory_write",
            "objdir",
            format!("GIT_OBJECT_DIRECTORY='{}'", objects.display()),
        ),
        (
            "git_dir_write",
            "gitdir",
            format!("GIT_DIR='{}'", source_git.display()),
        ),
    ] {
        let body = format!("{MARK}-{salt}");
        let sha = sha_of(&body);
        let script = format!(
            "{}printf '%s' '{body}' | git hash-object -w --stdin >/dev/null 2>./tm786-err; echo \"git_ctl=$?\"; printf '%s' '{body}' | env {prefix} git hash-object -w --stdin 2>./tm786-err >/dev/null; echo \"write=$?\"; echo \"err=$(head -n 1 ./tm786-err)\"",
            control(),
        );
        let o = f.run_with_objects(&objects, &script);
        let stored = Command::new("git")
            .current_dir(&source_work)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args(["cat-file", "-e", &sha])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        emit(
            case,
            &format!(
                "sandbox_exit={:?} ctl_rc={} git_ctl_rc={} write_rc={} object_in_source={stored} loose_file_in_source={} err={}",
                o.code,
                out_line(&o, "ctl"),
                out_line(&o, "git_ctl"),
                out_line(&o, "write"),
                objects.join(&sha[..2]).join(&sha[2..]).exists(),
                out_line(&o, "err"),
            ),
        );
    }
}
