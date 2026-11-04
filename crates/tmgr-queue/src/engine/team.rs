use super::{check_rule, pin_of, ts, Deps, Pass};
use crate::audit::{sha256_hex, Audit};
use crate::paths::Paths;
use crate::rules::{ProfilePin, Rule};
use crate::runs::{mode_name, Lookup, RunState, StartRequest, PERMISSION_MODE};
use crate::state::{self, Member, Pickup, Status, TeamPhase, TeamRequest, TeamState};
use crate::workdir::{clone_tip, is_sha, open_beneath, unlink_beneath, worker_branch, worker_dir};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use std::io::Read;
use std::path::Path;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_ipc::api::{
    BundleTeam, TaskRef, TeamLink, TeamMode, TeamRole, MAX_ASSIGNMENT_BYTES, MAX_TEAM_SIZE,
};

pub const PLAN_FILE: &str = ".tmgr/team-plan.json";
pub const DONE_FILE: &str = ".tmgr/done";
pub const TEAM_PROFILE: &str = "claude-code";
const MAX_REJECTIONS: u8 = 3;
const EXIT_TEXT: &str = "/exit\r";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanFile {
    v: u32,
    workers: Vec<PlanWorker>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanWorker {
    brief: String,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Plan {
    Missing,
    Invalid(String),
    Ready(Vec<String>),
}

enum Raw {
    Missing,
    Unusable,
    Bytes(Vec<u8>),
}

fn read_capped(root: &Path, rel: &str, cap: u64) -> Raw {
    let Some(file) = open_beneath(root, rel, libc::O_NONBLOCK) else {
        return if std::fs::symlink_metadata(root.join(rel)).is_ok() {
            Raw::Unusable
        } else {
            Raw::Missing
        };
    };
    let Ok(meta) = file.metadata() else {
        return Raw::Unusable;
    };
    if !meta.file_type().is_file() || meta.len() > cap {
        return Raw::Unusable;
    }
    let mut bytes = Vec::new();
    match file.take(cap + 1).read_to_end(&mut bytes) {
        Ok(_) if bytes.len() as u64 <= cap => Raw::Bytes(bytes),
        _ => Raw::Unusable,
    }
}

/// Reads the lead's plan as data: a regular file beneath the clone, size capped, strict fields.
pub fn read_plan(lead_cwd: &Path, size: u8) -> Plan {
    let cap = u64::from(size) * MAX_ASSIGNMENT_BYTES as u64 + 4096;
    let bytes = match read_capped(lead_cwd, PLAN_FILE, cap) {
        Raw::Missing => return Plan::Missing,
        Raw::Unusable => return Plan::Invalid("unusable".into()),
        Raw::Bytes(b) => b,
    };
    let fingerprint = sha256_hex(&bytes);
    let Ok(plan) = serde_json::from_slice::<PlanFile>(&bytes) else {
        return Plan::Invalid(fingerprint);
    };
    let fits = plan.v == 1
        && plan.workers.len() <= usize::from(size)
        && plan
            .workers
            .iter()
            .all(|w| !w.brief.trim().is_empty() && w.brief.len() <= MAX_ASSIGNMENT_BYTES);
    if !fits {
        return Plan::Invalid(fingerprint);
    }
    Plan::Ready(plan.workers.into_iter().map(|w| w.brief).collect())
}

/// Whether the agent left its finish marker. Only its presence counts; the content is never read.
pub fn marker_present(cwd: &Path) -> bool {
    open_beneath(cwd, DONE_FILE, libc::O_NONBLOCK)
        .and_then(|f| f.metadata().ok())
        .is_some_and(|m| m.file_type().is_file())
}

fn clear_marker(cwd: &Path) -> bool {
    !marker_present(cwd) || (unlink_beneath(cwd, DONE_FILE) && !marker_present(cwd))
}

fn refused_text(size: u8) -> String {
    format!(
        "The team plan was refused and nothing was started. Write .tmgr/team-plan.json again as a JSON object with \"v\" set to 1 and \"workers\" a list of at most {size} objects, each with only a \"brief\" of non-empty text, then stop."
    )
}

fn integrate_text(tips: &[(u8, String)], failed: &[u8]) -> String {
    let mut text = String::new();
    if tips.is_empty() {
        text.push_str("No worker left commits to bring in. ");
    } else {
        let list: Vec<String> = tips.iter().map(|(k, sha)| format!("w{k} {sha}")).collect();
        text.push_str(&format!(
            "All workers have finished. Their work is saved as these commits: {}. For each one run `git merge --squash <sha>` and commit the result as one normal commit; resolve conflicts yourself and never create merge commits. ",
            list.join(", ")
        ));
    }
    if !failed.is_empty() {
        let list: Vec<String> = failed.iter().map(|k| format!("w{k}")).collect();
        text.push_str(&format!("{} did not finish cleanly. ", list.join(", ")));
    }
    text.push_str(
        "When everything is committed on your branch, write the file .tmgr/done and stop.",
    );
    text
}

pub(super) fn refusal_for(rule: &Rule, p: &Pickup) -> Option<&'static str> {
    if rule.profile != TEAM_PROFILE {
        return Some("a team needs the claude-code profile");
    }
    if rule.repos.len() != 1 || p.cloud.as_ref().is_some_and(|c| !c.repos.is_empty()) {
        return Some("a team works in one repository");
    }
    None
}

/// The pin of a team member: the approved binary and profile with the team's model.
pub(super) fn member_pin(
    rule: &Rule,
    cloud: bool,
    deps: &Deps,
    model: &str,
    approved: &ProfilePin,
) -> std::result::Result<ProfilePin, String> {
    let mut r = rule.clone();
    r.model = Some(model.to_string());
    let pin = pin_of(&r, cloud, deps)?;
    if pin.exe_path == approved.exe_path
        && pin.exe_sha256 == approved.exe_sha256
        && pin.profile_sha256 == approved.profile_sha256
    {
        Ok(pin)
    } else {
        Err("the agent profile changed since the approval".into())
    }
}

pub(super) fn request_of(st: &state::State, task: TaskRef) -> Option<TeamRequest> {
    st.team_requests.iter().find(|r| r.task == task).cloned()
}

/// Turns an approved pickup into a team run: the pickup's bundle now describes the lead, and the request is used up.
pub(super) fn commit_team(pass: &mut Pass, i: usize, req: &TeamRequest) -> &'static str {
    let (lead_model, _) = state::team_models(req.mode);
    let p = &mut pass.state.pickups[i];
    let lead = crate::workdir::lead_dir(Path::new(&p.bundle.launch.cwd));
    p.bundle.launch.cwd = lead.to_string_lossy().into_owned();
    p.bundle.launch.model = Some(lead_model.to_string());
    p.bundle.team = Some(BundleTeam {
        size: req.size,
        mode: req.mode,
    });
    p.bundle_sha256 = p.bundle.sha256();
    p.team = Some(TeamState::new(req.size, req.mode));
    let task = p.task;
    pass.state.team_requests.retain(|r| r.task != task);
    lead_model
}

