use super::index::{self, Index};
use super::testkit::*;
use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use tmgr_usage::consent::ReadLogs;

fn ids(l: &Listing) -> Vec<String> {
    l.entries.iter().map(|e| e.id.clone()).collect()
}

fn all(fx: &Fx) -> Listing {
    list(&fx.env, Filter::default())
}

fn age(path: &Path, secs: u64) {
    let t = std::time::SystemTime::now() - Duration::from_secs(secs);
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(t)
        .unwrap();
}

use std::time::Duration;

fn hash_tree(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                out.insert(
                    p.clone(),
                    Sha256::digest(std::fs::read(&p).unwrap()).to_vec(),
                );
            }
        }
    }
    out
}

#[test]
fn claude_lists_only_top_level_session_files() {
    let fx = Fx::new();
    fx.grant();
    let id = uid(1);
    fx.claude("-w-a", &id, &[claude_user("/w/a", "hello there")]);
    let sub = fx
        .claude_root()
        .join("projects/-w-a")
        .join(&id)
        .join("subagents/agent-1.jsonl");
    fx.write(&sub, &claude_user("/w/a", "inner"));
    fx.write(&fx.claude_root().join("projects/-w-a/notes.jsonl"), "{}\n");
    fx.write(&fx.claude_root().join("projects/-w-a/readme.txt"), "x");
    assert_eq!(ids(&all(&fx)), [id]);
}

#[test]
fn claude_symlinked_files_and_dirs_are_skipped() {
    let fx = Fx::new();
    fx.grant();
    let real = fx.claude("-w-a", &uid(1), &[claude_user("/w/a", "hi")]);
    let proj = fx.claude_root().join("projects/-w-a");
    std::os::unix::fs::symlink(&real, proj.join(format!("{}.jsonl", uid(2)))).unwrap();
    std::os::unix::fs::symlink(&proj, fx.claude_root().join("projects/-w-link")).unwrap();
    assert_eq!(ids(&all(&fx)), [uid(1)]);
}

#[test]
fn claude_first_message_skips_meta_sidechain_and_command_wrappers() {
    let fx = Fx::new();
    fx.grant();
    let lines = vec![
        json!({"type":"summary"}).to_string(),
        json!({"type":"user","isMeta":true,"message":{"content":"meta text"}}).to_string(),
        json!({"type":"user","isSidechain":true,"cwd":"/w/side","message":{"content":"side text"}}).to_string(),
        "{not json".to_string(),
        json!({"type":"user","message":{"content":"<command-name>/clear</command-name>"}}).to_string(),
        json!({"type":"user","message":{"content":[{"type":"text","text":"<system-reminder>x</system-reminder>"}]}}).to_string(),
        json!({"type":"user","message":{"content":"<local-command-stdout>ok</local-command-stdout>"}}).to_string(),
        json!({"type":"user","cwd":"/w/real","gitBranch":"feat","timestamp":"2026-11-05T10:00:00Z",
            "message":{"content":[{"type":"text","text":"fix the bug"},{"type":"text","text":"in parser"}]}}).to_string(),
        claude_user("/w/other", "later message"),
    ];
    fx.claude("-w-real", &uid(1), &lines);
    let e = &all(&fx).entries[0];
    assert_eq!(e.first_message.as_deref(), Some("fix the bug in parser"));
    assert_eq!(e.cwd.as_deref(), Some("/w/side"), "first cwd seen wins");
    assert_eq!(e.git_branch.as_deref(), Some("feat"));
    assert!(e.started_at.is_some() && e.readable);
}

#[test]
fn claude_head_is_capped_and_later_content_is_ignored() {
    let fx = Fx::new();
    fx.grant();
    let filler = json!({"type":"system","content":"x".repeat(1000)}).to_string();
    let mut lines: Vec<String> = (0..300).map(|_| filler.clone()).collect();
    lines.push(claude_user("/w/late", "far beyond the head"));
    let p = fx.claude("-w-big", &uid(1), &lines);
    assert!(std::fs::metadata(&p).unwrap().len() > HEAD_CAP);
    let e = &all(&fx).entries[0];
    assert_eq!(e.first_message, None);
    assert_eq!(e.cwd, None);
}

