use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Value};

use rusqlite::types::Value as Sql;

use crate::context::Context;
use crate::db::execute;
use crate::error::Result;
use crate::persona::check_access;
use crate::router::{normalize_path, Outcome, Query, Request};
use crate::routes;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Response {
    pub status: u16,
    pub data: Value,
}

/// One request against the local API, answered with the JSON the desktop's local API returns.
/// `Ok(None)` when no local route matches: the caller decides what that means, it never goes to a server.
/// A write runs in one `BEGIN IMMEDIATE` transaction: a refusal or an error leaves nothing behind but
/// its journal row, so a desktop statement can only land between whole requests.
pub fn dispatch(
    conn: &Connection,
    ctx: &Context,
    method: &str,
    url: &str,
    body: Option<&Value>,
) -> Result<Option<Response>> {
    let method = method.to_ascii_uppercase();
    let path = normalize_path(url);
    let Some((route, params)) = routes::table()
        .iter()
        .filter(|r| r.method == method)
        .find_map(|r| r.matches(&path).map(|p| (r, p)))
    else {
        return Ok(None);
    };
    let req = Request {
        conn,
        ctx,
        params,
        query: Query::parse(url.split_once('?').map_or("", |(_, q)| q)),
        body: parse_body(body),
    };
    let tx = if method == "GET" {
        None
    } else {
        Some(Tx::begin(conn)?)
    };
    let result =
        check_access(conn, ctx, &method, route.pattern).and_then(|()| (route.handler)(&req));
    match result {
        Ok(data) => {
            if journal_worthy(ctx, &method, route.status) {
                let entity_id = entity_id(&req, &method, Some(&data));
                if journal(&req, &method, route.pattern, entity_id, route.status).is_err() {
                    return Ok(Some(Response {
                        status: 500,
                        data: json!({ "message": "Audit failed" }),
                    }));
                }
            }
            if let Some(tx) = tx {
                tx.commit()?;
            }
            Ok(Some(Response {
                status: route.status,
                data: envelope(data),
            }))
        }
        Err(Outcome::Http { status, body }) => {
            if let Some(tx) = &tx {
                tx.undo_request()?;
            }
            if journal_worthy(ctx, &method, status) {
                let entity_id = entity_id(&req, &method, None);
                let _ = journal(&req, &method, route.pattern, entity_id, status);
            }
            if let Some(tx) = tx {
                tx.commit()?;
            }
            Ok(Some(Response { status, data: body }))
        }
        Err(Outcome::Failed(e)) => Err(e),
        Err(Outcome::Unrouted) => Ok(None),
    }
}

/// `BEGIN IMMEDIATE` with a savepoint around the handler; dropped without `commit` it rolls everything back.
struct Tx<'a> {
    conn: &'a Connection,
    open: bool,
}

impl<'a> Tx<'a> {
    fn begin(conn: &'a Connection) -> Result<Self> {
        conn.execute_batch("BEGIN IMMEDIATE")?;
        let tx = Tx { conn, open: true };
        conn.execute_batch("SAVEPOINT request")?;
        Ok(tx)
    }

    fn undo_request(&self) -> Result<()> {
        Ok(self
            .conn
            .execute_batch("ROLLBACK TO request; RELEASE request")?)
    }

    fn commit(mut self) -> Result<()> {
        self.conn.execute_batch("COMMIT")?;
        self.open = false;
        Ok(())
    }
}

impl Drop for Tx<'_> {
    fn drop(&mut self) {
        if self.open {
            let _ = self.conn.execute_batch("ROLLBACK");
        }
    }
}

fn parse_body(body: Option<&Value>) -> Value {
    match body {
        None | Some(Value::Null) => json!({}),
        Some(Value::String(s)) => serde_json::from_str(s).unwrap_or_else(|_| json!({})),
        Some(v) => v.clone(),
    }
}

/// Journaled: any non-GET by a persona or plugin, and a persona refusal (401/403) even on a GET.
fn journal_worthy(ctx: &Context, method: &str, status: u16) -> bool {
    match ctx.actor.as_ref().map(|a| a.kind.as_str()) {
        Some("persona") => method != "GET" || status == 401 || status == 403,
        Some("plugin") => method != "GET",
        _ => false,
    }
}

