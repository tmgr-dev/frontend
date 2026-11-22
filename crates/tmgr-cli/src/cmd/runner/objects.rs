use super::{connect, emit, rerr, runner_config, Opts};
use crate::out::table;
use serde_json::{json, Value};
use std::io::{IsTerminal, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::{caps, method, ExemptFolder, ObjectsAllowParams, ObjectsDenied};
use tmgr_link::audit::Audit;
use tmgr_link::pin::{self, Verdict};
use tmgr_link::privileged;
use tmgr_link::store::Paths;
use tmgr_runner::client::Client;
use tmgr_runner::daemon;
use tmgr_runner::objects_exempt::{self, Ident, Measured};
use tmgr_runner::wire::now_ms;

const SHOWN: usize = 10;
const CONFIRM_WORD: &str = "exempt";
const CHANGED: &str = "the folder changed while it was checked, try again";

fn clean(s: &str) -> String {
    tmgr_core::text::sanitize_line(s)
}

fn client() -> Result<Client> {
    let cfg = runner_config()?;
    daemon::start_background(&cfg).map_err(rerr)?;
    let c = connect(&cfg)?;
    if !c.welcome().caps.iter().any(|h| h == caps::OBJECT_EXEMPT_V1) {
        return Err(Error::new(
            Kind::Conflict,
            "the running runner is older than this tmgr and keeps no exempt folders",
        )
        .hint(format!("{0} runner restart", super::bin())));
    }
    Ok(c)
}

fn gate(in_agent: bool, tty: bool) -> Result<()> {
    privileged::check(in_agent, tty)
}

fn no_pin() -> Error {
    Error::new(
        Kind::Forbidden,
        "no follower PIN is set, so no folder can be exempted",
    )
    .hint("set one first: tmgr link pin set")
}

fn require_pin_set(st: &pin::Status) -> Result<()> {
    if st.set {
        Ok(())
    } else {
        Err(no_pin())
    }
}

fn pin_verdict(v: Verdict) -> Result<()> {
    match v {
        Verdict::Ok => Ok(()),
        Verdict::Unset => Err(no_pin()),
        Verdict::Locked => Err(Error::new(
            Kind::Forbidden,
            "the follower PIN is locked after too many wrong tries",
        )
        .hint("unlock it with: tmgr link pin reset")),
        Verdict::TooShort => Err(Error::new(
            Kind::Forbidden,
            "the follower PIN is too short, nothing was changed",
        )),
        Verdict::Wrong { left } => Err(Error::new(
            Kind::Forbidden,
            format!("wrong follower PIN ({left} tries left), nothing was changed"),
        )),
    }
}

fn authorize(paths: &Paths, entered: &str) -> Result<()> {
    pin_verdict(pin::verify(paths, entered)?)
}

fn refuse_outside(m: &Measured) -> Result<()> {
    if m.outside_count == 0 {
        return Ok(());
    }
    let mut msg = format!(
        "{} file(s) in this folder are linked from outside it, so it cannot be exempted:",
        m.outside_count
    );
    let shown = m.outside.iter().take(SHOWN).collect::<Vec<_>>();
    for p in &shown {
        msg.push_str(&format!("\n  {}", clean(p)));
    }
    let more = m.outside_count.saturating_sub(shown.len() as u64);
    if more > 0 {
        msg.push_str(&format!("\n  and {more} more"));
    }
    Err(Error::new(Kind::Forbidden, msg).hint("remove the outside links and try again"))
}

fn confirm_text(path: &str, m: &Measured) -> String {
    format!(
        "Walked {path}: {} files in {}, none linked from outside.\n\
         Exempting it means only the shared-object check is skipped, for exactly this folder.\n\
         Links created later (for example `git clone <inside> <outside>` or `cp -al`) are not re-checked at start; use `git clone --no-local`.\n\
         Note that any process of this user can already edit this list; the PIN only shows that a person is at the keyboard.\n\
         Type {CONFIRM_WORD} to continue:",
        m.files_seen,
        fmt_ms(m.elapsed_ms),
        path = clean(path),
    )
}

fn fmt_ms(ms: u64) -> String {
    match ms {
        0..=999 => format!("{ms} ms"),
        1000..=59_999 => format!("{:.1} s", ms as f64 / 1000.0),
        _ => format!("{} min {:02} s", ms / 60_000, ms % 60_000 / 1000),
    }
}

fn rows(folders: &[ExemptFolder], now: u64) -> String {
    let mut rows = vec![vec![
        "FOLDER".into(),
        "STALE".into(),
        "MEASURED".into(),
        "ADDED".into(),
    ]];
    for f in folders {
        rows.push(vec![
            clean(&f.path),
            if f.stale { "stale" } else { "-" }.into(),
            format!("walk took {}", fmt_ms(f.measured_ms)),
            format!(
                "{} ago",
                super::ops::span(now.saturating_sub(f.added_ms) / 1000)
            ),
        ]);
    }
    let mut out = table(&rows);
    if folders.iter().any(|f| f.stale) {
        out.push_str(
            "\nstale folders were replaced or moved since they were added and are not exempt; remove them with deny",
        );
    }
    out
}

fn absolute(dir: &Path) -> Result<String> {
    let abs = if dir.is_absolute() {
        dir.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| Error::usage(e.to_string()))?
            .join(dir)
    };
    abs.to_str()
        .map(str::to_string)
        .ok_or_else(|| Error::usage("the folder path is not valid UTF-8"))
}

