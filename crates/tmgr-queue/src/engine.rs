use crate::audit::{sha256_hex, Audit};
use crate::paths::Paths;
use crate::rules::{self, hours_contains, ProfileResolver, Rule, GLOBAL_MAX};
use crate::runs::{RunsClient, StartRequest};
use crate::source::{Candidate, TaskSource};
use crate::state::{self, Pickup, State, Status};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::{BundleLaunch, BundlePersona, RunBundle};

const APPROVAL_TTL_HOURS: i64 = 24;
const MAX_BUNDLE: usize = 1 << 20;

pub struct Deps<'a> {
    pub source: &'a dyn TaskSource,
    pub runs: &'a dyn RunsClient,
    pub profiles: &'a dyn ProfileResolver,
    pub now: DateTime<Utc>,
    pub local_minutes: u32,
}

#[derive(Debug, Default, Serialize)]
pub struct PassReport {
    pub enabled: bool,
    pub paused: bool,
    pub mock_runs: bool,
    pub events: Vec<Value>,
}

fn ts(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn bundle_of(rule: &Rule, c: &Candidate) -> RunBundle {
    RunBundle {
        v: 1,
        task: c.task,
        title: c.title.clone(),
        body: c.body.clone(),
        author: tmgr_ipc::api::BundleAuthor {
            kind: c.author.bundle_kind(),
            id: None,
        },
        comments: c.comments.clone(),
        pages: c.pages.clone(),
        persona: Some(BundlePersona {
            name: rule.persona.clone(),
            instructions_sha256: c.persona_instructions_sha256.clone(),
        }),
        launch: BundleLaunch {
            profile: rule.profile.clone(),
            model: rule.model.clone(),
            cwd: rule.repo.clone(),
        },
    }
}

fn check_rule(rule: &Rule, profiles: &dyn ProfileResolver) -> Result<()> {
    rules::validate_rule(rule, profiles)?;
    match std::fs::canonicalize(&rule.repo) {
        Ok(p) if p.to_string_lossy() == rule.repo => Ok(()),
        _ => Err(Error::new(
            Kind::Forbidden,
            "repo is not a canonical directory path",
        )),
    }
}

struct Pass<'a> {
    paths: &'a Paths,
    audit: Audit<'a>,
    state: State,
    report: PassReport,
    touched: BTreeSet<String>,
    now: DateTime<Utc>,
}

impl Pass<'_> {
    fn event(&mut self, p: &Pickup, outcome: &str) {
        self.report
            .events
            .push(json!({"pickup": p.id, "rule": p.rule, "task": p.key, "outcome": outcome}));
    }

    fn skip_once(
        &mut self,
        key: String,
        fingerprint: String,
        rule: &str,
        task: &str,
        reason: &str,
        extra: Value,
    ) -> Result<()> {
        self.touched.insert(key.clone());
        if self.state.skips.get(&key) != Some(&fingerprint) {
            let mut fields = json!({"rule": rule, "task": task, "reason": reason});
            if let (Some(f), Some(e)) = (fields.as_object_mut(), extra.as_object()) {
                f.extend(e.clone());
            }
            self.audit.append("skipped", fields)?;
            self.report
                .events
                .push(json!({"rule": rule, "task": task, "outcome": "skipped", "reason": reason}));
            self.state.skips.insert(key, fingerprint);
        }
        Ok(())
    }

    fn set(&mut self, i: usize, status: Status, reason: Option<String>) {
        let p = &mut self.state.pickups[i];
        p.status = status;
        p.updated_at = ts(self.now);
        p.reason = reason;
    }
}

fn new_id(state: &State, rule: &str, task_id: i64, sha: &str, now: DateTime<Utc>) -> String {
    let mut n = 0u32;
    loop {
        let seed = format!(
            "{rule}/{task_id}/{sha}/{}/{n}",
            now.timestamp_nanos_opt().unwrap_or(0)
        );
        let id = format!("q_{}", &sha256_hex(seed.as_bytes())[..10]);
        if !state.pickups.iter().any(|p| p.id == id) {
            return id;
        }
        n += 1;
    }
}

