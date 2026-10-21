use super::tail::{Tailer, REPLY_CAP};
use super::*;
use crate::attach::Format;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;

fn private_tmp() -> tempfile::TempDir {
    let t = tempfile::tempdir().unwrap();
    std::fs::set_permissions(t.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
    t
}

const U: &str = "2f0e1c3a-5b7d-4e8f-9a1b-3c4d5e6f7a8b";
const SID: &str = "s_abcdefghjk";

fn user(text: &str) -> String {
    serde_json::json!({"type":"user","message":{"role":"user","content":text}}).to_string()
}

fn agent(text: &str) -> String {
    serde_json::json!({"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":text}]}}).to_string()
}

fn append(path: &Path, lines: &[String]) {
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap();
    for l in lines {
        writeln!(f, "{l}").unwrap();
    }
}

use std::path::Path;

fn tailer(dir: &Path, name: &str, resume: Option<u64>) -> (Tailer, std::path::PathBuf) {
    let path = dir.join(name);
    (
        Tailer::new(Format::Claude, dir.to_path_buf(), path.clone(), resume),
        path,
    )
}

#[test]
fn a_turn_is_emitted_once_when_settled() {
    let d = private_tmp();
    let (mut t, p) = tailer(d.path(), "a.jsonl", None);
    append(
        &p,
        &[
            user("do it"),
            agent("Done."),
            agent("Which flag do you prefer?"),
        ],
    );
    assert_eq!(t.poll(false), None);
    let turn = t.poll(true).unwrap();
    assert_eq!(turn.start, 0);
    assert_eq!(turn.user, "do it");
    assert_eq!(turn.text, "Done.\n\nWhich flag do you prefer?");
    assert_eq!(t.poll(true), None);
    assert_eq!(t.resume_offset(), std::fs::metadata(&p).unwrap().len());
}

#[test]
fn a_human_reply_drops_the_open_turn() {
    let d = private_tmp();
    let (mut t, p) = tailer(d.path(), "a.jsonl", None);
    append(
        &p,
        &[
            user("go"),
            agent("Which one?"),
            user("the first"),
            agent("ok"),
        ],
    );
    let turn = t.poll(true).unwrap();
    assert_eq!(turn.text, "ok");
    assert_eq!(turn.user, "the first");
}

