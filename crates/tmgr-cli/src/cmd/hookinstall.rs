use crate::cli_agent::{HookInstallArgs, HookTarget};
use crate::cmd::init::{self, Change};
use crate::ctx::Ctx;
use crate::gitx;
use crate::interact;
use crate::out::Output;
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::fs;
use std::path::{Path, PathBuf};
use tmgr_core::error::{Error, Result};

const MAX_FILE: u64 = 1024 * 1024;
const RUN_TOOLS_MATCHER: &str = "mcp__.*__(start_agent_work|finish_agent_work)$";
const CLAUDE_EVENTS: [(&str, &str, Option<&str>); 4] = [
    ("Notification", "notification", None),
    ("UserPromptSubmit", "user-prompt-submit", None),
    ("Stop", "stop", None),
    ("PostToolUse", "post-tool-use", Some(RUN_TOOLS_MATCHER)),
];

pub fn shell_quote(s: &str) -> String {
    if !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_@%+=:,./-".contains(&b))
    {
        s.to_string()
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

pub fn shell_split(cmd: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut started = false;
    let mut chars = cmd.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                started = true;
                loop {
                    match chars.next()? {
                        '\'' => break,
                        ch => cur.push(ch),
                    }
                }
            }
            '"' => {
                started = true;
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => match chars.next()? {
                            e @ ('"' | '\\' | '$' | '`') => cur.push(e),
                            other => {
                                cur.push('\\');
                                cur.push(other);
                            }
                        },
                        '$' | '`' => return None,
                        ch => cur.push(ch),
                    }
                }
            }
            '\\' => {
                started = true;
                cur.push(chars.next()?);
            }
            ';' | '|' | '&' | '<' | '>' | '(' | ')' | '`' | '$' | '\n' => return None,
            c if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            c => {
                started = true;
                cur.push(c);
            }
        }
    }
    if started {
        out.push(cur);
    }
    Some(out)
}

fn basename(p: &str) -> &str {
    p.rsplit('/').next().unwrap_or(p)
}

fn legacy_event(sub: &str) -> Option<&'static str> {
    match sub {
        "notification" => Some("notification"),
        "prompt" => Some("user-prompt-submit"),
        "stop" => Some("stop"),
        _ => None,
    }
}

pub fn classify_claude(cmd: &str) -> Option<&'static str> {
    let t = shell_split(cmd)?;
    let first = basename(t.first()?);
    if first == "tmgr" && t.len() == 4 && t[1] == "hook" && t[2] == "claude" {
        return CLAUDE_EVENTS
            .iter()
            .map(|(_, e, _)| *e)
            .find(|e| *e == t[3]);
    }
    if first == "tmgr-notify" && t.len() == 3 && t[1] == "hook" {
        return legacy_event(&t[2]);
    }
    if first == "npx" {
        let rest: Vec<&String> = t[1..].iter().filter(|a| !a.starts_with('-')).collect();
        if rest.len() == 3
            && (rest[0] == "@tmgr/notify" || rest[0].starts_with("@tmgr/notify@"))
            && rest[1] == "hook"
        {
            return legacy_event(rest[2]);
        }
    }
    None
}

fn is_ours_codex(arr: &[String]) -> bool {
    let Some(first) = arr.first() else {
        return false;
    };
    let name = basename(first);
    (name == "tmgr" && arr.len() == 4 && arr[1..] == ["hook", "codex", "notify"])
        || (name == "tmgr-notify" && arr.len() == 3 && arr[1..] == ["hook", "codex"])
        || (name == "npx"
            && arr[1..]
                .iter()
                .filter(|a| !a.starts_with('-'))
                .map(String::as_str)
                .collect::<Vec<_>>()
                == ["@tmgr/notify", "hook", "codex"])
}

fn invalid(what: &str) -> Error {
    Error::usage(format!(
        "{what} is not in the expected shape; nothing was changed"
    ))
}

