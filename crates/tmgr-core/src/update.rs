use crate::config::{ensure_private_dir, write_atomic};
use crate::error::{Error, Kind, Result};
use crate::pages::sha256_hex;
use chrono::{DateTime, Utc};
use minisign_verify::{PublicKey, Signature};
pub use semver::Version;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const DEFAULT_API: &str = "https://api.github.com";
pub const REPO: &str = "tmgr-dev/frontend";
pub const TAG_PREFIX: &str = "cli-v";
pub const PRODUCT: &str = "tmgr-cli";
pub const SUPPORTED_KEY_EPOCH: u32 = 1;
pub const PLACEHOLDER_MARK: &str = "TMGR-UPDATE-KEY-PLACEHOLDER";
const EMBEDDED_KEY: &str = include_str!("update_key.pub");
const STATE_SCHEMA: &str = "tmgr.update.v1";
const MAX_MANIFEST: u64 = 256 * 1024;
const MAX_LISTING: u64 = 4 * 1024 * 1024;
const MAX_ARCHIVE: u64 = 120 * 1024 * 1024;
const MAX_BINARY: u64 = 200 * 1024 * 1024;
const MAX_PAGES: u32 = 3;

pub fn embedded_key() -> Result<PublicKey> {
    key_from_text(EMBEDDED_KEY)
}

