---
title: 5. The Decisions inbox
section: CLI tutorials
order: 34
description: Let an agent ask you a question, answer it in one place and look up past answers.
video: https://tmgr.dev/l/video/en/tmgr-cli-05-decisions.mp4
---

In this lesson an agent asks a question, you answer it in the Decisions inbox, and the answer goes back to the agent.

## Video transcript {#transcript}

Lesson five: the Decisions inbox. An agent asks a question, you answer in one place, and the answer goes back to the agent.

### Ask {#t-ask}

`ask` sends a question card: a title, context, options, and the recommended option. The `--wait` flag blocks until you answer, so the agent waits for a decision instead of guessing. The `--key` flag names the question, so sending it again updates the same card.

```sh
tmgr ask --title "Which database for the blog?" \
  --context "Full-text search, about 10k posts" \
  --option SQLite --option Postgres --recommended 2 \
  --key blog-db --wait
```

### Decisions window {#t-inbox}

Cards collect in the Decisions window of the desktop app, on macOS and Linux. You see the context, the options, the agent's recommendation, and any changes to review. Answer with a number, with Enter, or in your own words.

### Answer {#t-answer}

The answer lands in the terminal right away, and the agent carries on.

### Past answers {#t-answers}

`decisions answers` finds answers you've already given, so an agent can check whether you've decided this before. `--key` matches cards whose key starts with the given prefix.

```sh
tmgr decisions answers --key blog-
```

And for the agents themselves, there's an MCP server: `tmgr mcp` gives them tools to ask you a question, look up past answers, and send you a notification.

```sh
tmgr mcp --help
```

### Next {#t-next}

That's lesson five. The next lesson is [6. Your task in the shell prompt](/docs/cli/your-task-in-the-shell-prompt).
