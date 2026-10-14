mod common;

use assert_cmd::Command;
use chrono::{Duration, NaiveDate, Utc};
use common::{assert_valid, dead_server, stdout_json, text};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tmgr_usage::limits::{LimitSnapshot, LimitSource, LimitWindow};
use tmgr_usage::{PriceBook, Store, Tool};

const PRICES: &str = r#"{
  "claude-test-opus-1": {"mode": "chat", "input_cost_per_token": 1e-5, "output_cost_per_token": 5e-5,
    "cache_read_input_token_cost": 1e-6, "cache_creation_input_token_cost": 1.25e-5},
  "claude-test-sonnet-1": {"mode": "chat", "input_cost_per_token": 3e-6, "output_cost_per_token": 1.5e-5,
    "cache_read_input_token_cost": 3e-7, "cache_creation_input_token_cost": 3.75e-6}
}"#;

const SECRETS: [&str; 4] = [
    "SECRET-PROMPT-1",
    "SECRET-RESPONSE-1",
    "SECRET-TOOL-1",
    "SECRET-STATUSLINE-1",
];

struct U {
    tmp: TempDir,
    server: String,
}

fn today() -> NaiveDate {
    Utc::now().date_naive()
}

fn at(offset_days: i64, clock: &str) -> String {
    format!("{}T{clock}Z", today() + Duration::days(offset_days))
}

struct Ev<'a> {
    session: &'a str,
    ts: String,
    msg: &'a str,
    model: &'a str,
    input: u64,
    output: u64,
    branch: Option<&'a str>,
    cwd: &'a str,
    sidechain: bool,
}

impl<'a> Ev<'a> {
    fn new(
        session: &'a str,
        ts: String,
        msg: &'a str,
        model: &'a str,
        input: u64,
        output: u64,
    ) -> Self {
        Ev {
            session,
            ts,
            msg,
            model,
            input,
            output,
            branch: Some("main"),
            cwd: "/work/alpha",
            sidechain: false,
        }
    }

    fn branch(mut self, b: Option<&'a str>) -> Self {
        self.branch = b;
        self
    }

    fn cwd(mut self, c: &'a str) -> Self {
        self.cwd = c;
        self
    }

    fn side(mut self) -> Self {
        self.sidechain = true;
        self
    }

    fn line(&self) -> String {
        let branch = self
            .branch
            .map_or(String::new(), |b| format!(r#","gitBranch":"{b}""#));
        format!(
            r#"{{"type":"assistant","sessionId":"{s}","timestamp":"{ts}","cwd":"{cwd}"{branch},"isSidechain":{side},"requestId":"req-{m}","message":{{"id":"{m}","model":"{model}","content":[{{"type":"text","text":"SECRET-RESPONSE-1"}},{{"type":"tool_use","input":{{"command":"SECRET-TOOL-1"}}}}],"usage":{{"input_tokens":{i},"output_tokens":{o},"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}}}}"#,
            s = self.session,
            ts = self.ts,
            cwd = self.cwd,
            side = self.sidechain,
            m = self.msg,
            model = self.model,
            i = self.input,
            o = self.output,
        )
    }
}

fn user_line(session: &str, ts: &str) -> String {
    format!(
        r#"{{"type":"user","sessionId":"{session}","timestamp":"{ts}","cwd":"/work/alpha","message":{{"role":"user","content":"SECRET-PROMPT-1"}}}}"#
    )
}

impl U {
    fn new() -> U {
        Self::with_server(dead_server())
    }

    fn with_server(server: String) -> U {
        let u = U {
            tmp: tempfile::tempdir().unwrap(),
            server,
        };
        for d in [
            "home",
            "claude",
            "codex",
            "data",
            "xdg",
            "state",
            "xdgdata",
            "tmgr-home/workspaces",
        ] {
            std::fs::create_dir_all(u.tmp.path().join(d)).unwrap();
        }
        #[cfg(unix)]
        for (link, target) in [(".claude", "../claude"), (".codex", "../codex")] {
            std::os::unix::fs::symlink(target, u.p("home").join(link)).unwrap();
        }
        u
    }

    fn p(&self, rel: &str) -> PathBuf {
        self.tmp.path().join(rel)
    }

    fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("tmgr").unwrap();
        c.current_dir(self.p("home"))
            .env_clear()
            .env("HOME", self.p("home"))
            .env("XDG_DATA_HOME", self.p("xdgdata"))
            .env("XDG_CONFIG_HOME", self.p("xdg"))
            .env("XDG_STATE_HOME", self.p("state"))
            .env("TMGR_USAGE_DIR", self.p("data"))
            .env("CLAUDE_CONFIG_DIR", self.p("claude"))
            .env("CODEX_HOME", self.p("codex"))
            .env("TMGR_LOCAL_ROOT", self.p("tmgr-home/workspaces"))
            .env("TMGR_CREDENTIAL_STORE", "file")
            .env("TMGR_SERVER", &self.server)
            .env("TZ", "UTC")
            .env("NO_COLOR", "1");
        c
    }

    fn run(&self, args: &[&str]) -> std::process::Output {
        self.cmd().args(args).output().unwrap()
    }

    fn ok(&self, args: &[&str]) -> std::process::Output {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "{args:?} exited {:?}: {}",
            out.status.code(),
            text(&out.stderr)
        );
        out
    }

    fn json(&self, args: &[&str]) -> Value {
        let mut all = vec!["--json"];
        all.extend_from_slice(args);
        stdout_json(&self.ok(&all))
    }

    fn session_file(&self, rel: &str, lines: &[String]) {
        let path = self.p("claude/projects").join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, format!("{}\n", lines.join("\n"))).unwrap();
    }

    fn seed_prices(&self) {
        let store = Store::open(&self.p("data/usage.db")).unwrap();
        let book = PriceBook::from_litellm_json(PRICES.as_bytes()).unwrap();
        store.save_prices(&book, "test", Utc::now()).unwrap();
    }

    fn standard_fixture(&self) {
        self.seed_prices();
        self.session_file(
            "-work-alpha/sess-a.jsonl",
            &[
                user_line("sess-a", &at(0, "11:59:00")),
                Ev::new(
                    "sess-a",
                    at(0, "12:00:00"),
                    "m1",
                    "claude-test-opus-1",
                    1000,
                    100,
                )
                .line(),
                Ev::new(
                    "sess-a",
                    at(0, "12:05:00"),
                    "m2",
                    "claude-test-opus-1",
                    2000,
                    200,
                )
                .line(),
            ],
        );
        self.session_file(
            "-work-alpha/sess-a/subagents/agent-1.jsonl",
            &[Ev::new(
                "sess-a",
                at(0, "12:10:00"),
                "m3",
                "claude-test-sonnet-1",
                1000,
                0,
            )
            .side()
            .line()],
        );
        self.session_file(
            "-work-beta/sess-b.jsonl",
            &[Ev::new(
                "sess-b",
                at(-1, "10:00:00"),
                "m4",
                "claude-test-sonnet-1",
                1000,
                1000,
            )
            .branch(Some("feat-x"))
            .cwd("/work/beta")
            .line()],
        );
        self.session_file(
            "-work-gamma/sess-c.jsonl",
            &[
                Ev::new("sess-c", at(0, "12:20:00"), "m5", "mystery-model-9", 10, 10)
                    .cwd("/work/gamma")
                    .line(),
            ],
        );
    }
}

