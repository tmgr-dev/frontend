use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::backend::{ChatMessage, HistoryParams, ManagerBackend, RoomInfo, RoomKind, RoomRef, SearchParams, SendParams, SlashCommand, StopOutcome, MAX_HISTORY};
use super::local::LocalPort;
use super::relay::port::RelayPort;

pub struct CompositeBackend {
  local: Arc<dyn LocalPort>,
  relay: Arc<dyn RelayPort>,
}

const MENTION_LOCAL_ONLY: &str = "mentions work only between chats on this machine";

pub fn without_mention(mut m: ChatMessage) -> ChatMessage {
  m.mention = None;
  m
}

const BLOB_LOCAL_ONLY: &str = "Attachments to another machine are not supported yet";

const CONTROL_LOCAL_ONLY: &str = "only this machine's chats can be stopped or deleted here";

enum Route {
  Local,
  Relay,
}

impl CompositeBackend {
  pub fn new(local: Arc<dyn LocalPort>, relay: Arc<dyn RelayPort>) -> Self {
    Self { local, relay }
  }

  fn route(&self, room: &RoomRef) -> Result<Route, String> {
    if self.local.machine().is_ok_and(|m| m == room.machine) {
      Ok(Route::Local)
    } else if self.relay.owns(room) {
      Ok(Route::Relay)
    } else {
      Err("unknown room".into())
    }
  }
}

fn static_commands(kind: RoomKind) -> Vec<SlashCommand> {
  let cmd = |name: &str, description: &str, args: Option<&str>, group: &str, dangerous| SlashCommand {
    name: name.into(),
    description: description.into(),
    args: args.map(Into::into),
    group: group.into(),
    dangerous,
  };
  // STUB: commands list until runner exposes one
  match kind {
    RoomKind::Manager => vec![
      cmd("/summary", "Morning summary of all sessions", None, "TMGR", false),
      cmd("/ask", "Ask a session a question", Some("@session <question>"), "TMGR", false),
      cmd("/stop", "Stop a session", Some("@session"), "TMGR", true),
    ],
    RoomKind::Session => vec![
      cmd("/compact", "Compact the conversation", Some("[instructions]"), "Agent built-ins", false),
      cmd("/clear", "Clear conversation history", None, "Agent built-ins", true),
      cmd("/model", "Switch model", Some("<name>"), "Agent built-ins", false),
    ],
  }
}

impl ManagerBackend for CompositeBackend {
  fn rooms(&self) -> Result<Vec<RoomInfo>, String> {
    let local = self.local.rooms();
    let relay = self.relay.rooms();
    if let (Err(e), Err(_)) = (&local, &relay) {
      return Err(e.clone());
    }
    let mut rooms = local.unwrap_or_default();
    let own = self.local.machine().ok();
    if let Some(own) = &own {
      rooms.iter_mut().filter(|r| &r.room.machine != own).for_each(|r| r.local = false);
    }
    let pinned = |r: &RoomInfo| r.room.kind == RoomKind::Manager && own.as_deref() == Some(r.room.machine.as_str());
    rooms.sort_by_key(|r| !pinned(r));
    rooms.extend(relay.unwrap_or_default().into_iter().map(|r| RoomInfo { local: false, mention: None, unrestricted: false, last: r.last.map(without_mention), ..r }));
    let mut seen: Vec<RoomRef> = vec![];
    rooms.retain(|r| {
      let fresh = !seen.contains(&r.room);
      if fresh {
        seen.push(r.room.clone());
      }
      fresh
    });
    Ok(rooms)
  }

  fn history(&self, params: HistoryParams) -> Result<Vec<ChatMessage>, String> {
    match self.route(&params.room)? {
      Route::Local => self.local.history(&params),
      Route::Relay => self.relay.history(&params).map(|rows| rows.into_iter().map(without_mention).collect()),
    }
  }

  fn send(&self, params: SendParams) -> Result<ChatMessage, String> {
    match self.route(&params.room)? {
      Route::Local => self.local.send(params),
      Route::Relay if params.mention.is_some() => Err(MENTION_LOCAL_ONLY.into()),
      Route::Relay => self.relay.send(params).map(without_mention),
    }
  }

