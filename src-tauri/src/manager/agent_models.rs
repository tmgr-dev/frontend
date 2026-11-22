use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

const MAX_ENTRIES: usize = 200;
const MAX_VALUE_LEN: usize = 64;
const MAX_INPUT_BYTES: u64 = 1024 * 1024;
const CLI_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelChoice {
  pub value: String,
  pub label: String,
}

fn valid_value(value: &str) -> bool {
  (1..=MAX_VALUE_LEN).contains(&value.len())
    && value.bytes().all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b':' | b'/' | b'#' | b'-'))
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

pub fn parse_agy(stdout: &str) -> Vec<ModelChoice> {
  let mut out: Vec<ModelChoice> = Vec::new();
  for line in stdout.lines() {
    let line = line.trim();
    let (slug, name) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
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

fn find_cli(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
  dirs.iter().filter(|d| d.is_absolute()).find_map(|d| {
    let candidates = if cfg!(windows) { vec![format!("{name}.exe"), format!("{name}.cmd")] } else { vec![name.to_owned()] };
    candidates.into_iter().map(|c| d.join(c)).find(|p| p.is_file())
  })
}

fn run_models(cli: &Path) -> Result<String, String> {
  let mut child = Command::new(cli)
    .arg("models")
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()
    .map_err(|e| e.kind().to_string())?;
  let stdout = child.stdout.take().ok_or("no stdout")?;
  let reader = std::thread::spawn(move || {
    let mut buf = Vec::new();
    let _ = stdout.take(MAX_INPUT_BYTES).read_to_end(&mut buf);
    buf
  });
  let deadline = Instant::now() + CLI_TIMEOUT;
  let status = loop {
    match child.try_wait() {
      Ok(Some(status)) => break status,
      Ok(None) if Instant::now() >= deadline => {
        let _ = child.kill();
        let _ = child.wait();
        return Err("timed out".to_owned());
      }
      Ok(None) => std::thread::sleep(Duration::from_millis(10)),
      Err(e) => return Err(e.kind().to_string()),
    }
  };
  let bytes = reader.join().map_err(|_| "reader failed".to_owned())?;
  if !status.success() {
    return Err("command failed".to_owned());
  }
  Ok(String::from_utf8_lossy(&bytes).into_owned())
}

pub fn load(agent: &str, home: Option<&Path>) -> Result<Vec<ModelChoice>, String> {
  let list = match agent {
    "codex" => {
      let home = home.ok_or("home directory unavailable")?;
      parse_codex(&read_capped(&home.join(".codex").join("models_cache.json"))?)
    }
    "agy" | "opencode" => {
      let cli = find_cli(agent, &cli_dirs(home)).ok_or_else(|| format!("{agent} not found"))?;
      let out = run_models(&cli)?;
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
    let long = "a".repeat(65);
    let ok = "a".repeat(64);
    let input = format!("{long}  Too long\n{ok}  Fits\nbad$slug  Nope\nopen/ai#x:1_2.3-4  Ok\n");
    assert_eq!(values(&parse_agy(&input)), ["default", ok.as_str(), "open/ai#x:1_2.3-4"]);
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
  fn unknown_agent_is_an_error() {
    assert!(load("claude", None).is_err());
  }
}
