use std::collections::BTreeMap;
use std::io::{BufRead, Write};

use tmgr_core::error::{Error, Kind, Result};
use tmgr_queue::rules::GLOBAL_MAX;
use tmgr_queue::runs::mode_name;
use tmgr_usage::Tool;

use super::model::Answers;
use super::orchestrate::ModeSetting;
use super::start::agent_tool;

pub type Flags = BTreeMap<&'static str, String>;

const TRIES: usize = 3;
const MAX_TEAM_SIZE: u32 = 5;

pub struct Draft {
    pub answers: Answers,
    pub picked: Option<Vec<String>>,
    pub known_keys: Vec<String>,
    pub rule_max: u32,
    pub rule_profile: String,
    pub rule_mode: Option<String>,
    pub only: Option<Vec<String>>,
    pub skip: Vec<String>,
    pub order: Vec<String>,
}

impl Draft {
    pub fn new(remembered: Option<Answers>, known_keys: Vec<String>, rule_max: u32) -> Draft {
        let mut answers = remembered.unwrap_or_default();
        answers.all_tasks = true;
        answers.max_teams = answers.max_concurrent;
        Draft {
            answers,
            picked: None,
            known_keys,
            rule_max,
            rule_profile: "claude-code".into(),
            rule_mode: None,
            only: None,
            skip: Vec::new(),
            order: Vec::new(),
        }
    }

    pub fn with_rule(mut self, profile: &str, mode: Option<&str>) -> Draft {
        self.rule_profile = profile.to_string();
        self.rule_mode = mode.map(str::to_string);
        self
    }

    pub fn profile(&self) -> &str {
        self.answers
            .profile
            .as_deref()
            .unwrap_or(&self.rule_profile)
    }

    pub fn is_codex(&self) -> bool {
        agent_tool(self.profile()) == Some(Tool::Codex)
    }

    pub fn is_chosen(&self, key: &str) -> bool {
        let has = |list: &[String]| list.iter().any(|k| k == key);
        self.picked.as_deref().is_none_or(has)
            && self.only.as_deref().is_none_or(has)
            && !has(&self.skip)
    }

    pub fn restrict(
        &mut self,
        only: Option<&str>,
        skip: Option<&str>,
        order: Option<&str>,
    ) -> Result<()> {
        let keys = |flag: &str, s: Option<&str>| -> Result<Option<Vec<String>>> {
            s.map(|s| parse_keys(&self.known_keys, s))
                .transpose()
                .map_err(|m| Error::usage(format!("--{flag}: {m}")))
        };
        let (only, skip, order) = (
            keys("only", only)?,
            keys("skip", skip)?,
            keys("order", order)?,
        );
        self.only = only;
        self.skip = skip.unwrap_or_default();
        if let Some(order) = order {
            if let Some(k) = order.iter().find(|k| !self.is_chosen(k)) {
                return Err(Error::usage(format!(
                    "--order: {k} is not among the chosen tasks"
                )));
            }
            self.order = order;
        }
        Ok(())
    }

    pub fn concurrent_cap(&self) -> u32 {
        (GLOBAL_MAX as u32).min(self.rule_max.max(1))
    }
}

pub struct Question {
    pub id: &'static str,
    pub label: &'static str,
    pub current: fn(&Draft) -> String,
    pub apply: fn(&mut Draft, &str) -> std::result::Result<(), String>,
    pub enabled: fn(&Draft) -> bool,
}

fn always(_: &Draft) -> bool {
    true
}

fn guarded(d: &Draft) -> bool {
    d.answers.usage_guard
}

fn shown<T: ToString>(v: Option<T>) -> String {
    v.map_or("none".into(), |v| v.to_string())
}

fn yes_no(v: bool) -> String {
    if v { "yes" } else { "no" }.into()
}

pub fn parse_range(s: &str, lo: u32, hi: u32) -> std::result::Result<u32, String> {
    let n: u32 = s
        .trim()
        .trim_end_matches('%')
        .parse()
        .map_err(|_| "enter a whole number".to_string())?;
    if (lo..=hi).contains(&n) {
        Ok(n)
    } else {
        Err(format!("enter a number from {lo} to {hi}"))
    }
}

pub fn parse_bool(s: &str) -> std::result::Result<bool, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "y" | "yes" | "true" | "on" => Ok(true),
        "n" | "no" | "false" | "off" => Ok(false),
        _ => Err("answer yes or no".into()),
    }
}

