use super::*;

const H: u64 = 3_600_000;
const MIN: u64 = 60_000;
const DAY: u64 = 24 * H;

fn at(c: &Clock) -> u64 {
    c.0.load(Ordering::SeqCst)
}

fn pair2(fake: &Fake, ca: &Clock, cb: &Clock) -> (Engine, Engine) {
    let (ta, tb) = (at(ca), at(cb));
    ca.set(tb);
    let a = engine_with(fake, ca, "Desktop A");
    let b = engine_with(fake, cb, "Desktop B");
    let code = a.open_join_code().unwrap();
    b.join_with_code(&code).unwrap();
    a.poll_once().unwrap();
    a.decide_join(&my_id(&b), true).unwrap();
    b.poll_once().unwrap();
    a.poll_once().unwrap();
    ca.set(ta);
    (a, b)
}

fn send_at(e: &Engine, clock: &Clock, text: &str) -> ChatMessage {
    let mut m = text_msg(new_message_id(), text);
    m.ts_ms = at(clock);
    e.send_chat(&m, true).unwrap();
    m
}

fn delivered_ts(e: &Engine) -> Vec<u64> {
    e.s.got
        .lock()
        .unwrap()
        .iter()
        .map(|(_, m)| m.ts_ms)
        .collect()
}

fn close(a: u64, b: u64) -> bool {
    a.abs_diff(b) < 50
}

fn skewed_pair_delivers_converted(skew_a: i64) {
    let fake = Fake::default();
    let cb = Clock::new(T0);
    let ca = Clock::new((T0 as i64 + skew_a) as u64);
    let (a, b) = pair2(&fake, &ca, &cb);
    for _ in 0..6 {
        send_at(&a, &ca, "warm up");
        send_at(&b, &cb, "warm up");
        b.poll_once().unwrap();
        a.poll_once().unwrap();
        cb.set(at(&cb) + 10 * MIN);
        ca.set(at(&ca) + 10 * MIN);
    }
    send_at(&a, &ca, "a to b");
    assert_eq!(b.poll_once().unwrap().len(), 1);
    assert!(close(*delivered_ts(&b).last().unwrap(), at(&cb)));
    send_at(&b, &cb, "b to a");
    assert_eq!(a.poll_once().unwrap().len(), 1);
    assert!(close(*delivered_ts(&a).last().unwrap(), at(&ca)));
}

#[test]
fn sender_an_hour_ahead_is_delivered_with_receiver_clock_timestamps() {
    skewed_pair_delivers_converted(H as i64);
}

#[test]
fn sender_an_hour_behind_is_delivered_with_receiver_clock_timestamps() {
    skewed_pair_delivers_converted(-(H as i64));
}

#[test]
fn replay_of_a_skewed_senders_message_stays_deduped_after_the_estimate_changes() {
    let fake = Fake::default();
    let cb = Clock::new(T0);
    let ca = Clock::new(T0 - H);
    let (a, b) = pair2(&fake, &ca, &cb);
    let bid = my_id(&b);
    send_at(&a, &ca, "once");
    let (id, from, body) = fake.first_body(&bid);
    assert_eq!(b.poll_once().unwrap().len(), 1);
    cb.set(T0 + MIN);
    ca.set(T0 + MIN);
    send_at(&a, &ca, "after the clock fix");
    assert_eq!(b.poll_once().unwrap().len(), 1);
    fake.inject(&bid, &id, &from, &body);
    assert!(b.poll_once().unwrap().is_empty());
    assert_eq!(history(&b), ["once", "after the clock fix"]);
}

#[test]
fn replay_after_the_window_closes_is_dropped() {
    let fake = Fake::default();
    let cb = Clock::new(T0);
    let ca = Clock::new(T0 + H);
    let (a, b) = pair2(&fake, &ca, &cb);
    let bid = my_id(&b);
    send_at(&a, &ca, "once");
    let (id, from, body) = fake.first_body(&bid);
    assert_eq!(b.poll_once().unwrap().len(), 1);
    cb.set(T0 + 31 * DAY + 1);
    fake.inject(&bid, &id, &from, &body);
    assert!(b.poll_once().unwrap().is_empty());
    assert_eq!(history(&b), ["once"]);
}

#[test]
fn messages_held_while_the_receiver_was_idle_stay_stale() {
    let fake = Fake::default();
    let cb = Clock::new(T0);
    let ca = Clock::new(T0 + H);
    let (a, b) = pair2(&fake, &ca, &cb);
    send_at(&a, &ca, "learn");
    b.poll_once().unwrap();
    for t in ["m1", "m2", "m3"] {
        send_at(&a, &ca, t);
    }
    cb.set(at(&cb) + 2 * H);
    ca.set(at(&ca) + 2 * H);
    assert_eq!(b.poll_once().unwrap().len(), 3);
    let now = at(&cb);
    for ts in &delivered_ts(&b)[1..] {
        assert!(now - ts >= 2 * H - 5000 && now - ts <= 2 * H + 5000, "{ts}");
    }
}

