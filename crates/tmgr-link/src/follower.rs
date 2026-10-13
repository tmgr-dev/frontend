use crate::audit::Audit;
use crate::authkeys;
use crate::grammar::{self, PubKey};
use crate::scope::{canonical_repo, Scope};
use crate::sshd::{Check, Verdict};
use crate::store::{Leader, Paths, Store};
use serde_json::json;
use std::path::{Path, PathBuf};
use tmgr_core::error::{Error, Result};

pub const MAX_RUNS_LIMIT: u32 = 64;

#[derive(Debug, Default, Clone)]
pub struct ScopeInput {
    pub repos: Option<Vec<String>>,
    pub agents: Option<Vec<String>>,
    pub models: Option<Vec<String>>,
    pub max: Option<u32>,
    pub from: Option<Vec<String>>,
    pub allow_attach_input: Option<bool>,
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
    if let Some(models) = &input.models {
        let mut out: Vec<String> = Vec::new();
        for raw in models {
            for m in raw.split(',') {
                if !grammar::model_name(m.trim()) {
                    return Err(Error::usage(format!("invalid model name: {m:?}")));
                }
                if !out.iter().any(|x| x == m.trim()) {
                    out.push(m.trim().to_string());
                }
            }
        }
        scope.models = out;
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
    if let Some(allow) = input.allow_attach_input {
        scope.allow_attach_input = allow;
    }
    Ok(scope)
}

pub fn new_enrollment() -> String {
    use rand_core::RngCore;
    let mut bytes = [0u8; 16];
    rand_core::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
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
    pub enrollment: String,
    pub sshd: Verdict,
    pub target: PathBuf,
    pub store_version: u64,
}

pub fn plan_trust(
    paths: &Paths,
    input: &TrustInput,
    bin: &Path,
    check: Check,
) -> Result<TrustPlan> {
    let name = grammar::link_name(&input.name)?.to_string();
    let key = grammar::parse_pubkey(&input.pubkey)?;
    let mut scope_input = input.scope.clone();
    scope_input.max.get_or_insert(1);
    let scope = build_scope(&Scope::default(), &scope_input)?;
    let Check {
        verdict,
        authorized_keys_files,
    } = check;
    if let Verdict::Violations(v) = &verdict {
        return Err(crate::forbidden(format!(
            "sshd configuration is unsafe for a restricted key: {}",
            v.join("; ")
        ))
        .hint("fix sshd_config (PermitUserEnvironment no, PermitTunnel no, a minimal AcceptEnv, no ForceCommand or AuthorizedKeysCommand for this user)"));
    }
    let target = paths.authorized_keys();
    if matches!(verdict, Verdict::Verified) && !authorized_keys_files.contains(&target) {
        return Err(crate::forbidden(format!(
            "sshd does not read {} (AuthorizedKeysFile points elsewhere)",
            target.display()
        )));
    }
    let bin = authkeys::check_binary(bin)?;
    if let Ok(own) = std::fs::read_to_string(paths.public_key()) {
        if grammar::parse_pubkey(&own).is_ok_and(|o| o.blob == key.blob) {
            return Err(Error::conflict(
                "this key is this machine's own link identity; a machine cannot lead itself",
            ));
        }
    }
    if is_pending(paths, &name) {
        return Err(Error::conflict(format!(
            "a previous change of {name} could not be rolled back"
        ))
        .hint(format!("tmgr link revoke {name}")));
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
    let mut inspect = authorized_keys_files;
    if !inspect.contains(&target) {
        inspect.push(target.clone());
    }
    if let Some(file) = authkeys::find_foreign(&inspect, &key, &name)? {
        return Err(Error::conflict(format!(
            "this public key is already in {} without tmgr's restrictions",
            file.display()
        ))
        .hint("remove that line first, or use a dedicated link key"));
    }
    let enrollment = new_enrollment();
    let line = authkeys::build_line(&bin, &name, &enrollment, &scope.from, &key)?;
    Ok(TrustPlan {
        name,
        key,
        scope,
        line,
        enrollment,
        sshd: verdict,
        target,
        store_version: store.version,
    })
}

pub fn is_pending(paths: &Paths, name: &str) -> bool {
    paths.pending_marker(name).exists()
}

fn mark_pending(paths: &Paths, name: &str) -> Result<()> {
    let marker = paths.pending_marker(name);
    if let Some(dir) = marker.parent() {
        tmgr_core::config::ensure_private_dir(dir)?;
    }
    tmgr_core::config::write_atomic(&marker, b"pending\n", 0o600)
}

fn rollback(
    paths: &Paths,
    name: &str,
    edit: Option<&authkeys::Edit>,
    store_reverted: bool,
) -> Result<()> {
    let restored = match edit {
        Some(e) => authkeys::restore(e),
        None => Ok(()),
    };
    if restored.is_ok() && store_reverted {
        return Ok(());
    }
    let marked = mark_pending(paths, name);
    let note = if marked.is_ok() {
        "the link is blocked until it is revoked"
    } else {
        "the link could not even be marked as blocked"
    };
    Err(
        Error::internal(format!("rollback failed for {name}: {note}"))
            .hint(format!("tmgr link revoke {name}")),
    )
}

fn changed_meanwhile() -> Error {
    Error::conflict("links.toml changed while you were confirming").hint("run the command again")
}

pub fn apply_trust(paths: &Paths, plan: &TrustPlan) -> Result<authkeys::Edit> {
    let _lock = crate::store::lock_links(paths)?;
    let mut store = Store::load(paths)?;
    if store.version != plan.store_version {
        return Err(changed_meanwhile());
    }
    let audit = Audit::new(&paths.audit_dir());
    audit.append(
        "link.trust.intent",
        json!({
            "link": plan.name,
            "fingerprint": plan.key.fingerprint,
            "scope": plan.scope.summary(),
        }),
    )?;
    let edit = authkeys::upsert(&plan.target, &plan.name, &plan.line, &plan.key)?;
    store.bump_generation(&plan.name);
    store.leaders.insert(
        plan.name.clone(),
        Leader {
            key: plan.key.blob.clone(),
            fingerprint: plan.key.fingerprint.clone(),
            added: crate::now_rfc3339(),
            scope: plan.scope.clone(),
            enrollment: plan.enrollment.clone(),
            authorized_keys: plan.target.display().to_string(),
        },
    );
    if let Err(e) = store.save(paths) {
        let rolled = rollback(paths, &plan.name, Some(&edit), true);
        let _ = audit.append("link.trust.aborted", json!({"link": plan.name}));
        return Err(rolled.err().unwrap_or(e));
    }
    let done = audit.append(
        "link.trust",
        json!({
            "link": plan.name,
            "fingerprint": plan.key.fingerprint,
            "scope": plan.scope.summary(),
            "enrollment": plan.enrollment,
            "sshd": match &plan.sshd { Verdict::Verified => "verified", _ => "unverified" },
        }),
    );
    if let Err(e) = done {
        store.leaders.remove(&plan.name);
        let saved = store.save(paths);
        let rolled = rollback(paths, &plan.name, Some(&edit), saved.is_ok());
        return Err(rolled.err().unwrap_or(e));
    }
    Ok(edit)
}

#[derive(Debug)]
pub struct ScopePlan {
    pub name: String,
    pub old: Scope,
    pub new: Scope,
    pub widening: bool,
    pub key_blob: String,
    pub enrollment: String,
    pub authorized_keys: PathBuf,
    pub store_version: u64,
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
        enrollment: leader.enrollment.clone(),
        authorized_keys: leader_keys_path(paths, leader),
        store_version: store.version,
    })
}

