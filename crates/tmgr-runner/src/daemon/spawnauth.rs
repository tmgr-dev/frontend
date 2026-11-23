use super::*;
use crate::chat::relay::store::RelayDir;
use crate::manager::{Approval, CardMiss, Proposed, Verdict};
use crate::spawnauth::{
    CardBudget, CardError, CardResult, CardState, CheckOutcome, ClaimOutcome, Identity,
    IssueOutcome, KeyOffer, LocalClaim, SpawnAuth, TrustSource,
};
use std::collections::{HashMap, VecDeque};
use tmgr_chat_relay::api::Request as RelayRequest;
use tmgr_chat_relay::engine::{
    ChatSink, DeviceState, EngineConfig, RelayEngine, Transport, TrustState,
};
use tmgr_chat_relay::engine::{SpawnKeySink, SpawnOkSink};
use tmgr_chat_relay::trust::SpawnKey;
use tmgr_ipc::api::{
    SpawnCardParams, SpawnCardResult, SpawnChallengeParams, SpawnChallengeResult,
    SpawnDecideParams, SpawnDecideResult, SpawnDecision,
};
use tmgr_ipc::chat::{
    Author, AuthorKind, ChatMessage, DecisionRef, MessageKind, SpawnCard, CHAT_V,
};
use tmgr_spawn_auth::{
    b64_encode, OfferOutcome, ProposalKind, Refusal, SignedSpawn, SpawnProposal, TrustStatus,
};

const CHALLENGES_PER_MINUTE: usize = 10;
const NO_ACCOUNT: &str = "spawn approval is not available: this machine is not linked to an account through the chat relay";
const STORE_ERROR: &str = "spawn approval store is not available";
const INTERRUPTED: &str = "interrupted; ask again with a new card";
const CARD_RT_CAP: usize = 64;
const CARD_RT_KEEP_MS: u64 = 10 * 60 * 1000;
const CARD_TICK: Duration = Duration::from_secs(15);
const OK_INBOX_CAP: usize = 16;
const OK_PER_DEVICE: usize = 4;
const OK_TICK: Duration = Duration::from_millis(500);
const SUMMARY_MAX: usize = 1024;
const REMOTE_PREFIX: &str = "remote:";
const WRONG_PROPOSER: &str = "this card was proposed by someone else; ask again with a new card";

#[derive(Default)]
pub(super) struct SpawnGate {
    slot: Mutex<Option<(Identity, Arc<SpawnAuth>)>>,
    asked: Mutex<VecDeque<u64>>,
    proposed: Mutex<VecDeque<u64>>,
    cards: Mutex<HashMap<String, CardRt>>,
    #[cfg(test)]
    pub(super) fixed: Mutex<Option<Arc<SpawnAuth>>>,
}

struct Offline;

impl Transport for Offline {
    fn call(&self, _: &RelayRequest, _: Option<&str>) -> Result<String, String> {
        Err("offline".into())
    }
}

struct Discard;

impl ChatSink for Discard {
    fn deliver(
        &self,
        _: &str,
        _: Option<&str>,
        _: tmgr_ipc::chat::ChatMessage,
    ) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Default)]
struct CardRt {
    msg_id: Option<String>,
    proposer: Option<String>,
    approval: Option<Approval>,
    exp_ms: u64,
    started_noted: bool,
    failed: bool,
    not_spawn: bool,
}

#[derive(Clone, Copy)]
enum Budget {
    Human,
    Manager,
}

impl Budget {
    fn card(self) -> CardBudget {
        match self {
            Budget::Human => CardBudget::Human,
            Budget::Manager => CardBudget::Manager,
        }
    }
}

fn card_belongs_to(bound: Option<&str>, proposer: Option<&str>) -> bool {
    match bound {
        Some(b) if b.starts_with(REMOTE_PREFIX) => Some(b) == proposer,
        _ => !proposer.is_some_and(|p| p.starts_with(REMOTE_PREFIX)),
    }
}

fn clip(mut s: String, max: usize) -> String {
    if s.len() > max {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
    }
    s
}

fn card_summary(p: &SpawnProposal) -> String {
    let session = p.session.as_deref().unwrap_or_default();
    match p.kind {
        ProposalKind::Stop => return clip(format!("Stop session {session}"), SUMMARY_MAX),
        ProposalKind::Send => {
            let from = p.from.as_deref().unwrap_or_default();
            return clip(
                format!("Send text to session {session} (from {from})"),
                SUMMARY_MAX,
            );
        }
        ProposalKind::Spawn => {}
    }
    let mode = match p.mode {
        tmgr_spawn_auth::SpawnMode::Ask => "ask",
        tmgr_spawn_auth::SpawnMode::AcceptEdits => "accept-edits",
        tmgr_spawn_auth::SpawnMode::Bypass => "bypass",
    };
    let flags = if p.sandbox.is_some() {
        format!("{mode}, sandbox off")
    } else {
        mode.to_string()
    };
    let mut line = format!("Start {} in {} ({flags})", p.profile, p.cwd);
    if let Some(what) = p.task.as_deref().or(p.name.as_deref()) {
        line.push_str(&format!(" - {what}"));
    }
    clip(line, SUMMARY_MAX)
}

fn relay_state(dir: &Path, machine: &str) -> Option<(u64, TrustState)> {
    fs::symlink_metadata(dir.join("relay")).ok()?;
    let relay = RelayDir::open(dir).ok()?;
    let user_id = relay.load_config().ok()??.user_id;
    let cfg = EngineConfig {
        user_id,
        label: "Runner".into(),
        local_machine: machine.into(),
    };
    let state = RelayEngine::new(Offline, relay, Discard, cfg)
        .ok()?
        .trust_state()
        .ok()?;
    state.registered.then_some((user_id, state))
}

fn identity_of(user_id: u64, machine: &str, state: &TrustState) -> Option<Identity> {
    Identity::new(&user_id.to_string(), machine, state.self_fp.as_deref()?)
}

fn status_of(state: &TrustState, device_id: &str, fp: &str) -> TrustStatus {
    if !state.registered {
        return TrustStatus::Unknown;
    }
    let named: Vec<_> = state.devices.iter().filter(|d| d.id == device_id).collect();
    let exact: Vec<_> = named.iter().filter(|d| d.fp == fp).collect();
    if exact.iter().any(|d| d.state == DeviceState::Revoked) {
        TrustStatus::Revoked
    } else if exact
        .iter()
        .any(|d| d.state == DeviceState::Trusted && !d.is_self)
    {
        TrustStatus::Trusted
    } else if named.is_empty() {
        TrustStatus::Unknown
    } else {
        TrustStatus::NotTrusted
    }
}

struct RelayTrust {
    dir: PathBuf,
    machine: String,
}

impl TrustSource for RelayTrust {
    fn status(&self, device_id: &str, fp: &str) -> TrustStatus {
        match relay_state(&self.dir, &self.machine) {
            Some((_, state)) => status_of(&state, device_id, fp),
            None => TrustStatus::Unknown,
        }
    }
}

impl SpawnGate {
    pub(super) fn auth(&self, rt: &Runner) -> Result<Arc<SpawnAuth>, String> {
        #[cfg(test)]
        if let Some(a) = lock(&self.fixed).clone() {
            return Ok(a);
        }
        let machine = rt.chat.machine();
        let (user_id, state) = relay_state(&rt.cfg.dir, machine).ok_or(NO_ACCOUNT)?;
        let identity = identity_of(user_id, machine, &state).ok_or(NO_ACCOUNT)?;
        let mut slot = lock(&self.slot);
        if let Some((have, auth)) = slot.as_ref() {
            if *have == identity {
                return Ok(auth.clone());
            }
        }
        let trust = Arc::new(RelayTrust {
            dir: rt.cfg.dir.clone(),
            machine: machine.to_owned(),
        });
        let auth = SpawnAuth::open(&rt.cfg.dir, identity.clone(), trust)
            .map(Arc::new)
            .map_err(|_| STORE_ERROR.to_owned())?;
        if auth.recover(now_ms()).is_err() {
            log("spawn approval: recovery of the card store failed");
        }
        *slot = Some((identity, auth.clone()));
        Ok(auth)
    }

    fn challenge_slot(&self, now: u64, budget: Budget) -> bool {
        let mut w = lock(match budget {
            Budget::Human => &self.asked,
            Budget::Manager => &self.proposed,
        });
        while w.front().is_some_and(|t| now.saturating_sub(*t) >= 60_000) {
            w.pop_front();
        }
        if w.len() >= CHALLENGES_PER_MINUTE {
            return false;
        }
        w.push_back(now);
        true
    }
}

enum Claim {
    Verified(Box<SpawnProposal>),
    Rejected(Refusal),
    Already(CardResult),
    Dead,
    Unknown,
}

fn card_error(e: CardError) -> String {
    match e {
        CardError::BadCardId => "invalid card id".into(),
        _ => STORE_ERROR.into(),
    }
}

impl Runner {
    fn spawn_challenge_within(
        &self,
        card_id: &str,
        proposal: &SpawnProposal,
        budget: Budget,
    ) -> Result<SpawnChallengeResult, ApiErr> {
        let now = now_ms();
        if !self.spawn_gate.challenge_slot(now, budget) {
            return Err(api(
                ErrorCode::Limit,
                format!("at most {CHALLENGES_PER_MINUTE} spawn challenges per minute"),
            ));
        }
        let auth = self
            .spawn_gate
            .auth(self)
            .map_err(|m| api(ErrorCode::Forbidden, m))?;
        match auth
            .issue_for(card_id, proposal, budget.card(), now)
            .map_err(|e| api(ErrorCode::InvalidParams, card_error(e)))?
        {
            IssueOutcome::Challenge(c) => Ok(SpawnChallengeResult {
                nonce: c.nonce,
                issued_ms: c.issued_ms,
                exp_ms: c.exp_ms,
                target_id: c.target_id,
                target_fp: c.target_fp,
                req: b64_encode(&c.req),
                req_sha256: c.req_sha256,
            }),
            IssueOutcome::Invalid(r) => Err(api(ErrorCode::InvalidParams, r.to_string())),
            IssueOutcome::Conflict => Err(api(
                ErrorCode::InvalidParams,
                "this card already exists with a different request",
            )),
            IssueOutcome::Full => Err(api(
                ErrorCode::Busy,
                "too many spawn cards are waiting for approval",
            )),
            IssueOutcome::Closed(_) => Err(api(ErrorCode::InvalidParams, "this card is closed")),
        }
    }

    pub(super) fn spawn_challenge_call(&self, conn: &Conn, p: &Value) -> Result<Value, ApiErr> {
        if conn.client != CLIENT_CLI && conn.client != CLIENT_DESKTOP {
            return Err(api(
                ErrorCode::Forbidden,
                "spawn challenges are available only to the tmgr CLI and the desktop app",
            ));
        }
        let q: SpawnChallengeParams = params(p)?;
        if q.proposal.kind != ProposalKind::Spawn {
            return Err(api(
                ErrorCode::InvalidParams,
                "only a spawn proposal can be challenged here",
            ));
        }
        self.spawn_challenge_within(&q.card_id, &q.proposal, Budget::Human)
            .map(|r| to_json(&r))
    }

    #[cfg(test)]
    pub(super) fn propose_spawn(&self, proposal: &SpawnProposal) -> Result<Proposed, String> {
        self.propose_spawn_by(proposal, None, None)
    }

    pub(super) fn propose_spawn_by(
        &self,
        proposal: &SpawnProposal,
        proposer: Option<&str>,
        prompt: Option<&str>,
    ) -> Result<Proposed, String> {
        let mut raw = [0u8; 16];
        getrandom::fill(&mut raw).map_err(|_| "no random source".to_string())?;
        let card_id = format!("card_{}", crate::spawnauth::hex_lower(&raw));
        let c = self
            .spawn_challenge_within(&card_id, proposal, Budget::Manager)
            .map_err(|e| e.msg)?;
        self.announce_card(&card_id, proposal, &c, proposer, prompt);
        Ok(Proposed {
            card_id,
            exp_ms: c.exp_ms,
            req_sha256: c.req_sha256,
        })
    }

    pub(super) fn verify_spawn(
        &self,
        approval: &Approval,
        proposal: &SpawnProposal,
        proposer: Option<&str>,
    ) -> Result<Verdict, String> {
        let auth = self.spawn_gate.auth(self)?;
        let card = approval.card_id();
        let bound = lock(&self.spawn_gate.cards)
            .get(card)
            .and_then(|c| c.proposer.clone());
        if !card_belongs_to(bound.as_deref(), proposer) {
            return Err(WRONG_PROPOSER.into());
        }
        let now = now_ms();
        let mismatch = || Refusal::Mismatch.to_string();
        let stored = auth.cards().state(card).map_err(card_error)?;
        if let Some(stored) = stored.as_ref().and_then(CardState::req_sha256) {
            if stored != proposal.req_sha256() {
                return Err(mismatch());
            }
        }
        let claim = match approval {
            Approval::Signed(wire) => match auth.claim(card, wire, now).map_err(card_error)? {
                ClaimOutcome::Verified(approved) => {
                    Claim::Verified(Box::new(approved.proposal().clone()))
                }
                ClaimOutcome::Rejected(r) => Claim::Rejected(r),
                ClaimOutcome::AlreadyClaimed(r) => Claim::Already(r),
                ClaimOutcome::Dead { .. } => Claim::Dead,
                ClaimOutcome::UnknownCard => Claim::Unknown,
            },
            Approval::Local { req_sha256, .. } => {
                match auth
                    .cards()
                    .claim_local(card, req_sha256, now)
                    .map_err(card_error)?
                {
                    LocalClaim::Claimed(p) => Claim::Verified(p),
                    LocalClaim::Rejected(r) => Claim::Rejected(r),
                    LocalClaim::AlreadyClaimed(r) => Claim::Already(r),
                    LocalClaim::Dead { .. } => Claim::Dead,
                    LocalClaim::UnknownCard => Claim::Unknown,
                }
            }
        };
        match claim {
            Claim::Verified(approved) => {
                if *approved != *proposal {
                    return Err(mismatch());
                }
                Ok(Verdict::Verified)
            }
            Claim::Rejected(r) => Err(r.to_string()),
            Claim::Already(CardResult::Spawned { session }) => self
                .session(&session)
                .map(|s| Verdict::Existing(Box::new(s.info())))
                .ok_or_else(|| {
                    "the session of this card is gone; ask again with a new card".into()
                }),
            Claim::Already(_) => match self.card_session(card) {
                Some(info) => {
                    self.spawn_done(card, &info.id);
                    Ok(Verdict::Existing(Box::new(info)))
                }
                None => Err(INTERRUPTED.into()),
            },
            Claim::Dead => Err("this card is closed; ask again with a new card".into()),
            Claim::Unknown => Err("unknown card".into()),
        }
    }

    pub(super) fn spawn_done(&self, card_id: &str, session: &str) {
        let spawner = lock(&self.spawn_gate.cards)
            .get(card_id)
            .and_then(|c| c.proposer.clone())
            .filter(|p| p.starts_with(REMOTE_PREFIX));
        if let Some(by) = spawner {
            self.set_spawned_by(session, &by);
        }
        let Ok(auth) = self.spawn_gate.auth(self) else {
            return;
        };
        if auth
            .cards()
            .mark_spawned(card_id, session, now_ms())
            .is_err()
        {
            log(&format!(
                "spawn approval: session {session} is running but its card was not marked"
            ));
            return;
        }
        let first = lock(&self.spawn_gate.cards)
            .get_mut(card_id)
            .is_some_and(|c| !std::mem::replace(&mut c.started_noted, true));
        if first {
            self.card_reply(card_id, &format!("Started session {session}"));
        }
    }

    pub(super) fn set_spawned_by(&self, session: &str, by: &str) {
        if let Some(s) = self.session(session) {
            lock(&s.inner).meta.spawned_by = Some(by.to_owned());
            s.persist();
        }
    }

    pub(super) fn spawn_failed(&self, card_id: &str, reason: &str) {
        if let Some(c) = lock(&self.spawn_gate.cards).get_mut(card_id) {
            c.failed = true;
        }
        let first = reason.lines().next().unwrap_or_default().trim();
        let first = clip(first.trim_end_matches('.').to_owned(), 200);
        self.card_reply(
            card_id,
            &format!("The session did not start: {first}. Ask for a new card."),
        );
    }

    fn card_session(&self, card_id: &str) -> Option<SessionInfo> {
        self.all()
            .into_iter()
            .find(|s| !s.is_exited() && lock(&s.inner).meta.spawn_card.as_deref() == Some(card_id))
            .map(|s| s.info())
    }
}