#[test]
fn claude_live_needs_an_alive_pid_and_a_matching_session() {
    let fx = Fx::new();
    fx.grant();
    let (a, b, c) = (uid(1), uid(2), uid(3));
    for id in [&a, &b, &c] {
        fx.claude("-w", id, &[claude_user("/w", "hi")]);
        age(
            &fx.claude_root().join(format!("projects/-w/{id}.jsonl")),
            3600,
        );
    }
    let sessions = fx.claude_root().join("sessions");
    let me = std::process::id();
    fx.write(
        &sessions.join(format!("{me}.json")),
        &json!({"pid":me,"sessionId":a,"cwd":"/w","startedAt":1,"name":"my\u{1b}[2J title"})
            .to_string(),
    );
    fx.write(
        &sessions.join("2147483000.json"),
        &json!({"pid":2147483000u32,"sessionId":b,"name":"dead"}).to_string(),
    );
    fx.write(
        &sessions.join("other.json"),
        &json!({"pid":me,"sessionId":uid(9),"name":"nobody"}).to_string(),
    );
    let l = all(&fx);
    let by = |id: &str| l.entries.iter().find(|e| e.id == id).unwrap();
    assert_eq!(by(&a).live, Live::Yes);
    assert_eq!(by(&a).title.as_deref(), Some("my?[2J title"));
    assert_eq!(by(&b).live, Live::No);
    assert_eq!(by(&c).live, Live::No);
}

#[test]
fn recent_mtime_without_proof_is_recent_and_old_is_not() {
    let fx = Fx::new();
    fx.grant();
    let fresh = fx.claude("-w", &uid(1), &[claude_user("/w", "a")]);
    let old = fx.claude("-w", &uid(2), &[claude_user("/w", "b")]);
    age(&old, 3600);
    let l = all(&fx);
    let by = |id: u64| l.entries.iter().find(|e| e.id == uid(id)).unwrap().live;
    assert_eq!(by(1), Live::Recent);
    assert_eq!(by(2), Live::No);
    let _ = fresh;
}

#[test]
fn entries_are_newest_first_and_limited() {
    let fx = Fx::new();
    fx.grant();
    for n in 1..=4 {
        let p = fx.claude("-w", &uid(n), &[claude_user("/w", "m")]);
        age(&p, 1000 * (5 - n));
    }
    let l = list(
        &fx.env,
        Filter {
            limit: Some(2),
            ..Filter::default()
        },
    );
    assert_eq!(ids(&l), [uid(4), uid(3)]);
}

#[test]
fn codex_lists_rollouts_and_drops_subagents_and_bad_ids() {
    let fx = Fx::new();
    fx.grant();
    for (n, src) in [
        (1, json!("cli")),
        (2, json!("exec")),
        (3, json!("vscode")),
        (4, json!({"subagent": {"parent": "x"}})),
    ] {
        fx.codex(
            &uid(n),
            &[codex_meta(&uid(n), "/w/c", src), codex_user("do it")],
        );
    }
    fx.codex(
        &uid(5),
        &[codex_meta("NOT-A-VALID-ID", "/w/c", json!("cli"))],
    );
    fx.write(
        &fx.codex_root()
            .join("sessions/2026/11/05/rollout-bad.jsonl"),
        "{}\n",
    );
    fx.write(&fx.codex_root().join("sessions/notes/x.jsonl"), "{}\n");
    let mut got = ids(&all(&fx));
    got.sort();
    assert_eq!(got, [uid(1), uid(2), uid(3)]);
}

#[test]
fn codex_fields_come_from_meta_user_message_and_the_thread_index() {
    let fx = Fx::new();
    fx.grant();
    let id = uid(7);
    fx.codex(
        &id,
        &[
            codex_meta(&id, "/w/cx", json!("cli")),
            codex_user("<environment_context>cwd</environment_context>"),
            codex_user("<user_instructions>be nice</user_instructions>"),
            json!({"type":"response_item","payload":{"type":"message"}}).to_string(),
            codex_user("refactor the parser"),
        ],
    );
    fx.write(
        &fx.codex_root().join("session_index.jsonl"),
        &format!(
            "{}\n{}\n",
            json!({"id": uid(9), "thread_name": "other"}),
            json!({"id": id, "thread_name": "Parser\u{1b}]0;x\u{7} work"})
        ),
    );
    let e = &all(&fx).entries[0];
    assert_eq!(e.cwd.as_deref(), Some("/w/cx"));
    assert_eq!(e.git_branch.as_deref(), Some("dev"));
    assert_eq!(e.first_message.as_deref(), Some("refactor the parser"));
    assert_eq!(e.title.as_deref(), Some("Parser?]0;x? work"));
    assert!(e.started_at.is_some());
}

