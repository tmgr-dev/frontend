---
title: Agent sessions
section: CLI guides
order: 61
description: Find, rename, talk to, stop and resume the sessions that tmgr run starts, search, summarize or export their transcripts, and see your attach time and open agent questions.
---

`tmgr run` starts an agent and gives you a session. This guide is about what you do with the session afterwards. To start your first one, see [AI agents under supervision](/docs/cli/ai-agents-under-supervision). To look after the process behind the sessions, see [Runner maintenance](/docs/cli/runner). The overview is in the [command line guide](/docs/cli#runner).

All of these commands are experimental. Anywhere a command takes a session, you can give its id or its name.

## See your sessions {#list}

To list sessions with their state:

```sh
tmgr agents
```

Each row shows the name, the state, the id, the command and the folder. A state is `starting`, `working`, `waiting` (the agent waits for you), `blocked` (it needs a decision), `exited` or `failed`. A line below the table counts them.

To print what a session shows right now as plain text, add `--lines` for more of the history:

```sh
tmgr peek <session> --lines 100
```

To open the live full-screen view, with no session it picks the newest running one:

```sh
tmgr attach
```

## Rename a session {#rename}

The id stays the same. The new name must be unique among your sessions.

```sh
tmgr rename <session> review-footer
```

## Type into a session {#send}

To type a line into a session and press Enter:

```sh
tmgr send <session> "Run the tests and fix what fails"
```

Add `--no-enter` to type without pressing Enter. To wait for the answer and print what appeared on the screen, add `--wait idle` (until the agent waits or is blocked again) or `--wait exit` (until the session ends). `--timeout` limits the wait and defaults to 10 minutes.

```sh
tmgr send <session> "Summarize your changes" --wait idle --timeout 5m
```

With `--wait idle`, the exit code is 8 if the agent ended up blocked. This is handy in scripts. The state is guessed from the screen, so an agent can look idle when it is not.

## Stop and remove a session {#stop}

`halt` stops one session: it interrupts the agent, and if that does not work, ends it. Stopping the runner does not do this, because sessions keep running when the runner stops.

```sh
tmgr halt <session>
```

If the agent does not stop, add `--kill` to end it at once. For a session that has already exited, `--kill` also stops background processes it left behind.

To delete a session that has exited:

```sh
tmgr rm <session>
```

If the session worked in its own copy of the project (next section), `rm` offers to delete the copy as well, but only when the copy is clean: no uncommitted or untracked changes and no git operation in progress. If the copy has new commits, its branch stays in the repository. When the copy is not clean, `rm` keeps it and says why.

## The session's own copy {#copy}

A sandboxed session that you start with `--agent` in a git repository works in its own copy of the project by default: a checkout in `<repo>-worktrees/<session name>` on a new branch of the same name. Two agents then never write the same files.

The session starts in the folder itself instead when you add `--no-worktree`, `--trust` or `--no-sandbox`. To turn the copy off for every session, set `attach.worktree` to `false` with `tmgr config set`.

## Start an exited session again {#resume}

`resume` starts an exited session in place and continues the agent's conversation. It shows what it is going to do and asks you to confirm.

```sh
tmgr resume <session>
```

After a plain `resume`, the output shows the command to open the session; the live view does not open. It also accepts the id of an agent conversation that `tmgr` did not start. To continue as a new session and leave the original conversation untouched, add `--fork`. `--mode` sets the permission mode. With `--fork`, `--mode`, `--no-sandbox` or a conversation id, the live view opens right away; add `--no-attach` to skip it.

If a reboot or logout stopped your sessions, this lists them and offers to start them again, one confirmation each:

```sh
tmgr agents restore
```

## Find past conversations {#find}

To list past sessions, both from `tmgr` and from Claude Code and Codex on this machine:

```sh
tmgr session list --agent claude --limit 20
```

`--agent` takes `claude` or `codex`. The default limit is 50. To find sessions by what was said in them, search instead. The first search builds a local index of your transcripts. Nothing leaves your computer.

```sh
tmgr session list --search "footer layout"
```

## Summarize or export a transcript {#transcripts}

Summary and export work only for sessions that `tmgr` started, the ones `tmgr agents` lists. For a conversation that only `session list` shows, they answer `no session matches`. They also work for Claude Code sessions only: for Codex sessions they answer that they are not available. Listing and searching cover both.

Reading transcripts needs your consent. If it is off, the command tells you how to turn it on.

To print a short summary of what a session did:

```sh
tmgr session summary <session>
```

The prose comes from a model. With `--plain` you get only the files, commits and pull requests, and no model is asked.

To save a session as a Markdown file, or as a page in your current workspace:

```sh
tmgr session export <session> --md notes.md
tmgr session export <session> --page
```

Before anything is written, `tmgr` scans the transcript for secrets and lets you review each finding. `--remove-findings` removes them without asking. The file is private to you and is not overwritten without a confirmation. With `--page`, the text goes to the TMGR server, so you type the workspace code to confirm. Without `--md` or `--page`, nothing is written.

## Time you spend in attach {#time}

`tmgr attach` counts the time you spend with each session. While the attach window is in front of you and a session is focused, its time counts: as typing while you type, as reading the rest of the time. After a pause longer than the idle cap, time stops counting. The cap is 2 minutes by default.

To see the totals:

```sh
tmgr time --today
```

Each row shows the session, its folder, its task, the total, typing and reading, and the send state. `--since` takes a date (`YYYY-MM-DD`) or an age such as `12h` or `7d`. `--by` groups the rows by `session` (the default), `folder` or `task`.

Time of a session that belongs to a task, such as one started with `tmgr run <task>`, is sent to that task and added to its tracked time. Time already covered by the task timer is not counted twice. Sending needs the **Session time → task** module in [Settings → Modules](/settings/modules). It is off by default, and only the workspace owner can turn it on. Without it, the rows show `skipped`. Time marked `skipped`, because the module or sending was off, is not sent later. Time of a local workspace stays on your computer and shows `local`.

Sending is on by default. To check it, or to turn it off or on:

```sh
tmgr time sync
tmgr time sync off
```

To change the idle cap, give it in seconds, from 30 to 3600:

```sh
tmgr time idle 300
```

## Questions agents asked you {#questions}

`tmgr attach` reads the transcripts of Claude Code and Codex sessions and keeps a list of the questions the agents asked you. To list the open ones:

```sh
tmgr questions
```

Each row shows the question id, the session, when it was asked, and the question. `--session` limits the list to one session. `--status` takes `open` (the default), `answered`, `ignored` or `all`.

Reading transcripts needs your consent. If it is off, the list ends with a line that tells you how to turn it on.

If you answered in the session yourself, mark the question answered. To drop a question from the open list, ignore it. The id needs at least 4 characters:

```sh
tmgr questions answer <id>
tmgr questions ignore <id>
```

To tell real questions from rhetorical ones, `tmgr` sends the text of agent messages that look like a question to your own `claude` command, with the Haiku model. This is on by default. Turn it off to find questions by built-in rules only, with nothing sent anywhere:

```sh
tmgr questions haiku off
```

Without `on` or `off`, the command shows the current setting.
