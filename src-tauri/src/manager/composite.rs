use std::collections::HashSet;
use std::sync::Arc;

use super::backend::{ChatMessage, HistoryParams, ManagerBackend, RoomInfo, RoomKind, RoomRef, SearchParams, SendParams, SlashCommand, MAX_HISTORY};
use super::local::LocalPort;
use super::relay::port::RelayPort;

pub struct CompositeBackend {
  local: Arc<dyn LocalPort>,
  relay: Arc<dyn RelayPort>,
}

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
    let pinned = |r: &RoomInfo| r.room.kind == RoomKind::Manager && own.as_deref() == Some(r.room.machine.as_str());
    rooms.sort_by_key(|r| !pinned(r));
    rooms.extend(relay.unwrap_or_default());
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
      Route::Relay => self.relay.history(&params),
    }
  }

  fn send(&self, params: SendParams) -> Result<ChatMessage, String> {
    match self.route(&params.room)? {
      Route::Local => self.local.send(params),
      Route::Relay => self.relay.send(params),
    }
  }

  fn search(&self, params: SearchParams) -> Result<Vec<ChatMessage>, String> {
    if let Some(room) = &params.room {
      return match self.route(room)? {
        Route::Local => self.local.search(&params),
        Route::Relay => self.relay.search(&params),
      };
    }
    let local = self.local.search(&params);
    let relay = self.relay.search(&params);
    if let (Err(e), Err(_)) = (&local, &relay) {
      return Err(e.clone());
    }
    let mut hits = local.unwrap_or_default();
    hits.extend(relay.unwrap_or_default());
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
    RoomInfo { room: r, title: "t".into(), agent: None, state: None, context_pct: None, context_tokens: None, last: None }
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
    }
  }

  fn params(r: RoomRef) -> SendParams {
    SendParams { room: r, body_md: "hi".into(), reply_to: None, attachments: vec![], id: None, ts_ms: None }
  }

  #[derive(Default)]
  struct FakeLocal {
    down: bool,
    rooms: Vec<RoomInfo>,
    hits: Vec<ChatMessage>,
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
  fn commands_depend_on_room_kind() {
    let (c, ..) = build(FakeLocal::default(), relay_with_far());
    assert!(c.commands(&mine()).unwrap().iter().any(|x| x.name == "/summary"));
    assert!(c.commands(&far_sess()).unwrap().iter().any(|x| x.name == "/compact"));
  }
}
