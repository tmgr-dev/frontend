use crate::rules::{workspace_code, Rule};
use rusqlite::params;
use serde_json::Value;
use tmgr_core::error::{Error, Result};
use tmgr_ipc::api::{AuthorKind, BundleAuthor, BundleComment, BundlePage, TaskRef};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Author {
    Human,
    Agent,
    Other,
    Unrecorded,
}

impl Author {
    pub fn name(self) -> &'static str {
        match self {
            Author::Human => "human",
            Author::Agent => "agent",
            Author::Other => "other",
            Author::Unrecorded => "unrecorded",
        }
    }

    pub fn bundle_kind(self) -> AuthorKind {
        match self {
            Author::Human => AuthorKind::Human,
            Author::Agent => AuthorKind::Agent,
            Author::Other | Author::Unrecorded => AuthorKind::Other,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Candidate {
    pub task: TaskRef,
    pub key: String,
    pub title: String,
    pub body: String,
    pub author: Author,
    pub comments: Vec<BundleComment>,
    pub pages: Vec<BundlePage>,
    pub context_tainted: bool,
    pub persona_instructions_sha256: String,
}

pub trait TaskSource {
    fn candidates(&self, rule: &Rule) -> Result<Vec<Candidate>>;
}

fn text(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn comment_kind(kind: &str) -> AuthorKind {
    match kind {
        "user" => AuthorKind::Human,
        "persona" | "agent" => AuthorKind::Agent,
        _ => AuthorKind::Other,
    }
}

fn utc_seconds(raw: &str) -> String {
    let head: String = raw.chars().take(19).collect();
    format!("{head}Z")
}

pub struct LocalSource {
    pub local_user_id: Option<i64>,
}

impl LocalSource {
    fn writers(workspace: &tmgr_local::LocalWorkspace) -> Result<std::collections::BTreeSet<i64>> {
        let conn = tmgr_local::open_read(&workspace.database)
            .map_err(|e| Error::internal(format!("local workspace: {e}")))?;
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT CAST(entity_id AS INTEGER) FROM activity_log
                 WHERE entity = 'tasks' AND actor_kind <> 'user' AND status < 400
                   AND method IN ('POST', 'PATCH', 'PUT', 'DELETE') AND entity_id IS NOT NULL",
            )
            .map_err(|e| Error::internal(e.to_string()))?;
        let rows = stmt
            .query_map(params![], |r| r.get::<_, i64>(0))
            .map_err(|e| Error::internal(e.to_string()))?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    }
}

impl TaskSource for LocalSource {
    fn candidates(&self, rule: &Rule) -> Result<Vec<Candidate>> {
        let code = workspace_code(&rule.workspace)?;
        let client = tmgr_core::local::client(code, self.local_user_id, "tmgr-queue", false)?;
        let workspace = tmgr_core::local::find(code)?;
        let fetch = |path: &str| -> Result<Value> {
            let (status, body) = client.call("GET", path, None, false)?;
            if status >= 400 {
                return Err(tmgr_core::local::status_error(status, &body));
            }
            Ok(tmgr_core::api::unwrap_data(body))
        };
        let wanted = match rule.status_type.as_str() {
            "backlog" => "default",
            other => other,
        };
        let statuses = fetch("/api/workspaces/statuses")?;
        let status_ids: Vec<i64> = statuses
            .as_array()
            .map(|a| {
                a.iter()
                    .filter(|s| text(s, "type") == wanted)
                    .filter_map(|s| s.get("id").and_then(Value::as_i64))
                    .collect()
            })
            .unwrap_or_default();
        let category_id = match &rule.project {
            None => None,
            Some(project) => {
                let cats = fetch("/api/project_categories?all")?;
                let found = cats.as_array().and_then(|a| {
                    a.iter()
                        .find(|c| text(c, "code").eq_ignore_ascii_case(project))
                        .and_then(|c| c.get("id").and_then(Value::as_i64))
                });
                match found {
                    Some(id) => Some(id),
                    None => return Ok(Vec::new()),
                }
            }
        };
        let writers = Self::writers(&workspace)?;
        let mut out = Vec::new();
        for status_id in status_ids {
            let listing = fetch(&format!("/api/tasks?status_id={status_id}&per_page=100"))?;
            let items = listing
                .as_array()
                .cloned()
                .or_else(|| listing.get("data").and_then(|d| d.as_array().cloned()))
                .unwrap_or_default();
            for item in items {
                let Some(id) = item.get("id").and_then(Value::as_i64) else {
                    continue;
                };
                if let Some(c) = category_id {
                    if item.get("project_category_id").and_then(Value::as_i64) != Some(c) {
                        continue;
                    }
                }
                let assigned = item
                    .get("persona_assignees")
                    .and_then(Value::as_array)
                    .is_some_and(|a| {
                        a.iter()
                            .any(|p| text(p, "name").eq_ignore_ascii_case(&rule.persona))
                    });
                if !assigned {
                    continue;
                }
                let full = fetch(&format!("/api/tasks/{id}"))?;
                let comments_raw = fetch(&format!("/api/tasks/{id}/comments"))?;
                let comments: Vec<BundleComment> = comments_raw
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|c| BundleComment {
                                author: BundleAuthor {
                                    kind: comment_kind(&text(c, "author_kind")),
                                    id: None,
                                },
                                at: utc_seconds(&text(c, "created_at")),
                                body: text(c, "message"),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let tainted = comments.iter().any(|c| c.author.kind != AuthorKind::Human);
                let key = match (
                    full.pointer("/category/code").and_then(Value::as_str),
                    full.get("category_tasks_sequence_id")
                        .and_then(Value::as_i64),
                ) {
                    (Some(c), Some(n)) => format!("{c}-{n}"),
                    _ => format!("#{id}"),
                };
                let persona_desc = item
                    .get("persona_assignees")
                    .and_then(Value::as_array)
                    .and_then(|a| {
                        a.iter()
                            .find(|p| text(p, "name").eq_ignore_ascii_case(&rule.persona))
                    })
                    .map(|p| text(p, "description"))
                    .unwrap_or_default();
                out.push(Candidate {
                    task: TaskRef {
                        workspace_id: workspace.manifest.id,
                        task_id: id,
                    },
                    key,
                    title: text(&full, "title"),
                    body: text(&full, "description"),
                    author: if writers.contains(&id) {
                        Author::Agent
                    } else {
                        Author::Human
                    },
                    comments,
                    pages: Vec::new(),
                    context_tainted: tainted,
                    persona_instructions_sha256: crate::audit::sha256_hex(persona_desc.as_bytes()),
                });
            }
        }
        out.sort_by_key(|c| c.task.task_id);
        Ok(out)
    }
}