fn recover(pass: &mut Pass) -> Result<()> {
    for i in 0..pass.state.pickups.len() {
        let p = &pass.state.pickups[i];
        if p.status == Status::Starting && p.run_id.is_none() {
            let (id, rule, task) = (p.id.clone(), p.rule.clone(), p.key.clone());
            pass.set(i, Status::Failed, Some("start was interrupted before a run id was recorded; check the agents list before retrying".into()));
            pass.audit.append(
                "start_interrupted",
                json!({"pickup": id, "rule": rule, "task": task}),
            )?;
        }
    }
    Ok(())
}

fn scan_rule(pass: &mut Pass, rule: &Rule, deps: &Deps) -> Result<Vec<Candidate>> {
    let candidates = match deps.source.candidates(rule) {
        Ok(c) => c,
        Err(e) => {
            pass.audit.append(
                "source_error",
                json!({"rule": rule.name, "error": e.message}),
            )?;
            pass.report
                .events
                .push(json!({"rule": rule.name, "outcome": "source_error", "reason": e.message}));
            return Ok(Vec::new());
        }
    };
    let present: BTreeSet<i64> = candidates.iter().map(|c| c.task.task_id).collect();
    for i in 0..pass.state.pickups.len() {
        let p = &pass.state.pickups[i];
        if p.rule == rule.name
            && !p.released
            && !present.contains(&p.task.task_id)
            && p.status != Status::Starting
        {
            let (id, status) = (p.id.clone(), p.status);
            if matches!(status, Status::PendingApproval | Status::Approved) {
                pass.set(
                    i,
                    Status::Expired,
                    Some("the task no longer matches the rule".into()),
                );
            }
            pass.state.pickups[i].released = true;
            pass.audit.append(
                "released",
                json!({"pickup": id, "rule": rule.name, "was": status.name()}),
            )?;
        }
    }
    for c in &candidates {
        let skey = format!("{}/{}", rule.name, c.task.task_id);
        let trusted = rule.trusted_authors.iter().any(|a| a == c.author.name());
        if !trusted {
            pass.skip_once(
                skey,
                format!("author:{}", c.author.name()),
                &rule.name,
                &c.key,
                "author_not_trusted",
                json!({"author": c.author.name()}),
            )?;
            continue;
        }
        let bundle = bundle_of(rule, c);
        if let Err(m) = bundle.validate() {
            pass.skip_once(
                skey,
                format!("invalid:{m}"),
                &rule.name,
                &c.key,
                "bundle_invalid",
                json!({"detail": m}),
            )?;
            continue;
        }
        if bundle.canonical().len() > MAX_BUNDLE {
            pass.skip_once(
                skey,
                "too_large".into(),
                &rule.name,
                &c.key,
                "bundle_too_large",
                json!({}),
            )?;
            continue;
        }
        let sha = bundle.sha256();
        let existing =
            pass.state.pickups.iter().position(|p| {
                p.rule == rule.name && p.task.task_id == c.task.task_id && !p.released
            });
        match existing {
            Some(i) => {
                let p = &pass.state.pickups[i];
                if p.bundle_sha256 != sha
                    && matches!(p.status, Status::PendingApproval | Status::Approved)
                {
                    let voided = p.status == Status::Approved;
                    let (id, old) = (p.id.clone(), p.bundle_sha256.clone());
                    let p = &mut pass.state.pickups[i];
                    p.bundle = bundle;
                    p.bundle_sha256 = sha.clone();
                    p.context_tainted = c.context_tainted;
                    p.approved_by = None;
                    pass.set(i, Status::PendingApproval, None);
                    pass.audit.append(
                        if voided { "approval_voided" } else { "bundle_changed" },
                        json!({"pickup": id, "rule": rule.name, "task": c.key, "old_sha256": old, "new_sha256": sha}),
                    )?;
                    let outcome = if voided {
                        "approval_voided"
                    } else {
                        "bundle_changed"
                    };
                    let p = pass.state.pickups[i].clone();
                    pass.event(&p, outcome);
                }
            }
            None => {
                let id = new_id(&pass.state, &rule.name, c.task.task_id, &sha, pass.now);
                let auto = rule.auto && !c.context_tainted;
                let now = ts(pass.now);
                pass.state.pickups.push(Pickup {
                    id: id.clone(),
                    rule: rule.name.clone(),
                    task: c.task,
                    key: c.key.clone(),
                    status: if auto {
                        Status::Approved
                    } else {
                        Status::PendingApproval
                    },
                    bundle,
                    bundle_sha256: sha.clone(),
                    context_tainted: c.context_tainted,
                    created_at: now.clone(),
                    updated_at: now,
                    approved_by: auto.then(|| "rule-auto".to_string()),
                    run_id: None,
                    reason: None,
                    released: false,
                });
                pass.audit.append(
                    "candidate",
                    json!({"pickup": id, "rule": rule.name, "task": c.key, "bundle_sha256": sha, "author": c.author.name(), "tainted": c.context_tainted, "decision": if auto { "auto_approved" } else { "pending_approval" }}),
                )?;
                let p = pass.state.pickups.last().cloned().expect("just pushed");
                pass.event(
                    &p,
                    if auto {
                        "auto_approved"
                    } else {
                        "pending_approval"
                    },
                );
            }
        }
    }
    Ok(candidates)
}

