use crate::cli::PageCommand;
use crate::ctx::Ctx;
use crate::out::Output;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use tmgr_core::api::Client;
use tmgr_core::api_ext::PageUpdate;
use tmgr_core::error::{Error, Result};
use tmgr_core::models::Workspace;
use tmgr_core::pages::{
    choose_file_name, is_page_file, remote_name, render, sha256_hex, split, ApiPage, FrontMatter,
    Index, IndexEntry, PageDir, MAX_FILE_BYTES,
};
use tmgr_core::text::sanitize_line;

const MAX_PAGES: usize = 1000;

pub fn run(ctx: &Ctx, cmd: &PageCommand) -> Result<Output> {
    match cmd {
        PageCommand::Pull {
            dir, tree, force, ..
        } => pull(ctx, dir, *tree, *force),
        PageCommand::Push { files } => push(ctx, files),
        PageCommand::Status { dir } => status(ctx, dir.as_deref().unwrap_or(Path::new("."))),
    }
}

struct Item {
    file: String,
    page_id: Option<u64>,
    title: Option<String>,
    state: &'static str,
    version: Option<i64>,
}

impl Item {
    fn json(&self) -> Value {
        json!({
            "file": self.file, "page_id": self.page_id, "title": self.title,
            "state": self.state, "version": self.version,
        })
    }
}

fn label(path: &Path) -> String {
    sanitize_line(&path.display().to_string())
}

fn glyph(state: &str) -> &'static str {
    match state {
        "new" | "updated" => "\u{2193}",
        "pushed" => "\u{2191}",
        "unchanged" => "=",
        "local_changes" | "modified" => "~",
        "remote_changed" => "\u{2193}",
        "conflict" => "!",
        _ => "?",
    }
}

fn state_text(state: &str) -> String {
    match state {
        "local_changes" => "local changes kept".to_string(),
        "conflict" => "changed on both sides".to_string(),
        "remote_changed" => "changed on the server".to_string(),
        other => other.replace('_', " "),
    }
}

fn counts(items: &[Item]) -> BTreeMap<&'static str, i64> {
    let mut m = BTreeMap::new();
    for i in items {
        *m.entry(i.state).or_insert(0) += 1;
    }
    m
}

fn human(ctx: &Ctx, items: &[Item]) -> String {
    let s = &ctx.style;
    let mut lines: Vec<String> = items
        .iter()
        .map(|i| {
            let text = format!("{} {}  {}", glyph(i.state), i.file, state_text(i.state));
            match i.state {
                "conflict" => s.accent(&text),
                "unchanged" => s.muted(&text),
                _ => text,
            }
        })
        .collect();
    let summary: Vec<String> = counts(items)
        .iter()
        .map(|(k, v)| format!("{v} {}", state_text(k)))
        .collect();
    if !summary.is_empty() {
        lines.push(summary.join(", "));
    }
    lines.join("\n")
}

fn finish(
    ctx: &Ctx,
    dir: &Path,
    ws_code: &str,
    items: Vec<Item>,
    remote_copies: bool,
    conflict_hint: &str,
) -> Result<Output> {
    let text = human(ctx, &items);
    let conflicts: Vec<String> = items
        .iter()
        .filter(|i| i.state == "conflict")
        .map(|i| match remote_name(&i.file).filter(|_| remote_copies) {
            Some(remote) => format!("{} (server copy: {remote})", i.file),
            None => i.file.clone(),
        })
        .collect();
    if !conflicts.is_empty() {
        if !ctx.json {
            println!("{text}");
        }
        return Err(Error::conflict(format!(
            "{} file(s) changed on both sides: {}",
            conflicts.len(),
            conflicts.join(", ")
        ))
        .hint(conflict_hint));
    }
    let data = json!({
        "dir": label(dir),
        "workspace": sanitize_line(ws_code),
        "pages": items.iter().map(Item::json).collect::<Vec<_>>(),
        "counts": counts(&items),
    });
    Ok(Output::new(data, text))
}

fn find_workspace(ctx: &Ctx, client: &Client, code: &str) -> Result<Workspace> {
    ctx.workspaces(client)?
        .iter()
        .find(|w| w.code.eq_ignore_ascii_case(code))
        .cloned()
        .ok_or_else(|| Error::not_found(format!("workspace {} not found", sanitize_line(code))))
}

