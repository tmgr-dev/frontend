use std::sync::atomic::{AtomicU64, Ordering};

use super::*;
use crate::chat::Chat;
use tmgr_ipc::chat::RoomRef;

struct Rec(Mutex<Vec<(Value, Vec<String>)>>);

impl Rec {
    fn new() -> Self {
        Rec(Mutex::new(Vec::new()))
    }

    fn count(&self) -> usize {
        self.0.lock().unwrap().len()
    }

    fn take(&self) -> Vec<(Value, Vec<String>)> {
        std::mem::take(&mut *self.0.lock().unwrap())
    }
}

impl ActivitySend for Rec {
    fn send(&self, payload: &Value, only: &[String]) -> Result<(), String> {
        self.0
            .lock()
            .unwrap()
            .push((payload.clone(), only.to_vec()));
        Ok(())
    }
}

struct Rig {
    chat: Chat,
    _dir: tempfile::TempDir,
    now: Arc<AtomicU64>,
    leases: Leases,
    rx: ActivityRx,
    flusher: Flusher,
    failures: FailureLog,
    rec: Rec,
}

impl Rig {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let chat = Chat::open(dir.path());
        chat.set_activity_consent(true);
        let now = Arc::new(AtomicU64::new(1_000));
        let clock = now.clone();
        let leases = Leases::new(Arc::new(move || clock.load(Ordering::SeqCst)));
        let rx = chat.subscribe_activity();
        let flusher = Flusher::new(chat.machine().to_owned());
        Rig {
            chat,
            _dir: dir,
            now,
            leases,
            rx,
            flusher,
            failures: FailureLog::default(),
            rec: Rec::new(),
        }
    }

    fn advance(&self, ms: u64) {
        self.now.fetch_add(ms, Ordering::SeqCst);
    }

    fn tick(&mut self) {
        let now = self.now.clone();
        tick(
            &mut self.flusher,
            &self.rx,
            &self.leases,
            &move || now.load(Ordering::SeqCst),
            &self.rec,
            &mut self.failures,
        );
    }

    fn room(&self) -> RoomRef {
        self.chat.session_room("s_abcdefghij")
    }

    fn publish(&self, state: ActivityState, current: &str) {
        self.chat.publish_activity(&ChatActivity {
            v: 1,
            room: self.room(),
            state,
            current: Some(current.to_owned()),
            used: vec!["Read: a.rs".into()],
            calls: Vec::new(),
        });
    }
}

fn call(n: u32, summary: &str, result: Option<&str>) -> ToolCall {
    ToolCall {
        n,
        name: "Bash".into(),
        summary: summary.into(),
        result: result.map(str::to_owned),
        running: false,
        error: false,
    }
}

fn words(seed: usize, chars: usize) -> String {
    let mut s = format!("w{seed} ");
    while s.len() < chars {
        s.push_str("lorem ipsum ");
    }
    s.truncate(chars);
    s
}

fn big_room(
    machine: &str,
    i: usize,
    used: usize,
    calls: usize,
    result_lines: usize,
) -> ChatActivity {
    ChatActivity {
        v: 1,
        room: RoomRef::session(machine, &format!("s_{i:010}")),
        state: ActivityState::Working,
        current: Some("Bash: cargo".into()),
        used: (0..used)
            .map(|u| format!("Read: {}", words(i * 1000 + u, 150)))
            .collect(),
        calls: (0..calls)
            .map(|c| {
                let r = (0..result_lines)
                    .map(|l| words(c * 100 + l, 190))
                    .collect::<Vec<_>>()
                    .join("\n");
                call(c as u32, &words(c, 150), Some(&r))
            })
            .collect(),
    }
}

fn collect(machine: &str, f: &mut Flusher, leased: &[String], now: u64) -> Vec<(String, Value)> {
    let _ = machine;
    let mut out = Vec::new();
    f.flush(leased, now, &mut |d, p| {
        out.push((d.to_owned(), p.clone()));
        Ok(())
    });
    out
}