fn indent_of(text: &str) -> Vec<u8> {
    text.lines()
        .find(|l| l.starts_with([' ', '\t']))
        .map(|l| {
            l.bytes()
                .take_while(|b| *b == b' ' || *b == b'\t')
                .collect::<Vec<u8>>()
        })
        .unwrap_or_else(|| b"  ".to_vec())
}

fn render_json(value: &Value, original: &str) -> String {
    let indent = indent_of(original);
    let fmt = serde_json::ser::PrettyFormatter::with_indent(&indent);
    let mut buf = Vec::new();
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, fmt);
    if value.serialize(&mut ser).is_err() {
        return original.to_string();
    }
    let mut text = String::from_utf8_lossy(&buf).to_string();
    if original.is_empty() || original.ends_with('\n') {
        text.push('\n');
    }
    text
}

fn our_hook(cmd: &str) -> Value {
    json!({"type": "command", "command": cmd})
}

fn our_group(matcher: Option<&str>, cmd: &str) -> Value {
    match matcher {
        Some(m) => json!({"matcher": m, "hooks": [our_hook(cmd)]}),
        None => json!({"hooks": [our_hook(cmd)]}),
    }
}

fn desired_command(exe: &str, sub: &str) -> String {
    format!("{} hook claude {sub}", shell_quote(exe))
}

pub fn claude_snippet(exe: &str) -> String {
    let mut hooks = Map::new();
    for (event, sub, matcher) in CLAUDE_EVENTS {
        hooks.insert(
            event.to_string(),
            json!([our_group(matcher, &desired_command(exe, sub))]),
        );
    }
    let mut text = serde_json::to_string_pretty(&json!({ "hooks": hooks })).unwrap_or_default();
    text.push('\n');
    text
}

pub fn codex_snippet(exe: &str) -> String {
    format!("notify = {}\n", codex_array(exe))
}

fn codex_array(exe: &str) -> String {
    let quoted = |s: &str| serde_json::to_string(s).unwrap_or_default();
    format!("[{}, \"hook\", \"codex\", \"notify\"]", quoted(exe))
}

pub fn plan_claude(old: &str, exe: &str, remove: bool) -> Result<String> {
    let mut root: Value = if old.trim().is_empty() {
        Value::Object(Map::new())
    } else {
        serde_json::from_str(old).map_err(|_| {
            Error::usage("the settings file is not valid JSON; fix it first (nothing was changed)")
        })?
    };
    let original = root.clone();
    let obj = root
        .as_object_mut()
        .ok_or_else(|| invalid("the settings file"))?;
    if remove && !obj.contains_key("hooks") {
        return Ok(old.to_string());
    }
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| invalid("\"hooks\""))?;
    let mut touched = false;
    if remove {
        let events: Vec<String> = hooks.keys().cloned().collect();
        for event in events {
            let Some(arr) = hooks.get_mut(&event).and_then(Value::as_array_mut) else {
                continue;
            };
            touched |= strip_ours(arr);
            if arr.is_empty() {
                hooks.remove(&event);
            }
        }
        if !touched {
            return Ok(old.to_string());
        }
        if hooks.is_empty() {
            obj.remove("hooks");
        }
    } else {
        for (event, sub, matcher) in CLAUDE_EVENTS {
            let arr = hooks
                .entry(event)
                .or_insert_with(|| Value::Array(Vec::new()))
                .as_array_mut()
                .ok_or_else(|| invalid(&format!("\"{event}\" hooks")))?;
            install_event(arr, sub, matcher, &desired_command(exe, sub));
        }
    }
    Ok(if root == original && !old.trim().is_empty() {
        old.to_string()
    } else {
        render_json(&root, old)
    })
}

