use clap::error::{ContextKind, ContextValue, ErrorKind};
use tmgr_core::error::Error;

pub fn friendly(e: &clap::Error) -> Error {
    let arg = e.get(ContextKind::InvalidArg).map(ToString::to_string);
    let arg_name = arg
        .as_deref()
        .map(|a| a.split_whitespace().next().unwrap_or(a));
    if matches!(
        e.kind(),
        ErrorKind::InvalidValue | ErrorKind::MissingRequiredArgument
    ) && arg_name == Some("--ws")
        && e.to_string().contains("none was supplied")
    {
        return Error::usage("`--ws` needs a workspace code")
            .hint("see `tmgr ws list`, e.g. `tmgr --ws personal task list`");
    }
    if e.kind() == ErrorKind::UnknownArgument && arg_name == Some("--list") {
        return Error::usage("unexpected argument '--list'")
            .hint("did you mean `tmgr ws list` or `tmgr task list`?");
    }
    let hint = match suggestion(e) {
        Some(s) => format!("did you mean `{s}`?"),
        None => "tmgr --help".to_string(),
    };
    Error::usage(first_line(e)).hint(hint)
}

fn suggestion(e: &clap::Error) -> Option<String> {
    [ContextKind::SuggestedSubcommand, ContextKind::SuggestedArg]
        .into_iter()
        .find_map(|k| match e.get(k)? {
            ContextValue::String(s) => Some(s.clone()),
            ContextValue::Strings(v) => v.first().cloned(),
            _ => None,
        })
}

fn first_line(e: &clap::Error) -> String {
    let rendered = e.to_string();
    let line = rendered
        .lines()
        .next()
        .unwrap_or("invalid usage")
        .trim_start_matches("error: ");
    tmgr_core::text::sanitize_line(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::Cli;
    use clap::Parser;

    fn err(args: &[&str]) -> Error {
        let e = Cli::try_parse_from(std::iter::once("tmgr").chain(args.iter().copied()))
            .err()
            .expect("parse fails");
        friendly(&e)
    }

    #[test]
    fn ws_without_a_value_asks_for_a_code() {
        let e = err(&["--ws"]);
        assert_eq!(e.message, "`--ws` needs a workspace code");
        assert!(e.hint.unwrap().contains("tmgr ws list"));
    }

    #[test]
    fn list_flag_points_at_the_list_commands() {
        let e = err(&["--list"]);
        assert_eq!(
            e.hint.as_deref(),
            Some("did you mean `tmgr ws list` or `tmgr task list`?")
        );
    }

    #[test]
    fn unknown_subcommand_keeps_the_clap_suggestion() {
        let e = err(&["statu"]);
        assert!(e.hint.as_ref().unwrap().contains("status"));
        assert_eq!(e.exit_code(), 2);
    }
}
