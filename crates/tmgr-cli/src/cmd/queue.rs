use crate::ctx::Ctx;
use crate::out::{table, Output};
use chrono::{Local, Timelike, Utc};
use clap::{Args, Subcommand};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::time::Duration;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::text::sanitize;
use tmgr_ipc::api::{ExePin, RunCreateReq, StateKind};
use tmgr_link::privileged;
use tmgr_queue::audit::sha256_hex;
use tmgr_queue::audit::Audit;
use tmgr_queue::rules::{self, ProfilePin, ProfileResolver, QueueFile, Rule};
use tmgr_queue::runs::{self, Lookup, RunState, RunsClient, StartRequest};
use tmgr_queue::source::LocalSource;
use tmgr_queue::state::{self, Status};
use tmgr_queue::workdir::GitClone;
use tmgr_queue::{Deps, Paths};

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
    Status,
    /// Stop picking up work immediately
    Pause,
    /// Continue picking up work
    Resume,
    /// Turn the queue on (needs a terminal and a typed confirmation)
    Enable,
    /// Turn the queue off
    Disable,
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
    /// Local workspace, written local:<code>
    #[arg(long, value_name = "local:CODE")]
    pub workspace: String,
    /// Persona the task must be assigned to
    #[arg(long, value_name = "NAME")]
    pub persona: String,
    /// Agent profile id
    #[arg(long, value_name = "PROFILE")]
    pub profile: String,
    /// Existing directory the agent works in
    #[arg(long, value_name = "DIR")]
    pub repo: String,
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
}

pub struct RunnerProfiles;

fn launch_argv(
    p: &tmgr_runner::profile::Profile,
    model: Option<&str>,
) -> std::result::Result<Vec<String>, String> {
    p.argv_with(model, p.permission_args(runs::PERMISSION_MODE)?)
}

impl ProfileResolver for RunnerProfiles {
    fn argv(&self, profile: &str, model: Option<&str>) -> std::result::Result<Vec<String>, String> {
        let dir = tmgr_runner::profile::user_dir();
        let (set, _) = tmgr_runner::profile::ProfileSet::load(dir.as_deref());
        launch_argv(set.lookup(profile)?, model)
    }

