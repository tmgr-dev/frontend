use super::capture;
use super::fuzzy;
use super::model::Field;
use serde_json::Value as Json;
use std::path::Path;
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Tasks,
    Sessions,
    Workspaces,
    Profiles,
}

impl Source {
    pub fn title(self) -> &'static str {
        match self {
            Source::Tasks => "Pick a task",
            Source::Sessions => "Pick a session",
            Source::Workspaces => "Pick a workspace",
            Source::Profiles => "Pick an agent profile",
        }
    }

    pub fn heads(self) -> &'static [&'static str] {
        match self {
            Source::Tasks => &["key", "title", "status", "project"],
            Source::Sessions => &["name", "state", "id"],
            Source::Workspaces => &["code", "name"],
            Source::Profiles => &["id", "name"],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub value: String,
    pub cols: Vec<String>,
    pub preview: Vec<String>,
}

impl Item {
    fn search_text(&self) -> String {
        self.cols.join(" ")
    }
}

const PROFILE_PATHS: &[&[&str]] = &[
    &["run"],
    &["agents", "bundle"],
    &["agents", "login"],
    &["agents", "logout"],
    &["agents", "sync"],
    &["queue"],
];

fn profile_field(path: &[String]) -> bool {
    cfg!(feature = "runner")
        && PROFILE_PATHS
            .iter()
            .any(|h| path.len() >= h.len() && h.iter().zip(path).all(|(a, b)| a == b))
}

pub fn source_for(path: &[String], field: &Field) -> Option<Source> {
    match field.id.as_str() {
        "reference" | "task" => Some(Source::Tasks),
        "session" => Some(Source::Sessions),
        "code" if path == ["ws", "use"] => Some(Source::Workspaces),
        "ws" => Some(Source::Workspaces),
        "agent" | "profile" if profile_field(path) => Some(Source::Profiles),
        _ => None,
    }
}

pub fn filter(items: &[Item], query: &str) -> Vec<usize> {
    let indexed: Vec<(usize, String)> = items
        .iter()
        .enumerate()
        .map(|(i, it)| (i, it.search_text()))
        .collect();
    fuzzy::rank(query, indexed, |(_, t)| t.clone())
        .into_iter()
        .map(|(i, _)| i)
        .collect()
}

fn run_json(exe: &Path, args: &[String]) -> Result<Json, String> {
    let mut full = args.to_vec();
    full.push("--json".into());
    full.push("--no-color".into());
    let (_, text) = capture::capture(exe, &full, Duration::from_secs(20))?;
    let json: Json =
        serde_json::from_str(text.trim()).map_err(|_| "unreadable answer".to_string())?;
    if json["ok"].as_bool() == Some(false) {
        let msg = json["error"]["message"]
            .as_str()
            .unwrap_or("the command failed");
        return Err(clean(msg));
    }
    Ok(json)
}

fn clean(text: &str) -> String {
    tmgr_core::text::sanitize_line(text).trim().to_string()
}

fn s(v: &Json, key: &str) -> String {
    clean(v[key].as_str().unwrap_or_default())
}

pub(super) fn ws_args(source: Source, ws: Option<&str>) -> Vec<String> {
    match ws.filter(|w| !w.is_empty() && source == Source::Tasks) {
        Some(w) => vec![format!("--ws={w}")],
        None => Vec::new(),
    }
}

pub fn load(source: Source, ws: Option<&str>, exe: &Path) -> Result<Vec<Item>, String> {
    let mut args = ws_args(source, ws);
    match source {
        Source::Tasks => {
            args.extend(["task", "list", "--limit=100"].map(String::from));
            tasks(&run_json(exe, &args)?)
        }
        Source::Sessions => {
            args.push("agents".into());
            sessions(&run_json(exe, &args)?)
        }
        Source::Workspaces => {
            args.extend(["ws", "list"].map(String::from));
            workspaces(&run_json(exe, &args)?)
        }
        Source::Profiles => profiles(),
    }
}

pub fn tasks(json: &Json) -> Result<Vec<Item>, String> {
    let items = json["data"]["items"]
        .as_array()
        .ok_or("no tasks in the answer")?;
    Ok(items
        .iter()
        .map(|t| {
            let key = s(t, "ref");
            let title = s(t, "title");
            let status = clean(t["status"]["name"].as_str().unwrap_or_default());
            let project = s(t, "project");
            let mut preview = vec![key.clone(), title.clone(), String::new()];
            if !status.is_empty() {
                preview.push(format!("Status:   {status}"));
            }
            if !project.is_empty() {
                preview.push(format!("Project:  {project}"));
            }
            let updated = s(t, "updated_at");
            if !updated.is_empty() {
                preview.push(format!("Updated:  {updated}"));
            }
            Item {
                value: key.clone(),
                cols: vec![key, title, status, project],
                preview,
            }
        })
        .collect())
}

pub fn sessions(json: &Json) -> Result<Vec<Item>, String> {
    let items = json["data"]["sessions"]
        .as_array()
        .ok_or("no sessions in the answer")?;
    Ok(items
        .iter()
        .map(|x| {
            let id = s(x, "id");
            let name = s(x, "name");
            let state = clean(x["state"]["kind"].as_str().unwrap_or_default());
            let mut preview = vec![name.clone(), String::new()];
            preview.push(format!("State:    {state}"));
            preview.push(format!("Id:       {id}"));
            let cwd = s(x, "cwd");
            if !cwd.is_empty() {
                preview.push(format!("Folder:   {cwd}"));
            }
            if let Some(p) = x["profile"].as_str() {
                preview.push(format!("Profile:  {}", clean(p)));
            }
            Item {
                value: id.clone(),
                cols: vec![name, state, id],
                preview,
            }
        })
        .collect())
}

pub fn workspaces(json: &Json) -> Result<Vec<Item>, String> {
    let items = json["data"]["workspaces"]
        .as_array()
        .ok_or("no workspaces in the answer")?;
    Ok(items
        .iter()
        .map(|w| {
            let code = s(w, "code");
            let name = s(w, "name");
            let local = w["local"].as_bool() == Some(true);
            let value = if local && !code.starts_with("local:") {
                format!("local:{code}")
            } else {
                code.clone()
            };
            let mut preview = vec![name.clone(), String::new(), format!("Code:     {value}")];
            if w["current"].as_bool() == Some(true) {
                preview.push("Current workspace".into());
            }
            if local {
                preview.push("Local workspace of the desktop app".into());
            }
            Item {
                value,
                cols: vec![code, name],
                preview,
            }
        })
        .collect())
}

#[cfg(feature = "runner")]
fn profiles() -> Result<Vec<Item>, String> {
    let (set, _) =
        tmgr_runner::profile::ProfileSet::load(tmgr_runner::profile::user_dir().as_deref());
    Ok(set
        .list()
        .into_iter()
        .map(|p| Item {
            value: clean(&p.id),
            cols: vec![clean(&p.id), clean(&p.name)],
            preview: vec![
                clean(&p.name),
                String::new(),
                format!("Profile id: {}", clean(&p.id)),
            ],
        })
        .collect())
}

#[cfg(not(feature = "runner"))]
fn profiles() -> Result<Vec<Item>, String> {
    Err("agent profiles need a build with the runner".into())
}
