use crate::audit::Audit;
use crate::authkeys;
use crate::grammar::{self, PubKey};
use crate::scope::{canonical_repo, Scope};
use crate::sshd::Verdict;
use crate::store::{Leader, Paths, Store};
use serde_json::json;
use std::path::{Path, PathBuf};
use tmgr_core::error::{Error, Result};

pub const MAX_RUNS_LIMIT: u32 = 64;

#[derive(Debug, Default, Clone)]
pub struct ScopeInput {
    pub repos: Option<Vec<String>>,
    pub agents: Option<Vec<String>>,
    pub max: Option<u32>,
    pub from: Option<Vec<String>>,
}

fn build_scope(base: &Scope, input: &ScopeInput) -> Result<Scope> {
    let mut scope = base.clone();
    if let Some(repos) = &input.repos {
        let mut out = Vec::new();
        for r in repos {
            let canon = canonical_repo(r).map_err(|why| {
                Error::usage(format!("invalid repo {r:?}: {why}"))
                    .hint("pass an existing absolute directory")
            })?;
            if !out.contains(&canon) {
                out.push(canon);
            }
        }
        scope.repos = out;
    }
    if let Some(agents) = &input.agents {
        let mut out: Vec<String> = Vec::new();
        for raw in agents {
            for a in raw.split(',') {
                let a = grammar::agent_name(a.trim())?.to_string();
                if !out.contains(&a) {
                    out.push(a);
                }
            }
        }
        scope.agents = out;
    }
    if let Some(max) = input.max {
        if !(1..=MAX_RUNS_LIMIT).contains(&max) {
            return Err(Error::usage("--max must be between 1 and 64"));
        }
        scope.max = max;
    }
    if let Some(from) = &input.from {
        scope.from = grammar::parse_cidr_list(from)?;
    }
    Ok(scope)
}

#[derive(Debug)]
pub struct TrustInput {
    pub name: String,
    pub pubkey: String,
    pub scope: ScopeInput,
}

#[derive(Debug)]
pub struct TrustPlan {
    pub name: String,
    pub key: PubKey,
    pub scope: Scope,
    pub line: String,
    pub sshd: Verdict,
}

pub fn plan_trust(
    paths: &Paths,
    input: &TrustInput,
    bin: &Path,
    sshd: Verdict,
) -> Result<TrustPlan> {
    let name = grammar::link_name(&input.name)?.to_string();
    let key = grammar::parse_pubkey(&input.pubkey)?;
    let mut scope_input = input.scope.clone();
    scope_input.max.get_or_insert(1);
    let scope = build_scope(&Scope::default(), &scope_input)?;
    if let Verdict::Violations(v) = &sshd {
        return Err(crate::forbidden(format!(
            "sshd configuration is unsafe for a restricted key: {}",
            v.join("; ")
        ))
        .hint("fix sshd_config (PermitUserEnvironment no, no ForceCommand or AuthorizedKeysCommand for this user)"));
    }
    let bin = authkeys::check_binary(bin)?;
    if let Ok(own) = std::fs::read_to_string(paths.public_key()) {
        if grammar::parse_pubkey(&own).is_ok_and(|o| o.blob == key.blob) {
            return Err(Error::conflict(
                "this key is this machine's own link identity; a machine cannot lead itself",
            ));
        }
    }
    let store = Store::load(paths)?;
    if store.leaders.contains_key(&name) {
        return Err(
            Error::conflict(format!("a leader named {name} is already trusted"))
                .hint(format!("tmgr link revoke {name} first")),
        );
    }
    if let Some((other, _)) = store.leaders.iter().find(|(_, l)| l.key == key.blob) {
        return Err(Error::conflict(format!(
            "this key is already trusted as {other}"
        )));
    }
    if authkeys::find_foreign(&paths.authorized_keys(), &key, &name)? {
        return Err(Error::conflict(
            "this public key is already in authorized_keys without tmgr's restrictions",
        )
        .hint("remove that line first, or use a dedicated link key"));
    }
    let line = authkeys::build_line(&bin, &name, &scope.from, &key)?;
    Ok(TrustPlan {
        name,
        key,
        scope,
        line,
        sshd,
    })
}