pub(super) fn refuse_start(pass: &mut Pass, i: usize, reason: &str) -> Result<()> {
    let task = pass.state.pickups[i].task;
    let id = pass.state.pickups[i].id.clone();
    pass.state.team_requests.retain(|r| r.task != task);
    pass.set(i, Status::Failed, Some(format!("team refused: {reason}")));
    pass.audit.append(
        "start_refused",
        json!({"pickup": id, "reason": "team_refused", "detail": reason}),
    )?;
    let p = pass.state.pickups[i].clone();
    pass.event(&p, "failed");
    Ok(())
}

pub(crate) const KILL_GRACE_SECS: i64 = 30;

fn kill_is_overdue(sent_at: Option<&str>, now: DateTime<Utc>) -> bool {
    sent_at
        .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
        .is_some_and(|t| (now - t.with_timezone(&Utc)).num_seconds() >= KILL_GRACE_SECS)
}

fn kill_workers(pass: &mut Pass, i: usize, deps: &Deps, why: &str) -> Result<()> {
    let id = pass.state.pickups[i].id.clone();
    let members = pass.state.pickups[i]
        .team
        .as_ref()
        .map(|t| t.members.clone())
        .unwrap_or_default();
    for m in members.iter().filter(|m| !m.exited) {
        let Some(run) = &m.run_id else { continue };
        let force = kill_is_overdue(m.kill_sent_at.as_deref(), deps.now);
        if deps.runs.kill_run(run, force, why).is_err() || m.kill_sent_at.is_some() {
            continue;
        }
        if let Some(slot) = member_mut(pass, i, &m.start_key) {
            slot.kill_sent_at = Some(ts(deps.now));
        }
        pass.audit.append(
            "team_member_killed",
            json!({"pickup": id, "member": m.start_key}),
        )?;
    }
    state::save(pass.paths, &mut pass.state)
}

