use std::collections::HashMap;

use rusqlite::types::Value as Sql;
use serde_json::{json, Map, Value};

use super::{int, text};
use crate::context::Context;
use crate::db::{execute, select, select_one, Row};
use crate::js;
use crate::router::{not_found, sql, Handled, Outcome, Request};
use crate::serialize::{col, paginate, task_json, TASK_SELECT};

pub(crate) const LOCAL_OWNER_ID: i64 = -1;

pub(super) fn current(req: &Request<'_>) -> Handled {
    list_tasks(
        req,
        vec!["(s.id IS NULL OR s.type <> 'archived')".into()],
        vec![],
        "t.id DESC",
        "tasks/current",
    )
}

pub(super) fn running(req: &Request<'_>) -> Handled {
    let rows = sql(select(
        req.conn,
        &format!("{TASK_SELECT} WHERE t.deleted_at IS NULL AND t.start_time > 0 ORDER BY t.start_time DESC"),
        &[],
    ))?;
    let n = rows.len() as i64;
    let tasks = with_persona_assignees(req.ctx, req.conn, tasks_of(&rows, req.ctx))?;
    Ok(paginate(
        tasks,
        n,
        1,
        n.max(1),
        "/api/tasks/runned",
        Map::new(),
    ))
}

pub(super) fn indexes(req: &Request<'_>) -> Handled {
    let n = count(
        req,
        "SELECT COUNT(*) AS n FROM tasks WHERE deleted_at IS NULL",
        &[],
    )?;
    let c = match req.query.filled("category") {
        Some(category) => count(
            req,
            "SELECT COUNT(*) AS n FROM tasks WHERE deleted_at IS NULL AND project_category_id = ?",
            &[number_param(category)],
        )?,
        None => 0,
    };
    Ok(json!({ "index": { "workspace": n + 1, "category": c + 1 } }))
}

pub(super) fn by_status(req: &Request<'_>) -> Handled {
    let raw = req.param("status");
    let status = super::catalog::status_for(req, raw)?;
    let order = board_order(req);
    list_tasks(
        req,
        vec!["t.status_id = ?".into()],
        vec![col(&status, "id").clone()],
        &order,
        &format!("tasks/status/{raw}"),
    )
}

pub(super) fn list(req: &Request<'_>) -> Handled {
    let order = task_sort_order(req);
    list_tasks(req, vec![], vec![], &order, "tasks")
}

pub(super) fn settings(_: &Request<'_>) -> Handled {
    Ok(json!([]))
}

pub(super) fn show(req: &Request<'_>) -> Handled {
    load_task(req, req.id("id"))
}

pub(super) fn assignable_personas(req: &Request<'_>) -> Handled {
    let rows = sql(select(
        req.conn,
        "SELECT p.* FROM personas p
	JOIN workspace_personas wp ON wp.persona_uuid = p.uuid
	WHERE p.archived_at IS NULL AND wp.disabled_at IS NULL AND p.owner_user_id IN (?, -1) ORDER BY p.name",
        &[int(req.ctx.user.id)],
    ))?;
    Ok(Value::Array(rows.iter().map(persona_view).collect()))
}

pub(super) fn relation_types(req: &Request<'_>) -> Handled {
    let rows = sql(select(
        req.conn,
        "SELECT * FROM task_relation_types ORDER BY id",
        &[],
    ))?;
    Ok(Value::Array(
        rows.iter()
            .map(|r| json!({ "id": col(r, "id"), "name": col(r, "name") }))
            .collect(),
    ))
}

pub(super) fn relations(req: &Request<'_>) -> Handled {
    let id = req.id("id");
    require_task(req, id)?;
    task_relations(req, id)
}

pub(super) fn start_timer(req: &Request<'_>) -> Handled {
    let id = req.id("id");
    sql(execute(
        req.conn,
        "UPDATE tasks SET start_time = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL AND start_time = 0",
        &[int(req.ctx.epoch()), text(&req.ctx.iso_now()), int(id)],
    ))?;
    load_task(req, id)
}

