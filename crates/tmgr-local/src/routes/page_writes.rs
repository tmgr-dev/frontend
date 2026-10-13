//! Port of the write side of the TS `pages/service.ts` and `pages/mentions.ts`. The surrounding
//! request transaction replaces the TS write queue and version-guarded batch, so a write never races.

use std::collections::HashSet;

use rusqlite::types::Value as Sql;
use serde_json::{json, Map, Value};

use super::pages::{assemble, page_not_found, resolve};
use super::{int, text};
use crate::db::{execute, select, select_one, Row};
use crate::js;
use crate::markdown::{
    self, append_in_section, append_new_heading, append_new_section, append_to_end,
    append_under_heading, autolink_task_keys, contains_section_marker, exceeds_limit,
    extract_links, find_section, is_chronicle_heading, is_known_type, non_human_violation,
    replace_section, structure_error, system_section_violation, task_key_candidates, template_body,
    valid_section_id, writable_by, AGENTS_OWNER, CONTEXT, MAX_LINKS, MEETING, PERSON,
    PROMISES_SECTION,
};
use crate::router::{sql, Handled, Outcome, Request};
use crate::serialize::{col, task_key};

const MAX_TITLE: usize = 255;
const MAX_SUMMARY: usize = 255;
const MAX_SLUG_ATTEMPTS: usize = 6;
const LAST_CONTACT_AT: &str = "last_contact_at";
const ANALYST_SETTING: &str = "pages.analyst_persona_id";

/// Who writes a page: `user`, `persona`, `plugin` or `system`, with the reference stored next to it.
#[derive(Clone)]
pub(crate) struct Who {
    kind: String,
    reference: String,
}

impl Who {
    pub(crate) fn of(req: &Request<'_>) -> Who {
        match req.ctx.actor.as_ref() {
            None => Who::user(req),
            Some(a) if a.kind == "user" => Who::user(req),
            Some(a) => Who {
                kind: match a.kind.as_str() {
                    "persona" | "system" => a.kind.clone(),
                    _ => "plugin".into(),
                },
                reference: a.id.clone(),
            },
        }
    }

    fn user(req: &Request<'_>) -> Who {
        Who {
            kind: "user".into(),
            reference: req.ctx.user.id.to_string(),
        }
    }

    fn system() -> Who {
        Who {
            kind: "system".into(),
            reference: "system".into(),
        }
    }

    fn human(&self) -> bool {
        self.kind == "user"
    }
}

fn page_error(status: u16, error: &str, message: &str, extra: Map<String, Value>) -> Outcome {
    let mut body = json!({ "message": message, "error": error });
    if let Value::Object(map) = &mut body {
        map.extend(extra);
    }
    Outcome::Http { status, body }
}

fn forbidden(error: &str, message: &str) -> Outcome {
    page_error(403, error, message, Map::new())
}

fn unprocessable(error: &str, message: &str) -> Outcome {
    page_error(422, error, message, Map::new())
}

fn too_large() -> Outcome {
    page_error(413, "page_too_large", "Page body exceeds 1 MB", Map::new())
}

fn conflict(req: &Request<'_>, row: &Row) -> Outcome {
    match assemble(req, row) {
        Ok(current) => {
            let mut extra = Map::new();
            extra.insert("data".into(), current);
            page_error(409, "page_conflict", "Page was changed", extra)
        }
        Err(e) => e,
    }
}

fn s(row: &Row, key: &str) -> String {
    js::string(col(row, key))
}

fn same_json(a: &str, b: &str) -> bool {
    let parse = |s: &str| serde_json::from_str::<Value>(s).unwrap_or(Value::Null);
    parse(a) == parse(b)
}

fn require_title(title: Option<&Value>) -> Result<String, Outcome> {
    let Some(t) = title
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty())
    else {
        return Err(unprocessable("title_required", "title is required"));
    };
    if js::length(t) > MAX_TITLE {
        return Err(unprocessable(
            "title_too_long",
            &format!("title must be at most {MAX_TITLE} characters"),
        ));
    }
    Ok(t.to_string())
}

fn clean_summary(raw: Option<&Value>) -> Option<String> {
    let s = raw?.as_str()?.trim();
    if s.is_empty() {
        return None;
    }
    let units: Vec<u16> = s.encode_utf16().collect();
    Some(if units.len() > MAX_SUMMARY {
        String::from_utf16_lossy(&units[..MAX_SUMMARY])
    } else {
        s.to_string()
    })
}

fn absent(v: Option<&Value>) -> bool {
    v.is_none_or(Value::is_null)
}

// ── properties ───────────────────────────────────────────────────────────────

fn integer(v: &Value) -> Option<i64> {
    match v {
        Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_f64().filter(|f| f.fract() == 0.0).map(|f| f as i64)),
        _ => None,
    }
}

