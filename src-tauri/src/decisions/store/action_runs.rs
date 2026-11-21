use super::*;
use serde_json::Value;
use tmgr_ipc::actions::{ActionDecision, ActionIn, ActionResult, Approved, OPT_APPROVE, OPT_REJECT};

use super::super::actions::{STATE_DONE, STATE_FAILED, STATE_PARTIAL, STATE_UNKNOWN};

pub const INTERRUPTED: &str =
  "interrupted: the desktop stopped while running it; outcome unknown, check before proposing again";
const ONLY_DECIDE: &str = "an action card can only be approved or rejected";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PendingAction {
  pub run_id: String,
  pub card_id: String,
  pub action: ActionIn,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RunView {
  pub run_id: String,
  pub state: String,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub result: Option<ActionResult>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ActionView {
  pub kind: String,
  pub payload: Value,
  pub sha256: String,
  pub editable: Vec<String>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub run: Option<RunView>,
  #[serde(skip_serializing_if = "Option::is_none")]
  pub approved: Option<Approved>,
}

pub struct Running {
  pub run_id: String,
  pub action: ActionIn,
}

fn new_run_id() -> String {
  let mut r = [0u8; 12];
  getrandom::fill(&mut r).expect("system random source");
  format!("r_{}", tmgr_phone::hex(&r))
}

fn has_reason(a: &AnswerIn) -> bool {
  a.note.iter().chain(&a.free_text).any(|t| !t.trim().is_empty())
}

fn add_result(card: &mut CardIn, action: &ActionIn, state: &str, r: &ActionResult) {
  let (kind, noun) = match action {
    ActionIn::Pr(_) => ("pr", "Pull request"),
    ActionIn::Tag(_) => ("tag", "Tag"),
    ActionIn::Comment(_) => ("url", "Comment"),
    ActionIn::Task(_) => ("url", "Task"),
  };
  let link = r
    .url
    .clone()
    .and_then(|url| clean_link(LinkItem { kind: kind.to_owned(), url, label: None }))
    .filter(|_| card.links.items.len() < MAX_LINKS);
  if let Some(l) = &link {
    if kind == "pr" {
      card.links.pr = Some(l.url.clone());
    }
    card.links.items.push(l.clone());
  }
  let (fact_state, text) = match state {
    STATE_DONE => ("ok", format!("{noun} created")),
    _ => ("bad", r.error.clone().unwrap_or_else(|| format!("{noun} was not created"))),
  };
  if card.facts.len() < MAX_ROWS {
    card.facts.push(Fact { state: fact_state.into(), text: cap(&text, FACT_MAX), link: link.map(|l| l.url) });
  }
}

impl Store {
  pub(super) fn action_view(&self, id: &str, c: &CardIn) -> Result<Option<ActionView>, String> {
    let Some(a) = &c.action else { return Ok(None) };
    let run = self
      .conn
      .query_row(
        "SELECT run_id, state, payload, final_sha, result FROM action_runs WHERE card_id=?1 ORDER BY created_ms DESC LIMIT 1",
        [id],
        |r| {
          Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, Option<String>>(4)?,
          ))
        },
      )
      .optional()
      .map_err(sql_err)?;
    let proposed = a.sha256();
    let mut view = ActionView {
      kind: a.kind().to_owned(),
      payload: serde_json::to_value(a).map_err(json_err)?["payload"].clone(),
      sha256: proposed.clone(),
      editable: a.editable().iter().map(|s| (*s).to_owned()).collect(),
      run: None,
      approved: None,
    };
    if let Some((run_id, state, payload, final_sha, result)) = run {
      let result = result.map(|t| serde_json::from_str::<ActionResult>(&t)).transpose().map_err(json_err)?;
      let action: ActionIn = serde_json::from_str(&payload).map_err(json_err)?;
      view.approved =
        Some(Approved { action, edited: final_sha != proposed, sha256: final_sha, proposed_sha256: proposed });
      view.run = Some(RunView { run_id, state, result });
    }
    Ok(Some(view))
  }

  pub(super) fn answer_action(&self, id: &str, card: CardIn, a: AnswerIn) -> Result<(Event, CardIn), String> {
    let action = card.action.clone().ok_or_else(|| "this card has no action".to_owned())?;
    if a.deferred || a.dismissed || !a.line_comments.is_empty() || !a.choices.is_empty() {
      return Err(ONLY_DECIDE.into());
    }
    let tx = self.conn.unchecked_transaction().map_err(sql_err)?;
    let out = match a.choice.as_deref() {
      Some(OPT_APPROVE) => {
        let decision = a.action.clone().ok_or_else(|| "approving needs the action you reviewed".to_owned())?;
        self.approve_in(&tx, id, card, &action, &decision)?
      }
      Some(OPT_REJECT) => {
        if !has_reason(&a) {
          return Err("a reason is required to reject".into());
        }
        let proposed = action.sha256();
        let done = self.write_answer(&tx, id, card.clone(), a, None)?;
        self.record_approval(&tx, id, &card, &action, &proposed, &proposed, false, "rejected")?;
        done
      }
      _ => return Err(ONLY_DECIDE.into()),
    };
    tx.commit().map_err(sql_err)?;
    Ok(out)
  }

  #[allow(clippy::too_many_arguments)]
  fn record_approval(
    &self,
    conn: &Connection,
    id: &str,
    card: &CardIn,
    action: &ActionIn,
    proposed: &str,
    final_sha: &str,
    edited: bool,
    outcome: &str,
  ) -> Result<(), String> {
    conn
      .execute(
        "INSERT INTO action_approvals(card_id, kind, agent, proposed_sha, final_sha, edited, outcome, via, decided_ms)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, 'desktop', ?8)",
        params![id, action.kind(), card.source.agent, proposed, final_sha, edited, outcome, now_ms()],
      )
      .map(drop)
      .map_err(sql_err)
  }

  fn approve_in(
    &self,
    conn: &Connection,
    id: &str,
    card: CardIn,
    action: &ActionIn,
    decision: &ActionDecision,
  ) -> Result<(Event, CardIn), String> {
    let approved = action.decide(decision)?;
    let run_id = new_run_id();
    conn
      .execute(
        "INSERT INTO action_runs(run_id, card_id, kind, final_sha, payload, state, claimed_ms, created_ms)
         VALUES(?1, ?2, ?3, ?4, ?5, 'executing', NULL, ?6)",
        params![
          run_id,
          id,
          approved.action.kind(),
          approved.sha256,
          serde_json::to_string(&approved.action).map_err(json_err)?,
          now_ms()
        ],
      )
      .map_err(sql_err)?;
    self.record_approval(
      conn,
      id,
      &card,
      action,
      &approved.proposed_sha256,
      &approved.sha256,
      approved.edited,
      "executing",
    )?;
    let seq = self.next_seq(id)?;
    let event = Event {
      card_id: id.to_owned(),
      kind: "action".into(),
      seq,
      action: Some("executing".into()),
      ..Default::default()
    };
    conn
      .execute(
        "INSERT INTO events(card_id, seq, json, at_ms) VALUES(?1, ?2, ?3, ?4)",
        params![id, seq as i64, serde_json::to_string(&event).map_err(json_err)?, now_ms()],
      )
      .map_err(sql_err)?;
    conn.execute("UPDATE cards SET state='executing' WHERE id=?1", [id]).map_err(sql_err)?;
    Ok((event, card))
  }

  pub fn running_action(&self, card_id: &str) -> Result<Option<Running>, String> {
    let r = self
      .conn
      .query_row(
        "SELECT run_id, payload FROM action_runs WHERE card_id=?1 AND state='executing' LIMIT 1",
        [card_id],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
      )
      .optional()
      .map_err(sql_err)?;
    r.map(|(run_id, payload)| Ok(Running { run_id, action: serde_json::from_str(&payload).map_err(json_err)? }))
      .transpose()
  }

  pub fn pending_actions(&self) -> Result<Vec<PendingAction>, String> {
    let mut stmt = self
      .conn
      .prepare(
        "SELECT run_id, card_id, payload FROM action_runs
         WHERE state='executing' AND claimed_ms IS NULL AND kind IN ('comment','task') ORDER BY created_ms, rowid",
      )
      .map_err(sql_err)?;
    let rows = stmt
      .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))
      .map_err(sql_err)?
      .collect::<Result<Vec<_>, _>>()
      .map_err(sql_err)?;
    rows
      .into_iter()
      .map(|(run_id, card_id, payload)| {
        Ok(PendingAction { run_id, card_id, action: serde_json::from_str(&payload).map_err(json_err)? })
      })
      .collect()
  }

  pub fn claim_action(&self, run_id: &str) -> Result<PendingAction, String> {
    let n = self
      .conn
      .execute(
        "UPDATE action_runs SET claimed_ms=?2
         WHERE run_id=?1 AND state='executing' AND claimed_ms IS NULL AND kind IN ('comment','task')",
        params![run_id, now_ms()],
      )
      .map_err(sql_err)?;
    if n != 1 {
      return Err("this action cannot be claimed".into());
    }
    let (card_id, payload): (String, String) = self
      .conn
      .query_row("SELECT card_id, payload FROM action_runs WHERE run_id=?1", [run_id], |r| Ok((r.get(0)?, r.get(1)?)))
      .map_err(sql_err)?;
    Ok(PendingAction { run_id: run_id.to_owned(), card_id, action: serde_json::from_str(&payload).map_err(json_err)? })
  }

  pub fn finish_claimed(&mut self, run_id: &str, result: ActionResult) -> Result<(Event, CardIn), String> {
    let claimed: Option<bool> = self
      .conn
      .query_row(
        "SELECT claimed_ms IS NOT NULL AND kind IN ('comment','task') FROM action_runs WHERE run_id=?1",
        [run_id],
        |r| r.get(0),
      )
      .optional()
      .map_err(sql_err)?;
    if claimed != Some(true) {
      return Err("this action was not claimed by the window".into());
    }
    let state = if result.ok { STATE_DONE } else { STATE_FAILED };
    self.finish_run(run_id, state, result)
  }

  pub fn finish_run(&mut self, run_id: &str, state: &str, result: ActionResult) -> Result<(Event, CardIn), String> {
    if !matches!(state, STATE_DONE | STATE_FAILED | STATE_PARTIAL | STATE_UNKNOWN) {
      return Err("unknown run state".into());
    }
    let tx = self.conn.unchecked_transaction().map_err(sql_err)?;
    let done = self.finish_in(&tx, run_id, state, result)?;
    tx.commit().map_err(sql_err)?;
    Ok(done)
  }

  fn finish_in(
    &self,
    conn: &Connection,
    run_id: &str,
    state: &str,
    result: ActionResult,
  ) -> Result<(Event, CardIn), String> {
    let (card_id, final_sha, payload, current): (String, String, String, String) = conn
      .query_row("SELECT card_id, final_sha, payload, state FROM action_runs WHERE run_id=?1", [run_id], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
      })
      .optional()
      .map_err(sql_err)?
      .ok_or_else(|| "unknown run".to_owned())?;
    if current != "executing" {
      return Err("this action already finished".into());
    }
    let action: ActionIn = serde_json::from_str(&payload).map_err(json_err)?;
    let proposed: String = conn
      .query_row("SELECT proposed_sha FROM action_approvals WHERE card_id=?1", [&card_id], |r| r.get(0))
      .map_err(sql_err)?;
    let row = self.raw(&card_id)?.ok_or_else(|| "unknown card".to_owned())?;
    let mut card: CardIn = serde_json::from_str(&row.card).map_err(json_err)?;
    let now = now_ms();
    conn
      .execute(
        "UPDATE action_runs SET state=?2, result=?3, finished_ms=?4 WHERE run_id=?1 AND state='executing'",
        params![run_id, state, serde_json::to_string(&result).map_err(json_err)?, now],
      )
      .map_err(sql_err)?;
    conn
      .execute("UPDATE action_approvals SET outcome=?2 WHERE card_id=?1", params![card_id, state])
      .map_err(sql_err)?;
    add_result(&mut card, &action, state, &result);
    let seq = self.next_seq(&card_id)?;
    let label = card.options.iter().find(|o| o.id == OPT_APPROVE).map(|o| o.label.clone());
    let event = Event {
      card_id: card_id.clone(),
      kind: "answer".into(),
      seq,
      choice: Some(OPT_APPROVE.into()),
      choice_label: label,
      approved: Some(Approved { edited: final_sha != proposed, action, sha256: final_sha, proposed_sha256: proposed }),
      result: Some(result),
      ..Default::default()
    };
    let json = serde_json::to_string(&event).map_err(json_err)?;
    conn
      .execute(
        "INSERT INTO events(card_id, seq, json, at_ms) VALUES(?1, ?2, ?3, ?4)",
        params![card_id, seq as i64, json, now],
      )
      .map_err(sql_err)?;
    conn
      .execute(
        "UPDATE cards SET state='answered', answer=?2, card=?3, updated_ms=?4 WHERE id=?1",
        params![card_id, json, serde_json::to_string(&card).map_err(json_err)?, now],
      )
      .map_err(sql_err)?;
    Ok((event, card))
  }

  pub(super) fn sweep_executing(&self) -> Result<(), String> {
    let ids = self
      .conn
      .prepare("SELECT run_id FROM action_runs WHERE state='executing'")
      .map_err(sql_err)?
      .query_map([], |r| r.get::<_, String>(0))
      .map_err(sql_err)?
      .collect::<Result<Vec<_>, _>>()
      .map_err(sql_err)?;
    for id in ids {
      let tx = self.conn.unchecked_transaction().map_err(sql_err)?;
      let result = ActionResult { ok: false, error: Some(INTERRUPTED.into()), ..Default::default() };
      self.finish_in(&tx, &id, STATE_UNKNOWN, result)?;
      tx.commit().map_err(sql_err)?;
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::process::Command;
  use tmgr_ipc::actions::{CommentPayload, Commit, PrPayload, TagPayload};

  const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
  const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

  fn mem() -> Store {
    Store::open(Path::new(":memory:")).unwrap()
  }

  fn tag() -> ActionIn {
    ActionIn::Tag(TagPayload { repo: "tmgr-dev/frontend".into(), name: "cli-v1.0.140".into(), target_sha: A.into() })
  }

  fn comment() -> ActionIn {
    ActionIn::Comment(CommentPayload {
      origin: "https://api.tmgr.dev".into(),
      user_id: 1,
      task_id: 7,
      task_label: "TM-7: fix".into(),
      text: "Shipped".into(),
    })
  }

  fn action_card(action: ActionIn) -> CardIn {
    CardIn {
      title: "Propose".into(),
      source: Source { agent: "claude".into(), session: Some("s1".into()), ..Default::default() },
      options: vec![Opt { id: "x".into(), label: "Whatever".into(), recommended: true }],
      action: Some(action),
      ..Default::default()
    }
  }

  fn approve(a: &ActionIn) -> AnswerIn {
    AnswerIn {
      choice: Some("approve".into()),
      action: Some(ActionDecision { sha256: a.sha256(), edit: None }),
      ..Default::default()
    }
  }

  fn reject(reason: Option<&str>) -> AnswerIn {
    AnswerIn { choice: Some("reject".into()), note: reason.map(Into::into), ..Default::default() }
  }

  fn run_of(s: &Store, id: &str) -> String {
    s.get(id).unwrap().unwrap().action.unwrap().run.unwrap().run_id
  }

  #[test]
  fn creating_forces_the_two_options_and_exposes_the_hash() {
    let mut s = mem();
    let id = s.create(action_card(tag())).unwrap().id;
    let v = s.get(&id).unwrap().unwrap();
    let ids: Vec<_> = v.options.iter().map(|o| (o.id.as_str(), o.label.as_str())).collect();
    assert_eq!(ids, [("approve", "Approve"), ("reject", "Reject")]);
    let a = v.action.unwrap();
    assert_eq!((a.kind.as_str(), a.sha256.as_str()), ("tag", tag().sha256().as_str()));
    assert_eq!(a.editable, ["name"]);
    assert_eq!(a.payload["name"], "cli-v1.0.140");
    assert!(a.run.is_none() && a.approved.is_none());
  }

  #[test]
  fn invalid_actions_replaces_and_hook_cards_are_refused() {
    let mut s = mem();
    let bad = ActionIn::Tag(TagPayload { repo: "x".into(), name: "v1".into(), target_sha: A.into() });
    assert!(s.create(action_card(bad)).is_err());
    let mut r = action_card(tag());
    r.replaces = Some("c_x".into());
    assert!(s.create(r).is_err());
    let mut h = action_card(tag());
    h.reply_via = "hook".into();
    assert!(s.create(h).is_err());
  }

  #[test]
  fn key_reuse_never_rewrites_an_action_card() {
    let mut s = mem();
    let mut first = action_card(tag());
    first.key = Some("k".into());
    let id = s.create(first).unwrap().id;
    let mut second = action_card(ActionIn::Tag(TagPayload {
      repo: "tmgr-dev/frontend".into(),
      name: "cli-v9".into(),
      target_sha: B.into(),
    }));
    second.key = Some("k".into());
    let other = s.create(second).unwrap();
    assert!(other.fresh);
    assert_ne!(other.id, id);
    let plain = CardIn {
      title: "Plain".into(),
      key: Some("k".into()),
      source: Source { agent: "claude".into(), session: Some("s1".into()), ..Default::default() },
      ..Default::default()
    };
    let p = s.create(plain).unwrap();
    assert_ne!(p.id, id);
    assert_eq!(s.get(&id).unwrap().unwrap().action.unwrap().sha256, tag().sha256());
  }

  #[test]
  fn reply_patches_cannot_change_an_action_card() {
    let mut s = mem();
    let id = s.create(action_card(tag())).unwrap().id;
    let patch = CardPatch { title: Some("Other".into()), ..Default::default() };
    let (_, status) = s.agent_reply(&id, None, vec![], Some(patch)).unwrap();
    assert!(status.unwrap().starts_with("refused"));
    assert_eq!(s.get(&id).unwrap().unwrap().title, "Propose");
  }

  #[test]
  fn approve_needs_the_reviewed_hash_and_nothing_else_answers() {
    let mut s = mem();
    let id = s.create(action_card(tag())).unwrap().id;
    assert!(s.answer(&id, AnswerIn { choice: Some("approve".into()), ..Default::default() }).is_err());
    let stale = AnswerIn {
      choice: Some("approve".into()),
      action: Some(ActionDecision { sha256: "0".repeat(64), edit: None }),
      ..Default::default()
    };
    let e = s.answer(&id, stale).unwrap_err();
    assert!(e.contains("changed since you opened it"), "{e}");
    for bad in [
      AnswerIn { choice: Some("other".into()), ..Default::default() },
      AnswerIn { deferred: true, ..Default::default() },
      AnswerIn {
        choice: Some("approve".into()),
        line_comments: vec![LineComment { file: "f".into(), line: 1, text: "t".into(), side: None }],
        ..approve(&tag())
      },
    ] {
      assert!(s.answer(&id, bad).is_err());
    }
    assert!(s.answer(&id, AnswerIn { action: Some(ActionDecision::default()), ..pick("a") }).is_err());
    assert_eq!(s.get(&id).unwrap().unwrap().state, "open");
  }

  fn pick(c: &str) -> AnswerIn {
    AnswerIn { choice: Some(c.into()), ..Default::default() }
  }

  #[test]
  fn approving_starts_a_run_that_is_not_final_and_not_live() {
    let mut s = mem();
    let id = s.create(action_card(tag())).unwrap().id;
    let edit = [("name".to_string(), "cli-v1.0.141".to_string())].into();
    let a = AnswerIn {
      choice: Some("approve".into()),
      action: Some(ActionDecision { sha256: tag().sha256(), edit: Some(edit) }),
      ..Default::default()
    };
    let (event, _) = s.answer(&id, a).unwrap();
    assert_eq!((event.kind.as_str(), event.action.as_deref()), ("action", Some("executing")));
    assert!(!event.is_final());
    let v = s.get(&id).unwrap().unwrap();
    assert_eq!(v.state, "executing");
    assert!(v.answer.is_none());
    let run = v.action.as_ref().unwrap().run.as_ref().unwrap();
    assert_eq!(run.state, "executing");
    let approved = v.action.unwrap().approved.unwrap();
    assert!(approved.edited);
    assert_ne!(approved.sha256, approved.proposed_sha256);
    assert!(s.wait_event(&id, 0, true).unwrap().is_none());
    assert_eq!(s.wait_event(&id, 0, false).unwrap().unwrap().action.as_deref(), Some("executing"));
    assert!(s.answer(&id, approve(&tag())).is_err());
    assert!(s.answer(&id, reject(Some("no"))).is_err());
    assert!(s.owner_message(&id, MessageIn { text: "hi".into(), ..Default::default() }).is_err());
    assert_eq!(s.open_count(), 0);
    let (kind, edited, outcome): (String, bool, String) = s
      .conn
      .query_row("SELECT kind, edited, outcome FROM action_approvals WHERE card_id=?1", [&id], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
      })
      .unwrap();
    assert_eq!((kind.as_str(), edited, outcome.as_str()), ("tag", true, "executing"));
  }

  #[test]
  fn finishing_gives_one_final_event_with_the_result_and_the_url() {
    let mut s = mem();
    let id = s.create(action_card(tag())).unwrap().id;
    s.answer(&id, approve(&tag())).unwrap();
    let run = run_of(&s, &id);
    let result = ActionResult {
      ok: true,
      url: Some("https://github.com/tmgr-dev/frontend/releases/tag/cli-v1.0.140".into()),
      sha: Some(A.into()),
      error: None,
    };
    let (event, _) = s.finish_run(&run, "done", result.clone()).unwrap();
    assert!(event.is_final());
    assert_eq!(event.choice.as_deref(), Some("approve"));
    assert_eq!(event.result, Some(result));
    let approved = event.approved.unwrap();
    assert_eq!((approved.sha256.as_str(), approved.edited), (tag().sha256().as_str(), false));
    let v = s.get(&id).unwrap().unwrap();
    assert_eq!(v.state, "answered");
    assert_eq!(v.links.items.iter().filter(|i| i.kind == "tag").count(), 1);
    assert_eq!(v.facts.last().unwrap().state, "ok");
    assert_eq!(v.action.unwrap().run.unwrap().state, "done");
    assert_eq!(s.wait_event(&id, 0, true).unwrap().unwrap().seq, event.seq);
    assert!(s.finish_run(&run, "done", ActionResult::default()).is_err());
  }

  #[test]
  fn a_partial_pr_keeps_its_url_and_reads_as_a_problem() {
    let mut s = mem();
    let pr = ActionIn::Pr(PrPayload {
      repo: "tmgr-dev/frontend".into(),
      branch: "b".into(),
      base: "master".into(),
      base_sha: A.into(),
      head_sha: B.into(),
      title: "t".into(),
      body: String::new(),
      commits: vec![],
    });
    let id = s.create_unchecked(pr.clone());
    s.answer(&id, approve(&pr)).unwrap();
    let run = run_of(&s, &id);
    let result = ActionResult {
      ok: false,
      url: Some("https://github.com/tmgr-dev/frontend/pull/9".into()),
      sha: None,
      error: Some("head moved during creation; review the PR".into()),
    };
    s.finish_run(&run, "partial", result).unwrap();
    let v = s.get(&id).unwrap().unwrap();
    assert_eq!(v.links.pr.as_deref(), Some("https://github.com/tmgr-dev/frontend/pull/9"));
    assert_eq!(v.facts.last().unwrap().state, "bad");
    assert_eq!(v.action.unwrap().run.unwrap().state, "partial");
  }

  impl Store {
    fn create_unchecked(&mut self, action: ActionIn) -> String {
      let card = sanitize(action_card(action)).unwrap();
      let id = new_card_id();
      self
        .conn
        .execute(
          "INSERT INTO cards(id, created_ms, card, state) VALUES(?1, ?2, ?3, 'open')",
          params![id, now_ms(), serde_json::to_string(&card).unwrap()],
        )
        .unwrap();
      id
    }
  }

  #[test]
  fn rejecting_needs_a_reason_and_runs_nothing() {
    let mut s = mem();
    let id = s.create(action_card(tag())).unwrap().id;
    assert!(s.answer(&id, reject(None)).is_err());
    assert!(s.answer(&id, reject(Some("  "))).is_err());
    let (event, _) = s.answer(&id, reject(Some("wrong tag"))).unwrap();
    assert!(event.is_final());
    assert_eq!((event.choice.as_deref(), event.note.as_deref()), (Some("reject"), Some("wrong tag")));
    assert!(event.result.is_none() && event.approved.is_none());
    let v = s.get(&id).unwrap().unwrap();
    assert_eq!(v.state, "answered");
    assert!(v.action.unwrap().run.is_none());
    let outcome: String =
      s.conn.query_row("SELECT outcome FROM action_approvals WHERE card_id=?1", [&id], |r| r.get(0)).unwrap();
    assert_eq!(outcome, "rejected");
    assert!(s.pending_actions().unwrap().is_empty());
  }

  #[test]
  fn an_interrupted_run_becomes_unknown_at_startup_and_is_never_retried() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("decisions.db");
    let id = {
      let mut s = Store::open(&path).unwrap();
      let id = s.create(action_card(tag())).unwrap().id;
      s.answer(&id, approve(&tag())).unwrap();
      id
    };
    let s = Store::open(&path).unwrap();
    let v = s.get(&id).unwrap().unwrap();
    assert_eq!(v.state, "answered");
    let run = v.action.unwrap().run.unwrap();
    assert_eq!(run.state, "unknown");
    assert_eq!(run.result.unwrap().error.as_deref(), Some(INTERRUPTED));
    let e = s.wait_event(&id, 0, true).unwrap().unwrap();
    assert!(!e.result.unwrap().ok);
    assert!(s.running_action(&id).unwrap().is_none());
    assert!(Store::open(&path).is_ok());
  }

  #[test]
  fn the_window_claims_comment_and_task_runs_once_and_finishes_only_what_it_claimed() {
    let mut s = mem();
    let c = s.create(action_card(comment())).unwrap().id;
    let t = s.create(action_card(tag())).unwrap().id;
    s.answer(&c, approve(&comment())).unwrap();
    s.answer(&t, approve(&tag())).unwrap();
    let pending = s.pending_actions().unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!((pending[0].card_id.as_str(), &pending[0].action), (c.as_str(), &comment()));
    let run = pending[0].run_id.clone();
    let tag_run = run_of(&s, &t);
    assert!(s.claim_action(&tag_run).is_err());
    assert!(s.finish_claimed(&run, ActionResult::default()).is_err());
    assert_eq!(s.claim_action(&run).unwrap().action, comment());
    assert!(s.claim_action(&run).is_err());
    assert!(s.pending_actions().unwrap().is_empty());
    assert!(s.finish_claimed(&tag_run, ActionResult { ok: true, ..Default::default() }).is_err());
    let failed = ActionResult { ok: false, error: Some("wrong account".into()), ..Default::default() };
    let (event, _) = s.finish_claimed(&run, failed).unwrap();
    assert!(event.is_final());
    assert_eq!(s.get(&c).unwrap().unwrap().action.unwrap().run.unwrap().state, "failed");
    assert!(s.finish_claimed(&run, ActionResult::default()).is_err());
  }

  fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
      .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
      .arg("-C")
      .arg(dir)
      .args(args)
      .env("GIT_CONFIG_GLOBAL", "/dev/null")
      .env("GIT_CONFIG_NOSYSTEM", "1")
      .output()
      .unwrap();
    assert!(out.status.success());
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
  }

  #[test]
  fn a_pr_card_is_stored_only_when_the_local_commits_match() {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "master"]);
    git(dir.path(), &["commit", "-q", "--allow-empty", "-m", "base"]);
    let base = git(dir.path(), &["rev-parse", "HEAD"]);
    git(dir.path(), &["commit", "-q", "--allow-empty", "-m", "feat: x"]);
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    let pr = |subject: &str| {
      ActionIn::Pr(PrPayload {
        repo: "tmgr-dev/frontend".into(),
        branch: "tm-1012".into(),
        base: "master".into(),
        base_sha: base.clone(),
        head_sha: head.clone(),
        title: "feat: x".into(),
        body: String::new(),
        commits: vec![Commit { sha: head.clone(), subject: subject.into() }],
      })
    };
    let with_diff = |a: ActionIn| {
      let mut c = action_card(a);
      c.diffs = vec![DiffSpec {
        repo: dir.path().to_string_lossy().into_owned(),
        base: Some(base.clone()),
        head: Some(head.clone()),
      }];
      c
    };
    let mut s = mem();
    assert!(s.create(with_diff(pr("feat: x"))).is_ok());
    assert!(s.create(with_diff(pr("something harmless"))).is_err());
    assert!(s.create(action_card(pr("feat: x"))).is_err());
    let mut wrong_pin = with_diff(pr("feat: x"));
    wrong_pin.diffs[0].head = Some(base.clone());
    assert!(s.create(wrong_pin).is_err());
    let mut gone = with_diff(pr("feat: x"));
    gone.diffs[0].repo = dir.path().join("missing").to_string_lossy().into_owned();
    assert!(s.create(gone).is_err());
  }

  #[test]
  fn phone_answers_on_an_action_card_are_not_eligible() {
    let mut s = mem();
    let id = s.create(action_card(tag())).unwrap().id;
    let v = s.get(&id).unwrap().unwrap();
    assert!(!super::super::super::phone::eligible(&v));
    let req = PhoneReq {
      answer_id: "a1".into(),
      phone_fp: "ffffffffffff".into(),
      card_id: id.clone(),
      card_digest: String::new(),
      session: String::new(),
      action: tmgr_phone::Action::Answer,
      choice: Some("approve".into()),
      text: None,
      choices: None,
      issued_at: 1_000,
    };
    let out = s.phone_answer(&req, 1_000).unwrap();
    assert_eq!(out.result, AckResult::NotEligible);
    assert_eq!(s.get(&id).unwrap().unwrap().state, "open");
  }
}
