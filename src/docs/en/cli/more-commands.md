---
title: More commands
section: CLI guides
order: 66
description: The smaller tmgr commands, from settings, task references and project folders to routines, agent sessions, local task checks, updates and hooks.
---

This guide covers the `tmgr` commands that the [command line guide](/docs/cli) only names. Every command also takes `--json` for output that scripts can read.

## Settings {#config}

`tmgr` keeps its settings in `config.toml` in `~/.config/tmgr`, or in `$XDG_CONFIG_HOME/tmgr` when that is set.

```sh
tmgr config list
tmgr config get branch_template
tmgr config set review_status "Code review"
tmgr config path
```

`list` prints every setting, `get` prints one, `set` changes one, and `path` prints where the file is. An unknown key or a bad value is refused, and the error lists the keys you can set.

| Key | What it sets |
| --- | --- |
| `server` | The TMGR API server. The default is `https://api.tmgr.dev`. |
| `web_url` | The address of the web app, used for task links. |
| `default_workspace` | The workspace used when you do not pass `--ws`. `tmgr ws use` sets it. It can be `local:<code>`. |
| `branch_template` | The name of the branch `tmgr start` creates. The default is `{key_lower}-{slug}`. Placeholders: `{key}`, `{key_lower}`, `{code_lower}`, `{seq}` and `{slug}` (the task title). |
| `review_status` | The status `tmgr done` moves a task to. The default is `Review`. |
| `upload_status` | `true` lets `tmgr agent status set` send session states. Off by default. See [Agent sessions](#agent-status). |
| `hook_git_commits` | `false` stops the Claude Code hook from adding the commits made during an agent run to that run. On by default. |
| `local_user_id` | The user id `tmgr` acts as in local workspaces of the desktop app. `-1` is the desktop's local account. |

The `attach.*` keys set up `tmgr attach`, which is experimental:

| Key | Values |
| --- | --- |
| `attach.sidebar_width` | Sidebar width, 16 to 60 columns. |
| `attach.theme` | `auto`, `dark`, `light` or `high-contrast`. |
| `attach.signal.permission`, `attach.signal.question`, `attach.signal.input` | `true` or `false`: a bell and a highlight when a session waits for a permission, a question or input. |
| `attach.signal.quiet` | Hours without bells, such as `22:00-08:00`, or `off`. |
| `attach.prompt.send` | `true` sends a saved prompt instead of only pasting it. |
| `attach.summary` | `ask`, `always` or `never`: the summary of a Claude session written into its task when the session ends. |
| `attach.push.long` | Minutes or `off`: a phone push when a turn that ran this long ends. |
| `attach.push.error` | `true` or `false`: a push when a session exits with an error. |
| `attach.push.blocked` | Minutes or `off`: a push when a permission prompt or question has waited this long. |
| `attach.push.quiet` | Hours without pushes, such as `22:00-08:00`, or `off`. |
| `attach.push.debounce` | Minutes: at most one push of a kind per session in that time. |
| `attach.copy.clean` | `false` copies text as it is shown, without cleaning. |
| `attach.worktree` | `false` starts agent sessions in place instead of in their own copy of the repository. |
| `attach.checkpoints` | `true` saves the session folder after every turn, for new sessions. |
| `attach.checkpoints_keep` | How many checkpoints a session keeps, 3 to 1000. |
| `attach.permission_cards` | `true` lets you answer an agent's permission prompts from the sidebar. |

## How to name a task {#refs}

```sh
tmgr refs
```

It prints the ways to name a task in any command:

- `TM-512`: the project (category) code and number, as in the task title. It needs `--ws` or a default workspace.
- `#8974` or `8974`: the global task id.

Anything else is refused before any request is sent.

## Project folders {#project}

Tell `tmgr` which folder holds the code of a project. Then `tmgr start TM-123` creates the branch in that repository wherever you run it, and agent runs for the project's tasks know where the code is.

```sh
tmgr project map TM ~/src/tmgr
tmgr project list
tmgr project unmap TM
```

`map` warns when the folder is not a git repository. `list` marks folders that no longer exist as missing.

## Routines {#routines}

```sh
tmgr routine list
tmgr routine list --date 2026-11-10
tmgr routine list --json
tmgr routine done 42
```

`list` shows the routines due today, or on the day you give with `--date`, with a mark for the ones done. The ids are only in the `--json` output, in the `id` field. `done` marks a routine done for today. With `--date` it marks another day, but only when that day already has an instance of the routine. Routines in the app: [Routines](/docs/app/routines).

## Notes, today and the run log {#day}

- `tmgr note` writes a page in your editor. See [Pages → From the terminal](/docs/app/pages#cli).
- `tmgr today` shows the tracked time, the agent time and the routines of a day. `--yesterday` or `--date` picks another day; `--mine` counts only the agent runs you started. See [The start → done cycle](/docs/cli/the-start-done-cycle#t-today).
- `tmgr log` lists agent runs. Narrow it with `--task`, `--agent` and `--since` (for example `2h` or `1d`), set `--limit`, or keep it updating with `--follow`. See [Agent work reports](/docs/cli/agent-work-reports#t-log).

```sh
tmgr today --yesterday --mine
tmgr log --agent codex --since 1d
```

## Agent sessions {#agent-status}

Agent sessions can share their state with your workspace: which task they work on, and whether they are working, waiting or blocked. This is off until you turn it on:

```sh
tmgr config set upload_status true
```

Each update then sends the task, the state, the agent profile, the times and this computer's host label. Terminal content is never sent.

```sh
tmgr agent status set my-session-01 --state working --task TM-123
tmgr agent status list
tmgr agent status clear my-session-01
```

The session id is yours to choose: 8 to 64 letters, digits, `_` and `-`. The state is `working`, `waiting`, `blocked`, `idle`, `done` or `exited`. `--profile` names the agent (the default is `claude-code`). `list` shows the sessions of the workspace and marks old ones as stale. `clear` ends a session. If agent work is off in the workspace, nothing is recorded.

## Confirm a local task for agents {#task-confirm}

In a local workspace of the desktop app, the [agent queue](/docs/cli/queue) by default starts only tasks whose text a person wrote. When an agent or a plugin has changed a task, read it and confirm it:

```sh
tmgr --ws local:my-notes task confirm TM-12
```

It shows everything an agent will read: the title, the description, the checkpoints and the comments. Type the first 12 characters of the snapshot it prints to confirm. It works only in a terminal, never inside an agent session, and only for local workspaces. If the task changes in the meantime, nothing is confirmed.

## Answers no agent picked up {#pending-delivery}

```sh
tmgr decisions pending-delivery
```

It lists the answers you gave in the Decisions inbox that no agent has picked up yet, oldest first: when you answered, the card id, the delivery state, the task and the title. It needs the TMGR desktop app. See [The Decisions inbox](/docs/cli/the-decisions-inbox).

## Version and updates {#version}

```sh
tmgr version
tmgr self-update
```

`version` prints the version, the commit and the build target. Updates and their errors are explained in [Troubleshooting → Updating tmgr](/docs/cli/troubleshooting#update).

`tmgr setup terminal` lets programs in your terminal write to the clipboard, so copying in `tmgr attach` works over SSH. See [Troubleshooting](/docs/cli/troubleshooting#clipboard).

## Git and agent hooks {#hooks}

`tmgr git install-hooks` adds the commit hook to the current repository: it adds a Refs line with the task of the current branch to every commit. See [The start → done cycle](/docs/cli/the-start-done-cycle#t-commit).

To add it to every repository on this computer:

```sh
tmgr git install-hooks --global
```

It sets one hooks folder for all repositories in your global git config (`core.hooksPath`) and asks first. The other hooks in a repository's own hooks folder then stop running; its commit-msg hook still runs. A commit-msg hook that is already in the shared folder is kept and runs first.

To remove the hooks that `tmgr hook install` added to Claude Code, Codex or the Decisions inbox:

```sh
tmgr hook uninstall claude
tmgr hook uninstall codex --print
```

It removes only the `tmgr` entries, shows the change, asks first and keeps a backup. `--print` only shows the change. `--project` works on the `.claude/settings.json` of the current repository instead of your user settings.