fn prepare_properties(
    kind: &str,
    raw: Option<&Value>,
    existing: Option<&Value>,
) -> Result<Value, Outcome> {
    let input = match raw {
        None | Some(Value::Null) => Map::new(),
        Some(Value::Object(m)) => m.clone(),
        Some(_) => {
            return Err(invalid_properties(
                json!({ "properties": "must be an object" }),
            ))
        }
    };
    if kind != PERSON && kind != MEETING {
        return Ok(Value::Object(input));
    }
    let mut errors = Map::new();
    let mut err = |k: &str, m: &str| {
        errors.insert(k.to_string(), json!(m));
    };
    let known: &[&str] = if kind == PERSON {
        &[
            "user_id",
            "aliases",
            "network",
            "company",
            "role",
            LAST_CONTACT_AT,
        ]
    } else {
        &["date", "participants", "related_tasks"]
    };
    for key in input.keys() {
        if !known.contains(&key.as_str()) {
            err(key, "is not a property of this page type");
        }
    }
    let get = |k: &str| input.get(k).filter(|v| !v.is_null());
    let out = if kind == PERSON {
        let user_id = match get("user_id") {
            None => Value::Null,
            Some(v) => integer(v).map(Value::from).unwrap_or_else(|| {
                err("user_id", "must be an integer or null");
                Value::Null
            }),
        };
        let aliases = match get("aliases") {
            None => json!([]),
            Some(Value::Array(items)) if items.len() <= 100 => {
                let mut out = Vec::new();
                for (i, alias) in items.iter().enumerate() {
                    let at = format!("aliases[{i}]");
                    let Some(a) = alias.as_object() else {
                        err(&at, "must be an object");
                        continue;
                    };
                    let source = a.get("source").cloned().unwrap_or(Value::Null);
                    let native = a.get("native_id").cloned().unwrap_or(Value::Null);
                    let display = a.get("display").cloned().unwrap_or(Value::Null);
                    let sources = [
                        "telegram",
                        "rocketchat",
                        "slack",
                        "github",
                        "email",
                        "other",
                    ];
                    if !source.as_str().is_some_and(|s| sources.contains(&s)) {
                        err(
                            &format!("{at}.source"),
                            "must be one of telegram, rocketchat, slack, github, email, other",
                        );
                    }
                    if !native
                        .as_str()
                        .is_some_and(|n| !n.trim().is_empty() && js::length(n) <= 255)
                    {
                        err(
                            &format!("{at}.native_id"),
                            "must be a non-empty string of at most 255 characters",
                        );
                    }
                    if !display.is_null() && display.as_str().is_none_or(|d| js::length(d) > 255) {
                        err(
                            &format!("{at}.display"),
                            "must be a string of at most 255 characters",
                        );
                    }
                    for key in a.keys() {
                        if !["source", "native_id", "display"].contains(&key.as_str()) {
                            err(&format!("{at}.{key}"), "is not a property of an alias");
                        }
                    }
                    let display = if display.is_null() {
                        json!("")
                    } else {
                        display
                    };
                    out.push(json!({ "source": source, "native_id": native, "display": display }));
                }
                Value::Array(out)
            }
            Some(_) => {
                err("aliases", "must be an array of at most 100 items");
                json!([])
            }
        };
        let network = match get("network") {
            None => Value::Null,
            Some(v)
                if v.as_str()
                    .is_some_and(|n| ["operational", "personal", "strategic"].contains(&n)) =>
            {
                v.clone()
            }
            Some(_) => {
                err(
                    "network",
                    "must be one of operational, personal, strategic or null",
                );
                Value::Null
            }
        };
        let mut text_of = |k: &str| match get(k) {
            None => Value::Null,
            Some(v) if v.as_str().is_some_and(|t| js::length(t) <= 255) => v.clone(),
            Some(_) => {
                err(k, "must be a string of at most 255 characters or null");
                Value::Null
            }
        };
        let company = text_of("company");
        let role = text_of("role");
        let last = existing
            .and_then(|e| e.get(LAST_CONTACT_AT))
            .filter(|v| !v.is_null())
            .cloned()
            .unwrap_or(Value::Null);
        json!({
            "user_id": user_id,
            "aliases": aliases,
            "network": network,
            "company": company,
            "role": role,
            LAST_CONTACT_AT: last,
        })
    } else {
        let date = match get("date") {
            None => Value::Null,
            Some(v)
                if v.as_str().is_some_and(|d| {
                    chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d")
                        .is_ok_and(|p| p.format("%Y-%m-%d").to_string() == d)
                }) =>
            {
                v.clone()
            }
            Some(_) => {
                err("date", "must be a date YYYY-MM-DD or null");
                Value::Null
            }
        };
        let participants = match get("participants") {
            None => json!([]),
            Some(Value::Array(items)) if items.len() <= 100 => {
                let mut out = Vec::new();
                for (i, item) in items.iter().enumerate() {
                    let ok = item.as_str().is_some_and(|t| {
                        let rest = t
                            .strip_prefix("tmgr://page/")
                            .or_else(|| t.strip_prefix("tmgr://user/"));
                        rest.is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit()))
                    });
                    if ok {
                        out.push(item.clone());
                    } else {
                        err(
                            &format!("participants[{i}]"),
                            "must be tmgr://page/<id> or tmgr://user/<id>",
                        );
                    }
                }
                Value::Array(out)
            }
            Some(_) => {
                err(
                    "participants",
                    "must be an array of at most 100 tmgr://page/<id> or tmgr://user/<id> links",
                );
                json!([])
            }
        };
        let related = match get("related_tasks") {
            None => json!([]),
            Some(Value::Array(items)) if items.len() <= 100 => {
                let mut out = Vec::new();
                for (i, item) in items.iter().enumerate() {
                    match integer(item).filter(|id| *id > 0) {
                        Some(id) => out.push(json!(id)),
                        None => err(&format!("related_tasks[{i}]"), "must be a task id"),
                    }
                }
                Value::Array(out)
            }
            Some(_) => {
                err("related_tasks", "must be an array of at most 100 task ids");
                json!([])
            }
        };
        json!({ "date": date, "participants": participants, "related_tasks": related })
    };
    if !errors.is_empty() {
        return Err(invalid_properties(Value::Object(errors)));
    }
    Ok(out)
}

fn invalid_properties(errors: Value) -> Outcome {
    let mut extra = Map::new();
    extra.insert("errors".into(), errors);
    page_error(
        422,
        "invalid_properties",
        "Page properties are invalid",
        extra,
    )
}

fn properties_json(
    kind: &str,
    raw: Option<&Value>,
    existing: Option<&Value>,
) -> Result<String, Outcome> {
    Ok(js::stringify(&prepare_properties(kind, raw, existing)?))
}

// ── links and task keys ──────────────────────────────────────────────────────

struct LinkPlan {
    remove: Vec<(String, i64)>,
    add: Vec<(String, i64)>,
}

fn existing_targets(
    req: &Request<'_>,
    self_id: i64,
    candidates: &[(String, i64)],
) -> Result<Vec<(String, i64)>, Outcome> {
    let mut by_kind: Vec<(String, Vec<i64>)> = Vec::new();
    for (kind, id) in candidates.iter().take(MAX_LINKS) {
        if (kind == "page" && *id == self_id) || kind == "persona" {
            continue;
        }
        match by_kind.iter_mut().find(|(k, _)| k == kind) {
            Some((_, ids)) => ids.push(*id),
            None => by_kind.push((kind.clone(), vec![*id])),
        }
    }
    let mut found = Vec::new();
    for (kind, ids) in by_kind {
        if kind == "user" {
            if ids.contains(&req.ctx.user.id) {
                found.push(("user".to_string(), req.ctx.user.id));
            }
            continue;
        }
        let table = match kind.as_str() {
            "page" => "pages",
            "task" => "tasks",
            _ => "categories",
        };
        let marks = vec!["?"; ids.len()].join(",");
        let params: Vec<Sql> = ids.iter().map(|i| int(*i)).collect();
        for row in sql(select(
            req.conn,
            &format!("SELECT id FROM {table} WHERE deleted_at IS NULL AND id IN ({marks})"),
            &params,
        ))? {
            let id = col(&row, "id").as_i64().unwrap_or(0);
            if !found.contains(&(kind.clone(), id)) {
                found.push((kind.clone(), id));
            }
        }
    }
    Ok(found)
}

