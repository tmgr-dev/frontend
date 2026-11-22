use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

const MAX_ENTRIES: usize = 200;
const MAX_VALUE_LEN: usize = 100;
const MAX_INPUT_BYTES: u64 = 1024 * 1024;
const CLI_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelChoice {
  pub value: String,
  pub label: String,
}

fn valid_value(value: &str) -> bool {
  (1..=MAX_VALUE_LEN).contains(&value.len())
    && value.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b':' | b'/' | b'#' | b'-' | b'@'))
}

fn choice(value: &str, label: &str) -> Option<ModelChoice> {
  let label = label.trim();
  valid_value(value).then(|| ModelChoice {
    value: value.to_owned(),
    label: if label.is_empty() { value.to_owned() } else { label.chars().take(100).collect() },
  })
}

fn with_default(mut list: Vec<ModelChoice>) -> Vec<ModelChoice> {
  list.retain(|m| m.value != "default");
  list.truncate(MAX_ENTRIES - 1);
  list.insert(0, ModelChoice { value: "default".to_owned(), label: "default".to_owned() });
  list
}

#[derive(Deserialize)]
struct CodexCache {
  models: Vec<serde_json::Value>,
}

pub fn parse_codex(json: &str) -> Vec<ModelChoice> {
  let Ok(cache) = serde_json::from_str::<CodexCache>(json) else { return Vec::new() };
  let mut rows: Vec<(f64, ModelChoice)> = cache
    .models
    .iter()
    .filter(|m| m.get("visibility").and_then(|v| v.as_str()) == Some("list"))
    .filter_map(|m| {
      let slug = m.get("slug")?.as_str()?;
      let name = m.get("display_name").and_then(|v| v.as_str()).unwrap_or("");
      let priority = m.get("priority").and_then(|v| v.as_f64()).unwrap_or(f64::MAX);
      choice(slug, name).map(|c| (priority, c))
    })
    .collect();
  rows.sort_by(|a, b| a.0.total_cmp(&b.0));
  let mut out: Vec<ModelChoice> = Vec::new();
  for (_, c) in rows {
    if !out.iter().any(|o| o.value == c.value) {
      out.push(c);
    }
    if out.len() == MAX_ENTRIES {
      break;
    }
  }
  out
}

fn valid_agy_slug(slug: &str) -> bool {
  let b = slug.as_bytes();
  !b.is_empty()
    && (b[0].is_ascii_lowercase() || b[0].is_ascii_digit())
    && b.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, b'.' | b'_' | b':' | b'-'))
    && !slug.ends_with(':')
}

pub fn parse_agy(stdout: &str) -> Vec<ModelChoice> {
  let mut out: Vec<ModelChoice> = Vec::new();
  for line in stdout.lines() {
    let line = line.trim();
    let Some((slug, name)) = line.split_once(char::is_whitespace) else { continue };
    if !valid_agy_slug(slug) || name.trim().is_empty() {
      continue;
    }
    if let Some(c) = choice(slug, name) {
      if !out.iter().any(|o| o.value == c.value) {
        out.push(c);
      }
    }
    if out.len() == MAX_ENTRIES - 1 {
      break;
    }
  }
  with_default(out)
}

pub fn parse_opencode(stdout: &str) -> Vec<ModelChoice> {
  let mut out: Vec<ModelChoice> = Vec::new();
  for line in stdout.lines() {
    let line = line.trim();
    if !line.contains('/') || line.contains(char::is_whitespace) || line.ends_with(':') {
      continue;
    }
    if let Some(c) = choice(line, line) {
      if !out.iter().any(|o| o.value == c.value) {
        out.push(c);
      }
    }
    if out.len() == MAX_ENTRIES - 1 {
      break;
    }
  }
  with_default(out)
}

fn read_capped(path: &Path) -> Result<String, String> {
  let file = std::fs::File::open(path).map_err(|e| e.kind().to_string())?;
  let mut buf = String::new();
  file.take(MAX_INPUT_BYTES).read_to_string(&mut buf).map_err(|e| e.kind().to_string())?;
  Ok(buf)
}

