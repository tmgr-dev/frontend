use super::app::{CommentLine, Detail, Effect, Failure, Msg, Run, Snapshot};
use serde_json::Value;
use tmgr_core::api::Client;
use tmgr_core::error::{Error, Kind};
use tmgr_core::models::Status;
use tmgr_core::text::{sanitize_line, truncate_chars};

const PER_PAGE: u64 = 50;
const PAGES_PER_STATUS: u64 = 2;
const COMMENTS_SHOWN: usize = 5;
const COMMENT_CHARS: usize = 500;
const RUNS_SHOWN: usize = 5;
const HIDDEN_KINDS: [&str; 2] = ["hidden", "archived"];

fn failure(e: Error) -> Failure {
    Failure::new(e.kind, &e.message)
}

pub fn fetch(client: &Client, ws_id: i64, category: Option<u64>) -> Result<Snapshot, Error> {
    let statuses = client.statuses(ws_id)?;
    let mut tasks = Vec::new();
    let mut more = false;
    for status in statuses
        .iter()
        .filter(|s| !HIDDEN_KINDS.contains(&s.kind.as_str()))
    {
        let (mut got, truncated) = fetch_status(client, ws_id, category, status)?;
        tasks.append(&mut got);
        more |= truncated;
    }
    Ok(Snapshot {
        statuses,
        tasks,
        more,
    })
}

fn fetch_status(
    client: &Client,
    ws_id: i64,
    category: Option<u64>,
    status: &Status,
) -> Result<(Vec<tmgr_core::models::ApiTask>, bool), Error> {
    let mut out = Vec::new();
    for page in 1..=PAGES_PER_STATUS {
        let res = client.tasks_page(ws_id, category, Some(status.id), page, PER_PAGE)?;
        let last = res.current_page >= res.last_page;
        out.extend(res.items);
        if last {
            return Ok((out, false));
        }
    }
    Ok((out, true))
}

fn str_of(v: &Value, keys: &[&str]) -> String {
    keys.iter()
        .find_map(|k| v.get(*k).and_then(Value::as_str))
        .map(sanitize_line)
        .unwrap_or_default()
}

fn int_of(v: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|k| v.get(*k).and_then(Value::as_i64))
}

pub fn parse_runs(body: &Value) -> Vec<Run> {
    let list = body
        .get("runs")
        .and_then(Value::as_array)
        .or_else(|| body.as_array());
    list.map(|runs| {
        runs.iter()
            .take(RUNS_SHOWN)
            .map(|r| {
                let tests = r.get("tests");
                Run {
                    agent: str_of(r, &["agent"]),
                    model: str_of(r, &["model"]),
                    status: str_of(r, &["status"]),
                    seconds: int_of(r, &["duration_seconds", "durationSeconds"]).unwrap_or(0),
                    branch: str_of(r, &["branch"]),
                    pr: str_of(r, &["pr_url", "prUrl"]),
                    passed: tests.and_then(|t| int_of(t, &["passed"])),
                    failed: tests.and_then(|t| int_of(t, &["failed"])),
                }
            })
            .collect()
    })
    .unwrap_or_default()
}

pub fn load_detail(client: &Client, id: u64) -> Result<Detail, Failure> {
    let task = match client.task_by_id(id) {
        Ok(t) => Some(t),
        Err(e) if matches!(e.kind, Kind::Forbidden | Kind::NotFound) => None,
        Err(e) => return Err(failure(e)),
    };
    let comments = match client.comments(id) {
        Ok(mut list) => {
            let keep = list.split_off(list.len().saturating_sub(COMMENTS_SHOWN));
            keep.iter()
                .map(|c| CommentLine {
                    author: sanitize_line(
                        c.user
                            .as_ref()
                            .and_then(|u| u.name.as_deref())
                            .unwrap_or("unknown"),
                    ),
                    text: sanitize_line(&truncate_chars(&c.message, COMMENT_CHARS).0),
                })
                .collect()
        }
        Err(e) if matches!(e.kind, Kind::Forbidden | Kind::NotFound) => Vec::new(),
        Err(e) => return Err(failure(e)),
    };
    let (runs, runs_available) = match client.agent_work(id) {
        Ok(body) => (parse_runs(&body), true),
        Err(e) if matches!(e.kind, Kind::Forbidden | Kind::NotFound) => (Vec::new(), false),
        Err(e) => return Err(failure(e)),
    };
    Ok(Detail {
        description: task
            .and_then(|t| t.description)
            .filter(|d| !d.trim().is_empty())
            .map(|d| super::app::plain_text(&d)),
        comments,
        runs,
        runs_available,
        error: None,
    })
}

