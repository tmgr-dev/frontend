use super::*;

fn open_last(fake: &Fake, to: &Engine) -> Opened {
    let (msg_id, from, body) = last_body_to(fake, &my_id(to));
    match to.open_one(
        &to.lock(),
        &Msg {
            cursor: 1,
            msg_id,
            from,
            body,
            created_at: Value::Null,
        },
        T0,
    ) {
        Ok(Inbound::Opened(o)) => o,
        _ => panic!("not opened"),
    }
}

fn orig() -> String {
    new_message_id()
}

#[test]
fn a_receipt_reaches_only_the_sending_device_without_push_and_decodes_to_the_contract() {
    let (fake, _c, runner, desktop, mobile, _other) = rooms_world();
    let (did, mid) = (my_id(&desktop), my_id(&mobile));
    let mbefore = fake.queue_len(&mid);
    let posts = fake.posts();
    let id = orig();
    runner
        .send_receipt(
            &did,
            &id,
            ReceiptState::Failed,
            Some("older than 10 minutes."),
        )
        .unwrap();
    assert_eq!(fake.posts(), posts + 1);
    assert_eq!(fake.queue_len(&mid), mbefore);
    assert_eq!(fake.0.lock().unwrap().pushes.last(), Some(&false));
    let o = open_last(&fake, &desktop);
    assert_eq!(o.kind, "receipt");
    assert_eq!(o.id.len(), 29);
    assert!(o.id.starts_with("rc_") && o.id.ends_with("_f"));
    assert_eq!(
        o.payload,
        json!({"id": id, "state": "failed", "reason": "older than 10 minutes."})
    );
}

#[test]
fn a_queued_receipt_carries_its_reason_on_the_wire() {
    let (fake, _c, runner, desktop, _m, _o) = rooms_world();
    let id = orig();
    runner
        .send_receipt(&my_id(&desktop), &id, ReceiptState::Queued, Some("turn"))
        .unwrap();
    let o = open_last(&fake, &desktop);
    assert_eq!(
        o.payload,
        json!({"id": id, "state": "queued", "reason": "turn"})
    );
    runner
        .send_receipt(&my_id(&desktop), &id, ReceiptState::Delivered, Some("turn"))
        .unwrap();
    assert_eq!(
        open_last(&fake, &desktop).payload,
        json!({"id": id, "state": "delivered"})
    );
}

#[test]
fn receipt_ids_differ_per_state_and_per_message_and_pass_the_wire_check() {
    let (fake, _c, runner, desktop, _m, _o) = rooms_world();
    let did = my_id(&desktop);
    let (a, b) = (orig(), orig());
    let mut ids = Vec::new();
    for (id, state) in [
        (&a, ReceiptState::Queued),
        (&a, ReceiptState::Delivered),
        (&b, ReceiptState::Delivered),
    ] {
        runner.send_receipt(&did, id, state, None).unwrap();
        let o = open_last(&fake, &desktop);
        crate::fields::check_msg_id(&o.id).unwrap();
        ids.push(o.id);
    }
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), 3);
}

#[test]
fn only_a_failed_receipt_carries_a_reason_and_it_is_cut_to_200_chars() {
    let (fake, _c, runner, desktop, _m, _o) = rooms_world();
    let did = my_id(&desktop);
    let id = orig();
    runner
        .send_receipt(&did, &id, ReceiptState::Delivered, Some("because"))
        .unwrap();
    assert_eq!(
        open_last(&fake, &desktop).payload,
        json!({"id": id, "state": "delivered"})
    );
    runner
        .send_receipt(&did, &id, ReceiptState::Failed, Some(&"я".repeat(500)))
        .unwrap();
    let p = open_last(&fake, &desktop).payload;
    assert_eq!(p["reason"].as_str().unwrap().chars().count(), 200);
    assert_eq!(p["state"], "failed");
}

