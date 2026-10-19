use crate::rules::Rule;
use crate::source::{Author, Candidate, Refusal, Scan, TaskSource};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::{CloudApproval, TaskRef};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CloudClaim {
    pub claim: CloudApproval,
    pub workspace_code: String,
    pub branch: String,
}

#[derive(Clone, Debug)]
pub struct CloudWorkspace {
    pub id: i64,
    pub code: String,
    pub repos: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct CloudTask {
    pub key: String,
    pub title: String,
    pub body: String,
    pub claim: CloudApproval,
}

#[derive(Clone, Debug)]
pub enum Gated {
    Ready(CloudTask),
    Refused { key: String, reason: String },
}

pub trait CloudApi {
    /// The live allow entry of the workspace; the error text names the condition and the fix.
    fn allow(&self, code: &str, now: DateTime<Utc>) -> std::result::Result<CloudWorkspace, String>;
    /// Ids of the rule's candidate tasks and whether the listing was complete.
    fn list(&self, rule: &Rule, ws: &CloudWorkspace) -> Result<(Vec<i64>, bool)>;
    /// The same gate `tmgr run` applies, plus the bound repository of the task's category being the rule's.
    fn gate(&self, rule: &Rule, ws: &CloudWorkspace, task_id: i64) -> Result<Gated>;
}

pub struct CloudSource<'a> {
    pub api: &'a dyn CloudApi,
    pub now: DateTime<Utc>,
}

pub fn repo_check(rule: &Rule, ws: &CloudWorkspace) -> std::result::Result<(), String> {
    if ws.repos.contains(&rule.repo) {
        Ok(())
    } else {
        Err(format!(
            "repo {} is not bound to a category of workspace {}; run `tmgr run <task>` once in a terminal to bind it",
            rule.repo, ws.code
        ))
    }
}

impl TaskSource for CloudSource<'_> {
    fn candidates(&self, rule: &Rule) -> Result<Scan> {
        let ws = self
            .api
            .allow(&rule.workspace, self.now)
            .map_err(|m| Error::new(Kind::Forbidden, m))?;
        let (ids, mut complete) = self.api.list(rule, &ws)?;
        let mut candidates = Vec::new();
        let mut refusals = Vec::new();
        for id in ids {
            match self.api.gate(rule, &ws, id) {
                Ok(Gated::Ready(t)) => {
                    let branch = crate::publish::branch_slug(&t.key, &t.title);
                    candidates.push(Candidate {
                        task: TaskRef {
                            workspace_id: ws.id,
                            task_id: id,
                        },
                        key: t.key,
                        title: t.title,
                        body: t.body,
                        author: Author::Human,
                        comments: Vec::new(),
                        pages: Vec::new(),
                        context_tainted: false,
                        persona_text: String::new(),
                        cloud: Some(CloudClaim {
                            claim: t.claim,
                            workspace_code: ws.code.clone(),
                            branch,
                        }),
                    });
                }
                Ok(Gated::Refused { key, reason }) => refusals.push(Refusal {
                    task_id: id,
                    key,
                    reason,
                }),
                Err(_) => complete = false,
            }
        }
        candidates.sort_by_key(|c| c.task.task_id);
        Ok(Scan {
            candidates,
            complete,
            refusals,
        })
    }
}
