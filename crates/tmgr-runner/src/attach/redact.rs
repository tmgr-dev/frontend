use regex::{Captures, Regex};
use serde_json::Value;
use std::sync::OnceLock;
use tmgr_ipc::chat::ToolCall;

pub(crate) const SUMMARY_MAX: usize = 200;
pub(crate) const RESULT_LINES: usize = 8;
const INPUT_CAP: usize = 4096;
const RESULT_CAP: usize = 8192;
const ARG_MAX: usize = 40;
const MAX_ARGS: usize = 3;
const RESULT_CALLS: usize = 5;
const MASK: &str = "…";

struct Rules {
    pem: Regex,
    auth: Regex,
    bearer: Regex,
    cookie: Regex,
    flag: Regex,
    short_user: Regex,
    named: Regex,
    json_named: Regex,
    header_named: Regex,
    literal: Regex,
    userinfo: Regex,
    query: Regex,
    bot: Regex,
    pass_flag: Regex,
    reminder: Regex,
    hex: Regex,
    blob: Regex,
}

fn rules() -> &'static Rules {
    static R: OnceLock<Rules> = OnceLock::new();
    R.get_or_init(|| {
        let re = |p: &str| Regex::new(p).expect("redaction pattern");
        let name = r"[A-Za-z0-9_.-]*(?:secret|token|passw(?:or)?d|passwd|pwd|key|credential|auth|session|cookie|dsn|signature|private)[A-Za-z0-9_.-]*";
        Rules {
            pem: re(r"(?s)-----BEGIN [A-Z ]*(?:PRIVATE KEY|CERTIFICATE)-----.*?(?:-----END [A-Z ]*(?:PRIVATE KEY|CERTIFICATE)-----|$)"),
            auth: re(r#"(?i)\b((?:proxy-)?authorization\s*[:=]\s*)(?:(?:bearer|basic|token|digest)\s+)?[^\s'"]+"#),
            bearer: re(r"(?i)\b(bearer\s+)[A-Za-z0-9._~+/=-]{6,}"),
            cookie: re(r#"(?i)\b((?:set-)?cookie\s*:\s*)[^\r\n'"]*"#),
            flag: re(
                r#"(?i)(\s--[a-z0-9-]*(?:password|passwd|pass|secret|token|api-?key|auth|cookie|key|credential|signature|dsn)[a-z0-9-]*(?:=|\s+))("[^"]*"|'[^']*'|\S+)"#,
            ),
            short_user: re(r#"(\s(?:-u|--user|-b)\s+)("[^"]*"|'[^']*'|\S+)"#),
            named: re(&format!(
                r#"(?i)\b({name}\s*=\s*)("[^"]*"|'[^']*'|[^\s'"&;,]+)"#
            )),
            json_named: re(
                r#"(?i)("[^"]*(?:secret|token|passw|key|credential|auth|session|cookie|dsn|signature|private)[^"]*"\s*:\s*)("(?:[^"\\]|\\.)*"|[^,}\s]+)"#,
            ),
            header_named: re(
                r#"(?i)\b([a-z0-9_-]*(?:api[_-]?key|token|secret|passw(?:or)?d)[a-z0-9_-]*\s*:\s*)[^\s'",}]+"#,
            ),
            literal: re(concat!(
                r"\b(?:ghp|gho|ghs|ghu|ghr)_[A-Za-z0-9]{8,}",
                r"|\bgithub_pat_[A-Za-z0-9_]{8,}",
                r"|\bsk-[A-Za-z0-9_-]{8,}",
                r"|\bxox[abprs]-[A-Za-z0-9-]{8,}",
                r"|\b(?:AKIA|ASIA)[A-Z0-9]{12,}",
                r"|\bAIza[0-9A-Za-z_-]{20,}",
                r"|\bglpat-[A-Za-z0-9_-]{8,}",
                r"|\bnpm_[A-Za-z0-9]{20,}",
                r"|\b[sr]k_(?:live|test)_[A-Za-z0-9]{8,}",
                r"|\bxapp-[A-Za-z0-9-]{8,}",
                r"|\bpypi-[A-Za-z0-9_-]{16,}",
                r"|\bhf_[A-Za-z0-9]{16,}",
                r"|\bSG\.[A-Za-z0-9_-]{16,}",
                r"|\bAGE-SECRET-KEY-[A-Z0-9]{8,}",
                r"|hooks\.slack\.com/services/[A-Za-z0-9/]+",
                r"|\beyJ[A-Za-z0-9_-]{4,}\.[A-Za-z0-9_-]{4,}\.?[A-Za-z0-9_-]*",
            )),
            userinfo: re(r"(\b[A-Za-z][A-Za-z0-9+.-]*://)[^\s/@'\x22]+@"),
            query: re(r"(\b[a-z][a-z0-9+.-]*://[^\s'\x22?#]*)\?[^\s'\x22#]*"),
            bot: re(r"(?i)(\bbot)[0-9]{5,}:[A-Za-z0-9_-]{20,}"),
            pass_flag: re(r#"(\s-p)(?:\s+("[^"]*"|'[^']*'|\S+)|(\S+))"#),
            reminder: re(r"(?s)<system-reminder>.*?(?:</system-reminder>|$)"),
            hex: re(r"\b[0-9a-fA-F]{32,}\b"),
            blob: re(r"[A-Za-z0-9+/=_-]{32,}"),
        }
    })
}

fn mixed(s: &str) -> bool {
    s.bytes().any(|b| b.is_ascii_lowercase())
        && s.bytes().any(|b| b.is_ascii_uppercase())
        && s.bytes().any(|b| b.is_ascii_digit())
}

fn keep_prefix(c: &Captures) -> String {
    format!("{}{MASK}", &c[1])
}

fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\u{1b}' => {
                if chars.peek() == Some(&'[') {
                    chars.next();
                    for n in chars.by_ref() {
                        if ('\u{40}'..='\u{7e}').contains(&n) {
                            break;
                        }
                    }
                }
            }
            '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{feff}' => {}
            c if c.is_control() && !matches!(c, '\n' | '\t') => {}
            c => out.push(c),
        }
    }
    out
}

