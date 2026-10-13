use crate::grammar;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Scope {
    #[serde(default)]
    pub repos: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub max: u32,
    #[serde(default)]
    pub from: Vec<String>,
}

impl Scope {
    pub fn widens(&self, old: &Scope) -> bool {
        let repos = self
            .repos
            .iter()
            .any(|r| !old.repos.iter().any(|o| Path::new(r).starts_with(o)));
        let agents = self.agents.iter().any(|a| !old.agents.contains(a));
        let from = !old.from.is_empty()
            && (self.from.is_empty()
                || self
                    .from
                    .iter()
                    .any(|f| !old.from.iter().any(|o| cidr_within(f, o))));
        let models = !old.models.is_empty()
            && (self.models.is_empty() || self.models.iter().any(|m| !old.models.contains(m)));
        repos || agents || models || self.max > old.max || from
    }

    pub fn summary(&self) -> String {
        let repos = if self.repos.is_empty() {
            "no repos".to_string()
        } else {
            format!("repo {}", self.repos.join(", "))
        };
        let agents = if self.agents.is_empty() {
            "no agents".to_string()
        } else {
            format!("agents {}", self.agents.join(", "))
        };
        let models = if self.models.is_empty() {
            "any model".to_string()
        } else {
            format!("models {}", self.models.join(", "))
        };
        let from = if self.from.is_empty() {
            "from any address".to_string()
        } else {
            format!("from {}", self.from.join(", "))
        };
        format!(
            "{repos} · {agents} · {models} · max {} running · {from} · bypass-permissions: forbidden",
            self.max
        )
    }
}

fn split_cidr(c: &str) -> Option<(std::net::IpAddr, u32)> {
    let (addr, prefix) = match c.split_once('/') {
        Some((a, p)) => (a, Some(p.parse::<u32>().ok()?)),
        None => (c, None),
    };
    let ip: std::net::IpAddr = addr.parse().ok()?;
    let bits = if ip.is_ipv4() { 32 } else { 128 };
    let prefix = prefix.unwrap_or(bits);
    (prefix <= bits).then_some((ip, prefix))
}

pub fn cidr_within(inner: &str, outer: &str) -> bool {
    let (Some((ip, ip_prefix)), Some((_, outer_prefix))) = (split_cidr(inner), split_cidr(outer))
    else {
        return false;
    };
    ip_prefix >= outer_prefix && crate::worker::cidr_contains(outer, ip)
}

pub fn canonical_repo(raw: &str) -> Result<String, String> {
    let p = Path::new(raw);
    if !p.is_absolute() {
        return Err("repo paths must be absolute".into());
    }
    let canon: PathBuf = std::fs::canonicalize(p).map_err(|_| "the path does not exist")?;
    if !canon.is_dir() {
        return Err("the path is not a directory".into());
    }
    canon
        .to_str()
        .map(str::to_string)
        .ok_or_else(|| "the path is not valid UTF-8".into())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reject {
    pub code: &'static str,
    pub reason: String,
}

impl Reject {
    fn bad(reason: impl Into<String>) -> Self {
        Reject {
            code: "bad_params",
            reason: reason.into(),
        }
    }

    fn forbidden(reason: impl Into<String>) -> Self {
        Reject {
            code: "forbidden",
            reason: reason.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Accepted {
    pub repo: Option<String>,
}

pub fn audit_args(params: &Value) -> Value {
    match params {
        Value::Object(m) => {
            let mut out = Map::new();
            for (k, v) in m.iter().take(12) {
                if let Value::String(s) = v {
                    out.insert(k.clone(), Value::String(s.clone()));
                }
            }
            Value::Object(out)
        }
        _ => Value::Object(Map::new()),
    }
}

fn only_keys(obj: &Map<String, Value>, allowed: &[&str]) -> Result<(), Reject> {
    if obj.contains_key("on") {
        return Err(Reject::forbidden("--on is not accepted over a link"));
    }
    match obj.keys().find(|k| !allowed.contains(&k.as_str())) {
        Some(k) => Err(Reject::bad(format!(
            "unknown parameter {}",
            tmgr_core::text::truncate_chars(k, 24).0
        ))),
        None => Ok(()),
    }
}

fn object(params: &Value) -> Result<Map<String, Value>, Reject> {
    match params {
        Value::Null => Ok(Map::new()),
        Value::Object(m) => Ok(m.clone()),
        _ => Err(Reject::bad("params must be an object")),
    }
}

fn string<'a>(obj: &'a Map<String, Value>, key: &str) -> Result<Option<&'a str>, Reject> {
    match obj.get(key) {
        None => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(Reject::bad(format!("{key} must be a string"))),
    }
}

fn required<'a>(obj: &'a Map<String, Value>, key: &str) -> Result<&'a str, Reject> {
    string(obj, key)?.ok_or_else(|| Reject::bad(format!("{key} is required")))
}

