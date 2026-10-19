use super::{emit, Opts};
use crate::ctx::Ctx;
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use tmgr_core::approval::{
    fingerprint, valid_public_key, AllowFile, Entry, PinnedKey, RepoBinding,
};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::text::sanitize_line;
use tmgr_link::privileged;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lifetime {
    Hours(i64),
    Days(i64),
    Forever,
}

impl Lifetime {
    pub fn parse(raw: &str) -> std::result::Result<Lifetime, String> {
        let bad =
            || format!("{raw:?} is not forever or a number of hours or days like 8h, 7d, 30d");
        if raw == "forever" {
            return Ok(Lifetime::Forever);
        }
        let (digits, unit) = raw.split_at(raw.len().saturating_sub(1));
        let n: i64 = digits.parse().map_err(|_| bad())?;
        if !(1..=3650).contains(&n) {
            return Err(bad());
        }
        match unit {
            "h" => Ok(Lifetime::Hours(n)),
            "d" => Ok(Lifetime::Days(n)),
            _ => Err(bad()),
        }
    }

    pub fn expires_at(self, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            Lifetime::Hours(n) => Some(now + Duration::hours(n)),
            Lifetime::Days(n) => Some(now + Duration::days(n)),
            Lifetime::Forever => None,
        }
    }
}

#[derive(Deserialize)]
struct ServerKey {
    key_id: String,
    public_key: String,
    #[serde(default)]
    device_name: String,
    #[serde(default)]
    created_at: Option<String>,
}

pub fn host_of(server: &str) -> Result<String> {
    let url = tmgr_core::Url::parse(server)
        .map_err(|_| Error::usage("the server address is not a valid URL"))?;
    let host = url
        .host_str()
        .ok_or_else(|| Error::usage("the server address has no host"))?
        .to_ascii_lowercase();
    Ok(match url.port() {
        Some(p) => format!("{host}:{p}"),
        None => host,
    })
}

