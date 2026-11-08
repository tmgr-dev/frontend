#![allow(dead_code)]
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use tmgr_runner::sandbox::{prepare, Grants, Network, SandboxSpec};

pub fn skip_without_landlock(reason: &str) {
    if std::env::var("TMGR_TEST_REQUIRE_LANDLOCK").as_deref() == Ok("1") {
        panic!("{reason}; TMGR_TEST_REQUIRE_LANDLOCK=1 requires Landlock to be usable");
    }
    let _ = std::io::Write::write_all(
        &mut std::io::stderr(),
        format!("SKIPPED: {reason}\n").as_bytes(),
    );
}

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
    pub fn self_contained_clone(&self) {
        let repo = self.root.join("origin");
        fs::create_dir(&repo).unwrap();
        let git = |args: &[&str]| {
            let o = Command::new("git")
                .current_dir(&repo)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
                .args(args)
                .output()
                .unwrap();
            assert!(o.status.success(), "{args:?}: {o:?}");
        };
        git(&["init", "-q", "-b", "main"]);
        fs::write(repo.join("a.txt"), "a\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "a"]);
        let plan = tmgr_runner::worktree::plan(&repo, "task", None).unwrap();
        let clone = tmgr_runner::worktree::clone_mode(&plan).unwrap();
        fs::remove_dir(&self.cwd).unwrap();
        fs::rename(&clone.path, &self.cwd).unwrap();
        assert!(!self.cwd.join(".git/objects/info/alternates").exists());
    }

    pub fn run(&self, script: &str) -> Out {
        self.run_net(script, Network::Open)
    }

    pub fn run_with_objects(&self, objects: &std::path::Path, script: &str) -> Out {
        self.run_objects(objects, script, &[])
    }

    pub fn run_objects(
        &self,
        objects: &std::path::Path,
        script: &str,
        helper_env: &[(&str, &str)],
    ) -> Out {
        use std::os::unix::fs::MetadataExt;
        let meta = objects.metadata().unwrap();
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
                objects_ro: vec![(objects.to_path_buf(), (meta.dev(), meta.ino()))],
                ..Default::default()
            },
            cwd_id: None,
            network: Network::Open,
        };
        let l = prepare(&spec).expect("prepare");
        let o = Command::new(&l.argv[0])
            .args(&l.argv[1..])
            .env_clear()
            .envs(l.env.iter().cloned())
            .envs(helper_env.iter().copied())
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

    pub fn borrowing_clone(&self) -> PathBuf {
        let source = self.root.join("source");
        fs::create_dir(&source).unwrap();
        let git = |dir: &Path, args: &[&str]| {
            let o = Command::new("git")
                .current_dir(dir)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
                .args(args)
                .output()
                .unwrap();
            assert!(o.status.success(), "{args:?}: {o:?}");
        };
        git(&source, &["init", "-q", "-b", "main"]);
        for n in ["a", "b", "c"] {
            fs::write(source.join(format!("{n}.txt")), format!("{n}\n")).unwrap();
        }
        git(&source, &["add", "."]);
        git(&source, &["commit", "-q", "-m", "a"]);
        fs::remove_dir(&self.cwd).unwrap();
        git(
            &self.root,
            &[
                "clone",
                "-q",
                "--shared",
                source.to_str().unwrap(),
                self.cwd.to_str().unwrap(),
            ],
        );
        let objects = source.join(".git/objects");
        let alt = fs::read_to_string(self.cwd.join(".git/objects/info/alternates")).unwrap();
        assert_eq!(Path::new(alt.trim()), objects);
        objects
    }

    pub fn run_net(&self, script: &str, network: Network) -> Out {
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
            cwd_id: None,
            network,
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
            cwd_id: None,
            network: Default::default(),
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
