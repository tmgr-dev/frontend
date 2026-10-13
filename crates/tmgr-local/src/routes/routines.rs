use std::collections::HashMap;

use chrono::{Datelike, Days, Months, NaiveDate};
use serde_json::{json, Value};

use rusqlite::types::Value as Sql;

use super::{int, text};
use crate::db::{execute, select, select_one, Row};
use crate::js;
use crate::router::{http, sql, Handled, Outcome, Request};
use crate::serialize::col;

const PATTERN_SQL: &str = "
	SELECT rp.*, r.title, r.description, r.routine_category, r.approximately_time, r.created_at, r.updated_at
	FROM routine_patterns rp JOIN routines r ON r.id = rp.routine_id
	WHERE r.deleted_at IS NULL AND r.archived_at IS NULL
		AND rp.frequency IS NOT NULL AND rp.frequency NOT IN ('', 'NONE')";

const INSTANCE_SQL: &str = "
	SELECT ri.* FROM routine_instances ri JOIN routines r ON r.id = ri.routine_id
	WHERE r.deleted_at IS NULL AND r.archived_at IS NULL
		AND ri.scheduled_for >= ? AND ri.scheduled_for < ?";

const ORPHAN_SQL: &str = "
	SELECT ri.*, r.title, r.description, r.routine_category, r.created_at, r.updated_at,
		rp.frequency AS pattern_frequency, COALESCE(rp.duration_min, r.approximately_time) AS duration_min, rp.reminder_min
	FROM routine_instances ri
	JOIN routines r ON r.id = ri.routine_id
	LEFT JOIN routine_patterns rp ON rp.routine_id = ri.routine_id
	WHERE r.deleted_at IS NULL AND r.archived_at IS NULL
		AND ri.scheduled_for >= ? AND ri.scheduled_for < ?
		AND (rp.id IS NULL OR rp.frequency IS NULL OR rp.frequency = '' OR rp.frequency = 'NONE')";

const PLAIN_SQL: &str = "
	SELECT r.id AS routine_id, r.title, r.description, r.routine_category, r.created_at, r.updated_at,
		r.scheduled_date, r.scheduled_time, r.approximately_time
	FROM routines r
	LEFT JOIN routine_patterns rp ON rp.routine_id = r.id
	WHERE r.deleted_at IS NULL AND r.archived_at IS NULL
		AND (rp.id IS NULL OR rp.frequency IS NULL OR rp.frequency = '' OR rp.frequency = 'NONE')
		AND NOT EXISTS (SELECT 1 FROM routine_instances ri2 WHERE ri2.routine_id = r.id)";

const NAN_DAY: &str = "NaN-NaN-NaN";

/// `new Date(Date.UTC(y, m - 1, d))` of `YYYY-MM-DD`, with JavaScript's overflow of month and day.
fn utc_date(day: &str) -> Option<NaiveDate> {
    let mut parts = day.split('-').map(js::parse_number);
    let (y, m, d) = (parts.next()?, parts.next()?, parts.next()?);
    if ![y, m, d].iter().all(|v| v.is_finite()) {
        return None;
    }
    let (y, m, d) = (y.trunc() as i32, m.trunc() as i64, d.trunc() as i64);
    let base = NaiveDate::from_ymd_opt(y, 1, 1)?;
    let months = m - 1;
    let base = if months >= 0 {
        base.checked_add_months(Months::new(months as u32))?
    } else {
        base.checked_sub_months(Months::new((-months) as u32))?
    };
    if d >= 1 {
        base.checked_add_days(Days::new((d - 1) as u64))
    } else {
        base.checked_sub_days(Days::new((1 - d) as u64))
    }
}