fn refresh_members(pass: &mut Pass, i: usize, deps: &Deps) -> Result<()> {
    let id = pass.state.pickups[i].id.clone();
    let members = pass.state.pickups[i]
        .team
        .as_ref()
        .map(|t| t.members.clone())
        .unwrap_or_default();
    for m in members.iter().filter(|m| !m.exited) {
        let (run_id, exited, clean) = match &m.run_id {
            Some(run) => match deps.runs.status(run) {
                RunState::Succeeded => (m.run_id.clone(), true, true),
                RunState::Exited => (m.run_id.clone(), true, false),
                _ => continue,
            },
            None => match deps.runs.lookup(&m.start_key) {
                Lookup::Found(run) => (Some(run), false, false),
                Lookup::NotFound => (None, true, false),
                Lookup::Unsupported => continue,
            },
        };
        if let Some(slot) = member_mut(pass, i, &m.start_key) {
            slot.run_id = run_id;
            slot.exited = exited;
            slot.clean = clean;
        }
        if exited {
            pass.audit.append(
                "team_member_exited",
                json!({"pickup": id, "member": m.start_key, "clean": clean}),
            )?;
        }
    }
    Ok(())
}

fn team_mut<'a>(pass: &'a mut Pass, i: usize) -> &'a mut TeamState {
    pass.state.pickups[i].team.as_mut().expect("team")
}

fn member_mut<'a>(pass: &'a mut Pass, i: usize, start_key: &str) -> Option<&'a mut Member> {
    team_mut(pass, i)
        .members
        .iter_mut()
        .find(|m| m.start_key == start_key)
}

fn phase_of(pass: &Pass, i: usize) -> TeamPhase {
    pass.state.pickups[i].team.as_ref().expect("team").phase
}

pub(super) fn team_phase(pass: &mut Pass, rules: &[Rule], deps: &Deps, starts: bool) -> Result<()> {
    for i in 0..pass.state.pickups.len() {
        let p = &pass.state.pickups[i];
        let running = p.status == Status::Started && p.run_id.is_some();
        if p.team.is_none() || !(running || p.has_live_team()) {
            continue;
        }
        refresh_members(pass, i, deps)?;
        let p = pass.state.pickups[i].clone();
        let t = p.team.clone().expect("team");
        let Some(lead_run) = p.run_id.clone().filter(|_| p.status == Status::Started) else {
            kill_workers(pass, i, deps, "the pickup of the team is no longer running")?;
            continue;
        };
        if p.run_exited {
            let early = !(t.phase == TeamPhase::Finishing && t.lead_exit_sent);
            if early && p.run_clean {
                pass.state.pickups[i].run_clean = false;
                pass.audit
                    .append("team_lead_ended_early", json!({"pickup": p.id}))?;
            }
            kill_workers(pass, i, deps, "the lead of the team ended")?;
            continue;
        }
        if phase_of(pass, i) == TeamPhase::Planning {
            planning(pass, i, deps, &lead_run)?;
        }
        if phase_of(pass, i) == TeamPhase::Working {
            working(pass, i, rules, deps, starts)?;
        }
        if phase_of(pass, i) == TeamPhase::Integrating {
            integrating(pass, i, rules, deps, &lead_run)?;
        }
        if phase_of(pass, i) == TeamPhase::Finishing {
            finishing(pass, i, deps, &lead_run)?;
        }
    }
    Ok(())
}

pub(super) fn team_stop_phase(pass: &mut Pass, deps: &Deps) -> Result<()> {
    for i in 0..pass.state.pickups.len() {
        let p = &pass.state.pickups[i];
        let lead_runs = p.status == Status::Started && p.run_id.is_some() && !p.run_exited;
        if !p.has_live_team() || lead_runs {
            continue;
        }
        refresh_members(pass, i, deps)?;
        kill_workers(
            pass,
            i,
            deps,
            "the queue is off and the lead of the team is gone",
        )?;
    }
    Ok(())
}

