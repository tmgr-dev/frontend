use super::cloud::{
    bind_checked, bound_set_with, canonical_root, confirm_replace, parse_eligibility, verify_repo,
    Category,
};
use super::{allow::host_of, emit, Opts};
use crate::cmd::queue::CliCloud;
use crate::ctx::Ctx;
use crate::interact;
use crate::out::table;
use chrono::Utc;
use clap::Args;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tmgr_core::api::Client;
use tmgr_core::approval::{AllowFile, Entry, RepoBinding};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::refs::{is_category_code, parse_task_ref, TaskRef};
use tmgr_core::text::sanitize_line;
use tmgr_link::privileged;
use tmgr_queue::cloud::CloudApi;
use tmgr_queue::rules::Rule;

#[derive(Args)]
pub struct BindArgs {
    /// WS:CODE (category code), WS:TM-12 or WS:123 (a task of the category), or owner/repo to move a repository of an already bound set
    #[arg(value_name = "TARGET")]
    pub target: String,
    /// Your clone of the category's repository
    #[arg(value_name = "PATH", value_parser = crate::paths::PathArg)]
    pub path: PathBuf,
}

#[derive(Args)]
pub struct UnbindArgs {
    /// WS:CODE (category code), WS:TM-12 or WS:123 (a task of the category)
    #[arg(value_name = "TARGET")]
    pub target: String,
}

enum Rest {
    Category(String),
    Task(TaskRef),
}

enum Target {
    Category { ws: String, rest: Rest },
    Repo(String),
}

fn clean(s: &str) -> String {
    sanitize_line(s)
}

fn parse_target(raw: &str) -> Result<Target> {
    if let Some((ws, rest)) = raw.split_once(':') {
        if ws.is_empty() {
            return Err(Error::usage("the target starts with a workspace code"));
        }
        let rest = if is_category_code(rest) {
            Rest::Category(rest.to_string())
        } else {
            Rest::Task(parse_task_ref(rest)?)
        };
        return Ok(Target::Category {
            ws: ws.to_string(),
            rest,
        });
    }
    match raw.split_once('/') {
        Some((owner, name)) if !owner.is_empty() && !name.is_empty() && !name.contains('/') => {
            Ok(Target::Repo(raw.to_string()))
        }
        _ => Err(Error::usage(format!(
            "{:?} is not WS:CODE, WS:TASK or owner/repo",
            clean(raw)
        ))
        .hint("for example tmgrdev:TM or tmgrdev:TM-12")),
    }
}

struct Resolved {
    entry: Entry,
    category: Category,
}

fn identity(ctx: &Ctx, client: &Client) -> Result<(String, i64)> {
    let host = host_of(&ctx.server()?)?;
    let user = client
        .current_user_id()?
        .ok_or_else(|| Error::auth("cannot tell which account is logged in").hint("tmgr login"))?;
    Ok((host, user))
}

fn task_category(client: &Client, ws_id: i64, task_id: i64) -> Result<Category> {
    let elig = parse_eligibility(client.agent_run_task(task_id)?)?;
    if elig.task_id != task_id || elig.workspace_id != ws_id {
        return Err(Error::new(
            Kind::Forbidden,
            "the server answered for another task or workspace",
        ));
    }
    elig.category.ok_or_else(|| {
        Error::new(
            Kind::Forbidden,
            "the task has no category, so no repository is known",
        )
    })
}

pub(crate) fn category_by_code(
    client: &Client,
    ws_id: i64,
    ws_code: &str,
    code: &str,
) -> Result<Category> {
    let found = client
        .categories(ws_id)?
        .into_iter()
        .find(|c| {
            c.code
                .as_deref()
                .is_some_and(|k| k.eq_ignore_ascii_case(code))
        })
        .ok_or_else(|| {
            Error::not_found(format!(
                "no category {} in workspace {}",
                clean(code),
                clean(ws_code)
            ))
        })?;
    let first = client
        .tasks_page(ws_id, Some(found.id), None, 1, 1)?
        .items
        .into_iter()
        .next()
        .ok_or_else(|| {
            Error::new(
                Kind::Conflict,
                format!(
                    "category {} has no tasks to read its repository from",
                    clean(code)
                ),
            )
            .hint(format!(
                "pass a task of the category: tmgr runner bind {}:<task> <path>",
                clean(ws_code)
            ))
        })?;
    let task_id = i64::try_from(first.id).map_err(|_| Error::usage("invalid task id"))?;
    let category = task_category(client, ws_id, task_id)?;
    if u64::try_from(category.id).ok() != Some(found.id) {
        return Err(Error::new(
            Kind::Forbidden,
            "the server answered with another category than the one asked for",
        ));
    }
    Ok(category)
}