fn ymd(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn add_days(day: &str, n: u64) -> String {
    utc_date(day)
        .and_then(|d| d.checked_add_days(Days::new(n)))
        .map_or_else(|| NAN_DAY.to_string(), ymd)
}

fn days_between(from: &str, to: &str) -> Option<i64> {
    Some((utc_date(to)? - utc_date(from)?).num_days())
}

fn start_of_week(date: NaiveDate) -> NaiveDate {
    date - Days::new(date.weekday().num_days_from_monday() as u64)
}

fn int_of(v: &Value) -> Option<i64> {
    v.as_i64()
}

fn ymd_parts(day: &str) -> Vec<f64> {
    day.split('-').map(js::parse_number).collect()
}

/// Port of the TS `firesOn` (Java `RoutineExpansionService`).
fn fires_on(p: &Row, day: &str, range_from: &str) -> bool {
    let start = col(p, "start_at").as_str();
    let end = col(p, "end_at").as_str();
    if start.is_some_and(|s| day < s) || end.is_some_and(|e| day > e) {
        return false;
    }
    let start_date = start.unwrap_or(range_from);
    let interval = col(p, "interval").as_i64().unwrap_or(1).max(1);
    let freq = js::string(col(p, "frequency")).to_uppercase();
    let day_parts = ymd_parts(day);
    let day_of = |i: usize| day_parts.get(i).copied().unwrap_or(f64::NAN);
    match freq.as_str() {
        "DAILY" => {
            days_between(start_date, day).is_some_and(|diff| diff >= 0 && diff % interval == 0)
        }
        "WEEKLY" => {
            let days: Vec<String> = match js::parse_json(col(p, "days_of_week"), json!([])) {
                Value::Array(a) => a
                    .into_iter()
                    .filter_map(|v| v.as_str().map(str::to_string))
                    .collect(),
                _ => Vec::new(),
            };
            let (Some(d), Some(s)) = (utc_date(day), utc_date(start_date)) else {
                return false;
            };
            const DOW: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
            let dow = DOW[d.weekday().num_days_from_sunday() as usize];
            if days.is_empty() || !days.iter().any(|k| k == dow) {
                return false;
            }
            let weeks = (start_of_week(d) - start_of_week(s)).num_days() / 7;
            weeks >= 0 && weeks % interval == 0
        }
        "MONTHLY" => {
            let Some(dof) = int_of(col(p, "day_of_frequency")) else {
                return false;
            };
            if day_of(2) != dof as f64 {
                return false;
            }
            let s = ymd_parts(start_date);
            let s_of = |i: usize| s.get(i).copied().unwrap_or(f64::NAN);
            let months = (day_of(0) - s_of(0)) * 12.0 + (day_of(1) - s_of(1));
            months >= 0.0 && months % interval as f64 == 0.0
        }
        "YEARLY" => {
            let (Some(dof), Some(mon)) =
                (int_of(col(p, "day_of_frequency")), int_of(col(p, "month")))
            else {
                return false;
            };
            if day_of(2) != dof as f64 || day_of(1) != (mon + 1) as f64 {
                return false;
            }
            let years = day_of(0) - ymd_parts(start_date).first().copied().unwrap_or(f64::NAN);
            years >= 0.0 && years % interval as f64 == 0.0
        }
        _ => false,
    }
}

fn category(id: &Value) -> Value {
    let key = id
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("none");
    let (id, name, color) = match key {
        "work" => ("work", "Work", "#5b8cff"),
        "health" => ("health", "Health", "#22c55e"),
        "learn" => ("learn", "Learn", "#a78bfa"),
        "home" => ("home", "Home", "#f5b54a"),
        "social" => ("social", "Social", "#ec4899"),
        _ => ("none", "General", "#888888"),
    };
    json!({ "id": id, "name": name, "color": color })
}

fn display_time(time: Option<&str>) -> Value {
    match time {
        Some(t) if !t.is_empty() && t != "00:00:00" => json!(t.chars().take(5).collect::<String>()),
        _ => Value::Null,
    }
}

fn first_of<'a>(values: &[&'a Value]) -> &'a Value {
    values
        .iter()
        .copied()
        .find(|v| !v.is_null())
        .unwrap_or(&Value::Null)
}

fn status_priority(status: &Value) -> u8 {
    match status.as_str() {
        Some("COMPLETED") => 3,
        Some("SKIPPED") => 2,
        _ => 1,
    }
}

