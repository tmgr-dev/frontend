use std::collections::HashMap;

use percent_encoding::percent_decode_str;
use serde_json::Value;

use crate::context::Context;
use crate::error::Error;

pub(crate) struct Request<'a> {
    pub conn: &'a rusqlite::Connection,
    pub ctx: &'a Context,
    pub params: HashMap<String, String>,
    pub query: Query,
}

impl Request<'_> {
    pub fn param(&self, key: &str) -> &str {
        self.params.get(key).map_or("", String::as_str)
    }

    pub fn id(&self, key: &str) -> i64 {
        self.param(key).parse().unwrap_or(i64::MAX)
    }
}

/// A handler result before the `{data}` envelope.
pub(crate) enum Outcome {
    /// The TS router has a route here that this crate does not answer yet.
    Unrouted,
    Http {
        status: u16,
        body: Value,
    },
    Failed(Error),
}

impl From<Error> for Outcome {
    fn from(e: Error) -> Self {
        Outcome::Failed(e)
    }
}

pub(crate) type Handled = Result<Value, Outcome>;
pub(crate) type Handler = fn(&Request<'_>) -> Handled;

pub(crate) fn unrouted(_: &Request<'_>) -> Handled {
    Err(Outcome::Unrouted)
}

pub(crate) fn http(status: u16, message: impl Into<String>) -> Outcome {
    Outcome::Http {
        status,
        body: serde_json::json!({ "message": message.into() }),
    }
}

pub(crate) fn not_found(what: &str) -> Outcome {
    http(404, format!("{what} not found"))
}

pub(crate) fn sql<T>(r: crate::error::Result<T>) -> Result<T, Outcome> {
    r.map_err(Outcome::Failed)
}

/// `URLSearchParams` over the query string: `+` is a space, the last write of `set` wins, `get` the first.
#[derive(Default)]
pub(crate) struct Query(Vec<(String, String)>);

impl Query {
    pub fn parse(raw: &str) -> Self {
        Query(
            form_urlencoded::parse(raw.as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect(),
        )
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    pub fn has(&self, key: &str) -> bool {
        self.0.iter().any(|(k, _)| k == key)
    }

    /// A non-empty value, like the `if (query.get(x))` checks of the TS API.
    pub fn filled(&self, key: &str) -> Option<&str> {
        self.get(key).filter(|v| !v.is_empty())
    }
}

enum Segment {
    Literal(&'static str),
    Any(&'static str),
    Digits(&'static str),
}

pub(crate) struct Route {
    pub method: &'static str,
    pub status: u16,
    pub handler: Handler,
    segments: Vec<Segment>,
}

impl Route {
    pub fn new(method: &'static str, pattern: &'static str, handler: Handler) -> Self {
        let segments = pattern
            .split('/')
            .map(|s| match s.strip_prefix(':') {
                Some(name) => match name.strip_suffix("(\\d+)") {
                    Some(name) => Segment::Digits(name),
                    None => Segment::Any(name),
                },
                None => Segment::Literal(s),
            })
            .collect();
        Route {
            method,
            status: 200,
            handler,
            segments,
        }
    }

    pub fn matches(&self, path: &str) -> Option<HashMap<String, String>> {
        let parts: Vec<&str> = path.split('/').collect();
        if parts.len() != self.segments.len() {
            return None;
        }
        let mut params = HashMap::new();
        for (segment, part) in self.segments.iter().zip(parts) {
            match segment {
                Segment::Literal(lit) if *lit == part => {}
                Segment::Literal(_) => return None,
                Segment::Any(name) if !part.is_empty() => {
                    params.insert(name.to_string(), decode(part));
                }
                Segment::Digits(name)
                    if !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()) =>
                {
                    params.insert(name.to_string(), decode(part));
                }
                _ => return None,
            }
        }
        Some(params)
    }
}

fn decode(part: &str) -> String {
    percent_decode_str(part).decode_utf8_lossy().into_owned()
}

/// `normalizePath` of the TS router: no origin, no `api/` prefix, no query, no edge slashes.
pub(crate) fn normalize_path(url: &str) -> String {
    let mut path = url.split('?').next().unwrap_or("");
    for scheme in ["https://", "http://"] {
        if let Some(rest) = path.strip_prefix(scheme) {
            path = rest.find('/').map_or("", |i| &rest[i..]);
            break;
        }
    }
    let path = path.strip_prefix('/').unwrap_or(path);
    let path = path.strip_prefix("api/").unwrap_or(path);
    path.trim_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noop(_: &Request<'_>) -> Handled {
        Ok(Value::Null)
    }

    #[test]
    fn normalizes_paths_like_the_ts_router() {
        assert_eq!(normalize_path("/api/tasks?page=2"), "tasks");
        assert_eq!(normalize_path("https://x.test/api/tasks/5/"), "tasks/5");
        assert_eq!(normalize_path("tasks/5"), "tasks/5");
        assert_eq!(normalize_path("api/statuses/1"), "statuses/1");
    }

    #[test]
    fn digit_segments_only_match_digits() {
        let route = Route::new("GET", "tasks/:id(\\d+)", noop);
        assert_eq!(route.matches("tasks/42").unwrap()["id"], "42");
        assert!(route.matches("tasks/runned").is_none());
        assert!(route.matches("tasks/42/comments").is_none());
        let any = Route::new("GET", "tasks/status/:status", noop);
        assert_eq!(
            any.matches("tasks/status/In%20work").unwrap()["status"],
            "In work"
        );
    }

    #[test]
    fn query_follows_url_search_params() {
        let q = Query::parse("search=a+b&order%5Bcolumn%5D=order&all&x=1&x=2");
        assert_eq!(q.get("search"), Some("a b"));
        assert_eq!(q.get("order[column]"), Some("order"));
        assert!(q.has("all"));
        assert_eq!(q.get("x"), Some("1"));
        assert_eq!(q.filled("all"), None);
    }
}
