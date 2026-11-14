---
title: Local workspaces and backups
section: Desktop app
order: 28
description: Keep a workspace on your computer without an account, back it up, export it, move it to the cloud, and connect agents and a local AI model to it.
---

A local workspace lives only on your computer. You do not need an account for it, and its tasks and pages never reach the TMGR server unless you move them to the cloud.

## What a local workspace is {#what}

Each local workspace is a folder in `~/.tmgr.dev/workspaces/<code>`. The code comes from the workspace name. The folder holds:

- `workspace.db`, the database with tasks, pages, routines and everything else;
- `files`, the attachments;
- `backups`, the backups;
- `exports`, your Markdown exports.

You get a local workspace when you press **Start without an account** on the first start. To add another one, open the workspace switcher at the top of the sidebar and choose **New local workspace**. Local workspaces carry the **Local** badge.

Things to know:

- A local workspace is not synced. It is not on your other devices or in the web app.
- You cannot invite people to it.
- Features that need the TMGR cloud show **Requires an account** there.

When the local workspace is open, the workspace switcher also has **Show in Finder** (**Show in Explorer** on Windows, **Show in file manager** on Linux), **Export to Markdown** and **Move to cloud…**.

## Backups {#backups}

TMGR backs up every local workspace once a day while the app runs. It keeps the newest copy of each of the last 7 days, 4 weeks and 3 months, and deletes older ones. Copies you make yourself are not deleted. The demo is not backed up.

Open [Settings → Local data](/settings?tab=data). Each local workspace has:

- **Data folder**, with a button that shows the folder;
- **Backups folder**, with **Open folder**;
- the time of the last backup, with **Back up now**. Make a backup before big changes.

If the last backup failed, or there was none for 3 days, a bar at the top of the window says so and offers **Back up now**.

## Export to Markdown {#export}

In the workspace switcher, choose **Export to Markdown**. TMGR writes a `README.md` with all tasks grouped by status, and one Markdown file for each task. They go to a new folder inside `exports` in the workspace folder, and TMGR shows it to you. Pages are not part of this export.

## Move to the cloud {#move-to-cloud}

You can copy a local workspace into a new workspace of your TMGR account. You need to be signed in. Choose **Move to cloud…** in the workspace switcher, or press **Move to cloud…** in [Settings → Local data](/settings?tab=data).

The dialog **Move to your cloud account** shows what happens:

- **Moves**: tasks with time and checkpoints, comments, files, pages with history, routines, agent runs, statuses, categories and relations, and the skills from this computer.
- **Stays on this computer**: persona tokens, plugin data, and files that cannot be uploaded. The dialog lists those files and why.
- **Cloud workspace name:** the name of the new workspace.
- **Personas**: for each local persona choose **Create in my account**, **Use my existing persona …** or **Attribute to me**.
- **Keep the local copy** is always on. The move copies, it never deletes.

A running timer is stopped first. Press **Move to cloud**. You can close the progress window: the move goes on, and it resumes if it stops. When it is done, press **Open the cloud workspace**.

Later the local workspace shows that it was copied, and that changes made there are not synced. You can start up to 3 moves a day. The demo cannot be moved.

## Remove the local copy {#remove-local-copy}

After a move, [Settings → Local data](/settings?tab=data) shows **Remove local copy** for that workspace. TMGR checks four things first:

- The move is published and verified in your cloud account.
- Nothing changed in this workspace since the snapshot.
- Every file that could not be uploaded is exported or accepted as lost. Use **Export** or **Accept as lost** next to each file.
- Skills from this device are copied to your cloud library.

You also need to be signed in to the account that made the move. When all checks pass, press **Remove local copy…** and type the workspace name to confirm.

TMGR first makes a fresh archive of the current data. It then deletes the database and the files. The rest of the workspace folder, with the backups and the archive, goes to `~/.tmgr.dev/removed`. The cloud copy stays.

## Use it from the command line {#cli}

The `tmgr` command line works with local workspaces too. Write a local workspace as `local:` and its code, for example `local:personal`. No sign-in and no network are needed.

```sh
tmgr ws list
tmgr ws use local:personal
tmgr task list --ws local:personal
```

`tmgr ws list` shows local workspaces next to the cloud ones. When the desktop app is signed in, the command line acts as that user. See [the command line guide](/docs/cli).

## Connect an agent {#local-agent}

AI agents on this computer, such as Claude Code, Codex or Cursor, can work in a local workspace. They connect to the `tmgr-local` MCP server that comes with TMGR Desktop.

Open the local workspace, go to [Settings → Personas](/settings/personas) (it needs the Personas module, which is on in new local workspaces), find **Local agent access** and press **Connect your agent**.

The **Connect your agent** screen lists the agents it found on this computer:

- For Claude Code and Codex, tick the agent and press **Install**. **Preview** shows the commands first.
- For Cursor, press **Copy config** and paste it into `~/.cursor/mcp.json`.

**Install** works on macOS only. On other systems, **Copy config** is shown instead. Local agent access is not available on Windows yet.

What **Install** does:

- It creates a token for a local persona. The token is valid for 90 days and stays in the macOS Keychain, not in the command.
- It adds the `tmgr-local` server to the agent.
- It turns on local agent access and the Personas and Agent work modules in this workspace.

Each agent connects to one workspace. Installing again replaces the earlier connection. Then restart the agent and ask it to list your TMGR tasks. **Remove** takes TMGR out of the agent and revokes the tokens made on this screen.

You cannot connect an agent to the demo, or while the app runs with `--safe-mode`. To connect an agent as a specific persona, use **Connect an agent** next to that persona in the same settings page.

## Local AI for personas {#local-ai}

A persona can answer with an AI model on your computer, such as Ollama or LM Studio. Set it up in [Settings → Personas](/settings/personas), under **Local LLM**:

1. Press **Detect** to find Ollama or LM Studio on their usual ports. Or enter the **Base URL** yourself.
2. Pick the **Model**.
3. Add an **API key** if your server needs one. The key is kept in the system keychain (the Keychain on macOS) and is used only with the server it was saved for.

A server on your local network is used only after you confirm it with **Allow**. A URL outside this computer and your network shows a warning, because the data will leave the computer.

To use it, open a task in a local workspace and press **Ask persona** next to the comment box. The persona reads the task and posts its answer as a comment.

The same server also powers the **Ask AI** assistant of a local workspace (turn on the AI assistant module). It reads your tasks and pages and asks you to confirm every change. Your data stays on this device unless the server is remote. See [AI assistant](/docs/app/ai-assistant#local-assistant).