fn is_none(s: &str) -> bool {
    matches!(s.trim().to_ascii_lowercase().as_str(), "none" | "no limit")
}

pub fn parse_limit(s: &str) -> std::result::Result<Option<f64>, String> {
    if is_none(s) {
        return Ok(None);
    }
    let v: f64 = s
        .trim()
        .trim_start_matches('$')
        .parse()
        .map_err(|_| "enter a number, or none".to_string())?;
    if v.is_finite() && v > 0.0 {
        Ok(Some(v))
    } else {
        Err("enter a number above 0, or none".into())
    }
}

pub fn parse_count(s: &str) -> std::result::Result<Option<u32>, String> {
    if is_none(s) {
        return Ok(None);
    }
    parse_range(s, 1, 100_000).map(Some)
}

fn parse_keys(known: &[String], s: &str) -> std::result::Result<Vec<String>, String> {
    let mut keys = Vec::new();
    for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let known = known
            .iter()
            .find(|k| k.eq_ignore_ascii_case(part))
            .ok_or_else(|| format!("{part} is not in the list above"))?;
        if !keys.contains(known) {
            keys.push(known.clone());
        }
    }
    if keys.is_empty() {
        return Err("enter task keys separated by commas".into());
    }
    Ok(keys)
}

fn tasks_apply(d: &mut Draft, s: &str) -> std::result::Result<(), String> {
    if s.trim().eq_ignore_ascii_case("all") {
        d.picked = None;
        return Ok(());
    }
    d.picked = Some(parse_keys(&d.known_keys, s)?);
    Ok(())
}

pub fn mode_word(m: ModeSetting) -> &'static str {
    match m {
        ModeSetting::Eco => "eco",
        ModeSetting::Heavy => "heavy",
        ModeSetting::Orchestrator => "orchestrator",
    }
}

pub fn check_profile(name: &str, mode: Option<&str>) -> std::result::Result<Tool, String> {
    let tool = agent_tool(name)
        .ok_or_else(|| format!("{name} is not a profile the runner knows for claude or codex"))?;
    let dir = tmgr_runner::profile::user_dir();
    let (set, _) = tmgr_runner::profile::ProfileSet::load(dir.as_deref());
    let profile = set.lookup(name)?;
    if profile.has_permission_modes() {
        let wanted = mode_name(mode);
        profile.permission_args(wanted).map_err(|_| {
            format!("profile {name} cannot run in mode {wanted}, which the rule asks for")
        })?;
    }
    Ok(tool)
}

fn soft_apply(d: &mut Draft, s: &str) -> std::result::Result<(), String> {
    d.answers.usage_soft = parse_range(s, 1, 99)?;
    Ok(())
}

fn hard_apply(d: &mut Draft, s: &str) -> std::result::Result<(), String> {
    let hard = parse_range(s, 2, 100)?;
    if hard <= d.answers.usage_soft {
        return Err(format!(
            "the pause level must be above {}, the level that stops new tasks",
            d.answers.usage_soft
        ));
    }
    d.answers.usage_hard = hard;
    Ok(())
}

