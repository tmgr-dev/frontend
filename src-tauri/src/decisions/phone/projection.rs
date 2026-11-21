use serde::Serialize;

use super::super::diff;
use super::super::store::{agent_expected, cloud_task_of, CardView};

pub const CONTEXT_MAX: usize = 8000;
pub const CARD_MAX: usize = 16 * 1024;
pub const CARDS_MAX: usize = 30;
const MESSAGE_MAX: usize = 2000;
const CONTEXT_STEPS: [usize; 5] = [CONTEXT_MAX, 4000, 2000, 1000, 0];
const LINE_MAX: usize = 300;
const RICH_STEPS: [usize; 3] = [usize::MAX, 40, 12];

#[derive(Serialize)]
struct OptOut {
  id: String,
  label: String,
  recommended: bool,
}

#[derive(Serialize)]
struct FactOut {
  state: String,
  text: String,
  link: Option<String>,
}

#[derive(Serialize)]
struct CheckOut {
  state: String,
  text: String,
}

#[derive(Serialize)]
struct LinkOut {
  kind: String,
  url: String,
  label: Option<String>,
}

#[derive(Serialize)]
struct RepoOut {
  repo: String,
  base: Option<String>,
  head: Option<String>,
}

#[derive(Serialize)]
struct MessageOut {
  seq: u64,
  author: String,
  text: String,
  created_ms: i64,
}

#[derive(Serialize)]
struct CodeOut {
  file: String,
  start: u32,
  lines: Vec<String>,
}

#[derive(Serialize)]
struct DiffLineOut {
  kind: &'static str,
  text: String,
}

#[derive(Serialize)]
struct DiffOut {
  file: String,
  lines: Vec<DiffLineOut>,
}

#[derive(Default)]
struct Rich {
  code: Option<CodeOut>,
  code_cut: bool,
  diff: Option<DiffOut>,
  diff_cut: bool,
}

struct RichSource {
  code: Option<CodeOut>,
  code_cut: bool,
  diff: Option<DiffOut>,
  diff_cut: bool,
}

#[derive(Serialize)]
struct Projection {
  p: u32,
  kind: String,
  reply_via: String,
  agent: String,
  source_title: Option<String>,
  session: String,
  title: String,
  question: Option<String>,
  context: Option<String>,
  context_cut: bool,
  options: Vec<OptOut>,
  facts: Vec<FactOut>,
  checks: Vec<CheckOut>,
  links: Vec<LinkOut>,
  repos: Vec<RepoOut>,
  messages: Vec<MessageOut>,
  chat: bool,
  task: bool,
  multi: bool,
  notes: bool,
  code: Option<CodeOut>,
  code_cut: bool,
  diff: Option<DiffOut>,
  diff_cut: bool,
  created_ms: i64,
  rev: String,
}

pub struct Projected {
  pub json: String,
  pub digest: String,
}

pub fn eligible(view: &CardView) -> bool {
  matches!(view.state.as_str(), "open" | "deferred") && matches!(view.reply_via.as_str(), "mcp" | "cli" | "hook")
}

pub fn capabilities(view: &CardView) -> (bool, bool) {
  let answerable = eligible(view) && matches!(view.reply_via.as_str(), "mcp" | "cli");
  (answerable && view.options.len() >= 2, answerable && !view.options.is_empty())
}

fn rich_source(view: &CardView) -> RichSource {
  let mut src = RichSource { code: None, code_cut: false, diff: None, diff_cut: false };
  if let Some(c) = &view.code {
    let lines: Vec<String> = c
      .text
      .lines()
      .map(|l| {
        let (l, cut) = chars(l, LINE_MAX);
        src.code_cut |= cut;
        l
      })
      .collect();
    src.code = Some(CodeOut { file: c.file.clone(), start: c.start, lines });
  }
  let file = view.first_patch.as_deref().and_then(|p| diff::parse(p).into_iter().next());
  if let Some(f) = file.filter(|f| !f.hunks.is_empty()) {
    let mut lines = Vec::new();
    for h in &f.hunks {
      let mut push = |kind, text: &str| {
        let (text, cut) = chars(text, LINE_MAX);
        src.diff_cut |= cut;
        lines.push(DiffLineOut { kind, text });
      };
      push("hunk", &h.header);
      for l in &h.lines {
        push(l.kind, &l.text);
      }
    }
    src.diff = Some(DiffOut { file: f.path, lines });
  }
  src
}

