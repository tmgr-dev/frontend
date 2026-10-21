use crate::ctx::Ctx;
use crate::out::Output;
use chrono::Utc;
use clap::{Args, Subcommand, ValueEnum};
use serde_json::json;
use tmgr_core::error::{Error, Result};
use tmgr_core::text::sanitize_line;
use tmgr_runner::questions::{ago, Entry, Status, Store};

#[derive(Args)]
#[command(args_conflicts_with_subcommands = true)]
pub struct QuestionsArgs {
    #[command(subcommand)]
    pub command: Option<QuestionsCommand>,
    /// Only the questions of this session
    #[arg(long, value_name = "ID")]
    pub session: Option<String>,
    /// Which questions to list
    #[arg(long, value_enum, default_value_t = Which::Open)]
    pub status: Which,
}

#[derive(Subcommand)]
pub enum QuestionsCommand {
    /// Mark a question as answered (use it when you replied in the session yourself)
    Answer {
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Hide a question from the open list
    Ignore {
        #[arg(value_name = "ID")]
        id: String,
    },
    /// Show or change asking Haiku (through your own `claude`) to tell real questions from rhetorical ones
    Haiku {
        #[arg(value_enum)]
        state: Option<Switch>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Which {
    Open,
    Answered,
    Ignored,
    All,
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Switch {
    On,
    Off,
}

pub fn run(_ctx: &Ctx, args: &QuestionsArgs) -> Result<Output> {
    let dir = tmgr_ipc::dirs::runner_dir()
        .map_err(|e| Error::internal(format!("cannot locate the runner directory: {e}")))?;
    let store = Store::new(&dir);
    let now_s = Utc::now().timestamp().max(0) as u64;
    match &args.command {
        Some(QuestionsCommand::Answer { id }) => mark(&store, id, Status::Answered, now_s),
        Some(QuestionsCommand::Ignore { id }) => mark(&store, id, Status::Ignored, now_s),
        Some(QuestionsCommand::Haiku { state }) => haiku(&store, *state),
        None => list(&store, args, now_s),
    }
}

fn mark(store: &Store, id: &str, status: Status, now_s: u64) -> Result<Output> {
    let entry = store.find(id).map_err(Error::usage)?;
    store
        .mark(&entry.q.id, status, now_s)
        .map_err(|e| Error::internal(format!("cannot save the mark: {e}")))?;
    Ok(Output::new(
        json!({"id": entry.q.id, "status": status.word()}),
        format!("{} marked {}", entry.q.id, status.word()),
    ))
}

fn haiku(store: &Store, state: Option<Switch>) -> Result<Output> {
    let mut settings = store.settings();
    if let Some(s) = state {
        settings.haiku = s == Switch::On;
        store
            .save_settings(settings)
            .map_err(|e| Error::internal(format!("cannot save the setting: {e}")))?;
    }
    let word = if settings.haiku { "on" } else { "off" };
    let human = if settings.haiku {
        "Haiku is on: the text of an agent message that looks like it asks something is sent through your own `claude` command (model haiku) to pick the real questions"
    } else {
        "Haiku is off: questions are found by rules only and nothing is sent anywhere"
    };
    Ok(Output::new(
        json!({"haiku": settings.haiku}),
        format!("Haiku is {word}\n{human}"),
    ))
}

fn wanted(e: &Entry, which: Which) -> bool {
    match which {
        Which::Open => e.status == Status::Open,
        Which::Answered => e.status == Status::Answered,
        Which::Ignored => e.status == Status::Ignored,
        Which::All => true,
    }
}

fn list(store: &Store, args: &QuestionsArgs, now_s: u64) -> Result<Output> {
    let entries: Vec<Entry> = store
        .entries(args.session.as_deref())
        .into_iter()
        .filter(|e| wanted(e, args.status))
        .collect();
    let mut lines = Vec::new();
    if entries.is_empty() {
        lines.push("No questions".to_string());
    } else {
        lines.push(format!(
            "{:<12}  {:<12}  {:<11}  QUESTION",
            "ID", "SESSION", "ASKED"
        ));
        for e in &entries {
            lines.push(format!(
                "{:<12}  {:<12}  {:<11}  {}",
                e.q.id,
                sanitize_line(&e.q.session),
                ago(now_s, e.q.asked_s),
                sanitize_line(&e.q.text)
            ));
        }
    }
    let data: Vec<_> = entries
        .iter()
        .map(|e| {
            json!({
                "id": e.q.id,
                "session": e.q.session,
                "question": e.q.text,
                "status": e.status.word(),
                "asked_s": e.q.asked_s,
                "origin": e.q.origin,
            })
        })
        .collect();
    Ok(Output::new(
        json!({"questions": data, "haiku": store.settings().haiku}),
        lines.join("\n"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tmgr_runner::questions::{Origin, Question, SessionFile};

    fn seeded() -> (tempfile::TempDir, Store) {
        let d = tempfile::tempdir().unwrap();
        let store = Store::new(d.path());
        let q = |id: &str, text: &str, t: u64| Question {
            id: id.into(),
            session: "s_abcdefghjk".into(),
            text: text.into(),
            asked_s: t,
            origin: Origin::Rule,
            user: String::new(),
            reply: String::new(),
        };
        let file = SessionFile {
            questions: vec![
                q("aaaa11112222", "Should I update the docs?", 100),
                q("bbbb33334444", "Keep the old flag\u{1b}[2J?", 200),
            ],
            ..SessionFile::default()
        };
        store.save("s_abcdefghjk", &file, 300).unwrap();
        (d, store)
    }

    fn args(status: Which) -> QuestionsArgs {
        QuestionsArgs {
            command: None,
            session: None,
            status,
        }
    }

    #[test]
    fn lists_open_questions_and_strips_escapes() {
        let (_d, store) = seeded();
        let out = list(&store, &args(Which::Open), 400).unwrap();
        assert!(out.human.contains("Should I update the docs?"));
        assert!(!out.human.contains('\u{1b}'));
        assert_eq!(out.data["questions"].as_array().unwrap().len(), 2);
        assert_eq!(out.data["questions"][0]["id"], "bbbb33334444");
        assert_eq!(out.data["haiku"], true);
    }

    #[test]
    fn answer_and_ignore_change_the_lists() {
        let (_d, store) = seeded();
        mark(&store, "aaaa", Status::Answered, 500).unwrap();
        mark(&store, "bbbb", Status::Ignored, 501).unwrap();
        assert!(list(&store, &args(Which::Open), 600).unwrap().human == "No questions");
        let ignored = list(&store, &args(Which::Ignored), 600).unwrap();
        assert_eq!(ignored.data["questions"][0]["id"], "bbbb33334444");
        let all = list(&store, &args(Which::All), 600).unwrap();
        assert_eq!(all.data["questions"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn unknown_and_short_ids_are_usage_errors() {
        let (_d, store) = seeded();
        assert!(mark(&store, "zzzz", Status::Answered, 1).is_err());
        assert!(mark(&store, "aa", Status::Answered, 1).is_err());
    }

    #[test]
    fn the_session_filter_applies() {
        let (_d, store) = seeded();
        let mut a = args(Which::All);
        a.session = Some("s_zzzzzzzzzz".into());
        assert_eq!(list(&store, &a, 400).unwrap().human, "No questions");
    }

    #[test]
    fn haiku_switch_persists() {
        let (_d, store) = seeded();
        assert!(haiku(&store, None)
            .unwrap()
            .human
            .starts_with("Haiku is on"));
        assert!(haiku(&store, Some(Switch::Off))
            .unwrap()
            .human
            .starts_with("Haiku is off"));
        assert!(!store.settings().haiku);
        assert_eq!(haiku(&store, Some(Switch::On)).unwrap().data["haiku"], true);
    }
}
