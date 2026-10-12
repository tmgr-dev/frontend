use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TARGETS: [&str; 4] = [
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
];

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../public/install.sh")
}

fn archive(tmp: &Path, version: &str, target: &str) -> Vec<u8> {
    let pkg = format!("tmgr-{version}-{target}");
    let dir = tmp.join("src").join(&pkg);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("tmgr"),
        format!("#!/bin/sh\necho tmgr {version}\n"),
    )
    .unwrap();
    let out = tmp.join(format!("{pkg}.tar.gz"));
    let status = Command::new("tar")
        .arg("-czf")
        .arg(&out)
        .arg("-C")
        .arg(tmp.join("src"))
        .arg(&pkg)
        .status()
        .unwrap();
    assert!(status.success());
    std::fs::read(out).unwrap()
}

struct Stand {
    rt: tokio::runtime::Runtime,
    server: MockServer,
    tmp: tempfile::TempDir,
}

impl Stand {
    fn new(prerelease: bool, corrupt: bool) -> Stand {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let server = rt.block_on(MockServer::start());
        let tmp = tempfile::tempdir().unwrap();
        let version = "0.4.0";
        let tag = format!("cli-v{version}");
        let mut sums = String::new();
        let mut files = Vec::new();
        for t in TARGETS {
            let bytes = archive(tmp.path(), version, t);
            let name = format!("tmgr-{version}-{t}.tar.gz");
            let mut hash = Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>();
            if corrupt {
                hash = "0".repeat(64);
            }
            sums.push_str(&format!("{hash}  {name}\n"));
            files.push((name, bytes));
        }
        files.push(("SHA256SUMS".to_string(), sums.into_bytes()));
        let listing = format!(
            "[\n  {{\n    \"url\": \"x\",\n    \"tag_name\": \"desktop-v9.0.0\",\n    \"draft\": false,\n    \"prerelease\": false\n  }},\n  {{\n    \"url\": \"x\",\n    \"tag_name\": \"{tag}\",\n    \"draft\": false,\n    \"prerelease\": {prerelease}\n  }}\n]"
        );
        rt.block_on(async {
            Mock::given(method("GET"))
                .and(path("/repos/tmgr-dev/frontend/releases"))
                .respond_with(ResponseTemplate::new(200).set_body_string(listing))
                .mount(&server)
                .await;
            for (name, bytes) in files {
                Mock::given(method("GET"))
                    .and(path(format!("/dl/{tag}/{name}")))
                    .respond_with(ResponseTemplate::new(200).set_body_bytes(bytes))
                    .mount(&server)
                    .await;
            }
        });
        Stand { rt, server, tmp }
    }

    fn dest(&self) -> PathBuf {
        self.tmp.path().join("bin")
    }

    fn run(&self, env: &[(&str, &str)], shim: Option<&Path>) -> Output {
        let _ = &self.rt;
        let mut c = Command::new("sh");
        c.arg(script())
            .env("HOME", self.tmp.path())
            .env("TMGR_API_URL", self.server.uri())
            .env("TMGR_DOWNLOAD_BASE", format!("{}/dl", self.server.uri()))
            .env("TMGR_INSTALL_DIR", self.dest())
            .env("TMPDIR", self.tmp.path());
        if let Some(dir) = shim {
            let old = std::env::var("PATH").unwrap();
            c.env("PATH", format!("{}:{old}", dir.display()));
        }
        for (k, v) in env {
            c.env(k, v);
        }
        c.output().unwrap()
    }
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn installs_a_beta_release_when_unsigned_is_allowed() {
    let s = Stand::new(true, false);
    let o = s.run(
        &[("TMGR_CHANNEL", "beta"), ("TMGR_ALLOW_UNSIGNED", "1")],
        None,
    );
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("WITHOUT signature verification"));
    let out = Command::new(s.dest().join("tmgr")).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "tmgr 0.4.0");
    let leftovers: Vec<_> = std::fs::read_dir(s.tmp.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with("tmgr-install"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn refuses_while_the_signing_key_is_a_placeholder() {
    let s = Stand::new(true, false);
    let o = s.run(&[("TMGR_CHANNEL", "beta")], None);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("no signing key yet"));
    assert!(!s.dest().join("tmgr").exists());
}

#[test]
fn stable_channel_skips_prereleases() {
    let s = Stand::new(true, false);
    let o = s.run(&[("TMGR_ALLOW_UNSIGNED", "1")], None);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("no stable release found"));
    assert!(!s.dest().join("tmgr").exists());
}

#[test]
fn stable_channel_installs_a_stable_release() {
    let s = Stand::new(false, false);
    let o = s.run(&[("TMGR_ALLOW_UNSIGNED", "1")], None);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(s.dest().join("tmgr").exists());
}

#[test]
fn checksum_mismatch_installs_nothing() {
    let s = Stand::new(false, true);
    let o = s.run(&[("TMGR_ALLOW_UNSIGNED", "1")], None);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("checksum mismatch"));
    assert!(!s.dest().join("tmgr").exists());
}

#[test]
fn windows_points_to_wsl2() {
    let s = Stand::new(false, false);
    let shim = s.tmp.path().join("shim");
    std::fs::create_dir_all(&shim).unwrap();
    let uname = shim.join("uname");
    std::fs::write(&uname, "#!/bin/sh\necho MINGW64_NT-10.0\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&uname, std::fs::Permissions::from_mode(0o755)).unwrap();
    let o = s.run(&[("TMGR_ALLOW_UNSIGNED", "1")], Some(&shim));
    assert!(!o.status.success());
    assert!(stderr(&o).contains("WSL2"));
}

#[test]
fn plain_http_urls_are_refused() {
    let s = Stand::new(false, false);
    let o = s.run(
        &[
            ("TMGR_ALLOW_UNSIGNED", "1"),
            ("TMGR_API_URL", "http://example.com"),
        ],
        None,
    );
    assert!(!o.status.success());
    assert!(stderr(&o).contains("non-https"));
}

#[test]
fn script_is_valid_posix_sh() {
    let o = Command::new("sh").arg("-n").arg(script()).output().unwrap();
    assert!(o.status.success());
}