pub fn questions() -> Vec<Question> {
    vec![
        Question {
            id: "tasks",
            label: "Which tasks? all, or keys separated by commas",
            current: |d| d.picked.as_ref().map_or("all".into(), |k| k.join(",")),
            apply: tasks_apply,
            enabled: always,
        },
        Question {
            id: "max-teams",
            label: "Teams at the same time",
            current: |d| d.answers.max_teams.min(d.concurrent_cap()).to_string(),
            apply: |d, s| {
                d.answers.max_teams = parse_range(s, 1, d.concurrent_cap())?;
                Ok(())
            },
            enabled: always,
        },
        Question {
            id: "profile",
            label: "Agent profile for the teams",
            current: |d| d.profile().to_string(),
            apply: |d, s| {
                let name = s.trim();
                check_profile(name, d.rule_mode.as_deref())?;
                d.answers.profile = Some(name.to_string());
                Ok(())
            },
            enabled: always,
        },
        Question {
            id: "team-size",
            label: "Agents per team (lead and workers)",
            current: |d| d.answers.team_size.to_string(),
            apply: |d, s| {
                d.answers.team_size = parse_range(s, 1, MAX_TEAM_SIZE)?;
                Ok(())
            },
            enabled: |d| !d.is_codex(),
        },
        Question {
            id: "team-mode",
            label: "Team mode (orchestrator = the orchestrator chooses per task)",
            current: |d| mode_word(d.answers.team_mode).into(),
            apply: |d, s| {
                d.answers.team_mode = match s.trim().to_ascii_lowercase().as_str() {
                    "eco" => ModeSetting::Eco,
                    "heavy" => ModeSetting::Heavy,
                    "orchestrator" => ModeSetting::Orchestrator,
                    _ => return Err("enter eco, heavy or orchestrator".into()),
                };
                Ok(())
            },
            enabled: always,
        },
        Question {
            id: "usage-soft",
            label: "Stop starting new tasks at this usage percent",
            current: |d| d.answers.usage_soft.to_string(),
            apply: soft_apply,
            enabled: guarded,
        },
        Question {
            id: "usage-hard",
            label: "Pause running agents at this usage percent",
            current: |d| d.answers.usage_hard.to_string(),
            apply: hard_apply,
            enabled: guarded,
        },
        Question {
            id: "resume-after-reset",
            label: "Continue after the usage window resets",
            current: |d| yes_no(d.answers.resume_after_reset),
            apply: |d, s| {
                d.answers.resume_after_reset = parse_bool(s)?;
                Ok(())
            },
            enabled: guarded,
        },
        Question {
            id: "max-cost",
            label: "Most to spend in USD on runs paid by API key",
            current: |d| shown(d.answers.max_cost_usd),
            apply: |d, s| {
                d.answers.max_cost_usd = parse_limit(s)?;
                Ok(())
            },
            enabled: always,
        },
        Question {
            id: "max-hours",
            label: "Most hours to run",
            current: |d| shown(d.answers.max_hours),
            apply: |d, s| {
                d.answers.max_hours = parse_limit(s)?;
                Ok(())
            },
            enabled: always,
        },
        Question {
            id: "max-tasks",
            label: "Most tasks to start",
            current: |d| shown(d.answers.max_tasks),
            apply: |d, s| {
                d.answers.max_tasks = parse_count(s)?;
                Ok(())
            },
            enabled: always,
        },
    ]
}

fn read_answer(input: &mut dyn BufRead) -> Result<String> {
    let mut line = String::new();
    let n = input
        .read_line(&mut line)
        .map_err(|_| Error::new(Kind::Interrupted, "could not read the answer"))?;
    if n == 0 {
        return Err(Error::new(
            Kind::Interrupted,
            "no answer, nothing was started",
        ));
    }
    Ok(line.trim().to_string())
}

