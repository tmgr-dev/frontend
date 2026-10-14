use crate::cli_agent::{AgentStatusClearArgs, AgentStatusCommand, AgentStatusSetArgs};
use crate::ctx::Ctx;
use crate::out::{table, Output};
use serde_json::json;
use tmgr_core::agent_status::{self as status, SessionView};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::refs::parse_task_ref;
use tmgr_core::text::sanitize_line;

pub fn run(ctx: &Ctx, cmd: &AgentStatusCommand) -> Result<Output> {
    match cmd {
        AgentStatusCommand::Set(a) => set(ctx, a),
        AgentStatusCommand::Clear(a) => clear(ctx, a),
        AgentStatusCommand::List => list(ctx),
    }
}

fn require_upload(ctx: &Ctx) -> Result<()> {
    if ctx.cfg.upload_status() {
        return Ok(());
    }
    Err(
        Error::new(Kind::Forbidden, "status upload is off, nothing was sent")
            .hint("tmgr config set upload_status true"),
    )
}

fn disabled(ctx: &Ctx, session_id: Option<&str>) -> Output {
    ctx.warn(
        "agent_work_disabled",
        "Agent work is turned off in this workspace; nothing was recorded",
    );
    Output::new(
        json!({"session": null, "disabled": true}),
        ctx.style.muted(&format!(
            "agent work is disabled in this workspace{}",
            session_id.map(|s| format!(" ({s})")).unwrap_or_default()
        )),
    )
}

fn task_label(s: &SessionView) -> String {
    s.task
        .as_ref()
        .and_then(|t| t.key.clone().or_else(|| t.id.map(|i| format!("#{i}"))))
        .unwrap_or_else(|| "-".into())
}

fn set(ctx: &Ctx, a: &AgentStatusSetArgs) -> Result<Output> {
    let session_id = status::validate_session_id(&a.session_id)?;
    let state = status::validate_state(&a.state)?;
    let profile = status::validate_profile(&a.profile)?;
    let task = a.task.as_deref().map(parse_task_ref).transpose()?;
    let since = a
        .since
        .as_deref()
        .map(|v| status::normalize_instant(v, "--since"))
        .transpose()?;
    let started = a
        .started
        .as_deref()
        .map(|v| status::normalize_instant(v, "--started"))
        .transpose()?;
    require_upload(ctx)?;
    let client = ctx.client()?;
    let ws = ctx.workspace(client)?;
    let body = status::PutBody {
        task: task.as_ref(),
        workspace_id: ws.id,
        agent_profile: profile,
        state,
        started_at: started.as_deref(),
        state_since: since.as_deref(),
    };
    match status::put(client, session_id, &body)? {
        None => Ok(disabled(ctx, Some(session_id))),
        Some(s) => {
            let human = format!(
                "{} session {} {} ({})",
                ctx.style.good("\u{2713}"),
                s.session_id,
                s.state,
                task_label(&s)
            );
            Ok(Output::new(json!({"session": s, "disabled": false}), human))
        }
    }
}

fn clear(ctx: &Ctx, a: &AgentStatusClearArgs) -> Result<Output> {
    let session_id = status::validate_session_id(&a.session_id)?;
    require_upload(ctx)?;
    let client = ctx.client()?;
    match status::clear(client, session_id)? {
        None => Ok(disabled(ctx, Some(session_id))),
        Some(s) => {
            let human = format!(
                "{} session {} ended",
                ctx.style.good("\u{2713}"),
                s.session_id
            );
            Ok(Output::new(json!({"session": s, "disabled": false}), human))
        }
    }
}

fn list(ctx: &Ctx) -> Result<Output> {
    let client = ctx.client()?;
    let ws = ctx.workspace(client)?;
    let code = sanitize_line(&ws.code);
    let Some(sessions) = status::list(client, ws.id)? else {
        ctx.warn(
            "agent_work_disabled",
            "Agent work is turned off in this workspace",
        );
        return Ok(Output::new(
            json!({"workspace": code, "sessions": [], "disabled": true}),
            ctx.style
                .muted("Agent work is turned off in this workspace."),
        ));
    };
    let human = if sessions.is_empty() {
        ctx.style.muted("No agent sessions reported.")
    } else {
        let mut rows = vec![[
            "TASK", "STATE", "SINCE", "PROFILE", "USER", "HOST", "SESSION",
        ]
        .map(String::from)
        .to_vec()];
        for s in &sessions {
            let state = if s.stale {
                format!("{} (stale)", s.state)
            } else {
                s.state.clone()
            };
            rows.push(vec![
                task_label(s),
                state,
                s.state_since.clone().unwrap_or_default(),
                s.agent_profile.clone().unwrap_or_default(),
                s.user
                    .as_ref()
                    .and_then(|u| u.name.clone())
                    .unwrap_or_default(),
                s.host_label.clone().unwrap_or_else(|| "-".into()),
                s.session_id.clone(),
            ]);
        }
        table(&rows)
    };
    Ok(Output::new(
        json!({"workspace": code, "sessions": sessions, "disabled": false}),
        human,
    ))
}
