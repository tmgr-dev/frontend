---
title: Modules reference
section: Web app
order: 34
description: Every module in Settings → Modules, what it adds, what it needs, who can turn it on and how presets work.
---

A module is a part of TMGR that a workspace can turn on or off, such as the Board, Pages or the AI assistant. Modules are set per workspace, in [Settings → Modules](/settings/modules). The page header names the workspace you are changing. For a short video tour, see the lesson [Workspace modules](/docs/web/workspace-modules).

Module names come from the server and stay in English in every language of the app.

## Who can change modules {#who}

- The workspace owner turns modules on and off with the switch on each row. This changes the workspace for everyone in it.
- Every member, the owner included, can press **Hide for me** on a module. It disappears for you only, and the row shows **Hidden for you**. **Show for me** brings it back.
- A member who is not the owner sees "Only the workspace owner can turn modules on or off. You can hide a module for yourself." An off module shows "Ask the owner to turn this on".

Turning a module off does not delete anything. "Turning a module off hides it for everyone in this workspace. Your data stays. Turn it back on and everything is where you left it."

When a page belongs to an off module, it says "This module is off." The owner gets a link to turn it on; a member is asked to ask the owner.

Each row lists the apps the module works in: web, mobile and desktop. If a module needs another one, the row says so, for example "needs: Pages".

## Presets {#presets}

A preset turns on a set of modules in one go. Press **Apply a preset** (only the owner can) and pick one:

| Preset | What it turns on |
| --- | --- |
| Personal | Dashboard, Checkpoints, Attachments & files, Pomodoro, Reports & export, Routines, Push notifications, Email notifications. No Workflow: tasks get a Done checkbox. |
| Team | Dashboard, Workflow, Checkpoints, Attachments & files, Relations, Routines, Reports & export, GitHub, Telegram bot, Import, Push notifications, Email notifications. |
| Developer with AI agents | Dashboard, Workflow, Checkpoints, Attachments & files, AI assistant, Agent work, Personas, Cursor agents, GitHub, Push notifications, Email notifications. |
| Everything | Every module. |
| Minimal | No modules, only the core: List, Timer and Comments. |

A preset also turns off the workspace modules that are not in it. Before you confirm, the dialog shows what **Will turn on:** and what **Will turn off:**. Press **Confirm** to apply. Modules under **This account** are not changed by a preset.

When the owner opens a new workspace for the first time, TMGR asks "What do you want TMGR for?". Pick a preset, or **Let me pick** to choose modules one by one. **Not now** skips the question.

In a local workspace of the desktop app, presets turn on only the modules that work without an account, and there is no Team preset.

## Core {#core}

Core sections are always on. Every member can still hide them for themselves under **Hide core sections for me**.

| Module | What it adds |
| --- | --- |
| Projects | Categories with ticket numbers such as TM-12: **Categories** in the sidebar, the category picker and the category filter. See [Categories](/docs/app/categories). |
| Board | The Board view of tasks. Needs Workflow: without it the Board is hidden. |
| Timer | A start and stop timer on any task. |
| Comments | Comments and reactions on tasks. |
| Assignees | Assigning tasks to members. |

## Time & focus {#time-focus}

| Module | What it adds | By default |
| --- | --- | --- |
| Dashboard | The Dashboard page: an overview of today and the week. | On |
| Pomodoro | 25/5 focus cycles on any task. | Off |
| Reports & export | Time reports and CSV. | Off |
| Session time → task | Adds your active time in terminal sessions attached with the tmgr CLI to the task they are bound to. | Off |

## Planning {#planning}

| Module | What it adds | By default |
| --- | --- | --- |
| Workflow | Custom statuses, the status picker and board columns. | On |
| Checkpoints | A checklist inside a task. | On |
| Relations | Links between tasks, such as blocks and relates to. | Off |
| Routines | Daily and recurring routines, and Routines in the sidebar. | Off |
| Attachments & files | Attachments on tasks and the Files page. | On |

Turning Workflow off asks you to confirm first: "Tasks get a Done checkbox instead of statuses; the Board is hidden. Statuses and their tasks stay and come back when you turn it on."

## Knowledge {#knowledge}

| Module | What it adds | By default | Needs |
| --- | --- | --- | --- |
| Pages | A Markdown wiki linked to tasks. Workspace memory of the AI assistant is kept here too. | Off | |
| Graph | A map of tasks, pages and agents: the Map in the sidebar and a Graph section on tasks and pages. | Off | Pages |

See [Pages](/docs/app/pages).

## AI agents {#ai}

| Module | What it adds | By default |
| --- | --- | --- |
| AI assistant | The Ask AI chat, AI replies to comments, Rewrite with AI and voice input. | Off |
| Agent work | The Agent work panel on tasks: what agents did and how long it took. When off, agents skip reporting. | Off |
| Personas | AI teammates with their own tokens and skills, and Settings → Personas. | Off |
| Cursor agents | Launching Cursor cloud agents from a task. | Off |

See [AI assistant and memory](/docs/app/ai-assistant), [Agent work in the web app](/docs/app/agent-work) and [Integrations](/docs/app/integrations#cursor).

## Integrations {#integrations}

| Module | What it adds | By default |
| --- | --- | --- |
| GitHub | The GitHub App: link a repository to a category and see commits, branches and pull requests on tasks. | Off |
| Telegram bot | Create routines, tasks and page notes from Telegram, and get notifications there. | Off |
| Import | Importing from other tools. In the web app it adds **Import** on the Routines page, for a calendar file. | Off |

See [Integrations](/docs/app/integrations).

## Notifications {#notifications}

| Module | What it adds | By default |
| --- | --- | --- |
| Push notifications | Push notifications to your devices. | On |
| Email notifications | Email notifications. | On |

## This account {#account}

These modules belong to you, not to the workspace. "Applies to you in every workspace." Every user turns them on and off for themselves.

| Module | What it adds | By default |
| --- | --- | --- |
| MCP & API tokens | Lets Claude Code, Codex and other agents use TMGR. | On |
| Agent alerts & alarms | Agents can push and ring you. [Settings → Agent alerts](/settings/agent-notifications) needs it. | On |
| More from the maker | Other apps by the TMGR maker in About and What's new. | On |

See [Notifications and agent alerts](/docs/app/notifications#agent-alerts).

## Other settings on this page {#other}

Below the modules, the page has a few settings that depend on them:

- **Personal preferences**: settings for your account only. **Default Landing Page** picks the page that opens first when you sign in: List, Board, Dashboard or Daily Routines. A page is offered only while its module is on; Board also needs Workflow.
- **Personas**: which personas may work in this workspace. Needs Personas. See [Personas in a workspace](/docs/app/agent-work#policy).
- **Agent runs**: **Let my agents run my tasks**. See [Confirm tasks for agent runs](/docs/app/agent-work#confirm).
- **AI assistant**: **Write mode**, set by the owner. See [Write mode](/docs/app/ai-assistant#write-mode).
- **Pages**: **Analyst persona for pages**, the persona that keeps the Insights section on person pages. Needs Pages and Personas, and only the owner can change it.