pub(super) fn stop_timer_route(req: &Request<'_>) -> Handled {
    let id = req.id("id");
    stop_timer(req, id)?;
    load_task(req, id)
}

pub(crate) fn stop_timer(req: &Request<'_>, id: i64) -> Result<(), Outcome> {
    let now = req.ctx.epoch();
    sql(execute(
        req.conn,
        "UPDATE tasks SET common_time = common_time + MAX(0, ? - start_time), start_time = 0, end_time = ?,
			updated_at = ?
		 WHERE id = ? AND start_time > 0",
        &[int(now), int(now), text(&req.ctx.iso_now()), int(id)],
    ))?;
    Ok(())
}

pub(crate) fn load_task(req: &Request<'_>, id: i64) -> Handled {
    let row = sql(select_one(
        req.conn,
        &format!("{TASK_SELECT} WHERE t.id = ? AND t.deleted_at IS NULL"),
        &[int(id)],
    ))?
    .ok_or_else(|| not_found("Task"))?;
    let mut task = with_persona_assignees(req.ctx, req.conn, vec![task_json(&row, req.ctx)])?
        .pop()
        .unwrap_or(Value::Null);
    let relations = task_relations(req, id)?;
    let mentioned = sql(select(
        req.conn,
        "SELECT p.id AS page_id, p.slug, p.title FROM task_page_mentions m JOIN pages p ON p.id = m.page_id
		 WHERE m.task_id = ? AND p.type = 'person' AND p.deleted_at IS NULL ORDER BY p.title, p.id",
        &[int(id)],
    ))?;
    if let Value::Object(map) = &mut task {
        map.insert("relationTypeWithTask".into(), relations);
        map.insert(
            "mentioned_people".into(),
            Value::Array(mentioned.into_iter().map(Value::Object).collect()),
        );
    }
    Ok(task)
}

pub(crate) fn require_task(req: &Request<'_>, id: i64) -> Result<Row, Outcome> {
    sql(select_one(
        req.conn,
        "SELECT id, common_time, start_time FROM tasks WHERE id = ? AND deleted_at IS NULL",
        &[int(id)],
    ))?
    .ok_or_else(|| not_found("Task"))
}

fn task_relations(req: &Request<'_>, id: i64) -> Handled {
    let rows = sql(select(
        req.conn,
        "SELECT r.id, rt.id AS type_id, rt.name AS type_name,
			t.id AS rt_id, t.title AS rt_title, t.status_id AS rt_status_id, t.project_category_id AS rt_category_id
		 FROM task_relations r
		 JOIN task_relation_types rt ON rt.id = r.relation_type_id
		 JOIN tasks t ON t.id = r.related_task_id
		 WHERE r.task_id = ? AND t.deleted_at IS NULL
		 ORDER BY r.id",
        &[int(id)],
    ))?;
    let ws = req.ctx.workspace.manifest.id;
    Ok(Value::Array(
        rows.iter()
            .map(|r| {
                json!({
                    "id": col(r, "id"),
                    "relation_type": { "id": col(r, "type_id"), "name": col(r, "type_name") },
                    "related_task": {
                        "id": col(r, "rt_id"),
                        "title": col(r, "rt_title"),
                        "status_id": col(r, "rt_status_id"),
                        "workspace_id": ws,
                        "project_category_id": col(r, "rt_category_id"),
                    },
                })
            })
            .collect(),
    ))
}

pub(crate) fn persona_view(row: &Row) -> Value {
    let uuid = js::string(col(row, "uuid"));
    json!({
        "id": col(row, "uuid"),
        "name": col(row, "name"),
        "description": col(row, "description"),
        "avatar_url": if js::truthy(col(row, "avatar_file")) {
            Value::String(format!("/api/personas/{uuid}/avatar"))
        } else {
            Value::Null
        },
        "owner": { "id": js::number(col(row, "owner_user_id")), "name": col(row, "owner_name") },
        "workspace_id": null,
    })
}