fn cli_dirs(home: Option<&Path>) -> Vec<PathBuf> {
  let mut dirs: Vec<PathBuf> = std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
  if let Some(home) = home {
    for rel in [".local/bin", ".opencode/bin", ".bun/bin", ".cargo/bin", "bin"] {
      dirs.push(home.join(rel));
    }
  }
  dirs.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"].map(PathBuf::from));
  dirs
}

fn is_executable(path: &Path) -> bool {
  let Ok(meta) = std::fs::metadata(path) else { return false };
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    meta.is_file() && meta.permissions().mode() & 0o111 != 0
  }
  #[cfg(not(unix))]
  {
    meta.is_file()
  }
}

fn find_cli(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
  dirs.iter().filter(|d| d.is_absolute()).find_map(|d| {
    let candidates = if cfg!(windows) { vec![format!("{name}.exe"), format!("{name}.cmd")] } else { vec![name.to_owned()] };
    candidates.into_iter().map(|c| d.join(c)).find(|p| is_executable(p))
  })
}

fn run_models(cli: &Path, cwd: &Path) -> Result<String, String> {
  run_models_at(cli, &["models"], cwd, CLI_TIMEOUT, MAX_INPUT_BYTES as usize)
}

fn kill_tree(child: &mut std::process::Child) {
  #[cfg(unix)]
  unsafe {
    libc::killpg(child.id() as i32, libc::SIGKILL);
  }
  let _ = child.kill();
  let _ = child.wait();
}

fn run_models_at(cli: &Path, args: &[&str], cwd: &Path, timeout: Duration, cap: usize) -> Result<String, String> {
  let mut cmd = Command::new(cli);
  cmd
    .args(args)
    .current_dir(cwd)
    .env_remove("OPENCODE_CONFIG")
    .env_remove("OPENCODE_CONFIG_DIR")
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null());
  #[cfg(unix)]
  std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
  let mut child = cmd.spawn().map_err(|e| e.kind().to_string())?;
  let stdout = child.stdout.take().ok_or("no stdout")?;
  let (tx, rx) = mpsc::channel();
  std::thread::spawn(move || {
    let mut buf = Vec::new();
    let _ = stdout.take(cap as u64 + 1).read_to_end(&mut buf);
    let _ = tx.send(buf);
  });
  let fail = |child: &mut std::process::Child, msg: &str| {
    kill_tree(child);
    Err(msg.to_owned())
  };
  let deadline = Instant::now() + timeout;
  let mut output: Option<Vec<u8>> = None;
  let mut status = None;
  loop {
    if output.is_none() {
      match rx.try_recv() {
        Ok(bytes) if bytes.len() > cap => return fail(&mut child, "output too large"),
        Ok(bytes) => output = Some(bytes),
        Err(mpsc::TryRecvError::Empty) => {}
        Err(mpsc::TryRecvError::Disconnected) => return fail(&mut child, "reader failed"),
      }
    }
    if status.is_none() {
      status = child.try_wait().map_err(|e| e.kind().to_string())?;
    }
    if let (Some(status), Some(bytes)) = (&status, &output) {
      if !status.success() {
        return Err("command failed".to_owned());
      }
      kill_tree(&mut child);
      return Ok(String::from_utf8_lossy(bytes).into_owned());
    }
    if Instant::now() >= deadline {
      return fail(&mut child, "timed out");
    }
    std::thread::sleep(Duration::from_millis(10));
  }
}

