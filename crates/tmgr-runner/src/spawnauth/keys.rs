use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use tmgr_spawn_auth::{
    ApproveError, DeviceFp, DeviceKeys, KeyBook, KeyState, OfferOutcome, PinnedKey, Refusal,
    SpawnKeyStatement, TrustView,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyError {
    Refused(Refusal),
    Approve(ApproveError),
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

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Stored {
    #[serde(flatten)]
    offer: KeyOffer,
    approved: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeysFile {
    v: u32,
    keys: BTreeMap<String, Stored>,
}

pub struct KeyStoreFile {
    path: PathBuf,
    user_id: String,
    book: KeyBook,
    stored: BTreeMap<String, Stored>,
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
    pub fn open(
        path: &Path,
        user_id: &str,
        trust: &dyn TrustView,
        now_ms: u64,
    ) -> Result<Self, KeyError> {
        let parent = path.parent().ok_or(KeyError::Io)?;
        super::ensure_private_dir(parent).map_err(|_| KeyError::Io)?;
        let mut store = Self {
            path: path.into(),
            user_id: user_id.into(),
            book: KeyBook::default(),
            stored: BTreeMap::new(),
        };
        let file = match read_capped(path)? {
            None => return Ok(store),
            Some(bytes) => {
                serde_json::from_slice::<KeysFile>(&bytes).map_err(|_| KeyError::Corrupt)?
            }
        };
        if file.v != 1 {
            return Err(KeyError::Corrupt);
        }
        for (fp, st) in file.keys {
            match verify_offer(user_id, &st.offer, trust, now_ms) {
                Ok(stmt) if stmt.device_fp() == fp => {
                    store.book.offer(stmt);
                    if st.approved {
                        let _ = store.book.approve(&fp);
                    }
                    store.stored.insert(fp, st);
                }
                Err(Refusal::NotTrusted) => {
                    store.stored.insert(fp, st);
                }
                _ => {}
            }
        }
        Ok(store)
    }

    pub fn offer(
        &mut self,
        offer: KeyOffer,
        trust: &dyn TrustView,
        now_ms: u64,
    ) -> Result<OfferOutcome, KeyError> {
        let stmt = verify_offer(&self.user_id, &offer, trust, now_ms).map_err(KeyError::Refused)?;
        let fp = stmt.device_fp().to_string();
        let outcome = self.book.offer(stmt);
        if matches!(outcome, OfferOutcome::Added | OfferOutcome::Replaced) {
            self.stored.insert(
                fp,
                Stored {
                    offer,
                    approved: false,
                },
            );
            self.persist()?;
        }
        Ok(outcome)
    }

    pub fn approve(&mut self, fp_prefix: &str) -> Result<DeviceFp, KeyError> {
        let fp = self.book.approve(fp_prefix).map_err(KeyError::Approve)?;
        if let Some(st) = self.stored.get_mut(&fp) {
            st.approved = true;
        }
        self.persist()?;
        Ok(fp)
    }

    pub fn pinned_key(&self, device_fp: &str) -> Option<PinnedKey> {
        self.book.pinned_key(&self.user_id, device_fp)
    }

    pub fn state(&self, device_fp: &str) -> Option<KeyState> {
        self.book.state(device_fp)
    }

    pub fn pending(&self) -> Vec<&SpawnKeyStatement> {
        self.book.pending()
    }

    fn persist(&self) -> Result<(), KeyError> {
        let bytes = serde_json::to_vec(&KeysFileRef {
            v: 1,
            keys: &self.stored,
        })
        .map_err(|_| KeyError::Corrupt)?;
        super::write_durable(&self.path, &bytes).map_err(|_| KeyError::Io)
    }
}

#[derive(Serialize)]
struct KeysFileRef<'a> {
    v: u32,
    keys: &'a BTreeMap<String, Stored>,
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
}

#[cfg(test)]
mod tests {
    use super::testutil::*;
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tmgr_spawn_auth::{b64_decode_strict, TrustStatus};

    const NOW: u64 = 1_795_000_100_000;
    const T0: u64 = 1_795_000_000_000;

    fn trusted() -> FakeTrust {
        FakeTrust::new(TrustStatus::Trusted)
    }

    fn path(dir: &Path) -> PathBuf {
        dir.join("spawnauth/keys.json")
    }

    fn open(dir: &Path, trust: &dyn TrustView) -> KeyStoreFile {
        KeyStoreFile::open(&path(dir), USER, trust, NOW).unwrap()
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

    fn edit_file(dir: &Path, from: &str, to: &str) {
        let text = std::fs::read_to_string(path(dir)).unwrap();
        assert!(text.contains(from), "{from}");
        std::fs::write(path(dir), text.replace(from, to)).unwrap();
    }

    #[test]
    fn fixture_statement_is_pending_until_approved_and_survives_reopen() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let mut ks = open(t.path(), &trust);
        let o = fixture_offer();
        assert_eq!(
            ks.offer(o.clone(), &trust, NOW).unwrap(),
            OfferOutcome::Added
        );
        let fp = fp_of(&o);
        assert_eq!(ks.state(&fp), Some(KeyState::Pending));
        assert_eq!(ks.pinned_key(&fp), None);
        assert_eq!(ks.pending().len(), 1);
        assert_eq!(ks.approve(&fp[..8]).unwrap(), fp);
        let k = ks.pinned_key(&fp).unwrap();
        assert_eq!(k.user_id, USER);
        assert_eq!(k.device_id, "device-uuid-1");
        assert_eq!(
            k.spawn_pub,
            b64_decode_strict(o.statement.split(':').nth(5).unwrap()).unwrap()
        );
        drop(ks);
        let ks = open(t.path(), &trusted());
        assert_eq!(ks.state(&fp), Some(KeyState::Approved));
        assert_eq!(ks.pinned_key(&fp), Some(k));
        assert_eq!(
            std::fs::metadata(path(t.path()))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(t.path().join("spawnauth"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
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
        assert_eq!(ks.state(&fp_of(&o)), Some(KeyState::Pending));
        assert_eq!(ks.pinned_key(&fp_of(&o)), None);
    }

    #[test]
    fn newer_statement_replaces_as_pending_and_needs_a_new_approval() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let mut ks = open(t.path(), &trust);
        let first = offer(3, T0);
        let fp = fp_of(&first);
        ks.offer(first, &trust, NOW).unwrap();
        ks.approve(&fp[..8]).unwrap();
        let second = offer(4, T0 + 1);
        assert_eq!(
            ks.offer(second, &trust, NOW).unwrap(),
            OfferOutcome::Replaced
        );
        assert_eq!(ks.pinned_key(&fp), None);
        drop(ks);
        let mut ks = open(t.path(), &trust);
        assert_eq!(ks.state(&fp), Some(KeyState::Pending));
        ks.approve(&fp[..8]).unwrap();
        let want = secret(4)
            .verifying_key()
            .to_encoded_point(false)
            .as_bytes()
            .to_vec();
        assert_eq!(ks.pinned_key(&fp).unwrap().spawn_pub, want);
        assert_eq!(
            ks.offer(offer(3, T0), &trust, NOW).unwrap(),
            OfferOutcome::Stale
        );
        assert_eq!(ks.pinned_key(&fp).unwrap().spawn_pub, want);
    }

    #[test]
    fn statement_with_a_bad_spawn_sig_is_rejected_and_nothing_is_written() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let mut ks = open(t.path(), &trust);
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
        assert!(ks.pending().is_empty());
        assert!(!path(t.path()).exists());
    }

    #[test]
    fn untrusted_device_cannot_offer_a_key() {
        let t = tempfile::tempdir().unwrap();
        let trust = FakeTrust::new(TrustStatus::Revoked);
        let mut ks = open(t.path(), &trust);
        assert_eq!(
            ks.offer(offer(3, T0), &trust, NOW),
            Err(KeyError::Refused(Refusal::NotTrusted))
        );
    }

    #[test]
    fn tampered_entries_are_dropped_on_load() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let mut ks = open(t.path(), &trust);
        let o = offer(3, T0);
        let fp = fp_of(&o);
        ks.offer(o.clone(), &trust, NOW).unwrap();
        ks.approve(&fp[..8]).unwrap();
        drop(ks);

        let evil = pub_b64(&secret(8));
        let good = pub_b64(&secret(3));
        edit_file(t.path(), &good, &evil);
        let ks = open(t.path(), &trust);
        assert_eq!(ks.state(&fp), None);
        assert_eq!(ks.pinned_key(&fp), None);
        edit_file(t.path(), &evil, &good);
        assert_eq!(
            open(t.path(), &trust).pinned_key(&fp).map(|k| k.device_fp),
            Some(fp.clone())
        );

        edit_file(t.path(), &o.spawn_sig, &sign(&secret(3), "other"));
        assert_eq!(open(t.path(), &trust).state(&fp), None);
    }

    #[test]
    fn other_users_entries_are_dropped_on_load() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let mut ks = open(t.path(), &trust);
        let o = offer(3, T0);
        ks.offer(o.clone(), &trust, NOW).unwrap();
        ks.approve(&fp_of(&o)[..8]).unwrap();
        let ks = KeyStoreFile::open(&path(t.path()), "43", &trust, NOW).unwrap();
        assert_eq!(ks.state(&fp_of(&o)), None);
    }

    #[test]
    fn untrusted_at_load_keeps_the_entry_dormant_not_usable() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let mut ks = open(t.path(), &trust);
        let o = offer(3, T0);
        let fp = fp_of(&o);
        ks.offer(o, &trust, NOW).unwrap();
        ks.approve(&fp[..8]).unwrap();
        drop(ks);

        let revoked = FakeTrust::new(TrustStatus::Revoked);
        let mut ks = open(t.path(), &revoked);
        assert_eq!(ks.pinned_key(&fp), None);
        let other = offer_with(&kex_key(Some(1)), "dev-2", &secret(3), T0);
        ks.offer(other, &trust, NOW).unwrap();
        drop(ks);

        let ks = open(t.path(), &trust);
        assert!(ks.pinned_key(&fp).is_some());
    }

    #[test]
    fn ambiguous_prefix_is_refused() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let mut ks = open(t.path(), &trust);
        let a = offer_with(&kex_key(Some(62958)), "dev-a", &secret(3), T0);
        let b = offer_with(&kex_key(Some(80799)), "dev-b", &secret(3), T0);
        let (fa, fb) = (fp_of(&a), fp_of(&b));
        assert_eq!(&fa[..8], "dac2df92");
        assert_eq!(&fb[..8], "dac2df92");
        ks.offer(a, &trust, NOW).unwrap();
        ks.offer(b, &trust, NOW).unwrap();
        assert_eq!(
            ks.approve("dac2df92"),
            Err(KeyError::Approve(ApproveError::Ambiguous))
        );
        assert_eq!(
            ks.approve(&fa[..7]),
            Err(KeyError::Approve(ApproveError::PrefixTooShort))
        );
        assert_eq!(ks.state(&fa), Some(KeyState::Pending));
        assert_eq!(ks.approve(&fa[..12]).unwrap(), fa);
        assert_eq!(ks.state(&fb), Some(KeyState::Pending));
    }

    #[test]
    fn unchanged_offer_keeps_approval() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        let mut ks = open(t.path(), &trust);
        let o = offer(3, T0);
        let fp = fp_of(&o);
        ks.offer(o.clone(), &trust, NOW).unwrap();
        ks.approve(&fp[..8]).unwrap();
        assert_eq!(ks.offer(o, &trust, NOW).unwrap(), OfferOutcome::Unchanged);
        assert!(ks.pinned_key(&fp).is_some());
    }

    #[test]
    fn corrupt_file_is_an_error_and_a_missing_file_is_empty() {
        let t = tempfile::tempdir().unwrap();
        let trust = trusted();
        assert!(open(t.path(), &trust).pending().is_empty());
        std::fs::create_dir_all(t.path().join("spawnauth")).unwrap();
        std::fs::write(path(t.path()), "{nope").unwrap();
        let err = KeyStoreFile::open(&path(t.path()), USER, &trust, NOW).err();
        assert_eq!(err, Some(KeyError::Corrupt));
    }
}
