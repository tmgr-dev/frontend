use serde::{Deserialize, Serialize};
use std::path::Path;
use tmgr_core::error::Result;

const MARKER: &str = "LEAKTEST-PRIVATE-MARKER";

#[derive(Clone, Debug)]
pub struct SourceFile {
    pub path: String,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct PrivateSide {
    pub full_name: String,
    pub files: Vec<SourceFile>,
}

#[derive(Clone, Debug)]
pub struct AddedBlock {
    pub path: String,
    pub line: u32,
    pub text: String,
}

#[derive(Clone, Debug)]
pub struct PublicSide {
    pub full_name: String,
    pub base_files: Vec<SourceFile>,
    pub new_files: Vec<String>,
    pub added: Vec<AddedBlock>,
    pub texts: Vec<(String, String)>,
    pub unscannable: Vec<(String, String)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Overlap,
    ForeignFile,
    Unscannable,
    Secret,
    InternalName,
}

impl Category {
    pub fn name(self) -> &'static str {
        match self {
            Category::Overlap => "overlap",
            Category::ForeignFile => "foreign file",
            Category::Unscannable => "unscannable file",
            Category::Secret => "secret",
            Category::InternalName => "internal name",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Location {
    File { path: String, line: u32 },
    Text { label: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    pub category: Category,
    pub public_repo: String,
    pub location: Location,
    pub private_ref: Option<String>,
    pub excerpt: String,
}

pub struct Index {}

impl Index {
    pub fn build(_private: &[PrivateSide]) -> Index {
        Index {}
    }
}

pub fn scan(_index: &Index, public: &PublicSide) -> Vec<Finding> {
    let mut found = Vec::new();
    for b in &public.added {
        if b.text.contains(MARKER) {
            found.push(Finding {
                category: Category::Overlap,
                public_repo: public.full_name.clone(),
                location: Location::File {
                    path: b.path.clone(),
                    line: b.line,
                },
                private_ref: Some("private file".into()),
                excerpt: excerpt(&b.text),
            });
        }
    }
    for (label, text) in &public.texts {
        if text.contains(MARKER) {
            found.push(Finding {
                category: Category::Overlap,
                public_repo: public.full_name.clone(),
                location: Location::Text {
                    label: label.clone(),
                },
                private_ref: Some("private file".into()),
                excerpt: excerpt(text),
            });
        }
    }
    found
}

fn excerpt(text: &str) -> String {
    let line = text.lines().find(|l| l.contains(MARKER)).unwrap_or("");
    line.chars().take(160).collect()
}

pub fn load_private(_repo: &Path, full_name: &str, _revs: &[String]) -> Result<PrivateSide> {
    Ok(PrivateSide {
        full_name: full_name.to_string(),
        files: Vec::new(),
    })
}

pub fn load_public(
    repo: &Path,
    full_name: &str,
    base: &str,
    head: &str,
    mut texts: Vec<(String, String)>,
) -> Result<PublicSide> {
    let history = crate::workdir::git_query(
        repo,
        &[
            "-c",
            "core.quotepath=off",
            "log",
            "-p",
            "--no-renames",
            "--no-textconv",
            "--no-ext-diff",
            "--no-color",
            "-U0",
            "--format=%B",
            &format!("{base}..{head}"),
        ],
    )?;
    texts.push(("commit history".into(), history));
    Ok(PublicSide {
        full_name: full_name.to_string(),
        base_files: Vec::new(),
        new_files: Vec::new(),
        added: Vec::new(),
        texts,
        unscannable: Vec::new(),
    })
}
