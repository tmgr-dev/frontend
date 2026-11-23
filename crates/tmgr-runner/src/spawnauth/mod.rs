//! Lock order: keys lock, then cards lock; the `with_pinned` closure must not call back into the key store (flock does not nest).
//! Local filesystem only (flock); same-user threat model (files are not MAC'd); pruning drops terminal cards after 30 days so card ids must be unique; rate limiting is owned by 3C.

mod cards;
mod keys;

pub use cards::{
    os_rng, CardBudget, CardError, CardResult, CardState, CardStore, Challenge, CheckOutcome,
    ClaimOutcome, Identity, IssueOutcome, LocalClaim, RecoverReport, Rng, ATTEMPT_LIMIT,
    CARD_TTL_MS, MAX_CARDS, MAX_MANAGER_PENDING, MAX_PENDING, PRUNE_AFTER_MS,
};
#[cfg(test)]
pub(crate) use keys::testutil;
pub use keys::{KeyError, KeyInfo, KeyOffer, KeyStoreFile, PendingKey};

use std::sync::Arc;
use tmgr_spawn_auth::{
    OfferOutcome, PinnedKey, Refusal, RevokedKey, SignedSpawn, SpawnProposal, TrustStatus,
    TrustView,
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
    keys: KeyStoreFile,
    trust: Arc<dyn TrustSource>,
}

impl SpawnAuth {
    /// Removes leftover keys.json temp files; a corrupt keys.json fails closed and needs operator repair.
    pub fn open(
        runner_dir: &std::path::Path,
        identity: Identity,
        trust: Arc<dyn TrustSource>,
    ) -> Result<Self, SpawnAuthError> {
        let keys = Self::open_keys(runner_dir, &identity, &trust)?;
        let cards = CardStore::open(runner_dir, identity).map_err(SpawnAuthError::Card)?;
        Ok(Self { cards, keys, trust })
    }

    #[cfg(test)]
    pub(crate) fn open_with_rng(
        runner_dir: &std::path::Path,
        identity: Identity,
        trust: Arc<dyn TrustSource>,
        rng: Rng,
    ) -> Result<Self, SpawnAuthError> {
        let keys = Self::open_keys(runner_dir, &identity, &trust)?;
        let cards =
            CardStore::open_with_rng(runner_dir, identity, rng).map_err(SpawnAuthError::Card)?;
        Ok(Self { cards, keys, trust })
    }

    fn open_keys(
        runner_dir: &std::path::Path,
        identity: &Identity,
        trust: &Arc<dyn TrustSource>,
    ) -> Result<KeyStoreFile, SpawnAuthError> {
        KeyStoreFile::open(
            &runner_dir.join("spawnauth").join("keys.json"),
            identity.user_id(),
            &TrustAdapter(&**trust),
        )
        .map_err(SpawnAuthError::Key)
    }

    pub fn cards(&self) -> &CardStore {
        &self.cards
    }

    pub fn keys(&self) -> &KeyStoreFile {
        &self.keys
    }

    pub fn recover(&self, now_ms: u64) -> Result<RecoverReport, SpawnAuthError> {
        self.keys.recover().map_err(SpawnAuthError::Key)?;
        self.cards.recover(now_ms).map_err(SpawnAuthError::Card)
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
        let terminal = match self.cards.result(card_id)? {
            None => return Ok(ClaimOutcome::UnknownCard),
            Some(CardResult::Refused { reason }) => Some(ClaimOutcome::Dead { reason }),
            Some(done @ (CardResult::Spawned { .. } | CardResult::Interrupted { .. })) => {
                Some(ClaimOutcome::AlreadyClaimed(done))
            }
            Some(CardResult::Pending { .. }) => None,
        };
        if let Some(outcome) = terminal {
            let known = self
                .keys
                .state(&wire.device_fp)
                .map_err(|_| CardError::Io)?;
            return Ok(match known {
                Some(_) => outcome,
                None => ClaimOutcome::Rejected(Refusal::UnknownKey),
            });
        }
        self.keys
            .with_pinned(&wire.device_fp, |key| match key {
                None => Ok(ClaimOutcome::Rejected(Refusal::UnknownKey)),
                Some(key) => {
                    self.cards
                        .claim(card_id, wire, &key, &TrustAdapter(&*self.trust), now_ms)
                }
            })
            .map_err(|_| CardError::Io)?
    }

    pub fn issue_for(
        &self,
        card_id: &str,
        proposal: &SpawnProposal,
        budget: CardBudget,
        now_ms: u64,
    ) -> Result<IssueOutcome, CardError> {
        self.cards.issue_for(card_id, proposal, budget, now_ms)
    }

    pub fn check(
        &self,
        card_id: &str,
        wire: &SignedSpawn,
        now_ms: u64,
    ) -> Result<CheckOutcome, CardError> {
        self.keys
            .with_pinned(&wire.device_fp, |key| match key {
                None => Ok(CheckOutcome::Rejected(Refusal::UnknownKey)),
                Some(key) => {
                    self.cards
                        .check(card_id, wire, &key, &TrustAdapter(&*self.trust), now_ms)
                }
            })
            .map_err(|_| CardError::Io)?
    }

