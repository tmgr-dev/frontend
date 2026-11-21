use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use tmgr_spawn_auth::{
    ApproveError, DeviceFp, DeviceKeys, KeyBook, KeyState, OfferOutcome, PinnedKey, Refusal,
    RevokeError, RevokedKey, SpawnKeyStatement, TrustStatus, TrustView,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyError {
    Refused(Refusal),
    Approve(ApproveError),
    Revoke(RevokeError),
    Io,
    Corrupt,
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyError::Refused(r) => write!(f, "spawn key refused: {}", r.code()),
            KeyError::Approve(ApproveError::PrefixTooShort) => {
                f.write_str("fingerprint prefix is too short")
            }
            KeyError::Approve(ApproveError::NotFound) => {
                f.write_str("no pending spawn key matches the prefix")
            }
            KeyError::Approve(ApproveError::Ambiguous) => {
                f.write_str("fingerprint prefix matches several pending spawn keys")
            }
            KeyError::Approve(ApproveError::KeyMismatch) => {
                f.write_str("the pending spawn key is not the one you confirmed")
            }
            KeyError::Approve(ApproveError::TooSoon) => {
                f.write_str("the spawn key was just replaced; wait a minute and check it again")
            }
            KeyError::Revoke(RevokeError::PrefixTooShort) => {
                f.write_str("fingerprint prefix is too short")
            }
            KeyError::Revoke(RevokeError::NotFound) => {
                f.write_str("no spawn key matches the prefix")
            }
            KeyError::Revoke(RevokeError::Ambiguous) => {
                f.write_str("fingerprint prefix matches several spawn keys")
            }
            KeyError::Io => f.write_str("spawn key store io error"),
            KeyError::Corrupt => f.write_str("spawn key file is corrupt"),
        }
    }
}

