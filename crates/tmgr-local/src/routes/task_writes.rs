//! Task, comment and persona-assignee writes, ported from the TS `api.ts` and `personaAssignees.ts`.

use rusqlite::types::Value as Sql;
use serde_json::{json, Value};

use super::catalog::{load_statuses, status_for};
use super::comments::{comment_json, reactions_for};
use super::page_writes::sync_task_mentions;
use super::tasks::{load_task, persona_view, stop_timer};
use super::{int, text};
use crate::db::{execute, select, select_one, Row};
use crate::journal::{self, Entry};
use crate::js;
use crate::persona::LOCAL_OWNER_ID;
use crate::router::{coded, http, not_found, sql, Handled, Outcome, Request};
use crate::serialize::col;

const ROUTINE_ID_BASE: i64 = 1_000_000_000;
const BOARD_MOVE_META_PREFIX: &str = "onboarding:board_move:";

/// `Number(value)` as a SQLite value: whole numbers as integers, NaN as NULL.
pub(super) fn number_sql(v: &Value) -> Sql {
    let f = match v {
        Value::Null => 0.0,
        Value::Bool(b) => *b as i64 as f64,
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::String(s) => js::parse_number(s),
        Value::Array(a) if a.is_empty() => 0.0,
        Value::Array(a) if a.len() == 1 => return number_sql(&Value::String(js::string(&a[0]))),
        _ => f64::NAN,
    };
    json_sql(&js::float_json(f))
}

pub(super) fn json_sql(v: &Value) -> Sql {
    match v {
        Value::Null => Sql::Null,
        Value::Bool(b) => Sql::Integer(*b as i64),
        Value::Number(n) => n
            .as_i64()
            .map(Sql::Integer)
            .unwrap_or_else(|| Sql::Real(n.as_f64().unwrap_or(f64::NAN))),
        Value::String(s) => Sql::Text(s.clone()),
        other => Sql::Text(js::string(other)),
    }
}

pub(super) fn number_or_null(v: &Value) -> Sql {
    match v {
        Value::Null => Sql::Null,
        Value::String(s) if s.is_empty() => Sql::Null,
        other => number_sql(other),
    }
}

/// `Number(x) || 0`, then `Math.max(0, …)` when `floor_zero`.
fn number_or_zero(v: &Value, floor_zero: bool) -> Sql {
    let n = match number_sql(v) {
        Sql::Integer(i) => i as f64,
        Sql::Real(f) => f,
        _ => 0.0,
    };
    let n = if n.is_nan() { 0.0 } else { n };
    json_sql(&js::float_json(if floor_zero { n.max(0.0) } else { n }))
}

pub(super) fn to_json(v: Option<&Value>) -> Sql {
    match v {
        None | Some(Value::Null) => Sql::Null,
        Some(v) => Sql::Text(js::stringify(v)),
    }
}

/// A bare date is kept as sent; anything with a time becomes ISO UTC, like the TS `normalizeExpiredAt`.
fn normalize_expired_at(req: &Request<'_>, v: &Value) -> Result<Sql, Outcome> {
    if v.is_null() {
        return Ok(Sql::Null);
    }
    let raw = js::string(v);
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(Sql::Null);
    }
    if chrono::NaiveDate::parse_from_str(trimmed, "%Y-%m-%d").is_ok() && trimmed.len() == 10 {
        return Ok(text(trimmed));
    }
    let parsed = chrono::DateTime::parse_from_rfc3339(trimmed)
        .map(|d| d.with_timezone(&chrono::Utc))
        .ok()
        .or_else(|| {
            [
                "%Y-%m-%dT%H:%M:%S%.f",
                "%Y-%m-%dT%H:%M",
                "%Y-%m-%d %H:%M:%S%.f",
                "%Y-%m-%d %H:%M",
            ]
            .iter()
            .find_map(|f| chrono::NaiveDateTime::parse_from_str(trimmed, f).ok())
            .and_then(|d| d.and_local_timezone(req.ctx.local_offset).single())
            .map(|d| d.with_timezone(&chrono::Utc))
        });
    let at = parsed.ok_or_else(|| http(422, "expired_at must be an ISO date"))?;
    let iso = at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    Ok(text(
        &iso.strip_suffix(".000Z")
            .map_or(iso.clone(), |s| format!("{s}Z")),
    ))
}

