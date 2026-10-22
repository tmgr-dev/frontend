use super::prompts::clean;
use serde::{Deserialize, Serialize};

pub const MAX_NAME: usize = 60;
pub const MAX_PROMPT: usize = 4000;
pub const MAX_VALUE: usize = 200;
pub const MAX_FIELDS: usize = 6;
pub const MAX_TEMPLATES: usize = 50;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub default: String,
}

impl Field {
    pub fn title(&self) -> &str {
        if self.label.is_empty() {
            &self.name
        } else {
            &self.label
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Template {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub agent: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub command: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub folder: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<Field>,
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    template: Vec<Template>,
}

fn field(name: &str, label: &str) -> Field {
    Field {
        name: name.into(),
        label: label.into(),
        default: String::new(),
    }
}

pub fn defaults() -> Vec<Template> {
    let t = |name: &str, prompt: &str, fields: Vec<Field>| Template {
        name: name.into(),
        prompt: prompt.into(),
        fields,
        ..Template::default()
    };
    vec![
        t(
            "Review PR #N",
            "Review pull request #{pr}: read the diff, point out bugs, risks and missing tests, and suggest concrete fixes.",
            vec![field("pr", "PR number")],
        ),
        t(
            "Fix failing CI",
            "CI is failing on this branch. Find out why, fix it, and run the checks locally before you stop.",
            Vec::new(),
        ),
        t(
            "Write tests for <file>",
            "Write tests for {file}. Cover the main behavior and the edge cases, and run them.",
            vec![field("file", "File")],
        ),
        t(
            "Explain this repo",
            "Explain this repository: what it does, how it is laid out, how to build and test it, and where to start reading.",
            Vec::new(),
        ),
    ]
}

fn clean_field(f: Field) -> Option<Field> {
    let valid = !f.name.is_empty()
        && f.name.len() <= 24
        && f.name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then(|| Field {
        name: f.name,
        label: clean(&f.label, MAX_NAME),
        default: clean(&f.default, MAX_VALUE),
    })
}

pub fn parse(text: &str) -> Result<Vec<Template>, String> {
    let file: File = toml::from_str(text).map_err(|e| e.message().to_string())?;
    let mut out: Vec<Template> = Vec::new();
    for t in file.template {
        let name = clean(&t.name, MAX_NAME);
        let prompt = clean(&t.prompt, MAX_PROMPT);
        let command = clean(&t.command, MAX_PROMPT);
        if name.is_empty()
            || (prompt.is_empty() && command.is_empty())
            || out.iter().any(|o| o.name.eq_ignore_ascii_case(&name))
            || out.len() >= MAX_TEMPLATES
        {
            continue;
        }
        let mut fields: Vec<Field> = Vec::new();
        for f in t.fields.into_iter().filter_map(clean_field) {
            if fields.len() < MAX_FIELDS && !fields.iter().any(|o| o.name == f.name) {
                fields.push(f);
            }
        }
        out.push(Template {
            name,
            agent: clean(&t.agent, MAX_NAME),
            command,
            folder: clean(&t.folder, 512),
            prompt,
            fields,
        });
    }
    Ok(out)
}

pub fn render(templates: &[Template]) -> String {
    let file = File {
        template: templates.to_vec(),
    };
    toml::to_string_pretty(&file).unwrap_or_default()
}

pub fn shell_quote(value: &str) -> String {
    if !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-./=:@%+,".contains(c))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub fn substitute(text: &str, fields: &[Field], values: &[String], quote: bool) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = rest.find(['{', '}']) {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        if tail.starts_with("{{") || tail.starts_with("}}") {
            out.push_str(&tail[..1]);
            rest = &tail[2..];
            continue;
        }
        if tail.starts_with('{') {
            if let Some(end) = tail.find('}') {
                let key = &tail[1..end];
                if let Some(v) = fields.iter().position(|f| f.name == key) {
                    let value = clean(values.get(v).map_or("", String::as_str), MAX_VALUE);
                    out.push_str(&if quote { shell_quote(&value) } else { value });
                    rest = &tail[end + 1..];
                    continue;
                }
            }
        }
        out.push_str(&tail[..1]);
        rest = &tail[1..];
    }
    out.push_str(rest);
    out
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TemplateForm {
    pub index: usize,
    pub title: String,
    pub labels: Vec<String>,
    pub values: Vec<String>,
    pub field: usize,
}

impl TemplateForm {
    pub fn new(index: usize, t: &Template) -> Self {
        Self {
            index,
            title: t.name.clone(),
            labels: t.fields.iter().map(|f| f.title().to_string()).collect(),
            values: t.fields.iter().map(|f| f.default.clone()).collect(),
            field: 0,
        }
    }

    pub fn active(&mut self) -> &mut String {
        let i = self.field.min(self.values.len().saturating_sub(1));
        &mut self.values[i]
    }

    pub fn first_empty(&self) -> Option<usize> {
        self.values.iter().position(|v| v.trim().is_empty())
    }
}
