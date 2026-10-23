use std::collections::BTreeSet;
use std::io::{BufRead, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde_json::json;
use tmgr_core::api::Client;
use tmgr_core::approval::{canonical_repos, AllowFile, CloudApproval};
use tmgr_core::creds::Tokens;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::text::sanitize_line;
use tmgr_ipc::api::caps::AUTOPILOT_V1;
use tmgr_ipc::api::AutopilotCommand;
use tmgr_link::privileged;
use tmgr_queue::cloud::{CloudApi, CloudWorkspace};
use tmgr_queue::rules::{self, Rule, GLOBAL_MAX};
use tmgr_queue::taken::Fresh;
use tmgr_queue::Paths;
use tmgr_usage::Tool;

use super::interview::{self, Draft};
use super::model::{Answers, Finish, LoopKind, Settings, QUEUE_SETTINGS_ID};
use super::orchestrate::ModeSetting;
use super::store::Store;
use super::usage;
use super::{render, StartArgs};
use crate::cli::Cli;
use crate::cmd::notifyenv::NotifyEnv;
use crate::cmd::queue::{self, CliCloud};
use crate::cmd::runner::cloud::{cloud_gate, parse_eligibility, Eligibility, GateOk};
use crate::cmd::runner::{connect, rerr, runner_config};
use crate::cmd::signer::{device_name, read_offer, Offer, SignTask, Signed, Signer, OFFER_OFFLINE};
use crate::ctx::Ctx;
use crate::out::{table, Output};

const LIST_CAP: usize = 40;
const TITLE_WIDTH: usize = 48;
const IO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq)]
pub enum Approval {
    Valid,
    NeedsSigning(String),
    Refused(String),
    Unchecked(String),
}

impl Approval {
    pub fn runnable(&self) -> bool {
        !matches!(self, Approval::Refused(_) | Approval::Unchecked(_))
    }

