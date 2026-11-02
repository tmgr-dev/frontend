use crate::cli_link::{
    AddArgs, AuditArgs, InitArgs, LinkCommand, RevokeArgs, ScopeArgs, Switch, TrustArgs,
};
use crate::ctx::Ctx;
use crate::out::{table, Output};
use serde_json::{json, Value};
use std::time::Instant;
use tmgr_core::creds::Backend;
use tmgr_core::error::{Error, Result};
use tmgr_core::text::sanitize_line;
use tmgr_link::audit::Audit;
use tmgr_link::cycle;
use tmgr_link::follower::{self, ScopeInput, TrustInput};
use tmgr_link::grammar;
use tmgr_link::jump;
use tmgr_link::leader::{self, Client, SshTarget};
use tmgr_link::privileged;
use tmgr_link::registry::{self, Terminated};
use tmgr_link::scope::Scope;
use tmgr_link::sshd::{self, Verdict};
use tmgr_link::store::{Follower, Paths, Store};

pub fn run(ctx: &Ctx, cmd: &LinkCommand) -> Result<Output> {
    let paths = Paths::from_env()?;
    match cmd {
        LinkCommand::Init(a) => init(ctx, &paths, a),
        LinkCommand::Add(a) => add(ctx, &paths, a),
        LinkCommand::Trust(a) => trust(&paths, a),
        LinkCommand::List => list(ctx, &paths),
        LinkCommand::Scope(a) => scope(ctx, &paths, a),
        LinkCommand::Revoke(a) => revoke(&paths, a),
        LinkCommand::Remove { name } => remove(&paths, name),
        LinkCommand::Test { name } => test(ctx, &paths, name),
        LinkCommand::Audit(a) => audit(ctx, &paths, a),
    }
}

fn cred(ctx: &Ctx, paths: &Paths) -> tmgr_core::creds::CredentialStore {
    let preferred = ctx.cfg.credential_store.as_deref().and_then(Backend::parse);
    leader::credential_store(paths, preferred)
}

fn short_fp(fp: &str) -> String {
    let chars: Vec<char> = fp.chars().collect();
    if chars.len() <= 20 {
        return fp.to_string();
    }
    let head: String = chars[..14].iter().collect();
    let tail: String = chars[chars.len() - 4..].iter().collect();
    format!("{head}…{tail}")
}

fn init(ctx: &Ctx, paths: &Paths, a: &InitArgs) -> Result<Output> {
    privileged::guard()?;
    let store = cred(ctx, paths);
    if a.rotate && leader::has_identity(paths, &store)? {
        eprintln!("This replaces the link identity: every follower must trust the new key, the old one stops working.");
        privileged::confirm_typed_stdio("rotate", "Type rotate to continue:")?;
    }
    let id = leader::init(paths, &store, a.rotate)?;
    if let Some(w) = &id.warning {
        ctx.warn("credential_store_file", w.clone());
    }
    let human = format!(
        "{} link identity created (private key in the {})\n  fingerprint {}\n  public key  {}\nOn the follower: tmgr link trust {} --as leader --name <NAME> --repos <DIR> --agents <AGENTS>",
        ctx.style.good("✓"),
        id.backend.label(),
        id.fingerprint,
        id.public,
        shell_quote(&id.public),
    );
    Ok(Output::new(
        json!({"public_key": id.public, "fingerprint": id.fingerprint, "backend": id.backend.as_str()}),
        human,
    ))
}

fn shell_quote(s: &str) -> String {
    format!("'{s}'")
}

