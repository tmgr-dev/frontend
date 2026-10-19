use crate::cli::Cli;
use crate::cmd::runner::allow::host_of;
use crate::cmd::runner::cloud::{
    bound_set_paths, cloud_gate, parse_eligibility, publish_outcome, CloneRef, Mode, Outcome,
    PublishTarget,
};
use crate::cmd::runner::Opts;
use crate::ctx::Ctx;
use crate::out::{stdout_style, table, Output};
use chrono::{DateTime, Local, Timelike, Utc};
use clap::{Args, Subcommand};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::time::Duration;
use tmgr_core::approval::{AllowFile, Rollback};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::text::sanitize;
use tmgr_core::text::sanitize_line;
use tmgr_ipc::api::{ExePin, RunCreateReq, StateKind};
use tmgr_link::privileged;
use tmgr_queue::audit::sha256_hex;
use tmgr_queue::audit::Audit;
use tmgr_queue::cloud::{CloudApi, CloudTask, CloudWorkspace, Gated};
use tmgr_queue::rules::{self, ProfilePin, ProfileResolver, QueueFile, Rule};
use tmgr_queue::runs::{self, Lookup, RunState, RunsClient, StartRequest};
use tmgr_queue::source::{LocalSource, LocalTasks};
use tmgr_queue::state::{self, Status};
use tmgr_queue::taken::{
    assess, owners_of, people_of, personas_of, running_agents_of, Facts, Fresh, FreshTask,
};
use tmgr_queue::workdir::GitClone;
use tmgr_queue::{Deps, Paths, Publisher};

const WARNING: &str = "\
RISK: with the queue on, tasks in your local workspaces start AI agents on this machine
without you looking at them first. A task, a comment or a page that someone or something
else wrote can steer an agent that can read and change your files and run commands.
Each rule limits what can start: its workspace, project, persona, agent profile, folder,
number of agents, hours and trusted authors. A rule never runs an agent without permission
prompts. Only the runner sandbox keeps an agent away from the rules in ~/.config/tmgr.";

#[derive(Subcommand)]
pub enum QueueCommand {
    /// Show the queue: switch, rules, pickups, pending approvals and the audit chain
    Status {
        /// Print the summary, then new events of the background loop until Ctrl+C
        #[arg(long, visible_aliases = ["watch", "follow"])]
        tail: bool,
    },
    /// Stop picking up work immediately
    Pause,
    /// Continue picking up work
    Resume,
    /// Turn the queue on and let it run by itself in the background (needs a terminal and a typed confirmation)
    Enable,
    /// Turn the queue off
    Disable {
        #[arg(long, hide = true)]
        reason: Option<String>,
    },
    /// List, add or remove rules
    Rules {
        #[command(subcommand)]
        command: RulesCmd,
    },
    /// Approve one pending pickup exactly as shown (needs a terminal)
    Approve {
        /// Pickup id from `tmgr queue status`
        pickup: String,
    },
    /// Give up a pickup whose start has an unknown outcome and free its slot (needs a terminal)
    Release { pickup: String },
    /// Run one pickup pass now
    RunOnce,
    /// Work through the backlog in the background: interview, one agreement, then the runner hosts the loop
    Autopilot(Box<super::autopilot::AutopilotArgs>),
}

#[derive(Subcommand)]
pub enum RulesCmd {
    /// List the rules
    List,
    /// Add a rule (widens what may start: needs a terminal and a typed confirmation)
    Add(Box<RuleArgs>),
    /// Remove a rule
    Rm { name: String },
}

#[derive(Args)]
pub struct RuleArgs {
    /// Rule name: a-z, 0-9 and -, at most 32
    pub name: String,
    /// Local workspace written local:<code>, or an allowed cloud workspace code (see `tmgr runner allow`)
    #[arg(long, value_name = "local:CODE|CODE")]
    pub workspace: String,
    /// Persona the task must be assigned to
    #[arg(long, value_name = "NAME")]
    pub persona: String,
    /// Agent profile id
    #[arg(long, value_name = "PROFILE")]
    pub profile: String,
    /// Existing directory the agent works in; repeat it for a cloud category that lists several repositories
    #[arg(long, value_name = "DIR", required = true)]
    pub repo: Vec<String>,
    /// Project (category) code; default: every project
    #[arg(long, value_name = "CODE")]
    pub project: Option<String>,
    /// Status type of the tasks: backlog or active
    #[arg(long, default_value = "backlog", value_name = "TYPE")]
    pub status_type: String,
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,
    /// At most this many agents of the rule at once (1 to 3)
    #[arg(long, default_value_t = 1, value_name = "N")]
    pub max_concurrent: u32,
    /// Working hours, HH-HH in local time, for example 09-19
    #[arg(long, value_name = "HH-HH")]
    pub hours: Option<String>,
    /// Trusted author kind: human, agent or unrecorded (repeatable; default human)
    #[arg(long = "trusted-author", value_name = "KIND")]
    pub trusted_authors: Vec<String>,
    /// Start without asking for each task (human authors only)
    #[arg(long)]
    pub auto: bool,
    /// Permission mode of a cloud rule: ask, accept-edits, plan or bypass (the runner decides in the end)
    #[arg(long, value_name = "MODE")]
    pub mode: Option<String>,
}

pub struct RunnerProfiles;

fn launch_argv(
    p: &tmgr_runner::profile::Profile,
    model: Option<&str>,
) -> std::result::Result<Vec<String>, String> {
    p.approved_argv(model, p.permission_args(runs::PERMISSION_MODE)?)
}

impl ProfileResolver for RunnerProfiles {
    fn argv(&self, profile: &str, model: Option<&str>) -> std::result::Result<Vec<String>, String> {
        let dir = tmgr_runner::profile::user_dir();
        let (set, _) = tmgr_runner::profile::ProfileSet::load(dir.as_deref());
        launch_argv(set.lookup(profile)?, model)
    }

    fn pin(&self, profile: &str, model: Option<&str>) -> std::result::Result<ProfilePin, String> {
        self.pin_for(profile, model, runs::PERMISSION_MODE)
    }

    fn pin_for(
        &self,
        profile: &str,
        model: Option<&str>,
        mode: &str,
    ) -> std::result::Result<ProfilePin, String> {
        let dir = tmgr_runner::profile::user_dir();
        let (set, _) = tmgr_runner::profile::ProfileSet::load(dir.as_deref());
        let p = set.lookup(profile)?;
        let argv = p.approved_argv(model, p.permission_args(mode)?)?;
        let profile_sha256 = match &p.source {
            tmgr_runner::profile::Source::User(path) => {
                rules::file_sha256(path).map_err(|e| e.to_string())?
            }
            tmgr_runner::profile::Source::Builtin => sha256_hex(format!("{p:?}").as_bytes()),
        };
        let exe = rules::resolve_exe(p.command.first().ok_or("profile has no command")?)?;
        Ok(ProfilePin {
            argv,
            profile_sha256,
            exe_sha256: rules::file_sha256(&exe).map_err(|e| e.to_string())?,
            exe_path: exe.to_string_lossy().into_owned(),
        })
    }
}

fn in_agent_session() -> bool {
    privileged::RUN_ENV
        .iter()
        .any(|k| std::env::var_os(k).is_some())
}

pub(crate) fn not_in_agent_session() -> Result<()> {
    if in_agent_session() {
        return Err(
            Error::new(Kind::Forbidden, "refused inside an agent session")
                .hint("run it yourself in a normal terminal"),
        );
    }
    Ok(())
}

