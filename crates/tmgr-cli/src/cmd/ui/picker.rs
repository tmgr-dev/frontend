use super::fuzzy;
use super::model::Field;
use serde_json::Value as Json;
use std::io::Read;
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

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

pub fn source_for(path: &[String], field: &Field) -> Option<Source> {
    let id = field.id.as_str();
    match id {
        "reference" | "task" => Some(Source::Tasks),
        "session" => Some(Source::Sessions),
        "code" if path == ["ws", "use"] => Some(Source::Workspaces),
        "ws" => Some(Source::Workspaces),
        "agent" | "profile" => Some(Source::Profiles),
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
    let mut child = std::process::Command::new(exe)
        .args(args)
        .arg("--json")
        .arg("--no-color")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| e.to_string())?;
    let mut out = child.stdout.take().ok_or("no output")?;
    let reader = std::thread::spawn(move || {
        let mut buf = String::new();
        let _ = out.read_to_string(&mut buf);
        buf
    });
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > Duration::from_secs(20) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("timed out".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(40)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let text = reader.join().map_err(|_| "reader failed".to_string())?;
    let json: Json =
        serde_json::from_str(text.trim()).map_err(|_| "unreadable answer".to_string())?;
    if json["ok"].as_bool() == Some(false) {
        let msg = json["error"]["message"]
            .as_str()
            .unwrap_or("the command failed");
        return Err(msg.to_string());
    }
    Ok(json)
}

fn clean(text: &str) -> String {
    tmgr_core::text::sanitize_line(text).trim().to_string()
}

fn s(v: &Json, key: &str) -> String {
    clean(v[key].as_str().unwrap_or_default())
}

pub fn load(source: Source, ws: Option<&str>, exe: &Path) -> Result<Vec<Item>, String> {
    let mut args: Vec<String> = Vec::new();
    if let Some(w) = ws.filter(|w| !w.is_empty()) {
        args.push(format!("--ws={w}"));
    }
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
                value: if name.is_empty() {
                    id.clone()
                } else {
                    name.clone()
                },
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
            value: p.id.clone(),
            cols: vec![p.id.clone(), p.name.clone()],
            preview: vec![
                p.name.clone(),
                String::new(),
                format!("Profile id: {}", p.id),
            ],
        })
        .collect())
}

#[cfg(not(feature = "runner"))]
fn profiles() -> Result<Vec<Item>, String> {
    Err("agent profiles need a build with the runner".into())
}
