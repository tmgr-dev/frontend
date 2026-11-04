use super::attach_consent::Reads;
use super::catalog::{self, clean_line, index, Agent, Entry, Filter, Live};
use super::ops::{short_cwd, span};
use super::{is_down, rerr, runner_config, Opts, SessionAgent, SessionListArgs};
use serde::Serialize;
use serde_json::json;
use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use tmgr_core::error::{Error, Result};
use tmgr_ipc::api::{SessionInfo, StateKind};
use tmgr_link::privileged;
use tmgr_runner::wire::now_ms;
use tmgr_usage::paths::Env;

const PATH_MAX: usize = 512;
const TEXT_MAX: usize = 70;
const PROJECT_MAX: usize = 40;

#[derive(Debug, Serialize)]
pub(super) struct Row {
    source: &'static str,
    agent: String,
    id: String,
    cwd: Option<String>,
    project: Option<String>,
    when_ms: u64,
    live: Live,
    title: Option<String>,
    first_message: Option<String>,
    git_branch: Option<String>,
    readable: bool,
    snippet: Option<String>,
    #[serde(skip)]
    agent_session: Option<String>,
}

impl Row {
    fn external(e: Entry, snippet: Option<&str>) -> Row {
        Row {
            source: "external",
            agent: e.agent.profile().to_string(),
            id: e.id,
            cwd: e.cwd.map(|c| clean_line(&c, PATH_MAX)),
            project: e.project,
            when_ms: e.mtime_ms,
            live: e.live,
            title: e.title.map(|t| clean_line(&t, 200)),
            first_message: e.first_message.map(|t| clean_line(&t, 200)),
            git_branch: e.git_branch.map(|t| clean_line(&t, 200)),
            readable: e.readable,
            snippet: snippet.map(|s| clean_line(s, 200)),
            agent_session: None,
        }
    }

    fn runner(s: &SessionInfo) -> Row {
        Row {
            source: "tmgr",
            agent: s
                .profile
                .as_deref()
                .map_or("-".to_string(), |p| clean_line(p, 64)),
            id: clean_line(&s.id, 64),
            cwd: Some(clean_line(&s.cwd, PATH_MAX)),
            project: None,
            when_ms: s.state.since_ms,
            live: if s.state.kind == StateKind::Exited {
                Live::No
            } else {
                Live::Yes
            },
            title: Some(clean_line(&s.name, 200)),
            first_message: None,
            git_branch: None,
            readable: true,
            snippet: None,
            agent_session: s.agent_session_id.clone(),
        }
    }

    fn text(&self) -> String {
        let t = self
            .snippet
            .as_ref()
            .or(self.title.as_ref())
            .or(self.first_message.as_ref());
        clean_line(t.map_or("-", String::as_str), TEXT_MAX)
    }

    fn project_label(&self) -> String {
        let p = match (&self.cwd, &self.project) {
            (Some(c), _) => short_cwd(c),
            (None, Some(p)) => p.clone(),
            (None, None) => "-".to_string(),
        };
        clean_line(&p, PROJECT_MAX)
    }
}

pub(super) fn dedupe(rows: Vec<Row>) -> Vec<Row> {
    let owned: Vec<String> = rows
        .iter()
        .filter(|r| r.source == "tmgr")
        .filter_map(|r| r.agent_session.clone())
        .collect();
    rows.into_iter()
        .filter(|r| r.source == "tmgr" || !owned.contains(&r.id))
        .collect()
}

fn refuse_in_agent() -> Result<()> {
    let in_agent = privileged::RUN_ENV
        .iter()
        .any(|k| std::env::var_os(k).is_some())
        || tmgr_core::local::agent_session_detected();
    privileged::check(in_agent, true)
}

fn index_file() -> Option<PathBuf> {
    tmgr_core::config::config_dir()
        .ok()
        .map(|d| index::index_path(&d))
}

