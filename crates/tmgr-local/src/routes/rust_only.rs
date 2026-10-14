//! Routes the CLI needs that the desktop's TS local API does not have; shaped like the Java API.

use chrono::{Days, NaiveDate};
use serde_json::{json, Value};

use super::{int, text};
use crate::db::select;
use crate::router::{http, not_found, sql, Handled, Request};
use crate::serialize::{col, task_key};
use crate::workspace::list_in;

const MAX_DAYS: i64 = 366;

pub(super) fn workspaces(req: &Request<'_>) -> Handled {
    let current = &req.ctx.workspace;
    let mut list = current.path.parent().map(list_in).unwrap_or_default();
    if !list
        .iter()
        .any(|w| w.manifest.code == current.manifest.code)
    {
        list.push(current.clone());
    }
    Ok(Value::Array(
        list.iter()
            .map(|w| {
                json!({
                    "id": if w.manifest.code == current.manifest.code { current.manifest.id } else { w.manifest.id },
                    "name": w.manifest.name,
                    "code": w.manifest.code,
                    "local": true,
                    "is_default": false,
                    "schema_version": w.manifest.schema_version,
                    "created_at": w.manifest.created_at,
                })
            })
            .collect(),
    ))
}

fn parse_key(raw: &str) -> Option<(String, String, i64)> {
    let key = raw.trim();
    let (code, digits) = key.split_once('-')?;
    let mut chars = code.chars();
    let first_ok = chars.next().is_some_and(|c| c.is_ascii_alphabetic());
    if !first_ok
        || !chars.all(|c| c.is_ascii_alphanumeric())
        || digits.is_empty()
        || digits.len() > 18
        || !digits.bytes().all(|b| b.is_ascii_digit())
    {
        return None;
    }
    Some((
        key.to_ascii_uppercase(),
        code.to_ascii_uppercase(),
        digits.parse().ok()?,
    ))
}

/// The newest live task whose visible key is `key`: title prefix first, category code and ticket number second.
pub(super) fn task_by_key(req: &Request<'_>) -> Handled {
    let Some((wanted, code, sequence)) = parse_key(req.param("key")) else {
        return Err(not_found("Task"));
    };
    let by_prefix = sql(select(
        req.conn,
        "SELECT t.id, t.title, c.code, t.category_tasks_sequence_id AS seq FROM tasks t
		 LEFT JOIN categories c ON c.id = t.project_category_id
		 WHERE t.deleted_at IS NULL AND (LTRIM(t.title) LIKE ? OR LTRIM(t.title) LIKE ?)
		 ORDER BY t.created_at DESC, t.id DESC LIMIT 50",
        &[text(&format!("{wanted}:%")), text(&format!("{wanted} %"))],
    ))?;
    let title_of = |r: &crate::db::Row| col(r, "title").as_str().unwrap_or("").to_string();
    let id_of = |r: &crate::db::Row| col(r, "id").as_i64().unwrap_or(0);
    let found = by_prefix
        .iter()
        .find(|r| task_key(&title_of(r), None, None, id_of(r)).eq_ignore_ascii_case(&wanted))
        .map(id_of);
    let found = match found {
        Some(id) => Some(id),
        None => sql(select(
            req.conn,
            "SELECT t.id, t.title, c.code, t.category_tasks_sequence_id AS seq FROM tasks t
			 JOIN categories c ON c.id = t.project_category_id
			 WHERE t.deleted_at IS NULL AND UPPER(c.code) = ? AND t.category_tasks_sequence_id = ?
			 ORDER BY t.created_at DESC, t.id DESC LIMIT 50",
            &[text(&code), int(sequence)],
        ))?
        .iter()
        .find(|r| {
            task_key(
                &title_of(r),
                col(r, "code").as_str(),
                col(r, "seq").as_i64(),
                id_of(r),
            )
            .eq_ignore_ascii_case(&wanted)
        })
        .map(id_of),
    };
    match found {
        Some(id) => super::tasks::load_task(req, id),
        None => Err(not_found("Task")),
    }
}

