#![allow(dead_code)]
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use tmgr_runner::sandbox::{prepare, Grants, SandboxSpec};

pub const SECRET: &str = "TOPSECRET-4711";
pub const SID: &str = "s_abcdefghij";
pub const OTHER_SID: &str = "s_klmnopqrst";

pub struct Fx {
    _t: tempfile::TempDir,
    pub root: PathBuf,
    pub real_home: PathBuf,
    pub runner_dir: PathBuf,
    pub cwd: PathBuf,
    pub home: PathBuf,
    pub other_home: PathBuf,
    pub self_exe: PathBuf,
    pub deny: Vec<PathBuf>,
}

pub struct Out {
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Out {
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }
}

pub fn fixture(self_exe: &str) -> Fx {
    let t = tempfile::Builder::new()
        .prefix("sb")
        .tempdir_in("/tmp")
        .unwrap();
    std::fs::set_permissions(
        t.path(),
        <std::fs::Permissions as std::os::unix::fs::PermissionsExt>::from_mode(0o700),
    )
    .unwrap();
    let root = t.path().canonicalize().unwrap();
    let real_home = root.join("fakehome");
    let runner_dir = root.join("runner");
    let cwd = root.join("proj");
    let homes = root.join("homes");
    let other_home = homes.join(OTHER_SID);
    for d in [
        real_home.join(".ssh"),
        runner_dir.clone(),
        cwd.clone(),
        other_home.clone(),
    ] {
        fs::create_dir_all(d).unwrap();
    }
    fs::write(real_home.join(".ssh/id_test"), SECRET).unwrap();
    fs::write(runner_dir.join("client.secret"), SECRET).unwrap();
    fs::write(other_home.join("notes.txt"), SECRET).unwrap();
    Fx {
        home: homes.join(SID),
        _t: t,
        root,
        real_home,
        runner_dir,
        cwd,
        other_home,
        self_exe: PathBuf::from(self_exe),
        deny: Vec::new(),
    }
}

impl Fx {
    pub fn run(&self, script: &str) -> Out {
        let spec = SandboxSpec {
            session_id: SID.into(),
            argv: vec!["/bin/sh".into(), "-c".into(), script.into()],
            cwd: self.cwd.clone(),
            home: self.home.clone(),
            real_home: self.real_home.clone(),
            runner_dir: self.runner_dir.clone(),
            self_exe: self.self_exe.clone(),
            env_pass: vec![],
            base_env: vec![("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into())],
            grants: Grants {
                deny: self.deny.clone(),
                ..Default::default()
            },
        };
        let l = prepare(&spec).expect("prepare");
        let o = Command::new(&l.argv[0])
            .args(&l.argv[1..])
            .env_clear()
            .envs(l.env.iter().cloned())
            .current_dir(&l.cwd)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        Out {
            code: o.status.code(),
            stdout: String::from_utf8_lossy(&o.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&o.stderr).into_owned(),
        }
    }

    pub fn run_pty(&self, script: &str) -> Out {
        let spec = SandboxSpec {
            session_id: SID.into(),
            argv: vec!["/bin/sh".into(), "-c".into(), script.into()],
            cwd: self.cwd.clone(),
            home: self.home.clone(),
            real_home: self.real_home.clone(),
            runner_dir: self.runner_dir.clone(),
            self_exe: self.self_exe.clone(),
            env_pass: vec![],
            base_env: vec![("PATH".into(), "/usr/bin:/bin:/usr/sbin:/sbin".into())],
            grants: Grants {
                deny: self.deny.clone(),
                ..Default::default()
            },
        };
        let l = prepare(&spec).expect("prepare");
        let pair = native_pty_system().openpty(pty_size()).unwrap();
        let mut cmd = CommandBuilder::new(&l.argv[0]);
        cmd.args(&l.argv[1..]);
        cmd.env_clear();
        for (k, v) in &l.env {
            cmd.env(k, v);
        }
        cmd.cwd(&l.cwd);
        let mut child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let t = std::thread::spawn(move || {
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            while let Ok(n) = reader.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            buf
        });
        let status = child.wait().unwrap();
        drop(pair.master);
        let out = t.join().unwrap();
        Out {
            code: Some(status.exit_code() as i32),
            stdout: String::from_utf8_lossy(&out).into_owned(),
            stderr: String::new(),
        }
    }

    pub fn path(&self, p: &Path) -> String {
        p.display().to_string()
    }
}

pub fn spawn_sleep() -> Child {
    Command::new("sleep").arg("60").spawn().unwrap()
}

pub fn assert_alive(c: &mut Child) {
    assert!(c.try_wait().unwrap().is_none(), "outside process died");
}

fn pty_size() -> PtySize {
    PtySize {
        rows: 24,
        cols: 80,
        pixel_width: 0,
        pixel_height: 0,
    }
}

pub struct Sacrificial {
    _master: Box<dyn MasterPty + Send>,
    _slave: Box<dyn portable_pty::SlavePty + Send>,
    pub path: PathBuf,
}

pub fn sacrificial_pty() -> Sacrificial {
    let pair = native_pty_system().openpty(pty_size()).unwrap();
    let path = pair.master.tty_name().expect("tty name");
    Sacrificial {
        _master: pair.master,
        _slave: pair.slave,
        path,
    }
}