fn add(ctx: &Ctx, paths: &Paths, a: &AddArgs) -> Result<Output> {
    privileged::guard()?;
    let name = grammar::link_name(&a.name)?.to_string();
    let target = grammar::parse_target(&a.ssh)?;
    let port = grammar::parse_port(a.port)?;
    if leader::identity(paths)?.is_none() {
        return Err(Error::not_found("this machine has no link identity").hint("tmgr link init"));
    }
    let store = Store::load(paths)?;
    if store.followers.contains_key(&name) {
        return Err(
            Error::conflict(format!("a follower named {name} already exists"))
                .hint(format!("tmgr link remove {name} first")),
        );
    }
    let hop = a
        .jump
        .as_deref()
        .map(|j| jump::hop_for(&store, j))
        .transpose()?;
    let key = match &hop {
        Some(h) => jump::scan_host_via(paths, &cred(ctx, paths), h, &target.host, port)?,
        None => leader::scan_host(&target.host, port)?,
    };
    let via = hop
        .as_ref()
        .map(|h| format!(" (seen through the pinned jump link {})", h.name))
        .unwrap_or_default();
    eprintln!(
        "host key of {}:{port}{via}\n  {}\nVerify this fingerprint with the follower's owner (ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub on it).",
        target.host, key.fingerprint
    );
    privileged::confirm_typed_stdio(&name, &format!("Type {name} to pin this host key:"))?;
    leader::pin_host(paths, &name, &key)?;
    let ssh = SshTarget {
        user: target.user.clone(),
        host: target.host.clone(),
        port,
    };
    let hello = (|| -> Result<Value> {
        let (mut client, _key_file) =
            Client::connect_via(paths, &cred(ctx, paths), &name, &ssh, hop.as_ref())?;
        client.hello()
    })();
    let hello = match hello {
        Ok(h) => h,
        Err(e) => {
            let _ = leader::unpin_host(paths, &name);
            return Err(e);
        }
    };
    let peer = match cycle::check_follower(
        paths,
        &Store::load(paths)?,
        &name,
        hello.get("link_key").and_then(Value::as_str),
    ) {
        Ok(p) => p,
        Err(e) => {
            let _ = leader::unpin_host(paths, &name);
            return Err(e);
        }
    };
    let registered = Store::update(paths, |s| {
        if let Some(h) = &hop {
            if !s.followers.contains_key(&h.name) || s.jumps.contains_key(&h.name) {
                return Err(Error::conflict("the jump link changed while adding"));
            }
            s.jumps.insert(name.clone(), h.name.clone());
        }
        {
            s.peer_keys.insert(name.clone(), peer.blob.clone());
        }
        s.followers.insert(
            name.clone(),
            Follower {
                user: target.user.clone(),
                host: target.host.clone(),
                port,
                host_key_fingerprint: key.fingerprint.clone(),
                added: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            },
        );
        Ok(())
    });
    if let Err(e) = registered {
        let _ = leader::unpin_host(paths, &name);
        return Err(e);
    }
    Audit::new(&paths.audit_dir()).append(
        "link.add",
        json!({"link": name, "host": target.host, "port": port, "fingerprint": key.fingerprint, "jump": hop.as_ref().map(|h| h.name.clone())}),
    )?;
    let version = hello.get("version").and_then(Value::as_str).unwrap_or("?");
    Ok(Output::new(
        json!({"link": name, "fingerprint": key.fingerprint, "follower_version": sanitize_line(version)}),
        format!(
            "{} {name} added: host key pinned ({}), handshake ok (follower tmgr {})",
            ctx.style.good("✓"),
            short_fp(&key.fingerprint),
            sanitize_line(version)
        ),
    ))
}

fn scope_json(s: &Scope) -> Value {
    json!({"repos": s.repos, "agents": s.agents, "models": s.models, "max": s.max, "from": s.from, "allow_attach_input": s.allow_attach_input, "remote_control": s.remote_control, "include_bypass": s.include_bypass, "allow_bypass": false})
}

