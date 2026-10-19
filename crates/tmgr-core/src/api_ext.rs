use crate::api::{items, map_error, parse, unexpected, unwrap_data, Client};
use crate::error::Result;
use crate::pages::ApiPage;
use crate::routines::ApiRoutine;
use crate::runs::ApiRun;
use reqwest::Method;
use serde_json::{json, Value};

pub struct TimeItem {
    pub task_id: u64,
    pub key: String,
    pub title: String,
    pub seconds: i64,
}

pub struct TimeSummary {
    pub total_s: i64,
    pub items: Vec<TimeItem>,
}

#[derive(Default, Clone)]
pub struct RunQuery {
    pub since: Option<String>,
    pub until: Option<String>,
    pub task_id: Option<u64>,
    pub agent: Option<String>,
    pub limit: Option<u32>,
    pub cursor: Option<String>,
}

pub struct RunsPage {
    pub runs: Vec<ApiRun>,
    pub next_cursor: Option<String>,
}

pub enum PageUpdate {
    Updated(ApiPage),
    Conflict(Option<ApiPage>),
}

fn cursor_of(body: &Value) -> Option<String> {
    [
        body.get("next_cursor"),
        body.get("meta").and_then(|m| m.get("next_cursor")),
    ]
    .into_iter()
    .flatten()
    .find_map(|v| v.as_str().filter(|s| !s.is_empty()).map(str::to_string))
}

pub fn parse_runs_page(body: Value) -> Result<RunsPage> {
    let next_cursor = cursor_of(&body);
    let list = match &body {
        Value::Array(_) => body.clone(),
        Value::Object(m) => match m.get("runs").or_else(|| m.get("data")) {
            Some(Value::Object(inner)) => inner.get("runs").cloned().unwrap_or(Value::Null),
            Some(other) => other.clone(),
            None => Value::Null,
        },
        _ => Value::Null,
    };
    let next_cursor = next_cursor.or_else(|| {
        body.get("data")
            .filter(|d| d.is_object())
            .and_then(cursor_of)
    });
    Ok(RunsPage {
        runs: items(list)?,
        next_cursor,
    })
}

