use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

pub const EVERY: Duration = Duration::from_secs(15 * 60);

pub struct Hit<'a> {
    pub profile: &'a str,
    pub line: &'a str,
    pub at_ms: u64,
}

fn hit_file(hit: &Hit<'_>) -> Option<PathBuf> {
    let dir = tmgr_usage::paths::ensure_data_dir(&tmgr_usage::paths::Env::from_process()).ok()?;
    let path = dir.join(format!("limit-hit-{}.json", hit.at_ms));
    let body = serde_json::json!({"profile": hit.profile, "line": hit.line, "at_ms": hit.at_ms});
    std::fs::write(&path, body.to_string()).ok()?;
    Some(path)
}

pub fn args(hit_path: Option<&Path>) -> Vec<String> {
    let mut a: Vec<String> = ["usage", "machine", "sync", "--auto", "-q"]
        .map(String::from)
        .to_vec();
    if let Some(p) = hit_path {
        a.push("--limit-hit-file".into());
        a.push(p.to_string_lossy().into_owned());
    }
    a
}

/// Fire and forget: a relative `exe` (unit tests) never spawns.
pub fn spawn(exe: &Path, hit: Option<&Hit<'_>>) {
    if !exe.is_absolute() {
        return;
    }
    let path = hit.and_then(hit_file);
    let mut cmd = Command::new(exe);
    cmd.args(args(path.as_deref()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let _ = cmd.spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_is_fixed() {
        assert_eq!(args(None), ["usage", "machine", "sync", "--auto", "-q"]);
        assert_eq!(
            args(Some(Path::new("/d/h.json"))),
            [
                "usage",
                "machine",
                "sync",
                "--auto",
                "-q",
                "--limit-hit-file",
                "/d/h.json"
            ]
        );
    }

    #[test]
    fn a_relative_exe_never_spawns() {
        spawn(Path::new("definitely-not-a-binary"), None);
    }
}
