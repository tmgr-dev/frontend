---
title: Queue control
section: CLI guides
order: 64
description: Check, pause, turn on and off the agent queue, manage its rules, and handle single pickups and teams from the terminal.
---

The agent queue starts agents on your machine for tasks that a rule allows and that you approved. The first setup, from the web app to the first pull request, is in the lesson [The agent queue](/docs/cli/the-agent-queue). This guide is about running the queue day to day.

`tmgr queue` is experimental and does not appear in the built-in help. `tmgr queue autopilot` is the same as `tmgr autopilot`; see [Autopilot](/docs/cli/autopilot).

## Check the queue {#status}

```sh
tmgr queue status
```

The output shows whether the queue is on or off and whether it is paused, the state of the background loop, the rules, and the open pickups. A pickup is one task the queue took: each row has its id, rule, task, status and the start of its bundle hash. Requested teams and running teams follow at the end.

To keep watching, add `--tail`. It prints the summary, then each new event of the background loop until you press Ctrl+C:

```sh
tmgr queue status --tail
```

If `queue.toml` was changed outside `tmgr queue`, the status warns you, and the queue does not run until you confirm the file with `tmgr queue enable`.

## Pause and resume {#pause}

Pause stops picking up work at once. Agents that are already running keep working.

```sh
tmgr queue pause
tmgr queue resume
```

Run `resume` yourself in a normal terminal: inside an agent session it is refused.

## Turn the queue on and off {#enable}

```sh
tmgr queue enable
```

`enable` shows the risk and your rules, and asks you to type `I AGREE`. It needs a terminal. Then the queue runs by itself in the background through the runner.

```sh
tmgr queue disable
```

`disable` turns the queue off: nothing new starts, and the background loop ends. You can always turn the queue off, even when its rules file was changed outside `tmgr queue`.

## Rules {#rules}

A rule decides what may start: the workspace, project, persona, agent profile, folder, and how many agents at once. How to add the first rule is shown in [the lesson](/docs/cli/the-agent-queue#t-rule). Adding a rule asks you to type `I AGREE`.

To list the rules, or remove one by name:

```sh
tmgr queue rules list
tmgr queue rules rm my-site
```

Other options of `tmgr queue rules add`:

- `--status-type`: Take tasks with status type `backlog` (the default) or `active`
- `--hours`: Working hours in local time, for example `09-19`
- `--max-concurrent`: Agents of the rule at once, 1 to 3 (default 1)
- `--trusted-author`: `human` (the default), `agent` or `unrecorded`; repeat it for several
- `--auto`: Start without asking for each task; only for tasks by human authors
- `--mode`: Permission mode of a cloud rule: `ask`, `accept-edits`, `plan` or `bypass`
- `--model`: Model to pass to the agent profile

## Approve a pickup {#approve}

In a local workspace, a rule without `--auto` waits for you before each task. `tmgr queue status` lists such a pickup as `pending_approval`. To approve it:

```sh
tmgr queue approve <pickup>
```

It shows exactly what would start, page by page, and asks you to type the first 12 characters of the bundle hash. It needs a terminal. A task of a cloud workspace is approved by the owner's signature in the web app, not here.

## Release a stuck pickup {#release}

If the queue cannot tell whether a start worked, the pickup shows `unknown_start` and keeps its slot. First check with `tmgr agents` that no agent of that task is running. Then free the slot:

```sh
tmgr queue release <pickup>
```

Type the pickup id, not the task key, to confirm. The pickup ends as failed.

## Ask for a team {#team}

To have one task worked by a team, a lead and 1 to 5 workers:

```sh
tmgr queue add TM-12 --team 2 --mode heavy
```

`--team` is required. `--mode` is `eco` (Sonnet lead, Haiku workers, the default) or `heavy` (Opus lead, Sonnet workers). A rule with the `claude-code` profile must match the task. The command shows a summary and asks you to type `TEAM`. The team starts when the rule lets the task start, with all of the rule's checks.

## Run one pass now {#run-once}

To have the queue look for work right now, instead of waiting for the background loop:

```sh
tmgr queue run-once
```

It prints what happened to each task, or `Nothing to do.`, `Queue is off.` or `Queue is paused.`.
