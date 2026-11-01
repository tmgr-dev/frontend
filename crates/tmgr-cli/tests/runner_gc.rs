#![cfg(feature = "runner")]

mod common;

use common::{git, init_repo, path_with, write_script, Env};
use serde_json::{json, Value};
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};

const BRANCH: &str = "tmgr/tm-410-fix-login";
const FLAT: &str = "tmgr-tm-410-fix-login";

fn pr(number: u64, state: &str, oid: &str) -> Value {
    json!({
        "number": number,
        "state": state,
        "headRefOid": oid,
        "mergedAt": if state == "MERGED" { json!("2026-10-01T10:00:00Z") } else { Value::Null },
        "isCrossRepository": false,
        "url": format!("https://github.com/acme/widgets/pull/{number}"),
    })
}

fn cross(mut v: Value) -> Value {
    v["isCrossRepository"] = json!(true);
    v
}

struct Rig {
    env: Env,
    _world: tempfile::TempDir,
    runner: tempfile::TempDir,
    root: PathBuf,
    repo: PathBuf,
    copy: PathBuf,
    head: String,
    up: bool,
}

impl Rig {
    async fn new() -> Rig {
        let env = Env::new().await;
        let world = tempfile::Builder::new()
            .prefix("gcw")
            .tempdir_in("/tmp")
            .unwrap();
        let runner = tempfile::Builder::new()
            .prefix("gcr")
            .tempdir_in("/tmp")
            .unwrap();
        let root = std::fs::canonicalize(world.path()).unwrap();
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
        std::fs::write(repo.join(".gitignore"), "target/\n.env\n").unwrap();
        git(&repo, &["add", ".gitignore"]);
        git(&repo, &["commit", "-q", "-m", "ignore"]);
        let copy = root.join("code/repo-worktrees").join(FLAT);
        let head = clone_with_commit(&root, &repo, &copy, BRANCH);
        write_script(
            &root.join("stub"),
            "gh",
            &format!(
                "r=\"\"; p=\"\"; for a in \"$@\"; do [ \"$p\" = \"--repo\" ] && r=\"$a\"; p=\"$a\"; done\n\
                 mkdir -p \"{g}\"; echo \"$@\" >> \"{g}/calls.txt\"\n\
                 f=\"{g}/$(echo \"$r\" | tr / -)\"\n\
                 if [ -f \"$f.fail\" ]; then echo boom >&2; exit 1; fi\n\
                 cat \"$f.json\"",
                g = root.join("gh").display()
            ),
        );
        let rig = Rig {
            env,
            _world: world,
            runner,
            root,
            repo,
            copy,
            head,
            up: false,
        };
        rig.record(json!({
            "repo": rig.repo, "clone": rig.copy, "branch": BRANCH, "cloud": true,
            "session": "s_none", "agent": "claude-code"
        }));
        rig
    }

    fn record(&self, body: Value) {
        let dir = self.env.state_dir().join("task-runs");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tmgrdev-9002.json"), body.to_string()).unwrap();
    }

    fn record_path(&self) -> PathBuf {
        self.env.state_dir().join("task-runs/tmgrdev-9002.json")
    }

    fn gh(&self, repo: &str, prs: Value) {
        self.gh_raw(repo, &prs.to_string());
    }

    fn gh_raw(&self, repo: &str, text: &str) {
        let dir = self.root.join("gh");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{}.json", repo.replace('/', "-"))), text).unwrap();
    }

