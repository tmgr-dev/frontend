---
title: 6. Your task in the shell prompt
section: CLI tutorials
order: 35
description: Show the current task and timer in your shell prompt, tmux and the Claude Code status line.
video:
---

In this lesson you put the current task and its timer into your shell prompt, tmux and the Claude Code status line.

## Video transcript {#transcript}

Lesson six: `tmgr` in your shell prompt, in tmux, and in the Claude Code status line, so your current task and timer are always in view.

### Snippet {#t-snippet}

`init` prints a snippet for your shell: bash, zsh or fish.

```sh
tmgr init bash
```

With the `--install` flag, it adds the snippet to your config itself: it shows the changes, asks for confirmation, and keeps a backup.

```sh
tmgr init bash --install
source ~/.bashrc
```

### Prompt {#t-prompt}

Now, with a task running, the prompt shows its number and how long the timer has been going.

```sh
tmgr start TM-123 --no-branch
tmgr status
```

The segment is read from a local state file, with no server calls, so your shell stays fast.

### tmux & Claude Code {#t-more}

The same works for tmux, Starship, and the Claude Code status line: `init tmux` and `init claude` print ready-made config snippets.

```sh
tmgr init tmux
tmgr init claude
```

### Pause {#t-stop}

`stop` pauses the timer, and the prompt shows that the task is paused.

```sh
tmgr stop
```

### Next {#t-next}

In the next lesson: what your AI agents cost, and how much of your limit is left. See [7. What AI costs you](/docs/cli/what-ai-costs-you).
