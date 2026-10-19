use super::super::model::{NotPlanned, PlanDoc, PlanRow, PlanState, Slots, TaskBrief, TriageRow};
use super::super::orchestrate::{
    self, ModeSetting, OrchInput, Plan, PlanItem, Poll, TeamMode, TriageItem, Verdict,
};
use super::decide::{verdict, Decision, Facts};
use super::ports::PlanFresh;
use super::{Autopilot, MAX_PASS_ERRORS};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use tmgr_core::text::sanitize_line;
use tmgr_ipc::api::BundleTeam;
use tmgr_queue::audit::sha256_hex;
use tmgr_queue::{StartPolicy, TaskPlan};
use tmgr_usage::Tool;

pub(super) const SESSION_TIMEOUT_MINUTES: i64 = 20;
pub(super) const MAX_ATTEMPTS: u32 = 2;
pub(super) const NOTHING_TO_PLAN: &str = "nothing to plan";
const KINDS_REFUSED: [&str; 3] = ["completed", "archived", "review"];

#[derive(Clone, Debug)]
pub(super) enum Phase {
    Off,
    Pending,
    Running {
        run: String,
        dir: PathBuf,
        since: DateTime<Utc>,
    },
    Done,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct Seen {
    pub task_id: i64,
    pub key: String,
    pub title: String,
    pub sha256: String,
    pub status_id: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct Frozen {
    pub input_sha256: String,
    pub made_at: DateTime<Utc>,
    pub attempt: u32,
    pub seen: Vec<Seen>,
    pub items: Vec<PlanItem>,
    pub triage: Vec<TriageItem>,
    pub rows: Vec<TriageRow>,
    pub not_planned: Vec<NotPlanned>,
    pub applied: bool,
}

pub(super) enum Flow {
    Wait,
    Go,
    Stop(String),
}

pub(super) struct Orch {
    pub phase: Phase,
    pub attempt: u32,
    pub errors: u32,
    pub input: Option<OrchInput>,
    pub input_sha: String,
    pub seen: Vec<Seen>,
    pub not_planned: Vec<NotPlanned>,
    pub plan: Option<Plan>,
    pub made_at: Option<DateTime<Utc>>,
    pub rows: Vec<TriageRow>,
    pub applied: bool,
}

impl Orch {
    pub fn off() -> Orch {
        Orch {
            phase: Phase::Off,
            attempt: 0,
            errors: 0,
            input: None,
            input_sha: String::new(),
            seen: Vec::new(),
            not_planned: Vec::new(),
            plan: None,
            made_at: None,
            rows: Vec::new(),
            applied: false,
        }
    }

    pub fn on(&self) -> bool {
        !matches!(self.phase, Phase::Off)
    }

    pub fn accepted(&self) -> bool {
        matches!(self.phase, Phase::Off | Phase::Done)
    }

    pub fn frozen(&self) -> Option<Frozen> {
        let plan = self.plan.as_ref()?;
        Some(Frozen {
            input_sha256: self.input_sha.clone(),
            made_at: self.made_at?,
            attempt: self.attempt,
            seen: self.seen.clone(),
            items: plan.items.clone(),
            triage: plan.triage.clone(),
            rows: self.rows.clone(),
            not_planned: self.not_planned.clone(),
            applied: self.applied,
        })
    }

    pub fn restore(&mut self, f: Frozen) {
        self.phase = Phase::Done;
        self.attempt = f.attempt;
        self.input_sha = f.input_sha256;
        self.made_at = Some(f.made_at);
        self.seen = f.seen;
        self.not_planned = f.not_planned;
        self.rows = f.rows;
        self.applied = f.applied;
        self.plan = Some(Plan {
            items: f.items,
            triage: f.triage,
        });
    }

    fn triaged(&self) -> BTreeSet<i64> {
        self.plan
            .iter()
            .flat_map(|p| p.triage.iter().map(|t| t.task_id))
            .collect()
    }

    pub fn excluded(&self) -> BTreeSet<i64> {
        let mut ids = self.triaged();
        ids.extend(self.not_planned.iter().map(|n| n.task_id));
        ids
    }

    pub fn applied_ids(&self, verdict: &str) -> BTreeSet<i64> {
        self.rows
            .iter()
            .filter(|r| r.applied && r.verdict == verdict)
            .map(|r| r.task_id)
            .collect()
    }
}

fn verdict_key(v: Verdict) -> &'static str {
    match v {
        Verdict::NeedsFeedback => "needs_feedback",
        Verdict::Blocked => "blocked",
    }
}

fn short(msg: &str) -> String {
    sanitize_line(msg).chars().take(200).collect()
}

impl Autopilot<'_> {
    pub(super) fn team_size(&self) -> u32 {
        if self.tool == Some(Tool::Codex) {
            1
        } else {
            self.s.answers.team_size.clamp(1, 5)
        }
    }

