use crate::error::{Error, Result};

pub const DEFAULT_TEMPLATE: &str = "{key_lower}-{slug}";
const MAX_SLUG: usize = 40;
const MAX_BRANCH: usize = 100;
const PLACEHOLDERS: [&str; 5] = ["key", "key_lower", "code_lower", "seq", "slug"];
const NOT_A_TASK_KEY: [&str; 18] = [
    "release", "hotfix", "bugfix", "feature", "feat", "fix", "chore", "docs", "rc", "v", "version",
    "sprint", "week", "epoch", "step", "phase", "stage", "node",
];

pub fn slug(title: &str) -> String {
    let mut out = String::new();
    let mut pending_dash = false;
    for c in title.chars() {
        if c.is_ascii_alphanumeric() {
            if pending_dash && !out.is_empty() {
                out.push('-');
            }
            pending_dash = false;
            out.push(c.to_ascii_lowercase());
        } else {
            pending_dash = true;
        }
    }
    out.truncate(MAX_SLUG);
    out.trim_end_matches('-').to_string()
}

pub fn validate_template(template: &str) -> Result<()> {
    let bad = |why: &str| {
        Error::usage(format!("invalid branch_template: {why}")).hint(
            "example: {key_lower}-{slug}; placeholders: key, key_lower, code_lower, seq, slug",
        )
    };
    if template.is_empty() || template.len() > 80 {
        return Err(bad("empty or too long"));
    }
    let mut rest = template;
    while let Some(open) = rest.find(['{', '}']) {
        if rest.as_bytes()[open] == b'}' {
            return Err(bad("unmatched }"));
        }
        let literal = &rest[..open];
        check_literal(literal)
            .map_err(|_| bad("literal part has characters outside a-z 0-9 . _ / -"))?;
        let after = &rest[open + 1..];
        let close = after.find('}').ok_or_else(|| bad("unmatched {"))?;
        if !PLACEHOLDERS.contains(&&after[..close]) {
            return Err(bad("unknown placeholder"));
        }
        rest = &after[close + 1..];
    }
    check_literal(rest).map_err(|_| bad("literal part has characters outside a-z 0-9 . _ / -"))
}

fn check_literal(s: &str) -> std::result::Result<(), ()> {
    s.bytes()
        .all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'/' | b'-')
        })
        .then_some(())
        .ok_or(())
}

pub fn render_template(template: &str, key: &str, title_slug: &str) -> Result<String> {
    validate_template(template)?;
    let (code, seq) = key
        .split_once('-')
        .ok_or_else(|| Error::usage("the task has no usable key for a branch name"))?;
    let rendered = template
        .replace("{key_lower}", &key.to_ascii_lowercase())
        .replace("{key}", &key.to_ascii_lowercase())
        .replace("{code_lower}", &code.to_ascii_lowercase())
        .replace("{seq}", seq)
        .replace("{slug}", title_slug);
    let mut name = String::new();
    for c in rendered.chars() {
        let prev = name.chars().last();
        if (c == '-' && prev == Some('-')) || (c == '/' && prev == Some('/')) {
            continue;
        }
        name.push(c);
    }
    let name = name.trim_end_matches(['-', '/', '.']).to_string();
    validate_branch_name(&name)?;
    Ok(name)
}

pub fn validate_branch_name(name: &str) -> Result<()> {
    let bad = || {
        Error::usage("invalid branch name")
            .hint("use letters, digits, . _ / - (no leading -, no .., max 100 characters)")
    };
    if name.is_empty() || name.len() > MAX_BRANCH {
        return Err(bad());
    }
    if !name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'-'))
    {
        return Err(bad());
    }
    if !name.as_bytes()[0].is_ascii_alphanumeric() || name.contains("..") {
        return Err(bad());
    }
    for part in name.split('/') {
        if part.is_empty()
            || part.starts_with('.')
            || part.ends_with('.')
            || part.ends_with(".lock")
        {
            return Err(bad());
        }
    }
    Ok(())
}

pub fn validate_base(base: &str) -> Result<()> {
    let ok = !base.is_empty()
        && base.len() <= 200
        && !base.starts_with('-')
        && !base.contains("..")
        && base.bytes().all(|b| {
            b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'-' | b'@' | b'~' | b'^')
        });
    if ok {
        Ok(())
    } else {
        Err(Error::usage("invalid --from value").hint("use a branch, tag or commit name"))
    }
}