fn data_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            data_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn assert_no_secrets(u: &U, outputs: &[&std::process::Output]) {
    for out in outputs {
        let all = format!("{}{}", text(&out.stdout), text(&out.stderr));
        for (i, s) in SECRETS.iter().enumerate() {
            assert!(!all.contains(s), "secret #{i} reached the output");
        }
    }
    let mut files = Vec::new();
    data_files(&u.p("data"), &mut files);
    for f in files {
        let bytes = std::fs::read(&f).unwrap();
        for (i, s) in SECRETS.iter().enumerate() {
            assert!(
                !bytes.windows(s.len()).any(|w| w == s.as_bytes()),
                "secret #{i} reached a data file"
            );
        }
    }
}

fn find<'a>(rows: &'a Value, dim: &str, value: &str) -> &'a Value {
    rows.as_array()
        .unwrap()
        .iter()
        .find(|r| r["group"][dim] == value)
        .unwrap_or_else(|| panic!("no row for {dim}={value}"))
}

fn cost(v: &Value) -> f64 {
    v["cost_usd"].as_f64().unwrap()
}

fn assert_close(a: f64, b: f64) {
    assert!((a - b).abs() < 1e-6, "{a} vs {b}");
}

#[test]
fn the_default_summary_lists_today_week_and_month_per_tool_and_model() {
    let u = U::new();
    u.standard_fixture();
    let out = u.ok(&["usage"]);
    let expected = "\
AI usage: cost is API-equivalent (list prices), not your subscription bill

Tool    Model                 Today (API-eq.)  7 days (API-eq.)  30 days (API-eq.)
claude  claude-test-opus-1    $0.04            $0.04             $0.04
claude  claude-test-sonnet-1  $0.0030          $0.02             $0.02
claude  mystery-model-9       $0.00            $0.00             $0.00
Total                         $0.05            $0.07             $0.07

Last 30 days tokens: input 5010, output 1310, cache read 0, cache write 0";
    assert_eq!(
        text(&out.stdout).trim_end(),
        expected,
        "{}",
        text(&out.stdout)
    );
    assert!(text(&out.stderr).contains("1 events use models without a known price"));
}

#[test]
fn a_single_period_groups_by_the_requested_dimension() {
    let u = U::new();
    u.standard_fixture();
    let out = u.ok(&["usage", "--today", "--by", "model"]);
    let t = text(&out.stdout);
    assert!(
        t.starts_with("AI usage, today: cost is API-equivalent"),
        "{t}"
    );
    assert!(t.contains("claude-test-opus-1    $0.04"), "{t}");
    assert!(t.contains("Total"), "{t}");

    let v = u.json(&["usage", "--today", "--by", "project"]);
    assert_valid("usage_summary", &v);
    let rows = &v["data"]["windows"][0]["rows"];
    let hash = tmgr_usage::model::project_hash("/work/alpha");
    let alpha = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["group"]["project"]["name"] == "alpha")
        .unwrap();
    assert_eq!(alpha["group"]["project"]["hash"], hash);
    assert_close(cost(alpha), 0.048);
}

#[test]
fn every_dimension_and_filter_works() {
    let u = U::new();
    u.standard_fixture();
    for by in ["tool", "model", "account", "project", "branch", "day"] {
        let v = u.json(&["usage", "--month", "--by", by]);
        assert_valid("usage_summary", &v);
        assert_eq!(v["data"]["group_by"], json!([by]));
    }
    let v = u.json(&["usage", "--month", "--by", "day"]);
    let rows = &v["data"]["windows"][0]["rows"];
    assert_close(cost(find(rows, "day", &today().to_string())), 0.048);
    assert_close(
        cost(find(
            rows,
            "day",
            &(today() - Duration::days(1)).to_string(),
        )),
        0.018,
    );
    let v = u.json(&["usage", "--month", "--by", "branch"]);
    assert_close(
        cost(find(&v["data"]["windows"][0]["rows"], "branch", "feat-x")),
        0.018,
    );

    let codex = u.json(&["usage", "--month", "--tool", "codex"]);
    assert_eq!(codex["data"]["windows"][0]["totals"]["events"], 0);
    let claude = u.json(&[
        "usage",
        "--month",
        "--tool",
        "claude",
        "--account",
        "claude",
    ]);
    assert_eq!(claude["data"]["windows"][0]["totals"]["events"], 5);
    let other = u.json(&["usage", "--month", "--account", "nobody"]);
    assert_eq!(other["data"]["windows"][0]["totals"]["events"], 0);
}

#[test]
fn explicit_date_ranges_are_inclusive() {
    let u = U::new();
    u.standard_fixture();
    let y = (today() - Duration::days(1)).to_string();
    let v = u.json(&["usage", "--since", &y, "--until", &y]);
    assert_close(cost(&v["data"]["windows"][0]["totals"]), 0.018);
    let v = u.json(&["usage", "--since", &y]);
    assert_close(cost(&v["data"]["windows"][0]["totals"]), 0.066);
    let v = u.json(&["usage", "--yesterday"]);
    assert_close(cost(&v["data"]["windows"][0]["totals"]), 0.018);
    let v = u.json(&["usage", "--week"]);
    assert_eq!(v["data"]["windows"][0]["key"], "week");
}

#[test]
fn bad_flags_exit_two() {
    let u = U::new();
    for args in [
        vec!["usage", "--by", "colour"],
        vec!["usage", "--tool", "gemini"],
        vec!["usage", "--since", "yesterday"],
        vec!["usage", "--until", "2026-01-01"],
        vec!["usage", "--today", "--week"],
        vec!["usage", "--today", "--since", "2026-01-01"],
        vec!["usage", "--since", "2026-02-01", "--until", "2026-01-01"],
        vec!["usage", "--task", "nonsense"],
        vec!["usage", "--by", "task", "--task", "TM-1"],
        vec!["usage", "limits", "--bogus"],
    ] {
        let out = u.run(&args);
        assert_eq!(
            out.status.code(),
            Some(2),
            "{args:?}: {}",
            text(&out.stderr)
        );
    }
}

#[test]
fn json_summary_follows_the_schema_and_leaks_no_paths() {
    let u = U::new();
    u.standard_fixture();
    let out = u.ok(&["--json", "usage"]);
    let v = stdout_json(&out);
    assert_valid("usage_summary", &v);
    assert_eq!(v["data"]["basis"], "api_equivalent");
    assert_eq!(v["data"]["windows"].as_array().unwrap().len(), 3);
    assert_eq!(v["data"]["scan"]["errors"], 0);
    let all = format!("{}{}", text(&out.stdout), text(&out.stderr));
    for needle in ["/work/", u.tmp.path().to_str().unwrap()] {
        assert!(!all.contains(needle), "a path reached the output");
    }
    let warnings = v["warnings"].as_array().unwrap();
    assert!(warnings.iter().any(|w| w["code"] == "usage_unpriced"));
}