    fn forced_mode(&self) -> Option<TeamMode> {
        match self.s.answers.team_mode {
            ModeSetting::Eco => Some(TeamMode::Eco),
            ModeSetting::Heavy => Some(TeamMode::Heavy),
            ModeSetting::Orchestrator => None,
        }
    }

    pub(super) fn team_of(&self, task_id: i64) -> Option<BundleTeam> {
        let item = self
            .orch
            .plan
            .as_ref()?
            .items
            .iter()
            .find(|i| i.task_id == task_id)?;
        Some(BundleTeam {
            size: self.team_size() as u8,
            mode: item.mode,
        })
    }

    pub(super) fn init_phase(&mut self) {
        if self.queue_mode {
            return;
        }
        self.orch.phase = if self.p.planner.enabled(&self.s.rule) {
            Phase::Pending
        } else if self.s.dry_run {
            Phase::Failed("the orchestrator needs a rule of a cloud workspace".into())
        } else {
            Phase::Off
        };
    }

    pub(super) fn kill_phase(&mut self) {
        let Phase::Running { run, .. } = self.orch.phase.clone() else {
            return;
        };
        if let Err(e) = self.p.planner.kill(&run) {
            let text = format!("Could not stop the orchestrator: {}", short(&e.message));
            self.event("error", None, text);
        }
        self.orch.attempt = self.orch.attempt.saturating_sub(1);
        self.orch.phase = Phase::Pending;
        self.event("plan", None, "The orchestrator session was stopped");
    }

    pub(super) fn drive_phase(
        &mut self,
        decision: &Decision,
        facts: &Facts,
        now: DateTime<Utc>,
    ) -> Flow {
        match self.orch.phase.clone() {
            Phase::Off | Phase::Done => Flow::Go,
            Phase::Failed(text) => Flow::Stop(text),
            Phase::Pending => {
                if !decision.starts_allowed() {
                    return match verdict(decision, facts, now) {
                        Some(r) => Flow::Stop(r.text()),
                        None => Flow::Wait,
                    };
                }
                self.start_session(now)
            }
            Phase::Running { run, dir, since } => self.poll_session(&run, &dir, since, now),
        }
    }

    fn fail(&mut self, text: String) -> Flow {
        self.orch.phase = Phase::Failed(text.clone());
        self.push("Autopilot: plan failed", &text);
        Flow::Stop(text)
    }

    fn start_session(&mut self, now: DateTime<Utc>) -> Flow {
        if self.orch.input.is_none() {
            if let Some(flow) = self.prepare_input() {
                return flow;
            }
        }
        let attempt = self.orch.attempt + 1;
        let digest =
            sha256_hex(format!("{}:{attempt}:{}", self.s.id, now.timestamp_millis()).as_bytes());
        let nonce = format!("{attempt}-{}", &digest[..24]);
        let Some(input) = self.orch.input.clone() else {
            return Flow::Wait;
        };
        self.orch.attempt = attempt;
        match self.p.planner.start(&self.s.rule, &input, &nonce) {
            Ok((run, dir)) => {
                self.orch.phase = Phase::Running {
                    run,
                    dir,
                    since: now,
                };
                self.event(
                    "plan",
                    None,
                    format!(
                        "Orchestrator started for {} task(s) (attempt {attempt})",
                        input.tasks.len()
                    ),
                );
                Flow::Wait
            }
            Err(e) => self.attempt_failed(&short(&e.message), now),
        }
    }

