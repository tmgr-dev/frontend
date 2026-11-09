---
title: Autopilot
section: CLI guides
order: 62
description: Let `tmgr autopilot` work through a workspace backlog with agent teams, with limits you set and controls to pause or stop.
---

Autopilot takes the tasks of a workspace and runs agent teams on them through the agent queue. Each task ends as a pull request; nothing is merged for you. If you have not set up the queue yet, start with [the agent queue](/docs/cli/the-agent-queue). The short version is in the [command line guide](/docs/cli#autopilot).

## What it does {#what}

Autopilot reads your backlog, shows the list of tasks it can run, and asks you to confirm once. After that it runs in the background on your machine:

- It signs the approval of the chosen tasks with this computer's key, so you do not confirm them one by one.
- It starts agents up to the number of teams you set, never more than the queue rule allows.
- It turns the queue on while it works. Stopping autopilot turns the queue off.

Autopilot needs a queue rule for a cloud workspace. Without one it stops with `autopilot needs a rule for a cloud workspace`; add the rule as shown in the queue lesson.

## Start it {#start}

Start with the limits you want:

```sh
tmgr autopilot start --max-teams 2 --max-hours 4
```

`start` uses the only cloud rule you have, or the one you pick with `--rule NAME`. To name a workspace instead, write `tmgr autopilot <workspace>` with the same flags. Autopilot asks the setup questions, prints a summary of tasks, signing, limits and the rule, and waits. To confirm, type the text it shows, which is the workspace code and the number of tasks. Flags you pass skip the matching questions. To ask them again, add `--setup`.

To read the full text of every task before you sign it, add `--review`. To choose tasks, use `--tasks`, `--only`, `--skip` and `--order` with task keys separated by commas.

## Try it first with --dry-run {#dry-run}

A dry run plans and sorts the tasks and shows the result in the status. It starts nothing, and moves or comments nothing.

```sh
tmgr autopilot start --max-teams 2 --dry-run
```

Read the plan with the status command below. A dry run cannot start while the queue's background loop is running; turn the queue off first.

## Set limits {#limits}

Limits keep an unattended run inside your budget.

- `--max-teams N`: Teams working at the same time
- `--max-hours HOURS`: How long autopilot runs
- `--max-tasks N`: How many tasks it starts
- `--max-cost USD`: Spending on runs paid by an API key
- `--usage-soft PCT`: No new tasks from this usage percent
- `--usage-hard PCT`: Running agents pause at this percent
- `--no-usage-limit`: Runs without the usage guard

Use `none` instead of a number to remove `--max-hours`, `--max-tasks` or `--max-cost`.

### Cost limit {#max-cost}

`--max-cost` counts only runs paid with an API key. Runs on a Claude or Codex subscription are not counted against it, so the cap does not stop them. If autopilot cannot read your AI usage logs, it cannot check the cap and starts no new task. Allow reading with `tmgr usage consent allow`.

### Plan usage {#usage-guard}

With usage reading allowed, autopilot watches how much of your plan limit is used. It stops starting tasks at the soft percent and pauses running agents at the hard one. `--resume-after-reset yes` makes it continue once the usage window resets.

If autopilot finds no real usage number, it cannot apply these limits. It then stops starting tasks after 8 hours, or after the number of hours you give with `--fallback-hours`.

## Check progress {#status}

```sh
tmgr autopilot status --watch
```

The status shows task counts, running teams and agents, limits, and the cost line. Without `--watch` it prints once. If nothing is running it says `Autopilot is not running.`. When autopilot finishes, the status lists the pull requests.

To read the last log lines:

```sh
tmgr autopilot log --tail 100
```

## Pause, resume and stop {#control}

```sh
tmgr autopilot pause
```

Pause stops new tasks from starting. Tasks that are already running finish.

```sh
tmgr autopilot resume
```

Resume continues. Run it yourself in a normal terminal: inside an agent session it is refused with `refused inside an agent session`.

```sh
tmgr autopilot stop
```

Stop ends autopilot and turns the queue off. If the queue was running in the background before autopilot, it goes back to that.

If autopilot is not running, these commands answer `autopilot is not running`.

## If it does not start {#problems}

- `no queue rule for workspace ...`: add a rule with `tmgr queue rules add`, or pass `--rule NAME` when the workspace has several.
- A 403 error that mentions `agent-approvals:write`: your login lacks the scope to sign approvals. Sign in again with the command from the hint. See [Troubleshooting](/docs/cli/troubleshooting).