  fn search(&self, params: SearchParams) -> Result<Vec<ChatMessage>, String> {
    if let Some(room) = &params.room {
      return match self.route(room)? {
        Route::Local => self.local.search(&params),
        Route::Relay => self.relay.search(&params).map(|rows| rows.into_iter().map(without_mention).collect()),
      };
    }
    let local = self.local.search(&params);
    let relay = self.relay.search(&params);
    if let (Err(e), Err(_)) = (&local, &relay) {
      return Err(e.clone());
    }
    let mut hits = local.unwrap_or_default();
    hits.extend(relay.unwrap_or_default().into_iter().map(without_mention));
    hits.sort_by(|a, b| b.ts_ms.cmp(&a.ts_ms).then_with(|| b.id.cmp(&a.id)));
    let mut seen = HashSet::new();
    hits.retain(|m| seen.insert(m.id.clone()));
    hits.truncate(params.limit.unwrap_or(50).min(MAX_HISTORY) as usize);
    Ok(hits)
  }

  fn commands(&self, room: &RoomRef) -> Result<Vec<SlashCommand>, String> {
    self.route(room)?;
    Ok(static_commands(room.kind))
  }

  fn queued(&self, room: &RoomRef) -> Result<Vec<String>, String> {
    match self.route(room)? {
      Route::Local => self.local.queued(room),
      Route::Relay => Ok(vec![]),
    }
  }

  fn queued_why(&self, room: &RoomRef) -> Result<HashMap<String, String>, String> {
    match self.route(room)? {
      Route::Local => self.local.queued_why(room),
      Route::Relay => Ok(HashMap::new()),
    }
  }

  fn cancel(&self, room: &RoomRef, id: &str) -> Result<bool, String> {
    match self.route(room)? {
      Route::Local => self.local.cancel(room, id),
      Route::Relay => Err("only this machine's queued messages can be cancelled here".into()),
    }
  }

  fn stop(&self, room: &RoomRef, force: bool) -> Result<StopOutcome, String> {
    match self.route(room)? {
      Route::Local => self.local.stop(room, force),
      Route::Relay => Err(CONTROL_LOCAL_ONLY.into()),
    }
  }

  fn remove(&self, room: &RoomRef) -> Result<(), String> {
    match self.route(room)? {
      Route::Local => self.local.remove(room),
      Route::Relay => Err(CONTROL_LOCAL_ONLY.into()),
    }
  }

  fn mention_inject(&self, room: &RoomRef, mention_id: &str, confirmed_risky: bool) -> Result<(), String> {
    match self.route(room)? {
      Route::Local => self.local.mention_inject(room, mention_id, confirmed_risky),
      Route::Relay => Err(MENTION_LOCAL_ONLY.into()),
    }
  }