/// The task fields a create, PUT or PATCH may set, in the TS order.
fn writable_fields(
    req: &Request<'_>,
    persona_rules: bool,
) -> Result<Vec<(&'static str, Sql)>, Outcome> {
    let body = &req.body;
    let mut fields: Vec<(&'static str, Sql)> = Vec::new();
    let set = |k: &'static str, v: Sql, fields: &mut Vec<(&'static str, Sql)>| match fields
        .iter_mut()
        .find(|(f, _)| *f == k)
    {
        Some(slot) => slot.1 = v,
        None => fields.push((k, v)),
    };
    let has = |k: &str| body.get(k).is_some();
    let get = |k: &str| body.get(k).unwrap_or(&Value::Null);
    if has("title") {
        set(
            "title",
            text(js::string_or_empty(body.get("title")).trim()),
            &mut fields,
        );
    }
    if has("description") {
        set("description", json_sql(get("description")), &mut fields);
    }
    if has("description_json") {
        set(
            "description_json",
            to_json(body.get("description_json")),
            &mut fields,
        );
    }
    if get("clear_description") == &Value::Bool(true) {
        set("description", Sql::Null, &mut fields);
    }
    if get("clear_description_json") == &Value::Bool(true) {
        set("description_json", Sql::Null, &mut fields);
    }
    if has("status_id") {
        set("status_id", number_or_null(get("status_id")), &mut fields);
    }
    if has("project_category_id") {
        set(
            "project_category_id",
            number_or_null(get("project_category_id")),
            &mut fields,
        );
    }
    if has("priority") && js::truthy(get("priority")) {
        set("priority", text(&js::string(get("priority"))), &mut fields);
    }
    if has("approximately_time") {
        set(
            "approximately_time",
            number_or_zero(get("approximately_time"), false),
            &mut fields,
        );
    }
    if has("checkpoints") {
        let checkpoints = match get("checkpoints") {
            Value::Null => json!([]),
            v => v.clone(),
        };
        set("checkpoints", to_json(Some(&checkpoints)), &mut fields);
    }
    if get("settings").is_array() {
        set("settings", to_json(body.get("settings")), &mut fields);
    }
    if has("expired_at") {
        set(
            "expired_at",
            normalize_expired_at(req, get("expired_at"))?,
            &mut fields,
        );
    }
    let persona = req.ctx.actor.as_ref().is_some_and(|a| a.kind == "persona");
    if has("common_time") && !(persona_rules && persona) {
        set(
            "common_time",
            number_or_zero(get("common_time"), true),
            &mut fields,
        );
    }
    Ok(fields)
}