fn entry(row: &Row, day: &str, time: Option<&str>) -> serde_json::Map<String, Value> {
    let json = json!({
        "task_id": col(row, "routine_id"),
        "title": col(row, "title"),
        "description": col(row, "description"),
        "routine_category": category(col(row, "routine_category")),
        "date": day,
        "time": display_time(time),
        "created_at": col(row, "created_at"),
        "updated_at": col(row, "updated_at"),
    });
    match json {
        Value::Object(m) => m,
        _ => unreachable!(),
    }
}

/// Calendar entries of every routine between two dates, like the TS `expandRange`.
pub(super) fn expand(req: &Request<'_>) -> Handled {
    let (Some(from), Some(to)) = (req.query.filled("from"), req.query.filled("to")) else {
        return Err(http(422, "from and to are required"));
    };
    let range = [
        text(&format!("{from}T00:00:00")),
        text(&format!("{}T00:00:00", add_days(to, 1))),
    ];
    let patterns = sql(select(req.conn, PATTERN_SQL, &[]))?;
    let instances = sql(select(req.conn, INSTANCE_SQL, &range))?;
    let orphans = sql(select(req.conn, ORPHAN_SQL, &range))?;
    let plain = sql(select(req.conn, PLAIN_SQL, &[]))?;

    let mut by_day: HashMap<String, &Row> = HashMap::new();
    for row in &instances {
        let date: String = js::string(col(row, "scheduled_for"))
            .chars()
            .take(10)
            .collect();
        let key = format!("{}|{date}", js::string(col(row, "routine_id")));
        let better = by_day.get(&key).is_none_or(|old| {
            status_priority(col(row, "status")) > status_priority(col(old, "status"))
        });
        if better {
            by_day.insert(key, row);
        }
    }

    let mut results: Vec<Value> = Vec::new();
    let mut day = from.to_string();
    while day.as_str() <= to && day != NAN_DAY {
        for row in &patterns {
            if !fires_on(row, &day, from) {
                continue;
            }
            let inst = by_day.get(&format!("{}|{day}", js::string(col(row, "routine_id"))));
            let time = col(row, "scheduled_time").as_str();
            let status = inst.map_or("pending".to_string(), |i| {
                js::string(col(i, "status")).to_lowercase()
            });
            let mut e = entry(row, &day, time);
            e.insert(
                "scheduled_for".into(),
                json!(format!("{day}T{}Z", time.unwrap_or("00:00:00"))),
            );
            e.insert(
                "duration_min".into(),
                first_of(&[col(row, "approximately_time"), col(row, "duration_min")]).clone(),
            );
            e.insert("reminder_min".into(), col(row, "reminder_min").clone());
            e.insert("frequency".into(), col(row, "frequency").clone());
            e.insert("completed".into(), json!(status == "completed"));
            e.insert("status".into(), json!(status));
            e.insert(
                "instance_id".into(),
                inst.map_or(Value::Null, |i| col(i, "id").clone()),
            );
            e.insert("virtual".into(), json!(inst.is_none()));
            results.push(Value::Object(e));
        }
        day = add_days(&day, 1);
    }

    for row in &orphans {
        let scheduled = js::string(col(row, "scheduled_for"));
        let mut parts = scheduled.split('T');
        let date = parts.next().unwrap_or("");
        let time = parts.next();
        let status = match col(row, "status") {
            Value::Null => "pending".to_string(),
            s => js::string(s).to_lowercase(),
        };
        let mut e = entry(row, date, time);
        e.insert("scheduled_for".into(), json!(format!("{scheduled}Z")));
        e.insert("duration_min".into(), col(row, "duration_min").clone());
        e.insert("reminder_min".into(), col(row, "reminder_min").clone());
        e.insert("frequency".into(), col(row, "pattern_frequency").clone());
        e.insert("completed".into(), json!(status == "completed"));
        e.insert("status".into(), json!(status));
        e.insert("instance_id".into(), col(row, "id").clone());
        e.insert("virtual".into(), json!(false));
        results.push(Value::Object(e));
    }

    let today = req.ctx.local_now().format("%Y-%m-%d").to_string();
    for row in &plain {
        let day = match col(row, "scheduled_date") {
            Value::Null => today.clone(),
            d => js::string(d),
        };
        if day.as_str() < from || day.as_str() > to {
            continue;
        }
        let time = col(row, "scheduled_time").as_str();
        let mut e = entry(row, &day, time);
        e.insert(
            "scheduled_for".into(),
            json!(format!("{day}T{}Z", time.unwrap_or("00:00:00"))),
        );
        e.insert(
            "duration_min".into(),
            col(row, "approximately_time").clone(),
        );
        e.insert("reminder_min".into(), Value::Null);
        e.insert("frequency".into(), json!("NONE"));
        e.insert("status".into(), json!("pending"));
        e.insert("completed".into(), json!(false));
        e.insert("instance_id".into(), Value::Null);
        e.insert("virtual".into(), json!(true));
        results.push(Value::Object(e));
    }

    results.sort_by(|a, b| {
        let key = |v: &Value| v["scheduled_for"].as_str().unwrap_or("").to_string();
        key(a).cmp(&key(b))
    });
    Ok(Value::Array(results))
}