fn leader_keys_path(paths: &Paths, leader: &Leader) -> PathBuf {
    if leader.authorized_keys.is_empty() {
        paths.authorized_keys()
    } else {
        PathBuf::from(&leader.authorized_keys)
    }
}

pub fn apply_scope(paths: &Paths, plan: &ScopePlan, bin: &Path) -> Result<()> {
    if plan.old == plan.new {
        return Ok(());
    }
    let _lock = crate::store::lock_links(paths)?;
    let mut store = Store::load(paths)?;
    if store.version != plan.store_version {
        return Err(changed_meanwhile());
    }
    let audit = Audit::new(&paths.audit_dir());
    let fields = json!({
        "link": plan.name,
        "widening": plan.widening,
        "from": plan.old.summary(),
        "to": plan.new.summary(),
    });
    if plan.widening {
        audit.append("link.scope.intent", fields.clone())?;
    }
    let edit = if plan.old.from != plan.new.from {
        let key = grammar::parse_pubkey(&format!("ssh-ed25519 {}", plan.key_blob))?;
        let bin = authkeys::check_binary(bin)?;
        let line = authkeys::build_line(&bin, &plan.name, &plan.enrollment, &plan.new.from, &key)?;
        Some(authkeys::upsert(
            &plan.authorized_keys,
            &plan.name,
            &line,
            &key,
        )?)
    } else {
        None
    };
    let Some(leader) = store.leaders.get_mut(&plan.name) else {
        rollback(paths, &plan.name, edit.as_ref(), true)?;
        return Err(Error::not_found(format!("no leader named {}", plan.name)));
    };
    leader.scope = plan.new.clone();
    store.bump_generation(&plan.name);
    if let Err(e) = store.save(paths) {
        let rolled = rollback(paths, &plan.name, edit.as_ref(), true);
        return Err(rolled.err().unwrap_or(e));
    }
    let done = audit.append("link.scope", fields);
    if plan.widening {
        if let Err(e) = done {
            if let Some(l) = store.leaders.get_mut(&plan.name) {
                l.scope = plan.old.clone();
            }
            store.bump_generation(&plan.name);
            let saved = store.save(paths);
            let rolled = rollback(paths, &plan.name, edit.as_ref(), saved.is_ok());
            return Err(rolled.err().unwrap_or(e));
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct Revoked {
    pub keys_removed: usize,
    pub backup: Option<PathBuf>,
}

pub fn revoke(paths: &Paths, name: &str) -> Result<Revoked> {
    grammar::link_name(name)?;
    let _lock = crate::store::lock_links(paths)?;
    let mut store = Store::load(paths)?;
    let pending = is_pending(paths, name);
    store.generations.remove(name);
    let leader = match store.leaders.remove(name) {
        Some(l) => Some(l),
        None if pending => None,
        None => {
            return Err(Error::not_found(format!("no leader named {name}")).hint("tmgr link list"))
        }
    };
    let saved = if leader.is_some() {
        store.save(paths)
    } else {
        Ok(())
    };
    let audit = Audit::new(&paths.audit_dir());
    let keys_path = leader
        .as_ref()
        .map_or_else(|| paths.authorized_keys(), |l| leader_keys_path(paths, l));
    let edit = authkeys::remove(&keys_path, name);
    if saved.is_ok() && edit.is_ok() {
        let _ = std::fs::remove_file(paths.pending_marker(name));
    }
    let audited = audit.append(
        "link.revoke",
        json!({
            "link": name,
            "fingerprint": leader.as_ref().map(|l| l.fingerprint.clone()),
            "keys_removed": edit.as_ref().map_or(0, |e| e.removed),
            "authorized_keys_error": edit.is_err(),
            "store_error": saved.is_err(),
            "pending_cleanup": pending,
        }),
    );
    saved?;
    let edit = edit?;
    audited?;
    Ok(Revoked {
        keys_removed: edit.removed,
        backup: edit.backup,
    })
}

#[derive(Debug, Default)]
pub struct Contained {
    pub workers: crate::registry::Terminated,
    pub runs_halted: usize,
    pub runs_known: usize,
    pub halt_failed: usize,
    pub runs_unreadable: bool,
    pub pending_remaining: usize,
    pub pending_cleared: usize,
    pub unknown_origin: Vec<String>,
}

impl Contained {
    pub fn incomplete(&self) -> bool {
        self.halt_failed > 0
            || self.workers.survivors > 0
            || self.pending_remaining > 0
            || self.runs_unreadable
    }
}

pub fn contain(
    paths: &Paths,
    name: &str,
    backend: Option<&mut dyn crate::runner::Backend>,
    halt: bool,
) -> Result<Contained> {
    grammar::link_name(name)?;
    let mut runs = crate::ownership::Runs::lock_lenient(paths)?;
    let workers = crate::registry::terminate(paths, name, std::time::Duration::from_secs(3))?;
    let records: Vec<String> = runs
        .of_link(name)
        .into_iter()
        .map(|r| r.run.clone())
        .collect();
    let mut out = Contained {
        workers,
        runs_known: records.len(),
        runs_unreadable: runs.is_damaged(),
        ..Contained::default()
    };
    out.pending_remaining = runs.pending_of(name).len();
    if let Some(backend) = backend.filter(|_| !out.runs_unreadable) {
        if let Ok(r) = crate::ownership::reconcile_pending(&mut runs, name, backend) {
            out.pending_remaining = r.remaining;
            out.pending_cleared = r.cleared;
            out.unknown_origin = r.unknown_origin;
        }
        if halt {
            for run in &records {
                match crate::runs::stop_run(backend, run) {
                    Ok("already") => {}
                    Ok(_) => out.runs_halted += 1,
                    Err(_) => out.halt_failed += 1,
                }
            }
        }
    }
    Audit::new(&paths.audit_dir()).append(
        "link.revoke.contain",
        json!({
            "link": name,
            "workers_signalled": out.workers.signalled,
            "workers_stale": out.workers.stale,
            "workers_surviving": out.workers.survivors,
            "runs_known": out.runs_known,
            "runs_halted": out.runs_halted,
            "halt_failed": out.halt_failed,
            "runs_unreadable": out.runs_unreadable,
            "pending_remaining": out.pending_remaining,
            "pending_cleared": out.pending_cleared,
            "unknown_origin": out.unknown_origin,
        }),
    )?;
    Ok(out)
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

    fn chk_with(f: &Fx, verdict: Verdict) -> Check {
        Check {
            verdict,
            authorized_keys_files: vec![f.paths.authorized_keys()],
        }
    }

    fn chk(f: &Fx) -> Check {
        chk_with(f, Verdict::Verified)
    }

    fn input(f: &Fx, name: &str, key: &str) -> TrustInput {
        TrustInput {
            name: name.into(),
            pubkey: key.into(),
            scope: ScopeInput {
                repos: Some(vec![f.repo.clone()]),
                agents: Some(vec!["claude-code,codex".into()]),
                models: None,
                max: Some(3),
                from: None,
                allow_attach_input: None,
            },
        }
    }

    fn trust(f: &Fx, name: &str) -> (TrustPlan, PubKey) {
        let k = testkeys::generate();
        let plan = plan_trust(&f.paths, &input(f, name, &k), &f.bin, chk(f)).unwrap();
        let key = plan.key.clone();
        apply_trust(&f.paths, &plan).unwrap();
        (plan, key)
    }

    #[test]
    fn generations_change_on_trust_and_scope_changes_and_are_never_reused() {
        let f = fx();
        trust(&f, "laptop");
        trust(&f, "second");
        let gen = |n: &str| Store::load(&f.paths).unwrap().generation(n);
        let (g1, g2) = (gen("laptop"), gen("second"));
        assert!(g1 > 0 && g2 > g1);
        let narrow = plan_scope(
            &f.paths,
            "laptop",
            &ScopeInput {
                max: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        apply_scope(&f.paths, &narrow, &f.bin).unwrap();
        assert!(gen("laptop") > g2);
        assert_eq!(gen("second"), g2);
        let before = gen("laptop");
        revoke(&f.paths, "laptop").unwrap();
        assert_eq!(gen("laptop"), 0);
        trust(&f, "laptop");
        assert!(gen("laptop") > before);
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
            "restrict,no-pty,no-port-forwarding,no-agent-forwarding,no-X11-forwarding,no-user-rc,command=\"{} worker --link laptop --enrollment {}\"",
            f.bin.display(),
            plan.enrollment
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
            chk(&f),
        );
        assert_eq!(again.unwrap_err().exit_code(), 6);
        let same_key = plan_trust(
            &f.paths,
            &input(&f, "other", &key.openssh()),
            &f.bin,
            chk(&f),
        );
        assert_eq!(same_key.unwrap_err().exit_code(), 6);
        let own = testkeys::generate();
        std::fs::write(f.paths.public_key(), &own).unwrap();
        let e = plan_trust(&f.paths, &input(&f, "me", &own), &f.bin, chk(&f)).unwrap_err();
        assert!(e.message.contains("own link identity"));
    }

    #[test]
    fn bad_inputs_are_refused_before_anything_is_written() {
        let f = fx();
        let k = testkeys::generate();
        let mut bad_name = input(&f, "Laptop", &k);
        assert!(plan_trust(&f.paths, &bad_name, &f.bin, chk(&f)).is_err());
        bad_name.name = "ok".into();
        bad_name.scope.repos = Some(vec!["/definitely/not/here".into()]);
        assert!(plan_trust(&f.paths, &bad_name, &f.bin, chk(&f)).is_err());
        let mut rel = input(&f, "ok", &k);
        rel.scope.repos = Some(vec!["relative".into()]);
        assert!(plan_trust(&f.paths, &rel, &f.bin, chk(&f)).is_err());
        let mut from = input(&f, "ok", &k);
        from.scope.from = Some(vec!["not-a-cidr".into()]);
        assert!(plan_trust(&f.paths, &from, &f.bin, chk(&f)).is_err());
        let mut max = input(&f, "ok", &k);
        max.scope.max = Some(0);
        assert!(plan_trust(&f.paths, &max, &f.bin, chk(&f)).is_err());
        let rsa = input(&f, "ok", "ssh-rsa AAAA");
        assert!(plan_trust(&f.paths, &rsa, &f.bin, chk(&f)).is_err());
        assert!(!f.paths.links_toml().exists());
        assert!(!f.paths.authorized_keys().exists());
    }

    #[test]
    fn unsafe_sshd_fails_closed_and_unverified_is_carried() {
        let f = fx();
        let k = testkeys::generate();
        let v = Verdict::Violations(vec!["PermitUserEnvironment is \"yes\"".into()]);
        let e = plan_trust(&f.paths, &input(&f, "a", &k), &f.bin, chk_with(&f, v)).unwrap_err();
        assert_eq!(e.exit_code(), 5);
        let p = plan_trust(
            &f.paths,
            &input(&f, "a", &k),
            &f.bin,
            chk_with(&f, Verdict::Unverified("x".into())),
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
        let e = plan_trust(&f.paths, &input(&f, "a", &k), &f.bin, chk(&f)).unwrap_err();
        assert_eq!(e.exit_code(), 6);
    }

    #[test]
    fn every_effective_authorized_keys_file_is_inspected() {
        let f = fx();
        let k = testkeys::generate();
        let key = grammar::parse_pubkey(&k).unwrap();
        let other = f.paths.home.join("elsewhere_keys");
        std::fs::create_dir_all(&f.paths.home).unwrap();
        std::fs::write(&other, format!("{}\n", key.openssh())).unwrap();
        let check = Check {
            verdict: Verdict::Verified,
            authorized_keys_files: vec![f.paths.authorized_keys(), other.clone()],
        };
        let e = plan_trust(&f.paths, &input(&f, "a", &k), &f.bin, check).unwrap_err();
        assert_eq!(e.exit_code(), 6);
        assert!(e.message.contains("elsewhere_keys"));
    }

    #[test]
    fn a_verified_sshd_that_does_not_read_our_file_is_refused() {
        let f = fx();
        let check = Check {
            verdict: Verdict::Verified,
            authorized_keys_files: vec![f.paths.home.join("other_keys")],
        };
        let e = plan_trust(
            &f.paths,
            &input(&f, "a", &testkeys::generate()),
            &f.bin,
            check,
        )
        .unwrap_err();
        assert_eq!(e.exit_code(), 5);
    }

    #[test]
    fn the_enrollment_id_is_in_the_forced_command_and_the_store() {
        let f = fx();
        let (plan, _) = trust(&f, "laptop");
        assert!(grammar::enrollment_id(&plan.enrollment));
        assert!(plan.line.contains(&format!(
            "worker --link laptop --enrollment {}",
            plan.enrollment
        )));
        let l = &Store::load(&f.paths).unwrap().leaders["laptop"];
        assert_eq!(l.enrollment, plan.enrollment);
        assert_eq!(
            l.authorized_keys,
            f.paths.authorized_keys().display().to_string()
        );
        revoke(&f.paths, "laptop").unwrap();
        let (again, _) = trust(&f, "laptop");
        assert_ne!(again.enrollment, plan.enrollment);
    }

    #[test]
    fn a_stale_plan_is_refused_under_the_lock() {
        let f = fx();
        trust(&f, "laptop");
        let widen = plan_scope(
            &f.paths,
            "laptop",
            &ScopeInput {
                max: Some(9),
                ..Default::default()
            },
        )
        .unwrap();
        assert!(widen.widening);
        let narrow = plan_scope(
            &f.paths,
            "laptop",
            &ScopeInput {
                max: Some(1),
                ..Default::default()
            },
        )
        .unwrap();
        apply_scope(&f.paths, &narrow, &f.bin).unwrap();
        let e = apply_scope(&f.paths, &widen, &f.bin).unwrap_err();
        assert_eq!(e.exit_code(), 6);
        assert_eq!(
            Store::load(&f.paths).unwrap().leaders["laptop"].scope.max,
            1
        );
    }

    #[test]
    fn a_failing_intent_audit_blocks_trust_and_widening_but_not_revoke() {
        let f = fx();
        trust(&f, "laptop");
        let widen = plan_scope(
            &f.paths,
            "laptop",
            &ScopeInput {
                max: Some(9),
                ..Default::default()
            },
        )
        .unwrap();
        let k = testkeys::generate();
        let plan = plan_trust(&f.paths, &input(&f, "second", &k), &f.bin, chk(&f)).unwrap();
        let dir = f.paths.audit_dir();
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::write(&dir, "not a directory").unwrap();
        let before = std::fs::read_to_string(f.paths.authorized_keys()).unwrap();
        assert!(apply_trust(&f.paths, &plan).is_err());
        assert!(apply_scope(&f.paths, &widen, &f.bin).is_err());
        assert_eq!(
            std::fs::read_to_string(f.paths.authorized_keys()).unwrap(),
            before
        );
        let store = Store::load(&f.paths).unwrap();
        assert!(!store.leaders.contains_key("second"));
        assert_eq!(store.leaders["laptop"].scope.max, 3);
        let err = revoke(&f.paths, "laptop").unwrap_err();
        assert!(err.message.contains("I/O") || !err.message.is_empty());
        let after = Store::load(&f.paths).unwrap();
        assert!(!after.leaders.contains_key("laptop"));
        assert!(!std::fs::read_to_string(f.paths.authorized_keys())
            .unwrap()
            .contains("tmgr-link:laptop"));
    }

    #[test]
    fn trust_rolls_back_authorized_keys_when_the_store_cannot_be_written() {
        let f = fx();
        Store::update(&f.paths, |_| Ok(())).unwrap();
        std::fs::create_dir_all(f.paths.authorized_keys().parent().unwrap()).unwrap();
        std::fs::write(f.paths.authorized_keys(), "ssh-rsa AAAA keep\n").unwrap();
        let k = testkeys::generate();
        let plan = plan_trust(&f.paths, &input(&f, "a", &k), &f.bin, chk(&f)).unwrap();
        let blocker = f
            .paths
            .config
            .join(format!(".links.toml.{}.tmp", std::process::id()));
        std::fs::create_dir(&blocker).unwrap();
        assert!(apply_trust(&f.paths, &plan).is_err());
        assert_eq!(
            std::fs::read_to_string(f.paths.authorized_keys()).unwrap(),
            "ssh-rsa AAAA keep\n"
        );
        assert!(!Store::load(&f.paths).unwrap().leaders.contains_key("a"));
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
        assert_eq!(
            events,
            [
                "link.trust.intent",
                "link.trust",
                "link.trust.intent",
                "link.trust",
                "link.revoke"
            ]
        );
        assert!(audit.verify().unwrap().is_ok());
        assert_eq!(revoke(&f.paths, "laptop").unwrap_err().exit_code(), 3);
    }

    #[test]
    fn a_failed_rollback_blocks_the_link_until_it_is_revoked() {
        let f = fx();
        let (_, key) = trust(&f, "laptop");
        let gone = authkeys::Edit {
            changed: true,
            existed: true,
            original: b"x".to_vec(),
            mode: Some(0o600),
            path: f.paths.home.join("no-such-dir").join("authorized_keys"),
            ..Default::default()
        };
        let e = rollback(&f.paths, "laptop", Some(&gone), true).unwrap_err();
        assert_eq!(e.exit_code(), 1);
        assert_eq!(e.hint.as_deref(), Some("tmgr link revoke laptop"));
        assert!(is_pending(&f.paths, "laptop"));
        let again = plan_trust(
            &f.paths,
            &input(&f, "laptop", &key.openssh()),
            &f.bin,
            chk(&f),
        )
        .unwrap_err();
        assert_eq!(again.exit_code(), 6);
        revoke(&f.paths, "laptop").unwrap();
        assert!(!is_pending(&f.paths, "laptop"));
        assert!(rollback(&f.paths, "laptop", None, true).is_ok());
        assert!(!is_pending(&f.paths, "laptop"));
        assert!(rollback(&f.paths, "laptop", None, false).is_err());
        assert!(is_pending(&f.paths, "laptop"));
        let r = revoke(&f.paths, "laptop").unwrap();
        assert_eq!(r.keys_removed, 0);
        assert!(!is_pending(&f.paths, "laptop"));
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