fn resolve(ctx: &Ctx, raw: &str, live: bool) -> Result<Resolved> {
    let client = ctx.cloud_client()?;
    let (host, user) = identity(ctx, client)?;
    let allow =
        AllowFile::load().map_err(|e| Error::internal(format!("runner allow list: {e}")))?;
    match parse_target(raw)? {
        Target::Category { ws, rest } => {
            let w = ctx
                .workspaces(client)?
                .into_iter()
                .find(|w| w.code.eq_ignore_ascii_case(&ws) && !w.local)
                .ok_or_else(|| {
                    Error::not_found(format!("workspace {} not found", clean(&ws)))
                        .hint("tmgr ws list")
                })?;
            let entry = if live {
                allow
                    .find_live(&host, user, w.id, &w.code, Utc::now())
                    .map_err(|r| Error::new(Kind::Forbidden, clean(&r.message)).hint(r.fix))?
            } else {
                allow.find(&host, user, w.id).ok_or_else(|| {
                    Error::not_found(format!(
                        "workspace {} is not allowed to run agents here",
                        clean(&w.code)
                    ))
                    .hint(format!("tmgr runner allow {}", clean(&w.code)))
                })?
            }
            .clone();
            let category = match rest {
                Rest::Category(code) => category_by_code(client, w.id, &w.code, &code)?,
                Rest::Task(r) => {
                    let task = match r {
                        TaskRef::Key(key) => client.task_by_key(&key, w.id)?,
                        TaskRef::Id(id) => client.task_by_id(id)?,
                    };
                    let id = i64::try_from(task.id).map_err(|_| Error::usage("invalid task id"))?;
                    task_category(client, w.id, id)?
                }
            };
            Ok(Resolved { entry, category })
        }
        Target::Repo(name) => {
            let mut found: Vec<(&Entry, &String)> = allow
                .entries
                .iter()
                .filter(|e| e.server == host && e.user_id == user)
                .filter(|e| !live || !e.expired(Utc::now()))
                .flat_map(|e| {
                    let name = &name;
                    e.repos.iter().filter_map(move |(k, b)| match b {
                        RepoBinding::Set(s) if s.contains_key(name) => Some((e, k)),
                        _ => None,
                    })
                })
                .collect();
            if found.len() > 1 {
                return Err(Error::usage(format!(
                    "{} is part of more than one bound set of repositories",
                    clean(&name)
                ))
                .hint("name the category: WS:CODE"));
            }
            let (entry, key) = found
                .pop()
                .map(|(e, k)| (e.clone(), k.clone()))
                .ok_or_else(|| {
                    Error::not_found(format!(
                        "{} is not part of a bound set of repositories",
                        clean(&name)
                    ))
                    .hint("use WS:CODE to bind a category the first time")
                })?;
            let id: i64 = key
                .parse()
                .map_err(|_| Error::internal("a category key in the allow list is not a number"))?;
            let list: Vec<String> = entry
                .repo_set(id)
                .map(|s| s.keys().cloned().collect())
                .unwrap_or_default();
            let code = client
                .categories(entry.workspace_id)?
                .into_iter()
                .find(|c| u64::try_from(id).ok() == Some(c.id))
                .and_then(|c| c.code)
                .ok_or_else(|| Error::not_found("the category no longer exists"))?;
            let category = Category {
                id,
                code,
                github_full_name: None,
                default_branch: None,
                repositories: Some(list),
                repos_sha256: None,
            };
            Ok(Resolved { entry, category })
        }
    }
}

fn bound_json(category: &Category, full_name: &str, path: &str) -> Value {
    json!({
        "category": category.code,
        "full_name": full_name,
        "path": path,
    })
}

