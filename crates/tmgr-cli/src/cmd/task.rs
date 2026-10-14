use crate::cli::TaskCommand;
use crate::ctx::Ctx;
use crate::out::{table, Output};
use chrono::DateTime;
use serde_json::{json, Value};
use std::collections::HashSet;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::models::{ApiTask, Status};
use tmgr_core::refs::{is_category_code, parse_task_ref, TaskRef};
use tmgr_core::text::{
    format_duration, markdown_to_text, parse_size, sanitize, sanitize_line, truncate_bytes,
};
use tmgr_core::view::{display_title, task_ref, CategoryView, CommentView, PersonView, StatusView};

const MAX_COMMENTS: usize = 50;
const MAX_PAGES_PER_CALL: u32 = 20;
const STATUS_TYPES: [&str; 5] = ["default", "active", "completed", "hidden", "archived"];

pub fn run(ctx: &Ctx, cmd: &TaskCommand) -> Result<Output> {
    match cmd {
        TaskCommand::Show {
            reference,
            comments,
            max_desc,
        } => show(ctx, reference, *comments, max_desc),
        TaskCommand::Confirm { reference } => confirm(ctx, reference),
        TaskCommand::List {
            project,
            status_type,
            limit,
            cursor,
        } => list(
            ctx,
            project.as_deref(),
            status_type.as_deref(),
            *limit,
            cursor.as_deref(),
        ),
    }
}

fn workspace_code(ctx: &Ctx, workspace_id: Option<i64>) -> Option<String> {
    let client = ctx.client().ok()?;
    let list = ctx.workspaces(client).ok()?;
    list.iter()
        .find(|w| Some(w.id) == workspace_id)
        .map(|w| sanitize_line(&w.code))
}