#[test]
fn scanning_is_incremental_and_no_scan_reads_only_the_database() {
    let u = U::new();
    u.standard_fixture();
    let first = u.json(&["usage", "scan"]);
    assert_eq!(first["data"]["events_new"], 5);
    let second = u.json(&["usage", "scan"]);
    assert_eq!(second["data"]["events_new"], 0);

    let file = u.p("claude/projects/-work-alpha/sess-a.jsonl");
    let mut content = std::fs::read_to_string(&file).unwrap();
    content.push_str(&format!(
        "{}\n",
        Ev::new(
            "sess-a",
            at(0, "12:30:00"),
            "m9",
            "claude-test-opus-1",
            100,
            10
        )
        .line()
    ));
    std::fs::write(&file, content).unwrap();
    let skipped = u.json(&["usage", "--no-scan", "--today"]);
    assert_eq!(skipped["data"]["windows"][0]["totals"]["events"], 4);
    assert_eq!(skipped["data"]["scan"], Value::Null);
    let third = u.json(&["usage", "scan"]);
    assert_eq!(third["data"]["events_new"], 1);
}

#[test]
fn an_empty_machine_says_so() {
    let u = U::new();
    let out = u.ok(&["usage", "--today"]);
    assert!(
        text(&out.stdout).contains("no usage recorded"),
        "{}",
        text(&out.stdout)
    );
    let v = u.json(&["usage"]);
    assert_valid("usage_summary", &v);
    assert_eq!(v["data"]["windows"][0]["rows"], json!([]));
}

#[test]
fn limits_show_stored_windows_and_say_so_when_there_are_none() {
    let u = U::new();
    let empty = u.ok(&["usage", "limits", "--no-scan"]);
    assert!(text(&empty.stdout).contains("No limit data yet"));
    let v = u.json(&["usage", "limits", "--no-scan"]);
    assert_valid("usage_limits", &v);
    assert_eq!(v["data"]["limits"], json!([]));

    let store = Store::open(&u.p("data/usage.db")).unwrap();
    let now = Utc::now();
    for (window, pct, hours) in [
        (LimitWindow::Session, 42.0, 3),
        (LimitWindow::Weekly, 80.0, 90),
    ] {
        store
            .record_limit(&LimitSnapshot {
                tool: Tool::Claude,
                account: "claude".into(),
                window,
                used_pct: pct,
                resets_at: Some(now + Duration::hours(hours)),
                plan: None,
                observed_at: now - Duration::minutes(5),
                source: LimitSource::Statusline,
                tool_session_id: "sess-a".into(),
            })
            .unwrap();
    }
    drop(store);
    let out = u.ok(&["usage", "limits", "--no-scan"]);
    let t = text(&out.stdout);
    assert!(t.contains("claude  claude   session  42%"), "{t}");
    assert!(t.contains("weekly   80%"), "{t}");
    assert!(t.contains("statusline"), "{t}");
    assert!(t.contains("5m ago"), "{t}");
    let v = u.json(&["usage", "limits", "--no-scan"]);
    assert_valid("usage_limits", &v);
    assert_eq!(v["data"]["limits"].as_array().unwrap().len(), 2);
}

