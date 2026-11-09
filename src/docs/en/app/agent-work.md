---
title: Agent work in the web app
section: Web app
order: 32
description: See what AI agents did on a task, confirm tasks for agent runs, decide which personas may work in a workspace and manage your personas.
---

AI agents such as Claude Code or Codex report their work on tasks to TMGR. This page shows where you see that work in the app and how you control what agents may do. To connect an agent, see the [AI agents guide](/docs/agents). The terminal side is in the lessons [Agent work reports](/docs/cli/agent-work-reports) and [The agent queue](/docs/cli/the-agent-queue).

## The Agent work panel {#panel}

The panel needs the Agent work module, which is off by default. The owner turns it on in [Settings → Modules](/settings/modules), in the AI agents group. When the module is off, agent runs are hidden and agents do not report.

The **Agent work** panel appears in a task as soon as an agent has reported a run on it. Its top line shows **Agents** and **You**: the time agents worked and the time people tracked on the task with the timer.

Each run shows:

- the agent and its model;
- the status: **running**, **succeeded**, **failed**, **cancelled** or **abandoned**;
- how long it took, and when it started;
- the summary the agent wrote;
- the branch, a **Pull request** link and the commits (click the commit count to see them);
- test results: how many tests passed and failed.

A run that sends no update for 45 minutes is marked **abandoned**. If the agent reports again on the same run, it continues.

### Run details in the desktop app {#details}

In the TMGR desktop app, the panel is a single line with your time, agent time, the AI cost and the pull request. Press **Details** to open **Agent work — details**. The window adds what the desktop app can read from the AI tool logs on your computer, plus usage that was synced to TMGR:

- **Totals**: your time, agent time and the AI cost. If an hourly rate is set, **Your time at** that rate shows what your time cost.
- **Tokens**: input, cache and output tokens, and the prices used.
- **Agents**: the agents that worked on the task, with their own cost.
- **Tools** and **Context**: the tools, MCP servers and hooks the agents used.
- **Code**: commits, the pull request, tests, releases, CI, deploys and checks. Commits marked "Found in git, not reported by the agent" were found in git only.

AI cost is API-equivalent: what the same tokens would cost at list prices, not your subscription bill. To see the cost of other people's runs, they have to sync their usage. See [Agent usage sync](#usage-sync).

## Confirm tasks for agent runs {#confirm}

Your agents can start work on tasks from the queue on your own computer. To keep a task that someone else wrote from steering your agent, TMGR lets your agents run only:

- tasks you wrote yourself;
- tasks you confirmed for agent runs.

First turn on agent runs for yourself. Open [Settings → Modules](/settings/modules), find the **Agent runs** section and turn on **Let my agents run my tasks**. This setting is yours and applies to this workspace only. Your computer also has to allow the workspace, with `tmgr runner allow <code>`. The lesson [The agent queue](/docs/cli/the-agent-queue) shows the whole setup.

### One task {#confirm-one}

With the setting on, a task shows an **Agent runs** line with its state, for example **Approved for agent runs** or **Needs confirmation**.

1. Press **Confirm for agent runs**.
2. The dialog shows exactly what an agent will read: **Title**, **Description**, **Category**, the **Repositories** of the category and, when they apply, the persona instructions and the **Workspace rules**. Invisible or direction-changing characters are shown highlighted, so hidden instructions cannot slip through.
3. Read it, and press **Confirm** only if you wrote or checked all of it.

The confirmation covers this exact text. When someone else or an agent edits the task or adds a comment, the confirmation is cancelled, and the line says why. If the persona instructions or the workspace rules change, the task asks for a new confirmation too. You can confirm only in the web or desktop app. An agent cannot confirm for you.

### Many tasks at once {#confirm-many}

1. On the List, press **Tasks selection mode** and select the tasks.
2. Press **Process (n)**.
3. In **Batch Actions**, choose **Confirm for agent runs** under **Agent runs** and press **Execute**.
4. Check the text of every task in the dialog and press **Confirm n**.

Each task then shows **Confirmed** or the reason it failed, for example "Changed since it was shown".

## Personas in a workspace {#policy}

A persona is an identity your agents work under. In the [Settings → Modules](/settings/modules) page of a workspace, the **Personas** section lists the personas with access to it. It needs the Personas module. For each persona you see its owner, its permissions and the permissions that are in effect here.

The workspace owner sets the **Policy** for the whole workspace:

- **Allowed** (the default): personas work with the permissions their owners gave them.
- **Read only**: personas keep only their read permissions.
- **Forbidden**: no persona can work in this workspace, and nobody can give one access.

The owner can also stop one persona with **Block**. A blocked persona has no access here until the owner presses **Unblock**.

## Manage your personas {#personas}

Your personas are in [Settings → Personas](/settings/personas). The page needs the Personas module. The lesson [Personas and skills](/docs/web/personas-and-skills) shows how to create one. Here is what else you can do with a persona.

- **Notify me about this persona's activity**: choose **All**, **Only when it asks for me** or **Never**.
- **Workspaces**: choose where the persona may work and with which permissions. Here you also see if a workspace owner blocked it.
- **Skills**: the persona's own skills, up to 20. **Convert to shared** moves a skill to your [skills library](/docs/app/skills-library) and attaches it to the persona. Agents keep the same instructions.
- **Agent connections**: the tokens agents use to work as this persona. **Connect an agent** issues a new one for one workspace. It expires after 30, 90, 180 or 365 days; 90 is the default. **Revoke** stops one token, **Revoke all** stops every token of this persona.
- **Archive** hides the persona. It also revokes all its tokens and removes it from the tasks it was assigned to. **Show archived** lists archived personas, and **Restore** brings one back. You can have up to 50 active personas. Tokens are not restored, so connect the agents again.

## Agent usage sync {#usage-sync}

The desktop app and the tmgr CLI read what agents cost from the logs on your computer. Agent usage sync uploads per-task totals to TMGR, so other devices can show them too.

Open [Settings → Agent work](/settings?tab=agent-work) and turn on **Agent usage sync**.

- What is uploaded: tokens, AI cost, the names of the agent, model and tools, and counts.
- What is never uploaded: prompts, replies, file paths and command arguments.
- Uploads start only from the CLI, after you turn them on there. Run `tmgr usage sync` by hand, or turn on automatic uploads after each agent run. See [Usage](/docs/cli/usage).

**Your uploads** lists what your account has uploaded, newest first. **Delete all** deletes the uploads and keeps sync on. Turning sync off deletes everything already uploaded.
