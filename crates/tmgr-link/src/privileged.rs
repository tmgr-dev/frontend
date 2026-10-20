use std::io::{BufRead, IsTerminal, Write};
use tmgr_core::error::{Error, Kind, Result};

pub const RUN_ENV: [&str; 2] = ["TMGR_RUN_TOKEN", "TMGR_RUN_ID"];

pub fn check(in_agent_session: bool, interactive: bool) -> Result<()> {
    if in_agent_session {
        return Err(
            crate::forbidden("privileged change refused inside an agent session")
                .hint("run it yourself in a normal terminal"),
        );
    }
    if !interactive {
        return Err(
            Error::needs_confirmation("this change needs an interactive terminal")
                .hint("run it in a terminal: this confirmation never accepts --yes"),
        );
    }
    Ok(())
}

pub fn guard() -> Result<()> {
    let in_agent = RUN_ENV.iter().any(|k| std::env::var_os(k).is_some());
    let tty = std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && std::io::stderr().is_terminal();
    check(in_agent, tty)
}

pub fn confirm_typed(
    expected: &str,
    prompt: &str,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> Result<()> {
    let _ = write!(out, "{prompt} ");
    let _ = out.flush();
    let mut line = String::new();
    let n = input
        .read_line(&mut line)
        .map_err(|_| Error::new(Kind::Interrupted, "could not read the answer"))?;
    if n > 0 && line.trim_end_matches(['\n', '\r']) == expected {
        Ok(())
    } else {
        Err(Error::new(
            Kind::Interrupted,
            "confirmation did not match, nothing was changed",
        ))
    }
}

pub fn confirm_typed_stdio(expected: &str, prompt: &str) -> Result<()> {
    confirm_typed(
        expected,
        prompt,
        &mut std::io::stdin().lock(),
        &mut std::io::stderr(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_sessions_are_refused_with_exit_5_even_on_a_tty() {
        let e = check(true, true).unwrap_err();
        assert_eq!(e.exit_code(), 5);
        assert!(e.message.contains("refused inside an agent session"));
    }

    #[test]
    fn non_tty_needs_confirmation_exit_10() {
        assert_eq!(check(false, false).unwrap_err().exit_code(), 10);
        assert_eq!(check(true, false).unwrap_err().exit_code(), 5);
        assert!(check(false, true).is_ok());
    }

    #[test]
    fn typed_confirmation_must_match_exactly() {
        let ok = |s: &str| confirm_typed("laptop", "?", &mut s.as_bytes(), &mut Vec::new());
        assert!(ok("laptop\n").is_ok());
        assert!(ok("laptop\r\n").is_ok());
        assert!(ok("Laptop\n").is_err());
        assert!(ok("laptop \n").is_err());
        assert!(ok("y\n").is_err());
        assert!(ok("").is_err());
    }
}
