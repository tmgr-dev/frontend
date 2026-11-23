use crate::lines::MAX_LINE;
use crate::locate::open_regular;
use crate::Agent;
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

pub const WINDOW: usize = 1 << 20;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Replies {
    pub turns: Vec<Turn>,
    pub offset: u64,
    pub usage: Usage,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Turn {
    pub user: bool,
    pub text: String,
    pub ts: Option<String>,
    pub end: u64,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Usage {
    pub model: Option<String>,
    pub context_tokens: Option<u64>,
    pub context_window: Option<u64>,
}

pub fn len(path: &Path) -> Result<u64, String> {
    open_regular(path)?
        .metadata()
        .map(|m| m.len())
        .map_err(|e| e.to_string())
}

pub fn backfill_start(path: &Path, end: u64, max: u64) -> Result<u64, String> {
    if end <= max {
        return Ok(0);
    }
    let from = end - max;
    let mut f = open_regular(path)?;
    f.seek(SeekFrom::Start(from)).map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    f.take(max)
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    Ok(match buf.iter().position(|b| *b == b'\n') {
        Some(i) => from + i as u64 + 1,
        None => end,
    })
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
                    turns: Vec::new(),
                    offset,
                    usage: Usage::default(),
                });
            }
            if want <= MAX_LINE {
                want = MAX_LINE + 1;
                continue;
            }
            return Ok(Replies {
                turns: Vec::new(),
                offset: offset + buf.len() as u64,
                usage: Usage::default(),
            });
        };
        let mut turns = Vec::new();
        let mut usage = Usage::default();
        let mut pos = offset;
        for line in buf[..end].split(|b| *b == b'\n') {
            pos += line.len() as u64 + 1;
            if line.len() > MAX_LINE {
                continue;
            }
            let Ok(v) = serde_json::from_slice::<Value>(line) else {
                continue;
            };
            meter(agent, &v, &mut usage);
            turns.extend(turn(agent, &v).map(|mut t| {
                t.end = pos;
                t
            }));
        }
        return Ok(Replies {
            turns,
            offset: offset + end as u64 + 1,
            usage,
        });
    }
}

pub fn meter(agent: Agent, v: &Value, usage: &mut Usage) {
    match agent {
        Agent::Claude => {
            let m = &v["message"];
            let model = m["model"].as_str().unwrap_or("");
            if v["type"].as_str() != Some("assistant")
                || v["isSidechain"].as_bool() == Some(true)
                || model.is_empty()
                || model.starts_with('<')
            {
                return;
            }
            usage.model = Some(model.to_string());
            let tokens: u64 = [
                "input_tokens",
                "cache_read_input_tokens",
                "cache_creation_input_tokens",
            ]
            .iter()
            .filter_map(|k| m["usage"][k].as_u64())
            .sum();
            if tokens > 0 {
                usage.context_tokens = Some(tokens);
                usage.context_window = None;
            }
        }
        Agent::Codex => {
            let p = &v["payload"];
            match (v["type"].as_str(), p["type"].as_str()) {
                (Some("turn_context"), _) => {
                    if let Some(model) = p["model"].as_str().filter(|m| !m.is_empty()) {
                        usage.model = Some(model.to_string());
                    }
                }
                (Some("event_msg"), Some("token_count")) => {
                    let info = &p["info"];
                    if let Some(tokens) = info["last_token_usage"]["input_tokens"]
                        .as_u64()
                        .filter(|t| *t > 0)
                    {
                        usage.context_tokens = Some(tokens);
                        usage.context_window =
                            info["model_context_window"].as_u64().filter(|w| *w > 0);
                    }
                }
                _ => {}
            }
        }
    }
}

fn injected(text: &str) -> bool {
    text.starts_with('<') || text.starts_with("[Request interrupted")
}