    fn pin(&self, profile: &str, model: Option<&str>) -> std::result::Result<ProfilePin, String> {
        let dir = tmgr_runner::profile::user_dir();
        let (set, _) = tmgr_runner::profile::ProfileSet::load(dir.as_deref());
        let p = set.lookup(profile)?;
        let argv = launch_argv(p, model)?;
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

fn not_in_agent_session() -> Result<()> {
    if in_agent_session() {
        return Err(
            Error::new(Kind::Forbidden, "refused inside an agent session")
                .hint("run it yourself in a normal terminal"),
        );
    }
    Ok(())
}

fn agree(summary: &str) -> Result<()> {
    privileged::guard()?;
    eprintln!("{WARNING}");
    if !summary.is_empty() {
        eprintln!("\n{summary}");
    }
    privileged::confirm_typed_stdio("I AGREE", "\nType I AGREE to continue:")
}

fn describe(r: &Rule) -> String {
    format!(
        "{}: {} {} persona {} profile {}{} repo {} max {} hours {} authors {}{}",
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
        r.repo,
        r.max_concurrent,
        r.hours.as_deref().unwrap_or("any"),
        r.trusted_authors.join("+"),
        if r.auto { " AUTO" } else { " approval needed" }
    )
}

fn rules_summary(f: &QueueFile) -> String {
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

pub fn run(ctx: &Ctx, cmd: &QueueCommand) -> Result<Output> {
    let paths = Paths::from_env()?;
    match cmd {
        QueueCommand::Status => status(&paths),
        QueueCommand::Pause => set_paused(&paths, true),
        QueueCommand::Resume => {
            not_in_agent_session()?;
            set_paused(&paths, false)
        }
        QueueCommand::Enable => enable(&paths),
        QueueCommand::Disable => disable(&paths),
        QueueCommand::Rules { command } => match command {
            RulesCmd::List => list(&paths),
            RulesCmd::Add(a) => add(&paths, a),
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
        QueueCommand::RunOnce => run_once(ctx, &paths),
    }
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
            json!({"id": p.id, "rule": p.rule, "task": p.key, "status": p.status.name(),
                "bundle_sha256": p.bundle_sha256, "tainted": p.context_tainted,
                "run": p.run_id, "reason": p.reason})
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
                p.status.name().to_string(),
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
            "audit": audit, "rules": loaded.file.rules, "pickups": pickups}),
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

fn enable(paths: &Paths) -> Result<Output> {
    privileged::guard()?;
    let shown = rules::load(paths)?;
    rules::validate_file(&shown.file, &RunnerProfiles)?;
    agree(&rules_summary(&shown.file))?;
    let _lock = paths.lock()?;
    let now = rules::load(paths)?;
    if now.sha256 != shown.sha256 {
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

fn disable(paths: &Paths) -> Result<Output> {
    let _lock = paths.lock()?;
    let mut file = rules::load(paths)?.file;
    file.enabled = false;
    let sha = rules::save(paths, &file)?;
    record(paths, "disable", &sha, json!({}))?;
    Ok(Output::new(json!({"enabled": false}), "Queue is off."))
}

fn list(paths: &Paths) -> Result<Output> {
    let loaded = rules::load(paths)?;
    Ok(Output::new(
        json!({"rules": loaded.file.rules}),
        rules_summary(&loaded.file),
    ))
}

fn add(paths: &Paths, a: &RuleArgs) -> Result<Output> {
    privileged::guard()?;
    let repo = std::fs::canonicalize(&a.repo)
        .map_err(|_| Error::usage("repo is not an existing directory"))?;
    let rule = Rule {
        name: a.name.clone(),
        workspace: a.workspace.clone(),
        project: a.project.clone(),
        status_type: a.status_type.clone(),
        persona: a.persona.clone(),
        profile: a.profile.clone(),
        model: a.model.clone(),
        repo: repo.to_string_lossy().into_owned(),
        max_concurrent: a.max_concurrent,
        hours: a.hours.clone(),
        trusted_authors: if a.trusted_authors.is_empty() {
            vec!["human".into()]
        } else {
            a.trusted_authors.clone()
        },
        auto: a.auto,
    };
    rules::validate_rule(&rule, &RunnerProfiles)?;
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

struct RunnerRuns {
    cfg: tmgr_runner::daemon::RunnerConfig,
    client: RefCell<Option<tmgr_runner::client::Client>>,
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
    fn new() -> Result<RunnerRuns> {
        Ok(RunnerRuns {
            cfg: super::runner::runner_config()?,
            client: RefCell::new(None),
        })
    }

    fn ready(&self) -> Result<()> {
        if self.client.borrow().is_some() {
            return Ok(());
        }
        tmgr_runner::daemon::start_background(&self.cfg).map_err(super::runner::rerr)?;
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

impl RunsClient for RunnerRuns {
    fn create(&self, req: &StartRequest) -> Result<String> {
        self.ready()?;
        self.with(|c| {
            c.create_run(RunCreateReq {
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
                clone_of: Some(req.clone_of.clone()),
                trust_cwd: None,
                agent_work: true,
                exe_pin: Some(ExePin {
                    path: req.exe_path.clone(),
                    sha256: req.exe_sha256.clone(),
                }),
                argv_sha256: Some(req.argv_sha256.clone()),
            })
        })
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
            Ok(s) if !s.alive || s.state == StateKind::Exited => RunState::Exited,
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

fn run_once(ctx: &Ctx, paths: &Paths) -> Result<Output> {
    not_in_agent_session()?;
    let source = LocalSource {
        local_user_id: ctx.cfg.local_user_id,
    };
    let client = RunnerRuns::new()?;
    if tmgr_queue::needs_runner(paths)? {
        client.ready()?;
    }
    let now = Local::now();
    let report = tmgr_queue::run_once(
        paths,
        &Deps {
            source: &source,
            runs: &client,
            profiles: &RunnerProfiles,
            workdirs: &GitClone,
            now: Utc::now(),
            local_minutes: now.hour() * 60 + now.minute(),
        },
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
