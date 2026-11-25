use super::*;

fn probe_world() -> (Fake, Clock, Engine, Engine) {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let o = engine_with(&fake, &clock, "Owner D");
    let runner = join_as(&fake, &clock, &o, "Runner 1", "runner").with_room_scope(RoomScope::Local);
    (fake, clock, o, runner)
}

fn delivered_probe(sender: &Engine, runner: &Engine) -> Vec<Receipt> {
    let sent = sender.send_probes().unwrap();
    assert_eq!(sent.len(), 1);
    runner.poll_once().unwrap();
    sender.poll_once().unwrap();
    let got = sender.take_receipts();
    assert!(got.iter().all(|r| r.id == sent[0].1));
    got
}

#[test]
fn a_trusted_desktop_mobile_or_terminal_gets_an_ok_probe_receipt() {
    let (fake, clock, o, runner) = probe_world();
    let desktop = join_as(&fake, &clock, &o, "Desk", "desktop");
    let mobile = join_as(&fake, &clock, &o, "Phone", "mobile");
    let term = join_as(&fake, &clock, &o, "Term", "terminal");
    for e in [&desktop, &mobile, &term, &o] {
        runner.poll_once().unwrap();
        e.poll_once().unwrap();
        let got = delivered_probe(e, &runner);
        assert_eq!(
            got,
            vec![Receipt {
                id: got[0].id.clone(),
                state: ReceiptState::Delivered,
                reason: None
            }]
        );
    }
}

#[test]
fn a_probe_never_reaches_the_chat_sink_of_the_runner() {
    let (fake, clock, o, runner) = probe_world();
    let term = join_as(&fake, &clock, &o, "Term", "terminal");
    runner.poll_once().unwrap();
    term.poll_once().unwrap();
    let before = (runner.s.bodies(), runner.s.drops());
    delivered_probe(&term, &runner);
    assert_eq!((runner.s.bodies(), runner.s.drops()), before);
    assert!(runner.s.got.lock().unwrap().is_empty());
}

#[test]
fn only_owner_devices_can_send_a_probe_and_only_runners_answer() {
    let (fake, clock, o, runner) = probe_world();
    assert!(runner.send_probes().is_err());
    let other = join_as(&fake, &clock, &o, "Runner 2", "runner");
    assert!(other.send_probes().is_err());
    assert!(o.send_probes().is_ok());
    let lone = join_as(
        &fake,
        &clock,
        &engine_with(&fake, &clock, "Solo"),
        "T",
        "terminal",
    );
    assert!(lone.send_probes().is_err());
}

fn forged_probe(from: &Engine, to: &Engine) -> String {
    let id = new_message_id();
    let mut g = from.lock();
    let dev =
        g.st.trusted
            .iter()
            .find(|d| d.id == my_id(to))
            .cloned()
            .unwrap();
    from.seal_and_post(&mut g, "probe", &id, &json!({"v": 1}), &[dev], false)
        .unwrap();
    id
}

#[test]
fn a_runner_sender_gets_no_receipt_for_a_probe() {
    let (fake, clock, o, runner) = probe_world();
    let other = join_as(&fake, &clock, &o, "Runner 2", "runner").with_room_scope(RoomScope::Local);
    runner.poll_once().unwrap();
    other.poll_once().unwrap();
    let posts = fake.posts();
    forged_probe(&other, &runner);
    let after_probe = fake.posts();
    runner.poll_once().unwrap();
    assert_eq!(fake.posts(), after_probe);
    assert!(posts < after_probe);
    other.poll_once().unwrap();
    assert!(other.take_receipts().is_empty());
    assert!(runner.s.got.lock().unwrap().is_empty());
}

#[test]
fn a_probe_from_a_device_with_a_pending_revoke_gets_no_receipt() {
    let (fake, clock, o, runner) = probe_world();
    let term = join_as(&fake, &clock, &o, "Term", "terminal");
    runner.poll_once().unwrap();
    term.poll_once().unwrap();
    let tfp = fp_of(&term);
    {
        let mut g = runner.lock();
        g.st.pending_revokes.push(PendingRevoke {
            statement_id: "s".into(),
            revoker_label: "r".into(),
            revoker_fp: "r".repeat(64),
            target_id: my_id(&term),
            target_label: "Term".into(),
            target_fp: tfp,
            received_at_ms: 0,
            issued_at_ms: 0,
        });
    }
    let ids = term.send_probes().unwrap();
    runner.poll_once().unwrap();
    term.poll_once().unwrap();
    assert!(ids.len() == 1 && term.take_receipts().is_empty());
}

