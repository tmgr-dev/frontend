use crate::cli::{PromptArgs, StatuslineTarget};
use std::io::{IsTerminal, Read, Write};
use tmgr_core::active;
use tmgr_core::prompt::{render, Format};

const MAX_STDIN: u64 = 256 * 1024;

fn now_s() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn wants_color(no_color_flag: bool) -> bool {
    !no_color_flag && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty())
}

pub fn prompt(args: &PromptArgs, no_color_flag: bool) -> i32 {
    let Some(format) = Format::parse(&args.format) else {
        eprintln!("error: unknown format (use plain, zsh, bash, fish, starship, tmux or claude)");
        return 2;
    };
    let state = active::load();
    let text = render(state.as_ref(), now_s(), format, wants_color(no_color_flag));
    if text.is_empty() {
        return 0;
    }
    let mut out = std::io::stdout().lock();
    let _ = if format.is_shell_prompt() {
        write!(out, "{text}")
    } else {
        writeln!(out, "{text}")
    };
    0
}

pub fn statusline(target: StatuslineTarget) -> i32 {
    let StatuslineTarget::Claude = target;
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        let mut buf = Vec::new();
        let _ = stdin.lock().take(MAX_STDIN + 1).read_to_end(&mut buf);
        let _ = std::io::copy(&mut stdin.lock(), &mut std::io::sink());
    }
    let state = active::load();
    let text = render(state.as_ref(), now_s(), Format::Claude, false);
    if !text.is_empty() {
        println!("{text}");
    }
    0
}
