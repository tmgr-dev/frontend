use crate::rules::Rule;
use serde_json::Value;
use tmgr_core::error::Result;
use tmgr_core::text::sanitize_line;
use tmgr_ipc::api::TaskRef;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Holder {
    pub name: String,
    pub kind: &'static str,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fresh {
    pub holders: Vec<Holder>,
    pub status_ok: bool,
}

impl Fresh {
    pub fn free(&self) -> bool {
        self.holders.is_empty() && self.status_ok
    }

    pub fn fingerprint(&self) -> String {
        let mut parts: Vec<String> = self
            .holders
            .iter()
            .map(|h| format!("{}:{}", h.kind, h.name))
            .collect();
        parts.sort();
        if !self.status_ok {
            parts.push("status".into());
        }
        format!("taken:{}", parts.join("|"))
    }

    pub fn comment(&self) -> String {
        let who: Vec<String> = self
            .holders
            .iter()
            .map(|h| format!("{} ({})", sanitize_line(&h.name), h.kind))
            .collect();
        format!(
            "Auto-run skipped: this task was taken by {}.",
            who.join(", ")
        )
    }
}

pub struct Facts<'a> {
    pub rule_persona: &'a str,
    pub me: i64,
    pub owners: Vec<i64>,
    pub assignees: Vec<(Option<i64>, String)>,
    pub personas: Vec<String>,
    pub running_agents: Vec<String>,
    pub status_ok: bool,
}

pub fn assess(f: Facts) -> Fresh {
    let mut holders = Vec::new();
    for (id, name) in f.assignees {
        let own = id.is_some_and(|i| i == f.me || f.owners.contains(&i));
        if !own {
            holders.push(Holder {
                name,
                kind: "assignee",
            });
        }
    }
    for name in f.personas {
        if !name.eq_ignore_ascii_case(f.rule_persona) {
            holders.push(Holder {
                name,
                kind: "persona",
            });
        }
    }
    for name in f.running_agents {
        holders.push(Holder {
            name,
            kind: "agent run",
        });
    }
    Fresh {
        holders,
        status_ok: f.status_ok,
    }
}

fn str_of(v: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|k| v.get(*k).and_then(Value::as_str))
        .map(str::to_string)
}

pub fn people_of(task: &Value) -> Vec<(Option<i64>, String)> {
    task.get("assignees")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|p| {
                    (
                        p.get("id").and_then(Value::as_i64),
                        str_of(p, &["name"]).unwrap_or_else(|| "unknown".into()),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn personas_of(task: &Value) -> Vec<String> {
    task.get("persona_assignees")
        .or_else(|| task.get("personaAssignees"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|p| str_of(p, &["name"]).unwrap_or_else(|| "unknown".into()))
                .collect()
        })
        .unwrap_or_default()
}

pub fn owners_of(task: &Value, rule_persona: &str) -> Vec<i64> {
    task.get("persona_assignees")
        .or_else(|| task.get("personaAssignees"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter(|p| {
                    str_of(p, &["name"]).is_some_and(|n| n.eq_ignore_ascii_case(rule_persona))
                })
                .filter_map(|p| p.pointer("/owner/id").and_then(Value::as_i64))
                .collect()
        })
        .unwrap_or_default()
}

pub fn running_agents_of(body: &Value) -> Vec<String> {
    let list = body
        .get("runs")
        .and_then(Value::as_array)
        .or_else(|| body.as_array());
    list.map(|runs| {
        runs.iter()
            .filter(|r| r.get("status").and_then(Value::as_str) == Some("running"))
            .map(|r| str_of(r, &["agent"]).unwrap_or_else(|| "agent".into()))
            .collect()
    })
    .unwrap_or_default()
}

pub trait FreshTask {
    fn look(&self, rule: &Rule, task: TaskRef) -> Result<Fresh>;
    fn comment(&self, rule: &Rule, task: TaskRef, text: &str) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn facts<'a>() -> Facts<'a> {
        Facts {
            rule_persona: "Agent · Team",
            me: 1,
            owners: vec![],
            assignees: vec![],
            personas: vec![],
            running_agents: vec![],
            status_ok: true,
        }
    }

    #[test]
    fn the_owner_implied_by_the_persona_and_the_rule_persona_do_not_hold_a_task() {
        let f = assess(Facts {
            assignees: vec![(Some(7), "Owner".into()), (Some(1), "Me".into())],
            personas: vec!["agent · team".into()],
            owners: vec![7],
            ..facts()
        });
        assert!(f.free());
        let other = assess(Facts {
            assignees: vec![(Some(8), "Anna".into())],
            owners: vec![7],
            ..facts()
        });
        assert!(!other.free());
    }

    #[test]
    fn another_person_persona_or_running_agent_holds_the_task() {
        let f = assess(Facts {
            assignees: vec![(Some(2), "Anna".into())],
            personas: vec!["Reviewer".into()],
            running_agents: vec!["codex".into()],
            ..facts()
        });
        assert_eq!(f.holders.len(), 3);
        assert_eq!(
            f.comment(),
            "Auto-run skipped: this task was taken by Anna (assignee), Reviewer (persona), codex (agent run)."
        );
        assert!(!f.free());
    }

    #[test]
    fn a_changed_status_blocks_without_a_holder() {
        let f = assess(Facts {
            status_ok: false,
            ..facts()
        });
        assert!(f.holders.is_empty() && !f.free());
    }

    #[test]
    fn task_json_is_read_in_either_key_style_and_only_running_runs_count() {
        let t = json!({"assignees":[{"id":2,"name":"Anna"}],"personaAssignees":[{"name":"P"}]});
        assert_eq!(people_of(&t), vec![(Some(2), "Anna".to_string())]);
        assert_eq!(personas_of(&t), vec!["P".to_string()]);
        let owned = json!({"persona_assignees":[{"name":"Agent · Team","owner":{"id":7}},{"name":"X","owner":{"id":9}}]});
        assert_eq!(owners_of(&owned, "agent · team"), vec![7]);
        let runs = json!({"runs":[{"agent":"codex","status":"running"},{"agent":"x","status":"succeeded"}]});
        assert_eq!(running_agents_of(&runs), vec!["codex".to_string()]);
    }
}