fn runner_rows(agent: Option<Agent>, notes: &mut Vec<String>) -> Vec<Row> {
    let listed =
        runner_config().and_then(|cfg| match tmgr_runner::client::Client::connect(&cfg.dir) {
            Ok(mut c) => c.list().map_err(rerr),
            Err(e) if is_down(&e) => Err(Error::new(
                tmgr_core::error::Kind::Offline,
                "the runner is not running",
            )),
            Err(e) => Err(rerr(e)),
        });
    match listed {
        Ok(v) => v
            .iter()
            .filter(|s| agent.is_none_or(|a| s.profile.as_deref() == Some(a.profile())))
            .map(Row::runner)
            .collect(),
        Err(e) => {
            notes.push(format!(
                "tmgr sessions are not listed: {}",
                clean_line(&e.message, 200)
            ));
            Vec::new()
        }
    }
}

fn search_rows(
    env: &Env,
    q: &str,
    agent: Option<Agent>,
    limit: usize,
    notes: &mut Vec<String>,
) -> Result<Vec<Row>> {
    let entries = catalog::scan(env, agent);
    if !entries.iter().any(|e| e.readable) {
        return Ok(Vec::new());
    }
    let fail = |e: index::IndexError| {
        Error::internal(format!("search index: {}", clean_line(&e.to_string(), 200)))
    };
    let path = index_file().ok_or_else(|| Error::internal("no config directory"))?;
    let ix = index::Index::open(&path).map_err(fail)?;
    let tty = std::io::stderr().is_terminal();
    let all: Vec<Entry> = catalog::scan(env, None);
    let out = ix
        .update(&all, |done, total| {
            if tty && (done % 20 == 0 || done == total) {
                eprint!("\rIndexing sessions {done}/{total}");
            }
        })
        .map_err(fail)?;
    if tty {
        eprint!("\r\x1b[K");
        let _ = std::io::stderr().flush();
    }
    if out.busy {
        notes.push("another tmgr is updating the search index; results may be partial".into());
    }
    let mut hits = ix.search(q, limit).map_err(fail)?;
    hits.sort_by(|a, b| a.rank.total_cmp(&b.rank));
    let ids: Vec<String> = hits.iter().map(|h| h.session_id.clone()).collect();
    let listing = catalog::list(
        env,
        Filter {
            agent,
            limit: None,
            ids: Some(ids),
        },
    );
    let mut rows = Vec::new();
    for h in &hits {
        if let Some(e) = listing.entries.iter().find(|e| e.id == h.session_id) {
            rows.push(Row::external(e.clone(), Some(&h.snippet)));
        }
    }
    Ok(rows)
}

pub(super) fn run(o: &Opts, a: &SessionListArgs) -> Result<i32> {
    refuse_in_agent()?;
    let env = Env::from_process();
    let reads = Reads::of(&env);
    if let Some(p) = index_file() {
        index::enforce_consent(&p, reads.claude, reads.codex);
    }
    let agent = a.agent.map(|x| match x {
        SessionAgent::Claude => Agent::Claude,
        SessionAgent::Codex => Agent::Codex,
    });
    let limit = a.limit as usize;
    let mut notes = Vec::new();
    let mut hints = Vec::new();
    let rows = match &a.search {
        Some(q) => {
            let listing = catalog::list(
                &env,
                Filter {
                    agent,
                    limit: Some(0),
                    ids: None,
                },
            );
            hints = listing.hints;
            search_rows(&env, q, agent, limit, &mut notes)?
        }
        None => {
            let listing = catalog::list(
                &env,
                Filter {
                    agent,
                    limit: Some(limit),
                    ids: None,
                },
            );
            hints = listing.hints.into_iter().chain(hints).collect();
            let mut rows: Vec<Row> = listing
                .entries
                .into_iter()
                .map(|e| Row::external(e, None))
                .collect();
            rows.extend(runner_rows(agent, &mut notes));
            let mut rows = dedupe(rows);
            rows.sort_by_key(|r| std::cmp::Reverse(r.when_ms));
            rows.truncate(limit);
            rows
        }
    };
    for n in &notes {
        if !o.json {
            eprintln!("{n}");
        }
    }
    let human = render(&rows, &hints);
    Ok(super::emit(
        o,
        json!({"sessions": rows, "hints": hints, "notes": notes}),
        human,
    ))
}