fn strip_ours(arr: &mut Vec<Value>) -> bool {
    let mut removed = false;
    for group in arr.iter_mut() {
        let Some(inner) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
            continue;
        };
        let before = inner.len();
        inner.retain(|h| {
            h.get("command")
                .and_then(Value::as_str)
                .and_then(classify_claude)
                .is_none()
        });
        removed |= inner.len() != before;
    }
    arr.retain(|g| !is_empty_group(g));
    removed
}

fn is_empty_group(g: &Value) -> bool {
    g.get("hooks")
        .and_then(Value::as_array)
        .is_some_and(Vec::is_empty)
        && g.as_object()
            .is_some_and(|o| o.keys().all(|k| k == "hooks" || k == "matcher"))
}

fn install_event(arr: &mut Vec<Value>, sub: &str, matcher: Option<&str>, desired: &str) {
    let mut found = false;
    for group in arr.iter_mut() {
        let Some(inner) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
            continue;
        };
        for h in inner.iter_mut() {
            let is_ours = h
                .get("command")
                .and_then(Value::as_str)
                .and_then(classify_claude)
                == Some(sub);
            if is_ours && !found {
                found = true;
                if h.get("command").and_then(Value::as_str) != Some(desired) {
                    h["command"] = json!(desired);
                }
            }
        }
    }
    if found {
        strip_duplicates(arr, sub, desired);
    } else {
        arr.push(our_group(matcher, desired));
    }
}

fn strip_duplicates(arr: &mut Vec<Value>, sub: &str, desired: &str) {
    let mut seen = false;
    for group in arr.iter_mut() {
        let Some(inner) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
            continue;
        };
        inner.retain(|h| {
            let ours = h
                .get("command")
                .and_then(Value::as_str)
                .and_then(classify_claude)
                == Some(sub);
            if !ours {
                return true;
            }
            if seen {
                return false;
            }
            seen = h.get("command").and_then(Value::as_str) == Some(desired);
            true
        });
    }
    arr.retain(|g| {
        !g.get("hooks")
            .and_then(Value::as_array)
            .is_some_and(Vec::is_empty)
    });
}

pub fn plan_codex(old: &str, exe: &str, remove: bool) -> Result<String> {
    let mut doc: toml_edit::DocumentMut = old.parse().map_err(|_| {
        Error::usage("config.toml is not valid TOML; fix it first (nothing was changed)")
    })?;
    let current: Option<Vec<String>> = doc.get("notify").and_then(|i| i.as_array()).map(|a| {
        a.iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    });
    let has_notify = doc.get("notify").is_some();
    match (&current, has_notify) {
        (Some(arr), _) if is_ours_codex(arr) => {
            if remove {
                doc.remove("notify");
            } else {
                let desired = [exe, "hook", "codex", "notify"];
                if arr.iter().map(String::as_str).ne(desired) {
                    doc["notify"] =
                        toml_edit::value(desired.iter().copied().collect::<toml_edit::Array>());
                }
            }
        }
        (_, true) => {
            if remove {
                return Ok(old.to_string());
            }
            return Err(Error::conflict(
                "config.toml already sets a notify program that is not tmgr",
            )
            .hint(format!(
                "Codex runs one notify program; replace it by hand with: {}",
                codex_snippet(exe).trim()
            )));
        }
        (_, false) => {
            if remove {
                return Ok(old.to_string());
            }
            let desired = [exe, "hook", "codex", "notify"];
            doc["notify"] = toml_edit::value(desired.iter().copied().collect::<toml_edit::Array>());
        }
    }
    let out = doc.to_string();
    Ok(if out == old { old.to_string() } else { out })
}

fn env_abs(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
}

fn home() -> Result<PathBuf> {
    env_abs("HOME").ok_or_else(|| Error::internal("HOME is not set"))
}