fn expire_stale(pass: &mut Pass) -> Result<()> {
    let limit = pass.now - Duration::hours(APPROVAL_TTL_HOURS);
    for i in 0..pass.state.pickups.len() {
        let p = &pass.state.pickups[i];
        if matches!(p.status, Status::PendingApproval | Status::Approved) && !p.released {
            let stale = DateTime::parse_from_rfc3339(&p.updated_at)
                .is_ok_and(|t| t.with_timezone(&Utc) < limit);
            if stale {
                let id = p.id.clone();
                pass.set(
                    i,
                    Status::Expired,
                    Some("not started within 24 hours".into()),
                );
                pass.audit.append("expired", json!({"pickup": id}))?;
            }
        }
    }
    Ok(())
}

fn start_phase(
    pass: &mut Pass,
    rules: &[Rule],
    fresh: &[(String, Vec<Candidate>)],
    deps: &Deps,
) -> Result<()> {
    for i in 0..pass.state.pickups.len() {
        let p = &pass.state.pickups[i];
        if p.status != Status::Approved || p.released {
            continue;
        }
        let (id, rule_name, key) = (p.id.clone(), p.rule.clone(), p.key.clone());
        let Some(rule) = rules.iter().find(|r| r.name == rule_name) else {
            pass.set(
                i,
                Status::Expired,
                Some("the rule was removed or is refused".into()),
            );
            pass.audit.append(
                "expired",
                json!({"pickup": id, "reason": "rule_unavailable"}),
            )?;
            continue;
        };
        if let Err(e) = check_rule(rule, deps.profiles) {
            pass.set(
                i,
                Status::Expired,
                Some(format!("rule refused: {}", e.message)),
            );
            pass.audit.append(
                "start_refused",
                json!({"pickup": id, "rule": rule_name, "reason": e.message}),
            )?;
            continue;
        }
        if rule
            .hours
            .as_deref()
            .is_some_and(|h| !hours_contains(h, deps.local_minutes))
        {
            continue;
        }
        if pass.state.live_count(Some(&rule_name)) >= rule.max_concurrent as usize
            || pass.state.live_count(None) >= GLOBAL_MAX
        {
            continue;
        }
        let current = fresh
            .iter()
            .find(|(n, _)| *n == rule_name)
            .and_then(|(_, cs)| {
                cs.iter()
                    .find(|c| c.task.task_id == pass.state.pickups[i].task.task_id)
            })
            .map(|c| bundle_of(rule, c).sha256());
        if current.as_deref() != Some(pass.state.pickups[i].bundle_sha256.as_str()) {
            pass.audit.append(
                "start_refused",
                json!({"pickup": id, "rule": rule_name, "reason": "content_changed_before_start"}),
            )?;
            pass.state.pickups[i].approved_by = None;
            pass.set(i, Status::PendingApproval, None);
            continue;
        }
        let p = pass.state.pickups[i].clone();
        pass.set(i, Status::Starting, None);
        state::save(pass.paths, &mut pass.state)?;
        pass.audit.append("starting", json!({"pickup": id, "rule": rule_name, "task": key, "bundle_sha256": p.bundle_sha256, "approved_by": p.approved_by}))?;
        let req = StartRequest {
            rule: rule_name.clone(),
            trigger: format!("rule:{rule_name}"),
            profile: rule.profile.clone(),
            model: rule.model.clone(),
            cwd: rule.repo.clone(),
            task: p.task,
            bundle: p.bundle.clone(),
            bundle_sha256: p.bundle_sha256.clone(),
        };
        match deps.runs.create(&req) {
            Ok(run_id) => {
                pass.state.pickups[i].run_id = Some(run_id.clone());
                pass.set(i, Status::Started, None);
                state::save(pass.paths, &mut pass.state)?;
                pass.audit.append("started", json!({"pickup": id, "rule": rule_name, "task": key, "run": run_id, "mock": deps.runs.is_mock()}))?;
                let p = pass.state.pickups[i].clone();
                pass.event(&p, "started");
            }
            Err(e) => {
                pass.set(i, Status::Failed, Some(e.message.clone()));
                state::save(pass.paths, &mut pass.state)?;
                pass.audit.append(
                    "start_failed",
                    json!({"pickup": id, "rule": rule_name, "error": e.message}),
                )?;
                let p = pass.state.pickups[i].clone();
                pass.event(&p, "failed");
            }
        }
    }
    Ok(())
}

