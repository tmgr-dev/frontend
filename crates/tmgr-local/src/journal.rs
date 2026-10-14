//! The hash-chained edit journal (`edit_journal`, schema 13): who wrote a task's title, description and
//! comments, so queue pickup and bypass can tell the owner's own text from an agent's.
//!
//! Row hash: SHA-256 (lowercase hex) of the UTF-8 JSON array
//! `[seq, at, entity, entity_id, task_id, field, author_kind, source, actor_id, content_sha256, prev_hash]`
//! (no whitespace; `seq` is a number, the rest strings). Genesis `prev_hash` is 64 zeros.

use std::io::IsTerminal;

use rusqlite::types::Value as Sql;
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::context::Context;
use crate::db::{execute, schema_version, select};
use crate::error::Result;

pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
pub const JOURNAL_SCHEMA: u32 = 13;

/// Environment variables that mark an agent session. A trailing `*` matches any name with that prefix.
pub const AGENT_SESSION_MARKERS: [&str; 6] = [
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CODEX_SANDBOX*",
    "TMGR_AGENT_SESSION",
    "TMGR_RUN_TOKEN",
    "TMGR_RUN_ID",
];

/// Where a request without an actor comes from. Everything but a terminal session is never human.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WriteOrigin {
    #[default]
    Cli,
    CliTty,
    DesktopUi,
}

impl WriteOrigin {
    /// `CliTty` only when stdin and stdout are terminals and no agent-session marker is set.
    pub fn from_session<I, S>(stdin_tty: bool, stdout_tty: bool, vars: I) -> WriteOrigin
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let in_agent = vars.into_iter().any(|name| {
            AGENT_SESSION_MARKERS.iter().any(|marker| match marker.strip_suffix('*') {
                Some(prefix) => name.as_ref().starts_with(prefix),
                None => name.as_ref() == *marker,
            })
        });
        if stdin_tty && stdout_tty && !in_agent {
            WriteOrigin::CliTty
        } else {
            WriteOrigin::Cli
        }
    }

    pub fn detect() -> WriteOrigin {
        let names: Vec<String> = std::env::vars_os()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        WriteOrigin::from_session(
            std::io::stdin().is_terminal(),
            std::io::stdout().is_terminal(),
            names,
        )
    }
}

struct Author {
    kind: &'static str,
    source: &'static str,
    actor_id: String,
}

