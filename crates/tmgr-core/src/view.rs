use crate::models::{ApiComment, ApiTask, Status};
use crate::text::{sanitize_line, truncate_chars};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct StatusView {
    pub id: Option<u64>,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
}

impl StatusView {
    pub fn from_status(s: &Status) -> Self {
        Self {
            id: Some(s.id),
            name: sanitize_line(&s.name),
            kind: sanitize_line(&s.kind),
        }
    }

    pub fn resolve(task: &ApiTask, statuses: &[Status]) -> Self {
        task.status_id
            .and_then(|id| statuses.iter().find(|s| s.id == id))
            .map(StatusView::from_status)
            .unwrap_or_else(|| StatusView {
                id: task.status_id,
                name: sanitize_line(task.status.as_deref().unwrap_or("unknown")),
                kind: sanitize_line(task.status.as_deref().unwrap_or("unknown")),
            })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct CategoryView {
    pub id: Option<u64>,
    pub code: Option<String>,
    pub title: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PersonView {
    pub id: Option<u64>,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct CommentView {
    pub id: u64,
    pub author: Option<PersonView>,
    pub message: String,
    pub created_at: Option<String>,
}

pub const MAX_COMMENT_CHARS: usize = 2000;

impl CommentView {
    pub fn from_api(c: &ApiComment) -> Self {
        Self {
            id: c.id,
            author: c.user.as_ref().map(|u| PersonView {
                id: u.id,
                name: sanitize_line(u.name.as_deref().unwrap_or("unknown")),
            }),
            message: truncate_chars(&c.message, MAX_COMMENT_CHARS).0,
            created_at: c.created_at.clone(),
        }
    }
}

pub fn visible_key(title: &str) -> Option<&str> {
    let (head, _) = title.split_once(':')?;
    crate::refs::parse_key(head)
}

pub fn task_ref(task: &ApiTask) -> String {
    if let Some(key) = visible_key(&task.title) {
        return key.to_string();
    }
    if let (Some(code), Some(seq)) = (
        task.category.as_ref().and_then(|c| c.code.as_deref()),
        task.category_tasks_sequence_id,
    ) {
        let candidate = format!("{code}-{seq}");
        if crate::refs::parse_key(&candidate).is_some() {
            return candidate;
        }
    }
    format!("#{}", task.id)
}

pub fn display_title(title: &str) -> String {
    let clean = match visible_key(title) {
        Some(key) => title[key.len() + 1..].trim_start(),
        None => title,
    };
    sanitize_line(clean)
}

pub fn task_url(web_base: &str, workspace_code: &str, id: u64) -> String {
    format!("{web_base}/{workspace_code}/tasks/{id}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Category;

    fn task(title: &str, code: Option<&str>, seq: Option<u64>) -> ApiTask {
        ApiTask {
            id: 77,
            title: title.into(),
            description: None,
            status: None,
            status_id: None,
            project_category_id: None,
            category_tasks_sequence_id: seq,
            category: code.map(|c| Category {
                id: 1,
                title: "T".into(),
                code: Some(c.into()),
            }),
            workspace_id: None,
            common_time: None,
            start_time: None,
            approximately_time: None,
            priority: None,
            assignees: vec![],
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn ref_prefers_visible_title_prefix_over_sequence() {
        assert_eq!(
            task_ref(&task("TM-509: Fix", Some("TM"), Some(342))),
            "TM-509"
        );
        assert_eq!(task_ref(&task("Fix", Some("TM"), Some(342))), "TM-342");
        assert_eq!(task_ref(&task("Fix", None, None)), "#77");
        assert_eq!(task_ref(&task("fix: this", Some("tm"), Some(1))), "#77");
    }

    #[test]
    fn title_prefix_is_stripped_and_sanitized() {
        assert_eq!(display_title("TM-509: Fix \u{1b}[31mit"), "Fix  [31mit");
        assert_eq!(display_title("Note: plain"), "Note: plain");
    }

    #[test]
    fn status_falls_back_to_the_task_field() {
        let mut t = task("x", None, None);
        t.status_id = Some(5);
        t.status = Some("active".into());
        let s = StatusView::resolve(&t, &[]);
        assert_eq!(
            (s.id, s.name.as_str(), s.kind.as_str()),
            (Some(5), "active", "active")
        );
        let st = Status {
            id: 5,
            name: "In progress".into(),
            kind: "active".into(),
        };
        assert_eq!(StatusView::resolve(&t, &[st]).name, "In progress");
    }
}