pub(crate) fn with_persona_assignees(
    ctx: &Context,
    conn: &rusqlite::Connection,
    tasks: Vec<Value>,
) -> Result<Vec<Value>, Outcome> {
    if tasks.is_empty() {
        return Ok(tasks);
    }
    let mut by_task: HashMap<i64, Vec<Value>> = HashMap::new();
    for chunk in tasks.chunks(500) {
        let ids: Vec<Sql> = chunk
            .iter()
            .map(|t| int(t["id"].as_i64().unwrap_or(0)))
            .collect();
        let placeholders = vec!["?"; ids.len()].join(",");
        let rows = sql(select(
            conn,
            &format!(
                "SELECT a.task_id, p.* FROM task_persona_assignees a
			 JOIN personas p ON p.uuid = a.persona_uuid
			 WHERE a.task_id IN ({placeholders}) AND p.archived_at IS NULL
			 ORDER BY a.created_at, p.name"
            ),
            &ids,
        ))?;
        for row in &rows {
            let task_id = col(row, "task_id").as_i64().unwrap_or(0);
            by_task.entry(task_id).or_default().push(persona_view(row));
        }
    }
    let mut assignee = json!({ "id": ctx.user.id, "name": ctx.user.name });
    if ctx.actor.as_ref().is_none_or(|a| a.kind != "persona") {
        assignee["email"] = json!(ctx.user.email);
    }
    assignee["has_avatar"] = json!(false);
    Ok(tasks
        .into_iter()
        .map(|mut task| {
            let personas = by_task
                .remove(&task["id"].as_i64().unwrap_or(0))
                .unwrap_or_default();
            if let Value::Object(map) = &mut task {
                let assignees = if personas.is_empty() {
                    json!([])
                } else {
                    json!([assignee.clone()])
                };
                map.insert("persona_assignees".into(), Value::Array(personas));
                map.insert("assignees".into(), assignees);
            }
            task
        })
        .collect())
}

fn tasks_of(rows: &[Row], ctx: &Context) -> Vec<Value> {
    rows.iter().map(|r| task_json(r, ctx)).collect()
}

fn count(req: &Request<'_>, query: &str, params: &[Sql]) -> Result<i64, Outcome> {
    Ok(sql(select_one(req.conn, query, params))?
        .and_then(|r| col(&r, "n").as_i64())
        .unwrap_or(0))
}

fn json_param(v: &Value) -> Sql {
    match v {
        Value::Null => Sql::Null,
        Value::Number(n) => n
            .as_i64()
            .map(Sql::Integer)
            .unwrap_or_else(|| Sql::Real(n.as_f64().unwrap_or(f64::NAN))),
        Value::String(s) => Sql::Text(s.clone()),
        other => Sql::Text(other.to_string()),
    }
}

/// `Number(raw)` bound as a SQLite parameter.
fn number_param(raw: &str) -> Sql {
    json_param(&js::float_json(js::parse_number(raw)))
}

fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn search_clause(req: &Request<'_>, params: &mut Vec<Value>) -> Vec<String> {
    let q = &req.query;
    let mut clauses = Vec::new();
    if let Some(search) = q
        .get("search")
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
    {
        let pattern = format!("%{}%", escape_like(&search));
        clauses.push(
            "(LOWER(t.title) LIKE ? ESCAPE '\\' OR LOWER(COALESCE(t.description, '')) LIKE ? ESCAPE '\\')"
                .to_string(),
        );
        params.push(json!(pattern));
        params.push(json!(pattern));
    }
    if let Some(persona) = q.filled("persona") {
        clauses.push(
            "EXISTS (SELECT 1 FROM task_persona_assignees a WHERE a.task_id = t.id AND a.persona_uuid = ?)"
                .into(),
        );
        params.push(json!(persona));
    }
    if matches!(q.get("my_personas"), Some("1" | "true")) {
        clauses.push(format!(
            "EXISTS (SELECT 1 FROM task_persona_assignees a JOIN personas p ON p.uuid = a.persona_uuid
				WHERE a.task_id = t.id AND p.owner_user_id IN (?, {LOCAL_OWNER_ID}))"
        ));
        params.push(json!(req.ctx.user.id));
    }
    let numeric = [
        ("project_category_id", "t.project_category_id = ?"),
        ("status_id", "t.status_id = ?"),
    ];
    for (key, clause) in numeric {
        if let Some(v) = q.filled(key) {
            clauses.push(clause.into());
            params.push(js::float_json(js::parse_number(v)));
        }
    }
    let plain = [
        ("status_type", "s.type = ?"),
        ("priority", "t.priority = ?"),
        ("updated_since", "julianday(t.updated_at) >= julianday(?)"),
        ("due_before", "julianday(t.expired_at) < julianday(?)"),
        ("due_after", "julianday(t.expired_at) > julianday(?)"),
    ];
    for (key, clause) in plain {
        if let Some(v) = q.filled(key) {
            clauses.push(clause.into());
            params.push(json!(v));
        }
    }
    clauses
}