#[test]
fn a_backward_clock_correction_looks_stale_for_one_window_then_recovers() {
    let fake = Fake::default();
    let cb = Clock::new(T0);
    let ca = Clock::new(T0 + H);
    let (a, b) = pair2(&fake, &ca, &cb);
    for _ in 0..3 {
        send_at(&a, &ca, "steady");
        b.poll_once().unwrap();
        cb.set(at(&cb) + 5 * MIN);
        ca.set(at(&cb) + H);
    }
    ca.set(at(&cb));
    let mut ages = Vec::new();
    for _ in 0..8 {
        send_at(&a, &ca, "fixed");
        b.poll_once().unwrap();
        ages.push(at(&cb) - *delivered_ts(&b).last().unwrap());
        cb.set(at(&cb) + 5 * MIN);
        ca.set(at(&cb));
    }
    assert!(ages[0] >= H - 50 && ages[0] <= H + 50, "{ages:?}");
    assert!(ages.iter().all(|a| *a <= H + 50), "{ages:?}");
    assert!(*ages.last().unwrap() < 50, "{ages:?}");
}

#[test]
fn sent_at_at_the_far_edge_of_the_window_opens_and_pruning_does_not_panic() {
    let fake = Fake::default();
    let cb = Clock::new(T0);
    let ca = Clock::new(T0);
    let (a, b) = pair2(&fake, &ca, &cb);
    ca.set(T0 + 31 * DAY);
    send_at(&a, &ca, "far");
    assert_eq!(b.poll_once().unwrap().len(), 1);
    cb.set(T0 + 100 * DAY);
    ca.set(T0 + 100 * DAY);
    send_at(&a, &ca, "later");
    assert_eq!(b.poll_once().unwrap().len(), 1);
}

#[test]
fn peer_clock_observe_follows_the_window_rules() {
    let w = CLOCK_WINDOW_MS;
    let mut p = PeerClock::first(100, 1000);
    assert_eq!(
        (p.cur, p.until, p.last, p.next),
        (100, 1000 + w, 1000, None)
    );
    p.observe(100, 2000);
    assert_eq!((p.cur, p.until), (100, 2000 + w));
    p.observe(50, 3000);
    p.observe(70, 4000);
    assert_eq!((p.cur, p.next), (100, Some(70)));
    p.observe(60, 2000 + w);
    assert_eq!((p.cur, p.next), (70, None));
    p.observe(500, 2000 + w + 1);
    assert_eq!((p.cur, p.next, p.until), (500, None, 2000 + w + 1 + w));
}

#[test]
fn peer_clock_resumed_after_idle_keeps_the_old_estimate_until_a_new_window_passes() {
    let w = CLOCK_WINDOW_MS;
    let mut p = PeerClock::first(100, 0);
    p.observe(-7000, 10 * w);
    assert_eq!(
        (p.cur, p.next, p.until, p.last),
        (100, Some(-7000), 11 * w, 10 * w)
    );
    p.observe(-7000, 11 * w);
    assert_eq!(p.cur, -7000);
}

#[test]
fn peer_clock_sample_is_clamped_to_a_day() {
    assert_eq!(PeerClock::sample(T0 + 40 * DAY, T0), DAY as i64);
    assert_eq!(PeerClock::sample(T0, T0 + 40 * DAY), -(DAY as i64));
    assert_eq!(PeerClock::sample(T0 + 5, T0), 5);
    assert_eq!(PeerClock::sample(u64::MAX, 0), DAY as i64);
    assert_eq!(to_local_ms(10, 100), 0);
    assert_eq!(to_local_ms(1000, -5), 1005);
}

#[test]
fn mgr_req_ts_is_converted_and_mgr_res_is_not() {
    let (fake, clock, a, _r1, r2, sink) = rpc_world();
    let ca = Clock::new(T0);
    let r1 = join_as(&fake, &ca, &a, "Runner 3", "runner");
    ca.set(T0 + H);
    for e in [&r1, &r2, &a] {
        e.poll_once().unwrap();
    }
    let sent = at(&ca);
    r1.send_rpc(
        &my_id(&r2),
        "mgr_req",
        &rpc_id("mgr_req"),
        &json!({"op": "x", "ts_ms": sent}),
    )
    .unwrap();
    r1.send_rpc(
        &my_id(&r2),
        "mgr_res",
        &rpc_id("mgr_res"),
        &json!({"ts_ms": sent}),
    )
    .unwrap();
    r2.poll_once().unwrap();
    let got = sink.0.lock().unwrap().clone();
    let ts = |kind: &str| {
        got.iter().find(|g| g.2 == kind).unwrap().4["ts_ms"]
            .as_u64()
            .unwrap()
    };
    assert!(close(ts("mgr_req"), at(&clock)));
    assert_eq!(ts("mgr_res"), sent);
}

