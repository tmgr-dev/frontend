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

#[test]
fn home_parent_is_stat_able_but_not_listable() {
    let f = fx();
    let parent = f.path(f.home.parent().unwrap());
    let o = f.run(&format!("stat -f %p {parent} >/dev/null && echo statted"));
    assert!(o.stdout.contains("statted"), "{}", o.stderr);
    let o = f.run(&format!("ls {parent}"));
    assert!(!o.ok() && !o.stdout.contains("s_"), "{}", o.stdout);
    let o = f.run(&format!("cat {}", f.path(&f.other_home.join("notes.txt"))));
    assert!(!o.ok() && !o.stdout.contains(SECRET));
}

#[test]
fn profile_directory_is_neither_readable_nor_writable() {
    let mut f = fx();
    let agents = f.root.join("agents");
    fs::create_dir_all(&agents).unwrap();
    fs::write(agents.join("p.toml"), SECRET).unwrap();
    f.deny = vec![agents.clone()];

    let a = f.path(&agents);
    let o = f.run(&format!("cat {a}/p.toml"));
    assert!(!o.ok() && !o.stdout.contains(SECRET), "{}", o.stdout);
    let o = f.run(&format!("echo x > {a}/p.toml; echo y > {a}/new.toml"));
    assert!(!o.ok());
    assert_eq!(fs::read_to_string(agents.join("p.toml")).unwrap(), SECRET);
    assert!(!agents.join("new.toml").exists());
}

const KC_SERVICE: &str = "dev.tmgr.cli.sandbox-test";

fn security(args: &[&str]) -> std::process::Output {
    Command::new("/usr/bin/security")
        .args(args)
        .output()
        .unwrap()
}

fn user_keychains() -> Vec<String> {
    let o = security(&["list-keychains", "-d", "user"]);
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .map(|l| l.trim().trim_matches('"').to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

struct TempKeychains {
    saved: Vec<String>,
    created: Vec<std::path::PathBuf>,
}

impl TempKeychains {
    fn new() -> Self {
        Self {
            saved: user_keychains(),
            created: Vec::new(),
        }
    }

    fn create(&mut self, path: &std::path::Path, password: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let p = path.to_str().unwrap();
        self.created.push(path.to_path_buf());
        let o = security(&["create-keychain", "-p", password, p]);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let o = security(&["unlock-keychain", "-p", password, p]);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
        let o = security(&[
            "add-generic-password",
            "-s",
            KC_SERVICE,
            "-a",
            "test",
            "-w",
            SECRET,
            p,
        ]);
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    }
}

impl Drop for TempKeychains {
    fn drop(&mut self) {
        for p in &self.created {
            let _ = security(&["delete-keychain", p.to_str().unwrap()]);
        }
        let mut args = vec!["list-keychains", "-d", "user", "-s"];
        args.extend(self.saved.iter().map(String::as_str));
        let _ = security(&args);
    }
}

fn random_password() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{nanos:x}{:x}", std::process::id())
}

#[test]
fn a_keychain_under_the_real_home_cannot_be_read_by_path_but_one_in_the_session_can() {
    let f = fx();
    assert_ne!(f.home, f.real_home);
    assert!(!f.home.starts_with(&f.real_home));
    let mut kcs = TempKeychains::new();
    let password = random_password();
    let denied = f.real_home.join("Library/Keychains/tmgr-test.keychain-db");
    let allowed = f.cwd.join("tmgr-allowed.keychain-db");
    kcs.create(&denied, &password);
    kcs.create(&allowed, &password);

    let read = |path: &std::path::Path| {
        format!(
            "/usr/bin/security find-generic-password -s {KC_SERVICE} -w {}",
            f.path(path)
        )
    };
    let outside = security(&[
        "find-generic-password",
        "-s",
        KC_SERVICE,
        "-w",
        denied.to_str().unwrap(),
    ]);
    assert!(
        outside.status.success() && String::from_utf8_lossy(&outside.stdout).trim() == SECRET,
        "control failed: {}",
        String::from_utf8_lossy(&outside.stderr)
    );

    let inside_allowed = f.run(&read(&allowed));
    assert!(
        inside_allowed.ok() && inside_allowed.stdout.trim() == SECRET,
        "the sandbox cannot run security at all, the test would prove nothing: {} {}",
        inside_allowed.stdout,
        inside_allowed.stderr
    );

    let inside = f.run(&read(&denied));
    assert!(
        !inside.ok() && !inside.stdout.contains(SECRET),
        "sandboxed run read a keychain under the real home: {} {}",
        inside.stdout,
        inside.stderr
    );
}