#[test]
fn codex_head_with_a_long_first_line_is_still_read() {
    let fx = Fx::new();
    fx.grant();
    let id = uid(1);
    let mut meta: serde_json::Value =
        serde_json::from_str(&codex_meta(&id, "/w", json!("cli"))).unwrap();
    meta["payload"]["instructions"] = json!("i".repeat(23_000));
    fx.codex(&id, &[meta.to_string(), codex_user("after a long line")]);
    assert_eq!(
        all(&fx).entries[0].first_message.as_deref(),
        Some("after a long line")
    );
}

#[test]
fn codex_live_is_decided_by_the_lock_not_by_the_file() {
    let fx = Fx::new();
    fx.grant();
    let (held, free, absent) = (uid(1), uid(2), uid(3));
    for id in [&held, &free, &absent] {
        let p = fx.codex(id, &[codex_meta(id, "/w", json!("cli"))]);
        age(&p, 3600);
    }
    let locks = fx.codex_root().join("thread-writer-locks");
    let lock = fx.write(&locks.join(format!("{held}.lock")), "");
    fx.write(&locks.join(format!("{free}.lock")), "");
    let holder = std::fs::File::open(&lock).unwrap();
    assert_eq!(
        unsafe { libc::flock(holder.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) },
        0
    );
    let l = all(&fx);
    let by = |id: &str| l.entries.iter().find(|e| e.id == id).unwrap().live;
    assert_eq!(by(&held), Live::Yes);
    assert_eq!(by(&free), Live::No);
    assert_eq!(by(&absent), Live::No);
    drop(holder);
    let l = all(&fx);
    assert_eq!(
        l.entries.iter().find(|e| e.id == held).unwrap().live,
        Live::No
    );
}

fn seed_both(fx: &Fx) {
    fx.claude(
        "-w-a",
        &uid(1),
        &[claude_user("/w/a", "secret claude text")],
    );
    fx.codex(
        &uid(2),
        &[
            codex_meta(&uid(2), "/w/c", json!("cli")),
            codex_user("secret codex text"),
        ],
    );
}

#[test]
fn granted_reads_both_tools_without_a_hint() {
    let fx = Fx::new();
    fx.grant();
    seed_both(&fx);
    let l = all(&fx);
    assert_eq!(l.entries.len(), 2);
    assert!(l
        .entries
        .iter()
        .all(|e| e.readable && e.first_message.is_some()));
    assert!(l.hints.is_empty());
}

fn assert_metadata_only(fx: &Fx, hint: &str) {
    let l = all(fx);
    assert_eq!(l.entries.len(), 2);
    for e in &l.entries {
        assert!(!e.readable, "{e:?}");
        assert!(
            e.cwd.is_none()
                && e.first_message.is_none()
                && e.title.is_none()
                && e.started_at.is_none()
        );
        assert_eq!(e.live, Live::Recent);
    }
    let claude = l.entries.iter().find(|e| e.agent == Agent::Claude).unwrap();
    assert_eq!(claude.id, uid(1));
    assert_eq!(claude.project.as_deref(), Some("-w-a"));
    let codex = l.entries.iter().find(|e| e.agent == Agent::Codex).unwrap();
    assert_eq!(codex.id, uid(2));
    assert_eq!(l.hints, [hint.to_string()]);
}

#[test]
fn unset_and_denied_give_metadata_only_rows_and_the_existing_hint() {
    let hint = Reads::default().hint().unwrap();
    let fx = Fx::new();
    seed_both(&fx);
    assert_metadata_only(&fx, hint);
    fx.consent(Some(ReadLogs::Denied), None, None);
    assert_metadata_only(&fx, hint);
}

