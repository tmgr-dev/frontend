use clap::{Arg, ArgAction, Command, ValueHint};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Bool,
    Count,
    Text,
    Path,
    Select(Vec<String>),
    Multi,
}

#[derive(Clone, Debug)]
pub struct Field {
    pub id: String,
    pub long: Option<String>,
    pub positional: bool,
    pub kind: Kind,
    pub required: bool,
    pub default: Option<String>,
    pub help: String,
    pub value_name: String,
    pub sensitive: bool,
}

impl Field {
    pub fn label(&self) -> String {
        match &self.long {
            Some(l) => l.replace('-', " "),
            None => self.id.replace('_', " "),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Node {
    pub path: Vec<String>,
    pub about: String,
    pub children: Vec<Node>,
    pub fields: Vec<Field>,
    pub runnable: bool,
}

impl Node {
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    pub fn find(&self, path: &[String]) -> Option<&Node> {
        if self.path == path {
            return Some(self);
        }
        self.children
            .iter()
            .find_map(|c| path.starts_with(&c.path).then(|| c.find(path)).flatten())
    }

    pub fn leaves(&self) -> Vec<&Node> {
        let mut out = Vec::new();
        for c in &self.children {
            if c.is_leaf() {
                out.push(c);
            } else {
                if c.runnable {
                    out.push(c);
                }
                out.extend(c.leaves());
            }
        }
        out
    }
}

const DENIED_FIELDS: [&str; 4] = ["yes", "json", "server", "verbose"];
const ALLOWED_GLOBALS: [&str; 1] = ["ws"];
const SENSITIVE_WORDS: [&str; 7] = [
    "token",
    "password",
    "passwd",
    "secret",
    "key",
    "credential",
    "auth",
];

pub fn is_sensitive_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SENSITIVE_WORDS.iter().any(|w| lower.contains(w))
}

pub fn build(root: &Command) -> Node {
    let mut root = root.clone();
    root.build();
    let mut node = node_of(&root, Vec::new());
    node.fields = root
        .get_arguments()
        .filter(|a| a.get_id().as_str() == "ws")
        .filter_map(|a| field_of(a, true))
        .collect();
    node
}

fn node_of(cmd: &Command, path: Vec<String>) -> Node {
    let mut children = Vec::new();
    for sub in cmd.get_subcommands() {
        let name = sub.get_name();
        if name == "help" {
            continue;
        }
        let mut p = path.clone();
        p.push(name.to_string());
        children.push(node_of(sub, p));
    }
    let fields = cmd
        .get_arguments()
        .filter_map(|a| field_of(a, false))
        .collect();
    Node {
        path,
        about: cmd
            .get_about()
            .map(|a| a.to_string())
            .unwrap_or_default()
            .lines()
            .next()
            .unwrap_or_default()
            .trim_start_matches("[experimental] ")
            .to_string(),
        runnable: children.is_empty() || !cmd.is_subcommand_required_set(),
        children,
        fields,
    }
}

fn field_of(arg: &Arg, allow_global: bool) -> Option<Field> {
    let id = arg.get_id().as_str();
    if id == "help" || id == "version" || arg.is_hide_set() {
        return None;
    }
    if arg.is_global_set() && !(allow_global && ALLOWED_GLOBALS.contains(&id)) {
        return None;
    }
    if DENIED_FIELDS.contains(&id) || arg.get_long().is_some_and(|l| DENIED_FIELDS.contains(&l)) {
        return None;
    }
    let possible: Vec<String> = arg
        .get_possible_values()
        .iter()
        .filter(|v| !v.is_hide_set())
        .map(|v| v.get_name().to_string())
        .collect();
    let kind = match arg.get_action() {
        ArgAction::SetTrue | ArgAction::SetFalse => Kind::Bool,
        ArgAction::Count => Kind::Count,
        ArgAction::Append => Kind::Multi,
        _ if !possible.is_empty() => Kind::Select(possible),
        _ if matches!(
            arg.get_value_hint(),
            ValueHint::AnyPath
                | ValueHint::FilePath
                | ValueHint::DirPath
                | ValueHint::ExecutablePath
        ) =>
        {
            Kind::Path
        }
        _ => Kind::Text,
    };
    let long = arg.get_long().map(str::to_string);
    let value_name = arg
        .get_value_names()
        .and_then(|n| n.first())
        .map(|n| n.to_string())
        .unwrap_or_else(|| id.to_uppercase());
    let default = arg
        .get_default_values()
        .first()
        .map(|d| d.to_string_lossy().into_owned());
    let sensitive = is_sensitive_name(id) || long.as_deref().is_some_and(is_sensitive_name);
    Some(Field {
        id: id.to_string(),
        positional: arg.is_positional(),
        long,
        kind,
        required: arg.is_required_set(),
        default,
        help: arg
            .get_help()
            .map(|h| h.to_string())
            .unwrap_or_default()
            .lines()
            .next()
            .unwrap_or_default()
            .to_string(),
        value_name,
        sensitive,
    })
}

#[cfg(test)]
pub fn describe(node: &Node) -> String {
    let mut out = String::new();
    describe_into(node, &mut out);
    out
}

#[cfg(test)]
fn describe_into(node: &Node, out: &mut String) {
    use std::fmt::Write;
    if !node.path.is_empty() {
        let _ = writeln!(
            out,
            "{}{}",
            node.path.join(" "),
            if node.runnable && !node.is_leaf() {
                " [runnable]"
            } else {
                ""
            }
        );
    }
    for f in &node.fields {
        let kind = match &f.kind {
            Kind::Select(v) => format!("select({})", v.join("|")),
            Kind::Bool => "bool".into(),
            Kind::Count => "count".into(),
            Kind::Text => "text".into(),
            Kind::Path => "path".into(),
            Kind::Multi => "multi".into(),
        };
        let _ = writeln!(
            out,
            "  {}{} {}{}{}{}",
            if f.positional { "<" } else { "--" },
            f.long.clone().unwrap_or_else(|| f.id.clone()),
            kind,
            if f.required { " required" } else { "" },
            f.default
                .as_ref()
                .map(|d| format!(" default={d}"))
                .unwrap_or_default(),
            if f.sensitive { " sensitive" } else { "" },
        );
    }
    for c in &node.children {
        describe_into(c, out);
    }
}