pub fn target_path(target: HookTarget, project: bool) -> Result<PathBuf> {
    match (target, project) {
        (HookTarget::Codex, true) => Err(Error::usage(
            "Codex reads `notify` from the user config only; --project does not apply",
        )),
        (HookTarget::Codex, false) => Ok(env_abs("CODEX_HOME")
            .unwrap_or(home()?.join(".codex"))
            .join("config.toml")),
        (HookTarget::Claude, false) => Ok(env_abs("CLAUDE_CONFIG_DIR")
            .unwrap_or(home()?.join(".claude"))
            .join("settings.json")),
        (HookTarget::Claude, true) => {
            let cwd = std::env::current_dir()?;
            let root = gitx::Git::at(&cwd).toplevel().unwrap_or(cwd);
            Ok(root.join(".claude").join("settings.json"))
        }
    }
}

fn read_existing(path: &Path) -> Result<(PathBuf, String)> {
    let real = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    match fs::metadata(&real) {
        Ok(m) if m.len() > MAX_FILE => Err(Error::usage("the file is too large to edit")),
        Ok(m) if !m.is_file() => Err(Error::usage("the target is not a regular file")),
        Ok(_) => fs::read_to_string(&real)
            .map(|t| (real, t))
            .map_err(|_| Error::usage("the file is not readable UTF-8 text")),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((real, String::new())),
        Err(_) => Err(Error::internal("cannot read the file")),
    }
}

pub fn plan(target: HookTarget, path: &Path, exe: &str, remove: bool) -> Result<Change> {
    let (real, old) = read_existing(path)?;
    let new = match target {
        HookTarget::Claude => plan_claude(&old, exe, remove)?,
        HookTarget::Codex => plan_codex(&old, exe, remove)?,
    };
    Ok(Change {
        path: real,
        old,
        new,
    })
}

fn exe_path() -> Result<String> {
    std::env::current_exe()?
        .to_str()
        .map(str::to_string)
        .ok_or_else(|| Error::internal("the path of this binary is not valid UTF-8"))
}