fn render(rows: &[Row], hints: &[String]) -> String {
    let mut lines = Vec::new();
    if rows.is_empty() {
        lines.push("No sessions found.".to_string());
    } else {
        let now = now_ms();
        let mut table = vec![["AGENT", "ID", "PROJECT", "WHEN", "LIVE", "TEXT"]
            .map(String::from)
            .to_vec()];
        for r in rows {
            table.push(vec![
                clean_line(&r.agent, 16),
                r.id.clone(),
                r.project_label(),
                format!("{} ago", span(now.saturating_sub(r.when_ms) / 1000)),
                match r.live {
                    Live::Yes => "live",
                    Live::Recent => "recent",
                    Live::No => "-",
                }
                .to_string(),
                r.text(),
            ]);
        }
        lines.push(crate::out::table(&table));
    }
    lines.extend(hints.iter().cloned());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(source: &'static str, id: &str, agent_session: Option<&str>) -> Row {
        Row {
            source,
            agent: "claude-code".into(),
            id: id.into(),
            cwd: None,
            project: None,
            when_ms: 0,
            live: Live::No,
            title: None,
            first_message: None,
            git_branch: None,
            readable: true,
            snippet: None,
            agent_session: agent_session.map(Into::into),
        }
    }

    #[test]
    fn an_external_row_is_dropped_when_a_tmgr_row_owns_its_agent_session() {
        let rows = vec![
            row("tmgr", "s_aaaaaaaaaa", Some("u1")),
            row("external", "u1", None),
            row("external", "u2", None),
        ];
        let ids: Vec<String> = dedupe(rows).into_iter().map(|r| r.id).collect();
        assert_eq!(ids, ["s_aaaaaaaaaa", "u2"]);
    }

    #[test]
    fn rows_without_an_agent_session_are_all_kept() {
        let rows = vec![row("tmgr", "s_a", None), row("external", "u1", None)];
        assert_eq!(dedupe(rows).len(), 2);
    }

    #[test]
    fn hostile_text_is_neutralised_in_every_field() {
        let mut e = catalog::testkit::entry(Agent::Claude);
        let evil = "a\u{1b}]0;x\u{7}b\u{202e}c\nd";
        e.cwd = Some(format!("/w/{evil}"));
        e.title = Some(evil.into());
        e.first_message = Some(evil.into());
        let r = Row::external(e, Some(evil));
        let json = serde_json::to_string(&r).unwrap();
        let human = render(&[r], &[]);
        for out in [json, human] {
            assert!(
                !out.contains('\u{1b}') && !out.contains('\u{202e}'),
                "{out:?}"
            );
            assert!(!out.contains('\u{7}'), "{out:?}");
        }
    }

    #[test]
    fn a_tmgr_row_carries_the_agent_session_so_its_conversation_is_listed_once() {
        let id = "2f0e1c3a-5b7d-4e8f-9a1b-3c4d5e6f7a8b";
        let info: SessionInfo = serde_json::from_value(json!({
            "id": "s_aaaaaaaaaa", "name": "w", "argv": ["/bin/sh"], "cwd": "/w",
            "profile": "claude-code", "state": {"kind": "exited", "since_ms": 1},
            "created_ms": 1, "cols": 80, "rows": 24, "shim_pid": 1, "child_pid": 2,
            "agent_session_id": id,
        }))
        .unwrap();
        let rows = vec![Row::runner(&info), row("external", id, None)];
        let ids: Vec<String> = dedupe(rows).into_iter().map(|r| r.id).collect();
        assert_eq!(ids, ["s_aaaaaaaaaa"]);
    }
}
