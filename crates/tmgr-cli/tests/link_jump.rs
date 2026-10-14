use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde_json::Value;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use tmgr_core::creds::Backend;
use tmgr_link::authkeys;
use tmgr_link::grammar;
use tmgr_link::jump;
use tmgr_link::leader::{self, Client, SshTarget};
use tmgr_link::scope::Scope;
use tmgr_link::store::{Follower, Leader, Paths, Store};

const ENR: &str = "0123456789abcdef0123456789abcdef";

struct Machine {
    home: PathBuf,
    paths: Paths,
}

fn machine(root: &Path, name: &str) -> Machine {
    let home = root.join(name);
    std::fs::create_dir_all(&home).unwrap();
    let paths = Paths {
        config: home.join(".config").join("tmgr"),
        state: home.join(".local").join("state").join("tmgr"),
        home: home.clone(),
    };
    Machine { home, paths }
}

impl Machine {
    fn tmgr(&self) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_tmgr"));
        c.env("HOME", &self.home)
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("XDG_STATE_HOME")
            .env_remove("XDG_RUNTIME_DIR")
            .env_remove("TMGR_RUN_TOKEN")
            .env_remove("TMGR_RUN_ID")
            .env("TMGR_RUNNER_DIR", self.home.join("no-runner"))
            .env("TMGR_CREDENTIAL_STORE", "file")
            .env("NO_COLOR", "1");
        c
    }

    fn cred(&self) -> tmgr_core::creds::CredentialStore {
        leader::credential_store(&self.paths, Some(Backend::File))
    }

    fn follow(&self, name: &str, host: &str, port: u16, jump: Option<&str>) {
        Store::update(&self.paths, |s| {
            s.followers.insert(
                name.into(),
                Follower {
                    user: "deploy".into(),
                    host: host.into(),
                    port,
                    host_key_fingerprint: "SHA256:fixture".into(),
                    added: "2026-10-13T00:00:00Z".into(),
                },
            );
            if let Some(j) = jump {
                s.jumps.insert(name.into(), j.into());
            }
            Ok(())
        })
        .unwrap();
    }
}

fn private_root() -> (tempfile::TempDir, PathBuf) {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    let root = std::fs::canonicalize(tmp.path()).unwrap();
    (tmp, root)
}

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).to_string()
}