pub fn run(ctx: &Ctx, args: &HookInstallArgs, remove: bool) -> Result<Output> {
    let exe = exe_path()?;
    let path = target_path(args.target, args.project)?;
    let name = match args.target {
        HookTarget::Claude => "claude",
        HookTarget::Codex => "codex",
    };
    let scope = if args.project { "project" } else { "user" };
    if args.print && !remove {
        let snippet = match args.target {
            HookTarget::Claude => claude_snippet(&exe),
            HookTarget::Codex => codex_snippet(&exe),
        };
        return Ok(Output::new(
            json!({"target": name, "scope": scope, "file": path.display().to_string(), "snippet": snippet, "changed": false, "backup": null}),
            snippet.trim_end().to_string(),
        ));
    }
    let change = plan(args.target, &path, &exe, remove)?;
    let file = change.path.display().to_string();
    if change.old == change.new {
        let what = if remove {
            "nothing to remove"
        } else {
            "already installed"
        };
        return Ok(Output::new(
            json!({"target": name, "scope": scope, "file": file, "changed": false, "backup": null}),
            ctx.style.muted(&format!("{file}: {what}")),
        ));
    }
    let diff = init::diff(&change.old, &change.new);
    if args.print {
        return Ok(Output::new(
            json!({"target": name, "scope": scope, "file": file, "diff": diff, "changed": false, "backup": null}),
            format!("{file}\n{}", diff.trim_end()),
        ));
    }
    if !ctx.json {
        eprintln!("{file}");
        eprint!("{diff}");
    }
    interact::confirm(
        &format!("Write these changes to {file}?"),
        args.project,
        ctx.yes,
    )?;
    let stamp = chrono::Local::now().format("%Y%m%d%H%M%S").to_string();
    let backup = init::apply(&change, &stamp)?;
    if args.project && !remove {
        ctx.warn(
            "project_hook_path",
            "the hook commands contain the absolute path of this binary; keep the file out of version control",
        );
    }
    let mut human = format!(
        "{} {} {file}",
        ctx.style.good("\u{2713}"),
        if remove {
            "removed tmgr hooks from"
        } else {
            "added tmgr hooks to"
        }
    );
    if let Some(b) = &backup {
        human.push_str(&format!("\n  backup: {}", b.display()));
    }
    if !remove {
        human.push_str("\n  restart the agent for the hooks to take effect");
    }
    Ok(Output::new(
        json!({"target": name, "scope": scope, "file": file, "changed": true, "backup": backup.map(|b| b.display().to_string())}),
        human,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXE: &str = "/opt/tmgr/bin/tmgr";

    fn cmds(v: &Value, event: &str) -> Vec<String> {
        v["hooks"][event]
            .as_array()
            .map(|groups| {
                groups
                    .iter()
                    .flat_map(|g| g["hooks"].as_array().cloned().unwrap_or_default())
                    .filter_map(|h| h["command"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn parsed(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn quoting_round_trips_through_the_splitter() {
        for p in [
            "/opt/tmgr/bin/tmgr",
            "/opt/my tools/bin/tmgr",
            "/x/it's/tmgr",
            "/x/$HOME/t",
        ] {
            assert_eq!(
                shell_split(&shell_quote(p)).unwrap(),
                vec![p.to_string()],
                "{p}"
            );
        }
        assert_eq!(shell_quote("/a b/t"), "'/a b/t'");
        assert!(shell_split("a; rm -rf x").is_none());
        assert!(shell_split("a $(b)").is_none());
        assert!(shell_split("'unterminated").is_none());
    }

    #[test]
    fn our_commands_and_the_legacy_ones_are_recognised() {
        assert_eq!(classify_claude("/opt/tmgr hook claude stop"), Some("stop"));
        assert_eq!(
            classify_claude("'/a b/tmgr' hook claude notification"),
            Some("notification")
        );
        assert_eq!(
            classify_claude("npx -y @tmgr/notify hook prompt"),
            Some("user-prompt-submit")
        );
        assert_eq!(
            classify_claude("npx -y @tmgr/notify@0.2.0 hook stop"),
            Some("stop")
        );
        assert_eq!(
            classify_claude("tmgr-notify hook notification"),
            Some("notification")
        );
        assert_eq!(
            classify_claude("/opt/tmgr hook claude post-tool-use"),
            Some("post-tool-use")
        );
        assert_eq!(classify_claude("/opt/tmgr hook claude pre-tool-use"), None);
        assert_eq!(classify_claude("echo hook claude stop"), None);
        assert_eq!(classify_claude("my-linter && tmgr hook claude stop"), None);
    }

    #[test]
    fn install_into_an_empty_file_adds_four_hooks() {
        let out = plan_claude("", EXE, false).unwrap();
        let v = parsed(&out);
        assert_eq!(cmds(&v, "Stop"), vec![format!("{EXE} hook claude stop")]);
        assert_eq!(cmds(&v, "Notification").len(), 1);
        assert_eq!(cmds(&v, "UserPromptSubmit").len(), 1);
        assert_eq!(cmds(&v, "PostToolUse").len(), 1);
        assert!(out.ends_with('\n'));
    }

    #[test]
    fn install_is_idempotent_and_keeps_unknown_keys_and_order() {
        let old = "{\n    \"model\": \"opus\",\n    \"permissions\": {\"allow\": [\"Bash(ls)\"]},\n    \"hooks\": {\n        \"Stop\": [{\"hooks\": [{\"type\": \"command\", \"command\": \"my-notifier\"}]}]\n    },\n    \"theme\": \"dark\"\n}\n";
        let once = plan_claude(old, EXE, false).unwrap();
        let v = parsed(&once);
        assert_eq!(v["model"], "opus");
        assert_eq!(v["theme"], "dark");
        assert_eq!(
            cmds(&v, "Stop"),
            vec!["my-notifier".to_string(), format!("{EXE} hook claude stop")]
        );
        let keys: Vec<&String> = v.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["model", "permissions", "hooks", "theme"]);
        assert!(
            once.contains("\n    \"model\""),
            "four-space indent is kept"
        );
        assert_eq!(plan_claude(&once, EXE, false).unwrap(), once);
    }

    #[test]
    fn legacy_npx_entries_move_to_the_absolute_path_in_place() {
        let old = r#"{"hooks":{
  "Notification":[{"hooks":[{"type":"command","command":"npx -y @tmgr/notify hook notification"}]}],
  "UserPromptSubmit":[{"hooks":[{"type":"command","command":"npx -y @tmgr/notify hook prompt"}]}],
  "Stop":[{"hooks":[{"type":"command","command":"npx -y @tmgr/notify hook stop"}]}]}}"#;
        let v = parsed(&plan_claude(old, EXE, false).unwrap());
        assert_eq!(cmds(&v, "Stop"), vec![format!("{EXE} hook claude stop")]);
        assert_eq!(
            cmds(&v, "UserPromptSubmit"),
            vec![format!("{EXE} hook claude user-prompt-submit")]
        );
        assert_eq!(
            cmds(&v, "Notification"),
            vec![format!("{EXE} hook claude notification")]
        );
    }

    #[test]
    fn a_hand_formatted_file_that_already_has_the_hooks_is_left_alone() {
        let old = format!(
            "{{\"hooks\": {{\n  \"Notification\": [{{\"hooks\": [{{\"type\": \"command\", \"command\": \"{EXE} hook claude notification\"}}]}}],\n  \"UserPromptSubmit\": [{{\"hooks\": [{{\"type\": \"command\", \"command\": \"{EXE} hook claude user-prompt-submit\"}}]}}],\n  \"Stop\": [{{\"hooks\": [{{\"type\": \"command\", \"command\": \"{EXE} hook claude stop\"}}]}}],\n  \"PostToolUse\": [{{\"matcher\": \"{RUN_TOOLS_MATCHER}\", \"hooks\": [{{\"type\": \"command\", \"command\": \"{EXE} hook claude post-tool-use\"}}]}}]\n}}}}\n"
        );
        assert_eq!(plan_claude(&old, EXE, false).unwrap(), old);
    }

    #[test]
    fn an_older_three_hook_install_gains_only_the_run_tools_hook() {
        let old = format!(
            "{{\"hooks\": {{\"Stop\": [{{\"hooks\": [{{\"type\": \"command\", \"command\": \"{EXE} hook claude stop\"}}]}}]}}}}"
        );
        let v = parsed(&plan_claude(&old, EXE, false).unwrap());
        assert_eq!(cmds(&v, "Stop").len(), 1);
        assert_eq!(
            cmds(&v, "PostToolUse"),
            vec![format!("{EXE} hook claude post-tool-use")]
        );
        assert_eq!(v["hooks"]["PostToolUse"][0]["matcher"], RUN_TOOLS_MATCHER);
    }

    #[test]
    fn the_post_tool_use_hook_is_not_duplicated_and_uninstalls_cleanly() {
        let once = plan_claude("{}", EXE, false).unwrap();
        let moved = plan_claude(&once, "/new/tmgr", false).unwrap();
        let v = parsed(&moved);
        assert_eq!(
            cmds(&v, "PostToolUse"),
            vec!["/new/tmgr hook claude post-tool-use".to_string()]
        );
        assert_eq!(plan_claude(&moved, "/new/tmgr", false).unwrap(), moved);
        let other = r#"{"hooks":{"PostToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"mine"}]}]}}"#;
        let both = plan_claude(other, EXE, false).unwrap();
        assert_eq!(cmds(&parsed(&both), "PostToolUse").len(), 2);
        let removed = plan_claude(&both, EXE, true).unwrap();
        assert_eq!(parsed(&removed), parsed(other));
        assert_eq!(parsed(&plan_claude(&once, EXE, true).unwrap()), json!({}));
    }

    #[test]
    fn a_moved_binary_replaces_the_old_path_instead_of_adding_a_second_entry() {
        let first = plan_claude("{}", "/old/tmgr", false).unwrap();
        let moved = plan_claude(&first, EXE, false).unwrap();
        let v = parsed(&moved);
        assert_eq!(cmds(&v, "Stop"), vec![format!("{EXE} hook claude stop")]);
    }

    #[test]
    fn uninstall_reverts_only_our_entries() {
        let old = "{\n  \"hooks\": {\n    \"Stop\": [{\"hooks\": [{\"type\": \"command\", \"command\": \"my-notifier\"}]}]\n  }\n}\n";
        let installed = plan_claude(old, EXE, false).unwrap();
        let removed = plan_claude(&installed, EXE, true).unwrap();
        assert_eq!(parsed(&removed), parsed(old));
        let clean = plan_claude("{\"a\": 1}\n", EXE, false).unwrap();
        assert_eq!(
            parsed(&plan_claude(&clean, EXE, true).unwrap()),
            json!({"a": 1})
        );
        assert_eq!(
            plan_claude("{\"a\": 1}\n", EXE, true).unwrap(),
            "{\"a\": 1}\n"
        );
    }

    #[test]
    fn broken_or_oddly_shaped_settings_are_refused_untouched() {
        assert!(plan_claude("{not json", EXE, false).is_err());
        assert!(plan_claude("[1]", EXE, false).is_err());
        assert!(plan_claude(r#"{"hooks": []}"#, EXE, false).is_err());
        assert!(plan_claude(r#"{"hooks": {"Stop": {}}}"#, EXE, false).is_err());
    }

    #[test]
    fn codex_install_adds_notify_and_keeps_the_rest() {
        let old = "# my config\nmodel = \"gpt\"\n\n[projects.x]\ntrust = \"trusted\"\n";
        let out = plan_codex(old, EXE, false).unwrap();
        assert!(out.contains("# my config"));
        assert!(out.contains("[projects.x]"));
        let doc: toml_edit::DocumentMut = out.parse().unwrap();
        let arr: Vec<&str> = doc["notify"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        assert_eq!(arr, [EXE, "hook", "codex", "notify"]);
        assert!(out.find("notify").unwrap() < out.find("[projects.x]").unwrap());
        assert_eq!(plan_codex(&out, EXE, false).unwrap(), out);
        assert_eq!(
            plan_codex(&out, EXE, true).unwrap().trim_end(),
            old.trim_end()
        );
    }

    #[test]
    fn codex_migrates_the_npx_program_and_refuses_a_foreign_one() {
        let legacy = "notify = [\"npx\", \"-y\", \"@tmgr/notify\", \"hook\", \"codex\"]\n";
        let out = plan_codex(legacy, EXE, false).unwrap();
        assert!(out.contains(EXE) && !out.contains("npx"));
        let foreign = "notify = [\"/usr/bin/other\"]\n";
        let err = plan_codex(foreign, EXE, false).unwrap_err();
        assert_eq!(err.exit_code(), 6);
        assert_eq!(plan_codex(foreign, EXE, true).unwrap(), foreign);
        assert!(plan_codex("notify = [", EXE, false).is_err());
    }

    #[test]
    fn codex_has_no_project_scope() {
        assert!(target_path(HookTarget::Codex, true).is_err());
    }

    #[test]
    fn snippets_name_the_binary_and_the_events() {
        let c = claude_snippet(EXE);
        for needle in [
            "Notification",
            "UserPromptSubmit",
            "Stop",
            "PostToolUse",
            "hook claude stop",
            "hook claude post-tool-use",
            RUN_TOOLS_MATCHER,
        ] {
            assert!(c.contains(needle), "{needle}");
        }
        assert_eq!(
            codex_snippet(EXE),
            format!("notify = [\"{EXE}\", \"hook\", \"codex\", \"notify\"]\n")
        );
    }
}