#[test]
fn metadata_only_listing_never_opens_the_files() {
    let fx = Fx::new();
    seed_both(&fx);
    for p in [
        fx.claude_root()
            .join(format!("projects/-w-a/{}.jsonl", uid(1))),
        fx.codex_root().join(format!(
            "sessions/2026/11/05/rollout-2026-11-05T10-00-00-{}.jsonl",
            uid(2)
        )),
    ] {
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    fx.write(
        &fx.claude_root().join("sessions/1.json"),
        &json!({"pid": std::process::id(), "sessionId": uid(1), "name": "n"}).to_string(),
    );
    let l = all(&fx);
    assert!(l.entries.iter().all(|e| !e.readable && e.title.is_none()));
    assert!(l.entries.iter().all(|e| e.live != Live::Yes));
}

#[test]
fn granted_with_one_tool_off_reads_only_the_other() {
    let fx = Fx::new();
    seed_both(&fx);
    fx.consent(Some(ReadLogs::Granted), None, Some(false));
    let l = all(&fx);
    let claude = l.entries.iter().find(|e| e.agent == Agent::Claude).unwrap();
    let codex = l.entries.iter().find(|e| e.agent == Agent::Codex).unwrap();
    assert!(claude.readable && claude.first_message.is_some());
    assert!(!codex.readable && codex.first_message.is_none());
    assert!(l.hints[0].starts_with("Codex titles"));
    let only_claude = list(
        &fx.env,
        Filter {
            agent: Some(Agent::Claude),
            ..Filter::default()
        },
    );
    assert!(only_claude.hints.is_empty());
}

#[test]
fn find_returns_an_exact_uuid_only() {
    let fx = Fx::new();
    fx.grant();
    seed_both(&fx);
    assert_eq!(find(&fx.env, &uid(2)).unwrap().agent, Agent::Codex);
    assert!(find(&fx.env, &uid(3)).is_none());
    assert!(find(&fx.env, "../../etc/passwd").is_none());
    assert!(find(&fx.env, "s_abcdefghij").is_none());
}

#[test]
fn clean_line_strips_escapes_bidi_newlines_and_truncates() {
    let s = "a\u{1b}]0;x\u{7}b\u{202e}c\r\nd\te";
    let c = clean_line(s, 100);
    assert_eq!(c, "a?]0;x?b?c d e");
    assert_eq!(clean_line(&"y".repeat(500), 10).chars().count(), 11);
    assert_eq!(clean_line("é".repeat(5).as_str(), 3), "ééé…");
}

#[test]
fn hostile_conversation_text_reaches_entries_clean() {
    let fx = Fx::new();
    fx.grant();
    let evil = "x\u{1b}]52;c;AAAA\u{7}y\u{202e}z\nw";
    fx.claude(
        "-w",
        &uid(1),
        &[
            json!({"type":"user","cwd":"/w","gitBranch":evil,"message":{"content":evil}})
                .to_string(),
        ],
    );
    fx.codex(
        &uid(2),
        &[codex_meta(&uid(2), "/w", json!("cli")), codex_user(evil)],
    );
    for e in all(&fx).entries {
        for f in [e.first_message, e.git_branch].into_iter().flatten() {
            assert!(
                f.chars().all(|c| !c.is_control()) && !f.contains('\u{202e}'),
                "{f:?}"
            );
        }
    }
}

#[test]
fn listing_and_indexing_never_modify_session_files() {
    let fx = Fx::new();
    fx.grant();
    seed_both(&fx);
    fx.write(&fx.claude_root().join("sessions/1.json"), "{}");
    fx.write(&fx.codex_root().join("session_index.jsonl"), "{}\n");
    let before = hash_tree(fx.home());
    let l = all(&fx);
    let ix = Index::open(&fx.tmp.path().join("cfg/sessions-index.sqlite")).unwrap();
    ix.update(&scan(&fx.env, None), |_, _| {}).unwrap();
    ix.search("secret", 10).unwrap();
    assert_eq!(l.entries.len(), 2);
    assert_eq!(hash_tree(fx.home()), before);
}

mod idx {
    use super::*;

    fn open(fx: &Fx) -> Index {
        Index::open(&fx.tmp.path().join("cfg/sessions-index.sqlite")).unwrap()
    }

    fn sync(fx: &Fx, ix: &Index) -> index::Outcome {
        ix.update(&scan(&fx.env, None), |_, _| {}).unwrap()
    }

    fn sids(ix: &Index, q: &str) -> Vec<String> {
        ix.search(q, 20)
            .unwrap()
            .into_iter()
            .map(|h| h.session_id)
            .collect()
    }

    fn count(ix: &Index, sql: &str) -> i64 {
        ix.conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn user_assistant_and_capped_tool_text_are_searchable() {
        let fx = Fx::new();
        fx.grant();
        let tool = json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t",
            "content":format!("error[E0308]: mismatched types {} tailmarker", "pad ".repeat(1500))}]}}).to_string();
        fx.claude(
            "-w",
            &uid(1),
            &[
                claude_user("/w", "Работаем над TM-845 в src/foo.rs"),
                claude_assistant("готово, смотри журнал"),
                tool,
                json!({"type":"assistant","message":{"content":[{"type":"thinking","thinking":"hiddenthought"}]}}).to_string(),
            ],
        );
        fx.codex(
            &uid(2),
            &[
                codex_meta(&uid(2), "/w", json!("cli")),
                codex_user("codexword question"),
                codex_agent("codexanswer here"),
                json!({"type":"response_item","payload":{"type":"function_call_output","output":"{\"output\":\"toolout42\"}"}}).to_string(),
                json!({"type":"response_item","payload":{"type":"reasoning","encrypted_content":"encsecret"}}).to_string(),
            ],
        );
        let ix = open(&fx);
        assert_eq!(sync(&fx, &ix).indexed, 2);
        assert_eq!(sids(&ix, "TM-845"), [uid(1)]);
        assert_eq!(sids(&ix, "src/foo.rs"), [uid(1)]);
        assert_eq!(sids(&ix, "работаем над"), [uid(1)]);
        assert_eq!(sids(&ix, "журн"), [uid(1)]);
        assert_eq!(sids(&ix, "error[E0308]: mismatched types"), [uid(1)]);
        assert!(
            sids(&ix, "tailmarker").is_empty(),
            "tool output is capped at 4 KiB"
        );
        assert!(sids(&ix, "hiddenthought").is_empty());
        assert_eq!(sids(&ix, "codexanswer"), [uid(2)]);
        assert_eq!(sids(&ix, "toolout42"), [uid(2)]);
        assert!(sids(&ix, "encsecret").is_empty());
        let hit = &ix.search("codexword", 5).unwrap()[0];
        assert!(hit.snippet.contains("codexword"));
    }

    #[test]
    fn no_raw_fts_syntax_passes_through() {
        assert_eq!(index::match_query("TM-845"), Some("\"TM-845\"*".into()));
        assert_eq!(
            index::match_query("a \"b c"),
            Some("\"a\" AND \"\"\"b\" AND \"c\"*".into())
        );
        assert_eq!(index::match_query("   "), None);
        assert_eq!(index::match_query("*"), None);
        assert_eq!(index::match_query(""), None);
        let fx = Fx::new();
        fx.grant();
        fx.claude("-w", &uid(1), &[claude_user("/w", "alpha beta gamma")]);
        let ix = open(&fx);
        sync(&fx, &ix);
        for q in [
            "alpha NEAR beta",
            "alpha OR zzz",
            "text:alpha",
            "session_id:alpha",
            "\"alpha",
            "alpha\"",
            "(alpha",
            "alpha -beta",
            "^alpha",
            "NEAR(alpha beta)",
            "*",
            "\"",
            "",
            "alpha AND",
        ] {
            let r = ix.search(q, 10);
            assert!(r.is_ok(), "{q:?}: {:?}", r.err());
        }
        assert!(sids(&ix, "alpha NEAR beta").is_empty());
        assert!(sids(&ix, "alpha OR zzz").is_empty());
        assert!(sids(&ix, "text:alpha").is_empty());
        assert!(sids(&ix, "").is_empty());
        assert!(sids(&ix, "*").is_empty());
        assert_eq!(sids(&ix, "alpha gam"), [uid(1)]);
        assert_eq!(
            sids(&ix, "beta alpha"),
            [uid(1)],
            "words are ANDed, order free"
        );
    }

    #[test]
    fn unchanged_files_are_skipped_and_appends_read_only_the_tail() {
        let fx = Fx::new();
        fx.grant();
        let p = fx.claude("-w", &uid(1), &[claude_user("/w", "first line text")]);
        let ix = open(&fx);
        assert_eq!(sync(&fx, &ix).indexed, 1);
        assert_eq!(
            sync(&fx, &ix),
            index::Outcome {
                skipped: 1,
                ..Default::default()
            }
        );
        let first_len = std::fs::metadata(&p).unwrap().len() as i64;
        assert_eq!(count(&ix, "SELECT indexed_bytes FROM files"), first_len);
        let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
        use std::io::Write;
        writeln!(f, "{}", claude_assistant("appended answer")).unwrap();
        write!(f, "{}", claude_assistant("half written")).unwrap();
        drop(f);
        assert_eq!(sync(&fx, &ix).indexed, 1);
        assert_eq!(sids(&ix, "appended"), [uid(1)]);
        assert!(
            sids(&ix, "half").is_empty(),
            "an incomplete last line waits"
        );
        assert_eq!(
            count(
                &ix,
                "SELECT count(*) FROM chunks WHERE chunks MATCH '\"first\"'"
            ),
            1
        );
        let mut f = std::fs::OpenOptions::new().append(true).open(&p).unwrap();
        writeln!(f, " now").unwrap();
        drop(f);
        sync(&fx, &ix);
        assert!(
            sids(&ix, "half").is_empty(),
            "the completed line is broken json"
        );
    }

    #[test]
    fn shrunk_or_replaced_files_are_reindexed() {
        let fx = Fx::new();
        fx.grant();
        let long = [
            claude_user("/w", "oldcontent one"),
            claude_assistant("oldcontent two"),
        ];
        let p = fx.claude("-w", &uid(1), &long);
        let ix = open(&fx);
        sync(&fx, &ix);
        assert_eq!(sids(&ix, "oldcontent"), [uid(1)]);
        fx.write(&p, &format!("{}\n", claude_user("/w", "brandnew")));
        sync(&fx, &ix);
        assert!(sids(&ix, "oldcontent").is_empty());
        assert_eq!(sids(&ix, "brandnew"), [uid(1)]);
        let bigger = format!(
            "{}\n{}\n",
            claude_user("/w", "replacedtext"),
            claude_assistant("p".repeat(400).as_str())
        );
        fx.write(&p, &bigger);
        sync(&fx, &ix);
        assert!(sids(&ix, "brandnew").is_empty());
        assert_eq!(sids(&ix, "replacedtext"), [uid(1)]);
    }

    #[test]
    fn a_session_stops_at_one_mebibyte_and_is_marked_truncated() {
        let fx = Fx::new();
        fx.grant();
        let mut lines = Vec::new();
        for i in 0..40 {
            lines.push(claude_user(
                "/w",
                &format!("chunk{i} {}", "word ".repeat(7000)),
            ));
        }
        lines.push(claude_user("/w", "zzzlast"));
        fx.claude("-w", &uid(1), &lines);
        let ix = open(&fx);
        sync(&fx, &ix);
        assert_eq!(count(&ix, "SELECT truncated FROM files"), 1);
        let bytes = count(&ix, "SELECT sum(length(text)) FROM chunks");
        assert!(bytes <= 1024 * 1024 && bytes > 900 * 1024, "{bytes}");
        assert!(sids(&ix, "zzzlast").is_empty());
        assert_eq!(sync(&fx, &ix).indexed, 0);
    }

    #[test]
    fn blobs_are_dropped_but_the_words_around_them_stay() {
        let fx = Fx::new();
        fx.grant();
        let blob = "QUFB".repeat(500);
        fx.claude(
            "-w",
            &uid(1),
            &[claude_user("/w", &format!("before {blob} after"))],
        );
        let ix = open(&fx);
        sync(&fx, &ix);
        assert_eq!(sids(&ix, "before after"), [uid(1)]);
        assert!(sids(&ix, "QUFBQUFB").is_empty());
    }

    #[test]
    fn codex_subagent_rollouts_are_not_indexed() {
        let fx = Fx::new();
        fx.grant();
        fx.codex(
            &uid(1),
            &[
                codex_meta(&uid(1), "/w", json!({"subagent": {"x": 1}})),
                codex_user("subagentword"),
            ],
        );
        let ix = open(&fx);
        sync(&fx, &ix);
        assert!(sids(&ix, "subagentword").is_empty());
    }

    #[test]
    fn vanished_files_are_removed_with_their_text() {
        let fx = Fx::new();
        fx.grant();
        let p = fx.claude("-w", &uid(1), &[claude_user("/w", "ephemeral")]);
        fx.claude("-w", &uid(2), &[claude_user("/w", "durable")]);
        let ix = open(&fx);
        sync(&fx, &ix);
        std::fs::remove_file(p).unwrap();
        assert_eq!(sync(&fx, &ix).removed, 1);
        assert!(sids(&ix, "ephemeral").is_empty());
        assert_eq!(sids(&ix, "durable"), [uid(2)]);
        assert_eq!(count(&ix, "SELECT count(*) FROM files"), 1);
    }

    #[test]
    fn files_of_a_tool_without_consent_are_not_indexed_or_kept() {
        let fx = Fx::new();
        seed_both(&fx);
        fx.consent(Some(ReadLogs::Granted), None, Some(false));
        let ix = open(&fx);
        sync(&fx, &ix);
        assert_eq!(sids(&ix, "secret"), [uid(1)]);
        fx.consent(None, None, Some(true));
        sync(&fx, &ix);
        assert_eq!(sids(&ix, "secret").len(), 2);
        ix.drop_agent(Agent::Codex).unwrap();
        assert_eq!(sids(&ix, "secret"), [uid(1)]);
        assert_eq!(
            count(&ix, "SELECT count(*) FROM files WHERE agent='codex'"),
            0
        );
    }

    #[test]
    fn dropping_the_index_removes_the_database_and_its_side_files() {
        let fx = Fx::new();
        fx.grant();
        seed_both(&fx);
        let path = fx.tmp.path().join("cfg/sessions-index.sqlite");
        let ix = Index::open(&path).unwrap();
        sync(&fx, &ix);
        assert!(path.exists());
        index::enforce_consent(&path, true, true);
        assert!(path.exists());
        drop(ix);
        index::enforce_consent(&path, false, false);
        for s in ["", "-wal", "-shm"] {
            assert!(
                !PathBuf::from(format!("{}{s}", path.display())).exists(),
                "{s}"
            );
        }
        index::enforce_consent(&path, false, false);
    }

    #[test]
    fn the_database_and_its_side_files_are_private_from_creation() {
        let fx = Fx::new();
        fx.grant();
        seed_both(&fx);
        let path = fx.tmp.path().join("cfg/sessions-index.sqlite");
        let ix = Index::open(&path).unwrap();
        sync(&fx, &ix);
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
        for s in ["-wal", "-shm"] {
            let side = PathBuf::from(format!("{}{s}", path.display()));
            assert!(side.exists(), "{s} should exist while open");
            assert_eq!(mode(&side), 0o600, "{s}");
        }
        assert_eq!(count(&ix, "PRAGMA secure_delete"), 1);
    }

    #[test]
    fn the_index_file_sits_in_a_sandbox_denied_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().canonicalize().unwrap();
        let path = index::index_path(&home.join(".config/tmgr"));
        let deny = tmgr_runner::sandbox::home_deny_paths(&home);
        assert!(deny.iter().any(|d| path.starts_with(d)), "{deny:?}");
    }

    #[test]
    fn a_busy_database_gives_up_the_run_without_failing() {
        let fx = Fx::new();
        fx.grant();
        seed_both(&fx);
        let path = fx.tmp.path().join("cfg/sessions-index.sqlite");
        let ix = Index::open(&path).unwrap();
        ix.conn.busy_timeout(Duration::from_millis(30)).unwrap();
        let other = Index::open(&path).unwrap();
        other.conn.execute_batch("BEGIN IMMEDIATE").unwrap();
        let out = sync(&fx, &ix);
        assert!(out.busy);
        assert_eq!(out.indexed, 0);
        other.conn.execute_batch("ROLLBACK").unwrap();
        assert!(!sync(&fx, &ix).busy);
        assert_eq!(sids(&ix, "secret").len(), 2);
    }

    #[test]
    fn a_partial_index_answers_and_progress_is_reported() {
        let fx = Fx::new();
        fx.grant();
        seed_both(&fx);
        let ix = open(&fx);
        let mut seen = Vec::new();
        let entries = scan(&fx.env, None);
        ix.update(&entries[..1], |d, t| seen.push((d, t))).unwrap();
        assert_eq!(seen, [(1, 1)]);
        assert_eq!(sids(&ix, "secret").len(), 1);
    }
}
