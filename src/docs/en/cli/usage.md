---
title: AI usage in detail
section: CLI guides
order: 65
description: Every tmgr usage command and option, from periods and limits to accounts, prices, uploads, consent and reset.
---

`tmgr usage` counts the tokens and cost of Claude Code and Codex from their logs on your computer. The lesson [7. What AI costs you](/docs/cli/what-ai-costs-you) shows the basics. This guide lists every command and option.

Costs are API list prices for the same tokens, not your subscription bill. The usage data lives on your computer and keeps the last 90 days.

## Reading the logs {#consent}

`tmgr` reads the logs only with your consent. See the current choice, allow, or deny:

```sh
tmgr usage consent
tmgr usage consent allow
tmgr usage consent deny
```

- Until you choose, the commands you run by hand read the logs. The TMGR desktop app and background work (autopilot, automatic uploads after an agent run) wait for `allow`.
- After `deny`, nothing reads the logs. The commands show the data stored before. The agent runner stops reading session logs too.
- What is kept: counts, tokens, cost, model names, times, session ids, project folders and branches, names of the tools, skills and commands used, types of edited files, and pull requests. The text of prompts and replies is never stored.
- Nothing leaves your computer unless you upload it with [`tmgr usage sync`](#sync).

The choice is shared with the TMGR desktop app.

## The summary {#summary}

With no options, the command shows three periods: today, the last 7 days and the last 30 days.

```sh
tmgr usage
```

Pick one period instead:

- `--today` or `--yesterday`.
- `--week`: the last 7 days, today included.
- `--month`: the last 30 days, today included.
- `--since` and `--until`: days as `YYYY-MM-DD`, in your time zone. `--until` is included and needs `--since`.

Group the rows with `--by`: `tool`, `model`, `account`, `project`, `branch`, `day` or `task`. Narrow them with `--task`, `--tool claude` or `--tool codex`, and `--account` with an account label.

```sh
tmgr usage --since 2026-10-01 --until 2026-10-31 --by project
tmgr usage --week --tool codex --by model
```

Every usage command first reads new log lines. Add `--no-scan` to skip that and show the stored data right away.

## Rate limits {#limits}

```sh
tmgr usage limits
```

Each row is a limit window: the tool, the account, the window (session, weekly, or weekly for one model), the percent used, the time until reset, where the number came from, and how old it is.

Codex limits come from its session logs. Claude Code limits need the status line hook: see [Claude Code status line](#statusline). Without it, the command warns that Claude limits are missing.

## One task {#task}

```sh
tmgr usage task TM-123
```

It shows the time agents worked on the task, the cost, the tokens (input, cache reads and writes, output, reasoning), how much of your limits it used, and the tools, skills, plugins, MCP servers and hooks the agents used.

- `--tree` shows every agent, not only the top level.
- `--days` sets how many days back to look: 30 by default, 90 at most.

Usage is linked to a task through agent runs, for example ones started with `tmgr run` or `tmgr agent start`. Usage outside a run shows up without a task in `tmgr usage --by task`.

## Scan the logs {#scan}

```sh
tmgr usage scan
```

It reads new log lines into the usage data and prints how many files and events it read. The other commands do this on their own. The first scan of large logs can take a minute.

## Accounts {#accounts}

An account is one config folder of Claude Code or Codex. By default `tmgr` reads `~/.claude` and `~/.codex` (labels `claude` and `codex`), and the folders in `CLAUDE_CONFIG_DIR` and `CODEX_HOME` when they are set.

To read another folder, for example a second Claude login, add it under a label:

```sh
tmgr usage accounts
tmgr usage accounts add --tool claude --root ~/.claude-work --label work
tmgr usage accounts remove work
```

The first command lists the accounts and where each one comes from. The folder must exist. `remove` works only for accounts you added, and the usage already stored stays.

## Prices {#pricing}

```sh
tmgr usage pricing
tmgr usage pricing show --model opus
```

The first command says which price list is in use and how many models it has. With `--model`, it shows the prices per million tokens of the models whose name contains that text.

`tmgr` ships with a price list. To download the current public LiteLLM list and re-price everything stored:

```sh
tmgr usage pricing update
```

`--offline` goes back to the list that ships with `tmgr`. When a model has no known price, its events are left out of the cost and you see a warning.

## Upload to TMGR {#sync}

Uploads let the members of a task's workspace see what agents cost on the task. One summary per task is sent: cost, tokens, time, the tree of agents, and the names of tools and skills. Prompts, replies, file paths and command arguments are never sent. Local workspaces are never uploaded.

Nothing is uploaded until both of these are on:

1. In a terminal, run `tmgr usage sync on`. It shows who will see the data and a real sample, then asks you to type `yes`.
2. In [Settings → Agent work](/settings?tab=agent-work), turn on **Agent usage sync**.

Then upload by hand:

```sh
tmgr usage sync --dry-run
tmgr usage sync
tmgr usage sync --task TM-123
```

`--dry-run` prints what would be sent and sends nothing; it works before you turn uploads on. `--days` sets how many days back to read (30 by default, 90 at most). `--force`, together with `--task`, sends even when the usage on this computer is smaller than what was uploaded before.

To upload automatically after each agent run:

```sh
tmgr usage sync auto on
tmgr usage sync auto status
tmgr usage sync auto off
```

`auto on` shows what is sent and asks for its own confirmation. Without a terminal, confirm with `--yes-auto`. `tmgr usage sync on --yes-auto` turns on both uploads and automatic uploads at once.

Check and stop:

```sh
tmgr usage sync status
tmgr usage sync off
tmgr usage sync forget
```

`status` says whether uploads are on and how the last run went. `off` stops uploads; what you uploaded before stays on the server. `forget` deletes your uploads from the server, all of them or one task with `--task`. It asks you to type a confirmation, or takes `--yes`. Turning off **Agent usage sync** in Settings also deletes everything you uploaded.

## Reset {#reset}

```sh
tmgr usage reset
```

It deletes all stored usage data and the saved questions of the agent runner. The logs of Claude Code and Codex stay. It asks first. Your consent choice stays as it was.

If you upload usage, run `tmgr usage sync forget` before `reset`. Otherwise a later, smaller upload could replace the fuller one on the server.

## Claude Code status line {#statusline}

`tmgr` gets the Claude Code rate limits through the Claude Code status line. It adds its own status line command, so `tmgr usage limits` can show Claude limits.

```sh
tmgr usage statusline install
```

This prints the change to the Claude Code `settings.json` and changes nothing. To write it:

```sh
tmgr usage statusline install --install
```

It shows the change, asks you to confirm in the terminal, and keeps a backup of the file. It never accepts `--yes`. If you already have a status line command, it keeps working: `tmgr` runs it and prints its output.

`--account` picks another Claude account by its label (the default is `claude`). To put back the previous status line command:

```sh
tmgr usage statusline uninstall
```
