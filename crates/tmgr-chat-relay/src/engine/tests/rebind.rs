use super::*;

fn world(fake: &Fake, clock: &Clock) -> (Engine, Engine, Engine, Engine) {
    let (a, b) = pair(fake, clock);
    let r1 = join_as(fake, clock, &a, "Runner 1", "runner");
    let r2 = join_as(fake, clock, &a, "Runner 2", "runner");
    b.poll_once().unwrap();
    let bid = my_id(&b);
    post_chat(&r1, &bid, &msg_n(2, AuthorKind::Manager));
    assert_eq!(b.poll_once().unwrap().len(), 1);
    (a, b, r1, r2)
}

#[test]
fn a_new_runner_takes_the_machine_when_the_old_one_has_a_pending_revoke() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b, r1, r2) = world(&fake, &clock);
    clock.set(T0 + 5_000);
    a.revoke(&my_id(&r1)).unwrap();
    b.poll_once().unwrap();
    assert_eq!(b.trust_state().unwrap().pending_revokes.len(), 1);
    clock.set(T0 + 6_000);
    post_chat(&r2, &my_id(&b), &msg_n(3, AuthorKind::Manager));
    assert_eq!(b.poll_once().unwrap().len(), 1);
    assert!(alert_kinds(&b).is_empty());
    assert_eq!(b.trust_state().unwrap().pending_revokes.len(), 1);
}

#[test]
fn the_old_runner_keeps_the_machine_when_it_speaks_after_the_revoke_arrived() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b, r1, r2) = world(&fake, &clock);
    let bid = my_id(&b);
    post_chat(&r1, &bid, &msg_n(4, AuthorKind::Session));
    let (mid, from, body) = last_body_to(&fake, &bid);
    fake.0.lock().unwrap().msgs.retain(|m| m.msg_id != mid);
    clock.set(T0 + 5_000);
    a.revoke(&my_id(&r1)).unwrap();
    b.poll_once().unwrap();
    clock.set(T0 + 6_000);
    fake.inject(&bid, &mid, &from, &body);
    assert_eq!(b.poll_once().unwrap().len(), 1);
    clock.set(T0 + 7_000);
    post_chat(&r2, &bid, &msg_n(3, AuthorKind::Manager));
    assert!(b.poll_once().unwrap().is_empty());
    assert_eq!(alert_kinds(&b), vec!["room_conflict".to_owned()]);
}

#[test]
fn a_pending_revoke_stored_without_a_receive_time_is_stamped_at_the_next_poll() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b, r1, r2) = world(&fake, &clock);
    clock.set(T0 + 5_000);
    a.revoke(&my_id(&r1)).unwrap();
    b.poll_once().unwrap();
    b.lock().st.pending_revokes[0].received_at_ms = 0;
    clock.set(T0 + 6_000);
    post_chat(&r2, &my_id(&b), &msg_n(3, AuthorKind::Manager));
    assert_eq!(b.poll_once().unwrap().len(), 1);
    assert_eq!(
        b.trust_state().unwrap().pending_revokes[0].received_at_ms,
        T0 + 6_000
    );
}

#[test]
fn the_new_runner_takes_the_machine_when_the_revoke_and_its_chat_arrive_in_one_poll() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b, r1, r2) = world(&fake, &clock);
    clock.set(T0 + 5_000);
    a.revoke(&my_id(&r1)).unwrap();
    post_chat(&r2, &my_id(&b), &msg_n(3, AuthorKind::Manager));
    assert_eq!(b.poll_once().unwrap().len(), 1);
    assert!(alert_kinds(&b).is_empty());
}

#[test]
fn a_runner_bound_to_another_machine_cannot_take_a_superseded_one() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b, r1, r2) = world(&fake, &clock);
    let bid = my_id(&b);
    let mut other = msg_n(5, AuthorKind::Manager);
    other.room = RoomRef::manager("mx_other");
    post_chat(&r2, &bid, &other);
    assert_eq!(b.poll_once().unwrap().len(), 1);
    clock.set(T0 + 5_000);
    a.revoke(&my_id(&r1)).unwrap();
    b.poll_once().unwrap();
    clock.set(T0 + 6_000);
    post_chat(&r2, &bid, &msg_n(3, AuthorKind::Manager));
    assert!(b.poll_once().unwrap().is_empty());
    assert_eq!(alert_kinds(&b), vec!["room_conflict".to_owned()]);
}

#[test]
fn an_applied_revoke_frees_the_machine_for_the_new_runner() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b, r1, r2) = world(&fake, &clock);
    clock.set(T0 + 5_000);
    a.revoke(&my_id(&r1)).unwrap();
    b.poll_once().unwrap();
    let sid = b.trust_state().unwrap().pending_revokes[0]
        .statement_id
        .clone();
    b.decide_revoke(&sid, true).unwrap();
    clock.set(T0 + 6_000);
    post_chat(&r2, &my_id(&b), &msg_n(3, AuthorKind::Manager));
    assert_eq!(b.poll_once().unwrap().len(), 1);
    assert!(alert_kinds(&b).is_empty());
}

#[test]
fn the_old_runner_keeps_the_machine_when_it_speaks_in_the_same_poll_as_a_runner_sorted_before_it() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let r1 = join_as(&fake, &clock, &a, "Runner 1", "runner");
    let r2 = join_as(&fake, &clock, &a, "Runner 2", "runner");
    b.poll_once().unwrap();
    assert!(my_id(&r1) < my_id(&r2));
    let bid = my_id(&b);
    post_chat(&r2, &bid, &msg_n(2, AuthorKind::Manager));
    assert_eq!(b.poll_once().unwrap().len(), 1);
    post_chat(&r2, &bid, &msg_n(4, AuthorKind::Manager));
    let (mid, from, body) = last_body_to(&fake, &bid);
    fake.0.lock().unwrap().msgs.retain(|m| m.msg_id != mid);
    clock.set(T0 + 5_000);
    a.revoke(&my_id(&r2)).unwrap();
    b.poll_once().unwrap();
    clock.set(T0 + 6_000);
    post_chat(&r1, &bid, &msg_n(3, AuthorKind::Manager));
    fake.inject(&bid, &mid, &from, &body);
    b.poll_once().unwrap();
    assert_eq!(alert_kinds(&b), vec!["room_conflict".to_owned()]);
    let bound: Vec<String> = b.lock().st.bindings.values().cloned().collect();
    assert_eq!(bound, vec![my_id(&r2)]);
}