    fn gh_fails(&self, repo: &str) {
        let dir = self.root.join("gh");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(format!("{}.fail", repo.replace('/', "-"))), "").unwrap();
    }

    fn merged(&self) {
        self.gh("acme/widgets", json!([pr(5, "MERGED", &self.head)]));
    }

    fn start_runner(&mut self) {
        let out = self.cmd(&["runner", "start"]);
        assert!(out.status.success(), "{}", common::text(&out.stderr));
        self.up = true;
    }

    fn cmd(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }

    fn command(&self, args: &[&str]) -> assert_cmd::Command {
        let mut c = self.env.cmd();
        c.env("PATH", path_with(Some(&self.root.join("stub"))))
            .env("TMGR_RUNNER_DIR", self.runner.path().join("r"))
            .env("TMGR_RUNNER_HOMES", self.runner.path().join("h"))
            .args(args);
        c
    }

    fn gc(&self, extra: &[&str]) -> (Output, Value) {
        let mut args = vec!["runner", "gc", "--json"];
        args.extend_from_slice(extra);
        let out = self.cmd(&args);
        let v = serde_json::from_slice(&out.stdout).unwrap_or(Value::Null);
        (out, v)
    }

    fn copy_json(&self, v: &Value, i: usize) -> Value {
        v["data"]["copies"][i].clone()
    }

    fn assert_skipped(&self, why: &str) {
        let (out, v) = self.gc(&[]);
        let c = self.copy_json(&v, 0);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}{c}",
            common::text(&out.stderr)
        );
        assert_eq!(c["decision"], "skipped", "{c}");
        let reason = c["reason"].as_str().unwrap_or("").to_lowercase();
        assert!(reason.contains(&why.to_lowercase()), "{why:?} not in {c}");
        assert!(self.copy.exists(), "the copy must stay");
        assert!(self.record_path().exists(), "the record must stay");
    }

    fn assert_removed(&self) {
        let (out, v) = self.gc(&[]);
        let c = self.copy_json(&v, 0);
        assert_eq!(
            out.status.code(),
            Some(0),
            "{}{c}",
            common::text(&out.stderr)
        );
        assert_eq!(c["decision"], "removed", "{c}");
        assert!(!self.copy.exists());
        assert!(!self.record_path().exists());
        let left: Vec<_> = std::fs::read_dir(self.copy.parent().unwrap())
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert!(
            left.iter().all(|n| !n.to_string_lossy().starts_with('.')),
            "{left:?}"
        );
    }
}

impl Drop for Rig {
    fn drop(&mut self) {
        if self.up {
            let _ = self.cmd(&["runner", "stop"]);
        }
    }
}

fn clone_with_commit(root: &Path, repo: &Path, copy: &Path, branch: &str) -> String {
    git(
        root,
        &[
            "clone",
            "-q",
            repo.to_str().unwrap(),
            copy.to_str().unwrap(),
        ],
    );
    git(copy, &["checkout", "-q", "-b", branch]);
    std::fs::write(copy.join("fix.txt"), "fixed\n").unwrap();
    git(copy, &["add", "fix.txt"]);
    git(copy, &["commit", "-q", "-m", "fix login"]);
    git(copy, &["rev-parse", "HEAD"])
}

