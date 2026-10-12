use super::*;
use std::io::Cursor;
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TARGET: &str = "x86_64-unknown-linux-musl";

struct Keys {
    pk: PublicKey,
    sk: minisign::SecretKey,
    pk_box: minisign::PublicKey,
}

fn keys() -> Keys {
    let kp = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
    let pk = PublicKey::decode(&kp.pk.to_box().unwrap().to_string()).unwrap();
    Keys {
        pk,
        sk: kp.sk,
        pk_box: kp.pk,
    }
}

fn sign(k: &Keys, data: &[u8]) -> String {
    minisign::sign(
        Some(&k.pk_box),
        &k.sk,
        Cursor::new(data),
        Some("test-only"),
        None,
    )
    .unwrap()
    .to_string()
}

fn archive(version: &str, binary: &[u8]) -> Vec<u8> {
    let enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut tar = tar::Builder::new(enc);
    for (name, body) in [("README.txt", b"readme".as_slice()), ("tmgr", binary)] {
        let mut h = tar::Header::new_gnu();
        h.set_size(body.len() as u64);
        h.set_mode(0o755);
        h.set_cksum();
        tar.append_data(&mut h, format!("tmgr-{version}-{TARGET}/{name}"), body)
            .unwrap();
    }
    tar.into_inner().unwrap().finish().unwrap()
}

fn manifest_json(version: &str, channel: &str, expires: &str, archive: &[u8]) -> Vec<u8> {
    serde_json::json!({
        "product": "tmgr-cli",
        "version": version,
        "channel": channel,
        "key_epoch": 1,
        "expires_at": expires,
        "artifacts": [{
            "target": TARGET,
            "name": format!("tmgr-{version}-{TARGET}.tar.gz"),
            "sha256": sha256_hex(archive),
            "size": archive.len(),
        }]
    })
    .to_string()
    .into_bytes()
}

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2030-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

fn v(s: &str) -> Version {
    Version::parse(s).unwrap()
}

#[test]
fn placeholder_key_fails_closed() {
    let err = embedded_key().expect_err("placeholder must not load");
    assert_eq!(err.kind, Kind::Forbidden);
    assert_eq!(err.exit_code(), 5);
    assert!(err.message.contains("no update key"));
}

#[test]
fn placeholder_text_is_rejected_and_garbage_is_rejected() {
    assert!(key_from_text(&format!("{PLACEHOLDER_MARK}\nx")).is_err());
    assert!(key_from_text("not a key").is_err());
}

#[test]
fn valid_manifest_verifies() {
    let k = keys();
    let m = manifest_json("0.4.0", "beta", "2031-01-01T00:00:00Z", b"a");
    let parsed = verify_manifest(&m, &sign(&k, &m), &k.pk).unwrap();
    assert_eq!(parsed.version, "0.4.0");
}

#[test]
fn tampered_manifest_is_rejected() {
    let k = keys();
    let m = manifest_json("0.4.0", "beta", "2031-01-01T00:00:00Z", b"a");
    let sig = sign(&k, &m);
    let mut bad = m.clone();
    bad[10] ^= 1;
    let err = verify_manifest(&bad, &sig, &k.pk).unwrap_err();
    assert_eq!(err.kind, Kind::Forbidden);
}

#[test]
fn manifest_signed_by_another_key_is_rejected() {
    let k = keys();
    let other = keys();
    let m = manifest_json("0.4.0", "beta", "2031-01-01T00:00:00Z", b"a");
    assert!(verify_manifest(&m, &sign(&other, &m), &k.pk).is_err());
}

#[test]
fn garbage_signature_is_rejected() {
    let k = keys();
    assert!(verify_manifest(b"{}", "nonsense", &k.pk).is_err());
}

fn manifest_with(f: impl FnOnce(&mut serde_json::Value)) -> Manifest {
    let mut j: serde_json::Value = serde_json::from_slice(&manifest_json(
        "0.4.0",
        "stable",
        "2031-01-01T00:00:00Z",
        b"a",
    ))
    .unwrap();
    f(&mut j);
    serde_json::from_value(j).unwrap()
}