impl Client {
    pub fn time_summary(&self, workspace_id: i64, from: &str, to: &str) -> Result<TimeSummary> {
        let body = self.request(
            Method::GET,
            &format!("/api/workspaces/{workspace_id}/time-summary"),
            &[
                ("from", from.to_string()),
                ("to", to.to_string()),
                ("group_by", "task".to_string()),
            ],
            None,
            "time:read",
        )?;
        let data = unwrap_data(body);
        let total_s = data
            .get("total_s")
            .or_else(|| data.get("total_seconds"))
            .and_then(Value::as_i64)
            .ok_or_else(unexpected)?;
        let items = data
            .get("items")
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| {
                        Some(TimeItem {
                            task_id: r.get("task_id")?.as_u64()?,
                            key: r.get("key").and_then(Value::as_str).unwrap_or("").into(),
                            title: r.get("title").and_then(Value::as_str).unwrap_or("").into(),
                            seconds: r.get("seconds").and_then(Value::as_i64).unwrap_or(0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();
        Ok(TimeSummary { total_s, items })
    }

    pub fn workspace_agent_runs(
        &self,
        workspace_id: i64,
        q: &RunQuery,
    ) -> Result<Option<RunsPage>> {
        let mut query: Vec<(&str, String)> = Vec::new();
        if let Some(v) = &q.since {
            query.push(("since", v.clone()));
        }
        if let Some(v) = &q.until {
            query.push(("until", v.clone()));
        }
        if let Some(v) = q.task_id {
            query.push(("task_id", v.to_string()));
        }
        if let Some(v) = &q.agent {
            query.push(("agent", v.clone()));
        }
        if let Some(v) = q.limit {
            query.push(("limit", v.to_string()));
        }
        if let Some(v) = &q.cursor {
            query.push(("cursor", v.clone()));
        }
        let (status, body) = self.request_status(
            Method::GET,
            &format!("/api/workspaces/{workspace_id}/agent-work"),
            &query,
            None,
        )?;
        if status == 403 && body.get("error").and_then(Value::as_str) == Some("feature_disabled") {
            return Ok(None);
        }
        if !(200..300).contains(&status) {
            return Err(map_error(status, &body, "tasks:read"));
        }
        parse_runs_page(body).map(Some)
    }

    pub fn routines_expand(&self, from: &str, to: &str) -> Result<Vec<ApiRoutine>> {
        items(self.request(
            Method::GET,
            "/api/daily-routines/expand",
            &[("from", from.to_string()), ("to", to.to_string())],
            None,
            "tasks:read",
        )?)
    }

    pub fn complete_routine_today(&self, task_id: u64) -> Result<()> {
        self.request(
            Method::POST,
            &format!("/api/daily-routines/tasks/{task_id}/complete"),
            &[],
            None,
            "tasks:write",
        )
        .map(|_| ())
    }

    pub fn complete_routine_instance(&self, task_id: u64, instance_id: u64) -> Result<()> {
        self.request(
            Method::POST,
            &format!("/api/daily-routines/tasks/{task_id}/instances/{instance_id}/complete"),
            &[],
            None,
            "tasks:write",
        )
        .map(|_| ())
    }

    fn in_workspace<T>(&self, workspace_id: i64, call: impl FnOnce() -> Result<T>) -> Result<T> {
        self.set_workspace_header(Some(workspace_id));
        let out = call();
        self.set_workspace_header(None);
        out
    }

    pub fn agent_run_keys(&self) -> Result<Value> {
        Ok(unwrap_data(self.request(
            Method::GET,
            "/api/agent-runs/keys",
            &[],
            None,
            "tasks:read",
        )?))
    }

    pub fn agent_run_task(&self, task_id: i64) -> Result<Value> {
        Ok(unwrap_data(self.request(
            Method::GET,
            &format!("/api/agent-runs/tasks/{task_id}"),
            &[],
            None,
            "tasks:read",
        )?))
    }

    pub fn pages_flat(&self, workspace_id: i64) -> Result<Vec<ApiPage>> {
        self.in_workspace(workspace_id, || {
            items(self.request(Method::GET, "/api/pages/tree", &[], None, "pages:read")?)
        })
    }

    pub fn page_get(&self, workspace_id: i64, id: u64) -> Result<ApiPage> {
        self.in_workspace(workspace_id, || {
            parse(unwrap_data(self.request(
                Method::GET,
                &format!("/api/pages/{id}"),
                &[],
                None,
                "pages:read",
            )?))
        })
    }

    pub fn page_create(&self, workspace_id: i64, body: &Value) -> Result<ApiPage> {
        self.in_workspace(workspace_id, || {
            parse(unwrap_data(self.request(
                Method::POST,
                "/api/pages",
                &[],
                Some(body),
                "pages:write",
            )?))
        })
    }

    pub fn page_update(
        &self,
        workspace_id: i64,
        id: u64,
        version: i64,
        body: &str,
    ) -> Result<PageUpdate> {
        self.in_workspace(workspace_id, || {
            let (status, value) = self.request_status(
                Method::PATCH,
                &format!("/api/pages/{id}"),
                &[],
                Some(&json!({"version": version, "body": body})),
            )?;
            if (200..300).contains(&status) {
                return parse(unwrap_data(value)).map(PageUpdate::Updated);
            }
            if status == 409 {
                let current = parse::<ApiPage>(unwrap_data(value)).ok();
                return Ok(PageUpdate::Conflict(current));
            }
            Err(map_error(status, &value, "pages:write"))
        })
    }
}

pub struct DailyUser {
    pub id: Option<u64>,
    pub name: String,
    pub seconds: i64,
}

pub struct DailyItem {
    pub task_id: u64,
    pub key: String,
    pub title: String,
    pub url: Option<String>,
    pub status: Option<String>,
    pub category: Option<String>,
    pub seconds: i64,
    pub users: Vec<DailyUser>,
}

pub struct DailyTime {
    pub date: String,
    pub total_s: i64,
    pub items: Vec<DailyItem>,
}

fn str_of(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

pub fn parse_daily_time(body: Value, date: &str) -> Result<DailyTime> {
    let data = unwrap_data(body);
    let days = data
        .get("days")
        .and_then(Value::as_array)
        .ok_or_else(unexpected)?;
    let Some(day) = days
        .iter()
        .find(|d| d.get("date").and_then(Value::as_str) == Some(date))
    else {
        return Ok(DailyTime {
            date: date.to_string(),
            total_s: 0,
            items: Vec::new(),
        });
    };
    let items: Vec<DailyItem> = day
        .get("items")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|r| {
                    let task = r.get("task")?;
                    Some(DailyItem {
                        task_id: task.get("id")?.as_u64()?,
                        key: str_of(task, "key").unwrap_or_default(),
                        title: str_of(task, "title").unwrap_or_default(),
                        url: str_of(task, "url"),
                        status: r.get("status").and_then(|s| str_of(s, "name")),
                        category: r.get("category").and_then(|c| str_of(c, "name")),
                        seconds: r.get("seconds").and_then(Value::as_i64).unwrap_or(0),
                        users: r
                            .get("users")
                            .and_then(Value::as_array)
                            .map(|us| {
                                us.iter()
                                    .map(|u| DailyUser {
                                        id: u.get("id").and_then(Value::as_u64),
                                        name: str_of(u, "name").unwrap_or_default(),
                                        seconds: u
                                            .get("seconds")
                                            .and_then(Value::as_i64)
                                            .unwrap_or(0),
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    let total_s = day
        .get("total_s")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| items.iter().map(|i| i.seconds).sum());
    Ok(DailyTime {
        date: date.to_string(),
        total_s,
        items,
    })
}

impl Client {
    pub fn time_daily(
        &self,
        workspace_id: i64,
        date: &str,
        tz: &str,
        mine: bool,
    ) -> Result<Option<DailyTime>> {
        // Local workspaces have no per-day endpoint; callers fall back to the time summary.
        if self.local().is_some() {
            return Ok(None);
        }
        let (status, body) = self.request_status(
            Method::GET,
            &format!("/api/workspaces/{workspace_id}/time/daily"),
            &[
                ("from", date.to_string()),
                ("to", date.to_string()),
                ("tz", tz.to_string()),
                ("mine", mine.to_string()),
            ],
            None,
        )?;
        if status == 404 || status == 405 {
            return Ok(None);
        }
        if !(200..300).contains(&status) {
            return Err(map_error(status, &body, "time:read"));
        }
        parse_daily_time(body, date).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_pages_accept_the_plausible_envelopes() {
        let run = json!({"id": 1, "agent": "codex"});
        for body in [
            json!([run]),
            json!({"data": [run], "next_cursor": "c1"}),
            json!({"runs": [run], "meta": {"next_cursor": "c1"}}),
            json!({"data": {"runs": [run], "next_cursor": "c1"}}),
        ] {
            let page = parse_runs_page(body.clone()).unwrap();
            assert_eq!(page.runs.len(), 1, "{body}");
            assert_eq!(
                page.next_cursor.as_deref(),
                if body.is_array() { None } else { Some("c1") },
                "{body}"
            );
        }
        assert_eq!(parse_runs_page(json!({"data": []})).unwrap().runs.len(), 0);
    }
}