fn stamp(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn store_error(e: String) -> Error {
    Error::internal(format!("runner allow list: {e}"))
}

fn live_keys(raw: Value) -> Result<Vec<(PinnedKey, Option<String>)>> {
    let list: Vec<ServerKey> = serde_json::from_value(raw)
        .map_err(|_| Error::internal("unexpected answer to the signing key list"))?;
    Ok(list
        .into_iter()
        .filter(|k| {
            k.key_id.len() <= 64
                && k.key_id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
                && valid_public_key(&k.public_key)
        })
        .map(|k| {
            (
                PinnedKey {
                    key_id: k.key_id,
                    public_key: k.public_key,
                    device_name: sanitize_line(&k.device_name).chars().take(100).collect(),
                },
                k.created_at.map(|c| sanitize_line(&c)),
            )
        })
        .collect())
}

pub fn key_conflict(previous: &Entry, keys: &[(PinnedKey, Option<String>)]) -> Result<()> {
    for (k, _) in keys {
        if let Some(old) = previous.key(&k.key_id) {
            if old.public_key != k.public_key {
                return Err(Error::new(
                    Kind::Conflict,
                    format!(
                        "key {} is pinned with fingerprint {} but the server now reports {} for it",
                        sanitize_line(&k.key_id),
                        fingerprint(&old.public_key),
                        fingerprint(&k.public_key)
                    ),
                )
                .hint("delete that key in the web app, then run this again"));
            }
        }
    }
    Ok(())
}

pub fn allow(o: &Opts, workspace: &str, lifetime: Lifetime) -> Result<i32> {
    privileged::guard()?;
    if workspace.starts_with(tmgr_core::local::PREFIX) {
        return Err(Error::usage(
            "only workspaces in the cloud can be allowed; local workspaces run tasks without it",
        ));
    }
    let ctx = Ctx::new(o.cli)?;
    let client = ctx.cloud_client()?;
    let host = host_of(&ctx.server()?)?;
    let user_id = client
        .current_user_id()?
        .ok_or_else(|| Error::auth("cannot tell which account is logged in").hint("tmgr login"))?;
    let ws = ctx
        .workspaces(client)?
        .into_iter()
        .find(|w| w.code.eq_ignore_ascii_case(workspace) && !w.local)
        .ok_or_else(|| {
            Error::not_found(format!("workspace {} not found", sanitize_line(workspace)))
                .hint("tmgr ws list")
        })?;
    let keys = live_keys(client.agent_run_keys()?)?;
    if keys.is_empty() {
        return Err(Error::new(
            Kind::Conflict,
            "your account has no signing key for agent runs",
        )
        .hint("open a task of this workspace in the web app or the desktop app once to create one, then run this again"));
    }
    let file = AllowFile::load().map_err(store_error)?;
    let previous = file.find(&host, user_id, ws.id).cloned();
    if let Some(p) = &previous {
        key_conflict(p, &keys)?;
    }
    let now = Utc::now();
    let expires = lifetime.expires_at(now);
    eprintln!(
        "Allow this machine to start agents on tasks of workspace {} ({}) for {}?",
        sanitize_line(&ws.code),
        sanitize_line(&ws.name),
        expires.map_or("no end date".to_string(), |e| format!("until {}", stamp(e)))
    );
    eprintln!(
        "A task is started only if it carries a signature by one of the keys below, made over its exact title and description. \
These keys are pinned now; a key added later is not trusted until you run this again. Anyone who controls one of these keys can start agents on this machine."
    );
    for (k, created) in &keys {
        let fresh = previous
            .as_ref()
            .is_some_and(|p| p.key(&k.key_id).is_none());
        eprintln!(
            "  {}  {}  created {}{}",
            fingerprint(&k.public_key),
            if k.device_name.is_empty() {
                "(unnamed device)"
            } else {
                &k.device_name
            },
            created.as_deref().unwrap_or("unknown"),
            if fresh { "  NEW" } else { "" }
        );
    }
    privileged::confirm_typed_stdio(
        &ws.code,
        &format!("Type the workspace code ({}) to confirm:", ws.code),
    )?;
    let entry = AllowFile::update(|file| {
        let current = file.find(&host, user_id, ws.id).cloned();
        if let Some(p) = &current {
            key_conflict(p, &keys).map_err(|e| e.message)?;
        }
        let entry = Entry {
            server: host.clone(),
            user_id,
            workspace_id: ws.id,
            workspace_code: ws.code.clone(),
            allowed_at: stamp(now),
            expires_at: expires.map(stamp),
            keys: keys.iter().map(|(k, _)| k.clone()).collect(),
            repos: current.map(|p| p.repos).unwrap_or_default(),
        };
        file.upsert(entry.clone());
        Ok(entry)
    })
    .map_err(store_error)?;
    let data = describe(&entry, now);
    let human = format!(
        "workspace {} is allowed{}",
        sanitize_line(&ws.code),
        expires.map_or(String::new(), |e| format!(" until {}", stamp(e)))
    );
    Ok(emit(o, data, human))
}

pub fn disallow(o: &Opts, workspace: &str) -> Result<i32> {
    let ctx = Ctx::new(o.cli)?;
    let host = host_of(&ctx.server()?)?;
    let removed = AllowFile::update(|file| {
        let before = file.entries.len();
        file.entries
            .retain(|e| !(e.server == host && e.workspace_code.eq_ignore_ascii_case(workspace)));
        Ok(file.entries.len() != before)
    })
    .map_err(store_error)?;
    let human = if removed {
        format!(
            "workspace {} is no longer allowed",
            sanitize_line(workspace)
        )
    } else {
        format!("workspace {} was not allowed", sanitize_line(workspace))
    };
    Ok(emit(o, json!({ "removed": removed }), human))
}

pub fn describe(e: &Entry, now: DateTime<Utc>) -> Value {
    let days_left = e
        .expiry()
        .map(|at| (at - now).num_seconds().div_euclid(86_400));
    json!({
        "workspace_code": e.workspace_code,
        "workspace_id": e.workspace_id,
        "user_id": e.user_id,
        "server": e.server,
        "expires_at": e.expires_at,
        "days_left": days_left,
        "expiring_soon": days_left.is_some_and(|d| d <= 3),
        "keys": e.keys.iter().map(|k| json!({
            "key_id": k.key_id,
            "device_name": k.device_name,
            "fingerprint": fingerprint(&k.public_key),
        })).collect::<Vec<_>>(),
        "repos": e.repos,
    })
}

pub fn set_lines(entries: &[Entry]) -> Vec<String> {
    let mut lines = Vec::new();
    for e in entries {
        for (category, binding) in &e.repos {
            if let RepoBinding::Set(set) = binding {
                lines.push(format!(
                    "{} category {}:",
                    sanitize_line(&e.workspace_code),
                    sanitize_line(category)
                ));
                for (name, path) in set {
                    lines.push(format!(
                        "  {}  {}",
                        sanitize_line(name),
                        sanitize_line(path)
                    ));
                }
            }
        }
    }
    lines
}

pub fn allowed(o: &Opts) -> Result<i32> {
    let file = AllowFile::load().map_err(store_error)?;
    let now = Utc::now();
    let data: Vec<Value> = file.entries.iter().map(|e| describe(e, now)).collect();
    let human = if file.entries.is_empty() {
        "no workspace is allowed to run agents here".to_string()
    } else {
        let mut rows = vec![vec![
            "WORKSPACE".into(),
            "SERVER".into(),
            "EXPIRES".into(),
            "KEYS".into(),
        ]];
        for e in &file.entries {
            let expires = match (&e.expires_at, e.expired(now)) {
                (None, _) => "never".to_string(),
                (Some(at), true) => format!("expired {}", sanitize_line(at)),
                (Some(at), false) => sanitize_line(at),
            };
            rows.push(vec![
                sanitize_line(&e.workspace_code),
                sanitize_line(&e.server),
                expires,
                e.keys
                    .iter()
                    .map(|k| fingerprint(&k.public_key))
                    .collect::<Vec<_>>()
                    .join(", "),
            ]);
        }
        let mut out = crate::out::table(&rows);
        for line in set_lines(&file.entries) {
            out.push('\n');
            out.push_str(&line);
        }
        out
    };
    Ok(emit(o, data, human))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifetimes_parse_and_reject_garbage() {
        assert_eq!(Lifetime::parse("8h"), Ok(Lifetime::Hours(8)));
        assert_eq!(Lifetime::parse("30d"), Ok(Lifetime::Days(30)));
        assert_eq!(Lifetime::parse("forever"), Ok(Lifetime::Forever));
        for bad in ["", "0d", "-1d", "d", "5m", "1.5d", "99999d", "forever1"] {
            assert!(Lifetime::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn host_keeps_non_default_ports_only() {
        assert_eq!(host_of("https://api.tmgr.dev").unwrap(), "api.tmgr.dev");
        assert_eq!(
            host_of("https://API.tmgr.dev:443/").unwrap(),
            "api.tmgr.dev"
        );
        assert_eq!(host_of("http://localhost:8080").unwrap(), "localhost:8080");
        assert!(host_of("nonsense").is_err());
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;
    use std::collections::BTreeMap;

    const A: &str =
        "BImU1e2SPEdD/HKLzjfuQlNuLgKE44/qEF5a+626Bj0KtbPSIGIwmRlfUMIyP75vzLHw7BUXRArO87Ad8xnmazE=";
    const B: &str =
        "BD7GudbwbOUVIvsBsw0CvYo7MXeWK1HUMFt+qGPDXCqeED93aEkCI5814pbvM3gcEwEBre/NlqieFT46CB9RzdA=";

    fn pin(id: &str, public: &str) -> PinnedKey {
        PinnedKey {
            key_id: id.into(),
            public_key: public.into(),
            device_name: "d".into(),
        }
    }

    fn entry() -> Entry {
        Entry {
            server: "s".into(),
            user_id: 1,
            workspace_id: 2,
            workspace_code: "w".into(),
            allowed_at: "2025-10-01T00:00:00Z".into(),
            expires_at: None,
            keys: vec![pin("k1", A)],
            repos: BTreeMap::new(),
        }
    }

    #[test]
    fn a_pinned_key_id_with_another_public_key_is_refused_naming_both_fingerprints() {
        let e = key_conflict(&entry(), &[(pin("k1", B), None)]).unwrap_err();
        assert!(e.message.contains("k1"), "{}", e.message);
        assert!(e.message.contains(&fingerprint(A)) && e.message.contains(&fingerprint(B)));
        assert!(e.hint.unwrap_or_default().contains("delete that key"));
        key_conflict(&entry(), &[(pin("k1", A), None), (pin("k2", B), None)]).unwrap();
    }

    #[test]
    fn a_set_binding_is_listed_with_its_paths_and_a_single_path_is_not() {
        let mut e = entry();
        e.repos.insert("1".into(), RepoBinding::Path("/one".into()));
        e.repos.insert(
            "2".into(),
            RepoBinding::Set(
                [("a/x", "/p1"), ("a/y", "/p2")]
                    .iter()
                    .map(|(n, p)| (n.to_string(), p.to_string()))
                    .collect(),
            ),
        );
        assert_eq!(
            set_lines(&[e]),
            ["w category 2:", "  a/x  /p1", "  a/y  /p2"]
        );
    }
}