#[test]
fn expired_manifest_is_rejected() {
    let m = manifest_with(|j| j["expires_at"] = "2029-12-31T23:59:59Z".into());
    let err = validate_manifest(&m, &v("0.4.0"), Channel::Stable, now()).unwrap_err();
    assert!(err.message.contains("expired"));
}

#[test]
fn manifest_policy_checks() {
    let ok = manifest_with(|_| {});
    assert_eq!(
        validate_manifest(&ok, &v("0.4.0"), Channel::Stable, now()).unwrap(),
        v("0.4.0")
    );
    let wrong_product = manifest_with(|j| j["product"] = "tmgr-desktop".into());
    assert!(validate_manifest(&wrong_product, &v("0.4.0"), Channel::Stable, now()).is_err());
    let future_epoch = manifest_with(|j| j["key_epoch"] = 2.into());
    assert!(validate_manifest(&future_epoch, &v("0.4.0"), Channel::Stable, now()).is_err());
    assert!(validate_manifest(&ok, &v("0.5.0"), Channel::Stable, now()).is_err());
    let beta = manifest_with(|j| j["channel"] = "beta".into());
    assert!(validate_manifest(&beta, &v("0.4.0"), Channel::Stable, now()).is_err());
    assert!(validate_manifest(&beta, &v("0.4.0"), Channel::Beta, now()).is_ok());
}

fn release(tag: &str, prerelease: bool, draft: bool) -> Release {
    Release {
        tag_name: tag.to_string(),
        prerelease,
        draft,
        assets: vec![],
    }
}

#[test]
fn pick_release_uses_the_cli_prefix_and_channel() {
    let all = || {
        vec![
            release("desktop-v9.9.9", false, false),
            release("cli-v0.3.0", false, false),
            release("cli-v0.4.0", true, false),
            release("cli-v0.5.0-beta.1", true, false),
            release("cli-v0.6.0", false, true),
            release("cli-vbogus", false, false),
        ]
    };
    let stable = pick_release(all(), Channel::Stable).unwrap();
    assert_eq!(stable.1, v("0.3.0"));
    let beta = pick_release(all(), Channel::Beta).unwrap();
    assert_eq!(beta.1, v("0.5.0-beta.1"));
    assert!(pick_release(vec![release("desktop-v1.0.0", false, false)], Channel::Beta).is_none());
}

#[test]
fn decision_refuses_to_go_backwards() {
    assert_eq!(decide(&v("0.3.0"), None, &v("0.4.0")), Decision::Update);
    assert_eq!(decide(&v("0.3.0"), None, &v("0.3.0")), Decision::UpToDate);
    assert_eq!(decide(&v("0.3.0"), None, &v("0.2.0")), Decision::Downgrade);
    assert_eq!(
        decide(&v("0.3.0"), Some(&v("0.5.0")), &v("0.4.0")),
        Decision::Downgrade
    );
    assert_eq!(
        decide(&v("0.3.0"), Some(&v("0.5.0")), &v("0.5.0")),
        Decision::UpToDate
    );
    assert_eq!(
        decide(&v("0.3.0"), Some(&v("0.2.0")), &v("0.4.0")),
        Decision::Update
    );
}

