#![cfg(feature = "runner")]

mod common;

use common::{git, init_repo, write_script, Env, ACCESS};
use serde_json::{json, Value};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
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
                "printf '%s\\n' \"$@\" >> \"{log}\"\nprintf -- '---\\n' >> \"{log}\"\ncase \"$1 $2\" in\n  \"pr list\") echo '[]' ;;\n  \"pr create\") repo=\"\"; while [ $# -gt 0 ]; do if [ \"$1\" = \"--repo\" ]; then repo=\"$2\"; fi; shift; done; echo \"https://github.com/$repo/pull/7\" ;;\nesac",
                log = root.join("gh-log.txt").display()
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
    rig.commit("backend", "api.txt");
    rig.commit("frontend", "ui.txt");
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
    rig.commit("backend", "api.txt");
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
    rig.commit("backend", "api.txt");
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
    rig.commit("backend", "api.txt");
    rig.commit("frontend", "ui.txt");
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
    rig.commit("backend", "api.txt");
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
