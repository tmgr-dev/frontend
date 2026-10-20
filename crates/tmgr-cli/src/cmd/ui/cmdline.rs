use super::model::{Field, Kind, Node};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Value {
    Bool(bool),
    Count(u8),
    Text(String),
    Multi(Vec<String>),
}

pub type Values = BTreeMap<String, Value>;

pub const HIDDEN: &str = "<hidden>";

pub fn expand_tilde(value: &str) -> String {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    match (value.strip_prefix('~'), home) {
        (Some(""), Some(h)) => h.to_string_lossy().into_owned(),
        (Some(rest), Some(h)) if rest.starts_with('/') => {
            format!("{}{}", h.to_string_lossy(), rest)
        }
        _ => value.to_string(),
    }
}

fn clean(value: &str) -> Result<(), String> {
    if value.chars().any(|c| c.is_control()) {
        Err("values cannot contain line breaks or control characters".into())
    } else {
        Ok(())
    }
}

fn texts(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Text(t)) if !t.is_empty() => vec![t.clone()],
        Some(Value::Multi(m)) => m.iter().filter(|s| !s.is_empty()).cloned().collect(),
        _ => Vec::new(),
    }
}

pub fn build_argv(node: &Node, values: &Values, mask: bool) -> Result<Vec<String>, String> {
    let mut argv = Vec::new();
    argv.extend(node.path.iter().cloned());
    let mut positional: Vec<String> = Vec::new();
    let mut gap: Option<&Field> = None;
    for f in &node.fields {
        let value = values.get(&f.id);
        let shown = |v: String| {
            if mask && f.sensitive {
                HIDDEN.to_string()
            } else if f.kind == Kind::Path {
                expand_tilde(&v)
            } else {
                v
            }
        };
        if f.positional {
            let items = texts(value);
            if items.is_empty() {
                if f.required {
                    return Err(format!("{} is required", f.label()));
                }
                gap = gap.or(Some(f));
                continue;
            }
            if let Some(g) = gap {
                return Err(format!("fill {} first", g.label()));
            }
            for v in items {
                clean(&v)?;
                positional.push(shown(v));
            }
            continue;
        }
        let long = f.long.as_deref().unwrap_or(&f.id);
        match (&f.kind, value) {
            (Kind::Bool, Some(Value::Bool(true))) => argv.push(format!("--{long}")),
            (Kind::Count, Some(Value::Count(n))) => {
                argv.extend((0..*n).map(|_| format!("--{long}")))
            }
            (Kind::Bool | Kind::Count, _) => {}
            _ => {
                let items = texts(value);
                if items.is_empty() && f.required {
                    return Err(format!("{} is required", f.label()));
                }
                for v in items {
                    clean(&v)?;
                    argv.push(format!("--{long}={}", shown(v)));
                }
            }
        }
    }
    if positional.iter().any(|p| p.starts_with('-')) {
        argv.push("--".into());
    }
    argv.extend(positional);
    Ok(argv)
}

pub fn quote(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c));
    if plain {
        arg.to_string()
    } else {
        format!("'{}'", arg.replace('\'', "'\\''"))
    }
}

pub fn render(argv: &[String]) -> String {
    let mut parts = vec!["tmgr".to_string()];
    parts.extend(argv.iter().map(|a| quote(a)));
    parts.join(" ")
}
