use std::collections::BTreeMap;
use std::io::{BufRead, Write};

use tmgr_core::error::{Error, Kind, Result};
use tmgr_queue::rules::GLOBAL_MAX;

use super::model::Answers;

pub type Flags = BTreeMap<&'static str, String>;

const TRIES: usize = 3;

pub struct Draft {
    pub answers: Answers,
    pub picked: Option<Vec<String>>,
    pub known_keys: Vec<String>,
    pub rule_max: u32,
}

impl Draft {
    pub fn new(remembered: Option<Answers>, known_keys: Vec<String>, rule_max: u32) -> Draft {
        let mut answers = remembered.unwrap_or_default();
        answers.all_tasks = true;
        Draft {
            answers,
            picked: None,
            known_keys,
            rule_max,
        }
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

fn tasks_apply(d: &mut Draft, s: &str) -> std::result::Result<(), String> {
    if s.trim().eq_ignore_ascii_case("all") {
        d.picked = None;
        return Ok(());
    }
    let mut keys = Vec::new();
    for part in s.split(',').map(str::trim).filter(|p| !p.is_empty()) {
        let known = d
            .known_keys
            .iter()
            .find(|k| k.eq_ignore_ascii_case(part))
            .ok_or_else(|| format!("{part} is not in the list above"))?;
        if !keys.contains(known) {
            keys.push(known.clone());
        }
    }
    if keys.is_empty() {
        return Err("enter all, or task keys separated by commas".into());
    }
    d.picked = Some(keys);
    Ok(())
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

/// The questions after the rule and the task list. TM-623 adds max teams, team size, mode and
/// the like by appending items here and a flag of the same name in `StartArgs::flags`.
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
            id: "max-concurrent",
            label: "Agents at the same time",
            current: |d| d.answers.max_concurrent.min(d.concurrent_cap()).to_string(),
            apply: |d, s| {
                d.answers.max_concurrent = parse_range(s, 1, d.concurrent_cap())?;
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
    if a.max_concurrent > d.concurrent_cap() {
        return Err(Error::usage(format!(
            "at most {} agents at the same time",
            d.concurrent_cap()
        )));
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
            usage_soft: 40,
            usage_hard: 70,
            max_hours: Some(6.0),
            ..Answers::default()
        };
        let mut d = Draft::new(Some(remembered.clone()), vec!["TM-1".into()], 3);
        let (r, shown) = run(&mut d, &Flags::new(), false, &"\n".repeat(8));
        r.unwrap();
        assert_eq!(d.answers, remembered);
        assert!(shown.contains("Agents at the same time [3]"));
        assert!(shown.contains("Most hours to run [6]"));
        assert!(shown.contains("Most to spend in USD on runs paid by API key [none]"));
    }

    #[test]
    fn defaults_without_a_remembered_file() {
        let mut d = draft();
        let (r, shown) = run(&mut d, &Flags::new(), false, &"\n".repeat(8));
        r.unwrap();
        assert_eq!(d.answers, Answers::default());
        assert!(shown.contains("Agents at the same time [2]"));
        assert!(shown.contains("usage percent [50]"));
        assert!(shown.contains("usage percent [80]"));
        assert!(shown.contains("resets [no]"));
        assert!(d.picked.is_none());
    }

    #[test]
    fn typed_answers_replace_the_remembered_ones() {
        let mut d = draft();
        let script = "tm-3, TM-1\n1\n30\n60\nyes\n$20\n4.5\n10\n";
        let (r, _) = run(&mut d, &Flags::new(), false, script);
        r.unwrap();
        assert_eq!(d.picked, Some(vec!["TM-3".into(), "TM-1".into()]));
        assert_eq!(d.answers.max_concurrent, 1);
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
        flags.insert("max-concurrent", "1".into());
        flags.insert("tasks", "TM-2".into());
        let (r, shown) = run(&mut d, &flags, false, &"\n".repeat(6));
        r.unwrap();
        assert_eq!(d.answers.max_concurrent, 1);
        assert_eq!(d.picked, Some(vec!["TM-2".into()]));
        assert!(!shown.contains("Agents at the same time"));
        assert!(!shown.contains("Which tasks"));
    }

    #[test]
    fn yes_reuses_everything_and_asks_nothing() {
        let remembered = Answers {
            max_concurrent: 1,
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
        flags.insert("max-concurrent", "9".into());
        let (r, _) = run(&mut d, &flags, true, "");
        let e = r.unwrap_err();
        assert_eq!(e.exit_code(), 2);
        assert!(e.message.contains("--max-concurrent"), "{}", e.message);
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
        flags.insert("max-concurrent", "2".into());
        assert!(run(&mut d, &flags, true, "").0.is_err());
    }

    #[test]
    fn no_usage_questions_without_the_guard() {
        let mut d = draft();
        d.answers.usage_guard = false;
        let (r, shown) = run(&mut d, &Flags::new(), false, &"\n".repeat(5));
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
            "TM-9\nTM-1\n\nabc\n\n\n\n\n\n\n",
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
