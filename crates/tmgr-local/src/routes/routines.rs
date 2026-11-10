use std::collections::{HashMap, HashSet};

use chrono::{Datelike, Days, Months, NaiveDate};
use serde_json::{json, Value};

use rusqlite::types::Value as Sql;

use super::routine_series::{fires_on, from_segment, governing, live_version, Version};
use super::{int, text};
use crate::db::{execute, schema_version, select, select_one, Row};
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

const SERIES_SCHEMA: u32 = 14;

const NAN_DAY: &str = "NaN-NaN-NaN";

/// `new Date(Date.UTC(y, m - 1, d))` of `YYYY-MM-DD`, with JavaScript's overflow of month and day.
pub(super) fn utc_date(day: &str) -> Option<NaiveDate> {
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

pub(super) fn add_days(day: &str, n: u64) -> String {
    utc_date(day)
        .and_then(|d| d.checked_add_days(Days::new(n)))
        .map_or_else(|| NAN_DAY.to_string(), ymd)
}

pub(super) fn days_between(from: &str, to: &str) -> Option<i64> {
    Some((utc_date(to)? - utc_date(from)?).num_days())
}

pub(super) fn start_of_week(date: NaiveDate) -> NaiveDate {
    date - Days::new(date.weekday().num_days_from_monday() as u64)
}

pub(super) fn ymd_parts(day: &str) -> Vec<f64> {
    day.split('-').map(js::parse_number).collect()
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

const SEGMENT_SQL: &str = "
	SELECT s.* FROM routine_pattern_segments s JOIN routines r ON r.id = s.routine_id
	WHERE r.deleted_at IS NULL AND r.archived_at IS NULL
	ORDER BY s.routine_id, s.valid_from, s.id";

const INSTANCE_SQL_V14: &str = "
	SELECT ri.* FROM routine_instances ri JOIN routines r ON r.id = ri.routine_id
	WHERE r.deleted_at IS NULL AND r.archived_at IS NULL AND ri.moved_from IS NULL
		AND ri.scheduled_for >= ? AND ri.scheduled_for < ?";

const MOVED_SQL: &str = "
	SELECT ri.* FROM routine_instances ri JOIN routines r ON r.id = ri.routine_id
	WHERE r.deleted_at IS NULL AND r.archived_at IS NULL AND ri.moved_from IS NOT NULL
		AND EXISTS (SELECT 1 FROM routine_patterns rp WHERE rp.routine_id = ri.routine_id
			AND rp.frequency IS NOT NULL AND rp.frequency NOT IN ('', 'NONE'))
		AND ((ri.scheduled_for >= ? AND ri.scheduled_for < ?) OR (ri.moved_from >= ? AND ri.moved_from <= ?))";

fn slice(value: &Value, from: usize, to: usize) -> String {
    js::string(value)
        .chars()
        .skip(from)
        .take(to - from)
        .collect()
}

/// The TS `buildPatternEntry`; `overridable` is false for moved and safety-net entries, which take their own time.
fn pattern_entry(
    row: &Row,
    v: &Version,
    day: &str,
    inst: Option<&Row>,
    overridable: bool,
) -> serde_json::Map<String, Value> {
    let own = |key: &str| inst.map(|i| col(i, key)).filter(|v| !v.is_null());
    let override_time = inst
        .filter(|i| {
            overridable
                && js::truthy(&js::number(col(i, "is_override")))
                && js::truthy(col(i, "scheduled_for"))
        })
        .map(|i| slice(col(i, "scheduled_for"), 11, 19));
    let time = override_time.or_else(|| v.scheduled_time.clone());
    let status = inst.map_or("pending".to_string(), |i| {
        js::string(col(i, "status")).to_lowercase()
    });
    let mut e = entry(row, day, time.as_deref());
    e.insert(
        "title".into(),
        own("title").cloned().unwrap_or_else(|| {
            if v.title.is_null() {
                json!("")
            } else {
                v.title.clone()
            }
        }),
    );
    e.insert(
        "description".into(),
        own("description")
            .cloned()
            .unwrap_or_else(|| v.description.clone()),
    );
    e.insert("routine_category".into(), category(&v.routine_category));
    e.insert(
        "scheduled_for".into(),
        json!(format!("{day}T{}Z", time.as_deref().unwrap_or("00:00:00"))),
    );
    e.insert(
        "duration_min".into(),
        own("duration_min")
            .cloned()
            .unwrap_or_else(|| v.duration_min.clone()),
    );
    e.insert("reminder_min".into(), v.reminder_min.clone());
    e.insert("frequency".into(), v.frequency.clone());
    e.insert("completed".into(), json!(status == "completed"));
    e.insert("status".into(), json!(status));
    e.insert(
        "instance_id".into(),
        inst.map_or(Value::Null, |i| col(i, "id").clone()),
    );
    e.insert("virtual".into(), json!(inst.is_none()));
    e
}

struct Ordered<'a> {
    keys: HashMap<String, usize>,
    rows: Vec<(String, &'a Row)>,
}

impl<'a> Ordered<'a> {
    fn new() -> Self {
        Ordered {
            keys: HashMap::new(),
            rows: Vec::new(),
        }
    }

    fn get(&self, key: &str) -> Option<&'a Row> {
        self.keys.get(key).map(|i| self.rows[*i].1)
    }

    fn set(&mut self, key: String, row: &'a Row) {
        match self.keys.get(&key) {
            Some(i) => self.rows[*i].1 = row,
            None => {
                self.keys.insert(key.clone(), self.rows.len());
                self.rows.push((key, row));
            }
        }
    }
}

fn comment_counts(req: &Request<'_>, ids: &[i64]) -> Result<HashMap<i64, i64>, Outcome> {
    let mut counts = HashMap::new();
    if ids.is_empty() {
        return Ok(counts);
    }
    let marks = vec!["?"; ids.len()].join(", ");
    let params: Vec<Sql> = ids.iter().map(|i| int(*i)).collect();
    let rows = sql(select(
        req.conn,
        &format!(
            "SELECT routine_id, COUNT(*) AS n FROM routine_comments
			 WHERE routine_id IN ({marks}) AND deleted_at IS NULL GROUP BY routine_id"
        ),
        &params,
    ))?;
    for r in rows {
        counts.insert(
            col(&r, "routine_id").as_i64().unwrap_or(0),
            col(&r, "n").as_i64().unwrap_or(0),
        );
    }
    Ok(counts)
}

/// Calendar entries of every routine between two dates, like the TS `expandRange`.
pub(super) fn expand(req: &Request<'_>) -> Handled {
    let (Some(from), Some(to)) = (req.query.filled("from"), req.query.filled("to")) else {
        return Err(http(422, "from and to are required"));
    };
    let series = sql(schema_version(req.conn))? >= SERIES_SCHEMA;
    let range = [
        text(&format!("{from}T00:00:00")),
        text(&format!("{}T00:00:00", add_days(to, 1))),
    ];
    let patterns = sql(select(req.conn, PATTERN_SQL, &[]))?;
    let segment_rows = if series {
        sql(select(req.conn, SEGMENT_SQL, &[]))?
    } else {
        Vec::new()
    };
    let instances = sql(select(
        req.conn,
        if series {
            INSTANCE_SQL_V14
        } else {
            INSTANCE_SQL
        },
        &range,
    ))?;
    let moved = if series {
        sql(select(
            req.conn,
            MOVED_SQL,
            &[range[0].clone(), range[1].clone(), text(from), text(to)],
        ))?
    } else {
        Vec::new()
    };
    let orphans = sql(select(req.conn, ORPHAN_SQL, &range))?;
    let plain = sql(select(req.conn, PLAIN_SQL, &[]))?;

    let mut segments_by: HashMap<i64, Vec<Version>> = HashMap::new();
    for row in &segment_rows {
        segments_by
            .entry(col(row, "routine_id").as_i64().unwrap_or(0))
            .or_default()
            .push(from_segment(row));
    }
    let no_segments: Vec<Version> = Vec::new();
    let mut pattern_by: HashMap<i64, &Row> = HashMap::new();
    let mut live_by: HashMap<i64, Version> = HashMap::new();
    for row in &patterns {
        let id = col(row, "routine_id").as_i64().unwrap_or(0);
        pattern_by.insert(id, row);
        live_by.insert(
            id,
            live_version(row, segments_by.get(&id).unwrap_or(&no_segments), true),
        );
    }
    let version_for = |id: i64, day: &str| -> Option<Version> {
        let live = live_by.get(&id)?;
        Some(
            governing(segments_by.get(&id).unwrap_or(&no_segments), live, day)
                .unwrap_or(live)
                .clone(),
        )
    };

    let mut by_day = Ordered::new();
    for row in &instances {
        let date = slice(col(row, "scheduled_for"), 0, 10);
        let key = format!("{}|{date}", js::string(col(row, "routine_id")));
        let better = by_day.get(&key).is_none_or(|old| {
            status_priority(col(row, "status")) > status_priority(col(old, "status"))
        });
        if better {
            by_day.set(key, row);
        }
    }
    let moved_away: HashSet<String> = moved
        .iter()
        .map(|m| {
            format!(
                "{}|{}",
                js::string(col(m, "routine_id")),
                js::string(col(m, "moved_from"))
            )
        })
        .collect();

    let mut results: Vec<Value> = Vec::new();
    let mut emitted: HashSet<String> = HashSet::new();
    let mut day = from.to_string();
    while day.as_str() <= to && day != NAN_DAY {
        for row in &patterns {
            let id = col(row, "routine_id").as_i64().unwrap_or(0);
            let key = format!("{}|{day}", js::string(col(row, "routine_id")));
            if moved_away.contains(&key) {
                continue;
            }
            let Some(live) = live_by.get(&id) else {
                continue;
            };
            let Some(v) = governing(segments_by.get(&id).unwrap_or(&no_segments), live, &day)
            else {
                continue;
            };
            if fires_on(v, &day, Some(from)) {
                let e = pattern_entry(row, v, &day, by_day.get(&key), true);
                results.push(Value::Object(e));
                emitted.insert(key);
            }
        }
        day = add_days(&day, 1);
    }

    for (key, inst) in &by_day.rows {
        let id = col(inst, "routine_id").as_i64().unwrap_or(0);
        let date = slice(col(inst, "scheduled_for"), 0, 10);
        let Some(row) = pattern_by.get(&id) else {
            continue;
        };
        if emitted.contains(key) || moved_away.contains(key) {
            continue;
        }
        let status = js::string(col(inst, "status")).to_uppercase();
        if status != "COMPLETED" && status != "SKIPPED" {
            continue;
        }
        let Some(v) = version_for(id, &date) else {
            continue;
        };
        let v = v.with_time(Some(slice(col(inst, "scheduled_for"), 11, 19)));
        results.push(Value::Object(pattern_entry(
            row,
            &v,
            &date,
            Some(inst),
            false,
        )));
    }

    for m in &moved {
        let id = col(m, "routine_id").as_i64().unwrap_or(0);
        let moved_day = slice(col(m, "scheduled_for"), 0, 10);
        let Some(row) = pattern_by.get(&id) else {
            continue;
        };
        if moved_day.as_str() < from || moved_day.as_str() > to {
            continue;
        }
        let Some(v) = version_for(id, &js::string(col(m, "moved_from"))) else {
            continue;
        };
        let v = v.with_time(Some(slice(col(m, "scheduled_for"), 11, 19)));
        let mut e = pattern_entry(row, &v, &moved_day, Some(m), false);
        e.insert("moved_from".into(), col(m, "moved_from").clone());
        results.push(Value::Object(e));
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

    if series {
        let mut ids: Vec<i64> = Vec::new();
        for r in &results {
            if let Some(id) = r["task_id"].as_i64() {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        let counts = comment_counts(req, &ids)?;
        for r in &mut results {
            let n = r["task_id"]
                .as_i64()
                .and_then(|id| counts.get(&id).copied())
                .unwrap_or(0);
            r["comments_count"] = json!(n);
        }
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
    let mut json = json!({
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
    });
    if row.contains_key("is_override") {
        json["moved_from"] = col(row, "moved_from").clone();
        json["override"] = json!(js::number(col(row, "is_override")).as_i64() != Some(0));
        json["duration_min"] = col(row, "duration_min").clone();
        json["title"] = col(row, "title").clone();
        json["description"] = col(row, "description").clone();
    }
    json
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
    use crate::testing::{ctx, db, db13, get};

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

    const SERIES_SEED: &str = "
        INSERT INTO routines (id, title, description, created_at, updated_at)
        VALUES (1000000001, 'Run', 'Morning', '2026-10-01T08:00:00Z', 'u');
        INSERT INTO routine_patterns (routine_id, frequency, interval, scheduled_time, start_at, end_at, series_version, created_at, updated_at)
        VALUES (1000000001, 'DAILY', 1, '09:00:00', '2026-10-05', NULL, 1, 'c', 'u');
        INSERT INTO routine_pattern_segments (routine_id, valid_from, valid_to, anchor_date, is_recurring, frequency, interval, scheduled_time, title, description, created_at)
        VALUES (1000000001, '2026-10-01', '2026-10-04', '2026-10-01', 1, 'DAILY', 2, '07:00:00', 'Old run', 'Old', 'c');
        INSERT INTO routine_instances (routine_id, scheduled_for, status, is_override, title, duration_min, created_at, updated_at)
        VALUES (1000000001, '2026-10-06T10:30:00', 'PENDING', 1, 'Once', 45, 'c', 'u');
        INSERT INTO routine_instances (routine_id, scheduled_for, status, moved_from, created_at, updated_at)
        VALUES (1000000001, '2026-10-08T18:00:00', 'PENDING', '2026-10-07', 'c', 'u');
        INSERT INTO routine_instances (routine_id, scheduled_for, status, created_at, updated_at)
        VALUES (1000000001, '2026-10-02T00:00:00', 'COMPLETED', 'c', 'u');
        INSERT INTO routine_comments (id, routine_id, message, created_at, updated_at)
        VALUES (1000000001, 1000000001, 'hi', 'c', 'u');
    ";

    #[test]
    fn expands_through_frozen_versions_overrides_and_moves() {
        let conn = db(SERIES_SEED);
        let c = ctx("2026-10-13T12:00:00.000Z");
        let (status, body) = get(
            &conn,
            &c,
            "daily-routines/expand?from=2026-10-01&to=2026-10-09",
        );
        assert_eq!(status, 200);
        let got: Vec<(String, String, String, i64)> = body["data"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["title"].as_str().unwrap().to_string(),
                    e["scheduled_for"].as_str().unwrap().to_string(),
                    e["moved_from"].as_str().unwrap_or("").to_string(),
                    e["comments_count"].as_i64().unwrap(),
                )
            })
            .collect();
        let row = |t: &str, s: &str, m: &str| (t.to_string(), s.to_string(), m.to_string(), 1);
        assert_eq!(
            got,
            [
                row("Old run", "2026-10-01T07:00:00Z", ""),
                row("Old run", "2026-10-02T00:00:00Z", ""),
                row("Old run", "2026-10-03T07:00:00Z", ""),
                row("Run", "2026-10-05T09:00:00Z", ""),
                row("Once", "2026-10-06T10:30:00Z", ""),
                row("Run", "2026-10-08T09:00:00Z", ""),
                row("Run", "2026-10-08T18:00:00Z", "2026-10-07"),
                row("Run", "2026-10-09T09:00:00Z", ""),
            ]
        );
        assert_eq!(body["data"][4]["duration_min"], 45);
    }

    #[test]
    fn a_schema_13_database_ignores_series_history() {
        let conn = db13(
            "INSERT INTO routines (id, title, created_at, updated_at) VALUES (1, 'Run', 'c', 'u');
             INSERT INTO routine_patterns (routine_id, frequency, interval, scheduled_time, start_at, created_at, updated_at)
             VALUES (1, 'DAILY', 1, '09:00:00', '2026-10-05', 'c', 'u');
             INSERT INTO routine_pattern_segments (routine_id, valid_from, valid_to, is_recurring, frequency, interval, title, created_at)
             VALUES (1, '2026-10-01', '2026-10-04', 1, 'DAILY', 1, 'Old', 'c');",
        );
        let c = ctx("2026-10-13T12:00:00.000Z");
        let (_, body) = get(
            &conn,
            &c,
            "daily-routines/expand?from=2026-10-03&to=2026-10-06",
        );
        let entries = body["data"].as_array().unwrap();
        assert_eq!(entries.len(), 2);
        assert!(entries
            .iter()
            .all(|e| e["title"] == "Run" && e.get("comments_count").is_none()));
    }

    #[test]
    fn instance_json_carries_the_series_fields() {
        let conn = db(SERIES_SEED);
        conn.execute_batch("PRAGMA query_only = OFF").unwrap();
        let c = ctx("2026-10-13T12:00:00.000Z");
        let r = crate::dispatch(
            &conn,
            &c,
            "POST",
            "daily-routines/tasks/1000000001/instances/1/skip",
            None,
        )
        .unwrap()
        .unwrap();
        let data = &r.data["data"];
        assert_eq!(data["override"], true);
        assert_eq!(data["title"], "Once");
        assert_eq!(data["duration_min"], 45);
        assert!(data["moved_from"].is_null());
    }

    #[test]
    fn routine_comments_follow_the_task_comment_shape() {
        let conn = db(SERIES_SEED);
        conn.execute_batch("PRAGMA query_only = OFF").unwrap();
        let c = ctx("2026-10-13T12:00:00.000Z");
        let call = |m: &str, u: &str, b: Option<Value>| {
            let r = crate::dispatch(&conn, &c, m, u, b.as_ref())
                .unwrap()
                .unwrap();
            (r.status, r.data["data"].clone())
        };
        let (status, added) = call(
            "POST",
            "tasks/1000000001/comments",
            Some(json!({ "message": "second" })),
        );
        assert_eq!((status, added["id"].as_i64()), (201, Some(1000000002)));
        assert_eq!(added["task_id"], 1000000001);
        assert_eq!(added["reactions"], json!([]));
        let (_, edited) = call(
            "PUT",
            "comments/1000000001",
            Some(json!({ "message": "edited" })),
        );
        assert_eq!(edited["content"], "edited");
        let (_, list) = call("GET", "tasks/1000000001/comments", None);
        assert_eq!(list.as_array().unwrap().len(), 2);
        let (status, _) = call(
            "POST",
            "comments/1000000001/reactions/toggle",
            Some(json!({ "emoji": "x" })),
        );
        assert_eq!(status, 422);
        let (_, deleted) = call("DELETE", "comments/1000000002", None);
        assert_eq!(deleted, json!({ "success": true, "task_id": 1000000001 }));
        let (_, list) = call("GET", "tasks/1000000001/comments", None);
        assert_eq!(list.as_array().unwrap().len(), 1);
        assert_eq!(call("GET", "tasks/1000000009/comments", None).0, 404);
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