fn plan_links(req: &Request<'_>, self_id: Option<i64>, body: &str) -> Result<LinkPlan, Outcome> {
    let wanted = existing_targets(req, self_id.unwrap_or(-1), &extract_links(body))?;
    let stored: Vec<(String, i64)> = match self_id {
        None => Vec::new(),
        Some(id) => sql(select(
            req.conn,
            "SELECT target_kind, target_id FROM page_links WHERE page_id = ?",
            &[int(id)],
        ))?
        .iter()
        .map(|r| {
            (
                s(r, "target_kind"),
                col(r, "target_id").as_i64().unwrap_or(0),
            )
        })
        .collect(),
    };
    let mut before: Vec<(String, i64)> = Vec::new();
    for link in stored {
        if !before.contains(&link) {
            before.push(link);
        }
    }
    Ok(LinkPlan {
        remove: before
            .iter()
            .filter(|l| !wanted.contains(l))
            .cloned()
            .collect(),
        add: wanted
            .iter()
            .filter(|l| !before.contains(l))
            .cloned()
            .collect(),
    })
}

fn apply_links(req: &Request<'_>, page_id: i64, plan: &LinkPlan) -> Result<(), Outcome> {
    for (kind, id) in &plan.remove {
        sql(execute(
            req.conn,
            "DELETE FROM page_links WHERE page_id = ? AND target_kind = ? AND target_id = ?",
            &[int(page_id), text(kind), int(*id)],
        ))?;
    }
    for (kind, id) in &plan.add {
        sql(execute(
            req.conn,
            "INSERT OR IGNORE INTO page_links (page_id, target_kind, target_id) VALUES (?, ?, ?)",
            &[int(page_id), text(kind), int(*id)],
        ))?;
    }
    Ok(())
}

fn resolve_task_keys(req: &Request<'_>, keys: &[String]) -> Result<Vec<(String, i64)>, Outcome> {
    let mut out = Vec::new();
    for key in keys {
        let dash = key.rfind('-').unwrap_or(0);
        let code = &key[..dash];
        let Ok(sequence) = key[dash + 1..].parse::<i64>() else {
            continue;
        };
        let rows = sql(select(
            req.conn,
            "SELECT t.id, t.title, c.code, t.category_tasks_sequence_id AS seq FROM tasks t
			 LEFT JOIN categories c ON c.id = t.project_category_id
			 WHERE t.deleted_at IS NULL AND (t.title LIKE ? OR (c.code = ? AND t.category_tasks_sequence_id = ?)) LIMIT 20",
            &[text(&format!("{key}:%")), text(code), int(sequence)],
        ))?;
        let hit = rows.iter().find(|r| {
            task_key(
                col(r, "title").as_str().unwrap_or(""),
                col(r, "code").as_str(),
                col(r, "seq").as_i64(),
                col(r, "id").as_i64().unwrap_or(0),
            )
            .eq_ignore_ascii_case(key)
        });
        if let Some(r) = hit {
            out.push((key.clone(), col(r, "id").as_i64().unwrap_or(0)));
        }
    }
    Ok(out)
}

fn finalize_body(req: &Request<'_>, body: &str) -> Result<String, Outcome> {
    let keys = task_key_candidates(body);
    let linked = if keys.is_empty() {
        body.to_string()
    } else {
        autolink_task_keys(body, &resolve_task_keys(req, &keys)?)
    };
    if exceeds_limit(&linked) {
        return Err(too_large());
    }
    Ok(linked)
}

fn require_well_formed(body: &str) -> Result<(), Outcome> {
    match structure_error(body) {
        Some(why) => Err(unprocessable("invalid_sections", &why)),
        None => Ok(()),
    }
}

#[allow(clippy::too_many_arguments)]
fn require_writable(
    who: &Who,
    old_body: &str,
    new_body: &str,
    context_page: bool,
    old_title: &str,
    new_title: &str,
    old_props: &str,
    new_props: &str,
    new_section: Option<&str>,
) -> Result<(), Outcome> {
    if who.kind == "system" {
        return Ok(());
    }
    if let Some(why) = system_section_violation(old_body, new_body) {
        return Err(forbidden("section_forbidden", &why));
    }
    if who.human() {
        return Ok(());
    }
    if context_page && (old_title != new_title || !same_json(old_props, new_props)) {
        return Err(forbidden(
            "section_forbidden",
            "A context page's title and properties are for people",
        ));
    }
    match non_human_violation(
        old_body,
        new_body,
        context_page,
        &who.kind,
        &who.reference,
        new_section,
    ) {
        Some(why) => Err(forbidden("section_forbidden", &why)),
        None => Ok(()),
    }
}

// ── write core ───────────────────────────────────────────────────────────────

struct Change {
    title: String,
    body: String,
    properties: String,
    summary: Option<String>,
    new_section: Option<String>,
}

fn load_live(req: &Request<'_>, id: i64) -> Result<Option<Row>, Outcome> {
    sql(select_one(
        req.conn,
        "SELECT * FROM pages WHERE id = ? AND deleted_at IS NULL",
        &[int(id)],
    ))
}

/// Applies `plan` to the live page and stores a new version when anything changed.
fn write(
    req: &Request<'_>,
    who: &Who,
    page_id: i64,
    plan: impl FnOnce(&Row) -> Result<Change, Outcome>,
) -> Result<Row, Outcome> {
    let row = load_live(req, page_id)?.ok_or_else(page_not_found)?;
    let change = plan(&row)?;
    if exceeds_limit(&change.body) {
        return Err(too_large());
    }
    let old_body = s(&row, "body");
    let body_changed = change.body != old_body;
    require_writable(
        who,
        &old_body,
        &change.body,
        s(&row, "type") == CONTEXT,
        &s(&row, "title"),
        &change.title,
        &s(&row, "properties"),
        &change.properties,
        change.new_section.as_deref(),
    )?;
    if body_changed {
        require_well_formed(&change.body)?;
    }
    let final_body = if body_changed {
        finalize_body(req, &change.body)?
    } else {
        change.body.clone()
    };
    if !body_changed
        && change.title == s(&row, "title")
        && same_json(&change.properties, &s(&row, "properties"))
    {
        return Ok(row);
    }
    let version = col(&row, "version").as_i64().unwrap_or(0) + 1;
    let links = if body_changed {
        plan_links(req, Some(page_id), &final_body)?
    } else {
        LinkPlan {
            remove: Vec::new(),
            add: Vec::new(),
        }
    };
    let at = req.ctx.iso_now();
    let user = req.ctx.user.id;
    sql(execute(
        req.conn,
        "UPDATE pages SET title = ?, body = ?, properties = ?, version = ?,
				updated_by_id = ?, updated_by_kind = ?, updated_by_ref = ?, updated_at = ?
			 WHERE id = ? AND version = ? AND deleted_at IS NULL",
        &[
            text(&change.title),
            text(&final_body),
            text(&change.properties),
            int(version),
            int(user),
            text(&who.kind),
            text(&who.reference),
            text(&at),
            int(page_id),
            int(version - 1),
        ],
    ))?;
    sql(execute(
        req.conn,
        "INSERT INTO page_versions (page_id, version, title, body, properties, author_id, author_kind, author_ref, summary, created_at)
			 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        &[
            int(page_id),
            int(version),
            text(&change.title),
            text(&final_body),
            text(&change.properties),
            int(user),
            text(&who.kind),
            text(&who.reference),
            change.summary.map_or(Sql::Null, Sql::Text),
            text(&at),
        ],
    ))?;
    apply_links(req, page_id, &links)?;
    load_live(req, page_id)?.ok_or_else(page_not_found)
}

