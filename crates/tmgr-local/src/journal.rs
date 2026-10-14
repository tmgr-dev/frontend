//! The hash-chained edit journal (`edit_journal`, schema 13): who wrote a task's title, description and
//! comments, so queue pickup and bypass can tell the owner's own text from an agent's.
//!
//! Row hash: SHA-256 (lowercase hex) of the UTF-8 JSON array
//! `[seq, at, entity, entity_id, task_id, field, author_kind, source, actor_id, content_sha256, prev_hash]`
//! (no whitespace; `seq` is a number, the rest strings). Genesis `prev_hash` is 64 zeros.
//!
//! Only the desktop UI (no actor) and an explicit `confirm` write `human` rows. Every CLI write is `unattended`.

use chrono::{DateTime, Utc};
use rusqlite::types::Value as Sql;
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::context::Context;
use crate::db::{execute, schema_version, select};
use crate::error::{Error, Result};

pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
pub const JOURNAL_SCHEMA: u32 = 13;
pub const CHECKPOINT_KEY: &str = "edit_journal_verified";
/// A walk from genesis is repeated at least this often, whatever the checkpoint says.
pub const FULL_VERIFY_MAX_AGE_SECS: i64 = 3600;

/// Environment variables that mark an agent session. A trailing `*` matches any name with that prefix.
pub const AGENT_SESSION_MARKERS: [&str; 6] = [
    "CLAUDECODE",
    "CLAUDE_CODE_ENTRYPOINT",
    "CODEX_SANDBOX*",
    "TMGR_AGENT_SESSION",
    "TMGR_RUN_TOKEN",
    "TMGR_RUN_ID",
];

/// True when any of the variable names is an agent-session marker.
pub fn in_agent_session<I, S>(vars: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    vars.into_iter().any(|name| {
        AGENT_SESSION_MARKERS
            .iter()
            .any(|marker| match marker.strip_suffix('*') {
                Some(prefix) => name.as_ref().starts_with(prefix),
                None => name.as_ref() == *marker,
            })
    })
}

pub fn agent_session_detected() -> bool {
    in_agent_session(
        std::env::vars_os()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect::<Vec<_>>(),
    )
}

/// Where a request without an actor comes from. Only the desktop UI is the owner typing; the CLI never is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WriteOrigin {
    #[default]
    Cli,
    DesktopUi,
}

#[derive(Clone)]
struct Author {
    kind: &'static str,
    source: &'static str,
    actor_id: String,
}