fn require_active(req: &Request<'_>, id: i64) -> Result<Row, Outcome> {
    sql(select_one(
        req.conn,
        "SELECT * FROM routines WHERE id = ? AND deleted_at IS NULL AND archived_at IS NULL",
        &[int(id)],
    ))?
    .ok_or_else(|| http(404, "Routine task not found"))
}

fn instance_json(row: &Row) -> Value {
    let scheduled = js::string(col(row, "scheduled_for"));
    json!({
        "id": col(row, "id"),
        "task_id": col(row, "routine_id"),
        "scheduled_for": format!("{scheduled}Z"),
        "status": col(row, "status"),
        "completed_at": col(row, "completed_at"),
        "skipped_at": col(row, "skipped_at"),
        "created_at": col(row, "created_at"),
        "updated_at": col(row, "updated_at"),
        "scheduled_date": scheduled.chars().take(10).collect::<String>(),
        "scheduled_time": scheduled.chars().skip(11).collect::<String>(),
    })
}

fn instance(req: &Request<'_>, id: i64) -> Result<Row, Outcome> {
    Ok(sql(select_one(
        req.conn,
        "SELECT * FROM routine_instances WHERE id = ?",
        &[int(id)],
    ))?
    .unwrap_or_default())
}

/// JavaScript `Date.parse` in milliseconds for the forms the local store holds.
fn parse_ms(req: &Request<'_>, v: &Value) -> Option<i64> {
    let s = v.as_str()?;
    if let Ok(d) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(d.timestamp_millis());
    }
    if let Ok(d) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S%.f") {
        return d
            .and_local_timezone(req.ctx.local_offset)
            .single()
            .map(|d| d.timestamp_millis());
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().map(|d| {
        d.and_hms_opt(0, 0, 0)
            .unwrap_or_default()
            .and_utc()
            .timestamp_millis()
    })
}