// ── create ───────────────────────────────────────────────────────────────────

const TRANSLIT: &[(char, &str)] = &[
    ('а', "a"),
    ('б', "b"),
    ('в', "v"),
    ('г', "g"),
    ('д', "d"),
    ('е', "e"),
    ('ё', "e"),
    ('ж', "zh"),
    ('з', "z"),
    ('и', "i"),
    ('й', "y"),
    ('к', "k"),
    ('л', "l"),
    ('м', "m"),
    ('н', "n"),
    ('о', "o"),
    ('п', "p"),
    ('р', "r"),
    ('с', "s"),
    ('т', "t"),
    ('у', "u"),
    ('ф', "f"),
    ('х', "h"),
    ('ц', "ts"),
    ('ч', "ch"),
    ('ш', "sh"),
    ('щ', "sch"),
    ('ъ', ""),
    ('ы', "y"),
    ('ь', ""),
    ('э', "e"),
    ('ю', "yu"),
    ('я', "ya"),
];

pub(crate) fn slugify(title: &str) -> String {
    let mut latin = String::new();
    for ch in title.chars() {
        let lower = ch.to_lowercase().next().unwrap_or(ch);
        match TRANSLIT.iter().find(|(c, _)| *c == lower) {
            Some((_, t)) => latin.push_str(t),
            None => latin.push(ch),
        }
    }
    let mut slug = String::new();
    for c in latin.to_lowercase().chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            slug.push(c);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let mut slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        return "page".into();
    }
    if slug.bytes().all(|b| b.is_ascii_digit())
        || ["tree", "trash", "search", "settings"].contains(&slug.as_str())
    {
        slug = format!("p-{slug}");
    }
    if slug.len() > 75 {
        slug = slug[..75].trim_end_matches('-').to_string();
    }
    slug
}

fn with_suffix(slug: &str) -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write(slug.as_bytes());
    let mut n = hasher.finish();
    let alphabet = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut suffix = String::new();
    for _ in 0..4 {
        suffix.push(alphabet[(n % 36) as usize] as char);
        n /= 36;
    }
    format!("{slug}-{suffix}")
}

fn slug_taken(req: &Request<'_>, slug: &str) -> Result<bool, Outcome> {
    Ok(sql(select_one(
        req.conn,
        "SELECT 1 AS x FROM pages WHERE slug = ? LIMIT 1",
        &[text(slug)],
    ))?
    .is_some())
}

fn number_of(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(f64::NAN),
        Value::String(s) => js::parse_number(s),
        Value::Bool(b) => *b as i64 as f64,
        _ => f64::NAN,
    }
}

fn safe_integer(f: f64) -> Option<i64> {
    (f.fract() == 0.0 && f.abs() <= 9_007_199_254_740_991.0).then_some(f as i64)
}

fn analyst_persona(req: &Request<'_>) -> Result<Option<String>, Outcome> {
    Ok(sql(select_one(
        req.conn,
        "SELECT value FROM meta WHERE key = ?",
        &[text(ANALYST_SETTING)],
    ))?
    .and_then(|r| col(&r, "value").as_str().map(str::to_string)))
}

pub(super) fn create(req: &Request<'_>) -> Handled {
    let who = Who::of(req);
    let b = &req.body;
    let title = require_title(b.get("title"))?;
    let kind = b
        .get("type")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .unwrap_or(markdown::PLAIN)
        .to_string();
    if !is_known_type(&kind) {
        return Err(unprocessable(
            "unsupported_type",
            &format!("Page type '{kind}' is not supported"),
        ));
    }
    if kind == CONTEXT && !who.human() {
        return Err(forbidden(
            "forbidden",
            "Only people can create context pages",
        ));
    }
    let parent = match b.get("parent_id").filter(|v| !v.is_null()) {
        None => None,
        Some(v) => Some(
            safe_integer(number_of(v))
                .ok_or_else(|| unprocessable("parent_not_found", "Parent page not found"))?,
        ),
    };
    let properties = properties_json(&kind, b.get("properties"), None)?;
    let from_template = absent(b.get("body"));
    let body = if from_template {
        let analyst = if kind == PERSON {
            analyst_persona(req)?
        } else {
            None
        };
        template_body(&kind, analyst.as_deref())
    } else {
        b.get("body")
            .and_then(Value::as_str)
            .ok_or_else(|| unprocessable("invalid_body", "body must be a string"))?
            .to_string()
    };
    if exceeds_limit(&body) {
        return Err(too_large());
    }
    if let Some(p) = parent {
        if load_live(req, p)?.is_none() {
            return Err(unprocessable("parent_not_found", "Parent page not found"));
        }
    }
    if !from_template {
        require_writable(
            &who,
            "",
            &body,
            false,
            &title,
            &title,
            &properties,
            &properties,
            None,
        )?;
    }
    require_well_formed(&body)?;
    let body = finalize_body(req, &body)?;
    let parent_param = parent.map_or(Sql::Null, Sql::Integer);
    let position = sql(select_one(
        req.conn,
        "SELECT COALESCE(MAX(position) + 1, 0) AS next FROM pages WHERE deleted_at IS NULL AND parent_id IS ?",
        std::slice::from_ref(&parent_param),
    ))?
    .and_then(|r| col(&r, "next").as_i64())
    .unwrap_or(0);
    let links = plan_links(req, None, &body)?;
    let at = req.ctx.iso_now();
    let user = req.ctx.user.id;
    let base = slugify(&title);
    let mut slug = if slug_taken(req, &base)? {
        with_suffix(&base)
    } else {
        base.clone()
    };
    let mut attempt = 0;
    let page_id = loop {
        let inserted = execute(
            req.conn,
            "INSERT INTO pages (parent_id, title, slug, type, body, properties, version, author_id, author_kind, author_ref,
							updated_by_id, updated_by_kind, updated_by_ref, position, pinned, created_at, updated_at)
						 VALUES (?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                parent_param.clone(),
                text(&title),
                text(&slug),
                text(&kind),
                text(&body),
                text(&properties),
                int(user),
                text(&who.kind),
                text(&who.reference),
                int(user),
                text(&who.kind),
                text(&who.reference),
                int(position),
                int((kind == CONTEXT) as i64),
                text(&at),
                text(&at),
            ],
        );
        match inserted {
            Ok(done) => break done.last_id,
            Err(crate::error::Error::Sql(why))
                if why.contains("UNIQUE") && attempt + 1 < MAX_SLUG_ATTEMPTS =>
            {
                attempt += 1;
                slug = with_suffix(&base);
            }
            Err(e) => return Err(Outcome::Failed(e)),
        }
    };
    sql(execute(
        req.conn,
        "INSERT INTO page_versions (page_id, version, title, body, properties, author_id, author_kind, author_ref, summary, created_at)
						 VALUES (?, 1, ?, ?, ?, ?, ?, ?, NULL, ?)",
        &[
            int(page_id),
            text(&title),
            text(&body),
            text(&properties),
            int(user),
            text(&who.kind),
            text(&who.reference),
            text(&at),
        ],
    ))?;
    apply_links(req, page_id, &links)?;
    let row = load_live(req, page_id)?.ok_or_else(page_not_found)?;
    assemble(req, &row)
}