pub(crate) fn agree(summary: &str) -> Result<()> {
    privileged::guard()?;
    eprintln!("{WARNING}");
    if !summary.is_empty() {
        eprintln!();
        let lines: Vec<String> = summary.lines().map(str::to_string).collect();
        page_through(&lines)?;
    }
    privileged::confirm_typed_stdio("I AGREE", "\nType I AGREE to continue:")
}

fn describe(r: &Rule) -> String {
    format!(
        "{}: {} {} persona {} profile {}{} {} {} max {} hours {} authors {}{}{}",
        r.name,
        r.workspace,
        r.project
            .as_deref()
            .map_or("all projects".into(), |p| format!("project {p}")),
        r.persona,
        r.profile,
        r.model
            .as_deref()
            .map_or(String::new(), |m| format!(" model {m}")),
        if r.repos.len() > 1 { "repos" } else { "repo" },
        r.repos.join(", "),
        r.max_concurrent,
        r.hours.as_deref().unwrap_or("any"),
        r.trusted_authors.join("+"),
        if r.auto { " AUTO" } else { " approval needed" },
        r.mode
            .as_deref()
            .map_or(String::new(), |m| format!(" mode {m}"))
    )
}

pub(crate) fn rules_summary(f: &QueueFile) -> String {
    if f.rules.is_empty() {
        return "No rules yet.".into();
    }
    let lines: Vec<String> = f
        .rules
        .iter()
        .map(|r| format!("  {}", describe(r)))
        .collect();
    format!("Rules:\n{}", lines.join("\n"))
}

fn tamper_error() -> Error {
    Error::new(
        Kind::Forbidden,
        "queue.toml was changed outside `tmgr queue`",
    )
    .hint("review the file, then confirm it with `tmgr queue enable`")
}

fn record(paths: &Paths, event: &str, sha: &str, fields: Value) -> Result<()> {
    let mut f = fields;
    f["config_sha256"] = json!(sha);
    Audit::new(paths).append(event, f)
}

pub fn run(ctx: &Ctx, cli: &Cli, cmd: &QueueCommand) -> Result<Output> {
    let paths = Paths::from_env()?;
    match cmd {
        QueueCommand::Status { tail } => {
            if *tail {
                let line = queue_line(&paths)?;
                super::autopilot::tail::run(&line, cli.json)
            } else {
                status(&paths)
            }
        }
        QueueCommand::Pause => set_paused(&paths, true),
        QueueCommand::Resume => {
            not_in_agent_session()?;
            set_paused(&paths, false)
        }
        QueueCommand::Enable => enable(ctx, &paths),
        QueueCommand::Disable { reason } => disable(&paths, reason.as_deref()),
        QueueCommand::Rules { command } => match command {
            RulesCmd::List => list(&paths),
            RulesCmd::Add(a) => add(ctx, &paths, a),
            RulesCmd::Rm { name } => remove(&paths, name),
        },
        QueueCommand::Approve { pickup } => approve(&paths, pickup),
        QueueCommand::Release { pickup } => {
            privileged::guard()?;
            eprintln!("The run may still be alive. Release only after checking `tmgr agents`.");
            privileged::confirm_typed_stdio(pickup, "Type the pickup id to release it:")?;
            tmgr_queue::release(&paths, pickup, Utc::now())?;
            Ok(Output::new(
                json!({"released": pickup}),
                format!("Pickup {pickup} released."),
            ))
        }
        QueueCommand::RunOnce => run_once(ctx, cli, &paths),
        QueueCommand::Autopilot(a) => super::autopilot::run(ctx, cli, a),
    }
}

fn shown_status(p: &state::Pickup) -> &'static str {
    if p.held.is_some() {
        "held"
    } else {
        p.status.name()
    }
}

fn queue_line(paths: &Paths) -> Result<String> {
    let loaded = rules::load(paths)?;
    let st = state::load(paths)?;
    Ok(format!(
        "Queue is {}{}.",
        if loaded.file.enabled { "on" } else { "off" },
        if st.paused { " (paused)" } else { "" }
    ))
}

fn status(paths: &Paths) -> Result<Output> {
    let loaded = rules::load(paths)?;
    let tampered = rules::is_tampered(paths, &loaded)?;
    let st = state::load(paths)?;
    let audit = match Audit::new(paths).verify()? {
        Ok(n) => json!({"ok": true, "entries": n}),
        Err(t) => json!({"ok": false, "line": t.line, "reason": t.reason}),
    };
    let pickups: Vec<Value> = st
        .pickups
        .iter()
        .filter(|p| !p.released)
        .map(|p| {
            json!({"id": p.id, "rule": p.rule, "task": p.key, "status": shown_status(p),
                "bundle_sha256": p.bundle_sha256, "tainted": p.context_tainted,
                "run": p.run_id, "reason": p.reason, "held": p.held})
        })
        .collect();
    let mut lines = vec![format!(
        "Queue is {}{}.",
        if loaded.file.enabled { "on" } else { "off" },
        if st.paused { " (paused)" } else { "" }
    )];
    if tampered {
        lines.push("WARNING: queue.toml was changed outside `tmgr queue`; the queue will not run until you confirm it with `tmgr queue enable`.".into());
    }
    if audit["ok"] == false {
        lines.push(format!(
            "WARNING: the audit log failed verification ({}).",
            audit["reason"].as_str().unwrap_or("")
        ));
    }
    let (background, background_line) = super::autopilot::queue_loop::view();
    lines.push(background_line);
    lines.push(rules_summary(&loaded.file));
    let rows: Vec<Vec<String>> = st
        .pickups
        .iter()
        .filter(|p| !p.released)
        .map(|p| {
            vec![
                p.id.clone(),
                p.rule.clone(),
                p.key.clone(),
                shown_status(p).to_string(),
                p.bundle_sha256[..12].to_string(),
            ]
        })
        .collect();
    if rows.is_empty() {
        lines.push("No open pickups.".into());
    } else {
        lines.push(table(&rows));
    }
    Ok(Output::new(
        json!({"enabled": loaded.file.enabled, "paused": st.paused, "tampered": tampered,
            "audit": audit, "rules": loaded.file.rules, "pickups": pickups,
            "background": background}),
        lines.join("\n"),
    ))
}

fn set_paused(paths: &Paths, paused: bool) -> Result<Output> {
    let _lock = paths.lock()?;
    let mut st = state::load(paths)?;
    st.paused = paused;
    state::save(paths, &mut st)?;
    Audit::new(paths).append(if paused { "pause" } else { "resume" }, json!({}))?;
    Ok(Output::new(
        json!({"paused": paused}),
        if paused {
            "Queue paused."
        } else {
            "Queue resumed."
        },
    ))
}

fn enable(ctx: &Ctx, paths: &Paths) -> Result<Output> {
    privileged::guard()?;
    let shown = validated_rules(ctx, paths)?;
    agree(&rules_summary(&shown.file))?;
    let out = commit_enable(paths, shown.sha256.as_deref())?;
    Ok(super::autopilot::queue_loop::after_enable(paths, out))
}

pub(crate) fn validated_rules(ctx: &Ctx, paths: &Paths) -> Result<rules::Loaded> {
    let shown = rules::load(paths)?;
    rules::validate_file_with(
        &shown.file,
        &RunnerProfiles,
        Some(&CliCloud::new(ctx)),
        Utc::now(),
    )?;
    Ok(shown)
}

