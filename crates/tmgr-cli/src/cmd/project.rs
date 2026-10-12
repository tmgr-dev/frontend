use crate::cli::ProjectCommand;
use crate::ctx::Ctx;
use crate::gitx::Git;
use crate::out::{table, Output};
use serde_json::json;
use tmgr_core::config::Config;
use tmgr_core::error::{Error, Result};
use tmgr_core::refs::is_category_code;
use tmgr_core::text::sanitize_line;

fn code_of(raw: &str) -> Result<String> {
    let code = raw.to_ascii_uppercase();
    if is_category_code(&code) {
        Ok(code)
    } else {
        Err(Error::usage("invalid project code").hint("example: TM"))
    }
}

pub fn run(ctx: &Ctx, cmd: &ProjectCommand) -> Result<Output> {
    match cmd {
        ProjectCommand::Map { code, path } => {
            let code = code_of(code)?;
            let abs =
                std::fs::canonicalize(path).map_err(|_| Error::usage("the path does not exist"))?;
            if !abs.is_dir() {
                return Err(Error::usage("the path is not a directory"));
            }
            let stored = abs
                .to_str()
                .ok_or_else(|| Error::usage("the path is not valid UTF-8"))?
                .to_string();
            if Git::at(&abs).toplevel().is_none() {
                ctx.warn(
                    "not_a_git_repo",
                    "the directory is not inside a git repository",
                );
            }
            let mut cfg = Config::load(&ctx.dir)?;
            cfg.projects.insert(code.clone(), stored.clone());
            cfg.save(&ctx.dir)?;
            Ok(Output::new(
                json!({"code": code, "path": stored}),
                format!("{code} \u{2192} {}", sanitize_line(&stored)),
            ))
        }
        ProjectCommand::Unmap { code } => {
            let code = code_of(code)?;
            let mut cfg = Config::load(&ctx.dir)?;
            let removed = cfg.projects.remove(&code).is_some();
            if removed {
                cfg.save(&ctx.dir)?;
            }
            let human = if removed {
                format!("{code} unmapped")
            } else {
                ctx.style.muted(&format!("{code} was not mapped"))
            };
            Ok(Output::new(
                json!({"code": code, "removed": removed}),
                human,
            ))
        }
        ProjectCommand::List => {
            let entries: Vec<_> = ctx
                .cfg
                .projects
                .iter()
                .map(|(code, path)| {
                    let exists = std::path::Path::new(path).is_dir();
                    (sanitize_line(code), sanitize_line(path), exists)
                })
                .collect();
            let human = if entries.is_empty() {
                ctx.style
                    .muted("No projects mapped. Map one: tmgr project map TM ~/src/repo")
            } else {
                let rows: Vec<Vec<String>> = entries
                    .iter()
                    .map(|(c, p, ok)| {
                        vec![
                            c.clone(),
                            if *ok {
                                p.clone()
                            } else {
                                format!("{p} (missing)")
                            },
                        ]
                    })
                    .collect();
                table(&rows)
            };
            let list: Vec<_> = entries
                .iter()
                .map(|(c, p, ok)| json!({"code": c, "path": p, "exists": ok}))
                .collect();
            Ok(Output::new(json!({ "projects": list }), human))
        }
    }
}
