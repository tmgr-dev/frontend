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
