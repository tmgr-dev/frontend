use serde_json::{json, Map, Value};

use crate::context::Context;
use crate::db::Row;
use crate::js;

pub(crate) const NULL: Value = Value::Null;

pub(crate) fn col<'a>(row: &'a Row, key: &str) -> &'a Value {
    row.get(key).unwrap_or(&NULL)
}

pub(crate) fn user_ref(ctx: &Context) -> Value {
    json!({ "id": ctx.user.id, "name": ctx.user.name })
}

pub(crate) fn category_json(row: &Row, ctx: &Context) -> Value {
    json!({
        "id": col(row, "id"),
        "title": col(row, "title"),
        "slug": col(row, "slug"),
        "code": col(row, "code"),
        "workspace_id": ctx.workspace.manifest.id,
        "project_category_id": col(row, "parent_id"),
        "parent_category": null,
        "settings": js::parse_json(col(row, "settings"), json!([])),
        "children_count": js::number(col(row, "children_count")),
        "tasks_count": js::number(col(row, "tasks_count")),
        "user_id": ctx.user.id,
        "user": user_ref(ctx),
        "created_at": col(row, "created_at"),
        "updated_at": col(row, "updated_at"),
        "deleted_at": col(row, "deleted_at"),
    })
}

pub(crate) const TASK_SELECT: &str = "
	SELECT t.*, s.type AS status_type,
		c.id AS c_id, c.title AS c_title, c.slug AS c_slug, c.code AS c_code, c.parent_id AS c_parent_id,
		(SELECT COUNT(*) FROM comments m WHERE m.task_id = t.id AND m.deleted_at IS NULL) AS comments_count
	FROM tasks t
	LEFT JOIN statuses s ON s.id = t.status_id
	LEFT JOIN categories c ON c.id = t.project_category_id";

pub(crate) fn task_json(row: &Row, ctx: &Context) -> Value {
    let category = if js::truthy(col(row, "c_id")) {
        json!({
            "id": col(row, "c_id"),
            "title": col(row, "c_title"),
            "slug": col(row, "c_slug"),
            "code": col(row, "c_code"),
            "workspace_id": ctx.workspace.manifest.id,
            "project_category_id": col(row, "c_parent_id"),
        })
    } else {
        Value::Null
    };
    json!({
        "id": col(row, "id"),
        "title": col(row, "title"),
        "description": col(row, "description"),
        "description_json": js::parse_json(col(row, "description_json"), Value::Null),
        "status": col(row, "status_type"),
        "status_id": col(row, "status_id"),
        "project_category_id": col(row, "project_category_id"),
        "category_tasks_sequence_id": col(row, "category_tasks_sequence_id"),
        "order": col(row, "sort_order"),
        "priority": col(row, "priority"),
        "common_time": js::number(col(row, "common_time")),
        "start_time": js::number(col(row, "start_time")),
        "end_time": col(row, "end_time"),
        "approximately_time": js::number(col(row, "approximately_time")),
        "checkpoints": js::parse_json(col(row, "checkpoints"), json!([])),
        "settings": js::parse_json(col(row, "settings"), json!([])),
        "expired_at": col(row, "expired_at"),
        "is_daily_routine": false,
        "daily_routine": false,
        "user_id": ctx.user.id,
        "user": user_ref(ctx),
        "workspace_id": ctx.workspace.manifest.id,
        "assignees": [],
        "comments_count": js::number(col(row, "comments_count")),
        "category": category,
        "created_at": col(row, "created_at"),
        "updated_at": col(row, "updated_at"),
        "deleted_at": col(row, "deleted_at"),
    })
}

pub(crate) fn status_json(row: &Row) -> Value {
    json!({
        "id": col(row, "id"),
        "name": col(row, "name"),
        "type": col(row, "type"),
        "color": col(row, "color"),
        "pivot": { "status_id": col(row, "id"), "is_active": 1, "order": col(row, "sort_order") },
    })
}

pub(crate) fn paginate(
    rows: Vec<Value>,
    total: i64,
    page: i64,
    per_page: i64,
    path: &str,
    extra: Map<String, Value>,
) -> Value {
    let per_page = per_page.max(1);
    let last_page = if total > 0 {
        (total - 1) / per_page + 1
    } else {
        1
    };
    let (from, to) = if total > 0 {
        (
            json!((page - 1).saturating_mul(per_page).saturating_add(1)),
            json!(page.saturating_mul(per_page).min(total)),
        )
    } else {
        (Value::Null, Value::Null)
    };
    let mut meta = json!({
        "current_page": page,
        "from": from,
        "last_page": last_page,
        "links": [],
        "path": path,
        "per_page": per_page,
        "to": to,
        "total": total,
    });
    if let Value::Object(m) = &mut meta {
        m.extend(extra);
    }
    json!({
        "data": rows,
        "links": { "first": null, "last": null, "prev": null, "next": null },
        "meta": meta,
    })
}

/// The key people see on a task, as the Java `TaskKey`: title prefix, else category code and ticket number, else `T<id>`.
pub(crate) fn task_key(title: &str, code: Option<&str>, sequence: Option<i64>, id: i64) -> String {
    if let Some(prefix) = title_prefix(title) {
        return prefix.to_ascii_uppercase();
    }
    match (code.filter(|c| !c.trim().is_empty()), sequence) {
        (Some(code), Some(seq)) => format!("{code}-{seq}"),
        _ => format!("T{id}"),
    }
}

fn title_prefix(title: &str) -> Option<&str> {
    let t = title.trim_start();
    let bytes = t.as_bytes();
    if !bytes.first()?.is_ascii_alphabetic() {
        return None;
    }
    let dash = t.find('-')?;
    if !t[..dash].bytes().all(|b| b.is_ascii_alphanumeric()) {
        return None;
    }
    let digits = t[dash + 1..].bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let end = dash + 1 + digits;
    t[end..].trim_start().starts_with(':').then_some(&t[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_key_prefers_the_title_prefix() {
        assert_eq!(task_key("tm-12: Fix", Some("X"), Some(3), 9), "TM-12");
        assert_eq!(task_key("  AB1-7 : y", None, None, 9), "AB1-7");
        assert_eq!(task_key("Fix TM-12: no", Some("TM"), Some(3), 9), "TM-3");
        assert_eq!(task_key("TM-12 no colon", Some(" "), Some(3), 9), "T9");
        assert_eq!(task_key("1A-2: x", None, None, 4), "T4");
    }

    #[test]
    fn paginate_matches_the_ts_meta() {
        let page = paginate(vec![], 0, 1, 20, "/api/tasks", Map::new());
        assert_eq!(page["meta"]["last_page"], 1);
        assert_eq!(page["meta"]["from"], Value::Null);
        let page = paginate(vec![], 41, 3, 20, "/api/tasks", Map::new());
        assert_eq!(page["meta"]["last_page"], 3);
        assert_eq!(page["meta"]["from"], 41);
        assert_eq!(page["meta"]["to"], 41);
    }
}