fn planning(pass: &mut Pass, i: usize, deps: &Deps, lead_run: &str) -> Result<()> {
    if !deps.runs.idle(lead_run) {
        return Ok(());
    }
    let p = pass.state.pickups[i].clone();
    let size = p.team.as_ref().expect("team").size;
    match read_plan(Path::new(&p.bundle.launch.cwd), size) {
        Plan::Missing => return Ok(()),
        Plan::Invalid(fingerprint) => {
            if p.team.as_ref().and_then(|t| t.rejected.as_deref()) == Some(fingerprint.as_str()) {
                return Ok(());
            }
            let t = team_mut(pass, i);
            t.rejected = Some(fingerprint);
            t.rejections += 1;
            let rejections = t.rejections;
            pass.audit
                .append("team_plan_refused", json!({"pickup": p.id}))?;
            if rejections == MAX_REJECTIONS {
                let _ = deps
                    .runs
                    .kill_run(lead_run, false, "the team plan was refused repeatedly");
            } else if rejections < MAX_REJECTIONS {
                let _ = deps
                    .runs
                    .send(lead_run, &format!("{}\r", refused_text(size)));
            }
        }
        Plan::Ready(briefs) => {
            let count = briefs.len();
            let t = team_mut(pass, i);
            t.briefs = briefs;
            t.phase = if count == 0 {
                TeamPhase::Integrating
            } else {
                TeamPhase::Working
            };
            pass.audit.append(
                "team_plan_accepted",
                json!({"pickup": p.id, "workers": count}),
            )?;
        }
    }
    state::save(pass.paths, &mut pass.state)
}

fn working(pass: &mut Pass, i: usize, rules: &[Rule], deps: &Deps, starts: bool) -> Result<()> {
    let p = pass.state.pickups[i].clone();
    let t = p.team.clone().expect("team");
    for m in t.members.iter().filter(|m| !m.exited && !m.exit_sent) {
        let Some(run) = &m.run_id else { continue };
        if marker_present(Path::new(&m.cwd))
            && deps.runs.idle(run)
            && deps.runs.send(run, EXIT_TEXT).is_ok()
        {
            if let Some(slot) = member_mut(pass, i, &m.start_key) {
                slot.exit_sent = true;
            }
        }
    }
    let usable = rules
        .iter()
        .find(|r| r.name == p.rule)
        .filter(|r| check_rule(r, deps).is_ok());
    loop {
        let t = pass.state.pickups[i].team.as_ref().expect("team");
        let k = t.started_workers + 1;
        if usize::from(t.started_workers) >= t.briefs.len() || k > t.size {
            break;
        }
        match (usable, starts) {
            (Some(rule), true) => start_worker(pass, i, k, rule, deps)?,
            (None, _) => fail_unstarted(pass, i, k, "the rule is no longer usable")?,
            (Some(_), false) => break,
        }
    }
    let t = pass.state.pickups[i].team.as_ref().expect("team");
    if usize::from(t.started_workers) == t.briefs.len() && t.all_exited() {
        team_mut(pass, i).phase = TeamPhase::Integrating;
    }
    state::save(pass.paths, &mut pass.state)
}

fn new_member(p: &Pickup, k: u8) -> Member {
    let team_dir = Path::new(&p.bundle.launch.cwd)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    Member {
        role: TeamRole::Worker(k),
        cwd: worker_dir(&team_dir, k).to_string_lossy().into_owned(),
        branch: worker_branch(&p.id, k),
        start_key: format!("{}-w{k}", p.start_key()),
        run_id: None,
        exited: false,
        clean: false,
        base: None,
        tip: None,
        exit_sent: false,
        kill_sent_at: None,
    }
}

fn fail_unstarted(pass: &mut Pass, i: usize, k: u8, why: &str) -> Result<()> {
    let p = pass.state.pickups[i].clone();
    let mut m = new_member(&p, k);
    m.exited = true;
    let t = team_mut(pass, i);
    t.members.push(m);
    t.started_workers = k;
    pass.audit.append(
        "team_worker_failed",
        json!({"pickup": p.id, "worker": k, "reason": why}),
    )
}

