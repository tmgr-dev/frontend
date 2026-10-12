//! Status, category and feature-toggle writes, ported from the TS `api.ts` and `modulesApi.ts`.

use rusqlite::types::Value as Sql;
use serde_json::{json, Value};

use super::catalog::{status_for, statuses, CATEGORY_COUNTS};
use super::task_writes::{json_sql, note_status_change, number_or_null, number_sql, to_json};
use super::tasks::stop_timer;
use super::{int, text};
use crate::db::{execute, select_one, Row};
use crate::js;
use crate::router::{http, not_found, sql, Handled, Outcome, Request};
use crate::serialize::{category_json, col, status_json};

/// Workspace modules a local workspace may switch, in catalog order.
const TOGGLEABLE: [&str; 10] = [
    "task.assignees",
    "workflow",
    "task.checkpoints",
    "task.relations",
    "daily_routines",
    "task.files",
    "pages",
    "graph",
    "agent_work",
    "personas",
];

fn write_meta(req: &Request<'_>, key: &str, value: &str) -> Result<(), Outcome> {
    sql(execute(
        req.conn,
        "INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        &[text(key), text(value)],
    ))
    .map(|_| ())
}

pub(super) fn put_feature_toggles(req: &Request<'_>) -> Handled {
    let features = req.body.get("features");
    for key in TOGGLEABLE {
        if let Some(Value::Bool(on)) = features.and_then(|f| f.get(key)) {
            write_meta(req, &format!("feature.{key}"), if *on { "1" } else { "0" })?;
        }
    }
    Ok(json!({}))
}

fn field<'a>(req: &'a Request<'_>, key: &str) -> Option<&'a Value> {
    req.body.get(key).filter(|v| !v.is_null())
}

fn status_row(req: &Request<'_>, id: i64) -> Result<Row, Outcome> {
    Ok(sql(select_one(
        req.conn,
        "SELECT * FROM statuses WHERE id = ?",
        &[int(id)],
    ))?
    .unwrap_or_default())
}

pub(super) fn create_status(req: &Request<'_>) -> Handled {
    let name = js::string_or_empty(req.body.get("name")).trim().to_string();
    if name.is_empty() {
        return Err(http(422, "name is required"));
    }
    let kind = req
        .body
        .get("type")
        .filter(|v| js::truthy(v))
        .map_or(text("active"), json_sql);
    let color = field(req, "color").map_or(Sql::Null, json_sql);
    let now = req.ctx.iso_now();
    let inserted = sql(execute(
        req.conn,
        "INSERT INTO statuses (name, type, color, sort_order, created_at, updated_at)
				 SELECT ?, ?, ?, COALESCE(MAX(sort_order), 0) + 1, ?, ? FROM statuses",
        &[text(&name), kind, color, text(&now), text(&now)],
    ))?;
    Ok(status_json(&status_row(req, inserted.last_id)?))
}

pub(super) fn order_statuses(req: &Request<'_>) -> Handled {
    let now = req.ctx.iso_now();
    for item in req
        .body
        .get("statuses_with_order")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        sql(execute(
            req.conn,
            "UPDATE statuses SET sort_order = ?, updated_at = ? WHERE id = ?",
            &[
                number_sql(item.get("order").unwrap_or(&Value::Null)),
                text(&now),
                number_sql(item.get("status_id").unwrap_or(&Value::Null)),
            ],
        ))?;
    }
    statuses(req)
}

pub(super) fn update_status(req: &Request<'_>) -> Handled {
    let status = status_for(req, req.param("id"))?;
    let pick = |k: &str| field(req, k).map_or_else(|| json_sql(col(&status, k)), json_sql);
    sql(execute(
        req.conn,
        "UPDATE statuses SET name = ?, type = ?, color = ?, updated_at = ? WHERE id = ?",
        &[
            pick("name"),
            pick("type"),
            pick("color"),
            text(&req.ctx.iso_now()),
            json_sql(col(&status, "id")),
        ],
    ))?;
    Ok(status_json(&status_for(req, req.param("id"))?))
}

pub(super) fn delete_status(req: &Request<'_>) -> Handled {
    let status = status_for(req, req.param("id"))?;
    let id = json_sql(col(&status, "id"));
    let n = sql(select_one(
        req.conn,
        "SELECT COUNT(*) AS n FROM tasks WHERE status_id = ? AND deleted_at IS NULL",
        std::slice::from_ref(&id),
    ))?
    .and_then(|r| col(&r, "n").as_i64())
    .unwrap_or(0);
    if n > 0 {
        return Err(http(409, "Move the tasks out of this status first"));
    }
    sql(execute(
        req.conn,
        "DELETE FROM statuses WHERE id = ?",
        &[id],
    ))?;
    Ok(json!({ "success": true }))
}