    pub fn expire_due(&self, now_ms: u64) -> Result<Vec<String>, CardError> {
        self.cards.expire_due(now_ms)
    }

    pub fn offer_key(&self, offer: KeyOffer, now_ms: u64) -> Result<OfferOutcome, KeyError> {
        self.keys.offer(offer, &TrustAdapter(&*self.trust), now_ms)
    }

    pub fn approve_key(
        &self,
        fp_prefix: &str,
        spawn_key_prefix: &str,
        now_ms: u64,
    ) -> Result<tmgr_spawn_auth::DeviceFp, KeyError> {
        self.keys.approve(fp_prefix, spawn_key_prefix, now_ms)
    }

    pub fn revoke_key(&self, fp_prefix: &str) -> Result<RevokedKey, KeyError> {
        self.keys.revoke(fp_prefix)
    }

    pub fn list_keys(&self) -> Result<Vec<KeyInfo>, KeyError> {
        self.keys.list()
    }

    pub fn pending_keys(&self) -> Result<Vec<PendingKey>, KeyError> {
        self.keys.pending()
    }

    pub fn pinned_key(&self, device_fp: &str) -> Result<Option<PinnedKey>, KeyError> {
        self.keys.pinned_key(device_fp)
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

pub(crate) fn lock_file(path: &Path) -> std::io::Result<File> {
    use std::os::fd::AsRawFd;
    let f = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags(libc::O_NOFOLLOW)
        .mode(0o600)
        .open(path)?;
    loop {
        // SAFETY: the fd is owned by `f` and stays open for the call.
        if unsafe { libc::flock(f.as_raw_fd(), libc::LOCK_EX) } == 0 {
            return Ok(f);
        }
        let err = std::io::Error::last_os_error();
        if err.kind() != std::io::ErrorKind::Interrupted {
            return Err(err);
        }
    }
}

pub(crate) fn sync_dir(dir: &Path) -> std::io::Result<()> {
    File::open(dir)?.sync_all()
}

pub(crate) fn write_durable(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    write_durable_with(path, bytes, sync_dir)
}

fn write_durable_with(
    path: &Path,
    bytes: &[u8],
    sync: impl FnOnce(&Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
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
    sync(dir)
}

pub(crate) fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::keys::testutil::*;
    use super::*;
    use std::sync::Mutex;
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
            sandbox: None,
            prompt_sha256: None,
            name: None,
            task: None,
            target_id: TARGET_ID.into(),
            kind: tmgr_spawn_auth::ProposalKind::Spawn,
            session: None,
            from: None,
            text: None,
        }
    }

    fn rng() -> Rng {
        Arc::new(|| Ok([0xab; 16]))
    }

