#![cfg(target_os = "linux")]
mod common;

use common::*;
use std::fs;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixListener};
use std::process::Command;

fn fx() -> Fx {
    fixture(env!("CARGO_BIN_EXE_tmgr-sandbox-probe"))
}

fn landlock_usable(f: &Fx) -> bool {
    let o = f.run("exit 0");
    if o.code == Some(78) {
        eprintln!("SKIPPED: {}", o.stderr);
        assert!(o.stderr.contains("sandbox"), "{}", o.stderr);
        assert!(o.stderr.contains("the run does not start"), "{}", o.stderr);
        return false;
    }
    assert!(o.ok(), "{} {}", o.stdout, o.stderr);
    true
}

#[test]
fn secrets_are_unreadable() {
    let f = fx();
    if !landlock_usable(&f) {
        return;
    }
    let p = f.path(&f.real_home.join(".ssh/id_test"));
    let o = f.run(&format!("cat {p}"));
    assert!(!o.ok() && !o.stdout.contains(SECRET), "{}", o.stdout);
    let p = f.path(&f.runner_dir.join("client.secret"));
    let o = f.run(&format!("cat {p}"));
    assert!(!o.ok() && !o.stdout.contains(SECRET));
    let p = f.path(&f.other_home.join("notes.txt"));
    let o = f.run(&format!("cat {p}"));
    assert!(!o.ok() && !o.stdout.contains(SECRET));
    let o = f.run(&format!("ls {}", f.path(&f.real_home)));
    assert!(!o.ok());
}

#[test]
fn policy_file_is_outside_the_sandbox() {
    let f = fx();
    if !landlock_usable(&f) {
        return;
    }
    let pol = f.runner_dir.join("sessions").join(SID).join("sandbox.json");
    assert!(pol.is_file());
    let o = f.run(&format!("cat {}", f.path(&pol)));
    assert!(!o.ok() && !o.stdout.contains("read"));
}

#[test]
fn abstract_socket_connect_is_blocked() {
    let f = fx();
    if !landlock_usable(&f) {
        return;
    }
    let name = "tmgr-test-abstract-4711";
    let _l = UnixListener::bind_addr(&SocketAddr::from_abstract_name(name).unwrap()).unwrap();
    fs::write(f.runner_dir.join("runner.addr"), name).unwrap();
    let code = format!(
        "import socket\ns=socket.socket(socket.AF_UNIX)\ntry:\n s.connect('\\0{name}')\n print('connected')\nexcept OSError as e:\n print('errno', e.errno)\n"
    );
    let outside = Command::new("python3")
        .args(["-c", &code])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&outside.stdout).trim(), "connected");
    let o = f.run(&format!("python3 -c \"{}\"", code.replace('"', "\\\"")));
    assert_eq!(o.stdout.trim(), "errno 1", "{} {}", o.stdout, o.stderr);
}

#[test]
fn signals_to_outside_processes_fail() {
    let f = fx();
    if !landlock_usable(&f) {
        return;
    }
    let mut c = spawn_sleep();
    let o = f.run(&format!("kill -TERM {}", c.id()));
    assert!(!o.ok(), "kill succeeded");
    std::thread::sleep(std::time::Duration::from_millis(200));
    assert_alive(&mut c);
    let o = f.run("sleep 30 & kill -TERM $!");
    assert!(o.ok(), "{}", o.stderr);
    c.kill().unwrap();
    c.wait().unwrap();
}

#[test]
fn outside_process_environment_is_unreadable() {
    let f = fx();
    if !landlock_usable(&f) {
        return;
    }
    let mut c = spawn_sleep();
    let o = f.run(&format!("cat /proc/{}/environ", c.id()));
    assert!(!o.ok(), "environ readable");
    c.kill().unwrap();
    c.wait().unwrap();
}

#[test]
fn writes_only_in_cwd_home_and_tmp() {
    let f = fx();
    if !landlock_usable(&f) {
        return;
    }
    let o = f.run(&format!("echo x > {}", f.path(&f.real_home.join("x"))));
    assert!(!o.ok());
    assert!(!f.real_home.join("x").exists());
    let o = f.run(&format!("echo x > {}", f.path(&f.root.join("outside.txt"))));
    assert!(!o.ok());
    let o = f.run("echo x > /tmp/tmgr-sandbox-should-not-exist");
    assert!(!o.ok());
    assert!(!std::path::Path::new("/tmp/tmgr-sandbox-should-not-exist").exists());
    let o = f.run("echo x > inside.txt && echo y > $HOME/h.txt && echo z > $TMPDIR/t.txt");
    assert!(o.ok(), "{}", o.stderr);
    assert!(f.cwd.join("inside.txt").is_file());
    assert!(f.home.join("h.txt").is_file());
    assert!(f.home.join("tmp/t.txt").is_file());
    let o = f.run("echo x > /dev/null && echo $HOME $SSH_AUTH_SOCK");
    assert!(o.ok());
    assert!(o.stdout.contains("homes"));
}

