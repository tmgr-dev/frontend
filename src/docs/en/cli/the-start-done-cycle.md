---
title: 3. The start → done cycle
section: CLI tutorials
order: 32
description: Start a task, see its timer, link commits to it and finish it with one command.
video:
---

In this lesson you work through a task from start to finish: one command when you start, one when you finish, and the branch, the status and the timer take care of themselves.

## Video transcript {#transcript}

Lesson three: the work cycle. One command when you start, one when you finish, and the branch, the status and the timer take care of themselves.

### Start {#t-start}

Pick up a task with `start` and its number. `tmgr` moves the task to In progress, creates a branch from its number and title, and starts the timer.

```sh
tmgr start TM-123
```

If your working tree has uncommitted changes, it stops, and the `--stash` flag tucks them away.

### Status {#t-status}

`status` shows the active task and how long the timer has been running.

```sh
tmgr status
```

You can put the same line into your shell prompt, into tmux, or into the Claude Code status line: `tmgr init` with the name of your shell, for example `tmgr init bash`, prints a ready-made snippet.

### Commit {#t-commit}

The commit hook is installed once per repository.

```sh
tmgr git install-hooks
```

From then on, it adds a Refs line with the active task's number to every commit, so your commits are linked to the task.

```sh
echo '<p lang="es">Hola</p>' >> index.html
git commit -qam "Add Spanish greeting"
git log -1
```

### Done {#t-done}

Finished? Run `done`. The timer stops, the time is logged to the task, and the task moves to review. If the branch has a pull request, its link is added to the task as a comment.

```sh
tmgr done
```

### Today {#t-today}

And `today` shows where your time went today, with your own time and your agents' time listed separately.

```sh
tmgr today
```

### Next {#t-next}

That's the whole cycle. In the next lesson: [4. AI agents under supervision](/docs/cli/ai-agents-under-supervision). Running them in a sandbox, watching them, and rolling back their changes.
