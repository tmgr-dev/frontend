---
title: 9. Agent work reports
section: CLI tutorials
order: 38
description: Install agent hooks, report a run with agent start, update and finish, and read the log.
video: https://tmgr.dev/l/video/en/tmgr-cli-09-agent-reports.mp4
---

This lesson shows how agents report their work on tasks and how you read those reports.

## Video transcript {#transcript}

Lesson nine: agent work reports. You see which agent worked on a task, for how long, and with which commits and tests.
### Hooks {#t-hooks}

This command adds the TMGR hooks to your Claude Code settings: notifications when an agent is waiting for you, and linking commits to its run.

```sh
tmgr hook install claude
```

The command shows the changes and asks you to confirm; answer `y`. For Codex, use `tmgr hook install codex`.

### Work report {#t-report}

The report itself is three commands. `tmgr agent start` opens a run on a task, with the agent, the model and the branch.

```sh
tmgr agent start TM-123 --agent claude-code --model claude-sonnet-5-5
```

`tmgr agent update` adds commits, a pull request and test results, and works as a heartbeat: a run with no updates for forty-five minutes counts as abandoned.

```sh
tmgr agent update --commit "$(git log -1 --format='%h %s')" --tests-passed 12 --tests-failed 0
```

`tmgr agent finish` closes the run with a status and a summary.

```sh
tmgr agent finish --status succeeded --summary "Redirect loop fixed, all tests pass"
```

Agents do the same through the task MCP server.

### Log {#t-log}

`tmgr log` lists agent runs for the workspace or for one task: who worked, for how long, the branch, commits, tests and the summary.

```sh
tmgr log --task TM-123
```

With the `--follow` flag, it updates live.

### Next {#t-next}

That's lesson nine. Setup for each agent is in the [agents guide](/docs/agents). The next lesson is [10. The agent queue](/docs/cli/the-agent-queue).