pub(crate) fn commit_enable(paths: &Paths, shown_sha: Option<&str>) -> Result<Output> {
    let _lock = paths.lock()?;
    let now = rules::load(paths)?;
    if now.sha256.as_deref() != shown_sha {
        return Err(Error::conflict(
            "queue.toml changed while you were reading it; nothing was enabled",
        )
        .hint("run `tmgr queue enable` again"));
    }
    let mut file = now.file;
    let rebaselined = Audit::new(paths).rebaseline()?;
    file.enabled = true;
    let sha = rules::save(paths, &file)?;
    record(
        paths,
        "enable",
        &sha,
        json!({"rules": file.rules.len(), "audit_rebaselined": rebaselined}),
    )?;
    Ok(Output::new(json!({"enabled": true}), "Queue is on."))
}

fn disable(paths: &Paths, reason: Option<&str>) -> Result<Output> {
    tmgr_queue::switch::disable(paths, reason)?;
    Ok(Output::new(json!({"enabled": false}), "Queue is off."))
}

fn list(paths: &Paths) -> Result<Output> {
    let loaded = rules::load(paths)?;
    Ok(Output::new(
        json!({"rules": loaded.file.rules}),
        rules_summary(&loaded.file),
    ))
}

fn add(ctx: &Ctx, paths: &Paths, a: &RuleArgs) -> Result<Output> {
    privileged::guard()?;
    let repos = a
        .repo
        .iter()
        .map(|r| {
            std::fs::canonicalize(r)
                .map(|p| p.to_string_lossy().into_owned())
                .map_err(|_| Error::usage("repo is not an existing directory"))
        })
        .collect::<Result<Vec<_>>>()?;
    let rule = Rule {
        name: a.name.clone(),
        workspace: a.workspace.clone(),
        project: a.project.clone(),
        status_type: a.status_type.clone(),
        persona: a.persona.clone(),
        profile: a.profile.clone(),
        model: a.model.clone(),
        repos,
        max_concurrent: a.max_concurrent,
        hours: a.hours.clone(),
        trusted_authors: if a.trusted_authors.is_empty() {
            vec!["human".into()]
        } else {
            a.trusted_authors.clone()
        },
        auto: a.auto,
        mode: a.mode.clone(),
    };
    rules::validate_rule_with(
        &rule,
        &RunnerProfiles,
        Some(&CliCloud::new(ctx)),
        Utc::now(),
    )?;
    let loaded = rules::load(paths)?;
    if rules::is_tampered(paths, &loaded)? {
        return Err(tamper_error());
    }
    if loaded.file.rules.iter().any(|r| r.name == rule.name) {
        return Err(Error::conflict(format!(
            "rule {} already exists",
            rule.name
        )));
    }
    agree(&format!("New rule:\n  {}", describe(&rule)))?;
    let _lock = paths.lock()?;
    let shown_sha = loaded.sha256.clone();
    let loaded = rules::load(paths)?;
    if loaded.sha256 != shown_sha {
        return Err(Error::conflict(
            "queue.toml changed while you were reading it; no rule was added",
        )
        .hint("run the command again"));
    }
    if rules::is_tampered(paths, &loaded)? {
        return Err(tamper_error());
    }
    let mut file = loaded.file;
    if file.rules.iter().any(|r| r.name == rule.name) {
        return Err(Error::conflict(format!(
            "rule {} already exists",
            rule.name
        )));
    }
    file.rules.push(rule.clone());
    let sha = rules::save(paths, &file)?;
    record(
        paths,
        "rule_add",
        &sha,
        json!({"rule": serde_json::to_value(&rule).unwrap_or(Value::Null)}),
    )?;
    Ok(Output::new(
        json!({"rule": rule}),
        format!("Rule {} added.", rule.name),
    ))
}

fn remove(paths: &Paths, name: &str) -> Result<Output> {
    let _lock = paths.lock()?;
    let loaded = rules::load(paths)?;
    if rules::is_tampered(paths, &loaded)? {
        return Err(tamper_error().hint("`tmgr queue disable` is always allowed"));
    }
    let mut file = loaded.file;
    let before = file.rules.len();
    file.rules.retain(|r| r.name != name);
    if file.rules.len() == before {
        return Err(Error::not_found(format!("no rule named {name}")).hint("tmgr queue rules list"));
    }
    let sha = rules::save(paths, &file)?;
    record(paths, "rule_rm", &sha, json!({"rule": name}))?;
    Ok(Output::new(
        json!({"removed": name}),
        format!("Rule {name} removed."),
    ))
}

const PAGE: usize = 40;
const REVIEW_CAP: usize = 64 * 1024;

fn review(p: &state::Pickup, pin: &ProfilePin) -> Vec<String> {
    let mut out = Vec::new();
    let mut push =
        |s: String| out.extend(sanitize(&s).lines().map(str::to_string).collect::<Vec<_>>());
    push(format!("Approve {} for rule {}", p.id, p.rule));
    push(format!("Task      {} {}", p.key, p.bundle.title));
    push(format!("Author    {}", p.bundle.author.kind.as_str()));
    if p.context_tainted {
        push("NOTE      part of the context was written by agents or others".into());
    }
    push(format!(
        "Persona   {}",
        p.bundle
            .persona
            .as_ref()
            .map_or("none", |x| x.name.as_str())
    ));
    push(format!(
        "Profile   {} model {}",
        p.bundle.launch.profile,
        p.bundle.launch.model.as_deref().unwrap_or("default")
    ));
    push(format!("Launch    {}", pin.argv.join(" ")));
    push(format!(
        "Executable {} sha256 {}",
        pin.exe_path, pin.exe_sha256
    ));
    push("          the runner refuses to start any other file; the pin covers this file only, not the interpreter of a script or what a wrapper starts".into());
    push(format!("Profile sha256 {}", pin.profile_sha256));
    push("Mode      the agent asks before it acts (permission prompts on); no bypass".into());
    push(format!(
        "Folder    {} (a separate clone on branch queue/{}), not your repository",
        p.bundle.launch.cwd, p.id
    ));
    push(format!("Bundle    sha256 {}", p.bundle_sha256));
    push(String::new());
    push("Persona instructions (given to the agent):".into());
    push(p.persona_text.clone());
    push(String::new());
    push("Task text (untrusted, given to the agent as data):".into());
    push(p.bundle.body.clone());
    for (n, c) in p.bundle.comments.iter().enumerate() {
        push(String::new());
        push(format!(
            "Comment {} by {} at {}:",
            n + 1,
            c.author.kind.as_str(),
            c.at
        ));
        push(c.body.clone());
    }
    for pg in &p.bundle.pages {
        push(String::new());
        push(format!(
            "Page {} revision {}: {}",
            pg.id, pg.revision, pg.title
        ));
        push(pg.body.clone());
    }
    out
}

fn page_through(lines: &[String]) -> Result<()> {
    let mut input = std::io::stdin().lock();
    for (n, chunk) in lines.chunks(PAGE).enumerate() {
        for l in chunk {
            eprintln!("{l}");
        }
        if (n + 1) * PAGE < lines.len() {
            eprint!("-- more, press Enter -- ");
            let mut sink = String::new();
            std::io::BufRead::read_line(&mut input, &mut sink)
                .map_err(|_| Error::new(Kind::Interrupted, "could not read the answer"))?;
        }
    }
    Ok(())
}

