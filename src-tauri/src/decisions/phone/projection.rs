use serde::Serialize;
use tmgr_ipc::decisions::CardIn;

use super::super::store::CardView;

pub const CONTEXT_MAX: usize = 8000;
pub const CARD_MAX: usize = 16 * 1024;
pub const CARDS_MAX: usize = 30;
const MESSAGE_MAX: usize = 2000;
const CONTEXT_STEPS: [usize; 5] = [CONTEXT_MAX, 4000, 2000, 1000, 0];

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
  author: String,
  text: String,
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
  created_ms: i64,
  rev: String,
}

pub struct Projected {
  pub json: String,
  pub digest: String,
}

pub fn delivery_target(card: &CardIn) -> String {
  card.source.runner_session.as_ref().map(|s| format!("runner:{s}")).unwrap_or_default()
}

pub fn session_of(view: &CardView) -> String {
  delivery_target(&CardIn { source: view.source.clone(), ..Default::default() })
}

pub fn eligible(view: &CardView) -> bool {
  matches!(view.state.as_str(), "open" | "deferred") && matches!(view.reply_via.as_str(), "mcp" | "cli" | "hook")
}

fn chars(s: &str, max: usize) -> (String, bool) {
  let mut it = s.chars();
  let head: String = it.by_ref().take(max).collect();
  (head, it.next().is_some())
}

fn build(view: &CardView, session: &str, context_max: usize, skip: usize, light: bool) -> Projection {
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
    p: 1,
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
      .map(|m| MessageOut { author: m.author.clone(), text: chars(&m.text, MESSAGE_MAX).0 })
      .collect(),
    created_ms: view.created_ms,
    rev: view.rev.clone(),
  }
}

pub fn project(view: &CardView, session: &str) -> Option<Projected> {
  let fits = |p: &Projection| serde_json::to_string(p).ok().filter(|j| j.len() <= CARD_MAX);
  let attempts = [false, true].into_iter().flat_map(|light| {
    CONTEXT_STEPS.into_iter().flat_map(move |ctx| (0..=view.messages.len()).map(move |skip| (light, ctx, skip)))
  });
  for (light, ctx, skip) in attempts {
    if let Some(json) = fits(&build(view, session, ctx, skip, light)) {
      let digest = tmgr_phone::digest(&json);
      return Some(Projected { json, digest });
    }
  }
  None
}