fn fail_member(pass: &mut Pass, i: usize, m: &Member, stage: &str, error: &str) -> Result<()> {
    if let Some(slot) = member_mut(pass, i, &m.start_key) {
        slot.exited = true;
    }
    let id = pass.state.pickups[i].id.clone();
    pass.audit.append(
        "team_worker_failed",
        json!({"pickup": id, "member": m.start_key, "stage": stage, "error": error}),
    )
}

fn start_worker(pass: &mut Pass, i: usize, k: u8, rule: &Rule, deps: &Deps) -> Result<()> {
    let p = pass.state.pickups[i].clone();
    let t = p.team.clone().expect("team");
    let mut m = new_member(&p, k);
    let slot = team_mut(pass, i);
    slot.members.push(m.clone());
    slot.started_workers = k;
    state::save(pass.paths, &mut pass.state)?;
    let (_, worker_model) = state::team_models(t.mode);
    let cloud = p.cloud.is_some();
    let Some(approved) = p.profile_pin.clone() else {
        return fail_member(pass, i, &m, "pin", "no approved profile");
    };
    let pin = match member_pin(rule, cloud, deps, worker_model, &approved) {
        Ok(pin) => pin,
        Err(e) => return fail_member(pass, i, &m, "pin", &e),
    };
    let repo = &rule.repos[0];
    if let Err(e) = deps.workdirs.prepare_member(repo, &p.id, &m.cwd, &m.branch) {
        return fail_member(pass, i, &m, "clone", &e.message);
    }
    m.base = clone_tip(Path::new(&m.cwd), &m.branch);
    if let Some(slot) = member_mut(pass, i, &m.start_key) {
        slot.base = m.base.clone();
    }
    let mut bundle = p.bundle.clone();
    bundle.launch.cwd = m.cwd.clone();
    bundle.launch.model = Some(worker_model.to_string());
    bundle.team = None;
    bundle.assignment = Some(t.briefs[usize::from(k) - 1].clone());
    if let Err(e) = bundle.validate() {
        return fail_member(pass, i, &m, "bundle", &e);
    }
    let req = StartRequest {
        permission_mode: if cloud {
            mode_name(rule.mode.as_deref())
        } else {
            PERMISSION_MODE
        },
        start_key: m.start_key.clone(),
        rule: rule.name.clone(),
        trigger: format!("rule:{}", rule.name),
        profile: rule.profile.clone(),
        model: Some(worker_model.to_string()),
        cwd: m.cwd.clone(),
        clone_of: repo.clone(),
        exe_path: pin.exe_path.clone(),
        exe_sha256: pin.exe_sha256.clone(),
        argv_sha256: pin.argv_sha256(),
        name: format!("{} w{k}", p.key).chars().take(64).collect(),
        task: p.task,
        bundle_sha256: bundle.sha256(),
        bundle,
        cloud: p.cloud.as_ref().map(|c| c.claim.clone()),
        unattended: cloud,
        clones: Vec::new(),
        team: Some(TeamLink {
            role: TeamRole::Worker(k),
            of: p.id.clone(),
        }),
    };
    match deps.runs.create(&req) {
        Ok(run) => {
            if let Some(slot) = member_mut(pass, i, &m.start_key) {
                slot.run_id = Some(run.clone());
            }
            pass.audit.append(
                "team_worker_started",
                json!({"pickup": p.id, "worker": k, "run": run}),
            )
        }
        Err(e) if e.kind == Kind::Offline => pass.audit.append(
            "team_worker_unconfirmed",
            json!({"pickup": p.id, "worker": k, "error": e.message}),
        ),
        Err(e) => fail_member(pass, i, &m, "start", &e.message),
    }
}