/// Toggles the instance of a day between completed and pending, like the TS `completeForDate`.
fn complete_for_date(req: &Request<'_>, routine_id: i64, date: &str) -> Result<Row, Outcome> {
    let day_start = format!("{date}T00:00:00");
    let day_end = format!("{}T00:00:00", add_days(date, 1));
    let now = req.ctx.iso_now();
    let existing = sql(select_one(
        req.conn,
        "SELECT * FROM routine_instances WHERE routine_id = ? AND scheduled_for >= ? AND scheduled_for < ?",
        &[int(routine_id), text(&day_start), text(&day_end)],
    ))?;
    let row = match existing {
        Some(row) => row,
        None => {
            let inserted = sql(execute(
                req.conn,
                "INSERT INTO routine_instances (routine_id, scheduled_for, status, created_at, updated_at) VALUES (?, ?, 'PENDING', ?, ?)",
                &[int(routine_id), text(&day_start), text(&now), text(&now)],
            ))?;
            instance(req, inserted.last_id)?
        }
    };
    let was_completed = col(&row, "status").as_str() == Some("COMPLETED");
    let id = col(&row, "id").as_i64().unwrap_or(0);
    if was_completed {
        let n = sql(select_one(
            req.conn,
            "SELECT COUNT(*) AS n FROM routine_instances WHERE routine_id = ?",
            &[int(routine_id)],
        ))?
        .and_then(|r| col(&r, "n").as_i64())
        .unwrap_or(0);
        let pattern = sql(select_one(
            req.conn,
            "SELECT frequency FROM routine_patterns WHERE routine_id = ? AND frequency IS NOT NULL AND frequency NOT IN ('', 'NONE')",
            &[int(routine_id)],
        ))?;
        let routine = sql(select_one(
            req.conn,
            "SELECT created_at FROM routines WHERE id = ?",
            &[int(routine_id)],
        ))?;
        let gap = |a: &Value, b: &Value| match (parse_ms(req, a), parse_ms(req, b)) {
            (Some(a), Some(b)) => Some((a - b).abs()),
            _ => None,
        };
        let by_completion =
            gap(col(&row, "created_at"), col(&row, "completed_at")).is_some_and(|g| g <= 60_000);
        let with_routine = routine.as_ref().is_some_and(|r| {
            let created = col(r, "created_at");
            let day_of_creation = js::string(created).chars().take(10).collect::<String>();
            gap(col(&row, "created_at"), created).is_some_and(|g| g <= 60_000)
                && gap(
                    &json!(format!("{date}T00:00:00Z")),
                    &json!(format!("{day_of_creation}T00:00:00Z")),
                )
                .is_some_and(|g| g <= 86_400_000)
        });
        let midnight = js::string(col(&row, "scheduled_for")).get(11..19) == Some("00:00:00");
        if pattern.is_none() && n == 1 && midnight && (by_completion || with_routine) {
            sql(execute(
                req.conn,
                "DELETE FROM routine_instances WHERE id = ?",
                &[int(id)],
            ))?;
            let mut gone = row.clone();
            gone.insert("id".into(), Value::Null);
            gone.insert("status".into(), json!("PENDING"));
            gone.insert("completed_at".into(), Value::Null);
            gone.insert("skipped_at".into(), Value::Null);
            gone.insert("updated_at".into(), json!(now));
            return Ok(gone);
        }
    }
    let status = if was_completed {
        "PENDING"
    } else {
        "COMPLETED"
    };
    sql(execute(
        req.conn,
        "UPDATE routine_instances SET status = ?, completed_at = ?, skipped_at = NULL, updated_at = ? WHERE id = ?",
        &[
            text(status),
            if was_completed { Sql::Null } else { text(&now) },
            text(&now),
            int(id),
        ],
    ))?;
    instance(req, id)
}

fn today(req: &Request<'_>) -> String {
    req.ctx.local_now().format("%Y-%m-%d").to_string()
}

pub(super) fn complete(req: &Request<'_>) -> Handled {
    let row = require_active(req, req.id("id"))?;
    let id = col(&row, "id").as_i64().unwrap_or(0);
    Ok(instance_json(&complete_for_date(req, id, &today(req))?))
}

pub(super) fn complete_on(req: &Request<'_>) -> Handled {
    let row = require_active(req, req.id("id"))?;
    let id = col(&row, "id").as_i64().unwrap_or(0);
    let from_body = req
        .body
        .get("date")
        .filter(|v| js::truthy(v))
        .map(js::string);
    let date = req
        .query
        .filled("date")
        .map(str::to_string)
        .or(from_body)
        .unwrap_or_else(|| today(req));
    let valid = date.len() == 10 && NaiveDate::parse_from_str(&date, "%Y-%m-%d").is_ok();
    if !valid {
        return Err(http(422, "date must be YYYY-MM-DD"));
    }
    let instance = complete_for_date(req, id, &date)?;
    let status = col(&instance, "status").clone();
    Ok(json!({
        "instance_id": col(&instance, "id"),
        "task_id": id,
        "date": date,
        "completed": status == "COMPLETED",
        "status": status,
    }))
}

fn set_instance_status(req: &Request<'_>, status: &str) -> Handled {
    let row = require_active(req, req.id("id"))?;
    let routine_id = col(&row, "id").as_i64().unwrap_or(0);
    let instance_id = req.id("instanceId");
    sql(select_one(
        req.conn,
        "SELECT id FROM routine_instances WHERE id = ? AND routine_id = ?",
        &[int(instance_id), int(routine_id)],
    ))?
    .ok_or_else(|| http(404, "Task instance not found for routine"))?;
    let now = req.ctx.iso_now();
    let stamp = |when: &str| {
        if status == when {
            text(&now)
        } else {
            Sql::Null
        }
    };
    sql(execute(
        req.conn,
        "UPDATE routine_instances SET status = ?, completed_at = ?, skipped_at = ?, updated_at = ? WHERE id = ?",
        &[text(status), stamp("COMPLETED"), stamp("SKIPPED"), text(&now), int(instance_id)],
    ))?;
    Ok(instance_json(&instance(req, instance_id)?))
}