#[test]
fn a_lease_lasts_150_seconds_on_the_runner_clock() {
    let now = Arc::new(AtomicU64::new(10));
    let c = now.clone();
    let leases = Leases::new(Arc::new(move || c.load(Ordering::SeqCst)));
    leases.deliver_activity_on("a", true);
    now.store(10 + 149_999, Ordering::SeqCst);
    assert_eq!(leases.active(), ["a"]);
    now.store(10 + 150_000, Ordering::SeqCst);
    assert!(leases.active().is_empty());
}

#[test]
fn renewing_extends_and_off_removes() {
    let now = Arc::new(AtomicU64::new(0));
    let c = now.clone();
    let leases = Leases::new(Arc::new(move || c.load(Ordering::SeqCst)));
    leases.deliver_activity_on("a", true);
    now.store(60_000, Ordering::SeqCst);
    leases.deliver_activity_on("a", true);
    now.store(200_000, Ordering::SeqCst);
    assert_eq!(leases.active(), ["a"]);
    leases.deliver_activity_on("a", false);
    assert!(leases.active().is_empty());
}

#[test]
fn at_most_20_leases_and_a_renewal_still_works_when_full() {
    let leases = Leases::new(Arc::new(|| 0));
    for i in 0..30 {
        leases.deliver_activity_on(&format!("d{i:02}"), true);
    }
    assert_eq!(leases.active().len(), 20);
    assert!(!leases.active().contains(&"d25".to_owned()));
    leases.deliver_activity_on("d03", true);
    assert_eq!(leases.active().len(), 20);
}

#[test]
fn without_a_lease_nothing_is_sent_even_with_100_events() {
    let mut rig = Rig::new();
    for i in 0..100 {
        rig.publish(ActivityState::Working, &format!("Bash: {i}"));
        if i % 10 == 0 {
            rig.tick();
            rig.advance(3_000);
        }
    }
    rig.tick();
    rig.advance(60_000);
    rig.tick();
    assert_eq!(rig.rec.count(), 0);
}

#[test]
fn a_burst_makes_one_envelope_per_leased_device_and_a_new_lease_gets_a_snapshot() {
    let mut rig = Rig::new();
    rig.leases.deliver_activity_on("p1", true);
    rig.leases.deliver_activity_on("p2", true);
    for i in 0..100 {
        rig.publish(ActivityState::Working, &format!("Bash: {i}"));
    }
    rig.tick();
    let sent = rig.rec.take();
    assert_eq!(sent.len(), 2);
    for (p, only) in &sent {
        assert_eq!(only.len(), 1);
        assert_eq!(p["v"], 1);
        assert_eq!(p["machine"], rig.chat.machine());
        assert_eq!(p["snapshot"], true);
        assert_eq!(p["rooms"].as_array().unwrap().len(), 1);
        assert_eq!(p["rooms"][0]["current"], "Bash: 99");
        assert!(p["rooms"][0].get("v").is_none());
    }
    rig.advance(3_000);
    rig.tick();
    assert_eq!(rig.rec.count(), 0);

    rig.publish(ActivityState::Working, "Bash: next");
    rig.advance(3_000);
    rig.leases.deliver_activity_on("p1", true);
    rig.leases.deliver_activity_on("p2", true);
    rig.leases.deliver_activity_on("p3", true);
    rig.tick();
    let sent = rig.rec.take();
    assert_eq!(sent.len(), 3);
    for (p, only) in &sent {
        assert_eq!(p["snapshot"], only[0] == "p3");
    }
}

