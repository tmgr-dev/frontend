use super::{attach, connect, emit, rerr, runner_config, Opts, PermissionMode, RunArgs};
use serde_json::{json, Value};
use std::io::IsTerminal;
use std::path::PathBuf;
use tmgr_core::branch;
use tmgr_core::config::{self, Config};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::local;
use tmgr_core::refs::{parse_task_ref, TaskRef as RefKind};
use tmgr_core::text::sanitize_line;
use tmgr_ipc::api::limits::MAX_BUNDLE;
use tmgr_ipc::api::{
    sha256_hex, BundleAuthor, BundleLaunch, BundlePersona, RunBundle, RunCreateReq,
};
use tmgr_queue::source::{Candidate, LocalTasks};
use tmgr_queue::workdir;
use tmgr_runner::daemon;

struct Plan {
    candidate: Candidate,
    persona: Option<String>,
    repo: String,
    clone: PathBuf,
    code: String,
    branch: String,
    chain_ok: bool,
}

fn record_path(code: &str, task_id: i64) -> Option<PathBuf> {
    tmgr_core::active::state_dir()
        .map(|d| d.join("task-runs").join(format!("{code}-{task_id}.json")))
}

fn remember(p: &Plan) -> Result<()> {
    let path = record_path(&p.code, p.candidate.task.task_id)
        .ok_or_else(|| Error::internal("no state directory for the run record"))?;
    let body = json!({ "repo": p.repo, "clone": p.clone, "branch": p.branch });
    if let Some(dir) = path.parent() {
        config::ensure_private_dir(dir)?;
    }
    config::write_atomic(&path, body.to_string().as_bytes(), 0o600)
}

fn record(p: &Plan) -> Option<(String, PathBuf, String)> {
    let text = std::fs::read_to_string(record_path(&p.code, p.candidate.task.task_id)?).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    Some((
        v["repo"].as_str()?.to_string(),
        PathBuf::from(v["clone"].as_str()?),
        v["branch"].as_str()?.to_string(),
    ))
}

fn recalled(p: &mut Plan) {
    if let Some((repo, clone, branch)) = record(p) {
        p.repo = repo;
        p.clone = clone;
        p.branch = branch;
    }
}

fn import_line(code: &str, reference: &str) -> String {
    format!(
        "{} --ws local:{code} run {} --fetch",
        super::bin(),
        shell_words::quote(reference)
    )
}

fn local_code(o: &Opts, cfg: &Config) -> Result<String> {
    let reference = o.ws.clone().or_else(|| cfg.default_workspace.clone());
    reference
        .as_deref()
        .and_then(local::local_code)
        .map(str::to_string)
        .ok_or_else(|| {
            Error::usage(
                "tmgr run starts tasks of local workspaces only (owner decision 16a.11); this is a cloud workspace",
            )
            .hint("pass --ws local:<code> for a task in a local workspace, or move the task to one first")
        })
}

fn task_id(tasks: &LocalTasks, reference: &str) -> Result<i64> {
    match parse_task_ref(reference)? {
        RefKind::Id(id) => i64::try_from(id).map_err(|_| Error::usage("invalid task reference")),
        RefKind::Key(key) => tasks
            .fetch(&format!("/api/tasks/by-key/{key}"))?
            .get("id")
            .and_then(Value::as_i64)
            .ok_or_else(|| Error::not_found(format!("task {key} not found"))),
    }
}