    fn shown(&self) -> String {
        match self {
            Approval::Unchecked(why) => format!("could not check, retry ({why})"),
            Approval::Valid => "signed".into(),
            Approval::NeedsSigning(_) => "needs signing".into(),
            Approval::Refused(why) => format!("refused: {why}"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Found {
    pub id: i64,
    pub key: String,
    pub title: String,
    pub repos: Vec<String>,
    pub approval: Approval,
    pub sign: Option<SignTask>,
}

pub struct Facts<'a> {
    pub rule: &'a Rule,
    pub ws: &'a CloudWorkspace,
    pub allow: &'a AllowFile,
    pub keys: &'a [String],
    pub host: &'a str,
    pub user: i64,
    pub now: DateTime<Utc>,
    pub persona_optional: bool,
}

fn placeholder_claim(task_id: i64, f: &Facts) -> CloudApproval {
    CloudApproval {
        host: f.host.to_string(),
        user_id: f.user,
        workspace_id: f.ws.id,
        task_id,
        category_id: 0,
        content_sha256: String::new(),
        key_id: String::new(),
        signed_at: 0,
        signature: String::new(),
        kind: String::new(),
        repos_sha256: None,
        persona_sha256: None,
        context_sha256: None,
    }
}

pub fn classify(task_id: i64, elig: &Eligibility, f: &Facts) -> Found {
    let key = sanitize_line(
        elig.task_key
            .as_deref()
            .unwrap_or(&format!("task-{task_id}")),
    );
    let title = sanitize_line(&elig.title);
    let repos = elig
        .category
        .as_ref()
        .and_then(|c| {
            c.repositories
                .clone()
                .or_else(|| c.github_full_name.clone().map(|n| vec![n]))
        })
        .unwrap_or_default();
    let make = |approval: Approval, sign: Option<SignTask>| Found {
        id: task_id,
        key: key.clone(),
        title: title.clone(),
        repos: repos.clone(),
        approval,
        sign,
    };
    let refused = |why: &str| make(Approval::Refused(sanitize_line(why)), None);
    if elig.task_id != task_id || elig.workspace_id != f.ws.id {
        return refused("the server answered for another task or workspace");
    }
    if elig.viewer_user_id != f.user {
        return refused("the server answered for another account");
    }
    if !elig.setting_enabled {
        return refused("agent runs are off for this workspace");
    }
    let Some(category) = &elig.category else {
        return refused("the task has no category, so no repository is known");
    };
    let Some(entry) = f.allow.find(f.host, f.user, f.ws.id) else {
        return refused("the workspace is not allowed to run agents on this machine");
    };
    let listed = match category.repositories.as_deref() {
        Some(list) => match canonical_repos(list) {
            Ok(c) => Some(c),
            Err(_) => return refused("the server sent an invalid repository list"),
        },
        None => None,
    };
    let probe = GateOk {
        claim: placeholder_claim(task_id, f),
        title: elig.title.clone(),
        body: String::new(),
        category: Some(category.clone()),
        task_key: key.clone(),
        repos: listed,
        persona: None,
        context: Vec::new(),
    };
    if let Err(why) = queue::rule_binding(f.rule, &probe, entry, category, &f.ws.code, task_id) {
        return refused(&why);
    }
    match cloud_gate(f.allow, f.keys, elig, f.host, f.user, f.now, None) {
        Ok(_) => make(Approval::Valid, None),
        Err(why) => make(
            Approval::NeedsSigning(sanitize_line(&why.message)),
            Some(SignTask {
                id: task_id,
                workspace_id: elig.workspace_id,
                category_id: category.id,
                title: elig.title.clone(),
                description: elig.description.clone().unwrap_or_default(),
                repos_sha256: category
                    .repositories
                    .as_deref()
                    .and_then(|list| tmgr_core::approval::repos_sha256(list).ok()),
                offer: Offer::default(),
            }),
        ),
    }
}

fn not_free(
    found: Found,
    persona_optional: bool,
    look: impl FnOnce(&Found) -> Result<Fresh>,
) -> Found {
    if !found.approval.runnable() {
        return found;
    }
    let why = match look(&found) {
        Ok(fresh) if fresh.free_with(persona_optional) => return found,
        Ok(fresh) => match fresh.reason() {
            "taken" => "taken by someone else".to_string(),
            "persona_missing" => "the rule's persona is not assigned to the task".to_string(),
            _ => "the task is not in the rule's status".to_string(),
        },
        Err(e) if e.kind == Kind::Offline => {
            return Found {
                approval: Approval::Unchecked(sanitize_line(&e.message)),
                sign: None,
                ..found
            }
        }
        Err(e) => format!("could not check the task ({})", sanitize_line(&e.message)),
    };
    Found {
        approval: Approval::Refused(why),
        sign: None,
        ..found
    }
}

fn with_offer(client: &Client, mut found: Found) -> (Found, bool) {
    let Some(sign) = found.sign.as_mut() else {
        return (found, false);
    };
    match read_offer(client, found.id) {
        Ok(offered) => {
            sign.offer = offered;
            (found, false)
        }
        Err(code) if code == OFFER_OFFLINE => (
            Found {
                approval: Approval::Unchecked("the approval offer could not be read".into()),
                sign: None,
                ..found
            },
            false,
        ),
        Err(code) => (
            Found {
                approval: Approval::Refused(format!(
                    "could not read the approval offer ({})",
                    sanitize_line(&code)
                )),
                sign: None,
                ..found
            },
            true,
        ),
    }
}

const RETRIES: u32 = 3;
const RETRY_PAUSE: std::time::Duration = std::time::Duration::from_millis(250);
const OFFLINE_STREAK: usize = 3;
const WORKERS: usize = 4;

pub fn make_keys_unique<T>(
    items: &mut [T],
    id: impl Fn(&T) -> i64,
    key: impl Fn(&mut T) -> &mut String,
) {
    loop {
        let mut seen = std::collections::HashSet::new();
        let mut dup = std::collections::HashSet::new();
        for it in items.iter_mut() {
            let k = key(it).to_lowercase();
            if !seen.insert(k.clone()) {
                dup.insert(k);
            }
        }
        if dup.is_empty() {
            return;
        }
        for it in items.iter_mut() {
            let label = format!("task-{}", id(it));
            let k = key(it);
            if dup.contains(&k.to_lowercase()) {
                *k = label;
            }
        }
    }
}

pub fn unique_keys(found: &mut [Found]) {
    make_keys_unique(found, |f| f.id, |f| &mut f.key);
}

fn unreachable_error(e: &Error) -> Error {
    Error::new(
        Kind::Offline,
        format!(
            "the server stopped answering while the tasks were checked ({})",
            sanitize_line(&e.message)
        ),
    )
    .hint("check your connection and retry")
}

struct Checked {
    found: Found,
    offline: bool,
    retry: bool,
}

fn check_card(
    client: &Client,
    facts: &Facts,
    card: &queue::Card,
    look: impl FnOnce(&Found) -> Result<Fresh>,
) -> Checked {
    let id = card.id;
    match client.agent_run_task(id).and_then(parse_eligibility) {
        Ok(elig) => {
            let look_failed = std::cell::Cell::new(false);
            let looked = not_free(classify(id, &elig, facts), facts.persona_optional, |f| {
                let fresh = look(f);
                if fresh.as_ref().is_err_and(|e| e.kind != Kind::Offline) {
                    look_failed.set(true);
                }
                fresh
            });
            let (found, offer_failed) = with_offer(client, looked);
            Checked {
                found,
                offline: false,
                retry: look_failed.get() || offer_failed,
            }
        }
        Err(e) if e.kind == Kind::Offline => Checked {
            found: Found {
                id,
                key: card.key.clone(),
                title: String::new(),
                repos: Vec::new(),
                approval: Approval::Unchecked(sanitize_line(&e.message)),
                sign: None,
            },
            offline: true,
            retry: false,
        },
        Err(e) => Checked {
            found: Found {
                id,
                key: card.key.clone(),
                title: String::new(),
                repos: Vec::new(),
                approval: Approval::Refused(format!(
                    "could not read the task ({})",
                    sanitize_line(&e.message)
                )),
                sign: None,
            },
            offline: false,
            retry: true,
        },
    }
}

struct Pool<'a> {
    server: &'a str,
    agent: &'a str,
    verbose: bool,
    tokens: Tokens,
}

fn check_in_parallel(
    pool: &Pool,
    rule: &Rule,
    facts: &Facts,
    cards: &[queue::Card],
    shared: &queue::FreshShared,
) -> (Vec<Option<Checked>>, Option<Error>) {
    let total = cards.len();
    let slots: Vec<Mutex<Option<Checked>>> = (0..total).map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let last_offline = Mutex::new(None::<Error>);
    std::thread::scope(|scope| {
        for _ in 0..WORKERS.min(total) {
            scope.spawn(|| {
                let Ok(client) = Client::new(
                    pool.server,
                    pool.agent,
                    Some(pool.tokens.clone()),
                    None,
                    pool.verbose,
                ) else {
                    return;
                };
                client.set_get_retries(RETRIES, RETRY_PAUSE);
                let mut streak = 0;
                while !stop.load(Ordering::Relaxed) {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(card) = cards.get(i) else { break };
                    let checked = check_card(&client, facts, card, |f| {
                        queue::fresh_look_shared(&client, rule, f.id, shared)
                    });
                    if checked.offline {
                        streak += 1;
                        if streak >= OFFLINE_STREAK {
                            stop.store(true, Ordering::Relaxed);
                        }
                        if let Approval::Unchecked(why) = &checked.found.approval {
                            *last_offline.lock().unwrap() = Some(Error::new(Kind::Offline, why));
                        }
                    } else if !matches!(checked.found.approval, Approval::Unchecked(_)) {
                        streak = 0;
                    }
                    *slots[i].lock().unwrap() = Some(checked);
                    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if n.is_multiple_of(10) {
                        let _ = writeln!(std::io::stderr(), "Checked {n} of {total} tasks...");
                    }
                }
            });
        }
    });
    let stopped = stop
        .load(Ordering::Relaxed)
        .then(|| last_offline.into_inner().unwrap())
        .flatten();
    let slots = slots.into_iter().map(|m| m.into_inner().unwrap()).collect();
    (slots, stopped)
}

fn discover(
    ctx: &Ctx,
    cloud: &CliCloud,
    rule: &Rule,
    ws: &CloudWorkspace,
    cards: &[queue::Card],
    complete: bool,
    persona_optional: bool,
) -> Result<(Vec<Found>, bool)> {
    let client = ctx.cloud_client()?;
    client.set_get_retries(RETRIES, RETRY_PAUSE);
    let (host, user) = cloud.identity()?;
    let allow =
        AllowFile::load().map_err(|e| Error::internal(format!("runner allow list: {e}")))?;
    let keys = cloud.keys()?;
    let facts = Facts {
        rule,
        ws,
        allow: &allow,
        keys: &keys,
        host: &host,
        user,
        now: Utc::now(),
        persona_optional,
    };
    let total = cards.len();
    let shared = if total > 0 && rules::is_cloud(rule) {
        Some(queue::fresh_shared(client, ws.id)?)
    } else {
        None
    };
    let server = ctx.server()?;
    let agent = Ctx::user_agent();
    let pool = client.access_only_tokens().map(|tokens| Pool {
        server: &server,
        agent: &agent,
        verbose: ctx.verbose,
        tokens,
    });
    let mut found = check_all(
        client,
        pool.as_ref(),
        rule,
        &facts,
        cards,
        shared.as_ref(),
        &|f| queue::fresh_look(ctx, rule, ws.id, f.id),
    )?;
    unique_keys(&mut found);
    Ok((found, complete))
}

fn check_all(
    client: &Client,
    pool: Option<&Pool>,
    rule: &Rule,
    facts: &Facts,
    cards: &[queue::Card],
    shared: Option<&queue::FreshShared>,
    fallback: &dyn Fn(&Found) -> Result<Fresh>,
) -> Result<Vec<Found>> {
    let total = cards.len();
    let mut prechecked: Vec<Option<Checked>> = (0..total).map(|_| None).collect();
    let mut parallel = false;
    if let (Some(shared), Some(pool)) = (shared, pool) {
        if total > 1 {
            let (slots, stopped) = check_in_parallel(pool, rule, facts, cards, shared);
            if let Some(e) = stopped {
                return Err(unreachable_error(&e));
            }
            prechecked = slots;
            parallel = true;
        }
    }
    let mut err = std::io::stderr();
    let mut found = Vec::with_capacity(total);
    let mut streak = 0;
    for (n, (card, ready)) in cards.iter().zip(prechecked).enumerate() {
        if !parallel && n > 0 && n % 10 == 0 {
            let _ = writeln!(err, "Checked {n} of {total} tasks...");
        }
        let checked = match ready {
            Some(c) if !c.retry => c,
            _ => check_card(client, facts, card, |f| match shared {
                Some(s) => queue::fresh_look_shared(client, rule, f.id, s),
                None => fallback(f),
            }),
        };
        if checked.offline {
            streak += 1;
            if streak >= OFFLINE_STREAK {
                if let Approval::Unchecked(why) = &checked.found.approval {
                    return Err(unreachable_error(&Error::new(Kind::Offline, why)));
                }
            }
        } else if !matches!(checked.found.approval, Approval::Unchecked(_)) {
            streak = 0;
        }
        found.push(checked.found);
    }
    Ok(found)
}

fn clip(s: &str, width: usize) -> String {
    if s.chars().count() <= width {
        return s.to_string();
    }
    let mut t: String = s.chars().take(width.saturating_sub(1)).collect();
    t.push('…');
    t
}

pub fn task_table(found: &[Found]) -> String {
    let mut rows: Vec<Vec<String>> = found
        .iter()
        .take(LIST_CAP)
        .map(|f| {
            vec![
                f.key.clone(),
                clip(&f.title, TITLE_WIDTH),
                f.repos.join(","),
                f.approval.shown(),
            ]
        })
        .collect();
    if found.len() > LIST_CAP {
        rows.push(vec![format!("... and {} more", found.len() - LIST_CAP)]);
    }
    table(&rows)
}

const DRY_RUN_NO_USAGE: &str =
    "No real usage number was found, so the usage limits are not applied. A dry run starts nothing.";

enum UsageGate {
    Limits(Vec<tmgr_usage::guard::GuardLimit>),
    Fallback,
    Unchecked,
}

fn usage_gate(
    dry_run: bool,
    tool: Option<Tool>,
    read: impl FnOnce(Tool) -> Vec<tmgr_usage::guard::GuardLimit>,
) -> UsageGate {
    let found = tool.map(read).unwrap_or_default();
    if usage::has_real_number(&found) {
        UsageGate::Limits(found)
    } else if dry_run {
        UsageGate::Unchecked
    } else {
        UsageGate::Fallback
    }
}

fn apply_gate(
    gate: UsageGate,
    handover: bool,
    answers: &mut Answers,
    tool: Option<Tool>,
    err: &mut dyn Write,
) -> Result<(Vec<tmgr_usage::guard::GuardLimit>, Option<f64>)> {
    match gate {
        UsageGate::Limits(limits) => Ok((limits, None)),
        UsageGate::Unchecked => {
            let _ = writeln!(err, "{DRY_RUN_NO_USAGE}");
            answers.usage_guard = false;
            Ok((Vec::new(), None))
        }
        UsageGate::Fallback => {
            if handover {
                return Err(Error::new(
                    Kind::Conflict,
                    "no real usage number was found and the queue loop is running, so autopilot cannot cap it",
                )
                .hint("turn the queue off first (tmgr queue disable), or pass --no-usage-limit"));
            }
            let hours = answers.fallback_hours;
            let _ = writeln!(err, "{}", fallback_warning(tool, hours));
            answers.usage_guard = false;
            Ok((Vec::new(), Some(hours)))
        }
    }
}

fn fallback_warning(tool: Option<Tool>, hours: f64) -> String {
    format!(
        "No real usage number for {} was found, so the usage guard is off; autopilot stops starting new tasks after {hours} h.",
        tool.map_or("this agent", |t| t.as_str())
    )
}

fn valid_fallback_hours(h: f64) -> bool {
    h.is_finite() && h > 0.0 && h <= 168.0
}

fn parse_fallback_hours(raw: Option<&str>) -> Result<Option<f64>> {
    let Some(raw) = raw else {
        return Ok(None);
    };
    match raw.trim().parse::<f64>() {
        Ok(h) if valid_fallback_hours(h) => Ok(Some(h)),
        _ => Err(Error::usage(
            "--fallback-hours: enter a number of hours above 0 and at most 168",
        )),
    }
}

fn effective_fallback_hours(
    flag: Option<f64>,
    remembered: f64,
    answers_file: &std::path::Path,
) -> Result<f64> {
    let hours = flag.unwrap_or(remembered);
    if valid_fallback_hours(hours) {
        return Ok(hours);
    }
    Err(Error::usage(format!(
        "the remembered fallback of {hours} hours in {} is not a number of hours above 0 and at most 168",
        answers_file.display()
    ))
    .hint("tmgr autopilot --setup, or pass --fallback-hours"))
}

fn remembered_for(remembered: Option<Answers>, rule: &str) -> Option<Answers> {
    remembered.filter(|a| a.rule.as_deref() == Some(rule))
}

fn live_teams(plan: &Plan) -> u32 {
    plan.answers
        .max_teams
        .min(plan.rule.max_concurrent.max(1))
        .min(GLOBAL_MAX as u32)
}

fn capped(max_hours: Option<f64>, fallback: Option<f64>) -> Option<f64> {
    match (max_hours, fallback) {
        (Some(m), Some(f)) => Some(m.min(f)),
        (m, f) => m.or(f),
    }
}

pub fn narrow_cards(cards: Vec<queue::Card>, only: Option<&str>) -> Result<Vec<queue::Card>> {
    let Some(only) = only else {
        return Ok(cards);
    };
    let wanted: Vec<&str> = only
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if wanted.is_empty() {
        return Err(Error::usage("--only: enter task keys separated by commas"));
    }
    let matches = |c: &queue::Card, w: &str| {
        c.key.eq_ignore_ascii_case(w)
            || w.eq_ignore_ascii_case(&format!("task-{}", c.id))
            || w.eq_ignore_ascii_case(&format!("T{}", c.id))
            || w.parse::<i64>() == Ok(c.id)
    };
    for w in &wanted {
        let hits: Vec<&queue::Card> = cards.iter().filter(|c| matches(c, w)).collect();
        match hits.as_slice() {
            [] => {
                return Err(Error::usage(format!(
                    "--only: {w} is not among the tasks this rule would run"
                ))
                .hint("tmgr queue autopilot --dry-run"))
            }
            [_] => {}
            many => {
                let ids: Vec<String> = many.iter().map(|c| format!("task-{}", c.id)).collect();
                return Err(Error::usage(format!(
                    "--only: {w} matches several tasks; use {}",
                    ids.join(" or ")
                )));
            }
        }
    }
    Ok(cards
        .into_iter()
        .filter(|c| wanted.iter().any(|w| matches(c, w)))
        .collect())
}

const ORCHESTRATOR_PROFILE: &str = "claude-code";

fn orchestrator_ready(rule: &Rule) -> Result<()> {
    if !rules::is_cloud(rule) {
        return Ok(());
    }
    let dir = tmgr_runner::profile::user_dir();
    let (set, _) = tmgr_runner::profile::ProfileSet::load(dir.as_deref());
    let path = std::env::var_os("PATH").unwrap_or_default();
    orchestrator_profile_ready(set.get(ORCHESTRATOR_PROFILE), &path)
}

fn orchestrator_profile_ready(
    profile: Option<&tmgr_runner::profile::Profile>,
    path: &std::ffi::OsStr,
) -> Result<()> {
    let missing = || {
        Error::usage(
            "the orchestrator needs the built-in claude-code profile, which is not available",
        )
        .hint("tmgr agents list")
    };
    let profile = profile.ok_or_else(missing)?;
    let argv = tmgr_runner::orchestrator::argv(profile).map_err(|(_, m)| {
        Error::usage(format!("the orchestrator cannot run: {m}")).hint("tmgr agents list")
    })?;
    tmgr_runner::exe::resolve_exe(&argv[0], path, None).map_err(|m| {
        Error::usage(format!("the orchestrator cannot run: {m}"))
            .hint("install Claude Code and put `claude` on PATH")
    })?;
    Ok(())
}

pub fn agent_tool(profile: &str) -> Option<Tool> {
    let dir = tmgr_runner::profile::user_dir();
    let (set, _) = tmgr_runner::profile::ProfileSet::load(dir.as_deref());
    let command = set.lookup(profile).ok()?.command.first()?.clone();
    let name = std::path::Path::new(&command).file_name()?.to_str()?;
    Tool::parse(name).ok()
}

pub struct Plan {
    pub rule: Rule,
    pub ws: CloudWorkspace,
    pub answers: Answers,
    pub chosen: Vec<Found>,
    pub dropped: Vec<Found>,
    pub tool: Option<Tool>,
    pub profile: String,
    pub order: Vec<String>,
    pub dry_run: bool,
    pub fallback_hours: Option<f64>,
    pub persona_optional: bool,
    pub limits: Vec<super::model::LimitRow>,
    pub push: Option<String>,
    pub rules_text: String,
    pub device: String,
    pub now: DateTime<Utc>,
    pub tz: chrono::FixedOffset,
}

impl Plan {
    fn to_sign(&self) -> Vec<&Found> {
        self.chosen.iter().filter(|f| f.sign.is_some()).collect()
    }

    fn run_answers(&self) -> Answers {
        let mut a = self.answers.clone();
        a.max_hours = capped(a.max_hours, self.fallback_hours);
        a
    }
}

fn limit_text(plan: &Plan) -> Vec<String> {
    let a = &plan.run_answers();
    let mut lines = Vec::new();
    if a.usage_guard {
        lines.push(format!(
            "Usage      no new tasks at {}%, running agents paused at {}%",
            a.usage_soft, a.usage_hard
        ));
        for l in &plan.limits {
            lines.push(format!(
                "           now: {}",
                render::limit_part(l, plan.now, plan.tz)
            ));
        }
        lines.push(format!(
            "           after the window resets: {}",
            if a.resume_after_reset {
                "continue"
            } else {
                "stop"
            }
        ));
    } else if plan.dry_run {
        lines.push("Usage      not checked (dry run)".into());
    } else if let Some(h) = plan.fallback_hours {
        lines.push(format!(
            "Usage      no real usage number: guard OFF, new tasks stop after {h} h"
        ));
    } else {
        lines.push("Usage      NO usage limit: the guard is off".into());
    }
    let none = |v: Option<String>| v.unwrap_or_else(|| "none".into());
    lines.push(format!(
        "Caps       cost of API-key runs: {}, hours: {}, tasks started: {}",
        none(a.max_cost_usd.map(|v| format!("${v}"))),
        none(a.max_hours.map(|v| v.to_string())),
        none(a.max_tasks.map(|v| v.to_string()))
    ));
    lines
}

fn shown_text(text: &str, hidden: &mut bool) -> String {
    let (flagged, found) = tmgr_core::text::flag_hidden(text);
    *hidden |= found;
    flagged
        .lines()
        .map(|l| format!("      {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn shown_line(text: &str, hidden: &mut bool) -> String {
    let (flagged, found) = tmgr_core::text::flag_hidden(text);
    *hidden |= found;
    flagged.replace('\n', " ")
}

fn signed_texts(plan: &Plan) -> Vec<String> {
    let mut lines = Vec::new();
    let mut warnings = Vec::new();
    for f in &plan.chosen {
        let Some(sign) = &f.sign else { continue };
        let mut hidden = false;
        lines.push(format!(
            "Signing    {} (task {}) signs exactly this:",
            f.key, sign.id
        ));
        lines.push(format!(
            "    Title: {}",
            shown_line(&sign.title, &mut hidden)
        ));
        lines.push("    Description:".to_string());
        lines.push(shown_text(&sign.description, &mut hidden));
        lines.push(match &sign.offer.repositories {
            Some(list) => format!(
                "    Repositories: {}",
                list.iter()
                    .map(|r| shown_line(r, &mut hidden))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            None => "    Repositories: no repository list signed".to_string(),
        });
        match &sign.offer.persona {
            Some(p) => {
                lines.push(format!(
                    "    Persona instructions (\"{}\"):",
                    shown_line(&p.name, &mut hidden)
                ));
                lines.push(shown_text(&p.instructions, &mut hidden));
            }
            None => lines.push("    Persona instructions: none signed".to_string()),
        }
        match &sign.offer.context {
            Some(pages) => {
                lines.push("    Workspace rules:".to_string());
                for page in pages {
                    lines.push(format!(
                        "      Page {}: {}",
                        page.id,
                        shown_line(&page.title, &mut hidden)
                    ));
                    lines.push(shown_text(&page.body, &mut hidden));
                }
            }
            None => lines.push("    Workspace rules: none signed".to_string()),
        }
        if hidden {
            warnings.push(format!(
                "WARNING    {} has hidden or control characters in what is signed; they are marked [hidden U+XXXX] below",
                f.key
            ));
        }
    }
    warnings.extend(lines);
    warnings
}

const DRY_RUN_TEXT: &str = "Dry run: the orchestrator plans and triages only. Approvals are signed as usual so it can read the signed texts; nothing else changes: no status changes, no comments, no code, the queue stays off.";

fn team_text(plan: &Plan) -> Vec<String> {
    let a = &plan.answers;
    let size = if plan.tool == Some(Tool::Codex) {
        "1 agent per team (codex runs one agent per team)".to_string()
    } else {
        format!("{} agents per team (lead and workers)", a.team_size)
    };
    let mode = match a.team_mode {
        ModeSetting::Orchestrator => "orchestrator (it chooses eco or heavy per task)",
        m => interview::mode_word(m),
    };
    let mut lines = vec![
        format!(
            "Teams      up to {} at the same time, {size}, team mode {mode}",
            live_teams(plan)
        ),
        format!("Profile    the teams run on {}", plan.profile),
        "Orchestrator  runs on claude-code inside the runner sandbox and sees only the signed task texts (no comments, pages or other context); it plans the order and can name tasks that need feedback or are blocked".to_string(),
    ];
    if !plan.order.is_empty() {
        lines.push(format!(
            "Order      {} first, in this order, then the rest",
            plan.order.join(", ")
        ));
    }
    lines
}

const HIDDEN_REASON: &str =
    "hidden characters in its text; run with --review to see it and sign it";
const FIRST_LINE_WIDTH: usize = 60;

fn any_hidden<'a>(texts: impl IntoIterator<Item = &'a str>) -> bool {
    texts.into_iter().any(|t| tmgr_core::text::flag_hidden(t).1)
}

pub fn hidden_in(sign: &SignTask) -> bool {
    let offer = &sign.offer;
    let repos = offer.repositories.iter().flatten().map(String::as_str);
    let persona = offer
        .persona
        .iter()
        .flat_map(|p| [p.name.as_str(), p.instructions.as_str()]);
    let pages = offer
        .context
        .iter()
        .flatten()
        .flat_map(|p| [p.title.as_str(), p.body.as_str()]);
    any_hidden(
        [sign.title.as_str(), sign.description.as_str()]
            .into_iter()
            .chain(repos)
            .chain(persona)
            .chain(pages),
    )
}

fn leave_out_hidden(found: Found, review: bool) -> Found {
    match &found.sign {
        Some(sign) if !review && hidden_in(sign) => Found {
            approval: Approval::Refused(HIDDEN_REASON.into()),
            sign: None,
            ..found
        },
        _ => found,
    }
}

pub fn batch_token(plan: &Plan) -> String {
    format!("{} {}", plan.ws.code, plan.chosen.len())
}

fn tasks(n: usize) -> String {
    format!("{n} task{}", if n == 1 { "" } else { "s" })
}

pub fn batch_prompt(plan: &Plan) -> String {
    let token = batch_token(plan);
    let signing = plan.to_sign().len();
    let run = if plan.dry_run {
        format!("the dry run of {}", tasks(plan.chosen.len()))
    } else {
        format!("{}", plan.chosen.len())
    };
    if signing == 0 {
        format!("Type \"{token}\" to start {run}:")
    } else {
        format!(
            "Type \"{token}\" to sign {} and start {run}:",
            tasks(signing)
        )
    }
}

fn chars_text(n: usize) -> String {
    if n < 1000 {
        format!("{n} chars")
    } else {
        format!("{:.1}k chars", n as f64 / 1000.0)
    }
}

fn preview(sign: &SignTask, hidden: &mut bool) -> String {
    let first = sign
        .description
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty());
    match first {
        Some(line) => format!(
            "\"{}\" ({})",
            clip(&shown_line(line, hidden), FIRST_LINE_WIDTH),
            chars_text(sign.description.chars().count())
        ),
        None => "(no description)".to_string(),
    }
}

pub fn tasks_to_sign(plan: &Plan) -> Vec<SignTask> {
    plan.to_sign()
        .into_iter()
        .filter_map(|f| f.sign.clone())
        .collect()
}

pub fn compact_summary(plan: &Plan, review: bool) -> String {
    let a = &plan.answers;
    let sign = plan.to_sign();
    let mut lines = vec![
        "AUTOPILOT".to_string(),
        format!(
            "Rule       {} (workspace {}, agent profile {})",
            plan.rule.name, plan.ws.code, plan.rule.profile
        ),
        format!(
            "Tasks      {} to work on, {} to sign now",
            plan.chosen.len(),
            sign.len()
        ),
    ];
    for f in &plan.chosen {
        let mut line = format!("           {}  {}  ", f.key, clip(&f.title, TITLE_WIDTH));
        match &f.sign {
            Some(s) => {
                let mut hidden = false;
                line.push_str(&format!("[sign now]  {}", preview(s, &mut hidden)));
            }
            None => line.push_str("[signed]"),
        }
        lines.push(line);
    }
    for f in &plan.dropped {
        lines.push(format!(
            "           {}  left out: {}",
            f.key,
            f.approval.shown()
        ));
    }
    if sign.is_empty() {
        lines.push("Signing    nothing to sign".into());
    } else {
        lines.push(format!(
            "Signing    this computer signs the approvals of the {} tasks marked [sign now] with its own key (kept in the OS keychain, registered as \"{}\") and trusts that key for workspace {}",
            sign.len(),
            plan.device,
            plan.ws.code
        ));
        lines.push(if review {
            "Review     the full text of each of them was shown above".to_string()
        } else {
            "Review     only the first line of each description is shown here; run with --review to read the full texts".to_string()
        });
    }
    if plan.dry_run {
        lines.push(
            "Finish     nothing is started; the plan and the triage are shown in the status".into(),
        );
        lines.push(
            "Stop       `tmgr autopilot stop`; `tmgr autopilot status --watch` shows the plan"
                .into(),
        );
    } else {
        lines.push("Finish     each task ends as a pull request; nothing is merged".into());
        lines.push("Stop       `tmgr autopilot stop` (or `pause`); `tmgr autopilot status --watch` shows progress; stopping turns the queue off".into());
    }
    lines.extend(team_text(plan));
    lines.push(format!(
        "Statuses   feedback \"{}\", blocked \"{}\"",
        sanitize_line(&a.feedback_status),
        sanitize_line(&a.blocked_status)
    ));
    if plan.dry_run {
        lines.push(DRY_RUN_TEXT.into());
    } else {
        lines.push(format!(
            "Agents     turns the queue ON for every rule below and runs agents in the background on this machine, up to {} teams at the same time ({} is the rule's own limit)",
            live_teams(plan),
            plan.rule.max_concurrent
        ));
    }
    lines.push(plan.rules_text.clone());
    lines.extend(limit_text(plan));
    lines.push(match &plan.push {
        Some(note) => format!("Push       {note}"),
        None => "Push       notifications are on".into(),
    });
    lines.join("\n")
}

pub fn confirm_batch(
    summary: &str,
    token: &str,
    prompt: &str,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> Result<()> {
    let _ = writeln!(out, "{}\n\n{summary}\n", queue::WARNING);
    privileged::confirm_typed(token, prompt, input, out)
}

fn confirm_batch_guarded(
    guard: impl FnOnce() -> Result<()>,
    plan: &Plan,
    review: bool,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> Result<()> {
    guard()?;
    if review {
        let texts = signed_texts(plan);
        if !texts.is_empty() {
            queue::page_through_from(&texts, input)?;
        }
    }
    confirm_batch(
        &compact_summary(plan, review),
        &batch_token(plan),
        &batch_prompt(plan),
        input,
        out,
    )
}

fn confirm_batch_stdio(plan: &Plan, review: bool) -> Result<()> {
    confirm_batch_guarded(
        privileged::guard,
        plan,
        review,
        &mut std::io::stdin().lock(),
        &mut std::io::stderr(),
    )
}

fn scope_hint(client: &Client, e: Error) -> Error {
    if e.kind != Kind::Forbidden || !e.message.contains("agent-approvals:write") {
        return e;
    }
    match client.current_device().map(|d| d.scopes) {
        Ok(scopes) if !scopes.is_empty() => e.hint(format!(
            "tmgr login --scopes {},agent-approvals:write",
            scopes.join(",")
        )),
        _ => e,
    }
}

pub struct SignReport {
    pub signed: Vec<i64>,
    pub failed: Vec<(i64, String)>,
}

pub fn sign_chosen(signer: &Signer, client: &Client, tasks: &[SignTask]) -> Result<SignReport> {
    let results: Vec<Signed> = signer
        .sign_tasks(tasks)
        .map_err(|e| scope_hint(client, e))?;
    let mut report = SignReport {
        signed: Vec::new(),
        failed: Vec::new(),
    };
    for r in results {
        if r.ok {
            report.signed.push(r.task_id);
        } else {
            report
                .failed
                .push((r.task_id, r.code.unwrap_or_else(|| "failed".into())));
        }
    }
    report.signed.sort_unstable();
    report.failed.sort();
    Ok(report)
}

fn settings_id(now: DateTime<Utc>) -> String {
    format!("ap-{}", now.format("%Y%m%d-%H%M%S"))
}

fn too_old() -> Error {
    Error::new(
        Kind::Conflict,
        "the running runner is older than this tmgr and cannot host autopilot",
    )
    .hint(format!(
        "{0} runner stop && {0} runner start",
        crate::cmd::runner::bin()
    ))
}

pub fn hosted_client() -> Result<tmgr_runner::client::Client> {
    let cfg = runner_config()?;
    tmgr_runner::daemon::start_background(&cfg).map_err(rerr)?;
    let client = connect(&cfg)?;
    client.set_timeouts(IO_TIMEOUT).map_err(rerr)?;
    if !client.welcome().caps.iter().any(|c| c == AUTOPILOT_V1) {
        return Err(too_old());
    }
    Ok(client)
}

pub fn start_hosted(id: &str) -> Result<u32> {
    hosted_client()?
        .autopilot_start(id)
        .map(|s| s.pid)
        .map_err(rerr)
}

fn answered(e: &std::io::Error, code: &str) -> bool {
    e.to_string()
        .split_once(": ")
        .is_some_and(|(c, _)| c == code)
}

pub fn hand_over() -> Result<()> {
    let mut client = hosted_client()?;
    match client.autopilot_control(AutopilotCommand::Suspend) {
        Ok(_) => {}
        Err(e) if answered(&e, "not_running") => return Ok(()),
        Err(e) => return Err(rerr(e)),
    }
    while client.autopilot_status().map_err(rerr)?.running {
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Ok(())
}

fn running_loop() -> Result<bool> {
    let Some(mut client) = super::control::connect_running()? else {
        return Ok(false);
    };
    if !client.welcome().caps.iter().any(|c| c == AUTOPILOT_V1) {
        return Err(too_old());
    }
    match client.autopilot_status() {
        Ok(s) if s.running && s.status["settings_id"] == QUEUE_SETTINGS_ID => Ok(true),
        Ok(s) if s.running => Err(Error::new(Kind::Conflict, "autopilot is already running")
            .hint("tmgr queue autopilot status")),
        _ => Ok(false),
    }
}

fn push_note(ctx: &Ctx) -> Option<String> {
    let mut warnings = Vec::new();
    match NotifyEnv::from_ctx(ctx).find_existing(&mut warnings) {
        Ok(Some(_)) => None,
        _ => Some(
            "push notifications are off (no notify token); run `tmgr notify` once to turn them on"
                .into(),
        ),
    }
}

pub fn rule_for_workspace(rules: &[Rule], workspace: &str, flag: Option<&str>) -> Result<String> {
    let mine: Vec<&Rule> = rules
        .iter()
        .filter(|r| r.workspace.eq_ignore_ascii_case(workspace))
        .collect();
    let names = mine
        .iter()
        .map(|r| r.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if let Some(f) = flag {
        return mine
            .iter()
            .find(|r| r.name.eq_ignore_ascii_case(f))
            .map(|r| r.name.clone())
            .ok_or_else(|| {
                Error::usage(format!(
                    "--rule: {f} is not a cloud rule of workspace {workspace}"
                ))
                .hint(format!("rules of {workspace}: {names}"))
            });
    }
    match mine.as_slice() {
        [] => Err(
            Error::usage(format!("no queue rule for workspace {workspace}"))
                .hint("tmgr queue rules add --help"),
        ),
        [only] => Ok(only.name.clone()),
        _ => Err(
            Error::usage(format!("several rules for workspace {workspace}: {names}"))
                .hint("--rule NAME"),
        ),
    }
}

pub fn assume_answers(by_workspace: bool, remembered: bool, setup: bool, yes: bool) -> bool {
    !setup && (yes || (by_workspace && remembered))
}

pub fn run(ctx: &Ctx, cli: &Cli, args: &StartArgs, workspace: Option<&str>) -> Result<Output> {
    privileged::guard()?;
    let paths = Paths::from_env()?;
    let store = Store::from_env()?;
    let shown = queue::validated_rules(ctx, &paths)?;
    let cloud_rules: Vec<Rule> = shown
        .file
        .rules
        .iter()
        .filter(|r| rules::is_cloud(r))
        .cloned()
        .collect();
    if cloud_rules.is_empty() && workspace.is_none() {
        return Err(Error::usage("autopilot needs a rule for a cloud workspace")
            .hint("tmgr queue rules add --help"));
    }
    let fallback_flag = parse_fallback_hours(args.fallback_hours.as_deref())?;
    let handover = running_loop()?;
    if args.dry_run && handover {
        return Err(Error::new(
            Kind::Conflict,
            "a dry run cannot start while the queue loop is running",
        )
        .hint("tmgr queue disable"));
    }
    let remembered = store.load_answers()?;
    let stdin = std::io::stdin();
    let mut err = std::io::stderr();
    let names: Vec<String> = cloud_rules.iter().map(|r| r.name.clone()).collect();
    let rule_name = match workspace {
        Some(ws) => rule_for_workspace(&cloud_rules, ws, args.rule.as_deref())?,
        None => interview::choose_rule(
            &names,
            args.rule.as_deref(),
            remembered.as_ref().and_then(|a| a.rule.as_deref()),
            cli.yes,
            &mut stdin.lock(),
            &mut err,
        )?,
    };
    let remembered = remembered_for(remembered, &rule_name);
    let assume = assume_answers(
        workspace.is_some(),
        remembered.is_some(),
        args.setup,
        cli.yes,
    );
    let rule = cloud_rules
        .into_iter()
        .find(|r| r.name == rule_name)
        .ok_or_else(|| Error::internal("the chosen rule disappeared"))?;
    orchestrator_ready(&rule)?;

    let now = Utc::now();
    let guard = !args.no_usage_limit;

    let cloud = CliCloud::new(ctx);
    let ws = cloud
        .allow(&rule.workspace, now)
        .map_err(|m| Error::new(Kind::Conflict, m))?;
    let _ = writeln!(err, "Reading the task list of rule {}...", rule.name);
    let (cards, complete) = cloud.list_cards(&rule, &ws)?;
    let cards = narrow_cards(cards, args.only.as_deref())?;
    let _ = writeln!(err, "Checking {} tasks...", cards.len());
    let (found, complete) = discover(
        ctx,
        &cloud,
        &rule,
        &ws,
        &cards,
        complete,
        !args.with_persona,
    )?;
    let _ = writeln!(err, "Tasks found for rule {}:", rule.name);
    if found.is_empty() {
        return Err(Error::usage(format!(
            "rule {} has no task to run right now",
            rule.name
        )));
    }
    let _ = writeln!(err, "{}", task_table(&found));
    if !complete {
        let _ = writeln!(err, "The list is cut: only the first pages were read.");
    }

    let mut draft = Draft::new(
        remembered,
        found.iter().map(|f| f.key.clone()).collect(),
        rule.max_concurrent,
    )
    .with_rule(&rule.profile, rule.mode.as_deref());
    draft.answers.rule = Some(rule.name.clone());
    draft.answers.usage_guard = guard;
    interview::ask_all(
        &interview::questions(),
        &mut draft,
        &args.flags(),
        assume,
        &mut stdin.lock(),
        &mut err,
    )?;
    draft.answers.fallback_hours = effective_fallback_hours(
        fallback_flag,
        draft.answers.fallback_hours,
        &store.answers_path(),
    )?;
    draft.restrict(None, args.skip.as_deref(), args.order.as_deref())?;
    let profile = draft.profile().to_string();
    let tool = agent_tool(&profile);
    let mut fallback_hours = None;
    let limits = if guard {
        let gate = usage_gate(args.dry_run, tool, |t| usage::read(&usage::Real, t, now));
        let (limits, hours) = apply_gate(gate, handover, &mut draft.answers, tool, &mut err)?;
        fallback_hours = hours;
        limits
    } else {
        Vec::new()
    };

    let (chosen, dropped): (Vec<Found>, Vec<Found>) = found
        .into_iter()
        .filter(|f| draft.is_chosen(&f.key))
        .map(|f| leave_out_hidden(f, args.review))
        .partition(|f| f.approval.runnable());
    if let Some(k) = draft
        .order
        .iter()
        .find(|k| !chosen.iter().any(|f| &f.key == *k))
    {
        return Err(Error::usage(format!(
            "--order: {k} cannot be run, see the reasons in the list above; a task marked \"could not check\" can be retried"
        )));
    }
    if chosen.is_empty() {
        let _ = writeln!(err, "{}", task_table(&dropped));
        return Err(Error::usage("none of the chosen tasks can be run")
            .hint("see the reasons in the list above"));
    }
    let mut answers = draft.answers.clone();
    answers.all_tasks = draft.picked.is_none() && args.only.is_none() && args.skip.is_none();
    let soft = answers.usage_guard.then_some(answers.usage_soft);
    let hard = answers.usage_guard.then_some(answers.usage_hard);
    let plan = Plan {
        rule,
        ws,
        answers,
        chosen,
        dropped,
        tool,
        profile,
        order: draft.order.clone(),
        dry_run: args.dry_run,
        fallback_hours,
        persona_optional: !args.with_persona,
        limits: usage::rows(&limits, soft, hard),
        push: push_note(ctx),
        rules_text: queue::rules_summary(&shown.file),
        device: device_name(),
        now,
        tz: super::control::local_offset(),
    };
    confirm_batch_stdio(&plan, args.review)?;
    begin(
        ctx,
        &paths,
        &store,
        &plan,
        shown.sha256.as_deref(),
        handover,
    )
}

fn begin(
    ctx: &Ctx,
    paths: &Paths,
    store: &Store,
    plan: &Plan,
    shown_sha: Option<&str>,
    handover: bool,
) -> Result<Output> {
    if handover {
        hand_over()?;
    }
    let started = begin_after_handover(ctx, paths, store, plan, shown_sha, handover);
    if started.is_err() && handover {
        let _ = super::queue_loop::ensure_running(paths);
    }
    started
}

fn begin_after_handover(
    ctx: &Ctx,
    paths: &Paths,
    store: &Store,
    plan: &Plan,
    shown_sha: Option<&str>,
    handover: bool,
) -> Result<Output> {
    let client = ctx.cloud_client()?;
    let cloud = CliCloud::new(ctx);
    let (host, user) = cloud.identity()?;
    let to_sign = tasks_to_sign(plan);
    let mut report = SignReport {
        signed: Vec::new(),
        failed: Vec::new(),
    };
    if !to_sign.is_empty() {
        let signer = Signer::new(client, &ctx.server()?, user)?;
        let own = signer.ensure_key().map_err(|e| scope_hint(client, e))?;
        report = sign_chosen(&signer, client, &to_sign)?;
        if !report.signed.is_empty() {
            crate::cmd::runner::allow::pin_own_key(&host, user, plan.ws.id, &own, &plan.device)?;
        }
    }
    let tasks: BTreeSet<i64> = plan
        .chosen
        .iter()
        .filter(|f| f.sign.is_none() || report.signed.contains(&f.id))
        .map(|f| f.id)
        .collect();
    if tasks.is_empty() {
        return Err(Error::new(Kind::Conflict, "no task could be approved")
            .hint(failure_text(&report, &plan.chosen)));
    }
    if !plan.dry_run {
        queue::commit_enable(paths, shown_sha)?;
    }
    match launch(paths, store, plan, &tasks, &report, handover) {
        Ok(out) => Ok(attach_failures(out, &report, &plan.chosen)),
        Err(e) => {
            if !handover && !plan.dry_run {
                let _ = tmgr_queue::switch::disable(paths, Some("autopilot did not start"));
            }
            Err(e)
        }
    }
}

fn launch(
    paths: &Paths,
    store: &Store,
    plan: &Plan,
    tasks: &BTreeSet<i64>,
    report: &SignReport,
    handover: bool,
) -> Result<Output> {
    store.save_answers(&plan.answers)?;
    let id = settings_id(Utc::now());
    let order: Vec<i64> = plan
        .order
        .iter()
        .filter_map(|k| plan.chosen.iter().find(|f| &f.key == k))
        .map(|f| f.id)
        .filter(|id| tasks.contains(id))
        .collect();
    let settings = Settings {
        id: id.clone(),
        created_at: Utc::now(),
        answers: plan.run_answers(),
        rule: plan.rule.name.clone(),
        agent: plan
            .tool
            .map_or_else(|| plan.profile.clone(), |t| t.as_str().to_string()),
        mode: "auto".into(),
        finish: Finish::PrOnly,
        tasks: tasks.clone(),
        signed: report.signed.clone(),
        windows: if plan.answers.usage_guard {
            plan.limits.iter().map(|l| l.window.clone()).collect()
        } else {
            Vec::new()
        },
        kind: LoopKind::Drain,
        resume_queue: handover,
        order,
        dry_run: plan.dry_run,
        persona_optional: plan.persona_optional,
    };
    store.save_settings(&settings)?;
    if !plan.dry_run {
        tmgr_queue::switch::forget_queue_loop(paths);
    }
    let pid = start_hosted(&id)?;
    let human = if plan.dry_run {
        format!(
            "Dry run started ({} tasks). Watch the plan with `tmgr queue autopilot status --watch`; stop it with `tmgr queue autopilot stop`.",
            tasks.len()
        )
    } else {
        format!(
            "Autopilot started ({} tasks). Watch it with `tmgr queue autopilot status --watch`; stop it with `tmgr queue autopilot stop`.",
            tasks.len()
        )
    };
    Ok(Output::new(
        json!({"id": id, "pid": pid, "tasks": tasks, "signed": report.signed, "dry_run": plan.dry_run}),
        human,
    ))
}

fn failure_text(report: &SignReport, chosen: &[Found]) -> String {
    report
        .failed
        .iter()
        .map(|(id, code)| {
            let key = chosen
                .iter()
                .find(|f| f.id == *id)
                .map_or_else(|| format!("task-{id}"), |f| f.key.clone());
            format!("{key}: {}", sanitize_line(code))
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn attach_failures(mut out: Output, report: &SignReport, chosen: &[Found]) -> Output {
    if report.failed.is_empty() {
        return out;
    }
    let text = failure_text(report, chosen);
    out.human = format!("{}\nNot signed, left out of the run: {text}", out.human);
    out.data["unsigned"] = json!(report.failed.iter().map(|(id, _)| id).collect::<Vec<_>>());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use serde_json::{json, Value};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use tmgr_core::approval::{content_sha256, Entry, PinnedKey};
    use tmgr_core::creds::Keychain;
    use tmgr_core::redact::Secret;
    use wiremock::matchers::{method, path, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 19, 20, 0, 0).unwrap()
    }

    fn fake_claude(dir: &std::path::Path) {
        use std::os::unix::fs::PermissionsExt;
        let exe = dir.join("claude");
        std::fs::write(&exe, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn the_orchestrator_needs_the_builtin_claude_code_profile_and_claude_on_the_path() {
        let (set, _) = tmgr_runner::profile::ProfileSet::load(None);
        let builtin = set.get("claude-code").unwrap();
        let bin = tempfile::tempdir().unwrap();
        let empty = tempfile::tempdir().unwrap();
        assert!(orchestrator_profile_ready(Some(builtin), empty.path().as_os_str()).is_err());
        fake_claude(bin.path());
        assert!(orchestrator_profile_ready(Some(builtin), bin.path().as_os_str()).is_ok());
        assert!(orchestrator_profile_ready(None, bin.path().as_os_str()).is_err());
        let mut user = builtin.clone();
        user.source = tmgr_runner::profile::Source::User("/x/claude-code.toml".into());
        assert!(orchestrator_profile_ready(Some(&user), bin.path().as_os_str()).is_err());
        let mut bare = builtin.clone();
        bare.headless = None;
        assert!(orchestrator_profile_ready(Some(&bare), bin.path().as_os_str()).is_err());
    }

    fn rule() -> Rule {
        Rule {
            name: "tm".into(),
            workspace: "TM".into(),
            project: Some("tm".into()),
            status_type: "backlog".into(),
            persona: "dev".into(),
            profile: "claude".into(),
            model: None,
            repos: vec!["/tmp/repo".into()],
            max_concurrent: 2,
            hours: None,
            trusted_authors: vec!["human".into()],
            auto: true,
            mode: None,
        }
    }

    fn ws() -> CloudWorkspace {
        CloudWorkspace {
            id: 7,
            code: "TM".into(),
            repos: vec![],
        }
    }

    fn elig(extra: Value) -> Eligibility {
        let mut v = json!({
            "task_id": 11, "workspace_id": 7, "workspace_code": "TM", "viewer_user_id": 1,
            "title": "Fix it", "description": "body", "task_key": "TM-11",
            "setting_enabled": true,
            "category": {"id": 5, "code": "tm", "github_full_name": "o/api", "repositories": null},
        });
        for (k, val) in extra.as_object().unwrap() {
            v[k] = val.clone();
        }
        parse_eligibility(v).unwrap()
    }

    fn signable() -> Found {
        Found {
            id: 7,
            key: "TM-7".into(),
            title: "t".into(),
            repos: vec![],
            approval: Approval::NeedsSigning("not signed".into()),
            sign: Some(SignTask {
                id: 7,
                workspace_id: 1,
                category_id: 2,
                title: "t".into(),
                description: String::new(),
                repos_sha256: None,
                offer: Offer::default(),
            }),
        }
    }

    fn fresh(personas_ok: bool, holders: bool) -> Fresh {
        Fresh {
            holders: if holders {
                vec![tmgr_queue::taken::Holder {
                    name: "Anna".into(),
                    kind: "person",
                }]
            } else {
                vec![]
            },
            status_ok: true,
            has_persona: personas_ok,
        }
    }

    #[test]
    fn a_task_without_the_rule_persona_is_refused_and_never_signed() {
        let f = not_free(signable(), false, |_| Ok(fresh(false, false)));
        assert!(f.sign.is_none());
        assert!(matches!(&f.approval, Approval::Refused(w) if w.contains("persona")));
    }

    #[test]
    fn a_taken_task_is_refused_and_a_free_one_stays_signable() {
        let f = not_free(signable(), false, |_| Ok(fresh(true, true)));
        assert!(matches!(&f.approval, Approval::Refused(w) if w.contains("taken")));
        assert!(not_free(signable(), false, |_| Ok(fresh(true, false)))
            .sign
            .is_some());
    }

    fn classify_with(e: &Eligibility, allow: &AllowFile) -> Found {
        let r = rule();
        let w = ws();
        classify(
            11,
            e,
            &Facts {
                rule: &r,
                ws: &w,
                allow,
                keys: &[],
                host: "tmgr.dev",
                user: 1,
                now: now(),
                persona_optional: false,
            },
        )
    }

    fn allow_with_binding() -> AllowFile {
        let mut file = AllowFile::default();
        let mut entry = Entry {
            server: "tmgr.dev".into(),
            user_id: 1,
            workspace_id: 7,
            workspace_code: "TM".into(),
            allowed_at: "2026-10-01T00:00:00Z".into(),
            expires_at: None,
            keys: Vec::new(),
            repos: Default::default(),
        };
        entry.keys.push(PinnedKey {
            key_id: "k".into(),
            public_key: "p".into(),
            device_name: "d".into(),
        });
        file.entries.push(entry);
        file
    }

    #[test]
    fn unfixable_tasks_are_refused_not_offered_for_signing() {
        let allow = allow_with_binding();
        let f = classify_with(&elig(json!({"setting_enabled": false})), &allow);
        assert!(
            matches!(&f.approval, Approval::Refused(r) if r.contains("off for this workspace"))
        );
        assert!(f.sign.is_none());
        let f = classify_with(&elig(json!({"viewer_user_id": 2})), &allow);
        assert!(matches!(f.approval, Approval::Refused(_)));
        let f = classify_with(&elig(json!({"category": null})), &allow);
        assert!(matches!(&f.approval, Approval::Refused(r) if r.contains("no category")));
        let f = classify_with(&elig(json!({"workspace_id": 9})), &allow);
        assert!(matches!(f.approval, Approval::Refused(_)));
        let f = classify_with(&elig(json!({})), &AllowFile::default());
        assert!(matches!(&f.approval, Approval::Refused(r) if r.contains("not allowed")));
    }

    #[test]
    fn a_repository_the_rule_does_not_cover_is_refused() {
        let f = classify_with(&elig(json!({})), &allow_with_binding());
        assert!(
            matches!(&f.approval, Approval::Refused(r) if r.contains("no repository is bound") || r.contains("repository")),
            "{:?}",
            f.approval
        );
    }

    #[test]
    fn the_table_lists_key_title_repos_and_state_and_caps_long_lists() {
        let mk = |n: i64, approval| Found {
            id: n,
            key: format!("TM-{n}"),
            title: "T".repeat(80),
            repos: vec!["o/api".into(), "o/web".into()],
            approval,
            sign: None,
        };
        let one = task_table(&[
            mk(1, Approval::Valid),
            mk(2, Approval::NeedsSigning("x".into())),
            mk(3, Approval::Refused("agent runs are off".into())),
        ]);
        assert!(one.contains("TM-1"));
        assert!(one.contains("o/api,o/web"));
        assert!(one.contains("signed"));
        assert!(one.contains("needs signing"));
        assert!(one.contains("refused: agent runs are off"));
        assert!(one.contains('…'));
        let many: Vec<Found> = (0..45).map(|n| mk(n, Approval::Valid)).collect();
        assert!(task_table(&many).contains("... and 5 more"));
    }

    fn plan(sign: bool, guard: bool) -> Plan {
        let found = |n: i64, sign: bool| Found {
            id: n,
            key: format!("TM-{n}"),
            title: format!("Task {n}"),
            repos: vec!["o/api".into()],
            approval: if sign {
                Approval::NeedsSigning("no approval".into())
            } else {
                Approval::Valid
            },
            sign: sign.then(|| SignTask {
                id: n,
                workspace_id: 7,
                category_id: 5,
                title: "t".into(),
                description: String::new(),
                repos_sha256: None,
                offer: Offer::default(),
            }),
        };
        Plan {
            rule: rule(),
            ws: ws(),
            answers: Answers {
                usage_guard: guard,
                max_cost_usd: Some(20.0),
                ..Answers::default()
            },
            chosen: vec![found(1, false), found(2, sign)],
            dropped: vec![Found {
                approval: Approval::Refused("agent runs are off".into()),
                ..found(3, false)
            }],
            tool: Some(Tool::Claude),
            profile: "claude-code".into(),
            order: vec![],
            dry_run: false,
            fallback_hours: None,
            persona_optional: true,
            limits: vec![],
            push: Some("push notifications are off".into()),
            rules_text: "Rules:\n  tm: TM project tm".into(),
            device: "CLI (macOS)".into(),
            now: now(),
            tz: chrono::FixedOffset::east_opt(0).unwrap(),
        }
    }

    #[test]
    fn the_summary_names_what_the_agreement_covers() {
        let s = compact_summary(&plan(true, true), false);
        for needle in [
            "TM-1  Task 1  [signed]",
            "TM-2  Task 2  [sign now]",
            "TM-3  left out: refused: agent runs are off",
            "signs the approvals of the 1 tasks",
            "turns the queue ON",
            "background on this machine, up to 2 teams at the same time",
            "Teams      up to 2 at the same time, 3 agents per team (lead and workers), team mode orchestrator (it chooses eco or heavy per task)",
            "Profile    the teams run on claude-code",
            "runs on claude-code inside the runner sandbox and sees only the signed task texts",
            "tm: TM project tm",
            "no new tasks at 50%, running agents paused at 80%",
            "cost of API-key runs: $20, hours: none, tasks started: none",
            "push notifications are off",
            "pull request; nothing is merged",
            "tmgr autopilot stop",
        ] {
            assert!(s.contains(needle), "missing {needle:?} in\n{s}");
        }
        let s = compact_summary(&plan(false, false), false);
        assert!(s.contains("nothing to sign"));
        assert!(s.contains("NO usage limit"));
    }

    fn gate_run(
        dry_run: bool,
        tool: Option<Tool>,
        handover: bool,
        answers: &mut Answers,
    ) -> (Result<Option<f64>>, String) {
        let gate = usage_gate(dry_run, tool, |_| Vec::new());
        let mut out = Vec::new();
        let r = apply_gate(gate, handover, answers, tool, &mut out).map(|(_, h)| h);
        (r, String::from_utf8(out).unwrap())
    }

    #[test]
    fn a_dry_run_needs_no_usage_number_and_stays_unchecked() {
        for tool in [Some(Tool::Claude), None] {
            let mut a = Answers::default();
            let (r, said) = gate_run(true, tool, false, &mut a);
            assert_eq!(r.unwrap(), None);
            assert!(!a.usage_guard);
            assert!(said.contains("A dry run starts nothing"));
        }
    }

    #[test]
    fn no_number_falls_back_to_a_time_limit_instead_of_an_error() {
        let mut a = Answers::default();
        let (r, said) = gate_run(false, Some(Tool::Claude), false, &mut a);
        assert_eq!(r.unwrap(), Some(8.0));
        assert!(!a.usage_guard);
        assert_eq!(
            said.trim(),
            "No real usage number for claude was found, so the usage guard is off; autopilot stops starting new tasks after 8 h."
        );
        let mut a = Answers {
            fallback_hours: 3.0,
            ..Answers::default()
        };
        assert_eq!(
            gate_run(false, Some(Tool::Codex), false, &mut a).0.unwrap(),
            Some(3.0)
        );
        let mut a = Answers::default();
        let (r, said) = gate_run(false, None, false, &mut a);
        assert_eq!(r.unwrap(), Some(8.0));
        assert!(said.contains("for this agent"));
    }

    #[test]
    fn the_fallback_is_refused_when_a_running_queue_loop_would_resume_uncapped() {
        let mut a = Answers::default();
        let (r, _) = gate_run(false, Some(Tool::Claude), true, &mut a);
        let e = r.unwrap_err();
        assert!(e.message.contains("cannot cap it"), "{}", e.message);
        let hint = e.hint.unwrap();
        assert!(hint.contains("tmgr queue disable") && hint.contains("--no-usage-limit"));
        let mut a = Answers::default();
        assert_eq!(gate_run(true, None, true, &mut a).0.unwrap(), None);
    }

    #[test]
    fn the_cap_applies_to_the_run_and_never_to_the_remembered_answers() {
        for (own, fallback, want) in [
            (None, Some(8.0), Some(8.0)),
            (Some(2.0), Some(8.0), Some(2.0)),
            (Some(20.0), Some(8.0), Some(8.0)),
            (Some(20.0), None, Some(20.0)),
            (None, None, None),
        ] {
            let mut p = plan(false, false);
            p.answers.max_hours = own;
            p.fallback_hours = fallback;
            assert_eq!(p.run_answers().max_hours, want);
            assert_eq!(p.answers.max_hours, own);
        }
    }

    #[test]
    fn the_summary_shows_the_fallback_and_the_effective_hours() {
        let mut p = plan(false, false);
        p.fallback_hours = Some(8.0);
        let s = compact_summary(&p, false);
        assert!(s.contains("no real usage number: guard OFF, new tasks stop after 8 h"));
        assert!(s.contains("hours: 8,"));
        assert!(!s.contains("NO usage limit"));
    }

    #[test]
    fn fallback_hours_are_validated() {
        assert_eq!(parse_fallback_hours(None).unwrap(), None);
        assert_eq!(parse_fallback_hours(Some("3")).unwrap(), Some(3.0));
        assert_eq!(parse_fallback_hours(Some("168")).unwrap(), Some(168.0));
        for bad in ["0", "-1", "169", "abc", "NaN", "inf", ""] {
            assert!(parse_fallback_hours(Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_remembered_fallback_is_validated_like_the_flag() {
        let path = std::path::Path::new("answers.json");
        assert_eq!(effective_fallback_hours(None, 8.0, path).unwrap(), 8.0);
        assert_eq!(effective_fallback_hours(Some(3.0), 0.0, path).unwrap(), 3.0);
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY, 169.0, 1e9] {
            let e = effective_fallback_hours(None, bad, path).unwrap_err();
            assert_eq!(e.kind, Kind::Usage, "{bad}");
            assert!(e.message.contains("answers.json"), "{}", e.message);
            let hint = e.hint.unwrap();
            assert!(hint.contains("--setup") && hint.contains("--fallback-hours"));
        }
    }

    #[test]
    fn remembered_answers_of_another_rule_are_not_reused() {
        let mine = Answers {
            rule: Some("a".into()),
            ..Answers::default()
        };
        assert_eq!(remembered_for(Some(mine.clone()), "a"), Some(mine.clone()));
        assert_eq!(remembered_for(Some(mine.clone()), "b"), None);
        assert_eq!(remembered_for(Some(Answers::default()), "a"), None);
        assert_eq!(remembered_for(None, "a"), None);
    }

    #[test]
    fn the_summary_shows_the_status_names_and_the_effective_team_limit() {
        let mut p = plan(false, false);
        p.answers.feedback_status = "Waiting".into();
        p.answers.blocked_status = "Stuck".into();
        p.answers.max_teams = 9;
        p.rule.max_concurrent = 9;
        let s = compact_summary(&p, false);
        assert!(s.contains("feedback \"Waiting\", blocked \"Stuck\""), "{s}");
        assert!(s.contains("Teams      up to 3 at the same time"), "{s}");
        assert!(
            s.contains("up to 3 teams at the same time (9 is the rule's own limit)"),
            "{s}"
        );
    }

    #[test]
    fn the_persona_is_optional_only_when_the_flag_allows_it() {
        let optional = not_free(signable(), true, |_| Ok(fresh(false, false)));
        assert!(optional.approval.runnable());
        let required = not_free(signable(), false, |_| Ok(fresh(false, false)));
        assert!(matches!(&required.approval, Approval::Refused(w) if w.contains("persona")));
        let taken = not_free(signable(), true, |_| Ok(fresh(false, true)));
        assert!(matches!(&taken.approval, Approval::Refused(w) if w.contains("taken")));
    }

    fn found_with(id: i64, key: &str) -> Found {
        Found {
            id,
            key: key.into(),
            title: String::new(),
            repos: Vec::new(),
            approval: Approval::Valid,
            sign: None,
        }
    }

    #[test]
    fn colliding_keys_fall_back_to_the_task_id() {
        let mut f = vec![
            found_with(1, "TM-210"),
            found_with(2, "tm-210"),
            found_with(3, "TM-211"),
        ];
        unique_keys(&mut f);
        let keys: Vec<&str> = f.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, ["task-1", "task-2", "TM-211"]);
    }

    #[test]
    fn a_relabel_that_collides_with_an_existing_key_is_relabelled_again() {
        let mut f = vec![
            found_with(1, "TM-210"),
            found_with(2, "TM-210"),
            found_with(3, "task-1"),
        ];
        unique_keys(&mut f);
        let keys: Vec<&str> = f.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, ["task-1", "task-2", "task-3"]);
    }

    #[test]
    fn keys_differing_only_in_case_get_distinct_labels() {
        let mut f = vec![found_with(1, "TM-1"), found_with(2, "tm-1")];
        unique_keys(&mut f);
        let keys: Vec<&str> = f.iter().map(|f| f.key.as_str()).collect();
        assert_eq!(keys, ["task-1", "task-2"]);
    }

    fn card(id: i64, key: &str) -> queue::Card {
        queue::Card {
            id,
            key: key.into(),
        }
    }

    #[test]
    fn only_narrows_the_cards_before_anything_is_fetched() {
        let cards = vec![card(1, "TM-430"), card(2, "TM-431"), card(3, "TM-432")];
        let kept = narrow_cards(cards.clone(), Some("tm-430, TM-432")).unwrap();
        assert_eq!(kept.iter().map(|c| c.id).collect::<Vec<_>>(), [1, 3]);
        assert_eq!(narrow_cards(cards.clone(), None).unwrap().len(), 3);
        let e = narrow_cards(cards, Some("TM-430,TM-999")).unwrap_err();
        assert!(e.message.contains("TM-999"), "{}", e.message);
        assert_eq!(e.exit_code(), 2);
    }

    #[test]
    fn only_with_a_duplicated_key_names_the_ids_to_use() {
        let cards = vec![card(7, "TM-210"), card(8, "TM-210"), card(9, "TM-1")];
        let e = narrow_cards(cards.clone(), Some("TM-210")).unwrap_err();
        assert!(e.message.contains("task-7 or task-8"), "{}", e.message);
        let kept = narrow_cards(cards, Some("task-8")).unwrap();
        assert_eq!(kept.iter().map(|c| c.id).collect::<Vec<_>>(), [8]);
    }

    #[test]
    fn an_unchecked_task_is_not_runnable_and_not_called_refused() {
        let a = Approval::Unchecked("cannot reach api".into());
        assert!(!a.runnable());
        let shown = a.shown();
        assert!(shown.starts_with("could not check, retry"), "{shown}");
        assert!(!shown.contains("refused"));
        assert!(Approval::Valid.runnable());
        assert!(Approval::NeedsSigning("x".into()).runnable());
        assert!(!Approval::Refused("x".into()).runnable());
    }

    #[test]
    fn a_dry_run_summary_does_not_claim_the_guard_is_off_by_choice() {
        let mut p = plan(false, false);
        p.dry_run = true;
        let s = compact_summary(&p, false);
        assert!(s.contains("not checked (dry run)"), "{s}");
        assert!(!s.contains("NO usage limit"));
    }

    #[test]
    fn the_summary_lists_teams_profile_order_and_mode() {
        let mut p = plan(false, true);
        p.answers.max_teams = 1;
        p.answers.team_size = 2;
        p.answers.team_mode = ModeSetting::Heavy;
        p.profile = "codex".into();
        p.tool = Some(Tool::Codex);
        p.order = vec!["TM-2".into(), "TM-1".into()];
        let s = compact_summary(&p, false);
        for needle in [
            "Teams      up to 1 at the same time, 1 agent per team (codex runs one agent per team), team mode heavy",
            "Profile    the teams run on codex",
            "Order      TM-2, TM-1 first, in this order, then the rest",
            "up to 1 teams at the same time",
        ] {
            assert!(s.contains(needle), "missing {needle:?} in\n{s}");
        }
        assert!(!compact_summary(&plan(false, true), false).contains("Order "));
    }

    #[test]
    fn the_dry_run_summary_says_nothing_but_planning_happens() {
        let mut p = plan(true, true);
        p.dry_run = true;
        let s = compact_summary(&p, false);
        for needle in [
            "Dry run: the orchestrator plans and triages only. Approvals are signed as usual so it can read the signed texts; nothing else changes: no status changes, no comments, no code, the queue stays off.",
            "signs the approvals of the 1 tasks",
            "tmgr autopilot status --watch",
        ] {
            assert!(s.contains(needle), "missing {needle:?} in\n{s}");
        }
        assert!(!s.contains("turns the queue ON"));
        assert!(!s.contains("stopping turns the queue off"));
    }

    #[test]
    fn the_summary_shows_everything_that_gets_signed_without_clipping() {
        use crate::cmd::signer::{OfferPage, OfferPersona};
        let mut p = plan(true, true);
        let sign = p.chosen[1].sign.as_mut().unwrap();
        sign.title = "Fix login".into();
        sign.description = format!("Line one\r\nLine two\n{}", "d".repeat(5000));
        sign.offer = Offer {
            repositories: Some(vec!["acme/api".into(), "acme/web".into()]),
            persona: Some(OfferPersona {
                id: "993c30a6-3528-42bf-9a70-507841cebc08".into(),
                name: "Agent".into(),
                instructions: format!("Follow the rules.\nEnd {}", "x".repeat(3000)),
            }),
            context: Some(vec![OfferPage {
                id: 1,
                title: "Team rules".into(),
                body: "Open a PR.".into(),
            }]),
        };
        let s = signed_texts(&p).join("\n");
        for needle in [
            "TM-2 (task 2) signs exactly this",
            "Title: Fix login",
            "      Line two",
            &"d".repeat(5000),
            "Repositories: acme/api, acme/web",
            "Persona instructions (\"Agent\")",
            "      Follow the rules.",
            &"x".repeat(3000),
            "Page 1: Team rules",
            "      Open a PR.",
        ] {
            assert!(s.contains(needle), "missing {needle:?}");
        }
        assert!(!s.contains("WARNING"));
        assert!(!s.contains('\r'));
        let plain = signed_texts(&plan(true, true)).join("\n");
        assert!(plain.contains("Repositories: no repository list signed"));
        assert!(plain.contains("Persona instructions: none signed"));
    }

    #[test]
    fn hidden_characters_are_marked_and_named_in_a_warning() {
        let mut p = plan(true, true);
        let sign = p.chosen[1].sign.as_mut().unwrap();
        sign.description = "ok\u{2028}bad \u{1b}[31m\u{e0041}".into();
        sign.offer.persona = Some(crate::cmd::signer::OfferPersona {
            id: "993c30a6-3528-42bf-9a70-507841cebc08".into(),
            name: "A\u{202e}B".into(),
            instructions: "fine".into(),
        });
        let s = signed_texts(&p).join("\n");
        for needle in [
            "WARNING    TM-2 has hidden or control characters",
            "[hidden U+2028]",
            "[hidden U+001B]",
            "[hidden U+E0041]",
            "[hidden U+202E]",
        ] {
            assert!(s.contains(needle), "missing {needle:?} in\n{s}");
        }
        assert!(!s.contains('\u{1b}') && !s.contains('\u{2028}'));
    }

    #[test]
    fn settings_ids_are_valid_for_the_runner() {
        let id = settings_id(now());
        assert_eq!(id, "ap-20261019-200000");
        assert!(tmgr_runner::autopilot::valid_settings_id(&id));
    }

    #[derive(Clone, Default)]
    struct Fake(Arc<Mutex<HashMap<String, String>>>);

    impl Keychain for Fake {
        fn get(&self, account: &str) -> keyring::Result<String> {
            self.0
                .lock()
                .unwrap()
                .get(account)
                .cloned()
                .ok_or(keyring::Error::NoEntry)
        }
        fn set(&self, account: &str, blob: &str) -> keyring::Result<()> {
            self.0.lock().unwrap().insert(account.into(), blob.into());
            Ok(())
        }
        fn delete(&self, account: &str) -> keyring::Result<()> {
            self.0.lock().unwrap().remove(account);
            Ok(())
        }
    }

    fn client(uri: &str) -> Client {
        let tokens = tmgr_core::creds::Tokens {
            access: Secret::new("acc"),
            refresh: None,
            expires_at: None,
        };
        Client::new(uri, "test", Some(tokens), None, false).unwrap()
    }

    async fn on_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
        tokio::task::spawn_blocking(f).await.unwrap()
    }

    fn sign_task(id: i64) -> SignTask {
        SignTask {
            id,
            workspace_id: 7,
            category_id: 5,
            title: "Fix it".into(),
            description: "body".into(),
            repos_sha256: None,
            offer: Offer::default(),
        }
    }

    #[test]
    fn content_hash_helper_is_the_gate_one() {
        assert_eq!(content_sha256("a", "b"), content_sha256("a", "b"));
    }

    #[tokio::test]
    async fn a_missing_scope_explains_how_to_log_in_again() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/agent-runs/keys"))
            .respond_with(
                ResponseTemplate::new(403).set_body_json(json!({"error": "insufficient_scope"})),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/auth/devices/current"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "data": {"user": {"id": 1}, "scopes": ["tasks:read", "tasks:write"]}
            })))
            .mount(&server)
            .await;
        let uri = server.uri();
        let e = on_thread(move || {
            let c = client(&uri);
            let signer = Signer::with_keychain(&c, &uri, 1, Box::new(Fake::default()), false)
                .unwrap()
                .with_guard(|| Ok(()));
            signer
                .ensure_key()
                .map_err(|e| scope_hint(&c, e))
                .err()
                .unwrap()
        })
        .await;
        assert_eq!(e.kind, Kind::Forbidden);
        assert!(e.message.contains("agent-approvals:write"), "{}", e.message);
        assert_eq!(
            e.hint.as_deref(),
            Some("tmgr login --scopes tasks:read,tasks:write,agent-approvals:write")
        );
    }

    #[tokio::test]
    async fn signing_reports_each_task_and_drops_the_failed_ones() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/agent-runs/keys"))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"key_id": "k1"})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(r"^/api/tasks/\d+/agent-approval$"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"data": {"repositories": null}})),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/agent-runs/approvals/bulk"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [
                {"task_id": 1, "ok": true},
                {"task_id": 2, "ok": false, "code": "content_changed"},
            ]})))
            .mount(&server)
            .await;
        let uri = server.uri();
        let report = on_thread(move || {
            let c = client(&uri);
            let signer = Signer::with_keychain(&c, &uri, 1, Box::new(Fake::default()), false)
                .unwrap()
                .with_guard(|| Ok(()));
            sign_chosen(&signer, &c, &[sign_task(1), sign_task(2)]).unwrap()
        })
        .await;
        assert_eq!(report.signed, vec![1]);
        assert_eq!(report.failed, vec![(2, "content_changed".to_string())]);
        assert_eq!(failure_text(&report, &[]), "task-2: content_changed");
    }

    fn bound_allow() -> AllowFile {
        let mut allow = allow_with_binding();
        allow.entries[0].repos.insert(
            "5".into(),
            tmgr_core::approval::RepoBinding::Path("/tmp/repo".into()),
        );
        allow
    }

    fn cards(n: i64) -> Vec<queue::Card> {
        (1..=n)
            .map(|id| queue::Card {
                id,
                key: format!("TM-{id}"),
            })
            .collect()
    }

    fn answer(req: &wiremock::Request) -> ResponseTemplate {
        let auth = req
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default()
            .to_string();
        if auth != "Bearer acc" {
            return ResponseTemplate::new(401).set_body_json(json!({"error": "unauthenticated"}));
        }
        let path = req.url.path().to_string();
        let id: i64 = path
            .split("tasks/")
            .nth(1)
            .and_then(|rest| rest.split('/').next())
            .and_then(|n| n.parse().ok())
            .unwrap_or(0);
        let data = if path.starts_with("/api/agent-runs/tasks/") {
            let mut e = json!({
                "task_id": id, "workspace_id": 7, "workspace_code": "TM", "viewer_user_id": 1,
                "title": format!("Task {id}"), "description": "body", "task_key": format!("TM-{id}"),
                "setting_enabled": true,
                "category": {"id": 5, "code": "tm", "github_full_name": "o/api", "repositories": null},
            });
            e["task_id"] = json!(id);
            e
        } else if path.ends_with("/statuses") {
            json!([{"id": 1, "name": "Backlog", "type": "default"}])
        } else if path.ends_with("/agent-work") {
            json!([])
        } else if path.ends_with("/agent-approval") {
            json!({"repositories": null})
        } else if path == "/api/auth/devices/current" {
            json!({"user": {"id": 1}})
        } else {
            json!({"id": id, "title": format!("Task {id}"), "status_id": 1,
                   "persona_assignees": [{"name": "dev"}], "assignees": []})
        };
        ResponseTemplate::new(200)
            .set_body_json(json!({"data": data}))
            .set_delay(std::time::Duration::from_millis(
                (9 - id.min(9) as u64) * 15,
            ))
    }

    async fn api() -> MockServer {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(answer)
            .mount(&server)
            .await;
        server
    }

    async fn counts(server: &MockServer) -> HashMap<&'static str, usize> {
        let mut by = HashMap::new();
        for r in server.received_requests().await.unwrap() {
            let p = r.url.path();
            let name = if p.starts_with("/api/agent-runs/tasks/") {
                "eligibility"
            } else if p.ends_with("/statuses") {
                "statuses"
            } else if p.ends_with("/agent-work") {
                "agent-work"
            } else if p.ends_with("/agent-approval") {
                "offer"
            } else if p == "/api/auth/devices/current" {
                "device"
            } else {
                "task"
            };
            *by.entry(name).or_default() += 1;
        }
        by
    }

    #[tokio::test]
    async fn the_shared_lookups_are_fetched_once_per_check_not_once_per_task() {
        let before_server = api().await;
        let after_server = api().await;
        let (b, a) = (before_server.uri(), after_server.uri());
        let n = 8;
        let (before, after) = on_thread(move || {
            let (r, w, allow) = (rule(), ws(), bound_allow());
            let facts = Facts {
                rule: &r,
                ws: &w,
                allow: &allow,
                keys: &[],
                host: "tmgr.dev",
                user: 1,
                now: now(),
                persona_optional: false,
            };
            let old = client(&b);
            let before: Vec<Found> = cards(n)
                .iter()
                .map(|c| {
                    check_card(&old, &facts, c, |f| {
                        let shared = queue::fresh_shared(&old, w.id)?;
                        queue::fresh_look_shared(&old, &r, f.id, &shared)
                    })
                    .found
                })
                .collect();
            let new = client(&a);
            let shared = queue::fresh_shared(&new, w.id).unwrap();
            let after: Vec<Found> = cards(n)
                .iter()
                .map(|c| {
                    check_card(&new, &facts, c, |f| {
                        queue::fresh_look_shared(&new, &r, f.id, &shared)
                    })
                    .found
                })
                .collect();
            (before, after)
        })
        .await;
        let ids = |v: &[Found]| {
            v.iter()
                .map(|f| (f.id, f.approval.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&before), ids(&after));
        assert!(after.iter().all(|f| f.sign.is_some()), "{after:?}");
        let (b, a) = (counts(&before_server).await, counts(&after_server).await);
        eprintln!("requests for {n} tasks, before: {b:?}, after: {a:?}");
        let n = n as usize;
        for key in ["eligibility", "task", "agent-work", "offer"] {
            assert_eq!((b[key], a[key]), (n, n), "{key}");
        }
        assert_eq!((b["statuses"], b["device"]), (n, n));
        assert_eq!((a["statuses"], a["device"]), (1, 1));
    }

    #[tokio::test]
    async fn parallel_checks_keep_the_card_order_and_match_the_sequential_result() {
        let server = api().await;
        let uri = server.uri();
        let (sequential, parallel) = on_thread(move || {
            let (r, w, allow) = (rule(), ws(), bound_allow());
            let facts = Facts {
                rule: &r,
                ws: &w,
                allow: &allow,
                keys: &[],
                host: "tmgr.dev",
                user: 1,
                now: now(),
                persona_optional: false,
            };
            let main = client(&uri);
            let shared = queue::fresh_shared(&main, w.id).unwrap();
            let list = cards(12);
            let none = |_: &Found| -> Result<Fresh> { unreachable!() };
            let sequential =
                check_all(&main, None, &r, &facts, &list, Some(&shared), &none).unwrap();
            let pool = Pool {
                server: &uri,
                agent: "test",
                verbose: false,
                tokens: main.access_only_tokens().unwrap(),
            };
            let parallel =
                check_all(&main, Some(&pool), &r, &facts, &list, Some(&shared), &none).unwrap();
            (sequential, parallel)
        })
        .await;
        let ids = |v: &[Found]| {
            v.iter()
                .map(|f| (f.id, f.approval.clone()))
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&sequential), ids(&parallel));
        assert_eq!(
            parallel.iter().map(|f| f.id).collect::<Vec<_>>(),
            (1..=12).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn a_worker_that_is_refused_is_retried_on_the_main_client() {
        let server = api().await;
        Mock::given(method("POST"))
            .and(path("/api/auth/refresh"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&server)
            .await;
        let uri = server.uri();
        let found = on_thread(move || {
            let (r, w, allow) = (rule(), ws(), bound_allow());
            let facts = Facts {
                rule: &r,
                ws: &w,
                allow: &allow,
                keys: &[],
                host: "tmgr.dev",
                user: 1,
                now: now(),
                persona_optional: false,
            };
            let main = client(&uri);
            let shared = queue::fresh_shared(&main, w.id).unwrap();
            let pool = Pool {
                server: &uri,
                agent: "test",
                verbose: false,
                tokens: tmgr_core::creds::Tokens {
                    access: Secret::new("stale"),
                    refresh: None,
                    expires_at: None,
                },
            };
            let none = |_: &Found| -> Result<Fresh> { unreachable!() };
            check_all(
                &main,
                Some(&pool),
                &r,
                &facts,
                &cards(6),
                Some(&shared),
                &none,
            )
            .unwrap()
        })
        .await;
        assert_eq!(
            found.iter().map(|f| f.id).collect::<Vec<_>>(),
            (1..=6).collect::<Vec<_>>()
        );
        assert!(found.iter().all(|f| f.sign.is_some()), "{found:?}");
        server.verify().await;
    }

    #[tokio::test]
    async fn only_a_failed_read_is_marked_for_retry() {
        let server = api().await;
        let uri = server.uri();
        let (good, stale) = on_thread(move || {
            let (r, w, allow) = (rule(), ws(), bound_allow());
            let facts = Facts {
                rule: &r,
                ws: &w,
                allow: &allow,
                keys: &[],
                host: "tmgr.dev",
                user: 1,
                now: now(),
                persona_optional: false,
            };
            let main = client(&uri);
            let shared = queue::fresh_shared(&main, w.id).unwrap();
            let card = &cards(1)[0];
            let good = check_card(&main, &facts, card, |f| {
                queue::fresh_look_shared(&main, &r, f.id, &shared)
            });
            let tokens = tmgr_core::creds::Tokens {
                access: Secret::new("stale"),
                refresh: None,
                expires_at: None,
            };
            let worker = Client::new(&uri, "test", Some(tokens), None, false).unwrap();
            let stale = check_card(&worker, &facts, card, |f| {
                queue::fresh_look_shared(&worker, &r, f.id, &shared)
            });
            (good.retry, stale.retry)
        })
        .await;
        assert!(!good);
        assert!(stale);
    }

    fn rules_of(list: &[(&str, &str)]) -> Vec<Rule> {
        list.iter()
            .map(|(name, workspace)| Rule {
                name: (*name).into(),
                workspace: (*workspace).into(),
                ..rule()
            })
            .collect()
    }

    #[test]
    fn a_workspace_picks_its_only_rule_ignoring_case() {
        let rules = rules_of(&[("a", "TM"), ("b", "OTHER")]);
        assert_eq!(rule_for_workspace(&rules, "tm", None).unwrap(), "a");
        assert_eq!(rule_for_workspace(&rules, "TM", Some("A")).unwrap(), "a");
    }

    #[test]
    fn a_workspace_without_a_rule_is_a_usage_error_with_a_hint() {
        let e = rule_for_workspace(&rules_of(&[("a", "TM")]), "nope", None).unwrap_err();
        assert_eq!(e.kind, Kind::Usage);
        assert_eq!(e.message, "no queue rule for workspace nope");
        assert_eq!(e.hint.as_deref(), Some("tmgr queue rules add --help"));
        assert!(rule_for_workspace(&[], "TM", None).is_err());
    }

    #[test]
    fn several_rules_of_one_workspace_ask_for_a_rule_name() {
        let rules = rules_of(&[("a", "TM"), ("b", "tm")]);
        let e = rule_for_workspace(&rules, "TM", None).unwrap_err();
        assert!(e.message.contains("a, b"), "{}", e.message);
        assert_eq!(e.hint.as_deref(), Some("--rule NAME"));
        assert_eq!(rule_for_workspace(&rules, "TM", Some("b")).unwrap(), "b");
    }

    #[test]
    fn a_rule_of_another_workspace_is_refused() {
        let rules = rules_of(&[("a", "TM"), ("b", "OTHER")]);
        let e = rule_for_workspace(&rules, "TM", Some("b")).unwrap_err();
        assert_eq!(e.kind, Kind::Usage);
        assert!(e.message.contains("not a cloud rule of workspace TM"));
    }

    #[test]
    fn remembered_answers_skip_the_interview_unless_setup_is_asked() {
        assert!(assume_answers(true, true, false, false));
        assert!(!assume_answers(true, false, false, false));
        assert!(!assume_answers(true, true, true, false));
        assert!(!assume_answers(true, true, true, true));
        assert!(assume_answers(false, false, false, true));
        assert!(!assume_answers(false, true, false, false));
        assert!(!assume_answers(false, false, true, true));
    }

    fn interview_run(assume: bool, input: &str) -> (Result<()>, String) {
        let remembered = Answers {
            max_teams: 1,
            max_concurrent: 1,
            ..Answers::default()
        };
        let mut d = Draft::new(Some(remembered), vec!["TM-1".into()], 3);
        let mut out = Vec::new();
        let r = interview::ask_all(
            &interview::questions(),
            &mut d,
            &interview::Flags::new(),
            assume,
            &mut input.as_bytes(),
            &mut out,
        );
        (r, String::from_utf8(out).unwrap())
    }

    #[test]
    fn remembered_answers_ask_nothing_and_setup_asks_again() {
        let (r, shown) = interview_run(assume_answers(true, true, false, false), "");
        r.unwrap();
        assert_eq!(shown, "");
        let (r, shown) = interview_run(assume_answers(true, true, true, false), &"\n".repeat(20));
        r.unwrap();
        assert!(shown.contains("Teams at the same time"), "{shown}");
        let (r, shown) = interview_run(assume_answers(true, false, false, false), &"\n".repeat(20));
        r.unwrap();
        assert!(shown.contains("Teams at the same time"), "{shown}");
    }

    fn batch_plan(code: &str, signing: usize, signed: usize) -> Plan {
        let mut p = plan(false, true);
        p.ws.code = code.into();
        let sign_task = |n: i64| SignTask {
            id: n,
            workspace_id: 7,
            category_id: 5,
            title: format!("Task {n}"),
            description: "Fix the login form\nand more".into(),
            repos_sha256: None,
            offer: Offer::default(),
        };
        let mk = |n: i64, sign: bool| Found {
            id: n,
            key: format!("TM-{n}"),
            title: format!("Task {n}"),
            repos: vec![],
            approval: if sign {
                Approval::NeedsSigning("no approval".into())
            } else {
                Approval::Valid
            },
            sign: sign.then(|| sign_task(n)),
        };
        p.chosen = (1..=signing as i64)
            .map(|n| mk(n, true))
            .chain((0..signed as i64).map(|n| mk(1000 + n, false)))
            .collect();
        p
    }

    #[test]
    fn the_token_is_the_server_workspace_code_in_its_case_and_the_task_count() {
        let p = batch_plan("TmGrDev", 2, 1);
        assert_eq!(batch_token(&p), "TmGrDev 3");
        assert_eq!(
            batch_prompt(&p),
            "Type \"TmGrDev 3\" to sign 2 tasks and start 3:"
        );
        assert_eq!(
            batch_prompt(&batch_plan("TM", 0, 4)),
            "Type \"TM 4\" to start 4:"
        );
        assert_eq!(
            batch_prompt(&batch_plan("TM", 1, 0)),
            "Type \"TM 1\" to sign 1 task and start 1:"
        );
        let mut dry = batch_plan("TM", 2, 0);
        dry.dry_run = true;
        assert_eq!(
            batch_prompt(&dry),
            "Type \"TM 2\" to sign 2 tasks and start the dry run of 2 tasks:"
        );
    }

    fn confirm(p: &Plan, typed: &str) -> (Result<()>, String) {
        let mut out = Vec::new();
        let r = confirm_batch(
            &compact_summary(p, false),
            &batch_token(p),
            &batch_prompt(p),
            &mut typed.as_bytes(),
            &mut out,
        );
        (r, String::from_utf8(out).unwrap())
    }

    #[test]
    fn only_the_exact_token_confirms_the_batch() {
        let p = batch_plan("TmGrDev", 2, 1);
        assert!(confirm(&p, "TmGrDev 3\n").0.is_ok());
        for wrong in [
            "tmgrdev 3\n",
            "TmGrDev 2\n",
            "TmGrDev 4\n",
            "TmGrDev\n",
            "y\n",
            "yes\n",
            "-y\n",
            "I AGREE\n",
            "\n",
            "",
        ] {
            let e = confirm(&p, wrong).0.unwrap_err();
            assert_eq!(e.kind, Kind::Interrupted, "{wrong:?}");
        }
    }

    #[test]
    fn the_token_is_asked_even_when_nothing_needs_signing() {
        let p = batch_plan("TM", 0, 2);
        assert!(confirm(&p, "y\n").0.is_err());
        let (r, shown) = confirm(&p, "TM 2\n");
        r.unwrap();
        assert!(shown.contains("Type \"TM 2\" to start 2:"));
    }

    #[test]
    fn the_confirmation_refuses_agent_sessions_and_missing_terminals_before_reading() {
        let p = batch_plan("TM", 1, 0);
        let mut out = Vec::new();
        let mut input = "TM 1\n".as_bytes();
        let e = confirm_batch_guarded(
            || privileged::check(true, true),
            &p,
            false,
            &mut input,
            &mut out,
        )
        .unwrap_err();
        assert_eq!(e.exit_code(), 5);
        let e = confirm_batch_guarded(
            || privileged::check(false, false),
            &p,
            false,
            &mut input,
            &mut out,
        )
        .unwrap_err();
        assert_eq!(e.exit_code(), 10);
        assert!(out.is_empty());
        assert_eq!(input, "TM 1\n".as_bytes());
        confirm_batch_guarded(
            || privileged::check(false, true),
            &p,
            false,
            &mut input,
            &mut out,
        )
        .unwrap();
    }

    #[test]
    fn the_compact_summary_lists_every_task_with_a_preview_and_the_limits_last() {
        let p = batch_plan("TM", 60, 5);
        let s = compact_summary(&p, false);
        for n in 1..=60 {
            assert!(s.contains(&format!(
                "TM-{n}  Task {n}  [sign now]  \"Fix the login form\" ("
            )));
        }
        assert!(s.contains("(27 chars)"));
        assert!(s.contains("TM-1000  Task 1000  [signed]"));
        assert!(!s.contains("and more"));
        assert!(!s.contains("more..."));
        let caps = s.find("Caps       ").unwrap();
        let push = s.find("Push       ").unwrap();
        let last_task = s.rfind("[signed]").unwrap();
        let usage = s.find("Usage      ").unwrap();
        assert!(last_task < usage && usage < caps && caps < push);
        assert!(s.trim_end().lines().last().unwrap().starts_with("Push"));
        let (_, shown) = confirm(&p, "");
        assert!(shown.starts_with("RISK:"));
        assert!(shown.trim_end().ends_with(&batch_prompt(&p)));
        assert!(shown.rfind("Push       ").unwrap() > shown.rfind("[sign now]").unwrap());
    }

    #[test]
    fn long_descriptions_show_only_the_first_line_and_a_size() {
        let mut p = batch_plan("TM", 1, 0);
        let sign = p.chosen[0].sign.as_mut().unwrap();
        sign.description = format!("\n  {}\nsecond line", "w".repeat(1200));
        let s = compact_summary(&p, false);
        assert!(s.contains("(1.2k chars)"), "{s}");
        assert!(s.contains('…'));
        assert!(!s.contains("second line"));
        p.chosen[0].sign.as_mut().unwrap().description = String::new();
        assert!(compact_summary(&p, false).contains("(no description)"));
    }

    #[test]
    fn the_signed_set_is_exactly_the_sign_now_list() {
        let p = batch_plan("TM", 3, 2);
        let s = compact_summary(&p, false);
        let listed: Vec<i64> = s
            .lines()
            .filter(|l| l.trim().starts_with("TM-") && l.contains("[sign now]"))
            .map(|l| {
                l.trim()
                    .strip_prefix("TM-")
                    .unwrap()
                    .split(' ')
                    .next()
                    .unwrap()
                    .parse()
                    .unwrap()
            })
            .collect();
        let signed: Vec<i64> = tasks_to_sign(&p).iter().map(|t| t.id).collect();
        assert_eq!(listed, signed);
        assert_eq!(signed, [1, 2, 3]);
    }

    fn hidden_variants() -> Vec<(&'static str, SignTask)> {
        use crate::cmd::signer::{OfferPage, OfferPersona};
        let base = || batch_plan("TM", 1, 0).chosen[0].sign.clone().unwrap();
        let persona = |name: &str, instructions: &str| OfferPersona {
            id: "993c30a6-3528-42bf-9a70-507841cebc08".into(),
            name: name.into(),
            instructions: instructions.into(),
        };
        let mut v = Vec::new();
        let mut t = base();
        t.title = "a\u{202e}b".into();
        v.push(("title", t));
        let mut t = base();
        t.description = "ok\u{2028}bad".into();
        v.push(("description", t));
        let mut t = base();
        t.offer.repositories = Some(vec!["o/a\u{200b}pi".into()]);
        v.push(("repository", t));
        let mut t = base();
        t.offer.persona = Some(persona("Agent", "do\u{1b}[31m this"));
        v.push(("persona instructions", t));
        let mut t = base();
        t.offer.persona = Some(persona("A\u{202e}B", "fine"));
        v.push(("persona name", t));
        let mut t = base();
        t.offer.context = Some(vec![OfferPage {
            id: 1,
            title: "Rules".into(),
            body: "x\u{e0041}y".into(),
        }]);
        v.push(("page body", t));
        let mut t = base();
        t.offer.context = Some(vec![OfferPage {
            id: 1,
            title: "Ru\u{2060}les".into(),
            body: "ok".into(),
        }]);
        v.push(("page title", t));
        v
    }

    #[test]
    fn hidden_characters_in_any_signed_field_leave_the_task_out_of_the_batch() {
        for (what, sign) in hidden_variants() {
            assert!(hidden_in(&sign), "{what}");
            let found = Found {
                sign: Some(sign),
                title: "clean title".into(),
                ..signable()
            };
            let left = leave_out_hidden(found.clone(), false);
            assert!(!left.approval.runnable(), "{what}");
            assert!(left.sign.is_none());
            assert!(
                matches!(&left.approval, Approval::Refused(w) if w == "hidden characters in its text; run with --review to see it and sign it"),
                "{what}"
            );
            let kept = leave_out_hidden(found, true);
            assert!(kept.approval.runnable(), "{what}");
            assert!(kept.sign.is_some());
        }
        let clean = batch_plan("TM", 1, 0).chosen[0].sign.clone().unwrap();
        assert!(!hidden_in(&clean));
        let mut multi = clean;
        multi.description = "line one\r\n\tline two\n".into();
        assert!(!hidden_in(&multi));
    }

    #[test]
    fn a_left_out_hidden_task_is_not_signed_and_is_named_in_the_summary() {
        let mut p = batch_plan("TM", 2, 0);
        let (chosen, dropped): (Vec<Found>, Vec<Found>) = {
            let mut bad = p.chosen[1].clone();
            bad.sign.as_mut().unwrap().description = "x\u{202e}y".into();
            vec![p.chosen[0].clone(), bad]
                .into_iter()
                .map(|f| leave_out_hidden(f, false))
                .partition(|f| f.approval.runnable())
        };
        p.chosen = chosen;
        p.dropped = dropped;
        assert_eq!(
            tasks_to_sign(&p).iter().map(|t| t.id).collect::<Vec<_>>(),
            [1]
        );
        assert_eq!(batch_token(&p), "TM 1");
        assert!(compact_summary(&p, false)
            .contains("TM-2  left out: refused: hidden characters in its text; run with --review"));
    }

    #[test]
    fn review_mode_says_the_full_texts_were_shown() {
        let p = batch_plan("TM", 1, 0);
        assert!(compact_summary(&p, true).contains("shown above"));
        assert!(compact_summary(&p, false).contains("--review"));
    }
}
