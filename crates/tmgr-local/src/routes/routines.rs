use std::collections::HashMap;

use chrono::{Datelike, Days, Months, NaiveDate};
use serde_json::{json, Value};

use super::text;
use crate::db::{select, Row};
use crate::js;
use crate::router::{http, sql, Handled, Request};
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
}
