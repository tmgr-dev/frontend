use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;
use tmgr_queue::workdir::{clone_dir, remote_branch_head, GitClone, WorkDirs};

const BRANCH: &str = "tmgr/tm-1-fix";

fn routes() -> &'static PathBuf {
    static ROUTES: OnceLock<PathBuf> = OnceLock::new();
    ROUTES.get_or_init(|| {
        let dir = tempfile::Builder::new()
            .prefix("cr")
            .tempdir()
            .unwrap()
            .keep();
        let routes = dir.join("routes");
        std::fs::write(&routes, "").unwrap();
        let script = format!(
            "#!/bin/sh\nfor a in \"$@\"; do cmd=\"$a\"; done\nsvc=${{cmd%% *}}\npath=${{cmd#* }}\npath=${{path#\\'}}\npath=${{path%\\'}}\npath=${{path#/}}\nbare=$(awk -F= -v p=\"$path\" '$1==p{{print $2; exit}}' \"{}\")\n[ -n \"$bare\" ] || {{ echo \"unknown repository $path\" >&2; exit 1; }}\nunset GIT_DIR GIT_WORK_TREE GIT_OBJECT_DIRECTORY GIT_CONFIG_PARAMETERS GIT_CONFIG_COUNT\ncase \"$svc\" in\n  git-upload-pack) exec git upload-pack \"$bare\" ;;\n  *) echo \"unsupported $svc\" >&2; exit 1 ;;\nesac\n",
            routes.display()
        );
        let ssh = dir.join("ssh");
        std::fs::write(&ssh, script).unwrap();
        std::os::unix::fs::PermissionsExt::set_mode(
            &mut std::fs::metadata(&ssh).unwrap().permissions(),
            0o755,
        );
        std::fs::set_permissions(
            &ssh,
            std::os::unix::fs::PermissionsExt::from_mode(0o755),
        )
        .unwrap();
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut parts = vec![dir.clone()];
        parts.extend(std::env::split_paths(&path));
        std::env::set_var("PATH", std::env::join_paths(parts).unwrap());
        routes
    })
}

fn route(full_name: &str, bare: &Path) {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(routes())
        .unwrap();
    f.write_all(format!("{full_name}.git={}\n", bare.display()).as_bytes())
        .unwrap();
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

struct Fixture {
    _tmp: tempfile::TempDir,
    repo: String,
    wip: String,
}

fn fixture(name: &str, origin: &str, served_as: &str) -> Fixture {
    routes();
    let tmp = tempfile::tempdir().unwrap();
    let root = std::fs::canonicalize(tmp.path()).unwrap();
    let repo = root.join(name);
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "a\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "a"]);
    let bare = root.join("origin.git");
    git(&root, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
    git(&repo, &["push", "-q", bare.to_str().unwrap(), "main"]);
    let other = root.join("other");
    git(
        &root,
        &[
            "clone",
            "-q",
            bare.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    git(&other, &["checkout", "-q", "-b", BRANCH]);
    std::fs::write(other.join("wip.txt"), "w\n").unwrap();
    git(&other, &["add", "."]);
    git(&other, &["commit", "-q", "-m", "wip"]);
    git(&other, &["push", "-q", "origin", BRANCH]);
    let wip = git(&other, &["rev-parse", "HEAD"]);
    git(&repo, &["remote", "add", "origin", origin]);
    route(served_as, &bare);
    Fixture {
        _tmp: tmp,
        repo: repo.to_string_lossy().into_owned(),
        wip,
    }
}

fn heads(sha: &str, key: &str) -> BTreeMap<String, String> {
    BTreeMap::from([(key.to_string(), sha.to_string())])
}

fn expect(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

#[test]
fn the_branch_head_is_read_from_the_expected_repository() {
    let fx = fixture("one", "git@github.com:acme/one.git", "acme/one");
    let head = remote_branch_head(&fx.repo, "acme/one", BRANCH).unwrap();
    assert_eq!(head.as_deref(), Some(fx.wip.as_str()));
}

#[test]
fn the_branch_head_is_not_read_from_an_origin_of_another_repository() {
    let fx = fixture("two", "git@github.com:other/other.git", "other/other");
    let err = remote_branch_head(&fx.repo, "acme/two", BRANCH).unwrap_err();
    assert!(
        err.message.contains("not the repository linked"),
        "{}",
        err.message
    );
}

#[test]
fn a_push_url_of_another_repository_is_refused_too() {
    let fx = fixture("three", "git@github.com:acme/three.git", "acme/three");
    git(
        Path::new(&fx.repo),
        &[
            "config",
            "remote.origin.pushurl",
            "git@github.com:other/other.git",
        ],
    );
    assert!(remote_branch_head(&fx.repo, "acme/three", BRANCH).is_err());
}

#[test]
fn a_continuation_fetches_the_saved_commit_from_the_expected_repository() {
    let fx = fixture("four", "git@github.com:acme/four.git", "acme/four");
    let dest = clone_dir(&fx.repo, "q_cr1").unwrap();
    GitClone
        .prepare_continue(
            &fx.repo,
            "q_cr1",
            &dest.to_string_lossy(),
            &[],
            BRANCH,
            &heads(&fx.wip, ""),
            &expect(&[("", "acme/four")]),
        )
        .unwrap();
    assert_eq!(git(&dest, &["rev-parse", "HEAD"]), fx.wip);
}

#[test]
fn a_continuation_with_an_origin_of_another_repository_leaves_no_clone() {
    let fx = fixture("five", "git@github.com:other/other.git", "other/other");
    let dest = clone_dir(&fx.repo, "q_cr2").unwrap();
    let err = GitClone
        .prepare_continue(
            &fx.repo,
            "q_cr2",
            &dest.to_string_lossy(),
            &[],
            BRANCH,
            &heads(&fx.wip, ""),
            &expect(&[("", "acme/five")]),
        )
        .unwrap_err();
    assert!(
        err.message.contains("not the repository linked"),
        "{}",
        err.message
    );
    assert!(!dest.exists());
}

#[test]
fn a_continuation_without_a_repository_name_is_refused() {
    let fx = fixture("six", "git@github.com:acme/six.git", "acme/six");
    let dest = clone_dir(&fx.repo, "q_cr3").unwrap();
    assert!(GitClone
        .prepare_continue(
            &fx.repo,
            "q_cr3",
            &dest.to_string_lossy(),
            &[],
            BRANCH,
            &heads(&fx.wip, ""),
            &BTreeMap::new(),
        )
        .is_err());
    assert!(!dest.exists());
}

#[test]
fn a_multi_repository_continuation_checks_every_repository() {
    let good = fixture("seven", "git@github.com:acme/seven.git", "acme/seven");
    let bad = fixture("eight", "git@github.com:other/other.git", "other/other");
    let clones = vec![
        ("seven".to_string(), good.repo.clone()),
        ("eight".to_string(), bad.repo.clone()),
    ];
    let saved = BTreeMap::from([
        ("seven".to_string(), good.wip.clone()),
        ("eight".to_string(), bad.wip.clone()),
    ]);
    let dest = clone_dir(&good.repo, "q_cr4").unwrap();
    let err = GitClone
        .prepare_continue(
            &good.repo,
            "q_cr4",
            &dest.to_string_lossy(),
            &clones,
            BRANCH,
            &saved,
            &expect(&[("seven", "acme/seven"), ("eight", "acme/eight")]),
        )
        .unwrap_err();
    assert!(
        err.message.contains("not the repository linked"),
        "{}",
        err.message
    );
    assert!(!dest.exists());
}