fn show(ctx: &Ctx, reference: &str, comments_n: usize, max_desc: &str) -> Result<Output> {
    let tref = parse_task_ref(reference)?;
    let max = parse_size(max_desc)
        .filter(|n| (1..=1 << 20).contains(n))
        .ok_or_else(|| Error::usage("invalid --max-desc").hint("example: --max-desc 64k"))?;
    let comments_n = comments_n.min(MAX_COMMENTS);
    let client = ctx.client()?;
    let task = match &tref {
        TaskRef::Key(key) => super::shared::task_by_key(ctx, client, key)
            .map_err(|e| not_found_for(e, reference, "check --ws, or tmgr task list -p CODE"))?,
        TaskRef::Id(id) => client
            .task_by_id(*id)
            .map_err(|e| not_found_for(e, reference, "tmgr task list"))?,
    };
    let ws_code = workspace_code(ctx, task.workspace_id);
    let url = match ws_code.as_deref() {
        Some(c) => ctx.task_link(c, task.id)?,
        None => None,
    };
    let statuses = match task.workspace_id {
        Some(w) => ctx
            .soft(client.statuses(w), "status_unresolved", "status names")?
            .unwrap_or_default(),
        None => Vec::new(),
    };
    let status = StatusView::resolve(&task, &statuses);
    let (all_comments, comments_total) = if comments_n > 0 {
        match ctx.soft(client.comments(task.id), "comments_unavailable", "comments")? {
            Some(mut list) => {
                let total = list.len();
                let keep = list.split_off(total.saturating_sub(comments_n));
                (
                    keep.iter().map(CommentView::from_api).collect(),
                    Some(total),
                )
            }
            None => (Vec::new(), None),
        }
    } else {
        (Vec::new(), None)
    };
    let (description, truncated) = match task.description.as_deref() {
        Some(d) if !d.trim().is_empty() => {
            let (t, cut) = truncate_bytes(d, max);
            (Some(t), cut)
        }
        _ => (None, false),
    };
    let category = task.category.as_ref().map(|c| CategoryView {
        id: Some(c.id),
        code: c.code.as_ref().map(|s| sanitize_line(s)),
        title: sanitize_line(&c.title),
    });
    let assignees: Vec<PersonView> = task
        .assignees
        .iter()
        .map(|p| PersonView {
            id: p.id,
            name: sanitize_line(p.name.as_deref().unwrap_or("unknown")),
        })
        .collect();
    let tref_str = task_ref(&task);
    let title = display_title(&task.title);
    let data = json!({
        "task": {
            "id": task.id,
            "ref": tref_str,
            "title": title,
            "url": url,
            "status": status,
            "category": category,
            "priority": task.priority,
            "assignees": assignees,
            "description": description,
            "description_truncated": truncated,
            "estimate_s": task.approximately_time,
            "tracked_s": task.common_time,
            "created_at": task.created_at,
            "updated_at": task.updated_at,
        },
        "comments": all_comments,
        "comments_total": comments_total,
    });
    let s = &ctx.style;
    let mut lines = vec![format!("{}  {}", s.accent(&tref_str), s.bold(&title))];
    lines.push(format!(
        "{}  {} ({})",
        s.muted("status   "),
        status.name,
        status.kind
    ));
    if let Some(c) = &category {
        lines.push(format!(
            "{}  {}",
            s.muted("category "),
            match &c.code {
                Some(code) => format!("{code} - {}", c.title),
                None => c.title.clone(),
            }
        ));
    }
    if !assignees.is_empty() {
        lines.push(format!(
            "{}  {}",
            s.muted("assignees"),
            assignees
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if task.common_time.unwrap_or(0) > 0 || task.approximately_time.unwrap_or(0) > 0 {
        lines.push(format!(
            "{}  {} tracked, {} estimated",
            s.muted("time     "),
            format_duration(task.common_time.unwrap_or(0)),
            format_duration(task.approximately_time.unwrap_or(0))
        ));
    }
    if let Some(u) = &url {
        lines.push(format!("{}  {}", s.muted("url      "), u));
    }
    if let Some(d) = &description {
        lines.push(String::new());
        lines.push(markdown_to_text(d));
        if truncated {
            lines.push(s.muted("... description truncated (use --max-desc)"));
        }
    }
    if !data["comments"].as_array().is_none_or(Vec::is_empty) {
        lines.push(String::new());
        lines.push(s.muted(&format!(
            "Comments (last {} of {})",
            all_comments_len(&data),
            comments_total.unwrap_or(0)
        )));
        for c in data["comments"].as_array().into_iter().flatten() {
            let who = c["author"]["name"].as_str().unwrap_or("unknown");
            let when = c["created_at"].as_str().map(short_time).unwrap_or_default();
            lines.push(format!("  {} {}", s.bold(who), s.muted(&when)));
            for l in sanitize(c["message"].as_str().unwrap_or("")).lines() {
                lines.push(format!("    {l}"));
            }
        }
    }
    Ok(Output::new(data, lines.join("\n")))
}

fn all_comments_len(data: &Value) -> usize {
    data["comments"].as_array().map_or(0, Vec::len)
}

fn short_time(raw: &str) -> String {
    DateTime::parse_from_rfc3339(raw)
        .map(|d| d.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|_| sanitize_line(raw))
}

fn not_found_for(e: Error, reference: &str, hint: &str) -> Error {
    if e.kind == Kind::NotFound {
        Error::not_found(format!("Task {} not found", sanitize_line(reference))).hint(hint)
    } else {
        e
    }
}

struct Cursor {
    page: u64,
    skip: usize,
}

fn parse_cursor(raw: Option<&str>) -> Result<Cursor> {
    let Some(raw) = raw else {
        return Ok(Cursor { page: 1, skip: 0 });
    };
    let bad = || Error::usage("invalid --cursor").hint("pass next_cursor from the previous page");
    let (p, s) = match raw.split_once('.') {
        Some((p, s)) => (p, s),
        None => (raw, "0"),
    };
    let ok = |v: &str| (1..=6).contains(&v.len()) && v.bytes().all(|b| b.is_ascii_digit());
    if !ok(p) || !ok(s) {
        return Err(bad());
    }
    let page: u64 = p.parse().map_err(|_| bad())?;
    let skip: usize = s.parse().map_err(|_| bad())?;
    if page == 0 {
        return Err(bad());
    }
    Ok(Cursor { page, skip })
}

fn encode_cursor(page: u64, skip: usize) -> String {
    if skip == 0 {
        page.to_string()
    } else {
        format!("{page}.{skip}")
    }
}

fn list(
    ctx: &Ctx,
    project: Option<&str>,
    status_type: Option<&str>,
    limit: usize,
    cursor: Option<&str>,
) -> Result<Output> {
    if !(1..=200).contains(&limit) {
        return Err(Error::usage("--limit must be between 1 and 200"));
    }
    if let Some(t) = status_type {
        if !STATUS_TYPES.contains(&t) {
            return Err(Error::usage("invalid --status-type")
                .hint(format!("one of: {}", STATUS_TYPES.join(", "))));
        }
    }
    let project = project.map(str::to_ascii_uppercase);
    if let Some(p) = &project {
        if !is_category_code(p) {
            return Err(Error::usage("invalid project code"));
        }
    }
    let cur = parse_cursor(cursor)?;
    let client = ctx.client()?;
    let ws = ctx.workspace(client)?;
    let category_id = match &project {
        Some(code) => Some(
            client
                .categories(ws.id)?
                .into_iter()
                .find(|c| {
                    c.code
                        .as_deref()
                        .is_some_and(|x| x.eq_ignore_ascii_case(code))
                })
                .map(|c| c.id)
                .ok_or_else(|| {
                    Error::not_found(format!("project {code} not found")).hint("tmgr ws list")
                })?,
        ),
        None => None,
    };
    let statuses: Vec<Status> = if status_type.is_some() {
        client.statuses(ws.id)?
    } else {
        ctx.soft(client.statuses(ws.id), "status_unresolved", "status names")?
            .unwrap_or_default()
    };
    let mut server_status = None;
    let mut filter: Option<HashSet<u64>> = None;
    if let Some(t) = status_type {
        let ids: Vec<u64> = statuses
            .iter()
            .filter(|s| s.kind == t)
            .map(|s| s.id)
            .collect();
        match ids.len() {
            0 => return Ok(render_list(ctx, &ws.code, Vec::new(), None, &statuses, &[])),
            1 => server_status = Some(ids[0]),
            _ => filter = Some(ids.into_iter().collect()),
        }
    }
    let per_page = limit.min(100) as u64;
    let (mut page, mut skip) = (cur.page, cur.skip);
    let mut items: Vec<ApiTask> = Vec::new();
    let mut next: Option<(u64, usize)> = None;
    let mut fetched = 0;
    'pages: loop {
        let res = client.tasks_page(ws.id, category_id, server_status, page, per_page)?;
        fetched += 1;
        let has_next = res.current_page < res.last_page;
        let n = res.items.len();
        for (i, t) in res.items.into_iter().enumerate().skip(skip) {
            if let Some(f) = &filter {
                if !t.status_id.is_some_and(|id| f.contains(&id)) {
                    continue;
                }
            }
            items.push(t);
            if items.len() == limit {
                next = if i + 1 < n {
                    Some((page, i + 1))
                } else if has_next {
                    Some((page + 1, 0))
                } else {
                    None
                };
                break 'pages;
            }
        }
        if !has_next {
            break;
        }
        page += 1;
        skip = 0;
        if fetched >= MAX_PAGES_PER_CALL {
            next = Some((page, 0));
            break;
        }
    }
    let urls: Vec<Option<String>> = items
        .iter()
        .map(|t| ctx.task_link(&sanitize_line(&ws.code), t.id))
        .collect::<Result<_>>()?;
    Ok(render_list(
        ctx,
        &ws.code,
        items,
        next.map(|(p, s)| encode_cursor(p, s)),
        &statuses,
        &urls,
    ))
}

fn render_list(
    ctx: &Ctx,
    ws_code: &str,
    items: Vec<ApiTask>,
    next_cursor: Option<String>,
    statuses: &[Status],
    urls: &[Option<String>],
) -> Output {
    let views: Vec<Value> = items
        .iter()
        .enumerate()
        .map(|(i, t)| {
            json!({
                "id": t.id,
                "ref": task_ref(t),
                "title": display_title(&t.title),
                "status": StatusView::resolve(t, statuses),
                "project": t.category.as_ref().and_then(|c| c.code.as_ref()).map(|c| sanitize_line(c)),
                "url": urls.get(i),
                "updated_at": t.updated_at,
            })
        })
        .collect();
    let s = &ctx.style;
    let human = if views.is_empty() {
        s.muted("No tasks found. Add one: tmgr add \"title\"")
    } else {
        let rows: Vec<Vec<String>> = views
            .iter()
            .map(|v| {
                vec![
                    v["ref"].as_str().unwrap_or("").to_string(),
                    v["status"]["name"].as_str().unwrap_or("").to_string(),
                    v["title"].as_str().unwrap_or("").to_string(),
                ]
            })
            .collect();
        let mut text = table(&rows);
        if let Some(c) = &next_cursor {
            text.push_str(&format!(
                "\n{}",
                s.muted(&format!("more: tmgr task list --cursor {c}"))
            ));
        }
        text
    };
    Output::new(
        json!({"workspace": sanitize_line(ws_code), "items": views, "next_cursor": next_cursor}),
        human,
    )
}

const CONFIRM_PAGE: usize = 40;

fn confirm(ctx: &Ctx, reference: &str) -> Result<Output> {
    let tag = ctx.local_tag()?.ok_or_else(|| {
        Error::usage("tmgr task confirm works on local workspaces only")
            .hint("use --ws local:<code>")
    })?;
    let code = tmgr_core::local::local_code(&tag)
        .ok_or_else(|| Error::usage("not a local workspace"))?
        .to_string();
    let tty = std::io::IsTerminal::is_terminal(&std::io::stdin())
        && std::io::IsTerminal::is_terminal(&std::io::stdout());
    tmgr_link::privileged::check(tmgr_core::local::agent_session_detected(), tty)?;
    let client = ctx.client()?;
    let task = super::shared::resolve_task(ctx, client, reference)?;
    let snapshot = tmgr_core::local::task_snapshot(&code, task.id as i64)?;
    let hash = snapshot.hash();
    let mut lines = vec![
        format!("Task {}", task_ref(&task)),
        String::new(),
        format!("Title: {}", sanitize_line(&snapshot.title)),
        String::new(),
        "Description:".to_string(),
    ];
    lines.extend(sanitize(&snapshot.description).lines().map(str::to_string));
    if !snapshot.description_json.is_empty() {
        lines.push(String::new());
        lines.push("Description (editor data):".to_string());
        lines.extend(
            sanitize(&snapshot.description_json)
                .lines()
                .map(str::to_string),
        );
    }
    if !snapshot.checkpoints.is_empty() {
        lines.push(String::new());
        lines.push("Checkpoints:".to_string());
        lines.extend(sanitize(&snapshot.checkpoints).lines().map(str::to_string));
    }
    for (id, message) in &snapshot.comments {
        lines.push(String::new());
        lines.push(format!("Comment #{id}:"));
        lines.extend(sanitize(message).lines().map(str::to_string));
    }
    lines.push(String::new());
    lines.push(format!("Snapshot: {hash}"));
    {
        let mut input = std::io::stdin().lock();
        for (n, chunk) in lines.chunks(CONFIRM_PAGE).enumerate() {
            for l in chunk {
                eprintln!("{l}");
            }
            if (n + 1) * CONFIRM_PAGE < lines.len() {
                eprint!("-- more, press Enter -- ");
                let mut sink = String::new();
                std::io::BufRead::read_line(&mut input, &mut sink)
                    .map_err(|_| Error::new(Kind::Interrupted, "could not read the answer"))?;
            }
        }
    }
    tmgr_link::privileged::confirm_typed_stdio(
        &hash[..12],
        "\nThis is everything an agent will read. Type the first 12 characters of the snapshot to confirm:",
    )?;
    let confirmed =
        tmgr_core::local::confirm_task(&code, ctx.cfg.local_user_id, task.id as i64, &hash)?;
    Ok(Output::new(
        json!({ "task": task_ref(&task), "snapshot": confirmed, "confirmed": true }),
        format!(
            "Confirmed {} for agent runs (snapshot {}).",
            task_ref(&task),
            &confirmed[..12]
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursors_round_trip_and_reject_garbage() {
        assert_eq!(encode_cursor(3, 0), "3");
        assert_eq!(encode_cursor(3, 7), "3.7");
        let c = parse_cursor(Some("3.7")).unwrap();
        assert_eq!((c.page, c.skip), (3, 7));
        let c = parse_cursor(None).unwrap();
        assert_eq!((c.page, c.skip), (1, 0));
        for bad in ["", "0", "a", "1.", ".1", "1.2.3", "-1", "1234567", "1;2"] {
            assert!(parse_cursor(Some(bad)).is_err(), "{bad}");
        }
    }
}
