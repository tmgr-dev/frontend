use chrono::{DateTime, Utc};
use tmgr_approval::{AllowFile, Entry, Rollback};
use tmgr_ipc::api::{RunCreateReq, TaskRef};

pub fn check(req: &RunCreateReq, task: TaskRef) -> Result<(), String> {
    check_primary(req, task).map(|_| ())
}

pub fn check_primary(req: &RunCreateReq, task: TaskRef) -> Result<Option<String>, String> {
    verify_state(req, task, AllowFile::load(), Utc::now(), Rollback::load())
}

/// The folder name of each repository of a set: its lowercase repo part, or `<owner>-<repo>` when two share it.
pub fn clone_names(full_names: &[String]) -> Result<Vec<String>, String> {
    let repo_of = |n: &String| n.split_once('/').map_or("", |(_, r)| r).to_string();
    let names: Vec<String> = full_names
        .iter()
        .map(|n| {
            let shared = full_names
                .iter()
                .filter(|o| repo_of(o) == repo_of(n))
                .count()
                > 1;
            if shared {
                n.replace('/', "-")
            } else {
                repo_of(n)
            }
        })
        .collect();
    let ok = |n: &String| {
        (1..=100).contains(&n.len())
            && n.starts_with(|c: char| c.is_ascii_lowercase() || c.is_ascii_digit())
            && n != ".git"
    };
    let mut sorted = names.clone();
    sorted.sort();
    if !names.iter().all(ok) || sorted.windows(2).any(|w| w[0] == w[1]) {
        return Err(
            "the repositories of this category cannot be given distinct folder names".into(),
        );
    }
    Ok(names)
}

fn check_set(
    req: &RunCreateReq,
    cloud: &tmgr_ipc::api::CloudApproval,
    entry: &Entry,
) -> Result<Option<String>, String> {
    let refused = |m: &str| Err(format!("agent_work: {m} ({})", entry.allow_command()));
    match (cloud.repos_sha256.as_deref(), req.clones.is_empty()) {
        (None, true) => return Ok(None),
        (None, false) => return refused("clones need an approval of a repository set"),
        (Some(_), true) => return refused("an approval of a repository set needs its clones"),
        _ => {}
    }
    if req.clone_of.is_some() {
        return refused("a multi-repository run cannot also name clone_of");
    }
    let Some(set) = entry.repo_set(cloud.category_id) else {
        return refused("no repository set is bound to this category");
    };
    let names: Vec<String> = set.keys().cloned().collect();
    let canonical =
        tmgr_approval::canonical_repos(&names).map_err(|e| format!("agent_work: {e}"))?;
    let hash = tmgr_approval::repos_sha256(&canonical).map_err(|e| format!("agent_work: {e}"))?;
    if Some(&hash) != cloud.repos_sha256.as_ref() {
        return refused("the approval is for another set of repositories");
    }
    let folders = clone_names(&canonical).map_err(|e| format!("agent_work: {e}"))?;
    let mut want: Vec<(String, String)> = canonical
        .iter()
        .zip(folders)
        .map(|(full, folder)| (folder, set[full].clone()))
        .collect();
    let mut got: Vec<(String, String)> = req
        .clones
        .iter()
        .map(|c| (c.name.clone(), c.clone_of.clone()))
        .collect();
    let primary = want[0].1.clone();
    want.sort();
    got.sort();
    if want != got {
        return refused(
            "the run is not a set of clones of the repositories bound to this category",
        );
    }
    Ok(Some(primary))
}

pub fn check_with(
    req: &RunCreateReq,
    task: TaskRef,
    allow: Result<AllowFile, String>,
    now: DateTime<Utc>,
    rollback: Option<Rollback>,
) -> Result<(), String> {
    verify(req, task, allow, now, rollback).map(|_| ())
}

pub fn check_with_state(
    req: &RunCreateReq,
    task: TaskRef,
    allow: Result<AllowFile, String>,
    now: DateTime<Utc>,
    rollback: Result<Rollback, String>,
) -> Result<(), String> {
    verify_state(req, task, allow, now, rollback).map(|_| ())
}

fn verify_state(
    req: &RunCreateReq,
    task: TaskRef,
    allow: Result<AllowFile, String>,
    now: DateTime<Utc>,
    rollback: Result<Rollback, String>,
) -> Result<Option<String>, String> {
    let rollback =
        rollback.map_err(|e| format!("agent_work: cloud approvals state is unreadable: {e}"))?;
    verify(req, task, allow, now, Some(rollback))
}

fn verify(
    req: &RunCreateReq,
    task: TaskRef,
    allow: Result<AllowFile, String>,
    now: DateTime<Utc>,
    rollback: Option<Rollback>,
) -> Result<Option<String>, String> {
    let cloud = req.cloud.as_ref().ok_or(
        "agent_work: a task of a cloud workspace needs a signed approval; start it with tmgr run",
    )?;
    if cloud.workspace_id != task.workspace_id || cloud.task_id != task.task_id {
        return Err("agent_work: the approval is for another task".into());
    }
    let bundle = req
        .bundle
        .as_ref()
        .ok_or("agent_work: a cloud task run needs its bundle")?;
    if !bundle.comments.is_empty() || !bundle.pages.is_empty() || bundle.persona.is_some() {
        return Err("agent_work: a cloud task run carries only the title and description".into());
    }
    let allow = allow?;
    let entry = allow
        .find_live(
            &cloud.host,
            cloud.user_id,
            cloud.workspace_id,
            "<workspace>",
            now,
        )
        .map_err(|r| r.to_string())?;
    tmgr_approval::check_claim(entry, cloud, &bundle.title, &bundle.body)
        .map_err(|r| r.to_string())?;
    if let Some(seen) = rollback.as_ref().and_then(|r| {
        r.seen(&Rollback::key(
            &cloud.host,
            cloud.user_id,
            cloud.workspace_id,
            cloud.task_id,
        ))
    }) {
        let (at, hash) = seen;
        if cloud.signed_at < at
            || (cloud.signed_at == at && hash.is_some_and(|h| h != Rollback::stamp(cloud)))
        {
            return Err("agent_work: an older approved version of this task was served".into());
        }
    }
    if cloud.repos_sha256.is_some() || !req.clones.is_empty() {
        return check_set(req, cloud, entry);
    }
    match (entry.repo(cloud.category_id), req.clone_of.as_deref()) {
        (Some(bound), Some(of)) if bound == of => Ok(None),
        _ => Err(format!(
            "agent_work: the run is not a clone of the repository bound to this category ({})",
            entry.allow_command()
        )),
    }
}
