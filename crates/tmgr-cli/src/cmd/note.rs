use crate::cli::NoteArgs;
use crate::cmd::shared::resolve_task;
use crate::ctx::Ctx;
use crate::out::Output;
use serde_json::json;
use std::io::Write;
use std::os::fd::AsFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::pages::slugify;
use tmgr_core::text::{sanitize_line, truncate_chars};
use tmgr_core::view::task_ref;

const MAX_TITLE_CHARS: usize = 200;
const MAX_BODY_BYTES: usize = 1024 * 1024;

pub fn split_command(input: &str) -> Result<Vec<String>> {
    let bad = || Error::usage("cannot parse $VISUAL / $EDITOR: unbalanced quotes");
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut started = false;
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {
                if started {
                    out.push(std::mem::take(&mut cur));
                    started = false;
                }
            }
            '\'' => {
                started = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(ch) => cur.push(ch),
                        None => return Err(bad()),
                    }
                }
            }
            '"' => {
                started = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(e @ ('"' | '\\' | '$' | '`')) => cur.push(e),
                            Some(other) => {
                                cur.push('\\');
                                cur.push(other);
                            }
                            None => return Err(bad()),
                        },
                        Some(ch) => cur.push(ch),
                        None => return Err(bad()),
                    }
                }
            }
            '\\' => {
                started = true;
                match chars.next() {
                    Some(e) => cur.push(e),
                    None => return Err(bad()),
                }
            }
            other => {
                started = true;
                cur.push(other);
            }
        }
    }
    if started {
        out.push(cur);
    }
    Ok(out)
}

fn editor_argv() -> Result<Vec<String>> {
    for var in ["VISUAL", "EDITOR"] {
        if let Ok(v) = std::env::var(var) {
            let argv = split_command(&v)?;
            if !argv.is_empty() {
                return Ok(argv);
            }
        }
    }
    Ok(vec!["vi".to_string()])
}

fn parse_id(input: &str, what: &str) -> Result<u64> {
    (1..=12)
        .contains(&input.len())
        .then(|| input.parse::<u64>().ok())
        .flatten()
        .filter(|v| *v > 0)
        .ok_or_else(|| Error::usage(format!("invalid {what}")))
}

fn title_from(content: &str) -> Option<String> {
    let line = content.lines().map(str::trim).find(|l| !l.is_empty())?;
    let line = line.trim_start_matches('#').trim();
    let clean = sanitize_line(line);
    let clean = clean.trim();
    (!clean.is_empty()).then(|| truncate_chars(clean, MAX_TITLE_CHARS).0)
}

fn run_editor(argv: &[String], file: &Path) -> Result<()> {
    let stdout: Stdio = std::io::stderr()
        .as_fd()
        .try_clone_to_owned()
        .map(Stdio::from)
        .unwrap_or_else(|_| Stdio::inherit());
    let status = Command::new(&argv[0])
        .args(&argv[1..])
        .arg(file)
        .stdout(stdout)
        .status()
        .map_err(|e| {
            Error::usage(format!(
                "cannot start the editor {:?}: {}",
                sanitize_line(&argv[0]),
                e.kind()
            ))
            .hint("set $VISUAL or $EDITOR")
        })?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::internal(
            "the editor exited with an error; nothing was saved",
        ))
    }
}