pub fn run_once(paths: &Paths, deps: &Deps) -> Result<PassReport> {
    let _lock = paths.lock()?;
    let audit = Audit::new(paths);
    let loaded = rules::load(paths)?;
    let report = PassReport {
        enabled: loaded.file.enabled,
        mock_runs: deps.runs.is_mock(),
        ..PassReport::default()
    };
    if !loaded.file.enabled {
        return Ok(report);
    }
    if rules::is_tampered(paths, &loaded)? {
        audit.append("config_tampered", json!({"config_sha256": loaded.sha256}))?;
        return Err(Error::new(
            Kind::Forbidden,
            "queue.toml was changed outside `tmgr queue`",
        )
        .hint("review the file, then confirm it with `tmgr queue enable`"));
    }
    if loaded.file.version != rules::VERSION {
        return Err(Error::usage("queue.toml version is not supported"));
    }
    let mut pass = Pass {
        paths,
        audit,
        state: state::load(paths)?,
        report,
        touched: BTreeSet::new(),
        now: deps.now,
    };
    recover(&mut pass)?;
    if pass.state.paused {
        pass.report.paused = true;
        state::save(paths, &mut pass.state)?;
        return Ok(pass.report);
    }
    let mut valid: Vec<Rule> = Vec::new();
    let mut fresh: Vec<(String, Vec<Candidate>)> = Vec::new();
    for rule in &loaded.file.rules {
        if let Err(e) = check_rule(rule, deps.profiles) {
            let key = format!("rule:{}", rule.name);
            pass.skip_once(
                key,
                e.message.clone(),
                &rule.name,
                "",
                "rule_refused",
                json!({"detail": e.message}),
            )?;
            continue;
        }
        let cs = scan_rule(&mut pass, rule, deps)?;
        valid.push(rule.clone());
        fresh.push((rule.name.clone(), cs));
    }
    let touched = std::mem::take(&mut pass.touched);
    pass.state.skips.retain(|k, _| touched.contains(k));
    expire_stale(&mut pass)?;
    state::save(paths, &mut pass.state)?;
    start_phase(&mut pass, &valid, &fresh, deps)?;
    state::save(paths, &mut pass.state)?;
    Ok(pass.report)
}

