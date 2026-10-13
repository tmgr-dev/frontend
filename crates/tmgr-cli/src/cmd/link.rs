use crate::cli_link::{
    AddArgs, AuditArgs, InitArgs, LinkCommand, RevokeArgs, ScopeArgs, TrustArgs,
};
use crate::ctx::Ctx;
use crate::out::{table, Output};
use serde_json::{json, Value};
use std::time::Instant;
use tmgr_core::creds::Backend;
use tmgr_core::error::{Error, Result};
use tmgr_core::text::sanitize_line;
use tmgr_link::audit::Audit;
use tmgr_link::follower::{self, ScopeInput, TrustInput};
use tmgr_link::grammar;
use tmgr_link::leader::{self, Client, SshTarget};
use tmgr_link::privileged;
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
    if a.jump.is_some() {
        return Err(
            Error::usage("--jump is not supported yet").hint("link the follower directly for now")
        );
    }
    let name = grammar::link_name(&a.name)?.to_string();
    let target = grammar::parse_target(&a.ssh)?;
    let port = grammar::parse_port(a.port)?;
    if leader::identity(paths)?.is_none() {
        return Err(Error::not_found("this machine has no link identity").hint("tmgr link init"));
    }
    if Store::load(paths)?.followers.contains_key(&name) {
        return Err(
            Error::conflict(format!("a follower named {name} already exists"))
                .hint(format!("tmgr link remove {name} first")),
        );
    }
    let key = leader::scan_host(&target.host, port)?;
    eprintln!(
        "host key of {}:{port}\n  {}\nVerify this fingerprint with the follower's owner (ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub on it).",
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
        let (mut client, _key_file) = Client::connect(paths, &cred(ctx, paths), &name, &ssh)?;
        client.hello()
    })();
    let hello = match hello {
        Ok(h) => h,
        Err(e) => {
            let _ = leader::unpin_host(paths, &name);
            return Err(e);
        }
    };
    Store::update(paths, |s| {
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
    })?;
    Audit::new(&paths.audit_dir()).append(
        "link.add",
        json!({"link": name, "host": target.host, "port": port, "fingerprint": key.fingerprint}),
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
    json!({"repos": s.repos, "agents": s.agents, "models": s.models, "max": s.max, "from": s.from, "allow_attach_input": s.allow_attach_input, "allow_bypass": false})
}

fn trust(paths: &Paths, a: &TrustArgs) -> Result<Output> {
    privileged::guard()?;
    let bin = std::env::current_exe()?;
    let from = grammar::parse_cidr_list(&a.from)?;
    let check = match std::env::var("USER").ok().filter(|u| !u.is_empty()) {
        Some(user) => sshd::check(&user, &paths.home, &from),
        None => sshd::Check {
            verdict: Verdict::Unverified("USER is not set".into()),
            authorized_keys_files: sshd::default_files(&paths.home),
        },
    };
    let input = TrustInput {
        name: a.name.clone(),
        pubkey: a.pubkey.join(" "),
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
    eprintln!("scope: {}", plan.scope.summary());
    if plan.scope.repos.is_empty() || plan.scope.agents.is_empty() {
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
        })).collect::<Vec<_>>(),
        "followers": store.followers.iter().map(|(n, f)| json!({
            "name": n, "user": f.user, "host": f.host, "port": f.port,
            "host_key_fingerprint": f.host_key_fingerprint, "added": f.added,
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
                l.scope.summary(),
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
                format!("{}@{}:{}", f.user, f.host, f.port),
                short_fp(&f.host_key_fingerprint),
                f.added.chars().take(10).collect(),
            ]);
        }
        parts.push(table(&rows));
    }
    if let Some((_, fp)) = &own {
        parts.push(format!("this machine's link identity: {fp}"));
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
    };
    let plan = follower::plan_scope(paths, &a.name, &input)?;
    let changes = a.repos.is_some()
        || a.agents.is_some()
        || a.models.is_some()
        || a.max.is_some()
        || a.from.is_some()
        || a.allow_attach_input.is_some();
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
    }
    let bin = std::env::current_exe()?;
    follower::apply_scope(paths, &plan, &bin)?;
    let kind = if plan.widening {
        "widened"
    } else if plan.old == plan.new {
        "unchanged"
    } else {
        "narrowed"
    };
    Ok(Output::new(
        json!({"link": plan.name, "change": kind, "scope": scope_json(&plan.new)}),
        format!(
            "{} scope {kind}: {}",
            ctx.style.good("✓"),
            plan.new.summary()
        ),
    ))
}

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
    if !problems.is_empty() {
        return Err(Error::internal(format!(
            "containment incomplete after revoking {name}: {}",
            problems.join("; ")
        ))
        .hint(format!(
            "start the runner if needed and retry: tmgr runner start; tmgr link revoke {name} --halt-runs"
        )));
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
    Store::update(paths, |s| {
        s.followers.remove(name).map(|_| ()).ok_or_else(|| {
            Error::not_found(format!("no follower named {name}")).hint("tmgr link list")
        })
    })?;
    leader::unpin_host(paths, name)?;
    Audit::new(&paths.audit_dir()).append("link.remove", json!({"link": name}))?;
    Ok(Output::new(
        json!({"link": name}),
        format!("✓ {name} removed, host key unpinned"),
    ))
}

fn test(ctx: &Ctx, paths: &Paths, name: &str) -> Result<Output> {
    grammar::link_name(name)?;
    let store = Store::load(paths)?;
    let f = store.followers.get(name).ok_or_else(|| {
        Error::not_found(format!("no follower named {name}")).hint("tmgr link list")
    })?;
    let target = SshTarget {
        user: f.user.clone(),
        host: f.host.clone(),
        port: f.port,
    };
    let started = Instant::now();
    let (mut client, _key) = Client::connect(paths, &cred(ctx, paths), name, &target)?;
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
        let args = e
            .get("args")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .map(|(k, v)| format!("{k}={}", sanitize_line(v.as_str().unwrap_or(""))))
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .unwrap_or_default();
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
        let what = format!("{} {args}", text("method"));
        return format!("{when} from {link}  {}  {verdict}", what.trim_end());
    }
    let rest: Vec<String> = e
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(k, _)| !matches!(k.as_str(), "v" | "seq" | "ts" | "prev" | "event" | "link"))
        .map(|(k, v)| {
            format!(
                "{k}={}",
                sanitize_line(&v.as_str().map_or_else(|| v.to_string(), str::to_string))
            )
        })
        .collect();
    format!("{when} {} {link} {}", text("event"), rest.join(" "))
        .trim_end()
        .to_string()
}
