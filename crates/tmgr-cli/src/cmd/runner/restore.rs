use super::{connect, emit, rerr, runner_config, Opts};
use crate::interact;
use crate::out::table;
use serde_json::json;
use tmgr_core::error::{Error, Result};
use tmgr_ipc::api::{SessionInfo, StateKind};
use tmgr_link::privileged;
use tmgr_runner::daemon::{self, RunnerConfig};
use tmgr_runner::restore::{self, Entry, Status};
use tmgr_runner::sanitize::sanitize_text;
use tmgr_runner::wire::now_ms;

fn clean(s: &str) -> String {
    tmgr_core::text::sanitize_line(&sanitize_text(s))
}

fn in_agent() -> bool {
    privileged::RUN_ENV
        .iter()
        .any(|k| std::env::var_os(k).is_some())
}

fn entries_with(cfg: &RunnerConfig, sessions: &[SessionInfo]) -> Vec<Entry> {
    let running = |id: &str| {
        sessions
            .iter()
            .any(|s| s.id == id && s.state.kind != StateKind::Exited)
    };
    restore::list(&restore::store(&cfg.homes), &cfg.homes, now_ms(), &running)
}

fn entries(cfg: &RunnerConfig) -> Result<Vec<Entry>> {
    let sessions = connect(cfg)?.list().map_err(rerr)?;
    Ok(entries_with(cfg, &sessions))
}

pub fn count(cfg: &RunnerConfig, sessions: &[SessionInfo]) -> usize {
    entries_with(cfg, sessions)
        .iter()
        .filter(|e| e.status == Status::Restorable)
        .count()
}

fn age(e: &Entry) -> String {
    super::ops::span(now_ms().saturating_sub(e.intent.created_ms) / 1000)
}

fn task(e: &Entry) -> String {
    e.intent
        .task
        .map_or("-".into(), |t| format!("w{}:{}", t.workspace_id, t.task_id))
}

fn agent(e: &Entry) -> String {
    match &e.intent.model {
        Some(m) => format!("{} ({})", clean(&e.intent.profile), clean(m)),
        None => clean(&e.intent.profile),
    }
}

fn describe(e: &Entry) -> String {
    let i = &e.intent;
    let mut out = format!(
        "  session  {}\n  agent    {}\n  folder   {}\n  task     {}\n  started  {} ago\n",
        clean(&i.session),
        agent(e),
        clean(&i.cwd),
        task(e),
        age(e),
    );
    if let Some(h) = &i.bundle_sha256 {
        out.push_str(&format!(
            "  bundle   {} (the task text approved at the start, reused as is)\n",
            clean(&h[..h.len().min(12)])
        ));
    }
    out.push_str(if e.resumable {
        "  resume   the agent continues its previous conversation when its profile supports it\n"
    } else {
        "  resume   no saved agent state; the agent starts fresh\n"
    });
    out
}

pub fn run(o: &Opts, session: Option<&str>, discard: bool) -> Result<i32> {
    let cfg = runner_config()?;
    if discard {
        let id = session.ok_or_else(|| Error::usage("--discard needs a session id"))?;
        privileged::check(in_agent(), true)?;
        restore::load(&restore::store(&cfg.homes), id)
            .map_err(|_| Error::not_found(format!("no restore record for {}", clean(id))))?;
        restore::forget(&restore::store(&cfg.homes), id);
        return Ok(emit(
            o,
            json!({"discarded": id}),
            format!("discarded the restore record of {}", clean(id)),
        ));
    }
    daemon::start_background(&cfg).map_err(rerr)?;
    let all = entries(&cfg)?;
    if let Some(id) = session {
        let e = all.iter().find(|e| e.intent.session == id).ok_or_else(|| {
            Error::not_found(format!("no restore record for {}", clean(id)))
                .hint(format!("{} agents restore", super::bin()))
        })?;
        return restore_one(o, &cfg, e);
    }
    if o.json {
        return Ok(emit(o, json!({"sessions": all}), ""));
    }
    if all.is_empty() {
        println!("Nothing to restore.");
        return Ok(0);
    }
    let mut rows = vec![["ID", "NAME", "AGENT", "FOLDER", "TASK", "AGE", "STATUS"]
        .map(String::from)
        .to_vec()];
    for e in &all {
        rows.push(vec![
            clean(&e.intent.session),
            clean(&e.intent.name),
            agent(e),
            clean(&e.intent.cwd),
            task(e),
            age(e),
            e.status.reason().to_string(),
        ]);
    }
    print!("{}", table(&rows));
    println!();
    let restorable: Vec<&Entry> = all
        .iter()
        .filter(|e| e.status == Status::Restorable)
        .collect();
    if restorable.is_empty() || !interact::is_interactive() || in_agent() {
        if !restorable.is_empty() {
            let bin = super::bin();
            println!("Restore one in a terminal with: {bin} agents restore <id>");
        }
        return Ok(0);
    }
    for e in restorable {
        match restore_one(o, &cfg, e) {
            Ok(_) => {}
            Err(err) if err.kind == tmgr_core::error::Kind::Interrupted => {
                eprintln!("skipped {}", clean(&e.intent.session));
            }
            Err(err) => return Err(err),
        }
    }
    Ok(0)
}

fn restore_one(o: &Opts, cfg: &RunnerConfig, e: &Entry) -> Result<i32> {
    if e.status != Status::Restorable {
        return Err(Error::new(
            tmgr_core::error::Kind::Forbidden,
            format!("{}: {}", clean(&e.intent.session), e.status.reason()),
        ));
    }
    privileged::guard()?;
    eprintln!("Restore this agent session?\n{}", describe(e));
    interact::confirm("Start it again?", false, false)?;
    let r = connect(cfg)?.restore(&e.intent.session).map_err(rerr)?;
    let how = if r.resumed {
        "resumed"
    } else {
        "started fresh"
    };
    let bin = super::bin();
    let human = format!(
        "{how}: {} ({}); attach with: {bin} attach {}",
        clean(&r.session.name),
        clean(&r.session.id),
        clean(&r.session.id)
    );
    Ok(emit(o, &r, human))
}