fn repo_of(a: &RunArgs, cfg: &Config, key: &str) -> Result<String> {
    let path = match (&a.cwd, key.split_once('-')) {
        (Some(dir), _) => dir.clone(),
        (None, Some((code, _))) => cfg
            .projects
            .get(code)
            .or_else(|| {
                cfg.projects
                    .iter()
                    .find(|(c, _)| c.eq_ignore_ascii_case(code))
                    .map(|(_, p)| p)
            })
            .map(PathBuf::from)
            .ok_or_else(|| {
                Error::usage(format!("no repository is mapped to project {code}")).hint(format!(
                    "tmgr project map {code} <repo> or pass --cwd <repo>"
                ))
            })?,
        (None, None) => {
            return Err(
                Error::usage("the task has no project, so no repository is known")
                    .hint("pass --cwd <repo>"),
            )
        }
    };
    let repo = std::fs::canonicalize(&path)
        .map_err(|e| Error::usage(format!("repository {}: {e}", path.display())))?;
    if !repo.join(".git").is_dir() {
        return Err(Error::usage(format!(
            "{} is not the root of a git repository",
            repo.display()
        )));
    }
    repo.to_str()
        .map(str::to_string)
        .ok_or_else(|| Error::usage("the repository path is not valid UTF-8"))
}

fn plan(o: &Opts, a: &RunArgs, reference: &str) -> Result<Plan> {
    let cfg = Config::load(&config::config_dir()?)?;
    let code = local_code(o, &cfg)?;
    let tasks = LocalTasks::open(&code, cfg.local_user_id)?;
    let id = task_id(&tasks, reference)?;
    let full = tasks.fetch(&format!("/api/tasks/{id}"))?;
    let persona = full
        .get("persona_assignees")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
        .cloned()
        .unwrap_or(Value::Null);
    let candidate = tasks.candidate(id, &persona)?;
    if candidate.key.starts_with('#') {
        return Err(
            Error::usage("the task has no key, so no branch name can be made")
                .hint("give the task a project in the local workspace"),
        );
    }
    let repo = repo_of(a, &cfg, &candidate.key)?;
    let branch = branch::render_template(
        cfg.branch_template(),
        &candidate.key,
        &branch::slug(&candidate.title),
    )?;
    let clone = workdir::run_clone_dir(&repo, &branch)
        .ok_or_else(|| Error::usage("the repository path has no parent directory"))?;
    Ok(Plan {
        code,
        persona: persona
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string),
        chain_ok: tasks.chain_ok(),
        candidate,
        repo,
        clone,
        branch,
    })
}

fn bundle(
    p: &Plan,
    profile: &str,
    model: Option<&str>,
    mode: &str,
    cwd: &str,
) -> Result<RunBundle> {
    let c = &p.candidate;
    let bundle = RunBundle {
        v: 1,
        task: c.task,
        title: c.title.clone(),
        body: c.body.clone(),
        author: BundleAuthor {
            kind: c.author.bundle_kind(),
            id: None,
        },
        comments: c.comments.clone(),
        pages: c.pages.clone(),
        persona: p.persona.as_ref().map(|name| BundlePersona {
            name: name.clone(),
            instructions_sha256: sha256_hex(c.persona_text.as_bytes()),
        }),
        launch: BundleLaunch {
            profile: profile.to_string(),
            model: model.map(str::to_string),
            cwd: cwd.to_string(),
            permission_mode: Some(mode.to_string()),
        },
    };
    bundle.validate().map_err(Error::usage)?;
    if bundle.canonical().len() > MAX_BUNDLE {
        return Err(Error::usage(
            "the run bundle is larger than 1 MiB (task text and comments together)",
        ));
    }
    Ok(bundle)
}

fn summary(p: &Plan, profile: &str, mode: &str) -> String {
    let c = &p.candidate;
    let clone = p.clone.display().to_string();
    [
        format!("Task:    {} {}", c.key, sanitize_line(&c.title)),
        format!(
            "Author:  {}{}",
            c.author.name(),
            if p.chain_ok {
                ""
            } else {
                " (the edit journal does not verify)"
            }
        ),
        format!("Repo:    {}", sanitize_line(&p.repo)),
        format!("Clone:   {}", sanitize_line(&clone)),
        format!("Branch:  {}", p.branch),
        format!("Agent:   {} in {mode} mode", sanitize_line(profile)),
    ]
    .join("\n")
}