impl Runner {
    fn announce_card(
        &self,
        card_id: &str,
        proposal: &SpawnProposal,
        c: &SpawnChallengeResult,
        proposer: Option<&str>,
        prompt: Option<&str>,
    ) {
        let request = tmgr_spawn_auth::b64_decode_strict(&c.req)
            .and_then(|b| String::from_utf8(b).ok())
            .unwrap_or_default();
        let summary = card_summary(proposal);
        let prompt = if proposal.kind == ProposalKind::Send {
            proposal.text.as_deref()
        } else {
            prompt
        };
        let remote = proposer.and_then(|p| p.strip_prefix(REMOTE_PREFIX));
        let machine = self.chat.machine().to_owned();
        let msg = ChatMessage {
            v: CHAT_V,
            id: self.chat.mint_id(),
            room: self.chat.manager_room(),
            author: Author {
                kind: AuthorKind::Manager,
                id: "manager".into(),
                machine: Some(machine),
                device: None,
                agent: None,
            },
            ts_ms: now_ms(),
            kind: MessageKind::Decision,
            body_md: self.card_body(
                &summary,
                proposer,
                prompt,
                proposal.kind == ProposalKind::Send,
            ),
            attachments: Vec::new(),
            reply_to: None,
            decision: Some(DecisionRef {
                card_id: card_id.into(),
                spawn: Some(SpawnCard {
                    summary,
                    request,
                    nonce: c.nonce.clone(),
                    exp_ms: c.exp_ms,
                    target_id: c.target_id.clone(),
                    target_fp: c.target_fp.clone(),
                    proposed_by: remote.map(|from| {
                        clip(format!("top manager on {}", self.proposer_name(from)), 256)
                    }),
                    prompt: remote.and(prompt).map(String::from),
                }),
            }),
            session_meta: None,
            user_turn: false,
            mention: None,
        };
        let msg_id = match self.chat.append(msg) {
            Ok(m) => Some(m.id),
            Err(e) => {
                log(&format!(
                    "spawn approval: the card message was not posted: {e}"
                ));
                None
            }
        };
        let mut cards = lock(&self.spawn_gate.cards);
        if cards.len() >= CARD_RT_CAP {
            if let Some(oldest) = cards
                .iter()
                .min_by_key(|(_, c)| c.exp_ms)
                .map(|(k, _)| k.clone())
            {
                cards.remove(&oldest);
            }
        }
        cards.insert(
            card_id.to_owned(),
            CardRt {
                msg_id,
                proposer: proposer.map(String::from),
                exp_ms: c.exp_ms,
                not_spawn: proposal.kind != ProposalKind::Spawn,
                ..CardRt::default()
            },
        );
    }

    fn proposer_name(&self, from: &str) -> String {
        let machines = crate::mgr_rpc::Machines::load(&self.cfg.dir);
        let label = machines
            .rows()
            .into_iter()
            .find(|(id, _)| *id == from)
            .map(|(_, l)| l)
            .filter(|l| !l.is_empty());
        match label {
            Some(l) => format!("{l} ({from})"),
            None => from.to_owned(),
        }
    }

    fn card_body(
        &self,
        summary: &str,
        proposer: Option<&str>,
        prompt: Option<&str>,
        show_local: bool,
    ) -> String {
        let remote = proposer.and_then(|p| p.strip_prefix(REMOTE_PREFIX));
        if remote.is_none() && !(show_local && prompt.is_some()) {
            return summary.to_owned();
        }
        let mut body = match remote {
            Some(from) => format!(
                "Proposed by the top manager on {}.\n\n{summary}",
                self.proposer_name(from)
            ),
            None => summary.to_owned(),
        };
        if let Some(p) = prompt {
            let longest = p
                .split(|c| c != '`')
                .map(str::len)
                .max()
                .unwrap_or_default();
            let fence = "`".repeat((longest + 1).max(3));
            body.push_str(&format!("\n\n{fence}\n{p}\n{fence}"));
        }
        body
    }

    fn card_reply(&self, card_id: &str, text: &str) {
        let reply_to = lock(&self.spawn_gate.cards)
            .get(card_id)
            .and_then(|c| c.msg_id.clone());
        let room = self.chat.manager_room();
        if let Err(e) = self.chat.reply_system(&room, reply_to.as_deref(), text) {
            log(&format!("spawn approval: a card reply was not posted: {e}"));
        }
    }

    pub(super) fn approved_spawn(
        &self,
        card_id: &str,
        proposer: Option<&str>,
    ) -> Result<Approval, CardMiss> {
        let (approval, exp_ms) = {
            let cards = lock(&self.spawn_gate.cards);
            let card = cards.get(card_id).ok_or(CardMiss::Gone)?;
            if !card_belongs_to(card.proposer.as_deref(), proposer) {
                return Err(CardMiss::Gone);
            }
            if card.not_spawn {
                return Err(CardMiss::NotSpawn);
            }
            (card.approval.clone(), card.exp_ms)
        };
        if let Some(approval) = approval {
            return Ok(approval);
        }
        let issued = self.spawn_gate.auth(self).is_ok_and(|auth| {
            matches!(
                auth.cards().state(card_id),
                Ok(Some(CardState::Issued { .. }))
            )
        });
        if issued && now_ms() <= exp_ms {
            Err(CardMiss::Pending)
        } else {
            Err(CardMiss::Gone)
        }
    }

    fn card_is_approved(&self, card_id: &str) -> bool {
        lock(&self.spawn_gate.cards)
            .get(card_id)
            .is_some_and(|c| c.approval.is_some())
    }

    pub(super) fn spawned_for(&self, session: &str, from: &str) -> bool {
        self.session(session).is_some_and(|s| {
            lock(&s.inner)
                .meta
                .spawned_by
                .as_deref()
                .and_then(|p| p.strip_prefix(REMOTE_PREFIX))
                == Some(from)
        })
    }

    pub(super) fn accept_spawnok(&self, signed: &SignedSpawn, now: u64) {
        let card = signed.card_id.as_str();
        let Ok(auth) = self.spawn_gate.auth(self) else {
            log("spawn approval: an answer was dropped, no account is linked");
            return;
        };
        let manager_card = match auth.cards().state(card) {
            Ok(Some(CardState::Issued {
                budget: CardBudget::Manager,
                proposal,
                ..
            })) => Some(proposal.kind),
            Ok(_) => None,
            Err(_) => {
                log("spawn approval: an answer was dropped, the card store is not available");
                return;
            }
        };
        let Some(kind) = manager_card else {
            log("spawn approval: an answer was dropped, the card is unknown or not waiting");
            return;
        };
        match auth.check(card, signed, now) {
            Ok(CheckOutcome::Valid) => {}
            Ok(_) | Err(_) => {
                log("spawn approval: an answer was dropped, it did not verify");
                return;
            }
        }
        let proposer = {
            let mut cards = lock(&self.spawn_gate.cards);
            let entry = cards.entry(card.to_owned()).or_insert_with(|| CardRt {
                exp_ms: signed.exp_ms,
                ..CardRt::default()
            });
            if entry.approval.is_some() {
                return;
            }
            let still_issued =
                matches!(auth.cards().state(card), Ok(Some(CardState::Issued { .. })));
            if !still_issued {
                log("spawn approval: an answer was dropped, the card was closed meanwhile");
                return;
            }
            entry.approval = Some(Approval::Signed(signed.clone()));
            entry.proposer.clone()
        };
        if kind != ProposalKind::Spawn {
            self.card_reply(card, "Approved on your phone.");
            self.execute_card(card, now);
            return;
        }
        match proposer.as_deref() {
            Some(p) if p.starts_with(REMOTE_PREFIX) => {
                self.card_reply(
                    card,
                    "Approved on your phone. The top manager can start it now.",
                );
            }
            Some(session) if tmgr_ipc::proto::valid_session_id(session) => {
                self.mgr.queue_prompt(
                    session,
                    &format!(
                        "Spawn card {card} was approved. Call spawn_session with card_id \"{card}\" and the same arguments."
                    ),
                    now,
                );
                self.card_reply(card, "Approved on your phone. Starting the session.");
            }
            _ => self.card_reply(
                card,
                "Approved on your phone. The manager can start it now.",
            ),
        }
    }

    pub(super) fn card_tick(&self, now: u64) {
        let Ok(auth) = self.spawn_gate.auth(self) else {
            return;
        };
        let expired = match auth.expire_due(now) {
            Ok(ids) => ids,
            Err(_) => {
                log("spawn approval: the expiry sweep failed");
                return;
            }
        };
        for id in &expired {
            let known = lock(&self.spawn_gate.cards).contains_key(id);
            if known {
                self.card_reply(id, "Not approved: the card expired");
                lock(&self.spawn_gate.cards).remove(id);
            }
        }
        lock(&self.spawn_gate.cards).retain(|_, c| now <= c.exp_ms.saturating_add(CARD_RT_KEEP_MS));
    }

    pub(super) fn spawn_card_loop(&self) {
        let mut last = Instant::now();
        while !self.stop.load(Ordering::SeqCst) {
            thread::sleep(Duration::from_millis(250));
            if last.elapsed() < CARD_TICK {
                continue;
            }
            last = Instant::now();
            self.card_tick(now_ms());
        }
    }

    pub(super) fn spawn_card_call(&self, conn: &Conn, p: &Value) -> Result<Value, ApiErr> {
        if conn.client != CLIENT_CLI && conn.client != CLIENT_DESKTOP {
            return Err(api(
                ErrorCode::Forbidden,
                "spawn cards are available only to the tmgr CLI and the desktop app",
            ));
        }
        let q: SpawnCardParams = params(p)?;
        let auth = self
            .spawn_gate
            .auth(self)
            .map_err(|m| api(ErrorCode::Forbidden, m))?;
        let state = auth
            .cards()
            .state(&q.card_id)
            .map_err(|e| api(ErrorCode::InvalidParams, card_error(e)))?
            .ok_or_else(|| api(ErrorCode::NotFound, "unknown card"))?;
        let none = |state: &str| SpawnCardResult {
            state: state.into(),
            exp_ms: None,
            session: None,
            reason: None,
        };
        let out = match state {
            CardState::Issued { exp_ms, .. } if now_ms() > exp_ms => SpawnCardResult {
                reason: Some("expired".into()),
                ..none("refused")
            },
            CardState::Issued { exp_ms, .. } => SpawnCardResult {
                exp_ms: Some(exp_ms),
                ..none(if self.card_is_approved(&q.card_id) {
                    "approved"
                } else {
                    "issued"
                })
            },
            CardState::Claimed { claimed_ms, .. } => {
                let session = self.card_session(&q.card_id).map(|s| s.id);
                let failed = lock(&self.spawn_gate.cards)
                    .get(&q.card_id)
                    .is_some_and(|c| c.failed);
                if session.is_none() && failed {
                    SpawnCardResult {
                        reason: Some("spawn_failed".into()),
                        ..none("refused")
                    }
                } else if session.is_none() && claimed_ms < self.started_ms() {
                    SpawnCardResult {
                        reason: Some("interrupted".into()),
                        ..none("refused")
                    }
                } else {
                    SpawnCardResult {
                        session,
                        ..none("claimed")
                    }
                }
            }
            CardState::Spawned { session, .. } => SpawnCardResult {
                session: Some(session),
                ..none("spawned")
            },
            CardState::Refused { reason } => SpawnCardResult {
                reason: Some(reason),
                ..none("refused")
            },
        };
        Ok(to_json(&out))
    }
}

fn spawn_call_args(card_id: &str, p: &SpawnProposal) -> String {
    let mode = match p.mode {
        tmgr_spawn_auth::SpawnMode::Ask => "ask",
        tmgr_spawn_auth::SpawnMode::AcceptEdits => "accept-edits",
        tmgr_spawn_auth::SpawnMode::Bypass => "bypass",
    };
    let mut args = serde_json::json!({
        "card_id": card_id,
        "profile": p.profile,
        "cwd": p.cwd,
        "permission_mode": mode,
    });
    if p.sandbox.is_some() {
        args["no_sandbox"] = true.into();
    }
    if let Some(name) = &p.name {
        args["name"] = name.clone().into();
    }
    if let Some(task) = &p.task {
        args["task"] = task.clone().into();
    }
    let prompt = if p.prompt_sha256.is_some() {
        " and the same prompt as in propose_spawn"
    } else {
        ""
    };
    format!("{args}{prompt}")
}

struct Carried {
    id: String,
    reply: String,
    note: String,
}

fn kind_word(kind: ProposalKind) -> &'static str {
    match kind {
        ProposalKind::Spawn => "Spawn",
        ProposalKind::Stop => "Stop",
        ProposalKind::Send => "Send",
    }
}

fn decide_refusal(msg: &str) -> ApiErr {
    api(ErrorCode::InvalidParams, msg)
}

impl Runner {
    pub(super) fn started_ms(&self) -> u64 {
        now_ms().saturating_sub(self.started.elapsed().as_millis() as u64)
    }

    pub(super) fn spawn_decide_call(&self, conn: &Conn, p: &Value) -> Result<Value, ApiErr> {
        if conn.client != CLIENT_DESKTOP {
            return Err(api(
                ErrorCode::Forbidden,
                "spawn decisions are available only to the desktop app",
            ));
        }
        let q: SpawnDecideParams = params(p)?;
        let auth = self
            .spawn_gate
            .auth(self)
            .map_err(|m| api(ErrorCode::Forbidden, m))?;
        let now = now_ms();
        let card = q.card_id.as_str();
        let (proposal, proposer) = {
            let mut cards = lock(&self.spawn_gate.cards);
            let state = auth
                .cards()
                .state(card)
                .map_err(|e| api(ErrorCode::InvalidParams, card_error(e)))?
                .ok_or_else(|| api(ErrorCode::NotFound, "unknown card"))?;
            let CardState::Issued {
                proposal,
                exp_ms,
                req_sha256,
                budget: CardBudget::Manager,
                ..
            } = state
            else {
                return Err(decide_refusal("this card is not waiting for an answer"));
            };
            if now > exp_ms {
                return Err(decide_refusal("this card has expired"));
            }
            if req_sha256 != q.req_sha256 {
                return Err(decide_refusal(
                    "the card does not match what you saw; reload it",
                ));
            }
            let entry = cards.entry(card.to_owned()).or_insert_with(|| CardRt {
                exp_ms,
                ..CardRt::default()
            });
            if entry.approval.is_some() {
                return Err(decide_refusal("this card was already approved"));
            }
            let proposer = entry.proposer.clone();
            match q.decision {
                SpawnDecision::Approve => {
                    entry.approval = Some(Approval::Local {
                        card_id: card.to_owned(),
                        req_sha256,
                    });
                }
                SpawnDecision::Reject | SpawnDecision::Edit => {
                    if q.decision == SpawnDecision::Edit && proposal.kind != ProposalKind::Spawn {
                        return Err(decide_refusal("only a spawn card can be edited"));
                    }
                    let local_manager = proposer.as_deref().is_some_and(|p| {
                        !p.starts_with(REMOTE_PREFIX) && tmgr_ipc::proto::valid_session_id(p)
                    });
                    if q.decision == SpawnDecision::Edit && !local_manager {
                        return Err(decide_refusal(
                            "only a card of a local manager session can be edited",
                        ));
                    }
                    if q.decision == SpawnDecision::Edit && q.edit.is_none() {
                        return Err(decide_refusal("edit is required to edit a card"));
                    }
                }
            }
            (*proposal, proposer)
        };
        match q.decision {
            SpawnDecision::Approve => {
                self.card_reply(card, "Approved on the desktop.");
                if proposal.kind != ProposalKind::Spawn {
                    self.execute_card(card, now);
                    return Ok(to_json(&SpawnDecideResult {
                        state: "approved".into(),
                        card_id: None,
                        exp_ms: None,
                    }));
                }
                self.prompt_manager(
                    proposer.as_deref(),
                    &format!(
                        "Spawn card {card} was approved. Call spawn_session with {}.",
                        spawn_call_args(card, &proposal)
                    ),
                    now,
                );
                Ok(to_json(&SpawnDecideResult {
                    state: "approved".into(),
                    card_id: None,
                    exp_ms: None,
                }))
            }
            SpawnDecision::Reject => {
                self.close_card(&auth, card, "rejected", now)?;
                self.card_reply(card, "Rejected on the desktop.");
                self.prompt_manager(
                    proposer.as_deref(),
                    &format!(
                        "{} card {card} was rejected by the owner; do not retry it.",
                        kind_word(proposal.kind)
                    ),
                    now,
                );
                Ok(to_json(&SpawnDecideResult {
                    state: "rejected".into(),
                    card_id: None,
                    exp_ms: None,
                }))
            }
            SpawnDecision::Edit => {
                let edited = self.edited_proposal(&proposal, q.edit.as_ref())?;
                self.close_card(&auth, card, "edited", now)?;
                self.card_reply(card, "Edited on the desktop; replaced by a new card.");
                match self.propose_spawn_by(&edited, proposer.as_deref(), None) {
                    Ok(new) => {
                        self.prompt_manager(
                            proposer.as_deref(),
                            &format!(
                                "Spawn card {card} was edited by the owner and replaced by card {}. It is not approved yet; wait for the owner. Then call spawn_session with {}.",
                                new.card_id,
                                spawn_call_args(&new.card_id, &edited)
                            ),
                            now,
                        );
                        Ok(to_json(&SpawnDecideResult {
                            state: "edited".into(),
                            card_id: Some(new.card_id),
                            exp_ms: Some(new.exp_ms),
                        }))
                    }
                    Err(e) => {
                        self.prompt_manager(
                            proposer.as_deref(),
                            &format!("Spawn card {card} was closed by the owner; do not retry it."),
                            now,
                        );
                        Err(api(
                            ErrorCode::Busy,
                            format!("the card was closed but the edited card was not issued: {e}"),
                        ))
                    }
                }
            }
        }
    }