fn rich_at(src: &RichSource, max: Option<usize>) -> Rich {
  let Some(max) = max else {
    return Rich {
      code_cut: src.code.is_some() || src.code_cut,
      diff_cut: src.diff.is_some() || src.diff_cut,
      ..Rich::default()
    };
  };
  let code = src.code.as_ref().map(|c| CodeOut {
    file: c.file.clone(),
    start: c.start,
    lines: c.lines.iter().take(max).cloned().collect(),
  });
  let diff = src.diff.as_ref().map(|d| DiffOut {
    file: d.file.clone(),
    lines: d.lines.iter().take(max).map(|l| DiffLineOut { kind: l.kind, text: l.text.clone() }).collect(),
  });
  Rich {
    code_cut: src.code_cut || src.code.as_ref().is_some_and(|c| c.lines.len() > max),
    diff_cut: src.diff_cut || src.diff.as_ref().is_some_and(|d| d.lines.len() > max),
    code,
    diff,
  }
}

fn chars(s: &str, max: usize) -> (String, bool) {
  let mut it = s.chars();
  let head: String = it.by_ref().take(max).collect();
  (head, it.next().is_some())
}

fn build(view: &CardView, session: &str, context_max: usize, skip: usize, light: bool, rich: Rich) -> Projection {
  let (multi, notes) = capabilities(view);
  let (context, context_cut) = match &view.context {
    Some(c) => {
      let (c, cut) = chars(c, context_max);
      (Some(c), cut)
    }
    None => (None, false),
  };
  let mut links = Vec::new();
  let mut repos = Vec::new();
  if !light {
    if let Some(t) = &view.links.task {
      links.push(LinkOut { kind: "task".into(), url: t.clone(), label: None });
    }
    if let Some(p) = &view.links.pr {
      links.push(LinkOut { kind: "pr".into(), url: p.clone(), label: None });
    }
    for i in &view.links.items {
      links.push(LinkOut { kind: i.kind.clone(), url: i.url.clone(), label: i.label.clone() });
    }
    for d in view.diff.iter().chain(&view.diffs) {
      repos.push(RepoOut { repo: d.repo.clone(), base: d.base.clone(), head: d.head.clone() });
    }
  }
  Projection {
    p: 3,
    kind: view.kind.clone(),
    reply_via: view.reply_via.clone(),
    agent: view.source.agent.clone(),
    source_title: view.source.title.clone(),
    session: session.to_owned(),
    title: view.title.clone(),
    question: view.question.clone(),
    context,
    context_cut,
    options: view
      .options
      .iter()
      .map(|o| OptOut { id: o.id.clone(), label: o.label.clone(), recommended: o.recommended })
      .collect(),
    facts: if light {
      Vec::new()
    } else {
      view
        .facts
        .iter()
        .map(|f| FactOut { state: f.state.clone(), text: f.text.clone(), link: f.link.clone() })
        .collect()
    },
    checks: if light {
      Vec::new()
    } else {
      view.checks.iter().map(|c| CheckOut { state: c.state.clone(), text: c.text.clone() }).collect()
    },
    links,
    repos,
    messages: view
      .messages
      .iter()
      .skip(skip)
      .map(|m| MessageOut {
        seq: m.seq,
        author: m.author.clone(),
        text: chars(&m.text, MESSAGE_MAX).0,
        created_ms: m.created_ms,
      })
      .collect(),
    chat: eligible(view) && agent_expected(&view.reply_via, &view.kind),
    task: cloud_task_of(&view.links).is_some(),
    multi,
    notes,
    code: rich.code,
    code_cut: rich.code_cut,
    diff: rich.diff,
    diff_cut: rich.diff_cut,
    created_ms: view.created_ms,
    rev: view.rev.clone(),
  }
}

pub fn project(view: &CardView, session: &str) -> Option<Projected> {
  let fits = |p: &Projection| serde_json::to_string(p).ok().filter(|j| j.len() <= CARD_MAX);
  let src = rich_source(view);
  let attempts = [false, true].into_iter().flat_map(|light| {
    CONTEXT_STEPS.into_iter().flat_map(move |ctx| (0..=view.messages.len()).map(move |skip| (light, ctx, skip)))
  });
  for (light, ctx, skip) in attempts {
    let at = |max| fits(&build(view, session, ctx, skip, light, rich_at(&src, max)));
    let Some(bare) = at(None) else { continue };
    let json = if src.code.is_none() && src.diff.is_none() {
      bare
    } else {
      RICH_STEPS.into_iter().find_map(|max| at(Some(max))).unwrap_or(bare)
    };
    let digest = tmgr_phone::digest(&json);
    return Some(Projected { json, digest });
  }
  None
}