fn canonical(dir: &Path) -> Result<PathBuf> {
    let path = absolute(dir)?;
    let real = std::fs::canonicalize(&path)
        .map_err(|_| Error::usage(format!("{} is not an existing folder", clean(&path))))?;
    if !real.is_dir() {
        return Err(Error::usage(format!("{} is not a folder", clean(&path))));
    }
    Ok(real)
}

fn walk(path: &Path) -> Result<Measured> {
    let show = std::io::stderr().is_terminal();
    let mut progress = |n: u64| {
        if show {
            eprint!("\rwalking the folder: {n} files seen");
            let _ = std::io::stderr().flush();
        }
    };
    let m = objects_exempt::measure(path, &mut progress).map_err(Error::usage)?;
    if show {
        eprintln!("\rwalked the folder: {} files seen   ", m.files_seen);
    }
    Ok(m)
}

fn identity(path: &Path) -> Result<Ident> {
    let f = std::fs::File::open(path).map_err(|e| Error::usage(e.to_string()))?;
    objects_exempt::ident_fd(f.as_raw_fd()).map_err(|e| Error::usage(e.to_string()))
}

fn tty() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

pub fn allow(o: &Opts, dir: &Path) -> Result<i32> {
    gate(privileged::in_agent_session(), tty())?;
    let shown = canonical(dir)?;
    let id = identity(&shown)?;
    let paths = Paths::from_env()?;
    require_pin_set(&pin::status(&paths)?)?;
    let entered = rpassword::prompt_password("Follower PIN (input hidden): ")?;
    authorize(&paths, &entered)?;
    drop(entered);
    let m = walk(&shown)?;
    if m.root != id {
        return Err(Error::usage(CHANGED));
    }
    refuse_outside(&m)?;
    let text = shown.display().to_string();
    privileged::confirm_typed_stdio(CONFIRM_WORD, &confirm_text(&text, &m))?;
    let mut c = client()?;
    Audit::new(&paths.audit_dir()).append(
        "object_exempt.add",
        json!({
            "path": text,
            "dev": id.dev,
            "ino": id.ino,
            "birth_ns": id.birth_ns,
            "fsid": id.fsid,
            "files_seen": m.files_seen,
            "measured_ms": m.elapsed_ms,
        }),
    )?;
    let audit = Audit::new(&paths.audit_dir());
    let call = c.call(
        method::OBJECTS_ALLOW,
        serde_json::to_value(ObjectsAllowParams {
            path: text.clone(),
            dev: id.dev,
            ino: id.ino,
            birth_ns: id.birth_ns,
            fsid: id.fsid,
            measured_ms: m.elapsed_ms,
        })
        .map_err(|e| Error::usage(e.to_string()))?,
    );
    let v = outcome(&audit, call, &text, |_| Some("object_exempt.added"))?;
    let f: ExemptFolder = serde_json::from_value(v).map_err(|e| Error::usage(e.to_string()))?;
    let human = format!(
        "{} no longer gets the shared-object check at start",
        clean(&f.path)
    );
    Ok(emit(o, &f, human))
}

fn outcome(
    audit: &Audit,
    call: std::io::Result<Value>,
    path: &str,
    done: impl Fn(&Value) -> Option<&'static str>,
) -> Result<Value> {
    match call {
        Ok(v) => {
            if let Some(event) = done(&v) {
                if audit.append(event, json!({ "path": path })).is_err() {
                    eprintln!(
                        "warning: the change was made, but its audit entry could not be written"
                    );
                }
            }
            Ok(v)
        }
        Err(e) => {
            let e = rerr(e);
            let _ = audit.append(
                "object_exempt.failed",
                json!({ "path": path, "error": e.message }),
            );
            Err(e)
        }
    }
}

