use crate::rules::{workspace_code, Rule};
use serde_json::Value;
use std::collections::BTreeMap;
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
    pub persona_text: String,
}

pub struct Scan {
    pub candidates: Vec<Candidate>,
    pub complete: bool,
}

pub trait TaskSource {
    fn candidates(&self, rule: &Rule) -> Result<Scan>;
}

pub const MAX_PAGES: usize = 50;

fn text(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

fn utc_seconds(raw: &str) -> String {
    let head: String = raw.chars().take(19).collect();
    format!("{head}Z")
}

pub struct LocalSource {
    pub local_user_id: Option<i64>,
}

#[derive(Default)]
struct Trail {
    owner_created: bool,
    foreign: Option<Author>,
    latest: String,
}

impl LocalSource {
    fn trails(workspace: &tmgr_local::LocalWorkspace, owner: &str) -> Result<BTreeMap<i64, Trail>> {
        let conn = tmgr_local::open_read(&workspace.database)
            .map_err(|e| Error::internal(format!("local workspace: {e}")))?;
        let mut stmt = conn
            .prepare(
                "SELECT CAST(entity_id AS INTEGER), actor_kind, actor_id, method, at FROM activity_log
                 WHERE entity = 'tasks' AND status < 400 AND entity_id IS NOT NULL
                   AND method IN ('POST', 'PATCH', 'PUT', 'DELETE') ORDER BY id",
            )
            .map_err(|e| Error::internal(e.to_string()))?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                    r.get::<_, String>(4)?,
                ))
            })
            .map_err(|e| Error::internal(e.to_string()))?;
        let mut out: BTreeMap<i64, Trail> = BTreeMap::new();
        for row in rows {
            let (task, kind, actor, method, at) =
                row.map_err(|e| Error::internal(e.to_string()))?;
            let t = out.entry(task).or_default();
            if at > t.latest {
                t.latest = at;
            }
            if kind == "user" && actor == owner {
                if method == "POST" {
                    t.owner_created = true;
                }
            } else {
                let found = if matches!(kind.as_str(), "persona" | "agent") {
                    Author::Agent
                } else {
                    Author::Other
                };
                if t.foreign != Some(Author::Agent) {
                    t.foreign = Some(found);
                }
            }
        }
        Ok(out)
    }
}

fn list_all(client: &tmgr_core::api::Client, base: &str) -> Result<(Vec<Value>, bool)> {
    let mut items = Vec::new();
    for page in 1..=MAX_PAGES {
        let sep = if base.contains('?') { '&' } else { '?' };
        let (status, body) = client.call(
            "GET",
            &format!("{base}{sep}per_page=100&page={page}"),
            None,
            false,
        )?;
        if status >= 400 {
            return Err(tmgr_core::local::status_error(status, &body));
        }
        let rows = body
            .get("data")
            .and_then(Value::as_array)
            .or_else(|| body.as_array())
            .cloned()
            .unwrap_or_default();
        items.extend(rows);
        let last = body
            .pointer("/meta/last_page")
            .and_then(Value::as_u64)
            .unwrap_or(1) as usize;
        if page >= last {
            return Ok((items, true));
        }
    }
    Ok((items, false))
}

impl TaskSource for LocalSource {
    fn candidates(&self, rule: &Rule) -> Result<Scan> {
        let code = workspace_code(&rule.workspace)?;
        let client = tmgr_core::local::client(code, self.local_user_id, "tmgr-queue", false)?;
        let workspace = tmgr_core::local::find(code)?;
        let owner = tmgr_core::local::identity(&tmgr_core::local::root()?, self.local_user_id)
            .id
            .to_string();
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
                    None => {
                        return Ok(Scan {
                            candidates: Vec::new(),
                            complete: true,
                        })
                    }
                }
            }
        };
        let trails = Self::trails(&workspace, &owner)?;
        let mut out = Vec::new();
        let mut complete = true;
        for status_id in status_ids {
            let (items, done) = list_all(&client, &format!("/api/tasks?status_id={status_id}"))?;
            complete &= done;
            for item in items {
                let Some(id) = item.get("id").and_then(Value::as_i64) else {
                    continue;
                };
                if let Some(c) = category_id {
                    if item.get("project_category_id").and_then(Value::as_i64) != Some(c) {
                        continue;
                    }
                }
                let persona = item
                    .get("persona_assignees")
                    .and_then(Value::as_array)
                    .and_then(|a| {
                        a.iter()
                            .find(|p| text(p, "name").eq_ignore_ascii_case(&rule.persona))
                    });
                let Some(persona) = persona else {
                    continue;
                };
                let full = fetch(&format!("/api/tasks/{id}"))?;
                let comments_raw = fetch(&format!("/api/tasks/{id}/comments"))?;
                let comments: Vec<BundleComment> = comments_raw
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|c| {
                                let kind = text(c, "author_kind");
                                let human = kind == "user" && text(c, "author_id") == owner;
                                BundleComment {
                                    author: BundleAuthor {
                                        kind: if human {
                                            AuthorKind::Human
                                        } else if matches!(kind.as_str(), "persona" | "agent") {
                                            AuthorKind::Agent
                                        } else {
                                            AuthorKind::Other
                                        },
                                        id: None,
                                    },
                                    at: utc_seconds(&text(c, "created_at")),
                                    body: text(c, "message"),
                                }
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
                let author = match trails.get(&id) {
                    None => Author::Unrecorded,
                    Some(t) => match t.foreign {
                        Some(a) => a,
                        None if t.owner_created && text(&full, "updated_at") <= t.latest => {
                            Author::Human
                        }
                        None => Author::Unrecorded,
                    },
                };
                out.push(Candidate {
                    task: TaskRef {
                        workspace_id: workspace.manifest.id,
                        task_id: id,
                    },
                    key,
                    title: text(&full, "title"),
                    body: text(&full, "description"),
                    author,
                    comments,
                    pages: Vec::new(),
                    context_tainted: tainted,
                    persona_text: text(persona, "description"),
                });
            }
        }
        out.sort_by_key(|c| c.task.task_id);
        Ok(Scan {
            candidates: out,
            complete,
        })
    }
}