    fn prepare_input(&mut self) -> Option<Flow> {
        let inputs = match self.p.planner.inputs(&self.s.rule, &self.s.tasks) {
            Ok(i) => i,
            Err(e) => {
                self.orch.errors += 1;
                let text = format!("Could not read the task texts: {}", short(&e.message));
                self.event("error", None, text);
                if self.orch.errors >= MAX_PASS_ERRORS {
                    return Some(self.fail(format!(
                        "orchestrator input unavailable: {}",
                        short(&e.message)
                    )));
                }
                return Some(Flow::Wait);
            }
        };
        self.orch.not_planned = inputs
            .not_planned
            .iter()
            .map(|(id, key, reason)| NotPlanned {
                task: key.clone(),
                task_id: *id,
                reason: short(reason),
            })
            .collect();
        for n in self.orch.not_planned.clone() {
            let text = format!("{} is not planned: {}", n.task, n.reason);
            self.event("skipped", Some(&n.task), text);
        }
        if inputs.tasks.is_empty() {
            self.orch.phase = Phase::Failed(NOTHING_TO_PLAN.into());
            return Some(Flow::Stop(NOTHING_TO_PLAN.into()));
        }
        let keys: BTreeMap<i64, String> = inputs
            .tasks
            .iter()
            .map(|p| (p.text.task_id, p.text.key.clone()))
            .collect();
        let order: Vec<String> = self
            .s
            .order
            .iter()
            .filter_map(|id| keys.get(id).cloned())
            .collect();
        self.orch.seen = inputs
            .tasks
            .iter()
            .map(|p| Seen {
                task_id: p.text.task_id,
                key: p.text.key.clone(),
                title: p.text.title.clone(),
                sha256: p.content_sha256.clone(),
                status_id: p.status_id,
            })
            .collect();
        let texts = inputs.tasks.into_iter().map(|p| p.text).collect();
        match OrchInput::new(
            self.s.answers.max_teams,
            self.team_size(),
            self.s.answers.team_mode,
            &order,
            texts,
        ) {
            Ok(input) => {
                self.orch.input_sha = sha256_hex(&input.to_bytes());
                self.orch.input = Some(input);
                None
            }
            Err(e) => Some(self.fail(format!("orchestrator input invalid: {}", short(&e.message)))),
        }
    }

    fn attempt_failed(&mut self, msg: &str, now: DateTime<Utc>) -> Flow {
        if self.orch.attempt < MAX_ATTEMPTS {
            self.event(
                "plan",
                None,
                format!("The orchestrator failed ({msg}); trying once more"),
            );
            self.orch.phase = Phase::Pending;
            return self.start_session(now);
        }
        let text = format!("orchestrator output rejected: {msg}");
        self.event("error", None, text.clone());
        self.fail(text)
    }

