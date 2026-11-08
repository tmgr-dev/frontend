---
title: 10. The agent queue
section: CLI tutorials
order: 39
description: Let approved tasks start agents on your machine by themselves and receive the result as a pull request.
video:
---

This lesson sets up the agent queue: tasks assigned to a persona and approved by you start on your machine, and the result arrives as a pull request.

## Video transcript {#transcript}

Lesson ten: the agent queue. Tasks assigned to a persona and approved by you start on your own machine by themselves, and the result arrives as a pull request. The agent in this demo is a simple script standing in for a real agent.

### On the web {#t-web}

It all starts on the web, shown here in the web UI. In the workspace modules, turn on Agent runs, assign the task to a persona, and use Confirm for agent runs. The confirmation is a signature over the exact text of the task: if anyone changes it, the signature is dropped.

### Trust this machine {#t-allow}

On your machine, allow the workspace:

```sh
tmgr runner allow <workspace>
```

`tmgr` shows the keys that sign your approvals and pins them, so a task signed by anyone else never starts. Confirm by typing the workspace code.

### Repository {#t-bind}

`tmgr runner bind` links the project to your local clone of the repository from the project settings.

```sh
tmgr runner bind <workspace>:<project> ~/acme-site
```

Confirm with the project code.

### Rule {#t-rule}

A queue rule sets what may start: the workspace, project, persona, agent profile, folder, and how many agents at once. A cloud rule only starts tasks that you signed.

```sh
tmgr queue rules add my-site --workspace <workspace> --persona Builder \
  --profile claude-code --repo ~/acme-site --project <project> --auto --max-concurrent 2
```

Adding the rule asks you to type `I AGREE`.

### Queue {#t-queue}

This turns the queue on in the background; it also asks for `I AGREE`.

```sh
tmgr queue enable
```

The queue finds the approved task, makes a separate copy of the project for it, and starts the agent in a sandbox. In the video, this runs one pass right away.

```sh
tmgr queue run-once
```

This shows the session: the agent has already finished.

```sh
tmgr agents
```

### Result {#t-result}

This shows what the agent did. It takes a session id or name.

```sh
tmgr peek <session>
```

When the agent finishes, the queue pushes the branch and opens a pull request that waits for your review. This turns the queue off.

```sh
tmgr queue disable
```

And the `tmgr autopilot` command builds whole agent teams on top of this queue, led by Claude Code.

### Next {#t-next}

That's the last lesson in the series. Full documentation is in the [command line guide](/docs/cli).