#[test]
fn sidechains_tool_results_meta_and_blobs_are_ignored() {
    let d = private_tmp();
    let (mut t, p) = tailer(d.path(), "a.jsonl", None);
    let side = serde_json::json!({"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"sub?"}]}}).to_string();
    let result = serde_json::json!({"type":"user","message":{"content":[{"type":"tool_result","content":"x"}]}}).to_string();
    let meta =
        serde_json::json!({"type":"user","isMeta":true,"message":{"content":"meta"}}).to_string();
    let blob = serde_json::json!({"type":"api-request-blob","message":{"role":"assistant","content":[{"type":"text","text":"blob?"}]}}).to_string();
    append(&p, &[user("go"), side, result, meta, blob, agent("final")]);
    let turn = t.poll(true).unwrap();
    assert_eq!((turn.user.as_str(), turn.text.as_str()), ("go", "final"));
}

#[test]
fn ask_user_question_tool_calls_are_collected() {
    let d = private_tmp();
    let (mut t, p) = tailer(d.path(), "a.jsonl", None);
    let ask = serde_json::json!({"type":"assistant","message":{"content":[{"type":"tool_use","name":"AskUserQuestion","input":{"questions":[{"question":"Pick a\nstorage engine?"}]}}]}}).to_string();
    append(&p, &[user("go"), ask]);
    assert_eq!(t.poll(true).unwrap().asked, vec!["Pick a storage engine?"]);
}

#[test]
fn partial_lines_wait_for_their_newline() {
    let d = private_tmp();
    let (mut t, p) = tailer(d.path(), "a.jsonl", None);
    append(&p, &[user("go")]);
    let line = agent("Is it ok?");
    let (a, b) = line.split_at(20);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&p)
        .unwrap()
        .write_all(a.as_bytes())
        .unwrap();
    assert_eq!(t.poll(true), None);
    std::fs::OpenOptions::new()
        .append(true)
        .open(&p)
        .unwrap()
        .write_all(format!("{b}\n").as_bytes())
        .unwrap();
    assert_eq!(t.poll(true).unwrap().text, "Is it ok?");
}

#[test]
fn resuming_from_an_offset_rebuilds_the_open_turn() {
    let d = private_tmp();
    let (mut t, p) = tailer(d.path(), "a.jsonl", None);
    append(
        &p,
        &[user("one"), agent("a1"), user("two"), agent("Which?")],
    );
    assert_eq!(t.poll(false), None);
    let off = t.resume_offset();
    assert_eq!(off, (user("one").len() + agent("a1").len() + 2) as u64);
    let (mut again, _) = tailer(d.path(), "a.jsonl", Some(off));
    let turn = again.poll(true).unwrap();
    assert_eq!((turn.user.as_str(), turn.text.as_str()), ("two", "Which?"));
}

#[test]
fn a_big_file_starts_from_the_tail_and_emits_only_the_last_turn() {
    let d = private_tmp();
    let (mut t, p) = tailer(d.path(), "a.jsonl", None);
    let filler = "x".repeat(8000);
    let mut lines = Vec::new();
    for i in 0..300 {
        lines.push(user(&format!("q{i}")));
        lines.push(agent(&filler));
    }
    lines.push(user("last"));
    lines.push(agent("Want more?"));
    append(&p, &lines);
    let turn = t.poll(true).unwrap();
    assert_eq!(
        (turn.user.as_str(), turn.text.as_str()),
        ("last", "Want more?")
    );
}

#[test]
fn a_rewritten_file_starts_over() {
    let d = private_tmp();
    let (mut t, p) = tailer(d.path(), "a.jsonl", None);
    append(
        &p,
        &[
            user("one"),
            agent("first long answer that is longer than the next one"),
        ],
    );
    assert!(t.poll(true).is_some());
    std::fs::remove_file(&p).unwrap();
    append(&p, &[user("n"), agent("new?")]);
    assert_eq!(t.poll(true).unwrap().text, "new?");
}

#[test]
fn reply_text_is_capped() {
    let d = private_tmp();
    let (mut t, p) = tailer(d.path(), "a.jsonl", None);
    append(&p, &[user("go"), agent(&"word ".repeat(5000))]);
    assert!(t.poll(true).unwrap().text.chars().count() <= REPLY_CAP);
}

#[test]
fn codex_rollouts_are_read() {
    let d = private_tmp();
    let p = d.path().join("rollout.jsonl");
    let human =
        serde_json::json!({"type":"event_msg","payload":{"type":"user_message","message":"hi"}})
            .to_string();
    let ai = serde_json::json!({"type":"response_item","payload":{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Shall I go on?"}]}}).to_string();
    let dev = serde_json::json!({"type":"response_item","payload":{"type":"message","role":"developer","content":[{"type":"input_text","text":"rules?"}]}}).to_string();
    append(&p, &[human, dev, ai]);
    let mut t = Tailer::new(Format::Codex, d.path().to_path_buf(), p, None);
    let turn = t.poll(true).unwrap();
    assert_eq!(
        (turn.user.as_str(), turn.text.as_str()),
        ("hi", "Shall I go on?")
    );
}

#[test]
fn codex_rollouts_are_found_by_id_or_folder() {
    let d = private_tmp();
    let day = d.path().join(".codex/sessions/2026/10/20");
    std::fs::create_dir_all(&day).unwrap();
    let a = day.join(format!("rollout-2026-10-20T10-00-00-{U}.jsonl"));
    let b = day.join("rollout-2026-10-20T11-00-00-0199a213-81c0-7800-8aa1-bbab2a035a53.jsonl");
    append(
        &a,
        &[serde_json::json!({"type":"session_meta","payload":{"cwd":"/work/a"}}).to_string()],
    );
    append(
        &b,
        &[serde_json::json!({"type":"session_meta","payload":{"cwd":"/work/b"}}).to_string()],
    );
    assert_eq!(
        tail::locate_codex_rollout(d.path(), Some(U), "/x"),
        Some(a.clone())
    );
    assert_eq!(
        tail::locate_codex_rollout(d.path(), None, "/work/b"),
        Some(b)
    );
    assert_eq!(
        tail::locate_codex_rollout(d.path(), None, "/work/none"),
        None
    );
}

#[test]
fn store_round_trip_marks_and_prefixes() {
    let d = private_tmp();
    let store = Store::new(d.path());
    let q = |id: &str, t: u64| Question {
        id: id.into(),
        session: SID.into(),
        text: format!("question {id}?"),
        asked_s: t,
        origin: Origin::Haiku,
        user: "u".into(),
        reply: "r".into(),
        item: String::new(),
        pos: 0,
        turn: 0,
    };
    let file = SessionFile {
        transcript: "/t".into(),
        offset: 7,
        questions: vec![
            q("aaaa11112222", 100),
            q("aaaa33334444", 200),
            q("bbbb55556666", 300),
        ],
    };
    store.save(SID, &file, 400).unwrap();
    assert_eq!(store.load(SID), file);
    store.mark("aaaa11112222", Status::Ignored, 401).unwrap();
    store.mark("aaaa11112222", Status::Open, 402).unwrap();
    store.mark("bbbb55556666", Status::Answered, 403).unwrap();
    let got = store.entries(None);
    let ids: Vec<_> = got.iter().map(|e| (e.q.id.as_str(), e.status)).collect();
    assert_eq!(
        ids,
        vec![
            ("bbbb55556666", Status::Answered),
            ("aaaa33334444", Status::Open),
            ("aaaa11112222", Status::Open)
        ]
    );
    assert!(store.find("aaa").is_err());
    assert!(store.find("aaaa").unwrap_err().contains("several"));
    assert_eq!(store.find("bbbb").unwrap().q.id, "bbbb55556666");
    assert!(store.find("zzzz").is_err());
}

#[test]
fn store_rejects_bad_session_ids_and_symlinks_and_caps_size() {
    let d = private_tmp();
    let store = Store::new(d.path());
    assert!(store.save("../evil", &SessionFile::default(), 1).is_err());
    assert_eq!(store.load("../evil"), SessionFile::default());
    let mut file = SessionFile::default();
    for i in 0..(MAX_PER_SESSION + 20) {
        file.questions.push(Question {
            id: format!("{i:012}"),
            session: SID.into(),
            text: "q?".into(),
            asked_s: 1000 + i as u64,
            origin: Origin::Rule,
            user: String::new(),
            reply: String::new(),
            item: String::new(),
            pos: 0,
            turn: 0,
        });
    }
    store.save(SID, &file, 2000).unwrap();
    let loaded = store.load(SID);
    assert_eq!(loaded.questions.len(), MAX_PER_SESSION);
    assert_eq!(loaded.questions[0].asked_s, 1020);
    let target = d.path().join("elsewhere.json");
    std::fs::write(&target, "{}").unwrap();
    let link = store.dir().join("s_linkedsess.json");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert_eq!(store.load("s_linkedsess"), SessionFile::default());
}

#[test]
fn old_questions_expire() {
    let d = private_tmp();
    let store = Store::new(d.path());
    let mut file = SessionFile::default();
    file.questions.push(Question {
        id: "000000000001".into(),
        session: SID.into(),
        text: "old?".into(),
        asked_s: 10,
        origin: Origin::Rule,
        user: String::new(),
        reply: String::new(),
        item: String::new(),
        pos: 0,
        turn: 0,
    });
    store.save(SID, &file, 10 + 31 * 24 * 3600).unwrap();
    assert!(store.load(SID).questions.is_empty());
}

#[test]
fn settings_default_to_haiku_on() {
    let d = private_tmp();
    let store = Store::new(d.path());
    assert!(store.settings().haiku);
    store.save_settings(Settings { haiku: false }).unwrap();
    assert!(!store.settings().haiku);
}

#[test]
fn only_one_scanner_holds_the_lock() {
    let d = private_tmp();
    let store = Store::new(d.path());
    let held = store.try_lock().unwrap().unwrap();
    assert!(store.try_lock().unwrap().is_none());
    drop(held);
    assert!(store.try_lock().unwrap().is_some());
}

fn fake(dir: &Path, body: &str) -> std::path::PathBuf {
    let src = dir.join("claude.src");
    std::fs::write(&src, format!("#!/bin/sh\n{body}\n")).unwrap();
    let bin = dir.join("claude");
    let copied = Command::new("cp").arg(&src).arg(&bin).status().unwrap();
    assert!(copied.success());
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

struct Rig {
    runner: tempfile::TempDir,
    home: tempfile::TempDir,
    bin: tempfile::TempDir,
    work: tempfile::TempDir,
    transcript: std::path::PathBuf,
}

impl Rig {
    fn new() -> Rig {
        let home = private_tmp();
        let proj = home.path().join(".claude/projects/-work-x");
        std::fs::create_dir_all(&proj).unwrap();
        Rig {
            runner: private_tmp(),
            transcript: proj.join(format!("{U}.jsonl")),
            home,
            bin: private_tmp(),
            work: private_tmp(),
        }
    }

    fn scanner(&self, claude: Option<std::path::PathBuf>) -> Scanner {
        Scanner::new(
            Store::new(self.runner.path()),
            claude,
            self.work.path().to_path_buf(),
        )
    }

    fn src(&self, settled: bool) -> SessionSrc {
        SessionSrc {
            id: SID.into(),
            format: Format::Claude,
            home: self.home.path().to_path_buf(),
            cwd: "/work/x".into(),
            pid: 0,
            log_id: Some(U.into()),
            settled,
        }
    }

    fn texts(&self) -> Vec<String> {
        Store::new(self.runner.path())
            .entries(None)
            .into_iter()
            .map(|e| e.q.text)
            .collect()
    }
}

#[test]
fn rules_alone_find_questions_when_haiku_is_off_or_missing() {
    let rig = Rig::new();
    append(
        &rig.transcript,
        &[user("go"), agent("Done. Should I also update the docs?")],
    );
    let mut s = rig.scanner(None);
    assert!(!s.scan(&[rig.src(false)], 1000));
    assert!(rig.texts().is_empty());
    assert!(s.scan(&[rig.src(true)], 1000));
    assert_eq!(rig.texts(), vec!["Should I also update the docs?"]);
    assert!(!s.scan(&[rig.src(true)], 1001));
    let e = &Store::new(rig.runner.path()).entries(None)[0];
    assert_eq!((e.q.origin, e.q.user.as_str()), (Origin::Rule, "go"));
}

#[test]
fn nothing_is_asked_of_haiku_without_a_candidate() {
    let rig = Rig::new();
    let bin = fake(
        rig.bin.path(),
        "echo called >> /dev/null\ntouch called.txt\ncat >/dev/null\necho '{\"questions\":[]}'",
    );
    append(
        &rig.transcript,
        &[user("go"), agent("All done, tests pass.")],
    );
    let mut s = rig.scanner(Some(bin));
    s.scan(&[rig.src(true)], 1000);
    assert!(rig.texts().is_empty());
    assert!(!rig.work.path().join("called.txt").exists());
}

#[test]
fn haiku_narrows_the_rule_candidates() {
    let rig = Rig::new();
    let bin = fake(
        rig.bin.path(),
        "cat >/dev/null\necho '{\"questions\":[\"Should I also update the docs?\"]}'",
    );
    append(
        &rig.transcript,
        &[
            user("go"),
            agent("Why does this work? Nobody knows. Should I also update the docs?"),
        ],
    );
    let mut s = rig.scanner(Some(bin));
    s.scan(&[rig.src(true)], 1000);
    let entries = Store::new(rig.runner.path()).entries(None);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].q.origin, Origin::Haiku);
}

#[test]
fn a_failing_haiku_falls_back_to_rules_and_backs_off() {
    let rig = Rig::new();
    let bin = fake(
        rig.bin.path(),
        "echo x >> calls.txt\ncat >/dev/null\nexit 1",
    );
    append(
        &rig.transcript,
        &[user("go"), agent("Should I also update the docs?")],
    );
    let mut s = rig.scanner(Some(bin));
    s.scan(&[rig.src(true)], 1000);
    assert_eq!(rig.texts().len(), 1);
    append(
        &rig.transcript,
        &[user("no"), agent("Do you want a changelog entry?")],
    );
    s.scan(&[rig.src(true)], 1100);
    assert_eq!(rig.texts().len(), 2);
    let calls = std::fs::read_to_string(rig.work.path().join("calls.txt")).unwrap();
    assert_eq!(calls.lines().count(), 1);
}

#[test]
fn the_haiku_setting_turns_it_off() {
    let rig = Rig::new();
    let bin = fake(
        rig.bin.path(),
        "touch called.txt\ncat >/dev/null\necho '{\"questions\":[]}'",
    );
    Store::new(rig.runner.path())
        .save_settings(Settings { haiku: false })
        .unwrap();
    append(
        &rig.transcript,
        &[user("go"), agent("Should I also update the docs?")],
    );
    rig.scanner(Some(bin)).scan(&[rig.src(true)], 1000);
    assert_eq!(rig.texts().len(), 1);
    assert!(!rig.work.path().join("called.txt").exists());
}

#[test]
fn a_restart_does_not_reclassify_or_duplicate() {
    let rig = Rig::new();
    append(
        &rig.transcript,
        &[user("go"), agent("Should I also update the docs?")],
    );
    rig.scanner(None).scan(&[rig.src(true)], 1000);
    let mut again = rig.scanner(None);
    assert!(!again.scan(&[rig.src(true)], 2000));
    assert_eq!(rig.texts().len(), 1);
    append(
        &rig.transcript,
        &[user("later"), agent("Should I also update the docs?")],
    );
    again.scan(&[rig.src(true)], 3000);
    assert_eq!(rig.texts().len(), 1);
}

#[test]
fn ask_user_question_does_not_need_haiku() {
    let rig = Rig::new();
    let ask = serde_json::json!({"type":"assistant","message":{"content":[{"type":"tool_use","name":"AskUserQuestion","input":{"questions":[{"question":"Pick one?"}]}}]}}).to_string();
    append(&rig.transcript, &[user("go"), ask]);
    let bin = fake(rig.bin.path(), "touch called.txt\ncat >/dev/null\nexit 1");
    rig.scanner(Some(bin)).scan(&[rig.src(true)], 1000);
    assert_eq!(rig.texts(), vec!["Pick one?"]);
    assert!(!rig.work.path().join("called.txt").exists());
}

#[test]
fn paste_text_is_one_safe_block_without_enter() {
    let q = Question {
        id: "x".into(),
        session: SID.into(),
        text: "Should I\u{1b}[201~ update\nthe docs?".into(),
        asked_s: 1000,
        origin: Origin::Rule,
        user: String::new(),
        reply: "First paragraph.\n\nWe changed the parser. Should I[201~ update the docs? It is cheap.\n\nLast.".into(),
        item: String::new(),
        pos: 0,
        turn: 0,
    };
    let text = paste_text(&q, 1000 + 12 * 60);
    assert!(!text.contains('\u{1b}'));
    assert!(text
        .starts_with("> Earlier you asked (12 min ago): \"Should I[201~ update the docs?\"\n> "));
    assert!(text.ends_with("\nAnswer: "));
    assert!(!text.contains('\r'));
    assert_eq!(text.matches('\n').count(), 2);
}

#[test]
fn ages() {
    assert_eq!(ago(100, 100), "just now");
    assert_eq!(ago(100 + 720, 100), "12 min ago");
    assert_eq!(ago(100 + 3 * 3600, 100), "3 h ago");
    assert_eq!(ago(100 + 2 * 86400, 100), "2 d ago");
    assert_eq!(short_age(100 + 720, 100), "12m");
}

#[test]
fn context_lines_pick_the_paragraphs_around_the_question() {
    let reply = "intro\n\nbefore\n\nShould we go on?\nplease say\n\nafter\n\nlast";
    assert_eq!(
        context_lines(reply, "Should we go on?"),
        vec!["before", "", "Should we go on?", "please say", "", "after"]
    );
}

fn ask(id: &str, q: &str) -> String {
    serde_json::json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":id,"name":"AskUserQuestion","input":{"questions":[{"question":q}]}}]}}).to_string()
}

fn tool(id: &str) -> String {
    serde_json::json!({"type":"assistant","message":{"content":[{"type":"tool_use","id":id,"name":"Bash","input":{}}]}}).to_string()
}

fn result(id: &str) -> String {
    serde_json::json!({"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":id,"content":"ok"}]}}).to_string()
}