pub fn ask_all(
    qs: &[Question],
    d: &mut Draft,
    flags: &Flags,
    assume: bool,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<()> {
    for q in qs {
        if !(q.enabled)(d) {
            if q.id == "team-size" {
                let _ = writeln!(out, "codex runs one agent per team");
            }
            continue;
        }
        if let Some(v) = flags.get(q.id) {
            (q.apply)(d, v).map_err(|m| Error::usage(format!("--{}: {m}", q.id)))?;
            continue;
        }
        if assume {
            continue;
        }
        let mut tries = 0;
        loop {
            let _ = write!(out, "{} [{}]: ", q.label, (q.current)(d));
            let _ = out.flush();
            let answer = read_answer(input)?;
            if answer.is_empty() {
                break;
            }
            match (q.apply)(d, &answer) {
                Ok(()) => break,
                Err(m) => {
                    let _ = writeln!(out, "  {m}");
                    tries += 1;
                    if tries >= TRIES {
                        return Err(Error::usage(format!("no valid answer to: {}", q.label)));
                    }
                }
            }
        }
    }
    settle(d)?;
    finish(d)
}

pub fn finish(d: &Draft) -> Result<()> {
    let a = &d.answers;
    if a.usage_guard && a.usage_hard <= a.usage_soft {
        return Err(Error::usage(format!(
            "the pause level ({}%) must be above the level that stops new tasks ({}%)",
            a.usage_hard, a.usage_soft
        ))
        .hint("pass --usage-soft and --usage-hard"));
    }
    Ok(())
}

pub fn settle(d: &mut Draft) -> Result<()> {
    d.answers.max_teams = d.answers.max_teams.min(d.concurrent_cap());
    d.answers.max_concurrent = d.answers.max_teams;
    if d.answers.profile.is_some() {
        check_profile(d.profile(), d.rule_mode.as_deref())
            .map_err(|m| Error::usage(format!("--profile: {m}")))?;
    }
    if d.is_codex() {
        d.answers.team_size = 1;
    }
    Ok(())
}

pub fn choose_rule(
    names: &[String],
    flag: Option<&str>,
    remembered: Option<&str>,
    assume: bool,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Result<String> {
    if let Some(f) = flag {
        return names
            .iter()
            .find(|n| n.eq_ignore_ascii_case(f))
            .cloned()
            .ok_or_else(|| {
                Error::usage(format!("--rule: no cloud rule named {f}"))
                    .hint(format!("rules: {}", names.join(", ")))
            });
    }
    if let [only] = names {
        return Ok(only.clone());
    }
    let default = remembered.and_then(|r| names.iter().find(|n| n.as_str() == r));
    if assume {
        return default.cloned().ok_or_else(|| {
            Error::usage("several cloud rules and none remembered")
                .hint(format!("pass --rule ({})", names.join(", ")))
        });
    }
    let _ = writeln!(out, "Cloud rules: {}", names.join(", "));
    for _ in 0..TRIES {
        let _ = write!(
            out,
            "Which rule? [{}]: ",
            default.map_or("", String::as_str)
        );
        let _ = out.flush();
        let answer = read_answer(input)?;
        if answer.is_empty() {
            if let Some(d) = default {
                return Ok(d.clone());
            }
        } else if let Some(n) = names.iter().find(|n| n.eq_ignore_ascii_case(&answer)) {
            return Ok(n.clone());
        }
        let _ = writeln!(out, "  choose one of: {}", names.join(", "));
    }
    Err(Error::usage("no valid rule chosen"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> Draft {
        Draft::new(None, vec!["TM-1".into(), "TM-2".into(), "TM-3".into()], 3)
    }

    fn run(d: &mut Draft, flags: &Flags, assume: bool, script: &str) -> (Result<()>, String) {
        let mut out = Vec::new();
        let r = ask_all(
            &questions(),
            d,
            flags,
            assume,
            &mut script.as_bytes(),
            &mut out,
        );
        (r, String::from_utf8(out).unwrap())
    }

    #[test]
    fn enter_keeps_the_remembered_answers() {
        let remembered = Answers {
            max_concurrent: 3,
            max_teams: 3,
            team_size: 4,
            team_mode: ModeSetting::Heavy,
            profile: Some("claude-code".into()),
            usage_soft: 40,
            usage_hard: 70,
            max_hours: Some(6.0),
            ..Answers::default()
        };
        let mut d = Draft::new(Some(remembered.clone()), vec!["TM-1".into()], 3);
        let (r, shown) = run(&mut d, &Flags::new(), false, &"\n".repeat(11));
        r.unwrap();
        assert_eq!(d.answers, remembered);
        assert!(shown.contains("Teams at the same time [3]"));
        assert!(shown.contains("Agents per team (lead and workers) [4]"));
        assert!(shown.contains("chooses per task) [heavy]"));
        assert!(shown.contains("Agent profile for the teams [claude-code]"));
        assert!(shown.contains("Most hours to run [6]"));
        assert!(shown.contains("Most to spend in USD on runs paid by API key [none]"));
    }

    #[test]
    fn defaults_without_a_remembered_file() {
        let mut d = draft();
        let (r, shown) = run(&mut d, &Flags::new(), false, &"\n".repeat(11));
        r.unwrap();
        assert_eq!(d.answers, Answers::default());
        assert!(shown.contains("Teams at the same time [2]"));
        assert!(shown.contains("Agents per team (lead and workers) [3]"));
        assert!(shown.contains("chooses per task) [orchestrator]"));
        assert!(shown.contains("Agent profile for the teams [claude-code]"));
        assert!(shown.contains("usage percent [50]"));
        assert!(shown.contains("usage percent [80]"));
        assert!(shown.contains("resets [no]"));
        assert!(d.picked.is_none());
    }

    #[test]
    fn typed_answers_replace_the_remembered_ones() {
        let mut d = draft();
        let script = "tm-3, TM-1\n1\ncodex\neco\n30\n60\nyes\n$20\n4.5\n10\n";
        let (r, _) = run(&mut d, &Flags::new(), false, script);
        r.unwrap();
        assert_eq!(d.picked, Some(vec!["TM-3".into(), "TM-1".into()]));
        assert_eq!(d.answers.max_concurrent, 1);
        assert_eq!(d.answers.max_teams, 1);
        assert_eq!(d.answers.profile.as_deref(), Some("codex"));
        assert_eq!(d.answers.team_size, 1);
        assert_eq!(d.answers.team_mode, ModeSetting::Eco);
        assert_eq!(d.answers.usage_soft, 30);
        assert_eq!(d.answers.usage_hard, 60);
        assert!(d.answers.resume_after_reset);
        assert_eq!(d.answers.max_cost_usd, Some(20.0));
        assert_eq!(d.answers.max_hours, Some(4.5));
        assert_eq!(d.answers.max_tasks, Some(10));
    }

    #[test]
    fn none_clears_a_remembered_limit() {
        let mut d = Draft::new(
            Some(Answers {
                max_hours: Some(3.0),
                ..Answers::default()
            }),
            vec![],
            3,
        );
        let mut flags = Flags::new();
        flags.insert("tasks", "all".into());
        flags.insert("max-hours", "none".into());
        let (r, _) = run(&mut d, &flags, true, "");
        r.unwrap();
        assert_eq!(d.answers.max_hours, None);
    }

    #[test]
    fn a_flag_answers_without_asking() {
        let mut d = draft();
        let mut flags = Flags::new();
        flags.insert("max-teams", "1".into());
        flags.insert("tasks", "TM-2".into());
        let (r, shown) = run(&mut d, &flags, false, &"\n".repeat(9));
        r.unwrap();
        assert_eq!(d.answers.max_concurrent, 1);
        assert_eq!(d.picked, Some(vec!["TM-2".into()]));
        assert!(!shown.contains("Teams at the same time"));
        assert!(!shown.contains("Which tasks"));
    }

    #[test]
    fn yes_reuses_everything_and_asks_nothing() {
        let remembered = Answers {
            max_concurrent: 1,
            max_teams: 1,
            max_cost_usd: Some(5.0),
            ..Answers::default()
        };
        let mut d = Draft::new(Some(remembered.clone()), vec!["TM-1".into()], 3);
        let (r, shown) = run(&mut d, &Flags::new(), true, "");
        r.unwrap();
        assert_eq!(d.answers, remembered);
        assert_eq!(shown, "");
    }

    #[test]
    fn a_remembered_pick_of_tasks_is_never_reused() {
        let mut d = Draft::new(
            Some(Answers {
                all_tasks: false,
                ..Answers::default()
            }),
            vec!["TM-1".into()],
            3,
        );
        let (r, _) = run(&mut d, &Flags::new(), true, "");
        r.unwrap();
        assert!(d.picked.is_none());
        assert!(d.answers.all_tasks);
    }

    #[test]
    fn a_bad_flag_is_a_usage_error_naming_the_flag() {
        let mut d = draft();
        let mut flags = Flags::new();
        flags.insert("max-teams", "9".into());
        let (r, _) = run(&mut d, &flags, true, "");
        let e = r.unwrap_err();
        assert_eq!(e.exit_code(), 2);
        assert!(e.message.contains("--max-teams"), "{}", e.message);
    }

    #[test]
    fn the_pause_level_must_be_above_the_stop_level() {
        let mut d = draft();
        let mut flags = Flags::new();
        flags.insert("usage-soft", "85".into());
        let (r, _) = run(&mut d, &flags, true, "");
        assert!(r.unwrap_err().message.contains("must be above"));
        let mut d = draft();
        let mut flags = Flags::new();
        flags.insert("usage-soft", "60".into());
        flags.insert("usage-hard", "60".into());
        assert!(run(&mut d, &flags, true, "").0.is_err());
    }

    #[test]
    fn the_concurrency_is_capped_by_the_rule() {
        let mut d = Draft::new(None, vec![], 1);
        let mut flags = Flags::new();
        flags.insert("max-teams", "2".into());
        assert!(run(&mut d, &flags, true, "").0.is_err());
    }

    #[test]
    fn no_usage_questions_without_the_guard() {
        let mut d = draft();
        d.answers.usage_guard = false;
        let (r, shown) = run(&mut d, &Flags::new(), false, &"\n".repeat(8));
        r.unwrap();
        assert!(!shown.contains("usage percent"));
        assert!(!shown.contains("resets"));
    }

    #[test]
    fn an_invalid_answer_is_asked_again_then_given_up() {
        let mut d = draft();
        let (r, shown) = run(&mut d, &Flags::new(), false, "TM-9\nTM-1\n");
        r.unwrap_err();
        assert!(shown.contains("TM-9 is not in the list above"));
        let mut d = draft();
        let (r, _) = run(
            &mut d,
            &Flags::new(),
            false,
            "TM-9\nTM-1\n\nabc\n\n\n\n\n\n\n\n\n\n\n",
        );
        r.unwrap();
        assert_eq!(d.picked, Some(vec!["TM-1".into()]));
    }

    #[test]
    fn end_of_input_stops_the_interview() {
        let mut d = draft();
        let e = run(&mut d, &Flags::new(), false, "").0.unwrap_err();
        assert!(e.message.contains("nothing was started"));
    }

    #[test]
    fn the_old_max_concurrent_answer_seeds_max_teams() {
        let d = Draft::new(
            Some(Answers {
                max_concurrent: 3,
                ..Answers::default()
            }),
            vec![],
            3,
        );
        assert_eq!(d.answers.max_teams, 3);
    }

    #[test]
    fn max_teams_is_capped_by_the_rule_and_mirrors_into_max_concurrent() {
        let mut d = Draft::new(
            Some(Answers {
                max_concurrent: 3,
                ..Answers::default()
            }),
            vec![],
            2,
        );
        run(&mut d, &Flags::new(), true, "").0.unwrap();
        assert_eq!((d.answers.max_teams, d.answers.max_concurrent), (2, 2));
        let mut d = draft();
        let mut flags = Flags::new();
        flags.insert("max-teams", "3".into());
        run(&mut d, &flags, true, "").0.unwrap();
        assert_eq!((d.answers.max_teams, d.answers.max_concurrent), (3, 3));
        for bad in ["0", "4", "x"] {
            let mut flags = Flags::new();
            flags.insert("max-teams", bad.into());
            assert!(run(&mut draft(), &flags, true, "").0.is_err(), "{bad}");
        }
    }

    #[test]
    fn team_size_and_mode_have_ranges_and_words() {
        for (size, ok) in [("0", false), ("1", true), ("5", true), ("6", false)] {
            let mut flags = Flags::new();
            flags.insert("team-size", size.into());
            assert_eq!(run(&mut draft(), &flags, true, "").0.is_ok(), ok, "{size}");
        }
        for (mode, want) in [
            ("eco", Some(ModeSetting::Eco)),
            ("HEAVY", Some(ModeSetting::Heavy)),
            ("orchestrator", Some(ModeSetting::Orchestrator)),
            ("fast", None),
        ] {
            let mut d = draft();
            let mut flags = Flags::new();
            flags.insert("team-mode", mode.into());
            let (r, _) = run(&mut d, &flags, true, "");
            match want {
                Some(m) => assert_eq!(d.answers.team_mode, m),
                None => assert!(r.unwrap_err().message.contains("--team-mode")),
            }
        }
    }

    #[test]
    fn codex_forces_one_agent_per_team_and_says_so() {
        let mut d = draft();
        let mut flags = Flags::new();
        flags.insert("profile", "codex".into());
        flags.insert("team-size", "4".into());
        let (r, shown) = run(&mut d, &flags, false, &"\n".repeat(9));
        r.unwrap();
        assert_eq!(d.answers.team_size, 1);
        assert!(shown.contains("codex runs one agent per team"));
        assert!(!shown.contains("Agents per team"));
    }

    #[test]
    fn a_rule_that_is_already_on_codex_gets_one_agent_per_team() {
        let mut d = draft().with_rule("codex", None);
        run(&mut d, &Flags::new(), true, "").0.unwrap();
        assert_eq!(d.answers.team_size, 1);
        assert_eq!(d.profile(), "codex");
    }

    #[test]
    fn codex_is_refused_when_the_rule_needs_a_mode_it_lacks() {
        let mut flags = Flags::new();
        flags.insert("profile", "codex".into());
        let mut d = draft().with_rule("claude-code", Some("accept-edits"));
        let e = run(&mut d, &flags, true, "").0.unwrap_err();
        assert!(e.message.contains("--profile"), "{}", e.message);
        assert!(e.message.contains("accept-edits"), "{}", e.message);
        let mut d = draft().with_rule("claude-code", Some("bypass"));
        run(&mut d, &flags, true, "").0.unwrap();
        let mut d = draft().with_rule("claude-code", None);
        run(&mut d, &flags, true, "").0.unwrap();
    }

    #[test]
    fn a_remembered_codex_profile_is_rechecked_against_a_rule_mode() {
        let mut d = Draft::new(
            Some(Answers {
                profile: Some("codex".into()),
                ..Answers::default()
            }),
            vec![],
            3,
        )
        .with_rule("claude-code", Some("plan"));
        assert!(run(&mut d, &Flags::new(), true, "").0.is_err());
    }

    #[test]
    fn only_claude_and_codex_profiles_are_accepted() {
        for bad in ["agy", "nope", "opencode"] {
            let mut flags = Flags::new();
            flags.insert("profile", bad.into());
            let e = run(&mut draft(), &flags, true, "").0.unwrap_err();
            assert!(e.message.contains("--profile"), "{bad}: {}", e.message);
        }
        let mut flags = Flags::new();
        flags.insert("profile", "claude-code".into());
        run(&mut draft(), &flags, true, "").0.unwrap();
    }

    #[test]
    fn only_skip_and_order_narrow_the_pick_and_are_never_remembered() {
        let mut d = draft();
        run(&mut d, &Flags::new(), true, "").0.unwrap();
        let before = d.answers.clone();
        d.restrict(Some("tm-1,TM-2"), Some("TM-1"), Some("TM-2"))
            .unwrap();
        assert!(!d.is_chosen("TM-1"));
        assert!(d.is_chosen("TM-2"));
        assert!(!d.is_chosen("TM-3"));
        assert_eq!(d.order, vec!["TM-2".to_string()]);
        assert_eq!(d.answers, before);
        assert!(d.picked.is_none());
    }

    #[test]
    fn only_works_on_top_of_the_picked_tasks() {
        let mut d = draft();
        d.picked = Some(vec!["TM-1".into(), "TM-2".into()]);
        d.restrict(Some("TM-2,TM-3"), None, None).unwrap();
        assert!(d.is_chosen("TM-2"));
        assert!(!d.is_chosen("TM-1"));
        assert!(!d.is_chosen("TM-3"));
    }

    #[test]
    fn unknown_or_unchosen_keys_in_the_filters_are_usage_errors() {
        let e = draft().restrict(Some("TM-9"), None, None).unwrap_err();
        assert!(e.message.contains("--only") && e.message.contains("TM-9"));
        let e = draft().restrict(None, Some("TM-9"), None).unwrap_err();
        assert!(e.message.contains("--skip"));
        let e = draft().restrict(None, None, Some("TM-9")).unwrap_err();
        assert!(e.message.contains("--order"));
        let e = draft()
            .restrict(None, Some("TM-2"), Some("TM-1,TM-2"))
            .unwrap_err();
        assert!(e.message.contains("--order") && e.message.contains("TM-2"));
        assert_eq!(e.exit_code(), 2);
    }

    #[test]
    fn a_fresh_draft_carries_no_filters_from_the_remembered_answers() {
        let mut d = draft();
        d.restrict(Some("TM-1"), Some("TM-2"), Some("TM-1"))
            .unwrap();
        let json = serde_json::to_string(&d.answers).unwrap();
        for word in ["only", "skip", "order", "dry"] {
            assert!(!json.contains(word), "{word} in {json}");
        }
        let again = Draft::new(Some(d.answers.clone()), vec!["TM-1".into()], 3);
        assert!(again.only.is_none() && again.skip.is_empty() && again.order.is_empty());
    }

    #[test]
    fn the_rule_is_chosen_by_flag_alone_remembered_or_asked() {
        let names = vec!["tm".to_string(), "web".to_string()];
        let pick = |flag, rem, assume, script: &str| {
            choose_rule(
                &names,
                flag,
                rem,
                assume,
                &mut script.as_bytes(),
                &mut Vec::new(),
            )
        };
        assert_eq!(pick(Some("WEB"), None, true, "").unwrap(), "web");
        assert!(pick(Some("zz"), None, true, "").is_err());
        assert_eq!(pick(None, Some("tm"), true, "").unwrap(), "tm");
        assert!(pick(None, None, true, "").is_err());
        assert_eq!(pick(None, Some("tm"), false, "\n").unwrap(), "tm");
        assert_eq!(pick(None, Some("tm"), false, "web\n").unwrap(), "web");
        assert!(pick(None, None, false, "\nzz\n\n").is_err());
        let one = vec!["tm".to_string()];
        assert_eq!(
            choose_rule(&one, None, None, false, &mut "".as_bytes(), &mut Vec::new()).unwrap(),
            "tm"
        );
    }
}
