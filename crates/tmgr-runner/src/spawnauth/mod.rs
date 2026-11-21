mod cards;
mod keys;

pub use cards::{
    os_rng, CardError, CardResult, CardState, CardStore, Challenge, ClaimOutcome, Identity,
    IssueOutcome, RecoverReport, Rng, ATTEMPT_LIMIT, CARD_TTL_MS, MAX_PENDING, PRUNE_AFTER_MS,
};
pub use keys::{KeyError, KeyOffer, KeyStoreFile};

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tmgr_spawn_auth::{
    OfferOutcome, PinnedKey, Refusal, SignedSpawn, SpawnProposal, TrustStatus, TrustView,
};

pub trait TrustSource: Send + Sync {
    fn status(&self, device_id: &str, fp: &str) -> TrustStatus;
}

pub struct TrustAdapter<'a>(pub &'a dyn TrustSource);

impl TrustView for TrustAdapter<'_> {
    fn status(&self, device_id: &str, fp: &str) -> TrustStatus {
        self.0.status(device_id, fp)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpawnAuthError {
    Card(CardError),
    Key(KeyError),
}

impl std::fmt::Display for SpawnAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SpawnAuthError::Card(e) => e.fmt(f),
            SpawnAuthError::Key(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for SpawnAuthError {}

pub struct SpawnAuth {
    cards: CardStore,
    keys: Mutex<KeyStoreFile>,
    trust: Arc<dyn TrustSource>,
}

impl SpawnAuth {
    pub fn open(
        runner_dir: &std::path::Path,
        identity: Identity,
        trust: Arc<dyn TrustSource>,
        rng: Rng,
        now_ms: u64,
    ) -> Result<Self, SpawnAuthError> {
        let keys = KeyStoreFile::open(
            &runner_dir.join("spawnauth").join("keys.json"),
            identity.user_id(),
            &TrustAdapter(&*trust),
            now_ms,
        )
        .map_err(SpawnAuthError::Key)?;
        let cards = CardStore::open(runner_dir, identity, rng).map_err(SpawnAuthError::Card)?;
        Ok(Self {
            cards,
            keys: Mutex::new(keys),
            trust,
        })
    }

    pub fn cards(&self) -> &CardStore {
        &self.cards
    }

    pub fn keys(&self) -> MutexGuard<'_, KeyStoreFile> {
        self.keys.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn issue(
        &self,
        card_id: &str,
        proposal: &SpawnProposal,
        now_ms: u64,
    ) -> Result<IssueOutcome, CardError> {
        self.cards.issue(card_id, proposal, now_ms)
    }

    pub fn claim(
        &self,
        card_id: &str,
        wire: &SignedSpawn,
        now_ms: u64,
    ) -> Result<ClaimOutcome, CardError> {
        self.cards.state(card_id)?;
        let Some(key) = self.pinned_key(&wire.device_fp) else {
            return Ok(ClaimOutcome::Rejected(Refusal::UnknownKey));
        };
        self.cards
            .claim(card_id, wire, &key, &TrustAdapter(&*self.trust), now_ms)
    }

    pub fn offer_key(&self, offer: KeyOffer, now_ms: u64) -> Result<OfferOutcome, KeyError> {
        self.keys()
            .offer(offer, &TrustAdapter(&*self.trust), now_ms)
    }

    pub fn approve_key(&self, fp_prefix: &str) -> Result<tmgr_spawn_auth::DeviceFp, KeyError> {
        self.keys().approve(fp_prefix)
    }

    pub fn pinned_key(&self, device_fp: &str) -> Option<PinnedKey> {
        self.keys().pinned_key(device_fp)
    }
}

use std::fs::{DirBuilder, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

pub(crate) fn ensure_private_dir(path: &Path) -> std::io::Result<()> {
    DirBuilder::new().recursive(true).mode(0o700).create(path)?;
    if !std::fs::symlink_metadata(path)?.is_dir() {
        return Err(std::io::Error::other("not a directory"));
    }
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

pub(crate) fn sync_dir(dir: &Path) -> std::io::Result<()> {
    File::open(dir)?.sync_all()
}

pub(crate) fn write_durable(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("no parent"))?;
    let mut rnd = [0u8; 8];
    getrandom::fill(&mut rnd).map_err(std::io::Error::other)?;
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let tmp = dir.join(format!(".{name}.{}.tmp", hex_lower(&rnd)));
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .custom_flags(libc::O_NOFOLLOW)
        .mode(0o600)
        .open(&tmp)?;
    let res = f
        .write_all(bytes)
        .and_then(|()| f.sync_all())
        .and_then(|()| {
            drop(f);
            std::fs::rename(&tmp, path)
        });
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
        return res;
    }
    sync_dir(dir)
}

pub(crate) fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::keys::testutil::*;
    use super::*;
    use tmgr_spawn_auth::SpawnMode;

    const T0: u64 = 1_795_000_000_000;
    const NOW: u64 = T0 + 100_000;
    const TARGET_ID: &str = "mx_0123456789abcdef";

    struct Source(Mutex<TrustStatus>);

    impl TrustSource for Source {
        fn status(&self, _: &str, _: &str) -> TrustStatus {
            *self.0.lock().unwrap()
        }
    }

    fn proposal() -> SpawnProposal {
        SpawnProposal {
            v: 1,
            profile: "claude".into(),
            cwd: "/work/app".into(),
            mode: SpawnMode::Bypass,
            prompt_sha256: None,
            name: None,
            task: None,
            target_id: TARGET_ID.into(),
        }
    }

    fn rng() -> Rng {
        Arc::new(|| [0xab; 16])
    }

    fn setup(dir: &std::path::Path) -> (SpawnAuth, Arc<Source>) {
        let source = Arc::new(Source(Mutex::new(TrustStatus::Trusted)));
        let identity = Identity::new(USER, TARGET_ID, &"b".repeat(64)).unwrap();
        let auth = SpawnAuth::open(dir, identity, source.clone(), rng(), NOW).unwrap();
        (auth, source)
    }

    fn signed(c: &Challenge, card_id: &str, fp: &str) -> SignedSpawn {
        let ts_ms = NOW;
        let msg = format!(
            "tmgr-spawn:v1:{USER}:{fp}:{}:{}:{card_id}:{}:{ts_ms}:{}:{}",
            c.target_id, c.target_fp, c.nonce, c.exp_ms, c.req_sha256
        );
        SignedSpawn {
            v: 1,
            user_id: USER.into(),
            device_fp: fp.into(),
            target_id: c.target_id.clone(),
            target_fp: c.target_fp.clone(),
            card_id: card_id.into(),
            nonce: c.nonce.clone(),
            ts_ms,
            exp_ms: c.exp_ms,
            req_sha256: c.req_sha256.clone(),
            alg: "es256".into(),
            sig: sign(&secret(3), &msg),
        }
    }

    fn challenge(auth: &SpawnAuth, card_id: &str) -> Challenge {
        match auth.issue(card_id, &proposal(), T0).unwrap() {
            IssueOutcome::Challenge(c) => c,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn approved_key_signs_one_card_end_to_end() {
        let t = tempfile::tempdir().unwrap();
        let (auth, _) = setup(t.path());
        let o = offer(3, T0);
        let fp = fp_of(&o);
        assert_eq!(auth.offer_key(o, NOW).unwrap(), OfferOutcome::Added);
        auth.approve_key(&fp[..8]).unwrap();
        let c = challenge(&auth, "card-1");
        let wire = signed(&c, "card-1", &fp);
        let out = auth.claim("card-1", &wire, NOW).unwrap();
        assert!(matches!(out, ClaimOutcome::Verified(_)), "{out:?}");
        let again = auth.claim("card-1", &wire, NOW).unwrap();
        assert!(
            matches!(again, ClaimOutcome::AlreadyClaimed(_)),
            "{again:?}"
        );
        drop(auth);
        let (auth, _) = setup(t.path());
        assert!(auth.pinned_key(&fp).is_some());
        assert!(matches!(
            auth.cards().result("card-1").unwrap(),
            Some(CardResult::Interrupted { .. })
        ));
    }

    #[test]
    fn pending_key_is_never_used_to_verify() {
        let t = tempfile::tempdir().unwrap();
        let (auth, _) = setup(t.path());
        let o = offer(3, T0);
        let fp = fp_of(&o);
        auth.offer_key(o, NOW).unwrap();
        let c = challenge(&auth, "card-1");
        let out = auth
            .claim("card-1", &signed(&c, "card-1", &fp), NOW)
            .unwrap();
        assert_eq!(out, ClaimOutcome::Rejected(Refusal::UnknownKey));
        assert!(matches!(
            auth.cards().state("card-1").unwrap(),
            Some(CardState::Issued {
                failed_attempts: 0,
                ..
            })
        ));
    }

    #[test]
    fn revoked_device_is_refused_at_claim_time() {
        let t = tempfile::tempdir().unwrap();
        let (auth, source) = setup(t.path());
        let o = offer(3, T0);
        let fp = fp_of(&o);
        auth.offer_key(o, NOW).unwrap();
        auth.approve_key(&fp[..8]).unwrap();
        let c = challenge(&auth, "card-1");
        let wire = signed(&c, "card-1", &fp);
        *source.0.lock().unwrap() = TrustStatus::Revoked;
        let out = auth.claim("card-1", &wire, NOW).unwrap();
        assert_eq!(out, ClaimOutcome::Rejected(Refusal::NotTrusted));
        assert!(matches!(
            auth.cards().state("card-1").unwrap(),
            Some(CardState::Issued {
                failed_attempts: 0,
                ..
            })
        ));
    }

    #[test]
    fn unknown_device_and_bad_card_id() {
        let t = tempfile::tempdir().unwrap();
        let (auth, _) = setup(t.path());
        let c = challenge(&auth, "card-1");
        let out = auth
            .claim("card-1", &signed(&c, "card-1", &"c".repeat(64)), NOW)
            .unwrap();
        assert_eq!(out, ClaimOutcome::Rejected(Refusal::UnknownKey));
        let wire = signed(&c, "card-1", &"c".repeat(64));
        assert_eq!(auth.claim("../x", &wire, NOW), Err(CardError::BadCardId));
    }
}