  fn blob_put(&self, room: &RoomRef, sha256: &str, bytes: &[u8]) -> Result<(), String> {
    match self.route(room)? {
      Route::Local => self.local.blob_put(sha256, bytes),
      Route::Relay => Err(BLOB_LOCAL_ONLY.into()),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::manager::backend::Author;
  use crate::manager::relay::port::{NoRelay, TrustState};
  use std::sync::Mutex;

  const ME: &str = "mx_me";
  const FAR: &str = "mx_far";

  fn room(kind: RoomKind, machine: &str, session: Option<&str>) -> RoomRef {
    RoomRef { kind, machine: machine.into(), session: session.map(Into::into) }
  }

  fn info(r: RoomRef) -> RoomInfo {
    RoomInfo { room: r, title: "t".into(), agent: None, state: None, context_pct: None, context_tokens: None, model: None, last: None, local: true, unrestricted: false, mention: None }
  }

  fn msg(id: &str, ts: u64, r: RoomRef) -> ChatMessage {
    ChatMessage {
      v: 1,
      id: id.into(),
      room: r,
      author: Author { kind: "owner".into(), id: "o".into(), machine: None, agent: None },
      ts_ms: ts,
      kind: "text".into(),
      body_md: "x".into(),
      attachments: vec![],
      reply_to: None,
      decision: None,
      mention: None,
      poll: None,
    }
  }

  fn params(r: RoomRef) -> SendParams {
    SendParams { room: r, body_md: "hi".into(), reply_to: None, attachments: vec![], id: None, ts_ms: None, mention: None }
  }

  #[derive(Default)]
  struct FakeLocal {
    down: bool,
    rooms: Vec<RoomInfo>,
    hits: Vec<ChatMessage>,
    queue: Vec<String>,
    calls: Mutex<Vec<&'static str>>,
  }

  impl FakeLocal {
    fn note(&self, c: &'static str) {
      self.calls.lock().unwrap().push(c);
    }
    fn up(&self) -> Result<(), String> {
      if self.down { Err("Agent runner is not running".into()) } else { Ok(()) }
    }
  }

  impl LocalPort for FakeLocal {
    fn machine(&self) -> Result<String, String> {
      self.up().map(|_| ME.to_owned())
    }
    fn rooms(&self) -> Result<Vec<RoomInfo>, String> {
      self.up().map(|_| self.rooms.clone())
    }
    fn history(&self, p: &HistoryParams) -> Result<Vec<ChatMessage>, String> {
      self.note("history");
      self.up().map(|_| vec![msg("l1", 1, p.room.clone())])
    }
    fn search(&self, _: &SearchParams) -> Result<Vec<ChatMessage>, String> {
      self.up().map(|_| self.hits.clone())
    }
    fn send(&self, p: SendParams) -> Result<ChatMessage, String> {
      self.note("send");
      self.up().map(|_| msg("ls", 9, p.room))
    }
    fn queued_why(&self, _: &RoomRef) -> Result<HashMap<String, String>, String> {
      self.note("queued_why");
      Ok(HashMap::from([("q1".to_owned(), "turn".to_owned())]))
    }

    fn queued(&self, _: &RoomRef) -> Result<Vec<String>, String> {
      self.note("queued");
      self.up().map(|_| self.queue.clone())
    }
    fn cancel(&self, _: &RoomRef, _: &str) -> Result<bool, String> {
      self.note("cancel");
      self.up().map(|_| true)
    }
    fn stop(&self, _: &RoomRef, force: bool) -> Result<StopOutcome, String> {
      self.note(if force { "force" } else { "stop" });
      self.up().map(|_| StopOutcome { exited: true })
    }
    fn remove(&self, _: &RoomRef) -> Result<(), String> {
      self.note("remove");
      self.up()
    }
    fn mention_inject(&self, _: &RoomRef, _: &str, _: bool) -> Result<(), String> {
      self.note("inject");
      self.up()
    }
    fn blob_put(&self, _: &str, _: &[u8]) -> Result<(), String> {
      self.note("blob");
      self.up()
    }
  }

  #[derive(Default)]
  struct FakeRelay {
    rooms: Vec<RoomInfo>,
    hits: Vec<ChatMessage>,
    calls: Mutex<Vec<&'static str>>,
  }

  impl RelayPort for FakeRelay {
    fn rooms(&self) -> Result<Vec<RoomInfo>, String> {
      Ok(self.rooms.clone())
    }
    fn history(&self, p: &HistoryParams) -> Result<Vec<ChatMessage>, String> {
      self.calls.lock().unwrap().push("history");
      Ok(vec![msg("r1", 1, p.room.clone())])
    }
    fn search(&self, _: &SearchParams) -> Result<Vec<ChatMessage>, String> {
      Ok(self.hits.clone())
    }
    fn send(&self, p: SendParams) -> Result<ChatMessage, String> {
      self.calls.lock().unwrap().push("send");
      Ok(msg("rs", 9, p.room))
    }
    fn owns(&self, r: &RoomRef) -> bool {
      self.rooms.iter().any(|i| &i.room == r)
    }
    fn trust_state(&self) -> Result<TrustState, String> {
      NoRelay.trust_state()
    }
    fn decide_join(&self, _: &str, _: bool) -> Result<(), String> {
      Ok(())
    }
    fn revoke(&self, _: &str) -> Result<(), String> {
      Ok(())
    }
    fn reset_identity(&self) -> Result<(), String> {
      Ok(())
    }
    fn poll_once(&self) -> Result<Vec<ChatMessage>, String> {
      Ok(vec![])
    }
  }

  fn mine() -> RoomRef {
    room(RoomKind::Manager, ME, None)
  }
  fn far_mgr() -> RoomRef {
    room(RoomKind::Manager, FAR, None)
  }
  fn far_sess() -> RoomRef {
    room(RoomKind::Session, FAR, Some("s1"))
  }

  fn build(local: FakeLocal, relay: FakeRelay) -> (CompositeBackend, Arc<FakeLocal>, Arc<FakeRelay>) {
    let (l, r) = (Arc::new(local), Arc::new(relay));
    (CompositeBackend::new(l.clone(), r.clone()), l, r)
  }

  fn relay_with_far() -> FakeRelay {
    FakeRelay { rooms: vec![info(far_mgr()), info(far_sess())], ..Default::default() }
  }

  #[test]
  fn own_manager_room_is_pinned_first_then_relay_rooms() {
    let sess = room(RoomKind::Session, ME, Some("a"));
    let local = FakeLocal { rooms: vec![info(sess.clone()), info(mine())], ..Default::default() };
    let (c, ..) = build(local, relay_with_far());
    let rooms: Vec<RoomRef> = c.rooms().unwrap().into_iter().map(|r| r.room).collect();
    assert_eq!(rooms, vec![mine(), sess, far_mgr(), far_sess()]);
  }

  #[test]
  fn rooms_are_deduped_by_room_ref() {
    let local = FakeLocal { rooms: vec![info(mine())], ..Default::default() };
    let relay = FakeRelay { rooms: vec![info(mine()), info(far_mgr())], ..Default::default() };
    let (c, ..) = build(local, relay);
    assert_eq!(c.rooms().unwrap().len(), 2);
  }

  #[test]
  fn only_relay_rooms_are_marked_not_local() {
    let local = FakeLocal { rooms: vec![info(mine())], ..Default::default() };
    let (c, ..) = build(local, relay_with_far());
    let flags: Vec<(RoomRef, bool)> = c.rooms().unwrap().into_iter().map(|r| (r.room, r.local)).collect();
    assert_eq!(flags, vec![(mine(), true), (far_mgr(), false), (far_sess(), false)]);
  }

  #[test]
  fn a_room_of_another_machine_in_the_local_list_is_not_marked_local() {
    let local = FakeLocal { rooms: vec![info(mine()), info(far_mgr()), info(far_sess())], ..Default::default() };
    let (c, ..) = build(local, FakeRelay::default());
    let flags: Vec<(RoomRef, bool)> = c.rooms().unwrap().into_iter().map(|r| (r.room, r.local)).collect();
    assert_eq!(flags, vec![(mine(), true), (far_mgr(), false), (far_sess(), false)]);
  }

  #[test]
  fn room_info_without_the_local_field_deserializes_as_local() {
    let r: RoomInfo = serde_json::from_str(r#"{"room":{"kind":"manager","machine":"m"},"title":"t"}"#).unwrap();
    assert!(r.local);
  }

  #[test]
  fn local_down_still_lists_relay_rooms() {
    let (c, ..) = build(FakeLocal { down: true, ..Default::default() }, relay_with_far());
    assert_eq!(c.rooms().unwrap().len(), 2);
  }

  #[test]
  fn history_routes_by_machine() {
    let (c, l, r) = build(FakeLocal::default(), relay_with_far());
    let h = |room| HistoryParams { room, before: None, limit: None };
    assert_eq!(c.history(h(mine())).unwrap()[0].id, "l1");
    assert_eq!(c.history(h(far_sess())).unwrap()[0].id, "r1");
    assert_eq!(*l.calls.lock().unwrap(), ["history"]);
    assert_eq!(*r.calls.lock().unwrap(), ["history"]);
  }

  #[test]
  fn unknown_remote_room_is_an_error() {
    let (c, ..) = build(FakeLocal::default(), FakeRelay::default());
    let other = room(RoomKind::Session, "mx_none", Some("z"));
    assert_eq!(c.history(HistoryParams { room: other.clone(), before: None, limit: None }).unwrap_err(), "unknown room");
    assert!(c.send(params(other.clone())).is_err());
    assert!(c.commands(&other).is_err());
  }

  #[test]
  fn blobs_go_to_the_local_runner_and_are_refused_for_another_machine() {
    let (c, l, r) = build(FakeLocal::default(), relay_with_far());
    c.blob_put(&mine(), "sha", b"x").unwrap();
    assert_eq!(l.calls.lock().unwrap().as_slice(), ["blob"]);
    let err = c.blob_put(&far_mgr(), "sha", b"x").unwrap_err();
    assert_eq!(err, "Attachments to another machine are not supported yet");
    assert_eq!(l.calls.lock().unwrap().len(), 1);
    assert!(r.calls.lock().unwrap().is_empty());
  }

  #[test]
  fn send_goes_to_the_owning_side() {
    let (c, l, r) = build(FakeLocal::default(), relay_with_far());
    assert_eq!(c.send(params(far_sess())).unwrap().id, "rs");
    assert_eq!(c.send(params(mine())).unwrap().id, "ls");
    assert_eq!(*r.calls.lock().unwrap(), ["send"]);
    assert_eq!(*l.calls.lock().unwrap(), ["send"]);
  }

  #[test]
  fn local_down_makes_local_room_unreachable() {
    let (c, ..) = build(FakeLocal { down: true, ..Default::default() }, relay_with_far());
    assert!(c.send(params(mine())).is_err());
  }

  #[test]
  fn unscoped_search_merges_dedupes_sorts_and_limits() {
    let local = FakeLocal { hits: vec![msg("a", 1, mine()), msg("dup", 5, mine())], ..Default::default() };
    let relay = FakeRelay { hits: vec![msg("dup", 5, far_mgr()), msg("b", 9, far_mgr()), msg("c", 3, far_mgr())], ..relay_with_far() };
    let (c, ..) = build(local, relay);
    let q = |limit| SearchParams { query: "x".into(), room: None, limit };
    let ids: Vec<String> = c.search(q(None)).unwrap().into_iter().map(|m| m.id).collect();
    assert_eq!(ids, ["b", "dup", "c", "a"]);
    assert_eq!(c.search(q(Some(2))).unwrap().len(), 2);
  }

  #[test]
  fn scoped_search_goes_to_one_side_and_survives_local_down() {
    let relay = FakeRelay { hits: vec![msg("b", 9, far_mgr())], ..relay_with_far() };
    let (c, ..) = build(FakeLocal { down: true, ..Default::default() }, relay);
    let scoped = SearchParams { query: "x".into(), room: Some(far_mgr()), limit: None };
    assert_eq!(c.search(scoped).unwrap().len(), 1);
    let all = SearchParams { query: "x".into(), room: None, limit: None };
    assert_eq!(c.search(all).unwrap().len(), 1);
  }

  #[test]
  fn queue_is_read_and_cancelled_only_on_this_machine() {
    let local = FakeLocal { queue: vec!["q1".into()], ..Default::default() };
    let (c, l, r) = build(local, relay_with_far());
    assert_eq!(c.queued(&mine()).unwrap(), ["q1"]);
    assert!(c.cancel(&mine(), "q1").unwrap());
    assert!(c.queued(&far_sess()).unwrap().is_empty());
    assert!(c.cancel(&far_sess(), "q1").is_err());
    assert_eq!(*l.calls.lock().unwrap(), ["queued", "cancel"]);
    assert!(r.calls.lock().unwrap().is_empty());
  }

  #[test]
  fn queue_reasons_come_only_from_this_machine() {
    let (c, l, _) = build(FakeLocal::default(), relay_with_far());
    assert_eq!(c.queued_why(&mine()).unwrap()["q1"], "turn");
    assert!(c.queued_why(&far_sess()).unwrap().is_empty());
    assert_eq!(*l.calls.lock().unwrap(), ["queued_why"]);
  }

  #[test]
  fn commands_depend_on_room_kind() {
    let (c, ..) = build(FakeLocal::default(), relay_with_far());
    assert!(c.commands(&mine()).unwrap().iter().any(|x| x.name == "/summary"));
    assert!(c.commands(&far_sess()).unwrap().iter().any(|x| x.name == "/compact"));
  }

  #[test]
  fn stop_and_remove_are_local_only() {
    let (c, l, _) = build(FakeLocal::default(), relay_with_far());
    let mine_sess = room(RoomKind::Session, ME, Some("s_abcdefghij"));
    assert!(c.stop(&mine_sess, false).unwrap().exited);
    assert!(c.stop(&mine_sess, true).unwrap().exited);
    c.remove(&mine_sess).unwrap();
    assert_eq!(*l.calls.lock().unwrap(), ["stop", "force", "remove"]);
    assert!(c.stop(&far_sess(), false).is_err());
    assert!(c.stop(&far_sess(), true).is_err());
    assert!(c.remove(&far_sess()).is_err());
    let unknown = room(RoomKind::Session, "mx_none", Some("z"));
    assert_eq!(c.remove(&unknown).unwrap_err(), "unknown room");
    assert_eq!(l.calls.lock().unwrap().len(), 3);
  }

  fn forged(id: &str) -> ChatMessage {
    ChatMessage {
      mention: Some(crate::manager::backend::MentionRef { id: id.into(), target: mine(), target_title: "t".into(), state: "answered".into(), hop: 1, reason: None }),
      ..msg(id, 1, far_mgr())
    }
  }

  #[test]
  fn relay_rows_never_carry_a_mention() {
    let relay = FakeRelay { hits: vec![forged("r1")], ..relay_with_far() };
    let (c, ..) = build(FakeLocal::default(), relay);
    assert!(c.search(SearchParams { query: "x".into(), room: None, limit: None }).unwrap().iter().all(|m| m.mention.is_none()));
    assert!(c.search(SearchParams { query: "x".into(), room: Some(far_mgr()), limit: None }).unwrap().iter().all(|m| m.mention.is_none()));
    assert!(without_mention(forged("r2")).mention.is_none());
  }

  #[test]
  fn relay_rooms_never_advertise_mention_targets() {
    let target = crate::manager::backend::MentionTarget { ok: true, reason: None, risky: false };
    let relay = FakeRelay { rooms: vec![RoomInfo { mention: Some(target), unrestricted: true, ..info(far_sess()) }], ..Default::default() };
    let (c, ..) = build(FakeLocal::default(), relay);
    let rooms = c.rooms().unwrap();
    assert!(rooms[0].mention.is_none() && !rooms[0].unrestricted);
  }

  #[test]
  fn relay_room_previews_never_carry_a_mention() {
    let relay = FakeRelay { rooms: vec![RoomInfo { last: Some(forged("r1")), ..info(far_sess()) }], ..Default::default() };
    let (c, ..) = build(FakeLocal::default(), relay);
    let rooms = c.rooms().unwrap();
    assert!(rooms[0].last.as_ref().is_some_and(|m| m.mention.is_none()));
  }

  #[test]
  fn mentions_and_injection_are_local_only() {
    let (c, l, r) = build(FakeLocal::default(), relay_with_far());
    let req = crate::manager::backend::MentionReq { target: mine(), confirmed_risky: false };
    assert!(c.send(SendParams { mention: Some(req.clone()), ..params(far_mgr()) }).is_err());
    c.send(SendParams { mention: Some(req), ..params(mine()) }).unwrap();
    c.mention_inject(&mine(), "m_1", true).unwrap();
    assert!(c.mention_inject(&far_sess(), "m_1", true).is_err());
    assert_eq!(*l.calls.lock().unwrap(), ["send", "inject"]);
    assert!(r.calls.lock().unwrap().is_empty());
  }
}