/// The first numeric path parameter, or the id a create answered with.
fn entity_id(req: &Request<'_>, method: &str, data: Option<&Value>) -> Option<String> {
    let from_path = req
        .params
        .iter()
        .find(|(_, v)| !v.is_empty() && v.bytes().all(|b| b.is_ascii_digit()))
        .map(|(_, v)| v.clone());
    if from_path.is_some() || method != "POST" {
        return from_path;
    }
    match data?.get("id")? {
        Value::Null => None,
        id => Some(crate::js::string(id)),
    }
}

fn journal(
    req: &Request<'_>,
    method: &str,
    pattern: &str,
    entity_id: Option<String>,
    status: u16,
) -> Result<()> {
    let Some(actor) = &req.ctx.actor else {
        return Ok(());
    };
    let entity = pattern.split('/').next().filter(|e| !e.is_empty());
    let opt = |v: Option<&str>| v.map_or(Sql::Null, |s| Sql::Text(s.to_string()));
    execute(
        req.conn,
        "INSERT INTO activity_log (at, actor_kind, actor_id, actor_name, method, route, entity, entity_id, status)
		 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        &[
            Sql::Text(req.ctx.iso_now()),
            Sql::Text(actor.kind.clone()),
            Sql::Text(actor.id.clone()),
            Sql::Text(actor.name.clone()),
            Sql::Text(method.to_string()),
            Sql::Text(pattern.to_string()),
            opt(entity),
            opt(entity_id.as_deref()),
            Sql::Integer(status as i64),
        ],
    )?;
    Ok(())
}

/// Same rule as the Java envelope filter: wrap unless the body is an object that already carries `data`.
fn envelope(data: Value) -> Value {
    match &data {
        Value::Object(map) if map.contains_key("data") => data,
        _ => json!({ "data": data }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Actor;
    use crate::testing::{ctx, db};

    #[test]
    fn unknown_routes_and_writes_are_not_answered() {
        let conn = db("");
        let c = ctx("2026-10-13T12:00:00.000Z");
        assert_eq!(dispatch(&conn, &c, "GET", "nope", None).unwrap(), None);
        assert_eq!(dispatch(&conn, &c, "POST", "tasks", None).unwrap(), None);
    }

    #[test]
    fn huge_page_numbers_answer_an_empty_page_instead_of_panicking() {
        let conn = db(
            "INSERT INTO categories (id, title, created_at, updated_at) VALUES (1, 'P', 'x', 'x');",
        );
        let c = ctx("2026-10-13T12:00:00.000Z");
        for url in [
            "project_categories/children/1?page=500000000000000000",
            "project_categories/children/1?per_page=1e300&page=1e300",
            "tasks?page=500000000000000000&per_page=900000000000000000",
            "tasks/status/1?page=1e400",
        ] {
            let r = dispatch(&conn, &c, "GET", url, None).unwrap().unwrap();
            assert_eq!(r.status, 200, "{url}");
            assert_eq!(r.data["data"], json!([]), "{url}");
        }
    }

    #[test]
    fn wraps_bodies_without_data_and_passes_pages_through() {
        let conn = db("");
        let c = ctx("2026-10-13T12:00:00.000Z");
        let statuses = dispatch(&conn, &c, "get", "/api/workspaces/statuses", None)
            .unwrap()
            .unwrap();
        assert_eq!(statuses.data["data"].as_array().unwrap().len(), 4);
        let page = dispatch(&conn, &c, "GET", "tasks", None).unwrap().unwrap();
        assert_eq!(page.data["meta"]["total"], 0);
    }

    #[test]
    fn an_unknown_persona_is_refused_and_journaled() {
        let conn = db("");
        conn.execute_batch("PRAGMA query_only = OFF").unwrap();
        let mut c = ctx("2026-10-13T12:00:00.000Z");
        c.actor = Some(Actor {
            kind: "persona".into(),
            id: "p1".into(),
            name: "P".into(),
            owner_id: None,
        });
        let r = dispatch(&conn, &c, "GET", "tasks", None).unwrap().unwrap();
        assert_eq!(r.status, 401);
        assert_eq!(r.data["code"], "PERSONA_UNKNOWN");
        let logged: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM activity_log WHERE status = 401 AND route = 'tasks'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(logged, 1);
    }
}
