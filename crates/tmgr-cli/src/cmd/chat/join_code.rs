use std::io::{self, BufRead};
use tmgr_chat_relay::engine::OLD_CODE_MSG;
use tmgr_chat_relay::trust::{decode_join_code, JoinCode, JOIN_PREFIX, JOIN_PREFIX_V2};
use tmgr_core::error::{Error, Result};

const PIPE_HINT: &str = "pipe the code in (echo ... | tmgr chat join) or type it at the prompt";

fn read_line(
    mut stdin: impl BufRead,
    is_tty: bool,
    mut prompt: impl FnMut() -> io::Result<String>,
) -> Result<String> {
    let line = if is_tty {
        prompt()
    } else {
        let mut line = String::new();
        stdin.read_line(&mut line).map(|_| line)
    }
    .map_err(|_| Error::usage("cannot read the join code"))?;
    let line = line.trim();
    if line.is_empty() {
        return Err(Error::usage("no join code given").hint(PIPE_HINT));
    }
    Ok(line.to_owned())
}

pub fn read_join_text(
    stdin: impl BufRead,
    is_tty: bool,
    prompt: impl FnMut() -> io::Result<String>,
) -> Result<String> {
    let line = read_line(stdin, is_tty, prompt)?;
    if line.starts_with(JOIN_PREFIX) {
        return Err(Error::usage(OLD_CODE_MSG));
    }
    if !line.starts_with(JOIN_PREFIX_V2) {
        return Err(Error::usage("that is not a valid join code").hint("copy the whole code"));
    }
    Ok(line)
}

pub fn read_join_code(
    stdin: impl BufRead,
    is_tty: bool,
    prompt: impl FnMut() -> io::Result<String>,
) -> Result<JoinCode> {
    let line = read_line(stdin, is_tty, prompt)?;
    let code = decode_join_code(&line)
        .map_err(|_| Error::usage("that is not a valid join code").hint("copy the whole code"))?;
    if code.version < 2 {
        return Err(Error::usage(OLD_CODE_MSG));
    }
    Ok(code)
}

pub fn reject_argv_code(_: &str) -> std::result::Result<String, String> {
    Err(format!(
        "a join code must not be passed as an argument; {PIPE_HINT}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use tmgr_chat_relay::trust::encode_join_code;

    fn code() -> JoinCode {
        JoinCode {
            version: 2,
            kind: Some("desktop".into()),
            user_id: 42,
            device_id: "00000000-0000-4000-8000-00000000000a".into(),
            fp: "ab".repeat(32),
            secret: [9u8; 32],
            exp_ms: 1_800_000_600_000,
        }
    }

    fn no_prompt() -> io::Result<String> {
        panic!("prompt must not run")
    }

    #[test]
    fn piped_line_is_trimmed_and_decoded() {
        let enc = encode_join_code(&code()).unwrap();
        let input = Cursor::new(format!("  {enc} \r\nrest\n"));
        assert_eq!(read_join_code(input, false, no_prompt).unwrap(), code());
    }

    #[test]
    fn tty_uses_the_prompt_and_ignores_stdin() {
        let enc = encode_join_code(&code()).unwrap();
        let got = read_join_code(Cursor::new("junk\n"), true, || Ok(format!("{enc}\n"))).unwrap();
        assert_eq!(got, code());
    }

    #[test]
    fn empty_input_is_an_error_with_hint() {
        let err = read_join_code(Cursor::new(""), false, no_prompt).unwrap_err();
        assert!(err.to_string().contains("no join code"));
    }

    #[test]
    fn bad_code_error_never_echoes_it() {
        let bad = "tmgr-chat-join:v1:SECRETSECRET";
        let err = read_join_code(Cursor::new(format!("{bad}\n")), false, no_prompt).unwrap_err();
        let shown = format!("{err} {:?}", err);
        assert!(!shown.contains("SECRETSECRET"));
    }

    #[test]
    fn join_text_keeps_the_raw_code_and_checks_only_the_prefix() {
        let raw = format!("{JOIN_PREFIX_V2}anything-with-a-kind-field");
        let got = read_join_text(Cursor::new(format!(" {raw}\n")), false, no_prompt).unwrap();
        assert_eq!(got, raw);
        let err = read_join_text(Cursor::new("SECRETSECRET\n"), false, no_prompt).unwrap_err();
        assert!(!format!("{err}").contains("SECRETSECRET"));
    }

    #[test]
    fn m12_a_v1_code_is_rejected_with_an_update_message() {
        let v1 = JoinCode {
            version: 1,
            kind: None,
            ..code()
        };
        let enc = encode_join_code(&v1).unwrap();
        let err = read_join_code(Cursor::new(format!("{enc}\n")), false, no_prompt).unwrap_err();
        assert!(err.to_string().contains(OLD_CODE_MSG));
        let err = read_join_text(Cursor::new(format!("{enc}\n")), false, no_prompt).unwrap_err();
        assert_eq!(
            OLD_CODE_MSG,
            "This join code is from an older app version. Update the desktop or mobile app that showed it and generate a new code."
        );
        assert!(err.to_string().contains(OLD_CODE_MSG));
        assert!(!err.to_string().contains(&enc));
    }

    #[test]
    fn argv_code_is_rejected_with_hint() {
        let shown = reject_argv_code("tmgr-chat-join:v1:SECRETSECRET").unwrap_err();
        assert!(shown.contains("pipe"));
        assert!(!shown.contains("SECRETSECRET"));
    }
}