fn date(raw: Option<&str>, fallback: NaiveDate) -> Result<NaiveDate, crate::router::Outcome> {
    match raw.map(str::trim).filter(|s| !s.is_empty()) {
        None => Ok(fallback),
        Some(v) => NaiveDate::parse_from_str(v, "%Y-%m-%d")
            .map_err(|_| http(400, "dates must be YYYY-MM-DD")),
    }
}

/// Tracked time per task of the tasks last updated in `[from, to]` (UTC days), like the Java time summary.
pub(super) fn time_summary(req: &Request<'_>) -> Handled {
    let q = &req.query;
    if q.get("group_by")
        .is_some_and(|g| !g.eq_ignore_ascii_case("task"))
    {
        return Err(http(400, "group_by must be task"));
    }
    let end = date(q.get("to"), req.ctx.now.date_naive())?;
    let start = date(
        q.get("from"),
        end.checked_sub_days(Days::new(7)).unwrap_or(end),
    )?;
    if start > end || (end - start).num_days() > MAX_DAYS {
        return Err(http(
            400,
            format!("from must not be after to, and the range is at most {MAX_DAYS} days"),
        ));
    }
    let lower = start.format("%Y-%m-%d").to_string();
    let upper = end
        .checked_add_days(Days::new(1))
        .unwrap_or(end)
        .format("%Y-%m-%d")
        .to_string();
    let range =
        "julianday(t.updated_at) >= julianday(?) AND julianday(t.updated_at) < julianday(?)";
    let total = sql(select(
        req.conn,
        &format!(
            "SELECT COALESCE(SUM(t.common_time), 0) AS total FROM tasks t WHERE t.deleted_at IS NULL AND {range}"
        ),
        &[text(&lower), text(&upper)],
    ))?
    .first()
    .and_then(|r| col(r, "total").as_i64())
    .unwrap_or(0);
    let rows = sql(select(
        req.conn,
        &format!(
            "SELECT t.id, t.title, c.code, t.category_tasks_sequence_id AS seq, COALESCE(t.common_time, 0) AS tracked
			 FROM tasks t LEFT JOIN categories c ON c.id = t.project_category_id
			 WHERE t.deleted_at IS NULL AND {range} AND COALESCE(t.common_time, 0) > 0
			 ORDER BY tracked DESC, t.id ASC LIMIT 100"
        ),
        &[text(&lower), text(&upper)],
    ))?;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let id = col(r, "id").as_i64().unwrap_or(0);
            let title = col(r, "title").as_str().unwrap_or("");
            json!({
                "task_id": id,
                "key": task_key(title, col(r, "code").as_str(), col(r, "seq").as_i64(), id),
                "title": title,
                "seconds": col(r, "tracked"),
            })
        })
        .collect();
    Ok(json!({
        "from": lower,
        "to": end.format("%Y-%m-%d").to_string(),
        "total_s": total,
        "items": items,
    }))
}

#[cfg(test)]
mod tests {
    use super::parse_key;
    use crate::testing::{ctx, db, get};
    use serde_json::json;

    const SEED: &str = "
        INSERT INTO categories (id, title, code, created_at, updated_at) VALUES (1, 'Work', 'WK', 'x', 'x');
        INSERT INTO tasks (id, title, project_category_id, category_tasks_sequence_id, common_time, created_at, updated_at)
        VALUES
          (1, 'TM-5: prefixed', 1, 1, 600, '2026-10-01T10:00:00.000Z', '2026-10-10T10:00:00.000Z'),
          (2, 'plain', 1, 2, 1200, '2026-10-02T10:00:00.000Z', '2026-10-12T23:59:59.000Z'),
          (3, 'old', NULL, NULL, 50, '2026-01-01T10:00:00.000Z', '2026-01-01T10:00:00.000Z'),
          (4, 'untracked', NULL, NULL, 0, '2026-10-03T10:00:00.000Z', '2026-10-11T10:00:00.000Z');
        INSERT INTO tasks (id, title, created_at, updated_at, deleted_at)
        VALUES (5, 'TM-5: deleted', '2026-10-05T10:00:00.000Z', '2026-10-05T10:00:00.000Z', '2026-10-06T00:00:00.000Z');
    ";