fn integrating(
    pass: &mut Pass,
    i: usize,
    rules: &[Rule],
    deps: &Deps,
    lead_run: &str,
) -> Result<()> {
    let p = pass.state.pickups[i].clone();
    let t = p.team.clone().expect("team");
    let Some(rule) = rules.iter().find(|r| r.name == p.rule) else {
        return Ok(());
    };
    let mut tips: Vec<(u8, String)> = Vec::new();
    let mut failed: Vec<u8> = Vec::new();
    for m in &t.members {
        let TeamRole::Worker(k) = m.role else {
            continue;
        };
        if !m.clean {
            failed.push(k);
            continue;
        }
        let cwd = Path::new(&m.cwd);
        let tip = clone_tip(cwd, &m.branch);
        if tip.is_none() || tip == m.base {
            continue;
        }
        match crate::workdir::import_branch(&rule.repos[0], cwd, &m.branch) {
            Ok(sha) if is_sha(&sha) => tips.push((k, sha)),
            Ok(_) => failed.push(k),
            Err(e) => {
                pass.audit.append(
                    "team_import_failed",
                    json!({"pickup": p.id, "worker": k, "error": e.message}),
                )?;
                failed.push(k);
            }
        }
    }
    let text = integrate_text(&tips, &failed);
    if !deps.runs.idle(lead_run)
        || !clear_marker(Path::new(&p.bundle.launch.cwd))
        || deps.runs.send(lead_run, &format!("{text}\r")).is_err()
    {
        return Ok(());
    }
    for (k, sha) in &tips {
        if let Some(x) = team_mut(pass, i)
            .members
            .iter_mut()
            .find(|x| x.role == TeamRole::Worker(*k))
        {
            x.tip = Some(sha.clone());
        }
    }
    team_mut(pass, i).phase = TeamPhase::Finishing;
    pass.audit.append(
        "team_integrating",
        json!({"pickup": p.id, "commits": tips.len(), "failed": failed.len()}),
    )?;
    state::save(pass.paths, &mut pass.state)
}

fn finishing(pass: &mut Pass, i: usize, deps: &Deps, lead_run: &str) -> Result<()> {
    let p = &pass.state.pickups[i];
    let t = p.team.as_ref().expect("team");
    if t.lead_exit_sent
        || !marker_present(Path::new(&p.bundle.launch.cwd))
        || !deps.runs.idle(lead_run)
        || deps.runs.send(lead_run, EXIT_TEXT).is_err()
    {
        return Ok(());
    }
    team_mut(pass, i).lead_exit_sent = true;
    state::save(pass.paths, &mut pass.state)
}

pub fn request_team(
    paths: &Paths,
    task: TaskRef,
    size: u8,
    mode: TeamMode,
    now: DateTime<Utc>,
) -> Result<()> {
    if !(1..=MAX_TEAM_SIZE).contains(&size) {
        return Err(Error::usage(format!(
            "a team has 1 to {MAX_TEAM_SIZE} workers"
        )));
    }
    let _lock = paths.lock()?;
    let mut st = state::load(paths)?;
    st.team_requests.retain(|r| r.task != task);
    st.team_requests.push(TeamRequest {
        task,
        size,
        mode,
        at: ts(now),
    });
    Audit::new(paths).append(
        "team_requested",
        json!({"task": task.task_id, "size": size, "mode": mode.as_str()}),
    )?;
    state::save(paths, &mut st)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmgr_ipc::api::TeamMode;

    fn task(id: i64) -> TaskRef {
        TaskRef {
            workspace_id: -7,
            task_id: id,
        }
    }

    #[test]
    fn a_new_request_replaces_the_earlier_one_for_that_task_only() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::at(tmp.path());
        let now = Utc::now();
        request_team(&paths, task(1), 2, TeamMode::Eco, now).unwrap();
        request_team(&paths, task(2), 3, TeamMode::Eco, now).unwrap();
        request_team(&paths, task(1), 4, TeamMode::Heavy, now).unwrap();
        let st = state::load(&paths).unwrap();
        assert_eq!(st.team_requests.len(), 2);
        let one = st.team_requests.iter().find(|r| r.task == task(1)).unwrap();
        assert_eq!((one.size, one.mode), (4, TeamMode::Heavy));
    }

    #[test]
    fn a_size_outside_one_to_five_is_refused_and_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::at(tmp.path());
        for size in [0, 6] {
            assert!(request_team(&paths, task(1), size, TeamMode::Eco, Utc::now()).is_err());
        }
        assert!(state::load(&paths).unwrap().team_requests.is_empty());
    }

    #[test]
    fn a_state_without_requests_or_teams_serializes_as_before() {
        let text = serde_json::to_string(&state::State::default()).unwrap();
        assert!(!text.contains("team"));
    }
}
