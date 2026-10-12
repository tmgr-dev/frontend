use crate::cli::AddArgs;
use crate::ctx::Ctx;
use crate::out::Output;
use serde_json::{json, Map, Value};
use std::io::Read;
use tmgr_core::error::{Error, Result};
use tmgr_core::refs::is_category_code;
use tmgr_core::text::sanitize_line;
use tmgr_core::view::{display_title, task_ref, task_url, StatusView};

const MAX_INPUT: u64 = 64 * 1024;
const MAX_TITLE_CHARS: usize = 255;

pub fn run(ctx: &Ctx, args: &AddArgs) -> Result<Output> {
    let (title, description) = read_input(args)?;
    let project = args.project.as_deref().map(str::to_ascii_uppercase);
    if let Some(p) = &project {
        if !is_category_code(p) {
            return Err(Error::usage("invalid project code"));
        }
    }
    let client = ctx.client()?;
    let ws = ctx.workspace(client)?;
    let category_id = match &project {
        Some(code) => Some(
            client
                .categories(ws.id)?
                .into_iter()
                .find(|c| {
                    c.code
                        .as_deref()
                        .is_some_and(|x| x.eq_ignore_ascii_case(code))
                })
                .map(|c| c.id)
                .ok_or_else(|| Error::not_found(format!("project {code} not found")))?,
        ),
        None => None,
    };
    let statuses = client.statuses(ws.id)?;
    let status = match &args.status {
        Some(name) => statuses
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(name.trim()))
            .ok_or_else(|| {
                Error::not_found(format!("status {} not found", sanitize_line(name)))
                    .hint("names come from the workspace's statuses")
            })?,
        None => statuses
            .iter()
            .find(|s| s.kind == "default")
            .ok_or_else(|| {
                Error::usage("the workspace has no default status").hint("pass one with -s NAME")
            })?,
    };
    let mut body = Map::new();
    body.insert("title".into(), json!(title));
    if let Some(d) = description {
        body.insert("description".into(), json!(d));
    }
    body.insert("status_id".into(), json!(status.id));
    body.insert("workspace_id".into(), json!(ws.id));
    if let Some(c) = category_id {
        body.insert("project_category_id".into(), json!(c));
    }
    let task = client.create_task(&Value::Object(body))?;
    let reference = task_ref(&task);
    let url = task_url(&ctx.web_base()?, &sanitize_line(&ws.code), task.id);
    let view = StatusView::resolve(&task, &statuses);
    let shown_title = display_title(&task.title);
    let human = if ctx.quiet {
        reference.clone()
    } else {
        format!(
            "{} created {} {}\n{}",
            ctx.style.good("\u{2713}"),
            ctx.style.accent(&reference),
            shown_title,
            ctx.style.muted(&format!("{url} \u{b7} {}", view.name))
        )
    };
    Ok(Output::new(
        json!({"id": task.id, "ref": reference, "url": url, "title": shown_title, "status": view}),
        human,
    ))
}

fn read_input(args: &AddArgs) -> Result<(String, Option<String>)> {
    if args.title == "-" {
        if args.desc_file.is_some() {
            return Err(Error::usage(
                "--desc-file cannot be combined with reading from stdin",
            ));
        }
        let text = read_bounded(std::io::stdin().lock(), "stdin")?;
        let mut parts = text.splitn(2, '\n');
        let title = clean_title(parts.next().unwrap_or(""))?;
        let rest = parts.next().unwrap_or("").trim_matches('\n').to_string();
        return Ok((title, (!rest.trim().is_empty()).then_some(rest)));
    }
    let title = clean_title(&args.title)?;
    let description = match &args.desc_file {
        Some(path) => {
            let file = std::fs::File::open(path)
                .map_err(|_| Error::usage("cannot open the --desc-file"))?;
            let text = read_bounded(file, "the description file")?;
            (!text.trim().is_empty()).then_some(text)
        }
        None => None,
    };
    Ok((title, description))
}

fn clean_title(raw: &str) -> Result<String> {
    let title = sanitize_line(raw.trim_end_matches('\r')).trim().to_string();
    if title.is_empty() {
        return Err(Error::usage("the task title is empty"));
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(Error::usage(format!(
            "the title is longer than {MAX_TITLE_CHARS} characters"
        )));
    }
    Ok(title)
}

fn read_bounded(reader: impl Read, what: &str) -> Result<String> {
    let mut buf = Vec::new();
    reader
        .take(MAX_INPUT + 1)
        .read_to_end(&mut buf)
        .map_err(|_| Error::usage(format!("cannot read {what}")))?;
    if buf.len() as u64 > MAX_INPUT {
        return Err(Error::usage(format!("{what} is larger than 64 KiB")));
    }
    String::from_utf8(buf).map_err(|_| Error::usage(format!("{what} is not valid UTF-8")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_are_trimmed_bounded_and_single_line() {
        assert_eq!(clean_title("  Fix it \r").unwrap(), "Fix it");
        assert!(clean_title("   ").is_err());
        assert!(clean_title(&"x".repeat(256)).is_err());
        assert_eq!(clean_title("a\u{1b}b").unwrap(), "a b");
    }

    #[test]
    fn input_is_bounded_to_64_kib() {
        let ok = vec![b'a'; 64 * 1024];
        assert!(read_bounded(&ok[..], "stdin").is_ok());
        let big = vec![b'a'; 64 * 1024 + 1];
        assert!(read_bounded(&big[..], "stdin").is_err());
        assert!(read_bounded(&[0xff, 0xfe][..], "stdin").is_err());
    }
}