#[test]
fn accounts_are_added_listed_and_removed() {
    let u = U::new();
    let extra = u.p("claude-work");
    std::fs::create_dir_all(&extra).unwrap();
    let v = u.json(&["usage", "accounts"]);
    assert_valid("usage_accounts", &v);
    let labels: Vec<_> = v["data"]["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["label"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(labels, ["claude", "codex"]);

    let added = u.json(&[
        "usage",
        "accounts",
        "add",
        "--tool",
        "claude",
        "--root",
        extra.to_str().unwrap(),
        "--label",
        "work",
    ]);
    assert_valid("usage_accounts", &added);
    let dup = u.run(&[
        "usage",
        "accounts",
        "add",
        "--tool",
        "claude",
        "--root",
        extra.to_str().unwrap(),
        "--label",
        "work",
    ]);
    assert_eq!(dup.status.code(), Some(2));
    let missing = u.run(&[
        "usage",
        "accounts",
        "add",
        "--tool",
        "claude",
        "--root",
        "/definitely/not/here",
        "--label",
        "x",
    ]);
    assert_eq!(missing.status.code(), Some(2));
    let list = u.ok(&["usage", "accounts", "list"]);
    assert!(text(&list.stdout).contains("work"));

    u.session_file("-w/s.jsonl", &[]);
    let path = extra.join("projects/-w/sw.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    u.seed_prices();
    std::fs::write(
        &path,
        format!(
            "{}\n",
            Ev::new("sw", at(0, "09:00:00"), "w1", "claude-test-opus-1", 100, 10).line()
        ),
    )
    .unwrap();
    let by = u.json(&["usage", "--today", "--by", "account"]);
    assert_eq!(
        find(&by["data"]["windows"][0]["rows"], "account", "work")["events"],
        1
    );

    let removed = u.json(&["usage", "accounts", "remove", "work"]);
    assert_valid("usage_accounts", &removed);
    assert_eq!(
        u.run(&["usage", "accounts", "remove", "work"])
            .status
            .code(),
        Some(3)
    );
    assert_eq!(
        u.run(&["usage", "accounts", "remove", "claude"])
            .status
            .code(),
        Some(3)
    );
}

#[test]
fn account_paths_are_shown_relative_to_home_or_as_external() {
    let u = U::new();
    let inside = u.p("home/alt-claude");
    let outside = u.p("elsewhere/ext-claude");
    std::fs::create_dir_all(&inside).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    let mut outputs = Vec::new();
    for (root, label) in [(&inside, "inside"), (&outside, "outside")] {
        outputs.push(u.ok(&[
            "usage",
            "accounts",
            "add",
            "--tool",
            "claude",
            "--root",
            root.to_str().unwrap(),
            "--label",
            label,
        ]));
    }
    let v = u.json(&["usage", "accounts", "list"]);
    let root_of = |label: &str| {
        v["data"]["accounts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["label"] == label)
            .unwrap()["root"]
            .as_str()
            .unwrap()
            .to_string()
    };
    assert_eq!(root_of("inside"), "~/alt-claude");
    assert_eq!(root_of("outside"), "<external>/ext-claude");
    assert_eq!(root_of("claude"), "~/.claude");
    outputs.push(u.ok(&["usage", "accounts", "list"]));
    outputs.push(u.ok(&["--json", "usage", "accounts", "list"]));
    let tmp = u.tmp.path().to_str().unwrap().to_string();
    let canon = std::fs::canonicalize(u.tmp.path()).unwrap();
    for out in &outputs {
        let all = format!("{}{}", text(&out.stdout), text(&out.stderr));
        assert!(!all.contains(&tmp) && !all.contains(canon.to_str().unwrap()));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn pricing_updates_from_a_public_list_without_credentials_and_reprices() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/prices.json"))
        .respond_with(ResponseTemplate::new(200).set_body_string(PRICES))
        .mount(&server)
        .await;
    let u = U::new();
    u.session_file(
        "-work-alpha/p.jsonl",
        &[Ev::new(
            "p",
            at(0, "12:00:00"),
            "p1",
            "claude-test-opus-1",
            1000,
            100,
        )
        .line()],
    );
    let before = u.json(&["usage", "--today"]);
    assert_eq!(before["data"]["windows"][0]["totals"]["unpriced_events"], 1);
    assert_close(cost(&before["data"]["windows"][0]["totals"]), 0.0);
    let shown = u.ok(&["usage", "pricing", "show"]);
    assert!(text(&shown.stdout).contains("bundled snapshot"));

    let url = format!("{}/prices.json", server.uri());
    let upd = u.json(&["usage", "pricing", "update", "--url", &url]);
    assert!(upd["data"]["models_total"].as_u64().unwrap() >= 2);
    assert_eq!(upd["data"]["events_repriced"], 1);
    let after = u.json(&["usage", "--today"]);
    assert_close(cost(&after["data"]["windows"][0]["totals"]), 0.015);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].headers.get("authorization").is_none());
    assert!(requests[0].headers.get("cookie").is_none());

    let shown = u.ok(&["usage", "pricing", "show", "--model", "test-opus"]);
    let t = text(&shown.stdout);
    assert!(t.contains("downloaded snapshot"), "{t}");
    assert!(t.contains("claude-test-opus-1  $10.00  $50.00"), "{t}");

    let back = u.json(&["usage", "pricing", "update", "--offline"]);
    assert_eq!(back["data"]["source"], "bundled");
    let t = text(&u.ok(&["usage", "pricing", "show"]).stdout);
    assert!(t.contains("bundled snapshot"), "{t}");
}

#[test]
fn pricing_update_accepts_only_the_litellm_list_or_loopback() {
    let u = U::new();
    for url in [
        "http://example.com/p.json",
        "https://example.com/BerriAI/litellm/p.json",
        "https://raw.githubusercontent.com/someone-else/repo/main/p.json",
        "https://raw.githubusercontent.com.evil.test/BerriAI/litellm/p.json",
        "https://raw.githubusercontent.com@evil.test/BerriAI/litellm/p.json",
        "http://127.0.0.1@evil.test/p.json",
        "http://localhost.evil.test/p.json",
        "file:///etc/passwd",
    ] {
        let out = u.run(&["usage", "pricing", "update", "--url", url]);
        assert_eq!(out.status.code(), Some(2), "{url}");
    }
}

#[test]
fn reset_needs_confirmation_and_deletes_everything() {
    let u = U::new();
    u.standard_fixture();
    u.ok(&["usage", "scan"]);
    std::fs::create_dir_all(u.p("data/snapshots/claude")).unwrap();
    std::fs::write(u.p("data/snapshots/claude/sess-a.json"), "{}").unwrap();

    let refused = u.run(&["usage", "reset"]);
    assert_eq!(refused.status.code(), Some(10));
    assert_eq!(
        u.json(&["usage", "--no-scan", "--month"])["data"]["windows"][0]["totals"]["events"],
        5
    );

    u.ok(&["--yes", "usage", "reset"]);
    assert!(!u.p("data/snapshots").exists());
    let after = u.json(&["usage", "--no-scan", "--month"]);
    assert_eq!(after["data"]["windows"][0]["totals"]["events"], 0);
    let rescanned = u.json(&["usage", "scan"]);
    assert_eq!(rescanned["data"]["events_new"], 5);
}

#[test]
fn planted_secrets_never_reach_output_or_the_data_dir() {
    let u = U::new();
    u.standard_fixture();
    let payload = format!(
        r#"{{"session_id":"sess-a","model":{{"id":"claude-test-opus-1"}},"prompt":"{0}","transcript_path":"/work/{0}","workspace":{{"current_dir":"/work/{0}"}},"rate_limits":{{"five_hour":{{"used_percentage":10,"resets_at":4102444800}}}}}}"#,
        SECRETS[3]
    );
    let mut outs = vec![
        u.ok(&["usage"]),
        u.ok(&["--json", "usage", "--by", "task"]),
        u.ok(&["--json", "usage", "--by", "project"]),
        u.ok(&["usage", "limits"]),
        u.ok(&["usage", "accounts"]),
    ];
    let hook = u
        .cmd()
        .args(["usage", "statusline-hook"])
        .write_stdin(payload)
        .output()
        .unwrap();
    assert!(hook.status.success());
    outs.push(hook);
    outs.push(u.ok(&["usage", "scan"]));
    outs.push(u.ok(&["usage", "limits"]));
    let refs: Vec<&std::process::Output> = outs.iter().collect();
    assert_no_secrets(&u, &refs);
}

#[test]
fn the_statusline_hook_prints_only_the_chained_output_and_never_fails() {
    let u = U::new();
    let plain = u
        .cmd()
        .args(["usage", "statusline-hook"])
        .write_stdin(r#"{"session_id":"s1"}"#)
        .output()
        .unwrap();
    assert!(plain.status.success());
    assert!(plain.stdout.is_empty() && plain.stderr.is_empty());
    assert!(!u.p("data/usage.db").exists(), "the hook opens no database");

    let chained = u
        .cmd()
        .args([
            "usage",
            "statusline-hook",
            "--chain",
            "cat; echo err >&2; echo tail",
        ])
        .write_stdin(r#"{"session_id":"s1"}"#)
        .output()
        .unwrap();
    assert!(chained.status.success());
    assert_eq!(text(&chained.stdout), "{\"session_id\":\"s1\"}tail\n");
    assert!(chained.stderr.is_empty(), "the chain's stderr is dropped");

    let garbage = u
        .cmd()
        .args(["usage", "statusline-hook", "--chain", "echo ok"])
        .write_stdin(vec![0xff, 0xfe, 0x00, b'{'])
        .output()
        .unwrap();
    assert!(garbage.status.success());
    assert_eq!(text(&garbage.stdout), "ok\n");

    let broken = u.run(&["usage", "statusline-hook", "--bogus-flag"]);
    assert_eq!(broken.status.code(), Some(0));
    assert!(broken.stdout.is_empty());

    let failing = u
        .cmd()
        .args(["usage", "statusline-hook", "--chain", "exit 3"])
        .write_stdin("{}")
        .output()
        .unwrap();
    assert!(failing.status.success());
    assert!(failing.stdout.is_empty());
}

#[test]
fn a_slow_chain_is_cut_off_after_two_seconds() {
    let u = U::new();
    let started = std::time::Instant::now();
    let out = u
        .cmd()
        .args(["usage", "statusline-hook", "--chain", "sleep 30; echo late"])
        .write_stdin("{}")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(started.elapsed() < std::time::Duration::from_secs(15));
}

#[test]
fn a_huge_hook_input_is_bounded_and_still_chained() {
    let u = U::new();
    let big = "x".repeat(3 * 1024 * 1024);
    let out = u
        .cmd()
        .args(["usage", "statusline-hook", "--chain", "wc -c"])
        .write_stdin(big)
        .output()
        .unwrap();
    assert!(out.status.success());
    let n: usize = text(&out.stdout).trim().parse().unwrap();
    assert!(n <= 1024 * 1024, "{n}");
}

#[test]
fn statusline_install_needs_a_known_claude_account() {
    let u = U::new();
    let out = u.run(&["usage", "statusline", "install", "--account", "nobody"]);
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn statusline_install_prints_the_snippet_and_changes_nothing() {
    let u = U::new();
    std::fs::write(
        u.p("claude/settings.json"),
        r#"{"statusLine":{"type":"command","command":"echo hi"},"theme":"dark"}"#,
    )
    .unwrap();
    let before = std::fs::read_to_string(u.p("claude/settings.json")).unwrap();
    let out = u.ok(&["usage", "statusline", "install"]);
    let t = text(&out.stdout);
    assert!(t.contains("statusline-hook"), "{t}");
    assert!(t.contains("--chain"), "{t}");
    assert!(t.contains("settings.json"), "{t}");
    assert_eq!(
        std::fs::read_to_string(u.p("claude/settings.json")).unwrap(),
        before
    );
    let v = u.json(&["usage", "statusline", "install"]);
    assert_valid("usage_statusline", &v);
    assert_eq!(v["data"]["installed"], false);
    assert_eq!(v["data"]["previous_command"], "echo hi");
    assert!(v["data"]["settings_path"]
        .as_str()
        .unwrap()
        .ends_with("claude/settings.json"));
}

#[test]
fn statusline_install_previews_only_the_status_line_and_never_leaks_other_settings() {
    let u = U::new();
    std::fs::write(
        u.p("claude/settings.json"),
        r#"{"apiKeyHelper":"/bin/sh -c 'echo SECRET-HELPER-1'","theme":"SECRET-THEME-1","env":{"K":"SECRET-ENV-1"},"statusLine":{"type":"command","command":"my-line --token=SECRET-TOKEN-1"}}"#,
    )
    .unwrap();
    let human = u.ok(&["usage", "statusline", "install"]);
    let json = u.ok(&["--json", "usage", "statusline", "install"]);
    let v = stdout_json(&json);
    assert_valid("usage_statusline", &v);
    assert_eq!(v["data"]["previous_command"], "<redacted>");
    assert!(v["data"].get("diff").is_none());
    for out in [&human, &json] {
        let all = format!("{}{}", text(&out.stdout), text(&out.stderr));
        for needle in [
            "SECRET-HELPER-1",
            "SECRET-THEME-1",
            "SECRET-ENV-1",
            "SECRET-TOKEN-1",
            "apiKeyHelper",
        ] {
            assert!(!all.contains(needle), "{needle} leaked");
        }
    }
    assert!(text(&human.stdout).contains("statusLine"));
}

#[test]
fn statusline_install_preview_is_a_minimal_hunk_of_the_status_line_key() {
    let u = U::new();
    std::fs::write(
        u.p("claude/settings.json"),
        r#"{"theme":"SECRET-THEME-1","statusLine":{"type":"command","command":"echo hi"}}"#,
    )
    .unwrap();
    let out = text(&u.ok(&["usage", "statusline", "install"]).stdout);
    assert!(out.contains("- ") && out.contains("echo hi"), "{out}");
    assert!(
        out.contains("+ ") && out.contains("statusline-hook"),
        "{out}"
    );
    assert!(!out.contains("SECRET-THEME-1"), "{out}");
}

#[test]
fn statusline_install_flag_never_accepts_yes_and_writes_nothing_without_a_terminal() {
    let u = U::new();
    let settings = r#"{"theme":"dark"}"#;
    std::fs::write(u.p("claude/settings.json"), settings).unwrap();
    let out = u.run(&["--yes", "usage", "statusline", "install", "--install"]);
    assert_eq!(out.status.code(), Some(10));
    assert_eq!(
        std::fs::read_to_string(u.p("claude/settings.json")).unwrap(),
        settings
    );
    let backups = std::fs::read_dir(u.p("claude"))
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains("tmgr-backup")
        })
        .count();
    assert_eq!(backups, 0);
}

#[test]
fn statusline_warns_about_a_project_level_status_line() {
    let u = U::new();
    std::fs::create_dir_all(u.p("home/.claude")).unwrap();
    std::fs::write(
        u.p("home/.claude/settings.local.json"),
        r#"{"statusLine":{"type":"command","command":"x"}}"#,
    )
    .unwrap();
    let v = u.json(&["usage", "statusline", "install"]);
    assert!(v["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w["code"] == "statusline_project_override"));
}

#[test]
fn the_hook_stores_a_whitelisted_snapshot_that_limits_then_shows() {
    let u = U::new();
    let payload = r#"{"session_id":"sess-a","model":{"id":"claude-test-opus-1"},"rate_limits":{"five_hour":{"used_percentage":33,"resets_at":4102444800},"seven_day":{"used_percentage":50,"resets_at":4102444800}},"cost":{"total_cost_usd":1.5}}"#;
    let hook = u
        .cmd()
        .args(["usage", "statusline-hook"])
        .write_stdin(payload)
        .output()
        .unwrap();
    assert!(hook.status.success());
    let v = u.json(&["usage", "limits"]);
    assert_valid("usage_limits", &v);
    let limits = v["data"]["limits"].as_array().unwrap();
    assert!(limits
        .iter()
        .any(|l| l["window"] == "session" && l["used_pct"] == 33.0));
    assert!(limits.iter().all(|l| l["source"] == "statusline"));
}

fn warning_codes(v: &Value) -> Vec<String> {
    v["warnings"]
        .as_array()
        .map(|w| {
            w.iter()
                .map(|x| x["code"].as_str().unwrap().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn unreadable_files_and_bad_lines_are_reported_separately() {
    let u = U::new();
    u.seed_prices();
    u.session_file(
        "-w/bad.jsonl",
        &[
            r#"{"type":"assistant", SECRET-BAD-LINE"#.to_string(),
            Ev::new("sb", at(0, "10:00:00"), "b1", "claude-test-opus-1", 1, 1).line(),
        ],
    );
    let v = u.json(&["usage", "--today"]);
    let codes = warning_codes(&v);
    assert!(
        codes.contains(&"usage_scan_bad_lines".to_string()),
        "{codes:?}"
    );
    assert!(
        !codes.contains(&"usage_scan_errors".to_string()),
        "{codes:?}"
    );
    assert_eq!(v["data"]["scan"]["bad_lines"], 1);
    assert_eq!(v["data"]["scan"]["errors"], 0);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        u.session_file(
            "-w/locked.jsonl",
            &[Ev::new("sl", at(0, "10:01:00"), "l1", "claude-test-opus-1", 1, 1).line()],
        );
        let locked = u.p("claude/projects/-w/locked.jsonl");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let v = u.json(&["usage", "--today"]);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();
        let codes = warning_codes(&v);
        assert!(
            codes.contains(&"usage_scan_errors".to_string()),
            "{codes:?}"
        );
        assert_eq!(v["data"]["scan"]["errors"], 1);
    }
}

#[test]
fn history_older_than_the_kept_window_is_reported_not_silently_dropped() {
    let u = U::new();
    u.seed_prices();
    u.session_file(
        "-w/now.jsonl",
        &[Ev::new("sn", at(0, "10:00:00"), "n1", "claude-test-opus-1", 1, 1).line()],
    );
    let first = u.json(&["usage", "--today"]);
    assert!(!warning_codes(&first).contains(&"usage_history_dropped".to_string()));
    u.session_file(
        "-w/ancient.jsonl",
        &[Ev::new("sa", at(-400, "10:00:00"), "a1", "claude-test-opus-1", 1, 1).line()],
    );
    let second = u.json(&["usage", "--today"]);
    assert!(warning_codes(&second).contains(&"usage_history_dropped".to_string()));
    assert_eq!(second["data"]["scan"]["skipped_before_floor"], 1);
}

#[test]
fn task_views_warn_when_the_range_reaches_before_the_kept_events() {
    let u = U::new();
    u.seed_prices();
    u.session_file(
        "-w/now.jsonl",
        &[Ev::new("sn", at(0, "10:00:00"), "n1", "claude-test-opus-1", 1, 1).line()],
    );
    let since = (today() - Duration::days(200)).to_string();
    let long = u.json(&["usage", "--by", "task", "--since", &since]);
    assert!(warning_codes(&long).contains(&"usage_task_history_limited".to_string()));
    let short = u.json(&["usage", "--by", "task", "--month"]);
    assert!(!warning_codes(&short).contains(&"usage_task_history_limited".to_string()));
}

fn hook_payload(session: &str, pct: u32) -> String {
    format!(
        r#"{{"session_id":"{session}","rate_limits":{{"five_hour":{{"used_percentage":{pct},"resets_at":4102444800}}}}}}"#
    )
}

fn run_hook(u: &U, config_dir: Option<&Path>, payload: &str) {
    let mut c = u.cmd();
    if let Some(dir) = config_dir {
        c.env("CLAUDE_CONFIG_DIR", dir);
    }
    let out = c
        .args(["usage", "statusline-hook"])
        .write_stdin(payload.to_string())
        .output()
        .unwrap();
    assert!(out.status.success());
}

#[test]
fn snapshots_from_two_accounts_land_on_their_own_labels() {
    let u = U::new();
    let work = u.p("claude-work");
    std::fs::create_dir_all(&work).unwrap();
    u.ok(&[
        "usage",
        "accounts",
        "add",
        "--tool",
        "claude",
        "--root",
        work.to_str().unwrap(),
        "--label",
        "work",
    ]);
    run_hook(&u, None, &hook_payload("sess-default", 40));
    run_hook(&u, Some(&work), &hook_payload("sess-work", 20));
    let v = u.json(&["usage", "limits"]);
    assert_valid("usage_limits", &v);
    let pct = |account: &str| {
        v["data"]["limits"]
            .as_array()
            .unwrap()
            .iter()
            .find(|l| l["account"] == account && l["window"] == "session")
            .map(|l| l["used_pct"].as_f64().unwrap())
    };
    assert_eq!(pct("claude"), Some(40.0));
    assert_eq!(pct("work"), Some(20.0));
    assert_eq!(v["data"]["limits"].as_array().unwrap().len(), 2);
}

#[test]
fn an_unset_config_dir_means_the_home_account() {
    let u = U::new();
    let mut c = u.cmd();
    c.env_remove("CLAUDE_CONFIG_DIR");
    let out = c
        .args(["usage", "statusline-hook"])
        .write_stdin(hook_payload("sess-home", 55))
        .output()
        .unwrap();
    assert!(out.status.success());
    let v = u.json(&["usage", "limits"]);
    let limits = v["data"]["limits"].as_array().unwrap();
    assert_eq!(limits.len(), 1);
    assert_eq!(limits[0]["account"], "claude");
    assert_eq!(limits[0]["used_pct"], 55.0);
}

#[test]
fn snapshot_paths_never_contain_words_from_the_config_dir() {
    let u = U::new();
    let planted = u.p("PlantedAcctDir-xyz");
    std::fs::create_dir_all(&planted).unwrap();
    run_hook(&u, Some(&planted), &hook_payload("sess-p", 10));
    let mut files = Vec::new();
    data_files(&u.p("data"), &mut files);
    assert!(!files.is_empty());
    for f in files {
        let rel = f.strip_prefix(u.p("data")).unwrap().display().to_string();
        assert!(!rel.to_lowercase().contains("planted"), "{rel}");
    }
}

struct Local<'a> {
    u: &'a U,
}

impl<'a> Local<'a> {
    fn workspace(u: &'a U, code: &str) -> Local<'a> {
        let dir = u.p("tmgr-home/workspaces").join(code);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("workspace.json"),
            json!({"id": 1, "name": code, "code": code, "schema_version": 12, "created_at": "2026-10-01T00:00:00Z"})
                .to_string(),
        )
        .unwrap();
        std::fs::write(
            u.p("tmgr-home/profile.json"),
            r#"{"id":-1,"uuid":"u","name":"Owner","created_at":"2026-10-01T00:00:00Z"}"#,
        )
        .unwrap();
        let golden: Value = serde_json::from_str(
            &std::fs::read_to_string(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../tmgr-local/tests/parity/golden/schema.json"
            ))
            .unwrap(),
        )
        .unwrap();
        let conn = Connection::open(dir.join("workspace.db")).unwrap();
        for object in golden["sqlite_master"].as_array().unwrap() {
            if let Some(ddl) = object["sql"].as_str() {
                conn.execute_batch(ddl).unwrap();
            }
        }
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('schema_version', '12')",
            [],
        )
        .unwrap();
        for s in golden["statuses"].as_array().unwrap() {
            conn.execute(
                "INSERT INTO statuses (id, name, type, color, sort_order, created_at, updated_at) VALUES (?, ?, ?, ?, ?, 'x', 'x')",
                rusqlite::params![s["id"].as_i64(), s["name"].as_str(), s["type"].as_str(), s["color"].as_str(), s["sort_order"].as_i64()],
            )
            .unwrap();
        }
        conn.execute_batch(
            "INSERT INTO categories (id, title, slug, code, settings, created_at, updated_at)
             VALUES (1, 'Work', 'work', 'WK', '[]', '2026-10-01T00:00:00Z', '2026-10-01T00:00:00Z')",
        )
        .unwrap();
        Local { u }
    }

    fn db(&self, code: &str) -> Connection {
        Connection::open(
            self.u
                .p("tmgr-home/workspaces")
                .join(code)
                .join("workspace.db"),
        )
        .unwrap()
    }

    fn task(&self, code: &str, title: &str) {
        self.u
            .ok(&["--ws", &format!("local:{code}"), "add", title, "-p", "WK"]);
    }

    #[allow(clippy::too_many_arguments)]
    fn run(
        &self,
        code: &str,
        task_id: i64,
        session: Option<&str>,
        branch: Option<&str>,
        from: &str,
        to: &str,
    ) {
        self.db(code)
            .execute(
                "INSERT INTO agent_work_runs (task_id, agent, session_id, branch, status, started_at, ended_at, duration_seconds, actor_kind, actor_id, created_at, updated_at)
                 VALUES (?, 'claude-code', ?, ?, 'succeeded', ?, ?, 60, 'user', '-1', ?, ?)",
                rusqlite::params![task_id, session, branch, from, to, from, to],
            )
            .unwrap();
    }
}

fn linked_fixture(u: &U) {
    u.seed_prices();
    let repo = u.p("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let repo = std::fs::canonicalize(repo).unwrap();
    let repo_s = repo.to_str().unwrap().to_string();
    u.session_file(
        "-s/sess-a.jsonl",
        &[
            Ev::new(
                "sess-a",
                at(0, "12:00:00"),
                "a1",
                "claude-test-opus-1",
                1000,
                100,
            )
            .line(),
            Ev::new(
                "sess-a",
                at(0, "12:05:00"),
                "a2",
                "claude-test-opus-1",
                2000,
                200,
            )
            .line(),
        ],
    );
    u.session_file(
        "-s/sess-a/subagents/agent-1.jsonl",
        &[Ev::new(
            "sess-a",
            at(0, "12:10:00"),
            "a3",
            "claude-test-sonnet-1",
            1000,
            0,
        )
        .side()
        .line()],
    );
    u.session_file(
        "-s/sess-b.jsonl",
        &[Ev::new(
            "sess-b",
            at(-1, "10:00:00"),
            "b1",
            "claude-test-sonnet-1",
            1000,
            1000,
        )
        .branch(Some("feat-x"))
        .cwd(&repo_s)
        .line()],
    );
    u.session_file(
        "-s/sess-c.jsonl",
        &[
            Ev::new("sess-c", at(0, "12:20:00"), "c1", "mystery-model-9", 10, 10)
                .cwd("/work/gamma")
                .line(),
        ],
    );
    u.session_file(
        "-s/sess-d.jsonl",
        &[Ev::new(
            "sess-d",
            at(0, "13:00:00"),
            "d1",
            "claude-test-opus-1",
            500,
            50,
        )
        .branch(Some("par"))
        .cwd(&repo_s)
        .line()],
    );
    u.session_file(
        "-s/sess-e.jsonl",
        &[Ev::new(
            "sess-e",
            at(0, "14:00:00"),
            "e1",
            "claude-test-sonnet-1",
            1000,
            100,
        )
        .branch(None)
        .cwd(&repo_s)
        .line()],
    );
    let ws = Local::workspace(u, "personal");
    for t in ["One", "Two", "Three", "Four", "Five", "Six"] {
        ws.task("personal", t);
    }
    ws.run(
        "personal",
        1,
        Some("sess-a"),
        None,
        &at(0, "11:55:00"),
        &at(0, "12:02:00"),
    );
    ws.run(
        "personal",
        2,
        Some("sess-a"),
        None,
        &at(0, "12:02:00"),
        &at(0, "12:30:00"),
    );
    ws.run(
        "personal",
        1,
        None,
        Some("feat-x"),
        &at(-1, "09:30:00"),
        &at(-1, "10:30:00"),
    );
    ws.run(
        "personal",
        3,
        None,
        Some("par"),
        &at(0, "12:50:00"),
        &at(0, "13:30:00"),
    );
    ws.run(
        "personal",
        4,
        None,
        Some("par"),
        &at(0, "12:55:00"),
        &at(0, "13:30:00"),
    );
    ws.run(
        "personal",
        5,
        None,
        None,
        &at(0, "13:50:00"),
        &at(0, "14:30:00"),
    );
    u.ok(&["project", "map", "WK", repo.to_str().unwrap()]);
}

#[test]
fn usage_is_split_by_task_with_ambiguous_and_unattributed_buckets() {
    let u = U::new();
    linked_fixture(&u);
    let v = u.json(&["usage", "--month", "--by", "task"]);
    assert_valid("usage_summary", &v);
    let w = &v["data"]["windows"][0];
    let rows = &w["rows"];
    assert_close(cost(find(rows, "task", "WK-1")), 0.015 + 0.018);
    assert_close(cost(find(rows, "task", "WK-2")), 0.03 + 0.003);
    assert_close(cost(find(rows, "task", "WK-5")), 0.0045);
    let buckets: Vec<_> = rows
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["bucket"].as_str().unwrap())
        .collect();
    assert!(
        buckets.contains(&"ambiguous") && buckets.contains(&"unattributed"),
        "{buckets:?}"
    );
    let ambiguous = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["bucket"] == "ambiguous")
        .unwrap();
    assert_eq!(ambiguous["events"], 1);
    assert_eq!(ambiguous["candidate_runs"].as_array().unwrap().len(), 2);
    assert_close(cost(ambiguous), 0.0075);
    let unattributed = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["bucket"] == "unattributed")
        .unwrap();
    assert_eq!(unattributed["unpriced_events"], 1);

    let summed: f64 = rows.as_array().unwrap().iter().map(cost).sum();
    assert_close(summed, cost(&w["totals"]));
    let plain = u.json(&["usage", "--month"]);
    assert_close(
        cost(&plain["data"]["windows"][0]["totals"]),
        cost(&w["totals"]),
    );
    assert_eq!(
        plain["data"]["windows"][0]["totals"]["events"],
        w["totals"]["events"]
    );

    let linking = &v["data"]["linking"];
    assert_eq!(linking["workspaces_ok"], json!(["local:personal"]));
    assert_eq!(linking["linked_events"], 5);
    assert_eq!(linking["ambiguous_events"], 1);
    assert_eq!(linking["unattributed_events"], 1);
    let warnings = v["warnings"].as_array().unwrap();
    assert!(warnings
        .iter()
        .any(|w| w["code"] == "usage_runs_cloud_skipped"));

    let human = text(&u.ok(&["usage", "--month", "--by", "task"]).stdout);
    assert!(
        human.contains("(ambiguous)") && human.contains("(unattributed)"),
        "{human}"
    );
}

#[test]
fn a_session_spanning_two_tasks_is_split_by_timestamp_and_sidechains_follow() {
    let u = U::new();
    linked_fixture(&u);
    let one = u.json(&["usage", "--month", "--task", "WK-1"]);
    assert_valid("usage_summary", &one);
    assert_eq!(one["data"]["task"]["ref"], "WK-1");
    assert_close(cost(&one["data"]["windows"][0]["totals"]), 0.033);
    assert_eq!(one["data"]["task"]["link_methods"]["session_id"], 1);
    assert_eq!(one["data"]["task"]["link_methods"]["branch_window"], 1);

    let two = u.json(&["usage", "--month", "--task", "WK-2", "--by", "model"]);
    let rows = &two["data"]["windows"][0]["rows"];
    assert_close(cost(find(rows, "model", "claude-test-opus-1")), 0.03);
    assert_close(cost(find(rows, "model", "claude-test-sonnet-1")), 0.003);
    let by_id = u.json(&["usage", "--month", "--task", "#2"]);
    assert_close(cost(&by_id["data"]["windows"][0]["totals"]), 0.033);
    let human = text(&u.ok(&["usage", "--month", "--task", "WK-2"]).stdout);
    assert!(human.contains("Task WK-2"), "{human}");
    assert!(human.contains("local:personal#"), "{human}");
    let none = text(&u.ok(&["usage", "--month", "--task", "WK-6"]).stdout);
    assert!(none.contains("No usage linked to WK-6"), "{none}");
}

#[test]
fn the_project_mapping_links_runs_without_session_or_branch() {
    let u = U::new();
    linked_fixture(&u);
    let v = u.json(&["usage", "--month", "--task", "WK-5"]);
    assert_close(cost(&v["data"]["windows"][0]["totals"]), 0.0045);
    assert_eq!(v["data"]["task"]["link_methods"]["project_window"], 1);
}

#[test]
fn a_broken_workspace_warns_and_the_rest_still_link() {
    let u = U::new();
    linked_fixture(&u);
    let dir = u.p("tmgr-home/workspaces/broken");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("workspace.json"),
        r#"{"id": 2, "name": "Broken", "code": "broken", "schema_version": 12, "created_at": "2026-10-01T00:00:00Z"}"#,
    )
    .unwrap();
    std::fs::write(dir.join("workspace.db"), b"not a database at all").unwrap();
    let v = u.json(&["usage", "--month", "--by", "task"]);
    assert_valid("usage_summary", &v);
    assert_eq!(
        v["data"]["linking"]["workspaces_failed"],
        json!(["local:broken"])
    );
    assert_eq!(
        v["data"]["linking"]["workspaces_ok"],
        json!(["local:personal"])
    );
    assert_close(
        cost(find(&v["data"]["windows"][0]["rows"], "task", "WK-1")),
        0.033,
    );
    let w = v["warnings"].as_array().unwrap();
    let broken = w
        .iter()
        .find(|w| w["code"] == "usage_runs_unavailable")
        .unwrap();
    assert_eq!(
        broken["message"],
        "runs of workspace local:broken could not be read"
    );
}

#[test]
fn an_explicit_workspace_limits_linking_to_it() {
    let u = U::new();
    linked_fixture(&u);
    let other = Local::workspace(&u, "side");
    other.task("side", "Other");
    other.run(
        "side",
        1,
        Some("sess-a"),
        None,
        &at(0, "11:00:00"),
        &at(0, "23:00:00"),
    );
    let both = u.json(&["usage", "--month", "--by", "task"]);
    assert_eq!(
        both["data"]["linking"]["workspaces_ok"],
        json!(["local:personal", "local:side"])
    );
    let only = u.json(&["--ws", "local:personal", "usage", "--month", "--by", "task"]);
    assert_eq!(
        only["data"]["linking"]["workspaces_ok"],
        json!(["local:personal"])
    );
}

#[test]
fn without_any_workspace_everything_is_unattributed_and_plain_usage_makes_no_requests() {
    let u = U::new();
    u.standard_fixture();
    let v = u.json(&["usage", "--month", "--by", "task"]);
    assert_valid("usage_summary", &v);
    let rows = v["data"]["windows"][0]["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["bucket"], "unattributed");
    let plain = u.json(&["usage"]);
    assert!(!plain["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w["code"].as_str().unwrap().starts_with("usage_runs")));
}

#[test]
fn run_output_exposes_the_session_id() {
    let u = U::new();
    linked_fixture(&u);
    let v = u.json(&["--ws", "local:personal", "log"]);
    assert_common_valid_run_list(&v);
    let sessions: Vec<_> = v["data"]["runs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["session_id"].clone())
        .collect();
    assert!(sessions.contains(&json!("sess-a")));
    assert!(sessions.contains(&Value::Null));
}

fn assert_common_valid_run_list(v: &Value) {
    assert_valid("run_list", v);
}

#[test]
fn a_run_recorded_by_the_cli_maps_to_its_repository_through_runs_json() {
    let u = U::new();
    linked_fixture(&u);
    let repo2 = std::fs::canonicalize({
        std::fs::create_dir_all(u.p("repo2")).unwrap();
        u.p("repo2")
    })
    .unwrap();
    let repo2_s = repo2.to_str().unwrap();
    u.session_file(
        "-s/sess-f.jsonl",
        &[Ev::new(
            "sess-f",
            at(0, "15:00:00"),
            "f1",
            "claude-test-opus-1",
            1000,
            0,
        )
        .branch(None)
        .cwd(repo2_s)
        .line()],
    );
    Local { u: &u }.run(
        "personal",
        6,
        None,
        None,
        &at(0, "14:50:00"),
        &at(0, "15:30:00"),
    );
    let state = u.p("state/tmgr");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(
        state.join("runs.json"),
        json!({"schema": "tmgr.runs.v1", "runs": [{
            "run_id": 7, "task_id": 6, "task_key": "WK-6", "repo": repo2_s,
            "agent": "claude-code", "started_at": at(0, "14:50:00"), "workspace": "local:personal"
        }]})
        .to_string(),
    )
    .unwrap();
    let v = u.json(&["usage", "--month", "--task", "WK-6"]);
    assert_close(cost(&v["data"]["windows"][0]["totals"]), 0.01);
    assert_eq!(v["data"]["task"]["link_methods"]["project_window"], 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn cloud_runs_link_through_their_session_id() {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/workspaces"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [
            {"id": 1, "name": "Personal", "code": "personal", "is_default": true}
        ]})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/workspaces/1/agent-work"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"runs": [{
            "id": 10, "task_id": 5, "agent": "claude-code", "session_id": "sess-a", "status": "succeeded",
            "started_at": at(0, "11:00:00"), "ended_at": at(0, "13:00:00"), "duration_seconds": 7200,
            "task": {"id": 5, "key": "TM-5", "title": "TM-5: Cloud task"}
        }]})))
        .mount(&server)
        .await;
    let u = U::with_server(server.uri());
    let account = format!("{}:default", server.address());
    std::fs::create_dir_all(u.p("xdg/tmgr")).unwrap();
    let creds = u.p("xdg/tmgr/credentials.json");
    std::fs::write(
        &creds,
        json!({ account: {"access_token": "acc-token-AAAA1111", "refresh_token": "ref-token-BBBB2222", "expires_at": "2099-12-01T00:00:00Z"} })
            .to_string(),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&creds, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::set_permissions(u.p("xdg/tmgr"), std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    u.standard_fixture();
    let v = u.json(&["usage", "--month", "--by", "task"]);
    assert_valid("usage_summary", &v);
    let rows = &v["data"]["windows"][0]["rows"];
    assert_close(cost(find(rows, "task", "TM-5")), 0.048);
    assert_eq!(v["data"]["linking"]["workspaces_ok"], json!(["personal"]));
    let all = text(&u.ok(&["--json", "usage", "--by", "task"]).stdout);
    assert!(!all.contains("acc-token-AAAA1111") && !all.contains("ref-token-BBBB2222"));
}