pub fn key_from_branch(branch: &str) -> Option<String> {
    let last = branch.rsplit('/').next()?;
    let (code, rest) = last.split_once('-')?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let after = &rest[digits.len()..];
    if !(after.is_empty() || after.starts_with('-')) || digits.is_empty() {
        return None;
    }
    if NOT_A_TASK_KEY.contains(&code) || !code.bytes().next()?.is_ascii_alphabetic() {
        return None;
    }
    let key = format!("{}-{}", code.to_ascii_uppercase(), digits);
    crate::refs::parse_key(&key).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_is_lowercase_ascii_dashes() {
        assert_eq!(
            slug("Repo prep: cargo workspace"),
            "repo-prep-cargo-workspace"
        );
        assert_eq!(slug("  --Fix  it!!  "), "fix-it");
        assert_eq!(slug("Привет мир"), "");
        assert_eq!(slug("Café au lait"), "caf-au-lait");
        assert_eq!(slug("a/b\\c\0d"), "a-b-c-d");
    }

    #[test]
    fn slug_is_bounded_and_never_ends_with_a_dash() {
        let long = "word ".repeat(30);
        let s = slug(&long);
        assert!(s.len() <= 40);
        assert!(!s.ends_with('-'));
        assert_eq!(slug(&"a".repeat(100)).len(), 40);
    }

    #[test]
    fn injection_like_titles_stay_inert() {
        assert_eq!(slug("--upload-pack=x"), "upload-pack-x");
        assert_eq!(slug("../../etc/passwd"), "etc-passwd");
        assert_eq!(slug("$(rm -rf ~)"), "rm-rf");
        assert_eq!(slug("`id` ; x"), "id-x");
        let name = render_template(DEFAULT_TEMPLATE, "TM-1", &slug("--upload-pack=x")).unwrap();
        assert_eq!(name, "tm-1-upload-pack-x");
    }

    #[test]
    fn default_template_renders_key_and_slug() {
        let s = slug("Repo prep: cargo workspace");
        assert_eq!(
            render_template(DEFAULT_TEMPLATE, "TM-509", &s).unwrap(),
            "tm-509-repo-prep-cargo-workspace"
        );
        assert_eq!(
            render_template(DEFAULT_TEMPLATE, "TM-509", "").unwrap(),
            "tm-509"
        );
        assert_eq!(
            render_template("feature/{code_lower}-{seq}-{slug}", "TM-509", "x").unwrap(),
            "feature/tm-509-x"
        );
        assert_eq!(render_template("{key}", "TM-509", "x").unwrap(), "tm-509");
    }

    #[test]
    fn templates_are_validated() {
        for ok in ["{key_lower}-{slug}", "feat/{seq}-{slug}", "{slug}"] {
            assert!(validate_template(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "{nope}",
            "{slug",
            "slug}",
            "A-{slug}",
            "a b",
            "--{slug}x$",
            "{key}{",
        ] {
            let e = validate_template(bad);
            assert_eq!(e.unwrap_err().exit_code(), 2, "{bad}");
        }
        assert!(render_template("-{slug}", "TM-1", "x").is_err());
        assert!(render_template("../{slug}", "TM-1", "x").is_err());
    }

    #[test]
    fn branch_names_follow_a_strict_grammar() {
        for ok in ["main", "tm-509-x", "feat/tm-1", "a.b_c", "release/1.2"] {
            assert!(validate_branch_name(ok).is_ok(), "{ok}");
        }
        for bad in [
            "",
            "-x",
            "--upload-pack=x",
            "../x",
            "a..b",
            "a//b",
            "a/",
            "a.",
            "x.lock",
            "a/.b",
            "a b",
            "a\nb",
            "a@{1}",
            "a~1",
            "a:b",
            "a?b",
            "ü",
            &"a".repeat(101),
        ] {
            assert!(validate_branch_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn base_refs_cannot_look_like_options() {
        assert!(validate_base("main").is_ok());
        assert!(validate_base("origin/main").is_ok());
        assert!(validate_base("HEAD~2").is_ok());
        assert!(validate_base("--output=x").is_err());
        assert!(validate_base("a..b").is_err());
        assert!(validate_base("a b").is_err());
        assert!(validate_base("").is_err());
    }

    #[test]
    fn keys_come_from_branch_names() {
        assert_eq!(
            key_from_branch("tm-509-repo-prep").as_deref(),
            Some("TM-509")
        );
        assert_eq!(key_from_branch("tm-509").as_deref(), Some("TM-509"));
        assert_eq!(key_from_branch("feat/tm-12-x").as_deref(), Some("TM-12"));
        assert_eq!(key_from_branch("ab2-7-z").as_deref(), Some("AB2-7"));
        for none in [
            "main",
            "master",
            "release-2",
            "feature-12",
            "tm-x",
            "tm-509x",
            "tm-12345678",
            "1-2",
            "HEAD",
            "",
        ] {
            assert_eq!(key_from_branch(none), None, "{none}");
        }
    }
}
