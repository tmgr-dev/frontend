use crate::lines::MAX_LINE;
use crate::locate::open_regular;
use crate::Agent;
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub const WINDOW: usize = 1 << 20;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Replies {
    pub texts: Vec<String>,
    pub offset: u64,
}

pub fn len(path: &Path) -> Result<u64, String> {
    open_regular(path)?
        .metadata()
        .map(|m| m.len())
        .map_err(|e| e.to_string())
}

pub fn read_new(path: &Path, agent: Agent, offset: u64, window: usize) -> Result<Replies, String> {
    let mut f = open_regular(path)?;
    let size = f.metadata().map_err(|e| e.to_string())?.len();
    let offset = if offset > size { 0 } else { offset };
    let mut want = window.max(1);
    loop {
        f.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
        let mut buf = Vec::new();
        (&mut f)
            .take(want as u64)
            .read_to_end(&mut buf)
            .map_err(|e| e.to_string())?;
        let Some(end) = buf.iter().rposition(|b| *b == b'\n') else {
            if buf.len() < want || buf.is_empty() {
                return Ok(Replies {
                    texts: Vec::new(),
                    offset,
                });
            }
            if want <= MAX_LINE {
                want = MAX_LINE + 1;
                continue;
            }
            return Ok(Replies {
                texts: Vec::new(),
                offset: offset + buf.len() as u64,
            });
        };
        let texts = buf[..end]
            .split(|b| *b == b'\n')
            .filter(|l| l.len() <= MAX_LINE)
            .filter_map(|l| serde_json::from_slice::<Value>(l).ok())
            .filter_map(|v| reply(agent, &v))
            .collect();
        return Ok(Replies {
            texts,
            offset: offset + end as u64 + 1,
        });
    }
}

pub fn reply(agent: Agent, v: &Value) -> Option<String> {
    let text = match agent {
        Agent::Claude => {
            if v["type"].as_str() != Some("assistant")
                || v["isSidechain"].as_bool() == Some(true)
                || v["message"]["model"].as_str() == Some("<synthetic>")
            {
                return None;
            }
            v["message"]["content"]
                .as_array()?
                .iter()
                .filter(|b| b["type"].as_str() == Some("text"))
                .filter_map(|b| b["text"].as_str())
                .collect::<String>()
        }
        Agent::Codex => {
            let p = &v["payload"];
            if v["type"].as_str() != Some("response_item")
                || p["type"].as_str() != Some("message")
                || p["role"].as_str() != Some("assistant")
            {
                return None;
            }
            p["content"]
                .as_array()?
                .iter()
                .filter_map(|c| c["text"].as_str())
                .collect::<String>()
        }
    };
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn claude_line(text: &str) -> String {
        json!({"type": "assistant", "message": {"model": "m", "content": [{"type": "text", "text": text}]}}).to_string()
    }

    fn append(path: &Path, s: &str) {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        f.write_all(s.as_bytes()).unwrap();
    }

    #[test]
    fn claude_only_assistant_text_counts() {
        let tool = json!({"type": "assistant", "message": {"content": [{"type": "tool_use", "name": "Bash", "input": {}}]}});
        let user = json!({"type": "user", "message": {"content": "question"}});
        let side = json!({"type": "assistant", "isSidechain": true, "message": {"content": [{"type": "text", "text": "side"}]}});
        let synthetic = json!({"type": "assistant", "message": {"model": "<synthetic>", "content": [{"type": "text", "text": "No response requested."}]}});
        let blank = json!({"type": "assistant", "message": {"content": [{"type": "text", "text": "  \n"}]}});
        let mixed = json!({"type": "assistant", "message": {"content": [
            {"type": "thinking", "thinking": "hmm"}, {"type": "text", "text": "Done. "}, {"type": "tool_use", "name": "x", "input": {}}]}});
        for v in [&tool, &user, &side, &synthetic, &blank] {
            assert_eq!(reply(Agent::Claude, v), None);
        }
        assert_eq!(reply(Agent::Claude, &mixed).as_deref(), Some("Done."));
    }

    #[test]
    fn codex_only_assistant_messages_count() {
        let msg = |role: &str, t: &str| {
            json!({"type": "response_item", "payload": {"type": "message", "role": role,
            "content": [{"type": "output_text", "text": t}]}})
        };
        let call = json!({"type": "response_item", "payload": {"type": "function_call", "name": "shell", "arguments": "{}"}});
        assert_eq!(
            reply(Agent::Codex, &msg("assistant", "All green")).as_deref(),
            Some("All green")
        );
        assert_eq!(reply(Agent::Codex, &msg("user", "hi")), None);
        assert_eq!(reply(Agent::Codex, &call), None);
        assert_eq!(
            reply(
                Agent::Codex,
                &json!({"type": "event_msg", "payload": {"type": "agent_message", "message": "dup"}})
            ),
            None
        );
    }

    #[test]
    fn reading_is_incremental_and_leaves_a_partial_line_for_later() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("log.jsonl");
        append(
            &p,
            &format!("{}\n{}\n", claude_line("one"), claude_line("two")),
        );
        let a = read_new(&p, Agent::Claude, 0, WINDOW).unwrap();
        assert_eq!(a.texts, ["one", "two"]);
        assert_eq!(a.offset, len(&p).unwrap());

        let partial = claude_line("three");
        append(&p, &partial[..10]);
        let b = read_new(&p, Agent::Claude, a.offset, WINDOW).unwrap();
        assert!(b.texts.is_empty());
        assert_eq!(b.offset, a.offset);

        append(&p, &format!("{}\n", &partial[10..]));
        let c = read_new(&p, Agent::Claude, b.offset, WINDOW).unwrap();
        assert_eq!(c.texts, ["three"]);
        assert_eq!(
            read_new(&p, Agent::Claude, c.offset, WINDOW)
                .unwrap()
                .texts
                .len(),
            0
        );
    }

    #[test]
    fn a_small_window_reads_in_bounded_steps() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("log.jsonl");
        let lines: Vec<String> = (0..6).map(|i| claude_line(&format!("m{i}"))).collect();
        append(&p, &(lines.join("\n") + "\n"));
        let mut offset = 0;
        let mut got = Vec::new();
        for _ in 0..20 {
            let r = read_new(&p, Agent::Claude, offset, lines[0].len() * 2 + 4).unwrap();
            got.extend(r.texts);
            offset = r.offset;
        }
        assert_eq!(got, ["m0", "m1", "m2", "m3", "m4", "m5"]);
    }

    #[test]
    fn garbage_lines_are_skipped_and_a_shrunk_file_restarts() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("log.jsonl");
        append(&p, &format!("garbage\n{}\n", claude_line("ok")));
        let r = read_new(&p, Agent::Claude, 0, WINDOW).unwrap();
        assert_eq!(r.texts, ["ok"]);
        std::fs::write(&p, format!("{}\n", claude_line("fresh"))).unwrap();
        assert_eq!(
            read_new(&p, Agent::Claude, 10_000, WINDOW).unwrap().texts,
            ["fresh"]
        );
    }

    #[test]
    fn a_missing_file_is_an_error() {
        let t = tempfile::tempdir().unwrap();
        assert!(read_new(&t.path().join("nope"), Agent::Claude, 0, WINDOW).is_err());
    }
}