    fn execute_card(&self, card: &str, now: u64) {
        let Ok(auth) = self.spawn_gate.auth(self) else {
            return;
        };
        let (approval, proposer) = {
            let cards = lock(&self.spawn_gate.cards);
            match cards.get(card) {
                Some(c) => (c.approval.clone(), c.proposer.clone()),
                None => return,
            }
        };
        let Some(approval) = approval else {
            return;
        };
        let proposal = match auth.cards().state(card) {
            Ok(Some(CardState::Issued { proposal, .. })) => *proposal,
            _ => return,
        };
        let word = kind_word(proposal.kind);
        let fail = |why: &str| {
            let why = clip(why.trim_end_matches('.').to_owned(), 200);
            self.card_reply(card, &format!("Not done: {why}"));
            self.prompt_manager(
                proposer.as_deref(),
                &format!("{word} card {card} was approved but not carried out: {why}."),
                now,
            );
        };
        match self.verify_spawn(&approval, &proposal, proposer.as_deref()) {
            Ok(Verdict::Verified) => {}
            Ok(Verdict::Existing(_)) => return,
            Err(e) => return fail(&e),
        }
        let session = match self.carry_out(&proposal, proposer.as_deref(), now) {
            Ok(done) => done,
            Err(e) => return fail(&e),
        };
        if auth
            .cards()
            .mark_spawned(card, &session.id, now_ms())
            .is_err()
        {
            log(&format!(
                "spawn approval: card {card} was carried out but not marked"
            ));
        }
        self.card_reply(card, &session.reply);
        self.prompt_manager(
            proposer.as_deref(),
            &format!("{word} card {card} was approved; {}.", session.note),
            now,
        );
    }

    fn carry_out(
        &self,
        p: &SpawnProposal,
        proposer: Option<&str>,
        now: u64,
    ) -> Result<Carried, String> {
        let id = p.session.as_deref().ok_or("the card has no session")?;
        if proposer == Some(id) {
            return Err("a manager cannot act on itself".into());
        }
        let target = self.session(id).ok_or("no such session")?;
        let info = target.info();
        if info.state.kind == StateKind::Exited {
            return Err("the session has exited".into());
        }
        if info.origin.is_some() {
            return Err("sessions started by a link cannot be changed".into());
        }
        if info.team.is_some() || info.managed {
            return Err("worker sessions of a team or queue cannot be changed".into());
        }
        match p.kind {
            ProposalKind::Stop => {
                let s = self.live(id).map_err(|e| e.msg)?;
                self.signal_session(
                    &s,
                    libc::SIGTERM,
                    Some("stopped by the manager after approval"),
                );
                Ok(Carried {
                    id: id.into(),
                    reply: format!("Stopped session {id}"),
                    note: format!("session {id} was stopped"),
                })
            }
            ProposalKind::Send => {
                let text = p.text.as_deref().ok_or("the card has no text")?;
                let remote = proposer.is_some_and(|p| p.starts_with(REMOTE_PREFIX));
                if remote && info.manager {
                    return Err("manager sessions cannot be written to".into());
                }
                let unrestricted = !remote
                    && proposer
                        .and_then(|m| self.session(m))
                        .is_some_and(|m| super::manager::is_unrestricted(&m));
                let risky = info.permission_mode.as_deref() == Some("bypass") || info.no_sandbox;
                if !remote && !unrestricted && risky {
                    return Err(
                        "sessions in bypass mode or without the sandbox cannot be written to"
                            .into(),
                    );
                }
                let s = self.live(id).map_err(|e| e.msg)?;
                match s.input_at_prompt(&crate::manager::prompt_bytes(text), Instant::now()) {
                    Ok(()) => Ok(Carried {
                        id: id.into(),
                        reply: format!("Delivered to session {id}"),
                        note: format!("the text was delivered to session {id}"),
                    }),
                    Err(_) => {
                        self.mgr.queue_prompt(id, text, now);
                        Ok(Carried {
                            id: id.into(),
                            reply: format!(
                                "Queued for session {id}; it is delivered when the agent is at its prompt"
                            ),
                            note: format!(
                                "the text is queued for session {id} and is delivered when the agent is at its prompt"
                            ),
                        })
                    }
                }
            }
            ProposalKind::Spawn => Err("this is a spawn card".into()),
        }
    }

    fn close_card(
        &self,
        auth: &SpawnAuth,
        card: &str,
        reason: &str,
        now: u64,
    ) -> Result<(), ApiErr> {
        match auth.cards().refuse(card, reason, now) {
            Ok(true) => Ok(()),
            Ok(false) => Err(decide_refusal("this card is not waiting for an answer")),
            Err(e) => Err(api(ErrorCode::InvalidParams, card_error(e))),
        }
    }

    fn edited_proposal(
        &self,
        old: &SpawnProposal,
        edit: Option<&tmgr_ipc::api::SpawnDecideEdit>,
    ) -> Result<SpawnProposal, ApiErr> {
        let edit = edit.ok_or_else(|| decide_refusal("edit is required to edit a card"))?;
        let mode = match edit.mode.as_str() {
            "ask" => tmgr_spawn_auth::SpawnMode::Ask,
            "accept-edits" => tmgr_spawn_auth::SpawnMode::AcceptEdits,
            "bypass" if old.mode == tmgr_spawn_auth::SpawnMode::Bypass => {
                tmgr_spawn_auth::SpawnMode::Bypass
            }
            _ => {
                return Err(decide_refusal(
                    "mode must be ask or accept-edits (bypass only if the card asked for it)",
                ))
            }
        };
        if edit.name.as_ref().is_some_and(|n| n.len() > 64)
            || edit.task.as_ref().is_some_and(|t| t.len() > 128)
        {
            return Err(decide_refusal("name is at most 64 bytes, task at most 128"));
        }
        let dir = self
            .trusted_dir_of(&edit.cwd)
            .map_err(|e| api(ErrorCode::InvalidParams, e))?;
        let edited = SpawnProposal {
            cwd: dir.path.display().to_string(),
            mode,
            name: edit.name.clone().filter(|n| !n.is_empty()),
            task: edit.task.clone().filter(|t| !t.is_empty()),
            ..old.clone()
        };
        edited
            .validate()
            .map_err(|r| api(ErrorCode::InvalidParams, r.to_string()))?;
        Ok(edited)
    }

    fn prompt_manager(&self, proposer: Option<&str>, text: &str, now: u64) {
        if let Some(session) = proposer.filter(|s| tmgr_ipc::proto::valid_session_id(s)) {
            self.mgr.queue_prompt(session, text, now);
        }
    }
}

struct OkItem {
    device: String,
    signed: SignedSpawn,
}

pub(super) struct SpawnOkInbox {
    tx: std::sync::mpsc::SyncSender<OkItem>,
    queued: Arc<Mutex<HashMap<String, usize>>>,
}

fn spawnok_inbox() -> (SpawnOkInbox, std::sync::mpsc::Receiver<OkItem>) {
    let (tx, rx) = std::sync::mpsc::sync_channel::<OkItem>(OK_INBOX_CAP);
    let queued = Arc::new(Mutex::new(HashMap::new()));
    (SpawnOkInbox { tx, queued }, rx)
}

fn release_queued(queued: &Mutex<HashMap<String, usize>>, device: &str) {
    let mut q = lock(queued);
    if let Some(n) = q.get_mut(device) {
        *n -= 1;
        if *n == 0 {
            q.remove(device);
        }
    }
}

impl SpawnOkSink for SpawnOkInbox {
    fn deliver_spawnok(&self, from_device: &str, _: &str, signed: &SignedSpawn) {
        let mut q = lock(&self.queued);
        let n = q.get(from_device).copied().unwrap_or(0);
        let sent = n < OK_PER_DEVICE
            && self
                .tx
                .try_send(OkItem {
                    device: from_device.to_owned(),
                    signed: signed.clone(),
                })
                .is_ok();
        if sent {
            q.insert(from_device.to_owned(), n + 1);
        } else {
            log("spawn approval: an answer was dropped, the intake is busy or stopped");
        }
    }
}