pub fn key_from_text(text: &str) -> Result<PublicKey> {
    if text.contains(PLACEHOLDER_MARK) {
        return Err(Error::new(
            Kind::Forbidden,
            "this build has no update key; install a signed release",
        )
        .hint("curl -fsSL https://tmgr.dev/install.sh | sh"));
    }
    PublicKey::decode(text).map_err(|_| {
        Error::new(
            Kind::Forbidden,
            "this build has an unreadable update key; install a signed release",
        )
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Channel {
    Stable,
    Beta,
}

impl Channel {
    pub fn name(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Beta => "beta",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Artifact {
    pub target: String,
    pub name: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub product: String,
    pub version: String,
    pub channel: String,
    pub key_epoch: u32,
    pub expires_at: String,
    pub artifacts: Vec<Artifact>,
}

fn untrusted(message: impl AsRef<str>) -> Error {
    Error::new(Kind::Forbidden, message)
}

pub fn verify_manifest(bytes: &[u8], signature: &str, key: &PublicKey) -> Result<Manifest> {
    let sig = Signature::decode(signature)
        .map_err(|_| untrusted("the manifest signature is malformed"))?;
    key.verify(bytes, &sig, true)
        .map_err(|_| untrusted("the manifest signature does not verify; refusing to update"))?;
    serde_json::from_slice(bytes).map_err(|_| untrusted("the signed manifest is not valid"))
}

pub fn validate_manifest(
    m: &Manifest,
    tag_version: &Version,
    channel: Channel,
    now: DateTime<Utc>,
) -> Result<Version> {
    if m.product != PRODUCT {
        return Err(untrusted("the manifest is for a different product"));
    }
    if m.key_epoch != SUPPORTED_KEY_EPOCH {
        return Err(untrusted(format!(
            "the manifest uses key epoch {}, this build understands {}",
            m.key_epoch, SUPPORTED_KEY_EPOCH
        ))
        .hint("install a newer release with https://tmgr.dev/install.sh"));
    }
    let version = Version::parse(&m.version)
        .map_err(|_| untrusted("the manifest carries an invalid version"))?;
    if &version != tag_version {
        return Err(untrusted(
            "the manifest version does not match the release tag",
        ));
    }
    let expires = DateTime::parse_from_rfc3339(&m.expires_at)
        .map_err(|_| untrusted("the manifest carries an invalid expiry"))?;
    if expires.with_timezone(&Utc) <= now {
        return Err(untrusted(
            "the manifest has expired; refusing a stale release",
        ));
    }
    let ok = match channel {
        Channel::Stable => m.channel == "stable",
        Channel::Beta => m.channel == "stable" || m.channel == "beta",
    };
    if !ok {
        return Err(untrusted(format!(
            "the release is on channel {:?}, not {}",
            m.channel,
            channel.name()
        )));
    }
    Ok(version)
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub tag_name: String,
    #[serde(default)]
    pub prerelease: bool,
    #[serde(default)]
    pub draft: bool,
    #[serde(default)]
    pub assets: Vec<ReleaseAsset>,
}

impl Release {
    fn version(&self) -> Option<Version> {
        Version::parse(self.tag_name.strip_prefix(TAG_PREFIX)?).ok()
    }

    fn asset(&self, name: &str) -> Option<&ReleaseAsset> {
        self.assets.iter().find(|a| a.name == name)
    }
}

pub fn pick_release(releases: Vec<Release>, channel: Channel) -> Option<(Release, Version)> {
    releases
        .into_iter()
        .filter(|r| !r.draft)
        .filter_map(|r| r.version().map(|v| (r, v)))
        .filter(|(r, v)| match channel {
            Channel::Stable => !r.prerelease && v.pre.is_empty(),
            Channel::Beta => true,
        })
        .max_by(|a, b| a.1.cmp(&b.1))
}

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    UpToDate,
    Update,
    Older,
    Rollback,
}

pub fn decide(current: &Version, trusted: Option<&Version>, candidate: &Version) -> Decision {
    if trusted.is_some_and(|t| candidate < t) {
        return Decision::Rollback;
    }
    match candidate.cmp(current) {
        std::cmp::Ordering::Equal => Decision::UpToDate,
        std::cmp::Ordering::Greater => Decision::Update,
        std::cmp::Ordering::Less => Decision::Older,
    }
}

#[derive(Serialize, Deserialize)]
struct TrustState {
    schema: String,
    highest_version: String,
}

fn state_file(dir: &Path) -> PathBuf {
    dir.join("update.json")
}

pub fn load_trusted(dir: &Path) -> Result<Option<Version>> {
    let path = state_file(dir);
    let file = match std::fs::File::open(&path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut buf = Vec::new();
    file.take(4097).read_to_end(&mut buf)?;
    let bad = || {
        Error::internal("the update state file is unreadable")
            .hint(format!("remove {} to reset it", path.display()))
    };
    if buf.len() > 4096 {
        return Err(bad());
    }
    let state: TrustState = serde_json::from_slice(&buf).map_err(|_| bad())?;
    if state.schema != STATE_SCHEMA {
        return Err(bad());
    }
    Version::parse(&state.highest_version)
        .map(Some)
        .map_err(|_| bad())
}

pub fn store_trusted(dir: &Path, version: &Version) -> Result<()> {
    ensure_private_dir(dir)?;
    let bytes = serde_json::to_vec_pretty(&TrustState {
        schema: STATE_SCHEMA.to_string(),
        highest_version: version.to_string(),
    })
    .map_err(|_| Error::internal("cannot serialize the update state"))?;
    write_atomic(&state_file(dir), &bytes, 0o600)
}

pub fn sha256_matches(bytes: &[u8], expected_hex: &str) -> bool {
    let expected = expected_hex.trim().to_ascii_lowercase();
    expected.len() == 64 && sha256_hex(bytes) == expected
}

pub fn extract_binary(archive: &[u8], package_dir: &str) -> Result<Vec<u8>> {
    let wanted = format!("{package_dir}/tmgr");
    let decoder = flate2::read::GzDecoder::new(archive);
    let mut tar = tar::Archive::new(decoder);
    let entries = tar
        .entries()
        .map_err(|_| untrusted("the archive is not a valid tar.gz"))?;
    for entry in entries {
        let entry = entry.map_err(|_| untrusted("the archive is not a valid tar.gz"))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let is_wanted = entry
            .path()
            .ok()
            .and_then(|p| p.to_str().map(|s| s == wanted))
            .unwrap_or(false);
        if !is_wanted {
            continue;
        }
        if entry.header().size().unwrap_or(u64::MAX) > MAX_BINARY {
            return Err(untrusted("the binary in the archive is too large"));
        }
        let mut out = Vec::new();
        entry.take(MAX_BINARY + 1).read_to_end(&mut out)?;
        if out.len() as u64 > MAX_BINARY || out.is_empty() {
            return Err(untrusted("the binary in the archive has an invalid size"));
        }
        return Ok(out);
    }
    Err(untrusted("the archive does not contain the tmgr binary"))
}

#[derive(Debug, PartialEq, Eq)]
pub struct Managed {
    pub manager: &'static str,
    pub command: &'static str,
}

pub fn detect_managed(exe: &Path) -> Option<Managed> {
    let s = exe.to_string_lossy();
    if s.starts_with("/nix/store/") {
        return Some(Managed {
            manager: "Nix",
            command: "nix profile upgrade tmgr-cli",
        });
    }
    if s.contains("/Cellar/") || s.contains("/Caskroom/") || s.contains("/linuxbrew/") {
        return Some(Managed {
            manager: "Homebrew",
            command: "brew upgrade tmgr",
        });
    }
    if s.contains("/.cargo/bin/") {
        return Some(Managed {
            manager: "cargo",
            command:
                "cargo install --locked --force --git https://github.com/tmgr-dev/frontend tmgr-cli",
        });
    }
    if s.contains("/node_modules/") || s.contains("/.npm/") {
        return Some(Managed {
            manager: "npm",
            command: "npm update -g (the package you installed it from)",
        });
    }
    None
}

#[cfg(unix)]
pub fn install_binary(exe: &Path, bytes: &[u8]) -> Result<PathBuf> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let dir = exe
        .parent()
        .ok_or_else(|| Error::internal("cannot locate the install directory"))?;
    let name = exe
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| Error::internal("cannot read the binary name"))?;
    let new = dir.join(format!(".{name}.new.{}", std::process::id()));
    let old = dir.join(format!("{name}.old"));
    let denied = |e: std::io::Error| {
        if e.kind() == std::io::ErrorKind::PermissionDenied {
            Error::new(
                Kind::Forbidden,
                "no permission to write the install directory",
            )
            .hint("tmgr never uses sudo: reinstall to a directory you own with https://tmgr.dev/install.sh")
        } else {
            Error::from(e)
        }
    };
    let result = (|| -> Result<()> {
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o755)
            .open(&new)
            .map_err(denied)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        std::fs::set_permissions(&new, std::fs::Permissions::from_mode(0o755))?;
        let _ = std::fs::remove_file(&old);
        if std::fs::hard_link(exe, &old).is_err() {
            std::fs::copy(exe, &old)?;
        }
        std::fs::rename(&new, exe)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&new);
    }
    result?;
    if let Ok(d) = std::fs::File::open(dir) {
        let _ = d.sync_all();
    }
    Ok(old)
}

#[cfg(not(unix))]
pub fn install_binary(_exe: &Path, _bytes: &[u8]) -> Result<PathBuf> {
    Err(Error::internal(
        "self-update is not supported on this platform",
    ))
}

fn allowed_url(url: &reqwest::Url) -> bool {
    match url.scheme() {
        "https" => true,
        "http" => matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")),
        _ => false,
    }
}

fn http_client(user_agent: &str) -> Result<reqwest::blocking::Client> {
    let policy = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.previous().len() >= 5 || !allowed_url(attempt.url()) {
            attempt.stop()
        } else {
            attempt.follow()
        }
    });
    reqwest::blocking::Client::builder()
        .user_agent(user_agent)
        .redirect(policy)
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|_| Error::internal("cannot build the HTTP client"))
}