pub(super) fn move_tasks(req: &Request<'_>) -> Handled {
    let status = status_for(req, req.param("id"))?;
    let status_id = col(&status, "id").clone();
    let archived = col(&status, "type").as_str() == Some("archived");
    for task in req
        .body
        .get("task_ids")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let task_id = number_sql(task);
        let id = match &task_id {
            Sql::Integer(i) => *i,
            _ => i64::MIN,
        };
        if archived {
            stop_timer(req, id)?;
        }
        let before = sql(select_one(
            req.conn,
            "SELECT status_id FROM tasks WHERE id = ?",
            std::slice::from_ref(&task_id),
        ))?;
        let moved = sql(execute(
            req.conn,
            "UPDATE tasks SET status_id = ?, updated_at = ? WHERE id = ?",
            &[json_sql(&status_id), text(&req.ctx.iso_now()), task_id],
        ))?;
        if moved.changes > 0 && before.is_some_and(|b| *col(&b, "status_id") != status_id) {
            note_status_change(req, id);
        }
    }
    Ok(json!({ "success": true }))
}

const CYRILLIC: &[(char, &str)] = &[
    ('а', "a"),
    ('б', "b"),
    ('в', "v"),
    ('г', "g"),
    ('д', "d"),
    ('е', "e"),
    ('ё', "e"),
    ('ж', "zh"),
    ('з', "z"),
    ('и', "i"),
    ('й', "y"),
    ('к', "k"),
    ('л', "l"),
    ('м', "m"),
    ('н', "n"),
    ('о', "o"),
    ('п', "p"),
    ('р', "r"),
    ('с', "s"),
    ('т', "t"),
    ('у', "u"),
    ('ф', "f"),
    ('х', "h"),
    ('ц', "ts"),
    ('ч', "ch"),
    ('ш', "sh"),
    ('щ', "sch"),
    ('ъ', ""),
    ('ы', "y"),
    ('ь', ""),
    ('э', "e"),
    ('ю', "yu"),
    ('я', "ya"),
];

/// Same derivation as the Java `ProjectCategoryService`: transliterate, hyphenate, uppercase, `CAT` when empty.
fn sanitize_code(input: &str) -> String {
    let mut latin = String::new();
    for ch in input.chars() {
        let lower = ch.to_lowercase().next().unwrap_or(ch);
        match CYRILLIC.iter().find(|(c, _)| *c == lower) {
            Some((_, t)) if lower != ch => {
                let mut cs = t.chars();
                if let Some(first) = cs.next() {
                    latin.extend(first.to_uppercase());
                    latin.extend(cs);
                }
            }
            Some((_, t)) => latin.push_str(t),
            None => latin.push(ch),
        }
    }
    let mut code = String::new();
    for c in latin.chars() {
        if c.is_ascii_alphanumeric() {
            code.push(c);
        } else if !code.ends_with('-') {
            code.push('-');
        }
    }
    let code = code.trim_matches('-').to_ascii_uppercase();
    if code.is_empty() {
        "CAT".into()
    } else {
        code
    }
}

fn unique_code(base: &str, existing: &[String]) -> String {
    let mut base = base.to_uppercase();
    if base.chars().count() > 50 {
        base = base.chars().take(50).collect();
    }
    let taken = |c: &str| existing.iter().any(|e| e.to_uppercase() == c);
    if !taken(&base) {
        return base;
    }
    (1..)
        .map(|i| {
            let suffix = format!("-{i}");
            let room = 50 - suffix.len();
            format!("{}{suffix}", base.chars().take(room).collect::<String>())
        })
        .find(|c| !taken(c))
        .unwrap_or_default()
}

fn require_unique_code(req: &Request<'_>, code: &str, except: Option<i64>) -> Result<(), Outcome> {
    let mut query = String::from(
        "SELECT COUNT(*) AS n FROM categories WHERE deleted_at IS NULL AND UPPER(code) = UPPER(?)",
    );
    let mut params = vec![text(code)];
    if let Some(id) = except.filter(|id| *id != 0) {
        query.push_str(" AND id <> ?");
        params.push(int(id));
    }
    let n = sql(select_one(req.conn, &query, &params))?
        .and_then(|r| col(&r, "n").as_i64())
        .unwrap_or(0);
    if n > 0 {
        return Err(http(422, format!("code {code} is already used")));
    }
    Ok(())
}

