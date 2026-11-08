---
title: 4. AI agents under supervision
section: CLI tutorials
order: 33
description: Run an AI agent in a sandbox, watch its sessions and roll back its changes.
video: https://tmgr.dev/l/video/en/tmgr-cli-04-agents.mp4
---

In this lesson you run an agent in a sandbox, watch what it does and roll back its changes.

## Video transcript {#transcript}

Lesson four: AI agents under supervision. We run an agent in a sandbox, watch what it does, and roll back its changes. To keep things predictable, the agent in the video is a simple script standing in for a real one; Claude Code and Codex work exactly the same way.

### Run {#t-run}

`run` starts an agent in a sandbox, with its own home folder and only the environment it needs. The `--worktree` flag gives it a separate copy of the project on its own branch, so two agents never write to the same files, and the `--checkpoints` flag saves the folder after every turn.

```sh
tmgr run --agent claude-code --worktree --checkpoints --name site-footer
```

The live session view opens right away: type a request, and watch the agent edit files. Ctrl+] then D detaches, and the agent keeps working.

### Watch {#t-watch}

`agents` lists every session and its state: working, waiting for input, or blocked.

```sh
tmgr agents
```

`peek` prints a session's current screen as plain text, and `attach` brings the live view back.

```sh
tmgr peek site-footer
```

### Roll back {#t-restore}

Every agent turn is a restore point. `checkpoints list` shows them.

```sh
tmgr checkpoints list site-footer
```

`checkpoints diff` shows what changed in a step.

```sh
tmgr checkpoints diff site-footer 3
```

`checkpoints restore` puts the folder back to that state. It asks for confirmation. The current state is saved first, so you can undo the rollback.

```sh
tmgr checkpoints restore site-footer 2
```

And `halt` stops the session.

```sh
tmgr halt site-footer
```

### Next {#t-next}

Agents can also ask you questions directly. That's the next lesson: [5. The Decisions inbox](/docs/cli/the-decisions-inbox).
