use std::sync::LazyLock;

use regex::Regex;

use super::matcher::normalize_path;

const SEG: &str = r"[^;&|\n]*";

static RULES: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    let r = |p: String, why| (Regex::new(&p).expect("valid danger pattern"), why);
    vec![
        r(r"\brm\b".into(), "deletes files (rm)"),
        r(r"\bsudo\b|\bdoas\b".into(), "runs with elevated rights"),
        r(
            format!(r"\bgit\b{SEG}\bpush\b{SEG}(\s-[a-zA-Z]*f\b|\s--force\S*|\s\+\S)"),
            "force-pushes with git",
        ),
        r(
            format!(r"\bgit\b{SEG}\breset\b{SEG}--hard"),
            "discards work (git reset --hard)",
        ),
        r(
            format!(r"\bgit\b{SEG}\bclean\b{SEG}(\s-[a-zA-Z]*f|\s--force)"),
            "deletes untracked files (git clean)",
        ),
        r(
            format!(r"\bgit\b{SEG}\bcheckout\b{SEG}(\s--(\s|$)|\s\.(\s|$))"),
            "discards changes (git checkout --)",
        ),
        r(r"\bchmod\b".into(), "changes permissions (chmod)"),
        r(r"\bchown\b".into(), "changes owner (chown)"),
        r(
            format!(r"\b(curl|wget)\b{SEG}\|\s*(sudo\s+)?(ba|z|da|k)?sh\b"),
            "pipes a download into a shell",
        ),
        r(
            format!(r"\b(ba|z|da|k)?sh\b{SEG}<\(\s*(curl|wget)\b"),
            "runs a download in a shell",
        ),
        r(r"\beval\b".into(), "runs generated code (eval)"),
        r(
            r"\b(ba|z|da|k)?sh\b(\s+\S+)*?\s+-[a-zA-Z]*c\b".into(),
            "runs a command string in a shell",
        ),
        r(
            format!(r"\bbase64\b{SEG}\s(-d|-D|--decode)\b"),
            "decodes and may run hidden text",
        ),
        r(
            format!(r"\b(python[0-9.]*|perl|ruby|node|php)\b{SEG}\s-[a-zA-Z]*[ce]\b"),
            "runs a code string in an interpreter",
        ),
        r(r"\bdd\b".into(), "raw disk copy (dd)"),
        r(r"\bmkfs".into(), "formats a disk (mkfs)"),
        r(r"\bshred\b".into(), "destroys files (shred)"),
        r(r"\btruncate\b".into(), "truncates files"),
        r(
            format!(r"\bfind\b{SEG}\s-delete\b"),
            "deletes files (find -delete)",
        ),
    ]
});

static REDIRECT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^\s>]?>>?\s*([^\s;&|<>()]+)").expect("valid redirect pattern"));

static SPLIT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[;&|\n]+").expect("valid split"));

const OUTSIDE: &str = "writes outside the session folder";

pub fn danger(
    tool: &str,
    text: &str,
    file_path: Option<&str>,
    cwd: Option<&str>,
) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    if tool == "Write" {
        match file_path {
            None => out.push("write target is unknown"),
            Some(p) if outside(p, cwd) => out.push(OUTSIDE),
            Some(_) => {}
        }
        return out;
    }
    if text.contains('`') {
        out.push("runs a backtick substitution");
    }
    if text.contains("$(") {
        out.push("runs a command substitution");
    }
    if hides_a_name(text) {
        out.push("uses a shell expansion that can hide the command");
    }
    let n: String = text
        .replace("\\\n", "")
        .chars()
        .filter(|c| !matches!(c, '"' | '\'' | '\\'))
        .collect();
    for (re, why) in RULES.iter() {
        if re.is_match(&n) {
            out.push(why);
        }
    }
    if writes_outside(&n, cwd) {
        out.push(OUTSIDE);
    }
    out
}

fn hides_a_name(text: &str) -> bool {
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '$'
            && chars
                .peek()
                .is_some_and(|n| !n.is_whitespace() && *n != '(')
        {
            return true;
        }
    }
    SPLIT.split(text).any(|seg| {
        seg.trim_start_matches(|c: char| c.is_whitespace() || c == '(' || c == '{')
            .split_whitespace()
            .next()
            .is_some_and(|name| name.contains(['*', '?', '[']))
    })
}

fn outside(target: &str, cwd: Option<&str>) -> bool {
    let t = target.trim();
    if t.is_empty() || matches!(t, "/dev/null" | "/dev/stdout" | "/dev/stderr") {
        return false;
    }
    if t.starts_with('~') || t.contains('$') {
        return true;
    }
    let base = cwd.unwrap_or("/_");
    let (Some(root), Some(p)) = (
        normalize_path(Some(base), "."),
        normalize_path(Some(base), t),
    ) else {
        return true;
    };
    !(p == root || p.starts_with(&format!("{}/", root.trim_end_matches('/'))))
}

fn writes_outside(n: &str, cwd: Option<&str>) -> bool {
    if REDIRECT.captures_iter(n).any(|c| outside(&c[1], cwd)) {
        return true;
    }
    SPLIT.split(n).any(|seg| {
        let toks: Vec<&str> = seg.split_whitespace().collect();
        toks.iter().enumerate().any(|(i, t)| {
            let name = t.rsplit('/').next().unwrap_or(t);
            let args: Vec<&str> = toks[i + 1..].to_vec();
            match name {
                "tee" => args
                    .iter()
                    .filter(|a| !a.starts_with('-'))
                    .any(|a| outside(a, cwd)),
                "cp" | "mv" => copy_dest(&args).is_some_and(|d| outside(d, cwd)),
                _ => false,
            }
        })
    })
}

fn copy_dest<'a>(args: &[&'a str]) -> Option<&'a str> {
    let mut plain: Vec<&str> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if let Some(d) = a.strip_prefix("--target-directory=") {
            return Some(d);
        }
        if *a == "--target-directory"
            || (a.starts_with('-') && !a.starts_with("--") && a.ends_with('t'))
        {
            return it.next().copied();
        }
        if !a.starts_with('-') {
            plain.push(a);
        }
    }
    plain.last().copied()
}
