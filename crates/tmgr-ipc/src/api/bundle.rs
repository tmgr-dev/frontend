use super::types::TaskRef;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt::Write;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorKind {
    Human,
    Agent,
    Other,
}

impl AuthorKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AuthorKind::Human => "human",
            AuthorKind::Agent => "agent",
            AuthorKind::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleAuthor {
    pub kind: AuthorKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleComment {
    pub author: BundleAuthor,
    pub at: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundlePage {
    pub id: i64,
    pub revision: i64,
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundlePersona {
    pub name: String,
    pub instructions_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleLaunch {
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
}

/// The immutable input of one agent run (see `PROTOCOL.md`, "Run bundles").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunBundle {
    pub v: u32,
    pub task: TaskRef,
    pub title: String,
    pub body: String,
    pub author: BundleAuthor,
    pub comments: Vec<BundleComment>,
    pub pages: Vec<BundlePage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub persona: Option<BundlePersona>,
    pub launch: BundleLaunch,
}

impl RunBundle {
    pub fn canonical(&self) -> String {
        canonical_json(&serde_json::to_value(self).expect("bundle serializes"))
    }

    pub fn sha256(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }

    pub fn validate(&self) -> Result<(), String> {
        if let Some(m) = &self.launch.permission_mode {
            if !valid_permission_mode(m) {
                return Err("launch.permission_mode must be ask, accept-edits or plan".into());
            }
        }
        valid_author("author.id", &self.author)?;
        for c in &self.comments {
            valid_author("comments[].author.id", &c.author)?;
            if !valid_rfc3339(&c.at) {
                return Err("comments[].at must be an RFC 3339 timestamp".into());
            }
        }
        if let Some(p) = &self.persona {
            let n = p.name.chars().count();
            if !(1..=64).contains(&n) || p.name.chars().any(char::is_control) {
                return Err(
                    "persona.name must be 1 to 64 characters without control characters".into(),
                );
            }
            let hex = |c: u8| c.is_ascii_digit() || (b'a'..=b'f').contains(&c);
            if p.instructions_sha256.len() != 64 || !p.instructions_sha256.bytes().all(hex) {
                return Err(
                    "persona.instructions_sha256 must be 64 lowercase hex characters".into(),
                );
            }
        }
        Ok(())
    }
}

pub fn valid_permission_mode(m: &str) -> bool {
    matches!(m, "ask" | "accept-edits" | "plan")
}

fn valid_author(field: &str, a: &BundleAuthor) -> Result<(), String> {
    let ok = |c: char| c.is_ascii_alphanumeric() || "._:@-".contains(c);
    match &a.id {
        Some(id) if id.chars().count() > 64 || !id.chars().all(ok) => Err(format!(
            "{field} must be at most 64 characters of A-Za-z0-9._:@-"
        )),
        _ => Ok(()),
    }
}

fn valid_rfc3339(s: &str) -> bool {
    let b = s.as_bytes();
    let digits =
        |r: std::ops::Range<usize>| b.get(r).is_some_and(|d| d.iter().all(u8::is_ascii_digit));
    let num = |r: std::ops::Range<usize>| s[r].parse::<u32>().unwrap_or(u32::MAX);
    let shape = digits(0..4)
        && digits(5..7)
        && digits(8..10)
        && digits(11..13)
        && digits(14..16)
        && digits(17..19);
    if !(s.is_ascii() && shape) || [b[4], b[7], b[10], b[13], b[16]] != *b"--T::" {
        return false;
    }
    if !(1..=12).contains(&num(5..7))
        || !(1..=31).contains(&num(8..10))
        || num(11..13) > 23
        || num(14..16) > 59
        || num(17..19) > 60
    {
        return false;
    }
    let mut i = 19;
    if b.get(i) == Some(&b'.') {
        let start = i + 1;
        i = start;
        while b.get(i).is_some_and(u8::is_ascii_digit) {
            i += 1;
        }
        if !(1..=9).contains(&(i - start)) {
            return false;
        }
    }
    match &b[i..] {
        b"Z" => true,
        [b'+' | b'-', ..] => {
            b.len() == i + 6
                && digits(i + 1..i + 3)
                && b[i + 3] == b':'
                && digits(i + 4..i + 6)
                && num(i + 1..i + 3) <= 23
                && num(i + 4..i + 6) <= 59
        }
        _ => false,
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

/// RFC 8785 (JCS) for the value kinds the contract uses: no floats.
pub fn canonical_json(v: &Value) -> String {
    let mut out = String::new();
    write_value(v, &mut out);
    out
}

fn write_value(v: &Value, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => {
            let _ = write!(out, "{n}");
        }
        Value::String(s) => write_string(s, out),
        Value::Array(a) => {
            out.push('[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(x, out);
            }
            out.push(']');
        }
        Value::Object(m) => {
            let mut keys: Vec<&String> = m.keys().collect();
            keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
            out.push('{');
            for (i, k) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_string(k, out);
                out.push(':');
                write_value(&m[k], out);
            }
            out.push('}');
        }
    }
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}