fn descendants(flat: &[ApiPage], root: u64) -> Result<Vec<&ApiPage>> {
    if !flat.iter().any(|p| p.id == root) {
        return Err(Error::not_found(format!("page {root} not found")));
    }
    let mut keep: HashSet<u64> = HashSet::from([root]);
    loop {
        let before = keep.len();
        for p in flat {
            if p.parent_id.is_some_and(|parent| keep.contains(&parent)) {
                keep.insert(p.id);
            }
        }
        if keep.len() == before {
            break;
        }
    }
    Ok(flat.iter().filter(|p| keep.contains(&p.id)).collect())
}

fn pull(ctx: &Ctx, dir: &Path, tree: Option<u64>, force: bool) -> Result<Output> {
    let client = ctx.client()?;
    let ws = ctx.workspace(client)?;
    let d = PageDir::open(dir, true)?;
    let mut index = match d.load_index()? {
        Some(i) if !i.workspace.eq_ignore_ascii_case(&ws.code) => {
            return Err(Error::usage(format!(
                "this directory holds pages of workspace {}",
                sanitize_line(&i.workspace)
            ))
            .hint("use --ws with that workspace, or another directory"))
        }
        Some(i) => i,
        None => Index::new(&ws.code),
    };
    let flat = client.pages_flat(ws.id)?;
    let mut selected: Vec<&ApiPage> = match tree {
        Some(root) => descendants(&flat, root)?,
        None => flat.iter().collect(),
    };
    selected.sort_by_key(|p| p.id);
    if selected.len() > MAX_PAGES {
        ctx.warn(
            "too_many_pages",
            format!("only the first {MAX_PAGES} pages were pulled; use --tree"),
        );
        selected.truncate(MAX_PAGES);
    }
    let mut items = Vec::new();
    for summary in selected {
        let title = sanitize_line(&summary.title);
        let existing = index.file_for(summary.id).map(str::to_string);
        let (local_modified, file_present) = match &existing {
            Some(file) => match d.read(file, MAX_FILE_BYTES)? {
                Some(bytes) => {
                    let modified = match std::str::from_utf8(&bytes).ok().map(split) {
                        Some(Ok((_, body))) => {
                            Some(sha256_hex(body.as_bytes()))
                                != index.pages.get(file).map(|e| e.body_sha256.clone())
                        }
                        _ => true,
                    };
                    (modified, true)
                }
                None => (false, false),
            },
            None => (false, false),
        };
        let same_stamp = existing
            .as_deref()
            .and_then(|f| index.pages.get(f))
            .is_some_and(|e| e.updated_at.is_some() && e.updated_at == summary.updated_at);
        if !force && file_present && same_stamp {
            items.push(Item {
                file: existing.clone().unwrap_or_default(),
                page_id: Some(summary.id),
                title: Some(title),
                state: if local_modified {
                    "local_changes"
                } else {
                    "unchanged"
                },
                version: existing
                    .as_deref()
                    .and_then(|f| index.pages.get(f))
                    .map(|e| e.version),
            });
            continue;
        }
        let page = client.page_get(ws.id, summary.id)?;
        let version = page.version.unwrap_or(0);
        let known_version = existing
            .as_deref()
            .and_then(|f| index.pages.get(f))
            .map(|e| e.version);
        let remote_changed = !file_present || known_version != Some(version);
        if !force && file_present && !remote_changed {
            items.push(Item {
                file: existing.clone().unwrap_or_default(),
                page_id: Some(page.id),
                title: Some(title),
                state: if local_modified {
                    "local_changes"
                } else {
                    "unchanged"
                },
                version: Some(version),
            });
            continue;
        }
        if !force && file_present && local_modified {
            items.push(Item {
                file: existing.clone().unwrap_or_default(),
                page_id: Some(page.id),
                title: Some(title),
                state: "conflict",
                version: Some(version),
            });
            continue;
        }
        let file = match &existing {
            Some(f) => f.clone(),
            None => {
                let taken =
                    |name: &str| index.pages.contains_key(name) || d.exists(name).unwrap_or(true);
                match choose_file_name(&page.title, page.id, &taken) {
                    Some(f) => f,
                    None => {
                        ctx.warn(
                            "page_skipped",
                            format!("no free file name for page {}", page.id),
                        );
                        continue;
                    }
                }
            }
        };
        let body = page.body.clone().unwrap_or_default();
        let fm = FrontMatter {
            page_id: page.id,
            version,
            updated_at: page.updated_at.clone(),
            workspace: ws.code.clone(),
        };
        d.write_atomic(&file, render(&fm, &body).as_bytes())?;
        index.pages.insert(
            file.clone(),
            IndexEntry {
                page_id: page.id,
                version,
                updated_at: page.updated_at.clone(),
                body_sha256: sha256_hex(body.as_bytes()),
            },
        );
        items.push(Item {
            file,
            page_id: Some(page.id),
            title: Some(title),
            state: if existing.is_some() && file_present {
                "updated"
            } else {
                "new"
            },
            version: Some(version),
        });
    }
    d.save_index(&index)?;
    finish(
        ctx,
        dir,
        &ws.code,
        items,
        false,
        "use `tmgr page pull --force` to overwrite local changes (keep a copy first)",
    )
}

