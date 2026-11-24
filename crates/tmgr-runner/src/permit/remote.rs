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
    pub burned: HashSet<String>,
}

#[derive(Debug, Default)]
pub struct PermitDesk {
    pending: HashMap<String, PendingPermit>,
    closed: VecDeque<String>,
    desktop: DesktopPolicy,
}

impl PermitDesk {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_desktop(&mut self, policy: DesktopPolicy) {
        self.desktop = policy;
    }

    pub fn pending(&self, card_id: &str) -> Option<&PendingPermit> {
        self.pending.get(card_id)
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
        fresh_nonce: &mut dyn FnMut() -> String,
    ) -> Vec<Action> {
        let mut out = Vec::new();
        let desktop = self.desktop;
        let live = card.filter(|c| c.can_answer);
        let held = self
            .pending
            .iter()
            .find(|(_, p)| p.record.session == session)
            .map(|(id, _)| id.clone());
        if let Some(id) = held {
            if live.is_some_and(|c| c.id == id) {
                let p = self.pending.get_mut(&id).expect("held card is pending");
                if now_ms > p.record.exp_ms {
                    if p.reissues < MAX_REISSUES {
                        p.reissue(now_ms, fresh_nonce());
                        out.push(post(&id, &p.record, p.reissues, desktop));
                    } else {
                        self.pending.remove(&id);
                        self.tombstone(&id);
                        out.push(close(&id, "expired"));
                    }
                }
                return out;
            }
            self.pending.remove(&id);
            out.push(close(&id, "gone"));
        }
        let Some(c) = live else {
            return out;
        };
        if self.closed.contains(&c.id) || self.pending.contains_key(&c.id) {
            return out;
        }
        if self.pending.len() >= MAX_PENDING {
            let oldest = self
                .pending
                .iter()
                .min_by_key(|(_, p)| p.record.exp_ms)
                .map(|(id, _)| id.clone());
            if let Some(id) = oldest {
                self.pending.remove(&id);
                self.tombstone(&id);
                out.push(close(&id, "gone"));
            }
        }
        let record = PermitRecord {
            user_id: ids.user_id.clone(),
            runner_id: ids.runner_id.clone(),
            runner_fp: ids.runner_fp.clone(),
            card_id: c.id.clone(),
            session: session.to_string(),
            nonce: fresh_nonce(),
            issued_ms: now_ms,
            exp_ms: now_ms.saturating_add(MAX_PERMIT_TTL_MS),
            screen: screen_of(c),
        };
        out.push(post(&c.id, &record, 0, desktop));
        self.pending.insert(
            c.id.clone(),
            PendingPermit {
                record,
                reissues: 0,
                burned: HashSet::new(),
            },
        );
        out
    }

    pub fn answered_locally(&mut self, card_id: &str) -> Vec<Action> {
        if self.pending.remove(card_id).is_none() {
            return Vec::new();
        }
        self.tombstone(card_id);
        vec![close(card_id, "answered in attach")]
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
        fresh_nonce: &mut dyn FnMut() -> String,
    ) -> Vec<Action> {
        let id = signed.card_id.clone();
        let desktop = self.desktop;
        let sender = PermitSender { desktop, ..sender };
        let line = |result: &str, action: &str| Action::Audit(audit_line(signed, result, action));
        let Some(p) = self.pending.get_mut(&id) else {
            return vec![line("stale", "dropped")];
        };
        if p.burned.contains(&signed.nonce) {
            return vec![line("stale", "dropped")];
        }
        let Some(key) = key else {
            return vec![line("unknown_key", "dropped")];
        };
        let verdict = verify_permit(signed, &p.record, key, sender, trust, now_ms);
        let refusal = verdict.err();
        let Some(e) = refusal else {
            p.burned.insert(p.record.nonce.clone());
            let screen_same = current.is_some_and(|c| {
                c.id == id
                    && c.can_answer
                    && screen_of(c).req_sha256() == p.record.screen.req_sha256()
            });
            let session = p.record.session.clone();
            self.pending.remove(&id);
            self.tombstone(&id);
            if !screen_same {
                return vec![close(&id, "gone"), line("screen_changed", "closed")];
            }
            return vec![
                Action::Answer {
                    session,
                    card_id: id.clone(),
                    choice: match signed.choice {
                        PermitChoice::Yes => PermissionChoice::Yes,
                        PermitChoice::No => PermissionChoice::No,
                        PermitChoice::Always => PermissionChoice::Always,
                    },
                    confirmed: signed.confirmed == 1,
                    audit: audit_line(signed, "ok", "answered"),
                },
                close(&id, &format!("answered on {sender_label}")),
            ];
        };
        match e.outcome() {
            PermitOutcome::Drop => vec![line(e.code(), "dropped")],
            PermitOutcome::Reissue => {
                if p.reissues < MAX_REISSUES {
                    p.reissue(now_ms, fresh_nonce());
                    vec![
                        post(&id, &p.record, p.reissues, desktop),
                        line(e.code(), "reissued"),
                    ]
                } else {
                    self.pending.remove(&id);
                    self.tombstone(&id);
                    vec![close(&id, "answer in attach"), line(e.code(), "closed")]
                }
            }
        }
    }

