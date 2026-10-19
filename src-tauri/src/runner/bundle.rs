use tmgr_ipc::api::limits::MAX_BUNDLE;
use tmgr_ipc::api::{AuthorKind, BundleAuthor, BundleComment, BundleLaunch, RunBundle, TaskRef};

use super::task_lookup::{CommentInfo, TaskInfo};

pub const MAX_COMMENTS: usize = 500;

#[derive(Debug, Clone, PartialEq)]
pub struct Prepared {
  pub bundle: RunBundle,
  pub sha256: String,
}

fn author_id(id: Option<&str>) -> Option<String> {
  let ok = |c: char| c.is_ascii_alphanumeric() || "._:@-".contains(c);
  id.filter(|i| !i.is_empty() && i.chars().count() <= 64 && i.chars().all(ok)).map(str::to_owned)
}

fn comment(c: &CommentInfo) -> BundleComment {
  let kind = match c.author_kind.as_str() {
    "persona" | "agent" => AuthorKind::Agent,
    _ => AuthorKind::Other,
  };
  BundleComment { author: BundleAuthor { kind, id: author_id(c.author_id.as_deref()) }, at: c.created_at.clone(), body: c.message.clone() }
}

pub fn prepare(task: &TaskInfo, workspace_id: i64, task_id: i64, profile: &str, cwd: &str) -> Result<Prepared, String> {
  if task.comments.len() > MAX_COMMENTS {
    return Err(format!("The task has {} comments; the agent bundle allows at most {MAX_COMMENTS}", task.comments.len()));
  }
  let bundle = RunBundle {
    v: 1,
    task: TaskRef { workspace_id, task_id },
    title: task.title.clone(),
    body: task.description.clone(),
    author: BundleAuthor { kind: AuthorKind::Other, id: None },
    comments: task.comments.iter().map(comment).collect(),
    pages: Vec::new(),
    persona: None,
    context: Vec::new(),
    launch: BundleLaunch { profile: profile.to_owned(), model: None, cwd: cwd.to_owned(), permission_mode: None },
  };
  bundle.validate().map_err(|e| format!("The task cannot be sent to the agent: {e}"))?;
  if bundle.canonical().len() > MAX_BUNDLE {
    return Err(format!("The task and its comments are larger than {} KiB; shorten them and run again", MAX_BUNDLE >> 10));
  }
  let sha256 = bundle.sha256();
  Ok(Prepared { bundle, sha256 })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::local_workspaces::create_in;
  use crate::runner::task_lookup::{lookup_in, TEST_SCHEMA};
  use tmgr_ipc::api::{canonical_json, sha256_hex};

  fn info(comments: usize, size: usize) -> TaskInfo {
    let comment = |i: usize| CommentInfo {
      author_kind: "user".into(),
      author_id: None,
      created_at: "2026-10-13T09:30:00.000Z".into(),
      message: format!("{i}{}", "x".repeat(size)),
    };
    TaskInfo {
      title: "T".into(),
      description: "B".into(),
      comments: (0..comments).map(comment).collect(),
    }
  }

  #[test]
  fn built_from_a_local_workspace_database_with_the_crate_hash() {
    let root = tempfile::tempdir().unwrap();
    let ws = create_in(root.path(), "Personal", "now").unwrap();
    rusqlite::Connection::open(&ws.database).unwrap().execute_batch(TEST_SCHEMA).unwrap();
    let task = lookup_in(root.path(), ws.manifest.id, 7).unwrap();
    let p = prepare(&task, ws.manifest.id, 7, "codex", "/work/project").unwrap();
    let b = &p.bundle;
    assert_eq!((b.v, b.title.as_str(), b.body.as_str()), (1, "Real title", "Real body"));
    assert_eq!(b.task, TaskRef { workspace_id: ws.manifest.id, task_id: 7 });
    assert_eq!(b.author, BundleAuthor { kind: AuthorKind::Other, id: None });
    assert!(b.pages.is_empty() && b.persona.is_none());
    assert_eq!(b.launch, BundleLaunch { profile: "codex".into(), model: None, cwd: "/work/project".into(), permission_mode: None });
    let got: Vec<_> = b.comments.iter().map(|c| (c.body.as_str(), c.author.kind, c.author.id.as_deref(), c.at.as_str())).collect();
    assert_eq!(
      got,
      [
        ("first", AuthorKind::Other, Some("1"), "2026-10-13T09:30:00.000Z"),
        ("second", AuthorKind::Agent, Some("p-1"), "2026-10-13T09:31:00.000Z")
      ]
    );
    let canonical = canonical_json(&serde_json::to_value(b).unwrap());
    assert_eq!(p.sha256, sha256_hex(b.canonical().as_bytes()));
    assert_eq!(p.sha256, sha256_hex(canonical.as_bytes()));
  }

  #[test]
  fn unknown_comment_kinds_are_other_and_odd_ids_are_dropped() {
    let mut t = info(1, 0);
    t.comments[0].author_kind = "bot".into();
    t.comments[0].author_id = Some("a b".into());
    let c = &prepare(&t, -1, 1, "codex", "/w").unwrap().bundle.comments[0];
    assert_eq!((c.author.kind, c.author.id.clone()), (AuthorKind::Other, None));
  }

  #[test]
  fn user_comments_are_never_human_and_agents_stay_agents() {
    let mut t = info(3, 0);
    for (c, kind) in t.comments.iter_mut().zip(["user", "agent", "persona"]) {
      c.author_kind = kind.into();
    }
    let kinds: Vec<_> = prepare(&t, -1, 1, "codex", "/w").unwrap().bundle.comments.iter().map(|c| c.author.kind).collect();
    assert_eq!(kinds, [AuthorKind::Other, AuthorKind::Agent, AuthorKind::Agent]);
  }

  #[test]
  fn too_many_or_too_large_comments_are_refused_not_truncated() {
    assert!(prepare(&info(MAX_COMMENTS, 1), -1, 1, "codex", "/w").is_ok());
    assert!(prepare(&info(MAX_COMMENTS + 1, 1), -1, 1, "codex", "/w").unwrap_err().contains("at most 500"));
    assert!(prepare(&info(4, MAX_BUNDLE / 3), -1, 1, "codex", "/w").unwrap_err().contains("larger than 1024 KiB"));
  }

  #[test]
  fn an_unparseable_timestamp_is_refused() {
    let mut t = info(1, 0);
    t.comments[0].created_at = "yesterday".into();
    assert!(prepare(&t, -1, 1, "codex", "/w").unwrap_err().contains("RFC 3339"));
  }
}
