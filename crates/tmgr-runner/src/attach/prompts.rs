use serde::{Deserialize, Serialize};

pub const MAX_NAME: usize = 60;
pub const MAX_TEXT: usize = 4000;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prompt {
    pub name: String,
    pub text: String,
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    prompt: Vec<Prompt>,
}

pub fn clean(s: &str, max: usize) -> String {
    let flat: String = s
        .chars()
        .filter_map(|c| match c {
            '\n' | '\r' | '\t' | '\u{2028}' | '\u{2029}' => Some(' '),
            c if crate::sanitize::is_unsafe(c) => None,
            c => Some(c),
        })
        .collect();
    flat.trim().chars().take(max).collect()
}

pub fn defaults() -> Vec<Prompt> {
    [
        ("run tests", "Run the tests and fix any failures."),
        (
            "commit and open PR",
            "Commit the changes with a clear message and open a pull request.",
        ),
        (
            "summarize what you did",
            "Summarize what you did so far and what is left.",
        ),
    ]
    .into_iter()
    .map(|(name, text)| Prompt {
        name: name.into(),
        text: text.into(),
    })
    .collect()
}

pub fn parse(text: &str) -> Result<Vec<Prompt>, String> {
    let file: File = toml::from_str(text).map_err(|e| e.message().to_string())?;
    Ok(file
        .prompt
        .into_iter()
        .map(|p| Prompt {
            name: clean(&p.name, MAX_NAME),
            text: clean(&p.text, MAX_TEXT),
        })
        .filter(|p| !p.name.is_empty() && !p.text.is_empty())
        .collect())
}

pub fn render(prompts: &[Prompt]) -> String {
    let file = File {
        prompt: prompts.to_vec(),
    };
    toml::to_string_pretty(&file).unwrap_or_default()
}

pub fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    let text = clean(text, MAX_TEXT);
    if bracketed {
        format!("\x1b[200~{text}\x1b[201~").into_bytes()
    } else {
        text.into_bytes()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PromptForm {
    pub index: Option<usize>,
    pub name: String,
    pub text: String,
    pub field: usize,
}

impl PromptForm {
    pub fn active(&mut self) -> (&mut String, usize) {
        if self.field == 0 {
            (&mut self.name, MAX_NAME)
        } else {
            (&mut self.text, MAX_TEXT)
        }
    }
}