pub fn approve(paths: &Paths, id: &str, shown_sha256: &str, now: DateTime<Utc>) -> Result<Pickup> {
    let _lock = paths.lock()?;
    let mut st = state::load(paths)?;
    let p = st
        .pickups
        .iter_mut()
        .find(|p| p.id == id && !p.released)
        .ok_or_else(|| {
            Error::not_found(format!("no open pickup {id}")).hint("tmgr queue status")
        })?;
    if p.status != Status::PendingApproval {
        return Err(Error::conflict(format!(
            "pickup {id} is {}, not waiting for approval",
            p.status.name()
        )));
    }
    if p.bundle_sha256 != shown_sha256 {
        return Err(
            Error::conflict("the task changed after it was shown to you; review it again")
                .hint(format!("tmgr queue approve {id}")),
        );
    }
    p.status = Status::Approved;
    p.approved_by = Some("local".into());
    p.updated_at = ts(now);
    let done = p.clone();
    Audit::new(paths).append(
        "approved",
        json!({"pickup": done.id, "rule": done.rule, "task": done.key, "bundle_sha256": done.bundle_sha256}),
    )?;
    state::save(paths, &mut st)?;
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{QueueFile, VERSION};
    use crate::source::Author;
    use std::cell::{Cell, RefCell};
    use tmgr_ipc::api::TaskRef;

    struct Source(RefCell<Vec<Candidate>>);

    impl TaskSource for Source {
        fn candidates(&self, _rule: &Rule) -> Result<Vec<Candidate>> {
            Ok(self.0.borrow().clone())
        }
    }

    #[derive(Default)]
    struct Runs {
        created: RefCell<Vec<String>>,
        fail: Cell<bool>,
    }

    impl RunsClient for Runs {
        fn create(&self, req: &StartRequest) -> Result<String> {
            if self.fail.get() {
                return Err(Error::internal("runner said no"));
            }
            let id = format!("mock-{}", self.created.borrow().len() + 1);
            self.created
                .borrow_mut()
                .push(format!("{}:{}", req.task.task_id, req.bundle_sha256));
            Ok(id)
        }

        fn is_mock(&self) -> bool {
            true
        }
    }

    struct Profiles(Cell<bool>);

    impl ProfileResolver for Profiles {
        fn argv(&self, _p: &str, _m: Option<&str>) -> std::result::Result<Vec<String>, String> {
            Ok(if self.0.get() {
                vec!["claude".into(), "--dangerously-skip-permissions".into()]
            } else {
                vec!["claude".into()]
            })
        }
    }

    fn cand(id: i64, body: &str) -> Candidate {
        Candidate {
            task: TaskRef {
                workspace_id: -7,
                task_id: id,
            },
            key: format!("TM-{id}"),
            title: format!("Task {id}"),
            body: body.into(),
            author: Author::Human,
            comments: vec![],
            pages: vec![],
            context_tainted: false,
            persona_instructions_sha256: "0".repeat(64),
        }
    }

    struct Fx {
        _tmp: tempfile::TempDir,
        paths: Paths,
        repo: String,
        source: Source,
        runs: Runs,
        profiles: Profiles,
    }

    impl Fx {
        fn new(edit: impl Fn(&mut Rule)) -> Fx {
            let tmp = tempfile::tempdir().unwrap();
            let repo = std::fs::canonicalize(tmp.path()).unwrap().join("repo");
            std::fs::create_dir(&repo).unwrap();
            let repo = std::fs::canonicalize(repo)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            let paths = Paths::at(&tmp.path().join("cfg"));
            let mut rule = Rule {
                name: "r1".into(),
                workspace: "local:personal".into(),
                project: None,
                status_type: "backlog".into(),
                persona: "builder".into(),
                profile: "claude-code".into(),
                model: None,
                repo: repo.clone(),
                max_concurrent: 1,
                hours: None,
                trusted_authors: vec!["human".into()],
                auto: false,
            };
            edit(&mut rule);
            let fx = Fx {
                _tmp: tmp,
                paths,
                repo,
                source: Source(RefCell::new(vec![])),
                runs: Runs::default(),
                profiles: Profiles(Cell::new(false)),
            };
            fx.write(QueueFile {
                version: VERSION,
                enabled: true,
                rules: vec![rule],
            });
            fx
        }

        fn write(&self, file: QueueFile) {
            let sha = rules::save(&self.paths, &file).unwrap();
            Audit::new(&self.paths)
                .append("enable", json!({"config_sha256": sha}))
                .unwrap();
        }

        fn pass(&self) -> Result<PassReport> {
            self.pass_at(12 * 60)
        }

        fn pass_at(&self, minutes: u32) -> Result<PassReport> {
            run_once(
                &self.paths,
                &Deps {
                    source: &self.source,
                    runs: &self.runs,
                    profiles: &self.profiles,
                    now: Utc::now(),
                    local_minutes: minutes,
                },
            )
        }

        fn pickups(&self) -> Vec<Pickup> {
            state::load(&self.paths).unwrap().pickups
        }

        fn audit_events(&self, name: &str) -> usize {
            Audit::new(&self.paths)
                .events(1000)
                .unwrap()
                .iter()
                .filter(|e| e["event"] == name)
                .count()
        }

        fn approve_all(&self) {
            for p in self
                .pickups()
                .iter()
                .filter(|p| p.status == Status::PendingApproval)
            {
                approve(&self.paths, &p.id, &p.bundle_sha256, Utc::now()).unwrap();
            }
        }
    }

    #[test]
    fn a_manual_rule_waits_for_approval_and_never_starts_on_its_own() {
        let fx = Fx::new(|_| {});
        fx.source.0.replace(vec![cand(1, "do it")]);
        fx.pass().unwrap();
        fx.pass().unwrap();
        let p = fx.pickups();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].status, Status::PendingApproval);
        assert!(fx.runs.created.borrow().is_empty());
    }

    #[test]
    fn approval_starts_exactly_once_across_passes() {
        let fx = Fx::new(|_| {});
        fx.source.0.replace(vec![cand(1, "do it")]);
        fx.pass().unwrap();
        fx.approve_all();
        fx.pass().unwrap();
        fx.pass().unwrap();
        fx.pass().unwrap();
        assert_eq!(fx.runs.created.borrow().len(), 1);
        let p = &fx.pickups()[0];
        assert_eq!(p.status, Status::Started);
        assert_eq!(p.run_id.as_deref(), Some("mock-1"));
        assert_eq!(fx.audit_events("started"), 1);
    }

    #[test]
    fn a_content_change_voids_the_approval_and_nothing_starts() {
        let fx = Fx::new(|_| {});
        fx.source.0.replace(vec![cand(1, "do it")]);
        fx.pass().unwrap();
        let first = fx.pickups()[0].bundle_sha256.clone();
        fx.approve_all();
        assert_eq!(fx.pickups()[0].status, Status::Approved);
        fx.source.0.replace(vec![cand(1, "do something else")]);
        fx.pass().unwrap();
        let p = &fx.pickups()[0];
        assert_eq!(p.status, Status::PendingApproval);
        assert_ne!(p.bundle_sha256, first);
        assert!(p.approved_by.is_none());
        assert!(fx.runs.created.borrow().is_empty());
        assert_eq!(fx.audit_events("approval_voided"), 1);
    }

    #[test]
    fn approving_a_stale_hash_is_refused() {
        let fx = Fx::new(|_| {});
        fx.source.0.replace(vec![cand(1, "do it")]);
        fx.pass().unwrap();
        let p = fx.pickups().remove(0);
        let e = approve(&fx.paths, &p.id, &"0".repeat(64), Utc::now()).unwrap_err();
        assert_eq!(e.exit_code(), 6);
        assert_eq!(fx.pickups()[0].status, Status::PendingApproval);
    }

    #[test]
    fn an_auto_rule_starts_human_tasks_but_tainted_context_needs_approval() {
        let fx = Fx::new(|r| r.auto = true);
        let mut tainted = cand(2, "with agent comment");
        tainted.context_tainted = true;
        fx.source.0.replace(vec![cand(1, "clean"), tainted]);
        fx.runs.fail.set(false);
        fx.pass().unwrap();
        let p = fx.pickups();
        assert_eq!(p[0].status, Status::Started);
        assert_eq!(p[0].approved_by.as_deref(), Some("rule-auto"));
        assert_eq!(p[1].status, Status::PendingApproval);
    }

    #[test]
    fn untrusted_authors_are_skipped_and_audited_once() {
        let fx = Fx::new(|_| {});
        let mut agent = cand(1, "x");
        agent.author = Author::Agent;
        let mut unknown = cand(2, "y");
        unknown.author = Author::Unrecorded;
        fx.source.0.replace(vec![agent, unknown, cand(3, "z")]);
        fx.pass().unwrap();
        fx.pass().unwrap();
        assert_eq!(fx.pickups().len(), 1);
        assert_eq!(fx.audit_events("skipped"), 2);
    }

    #[test]
    fn a_rule_may_trust_unrecorded_authors_explicitly() {
        let fx = Fx::new(|r| r.trusted_authors = vec!["human".into(), "unrecorded".into()]);
        let mut unknown = cand(2, "y");
        unknown.author = Author::Unrecorded;
        fx.source.0.replace(vec![unknown]);
        fx.pass().unwrap();
        assert_eq!(fx.pickups().len(), 1);
    }

    #[test]
    fn concurrency_is_per_rule_and_slots_free_when_the_task_leaves() {
        let fx = Fx::new(|r| r.auto = true);
        fx.source.0.replace(vec![cand(1, "a"), cand(2, "b")]);
        fx.pass().unwrap();
        fx.pass().unwrap();
        assert_eq!(fx.runs.created.borrow().len(), 1);
        fx.source.0.replace(vec![cand(2, "b")]);
        fx.pass().unwrap();
        assert_eq!(fx.runs.created.borrow().len(), 2);
        let p = fx.pickups();
        assert!(p[0].released && !p[1].released);
    }

    #[test]
    fn the_global_cap_applies_across_rules() {
        let fx = Fx::new(|r| {
            r.auto = true;
            r.max_concurrent = 3;
        });
        fx.source.0.replace((1..=5).map(|i| cand(i, "t")).collect());
        fx.pass().unwrap();
        assert_eq!(fx.runs.created.borrow().len(), GLOBAL_MAX);
    }

    #[test]
    fn the_hours_window_gates_starts_not_proposals() {
        let fx = Fx::new(|r| {
            r.auto = true;
            r.hours = Some("22-06".into());
        });
        fx.source.0.replace(vec![cand(1, "a")]);
        fx.pass_at(12 * 60).unwrap();
        assert_eq!(fx.pickups()[0].status, Status::Approved);
        assert!(fx.runs.created.borrow().is_empty());
        fx.pass_at(23 * 60).unwrap();
        assert_eq!(fx.pickups()[0].status, Status::Started);
    }

    #[test]
    fn pause_stops_everything_and_resume_continues() {
        let fx = Fx::new(|r| r.auto = true);
        fx.source.0.replace(vec![cand(1, "a")]);
        let mut st = state::load(&fx.paths).unwrap();
        st.paused = true;
        state::save(&fx.paths, &mut st).unwrap();
        let r = fx.pass().unwrap();
        assert!(r.paused);
        assert!(fx.pickups().is_empty());
        let mut st = state::load(&fx.paths).unwrap();
        st.paused = false;
        state::save(&fx.paths, &mut st).unwrap();
        fx.pass().unwrap();
        assert_eq!(fx.runs.created.borrow().len(), 1);
    }

    #[test]
    fn a_disabled_queue_does_nothing() {
        let fx = Fx::new(|r| r.auto = true);
        fx.write(QueueFile {
            version: VERSION,
            enabled: false,
            rules: vec![],
        });
        fx.source.0.replace(vec![cand(1, "a")]);
        assert!(!fx.pass().unwrap().enabled);
        assert!(fx.pickups().is_empty());
    }

    #[test]
    fn a_hand_edited_rules_file_stops_the_pass_and_is_audited() {
        let fx = Fx::new(|_| {});
        let text = std::fs::read_to_string(fx.paths.rules()).unwrap();
        std::fs::write(
            fx.paths.rules(),
            text.replace("auto = false", "auto = true"),
        )
        .unwrap();
        std::fs::set_permissions(
            fx.paths.rules(),
            std::os::unix::fs::PermissionsExt::from_mode(0o600),
        )
        .unwrap();
        fx.source.0.replace(vec![cand(1, "a")]);
        let e = fx.pass().err().unwrap();
        assert_eq!(e.exit_code(), 5);
        assert_eq!(fx.audit_events("config_tampered"), 1);
        assert!(fx.pickups().is_empty());
    }

    #[test]
    fn an_interrupted_start_is_failed_and_never_retried() {
        let fx = Fx::new(|_| {});
        fx.source.0.replace(vec![cand(1, "a")]);
        fx.pass().unwrap();
        fx.approve_all();
        let mut st = state::load(&fx.paths).unwrap();
        st.pickups[0].status = Status::Starting;
        state::save(&fx.paths, &mut st).unwrap();
        fx.pass().unwrap();
        fx.pass().unwrap();
        assert_eq!(fx.pickups()[0].status, Status::Failed);
        assert!(fx.runs.created.borrow().is_empty());
        assert_eq!(fx.audit_events("start_interrupted"), 1);
    }

    #[test]
    fn a_failed_start_is_not_retried_until_the_task_leaves_and_returns() {
        let fx = Fx::new(|r| r.auto = true);
        fx.runs.fail.set(true);
        fx.source.0.replace(vec![cand(1, "a")]);
        fx.pass().unwrap();
        fx.runs.fail.set(false);
        fx.pass().unwrap();
        assert_eq!(fx.pickups()[0].status, Status::Failed);
        assert!(fx.runs.created.borrow().is_empty());
        fx.source.0.replace(vec![]);
        fx.pass().unwrap();
        fx.source.0.replace(vec![cand(1, "a")]);
        fx.pass().unwrap();
        assert_eq!(fx.runs.created.borrow().len(), 1);
        assert_eq!(fx.pickups().len(), 2);
    }

    #[test]
    fn a_profile_that_turns_into_bypass_is_refused_at_pickup() {
        let fx = Fx::new(|_| {});
        fx.source.0.replace(vec![cand(1, "a")]);
        fx.pass().unwrap();
        fx.approve_all();
        fx.profiles.0.set(true);
        fx.pass().unwrap();
        assert!(fx.runs.created.borrow().is_empty());
        assert_eq!(fx.audit_events("skipped"), 1);
        assert_eq!(fx.pickups()[0].status, Status::Expired);
    }

    #[test]
    fn the_start_request_carries_the_approved_bundle_hash() {
        let fx = Fx::new(|_| {});
        fx.source.0.replace(vec![cand(1, "a")]);
        fx.pass().unwrap();
        let sha = fx.pickups()[0].bundle_sha256.clone();
        fx.approve_all();
        fx.pass().unwrap();
        assert_eq!(fx.runs.created.borrow()[0], format!("1:{sha}"));
        assert!(fx.pickups()[0].bundle.launch.cwd == fx.repo);
    }

    #[test]
    fn stale_approvals_expire() {
        let fx = Fx::new(|_| {});
        fx.source.0.replace(vec![cand(1, "a")]);
        fx.pass().unwrap();
        let mut st = state::load(&fx.paths).unwrap();
        st.pickups[0].updated_at = "2020-01-01T00:00:00Z".into();
        state::save(&fx.paths, &mut st).unwrap();
        fx.pass().unwrap();
        assert_eq!(fx.pickups()[0].status, Status::Expired);
    }

    #[test]
    fn the_audit_chain_stays_valid_through_a_whole_flow() {
        let fx = Fx::new(|_| {});
        fx.source.0.replace(vec![cand(1, "a")]);
        fx.pass().unwrap();
        fx.approve_all();
        fx.pass().unwrap();
        assert!(Audit::new(&fx.paths).verify().unwrap().is_ok());
    }
}