fn field<'a>(fields: &'a [(&'static str, Sql)], key: &str) -> Option<&'a Sql> {
    fields.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
}

fn require_human(req: &Request<'_>) -> Result<(), Outcome> {
    if req.ctx.actor.as_ref().is_some_and(|a| a.kind == "persona") {
        return Err(coded(
            403,
            "A persona cannot change persona assignees",
            "ROUTE_NOT_ALLOWED",
        ));
    }
    Ok(())
}

fn default_status_id(req: &Request<'_>) -> Result<Sql, Outcome> {
    let statuses = load_statuses(req)?;
    let kind = |r: &&Row| col(r, "type").as_str().map(str::to_string);
    let pick = statuses
        .iter()
        .find(|r| kind(r).as_deref() == Some("default"))
        .or_else(|| {
            statuses
                .iter()
                .find(|r| kind(r).as_deref() == Some("active"))
        })
        .or_else(|| {
            statuses
                .iter()
                .find(|r| kind(r).as_deref() != Some("archived"))
        })
        .or(statuses.first());
    Ok(pick.map_or(Sql::Null, |r| json_sql(col(r, "id"))))
}

fn sql_text(value: &Sql) -> String {
    match value {
        Sql::Null | Sql::Blob(_) => String::new(),
        Sql::Text(s) => s.clone(),
        Sql::Integer(i) => i.to_string(),
        Sql::Real(f) => js::string(&js::float_json(*f)),
    }
}

fn same_text(a: &str, b: &str) -> bool {
    a.replace("\r\n", "\n") == b.replace("\r\n", "\n")
}

/// Journal entries for the title, description and description_json a write changes.
fn task_edit_entries(
    req: &Request<'_>,
    id: i64,
    fields: &[(&'static str, Sql)],
) -> Result<Vec<Entry>, Outcome> {
    let title = field(fields, "title");
    let description = field(fields, "description");
    let description_json = field(fields, "description_json");
    if title.is_none() && description.is_none() && description_json.is_none() {
        return Ok(Vec::new());
    }
    let Some(before) = sql(select_one(
        req.conn,
        "SELECT title, description, description_json FROM tasks WHERE id = ? AND deleted_at IS NULL",
        &[int(id)],
    ))?
    else {
        return Ok(Vec::new());
    };
    let was = |key: &str| js::string_or_empty(before.get(key));
    let mut entries = Vec::new();
    if let Some(title) = title.map(sql_text).filter(|t| !same_text(t, &was("title"))) {
        entries.push(Entry::task(id, "title", Some(title)));
    }
    if let Some(after) = description
        .map(sql_text)
        .filter(|d| !same_text(d, &was("description")))
    {
        entries.push(Entry::task(id, "description", Some(after)));
    }
    if let Some(after) = description_json
        .map(sql_text)
        .filter(|d| !same_text(d, &was("description_json")))
    {
        entries.push(Entry::task(id, "description_json", Some(after)));
    }
    Ok(entries)
}

pub(super) fn note_status_change(req: &Request<'_>, task_id: i64) {
    if req.ctx.actor.as_ref().is_some_and(|a| a.kind != "user") {
        return;
    }
    let _ = execute(
        req.conn,
        "INSERT OR IGNORE INTO meta (key, value) VALUES (?, ?)",
        &[
            text(&format!("{BOARD_MOVE_META_PREFIX}{task_id}")),
            text(&req.ctx.iso_now()),
        ],
    );
}

fn task_exists(req: &Request<'_>, id: i64) -> Result<(), Outcome> {
    load_task(req, id).map(|_| ())
}

pub(super) fn create(req: &Request<'_>) -> Handled {
    let personas = req.body.get("persona_assignees").filter(|v| !v.is_null());
    if personas.is_some() {
        require_human(req)?;
    }
    let mut fields = writable_fields(req, true)?;
    let title_ok = matches!(field(&fields, "title"), Some(Sql::Text(t)) if !t.is_empty());
    if !title_ok {
        return Err(http(422, "title is required"));
    }
    if matches!(field(&fields, "status_id"), None | Some(Sql::Null)) {
        let status = default_status_id(req)?;
        match fields.iter_mut().find(|(k, _)| *k == "status_id") {
            Some(slot) => slot.1 = status,
            None => fields.push(("status_id", status)),
        }
    }
    let category = field(&fields, "project_category_id")
        .cloned()
        .unwrap_or(Sql::Null);
    let now = req.ctx.iso_now();
    let keys: Vec<&str> = fields.iter().map(|(k, _)| *k).collect();
    let mut params: Vec<Sql> = fields.iter().map(|(_, v)| v.clone()).collect();
    params.extend([category.clone(), category, text(&now), text(&now)]);
    let inserted = sql(execute(
        req.conn,
        &format!(
            "INSERT INTO tasks ({}, category_tasks_sequence_id, created_at, updated_at)
				 SELECT {},
					CASE WHEN ? IS NULL THEN NULL ELSE
						(SELECT COALESCE(MAX(category_tasks_sequence_id), 0) + 1 FROM tasks WHERE project_category_id = ?)
					END, ?, ?",
            keys.join(", "),
            vec!["?"; keys.len()].join(", ")
        ),
        &params,
    ))?;
    let id = inserted.last_id;
    let mut entries = vec![Entry::task(
        id,
        "create",
        field(&fields, "title").map(sql_text),
    )];
    if let Some(description) = field(&fields, "description")
        .map(sql_text)
        .filter(|d| !d.is_empty())
    {
        entries.push(Entry::task(id, "description", Some(description)));
    }
    if let Some(json) = field(&fields, "description_json")
        .map(sql_text)
        .filter(|d| !d.is_empty())
    {
        entries.push(Entry::task(id, "description_json", Some(json)));
    }
    sql(journal::append(req.conn, req.ctx, &entries))?;
    if let Some(wanted) = personas {
        set_task_personas(req, id, wanted)?;
    }
    sync_task_mentions(req, id)?;
    load_task(req, id)
}

fn update_task(req: &Request<'_>, id: i64, fields: Vec<(&'static str, Sql)>) -> Handled {
    let status = field(&fields, "status_id").cloned();
    let before = match &status {
        Some(_) => Some(
            sql(select_one(
                req.conn,
                "SELECT status_id FROM tasks WHERE id = ?",
                &[int(id)],
            ))?
            .map(|r| col(&r, "status_id").clone()),
        ),
        None => None,
    };
    if let Some(s) = status.as_ref().filter(|s| !matches!(s, Sql::Null)) {
        let kind = sql(select_one(
            req.conn,
            "SELECT type FROM statuses WHERE id = ?",
            std::slice::from_ref(s),
        ))?;
        if kind.is_some_and(|r| col(&r, "type").as_str() == Some("archived")) {
            stop_timer(req, id)?;
        }
    }
    let mut sets: Vec<String> = fields.iter().map(|(k, _)| format!("{k} = ?")).collect();
    sets.push("updated_at = ?".into());
    let mut values: Vec<Sql> = fields.iter().map(|(_, v)| v.clone()).collect();
    values.push(text(&req.ctx.iso_now()));
    if let Some(category) = field(&fields, "project_category_id") {
        sets.push(
            "category_tasks_sequence_id = CASE
			WHEN project_category_id IS ? THEN category_tasks_sequence_id
			WHEN ? IS NULL THEN NULL
			ELSE (SELECT COALESCE(MAX(category_tasks_sequence_id), 0) + 1 FROM tasks WHERE project_category_id = ?)
		END"
            .into(),
        );
        values.extend([category.clone(), category.clone(), category.clone()]);
    }
    values.push(int(id));
    let entries = task_edit_entries(req, id, &fields)?;
    let updated = sql(execute(
        req.conn,
        &format!(
            "UPDATE tasks SET {} WHERE id = ? AND deleted_at IS NULL",
            sets.join(", ")
        ),
        &values,
    ))?;
    if updated.changes == 0 {
        return Err(not_found("Task"));
    }
    sql(journal::append(req.conn, req.ctx, &entries))?;
    if let (Some(Some(before)), Some(after)) = (&before, &status) {
        let changed = match after {
            Sql::Null => false,
            Sql::Integer(n) => before.as_i64() != Some(*n),
            Sql::Real(f) => before.as_f64() != Some(*f),
            _ => true,
        };
        if changed {
            note_status_change(req, id);
        }
    }
    sync_task_mentions(req, id)?;
    load_task(req, id)
}

fn update_with_personas(req: &Request<'_>, persona_rules: bool) -> Handled {
    let id = req.id("id");
    if id > ROUTINE_ID_BASE {
        return Err(Outcome::Unrouted);
    }
    let fields = writable_fields(req, persona_rules)?;
    let personas = req.body.get("persona_assignees").filter(|v| !v.is_null());
    if let Some(wanted) = personas {
        require_human(req)?;
        task_exists(req, id)?;
        set_task_personas(req, id, wanted)?;
        if fields.is_empty() {
            return load_task(req, id);
        }
    }
    update_task(req, id, fields)
}

pub(super) fn put(req: &Request<'_>) -> Handled {
    update_with_personas(req, false)
}

pub(super) fn patch(req: &Request<'_>) -> Handled {
    update_with_personas(req, true)
}

pub(super) fn delete(req: &Request<'_>) -> Handled {
    let id = req.id("id");
    stop_timer(req, id)?;
    let now = req.ctx.iso_now();
    let deleted = sql(execute(
        req.conn,
        "UPDATE tasks SET deleted_at = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
        &[text(&now), text(&now), int(id)],
    ))?;
    if deleted.changes == 0 {
        return Err(not_found("Task"));
    }
    sql(journal::append(
        req.conn,
        req.ctx,
        &[Entry::task(id, "delete", None)],
    ))?;
    for statement in [
        "DELETE FROM plugin_task_data WHERE task_id = ?",
        "DELETE FROM agent_work_runs WHERE task_id = ?",
        "DELETE FROM task_persona_assignees WHERE task_id = ?",
    ] {
        sql(execute(req.conn, statement, &[int(id)]))?;
    }
    sql(execute(
        req.conn,
        "DELETE FROM task_relations WHERE task_id = ? OR related_task_id = ?",
        &[int(id), int(id)],
    ))?;
    sql(execute(
        req.conn,
        "DELETE FROM comment_reactions WHERE comment_id IN (SELECT id FROM comments WHERE task_id = ?)",
        &[int(id)],
    ))?;
    sync_task_mentions(req, id)?;
    Ok(json!({ "success": true }))
}

pub(super) fn set_time(req: &Request<'_>) -> Handled {
    let time = number_or_zero(req.body.get("common_time").unwrap_or(&Value::Null), true);
    update_task(req, req.id("id"), vec![("common_time", time)])
}

pub(super) fn set_status(req: &Request<'_>) -> Handled {
    let status = status_for(req, req.param("status"))?;
    let id = req.id("id");
    if col(&status, "type").as_str() == Some("archived") {
        stop_timer(req, id)?;
    }
    update_task(req, id, vec![("status_id", json_sql(col(&status, "id")))])
}

pub(super) fn update_orders(req: &Request<'_>) -> Handled {
    let now = req.ctx.iso_now();
    for item in req
        .body
        .get("tasks")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        sql(execute(
            req.conn,
            "UPDATE tasks SET sort_order = ?, updated_at = ? WHERE id = ?",
            &[
                number_sql(item.get("order").unwrap_or(&Value::Null)),
                text(&now),
                number_sql(item.get("id").unwrap_or(&Value::Null)),
            ],
        ))?;
    }
    Ok(json!([]))
}

pub(super) fn put_settings(req: &Request<'_>) -> Handled {
    load_task(req, req.id("id"))
}

// ── comments ────────────────────────────────────────────────────────────────

fn message_of(req: &Request<'_>) -> Result<String, Outcome> {
    let b = &req.body;
    let raw = b
        .get("message")
        .filter(|v| !v.is_null())
        .or_else(|| b.get("content").filter(|v| !v.is_null()));
    let message = js::string_or_empty(raw).trim().to_string();
    if message.is_empty() {
        return Err(http(422, "message is required"));
    }
    Ok(message)
}

fn comment_with_reactions(req: &Request<'_>, id: i64) -> Handled {
    let row = sql(select_one(
        req.conn,
        "SELECT * FROM comments WHERE id = ?",
        &[int(id)],
    ))?
    .unwrap_or_default();
    let mut json = comment_json(&row, req.ctx);
    json["reactions"] = Value::Array(reactions_for(req, &[id])?.remove(&id).unwrap_or_default());
    Ok(json)
}

pub(super) fn add_comment(req: &Request<'_>) -> Handled {
    let message = message_of(req)?;
    let task_id = req.id("id");
    task_exists(req, task_id)?;
    let actor = req.ctx.acting();
    let now = req.ctx.iso_now();
    let inserted = sql(execute(
        req.conn,
        "INSERT INTO comments (task_id, message, author_kind, author_id, author_name, created_at, updated_at)
				 VALUES (?, ?, ?, ?, ?, ?, ?)",
        &[
            int(task_id),
            text(&message),
            text(&actor.kind),
            text(&actor.id),
            text(&actor.name),
            text(&now),
            text(&now),
        ],
    ))?;
    sql(journal::append(
        req.conn,
        req.ctx,
        &[Entry::comment(
            inserted.last_id,
            task_id,
            "comment",
            Some(message.clone()),
        )],
    ))?;
    let row = sql(select_one(
        req.conn,
        "SELECT * FROM comments WHERE id = ?",
        &[int(inserted.last_id)],
    ))?
    .unwrap_or_default();
    sync_task_mentions(req, task_id)?;
    let mut json = comment_json(&row, req.ctx);
    json["reactions"] = json!([]);
    Ok(json)
}

fn own_comment_check(req: &Request<'_>, comment: &Row, verb: &str) -> Result<(), Outcome> {
    let Some(actor) = req.ctx.actor.as_ref().filter(|a| a.kind == "persona") else {
        return Ok(());
    };
    if col(comment, "author_kind").as_str() != Some("persona")
        || js::string(col(comment, "author_id")) != actor.id
    {
        return Err(coded(
            403,
            &format!("A persona may only {verb} its own comments"),
            "NOT_OWN",
        ));
    }
    Ok(())
}

pub(super) fn edit_comment(req: &Request<'_>) -> Handled {
    let id = req.id("id");
    let comment = sql(select_one(
        req.conn,
        "SELECT * FROM comments WHERE id = ? AND deleted_at IS NULL",
        &[int(id)],
    ))?
    .ok_or_else(|| not_found("Comment"))?;
    own_comment_check(req, &comment, "edit")?;
    let message = message_of(req)?;
    sql(execute(
        req.conn,
        "UPDATE comments SET message = ?, updated_at = ? WHERE id = ?",
        &[text(&message), text(&req.ctx.iso_now()), int(id)],
    ))?;
    let task_id = col(&comment, "task_id").as_i64().unwrap_or(0);
    sql(journal::append(
        req.conn,
        req.ctx,
        &[Entry::comment(id, task_id, "comment", Some(message))],
    ))?;
    sync_task_mentions(req, task_id)?;
    comment_with_reactions(req, id)
}

pub(super) fn delete_comment(req: &Request<'_>) -> Handled {
    let id = req.id("id");
    let comment = sql(select_one(
        req.conn,
        "SELECT task_id, author_kind, author_id, deleted_at FROM comments WHERE id = ?",
        &[int(id)],
    ))?;
    if req.ctx.actor.as_ref().is_some_and(|a| a.kind == "persona") {
        let found = comment.as_ref().ok_or_else(|| not_found("Comment"))?;
        own_comment_check(req, found, "delete")?;
    }
    sql(execute(
        req.conn,
        "UPDATE comments SET deleted_at = ? WHERE id = ?",
        &[text(&req.ctx.iso_now()), int(id)],
    ))?;
    let task_id = comment.as_ref().map(|c| col(c, "task_id").clone());
    if let Some(live) = comment.as_ref().filter(|c| col(c, "deleted_at").is_null()) {
        let task = col(live, "task_id").as_i64().unwrap_or(0);
        sql(journal::append(
            req.conn,
            req.ctx,
            &[Entry::comment(id, task, "delete", None)],
        ))?;
    }
    if let Some(t) = task_id.as_ref().and_then(Value::as_i64) {
        sync_task_mentions(req, t)?;
    }
    Ok(match task_id {
        Some(t) => json!({ "success": true, "task_id": t }),
        None => json!({ "success": true }),
    })
}

pub(super) fn toggle_reaction(req: &Request<'_>) -> Handled {
    let emoji = js::string_or_empty(req.body.get("emoji"));
    if emoji.trim().is_empty() || js::length(&emoji) > 32 {
        return Err(http(422, "emoji is required"));
    }
    let comment = sql(select_one(
        req.conn,
        "SELECT c.* FROM comments c JOIN tasks t ON t.id = c.task_id
				 WHERE c.id = ? AND c.deleted_at IS NULL AND t.deleted_at IS NULL",
        &[int(req.id("id"))],
    ))?
    .ok_or_else(|| not_found("Comment"))?;
    let comment_id = col(&comment, "id").as_i64().unwrap_or(0);
    let actor = req.ctx.acting();
    let owner = actor.owner().to_string();
    let existing = sql(select_one(
        req.conn,
        "SELECT id FROM comment_reactions WHERE comment_id = ? AND emoji = ? AND actor_kind = ? AND actor_id = ?",
        &[int(comment_id), text(&emoji), text(&actor.kind), text(&owner)],
    ))?;
    match existing {
        Some(row) => {
            sql(execute(
                req.conn,
                "DELETE FROM comment_reactions WHERE id = ?",
                &[json_sql(col(&row, "id"))],
            ))?;
        }
        None => {
            sql(execute(
                req.conn,
                "INSERT INTO comment_reactions (comment_id, emoji, actor_kind, actor_id, created_at) VALUES (?, ?, ?, ?, ?)",
                &[int(comment_id), text(&emoji), text(&actor.kind), text(&owner), text(&req.ctx.iso_now())],
            ))?;
        }
    }
    let reactions = reactions_for(req, &[comment_id])?
        .remove(&comment_id)
        .unwrap_or_default();
    Ok(json!({ "reactions": reactions, "task_id": col(&comment, "task_id") }))
}

// ── persona assignees ───────────────────────────────────────────────────────

fn assignable(req: &Request<'_>) -> Result<Vec<Row>, Outcome> {
    sql(select(
        req.conn,
        &format!(
            "SELECT p.* FROM personas p
	JOIN workspace_personas wp ON wp.persona_uuid = p.uuid
	WHERE p.archived_at IS NULL AND wp.disabled_at IS NULL AND p.owner_user_id IN (?, {LOCAL_OWNER_ID}) ORDER BY p.name"
        ),
        &[int(req.ctx.user.id)],
    ))
}

fn assigned(req: &Request<'_>, task_id: i64) -> Result<Vec<Row>, Outcome> {
    sql(select(
        req.conn,
        "SELECT p.*, a.owner_implied FROM task_persona_assignees a
		 JOIN personas p ON p.uuid = a.persona_uuid
		 WHERE a.task_id = ? ORDER BY a.created_at, p.name",
        &[int(task_id)],
    ))
}

fn resolve_persona(
    req: &Request<'_>,
    reference: &str,
    task_id: Option<i64>,
) -> Result<String, Outcome> {
    let mut pool = assignable(req)?;
    if let Some(t) = task_id {
        for row in assigned(req, t)? {
            if !pool.iter().any(|p| col(p, "uuid") == col(&row, "uuid")) {
                pool.push(row);
            }
        }
    }
    let view: Vec<Value> = pool.iter().map(persona_view).collect();
    view.iter()
        .find(|p| p["id"].as_str() == Some(reference))
        .or_else(|| view.iter().find(|p| p["name"].as_str() == Some(reference)))
        .and_then(|p| p["id"].as_str().map(str::to_string))
        .ok_or_else(|| {
            http(
                422,
                format!("Persona {reference} is not assignable in this workspace"),
            )
        })
}

fn set_task_personas(req: &Request<'_>, task_id: i64, wanted: &Value) -> Result<(), Outcome> {
    let Some(list) = wanted.as_array() else {
        return Err(http(422, "persona_assignees must be a list of persona ids"));
    };
    let current: Vec<String> = assigned(req, task_id)?
        .iter()
        .map(|r| js::string(col(r, "uuid")))
        .collect();
    let mut targets: Vec<String> = Vec::new();
    for item in list {
        let id = js::string(item);
        if !targets.contains(&id) {
            targets.push(id);
        }
    }
    let mut to_add = Vec::new();
    for id in &targets {
        if !current.contains(id) {
            to_add.push(resolve_persona(req, id, None)?);
        }
    }
    let to_remove: Vec<&String> = current.iter().filter(|c| !targets.contains(c)).collect();
    let now = req.ctx.iso_now();
    for uuid in &to_add {
        sql(execute(
            req.conn,
            "INSERT OR IGNORE INTO task_persona_assignees (task_id, persona_uuid, owner_implied, created_at)
			 VALUES (?, ?, 1, ?)",
            &[int(task_id), text(uuid), text(&now)],
        ))?;
    }
    for uuid in &to_remove {
        sql(execute(
            req.conn,
            "DELETE FROM task_persona_assignees WHERE task_id = ? AND persona_uuid = ?",
            &[int(task_id), text(uuid)],
        ))?;
    }
    if !to_add.is_empty() || !to_remove.is_empty() {
        sql(execute(
            req.conn,
            "UPDATE tasks SET updated_at = ? WHERE id = ?",
            &[text(&now), int(task_id)],
        ))?;
    }
    Ok(())
}

pub(super) fn assign_persona(req: &Request<'_>) -> Handled {
    require_human(req)?;
    let id = req.id("id");
    task_exists(req, id)?;
    let current: Vec<String> = assigned(req, id)?
        .iter()
        .map(|r| js::string(col(r, "uuid")))
        .collect();
    let uuid = resolve_persona(req, req.param("uuid"), None)?;
    if !current.contains(&uuid) {
        let mut next: Vec<Value> = current.iter().map(|c| json!(c)).collect();
        next.push(json!(uuid));
        set_task_personas(req, id, &Value::Array(next))?;
    }
    load_task(req, id)
}

pub(super) fn unassign_persona(req: &Request<'_>) -> Handled {
    require_human(req)?;
    let id = req.id("id");
    task_exists(req, id)?;
    let current: Vec<String> = assigned(req, id)?
        .iter()
        .map(|r| js::string(col(r, "uuid")))
        .collect();
    let uuid = resolve_persona(req, req.param("uuid"), Some(id))?;
    if current.contains(&uuid) {
        let next: Vec<Value> = current
            .iter()
            .filter(|c| **c != uuid)
            .map(|c| json!(c))
            .collect();
        set_task_personas(req, id, &Value::Array(next))?;
    }
    load_task(req, id)
}