pub fn exec(client: &Client, ws_id: i64, category: Option<u64>, effect: Effect) -> Msg {
    match effect {
        Effect::Refresh => Msg::Loaded(fetch(client, ws_id, category).map_err(failure)),
        Effect::LoadDetail(id) => Msg::Detail(id, Box::new(load_detail(client, id))),
        Effect::Move { id, to } => {
            Msg::MoveDone(id, client.set_task_status(id, to).map_err(failure))
        }
        Effect::Timer { id, start } => {
            let res = if start {
                client.start_countdown(id)
            } else {
                client.stop_countdown(id)
            };
            Msg::TimerDone(id, start, res.map(|_| ()).map_err(failure))
        }
        Effect::Comment { id, text } => {
            Msg::CommentDone(id, client.add_comment(id, &text).map_err(failure))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tmgr_core::creds::Tokens;
    use tmgr_core::redact::Secret;
    use wiremock::matchers::{body_json, header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn client(uri: &str) -> Client {
        let tokens = Tokens {
            access: Secret::new("tok-1"),
            refresh: None,
            expires_at: None,
        };
        Client::new(uri, "tmgr-test", Some(tokens), None, false).unwrap()
    }

    fn page(items: Vec<Value>, current: u64, last: u64) -> Value {
        json!({"data": items, "meta": {"current_page": current, "last_page": last}})
    }

    async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
        tokio::task::spawn_blocking(f).await.unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_asks_only_for_visible_statuses_and_flags_long_columns() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/workspaces/5/statuses"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [
                {"id": 1, "name": "Backlog", "type": "default"},
                {"id": 2, "name": "Done", "type": "completed"},
                {"id": 9, "name": "Parked", "type": "hidden"},
                {"id": 10, "name": "Archive", "type": "archived"}
            ]})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/tasks"))
            .and(query_param("status_id", "1"))
            .and(query_param("workspace_id", "5"))
            .respond_with(ResponseTemplate::new(200).set_body_json(page(
                vec![json!({"id": 1, "title": "TM-1: a", "status_id": 1})],
                1,
                1,
            )))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/tasks"))
            .and(query_param("status_id", "2"))
            .respond_with(ResponseTemplate::new(200).set_body_json(page(
                vec![json!({"id": 2, "title": "TM-2: b", "status_id": 2})],
                1,
                9,
            )))
            .mount(&server)
            .await;
        let uri = server.uri();
        let snap = blocking(move || fetch(&client(&uri), 5, None).unwrap()).await;
        assert_eq!(snap.tasks.len(), 3);
        assert!(snap.more);
        let asked: Vec<String> = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|r| r.url.query().unwrap_or("").to_string())
            .collect();
        assert!(!asked
            .iter()
            .any(|q| q.contains("status_id=9") || q.contains("status_id=10")));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn actions_call_the_wave_one_routes() {
        let server = MockServer::start().await;
        for (m, p) in [
            ("PUT", "/api/tasks/7/3"),
            ("POST", "/api/tasks/7/countdown"),
            ("DELETE", "/api/tasks/7/countdown"),
        ] {
            Mock::given(method(m))
                .and(path(p))
                .and(header("authorization", "Bearer tok-1"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_json(json!({"data": {"id": 7, "title": "T"}})),
                )
                .expect(1)
                .mount(&server)
                .await;
        }
        Mock::given(method("POST"))
            .and(path("/api/tasks/7/comments"))
            .and(body_json(json!({"message": "hi"})))
            .respond_with(ResponseTemplate::new(201).set_body_json(json!({"data": {"id": 1}})))
            .expect(1)
            .mount(&server)
            .await;
        let uri = server.uri();
        let msgs = blocking(move || {
            let c = client(&uri);
            vec![
                exec(&c, 5, None, Effect::Move { id: 7, to: 3 }),
                exec(&c, 5, None, Effect::Timer { id: 7, start: true }),
                exec(
                    &c,
                    5,
                    None,
                    Effect::Timer {
                        id: 7,
                        start: false,
                    },
                ),
                exec(
                    &c,
                    5,
                    None,
                    Effect::Comment {
                        id: 7,
                        text: "hi".into(),
                    },
                ),
            ]
        })
        .await;
        assert!(matches!(msgs[0], Msg::MoveDone(7, Ok(()))));
        assert!(matches!(msgs[1], Msg::TimerDone(7, true, Ok(()))));
        assert!(matches!(msgs[2], Msg::TimerDone(7, false, Ok(()))));
        assert!(matches!(msgs[3], Msg::CommentDone(7, Ok(()))));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_rejected_move_comes_back_as_a_failure() {
        let server = MockServer::start().await;
        Mock::given(method("PUT"))
            .and(path("/api/tasks/7/3"))
            .respond_with(
                ResponseTemplate::new(403).set_body_json(json!({"error": "insufficient_scope"})),
            )
            .mount(&server)
            .await;
        let uri = server.uri();
        let msg =
            blocking(move || exec(&client(&uri), 5, None, Effect::Move { id: 7, to: 3 })).await;
        match msg {
            Msg::MoveDone(7, Err(f)) => assert_eq!(f.kind, Kind::Forbidden),
            _ => panic!("expected a failed move"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn detail_keeps_the_last_five_comments_and_tolerates_disabled_agent_work() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/tasks/7"))
            .respond_with(ResponseTemplate::new(200).set_body_json(
                json!({"data": {"id": 7, "title": "TM-7: x", "description": "# Head\n**bold** text"}}),
            ))
            .mount(&server)
            .await;
        let comments: Vec<Value> = (1..=8)
            .map(|i| json!({"id": i, "message": format!("c{i}\u{1b}[31m"), "user": {"id": 1, "name": "Ann"}}))
            .collect();
        Mock::given(method("GET"))
            .and(path("/api/tasks/7/comments"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": comments})))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/tasks/7/agent-work"))
            .respond_with(ResponseTemplate::new(403).set_body_json(json!({"error": "disabled"})))
            .mount(&server)
            .await;
        let uri = server.uri();
        let d = blocking(move || load_detail(&client(&uri), 7).unwrap()).await;
        assert_eq!(d.description.as_deref(), Some("Head\nbold text"));
        let texts: Vec<_> = d.comments.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(
            texts,
            ["c4 [31m", "c5 [31m", "c6 [31m", "c7 [31m", "c8 [31m"]
        );
        assert!(!d.runs_available);
    }

    #[test]
    fn agent_runs_parse_snake_and_camel_case() {
        let body = json!({"runs": [
            {"agent": "claude-code", "model": "opus", "status": "succeeded", "duration_seconds": 90,
             "branch": "b", "pr_url": "https://x/pr/1", "tests": {"passed": 3, "failed": 1}},
            {"agent": "codex\u{1b}", "durationSeconds": 5, "prUrl": "u"}
        ], "totals": {}});
        let runs = parse_runs(&body);
        assert_eq!(runs[0].seconds, 90);
        assert_eq!((runs[0].passed, runs[0].failed), (Some(3), Some(1)));
        assert_eq!(runs[0].pr, "https://x/pr/1");
        assert_eq!(
            (runs[1].agent.as_str(), runs[1].seconds, runs[1].pr.as_str()),
            ("codex ", 5, "u")
        );
    }
}
