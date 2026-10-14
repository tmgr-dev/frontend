use crate::audit::Audit;
use crate::grammar::{self, PubKey};
use crate::store::{Paths, Store};
use serde_json::json;
use tmgr_core::error::{Error, Result};

pub fn own_key(paths: &Paths) -> Result<Option<PubKey>> {
    match std::fs::read_to_string(paths.public_key()) {
        Ok(text) => grammar::parse_pubkey(&text).map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn refuse(paths: &Paths, event: &str, link: &str, fp: &str, reason: &str) -> Error {
    let _ = Audit::new(&paths.audit_dir())
        .append(event, json!({"link": link, "fp": fp, "reason": reason}));
    Error::conflict(reason)
}

pub fn check_trust(paths: &Paths, store: &Store, name: &str, key: &PubKey) -> Result<()> {
    if own_key(paths)?.is_some_and(|o| o.blob == key.blob) {
        return Err(refuse(
            paths,
            "link.refused",
            name,
            &key.fingerprint,
            "this key is this machine's own link identity; a machine cannot lead itself",
        ));
    }
    if let Some((peer, _)) = store.peer_keys.iter().find(|(_, k)| **k == key.blob) {
        return Err(refuse(
            paths,
            "link.refused",
            name,
            &key.fingerprint,
            &format!("this key is the link identity of {peer}, which this machine leads; trusting it would create a cycle"),
        ));
    }
    Ok(())
}

pub fn check_follower(
    paths: &Paths,
    store: &Store,
    name: &str,
    link_key: Option<&str>,
) -> Result<PubKey> {
    let Some(text) = link_key else {
        let _ = Audit::new(&paths.audit_dir()).append(
            "link.refused",
            json!({"link": name, "fp": "", "reason": "follower sent no link identity"}),
        );
        return Err(crate::link_err(
            "the follower is too old or unidentified: its handshake carries no link identity",
        )
        .hint("update tmgr there and run tmgr link init on it"));
    };
    let key = grammar::parse_pubkey(text).map_err(|_| {
        crate::link_err("the follower sent an invalid link identity in its handshake")
    })?;
    if own_key(paths)?.is_some_and(|o| o.blob == key.blob) {
        return Err(refuse(
            paths,
            "link.refused",
            name,
            &key.fingerprint,
            "the follower has this machine's own link identity; a machine cannot lead itself",
        ));
    }
    if let Some((leader, _)) = store.leaders.iter().find(|(_, l)| l.key == key.blob) {
        return Err(refuse(
            paths,
            "link.refused",
            name,
            &key.fingerprint,
            &format!("the follower already leads this machine (trusted as {leader}); linking it would create a cycle"),
        ));
    }
    Ok(key)
}

pub fn worker_refusal(paths: &Paths, store: &Store, leader_key: &str) -> Option<&'static str> {
    match own_key(paths) {
        Err(_) => Some("link identity unreadable"),
        Ok(Some(own)) if own.blob == leader_key => {
            Some("leader key is this machine's own identity")
        }
        _ if store.peer_keys.values().any(|k| k == leader_key) => {
            Some("leader is a follower of this machine")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::testkeys;
    use crate::store::Leader;

    fn paths_with_identity() -> (tempfile::TempDir, Paths, PubKey) {
        let d = crate::testutil::private_tempdir();
        let paths = Paths::under(d.path());
        std::fs::create_dir_all(&paths.config).unwrap();
        let own = testkeys::key();
        std::fs::write(paths.public_key(), format!("{} tmgr-link\n", own.openssh())).unwrap();
        (d, paths, own)
    }

    fn leader_with(key: &PubKey) -> Leader {
        Leader {
            key: key.blob.clone(),
            fingerprint: key.fingerprint.clone(),
            added: "2026-10-13T00:00:00Z".into(),
            scope: Default::default(),
            enrollment: String::new(),
            authorized_keys: String::new(),
        }
    }

    fn refusals(paths: &Paths) -> Vec<serde_json::Value> {
        Audit::new(&paths.audit_dir())
            .tail(20)
            .unwrap()
            .into_iter()
            .filter(|e| e["event"] == "link.refused")
            .collect()
    }

    #[test]
    fn trust_refuses_the_own_identity_and_a_led_follower_and_audits_both() {
        let (_d, paths, own) = paths_with_identity();
        let mut store = Store::default();
        let err = check_trust(&paths, &store, "me", &own).unwrap_err();
        assert!(err.message.contains("cannot lead itself"));
        let led = testkeys::key();
        store.peer_keys.insert("spark".into(), led.blob.clone());
        let err = check_trust(&paths, &store, "spark-as-leader", &led).unwrap_err();
        assert!(err.message.contains("cycle") && err.message.contains("spark"));
        assert!(check_trust(&paths, &store, "ok", &testkeys::key()).is_ok());
        let log = refusals(&paths);
        assert_eq!(log.len(), 2);
        assert_eq!(log[0]["link"], "me");
        assert_eq!(log[1]["fp"], led.fingerprint);
    }

    #[test]
    fn adding_a_follower_refuses_itself_and_a_leader_of_this_machine() {
        let (_d, paths, own) = paths_with_identity();
        let mut store = Store::default();
        let same = format!("{} tmgr-link", own.openssh());
        assert!(check_follower(&paths, &store, "x", Some(&same))
            .unwrap_err()
            .message
            .contains("own link identity"));
        let boss = testkeys::key();
        store.leaders.insert("boss".into(), leader_with(&boss));
        let err = check_follower(&paths, &store, "x", Some(&boss.openssh())).unwrap_err();
        assert!(err.message.contains("already leads") && err.message.contains("boss"));
        assert_eq!(refusals(&paths).len(), 2);
    }

    #[test]
    fn a_follower_without_an_identity_is_refused_and_a_fresh_key_is_accepted() {
        let (_d, paths, _own) = paths_with_identity();
        let store = Store::default();
        let e = check_follower(&paths, &store, "x", None).unwrap_err();
        assert_eq!(e.exit_code(), 9);
        assert!(e.message.contains("too old or unidentified"));
        let fresh = testkeys::key();
        assert_eq!(
            check_follower(&paths, &store, "x", Some(&fresh.openssh())).unwrap(),
            fresh
        );
        assert!(check_follower(&paths, &store, "x", Some("garbage")).is_err());
        assert_eq!(refusals(&paths).len(), 1);
    }

    #[test]
    fn the_worker_refuses_its_own_identity_and_its_followers_without_one() {
        let (_d, paths, own) = paths_with_identity();
        let mut store = Store::default();
        assert!(worker_refusal(&paths, &store, &own.blob).is_some());
        assert!(worker_refusal(&paths, &store, "other").is_none());
        store.peer_keys.insert("spark".into(), "peer".into());
        assert!(worker_refusal(&paths, &store, "peer").is_some());
        let d = crate::testutil::private_tempdir();
        let bare = Paths::under(d.path());
        assert!(worker_refusal(&bare, &Store::default(), "x").is_none());
        std::fs::create_dir_all(&bare.config).unwrap();
        std::fs::write(bare.public_key(), "junk").unwrap();
        assert!(worker_refusal(&bare, &Store::default(), "x").is_some());
    }
}