#[test]
fn a_link_used_as_a_jump_host_cannot_be_removed_until_its_users_are() {
    let (_t, root) = private_root();
    let m = machine(&root, "leader");
    m.follow("bastion", "192.0.2.10", 22, None);
    m.follow("inner", "192.0.2.20", 22, Some("bastion"));
    m.follow("other", "192.0.2.30", 22, Some("bastion"));

    let out = m
        .tmgr()
        .args(["link", "remove", "bastion"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(6), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("jump host of inner, other"),
        "{}",
        stderr(&out)
    );
    assert!(Store::load(&m.paths)
        .unwrap()
        .followers
        .contains_key("bastion"));
    let log = tmgr_link::audit::Audit::new(&m.paths.audit_dir())
        .tail(5)
        .unwrap();
    assert_eq!(log.last().unwrap()["event"], "link.remove.refused");

    let out = m.tmgr().args(["link", "remove", "inner"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert_eq!(
        m.tmgr()
            .args(["link", "remove", "bastion"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(6)
    );
    assert_eq!(
        m.tmgr()
            .args(["link", "remove", "other"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(0)
    );
    let out = m
        .tmgr()
        .args(["link", "remove", "bastion"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let s = Store::load(&m.paths).unwrap();
    assert!(s.followers.is_empty() && s.jumps.is_empty());
}

#[test]
fn list_shows_the_jump_in_json_and_text() {
    let (_t, root) = private_root();
    let m = machine(&root, "leader");
    m.follow("bastion", "192.0.2.10", 22, None);
    m.follow("inner", "192.0.2.20", 2200, Some("bastion"));
    let out = m.tmgr().args(["link", "list", "--json"]).output().unwrap();
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let find = |n: &str| {
        v["data"]["followers"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["name"] == n)
            .cloned()
            .unwrap()
    };
    assert_eq!(find("inner")["jump"], "bastion");
    assert!(find("bastion")["jump"].is_null());
    let out = m.tmgr().args(["link", "list"]).output().unwrap();
    assert!(String::from_utf8_lossy(&out.stdout).contains("deploy@192.0.2.20:2200 via bastion"));
}

#[test]
fn a_home_that_ssh_cannot_be_given_safely_is_refused_before_any_connection() {
    let (_t, root) = private_root();
    let m = machine(&root, "lead er");
    leader::init(&m.paths, &m.cred(), false).unwrap();
    m.follow("bastion", "192.0.2.10", 22, None);
    m.follow("inner", "192.0.2.20", 22, Some("bastion"));
    let out = m.tmgr().args(["link", "test", "inner"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("cannot be passed to ssh safely"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn a_chained_or_dangling_jump_in_the_store_is_refused_at_connect_time() {
    let (_t, root) = private_root();
    let m = machine(&root, "leader");
    leader::init(&m.paths, &m.cred(), false).unwrap();
    m.follow("a", "192.0.2.10", 22, None);
    m.follow("b", "192.0.2.11", 22, Some("a"));
    m.follow("c", "192.0.2.12", 22, Some("b"));
    let out = m.tmgr().args(["link", "test", "c"]).output().unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(stderr(&out).contains("only one hop"), "{}", stderr(&out));
    m.follow("d", "192.0.2.13", 22, Some("ghost"));
    let out = m.tmgr().args(["link", "test", "d"]).output().unwrap();
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn add_with_jump_stays_privileged_and_needs_a_terminal() {
    let (_t, root) = private_root();
    let m = machine(&root, "leader");
    let out = m
        .tmgr()
        .args([
            "link",
            "add",
            "inner",
            "--ssh",
            "deploy@192.0.2.20",
            "--jump",
            "bastion",
        ])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(10), "{}", stderr(&out));
    assert!(Store::load(&m.paths).unwrap().followers.is_empty());
}

fn pty_tmgr(m: &Machine, args: &[&str], typed: &str) -> (u32, String) {
    let pair = native_pty_system()
        .openpty(PtySize {
            rows: 24,
            cols: 200,
            pixel_width: 0,
            pixel_height: 0,
        })
        .unwrap();
    let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_tmgr"));
    cmd.args(args);
    cmd.env("HOME", &m.home);
    for k in [
        "XDG_CONFIG_HOME",
        "XDG_STATE_HOME",
        "XDG_RUNTIME_DIR",
        "TMGR_RUN_TOKEN",
        "TMGR_RUN_ID",
    ] {
        cmd.env_remove(k);
    }
    cmd.env("TMGR_RUNNER_DIR", m.home.join("no-runner"));
    cmd.env("TMGR_CREDENTIAL_STORE", "file");
    cmd.env("NO_COLOR", "1");
    let mut child = pair.slave.spawn_command(cmd).unwrap();
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().unwrap();
    let mut writer = pair.master.take_writer().unwrap();
    writer.write_all(typed.as_bytes()).unwrap();
    let text = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = reader.read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).to_string()
    });
    let status = child.wait().unwrap();
    drop(writer);
    drop(pair.master);
    (status.exit_code(), text.join().unwrap_or_default())
}

struct Sshd(Child);

impl Drop for Sshd {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn keygen(path: &Path) {
    assert!(Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(path)
        .status()
        .unwrap()
        .success());
}

struct FollowerMachine {
    root: PathBuf,
    xdg_config: PathBuf,
    authorized_keys: PathBuf,
    base_line: String,
}

fn splice_forwarding(line: &str, port: u16) -> String {
    assert!(line.contains(",no-port-forwarding,"));
    line.replacen(
        "no-port-forwarding",
        &format!("port-forwarding,permitopen=\"127.0.0.1:{port}\""),
        1,
    )
}

fn follower_machine(
    root: &Path,
    dir: &str,
    key: &grammar::PubKey,
    repo: &Path,
    forward_to: Option<u16>,
) -> FollowerMachine {
    let fol_root = root.join(dir);
    let fol = Paths::under(&fol_root);
    let bin = std::fs::canonicalize(env!("CARGO_BIN_EXE_tmgr")).unwrap();
    let line = authkeys::build_line(&bin, "laptop", ENR, &["127.0.0.1".to_string()], key).unwrap();
    let written = match forward_to {
        Some(p) => splice_forwarding(&line, p),
        None => line.clone(),
    };
    authkeys::upsert(&fol.authorized_keys(), "laptop", &written, key).unwrap();
    Store::update(&fol, |s| {
        s.leaders.insert(
            "laptop".into(),
            Leader {
                key: key.blob.clone(),
                fingerprint: key.fingerprint.clone(),
                added: "2026-10-13T00:00:00Z".into(),
                enrollment: ENR.into(),
                authorized_keys: String::new(),
                scope: Scope {
                    repos: vec![repo.to_str().unwrap().into()],
                    agents: vec!["codex".into()],
                    models: vec![],
                    max: 1,
                    from: vec!["127.0.0.1".into()],
                    allow_attach_input: false,
                },
            },
        );
        Ok(())
    })
    .unwrap();
    let xdg_config = fol_root.join("xdg-config");
    std::fs::create_dir_all(&xdg_config).unwrap();
    std::fs::rename(&fol.config, xdg_config.join("tmgr")).unwrap();
    FollowerMachine {
        root: fol_root,
        xdg_config,
        authorized_keys: fol.authorized_keys(),
        base_line: line,
    }
}

fn write_jump_keys(fm: &FollowerMachine, allowed: u16) {
    std::fs::write(
        &fm.authorized_keys,
        format!("{}\n", splice_forwarding(&fm.base_line, allowed)),
    )
    .unwrap();
}

fn start_sshd(root: &Path, name: &str, port: u16, fm: &FollowerMachine) -> Sshd {
    let me = std::env::var("USER").expect("USER");
    let host_key = root.join(format!("{name}_host_key"));
    keygen(&host_key);
    let cfg = root.join(format!("{name}_sshd_config"));
    std::fs::write(
        &cfg,
        format!(
            "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nPidFile {}\nAuthorizedKeysFile {}\nStrictModes no\nUsePAM no\nPermitUserEnvironment no\nAllowUsers {me}\nPubkeyAuthentication yes\nPasswordAuthentication no\nAllowTcpForwarding yes\nSetEnv XDG_CONFIG_HOME={} XDG_STATE_HOME={} TMGR_RUNNER_DIR={}\n",
            host_key.display(),
            root.join(format!("{name}.pid")).display(),
            fm.authorized_keys.display(),
            fm.xdg_config.display(),
            fm.root.join("xdg-state").display(),
            root.join("no-runner").display(),
        ),
    )
    .unwrap();
    let sshd = Sshd(
        Command::new("/usr/sbin/sshd")
            .args(["-D", "-e", "-f"])
            .arg(&cfg)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("sshd"),
    );
    for _ in 0..50 {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    sshd
}

#[test]
fn leader_reaches_a_follower_through_a_pinned_jump_sshd() {
    if std::env::var("TMGR_LINK_SSHD_TEST").as_deref() != Ok("1") {
        eprintln!("skipped: set TMGR_LINK_SSHD_TEST=1 to run against a local sshd");
        return;
    }
    let (_t, root) = private_root();
    let me = std::env::var("USER").expect("USER");
    let lead = machine(&root, "leader");
    let id = leader::init(&lead.paths, &lead.cred(), false).unwrap();
    let key = grammar::parse_pubkey(&id.public).unwrap();

    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let target_port = free_port();
    let jump_port = free_port();
    let target_fol = follower_machine(&root, "follower", &key, &repo, None);
    let jump_fol = follower_machine(&root, "jumpfol", &key, &repo, Some(target_port));
    let fol_root = target_fol.root.clone();
    let _target = start_sshd(&root, "target", target_port, &target_fol);
    let _jump = start_sshd(&root, "jump", jump_port, &jump_fol);

    let direct = leader::scan_host("127.0.0.1", target_port).unwrap();
    let ssh_user = format!("{me}@127.0.0.1");
    let jump_port_arg = jump_port.to_string();
    let (code, text) = pty_tmgr(
        &lead,
        &[
            "link",
            "add",
            "hop",
            "--ssh",
            &ssh_user,
            "--port",
            &jump_port_arg,
        ],
        "hop\n",
    );
    assert_eq!(
        code, 0,
        "a jump host is enrolled with a real link add: {text}"
    );
    let out = lead.tmgr().args(["link", "test", "hop"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let jump_key = leader::scan_host("127.0.0.1", jump_port).unwrap();
    let hop = jump::hop_for(&Store::load(&lead.paths).unwrap(), "hop").unwrap();
    let through =
        jump::scan_host_via(&lead.paths, &lead.cred(), &hop, "127.0.0.1", target_port).unwrap();
    assert_eq!(
        through, direct,
        "TOFU through the jump sees the target's key"
    );

    leader::pin_host(&lead.paths, "dest", &direct).unwrap();
    Store::update(&lead.paths, |s| {
        s.followers.insert(
            "dest".into(),
            Follower {
                user: me.clone(),
                host: "127.0.0.1".into(),
                port: target_port,
                host_key_fingerprint: direct.fingerprint.clone(),
                added: "2026-10-13T00:00:00Z".into(),
            },
        );
        s.jumps.insert("dest".into(), "hop".into());
        Ok(())
    })
    .unwrap();

    let target = SshTarget {
        user: me.clone(),
        host: "127.0.0.1".into(),
        port: target_port,
    };
    let (mut client, _k) =
        Client::connect_via(&lead.paths, &lead.cred(), "dest", &target, Some(&hop)).unwrap();
    let hello = client.hello().unwrap();
    assert_eq!(hello["role"], "follower");
    drop(client);

    let out = lead
        .tmgr()
        .args(["link", "test", "dest", "--json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["data"]["link"], "dest");

    let other = {
        let k = root.join("other_host");
        keygen(&k);
        grammar::parse_pubkey(&std::fs::read_to_string(k.with_extension("pub")).unwrap()).unwrap()
    };
    leader::pin_host(&lead.paths, "dest", &other).unwrap();
    let out = lead.tmgr().args(["link", "test", "dest"]).output().unwrap();
    assert_eq!(out.status.code(), Some(9), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("host key mismatch"),
        "{}",
        stderr(&out)
    );
    leader::pin_host(&lead.paths, "dest", &direct).unwrap();

    leader::pin_host(&lead.paths, "hop", &other).unwrap();
    let out = lead.tmgr().args(["link", "test", "dest"]).output().unwrap();
    assert_eq!(out.status.code(), Some(9), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("host key mismatch"),
        "{}",
        stderr(&out)
    );
    leader::pin_host(&lead.paths, "hop", &jump_key).unwrap();

    write_jump_keys(&jump_fol, free_port());
    let out = lead.tmgr().args(["link", "test", "dest"]).output().unwrap();
    assert_eq!(
        out.status.code(),
        Some(9),
        "forwarding to a port outside permitopen: {}",
        stderr(&out)
    );
    write_jump_keys(&jump_fol, target_port);

    let out = lead.tmgr().args(["link", "test", "dest"]).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let out = lead
        .tmgr()
        .args(["link", "remove", "hop"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(6));
    let own_key_line = format!("{} tmgr-link\n", key.openssh());
    let follower_identity = target_fol.xdg_config.join("tmgr").join("link_key.pub");
    std::fs::write(&follower_identity, &own_key_line).unwrap();
    let port = target_port.to_string();
    let add = |name: &str| {
        pty_tmgr(
            &lead,
            &[
                "link",
                "add",
                name,
                "--ssh",
                &format!("{me}@127.0.0.1"),
                "--port",
                &port,
                "--jump",
                "hop",
            ],
            &format!("{name}\n"),
        )
    };
    let (code, text) = add("cyc");
    assert_eq!(code, 9, "{text}");
    let s = Store::load(&lead.paths).unwrap();
    assert!(!s.followers.contains_key("cyc") && !s.jumps.contains_key("cyc"));
    let kh = std::fs::read_to_string(lead.paths.known_hosts()).unwrap();
    assert!(
        !kh.contains("tmgr-link-cyc"),
        "a refused link must not stay pinned"
    );
    let follower_log =
        tmgr_link::audit::Audit::new(&fol_root.join("xdg-state").join("tmgr").join("link-audit"))
            .tail(10)
            .unwrap();
    assert!(follower_log.iter().any(|e| e["event"] == "worker.refused"
        && e["reason"]
            .as_str()
            .is_some_and(|r| r.contains("own identity"))));

    let fresh = {
        let k = root.join("fresh_id");
        keygen(&k);
        std::fs::read_to_string(k.with_extension("pub")).unwrap()
    };
    Store::update(&lead.paths, |s| {
        s.leaders.insert(
            "boss".into(),
            Leader {
                key: grammar::parse_pubkey(&fresh).unwrap().blob,
                fingerprint: "SHA256:fixture".into(),
                added: "2026-10-13T00:00:00Z".into(),
                enrollment: ENR.into(),
                authorized_keys: String::new(),
                scope: Scope::default(),
            },
        );
        Ok(())
    })
    .unwrap();
    std::fs::write(&follower_identity, &fresh).unwrap();
    let (code, text) = add("cyc");
    assert_eq!(code, 6, "{text}");
    assert!(text.contains("already leads this machine"), "{text}");
    let kh = std::fs::read_to_string(lead.paths.known_hosts()).unwrap();
    assert!(!kh.contains("tmgr-link-cyc"));
    let log = tmgr_link::audit::Audit::new(&lead.paths.audit_dir())
        .tail(10)
        .unwrap();
    assert!(log
        .iter()
        .any(|e| e["event"] == "link.refused" && e["link"] == "cyc"));
    Store::update(&lead.paths, |s| {
        s.leaders.remove("boss");
        Ok(())
    })
    .unwrap();

    let (code, text) = add("viajump");
    assert_eq!(code, 0, "{text}");
    assert!(text.contains(&direct.fingerprint), "{text}");
    assert!(
        text.contains("seen through the pinned jump link hop"),
        "{text}"
    );
    let s = Store::load(&lead.paths).unwrap();
    assert_eq!(s.jumps.get("viajump").map(String::as_str), Some("hop"));
    assert_eq!(
        s.peer_keys.get("viajump"),
        Some(&grammar::parse_pubkey(&fresh).unwrap().blob)
    );
    let out = lead
        .tmgr()
        .args(["link", "test", "viajump"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));

    let (code, text) = pty_tmgr(
        &lead,
        &[
            "link",
            "add",
            "nojump",
            "--ssh",
            &format!("{me}@127.0.0.1"),
            "--jump",
            "ghost",
        ],
        "nojump\n",
    );
    assert_eq!(code, 3, "{text}");

    let (code, _) = pty_tmgr(
        &lead,
        &[
            "link",
            "add",
            "chain",
            "--ssh",
            &format!("{me}@127.0.0.1"),
            "--jump",
            "viajump",
        ],
        "chain\n",
    );
    assert_eq!(code, 2, "a jump link may not itself be jumped");
}
