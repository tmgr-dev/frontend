---
title: 7. What AI costs you
section: CLI tutorials
order: 36
description: Allow tmgr to read agent logs, then see tokens, cost and rate limits by day, branch or task.
video: https://tmgr.dev/l/video/en/tmgr-cli-07-usage.mp4
---

This lesson shows how much your AI agents cost: tokens, cost and rate limits, counted from the local logs of Claude Code and Codex.

## Video transcript {#transcript}

Lesson seven: what your AI agents cost. `tmgr` reads the local logs of Claude Code and Codex, and counts tokens, cost and limits. The logs in this demo are generated.

### Consent {#t-consent}

First, permission: logs are read only with your consent.

```sh
tmgr usage consent allow
```

`tmgr` keeps token counts, models, times and branches, but never the text of prompts or replies. Nothing leaves your device unless you turn on sync yourself.

### This week {#t-week}

Usage for the week: cost by model, input and output tokens, and cache reads and writes.

```sh
tmgr usage --week
```

This is the API list-price equivalent, not your subscription bill.

### Breakdown {#t-by}

The `--by` flag groups usage by day, project, branch or task.

```sh
tmgr usage --week --by day
```

Grouped by branch, you can see right away which feature cost the most.

```sh
tmgr usage --week --by branch
```

Task attribution appears when agents work through `run` or `agent start`.

### Limits {#t-limits}

This shows your rate-limit windows, the percentage used, and the time until reset.

```sh
tmgr usage limits
```

For Claude Code, the data comes from the status line hook. `tmgr usage statusline install` shows the change to the Claude Code settings; add `--install` to apply it:

```sh
tmgr usage statusline install --install
```

### Next {#t-next}

In the next lesson: personas, and a shared skills library for your agents. See [8. Personas and skills](/docs/cli/personas-and-skills).