// ── update, append, sections ────────────────────────────────────────────────

pub(super) fn update(req: &Request<'_>) -> Handled {
    let b = &req.body;
    if absent(b.get("version")) {
        return Err(unprocessable("version_required", "version is required"));
    }
    let page = resolve(req, req.param("id"))?;
    let id = col(&page, "id").as_i64().unwrap_or(0);
    let who = Who::of(req);
    let row = write(req, &who, id, |row| {
        if Some(number_of(&b["version"])) != col(row, "version").as_f64() {
            return Err(conflict(req, row));
        }
        let title = if absent(b.get("title")) {
            s(row, "title")
        } else {
            require_title(b.get("title"))?
        };
        let body = match b.get("body") {
            None | Some(Value::Null) => s(row, "body"),
            Some(Value::String(body)) => body.clone(),
            Some(_) => return Err(unprocessable("invalid_body", "body must be a string")),
        };
        if exceeds_limit(&body) {
            return Err(too_large());
        }
        let properties = if absent(b.get("properties")) {
            s(row, "properties")
        } else {
            let existing = js::parse_json(col(row, "properties"), json!({}));
            properties_json(&s(row, "type"), b.get("properties"), Some(&existing))?
        };
        Ok(Change {
            title,
            body,
            properties,
            summary: clean_summary(b.get("summary")),
            new_section: None,
        })
    })?;
    assemble(req, &row)
}

fn require_markdown(req: &Request<'_>, who: &Who, allow_blank: bool) -> Result<String, Outcome> {
    let md = req
        .body
        .get("markdown")
        .and_then(Value::as_str)
        .filter(|m| allow_blank || !m.trim().is_empty())
        .ok_or_else(|| unprocessable("markdown_required", "markdown is required"))?;
    if exceeds_limit(md) {
        return Err(too_large());
    }
    if !who.human() && contains_section_marker(md) {
        return Err(forbidden(
            "section_forbidden",
            "Agents cannot write section markers",
        ));
    }
    Ok(md.to_string())
}

pub(super) fn append(req: &Request<'_>) -> Handled {
    let who = Who::of(req);
    let md = require_markdown(req, &who, false)?;
    let page = resolve(req, req.param("id"))?;
    let id = col(&page, "id").as_i64().unwrap_or(0);
    let b = &req.body;
    let row = write(req, &who, id, |row| {
        let current = s(row, "body");
        let heading = b
            .get("heading")
            .and_then(Value::as_str)
            .filter(|h| !h.trim().is_empty());
        let body = if let Some(heading) = heading {
            match append_under_heading(&current, heading, &md) {
                Some(body) => body,
                None if b.get("create_heading") == Some(&Value::Bool(true)) => {
                    append_new_heading(&current, heading, &md)
                }
                None => {
                    return Err(unprocessable(
                        "heading_not_found",
                        &format!("Heading '{}' not found", heading.trim()),
                    ))
                }
            }
        } else if !who.human() && s(row, "type") == CONTEXT {
            let agents = (who.kind == "persona")
                .then(|| {
                    markdown::sections(&current)
                        .into_iter()
                        .find(|s| s.owner == AGENTS_OWNER)
                })
                .flatten()
                .ok_or_else(|| {
                    forbidden(
                        "section_forbidden",
                        "This context page has no section agents may write to",
                    )
                })?;
            append_in_section(&current, &agents, &md)
        } else {
            append_to_end(&current, &md)
        };
        let mut properties = s(row, "properties");
        if s(row, "type") == PERSON
            && is_chronicle_heading(b.get("heading").and_then(Value::as_str))
        {
            let mut props = match js::parse_json(col(row, "properties"), json!({})) {
                Value::Object(m) => m,
                _ => Map::new(),
            };
            props.insert(LAST_CONTACT_AT.into(), json!(req.ctx.iso_now()[..10]));
            properties = js::stringify(&Value::Object(props));
        }
        Ok(Change {
            title: s(row, "title"),
            body,
            properties,
            summary: clean_summary(b.get("summary")),
            new_section: None,
        })
    })?;
    assemble(req, &row)
}