#[test]
fn ordinary_tools_work() {
    let f = fx();
    if !landlock_usable(&f) {
        return;
    }
    let o = f.run("/bin/echo hello");
    assert_eq!(o.stdout.trim(), "hello");
    let o = f.run("python3 -c 'print(1)'");
    assert_eq!(o.stdout.trim(), "1", "{}", o.stderr);
    let o = f.run("git --version");
    assert!(o.stdout.starts_with("git version"), "{}", o.stderr);
    let o = f.run("git init -q . && git config user.name t && git config user.email t@e && git commit -q --allow-empty -m x && git log --oneline");
    assert!(o.ok(), "{}", o.stderr);
}

#[test]
fn missing_command_exits_127() {
    let f = fx();
    if !landlock_usable(&f) {
        return;
    }
    let o = f.run("exec /nonexistent/binary");
    assert_eq!(o.code, Some(127));
}

#[test]
fn other_terminals_are_unreachable_but_own_pty_works() {
    let f = fx();
    if !landlock_usable(&f) {
        return;
    }
    let other = sacrificial_pty();
    let t = f.path(&other.path);
    let o = f.run_pty(&format!(
        "echo own-out; echo tty-out > /dev/tty; (: > {t}) 2>/dev/null; echo w=$?; (: < {t}) 2>/dev/null; echo r=$?; own=$(readlink /proc/self/fd/0); (echo own-path >> $own) 2>/dev/null; echo o=$?"
    ));
    assert!(o.stdout.contains("own-out"), "{}", o.stdout);
    assert!(o.stdout.contains("tty-out"), "{}", o.stdout);
    assert!(
        o.stdout.contains("w=") && !o.stdout.contains("w=0"),
        "{}",
        o.stdout
    );
    assert!(
        o.stdout.contains("r=") && !o.stdout.contains("r=0"),
        "{}",
        o.stdout
    );
    assert!(o.stdout.contains("o=0"), "{}", o.stdout);
}

fn launch_with(f: &Fx, script: &str, edit: impl FnOnce(&mut serde_json::Value)) -> Out {
    use std::process::Stdio;
    use tmgr_runner::sandbox::{prepare, SandboxSpec};
    let spec = SandboxSpec {
        session_id: SID.into(),
        argv: vec!["/bin/sh".into(), "-c".into(), script.into()],
        cwd: f.cwd.join("sub"),
        home: f.home.clone(),
        real_home: f.real_home.clone(),
        runner_dir: f.runner_dir.clone(),
        self_exe: f.self_exe.clone(),
        env_pass: vec![],
        base_env: vec![("PATH".into(), "/usr/bin:/bin".into())],
        grants: Default::default(),
    };
    let l = prepare(&spec).expect("prepare");
    let pol = std::path::Path::new(&l.argv[2]);
    let mut v: serde_json::Value = serde_json::from_slice(&fs::read(pol).unwrap()).unwrap();
    edit(&mut v);
    fs::write(pol, v.to_string()).unwrap();
    let o = Command::new(&l.argv[0])
        .args(&l.argv[1..])
        .env_clear()
        .envs(l.env.iter().cloned())
        .current_dir(&f.cwd)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    Out {
        code: o.status.code(),
        stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

fn swap_for_root_symlink(dir: &std::path::Path) {
    let moved = dir.with_extension("moved");
    fs::rename(dir, moved).unwrap();
    std::os::unix::fs::symlink("/", dir).unwrap();
}

#[test]
fn swapped_cwd_is_refused() {
    let f = fx();
    if !landlock_usable(&f) {
        return;
    }
    fs::create_dir_all(f.cwd.join("sub")).unwrap();
    let marker = f.cwd.join("marker");
    let script = format!("echo ran > {}", f.path(&marker));
    let cwd_sub = f.cwd.join("sub");
    let o = launch_with(&f, &script, |_| swap_for_root_symlink(&cwd_sub));
    assert_eq!(o.code, Some(78), "{} {}", o.stdout, o.stderr);
    assert!(
        o.stderr.contains("changed after validation"),
        "{}",
        o.stderr
    );
    assert!(!marker.exists());
}

#[test]
fn swapped_root_is_refused() {
    let f = fx();
    if !landlock_usable(&f) {
        return;
    }
    fs::create_dir_all(f.cwd.join("sub")).unwrap();
    let extra = f.cwd.join("extra");
    fs::create_dir_all(&extra).unwrap();
    let marker = f.cwd.join("marker");
    let script = format!("echo ran > {}", f.path(&marker));
    let extra_json = extra.display().to_string();
    let o = launch_with(&f, &script, |v| {
        v["write"].as_array_mut().unwrap().push(extra_json.into());
        swap_for_root_symlink(&extra);
    });
    assert_eq!(o.code, Some(78), "{} {}", o.stdout, o.stderr);
    assert!(
        o.stderr.contains("changed after validation"),
        "{}",
        o.stderr
    );
    assert!(!marker.exists());
}