#[test]
fn trust_state_round_trips_with_private_mode() {
    let dir = tempfile::tempdir().unwrap();
    let state = dir.path().join("tmgr");
    assert_eq!(load_trusted(&state).unwrap(), None);
    store_trusted(&state, &v("1.2.3")).unwrap();
    assert_eq!(load_trusted(&state).unwrap(), Some(v("1.2.3")));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(state.join("update.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    std::fs::write(state.join("update.json"), b"garbage").unwrap();
    assert!(load_trusted(&state).is_err());
}

#[test]
fn sha256_check() {
    let h = sha256_hex(b"abc");
    assert!(sha256_matches(b"abc", &h));
    assert!(sha256_matches(b"abc", &h.to_uppercase()));
    assert!(!sha256_matches(b"abd", &h));
    assert!(!sha256_matches(b"abc", "abc"));
}

#[test]
fn extract_takes_only_the_tmgr_binary() {
    let a = archive("0.4.0", b"BINARY");
    let out = extract_binary(&a, &format!("tmgr-0.4.0-{TARGET}")).unwrap();
    assert_eq!(out, b"BINARY");
    assert!(extract_binary(&a, "tmgr-9.9.9-other").is_err());
    assert!(extract_binary(b"not gzip", "x").is_err());
}

#[test]
fn extract_ignores_traversal_entries_and_symlinks() {
    let enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut tar = tar::Builder::new(enc);
    let mut link = tar::Header::new_gnu();
    link.set_entry_type(tar::EntryType::Symlink);
    link.set_size(0);
    link.set_cksum();
    tar.append_link(&mut link, "pkg/tmgr", "/etc/passwd")
        .unwrap();
    let a = tar.into_inner().unwrap().finish().unwrap();
    assert!(extract_binary(&a, "pkg").is_err());

    let enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut tar = tar::Builder::new(enc);
    let mut h = tar::Header::new_gnu();
    h.set_size(1);
    let raw = b"pkg/../../tmgr";
    h.as_old_mut().name[..raw.len()].copy_from_slice(raw);
    h.set_cksum();
    tar.append(&h, &b"x"[..]).unwrap();
    let a = tar.into_inner().unwrap().finish().unwrap();
    assert!(extract_binary(&a, "pkg").is_err());
}

#[test]
fn package_managers_are_detected() {
    let hit = |p: &str| detect_managed(Path::new(p)).map(|m| m.manager);
    assert_eq!(hit("/nix/store/abc-tmgr/bin/tmgr"), Some("Nix"));
    assert_eq!(
        hit("/opt/homebrew/Cellar/tmgr/1.0/bin/tmgr"),
        Some("Homebrew")
    );
    assert_eq!(hit("/home/u/.cargo/bin/tmgr"), Some("cargo"));
    assert_eq!(hit("/usr/lib/node_modules/@x/tmgr/bin/tmgr"), Some("npm"));
    assert_eq!(hit("/home/u/.local/bin/tmgr"), None);
}

#[cfg(unix)]
#[test]
fn install_replaces_atomically_and_keeps_one_backup() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let exe = dir.path().join("tmgr");
    std::fs::write(&exe, b"OLD").unwrap();
    let old = install_binary(&exe, b"NEW").unwrap();
    assert_eq!(std::fs::read(&exe).unwrap(), b"NEW");
    assert_eq!(std::fs::read(&old).unwrap(), b"OLD");
    assert_eq!(
        std::fs::metadata(&exe).unwrap().permissions().mode() & 0o777,
        0o755
    );
    install_binary(&exe, b"NEWER").unwrap();
    assert_eq!(std::fs::read(&old).unwrap(), b"NEW");
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(leftovers.len(), 2, "{leftovers:?}");
}

struct Stand {
    rt: tokio::runtime::Runtime,
    server: MockServer,
    dir: tempfile::TempDir,
    keys: Keys,
}

impl Stand {
    fn new() -> Stand {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let server = rt.block_on(MockServer::start());
        Stand {
            rt,
            server,
            dir: tempfile::tempdir().unwrap(),
            keys: keys(),
        }
    }

    fn exe(&self) -> PathBuf {
        self.dir.path().join("bin").join("tmgr")
    }

    fn mount(&self, p: &str, body: Vec<u8>) {
        self.rt.block_on(
            Mock::given(method("GET"))
                .and(path(p.to_string()))
                .respond_with(ResponseTemplate::new(200).set_body_bytes(body))
                .mount(&self.server),
        );
    }

    fn publish(&self, version: &str, prerelease: bool, manifest: &[u8], sig: &str, arch: &[u8]) {
        let base = self.server.uri();
        let tag = format!("cli-v{version}");
        let name = format!("tmgr-{version}-{TARGET}.tar.gz");
        let listing = serde_json::json!([
            {"tag_name": "desktop-v9.0.0", "prerelease": false, "assets": []},
            {"tag_name": tag, "prerelease": prerelease, "assets": [
                {"name": "manifest.json", "browser_download_url": format!("{base}/dl/{tag}/manifest.json")},
                {"name": "manifest.json.minisig", "browser_download_url": format!("{base}/dl/{tag}/manifest.json.minisig")},
                {"name": name, "browser_download_url": format!("{base}/dl/{tag}/{name}")},
            ]}
        ]);
        self.rt.block_on(
            Mock::given(method("GET"))
                .and(path(format!("/repos/{REPO}/releases")))
                .and(query_param("page", "1"))
                .respond_with(ResponseTemplate::new(200).set_body_json(listing))
                .mount(&self.server),
        );
        self.mount(&format!("/dl/{tag}/manifest.json"), manifest.to_vec());
        self.mount(
            &format!("/dl/{tag}/manifest.json.minisig"),
            sig.as_bytes().to_vec(),
        );
        self.mount(&format!("/dl/{tag}/{name}"), arch.to_vec());
    }

    fn publish_good(&self, version: &str) {
        let arch = archive(version, b"NEWBIN");
        let m = manifest_json(version, "beta", "2031-01-01T00:00:00Z", &arch);
        let sig = sign(&self.keys, &m);
        self.publish(version, true, &m, &sig, &arch);
    }

    fn opts(&self, current: &str) -> Options {
        let exe = self.exe();
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"OLDBIN").unwrap();
        Options {
            api_base: self.server.uri(),
            channel: Channel::Beta,
            check_only: false,
            allow_downgrade: false,
            current: v(current),
            target: TARGET.to_string(),
            exe,
            state_dir: self.dir.path().join("state"),
            now: now(),
            user_agent: "tmgr-test".into(),
        }
    }
}

