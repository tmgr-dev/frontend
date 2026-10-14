use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::model::Tool;

#[derive(Debug, Clone, PartialEq)]
pub struct RunWindow {
    pub run_ref: String,
    pub task_ref: String,
    pub workspace: String,
    pub session_id: Option<String>,
    pub branch: Option<String>,
    pub project_hash: Option<String>,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkEvent {
    pub id: i64,
    pub tool: Tool,
    pub account: String,
    pub tool_session_id: String,
    pub parent_session_id: Option<String>,
    pub ts: DateTime<Utc>,
    pub model: String,
    pub git_branch: Option<String>,
    pub project_hash: Option<String>,
    pub cost_usd: Option<f64>,
    pub tokens: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LinkMethod {
    SessionId,
    BranchWindow,
    ProjectWindow,
}

impl LinkMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            LinkMethod::SessionId => "session_id",
            LinkMethod::BranchWindow => "branch_window",
            LinkMethod::ProjectWindow => "project_window",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub event_index: usize,
    pub run_ref: String,
    pub task_ref: String,
    pub method: LinkMethod,
    pub confidence: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Ambiguous {
    pub event_index: usize,
    pub candidates: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Totals {
    pub key: String,
    pub cost_usd: f64,
    pub tokens: u64,
    pub events: u64,
    pub unpriced_events: u64,
}

impl Totals {
    fn add(&mut self, ev: &LinkEvent) {
        self.events += 1;
        self.tokens += ev.tokens;
        match ev.cost_usd {
            Some(c) => self.cost_usd += c,
            None => self.unpriced_events += 1,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct LinkReport {
    pub links: Vec<Link>,
    pub ambiguous: Vec<Ambiguous>,
    pub unattributed: Vec<usize>,
    pub per_run: Vec<Totals>,
    pub per_task: Vec<Totals>,
    pub ambiguous_totals: Totals,
    pub unattributed_totals: Totals,
}

const CONFIDENCE_SESSION: f64 = 1.0;
const CONFIDENCE_PARENT: f64 = 0.9;
const CONFIDENCE_BRANCH: f64 = 0.8;
const CONFIDENCE_BRANCH_UNVERIFIED: f64 = 0.7;
const CONFIDENCE_PROJECT: f64 = 0.5;

pub fn attribute(events: &[LinkEvent], runs: &[RunWindow]) -> LinkReport {
    attribute_at(events, runs, Utc::now())
}

fn contains(run: &RunWindow, ts: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    if ts < run.started_at {
        return false;
    }
    match run.ended_at {
        Some(end) => ts < end,
        None => ts <= now,
    }
}

type Index<'a> = HashMap<&'a str, Vec<usize>>;

fn index_by<'a>(
    runs: &'a [RunWindow],
    key: impl Fn(&'a RunWindow) -> Option<&'a str>,
) -> Index<'a> {
    let mut map: Index<'a> = HashMap::new();
    for (i, r) in runs.iter().enumerate() {
        if let Some(k) = key(r).filter(|k| !k.is_empty()) {
            map.entry(k).or_default().push(i);
        }
    }
    map
}

fn candidates(
    index: &Index<'_>,
    key: Option<&str>,
    runs: &[RunWindow],
    ts: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Vec<usize> {
    let Some(list) = key.and_then(|k| index.get(k)) else {
        return Vec::new();
    };
    let mut seen = BTreeSet::new();
    list.iter()
        .copied()
        .filter(|&i| contains(&runs[i], ts, now) && seen.insert(runs[i].run_ref.as_str()))
        .collect()
}

pub fn attribute_at(events: &[LinkEvent], runs: &[RunWindow], now: DateTime<Utc>) -> LinkReport {
    let by_session = index_by(runs, |r| r.session_id.as_deref());
    let by_branch = index_by(runs, |r| r.branch.as_deref());
    let by_project = index_by(runs, |r| r.project_hash.as_deref());
    let mut events_by_branch: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, e) in events.iter().enumerate() {
        if let Some(b) = e.git_branch.as_deref() {
            events_by_branch.entry(b).or_default().push(i);
        }
    }
    let mut contended: HashMap<usize, bool> = HashMap::new();
    let mut report = LinkReport::default();
    let mut per_run: BTreeMap<&str, Totals> = BTreeMap::new();
    let mut per_task: BTreeMap<&str, Totals> = BTreeMap::new();
    for (idx, ev) in events.iter().enumerate() {
        let on_branch = candidates(&by_branch, ev.git_branch.as_deref(), runs, ev.ts, now);
        let same_project = |i: &usize| match (&ev.project_hash, &runs[*i].project_hash) {
            (Some(a), Some(b)) => Some(a == b),
            _ => None,
        };
        let verified: Vec<usize> = on_branch
            .iter()
            .copied()
            .filter(|i| same_project(i) == Some(true))
            .collect();
        let unverified: Vec<usize> = on_branch
            .iter()
            .copied()
            .filter(|i| same_project(i).is_none())
            .collect();
        let rungs = [
            (
                LinkMethod::SessionId,
                CONFIDENCE_SESSION,
                candidates(&by_session, Some(&ev.tool_session_id), runs, ev.ts, now),
            ),
            (
                LinkMethod::SessionId,
                CONFIDENCE_PARENT,
                candidates(
                    &by_session,
                    ev.parent_session_id.as_deref(),
                    runs,
                    ev.ts,
                    now,
                ),
            ),
            (LinkMethod::BranchWindow, CONFIDENCE_BRANCH, verified),
            (
                LinkMethod::BranchWindow,
                CONFIDENCE_BRANCH_UNVERIFIED,
                unverified,
            ),
            (
                LinkMethod::ProjectWindow,
                CONFIDENCE_PROJECT,
                candidates(&by_project, ev.project_hash.as_deref(), runs, ev.ts, now),
            ),
        ];
        let Some((method, confidence, found)) = rungs.into_iter().find(|(_, _, c)| !c.is_empty())
        else {
            report.unattributed_totals.add(ev);
            report.unattributed.push(idx);
            continue;
        };
        let shared_branch = |run_idx: usize, cache: &mut HashMap<usize, bool>| {
            *cache.entry(run_idx).or_insert_with(|| {
                let run = &runs[run_idx];
                let hashes: BTreeSet<&str> = run
                    .branch
                    .as_deref()
                    .and_then(|b| events_by_branch.get(b))
                    .into_iter()
                    .flatten()
                    .map(|&i| &events[i])
                    .filter(|e| contains(run, e.ts, now))
                    .filter_map(|e| e.project_hash.as_deref())
                    .collect();
                hashes.len() > 1
            })
        };
        let unverified_rung = confidence == CONFIDENCE_BRANCH_UNVERIFIED;
        if let [one] = found[..] {
            if unverified_rung && shared_branch(one, &mut contended) {
                report.ambiguous_totals.add(ev);
                report.ambiguous.push(Ambiguous {
                    event_index: idx,
                    candidates: vec![runs[one].run_ref.clone()],
                });
                continue;
            }
            let run = &runs[one];
            per_run
                .entry(run.run_ref.as_str())
                .or_insert_with(|| Totals {
                    key: run.run_ref.clone(),
                    ..Totals::default()
                })
                .add(ev);
            per_task
                .entry(run.task_ref.as_str())
                .or_insert_with(|| Totals {
                    key: run.task_ref.clone(),
                    ..Totals::default()
                })
                .add(ev);
            report.links.push(Link {
                event_index: idx,
                run_ref: run.run_ref.clone(),
                task_ref: run.task_ref.clone(),
                method,
                confidence,
            });
        } else {
            let mut refs: Vec<String> = found.iter().map(|&i| runs[i].run_ref.clone()).collect();
            refs.sort();
            report.ambiguous_totals.add(ev);
            report.ambiguous.push(Ambiguous {
                event_index: idx,
                candidates: refs,
            });
        }
    }
    report.per_run = per_run.into_values().collect();
    report.per_task = per_task.into_values().collect();
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};

    fn t(min: i64) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 14, 9, 0, 0).unwrap() + Duration::minutes(min)
    }

    fn now() -> DateTime<Utc> {
        t(600)
    }

    fn run(r: &str, task: &str, from: i64, to: Option<i64>) -> RunWindow {
        RunWindow {
            run_ref: r.into(),
            task_ref: task.into(),
            workspace: "w".into(),
            session_id: None,
            branch: None,
            project_hash: None,
            started_at: t(from),
            ended_at: to.map(t),
        }
    }

    fn ev(id: i64, session: &str, at: i64, cost: Option<f64>) -> LinkEvent {
        LinkEvent {
            id,
            tool: Tool::Claude,
            account: "claude".into(),
            tool_session_id: session.into(),
            parent_session_id: None,
            ts: t(at),
            model: "m".into(),
            git_branch: None,
            project_hash: None,
            cost_usd: cost,
            tokens: 10,
        }
    }

    #[test]
    fn session_id_beats_branch_and_project() {
        let mut a = run("a", "TM-1", 0, Some(60));
        a.session_id = Some("s1".into());
        let mut b = run("b", "TM-2", 0, Some(60));
        b.branch = Some("feat".into());
        b.project_hash = Some("p".into());
        let mut e = ev(1, "s1", 5, Some(1.0));
        e.git_branch = Some("feat".into());
        e.project_hash = Some("p".into());
        let r = attribute_at(&[e], &[a, b], now());
        assert_eq!(r.links.len(), 1);
        assert_eq!(r.links[0].run_ref, "a");
        assert_eq!(r.links[0].method, LinkMethod::SessionId);
        assert!((r.links[0].confidence - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn branch_and_project_rungs_use_the_window() {
        let mut a = run("a", "TM-1", 0, Some(60));
        a.branch = Some("feat".into());
        let mut b = run("b", "TM-2", 100, Some(160));
        b.project_hash = Some("p".into());
        let mut e1 = ev(1, "x", 5, Some(1.0));
        e1.git_branch = Some("feat".into());
        let mut e2 = ev(2, "x", 110, Some(2.0));
        e2.project_hash = Some("p".into());
        let mut e3 = ev(3, "x", 70, Some(4.0));
        e3.git_branch = Some("feat".into());
        let r = attribute_at(&[e1, e2, e3], &[a, b], now());
        assert_eq!(r.links[0].method, LinkMethod::BranchWindow);
        assert_eq!(r.links[1].method, LinkMethod::ProjectWindow);
        assert_eq!(r.unattributed, vec![2]);
        assert!(r.links[0].confidence > r.links[1].confidence);
    }

    #[test]
    fn a_branch_name_alone_does_not_cross_repositories() {
        let mut x = run("x", "TM-1", 0, Some(60));
        x.branch = Some("main".into());
        x.project_hash = Some("repo-x".into());
        let mut in_y = ev(1, "s", 5, Some(1.0));
        in_y.git_branch = Some("main".into());
        in_y.project_hash = Some("repo-y".into());
        let mut in_x = ev(2, "s", 6, Some(2.0));
        in_x.git_branch = Some("main".into());
        in_x.project_hash = Some("repo-x".into());
        let mut unknown = ev(3, "s", 7, Some(4.0));
        unknown.git_branch = Some("main".into());
        let r = attribute_at(&[in_y, in_x, unknown], &[x], now());
        assert_eq!(r.unattributed, vec![0]);
        assert_eq!(r.links.len(), 1);
        assert_eq!(r.links[0].event_index, 1);
        assert_eq!(
            r.ambiguous.len(),
            1,
            "an unknown repo sharing a branch name is not guessed"
        );
        assert_eq!(r.ambiguous[0].event_index, 2);
    }

    #[test]
    fn an_unverified_branch_match_links_when_one_repository_uses_the_branch() {
        let mut x = run("x", "TM-1", 0, Some(60));
        x.branch = Some("main".into());
        x.project_hash = Some("repo-x".into());
        let mut in_x = ev(1, "s", 5, Some(1.0));
        in_x.git_branch = Some("main".into());
        in_x.project_hash = Some("repo-x".into());
        let mut unknown = ev(2, "s", 7, Some(4.0));
        unknown.git_branch = Some("main".into());
        let r = attribute_at(&[in_x, unknown], &[x], now());
        assert_eq!(r.links.len(), 2);
        assert!(r.ambiguous.is_empty());
        assert!(r.links[0].confidence > r.links[1].confidence);
    }

    #[test]
    fn two_repositories_on_main_without_a_run_project_are_ambiguous() {
        let mut x = run("x", "TM-1", 0, Some(60));
        x.branch = Some("main".into());
        let mut a = ev(1, "s1", 5, Some(1.0));
        a.git_branch = Some("main".into());
        a.project_hash = Some("repo-a".into());
        let mut b = ev(2, "s2", 6, Some(2.0));
        b.git_branch = Some("main".into());
        b.project_hash = Some("repo-b".into());
        let r = attribute_at(&[a, b], &[x], now());
        assert!(r.links.is_empty());
        assert_eq!(r.ambiguous.len(), 2);
    }

    #[test]
    fn a_child_thread_follows_its_parent_session() {
        let mut a = run("a", "TM-1", 0, Some(60));
        a.session_id = Some("parent".into());
        let mut child = ev(1, "child", 5, Some(1.0));
        child.parent_session_id = Some("parent".into());
        let orphan = ev(2, "child", 6, Some(1.0));
        let mut own = ev(3, "parent", 7, Some(1.0));
        own.parent_session_id = Some("elsewhere".into());
        let r = attribute_at(&[child, orphan, own], &[a], now());
        assert_eq!(r.unattributed, vec![1]);
        assert_eq!(r.links.len(), 2);
        assert!(r.links[0].confidence < r.links[1].confidence);
        assert_eq!(r.links[0].method, LinkMethod::SessionId);
    }

    #[test]
    fn parallel_runs_on_one_branch_are_ambiguous_and_counted_once() {
        let mut a = run("a", "TM-1", 0, Some(60));
        a.branch = Some("feat".into());
        let mut b = run("b", "TM-2", 10, Some(60));
        b.branch = Some("feat".into());
        let mut early = ev(1, "x", 5, Some(1.0));
        early.git_branch = Some("feat".into());
        let mut both = ev(2, "x", 20, Some(2.0));
        both.git_branch = Some("feat".into());
        let r = attribute_at(&[early, both], &[a, b], now());
        assert_eq!(r.links.len(), 1);
        assert_eq!(r.ambiguous.len(), 1);
        assert_eq!(r.ambiguous[0].candidates, vec!["a", "b"]);
        assert!((r.ambiguous_totals.cost_usd - 2.0).abs() < 1e-9);
        assert!((r.per_run[0].cost_usd - 1.0).abs() < 1e-9);
    }

    #[test]
    fn the_same_run_listed_twice_is_not_ambiguous() {
        let mut a = run("a", "TM-1", 0, Some(60));
        a.branch = Some("feat".into());
        let a2 = a.clone();
        let mut e = ev(1, "x", 5, Some(1.0));
        e.git_branch = Some("feat".into());
        let r = attribute_at(&[e], &[a, a2], now());
        assert_eq!(r.links.len(), 1);
        assert!(r.ambiguous.is_empty());
    }

    #[test]
    fn window_edges_are_half_open_and_open_runs_end_now() {
        let mut a = run("a", "TM-1", 0, Some(60));
        a.session_id = Some("s".into());
        let mut b = run("b", "TM-2", 60, None);
        b.session_id = Some("s".into());
        let events = [
            ev(1, "s", -1, None),
            ev(2, "s", 0, None),
            ev(3, "s", 59, None),
            ev(4, "s", 60, None),
            ev(5, "s", 600, None),
            ev(6, "s", 601, None),
        ];
        let r = attribute_at(&events, &[a, b], now());
        let who: Vec<(usize, &str)> = r
            .links
            .iter()
            .map(|l| (l.event_index, l.run_ref.as_str()))
            .collect();
        assert_eq!(who, vec![(1, "a"), (2, "a"), (3, "b"), (4, "b")], "{who:?}");
        assert_eq!(r.unattributed, vec![0, 5]);
        assert_eq!(r.unattributed_totals.unpriced_events, 2);
    }

    #[test]
    fn one_session_across_two_sequential_tasks_is_split_by_timestamp() {
        let mut a = run("a", "TM-1", 0, Some(30));
        a.session_id = Some("s".into());
        let mut b = run("b", "TM-2", 30, Some(90));
        b.session_id = Some("s".into());
        let events: Vec<_> = (0..90).map(|m| ev(m, "s", m, Some(1.0))).collect();
        let r = attribute_at(&events, &[a, b], now());
        let tasks: Vec<_> = r
            .per_task
            .iter()
            .map(|t| (t.key.as_str(), t.events))
            .collect();
        assert_eq!(tasks, vec![("TM-1", 30), ("TM-2", 60)]);
    }

    #[test]
    fn a_session_outside_every_window_falls_back_to_lower_rungs() {
        let mut a = run("a", "TM-1", 0, Some(10));
        a.session_id = Some("s".into());
        let mut b = run("b", "TM-2", 50, Some(90));
        b.branch = Some("feat".into());
        let mut e = ev(1, "s", 60, Some(1.0));
        e.git_branch = Some("feat".into());
        let r = attribute_at(&[e], &[a, b], now());
        assert_eq!(r.links[0].run_ref, "b");
        assert_eq!(r.links[0].method, LinkMethod::BranchWindow);
    }

    #[test]
    fn unpriced_events_are_counted_apart_from_zero_cost() {
        let mut a = run("a", "TM-1", 0, Some(60));
        a.session_id = Some("s".into());
        let r = attribute_at(
            &[ev(1, "s", 1, Some(0.0)), ev(2, "s", 2, None)],
            &[a],
            now(),
        );
        assert_eq!(r.per_run[0].unpriced_events, 1);
        assert_eq!(r.per_run[0].events, 2);
    }

    struct Lcg(u64);

    impl Lcg {
        fn next(&mut self, n: u64) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (self.0 >> 33) % n
        }
    }

    #[test]
    fn every_event_lands_in_exactly_one_bucket_and_cost_is_conserved() {
        for seed in 0..40u64 {
            let mut rng = Lcg(seed + 1);
            let sessions = ["s0", "s1", "s2"];
            let branches = ["b0", "b1"];
            let projects = ["p0", "p1"];
            let runs: Vec<RunWindow> = (0..rng.next(6) + 1)
                .map(|i| {
                    let from = rng.next(100) as i64;
                    let to = (rng.next(3) != 0).then(|| from + rng.next(60) as i64 + 1);
                    let mut r = run(&format!("r{i}"), &format!("TM-{}", rng.next(3)), from, to);
                    r.session_id =
                        (rng.next(2) == 0).then(|| sessions[rng.next(3) as usize].into());
                    r.branch = (rng.next(2) == 0).then(|| branches[rng.next(2) as usize].into());
                    r.project_hash =
                        (rng.next(2) == 0).then(|| projects[rng.next(2) as usize].into());
                    r
                })
                .collect();
            let events: Vec<LinkEvent> = (0..200)
                .map(|i| {
                    let mut e = ev(
                        i,
                        sessions[rng.next(3) as usize],
                        rng.next(200) as i64 - 20,
                        (rng.next(5) != 0).then(|| rng.next(100) as f64 / 10.0),
                    );
                    e.git_branch =
                        (rng.next(2) == 0).then(|| branches[rng.next(2) as usize].into());
                    e.project_hash =
                        (rng.next(2) == 0).then(|| projects[rng.next(2) as usize].into());
                    e
                })
                .collect();
            let r = attribute_at(&events, &runs, now());
            let mut seen = vec![0u8; events.len()];
            for l in &r.links {
                seen[l.event_index] += 1;
            }
            for a in &r.ambiguous {
                seen[a.event_index] += 1;
                assert!(!a.candidates.is_empty());
            }
            for &u in &r.unattributed {
                seen[u] += 1;
            }
            assert!(seen.iter().all(|&n| n == 1), "seed {seed}");
            let input: f64 = events.iter().filter_map(|e| e.cost_usd).sum();
            let output: f64 = r.per_run.iter().map(|t| t.cost_usd).sum::<f64>()
                + r.ambiguous_totals.cost_usd
                + r.unattributed_totals.cost_usd;
            assert!((input - output).abs() < 1e-6, "seed {seed}");
            let by_task: f64 = r.per_task.iter().map(|t| t.cost_usd).sum();
            let by_run: f64 = r.per_run.iter().map(|t| t.cost_usd).sum();
            assert!((by_task - by_run).abs() < 1e-6, "seed {seed}");
        }
    }
}