fn build_output(copy: &Path) {
    std::fs::create_dir_all(copy.join("target/debug")).unwrap();
    std::fs::write(copy.join("target/debug/big.bin"), vec![7u8; 200_000]).unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_merged_pr_removes_the_copy_with_its_build_output_and_the_record() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    build_output(&rig.copy);
    rig.merged();
    let (out, v) = rig.gc(&[]);
    assert_eq!(out.status.code(), Some(0), "{}", common::text(&out.stderr));
    let c = rig.copy_json(&v, 0);
    assert_eq!(c["decision"], "removed", "{c}");
    assert_eq!(c["pr"]["number"], 5);
    assert_eq!(c["pr"]["state"], "MERGED");
    assert!(c["bytes"].as_u64().unwrap() >= 200_000, "{c}");
    assert!(v["data"]["reclaimed_bytes"].as_u64().unwrap() >= 200_000);
    assert!(!rig.copy.exists());
    assert!(!rig.record_path().exists());
    let argv = std::fs::read_to_string(rig.root.join("gh/calls.txt")).unwrap();
    for part in [
        "pr list",
        "--repo acme/widgets",
        "--head tmgr/tm-410-fix-login",
        "--state all",
        "--limit 100",
    ] {
        assert!(argv.contains(part), "{part:?} missing in {argv}");
    }
    assert!(rig.repo.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_closed_pr_at_the_same_head_removes_the_copy() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.gh("acme/widgets", json!([pr(6, "CLOSED", &rig.head)]));
    rig.assert_removed();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_open_pr_keeps_the_copy() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.gh(
        "acme/widgets",
        json!([pr(5, "MERGED", &rig.head), pr(7, "OPEN", &rig.head)]),
    );
    rig.assert_skipped("open");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn no_pr_keeps_the_copy_even_when_the_branch_looks_merged() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    git(
        &rig.repo,
        &["fetch", "-q", rig.copy.to_str().unwrap(), BRANCH],
    );
    git(&rig.repo, &["merge", "-q", "--ff-only", "FETCH_HEAD"]);
    rig.gh("acme/widgets", json!([]));
    rig.assert_skipped("no pull request");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_failing_gh_keeps_the_copy() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.gh_fails("acme/widgets");
    rig.assert_skipped("uncertain");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_gh_output_keeps_the_copy() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.gh_raw("acme/widgets", "{not json");
    rig.assert_skipped("uncertain");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_truncated_gh_answer_keeps_the_copy() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    let many: Vec<Value> = (1..=100).map(|n| pr(n, "MERGED", &rig.head)).collect();
    rig.gh("acme/widgets", json!(many));
    rig.assert_skipped("uncertain");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_pr_state_keeps_the_copy() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.gh("acme/widgets", json!([pr(5, "DRAFTY", &rig.head)]));
    rig.assert_skipped("uncertain");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cross_repository_pr_is_ignored() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.gh(
        "acme/widgets",
        json!([
            cross(pr(9, "MERGED", &rig.head)),
            cross(pr(10, "OPEN", &rig.head))
        ]),
    );
    rig.assert_skipped("no pull request");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_head_that_differs_from_the_pr_head_keeps_the_copy() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.gh(
        "acme/widgets",
        json!([pr(5, "MERGED", "0123456789abcdef0123456789abcdef01234567")]),
    );
    rig.assert_skipped("head");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_modified_tracked_file_keeps_the_copy() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.merged();
    std::fs::write(rig.copy.join("fix.txt"), "changed\n").unwrap();
    rig.assert_skipped("fix.txt");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_untracked_file_keeps_the_copy() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.merged();
    std::fs::write(rig.copy.join("notes.txt"), "mine\n").unwrap();
    rig.assert_skipped("notes.txt");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_ignored_env_file_keeps_the_copy() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.merged();
    std::fs::write(rig.copy.join(".env"), "SECRET=1\n").unwrap();
    rig.assert_skipped(".env");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stash_keeps_the_copy() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.merged();
    std::fs::write(rig.copy.join("fix.txt"), "stashed\n").unwrap();
    git(&rig.copy, &["stash", "-q"]);
    rig.assert_skipped("stash");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_branch_not_merged_into_head_keeps_the_copy() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.merged();
    git(&rig.copy, &["checkout", "-q", "-b", "side", "main"]);
    std::fs::write(rig.copy.join("side.txt"), "x\n").unwrap();
    git(&rig.copy, &["add", "side.txt"]);
    git(&rig.copy, &["commit", "-q", "-m", "side"]);
    git(&rig.copy, &["checkout", "-q", BRANCH]);
    rig.assert_skipped("side");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_assume_unchanged_entry_keeps_the_copy() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.merged();
    git(
        &rig.copy,
        &["update-index", "--assume-unchanged", "fix.txt"],
    );
    rig.assert_skipped("assume-unchanged");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_skip_worktree_entry_keeps_the_copy() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.merged();
    git(&rig.copy, &["update-index", "--skip-worktree", "fix.txt"]);
    rig.assert_skipped("skip-worktree");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_merge_in_progress_keeps_the_copy() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.merged();
    std::fs::write(rig.copy.join(".git/MERGE_HEAD"), format!("{}\n", rig.head)).unwrap();
    rig.assert_skipped("in progress");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_head_on_another_branch_keeps_the_copy() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.merged();
    git(&rig.copy, &["checkout", "-q", "-b", "other"]);
    rig.assert_skipped("branch");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_local_git_config_that_runs_programs_is_not_executed() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.merged();
    let marker = rig.root.join("marker");
    let cfg = std::fs::read_to_string(rig.copy.join(".git/config")).unwrap();
    std::fs::write(
        rig.copy.join(".git/config"),
        format!(
            "{cfg}[filter \"x\"]\n\tclean = touch {m}\n\tprocess = touch {m}\n[diff \"y\"]\n\ttextconv = touch {m}\n",
            m = marker.display()
        ),
    )
    .unwrap();
    std::fs::create_dir_all(rig.copy.join(".git/info")).unwrap();
    std::fs::write(rig.copy.join(".git/info/attributes"), "* filter=x diff=y\n").unwrap();
    rig.assert_skipped("runs programs");
    assert!(!marker.exists(), "the configured program ran");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_symlinked_copy_is_left_alone() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.merged();
    let real = rig.root.join("elsewhere/real");
    std::fs::create_dir_all(real.parent().unwrap()).unwrap();
    std::fs::rename(&rig.copy, &real).unwrap();
    std::os::unix::fs::symlink(&real, &rig.copy).unwrap();
    let (out, v) = rig.gc(&[]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(rig.copy_json(&v, 0)["decision"], "skipped");
    assert!(real.join("fix.txt").exists());
    assert!(std::fs::symlink_metadata(&rig.copy).is_ok());
    assert!(rig.record_path().exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_recorded_path_outside_the_worktrees_folder_is_left_alone() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.merged();
    let outside = rig.root.join("elsewhere/clone");
    std::fs::create_dir_all(outside.parent().unwrap()).unwrap();
    git(
        &rig.root,
        &[
            "clone",
            "-q",
            rig.repo.to_str().unwrap(),
            outside.to_str().unwrap(),
        ],
    );
    rig.record(json!({
        "repo": rig.repo, "clone": outside, "branch": BRANCH, "cloud": true
    }));
    let (out, v) = rig.gc(&[]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(rig.copy_json(&v, 0)["decision"], "skipped");
    assert!(outside.join("README.md").exists());
    assert!(rig.copy.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unknown_folder_in_the_worktrees_folder_is_never_touched() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.merged();
    let stranger = rig.copy.parent().unwrap().join("someone-elses");
    std::fs::create_dir_all(&stranger).unwrap();
    std::fs::write(stranger.join("data.txt"), "keep\n").unwrap();
    rig.assert_removed();
    assert!(stranger.join("data.txt").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_record_whose_copy_is_gone_is_reported_and_kept() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    std::fs::remove_dir_all(&rig.copy).unwrap();
    let (out, v) = rig.gc(&[]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(rig.copy_json(&v, 0)["decision"], "gone");
    assert!(rig.record_path().exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_held_run_is_kept() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.merged();
    rig.record(json!({
        "repo": rig.repo, "clone": rig.copy, "branch": BRANCH, "cloud": true,
        "held": {"at": "2026-10-01T00:00:00Z", "shas": {}, "categories": [], "findings": 1}
    }));
    rig.assert_skipped("held");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_process_working_inside_the_copy_keeps_it() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.merged();
    let mut child: Child = Command::new("sleep")
        .arg("300")
        .current_dir(&rig.copy)
        .stdout(Stdio::null())
        .spawn()
        .unwrap();
    let (out, v) = rig.gc(&[]);
    let _ = child.kill();
    let _ = child.wait();
    let c = rig.copy_json(&v, 0);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(c["decision"], "skipped", "{c}");
    assert!(c["reason"].as_str().unwrap().contains("process"), "{c}");
    assert!(rig.copy.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_live_runner_session_in_the_copy_keeps_it() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    rig.merged();
    let script = write_script(&rig.root.join("agent"), "agent.sh", "exec /bin/sleep 300");
    let started = rig.cmd(&[
        "run",
        "--agent-cmd",
        script.to_str().unwrap(),
        "--cwd",
        rig.copy.to_str().unwrap(),
        "--no-attach",
    ]);
    assert!(
        started.status.success(),
        "{}",
        common::text(&started.stderr)
    );
    std::thread::sleep(std::time::Duration::from_millis(800));
    let listed = common::text(&rig.cmd(&["agents", "--json"]).stdout);
    if listed.contains("\"kind\":\"exited\"") {
        eprintln!("skipped: the sandbox could not start the session here");
        return;
    }
    let (out, v) = rig.gc(&[]);
    let c = rig.copy_json(&v, 0);
    let _ = rig.cmd(&["runner", "stop"]);
    assert_eq!(out.status.code(), Some(0), "{c}");
    assert_eq!(c["decision"], "skipped", "{c}");
    assert!(c["reason"].as_str().unwrap().contains("session"), "{c}");
    assert!(rig.copy.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_unreachable_runner_refuses_and_removes_nothing() {
    let rig = Rig::new().await;
    rig.merged();
    let out = rig.cmd(&["runner", "gc"]);
    assert_ne!(out.status.code(), Some(0));
    assert!(
        common::text(&out.stderr).contains("runner"),
        "{}",
        common::text(&out.stderr)
    );
    assert!(rig.copy.exists());
    assert!(rig.record_path().exists());
    let status = rig.cmd(&["runner", "status"]);
    assert_ne!(
        status.status.code(),
        Some(0),
        "gc must not start the runner"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dry_run_changes_nothing_and_reports_the_same_decision() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    build_output(&rig.copy);
    rig.merged();
    let (out, dry) = rig.gc(&["--dry-run"]);
    assert_eq!(out.status.code(), Some(0));
    let c = rig.copy_json(&dry, 0);
    assert_eq!(c["decision"], "would_remove", "{c}");
    assert_eq!(c["pr"]["number"], 5);
    assert!(c["bytes"].as_u64().unwrap() >= 200_000);
    assert!(rig.copy.join("target/debug/big.bin").exists());
    assert!(rig.record_path().exists());
    let human = rig.cmd(&["runner", "gc", "--dry-run"]);
    assert!(
        common::text(&human.stdout).contains("would remove"),
        "{}",
        common::text(&human.stdout)
    );
    assert!(rig.copy.exists());
    rig.assert_removed();
}

fn pickup(rig: &Rig, status: &str, run_exited: bool, cwd: &Path) -> Value {
    json!({
        "id": "q_1", "rule": "nightly", "task": {"workspace_id": 56, "task_id": 9002},
        "key": "TM-410", "status": status,
        "bundle": {
            "v": 1, "task": {"workspace_id": 56, "task_id": 9002}, "title": "Fix login",
            "body": "b", "author": {"kind": "human"}, "comments": [], "pages": [],
            "launch": {"profile": "claude-code", "cwd": cwd}
        },
        "bundle_sha256": "00", "context_tainted": false,
        "created_at": "2026-10-01T00:00:00Z", "updated_at": "2026-10-01T00:00:00Z",
        "run_exited": run_exited, "run_clean": true,
        "pr_url": "https://github.com/acme/widgets/pull/5",
        "cloud": {
            "claim": {
                "host": "h", "user_id": 1, "workspace_id": 56, "task_id": 9002, "category_id": 127,
                "content_sha256": "00", "key_id": "k", "signed_at": 1, "signature": "s", "kind": "author"
            },
            "workspace_code": "tmgrdev", "branch": BRANCH
        },
        "_repo": rig.repo,
    })
}

fn write_queue(rig: &Rig, p: Value) {
    let dir = rig.env.config_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("queue-state.json");
    std::fs::write(&path, json!({"version": 1, "pickups": [p]}).to_string()).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
}

fn queue_copy(rig: &mut Rig) -> PathBuf {
    std::fs::remove_file(rig.record_path()).unwrap();
    std::fs::remove_dir_all(&rig.copy).unwrap();
    let copy = rig.root.join("code/repo-worktrees/queue/q_1");
    std::fs::create_dir_all(copy.parent().unwrap()).unwrap();
    rig.head = clone_with_commit(&rig.root, &rig.repo, &copy, BRANCH);
    rig.copy = copy.clone();
    copy
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_merged_queue_copy_is_removed_and_its_pickup_released() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    let copy = queue_copy(&mut rig);
    rig.merged();
    write_queue(&rig, pickup(&rig, "started", true, &copy));
    let (out, v) = rig.gc(&[]);
    assert_eq!(out.status.code(), Some(0), "{}", common::text(&out.stderr));
    let c = rig.copy_json(&v, 0);
    assert_eq!(c["decision"], "removed", "{c}");
    assert!(!copy.exists());
    let state: Value = serde_json::from_str(
        &std::fs::read_to_string(rig.env.config_dir().join("queue-state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(state["pickups"][0]["released"], true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_running_queue_pickup_keeps_its_copy() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    let copy = queue_copy(&mut rig);
    rig.merged();
    write_queue(&rig, pickup(&rig, "started", false, &copy));
    let (out, v) = rig.gc(&[]);
    assert_eq!(out.status.code(), Some(0));
    let c = rig.copy_json(&v, 0);
    assert_eq!(c["decision"], "skipped", "{c}");
    assert!(copy.exists());
}

fn multi_setup(rig: &Rig) -> (PathBuf, PathBuf, PathBuf) {
    let repo2 = init_repo(&rig.root.join("code/repo2"));
    git(
        &repo2,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/acme/gadgets.git",
        ],
    );
    std::fs::write(repo2.join(".gitignore"), "target/\n.env\n").unwrap();
    git(&repo2, &["add", ".gitignore"]);
    git(&repo2, &["commit", "-q", "-m", "ignore"]);
    std::fs::remove_dir_all(&rig.copy).unwrap();
    let run_dir = rig.copy.clone();
    std::fs::create_dir_all(&run_dir).unwrap();
    let a = run_dir.join("widgets");
    let b = run_dir.join("gadgets");
    clone_with_commit(&rig.root, &rig.repo, &a, BRANCH);
    clone_with_commit(&rig.root, &repo2, &b, BRANCH);
    let ident = |p: &Path| {
        let m = std::fs::metadata(p).unwrap();
        (m.dev(), m.ino())
    };
    let (da, ia) = ident(&a);
    let (db, ib) = ident(&b);
    rig.record(json!({
        "repo": rig.repo, "clone": run_dir, "branch": BRANCH, "cloud": true,
        "clones": [
            {"name": "widgets", "repo": "acme/widgets", "dev": da, "ino": ia},
            {"name": "gadgets", "repo": "acme/gadgets", "dev": db, "ino": ib},
        ]
    }));
    let allow = json!({"version": 2, "entries": [{
        "server": rig.env.server.address().to_string(), "user_id": 1, "workspace_id": 56,
        "workspace_code": "tmgrdev", "allowed_at": "2025-10-01T00:00:00Z", "expires_at": null,
        "keys": [],
        "repos": {"127": {"acme/widgets": rig.repo, "acme/gadgets": repo2}}}]});
    std::fs::create_dir_all(rig.env.config_dir()).unwrap();
    std::fs::write(
        rig.env.config_dir().join("runner-allow.json"),
        allow.to_string(),
    )
    .unwrap();
    (run_dir, a, b)
}

fn head_of(dir: &Path) -> String {
    git(dir, &["rev-parse", "HEAD"])
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_multi_repo_run_is_removed_when_every_clone_has_a_finished_pr() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    let (run_dir, a, b) = multi_setup(&rig);
    rig.gh("acme/widgets", json!([pr(5, "MERGED", &head_of(&a))]));
    rig.gh("acme/gadgets", json!([pr(8, "CLOSED", &head_of(&b))]));
    let (out, v) = rig.gc(&[]);
    assert_eq!(out.status.code(), Some(0), "{}", common::text(&out.stderr));
    assert_eq!(rig.copy_json(&v, 0)["decision"], "removed", "{v}");
    assert!(!run_dir.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_multi_repo_run_with_one_dirty_clone_removes_nothing() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    let (run_dir, a, b) = multi_setup(&rig);
    rig.gh("acme/widgets", json!([pr(5, "MERGED", &head_of(&a))]));
    rig.gh("acme/gadgets", json!([pr(8, "MERGED", &head_of(&b))]));
    std::fs::write(b.join("notes.txt"), "mine\n").unwrap();
    let (out, v) = rig.gc(&[]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(rig.copy_json(&v, 0)["decision"], "skipped", "{v}");
    assert!(a.join("fix.txt").exists() && b.join("notes.txt").exists());
    assert!(run_dir.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_multi_repo_clone_with_another_identity_is_left_alone() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    let (run_dir, a, b) = multi_setup(&rig);
    rig.gh("acme/widgets", json!([pr(5, "MERGED", &head_of(&a))]));
    rig.gh("acme/gadgets", json!([pr(8, "MERGED", &head_of(&b))]));
    let mut rec: Value =
        serde_json::from_str(&std::fs::read_to_string(rig.record_path()).unwrap()).unwrap();
    rec["clones"][0]["ino"] = json!(1);
    rig.record(rec);
    let (out, v) = rig.gc(&[]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(rig.copy_json(&v, 0)["decision"], "skipped", "{v}");
    assert!(run_dir.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_multi_repo_folder_with_an_extra_entry_is_left_alone() {
    let mut rig = Rig::new().await;
    rig.start_runner();
    let (run_dir, a, b) = multi_setup(&rig);
    rig.gh("acme/widgets", json!([pr(5, "MERGED", &head_of(&a))]));
    rig.gh("acme/gadgets", json!([pr(8, "MERGED", &head_of(&b))]));
    std::fs::write(run_dir.join("extra.txt"), "x\n").unwrap();
    let (out, v) = rig.gc(&[]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(rig.copy_json(&v, 0)["decision"], "skipped", "{v}");
    assert!(run_dir.join("extra.txt").exists());
}