#[test]
fn an_answered_ask_user_question_is_dropped() {
    let d = private_tmp();
    let (mut t, p) = tailer(d.path(), "a.jsonl", None);
    append(
        &p,
        &[
            user("go"),
            ask("t1", "Pick one?"),
            result("t1"),
            agent("Thanks, done."),
        ],
    );
    let turn = t.poll(true).unwrap();
    assert!(turn.asked.is_empty());
}

#[test]
fn a_turn_with_a_running_tool_is_not_closed() {
    let d = private_tmp();
    let (mut t, p) = tailer(d.path(), "a.jsonl", None);
    append(
        &p,
        &[user("go"), agent("Should I run the tests?"), tool("t9")],
    );
    assert_eq!(t.poll(true), None);
    append(&p, &[result("t9"), agent("They pass.")]);
    let turn = t.poll(true).unwrap();
    assert_eq!(turn.text, "Should I run the tests?\n\nThey pass.");
}

#[test]
fn a_huge_human_line_still_ends_the_turn() {
    let d = private_tmp();
    let (mut t, p) = tailer(d.path(), "a.jsonl", None);
    let big =
        serde_json::json!({"type":"user","message":{"content":"x".repeat(1_200_000)}}).to_string();
    append(&p, &[user("go"), agent("Which one?"), big, agent("Fine.")]);
    assert_eq!(t.poll(true).unwrap().text, "Fine.");
}