#[test]
fn keepalive_snapshot_every_30_seconds_only_while_working() {
    let mut rig = Rig::new();
    rig.leases.deliver_activity_on("p1", true);
    rig.publish(ActivityState::Working, "Bash: a");
    rig.tick();
    assert_eq!(rig.rec.take().len(), 1);
    for _ in 0..9 {
        rig.advance(3_000);
        rig.leases.deliver_activity_on("p1", true);
        rig.tick();
    }
    assert_eq!(rig.rec.count(), 0);
    rig.advance(3_000);
    rig.leases.deliver_activity_on("p1", true);
    rig.tick();
    let sent = rig.rec.take();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0["snapshot"], true);
    rig.advance(3_000);
    rig.tick();
    assert_eq!(rig.rec.count(), 0);

    rig.publish(ActivityState::Idle, "");
    rig.advance(3_000);
    rig.tick();
    assert_eq!(rig.rec.take().len(), 1);
    for _ in 0..30 {
        rig.advance(3_000);
        rig.leases.deliver_activity_on("p1", true);
        rig.tick();
    }
    assert_eq!(rig.rec.count(), 0);
}

#[test]
fn blank_idle_is_sent_once_then_forgotten() {
    let mut rig = Rig::new();
    rig.leases.deliver_activity_on("p1", true);
    rig.publish(ActivityState::Working, "Bash: a");
    rig.tick();
    rig.rec.take();
    rig.chat.publish_activity(&ChatActivity {
        v: 1,
        room: rig.room(),
        state: ActivityState::Idle,
        current: None,
        used: Vec::new(),
        calls: Vec::new(),
    });
    rig.advance(3_000);
    rig.tick();
    let sent = rig.rec.take();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].0["rooms"][0]["state"], "idle");
    assert!(sent[0].0["rooms"][0]["current"].is_null());
    rig.leases.deliver_activity_on("p2", true);
    rig.advance(3_000);
    rig.tick();
    assert_eq!(rig.rec.count(), 0);
}

#[test]
fn rooms_of_another_machine_are_never_sent() {
    let mut rig = Rig::new();
    rig.leases.deliver_activity_on("p1", true);
    let other = RoomRef::manager("mx_ffffffffffffffff");
    assert_ne!(other.machine, rig.chat.machine());
    rig.flusher.ingest(vec![ChatActivity {
        v: 1,
        room: other,
        state: ActivityState::Working,
        current: Some("Bash: ls".into()),
        used: Vec::new(),
        calls: Vec::new(),
    }]);
    rig.tick();
    rig.advance(60_000);
    rig.tick();
    assert_eq!(rig.rec.count(), 0);
}

#[test]
fn secrets_and_long_lines_are_masked_and_cut_before_sending() {
    let ghp = format!("ghp_{}", "a1B2".repeat(9));
    let akia = format!("AKIA{}", "ABCD".repeat(4));
    let long = "word ".repeat(100);
    let mut rig = Rig::new();
    rig.leases.deliver_activity_on("p1", true);
    rig.chat.publish_activity(&ChatActivity {
        v: 1,
        room: rig.room(),
        state: ActivityState::Working,
        current: Some(format!("Bash: curl -H token {ghp}")),
        used: vec![format!("Read: {akia}")],
        calls: vec![
            call(
                1,
                &format!("echo {ghp} {akia} password=hunter2"),
                Some(&format!("{ghp}\n{akia}\npassword=hunter2\n{long}")),
            ),
            call(2, &long, None),
        ],
    });
    rig.tick();
    let sent = rig.rec.take();
    let text = sent[0].0.to_string();
    for s in [ghp.as_str(), akia.as_str(), "hunter2"] {
        assert!(!text.contains(s), "{s}");
    }
    let p = &sent[0].0["rooms"][0];
    assert!(p["calls"][1]["summary"].as_str().unwrap().chars().count() <= 200);
    for line in p["calls"][0]["result"].as_str().unwrap().lines() {
        assert!(line.chars().count() <= 200);
    }
}