#[test]
fn end_to_end_update_installs_and_records_trust() {
    let s = Stand::new();
    s.publish_good("0.4.0");
    let opts = s.opts("0.3.0");
    let r = run(&opts, &s.keys.pk).unwrap();
    assert!(r.installed);
    assert_eq!(r.latest.as_deref(), Some("0.4.0"));
    assert_eq!(std::fs::read(&opts.exe).unwrap(), b"NEWBIN");
    assert_eq!(
        std::fs::read(opts.exe.with_file_name("tmgr.old")).unwrap(),
        b"OLDBIN"
    );
    assert_eq!(load_trusted(&opts.state_dir).unwrap(), Some(v("0.4.0")));
}

#[test]
fn check_reports_without_touching_anything() {
    let s = Stand::new();
    s.publish_good("0.4.0");
    let mut opts = s.opts("0.3.0");
    opts.check_only = true;
    let r = run(&opts, &s.keys.pk).unwrap();
    assert!(r.update_available && !r.installed);
    assert_eq!(std::fs::read(&opts.exe).unwrap(), b"OLDBIN");
    assert_eq!(load_trusted(&opts.state_dir).unwrap(), None);
}

#[test]
fn up_to_date_installs_nothing() {
    let s = Stand::new();
    s.publish_good("0.4.0");
    let opts = s.opts("0.4.0");
    let r = run(&opts, &s.keys.pk).unwrap();
    assert!(!r.installed && !r.update_available);
    assert_eq!(std::fs::read(&opts.exe).unwrap(), b"OLDBIN");
}

#[test]
fn downgrade_is_refused_unless_allowed() {
    let s = Stand::new();
    s.publish_good("0.4.0");
    let mut opts = s.opts("0.5.0");
    let err = run(&opts, &s.keys.pk).unwrap_err();
    assert_eq!(err.kind, Kind::Conflict);
    assert_eq!(std::fs::read(&opts.exe).unwrap(), b"OLDBIN");
    opts.allow_downgrade = true;
    assert!(run(&opts, &s.keys.pk).unwrap().installed);
}

#[test]
fn trusted_floor_blocks_a_replayed_older_release() {
    let s = Stand::new();
    s.publish_good("0.4.0");
    let opts = s.opts("0.3.0");
    store_trusted(&opts.state_dir, &v("0.9.0")).unwrap();
    let err = run(&opts, &s.keys.pk).unwrap_err();
    assert_eq!(err.kind, Kind::Conflict);
    assert_eq!(std::fs::read(&opts.exe).unwrap(), b"OLDBIN");
}