fn author_of(ctx: &Context) -> Author {
    let id = || ctx.user.id.to_string();
    match &ctx.actor {
        None => match ctx.origin {
            WriteOrigin::DesktopUi => Author {
                kind: "human",
                source: "desktop_ui",
                actor_id: id(),
            },
            WriteOrigin::Cli => Author {
                kind: "unattended",
                source: "cli",
                actor_id: id(),
            },
        },
        Some(actor) => match actor.kind.as_str() {
            "persona" => Author {
                kind: "agent",
                source: "local_access",
                actor_id: actor.id.clone(),
            },
            "agent" => Author {
                kind: "agent",
                source: "runner",
                actor_id: actor.id.clone(),
            },
            "companion" => Author {
                kind: "agent",
                source: "local_access",
                actor_id: actor.id.clone(),
            },
            "plugin" if actor.id == "runner" && actor.owner_id.is_none() => Author {
                kind: "agent",
                source: "runner",
                actor_id: "runner".into(),
            },
            "plugin" => Author {
                kind: "plugin",
                source: "plugin",
                actor_id: actor.owner().to_string(),
            },
            _ => Author {
                kind: "unattended",
                source: "import",
                actor_id: actor.id.clone(),
            },
        },
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
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
    entity: &'static str,
    entity_id: i64,
    task_id: i64,
    field: &'static str,
    content_sha: String,
}

impl Entry {
    pub fn task(task_id: i64, field: &'static str, content: Option<String>) -> Entry {
        Entry {
            entity: "task",
            entity_id: task_id,
            task_id,
            field,
            content_sha: content
                .as_deref()
                .map_or(String::new(), |c| content_sha(Some(c))),
        }
    }

    pub fn comment(
        comment_id: i64,
        task_id: i64,
        field: &'static str,
        content: Option<String>,
    ) -> Entry {
        Entry {
            entity: "comment",
            entity_id: comment_id,
            task_id,
            field,
            content_sha: content
                .as_deref()
                .map_or(String::new(), |c| content_sha(Some(c))),
        }
    }
}

pub(crate) fn enabled(conn: &Connection) -> Result<bool> {
    Ok(schema_version(conn)? >= JOURNAL_SCHEMA)
}

/// Appends `entries` to the chain in the caller's transaction; a schema-12 database has no journal and gets nothing.
pub(crate) fn append(conn: &Connection, ctx: &Context, entries: &[Entry]) -> Result<()> {
    append_as(conn, ctx, entries, &author_of(ctx))
}

fn append_as(conn: &Connection, ctx: &Context, entries: &[Entry], author: &Author) -> Result<()> {
    if entries.is_empty() || !enabled(conn)? {
        return Ok(());
    }
    let tail = select(
        conn,
        "SELECT seq, hash FROM edit_journal ORDER BY seq DESC LIMIT 1",
        &[],
    )?;
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
            content_sha256: entry.content_sha.clone(),
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

const COLUMNS: &str = "seq, at, entity, entity_id, task_id, field, author_kind, source, actor_id, content_sha256, prev_hash, hash";
const PAGE: i64 = 500;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ChainStatus {
    pub ok: bool,
    pub broken_at: Option<i64>,
    /// Rows walked by this verification (all of them for a full walk, those after the checkpoint otherwise).
    pub rows: i64,
    pub tail_seq: i64,
    pub tail_hash: String,
    /// The walk started at genesis (not at the checkpoint).
    pub full: bool,
}

struct Checkpoint {
    seq: i64,
    hash: String,
    full_at: DateTime<Utc>,
}

fn parse_checkpoint(conn: &Connection) -> Option<Checkpoint> {
    let rows = select(
        conn,
        "SELECT value FROM meta WHERE key = ?",
        &[Sql::Text(CHECKPOINT_KEY.into())],
    )
    .ok()?;
    let value = rows.first()?.get("value")?.as_str()?.to_string();
    let mut parts = value.splitn(3, ':');
    let seq = parts.next()?.parse().ok()?;
    let hash = parts.next()?.to_string();
    let full_at = DateTime::parse_from_rfc3339(parts.next()?)
        .ok()?
        .with_timezone(&Utc);
    Some(Checkpoint { seq, hash, full_at })
}

/// Verifies the chain from the stored checkpoint (a row already verified once), or from genesis when there
/// is none, the checkpointed row is gone or changed, or the last walk from genesis is older than an hour.
/// `chain_ok` therefore means: verified from genesis at the checkpoint's `full_at`, incrementally since.
/// The checkpoint is same-user-forgeable like the rest.
pub fn verify_chain(conn: &Connection) -> Result<ChainStatus> {
    verify_chain_at(conn, Utc::now())
}

pub fn verify_chain_at(conn: &Connection, now: DateTime<Utc>) -> Result<ChainStatus> {
    if !enabled(conn)? {
        return Ok(ChainStatus {
            ok: false,
            broken_at: Some(0),
            rows: 0,
            tail_seq: 0,
            tail_hash: GENESIS_HASH.into(),
            full: true,
        });
    }
    let start = parse_checkpoint(conn)
        .filter(|c| {
            let age = now.signed_duration_since(c.full_at);
            age >= chrono::Duration::zero()
                && age <= chrono::Duration::seconds(FULL_VERIFY_MAX_AGE_SECS)
        })
        .filter(|c| {
            select(
                conn,
                "SELECT hash FROM edit_journal WHERE seq = ?",
                &[Sql::Integer(c.seq)],
            )
            .ok()
            .and_then(|r| {
                r.first()
                    .and_then(|r| r["hash"].as_str().map(|h| h == c.hash))
            })
            .unwrap_or(false)
        })
        .map(|c| (c.seq, c.hash));
    let mut status = walk(conn, start.clone())?;
    status.full = start.is_none();
    Ok(status)
}

/// Verifies every row from genesis, ignoring the checkpoint.
pub fn verify_chain_full(conn: &Connection) -> Result<ChainStatus> {
    if !enabled(conn)? {
        return verify_chain(conn);
    }
    let mut status = walk(conn, None)?;
    status.full = true;
    Ok(status)
}

fn walk(conn: &Connection, start: Option<(i64, String)>) -> Result<ChainStatus> {
    let (mut prev_seq, mut prev_hash) = start.unwrap_or((0, GENESIS_HASH.to_string()));
    let mut rows = 0i64;
    loop {
        let page = select(
            conn,
            &format!(
                "SELECT {COLUMNS} FROM edit_journal WHERE seq > ? ORDER BY seq ASC LIMIT {PAGE}"
            ),
            &[Sql::Integer(prev_seq)],
        )?;
        if page.is_empty() {
            return Ok(ChainStatus {
                ok: true,
                broken_at: None,
                rows,
                tail_seq: prev_seq,
                tail_hash: prev_hash,
                full: false,
            });
        }
        for raw in &page {
            let row = to_row(raw);
            if row.seq != prev_seq + 1 || row.prev_hash != prev_hash || row_hash(&row) != row.hash {
                return Ok(ChainStatus {
                    ok: false,
                    broken_at: Some(row.seq),
                    rows,
                    tail_seq: prev_seq,
                    tail_hash: prev_hash,
                    full: false,
                });
            }
            prev_hash = row.hash;
            prev_seq = row.seq;
            rows += 1;
        }
    }
}

/// Stores the verified tail as the next starting point; a read-only handle simply cannot. A walk from
/// genesis renews `full_at`, an incremental one keeps it.
pub fn store_checkpoint(conn: &Connection, status: &ChainStatus) -> Result<()> {
    store_checkpoint_at(conn, status, Utc::now())
}

pub fn store_checkpoint_at(
    conn: &Connection,
    status: &ChainStatus,
    now: DateTime<Utc>,
) -> Result<()> {
    if !status.ok || status.tail_seq == 0 {
        return Ok(());
    }
    let existing = parse_checkpoint(conn);
    let full_at = match (&existing, status.full) {
        (_, true) => now,
        (Some(c), false) if c.seq >= status.tail_seq => return Ok(()),
        (Some(c), false) => c.full_at,
        (None, false) => return Ok(()),
    };
    execute(
        conn,
        "INSERT INTO meta (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        &[
            Sql::Text(CHECKPOINT_KEY.into()),
            Sql::Text(format!(
                "{}:{}:{}",
                status.tail_seq,
                status.tail_hash,
                full_at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
            )),
        ],
    )?;
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HumanContent {
    pub title_sha: String,
    pub description_sha: String,
    pub description_json_sha: String,
    pub checkpoints_sha: String,
    pub comments_sha: String,
}

impl HumanContent {
    /// The hash a `confirm` row carries: SHA-256 of the JSON array of the five parts.
    pub fn snapshot_sha(&self) -> String {
        sha256_hex(
            json!([
                self.title_sha,
                self.description_sha,
                self.description_json_sha,
                self.checkpoints_sha,
                self.comments_sha
            ])
            .to_string()
            .as_bytes(),
        )
    }
}

/// The task text that reaches an agent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub title: String,
    pub description: String,
    pub description_json: String,
    pub checkpoints: String,
    pub comments: Vec<(i64, String)>,
}

impl Snapshot {
    pub fn parts(&self) -> HumanContent {
        let comments: Vec<(i64, String)> = self
            .comments
            .iter()
            .map(|(id, m)| (*id, content_sha(Some(m))))
            .collect();
        HumanContent {
            title_sha: content_sha(Some(&self.title)),
            description_sha: content_sha(Some(&self.description)),
            description_json_sha: content_sha(Some(&self.description_json)),
            checkpoints_sha: content_sha(Some(&self.checkpoints)),
            comments_sha: comments_digest(&comments),
        }
    }

    pub fn hash(&self) -> String {
        self.parts().snapshot_sha()
    }
}

pub fn snapshot(conn: &Connection, task_id: i64) -> Result<Option<Snapshot>> {
    let task = select(
        conn,
        "SELECT title, description, description_json, checkpoints FROM tasks WHERE id = ? AND deleted_at IS NULL",
        &[Sql::Integer(task_id)],
    )?;
    let Some(task) = task.first() else {
        return Ok(None);
    };
    let comments = select(
        conn,
        "SELECT id, message FROM comments WHERE task_id = ? AND deleted_at IS NULL ORDER BY id",
        &[Sql::Integer(task_id)],
    )?
    .iter()
    .map(|c| (c["id"].as_i64().unwrap_or(0), text(c, "message")))
    .collect();
    Ok(Some(Snapshot {
        title: text(task, "title"),
        description: text(task, "description"),
        description_json: text(task, "description_json"),
        checkpoints: text(task, "checkpoints"),
        comments,
    }))
}

/// Records that the owner has read the task text as it is now and lets agents run on it. `expected` is the
/// snapshot hash the owner was shown; if the task changed since, nothing is written. The caller must have
/// established that this is the owner (a desktop action, or the CLI after a terminal and a typed hash).
pub fn confirm_task(
    conn: &Connection,
    ctx: &Context,
    task_id: i64,
    expected: Option<&str>,
) -> Result<String> {
    conn.execute_batch("BEGIN IMMEDIATE")?;
    let done = (|| {
        crate::db::check_schema(conn)?;
        if !enabled(conn)? {
            return Err(Error::Unsupported(
                "this workspace has no edit journal yet (open it in the TMGR desktop app once to upgrade it)"
                    .into(),
            ));
        }
        let Some(snap) = snapshot(conn, task_id)? else {
            return Err(Error::NotFound(format!("task {task_id}")));
        };
        let hash = snap.hash();
        if expected.is_some_and(|e| e != hash) {
            return Err(Error::Unsupported(
                "the task changed since it was shown; nothing was confirmed".into(),
            ));
        }
        let author = Author {
            kind: "human",
            source: match ctx.origin {
                WriteOrigin::DesktopUi => "desktop_ui",
                WriteOrigin::Cli => "cli_confirm",
            },
            actor_id: ctx.user.id.to_string(),
        };
        let entry = Entry {
            entity: "task",
            entity_id: task_id,
            task_id,
            field: "confirm",
            content_sha: hash.clone(),
        };
        append_as(conn, ctx, &[entry], &author)?;
        Ok(hash)
    })();
    match done {
        Ok(hash) => {
            conn.execute_batch("COMMIT")?;
            Ok(hash)
        }
        Err(e) => {
            let _ = conn.execute_batch("ROLLBACK");
            Err(e)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Provenance {
    /// `human`, `agent`, `plugin`, `unattended` or `unrecorded`.
    pub created_by: String,
    pub last_human_seq: Option<i64>,
    pub last_human_content: Option<HumanContent>,
    pub modified_by_non_human_since: bool,
    /// The sorted, distinct `author_kind` of the non-human rows after the last human row.
    pub modified_by_kinds: Vec<String>,
    pub chain_ok: bool,
    /// The current text equals `last_human_content`.
    pub content_matches: bool,
    /// The latest `confirm` row's snapshot equals the current text and nothing non-human came after it.
    pub confirmed: bool,
    /// `chain_ok` and (`confirmed`, or created by the owner with every part last written by the owner and
    /// untouched by anything else since).
    pub human_written: bool,
}

impl Provenance {
    fn unrecorded(chain_ok: bool) -> Provenance {
        Provenance {
            created_by: "unrecorded".into(),
            last_human_seq: None,
            last_human_content: None,
            modified_by_non_human_since: false,
            modified_by_kinds: Vec::new(),
            chain_ok,
            content_matches: false,
            confirmed: false,
            human_written: false,
        }
    }

    pub fn is_human_written(&self) -> bool {
        self.human_written
    }
}

/// A task's text as a consumer saw it (for example a run bundle built from API answers), to be compared
/// with the human or confirmed text as well as the database.
pub struct Observed<'a> {
    pub title: &'a str,
    pub description: &'a str,
    pub description_json: &'a str,
    pub checkpoints: &'a str,
    pub comments: Vec<(i64, &'a str)>,
}

impl Observed<'_> {
    fn parts(&self) -> HumanContent {
        Snapshot {
            title: self.title.into(),
            description: self.description.into(),
            description_json: self.description_json.into(),
            checkpoints: self.checkpoints.into(),
            comments: self
                .comments
                .iter()
                .map(|(i, m)| (*i, m.to_string()))
                .collect(),
        }
        .parts()
    }
}

pub fn provenance(conn: &Connection, task_id: i64) -> Result<Provenance> {
    provenance_in(conn, task_id, None, &verify_chain(conn)?)
}

/// `provenance` that also requires `observed` to be the human or confirmed text.
pub fn provenance_observed(
    conn: &Connection,
    task_id: i64,
    observed: &Observed<'_>,
) -> Result<Provenance> {
    provenance_in(conn, task_id, Some(observed), &verify_chain(conn)?)
}

fn rows_intact(conn: &Connection, rows: &[JournalRow]) -> Result<bool> {
    for row in rows {
        if row_hash(row) != row.hash {
            return Ok(false);
        }
        let expected = if row.seq == 1 {
            Some(GENESIS_HASH.to_string())
        } else {
            select(
                conn,
                "SELECT hash FROM edit_journal WHERE seq = ?",
                &[Sql::Integer(row.seq - 1)],
            )?
            .first()
            .map(|r| text(r, "hash"))
        };
        if expected.as_deref() != Some(row.prev_hash.as_str()) {
            return Ok(false);
        }
    }
    Ok(true)
}

#[derive(Clone, Default)]
struct Fold {
    title: String,
    description: String,
    description_json: String,
    checkpoints: String,
    comments: Vec<(i64, String)>,
}

impl Fold {
    fn apply(&mut self, row: &JournalRow) {
        match (row.entity.as_str(), row.field.as_str()) {
            ("task", "create" | "title") => self.title = row.content_sha256.clone(),
            ("task", "description") => self.description = row.content_sha256.clone(),
            ("task", "description_json") => self.description_json = row.content_sha256.clone(),
            ("task", "checkpoints") => self.checkpoints = row.content_sha256.clone(),
            ("comment", "comment") => {
                let id = row.entity_id.parse().unwrap_or(-1);
                self.comments.retain(|(c, _)| *c != id);
                self.comments.push((id, row.content_sha256.clone()));
            }
            ("comment", "delete") => {
                let id = row.entity_id.parse().unwrap_or(-1);
                self.comments.retain(|(c, _)| *c != id);
            }
            _ => {}
        }
    }

    fn content(&self) -> HumanContent {
        let empty = content_sha(None);
        HumanContent {
            title_sha: self.title.clone(),
            description_sha: if self.description.is_empty() {
                empty.clone()
            } else {
                self.description.clone()
            },
            description_json_sha: if self.description_json.is_empty() {
                empty
            } else {
                self.description_json.clone()
            },
            checkpoints_sha: if self.checkpoints.is_empty() {
                content_sha(None)
            } else {
                self.checkpoints.clone()
            },
            comments_sha: comments_digest(&self.comments),
        }
    }
}

/// `provenance` for many tasks of one scan: the chain is verified once by the caller and the same answer is
/// used for each task; the task's own rows are always verified in full.
pub fn provenance_in(
    conn: &Connection,
    task_id: i64,
    observed: Option<&Observed<'_>>,
    chain: &ChainStatus,
) -> Result<Provenance> {
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
    let chain_ok = chain.ok && rows_intact(conn, &rows)?;
    if rows.is_empty() {
        return Ok(Provenance::unrecorded(chain_ok));
    }
    let created = rows
        .iter()
        .find(|r| r.entity == "task" && r.field == "create");
    let created_by = created.map_or("unrecorded".to_string(), |r| r.author_kind.clone());
    let last_human = rows
        .iter()
        .filter(|r| r.author_kind == "human")
        .map(|r| r.seq)
        .max();
    let mut kinds: Vec<String> = rows
        .iter()
        .filter(|r| r.author_kind != "human" && last_human.is_none_or(|h| r.seq > h))
        .map(|r| r.author_kind.clone())
        .collect();
    kinds.sort();
    kinds.dedup();
    let modified = !kinds.is_empty();

    let mut fold = Fold::default();
    let mut at_last_human = Fold::default();
    let mut title_by = String::new();
    let mut description_by = created_by.clone();
    let mut json_by = created_by.clone();
    let mut checkpoints_by = created_by.clone();
    let mut comment_by: Vec<(i64, String)> = Vec::new();
    for row in &rows {
        fold.apply(row);
        if last_human.is_some_and(|h| row.seq <= h) {
            at_last_human.apply(row);
        }
        match (row.entity.as_str(), row.field.as_str()) {
            ("task", "create" | "title") => title_by = row.author_kind.clone(),
            ("task", "description") => description_by = row.author_kind.clone(),
            ("task", "description_json") => json_by = row.author_kind.clone(),
            ("task", "checkpoints") => checkpoints_by = row.author_kind.clone(),
            ("comment", "comment") => {
                let id = row.entity_id.parse().unwrap_or(-1);
                comment_by.retain(|(c, _)| *c != id);
                comment_by.push((id, row.author_kind.clone()));
            }
            ("comment", "delete") => {
                let id = row.entity_id.parse().unwrap_or(-1);
                comment_by.retain(|(c, _)| *c != id);
            }
            _ => {}
        }
    }
    let human_content = last_human.map(|_| at_last_human.content());
    let current = snapshot(conn, task_id)?.map(|s| s.parts());
    let observed_parts = observed.map(Observed::parts);
    let text_is = |target: &HumanContent| {
        current.as_ref() == Some(target) && observed_parts.as_ref().is_none_or(|o| o == target)
    };
    let content_matches = human_content.as_ref().is_some_and(&text_is);

    let confirm = rows
        .iter()
        .filter(|r| r.entity == "task" && r.field == "confirm" && r.author_kind == "human")
        .max_by_key(|r| r.seq);
    let confirmed = confirm.is_some_and(|c| {
        let snapshot_is = |parts: &Option<HumanContent>| {
            parts
                .as_ref()
                .is_some_and(|p| p.snapshot_sha() == c.content_sha256)
        };
        snapshot_is(&current)
            && observed_parts
                .as_ref()
                .is_none_or(|_| snapshot_is(&observed_parts))
            && !rows
                .iter()
                .any(|r| r.author_kind != "human" && r.seq > c.seq)
    });

    let all_human = title_by == "human"
        && description_by == "human"
        && json_by == "human"
        && checkpoints_by == "human"
        && comment_by.iter().all(|(_, k)| k == "human");
    let journal_state_current = text_is(&fold.content());
    let human_written = chain_ok
        && (confirmed
            || (created_by == "human"
                && !modified
                && all_human
                && journal_state_current
                && content_matches));
    Ok(Provenance {
        created_by,
        last_human_seq: last_human,
        last_human_content: human_content,
        modified_by_non_human_since: modified,
        modified_by_kinds: kinds,
        chain_ok,
        content_matches,
        confirmed,
        human_written,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::Actor;
    use crate::testing::{ctx, db, db12};
    use crate::{dispatch, Response};
    use chrono::{DateTime, Utc};

    const T: &str = "2026-10-14T10:00:00.000Z";

    fn writable() -> Connection {
        let conn = db("INSERT INTO personas (uuid, owner_user_id, owner_name, name, synced_at) VALUES ('p1', 1, 'Owner', 'Scribe', 'x');
            INSERT INTO workspace_personas (persona_uuid, permissions, enabled_at) VALUES ('p1', '[\"tasks:write\",\"comments:write\"]', 'x');");
        conn.execute_batch("PRAGMA query_only = OFF").unwrap();
        conn
    }

    fn call(conn: &Connection, c: &Context, method: &str, url: &str, body: Value) -> Response {
        dispatch(conn, c, method, url, Some(&body))
            .unwrap()
            .unwrap()
    }

    fn origin(origin: WriteOrigin) -> Context {
        let mut c = ctx(T);
        c.origin = origin;
        c
    }

    fn desktop() -> Context {
        origin(WriteOrigin::DesktopUi)
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
        select(
            conn,
            "SELECT author_kind, source FROM edit_journal ORDER BY seq",
            &[],
        )
        .unwrap()
        .iter()
        .map(|r| (text(r, "author_kind"), text(r, "source")))
        .collect()
    }

    fn vars(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn every_marker_makes_an_agent_session() {
        assert!(!in_agent_session(vars(&[
            "PATH",
            "HOME",
            "CLAUDECODE_NOT",
            "MY_CODEX_SANDBOX"
        ])));
        for marker in [
            "CLAUDECODE",
            "CLAUDE_CODE_ENTRYPOINT",
            "CODEX_SANDBOX",
            "CODEX_SANDBOX_NETWORK_DISABLED",
            "TMGR_AGENT_SESSION",
            "TMGR_RUN_TOKEN",
            "TMGR_RUN_ID",
        ] {
            assert!(in_agent_session(vars(&["PATH", marker])), "{marker}");
        }
    }

    #[test]
    fn only_the_desktop_ui_writes_human_rows() {
        let conn = writable();
        for (o, expected) in [
            (WriteOrigin::DesktopUi, ("human", "desktop_ui")),
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
        assert_eq!(
            rows(&conn),
            vec![("unattended".to_string(), "cli".to_string())]
        );
        assert!(!provenance(&conn, 1).unwrap().is_human_written());
    }

    #[test]
    fn a_persona_is_an_agent_whatever_the_body_and_origin_claim() {
        let conn = writable();
        let mut c = persona();
        c.origin = WriteOrigin::DesktopUi;
        let forged = json!({
            "title": "Forged", "author_kind": "human", "source": "desktop_ui", "actor_id": "1",
            "actor": { "kind": "user", "id": "1" }, "origin": "desktop_ui", "seq": 1, "hash": "x"
        });
        assert_eq!(call(&conn, &c, "POST", "tasks", forged.clone()).status, 201);
        let mut comment = forged.clone();
        comment["message"] = json!("note");
        assert_eq!(
            call(&conn, &c, "POST", "tasks/1/comments", comment).status,
            201
        );
        let mut edit = forged;
        edit["title"] = json!("Retitled");
        assert_eq!(call(&conn, &c, "PATCH", "tasks/1", edit).status, 200);
        let all = select(
            &conn,
            "SELECT author_kind, source, actor_id FROM edit_journal",
            &[],
        )
        .unwrap();
        assert_eq!(all.len(), 3);
        for r in &all {
            assert_eq!(
                (
                    text(r, "author_kind"),
                    text(r, "source"),
                    text(r, "actor_id")
                ),
                (
                    "agent".to_string(),
                    "local_access".to_string(),
                    "p1".to_string()
                )
            );
        }
        assert!(!provenance(&conn, 1).unwrap().is_human_written());
    }

    #[test]
    fn a_plugin_and_unknown_actors_are_never_human() {
        let conn = writable();
        for (kind, expected) in [
            ("plugin", "plugin"),
            ("system", "unattended"),
            ("whatever", "unattended"),
            ("user", "unattended"),
        ] {
            let mut c = desktop();
            c.actor = Some(Actor {
                kind: kind.into(),
                id: "x".into(),
                name: "x".into(),
                owner_id: None,
            });
            call(&conn, &c, "POST", "tasks", json!({ "title": "t" }));
            assert_eq!(rows(&conn).pop().unwrap().0, expected, "{kind}");
        }
    }

    #[test]
    fn the_runner_actor_is_an_agent_and_no_actor_is_ever_human() {
        let conn = writable();
        let actor = |kind: &str, id: &str, owner: Option<&str>| {
            let mut c = desktop();
            c.actor = Some(Actor {
                kind: kind.into(),
                id: id.into(),
                name: id.into(),
                owner_id: owner.map(str::to_string),
            });
            c
        };
        let cases = [
            (
                actor("plugin", "runner", None),
                ("agent", "runner", "runner"),
            ),
            (
                actor("plugin", "runner", Some("evil")),
                ("plugin", "plugin", "evil"),
            ),
            (
                actor("plugin", "tmgr.x", None),
                ("plugin", "plugin", "tmgr.x"),
            ),
            (actor("user", "1", None), ("unattended", "import", "1")),
            (actor("mystery", "m", None), ("unattended", "import", "m")),
            (actor("", "", None), ("unattended", "import", "")),
        ];
        for (c, expected) in cases {
            call(&conn, &c, "POST", "tasks", json!({ "title": "t" }));
            let last = select(
                &conn,
                "SELECT author_kind, source, actor_id FROM edit_journal ORDER BY seq DESC LIMIT 1",
                &[],
            )
            .unwrap();
            let got = (
                text(&last[0], "author_kind"),
                text(&last[0], "source"),
                text(&last[0], "actor_id"),
            );
            assert_eq!(
                got,
                (
                    expected.0.to_string(),
                    expected.1.to_string(),
                    expected.2.to_string()
                )
            );
        }
        let humans: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM edit_journal WHERE author_kind = 'human'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(humans, 0);
    }

    #[test]
    fn a_schema_12_database_gets_no_journal_and_answers_unrecorded() {
        let conn = db12("");
        conn.execute_batch("PRAGMA query_only = OFF").unwrap();
        let r = call(&conn, &desktop(), "POST", "tasks", json!({ "title": "t" }));
        assert_eq!(r.status, 201);
        let tables: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name = 'edit_journal'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 0);
        let p = provenance(&conn, 1).unwrap();
        assert_eq!(p.created_by, "unrecorded");
        assert!(!p.chain_ok && !p.is_human_written());
        assert!(!verify_chain(&conn).unwrap().ok);
        assert!(confirm_task(&conn, &origin(WriteOrigin::Cli), 1, None).is_err());
    }

    #[test]
    fn a_failed_request_leaves_no_row() {
        let conn = writable();
        let c = desktop();
        assert_eq!(
            call(&conn, &c, "PATCH", "tasks/9", json!({ "title": "x" })).status,
            404
        );
        assert_eq!(
            call(&conn, &c, "POST", "tasks", json!({ "title": "" })).status,
            422
        );
        let body = json!({ "title": "late", "persona_assignees": ["nope"] });
        assert_eq!(call(&conn, &c, "POST", "tasks", body).status, 422);
        assert!(rows(&conn).is_empty());
    }

    #[test]
    fn a_tampered_chain_is_found_for_an_edit_a_delete_and_a_reorder() {
        let make = || {
            let conn = writable();
            let c = desktop();
            call(
                &conn,
                &c,
                "POST",
                "tasks",
                json!({ "title": "T", "description": "D" }),
            );
            call(&conn, &c, "PATCH", "tasks/1", json!({ "title": "T2" }));
            call(
                &conn,
                &c,
                "POST",
                "tasks/1/comments",
                json!({ "message": "hi" }),
            );
            conn
        };
        let intact = make();
        let status = verify_chain_full(&intact).unwrap();
        assert!(status.ok && status.rows == 4 && status.tail_seq == 4);
        assert!(provenance(&intact, 1).unwrap().is_human_written());

        let edited = make();
        edited
            .execute_batch("UPDATE edit_journal SET author_kind = 'agent' WHERE seq = 3")
            .unwrap();
        assert_eq!(verify_chain_full(&edited).unwrap().broken_at, Some(3));
        assert!(!provenance(&edited, 1).unwrap().chain_ok);

        let deleted = make();
        deleted
            .execute_batch("DELETE FROM edit_journal WHERE seq = 2")
            .unwrap();
        assert_eq!(verify_chain_full(&deleted).unwrap().broken_at, Some(3));
        assert!(!provenance(&deleted, 1).unwrap().is_human_written());

        let reordered = make();
        reordered
            .execute_batch(
                "UPDATE edit_journal SET seq = 99 WHERE seq = 2; UPDATE edit_journal SET seq = 2 WHERE seq = 3; UPDATE edit_journal SET seq = 3 WHERE seq = 99;",
            )
            .unwrap();
        assert!(!verify_chain_full(&reordered).unwrap().ok);
        assert!(!provenance(&reordered, 1).unwrap().is_human_written());
    }

    #[test]
    fn verification_resumes_from_the_checkpoint_but_a_tasks_own_rows_are_always_checked() {
        let conn = writable();
        let c = desktop();
        call(&conn, &c, "POST", "tasks", json!({ "title": "T" }));
        call(&conn, &c, "POST", "tasks", json!({ "title": "U" }));
        let first = verify_chain(&conn).unwrap();
        assert_eq!(first.rows, 2);
        store_checkpoint(&conn, &first).unwrap();
        call(&conn, &c, "PATCH", "tasks/1", json!({ "title": "T2" }));
        let next = verify_chain(&conn).unwrap();
        assert!(next.ok && next.rows == 1 && next.tail_seq == 3);
        conn.execute_batch("UPDATE edit_journal SET author_kind = 'agent' WHERE seq = 1")
            .unwrap();
        assert!(verify_chain(&conn).unwrap().ok);
        assert!(!verify_chain_full(&conn).unwrap().ok);
        assert!(!provenance(&conn, 1).unwrap().chain_ok);
        assert!(provenance(&conn, 2).unwrap().chain_ok);
        conn.execute_batch("UPDATE edit_journal SET hash = 'x' WHERE seq = 2")
            .unwrap();
        assert!(!verify_chain(&conn).unwrap().ok);
    }

    #[test]
    fn a_walk_from_genesis_is_repeated_when_the_last_one_is_an_hour_old() {
        let conn = writable();
        let c = desktop();
        call(&conn, &c, "POST", "tasks", json!({ "title": "T" }));
        call(&conn, &c, "POST", "tasks", json!({ "title": "U" }));
        let t0 = DateTime::parse_from_rfc3339("2026-10-14T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let first = verify_chain_at(&conn, t0).unwrap();
        assert!(first.full && first.ok);
        store_checkpoint_at(&conn, &first, t0).unwrap();
        call(&conn, &c, "PATCH", "tasks/1", json!({ "title": "T2" }));
        let t1 = t0 + chrono::Duration::minutes(30);
        let second = verify_chain_at(&conn, t1).unwrap();
        assert!(!second.full && second.rows == 1);
        store_checkpoint_at(&conn, &second, t1).unwrap();
        conn.execute_batch("UPDATE edit_journal SET author_kind = 'agent' WHERE seq = 1")
            .unwrap();
        assert!(verify_chain_at(&conn, t1).unwrap().ok);
        let stored: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key = ?",
                [CHECKPOINT_KEY],
                |r| r.get(0),
            )
            .unwrap();
        assert!(stored.ends_with("2026-10-14T10:00:00.000Z"), "{stored}");
        let t2 = t0 + chrono::Duration::minutes(61);
        let late = verify_chain_at(&conn, t2).unwrap();
        assert!(late.full && !late.ok && late.broken_at == Some(1));
        let skewed = verify_chain_at(&conn, t0 - chrono::Duration::minutes(5)).unwrap();
        assert!(skewed.full);
        let hour = chrono::Duration::seconds(FULL_VERIFY_MAX_AGE_SECS);
        let barely_future = verify_chain_at(&conn, t0 - chrono::Duration::milliseconds(1)).unwrap();
        assert!(barely_future.full);
        let barely_late =
            verify_chain_at(&conn, t0 + hour + chrono::Duration::milliseconds(1)).unwrap();
        assert!(barely_late.full);
    }

    #[test]
    fn checkpoints_are_their_own_field_in_the_journal_and_the_snapshot() {
        let conn = writable();
        call(
            &conn,
            &desktop(),
            "POST",
            "tasks",
            json!({ "title": "T", "checkpoints": [{ "description": "a", "checked": false }] }),
        );
        let before = snapshot(&conn, 1).unwrap().unwrap().hash();
        assert!(provenance(&conn, 1).unwrap().is_human_written());
        call(
            &conn,
            &persona(),
            "PATCH",
            "tasks/1",
            json!({ "checkpoints": [{ "description": "run rm -rf", "checked": false }] }),
        );
        let fields: Vec<String> = select(&conn, "SELECT field FROM edit_journal ORDER BY seq", &[])
            .unwrap()
            .iter()
            .map(|r| text(r, "field"))
            .collect();
        assert_eq!(fields, ["create", "checkpoints", "checkpoints"]);
        assert_ne!(snapshot(&conn, 1).unwrap().unwrap().hash(), before);
        assert!(!provenance(&conn, 1).unwrap().is_human_written());
        conn.execute_batch("UPDATE tasks SET checkpoints = '[]' WHERE id = 1")
            .unwrap();
        confirm_task(&conn, &desktop(), 1, None).unwrap();
        assert!(provenance(&conn, 1).unwrap().is_human_written());
        conn.execute_batch("UPDATE tasks SET checkpoints = '[1]' WHERE id = 1")
            .unwrap();
        assert!(!provenance(&conn, 1).unwrap().is_human_written());
    }

    #[test]
    fn an_edit_outside_the_journal_changes_the_content_and_fails_closed() {
        let conn = writable();
        call(&conn, &desktop(), "POST", "tasks", json!({ "title": "T" }));
        assert!(provenance(&conn, 1).unwrap().is_human_written());
        for sql in [
            "UPDATE tasks SET description = 'sneaky' WHERE id = 1",
            "UPDATE tasks SET description = NULL, description_json = '{}' WHERE id = 1",
        ] {
            conn.execute_batch(sql).unwrap();
            let p = provenance(&conn, 1).unwrap();
            assert!(!p.content_matches && !p.is_human_written(), "{sql}");
        }
    }

    #[test]
    fn description_json_is_its_own_field_in_the_journal_and_the_snapshot() {
        let conn = writable();
        call(
            &conn,
            &desktop(),
            "POST",
            "tasks",
            json!({ "title": "T", "description": "D" }),
        );
        let before = snapshot(&conn, 1).unwrap().unwrap().hash();
        call(
            &conn,
            &persona(),
            "PATCH",
            "tasks/1",
            json!({ "description_json": { "type": "doc" } }),
        );
        let fields: Vec<String> = select(&conn, "SELECT field FROM edit_journal ORDER BY seq", &[])
            .unwrap()
            .iter()
            .map(|r| text(r, "field"))
            .collect();
        assert_eq!(fields, ["create", "description", "description_json"]);
        assert_ne!(snapshot(&conn, 1).unwrap().unwrap().hash(), before);
        assert!(!provenance(&conn, 1).unwrap().is_human_written());
    }

    #[test]
    fn what_a_consumer_saw_must_be_the_human_text() {
        let conn = writable();
        let c = desktop();
        call(
            &conn,
            &c,
            "POST",
            "tasks",
            json!({ "title": "T", "description": "D\r\nE" }),
        );
        call(
            &conn,
            &c,
            "POST",
            "tasks/1/comments",
            json!({ "message": "note" }),
        );
        let same = Observed {
            title: "T",
            description: "D\nE",
            description_json: "",
            checkpoints: "",
            comments: vec![(1, "note")],
        };
        assert!(provenance_observed(&conn, 1, &same)
            .unwrap()
            .is_human_written());
        let other = Observed {
            title: "T",
            description: "changed",
            description_json: "",
            checkpoints: "",
            comments: vec![(1, "note")],
        };
        assert!(!provenance_observed(&conn, 1, &other)
            .unwrap()
            .is_human_written());
        let extra = Observed {
            title: "T",
            description: "D\nE",
            description_json: "",
            checkpoints: "",
            comments: vec![(1, "note"), (2, "more")],
        };
        assert!(!provenance_observed(&conn, 1, &extra)
            .unwrap()
            .is_human_written());
        let json = Observed {
            title: "T",
            description: "D\nE",
            description_json: "{}",
            checkpoints: "",
            comments: vec![(1, "note")],
        };
        assert!(!provenance_observed(&conn, 1, &json)
            .unwrap()
            .is_human_written());
    }

    #[test]
    fn a_cli_task_is_not_human_until_the_owner_confirms_exactly_that_text() {
        let conn = writable();
        let cli = origin(WriteOrigin::Cli);
        call(
            &conn,
            &cli,
            "POST",
            "tasks",
            json!({ "title": "From a script", "description": "steps" }),
        );
        call(
            &conn,
            &cli,
            "POST",
            "tasks/1/comments",
            json!({ "message": "context" }),
        );
        assert!(!provenance(&conn, 1).unwrap().is_human_written());
        let shown = snapshot(&conn, 1).unwrap().unwrap().hash();
        assert!(confirm_task(&conn, &cli, 1, Some("not the shown hash")).is_err());
        assert_eq!(rows(&conn).len(), 3);
        assert_eq!(confirm_task(&conn, &cli, 1, Some(&shown)).unwrap(), shown);
        let p = provenance(&conn, 1).unwrap();
        assert!(p.confirmed && p.is_human_written());
        assert_eq!(
            rows(&conn).pop().unwrap(),
            ("human".to_string(), "cli_confirm".to_string())
        );
        let same = Observed {
            title: "From a script",
            description: "steps",
            description_json: "",
            checkpoints: "",
            comments: vec![(1, "context")],
        };
        assert!(provenance_observed(&conn, 1, &same)
            .unwrap()
            .is_human_written());
        let other = Observed {
            title: "From a script",
            description: "steps!",
            description_json: "",
            checkpoints: "",
            comments: vec![(1, "context")],
        };
        assert!(!provenance_observed(&conn, 1, &other)
            .unwrap()
            .is_human_written());

        call(
            &conn,
            &persona(),
            "POST",
            "tasks/1/comments",
            json!({ "message": "agent adds" }),
        );
        assert!(!provenance(&conn, 1).unwrap().is_human_written());
        confirm_task(&conn, &desktop(), 1, None).unwrap();
        assert!(provenance(&conn, 1).unwrap().is_human_written());
        conn.execute_batch("UPDATE tasks SET title = 'edited behind' WHERE id = 1")
            .unwrap();
        assert!(!provenance(&conn, 1).unwrap().is_human_written());
    }

    #[test]
    fn a_legacy_task_becomes_human_only_through_a_confirm() {
        let conn = writable();
        conn.execute_batch(
            "INSERT INTO tasks (id, title, created_at, updated_at) VALUES (1, 'old', 'x', 'x')",
        )
        .unwrap();
        assert!(!provenance(&conn, 1).unwrap().is_human_written());
        call(
            &conn,
            &desktop(),
            "PATCH",
            "tasks/1",
            json!({ "description": "human edit" }),
        );
        assert!(!provenance(&conn, 1).unwrap().is_human_written());
        confirm_task(&conn, &origin(WriteOrigin::Cli), 1, None).unwrap();
        assert!(provenance(&conn, 1).unwrap().is_human_written());
    }

    #[test]
    fn two_writers_on_one_file_extend_one_chain() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("workspace.db");
        Connection::open(&path)
            .unwrap()
            .execute_batch(&crate::testing::schema())
            .unwrap();
        let desktop_conn = crate::open_write(&path).unwrap();
        let cli = crate::open_write(&path).unwrap();
        for i in 0..4 {
            let (conn, o) = if i % 2 == 0 {
                (&desktop_conn, WriteOrigin::DesktopUi)
            } else {
                (&cli, WriteOrigin::Cli)
            };
            let r = call(
                conn,
                &origin(o),
                "POST",
                "tasks",
                json!({ "title": format!("t{i}") }),
            );
            assert_eq!(r.status, 201);
        }
        let status = verify_chain_full(&desktop_conn).unwrap();
        assert!(status.ok && status.rows == 4);
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
        assert_eq!(
            content_sha(None),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(content_sha(Some("a\r\nb")), content_sha(Some("a\nb")));
        let parts = HumanContent {
            title_sha: "a".into(),
            description_sha: "b".into(),
            description_json_sha: "c".into(),
            checkpoints_sha: "k".into(),
            comments_sha: "d".into(),
        };
        assert_eq!(
            parts.snapshot_sha(),
            sha256_hex(b"[\"a\",\"b\",\"c\",\"k\",\"d\"]")
        );
    }
}