pub fn validate_run_method(
    method: &str,
    params: &Value,
    scope: &Scope,
    live_runs: u32,
) -> Result<Accepted, Reject> {
    let obj = object(params)?;
    match method {
        "runs.list" => only_keys(&obj, &[]).map(|_| Accepted::default()),
        "runs.get" | "runs.halt" => {
            only_keys(&obj, &["run"])?;
            let run = required(&obj, "run")?;
            if grammar::token(run, 64) && !run.contains('/') {
                Ok(Accepted::default())
            } else {
                Err(Reject::bad("invalid run id"))
            }
        }
        "runs.create" => validate_create(&obj, scope, live_runs),
        _ => Err(Reject::bad("not a run method")),
    }
}

fn validate_create(obj: &Map<String, Value>, scope: &Scope, live: u32) -> Result<Accepted, Reject> {
    only_keys(obj, &["task", "repo", "agent", "persona", "model", "mode"])?;
    let task = required(obj, "task")?;
    if !grammar::token(task, 64) || task.contains('/') {
        return Err(Reject::bad("invalid task reference"));
    }
    let agent = required(obj, "agent")?;
    grammar::agent_name(agent).map_err(|_| Reject::bad("invalid agent name"))?;
    if let Some(v) = string(obj, "persona")? {
        if !grammar::persona_name(v) {
            return Err(Reject::bad("invalid persona"));
        }
    }
    let model = string(obj, "model")?;
    if let Some(v) = model {
        if !grammar::model_name(v) {
            return Err(Reject::bad("invalid model"));
        }
    }
    match string(obj, "mode")? {
        None | Some("default") => {}
        Some("bypass") => return Err(Reject::forbidden("bypass not allowed")),
        Some(_) => return Err(Reject::bad("invalid mode")),
    }
    let repo = required(obj, "repo")?;
    let canon =
        canonical_repo(repo).map_err(|e| Reject::forbidden(format!("repo refused: {e}")))?;
    if !scope.repos.iter().any(|r| Path::new(&canon).starts_with(r)) {
        return Err(Reject::forbidden("repo is outside the allowed repos"));
    }
    if !scope.agents.iter().any(|a| a == agent) {
        return Err(Reject::forbidden("agent is not allowed"));
    }
    if !scope.models.is_empty() {
        match model {
            Some(m) if scope.models.iter().any(|a| a == m) => {}
            Some(_) => return Err(Reject::forbidden("model is not allowed")),
            None => return Err(Reject::forbidden("a model from the allowlist is required")),
        }
    }
    if live >= scope.max {
        return Err(Reject::forbidden("max running runs reached"));
    }
    Ok(Accepted { repo: Some(canon) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn scope(repo: &str) -> Scope {
        Scope {
            repos: vec![repo.to_string()],
            agents: vec!["claude-code".into(), "codex".into()],
            models: vec![],
            max: 2,
            from: vec![],
        }
    }

    fn create(repo: &str) -> Value {
        json!({"task":"TM-512","repo":repo,"agent":"codex"})
    }

    #[test]
    fn create_inside_the_scope_is_accepted() {
        let d = tempfile::tempdir().unwrap();
        let repo = canonical_repo(d.path().to_str().unwrap()).unwrap();
        let sub = format!("{repo}/sub");
        std::fs::create_dir(&sub).unwrap();
        let s = scope(&repo);
        assert!(validate_run_method("runs.create", &create(&repo), &s, 0).is_ok());
        assert!(validate_run_method("runs.create", &create(&sub), &s, 1).is_ok());
    }

    #[test]
    fn create_outside_the_scope_is_refused() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let ra = canonical_repo(a.path().to_str().unwrap()).unwrap();
        let rb = canonical_repo(b.path().to_str().unwrap()).unwrap();
        let s = scope(&ra);
        let err = validate_run_method("runs.create", &create(&rb), &s, 0).unwrap_err();
        assert_eq!(err.code, "forbidden");
        let dotdot = format!(
            "{ra}/../{}",
            Path::new(&rb).file_name().unwrap().to_str().unwrap()
        );
        assert!(validate_run_method("runs.create", &create(&dotdot), &s, 0).is_err());
        assert!(validate_run_method("runs.create", &create("/nonexistent-path-x"), &s, 0).is_err());
        assert!(validate_run_method("runs.create", &create("relative/dir"), &s, 0).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_out_of_the_scope_are_refused() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let ra = canonical_repo(a.path().to_str().unwrap()).unwrap();
        let link = format!("{ra}/escape");
        std::os::unix::fs::symlink(b.path(), &link).unwrap();
        let s = scope(&ra);
        assert!(validate_run_method("runs.create", &create(&link), &s, 0).is_err());
    }

    #[test]
    fn agent_mode_max_and_on_are_enforced() {
        let d = tempfile::tempdir().unwrap();
        let repo = canonical_repo(d.path().to_str().unwrap()).unwrap();
        let s = scope(&repo);
        let mut p = create(&repo);
        p["agent"] = json!("aider");
        assert_eq!(
            validate_run_method("runs.create", &p, &s, 0)
                .unwrap_err()
                .code,
            "forbidden"
        );
        let mut p = create(&repo);
        p["mode"] = json!("bypass");
        let e = validate_run_method("runs.create", &p, &s, 0).unwrap_err();
        assert_eq!(e.reason, "bypass not allowed");
        let mut p = create(&repo);
        p["on"] = json!("spark");
        assert_eq!(
            validate_run_method("runs.create", &p, &s, 0)
                .unwrap_err()
                .code,
            "forbidden"
        );
        assert!(validate_run_method("runs.create", &create(&repo), &s, 2).is_err());
        let mut zero = s.clone();
        zero.max = 0;
        assert!(validate_run_method("runs.create", &create(&repo), &zero, 0).is_err());
        assert!(validate_run_method("runs.create", &create(&repo), &Scope::default(), 0).is_err());
    }

    #[test]
    fn the_canonical_repo_is_passed_on() {
        let d = tempfile::tempdir().unwrap();
        let repo = canonical_repo(d.path().to_str().unwrap()).unwrap();
        let s = scope(&repo);
        let dotted = format!(
            "{repo}/../{}",
            Path::new(&repo).file_name().unwrap().to_str().unwrap()
        );
        let ok = validate_run_method("runs.create", &create(&dotted), &s, 0).unwrap();
        assert_eq!(ok.repo.as_deref(), Some(repo.as_str()));
    }

    #[test]
    fn models_are_an_allowlist_and_personas_cannot_be_paths() {
        let d = tempfile::tempdir().unwrap();
        let repo = canonical_repo(d.path().to_str().unwrap()).unwrap();
        let mut s = scope(&repo);
        s.models = vec!["claude-opus-4".into()];
        let with = |k: &str, v: &str| {
            let mut p = create(&repo);
            p[k] = json!(v);
            p
        };
        assert!(validate_run_method("runs.create", &with("model", "claude-opus-4"), &s, 0).is_ok());
        assert_eq!(
            validate_run_method("runs.create", &with("model", "other"), &s, 0)
                .unwrap_err()
                .reason,
            "model is not allowed"
        );
        assert!(validate_run_method("runs.create", &create(&repo), &s, 0).is_err());
        assert!(validate_run_method("runs.create", &create(&repo), &scope(&repo), 0).is_ok());
        let free = scope(&repo);
        for bad in ["../x", "/etc/passwd", "a/b", ".hidden"] {
            assert!(
                validate_run_method("runs.create", &with("persona", bad), &free, 0).is_err(),
                "{bad}"
            );
        }
        assert!(validate_run_method("runs.create", &with("persona", "Builder"), &free, 0).is_ok());
    }

    #[test]
    fn parameter_shapes_are_strict() {
        let s = Scope::default();
        assert!(validate_run_method("runs.list", &Value::Null, &s, 0).is_ok());
        assert!(validate_run_method("runs.list", &json!({"x":1}), &s, 0).is_err());
        assert!(validate_run_method("runs.get", &json!({"run":"r-1"}), &s, 0).is_ok());
        assert!(validate_run_method("runs.get", &json!({"run":"../x"}), &s, 0).is_err());
        assert!(validate_run_method("runs.halt", &json!({}), &s, 0).is_err());
        assert!(validate_run_method("runs.halt", &json!([1]), &s, 0).is_err());
        assert!(validate_run_method("runs.get", &json!({"run":5}), &s, 0).is_err());
    }

    #[test]
    fn widening_is_detected() {
        let base = Scope {
            repos: vec!["/srv/a".into()],
            agents: vec!["codex".into()],
            models: vec!["m1".into()],
            max: 2,
            from: vec!["192.0.2.0/24".into()],
        };
        assert!(!base.widens(&base));
        let mut narrower = base.clone();
        narrower.repos = vec!["/srv/a/sub".into()];
        narrower.max = 1;
        narrower.from = vec!["192.0.2.0/24".into()];
        assert!(!narrower.widens(&base));
        for f in [
            |s: &mut Scope| s.repos.push("/srv/b".into()),
            |s: &mut Scope| s.repos = vec!["/srv".into()],
            |s: &mut Scope| s.agents.push("claude-code".into()),
            |s: &mut Scope| s.max = 3,
            |s: &mut Scope| s.models.clear(),
            |s: &mut Scope| s.models.push("m2".into()),
            |s: &mut Scope| s.from.clear(),
            |s: &mut Scope| s.from.push("198.51.100.1".into()),
        ] {
            let mut wider = base.clone();
            f(&mut wider);
            assert!(wider.widens(&base));
        }
        let mut any_to_cidr = base.clone();
        any_to_cidr.from.clear();
        assert!(!base.widens(&any_to_cidr));
    }

    #[test]
    fn cidrs_are_compared_by_containment() {
        assert!(cidr_within("192.0.2.0/25", "192.0.2.0/24"));
        assert!(cidr_within("192.0.2.77", "192.0.2.0/24"));
        assert!(cidr_within("192.0.2.0/24", "192.0.2.0/24"));
        assert!(!cidr_within("192.0.2.0/24", "192.0.2.0/25"));
        assert!(!cidr_within("192.0.2.0/23", "192.0.2.0/24"));
        assert!(!cidr_within("198.51.100.0/24", "192.0.2.0/24"));
        assert!(!cidr_within("2001:db8::/48", "192.0.2.0/24"));
        assert!(cidr_within("2001:db8:1::/48", "2001:db8::/32"));
        assert!(!cidr_within("nonsense", "192.0.2.0/24"));
        let old = Scope {
            from: vec!["192.0.2.0/24".into()],
            ..Scope::default()
        };
        let narrower = Scope {
            from: vec!["192.0.2.128/25".into(), "192.0.2.5".into()],
            ..Scope::default()
        };
        let wider = Scope {
            from: vec!["192.0.0.0/16".into()],
            ..Scope::default()
        };
        assert!(!narrower.widens(&old));
        assert!(wider.widens(&old));
    }

    #[test]
    fn audit_args_keep_only_short_string_fields() {
        let a = audit_args(&json!({"task":"TM-1","n":5,"nested":{"a":1}}));
        assert_eq!(a, json!({"task":"TM-1"}));
    }
}