#[test]
fn tampered_archive_is_not_installed() {
    let s = Stand::new();
    let good = archive("0.4.0", b"NEWBIN");
    let m = manifest_json("0.4.0", "beta", "2031-01-01T00:00:00Z", &good);
    let sig = sign(&s.keys, &m);
    let mut evil = good.clone();
    let mid = evil.len() / 2;
    evil[mid] ^= 0xff;
    s.publish("0.4.0", true, &m, &sig, &evil);
    let opts = s.opts("0.3.0");
    let err = run(&opts, &s.keys.pk).unwrap_err();
    assert_eq!(err.kind, Kind::Forbidden);
    assert!(err.message.contains("checksum"));
    assert_eq!(std::fs::read(&opts.exe).unwrap(), b"OLDBIN");
}

#[test]
fn tampered_manifest_is_not_trusted() {
    let s = Stand::new();
    let arch = archive("0.4.0", b"NEWBIN");
    let m = manifest_json("0.4.0", "beta", "2031-01-01T00:00:00Z", &arch);
    let sig = sign(&s.keys, &m);
    let forged = manifest_json("0.4.0", "beta", "2099-01-01T00:00:00Z", &arch);
    s.publish("0.4.0", true, &forged, &sig, &arch);
    let opts = s.opts("0.3.0");
    let err = run(&opts, &s.keys.pk).unwrap_err();
    assert!(err.message.contains("signature"));
    assert_eq!(std::fs::read(&opts.exe).unwrap(), b"OLDBIN");
}

#[test]
fn expired_manifest_stops_the_update() {
    let s = Stand::new();
    let arch = archive("0.4.0", b"NEWBIN");
    let m = manifest_json("0.4.0", "beta", "2020-01-01T00:00:00Z", &arch);
    let sig = sign(&s.keys, &m);
    s.publish("0.4.0", true, &m, &sig, &arch);
    let opts = s.opts("0.3.0");
    let err = run(&opts, &s.keys.pk).unwrap_err();
    assert!(err.message.contains("expired"));
}

#[test]
fn stable_channel_ignores_prereleases() {
    let s = Stand::new();
    s.publish_good("0.4.0");
    let mut opts = s.opts("0.3.0");
    opts.channel = Channel::Stable;
    let err = run(&opts, &s.keys.pk).unwrap_err();
    assert_eq!(err.kind, Kind::NotFound);
    assert!(err.hint.unwrap().contains("beta"));
}

#[test]
fn unsigned_release_is_refused() {
    let s = Stand::new();
    let base = s.server.uri();
    let listing = serde_json::json!([{"tag_name": "cli-v0.4.0", "prerelease": true, "assets": [
        {"name": "SHA256SUMS", "browser_download_url": format!("{base}/dl/x")}
    ]}]);
    s.rt.block_on(
        Mock::given(method("GET"))
            .and(path(format!("/repos/{REPO}/releases")))
            .respond_with(ResponseTemplate::new(200).set_body_json(listing))
            .mount(&s.server),
    );
    let err = run(&s.opts("0.3.0"), &s.keys.pk).unwrap_err();
    assert_eq!(err.kind, Kind::Forbidden);
}

#[cfg(unix)]
#[test]
fn managed_install_is_refused_with_a_hint() {
    let s = Stand::new();
    s.publish_good("0.4.0");
    let mut opts = s.opts("0.3.0");
    let cellar = s.dir.path().join("Cellar").join("tmgr").join("bin");
    std::fs::create_dir_all(&cellar).unwrap();
    opts.exe = cellar.join("tmgr");
    std::fs::write(&opts.exe, b"OLDBIN").unwrap();
    let err = run(&opts, &s.keys.pk).unwrap_err();
    assert_eq!(err.kind, Kind::Conflict);
    assert_eq!(err.hint.as_deref(), Some("brew upgrade tmgr"));
}