pub fn load(agent: &str, home: Option<&Path>) -> Result<Vec<ModelChoice>, String> {
  let list = match agent {
    "codex" => {
      let home = home.ok_or("home directory unavailable")?;
      parse_codex(
        &read_capped(&home.join(".codex").join("models_cache.json")).map_err(|_| "no codex model cache".to_owned())?,
      )
    }
    "agy" | "opencode" => {
      let cli = find_cli(agent, &cli_dirs(home)).ok_or_else(|| format!("{agent} not found"))?;
      let cwd = home.map(Path::to_path_buf).unwrap_or_else(std::env::temp_dir);
      let out = run_models(&cli, &cwd)?;
      if agent == "agy" {
        parse_agy(&out)
      } else {
        parse_opencode(&out)
      }
    }
    _ => return Err(format!("no model list for {agent}")),
  };
  if list.iter().all(|m| m.value == "default") {
    return Err("no models listed".to_owned());
  }
  Ok(list)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn values(list: &[ModelChoice]) -> Vec<&str> {
    list.iter().map(|m| m.value.as_str()).collect()
  }

  #[test]
  fn agy_parses_slug_and_display_name_with_default_first() {
    let list = parse_agy("gemini-3.8-flash-medium  Gemini 3.8 Flash (Medium)\nclaude-x   Claude X\n");
    assert_eq!(values(&list), ["default", "gemini-3.8-flash-medium", "claude-x"]);
    assert_eq!(list[1].label, "Gemini 3.8 Flash (Medium)");
  }

  #[test]
  fn opencode_uses_whole_line_as_value_and_label() {
    let list = parse_opencode("anthropic/claude-sonnet-5-5\nopenai/gpt-6\n");
    assert_eq!(values(&list), ["default", "anthropic/claude-sonnet-5-5", "openai/gpt-6"]);
    assert_eq!(list[1].label, "anthropic/claude-sonnet-5-5");
  }

  #[test]
  fn codex_keeps_listed_entries_ordered_by_priority() {
    let json = r#"{"models":[
      {"slug":"gpt-hidden","display_name":"Hidden","visibility":"hide","priority":0},
      {"slug":"gpt-b","display_name":"GPT B","visibility":"list","priority":5},
      {"slug":"gpt-a","display_name":"GPT A","visibility":"list","priority":1},
      {"slug":"gpt-nop","display_name":"No Priority","visibility":"list"}
    ],"api_key":"secret"}"#;
    let list = parse_codex(json);
    assert_eq!(values(&list), ["gpt-a", "gpt-b", "gpt-nop"]);
    assert_eq!(list[0].label, "GPT A");
  }

  #[test]
  fn empty_and_garbage_input_yield_no_models() {
    assert!(parse_codex("").is_empty());
    assert!(parse_codex("not json").is_empty());
    assert!(parse_codex(r#"{"models":"x"}"#).is_empty());
    assert!(parse_codex(r#"{"models":[{"slug":5,"visibility":"list"}]}"#).is_empty());
    assert_eq!(values(&parse_agy("")), ["default"]);
    assert_eq!(values(&parse_opencode("\n\n")), ["default"]);
    assert_eq!(values(&parse_opencode("bad line with spaces\n$(rm -rf)\n;ls\n")), ["default"]);
  }

  #[test]
  fn invalid_values_and_oversized_slugs_are_dropped() {
    let long = "a".repeat(101);
    let ok = "a".repeat(100);
    let input = format!("{long}  Too long\n{ok}  Fits\nbad$slug  Nope\nx.y:1-2  Ok\n");
    assert_eq!(values(&parse_agy(&input)), ["default", ok.as_str(), "x.y:1-2"]);
  }

  #[test]
  fn entries_are_capped() {
    let input: String = (0..500).map(|i| format!("m{i}  Model {i}\n")).collect();
    assert_eq!(parse_agy(&input).len(), MAX_ENTRIES);
    let lines: String = (0..500).map(|i| format!("p/m{i}\n")).collect();
    assert_eq!(parse_opencode(&lines).len(), MAX_ENTRIES);
    let codex = format!(
      r#"{{"models":[{}]}}"#,
      (0..500).map(|i| format!(r#"{{"slug":"m{i}","display_name":"M","visibility":"list","priority":{i}}}"#)).collect::<Vec<_>>().join(",")
    );
    assert_eq!(parse_codex(&codex).len(), MAX_ENTRIES);
  }

  #[test]
  fn duplicates_and_cli_default_are_collapsed() {
    let list = parse_agy("default  Fake\nm1  A\nm1  B\n");
    assert_eq!(values(&list), ["default", "m1"]);
    assert_eq!(list[1].label, "A");
  }

  #[test]
  fn agy_skips_headers_logs_and_single_column_lines() {
    let list = parse_agy("Available models:\nLoading...\nlonely\nm1  Model One\n");
    assert_eq!(values(&list), ["default", "m1"]);
  }

  #[test]
  fn opencode_requires_provider_slash_model() {
    let list = parse_opencode("Available models:\nLoading...\nnoslash\nprov/:\ncloudflare-workers-ai/@cf/meta/x\n");
    assert_eq!(values(&list), ["default", "cloudflare-workers-ai/@cf/meta/x"]);
  }

  #[cfg(unix)]
  #[test]
  fn find_cli_skips_non_executable_files() {
    use std::os::unix::fs::PermissionsExt;
    let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    std::fs::write(a.path().join("agy"), "x").unwrap();
    std::fs::write(b.path().join("agy"), "x").unwrap();
    std::fs::set_permissions(b.path().join("agy"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let found = find_cli("agy", &[a.path().to_path_buf(), b.path().to_path_buf()]);
    assert_eq!(found, Some(b.path().join("agy")));
  }

  #[test]
  fn missing_codex_cache_has_a_clear_error() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(load("codex", Some(dir.path())), Err("no codex model cache".to_owned()));
  }

  #[test]
  fn unknown_agent_is_an_error() {
    assert!(load("claude", None).is_err());
  }

  #[cfg(unix)]
  mod process {
    use super::super::*;
    use std::os::unix::fs::PermissionsExt;

    fn script(dir: &tempfile::TempDir, body: &str) -> PathBuf {
      let path = dir.path().join("fake-cli");
      std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
      std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
      path
    }

    const SHORT: Duration = Duration::from_millis(500);

    #[test]
    fn normal_output_is_returned() {
      let dir = tempfile::tempdir().unwrap();
      let cli = script(&dir, "echo 'anthropic/claude-sonnet-5-5'");
      assert_eq!(run_models_at(&cli, &["models"], dir.path(), Duration::from_secs(5), 1024).unwrap(), "anthropic/claude-sonnet-5-5\n");
    }

    #[test]
    fn output_over_the_cap_is_an_error() {
      let dir = tempfile::tempdir().unwrap();
      let cli = script(&dir, "yes model");
      assert_eq!(run_models_at(&cli, &["models"], dir.path(), Duration::from_secs(5), 1024), Err("output too large".to_owned()));
    }

    #[test]
    fn descendant_holding_stdout_does_not_hang() {
      let dir = tempfile::tempdir().unwrap();
      let cli = script(&dir, "sleep 30 &\necho partial\nexit 0");
      let started = Instant::now();
      assert_eq!(run_models_at(&cli, &["models"], dir.path(), SHORT, 1024), Err("timed out".to_owned()));
      assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn failing_command_is_an_error() {
      let dir = tempfile::tempdir().unwrap();
      let cli = script(&dir, "echo x\nexit 3");
      assert_eq!(run_models_at(&cli, &["models"], dir.path(), SHORT, 1024), Err("command failed".to_owned()));
    }

    #[test]
    fn child_runs_in_the_given_directory() {
      let dir = tempfile::tempdir().unwrap();
      let cwd = tempfile::tempdir().unwrap();
      let cli = script(&dir, "pwd");
      let out = run_models_at(&cli, &["models"], cwd.path(), Duration::from_secs(5), 1024).unwrap();
      assert_eq!(std::fs::canonicalize(out.trim()).unwrap(), std::fs::canonicalize(cwd.path()).unwrap());
    }

    #[test]
    fn timeout_kills_the_whole_process_tree() {
      let dir = tempfile::tempdir().unwrap();
      let pidfile = dir.path().join("pid");
      let cli = script(&dir, &format!("sleep 30 &\necho $! > {}\necho partial\nexit 0", pidfile.display()));
      assert!(run_models_at(&cli, &["models"], dir.path(), SHORT, 1024).is_err());
      let pid: i32 = std::fs::read_to_string(&pidfile).unwrap().trim().parse().unwrap();
      let deadline = Instant::now() + Duration::from_secs(3);
      while unsafe { libc::kill(pid, 0) } == 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(20));
      }
      assert_ne!(unsafe { libc::kill(pid, 0) }, 0);
    }

    #[test]
    fn missing_command_is_an_error() {
      let dir = tempfile::tempdir().unwrap();
      assert!(run_models_at(&dir.path().join("nope"), &["models"], dir.path(), SHORT, 1024).is_err());
    }
  }
}
