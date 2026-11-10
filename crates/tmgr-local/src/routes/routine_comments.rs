use serde_json::{json, Value};

use super::comments::comment_json;
use super::int;
use super::task_writes::{message_of, own_comment_check};
use super::text;
use crate::db::{execute, schema_version, select, select_one, Row};
use crate::router::{http, not_found, sql, Handled, Outcome, Request};
use crate::serialize::col;

const ROUTINE_ID_BASE: i64 = 1_000_000_000;

pub(super) fn applies(req: &Request<'_>, id: i64) -> Result<bool, Outcome> {
    Ok(id > ROUTINE_ID_BASE && sql(schema_version(req.conn))? >= 14)
}

fn json_of(req: &Request<'_>, row: &Row) -> Value {
    let mut shaped = row.clone();
    shaped.insert("task_id".into(), col(row, "routine_id").clone());
    let mut json = comment_json(&shaped, req.ctx);
    json["reactions"] = json!([]);
    json
}

fn load(req: &Request<'_>, id: i64) -> Result<Option<Row>, Outcome> {
    sql(select_one(
        req.conn,
        "SELECT * FROM routine_comments WHERE id = ?",
        &[int(id)],
    ))
}

fn require_routine(req: &Request<'_>, id: i64) -> Result<(), Outcome> {
    sql(select_one(
        req.conn,
        "SELECT id FROM routines WHERE id = ? AND deleted_at IS NULL",
        &[int(id)],
    ))?
    .map(|_| ())
    .ok_or_else(|| http(404, "Routine task not found"))
}

pub(super) fn list(req: &Request<'_>, id: i64) -> Handled {
    require_routine(req, id)?;
    let rows = sql(select(
        req.conn,
        "SELECT * FROM routine_comments WHERE routine_id = ? AND deleted_at IS NULL ORDER BY id ASC",
        &[int(id)],
    ))?;
    Ok(Value::Array(rows.iter().map(|r| json_of(req, r)).collect()))
}

pub(super) fn add(req: &Request<'_>, routine_id: i64) -> Handled {
    let message = message_of(req)?;
    require_routine(req, routine_id)?;
    let actor = req.ctx.acting();
    let now = req.ctx.iso_now();
    let inserted = sql(execute(
        req.conn,
        "INSERT INTO routine_comments (id, routine_id, message, author_kind, author_id, author_name, created_at, updated_at)
		 SELECT COALESCE(MAX(id), ?) + 1, ?, ?, ?, ?, ?, ?, ? FROM routine_comments",
        &[
            int(ROUTINE_ID_BASE),
            int(routine_id),
            text(&message),
            text(&actor.kind),
            text(&actor.id),
            text(&actor.name),
            text(&now),
            text(&now),
        ],
    ))?;
    let row = load(req, inserted.last_id)?.unwrap_or_default();
    Ok(json_of(req, &row))
}

pub(super) fn edit(req: &Request<'_>, id: i64) -> Handled {
    let existing = load(req, id)?
        .filter(|c| col(c, "deleted_at").is_null())
        .ok_or_else(|| not_found("Comment"))?;
    own_comment_check(req, &existing, "edit")?;
    let message = message_of(req)?;
    sql(execute(
        req.conn,
        "UPDATE routine_comments SET message = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
        &[text(&message), text(&req.ctx.iso_now()), int(id)],
    ))?;
    let row = load(req, id)?.unwrap_or_default();
    Ok(json_of(req, &row))
}

pub(super) fn delete(req: &Request<'_>, id: i64) -> Handled {
    let existing = load(req, id)?;
    if req.ctx.actor.as_ref().is_some_and(|a| a.kind == "persona") && existing.is_none() {
        return Err(not_found("Comment"));
    }
    if let Some(found) = &existing {
        own_comment_check(req, found, "delete")?;
    }
    sql(execute(
        req.conn,
        "UPDATE routine_comments SET deleted_at = ? WHERE id = ? AND deleted_at IS NULL",
        &[text(&req.ctx.iso_now()), int(id)],
    ))?;
    Ok(match existing {
        Some(c) => json!({ "success": true, "task_id": col(&c, "routine_id") }),
        None => json!({ "success": true }),
    })
}

pub(super) fn toggle() -> Handled {
    Err(http(422, "Reactions are not supported on routine comments"))
}