pub fn run(ctx: &Ctx, args: &NoteArgs) -> Result<Output> {
    let given = args.title.as_deref().or(args.title_flag.as_deref());
    let title_arg = match given {
        Some(t) => {
            let clean = sanitize_line(t).trim().to_string();
            if clean.is_empty() || clean.chars().count() > MAX_TITLE_CHARS {
                return Err(Error::usage("invalid title"));
            }
            Some(clean)
        }
        None => None,
    };
    let parent = args
        .parent
        .as_deref()
        .map(|p| parse_id(p, "parent page id"))
        .transpose()?;
    let argv = editor_argv()?;
    let client = ctx.client()?;
    let ws = ctx.workspace(client)?;
    let task = args
        .task
        .as_deref()
        .map(|r| resolve_task(ctx, client, r))
        .transpose()?;

    let dir = tempfile::Builder::new()
        .prefix("tmgr-note-")
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .map_err(|_| Error::internal("cannot create a private temporary directory"))?;
    let name = format!(
        "{}.md",
        title_arg
            .as_deref()
            .map(slugify)
            .unwrap_or_else(|| "note".into())
    );
    let file = dir.path().join(name);
    let template = title_arg
        .as_ref()
        .map(|t| format!("# {t}\n\n"))
        .unwrap_or_default();
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&file)
        .and_then(|mut f| f.write_all(template.as_bytes()))
        .map_err(|_| Error::internal("cannot create the temporary note file"))?;

    let editor_name = Path::new(&argv[0])
        .file_name()
        .map(|n| sanitize_line(&n.to_string_lossy()))
        .unwrap_or_default();
    if !ctx.quiet {
        eprintln!(
            "{}",
            ctx.err_style
                .muted(&format!("opening $EDITOR ({editor_name}) ..."))
        );
    }
    run_editor(&argv, &file)?;

    let bytes = std::fs::read(&file).map_err(|_| Error::internal("cannot read the note back"))?;
    let content = String::from_utf8(bytes)
        .map_err(|_| Error::usage("the note is not valid UTF-8; nothing was saved"))?;
    if content.trim().is_empty() || content.trim() == template.trim() {
        return Err(Error::new(
            Kind::Interrupted,
            "empty note, nothing was saved",
        ));
    }
    let title = match title_arg.clone().or_else(|| title_from(&content)) {
        Some(t) => t,
        None => {
            return Err(Error::new(
                Kind::Interrupted,
                "empty note, nothing was saved",
            ))
        }
    };
    let mut body = content;
    if let Some(t) = &task {
        let reference = task_ref(t);
        body.push_str(&format!("\n\nTask: [{reference}](tmgr://task/{})\n", t.id));
    }
    let keep = |dir: tempfile::TempDir, e: Error| -> Error {
        let kept: PathBuf = dir.keep();
        let hint = format!(
            "your note is kept at {}",
            kept.join(file_name(&file)).display()
        );
        e.hint(hint)
    };
    if body.len() > MAX_BODY_BYTES {
        return Err(keep(dir, Error::usage("the note is larger than 1 MiB")));
    }
    let mut payload = json!({"title": title, "body": body});
    if let Some(p) = parent {
        payload["parent_id"] = json!(p);
    }
    let page = match client.page_create(ws.id, &payload) {
        Ok(p) => p,
        Err(e) => return Err(keep(dir, e)),
    };
    let s = &ctx.style;
    let human = format!(
        "{} saved as page {} (id {})",
        s.good("\u{2713}"),
        s.bold(&title),
        page.id
    );
    Ok(Output::new(
        json!({
            "page_id": page.id,
            "title": title,
            "version": page.version,
            "task": task.as_ref().map(task_ref),
            "parent_id": parent,
        }),
        human,
    ))
}

fn file_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_commands_split_like_a_shell_but_never_expand() {
        let cases = [
            ("vim", vec!["vim"]),
            ("code --wait", vec!["code", "--wait"]),
            ("  nvim   -u NONE ", vec!["nvim", "-u", "NONE"]),
            (
                r#""/opt/my editor/bin" --flag"#,
                vec!["/opt/my editor/bin", "--flag"],
            ),
            ("emacsclient -a ''", vec!["emacsclient", "-a", ""]),
            (r"my\ editor -x", vec!["my editor", "-x"]),
            ("ed $HOME `id` ~ *", vec!["ed", "$HOME", "`id`", "~", "*"]),
            (r#"x "a\"b" 'c\d'"#, vec!["x", "a\"b", r"c\d"]),
            ("a;b && c", vec!["a;b", "&&", "c"]),
        ];
        for (input, want) in cases {
            assert_eq!(split_command(input).unwrap(), want, "{input}");
        }
        assert!(split_command("").unwrap().is_empty());
        for bad in ["'open", "\"open", "trailing\\"] {
            assert!(split_command(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn titles_come_from_the_first_non_empty_line() {
        assert_eq!(
            title_from("\n\n# Deploy checklist\n- x").as_deref(),
            Some("Deploy checklist")
        );
        assert_eq!(
            title_from("plain first line\nmore").as_deref(),
            Some("plain first line")
        );
        assert_eq!(title_from("# \u{1b}[31mRed").as_deref(), Some("[31mRed"));
        assert_eq!(title_from("###\n").as_deref(), None);
        assert_eq!(title_from("   \n").as_deref(), None);
    }
}
