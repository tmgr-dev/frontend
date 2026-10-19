use chrono::{DateTime, Utc};
use tmgr_approval::AllowFile;
use tmgr_ipc::api::{RunCreateReq, TaskRef};

pub fn check(req: &RunCreateReq, task: TaskRef) -> Result<(), String> {
    check_with(req, task, AllowFile::load(), Utc::now())
}

pub fn check_with(
    req: &RunCreateReq,
    task: TaskRef,
    allow: Result<AllowFile, String>,
    now: DateTime<Utc>,
) -> Result<(), String> {
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
    match (entry.repo(cloud.category_id), req.clone_of.as_deref()) {
        (Some(bound), Some(of)) if bound == of => Ok(()),
        _ => Err(format!(
            "agent_work: the run is not a clone of the repository bound to this category ({})",
            entry.allow_command()
        )),
    }
}