pub(crate) fn mask(text: &str) -> String {
    let r = rules();
    let text = normalize(text);
    let mut s = r.pem.replace_all(&text, MASK).into_owned();
    for re in [
        &r.auth,
        &r.bearer,
        &r.cookie,
        &r.flag,
        &r.short_user,
        &r.named,
        &r.header_named,
    ] {
        s = re
            .replace_all(&s, |c: &Captures| keep_prefix(c))
            .into_owned();
    }
    s = r
        .json_named
        .replace_all(&s, |c: &Captures| format!("{}\"{MASK}\"", &c[1]))
        .into_owned();
    s = r
        .bot
        .replace_all(&s, |c: &Captures| format!("{}{MASK}", &c[1]))
        .into_owned();
    s = r.literal.replace_all(&s, MASK).into_owned();
    s = r
        .query
        .replace_all(&s, |c: &Captures| format!("{}?{MASK}", &c[1]))
        .into_owned();
    if ["sshpass", "docker login", "mysql", "mongo", "psql", "ftp"]
        .iter()
        .any(|p| s.contains(p))
    {
        s = r
            .pass_flag
            .replace_all(&s, |c: &Captures| format!("{} {MASK}", &c[1]))
            .into_owned();
    }
    s = r
        .userinfo
        .replace_all(&s, |c: &Captures| format!("{}{MASK}@", &c[1]))
        .into_owned();
    s = r
        .hex
        .replace_all(&s, |c: &Captures| {
            let m = &c[0];
            if m.len() == 40 && !m.bytes().any(|b| b.is_ascii_uppercase()) {
                m.to_string()
            } else {
                MASK.to_string()
            }
        })
        .into_owned();
    r.blob
        .replace_all(&s, |c: &Captures| {
            if mixed(&c[0]) && !c[0].starts_with('/') {
                MASK.to_string()
            } else {
                c[0].to_string()
            }
        })
        .into_owned()
}

fn cap_input(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let head = &text[..end];
    match head.rfind(char::is_whitespace) {
        Some(i) => &head[..i],
        None => "",
    }
}

fn r_strip(text: &str) -> String {
    rules().reminder.replace_all(text, "").into_owned()
}

pub(crate) fn trim_results(calls: &mut [ToolCall]) {
    let keep_from = calls.len().saturating_sub(RESULT_CALLS);
    for c in &mut calls[..keep_from] {
        c.result = None;
    }
}

const SECRET_COMMANDS: [&str; 14] = [
    "printenv",
    "env ",
    "gh auth token",
    "aws configure",
    "security find",
    "op read",
    "vault ",
    "kubectl get secret",
    ".env",
    ".netrc",
    ".npmrc",
    ".pypirc",
    "id_rsa",
    "id_ed25519",
];

