mod danger;
mod dialog;
mod matcher;
#[cfg(test)]
mod tests;

use serde_json::Value;
use sha2::{Digest, Sha256};

pub use danger::danger;
pub use dialog::parse_dialog;
pub use matcher::verdict;

const SPOOL_CAP: usize = 4 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Claude,
    Codex,
}

impl Agent {
    fn name(self) -> &'static str {
        match self {
            Agent::Claude => "claude",
            Agent::Codex => "codex",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookEvent {
    pub id: String,
    pub agent: Agent,
    pub at_ms: u64,
    pub tool: String,
    pub command: Option<String>,
    pub description: Option<String>,
    pub file_path: Option<String>,
    pub content: Option<String>,
    pub cwd: Option<String>,
    pub rule_for_bash: bool,
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionAction {
    Yes,
    Always,
    No,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogOption {
    pub key: String,
    pub label: String,
    pub action: OptionAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DialogBody {
    Command {
        rows: Vec<String>,
        reason: Option<String>,
    },
    WriteFile {
        shown_path: String,
        preview: Vec<String>,
    },
    Withheld,
    Other {
        title: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Dialog {
    pub agent: Agent,
    pub body: DialogBody,
    pub options: Vec<DialogOption>,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Approvable,
    ViewOnly(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Choice {
    Yes,
    Always,
    No,
}

pub fn parse_spool(id: &str, bytes: &[u8]) -> Option<HookEvent> {
    if bytes.len() > SPOOL_CAP {
        return None;
    }
    let v: Value = serde_json::from_slice(bytes).ok()?;
    if v.get("v")?.as_u64()? != 1 {
        return None;
    }
    let agent = match v.get("agent")?.as_str()? {
        "claude" => Agent::Claude,
        "codex" => Agent::Codex,
        _ => return None,
    };
    let at_ms = v.get("at_ms")?.as_u64()?;
    let truncated = v.get("truncated")?.as_bool()?;
    let hook = v.get("hook")?.as_object()?;
    let tool = hook.get("tool_name")?.as_str()?.to_string();
    let mut ev = HookEvent {
        id: id.to_string(),
        agent,
        at_ms,
        tool,
        command: None,
        description: None,
        file_path: None,
        content: None,
        cwd: None,
        rule_for_bash: false,
        truncated,
    };
    if truncated {
        return Some(ev);
    }
    let s = |v: Option<&Value>| v.and_then(Value::as_str).map(str::to_string);
    let input = hook.get("tool_input").and_then(Value::as_object);
    ev.command = s(input.and_then(|i| i.get("command")));
    ev.description = s(input.and_then(|i| i.get("description")));
    ev.file_path = s(input.and_then(|i| i.get("file_path")));
    ev.content = s(input.and_then(|i| i.get("content")));
    ev.cwd = s(hook.get("cwd"));
    ev.rule_for_bash = hook
        .get("permission_suggestions")
        .and_then(Value::as_array)
        .is_some_and(|a| a.iter().any(adds_bash_rule));
    Some(ev)
}

fn adds_bash_rule(s: &Value) -> bool {
    s.get("type").and_then(Value::as_str) == Some("addRules")
        && s.get("rules").and_then(Value::as_array).is_some_and(|r| {
            !r.is_empty()
                && r.iter()
                    .all(|x| x.get("toolName").and_then(Value::as_str) == Some("Bash"))
        })
}

pub fn fingerprint(session: &str, d: &Dialog, h: &HookEvent) -> String {
    let mut sha = Sha256::new();
    let mut put = |s: &str| {
        sha.update((s.len() as u64).to_be_bytes());
        sha.update(s.as_bytes());
    };
    put(session);
    put(d.agent.name());
    put(&h.id);
    put(&d.text);
    for o in &d.options {
        put(&o.key);
        put(&o.label);
    }
    sha.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn key_for(d: &Dialog, h: Option<&HookEvent>, choice: Choice) -> Option<Vec<u8>> {
    if verdict(d, h) != Verdict::Approvable {
        return None;
    }
    let want = match choice {
        Choice::Yes => OptionAction::Yes,
        Choice::No => OptionAction::No,
        Choice::Always => OptionAction::Always,
    };
    if choice == Choice::Always {
        let h = h?;
        if !matches!(d.body, DialogBody::Command { .. }) {
            return None;
        }
        if d.agent == Agent::Claude && !(h.rule_for_bash && h.tool == "Bash") {
            return None;
        }
    }
    let mut it = d.options.iter().filter(|o| o.action == want);
    let o = it.next()?;
    if it.next().is_some() {
        return None;
    }
    if o.key == "esc" {
        return Some(vec![0x1b]);
    }
    let mut cs = o.key.chars();
    let c = cs.next()?;
    if cs.next().is_some() || c.is_control() {
        return None;
    }
    Some(c.to_string().into_bytes())
}
