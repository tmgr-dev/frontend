use std::collections::HashMap;

use serde_json::{json, Value};

use super::{int, text};
use crate::context::Context;
use crate::db::{select, select_one, Row};
use crate::js;
use crate::markdown::sections;
use crate::router::{sql, Handled, Outcome, Request};
use crate::serialize::col;

const SUMMARY_COLUMNS: &str = "id, parent_id, title, slug, type, '' AS body, '{}' AS properties, version,
	author_id, author_kind, author_ref, updated_by_id, updated_by_kind, updated_by_ref, position, pinned,
	created_at, updated_at, deleted_at";

pub(crate) fn page_not_found() -> Outcome {
    Outcome::Http {
        status: 404,
        body: json!({ "message": "Page not found", "error": "page_not_found" }),
    }
}

fn summary_json(row: &Row) -> Value {
    json!({
        "id": col(row, "id"),
        "title": col(row, "title"),
        "slug": col(row, "slug"),
        "type": col(row, "type"),
        "parent_id": col(row, "parent_id"),
        "position": col(row, "position"),
        "pinned": js::truthy(col(row, "pinned")),
        "updated_at": col(row, "updated_at"),
    })
}

pub(super) fn tree(req: &Request<'_>) -> Handled {
    let rows = sql(select(
        req.conn,
        &format!(
            "SELECT {SUMMARY_COLUMNS} FROM pages WHERE deleted_at IS NULL
			 ORDER BY COALESCE(parent_id, 0), pinned DESC, position ASC, id ASC"
        ),
        &[],
    ))?;
    Ok(Value::Array(rows.iter().map(summary_json).collect()))
}

pub(crate) fn resolve(req: &Request<'_>, reference: &str) -> Result<Row, Outcome> {
    lookup(req, reference, false)?.ok_or_else(page_not_found)
}

/// A page by id or slug, optionally including deleted ones.
pub(crate) fn lookup(
    req: &Request<'_>,
    reference: &str,
    include_deleted: bool,
) -> Result<Option<Row>, Outcome> {
    let trimmed = reference.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let live = if include_deleted {
        ""
    } else {
        " AND deleted_at IS NULL"
    };
    Ok(if trimmed.bytes().all(|b| b.is_ascii_digit()) {
        let id = js::parse_number(trimmed);
        if id > 9_007_199_254_740_991.0 {
            return Ok(None);
        }
        sql(select_one(
            req.conn,
            &format!("SELECT * FROM pages WHERE id = ?{live}"),
            &[int(id as i64)],
        ))?
    } else {
        sql(select_one(
            req.conn,
            &format!("SELECT * FROM pages WHERE slug = ?{live}"),
            &[text(&trimmed.to_lowercase())],
        ))?
    })
}

fn persona_names(req: &Request<'_>, row: &Row) -> Result<HashMap<String, Value>, Outcome> {
    names_for(
        req,
        &[
            (col(row, "author_kind"), col(row, "author_ref")),
            (col(row, "updated_by_kind"), col(row, "updated_by_ref")),
        ],
    )
}

fn names_for(
    req: &Request<'_>,
    refs: &[(&Value, &Value)],
) -> Result<HashMap<String, Value>, Outcome> {
    let mut uuids: Vec<String> = Vec::new();
    for (kind, reference) in refs {
        let r = js::string(reference);
        if kind.as_str() == Some("persona") && !uuids.contains(&r) {
            uuids.push(r);
        }
    }
    if uuids.is_empty() {
        return Ok(HashMap::new());
    }
    let marks = vec!["?"; uuids.len()].join(",");
    let params: Vec<_> = uuids.iter().map(|u| text(u)).collect();
    let rows = sql(select(
        req.conn,
        &format!("SELECT uuid, name FROM personas WHERE uuid IN ({marks})"),
        &params,
    ))?;
    Ok(rows
        .iter()
        .map(|r| (js::string(col(r, "uuid")), col(r, "name").clone()))
        .collect())
}

fn author_json(
    ctx: &Context,
    kind: &Value,
    reference: &Value,
    names: &HashMap<String, Value>,
) -> Value {
    let kind_str = js::string(kind);
    let ref_str = js::string(reference);
    if kind_str == "user" {
        let n = js::number(reference);
        let id = if n.is_null() { reference.clone() } else { n };
        return json!({ "kind": "user", "id": id, "name": ctx.user.name });
    }
    let known = if kind_str == "persona" {
        names.get(&ref_str).filter(|n| !n.is_null()).cloned()
    } else {
        None
    };
    let name = known.unwrap_or_else(|| match &ctx.actor {
        Some(a) if a.kind == kind_str && a.id == ref_str => json!(a.name),
        _ => reference.clone(),
    });
    json!({
        "kind": kind,
        "id": reference,
        "name": name,
        "owner": { "id": ctx.user.id.to_string(), "name": ctx.user.name },
    })
}

