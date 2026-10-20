use super::model::Node;

#[derive(Clone, Debug)]
pub struct Entry {
    pub path: Vec<String>,
    pub label: String,
    pub about: String,
}

#[derive(Clone, Debug)]
pub struct Group {
    pub label: String,
    pub summary: String,
    pub entries: Vec<Entry>,
}

const GROUPS: &[(&str, &[&str])] = &[
    (
        "Tasks",
        &[
            "start", "stop", "switch", "done", "add", "task", "board", "status",
        ],
    ),
    (
        "Agents",
        &[
            "run", "attach", "agents", "send", "peek", "halt", "rm", "rename", "agent", "log",
        ],
    ),
    ("Queue", &["queue"]),
    ("Runner", &["runner"]),
    ("Pages", &["page", "note"]),
    ("Time", &["today", "routine"]),
    ("Usage", &["usage"]),
    (
        "Account",
        &[
            "login",
            "logout",
            "whoami",
            "auth",
            "ws",
            "config",
            "project",
            "init",
            "git",
            "hook",
            "notify",
            "alarm",
            "self-update",
        ],
    ),
];

const OTHER: &str = "Other";

const INTERNAL: &[&[&str]] = &[
    &["worker"],
    &["link"],
    &["mcp"],
    &["prompt"],
    &["statusline"],
    &["ui"],
    &["hook", "claude"],
    &["hook", "codex"],
    &["git", "hook"],
    &["runner", "serve"],
    &["queue", "autopilot", "serve"],
    &["usage", "statusline-hook"],
];

pub fn is_internal(path: &[String]) -> bool {
    INTERNAL
        .iter()
        .any(|h| path.len() >= h.len() && h.iter().zip(path).all(|(a, b)| a == b))
}

fn group_of(top: &str) -> &'static str {
    GROUPS
        .iter()
        .find(|(_, names)| names.contains(&top))
        .map(|(g, _)| *g)
        .unwrap_or(OTHER)
}

pub fn groups(root: &Node) -> Vec<Group> {
    let mut labels: Vec<&str> = GROUPS.iter().map(|(g, _)| *g).collect();
    labels.push(OTHER);
    let mut out = Vec::new();
    for label in labels {
        let entries: Vec<Entry> = root
            .leaves()
            .into_iter()
            .filter(|n| !is_internal(&n.path) && group_of(&n.path[0]) == label)
            .map(|n| Entry {
                path: n.path.clone(),
                label: n.path.join(" "),
                about: n.about.clone(),
            })
            .collect();
        if entries.is_empty() {
            continue;
        }
        let mut tops: Vec<&str> = Vec::new();
        for e in &entries {
            if !tops.contains(&e.path[0].as_str()) {
                tops.push(&e.path[0]);
            }
        }
        out.push(Group {
            label: label.to_string(),
            summary: tops.join(", "),
            entries,
        });
    }
    out
}