#[test]
fn a_second_window_does_not_reclassify_a_closed_turn() {
    let rig = Rig::new();
    append(
        &rig.transcript,
        &[user("go"), agent("Should I also update the docs?")],
    );
    let mut a = rig.scanner(None);
    let mut b = rig.scanner(None);
    a.scan(&[rig.src(false)], 1000);
    b.scan(&[rig.src(false)], 1000);
    a.scan(&[rig.src(true)], 1001);
    b.scan(&[rig.src(true)], 1002);
    assert_eq!(rig.texts().len(), 1);
}

const TWO_ITEMS: &str = "1. **Что именно экспортируем?** Задачи (из списка/воркспейса), комментарии, страницы или что-то ещё? И по какому срезу: весь воркспейс, текущий фильтр/список или выбранные задачи?\n\n2. **Где живёт экспорт и как его получает пользователь?** Это кнопка в веб-UI (`task-manager-ui`) с эндпоинтом в Java-бэкенде, команда в CLI (`tmgr`), или оба варианта? Нужна ли выгрузка через скачивание файла сразу, или фоновая генерация с уведомлением (если данных много)?";

const SIX_SENTENCES: &str = "{\"questions\":[\"Что именно экспортируем?\",\"Задачи (из списка/воркспейса), комментарии, страницы или что-то ещё?\",\"И по какому срезу: весь воркспейс, текущий фильтр/список или выбранные задачи?\",\"Где живёт экспорт и как его получает пользователь?\",\"Это кнопка в веб-UI (`task-manager-ui`) с эндпоинтом в Java-бэкенде, команда в CLI (`tmgr`), или оба варианта?\",\"Нужна ли выгрузка через скачивание файла сразу, или фоновая генерация с уведомлением (если данных много)?\"]}";