fn approve(paths: &Paths, id: &str) -> Result<Output> {
    privileged::guard()?;
    let st = state::load(paths)?;
    let p = st
        .pickups
        .iter()
        .find(|p| p.id == id && !p.released)
        .ok_or_else(|| {
            Error::not_found(format!("no open pickup {id}")).hint("tmgr queue status")
        })?;
    if p.status != Status::PendingApproval {
        return Err(Error::conflict(format!(
            "pickup {id} is {}, not waiting for approval",
            p.status.name()
        )));
    }
    if p.cloud.is_some() {
        return Err(Error::new(
            Kind::Forbidden,
            "a pickup of a cloud task is approved by the owner's signature, not here",
        ));
    }
    if p.bundle.canonical().len() > REVIEW_CAP {
        return Err(Error::usage("the bundle is too large to review")
            .hint("shorten the task text or its comments"));
    }
    if sha256_hex(p.persona_text.as_bytes())
        != p.bundle
            .persona
            .as_ref()
            .map_or(String::new(), |x| x.instructions_sha256.clone())
    {
        return Err(Error::new(
            Kind::Forbidden,
            "the stored persona text does not match its hash",
        ));
    }
    let pin = RunnerProfiles
        .pin(&p.bundle.launch.profile, p.bundle.launch.model.as_deref())
        .map_err(|m| {
            Error::new(
                Kind::Forbidden,
                format!("the agent profile cannot be pinned: {m}"),
            )
        })?;
    if rules::argv_is_bypass(&pin.argv) {
        return Err(Error::new(
            Kind::Forbidden,
            "the profile runs without permission prompts",
        ));
    }
    tmgr_core::local::verify_journal_from_genesis(p.task.workspace_id)?;
    let sha = p.bundle_sha256.clone();
    page_through(&review(p, &pin))?;
    eprintln!();
    privileged::confirm_typed_stdio(
        &sha[..12],
        "Type the first 12 characters of the bundle sha256 to approve exactly this:",
    )?;
    tmgr_queue::approve(paths, id, &sha, pin, Utc::now())?;
    Ok(Output::new(
        json!({"approved": id, "bundle_sha256": sha}),
        format!("Pickup {id} approved. `tmgr queue run-once` starts it."),
    ))
}

const REFRESH_PAGES: u64 = 50;

pub(crate) struct CliCloud<'a> {
    ctx: &'a Ctx,
    live_keys: RefCell<Option<Vec<String>>>,
}

impl<'a> CliCloud<'a> {
    pub(crate) fn new(ctx: &'a Ctx) -> Self {
        CliCloud {
            ctx,
            live_keys: RefCell::new(None),
        }
    }

    pub(crate) fn identity(&self) -> Result<(String, i64)> {
        let host = host_of(&self.ctx.server()?)?;
        let user = self.ctx.cloud_client()?.current_user_id()?.ok_or_else(|| {
            Error::auth("cannot tell which account is logged in").hint("tmgr login")
        })?;
        Ok((host, user))
    }

    pub(crate) fn keys(&self) -> Result<Vec<String>> {
        if let Some(k) = self.live_keys.borrow().clone() {
            return Ok(k);
        }
        let keys: Vec<String> = self
            .ctx
            .cloud_client()?
            .agent_run_keys()?
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|k| k.get("key_id").and_then(Value::as_str).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        *self.live_keys.borrow_mut() = Some(keys.clone());
        Ok(keys)
    }
}

impl CloudApi for CliCloud<'_> {
    fn allow(&self, code: &str, now: DateTime<Utc>) -> std::result::Result<CloudWorkspace, String> {
        let (host, user) = self.identity().map_err(|e| e.message)?;
        let allow = AllowFile::load().map_err(|e| format!("runner allow list: {e}"))?;
        let command = format!("tmgr runner allow {code}");
        let Some(found) = allow.entries.iter().find(|e| {
            e.server == host && e.user_id == user && e.workspace_code.eq_ignore_ascii_case(code)
        }) else {
            return Err(format!(
                "workspace {code} is not allowed to run agents on this machine; run `{command}`"
            ));
        };
        let entry = allow
            .find_live(&host, user, found.workspace_id, &found.workspace_code, now)
            .map_err(|r| format!("{}; run `{}`", sanitize_line(&r.message), r.fix))?;
        Ok(CloudWorkspace {
            id: entry.workspace_id,
            code: entry.workspace_code.clone(),
            repos: entry.repo_paths(),
        })
    }

    fn list(&self, rule: &Rule, ws: &CloudWorkspace) -> Result<(Vec<i64>, bool)> {
        let client = self.ctx.cloud_client()?;
        let wanted = match rule.status_type.as_str() {
            "backlog" => "default",
            other => other,
        };
        let status_ids: Vec<u64> = client
            .statuses(ws.id)?
            .iter()
            .filter(|s| s.kind == wanted)
            .map(|s| s.id)
            .collect();
        let project = rule.project.as_deref().unwrap_or_default();
        let Some(category) = client.categories(ws.id)?.into_iter().find(|c| {
            c.code
                .as_deref()
                .is_some_and(|code| code.eq_ignore_ascii_case(project))
        }) else {
            return Ok((Vec::new(), true));
        };
        let mut ids = Vec::new();
        let mut complete = true;
        for status in status_ids {
            let mut page = 1;
            loop {
                let got = client.tasks_page(ws.id, Some(category.id), Some(status), page, 100)?;
                ids.extend(got.items.iter().filter_map(|t| i64::try_from(t.id).ok()));
                if page >= got.last_page {
                    break;
                }
                if page >= REFRESH_PAGES {
                    complete = false;
                    break;
                }
                page += 1;
            }
        }
        ids.sort_unstable();
        ids.dedup();
        Ok((ids, complete))
    }

    fn gate(&self, rule: &Rule, ws: &CloudWorkspace, task_id: i64) -> Result<Gated> {
        let client = self.ctx.cloud_client()?;
        let (host, user) = self.identity()?;
        let elig = parse_eligibility(client.agent_run_task(task_id)?)?;
        let key = sanitize_line(
            elig.task_key
                .as_deref()
                .unwrap_or(&format!("task-{task_id}")),
        );
        let refused = |reason: String| Gated::Refused {
            key: key.clone(),
            reason: sanitize_line(&reason),
        };
        if elig.task_id != task_id || elig.workspace_id != ws.id {
            return Ok(refused(
                "the server answered for another task or workspace".into(),
            ));
        }
        let allow =
            AllowFile::load().map_err(|e| Error::internal(format!("runner allow list: {e}")))?;
        let mut rollback =
            Rollback::load().map_err(|e| Error::internal(format!("cloud approvals state: {e}")))?;
        let rkey = Rollback::key(&host, user, ws.id, task_id);
        let ok = match cloud_gate(
            &allow,
            &self.keys()?,
            &elig,
            &host,
            user,
            Utc::now(),
            rollback.seen(&rkey),
        ) {
            Ok(ok) => ok,
            Err(r) => return Ok(refused(format!("{} ({})", r.message, r.fix))),
        };
        let Some(entry) = allow.find(&host, user, ws.id) else {
            return Ok(refused("the allow entry disappeared".into()));
        };
        let Some(category) = &ok.category else {
            return Ok(refused(
                "the task has no category, so no repository is known".into(),
            ));
        };
        let claim_repos = match rule_binding(rule, &ok, entry, category, &ws.code, task_id) {
            Ok(repos) => repos,
            Err(reason) => return Ok(refused(reason)),
        };
        rollback.record(rkey, ok.claim.signed_at, &Rollback::stamp(&ok.claim));
        rollback
            .save()
            .map_err(|e| Error::internal(format!("cloud approvals state: {e}")))?;
        Ok(Gated::Ready(Box::new(CloudTask {
            key: ok.task_key,
            title: ok.title,
            body: ok.body,
            claim: ok.claim,
            repos: claim_repos,
            persona: ok.persona,
            context: ok.context,
        })))
    }
}