    fn tombstone(&mut self, card_id: &str) {
        if self.closed.len() >= MAX_CLOSED {
            self.closed.pop_front();
        }
        self.closed.push_back(card_id.to_string());
    }
}

impl PendingPermit {
    fn reissue(&mut self, now_ms: u64, nonce: String) {
        let old = std::mem::replace(&mut self.record.nonce, nonce);
        self.burned.insert(old);
        self.record.issued_ms = now_ms;
        self.record.exp_ms = now_ms.saturating_add(MAX_PERMIT_TTL_MS);
        self.reissues += 1;
    }
}

fn close(card_id: &str, reason: &str) -> Action {
    Action::Close {
        card_id: card_id.to_string(),
        reason: reason.to_string(),
    }
}

fn post(card_id: &str, r: &PermitRecord, reissue: u8, desktop: DesktopPolicy) -> Action {
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
        }),
    }
}

fn screen_of(c: &PermissionCard) -> PermitScreen {
    PermitScreen {
        agent: c.agent.clone(),
        tool: c.tool.clone(),
        text: c.text.clone(),
        path: c.path.clone(),
        reason: c.reason.clone(),
        from: c.from.clone(),
        warning: c.warning.clone(),
        danger: c.danger.clone(),
        always_label: c.always_label.clone(),
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
    use tmgr_spawn_auth::{b64_encode, permit_signed_string, TrustStatus};

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
            warning: Some("writes outside cwd".into()),
            danger: vec![],
            can_answer: true,
            why_not: None,
            always_label: Some("Always allow".into()),
            armed_in_ms: 0,
        }
    }

    struct Gen(u64);
    impl Gen {
        fn next(&mut self) -> String {
            self.0 += 1;
            format!("{:032x}", self.0)
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
            sig: String::new(),
        };
        let sig: Signature = sk().sign(permit_signed_string(rec, &k, &s).as_bytes());
        s.sig = b64_encode(sig.to_der().as_bytes());
        s
    }

    fn rec(d: &PermitDesk) -> PermitRecord {
        d.pending(CARD).unwrap().record.clone()
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
        let s = sign(&rec(&d), PermitChoice::Yes, 0);
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 1000, &mut g);
        assert_eq!(
            acts[0],
            Action::Answer {
                session: SESSION.into(),
                card_id: CARD.into(),
                choice: PermissionChoice::Yes,
                confirmed: false,
                audit: format!(
                    "permit answer: card={CARD} device={} choice=yes result=ok action=answered",
                    "d".repeat(12)
                )
            }
        );
        assert_eq!(closes(&acts), vec!["answered on Mac".to_string()]);
        let a = answer_audit(&acts);
        assert_eq!(
            a,
            format!(
                "permit answer: card={CARD} device={} choice=yes result=ok action=answered",
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
        assert_eq!(closes(&acts), vec!["answered on Phone".to_string()]);
        assert!(answer_audit(&acts).contains("choice=always result=ok action=answered"));
    }

    fn assert_dropped_and_still_answerable(ctx: Ctx) {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let before = d.pending(CARD).unwrap().clone();
        let s = sign(&before.record, PermitChoice::Yes, 0);
        let acts = accept(&mut d, &s, &ctx, Some(&c), T0 + 1000, &mut g);
        only_audit(&acts);
        assert!(audit(&acts).ends_with("action=dropped"), "{acts:?}");
        assert_eq!(d.pending(CARD).unwrap(), &before);
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
        assert!(d.pending(CARD).is_some());
    }

    #[test]
    fn bad_signature_is_dropped_and_the_card_stays_answerable() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let before = d.pending(CARD).unwrap().clone();
        let r = rec(&d);
        let mut s = sign(&r, PermitChoice::Yes, 0);
        let other: Signature = SigningKey::from_slice(&[9; 32])
            .unwrap()
            .sign(permit_signed_string(&r, &key(), &s).as_bytes());
        s.sig = b64_encode(other.to_der().as_bytes());
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 1, &mut g);
        only_audit(&acts);
        assert!(audit(&acts).contains("result=bad_sig action=dropped"));
        assert_eq!(d.pending(CARD).unwrap(), &before);
        let good = sign(&r, PermitChoice::Yes, 0);
        let acts = accept(&mut d, &good, &Ctx::ok(), Some(&c), T0 + 2, &mut g);
        assert_eq!(answers(&acts), 1);
    }

    #[test]
    fn unsigned_junk_on_a_danger_card_burns_nothing_and_uses_no_reissue() {
        let mut g = Gen(0);
        let c = danger_card();
        let mut d = desk_under(DesktopPolicy::Safe, &c, &mut g);
        let before = d.pending(CARD).unwrap().clone();
        for _ in 0..5 {
            let mut s = sign(&rec(&d), PermitChoice::Yes, 0);
            s.sig = "AAAA".into();
            let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 1, &mut g);
            only_audit(&acts);
            assert!(audit(&acts).contains("action=dropped"));
        }
        assert_eq!(d.pending(CARD).unwrap(), &before);
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
        assert!(matches!(&acts[0], Action::Post { card_id, .. } if card_id == CARD));
        let acts = d.observe(SESSION, Some(&c), &ids(), T0 + 5, &mut || g.next());
        assert!(acts.is_empty());
    }

    #[test]
    fn observe_disappearing_card_closes_gone() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let acts = d.observe(SESSION, None, &ids(), T0 + 5, &mut || g.next());
        assert_eq!(
            acts,
            vec![Action::Close {
                card_id: CARD.into(),
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
        assert!(matches!(&acts[1], Action::Post { card_id, .. } if card_id == "card-2"));
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

    #[test]
    fn expiry_on_screen_reposts_with_a_new_nonce() {
        let mut g = Gen(0);
        let c = card();
        let mut d = desk_with(&c, &mut g);
        let old = rec(&d);
        let now = old.exp_ms + 1;
        let acts = d.observe(SESSION, Some(&c), &ids(), now, &mut || g.next());
        let p = posted(&acts);
        assert_ne!(p.nonce, old.nonce);
        assert_eq!(p.reissue, 1);
        assert_eq!(p.exp_ms, now + MAX_PERMIT_TTL_MS);
        let s = sign(&old, PermitChoice::Yes, 0);
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), now + 1, &mut g);
        assert_eq!(answers(&acts), 0);
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
            let again = d.observe(SESSION, Some(&c), &ids(), later, &mut || g.next());
            let p = posted(&again);
            assert_eq!(p.reissue, 1);
            assert_eq!(p.desktop_ok, want, "reissue {policy:?} danger={danger}");
        }
    }

    #[test]
    fn a_dropped_desktop_answer_burns_nothing_and_a_phone_can_still_answer() {
        let mut g = Gen(0);
        let c = danger_card();
        let mut d = desk_under(DesktopPolicy::Safe, &c, &mut g);
        let before = d.pending(CARD).unwrap().clone();
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
        assert_eq!(d.pending(CARD).unwrap(), &before);
        assert_eq!(d.pending(CARD).unwrap().reissues, before.reissues);
        let acts = accept(&mut d, &s, &Ctx::ok(), Some(&c), T0 + 1001, &mut g);
        assert_eq!(answers(&acts), 1, "{acts:?}");
    }
}