fn author_of(ctx: &Context) -> Author {
    let id = || ctx.user.id.to_string();
    match &ctx.actor {
        None => match ctx.origin {
            WriteOrigin::DesktopUi => Author { kind: "human", source: "desktop_ui", actor_id: id() },
            WriteOrigin::CliTty => Author { kind: "human", source: "cli_tty", actor_id: id() },
            WriteOrigin::Cli => Author { kind: "unattended", source: "cli", actor_id: id() },
        },
        Some(actor) => match actor.kind.as_str() {
            "persona" => Author { kind: "agent", source: "local_access", actor_id: actor.id.clone() },
            "agent" => Author { kind: "agent", source: "runner", actor_id: actor.id.clone() },
            "companion" => Author { kind: "agent", source: "local_access", actor_id: actor.id.clone() },
            "plugin" => Author { kind: "plugin", source: "plugin", actor_id: actor.owner().to_string() },
            _ => Author { kind: "unattended", source: "import", actor_id: actor.id.clone() },
        },
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 of the text with `\r\n` folded to `\n`; `None` counts as the empty string.
pub fn content_sha(value: Option<&str>) -> String {
    sha256_hex(value.unwrap_or("").replace("\r\n", "\n").as_bytes())
}

/// The comments digest: SHA-256 of the JSON array `[[comment_id, content_sha256], ...]` ordered by id.
pub fn comments_digest(comments: &[(i64, String)]) -> String {
    let mut sorted: Vec<&(i64, String)> = comments.iter().collect();
    sorted.sort_by_key(|(id, _)| *id);
    let items: Vec<Value> = sorted.iter().map(|(id, sha)| json!([id, sha])).collect();
    sha256_hex(Value::Array(items).to_string().as_bytes())
}

#[derive(Clone, Debug)]
pub struct JournalRow {
    pub seq: i64,
    pub at: String,
    pub entity: String,
    pub entity_id: String,
    pub task_id: String,
    pub field: String,
    pub author_kind: String,
    pub source: String,
    pub actor_id: String,
    pub content_sha256: String,
    pub prev_hash: String,
    pub hash: String,
}

pub fn row_hash(row: &JournalRow) -> String {
    let encoded = json!([
        row.seq,
        row.at,
        row.entity,
        row.entity_id,
        row.task_id,
        row.field,
        row.author_kind,
        row.source,
        row.actor_id,
        row.content_sha256,
        row.prev_hash
    ]);
    sha256_hex(encoded.to_string().as_bytes())
}

pub(crate) struct Entry {
    pub entity: &'static str,
    pub entity_id: i64,
    pub task_id: i64,
    pub field: &'static str,
    pub content: Option<String>,
}

impl Entry {
    pub fn task(task_id: i64, field: &'static str, content: Option<String>) -> Entry {
        Entry { entity: "task", entity_id: task_id, task_id, field, content }
    }

    pub fn comment(comment_id: i64, task_id: i64, field: &'static str, content: Option<String>) -> Entry {
        Entry { entity: "comment", entity_id: comment_id, task_id, field, content }
    }
}

pub(crate) fn enabled(conn: &Connection) -> Result<bool> {
    Ok(schema_version(conn)? >= JOURNAL_SCHEMA)
}

/// Appends `entries` to the chain in the caller's transaction; a schema-12 database has no journal and gets nothing.
pub(crate) fn append(conn: &Connection, ctx: &Context, entries: &[Entry]) -> Result<()> {
    if entries.is_empty() || !enabled(conn)? {
        return Ok(());
    }
    let author = author_of(ctx);
    let tail = select(conn, "SELECT seq, hash FROM edit_journal ORDER BY seq DESC LIMIT 1", &[])?;
    let (mut seq, mut prev) = match tail.first() {
        Some(row) => (
            row["seq"].as_i64().unwrap_or(0),
            row["hash"].as_str().unwrap_or(GENESIS_HASH).to_string(),
        ),
        None => (0, GENESIS_HASH.to_string()),
    };
    for entry in entries {
        seq += 1;
        let mut row = JournalRow {
            seq,
            at: ctx.iso_now(),
            entity: entry.entity.into(),
            entity_id: entry.entity_id.to_string(),
            task_id: entry.task_id.to_string(),
            field: entry.field.into(),
            author_kind: author.kind.into(),
            source: author.source.into(),
            actor_id: author.actor_id.clone(),
            content_sha256: entry.content.as_deref().map_or(String::new(), |c| content_sha(Some(c))),
            prev_hash: prev.clone(),
            hash: String::new(),
        };
        row.hash = row_hash(&row);
        execute(
            conn,
            "INSERT INTO edit_journal (seq, at, entity, entity_id, task_id, field, author_kind, source, actor_id, content_sha256, prev_hash, hash)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            &[
                Sql::Integer(row.seq),
                Sql::Text(row.at.clone()),
                Sql::Text(row.entity.clone()),
                Sql::Text(row.entity_id.clone()),
                Sql::Text(row.task_id.clone()),
                Sql::Text(row.field.clone()),
                Sql::Text(row.author_kind.clone()),
                Sql::Text(row.source.clone()),
                Sql::Text(row.actor_id.clone()),
                Sql::Text(row.content_sha256.clone()),
                Sql::Text(row.prev_hash.clone()),
                Sql::Text(row.hash.clone()),
            ],
        )?;
        prev = row.hash;
    }
    Ok(())
}

fn text(row: &crate::db::Row, key: &str) -> String {
    match row.get(key) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

fn to_row(row: &crate::db::Row) -> JournalRow {
    JournalRow {
        seq: row.get("seq").and_then(Value::as_i64).unwrap_or(-1),
        at: text(row, "at"),
        entity: text(row, "entity"),
        entity_id: text(row, "entity_id"),
        task_id: text(row, "task_id"),
        field: text(row, "field"),
        author_kind: text(row, "author_kind"),
        source: text(row, "source"),
        actor_id: text(row, "actor_id"),
        content_sha256: text(row, "content_sha256"),
        prev_hash: text(row, "prev_hash"),
        hash: text(row, "hash"),
    }
}

const COLUMNS: &str =
    "seq, at, entity, entity_id, task_id, field, author_kind, source, actor_id, content_sha256, prev_hash, hash";
const PAGE: i64 = 500;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChainStatus {
    pub ok: bool,
    pub broken_at: Option<i64>,
    pub rows: i64,
}

/// Walks the chain in pages and reports the first row that does not follow from the one before it.
/// A database without the journal has nothing to verify and is reported broken at 0.
pub fn verify_chain(conn: &Connection) -> Result<ChainStatus> {
    if !enabled(conn)? {
        return Ok(ChainStatus { ok: false, broken_at: Some(0), rows: 0 });
    }
    let (mut prev_hash, mut prev_seq, mut rows) = (GENESIS_HASH.to_string(), 0i64, 0i64);
    loop {
        let page = select(
            conn,
            &format!("SELECT {COLUMNS} FROM edit_journal WHERE seq > ? ORDER BY seq ASC LIMIT {PAGE}"),
            &[Sql::Integer(prev_seq)],
        )?;
        if page.is_empty() {
            return Ok(ChainStatus { ok: true, broken_at: None, rows });
        }
        for raw in &page {
            let row = to_row(raw);
            if row.seq != prev_seq + 1 || row.prev_hash != prev_hash || row_hash(&row) != row.hash {
                return Ok(ChainStatus { ok: false, broken_at: Some(row.seq), rows });
            }
            prev_hash = row.hash;
            prev_seq = row.seq;
            rows += 1;
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HumanContent {
    pub title_sha: String,
    pub description_sha: String,
    pub comments_sha: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Provenance {
    /// `human`, `agent`, `plugin`, `unattended` or `unrecorded`.
    pub created_by: String,
    pub last_human_seq: Option<i64>,
    pub last_human_content: Option<HumanContent>,
    pub modified_by_non_human_since: bool,
    pub chain_ok: bool,
    pub content_matches: bool,
}

impl Provenance {
    fn unrecorded(chain_ok: bool) -> Provenance {
        Provenance {
            created_by: "unrecorded".into(),
            last_human_seq: None,
            last_human_content: None,
            modified_by_non_human_since: false,
            chain_ok,
            content_matches: false,
        }
    }

    /// Created by the owner, untouched by anything else since, the chain verifies and the content is
    /// what the owner last wrote.
    pub fn is_human_written(&self) -> bool {
        self.created_by == "human"
            && !self.modified_by_non_human_since
            && self.chain_ok
            && self.content_matches
    }
}

/// A task's text as a consumer saw it (for example a run bundle built from API answers), to be compared
/// with the last human content as well as the database.
pub struct Observed<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub comments: Vec<(i64, &'a str)>,
}

fn content_of(title: &str, description: Option<&str>, comments: &[(i64, String)]) -> HumanContent {
    HumanContent {
        title_sha: content_sha(Some(title)),
        description_sha: content_sha(description),
        comments_sha: comments_digest(comments),
    }
}

fn current_content(conn: &Connection, task_id: i64) -> Result<Option<HumanContent>> {
    let task = select(
        conn,
        "SELECT title, description FROM tasks WHERE id = ? AND deleted_at IS NULL",
        &[Sql::Integer(task_id)],
    )?;
    let Some(task) = task.first() else {
        return Ok(None);
    };
    let comments: Vec<(i64, String)> = select(
        conn,
        "SELECT id, message FROM comments WHERE task_id = ? AND deleted_at IS NULL ORDER BY id",
        &[Sql::Integer(task_id)],
    )?
    .iter()
    .map(|c| (c["id"].as_i64().unwrap_or(0), content_sha(Some(&text(c, "message")))))
    .collect();
    Ok(Some(content_of(&text(task, "title"), Some(&text(task, "description")), &comments)))
}

pub fn provenance(conn: &Connection, task_id: i64) -> Result<Provenance> {
    provenance_inner(conn, task_id, None)
}

/// `provenance` that also requires `observed` to be the last human content.
pub fn provenance_observed(conn: &Connection, task_id: i64, observed: &Observed<'_>) -> Result<Provenance> {
    provenance_inner(conn, task_id, Some(observed))
}

fn provenance_inner(conn: &Connection, task_id: i64, observed: Option<&Observed<'_>>) -> Result<Provenance> {
    let chain = verify_chain(conn)?;
    if !enabled(conn)? {
        return Ok(Provenance::unrecorded(false));
    }
    let rows: Vec<JournalRow> = select(
        conn,
        &format!("SELECT {COLUMNS} FROM edit_journal WHERE task_id = ? ORDER BY seq ASC"),
        &[Sql::Text(task_id.to_string())],
    )?
    .iter()
    .map(to_row)
    .collect();
    if rows.is_empty() {
        return Ok(Provenance::unrecorded(chain.ok));
    }
    let created_by = rows
        .iter()
        .find(|r| r.entity == "task" && r.field == "create")
        .map_or("unrecorded".to_string(), |r| r.author_kind.clone());
    let last_human = rows.iter().filter(|r| r.author_kind == "human").map(|r| r.seq).max();
    let modified = rows
        .iter()
        .any(|r| r.author_kind != "human" && last_human.is_none_or(|h| r.seq > h));
    let human_content = last_human.map(|limit| {
        let mut title = String::new();
        let mut description = content_sha(None);
        let mut comments: Vec<(i64, String)> = Vec::new();
        for row in rows.iter().take_while(|r| r.seq <= limit) {
            match (row.entity.as_str(), row.field.as_str()) {
                ("task", "create" | "title") => title = row.content_sha256.clone(),
                ("task", "description") => description = row.content_sha256.clone(),
                ("comment", "comment") => {
                    let id = row.entity_id.parse().unwrap_or(-1);
                    comments.retain(|(c, _)| *c != id);
                    comments.push((id, row.content_sha256.clone()));
                }
                ("comment", "delete") => {
                    let id = row.entity_id.parse().unwrap_or(-1);
                    comments.retain(|(c, _)| *c != id);
                }
                _ => {}
            }
        }
        HumanContent {
            title_sha: title,
            description_sha: description,
            comments_sha: comments_digest(&comments),
        }
    });
    let mut matches = match (&human_content, current_content(conn, task_id)?) {
        (Some(human), Some(current)) => *human == current,
        _ => false,
    };
    if let (true, Some(observed), Some(human)) = (matches, observed, &human_content) {
        let comments: Vec<(i64, String)> = observed
            .comments
            .iter()
            .map(|(id, message)| (*id, content_sha(Some(message))))
            .collect();
        matches = *human == content_of(observed.title, Some(observed.description), &comments);
    }
    Ok(Provenance {
        created_by,
        last_human_seq: last_human,
        last_human_content: human_content,
        modified_by_non_human_since: modified,
        chain_ok: chain.ok,
        content_matches: matches,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Actor;
    use crate::testing::{ctx, db, db12};
    use crate::{dispatch, Response};

    const T: &str = "2026-10-14T10:00:00.000Z";

    fn writable() -> Connection {
        let conn = db("INSERT INTO personas (uuid, owner_user_id, owner_name, name, synced_at) VALUES ('p1', 1, 'Owner', 'Scribe', 'x');
            INSERT INTO workspace_personas (persona_uuid, permissions, enabled_at) VALUES ('p1', '[\"tasks:write\",\"comments:write\"]', 'x');");
        conn.execute_batch("PRAGMA query_only = OFF").unwrap();
        conn
    }

    fn call(conn: &Connection, c: &Context, method: &str, url: &str, body: Value) -> Response {
        dispatch(conn, c, method, url, Some(&body)).unwrap().unwrap()
    }

    fn origin(origin: WriteOrigin) -> Context {
        let mut c = ctx(T);
        c.origin = origin;
        c
    }

    fn persona() -> Context {
        let mut c = ctx(T);
        c.actor = Some(Actor {
            kind: "persona".into(),
            id: "p1".into(),
            name: "Scribe".into(),
            owner_id: None,
        });
        c
    }

    fn rows(conn: &Connection) -> Vec<(String, String)> {
        select(conn, "SELECT author_kind, source FROM edit_journal ORDER BY seq", &[])
            .unwrap()
            .iter()
            .map(|r| (text(r, "author_kind"), text(r, "source")))
            .collect()
    }

    fn vars(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn only_a_clean_terminal_session_is_cli_tty() {
        let none = vars(&["PATH", "HOME"]);
        assert_eq!(WriteOrigin::from_session(true, true, none.clone()), WriteOrigin::CliTty);
        assert_eq!(WriteOrigin::from_session(false, true, none.clone()), WriteOrigin::Cli);
        assert_eq!(WriteOrigin::from_session(true, false, none.clone()), WriteOrigin::Cli);
        for marker in [
            "CLAUDECODE",
            "CLAUDE_CODE_ENTRYPOINT",
            "CODEX_SANDBOX",
            "CODEX_SANDBOX_NETWORK_DISABLED",
            "TMGR_AGENT_SESSION",
            "TMGR_RUN_TOKEN",
            "TMGR_RUN_ID",
        ] {
            assert_eq!(
                WriteOrigin::from_session(true, true, vars(&["PATH", marker])),
                WriteOrigin::Cli,
                "{marker}"
            );
        }
        assert_eq!(
            WriteOrigin::from_session(true, true, vars(&["CLAUDECODE_NOT", "MY_CODEX_SANDBOX"])),
            WriteOrigin::CliTty
        );
    }

    #[test]
    fn the_origin_decides_what_a_user_write_is() {
        let conn = writable();
        for (o, expected) in [
            (WriteOrigin::DesktopUi, ("human", "desktop_ui")),
            (WriteOrigin::CliTty, ("human", "cli_tty")),
            (WriteOrigin::Cli, ("unattended", "cli")),
        ] {
            call(&conn, &origin(o), "POST", "tasks", json!({ "title": "t" }));
            let last = rows(&conn).pop().unwrap();
            assert_eq!(last, (expected.0.to_string(), expected.1.to_string()));
        }
    }

    #[test]
    fn the_default_origin_is_never_human() {
        assert_eq!(WriteOrigin::default(), WriteOrigin::Cli);
        let conn = writable();
        call(&conn, &ctx(T), "POST", "tasks", json!({ "title": "t" }));
        assert_eq!(rows(&conn), vec![("unattended".to_string(), "cli".to_string())]);
        assert!(!provenance(&conn, 1).unwrap().is_human_written());
    }

    #[test]
    fn a_persona_is_an_agent_whatever_the_body_and_origin_claim() {
        let conn = writable();
        let mut c = persona();
        c.origin = WriteOrigin::CliTty;
        let forged = json!({
            "title": "Forged", "author_kind": "human", "source": "cli_tty", "actor_id": "1",
            "actor": { "kind": "user", "id": "1" }, "origin": "cli_tty", "seq": 1, "hash": "x"
        });
        assert_eq!(call(&conn, &c, "POST", "tasks", forged.clone()).status, 201);
        let mut comment = forged.clone();
        comment["message"] = json!("note");
        assert_eq!(call(&conn, &c, "POST", "tasks/1/comments", comment).status, 201);
        let mut edit = forged;
        edit["title"] = json!("Retitled");
        assert_eq!(call(&conn, &c, "PATCH", "tasks/1", edit).status, 200);
        let all = select(&conn, "SELECT author_kind, source, actor_id FROM edit_journal", &[]).unwrap();
        assert_eq!(all.len(), 3);
        for r in &all {
            assert_eq!(
                (text(r, "author_kind"), text(r, "source"), text(r, "actor_id")),
                ("agent".to_string(), "local_access".to_string(), "p1".to_string())
            );
        }
        assert!(!provenance(&conn, 1).unwrap().is_human_written());
    }

    #[test]
    fn a_plugin_and_unknown_actors_are_never_human() {
        let conn = writable();
        for (kind, expected) in [("plugin", "plugin"), ("system", "unattended"), ("whatever", "unattended"), ("user", "unattended")] {
            let mut c = ctx(T);
            c.origin = WriteOrigin::DesktopUi;
            c.actor = Some(Actor { kind: kind.into(), id: "x".into(), name: "x".into(), owner_id: None });
            call(&conn, &c, "POST", "tasks", json!({ "title": "t" }));
            assert_eq!(rows(&conn).pop().unwrap().0, expected, "{kind}");
        }
    }

    #[test]
    fn a_schema_12_database_gets_no_journal_and_answers_unrecorded() {
        let conn = db12("");
        conn.execute_batch("PRAGMA query_only = OFF").unwrap();
        let r = call(&conn, &origin(WriteOrigin::CliTty), "POST", "tasks", json!({ "title": "t" }));
        assert_eq!(r.status, 201);
        let tables: i64 = conn
            .query_row("SELECT COUNT(*) FROM sqlite_master WHERE name = 'edit_journal'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(tables, 0);
        let p = provenance(&conn, 1).unwrap();
        assert_eq!(p.created_by, "unrecorded");
        assert!(!p.chain_ok);
        assert!(!p.is_human_written());
        assert!(!verify_chain(&conn).unwrap().ok);
    }

    #[test]
    fn a_failed_request_leaves_no_row() {
        let conn = writable();
        let c = origin(WriteOrigin::CliTty);
        assert_eq!(call(&conn, &c, "PATCH", "tasks/9", json!({ "title": "x" })).status, 404);
        assert_eq!(call(&conn, &c, "POST", "tasks", json!({ "title": "" })).status, 422);
        let body = json!({ "title": "late", "persona_assignees": ["nope"] });
        assert_eq!(call(&conn, &c, "POST", "tasks", body).status, 422);
        assert!(rows(&conn).is_empty());
    }

    #[test]
    fn a_tampered_chain_is_found_for_an_edit_a_delete_and_a_reorder() {
        let make = || {
            let conn = writable();
            let c = origin(WriteOrigin::CliTty);
            call(&conn, &c, "POST", "tasks", json!({ "title": "T", "description": "D" }));
            call(&conn, &c, "PATCH", "tasks/1", json!({ "title": "T2" }));
            call(&conn, &c, "POST", "tasks/1/comments", json!({ "message": "hi" }));
            conn
        };
        let intact = make();
        assert_eq!(verify_chain(&intact).unwrap(), ChainStatus { ok: true, broken_at: None, rows: 4 });
        assert!(provenance(&intact, 1).unwrap().is_human_written());

        let edited = make();
        edited.execute_batch("UPDATE edit_journal SET author_kind = 'agent' WHERE seq = 3").unwrap();
        assert_eq!(verify_chain(&edited).unwrap().broken_at, Some(3));
        assert!(!provenance(&edited, 1).unwrap().chain_ok);

        let deleted = make();
        deleted.execute_batch("DELETE FROM edit_journal WHERE seq = 2").unwrap();
        assert_eq!(verify_chain(&deleted).unwrap().broken_at, Some(3));

        let reordered = make();
        reordered
            .execute_batch(
                "UPDATE edit_journal SET seq = 99 WHERE seq = 2; UPDATE edit_journal SET seq = 2 WHERE seq = 3; UPDATE edit_journal SET seq = 3 WHERE seq = 99;",
            )
            .unwrap();
        assert!(!verify_chain(&reordered).unwrap().ok);
        assert!(!provenance(&reordered, 1).unwrap().is_human_written());
    }

    #[test]
    fn an_edit_outside_the_journal_changes_the_content_and_fails_closed() {
        let conn = writable();
        call(&conn, &origin(WriteOrigin::CliTty), "POST", "tasks", json!({ "title": "T" }));
        assert!(provenance(&conn, 1).unwrap().is_human_written());
        conn.execute_batch("UPDATE tasks SET description = 'sneaky' WHERE id = 1").unwrap();
        let p = provenance(&conn, 1).unwrap();
        assert!(!p.content_matches && !p.is_human_written());
    }

    #[test]
    fn what_a_consumer_saw_must_be_the_last_human_content() {
        let conn = writable();
        let c = origin(WriteOrigin::CliTty);
        call(&conn, &c, "POST", "tasks", json!({ "title": "T", "description": "D\r\nE" }));
        call(&conn, &c, "POST", "tasks/1/comments", json!({ "message": "note" }));
        let same = Observed { title: "T", description: "D\nE", comments: vec![(1, "note")] };
        assert!(provenance_observed(&conn, 1, &same).unwrap().is_human_written());
        let other = Observed { title: "T", description: "changed", comments: vec![(1, "note")] };
        assert!(!provenance_observed(&conn, 1, &other).unwrap().is_human_written());
        let extra = Observed { title: "T", description: "D\nE", comments: vec![(1, "note"), (2, "more")] };
        assert!(!provenance_observed(&conn, 1, &extra).unwrap().is_human_written());
    }

    #[test]
    fn two_writers_on_one_file_extend_one_chain() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.db");
        Connection::open(&path).unwrap().execute_batch(&crate::testing::schema()).unwrap();
        let desktop = crate::open_write(&path).unwrap();
        let cli = crate::open_write(&path).unwrap();
        for i in 0..4 {
            let (conn, o) = if i % 2 == 0 { (&desktop, WriteOrigin::DesktopUi) } else { (&cli, WriteOrigin::CliTty) };
            let r = call(conn, &origin(o), "POST", "tasks", json!({ "title": format!("t{i}") }));
            assert_eq!(r.status, 201);
        }
        assert_eq!(verify_chain(&desktop).unwrap(), ChainStatus { ok: true, broken_at: None, rows: 4 });
        assert_eq!(rows(&cli).len(), 4);
    }

    #[test]
    fn the_hash_encoding_is_the_documented_one() {
        let row = JournalRow {
            seq: 1,
            at: "2026-10-14T10:00:00.000Z".into(),
            entity: "task".into(),
            entity_id: "1".into(),
            task_id: "1".into(),
            field: "create".into(),
            author_kind: "human".into(),
            source: "desktop_ui".into(),
            actor_id: "7".into(),
            content_sha256: content_sha(Some("T")),
            prev_hash: GENESIS_HASH.into(),
            hash: String::new(),
        };
        let encoded = format!(
            "[1,\"2026-10-14T10:00:00.000Z\",\"task\",\"1\",\"1\",\"create\",\"human\",\"desktop_ui\",\"7\",\"{}\",\"{}\"]",
            content_sha(Some("T")),
            GENESIS_HASH
        );
        assert_eq!(row_hash(&row), sha256_hex(encoded.as_bytes()));
        assert_eq!(content_sha(None), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(content_sha(Some("a\r\nb")), content_sha(Some("a\nb")));
    }
}
