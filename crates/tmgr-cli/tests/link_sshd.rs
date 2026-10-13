use serde_json::json;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use tmgr_core::creds::Backend;
use tmgr_link::authkeys;
use tmgr_link::grammar;
use tmgr_link::leader::{self, Client, SshTarget};
use tmgr_link::proto::Reply;
use tmgr_link::scope::Scope;
use tmgr_link::store::{Follower, Leader, Paths, Store};

const ENR: &str = "0123456789abcdef0123456789abcdef";

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
    let ok = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(path)
        .status()
        .unwrap()
        .success();
    assert!(ok);
}

#[test]
fn leader_reaches_a_real_sshd_through_the_isolated_transport() {
    if std::env::var("TMGR_LINK_SSHD_TEST").as_deref() != Ok("1") {
        eprintln!("skipped: set TMGR_LINK_SSHD_TEST=1 to run against a local sshd");
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(tmp.path()).unwrap();
    let follower = Paths::under(&root.join("follower"));
    let leader_paths = Paths::under(&root.join("leader"));
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();

    let cred = leader::credential_store(&leader_paths, Some(Backend::File));
    let id = leader::init(&leader_paths, &cred, false).unwrap();
    let key = grammar::parse_pubkey(&id.public).unwrap();

    let bin = std::fs::canonicalize(assert_cmd::cargo::cargo_bin("tmgr")).unwrap();
    let line = authkeys::build_line(&bin, "laptop", ENR, &["127.0.0.1".to_string()], &key).unwrap();
    authkeys::upsert(&follower.authorized_keys(), "laptop", &line, &key).unwrap();
    Store::update(&follower, |s| {
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
                },
            },
        );
        Ok(())
    })
    .unwrap();

    let host_key = root.join("host_key");
    keygen(&host_key);
    let port = free_port();
    let me = std::env::var("USER").expect("USER");
    let cfg = root.join("sshd_config");
    std::fs::write(
        &cfg,
        format!(
            "Port {port}\nListenAddress 127.0.0.1\nHostKey {}\nPidFile {}\nAuthorizedKeysFile {}\nStrictModes no\nUsePAM no\nPermitUserEnvironment no\nAllowUsers {me}\nPubkeyAuthentication yes\nPasswordAuthentication no\nSetEnv XDG_CONFIG_HOME={} XDG_STATE_HOME={}\n",
            host_key.display(),
            root.join("sshd.pid").display(),
            follower.authorized_keys().display(),
            follower.config.parent().unwrap().join("xdg-config").display(),
            follower.config.parent().unwrap().join("xdg-state").display(),
        ),
    )
    .unwrap();
    let xdg_config = follower.config.parent().unwrap().join("xdg-config");
    std::fs::create_dir_all(&xdg_config).unwrap();
    std::fs::rename(&follower.config, xdg_config.join("tmgr")).unwrap();
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

    let pinned = leader::scan_host("127.0.0.1", port).unwrap();
    leader::pin_host(&leader_paths, "spark", &pinned).unwrap();
    let target = SshTarget {
        user: me.clone(),
        host: "127.0.0.1".into(),
        port,
    };
    let (mut client, _key_file) = Client::connect(&leader_paths, &cred, "spark", &target).unwrap();
    let hello = client.hello().unwrap();
    assert_eq!(hello["role"], "follower");
    let r = client
        .call(
            "runs.create",
            json!({"task":"TM-1","repo":repo.to_str().unwrap(),"agent":"codex"}),
        )
        .unwrap();
    assert_eq!(
        r,
        Reply::Err {
            code: "not_available".into(),
            message: "runner not installed".into()
        }
    );
    drop(client);

    Store::update(&leader_paths, |s| {
        s.followers.insert(
            "spark".into(),
            Follower {
                user: me.clone(),
                host: "127.0.0.1".into(),
                port,
                host_key_fingerprint: pinned.fingerprint.clone(),
                added: "2026-10-13T00:00:00Z".into(),
            },
        );
        Ok(())
    })
    .unwrap();

    let other = grammar::parse_pubkey(&{
        let k = root.join("other_host");
        keygen(&k);
        std::fs::read_to_string(k.with_extension("pub")).unwrap()
    })
    .unwrap();
    leader::pin_host(&leader_paths, "spark", &other).unwrap();
    let (mut bad, _k2) = Client::connect(&leader_paths, &cred, "spark", &target).unwrap();
    let err = bad.hello().unwrap_err();
    assert_eq!(err.exit_code(), 9);
    assert!(err.message.contains("host key mismatch"), "{}", err.message);
    drop(sshd);
}