pub fn bind(o: &Opts, target: &str, path: &Path) -> Result<i32> {
    privileged::guard()?;
    let ctx = Ctx::new(o.cli)?;
    let Resolved { entry, category } = resolve(&ctx, target, true)?;
    let mut allow =
        AllowFile::load().map_err(|e| Error::internal(format!("runner allow list: {e}")))?;
    let hint = format!(
        "tmgr runner bind {}:{}",
        clean(&entry.workspace_code),
        clean(&category.code)
    );
    let bound: Vec<Value> = match &category.repositories {
        Some(list) => {
            let root = canonical_root(path)?;
            let name = list
                .iter()
                .find(|n| verify_repo(&root, n).is_ok())
                .ok_or_else(|| {
                    Error::new(
                        Kind::Forbidden,
                        format!(
                            "the origin of {} matches none of the repositories of category {}",
                            clean(&root.display().to_string()),
                            clean(&category.code)
                        ),
                    )
                    .hint(format!("they are {}", clean(&list.join(", "))))
                })?;
            bound_set_with(
                true,
                &mut allow,
                &entry,
                &category,
                list,
                &hint,
                &[(name, &root)],
            )?
            .iter()
            .map(|r| bound_json(&category, &r.full_name, &r.path))
            .collect()
        }
        None => {
            let full_name = category.github_full_name.as_deref().ok_or_else(|| {
                Error::new(
                    Kind::Forbidden,
                    format!(
                        "category {} has no linked GitHub repository",
                        clean(&category.code)
                    ),
                )
                .hint(format!(
                    "link a GitHub repository to category {}",
                    clean(&category.code)
                ))
            })?;
            if entry.repo_set(category.id).is_some() {
                confirm_replace(
                    &entry.workspace_code,
                    &category,
                    "is bound to a set of repositories, but the server reports one repository now; replace it with that one",
                )?;
            }
            let repo = bind_checked(&mut allow, &entry, &category, full_name, path, true)?;
            vec![bound_json(
                &category,
                full_name,
                &repo.display().to_string(),
            )]
        }
    };
    let human = bound
        .iter()
        .map(|b| {
            format!(
                "bound {} to category {}",
                clean(b["path"].as_str().unwrap_or_default()),
                clean(&category.code)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(emit(o, json!({ "bound": bound }), human))
}

pub fn unbind(o: &Opts, target: &str) -> Result<i32> {
    privileged::guard()?;
    if matches!(parse_target(target)?, Target::Repo(_)) {
        return Err(Error::usage(
            "unbind takes WS:CODE or WS:TASK, not a repository",
        ));
    }
    let ctx = Ctx::new(o.cli)?;
    let Resolved { entry, category } = resolve(&ctx, target, false)?;
    if !entry.repos.contains_key(&category.id.to_string()) {
        return Err(Error::not_found(format!(
            "category {} is not bound on this machine",
            clean(&category.code)
        )));
    }
    interact::confirm(
        &format!(
            "Unbind category {}? Agents of its tasks will not start here until it is bound again.",
            clean(&category.code)
        ),
        false,
        false,
    )?;
    let (server, user, ws_id) = (entry.server.clone(), entry.user_id, entry.workspace_id);
    let key = category.id.to_string();
    AllowFile::update(|file| {
        let mut updated = file
            .find(&server, user, ws_id)
            .cloned()
            .ok_or("the allow entry disappeared")?;
        updated.repos.remove(&key);
        file.upsert(updated);
        Ok(())
    })
    .map_err(|e| Error::internal(format!("runner allow list: {e}")))?;
    let human = format!("category {} is no longer bound", clean(&category.code));
    Ok(emit(o, json!({ "unbound": category.code }), human))
}

pub fn list(o: &Opts) -> Result<i32> {
    let file = AllowFile::load().map_err(|e| Error::internal(format!("runner allow list: {e}")))?;
    let mut rows: Vec<Value> = Vec::new();
    for e in &file.entries {
        for (key, binding) in &e.repos {
            let category_id = key.parse::<i64>().ok();
            match binding {
                RepoBinding::Path(p) => rows.push(json!({
                    "workspace": e.workspace_code, "category_id": category_id, "path": p,
                })),
                RepoBinding::Set(s) => {
                    for (name, p) in s {
                        rows.push(json!({
                            "workspace": e.workspace_code, "category_id": category_id,
                            "full_name": name, "path": p,
                        }));
                    }
                }
            }
        }
    }
    let human = if rows.is_empty() {
        "no repository is bound".to_string()
    } else {
        let mut t = vec![vec![
            "WORKSPACE".into(),
            "CATEGORY".into(),
            "REPOSITORY".into(),
            "PATH".into(),
        ]];
        for r in &rows {
            let text = |k: &str| clean(&r[k].as_str().map(str::to_string).unwrap_or_default());
            t.push(vec![
                text("workspace"),
                r["category_id"].to_string(),
                text("full_name"),
                text("path"),
            ]);
        }
        table(&t)
    };
    Ok(emit(o, json!({ "bindings": rows }), human))
}

pub(crate) fn offer_for_rule(ctx: &Ctx, rule: &Rule) -> Result<()> {
    if !tmgr_queue::rules::is_cloud(rule) {
        return Ok(());
    }
    let ws = &rule.workspace;
    let Some(project) = rule.project.as_deref().filter(|_| rule.auto) else {
        return Ok(());
    };
    let cloud = CliCloud::new(ctx);
    let Ok(ws_info) = cloud.allow(ws, Utc::now()) else {
        return Ok(());
    };
    let unbound: Vec<&String> = rule
        .repos
        .iter()
        .filter(|r| !ws_info.repos.contains(r))
        .collect();
    if unbound.is_empty() {
        return Ok(());
    }
    let (Ok((host, user)), Ok(client)) = (cloud.identity(), ctx.cloud_client()) else {
        return Ok(());
    };
    let Ok(category) = category_by_code(client, ws_info.id, &ws_info.code, project) else {
        return Ok(());
    };
    let mut allow =
        AllowFile::load().map_err(|e| Error::internal(format!("runner allow list: {e}")))?;
    let Ok(entry) = allow
        .find_live(&host, user, ws_info.id, &ws_info.code, Utc::now())
        .cloned()
    else {
        return Ok(());
    };
    let hint = format!(
        "tmgr runner bind {}:{}",
        clean(&ws_info.code),
        clean(&category.code)
    );
    match &category.repositories {
        Some(list) => {
            let mut roots = Vec::new();
            for path in &unbound {
                let Ok(root) = canonical_root(Path::new(path)) else {
                    return Ok(());
                };
                roots.push(root);
            }
            let mut given: Vec<(&str, &Path)> = Vec::new();
            for root in &roots {
                let name = list
                    .iter()
                    .find(|n| verify_repo(root, n).is_ok())
                    .ok_or_else(|| {
                        Error::new(
                            Kind::Forbidden,
                            format!(
                                "the origin of {} matches none of the repositories of category {}",
                                clean(&root.display().to_string()),
                                clean(&category.code)
                            ),
                        )
                    })?;
                if given.iter().any(|(n, _)| n == name) {
                    return Err(Error::usage(format!(
                        "two folders are clones of {}; give one",
                        clean(name)
                    )));
                }
                given.push((name, root));
            }
            bound_set_with(true, &mut allow, &entry, &category, list, &hint, &given)?;
        }
        None => {
            if unbound.len() > 1 {
                return Err(Error::usage(format!(
                    "category {} has one repository, but {} unbound folders were given",
                    clean(&category.code),
                    unbound.len()
                )));
            }
            let full_name = category.github_full_name.as_deref().ok_or_else(|| {
                Error::new(
                    Kind::Forbidden,
                    format!(
                        "category {} has no linked GitHub repository",
                        clean(&category.code)
                    ),
                )
            })?;
            if entry.repo_set(category.id).is_some() {
                confirm_replace(
                    &entry.workspace_code,
                    &category,
                    "is bound to a set of repositories, but the server reports one repository now; replace it with that one",
                )?;
            }
            bind_checked(
                &mut allow,
                &entry,
                &category,
                full_name,
                Path::new(unbound[0]),
                true,
            )?;
        }
    }
    Ok(())
}