pub(super) fn set_section(req: &Request<'_>) -> Handled {
    let who = Who::of(req);
    let md = require_markdown(req, &who, true)?;
    let page = resolve(req, req.param("id"))?;
    let id = col(&page, "id").as_i64().unwrap_or(0);
    let section_id = req.param("sectionId").to_string();
    let b = &req.body;
    let row = write(req, &who, id, |row| {
        let current = s(row, "body");
        let context_page = s(row, "type") == CONTEXT;
        let Some(section) = find_section(&current, &section_id) else {
            if who.human() {
                return Err(page_error(
                    404,
                    "section_not_found",
                    &format!("Section '{section_id}' not found"),
                    Map::new(),
                ));
            }
            if context_page {
                return Err(forbidden(
                    "section_forbidden",
                    "A context page has no room for new agent sections",
                ));
            }
            if !valid_section_id(&section_id) {
                return Err(unprocessable(
                    "invalid_section_id",
                    "Section id must match [a-z0-9][a-z0-9-]{0,63}",
                ));
            }
            let given = b.get("heading").filter(|h| !h.is_null());
            let heading = given
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|h| !h.is_empty())
                .unwrap_or(&section_id)
                .to_string();
            if given.is_some_and(|h| !h.is_string())
                || js::length(&heading) > 200
                || heading.contains(['\r', '\n'])
                || heading.contains("<!--")
            {
                return Err(unprocessable(
                    "invalid_heading",
                    "Heading must be one line of at most 200 characters",
                ));
            }
            return Ok(Change {
                title: s(row, "title"),
                body: append_new_section(
                    &current,
                    &section_id,
                    &format!("{}:{}", who.kind, who.reference),
                    &heading,
                    &md,
                ),
                properties: s(row, "properties"),
                summary: clean_summary(b.get("summary")),
                new_section: Some(section_id.clone()),
            });
        };
        if !who.human() && !writable_by(&section.owner, &who.kind, &who.reference, context_page) {
            return Err(forbidden(
                "section_forbidden",
                &format!("Section '{section_id}' is managed by {}", section.owner),
            ));
        }
        Ok(Change {
            title: s(row, "title"),
            body: replace_section(&current, &section, &md),
            properties: s(row, "properties"),
            summary: clean_summary(b.get("summary")),
            new_section: None,
        })
    })?;
    assemble(req, &row)
}

// ── tree operations ──────────────────────────────────────────────────────────

fn human_only(who: &Who) -> Result<(), Outcome> {
    if who.human() {
        Ok(())
    } else {
        Err(forbidden("forbidden", "Only people can do this on a page"))
    }
}

fn reload(req: &Request<'_>, id: i64) -> Result<Row, Outcome> {
    sql(select_one(
        req.conn,
        "SELECT * FROM pages WHERE id = ?",
        &[int(id)],
    ))?
    .ok_or_else(page_not_found)
}

fn next_position(req: &Request<'_>, parent: Sql) -> Result<i64, Outcome> {
    Ok(sql(select_one(
        req.conn,
        "SELECT COALESCE(MAX(position) + 1, 0) AS next FROM pages WHERE deleted_at IS NULL AND parent_id IS ?",
        &[parent],
    ))?
    .and_then(|r| col(&r, "next").as_i64())
    .unwrap_or(0))
}

fn set_pinned(req: &Request<'_>, pinned: bool) -> Handled {
    human_only(&Who::of(req))?;
    let page = resolve(req, req.param("id"))?;
    let id = col(&page, "id").as_i64().unwrap_or(0);
    load_live(req, id)?.ok_or_else(page_not_found)?;
    sql(execute(
        req.conn,
        "UPDATE pages SET pinned = ?, updated_at = ? WHERE id = ?",
        &[int(pinned as i64), text(&req.ctx.iso_now()), int(id)],
    ))?;
    assemble(req, &reload(req, id)?)
}

pub(super) fn pin(req: &Request<'_>) -> Handled {
    set_pinned(req, true)
}

pub(super) fn unpin(req: &Request<'_>) -> Handled {
    set_pinned(req, false)
}

pub(super) fn move_page(req: &Request<'_>) -> Handled {
    human_only(&Who::of(req))?;
    let page = resolve(req, req.param("id"))?;
    let parent = match req.body.get("parent_id").filter(|v| !v.is_null()) {
        None => None,
        Some(v) => Some(
            safe_integer(number_of(v))
                .ok_or_else(|| unprocessable("parent_not_found", "Parent page not found"))?,
        ),
    };
    let id = col(&page, "id").as_i64().unwrap_or(0);
    let row = load_live(req, id)?.ok_or_else(page_not_found)?;
    if let Some(p) = parent {
        if p == id {
            return Err(unprocessable(
                "page_cycle",
                "A page cannot be moved under itself",
            ));
        }
        if load_live(req, p)?.is_none() {
            return Err(unprocessable("parent_not_found", "Parent page not found"));
        }
        let cycle = sql(select_one(
            req.conn,
            "WITH RECURSIVE up (id, parent_id) AS (
				SELECT id, parent_id FROM pages WHERE id = ?
				UNION ALL SELECT p.id, p.parent_id FROM pages p JOIN up ON p.id = up.parent_id)
			 SELECT 1 AS x FROM up WHERE id = ? LIMIT 1",
            &[int(p), int(id)],
        ))?;
        if cycle.is_some() {
            return Err(unprocessable(
                "page_cycle",
                "A page cannot be moved under its own descendant",
            ));
        }
    }
    let parent_param = parent.map_or(Sql::Null, Sql::Integer);
    let mut siblings: Vec<i64> = sql(select(
        req.conn,
        "SELECT id FROM pages WHERE deleted_at IS NULL AND id <> ? AND parent_id IS ? ORDER BY pinned DESC, position ASC, id ASC",
        &[int(id), parent_param.clone()],
    ))?
    .iter()
    .filter_map(|r| col(r, "id").as_i64())
    .collect();
    let requested = req.body.get("position").map_or(f64::NAN, number_of);
    let index = if requested.is_finite() {
        (requested.floor().max(0.0) as usize).min(siblings.len())
    } else {
        siblings.len()
    };
    siblings.insert(index, col(&row, "id").as_i64().unwrap_or(id));
    let at = req.ctx.iso_now();
    for (position, sibling) in siblings.iter().enumerate() {
        let position = position as i64;
        if *sibling == id {
            sql(execute(
                req.conn,
                "UPDATE pages SET parent_id = ?, position = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
                &[parent_param.clone(), int(position), text(&at), int(id)],
            ))?;
        } else {
            sql(execute(
                req.conn,
                "UPDATE pages SET position = ? WHERE id = ? AND position <> ?",
                &[int(position), int(*sibling), int(position)],
            ))?;
        }
    }
    assemble(req, &reload(req, id)?)
}

fn id_list(rows: &[Row]) -> (String, Vec<Sql>) {
    let ids: Vec<Sql> = rows
        .iter()
        .filter_map(|r| col(r, "id").as_i64())
        .map(int)
        .collect();
    (vec!["?"; ids.len()].join(","), ids)
}

