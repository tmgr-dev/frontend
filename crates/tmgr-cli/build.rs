use std::process::Command;

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn commit() -> String {
    if let Some(sha) = std::env::var("TMGR_BUILD_COMMIT")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        return sha.chars().take(12).collect();
    }
    git(&["rev-parse", "--short=12", "HEAD"]).unwrap_or_else(|| "unknown".to_string())
}

fn main() {
    println!("cargo:rustc-env=TMGR_COMMIT={}", commit());
    println!(
        "cargo:rustc-env=TMGR_TARGET={}",
        std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string())
    );
    println!("cargo:rerun-if-env-changed=TMGR_BUILD_COMMIT");
    println!("cargo:rerun-if-env-changed=TARGET");

    let git_path = |p: &str| git(&["rev-parse", "--path-format=absolute", "--git-path", p]);
    for p in ["HEAD", "packed-refs"] {
        if let Some(path) = git_path(p) {
            println!("cargo:rerun-if-changed={path}");
        }
    }
    if let Some(path) = git(&["symbolic-ref", "-q", "HEAD"]).and_then(|r| git_path(&r)) {
        println!("cargo:rerun-if-changed={path}");
    }
}