fn custom_b(fake: &Fake, clock: &Clock, store: &Arc<MemKeyStore>) -> Engine {
    let c = clock.0.clone();
    RelayEngine::new(
        fake.clone(),
        store.clone(),
        Rec::default(),
        EngineConfig {
            user_id: USER,
            label: "Desktop B".into(),
            local_machine: "mx_local".into(),
        },
    )
    .unwrap()
    .with_clock(move || c.load(Ordering::SeqCst))
}

#[test]
fn an_accepted_message_is_not_redelivered_after_a_restart() {
    let fake = Fake::default();
    let clock = Clock::new(T0);
    let store = Arc::new(MemKeyStore::default());
    let a = engine_with(&fake, &clock, "Desktop A");
    let b = custom_b(&fake, &clock, &store);
    let code = a.open_join_code().unwrap();
    b.join_with_code(&code).unwrap();
    a.poll_once().unwrap();
    a.decide_join(&my_id(&b), true).unwrap();
    b.poll_once().unwrap();
    a.poll_once().unwrap();
    let bid = my_id(&b);
    send_at(&a, &clock, "once");
    let (id, from, body) = fake.first_body(&bid);
    assert_eq!(b.poll_once().unwrap().len(), 1);
    clock.set(T0 + 31_000);
    b.poll_once().unwrap();
    drop(b);
    let b2 = custom_b(&fake, &clock, &store);
    fake.inject(&bid, &id, &from, &body);
    assert!(b2.poll_once().unwrap().is_empty());
    clock.set(T0 + 7 * DAY);
    fake.inject(&bid, &id, &from, &body);
    assert!(b2.poll_once().unwrap().is_empty());
    assert!(history(&b2).is_empty());
}

#[test]
fn a_message_the_sink_refused_is_delivered_exactly_once_on_a_later_poll() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    b.s.fail.store(true, Ordering::SeqCst);
    send(&a, "retry me").unwrap();
    let _ = b.poll_once();
    assert!(history(&b).is_empty());
    b.s.fail.store(false, Ordering::SeqCst);
    assert_eq!(b.poll_once().unwrap().len(), 1);
    assert!(b.poll_once().unwrap().is_empty());
    assert_eq!(history(&b), ["retry me"]);
}

fn filler(n: usize, at: u64) -> Vec<Seen> {
    (0..n)
        .map(|i| Seen {
            from: "zz".into(),
            id: format!("m_fill{i}"),
            at,
        })
        .collect()
}

#[test]
fn a_chat_evicted_by_the_seen_cap_cannot_be_replayed() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let bid = my_id(&b);
    send_at(&a, &clock, "victim");
    let (id, from, body) = fake.first_body(&bid);
    assert_eq!(b.poll_once().unwrap().len(), 1);
    clock.set(T0 + 1000);
    b.lock().st.seen.extend(filler(MAX_SEEN, T0 + 1000));
    send_at(&a, &clock, "next");
    assert_eq!(b.poll_once().unwrap().len(), 1);
    fake.inject(&bid, &id, &from, &body);
    assert!(b.poll_once().unwrap().is_empty());
    assert_eq!(b.s.drops().last(), Some(&"replayed or too old"));
    clock.set(T0 + 2000);
    send_at(&a, &clock, "still works");
    assert_eq!(b.poll_once().unwrap().len(), 1);
}

#[test]
fn an_rpc_evicted_by_the_rpc_seen_cap_cannot_be_replayed() {
    let (fake, clock, _a, r1, r2, sink) = rpc_world();
    let rid = my_id(&r2);
    r1.send_rpc(&rid, "mgr_req", &rpc_id("mgr_req"), &json!({"op": "x"}))
        .unwrap();
    let (id, from, body) = last_body_to(&fake, &rid);
    r2.poll_once().unwrap();
    assert_eq!(sink.n(), 1);
    clock.set(T0 + 1000);
    r2.lock()
        .st
        .rpc_seen
        .extend(filler(MAX_RPC_SEEN, T0 + 1000));
    r1.send_rpc(&rid, "mgr_req", &rpc_id("mgr_req"), &json!({"op": "y"}))
        .unwrap();
    r2.poll_once().unwrap();
    assert_eq!(sink.n(), 2);
    fake.inject(&rid, &id, &from, &body);
    r2.poll_once().unwrap();
    assert_eq!(sink.n(), 2);
}
