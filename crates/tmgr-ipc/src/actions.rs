use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const CAP_ACTIONS: &str = "actions";
pub const OPT_APPROVE: &str = "approve";
pub const OPT_REJECT: &str = "reject";
pub const TITLE_MAX: usize = 255;
pub const TEXT_MAX: usize = 6000;
pub const COMMITS_MAX: usize = 50;
pub const SUBJECT_MAX: usize = 200;
pub const REF_MAX: usize = 100;
const ORIGIN_MAX: usize = 200;
const LABEL_MAX: usize = 300;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommentPayload {
    pub origin: String,
    pub user_id: u64,
    pub task_id: u64,
    pub task_label: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskPayload {
    pub origin: String,
    pub user_id: u64,
    pub workspace_id: u64,
    #[serde(default)]
    pub category_id: Option<u64>,
    #[serde(default)]
    pub status_id: Option<u64>,
    pub title: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrPayload {
    pub repo: String,
    pub branch: String,
    pub base: String,
    pub base_sha: String,
    pub head_sha: String,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub commits: Vec<Commit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TagPayload {
    pub repo: String,
    pub name: String,
    pub target_sha: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum ActionIn {
    Comment(CommentPayload),
    Task(TaskPayload),
    Pr(PrPayload),
    Tag(TagPayload),
}

pub type ActionEdit = BTreeMap<String, String>;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionDecision {
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit: Option<ActionEdit>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Approved {
    pub action: ActionIn,
    pub sha256: String,
    pub proposed_sha256: String,
    pub edited: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionResult {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ActionIn {
    pub fn kind(&self) -> &'static str {
        match self {
            ActionIn::Comment(_) => "comment",
            ActionIn::Task(_) => "task",
            ActionIn::Pr(_) => "pr",
            ActionIn::Tag(_) => "tag",
        }
    }

    pub fn editable(&self) -> &'static [&'static str] {
        match self {
            ActionIn::Comment(_) => &["text"],
            ActionIn::Task(_) => &["title", "description"],
            ActionIn::Pr(_) => &["title", "body"],
            ActionIn::Tag(_) => &["name"],
        }
    }

    pub fn sha256(&self) -> String {
        let bytes = serde_json::to_vec(self).expect("action payloads always serialize");
        hex(&Sha256::digest(bytes))
    }

    pub fn validate(&self) -> Result<(), String> {
        match self {
            ActionIn::Comment(p) => {
                origin(&p.origin)?;
                id("user_id", p.user_id)?;
                id("task_id", p.task_id)?;
                text("task_label", &p.task_label, LABEL_MAX, false)?;
                text("text", &p.text, TEXT_MAX, true)
            }
            ActionIn::Task(p) => {
                origin(&p.origin)?;
                id("user_id", p.user_id)?;
                id("workspace_id", p.workspace_id)?;
                if let Some(c) = p.category_id {
                    id("category_id", c)?;
                }
                if let Some(s) = p.status_id {
                    id("status_id", s)?;
                }
                text("title", &p.title, TITLE_MAX, true)?;
                text("description", &p.description, TEXT_MAX, false)
            }
            ActionIn::Pr(p) => {
                repo(&p.repo)?;
                ref_name("branch", &p.branch)?;
                ref_name("base", &p.base)?;
                if p.branch == p.base {
                    return Err("branch and base must differ".into());
                }
                sha("base_sha", &p.base_sha)?;
                sha("head_sha", &p.head_sha)?;
                if p.base_sha == p.head_sha {
                    return Err("the branch has no commits over the base".into());
                }
                text("title", &p.title, TITLE_MAX, true)?;
                text("body", &p.body, TEXT_MAX, false)?;
                if p.commits.len() > COMMITS_MAX {
                    return Err(format!("at most {COMMITS_MAX} commits"));
                }
                for c in &p.commits {
                    sha("commit sha", &c.sha)?;
                    text("commit subject", &c.subject, SUBJECT_MAX, false)?;
                }
                Ok(())
            }
            ActionIn::Tag(p) => {
                repo(&p.repo)?;
                ref_name("name", &p.name)?;
                sha("target_sha", &p.target_sha)
            }
        }
    }

    pub fn apply_edit(&self, edit: &ActionEdit) -> Result<ActionIn, String> {
        let mut out = self.clone();
        for (field, value) in edit {
            if !self.editable().contains(&field.as_str()) {
                return Err(format!(
                    "{field} cannot be edited on a {} action",
                    self.kind()
                ));
            }
            let slot = match (&mut out, field.as_str()) {
                (ActionIn::Comment(p), "text") => &mut p.text,
                (ActionIn::Task(p), "title") => &mut p.title,
                (ActionIn::Task(p), "description") => &mut p.description,
                (ActionIn::Pr(p), "title") => &mut p.title,
                (ActionIn::Pr(p), "body") => &mut p.body,
                (ActionIn::Tag(p), "name") => &mut p.name,
                _ => unreachable!("editable() lists every field matched here"),
            };
            slot.clone_from(value);
        }
        out.validate()?;
        Ok(out)
    }

    pub fn decide(&self, d: &ActionDecision) -> Result<Approved, String> {
        let proposed = self.sha256();
        if d.sha256 != proposed {
            return Err(
                "the action changed since you opened it; review it and answer again".into(),
            );
        }
        let action = match &d.edit {
            Some(e) if !e.is_empty() => self.apply_edit(e)?,
            _ => self.clone(),
        };
        let sha256 = action.sha256();
        let edited = sha256 != proposed;
        Ok(Approved {
            action,
            sha256,
            proposed_sha256: proposed,
            edited,
        })
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn id(field: &str, v: u64) -> Result<(), String> {
    if v == 0 {
        return Err(format!("{field} is required"));
    }
    Ok(())
}

fn text(field: &str, v: &str, max: usize, required: bool) -> Result<(), String> {
    if required && v.trim().is_empty() {
        return Err(format!("{field} is required"));
    }
    if v.len() > max {
        return Err(format!("{field} is longer than {max} bytes"));
    }
    if v.chars().any(|c| c.is_control() && c != '\n' && c != '\t') {
        return Err(format!("{field} has control characters"));
    }
    Ok(())
}

fn origin(v: &str) -> Result<(), String> {
    let rest = v
        .strip_prefix("https://")
        .or_else(|| v.strip_prefix("http://"));
    let ok = v.len() <= ORIGIN_MAX
        && rest.is_some_and(|h| {
            !h.is_empty()
                && h.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | ':' | '[' | ']'))
        });
    if ok {
        Ok(())
    } else {
        Err("origin must be a bare http(s) origin".into())
    }
}

fn sha(field: &str, v: &str) -> Result<(), String> {
    if v.len() == 40
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(())
    } else {
        Err(format!(
            "{field} must be a full 40-character lowercase commit sha"
        ))
    }
}

fn repo(v: &str) -> Result<(), String> {
    let part = |s: &str| {
        !s.is_empty()
            && s.len() <= 100
            && s != "."
            && s != ".."
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    };
    match v.split_once('/') {
        Some((owner, name)) if part(owner) && part(name) => Ok(()),
        _ => Err("repo must be owner/name".into()),
    }
}

fn ref_name(field: &str, v: &str) -> Result<(), String> {
    let ok = !v.is_empty()
        && v.len() <= REF_MAX
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'))
        && v.starts_with(|c: char| c.is_ascii_alphanumeric())
        && !v.ends_with(['/', '.'])
        && !v.ends_with(".lock")
        && !v.contains("..")
        && !v.contains("//")
        && !v.contains("/.")
        && !v.split('/').any(|s| s.ends_with(".lock"));
    if ok {
        Ok(())
    } else {
        Err(format!("{field} is not a valid git ref name"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn comment() -> ActionIn {
        ActionIn::Comment(CommentPayload {
            origin: "https://api.tmgr.dev".into(),
            user_id: 1,
            task_id: 7,
            task_label: "TM-7: fix".into(),
            text: "Shipped in 1.2.3".into(),
        })
    }

    fn pr() -> ActionIn {
        ActionIn::Pr(PrPayload {
            repo: "tmgr-dev/frontend".into(),
            branch: "tm-1012".into(),
            base: "master".into(),
            base_sha: A.into(),
            head_sha: B.into(),
            title: "feat: x".into(),
            body: String::new(),
            commits: vec![Commit {
                sha: B.into(),
                subject: "feat: x".into(),
            }],
        })
    }

    fn tag() -> ActionIn {
        ActionIn::Tag(TagPayload {
            repo: "tmgr-dev/frontend".into(),
            name: "cli-v1.0.140".into(),
            target_sha: A.into(),
        })
    }

    #[test]
    fn wire_format_is_kind_and_payload() {
        let v = serde_json::to_value(tag()).unwrap();
        assert_eq!(v["kind"], "tag");
        assert_eq!(v["payload"]["name"], "cli-v1.0.140");
        let back: ActionIn = serde_json::from_value(v).unwrap();
        assert_eq!(back, tag());
    }

    #[test]
    fn hash_is_stable_and_covers_every_field() {
        assert_eq!(comment().sha256(), comment().sha256());
        assert_eq!(comment().sha256().len(), 64);
        let mut other = comment();
        if let ActionIn::Comment(p) = &mut other {
            p.task_id = 8;
        }
        assert_ne!(comment().sha256(), other.sha256());
        let ActionIn::Tag(t) = tag() else {
            unreachable!()
        };
        let as_pr_like = ActionIn::Tag(TagPayload {
            target_sha: B.into(),
            ..t
        });
        assert_ne!(tag().sha256(), as_pr_like.sha256());
    }

    #[test]
    fn unknown_payload_fields_are_refused() {
        let r = serde_json::from_str::<ActionIn>(
            r#"{"kind":"tag","payload":{"repo":"a/b","name":"v1","target_sha":"x","force":true}}"#,
        );
        assert!(r.is_err());
    }

    #[test]
    fn validation_refuses_bad_input_instead_of_truncating() {
        assert!(comment().validate().is_ok());
        assert!(pr().validate().is_ok());
        assert!(tag().validate().is_ok());
        let mut long = comment();
        if let ActionIn::Comment(p) = &mut long {
            p.text = "x".repeat(TEXT_MAX + 1);
        }
        assert!(long.validate().is_err());
        let mut nobody = comment();
        if let ActionIn::Comment(p) = &mut nobody {
            p.user_id = 0;
        }
        assert!(nobody.validate().is_err());
        for bad in [
            "-x",
            "a..b",
            "a/.b",
            "a.lock",
            "a b",
            "a/",
            "",
            "refs/heads/../x",
        ] {
            let t = ActionIn::Tag(TagPayload {
                repo: "a/b".into(),
                name: bad.into(),
                target_sha: A.into(),
            });
            assert!(t.validate().is_err(), "{bad} must be refused");
        }
        for bad in ["a", "a/b/c", "../b", "a/..", "a /b"] {
            let t = ActionIn::Tag(TagPayload {
                repo: bad.into(),
                name: "v1".into(),
                target_sha: A.into(),
            });
            assert!(t.validate().is_err(), "{bad} must be refused");
        }
        let short = ActionIn::Tag(TagPayload {
            repo: "a/b".into(),
            name: "v1".into(),
            target_sha: "abc".into(),
        });
        assert!(short.validate().is_err());
        let upper = ActionIn::Tag(TagPayload {
            repo: "a/b".into(),
            name: "v1".into(),
            target_sha: A.to_uppercase(),
        });
        assert!(upper.validate().is_err());
        for bad in [
            "api.tmgr.dev",
            "https://api.tmgr.dev/api",
            "ftp://x",
            "https://",
        ] {
            let mut c = comment();
            if let ActionIn::Comment(p) = &mut c {
                p.origin = bad.into();
            }
            assert!(c.validate().is_err(), "{bad} must be refused");
        }
        let mut ctl = comment();
        if let ActionIn::Comment(p) = &mut ctl {
            p.text = "a\u{1b}[31m".into();
        }
        assert!(ctl.validate().is_err());
    }

    #[test]
    fn decide_binds_to_the_shown_hash() {
        let a = comment();
        let ok = a
            .decide(&ActionDecision {
                sha256: a.sha256(),
                edit: None,
            })
            .unwrap();
        assert!(!ok.edited);
        assert_eq!(ok.sha256, a.sha256());
        assert_eq!(ok.proposed_sha256, a.sha256());
        assert_eq!(ok.action, a);
        let stale = a.decide(&ActionDecision {
            sha256: "0".repeat(64),
            edit: None,
        });
        assert!(stale.is_err());
    }

    #[test]
    fn edits_only_touch_editable_fields_and_rehash() {
        let a = pr();
        let edit: ActionEdit = [("title".to_string(), "feat: better".to_string())].into();
        let ok = a
            .decide(&ActionDecision {
                sha256: a.sha256(),
                edit: Some(edit),
            })
            .unwrap();
        assert!(ok.edited);
        assert_ne!(ok.sha256, ok.proposed_sha256);
        assert_eq!(ok.sha256, ok.action.sha256());
        let ActionIn::Pr(p) = &ok.action else {
            unreachable!()
        };
        assert_eq!(p.title, "feat: better");
        assert_eq!(p.head_sha, B);
        let sneaky: ActionEdit = [("head_sha".to_string(), A.to_string())].into();
        assert!(a
            .decide(&ActionDecision {
                sha256: a.sha256(),
                edit: Some(sneaky)
            })
            .is_err());
        let empty: ActionEdit = [("title".to_string(), " ".to_string())].into();
        assert!(a
            .decide(&ActionDecision {
                sha256: a.sha256(),
                edit: Some(empty)
            })
            .is_err());
        let same: ActionEdit = [("title".to_string(), "feat: x".to_string())].into();
        assert!(
            !a.decide(&ActionDecision {
                sha256: a.sha256(),
                edit: Some(same)
            })
            .unwrap()
            .edited
        );
        let rename: ActionEdit = [("name".to_string(), "bad..name".to_string())].into();
        assert!(tag()
            .decide(&ActionDecision {
                sha256: tag().sha256(),
                edit: Some(rename)
            })
            .is_err());
    }
}
