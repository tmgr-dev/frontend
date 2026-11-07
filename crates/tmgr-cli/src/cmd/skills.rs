use crate::cli::SkillsCommand;
use crate::ctx::Ctx;
use crate::interact;
use crate::out::Output;
use serde_json::{json, Value};
use std::io::Read;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::text::{sanitize, sanitize_line};

const MAX_INPUT: u64 = 64 * 1024;

pub fn run(ctx: &Ctx, cmd: &SkillsCommand) -> Result<Output> {
    match cmd {
        SkillsCommand::List => list(ctx),
        SkillsCommand::Show { slug } => show(ctx, slug),
        SkillsCommand::Put { slug, file } => put(ctx, slug, file),
        SkillsCommand::Delete { slug } => delete(ctx, slug),
        SkillsCommand::Attach { persona, slug } => attach(ctx, persona, slug),
        SkillsCommand::Detach { persona, slug } => detach(ctx, persona, slug),
    }
}

fn valid_slug(slug: &str) -> Result<&str> {
    let ok = (1..=60).contains(&slug.len())
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if ok {
        Ok(slug)
    } else {
        Err(Error::usage("invalid skill slug")
            .hint("a slug is 1 to 60 lowercase letters, digits or '-'; see `tmgr skills list`"))
    }
}

fn valid_persona(persona: &str) -> Result<String> {
    let p = persona.trim().to_ascii_lowercase();
    let shape = p.len() == 36
        && p.bytes().enumerate().all(|(i, b)| match i {
            8 | 13 | 18 | 23 => b == b'-',
            _ => b.is_ascii_hexdigit(),
        });
    if shape {
        Ok(p)
    } else {
        Err(Error::usage("invalid persona id")
            .hint("pass the persona's UUID, as shown on its page in the web app"))
    }
}

fn forbidden_hint(e: Error) -> Error {
    if e.kind != Kind::Forbidden {
        return e;
    }
    if e.message.starts_with("this login does not allow that") {
        Error::new(Kind::Forbidden, "this login cannot use shared skills")
            .hint("run `tmgr login` again and approve shared skills access; logins from before this version do not have it. With a custom `--scopes` list add `skills:read,skills:write`")
    } else if e.hint.is_none() {
        e.hint("on a team persona only the workspace owner can attach or detach")
    } else {
        e
    }
}

fn text(v: &Value, key: &str) -> String {
    sanitize_line(v.get(key).and_then(Value::as_str).unwrap_or(""))
        .trim()
        .to_string()
}

