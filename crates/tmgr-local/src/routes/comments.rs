use std::collections::HashMap;

use serde_json::{json, Value};

use super::int;
use crate::context::Context;
use crate::db::{select, Row};
use crate::js;
use crate::router::{sql, Handled, Outcome, Request};
use crate::serialize::col;

pub(super) fn list(req: &Request<'_>) -> Handled {
    let id = req.id("id");
    super::tasks::require_task(req, id)?;
    let rows = sql(select(
        req.conn,
        "SELECT * FROM comments WHERE task_id = ? AND deleted_at IS NULL ORDER BY id ASC",
        &[int(id)],
    ))?;
    let ids: Vec<i64> = rows.iter().filter_map(|r| col(r, "id").as_i64()).collect();
    let mut reactions = reactions_for(req, &ids)?;
    Ok(Value::Array(
        rows.iter()
            .map(|row| {
                let mut json = comment_json(row, req.ctx);
                let id = col(row, "id").as_i64().unwrap_or(0);
                json["reactions"] = Value::Array(reactions.remove(&id).unwrap_or_default());
                json
            })
            .collect(),
    ))
}

fn comment_json(row: &Row, ctx: &Context) -> Value {
    let kind = match col(row, "author_kind") {
        Value::Null => json!("user"),
        other => other.clone(),
    };
    let author_id = match col(row, "author_id") {
        Value::Null => json!(ctx.user.id.to_string()),
        other => other.clone(),
    };
    let author_name = match col(row, "author_name") {
        Value::Null => json!(ctx.user.name),
        other => other.clone(),
    };
    let mut author = json!({ "kind": kind, "id": author_id, "name": author_name });
    if kind != "user" {
        author["owner"] = json!({ "id": ctx.user.id.to_string(), "name": ctx.user.name });
    }
    json!({
        "id": col(row, "id"),
        "task_id": col(row, "task_id"),
        "message": col(row, "message"),
        "content": col(row, "message"),
        "created_at": col(row, "created_at"),
        "updated_at": col(row, "updated_at"),
        "user_id": ctx.user.id,
        "user": { "id": ctx.user.id, "name": ctx.user.name, "email": ctx.user.email, "has_avatar": false },
        "author": author,
        "cursor_agent_id": null,
        "cursor_message_type": null,
    })
}

/// Per-comment reaction summaries; `reacted` is relative to whoever asks.
fn reactions_for(req: &Request<'_>, ids: &[i64]) -> Result<HashMap<i64, Vec<Value>>, Outcome> {
    let mut result = HashMap::new();
    if ids.is_empty() {
        return Ok(result);
    }
    let placeholders = vec!["?"; ids.len()].join(",");
    let params: Vec<_> = ids.iter().map(|id| int(*id)).collect();
    let rows = sql(select(
        req.conn,
        &format!(
            "SELECT comment_id, emoji, actor_kind, actor_id FROM comment_reactions WHERE comment_id IN ({placeholders})"
        ),
        &params,
    ))?;
    let actor = req.ctx.acting();
    let owner = actor.owner().to_string();
    let mut order: Vec<(i64, String)> = Vec::new();
    let mut entries: HashMap<(i64, String), Value> = HashMap::new();
    for row in &rows {
        let comment = col(row, "comment_id").as_i64().unwrap_or(0);
        let emoji = js::string(col(row, "emoji"));
        let key = (comment, emoji.clone());
        let entry = entries.entry(key.clone()).or_insert_with(|| {
            order.push(key);
            json!({ "emoji": emoji, "count": 0, "reacted": false, "users": [] })
        });
        entry["count"] = json!(entry["count"].as_i64().unwrap_or(0) + 1);
        let kind = col(row, "actor_kind");
        if kind.as_str() == Some(actor.kind.as_str()) && js::string(col(row, "actor_id")) == owner {
            entry["reacted"] = json!(true);
        }
        if kind.as_str() == Some("user") {
            let user = json!({ "id": js::number(col(row, "actor_id")), "name": req.ctx.user.name });
            if let Some(users) = entry["users"].as_array_mut() {
                users.push(user);
            }
        }
    }
    for key in order {
        if let Some(entry) = entries.remove(&key) {
            result.entry(key.0).or_insert_with(Vec::new).push(entry);
        }
    }
    Ok(result)
}
