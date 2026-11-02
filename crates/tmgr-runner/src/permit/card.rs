use super::{
    danger, fingerprint, key_for, parse_dialog, verdict, Agent, Choice, Dialog, DialogBody,
    HookEvent, OptionAction, Verdict,
};
use crate::sanitize::sanitize_text;
use sha2::{Digest, Sha256};

pub fn agent_of(profile: &str) -> Option<Agent> {
    match profile {
        "claude-code" => Some(Agent::Claude),
        "codex" => Some(Agent::Codex),
        _ => None,
    }
}

pub fn digest(parts: &[&str]) -> String {
    let mut sha = Sha256::new();
    for p in parts {
        sha.update((p.len() as u64).to_be_bytes());
        sha.update(p.as_bytes());
    }
    sha.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn clean_text(s: &str) -> String {
    s.split('\n')
        .map(|l| sanitize_text(l).into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}

pub struct Eval {
    pub id: String,
    pub dialog: Dialog,
    pub hook: Option<HookEvent>,
    pub verdict: Verdict,
    pub danger: Vec<&'static str>,
}

impl Eval {
    pub fn new(
        session: &str,
        agent: Agent,
        rows: &[String],
        hook: Option<HookEvent>,
    ) -> Option<Eval> {
        let dialog = parse_dialog(agent, rows)?;
        let verdict = verdict(&dialog, hook.as_ref());
        let id = match &hook {
            Some(h) => fingerprint(session, &dialog, h),
            None => digest(&[session, &dialog.text]),
        };
        let danger = match &hook {
            Some(h) => danger(
                &h.tool,
                h.command.as_deref().unwrap_or(""),
                h.file_path.as_deref(),
                h.cwd.as_deref(),
            ),
            None => Vec::new(),
        };
        Some(Eval {
            id,
            dialog,
            hook,
            verdict,
            danger,
        })
    }

    pub fn approvable(&self) -> bool {
        self.verdict == Verdict::Approvable
    }

    fn approved_hook(&self) -> Option<&HookEvent> {
        self.hook.as_ref().filter(|_| self.approvable())
    }

    pub fn tool(&self) -> String {
        if let Some(h) = &self.hook {
            return clean_text(&h.tool);
        }
        match &self.dialog.body {
            DialogBody::Command { .. } | DialogBody::Withheld => "Bash".into(),
            DialogBody::WriteFile { .. } => "Write".into(),
            DialogBody::Other { title } => clean_text(title),
        }
    }

    pub fn text(&self) -> String {
        if let Some(h) = self.approved_hook() {
            let full = match (h.tool.as_str(), &h.command, &h.file_path, &h.content) {
                ("Bash", Some(c), ..) => Some(c.clone()),
                ("Write", _, Some(p), Some(c)) => Some(format!("{p}\n{c}")),
                _ => None,
            };
            if let Some(t) = full {
                return clean_text(&t);
            }
        }
        let shown = match &self.dialog.body {
            DialogBody::Command { rows, .. } => rows.join("\n").trim().to_string(),
            DialogBody::WriteFile {
                shown_path,
                preview,
            } => format!("{shown_path}\n{}", preview.join("\n")),
            _ => self.dialog.text.clone(),
        };
        clean_text(&shown)
    }

    pub fn path(&self) -> Option<String> {
        match (&self.dialog.body, self.approved_hook()) {
            (_, Some(h)) if h.file_path.is_some() => h.file_path.as_deref().map(clean_text),
            (DialogBody::WriteFile { shown_path, .. }, _) => Some(clean_text(shown_path)),
            _ => None,
        }
    }

    pub fn reason(&self) -> Option<String> {
        match &self.dialog.body {
            DialogBody::Command { reason, .. } => reason.as_deref().map(clean_text),
            _ => None,
        }
    }

    pub fn always_label(&self) -> Option<String> {
        if !self.danger.is_empty()
            || key_for(&self.dialog, self.hook.as_ref(), Choice::Always).is_none()
        {
            return None;
        }
        let o = self
            .dialog
            .options
            .iter()
            .find(|o| o.action == OptionAction::Always)?;
        Some(clean_text(&o.label))
    }

    pub fn key(&self, choice: Choice, confirmed: bool) -> Result<Vec<u8>, &'static str> {
        if choice == Choice::Always && !self.danger.is_empty() {
            return Err("always is never allowed for a dangerous command");
        }
        if choice != Choice::No && !self.danger.is_empty() && !confirmed {
            return Err("this command needs a second confirmation");
        }
        key_for(&self.dialog, self.hook.as_ref(), choice)
            .ok_or("the agent does not offer this choice for this prompt")
    }
}