/// The repositories a queue pickup works on, or why the category's binding does not match the rule.
/// Explicit set: the rule lists exactly the bound folders. No list: exactly the one bound folder.
pub(crate) fn count_unconfirmed(
    ctx: &Ctx,
    paths: &Paths,
    rule_name: &str,
    selected: &BTreeSet<i64>,
) -> Result<u32> {
    const LOOKUPS: usize = 50;
    let loaded = rules::load(paths)?;
    let rule = loaded
        .file
        .rules
        .iter()
        .find(|r| r.name == rule_name)
        .ok_or_else(|| Error::not_found(format!("no rule {rule_name}")))?;
    let cloud = CliCloud::new(ctx);
    let ws = cloud
        .allow(&rule.workspace, Utc::now())
        .map_err(|m| Error::new(Kind::Forbidden, m))?;
    let (ids, _) = cloud.list(rule, &ws)?;
    let mut refused = 0;
    for id in ids
        .into_iter()
        .filter(|i| !selected.contains(i))
        .take(LOOKUPS)
    {
        if matches!(cloud.gate(rule, &ws, id)?, Gated::Refused { .. }) {
            refused += 1;
        }
    }
    Ok(refused)
}

pub(crate) fn rule_binding(
    rule: &Rule,
    ok: &crate::cmd::runner::cloud::GateOk,
    entry: &tmgr_core::approval::Entry,
    category: &crate::cmd::runner::cloud::Category,
    ws_code: &str,
    task_id: i64,
) -> std::result::Result<Vec<tmgr_queue::clones::ClaimRepo>, String> {
    let Some(list) = &ok.repos else {
        let bind =
            format!("run `tmgr run {ws_code}:{task_id}` once in a terminal to bind a repository");
        if category.github_full_name.is_none() {
            return Err(format!(
                "category {} has no linked GitHub repository",
                category.code
            ));
        }
        return match entry.repo(category.id) {
            None if entry.repo_set(category.id).is_some() => Err(format!(
                "category {} is bound to a set of repositories, but the server lists none",
                category.code
            )),
            None => Err(format!(
                "no repository is bound to category {}; {bind}",
                category.code
            )),
            Some(bound) if rule.repos.len() != 1 || bound != rule.repos[0] => Err(format!(
                "category {} is bound to another repository than the rule's",
                category.code
            )),
            Some(_) => Ok(Vec::new()),
        };
    };
    let bound = bound_set_paths(entry, category, list).map_err(|m| {
        format!(
            "{m}; run `tmgr run {ws_code}:{task_id}` once in a terminal to bind the repositories of category {}",
            category.code
        )
    })?;
    let canonical = |p: &str| {
        std::fs::canonicalize(p)
            .ok()
            .and_then(|c| c.to_str().map(str::to_string))
            .unwrap_or_else(|| p.to_string())
    };
    let bound_paths: BTreeSet<String> = bound.iter().map(|r| r.path.clone()).collect();
    let rule_paths: BTreeSet<String> = rule.repos.iter().map(|r| canonical(r)).collect();
    if bound_paths != rule_paths || rule.repos.len() != bound.len() {
        return Err(format!(
            "category {} is bound to other repositories than the rule's",
            category.code
        ));
    }
    Ok(bound)
}

pub(crate) fn fresh_look(ctx: &Ctx, rule: &Rule, workspace_id: i64, task_id: i64) -> Result<Fresh> {
    CliFresh {
        ctx,
        local_user_id: ctx.cfg.local_user_id,
    }
    .look(
        rule,
        tmgr_ipc::api::TaskRef {
            workspace_id,
            task_id,
        },
    )
}

struct CliFresh<'a> {
    ctx: &'a Ctx,
    local_user_id: Option<i64>,
}

fn wanted_kind(rule: &Rule) -> &str {
    match rule.status_type.as_str() {
        "backlog" => "default",
        other => other,
    }
}

impl FreshTask for CliFresh<'_> {
    fn look(&self, rule: &Rule, task: tmgr_ipc::api::TaskRef) -> Result<Fresh> {
        let id = task.task_id;
        if rules::is_cloud(rule) {
            let client = self.ctx.cloud_client()?;
            let t =
                client.task_by_id(u64::try_from(id).map_err(|_| Error::usage("bad task id"))?)?;
            let wanted = wanted_kind(rule);
            let status_ok = client
                .statuses(task.workspace_id)?
                .iter()
                .any(|s| Some(s.id) == t.status_id && s.kind == wanted);
            let runs = match client.agent_work(t.id) {
                Ok(body) => running_agents_of(&body),
                Err(e) if matches!(e.kind, Kind::Forbidden | Kind::NotFound) => Vec::new(),
                Err(e) => return Err(e),
            };
            let persona_json = json!({"persona_assignees": t.persona_assignees});
            return Ok(assess(Facts {
                rule_persona: &rule.persona,
                me: client
                    .current_user_id()?
                    .ok_or_else(|| Error::auth("cannot tell which account is logged in"))?,
                owners: owners_of(&persona_json, &rule.persona),
                assignees: t
                    .assignees
                    .iter()
                    .map(|p| (p.id, p.name.clone().unwrap_or_else(|| "unknown".into())))
                    .collect(),
                personas: personas_of(&persona_json),
                running_agents: runs,
                status_ok,
            }));
        }
        let code = rules::workspace_code(&rule.workspace)?;
        let local = LocalTasks::open(code, self.local_user_id)?;
        let t = local.fetch(&format!("/api/tasks/{id}"))?;
        let wanted = wanted_kind(rule);
        let status_id = t.get("status_id").and_then(Value::as_i64);
        let status_ok = local
            .fetch("/api/workspaces/statuses")?
            .as_array()
            .is_some_and(|a| {
                a.iter().any(|s| {
                    s.get("id").and_then(Value::as_i64) == status_id
                        && s.get("type").and_then(Value::as_str) == Some(wanted)
                })
            });
        let runs = match local.fetch(&format!("/api/tasks/{id}/agent-work")) {
            Ok(body) => running_agents_of(&body),
            Err(e) if matches!(e.kind, Kind::Forbidden | Kind::NotFound) => Vec::new(),
            Err(e) => return Err(e),
        };
        Ok(assess(Facts {
            rule_persona: &rule.persona,
            me: local
                .owner_id()
                .ok_or_else(|| Error::auth("cannot tell which local account runs the queue"))?,
            owners: owners_of(&t, &rule.persona),
            assignees: people_of(&t),
            personas: personas_of(&t),
            running_agents: runs,
            status_ok,
        }))
    }

    fn comment(&self, rule: &Rule, task: tmgr_ipc::api::TaskRef, text: &str) -> Result<()> {
        if rules::is_cloud(rule) {
            let id = u64::try_from(task.task_id).map_err(|_| Error::usage("bad task id"))?;
            return self.ctx.cloud_client()?.add_comment(id, text);
        }
        let code = rules::workspace_code(&rule.workspace)?;
        let local = LocalTasks::open(code, self.local_user_id)?;
        let (status, body) = local.client().call(
            "POST",
            &format!("/api/tasks/{}/comments", task.task_id),
            Some(&json!({"message": text})),
            false,
        )?;
        if status >= 400 {
            return Err(tmgr_core::local::status_error(status, &body));
        }
        Ok(())
    }
}

struct CliPublisher<'a> {
    opts: &'a Opts<'a>,
    held: RefCell<Option<state::Held>>,
}

