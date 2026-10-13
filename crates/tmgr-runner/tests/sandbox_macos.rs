#![cfg(target_os = "macos")]
mod common;

use common::*;
use std::fs;
use std::os::unix::net::UnixListener;
use std::process::Command;

fn fx() -> Fx {
    fixture("/usr/bin/true")
}

const CONNECT: &str = "use Socket; socket(S, PF_UNIX, SOCK_STREAM, 0); if (connect(S, sockaddr_un($ARGV[0]))) { print qq(connected\\n) } else { print qq(errno=).($!+0).qq(\\n) }";

#[test]
fn secrets_are_unreadable() {
    let f = fx();
    for p in [
        f.real_home.join(".ssh/id_test"),
        f.runner_dir.join("client.secret"),
        f.other_home.join("notes.txt"),
    ] {
        let o = f.run(&format!("cat {}", f.path(&p)));
        assert!(!o.ok() && !o.stdout.contains(SECRET), "{}", p.display());
    }
}

#[test]
fn runner_socket_connect_is_blocked() {
    let f = fx();
    let sock = f.runner_dir.join("r.sock");
    let _l = UnixListener::bind(&sock).unwrap();
    let s = f.path(&sock);
    let outside = Command::new("perl")
        .args(["-e", CONNECT, &s])
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&outside.stdout).trim(), "connected");
    let o = f.run(&format!("perl -e '{CONNECT}' {s}"));
    assert_eq!(o.stdout.trim(), "errno=1", "{} {}", o.stdout, o.stderr);
}

#[test]
fn writes_only_in_cwd_and_home() {
    let f = fx();
    let o = f.run(&format!("echo x > {}", f.path(&f.real_home.join("x"))));
    assert!(!o.ok());
    assert!(!f.real_home.join("x").exists());
    let o = f.run("echo x > inside.txt && echo y > $HOME/h.txt && echo z > $TMPDIR/t.txt");
    assert!(o.ok(), "{}", o.stderr);
    assert!(f.cwd.join("inside.txt").is_file());
    assert!(f.home.join("h.txt").is_file());
    let o = f.run("echo x > /dev/null");
    assert!(o.ok());
}

#[test]
fn signals_to_outside_processes_fail() {
    let f = fx();
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
fn ordinary_tools_work() {
    let f = fx();
    let o = f.run("/bin/echo hello");
    assert_eq!(o.stdout.trim(), "hello");
    let o = f.run("git --version");
    assert!(o.stdout.starts_with("git version"), "{}", o.stderr);
    let _ = fs::metadata(&f.cwd);
}

#[test]
fn other_terminals_are_unreachable_but_own_pty_works() {
    let f = fx();
    let other = sacrificial_pty();
    let t = f.path(&other.path);
    let o = f.run_pty(&format!(
        "echo own-out; echo tty-out > /dev/tty; (: > {t}) 2>/dev/null; echo w=$?; (: < {t}) 2>/dev/null; echo r=$?"
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
    if std::path::Path::new("/dev/ttyp0").exists() {
        let o = f.run(
            "perl -e 'use Fcntl; if (sysopen(F, \"/dev/ttyp0\", O_RDONLY|O_NONBLOCK|O_NOCTTY)) { print qq(opened\\n) } else { print qq(errno=).($!+0).qq(\\n) }'",
        );
        assert_eq!(o.stdout.trim(), "errno=1", "{} {}", o.stdout, o.stderr);
    } else {
        println!("note: /dev/ttyp0 does not exist, legacy slave assertion skipped");
    }
}