pub(super) fn delete(req: &Request<'_>) -> Handled {
    human_only(&Who::of(req))?;
    let page = resolve(req, req.param("id"))?;
    let id = col(&page, "id").as_i64().unwrap_or(0);
    load_live(req, id)?.ok_or_else(page_not_found)?;
    let subtree = sql(select(
        req.conn,
        "WITH RECURSIVE sub (id) AS (
				SELECT id FROM pages WHERE id = ?
				UNION ALL SELECT p.id FROM pages p JOIN sub s ON p.parent_id = s.id WHERE p.deleted_at IS NULL)
			 SELECT id FROM sub",
        &[int(id)],
    ))?;
    let (marks, ids) = id_list(&subtree);
    let mut params = vec![text(&req.ctx.iso_now())];
    params.extend(ids);
    let deleted = sql(execute(
        req.conn,
        &format!("UPDATE pages SET deleted_at = ? WHERE id IN ({marks}) AND deleted_at IS NULL"),
        &params,
    ))?
    .changes;
    Ok(json!({ "deleted": deleted }))
}

pub(super) fn restore(req: &Request<'_>) -> Handled {
    human_only(&Who::of(req))?;
    let page = super::pages::lookup(req, req.param("id"), true)?.ok_or_else(page_not_found)?;
    let id = col(&page, "id").as_i64().unwrap_or(0);
    let row = reload(req, id)?;
    let deleted_at = col(&row, "deleted_at").clone();
    if deleted_at.is_null() {
        return Err(unprocessable("page_not_deleted", "Page is not deleted"));
    }
    let deleted_param = text(&js::string(&deleted_at));
    let subtree = sql(select(
        req.conn,
        "WITH RECURSIVE sub (id) AS (
				SELECT id FROM pages WHERE id = ?
				UNION ALL SELECT p.id FROM pages p JOIN sub s ON p.parent_id = s.id WHERE p.deleted_at = ?)
			 SELECT id FROM sub",
        &[int(id), deleted_param.clone()],
    ))?;
    let (marks, ids) = id_list(&subtree);
    let orphaned = match col(&row, "parent_id").as_i64() {
        Some(parent) => load_live(req, parent)?.is_none(),
        None => false,
    };
    let position = if orphaned {
        Some(next_position(req, Sql::Null)?)
    } else {
        None
    };
    let mut params = vec![text(&req.ctx.iso_now()), deleted_param];
    params.extend(ids);
    let restored = sql(execute(
        req.conn,
        &format!("UPDATE pages SET deleted_at = NULL, updated_at = ? WHERE deleted_at = ? AND id IN ({marks})"),
        &params,
    ))?;
    if restored.changes == 0 {
        return Err(unprocessable("page_not_deleted", "Page is not deleted"));
    }
    if let Some(position) = position {
        sql(execute(
            req.conn,
            "UPDATE pages SET parent_id = NULL, position = ? WHERE id = ?",
            &[int(position), int(id)],
        ))?;
    }
    assemble(req, &reload(req, id)?)
}

// ── mentions and promises ────────────────────────────────────────────────────

/// Runs `work` in a savepoint; a failure rolls back only its own writes and is swallowed, as the TS does.
fn best_effort(
    req: &Request<'_>,
    name: &str,
    work: impl FnOnce() -> Result<(), Outcome>,
) -> Result<(), Outcome> {
    sql(req
        .conn
        .execute_batch(&format!("SAVEPOINT {name}"))
        .map_err(Into::into))?;
    match work() {
        Ok(()) => sql(req
            .conn
            .execute_batch(&format!("RELEASE {name}"))
            .map_err(Into::into)),
        Err(_) => sql(req
            .conn
            .execute_batch(&format!("ROLLBACK TO {name}; RELEASE {name}"))
            .map_err(Into::into)),
    }
}

fn key_prefix_len(title: &str) -> usize {
    let t = title.trim_start();
    let b = t.as_bytes();
    if !b.first().is_some_and(u8::is_ascii_alphabetic) {
        return 0;
    }
    let Some(dash) = t.find('-') else { return 0 };
    if !t[..dash].bytes().all(|c| c.is_ascii_alphanumeric()) {
        return 0;
    }
    let digits = t[dash + 1..].bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return 0;
    }
    let rest = &t[dash + 1 + digits..];
    let after_space = rest.trim_start();
    let Some(after_colon) = after_space.strip_prefix(':') else {
        return 0;
    };
    let tail = after_colon.trim_start();
    title.len() - tail.len()
}

fn promise_line(row: &Row) -> String {
    let id = col(row, "id").as_i64().unwrap_or(0);
    let raw = match col(row, "title") {
        Value::Null => String::new(),
        v => js::string(v),
    };
    let key = task_key(
        &raw,
        col(row, "code").as_str(),
        col(row, "seq").as_i64(),
        id,
    );
    let stripped = &raw[key_prefix_len(&raw)..];
    let title = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
    let title = title.replace('<', "&lt;").trim().to_string();
    let mut line = format!("- [{key}](tmgr://task/{id}) — {title}");
    if js::truthy(col(row, "expired_at")) {
        let due: String = js::string(col(row, "expired_at"))
            .chars()
            .take(10)
            .collect();
        line.push_str(&format!(" · due {due}"));
    }
    if let Some(status) = col(row, "status_name")
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        line.push_str(&format!(" · {status}"));
    }
    line
}

/// Rewrites the system Promises section of a person page as the system, on behalf of whoever caused it.
fn rebuild_promises(req: &Request<'_>, page_id: i64) -> Result<(), Outcome> {
    let rows = sql(select(
        req.conn,
        "SELECT t.id, t.title, t.expired_at, t.category_tasks_sequence_id AS seq, c.code, s.name AS status_name
		 FROM task_page_mentions m
		 JOIN tasks t ON t.id = m.task_id AND t.deleted_at IS NULL
		 LEFT JOIN categories c ON c.id = t.project_category_id
		 LEFT JOIN statuses s ON s.id = t.status_id
		 WHERE m.page_id = ? AND (s.type IS NULL OR s.type NOT IN ('completed', 'archived'))
		 ORDER BY t.expired_at IS NULL, t.expired_at, t.id",
        &[int(page_id)],
    ))?;
    let markdown = rows.iter().map(promise_line).collect::<Vec<_>>().join("\n");
    best_effort(req, "promises", || {
        write(req, &Who::system(), page_id, |row| {
            let current = s(row, "body");
            let body = match find_section(&current, PROMISES_SECTION) {
                Some(section) => replace_section(&current, &section, &markdown),
                None => current,
            };
            Ok(Change {
                title: s(row, "title"),
                body,
                properties: s(row, "properties"),
                summary: Some("Promises updated".into()),
                new_section: None,
            })
        })
        .map(|_| ())
    })
}

