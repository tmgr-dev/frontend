use crate::audit::{sha256_hex, Audit};
use crate::clones::plan_clones;
use crate::cloud::{CloudApi, CloudClaim, CloudSource};
use crate::paths::Paths;
use crate::rules::{self, hours_contains, ProfilePin, ProfileResolver, Rule, GLOBAL_MAX};
use crate::runs::{mode_name, Lookup, RunState, RunsClient, StartRequest, PERMISSION_MODE};
use crate::source::{Candidate, TaskSource};
use crate::state::{self, Pickup, PickupClone, State, Status};
use crate::workdir::{clone_dir, WorkDirs};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeSet;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::{BundleLaunch, BundlePersona, CloneSpec, RunBundle};

const APPROVAL_TTL_HOURS: i64 = 24;
pub const MAX_BUNDLE: usize = 64 * 1024;

pub trait Publisher {
    fn publish(&self, pickup: &Pickup) -> Result<String>;
}

pub struct Deps<'a> {
    pub source: &'a dyn TaskSource,
    pub cloud: &'a dyn CloudApi,
    pub publisher: &'a dyn Publisher,
    pub runs: &'a dyn RunsClient,
    pub profiles: &'a dyn ProfileResolver,
    pub workdirs: &'a dyn WorkDirs,
    pub now: DateTime<Utc>,
    pub local_minutes: u32,
}

#[derive(Debug, Default, Serialize)]
pub struct PassReport {
    pub enabled: bool,
    pub paused: bool,
    pub events: Vec<Value>,
}

fn ts(now: DateTime<Utc>) -> String {
    now.to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn bundle_of(rule: &Rule, c: &Candidate, cwd: &str) -> RunBundle {
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
            instructions_sha256: sha256_hex(c.persona_text.as_bytes()),
        }),
        launch: BundleLaunch {
            profile: rule.profile.clone(),
            model: rule.model.clone(),
            cwd: cwd.to_string(),
            permission_mode: None,
        },
    }
}

fn pin_of(rule: &Rule, cloud: bool, deps: &Deps) -> std::result::Result<ProfilePin, String> {
    if cloud {
        deps.profiles.pin_for(
            &rule.profile,
            rule.model.as_deref(),
            mode_name(rule.mode.as_deref()),
        )
    } else {
        deps.profiles.pin(&rule.profile, rule.model.as_deref())
    }
}

fn run_dir(rule: &Rule, cloud: Option<&CloudClaim>, pickup_id: &str) -> Option<std::path::PathBuf> {
    match cloud.filter(|c| !c.repos.is_empty()) {
        Some(c) => {
            let primary = c
                .repos
                .iter()
                .min_by(|a, b| a.full_name.cmp(&b.full_name))?;
            clone_dir(&primary.path, pickup_id)
        }
        None => clone_dir(rule.repos.first()?, pickup_id),
    }
}