#[test]
fn re_redaction_is_idempotent_over_secret_fixtures() {
    let samples = [
        [
            "eyJhbGciOiJIUzI1NiJ9",
            "eyJzdWIiOiIxMjM0NTY3ODkwIn0",
            "dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
        ]
        .join("."),
        ["wJalrXUtnFEMI", "K7MDENG", "bPxRfiCYEXAMPLEKEY12"].join("/"),
        "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08".to_owned(),
        format!("ghp_{}", "abcdefghijklmnopqrstuvwxyz0123456789"),
        format!("AKIA{}", "IOSFODNN7EXAMPLE"),
        format!(
            "curl -H 'Authorization: Bearer {}' https://x.io",
            "abcdef1234567890abcdef"
        ),
        "export DB_PASSWORD=hunter2 && cargo test".to_owned(),
    ];

    let long = "word ".repeat(100);
    for s in samples.iter().map(String::as_str).chain([long.as_str()]) {
        for width in [8, 20, s.len()] {
            let text = format!("run token={}", &s[..width.min(s.len())]);
            let once = redact::summary(&text);
            assert_eq!(redact::summary(&once), once, "{text}");
            if let Some(e) = redact::excerpt(&format!("{text}\n{text}")) {
                assert_eq!(redact::excerpt(&e).as_deref(), Some(e.as_str()), "{text}");
            }
        }
    }
}

#[test]
fn an_oversized_payload_drops_results_then_calls_then_splits() {
    let machine = "mx_0123456789abcdef";
    let leased = ["p1".to_owned()];
    let fits = |v: &Value| serde_json::to_vec(v).unwrap().len() <= MAX_ENVELOPE;

    let mut f = Flusher::new(machine.into());
    f.ingest((0..4).map(|i| big_room(machine, i, 3, 10, 8)).collect());
    let sent = collect(machine, &mut f, &leased, 0);
    assert_eq!(sent.len(), 1);
    assert!(fits(&sent[0].1));
    let call0 = &sent[0].1["rooms"][0]["calls"][0];
    assert!(call0.get("result").is_none());
    assert_eq!(sent[0].1["rooms"][0]["calls"].as_array().unwrap().len(), 10);

    let mut f = Flusher::new(machine.into());
    f.ingest((0..60).map(|i| big_room(machine, i, 3, 10, 0)).collect());
    let sent = collect(machine, &mut f, &leased, 0);
    assert_eq!(sent.len(), 1);
    assert!(fits(&sent[0].1));
    assert!(sent[0].1["rooms"][0].get("calls").is_none());

    let mut f = Flusher::new(machine.into());
    f.ingest((0..20).map(|i| big_room(machine, i, 50, 10, 8)).collect());
    let sent = collect(machine, &mut f, &leased, 0);
    assert!(sent.len() > 1);
    let mut rooms = 0;
    for (i, (_, p)) in sent.iter().enumerate() {
        assert!(fits(p));
        assert_eq!(p["snapshot"], i == 0);
        rooms += p["rooms"].as_array().unwrap().len();
    }
    assert_eq!(rooms, 20);
}

#[test]
fn only_the_last_10_calls_and_50_labels_are_kept() {
    let machine = "mx_0123456789abcdef";
    let mut f = Flusher::new(machine.into());
    f.ingest(vec![big_room(machine, 1, 80, 25, 0)]);
    let sent = collect(machine, &mut f, &["p1".into()], 0);
    let r = &sent[0].1["rooms"][0];
    assert_eq!(r["used"].as_array().unwrap().len(), 50);
    let calls = r["calls"].as_array().unwrap();
    assert_eq!(calls.len(), 10);
    assert_eq!(calls[9]["n"], 24);
}

#[test]
fn revoked_consent_clears_phones_and_a_new_lease_gets_no_stale_state() {
    let mut rig = Rig::new();
    rig.leases.deliver_activity_on("p1", true);
    rig.publish(ActivityState::Working, "Bash: secret work");
    rig.tick();
    rig.rec.take();

    rig.chat.set_activity_consent(false);
    rig.advance(3_000);
    rig.tick();
    let sent = rig.rec.take();
    assert_eq!(sent.len(), 1);
    let room = &sent[0].0["rooms"][0];
    assert_eq!(room["state"], "idle");
    assert!(room["current"].is_null());

    rig.leases.deliver_activity_on("p2", true);
    rig.advance(3_000);
    rig.tick();
    assert_eq!(rig.rec.count(), 0);
}
