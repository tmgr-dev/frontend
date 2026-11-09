---
title: Agent sessions
section: CLI guides
order: 61
description: Find, rename, talk to, stop and resume the sessions that tmgr run starts, and search, summarize or export their transcripts.
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

If the session had its own checkout from `--worktree`, you are offered to remove it as well.

## Start an exited session again {#resume}

`resume` starts an exited session in place and continues the agent's conversation. It shows what it is going to do and asks you to confirm.

```sh
tmgr resume <session>
```

It also accepts the id of an agent conversation that `tmgr` did not start. To continue as a new session and leave the original conversation untouched, add `--fork`. `--mode` sets the permission mode, and `--no-attach` starts the session without opening the live view.

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

Summary and export work for Claude Code sessions only. For Codex sessions they answer that they are not available. Listing and searching cover both.

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
