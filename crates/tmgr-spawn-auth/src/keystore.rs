use crate::key_statement::SpawnKeyStatement;
use crate::verify::PinnedKey;
use std::collections::BTreeMap;

pub type DeviceFp = String;

const MIN_PREFIX_LEN: usize = 8;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApproveError {
    PrefixTooShort,
    NotFound,
    Ambiguous,
}

#[derive(Debug, Default)]
pub struct KeyBook {
    entries: BTreeMap<DeviceFp, (SpawnKeyStatement, KeyState)>,
}

impl KeyBook {
    pub fn offer(&mut self, statement: SpawnKeyStatement) -> OfferOutcome {
        let fp = statement.device_fp().to_string();
        let outcome = match self.entries.get(&fp) {
            None => OfferOutcome::Added,
            Some((old, _)) if *old == statement => return OfferOutcome::Unchanged,
            Some((old, _)) if crate::supersedes(&statement, old) => OfferOutcome::Replaced,
            Some((old, _))
                if old.user_id() == statement.user_id()
                    && statement.issued_at_ms() < old.issued_at_ms() =>
            {
                return OfferOutcome::Stale
            }
            Some(_) => return OfferOutcome::Conflict,
        };
        self.entries.insert(fp, (statement, KeyState::Pending));
        outcome
    }

    pub fn approve(&mut self, fp_prefix: &str) -> Result<DeviceFp, ApproveError> {
        if fp_prefix.len() < MIN_PREFIX_LEN {
            return Err(ApproveError::PrefixTooShort);
        }
        let mut hits = self
            .entries
            .iter_mut()
            .filter(|(fp, (_, state))| *state == KeyState::Pending && fp.starts_with(fp_prefix));
        let (fp, (_, state)) = hits.next().ok_or(ApproveError::NotFound)?;
        if hits.next().is_some() {
            return Err(ApproveError::Ambiguous);
        }
        *state = KeyState::Approved;
        Ok(fp.clone())
    }

    pub fn state(&self, device_fp: &str) -> Option<KeyState> {
        self.entries.get(device_fp).map(|(_, s)| *s)
    }

    pub fn pending(&self) -> Vec<&SpawnKeyStatement> {
        self.entries
            .values()
            .filter(|(_, s)| *s == KeyState::Pending)
            .map(|(st, _)| st)
            .collect()
    }

