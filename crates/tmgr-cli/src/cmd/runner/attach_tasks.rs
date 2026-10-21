use super::run_task::local_code;
use serde_json::Value;
use tmgr_core::config::{self, Config};
use tmgr_core::text::sanitize_line;
use tmgr_queue::source::LocalTasks;
use tmgr_runner::attach::TaskOpt;

const MAX_TITLE: usize = 120;
const MAX_TASKS: usize = 200;
const PER_STATUS: usize = 100;

pub(super) const NOTE_AGENT: &str = "not available inside an agent session";
pub(super) const NOTE_LOCAL: &str = "local workspaces only, see `tmgr run`";

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}

pub(super) fn options(items: &[Value]) -> Vec<TaskOpt> {
    let mut rows: Vec<(i64, TaskOpt)> = items
        .iter()
        .filter_map(|item| {
            let id = item.get("id").and_then(Value::as_i64)?;
            let code = item.pointer("/category/code").and_then(Value::as_str)?;
            let seq = item
                .get("category_tasks_sequence_id")
                .and_then(Value::as_i64)?;
            let key = format!("{code}-{seq}");
            tmgr_core::refs::parse_key(&key)?;
            let title: String = sanitize_line(text(item, "title"))
                .chars()
                .take(MAX_TITLE)
                .collect();
            Some((id, TaskOpt { key, title }))
        })
        .collect();
    rows.sort_by_key(|(id, _)| std::cmp::Reverse(*id));
    rows.dedup_by(|a, b| a.1.key == b.1.key);
    rows.into_iter().map(|(_, t)| t).take(MAX_TASKS).collect()
}

pub(super) fn load(in_agent: bool) -> Result<Vec<TaskOpt>, String> {
    if in_agent {
        return Err(NOTE_AGENT.into());
    }
    let cfg = config::config_dir()
        .and_then(|d| Config::load(&d))
        .map_err(|e| e.message)?;
    let code = local_code(None, &cfg).map_err(|_| NOTE_LOCAL.to_string())?;
    let tasks = LocalTasks::open(&code, cfg.local_user_id).map_err(|e| e.message)?;
    let statuses = tasks
        .fetch("/api/workspaces/statuses")
        .map_err(|e| e.message)?;
    let mut items = Vec::new();
    for s in statuses.as_array().into_iter().flatten() {
        if !matches!(text(s, "type"), "default" | "active") {
            continue;
        }
        let Some(id) = s.get("id").and_then(Value::as_i64) else {
            continue;
        };
        let page = tasks
            .fetch(&format!("/api/tasks?status_id={id}&per_page={PER_STATUS}"))
            .map_err(|e| e.message)?;
        items.extend(page.as_array().cloned().unwrap_or_default());
    }
    Ok(options(&items))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn item(id: i64, code: &str, seq: i64, title: &str) -> Value {
        json!({"id": id, "category": {"code": code}, "category_tasks_sequence_id": seq, "title": title})
    }

    #[test]
    fn options_keep_keyed_tasks_newest_first() {
        let got = options(&[
            item(1, "TM", 5, "old"),
            item(9, "TM", 12, "new"),
            json!({"id": 3, "title": "no project"}),
        ]);
        let keys: Vec<&str> = got.iter().map(|t| t.key.as_str()).collect();
        assert_eq!(keys, ["TM-12", "TM-5"]);
    }

    #[test]
    fn options_strip_control_text_and_bad_keys() {
        let got = options(&[
            item(1, "TM", 5, "a\u{1b}[31mred\nline"),
            item(2, "tm; rm", 6, "bad code"),
            item(3, "TM", 7, &"x".repeat(500)),
        ]);
        assert_eq!(got.len(), 2);
        assert!(got.iter().all(|t| !t.title.chars().any(char::is_control)));
        assert!(got.iter().all(|t| t.title.chars().count() <= MAX_TITLE));
    }

    #[test]
    fn an_agent_context_never_lists_tasks() {
        assert_eq!(load(true), Err(NOTE_AGENT.to_string()));
    }
}