fn task_sort_order(req: &Request<'_>) -> String {
    let direction = if req.query.get("direction") == Some("desc") {
        "DESC"
    } else {
        "ASC"
    };
    match req.query.get("sort") {
        Some("queue") => {
            "s.sort_order IS NULL, s.sort_order ASC, t.sort_order IS NULL, t.sort_order ASC, t.id ASC".into()
        }
        Some("due") => format!("t.expired_at IS NULL, julianday(t.expired_at) {direction}, t.id DESC"),
        Some("updated") => format!("t.updated_at {direction}, t.id DESC"),
        Some("created") => format!("t.created_at {direction}, t.id DESC"),
        _ => "t.id DESC".into(),
    }
}

fn board_order(req: &Request<'_>) -> String {
    let column = req.query.get("order[column]");
    let asc = req.query.get("order[direction]") == Some("asc");
    if column == Some("order") {
        return if asc {
            "t.sort_order IS NOT NULL, t.sort_order ASC, t.id DESC".into()
        } else {
            "t.sort_order IS NULL, t.sort_order DESC, t.id DESC".into()
        };
    }
    let col = match column {
        Some("created_at") => "t.created_at",
        Some("expired_at") => "t.expired_at",
        Some("end_time") => "t.end_time",
        _ => "t.updated_at",
    };
    format!("{col} {}, t.id DESC", if asc { "ASC" } else { "DESC" })
}

pub(crate) fn list_tasks(
    req: &Request<'_>,
    mut clauses: Vec<String>,
    mut params: Vec<Value>,
    order_by: &str,
    path: &str,
) -> Handled {
    clauses.insert(0, "t.deleted_at IS NULL".into());
    clauses.extend(search_clause(req, &mut params));
    let where_sql = format!("WHERE {}", clauses.join(" AND "));
    let all = req.query.has("all");
    let per_page = if all {
        0
    } else {
        js::positive_or(req.query.get("per_page"), 20)
    };
    let page = if all {
        1
    } else {
        js::positive_or(req.query.get("page"), 1)
    };
    let mut bound: Vec<Sql> = params.iter().map(json_param).collect();
    let totals = sql(select_one(
        req.conn,
        &format!(
            "SELECT COUNT(*) AS n, COALESCE(SUM(t.common_time), 0) AS seconds FROM tasks t
		 LEFT JOIN statuses s ON s.id = t.status_id {where_sql}"
        ),
        &bound,
    ))?
    .unwrap_or_default();
    let mut query = format!("{TASK_SELECT} {where_sql} ORDER BY {order_by}");
    if !all {
        query.push_str(" LIMIT ? OFFSET ?");
        bound.push(int(per_page));
        bound.push(int((page - 1).saturating_mul(per_page)));
    }
    let rows = sql(select(req.conn, &query, &bound))?;
    let total = col(&totals, "n").as_i64().unwrap_or(0);
    let mut extra = Map::new();
    extra.insert(
        "total_seconds".into(),
        json!(js::string(col(&totals, "seconds"))),
    );
    extra.insert("total_overtime_seconds".into(), json!("0"));
    let tasks = with_persona_assignees(req.ctx, req.conn, tasks_of(&rows, req.ctx))?;
    Ok(paginate(
        tasks,
        total,
        page,
        if all { total.max(1) } else { per_page },
        &format!("/api/{path}"),
        extra,
    ))
}