fn trust(paths: &Paths, a: &TrustArgs) -> Result<Output> {
    privileged::guard()?;
    let bin = std::env::current_exe()?;
    let from = grammar::parse_cidr_list(&a.from)?;
    let check = match std::env::var("USER").ok().filter(|u| !u.is_empty()) {
        Some(user) if !a.as_jump_for.is_empty() => sshd::check_jump(&user, &paths.home, &from),
        Some(user) => sshd::check(&user, &paths.home, &from),
        None => sshd::Check {
            verdict: Verdict::Unverified("USER is not set".into()),
            authorized_keys_files: sshd::default_files(&paths.home),
        },
    };
    let input = TrustInput {
        name: a.name.clone(),
        pubkey: a.pubkey.join(" "),
        jump_for: a.as_jump_for.clone(),
        scope: ScopeInput {
            repos: Some(a.repos.clone()),
            agents: Some(a.agents.clone()),
            models: if a.models.is_empty() {
                None
            } else {
                Some(a.models.clone())
            },
            max: a.max,
            from: Some(a.from.clone()),
            allow_attach_input: a.allow_attach_input.then_some(true),
            remote_control: None,
            include_bypass: None,
        },
    };
    let plan = follower::plan_trust(paths, &input, &bin, check)?;
    eprintln!(
        "key fingerprint {} (confirm it matches the leader)",
        plan.key.fingerprint
    );
    eprintln!(
        "will add to {}:\n  {}",
        paths.authorized_keys().display(),
        plan.line
    );
    if !plan.jump_for.is_empty() {
        eprintln!(
            "jump enrollment: this key may only forward to {} and answer hello; it cannot start runs",
            plan.jump_for.join(", ")
        );
    } else {
        eprintln!("scope: {}", plan.scope.summary());
    }
    if plan.jump_for.is_empty() && (plan.scope.repos.is_empty() || plan.scope.agents.is_empty()) {
        eprintln!("note: the scope has no repos or no agents, so every run will be refused");
    }
    if let Verdict::Unverified(why) = &plan.sshd {
        eprintln!("warning: the sshd configuration could not be fully verified: {why}");
        eprintln!("PermitUserEnvironment, AcceptEnv, ForceCommand, AuthorizedKeysCommand, PermitTunnel or a Match block may weaken the restriction.");
        privileged::confirm_typed_stdio("unverified", "Type unverified to continue anyway:")?;
    }
    privileged::confirm_typed_stdio(
        &plan.name,
        &format!("Type {} to trust this leader:", plan.name),
    )?;
    let edit = follower::apply_trust(paths, &plan)?;
    let sshd_state = if matches!(plan.sshd, Verdict::Verified) {
        "verified"
    } else {
        "unverified"
    };
    let mut human = format!(
        "✓ key fingerprint {} added\n✓ authorized_keys updated: {}\n✓ scope: {}",
        plan.key.fingerprint,
        paths.authorized_keys().display(),
        plan.scope.summary()
    );
    if let Some(b) = &edit.backup {
        human.push_str(&format!("\n  backup: {}", b.display()));
    }
    Ok(Output::new(
        json!({
            "link": plan.name,
            "fingerprint": plan.key.fingerprint,
            "authorized_keys": paths.authorized_keys().display().to_string(),
            "backup": edit.backup.as_ref().map(|b| b.display().to_string()),
            "scope": scope_json(&plan.scope),
            "jump_for": plan.jump_for,
            "sshd": sshd_state,
        }),
        human,
    ))
}

