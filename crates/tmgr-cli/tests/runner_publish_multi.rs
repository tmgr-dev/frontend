#![cfg(feature = "runner")]

mod common;

use common::{git, init_repo, write_script, Env, ACCESS};
use portable_pty::{native_pty_system, Child, CommandBuilder, PtySize};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, ResponseTemplate};

const FIXTURES: &str = include_str!("../../tmgr-core/tests/fixtures/agent-approval-fixtures.json");
const BRANCH: &str = "tmgr/tm-411-backend-and-frontend";
const NAMES: [&str; 2] = ["tmgr-dev/backend", "tmgr-dev/frontend"];

async fn mount(env: &Env, verb: &str, p: &str, body: Value) {
    Mock::given(method(verb))
        .and(path(p))
        .and(header("authorization", format!("Bearer {ACCESS}").as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(&env.server)
        .await;
}

fn sign_v2(host: &str, f: &Value, c: &Value) -> String {
    use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
    use base64::Engine;
    use p256::ecdsa::{signature::Signer, Signature, SigningKey};
    let d = URL_SAFE_NO_PAD
        .decode(f["key"]["private_jwk"]["d"].as_str().unwrap())
        .unwrap();
    let key = SigningKey::from_slice(&d).unwrap();
    let msg = format!(
        "tmgr-agent-approval:v2:{host}:1:56:9100:127:{}:{}:{}:1760000100",
        c["content_sha256"].as_str().unwrap(),
        c["repos_sha256"].as_str().unwrap(),
        f["key"]["key_id"].as_str().unwrap()
    );
    let sig: Signature = key.sign(msg.as_bytes());
    STANDARD.encode(sig.to_bytes())
}

struct Rig {
    env: Env,
    run_dir: tempfile::TempDir,
    repos: [PathBuf; 2],
    bares: [PathBuf; 2],
    run: PathBuf,
    stub: PathBuf,
}

impl Rig {
    async fn new() -> Rig {
        let env = Env::new().await;
        env.login_seed();
        let root = env.tmp.path().to_path_buf();
        let mut config = String::new();
        let mut repos = Vec::new();
        let mut bares = Vec::new();
        for name in NAMES {
            let short = name.split('/').nth(1).unwrap();
            let repo = init_repo(&root.join("code").join(short));
            let url = format!("https://github.com/{name}.git");
            git(&repo, &["remote", "add", "origin", &url]);
            git(&repo, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
            git(
                &repo,
                &[
                    "symbolic-ref",
                    "refs/remotes/origin/HEAD",
                    "refs/remotes/origin/main",
                ],
            );
            let bare = root.join(format!("{short}-remote.git"));
            git(&root, &["init", "-q", "--bare", bare.to_str().unwrap()]);
            config.push_str(&format!(
                "[url \"{}\"]\n\tinsteadOf = {url}\n",
                bare.display()
            ));
            repos.push(repo);
            bares.push(bare);
        }
        std::fs::write(root.join(".gitconfig"), config).unwrap();
        let run = root.join("code/backend-worktrees/tmgr-tm-411-backend-and-frontend");
        std::fs::create_dir_all(&run).unwrap();
        let run = std::fs::canonicalize(run).unwrap();
        for (repo, name) in repos.iter().zip(["backend", "frontend"]) {
            git(
                &root,
                &[
                    "clone",
                    "-q",
                    repo.to_str().unwrap(),
                    run.join(name).to_str().unwrap(),
                ],
            );
            git(&run.join(name), &["checkout", "-q", "-b", BRANCH]);
        }
        let stub = root.join("stub");
        write_script(
            &stub,
            "gh",
            &format!(
                "if [ \"$1 $2\" = \"repo view\" ]; then\n  printf '%s\\n' \"$3\" >> \"{views}\"\n  f=\"{vis}/$(printf '%s' \"$3\" | tr / _)\"\n  if [ -f \"$f\" ]; then cat \"$f\"; exit 0; fi\n  exit 1\nfi\nprintf '%s\\n' \"$@\" >> \"{log}\"\nprintf -- '---\\n' >> \"{log}\"\ncase \"$1 $2\" in\n  \"pr list\") echo '[]' ;;\n  \"pr create\") repo=\"\"; while [ $# -gt 0 ]; do if [ \"$1\" = \"--repo\" ]; then repo=\"$2\"; fi; shift; done; echo \"https://github.com/$repo/pull/7\" ;;\nesac",
                log = root.join("gh-log.txt").display(),
                views = root.join("gh-views.txt").display(),
                vis = root.join("visibility").display()
            ),
        );
        let rig = Rig {
            env,
            run_dir: tempfile::Builder::new()
                .prefix("rm")
                .tempdir_in("/tmp")
                .unwrap(),
            repos: [repos[0].clone(), repos[1].clone()],
            bares: [bares[0].clone(), bares[1].clone()],
            run,
            stub,
        };
        rig.seed().await;
        rig
    }

    fn commit(&self, name: &str, file: &str) {
        let clone = self.run.join(name);
        std::fs::create_dir_all(clone.join(file).parent().unwrap()).unwrap();
        std::fs::write(clone.join(file), "x\n").unwrap();
        git(&clone, &["add", "-A"]);
        git(&clone, &["commit", "-q", "-m", "work"]);
    }

    fn visibility(&self, name: &str, visibility: &str) {
        let dir = self.env.tmp.path().join("visibility");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(name.replace('/', "_")),
            json!({"visibility": visibility, "owner": {"login": "tmgr-dev"}}).to_string(),
        )
        .unwrap();
    }

    fn mixed_visibility(&self) {
        self.visibility(NAMES[0], "PRIVATE");
        self.visibility(NAMES[1], "PUBLIC");
    }

    fn commit_text(&self, name: &str, file: &str, text: &str, message: &str) {
        let clone = self.run.join(name);
        std::fs::write(clone.join(file), format!("{text}\n")).unwrap();
        git(&clone, &["add", "-A"]);
        git(&clone, &["commit", "-q", "-m", message]);
    }

    fn views(&self) -> Vec<String> {
        std::fs::read_to_string(self.env.tmp.path().join("gh-views.txt"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn read_record(&self) -> Value {
        serde_json::from_str(
            &std::fs::read_to_string(self.env.state_dir().join("task-runs/tmgrdev-9100.json"))
                .unwrap(),
        )
        .unwrap()
    }

    async fn seed(&self) {
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
            json!({"data": [{"id": 56, "name": "Tmgr Dev", "code": "tmgrdev", "is_default": true}]}),
        )
        .await;
        mount(
            env,
            "GET",
            "/api/tasks/9100",
            common::task_json(9100, "Backend and frontend"),
        )
        .await;
        let f: Value = serde_json::from_str(FIXTURES).unwrap();
        let c = &f["cases_v2"][0];
        mount(
            env,
            "GET",
            "/api/agent-runs/keys",
            json!({"data": [{"key_id": f["key"]["key_id"], "public_key": f["key"]["public_key"], "device_name": "laptop"}]}),
        )
        .await;
        mount(
            env,
            "GET",
            "/api/agent-runs/tasks/9100",
            json!({"data": {
                "task_id": 9100, "workspace_id": 56, "workspace_code": "tmgrdev", "viewer_user_id": 1,
                "title": c["title"], "description": c["description"], "task_key": "TM-411", "setting_enabled": true,
                "category": {"id": 127, "code": "TM", "github_full_name": null, "default_branch": null,
                    "repositories": c["repositories"], "repos_sha256": c["repos_sha256"]},
                "approval": {"kind": "author", "key_id": f["key"]["key_id"], "content_sha256": c["content_sha256"],
                    "repos_sha256": c["repos_sha256"], "signed_at": 1760000100,
                    "signature": sign_v2(&env.server.address().to_string(), &f, c),
                    "revoked_at": null, "revoked_reason": null}
            }}),
        )
        .await;
        mount(
            env,
            "GET",
            "/api/tasks/9100/agent-work",
            json!({"data": {"runs": [{"id": 77, "status": "running", "agent": "claude-code", "branch": BRANCH}]}}),
        )
        .await;
        mount(
            env,
            "POST",
            "/api/agent-work/77/finish",
            json!({"data": {"id": 77, "status": "succeeded"}}),
        )
        .await;
        mount(
            env,
            "GET",
            "/api/workspaces/56/statuses",
            json!({"data": [{"id": 1, "name": "Todo", "type": "default"}, {"id": 4, "name": "Review", "type": "review"}]}),
        )
        .await;
        mount(env, "PUT", "/api/tasks/9100/4", json!({"data": {}})).await;
        mount(env, "POST", "/api/tasks/9100/comments", json!({"data": {}})).await;
        std::fs::create_dir_all(env.config_dir()).unwrap();
        let allow = json!({"version": 2, "entries": [{
            "server": env.server.address().to_string(), "user_id": 1, "workspace_id": 56,
            "workspace_code": "tmgrdev", "allowed_at": "2025-10-01T00:00:00Z", "expires_at": null,
            "keys": [{"key_id": f["key"]["key_id"], "public_key": f["key"]["public_key"], "device_name": "laptop"}],
            "repos": {"127": {
                NAMES[0]: self.repos[0].display().to_string(),
                NAMES[1]: self.repos[1].display().to_string()}}}]});
        std::fs::write(
            env.config_dir().join("runner-allow.json"),
            allow.to_string(),
        )
        .unwrap();
        self.record(&self.run);
    }

    fn record(&self, from: &Path) {
        let clones: Vec<Value> = ["backend", "frontend"]
            .iter()
            .zip(NAMES)
            .map(|(n, full)| {
                let m = std::fs::metadata(from.join(n)).unwrap();
                json!({"name": n, "repo": full, "dev": m.dev(), "ino": m.ino()})
            })
            .collect();
        let runs = self.env.state_dir().join("task-runs");
        std::fs::create_dir_all(&runs).unwrap();
        let record = json!({"repo": self.repos[0], "clone": self.run, "branch": BRANCH,
            "cloud": true, "session": "s_none", "agent": "claude-code", "clones": clones});
        std::fs::write(runs.join("tmgrdev-9100.json"), record.to_string()).unwrap();
    }

    fn cmd(&self, args: &[&str]) -> std::process::Output {
        let mut c = self.env.cmd();
        c.env("PATH", common::path_with(Some(&self.stub)))
            .env("TMGR_RUNNER_DIR", self.run_dir.path().join("r"))
            .env("TMGR_RUNNER_HOMES", self.run_dir.path().join("h"))
            .args(args);
        c.output().unwrap()
    }

    fn start(&self) {
        assert!(self.cmd(&["runner", "start"]).status.success());
    }

    fn publish(&self) -> std::process::Output {
        self.cmd(&["--ws", "tmgrdev", "run", "9100", "--publish"])
    }

    fn gh_calls(&self) -> Vec<Vec<String>> {
        let t = std::fs::read_to_string(self.env.tmp.path().join("gh-log.txt")).unwrap_or_default();
        t.split("---\n")
            .filter(|c| !c.is_empty())
            .map(|c| c.lines().map(str::to_string).collect())
            .collect()
    }

    fn pushed(&self, i: usize) -> bool {
        !git(&self.bares[i], &["branch", "--list"]).is_empty()
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

fn stderr(o: &std::process::Output) -> String {
    common::text(&o.stderr)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn both_repositories_are_pushed_opened_linked_and_reported_once() {
    let rig = Rig::new().await;
    rig.start();
    rig.commit("backend", "api.md");
    rig.commit("frontend", "ui.md");
    let out = rig.publish();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let stdout = common::text(&out.stdout);
    assert!(stdout.contains("https://github.com/tmgr-dev/backend/pull/7"));
    assert!(stdout.contains("https://github.com/tmgr-dev/frontend/pull/7"));
    for (i, repo) in rig.repos.iter().enumerate() {
        assert_eq!(
            git(
                &rig.bares[i],
                &["rev-parse", &format!("refs/heads/{BRANCH}")]
            ),
            git(repo, &["rev-parse", &format!("refs/heads/{BRANCH}")])
        );
    }
    let calls = rig.gh_calls();
    let kinds: Vec<String> = calls.iter().map(|c| c[..2].join(" ")).collect();
    assert_eq!(
        kinds,
        [
            "pr list",
            "pr create",
            "pr list",
            "pr create",
            "pr edit",
            "pr edit"
        ]
    );
    assert!(calls
        .iter()
        .flatten()
        .all(|a| a != "merge" && a != "--auto"));
    assert!(calls[4][2].contains("tmgr-dev/backend/pull/7"));
    let edit = calls[4].join("\n");
    assert!(edit.contains("Related pull requests:\n- https://github.com/tmgr-dev/frontend/pull/7"));
    let api = rig.calls().await;
    let finishes: Vec<_> = api
        .iter()
        .filter(|c| c.1 == "/api/agent-work/77/finish")
        .collect();
    assert_eq!(finishes.len(), 1);
    assert!(finishes[0].2.contains("tmgr-dev/backend/pull/7"));
    let comments: Vec<_> = api
        .iter()
        .filter(|c| c.1 == "/api/tasks/9100/comments")
        .collect();
    assert_eq!(comments.len(), 1);
    assert!(comments[0].2.contains("The agent finished. Pull requests:"));
    assert!(comments[0].2.contains("tmgr-dev/frontend/pull/7"));
    assert_eq!(
        api.iter()
            .filter(|c| c.0 == "PUT" && c.1 == "/api/tasks/9100/4")
            .count(),
        1
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_repository_without_commits_is_skipped() {
    let rig = Rig::new().await;
    rig.start();
    rig.commit("backend", "api.md");
    let out = rig.publish();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(rig.pushed(0) && !rig.pushed(1));
    let kinds: Vec<String> = rig.gh_calls().iter().map(|c| c[..2].join(" ")).collect();
    assert_eq!(kinds, ["pr list", "pr create"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn one_refused_repository_pushes_nothing() {
    let rig = Rig::new().await;
    rig.start();
    rig.commit("backend", "api.md");
    rig.commit("frontend", ".github/workflows/ci.yml");
    let out = rig.publish();
    assert_ne!(out.status.code(), Some(0));
    assert!(
        stderr(&out).contains("tmgr-dev/frontend") && stderr(&out).contains(".github"),
        "{}",
        stderr(&out)
    );
    assert!(!rig.pushed(0) && !rig.pushed(1));
    assert!(rig.gh_calls().is_empty());
}

fn branch_exists(repo: &Path) -> bool {
    !git(repo, &["branch", "--list", BRANCH]).is_empty()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refused_publish_leaves_no_branch_in_the_owners_repositories() {
    let rig = Rig::new().await;
    rig.start();
    rig.commit("backend", "api.md");
    rig.commit("frontend", ".github/workflows/ci.yml");
    let out = rig.publish();
    assert_ne!(out.status.code(), Some(0));
    assert!(!branch_exists(&rig.repos[0]) && !branch_exists(&rig.repos[1]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn merge_commits_are_refused_and_nothing_is_left_behind() {
    let rig = Rig::new().await;
    rig.start();
    rig.commit("backend", "api.md");
    let clone = rig.run.join("frontend");
    git(&clone, &["branch", "side"]);
    rig.commit("frontend", "ui.md");
    git(&clone, &["checkout", "-q", "side"]);
    rig.commit("frontend", "other.md");
    git(&clone, &["checkout", "-q", BRANCH]);
    git(&clone, &["merge", "-q", "--no-ff", "-m", "merge", "side"]);
    let out = rig.publish();
    assert_ne!(out.status.code(), Some(0));
    assert!(
        stderr(&out).contains("must be a straight line of new commits"),
        "{}",
        stderr(&out)
    );
    assert!(!rig.pushed(0) && !rig.pushed(1));
    assert!(!branch_exists(&rig.repos[0]) && !branch_exists(&rig.repos[1]));
    assert!(rig.gh_calls().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unrelated_history_is_refused() {
    let rig = Rig::new().await;
    rig.start();
    rig.commit("backend", "api.md");
    let clone = rig.run.join("frontend");
    git(&clone, &["checkout", "-q", "--orphan", "foreign"]);
    rig.commit("frontend", "ui.md");
    git(&clone, &["branch", "-M", "foreign", BRANCH]);
    let out = rig.publish();
    assert_ne!(out.status.code(), Some(0));
    assert!(
        stderr(&out).contains("unrelated history are refused"),
        "{}",
        stderr(&out)
    );
    assert!(!rig.pushed(0) && !rig.pushed(1));
    assert!(!branch_exists(&rig.repos[0]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn nothing_to_publish_is_todays_error() {
    let rig = Rig::new().await;
    rig.start();
    let out = rig.publish();
    assert_ne!(out.status.code(), Some(0));
    assert!(stderr(&out).contains("made no commits"), "{}", stderr(&out));
    assert!(rig.gh_calls().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_replaced_clone_folder_is_never_published() {
    let rig = Rig::new().await;
    rig.start();
    rig.commit("backend", "api.md");
    rig.commit("frontend", "ui.md");
    let copy = rig.run.with_extension("copy");
    git(
        rig.run.parent().unwrap(),
        &[
            "clone",
            "-q",
            rig.run.join("backend").to_str().unwrap(),
            copy.to_str().unwrap(),
        ],
    );
    std::fs::rename(rig.run.join("backend"), rig.run.join("backend-moved")).unwrap();
    std::fs::rename(&copy, rig.run.join("backend")).unwrap();
    let out = rig.publish();
    assert_ne!(out.status.code(), Some(0));
    assert!(
        stderr(&out).contains("not the folder this run started with"),
        "{}",
        stderr(&out)
    );
    assert!(!rig.pushed(0) && !rig.pushed(1));
    assert!(rig.gh_calls().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_binding_that_changed_since_the_run_is_refused() {
    let rig = Rig::new().await;
    rig.start();
    rig.commit("backend", "api.md");
    let path = rig.env.config_dir().join("runner-allow.json");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    v["entries"][0]["repos"]["127"] = json!({ NAMES[0]: rig.repos[0].display().to_string() });
    std::fs::write(&path, v.to_string()).unwrap();
    let out = rig.publish();
    assert_ne!(out.status.code(), Some(0));
    assert!(
        stderr(&out).contains("not the ones the task is approved for"),
        "{}",
        stderr(&out)
    );
    assert!(!rig.pushed(0));
    assert!(rig.gh_calls().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cwd_and_base_are_refused_for_a_category_with_a_list() {
    let rig = Rig::new().await;
    for extra in [["--cwd", "/tmp"], ["--base", "main"]] {
        let mut args = vec!["--ws", "tmgrdev", "run", "9100", "--agent", "claude-code"];
        args.extend(extra);
        let out = rig.cmd(&args);
        assert_eq!(out.status.code(), Some(2), "{}", stderr(&out));
        assert!(
            stderr(&out).contains("list of repositories"),
            "{}",
            stderr(&out)
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn without_a_terminal_unbound_repositories_are_refused_and_no_folder_is_made() {
    let rig = Rig::new().await;
    let path = rig.env.config_dir().join("runner-allow.json");
    let mut v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    v["entries"][0]["repos"] = json!({});
    std::fs::write(&path, v.to_string()).unwrap();
    let fresh = rig
        .run
        .with_file_name("tmgr-tm-411-backend-and-frontend-new");
    let out = rig.cmd(&[
        "--ws",
        "tmgrdev",
        "run",
        "9100",
        "--agent",
        "claude-code",
        "--yes",
    ]);
    assert_eq!(out.status.code(), Some(10), "{}", stderr(&out));
    assert!(
        stderr(&out).contains("once in a terminal to bind the repositories of category TM"),
        "{}",
        stderr(&out)
    );
    assert!(!fresh.exists());
    let after = std::fs::read_to_string(&path).unwrap();
    assert!(!after.contains("tmgr-dev/backend"));
}

fn start_multi_run(rig: &Rig) -> std::process::Output {
    let agents = rig.env.config_dir().join("agents");
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(
        agents.join("fake.toml"),
        "id = \"fake\"\nname = \"fake\"\ncommand = [\"/bin/sh\", \"-c\", \"exit 0\", \"fake\"]\n[permission_modes]\nask = [\"--ask\"]\n",
    )
    .unwrap();
    std::fs::remove_dir_all(&rig.run).unwrap();
    std::fs::remove_file(rig.env.tmp.path().join(".gitconfig")).unwrap();
    rig.cmd(&[
        "--json",
        "--ws",
        "tmgrdev",
        "run",
        "9100",
        "--agent",
        "fake",
        "--mode",
        "ask",
        "--yes",
        "--no-attach",
    ])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_multi_run_makes_every_clone_and_records_their_identity() {
    let rig = Rig::new().await;
    let out = start_multi_run(&rig);
    let _ = out.status;
    for name in ["backend", "frontend"] {
        let clone = rig.run.join(name);
        assert!(
            clone.join(".git").is_dir(),
            "{name}: {} {}",
            stderr(&out),
            common::text(&out.stdout)
        );
        assert_eq!(git(&clone, &["rev-parse", "--abbrev-ref", "HEAD"]), BRANCH);
    }
    assert!(!rig.run.join(".git").exists());
    let record: Value = serde_json::from_str(
        &std::fs::read_to_string(rig.env.state_dir().join("task-runs/tmgrdev-9100.json")).unwrap(),
    )
    .unwrap();
    let clones = record["clones"].as_array().unwrap();
    assert_eq!(clones.len(), 2);
    for (c, full) in clones.iter().zip(NAMES) {
        let m = std::fs::metadata(rig.run.join(c["name"].as_str().unwrap())).unwrap();
        assert_eq!(c["repo"], full);
        assert_eq!(c["dev"].as_u64(), Some(m.dev()));
        assert_eq!(c["ino"].as_u64(), Some(m.ino()));
    }
    assert_eq!(
        record["clone"],
        json!(std::fs::canonicalize(&rig.run).unwrap())
    );
}

/// Needs the multi-clone side of the runner (feat/tm-614-runner): a daemon that accepts `clones`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "needs the tmgr-runner multi-clone side; run after merging feat/tm-614-runner"]
async fn a_multi_run_is_accepted_by_the_runner_and_its_session_is_recorded() {
    let rig = Rig::new().await;
    let out = start_multi_run(&rig);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let v: Value = common::stdout_json(&out);
    assert_eq!(v["data"]["repos"].as_array().map(Vec::len), Some(2));
    let record: Value = serde_json::from_str(
        &std::fs::read_to_string(rig.env.state_dir().join("task-runs/tmgrdev-9100.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(record["session"], v["data"]["session"]["id"]);
    assert_eq!(record["agent"], "fake");
}

const MARKER: &str = "fn reconcile_ledger_entries(batch: &LedgerBatch, cutoff: Instant) -> Result<Settlement> {\n    let pending = batch.entries.iter().filter(|e| e.posted_at < cutoff && !e.voided).collect::<Vec<_>>();\n    let total = pending.iter().map(|e| e.amount_minor).sum::<i64>();\n    Settlement::new(batch.account_id, total, pending.len())\n}";
const MARKER_NAME: &str = "reconcile_ledger_entries";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_leak_into_the_public_repository_holds_everything_back() {
    let rig = Rig::new().await;
    rig.mixed_visibility();
    rig.start();
    rig.commit_text("backend", "api.md", MARKER, "work");
    rig.commit_text("frontend", "ui.md", MARKER, "work");
    let out = rig.publish();
    assert_eq!(out.status.code(), Some(6), "{}", stderr(&out));
    assert!(stderr(&out).contains("held:"), "{}", stderr(&out));
    assert!(!rig.pushed(0) && !rig.pushed(1));
    assert!(rig.gh_calls().is_empty());
    assert_eq!(rig.views().len(), 2);
    assert!(branch_exists(&rig.repos[0]) && branch_exists(&rig.repos[1]));
    let held = &rig.read_record()["held"];
    assert_eq!(held["categories"], json!(["overlap"]));
    assert_eq!(held["shas"].as_object().unwrap().len(), 2);
    for (i, name) in NAMES.iter().enumerate() {
        assert_eq!(
            held["shas"][name],
            git(
                &rig.repos[i],
                &["rev-parse", &format!("refs/heads/{BRANCH}")]
            )
        );
    }
    let api = rig.calls().await;
    let comments: Vec<_> = api
        .iter()
        .filter(|c| c.1 == "/api/tasks/9100/comments")
        .collect();
    assert_eq!(comments.len(), 1);
    let comment = &comments[0].2;
    assert!(comment.contains("Publish held: the leak gate found 1 finding(s) (overlap) in tmgr-dev/frontend. Nothing was pushed."));
    assert!(comment.contains("tmgr run TM-411 --review"));
    let finishes: Vec<_> = api
        .iter()
        .filter(|c| c.1 == "/api/agent-work/77/finish")
        .collect();
    assert_eq!(finishes.len(), 1);
    assert!(finishes[0].2.contains("cancelled"));
    for (_, _, body) in &api {
        assert!(
            !body.contains("ui.md") && !body.contains(MARKER_NAME),
            "{body}"
        );
    }
    assert!(api
        .iter()
        .all(|c| !(c.0 == "PUT" && c.1 == "/api/tasks/9100/4")));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_leak_in_a_commit_message_is_held_too() {
    let rig = Rig::new().await;
    rig.mixed_visibility();
    rig.start();
    rig.commit_text("backend", "api.md", MARKER, "work");
    rig.commit_text("frontend", "ui.md", "x", MARKER);
    let out = rig.publish();
    assert_eq!(out.status.code(), Some(6), "{}", stderr(&out));
    assert!(!rig.pushed(0) && !rig.pushed(1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_clean_mixed_run_is_published_as_before() {
    let rig = Rig::new().await;
    rig.mixed_visibility();
    rig.start();
    rig.commit("backend", "api.md");
    rig.commit("frontend", "ui.md");
    let out = rig.publish();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(rig.pushed(0) && rig.pushed(1));
    assert!(rig.read_record().get("held").is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_marker_in_the_private_repository_itself_is_not_gated() {
    let rig = Rig::new().await;
    rig.mixed_visibility();
    rig.start();
    rig.commit_text("backend", "api.md", MARKER, "work");
    rig.commit("frontend", "ui.md");
    let out = rig.publish();
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(rig.pushed(0) && rig.pushed(1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repositories_of_the_same_visibility_and_owner_are_not_gated() {
    for visibility in ["PRIVATE", "PUBLIC"] {
        let rig = Rig::new().await;
        rig.visibility(NAMES[0], visibility);
        rig.visibility(NAMES[1], visibility);
        rig.start();
        rig.commit_text("backend", "api.md", MARKER, "work");
        rig.commit_text("frontend", "ui.md", MARKER, "work");
        let out = rig.publish();
        assert_eq!(out.status.code(), Some(0), "{visibility}: {}", stderr(&out));
        assert!(rig.pushed(0) && rig.pushed(1));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repositories_that_cannot_be_looked_up_are_gated() {
    let rig = Rig::new().await;
    rig.start();
    rig.commit_text("backend", "api.md", MARKER, "work");
    rig.commit_text("frontend", "ui.md", MARKER, "work");
    let out = rig.publish();
    assert_eq!(out.status.code(), Some(6), "{}", stderr(&out));
    assert!(!rig.pushed(0) && !rig.pushed(1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_private_repository_of_another_owner_is_a_source_for_a_private_one() {
    let rig = Rig::new().await;
    rig.visibility(NAMES[0], "PRIVATE");
    let dir = rig.env.tmp.path().join("visibility");
    std::fs::write(
        dir.join(NAMES[1].replace('/', "_")),
        json!({"visibility": "PRIVATE", "owner": {"login": "someone-else"}}).to_string(),
    )
    .unwrap();
    rig.start();
    rig.commit_text("backend", "api.md", MARKER, "work");
    rig.commit_text("frontend", "ui.md", MARKER, "work");
    let out = rig.publish();
    assert_eq!(out.status.code(), Some(6), "{}", stderr(&out));
    assert!(!rig.pushed(0) && !rig.pushed(1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn review_push_and_drop_need_a_terminal_and_never_run_in_an_agent_session() {
    let rig = Rig::new().await;
    rig.mixed_visibility();
    rig.start();
    rig.commit_text("backend", "api.md", MARKER, "work");
    rig.commit_text("frontend", "ui.md", MARKER, "work");
    assert_eq!(rig.publish().status.code(), Some(6));
    for flags in [
        vec!["--review"],
        vec!["--review", "--push"],
        vec!["--review", "--drop"],
    ] {
        let mut args = vec!["--ws", "tmgrdev", "run", "9100"];
        args.extend(flags.iter().copied());
        let out = rig.cmd(&args);
        assert_eq!(out.status.code(), Some(10), "{flags:?}: {}", stderr(&out));
        let mut c = rig.env.cmd();
        c.env("PATH", common::path_with(Some(&rig.stub)))
            .env("TMGR_RUN_ID", "r1")
            .args(&args);
        let out = c.output().unwrap();
        assert_eq!(out.status.code(), Some(5), "{flags:?}: {}", stderr(&out));
    }
    assert!(!rig.pushed(0) && !rig.pushed(1));
    assert!(rig.read_record().get("held").is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_owner_flags_fit_together() {
    let rig = Rig::new().await;
    for args in [
        vec!["run", "9100", "--push"],
        vec!["run", "9100", "--drop"],
        vec!["run", "9100", "--review", "--push", "--drop"],
        vec!["run", "9100", "--review", "--publish"],
        vec!["run", "--review"],
    ] {
        let out = rig.cmd(&args);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", stderr(&out));
    }
}

struct Pty {
    screen: Arc<Mutex<vt100::Parser>>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    _master: Box<dyn portable_pty::MasterPty + Send>,
}

impl Drop for Pty {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

impl Pty {
    fn text(&self) -> String {
        self.screen.lock().unwrap().screen().contents()
    }

    fn wait_for(&self, needle: &str) {
        let end = Instant::now() + Duration::from_secs(30);
        while !self.text().contains(needle) {
            assert!(
                Instant::now() < end,
                "timed out waiting for {needle:?}; screen:\n{}",
                self.text()
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn line(&mut self, s: &str) {
        self.writer.write_all(format!("{s}\r").as_bytes()).unwrap();
        self.writer.flush().unwrap();
    }

    fn exit(&mut self) -> i32 {
        let end = Instant::now() + Duration::from_secs(30);
        loop {
            if let Some(st) = self.child.try_wait().unwrap() {
                return st.exit_code() as i32;
            }
            assert!(Instant::now() < end, "did not exit:\n{}", self.text());
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Rig {
    fn pty(&self, args: &[&str]) -> Pty {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 60,
                cols: 160,
                pixel_width: 0,
                pixel_height: 0,
            })
            .unwrap();
        let mut cmd = CommandBuilder::new(env!("CARGO_BIN_EXE_tmgr"));
        cmd.args(args);
        cmd.env_clear();
        for (k, v) in self.env.std_cmd().get_envs() {
            if let Some(v) = v {
                cmd.env(k, v);
            }
        }
        cmd.env("PATH", common::path_with(Some(&self.stub)));
        cmd.env("TMGR_RUNNER_DIR", self.run_dir.path().join("r"));
        cmd.env("TMGR_RUNNER_HOMES", self.run_dir.path().join("h"));
        let child = pair.slave.spawn_command(cmd).unwrap();
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader().unwrap();
        let screen = Arc::new(Mutex::new(vt100::Parser::new(60, 160, 0)));
        let sink = screen.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = reader.read(&mut buf) {
                if n == 0 {
                    break;
                }
                sink.lock().unwrap().process(&buf[..n]);
            }
        });
        Pty {
            screen,
            writer: pair.master.take_writer().unwrap(),
            child,
            _master: pair.master,
        }
    }

    async fn held(&self) {
        self.mixed_visibility();
        self.start();
        self.commit_text("backend", "api.md", MARKER, "work");
        self.commit_text("frontend", "ui.md", MARKER, "work");
        assert_eq!(self.publish().status.code(), Some(6));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_owner_reviews_the_findings_and_the_public_diff_in_a_terminal() {
    let rig = Rig::new().await;
    rig.held().await;
    let mut p = rig.pty(&["--ws", "tmgrdev", "run", "9100", "--review"]);
    p.wait_for("tmgr-dev/frontend diffstat");
    assert_eq!(p.exit(), 0, "{}", p.text());
    let screen = p.text();
    assert!(screen.contains("overlap | tmgr-dev/frontend | ui.md:1"));
    assert!(screen.contains("ui.md"), "{screen}");
    assert!(screen.contains(MARKER_NAME), "{screen}");
    assert!(!rig.pushed(0) && !rig.pushed(1));
    assert!(rig.read_record().get("held").is_some());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wrong_confirmation_pushes_nothing_and_the_right_one_publishes_and_clears_the_hold() {
    let rig = Rig::new().await;
    rig.held().await;
    let args = ["--ws", "tmgrdev", "run", "9100", "--review", "--push"];
    let mut p = rig.pty(&args);
    p.wait_for("to push the held branch:");
    p.line("yes");
    assert_ne!(p.exit(), 0);
    assert!(!rig.pushed(0) && !rig.pushed(1));
    assert!(rig.read_record().get("held").is_some());

    let mut p = rig.pty(&args);
    p.wait_for("Type tmgr-dev/frontend to push the held branch:");
    p.line("tmgr-dev/frontend");
    assert_eq!(p.exit(), 0, "{}", p.text());
    assert!(rig.pushed(0) && rig.pushed(1));
    assert!(rig.read_record().get("held").is_none());
    let kinds: Vec<String> = rig.gh_calls().iter().map(|c| c[..2].join(" ")).collect();
    assert_eq!(
        kinds,
        [
            "pr list",
            "pr create",
            "pr list",
            "pr create",
            "pr edit",
            "pr edit"
        ]
    );
    let api = rig.calls().await;
    let comments: Vec<_> = api
        .iter()
        .filter(|c| c.1 == "/api/tasks/9100/comments")
        .collect();
    assert_eq!(comments.len(), 2);
    assert!(comments[1].2.contains("The agent finished. Pull requests:"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_branch_that_changed_since_it_was_held_is_refused() {
    let rig = Rig::new().await;
    rig.held().await;
    rig.commit("frontend", "more.md");
    let mut p = rig.pty(&["--ws", "tmgrdev", "run", "9100", "--review", "--push"]);
    assert_ne!(p.exit(), 0);
    assert!(
        p.text().contains("changed since it was held"),
        "{}",
        p.text()
    );
    assert!(!rig.pushed(0) && !rig.pushed(1));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dropping_a_hold_clears_it_comments_and_keeps_the_branches() {
    let rig = Rig::new().await;
    rig.held().await;
    let args = ["--ws", "tmgrdev", "run", "9100", "--review", "--drop"];
    let mut p = rig.pty(&args);
    p.wait_for("to drop the held branch:");
    p.line("nope");
    assert_ne!(p.exit(), 0);
    assert!(rig.read_record().get("held").is_some());

    let mut p = rig.pty(&args);
    p.wait_for("Type TM-342 to drop the held branch:");
    p.line("TM-342");
    assert_eq!(p.exit(), 0, "{}", p.text());
    assert!(rig.read_record().get("held").is_none());
    assert!(!rig.pushed(0) && !rig.pushed(1));
    assert!(branch_exists(&rig.repos[0]) && branch_exists(&rig.repos[1]));
    let api = rig.calls().await;
    assert!(api.iter().any(|c| c.1 == "/api/tasks/9100/comments"
        && c.2
            .contains("Dropped by the owner after the leak gate; nothing was pushed.")));
}
