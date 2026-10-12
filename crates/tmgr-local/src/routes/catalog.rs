use serde_json::{json, Map, Value};

use super::int;
use crate::db::{select, select_one, Row};
use crate::router::{not_found, sql, Handled, Outcome, Request};
use crate::serialize::{category_json, col, paginate, status_json};

pub(super) const CATEGORY_COUNTS: &str = "
	SELECT c.*,
		(SELECT COUNT(*) FROM categories k WHERE k.parent_id = c.id AND k.deleted_at IS NULL) AS children_count,
		(SELECT COUNT(*) FROM tasks t WHERE t.project_category_id = c.id AND t.deleted_at IS NULL) AS tasks_count
	FROM categories c";

const FEATURE_TOGGLES: [(&str, bool); 11] = [
    ("board", true),
    ("categories", true),
    ("dashboard", false),
    ("daily_routines", true),
    ("task.countdown", true),
    ("task.checkpoints", true),
    ("task.assignees", false),
    ("task.files", true),
    ("task.relations", true),
    ("pages", true),
    ("workflow", true),
];

pub(super) fn load_statuses(req: &Request<'_>) -> Result<Vec<Row>, Outcome> {
    sql(select(
        req.conn,
        "SELECT * FROM statuses ORDER BY CASE WHEN type = 'archived' THEN 1 ELSE 0 END, sort_order, id",
        &[],
    ))
}

/// A status by id, or by type (`done` meaning `archived`), like the TS `statusFor`.
pub(super) fn status_for(req: &Request<'_>, id_or_type: &str) -> Result<Row, Outcome> {
    let statuses = load_statuses(req)?;
    let found = if !id_or_type.is_empty() && id_or_type.bytes().all(|b| b.is_ascii_digit()) {
        let id: f64 = id_or_type.parse().unwrap_or(f64::NAN);
        statuses
            .into_iter()
            .find(|s| col(s, "id").as_f64() == Some(id))
    } else {
        let wanted = if id_or_type == "done" {
            "archived"
        } else {
            id_or_type
        };
        statuses
            .into_iter()
            .find(|s| col(s, "type").as_str() == Some(wanted))
    };
    found.ok_or_else(|| not_found("Status"))
}

pub(super) fn statuses(req: &Request<'_>) -> Handled {
    Ok(Value::Array(
        load_statuses(req)?.iter().map(status_json).collect(),
    ))
}

pub(super) fn status(req: &Request<'_>) -> Handled {
    Ok(status_json(&status_for(req, req.param("id"))?))
}

fn categories_where(
    req: &Request<'_>,
    tail: &str,
    params: &[rusqlite::types::Value],
) -> Result<Vec<Value>, Outcome> {
    let rows = sql(select(
        req.conn,
        &format!("{CATEGORY_COUNTS} {tail}"),
        params,
    ))?;
    Ok(rows.iter().map(|r| category_json(r, req.ctx)).collect())
}

pub(super) fn categories(req: &Request<'_>) -> Handled {
    Ok(Value::Array(categories_where(
        req,
        "WHERE c.deleted_at IS NULL ORDER BY c.title COLLATE NOCASE",
        &[],
    )?))
}

pub(super) fn root_categories(req: &Request<'_>) -> Handled {
    Ok(Value::Array(categories_where(
        req,
        "WHERE c.deleted_at IS NULL AND c.parent_id IS NULL ORDER BY c.updated_at DESC",
        &[],
    )?))
}

pub(super) fn child_categories(req: &Request<'_>) -> Handled {
    let all = categories_where(
        req,
        "WHERE c.deleted_at IS NULL AND c.parent_id = ? ORDER BY c.updated_at DESC",
        &[int(req.id("id"))],
    )?;
    let per_page = crate::js::positive_or(req.query.get("per_page"), 20);
    let page = crate::js::positive_or(req.query.get("page"), 1);
    let total = all.len() as i64;
    let start = (page - 1).saturating_mul(per_page).min(total) as usize;
    let end = page.saturating_mul(per_page).min(total) as usize;
    Ok(paginate(
        all[start..end].to_vec(),
        total,
        page,
        per_page,
        &format!("/api/project_categories/children/{}", req.param("id")),
        Map::new(),
    ))
}

fn category_row(req: &Request<'_>, id: i64) -> Result<Option<Row>, Outcome> {
    sql(select_one(
        req.conn,
        &format!("{CATEGORY_COUNTS} WHERE c.id = ? AND c.deleted_at IS NULL"),
        &[int(id)],
    ))
}

pub(super) fn category_with_parents(req: &Request<'_>) -> Handled {
    fn load(req: &Request<'_>, id: i64, depth: u32) -> Result<Value, Outcome> {
        let Some(row) = category_row(req, id)? else {
            return Ok(Value::Null);
        };
        let mut json = category_json(&row, req.ctx);
        let parent = col(&row, "parent_id").as_i64().filter(|p| *p != 0);
        json["parent_category"] = match parent {
            Some(p) if depth < 32 => load(req, p, depth + 1)?,
            _ => Value::Null,
        };
        Ok(json)
    }
    match load(req, req.id("id"), 0)? {
        Value::Null => Err(not_found("Category")),
        found => Ok(found),
    }
}

pub(super) fn category(req: &Request<'_>) -> Handled {
    let row = category_row(req, req.id("id"))?.ok_or_else(|| not_found("Category"))?;
    Ok(category_json(&row, req.ctx))
}

pub(super) fn members(req: &Request<'_>) -> Handled {
    let user = &req.ctx.user;
    Ok(
        json!([{ "id": user.id, "name": user.name, "email": user.email, "role": "owner", "has_avatar": false }]),
    )
}

pub(crate) fn meta_value(req: &Request<'_>, key: &str) -> Result<Option<String>, Outcome> {
    Ok(sql(select_one(
        req.conn,
        "SELECT value FROM meta WHERE key = ?",
        &[super::text(key)],
    ))?
    .and_then(|r| col(&r, "value").as_str().map(str::to_string)))
}

pub(crate) fn agent_work_enabled(req: &Request<'_>) -> Result<bool, Outcome> {
    Ok(meta_value(req, "feature.agent_work")?.as_deref() != Some("0"))
}

pub(super) fn feature_toggles(req: &Request<'_>) -> Handled {
    let graph = meta_value(req, "feature.graph")?.as_deref() == Some("1");
    let agent_work = agent_work_enabled(req)?;
    let mut out = Map::new();
    let entries = FEATURE_TOGGLES
        .iter()
        .copied()
        .chain([("graph", graph), ("agent_work", agent_work)]);
    for (key, enabled) in entries {
        let name = match key {
            "agent_work" => "Agent work",
            "workflow" => "Workflow",
            other => other,
        };
        out.insert(
            key.into(),
            json!({ "key": key, "name": name, "group": "local", "enabled": enabled }),
        );
    }
    Ok(Value::Object(out))
}