fn list(ctx: &Ctx, paths: &Paths) -> Result<Output> {
    let store = Store::load(paths)?;
    let own = leader::identity(paths)?;
    let data = json!({
        "identity": own.as_ref().map(|(_, fp)| json!({"fingerprint": fp})),
        "leaders": store.leaders.iter().map(|(n, l)| json!({
            "name": n, "role": "leader", "fingerprint": l.fingerprint, "added": l.added, "scope": scope_json(&l.scope),
            "jump_for": store.jump_for.get(n),
        })).collect::<Vec<_>>(),
        "followers": store.followers.iter().map(|(n, f)| json!({
            "name": n, "user": f.user, "host": f.host, "port": f.port,
            "host_key_fingerprint": f.host_key_fingerprint, "added": f.added,
            "jump": store.jumps.get(n),
            "identity": if store.peer_keys.contains_key(n) { "verified" } else { "unverified" },
        })).collect::<Vec<_>>(),
    });
    let mut parts: Vec<String> = Vec::new();
    if !store.leaders.is_empty() {
        let mut rows = vec![vec![
            "NAME".to_string(),
            "ROLE".into(),
            "FINGERPRINT".into(),
            "SCOPES".into(),
            "ADDED".into(),
        ]];
        for (n, l) in &store.leaders {
            rows.push(vec![
                n.clone(),
                "leader".into(),
                short_fp(&l.fingerprint),
                match store.jump_for.get(n) {
                    Some(d) => format!("jump enrollment: {}", d.join(",")),
                    None => l.scope.summary(),
                },
                l.added.chars().take(10).collect(),
            ]);
        }
        parts.push(table(&rows));
    }
    if !store.followers.is_empty() {
        let mut rows = vec![vec![
            "NAME".to_string(),
            "ROLE".into(),
            "TARGET".into(),
            "HOST KEY".into(),
            "ADDED".into(),
        ]];
        for (n, f) in &store.followers {
            rows.push(vec![
                n.clone(),
                "follower".into(),
                match store.jumps.get(n) {
                    Some(j) => format!("{}@{}:{} via {j}", f.user, f.host, f.port),
                    None => format!("{}@{}:{}", f.user, f.host, f.port),
                },
                short_fp(&f.host_key_fingerprint),
                f.added.chars().take(10).collect(),
            ]);
        }
        parts.push(table(&rows));
    }
    if let Some((_, fp)) = &own {
        parts.push(format!("this machine's link identity: {fp}"));
    }
    let unverified: Vec<&String> = store
        .followers
        .keys()
        .filter(|n| !store.peer_keys.contains_key(*n))
        .collect();
    if !unverified.is_empty() {
        parts.push(format!(
            "warning: identity unverified for {}: cycle protection does not cover these links (remove and add them again once the follower has run tmgr link init)",
            unverified.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }
    let human = if parts.is_empty() {
        ctx.style
            .muted("No links. Trust a leader: tmgr link trust <pubkey> --as leader --name NAME, or create an identity: tmgr link init")
    } else {
        parts.join("\n\n")
    };
    Ok(Output::new(data, human))
}

fn scope(ctx: &Ctx, paths: &Paths, a: &ScopeArgs) -> Result<Output> {
    let from = a.from.as_ref().map(|v| {
        if v.len() == 1 && v[0] == "any" {
            Vec::new()
        } else {
            v.clone()
        }
    });
    let input = ScopeInput {
        repos: a.repos.clone(),
        agents: a.agents.clone(),
        models: a.models.clone(),
        max: a.max,
        from,
        allow_attach_input: a.allow_attach_input,
        remote_control: a.remote_control.map(Switch::is_on),
        include_bypass: a.include_bypass.map(Switch::is_on),
    };
    let plan = follower::plan_scope(paths, &a.name, &input)?;
    let changes = a.repos.is_some()
        || a.agents.is_some()
        || a.models.is_some()
        || a.max.is_some()
        || a.from.is_some()
        || a.allow_attach_input.is_some()
        || a.remote_control.is_some()
        || a.include_bypass.is_some();
    if !changes {
        return Ok(Output::new(
            json!({"link": plan.name, "scope": scope_json(&plan.new)}),
            format!("{}: {}", plan.name, plan.new.summary()),
        ));
    }
    if plan.widening {
        privileged::guard()?;
        eprintln!("was: {}\nnew: {}", plan.old.summary(), plan.new.summary());
        privileged::confirm_typed_stdio(
            &plan.name,
            &format!(
                "This widens what {} may do. Type {} to confirm:",
                plan.name, plan.name
            ),
        )?;
        for warning in full_access_warnings(&plan.old, &plan.new) {
            eprintln!("{warning}");
        }
        if !full_access_warnings(&plan.old, &plan.new).is_empty() {
            privileged::confirm_typed_stdio(
                FULL_ACCESS_WORD,
                &format!("Type {FULL_ACCESS_WORD} to give {} this access:", plan.name),
            )?;
        }
    }
    let bin = std::env::current_exe()?;
    follower::apply_scope(paths, &plan, &bin)?;
    let closed = close_attached(paths, &plan.name, &plan.new)?;
    let unknown = closed.as_ref().map_or(0, |t| t.unknown);
    let unchecked = format!(
        "{unknown} worker record(s) of {} could not be checked, so an attach may still be open until its next check",
        plan.name
    );
    if unknown > 0 && (input.remote_control == Some(false) || input.include_bypass == Some(false)) {
        return Err(
            Error::internal(format!("the scope is saved, but {unchecked}"))
                .hint(unknown_workers_hint(paths, &plan.name)),
        );
    }
    let kind = if plan.widening {
        "widened"
    } else if plan.old == plan.new {
        "unchanged"
    } else {
        "narrowed"
    };
    let mut human = format!(
        "{} scope {kind}: {}",
        ctx.style.good("✓"),
        plan.new.summary()
    );
    if plan.new.include_bypass && !plan.new.remote_control {
        human.push_str("\nnote: include-bypass has no effect until remote control is on");
    }
    if let Some(t) = &closed {
        human.push_str(&format!(
            "\nclosed {} open remote attach(es), {} still running",
            t.signalled, t.survivors
        ));
    }
    if unknown > 0 {
        human.push_str(&format!("\nwarning: {unchecked}"));
    }
    Ok(Output::new(
        json!({
            "link": plan.name,
            "change": kind,
            "scope": scope_json(&plan.new),
            "closed": closed.as_ref().map(|t| json!({"signalled": t.signalled, "survivors": t.survivors, "unknown": t.unknown})),
        }),
        human,
    ))
}

const FULL_ACCESS_WORD: &str = "full access";

fn full_access_warnings(old: &Scope, new: &Scope) -> Vec<String> {
    let mut out = Vec::new();
    if new.remote_control && !old.remote_control {
        out.push(
            "Remote control gives this leader FULL ACCESS to this machine with your tokens and keys: it can see, type into and halt every runner session here, including ones you started yourself."
                .to_string(),
        );
    }
    if new.include_bypass && (!old.include_bypass || (new.remote_control && !old.remote_control)) {
        out.push(
            "Include-bypass makes sandbox-free sessions (--no-sandbox, --mode bypass) reachable too: full access to this machine with your tokens and keys, without the sandbox."
                .to_string(),
        );
        if !new.remote_control {
            out.push("It has no effect until remote control is on.".to_string());
        }
    }
    out
}

fn close_attached(paths: &Paths, name: &str, new: &Scope) -> Result<Option<Terminated>> {
    if new.remote_control && new.include_bypass {
        return Ok(None);
    }
    let hint = |e: Error| e.hint(unknown_workers_hint(paths, name));
    if !registry::any_attached(paths, name, &|a| follower::must_close(new, a)).map_err(hint)? {
        return Ok(None);
    }
    std::thread::sleep(CLOSE_WAIT);
    let t = follower::close_after_scope_change(paths, name, new, CLOSE_GRACE).map_err(hint)?;
    Audit::new(&paths.audit_dir()).append(
        "link.scope.closed",
        json!({"link": name, "signalled": t.signalled, "survivors": t.survivors, "unknown": t.unknown}),
    )?;
    if t.survivors > 0 {
        return Err(Error::internal(format!(
            "{} remote attach worker(s) of {name} did not stop",
            t.survivors
        ))
        .hint(format!("tmgr link revoke {name}")));
    }
    Ok(Some(t))
}

fn unknown_workers_hint(paths: &Paths, name: &str) -> String {
    format!(
        "check the records of {name} in {}, remove one only once its worker is gone, then rerun",
        paths.link_dir().join("workers").display()
    )
}

const CLOSE_WAIT: std::time::Duration = std::time::Duration::from_millis(1500);
const CLOSE_GRACE: std::time::Duration = std::time::Duration::from_secs(2);

fn revoke(paths: &Paths, a: &RevokeArgs) -> Result<Output> {
    let name = a.name.as_str();
    let revoked = follower::revoke(paths, name);
    let mut runner = tmgr_link::runner::RunnerClient::connect_default_as("cli").ok();
    let carried = revoked.as_ref().ok().and_then(|r| r.tombstone.clone());
    let contained = follower::contain_revoked(
        paths,
        name,
        runner
            .as_mut()
            .map(|c| c as &mut dyn tmgr_link::runner::Backend),
        a.halt_runs,
        carried,
    )?;
    let nothing_else = contained.workers.signalled
        + contained.workers.stale
        + contained.runs_known
        + contained.tombstones;
    let r = match revoked {
        Ok(r) => Some(r),
        Err(e) if e.exit_code() == 3 && nothing_else > 0 => None,
        Err(e) => return Err(e),
    };
    let mut problems = Vec::new();
    if contained.workers.survivors > 0 {
        problems.push(format!(
            "{} worker process(es) did not stop",
            contained.workers.survivors
        ));
    }
    if let Some(cause) = &contained.workers_unreadable {
        problems.push(format!(
            "the worker registry could not be read ({cause}), so its workers could not be checked"
        ));
    }
    if contained.workers.unknown > 0 {
        problems.push(format!(
            "{} worker record(s) could not be checked",
            contained.workers.unknown
        ));
    }
    if contained.runs_unreadable {
        problems
            .push("the link runs file is unreadable, so its runs could not be checked".to_string());
    }
    if a.halt_runs && runner.is_none() {
        problems.push("the runner is not reachable, so its runs were not halted".to_string());
    } else if contained.halt_failed > 0 {
        problems.push(format!(
            "{} run(s) could not be halted",
            contained.halt_failed
        ));
    }
    if !contained.unconfirmed.is_empty() {
        problems.push(format!(
            "the runner could not confirm the kill of: {}",
            contained.unconfirmed.join(", ")
        ));
    }
    if contained.runs_surviving > 0 {
        problems.push(format!(
            "{} run(s) of this link are still running after origin.revoke",
            contained.runs_surviving
        ));
    }
    if contained.incomplete() && problems.is_empty() {
        problems.push("a containment step did not finish".to_string());
    }
    if !problems.is_empty() {
        let retry = format!(
            "start the runner if needed and retry: tmgr runner start; tmgr link revoke {name} --halt-runs"
        );
        let workers_unclear =
            contained.workers_unreadable.is_some() || contained.workers.unknown > 0;
        return Err(Error::internal(format!(
            "containment incomplete after revoking {name}: {}",
            problems.join("; ")
        ))
        .hint(if workers_unclear {
            format!("{}; {retry}", unknown_workers_hint(paths, name))
        } else {
            retry
        }));
    }
    let keys_removed = r.as_ref().map_or(0, |r| r.keys_removed);
    let removed = if r.is_none() {
        "already revoked".to_string()
    } else if keys_removed > 0 {
        "key removed from authorized_keys, trust dropped".to_string()
    } else {
        "no authorized_keys line found, trust dropped".to_string()
    };
    let kept = contained.runs_known.saturating_sub(contained.runs_halted);
    let runs = if a.halt_runs {
        format!("{} run(s) halted", contained.runs_halted)
    } else if kept > 0 {
        format!("{kept} run(s) keep going (shown as origin: link:{name} (revoked); --halt-runs stops them)")
    } else {
        "no runs of this link".to_string()
    };
    Ok(Output::new(
        json!({
            "link": name,
            "keys_removed": keys_removed,
            "backup": r.as_ref().and_then(|r| r.backup.as_ref()).map(|b| b.display().to_string()),
            "workers_stopped": contained.workers.signalled,
            "workers_surviving": contained.workers.survivors,
            "runs_halted": contained.runs_halted,
            "runs_known": contained.runs_known,
            "halt_failed": contained.halt_failed,
            "runs_surviving": contained.runs_surviving,
            "unconfirmed": contained.unconfirmed,
            "runs_unreadable": contained.runs_unreadable,
        }),
        format!(
            "✓ revoked: {removed}, {} worker(s) stopped, {runs}, audit kept",
            contained.workers.signalled
        ),
    ))
}

fn remove(paths: &Paths, name: &str) -> Result<Output> {
    grammar::link_name(name)?;
    let in_use = |s: &Store| -> Result<()> {
        let users = jump::referenced_by(s, name);
        if users.is_empty() {
            return Ok(());
        }
        Err(
            Error::conflict(format!("{name} is the jump host of {}", users.join(", ")))
                .hint("remove those links first"),
        )
    };
    if let Err(e) = in_use(&Store::load(paths)?) {
        let _ = Audit::new(&paths.audit_dir()).append(
            "link.remove.refused",
            json!({"link": name, "reason": "used as a jump host"}),
        );
        return Err(e);
    }
    Store::update(paths, |s| {
        in_use(s)?;
        s.followers.remove(name).ok_or_else(|| {
            Error::not_found(format!("no follower named {name}")).hint("tmgr link list")
        })?;
        s.jumps.remove(name);
        s.peer_keys.remove(name);
        Ok(())
    })?;
    leader::unpin_host(paths, name)?;
    Audit::new(&paths.audit_dir()).append("link.remove", json!({"link": name}))?;
    Ok(Output::new(
        json!({"link": name}),
        format!("✓ {name} removed, host key unpinned"),
    ))
}

fn test(ctx: &Ctx, paths: &Paths, name: &str) -> Result<Output> {
    let started = Instant::now();
    let (mut client, _key) = Client::dial(paths, &cred(ctx, paths), name)?;
    let hello = client.hello()?;
    let ms = started.elapsed().as_millis() as u64;
    let version = sanitize_line(hello.get("version").and_then(Value::as_str).unwrap_or("?"));
    Ok(Output::new(
        json!({"link": name, "rtt_ms": ms, "follower_version": version}),
        format!(
            "{} {name}: handshake ok in {ms} ms (follower tmgr {version})",
            ctx.style.good("✓")
        ),
    ))
}

fn audit(ctx: &Ctx, paths: &Paths, a: &AuditArgs) -> Result<Output> {
    let log = Audit::new(&paths.audit_dir());
    if a.verify {
        return match log.verify()? {
            Ok(v) if v.incomplete => Err(Error::internal(format!(
                "audit chain intact over {} entries, but the newest entry is not anchored yet (incomplete)",
                v.entries
            ))
            .hint("run it again after the next entry; a crash between append and anchor causes this")),
            Ok(v) => Ok(Output::new(
                json!({"ok": true, "entries": v.entries, "files": v.files}),
                format!(
                    "{} audit chain intact: {} entries in {} file(s)",
                    ctx.style.good("✓"),
                    v.entries,
                    v.files
                ),
            )),
            Err(t) => Err(Error::internal(format!(
                "audit chain broken in {} at line {}: {}",
                t.file, t.line, t.reason
            ))
            .hint("the log was edited, reordered or truncated; keep it as evidence")),
        };
    }
    let entries = log.tail(a.tail.min(1000))?;
    if entries.is_empty() {
        return Ok(Output::new(
            json!({"entries": []}),
            ctx.style.muted("The audit log is empty."),
        ));
    }
    let human = entries
        .iter()
        .map(|e| format_entry(ctx, e))
        .collect::<Vec<_>>()
        .join("\n");
    Ok(Output::new(json!({ "entries": entries }), human))
}

fn format_entry(ctx: &Ctx, e: &Value) -> String {
    let text = |k: &str| sanitize_line(e.get(k).and_then(Value::as_str).unwrap_or(""));
    let when = chrono::DateTime::parse_from_rfc3339(&text("ts"))
        .map(|t| {
            t.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M:%S")
                .to_string()
        })
        .unwrap_or_else(|_| text("ts"));
    let link = text("link");
    if text("event") == "worker.request" {
        let decision = text("decision");
        let reason = text("reason");
        let verdict = if decision == "accepted" {
            ctx.style.good(&decision)
        } else {
            let tail = if reason.is_empty() {
                decision
            } else {
                format!("{decision}: {reason}")
            };
            ctx.style.accent(&tail)
        };
        return format!("{when} from {link}  {}  {verdict}", request_what(e));
    }
    if let Some(line) = remote_line(e, &when, &link) {
        return line;
    }
    let rest: Vec<String> = e
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(k, _)| !matches!(k.as_str(), "v" | "seq" | "ts" | "prev" | "event" | "link"))
        .map(|(k, v)| {
            format!(
                "{}={}",
                sanitize_line(k),
                sanitize_line(&v.as_str().map_or_else(|| v.to_string(), str::to_string))
            )
        })
        .collect();
    format!("{when} {} {link} {}", text("event"), rest.join(" "))
        .trim_end()
        .to_string()
}

fn audit_field(raw: &str) -> String {
    let clean = sanitize_line(raw);
    if !clean
        .chars()
        .all(|c| c.is_ascii_graphic() && !matches!(c, '"' | '\\' | '='))
    {
        format!("{clean:?}")
    } else {
        clean
    }
}

fn request_what(e: &Value) -> String {
    let method = audit_field(e.get("method").and_then(Value::as_str).unwrap_or(""));
    format!("{method} {}", request_args(e))
        .trim_end()
        .to_string()
}

fn request_args(e: &Value) -> String {
    e.get("args")
        .and_then(Value::as_object)
        .map(|m| {
            m.iter()
                .map(|(k, v)| {
                    format!(
                        "{}={}",
                        audit_field(k),
                        audit_field(v.as_str().unwrap_or(""))
                    )
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

fn remote_line(e: &Value, when: &str, link: &str) -> Option<String> {
    let text = |k: &str| sanitize_line(e.get(k).and_then(Value::as_str).unwrap_or(""));
    match text("event").as_str() {
        "worker.attach_input" => {
            let session = ["run", "session", "id"]
                .iter()
                .map(|k| text(k))
                .find(|v| !v.is_empty())
                .unwrap_or_default();
            let via = if text("via") == "remote_control" {
                " (remote control)"
            } else {
                ""
            };
            let count = e
                .get("msgs")
                .and_then(Value::as_u64)
                .map_or(String::new(), |n| format!(", {n} messages"));
            Some(
                format!(
                    "{when} worker.attach_input {link} input sent to session {session}{via}{count}"
                )
                .trim_end()
                .to_string(),
            )
        }
        "link.scope.closed" => {
            let num = |k: &str| e.get(k).and_then(Value::as_u64).unwrap_or(0);
            Some(format!(
                "{when} link.scope.closed {link} remote attaches stopped={} still-running={} unknown={}",
                num("signalled"),
                num("survivors"),
                num("unknown")
            ))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attach_input_audit_names_link_and_session_but_never_content() {
        let e = json!({"event": "worker.attach_input", "link": "macbook", "run": "s_abc", "via": "remote_control", "bytes": 12, "data": "SECRET"});
        let line = remote_line(&e, "t", "macbook").unwrap();
        assert!(line.contains("macbook"));
        assert!(line.contains("s_abc"));
        assert!(line.contains("remote control"));
        assert!(!line.contains("SECRET"));
    }

    #[test]
    fn audit_view_strips_terminal_controls_from_request_field_names() {
        let e = json!({"args": {"\u{1b}[2Jevil": "\u{1b}[31mv"}});
        assert!(!request_args(&e).contains('\u{1b}'));
    }

    #[test]
    fn audit_view_keeps_a_spaced_value_from_looking_like_more_fields() {
        let e = json!({"args": {"run": "s_x accepted=1"}});
        assert_eq!(request_args(&e), r#"run="s_x accepted=1""#);
        assert_eq!(request_args(&json!({"args": {"run": "s_x"}})), "run=s_x");
    }

    #[test]
    fn audit_view_keeps_a_spaced_method_from_looking_like_more_fields() {
        let e = json!({"method": "runs.halt accepted", "args": {"run": "s_x"}});
        assert_eq!(request_what(&e), r#""runs.halt accepted" run=s_x"#);
        let plain = json!({"method": "runs.list"});
        assert_eq!(request_what(&plain), "runs.list");
    }

    #[test]
    fn audit_view_keeps_quote_characters_from_joining_fields() {
        let e = json!({"args": {"a": "\"x", "b": "y\""}});
        assert_eq!(request_args(&e), r#"a="\"x" b="y\"""#);
    }

    #[test]
    fn audit_view_quotes_blank_looking_unicode() {
        let e = json!({"method": "runs.halt\u{2800}\u{2800}accepted"});
        assert!(request_what(&e).starts_with('"'));
    }

    #[test]
    fn attach_input_audit_shows_the_count() {
        let e = json!({"event": "worker.attach_input", "link": "macbook", "run": "s_abc", "via": "remote_control", "msgs": 3, "bytes": 12});
        assert!(remote_line(&e, "t", "macbook")
            .unwrap()
            .contains("3 messages"));
    }

    #[test]
    fn scope_closed_audit_shows_the_link() {
        let e = json!({"event": "link.scope.closed", "link": "macbook", "signalled": 2, "survivors": 0});
        let line = remote_line(&e, "t", "macbook").unwrap();
        assert!(line.contains("macbook"));
        assert!(line.contains("stopped=2"));
        let e = json!({"event": "link.scope.closed", "link": "macbook", "unknown": 1});
        assert!(remote_line(&e, "t", "macbook")
            .unwrap()
            .contains("unknown=1"));
    }

    #[test]
    fn enabling_remote_control_warns_about_full_access_and_bypass_about_the_sandbox() {
        let off = Scope::default();
        let remote = Scope {
            remote_control: true,
            ..Default::default()
        };
        let both = Scope {
            remote_control: true,
            include_bypass: true,
            ..Default::default()
        };
        let text = full_access_warnings(&off, &remote).join(" ");
        assert!(text.contains("FULL ACCESS") && text.contains("tokens and keys"));
        assert!(!text.contains("sandbox"));
        let text = full_access_warnings(&remote, &both).join(" ");
        assert!(text.contains("sandbox-free") && text.contains("tokens and keys"));
        assert!(full_access_warnings(&both, &off).is_empty());
        let stored = Scope {
            include_bypass: true,
            ..Default::default()
        };
        assert!(full_access_warnings(&stored, &both)
            .join(" ")
            .contains("sandbox-free"));
        let alone = Scope {
            include_bypass: true,
            ..Default::default()
        };
        assert!(full_access_warnings(&off, &alone)
            .join(" ")
            .contains("no effect until remote control is on"));
    }
}