pub fn apply_trust(paths: &Paths, plan: &TrustPlan) -> Result<authkeys::Edit> {
    Store::update(paths, |s| {
        s.leaders.insert(
            plan.name.clone(),
            Leader {
                key: plan.key.blob.clone(),
                fingerprint: plan.key.fingerprint.clone(),
                added: crate::now_rfc3339(),
                scope: plan.scope.clone(),
            },
        );
        Ok(())
    })?;
    let edit = match authkeys::upsert(&paths.authorized_keys(), &plan.name, &plan.line, &plan.key) {
        Ok(e) => e,
        Err(e) => {
            let _ = Store::update(paths, |s| {
                s.leaders.remove(&plan.name);
                Ok(())
            });
            return Err(e);
        }
    };
    Audit::new(&paths.audit_dir()).append(
        "link.trust",
        json!({
            "link": plan.name,
            "fingerprint": plan.key.fingerprint,
            "scope": plan.scope.summary(),
            "sshd": match &plan.sshd { Verdict::Verified => "verified", _ => "unverified" },
        }),
    )?;
    Ok(edit)
}

#[derive(Debug)]
pub struct ScopePlan {
    pub name: String,
    pub old: Scope,
    pub new: Scope,
    pub widening: bool,
    pub key_blob: String,
}

pub fn plan_scope(paths: &Paths, name: &str, input: &ScopeInput) -> Result<ScopePlan> {
    grammar::link_name(name)?;
    let store = Store::load(paths)?;
    let leader = store.leaders.get(name).ok_or_else(|| {
        Error::not_found(format!("no leader named {name}")).hint("tmgr link list")
    })?;
    let new = build_scope(&leader.scope, input)?;
    Ok(ScopePlan {
        name: name.to_string(),
        widening: new.widens(&leader.scope),
        old: leader.scope.clone(),
        new,
        key_blob: leader.key.clone(),
    })
}

pub fn apply_scope(paths: &Paths, plan: &ScopePlan, bin: &Path) -> Result<()> {
    if plan.old == plan.new {
        return Ok(());
    }
    let from_changed = plan.old.from != plan.new.from;
    let key = grammar::parse_pubkey(&format!("ssh-ed25519 {}", plan.key_blob));
    let line = if from_changed {
        let bin = authkeys::check_binary(bin)?;
        Some(authkeys::build_line(
            &bin,
            &plan.name,
            &plan.new.from,
            key.as_ref().map_err(|e| Error::usage(e.message.clone()))?,
        )?)
    } else {
        None
    };
    Store::update(paths, |s| {
        let leader = s
            .leaders
            .get_mut(&plan.name)
            .ok_or_else(|| Error::not_found(format!("no leader named {}", plan.name)))?;
        leader.scope = plan.new.clone();
        Ok(())
    })?;
    if let Some(line) = line {
        authkeys::upsert(&paths.authorized_keys(), &plan.name, &line, &key?)?;
    }
    Audit::new(&paths.audit_dir()).append(
        "link.scope",
        json!({
            "link": plan.name,
            "widening": plan.widening,
            "from": plan.old.summary(),
            "to": plan.new.summary(),
        }),
    )?;
    Ok(())
}

#[derive(Debug)]
pub struct Revoked {
    pub keys_removed: usize,
    pub backup: Option<PathBuf>,
}

