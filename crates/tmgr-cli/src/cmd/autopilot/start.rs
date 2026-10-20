use std::collections::BTreeSet;
use std::io::Write;

use chrono::{DateTime, Utc};
use serde_json::json;
use tmgr_core::api::Client;
use tmgr_core::approval::{canonical_repos, AllowFile, CloudApproval};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::text::sanitize_line;
use tmgr_ipc::api::caps::AUTOPILOT_V1;
use tmgr_ipc::api::AutopilotCommand;
use tmgr_link::privileged;
use tmgr_queue::cloud::{CloudApi, CloudWorkspace};
use tmgr_queue::rules::{self, Rule};
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
use crate::cmd::signer::{device_name, read_offer, Offer, SignTask, Signed, Signer};
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
}

impl Approval {
    fn shown(&self) -> String {
        match self {
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

fn not_free(found: Found, look: impl FnOnce(&Found) -> Result<Fresh>) -> Found {
    if matches!(found.approval, Approval::Refused(_)) {
        return found;
    }
    let why = match look(&found) {
        Ok(fresh) if fresh.free() => return found,
        Ok(fresh) => match fresh.reason() {
            "taken" => "taken by someone else".to_string(),
            "persona_missing" => "the rule's persona is not assigned to the task".to_string(),
            _ => "the task is not in the rule's status".to_string(),
        },
        Err(e) => format!("could not check the task ({})", sanitize_line(&e.message)),
    };
    Found {
        approval: Approval::Refused(why),
        sign: None,
        ..found
    }
}

fn with_offer(client: &Client, mut found: Found) -> Found {
    let Some(sign) = found.sign.as_mut() else {
        return found;
    };
    match read_offer(client, found.id) {
        Ok(offered) => {
            sign.offer = offered;
            found
        }
        Err(code) => Found {
            approval: Approval::Refused(format!(
                "could not read the approval offer ({})",
                sanitize_line(&code)
            )),
            sign: None,
            ..found
        },
    }
}

fn discover(
    ctx: &Ctx,
    cloud: &CliCloud,
    rule: &Rule,
    ws: &CloudWorkspace,
) -> Result<(Vec<Found>, bool)> {
    let (ids, complete) = cloud.list(rule, ws)?;
    let client = ctx.cloud_client()?;
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
    };
    let found = ids
        .into_iter()
        .map(
            |id| match client.agent_run_task(id).and_then(parse_eligibility) {
                Ok(elig) => with_offer(
                    client,
                    not_free(classify(id, &elig, &facts), |f| {
                        queue::fresh_look(ctx, rule, ws.id, f.id)
                    }),
                ),
                Err(e) => Found {
                    id,
                    key: format!("task-{id}"),
                    title: String::new(),
                    repos: Vec::new(),
                    approval: Approval::Refused(format!(
                        "could not read the task ({})",
                        sanitize_line(&e.message)
                    )),
                    sign: None,
                },
            },
        )
        .collect();
    Ok((found, complete))
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
}

fn limit_text(plan: &Plan) -> Vec<String> {
    let a = &plan.answers;
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
            a.max_teams.min(plan.rule.max_concurrent)
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

pub fn summary_text(plan: &Plan) -> String {
    let a = &plan.answers;
    let sign = plan.to_sign();
    let mut lines = vec![
        "AUTOPILOT".to_string(),
        format!(
            "Rule       {} (workspace {}, agent profile {})",
            plan.rule.name, plan.ws.code, plan.rule.profile
        ),
        format!("Tasks      {} to work on", plan.chosen.len()),
    ];
    for f in &plan.chosen {
        lines.push(format!(
            "           {}  {}  [{}]",
            f.key,
            clip(&f.title, TITLE_WIDTH),
            if f.sign.is_some() {
                "to be signed now"
            } else {
                "already signed"
            }
        ));
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
            "Signing    this computer signs the approvals of the {} tasks marked \"to be signed\" with its own key (kept in the OS keychain, registered as \"{}\") and trusts that key for workspace {}",
            sign.len(),
            plan.device,
            plan.ws.code
        ));
    }
    lines.extend(signed_texts(plan));
    lines.extend(team_text(plan));
    if plan.dry_run {
        lines.push(DRY_RUN_TEXT.into());
    } else {
        lines.push(format!(
            "Agents     turns the queue ON for every rule below and runs agents in the background on this machine, up to {} teams at the same time ({} is the rule's own limit)",
            a.max_teams.min(plan.rule.max_concurrent),
            plan.rule.max_concurrent
        ));
    }
    lines.push(plan.rules_text.clone());
    lines.extend(limit_text(plan));
    lines.push(match &plan.push {
        Some(note) => format!("Push       {note}"),
        None => "Push       notifications are on".into(),
    });
    if plan.dry_run {
        lines.push(
            "Finish     nothing is started; the plan and the triage are shown in the status".into(),
        );
        lines.push("Stop       `tmgr queue autopilot stop`; `tmgr queue autopilot status --watch` shows the plan".into());
    } else {
        lines.push("Finish     each task ends as a pull request; nothing is merged".into());
        lines.push("Stop       `tmgr queue autopilot stop` (or `pause`); `tmgr queue autopilot status --watch` shows progress; stopping turns the queue off".into());
    }
    lines.join("\n")
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

pub fn run(ctx: &Ctx, cli: &Cli, args: &StartArgs) -> Result<Output> {
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
    if cloud_rules.is_empty() {
        return Err(Error::usage("autopilot needs a rule for a cloud workspace")
            .hint("tmgr queue rules add --help"));
    }
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
    let rule_name = interview::choose_rule(
        &names,
        args.rule.as_deref(),
        remembered.as_ref().and_then(|a| a.rule.as_deref()),
        cli.yes,
        &mut stdin.lock(),
        &mut err,
    )?;
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
    let (found, complete) = discover(ctx, &cloud, &rule, &ws)?;
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
        cli.yes,
        &mut stdin.lock(),
        &mut err,
    )?;
    draft.restrict(
        args.only.as_deref(),
        args.skip.as_deref(),
        args.order.as_deref(),
    )?;
    let profile = draft.profile().to_string();
    let tool = agent_tool(&profile);
    let limits = if guard {
        let tool = tool.ok_or_else(usage::unknown_tool)?;
        let found = usage::read(&usage::Real, tool, now);
        if !usage::has_real_number(&found) {
            return Err(usage::refuse_without_numbers(tool));
        }
        found
    } else {
        Vec::new()
    };

    let (chosen, dropped): (Vec<Found>, Vec<Found>) = found
        .into_iter()
        .filter(|f| draft.is_chosen(&f.key))
        .partition(|f| !matches!(f.approval, Approval::Refused(_)));
    if let Some(k) = draft
        .order
        .iter()
        .find(|k| !chosen.iter().any(|f| &f.key == *k))
    {
        return Err(Error::usage(format!(
            "--order: {k} cannot be run, see the reasons in the list above"
        )));
    }
    if chosen.is_empty() {
        return Err(Error::usage("none of the chosen tasks can be run")
            .hint("see the reasons in the list above"));
    }
    let mut answers = draft.answers.clone();
    answers.all_tasks = draft.picked.is_none();
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
        limits: usage::rows(&limits, soft, hard),
        push: push_note(ctx),
        rules_text: queue::rules_summary(&shown.file),
        device: device_name(),
        now,
        tz: super::control::local_offset(),
    };
    queue::agree(&summary_text(&plan))?;
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
    let to_sign: Vec<SignTask> = plan.chosen.iter().filter_map(|f| f.sign.clone()).collect();
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
        answers: plan.answers.clone(),
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
        let f = not_free(signable(), |_| Ok(fresh(false, false)));
        assert!(f.sign.is_none());
        assert!(matches!(&f.approval, Approval::Refused(w) if w.contains("persona")));
    }

    #[test]
    fn a_taken_task_is_refused_and_a_free_one_stays_signable() {
        let f = not_free(signable(), |_| Ok(fresh(true, true)));
        assert!(matches!(&f.approval, Approval::Refused(w) if w.contains("taken")));
        assert!(not_free(signable(), |_| Ok(fresh(true, false)))
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
        let s = summary_text(&plan(true, true));
        for needle in [
            "TM-1  Task 1  [already signed]",
            "TM-2  Task 2  [to be signed now]",
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
            "tmgr queue autopilot stop",
        ] {
            assert!(s.contains(needle), "missing {needle:?} in\n{s}");
        }
        let s = summary_text(&plan(false, false));
        assert!(s.contains("nothing to sign"));
        assert!(s.contains("NO usage limit"));
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
        let s = summary_text(&p);
        for needle in [
            "Teams      up to 1 at the same time, 1 agent per team (codex runs one agent per team), team mode heavy",
            "Profile    the teams run on codex",
            "Order      TM-2, TM-1 first, in this order, then the rest",
            "up to 1 teams at the same time",
        ] {
            assert!(s.contains(needle), "missing {needle:?} in\n{s}");
        }
        assert!(!summary_text(&plan(false, true)).contains("Order "));
    }

    #[test]
    fn the_dry_run_summary_says_nothing_but_planning_happens() {
        let mut p = plan(true, true);
        p.dry_run = true;
        let s = summary_text(&p);
        for needle in [
            "Dry run: the orchestrator plans and triages only. Approvals are signed as usual so it can read the signed texts; nothing else changes: no status changes, no comments, no code, the queue stays off.",
            "signs the approvals of the 1 tasks",
            "tmgr queue autopilot status --watch",
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
        let s = summary_text(&p);
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
        let plain = summary_text(&plan(true, true));
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
        let s = summary_text(&p);
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
}
