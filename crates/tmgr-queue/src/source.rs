use crate::rules::{workspace_code, Rule};
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

/// Run bundles carry only what the journal's snapshot covers: title, description, editor data, checkpoints
/// and comments. Task files and settings are never given to an agent, and a local task brings no pages. The
/// exhaustive destructuring fails to compile when `Candidate` gains a field, so a new input has to be
/// reviewed against the snapshot before it can reach a bundle.
pub fn assert_snapshot_scope(c: &Candidate) -> Result<()> {
    let Candidate {
        task: _,
        key: _,
        title: _,
        body: _,
        author: _,
        comments: _,
        pages,
        context_tainted: _,
        persona_text: _,
    } = c;
    if !pages.is_empty() {
        return Err(Error::internal(
            "a local task must not bring pages into a run bundle: they are outside the confirmed snapshot",
        ));
    }
    Ok(())
}

/// The API answers `description_json` parsed, the journal hashes the stored text: the stored text stands in
/// for it only when it parses to exactly what the API showed, otherwise the API's own serialization is used
/// and will not match.
fn observed_json(shown: Option<&Value>, stored: Option<String>) -> String {
    let shown = shown.cloned().unwrap_or(Value::Null);
    let blank = shown.is_null() || shown.as_array().is_some_and(Vec::is_empty);
    if blank && stored.as_deref().is_some_and(str::is_empty) {
        return String::new();
    }
    if let Some(stored) = stored {
        let parsed = if stored.is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&stored).unwrap_or(Value::String(stored.clone()))
        };
        if parsed == shown {
            return stored;
        }
    }
    match shown {
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn author_of(p: &tmgr_local::Provenance) -> Author {
    if !p.chain_ok {
        return Author::Unrecorded;
    }
    if p.is_human_written() {
        return Author::Human;
    }
    if p.created_by == "unrecorded" {
        return Author::Unrecorded;
    }
    let by = |kind: &str| p.modified_by_kinds.iter().any(|k| k == kind);
    match p.created_by.as_str() {
        "agent" => Author::Agent,
        _ if by("agent") => Author::Agent,
        "plugin" => Author::Other,
        _ if p.modified_by_non_human_since => Author::Other,
        _ => Author::Unrecorded,
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

/// A local workspace opened for reading tasks the way the queue snapshots them: the edit journal is
/// verified once and every task's author comes from it.
pub struct LocalTasks {
    client: tmgr_core::api::Client,
    workspace: tmgr_core::local::LocalWorkspace,
    owner: String,
    journal: rusqlite::Connection,
    chain: tmgr_local::ChainStatus,
}

impl LocalTasks {
    pub fn open(code: &str, local_user_id: Option<i64>) -> Result<LocalTasks> {
        let client = tmgr_core::local::client(code, local_user_id, "tmgr-queue", false)?;
        let workspace = tmgr_core::local::find(code)?;
        let owner = tmgr_core::local::identity(&tmgr_core::local::root()?, local_user_id)
            .id
            .to_string();
        let journal = tmgr_local::open_read(&workspace.database)
            .map_err(|e| Error::internal(format!("local workspace: {e}")))?;
        let chain = tmgr_local::verify_chain(&journal)
            .map_err(|e| Error::internal(format!("local workspace: {e}")))?;
        if let Ok(writer) = tmgr_local::open_write(&workspace.database) {
            let _ = tmgr_local::store_checkpoint(&writer, &chain);
        }
        Ok(LocalTasks {
            client,
            workspace,
            owner,
            journal,
            chain,
        })
    }

    pub fn chain_ok(&self) -> bool {
        self.chain.ok
    }

    pub fn fetch(&self, path: &str) -> Result<Value> {
        let (status, body) = self.client.call("GET", path, None, false)?;
        if status >= 400 {
            return Err(tmgr_core::local::status_error(status, &body));
        }
        Ok(tmgr_core::api::unwrap_data(body))
    }

    /// The task as a run bundle would carry it; `persona` is the persona assignee the run is for.
    pub fn candidate(&self, id: i64, persona: &Value) -> Result<Candidate> {
        let fetch = |path: &str| self.fetch(path);
        let owner = &self.owner;
        let journal = &self.journal;
        let chain = &self.chain;
        let workspace = &self.workspace;
        let full = fetch(&format!("/api/tasks/{id}"))?;
        let comments_raw = fetch(&format!("/api/tasks/{id}/comments"))?;
        let comments: Vec<BundleComment> = comments_raw
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|c| {
                        let kind = text(c, "author_kind");
                        let human = kind == "user" && text(c, "author_id") == *owner;
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
        let title = text(&full, "title");
        let body = text(&full, "description");
        let stored = tmgr_local::snapshot(journal, id).ok().flatten();
        let description_json = observed_json(
            full.get("description_json"),
            stored.as_ref().map(|s| s.description_json.clone()),
        );
        let checkpoints = observed_json(full.get("checkpoints"), stored.map(|s| s.checkpoints));
        let seen: Vec<(i64, String)> = comments_raw
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|c| Some((c.get("id")?.as_i64()?, text(c, "message"))))
                    .collect()
            })
            .unwrap_or_default();
        let observed = tmgr_local::Observed {
            title: &title,
            description: &body,
            description_json: &description_json,
            checkpoints: &checkpoints,
            comments: seen.iter().map(|(i, m)| (*i, m.as_str())).collect(),
        };
        let author = match tmgr_local::provenance_in(journal, id, Some(&observed), chain) {
            Ok(p) => author_of(&p),
            Err(_) => Author::Unrecorded,
        };
        let candidate = Candidate {
            task: TaskRef {
                workspace_id: workspace.manifest.id,
                task_id: id,
            },
            key,
            title,
            body,
            author,
            comments,
            pages: Vec::new(),
            context_tainted: tainted,
            persona_text: text(persona, "description"),
        };
        assert_snapshot_scope(&candidate)?;
        Ok(candidate)
    }
}

impl TaskSource for LocalSource {
    fn candidates(&self, rule: &Rule) -> Result<Scan> {
        let code = workspace_code(&rule.workspace)?;
        let local = LocalTasks::open(code, self.local_user_id)?;
        let fetch = |path: &str| local.fetch(path);
        let client = &local.client;
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
        let mut out = Vec::new();
        let mut complete = true;
        for status_id in status_ids {
            let (items, done) = list_all(client, &format!("/api/tasks?status_id={status_id}"))?;
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
                let candidate = local.candidate(id, persona)?;
                out.push(candidate);
            }
        }
        out.sort_by_key(|c| c.task.task_id);
        Ok(Scan {
            candidates: out,
            complete,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate() -> Candidate {
        Candidate {
            task: TaskRef {
                workspace_id: 1,
                task_id: 1,
            },
            key: "WK-1".into(),
            title: "t".into(),
            body: "b".into(),
            author: Author::Human,
            comments: Vec::new(),
            pages: Vec::new(),
            context_tainted: false,
            persona_text: String::new(),
        }
    }

    #[test]
    fn nothing_outside_the_snapshot_reaches_a_bundle() {
        assert!(assert_snapshot_scope(&candidate()).is_ok());
        let mut with_page = candidate();
        with_page.pages.push(BundlePage {
            id: 1,
            revision: 1,
            title: "p".into(),
            body: "page text".into(),
        });
        assert!(assert_snapshot_scope(&with_page).is_err());
    }

    #[test]
    fn the_stored_text_stands_in_for_parsed_json_only_when_it_parses_to_what_was_shown() {
        let shown = serde_json::json!([{ "description": "a" }]);
        let stored = "[ {\"description\":\"a\"} ]".to_string();
        assert_eq!(observed_json(Some(&shown), Some(stored.clone())), stored);
        assert_ne!(
            observed_json(Some(&shown), Some("[{\"description\":\"b\"}]".into())),
            "[{\"description\":\"b\"}]"
        );
        assert_eq!(
            observed_json(Some(&serde_json::json!([])), Some(String::new())),
            ""
        );
        assert_eq!(observed_json(None, Some(String::new())), "");
    }
}