pub fn run(o: &Opts, a: &RunArgs, reference: &str) -> Result<i32> {
    let profile = a
        .agent
        .clone()
        .ok_or_else(|| Error::usage("pass --agent <profile> to run a task"))?;
    let mode = a.mode.unwrap_or(PermissionMode::Ask).as_str();
    let p = plan(o, a, reference)?;
    if !o.json && !o.quiet {
        eprintln!("{}", summary(&p, &profile, mode));
    }
    crate::interact::confirm("Start this run?", true, a.yes)?;
    super::ops::login_preflight(&profile)?;
    let earlier = record(&p)
        .map(|(_, clone, _)| clone)
        .filter(|clone| clone.exists());
    if let Some(taken) = earlier.as_ref().or(Some(&p.clone).filter(|c| c.exists())) {
        return Err(Error::new(
            Kind::Conflict,
            format!("{} already exists", taken.display()),
        )
        .hint(format!(
            "bring its branch in with `{}`, then remove the folder and run again",
            import_line(&p.code, reference)
        )));
    }
    remember(&p)?;
    workdir::prepare_clone(&p.repo, &p.clone, &p.branch, a.base.as_deref())?;
    let cwd = std::fs::canonicalize(&p.clone)
        .map_err(|e| Error::internal(format!("clone {}: {e}", p.clone.display())))?
        .to_string_lossy()
        .into_owned();
    let bundle = bundle(&p, &profile, a.model.as_deref(), mode, &cwd)?;
    let sha = bundle.sha256();
    let (cols, rows) = super::ops::terminal_size();
    let cfg = runner_config()?;
    daemon::start_background(&cfg).map_err(rerr)?;
    let mut client = connect(&cfg)?;
    let info = client
        .create_run(RunCreateReq {
            profile: profile.clone(),
            task: Some(p.candidate.task),
            cwd: cwd.clone(),
            name: Some(a.name.clone().unwrap_or_else(|| p.candidate.key.clone())),
            model: a.model.clone(),
            cols,
            rows,
            bundle: Some(bundle),
            bundle_sha256: Some(sha),
            cwd_pin: None,
            origin: None,
            permission_mode: Some(mode.to_string()),
            start_key: None,
            clone_of: Some(p.repo.clone()),
            trust_cwd: None,
            agent_work: true,
            exe_pin: None,
        })
        .map_err(rerr)?;
    let fetch = import_line(&p.code, reference);
    if !a.no_attach && !o.json && std::io::stdout().is_terminal() {
        eprintln!(
            "The agent's commits stay in its clone; bring its branch into your repository with:\n  {fetch}"
        );
        let code = attach::run(&cfg, &info.id)?;
        eprintln!("To bring the agent's branch into your repository:\n  {fetch}");
        return Ok(code);
    }
    let bin = super::bin();
    let human = format!(
        "started {} ({}) in {}\nattach with: {bin} attach {}\nto bring the agent's branch into your repository:\n  {fetch}",
        sanitize_line(&info.name),
        info.id,
        sanitize_line(&cwd),
        info.id
    );
    Ok(emit(
        o,
        json!({
            "session": info,
            "repo": p.repo,
            "clone": cwd,
            "branch": p.branch,
            "author": p.candidate.author.name(),
            "fetch": fetch,
        }),
        human,
    ))
}

pub fn fetch(o: &Opts, a: &RunArgs, reference: &str) -> Result<i32> {
    let mut p = plan(o, a, reference)?;
    recalled(&mut p);
    if !p.clone.is_dir() {
        return Err(Error::not_found(format!(
            "there is no clone at {}",
            p.clone.display()
        )));
    }
    let sha = workdir::import_branch(&p.repo, &p.clone, &p.branch)?;
    let human = format!(
        "{} is now at {} in {}",
        p.branch,
        &sha[..12],
        sanitize_line(&p.repo)
    );
    Ok(emit(
        o,
        json!({ "repo": p.repo, "clone": p.clone, "branch": p.branch, "commit": sha }),
        human,
    ))
}