pub(crate) fn result_allowed(name: &str, input: &Value) -> bool {
    match name {
        "Bash" => {
            let cmd = first_str(input, &["command"])
                .unwrap_or("")
                .to_ascii_lowercase();
            let sensitive = SECRET_COMMANDS.iter().any(|p| cmd.contains(p))
                || cmd.trim() == "env"
                || cmd.trim() == "set"
                || cmd.contains(".pem")
                || cmd.contains("kubeconfig")
                || cmd.contains("credentials");
            !sensitive
        }
        "Glob" => true,
        "Grep" => input.get("output_mode").and_then(Value::as_str) != Some("content"),
        _ => false,
    }
}

fn home_short(path: &str) -> String {
    match std::env::var("HOME") {
        Ok(h) if !h.is_empty() && (path == h || path.starts_with(&format!("{h}/"))) => {
            format!("~{}", &path[h.len()..])
        }
        _ => path.to_string(),
    }
}

fn clip_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn flat(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(crate) fn summary(text: &str) -> String {
    let masked = mask(cap_input(text, INPUT_CAP));
    clip_chars(&flat(&masked), SUMMARY_MAX)
}

pub(crate) fn excerpt(text: &str) -> Option<String> {
    let masked = mask(&r_strip(cap_input(text, RESULT_CAP)));
    let lines: Vec<String> = masked
        .lines()
        .map(|l| clip_chars(l.trim_end(), SUMMARY_MAX))
        .filter(|l| !l.trim().is_empty())
        .take(RESULT_LINES)
        .collect();
    (!lines.is_empty()).then(|| lines.join("\n"))
}

pub(crate) fn result_text(content: &Value) -> Option<String> {
    match content {
        Value::String(s) => Some(s.clone()),
        Value::Array(items) => {
            let parts: Vec<&str> = items
                .iter()
                .filter_map(|i| i.get("text").and_then(Value::as_str))
                .collect();
            (!parts.is_empty()).then(|| parts.join("\n"))
        }
        _ => None,
    }
}

fn first_str<'a>(input: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter()
        .find_map(|k| input.get(*k).and_then(Value::as_str))
        .filter(|s| !s.trim().is_empty())
}

fn arg_value(v: &Value) -> Option<String> {
    let text = match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => return None,
    };
    Some(clip_chars(&flat(cap_input(&text, 512)), ARG_MAX))
}

fn key_args(input: &Value) -> String {
    let Some(map) = input.as_object() else {
        return String::new();
    };
    map.iter()
        .filter_map(|(k, v)| arg_value(v).map(|v| format!("{k}={v}")))
        .take(MAX_ARGS)
        .collect::<Vec<_>>()
        .join(" ")
}

