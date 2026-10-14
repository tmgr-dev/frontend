use sha2::{Digest, Sha256};
use std::os::unix::fs::PermissionsExt;
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
const TEST_KEY: &str = "RWTESTONLYKEY";
const PLACEHOLDER: &str = "TMGR-UPDATE-KEY-PLACEHOLDER";

fn script() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../public/install.sh")
}

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn fake_tmgr(version: &str) -> String {
    format!(
        "#!/bin/sh\ncase \"$1\" in version) echo '{{\"schema\":\"tmgr.v1\",\"ok\":true,\"data\":{{\"version\":\"{version}\",\"commit\":\"c\",\"target\":\"t\"}},\"warnings\":[]}}';; *) echo tmgr {version};; esac\n"
    )
}

fn archive(tmp: &Path, version: &str, target: &str) -> Vec<u8> {
    let pkg = format!("tmgr-{version}-{target}");
    let dir = tmp.join("src").join(&pkg);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("tmgr"), fake_tmgr(version)).unwrap();
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

struct Spec {
    prerelease: bool,
    channel: &'static str,
    expires: &'static str,
    tamper_archive: bool,
    signature: Option<&'static str>,
}

impl Default for Spec {
    fn default() -> Self {
        Spec {
            prerelease: false,
            channel: "stable",
            expires: "2999-01-01T00:00:00Z",
            tamper_archive: false,
            signature: Some("GOODSIG"),
        }
    }
}

struct Stand {
    _rt: tokio::runtime::Runtime,
    server: MockServer,
    tmp: tempfile::TempDir,
}