    #[test]
    fn keys_follow_the_java_grammar() {
        assert_eq!(
            parse_key(" tm-12 "),
            Some(("TM-12".into(), "TM".into(), 12))
        );
        assert!(parse_key("1A-2").is_none());
        assert!(parse_key("TM-").is_none());
        assert!(parse_key("T-M-1").is_none());
        assert!(parse_key(&format!("TM-{}", "9".repeat(19))).is_none());
    }

    #[test]
    fn finds_tasks_by_title_prefix_then_category_sequence() {
        let conn = db(SEED);
        let c = ctx("2026-10-13T12:00:00.000Z");
        let (status, body) = get(&conn, &c, "tasks/by-key/tm-5");
        assert_eq!(status, 200);
        assert_eq!(body["data"]["id"], 1);
        assert_eq!(body["data"]["workspace_id"], -42);
        assert_eq!(get(&conn, &c, "tasks/by-key/WK-2").1["data"]["id"], 2);
        assert_eq!(get(&conn, &c, "tasks/by-key/WK-1").0, 404);
        assert_eq!(get(&conn, &c, "tasks/by-key/nope").0, 404);
    }

    #[test]
    fn a_lowercase_category_code_still_resolves_its_key() {
        let conn = db("
            INSERT INTO categories (id, title, code, created_at, updated_at) VALUES (2, 'Backend', 'be', 'x', 'x');
            INSERT INTO tasks (id, title, project_category_id, category_tasks_sequence_id, created_at, updated_at)
            VALUES (7, 'Lower', 2, 1, 'x', 'x');
        ");
        let c = ctx("2026-10-13T12:00:00.000Z");
        assert_eq!(get(&conn, &c, "tasks/by-key/BE-1").1["data"]["id"], 7);
        assert_eq!(get(&conn, &c, "tasks/by-key/be-1").1["data"]["id"], 7);
    }

    #[test]
    fn time_summary_counts_tasks_updated_in_the_range() {
        let conn = db(SEED);
        let c = ctx("2026-10-13T12:00:00.000Z");
        let (status, body) = get(
            &conn,
            &c,
            "workspaces/-42/time-summary?from=2026-10-10&to=2026-10-12&group_by=task",
        );
        assert_eq!(status, 200);
        assert_eq!(
            body,
            json!({ "data": {
                "from": "2026-10-10",
                "to": "2026-10-12",
                "total_s": 1800,
                "items": [
                    { "task_id": 2, "key": "WK-2", "title": "plain", "seconds": 1200 },
                    { "task_id": 1, "key": "TM-5", "title": "TM-5: prefixed", "seconds": 600 },
                ],
            }})
        );
        let (_, week) = get(&conn, &c, "workspaces/-42/time-summary");
        assert_eq!(week["data"]["from"], "2026-10-06");
        assert_eq!(week["data"]["to"], "2026-10-13");
        assert_eq!(
            get(&conn, &c, "workspaces/-42/time-summary?group_by=user").0,
            400
        );
        assert_eq!(
            get(
                &conn,
                &c,
                "workspaces/-42/time-summary?from=2026-10-12&to=2026-10-01"
            )
            .0,
            400
        );
        assert_eq!(
            get(&conn, &c, "workspaces/-42/time-summary?from=bad").0,
            400
        );
    }

    #[test]
    fn lists_the_workspaces_next_to_the_current_one() {
        let dir = tempfile::tempdir().unwrap();
        for (code, name) in [("alpha", "Alpha"), ("test", "Test")] {
            let ws = dir.path().join(code);
            std::fs::create_dir_all(&ws).unwrap();
            std::fs::write(
                ws.join("workspace.json"),
                format!(r#"{{"id":1,"name":"{name}","code":"{code}","schema_version":12,"created_at":"x"}}"#),
            )
            .unwrap();
        }
        let conn = db("");
        let mut c = ctx("2026-10-13T12:00:00.000Z");
        c.workspace.path = dir.path().join("test");
        let (_, body) = get(&conn, &c, "workspaces");
        let list = body["data"].as_array().unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0]["code"], "alpha");
        assert_eq!(list[0]["id"], crate::local_id("alpha"));
        assert_eq!(list[1]["id"], -42);
        assert_eq!(list[1]["local"], true);
    }
}