fn persona_names(v: &Value) -> String {
    v.get("attached_personas")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .map(|p| {
                    let name = text(p, "name");
                    if p.get("overridden").and_then(Value::as_bool) == Some(true) {
                        format!("{name} (overridden)")
                    } else {
                        name
                    }
                })
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

fn line(ctx: &Ctx, skill: &Value) -> String {
    let s = &ctx.style;
    let version = skill.get("version").and_then(Value::as_u64).unwrap_or(0);
    let personas = persona_names(skill);
    let tail = if personas.is_empty() {
        String::new()
    } else {
        format!("  {}", s.muted(&personas))
    };
    format!(
        "  {}  {}  {}{tail}",
        s.accent(&text(skill, "slug")),
        text(skill, "title"),
        s.muted(&format!("v{version}"))
    )
}

fn list(ctx: &Ctx) -> Result<Output> {
    let skills = ctx
        .cloud_client()?
        .shared_skills()
        .map_err(forbidden_hint)?;
    let items = skills.as_array().cloned().unwrap_or_default();
    let human = if items.is_empty() {
        ctx.style.muted("No shared skills.")
    } else {
        format!(
            "{}\n{}",
            ctx.style.bold("Shared skills"),
            items
                .iter()
                .map(|s| line(ctx, s))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    Ok(Output::new(json!({ "skills": items }), human))
}

fn show(ctx: &Ctx, slug: &str) -> Result<Output> {
    let slug = valid_slug(slug)?;
    let skill = ctx
        .cloud_client()?
        .shared_skill(slug)
        .map_err(forbidden_hint)?;
    let body = sanitize(skill.get("body").and_then(Value::as_str).unwrap_or(""));
    let human = if ctx.quiet {
        body
    } else {
        let when = text(&skill, "when");
        let when = if when.is_empty() {
            String::new()
        } else {
            format!("\n{}", ctx.style.muted(&format!("when: {when}")))
        };
        format!("{}{when}\n\n{body}", line(ctx, &skill).trim_start())
    };
    Ok(Output::new(json!({ "skill": skill }), human))
}

fn read_input(file: &std::path::Path) -> Result<String> {
    let mut buf = Vec::new();
    let read = if file.as_os_str() == "-" {
        std::io::stdin()
            .lock()
            .take(MAX_INPUT + 1)
            .read_to_end(&mut buf)
    } else {
        std::fs::File::open(file)
            .map_err(|_| Error::usage("cannot open the skill file"))?
            .take(MAX_INPUT + 1)
            .read_to_end(&mut buf)
    };
    read.map_err(|_| Error::usage("cannot read the skill"))?;
    if buf.len() as u64 > MAX_INPUT {
        return Err(Error::usage("the skill is larger than 64 KiB"));
    }
    let text = String::from_utf8(buf).map_err(|_| Error::usage("the skill is not valid UTF-8"))?;
    if text.trim().is_empty() {
        return Err(Error::usage("the skill is empty"));
    }
    Ok(text)
}

fn put(ctx: &Ctx, slug: &str, file: &std::path::Path) -> Result<Output> {
    let slug = valid_slug(slug)?;
    let body = read_input(file)?;
    let skill = ctx
        .client()?
        .put_shared_skill(slug, &body)
        .map_err(forbidden_hint)?;
    let human = format!(
        "{} saved {}",
        ctx.style.good("\u{2713}"),
        line(ctx, &skill).trim_start()
    );
    Ok(Output::new(json!({ "skill": skill }), human))
}

fn delete(ctx: &Ctx, slug: &str) -> Result<Output> {
    let slug = valid_slug(slug)?;
    let client = ctx.cloud_client()?;
    interact::confirm(
        &format!("Delete shared skill {slug}? It is detached from every persona."),
        true,
        ctx.yes,
    )?;
    let result = client.delete_shared_skill(slug).map_err(forbidden_hint)?;
    let detached = result
        .get("detached_personas")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let names = detached
        .iter()
        .map(|p| text(p, "name"))
        .collect::<Vec<_>>()
        .join(", ");
    let tail = if detached.is_empty() {
        String::new()
    } else {
        format!(" (detached from {names})")
    };
    Ok(Output::new(
        json!({ "slug": slug, "detached_personas": detached }),
        format!("{} deleted {slug}{tail}", ctx.style.good("\u{2713}")),
    ))
}

fn attach(ctx: &Ctx, persona: &str, slug: &str) -> Result<Output> {
    let persona = valid_persona(persona)?;
    let slug = valid_slug(slug)?;
    let skill = ctx
        .client()?
        .attach_shared_skill(&persona, slug)
        .map_err(forbidden_hint)?;
    let note = if skill.get("overridden").and_then(Value::as_bool) == Some(true) {
        " (the persona's own skill with this slug wins)"
    } else {
        ""
    };
    Ok(Output::new(
        json!({ "persona": persona, "skill": skill }),
        format!("{} attached {slug}{note}", ctx.style.good("\u{2713}")),
    ))
}

fn detach(ctx: &Ctx, persona: &str, slug: &str) -> Result<Output> {
    let persona = valid_persona(persona)?;
    let slug = valid_slug(slug)?;
    ctx.cloud_client()?
        .detach_shared_skill(&persona, slug)
        .map_err(forbidden_hint)?;
    Ok(Output::new(
        json!({ "persona": persona, "slug": slug, "detached": true }),
        format!("{} detached {slug}", ctx.style.good("\u{2713}")),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_restricted_to_a_safe_charset() {
        assert!(valid_slug("triage-2").is_ok());
        for bad in [
            "",
            "a/b",
            "../x",
            "a b",
            "a?b",
            "Triage",
            "a_b",
            &"x".repeat(61),
            "é",
        ] {
            assert!(valid_slug(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn personas_must_be_uuids() {
        let id = "0F3A9C1E-1b2c-4d5e-8f90-abcdef012345";
        assert_eq!(valid_persona(id).unwrap(), id.to_ascii_lowercase());
        for bad in [
            "",
            "Reviewer",
            "0f3a9c1e1b2c4d5e8f90abcdef012345",
            "../x/1b2c-4d5e-8f90-abcdef0123",
        ] {
            assert!(valid_persona(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn a_missing_file_is_a_usage_error() {
        let e = read_input(std::path::Path::new("/nonexistent/skill.md")).unwrap_err();
        assert_eq!(e.exit_code(), 2);
    }
}