impl Stand {
    fn new(spec: Spec) -> Stand {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let server = rt.block_on(MockServer::start());
        let tmp = tempfile::tempdir().unwrap();
        let version = "0.4.0";
        let tag = format!("cli-v{version}");
        let mut files = Vec::new();
        let mut artifacts = Vec::new();
        for t in TARGETS {
            let bytes = archive(tmp.path(), version, t);
            let name = format!("tmgr-{version}-{t}.tar.gz");
            artifacts.push(serde_json::json!({
                "target": t, "name": name, "sha256": hex(&bytes), "size": bytes.len()
            }));
            let served = if spec.tamper_archive {
                let mut b = bytes.clone();
                let last = b.len() - 1;
                b[last] ^= 0xff;
                b
            } else {
                bytes
            };
            files.push((name, served));
        }
        let manifest = serde_json::to_string_pretty(&serde_json::json!({
            "product": "tmgr-cli", "version": version, "channel": spec.channel,
            "key_epoch": 1, "expires_at": spec.expires, "artifacts": artifacts
        }))
        .unwrap();
        files.push(("manifest.json".to_string(), manifest.into_bytes()));
        if let Some(sig) = spec.signature {
            files.push(("manifest.json.minisig".to_string(), sig.as_bytes().to_vec()));
        }
        let listing = format!(
            "[\n  {{\n    \"url\": \"x\",\n    \"tag_name\": \"desktop-v9.0.0\",\n    \"draft\": false,\n    \"prerelease\": false\n  }},\n  {{\n    \"url\": \"x\",\n    \"tag_name\": \"{tag}\",\n    \"draft\": false,\n    \"prerelease\": {}\n  }}\n]",
            spec.prerelease
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
        Stand {
            _rt: rt,
            server,
            tmp,
        }
    }

    fn dest(&self) -> PathBuf {
        self.tmp.path().join("bin")
    }

    fn keyed_script(&self) -> PathBuf {
        let body = std::fs::read_to_string(script()).unwrap();
        let out = self.tmp.path().join("install-keyed.sh");
        std::fs::write(
            &out,
            body.replacen(
                &format!("MINISIGN_PUBKEY=\"{PLACEHOLDER}\""),
                &format!("MINISIGN_PUBKEY=\"{TEST_KEY}\""),
                1,
            ),
        )
        .unwrap();
        out
    }

    fn shim_dir(&self) -> PathBuf {
        let dir = self.tmp.path().join("shim");
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_shim(&self, name: &str, body: &str) {
        let f = self.shim_dir().join(name);
        std::fs::write(&f, body).unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn with_minisign(&self) {
        self.write_shim(
            "minisign",
            &format!("#!/bin/sh\n[ \"$6\" = \"{TEST_KEY}\" ] && grep -q GOODSIG \"$4\"\n"),
        );
    }

    fn path_with_shims(&self, without_minisign: bool) -> String {
        let old = std::env::var("PATH").unwrap();
        let kept: Vec<&str> = old
            .split(':')
            .filter(|d| !(without_minisign && Path::new(d).join("minisign").exists()))
            .collect();
        format!("{}:{}", self.shim_dir().display(), kept.join(":"))
    }

    fn run_script(&self, script: &Path, env: &[(&str, &str)], without_minisign: bool) -> Output {
        let mut c = Command::new("sh");
        c.args(["-c", "umask 002; exec sh \"$1\"", "sh"])
            .arg(script)
            .env("HOME", self.tmp.path())
            .env("PATH", self.path_with_shims(without_minisign))
            .env("TMGR_API_URL", self.server.uri())
            .env("TMGR_DOWNLOAD_BASE", format!("{}/dl", self.server.uri()))
            .env("TMGR_INSTALL_DIR", self.dest())
            .env("TMPDIR", self.tmp.path());
        for (k, v) in env {
            c.env(k, v);
        }
        c.output().unwrap()
    }

    fn run(&self, env: &[(&str, &str)]) -> Output {
        self.run_script(&script(), env, false)
    }

    fn run_keyed(&self, env: &[(&str, &str)]) -> Output {
        self.run_script(&self.keyed_script(), env, false)
    }

    fn preinstall(&self, version: &str) {
        std::fs::create_dir_all(self.dest()).unwrap();
        std::fs::set_permissions(self.dest(), std::fs::Permissions::from_mode(0o755)).unwrap();
        let f = self.dest().join("tmgr");
        std::fs::write(&f, fake_tmgr(version)).unwrap();
        std::fs::set_permissions(&f, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    fn installed_version(&self) -> Option<String> {
        let out = Command::new(self.dest().join("tmgr")).output().ok()?;
        Some(String::from_utf8_lossy(&out.stdout).trim().to_string())
    }
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

#[test]
fn placeholder_key_refuses_without_the_override() {
    let s = Stand::new(Spec::default());
    let o = s.run(&[]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("no signing key yet"));
    assert!(s.installed_version().is_none());
}

#[test]
fn placeholder_key_installs_with_a_loud_warning_when_overridden() {
    let s = Stand::new(Spec::default());
    let o = s.run(&[("TMGR_ALLOW_UNSIGNED", "1")]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("WITHOUT signature verification"));
    assert_eq!(s.installed_version().as_deref(), Some("tmgr 0.4.0"));
    let leftovers: Vec<_> = std::fs::read_dir(s.tmp.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.starts_with("tmgr-install"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
    let hidden: Vec<_> = std::fs::read_dir(s.dest())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(hidden, vec!["tmgr".to_string()]);
}

#[test]
fn signed_release_installs_with_a_verifier() {
    let s = Stand::new(Spec::default());
    s.with_minisign();
    let o = s.run_keyed(&[]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(String::from_utf8_lossy(&o.stdout).contains("signature verified"));
    assert_eq!(s.installed_version().as_deref(), Some("tmgr 0.4.0"));
}

#[test]
fn real_key_without_minisign_fails_closed() {
    let s = Stand::new(Spec::default());
    let o = s.run_script(&s.keyed_script(), &[], true);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("minisign is required"));
    assert!(s.installed_version().is_none());
    let o = s.run_script(&s.keyed_script(), &[("TMGR_ALLOW_UNSIGNED", "1")], true);
    assert!(o.status.success(), "{}", stderr(&o));
    assert!(stderr(&o).contains("WITHOUT signature verification"));
}

#[test]
fn bad_signature_installs_nothing() {
    let s = Stand::new(Spec {
        signature: Some("BADSIG"),
        ..Spec::default()
    });
    s.with_minisign();
    let o = s.run_keyed(&[]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("signature verification FAILED"));
    assert!(s.installed_version().is_none());
}

#[test]
fn missing_signature_is_refused_when_a_key_is_set() {
    let s = Stand::new(Spec {
        signature: None,
        ..Spec::default()
    });
    s.with_minisign();
    let o = s.run_keyed(&[]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("no signature"), "{}", stderr(&o));
    assert!(s.installed_version().is_none());
}

#[test]
fn expired_manifest_is_refused() {
    let s = Stand::new(Spec {
        expires: "2020-01-01T00:00:00Z",
        ..Spec::default()
    });
    s.with_minisign();
    let o = s.run_keyed(&[]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("expired"));
    assert!(s.installed_version().is_none());
}

#[test]
fn stable_request_refuses_a_beta_manifest() {
    let s = Stand::new(Spec {
        channel: "beta",
        ..Spec::default()
    });
    s.with_minisign();
    let o = s.run_keyed(&[]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("channel"));
}

#[test]
fn archive_not_matching_the_manifest_hash_installs_nothing() {
    let s = Stand::new(Spec {
        tamper_archive: true,
        ..Spec::default()
    });
    s.with_minisign();
    let o = s.run_keyed(&[]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("checksum mismatch"));
    assert!(s.installed_version().is_none());
}

#[test]
fn newer_installed_tmgr_is_not_downgraded() {
    let s = Stand::new(Spec::default());
    s.with_minisign();
    s.preinstall("0.9.0");
    let o = s.run_keyed(&[]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("refusing to downgrade"));
    assert_eq!(s.installed_version().as_deref(), Some("tmgr 0.9.0"));
    let o = s.run_keyed(&[("TMGR_ALLOW_DOWNGRADE", "1")]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(s.installed_version().as_deref(), Some("tmgr 0.4.0"));
}

#[test]
fn older_installed_tmgr_is_upgraded() {
    let s = Stand::new(Spec::default());
    s.with_minisign();
    s.preinstall("0.3.0");
    let o = s.run_keyed(&[]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(s.installed_version().as_deref(), Some("tmgr 0.4.0"));
}

#[test]
fn missing_install_dir_is_created_private_under_a_group_writable_umask() {
    use std::os::unix::fs::PermissionsExt;
    let s = Stand::new(Spec::default());
    s.with_minisign();
    assert!(!s.dest().exists());
    let o = s.run_keyed(&[]);
    assert!(o.status.success(), "{}", stderr(&o));
    assert_eq!(s.installed_version().as_deref(), Some("tmgr 0.4.0"));
    let mode = std::fs::metadata(s.dest()).unwrap().permissions().mode();
    assert_eq!(mode & 0o777, 0o755);
}

#[test]
fn group_writable_install_dir_is_refused() {
    let s = Stand::new(Spec::default());
    s.with_minisign();
    std::fs::create_dir_all(s.dest()).unwrap();
    std::fs::set_permissions(s.dest(), std::fs::Permissions::from_mode(0o775)).unwrap();
    let o = s.run_keyed(&[]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("writable by group or others"));
    assert!(s.installed_version().is_none());
}

#[test]
fn stable_channel_skips_prereleases() {
    let s = Stand::new(Spec {
        prerelease: true,
        channel: "beta",
        ..Spec::default()
    });
    s.with_minisign();
    let o = s.run_keyed(&[]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("no stable release found"));
    let o = s.run_keyed(&[("TMGR_CHANNEL", "beta")]);
    assert!(o.status.success(), "{}", stderr(&o));
}

#[test]
fn windows_points_to_wsl2() {
    let s = Stand::new(Spec::default());
    s.write_shim("uname", "#!/bin/sh\necho MINGW64_NT-10.0\n");
    let o = s.run(&[("TMGR_ALLOW_UNSIGNED", "1")]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("WSL2"));
}

#[test]
fn plain_http_urls_are_refused() {
    let s = Stand::new(Spec::default());
    let o = s.run(&[
        ("TMGR_ALLOW_UNSIGNED", "1"),
        ("TMGR_API_URL", "http://example.com"),
    ]);
    assert!(!o.status.success());
    assert!(stderr(&o).contains("non-https"));
}

#[test]
fn script_is_valid_posix_sh() {
    let o = Command::new("sh").arg("-n").arg(script()).output().unwrap();
    assert!(o.status.success());
}