pub(super) fn complete_instance(req: &Request<'_>) -> Handled {
    set_instance_status(req, "COMPLETED")
}

pub(super) fn skip_instance(req: &Request<'_>) -> Handled {
    set_instance_status(req, "SKIPPED")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{ctx, db, get};

    #[test]
    fn dates_overflow_like_javascript() {
        assert_eq!(add_days("2026-02-28", 1), "2026-03-01");
        assert_eq!(add_days("2026-02-30", 1), "2026-03-03");
        assert_eq!(add_days("2026-12-31", 1), "2027-01-01");
        assert_eq!(add_days("nope", 1), NAN_DAY);
    }

    #[test]
    fn expands_patterns_instances_and_plain_routines() {
        let conn = db("
            INSERT INTO routines (id, title, routine_category, scheduled_time, created_at, updated_at)
            VALUES (1, 'Run', 'health', '07:30:00', 'c', 'u'), (2, 'Weekly', NULL, NULL, 'c', 'u'),
                   (3, 'Once', 'bogus', NULL, 'c', 'u');
            UPDATE routines SET scheduled_date = '2026-10-02' WHERE id = 3;
            INSERT INTO routine_patterns (routine_id, frequency, interval, days_of_week, scheduled_time, start_at, created_at, updated_at)
            VALUES (1, 'DAILY', 2, NULL, '07:30:00', '2026-09-30', 'c', 'u'),
                   (2, 'WEEKLY', 1, '[\"FRI\"]', NULL, NULL, 'c', 'u');
            INSERT INTO routine_instances (routine_id, scheduled_for, status, created_at, updated_at)
            VALUES (1, '2026-10-02T07:30:00', 'PENDING', 'c', 'u'), (1, '2026-10-02T07:30:00', 'COMPLETED', 'c', 'u');
        ");
        let c = ctx("2026-10-13T12:00:00.000Z");
        let (status, body) = get(
            &conn,
            &c,
            "daily-routines/expand?from=2026-10-01&to=2026-10-04",
        );
        assert_eq!(status, 200);
        let got: Vec<(String, String, bool)> = body["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["title"].as_str().unwrap().to_string(),
                    e["scheduled_for"].as_str().unwrap().to_string(),
                    e["completed"].as_bool().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            got,
            [
                (
                    "Weekly".to_string(),
                    "2026-10-02T00:00:00Z".to_string(),
                    false
                ),
                (
                    "Once".to_string(),
                    "2026-10-02T00:00:00Z".to_string(),
                    false
                ),
                ("Run".to_string(), "2026-10-02T07:30:00Z".to_string(), true),
                ("Run".to_string(), "2026-10-04T07:30:00Z".to_string(), false),
            ]
        );
        assert_eq!(body["data"][1]["routine_category"]["id"], "none");
        assert_eq!(
            get(&conn, &c, "daily-routines/expand?from=2026-10-01").0,
            422
        );
    }

    #[test]
    fn complete_on_refuses_a_date_that_is_not_a_day() {
        let conn = db(
            "INSERT INTO routines (id, title, created_at, updated_at) VALUES (1, 'R', 'x', 'x');",
        );
        conn.execute_batch("PRAGMA query_only = OFF").unwrap();
        let c = ctx("2026-10-13T12:00:00.000Z");
        for date in ["2026-13-01", "tomorrow", "2026-10-1"] {
            let r = crate::dispatch(
                &conn,
                &c,
                "POST",
                "daily-routines/tasks/1/complete-on",
                Some(&json!({ "date": date })),
            )
            .unwrap()
            .unwrap();
            assert_eq!(r.status, 422, "{date}");
        }
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM routine_instances", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 0);
    }
}