    pub fn pinned_key(&self, user_id: &str, device_fp: &str) -> Option<PinnedKey> {
        let (st, state) = self.entries.get(device_fp)?;
        (*state == KeyState::Approved && st.user_id() == user_id).then(|| PinnedKey {
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

    fn fp(c: char) -> String {
        c.to_string().repeat(64)
    }

    fn st(user: &str, fp: &str, key: u8, issued: u64) -> SpawnKeyStatement {
        SpawnKeyStatement::for_test(user, fp, "dev-1", vec![key; 65], issued)
    }

    #[test]
    fn first_key_is_pending_and_never_pinned() {
        let mut b = KeyBook::default();
        assert_eq!(b.offer(st("42", &fp('a'), 1, 10)), OfferOutcome::Added);
        assert_eq!(b.state(&fp('a')), Some(KeyState::Pending));
        assert_eq!(b.pinned_key("42", &fp('a')), None);
        assert_eq!(b.pending().len(), 1);
    }

    #[test]
    fn approve_pins_the_key_for_its_user_only() {
        let mut b = KeyBook::default();
        b.offer(st("42", &fp('a'), 1, 10));
        assert_eq!(b.approve(&fp('a')[..8]), Ok(fp('a')));
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
    fn approve_prefix_rules() {
        let mut b = KeyBook::default();
        b.offer(st("42", &format!("abcdef01{}", "0".repeat(56)), 1, 10));
        b.offer(st("42", &format!("abcdef02{}", "0".repeat(56)), 2, 10));
        b.offer(st("42", &fp('c'), 3, 10));
        assert_eq!(b.approve("abcdef0"), Err(ApproveError::PrefixTooShort));
        assert_eq!(b.approve(""), Err(ApproveError::PrefixTooShort));
        assert_eq!(b.approve("abcdef0z"), Err(ApproveError::NotFound));
        assert_eq!(b.approve("ABCDEF01"), Err(ApproveError::NotFound));
        assert_eq!(b.approve("00000000"), Err(ApproveError::NotFound));
        assert_eq!(b.approve("cccccccc"), Ok(fp('c')));
        assert_eq!(
            b.approve("abcdef01"),
            Ok(format!("abcdef01{}", "0".repeat(56)))
        );
        assert_eq!(b.approve("abcdef01"), Err(ApproveError::NotFound));
    }

    #[test]
    fn ambiguous_prefix_is_refused() {
        let mut c = KeyBook::default();
        c.offer(st("42", &format!("abcdef01{}", "0".repeat(56)), 1, 10));
        c.offer(st("42", &format!("abcdef01{}", "1".repeat(56)), 2, 10));
        assert_eq!(c.approve("abcdef01"), Err(ApproveError::Ambiguous));
        assert_eq!(
            c.state(&format!("abcdef01{}", "0".repeat(56))),
            Some(KeyState::Pending)
        );
    }

    #[test]
    fn approved_entries_do_not_count_for_ambiguity() {
        let mut b = KeyBook::default();
        b.offer(st("42", &format!("abcdef01{}", "0".repeat(56)), 1, 10));
        b.offer(st("42", &format!("abcdef01{}", "1".repeat(56)), 2, 10));
        b.approve(&format!("abcdef01{}", "0".repeat(10))).unwrap();
        assert_eq!(
            b.approve("abcdef01"),
            Ok(format!("abcdef01{}", "1".repeat(56)))
        );
    }

    #[test]
    fn identical_statement_is_a_no_op_and_keeps_approval() {
        let mut b = KeyBook::default();
        b.offer(st("42", &fp('a'), 1, 10));
        b.approve("aaaaaaaa").unwrap();
        assert_eq!(b.offer(st("42", &fp('a'), 1, 10)), OfferOutcome::Unchanged);
        assert_eq!(b.state(&fp('a')), Some(KeyState::Approved));
    }

    #[test]
    fn newer_statement_replaces_and_drops_approval() {
        let mut b = KeyBook::default();
        b.offer(st("42", &fp('a'), 1, 10));
        b.approve("aaaaaaaa").unwrap();
        assert_eq!(b.offer(st("42", &fp('a'), 2, 11)), OfferOutcome::Replaced);
        assert_eq!(b.state(&fp('a')), Some(KeyState::Pending));
        assert_eq!(b.pinned_key("42", &fp('a')), None);
        b.approve("aaaaaaaa").unwrap();
        assert_eq!(b.pinned_key("42", &fp('a')).unwrap().spawn_pub, vec![2; 65]);
    }

    #[test]
    fn older_equal_time_and_foreign_statements_are_refused() {
        let mut b = KeyBook::default();
        b.offer(st("42", &fp('a'), 1, 10));
        b.approve("aaaaaaaa").unwrap();
        assert_eq!(b.offer(st("42", &fp('a'), 2, 9)), OfferOutcome::Stale);
        assert_eq!(b.offer(st("42", &fp('a'), 1, 9)), OfferOutcome::Stale);
        assert_eq!(b.offer(st("42", &fp('a'), 2, 10)), OfferOutcome::Conflict);
        assert_eq!(b.offer(st("43", &fp('a'), 2, 11)), OfferOutcome::Conflict);
        assert_eq!(b.state(&fp('a')), Some(KeyState::Approved));
        assert_eq!(b.pinned_key("42", &fp('a')).unwrap().spawn_pub, vec![1; 65]);
    }
}