impl std::error::Error for KeyError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeyOffer {
    pub statement: String,
    pub sig: String,
    pub spawn_sig: String,
    pub device_id: String,
    pub sign_pub_b64: String,
    pub kex_pub_b64: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingKey {
    pub device_fp: DeviceFp,
    pub device_id: String,
    pub spawn_key_fp: String,
    pub issued_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyInfo {
    pub device_fp: DeviceFp,
    pub device_id: String,
    pub spawn_key_fp: String,
    pub state: KeyState,
    pub issued_at_ms: u64,
    pub replaced_at_ms: Option<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    #[serde(flatten)]
    offer: KeyOffer,
    approved: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    replaced_at_ms: Option<u64>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeysFile {
    v: u32,
    keys: BTreeMap<String, Stored>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    revoked: Vec<String>,
}

struct Loaded {
    book: KeyBook,
    offers: BTreeMap<String, KeyOffer>,
}

struct IntegrityOnly;

impl TrustView for IntegrityOnly {
    fn status(&self, _: &str, _: &str) -> TrustStatus {
        TrustStatus::Trusted
    }
}

pub struct KeyStoreFile {
    path: PathBuf,
    lock_path: PathBuf,
    user_id: String,
}

const MAX_KEYS_BYTES: u64 = 1024 * 1024;

fn verify_offer(
    user_id: &str,
    o: &KeyOffer,
    trust: &dyn TrustView,
    now_ms: u64,
) -> Result<SpawnKeyStatement, Refusal> {
    let device = DeviceKeys {
        device_id: &o.device_id,
        sign_pub_b64: &o.sign_pub_b64,
        kex_pub_b64: &o.kex_pub_b64,
    };
    tmgr_spawn_auth::verify_spawn_key_statement(
        user_id,
        &o.statement,
        &o.sig,
        &o.spawn_sig,
        &device,
        trust,
        now_ms,
    )
}

impl KeyStoreFile {
    pub fn open(path: &Path, user_id: &str, trust: &dyn TrustView) -> Result<Self, KeyError> {
        let parent = path.parent().ok_or(KeyError::Io)?;
        super::ensure_private_dir(parent).map_err(|_| KeyError::Io)?;
        let store = Self {
            path: path.into(),
            lock_path: parent.join(".keys.lock"),
            user_id: user_id.into(),
        };
        let _lock = store.lock()?;
        store.remove_temp_files()?;
        let (loaded, revoked_cleared) = store.load(Some(trust))?;
        if revoked_cleared {
            store.persist(&loaded)?;
        }
        Ok(store)
    }

    pub fn offer(
        &self,
        offer: KeyOffer,
        trust: &dyn TrustView,
        now_ms: u64,
    ) -> Result<OfferOutcome, KeyError> {
        let stmt = verify_offer(&self.user_id, &offer, trust, now_ms).map_err(KeyError::Refused)?;
        let fp = stmt.device_fp().to_string();
        let _lock = self.lock()?;
        let (mut loaded, _) = self.load(None)?;
        let outcome = loaded.book.offer(stmt, now_ms);
        if matches!(outcome, OfferOutcome::Added | OfferOutcome::Replaced) {
            loaded.offers.insert(fp, offer);
            self.persist(&loaded)?;
        }
        Ok(outcome)
    }

    pub fn approve(
        &self,
        fp_prefix: &str,
        spawn_key_prefix: &str,
        now_ms: u64,
    ) -> Result<DeviceFp, KeyError> {
        let _lock = self.lock()?;
        let (mut loaded, _) = self.load(None)?;
        let fp = match loaded.book.approve(fp_prefix, spawn_key_prefix, now_ms) {
            Ok(fp) => fp,
            Err(e) => {
                if e == ApproveError::TooSoon {
                    let _ = self.persist(&loaded);
                }
                return Err(KeyError::Approve(e));
            }
        };
        self.persist(&loaded)?;
        Ok(fp)
    }

    pub fn revoke(&self, fp_prefix: &str) -> Result<RevokedKey, KeyError> {
        let _lock = self.lock()?;
        let (mut loaded, _) = self.load(None)?;
        let revoked = loaded.book.revoke(fp_prefix).map_err(KeyError::Revoke)?;
        loaded.offers.remove(&revoked.device_fp);
        self.persist(&loaded)?;
        Ok(revoked)
    }

    pub fn list(&self) -> Result<Vec<KeyInfo>, KeyError> {
        let _lock = self.lock()?;
        let (loaded, _) = self.load(None)?;
        Ok(loaded
            .book
            .all()
            .into_iter()
            .map(|v| KeyInfo {
                device_fp: v.statement.device_fp().into(),
                device_id: v.statement.device_id().into(),
                spawn_key_fp: v.statement.spawn_key_fp(),
                state: v.state,
                issued_at_ms: v.statement.issued_at_ms(),
                replaced_at_ms: v.replaced_at_ms,
            })
            .collect())
    }

    pub fn with_pinned<R>(
        &self,
        device_fp: &str,
        f: impl FnOnce(Option<PinnedKey>) -> R,
    ) -> Result<R, KeyError> {
        let _lock = self.lock()?;
        let (loaded, _) = self.load(None)?;
        Ok(f(loaded.book.pinned_key(&self.user_id, device_fp)))
    }

    pub fn pinned_key(&self, device_fp: &str) -> Result<Option<PinnedKey>, KeyError> {
        self.with_pinned(device_fp, |k| k)
    }

    pub fn state(&self, device_fp: &str) -> Result<Option<KeyState>, KeyError> {
        let _lock = self.lock()?;
        Ok(self.load(None)?.0.book.state(device_fp))
    }

    pub fn pending(&self) -> Result<Vec<PendingKey>, KeyError> {
        let _lock = self.lock()?;
        let (loaded, _) = self.load(None)?;
        Ok(loaded
            .book
            .pending()
            .into_iter()
            .map(|s| PendingKey {
                device_fp: s.device_fp().into(),
                device_id: s.device_id().into(),
                spawn_key_fp: s.spawn_key_fp(),
                issued_at_ms: s.issued_at_ms(),
            })
            .collect())
    }

    pub fn recover(&self) -> Result<(), KeyError> {
        let _lock = self.lock()?;
        self.remove_temp_files()
    }

    fn remove_temp_files(&self) -> Result<(), KeyError> {
        let dir = self.path.parent().ok_or(KeyError::Io)?;
        let name = self
            .path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let prefix = format!(".{name}.");
        for entry in std::fs::read_dir(dir).map_err(|_| KeyError::Io)?.flatten() {
            let file = entry.file_name().to_string_lossy().into_owned();
            if file.starts_with(&prefix) && file.ends_with(".tmp") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        Ok(())
    }

    fn lock(&self) -> Result<std::fs::File, KeyError> {
        super::lock_file(&self.lock_path).map_err(|_| KeyError::Io)
    }

    fn load(&self, trust: Option<&dyn TrustView>) -> Result<(Loaded, bool), KeyError> {
        let mut loaded = Loaded {
            book: KeyBook::default(),
            offers: BTreeMap::new(),
        };
        let Some(bytes) = read_capped(&self.path)? else {
            return Ok((loaded, false));
        };
        let file = serde_json::from_slice::<KeysFile>(&bytes).map_err(|_| KeyError::Corrupt)?;
        if file.v != 1 {
            return Err(KeyError::Corrupt);
        }
        let mut cleared = false;
        for spawn_key_fp in file.revoked {
            loaded.book.restore_revoked(spawn_key_fp);
        }
        for (fp, st) in file.keys {
            let Ok(stmt) = verify_offer(&self.user_id, &st.offer, &IntegrityOnly, u64::MAX) else {
                continue;
            };
            if stmt.device_fp() != fp {
                continue;
            }
            let revoked =
                trust.is_some_and(|t| t.status(stmt.device_id(), &fp) == TrustStatus::Revoked);
            let state = if st.approved && !revoked {
                KeyState::Approved
            } else {
                KeyState::Pending
            };
            cleared |= st.approved && revoked;
            loaded.book.restore(stmt, state, st.replaced_at_ms);
            loaded.offers.insert(fp, st.offer);
        }
        Ok((loaded, cleared))
    }

    fn persist(&self, loaded: &Loaded) -> Result<(), KeyError> {
        let keys = loaded
            .offers
            .iter()
            .map(|(fp, offer)| {
                let stored = Stored {
                    offer: offer.clone(),
                    approved: loaded.book.state(fp) == Some(KeyState::Approved),
                    replaced_at_ms: loaded.book.replaced_at_ms(fp),
                };
                (fp.clone(), stored)
            })
            .collect();
        let revoked = loaded
            .book
            .revoked()
            .into_iter()
            .map(str::to_owned)
            .collect();
        let bytes = serde_json::to_vec(&KeysFile {
            v: 1,
            keys,
            revoked,
        })
        .map_err(|_| KeyError::Corrupt)?;
        super::write_durable(&self.path, &bytes).map_err(|_| KeyError::Io)
    }
}

fn read_capped(path: &Path) -> Result<Option<Vec<u8>>, KeyError> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let f = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(KeyError::Io),
    };
    let mut bytes = Vec::new();
    f.take(MAX_KEYS_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| KeyError::Io)?;
    if bytes.len() as u64 > MAX_KEYS_BYTES {
        return Err(KeyError::Corrupt);
    }
    Ok(Some(bytes))
}

#[cfg(test)]
pub(crate) mod testutil {
    use super::*;
    use p256::ecdsa::signature::Signer;
    use p256::ecdsa::{Signature, SigningKey};
    use std::sync::Mutex;
    use tmgr_spawn_auth::{b64_encode, build_spawn_key_statement, device_fp, TrustStatus};

    pub const USER: &str = "42";
    pub const DEVICE_ID: &str = "dev-1";

    pub struct FakeTrust(pub Mutex<TrustStatus>);

    impl FakeTrust {
        pub fn new(s: TrustStatus) -> Self {
            Self(Mutex::new(s))
        }
    }

    impl TrustView for FakeTrust {
        fn status(&self, _: &str, _: &str) -> TrustStatus {
            *self.0.lock().unwrap()
        }
    }

    pub fn secret(byte: u8) -> SigningKey {
        SigningKey::from_slice(&[byte; 32]).unwrap()
    }

    pub fn pub_b64(k: &SigningKey) -> String {
        b64_encode(k.verifying_key().to_encoded_point(false).as_bytes())
    }

    pub fn sign(k: &SigningKey, m: &str) -> String {
        let s: Signature = k.sign(m.as_bytes());
        b64_encode(s.to_der().as_bytes())
    }

    pub fn kex_key(index: Option<u32>) -> SigningKey {
        let mut b = [7u8; 32];
        if let Some(i) = index {
            b[28..].copy_from_slice(&i.to_be_bytes());
        }
        SigningKey::from_slice(&b).unwrap()
    }

    pub fn offer_with(
        kex: &SigningKey,
        device_id: &str,
        spawn: &SigningKey,
        issued_ms: u64,
    ) -> KeyOffer {
        let dev = secret(5);
        let (sign_pub, kex_pub) = (pub_b64(&dev), pub_b64(kex));
        let fp = device_fp(&sign_pub, &kex_pub);
        let statement = build_spawn_key_statement(USER, &fp, device_id, &pub_b64(spawn), issued_ms);
        KeyOffer {
            sig: sign(&dev, &statement),
            spawn_sig: sign(spawn, &statement),
            statement,
            device_id: device_id.into(),
            sign_pub_b64: sign_pub,
            kex_pub_b64: kex_pub,
        }
    }

    pub fn offer(spawn_byte: u8, issued_ms: u64) -> KeyOffer {
        offer_with(&kex_key(None), DEVICE_ID, &secret(spawn_byte), issued_ms)
    }

    pub fn fp_of(o: &KeyOffer) -> String {
        device_fp(&o.sign_pub_b64, &o.kex_pub_b64)
    }

    pub fn spawn_fp(spawn_byte: u8) -> String {
        tmgr_spawn_auth::sha256_hex(
            secret(spawn_byte)
                .verifying_key()
                .to_encoded_point(false)
                .as_bytes(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tmgr_spawn_auth::{b64_decode_strict, TrustStatus};

    const NOW: u64 = 1_795_000_100_000;
    const T0: u64 = 1_795_000_000_000;
    const LATER: u64 = NOW + 60_000;

    fn trusted() -> FakeTrust {
        FakeTrust::new(TrustStatus::Trusted)
    }

    fn path(dir: &Path) -> PathBuf {
        dir.join("spawnauth/keys.json")
    }

    fn open(dir: &Path, trust: &dyn TrustView) -> KeyStoreFile {
        KeyStoreFile::open(&path(dir), USER, trust).unwrap()
    }

    fn fixture_offer() -> KeyOffer {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../../tmgr-spawn-auth/tests/fixtures/spawn-v1.json"
        ))
        .unwrap();
        let (v, s) = (&f["statement_vector"], &f["statement"]);
        KeyOffer {
            statement: v["statement"].as_str().unwrap().into(),
            sig: v["sig"].as_str().unwrap().into(),
            spawn_sig: v["spawn_sig"].as_str().unwrap().into(),
            device_id: s["device_id"].as_str().unwrap().into(),
            sign_pub_b64: s["test_device_sign_pub_b64"].as_str().unwrap().into(),
            kex_pub_b64: s["test_device_kex_pub_b64"].as_str().unwrap().into(),
        }
    }

    fn spawn_fp_of(o: &KeyOffer) -> String {
        let pk = b64_decode_strict(o.statement.split(':').nth(5).unwrap()).unwrap();
        tmgr_spawn_auth::sha256_hex(&pk)
    }

    fn edit_file(dir: &Path, from: &str, to: &str) {
        let text = std::fs::read_to_string(path(dir)).unwrap();
        assert!(text.contains(from), "{from}");
        std::fs::write(path(dir), text.replace(from, to)).unwrap();
    }

    fn set_mode(p: &Path, mode: u32) {
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(mode)).unwrap();
    }

    #[test]
    fn fixture_statement_is_pending_until_approved_and_survives_reopen() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        let o = fixture_offer();
        assert_eq!(
            ks.offer(o.clone(), &trust, NOW).unwrap(),
            OfferOutcome::Added
        );
        let fp = fp_of(&o);
        assert_eq!(ks.state(&fp).unwrap(), Some(KeyState::Pending));
        assert_eq!(ks.pinned_key(&fp).unwrap(), None);
        let pending = ks.pending().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].device_fp, fp);
        assert_eq!(pending[0].spawn_key_fp, spawn_fp_of(&o));
        assert_eq!(
            ks.approve(&fp[..8], &spawn_fp_of(&o)[..16], NOW).unwrap(),
            fp
        );
        let k = ks.pinned_key(&fp).unwrap().unwrap();
        assert_eq!(k.user_id, USER);
        assert_eq!(k.device_id, "device-uuid-1");
        assert_eq!(
            k.spawn_pub,
            b64_decode_strict(o.statement.split(':').nth(5).unwrap()).unwrap()
        );
        drop(ks);
        let ks = open(t.path(), &trusted());
        assert_eq!(ks.state(&fp).unwrap(), Some(KeyState::Approved));
        assert_eq!(ks.pinned_key(&fp).unwrap(), Some(k));
        assert_eq!(mode(&path(t.path())), 0o600);
        assert_eq!(mode(&t.path().join("spawnauth")), 0o700);
        assert_eq!(mode(&t.path().join("spawnauth/.keys.lock")), 0o600);
    }

    fn mode(p: &Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn pending_key_survives_reopen_and_is_still_unusable() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let o = offer(3, T0);
        open(t.path(), &trust)
            .offer(o.clone(), &trust, NOW)
            .unwrap();
        let ks = open(t.path(), &trust);
        assert_eq!(ks.state(&fp_of(&o)).unwrap(), Some(KeyState::Pending));
        assert_eq!(ks.pinned_key(&fp_of(&o)).unwrap(), None);
    }

    #[test]
    fn newer_statement_replaces_as_pending_and_needs_a_new_approval_after_a_minute() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        let first = offer(3, T0);
        let fp = fp_of(&first);
        ks.offer(first, &trust, NOW).unwrap();
        ks.approve(&fp[..8], &spawn_fp(3)[..16], NOW).unwrap();
        let second = offer(4, T0 + 1);
        assert_eq!(
            ks.offer(second, &trust, NOW).unwrap(),
            OfferOutcome::Replaced
        );
        assert_eq!(ks.pinned_key(&fp).unwrap(), None);
        drop(ks);
        let ks = open(t.path(), &trust);
        assert_eq!(ks.state(&fp).unwrap(), Some(KeyState::Pending));
        assert_eq!(
            ks.approve(&fp[..8], &spawn_fp(4)[..16], NOW + 59_999),
            Err(KeyError::Approve(ApproveError::TooSoon))
        );
        ks.approve(&fp[..8], &spawn_fp(4)[..16], LATER).unwrap();
        let want = secret(4)
            .verifying_key()
            .to_encoded_point(false)
            .as_bytes()
            .to_vec();
        assert_eq!(ks.pinned_key(&fp).unwrap().unwrap().spawn_pub, want);
        assert_eq!(
            ks.offer(offer(3, T0), &trust, NOW).unwrap(),
            OfferOutcome::Stale
        );
        assert_eq!(ks.pinned_key(&fp).unwrap().unwrap().spawn_pub, want);
    }

    #[test]
    fn approval_names_the_spawn_key_so_a_swapped_pending_key_is_not_approved() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        let shown = offer(3, T0);
        let fp = fp_of(&shown);
        ks.offer(shown, &trust, NOW).unwrap();
        ks.offer(offer(4, T0 + 1), &trust, NOW).unwrap();
        assert_eq!(
            ks.approve(&fp[..8], &spawn_fp(3)[..16], LATER),
            Err(KeyError::Approve(ApproveError::KeyMismatch))
        );
        assert_eq!(ks.state(&fp).unwrap(), Some(KeyState::Pending));
        assert_eq!(ks.pinned_key(&fp).unwrap(), None);
        assert_eq!(ks.pending().unwrap()[0].spawn_key_fp, spawn_fp(4),);
    }

    #[test]
    fn statement_with_a_bad_spawn_sig_is_rejected_and_nothing_is_written() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        let mut o = offer(3, T0);
        o.spawn_sig = sign(&secret(9), &o.statement);
        assert_eq!(
            ks.offer(o, &trust, NOW),
            Err(KeyError::Refused(Refusal::BadSig))
        );
        let mut o = offer(3, T0);
        o.sig = sign(&secret(9), &o.statement);
        assert_eq!(
            ks.offer(o, &trust, NOW),
            Err(KeyError::Refused(Refusal::BadSig))
        );
        assert!(ks.pending().unwrap().is_empty());
        assert!(!path(t.path()).exists());
    }

    #[test]
    fn untrusted_device_cannot_offer_a_key() {
        let t = tempfile::tempdir().unwrap();
        let trust = FakeTrust::new(TrustStatus::Revoked);
        let ks = open(t.path(), &trust);
        assert_eq!(
            ks.offer(offer(3, T0), &trust, NOW),
            Err(KeyError::Refused(Refusal::NotTrusted))
        );
    }

    #[test]
    fn tampered_entries_are_dropped_on_load() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        let o = offer(3, T0);
        let fp = fp_of(&o);
        ks.offer(o.clone(), &trust, NOW).unwrap();
        ks.approve(&fp[..8], &spawn_fp(3)[..16], NOW).unwrap();
        drop(ks);

        let evil = pub_b64(&secret(8));
        let good = pub_b64(&secret(3));
        edit_file(t.path(), &good, &evil);
        let ks = open(t.path(), &trust);
        assert_eq!(ks.state(&fp).unwrap(), None);
        assert_eq!(ks.pinned_key(&fp).unwrap(), None);
        edit_file(t.path(), &evil, &good);
        assert_eq!(
            open(t.path(), &trust)
                .pinned_key(&fp)
                .unwrap()
                .map(|k| k.device_fp),
            Some(fp.clone())
        );

        edit_file(t.path(), &o.spawn_sig, &sign(&secret(3), "other"));
        assert_eq!(open(t.path(), &trust).state(&fp).unwrap(), None);
    }

    #[test]
    fn other_users_entries_are_dropped_on_load() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        let o = offer(3, T0);
        ks.offer(o.clone(), &trust, NOW).unwrap();
        ks.approve(&fp_of(&o)[..8], &spawn_fp(3)[..16], NOW)
            .unwrap();
        let ks = KeyStoreFile::open(&path(t.path()), "43", &trust).unwrap();
        assert_eq!(ks.state(&fp_of(&o)).unwrap(), None);
    }

    #[test]
    fn revoked_at_load_clears_the_approval_for_good() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        let o = offer(3, T0);
        let fp = fp_of(&o);
        ks.offer(o, &trust, NOW).unwrap();
        ks.approve(&fp[..8], &spawn_fp(3)[..16], NOW).unwrap();
        drop(ks);

        let revoked = FakeTrust::new(TrustStatus::Revoked);
        let ks = open(t.path(), &revoked);
        assert_eq!(ks.pinned_key(&fp).unwrap(), None);
        assert_eq!(ks.state(&fp).unwrap(), Some(KeyState::Pending));
        drop(ks);

        let ks = open(t.path(), &trust);
        assert_eq!(ks.pinned_key(&fp).unwrap(), None);
        assert_eq!(ks.state(&fp).unwrap(), Some(KeyState::Pending));
    }

    #[test]
    fn unknown_or_not_trusted_at_load_keeps_the_approval() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        let o = offer(3, T0);
        let fp = fp_of(&o);
        ks.offer(o, &trust, NOW).unwrap();
        ks.approve(&fp[..8], &spawn_fp(3)[..16], NOW).unwrap();
        drop(ks);

        for status in [TrustStatus::Unknown, TrustStatus::NotTrusted] {
            let ks = open(t.path(), &FakeTrust::new(status));
            assert_eq!(
                ks.state(&fp).unwrap(),
                Some(KeyState::Approved),
                "{status:?}"
            );
        }
    }

    #[test]
    fn ambiguous_prefix_is_refused() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        let a = offer_with(&kex_key(Some(62958)), "dev-a", &secret(3), T0);
        let b = offer_with(&kex_key(Some(80799)), "dev-b", &secret(3), T0);
        let (fa, fb) = (fp_of(&a), fp_of(&b));
        assert_eq!(&fa[..8], "dac2df92");
        assert_eq!(&fb[..8], "dac2df92");
        ks.offer(a, &trust, NOW).unwrap();
        ks.offer(b, &trust, NOW).unwrap();
        let sf = spawn_fp(3);
        assert_eq!(
            ks.approve("dac2df92", &sf[..16], NOW),
            Err(KeyError::Approve(ApproveError::Ambiguous))
        );
        assert_eq!(
            ks.approve(&fa[..7], &sf[..16], NOW),
            Err(KeyError::Approve(ApproveError::PrefixTooShort))
        );
        assert_eq!(ks.state(&fa).unwrap(), Some(KeyState::Pending));
        assert_eq!(ks.approve(&fa[..12], &sf[..16], NOW).unwrap(), fa);
        assert_eq!(ks.state(&fb).unwrap(), Some(KeyState::Pending));
    }

    #[test]
    fn unchanged_offer_keeps_approval() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        let o = offer(3, T0);
        let fp = fp_of(&o);
        ks.offer(o.clone(), &trust, NOW).unwrap();
        ks.approve(&fp[..8], &spawn_fp(3)[..16], NOW).unwrap();
        assert_eq!(ks.offer(o, &trust, NOW).unwrap(), OfferOutcome::Unchanged);
        assert!(ks.pinned_key(&fp).unwrap().is_some());
    }

    #[test]
    fn corrupt_file_is_an_error_and_a_missing_file_is_empty() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        assert!(open(t.path(), &trust).pending().unwrap().is_empty());
        std::fs::create_dir_all(t.path().join("spawnauth")).unwrap();
        std::fs::write(path(t.path()), "{nope").unwrap();
        let err = KeyStoreFile::open(&path(t.path()), USER, &trust).err();
        assert_eq!(err, Some(KeyError::Corrupt));
    }

    #[test]
    fn two_instances_do_not_lose_each_others_writes() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let (a, b) = (open(t.path(), &trust), open(t.path(), &trust));
        let first = offer(3, T0);
        let fp = fp_of(&first);
        a.offer(first, &trust, NOW).unwrap();
        a.approve(&fp[..8], &spawn_fp(3)[..16], NOW).unwrap();
        let other = offer_with(&kex_key(Some(1)), "dev-2", &secret(3), T0);
        let other_fp = fp_of(&other);
        assert_eq!(b.offer(other, &trust, NOW).unwrap(), OfferOutcome::Added);
        assert!(b.pinned_key(&fp).unwrap().is_some());
        drop((a, b));
        let ks = open(t.path(), &trust);
        assert!(ks.pinned_key(&fp).unwrap().is_some());
        assert_eq!(ks.state(&other_fp).unwrap(), Some(KeyState::Pending));
    }

    #[test]
    fn changes_through_another_instance_are_seen_without_a_restart() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let (a, b) = (open(t.path(), &trust), open(t.path(), &trust));
        let o = offer(3, T0);
        let fp = fp_of(&o);
        a.offer(o, &trust, NOW).unwrap();
        assert_eq!(b.pinned_key(&fp).unwrap(), None);
        a.approve(&fp[..8], &spawn_fp(3)[..16], NOW).unwrap();
        assert!(b.pinned_key(&fp).unwrap().is_some());
        let _revoking = open(t.path(), &FakeTrust::new(TrustStatus::Revoked));
        assert_eq!(b.pinned_key(&fp).unwrap(), None);
        assert_eq!(a.state(&fp).unwrap(), Some(KeyState::Pending));
    }

    #[test]
    fn the_keys_lock_is_held_while_a_pinned_key_is_in_use() {
        use std::os::fd::AsRawFd;
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        let held = ks
            .with_pinned("x", |_| {
                let f = std::fs::File::open(t.path().join("spawnauth/.keys.lock")).unwrap();
                // SAFETY: the fd is owned by `f` and stays open for the call.
                unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) != 0 }
            })
            .unwrap();
        assert!(held);
    }

    #[test]
    fn a_failed_write_leaves_the_key_pending_and_unusable() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        let o = offer(3, T0);
        let fp = fp_of(&o);
        ks.offer(o, &trust, NOW).unwrap();
        let dir = t.path().join("spawnauth");
        set_mode(&dir, 0o500);
        let res = ks.approve(&fp[..8], &spawn_fp(3)[..16], NOW);
        set_mode(&dir, 0o700);
        assert_eq!(res, Err(KeyError::Io));
        assert_eq!(ks.state(&fp).unwrap(), Some(KeyState::Pending));
        assert_eq!(ks.pinned_key(&fp).unwrap(), None);
        assert_eq!(ks.pending().unwrap().len(), 1);
    }

    #[test]
    fn a_failed_write_does_not_record_an_offer() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        let o = offer(3, T0);
        let dir = t.path().join("spawnauth");
        set_mode(&dir, 0o500);
        let res = ks.offer(o.clone(), &trust, NOW);
        set_mode(&dir, 0o700);
        assert_eq!(res, Err(KeyError::Io));
        assert_eq!(ks.state(&fp_of(&o)).unwrap(), None);
    }

    #[test]
    fn recover_removes_leftover_temp_files_of_the_keys_file_only() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        ks.offer(offer(3, T0), &trust, NOW).unwrap();
        let dir = t.path().join("spawnauth");
        std::fs::write(dir.join(".keys.json.00ff00ff00ff00ff.tmp"), "x").unwrap();
        std::fs::write(dir.join("keep.tmp"), "x").unwrap();
        ks.recover().unwrap();
        assert!(!dir.join(".keys.json.00ff00ff00ff00ff.tmp").exists());
        assert!(dir.join("keep.tmp").exists());
        assert!(path(t.path()).exists());
    }

    #[test]
    fn open_removes_leftover_temp_files() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        drop(open(t.path(), &trust));
        let tmp = t.path().join("spawnauth/.keys.json.00ff00ff00ff00ff.tmp");
        std::fs::write(&tmp, "x").unwrap();
        drop(open(t.path(), &trust));
        assert!(!tmp.exists());
    }

    #[test]
    fn the_sixty_fifth_device_is_refused_and_not_persisted() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        for i in 0..64 {
            let o = offer_with(&kex_key(Some(i)), "dev", &secret(3), T0);
            assert_eq!(ks.offer(o, &trust, NOW).unwrap(), OfferOutcome::Added);
        }
        let extra = offer_with(&kex_key(Some(64)), "dev", &secret(3), T0);
        assert_eq!(
            ks.offer(extra.clone(), &trust, NOW).unwrap(),
            OfferOutcome::Full
        );
        assert_eq!(ks.state(&fp_of(&extra)).unwrap(), None);
        assert_eq!(ks.pending().unwrap().len(), 64);
    }

    #[test]
    fn a_cooldown_from_a_skewed_clock_is_clamped_and_persisted() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        let o = offer(3, T0);
        let fp = fp_of(&o);
        ks.offer(o, &trust, NOW).unwrap();
        ks.offer(offer(4, T0 + 1), &trust, NOW + 10_000_000)
            .unwrap();
        assert_eq!(
            ks.approve(&fp[..8], &spawn_fp(4)[..16], NOW),
            Err(KeyError::Approve(ApproveError::TooSoon))
        );
        ks.approve(&fp[..8], &spawn_fp(4)[..16], NOW + 60_000)
            .unwrap();
    }

    #[test]
    fn revoke_unpins_tombstones_and_survives_reopen() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        let o = offer(3, T0);
        let fp = fp_of(&o);
        ks.offer(o.clone(), &trust, NOW).unwrap();
        ks.approve(&fp[..8], &spawn_fp(3)[..16], NOW).unwrap();
        let r = ks.revoke(&fp[..8]).unwrap();
        assert_eq!(r.device_fp, fp);
        assert_eq!(r.spawn_key_fp, spawn_fp(3));
        assert_eq!(ks.pinned_key(&fp).unwrap(), None);
        assert_eq!(ks.state(&fp).unwrap(), None);
        assert_eq!(
            ks.offer(o.clone(), &trust, NOW).unwrap(),
            OfferOutcome::Revoked
        );
        drop(ks);
        let ks = open(t.path(), &trust);
        assert_eq!(ks.state(&fp).unwrap(), None);
        assert_eq!(ks.offer(o, &trust, NOW).unwrap(), OfferOutcome::Revoked);
        assert!(ks.list().unwrap().is_empty());
    }

    #[test]
    fn a_new_key_after_revoke_is_pending() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        let o = offer(3, T0);
        let fp = fp_of(&o);
        ks.offer(o, &trust, NOW).unwrap();
        ks.revoke(&fp[..8]).unwrap();
        assert_eq!(
            ks.offer(offer(4, T0 + 1), &trust, NOW).unwrap(),
            OfferOutcome::Added
        );
        assert_eq!(ks.state(&fp).unwrap(), Some(KeyState::Pending));
        assert_eq!(ks.pinned_key(&fp).unwrap(), None);
    }

    #[test]
    fn revoke_errors_are_typed() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        let o = offer(3, T0);
        let fp = fp_of(&o);
        ks.offer(o, &trust, NOW).unwrap();
        assert_eq!(
            ks.revoke("abc"),
            Err(KeyError::Revoke(RevokeError::PrefixTooShort))
        );
        assert_eq!(
            ks.revoke("ffffffff"),
            Err(KeyError::Revoke(RevokeError::NotFound))
        );
        assert_eq!(ks.state(&fp).unwrap(), Some(KeyState::Pending));
    }

    #[test]
    fn list_shows_pending_and_approved_with_details() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        assert!(ks.list().unwrap().is_empty());
        let o = offer(3, T0);
        let fp = fp_of(&o);
        ks.offer(o, &trust, NOW).unwrap();
        let l = ks.list().unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].device_fp, fp);
        assert_eq!(l[0].device_id, DEVICE_ID);
        assert_eq!(l[0].spawn_key_fp, spawn_fp(3));
        assert_eq!(l[0].state, KeyState::Pending);
        assert_eq!(l[0].issued_at_ms, T0);
        assert_eq!(l[0].replaced_at_ms, None);
        ks.approve(&fp[..8], &spawn_fp(3)[..16], NOW).unwrap();
        assert_eq!(ks.list().unwrap()[0].state, KeyState::Approved);
    }

    #[test]
    fn a_file_without_tombstones_still_loads() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let ks = open(t.path(), &trust);
        ks.offer(offer(3, T0), &trust, NOW).unwrap();
        let text = std::fs::read_to_string(path(t.path())).unwrap();
        assert!(!text.contains("revoked"));
        assert_eq!(ks.list().unwrap().len(), 1);
    }
}
