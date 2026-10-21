mod common;

use assert_cmd::Command;
use chrono::{Duration, NaiveDate, Utc};
use common::{assert_valid, dead_server, stdout_json, text};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tmgr_usage::limits::{LimitSnapshot, LimitSource, LimitWindow};
use tmgr_usage::{AgentMeta, Fact, FactEnd, FactKind, PriceBook, Store, Tool};

const PRICES: &str = r#"{
  "claude-test-opus-1": {"mode": "chat", "input_cost_per_token": 1e-5, "output_cost_per_token": 5e-5,
    "cache_read_input_token_cost": 1e-6, "cache_creation_input_token_cost": 1.25e-5},
  "claude-test-sonnet-1": {"mode": "chat", "input_cost_per_token": 3e-6, "output_cost_per_token": 1.5e-5,
    "cache_read_input_token_cost": 3e-7, "cache_creation_input_token_cost": 3.75e-6},
  "gpt-test-1": {"mode": "chat", "input_cost_per_token": 2e-6, "output_cost_per_token": 8e-6,
    "cache_read_input_token_cost": 5e-7}
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
            .env("TMGR_USAGE_SYNC_WAIT_MS", "30")
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
    assert!(text(&empty.stderr).contains("Claude limits: run `tmgr usage statusline install`"));
    let v = u.json(&["usage", "limits", "--no-scan"]);
    assert_valid("usage_limits", &v);
    assert_eq!(v["data"]["limits"], json!([]));
    assert_eq!(v["warnings"][0]["code"], "claude_limits_missing", "{v}");

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
    assert!(
        !text(&out.stderr).contains("statusline install"),
        "{}",
        text(&out.stderr)
    );
    let v = u.json(&["usage", "limits", "--no-scan"]);
    assert_valid("usage_limits", &v);
    assert_eq!(v["data"]["limits"].as_array().unwrap().len(), 2);
    assert_eq!(v["warnings"], json!([]), "{v}");

    rusqlite::Connection::open(u.p("data/usage.db"))
        .unwrap()
        .execute(
            "UPDATE usage_limit_snapshot SET observed_at = observed_at - 2 * 86400000",
            [],
        )
        .unwrap();
    let out = u.ok(&["usage", "limits", "--no-scan"]);
    assert!(text(&out.stderr).contains("Claude limits: run `tmgr usage statusline install`"));
    assert!(!text(&out.stdout).contains("Claude limits:"));
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
        "https://raw.githubusercontent.com/BerriAI/litellm/../evil/p.json",
        "https://raw.githubusercontent.com/BerriAI/litellm/%2e%2e/evil/p.json",
        "https://RAW.githubusercontent.com/BerriAI/litellm/p.json",
        "https://raw.githubusercontent.com:8443/BerriAI/litellm/p.json",
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
    let shown = v["data"]["settings_path"].as_str().unwrap();
    assert!(
        shown.ends_with("/settings.json")
            && (shown.starts_with("~/") || shown.starts_with("<external>/")),
        "settings_path is not masked"
    );
    assert!(!shown.contains(u.p("claude").to_str().unwrap()));
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
fn dropped_history_warning_reaches_stderr_three_times_then_only_json() {
    let u = U::new();
    u.seed_prices();
    u.session_file(
        "-w/now.jsonl",
        &[Ev::new("sn", at(0, "10:00:00"), "n1", "claude-test-opus-1", 1, 1).line()],
    );
    u.json(&["usage", "--today"]);
    let msg = "events are older than the 90 days of kept history";
    let mut lines = Vec::new();
    for i in 0..5 {
        lines.push(
            Ev::new(
                "sa",
                at(-400, "10:00:00"),
                &format!("a{i}"),
                "claude-test-opus-1",
                1,
                1,
            )
            .line(),
        );
        u.session_file("-w/ancient.jsonl", &lines);
        let out = u.ok(&["usage", "--today"]);
        assert_eq!(text(&out.stderr).contains(msg), i < 3, "run {i}");
    }
    lines.push(Ev::new("sa", at(-400, "10:00:00"), "a9", "claude-test-opus-1", 1, 1).line());
    u.session_file("-w/ancient.jsonl", &lines);
    let v = u.json(&["usage", "--today"]);
    assert!(warning_codes(&v).contains(&"usage_history_dropped".to_string()));
    lines.push(
        Ev::new(
            "sa",
            at(-400, "10:00:00"),
            "a10",
            "claude-test-opus-1",
            1,
            1,
        )
        .line(),
    );
    u.session_file("-w/ancient.jsonl", &lines);
    let out = u.ok(&["usage", "--today"]);
    assert!(!text(&out.stderr).contains(msg));
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
                at(-2, "12:00:00"),
                "a1",
                "claude-test-opus-1",
                1000,
                100,
            )
            .line(),
            Ev::new(
                "sess-a",
                at(-2, "12:05:00"),
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
            at(-2, "12:10:00"),
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
            at(-3, "10:00:00"),
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
        &[Ev::new(
            "sess-c",
            at(-2, "12:20:00"),
            "c1",
            "mystery-model-9",
            10,
            10,
        )
        .cwd("/work/gamma")
        .line()],
    );
    u.session_file(
        "-s/sess-d.jsonl",
        &[Ev::new(
            "sess-d",
            at(-2, "13:00:00"),
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
            at(-2, "14:00:00"),
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
        &at(-2, "11:55:00"),
        &at(-2, "12:02:00"),
    );
    ws.run(
        "personal",
        2,
        Some("sess-a"),
        None,
        &at(-2, "12:02:00"),
        &at(-2, "12:30:00"),
    );
    ws.run(
        "personal",
        1,
        None,
        Some("feat-x"),
        &at(-3, "09:30:00"),
        &at(-3, "10:30:00"),
    );
    ws.run(
        "personal",
        3,
        None,
        Some("par"),
        &at(-2, "12:50:00"),
        &at(-2, "13:30:00"),
    );
    ws.run(
        "personal",
        4,
        None,
        Some("par"),
        &at(-2, "12:55:00"),
        &at(-2, "13:30:00"),
    );
    ws.run(
        "personal",
        5,
        None,
        None,
        &at(-2, "13:50:00"),
        &at(-2, "14:30:00"),
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
        &at(-2, "11:00:00"),
        &at(-2, "23:00:00"),
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
            at(-2, "15:00:00"),
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
        &at(-2, "14:50:00"),
        &at(-2, "15:30:00"),
    );
    let state = u.p("state/tmgr");
    std::fs::create_dir_all(&state).unwrap();
    std::fs::write(
        state.join("runs.json"),
        json!({"schema": "tmgr.runs.v1", "runs": [{
            "run_id": 7, "task_id": 6, "task_key": "WK-6", "repo": repo2_s,
            "agent": "claude-code", "started_at": at(-2, "14:50:00"), "workspace": "local:personal"
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
            "started_at": at(-1, "11:00:00"), "ended_at": at(0, "13:00:00"), "duration_seconds": 93600,
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

#[tokio::test(flavor = "multi_thread")]
async fn an_endless_run_listing_is_cut_off_with_a_warning() {
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
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"next_cursor": "more", "runs": [{
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
    assert!(warning_codes(&v).contains(&"usage_runs_truncated".to_string()));
}

fn stamp(clock: &str) -> chrono::DateTime<Utc> {
    at(-2, clock).parse().unwrap()
}

fn codex_rollout(u: &U, cwd: &str) {
    let line = |clock: &str, body: &str| format!(r#"{{"timestamp":"{}",{body}}}"#, at(-2, clock));
    let usage = |i: u64, o: u64| {
        format!(
            r#"{{"input_tokens":{i},"cached_input_tokens":0,"output_tokens":{o},"reasoning_output_tokens":0,"total_tokens":{}}}"#,
            i + o
        )
    };
    let count = |clock: &str, total: (u64, u64), last: (u64, u64)| {
        line(
            clock,
            &format!(
                r#""type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{},"last_token_usage":{}}},"rate_limits":null}}"#,
                usage(total.0, total.1),
                usage(last.0, last.1)
            ),
        )
    };
    let lines = [
        line(
            "12:08:00",
            &format!(r#""type":"session_meta","payload":{{"id":"cx1","cwd":"{cwd}"}}"#),
        ),
        line(
            "12:08:01",
            &format!(r#""type":"turn_context","payload":{{"model":"gpt-test-1","cwd":"{cwd}"}}"#),
        ),
        count("12:09:00", (1000, 100), (1000, 100)),
        count("12:12:00", (3000, 300), (2000, 200)),
    ];
    let path = u.p("codex/sessions/2026/10/rollout-cx1.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, format!("{}\n", lines.join("\n"))).unwrap();
}

fn call_fact(session: &str, agent: Option<&str>, key: &str, name: &str, clock: &str) -> Fact {
    Fact {
        account: "claude".into(),
        key: key.into(),
        tool_session_id: session.into(),
        agent_id: agent.map(String::from),
        ts: stamp(clock),
        kind: Some(FactKind::ToolCall),
        name: name.into(),
        cwd: Some("/work/alpha".into()),
        ..Fact::default()
    }
}

fn tree_fixture(u: &U, codex_cwd: &str) {
    linked_fixture(u);
    tree_extras(u, codex_cwd);
}

fn tree_extras(u: &U, codex_cwd: &str) {
    codex_rollout(u, codex_cwd);
    u.ok(&["usage", "scan"]);
    Connection::open(u.p("data/usage.db"))
        .unwrap()
        .execute(
            "UPDATE usage_event SET agent_id = 'ag1' WHERE event_key LIKE '%a3%'",
            [],
        )
        .unwrap();
    let store = Store::open(&u.p("data/usage.db")).unwrap();
    store
        .record_agent(
            Tool::Claude,
            &AgentMeta {
                account: "claude".into(),
                tool_session_id: "sess-a".into(),
                version: Some("2.1.90".into()),
                entrypoint: Some("cli".into()),
                ..AgentMeta::default()
            },
        )
        .unwrap();
    store
        .record_agent(
            Tool::Claude,
            &AgentMeta {
                account: "claude".into(),
                tool_session_id: "sess-a".into(),
                agent_id: Some("ag1".into()),
                agent_type: Some("codex:codex-rescue".into()),
                spawn_tool_use_id: Some("tu1".into()),
                ..AgentMeta::default()
            },
        )
        .unwrap();
    store
        .record_agent(
            Tool::Codex,
            &AgentMeta {
                account: "codex".into(),
                tool_session_id: "cx1".into(),
                originator: Some("Claude_Code".into()),
                source: Some("vscode".into()),
                version: Some("0.150.0".into()),
                started_at: Some(stamp("12:08:00")),
                cwd: Some(codex_cwd.into()),
                ..AgentMeta::default()
            },
        )
        .unwrap();
    let mut agent_call = call_fact("sess-a", None, "tu1", "Agent", "12:07:00");
    agent_call.detail = Some("codex:codex-rescue".into());
    let mut bash = call_fact("sess-a", Some("ag1"), "tu2", "Bash", "12:07:30");
    bash.detail = Some("codex".into());
    let mut skill = call_fact(
        "sess-a",
        None,
        "sk1",
        "superpowers:brainstorming",
        "12:06:00",
    );
    skill.kind = Some(FactKind::Skill);
    let mut hook = call_fact("sess-a", None, "hk1", "token-goat", "12:06:30");
    hook.kind = Some(FactKind::Hook);
    hook.detail = Some("PostToolUse".into());
    hook.duration_ms = Some(120);
    for f in [&agent_call, &bash, &skill, &hook] {
        store.record_fact(Tool::Claude, f).unwrap();
    }
    store
        .end_fact(
            Tool::Claude,
            &FactEnd {
                key: "tu2".into(),
                ended_at: stamp("12:16:00"),
                is_error: false,
            },
        )
        .unwrap();
    for (key, name, clock) in [
        ("c1", "exec", "12:09:00"),
        ("c2", "exec", "12:10:00"),
        ("c3", "apply_patch", "12:11:00"),
    ] {
        let mut f = call_fact("cx1", None, key, name, clock);
        f.account = "codex".into();
        f.cwd = Some(codex_cwd.into());
        store.record_fact(Tool::Codex, &f).unwrap();
    }
}

#[test]
fn usage_task_json_carries_the_aggregate_with_subagent_and_linked_codex_nodes() {
    let u = U::new();
    tree_fixture(&u, "/work/alpha");
    let out = u.ok(&["--json", "usage", "task", "WK-2"]);
    let v = stdout_json(&out);
    assert_valid("usage_task", &v);
    let a = &v["data"]["aggregate"];
    assert_eq!(a["schema"], "task_usage.v1");
    assert_eq!(a["task"]["ref"], "WK-2");
    assert_eq!(a["task"]["workspace"], "local:personal");
    assert!(a["source_id"].as_str().unwrap().len() >= 16);
    let nodes = a["nodes"].as_array().unwrap();
    let kinds: Vec<(&str, &str)> = nodes
        .iter()
        .map(|n| {
            (
                n["tool"].as_str().unwrap(),
                n["agent_type"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        kinds,
        vec![
            ("claude", "main"),
            ("claude", "codex:codex-rescue"),
            ("codex", "codex")
        ]
    );
    assert_eq!(nodes[1]["parent_id"], "n0");
    assert_eq!(nodes[2]["parent_id"], "n1");
    assert_eq!(nodes[2]["link"], "exact");
    assert_eq!(nodes[2]["origin"], "vscode");
    assert_eq!(nodes[0]["origin"], "cli");
    assert_eq!(nodes[2]["events"], 2);
    assert_eq!(a["totals"]["events"], 4);
    assert_eq!(
        a["context"]["skills"][0]["name"],
        "superpowers:brainstorming"
    );
    assert_eq!(a["context"]["hooks"][0]["event"], "PostToolUse");
    let codex_tools: Vec<&str> = nodes[2]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(codex_tools, vec!["exec", "apply_patch"]);
    assert!(a["totals"].get("waiting_ms").is_none());
    assert_eq!(v["data"]["linking"]["linked_facts"], 4);

    let raw = text(&out.stdout);
    for leak in [
        u.p("home").to_str().unwrap(),
        "/work/alpha",
        "sess-a",
        "cx1",
        "SECRET",
    ] {
        assert!(!raw.contains(leak), "{leak} leaked: {raw}");
    }
}

#[test]
fn usage_task_marks_a_codex_session_from_another_repository_as_likely() {
    let u = U::new();
    tree_fixture(&u, "/work/other");
    let v = u.json(&["usage", "task", "WK-2"]);
    assert_valid("usage_task", &v);
    assert_eq!(v["data"]["aggregate"]["nodes"][2]["link"], "likely");
    let human = text(
        &u.ok(&["usage", "task", "WK-2", "--tree", "--no-scan"])
            .stdout,
    );
    assert!(human.contains("(likely)"), "{human}");
}

#[test]
fn usage_task_tree_text_follows_the_layout_and_hides_the_tree_without_the_flag() {
    let u = U::new();
    tree_fixture(&u, "/work/alpha");
    let tree = text(&u.ok(&["usage", "task", "WK-2", "--tree"]).stdout);
    println!("{tree}");
    for part in [
        "WK-2",
        "Time    agents",
        "Cost    AI $",
        "(API-equivalent)",
        "Tokens  input",
        "Claude Code",
        "└─ codex:codex-rescue",
        "   └─ Codex",
        "gpt-test-1",
        "Tools   ",
        "Skills  superpowers:brainstorming ×1",
        "Plugins ",
        "Hooks   1 runs",
    ] {
        assert!(tree.contains(part), "missing {part:?} in\n{tree}");
    }
    assert!(!tree.contains("(likely)"), "{tree}");
    assert!(!tree.contains("\x1b["), "{tree}");
    for leak in [u.p("home").to_str().unwrap(), "/work/alpha", "sess-a"] {
        assert!(!tree.contains(leak), "{leak} leaked: {tree}");
    }
    let flat = text(&u.ok(&["usage", "task", "WK-2", "--no-scan"]).stdout);
    assert!(flat.contains("Claude Code"), "{flat}");
    assert!(!flat.contains("└─") && !flat.contains("├─"), "{flat}");
}

#[test]
fn usage_task_for_an_unlinked_task_says_so_and_a_bad_ref_exits_two() {
    let u = U::new();
    linked_fixture(&u);
    let none = text(&u.ok(&["usage", "task", "WK-6"]).stdout);
    assert!(none.contains("No usage linked to WK-6"), "{none}");
    let v = u.json(&["usage", "task", "WK-6"]);
    assert_valid("usage_task", &v);
    assert!(v["data"]["aggregate"].is_null());
    let unknown = text(&u.ok(&["usage", "task", "ZZ-99"]).stdout);
    assert!(unknown.contains("No usage linked to ZZ-99"), "{unknown}");
    let bad = u.run(&["usage", "task", "not-a-ref"]);
    assert_eq!(bad.status.code(), Some(2), "{}", text(&bad.stderr));
}

struct Call<'a> {
    id: &'a str,
    name: &'a str,
    input: String,
}

fn work_line(session: &str, clock: &str, msg: &str, side: bool, calls: &[Call]) -> String {
    let blocks: Vec<String> = calls
        .iter()
        .map(|c| {
            format!(
                r#"{{"type":"tool_use","id":"{}","name":"{}","input":{}}}"#,
                c.id, c.name, c.input
            )
        })
        .collect();
    format!(
        r#"{{"type":"assistant","sessionId":"{session}","timestamp":"{}","cwd":"/work/orch","gitBranch":"main","isSidechain":{side},"requestId":"req-{msg}","message":{{"id":"{msg}","model":"claude-test-opus-1","content":[{}],"usage":{{"input_tokens":1000,"output_tokens":100,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}}}}"#,
        at(-2, clock),
        blocks.join(",")
    )
}

fn subagent(u: &U, session: &str, id: &str, meta: &str, lines: &[String]) {
    u.session_file(
        &format!("-orch/{session}/subagents/agent-{id}.jsonl"),
        lines,
    );
    let path = u.p(&format!(
        "claude/projects/-orch/{session}/subagents/agent-{id}.meta.json"
    ));
    std::fs::write(path, meta).unwrap();
}

fn agent_call(id: &str) -> Call<'_> {
    Call {
        id,
        name: "Agent",
        input: r#"{"subagent_type":"general-purpose"}"#.into(),
    }
}

fn start_call<'a>(id: &'a str, task: u64) -> Call<'a> {
    Call {
        id,
        name: "mcp__tmgr__start_agent_work",
        input: format!(r#"{{"taskId":{task},"agent":"claude-code","branch":"feat/other"}}"#),
    }
}

fn finish_call<'a>(id: &'a str, run: u64) -> Call<'a> {
    Call {
        id,
        name: "mcp__tmgr__finish_agent_work",
        input: format!(
            r#"{{"runId":{run},"summary":"SECRET-SUMMARY","commits":["abc SECRET-COMMIT"],"prUrl":"https://example.com/SECRET-PR"}}"#
        ),
    }
}

fn owner_shape(u: &U) {
    u.seed_prices();
    u.session_file(
        "-orch/sess-o.jsonl",
        &[
            work_line("sess-o", "11:50:00", "o1", false, &[]),
            work_line("sess-o", "12:00:30", "o2", false, &[agent_call("tuL")]),
            work_line("sess-o", "12:25:00", "o3", false, &[]),
        ],
    );
    subagent(
        u,
        "sess-o",
        "l1",
        r#"{"agentType":"general-purpose","toolUseId":"tuL"}"#,
        &[
            work_line("sess-o", "12:01:00", "l1a", true, &[start_call("tl1", 2)]),
            work_line("sess-o", "12:05:00", "l1b", true, &[agent_call("tuW")]),
            work_line("sess-o", "12:10:00", "l1c", true, &[]),
            work_line("sess-o", "12:20:00", "l1d", true, &[finish_call("tl2", 1)]),
        ],
    );
    subagent(
        u,
        "sess-o",
        "w1",
        r#"{"agentType":"general-purpose","parentAgentId":"l1","toolUseId":"tuW"}"#,
        &[work_line("sess-o", "12:12:00", "w1a", true, &[])],
    );
    let ws = Local::workspace(u, "personal");
    ws.task("personal", "One");
    ws.task("personal", "Two");
    ws.run(
        "personal",
        2,
        None,
        Some("feat/other"),
        &at(-2, "12:00:00"),
        &at(-2, "12:30:00"),
    );
}

#[test]
fn agent_work_calls_link_a_lead_and_its_workers_but_not_the_orchestrator() {
    let u = U::new();
    owner_shape(&u);
    let out = u.ok(&["--json", "usage", "task", "WK-2"]);
    let v = stdout_json(&out);
    assert_valid("usage_task", &v);
    let a = &v["data"]["aggregate"];
    assert_eq!(a["totals"]["events"], 5, "{v}");
    assert_eq!(v["data"]["linking"]["linked_events"], 5);
    assert_eq!(v["data"]["linking"]["linked_facts"], 3);
    let nodes = a["nodes"].as_array().unwrap();
    assert_eq!(nodes.len(), 3, "{nodes:?}");
    assert_eq!(nodes[0]["agent_type"], "main");
    assert_eq!(nodes[0]["events"], 0);
    assert_eq!(nodes[1]["parent_id"], nodes[0]["id"]);
    assert_eq!(nodes[1]["events"], 4);
    assert_eq!(nodes[2]["parent_id"], nodes[1]["id"]);
    assert_eq!(nodes[2]["events"], 1);

    let by_task = u.json(&["usage", "--task", "WK-2", "--month"]);
    assert_eq!(
        by_task["data"]["task"]["link_methods"]["agent_work_run"], 5,
        "{by_task}"
    );

    let mut files = Vec::new();
    data_files(&u.p("data"), &mut files);
    let raw = text(&out.stdout);
    for leak in [
        "SECRET-SUMMARY",
        "SECRET-COMMIT",
        "SECRET-PR",
        "example.com",
    ] {
        assert!(!raw.contains(leak), "{leak} leaked in output");
        for f in &files {
            let bytes = std::fs::read(f).unwrap();
            assert!(
                !bytes.windows(leak.len()).any(|w| w == leak.as_bytes()),
                "{leak} leaked into {f:?}"
            );
        }
    }
}

fn two_task_workspace(u: &U) {
    let ws = Local::workspace(u, "personal");
    ws.task("personal", "One");
    ws.task("personal", "Two");
    for task in [1, 2] {
        ws.run(
            "personal",
            task,
            None,
            Some("feat/other"),
            &at(-2, "12:00:00"),
            &at(-2, "12:30:00"),
        );
    }
}

#[test]
fn a_start_call_alone_links_likely_and_the_text_says_so() {
    let u = U::new();
    u.seed_prices();
    u.session_file(
        "-orch/sess-o.jsonl",
        &[work_line(
            "sess-o",
            "12:00:30",
            "o1",
            false,
            &[agent_call("tuL")],
        )],
    );
    subagent(
        &u,
        "sess-o",
        "l1",
        r#"{"agentType":"general-purpose","toolUseId":"tuL"}"#,
        &[
            work_line("sess-o", "12:01:30", "l1a", true, &[start_call("tl1", 2)]),
            work_line("sess-o", "12:10:00", "l1b", true, &[]),
        ],
    );
    two_task_workspace(&u);
    let v = u.json(&["usage", "--task", "WK-2", "--month"]);
    assert_eq!(v["data"]["task"]["link_methods"]["agent_work_task"], 2);
    let human = text(&u.ok(&["usage", "task", "WK-2"]).stdout);
    assert!(human.contains("some by task id only: likely"), "{human}");
}

#[test]
fn a_start_call_without_a_run_start_nearby_and_a_run_id_outside_the_window_bind_nothing() {
    let u = U::new();
    u.seed_prices();
    u.session_file(
        "-orch/sess-o.jsonl",
        &[work_line(
            "sess-o",
            "12:00:30",
            "o1",
            false,
            &[agent_call("tuL")],
        )],
    );
    subagent(
        &u,
        "sess-o",
        "l1",
        r#"{"agentType":"general-purpose","toolUseId":"tuL"}"#,
        &[
            work_line("sess-o", "12:05:00", "l1a", true, &[start_call("tl1", 2)]),
            work_line("sess-o", "13:10:00", "l1b", true, &[finish_call("tl2", 1)]),
        ],
    );
    two_task_workspace(&u);
    let v = u.json(&["usage", "task", "WK-2"]);
    assert!(v["data"]["aggregate"].is_null(), "{v}");
    assert_eq!(v["data"]["linking"]["linked_events"], 0);
}

#[test]
fn parallel_leads_on_two_tasks_each_link_to_their_own_task_once() {
    let u = U::new();
    u.seed_prices();
    u.session_file(
        "-orch/sess-o.jsonl",
        &[work_line(
            "sess-o",
            "12:00:30",
            "o1",
            false,
            &[agent_call("tuA"), agent_call("tuB")],
        )],
    );
    for (id, tool_use, task) in [("a1", "tuA", 1), ("b1", "tuB", 2)] {
        let msg = |n: &str| format!("{id}{n}");
        subagent(
            &u,
            "sess-o",
            id,
            &format!(r#"{{"agentType":"general-purpose","toolUseId":"{tool_use}"}}"#),
            &[
                work_line(
                    "sess-o",
                    "12:01:00",
                    &msg("a"),
                    true,
                    &[start_call(&format!("t{id}"), task)],
                ),
                work_line("sess-o", "12:10:00", &msg("b"), true, &[]),
                work_line("sess-o", "12:11:00", &msg("c"), true, &[]),
            ],
        );
    }
    two_task_workspace(&u);
    for task in ["WK-1", "WK-2"] {
        let v = u.json(&["usage", "task", task]);
        assert_eq!(v["data"]["aggregate"]["totals"]["events"], 3, "{task}: {v}");
        assert_eq!(v["data"]["linking"]["ambiguous_events"], 0);
        assert_eq!(v["data"]["linking"]["linked_events"], 6);
    }
}

#[test]
fn a_binding_made_before_the_period_still_links_events_inside_it() {
    let u = U::new();
    u.seed_prices();
    let next_day = |line: String, clock: &str| line.replace(&at(-2, clock), &at(-1, clock));
    u.session_file(
        "-orch/sess-o.jsonl",
        &[
            work_line("sess-o", "23:31:00", "o1", false, &[start_call("tl1", 2)]),
            next_day(
                work_line("sess-o", "00:10:00", "o2", false, &[]),
                "00:10:00",
            ),
        ],
    );
    let ws = Local::workspace(&u, "personal");
    ws.task("personal", "One");
    ws.task("personal", "Two");
    ws.run(
        "personal",
        2,
        None,
        Some("feat/other"),
        &at(-2, "23:30:00"),
        &at(-1, "00:30:00"),
    );
    let y = (today() - Duration::days(1)).to_string();
    let v = u.json(&["usage", "--task", "WK-2", "--since", &y, "--until", &y]);
    assert_eq!(
        v["data"]["task"]["link_methods"]["agent_work_task"], 1,
        "{v}"
    );
}

mod usage_sync {
    use super::*;
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};
    use tmgr_usage::sync::LedgerEntry;
    use wiremock::matchers::{body_partial_json, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const PUT: &str = "/api/agent-usage";
    const A: &str = "/Users/someone/secret";
    const B: &str = "alice@example.com";
    const C: &str = "PROMPT SECRET TEXT";

    fn put_ok() -> ResponseTemplate {
        ResponseTemplate::new(200).set_body_json(json!({"data": {
            "task_id": 5, "source_id": "x", "generated_at": "2026-10-16T12:00:00Z",
            "stale": false, "updated_at": "2026-10-16T12:00:01Z"
        }}))
    }

    fn put_mock(response: ResponseTemplate) -> Mock {
        Mock::given(method("PUT"))
            .and(path(PUT))
            .respond_with(response)
    }

    fn run_json(
        id: u64,
        task_id: u64,
        key: &str,
        session: &str,
        from: String,
        to: String,
    ) -> Value {
        json!({
            "id": id, "task_id": task_id, "agent": "claude-code", "session_id": session,
            "status": "succeeded", "started_at": from, "ended_at": to, "duration_seconds": 600,
            "task": {"id": task_id, "key": key, "title": format!("{key}: a title")}
        })
    }

    fn standard_runs() -> (Vec<Value>, Vec<Value>) {
        (
            vec![
                run_json(
                    10,
                    5,
                    "TM-5",
                    "sess-a",
                    at(-2, "11:55:00"),
                    at(-2, "12:30:00"),
                ),
                run_json(
                    11,
                    7,
                    "TM-7",
                    "sess-b",
                    at(-1, "09:30:00"),
                    at(-1, "10:30:00"),
                ),
            ],
            vec![run_json(
                20,
                9,
                "WK-1",
                "sess-c",
                at(-2, "12:15:00"),
                at(-2, "12:40:00"),
            )],
        )
    }

    struct Cloud {
        u: U,
        server: MockServer,
    }

    impl Cloud {
        async fn new(puts: Vec<Mock>) -> Cloud {
            let (personal, work) = standard_runs();
            Cloud::build(personal, work, puts).await
        }

        async fn build(personal: Vec<Value>, work: Vec<Value>, mocks: Vec<Mock>) -> Cloud {
            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/api/workspaces"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": [
                    {"id": 1, "name": "Personal", "code": "personal", "is_default": true},
                    {"id": 2, "name": "Work", "code": "work", "is_default": false}
                ]})))
                .mount(&server)
                .await;
            for (id, runs) in [(1, personal), (2, work)] {
                Mock::given(method("GET"))
                    .and(path(format!("/api/workspaces/{id}/agent-work")))
                    .respond_with(ResponseTemplate::new(200).set_body_json(json!({"runs": runs})))
                    .mount(&server)
                    .await;
            }
            for m in mocks {
                m.mount(&server).await;
            }
            put_mock(put_ok()).mount(&server).await;
            Mock::given(method("DELETE"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({"data": {}})))
                .mount(&server)
                .await;
            let u = U::with_server(server.uri());
            login(&u, &server);
            fixture(&u);
            Cloud { u, server }
        }

        fn consent(&self) {
            Store::open(&self.u.p("data/usage.db"))
                .unwrap()
                .set_sync_consent(Utc::now())
                .unwrap();
        }

        fn store(&self) -> Store {
            Store::open(&self.u.p("data/usage.db")).unwrap()
        }

        async fn requests(&self, verb: &str, p: &str) -> Vec<wiremock::Request> {
            self.server
                .received_requests()
                .await
                .unwrap()
                .into_iter()
                .filter(|r| r.method.as_str() == verb && r.url.path() == p)
                .collect()
        }

        async fn puts(&self) -> Vec<wiremock::Request> {
            self.requests("PUT", PUT).await
        }

        async fn all_requests(&self) -> usize {
            self.server.received_requests().await.unwrap().len()
        }
    }

    fn login(u: &U, server: &MockServer) {
        let account = format!("{}:default", server.address());
        std::fs::create_dir_all(u.p("xdg/tmgr")).unwrap();
        let creds = u.p("xdg/tmgr/credentials.json");
        std::fs::write(
            &creds,
            json!({ account: {"access_token": "acc-token-AAAA1111", "refresh_token": "ref-token-BBBB2222", "expires_at": "2099-12-01T00:00:00Z"} })
                .to_string(),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&creds, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::fs::set_permissions(u.p("xdg/tmgr"), std::fs::Permissions::from_mode(0o700)).unwrap();
    }

    fn fixture(u: &U) {
        u.seed_prices();
        u.session_file(
            "-s/sess-a.jsonl",
            &[
                user_line("sess-a", &at(-2, "11:59:00")),
                Ev::new(
                    "sess-a",
                    at(-2, "12:00:00"),
                    "a1",
                    "claude-test-opus-1",
                    1000,
                    100,
                )
                .line(),
                Ev::new(
                    "sess-a",
                    at(-2, "12:05:00"),
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
                at(-2, "12:10:00"),
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
            .line()],
        );
        u.session_file(
            "-s/sess-c.jsonl",
            &[Ev::new(
                "sess-c",
                at(-2, "12:20:00"),
                "c1",
                "claude-test-sonnet-1",
                10,
                10,
            )
            .cwd("/work/gamma")
            .line()],
        );
        tree_extras(u, "/work/alpha");
    }

    fn results(v: &Value) -> Vec<(String, String, String)> {
        v["data"]["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r["workspace"].as_str().unwrap().to_string(),
                    r["ref"].as_str().unwrap().to_string(),
                    r["outcome"].as_str().unwrap().to_string(),
                )
            })
            .collect()
    }

    fn outcome_of(v: &Value, task_ref: &str) -> String {
        results(v)
            .into_iter()
            .find(|r| r.1 == task_ref)
            .unwrap_or_else(|| panic!("{task_ref} missing in {v}"))
            .2
    }

    fn without_clock(mut v: Value) -> Value {
        v.as_object_mut().unwrap().remove("generated_at");
        v
    }

    fn body(r: &wiremock::Request) -> Value {
        serde_json::from_slice(&r.body).unwrap()
    }

    fn run_sync(c: &Cloud, extra: &[&str]) -> (std::process::Output, Value) {
        let mut args = vec!["--json", "usage", "sync"];
        args.extend_from_slice(extra);
        let out = c.u.run(&args);
        let v = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
        (out, v)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_without_consent_is_refused_and_dry_run_sends_nothing() {
        let c = Cloud::new(vec![]).await;
        let refused = c.u.run(&["usage", "sync"]);
        assert!(!refused.status.success());
        assert!(
            text(&refused.stderr).contains("usage sync on"),
            "{}",
            text(&refused.stderr)
        );
        assert_eq!(c.all_requests().await, 0);

        let v = c.u.json(&["usage", "sync", "--dry-run"]);
        assert_valid("usage_sync", &v);
        assert_eq!(v["data"]["dry_run"], true);
        let rows = results(&v);
        assert_eq!(rows.len(), 3, "{v}");
        assert!(rows.iter().all(|r| r.2 == "would_send"));
        for r in v["data"]["results"].as_array().unwrap() {
            assert_eq!(r["aggregate"]["schema"], "task_usage.v1");
        }
        let shown = text(&c.u.ok(&["usage", "sync", "--dry-run"]).stdout);
        assert!(
            shown.contains("would send") && shown.contains("\"schema\": \"task_usage.v1\""),
            "{shown}"
        );
        assert!(c.puts().await.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn each_task_is_put_once_with_the_dry_run_body() {
        let c = Cloud::new(vec![]).await;
        let dry = c.u.json(&["usage", "sync", "--dry-run"]);
        c.consent();
        let out = c.u.ok(&["usage", "sync"]);
        println!("{}", text(&out.stdout));
        let shown = text(&out.stdout);
        assert!(shown.contains("TM-5") && shown.contains("sent"), "{shown}");
        let puts = c.puts().await;
        assert_eq!(puts.len(), 3);
        for r in &puts {
            let parsed = body(r);
            assert_eq!(r.body, serde_json::to_vec(&parsed).unwrap());
            let task_ref = parsed["task"]["ref"].as_str().unwrap();
            let planned = dry["data"]["results"]
                .as_array()
                .unwrap()
                .iter()
                .find(|d| d["ref"] == task_ref)
                .unwrap();
            assert_eq!(
                without_clock(parsed.clone()),
                without_clock(planned["aggregate"].clone())
            );
            assert_eq!(parsed["task"]["workspace"], planned["workspace"]);
            assert_eq!(
                r.headers.get("authorization").unwrap().to_str().unwrap(),
                "Bearer acc-token-AAAA1111"
            );
        }
        let (out, v) = run_sync(&c, &["--no-scan"]);
        assert_eq!(out.status.code(), Some(0));
        assert_valid("usage_sync", &v);
        assert_eq!(v["data"]["counts"]["unchanged"], 3, "{v}");
        assert_eq!(c.puts().await.len(), 3);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_stale_answer_is_reported_and_not_recorded() {
        let stale = ResponseTemplate::new(200).set_body_json(json!({"data": {
            "task_id": 5, "source_id": "x", "generated_at": "2026-10-16T12:00:00Z",
            "stale": true, "updated_at": "2026-10-16T12:00:01Z"
        }}));
        let c = Cloud::new(vec![put_mock(stale)]).await;
        c.consent();
        let (out, v) = run_sync(&c, &[]);
        assert_eq!(out.status.code(), Some(0));
        assert_valid("usage_sync", &v);
        assert_eq!(v["data"]["counts"]["stale"], 3, "{v}");
        assert_eq!(c.store().ledger_count().unwrap(), 0);
        run_sync(&c, &["--no-scan"]);
        assert_eq!(c.puts().await.len(), 6);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn less_usage_than_uploaded_is_refused_unless_forced() {
        let c = Cloud::new(vec![]).await;
        c.consent();
        run_sync(&c, &[]);
        let store = c.store();
        let mut last = store.ledger_get("personal", "TM-5").unwrap().unwrap();
        last.events += 10;
        last.digest = "other".into();
        store.ledger_put(&last).unwrap();

        let (out, v) = run_sync(&c, &["--no-scan"]);
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(outcome_of(&v, "TM-5"), "refused_shrink", "{v}");
        assert_eq!(c.puts().await.len(), 3);
        assert!(v["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["code"] == "usage_sync_shrink"));

        let (out, v) = run_sync(&c, &["--no-scan", "--task", "TM-5", "--force"]);
        assert_eq!(out.status.code(), Some(0), "{v}");
        assert_eq!(outcome_of(&v, "TM-5"), "sent");
        assert_eq!(c.puts().await.len(), 4);
        let bad = c.u.run(&["usage", "sync", "--force"]);
        assert_eq!(bad.status.code(), Some(2));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_disabled_server_setting_stops_the_run_with_the_settings_hint() {
        let c = Cloud::new(vec![put_mock(
            ResponseTemplate::new(403).set_body_json(json!({"error": "agent_usage_sync_disabled"})),
        )])
        .await;
        c.consent();
        let out = c.u.run(&["usage", "sync"]);
        assert_eq!(out.status.code(), Some(1));
        let shown = text(&out.stdout);
        assert!(
            shown.contains("Turn on Agent usage sync in TMGR Settings → Agent work"),
            "{shown}"
        );
        assert_eq!(c.puts().await.len(), 1);
        let (_, v) = run_sync(&c, &["--no-scan"]);
        assert_valid("usage_sync", &v);
        assert_eq!(v["data"]["stopped"]["code"], "sync_disabled");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_missing_scope_stops_the_run_with_a_login_hint() {
        let c = Cloud::new(vec![put_mock(
            ResponseTemplate::new(403).set_body_json(json!({"error": "insufficient_scope"})),
        )])
        .await;
        c.consent();
        let out = c.u.run(&["usage", "sync"]);
        assert_eq!(out.status.code(), Some(1));
        let shown = text(&out.stdout);
        assert!(
            shown.contains("tmgr login --scopes workspaces:read,tasks:read,tasks:write,tasks:create,comments:create,time:read,time:write,agent-usage:write"),
            "{shown}"
        );
        assert_eq!(c.puts().await.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_workspace_with_agent_work_off_is_skipped_and_the_rest_continue() {
        let off = Mock::given(method("PUT"))
            .and(path(PUT))
            .and(body_partial_json(json!({"task": {"workspace": "work"}})))
            .respond_with(
                ResponseTemplate::new(403).set_body_json(json!({"error": "feature_disabled"})),
            );
        let c = Cloud::new(vec![off]).await;
        c.consent();
        let (out, v) = run_sync(&c, &[]);
        assert_eq!(out.status.code(), Some(0), "{v}");
        assert_eq!(outcome_of(&v, "TM-5"), "sent");
        assert_eq!(outcome_of(&v, "TM-7"), "sent");
        assert_eq!(outcome_of(&v, "WK-1"), "skipped");
        assert!(v["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["code"] == "usage_sync_workspace_off"));
        assert_eq!(c.puts().await.len(), 3);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_rejected_task_fails_the_run_but_the_others_go_on() {
        let reject = Mock::given(method("PUT"))
            .and(path(PUT))
            .and(body_partial_json(json!({"task": {"ref": "TM-5"}})))
            .respond_with(
                ResponseTemplate::new(422).set_body_json(json!({"message": "unknown task"})),
            );
        let c = Cloud::new(vec![reject]).await;
        c.consent();
        let (out, v) = run_sync(&c, &[]);
        assert_eq!(out.status.code(), Some(1));
        assert_valid("usage_sync", &v);
        assert_eq!(outcome_of(&v, "TM-5"), "failed");
        assert_eq!(outcome_of(&v, "TM-7"), "sent");
        assert_eq!(outcome_of(&v, "WK-1"), "sent");
        assert!(v["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["code"] == "usage_sync_failed"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_rate_limit_is_retried_once_and_then_stops_the_run() {
        let once = put_mock(ResponseTemplate::new(429).set_body_json(json!({}))).up_to_n_times(1);
        let c = Cloud::new(vec![once]).await;
        c.consent();
        let (out, v) = run_sync(&c, &[]);
        assert_eq!(out.status.code(), Some(0), "{v}");
        assert_eq!(v["data"]["counts"]["sent"], 3);
        assert_eq!(c.puts().await.len(), 4);

        let always = Cloud::new(vec![put_mock(
            ResponseTemplate::new(429).set_body_json(json!({})),
        )])
        .await;
        always.consent();
        let (out, v) = run_sync(&always, &[]);
        assert_eq!(out.status.code(), Some(1));
        assert_eq!(v["data"]["stopped"]["code"], "rate_limited");
        assert_eq!(always.puts().await.len(), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn local_workspaces_are_never_uploaded() {
        let c = Cloud::new(vec![]).await;
        c.u.session_file(
            "-s/sess-d.jsonl",
            &[Ev::new(
                "sess-d",
                at(-2, "14:00:00"),
                "d1",
                "claude-test-opus-1",
                500,
                50,
            )
            .line()],
        );
        let ws = Local::workspace(&c.u, "loc");
        ws.task("loc", "One");
        ws.run(
            "loc",
            1,
            Some("sess-d"),
            None,
            &at(-2, "13:50:00"),
            &at(-2, "14:30:00"),
        );
        c.consent();
        let (out, v) = run_sync(&c, &[]);
        assert_eq!(out.status.code(), Some(0), "{v}");
        assert_valid("usage_sync", &v);
        let local: Vec<_> = results(&v)
            .into_iter()
            .filter(|r| r.0.starts_with("local:"))
            .collect();
        assert_eq!(local.len(), 1, "{v}");
        assert_eq!(local[0].2, "skipped");
        let puts = c.puts().await;
        assert_eq!(puts.len(), 3);
        assert!(puts.iter().all(|r| !text(&r.body).contains("local:")));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_workspace_prefixed_ref_is_split_into_workspace_and_ref() {
        let personal = vec![run_json(
            10,
            5,
            "TM-5",
            "sess-a",
            at(-2, "11:55:00"),
            at(-2, "12:30:00"),
        )];
        let work = vec![run_json(
            20,
            15,
            "TM-5",
            "sess-c",
            at(-2, "12:15:00"),
            at(-2, "12:40:00"),
        )];
        let c = Cloud::build(personal, work, vec![]).await;
        c.consent();
        let (out, v) = run_sync(&c, &[]);
        assert_eq!(out.status.code(), Some(0), "{v}");
        let mut seen: Vec<(String, String, u64)> = c
            .puts()
            .await
            .iter()
            .map(|r| {
                let b = body(r);
                (
                    b["task"]["workspace"].as_str().unwrap().to_string(),
                    b["task"]["ref"].as_str().unwrap().to_string(),
                    b["task"]["id"].as_u64().unwrap(),
                )
            })
            .collect();
        seen.sort();
        assert_eq!(
            seen,
            vec![
                ("personal".to_string(), "TM-5".to_string(), 5),
                ("work".to_string(), "TM-5".to_string(), 15)
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn forget_deletes_on_the_server_and_needs_yes_without_a_terminal() {
        let task = Mock::given(method("GET"))
            .and(path("/api/tasks/by-key/TM-5"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({"data": common::task_json(5, "TM-5: x")})),
            );
        let c = Cloud::new(vec![task]).await;
        c.consent();
        run_sync(&c, &[]);
        assert_eq!(c.store().ledger_count().unwrap(), 3);

        let refused = c.u.run(&["usage", "sync", "forget"]);
        assert_eq!(refused.status.code(), Some(10));
        assert!(c.requests("DELETE", PUT).await.is_empty());
        assert_eq!(c.store().ledger_count().unwrap(), 3);

        let v = c.u.json(&["--yes", "usage", "sync", "forget"]);
        assert_valid("usage_sync", &v);
        assert_eq!(c.requests("DELETE", PUT).await.len(), 1);
        assert_eq!(c.store().ledger_count().unwrap(), 0);

        run_sync(&c, &["--no-scan"]);
        assert_eq!(c.puts().await.len(), 6);
        c.u.ok(&["--yes", "usage", "sync", "forget", "--task", "TM-5"]);
        assert_eq!(
            c.requests("DELETE", "/api/tasks/5/agent-usage").await.len(),
            1
        );
        assert_eq!(c.requests("DELETE", PUT).await.len(), 1);
        assert_eq!(c.store().ledger_count().unwrap(), 2);
        assert!(c.store().ledger_get("personal", "TM-5").unwrap().is_none());
    }

    fn plant(u: &U) {
        let conn = Connection::open(u.p("data/usage.db")).unwrap();
        let sets = [
            ("usage_fact", "name = ?1, detail = ?2, target = ?3"),
            (
                "usage_agent",
                "originator = ?1, source = ?2, version = ?3, entrypoint = ?1",
            ),
            (
                "usage_event",
                "model = ?1, git_branch = ?2, cwd = ?3, project_name = ?1",
            ),
            (
                "usage_session",
                "git_branch = ?1, cwd = ?2, project_name = ?3",
            ),
        ];
        for (table, set) in sets {
            for (i, args) in [[A, B, C], [B, C, A], [C, A, B]].iter().enumerate() {
                conn.execute(
                    &format!("UPDATE {table} SET {set} WHERE rowid % 3 = {i}"),
                    rusqlite::params![args[0], args[1], args[2]],
                )
                .unwrap();
            }
        }
        conn.execute(
            "UPDATE usage_agent SET agent_type = ?1 WHERE agent_id != ''",
            [C],
        )
        .unwrap();
    }

    async fn planted_outputs(c: &Cloud) -> Vec<String> {
        plant(&c.u);
        c.consent();
        let dry_json =
            c.u.ok(&["--json", "usage", "sync", "--dry-run", "--no-scan"]);
        let dry_text = c.u.ok(&["usage", "sync", "--dry-run", "--no-scan"]);
        let real_json = c.u.ok(&["--json", "usage", "sync", "--no-scan"]);
        let real_text =
            c.u.ok(&["usage", "sync", "--force", "--task", "TM-5", "--no-scan"]);
        let mut seen: Vec<String> = [&dry_json, &dry_text, &real_json, &real_text]
            .iter()
            .flat_map(|o| [text(&o.stdout), text(&o.stderr)])
            .collect();
        let puts = c.puts().await;
        assert!(puts.len() >= 3);
        seen.extend(puts.iter().map(|r| text(&r.body)));
        seen
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn planted_paths_emails_and_prompts_never_leave_the_machine_or_reach_the_output() {
        let c = Cloud::new(vec![]).await;
        for haystack in &planted_outputs(&c).await {
            for needle in [A, B, C, "/Users", "@example.com", "PROMPT SECRET"] {
                assert!(!haystack.contains(needle), "{needle} leaked: {haystack}");
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn planted_values_do_not_survive_in_a_rewritten_form_either() {
        let c = Cloud::new(vec![]).await;
        for haystack in &planted_outputs(&c).await {
            for needle in ["someone", "alice", "SECRET_TEXT", "SECRET:TEXT"] {
                assert!(!haystack.contains(needle), "{needle} leaked: {haystack}");
            }
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_body_that_breaks_a_server_rule_is_not_sent() {
        let c = Cloud::new(vec![]).await;
        Connection::open(c.u.p("data/usage.db"))
            .unwrap()
            .execute("UPDATE usage_event SET cost_usd = 5000000", [])
            .unwrap();
        c.consent();
        let (out, v) = run_sync(&c, &["--no-scan"]);
        assert_eq!(out.status.code(), Some(0), "{v}");
        assert_eq!(outcome_of(&v, "TM-5"), "invalid", "{v}");
        assert!(c.puts().await.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn consent_commands_without_a_terminal_status_off_and_reset() {
        let c = Cloud::new(vec![]).await;
        let refused = c.u.run(&["usage", "sync", "on"]);
        assert_eq!(refused.status.code(), Some(10));
        let yes = c.u.run(&["--yes", "usage", "sync", "on"]);
        assert_eq!(yes.status.code(), Some(10));
        assert!(c.store().sync_consent().unwrap().is_none());
        assert_eq!(c.all_requests().await, 0);

        let v = c.u.json(&["usage", "sync", "status"]);
        assert_valid("usage_sync", &v);
        assert_eq!(v["data"]["enabled"], false);

        c.consent();
        run_sync(&c, &[]);
        let v = c.u.json(&["usage", "sync", "status"]);
        assert_valid("usage_sync", &v);
        assert_eq!(v["data"]["enabled"], true);
        assert_eq!(v["data"]["last_run"]["counts"]["sent"], 3);
        assert_eq!(v["data"]["tasks_uploaded"], 3);
        let shown = text(&c.u.ok(&["usage", "sync", "status"]).stdout);
        assert!(
            shown.contains("on since") && shown.contains("Last run"),
            "{shown}"
        );

        let off = c.u.json(&["usage", "sync", "off"]);
        assert_valid("usage_sync", &off);
        assert!(c.store().sync_consent().unwrap().is_none());
        assert_eq!(c.store().ledger_count().unwrap(), 3);

        c.consent();
        c.store()
            .ledger_put(&LedgerEntry {
                workspace: "personal".into(),
                task_ref: "TM-1".into(),
                task_id: None,
                generated_at: "2026-10-16T12:00:00Z".into(),
                events: 1,
                cost_usd: 1.0,
                digest: "d".into(),
                first_event_at: None,
                last_event_at: None,
            })
            .unwrap();
        c.u.ok(&["--yes", "usage", "reset"]);
        assert!(c.store().sync_consent().unwrap().is_none());
        assert_eq!(c.store().ledger_count().unwrap(), 0);
        assert!(c.store().sync_last_run().unwrap().is_none());
    }

    fn terminal(c: &Cloud, args: &[&str], input: &str) -> (i32, String) {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 50,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_tmgr"));
        cmd.args(args);
        cmd.env_clear();
        cmd.cwd(c.u.p("home"));
        for (k, v) in [
            ("HOME", c.u.p("home").into_os_string()),
            ("XDG_DATA_HOME", c.u.p("xdgdata").into_os_string()),
            ("XDG_CONFIG_HOME", c.u.p("xdg").into_os_string()),
            ("XDG_STATE_HOME", c.u.p("state").into_os_string()),
            ("TMGR_USAGE_DIR", c.u.p("data").into_os_string()),
            ("CLAUDE_CONFIG_DIR", c.u.p("claude").into_os_string()),
            ("CODEX_HOME", c.u.p("codex").into_os_string()),
            (
                "TMGR_LOCAL_ROOT",
                c.u.p("tmgr-home/workspaces").into_os_string(),
            ),
            ("TMGR_CREDENTIAL_STORE", "file".into()),
            ("TMGR_SERVER", c.u.server.clone().into()),
            ("TZ", "UTC".into()),
            ("NO_COLOR", "1".into()),
        ] {
            cmd.env(k, v);
        }
        let mut child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let mut writer = pair.master.take_writer().unwrap();
        let _ = std::io::Write::write_all(&mut writer, input.as_bytes());
        let seen = std::thread::spawn(move || {
            let mut sink = Vec::new();
            let _ = std::io::Read::read_to_end(&mut reader, &mut sink);
            String::from_utf8_lossy(&sink).to_string()
        });
        let code = child.wait().unwrap().exit_code() as i32;
        drop(writer);
        (code, seen.join().unwrap())
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn on_shows_the_audience_and_a_sample_and_stores_consent_only_after_yes() {
        let c = Cloud::new(vec![]).await;
        let (code, shown) = terminal(&c, &["usage", "sync", "on"], "no\n");
        assert_ne!(code, 0, "{shown}");
        assert!(c.store().sync_consent().unwrap().is_none());
        let (code, shown) = terminal(&c, &["usage", "sync", "on"], "yes\nn\n");
        assert_eq!(code, 0, "{shown}");
        assert!(c.store().sync_consent().unwrap().is_some());
        for part in [
            "personal, work",
            "\"schema\": \"task_usage.v1\"",
            "Type yes",
            "Nothing is uploaded automatically",
        ] {
            assert!(shown.contains(part), "missing {part:?} in\n{shown}");
        }
        assert!(c.puts().await.is_empty());
    }

    fn auto_set(c: &Cloud) -> bool {
        c.store().sync_auto().unwrap().is_some()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn auto_on_needs_consent_then_a_terminal_or_the_flag() {
        let c = Cloud::new(vec![]).await;
        let no_consent = c.u.run(&["usage", "sync", "auto", "on", "--yes-auto"]);
        assert!(!no_consent.status.success());
        assert!(text(&no_consent.stderr).contains("usage sync on"));
        assert!(!auto_set(&c));

        c.consent();
        let refused = c.u.run(&["usage", "sync", "auto", "on"]);
        assert_eq!(refused.status.code(), Some(10));
        let yes = c.u.run(&["--yes", "usage", "sync", "auto", "on"]);
        assert_eq!(yes.status.code(), Some(10));
        assert!(!auto_set(&c));

        let shown = text(&c.u.run(&["usage", "sync", "auto", "on"]).stderr);
        assert!(shown.contains("--yes-auto"), "{shown}");

        let v = c.u.json(&["usage", "sync", "auto", "on", "--yes-auto"]);
        assert_valid("usage_sync", &v);
        assert_eq!(v["data"]["auto"], true);
        assert!(auto_set(&c));

        let v = c.u.json(&["usage", "sync", "status"]);
        assert_valid("usage_sync", &v);
        assert_eq!(v["data"]["auto"], true);
        let v = c.u.json(&["usage", "sync", "auto", "status"]);
        assert_valid("usage_sync", &v);
        assert_eq!(v["data"]["auto"], true);

        let v = c.u.json(&["usage", "sync", "auto", "off"]);
        assert_valid("usage_sync", &v);
        assert_eq!(v["data"]["auto"], false);
        assert!(!auto_set(&c));
        assert!(c.store().sync_consent().unwrap().is_some());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_off_clears_auto_and_a_later_consent_does_not_bring_it_back() {
        let c = Cloud::new(vec![]).await;
        c.consent();
        c.store().set_sync_auto(Utc::now()).unwrap();
        c.u.ok(&["usage", "sync", "off"]);
        assert!(!auto_set(&c));
        c.consent();
        let v = c.u.json(&["usage", "sync", "auto", "status"]);
        assert_eq!(v["data"]["auto"], false);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn auto_trigger_is_a_silent_no_op_unless_consent_and_auto_are_on() {
        let c = Cloud::new(vec![]).await;
        let trigger = ["usage", "sync", "--auto", "--task-id", "5", "-q"];
        let out = c.u.run(&trigger);
        assert_eq!(out.status.code(), Some(0));
        assert!(out.stdout.is_empty() && out.stderr.is_empty());
        c.consent();
        let out = c.u.run(&trigger);
        assert_eq!(out.status.code(), Some(0));
        assert!(out.stdout.is_empty() && out.stderr.is_empty());
        c.store().clear_sync_consent().unwrap();
        c.store().set_sync_auto(Utc::now()).unwrap();
        let out = c.u.run(&trigger);
        assert_eq!(out.status.code(), Some(0));
        assert!(out.stdout.is_empty() && out.stderr.is_empty());
        assert_eq!(c.all_requests().await, 0);
        assert!(c.puts().await.is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn auto_trigger_uploads_only_the_buckets_of_that_task_id() {
        let c = Cloud::new(vec![]).await;
        c.consent();
        c.store().set_sync_auto(Utc::now()).unwrap();
        let out =
            c.u.run(&["usage", "sync", "--auto", "--task-id", "9", "-q"]);
        assert_eq!(out.status.code(), Some(0), "{}", text(&out.stderr));
        let puts = c.puts().await;
        assert_eq!(puts.len(), 1);
        assert_eq!(body(&puts[0])["task"]["ref"], "WK-1");
        assert_eq!(body(&puts[0])["task"]["workspace"], "work");
        assert_eq!(c.store().ledger_count().unwrap(), 1);
        let lock = std::fs::read_dir(c.u.p("data"))
            .unwrap()
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().ends_with(".lock"));
        assert!(!lock);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn auto_flags_belong_together() {
        let c = Cloud::new(vec![]).await;
        assert!(!c.u.run(&["usage", "sync", "--auto"]).status.success());
        assert!(!c
            .u
            .run(&["usage", "sync", "--task-id", "5"])
            .status
            .success());
        assert!(!c
            .u
            .run(&["usage", "sync", "--auto", "--task-id", "5", "--dry-run"])
            .status
            .success());
        assert_eq!(c.all_requests().await, 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn auto_on_in_a_terminal_asks_and_defaults_to_no() {
        let c = Cloud::new(vec![]).await;
        c.consent();
        let (code, shown) = terminal(&c, &["usage", "sync", "auto", "on"], "\n");
        assert_ne!(code, 0, "{shown}");
        assert!(!auto_set(&c));
        for part in [
            "task_usage.v1",
            "Never sent: prompts, replies, code, file contents, file paths or command lines",
            "tmgr usage sync auto off",
            "tmgr usage sync forget",
            "Also upload automatically after each agent run? [y/N]",
        ] {
            assert!(shown.contains(part), "missing {part:?} in\n{shown}");
        }
        let (code, shown) = terminal(&c, &["usage", "sync", "auto", "on"], "y\n");
        assert_eq!(code, 0, "{shown}");
        assert!(auto_set(&c));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn sync_on_ends_with_the_auto_question_and_declining_keeps_it_off() {
        let c = Cloud::new(vec![]).await;
        let (code, shown) = terminal(&c, &["usage", "sync", "on"], "yes\nn\n");
        assert_eq!(code, 0, "{shown}");
        assert!(c.store().sync_consent().unwrap().is_some());
        assert!(!auto_set(&c));
        assert!(shown.contains("Also upload automatically after each agent run?"));
        c.u.ok(&["usage", "sync", "off"]);
        let (code, shown) = terminal(&c, &["usage", "sync", "on"], "yes\ny\n");
        assert_eq!(code, 0, "{shown}");
        assert!(auto_set(&c));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_server_that_turned_sync_off_clears_the_ledger_so_re_enabling_resends() {
        let c = Cloud::new(vec![]).await;
        c.consent();
        run_sync(&c, &[]);
        assert_eq!(c.store().ledger_count().unwrap(), 3);
        put_mock(
            ResponseTemplate::new(403).set_body_json(json!({"error": "agent_usage_sync_disabled"})),
        )
        .with_priority(1)
        .up_to_n_times(1)
        .mount(&c.server)
        .await;
        let (out, v) = run_sync(&c, &["--no-scan", "--task", "TM-5", "--force"]);
        assert_eq!(out.status.code(), Some(1), "{v}");
        assert_eq!(c.store().ledger_count().unwrap(), 0);
        let (out, v) = run_sync(&c, &["--no-scan"]);
        assert_eq!(out.status.code(), Some(0), "{v}");
        assert_eq!(v["data"]["counts"]["sent"], 3, "{v}");
        assert_eq!(v["data"]["counts"]["unchanged"], 0, "{v}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_window_that_slid_past_uploaded_usage_is_refused_even_with_equal_totals() {
        let c = Cloud::new(vec![]).await;
        c.consent();
        run_sync(&c, &[]);
        let store = c.store();
        let mut last = store.ledger_get("personal", "TM-5").unwrap().unwrap();
        let first = last
            .first_event_at
            .clone()
            .expect("first event time is stored");
        assert!(last.last_event_at.is_some());
        last.first_event_at = Some("2000-01-01T00:00:00Z".into());
        last.digest = "other".into();
        store.ledger_put(&last).unwrap();
        assert!(first.as_str() > "2000-01-01T00:00:00Z");

        let (out, v) = run_sync(&c, &["--no-scan"]);
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(outcome_of(&v, "TM-5"), "refused_shrink", "{v}");
        assert_eq!(c.puts().await.len(), 3);

        let (_, v) = run_sync(&c, &["--no-scan", "--task", "TM-5", "--force"]);
        assert_eq!(outcome_of(&v, "TM-5"), "sent", "{v}");
        assert_eq!(c.puts().await.len(), 4);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn consent_withdrawn_between_uploads_stops_the_run() {
        let db = std::sync::Arc::new(std::sync::Mutex::new(None::<std::path::PathBuf>));
        let hook = db.clone();
        let withdraw = move |_: &wiremock::Request| {
            if let Some(path) = hook.lock().unwrap().clone() {
                Store::open(&path).unwrap().clear_sync_consent().unwrap();
            }
            put_ok()
        };
        let c = Cloud::new(vec![Mock::given(method("PUT"))
            .and(path(PUT))
            .respond_with(withdraw)
            .with_priority(1)])
        .await;
        *db.lock().unwrap() = Some(c.u.p("data/usage.db"));
        c.consent();
        let (out, v) = run_sync(&c, &[]);
        assert_eq!(out.status.code(), Some(1), "{v}");
        assert_eq!(c.puts().await.len(), 1);
        assert_eq!(v["data"]["stopped"]["message"], "consent withdrawn", "{v}");
        assert!(
            text(&out.stderr).contains("consent withdrawn")
                || text(&out.stdout).contains("consent withdrawn")
        );
        assert!(c.store().sync_consent().unwrap().is_none());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn one_ref_with_two_task_ids_is_never_uploaded() {
        let personal = vec![
            run_json(
                10,
                5,
                "TM-5",
                "sess-a",
                at(-2, "11:55:00"),
                at(-2, "12:30:00"),
            ),
            run_json(
                11,
                55,
                "TM-5",
                "sess-b",
                at(-1, "09:30:00"),
                at(-1, "10:30:00"),
            ),
        ];
        let c = Cloud::build(personal, vec![], vec![]).await;
        c.consent();
        let (out, v) = run_sync(&c, &[]);
        assert_eq!(out.status.code(), Some(0), "{v}");
        assert_eq!(outcome_of(&v, "TM-5"), "skipped", "{v}");
        assert!(c.puts().await.is_empty());
        assert!(v["warnings"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["message"]
                .as_str()
                .unwrap_or("")
                .contains("task id mismatch")));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn server_error_text_never_reaches_the_output() {
        let planted = "home/alice@example.com PROMPT SECRET";
        let reject = Mock::given(method("PUT"))
            .and(path(PUT))
            .and(body_partial_json(json!({"task": {"ref": "TM-5"}})))
            .respond_with(
                ResponseTemplate::new(422)
                    .set_body_json(json!({"message": planted, "error": "rate_limited"})),
            );
        let odd = Mock::given(method("PUT"))
            .and(path(PUT))
            .and(body_partial_json(json!({"task": {"ref": "TM-7"}})))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(json!({"message": planted, "error": planted})),
            );
        let c = Cloud::new(vec![reject, odd]).await;
        c.consent();
        let json_out = c.u.run(&["--json", "usage", "sync"]);
        let text_out =
            c.u.run(&["usage", "sync", "--force", "--task", "TM-5", "--no-scan"]);
        for o in [&json_out, &text_out] {
            for h in [text(&o.stdout), text(&o.stderr)] {
                for needle in ["alice", "example.com", "SECRET", "home/"] {
                    assert!(!h.contains(needle), "{needle} leaked: {h}");
                }
            }
        }
        let v: Value = serde_json::from_slice(&json_out.stdout).unwrap();
        let reasons: Vec<String> = v["data"]["results"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|r| r["reason"].as_str().map(String::from))
            .collect();
        assert!(
            reasons
                .iter()
                .any(|r| r.contains("422") && r.contains("rate_limited")),
            "{reasons:?}"
        );
        assert!(
            reasons
                .iter()
                .any(|r| r.contains("400") && r.contains("unknown")),
            "{reasons:?}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn consent_withdrawn_during_a_rate_limit_wait_stops_before_the_retry() {
        let db = std::sync::Arc::new(std::sync::Mutex::new(None::<std::path::PathBuf>));
        let hook = db.clone();
        let limit = move |_: &wiremock::Request| {
            if let Some(path) = hook.lock().unwrap().clone() {
                Store::open(&path).unwrap().clear_sync_consent().unwrap();
            }
            ResponseTemplate::new(429).set_body_json(json!({}))
        };
        let c = Cloud::new(vec![Mock::given(method("PUT"))
            .and(path(PUT))
            .respond_with(limit)
            .with_priority(1)])
        .await;
        *db.lock().unwrap() = Some(c.u.p("data/usage.db"));
        c.consent();
        let (out, v) = run_sync(&c, &[]);
        assert_eq!(out.status.code(), Some(1), "{v}");
        assert_eq!(c.puts().await.len(), 1);
        assert_eq!(v["data"]["stopped"]["message"], "consent withdrawn", "{v}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_server_error_on_forget_never_echoes_its_text() {
        let planted = "home/alice@example.com PROMPT SECRET";
        let c = Cloud::new(vec![]).await;
        Mock::given(method("DELETE"))
            .respond_with(
                ResponseTemplate::new(422)
                    .set_body_json(json!({"message": planted, "error": "not allowed!"})),
            )
            .with_priority(1)
            .mount(&c.server)
            .await;
        let out = c.u.run(&["usage", "sync", "forget", "--yes"]);
        assert!(!out.status.success());
        for h in [text(&out.stdout), text(&out.stderr)] {
            for needle in ["alice", "example.com", "SECRET", "home/"] {
                assert!(!h.contains(needle), "{needle} leaked: {h}");
            }
        }
        assert!(text(&out.stderr).contains("422"), "{}", text(&out.stderr));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_error_code_outside_the_server_contract_is_unknown() {
        let reject = Mock::given(method("PUT")).and(path(PUT)).respond_with(
            ResponseTemplate::new(422).set_body_json(json!({"error": "alice_at_example"})),
        );
        let c = Cloud::new(vec![reject]).await;
        c.consent();
        let out = c.u.run(&["--json", "usage", "sync"]);
        let h = text(&out.stdout);
        assert!(!h.contains("alice") && h.contains("unknown"), "{h}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn consent_withdrawn_during_a_token_refresh_stops_before_the_resend() {
        let db = std::sync::Arc::new(std::sync::Mutex::new(None::<std::path::PathBuf>));
        let hook = db.clone();
        let c = Cloud::new(vec![put_mock(ResponseTemplate::new(401))
            .with_priority(1)
            .up_to_n_times(1)])
        .await;
        *db.lock().unwrap() = Some(c.u.p("data/usage.db"));
        let withdraw = move |_: &wiremock::Request| {
            if let Some(path) = hook.lock().unwrap().clone() {
                Store::open(&path).unwrap().clear_sync_consent().unwrap();
            }
            ResponseTemplate::new(200).set_body_json(json!({"data": {
                "token": "new-access-CCCC", "token_type": "Bearer", "expires_in": 3600,
                "refresh_token": "new-refresh-DDDD", "refresh_expires_in": 86400
            }}))
        };
        Mock::given(method("POST"))
            .and(path("/api/auth/refresh"))
            .respond_with(withdraw)
            .mount(&c.server)
            .await;
        c.consent();
        let (out, v) = run_sync(&c, &[]);
        assert_eq!(out.status.code(), Some(1), "{v}");
        assert_eq!(c.puts().await.len(), 1);
        assert_eq!(c.requests("POST", "/api/auth/refresh").await.len(), 1);
        assert_eq!(v["data"]["stopped"]["message"], "consent withdrawn", "{v}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn consent_withdrawn_during_the_pacing_wait_sends_nothing_more() {
        let db = std::sync::Arc::new(std::sync::Mutex::new(None::<std::path::PathBuf>));
        let hook = db.clone();
        let withdraw_later = move |_: &wiremock::Request| {
            if let Some(path) = hook.lock().unwrap().clone() {
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    Store::open(&path).unwrap().clear_sync_consent().unwrap();
                });
            }
            put_ok()
        };
        let c = Cloud::new(vec![Mock::given(method("PUT"))
            .and(path(PUT))
            .respond_with(withdraw_later)
            .with_priority(1)])
        .await;
        *db.lock().unwrap() = Some(c.u.p("data/usage.db"));
        c.consent();
        let out =
            c.u.cmd()
                .args(["--json", "usage", "sync"])
                .env("TMGR_USAGE_SYNC_WAIT_MS", "400")
                .env("TMGR_USAGE_SYNC_PER_WINDOW", "1")
                .output()
                .unwrap();
        let v: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(out.status.code(), Some(1), "{v}");
        assert_eq!(c.puts().await.len(), 1);
        assert_eq!(v["data"]["stopped"]["message"], "consent withdrawn", "{v}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_failed_token_refresh_is_an_auth_error_with_the_login_hint() {
        let c = Cloud::new(vec![put_mock(ResponseTemplate::new(401)).with_priority(1)]).await;
        Mock::given(method("POST"))
            .and(path("/api/auth/refresh"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&c.server)
            .await;
        c.consent();
        let out = c.u.run(&["usage", "sync"]);
        assert_eq!(out.status.code(), Some(4), "{}", text(&out.stderr));
        assert!(
            text(&out.stderr).contains("tmgr login"),
            "{}",
            text(&out.stderr)
        );
        assert_eq!(c.puts().await.len(), 1);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn pull_requests_stay_local_and_are_never_uploaded() {
        let c = Cloud::new(vec![]).await;
        let mut pr = call_fact(
            "sess-a",
            None,
            "pr1",
            "secret-owner/hidden-repo",
            "12:08:30",
        );
        pr.kind = Some(FactKind::Pr);
        pr.value = Some(77);
        c.store().record_fact(Tool::Claude, &pr).unwrap();

        let local = c.u.json(&["usage", "task", "TM-5", "--no-scan"]);
        assert_eq!(
            local["data"]["aggregate"]["context"]["prs"],
            json!([{"repo": "secret-owner/hidden-repo", "number": 77}]),
            "{local}"
        );

        let (code, sample) = terminal(&c, &["usage", "sync", "on"], "yes\nn\n");
        assert_eq!(code, 0, "{sample}");
        let dry_json =
            c.u.ok(&["--json", "usage", "sync", "--dry-run", "--no-scan"]);
        let dry_text = c.u.ok(&["usage", "sync", "--dry-run", "--no-scan"]);
        let (out, v) = run_sync(&c, &["--no-scan"]);
        assert_eq!(out.status.code(), Some(0), "{v}");
        let puts = c.puts().await;
        assert_eq!(puts.len(), 3);
        for r in &puts {
            assert_eq!(body(r)["context"]["prs"], json!([]));
        }
        let mut seen = vec![sample, text(&dry_json.stdout), text(&dry_text.stdout)];
        seen.extend(puts.iter().map(|r| text(&r.body)));
        for h in seen {
            for needle in ["secret-owner", "hidden-repo"] {
                assert!(!h.contains(needle), "{needle} leaked: {h}");
            }
        }
    }
}