fn labels(text: &str) -> Vec<String> {
    rules::groups(text).into_iter().map(|g| g.label).collect()
}

#[test]
fn a_numbered_list_item_is_one_question_with_its_bold_lead_as_the_row_text() {
    let got = rules::groups(TWO_ITEMS);
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(got[0].label, "Что именно экспортируем?");
    assert_eq!(
        got[1].label,
        "Где живёт экспорт и как его получает пользователь?"
    );
    assert!(got[0].pos < got[1].pos);
    assert!(got[0].item.contains("И по какому срезу"));
    assert!(got[1].item.contains("Нужна ли выгрузка"));
    assert!(!got[1].item.contains("**") && !got[1].item.contains('`'));
}

#[test]
fn a_paragraph_with_several_question_sentences_is_one_question() {
    let got = rules::groups(
        "We changed the parser. Should the docs change too? Or only the README? Your call.",
    );
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].label, "Should the docs change too?");
    assert!(got[0].item.contains("Or only the README?"));
}

#[test]
fn separate_paragraphs_and_tight_list_items_are_separate_questions() {
    assert_eq!(
        labels("Which port should the server use?\n\nShould we keep the old flag?"),
        vec![
            "Which port should the server use?",
            "Should we keep the old flag?"
        ]
    );
    assert_eq!(
        labels("Two things:\n- Which port should it use?\n- Keep the old flag or drop it?\n  Either is fine for me, honestly?"),
        vec!["Which port should it use?", "Keep the old flag or drop it?"]
    );
}