pub(crate) fn publish_target(pickup: &state::Pickup) -> Result<PublishTarget> {
    let claim = pickup
        .cloud
        .as_ref()
        .ok_or_else(|| Error::internal("only cloud pickups are published"))?;
    Ok(PublishTarget {
        workspace_code: claim.workspace_code.clone(),
        task_id: pickup.task.task_id,
        clone_dir: std::path::PathBuf::from(&pickup.bundle.launch.cwd),
        branch: claim.branch.clone(),
        session_id: pickup.run_id.clone(),
        agent: Some(pickup.bundle.launch.profile.clone()),
        repo: None,
        clones: pickup
            .clones
            .iter()
            .map(|c| CloneRef {
                name: c.name.clone(),
                identity: Some((c.dev, c.ino)),
            })
            .collect(),
    })
}

impl CliPublisher<'_> {
    fn publish_as(&self, pickup: &state::Pickup, mode: Mode) -> Result<String> {
        match publish_outcome(self.opts, &publish_target(pickup)?, mode)? {
            Outcome::Published(urls) => {
                Ok(urls.into_iter().next().map(|(_, u)| u).unwrap_or_default())
            }
            Outcome::Held { held, text } => {
                *self.held.borrow_mut() = Some(held);
                Err(state::held_error(&text))
            }
            Outcome::Reviewed => Err(Error::internal("unexpected outcome of a publish")),
        }
    }
}

impl Publisher for CliPublisher<'_> {
    fn publish(&self, pickup: &state::Pickup) -> Result<String> {
        self.publish_as(pickup, Mode::Publish)
    }

    fn publish_draft(&self, pickup: &state::Pickup) -> Result<String> {
        self.publish_as(pickup, Mode::Draft)
    }

    fn take_held(&self) -> Option<state::Held> {
        self.held.borrow_mut().take()
    }
}

pub(crate) struct RunnerRuns {
    cfg: tmgr_runner::daemon::RunnerConfig,
    client: RefCell<Option<tmgr_runner::client::Client>>,
    autostart: bool,
}

fn runner_refused(e: &std::io::Error) -> bool {
    let text = e.to_string();
    e.kind() == std::io::ErrorKind::Other
        && [
            "invalid_params: ",
            "unknown_profile: ",
            "forbidden: ",
            "sandbox: ",
            "busy: ",
            "limit: ",
            "not_found: ",
            "not_running: ",
            "unauthorized: ",
            "unknown_method: ",
            "proto_mismatch: ",
        ]
        .iter()
        .any(|code| text.starts_with(code))
}

fn answered_not_found(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::Other && e.to_string().starts_with("not_found: ")
}

impl RunnerRuns {
    pub(crate) fn new() -> Result<RunnerRuns> {
        Ok(RunnerRuns {
            cfg: super::runner::runner_config()?,
            client: RefCell::new(None),
            autostart: true,
        })
    }

    pub(crate) fn attach_only() -> Result<RunnerRuns> {
        Ok(RunnerRuns {
            autostart: false,
            ..RunnerRuns::new()?
        })
    }

    pub(crate) fn ready(&self) -> Result<()> {
        if self.client.borrow().is_some() {
            return Ok(());
        }
        if self.autostart {
            tmgr_runner::daemon::start_background(&self.cfg).map_err(super::runner::rerr)?;
        }
        let client = super::runner::connect(&self.cfg)?;
        client
            .set_timeouts(Duration::from_secs(30))
            .map_err(super::runner::rerr)?;
        if !runs::runner_supports_queue(&client.welcome().caps) {
            return Err(Error::new(
                Kind::Conflict,
                "the running runner is older than this tmgr and cannot start queue runs",
            )
            .hint(format!(
                "{0} runner stop && {0} runner start",
                super::runner::bin()
            )));
        }
        *self.client.borrow_mut() = Some(client);
        Ok(())
    }

    pub(crate) fn kill(
        &self,
        run_id: &str,
        signal: &str,
        reason: &str,
    ) -> std::result::Result<(), std::io::Error> {
        self.with(|c| c.kill_with_reason(run_id, signal, Some(reason)))
    }

    fn with<T>(
        &self,
        f: impl FnOnce(&mut tmgr_runner::client::Client) -> std::io::Result<T>,
    ) -> std::result::Result<T, std::io::Error> {
        self.ready()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::NotConnected, e.message))?;
        let mut slot = self.client.borrow_mut();
        let client = slot.as_mut().expect("connected");
        let out = f(client);
        if out.as_ref().is_err_and(|e| !answered_not_found(e)) {
            *slot = None;
        }
        out
    }
}

fn run_create_req(req: &StartRequest) -> RunCreateReq {
    RunCreateReq {
        clones: req.clones.clone(),
        profile: req.profile.clone(),
        task: Some(req.task),
        cwd: req.cwd.clone(),
        name: Some(req.name.clone()),
        model: req.model.clone(),
        cols: 120,
        rows: 40,
        bundle: Some(req.bundle.clone()),
        bundle_sha256: Some(req.bundle_sha256.clone()),
        cwd_pin: None,
        origin: None,
        permission_mode: Some(req.permission_mode.to_string()),
        start_key: Some(req.start_key.clone()),
        clone_of: req.clones.is_empty().then(|| req.clone_of.clone()),
        trust_cwd: None,
        trust_via_repo: None,
        agent_work: true,
        cloud: req.cloud.clone(),
        unattended: req.unattended,
        exe_pin: Some(ExePin {
            path: req.exe_path.clone(),
            sha256: req.exe_sha256.clone(),
        }),
        argv_sha256: Some(req.argv_sha256.clone()),
    }
}

fn team_supported(bundle: &tmgr_ipc::api::RunBundle, caps: &[String]) -> Result<()> {
    if bundle.team.is_some() && !caps.iter().any(|c| c == tmgr_ipc::api::caps::TEAM_V1) {
        return Err(Error::new(
            Kind::Conflict,
            "the runner is too old for team runs; restart it after the update",
        )
        .hint(format!(
            "{0} runner stop && {0} runner start",
            super::runner::bin()
        )));
    }
    Ok(())
}

impl RunsClient for RunnerRuns {
    fn create(&self, req: &StartRequest) -> Result<String> {
        self.ready()?;
        let caps = self
            .client
            .borrow()
            .as_ref()
            .map(|c| c.welcome().caps.clone())
            .unwrap_or_default();
        team_supported(&req.bundle, &caps)?;
        if tmgr_runner::bundle::has_owner_text(&req.bundle) {
            let supported = self.client.borrow().as_ref().is_some_and(|c| {
                c.welcome()
                    .caps
                    .iter()
                    .any(|have| have == tmgr_ipc::api::caps::SIGNED_CONTEXT_V1)
            });
            if !supported {
                return Err(Error::new(
                    Kind::Conflict,
                    format!(
                        "the running runner is older than this tmgr and cannot deliver persona instructions and workspace rules; run {0} runner stop && {0} runner start",
                        super::runner::bin()
                    ),
                ));
            }
        }
        if !req.clones.is_empty() {
            let supported = self.client.borrow().as_ref().is_some_and(|c| {
                c.welcome()
                    .caps
                    .iter()
                    .any(|have| have == tmgr_ipc::api::caps::MULTI_CLONE_V1)
            });
            if !supported {
                return Err(Error::new(
                    Kind::Conflict,
                    format!(
                        "the running runner is older than this tmgr and cannot run several repositories; run {0} runner stop && {0} runner start",
                        super::runner::bin()
                    ),
                ));
            }
        }
        self.with(|c| c.create_run(run_create_req(req)))
            .map(|info| info.id)
            .map_err(|e| {
                if runner_refused(&e) {
                    super::runner::rerr(e)
                } else {
                    Error::new(
                        Kind::Offline,
                        format!("the runner did not confirm the start: {e}"),
                    )
                }
            })
    }

