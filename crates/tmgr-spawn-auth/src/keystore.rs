use crate::key_statement::SpawnKeyStatement;
use crate::verify::PinnedKey;
use std::collections::BTreeMap;

pub type DeviceFp = String;

const MIN_PREFIX_LEN: usize = 8;
const MIN_SPAWN_KEY_PREFIX_LEN: usize = 16;
const MAX_ENTRIES: usize = 64;
const REPLACE_COOLDOWN_MS: u64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyState {
    Pending,
    Approved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferOutcome {
    Added,
    Replaced,
    Unchanged,
    Stale,
    Conflict,
    /// The book already holds `MAX_ENTRIES` devices; a new device_fp is not recorded.
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApproveError {
    PrefixTooShort,
    NotFound,
    Ambiguous,
    KeyMismatch,
    TooSoon,
}

#[derive(Debug)]
struct Entry {
    statement: SpawnKeyStatement,
    state: KeyState,
    replaced_at_ms: Option<u64>,
}

#[derive(Debug, Default)]
pub struct KeyBook {
    entries: BTreeMap<DeviceFp, Entry>,
}

impl KeyBook {
    pub fn offer(&mut self, statement: SpawnKeyStatement, now_ms: u64) -> OfferOutcome {
        let fp = statement.device_fp().to_string();
        let mut replaced_at_ms = None;
        let outcome = match self.entries.get(&fp) {
            None if self.entries.len() >= MAX_ENTRIES => return OfferOutcome::Full,
            None => OfferOutcome::Added,
            Some(e) if e.statement == statement => return OfferOutcome::Unchanged,
            Some(e) if crate::supersedes(&statement, &e.statement) => {
                replaced_at_ms = Some(now_ms);
                OfferOutcome::Replaced
            }
            Some(e)
                if e.statement.user_id() == statement.user_id()
                    && statement.issued_at_ms() < e.statement.issued_at_ms() =>
            {
                return OfferOutcome::Stale
            }
            Some(_) => return OfferOutcome::Conflict,
        };
        self.entries.insert(
            fp,
            Entry {
                statement,
                state: KeyState::Pending,
                replaced_at_ms,
            },
        );
        outcome
    }

    pub fn restore(
        &mut self,
        statement: SpawnKeyStatement,
        state: KeyState,
        replaced_at_ms: Option<u64>,
    ) {
        self.entries.insert(
            statement.device_fp().to_string(),
            Entry {
                statement,
                state,
                replaced_at_ms,
            },
        );
    }

    pub fn approve(
        &mut self,
        fp_prefix: &str,
        spawn_key_prefix: &str,
        now_ms: u64,
    ) -> Result<DeviceFp, ApproveError> {
        if fp_prefix.len() < MIN_PREFIX_LEN || spawn_key_prefix.len() < MIN_SPAWN_KEY_PREFIX_LEN {
            return Err(ApproveError::PrefixTooShort);
        }
        let mut hits = self
            .entries
            .iter_mut()
            .filter(|(fp, e)| e.state == KeyState::Pending && fp.starts_with(fp_prefix));
        let (fp, entry) = hits.next().ok_or(ApproveError::NotFound)?;
        if hits.next().is_some() {
            return Err(ApproveError::Ambiguous);
        }
        if !entry.statement.spawn_key_fp().starts_with(spawn_key_prefix) {
            return Err(ApproveError::KeyMismatch);
        }
        if let Some(at) = entry.replaced_at_ms {
            let at = at.min(now_ms);
            entry.replaced_at_ms = Some(at);
            if now_ms < at.saturating_add(REPLACE_COOLDOWN_MS) {
                return Err(ApproveError::TooSoon);
            }
        }
        entry.state = KeyState::Approved;
        entry.replaced_at_ms = None;
        Ok(fp.clone())
    }

    pub fn state(&self, device_fp: &str) -> Option<KeyState> {
        self.entries.get(device_fp).map(|e| e.state)
    }

    pub fn replaced_at_ms(&self, device_fp: &str) -> Option<u64> {
        self.entries.get(device_fp)?.replaced_at_ms
    }

    pub fn pending(&self) -> Vec<&SpawnKeyStatement> {
        self.entries
            .values()
            .filter(|e| e.state == KeyState::Pending)
            .map(|e| &e.statement)
            .collect()
    }

    pub fn pinned_key(&self, user_id: &str, device_fp: &str) -> Option<PinnedKey> {
        let e = self.entries.get(device_fp)?;
        let st = &e.statement;
        (e.state == KeyState::Approved && st.user_id() == user_id).then(|| PinnedKey {
            user_id: st.user_id().into(),
            device_id: st.device_id().into(),
            device_fp: st.device_fp().into(),
            spawn_pub: st.spawn_pub().to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::key_statement::SpawnKeyStatement;

    const N: u64 = 1_000_000;

    fn fp(c: char) -> String {
        c.to_string().repeat(64)
    }

    fn st(user: &str, fp: &str, key: u8, issued: u64) -> SpawnKeyStatement {
        SpawnKeyStatement::for_test(user, fp, "dev-1", vec![key; 65], issued)
    }

    fn sk(key: u8) -> String {
        st("42", &fp('a'), key, 0).spawn_key_fp()
    }

    #[test]
    fn first_key_is_pending_and_never_pinned() {
        let mut b = KeyBook::default();
        assert_eq!(b.offer(st("42", &fp('a'), 1, 10), N), OfferOutcome::Added);
        assert_eq!(b.state(&fp('a')), Some(KeyState::Pending));
        assert_eq!(b.pinned_key("42", &fp('a')), None);
        assert_eq!(b.pending().len(), 1);
    }

    #[test]
    fn spawn_key_fp_is_sha256_of_the_spawn_pub() {
        use sha2::{Digest, Sha256};
        let s = st("42", &fp('a'), 1, 10);
        assert_eq!(s.spawn_key_fp(), hex::encode(Sha256::digest(vec![1u8; 65])));
        assert_eq!(s.spawn_key_fp().len(), 64);
    }

    #[test]
    fn approve_pins_the_key_for_its_user_only() {
        let mut b = KeyBook::default();
        b.offer(st("42", &fp('a'), 1, 10), N);
        assert_eq!(b.approve(&fp('a')[..8], &sk(1)[..16], N), Ok(fp('a')));
        assert_eq!(b.state(&fp('a')), Some(KeyState::Approved));
        let k = b.pinned_key("42", &fp('a')).unwrap();
        assert_eq!(k.user_id, "42");
        assert_eq!(k.device_id, "dev-1");
        assert_eq!(k.device_fp, fp('a'));
        assert_eq!(k.spawn_pub, vec![1; 65]);
        assert_eq!(b.pinned_key("43", &fp('a')), None);
        assert_eq!(b.pinned_key("42", &fp('b')), None);
        assert!(b.pending().is_empty());
    }

    #[test]
    fn approve_refuses_a_spawn_key_that_is_not_the_pending_one() {
        let mut b = KeyBook::default();
        b.offer(st("42", &fp('a'), 1, 10), N);
        assert_eq!(
            b.approve(&fp('a')[..8], &sk(2)[..16], N),
            Err(ApproveError::KeyMismatch)
        );
        assert_eq!(
            b.approve(&fp('a')[..8], "ZZZZZZZZZZZZZZZZ", N),
            Err(ApproveError::KeyMismatch)
        );
        assert_eq!(
            b.approve(&fp('a')[..8], &sk(1)[..15], N),
            Err(ApproveError::PrefixTooShort)
        );
        assert_eq!(b.state(&fp('a')), Some(KeyState::Pending));
        assert_eq!(b.pinned_key("42", &fp('a')), None);
    }

    #[test]
    fn approve_prefix_rules() {
        let mut b = KeyBook::default();
        b.offer(st("42", &format!("abcdef01{}", "0".repeat(56)), 1, 10), N);
        b.offer(st("42", &format!("abcdef02{}", "0".repeat(56)), 2, 10), N);
        b.offer(st("42", &fp('c'), 3, 10), N);
        assert_eq!(
            b.approve("abcdef0", &sk(1), N),
            Err(ApproveError::PrefixTooShort)
        );
        assert_eq!(b.approve("", &sk(1), N), Err(ApproveError::PrefixTooShort));
        assert_eq!(
            b.approve("abcdef0z", &sk(1), N),
            Err(ApproveError::NotFound)
        );
        assert_eq!(
            b.approve("ABCDEF01", &sk(1), N),
            Err(ApproveError::NotFound)
        );
        assert_eq!(
            b.approve("00000000", &sk(1), N),
            Err(ApproveError::NotFound)
        );
        assert_eq!(b.approve("cccccccc", &sk(3), N), Ok(fp('c')));
        assert_eq!(
            b.approve("abcdef01", &sk(1), N),
            Ok(format!("abcdef01{}", "0".repeat(56)))
        );
        assert_eq!(
            b.approve("abcdef01", &sk(1), N),
            Err(ApproveError::NotFound)
        );
    }

    #[test]
    fn ambiguous_prefix_is_refused() {
        let mut c = KeyBook::default();
        c.offer(st("42", &format!("abcdef01{}", "0".repeat(56)), 1, 10), N);
        c.offer(st("42", &format!("abcdef01{}", "1".repeat(56)), 2, 10), N);
        assert_eq!(
            c.approve("abcdef01", &sk(1), N),
            Err(ApproveError::Ambiguous)
        );
        assert_eq!(
            c.state(&format!("abcdef01{}", "0".repeat(56))),
            Some(KeyState::Pending)
        );
    }

    #[test]
    fn approved_entries_do_not_count_for_ambiguity() {
        let mut b = KeyBook::default();
        b.offer(st("42", &format!("abcdef01{}", "0".repeat(56)), 1, 10), N);
        b.offer(st("42", &format!("abcdef01{}", "1".repeat(56)), 2, 10), N);
        b.approve(&format!("abcdef01{}", "0".repeat(10)), &sk(1), N)
            .unwrap();
        assert_eq!(
            b.approve("abcdef01", &sk(2), N),
            Ok(format!("abcdef01{}", "1".repeat(56)))
        );
    }

    #[test]
    fn identical_statement_is_a_no_op_and_keeps_approval() {
        let mut b = KeyBook::default();
        b.offer(st("42", &fp('a'), 1, 10), N);
        b.approve("aaaaaaaa", &sk(1), N).unwrap();
        assert_eq!(
            b.offer(st("42", &fp('a'), 1, 10), N),
            OfferOutcome::Unchanged
        );
        assert_eq!(b.state(&fp('a')), Some(KeyState::Approved));
    }

    #[test]
    fn newer_statement_replaces_and_drops_approval() {
        let mut b = KeyBook::default();
        b.offer(st("42", &fp('a'), 1, 10), N);
        b.approve("aaaaaaaa", &sk(1), N).unwrap();
        assert_eq!(
            b.offer(st("42", &fp('a'), 2, 11), N + 5),
            OfferOutcome::Replaced
        );
        assert_eq!(b.state(&fp('a')), Some(KeyState::Pending));
        assert_eq!(b.pinned_key("42", &fp('a')), None);
        assert_eq!(b.replaced_at_ms(&fp('a')), Some(N + 5));
        b.approve("aaaaaaaa", &sk(2), N + 5 + 60_000).unwrap();
        assert_eq!(b.pinned_key("42", &fp('a')).unwrap().spawn_pub, vec![2; 65]);
        assert_eq!(b.replaced_at_ms(&fp('a')), None);
    }

    #[test]
    fn approving_a_replaced_entry_is_refused_for_sixty_seconds() {
        let mut b = KeyBook::default();
        b.offer(st("42", &fp('a'), 1, 10), N);
        b.approve("aaaaaaaa", &sk(1), N).unwrap();
        b.offer(st("42", &fp('a'), 2, 11), N + 5);
        assert_eq!(
            b.approve("aaaaaaaa", &sk(2), N + 5),
            Err(ApproveError::TooSoon)
        );
        assert_eq!(
            b.approve("aaaaaaaa", &sk(2), N + 5 + 59_999),
            Err(ApproveError::TooSoon)
        );
        assert_eq!(
            b.approve("aaaaaaaa", &sk(2), N - 1),
            Err(ApproveError::TooSoon)
        );
        assert_eq!(b.state(&fp('a')), Some(KeyState::Pending));
        assert!(b.approve("aaaaaaaa", &sk(2), N + 5 + 60_000).is_ok());
    }

    #[test]
    fn replacing_a_never_approved_entry_also_cools_down() {
        let mut b = KeyBook::default();
        b.offer(st("42", &fp('a'), 1, 10), N);
        b.offer(st("42", &fp('a'), 2, 11), N);
        assert_eq!(b.replaced_at_ms(&fp('a')), Some(N));
        assert_eq!(
            b.approve("aaaaaaaa", &sk(2), N + 59_999),
            Err(ApproveError::TooSoon)
        );
        assert!(b.approve("aaaaaaaa", &sk(2), N + 60_000).is_ok());
    }

    #[test]
    fn spawn_key_prefix_needs_sixteen_chars_and_the_full_hash_is_accepted() {
        let mut b = KeyBook::default();
        b.offer(st("42", &fp('a'), 1, 10), N);
        assert_eq!(
            b.approve("aaaaaaaa", &sk(1)[..15], N),
            Err(ApproveError::PrefixTooShort)
        );
        assert_eq!(b.approve("aaaaaaaa", &sk(1), N), Ok(fp('a')));
    }

    #[test]
    fn cooldown_never_outlasts_sixty_seconds_of_the_callers_clock() {
        let mut b = KeyBook::default();
        b.restore(
            st("42", &fp('a'), 2, 11),
            KeyState::Pending,
            Some(N + 10_000_000),
        );
        assert_eq!(b.approve("aaaaaaaa", &sk(2), N), Err(ApproveError::TooSoon));
        assert_eq!(b.replaced_at_ms(&fp('a')), Some(N));
        assert!(b.approve("aaaaaaaa", &sk(2), N + 60_000).is_ok());
    }

    #[test]
    fn the_sixty_fifth_distinct_device_is_refused_but_known_ones_still_update() {
        let mut b = KeyBook::default();
        let fpn = |i: usize| format!("{i:064x}");
        for i in 0..MAX_ENTRIES {
            assert_eq!(b.offer(st("42", &fpn(i), 1, 10), N), OfferOutcome::Added);
        }
        assert_eq!(
            b.offer(st("42", &fpn(MAX_ENTRIES), 1, 10), N),
            OfferOutcome::Full
        );
        assert_eq!(b.state(&fpn(MAX_ENTRIES)), None);
        assert_eq!(b.offer(st("42", &fpn(0), 2, 11), N), OfferOutcome::Replaced);
    }

    #[test]
    fn older_equal_time_and_foreign_statements_are_refused() {
        let mut b = KeyBook::default();
        b.offer(st("42", &fp('a'), 1, 10), N);
        b.approve("aaaaaaaa", &sk(1), N).unwrap();
        assert_eq!(b.offer(st("42", &fp('a'), 2, 9), N), OfferOutcome::Stale);
        assert_eq!(b.offer(st("42", &fp('a'), 1, 9), N), OfferOutcome::Stale);
        assert_eq!(
            b.offer(st("42", &fp('a'), 2, 10), N),
            OfferOutcome::Conflict
        );
        assert_eq!(
            b.offer(st("43", &fp('a'), 2, 11), N),
            OfferOutcome::Conflict
        );
        assert_eq!(b.state(&fp('a')), Some(KeyState::Approved));
        assert_eq!(b.pinned_key("42", &fp('a')).unwrap().spawn_pub, vec![1; 65]);
    }
}