pub fn revoke(paths: &Paths, name: &str) -> Result<Revoked> {
    grammar::link_name(name)?;
    let leader = Store::update(paths, |s| {
        s.leaders.remove(name).ok_or_else(|| {
            Error::not_found(format!("no leader named {name}")).hint("tmgr link list")
        })
    })?;
    let audit = Audit::new(&paths.audit_dir());
    let edit = authkeys::remove(&paths.authorized_keys(), name);
    let audit_result = audit.append(
        "link.revoke",
        json!({
            "link": name,
            "fingerprint": leader.fingerprint,
            "keys_removed": edit.as_ref().map_or(0, |e| e.removed),
            "authorized_keys_error": edit.is_err(),
        }),
    );
    let edit = edit?;
    audit_result?;
    Ok(Revoked {
        keys_removed: edit.removed,
        backup: edit.backup,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::grammar::testkeys;
    use std::os::unix::fs::PermissionsExt;

    struct Fx {
        _d: tempfile::TempDir,
        paths: Paths,
        bin: PathBuf,
        repo: String,
    }

    fn fx() -> Fx {
        let d = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(d.path()).unwrap();
        let paths = Paths::under(&root);
        let bin_dir = root.join("bin");
        std::fs::create_dir(&bin_dir).unwrap();
        std::fs::set_permissions(&bin_dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        let bin = bin_dir.join("tmgr");
        std::fs::write(&bin, "x").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let repo = root.join("repo");
        std::fs::create_dir(&repo).unwrap();
        Fx {
            paths,
            bin,
            repo: repo.to_str().unwrap().to_string(),
            _d: d,
        }
    }

    fn input(f: &Fx, name: &str, key: &str) -> TrustInput {
        TrustInput {
            name: name.into(),
            pubkey: key.into(),
            scope: ScopeInput {
                repos: Some(vec![f.repo.clone()]),
                agents: Some(vec!["claude-code,codex".into()]),
                max: Some(3),
                from: None,
            },
        }
    }

    fn trust(f: &Fx, name: &str) -> (TrustPlan, PubKey) {
        let k = testkeys::generate();
        let plan = plan_trust(&f.paths, &input(f, name, &k), &f.bin, Verdict::Verified).unwrap();
        let key = plan.key.clone();
        apply_trust(&f.paths, &plan).unwrap();
        (plan, key)
    }

    #[test]
    fn trust_writes_store_authorized_keys_and_audit() {
        let f = fx();
        let (plan, key) = trust(&f, "laptop");
        let store = Store::load(&f.paths).unwrap();
        let l = &store.leaders["laptop"];
        assert_eq!(l.key, key.blob);
        assert_eq!(l.scope.max, 3);
        assert_eq!(l.scope.agents, ["claude-code", "codex"]);
        assert_eq!(l.scope.repos.len(), 1);
        assert_eq!(l.scope.repos[0], f.repo);
        let ak = std::fs::read_to_string(f.paths.authorized_keys()).unwrap();
        assert_eq!(ak.trim_end(), plan.line);
        assert!(ak.starts_with(&format!(
            "restrict,command=\"{} worker --link laptop\"",
            f.bin.display()
        )));
        let audit = Audit::new(&f.paths.audit_dir());
        assert_eq!(audit.tail(1).unwrap()[0]["event"], "link.trust");
        assert!(audit.verify().unwrap().is_ok());
    }

    #[test]
    fn duplicates_and_own_identity_are_refused() {
        let f = fx();
        let (_, key) = trust(&f, "laptop");
        let again = plan_trust(
            &f.paths,
            &input(&f, "laptop", &testkeys::generate()),
            &f.bin,
            Verdict::Verified,
        );
        assert_eq!(again.unwrap_err().exit_code(), 6);
        let same_key = plan_trust(
            &f.paths,
            &input(&f, "other", &key.openssh()),
            &f.bin,
            Verdict::Verified,
        );
        assert_eq!(same_key.unwrap_err().exit_code(), 6);
        let own = testkeys::generate();
        std::fs::write(f.paths.public_key(), &own).unwrap();
        let e =
            plan_trust(&f.paths, &input(&f, "me", &own), &f.bin, Verdict::Verified).unwrap_err();
        assert!(e.message.contains("own link identity"));
    }

    #[test]
    fn bad_inputs_are_refused_before_anything_is_written() {
        let f = fx();
        let k = testkeys::generate();
        let mut bad_name = input(&f, "Laptop", &k);
        assert!(plan_trust(&f.paths, &bad_name, &f.bin, Verdict::Verified).is_err());
        bad_name.name = "ok".into();
        bad_name.scope.repos = Some(vec!["/definitely/not/here".into()]);
        assert!(plan_trust(&f.paths, &bad_name, &f.bin, Verdict::Verified).is_err());
        let mut rel = input(&f, "ok", &k);
        rel.scope.repos = Some(vec!["relative".into()]);
        assert!(plan_trust(&f.paths, &rel, &f.bin, Verdict::Verified).is_err());
        let mut from = input(&f, "ok", &k);
        from.scope.from = Some(vec!["not-a-cidr".into()]);
        assert!(plan_trust(&f.paths, &from, &f.bin, Verdict::Verified).is_err());
        let mut max = input(&f, "ok", &k);
        max.scope.max = Some(0);
        assert!(plan_trust(&f.paths, &max, &f.bin, Verdict::Verified).is_err());
        let rsa = input(&f, "ok", "ssh-rsa AAAA");
        assert!(plan_trust(&f.paths, &rsa, &f.bin, Verdict::Verified).is_err());
        assert!(!f.paths.links_toml().exists());
        assert!(!f.paths.authorized_keys().exists());
    }

    #[test]
    fn unsafe_sshd_fails_closed_and_unverified_is_carried() {
        let f = fx();
        let k = testkeys::generate();
        let v = Verdict::Violations(vec!["PermitUserEnvironment is \"yes\"".into()]);
        let e = plan_trust(&f.paths, &input(&f, "a", &k), &f.bin, v).unwrap_err();
        assert_eq!(e.exit_code(), 5);
        let p = plan_trust(
            &f.paths,
            &input(&f, "a", &k),
            &f.bin,
            Verdict::Unverified("x".into()),
        )
        .unwrap();
        assert!(matches!(p.sshd, Verdict::Unverified(_)));
    }

    #[test]
    fn a_foreign_authorized_key_blocks_trust() {
        let f = fx();
        let k = testkeys::generate();
        let key = grammar::parse_pubkey(&k).unwrap();
        std::fs::create_dir_all(f.paths.authorized_keys().parent().unwrap()).unwrap();
        std::fs::write(f.paths.authorized_keys(), format!("{}\n", key.openssh())).unwrap();
        let e = plan_trust(&f.paths, &input(&f, "a", &k), &f.bin, Verdict::Verified).unwrap_err();
        assert_eq!(e.exit_code(), 6);
    }

    #[test]
    fn narrowing_needs_no_confirmation_and_widening_is_flagged() {
        let f = fx();
        trust(&f, "laptop");
        let narrow = plan_scope(
            &f.paths,
            "laptop",
            &ScopeInput {
                max: Some(1),
                agents: Some(vec!["codex".into()]),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!narrow.widening);
        apply_scope(&f.paths, &narrow, &f.bin).unwrap();
        assert_eq!(
            Store::load(&f.paths).unwrap().leaders["laptop"].scope.max,
            1
        );
        let wide = plan_scope(
            &f.paths,
            "laptop",
            &ScopeInput {
                max: Some(5),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(wide.widening);
        let more_agents = plan_scope(
            &f.paths,
            "laptop",
            &ScopeInput {
                agents: Some(vec!["codex".into(), "aider".into()]),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(more_agents.widening);
        let other_repo = tempfile::tempdir().unwrap();
        let repo2 = plan_scope(
            &f.paths,
            "laptop",
            &ScopeInput {
                repos: Some(vec![other_repo.path().to_str().unwrap().into()]),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(repo2.widening);
        assert_eq!(
            plan_scope(&f.paths, "nope", &ScopeInput::default())
                .unwrap_err()
                .exit_code(),
            3
        );
    }

    #[test]
    fn changing_from_rewrites_the_authorized_keys_line() {
        let f = fx();
        trust(&f, "laptop");
        let plan = plan_scope(
            &f.paths,
            "laptop",
            &ScopeInput {
                from: Some(vec!["192.0.2.0/24".into()]),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!plan.widening);
        apply_scope(&f.paths, &plan, &f.bin).unwrap();
        let ak = std::fs::read_to_string(f.paths.authorized_keys()).unwrap();
        assert!(ak.contains("from=\"192.0.2.0/24\""));
        assert_eq!(ak.lines().count(), 1);
        let drop = plan_scope(
            &f.paths,
            "laptop",
            &ScopeInput {
                from: Some(vec![]),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(drop.widening);
    }

    #[test]
    fn revoke_removes_everything_and_keeps_the_audit_log() {
        let f = fx();
        let (_, key) = trust(&f, "laptop");
        trust(&f, "second");
        let r = revoke(&f.paths, "laptop").unwrap();
        assert_eq!(r.keys_removed, 1);
        let store = Store::load(&f.paths).unwrap();
        assert!(!store.leaders.contains_key("laptop") && store.leaders.contains_key("second"));
        let ak = std::fs::read_to_string(f.paths.authorized_keys()).unwrap();
        assert!(!ak.contains(&key.blob) && ak.contains("tmgr-link:second"));
        let audit = Audit::new(&f.paths.audit_dir());
        let events: Vec<_> = audit
            .tail(10)
            .unwrap()
            .iter()
            .map(|e| e["event"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(events, ["link.trust", "link.trust", "link.revoke"]);
        assert!(audit.verify().unwrap().is_ok());
        assert_eq!(revoke(&f.paths, "laptop").unwrap_err().exit_code(), 3);
    }

    #[test]
    fn revoke_works_even_when_authorized_keys_is_gone() {
        let f = fx();
        trust(&f, "laptop");
        std::fs::remove_file(f.paths.authorized_keys()).unwrap();
        assert_eq!(revoke(&f.paths, "laptop").unwrap().keys_removed, 0);
        assert!(Store::load(&f.paths).unwrap().leaders.is_empty());
    }
}