#[test]
fn a_probe_is_answered_once_even_if_the_same_message_is_replayed() {
    let (fake, clock, o, runner) = probe_world();
    let term = join_as(&fake, &clock, &o, "Term", "terminal");
    runner.poll_once().unwrap();
    term.poll_once().unwrap();
    let before = fake.posts();
    term.send_probes().unwrap();
    runner.poll_once().unwrap();
    runner.poll_once().unwrap();
    assert_eq!(fake.posts(), before + 2);
}

#[test]
fn an_unknown_device_gets_no_receipt_for_a_probe() {
    let (fake, clock, _o, runner) = probe_world();
    let (stranger, _, _) = registered(&fake, &clock, "Stranger", "terminal");
    let id = new_message_id();
    {
        let mut g = stranger.lock();
        let dir = stranger.fetch_dir(&mut g).unwrap();
        let entry = dir
            .devices
            .iter()
            .find(|d| d.device_id == my_id(&runner))
            .unwrap();
        let dev = dev_from(entry);
        stranger
            .seal_and_post(&mut g, "probe", &id, &json!({"v": 1}), &[dev], false)
            .unwrap();
    }
    let posts = fake.posts();
    runner.poll_once().unwrap();
    assert_eq!(fake.posts(), posts);
    assert!(runner.s.got.lock().unwrap().is_empty());
}

#[test]
fn a_sink_that_refuses_owner_input_never_answers_a_probe() {
    let (fake, clock, o, runner) = probe_world();
    let term = join_as(&fake, &clock, &o, "Term", "terminal");
    runner.poll_once().unwrap();
    term.poll_once().unwrap();
    runner.s.refuse_probes.store(true, Ordering::SeqCst);
    term.send_probes().unwrap();
    runner.poll_once().unwrap();
    term.poll_once().unwrap();
    assert!(term.take_receipts().is_empty());
}

#[test]
fn the_probe_path_asks_the_sink_with_the_kind_chat_delivery_uses() {
    let (fake, clock, o, runner) = probe_world();
    let term = join_as(&fake, &clock, &o, "Term", "terminal");
    runner.poll_once().unwrap();
    term.poll_once().unwrap();
    delivered_probe(&term, &runner);
    assert_eq!(
        *runner.s.probe_kinds.lock().unwrap(),
        [Some("terminal".to_owned())]
    );
    let tfp = fp_of(&term);
    {
        let mut g = runner.lock();
        g.st.pending_revokes.push(PendingRevoke {
            statement_id: "s".into(),
            revoker_label: "r".into(),
            revoker_fp: "r".repeat(64),
            target_id: my_id(&term),
            target_label: "Term".into(),
            target_fp: tfp,
            received_at_ms: 0,
            issued_at_ms: 0,
        });
    }
    runner.s.probe_kinds.lock().unwrap().clear();
    term.send_probes().unwrap();
    runner.poll_once().unwrap();
    term.poll_once().unwrap();
    assert!(term.take_receipts().is_empty());
    assert_eq!(*runner.s.probe_kinds.lock().unwrap(), [None]);
}

#[test]
fn a_probe_flood_from_one_sender_cannot_crowd_out_another_sender() {
    let (_fake, _clock, o, runner) = probe_world();
    let mut g = runner.lock();
    for n in 0..100 {
        push_probe(&mut g, "flooder".into(), format!("p{n}"));
    }
    push_probe(&mut g, my_id(&o), "mine".into());
    assert!(g.probes.iter().any(|p| p.1 == "mine"));
    assert_eq!(g.probes.iter().filter(|p| p.0 == "flooder").count(), 8);
}

#[test]
fn a_probe_from_a_sender_that_is_no_longer_trusted_is_dropped_not_requeued() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (_a, b) = pair(&fake, &clock);
    {
        let mut g = b.lock();
        push_probe(&mut g, "gone".into(), "p1".into());
    }
    b.poll_once().unwrap();
    assert!(b.lock().probes.is_empty());
}

#[test]
fn a_probe_answer_is_requeued_on_a_transient_error_and_dropped_on_a_permanent_one() {
    let (fake, clock) = (Fake::default(), Clock::new(T0));
    let (a, b) = pair(&fake, &clock);
    let pid = new_message_id();
    for (error, kept) in [
        ("relay unreachable", 1),
        (r#"{"error":"forbidden","http_status":403}"#, 0),
    ] {
        b.lock().probes.clear();
        {
            let mut g = b.lock();
            push_probe(&mut g, my_id(&a), pid.clone());
        }
        fake.0.lock().unwrap().post_error = Some(error.into());
        let _ = b.poll_once();
        assert_eq!(b.lock().probes.len(), kept, "{error}");
    }
}
