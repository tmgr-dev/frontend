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
use super::store::Store;
use super::usage;
use super::{render, StartArgs};
use crate::cli::Cli;
use crate::cmd::notifyenv::NotifyEnv;
use crate::cmd::queue::{self, CliCloud};
use crate::cmd::runner::cloud::{cloud_gate, parse_eligibility, Eligibility, GateOk};
use crate::cmd::runner::{connect, rerr, runner_config};
use crate::cmd::signer::{device_name, SignTask, Signed, Signer};
use crate::ctx::Ctx;
use crate::out::{table, Output};

const LIST_CAP: usize = 40;
const TITLE_WIDTH: usize = 48;
const IO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const HANDOVER_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

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
                Ok(elig) => not_free(classify(id, &elig, &facts), |f| {
                    queue::fresh_look(ctx, rule, ws.id, f.id)
                }),
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
    lines.push(format!(
        "Agents     turns the queue ON for every rule below and runs agents in the background on this machine, up to {} at the same time ({} is the rule's own limit)",
        a.max_concurrent.min(plan.rule.max_concurrent),
        plan.rule.max_concurrent
    ));
    lines.push(plan.rules_text.clone());
    lines.extend(limit_text(plan));
    lines.push(match &plan.push {
        Some(note) => format!("Push       {note}"),
        None => "Push       notifications are on".into(),
    });
    lines.push("Finish     each task ends as a pull request; nothing is merged".into());
    lines.push("Stop       `tmgr queue autopilot stop` (or `pause`); `tmgr queue autopilot status --watch` shows progress; stopping turns the queue off".into());
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

pub fn hand_over() -> Result<()> {
    let mut client = hosted_client()?;
    match client.autopilot_control(AutopilotCommand::Suspend) {
        Ok(_) => {}
        Err(e) if e.to_string().starts_with("not_running: ") => return Ok(()),
        Err(e) => return Err(rerr(e)),
    }
    let end = std::time::Instant::now() + HANDOVER_WAIT;
    while client.autopilot_status().map_err(rerr)?.running {
        if std::time::Instant::now() >= end {
            return Err(Error::new(
                Kind::Conflict,
                "the queue loop did not suspend in time",
            ));
        }
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

    let now = Utc::now();
    let guard = !args.no_usage_limit;
    let tool = agent_tool(&rule.profile);
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
    );
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

    let (chosen, dropped): (Vec<Found>, Vec<Found>) = found
        .into_iter()
        .filter(|f| {
            draft
                .picked
                .as_ref()
                .is_none_or(|keys| keys.iter().any(|k| k == &f.key))
        })
        .partition(|f| !matches!(f.approval, Approval::Refused(_)));
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
        crate::cmd::runner::allow::pin_own_key(&host, user, plan.ws.id, &own, &plan.device)?;
        report = sign_chosen(&signer, client, &to_sign)?;
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
    queue::commit_enable(paths, shown_sha)?;
    match launch(store, plan, &tasks, &report, handover) {
        Ok(out) => Ok(attach_failures(out, &report, &plan.chosen)),
        Err(e) => {
            if handover {
                let _ = start_hosted(QUEUE_SETTINGS_ID);
            } else {
                let _ = tmgr_queue::switch::disable(paths, Some("autopilot did not start"));
            }
            Err(e)
        }
    }
}

fn launch(
    store: &Store,
    plan: &Plan,
    tasks: &BTreeSet<i64>,
    report: &SignReport,
    handover: bool,
) -> Result<Output> {
    store.save_answers(&plan.answers)?;
    let id = settings_id(Utc::now());
    let settings = Settings {
        id: id.clone(),
        created_at: Utc::now(),
        answers: plan.answers.clone(),
        rule: plan.rule.name.clone(),
        agent: plan
            .tool
            .map_or_else(|| plan.rule.profile.clone(), |t| t.as_str().to_string()),
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
    };
    store.save_settings(&settings)?;
    if handover {
        hand_over()?;
    }
    let pid = start_hosted(&id)?;
    Ok(Output::new(
        json!({"id": id, "pid": pid, "tasks": tasks, "signed": report.signed}),
        format!(
            "Autopilot started ({} tasks). Watch it with `tmgr queue autopilot status --watch`; stop it with `tmgr queue autopilot stop`.",
            tasks.len()
        ),
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
            "background on this machine, up to 2 at the same time",
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