fn fetch(client: &reqwest::blocking::Client, url: &str, cap: u64) -> Result<Vec<u8>> {
    let parsed = reqwest::Url::parse(url).map_err(|_| Error::internal("invalid download URL"))?;
    if !allowed_url(&parsed) {
        return Err(untrusted("refusing a download over an insecure URL"));
    }
    let host = parsed.host_str().unwrap_or("the server").to_string();
    let resp = client.get(parsed).send().map_err(|_| {
        Error::new(Kind::Offline, format!("cannot reach {host}"))
            .hint("check your connection and try again")
    })?;
    let status = resp.status().as_u16();
    if status == 403 || status == 429 {
        return Err(Error::new(
            Kind::Offline,
            format!("{host} refused the request (HTTP {status}, rate limit?)"),
        )
        .hint("try again later"));
    }
    if status != 200 {
        return Err(Error::new(
            Kind::Offline,
            format!("{host} answered HTTP {status}"),
        ));
    }
    let mut out = Vec::new();
    resp.take(cap + 1).read_to_end(&mut out).map_err(|_| {
        Error::new(Kind::Offline, "connection lost during the download")
            .hint("check your connection and try again")
    })?;
    if out.len() as u64 > cap {
        return Err(untrusted("the download is larger than allowed"));
    }
    Ok(out)
}

fn list_releases(client: &reqwest::blocking::Client, api_base: &str) -> Result<Vec<Release>> {
    let mut all = Vec::new();
    for page in 1..=MAX_PAGES {
        let url = format!(
            "{}/repos/{REPO}/releases?per_page=100&page={page}",
            api_base.trim_end_matches('/')
        );
        let body = fetch(client, &url, MAX_LISTING)?;
        let batch: Vec<Release> = serde_json::from_slice(&body)
            .map_err(|_| Error::internal("unexpected answer from the releases API"))?;
        let full = batch.len() >= 100;
        let has_cli = batch.iter().any(|r| r.tag_name.starts_with(TAG_PREFIX));
        all.extend(batch);
        if !full || has_cli {
            break;
        }
    }
    Ok(all)
}