#[test]
fn send_receipt_refuses_runners_unknown_devices_self_and_bad_ids() {
    let (fake, _c, runner, desktop, mobile, other) = rooms_world();
    let (did, mid, oid) = (my_id(&desktop), my_id(&mobile), my_id(&other));
    let posts = fake.posts();
    let id = orig();
    assert!(runner
        .send_receipt(&oid, &id, ReceiptState::Delivered, None)
        .is_err());
    assert!(runner
        .send_receipt(&my_id(&runner), &id, ReceiptState::Delivered, None)
        .is_err());
    assert!(runner
        .send_receipt(
            "00000000-0000-0000-0000-000000000000",
            &id,
            ReceiptState::Delivered,
            None
        )
        .is_err());
    assert!(runner
        .send_receipt(&did, "m_short", ReceiptState::Delivered, None)
        .is_err());
    assert!(runner
        .send_receipt(&did, "r_abc", ReceiptState::Delivered, None)
        .is_err());
    assert_eq!(fake.posts(), posts);
    runner
        .send_receipt(&did, &id, ReceiptState::Queued, None)
        .unwrap();
    runner
        .send_receipt(&mid, &id, ReceiptState::Queued, None)
        .unwrap();
    assert_eq!(fake.posts(), posts + 2);
}

#[test]
fn a_receipt_never_contains_message_text() {
    let (fake, _c, runner, desktop, _m, _o) = rooms_world();
    let id = orig();
    runner
        .send_receipt(&my_id(&desktop), &id, ReceiptState::Delivered, None)
        .unwrap();
    let o = open_last(&fake, &desktop);
    assert_eq!(o.payload.as_object().unwrap().len(), 2);
}

#[test]
fn an_unknown_receipt_kind_is_ignored_without_alert_or_hold_and_the_next_message_is_not_delayed() {
    let (fake, clock, runner, desktop, _m, _o) = rooms_world();
    let did = my_id(&desktop);
    desktop.poll_once().unwrap();
    let rid = my_id(&runner);
    let before = desktop.lock().st.seq_in.get(&rid).copied().unwrap_or(0);
    let alerts = alert_kinds(&desktop);
    runner
        .send_receipt(&did, &orig(), ReceiptState::Queued, None)
        .unwrap();
    runner
        .send_receipt(&did, &orig(), ReceiptState::Delivered, None)
        .unwrap();
    assert_eq!(desktop.poll_once().unwrap().len(), 0);
    assert_eq!(
        desktop.lock().st.seq_in.get(&rid).copied(),
        Some(before + 2)
    );
    clock.set(T0 + WRITE_EVERY_MS);
    desktop.poll_once().unwrap();
    assert_eq!(fake.queue_len(&did), 0);
    assert_eq!(alert_kinds(&desktop), alerts);
    assert!(desktop.trust_state().unwrap().seq_gaps.is_empty());
    assert!(desktop.s.drops().is_empty());
}

fn sent_world() -> (Engine, Engine, Engine, Engine, String) {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let a = engine_with(&fake, &clock, "Desktop A");
    let phone = join_as(&fake, &clock, &a, "Phone", "mobile");
    let r1 = join_as(&fake, &clock, &a, "Runner 1", "runner");
    let r2 = join_as(&fake, &clock, &a, "Runner 2", "runner");
    post_chat(&r1, &my_id(&a), &msg_n(2, AuthorKind::Manager));
    assert_eq!(a.poll_once().unwrap().len(), 1);
    let mut out = text_msg(new_message_id(), "hello");
    out.room = msg_n(2, AuthorKind::Manager).room;
    a.send_chat_for_machine(&out, true).unwrap();
    (a, phone, r1, r2, out.id)
}

#[test]
fn a_failed_receipt_from_the_bound_runner_for_a_sent_message_is_exposed_once() {
    let (a, _p, r1, _r2, id) = sent_world();
    r1.send_receipt(&my_id(&a), &id, ReceiptState::Failed, Some("busy"))
        .unwrap();
    assert_eq!(a.poll_once().unwrap().len(), 0);
    assert_eq!(
        a.take_receipts(),
        vec![Receipt {
            id,
            state: ReceiptState::Failed,
            reason: Some("busy".into())
        }]
    );
    assert!(a.take_receipts().is_empty());
}

#[test]
fn receipts_from_other_devices_or_for_unsent_messages_are_ignored_without_a_drop() {
    let (a, p, r1, r2, id) = sent_world();
    let aid = my_id(&a);
    p.send_receipt(&aid, &id, ReceiptState::Failed, Some("x"))
        .unwrap();
    r2.send_receipt(&aid, &id, ReceiptState::Failed, Some("x"))
        .unwrap();
    r1.send_receipt(&aid, &orig(), ReceiptState::Failed, Some("x"))
        .unwrap();
    assert_eq!(a.poll_once().unwrap().len(), 0);
    assert!(a.take_receipts().is_empty());
    assert!(a.s.drops().is_empty());
}
