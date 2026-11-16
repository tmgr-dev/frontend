use regex::{Captures, Regex};
use std::sync::OnceLock;

struct Rules {
    pem: Regex,
    url: Regex,
    shapes: Regex,
    auth: Regex,
    bearer: Regex,
    flag: Regex,
    short_pass: Regex,
    user_flag: Regex,
    kv: Regex,
}

fn rules() -> &'static Rules {
    static R: OnceLock<Rules> = OnceLock::new();
    R.get_or_init(|| Rules {
        pem: Regex::new(
            r"(?s)-----BEGIN [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----.*?(?:-----END [A-Z0-9 ]*PRIVATE KEY(?: BLOCK)?-----|$)",
        )
        .expect("pem"),
        url: Regex::new(r"(?i)([a-z][a-z0-9+.-]*://[^\s/:@]*:)[^\s@]+@").expect("url"),
        shapes: Regex::new(concat!(
            r"\b(?:sk-[A-Za-z0-9_-]{16,}",
            r"|[sr]k_(?:live|test)_[A-Za-z0-9]{10,}",
            r"|gh[pousr]_[A-Za-z0-9]{20,}",
            r"|github_pat_[A-Za-z0-9_]{20,}",
            r"|glpat-[A-Za-z0-9_-]{15,}",
            r"|hf_[A-Za-z0-9]{20,}",
            r"|npm_[A-Za-z0-9]{20,}",
            r"|ya29\.[A-Za-z0-9_-]{20,}",
            r"|xox[abprs]-[A-Za-z0-9-]{10,}",
            r"|AKIA[0-9A-Z]{16}",
            r"|AIza[0-9A-Za-z_-]{30,}",
            r"|\d{8,10}:[A-Za-z0-9_-]{35}",
            r"|eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,})",
        ))
        .expect("shapes"),
        auth: Regex::new(
            r#"(?i)\b(authorization\s*[:=]\s*\\?["']?(?:basic|bearer|token)\s+)[^\s"'\\]+"#,
        )
        .expect("auth"),
        bearer: Regex::new(r"(?i)\b((?:bearer|basic)\s+)[A-Za-z0-9._~+/=-]{8,}").expect("bearer"),
        flag: Regex::new(
            r#"(?i)(\s--?(?:password|passwd|pass|pwd|token|secret|api-key|apikey|access-token|auth-token|client-secret)(?:\s+|=))("[^"]*"|'[^']*'|\S+)"#,
        )
        .expect("flag"),
        short_pass: Regex::new(r"(\s-p)[^\s=-]\S{2,}").expect("short_pass"),
        user_flag: Regex::new(r"(\s(?:-u|--user)(?:\s+|=)[^\s:]+:)\S+").expect("user_flag"),
        kv: Regex::new(
            r#"(?i)\b([A-Za-z0-9_.-]*(?:password|passwd|passphrase|secret|token|api[_-]?key|access[_-]?key|private[_-]?key|credential|authorization)[A-Za-z0-9_.-]*)((?:\\?["'])?\s*[:=]\s*(?:\\?["'])?)([^\s"',;&\\]{4,})"#,
        )
        .expect("kv"),
    })
}

pub fn mask(text: &str) -> String {
    let r = rules();
    let s = r.pem.replace_all(text, "[masked private key]");
    let s = r.url.replace_all(&s, "${1}***@");
    let s = r.shapes.replace_all(&s, "***");
    let s = r.auth.replace_all(&s, "${1}***");
    let s = r.bearer.replace_all(&s, "${1}***");
    let s = r.flag.replace_all(&s, "${1}***");
    let s = r.short_pass.replace_all(&s, "${1}***");
    let s = r.user_flag.replace_all(&s, "${1}***");
    r.kv.replace_all(&s, |c: &Captures| {
        if c[1].to_ascii_lowercase().ends_with("tokens") {
            c[0].to_string()
        } else {
            format!("{}{}***", &c[1], &c[2])
        }
    })
    .into_owned()
}

fn secret_key(k: &str) -> bool {
    let k = k.to_ascii_lowercase();
    !k.ends_with("tokens")
        && [
            "password",
            "passwd",
            "passphrase",
            "secret",
            "token",
            "api_key",
            "apikey",
            "api-key",
            "access_key",
            "private_key",
            "credential",
            "authorization",
        ]
        .iter()
        .any(|n| k.contains(n))
}

fn mask_value(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::String(s) => {
            let t = s.trim_start();
            if t.starts_with('{') || t.starts_with('[') {
                if let Ok(mut inner) = serde_json::from_str::<serde_json::Value>(s) {
                    mask_value(&mut inner);
                    *s = inner.to_string();
                    return;
                }
            }
            *s = mask(s);
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(mask_value),
        serde_json::Value::Object(o) => {
            for (k, val) in o.iter_mut() {
                if secret_key(k) && val.is_string() {
                    *val = serde_json::Value::String("***".into());
                } else {
                    mask_value(val);
                }
            }
        }
        _ => {}
    }
}

pub fn mask_json_line(line: &str) -> Option<String> {
    let mut v: serde_json::Value = serde_json::from_str(line).ok()?;
    mask_value(&mut v);
    Some(v.to_string())
}