fn slug_of(title: &str) -> String {
    let mut slug = String::new();
    for c in title.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            slug.push(c);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    if slug.is_empty() {
        "category".into()
    } else {
        slug.into()
    }
}

fn category_by_id(req: &Request<'_>, id: i64) -> Handled {
    let row = sql(select_one(
        req.conn,
        &format!("{CATEGORY_COUNTS} WHERE c.id = ?"),
        &[int(id)],
    ))?
    .unwrap_or_default();
    Ok(category_json(&row, req.ctx))
}

fn code_value(v: &Value) -> Option<String> {
    js::truthy(v).then(|| js::string(v).to_uppercase())
}

fn settings_of(req: &Request<'_>) -> Option<Sql> {
    field(req, "settings")
        .filter(|s| s.is_array())
        .map(|s| to_json(Some(s)))
}

pub(super) fn create_category(req: &Request<'_>) -> Handled {
    let title = js::string_or_empty(req.body.get("title"))
        .trim()
        .to_string();
    if title.is_empty() {
        return Err(http(422, "title is required"));
    }
    let code = match req.body.get("code").and_then(code_value) {
        Some(code) => {
            require_unique_code(req, &code, None)?;
            code
        }
        None => {
            let existing: Vec<String> = crate::db::select(
                req.conn,
                "SELECT code FROM categories WHERE deleted_at IS NULL AND code IS NOT NULL",
                &[],
            )
            .map_err(Outcome::Failed)?
            .iter()
            .filter_map(|r| col(r, "code").as_str().map(str::to_string))
            .collect();
            unique_code(&sanitize_code(&title), &existing)
        }
    };
    let slug = field(req, "slug").map_or_else(|| text(&slug_of(&title)), json_sql);
    let parent = number_or_null(req.body.get("project_category_id").unwrap_or(&Value::Null));
    let now = req.ctx.iso_now();
    let inserted = sql(execute(
        req.conn,
        "INSERT INTO categories (title, slug, code, parent_id, settings, created_at, updated_at)
				 VALUES (?, ?, ?, ?, ?, ?, ?)",
        &[
            text(&title),
            slug,
            text(&code),
            parent,
            settings_of(req).unwrap_or_else(|| text("[]")),
            text(&now),
            text(&now),
        ],
    ))?;
    category_by_id(req, inserted.last_id)
}

pub(super) fn update_category(req: &Request<'_>) -> Handled {
    let id = req.id("id");
    let row = sql(select_one(
        req.conn,
        "SELECT * FROM categories WHERE id = ? AND deleted_at IS NULL",
        &[int(id)],
    ))?
    .ok_or_else(|| not_found("Category"))?;
    let current_code = col(&row, "code").as_str().map(str::to_string);
    let next_code = match req.body.get("code") {
        Some(v) => code_value(v),
        None => current_code.clone(),
    };
    if let Some(code) = &next_code {
        if code.to_uppercase() != current_code.clone().unwrap_or_default().to_uppercase() {
            require_unique_code(req, code, Some(id))?;
        }
    }
    let parent = match req.body.get("project_category_id") {
        Some(v) => number_or_null(v),
        None => json_sql(col(&row, "parent_id")),
    };
    sql(execute(
        req.conn,
        "UPDATE categories SET title = ?, code = ?, parent_id = ?, settings = ?, updated_at = ? WHERE id = ?",
        &[
            field(req, "title").map_or_else(|| json_sql(col(&row, "title")), json_sql),
            next_code.map_or(Sql::Null, Sql::Text),
            parent,
            settings_of(req).unwrap_or_else(|| json_sql(col(&row, "settings"))),
            text(&req.ctx.iso_now()),
            int(id),
        ],
    ))?;
    category_by_id(req, id)
}

pub(super) fn delete_category(req: &Request<'_>) -> Handled {
    sql(execute(
        req.conn,
        "UPDATE categories SET deleted_at = ? WHERE id = ?",
        &[text(&req.ctx.iso_now()), int(req.id("id"))],
    ))?;
    Ok(json!({ "success": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_codes_follow_the_java_derivation() {
        assert_eq!(sanitize_code("Ops team"), "OPS-TEAM");
        assert_eq!(sanitize_code("Жизнь"), "ZHIZN");
        assert_eq!(sanitize_code("!!"), "CAT");
        assert_eq!(unique_code("OPS", &["ops".into(), "OPS-1".into()]), "OPS-2");
    }
}