fn split_target(arg: &Path) -> Result<(PathBuf, Option<String>)> {
    let meta = std::fs::symlink_metadata(arg)
        .map_err(|_| Error::not_found(format!("{} not found", label(arg))))?;
    if meta.is_dir() {
        return Ok((arg.to_path_buf(), None));
    }
    let name = arg
        .file_name()
        .and_then(|n| n.to_str())
        .map(str::to_string)
        .filter(|n| is_page_file(n))
        .ok_or_else(|| {
            Error::usage(format!("{} is not a pulled page file", label(arg)))
                .hint("only <slug>.md files written by `tmgr page pull` can be pushed")
        })?;
    let parent = match arg.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    Ok((parent, Some(name)))
}

fn push(ctx: &Ctx, files: &[PathBuf]) -> Result<Output> {
    let defaults = [PathBuf::from(".")];
    let args = if files.is_empty() {
        &defaults[..]
    } else {
        files
    };
    let mut by_dir: Vec<(PathBuf, Vec<Option<String>>)> = Vec::new();
    for arg in args {
        let (dir, name) = split_target(arg)?;
        match by_dir.iter_mut().find(|(d, _)| *d == dir) {
            Some((_, names)) => names.push(name),
            None => by_dir.push((dir, vec![name])),
        }
    }
    let client = ctx.client()?;
    let mut items = Vec::new();
    let mut first_dir: Option<(PathBuf, String)> = None;
    for (dir, names) in &by_dir {
        let d = PageDir::open(dir, false)?;
        let mut index = d.load_index()?.ok_or_else(|| {
            Error::usage(format!("{} has no pulled pages", label(dir)))
                .hint("run `tmgr page pull <dir>` first")
        })?;
        if ctx
            .ws
            .as_deref()
            .is_some_and(|w| !w.eq_ignore_ascii_case(&index.workspace))
        {
            return Err(Error::usage(format!(
                "this directory holds pages of workspace {}",
                sanitize_line(&index.workspace)
            )));
        }
        let ws = find_workspace(ctx, client, &index.workspace)?;
        first_dir.get_or_insert_with(|| (dir.clone(), ws.code.clone()));
        let mut wanted: Vec<String> = Vec::new();
        for n in names {
            match n {
                Some(name) => wanted.push(name.clone()),
                None => wanted.extend(index.pages.keys().cloned()),
            }
        }
        wanted.sort();
        wanted.dedup();
        let explicit: HashSet<&String> = names.iter().flatten().collect();
        let mut failure: Option<Error> = None;
        for file in wanted {
            let Some(entry) = index.pages.get(&file).cloned() else {
                return Err(Error::usage(format!(
                    "{} was not pulled into this directory",
                    sanitize_line(&file)
                ))
                .hint("run `tmgr page pull <dir>`"));
            };
            let bytes = match d.read(&file, MAX_FILE_BYTES)? {
                Some(b) => b,
                None if explicit.contains(&file) => {
                    return Err(Error::not_found(format!("{file} not found")))
                }
                None => {
                    items.push(Item {
                        file,
                        page_id: Some(entry.page_id),
                        title: None,
                        state: "missing",
                        version: Some(entry.version),
                    });
                    continue;
                }
            };
            let content = String::from_utf8(bytes)
                .map_err(|_| Error::usage(format!("{file} is not valid UTF-8")))?;
            let (fm, body) = split(&content)?;
            if fm.page_id != entry.page_id || !fm.workspace.eq_ignore_ascii_case(&index.workspace) {
                return Err(Error::usage(format!(
                    "the front matter of {file} does not match this directory"
                )));
            }
            if sha256_hex(body.as_bytes()) == entry.body_sha256 {
                items.push(Item {
                    file,
                    page_id: Some(fm.page_id),
                    title: None,
                    state: "unchanged",
                    version: Some(fm.version),
                });
                continue;
            }
            match client.page_update(ws.id, fm.page_id, fm.version, body) {
                Ok(PageUpdate::Updated(page)) => {
                    let version = page.version.unwrap_or(fm.version + 1);
                    let new_fm = FrontMatter {
                        page_id: fm.page_id,
                        version,
                        updated_at: page.updated_at.clone(),
                        workspace: index.workspace.clone(),
                    };
                    d.write_atomic(&file, render(&new_fm, body).as_bytes())?;
                    index.pages.insert(
                        file.clone(),
                        IndexEntry {
                            page_id: fm.page_id,
                            version,
                            updated_at: page.updated_at,
                            body_sha256: sha256_hex(body.as_bytes()),
                        },
                    );
                    items.push(Item {
                        file,
                        page_id: Some(fm.page_id),
                        title: None,
                        state: "pushed",
                        version: Some(version),
                    });
                }
                Ok(PageUpdate::Conflict(current)) => {
                    let current = match current {
                        Some(p) if p.body.is_some() => p,
                        _ => client.page_get(ws.id, fm.page_id)?,
                    };
                    if let Some(remote) = remote_name(&file) {
                        let remote_fm = FrontMatter {
                            page_id: fm.page_id,
                            version: current.version.unwrap_or(0),
                            updated_at: current.updated_at.clone(),
                            workspace: index.workspace.clone(),
                        };
                        d.write_atomic(
                            &remote,
                            render(&remote_fm, current.body.as_deref().unwrap_or("")).as_bytes(),
                        )?;
                    }
                    items.push(Item {
                        file,
                        page_id: Some(fm.page_id),
                        title: None,
                        state: "conflict",
                        version: current.version,
                    });
                }
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            }
        }
        d.save_index(&index)?;
        if let Some(e) = failure {
            return Err(e);
        }
    }
    let (dir, code) = first_dir.unwrap_or_else(|| (PathBuf::from("."), String::new()));
    finish(
        ctx,
        &dir,
        &code,
        items,
        true,
        "merge the server copy (<slug>.remote.md) into <slug>.md, set `version:` in the front matter to the remote version, delete the remote file and push again",
    )
}