    fn status(&self, run_id: &str) -> RunState {
        match self.with(|c| c.run_status(run_id)) {
            Ok(s) if !s.alive || s.state == StateKind::Exited => {
                if s.exit_code == Some(0) && s.signal.is_none() {
                    RunState::Succeeded
                } else {
                    RunState::Exited
                }
            }
            Ok(_) => RunState::Running,
            Err(e) if answered_not_found(&e) => RunState::Exited,
            Err(_) => RunState::Unknown,
        }
    }

    fn lookup(&self, start_key: &str) -> Lookup {
        match self.with(|c| c.lookup(start_key)) {
            Ok(info) => Lookup::Found(info.id),
            Err(e) if answered_not_found(&e) => Lookup::NotFound,
            Err(_) => Lookup::Unsupported,
        }
    }
}

pub(crate) fn with_deps<T>(
    ctx: &Ctx,
    cli: &Cli,
    runs: &RunnerRuns,
    policy: tmgr_queue::StartPolicy,
    f: impl FnOnce(&Deps) -> Result<T>,
) -> Result<T> {
    let source = LocalSource {
        local_user_id: ctx.cfg.local_user_id,
    };
    let cloud = CliCloud::new(ctx);
    let fresh = CliFresh {
        ctx,
        local_user_id: ctx.cfg.local_user_id,
    };
    let opts = Opts {
        cli,
        ws: cli.ws.clone(),
        json: cli.json,
        quiet: cli.quiet,
        style: stdout_style(cli.no_color),
    };
    let publisher = CliPublisher {
        opts: &opts,
        held: RefCell::new(None),
    };
    let now = Local::now();
    f(&Deps {
        source: &source,
        cloud: &cloud,
        publisher: &publisher,
        runs,
        profiles: &RunnerProfiles,
        workdirs: &GitClone,
        fresh: &fresh,
        now: Utc::now(),
        local_minutes: now.hour() * 60 + now.minute(),
        policy,
    })
}

fn run_once(ctx: &Ctx, cli: &Cli, paths: &Paths) -> Result<Output> {
    not_in_agent_session()?;
    let client = RunnerRuns::new()?;
    if tmgr_queue::needs_runner(paths)? {
        client.ready()?;
    }
    let report = with_deps(
        ctx,
        cli,
        &client,
        tmgr_queue::StartPolicy::default(),
        |deps| tmgr_queue::run_once(paths, deps),
    )?;
    let mut lines = Vec::new();
    if !report.enabled {
        lines.push("Queue is off.".to_string());
    } else if report.paused {
        lines.push("Queue is paused.".to_string());
    } else if report.events.is_empty() {
        lines.push("Nothing to do.".to_string());
    }
    for e in &report.events {
        lines.push(format!(
            "{} {} {}",
            e["task"].as_str().unwrap_or(""),
            e["outcome"].as_str().unwrap_or(""),
            e["reason"].as_str().unwrap_or("")
        ));
    }
    Ok(Output::new(&report, lines.join("\n")))
}

#[cfg(test)]
mod publish_target_tests {
    use super::*;

    fn pickup(cloud: bool) -> state::Pickup {
        let mut v = json!({
            "id": "q_1", "rule": "r", "task": {"workspace_id": 56, "task_id": 9002}, "key": "TM-410",
            "status": "started",
            "bundle": {"v": 1, "task": {"workspace_id": 56, "task_id": 9002}, "title": "t", "body": "b",
                "author": {"kind": "human"}, "comments": [], "pages": [],
                "launch": {"profile": "claude-code", "cwd": "/work/repo-worktrees/queue/q_1"}},
            "bundle_sha256": "x", "context_tainted": false,
            "created_at": "2026-10-19T00:00:00Z", "updated_at": "2026-10-19T00:00:00Z",
            "run_id": "s_abc"
        });
        if cloud {
            v["cloud"] = json!({
                "claim": {"host": "api.tmgr.dev", "user_id": 1, "workspace_id": 56, "task_id": 9002,
                    "category_id": 127, "content_sha256": "h", "key_id": "k", "signed_at": 1,
                    "signature": "s", "kind": "author"},
                "workspace_code": "other", "branch": "tmgr/tm-410-fix"
            });
        }
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn the_target_comes_from_the_pickup_and_its_rule_workspace() {
        let t = publish_target(&pickup(true)).unwrap();
        assert_eq!(t.workspace_code, "other");
        assert_eq!(t.task_id, 9002);
        assert_eq!(t.branch, "tmgr/tm-410-fix");
        assert_eq!(
            t.clone_dir,
            std::path::Path::new("/work/repo-worktrees/queue/q_1")
        );
        assert_eq!(t.session_id.as_deref(), Some("s_abc"));
        assert!(publish_target(&pickup(false)).is_err());
    }
}

#[cfg(test)]
mod run_request_tests {
    use super::*;

    fn start_request() -> StartRequest {
        StartRequest {
            start_key: "q_1-1".into(),
            permission_mode: "accept-edits",
            rule: "r".into(),
            trigger: "rule:r".into(),
            profile: "claude-code".into(),
            model: Some("opus".into()),
            cwd: "/work/repo-worktrees/queue/q_1".into(),
            clone_of: "/work/repo".into(),
            exe_path: "/bin/claude".into(),
            exe_sha256: "e".repeat(64),
            argv_sha256: "a".repeat(64),
            name: "TM-410 rule:r".into(),
            task: tmgr_ipc::api::TaskRef {
                workspace_id: 56,
                task_id: 9002,
            },
            bundle: serde_json::from_value(json!({
                "v": 1, "task": {"workspace_id": 56, "task_id": 9002}, "title": "t", "body": "b",
                "author": {"kind": "human"}, "comments": [], "pages": [],
                "launch": {"profile": "claude-code", "cwd": "/work/repo-worktrees/queue/q_1"}
            }))
            .unwrap(),
            bundle_sha256: "b".repeat(64),
            cloud: serde_json::from_value(json!({
                "host": "api.tmgr.dev", "user_id": 1, "workspace_id": 56, "task_id": 9002,
                "category_id": 127, "content_sha256": "h", "key_id": "k", "signed_at": 1,
                "signature": "s", "kind": "author"
            }))
            .unwrap(),
            unattended: true,
            clones: Vec::new(),
        }
    }

    #[test]
    fn golden_single_repo_queue_request() {
        let text = serde_json::to_string(&run_create_req(&start_request())).unwrap();
        assert_eq!(
            text,
            r#"{"profile":"claude-code","task":{"workspace_id":56,"task_id":9002},"cwd":"/work/repo-worktrees/queue/q_1","name":"TM-410 rule:r","model":"opus","cols":120,"rows":40,"bundle":{"v":1,"task":{"workspace_id":56,"task_id":9002},"title":"t","body":"b","author":{"kind":"human"},"comments":[],"pages":[],"launch":{"profile":"claude-code","cwd":"/work/repo-worktrees/queue/q_1"}},"bundle_sha256":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","permission_mode":"accept-edits","start_key":"q_1-1","clone_of":"/work/repo","exe_pin":{"path":"/bin/claude","sha256":"eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"},"argv_sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","agent_work":true,"cloud":{"host":"api.tmgr.dev","user_id":1,"workspace_id":56,"task_id":9002,"category_id":127,"content_sha256":"h","key_id":"k","signed_at":1,"signature":"s","kind":"author"},"unattended":true}"#
        );
    }