pub(crate) fn tool_call(name: &str, input: &Value) -> ToolCall {
    let (shown, text) = match name {
        "Bash" => (
            name.to_string(),
            first_str(input, &["command"]).map(str::to_string),
        ),
        "Read" | "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => (
            name.to_string(),
            first_str(input, &["file_path", "notebook_path"]).map(home_short),
        ),
        "Grep" | "Glob" => {
            let pattern = first_str(input, &["pattern"]).map(str::to_string);
            let text = match (pattern, first_str(input, &["path"])) {
                (Some(p), Some(dir)) => Some(format!("{p} in {dir}")),
                (p, _) => p,
            };
            (name.to_string(), text)
        }
        "WebFetch" => (
            name.to_string(),
            first_str(input, &["url"]).map(str::to_string),
        ),
        "WebSearch" => (
            name.to_string(),
            first_str(input, &["query"]).map(str::to_string),
        ),
        "Agent" | "Task" => (
            "Agent".to_string(),
            first_str(input, &["description"]).map(str::to_string),
        ),
        other => {
            let mut parts = other.splitn(3, "__");
            let shown = match (parts.next(), parts.next(), parts.next()) {
                (Some("mcp"), Some(server), Some(tool)) => format!("{server}.{tool}"),
                _ => other.to_string(),
            };
            (shown, Some(key_args(input)).filter(|a| !a.is_empty()))
        }
    };
    ToolCall {
        n: 0,
        name: clip_chars(&flat(&mask(&shown)), 60),
        summary: text.map(|t| summary(&t)).unwrap_or_default(),
        result: None,
        running: false,
        error: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const JWT: &str = "eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";
    const HEX: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
    const B64: &str = "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY12";

    #[test]
    fn secrets_are_masked() {
        let cases: Vec<(String, &str)> = vec![
            (
                "curl -H 'Authorization: Bearer abc123def456' https://x.io".into(),
                "abc123def456",
            ),
            (
                "curl -H \"authorization: Basic dXNlcjpwYXNz\" x".into(),
                "dXNlcjpwYXNz",
            ),
            ("echo Bearer abcdefghijkl".into(), "abcdefghijkl"),
            ("login --password hunter2hunter2".into(), "hunter2hunter2"),
            ("login --password=hunter2hunter2".into(), "hunter2hunter2"),
            ("mysql --pass 'my secret pw'".into(), "secret pw"),
            ("curl -u admin:s3cr3tpass https://x".into(), "s3cr3tpass"),
            ("PASSWORD=hunter2hunter2 ./run".into(), "hunter2hunter2"),
            (
                "export GITHUB_TOKEN=ghp_abcdefghijklmnop1234".into(),
                "abcdefghijklmnop1234",
            ),
            ("api_key=zzzzzzzzzzzz1 x".into(), "zzzzzzzzzzzz1"),
            (
                "curl 'https://x/api?access_token=abcdef123456&b=1'".into(),
                "abcdef123456",
            ),
            ("MY_SECRET_VALUE=\"two words\" cmd".into(), "two words"),
            (
                "AWS_SECRET_ACCESS_KEY=abcdEFGH1234 run".into(),
                "abcdEFGH1234",
            ),
            ("echo AKIAIOSFODNN7EXAMPLE".into(), "AKIAIOSFODNN7EXAMPLE"),
            (
                "git push https://gho_abcdefghijkl1234@github.com/x".into(),
                "gho_abcdefghijkl1234",
            ),
            (
                "tok github_pat_11ABCDEFG0abcdefghijkl_mnop".into(),
                "github_pat_11ABCDEFG0abcdefghijkl_mnop",
            ),
            (
                "key sk-ant-api03-abcdefghijklmnop".into(),
                "sk-ant-api03-abcdefghijklmnop",
            ),
            (
                "slack xoxb-123456789012-abcdefghij".into(),
                "xoxb-123456789012-abcdefghij",
            ),
            (format!("jwt {JWT}"), "dozjgNryP4J3jVmNHl0w5N"),
            (format!("sha {HEX}"), HEX),
            (format!("blob {B64}"), "bPxRfiCYEXAMPLEKEY12"),
            ("postgres://admin:pw0rd@db.example.com/app".into(), "pw0rd"),
            ("Cookie: session=abc123; other=1".into(), "abc123"),
            ("{\"api_key\": \"abcdef\", \"n\": 1}".into(), "abcdef"),
            ("X-Api-Key: abcdef123".into(), "abcdef123"),
            (
                "-----BEGIN RSA PRIVATE KEY-----\nMIIEabc\n-----END RSA PRIVATE KEY-----".into(),
                "MIIEabc",
            ),
            (
                "-----BEGIN PRIVATE KEY-----\nMIIEabc\nnever ends".into(),
                "MIIEabc",
            ),
            (
                "kubectl create secret generic x --from-literal=password=hunter2hunter2".into(),
                "hunter2hunter2",
            ),
            (
                "helm install --set auth.password=hunter2hunter2 x".into(),
                "hunter2hunter2",
            ),
            (
                "docker run -e PGPASSWORD=hunter2hunter2 img".into(),
                "hunter2hunter2",
            ),
            (
                "sshpass -p hunter2hunter2 ssh host".into(),
                "hunter2hunter2",
            ),
            ("mysql -u root -phunter2hunter2 db".into(), "hunter2hunter2"),
            (
                "curl 'https://s3.example.com/b/k?X-Amz-Signature=abcdef123456&X-Amz-Expires=60'"
                    .into(),
                "abcdef123456",
            ),
            (
                "curl https://hooks.slack.com/services/T000/B000/abcdefghijkl".into(),
                "abcdefghijkl",
            ),
            (
                "curl https://api.telegram.org/bot123456789:AAabcdefghijklmnopqrstuvwxyz/getMe"
                    .into(),
                "AAabcdefghijklmnopqrstuvwxyz",
            ),
            (
                "psql \"host=db password=hunter2hunter2 user=x\"".into(),
                "hunter2hunter2",
            ),
            ("echo to\u{200b}ken=hunter2hunter2".into(), "hunter2hunter2"),
            (
                "echo \u{1b}[31mtoken=hunter2hunter2".into(),
                "hunter2hunter2",
            ),
            ("SESSION_ID=abcdefghijkl run".into(), "abcdefghijkl"),
            (
                "hf_abcdefghijklmnopqrstuv".into(),
                "hf_abcdefghijklmnopqrstuv",
            ),
            (
                "cat <<EOF > .env\nDB_PASSWORD=hunter2hunter2\nEOF".into(),
                "hunter2hunter2",
            ),
        ];
        for (input, secret) in cases {
            let out = mask(&input);
            assert!(!out.contains(secret), "{input} -> {out}");
            assert!(out.contains(MASK), "{input} -> {out}");
        }
    }

    #[test]
    fn ordinary_text_survives() {
        let cases = [
            "git commit -m 'fix the parser'",
            "cargo test -p tmgr-runner --lib chat::",
            "/Volumes/OwnProjects/tmgr.dev/task-manager-ui/src/manager/store.ts",
            "src/manager/components/ActivityRow",
            "rg -n 'fn main' crates",
            "https://tmgr.dev/tmgrdev/tasks/11317",
            "find . -name '*.rs' -print",
            "mkdir -p /tmp/a/b",
            "git show 9f86d081884c7d659a2feaa0c55ad015a3bf4f1b",
            "ssh -p 22 host",
        ];
        for c in cases {
            assert_eq!(mask(c), c);
        }
    }

    #[test]
    fn a_secret_cut_by_the_input_cap_is_not_left_half_visible() {
        let text = format!(
            "{} ghp_abcdefghijklmnopqrstuvwxyz0123456789",
            "a ".repeat(INPUT_CAP / 2 - 8)
        );
        let out = summary(&text);
        assert!(!out.contains("ghp_"), "{out}");
        let secret_at_edge = format!("{}ghp_abcdefghijklmnop", "x".repeat(INPUT_CAP - 12));
        assert!(!summary(&secret_at_edge).contains("ghp_"));
    }

    #[test]
    fn a_summary_is_masked_before_it_is_clipped() {
        let text = format!(
            "{} Authorization: Bearer abcdefghijklmnopqrstuv",
            "y".repeat(SUMMARY_MAX - 22)
        );
        let out = summary(&text);
        assert!(!out.contains("abcdef"), "{out}");
        assert!(out.chars().count() <= SUMMARY_MAX);
    }

    #[test]
    fn summaries_are_one_line_and_at_most_200_chars() {
        let out = summary(&format!("echo a\n\n  echo b\t{}", "z ".repeat(300)));
        assert!(!out.contains('\n') && out.starts_with("echo a echo b"));
        assert_eq!(out.chars().count(), SUMMARY_MAX);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn an_excerpt_keeps_eight_masked_lines_and_nothing_for_empty_output() {
        let text = (0..20)
            .map(|i| format!("line {i} token=abcdefgh"))
            .collect::<Vec<_>>()
            .join("\n");
        let out = excerpt(&text).unwrap();
        assert_eq!(out.lines().count(), RESULT_LINES);
        assert!(!out.contains("abcdefgh"));
        assert_eq!(excerpt("  \n\n"), None);
        let long = excerpt(&"w".repeat(1000)).unwrap();
        assert_eq!(long.chars().count(), SUMMARY_MAX);
    }

    #[test]
    fn a_result_is_read_from_a_string_or_text_blocks() {
        assert_eq!(result_text(&json!("hi")).as_deref(), Some("hi"));
        let blocks = json!([{"type": "text", "text": "a"}, {"type": "image"}, {"type": "text", "text": "b"}]);
        assert_eq!(result_text(&blocks).as_deref(), Some("a\nb"));
        assert_eq!(result_text(&json!(null)), None);
    }

    #[test]
    fn masking_is_idempotent_and_leaves_no_long_piece_of_a_secret_after_any_clip() {
        let secrets = [
            JWT,
            B64,
            "ghp_abcdefghijklmnopqrstuvwxyz0123",
            "AKIAIOSFODNN7EXAMPLE",
            HEX,
        ];
        for secret in secrets {
            for width in 1..=secret.len() {
                let text = format!("run token={}", &secret[..width]);
                let out = summary(&text);
                assert_eq!(mask(&out), out, "{text}");
                for w in secret.as_bytes().windows(8.min(width)) {
                    let piece = std::str::from_utf8(w).unwrap();
                    assert!(
                        piece.len() < 8 || !out.contains(piece),
                        "{secret} {width} {out}"
                    );
                }
            }
        }
    }

    #[test]
    fn results_are_shown_only_for_listing_tools_and_never_for_secret_sources() {
        let yes = [
            ("Bash", json!({"command": "cargo test"})),
            ("Glob", json!({"pattern": "*.rs"})),
            ("Grep", json!({"pattern": "x"})),
        ];
        for (n, i) in yes {
            assert!(result_allowed(n, &i), "{n}");
        }
        let no = [
            ("Read", json!({"file_path": "/a"})),
            ("Edit", json!({})),
            ("Write", json!({})),
            ("WebFetch", json!({"url": "https://x.io"})),
            ("mcp__a__b", json!({})),
            ("Agent", json!({})),
            ("Grep", json!({"pattern": "x", "output_mode": "content"})),
            ("Bash", json!({"command": "cat .env"})),
            ("Bash", json!({"command": "env"})),
            ("Bash", json!({"command": "printenv | sort"})),
            ("Bash", json!({"command": "kubectl get secret x -o yaml"})),
            ("Bash", json!({"command": "gh auth token"})),
            ("Bash", json!({"command": "cat ~/.ssh/id_ed25519"})),
        ];
        for (n, i) in no {
            assert!(!result_allowed(n, &i), "{n} {i}");
        }
    }

    #[test]
    fn only_the_latest_calls_keep_their_result() {
        let mut calls: Vec<ToolCall> = (0..8)
            .map(|i| ToolCall {
                n: i,
                name: "Bash".into(),
                summary: String::new(),
                result: Some("x".into()),
                running: false,
                error: false,
            })
            .collect();
        trim_results(&mut calls);
        assert_eq!(
            calls.iter().filter(|c| c.result.is_some()).count(),
            RESULT_CALLS
        );
        assert!(calls[7].result.is_some() && calls[0].result.is_none());
    }

    #[test]
    fn a_system_reminder_is_dropped_from_a_result() {
        let out = excerpt("ok\n<system-reminder>\nsecret rule\n</system-reminder>\ndone").unwrap();
        assert_eq!(out, "ok\ndone");
    }

    #[test]
    fn calls_show_what_the_tool_does() {
        let call = |n: &str, i: Value| {
            let c = tool_call(n, &i);
            (c.name, c.summary)
        };
        let cases = [
            (
                call("Bash", json!({"command": "ls -la /tmp"})),
                ("Bash", "ls -la /tmp"),
            ),
            (
                call("Read", json!({"file_path": "/a/b.rs"})),
                ("Read", "/a/b.rs"),
            ),
            (
                call("Edit", json!({"file_path": "/a/b.rs", "old_string": "x"})),
                ("Edit", "/a/b.rs"),
            ),
            (
                call("Grep", json!({"pattern": "fn main", "path": "src"})),
                ("Grep", "fn main in src"),
            ),
            (
                call("Glob", json!({"pattern": "**/*.rs"})),
                ("Glob", "**/*.rs"),
            ),
            (
                call("WebFetch", json!({"url": "https://example.com/x"})),
                ("WebFetch", "https://example.com/x"),
            ),
            (
                call(
                    "mcp__tmgr__get_task",
                    json!({"taskId": 5, "nested": {"a": 1}}),
                ),
                ("tmgr.get_task", "taskId=5"),
            ),
            (call("Whatever", json!({})), ("Whatever", "")),
        ];
        for ((name, summary), (en, es)) in cases {
            assert_eq!((name.as_str(), summary.as_str()), (en, es));
        }
    }

    #[test]
    fn call_summaries_mask_secrets_in_every_tool() {
        let c = tool_call(
            "Bash",
            &json!({"command": "curl -H 'Authorization: Bearer abcdefgh12345' x"}),
        );
        assert!(!c.summary.contains("abcdefgh12345"));
        let c = tool_call(
            "mcp__x__login",
            &json!({"password": "hunter2hunter2", "user": "bob"}),
        );
        assert!(!c.summary.contains("hunter2hunter2") && c.summary.contains("user=bob"));
        let c = tool_call(
            "WebFetch",
            &json!({"url": "https://x.io/?token=abcdefgh12345"}),
        );
        assert!(!c.summary.contains("abcdefgh12345"));
        let c = tool_call("Read", &json!({"file_path": format!("/a/{HEX}")}));
        assert!(!c.summary.contains(HEX));
    }
}