pub fn deny(o: &Opts, dir: &Path) -> Result<i32> {
    gate(privileged::in_agent_session(), true)?;
    let path = absolute(dir)?;
    let target = canonical(dir)
        .ok()
        .and_then(|p| std::fs::metadata(p).ok())
        .map(|m| (m.dev(), m.ino()));
    let mut c = client()?;
    let paths = Paths::from_env()?;
    let audit = Audit::new(&paths.audit_dir());
    audit.append("object_exempt.remove", json!({ "path": path }))?;
    let mut params = json!({ "path": path });
    if let Some((dev, ino)) = target {
        params["dev"] = dev.into();
        params["ino"] = ino.into();
    }
    let call = c.call(method::OBJECTS_DENY, params);
    let v = outcome(&audit, call, &path, |v| {
        (v["removed"] == true).then_some("object_exempt.removed")
    })?;
    let r: ObjectsDenied = serde_json::from_value(v).map_err(|e| Error::usage(e.to_string()))?;
    let human = if r.removed {
        format!(
            "{} is no longer exempt; sessions already running keep the exemption they started with",
            clean(&path)
        )
    } else {
        format!("{} was not exempt", clean(&path))
    };
    Ok(emit(o, &r, human))
}

pub fn list(o: &Opts) -> Result<i32> {
    let v = client()?
        .call(method::OBJECTS_LIST, Value::Null)
        .map_err(rerr)?;
    let folders: Vec<ExemptFolder> =
        serde_json::from_value(v).map_err(|e| Error::usage(e.to_string()))?;
    let human = if folders.is_empty() {
        "no exempt folders".to_string()
    } else {
        rows(&folders, now_ms())
    };
    Ok(emit(o, &folders, human))
}

