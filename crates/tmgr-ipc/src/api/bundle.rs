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
