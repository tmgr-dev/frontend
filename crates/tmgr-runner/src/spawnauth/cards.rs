use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tmgr_spawn_auth::{
    is_hex_lower, is_target_id, is_token, Approved, PinnedKey, Refusal, SignedSpawn, SpawnProposal,
    TrustView,
};

pub const CARD_TTL_MS: u64 = 5 * 60 * 1000;
pub const MAX_PENDING: usize = 8;
pub const MAX_MANAGER_PENDING: usize = 4;
pub const ATTEMPT_LIMIT: u32 = 5;
pub const PRUNE_AFTER_MS: u64 = 30 * 24 * 60 * 60 * 1000;
pub const MAX_CARDS: usize = 512;

const EVICT_MIN_AGE_MS: u64 = 60 * 60 * 1000;
const REASON_EXPIRED: &str = "expired";
const REASON_ATTEMPTS: &str = "too_many_attempts";

pub type Rng = Arc<dyn Fn() -> std::io::Result<[u8; 16]> + Send + Sync>;

pub fn os_rng() -> Rng {
    Arc::new(|| {
        let mut b = [0u8; 16];
        getrandom::fill(&mut b).map_err(std::io::Error::other)?;
        Ok(b)
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    user_id: String,
    target_id: String,
    target_fp: String,
}

impl Identity {
    pub fn new(user_id: &str, target_id: &str, target_fp: &str) -> Option<Self> {
        (is_token(user_id) && is_target_id(target_id) && is_hex_lower(target_fp, 64)).then(|| {
            Self {
                user_id: user_id.into(),
                target_id: target_id.into(),
                target_fp: target_fp.into(),
            }
        })
    }

    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    pub fn target_id(&self) -> &str {
        &self.target_id
    }

    pub fn target_fp(&self) -> &str {
        &self.target_fp
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardBudget {
    #[default]
    Human,
    Manager,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CardState {
    Issued {
        proposal: Box<SpawnProposal>,
        nonce: String,
        issued_ms: u64,
        exp_ms: u64,
        req_sha256: String,
        target_id: String,
        target_fp: String,
        user_id: String,
        failed_attempts: u32,
        #[serde(default)]
        budget: CardBudget,
    },
    Claimed {
        claimed_ms: u64,
        #[serde(default)]
        req_sha256: String,
    },
    Spawned {
        session: String,
        #[serde(default)]
        req_sha256: String,
    },
    Refused {
        reason: String,
    },
}

impl CardState {
    pub fn req_sha256(&self) -> Option<&str> {
        match self {
            CardState::Issued { req_sha256, .. }
            | CardState::Claimed { req_sha256, .. }
            | CardState::Spawned { req_sha256, .. } => Some(req_sha256),
            CardState::Refused { .. } => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardResult {
    Pending { nonce: String, exp_ms: u64 },
    Interrupted { claimed_ms: u64 },
    Spawned { session: String },
    Refused { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Challenge {
    pub nonce: String,
    pub issued_ms: u64,
    pub exp_ms: u64,
    pub target_id: String,
    pub target_fp: String,
    pub req: Vec<u8>,
    pub req_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IssueOutcome {
    Challenge(Challenge),
    Invalid(Refusal),
    Conflict,
    Full,
    Closed(CardResult),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ClaimOutcome {
    Verified(Box<Approved>),
    Rejected(Refusal),
    AlreadyClaimed(CardResult),
    Dead { reason: String },
    UnknownCard,
}

#[derive(Debug, PartialEq, Eq)]
pub enum LocalClaim {
    Claimed(Box<SpawnProposal>),
    Rejected(Refusal),
    AlreadyClaimed(CardResult),
    Dead { reason: String },
    UnknownCard,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CheckOutcome {
    Valid,
    Rejected(Refusal),
    NotIssued,
    UnknownCard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardError {
    BadCardId,
    Io,
    Corrupt,
    NotClaimed,
    AlreadySpawned,
    BadSession,
}

impl fmt::Display for CardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            CardError::BadCardId => "invalid card id",
            CardError::Io => "spawn card store io error",
            CardError::Corrupt => "spawn card record is corrupt",
            CardError::NotClaimed => "spawn card is not claimed",
            CardError::AlreadySpawned => "spawn card already has a different session",
            CardError::BadSession => "invalid session id",
        })
    }
}

impl std::error::Error for CardError {}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RecoverReport {
    pub expired: usize,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CardFile {
    card_id: String,
    updated_ms: u64,
    state: CardState,
}

pub struct CardStore {
    dir: PathBuf,
    identity: Identity,
    rng: Rng,
}

fn check_id(card_id: &str) -> Result<&str, CardError> {
    (is_token(card_id) && !card_id.starts_with('.'))
        .then_some(card_id)
        .ok_or(CardError::BadCardId)
}

fn io<T>(r: std::io::Result<T>) -> Result<T, CardError> {
    r.map_err(|_| CardError::Io)
}

fn result_of(state: &CardState) -> CardResult {
    match state {
        CardState::Issued { nonce, exp_ms, .. } => CardResult::Pending {
            nonce: nonce.clone(),
            exp_ms: *exp_ms,
        },
        CardState::Claimed { claimed_ms, .. } => CardResult::Interrupted {
            claimed_ms: *claimed_ms,
        },
        CardState::Spawned { session, .. } => CardResult::Spawned {
            session: session.clone(),
        },
        CardState::Refused { reason } => CardResult::Refused {
            reason: reason.clone(),
        },
    }
}

fn refused(reason: &str) -> CardState {
    CardState::Refused {
        reason: reason.into(),
    }
}

impl CardStore {
    pub fn open(runner_dir: &Path, identity: Identity) -> Result<Self, CardError> {
        Self::open_inner(runner_dir, identity, os_rng())
    }

    #[cfg(test)]
    pub(crate) fn open_with_rng(
        runner_dir: &Path,
        identity: Identity,
        rng: Rng,
    ) -> Result<Self, CardError> {
        Self::open_inner(runner_dir, identity, rng)
    }

    pub fn identity(&self) -> &Identity {
        &self.identity
    }

    fn open_inner(runner_dir: &Path, identity: Identity, rng: Rng) -> Result<Self, CardError> {
        let root = runner_dir.join("spawnauth");
        io(super::ensure_private_dir(&root))?;
        let dir = root.join("cards");
        io(super::ensure_private_dir(&dir))?;
        Ok(Self { dir, identity, rng })
    }

    pub fn issue(
        &self,
        card_id: &str,
        proposal: &SpawnProposal,
        now_ms: u64,
    ) -> Result<IssueOutcome, CardError> {
        self.issue_for(card_id, proposal, CardBudget::Human, now_ms)
    }

    pub fn issue_for(
        &self,
        card_id: &str,
        proposal: &SpawnProposal,
        budget: CardBudget,
        now_ms: u64,
    ) -> Result<IssueOutcome, CardError> {
        let id = check_id(card_id)?;
        if let Err(r) = proposal.validate() {
            return Ok(IssueOutcome::Invalid(r));
        }
        if proposal.target_id != self.identity.target_id {
            return Ok(IssueOutcome::Invalid(Refusal::Mismatch));
        }
        let _lock = self.lock()?;
        let req_sha256 = proposal.req_sha256();
        if let Some(file) = self.load(id)? {
            match file.state {
                CardState::Issued {
                    proposal: stored,
                    nonce,
                    issued_ms,
                    exp_ms,
                    req_sha256: stored_sha,
                    target_id,
                    target_fp,
                    ..
                } if now_ms <= exp_ms => {
                    return Ok(if stored_sha == req_sha256 {
                        IssueOutcome::Challenge(Challenge {
                            nonce,
                            issued_ms,
                            exp_ms,
                            target_id,
                            target_fp,
                            req: stored.canonical_bytes(),
                            req_sha256: stored_sha,
                        })
                    } else {
                        IssueOutcome::Conflict
                    });
                }
                CardState::Issued { .. } => {}
                CardState::Refused { ref reason } if reason == REASON_EXPIRED => {}
                other => return Ok(IssueOutcome::Closed(result_of(&other))),
            }
        }
        let others: Vec<String> = self.ids()?.into_iter().filter(|o| o != id).collect();
        let mut pending = 0;
        let mut manager_pending = 0;
        for other in &others {
            let Ok(Some(file)) = self.load(other) else {
                continue;
            };
            if let CardState::Issued {
                exp_ms,
                budget: other_budget,
                ..
            } = file.state
            {
                if now_ms > exp_ms {
                    self.save(other, refused(REASON_EXPIRED), exp_ms)?;
                } else {
                    pending += 1;
                    manager_pending += usize::from(other_budget == CardBudget::Manager);
                    if pending >= MAX_PENDING {
                        return Ok(IssueOutcome::Full);
                    }
                }
            }
        }
        if budget == CardBudget::Manager && manager_pending >= MAX_MANAGER_PENDING {
            return Ok(IssueOutcome::Full);
        }
        if others.len() >= MAX_CARDS {
            self.prune_locked(now_ms)?;
            let left = self.ids()?.iter().filter(|o| *o != id).count();
            if left >= MAX_CARDS && !self.evict_locked(id, left + 1 - MAX_CARDS, now_ms)? {
                return Ok(IssueOutcome::Full);
            }
        }
        let nonce = super::hex_lower(&io((self.rng)())?);
        let exp_ms = now_ms.saturating_add(CARD_TTL_MS);
        self.save(
            id,
            CardState::Issued {
                proposal: Box::new(proposal.clone()),
                nonce: nonce.clone(),
                issued_ms: now_ms,
                exp_ms,
                req_sha256: req_sha256.clone(),
                target_id: self.identity.target_id.clone(),
                target_fp: self.identity.target_fp.clone(),
                user_id: self.identity.user_id.clone(),
                failed_attempts: 0,
                budget,
            },
            now_ms,
        )?;
        Ok(IssueOutcome::Challenge(Challenge {
            nonce,
            issued_ms: now_ms,
            exp_ms,
            target_id: self.identity.target_id.clone(),
            target_fp: self.identity.target_fp.clone(),
            req: proposal.canonical_bytes(),
            req_sha256,
        }))
    }

    pub fn claim(
        &self,
        card_id: &str,
        wire: &SignedSpawn,
        key: &PinnedKey,
        trust: &dyn TrustView,
        now_ms: u64,
    ) -> Result<ClaimOutcome, CardError> {
        let id = check_id(card_id)?;
        let _lock = self.lock()?;
        let Some(file) = self.load(id)? else {
            return Ok(ClaimOutcome::UnknownCard);
        };
        let CardState::Issued {
            proposal,
            nonce,
            issued_ms,
            exp_ms,
            req_sha256,
            target_id,
            target_fp,
            user_id,
            failed_attempts,
            budget,
        } = file.state
        else {
            return Ok(match file.state {
                CardState::Refused { reason } => ClaimOutcome::Dead { reason },
                other => ClaimOutcome::AlreadyClaimed(result_of(&other)),
            });
        };
        let record = tmgr_spawn_auth::ChallengeRecord {
            user_id: user_id.clone(),
            target_id: target_id.clone(),
            target_fp: target_fp.clone(),
            card_id: id.into(),
            nonce: nonce.clone(),
            issued_ms,
            exp_ms,
            proposal: (*proposal).clone(),
        };
        match tmgr_spawn_auth::verify(wire, &record, key, trust, now_ms) {
            Ok(approved) => {
                self.save(
                    id,
                    CardState::Claimed {
                        claimed_ms: now_ms,
                        req_sha256,
                    },
                    now_ms,
                )?;
                Ok(ClaimOutcome::Verified(Box::new(approved)))
            }
            Err(refusal) => {
                if refusal.counts_as_attempt() {
                    let failed_attempts = failed_attempts.saturating_add(1);
                    let next = if failed_attempts >= ATTEMPT_LIMIT {
                        refused(REASON_ATTEMPTS)
                    } else {
                        CardState::Issued {
                            proposal,
                            nonce,
                            issued_ms,
                            exp_ms,
                            req_sha256,
                            target_id,
                            target_fp,
                            user_id,
                            failed_attempts,
                            budget,
                        }
                    };
                    self.save(id, next, now_ms)?;
                }
                Ok(ClaimOutcome::Rejected(refusal))
            }
        }
    }

    pub fn claim_local(
        &self,
        card_id: &str,
        req_sha256: &str,
        now_ms: u64,
    ) -> Result<LocalClaim, CardError> {
        let id = check_id(card_id)?;
        let _lock = self.lock()?;
        let Some(file) = self.load(id)? else {
            return Ok(LocalClaim::UnknownCard);
        };
        let CardState::Issued {
            proposal,
            exp_ms,
            req_sha256: stored,
            budget,
            ..
        } = file.state
        else {
            return Ok(match file.state {
                CardState::Refused { reason } => LocalClaim::Dead { reason },
                other => LocalClaim::AlreadyClaimed(result_of(&other)),
            });
        };
        if budget != CardBudget::Manager {
            return Ok(LocalClaim::Rejected(Refusal::NotTrusted));
        }
        if now_ms > exp_ms {
            return Ok(LocalClaim::Rejected(Refusal::Expired));
        }
        if stored != req_sha256 {
            return Ok(LocalClaim::Rejected(Refusal::Mismatch));
        }
        self.save(
            id,
            CardState::Claimed {
                claimed_ms: now_ms,
                req_sha256: stored,
            },
            now_ms,
        )?;
        Ok(LocalClaim::Claimed(proposal))
    }

    pub fn refuse(&self, card_id: &str, reason: &str, now_ms: u64) -> Result<bool, CardError> {
        let id = check_id(card_id)?;
        let _lock = self.lock()?;
        match self.load(id)?.map(|f| f.state) {
            Some(CardState::Issued { .. }) => {
                self.save(id, refused(reason), now_ms)?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    pub fn mark_spawned(&self, card_id: &str, session: &str, now_ms: u64) -> Result<(), CardError> {
        let id = check_id(card_id)?;
        if !is_token(session) {
            return Err(CardError::BadSession);
        }
        let _lock = self.lock()?;
        match self.load(id)?.map(|f| f.state) {
            Some(CardState::Claimed { req_sha256, .. }) => self.save(
                id,
                CardState::Spawned {
                    session: session.into(),
                    req_sha256,
                },
                now_ms,
            ),
            Some(CardState::Spawned { session: prev, .. }) if prev == session => Ok(()),
            Some(CardState::Spawned { .. }) => Err(CardError::AlreadySpawned),
            _ => Err(CardError::NotClaimed),
        }
    }

    pub fn result(&self, card_id: &str) -> Result<Option<CardResult>, CardError> {
        Ok(self.state(card_id)?.as_ref().map(result_of))
    }

    pub fn state(&self, card_id: &str) -> Result<Option<CardState>, CardError> {
        let id = check_id(card_id)?;
        Ok(self.load(id)?.map(|f| f.state))
    }

    pub fn recover(&self, now_ms: u64) -> Result<RecoverReport, CardError> {
        let _lock = self.lock()?;
        for entry in io(std::fs::read_dir(&self.dir))?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') && name.ends_with(".tmp") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        let mut report = RecoverReport::default();
        for id in self.ids()? {
            let Ok(Some(file)) = self.load(&id) else {
                continue;
            };
            if let CardState::Issued { exp_ms, .. } = file.state {
                if now_ms <= exp_ms {
                    continue;
                }
                self.save(&id, refused(REASON_EXPIRED), exp_ms)?;
                report.expired += 1;
            }
        }
        Ok(report)
    }

    pub fn expire_due(&self, now_ms: u64) -> Result<Vec<String>, CardError> {
        let _lock = self.lock()?;
        let mut expired = Vec::new();
        for id in self.ids()? {
            let Ok(Some(file)) = self.load(&id) else {
                continue;
            };
            if let CardState::Issued { exp_ms, .. } = file.state {
                if now_ms > exp_ms {
                    self.save(&id, refused(REASON_EXPIRED), exp_ms)?;
                    expired.push(id);
                }
            }
        }
        Ok(expired)
    }

    pub fn check(
        &self,
        card_id: &str,
        wire: &SignedSpawn,
        key: &PinnedKey,
        trust: &dyn TrustView,
        now_ms: u64,
    ) -> Result<CheckOutcome, CardError> {
        let id = check_id(card_id)?;
        let _lock = self.lock()?;
        let Some(file) = self.load(id)? else {
            return Ok(CheckOutcome::UnknownCard);
        };
        let CardState::Issued {
            proposal,
            nonce,
            issued_ms,
            exp_ms,
            target_id,
            target_fp,
            user_id,
            ..
        } = file.state
        else {
            return Ok(CheckOutcome::NotIssued);
        };
        let record = tmgr_spawn_auth::ChallengeRecord {
            user_id,
            target_id,
            target_fp,
            card_id: id.into(),
            nonce,
            issued_ms,
            exp_ms,
            proposal: *proposal,
        };
        Ok(
            match tmgr_spawn_auth::verify(wire, &record, key, trust, now_ms) {
                Ok(_) => CheckOutcome::Valid,
                Err(r) => CheckOutcome::Rejected(r),
            },
        )
    }

    pub fn prune(&self, now_ms: u64) -> Result<usize, CardError> {
        let _lock = self.lock()?;
        self.prune_locked(now_ms)
    }

    fn evict_locked(&self, keep: &str, need: usize, now_ms: u64) -> Result<bool, CardError> {
        let mut expired = Vec::new();
        let mut done = Vec::new();
        for id in self.ids()? {
            let Ok(Some(file)) = self.load(&id) else {
                continue;
            };
            if id == keep || now_ms < file.updated_ms.saturating_add(EVICT_MIN_AGE_MS) {
                continue;
            }
            match file.state {
                CardState::Refused { reason } if reason == REASON_EXPIRED => {
                    expired.push((file.updated_ms, id))
                }
                CardState::Refused { .. } | CardState::Spawned { .. } => {
                    done.push((file.updated_ms, id))
                }
                CardState::Issued { .. } | CardState::Claimed { .. } => {}
            }
        }
        if expired.len() + done.len() < need {
            return Ok(false);
        }
        expired.sort();
        done.sort();
        for (_, id) in expired.into_iter().chain(done).take(need) {
            io(std::fs::remove_file(self.path(&id)))?;
        }
        io(super::sync_dir(&self.dir))?;
        Ok(true)
    }

    fn prune_locked(&self, now_ms: u64) -> Result<usize, CardError> {
        let mut removed = 0;
        for id in self.ids()? {
            let Ok(Some(file)) = self.load(&id) else {
                continue;
            };
            if now_ms > file.updated_ms.saturating_add(PRUNE_AFTER_MS)
                && std::fs::remove_file(self.path(&id)).is_ok()
            {
                removed += 1;
            }
        }
        if removed > 0 {
            io(super::sync_dir(&self.dir))?;
        }
        Ok(removed)
    }

    fn path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }

    fn lock(&self) -> Result<std::fs::File, CardError> {
        io(super::lock_file(&self.dir.join(".lock")))
    }

    fn ids(&self) -> Result<Vec<String>, CardError> {
        let mut ids = Vec::new();
        for entry in io(std::fs::read_dir(&self.dir))?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(stem) = name.strip_suffix(".json") {
                if check_id(stem).is_ok() {
                    ids.push(stem.to_string());
                }
            }
        }
        Ok(ids)
    }

    fn load(&self, id: &str) -> Result<Option<CardFile>, CardError> {
        use std::io::Read;
        use std::os::unix::fs::OpenOptionsExt;
        let f = match std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(self.path(id))
        {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(CardError::Io),
        };
        let mut bytes = Vec::new();
        io(f.take(MAX_CARD_BYTES + 1).read_to_end(&mut bytes))?;
        if bytes.len() as u64 > MAX_CARD_BYTES {
            return Err(CardError::Corrupt);
        }
        let file: CardFile = serde_json::from_slice(&bytes).map_err(|_| CardError::Corrupt)?;
        let consistent = file.card_id == id
            && match &file.state {
                CardState::Issued {
                    proposal,
                    req_sha256,
                    target_id,
                    target_fp,
                    user_id,
                    ..
                } => {
                    *req_sha256 == proposal.req_sha256()
                        && proposal.target_id == *target_id
                        && *target_id == self.identity.target_id
                        && *target_fp == self.identity.target_fp
                        && *user_id == self.identity.user_id
                }
                _ => true,
            };
        consistent
            .then_some(file)
            .map(Some)
            .ok_or(CardError::Corrupt)
    }

    fn save(&self, id: &str, state: CardState, now_ms: u64) -> Result<(), CardError> {
        let file = CardFile {
            card_id: id.into(),
            updated_ms: now_ms,
            state,
        };
        let bytes = serde_json::to_vec(&file).map_err(|_| CardError::Corrupt)?;
        io(super::write_durable(&self.path(id), &bytes))
    }
}

const MAX_CARD_BYTES: u64 = 64 * 1024;

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::sync::atomic::{AtomicU8, Ordering};
    use std::sync::{Barrier, Mutex};
    use tmgr_spawn_auth::{b64_decode_strict, TrustStatus};

    const FIXTURE: &str = include_str!("../../../tmgr-spawn-auth/tests/fixtures/spawn-v1.json");
    const ISSUED: u64 = 1_795_000_000_000;
    const NOW: u64 = 1_795_000_100_000;
    const USER: &str = "42";
    const TARGET_ID: &str = "mx_0123456789abcdef";

    fn target_fp() -> String {
        "b".repeat(64)
    }

    struct FakeTrust(Mutex<TrustStatus>);

    impl FakeTrust {
        fn trusted() -> Self {
            Self(Mutex::new(TrustStatus::Trusted))
        }
    }

    impl TrustView for FakeTrust {
        fn status(&self, _: &str, _: &str) -> TrustStatus {
            *self.0.lock().unwrap()
        }
    }

    fn fixture() -> serde_json::Value {
        serde_json::from_str(FIXTURE).unwrap()
    }

    fn case(name: &str) -> serde_json::Value {
        fixture()["verify_cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == name)
            .unwrap()
            .clone()
    }

    fn wire_of(name: &str) -> SignedSpawn {
        serde_json::from_value(case(name)["wire"].clone()).unwrap()
    }

    fn fixture_key() -> PinnedKey {
        let k = &case("ok_low_s")["pinned_key"];
        PinnedKey {
            user_id: k["user_id"].as_str().unwrap().into(),
            device_id: k["device_id"].as_str().unwrap().into(),
            device_fp: k["device_fp"].as_str().unwrap().into(),
            spawn_pub: b64_decode_strict(k["spawn_pub_b64"].as_str().unwrap()).unwrap(),
        }
    }

    fn fixture_proposal() -> SpawnProposal {
        serde_json::from_value(fixture()["proposal"]["fields"].clone()).unwrap()
    }

    fn proposal(cwd: &str) -> SpawnProposal {
        SpawnProposal {
            cwd: cwd.into(),
            ..fixture_proposal()
        }
    }

    fn fixed_rng() -> Rng {
        Arc::new(|| {
            Ok([
                0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0x01, 0x23, 0x45, 0x67, 0x89, 0xab,
                0xcd, 0xef,
            ])
        })
    }

    fn counting_rng() -> Rng {
        let n = Arc::new(AtomicU8::new(0));
        Arc::new(move || Ok([n.fetch_add(1, Ordering::SeqCst) + 1; 16]))
    }

    fn identity() -> Identity {
        Identity::new(USER, TARGET_ID, &target_fp()).unwrap()
    }

    fn store(dir: &Path, rng: Rng) -> CardStore {
        CardStore::open_with_rng(dir, identity(), rng).unwrap()
    }

    fn fixture_store(dir: &Path) -> CardStore {
        let s = store(dir, fixed_rng());
        let out = s.issue("card-0001", &fixture_proposal(), ISSUED).unwrap();
        let IssueOutcome::Challenge(c) = out else {
            panic!("{out:?}")
        };
        assert_eq!(c.nonce, "0123456789abcdef0123456789abcdef");
        s
    }

    fn mode(p: &Path) -> u32 {
        std::fs::metadata(p).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn issue_matches_the_fixture_challenge() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), fixed_rng());
        let out = s.issue("card-0001", &fixture_proposal(), ISSUED).unwrap();
        let IssueOutcome::Challenge(c) = out else {
            panic!("{out:?}")
        };
        assert_eq!(c.issued_ms, ISSUED);
        assert_eq!(c.exp_ms, ISSUED + CARD_TTL_MS);
        assert_eq!(c.exp_ms, 1_795_000_300_000);
        assert_eq!(c.req_sha256, case("ok_low_s")["wire"]["req_sha256"]);
        assert_eq!(c.req, fixture_proposal().canonical_bytes());
        assert_eq!(c.target_id, TARGET_ID);
        assert_eq!(c.target_fp, target_fp());
    }

    #[test]
    fn real_signatures_low_s_and_high_s_claim_once() {
        for name in ["ok_low_s", "ok_high_s"] {
            let t = tempfile::tempdir().unwrap();
            let s = fixture_store(t.path());
            let out = s
                .claim(
                    "card-0001",
                    &wire_of(name),
                    &fixture_key(),
                    &FakeTrust::trusted(),
                    NOW,
                )
                .unwrap();
            let ClaimOutcome::Verified(a) = out else {
                panic!("{name}: {out:?}")
            };
            assert_eq!(a.card_id(), "card-0001");
            assert_eq!(a.proposal(), &fixture_proposal());
            let Some(CardState::Claimed {
                claimed_ms,
                req_sha256,
            }) = s.state("card-0001").unwrap()
            else {
                panic!("{name}: not claimed")
            };
            assert_eq!(claimed_ms, NOW);
            assert_eq!(req_sha256, fixture_proposal().req_sha256());
            let again = s
                .claim(
                    "card-0001",
                    &wire_of(name),
                    &fixture_key(),
                    &FakeTrust::trusted(),
                    NOW,
                )
                .unwrap();
            assert_eq!(
                again,
                ClaimOutcome::AlreadyClaimed(CardResult::Interrupted { claimed_ms: NOW })
            );
        }
    }

    #[test]
    fn concurrent_claims_from_separate_stores_have_one_winner() {
        let t = tempfile::tempdir().unwrap();
        drop(fixture_store(t.path()));
        let n = 8;
        let barrier = Barrier::new(n);
        let outcomes: Vec<ClaimOutcome> = std::thread::scope(|sc| {
            let hs: Vec<_> = (0..n)
                .map(|_| {
                    sc.spawn(|| {
                        let s = store(t.path(), fixed_rng());
                        let key = fixture_key();
                        let wire = wire_of("ok_low_s");
                        let trust = FakeTrust::trusted();
                        barrier.wait();
                        s.claim("card-0001", &wire, &key, &trust, NOW).unwrap()
                    })
                })
                .collect();
            hs.into_iter().map(|h| h.join().unwrap()).collect()
        });
        let won = outcomes
            .iter()
            .filter(|o| matches!(o, ClaimOutcome::Verified(_)))
            .count();
        let lost = outcomes
            .iter()
            .filter(|o| matches!(o, ClaimOutcome::AlreadyClaimed(_)))
            .count();
        assert_eq!((won, lost), (1, n - 1), "{outcomes:?}");
    }

    #[test]
    fn crash_between_claimed_and_spawned_never_allows_a_second_claim() {
        let t = tempfile::tempdir().unwrap();
        let s = fixture_store(t.path());
        let trust = FakeTrust::trusted();
        assert!(matches!(
            s.claim(
                "card-0001",
                &wire_of("ok_low_s"),
                &fixture_key(),
                &trust,
                NOW
            )
            .unwrap(),
            ClaimOutcome::Verified(_)
        ));
        drop(s);
        let s = store(t.path(), fixed_rng());
        s.recover(NOW + 1).unwrap();
        assert_eq!(
            s.result("card-0001").unwrap(),
            Some(CardResult::Interrupted { claimed_ms: NOW })
        );
        for name in ["ok_low_s", "ok_high_s"] {
            let out = s
                .claim("card-0001", &wire_of(name), &fixture_key(), &trust, NOW + 2)
                .unwrap();
            assert!(matches!(out, ClaimOutcome::AlreadyClaimed(_)), "{out:?}");
        }
        assert!(matches!(
            s.issue("card-0001", &fixture_proposal(), NOW + 3).unwrap(),
            IssueOutcome::Closed(CardResult::Interrupted { .. })
        ));
        s.mark_spawned("card-0001", "sess-1", NOW + 4).unwrap();
        s.mark_spawned("card-0001", "sess-1", NOW + 5).unwrap();
        assert_eq!(
            s.mark_spawned("card-0001", "sess-2", NOW + 6),
            Err(CardError::AlreadySpawned)
        );
        let expect = CardResult::Spawned {
            session: "sess-1".into(),
        };
        assert_eq!(s.result("card-0001").unwrap(), Some(expect.clone()));
        let out = s
            .claim(
                "card-0001",
                &wire_of("ok_low_s"),
                &fixture_key(),
                &trust,
                NOW + 7,
            )
            .unwrap();
        assert_eq!(out, ClaimOutcome::AlreadyClaimed(expect));
    }

    #[test]
    fn mark_spawned_needs_a_claim() {
        let t = tempfile::tempdir().unwrap();
        let s = fixture_store(t.path());
        assert_eq!(
            s.mark_spawned("card-0001", "sess-1", NOW),
            Err(CardError::NotClaimed)
        );
        assert_eq!(
            s.mark_spawned("nope", "sess-1", NOW),
            Err(CardError::NotClaimed)
        );
    }

    #[test]
    fn manager_cards_are_capped_below_the_total_and_human_cards_still_issue() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), counting_rng());
        for i in 0..MAX_MANAGER_PENDING {
            let out = s
                .issue_for(
                    &format!("card-{i}"),
                    &proposal(&format!("/w/{i}")),
                    CardBudget::Manager,
                    ISSUED,
                )
                .unwrap();
            assert!(matches!(out, IssueOutcome::Challenge(_)), "{i}: {out:?}");
        }
        let fifth = s
            .issue_for("card-m", &proposal("/w/m"), CardBudget::Manager, ISSUED)
            .unwrap();
        assert_eq!(fifth, IssueOutcome::Full);
        assert!(matches!(
            s.issue("card-h", &proposal("/w/h"), ISSUED).unwrap(),
            IssueOutcome::Challenge(_)
        ));
        assert!(matches!(
            s.issue_for("card-1", &proposal("/w/1"), CardBudget::Manager, ISSUED)
                .unwrap(),
            IssueOutcome::Challenge(_)
        ));
    }

    #[test]
    fn expire_due_refuses_only_overdue_issued_cards_once() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), counting_rng());
        s.issue("card-a", &proposal("/w/a"), ISSUED).unwrap();
        s.issue("card-b", &proposal("/w/b"), ISSUED + 10_000)
            .unwrap();
        assert!(s.expire_due(ISSUED + 1).unwrap().is_empty());
        let due = ISSUED + CARD_TTL_MS + 1;
        assert_eq!(s.expire_due(due).unwrap(), vec!["card-a".to_string()]);
        assert!(s.expire_due(due).unwrap().is_empty());
        assert_eq!(
            s.result("card-a").unwrap(),
            Some(CardResult::Refused {
                reason: "expired".into()
            })
        );
        assert!(matches!(
            s.result("card-b").unwrap(),
            Some(CardResult::Pending { .. })
        ));
    }

    #[test]
    fn check_verifies_without_changing_the_card() {
        let t = tempfile::tempdir().unwrap();
        let s = fixture_store(t.path());
        let trust = FakeTrust::trusted();
        let before = s.state("card-0001").unwrap();
        let ok = s
            .check(
                "card-0001",
                &wire_of("ok_low_s"),
                &fixture_key(),
                &trust,
                NOW,
            )
            .unwrap();
        assert_eq!(ok, CheckOutcome::Valid);
        let mut bad = wire_of("ok_low_s");
        bad.sig = "AAAA".into();
        for _ in 0..ATTEMPT_LIMIT + 1 {
            let out = s
                .check("card-0001", &bad, &fixture_key(), &trust, NOW)
                .unwrap();
            assert_eq!(out, CheckOutcome::Rejected(Refusal::BadSig));
        }
        assert_eq!(s.state("card-0001").unwrap(), before);
        assert_eq!(
            s.check("nope", &bad, &fixture_key(), &trust, NOW).unwrap(),
            CheckOutcome::UnknownCard
        );
        s.claim(
            "card-0001",
            &wire_of("ok_low_s"),
            &fixture_key(),
            &trust,
            NOW,
        )
        .unwrap();
        assert_eq!(
            s.check(
                "card-0001",
                &wire_of("ok_low_s"),
                &fixture_key(),
                &trust,
                NOW
            )
            .unwrap(),
            CheckOutcome::NotIssued
        );
    }

    #[test]
    fn ninth_pending_card_is_refused() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), counting_rng());
        for i in 0..MAX_PENDING {
            let out = s
                .issue(&format!("card-{i}"), &proposal(&format!("/w/{i}")), ISSUED)
                .unwrap();
            assert!(matches!(out, IssueOutcome::Challenge(_)), "{i}: {out:?}");
        }
        assert_eq!(
            s.issue("card-9", &proposal("/w/9"), ISSUED + 1).unwrap(),
            IssueOutcome::Full
        );
        assert!(matches!(
            s.issue("card-3", &proposal("/w/3"), ISSUED + 1).unwrap(),
            IssueOutcome::Challenge(_)
        ));
        let later = ISSUED + CARD_TTL_MS + 1;
        assert!(matches!(
            s.issue("card-9", &proposal("/w/9"), later).unwrap(),
            IssueOutcome::Challenge(_)
        ));
        assert_eq!(
            s.result("card-0").unwrap(),
            Some(CardResult::Refused {
                reason: "expired".into()
            })
        );
    }

    #[test]
    fn pending_ask_repeats_the_nonce_and_expiry_gives_a_new_one() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), counting_rng());
        let p = proposal("/w/a");
        let IssueOutcome::Challenge(first) = s.issue("c1", &p, ISSUED).unwrap() else {
            panic!()
        };
        let IssueOutcome::Challenge(again) = s.issue("c1", &p, ISSUED + 10).unwrap() else {
            panic!()
        };
        assert_eq!(first, again);
        assert_eq!(
            s.issue("c1", &proposal("/w/other"), ISSUED + 10).unwrap(),
            IssueOutcome::Conflict
        );
        let at_exp = ISSUED + CARD_TTL_MS;
        let IssueOutcome::Challenge(edge) = s.issue("c1", &p, at_exp).unwrap() else {
            panic!()
        };
        assert_eq!(edge.nonce, first.nonce);
        let IssueOutcome::Challenge(fresh) = s.issue("c1", &p, at_exp + 1).unwrap() else {
            panic!()
        };
        assert_ne!(fresh.nonce, first.nonce);
        assert_eq!(fresh.issued_ms, at_exp + 1);
        assert_eq!(fresh.exp_ms, at_exp + 1 + CARD_TTL_MS);
        assert_eq!(fresh.nonce.len(), 32);
    }

    #[test]
    fn bad_signatures_count_to_five_then_the_card_is_refused() {
        let t = tempfile::tempdir().unwrap();
        let s = fixture_store(t.path());
        let trust = FakeTrust::trusted();
        let bad = wire_of("bad_sig_signed_other_nonce");
        for i in 1..ATTEMPT_LIMIT {
            let out = s
                .claim("card-0001", &bad, &fixture_key(), &trust, NOW)
                .unwrap();
            assert_eq!(out, ClaimOutcome::Rejected(Refusal::BadSig));
            let Some(CardState::Issued {
                failed_attempts, ..
            }) = s.state("card-0001").unwrap()
            else {
                panic!()
            };
            assert_eq!(failed_attempts, i);
        }
        let out = s
            .claim("card-0001", &bad, &fixture_key(), &trust, NOW)
            .unwrap();
        assert_eq!(out, ClaimOutcome::Rejected(Refusal::BadSig));
        assert_eq!(
            s.state("card-0001").unwrap(),
            Some(CardState::Refused {
                reason: "too_many_attempts".into()
            })
        );
        let out = s
            .claim(
                "card-0001",
                &wire_of("ok_low_s"),
                &fixture_key(),
                &trust,
                NOW,
            )
            .unwrap();
        assert_eq!(
            out,
            ClaimOutcome::Dead {
                reason: "too_many_attempts".into()
            }
        );
    }

    #[test]
    fn mismatch_expired_and_untrusted_do_not_count_or_burn() {
        let t = tempfile::tempdir().unwrap();
        let s = fixture_store(t.path());
        let trust = FakeTrust::trusted();
        let mut other_nonce = wire_of("ok_low_s");
        other_nonce.nonce = "f".repeat(32);
        let out = s
            .claim("card-0001", &other_nonce, &fixture_key(), &trust, NOW)
            .unwrap();
        assert_eq!(out, ClaimOutcome::Rejected(Refusal::Mismatch));
        let late = ISSUED + CARD_TTL_MS + 1;
        let out = s
            .claim(
                "card-0001",
                &wire_of("ok_low_s"),
                &fixture_key(),
                &trust,
                late,
            )
            .unwrap();
        assert_eq!(out, ClaimOutcome::Rejected(Refusal::Expired));
        *trust.0.lock().unwrap() = TrustStatus::Revoked;
        let out = s
            .claim(
                "card-0001",
                &wire_of("ok_low_s"),
                &fixture_key(),
                &trust,
                NOW,
            )
            .unwrap();
        assert_eq!(out, ClaimOutcome::Rejected(Refusal::NotTrusted));
        let Some(CardState::Issued {
            failed_attempts, ..
        }) = s.state("card-0001").unwrap()
        else {
            panic!()
        };
        assert_eq!(failed_attempts, 0);
        *trust.0.lock().unwrap() = TrustStatus::Trusted;
        assert!(matches!(
            s.claim(
                "card-0001",
                &wire_of("ok_low_s"),
                &fixture_key(),
                &trust,
                NOW
            )
            .unwrap(),
            ClaimOutcome::Verified(_)
        ));
    }

    #[test]
    fn claim_of_an_unknown_card() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), fixed_rng());
        let out = s
            .claim(
                "card-0001",
                &wire_of("ok_low_s"),
                &fixture_key(),
                &FakeTrust::trusted(),
                NOW,
            )
            .unwrap();
        assert_eq!(out, ClaimOutcome::UnknownCard);
    }

    #[test]
    fn invalid_proposals_are_refused_at_issue() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), fixed_rng());
        let mut p = fixture_proposal();
        p.cwd = "bad\ncwd".into();
        assert_eq!(
            s.issue("c1", &p, ISSUED).unwrap(),
            IssueOutcome::Invalid(Refusal::BadToken)
        );
        let mut p = fixture_proposal();
        p.target_id = "mx_ffff".into();
        assert_eq!(
            s.issue("c1", &p, ISSUED).unwrap(),
            IssueOutcome::Invalid(Refusal::Mismatch)
        );
        assert_eq!(s.state("c1").unwrap(), None);
    }

    #[test]
    fn recover_fails_expired_issued_and_keeps_claimed() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), counting_rng());
        s.issue("old", &proposal("/w/1"), ISSUED).unwrap();
        s.issue("live", &proposal("/w/2"), ISSUED + CARD_TTL_MS)
            .unwrap();
        let s2 = fixture_store(t.path());
        s2.claim(
            "card-0001",
            &wire_of("ok_low_s"),
            &fixture_key(),
            &FakeTrust::trusted(),
            NOW,
        )
        .unwrap();
        let report = s.recover(ISSUED + CARD_TTL_MS + 1).unwrap();
        assert_eq!(report.expired, 1);
        assert_eq!(
            s.state("old").unwrap(),
            Some(CardState::Refused {
                reason: "expired".into()
            })
        );
        assert!(matches!(
            s.state("live").unwrap(),
            Some(CardState::Issued { .. })
        ));
        assert!(matches!(
            s.state("card-0001").unwrap(),
            Some(CardState::Claimed { .. })
        ));
    }

    #[test]
    fn prune_removes_cards_older_than_thirty_days() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), counting_rng());
        s.issue("old", &proposal("/w/1"), ISSUED).unwrap();
        s.issue("new", &proposal("/w/2"), ISSUED + PRUNE_AFTER_MS)
            .unwrap();
        let boundary = ISSUED + CARD_TTL_MS + PRUNE_AFTER_MS;
        assert_eq!(s.prune(boundary).unwrap(), 0);
        assert_eq!(s.prune(boundary + 1).unwrap(), 1);
        assert_eq!(s.state("old").unwrap(), None);
        assert!(s.state("new").unwrap().is_some());
    }

    #[test]
    fn card_ids_that_could_escape_the_directory_are_refused() {
        let t = tempfile::tempdir().unwrap();
        let runner = t.path().join("runner");
        let s = store(&runner, fixed_rng());
        let trust = FakeTrust::trusted();
        for id in [
            "../x",
            "..",
            ".",
            ".hidden",
            "a/b",
            "",
            "a b",
            "a\0b",
            &"a".repeat(65),
        ] {
            assert_eq!(
                s.issue(id, &fixture_proposal(), ISSUED),
                Err(CardError::BadCardId),
                "{id:?}"
            );
            assert_eq!(
                s.claim(id, &wire_of("ok_low_s"), &fixture_key(), &trust, NOW),
                Err(CardError::BadCardId)
            );
            assert_eq!(s.mark_spawned(id, "s", NOW), Err(CardError::BadCardId));
            assert_eq!(s.result(id), Err(CardError::BadCardId));
            assert_eq!(s.state(id), Err(CardError::BadCardId));
        }
        let names: Vec<_> = std::fs::read_dir(runner.join("spawnauth/cards"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name())
            .collect();
        assert!(
            names.iter().all(|n| n.to_string_lossy().starts_with('.')),
            "{names:?}"
        );
        assert!(!runner.join("spawnauth/x.json").exists());
    }

    #[test]
    fn directories_are_private_and_files_are_0600() {
        let t = tempfile::tempdir().unwrap();
        let runner = t.path().join("runner");
        let s = store(&runner, fixed_rng());
        s.issue("card-0001", &fixture_proposal(), ISSUED).unwrap();
        assert_eq!(mode(&runner.join("spawnauth")), 0o700);
        assert_eq!(mode(&runner.join("spawnauth/cards")), 0o700);
        assert_eq!(mode(&runner.join("spawnauth/cards/card-0001.json")), 0o600);
        let leftovers: Vec<_> = std::fs::read_dir(runner.join("spawnauth/cards"))
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
    }

    #[test]
    fn tampered_card_files_are_reported_corrupt_without_echoing_content() {
        let t = tempfile::tempdir().unwrap();
        let s = fixture_store(t.path());
        let path = t.path().join("spawnauth/cards/card-0001.json");
        let text = std::fs::read_to_string(&path).unwrap();
        let tampered = text.replace("/work/app", "/etc");
        assert_ne!(text, tampered);
        std::fs::write(&path, tampered).unwrap();
        let err = s.state("card-0001").unwrap_err();
        assert_eq!(err, CardError::Corrupt);
        assert!(!format!("{err} {err:?}").contains("/etc"));
        std::fs::write(&path, "{not json").unwrap();
        assert_eq!(s.result("card-0001"), Err(CardError::Corrupt));
        std::fs::write(
            &path,
            text.replace("card-0001\",\"updated", "card-0002\",\"updated"),
        )
        .unwrap();
        assert_eq!(s.result("card-0001"), Err(CardError::Corrupt));
    }

    fn write_card(dir: &Path, id: &str, updated_ms: u64) {
        let json = format!(
            r#"{{"card_id":"{id}","updated_ms":{updated_ms},"state":{{"kind":"spawned","session":"s"}}}}"#
        );
        std::fs::write(dir.join(format!("spawnauth/cards/{id}.json")), json).unwrap();
    }

    #[test]
    fn issue_refuses_when_the_card_directory_is_full_after_pruning() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), counting_rng());
        s.issue("seed", &proposal("/w/0"), NOW).unwrap();
        for i in 0..MAX_CARDS {
            write_card(t.path(), &format!("fill-{i}"), NOW);
        }
        assert_eq!(
            s.issue("one-more", &proposal("/w/1"), NOW + 1).unwrap(),
            IssueOutcome::Full
        );
        assert_eq!(s.state("one-more").unwrap(), None);
    }

    #[test]
    fn issue_prunes_old_terminal_cards_to_make_room() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), counting_rng());
        s.issue("seed", &proposal("/w/0"), NOW).unwrap();
        for i in 0..MAX_CARDS {
            write_card(t.path(), &format!("old-{i}"), ISSUED);
        }
        let later = ISSUED + PRUNE_AFTER_MS + 1;
        let out = s.issue("fresh", &proposal("/w/1"), later).unwrap();
        assert!(matches!(out, IssueOutcome::Challenge(_)), "{out:?}");
        assert_eq!(s.state("old-0").unwrap(), None);
    }

    fn write_refused(dir: &Path, id: &str, reason: &str, updated_ms: u64) {
        let json = format!(
            r#"{{"card_id":"{id}","updated_ms":{updated_ms},"state":{{"kind":"refused","reason":"{reason}"}}}}"#
        );
        std::fs::write(dir.join(format!("spawnauth/cards/{id}.json")), json).unwrap();
    }

    const HOUR: u64 = 3_600_000;

    #[test]
    fn full_store_of_old_expired_cards_accepts_a_new_issue() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), counting_rng());
        for i in 0..MAX_CARDS {
            write_refused(t.path(), &format!("dead-{i}"), "expired", ISSUED + i as u64);
        }
        let out = s
            .issue("fresh", &proposal("/w/1"), ISSUED + 600 + HOUR)
            .unwrap();
        assert!(matches!(out, IssueOutcome::Challenge(_)), "{out:?}");
        assert_eq!(s.state("dead-0").unwrap(), None);
        assert!(s.state("dead-511").unwrap().is_some());
    }

    #[test]
    fn full_store_of_fresh_expired_cards_is_still_full() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), counting_rng());
        for i in 0..MAX_CARDS {
            write_refused(t.path(), &format!("dead-{i}"), "expired", ISSUED);
        }
        assert_eq!(
            s.issue("fresh", &proposal("/w/1"), ISSUED + HOUR - 1)
                .unwrap(),
            IssueOutcome::Full
        );
        assert!(s.state("dead-0").unwrap().is_some());
    }

    #[test]
    fn full_store_evicts_expired_refusals_before_other_terminal_cards() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), counting_rng());
        for i in 0..MAX_CARDS - 1 {
            write_card(t.path(), &format!("done-{i}"), ISSUED);
        }
        write_refused(t.path(), "dead", "expired", ISSUED + 1_000);
        let out = s
            .issue("fresh", &proposal("/w/1"), ISSUED + 2 * HOUR)
            .unwrap();
        assert!(matches!(out, IssueOutcome::Challenge(_)), "{out:?}");
        assert_eq!(s.state("dead").unwrap(), None);
        assert!(s.state("done-0").unwrap().is_some());
    }

    #[test]
    fn full_store_falls_back_to_other_old_terminal_cards_and_spares_fresh_ones() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), counting_rng());
        for i in 0..MAX_CARDS {
            write_card(t.path(), &format!("done-{i}"), ISSUED);
        }
        write_refused(t.path(), "attempts", "too_many_attempts", ISSUED);
        assert_eq!(
            s.issue("fresh", &proposal("/w/1"), ISSUED + HOUR - 1)
                .unwrap(),
            IssueOutcome::Full
        );
        let out = s.issue("fresh", &proposal("/w/1"), ISSUED + HOUR).unwrap();
        assert!(matches!(out, IssueOutcome::Challenge(_)), "{out:?}");
    }

    #[test]
    fn an_evicted_id_can_be_reissued_with_a_new_nonce() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), counting_rng());
        let first = match s.issue("same", &proposal("/w/1"), ISSUED).unwrap() {
            IssueOutcome::Challenge(c) => c,
            other => panic!("{other:?}"),
        };
        let later = ISSUED + CARD_TTL_MS + HOUR + 1;
        for i in 0..MAX_CARDS - 1 {
            write_refused(t.path(), &format!("dead-{i}"), "expired", later);
        }
        s.issue("other", &proposal("/w/2"), later).unwrap();
        assert_eq!(s.state("same").unwrap(), None);
        std::fs::remove_file(t.path().join("spawnauth/cards/dead-0.json")).unwrap();
        let second = match s.issue("same", &proposal("/w/1"), later + 1).unwrap() {
            IssueOutcome::Challenge(c) => c,
            other => panic!("{other:?}"),
        };
        assert_ne!(first.nonce, second.nonce);
    }

    #[test]
    fn a_failing_random_source_is_an_io_error_not_a_panic() {
        let t = tempfile::tempdir().unwrap();
        let rng: Rng = Arc::new(|| Err(std::io::Error::other("no entropy")));
        let s = store(t.path(), rng);
        assert_eq!(
            s.issue("card-1", &proposal("/w/1"), ISSUED),
            Err(CardError::Io)
        );
        assert_eq!(s.state("card-1").unwrap(), None);
    }

    #[test]
    fn mark_spawned_requires_a_token_session() {
        let t = tempfile::tempdir().unwrap();
        let s = fixture_store(t.path());
        s.claim(
            "card-0001",
            &wire_of("ok_low_s"),
            &fixture_key(),
            &FakeTrust::trusted(),
            NOW,
        )
        .unwrap();
        for bad in ["", "a b", "x:y", &"s".repeat(65)] {
            assert_eq!(
                s.mark_spawned("card-0001", bad, NOW),
                Err(CardError::BadSession),
                "{bad:?}"
            );
        }
        assert!(matches!(
            s.state("card-0001").unwrap(),
            Some(CardState::Claimed { .. })
        ));
    }

    #[test]
    fn failed_attempts_at_the_maximum_do_not_panic_or_reset() {
        let t = tempfile::tempdir().unwrap();
        let s = fixture_store(t.path());
        let file = t.path().join("spawnauth/cards/card-0001.json");
        let text = std::fs::read_to_string(&file).unwrap();
        std::fs::write(
            &file,
            text.replace("\"failed_attempts\":0", "\"failed_attempts\":4294967295"),
        )
        .unwrap();
        let out = s
            .claim(
                "card-0001",
                &wire_of("bad_sig_signed_other_nonce"),
                &fixture_key(),
                &FakeTrust::trusted(),
                NOW,
            )
            .unwrap();
        assert_eq!(out, ClaimOutcome::Rejected(Refusal::BadSig));
        assert_eq!(
            s.state("card-0001").unwrap(),
            Some(CardState::Refused {
                reason: "too_many_attempts".into()
            })
        );
    }

    #[test]
    fn a_failed_claimed_write_returns_no_verified_and_keeps_the_card_issued() {
        let t = tempfile::tempdir().unwrap();
        let s = fixture_store(t.path());
        let dir = t.path().join("spawnauth/cards");
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o500)).unwrap();
        let out = s.claim(
            "card-0001",
            &wire_of("ok_low_s"),
            &fixture_key(),
            &FakeTrust::trusted(),
            NOW,
        );
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(out, Err(CardError::Io));
        assert!(matches!(
            s.state("card-0001").unwrap(),
            Some(CardState::Issued { .. })
        ));
    }

    fn manager_card(s: &CardStore, id: &str) -> String {
        match s
            .issue_for(id, &proposal("/w/m"), CardBudget::Manager, ISSUED)
            .unwrap()
        {
            IssueOutcome::Challenge(c) => c.req_sha256,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn claim_local_claims_a_live_manager_card_once() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), counting_rng());
        let sha = manager_card(&s, "card-m");
        let LocalClaim::Claimed(p) = s.claim_local("card-m", &sha, NOW).unwrap() else {
            panic!("not claimed")
        };
        assert_eq!(*p, proposal("/w/m"));
        assert!(matches!(
            s.state("card-m").unwrap(),
            Some(CardState::Claimed {
                claimed_ms: NOW,
                ..
            })
        ));
        assert!(matches!(
            s.claim_local("card-m", &sha, NOW + 1).unwrap(),
            LocalClaim::AlreadyClaimed(CardResult::Interrupted { .. })
        ));
    }

    #[test]
    fn claim_local_refuses_a_wrong_hash_and_keeps_the_card() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), counting_rng());
        manager_card(&s, "card-m");
        let wrong = "0".repeat(64);
        assert_eq!(
            s.claim_local("card-m", &wrong, NOW).unwrap(),
            LocalClaim::Rejected(Refusal::Mismatch)
        );
        assert!(matches!(
            s.state("card-m").unwrap(),
            Some(CardState::Issued { .. })
        ));
    }

    #[test]
    fn claim_local_refuses_an_expired_card_and_a_human_card() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), counting_rng());
        let sha = manager_card(&s, "card-m");
        assert_eq!(
            s.claim_local("card-m", &sha, ISSUED + CARD_TTL_MS + 1)
                .unwrap(),
            LocalClaim::Rejected(Refusal::Expired)
        );
        let IssueOutcome::Challenge(c) = s.issue("card-h", &proposal("/w/h"), ISSUED).unwrap()
        else {
            panic!("not issued")
        };
        assert_eq!(
            s.claim_local("card-h", &c.req_sha256, NOW).unwrap(),
            LocalClaim::Rejected(Refusal::NotTrusted)
        );
        assert!(matches!(
            s.state("card-h").unwrap(),
            Some(CardState::Issued { .. })
        ));
    }

    #[test]
    fn claim_local_refuses_cards_that_are_not_issued() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), counting_rng());
        assert_eq!(
            s.claim_local("card-none", &"0".repeat(64), NOW).unwrap(),
            LocalClaim::UnknownCard
        );
        let sha = manager_card(&s, "card-m");
        assert!(s.refuse("card-m", "rejected", NOW).unwrap());
        assert_eq!(
            s.claim_local("card-m", &sha, NOW).unwrap(),
            LocalClaim::Dead {
                reason: "rejected".into()
            }
        );
    }

    #[test]
    fn two_racing_local_claims_have_one_winner() {
        let t = tempfile::tempdir().unwrap();
        let s = Arc::new(store(t.path(), counting_rng()));
        let sha = manager_card(&s, "card-m");
        let barrier = Arc::new(Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let (s, sha, b) = (s.clone(), sha.clone(), barrier.clone());
                std::thread::spawn(move || {
                    b.wait();
                    s.claim_local("card-m", &sha, NOW).unwrap()
                })
            })
            .collect();
        let won = handles
            .into_iter()
            .map(|h| h.join().unwrap())
            .filter(|o| matches!(o, LocalClaim::Claimed(_)))
            .count();
        assert_eq!(won, 1);
    }

    #[test]
    fn refuse_only_closes_an_issued_card() {
        let t = tempfile::tempdir().unwrap();
        let s = store(t.path(), counting_rng());
        let sha = manager_card(&s, "card-m");
        assert!(!s.refuse("card-none", "rejected", NOW).unwrap());
        assert!(s.refuse("card-m", "rejected", NOW).unwrap());
        assert_eq!(
            s.state("card-m").unwrap(),
            Some(CardState::Refused {
                reason: "rejected".into()
            })
        );
        assert!(!s.refuse("card-m", "edited", NOW).unwrap());
        manager_card(&s, "card-c");
        let sha_c = s
            .state("card-c")
            .unwrap()
            .unwrap()
            .req_sha256()
            .unwrap()
            .to_owned();
        s.claim_local("card-c", &sha_c, NOW).unwrap();
        assert!(!s.refuse("card-c", "rejected", NOW).unwrap());
        assert!(matches!(
            s.state("card-c").unwrap(),
            Some(CardState::Claimed { .. })
        ));
        let _ = sha;
    }
}