pub fn check(o: &Opts, dir: &Path) -> Result<i32> {
    let shown = canonical(dir)?;
    let m = walk(&shown)?;
    let clean_folder = m.outside_count == 0;
    let data = json!({
        "path": shown,
        "clean": clean_folder,
        "files_seen": m.files_seen,
        "outside_count": m.outside_count,
        "outside": m.outside,
        "elapsed_ms": m.elapsed_ms,
    });
    let human = if clean_folder {
        format!(
            "{}: {} files in {}, none linked from outside; it could be exempted",
            clean(&shown.display().to_string()),
            m.files_seen,
            fmt_ms(m.elapsed_ms)
        )
    } else {
        refuse_outside(&m)
            .err()
            .map(|e| e.message)
            .unwrap_or_default()
    };
    emit(o, data, human);
    Ok(i32::from(!clean_folder))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measured(outside: &[&str], count: u64) -> Measured {
        Measured {
            root: Ident {
                dev: 1,
                ino: 2,
                birth_ns: 3,
                fsid: 4,
            },
            files_seen: 1234,
            outside: outside.iter().map(|s| s.to_string()).collect(),
            outside_count: count,
            elapsed_ms: 3200,
        }
    }

    fn folder(path: &str, stale: bool) -> ExemptFolder {
        ExemptFolder {
            path: path.into(),
            dev: 1,
            ino: 2,
            birth_ns: 3,
            fsid: 4,
            added_ms: 1_000_000,
            measured_ms: 3200,
            stale,
        }
    }

    fn paths_with_pin(pin: Option<&str>) -> (tempfile::TempDir, Paths) {
        let d = tempfile::tempdir().unwrap();
        let p = Paths::under(&std::fs::canonicalize(d.path()).unwrap());
        if let Some(pin) = pin {
            tmgr_link::pin::set(&p, pin).unwrap();
        }
        (d, p)
    }

    #[test]
    fn an_agent_session_is_refused_with_exit_5_even_on_a_terminal() {
        assert_eq!(gate(true, true).unwrap_err().exit_code(), 5);
        assert_eq!(gate(false, false).unwrap_err().exit_code(), 10);
        assert!(gate(false, true).is_ok());
    }

    #[test]
    fn without_a_pin_set_allow_is_refused_and_names_the_command() {
        let (_d, p) = paths_with_pin(None);
        let e = require_pin_set(&tmgr_link::pin::status(&p).unwrap()).unwrap_err();
        assert_eq!(e.exit_code(), 5);
        assert!(e
            .hint
            .as_deref()
            .unwrap_or("")
            .contains("tmgr link pin set"));
        assert!(e.message.contains("PIN"));
        let (_d, p) = paths_with_pin(Some("owner-pin-1234"));
        assert!(require_pin_set(&tmgr_link::pin::status(&p).unwrap()).is_ok());
    }

    #[test]
    fn a_wrong_pin_is_refused_and_counts_toward_the_lockout() {
        let (_d, p) = paths_with_pin(Some("owner-pin-1234"));
        let e = authorize(&p, "wrong-pin-x").unwrap_err();
        assert_eq!(e.exit_code(), 5);
        assert!(e.message.contains("wrong"));
        assert!(!e.message.contains("wrong-pin-x"));
        assert_eq!(tmgr_link::pin::status(&p).unwrap().failures, 1);
        assert!(authorize(&p, "owner-pin-1234").is_ok());
        assert_eq!(tmgr_link::pin::status(&p).unwrap().failures, 0);
    }

    #[test]
    fn a_locked_pin_is_refused_even_with_the_right_pin() {
        let (_d, p) = paths_with_pin(Some("owner-pin-1234"));
        for _ in 0..tmgr_link::pin::MAX_FAILURES {
            let _ = authorize(&p, "wrong-pin-x");
        }
        let e = authorize(&p, "owner-pin-1234").unwrap_err();
        assert!(e.message.contains("locked"));
        assert!(e
            .hint
            .as_deref()
            .unwrap_or("")
            .contains("tmgr link pin reset"));
    }

    #[test]
    fn a_folder_with_links_outside_is_refused_and_lists_at_most_ten() {
        assert!(refuse_outside(&measured(&[], 0)).is_ok());
        let names: Vec<String> = (0..12).map(|i| format!("/x/f{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let e = refuse_outside(&measured(&refs, 12)).unwrap_err();
        assert_eq!(e.exit_code(), 5);
        assert!(e.message.contains("12"));
        assert!(e.message.contains("/x/f9"));
        assert!(!e.message.contains("/x/f10"));
        assert!(e.message.contains("2 more"));
    }

    #[test]
    fn the_confirmation_states_what_is_skipped_and_what_is_not_covered() {
        let t = confirm_text("/work/site", &measured(&[], 0));
        assert!(t.contains("/work/site"));
        assert!(t.contains("1234 files"));
        assert!(t.contains("only the shared-object check is skipped"));
        assert!(t.contains("exactly this folder"));
        assert!(t.contains("git clone <inside> <outside>"));
        assert!(t.contains("cp -al"));
        assert!(t.contains("git clone --no-local"));
        assert!(t.contains("any process of this user can already edit this list"));
        assert!(t.contains(CONFIRM_WORD));
    }

    #[test]
    fn list_rows_mark_stale_folders_and_show_the_walk_time() {
        let out = rows(
            &[folder("/a", false), folder("/b", true)],
            1_000_000 + 7_200_000,
        );
        let lines: Vec<&str> = out.lines().collect();
        assert!(lines[0].contains("FOLDER") && lines[0].contains("STALE"));
        assert!(lines[0].contains("MEASURED") && lines[0].contains("ADDED"));
        assert!(lines[1].starts_with("/a") && !lines[1].contains("stale"));
        assert!(lines[2].starts_with("/b") && lines[2].contains("stale"));
        assert!(lines[1].contains("3.2 s") && lines[1].contains("2h 00m ago"));
        assert!(out.contains("not exempt"));
        assert!(!rows(&[folder("/a", false)], 0).contains("not exempt"));
    }

    fn broken_audit() -> (tempfile::TempDir, Audit) {
        let d = tempfile::tempdir().unwrap();
        let file = d.path().join("f");
        std::fs::write(&file, "").unwrap();
        let audit = Audit::new(&file.join("sub"));
        assert!(audit.append("x", json!({})).is_err());
        (d, audit)
    }

    fn events(audit: &Audit) -> Vec<String> {
        audit
            .tail(100)
            .unwrap()
            .iter()
            .map(|e| e["event"].as_str().unwrap_or_default().to_string())
            .collect()
    }

    #[test]
    fn a_removed_false_answer_writes_no_removed_audit_entry() {
        let d = tempfile::tempdir().unwrap();
        let audit = Audit::new(d.path());
        let done = |v: &Value| (v["removed"] == true).then_some("object_exempt.removed");
        outcome(&audit, Ok(json!({"removed": false})), "/p", done).unwrap();
        assert!(events(&audit).is_empty());
        outcome(&audit, Ok(json!({"removed": true})), "/p", done).unwrap();
        assert_eq!(events(&audit), vec!["object_exempt.removed"]);
    }

    #[test]
    fn an_audit_failure_after_a_successful_change_keeps_the_success() {
        let (_d, audit) = broken_audit();
        let v = outcome(&audit, Ok(json!({"removed": true})), "/p", |_| {
            Some("object_exempt.removed")
        })
        .unwrap();
        assert_eq!(v["removed"], true);
    }

    #[test]
    fn a_failed_call_is_still_an_error_even_when_the_audit_is_broken() {
        let (_d, audit) = broken_audit();
        let e = outcome(
            &audit,
            Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof)),
            "/p",
            |_| Some("object_exempt.added"),
        )
        .unwrap_err();
        assert_eq!(e.exit_code(), 7);
    }

    #[test]
    fn walk_times_are_short_and_readable() {
        assert_eq!(fmt_ms(850), "850 ms");
        assert_eq!(fmt_ms(3200), "3.2 s");
        assert_eq!(fmt_ms(125_000), "2 min 05 s");
    }
}
