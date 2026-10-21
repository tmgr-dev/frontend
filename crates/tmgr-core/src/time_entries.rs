use crate::api::{map_error, unwrap_data, Client};
use crate::error::{Error, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::{json, Value};

pub const MODULE: &str = "session_time";
pub const SOURCE_ATTACH: &str = "attach";
pub const MAX_BATCH: usize = 100;

#[derive(Debug, Clone, PartialEq)]
pub struct Interval {
    pub client_interval_id: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub seconds: u64,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct EntryResult {
    pub client_interval_id: String,
    pub status: String,
    pub id: Option<u64>,
    pub seconds: Option<u64>,
    pub counted_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PostResult {
    pub task_id: Option<u64>,
    pub common_time: Option<u64>,
    pub entries: Vec<EntryResult>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Posted {
    Done(PostResult),
    Disabled,
}

pub fn validate_interval_id(raw: &str) -> Result<&str> {
    let ok = (8..=64).contains(&raw.len())
        && raw
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if ok {
        Ok(raw)
    } else {
        Err(Error::usage("invalid interval id")
            .hint("8 to 64 characters: letters, digits, _ and -"))
    }
}

fn stamp(t: &DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn body(intervals: &[Interval]) -> Result<Value> {
    if intervals.is_empty() || intervals.len() > MAX_BATCH {
        return Err(Error::usage(format!(
            "send 1 to {MAX_BATCH} intervals at a time"
        )));
    }
    let mut entries = Vec::with_capacity(intervals.len());
    for i in intervals {
        validate_interval_id(&i.client_interval_id)?;
        let span = (i.ended_at - i.started_at).num_seconds();
        if span <= 0 || i.seconds == 0 || i.seconds > span as u64 {
            return Err(Error::usage("invalid interval")
                .hint("start before end, and 0 < seconds <= end - start"));
        }
        entries.push(json!({
            "client_interval_id": i.client_interval_id,
            "started_at": stamp(&i.started_at),
            "ended_at": stamp(&i.ended_at),
            "seconds": i.seconds,
            "source": SOURCE_ATTACH,
        }));
    }
    Ok(json!({ "entries": entries }))
}

pub fn parse_post(v: Value) -> Result<PostResult> {
    let d = unwrap_data(v);
    let entries = d
        .get("entries")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::internal("unexpected time entries response from the server"))?
        .iter()
        .map(|e| {
            let id = e
                .get("client_interval_id")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::internal("time entry without client_interval_id"))?;
            Ok(EntryResult {
                client_interval_id: id.to_string(),
                status: e
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string(),
                id: e.get("id").and_then(Value::as_u64),
                seconds: e.get("seconds").and_then(Value::as_u64),
                counted_seconds: e.get("counted_seconds").and_then(Value::as_u64),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(PostResult {
        task_id: d.get("task_id").and_then(Value::as_u64),
        common_time: d.get("common_time").and_then(Value::as_u64),
        entries,
    })
}

pub fn module_enabled(v: &Value, key: &str) -> bool {
    let d = match v.get("data") {
        Some(inner) if inner.get("modules").is_some() => inner,
        _ => v,
    };
    d.get("modules")
        .and_then(Value::as_array)
        .and_then(|list| {
            list.iter()
                .find(|m| m.get("key").and_then(Value::as_str) == Some(key))
        })
        .and_then(|m| m.get("enabled").and_then(Value::as_bool))
        .unwrap_or(false)
}

fn is_disabled(status: u16, body: &Value) -> bool {
    status == 403 && body.get("error").and_then(Value::as_str) == Some("feature_disabled")
}

pub fn session_time_enabled(client: &Client, workspace_id: i64) -> Result<bool> {
    let (status, body) = client.call(
        "GET",
        &format!("/api/workspaces/{workspace_id}/modules"),
        None,
        true,
    )?;
    if (200..300).contains(&status) {
        return Ok(module_enabled(&body, MODULE));
    }
    Err(map_error(status, &body, "workspaces:read"))
}

pub fn post(client: &Client, task_id: u64, intervals: &[Interval]) -> Result<Posted> {
    let payload = body(intervals)?;
    let (status, resp) = client.call(
        "POST",
        &format!("/api/tasks/{task_id}/time-entries"),
        Some(&payload),
        true,
    )?;
    if is_disabled(status, &resp) {
        return Ok(Posted::Disabled);
    }
    if (200..300).contains(&status) {
        return parse_post(resp).map(Posted::Done);
    }
    Err(map_error(status, &resp, "time:write"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    fn interval(id: &str, seconds: u64) -> Interval {
        Interval {
            client_interval_id: id.into(),
            started_at: at("2026-10-21T10:00:00+02:00"),
            ended_at: at("2026-10-21T10:05:00+02:00"),
            seconds,
        }
    }

    #[test]
    fn body_is_snake_case_utc_and_marks_the_source() {
        assert_eq!(
            body(&[interval("int-0001", 240)]).unwrap(),
            json!({"entries": [{"client_interval_id": "int-0001",
                "started_at": "2026-10-21T08:00:00Z", "ended_at": "2026-10-21T08:05:00Z",
                "seconds": 240, "source": "attach"}]})
        );
    }

    #[test]
    fn body_refuses_what_the_server_would_reject() {
        assert!(body(&[]).is_err());
        assert!(body(&[interval("short", 10)]).is_err());
        assert!(body(&[interval("has space 1", 10)]).is_err());
        assert!(body(&[interval("int-0001", 0)]).is_err());
        assert!(body(&[interval("int-0001", 301)]).is_err());
        assert!(body(&[interval("int-0001", 300)]).is_ok());
        let mut backwards = interval("int-0001", 10);
        std::mem::swap(&mut backwards.started_at, &mut backwards.ended_at);
        assert!(body(&[backwards]).is_err());
        let many: Vec<_> = (0..=MAX_BATCH)
            .map(|n| interval(&format!("int-{n:04}"), 1))
            .collect();
        assert!(body(&many).is_err());
        assert!(body(&many[..MAX_BATCH]).is_ok());
    }

    #[test]
    fn post_results_are_parsed_per_entry() {
        let r = parse_post(json!({"data": {"task_id": 7, "common_time": 900, "entries": [
            {"client_interval_id": "int-0001", "status": "created", "id": 3, "seconds": 240, "counted_seconds": 180},
            {"client_interval_id": "int-0002", "status": "duplicate", "id": 2, "seconds": 60, "counted_seconds": 60},
            {"client_interval_id": "int-0003", "status": "conflict"}
        ]}}))
        .unwrap();
        assert_eq!(r.task_id, Some(7));
        assert_eq!(r.common_time, Some(900));
        assert_eq!(r.entries.len(), 3);
        assert_eq!(r.entries[0].counted_seconds, Some(180));
        assert_eq!(r.entries[2].status, "conflict");
        assert_eq!(r.entries[2].id, None);
        assert!(parse_post(json!({"data": {}})).is_err());
    }

    #[test]
    fn the_toggle_is_read_from_the_modules_payload_and_defaults_off() {
        let on = json!({"modules": [{"key": "pages", "enabled": true}, {"key": "session_time", "enabled": true}]});
        assert!(module_enabled(&on, MODULE));
        assert!(module_enabled(&json!({"data": on}), MODULE));
        let off = json!({"modules": [{"key": "session_time", "enabled": false}]});
        assert!(!module_enabled(&off, MODULE));
        assert!(!module_enabled(
            &json!({"modules": [{"key": "pages", "enabled": true}]}),
            MODULE
        ));
        assert!(!module_enabled(&json!({}), MODULE));
    }
}