fn mentioned_ids(texts: &[String]) -> Vec<i64> {
    let mut ids: Vec<i64> = Vec::new();
    for t in texts {
        let mut rest = t.as_str();
        while let Some(at) = rest.find("tmgr:") {
            rest = &rest[at + 5..];
            if ids.len() >= 100 {
                break;
            }
            let mut probe = rest;
            let mut ok = true;
            for part in ["/", "/", "page", "/"] {
                probe = probe
                    .strip_prefix('\\')
                    .filter(|_| part == "/")
                    .unwrap_or(probe);
                match probe.strip_prefix(part) {
                    Some(p) => probe = p,
                    None => {
                        ok = false;
                        break;
                    }
                }
            }
            let digits: String = probe.chars().take_while(char::is_ascii_digit).collect();
            if ok && !digits.is_empty() {
                let id = digits.parse::<f64>().unwrap_or(f64::NAN) as i64;
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
    }
    ids
}

/// Re-reads a task and its comments, updates task_page_mentions and rebuilds the promises of the person pages it touches.
pub(crate) fn sync_task_mentions(req: &Request<'_>, task_id: i64) -> Result<(), Outcome> {
    best_effort(req, "mentions", || {
        let Some(task) = sql(select_one(
            req.conn,
            "SELECT description, description_json, deleted_at FROM tasks WHERE id = ?",
            &[int(task_id)],
        ))?
        else {
            return Ok(());
        };
        let stored: Vec<i64> = sql(select(
            req.conn,
            "SELECT page_id FROM task_page_mentions WHERE task_id = ?",
            &[int(task_id)],
        ))?
        .iter()
        .filter_map(|r| col(r, "page_id").as_i64())
        .collect();
        let mut wanted: Vec<i64> = Vec::new();
        if col(&task, "deleted_at").is_null() {
            let mut texts: Vec<String> = ["description", "description_json"]
                .iter()
                .filter_map(|k| col(&task, k).as_str().map(str::to_string))
                .collect();
            for c in sql(select(
                req.conn,
                "SELECT message FROM comments WHERE task_id = ? AND deleted_at IS NULL",
                &[int(task_id)],
            ))? {
                texts.extend(col(&c, "message").as_str().map(str::to_string));
            }
            let candidates = mentioned_ids(&texts);
            if !candidates.is_empty() {
                let marks = vec!["?"; candidates.len()].join(",");
                let params: Vec<Sql> = candidates.iter().map(|i| int(*i)).collect();
                wanted = sql(select(
                    req.conn,
                    &format!("SELECT id FROM pages WHERE deleted_at IS NULL AND id IN ({marks})"),
                    &params,
                ))?
                .iter()
                .filter_map(|r| col(r, "id").as_i64())
                .collect();
            }
        }
        if stored.is_empty() && wanted.is_empty() {
            return Ok(());
        }
        for id in stored.iter().filter(|id| !wanted.contains(id)) {
            sql(execute(
                req.conn,
                "DELETE FROM task_page_mentions WHERE task_id = ? AND page_id = ?",
                &[int(task_id), int(*id)],
            ))?;
        }
        for id in wanted.iter().filter(|id| !stored.contains(id)) {
            sql(execute(
                req.conn,
                "INSERT OR IGNORE INTO task_page_mentions (task_id, page_id) VALUES (?, ?)",
                &[int(task_id), int(*id)],
            ))?;
        }
        let mut affected: Vec<i64> = Vec::new();
        let mut seen = HashSet::new();
        for id in stored.iter().chain(&wanted) {
            if seen.insert(*id) {
                affected.push(*id);
            }
        }
        let marks = vec!["?"; affected.len()].join(",");
        let params: Vec<Sql> = affected.iter().map(|i| int(*i)).collect();
        let people = sql(select(
            req.conn,
            &format!("SELECT id FROM pages WHERE deleted_at IS NULL AND type = 'person' AND id IN ({marks})"),
            &params,
        ))?;
        for person in people {
            rebuild_promises(req, col(&person, "id").as_i64().unwrap_or(0))?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_writes_keep_the_search_index_consistent() {
        let conn = crate::testing::db("");
        conn.execute_batch("PRAGMA query_only = OFF").unwrap();
        let c = crate::testing::ctx("2026-10-13T12:00:00.000Z");
        let post = |method: &str, url: &str, body: Value| {
            crate::dispatch(&conn, &c, method, url, Some(&body))
                .unwrap()
                .unwrap()
        };
        assert_eq!(
            post(
                "POST",
                "pages",
                json!({ "title": "Alpha", "body": "first zebra" })
            )
            .status,
            201
        );
        post(
            "PATCH",
            "pages/1",
            json!({ "version": 1, "body": "second giraffe" }),
        );
        post(
            "POST",
            "pages/1/append",
            json!({ "markdown": "third okapi" }),
        );
        post("DELETE", "pages/1", json!({}));
        post("POST", "pages/1/restore", json!({}));
        conn.execute_batch("INSERT INTO pages_fts(pages_fts) VALUES ('integrity-check')")
            .unwrap();
        let hits = |term: &str| -> i64 {
            conn.query_row(
                "SELECT COUNT(*) FROM pages_fts WHERE pages_fts MATCH ?",
                [term],
                |r| r.get(0),
            )
            .unwrap()
        };
        assert_eq!(hits("zebra"), 0);
        assert_eq!(hits("giraffe"), 1);
        assert_eq!(hits("okapi"), 1);
    }

    #[test]
    fn slugs_follow_the_ts_rules() {
        assert_eq!(slugify("Plain note"), "plain-note");
        assert_eq!(slugify("Привет мир"), "privet-mir");
        assert_eq!(slugify("2026"), "p-2026");
        assert_eq!(slugify("Tree"), "p-tree");
        assert_eq!(slugify("!!!"), "page");
        assert!(slugify(&"a".repeat(100)).len() == 75);
    }

    #[test]
    fn page_links_in_mentions_may_be_json_escaped() {
        let texts = [
            "see tmgr://page/5 and tmgr:\\/\\/page\\/6".to_string(),
            "tmgr://task/7".into(),
        ];
        assert_eq!(mentioned_ids(&texts), [5, 6]);
    }

    #[test]
    fn promise_lines_strip_the_key_prefix() {
        let row: Row = serde_json::from_value(json!({
            "id": 3, "title": "TM-4:  Call  <Bob>", "code": null, "seq": null,
            "expired_at": "2026-10-20T00:00:00Z", "status_name": " Backlog "
        }))
        .unwrap();
        assert_eq!(
            promise_line(&row),
            "- [TM-4](tmgr://task/3) — Call &lt;Bob> · due 2026-10-20 · Backlog"
        );
    }
}
