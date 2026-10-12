use crate::text::sanitize_line;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct ApiRoutine {
    #[serde(default)]
    pub task_id: Option<u64>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub time: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub completed: Option<bool>,
    #[serde(default)]
    pub instance_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RoutineView {
    pub id: u64,
    pub title: String,
    pub date: Option<String>,
    pub time: Option<String>,
    pub done: bool,
    pub status: String,
    pub instance_id: Option<u64>,
}

impl RoutineView {
    pub fn from_api(r: &ApiRoutine) -> Option<Self> {
        let status = r
            .status
            .as_deref()
            .map(|s| sanitize_line(s).to_lowercase())
            .unwrap_or_else(|| "pending".to_string());
        Some(Self {
            id: r.task_id?,
            title: sanitize_line(r.title.as_deref().unwrap_or(""))
                .trim()
                .to_string(),
            date: r.date.as_deref().map(sanitize_line),
            time: r.time.as_deref().map(sanitize_line),
            done: r.completed.unwrap_or(status == "completed"),
            status,
            instance_id: r.instance_id,
        })
    }
}

pub fn views(entries: &[ApiRoutine]) -> Vec<RoutineView> {
    let mut out: Vec<RoutineView> = entries.iter().filter_map(RoutineView::from_api).collect();
    out.sort_by(|a, b| {
        (a.time.is_none(), &a.time, &a.title, a.id).cmp(&(
            b.time.is_none(),
            &b.time,
            &b.title,
            b.id,
        ))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routines_sort_by_time_with_untimed_last_and_drop_entries_without_a_task() {
        let entries: Vec<ApiRoutine> = serde_json::from_value(serde_json::json!([
            {"task_id": 3, "title": "Weekly\u{1b}[31m review", "status": "pending"},
            {"task_id": 2, "title": "Inbox", "time": "12:30", "status": "completed", "completed": true},
            {"task_id": 1, "title": "Plan", "time": "09:00", "status": "PENDING"},
            {"title": "orphan"}
        ]))
        .unwrap();
        let v = views(&entries);
        assert_eq!(v.iter().map(|r| r.id).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert!(v[1].done && !v[0].done);
        assert_eq!(v[2].title, "Weekly [31m review");
        assert_eq!(v[0].status, "pending");
    }
}