fn file_json(row: &Row, ctx: &Context) -> Value {
    json!({
        "id": col(row, "id"),
        "page_id": col(row, "page_id"),
        "user_id": ctx.user.id,
        "workspace_id": ctx.workspace.manifest.id,
        "name": col(row, "name"),
        "original_name": col(row, "name"),
        "file_path": col(row, "file_path"),
        "mime_type": col(row, "mime_type"),
        "size": col(row, "size"),
        "created_at": col(row, "created_at"),
    })
}

pub(super) fn show(req: &Request<'_>) -> Handled {
    let row = resolve(req, req.param("id"))?;
    assemble(req, &row)
}

/// The full page payload the TS `assemble` returns.
pub(crate) fn assemble(req: &Request<'_>, row: &Row) -> Handled {
    let row = row.clone();
    let ctx = req.ctx;
    let id = col(&row, "id").as_i64().unwrap_or(0);
    let names = persona_names(req, &row)?;
    let backlinks = sql(select(
        req.conn,
        "SELECT p.* FROM page_links l JOIN pages p ON p.id = l.page_id
		 WHERE l.target_kind = ? AND l.target_id = ? AND p.deleted_at IS NULL
		 ORDER BY p.updated_at DESC, p.id DESC",
        &[text("page"), int(id)],
    ))?;
    let files = sql(select(
        req.conn,
        "SELECT * FROM files WHERE page_id = ? ORDER BY id",
        &[int(id)],
    ))?;
    let body = js::string(col(&row, "body"));
    let mut page = summary_json(&row);
    let extra = json!({
        "workspace_id": ctx.workspace.manifest.id,
        "body": col(&row, "body"),
        "properties": js::parse_json(col(&row, "properties"), json!({})),
        "version": col(&row, "version"),
        "author": author_json(ctx, col(&row, "author_kind"), col(&row, "author_ref"), &names),
        "updated_by": author_json(ctx, col(&row, "updated_by_kind"), col(&row, "updated_by_ref"), &names),
        "created_at": col(&row, "created_at"),
        "deleted_at": col(&row, "deleted_at"),
        "backlinks": backlinks.iter().map(summary_json).collect::<Vec<_>>(),
        "sections": sections(&body)
            .into_iter()
            .map(|s| json!({ "id": s.id, "owner": s.owner, "heading": s.heading }))
            .collect::<Vec<_>>(),
        "files": files.iter().map(|f| file_json(f, ctx)).collect::<Vec<_>>(),
        "following": false,
    });
    if let (Value::Object(page), Value::Object(extra)) = (&mut page, extra) {
        page.extend(extra);
    }
    Ok(page)
}

fn version_json(row: &Row, ctx: &Context, names: &HashMap<String, Value>) -> Value {
    json!({
        "version": col(row, "version"),
        "title": col(row, "title"),
        "author": author_json(ctx, col(row, "author_kind"), col(row, "author_ref"), names),
        "summary": col(row, "summary"),
        "created_at": col(row, "created_at"),
    })
}

pub(super) fn versions(req: &Request<'_>) -> Handled {
    let page = resolve(req, req.param("id"))?;
    let rows = sql(select(
        req.conn,
        "SELECT page_id, version, title, '' AS body, '{}' AS properties, author_id, author_kind, author_ref, summary, created_at
		 FROM page_versions WHERE page_id = ? ORDER BY version DESC",
        &[col(&page, "id").as_i64().map_or(rusqlite::types::Value::Null, int)],
    ))?;
    let refs: Vec<(&Value, &Value)> = rows
        .iter()
        .map(|r| (col(r, "author_kind"), col(r, "author_ref")))
        .collect();
    let names = names_for(req, &refs)?;
    Ok(Value::Array(
        rows.iter()
            .map(|r| version_json(r, req.ctx, &names))
            .collect(),
    ))
}

pub(super) fn version(req: &Request<'_>) -> Handled {
    let page = resolve(req, req.param("id"))?;
    let row = sql(select_one(
        req.conn,
        "SELECT * FROM page_versions WHERE page_id = ? AND version = ?",
        &[
            col(&page, "id")
                .as_i64()
                .map_or(rusqlite::types::Value::Null, int),
            int(req.id("version")),
        ],
    ))?
    .ok_or_else(|| Outcome::Http {
        status: 404,
        body: json!({ "message": "Version not found", "error": "version_not_found" }),
    })?;
    let names = names_for(req, &[(col(&row, "author_kind"), col(&row, "author_ref"))])?;
    let mut out = version_json(&row, req.ctx, &names);
    out["body"] = col(&row, "body").clone();
    out["properties"] = js::parse_json(col(&row, "properties"), json!({}));
    Ok(out)
}

pub(super) fn backlinks(req: &Request<'_>) -> Handled {
    let page = resolve(req, req.param("id"))?;
    let rows = sql(select(
        req.conn,
        "SELECT p.* FROM page_links l JOIN pages p ON p.id = l.page_id
		 WHERE l.target_kind = ? AND l.target_id = ? AND p.deleted_at IS NULL
		 ORDER BY p.updated_at DESC, p.id DESC",
        &[
            text("page"),
            col(&page, "id")
                .as_i64()
                .map_or(rusqlite::types::Value::Null, int),
        ],
    ))?;
    Ok(Value::Array(rows.iter().map(summary_json).collect()))
}
