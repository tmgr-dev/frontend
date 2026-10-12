use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Value};

use crate::context::Context;
use crate::error::{Error, Result};
use crate::router::{normalize_path, Outcome, Query, Request};
use crate::routes;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Response {
    pub status: u16,
    pub data: Value,
}

/// One request against the local API, answered with the JSON the desktop's local API returns.
/// `Ok(None)` when no local route matches: the caller decides what that means, it never goes to a server.
pub fn dispatch(
    conn: &Connection,
    ctx: &Context,
    method: &str,
    url: &str,
    _body: Option<&Value>,
) -> Result<Option<Response>> {
    if ctx.actor.as_ref().is_some_and(|a| a.kind == "persona") {
        return Err(Error::Unsupported(
            "persona actors are not supported by the local API in tmgr yet".into(),
        ));
    }
    let method = method.to_ascii_uppercase();
    let path = normalize_path(url);
    let Some((route, params)) = routes::table()
        .iter()
        .filter(|r| r.method == method)
        .find_map(|r| r.matches(&path).map(|p| (r, p)))
    else {
        return Ok(None);
    };
    let query = Query::parse(url.split_once('?').map_or("", |(_, q)| q));
    let req = Request {
        conn,
        ctx,
        params,
        query,
    };
    match (route.handler)(&req) {
        Ok(data) => Ok(Some(Response {
            status: route.status,
            data: envelope(data),
        })),
        Err(Outcome::Http { status, body }) => Ok(Some(Response { status, data: body })),
        Err(Outcome::Failed(e)) => Err(e),
    }
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
    fn refuses_persona_actors() {
        let conn = db("");
        let mut c = ctx("2026-10-13T12:00:00.000Z");
        c.actor = Some(Actor {
            kind: "persona".into(),
            id: "p1".into(),
            name: "P".into(),
            owner_id: None,
        });
        assert!(matches!(
            dispatch(&conn, &c, "GET", "tasks", None),
            Err(Error::Unsupported(_))
        ));
    }
}