    fn setup(dir: &std::path::Path) -> (SpawnAuth, Arc<Source>) {
        let source = Arc::new(Source(Mutex::new(TrustStatus::Trusted)));
        let identity = Identity::new(USER, TARGET_ID, &"b".repeat(64)).unwrap();
        let auth = SpawnAuth::open_with_rng(dir, identity, source.clone(), rng()).unwrap();
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
        auth.approve_key(&fp[..8], &spawn_fp(3)[..16], NOW).unwrap();
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
        assert!(auth.pinned_key(&fp).unwrap().is_some());
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
        auth.approve_key(&fp[..8], &spawn_fp(3)[..16], NOW).unwrap();
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

    #[test]
    fn approving_with_the_wrong_spawn_key_is_refused() {
        let t = tempfile::tempdir().unwrap();
        let (auth, _) = setup(t.path());
        let o = offer(3, T0);
        let fp = fp_of(&o);
        auth.offer_key(o, NOW).unwrap();
        assert_eq!(
            auth.approve_key(&fp[..8], &spawn_fp(4)[..16], NOW),
            Err(KeyError::Approve(
                tmgr_spawn_auth::ApproveError::KeyMismatch
            ))
        );
        assert_eq!(auth.pinned_key(&fp).unwrap(), None);
        assert_eq!(auth.pending_keys().unwrap()[0].spawn_key_fp, spawn_fp(3));
    }

    #[test]
    fn a_replacement_between_issue_and_claim_is_not_ignored() {
        let t = tempfile::tempdir().unwrap();
        let (auth, _) = setup(t.path());
        let o = offer(3, T0);
        let fp = fp_of(&o);
        auth.offer_key(o, NOW).unwrap();
        auth.approve_key(&fp[..8], &spawn_fp(3)[..16], NOW).unwrap();
        let c = challenge(&auth, "card-1");
        let wire = signed(&c, "card-1", &fp);
        auth.offer_key(offer(4, T0 + 1), NOW).unwrap();
        let out = auth.claim("card-1", &wire, NOW).unwrap();
        assert_eq!(out, ClaimOutcome::Rejected(Refusal::UnknownKey));
    }

    #[test]
    fn terminal_cards_answer_before_the_key_lookup_for_known_devices_only() {
        let t = tempfile::tempdir().unwrap();
        let (auth, _) = setup(t.path());
        let o = offer(3, T0);
        let fp = fp_of(&o);
        auth.offer_key(o, NOW).unwrap();
        auth.approve_key(&fp[..8], &spawn_fp(3)[..16], NOW).unwrap();
        let pending_dev = offer_with(&kex_key(Some(1)), "dev-2", &secret(3), T0);
        let pending_fp = fp_of(&pending_dev);
        auth.offer_key(pending_dev, NOW).unwrap();
        let c = challenge(&auth, "card-1");
        auth.claim("card-1", &signed(&c, "card-1", &fp), NOW)
            .unwrap();
        let known_pending = signed(&c, "card-1", &pending_fp);
        assert!(matches!(
            auth.claim("card-1", &known_pending, NOW).unwrap(),
            ClaimOutcome::AlreadyClaimed(CardResult::Interrupted { .. })
        ));
        let stranger = signed(&c, "card-1", &"c".repeat(64));
        assert_eq!(
            auth.claim("card-1", &stranger, NOW).unwrap(),
            ClaimOutcome::Rejected(Refusal::UnknownKey)
        );
        auth.cards().mark_spawned("card-1", "sess-1", NOW).unwrap();
        assert!(matches!(
            auth.claim("card-1", &known_pending, NOW).unwrap(),
            ClaimOutcome::AlreadyClaimed(CardResult::Spawned { .. })
        ));
        assert_eq!(
            auth.claim("card-1", &stranger, NOW).unwrap(),
            ClaimOutcome::Rejected(Refusal::UnknownKey)
        );
        let c2 = challenge(&auth, "card-2");
        auth.recover(T0 + CARD_TTL_MS + 1).unwrap();
        assert_eq!(
            auth.claim("card-2", &signed(&c2, "card-2", &pending_fp), NOW)
                .unwrap(),
            ClaimOutcome::Dead {
                reason: "expired".into()
            }
        );
        assert_eq!(
            auth.claim("card-2", &signed(&c2, "card-2", &"c".repeat(64)), NOW)
                .unwrap(),
            ClaimOutcome::Rejected(Refusal::UnknownKey)
        );
        assert_eq!(
            auth.claim("card-3", &stranger, NOW).unwrap(),
            ClaimOutcome::UnknownCard
        );
    }

    #[test]
    fn open_removes_leftover_key_temp_files_without_recover() {
        let t = tempfile::tempdir().unwrap();
        let (_auth, _) = setup(t.path());
        let tmp = t.path().join("spawnauth/.keys.json.00ff00ff00ff00ff.tmp");
        std::fs::write(&tmp, "x").unwrap();
        drop(_auth);
        let _ = setup(t.path());
        assert!(!tmp.exists());
    }

    #[test]
    fn a_failing_directory_fsync_after_the_rename_is_an_error() {
        let t = tempfile::tempdir().unwrap();
        let target = t.path().join("f.json");
        let res = write_durable_with(&target, b"new", |_| Err(std::io::Error::other("fsync")));
        assert!(res.is_err());
        assert_eq!(std::fs::read(&target).unwrap(), b"new");
    }

    #[test]
    fn a_failure_before_the_rename_is_an_error_and_leaves_no_temp_file() {
        let t = tempfile::tempdir().unwrap();
        let target = t.path().join("f.json");
        std::fs::create_dir(&target).unwrap();
        let res = write_durable_with(&target, b"new", |_| Ok(()));
        assert!(res.is_err());
        let leftovers = std::fs::read_dir(t.path()).unwrap().count();
        assert_eq!(leftovers, 1);
    }

    #[test]
    fn recover_also_removes_leftover_key_temp_files() {
        let t = tempfile::tempdir().unwrap();
        let (auth, _) = setup(t.path());
        let tmp = t.path().join("spawnauth/.keys.json.00ff00ff00ff00ff.tmp");
        std::fs::write(&tmp, "x").unwrap();
        auth.recover(NOW).unwrap();
        assert!(!tmp.exists());
    }

    #[test]
    fn a_revoked_key_no_longer_verifies_and_cannot_come_back() {
        let t = tempfile::tempdir().unwrap();
        let (auth, _) = setup(t.path());
        let o = offer(3, T0);
        let fp = fp_of(&o);
        auth.offer_key(o.clone(), NOW).unwrap();
        auth.approve_key(&fp[..8], &spawn_fp(3)[..16], NOW).unwrap();
        let c = challenge(&auth, "card-1");
        let wire = signed(&c, "card-1", &fp);
        auth.revoke_key(&fp[..8]).unwrap();
        assert_eq!(auth.pinned_key(&fp).unwrap(), None);
        let out = auth.claim("card-1", &wire, NOW).unwrap();
        assert_eq!(out, ClaimOutcome::Rejected(Refusal::UnknownKey));
        assert_eq!(auth.offer_key(o, NOW).unwrap(), OfferOutcome::Revoked);
        assert!(auth.list_keys().unwrap().is_empty());
        assert_eq!(
            auth.offer_key(offer(4, T0 + 1), NOW).unwrap(),
            OfferOutcome::Added
        );
        assert_eq!(auth.list_keys().unwrap().len(), 1);
    }
}