impl Runner {
    pub(super) fn start_spawnok_intake(self: &Arc<Self>) -> Arc<SpawnOkInbox> {
        let (inbox, rx) = spawnok_inbox();
        let queued = inbox.queued.clone();
        let rt = self.clone();
        let spawned = thread::Builder::new()
            .name("spawnok-intake".into())
            .spawn(move || {
                while !rt.stop.load(Ordering::SeqCst) {
                    match rx.recv_timeout(OK_TICK) {
                        Ok(item) => {
                            release_queued(&queued, &item.device);
                            rt.accept_spawnok(&item.signed, now_ms());
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            });
        if spawned.is_err() {
            log("spawn approval: the answer intake did not start");
        }
        Arc::new(inbox)
    }
}

pub(crate) fn ingest_spawnkey(
    auth: &SpawnAuth,
    device_id: &str,
    sign_pub_b64: &str,
    kex_pub_b64: &str,
    key: &SpawnKey,
    now: u64,
) -> Result<OfferOutcome, &'static str> {
    auth.offer_key(
        KeyOffer {
            statement: key.statement.clone(),
            sig: key.sig.clone(),
            spawn_sig: key.spawn_sig.clone(),
            device_id: device_id.to_owned(),
            sign_pub_b64: sign_pub_b64.to_owned(),
            kex_pub_b64: kex_pub_b64.to_owned(),
        },
        now,
    )
    .map_err(|_| "spawn key rejected")
}

const KEY_INBOX_CAP: usize = 16;
const KEY_TICK: Duration = Duration::from_millis(500);

struct KeyItem {
    device_id: String,
    sign_pub: String,
    kex_pub: String,
    key: SpawnKey,
}

pub(super) struct SpawnKeyInbox {
    tx: std::sync::mpsc::SyncSender<KeyItem>,
}

impl SpawnKeySink for SpawnKeyInbox {
    fn deliver_spawnkey(&self, from: &str, _: &str, sign_pub: &str, kex_pub: &str, key: &SpawnKey) {
        let sent = self.tx.try_send(KeyItem {
            device_id: from.to_owned(),
            sign_pub: sign_pub.to_owned(),
            kex_pub: kex_pub.to_owned(),
            key: key.clone(),
        });
        if sent.is_err() {
            log("spawn approval: a spawn key statement was dropped, the intake is busy or stopped");
        }
    }
}

impl Runner {
    pub(super) fn start_spawnkey_intake(self: &Arc<Self>) -> Arc<SpawnKeyInbox> {
        let (tx, rx) = std::sync::mpsc::sync_channel::<KeyItem>(KEY_INBOX_CAP);
        let rt = self.clone();
        let spawned = thread::Builder::new()
            .name("spawnkey-intake".into())
            .spawn(move || {
                while !rt.stop.load(Ordering::SeqCst) {
                    let item = match rx.recv_timeout(KEY_TICK) {
                        Ok(item) => item,
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    let Ok(auth) = rt.spawn_gate.auth(&rt) else {
                        continue;
                    };
                    if ingest_spawnkey(
                        &auth,
                        &item.device_id,
                        &item.sign_pub,
                        &item.kex_pub,
                        &item.key,
                        now_ms(),
                    )
                    .is_err()
                    {
                        log("spawn approval: a spawn key statement was not accepted");
                    }
                }
            });
        if spawned.is_err() {
            log("spawn approval: the spawn key intake did not start");
        }
        Arc::new(SpawnKeyInbox { tx })
    }
}

#[cfg(test)]
mod tests {
    use super::manager::tests::{add, runner};
    use super::*;
    use crate::manager::{self as core, Host, SpawnSpec};
    use crate::spawnauth::testutil::{
        fp_of, kex_key, offer, offer_with, secret, sign, spawn_fp, USER,
    };

    const PHONE: &str = "00000000-0000-4000-8000-000000000001";
    use std::os::unix::net::UnixStream;
    use std::sync::atomic::AtomicU8;
    use tmgr_ipc::manager::SpoolReq;

    const MGR: &str = "s_aaaaaaaaaa";

    struct Source(Mutex<TrustStatus>);

    impl TrustSource for Source {
        fn status(&self, _: &str, _: &str) -> TrustStatus {
            *lock(&self.0)
        }
    }

    struct Env {
        _t: tempfile::TempDir,
        rt: Arc<Runner>,
        auth: Arc<SpawnAuth>,
        work: PathBuf,
        fp: String,
    }

    fn env() -> Env {
        let t = tempfile::Builder::new()
            .prefix("sa")
            .tempdir_in("/tmp")
            .unwrap();
        let rt = runner(t.path());
        add(&rt, t.path(), MGR, true);
        let work = fs::canonicalize(t.path()).unwrap().join("work");
        fs::create_dir_all(&work).unwrap();
        let identity = Identity::new(USER, rt.chat.machine(), &"b".repeat(64)).unwrap();
        let source = Arc::new(Source(Mutex::new(TrustStatus::Trusted)));
        let auth = Arc::new(SpawnAuth::open(&t.path().join("sa"), identity, source).unwrap());
        *lock(&rt.spawn_gate.fixed) = Some(auth.clone());
        let o = offer(3, now_ms() - 10_000);
        let fp = fp_of(&o);
        auth.offer_key(o, now_ms()).unwrap();
        auth.approve_key(&fp[..8], &spawn_fp(3)[..16], now_ms())
            .unwrap();
        Env {
            _t: t,
            rt,
            auth,
            work,
            fp,
        }
    }

    #[derive(Clone, Copy, PartialEq)]
    enum Fail {
        No,
        BeforeCreate,
        AfterCreate,
    }

    struct TestHost<'a> {
        inner: super::manager::ManagerHost<'a>,
        work: PathBuf,
        spawned: Mutex<Vec<(String, Option<String>)>>,
        fail: AtomicU8,
    }

    impl<'a> TestHost<'a> {
        fn new(e: &'a Env) -> Self {
            Self::with_me(e, MGR)
        }

        fn with_me(e: &'a Env, me: &'a str) -> Self {
            TestHost {
                inner: super::manager::ManagerHost {
                    rt: &e.rt,
                    me,
                    unrestricted: false,
                },
                work: e.work.clone(),
                spawned: Mutex::default(),
                fail: AtomicU8::new(0),
            }
        }

        fn fail(&self, f: Fail) {
            self.fail.store(f as u8, Ordering::SeqCst);
        }
    }

    impl Host for TestHost<'_> {
        fn me(&self) -> &str {
            self.inner.me()
        }
        fn machine(&self) -> String {
            self.inner.machine()
        }
        fn now_ms(&self) -> u64 {
            now_ms()
        }
        fn sessions(&self) -> Vec<SessionInfo> {
            self.inner.sessions()
        }
        fn text(&self, id: &str, lines: u32) -> Result<Vec<String>, String> {
            self.inner.text(id, lines)
        }
        fn send(&self, id: &str, text: &str) -> Result<(), String> {
            self.inner.send(id, text)
        }
        fn trusted_dir(&self, cwd: &str) -> Result<core::TrustedDir, String> {
            let p = PathBuf::from(cwd);
            p.starts_with(&self.work)
                .then_some(core::TrustedDir {
                    path: p,
                    pin: CwdPin { dev: 0, ino: 0 },
                })
                .ok_or_else(|| "cwd is not a trusted folder".into())
        }
        fn precheck_spawn(&self, spec: &SpawnSpec) -> Result<(), String> {
            self.inner.precheck_spawn(spec)
        }
        fn spawn(&self, spec: SpawnSpec) -> Result<SessionInfo, String> {
            let fail = self.fail.load(Ordering::SeqCst);
            if fail == Fail::BeforeCreate as u8 {
                return Err("agent did not start".into());
            }
            let id = new_session_id();
            let meta: Meta = serde_json::from_value(serde_json::json!({
                "id": id, "name": id, "argv": ["claude"], "cwd": spec.cwd,
                "profile": "claude-code", "created_ms": 0, "cols": 100, "rows": 24,
                "shim_pid": 1, "child_pid": 2, "spawn_card": spec.card,
                "permission_mode": spec.mode,
            }))
            .unwrap();
            let sdir = self.work.join("sessions").join(&id);
            fs::create_dir_all(&sdir).unwrap();
            let s = self.inner.rt.new_session(meta, None, &sdir);
            lock(&self.inner.rt.sessions).insert(id.clone(), s);
            self.spawned.lock().unwrap().push((id.clone(), spec.card));
            if fail == Fail::AfterCreate as u8 {
                return Err("lost the answer".into());
            }
            Ok(self.inner.rt.session(&id).unwrap().info())
        }
        fn verify_spawn(
            &self,
            a: &Approval,
            p: &SpawnProposal,
            by: Option<&str>,
        ) -> Result<core::Verdict, String> {
            self.inner.verify_spawn(a, p, by)
        }
        fn spawn_done(&self, c: &str, s: &str) {
            self.inner.spawn_done(c, s)
        }
        fn propose_spawn(
            &self,
            p: &SpawnProposal,
            by: Option<&str>,
            prompt: Option<&str>,
        ) -> Result<core::Proposed, String> {
            self.inner.propose_spawn(p, by, prompt)
        }
        fn spawn_failed(&self, c: &str, reason: &str) {
            self.inner.spawn_failed(c, reason)
        }
        fn approved_spawn(&self, c: &str, by: Option<&str>) -> Result<Approval, CardMiss> {
            self.inner.approved_spawn(c, by)
        }
        fn proposer(&self) -> Option<String> {
            self.inner.proposer()
        }
        fn remote_caller(&self) -> Option<&str> {
            None
        }
        fn spawned_for(&self, session: &str, from: &str) -> bool {
            self.inner.spawned_for(session, from)
        }
        fn unrestricted(&self) -> bool {
            self.inner.unrestricted()
        }
        fn switch(&self, id: &str, to: &str) -> Result<core::SwitchResult, String> {
            self.inner.switch(id, to)
        }
    }

    fn run(h: &TestHost, e: &Env, tool: &str, args: Value) -> Result<Value, String> {
        let req = SpoolReq {
            v: 1,
            id: "x".into(),
            tool: tool.into(),
            args,
        };
        core::execute(h, &e.rt.mgr, &req)
    }

    fn propose(h: &TestHost, e: &Env, mode: &str, dir: &str) -> String {
        let args = serde_json::json!({
            "profile": "claude-code", "cwd": e.work.join(dir), "permission_mode": mode, "prompt": "go"
        });
        run(h, e, "propose_spawn", args).unwrap()["card_id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn propose_no_sandbox(h: &TestHost, e: &Env, dir: &str) -> String {
        let args = serde_json::json!({
            "profile": "claude-code", "cwd": e.work.join(dir), "permission_mode": "bypass",
            "prompt": "go", "no_sandbox": true
        });
        run(h, e, "propose_spawn", args).unwrap()["card_id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn approval(e: &Env, card: &str) -> Value {
        let Some(CardState::Issued {
            nonce,
            issued_ms,
            exp_ms,
            req_sha256,
            target_id,
            target_fp,
            ..
        }) = e.auth.cards().state(card).unwrap()
        else {
            panic!("card is not issued");
        };
        let _ = issued_ms;
        let ts = now_ms();
        let msg = format!(
            "tmgr-spawn:v1:{USER}:{}:{target_id}:{target_fp}:{card}:{nonce}:{ts}:{exp_ms}:{req_sha256}",
            e.fp
        );
        serde_json::to_value(SignedSpawn {
            v: 1,
            user_id: USER.into(),
            device_fp: e.fp.clone(),
            target_id,
            target_fp,
            card_id: card.into(),
            nonce,
            ts_ms: ts,
            exp_ms,
            req_sha256,
            alg: "es256".into(),
            sig: sign(&secret(3), &msg),
        })
        .unwrap()
    }

    fn spawn_args(e: &Env, mode: &str, dir: &str, card: &str) -> Value {
        serde_json::json!({
            "profile": "claude-code", "cwd": e.work.join(dir), "permission_mode": mode,
            "prompt": "go", "approval": approval(e, card)
        })
    }

    fn with_dirs(e: &Env) {
        for d in ["a", "b"] {
            fs::create_dir_all(e.work.join(d)).unwrap();
        }
    }

    #[test]
    fn a_signed_bypass_spawn_happens_once_and_a_retry_returns_the_same_session() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let args = spawn_args(&e, "bypass", "a", &card);
        let first = run(&h, &e, "spawn_session", args.clone()).unwrap();
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
        assert_eq!(
            h.spawned.lock().unwrap()[0].1.as_deref(),
            Some(card.as_str())
        );
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Spawned { .. })
        ));
        let again = run(&h, &e, "spawn_session", args).unwrap();
        assert_eq!(again["id"], first["id"]);
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_changed_mode_or_folder_after_the_challenge_is_refused_and_keeps_the_card() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let bad_mode = spawn_args(&e, "accept-edits", "a", &card);
        assert!(run(&h, &e, "spawn_session", bad_mode)
            .unwrap_err()
            .contains("does not match"));
        let bad_cwd = spawn_args(&e, "bypass", "b", &card);
        assert!(run(&h, &e, "spawn_session", bad_cwd).is_err());
        assert!(h.spawned.lock().unwrap().is_empty());
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Issued { .. })
        ));
        let good = spawn_args(&e, "bypass", "a", &card);
        run(&h, &e, "spawn_session", good).unwrap();
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
    }

    #[test]
    fn manager_is_refused_even_when_signed_and_keeps_the_card() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let mut args = spawn_args(&e, "bypass", "a", &card);
        args["manager"] = serde_json::json!(true);
        assert!(run(&h, &e, "spawn_session", args).is_err());
        assert!(h.spawned.lock().unwrap().is_empty());
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Issued { .. })
        ));
    }

    #[test]
    fn no_sandbox_is_refused_when_the_signed_proposal_lacks_the_flag_and_keeps_the_card() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let mut args = spawn_args(&e, "bypass", "a", &card);
        args["no_sandbox"] = serde_json::json!(true);
        assert!(run(&h, &e, "spawn_session", args)
            .unwrap_err()
            .contains("does not match"));
        assert!(h.spawned.lock().unwrap().is_empty());
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Issued { .. })
        ));
    }

    #[test]
    fn a_flagged_card_refuses_a_spawn_without_the_flag_and_allows_it_with_the_flag() {
        let e = env();
        with_dirs(&e);
        let mut h = TestHost::new(&e);
        h.inner.unrestricted = true;
        let card = propose_no_sandbox(&h, &e, "a");
        let plain = spawn_args(&e, "bypass", "a", &card);
        assert!(run(&h, &e, "spawn_session", plain)
            .unwrap_err()
            .contains("does not match"));
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Issued { .. })
        ));
        let mut flagged = spawn_args(&e, "bypass", "a", &card);
        flagged["no_sandbox"] = serde_json::json!(true);
        run(&h, &e, "spawn_session", flagged).unwrap();
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Spawned { .. })
        ));
    }

    #[test]
    fn a_restricted_local_manager_cannot_propose_no_sandbox_and_an_unapproved_spawn_is_refused() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let args = serde_json::json!({
            "profile": "claude-code", "cwd": e.work.join("a"), "permission_mode": "ask",
            "no_sandbox": true
        });
        assert!(run(&h, &e, "propose_spawn", args.clone()).is_err());
        assert!(run(&h, &e, "spawn_session", args).is_err());
        assert!(h.spawned.lock().unwrap().is_empty());
    }

    #[test]
    fn the_card_and_the_carry_out_instruction_show_the_sandbox_flag() {
        let mut p: SpawnProposal = serde_json::from_value(serde_json::json!({
            "v": 1, "profile": "claude", "cwd": "/w", "mode": "bypass",
            "target_id": "mx_00"
        }))
        .unwrap();
        assert_eq!(card_summary(&p), "Start claude in /w (bypass)");
        assert!(!spawn_call_args("c", &p).contains("no_sandbox"));
        p.sandbox = Some("off".into());
        assert_eq!(card_summary(&p), "Start claude in /w (bypass, sandbox off)");
        assert!(spawn_call_args("c", &p).contains("\"no_sandbox\":true"));
    }

    #[test]
    fn an_edited_flagged_card_still_shows_sandbox_off() {
        let e = env();
        with_dirs(&e);
        trust(&e, "b");
        let mut h = TestHost::new(&e);
        h.inner.unrestricted = true;
        let card = propose_no_sandbox(&h, &e, "a");
        let out = edit(
            &e,
            &card,
            serde_json::json!({"cwd": e.work.join("b"), "mode": "accept-edits"}),
        )
        .unwrap();
        let new = out["card_id"].as_str().unwrap().to_owned();
        let Some(CardState::Issued { proposal, .. }) = e.auth.cards().state(&new).unwrap() else {
            panic!("new card is not issued")
        };
        assert_eq!(proposal.sandbox.as_deref(), Some("off"));
        let shown = card_msg(&e, &new).decision.unwrap().spawn.unwrap();
        assert!(
            shown.summary.contains("accept-edits, sandbox off"),
            "{}",
            shown.summary
        );
    }

    #[test]
    fn bypass_without_an_approval_is_still_refused() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let args = serde_json::json!({
            "profile": "claude-code", "cwd": e.work.join("a"), "permission_mode": "bypass"
        });
        assert!(run(&h, &e, "spawn_session", args).is_err());
        assert!(h.spawned.lock().unwrap().is_empty());
    }

    #[test]
    fn a_failed_spawn_after_the_claim_is_interrupted_and_a_retry_never_spawns() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let args = spawn_args(&e, "bypass", "a", &card);
        h.fail(Fail::BeforeCreate);
        assert!(run(&h, &e, "spawn_session", args.clone()).is_err());
        h.fail(Fail::No);
        let err = run(&h, &e, "spawn_session", args).unwrap_err();
        assert_eq!(err, "interrupted; ask again with a new card");
        assert!(h.spawned.lock().unwrap().is_empty());
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Claimed { .. })
        ));
    }

    #[test]
    fn an_interrupted_card_with_a_live_tagged_session_returns_that_session() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let args = spawn_args(&e, "bypass", "a", &card);
        h.fail(Fail::AfterCreate);
        assert!(run(&h, &e, "spawn_session", args.clone()).is_err());
        h.fail(Fail::No);
        let v = run(&h, &e, "spawn_session", args.clone()).unwrap();
        assert_eq!(v["id"], h.spawned.lock().unwrap()[0].0);
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Spawned { .. })
        ));
    }

    #[test]
    fn signed_spawns_share_the_per_minute_spawn_limit() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let mut last = String::new();
        for _ in 0..tmgr_ipc::manager::limits::SPAWNS_PER_MINUTE {
            let card = propose(&h, &e, "bypass", "a");
            run(
                &h,
                &e,
                "spawn_session",
                spawn_args(&e, "bypass", "a", &card),
            )
            .unwrap();
            last = card;
        }
        let card = propose(&h, &e, "bypass", "a");
        let err = run(
            &h,
            &e,
            "spawn_session",
            spawn_args(&e, "bypass", "a", &card),
        )
        .unwrap_err();
        assert!(err.contains("per minute"));
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Issued { .. })
        ));
        assert!(!last.is_empty());
    }

    #[test]
    fn a_retry_of_a_spawned_card_with_other_arguments_is_refused_and_returns_no_session() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let args = spawn_args(&e, "bypass", "a", &card);
        let first = run(&h, &e, "spawn_session", args.clone()).unwrap();
        for (key, other) in [
            ("permission_mode", serde_json::json!("accept-edits")),
            ("cwd", serde_json::json!(e.work.join("b"))),
            ("prompt", serde_json::json!("different")),
        ] {
            let mut changed = args.clone();
            changed[key] = other;
            let err = run(&h, &e, "spawn_session", changed).unwrap_err();
            assert_eq!(err, Refusal::Mismatch.to_string(), "{key}");
        }
        assert_eq!(
            run(&h, &e, "spawn_session", args).unwrap()["id"],
            first["id"]
        );
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
    }

    #[test]
    fn an_interrupted_card_healed_with_other_arguments_is_refused_and_stays_claimed() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let args = spawn_args(&e, "bypass", "a", &card);
        h.fail(Fail::AfterCreate);
        assert!(run(&h, &e, "spawn_session", args.clone()).is_err());
        h.fail(Fail::No);
        let mut changed = args;
        changed["permission_mode"] = serde_json::json!("accept-edits");
        let err = run(&h, &e, "spawn_session", changed).unwrap_err();
        assert_eq!(err, Refusal::Mismatch.to_string());
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Claimed { .. })
        ));
    }

    #[test]
    fn a_profile_that_cannot_bypass_is_refused_before_the_card_is_claimed() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let args = serde_json::json!({
            "profile": "opencode", "cwd": e.work.join("a"), "permission_mode": "bypass", "prompt": "go"
        });
        let card = run(&h, &e, "propose_spawn", args.clone()).unwrap()["card_id"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut signed = args;
        signed["approval"] = approval(&e, &card);
        let err = run(&h, &e, "spawn_session", signed).unwrap_err();
        assert!(err.contains("cannot run in mode bypass"), "{err}");
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Issued { .. })
        ));
    }

    #[test]
    fn refused_verifications_do_not_use_up_the_spawn_rate_window() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let mut bad = spawn_args(&e, "bypass", "a", &card);
        bad["approval"]["sig"] = serde_json::json!("AAAA");
        for _ in 0..tmgr_ipc::manager::limits::SPAWNS_PER_MINUTE + 2 {
            let err = run(&h, &e, "spawn_session", bad.clone()).unwrap_err();
            assert!(!err.contains("per minute"), "{err}");
        }
    }

    #[test]
    fn the_manager_and_the_human_have_separate_challenge_budgets() {
        let gate = SpawnGate::default();
        for _ in 0..CHALLENGES_PER_MINUTE {
            assert!(gate.challenge_slot(1_000, Budget::Manager));
        }
        assert!(!gate.challenge_slot(1_000, Budget::Manager));
        for _ in 0..CHALLENGES_PER_MINUTE {
            assert!(gate.challenge_slot(1_000, Budget::Human));
        }
        assert!(!gate.challenge_slot(1_000, Budget::Human));
        assert!(gate.challenge_slot(61_000, Budget::Manager));
    }

    #[test]
    fn a_looping_manager_cannot_use_up_the_human_challenge_budget() {
        let e = env();
        let proposal: SpawnProposal = serde_json::from_value(serde_json::json!({
            "v": 1, "profile": "claude-code", "cwd": e.work.display().to_string(),
            "mode": "bypass", "target_id": e.rt.chat.machine()
        }))
        .unwrap();
        let mut limited = false;
        for _ in 0..CHALLENGES_PER_MINUTE + 2 {
            limited |= matches!(
                e.rt.propose_spawn(&proposal),
                Err(m) if m.contains("per minute")
            );
        }
        assert!(limited);
        challenge(&e, CLIENT_CLI, "card_human").unwrap();
    }

    fn room_msgs(e: &Env) -> Vec<ChatMessage> {
        e.rt.chat
            .history(&e.rt.chat.manager_room(), None, Some(200))
            .unwrap()
    }

    fn card_msg(e: &Env, card: &str) -> ChatMessage {
        room_msgs(e)
            .into_iter()
            .find(|m| m.decision.as_ref().is_some_and(|d| d.card_id == card))
            .expect("card message")
    }

    fn replies(e: &Env, to: &str) -> Vec<String> {
        room_msgs(e)
            .into_iter()
            .filter(|m| m.reply_to.as_deref() == Some(to))
            .map(|m| m.body_md)
            .collect()
    }

    #[test]
    fn one_flooding_device_cannot_crowd_out_the_phone_answer() {
        let signed: SignedSpawn = serde_json::from_value(serde_json::json!({
            "v": 1, "user_id": "42", "device_fp": "ab".repeat(32), "target_id": "d_runner1",
            "target_fp": "cd".repeat(32), "card_id": "card_00112233445566778899aabbccddeeff",
            "nonce": "ab".repeat(16), "ts_ms": 1, "exp_ms": 2, "req_sha256": "ef".repeat(32),
            "alg": "es256", "sig": "c2ln"
        }))
        .unwrap();
        let (inbox, rx) = spawnok_inbox();
        for _ in 0..OK_INBOX_CAP {
            inbox.deliver_spawnok("d_flood", "ff", &signed);
        }
        inbox.deliver_spawnok("phone", "ab", &signed);
        let from: Vec<String> = rx.try_iter().map(|i| i.device).collect();
        assert_eq!(
            from.iter().filter(|d| *d == "d_flood").count(),
            OK_PER_DEVICE
        );
        assert!(from.iter().any(|d| d == "phone"));

        for _ in 0..OK_PER_DEVICE {
            release_queued(&inbox.queued, "d_flood");
        }
        inbox.deliver_spawnok("d_flood", "ff", &signed);
        assert_eq!(rx.try_iter().count(), 1);
        assert_eq!(lock(&inbox.queued).get("d_flood"), Some(&1));
    }

    fn phone_answer(e: &Env, msg: &ChatMessage) -> SignedSpawn {
        let d = msg.decision.as_ref().unwrap();
        let c = d.spawn.as_ref().unwrap();
        let req_sha256 = tmgr_spawn_auth::sha256_hex(c.request.as_bytes());
        let ts = now_ms();
        let card = &d.card_id;
        let signed = format!(
            "tmgr-spawn:v1:{USER}:{}:{}:{}:{card}:{}:{ts}:{}:{req_sha256}",
            e.fp, c.target_id, c.target_fp, c.nonce, c.exp_ms
        );
        SignedSpawn {
            v: 1,
            user_id: USER.into(),
            device_fp: e.fp.clone(),
            target_id: c.target_id.clone(),
            target_fp: c.target_fp.clone(),
            card_id: card.clone(),
            nonce: c.nonce.clone(),
            ts_ms: ts,
            exp_ms: c.exp_ms,
            req_sha256,
            alg: "es256".into(),
            sig: sign(&secret(3), &signed),
        }
    }

    fn manager_prompts(e: &Env) -> Vec<(String, String)> {
        let mut out = Vec::new();
        e.rt.mgr.deliver_prompts(now_ms(), |p| {
            out.push((p.session.clone(), p.text.clone()));
            core::Delivery::Done
        });
        out
    }

    fn card_args(e: &Env, card: &str) -> Value {
        serde_json::json!({
            "profile": "claude-code", "cwd": e.work.join("a"), "permission_mode": "bypass",
            "prompt": "go", "card_id": card
        })
    }

    fn card_state(e: &Env, client: &str, card: &str) -> Result<SpawnCardResult, ApiErr> {
        let (c, _peer) = conn(client);
        e.rt.dispatch(
            &c,
            &Request {
                id: 1,
                method: method::SPAWN_CARD.into(),
                params: serde_json::json!({"card_id": card}),
            },
        )
        .map(|v| serde_json::from_value(v).unwrap())
    }

    fn wait_for(what: &str, mut f: impl FnMut() -> bool) {
        let end = Instant::now() + Duration::from_secs(5);
        while !f() {
            assert!(Instant::now() < end, "timed out waiting for {what}");
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_phone_approval_starts_exactly_one_session_and_reports_back_in_the_room() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let msg = card_msg(&e, &card);
        assert_eq!(msg.kind, MessageKind::Decision);
        assert_eq!(msg.author.kind, AuthorKind::Manager);
        let spawn = msg.decision.as_ref().unwrap().spawn.as_ref().unwrap();
        assert!(spawn.summary.starts_with("Start claude-code in "));
        assert!(spawn.summary.contains("(bypass)"));
        let Some(CardState::Issued {
            nonce,
            exp_ms,
            req_sha256,
            target_id,
            target_fp,
            ..
        }) = e.auth.cards().state(&card).unwrap()
        else {
            panic!("card is not issued");
        };
        assert_eq!(
            tmgr_spawn_auth::sha256_hex(spawn.request.as_bytes()),
            req_sha256
        );
        assert_eq!(
            (
                &spawn.nonce,
                spawn.exp_ms,
                &spawn.target_id,
                &spawn.target_fp
            ),
            (&nonce, exp_ms, &target_id, &target_fp)
        );
        assert_eq!(card_state(&e, CLIENT_CLI, &card).unwrap().state, "issued");

        let signed = phone_answer(&e, &msg);
        let inbox = e.rt.start_spawnok_intake();
        inbox.deliver_spawnok("phone", &e.fp, &signed);
        wait_for("the approval", || {
            e.rt.approved_spawn(&card, Some(MGR)).is_ok() && !replies(&e, &msg.id).is_empty()
        });
        assert_eq!(
            replies(&e, &msg.id),
            vec!["Approved on your phone. Starting the session."]
        );
        let notes = manager_prompts(&e);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].0, MGR);
        assert!(notes[0].1.contains(&card) && notes[0].1.contains("spawn_session"));
        let got = card_state(&e, CLIENT_DESKTOP, &card).unwrap();
        assert_eq!((got.state.as_str(), got.exp_ms), ("approved", Some(exp_ms)));

        inbox.deliver_spawnok("phone", &e.fp, &signed);
        e.rt.accept_spawnok(&signed, now_ms());
        assert_eq!(replies(&e, &msg.id).len(), 1);
        assert!(manager_prompts(&e).is_empty());

        let first = run(&h, &e, "spawn_session", card_args(&e, &card)).unwrap();
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
        let session = first["id"].as_str().unwrap().to_owned();
        assert!(session.starts_with("s_"));
        assert_eq!(
            replies(&e, &msg.id),
            vec![
                "Approved on your phone. Starting the session.".to_string(),
                format!("Started session {session}")
            ]
        );
        let done = card_state(&e, CLIENT_CLI, &card).unwrap();
        assert_eq!(done.state, "spawned");
        assert_eq!(done.session.as_deref(), Some(session.as_str()));

        let again = run(&h, &e, "spawn_session", card_args(&e, &card)).unwrap();
        assert_eq!(again["id"], first["id"]);
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
        assert_eq!(replies(&e, &msg.id).len(), 2);
        e.rt.stop.store(true, Ordering::SeqCst);
    }

    const PEER: &str = "mx_00000000000000bb";
    const GONE: &str = "this card has expired or is closed; propose the spawn again";

    struct As<'a, 'b> {
        inner: &'a TestHost<'b>,
        proposer: Option<String>,
        remote: Option<&'static str>,
    }

    impl<'a, 'b> As<'a, 'b> {
        fn remote(inner: &'a TestHost<'b>) -> Self {
            As {
                inner,
                proposer: Some(format!("remote:{PEER}")),
                remote: Some(PEER),
            }
        }
    }

    impl Host for As<'_, '_> {
        fn me(&self) -> &str {
            self.inner.me()
        }
        fn machine(&self) -> String {
            self.inner.machine()
        }
        fn now_ms(&self) -> u64 {
            now_ms()
        }
        fn sessions(&self) -> Vec<SessionInfo> {
            self.inner.sessions()
        }
        fn text(&self, id: &str, lines: u32) -> Result<Vec<String>, String> {
            self.inner.text(id, lines)
        }
        fn send(&self, id: &str, text: &str) -> Result<(), String> {
            self.inner.send(id, text)
        }
        fn trusted_dir(&self, cwd: &str) -> Result<core::TrustedDir, String> {
            self.inner.trusted_dir(cwd)
        }
        fn precheck_spawn(&self, spec: &SpawnSpec) -> Result<(), String> {
            self.inner.precheck_spawn(spec)
        }
        fn spawn(&self, spec: SpawnSpec) -> Result<SessionInfo, String> {
            self.inner.spawn(spec)
        }
        fn verify_spawn(
            &self,
            a: &Approval,
            p: &SpawnProposal,
            by: Option<&str>,
        ) -> Result<core::Verdict, String> {
            self.inner.verify_spawn(a, p, by)
        }
        fn spawn_done(&self, c: &str, s: &str) {
            self.inner.spawn_done(c, s)
        }
        fn propose_spawn(
            &self,
            p: &SpawnProposal,
            by: Option<&str>,
            prompt: Option<&str>,
        ) -> Result<core::Proposed, String> {
            self.inner.propose_spawn(p, by, prompt)
        }
        fn spawn_failed(&self, c: &str, reason: &str) {
            self.inner.spawn_failed(c, reason)
        }
        fn approved_spawn(&self, c: &str, by: Option<&str>) -> Result<Approval, CardMiss> {
            self.inner.approved_spawn(c, by)
        }
        fn proposer(&self) -> Option<String> {
            self.proposer.clone()
        }
        fn remote_caller(&self) -> Option<&str> {
            self.remote
        }
        fn spawned_for(&self, session: &str, from: &str) -> bool {
            self.inner.spawned_for(session, from)
        }
        fn switch(&self, id: &str, to: &str) -> Result<core::SwitchResult, String> {
            self.inner.switch(id, to)
        }
    }

    fn run_as(h: &dyn Host, e: &Env, tool: &str, args: Value) -> Result<Value, String> {
        let req = SpoolReq {
            v: 1,
            id: "x".into(),
            tool: tool.into(),
            args,
        };
        core::execute(h, &e.rt.mgr, &req)
    }

    fn propose_as(h: &dyn Host, e: &Env, prompt: &str) -> String {
        let args = serde_json::json!({
            "profile": "claude-code", "cwd": e.work.join("a"), "permission_mode": "bypass",
            "prompt": prompt
        });
        run_as(h, e, "propose_spawn", args).unwrap()["card_id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn approve_on_phone(e: &Env, card: &str) -> ChatMessage {
        let msg = card_msg(e, card);
        let signed = phone_answer(e, &msg);
        e.rt.accept_spawnok(&signed, now_ms());
        msg
    }

    #[test]
    fn a_remote_card_names_the_top_manager_and_quotes_the_whole_prompt() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let remote = As::remote(&h);
        let card = propose_as(&remote, &e, "first line\n\nthird line");
        let body = card_msg(&e, &card).body_md;
        assert!(
            body.starts_with(&format!("Proposed by the top manager on {PEER}.\n\nStart ")),
            "{body}"
        );
        assert!(
            body.ends_with("\n\n```\nfirst line\n\nthird line\n```"),
            "{body}"
        );
        let spawn = card_msg(&e, &card).decision.unwrap().spawn.unwrap();
        assert_eq!(
            spawn.proposed_by.as_deref(),
            Some(format!("top manager on {PEER}").as_str())
        );
        assert_eq!(spawn.prompt.as_deref(), Some("first line\n\nthird line"));
        let local = propose(&h, &e, "bypass", "a");
        assert!(card_msg(&e, &local)
            .body_md
            .starts_with("Start claude-code in "));
        let local_spawn = card_msg(&e, &local).decision.unwrap().spawn.unwrap();
        assert_eq!((local_spawn.proposed_by, local_spawn.prompt), (None, None));

        let machines = serde_json::json!({"machines": [{
            "id": PEER, "label": "Workstation",
            "device_id": "11111111-2222-3333-4444-555555555555",
            "fp": "a".repeat(64)
        }]});
        let file = e.rt.cfg.dir.join(crate::mgr_rpc::machines::MACHINES_FILE);
        fs::write(&file, machines.to_string()).unwrap();
        fs::set_permissions(&file, std::os::unix::fs::PermissionsExt::from_mode(0o600)).unwrap();
        let labelled = propose_as(&remote, &e, "go");
        assert!(card_msg(&e, &labelled).body_md.starts_with(&format!(
            "Proposed by the top manager on Workstation ({PEER})."
        )));
        let spawn = card_msg(&e, &labelled).decision.unwrap().spawn.unwrap();
        assert_eq!(
            spawn.proposed_by.unwrap(),
            format!("top manager on Workstation ({PEER})")
        );
    }

    #[test]
    fn the_quoted_prompt_fence_is_longer_than_any_backtick_run_in_it() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let remote = As::remote(&h);
        for (prompt, fence) in [
            ("no ticks", "```"),
            ("a ` b `` c", "```"),
            ("a ``` b", "````"),
            ("a ````` b\n```", "``````"),
        ] {
            let card = propose_as(&remote, &e, prompt);
            let body = card_msg(&e, &card).body_md;
            assert!(
                body.ends_with(&format!("\n\n{fence}\n{prompt}\n{fence}")),
                "{body}"
            );
        }
    }

    #[test]
    fn the_approval_reply_depends_on_who_proposed() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let remote = As::remote(&h);
        let card = propose_as(&remote, &e, "go");
        let msg = approve_on_phone(&e, &card);
        assert_eq!(
            replies(&e, &msg.id),
            vec!["Approved on your phone. The top manager can start it now."]
        );
        assert!(manager_prompts(&e).is_empty());

        let orphan = As {
            inner: &h,
            proposer: None,
            remote: None,
        };
        let card = propose_as(&orphan, &e, "go");
        let msg = approve_on_phone(&e, &card);
        assert_eq!(
            replies(&e, &msg.id),
            vec!["Approved on your phone. The manager can start it now."]
        );
        assert!(manager_prompts(&e).is_empty());
        let args = card_args(&e, &card);
        assert_eq!(
            run_as(&As::remote(&h), &e, "spawn_session", args.clone()).unwrap_err(),
            GONE
        );
        assert!(h.spawned.lock().unwrap().is_empty());
        run(&h, &e, "spawn_session", args).unwrap();
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_card_can_only_be_used_by_the_proposer_that_asked_for_it() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let remote = As::remote(&h);
        let remote_card = propose_as(&remote, &e, "go");
        approve_on_phone(&e, &remote_card);
        let remote_by = format!("remote:{PEER}");
        assert!(e.rt.approved_spawn(&remote_card, Some(&remote_by)).is_ok());
        for other in [
            Some(MGR),
            Some("remote:mx_00000000000000cc"),
            Some("-"),
            None,
        ] {
            assert_eq!(
                e.rt.approved_spawn(&remote_card, other).unwrap_err(),
                CardMiss::Gone,
                "{other:?}"
            );
        }
        let inline = spawn_args(&e, "bypass", "a", &remote_card);
        assert!(run(&h, &e, "spawn_session", inline)
            .unwrap_err()
            .contains("proposed by someone else"));
        assert!(h.spawned.lock().unwrap().is_empty());
        assert!(matches!(
            e.auth.cards().state(&remote_card).unwrap(),
            Some(CardState::Issued { .. })
        ));

        let local_card = propose(&h, &e, "bypass", "a");
        approve_on_phone(&e, &local_card);
        let err = run_as(&remote, &e, "spawn_session", card_args(&e, &local_card)).unwrap_err();
        assert_eq!(err, GONE);
        assert!(h.spawned.lock().unwrap().is_empty());
        let wire: SignedSpawn = serde_json::from_value(approval(&e, &local_card)).unwrap();
        let proposal = SpawnProposal {
            v: 1,
            profile: "claude-code".into(),
            cwd: e.work.join("a").display().to_string(),
            mode: tmgr_spawn_auth::SpawnMode::Bypass,
            sandbox: None,
            prompt_sha256: Some(tmgr_spawn_auth::sha256_hex(b"go")),
            name: None,
            task: None,
            target_id: h.machine(),
            kind: tmgr_spawn_auth::ProposalKind::Spawn,
            session: None,
            from: None,
            text: None,
        };
        let Err(err) =
            e.rt.verify_spawn(&Approval::Signed(wire.clone()), &proposal, Some(&remote_by))
        else {
            panic!("a foreign proposer must be refused");
        };
        assert!(err.contains("proposed by someone else"));
    }

    #[test]
    fn a_remote_spawn_after_the_phone_approval_starts_one_orphan_session_tied_to_the_caller() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let remote = As::remote(&h);
        let card = propose_as(&remote, &e, "go");
        approve_on_phone(&e, &card);
        let mut changed = card_args(&e, &card);
        changed["cwd"] = serde_json::json!(e.work.join("b"));
        assert!(run_as(&remote, &e, "spawn_session", changed).is_err());
        assert!(h.spawned.lock().unwrap().is_empty());
        let first = run_as(&remote, &e, "spawn_session", card_args(&e, &card)).unwrap();
        let session = first["id"].as_str().unwrap();
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
        assert!(e.rt.spawned_for(session, PEER));
        assert!(!e.rt.spawned_for(session, "mx_00000000000000cc"));
        assert!(!e.rt.spawned_for(MGR, PEER));
        let again = run_as(&remote, &e, "spawn_session", card_args(&e, &card)).unwrap();
        assert_eq!(again["id"], first["id"]);
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
    }

    #[test]
    fn an_expired_card_is_refused_and_announced_once_and_a_late_answer_is_dropped() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let msg = card_msg(&e, &card);
        let signed = phone_answer(&e, &msg);
        let exp = msg
            .decision
            .as_ref()
            .unwrap()
            .spawn
            .as_ref()
            .unwrap()
            .exp_ms;
        e.rt.card_tick(exp - 1);
        assert!(replies(&e, &msg.id).is_empty());
        e.rt.card_tick(exp + 1);
        assert_eq!(replies(&e, &msg.id), vec!["Not approved: the card expired"]);
        e.rt.card_tick(exp + 2);
        e.rt.card_tick(exp + 20_000);
        assert_eq!(replies(&e, &msg.id).len(), 1);
        let got = card_state(&e, CLIENT_CLI, &card).unwrap();
        assert_eq!(got.state, "refused");
        assert_eq!(got.reason.as_deref(), Some("expired"));
        e.rt.accept_spawnok(&signed, exp + 3);
        assert!(e.rt.approved_spawn(&card, Some(MGR)).is_err());
        assert_eq!(replies(&e, &msg.id).len(), 1);
        let err = run(&h, &e, "spawn_session", card_args(&e, &card)).unwrap_err();
        assert_eq!(err, GONE);
        assert!(h.spawned.lock().unwrap().is_empty());
    }

    #[test]
    fn an_approved_card_that_expires_before_the_spawn_loses_its_approval() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let msg = card_msg(&e, &card);
        e.rt.accept_spawnok(&phone_answer(&e, &msg), now_ms());
        assert!(e.rt.approved_spawn(&card, Some(MGR)).is_ok());
        let exp = msg
            .decision
            .as_ref()
            .unwrap()
            .spawn
            .as_ref()
            .unwrap()
            .exp_ms;
        e.rt.card_tick(exp + 1);
        assert!(e.rt.approved_spawn(&card, Some(MGR)).is_err());
        assert!(run(&h, &e, "spawn_session", card_args(&e, &card)).is_err());
        assert!(h.spawned.lock().unwrap().is_empty());
    }

    #[test]
    fn a_card_says_pending_only_while_it_is_live_and_the_callers() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let pending = "this card is not approved yet";
        assert_eq!(
            run(&h, &e, "spawn_session", card_args(&e, &card)).unwrap_err(),
            pending
        );
        assert_eq!(
            run_as(&As::remote(&h), &e, "spawn_session", card_args(&e, &card)).unwrap_err(),
            GONE
        );
        lock(&e.rt.spawn_gate.cards).get_mut(&card).unwrap().exp_ms = 1;
        assert_eq!(
            run(&h, &e, "spawn_session", card_args(&e, &card)).unwrap_err(),
            GONE
        );
        assert_eq!(
            run(&h, &e, "spawn_session", card_args(&e, "card_unknown")).unwrap_err(),
            GONE
        );
        assert!(h.spawned.lock().unwrap().is_empty());
    }

    #[test]
    fn a_local_card_is_usable_by_any_local_manager_and_a_remote_card_is_not() {
        let e = env();
        with_dirs(&e);
        let a = TestHost::new(&e);
        let other_id = "s_bbbbbbbbbb";
        let b = TestHost::with_me(&e, other_id);
        let card = propose(&a, &e, "bypass", "a");
        let msg = approve_on_phone(&e, &card);
        assert_eq!(
            replies(&e, &msg.id),
            vec!["Approved on your phone. Starting the session."]
        );
        run(&b, &e, "spawn_session", card_args(&e, &card)).unwrap();
        assert_eq!(b.spawned.lock().unwrap().len(), 1);

        let remote = As::remote(&a);
        let remote_card = propose_as(&remote, &e, "go");
        approve_on_phone(&e, &remote_card);
        for local in [&a, &b] {
            let err = run(local, &e, "spawn_session", card_args(&e, &remote_card)).unwrap_err();
            assert_eq!(err, GONE);
        }
        assert!(a.spawned.lock().unwrap().is_empty());
        run_as(&remote, &e, "spawn_session", card_args(&e, &remote_card)).unwrap();
    }

    #[test]
    fn a_card_that_lost_its_runner_state_is_still_usable_locally_and_never_remotely() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let remote = As::remote(&h);
        let local_card = propose(&h, &e, "bypass", "a");
        lock(&e.rt.spawn_gate.cards).remove(&local_card);
        approve_on_phone(&e, &local_card);
        assert_eq!(
            room_msgs(&e).last().unwrap().body_md,
            "Approved on your phone. The manager can start it now."
        );
        assert!(manager_prompts(&e).is_empty());
        run(&h, &e, "spawn_session", card_args(&e, &local_card)).unwrap();

        let remote_card = propose_as(&remote, &e, "go");
        lock(&e.rt.spawn_gate.cards).remove(&remote_card);
        approve_on_phone(&e, &remote_card);
        let err = run_as(&remote, &e, "spawn_session", card_args(&e, &remote_card)).unwrap_err();
        assert_eq!(err, GONE);
    }

    #[test]
    fn a_spawn_that_fails_after_the_approval_tells_the_card_to_ask_again() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let msg = approve_on_phone(&e, &card);
        h.fail(Fail::BeforeCreate);
        assert_eq!(
            run(&h, &e, "spawn_session", card_args(&e, &card)).unwrap_err(),
            "agent did not start"
        );
        assert_eq!(
            replies(&e, &msg.id),
            vec![
                "Approved on your phone. Starting the session.",
                "The session did not start: agent did not start. Ask for a new card."
            ]
        );
    }

    #[test]
    fn who_spawned_a_session_is_stored_with_it_and_survives_the_card() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let remote = As::remote(&h);
        let card = propose_as(&remote, &e, "go");
        approve_on_phone(&e, &card);
        let first = run_as(&remote, &e, "spawn_session", card_args(&e, &card)).unwrap();
        let session = first["id"].as_str().unwrap();
        let by = lock(&e.rt.session(session).unwrap().inner)
            .meta
            .spawned_by
            .clone();
        assert_eq!(by, Some(format!("remote:{PEER}")));
        lock(&e.rt.spawn_gate.cards).clear();
        assert!(e.rt.spawned_for(session, PEER));
        assert!(!e.rt.spawned_for(session, "mx_00000000000000cc"));

        let local_card = propose(&h, &e, "bypass", "a");
        approve_on_phone(&e, &local_card);
        let local = run(&h, &e, "spawn_session", card_args(&e, &local_card)).unwrap();
        let id = local["id"].as_str().unwrap();
        assert!(lock(&e.rt.session(id).unwrap().inner)
            .meta
            .spawned_by
            .is_none());
        assert!(!e.rt.spawned_for(id, PEER));
    }

    #[test]
    fn answers_with_a_bad_signature_another_target_or_an_unknown_card_are_dropped() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let msg = card_msg(&e, &card);
        let good = phone_answer(&e, &msg);

        let mut bad_sig = good.clone();
        bad_sig.sig = sign(&secret(4), "tmgr-spawn:v1:other");
        let mut other_target = good.clone();
        other_target.target_fp = "c".repeat(64);
        let mut other_req = good.clone();
        other_req.req_sha256 = "d".repeat(64);
        let mut other_key = good.clone();
        other_key.device_fp = "e".repeat(64);
        let mut unknown = good.clone();
        unknown.card_id = "card_unknown".into();
        for wire in [bad_sig, other_target, other_req, other_key, unknown] {
            e.rt.accept_spawnok(&wire, now_ms());
            assert!(e.rt.approved_spawn(&wire.card_id, Some(MGR)).is_err());
        }
        assert!(replies(&e, &msg.id).is_empty());
        assert!(manager_prompts(&e).is_empty());
        let err = run(&h, &e, "spawn_session", card_args(&e, &card)).unwrap_err();
        assert_eq!(err, "this card is not approved yet");
        assert!(matches!(
            e.auth.cards().state(&card).unwrap(),
            Some(CardState::Issued {
                failed_attempts: 0,
                ..
            })
        ));
        e.rt.accept_spawnok(&good, now_ms());
        assert!(e.rt.approved_spawn(&card, Some(MGR)).is_ok());
    }

    #[test]
    fn an_answer_for_a_human_challenge_card_is_not_stored() {
        let e = env();
        challenge(&e, CLIENT_CLI, "card_human").unwrap();
        let Some(CardState::Issued {
            nonce,
            exp_ms,
            req_sha256,
            target_id,
            target_fp,
            ..
        }) = e.auth.cards().state("card_human").unwrap()
        else {
            panic!("card is not issued");
        };
        let ts = now_ms();
        let msg = format!(
            "tmgr-spawn:v1:{USER}:{}:{target_id}:{target_fp}:card_human:{nonce}:{ts}:{exp_ms}:{req_sha256}",
            e.fp
        );
        let wire = SignedSpawn {
            v: 1,
            user_id: USER.into(),
            device_fp: e.fp.clone(),
            target_id,
            target_fp,
            card_id: "card_human".into(),
            nonce,
            ts_ms: ts,
            exp_ms,
            req_sha256,
            alg: "es256".into(),
            sig: sign(&secret(3), &msg),
        };
        e.rt.accept_spawnok(&wire, now_ms());
        assert!(e.rt.approved_spawn("card_human", Some(MGR)).is_err());
    }

    #[test]
    fn spawn_card_is_only_for_the_cli_and_desktop() {
        let e = env();
        challenge(&e, CLIENT_CLI, "card_s").unwrap();
        for client in [CLIENT_WORKER, "other", ""] {
            let err = card_state(&e, client, "card_s").unwrap_err();
            assert_eq!(err.code, ErrorCode::Forbidden, "{client}");
        }
        for client in [CLIENT_CLI, CLIENT_DESKTOP] {
            assert_eq!(card_state(&e, client, "card_s").unwrap().state, "issued");
        }
        let err = card_state(&e, CLIENT_CLI, "card_nope").unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);
    }

    #[test]
    fn at_most_four_manager_cards_wait_while_a_human_challenge_still_issues() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        for _ in 0..crate::spawnauth::MAX_MANAGER_PENDING {
            propose(&h, &e, "bypass", "a");
        }
        let args = serde_json::json!({
            "profile": "claude-code", "cwd": e.work.join("a"), "permission_mode": "bypass"
        });
        let err = run(&h, &e, "propose_spawn", args).unwrap_err();
        assert_eq!(err, "too many spawn cards are waiting for approval");
        challenge(&e, CLIENT_CLI, "card_human").unwrap();
    }

    #[test]
    fn an_unlinked_runner_dir_is_not_given_a_relay_directory_by_a_trust_lookup() {
        let t = tempfile::tempdir().unwrap();
        assert!(relay_state(t.path(), "mx_0123456789abcdef").is_none());
        assert!(!t.path().join("relay").exists());
    }

    fn conn(client: &str) -> (Arc<Conn>, UnixStream) {
        let (a, b) = UnixStream::pair().unwrap();
        let c = Arc::new(Conn {
            id: 1,
            w: Mutex::new(a),
            watched: Mutex::new(Vec::new()),
            scroll: Mutex::new(HashMap::new()),
            sizes: Mutex::new(HashMap::new()),
            closed: AtomicBool::new(false),
            attached: AtomicBool::new(false),
            client: client.into(),
            peer_pid: None,
            pending_rx: Mutex::new(None),
            chat_rx: Mutex::new(None),
            chat_subscribed: AtomicBool::new(false),
        });
        (c, b)
    }

    fn challenge_params(e: &Env, card: &str) -> Value {
        serde_json::json!({
            "card_id": card,
            "proposal": {
                "v": 1, "profile": "claude-code", "cwd": e.work.display().to_string(),
                "mode": "bypass", "target_id": e.rt.chat.machine()
            }
        })
    }

    fn challenge(e: &Env, client: &str, card: &str) -> Result<Value, ApiErr> {
        let (c, _peer) = conn(client);
        e.rt.dispatch(
            &c,
            &Request {
                id: 1,
                method: method::SPAWN_CHALLENGE.into(),
                params: challenge_params(e, card),
            },
        )
    }

    #[test]
    fn only_cli_and_desktop_may_ask_for_a_challenge() {
        let e = env();
        for client in [CLIENT_WORKER, "other", ""] {
            let err = challenge(&e, client, "card_w").unwrap_err();
            assert_eq!(err.code, ErrorCode::Forbidden, "{client}");
        }
        assert!(e.auth.cards().state("card_w").unwrap().is_none());
        for client in [CLIENT_CLI, CLIENT_DESKTOP] {
            let v = challenge(&e, client, "card_ok").unwrap();
            let r: SpawnChallengeResult = serde_json::from_value(v).unwrap();
            assert_eq!(r.target_id, e.rt.chat.machine());
            let bytes = tmgr_spawn_auth::b64_decode_strict(&r.req).unwrap();
            assert_eq!(tmgr_spawn_auth::sha256_hex(&bytes), r.req_sha256);
        }
    }

    #[test]
    fn a_client_cannot_set_the_runner_internal_spawn_card_tag() {
        let e = env();
        let (c, _peer) = conn(CLIENT_CLI);
        let err =
            e.rt.dispatch(
                &c,
                &Request {
                    id: 1,
                    method: method::RUNS_CREATE.into(),
                    params: serde_json::json!({
                        "profile": "claude-code", "cwd": "/tmp", "permission_mode": "bypass",
                        "spawn_card": "card_forged"
                    }),
                },
            )
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidParams);
        assert!(err.msg.contains("spawn_card"));
    }

    #[test]
    fn the_eleventh_challenge_in_a_minute_is_refused() {
        let e = env();
        for _ in 0..10 {
            challenge(&e, CLIENT_CLI, "card_r").unwrap();
        }
        let err = challenge(&e, CLIENT_CLI, "card_r").unwrap_err();
        assert_eq!(err.code, ErrorCode::Limit);
    }

    #[test]
    fn without_a_linked_account_challenges_and_verification_are_refused() {
        let t = tempfile::Builder::new()
            .prefix("sa")
            .tempdir_in("/tmp")
            .unwrap();
        let rt = runner(t.path());
        let proposal: SpawnProposal = serde_json::from_value(serde_json::json!({
            "v": 1, "profile": "claude-code", "cwd": "/w", "mode": "bypass",
            "target_id": rt.chat.machine()
        }))
        .unwrap();
        assert_eq!(
            rt.spawn_challenge_within("card_n", &proposal, Budget::Human)
                .unwrap_err()
                .code,
            ErrorCode::Forbidden
        );
        let wire: SignedSpawn = serde_json::from_value(serde_json::json!({
            "v":1,"user_id":"7","device_fp":"d","target_id":"mx_1","target_fp":"f","card_id":"card_n",
            "nonce":"n","ts_ms":1,"exp_ms":2,"req_sha256":"r","alg":"es256","sig":"s"
        }))
        .unwrap();
        assert!(rt
            .verify_spawn(&Approval::Signed(wire.clone()), &proposal, None)
            .is_err());
    }

    #[test]
    fn relay_trust_maps_device_states_and_never_trusts_unknown_or_self() {
        use tmgr_chat_relay::engine::TrustDevice;
        let dev = |id: &str, fp: &str, state, is_self| TrustDevice {
            id: id.into(),
            kind: "phone".into(),
            label: "x".into(),
            fp: fp.into(),
            state,
            is_self,
            kind_verified: true,
            can_confirm_kind: false,
        };
        let st = TrustState {
            registered: true,
            self_fp: Some("c".repeat(64)),
            devices: vec![
                dev("t", "f1", DeviceState::Trusted, false),
                dev("r", "f2", DeviceState::Revoked, false),
                dev("p", "f3", DeviceState::Pending, false),
                dev("me", "f4", DeviceState::Trusted, true),
            ],
            ..TrustState::default()
        };
        assert_eq!(status_of(&st, "t", "f1"), TrustStatus::Trusted);
        assert_eq!(status_of(&st, "t", "other"), TrustStatus::NotTrusted);
        assert_eq!(status_of(&st, "r", "f2"), TrustStatus::Revoked);
        assert_eq!(status_of(&st, "p", "f3"), TrustStatus::NotTrusted);
        assert_eq!(status_of(&st, "me", "f4"), TrustStatus::NotTrusted);
        assert_eq!(status_of(&st, "nobody", "f1"), TrustStatus::Unknown);
        assert_eq!(
            status_of(&TrustState::default(), "t", "f1"),
            TrustStatus::Unknown
        );
        let id = identity_of(42, "mx_0123456789abcdef", &st).unwrap();
        assert_eq!(id.user_id(), "42");
        let no_fp = TrustState {
            registered: true,
            ..TrustState::default()
        };
        assert!(identity_of(42, "mx_0123456789abcdef", &no_fp).is_none());
    }

    fn statement_json(o: &KeyOffer) -> String {
        serde_json::to_string(&SpawnKey {
            statement: o.statement.clone(),
            sig: o.sig.clone(),
            spawn_sig: o.spawn_sig.clone(),
        })
        .unwrap()
    }

    fn fresh_auth() -> (tempfile::TempDir, SpawnAuth) {
        let t = tempfile::tempdir().unwrap();
        let identity = Identity::new(USER, "mx_0123456789abcdef", &"b".repeat(64)).unwrap();
        let source = Arc::new(Source(Mutex::new(TrustStatus::Trusted)));
        let auth = SpawnAuth::open(t.path(), identity, source).unwrap();
        (t, auth)
    }

    #[test]
    fn a_spawnkey_statement_lands_as_pending_and_only_a_newer_one_replaces_it() {
        let (_t, auth) = fresh_auth();
        let now = now_ms();
        let ingest = |o: &KeyOffer| {
            ingest_spawnkey(
                &auth,
                PHONE,
                &o.sign_pub_b64,
                &o.kex_pub_b64,
                &serde_json::from_str(&statement_json(o)).unwrap(),
                now,
            )
        };
        let first = offer_with(&kex_key(None), PHONE, &secret(3), now - 2_000);
        assert_eq!(ingest(&first).unwrap(), OfferOutcome::Added);
        assert_eq!(auth.pending_keys().unwrap().len(), 1);
        assert!(auth.pinned_key(&fp_of(&first)).unwrap().is_none());

        let older = offer_with(&kex_key(None), PHONE, &secret(4), now - 3_000);
        assert_eq!(ingest(&older).unwrap(), OfferOutcome::Stale);
        assert_eq!(auth.pending_keys().unwrap()[0].spawn_key_fp, spawn_fp(3));

        let newer = offer_with(&kex_key(None), PHONE, &secret(6), now - 1_000);
        assert_eq!(ingest(&newer).unwrap(), OfferOutcome::Replaced);
        let pending = auth.pending_keys().unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].spawn_key_fp, spawn_fp(6));
        assert!(auth.pinned_key(&fp_of(&first)).unwrap().is_none());
    }

    #[test]
    fn a_tampered_spawnkey_statement_is_rejected() {
        let (_t, auth) = fresh_auth();
        let o = offer_with(&kex_key(None), PHONE, &secret(3), now_ms() - 1_000);
        let mut key: SpawnKey = serde_json::from_str(&statement_json(&o)).unwrap();
        key.spawn_sig = key.sig.clone();
        let out = ingest_spawnkey(
            &auth,
            PHONE,
            &o.sign_pub_b64,
            &o.kex_pub_b64,
            &key,
            now_ms(),
        );
        assert!(out.is_err());
        assert!(auth.pending_keys().unwrap().is_empty());
    }

    fn trust(e: &Env, dir: &str) {
        fs::create_dir_all(&e.rt.cfg.homes).unwrap();
        folders::add(&e.rt.cfg.homes, &e.work.join(dir), None, 1).unwrap();
    }

    fn card_hash(e: &Env, card: &str) -> String {
        let msg = card_msg(e, card);
        let request = &msg
            .decision
            .as_ref()
            .unwrap()
            .spawn
            .as_ref()
            .unwrap()
            .request;
        tmgr_spawn_auth::sha256_hex(request.as_bytes())
    }

    fn decide_raw(e: &Env, client: &str, params: Value) -> Result<Value, ApiErr> {
        let (c, _peer) = conn(client);
        e.rt.dispatch(
            &c,
            &Request {
                id: 1,
                method: method::SPAWN_DECIDE.into(),
                params,
            },
        )
    }

    fn decide(e: &Env, card: &str, decision: &str) -> Result<Value, ApiErr> {
        decide_raw(
            e,
            CLIENT_DESKTOP,
            serde_json::json!({
                "card_id": card, "req_sha256": card_hash(e, card), "decision": decision
            }),
        )
    }

    fn edit(e: &Env, card: &str, edit: Value) -> Result<Value, ApiErr> {
        decide_raw(
            e,
            CLIENT_DESKTOP,
            serde_json::json!({
                "card_id": card, "req_sha256": card_hash(e, card), "decision": "edit", "edit": edit
            }),
        )
    }

    fn is_issued(e: &Env, card: &str) -> bool {
        matches!(
            e.auth.cards().state(card).unwrap(),
            Some(CardState::Issued { .. })
        )
    }

    #[test]
    fn only_the_desktop_client_may_decide_a_card() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let params = serde_json::json!({
            "card_id": card, "req_sha256": card_hash(&e, &card), "decision": "approve"
        });
        for client in [CLIENT_CLI, CLIENT_WORKER, "manager", "other", ""] {
            let err = decide_raw(&e, client, params.clone()).unwrap_err();
            assert_eq!(err.code, ErrorCode::Forbidden, "{client}");
        }
        assert!(e.rt.approved_spawn(&card, Some(MGR)).is_err());
        assert!(is_issued(&e, &card));
        decide_raw(&e, CLIENT_DESKTOP, params).unwrap();
        assert!(e.rt.approved_spawn(&card, Some(MGR)).is_ok());
    }

    #[test]
    fn a_payload_hash_mismatch_is_refused_and_the_card_stays_open() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        for decision in ["approve", "reject"] {
            let err = decide_raw(
                &e,
                CLIENT_DESKTOP,
                serde_json::json!({
                    "card_id": card, "req_sha256": "0".repeat(64), "decision": decision
                }),
            )
            .unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidParams);
        }
        assert!(is_issued(&e, &card));
        assert!(e.rt.approved_spawn(&card, Some(MGR)).is_err());
        assert!(manager_prompts(&e).is_empty());
    }

    #[test]
    fn decisions_on_unknown_closed_and_human_cards_are_refused() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let err = decide_raw(
            &e,
            CLIENT_DESKTOP,
            serde_json::json!({"card_id": "card_nope", "req_sha256": "ab", "decision": "approve"}),
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFound);
        challenge(&e, CLIENT_CLI, "card_human").unwrap();
        let err = decide_raw(
            &e,
            CLIENT_DESKTOP,
            serde_json::json!({"card_id": "card_human", "req_sha256": "ab", "decision": "approve"}),
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidParams);
        let card = propose(&h, &e, "bypass", "a");
        decide(&e, &card, "reject").unwrap();
        assert!(decide(&e, &card, "approve").is_err());
    }

    #[test]
    fn a_desktop_approval_starts_exactly_one_session_and_a_replay_starts_none() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let msg = card_msg(&e, &card);
        let out = decide(&e, &card, "approve").unwrap();
        assert_eq!(out["state"], "approved");
        assert_eq!(replies(&e, &msg.id), vec!["Approved on the desktop."]);
        let notes = manager_prompts(&e);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].0, MGR);
        assert!(notes[0].1.contains(&card) && notes[0].1.contains("spawn_session"));
        assert!(notes[0].1.contains(&e.work.join("a").display().to_string()));
        assert!(notes[0].1.contains("\"permission_mode\":\"bypass\""));
        assert_eq!(
            card_state(&e, CLIENT_DESKTOP, &card).unwrap().state,
            "approved"
        );
        assert!(decide(&e, &card, "approve").is_err());
        assert!(decide(&e, &card, "reject").is_err());

        let first = run(&h, &e, "spawn_session", card_args(&e, &card)).unwrap();
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
        assert_eq!(
            h.spawned.lock().unwrap()[0].1.as_deref(),
            Some(card.as_str())
        );
        let done = card_state(&e, CLIENT_DESKTOP, &card).unwrap();
        assert_eq!(done.state, "spawned");
        assert_eq!(done.session, first["id"].as_str().map(String::from));
        let again = run(&h, &e, "spawn_session", card_args(&e, &card)).unwrap();
        assert_eq!(again["id"], first["id"]);
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
        assert!(decide(&e, &card, "approve").is_err());
    }

    #[test]
    fn a_desktop_approval_binds_the_exact_arguments() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        decide(&e, &card, "approve").unwrap();
        for (key, other) in [
            ("permission_mode", serde_json::json!("accept-edits")),
            ("cwd", serde_json::json!(e.work.join("b"))),
            ("prompt", serde_json::json!("different")),
        ] {
            let mut changed = card_args(&e, &card);
            changed[key] = other;
            let err = run(&h, &e, "spawn_session", changed).unwrap_err();
            assert_eq!(err, Refusal::Mismatch.to_string(), "{key}");
        }
        assert!(h.spawned.lock().unwrap().is_empty());
        assert!(is_issued(&e, &card));
        run(&h, &e, "spawn_session", card_args(&e, &card)).unwrap();
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
    }

    #[test]
    fn a_card_approved_on_the_desktop_cannot_be_used_by_a_local_approval_of_another_card() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let first = propose(&h, &e, "bypass", "a");
        let second = propose(&h, &e, "bypass", "a");
        decide(&e, &first, "approve").unwrap();
        let err = run(&h, &e, "spawn_session", card_args(&e, &second)).unwrap_err();
        assert_eq!(err, "this card is not approved yet");
        assert!(h.spawned.lock().unwrap().is_empty());
    }

    #[test]
    fn a_rejected_card_refuses_the_spawn_and_tells_the_manager() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let msg = card_msg(&e, &card);
        assert_eq!(decide(&e, &card, "reject").unwrap()["state"], "rejected");
        assert_eq!(replies(&e, &msg.id), vec!["Rejected on the desktop."]);
        let notes = manager_prompts(&e);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].0, MGR);
        assert!(notes[0].1.contains(&card) && notes[0].1.contains("do not retry"));
        let err = run(&h, &e, "spawn_session", card_args(&e, &card)).unwrap_err();
        assert_eq!(err, GONE);
        assert!(h.spawned.lock().unwrap().is_empty());
        let got = card_state(&e, CLIENT_DESKTOP, &card).unwrap();
        assert_eq!(
            (got.state.as_str(), got.reason.as_deref()),
            ("refused", Some("rejected"))
        );
    }

    #[test]
    fn an_edit_replaces_the_card_and_only_the_new_arguments_work_after_approval() {
        let e = env();
        with_dirs(&e);
        trust(&e, "b");
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let msg = card_msg(&e, &card);
        let out = edit(
            &e,
            &card,
            serde_json::json!({
                "cwd": e.work.join("b"), "mode": "accept-edits", "name": "n1", "task": "t1"
            }),
        )
        .unwrap();
        assert_eq!(out["state"], "edited");
        let new = out["card_id"].as_str().unwrap().to_owned();
        assert_ne!(new, card);
        assert!(out["exp_ms"].as_u64().is_some());
        assert_eq!(
            replies(&e, &msg.id),
            vec!["Edited on the desktop; replaced by a new card."]
        );
        let got = card_state(&e, CLIENT_DESKTOP, &card).unwrap();
        assert_eq!(
            (got.state.as_str(), got.reason.as_deref()),
            ("refused", Some("edited"))
        );
        assert_eq!(
            card_state(&e, CLIENT_DESKTOP, &new).unwrap().state,
            "issued"
        );
        let Some(CardState::Issued { proposal, .. }) = e.auth.cards().state(&new).unwrap() else {
            panic!("new card is not issued")
        };
        let old_proposal = card_msg(&e, &new).decision.unwrap().spawn.unwrap();
        assert!(old_proposal.summary.contains("accept-edits"));
        assert_eq!(proposal.cwd, e.work.join("b").display().to_string());
        assert_eq!(proposal.name.as_deref(), Some("n1"));
        assert_eq!(proposal.task.as_deref(), Some("t1"));
        assert_eq!(proposal.profile, "claude-code");
        assert_eq!(
            proposal.prompt_sha256,
            Some(tmgr_spawn_auth::sha256_hex(b"go"))
        );

        let notes = manager_prompts(&e);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].1.contains(&new) && notes[0].1.contains("not approved yet"));

        let old_err = run(&h, &e, "spawn_session", card_args(&e, &card)).unwrap_err();
        assert_eq!(old_err, GONE);
        let new_args = serde_json::json!({
            "profile": "claude-code", "cwd": e.work.join("b"), "permission_mode": "accept-edits",
            "prompt": "go", "name": "n1", "task": "t1", "card_id": new
        });
        let err = run(&h, &e, "spawn_session", new_args.clone()).unwrap_err();
        assert_eq!(err, "this card is not approved yet");
        assert!(decide(&e, &card, "approve").is_err());
        decide(&e, &new, "approve").unwrap();
        let mut stale = new_args.clone();
        stale["cwd"] = serde_json::json!(e.work.join("a"));
        assert!(run(&h, &e, "spawn_session", stale).is_err());
        assert!(h.spawned.lock().unwrap().is_empty());
        run(&h, &e, "spawn_session", new_args).unwrap();
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
        assert_eq!(
            h.spawned.lock().unwrap()[0].1.as_deref(),
            Some(new.as_str())
        );
    }

    #[test]
    fn a_bad_edit_is_refused_and_keeps_the_old_card_untouched() {
        let e = env();
        with_dirs(&e);
        trust(&e, "b");
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "ask", "a");
        let outside = e.work.join("c");
        fs::create_dir_all(&outside).unwrap();
        let bad = [
            serde_json::json!({"cwd": outside, "mode": "ask"}),
            serde_json::json!({"cwd": "relative", "mode": "ask"}),
            serde_json::json!({"cwd": e.work.join("b"), "mode": "bypass"}),
            serde_json::json!({"cwd": e.work.join("b"), "mode": "no_sandbox"}),
            serde_json::json!({"cwd": e.work.join("b"), "mode": "ask", "name": "x".repeat(65)}),
        ];
        for edit_args in bad {
            let err = edit(&e, &card, edit_args.clone()).unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidParams, "{edit_args}");
            assert!(is_issued(&e, &card));
        }
        let err = decide_raw(
            &e,
            CLIENT_DESKTOP,
            serde_json::json!({
                "card_id": card, "req_sha256": card_hash(&e, &card), "decision": "edit"
            }),
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidParams);
        assert!(is_issued(&e, &card));
        assert!(manager_prompts(&e).is_empty());
        assert_eq!(
            room_msgs(&e)
                .iter()
                .filter(|m| m.decision.is_some())
                .count(),
            1
        );
    }

    #[test]
    fn a_remote_proposers_card_is_never_edited() {
        let e = env();
        with_dirs(&e);
        trust(&e, "b");
        let h = TestHost::new(&e);
        let rh = As::remote(&h);
        let card = propose_as(&rh, &e, "go");
        let err = edit(
            &e,
            &card,
            serde_json::json!({"cwd": e.work.join("b"), "mode": "ask"}),
        )
        .unwrap_err();
        assert_eq!(err.code, ErrorCode::InvalidParams);
        assert!(is_issued(&e, &card));
        assert_eq!(decide(&e, &card, "approve").unwrap()["state"], "approved");
    }

    #[test]
    fn the_phone_answer_after_a_desktop_decision_is_dropped() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let approved = propose(&h, &e, "bypass", "a");
        let rejected = propose(&h, &e, "bypass", "a");
        let edited = propose(&h, &e, "bypass", "a");
        trust(&e, "b");
        let phone: Vec<SignedSpawn> = [&approved, &rejected, &edited]
            .iter()
            .map(|c| phone_answer(&e, &card_msg(&e, c)))
            .collect();
        decide(&e, &approved, "approve").unwrap();
        decide(&e, &rejected, "reject").unwrap();
        edit(
            &e,
            &edited,
            serde_json::json!({"cwd": e.work.join("b"), "mode": "ask"}),
        )
        .unwrap();
        let before = manager_prompts(&e).len();
        assert_eq!(before, 3);
        for signed in &phone {
            e.rt.accept_spawnok(signed, now_ms());
        }
        assert!(manager_prompts(&e).is_empty());
        assert!(matches!(
            e.rt.approved_spawn(&approved, Some(MGR)),
            Ok(Approval::Local { .. })
        ));
        assert!(e.rt.approved_spawn(&rejected, Some(MGR)).is_err());
        assert!(e.rt.approved_spawn(&edited, Some(MGR)).is_err());
        let msg = card_msg(&e, &approved);
        assert_eq!(replies(&e, &msg.id), vec!["Approved on the desktop."]);
    }

    #[test]
    fn a_desktop_decision_after_the_phone_answer_is_refused() {
        let e = env();
        with_dirs(&e);
        trust(&e, "b");
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        e.rt.accept_spawnok(&phone_answer(&e, &card_msg(&e, &card)), now_ms());
        assert!(matches!(
            e.rt.approved_spawn(&card, Some(MGR)),
            Ok(Approval::Signed(_))
        ));
        assert_eq!(manager_prompts(&e).len(), 1);
        for call in [
            decide(&e, &card, "approve"),
            decide(&e, &card, "reject"),
            edit(
                &e,
                &card,
                serde_json::json!({"cwd": e.work.join("b"), "mode": "ask"}),
            ),
        ] {
            assert_eq!(call.unwrap_err().code, ErrorCode::InvalidParams);
        }
        assert!(is_issued(&e, &card));
        assert!(manager_prompts(&e).is_empty());
        run(&h, &e, "spawn_session", card_args(&e, &card)).unwrap();
        assert_eq!(h.spawned.lock().unwrap().len(), 1);
    }

    #[test]
    fn spawn_card_tells_a_failed_spawn_from_one_in_flight_and_one_from_before_the_restart() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);

        let failed = propose(&h, &e, "bypass", "a");
        decide(&e, &failed, "approve").unwrap();
        h.fail(Fail::BeforeCreate);
        assert!(run(&h, &e, "spawn_session", card_args(&e, &failed)).is_err());
        h.fail(Fail::No);
        let got = card_state(&e, CLIENT_DESKTOP, &failed).unwrap();
        assert_eq!(
            (got.state.as_str(), got.reason.as_deref()),
            ("refused", Some("spawn_failed"))
        );

        let in_flight = propose(&h, &e, "bypass", "a");
        let sha = card_hash(&e, &in_flight);
        decide(&e, &in_flight, "approve").unwrap();
        e.auth
            .cards()
            .claim_local(&in_flight, &sha, now_ms())
            .unwrap();
        let got = card_state(&e, CLIENT_DESKTOP, &in_flight).unwrap();
        assert_eq!(
            (got.state.as_str(), got.reason.as_deref()),
            ("claimed", None)
        );

        let old = propose(&h, &e, "bypass", "a");
        let sha = card_hash(&e, &old);
        e.auth.cards().claim_local(&old, &sha, 1).unwrap();
        let got = card_state(&e, CLIENT_DESKTOP, &old).unwrap();
        assert_eq!(
            (got.state.as_str(), got.reason.as_deref()),
            ("refused", Some("interrupted"))
        );
    }

    #[test]
    fn the_manager_has_no_tool_that_decides_a_card_and_an_old_runner_says_unknown_method() {
        let e = env();
        with_dirs(&e);
        let h = TestHost::new(&e);
        let card = propose(&h, &e, "bypass", "a");
        let args = serde_json::json!({
            "card_id": card, "req_sha256": card_hash(&e, &card), "decision": "approve"
        });
        for t in [
            "spawn.decide",
            "spawn_decide",
            "decide_spawn",
            "approve_spawn",
        ] {
            assert!(run(&h, &e, t, args.clone())
                .unwrap_err()
                .contains("unknown tool"));
        }
        assert!(e.rt.approved_spawn(&card, Some(MGR)).is_err());
        let (c, _peer) = conn(CLIENT_DESKTOP);
        let err =
            e.rt.dispatch(
                &c,
                &Request {
                    id: 1,
                    method: "spawn.nope".into(),
                    params: Value::Null,
                },
            )
            .unwrap_err();
        assert_eq!(err.code, ErrorCode::UnknownMethod);
        assert_eq!(err.msg, "unknown method spawn.nope");
    }

    const TARGET: &str = "s_bbbbbbbbbb";

    fn add_live(e: &Env, id: &str, extra: Value) -> (Arc<Session>, mpsc::Receiver<Frame>) {
        let mut meta = serde_json::json!({
            "id": id, "name": id, "argv": ["claude"], "cwd": e.work,
            "profile": "claude-code", "created_ms": 0, "cols": 100, "rows": 24,
            "shim_pid": 1, "child_pid": 2,
        });
        meta.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let meta: Meta = serde_json::from_value(meta).unwrap();
        let (set, _) = ProfileSet::load(None);
        let matcher = Matcher::new(set.get("claude-code").unwrap()).ok();
        let sdir = e.work.join("sessions").join(id);
        fs::create_dir_all(&sdir).unwrap();
        let s = e.rt.new_session(meta, matcher, &sdir);
        let (tx, rx) = mpsc::sync_channel::<Frame>(SHIM_QUEUE);
        *lock(&s.shim) = Some(ShimLink {
            tx,
            queued_bytes: Arc::new(AtomicUsize::new(0)),
            queued_frames: Arc::new(AtomicUsize::new(0)),
            credit: None,
        });
        s.feed("done\r\n\r\n\u{276f}\r\n".as_bytes(), true);
        lock(&e.rt.sessions).insert(id.to_string(), s.clone());
        (s, rx)
    }

    fn typed(rx: &mpsc::Receiver<Frame>) -> String {
        thread::sleep(SUBMIT_DELAY + Duration::from_millis(150));
        let mut out = Vec::new();
        while let Ok(frame) = rx.try_recv() {
            if let Frame::Bin(b) = frame {
                out.extend_from_slice(&b.data);
            }
        }
        String::from_utf8(out).unwrap()
    }

    fn send_card(h: &TestHost, e: &Env, text: &str) -> String {
        run(
            h,
            e,
            "send_to_session",
            serde_json::json!({"session": TARGET, "text": text}),
        )
        .unwrap()["card_id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn stop_card(h: &TestHost, e: &Env) -> String {
        run(h, e, "stop_session", serde_json::json!({"session": TARGET})).unwrap()["card_id"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    fn card_kind_state(e: &Env, card: &str) -> &'static str {
        match e.auth.cards().state(card).unwrap() {
            Some(CardState::Issued { .. }) => "issued",
            Some(CardState::Claimed { .. }) => "claimed",
            Some(CardState::Spawned { .. }) => "spawned",
            Some(CardState::Refused { .. }) => "refused",
            None => "none",
        }
    }

    #[test]
    fn a_send_card_delivers_nothing_before_approval_and_the_summary_names_the_target() {
        let e = env();
        let (_s, rx) = add_live(&e, TARGET, serde_json::json!({}));
        let h = TestHost::new(&e);
        let card = send_card(&h, &e, "do the thing\nsecond line");
        let msg = card_msg(&e, &card);
        let spawn = msg.decision.as_ref().unwrap().spawn.as_ref().unwrap();
        assert_eq!(
            spawn.summary,
            format!("Send text to session {TARGET} (from {MGR})")
        );
        assert!(msg.body_md.contains("do the thing\nsecond line"));
        assert!(spawn.request.contains("\"kind\":\"send\""));
        assert_eq!(typed(&rx), "");
        assert_eq!(card_kind_state(&e, &card), "issued");
    }

    #[test]
    fn a_desktop_approve_of_a_send_card_delivers_the_stored_text_once() {
        let e = env();
        let (_s, rx) = add_live(&e, TARGET, serde_json::json!({}));
        let h = TestHost::new(&e);
        let card = send_card(&h, &e, "hello there");
        decide(&e, &card, "approve").unwrap();
        assert_eq!(typed(&rx), "hello there\r");
        assert_eq!(card_kind_state(&e, &card), "spawned");
        let msg = card_msg(&e, &card);
        assert_eq!(
            replies(&e, &msg.id),
            vec![
                "Approved on the desktop.".to_string(),
                format!("Delivered to session {TARGET}")
            ]
        );
        let notes = manager_prompts(&e);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].0, MGR);
        assert!(notes[0].1.contains(&card) && !notes[0].1.contains("spawn_session"));
        assert!(decide(&e, &card, "approve").is_err());
        assert!(decide(&e, &card, "reject").is_err());
        assert_eq!(typed(&rx), "");
    }

    #[test]
    fn a_stop_card_signals_the_session_only_after_a_desktop_approve() {
        let e = env();
        let (s, _rx) = add_live(&e, TARGET, serde_json::json!({}));
        let h = TestHost::new(&e);
        let card = stop_card(&h, &e);
        let msg = card_msg(&e, &card);
        assert_eq!(
            msg.decision
                .as_ref()
                .unwrap()
                .spawn
                .as_ref()
                .unwrap()
                .summary,
            format!("Stop session {TARGET}")
        );
        assert!(!s.halted.load(Ordering::SeqCst));
        decide(&e, &card, "approve").unwrap();
        assert!(s.halted.load(Ordering::SeqCst));
        assert_eq!(card_kind_state(&e, &card), "spawned");
        assert!(replies(&e, &msg.id).contains(&format!("Stopped session {TARGET}")));
        let notes = manager_prompts(&e);
        assert!(notes[0].1.contains("was stopped") && !notes[0].1.contains("spawn_session"));
        assert!(decide(&e, &card, "approve").is_err());
    }

    #[test]
    fn a_rejected_send_or_stop_card_does_nothing() {
        let e = env();
        let (s, rx) = add_live(&e, TARGET, serde_json::json!({}));
        let h = TestHost::new(&e);
        let send = send_card(&h, &e, "nope");
        let stop = stop_card(&h, &e);
        decide(&e, &send, "reject").unwrap();
        decide(&e, &stop, "reject").unwrap();
        assert_eq!(typed(&rx), "");
        assert!(!s.halted.load(Ordering::SeqCst));
        assert_eq!(card_kind_state(&e, &send), "refused");
        assert_eq!(card_kind_state(&e, &stop), "refused");
    }

    #[test]
    fn an_expired_send_or_stop_card_does_nothing_even_with_a_late_answer() {
        let e = env();
        let (s, rx) = add_live(&e, TARGET, serde_json::json!({}));
        let h = TestHost::new(&e);
        let send = send_card(&h, &e, "late");
        let stop = stop_card(&h, &e);
        let late: Vec<SignedSpawn> = [&send, &stop]
            .iter()
            .map(|c| phone_answer(&e, &card_msg(&e, c)))
            .collect();
        let exp = [&send, &stop]
            .iter()
            .map(|c| card_msg(&e, c).decision.unwrap().spawn.unwrap().exp_ms)
            .max()
            .unwrap();
        e.rt.card_tick(exp + 1);
        for signed in &late {
            e.rt.accept_spawnok(signed, exp + 2);
        }
        assert_eq!(typed(&rx), "");
        assert!(!s.halted.load(Ordering::SeqCst));
        assert_eq!(card_kind_state(&e, &send), "refused");
        assert_eq!(card_kind_state(&e, &stop), "refused");
    }

    #[test]
    fn a_signed_phone_approval_of_a_send_card_executes_once_and_a_replay_does_not() {
        let e = env();
        let (_s, rx) = add_live(&e, TARGET, serde_json::json!({}));
        let h = TestHost::new(&e);
        let card = send_card(&h, &e, "from the phone");
        let signed = phone_answer(&e, &card_msg(&e, &card));
        e.rt.accept_spawnok(&signed, now_ms());
        assert_eq!(typed(&rx), "from the phone\r");
        assert_eq!(card_kind_state(&e, &card), "spawned");
        let msg = card_msg(&e, &card);
        assert_eq!(
            replies(&e, &msg.id),
            vec![
                "Approved on your phone.".to_string(),
                format!("Delivered to session {TARGET}")
            ]
        );
        let notes = manager_prompts(&e);
        assert_eq!(notes.len(), 1);
        assert!(!notes[0].1.contains("spawn_session"));
        e.rt.accept_spawnok(&signed, now_ms());
        assert_eq!(typed(&rx), "");
        assert!(manager_prompts(&e).is_empty());
        assert!(decide(&e, &card, "approve").is_err());
        assert_eq!(typed(&rx), "");
    }

    #[test]
    fn a_signed_phone_approval_of_a_stop_card_signals_once() {
        let e = env();
        let (s, _rx) = add_live(&e, TARGET, serde_json::json!({}));
        let h = TestHost::new(&e);
        let card = stop_card(&h, &e);
        e.rt.accept_spawnok(&phone_answer(&e, &card_msg(&e, &card)), now_ms());
        assert!(s.halted.load(Ordering::SeqCst));
        assert_eq!(card_kind_state(&e, &card), "spawned");
        assert_eq!(manager_prompts(&e).len(), 1);
    }

    #[test]
    fn a_forged_phone_signature_executes_nothing() {
        let e = env();
        let (s, rx) = add_live(&e, TARGET, serde_json::json!({}));
        let h = TestHost::new(&e);
        let send = send_card(&h, &e, "forged");
        let stop = stop_card(&h, &e);
        for card in [&send, &stop] {
            let good = phone_answer(&e, &card_msg(&e, card));
            let mut other_key = good.clone();
            other_key.sig = sign(&secret(4), "tmgr-spawn:v1:forged");
            let mut other_request = good.clone();
            other_request.req_sha256 = "00".repeat(32);
            let mut other_card = good.clone();
            other_card.card_id = if card == &send {
                stop.clone()
            } else {
                send.clone()
            };
            for bad in [other_key, other_request, other_card] {
                e.rt.accept_spawnok(&bad, now_ms());
            }
        }
        assert_eq!(typed(&rx), "");
        assert!(!s.halted.load(Ordering::SeqCst));
        assert_eq!(card_kind_state(&e, &send), "issued");
        assert_eq!(card_kind_state(&e, &stop), "issued");
        assert!(manager_prompts(&e).is_empty());
    }

    #[test]
    fn a_tampered_req_sha256_on_decide_is_refused_and_executes_nothing() {
        let e = env();
        let (s, rx) = add_live(&e, TARGET, serde_json::json!({}));
        let h = TestHost::new(&e);
        let send = send_card(&h, &e, "tampered");
        let stop = stop_card(&h, &e);
        for card in [&send, &stop] {
            let err = decide_raw(
                &e,
                CLIENT_DESKTOP,
                serde_json::json!({
                    "card_id": card, "req_sha256": "00".repeat(32), "decision": "approve"
                }),
            )
            .unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidParams);
            assert!(is_issued(&e, card));
        }
        assert_eq!(typed(&rx), "");
        assert!(!s.halted.load(Ordering::SeqCst));
    }

    #[test]
    fn spawn_session_never_spawns_from_a_send_or_stop_card() {
        let e = env();
        with_dirs(&e);
        let (_s, rx) = add_live(&e, TARGET, serde_json::json!({}));
        let h = TestHost::new(&e);
        let send = send_card(&h, &e, "x");
        let stop = stop_card(&h, &e);
        for card in [&send, &stop] {
            let err = run(&h, &e, "spawn_session", card_args(&e, card)).unwrap_err();
            assert_eq!(err, "this card is not a spawn card");
            let mut signed = card_args(&e, card);
            signed["approval"] = approval(&e, card);
            assert!(run(&h, &e, "spawn_session", signed).is_err());
        }
        decide(&e, &send, "approve").unwrap();
        decide(&e, &stop, "approve").unwrap();
        for card in [&send, &stop] {
            assert!(run(&h, &e, "spawn_session", card_args(&e, card)).is_err());
        }
        assert!(h.spawned.lock().unwrap().is_empty());
        assert_eq!(typed(&rx), "x\r");
    }

    #[test]
    fn a_send_or_stop_card_is_never_edited() {
        let e = env();
        with_dirs(&e);
        let (s, rx) = add_live(&e, TARGET, serde_json::json!({}));
        let h = TestHost::new(&e);
        let send = send_card(&h, &e, "x");
        let stop = stop_card(&h, &e);
        for card in [&send, &stop] {
            let err = edit(
                &e,
                card,
                serde_json::json!({"cwd": e.work.join("a"), "mode": "ask"}),
            )
            .unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidParams);
            assert!(err.msg.contains("only a spawn card"), "{}", err.msg);
            assert!(is_issued(&e, card));
        }
        assert_eq!(typed(&rx), "");
        assert!(!s.halted.load(Ordering::SeqCst));
    }

    #[test]
    fn spawn_challenge_refuses_stop_and_send_proposals() {
        let e = env();
        for (kind, extra) in [
            ("stop", serde_json::json!({"session": TARGET})),
            (
                "send",
                serde_json::json!({"session": TARGET, "from": MGR, "text": "x"}),
            ),
        ] {
            let mut proposal = serde_json::json!({
                "v": 1, "kind": kind, "target_id": e.rt.chat.machine()
            });
            proposal
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            let (c, _peer) = conn(CLIENT_CLI);
            let err =
                e.rt.dispatch(
                    &c,
                    &Request {
                        id: 1,
                        method: method::SPAWN_CHALLENGE.into(),
                        params: serde_json::json!({"card_id": "card_k", "proposal": proposal}),
                    },
                )
                .unwrap_err();
            assert_eq!(err.code, ErrorCode::InvalidParams, "{kind}");
            assert!(e.auth.cards().state("card_k").unwrap().is_none());
        }
    }

    #[test]
    fn a_target_that_exited_before_approval_is_not_touched_and_reported() {
        let e = env();
        let (s, rx) = add_live(&e, TARGET, serde_json::json!({}));
        let h = TestHost::new(&e);
        let send = send_card(&h, &e, "too late");
        let stop = stop_card(&h, &e);
        lock(&s.inner).fixed = Some(State {
            kind: StateKind::Exited,
            since_ms: 1,
            detail: None,
            exit_code: Some(0),
        });
        assert!(s.is_exited());
        decide(&e, &send, "approve").unwrap();
        decide(&e, &stop, "approve").unwrap();
        assert_eq!(typed(&rx), "");
        assert!(!s.halted.load(Ordering::SeqCst));
        for card in [&send, &stop] {
            assert_eq!(card_kind_state(&e, card), "claimed");
            let msg = card_msg(&e, card);
            assert!(replies(&e, &msg.id)
                .iter()
                .any(|r| r.starts_with("Not done: ")));
        }
        assert!(manager_prompts(&e).iter().all(|p| p.1.contains("not")));
    }

    #[test]
    fn a_target_that_became_a_team_worker_or_risky_is_refused_at_approval() {
        let e = env();
        let (s, rx) = add_live(&e, TARGET, serde_json::json!({}));
        let h = TestHost::new(&e);
        let send = send_card(&h, &e, "x");
        lock(&s.inner).meta.no_sandbox = true;
        decide(&e, &send, "approve").unwrap();
        assert_eq!(typed(&rx), "");
        let msg = card_msg(&e, &send);
        assert!(replies(&e, &msg.id)
            .iter()
            .any(|r| r.starts_with("Not done: ")));
    }
}