fn status(ctx: &Ctx, dir: &Path) -> Result<Output> {
    let d = PageDir::open(dir, false)?;
    let index = d.load_index()?.ok_or_else(|| {
        Error::usage(format!("{} has no pulled pages", label(dir)))
            .hint("run `tmgr page pull <dir>` first")
    })?;
    let remote: Option<HashMap<u64, Option<String>>> = ctx
        .client()
        .and_then(|client| {
            let ws = find_workspace(ctx, client, &index.workspace)?;
            client.pages_flat(ws.id)
        })
        .map(|flat| flat.into_iter().map(|p| (p.id, p.updated_at)).collect())
        .map_err(|e| {
            ctx.warn(
                "remote_unavailable",
                format!("showing local changes only ({})", e.message),
            )
        })
        .ok();
    let mut items = Vec::new();
    for (file, entry) in &index.pages {
        let local = d.read(file, MAX_FILE_BYTES)?.map(|bytes| {
            std::str::from_utf8(&bytes)
                .ok()
                .and_then(|c| split(c).ok())
                .map(|(_, body)| sha256_hex(body.as_bytes()) != entry.body_sha256)
                .unwrap_or(true)
        });
        let remote_state = remote.as_ref().map(|m| m.get(&entry.page_id));
        let remote_changed =
            matches!(remote_state, Some(Some(stamp)) if *stamp != entry.updated_at);
        let remote_gone = matches!(remote_state, Some(None));
        let state = match (local, remote_changed, remote_gone) {
            (None, _, _) => "missing",
            (_, _, true) => "deleted_remote",
            (Some(true), true, _) => "conflict",
            (Some(true), false, _) => "modified",
            (Some(false), true, _) => "remote_changed",
            (Some(false), false, _) => "unchanged",
        };
        items.push(Item {
            file: file.clone(),
            page_id: Some(entry.page_id),
            title: None,
            state,
            version: Some(entry.version),
        });
    }
    let text = if items.is_empty() {
        ctx.style.muted("No pulled pages here.")
    } else {
        human(ctx, &items)
    };
    Ok(Output::new(
        json!({
            "dir": label(dir),
            "workspace": sanitize_line(&index.workspace),
            "files": items.iter().map(Item::json).collect::<Vec<_>>(),
        }),
        text,
    ))
}
