use regex::Regex;
use std::sync::OnceLock;

struct Rules {
    pem: Regex,
    url: Regex,
    shapes: Regex,
    bearer: Regex,
    kv: Regex,
}

fn rules() -> &'static Rules {
    static R: OnceLock<Rules> = OnceLock::new();
    R.get_or_init(|| Rules {
        pem: Regex::new(
            r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?(?:-----END [A-Z ]*PRIVATE KEY-----|$)",
        )
        .expect("pem"),
        url: Regex::new(r"(?i)([a-z][a-z0-9+.-]*://[^\s/:@]+:)[^\s/@]+@").expect("url"),
        shapes: Regex::new(
            r"\b(?:sk-[A-Za-z0-9_-]{16,}|gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|xox[abprs]-[A-Za-z0-9-]{10,}|AKIA[0-9A-Z]{16}|AIza[0-9A-Za-z_-]{30,}|eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,})",
        )
        .expect("shapes"),
        bearer: Regex::new(r"(?i)\b(bearer\s+)[A-Za-z0-9._~+/=-]{8,}").expect("bearer"),
        kv: Regex::new(
            r#"(?i)\b([A-Za-z0-9_.-]*(?:password|passwd|secret|token|api[_-]?key|access[_-]?key|private[_-]?key|credential|authorization)[A-Za-z0-9_.-]*)(["']?\s*[:=]\s*["']?)([^\s"',;&]{4,})"#,
        )
        .expect("kv"),
    })
}

pub fn mask(text: &str) -> String {
    let r = rules();
    let s = r.pem.replace_all(text, "[masked private key]");
    let s = r.url.replace_all(&s, "${1}***@");
    let s = r.shapes.replace_all(&s, "***");
    let s = r.bearer.replace_all(&s, "${1}***");
    r.kv.replace_all(&s, "${1}${2}***").into_owned()
}
