use std::io::{BufRead, IsTerminal, Write};
use tmgr_core::error::{Error, Kind, Result};

pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

pub fn confirm(question: &str, accept_yes_flag: bool, yes_flag: bool) -> Result<()> {
    if accept_yes_flag && yes_flag {
        return Ok(());
    }
    if !is_interactive() {
        let hint = if accept_yes_flag {
            "run it in a terminal or pass --yes"
        } else {
            "run it in a terminal: this confirmation never accepts --yes"
        };
        return Err(Error::needs_confirmation("confirmation required").hint(hint));
    }
    eprint!("{question} [y/N] ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|_| Error::new(Kind::Interrupted, "could not read the answer"))?;
    if matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
        Ok(())
    } else {
        Err(Error::new(
            Kind::Interrupted,
            "cancelled, nothing was changed",
        ))
    }
}

pub fn confirm_typed(prompt: &str, word: &str) -> Result<()> {
    if !is_interactive() {
        return Err(Error::needs_confirmation("confirmation required")
            .hint("run it in a terminal: this confirmation never accepts --yes"));
    }
    eprint!("{prompt}");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|_| Error::new(Kind::Interrupted, "could not read the answer"))?;
    if line.trim() == word {
        Ok(())
    } else {
        Err(Error::new(
            Kind::Interrupted,
            "cancelled, nothing was changed",
        ))
    }
}