pub struct Options {
    pub api_base: String,
    pub channel: Channel,
    pub check_only: bool,
    pub allow_downgrade: bool,
    pub current: Version,
    pub target: String,
    pub exe: PathBuf,
    pub state_dir: PathBuf,
    pub now: DateTime<Utc>,
    pub user_agent: String,
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub current: String,
    pub latest: Option<String>,
    pub channel: &'static str,
    pub update_available: bool,
    pub installed: bool,
    pub backup: Option<String>,
    pub expires_at: Option<String>,
}

pub fn run(opts: &Options, key: &PublicKey) -> Result<Report> {
    let client = http_client(&opts.user_agent)?;
    let report = |latest: Option<String>, expires_at: Option<String>| Report {
        current: opts.current.to_string(),
        latest,
        channel: opts.channel.name(),
        update_available: false,
        installed: false,
        backup: None,
        expires_at,
    };
    let releases = list_releases(&client, &opts.api_base)?;
    let Some((release, version)) = pick_release(releases, opts.channel) else {
        let mut e = Error::not_found(format!("no {} release found", opts.channel.name()));
        if opts.channel == Channel::Stable {
            e = e.hint("try --channel beta");
        }
        return Err(e);
    };
    let manifest_asset = release
        .asset("manifest.json")
        .ok_or_else(|| untrusted("the release has no manifest.json; refusing to update"))?;
    let sig_asset = release
        .asset("manifest.json.minisig")
        .ok_or_else(|| untrusted("the release is not signed (no manifest.json.minisig)"))?;
    let manifest_bytes = fetch(&client, &manifest_asset.browser_download_url, MAX_MANIFEST)?;
    let sig_bytes = fetch(&client, &sig_asset.browser_download_url, MAX_MANIFEST)?;
    let sig_text = String::from_utf8(sig_bytes)
        .map_err(|_| untrusted("the manifest signature is malformed"))?;
    let manifest = verify_manifest(&manifest_bytes, &sig_text, key)?;
    let version = validate_manifest(&manifest, &version, opts.channel, opts.now)?;

    let trusted = load_trusted(&opts.state_dir)?;
    let decision = decide(&opts.current, trusted.as_ref(), &version);
    let latest = Some(version.to_string());
    let expires = Some(manifest.expires_at.clone());
    match decision {
        Decision::UpToDate => return Ok(report(latest, expires)),
        Decision::Older if !opts.allow_downgrade => return Ok(report(latest, expires)),
        Decision::Rollback if !opts.allow_downgrade => {
            return Err(Error::conflict(format!(
                "version {version} is older than what this machine already trusts; refusing to downgrade"
            ))
            .hint("pass --allow-downgrade in a terminal if this is intended"));
        }
        _ => {}
    }
    if opts.check_only {
        return Ok(Report {
            update_available: true,
            ..report(latest, expires)
        });
    }
    let exe = std::fs::canonicalize(&opts.exe).unwrap_or_else(|_| opts.exe.clone());
    if let Some(m) = detect_managed(&exe).or_else(|| detect_managed(&opts.exe)) {
        return Err(Error::conflict(format!(
            "tmgr is managed by {}; self-update would fight it",
            m.manager
        ))
        .hint(m.command));
    }
    let package = format!("tmgr-{version}-{}", opts.target);
    let artifact = manifest
        .artifacts
        .iter()
        .find(|a| a.target == opts.target)
        .ok_or_else(|| {
            Error::not_found(format!(
                "release {version} has no build for {}",
                opts.target
            ))
        })?;
    if artifact.name != format!("{package}.tar.gz") {
        return Err(untrusted("the manifest names an unexpected archive"));
    }
    if artifact.size == 0 || artifact.size > MAX_ARCHIVE {
        return Err(untrusted("the manifest declares an invalid archive size"));
    }
    let archive_asset = release.asset(&artifact.name).ok_or_else(|| {
        untrusted("the release does not contain the archive named in the manifest")
    })?;
    let archive = fetch(&client, &archive_asset.browser_download_url, artifact.size)?;
    if archive.len() as u64 != artifact.size || !sha256_matches(&archive, &artifact.sha256) {
        return Err(untrusted(
            "the archive does not match the signed checksum; nothing was installed",
        ));
    }
    let binary = extract_binary(&archive, &package)?;
    let backup = install_binary(&exe, &binary)?;
    let highest = match &trusted {
        Some(t) if t > &version => t.clone(),
        _ => version.clone(),
    };
    store_trusted(&opts.state_dir, &highest)?;
    Ok(Report {
        installed: true,
        backup: Some(backup.display().to_string()),
        ..report(latest, expires)
    })
}

#[cfg(test)]
mod tests;