#[test]
fn nested_bullets_stay_with_their_item() {
    let t = "1. Which formats do you need?\n   - CSV with headers?\n   - JSON lines?\n2. Where should the file go?";
    let got = rules::groups(t);
    assert_eq!(got.len(), 2, "{got:?}");
    assert!(got[0].item.contains("JSON lines"));
}

#[test]
fn haiku_sentence_answers_collapse_into_their_items() {
    let spans = haiku::parse(SIX_SENTENCES, TWO_ITEMS).unwrap();
    assert_eq!(spans.len(), 6);
    let got = rules::groups_for_spans(TWO_ITEMS, &spans);
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(got[0].label, "Что именно экспортируем?");
    assert!(got[0].pos < got[1].pos);
}

#[test]
fn haiku_spans_for_whole_items_parse_and_map_to_the_same_groups() {
    let reply = serde_json::json!({"questions": [
        TWO_ITEMS.split("\n\n").nth(1).unwrap().trim_start_matches("2. "),
        TWO_ITEMS.split("\n\n").next().unwrap().trim_start_matches("1. "),
    ]})
    .to_string();
    let spans = haiku::parse(&reply, TWO_ITEMS).unwrap();
    assert_eq!(spans.len(), 2);
    let got = rules::groups_for_spans(TWO_ITEMS, &spans);
    assert_eq!(got.len(), 2);
    assert_eq!(
        got[0].label,
        "Где живёт экспорт и как его получает пользователь?"
    );
}