    fn poll_session(
        &mut self,
        run: &str,
        dir: &std::path::Path,
        since: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Flow {
        if now - since >= Duration::minutes(SESSION_TIMEOUT_MINUTES) {
            if let Err(e) = self.p.planner.kill(run) {
                let text = format!("Could not stop the orchestrator: {}", short(&e.message));
                self.event("error", None, text);
            }
            return self.attempt_failed("the orchestrator timed out", now);
        }
        let code = match self.p.planner.poll(run) {
            Err(e) => {
                let text = format!(
                    "Could not read the orchestrator state: {}",
                    short(&e.message)
                );
                self.event("error", None, text);
                return Flow::Wait;
            }
            Ok(Poll::Running) => return Flow::Wait,
            Ok(Poll::Exited(code)) => code,
        };
        if code != Some(0) {
            return self.attempt_failed("the orchestrator exited with an error", now);
        }
        let raw = match self.p.planner.read(dir) {
            Ok(raw) => raw,
            Err(e) => return self.attempt_failed(&short(&e.message), now),
        };
        let Some(input) = self.orch.input.clone() else {
            return self.attempt_failed("the orchestrator input is missing", now);
        };
        match orchestrate::validate(&input, &raw, self.forced_mode()) {
            Ok(plan) => {
                self.p.planner.discard(dir);
                self.accept(plan, now);
                Flow::Go
            }
            Err(r) => self.attempt_failed(&short(r.message()), now),
        }
    }

    fn accept(&mut self, plan: Plan, now: DateTime<Utc>) {
        self.orch.rows = plan
            .triage
            .iter()
            .map(|t| TriageRow {
                task: t.key.clone(),
                task_id: t.task_id,
                verdict: verdict_key(t.verdict).into(),
                reason: t.reason.clone(),
                question: t.question.clone(),
                applied: false,
                error: None,
            })
            .collect();
        let text = format!(
            "Plan accepted: {} to start, {} need a human",
            plan.items.len(),
            plan.triage.len()
        );
        self.orch.plan = Some(plan);
        self.orch.made_at = Some(now);
        self.orch.phase = Phase::Done;
        self.event("plan", None, text);
        self.write_status();
    }

    pub(super) fn interrupt_phase(&mut self) {
        if matches!(self.orch.phase, Phase::Running { .. }) {
            self.kill_phase();
        }
    }

    fn resolve_status(
        &self,
        statuses: &Result<Vec<(u64, String, String)>, String>,
        name: &str,
    ) -> Result<u64, String> {
        let list = statuses.as_ref().map_err(|e| e.clone())?;
        let wanted = name.trim();
        let mut found = list
            .iter()
            .filter(|(_, n, _)| n.trim().eq_ignore_ascii_case(wanted));
        let first = found
            .next()
            .ok_or_else(|| format!("no status is named \"{wanted}\""))?;
        if found.next().is_some() {
            return Err(format!("more than one status is named \"{wanted}\""));
        }
        if KINDS_REFUSED.contains(&first.2.as_str()) {
            return Err(format!(
                "status \"{wanted}\" is a {} status, which triage does not use",
                first.2
            ));
        }
        Ok(first.0)
    }

    pub(super) fn apply_triage(&mut self) {
        if self.orch.applied || self.s.dry_run {
            return;
        }
        let Some(plan) = self.orch.plan.clone() else {
            return;
        };
        let pending: Vec<&TriageItem> = plan
            .triage
            .iter()
            .filter(|t| {
                self.orch
                    .rows
                    .iter()
                    .any(|r| r.task_id == t.task_id && !r.applied && r.error.is_none())
            })
            .collect();
        if !pending.is_empty() {
            let statuses = self.load_statuses();
            for item in pending {
                let name = match item.verdict {
                    Verdict::NeedsFeedback => self.s.answers.feedback_status.clone(),
                    Verdict::Blocked => self.s.answers.blocked_status.clone(),
                };
                let outcome = self
                    .resolve_status(&statuses, &name)
                    .and_then(|sid| self.apply_one(item, sid));
                if let Some(row) = self
                    .orch
                    .rows
                    .iter_mut()
                    .find(|r| r.task_id == item.task_id)
                {
                    row.applied = outcome.is_ok();
                    row.error = outcome.err();
                }
                self.log_triage(item);
                self.orch.applied = false;
                self.write_status();
            }
        }
        self.orch.applied = true;
        let n = self.orch.rows.iter().filter(|r| r.applied).count();
        if n > 0 {
            let text = format!(
                "{n} task{} need your feedback",
                if n == 1 { "" } else { "s" }
            );
            self.event("triage", None, text.clone());
            self.push("Autopilot: feedback needed", &text);
        }
        self.write_status();
    }

    fn log_triage(&mut self, item: &TriageItem) {
        let Some(row) = self.orch.rows.iter().find(|r| r.task_id == item.task_id) else {
            return;
        };
        let text = match &row.error {
            None => format!("{} moved to {}", item.key, item.verdict.label()),
            Some(e) => format!(
                "{} was not moved to {}: {e}",
                item.key,
                item.verdict.label()
            ),
        };
        let kind = if row.error.is_some() {
            "error"
        } else {
            "triage"
        };
        self.event(kind, Some(&item.key.clone()), text);
    }

    fn load_statuses(&mut self) -> Result<Vec<(u64, String, String)>, String> {
        let ws = match self.remote.ws {
            Some(ws) => ws,
            None => {
                let code = self
                    .p
                    .queue
                    .workspace_code(&self.s.rule)
                    .ok_or_else(|| "the rule has no workspace".to_string())?;
                let ws =
                    self.p.remote.workspace_id(&code).map_err(|e| {
                        format!("could not read the workspace: {}", short(&e.message))
                    })?;
                self.remote.ws = Some(ws);
                ws
            }
        };
        self.p
            .remote
            .statuses(ws)
            .map_err(|e| format!("could not read the statuses: {}", short(&e.message)))
    }

    fn apply_one(&self, item: &TriageItem, status_id: u64) -> Result<(), String> {
        let seen = self
            .orch
            .seen
            .iter()
            .find(|s| s.task_id == item.task_id)
            .ok_or_else(|| "the task is not part of the planning input".to_string())?;
        let fresh: PlanFresh = self
            .p
            .planner
            .fresh(&self.s.rule, item.task_id)
            .map_err(|e| format!("could not re-check the task: {}", short(&e.message)))?;
        if fresh.content_sha256 != seen.sha256 || !fresh.free || fresh.status_id != seen.status_id {
            return Err("changed since planning".into());
        }
        self.p
            .remote
            .comment(item.task_id, &orchestrate::triage_comment(item))
            .map_err(|e| format!("could not comment: {}", short(&e.message)))?;
        self.p
            .remote
            .set_status(item.task_id, status_id)
            .map_err(|e| format!("could not change the status: {}", short(&e.message)))
    }

    pub(super) fn dispatch(&self) -> (BTreeSet<i64>, Arc<TaskPlan>) {
        let excluded = self.orch.excluded();
        let only = self
            .s
            .tasks
            .iter()
            .copied()
            .filter(|id| !excluded.contains(id))
            .collect();
        let items = self
            .orch
            .plan
            .as_ref()
            .map(|p| p.items.as_slice())
            .unwrap_or_default();
        let mut order = Vec::new();
        for id in self.s.order.iter().chain(items.iter().map(|i| &i.task_id)) {
            if !order.contains(id) {
                order.push(*id);
            }
        }
        let size = self.team_size() as u8;
        let plan = TaskPlan {
            order,
            groups: items
                .iter()
                .filter_map(|i| i.group.clone().map(|g| (i.task_id, g)))
                .collect(),
            teams: items
                .iter()
                .map(|i| (i.task_id, BundleTeam { size, mode: i.mode }))
                .collect(),
            profile: self.s.answers.profile.clone(),
        };
        (only, Arc::new(plan))
    }

    pub(super) fn start_policy(&self, starts_allowed: bool) -> StartPolicy {
        let (only, plan) = self.dispatch();
        StartPolicy {
            starts_allowed,
            max_live: Some(self.s.answers.max_teams as usize),
            max_new_starts: self
                .s
                .answers
                .max_tasks
                .map(|m| (m as usize).saturating_sub(self.started_ids.len())),
            only: Some(only),
            plan: Some(plan),
        }
    }

    pub(super) fn brief(&self, id: i64, fallback: Option<(&str, &str)>) -> TaskBrief {
        let seen = self.orch.seen.iter().find(|s| s.task_id == id);
        let (key, title) = match (seen, fallback) {
            (Some(s), _) => (s.key.clone(), s.title.clone()),
            (None, Some((k, t))) => (k.into(), t.into()),
            (None, None) => (format!("#{id}"), String::new()),
        };
        TaskBrief {
            key,
            id,
            title,
            workspace_code: self.ws_code.clone(),
        }
    }

    pub(super) fn plan_doc(&self) -> PlanDoc {
        let (state, error) = match &self.orch.phase {
            Phase::Off | Phase::Pending => (PlanState::Pending, None),
            Phase::Running { .. } => (PlanState::Running, None),
            Phase::Done => (PlanState::Done, None),
            Phase::Failed(t) if t == NOTHING_TO_PLAN => (PlanState::Skipped, Some(t.clone())),
            Phase::Failed(t) => (PlanState::Rejected, Some(t.clone())),
        };
        let items = self
            .orch
            .plan
            .iter()
            .flat_map(|p| p.items.iter())
            .enumerate()
            .map(|(i, it)| PlanRow {
                task: it.key.clone(),
                task_id: it.task_id,
                position: i as u32 + 1,
                group: it.group.clone(),
                mode: it.mode.as_str().into(),
                reason: it.reason.clone(),
                state: self
                    .view
                    .plan_states
                    .get(&it.task_id)
                    .copied()
                    .unwrap_or("queued")
                    .into(),
            })
            .collect();
        PlanDoc {
            state,
            dry_run: self.s.dry_run,
            profile: self
                .s
                .answers
                .profile
                .clone()
                .unwrap_or_else(|| self.s.agent.clone()),
            made_at: self.orch.made_at,
            attempt: self.orch.attempt,
            error,
            items,
        }
    }

    pub(super) fn agents_planned(&self) -> Slots {
        let running = self
            .view
            .live
            .iter()
            .map(|l| {
                self.team_of(l.task_id)
                    .map_or(self.team_size(), |t| u32::from(t.size))
            })
            .sum();
        Slots {
            running,
            max: self.s.answers.max_teams * self.team_size(),
        }
    }
}
