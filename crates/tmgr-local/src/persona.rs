use crate::context::Context;
use crate::db::select_one;
use crate::router::{coded, sql, Outcome};
use crate::routes::text;
use crate::serialize::col;

pub(crate) const LOCAL_OWNER_ID: i64 = -1;

/// Closed by default: a route missing here is a 403 for a persona, like the TS `PERSONA_WHITELIST`.
const WHITELIST: &[(&str, &str, &str)] = &[
    ("GET", "tasks", "tasks:read"),
    ("POST", "tasks", "tasks:write"),
    ("GET", "tasks/:id(\\d+)", "tasks:read"),
    ("PATCH", "tasks/:id(\\d+)", "tasks:write"),
    ("GET", "tasks/:id(\\d+)/comments", "comments:read"),
    ("POST", "tasks/:id(\\d+)/comments", "comments:write"),
    ("PUT", "comments/:id(\\d+)", "comments:write"),
    ("DELETE", "comments/:id(\\d+)", "comments:write"),
    (
        "POST",
        "comments/:id(\\d+)/reactions/toggle",
        "comments:write",
    ),
    ("GET", "tasks/:id(\\d+)/files", "files:attachments"),
    ("GET", "files/:id(\\d+)", "files:attachments"),
    ("GET", "files/:id(\\d+)/content", "files:attachments"),
    ("GET", "workspaces/statuses", "statuses:read"),
    ("GET", "project_categories", "categories:read"),
    ("GET", "project_categories/:id(\\d+)", "categories:read"),
    ("GET", "task-relation-types", "relations:read"),
    ("GET", "tasks/:id(\\d+)/relations", "relations:read"),
    (
        "POST",
        "tasks/:id(\\d+)/related-to/:otherId(\\d+)/with/:typeId(\\d+)",
        "relations:write",
    ),
    ("GET", "tasks/:id(\\d+)/agent-work", "agent_work:read"),
    ("POST", "tasks/:id(\\d+)/agent-work", "agent_work:write"),
    ("PATCH", "agent-work/:id(\\d+)", "agent_work:write"),
    ("POST", "agent-work/:id(\\d+)/finish", "agent_work:write"),
    ("GET", "workspaces/context", "pages:read"),
    ("GET", "pages", "pages:read"),
    ("GET", "pages/tree", "pages:read"),
    ("GET", "pages/search", "pages:read"),
    ("GET", "pages/:id", "pages:read"),
    ("GET", "pages/:id/backlinks", "pages:read"),
    ("GET", "pages/:id/versions", "pages:read"),
    ("GET", "pages/:id/versions/:version(\\d+)", "pages:read"),
    ("GET", "pages/:id/files", "pages:read"),
    ("GET", "tasks/:id(\\d+)/pages", "pages:read"),
    ("POST", "pages", "pages:write"),
    ("PATCH", "pages/:id", "pages:write"),
    ("POST", "pages/:id/append", "pages:write"),
    ("PUT", "pages/:id/sections/:sectionId", "pages:write"),
    ("POST", "pages/:id/files", "pages:write"),
];

/// Identity first (401), so an offline disable or archive always wins over a 403, then the whitelist.
pub(crate) fn check_access(
    conn: &rusqlite::Connection,
    ctx: &Context,
    method: &str,
    pattern: &str,
) -> Result<(), Outcome> {
    let Some(actor) = ctx.actor.as_ref().filter(|a| a.kind == "persona") else {
        return Ok(());
    };
    let persona = sql(select_one(
        conn,
        "SELECT owner_user_id, archived_at FROM personas WHERE uuid = ?",
        &[text(&actor.id)],
    ))?
    .ok_or_else(|| {
        coded(
            401,
            "Persona is not known in this workspace",
            "PERSONA_UNKNOWN",
        )
    })?;
    if !col(&persona, "archived_at").is_null() {
        return Err(coded(401, "Persona is archived", "PERSONA_ARCHIVED"));
    }
    let owner = crate::js::number(col(&persona, "owner_user_id"));
    if owner != LOCAL_OWNER_ID && owner != ctx.user.id {
        return Err(coded(
            401,
            "Persona belongs to a different account",
            "OWNER_MISMATCH",
        ));
    }
    let grant = sql(select_one(
        conn,
        "SELECT permissions, disabled_at FROM workspace_personas WHERE persona_uuid = ?",
        &[text(&actor.id)],
    ))?
    .ok_or_else(|| {
        coded(
            401,
            "Persona is not enabled in this workspace",
            "PERSONA_DISABLED",
        )
    })?;
    if !col(&grant, "disabled_at").is_null() {
        return Err(coded(
            401,
            "Persona is disabled in this workspace",
            "PERSONA_DISABLED",
        ));
    }
    let Some((_, _, permission)) = WHITELIST
        .iter()
        .find(|(m, p, _)| *m == method && *p == pattern)
    else {
        return Err(coded(
            403,
            "This route is not available to personas",
            "ROUTE_NOT_ALLOWED",
        ));
    };
    let granted: Vec<String> = col(&grant, "permissions")
        .as_str()
        .filter(|s| !s.is_empty())
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or_default();
    if !granted.iter().any(|g| g == permission) {
        return Err(coded(
            403,
            &format!("Persona is missing permission {permission}"),
            "PERMISSION_MISSING",
        ));
    }
    Ok(())
}
