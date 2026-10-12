use crate::ctx::Ctx;
use crate::out::Output;
use serde_json::json;
use tmgr_core::error::Result;
use tmgr_core::text::{format_duration, sanitize_line};

pub fn run(ctx: &Ctx) -> Result<Output> {
    let client = ctx.client()?;
    let ws = ctx.workspace(client)?;
    let today = chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string();
    let total = ctx.soft(
        client.time_summary_total(ws.id, &today, &today),
        "time_summary_unavailable",
        "today's time",
    )?;
    ctx.warn(
        "active_unavailable",
        "the running task is not available with this login yet",
    );
    let s = &ctx.style;
    let mut lines = vec![format!(
        "{}  {} {}",
        s.bold("TMGR"),
        s.muted("workspace"),
        sanitize_line(&ws.name)
    )];
    lines.push(String::new());
    lines.push(format!(
        "{}  {}",
        s.muted("ACTIVE"),
        s.muted("no active task")
    ));
    if let Some(t) = total {
        lines.push(format!(
            "{}   {} tracked",
            s.muted("TODAY"),
            s.bold(&format_duration(t))
        ));
    }
    lines.push(String::new());
    lines.push(s.muted("hint: tmgr task list \u{b7} tmgr add \"title\" \u{b7} tmgr ws list"));
    let data = json!({
        "workspace": {"id": ws.id, "code": sanitize_line(&ws.code), "name": sanitize_line(&ws.name)},
        "active": null,
        "git": null,
        "today": total.map(|t| json!({"total_s": t, "human_s": null, "agent_s": null})),
        "agents": null,
    });
    Ok(Output::new(data, lines.join("\n")))
}