    #[test]
    fn a_multi_request_sends_the_clones_and_no_clone_of() {
        let mut req = start_request();
        req.clones = vec![
            tmgr_ipc::api::CloneSpec {
                name: "backend".into(),
                clone_of: "/work/be".into(),
            },
            tmgr_ipc::api::CloneSpec {
                name: "frontend".into(),
                clone_of: "/work/fe".into(),
            },
        ];
        let v = serde_json::to_value(run_create_req(&req)).unwrap();
        assert!(v.get("clone_of").is_none());
        assert_eq!(
            v["clones"],
            json!([
                {"name": "backend", "clone_of": "/work/be"},
                {"name": "frontend", "clone_of": "/work/fe"}
            ])
        );
        assert_eq!(v["agent_work"], json!(true));
    }
}

#[cfg(test)]
mod rule_binding_tests {
    use super::*;
    use crate::cmd::runner::cloud::{Category, GateOk};
    use std::collections::BTreeMap;
    use std::path::Path;
    use tmgr_core::approval::{Entry, RepoBinding};

    fn rule(repos: &[&Path]) -> Rule {
        Rule {
            name: "r".into(),
            workspace: "tmgrdev".into(),
            project: Some("TM".into()),
            status_type: "backlog".into(),
            persona: "p".into(),
            profile: "claude-code".into(),
            model: None,
            repos: repos.iter().map(|p| p.display().to_string()).collect(),
            max_concurrent: 1,
            hours: None,
            trusted_authors: vec!["human".into()],
            auto: true,
            mode: None,
        }
    }

    fn entry(binding: Option<RepoBinding>) -> Entry {
        let mut repos = BTreeMap::new();
        if let Some(b) = binding {
            repos.insert("127".to_string(), b);
        }
        Entry {
            server: "s".into(),
            user_id: 1,
            workspace_id: 56,
            workspace_code: "tmgrdev".into(),
            allowed_at: "2025-10-01T00:00:00Z".into(),
            expires_at: None,
            keys: vec![],
            repos,
        }
    }

    fn category(github: Option<&str>) -> Category {
        serde_json::from_value(json!({"id": 127, "code": "TM", "github_full_name": github}))
            .unwrap()
    }

    fn gate_ok(repos: Option<&[&str]>) -> GateOk {
        GateOk {
            claim: serde_json::from_value(json!({
                "host": "s", "user_id": 1, "workspace_id": 56, "task_id": 1, "category_id": 127,
                "content_sha256": "h", "key_id": "k", "signed_at": 1, "signature": "s", "kind": "author"
            }))
            .unwrap(),
            title: "t".into(),
            body: String::new(),
            category: None,
            task_key: "TM-1".into(),
            repos: repos.map(|r| r.iter().map(|s| s.to_string()).collect()),
            persona: None,
            context: Vec::new(),
        }
    }

    fn set(pairs: &[(&str, &Path)]) -> RepoBinding {
        RepoBinding::Set(
            pairs
                .iter()
                .map(|(n, p)| (n.to_string(), p.display().to_string()))
                .collect(),
        )
    }

    fn check(
        rule: &Rule,
        ok: &GateOk,
        e: &Entry,
        github: Option<&str>,
    ) -> std::result::Result<Vec<tmgr_queue::clones::ClaimRepo>, String> {
        rule_binding(rule, ok, e, &category(github), "tmgrdev", 1)
    }

    #[test]
    fn an_explicit_set_needs_the_rule_to_hold_exactly_the_bound_folders() {
        let d = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(d.path()).unwrap();
        let (be, fe, extra) = (root.join("be"), root.join("fe"), root.join("extra"));
        for p in [&be, &fe, &extra] {
            std::fs::create_dir(p).unwrap();
        }
        let names = ["tmgr-dev/backend", "tmgr-dev/frontend"];
        let ok = gate_ok(Some(&names));
        let e = entry(Some(set(&[(names[0], &be), (names[1], &fe)])));
        let found = check(&rule(&[&fe, &be]), &ok, &e, None).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].full_name, "tmgr-dev/backend");
        assert!(check(&rule(&[&be]), &ok, &e, None).is_err());
        assert!(check(&rule(&[&be, &fe, &extra]), &ok, &e, None).is_err());
        assert!(check(&rule(&[&be, &extra]), &ok, &e, None).is_err());
        assert!(check(&rule(&[&be, &be]), &ok, &e, None).is_err());
        let path_bound = entry(Some(RepoBinding::Path(be.display().to_string())));
        let m = check(&rule(&[&be, &fe]), &ok, &path_bound, None).unwrap_err();
        assert!(
            m.contains(
                "`tmgr run tmgrdev:1` once in a terminal to bind the repositories of category TM"
            ),
            "{m}"
        );
        assert!(check(&rule(&[&be, &fe]), &ok, &entry(None), None).is_err());
    }

    #[test]
    fn a_set_with_one_name_still_needs_the_rule_to_match() {
        let d = tempfile::tempdir().unwrap();
        let be = std::fs::canonicalize(d.path()).unwrap();
        let ok = gate_ok(Some(&["tmgr-dev/backend"]));
        let e = entry(Some(set(&[("tmgr-dev/backend", &be)])));
        assert_eq!(check(&rule(&[&be]), &ok, &e, None).unwrap().len(), 1);
    }

    #[test]
    fn a_legacy_category_needs_exactly_one_rule_repo_equal_to_the_bound_path() {
        let d = tempfile::tempdir().unwrap();
        let be = std::fs::canonicalize(d.path()).unwrap();
        let other = be.join("other");
        let ok = gate_ok(None);
        let e = entry(Some(RepoBinding::Path(be.display().to_string())));
        assert!(check(&rule(&[&be]), &ok, &e, Some("acme/x"))
            .unwrap()
            .is_empty());
        assert!(check(&rule(&[&other]), &ok, &e, Some("acme/x")).is_err());
        assert!(check(&rule(&[&be, &other]), &ok, &e, Some("acme/x")).is_err());
        assert!(check(&rule(&[&be]), &ok, &e, None).is_err());
        let m = check(&rule(&[&be]), &ok, &entry(None), Some("acme/x")).unwrap_err();
        assert!(m.contains("once in a terminal to bind a repository"), "{m}");
        let s = entry(Some(set(&[("acme/x", &be)])));
        let m = check(&rule(&[&be]), &ok, &s, Some("acme/x")).unwrap_err();
        assert!(m.contains("set of repositories"), "{m}");
    }
}

#[cfg(test)]
mod team_cap_tests {
    use super::*;
    use tmgr_ipc::api::{caps, BundleTeam, RunBundle, TeamMode};

    fn bundle(team: Option<BundleTeam>) -> RunBundle {
        let mut b: RunBundle = serde_json::from_str(include_str!(
            "../../../tmgr-ipc/tests/fixtures/run_bundle.json"
        ))
        .unwrap();
        b.team = team;
        b
    }

    fn team() -> Option<BundleTeam> {
        Some(BundleTeam {
            size: 2,
            mode: TeamMode::Eco,
        })
    }

    #[test]
    fn a_team_needs_the_team_capability() {
        let old = vec![caps::SIGNED_CONTEXT_V1.to_string()];
        let new = vec![caps::TEAM_V1.to_string()];
        let e = team_supported(&bundle(team()), &old).unwrap_err();
        assert!(e.message.contains("too old for team runs"), "{}", e.message);
        assert!(team_supported(&bundle(team()), &new).is_ok());
        assert!(team_supported(&bundle(None), &old).is_ok());
    }
}
