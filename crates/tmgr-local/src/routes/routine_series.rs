use chrono::Datelike;
use serde_json::{json, Value};

use super::routines::{add_days, days_between, start_of_week, utc_date, ymd_parts};
use crate::db::Row;
use crate::js;
use crate::serialize::col;

#[derive(Clone)]
pub(super) struct Version {
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub anchor: Option<String>,
    pub recurring: bool,
    pub frequency: Value,
    pub interval: Option<i64>,
    pub day_of_frequency: Option<i64>,
    pub month: Option<i64>,
    pub days_of_week: Vec<String>,
    pub scheduled_time: Option<String>,
    pub duration_min: Value,
    pub reminder_min: Value,
    pub title: Value,
    pub description: Value,
    pub routine_category: Value,
}

impl Version {
    pub fn with_time(&self, time: Option<String>) -> Version {
        Version {
            scheduled_time: time,
            ..self.clone()
        }
    }

    fn covers(&self, day: &str) -> bool {
        self.valid_from.as_deref().is_none_or(|f| day >= f)
            && self.valid_to.as_deref().is_none_or(|t| day <= t)
    }
}

fn string_of(v: &Value) -> Option<String> {
    (!v.is_null()).then(|| js::string(v))
}

fn days_of(raw: &Value) -> Vec<String> {
    match js::parse_json(raw, json!([])) {
        Value::Array(a) => a
            .into_iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

pub(super) fn from_segment(row: &Row) -> Version {
    Version {
        valid_from: string_of(col(row, "valid_from")),
        valid_to: string_of(col(row, "valid_to")),
        anchor: string_of(col(row, "anchor_date")),
        recurring: col(row, "is_recurring").is_null()
            || js::number(col(row, "is_recurring")).as_i64() != Some(0),
        frequency: col(row, "frequency").clone(),
        interval: col(row, "interval").as_i64(),
        day_of_frequency: col(row, "day_of_frequency").as_i64(),
        month: col(row, "month").as_i64(),
        days_of_week: days_of(col(row, "days_of_week")),
        scheduled_time: string_of(col(row, "scheduled_time")),
        duration_min: col(row, "duration_min").clone(),
        reminder_min: col(row, "reminder_min").clone(),
        title: col(row, "title").clone(),
        description: col(row, "description").clone(),
        routine_category: col(row, "routine_category").clone(),
    }
}

fn day10(v: &Value) -> Option<String> {
    (js::truthy(v)).then(|| js::string(v).chars().take(10).collect())
}

pub(super) fn frozen_until(segments: &[Version]) -> Option<String> {
    segments.iter().filter_map(|s| s.valid_to.clone()).max()
}

pub(super) fn governing<'a>(
    segments: &'a [Version],
    live: &'a Version,
    day: &str,
) -> Option<&'a Version> {
    match frozen_until(segments) {
        Some(frozen) if day <= frozen.as_str() => segments.iter().find(|s| s.covers(day)),
        _ => Some(live),
    }
}

/// `row` is a pattern joined with its routine, as `PATTERN_SQL` yields it.
pub(super) fn live_version(row: &Row, segments: &[Version], clamp_to_start: bool) -> Version {
    let frozen = frozen_until(segments);
    let start = day10(col(row, "start_at"));
    let mut valid_from = frozen.map(|f| add_days(&f, 1));
    if clamp_to_start {
        if let Some(s) = &start {
            if valid_from.as_deref().is_none_or(|v| s.as_str() > v) {
                valid_from = Some(s.clone());
            }
        }
    }
    let anchor = if start.is_none() && !segments.is_empty() {
        created_on(row)
    } else {
        start
    };
    let approx = col(row, "approximately_time");
    Version {
        valid_from,
        valid_to: day10(col(row, "end_at")),
        anchor,
        recurring: true,
        frequency: col(row, "frequency").clone(),
        interval: col(row, "interval").as_i64(),
        day_of_frequency: col(row, "day_of_frequency").as_i64(),
        month: col(row, "month").as_i64(),
        days_of_week: days_of(col(row, "days_of_week")),
        scheduled_time: string_of(col(row, "scheduled_time")),
        duration_min: if approx.is_null() {
            col(row, "duration_min").clone()
        } else {
            approx.clone()
        },
        reminder_min: col(row, "reminder_min").clone(),
        title: col(row, "title").clone(),
        description: col(row, "description").clone(),
        routine_category: col(row, "routine_category").clone(),
    }
}

pub(super) fn created_on(row: &Row) -> Option<String> {
    day10(col(row, "created_at"))
}

fn num_at(s: &str, from: usize, to: usize) -> f64 {
    js::parse_number(&s.chars().skip(from).take(to - from).collect::<String>())
}

pub(super) fn fires_on(v: &Version, day: &str, fallback_anchor: Option<&str>) -> bool {
    if !v.recurring || !v.covers(day) {
        return false;
    }
    let Some(anchor) = v
        .anchor
        .as_deref()
        .or(v.valid_from.as_deref())
        .or(fallback_anchor)
    else {
        return false;
    };
    let interval = v.interval.unwrap_or(1).max(1);
    match v.frequency.as_str().unwrap_or("").to_uppercase().as_str() {
        "DAILY" => days_between(anchor, day).is_some_and(|d| d >= 0 && d % interval == 0),
        "WEEKLY" => {
            let (Some(d), Some(a)) = (utc_date(day), utc_date(anchor)) else {
                return false;
            };
            const DOW: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
            let dow = DOW[d.weekday().num_days_from_sunday() as usize];
            if !v.days_of_week.iter().any(|k| k == dow) {
                return false;
            }
            let weeks = (start_of_week(d) - start_of_week(a)).num_days() / 7;
            weeks >= 0 && weeks % interval == 0
        }
        "MONTHLY" => {
            let Some(dof) = v.day_of_frequency else {
                return false;
            };
            if num_at(day, 8, 10) != dof as f64 {
                return false;
            }
            let (d, a) = (ymd_parts(day), ymd_parts(anchor));
            let at = |p: &[f64], i: usize| p.get(i).copied().unwrap_or(f64::NAN);
            let months = (at(&d, 0) - at(&a, 0)) * 12.0 + (at(&d, 1) - at(&a, 1));
            months >= 0.0 && months % interval as f64 == 0.0
        }
        "YEARLY" => {
            let (Some(dof), Some(month)) = (v.day_of_frequency, v.month) else {
                return false;
            };
            if num_at(day, 8, 10) != dof as f64 || num_at(day, 5, 7) != (month + 1) as f64 {
                return false;
            }
            let years = num_at(day, 0, 4) - num_at(anchor, 0, 4);
            years >= 0.0 && years % interval as f64 == 0.0
        }
        _ => false,
    }
}
