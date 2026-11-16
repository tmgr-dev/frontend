use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, Default)]
pub struct GitState {
    pub status: String,
    pub diff_stat: String,
    pub log: String,
}

fn git(dir: &Path, args: &[&str]) -> String {
    Command::new("git")
        .args(["-c", "core.fsmonitor=false", "-C"])
        .arg(dir)
        .args(args)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

pub fn collect(dir: &Path) -> Option<GitState> {
    if !dir.is_dir() || git(dir, &["rev-parse", "--is-inside-work-tree"]) != "true" {
        return None;
    }
    Some(GitState {
        status: git(dir, &["status", "--short", "--branch"]),
        diff_stat: git(dir, &["diff", "--no-ext-diff", "--stat", "HEAD"]),
        log: git(dir, &["log", "--oneline", "-5"]),
    })
}
