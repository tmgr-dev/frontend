---
title: Command line (tmgr)
section: Guides
order: 20
description: Work with tasks from the terminal and run AI agents under supervision.
---

`tmgr` is a small command-line program for TMGR. It lets you work with your tasks without opening the browser, and run AI coding agents such as Claude Code and Codex where you can watch them, stop them and undo what they did.

New to `tmgr`? Follow the tutorials: [1. Install and sign in](/docs/cli/install-and-sign-in) and [2. Tasks from the terminal](/docs/cli/tasks-from-the-terminal).

## Install {#install}

On macOS or Linux:

```sh
curl -fsSL https://tmgr.dev/install.sh | sh
```

No sudo needed. On Windows, use WSL2 and run the same command inside it.

## Work on tasks without leaving the terminal {#tasks}

One command picks up a task: it creates the git branch, moves the task to In progress and starts the timer. When you are done, one more command stops the timer, moves the task to review and adds the pull request link. No tab switching.

```sh
tmgr start TM-123
```

Also:

```sh
tmgr status
tmgr board
tmgr today
tmgr switch TM-124
tmgr done
tmgr add "Fix the login redirect"
tmgr task show TM-123
tmgr git install-hooks
tmgr project map
tmgr init zsh
```

`git install-hooks` adds the task key to your commit messages. `project map` tells `tmgr` which folder holds the code of a project. `init zsh` shows the active task in your prompt.

## Run AI agents under supervision {#runner}

Agents run inside a sandbox. Start one with `--worktree` and it works in its own copy of your project, on its own branch, so two agents never write the same files. Start it with `--checkpoints` and its work is saved after every turn, so you can see what changed and roll it back. You can look at what it is doing at any moment.

```sh
tmgr run --agent claude-code --worktree --checkpoints
```

Also:

```sh
tmgr attach
tmgr peek
tmgr send
tmgr halt
tmgr resume
tmgr checkpoints diff
tmgr checkpoints restore
tmgr session summary
tmgr session export --page
tmgr runner trust
tmgr runner allow
```

`session export --page` saves a session as a page in your workspace. Checkpoints need a git folder. `runner trust` and `runner allow` are experimental: they set which folders and workspaces agents may start in.

## Answer every agent question in one inbox {#decisions}

When an agent needs a decision, it sends you a card with the question, the options, its recommendation and the changes to look at. You answer once in the TMGR desktop app, and the agent gets your answer and carries on. Answers you already gave can be found again, so agents do not ask the same thing twice.

```sh
tmgr ask --title "Which database?" --option SQLite --option Postgres --recommended 1 --wait
```

Also:

```sh
tmgr decisions answers
tmgr hook install decisions
tmgr mcp
```

The Decisions inbox works in the desktop app on macOS and Linux. Agents can also ask through the MCP tools `ask_owner` and `find_answers`.

### Instant delivery to OpenCode

By default an answer reaches an OpenCode session when the agent next checks. To have the desktop app push it into the running session, opt in:

1. Start OpenCode with a fixed local port, for example `opencode serve --hostname 127.0.0.1 --port 4096`, or `opencode --port 4096`.
2. In `opencode.json`, give the `tmgr` MCP entry this `environment`: `TMGR_OPENCODE_URL` set to `http://127.0.0.1:4096` and `OPENCODE_SERVER_PASSWORD` set to `{env:OPENCODE_SERVER_PASSWORD}`. Never write the password itself into the file.

The address must be a loopback IP literal (`127.0.0.1` or `[::1]`) over plain `http`. `tmgr` also uses `OPENCODE_PID`, which OpenCode passes to the MCP process by default, and `lsof` to confirm that this OpenCode process owns the port. The session is found as the single busy OpenCode session in the working directory of the agent; with none or several, nothing is pushed. If the check fails, or the variable is not set, nothing changes. The server password is kept in the desktop app's private local store, like the Claude socket token.

## Get a message on your phone when an agent needs you {#phone}

Walk away from the screen. A push tells you when an agent is waiting for input or has finished a long job. For something urgent, an alarm rings in the TMGR iOS app and then calls your phone if you do not answer.

```sh
tmgr alarm "Production is down"
```

Also:

```sh
tmgr notify "Build finished"
tmgr hook install claude
tmgr hook install codex
```

Needs the TMGR iOS app and a signed-in `tmgr` (`tmgr login`). The same tools ship as the [@tmgr/notify](https://www.npmjs.com/package/@tmgr/notify) package for any MCP client; it uses a token from Settings → Agent alerts.

## Let agents work through your backlog {#autopilot}

Point autopilot at a workspace and it starts agent teams on your tasks, with limits you set: how many teams, how long, how much to spend. It watches your plan usage, stops before you hit the limit and can continue after the limit resets. Allow reading your AI usage logs (`tmgr usage consent allow`) for the cost limit.

```sh
tmgr autopilot start --max-teams 2 --max-hours 4 --dry-run
```

Also:

```sh
tmgr autopilot status
tmgr agents login
tmgr agents sync
```

`--dry-run` only plans: no task starts. Approvals are still signed. `--max-cost` counts only runs paid by API key. `agents login` and `agents sync` are experimental: they manage the sign-in of an agent profile used by runner sessions.

## See what your AI tools cost and how much limit is left {#usage}

Know the cost per day, per task and per account, and how close you are to the limit, without opening a billing page. `tmgr` reads the usage logs of Claude Code and Codex on your computer. The usage database never stores your prompts.

```sh
tmgr usage --week --by task
```

Also:

```sh
tmgr usage limits
tmgr usage task TM-123 --tree
tmgr usage statusline install
tmgr time
```

The desktop app asks before reading the logs. In the terminal, usage commands you run by hand read them until you run `tmgr usage consent deny`.

## Pair it with the desktop app {#desktop}

Use `tmgr` next to the TMGR desktop app. Decisions show up in the app, local workspaces that never touch the cloud work from the command line too, and shared skills can be attached to your personas.

```sh
tmgr --ws local:my-notes status
```

Also:

```sh
tmgr skills list
tmgr skills attach
```

## Give your agents the same tools {#agents}

`tmgr` speaks MCP, so Claude Code, Codex, Cursor and other agents can ask you questions, find your earlier answers and send you notifications on their own. To let them work with your tasks, connect the TMGR task server too.

```sh
tmgr mcp
```

Also:

```sh
tmgr hook install claude
tmgr hook install codex
```

Step-by-step setup for each agent is in the [AI agents guide](/docs/agents).

## Built to be safe {#safety}

- Your sign-in token is kept in the system keychain (or in a private file when there is none), and `tmgr` never prints it.
- `tmgr self-update` installs only releases with a valid signature, and checks them before replacing itself.
- Agents run in a sandbox by default. Leaving it needs a typed confirmation.
- The TMGR server never sends commands to your agents. You control them from your own computer.
- `tmgr setup terminal` makes copying from `tmgr attach` work over SSH. It shows what it will change and asks first.

## More {#more}

- [AI agents guide](/docs/agents): connect Claude Code, Codex and Cursor.
- Tutorials: [1. Install and sign in](/docs/cli/install-and-sign-in) and [2. Tasks from the terminal](/docs/cli/tasks-from-the-terminal).
- [@tmgr/notify](https://www.npmjs.com/package/@tmgr/notify): phone pushes and alarms for any MCP client.
- [Source code](https://github.com/tmgr-dev/frontend/tree/master/crates): `tmgr` is open source.
