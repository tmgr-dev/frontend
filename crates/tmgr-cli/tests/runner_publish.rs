#![cfg(feature = "runner")]

mod common;

use common::{git, init_repo, write_script, Env, ACCESS};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

const KEY: &str =
    "BImU1e2SPEdD/HKLzjfuQlNuLgKE44/qEF5a+626Bj0KtbPSIGIwmRlfUMIyP75vzLHw7BUXRArO87Ad8xnmazE=";
const KEY_ID: &str = "0b6f1c2e-6a7d-4e8f-9a10-1b2c3d4e5f60";
const BRANCH: &str = "tmgr/tm-410-fix-login";
const PR: &str = "https://github.com/acme/widgets/pull/5";

async fn mount(env: &Env, verb: &str, p: &str, body: Value) {
    Mock::given(method(verb))
        .and(path(p))
        .and(header("authorization", format!("Bearer {ACCESS}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&env.server)
        .await;
}

struct Rig {
    env: Env,
    run_dir: tempfile::TempDir,
    repo: PathBuf,
    clone: PathBuf,
    bare: PathBuf,
    stub: PathBuf,
}

impl Rig {
    async fn new(agent_runs: Value) -> Rig {
        let env = Env::new().await;
        env.login_seed();
        let root = env.tmp.path().to_path_buf();
        let repo = init_repo(&root.join("code/repo"));
        git(
            &repo,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/acme/widgets.git",
            ],
        );
        git(&repo, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        git(
            &repo,
            &[
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
                "refs/remotes/origin/main",
            ],
        );
        let clone = root.join("code/repo-worktrees/tmgr-tm-410-fix-login");
        git(
            &root,
            &[
                "clone",
                "-q",
                repo.to_str().unwrap(),
                clone.to_str().unwrap(),
            ],
        );
        git(&clone, &["checkout", "-q", "-b", BRANCH]);
        std::fs::write(clone.join("fix.txt"), "fixed\n").unwrap();
        git(&clone, &["add", "fix.txt"]);
        git(&clone, &["commit", "-q", "-m", "fix login"]);
        let bare = root.join("remote.git");
        git(&root, &["init", "-q", "--bare", bare.to_str().unwrap()]);
        std::fs::write(
            root.join(".gitconfig"),
            format!(
                "[url \"{}\"]\n\tinsteadOf = https://github.com/acme/widgets.git\n",
                bare.display()
            ),
        )
        .unwrap();
        let stub = root.join("stub");
        write_script(
            &stub,
            "gh",
            &format!(
                "printf '%s\\n' \"$@\" > \"{}/gh-argv.txt\"\necho {PR}",
                root.display()
            ),
        );
        let rig = Rig {
            env,
            run_dir: tempfile::Builder::new()
                .prefix("rp")
                .tempdir_in("/tmp")
                .unwrap(),
            repo,
            clone,
            bare,
            stub,
        };
        rig.seed(agent_runs).await;
        rig
    }

    async fn seed(&self, agent_runs: Value) {
        let env = &self.env;
        mount(
            env,
            "GET",
            "/api/auth/devices/current",
            json!({"data": {"device_id": 9, "user": {"id": 1, "name": "Ann"}, "scopes": ["tasks:read"]}}),
        )
        .await;
        mount(
            env,
            "GET",
            "/api/workspaces",
            json!({"data": [
                {"id": 99, "name": "Decoy", "code": "decoy", "is_default": true},
                {"id": 56, "name": "Tmgr Dev", "code": "tmgrdev", "is_default": false}]}),
        )
        .await;
        mount(
            env,
            "GET",
            "/api/tasks/9002",
            common::task_json(9002, "Fix login"),
        )
        .await;
        mount(
            env,
            "GET",
            "/api/agent-runs/tasks/9002",
            json!({"data": {
                "task_id": 9002, "workspace_id": 56, "workspace_code": "tmgrdev", "viewer_user_id": 1,
                "title": "Fix login", "description": "x", "task_key": "TM-410", "setting_enabled": true,
                "category": {"id": 127, "code": "TM", "github_full_name": "acme/widgets", "default_branch": "main"},
                "approval": null
            }}),
        )
        .await;
        mount(
            env,
            "GET",
            "/api/tasks/9002/agent-work",
            json!({"data": {"runs": agent_runs}}),
        )
        .await;
        mount(
            env,
            "POST",
            "/api/agent-work/77/finish",
            json!({"data": {"id": 77, "status": "succeeded", "pr_url": PR}}),
        )
        .await;
        mount(
            env,
            "GET",
            "/api/workspaces/56/statuses",
            json!({"data": [{"id": 1, "name": "Todo", "type": "default"}, {"id": 4, "name": "Review", "type": "review"}]}),
        )
        .await;
        mount(env, "PUT", "/api/tasks/9002/4", json!({"data": {}})).await;
        mount(env, "POST", "/api/tasks/9002/comments", json!({"data": {}})).await;
        std::fs::create_dir_all(env.config_dir()).unwrap();
        let allow = json!({"version": 1, "entries": [{
            "server": env.server.address().to_string(), "user_id": 1, "workspace_id": 56,
            "workspace_code": "tmgrdev", "allowed_at": "2025-10-01T00:00:00Z", "expires_at": null,
            "keys": [{"key_id": KEY_ID, "public_key": KEY, "device_name": "laptop"}],
            "repos": {"127": self.repo.display().to_string()}}]});
        std::fs::write(
            env.config_dir().join("runner-allow.json"),
            allow.to_string(),
        )
        .unwrap();
        let runs = env.state_dir().join("task-runs");
        std::fs::create_dir_all(&runs).unwrap();
        let record = json!({"repo": self.repo, "clone": self.clone, "branch": BRANCH,
            "cloud": true, "session": "s_none", "agent": "claude-code"});
        std::fs::write(runs.join("tmgrdev-9002.json"), record.to_string()).unwrap();
    }

    fn cmd(&self, args: &[&str]) -> std::process::Output {
        let mut c = self.env.cmd();
        c.env("PATH", common::path_with(Some(&self.stub)))
            .env("TMGR_RUNNER_DIR", self.run_dir.path().join("r"))
            .env("TMGR_RUNNER_HOMES", self.run_dir.path().join("h"))
            .args(args);
        c.output().unwrap()
    }

    fn publish(&self) -> std::process::Output {
        self.cmd(&["--ws", "tmgrdev", "run", "9002", "--publish"])
    }

    fn gh_argv(&self) -> Option<Vec<String>> {
        let t = std::fs::read_to_string(self.env.tmp.path().join("gh-argv.txt")).ok()?;
        Some(t.lines().map(str::to_string).collect())
    }

    async fn calls(&self) -> Vec<(String, String, String)> {
        self.env
            .server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .map(|r| {
                (
                    r.method.to_string(),
                    r.url.path().to_string(),
                    String::from_utf8_lossy(&r.body).to_string(),
                )
            })
            .collect()
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = self.cmd(&["runner", "stop"]);
    }
}

fn running_run() -> Value {
    json!([{"id": 77, "status": "running", "agent": "claude-code", "branch": BRANCH}])
}

fn stderr(o: &std::process::Output) -> String {
    common::text(&o.stderr)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn publish_is_refused_while_the_runner_state_is_unknown() {
    let rig = Rig::new(running_run()).await;
    let out = rig.publish();
    assert_ne!(out.status.code(), Some(0));
    assert!(
        stderr(&out).contains("runner is not running"),
        "{}",
        stderr(&out)
    );
    assert!(rig.gh_argv().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_finished_run_is_pushed_opened_as_a_pr_and_reported_without_merging() {
    let rig = Rig::new(running_run()).await;
    let start = rig.cmd(&["runner", "start"]);
    assert!(start.status.success(), "{}", stderr(&start));
    let out = rig.publish();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(common::text(&out.stdout).contains(PR));
    let pushed = git(&rig.bare, &["rev-parse", &format!("refs/heads/{BRANCH}")]);
    assert_eq!(
        pushed,
        git(&rig.repo, &["rev-parse", &format!("refs/heads/{BRANCH}")])
    );
    let argv = rig.gh_argv().expect("gh ran");
    assert_eq!(&argv[..2], ["pr", "create"]);
    assert!(
        argv.iter().all(|a| a != "merge" && a != "--auto"),
        "{argv:?}"
    );
    assert!(argv.windows(2).any(|w| w == ["--base", "main"]));
    assert!(argv.windows(2).any(|w| w == ["--head", BRANCH]));
    let calls = rig.calls().await;
    let finish = calls
        .iter()
        .find(|c| c.1 == "/api/agent-work/77/finish")
        .expect("finish call");
    assert!(
        finish.2.contains(PR) && finish.2.contains("succeeded"),
        "{}",
        finish.2
    );
    assert!(calls
        .iter()
        .any(|c| c.0 == "PUT" && c.1 == "/api/tasks/9002/4"));
    let comment = calls
        .iter()
        .find(|c| c.1 == "/api/tasks/9002/comments")
        .expect("comment");
    assert!(comment.2.contains(PR));
    assert!(calls.iter().any(|c| c.1 == "/api/workspaces/56/statuses"));
    assert!(calls.iter().all(|c| !c.1.contains("/99/")), "{calls:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_clone_on_another_branch_publishes_nothing() {
    let rig = Rig::new(running_run()).await;
    assert!(rig.cmd(&["runner", "start"]).status.success());
    git(&rig.clone, &["checkout", "-q", "-b", "main-copy"]);
    let out = rig.publish();
    assert_ne!(out.status.code(), Some(0));
    assert!(stderr(&out).contains("pinned"), "{}", stderr(&out));
    assert!(rig.gh_argv().is_none());
    assert!(git(&rig.bare, &["branch", "--list"]).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_workflow_change_or_an_empty_run_publishes_nothing() {
    let rig = Rig::new(running_run()).await;
    assert!(rig.cmd(&["runner", "start"]).status.success());
    std::fs::create_dir_all(rig.clone.join(".github/workflows")).unwrap();
    std::fs::write(rig.clone.join(".github/workflows/ci.yml"), "on: push\n").unwrap();
    git(&rig.clone, &["add", "-A"]);
    git(&rig.clone, &["commit", "-q", "-m", "ci"]);
    let out = rig.publish();
    assert_ne!(out.status.code(), Some(0));
    assert!(stderr(&out).contains(".github"), "{}", stderr(&out));
    assert!(rig.gh_argv().is_none());
    assert!(!Path::new(&rig.bare).join("refs/heads/tmgr").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn publish_is_refused_while_the_run_is_live() {
    let rig = Rig::new(running_run()).await;
    assert!(rig.cmd(&["runner", "start"]).status.success());
    let started = rig.cmd(&[
        "--json",
        "run",
        "--agent-cmd",
        "/bin/sleep 60",
        "--no-attach",
        "--cwd",
        rig.clone.to_str().unwrap(),
    ]);
    if !started.status.success() {
        eprintln!("skipped: no sandboxed sessions here: {}", stderr(&started));
        return;
    }
    let v: Value = serde_json::from_slice(&started.stdout).unwrap();
    let id = v["data"]["id"].as_str().unwrap().to_string();
    let record = rig.env.state_dir().join("task-runs/tmgrdev-9002.json");
    let mut r: Value = serde_json::from_str(&std::fs::read_to_string(&record).unwrap()).unwrap();
    r["session"] = json!(id);
    std::fs::write(&record, r.to_string()).unwrap();
    let out = rig.publish();
    assert_ne!(out.status.code(), Some(0));
    assert!(stderr(&out).contains("still live"), "{}", stderr(&out));
    assert!(rig.gh_argv().is_none());
    let _ = rig.cmd(&["halt", &id, "--kill"]);
}