pub fn turn(agent: Agent, v: &Value) -> Option<Turn> {
    let (user, text) = match agent {
        Agent::Claude => {
            if v["isSidechain"].as_bool() == Some(true) {
                return None;
            }
            match v["type"].as_str()? {
                "assistant" => {
                    if v["message"]["model"].as_str() == Some("<synthetic>") {
                        return None;
                    }
                    let text = v["message"]["content"]
                        .as_array()?
                        .iter()
                        .filter(|b| b["type"].as_str() == Some("text"))
                        .filter_map(|b| b["text"].as_str())
                        .collect::<String>();
                    (false, text)
                }
                "user" => {
                    if ["isMeta", "isCompactSummary", "isVisibleInTranscriptOnly"]
                        .iter()
                        .any(|k| v[*k].as_bool() == Some(true))
                    {
                        return None;
                    }
                    let content = &v["message"]["content"];
                    let text = match content {
                        Value::String(s) => s.clone(),
                        Value::Array(blocks) => {
                            if blocks
                                .iter()
                                .any(|b| b["type"].as_str() == Some("tool_result"))
                            {
                                return None;
                            }
                            blocks
                                .iter()
                                .filter(|b| b["type"].as_str() == Some("text"))
                                .filter_map(|b| b["text"].as_str())
                                .collect::<Vec<_>>()
                                .join("\n")
                        }
                        _ => return None,
                    };
                    (true, text)
                }
                _ => return None,
            }
        }
        Agent::Codex => {
            let p = &v["payload"];
            if v["type"].as_str() != Some("response_item") || p["type"].as_str() != Some("message")
            {
                return None;
            }
            let user = match p["role"].as_str()? {
                "assistant" => false,
                "user" => true,
                _ => return None,
            };
            let text = p["content"]
                .as_array()?
                .iter()
                .filter_map(|c| c["text"].as_str())
                .collect::<String>();
            (user, text)
        }
    };
    let text = text.trim();
    if text.is_empty() || (user && (injected(text) || text.starts_with("# AGENTS.md"))) {
        return None;
    }
    Some(Turn {
        user,
        text: text.to_string(),
        ts: v["timestamp"].as_str().map(str::to_string),
        end: 0,
    })
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

    fn texts(r: &Replies) -> Vec<String> {
        r.turns.iter().map(|t| t.text.clone()).collect()
    }

    fn reply(agent: Agent, v: &Value) -> Option<String> {
        turn(agent, v).filter(|t| !t.user).map(|t| t.text)
    }

    fn user_text(agent: Agent, v: &Value) -> Option<String> {
        turn(agent, v).filter(|t| t.user).map(|t| t.text)
    }

    #[test]
    fn claude_user_prompts_count_but_injected_lines_do_not() {
        let plain = json!({"type": "user", "timestamp": "2026-01-02T03:04:05Z", "message": {"role": "user", "content": " fix the bug \n"}});
        let blocks = json!({"type": "user", "message": {"content": [{"type": "text", "text": "a"}, {"type": "text", "text": "b"}]}});
        assert_eq!(
            user_text(Agent::Claude, &plain).as_deref(),
            Some("fix the bug")
        );
        assert_eq!(
            turn(Agent::Claude, &plain).unwrap().ts.as_deref(),
            Some("2026-01-02T03:04:05Z")
        );
        assert_eq!(user_text(Agent::Claude, &blocks).as_deref(), Some("a\nb"));
        let skipped = [
            json!({"type": "user", "isMeta": true, "message": {"content": "meta"}}),
            json!({"type": "user", "isCompactSummary": true, "message": {"content": "This session is being continued"}}),
            json!({"type": "user", "isSidechain": true, "message": {"content": "side"}}),
            json!({"type": "user", "message": {"content": [{"type": "tool_result", "tool_use_id": "x", "content": "out"}, {"type": "text", "text": "t"}]}}),
            json!({"type": "user", "message": {"content": "<task-notification>done</task-notification>"}}),
            json!({"type": "user", "message": {"content": "<local-command-stdout>x</local-command-stdout>"}}),
            json!({"type": "user", "message": {"content": "[Request interrupted by user]"}}),
        ];
        for v in &skipped {
            assert_eq!(turn(Agent::Claude, v), None, "{v}");
        }
    }

    #[test]
    fn codex_user_messages_count_but_context_and_events_do_not() {
        let user = |text: &str| json!({"type": "response_item", "payload": {"type": "message", "role": "user", "content": [{"type": "input_text", "text": text}]}});
        assert_eq!(
            user_text(Agent::Codex, &user("run tests")).as_deref(),
            Some("run tests")
        );
        assert_eq!(
            turn(
                Agent::Codex,
                &user("<environment_context>x</environment_context>")
            ),
            None
        );
        assert_eq!(
            turn(Agent::Codex, &user("# AGENTS.md instructions for /x")),
            None
        );
        let event = json!({"type": "event_msg", "payload": {"type": "user_message", "message": "run tests"}});
        assert_eq!(turn(Agent::Codex, &event), None);
    }

    #[test]
    fn backfill_start_lands_on_a_line_boundary_inside_the_window() {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("log.jsonl");
        let lines: Vec<String> = (0..10).map(|i| claude_line(&format!("m{i}"))).collect();
        append(&p, &(lines.join("\n") + "\n"));
        let end = len(&p).unwrap();
        assert_eq!(backfill_start(&p, end, end).unwrap(), 0);
        let max = (lines[0].len() as u64 + 1) * 3 + 5;
        let start = backfill_start(&p, end, max).unwrap();
        assert!(end - start <= max);
        assert_eq!(
            texts(&read_new(&p, Agent::Claude, start, WINDOW).unwrap()),
            ["m7", "m8", "m9"]
        );
        let long = t.path().join("long.jsonl");
        append(&long, &(claude_line(&"x".repeat(500)) + "\n"));
        let n = len(&long).unwrap();
        assert_eq!(backfill_start(&long, n, 100).unwrap(), n);
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
    fn claude_usage_comes_from_the_last_main_assistant_line() {
        let main = |model: &str, input: u64| {
            json!({"type": "assistant", "message": {"model": model, "content": [],
            "usage": {"input_tokens": input, "cache_read_input_tokens": 1000, "cache_creation_input_tokens": 10}}})
        };
        let side = json!({"type": "assistant", "isSidechain": true, "message": {"model": "side", "usage": {"input_tokens": 9}}});
        let synthetic = json!({"type": "assistant", "message": {"model": "<synthetic>", "usage": {"input_tokens": 9}}});
        let mut u = Usage::default();
        for v in [main("claude-a", 5), main("claude-b", 90), side, synthetic] {
            meter(Agent::Claude, &v, &mut u);
        }
        assert_eq!(
            u,
            Usage {
                model: Some("claude-b".into()),
                context_tokens: Some(1100),
                context_window: None,
            }
        );
    }

    #[test]
    fn codex_usage_comes_from_turn_context_and_token_count() {
        let mut u = Usage::default();
        let lines = [
            json!({"type": "turn_context", "payload": {"model": "gpt-x"}}),
            json!({"type": "event_msg", "payload": {"type": "token_count", "info": null}}),
            json!({"type": "event_msg", "payload": {"type": "token_count", "info": {
                "total_token_usage": {"input_tokens": 999999},
                "last_token_usage": {"input_tokens": 33450, "cached_input_tokens": 33280},
                "model_context_window": 258400}}}),
        ];
        for v in &lines {
            meter(Agent::Codex, v, &mut u);
        }
        assert_eq!(
            u,
            Usage {
                model: Some("gpt-x".into()),
                context_tokens: Some(33450),
                context_window: Some(258400),
            }
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
        assert_eq!(texts(&a), ["one", "two"]);
        assert_eq!(a.offset, len(&p).unwrap());

        let partial = claude_line("three");
        append(&p, &partial[..10]);
        let b = read_new(&p, Agent::Claude, a.offset, WINDOW).unwrap();
        assert!(b.turns.is_empty());
        assert_eq!(b.offset, a.offset);

        append(&p, &format!("{}\n", &partial[10..]));
        let c = read_new(&p, Agent::Claude, b.offset, WINDOW).unwrap();
        assert_eq!(texts(&c), ["three"]);
        assert_eq!(
            read_new(&p, Agent::Claude, c.offset, WINDOW)
                .unwrap()
                .turns
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
            got.extend(texts(&r));
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
        assert_eq!(texts(&r), ["ok"]);
        std::fs::write(&p, format!("{}\n", claude_line("fresh"))).unwrap();
        assert_eq!(
            texts(&read_new(&p, Agent::Claude, 10_000, WINDOW).unwrap()),
            ["fresh"]
        );
    }

    #[test]
    fn a_missing_file_is_an_error() {
        let t = tempfile::tempdir().unwrap();
        assert!(read_new(&t.path().join("nope"), Agent::Claude, 0, WINDOW).is_err());
    }
}
