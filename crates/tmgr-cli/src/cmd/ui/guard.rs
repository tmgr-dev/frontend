use super::cmdline::{Value, Values};
use super::model::Node;

pub struct Rule {
    pub path: &'static [&'static str],
    pub arg: Option<&'static str>,
    pub value: Option<&'static str>,
}

const fn rule(
    path: &'static [&'static str],
    arg: Option<&'static str>,
    value: Option<&'static str>,
) -> Rule {
    Rule { path, arg, value }
}

pub const GUARDED: &[Rule] = &[
    rule(&["task", "confirm"], None, None),
    rule(&["runner", "allow"], None, None),
    rule(&["runner", "trust"], None, None),
    rule(&["runner", "bind"], None, None),
    rule(&["runner", "unbind"], None, None),
    rule(&["runner", "install-service"], Some("install"), None),
    rule(&["run"], Some("mode"), Some("bypass")),
    rule(&["run"], Some("no_sandbox"), None),
    rule(&["run"], Some("trust"), None),
    rule(&["run"], Some("publish"), None),
    rule(&["run"], Some("review"), None),
    rule(&["run"], Some("push"), None),
    rule(&["run"], Some("drop"), None),
    rule(&["queue", "enable"], None, None),
    rule(&["queue", "approve"], None, None),
    rule(&["queue", "release"], None, None),
    rule(&["queue", "rules", "add"], None, None),
    rule(&["queue", "autopilot"], None, None),
    rule(&["queue", "autopilot", "start"], None, None),
    rule(&["autopilot"], None, None),
    rule(&["autopilot", "start"], None, None),
    rule(&["usage", "sync", "on"], None, None),
    rule(&["self-update"], Some("allow_downgrade"), None),
    rule(&["git", "install-hooks"], Some("global"), None),
    rule(&["init"], Some("install"), None),
    rule(&["hook", "install"], None, None),
];

pub fn is_guarded(node: &Node, values: &Values) -> bool {
    GUARDED.iter().any(|r| {
        if !r
            .path
            .iter()
            .map(|s| s.to_string())
            .eq(node.path.iter().cloned())
        {
            return false;
        }
        let Some(arg) = r.arg else { return true };
        match (values.get(arg), r.value) {
            (Some(Value::Bool(b)), None) => *b,
            (Some(Value::Text(t)), Some(v)) => t == v,
            (Some(Value::Text(t)), None) => !t.is_empty(),
            _ => false,
        }
    })
}

#[cfg(test)]
pub fn rule_resolves(root: &Node, r: &Rule) -> bool {
    use super::model::Kind;
    let path: Vec<String> = r.path.iter().map(|s| s.to_string()).collect();
    let Some(node) = root.find(&path) else {
        return false;
    };
    let Some(arg) = r.arg else { return true };
    let Some(field) = node.fields.iter().find(|f| f.id == arg) else {
        return false;
    };
    match (r.value, &field.kind) {
        (Some(v), Kind::Select(options)) => options.iter().any(|o| o == v),
        (Some(_), _) => false,
        (None, _) => true,
    }
}