fn rig_with_two_items() -> Rig {
    let rig = Rig::new();
    append(&rig.transcript, &[user("go"), agent(TWO_ITEMS)]);
    rig
}

#[test]
fn rule_fallback_stores_two_questions_in_message_order() {
    let rig = rig_with_two_items();
    rig.scanner(None).scan(&[rig.src(true)], 1000);
    assert_eq!(
        rig.texts(),
        vec![
            "Что именно экспортируем?",
            "Где живёт экспорт и как его получает пользователь?"
        ]
    );
}

#[test]
fn six_haiku_sentences_still_store_two_questions_in_message_order() {
    let rig = rig_with_two_items();
    let bin = fake(
        rig.bin.path(),
        &format!("cat >/dev/null\ncat <<'EOF'\n{SIX_SENTENCES}\nEOF"),
    );
    rig.scanner(Some(bin)).scan(&[rig.src(true)], 1000);
    let entries = Store::new(rig.runner.path()).entries(None);
    assert_eq!(entries.len(), 2);
    assert!(entries.iter().all(|e| e.q.origin == Origin::Haiku));
    assert_eq!(entries[0].q.text, "Что именно экспортируем?");
    assert!(!entries[0].q.item.is_empty());
}

#[test]
fn the_answer_quote_carries_the_whole_item() {
    let rig = rig_with_two_items();
    rig.scanner(None).scan(&[rig.src(true)], 1000);
    let e = &Store::new(rig.runner.path()).entries(None)[0];
    let text = paste_text(&e.q, 1000);
    assert!(
        text.starts_with("> Earlier you asked (just now): \""),
        "{text}"
    );
    assert!(text.contains("И по какому срезу"));
    assert!(text.ends_with("Answer: "));
}

#[test]
fn old_per_sentence_entries_load_sort_and_keep_their_marks() {
    let rig = Rig::new();
    let store = Store::new(rig.runner.path());
    let old = serde_json::json!({
        "transcript": "", "offset": 0,
        "questions": [
            {"id":"aaaaaaaaaaaa","session":SID,"text":"Old sentence one here?","asked_s":10,"origin":"haiku"},
            {"id":"bbbbbbbbbbbb","session":SID,"text":"Old sentence two here?","asked_s":10,"origin":"haiku","user":"u","reply":"r"}
        ]
    });
    tmgr_ipc::dirs::ensure_private_dir(store.dir()).unwrap();
    std::fs::write(store.dir().join(format!("{SID}.json")), old.to_string()).unwrap();
    store.mark("aaaaaaaaaaaa", Status::Ignored, 11).unwrap();
    let got = store.entries(None);
    assert_eq!(got.len(), 2);
    let a = got.iter().find(|e| e.q.id == "aaaaaaaaaaaa").unwrap();
    assert_eq!(a.status, Status::Ignored);
    assert!(a.q.item.is_empty());
    assert_eq!(
        paste_text(&a.q, 20),
        "> Earlier you asked (just now): \"Old sentence one here?\"\nAnswer: "
    );
}