fn check_rule(rule: &Rule, deps: &Deps) -> Result<()> {
    rules::validate_rule_with(rule, deps.profiles, Some(deps.cloud), deps.now)?;
    for repo in &rule.repos {
        match std::fs::canonicalize(repo) {
            Ok(p) if p.to_string_lossy() == *repo => {}
            _ => {
                return Err(Error::new(
                    Kind::Forbidden,
                    "repo is not a canonical directory path",
                ))
            }
        }
    }
    Ok(())
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

fn new_id(state: &State, rule: &str, task_id: i64, now: DateTime<Utc>) -> String {
    let mut n = 0u32;
    loop {
        let seed = format!(
            "{rule}/{task_id}/{}/{n}",
            now.timestamp_nanos_opt().unwrap_or(0)
        );
        let id = format!("q_{}", &sha256_hex(seed.as_bytes())[..10]);
        if !state.pickups.iter().any(|p| p.id == id) {
            return id;
        }
        n += 1;
    }
}

fn recover(pass: &mut Pass, deps: &Deps) -> Result<()> {
    let limit = pass.now - Duration::hours(APPROVAL_TTL_HOURS);
    for i in 0..pass.state.pickups.len() {
        let p = &pass.state.pickups[i];
        if p.status == Status::Starting && p.run_id.is_none() {
            let (id, rule, task) = (p.id.clone(), p.rule.clone(), p.key.clone());
            pass.set(
                i,
                Status::UnknownStart,
                Some("the start was interrupted; the slot is kept until the run is found or released".into()),
            );
            pass.audit.append(
                "start_interrupted",
                json!({"pickup": id, "rule": rule, "task": task}),
            )?;
        }
        let p = &pass.state.pickups[i];
        if p.status != Status::UnknownStart {
            continue;
        }
        let (id, key) = (p.id.clone(), p.start_key());
        let old = DateTime::parse_from_rfc3339(&p.updated_at)
            .is_ok_and(|t| t.with_timezone(&Utc) < limit);
        match deps.runs.lookup(&key) {
            Lookup::Found(run) => {
                pass.state.pickups[i].run_id = Some(run.clone());
                pass.set(i, Status::Started, None);
                pass.audit
                    .append("start_reconciled", json!({"pickup": id, "run": run}))?;
            }
            Lookup::NotFound => {
                pass.set(i, Status::Failed, Some("the run was never created".into()));
                pass.audit
                    .append("start_not_found", json!({"pickup": id}))?;
            }
            Lookup::Unsupported if old => {
                pass.set(
                    i,
                    Status::Failed,
                    Some("unreconciled after 24 hours".into()),
                );
                pass.audit.append("start_expired", json!({"pickup": id}))?;
            }
            Lookup::Unsupported => {}
        }
    }
    Ok(())
}

pub fn release(paths: &Paths, id: &str, now: DateTime<Utc>) -> Result<()> {
    let _lock = paths.lock()?;
    let mut st = state::load(paths)?;
    let p = st
        .pickups
        .iter_mut()
        .find(|p| p.id == id && p.status == Status::UnknownStart)
        .ok_or_else(|| Error::not_found(format!("no pickup {id} with an unknown start")))?;
    p.status = Status::Failed;
    p.reason = Some("released by the user".into());
    p.updated_at = ts(now);
    Audit::new(paths).append("start_released", json!({"pickup": id}))?;
    state::save(paths, &mut st)
}

fn refresh_runs(pass: &mut Pass, deps: &Deps) -> Result<()> {
    for i in 0..pass.state.pickups.len() {
        let p = &pass.state.pickups[i];
        if p.status != Status::Started || p.run_exited {
            continue;
        }
        let Some(run) = p.run_id.clone() else {
            continue;
        };
        let state = deps.runs.status(&run);
        if matches!(state, RunState::Exited | RunState::Succeeded) {
            let id = p.id.clone();
            pass.state.pickups[i].run_exited = true;
            pass.state.pickups[i].run_clean = state == RunState::Succeeded;
            pass.audit
                .append("run_exited", json!({"pickup": id, "run": run}))?;
        }
    }
    Ok(())
}

fn publish_phase(pass: &mut Pass, deps: &Deps) -> Result<()> {
    for i in 0..pass.state.pickups.len() {
        if !pass.state.pickups[i].awaits_publish() {
            continue;
        }
        pass.state.pickups[i].publish_attempts += 1;
        state::save(pass.paths, &mut pass.state)?;
        let p = pass.state.pickups[i].clone();
        match deps.publisher.publish(&p) {
            Ok(url) => {
                pass.state.pickups[i].pr_url = Some(url.clone());
                pass.state.pickups[i].publish_error = None;
                pass.audit.append(
                    "published",
                    json!({"pickup": p.id, "rule": p.rule, "task": p.key, "pr_url": url}),
                )?;
                let p = pass.state.pickups[i].clone();
                pass.event(&p, "published");
            }
            Err(e) => {
                pass.state.pickups[i].publish_error = Some(e.message.clone());
                let attempts = pass.state.pickups[i].publish_attempts;
                pass.audit.append(
                    "publish_failed",
                    json!({"pickup": p.id, "rule": p.rule, "task": p.key, "error": e.message, "attempt": attempts, "final": attempts >= state::MAX_PUBLISH_ATTEMPTS}),
                )?;
                let p = pass.state.pickups[i].clone();
                pass.event(&p, "publish_failed");
            }
        }
        state::save(pass.paths, &mut pass.state)?;
    }
    Ok(())
}

fn scan_rule(pass: &mut Pass, rule: &Rule, deps: &Deps) -> Result<Vec<Candidate>> {
    let found = if rules::is_cloud(rule) {
        CloudSource {
            api: deps.cloud,
            now: deps.now,
        }
        .candidates(rule)
    } else {
        deps.source.candidates(rule)
    };
    let scan = match found {
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
    let candidates = scan.candidates;
    for r in scan.refusals {
        pass.skip_once(
            format!("{}/{}", rule.name, r.task_id),
            format!("gate:{}", r.reason),
            &rule.name,
            &r.key,
            "cloud_gate_refused",
            json!({"detail": r.reason}),
        )?;
    }
    if scan.complete {
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
    } else {
        pass.skip_once(
            format!("{}/scan", rule.name),
            "incomplete".into(),
            &rule.name,
            "",
            "scan_incomplete",
            json!({}),
        )?;
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
        let existing = pass.state.pickups.iter().position(|p| {
            p.rule == rule.name && p.task.task_id == c.task.task_id && (!p.released || p.occupies())
        });
        let (id, cwd) = match existing {
            Some(i) => (
                pass.state.pickups[i].id.clone(),
                pass.state.pickups[i].bundle.launch.cwd.clone(),
            ),
            None => {
                let id = new_id(&pass.state, &rule.name, c.task.task_id, pass.now);
                let Some(dir) = run_dir(rule, c.cloud.as_ref(), &id) else {
                    continue;
                };
                (id, dir.to_string_lossy().into_owned())
            }
        };
        let bundle = bundle_of(rule, c, &cwd);
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
        if bundle.canonical().len() > MAX_BUNDLE || c.persona_text.len() > MAX_BUNDLE {
            pass.skip_once(
                skey,
                "too_large".into(),
                &rule.name,
                &c.key,
                "bundle_too_large",
                json!({"limit": MAX_BUNDLE}),
            )?;
            continue;
        }
        let sha = bundle.sha256();
        match existing {
            Some(i) => {
                let p = &pass.state.pickups[i];
                if p.bundle_sha256 != sha
                    && matches!(p.status, Status::PendingApproval | Status::Approved)
                {
                    let voided = p.status == Status::Approved;
                    let old = p.bundle_sha256.clone();
                    let p = &mut pass.state.pickups[i];
                    p.bundle = bundle;
                    p.bundle_sha256 = sha.clone();
                    p.context_tainted = c.context_tainted;
                    p.persona_text = c.persona_text.clone();
                    p.cloud = c.cloud.clone();
                    p.approved_by = None;
                    p.profile_pin = None;
                    pass.set(i, Status::PendingApproval, None);
                    pass.audit.append(
                        if voided { "approval_voided" } else { "bundle_changed" },
                        json!({"pickup": id, "rule": rule.name, "task": c.key, "old_sha256": old, "new_sha256": sha}),
                    )?;
                    let p = pass.state.pickups[i].clone();
                    pass.event(
                        &p,
                        if voided {
                            "approval_voided"
                        } else {
                            "bundle_changed"
                        },
                    );
                    if c.cloud.is_some() && rule.auto {
                        if let Ok(pin) = pin_of(rule, true, deps) {
                            pass.state.pickups[i].approved_by = Some("rule-auto".into());
                            pass.state.pickups[i].profile_pin = Some(pin);
                            pass.set(i, Status::Approved, None);
                            pass.audit.append(
                                "auto_approved",
                                json!({"pickup": id, "rule": rule.name, "task": c.key, "bundle_sha256": sha}),
                            )?;
                        }
                    }
                } else if matches!(p.status, Status::PendingApproval | Status::Approved) {
                    pass.state.pickups[i].cloud = c.cloud.clone();
                }
            }
            None => {
                let pin = if rule.auto && !c.context_tainted {
                    pin_of(rule, c.cloud.is_some(), deps).ok()
                } else {
                    None
                };
                let auto = pin.is_some();
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
                    persona_text: c.persona_text.clone(),
                    profile_pin: pin,
                    run_exited: false,
                    run_clean: false,
                    attempt: 1,
                    cloud: c.cloud.clone(),
                    pr_url: None,
                    publish_attempts: 0,
                    publish_error: None,
                    clones: Vec::new(),
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

fn back_to_pending(pass: &mut Pass, i: usize, event: &str, fields: Value) -> Result<()> {
    pass.audit.append(event, fields)?;
    pass.state.pickups[i].approved_by = None;
    pass.state.pickups[i].profile_pin = None;
    pass.set(i, Status::PendingApproval, None);
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
        if let Err(e) = check_rule(rule, deps) {
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
        let cwd = pass.state.pickups[i].bundle.launch.cwd.clone();
        if run_dir(rule, pass.state.pickups[i].cloud.as_ref(), &id)
            .is_none_or(|d| d.to_string_lossy() != cwd)
        {
            pass.set(
                i,
                Status::Failed,
                Some("run directory does not match the pickup".into()),
            );
            pass.audit.append(
                "start_refused",
                json!({"pickup": id, "reason": "cwd_mismatch"}),
            )?;
            continue;
        }
        let current = fresh
            .iter()
            .find(|(n, _)| *n == rule_name)
            .and_then(|(_, cs)| {
                cs.iter()
                    .find(|c| c.task.task_id == pass.state.pickups[i].task.task_id)
            })
            .map(|c| bundle_of(rule, c, &cwd).sha256());
        if current.as_deref() != Some(pass.state.pickups[i].bundle_sha256.as_str()) {
            back_to_pending(
                pass,
                i,
                "start_refused",
                json!({"pickup": id, "rule": rule_name, "reason": "content_changed_before_start"}),
            )?;
            continue;
        }
        let is_cloud = pass.state.pickups[i].cloud.is_some();
        let pin_now = pin_of(rule, is_cloud, deps);
        let pin = match (pin_now, pass.state.pickups[i].profile_pin.clone()) {
            (Ok(now), Some(approved)) if now == approved => approved,
            _ => {
                back_to_pending(
                    pass,
                    i,
                    "start_refused",
                    json!({"pickup": id, "rule": rule_name, "reason": "profile_changed_since_approval"}),
                )?;
                continue;
            }
        };
        let p = pass.state.pickups[i].clone();
        pass.set(i, Status::Starting, None);
        state::save(pass.paths, &mut pass.state)?;
        pass.audit.append(
            "starting",
            json!({"pickup": id, "rule": rule_name, "task": key, "bundle_sha256": p.bundle_sha256, "approved_by": p.approved_by, "cwd": cwd}),
        )?;
        let plans = p
            .cloud
            .as_ref()
            .filter(|c| !c.repos.is_empty())
            .map(|c| plan_clones(&c.repos).map_err(|e| e.message));
        let prepared = match (&p.cloud, &plans) {
            (_, Some(Err(m))) => Err(Error::new(Kind::Forbidden, m.clone())),
            (Some(c), Some(Ok(plans))) => deps.workdirs.prepare_multi(
                &plans[0].path,
                &id,
                &cwd,
                &plans
                    .iter()
                    .map(|c| (c.name.clone(), c.path.clone()))
                    .collect::<Vec<_>>(),
                &c.branch,
            ),
            (Some(c), None) => deps
                .workdirs
                .prepare_branch(&rule.repos[0], &id, &cwd, &c.branch),
            (None, _) => deps.workdirs.prepare(&rule.repos[0], &id, &cwd),
        };
        if let Err(e) = prepared {
            pass.set(i, Status::Failed, Some(e.message.clone()));
            state::save(pass.paths, &mut pass.state)?;
            pass.audit.append(
                "start_failed",
                json!({"pickup": id, "rule": rule_name, "error": e.message, "stage": "clone"}),
            )?;
            let p = pass.state.pickups[i].clone();
            pass.event(&p, "failed");
            continue;
        }
        if pin_of(rule, p.cloud.is_some(), deps).ok() != Some(pin.clone()) {
            pass.set(
                i,
                Status::Failed,
                Some("the agent profile changed while the run was prepared".into()),
            );
            state::save(pass.paths, &mut pass.state)?;
            pass.audit.append(
                "start_refused",
                json!({"pickup": id, "rule": rule_name, "reason": "profile_changed_during_start"}),
            )?;
            let p = pass.state.pickups[i].clone();
            pass.event(&p, "failed");
            continue;
        }
        let mut clone_specs = Vec::new();
        if let Some(Ok(plans)) = &plans {
            let mut ids = Vec::new();
            for plan in plans {
                let dir = std::path::Path::new(&cwd).join(&plan.name);
                match deps.workdirs.identity(&dir) {
                    Some((dev, ino)) => ids.push(PickupClone {
                        name: plan.name.clone(),
                        dev,
                        ino,
                    }),
                    None => break,
                }
                clone_specs.push(CloneSpec {
                    name: plan.name.clone(),
                    clone_of: plan.path.clone(),
                });
            }
            if ids.len() != plans.len() {
                pass.set(
                    i,
                    Status::Failed,
                    Some("a clone of the run is missing after it was prepared".into()),
                );
                state::save(pass.paths, &mut pass.state)?;
                pass.audit.append(
                    "start_failed",
                    json!({"pickup": id, "rule": rule_name, "error": "clone_missing", "stage": "clone"}),
                )?;
                let p = pass.state.pickups[i].clone();
                pass.event(&p, "failed");
                continue;
            }
            pass.state.pickups[i].clones = ids;
            state::save(pass.paths, &mut pass.state)?;
        }
        let p = pass.state.pickups[i].clone();
        let req = StartRequest {
            permission_mode: if p.cloud.is_some() {
                mode_name(rule.mode.as_deref())
            } else {
                PERMISSION_MODE
            },
            start_key: p.start_key(),
            rule: rule_name.clone(),
            trigger: format!("rule:{rule_name}"),
            profile: rule.profile.clone(),
            model: rule.model.clone(),
            cwd,
            clone_of: match &plans {
                Some(Ok(plans)) => plans[0].path.clone(),
                _ => rule.repos[0].clone(),
            },
            clones: clone_specs,
            exe_path: pin.exe_path.clone(),
            exe_sha256: pin.exe_sha256.clone(),
            argv_sha256: pin.argv_sha256(),
            name: format!("{key} rule:{rule_name}").chars().take(64).collect(),
            task: p.task,
            bundle: p.bundle.clone(),
            bundle_sha256: p.bundle_sha256.clone(),
            cloud: p.cloud.as_ref().map(|c| c.claim.clone()),
            unattended: p.cloud.is_some(),
        };
        match deps.runs.create(&req) {
            Ok(run_id) => {
                pass.state.pickups[i].run_id = Some(run_id.clone());
                pass.set(i, Status::Started, None);
                state::save(pass.paths, &mut pass.state)?;
                pass.audit.append(
                    "started",
                    json!({"pickup": id, "rule": rule_name, "task": key, "run": run_id}),
                )?;
                let p = pass.state.pickups[i].clone();
                pass.event(&p, "started");
            }
            Err(e) if e.kind == Kind::Offline => {
                pass.set(
                    i,
                    Status::UnknownStart,
                    Some("the runner did not confirm the start; the slot is kept until the run is found or released".into()),
                );
                state::save(pass.paths, &mut pass.state)?;
                pass.audit.append(
                    "start_unconfirmed",
                    json!({"pickup": id, "rule": rule_name, "error": e.message}),
                )?;
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

/// Whether the next pass may talk to the runner: a pickup is approved, starting or running, or an
/// enabled rule may approve and start one by itself.
pub fn needs_runner(paths: &Paths) -> Result<bool> {
    let loaded = rules::load(paths)?;
    if !loaded.file.enabled {
        return Ok(false);
    }
    let st = state::load(paths)?;
    let open = st.pickups.iter().any(|p| {
        matches!(
            p.status,
            Status::Approved | Status::Starting | Status::UnknownStart
        ) || (p.status == Status::Started && !p.run_exited)
    });
    Ok(open || (!st.paused && loaded.file.rules.iter().any(|r| r.auto)))
}

pub fn run_once(paths: &Paths, deps: &Deps) -> Result<PassReport> {
    let _lock = paths.lock()?;
    let audit = Audit::new(paths);
    let loaded = rules::load(paths)?;
    let report = PassReport {
        enabled: loaded.file.enabled,
        ..PassReport::default()
    };
    if !loaded.file.enabled {
        return Ok(report);
    }
    if rules::is_tampered(paths, &loaded)? {
        let _ = audit.append("config_tampered", json!({"config_sha256": loaded.sha256}));
        return Err(Error::new(
            Kind::Forbidden,
            "queue.toml or the audit log changed outside `tmgr queue`",
        )
        .hint("review the files, then confirm with `tmgr queue enable`"));
    }
    rules::check_version(&loaded.file)?;
    let mut pass = Pass {
        paths,
        audit,
        state: state::load(paths)?,
        report,
        touched: BTreeSet::new(),
        now: deps.now,
    };
    recover(&mut pass, deps)?;
    refresh_runs(&mut pass, deps)?;
    if pass.state.paused {
        pass.report.paused = true;
        state::save(paths, &mut pass.state)?;
        return Ok(pass.report);
    }
    publish_phase(&mut pass, deps)?;
    let mut valid: Vec<Rule> = Vec::new();
    let mut fresh: Vec<(String, Vec<Candidate>)> = Vec::new();
    for rule in &loaded.file.rules {
        if let Err(e) = check_rule(rule, deps) {
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

pub fn approve(
    paths: &Paths,
    id: &str,
    shown_sha256: &str,
    pin: ProfilePin,
    now: DateTime<Utc>,
) -> Result<Pickup> {
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
    p.profile_pin = Some(pin);
    p.updated_at = ts(now);
    let done = p.clone();
    Audit::new(paths).append(
        "approved",
        json!({"pickup": done.id, "rule": done.rule, "task": done.key, "bundle_sha256": done.bundle_sha256,
               "profile_sha256": done.profile_pin.as_ref().map(|p| p.profile_sha256.clone()),
               "exe_sha256": done.profile_pin.as_ref().map(|p| p.exe_sha256.clone())}),
    )?;
    state::save(paths, &mut st)?;
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cloud::{CloudTask, CloudWorkspace, Gated};
    use crate::rules::{QueueFile, VERSION};
    use crate::source::{Author, Scan};
    use crate::workdir::{init_repo, GitClone};
    use std::cell::{Cell, RefCell};
    use tmgr_ipc::api::CloudApproval;
    use tmgr_ipc::api::TaskRef;

    struct Source(RefCell<Vec<Candidate>>, Cell<bool>);

    impl TaskSource for Source {
        fn candidates(&self, _rule: &Rule) -> Result<Scan> {
            Ok(Scan {
                candidates: self.0.borrow().clone(),
                complete: self.1.get(),
                refusals: Vec::new(),
            })
        }
    }

    #[derive(Default)]
    struct Runs {
        created: RefCell<Vec<String>>,
        fail: Cell<bool>,
        exited: RefCell<Vec<String>>,
        failed: RefCell<Vec<String>>,
        lookup_mode: Cell<u8>,
        keys: RefCell<Vec<String>>,
        cwds: RefCell<Vec<String>>,
        requests: RefCell<Vec<StartRequest>>,
        lost: Cell<bool>,
    }

    impl RunsClient for Runs {
        fn create(&self, req: &StartRequest) -> Result<String> {
            if self.fail.get() {
                return Err(Error::internal("runner said no"));
            }
            if self.lost.get() {
                return Err(Error::new(Kind::Offline, "lost the connection"));
            }
            self.keys.borrow_mut().push(req.start_key.clone());
            self.cwds.borrow_mut().push(req.cwd.clone());
            self.requests.borrow_mut().push(req.clone());
            if req.cloud.is_none() {
                assert_eq!(req.permission_mode, "ask");
                assert!(!req.unattended);
            }
            let id = format!("mock-{}", self.created.borrow().len() + 1);
            self.created
                .borrow_mut()
                .push(format!("{}:{}", req.task.task_id, req.bundle_sha256));
            Ok(id)
        }

        fn lookup(&self, key: &str) -> Lookup {
            match self.lookup_mode.get() {
                0 => Lookup::Unsupported,
                1 => Lookup::NotFound,
                _ => Lookup::Found(format!("found-{key}")),
            }
        }

        fn status(&self, run_id: &str) -> RunState {
            if self.failed.borrow().iter().any(|r| r == run_id) {
                RunState::Exited
            } else if self.exited.borrow().iter().any(|r| r == run_id) {
                RunState::Succeeded
            } else {
                RunState::Running
            }
        }
    }

    struct Profiles(Cell<bool>, Cell<u8>);

    impl ProfileResolver for Profiles {
        fn argv(&self, _p: &str, _m: Option<&str>) -> std::result::Result<Vec<String>, String> {
            Ok(if self.0.get() {
                vec!["claude".into(), "--dangerously-skip-permissions".into()]
            } else {
                vec!["claude".into()]
            })
        }

        fn pin(&self, p: &str, m: Option<&str>) -> std::result::Result<ProfilePin, String> {
            Ok(ProfilePin {
                argv: self.argv(p, m)?,
                profile_sha256: "p".repeat(64),
                exe_path: "/bin/claude".into(),
                exe_sha256: format!("{:064}", self.1.get()),
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
            persona_text: "persona rules".into(),
            cloud: None,
        }
    }

    struct Fx {
        _tmp: tempfile::TempDir,
        paths: Paths,
        repo: String,
        source: Source,
        runs: Runs,
        profiles: Profiles,
        cloud: CloudFake,
        publisher: PublisherFake,
    }

    impl Fx {
        fn new(edit: impl Fn(&mut Rule)) -> Fx {
            let tmp = tempfile::tempdir().unwrap();
            let repo = std::fs::canonicalize(tmp.path()).unwrap().join("repo");
            std::fs::create_dir(&repo).unwrap();
            init_repo(&repo);
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
                repos: vec![repo.clone()],
                max_concurrent: 1,
                hours: None,
                trusted_authors: vec!["human".into()],
                auto: false,
                mode: None,
            };
            edit(&mut rule);
            let fx = Fx {
                _tmp: tmp,
                paths,
                repo,
                source: Source(RefCell::new(vec![]), Cell::new(true)),
                runs: Runs::default(),
                profiles: Profiles(Cell::new(false), Cell::new(0)),
                cloud: CloudFake::default(),
                publisher: PublisherFake::default(),
            };
            fx.cloud.repos.replace(vec![fx.repo.clone()]);
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
                    cloud: &self.cloud,
                    publisher: &self.publisher,
                    runs: &self.runs,
                    profiles: &self.profiles,
                    workdirs: &GitClone,
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
                approve(
                    &self.paths,
                    &p.id,
                    &p.bundle_sha256,
                    self.profiles.pin("claude-code", None).unwrap(),
                    Utc::now(),
                )
                .unwrap();
            }
        }
    }

    #[derive(Clone)]
    struct FakeTask {
        id: i64,
        title: String,
        body: String,
        refused: Option<String>,
    }

    #[derive(Default)]
    struct CloudFake {
        allow: Cell<u8>,
        repos: RefCell<Vec<String>>,
        claim_repos: RefCell<Vec<crate::clones::ClaimRepo>>,
        tasks: RefCell<Vec<FakeTask>>,
    }

    impl CloudApi for CloudFake {
        fn allow(
            &self,
            code: &str,
            _now: DateTime<Utc>,
        ) -> std::result::Result<CloudWorkspace, String> {
            match self.allow.get() {
                0 => Ok(CloudWorkspace {
                    id: 56,
                    code: code.into(),
                    repos: self.repos.borrow().clone(),
                }),
                1 => Err(format!("workspace {code} is not allowed to run agents on this machine; run `tmgr runner allow {code}`")),
                _ => Err(format!("the permission for workspace {code} expired; run `tmgr runner allow {code}`")),
            }
        }

        fn list(&self, _rule: &Rule, _ws: &CloudWorkspace) -> Result<(Vec<i64>, bool)> {
            Ok((self.tasks.borrow().iter().map(|t| t.id).collect(), true))
        }

        fn gate(&self, _rule: &Rule, ws: &CloudWorkspace, id: i64) -> Result<Gated> {
            let t = self
                .tasks
                .borrow()
                .iter()
                .find(|t| t.id == id)
                .cloned()
                .expect("listed");
            let key = format!("TM-{id}");
            if let Some(reason) = t.refused {
                return Ok(Gated::Refused { key, reason });
            }
            let sha = tmgr_core::approval::content_sha256(&t.title, &t.body);
            Ok(Gated::Ready(Box::new(CloudTask {
                key,
                title: t.title,
                body: t.body,
                claim: CloudApproval {
                    repos_sha256: None,
                    host: "api.example.com".into(),
                    user_id: 1,
                    workspace_id: ws.id,
                    task_id: id,
                    category_id: 127,
                    content_sha256: sha,
                    key_id: "k1".into(),
                    signed_at: 10,
                    signature: "sig".into(),
                    kind: "confirm".into(),
                },
                repos: self.claim_repos.borrow().clone(),
            })))
        }
    }

    #[derive(Default)]
    struct PublisherFake {
        calls: RefCell<Vec<String>>,
        fail: Cell<bool>,
    }

    impl Publisher for PublisherFake {
        fn publish(&self, pickup: &Pickup) -> Result<String> {
            self.calls.borrow_mut().push(pickup.id.clone());
            if self.fail.get() {
                return Err(Error::internal("push refused"));
            }
            Ok(format!(
                "https://github.com/o/r/pull/{}",
                pickup.task.task_id
            ))
        }
    }

    fn signed(id: i64, body: &str) -> FakeTask {
        FakeTask {
            id,
            title: format!("Task {id}"),
            body: body.into(),
            refused: None,
        }
    }

    fn cloud_fx() -> Fx {
        Fx::new(|r| {
            r.workspace = "tmgrdev".into();
            r.project = Some("TM".into());
            r.auto = true;
            r.mode = Some("bypass".into());
        })
    }

    #[test]
    fn a_cloud_rule_is_refused_when_the_workspace_is_not_allowed_or_expired() {
        for (state, word) in [(1u8, "not allowed"), (2, "expired")] {
            let fx = cloud_fx();
            fx.cloud.allow.set(state);
            fx.cloud.tasks.replace(vec![signed(1, "do it")]);
            fx.pass().unwrap();
            assert!(fx.pickups().is_empty());
            assert!(fx.runs.created.borrow().is_empty());
            let events = Audit::new(&fx.paths).events(1000).unwrap();
            let skipped = events
                .iter()
                .find(|e| e["event"] == "skipped")
                .expect("skipped");
            assert_eq!(skipped["reason"], "rule_refused");
            let detail = skipped["detail"].as_str().unwrap();
            assert!(detail.contains(word) && detail.contains("tmgr runner allow tmgrdev"));
        }
    }

    #[test]
    fn a_cloud_rule_must_be_auto() {
        let fx = cloud_fx();
        let mut file = rules::load(&fx.paths).unwrap().file;
        file.rules[0].auto = false;
        fx.write(file);
        fx.cloud.tasks.replace(vec![signed(1, "do it")]);
        fx.pass().unwrap();
        assert!(fx.pickups().is_empty());
        assert_eq!(fx.audit_events("skipped"), 1);
    }

    #[test]
    fn a_cloud_rule_needs_a_bound_repo() {
        let fx = cloud_fx();
        fx.cloud.repos.replace(vec!["/elsewhere".into()]);
        fx.cloud.tasks.replace(vec![signed(1, "do it")]);
        fx.pass().unwrap();
        assert!(fx.pickups().is_empty());
        let events = Audit::new(&fx.paths).events(1000).unwrap();
        let detail = events
            .iter()
            .find(|e| e["event"] == "skipped")
            .and_then(|e| e["detail"].as_str().map(str::to_string))
            .unwrap();
        assert!(detail.contains("tmgr run <task>"));
    }

    #[test]
    fn unsigned_revoked_or_edited_tasks_are_skipped_and_never_started() {
        let fx = cloud_fx();
        let mut t = signed(1, "do it");
        t.refused = Some("the task has no approval for agent runs".into());
        let mut u = signed(2, "do it");
        u.refused = Some("the approval of this task was revoked".into());
        let mut v = signed(3, "do it");
        v.refused = Some("the task changed since it was approved".into());
        fx.cloud.tasks.replace(vec![t, u, v]);
        fx.pass().unwrap();
        fx.pass().unwrap();
        assert!(fx.pickups().is_empty());
        assert!(fx.runs.created.borrow().is_empty());
        assert_eq!(fx.audit_events("skipped"), 3);
        assert_eq!(fx.audit_events("started"), 0);
    }

    #[test]
    fn a_task_edited_after_it_was_queued_is_not_started() {
        let fx = Fx::new(|r| {
            r.workspace = "tmgrdev".into();
            r.project = Some("TM".into());
            r.auto = true;
            r.hours = Some("09-19".into());
        });
        fx.cloud.tasks.replace(vec![signed(1, "do it")]);
        fx.pass_at(3 * 60).unwrap();
        assert_eq!(fx.pickups()[0].status, Status::Approved);
        let mut edited = signed(1, "do it");
        edited.refused = Some("the task changed since it was approved".into());
        fx.cloud.tasks.replace(vec![edited]);
        fx.pass_at(12 * 60).unwrap();
        assert_eq!(fx.pickups()[0].status, Status::Expired);
        assert!(fx.runs.created.borrow().is_empty());
    }

    #[test]
    fn a_re_signed_text_change_voids_the_approval() {
        let fx = Fx::new(|r| {
            r.workspace = "tmgrdev".into();
            r.project = Some("TM".into());
            r.auto = true;
            r.hours = Some("09-19".into());
        });
        fx.cloud.tasks.replace(vec![signed(1, "do it")]);
        fx.pass_at(3 * 60).unwrap();
        assert_eq!(fx.pickups()[0].status, Status::Approved);
        let first = fx.pickups()[0].bundle_sha256.clone();
        fx.cloud.tasks.replace(vec![signed(1, "do something else")]);
        fx.pass_at(3 * 60).unwrap();
        assert_eq!(fx.audit_events("approval_voided"), 1);
        assert_eq!(fx.pickups()[0].status, Status::Approved);
        assert_ne!(fx.pickups()[0].bundle_sha256, first);
        fx.pass_at(12 * 60).unwrap();
        let created = fx.runs.created.borrow();
        assert_eq!(created.len(), 1);
        assert!(created[0].ends_with(&fx.pickups()[0].bundle_sha256));
    }

    #[test]
    fn a_signed_task_is_auto_approved_and_started_unattended_with_its_claim() {
        let fx = cloud_fx();
        fx.cloud.tasks.replace(vec![signed(1, "do it")]);
        fx.pass().unwrap();
        let reqs = fx.runs.requests.borrow();
        assert_eq!(reqs.len(), 1);
        assert!(reqs[0].unattended);
        assert_eq!(reqs[0].permission_mode, "bypass");
        let claim = reqs[0].cloud.clone().expect("claim");
        assert_eq!(claim.task_id, 1);
        assert_eq!(claim.workspace_id, 56);
        assert_eq!(reqs[0].clone_of, fx.repo);
        assert!(reqs[0].bundle.comments.is_empty() && reqs[0].bundle.pages.is_empty());
        assert!(reqs[0]
            .cwd
            .starts_with(&format!("{}-worktrees/queue/", fx.repo)));
        let branch = crate::publish::branch_slug("TM-1", "Task 1");
        let out = std::process::Command::new("git")
            .args(["-C", &reqs[0].cwd, "rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), branch);
        assert_eq!(fx.pickups()[0].approved_by.as_deref(), Some("rule-auto"));
    }

    fn multi_fx() -> (Fx, String) {
        let tmp = tempfile::tempdir().unwrap();
        let second = std::fs::canonicalize(tmp.path())
            .unwrap()
            .join("front")
            .to_string_lossy()
            .into_owned();
        std::fs::create_dir(&second).unwrap();
        init_repo(std::path::Path::new(&second));
        let second_for_rule = second.clone();
        let fx = Fx::new(move |r| {
            r.workspace = "tmgrdev".into();
            r.project = Some("TM".into());
            r.auto = true;
            r.mode = Some("bypass".into());
            r.repos.push(second_for_rule.clone());
        });
        std::mem::forget(tmp);
        fx.cloud
            .repos
            .replace(vec![fx.repo.clone(), second.clone()]);
        fx.cloud.claim_repos.replace(vec![
            crate::clones::ClaimRepo {
                full_name: "tmgr-dev/frontend".into(),
                path: second.clone(),
            },
            crate::clones::ClaimRepo {
                full_name: "tmgr-dev/backend".into(),
                path: fx.repo.clone(),
            },
        ]);
        (fx, second)
    }

    #[test]
    fn a_multi_pickup_prepares_every_clone_and_sends_them_with_their_identity() {
        use std::os::unix::fs::MetadataExt;
        let (fx, second) = multi_fx();
        fx.cloud.tasks.replace(vec![signed(1, "do it")]);
        fx.pass().unwrap();
        let reqs = fx.runs.requests.borrow();
        assert_eq!(reqs.len(), 1);
        let cwd = reqs[0].cwd.clone();
        assert!(cwd.starts_with(&format!("{}-worktrees/queue/", fx.repo)));
        assert_eq!(
            reqs[0].clones,
            [
                CloneSpec {
                    name: "backend".into(),
                    clone_of: fx.repo.clone()
                },
                CloneSpec {
                    name: "frontend".into(),
                    clone_of: second.clone()
                }
            ]
        );
        assert!(!std::path::Path::new(&cwd).join(".git").exists());
        let branch = crate::publish::branch_slug("TM-1", "Task 1");
        for name in ["backend", "frontend"] {
            let out = std::process::Command::new("git")
                .args([
                    "-C",
                    &format!("{cwd}/{name}"),
                    "rev-parse",
                    "--abbrev-ref",
                    "HEAD",
                ])
                .output()
                .unwrap();
            assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), branch);
        }
        let p = &fx.pickups()[0];
        assert_eq!(p.clones.len(), 2);
        for c in &p.clones {
            let m = std::fs::metadata(format!("{cwd}/{}", c.name)).unwrap();
            assert_eq!((c.dev, c.ino), (m.dev(), m.ino()));
        }
        assert_eq!(p.cloud.as_ref().unwrap().repos.len(), 2);
    }

    #[test]
    fn a_failed_second_clone_leaves_no_run_folder_and_no_run() {
        let (fx, second) = multi_fx();
        std::fs::remove_dir_all(std::path::Path::new(&second).join(".git")).unwrap();
        fx.cloud.tasks.replace(vec![signed(1, "do it")]);
        fx.pass().unwrap();
        assert!(fx.runs.requests.borrow().is_empty());
        let p = &fx.pickups()[0];
        assert_eq!(p.status, Status::Failed);
        assert!(!std::path::Path::new(&p.bundle.launch.cwd).exists());
        assert!(p.clones.is_empty());
    }

    #[test]
    fn single_repo_pickups_keep_their_state_shape() {
        let fx = cloud_fx();
        fx.cloud.tasks.replace(vec![signed(1, "do it")]);
        fx.pass().unwrap();
        let text = std::fs::read_to_string(fx.paths.state()).unwrap();
        assert!(
            !text.contains("\"clones\"") && !text.contains("\"repos\""),
            "{text}"
        );
        let reqs = fx.runs.requests.borrow();
        assert!(reqs[0].clones.is_empty());
        assert_eq!(reqs[0].clone_of, fx.repo);
    }

    #[test]
    fn publish_runs_once_after_exit_and_records_the_pr() {
        let fx = cloud_fx();
        fx.cloud.tasks.replace(vec![signed(1, "do it")]);
        fx.pass().unwrap();
        fx.pass().unwrap();
        assert!(fx.publisher.calls.borrow().is_empty());
        fx.runs.exited.borrow_mut().push("mock-1".into());
        fx.pass().unwrap();
        fx.pass().unwrap();
        fx.pass().unwrap();
        assert_eq!(fx.publisher.calls.borrow().len(), 1);
        let p = &fx.pickups()[0];
        assert_eq!(p.pr_url.as_deref(), Some("https://github.com/o/r/pull/1"));
        assert_eq!(fx.audit_events("published"), 1);
    }

    #[test]
    fn a_run_that_did_not_end_cleanly_is_never_published() {
        let fx = cloud_fx();
        fx.cloud.tasks.replace(vec![signed(1, "do it")]);
        fx.pass().unwrap();
        fx.pass().unwrap();
        fx.runs.failed.borrow_mut().push("mock-1".into());
        for _ in 0..4 {
            fx.pass().unwrap();
        }
        assert!(fx.publisher.calls.borrow().is_empty());
        let p = &fx.pickups()[0];
        assert!(p.run_exited && !p.run_clean && p.pr_url.is_none());
    }

    #[test]
    fn a_failed_publish_is_recorded_and_retried_a_bounded_number_of_times() {
        let fx = cloud_fx();
        fx.publisher.fail.set(true);
        fx.cloud.tasks.replace(vec![signed(1, "do it")]);
        fx.pass().unwrap();
        fx.runs.exited.borrow_mut().push("mock-1".into());
        for _ in 0..6 {
            fx.pass().unwrap();
        }
        assert_eq!(fx.publisher.calls.borrow().len(), 3);
        let p = &fx.pickups()[0];
        assert!(p.pr_url.is_none());
        assert_eq!(p.publish_error.as_deref(), Some("push refused"));
        assert_eq!(fx.audit_events("publish_failed"), 3);
    }

    #[test]
    fn local_pickups_are_never_published() {
        let fx = Fx::new(|_| {});
        fx.source.0.replace(vec![cand(1, "do it")]);
        fx.pass().unwrap();
        fx.approve_all();
        fx.pass().unwrap();
        fx.runs.exited.borrow_mut().push("mock-1".into());
        fx.pass().unwrap();
        assert!(fx.publisher.calls.borrow().is_empty());
        assert_eq!(fx.runs.requests.borrow()[0].cloud, None);
    }

    #[test]
    fn the_runner_is_needed_only_for_approved_or_live_pickups_or_auto_rules() {
        let fx = Fx::new(|_| {});
        assert!(!needs_runner(&fx.paths).unwrap());
        fx.source.0.replace(vec![cand(1, "do it")]);
        fx.pass().unwrap();
        assert!(!needs_runner(&fx.paths).unwrap());
        fx.approve_all();
        assert!(needs_runner(&fx.paths).unwrap());
        fx.pass().unwrap();
        assert!(needs_runner(&fx.paths).unwrap());
        let mut st = state::load(&fx.paths).unwrap();
        st.pickups[0].run_exited = true;
        st.pickups[0].run_clean = true;
        state::save(&fx.paths, &mut st).unwrap();
        assert!(!needs_runner(&fx.paths).unwrap());

        let auto = Fx::new(|r| r.auto = true);
        assert!(needs_runner(&auto.paths).unwrap());
        let mut st = state::load(&auto.paths).unwrap();
        st.paused = true;
        state::save(&auto.paths, &mut st).unwrap();
        assert!(!needs_runner(&auto.paths).unwrap());
    }

    #[test]
    fn a_start_names_the_task_and_clones_from_the_rule_repo() {
        let fx = Fx::new(|_| {});
        fx.source.0.replace(vec![cand(1, "do it")]);
        fx.pass().unwrap();
        fx.approve_all();
        fx.pass().unwrap();
        let created = fx.runs.requests.borrow();
        assert_eq!(created[0].name, "TM-1 rule:r1");
        assert_eq!(created[0].clone_of, fx.repo);
        assert_eq!(created[0].trigger, "rule:r1");
        let pin = fx.pickups()[0].profile_pin.clone().unwrap();
        assert_eq!(created[0].exe_path, pin.exe_path);
        assert_eq!(created[0].exe_sha256, pin.exe_sha256);
        let mut launched = pin.argv.clone();
        launched[0] = pin.exe_path.clone();
        assert_eq!(
            created[0].argv_sha256,
            tmgr_ipc::api::argv_sha256(&launched)
        );
    }

    #[test]
    fn an_unconfirmed_start_keeps_its_slot_until_the_key_is_looked_up() {
        let fx = Fx::new(|_| {});
        fx.source
            .0
            .replace(vec![cand(1, "do it"), cand(2, "and this")]);
        fx.pass().unwrap();
        fx.approve_all();
        fx.runs.lost.set(true);
        fx.pass().unwrap();
        let p = fx.pickups();
        assert_eq!(
            p.iter()
                .filter(|p| p.status == Status::UnknownStart)
                .count(),
            1
        );
        assert_eq!(p.iter().filter(|p| p.status == Status::Approved).count(), 1);
        assert_eq!(fx.audit_events("start_unconfirmed"), 1);
        fx.runs.lost.set(false);
        fx.pass().unwrap();
        assert!(fx.runs.created.borrow().is_empty());
        fx.runs.lookup_mode.set(2);
        fx.pass().unwrap();
        let p = fx.pickups();
        assert_eq!(p.iter().filter(|p| p.status == Status::Started).count(), 1);
        assert!(fx.runs.created.borrow().is_empty());
    }

    struct ChangesProfile<'a>(&'a Profiles);

    impl WorkDirs for ChangesProfile<'_> {
        fn prepare(&self, repo: &str, pickup_id: &str, cwd: &str) -> Result<()> {
            self.0 .1.set(self.0 .1.get() + 1);
            GitClone.prepare(repo, pickup_id, cwd)
        }
    }

    #[test]
    fn a_profile_changed_while_the_clone_is_made_stops_the_start() {
        let fx = Fx::new(|_| {});
        fx.source.0.replace(vec![cand(1, "do it")]);
        fx.pass().unwrap();
        fx.approve_all();
        run_once(
            &fx.paths,
            &Deps {
                source: &fx.source,
                cloud: &fx.cloud,
                publisher: &fx.publisher,
                runs: &fx.runs,
                profiles: &fx.profiles,
                workdirs: &ChangesProfile(&fx.profiles),
                now: Utc::now(),
                local_minutes: 12 * 60,
            },
        )
        .unwrap();
        assert!(fx.runs.created.borrow().is_empty());
        assert_eq!(fx.pickups()[0].status, Status::Failed);
        assert_eq!(fx.audit_events("start_refused"), 1);
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
        let pin = fx.profiles.pin("claude-code", None).unwrap();
        let e = approve(&fx.paths, &p.id, &"0".repeat(64), pin, Utc::now()).unwrap_err();
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
    fn a_slot_is_held_until_the_run_exits_even_when_the_task_leaves() {
        let fx = Fx::new(|r| r.auto = true);
        fx.source.0.replace(vec![cand(1, "a"), cand(2, "b")]);
        fx.pass().unwrap();
        fx.pass().unwrap();
        assert_eq!(fx.runs.created.borrow().len(), 1);
        fx.source.0.replace(vec![cand(2, "b")]);
        fx.pass().unwrap();
        assert_eq!(
            fx.runs.created.borrow().len(),
            1,
            "a leaving task must not free a live run"
        );
        fx.runs.exited.borrow_mut().push("mock-1".into());
        fx.pass().unwrap();
        assert_eq!(fx.runs.created.borrow().len(), 2);
        assert_eq!(fx.audit_events("run_exited"), 1);
    }

    #[test]
    fn a_task_that_returns_while_its_run_is_live_is_not_started_twice() {
        let fx = Fx::new(|r| {
            r.auto = true;
            r.max_concurrent = 2;
        });
        fx.source.0.replace(vec![cand(1, "a")]);
        fx.pass().unwrap();
        fx.source.0.replace(vec![]);
        fx.pass().unwrap();
        fx.source.0.replace(vec![cand(1, "a")]);
        fx.pass().unwrap();
        assert_eq!(fx.runs.created.borrow().len(), 1);
        assert_eq!(fx.pickups().len(), 1);
    }

    #[test]
    fn an_incomplete_scan_releases_nothing() {
        let fx = Fx::new(|_| {});
        fx.source.0.replace(vec![cand(1, "a")]);
        fx.pass().unwrap();
        fx.source.0.replace(vec![]);
        fx.source.1.set(false);
        fx.pass().unwrap();
        assert_eq!(fx.pickups()[0].status, Status::PendingApproval);
        assert!(!fx.pickups()[0].released);
        assert_eq!(fx.audit_events("skipped"), 1);
        fx.source.1.set(true);
        fx.pass().unwrap();
        assert_eq!(fx.pickups()[0].status, Status::Expired);
    }

    #[test]
    fn a_changed_profile_after_approval_sends_the_pickup_back_to_approval() {
        let fx = Fx::new(|_| {});
        fx.source.0.replace(vec![cand(1, "a")]);
        fx.pass().unwrap();
        fx.approve_all();
        fx.profiles.1.set(1);
        fx.pass().unwrap();
        assert!(fx.runs.created.borrow().is_empty());
        let p = &fx.pickups()[0];
        assert_eq!(p.status, Status::PendingApproval);
        assert!(p.profile_pin.is_none());
    }

    #[test]
    fn runs_start_in_a_separate_clone_beside_the_repo() {
        let fx = Fx::new(|_| {});
        fx.source.0.replace(vec![cand(1, "a")]);
        fx.pass().unwrap();
        fx.approve_all();
        fx.pass().unwrap();
        let cwd = fx.runs.cwds.borrow()[0].clone();
        assert_ne!(cwd, fx.repo);
        assert!(cwd.starts_with(&format!("{}-worktrees/queue/q_", fx.repo)));
        assert!(std::path::Path::new(&cwd).join("README.md").is_file());
        assert_eq!(fx.pickups()[0].bundle.launch.cwd, cwd);
    }

    #[test]
    fn an_oversized_bundle_is_skipped() {
        let fx = Fx::new(|_| {});
        fx.source
            .0
            .replace(vec![cand(1, &"x".repeat(MAX_BUNDLE + 1))]);
        fx.pass().unwrap();
        assert!(fx.pickups().is_empty());
        assert_eq!(fx.audit_events("skipped"), 1);
    }

    #[test]
    fn a_broken_audit_chain_stops_the_pass() {
        let fx = Fx::new(|_| {});
        let text = std::fs::read_to_string(fx.paths.audit()).unwrap();
        std::fs::write(fx.paths.audit(), text.replace("enable", "enabl3")).unwrap();
        assert_eq!(fx.pass().err().unwrap().exit_code(), 5);
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

    fn interrupted(fx: &Fx) {
        fx.source.0.replace(vec![cand(1, "a")]);
        fx.pass().unwrap();
        fx.approve_all();
        let mut st = state::load(&fx.paths).unwrap();
        st.pickups[0].status = Status::Starting;
        state::save(&fx.paths, &mut st).unwrap();
    }

    #[test]
    fn an_interrupted_start_keeps_its_slot_and_is_never_retried() {
        let fx = Fx::new(|_| {});
        interrupted(&fx);
        fx.source.0.replace(vec![cand(1, "a"), cand(2, "b")]);
        fx.pass().unwrap();
        fx.approve_all();
        fx.pass().unwrap();
        assert_eq!(fx.pickups()[0].status, Status::UnknownStart);
        assert!(
            fx.runs.created.borrow().is_empty(),
            "the slot is still taken"
        );
        assert_eq!(fx.audit_events("start_interrupted"), 1);
        let mut st = state::load(&fx.paths).unwrap();
        st.pickups[0].updated_at = "2020-01-01T00:00:00Z".into();
        state::save(&fx.paths, &mut st).unwrap();
        fx.pass().unwrap();
        assert_eq!(fx.pickups()[0].status, Status::Failed);
        assert_eq!(fx.runs.created.borrow().len(), 1);
    }

    #[test]
    fn a_start_found_by_its_key_is_reconciled_to_started() {
        let fx = Fx::new(|_| {});
        interrupted(&fx);
        fx.runs.lookup_mode.set(2);
        fx.pass().unwrap();
        let p = &fx.pickups()[0];
        assert_eq!(p.status, Status::Started);
        assert!(p.run_id.as_deref().unwrap().starts_with("found-q_"));
        assert!(p.run_id.as_deref().unwrap().ends_with("-1"));
    }

    #[test]
    fn a_start_that_was_never_created_is_failed_and_frees_the_slot() {
        let fx = Fx::new(|_| {});
        interrupted(&fx);
        fx.runs.lookup_mode.set(1);
        fx.pass().unwrap();
        assert_eq!(fx.pickups()[0].status, Status::Failed);
    }

    #[test]
    fn release_frees_an_unknown_start_and_is_audited() {
        let fx = Fx::new(|_| {});
        interrupted(&fx);
        fx.pass().unwrap();
        let id = fx.pickups()[0].id.clone();
        release(&fx.paths, &id, Utc::now()).unwrap();
        assert_eq!(fx.pickups()[0].status, Status::Failed);
        assert_eq!(fx.audit_events("start_released"), 1);
        assert!(release(&fx.paths, &id, Utc::now()).is_err());
    }

    #[test]
    fn every_start_carries_a_stable_key() {
        let fx = Fx::new(|r| r.auto = true);
        fx.source.0.replace(vec![cand(1, "a")]);
        fx.pass().unwrap();
        let id = fx.pickups()[0].id.clone();
        assert_eq!(fx.runs.keys.borrow()[0], format!("{id}-1"));
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
        assert!(fx.pickups()[0]
            .bundle
            .launch
            .cwd
            .starts_with(&format!("{}-worktrees/queue/", fx.repo)));
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
