---
title: MCP server
section: Guides
order: 15
description: What the TMGR task MCP server can do, which token to use for what, and how agents report their work through it.
---

TMGR runs an MCP server for AI agents such as Claude Code, Codex and Cursor. Through it an agent reads and changes your tasks, pages and routines, and reports the work it does. It acts as you, or as one of your personas.

## Connect {#connect}

The server address is `{{mcpUrl}}`. Setup for Claude Code and for an `mcp.json` file is in [AI agents → Task MCP](/docs/agents#task-mcp).

- It is a remote HTTP server (Streamable HTTP) without sessions: each request gets its answer on the same connection. If a connection breaks, your agent gets an error at once instead of waiting.
- It is not an OAuth connector, so claude.ai custom connectors that require OAuth do not work with it.
- When an agent connects, the server sends it instructions: how to find ids, how to create tasks and how to report work.
- The token section on [Settings → Smart Devices](/settings?tab=device) shows only while the **MCP & API tokens** module is on. It is on by default.

## Tokens {#tokens}

| Token | Where you get it | What it is for |
| --- | --- | --- |
| Device token | [Settings → Smart Devices](/settings?tab=device), **Generate token** | The task MCP server, as you, in all your workspaces. Send it in the `X-Smart-Device-Token` header. You have one: **Generate new token** replaces it, **Revoke token** turns it off. |
| Persona token | [Settings → Personas](/settings/personas), a persona, **Agent connections** → **Connect an agent** | The task MCP server, as one persona in one workspace, with that persona's permissions there. Send it in the `X-Persona-Token` header. Pick **Expires**: 30, 90, 180 or 365 days. **Revoke** ends one connection, **Revoke all** ends them all. Needs the **Personas** module. |
| Notify token | [Settings → Agent alerts](/settings/agent-notifications), **Create token** | Pushes and alarms from the `@tmgr/notify` package. It starts with `tmgrn_` and does not open the task MCP server. Up to 20 at a time, each with **Revoke**. Needs the **Agent alerts & alarms** module. |
| Local access token | The TMGR desktop app, in a local workspace | The desktop app's own MCP server for that local workspace. It starts with `lt_`. See [Local workspaces](#local). |
| CLI sign-in | `tmgr login` | The `tmgr` command line. You approve a device code in the browser. It does not open the task MCP server. Each sign-in is listed in [Settings → Profile](/settings?tab=profile) under **Signed-in devices**, with **Edit scopes** and **Revoke**. See [Install and sign in](/docs/cli/install-and-sign-in). |

Device, persona and notify tokens are shown once, when you create them. Copy them right away. A request that sends both a device token and a persona token is refused.

## Tools {#tools}

The server has 55 tools. This list tells you what each one does.

### Tasks {#tools-tasks}

- `list_tasks`: your tasks in your default workspace, page by page.
- `get_task`: one task by id, with its link.
- `create_task`: creates a task. With a project (category), the task gets the project's number and title prefix.
- `update_task`: changes only the fields the agent sends.
- `delete_task`: deletes a task.
- `search_tasks`: searches the tasks of a workspace by text.
- `list_tasks_by_status`: tasks with one status type (backlog, active, completed, hidden, archived) or one status.

### Statuses {#tools-statuses}

- `list_statuses`: all statuses of a workspace with id, name and type.
- `create_status`: creates a status with a color.
- `update_status`: changes a status.

### Comments {#tools-comments}

- `list_comments`: the comments of a task.
- `add_comment`: adds a comment, or a reply to a comment.

### Your timer {#tools-timer}

These tools track your own time, not the agent's. An agent uses them only when you ask it to track your time.

- `start_task_timer`: starts your timer on a task.
- `stop_task_timer`: stops it and adds the time to the task.

### Routines {#tools-routines}

- `list_routines`: the routines of a workspace.
- `create_routine`: creates a one-off routine.
- `create_recurring_routine`: creates a routine with a schedule.
- `complete_routine`: marks a routine done for a date.
- `skip_instance`: skips one occurrence.
- `expand_routines`: the occurrences of routines in a date range.

### Pages {#tools-pages}

- `workspace_context`: all context pages of the workspace as one document. Agents read it before they start a task.
- `pages_for_task`: the pages that mention a task.
- `pages_search`: searches page titles and text.
- `pages_tree`: all pages of a workspace as a tree, without their text.
- `pages_get`: one page with its text, properties, version and sections.
- `pages_create`: creates a plain, person or meeting page. Only people create context pages.
- `pages_update`: replaces the title, text or properties. It needs the page's latest version, so it never overwrites someone else's edit.
- `pages_append`: adds text at the end of a page or under a heading.
- `pages_set_section`: replaces one section that belongs to the agent. On a context page, agents may write only to the "Agent notes" section.

### Graph {#tools-graph}

- `graph_related`: everything linked to a task or page, up to two steps away: related tasks, pages, assignees, personas and agent runs.
- `graph_path`: the shortest chain of links between two tasks or pages.
- `graph_hubs`: the most linked tasks and pages, and the tasks that block the most open work.
- `graph_orphans`: open tasks and pages that nothing links to.
- `graph_map`: an overview of the whole workspace for reviews and planning.

### Agent work {#tools-agent-work}

- `start_agent_work`: starts a record of an agent working on a task.
- `update_agent_work`: reports progress: branch, commits, pull request, tests.
- `finish_agent_work`: ends the record with a result and a summary.
- `list_agent_work`: the agent runs of a task with the total agent time.

How to use them: [The agent work workflow](#agent-work).

### Personas {#tools-personas}

- `list_my_queue`: with a persona token, the tasks assigned to that persona, in board order.
- `list_assignable_personas`: the personas you can put on tasks in a workspace.
- `assign_persona`: puts a persona on a task. No run starts. The persona's owner is assigned too and gets a notification.
- `unassign_persona`: takes a persona off a task.
- `get_persona`: with a persona token, the persona's instructions and list of skills.
- `get_persona_skill`: one skill of the persona.
- `get_persona_skill_file`: one extra file of a skill shared from your library.

### Files {#tools-files}

- `list_task_files`: the files attached to a task.
- `get_task_image`: an attached image, so the agent can look at it. Large images are scaled down.

### Reports {#tools-insights}

- `time_summary`: tracked time in a workspace for a date range, by task, category or person.
- `task_cost`: what a task cost: tracked time times the hourly rate, agent time, and the AI tokens and cost that members uploaded with [`tmgr usage sync`](/docs/cli/usage#sync).
- `deadline_report`: open tasks that are overdue, due soon, unassigned, without an estimate, or not touched for a while.

### Workspaces and account {#tools-workspaces}

- `list_workspaces`: your workspaces with id and code. With a persona token, only its workspace.
- `list_members`: the members of a workspace.
- `whoami`: who the token acts as. With a persona token, also the persona, its workspace, permissions and skills.

### Other {#tools-other}

- `tmgr_request`: calls the TMGR API as the same user, for things no tool covers, such as links between tasks. It cannot change your password, your sign-in or your tokens.
- `plugin_authoring_guide`: the guide to building a TMGR desktop plugin. The same text is also the resource `tmgr://docs/plugin-authoring` and the prompt `plugin_authoring`.

## When a tool is refused {#refused}

### A module is off {#refused-module}

A tool of a module that is off in the workspace answers with an error instead of running. The error names the module key and the workspace.

| Module | Key | Tools |
| --- | --- | --- |
| **Routines** | `daily_routines` | `list_routines`, `create_routine`, `create_recurring_routine`, `complete_routine`, `skip_instance` |
| **Personas** | `personas` | `list_my_queue`, `list_assignable_personas`, `assign_persona`, `unassign_persona`, `get_persona`, `get_persona_skill`, `get_persona_skill_file` |
| **Attachments & files** | `task.files` | `list_task_files`, `get_task_image` |
| **Reports & export** | `exports` | `time_summary` |

Three modules answer in their own words:

- **Pages** off: "Pages are disabled in this workspace". The page tools do nothing.
- **Graph** off: "Graph is disabled in workspace" and the workspace code.
- **Agent work** off: "Agent work is disabled in workspace" and the workspace code. This is not a failure: the agent skips the reports and goes on with the task.

The workspace owner turns modules on in [Settings → Modules](/settings/modules). See [Modules](/docs/app/modules).

### Persona tokens {#refused-persona}

A persona token can use only the tools that a persona needs, and each of them only with the matching permission. The permissions are set per workspace on the persona's settings page.

| Permission | Tools |
| --- | --- |
| `tasks:read` | `get_task`, `list_my_queue`, `search_tasks`, `list_tasks_by_status`, the graph tools |
| `tasks:write` | `create_task`, `update_task` |
| `statuses:read` | `list_statuses` |
| `comments:read` | `list_comments` |
| `comments:write` | `add_comment` |
| `files:attachments` | `list_task_files`, `get_task_image` |
| `agent_work:read` | `list_agent_work`, `task_cost` |
| `agent_work:write` | `start_agent_work`, `update_agent_work`, `finish_agent_work` |
| `pages:read` | `workspace_context`, `pages_for_task`, `pages_search`, `pages_tree`, `pages_get` |
| `pages:write` | `pages_create`, `pages_update`, `pages_append`, `pages_set_section` |
| `memory:read` | `memory_recall` |
| `memory:write` | `memory_remember`, `memory_forget` |

`whoami`, `list_workspaces`, `get_persona`, `get_persona_skill`, `get_persona_skill_file`, `plugin_authoring_guide` and `tmgr_request` need no permission. `tmgr_request` checks each API call on its own.

A persona token is also refused when:

- the tool is not one of those above, for example `list_tasks`, `delete_task`, the timer, the routine tools or `assign_persona`;
- the workspace lets the persona only read, and the tool writes;
- the task, file, page or workspace id is from another workspace;
- `update_task` tries to move a task to another workspace;
- `update_agent_work` or `finish_agent_work` names a run that this persona did not start;
- the persona sends too many requests at once.

The error starts with "persona guard:" and says why.

## The agent work workflow {#agent-work}

Agent work is how you see what an agent did on a task and how long it took. The server tells every agent to follow these steps.

1. Pick up the task. Call `workspace_context` and follow the workspace rules it returns. Move the task to an active status with `update_task`. Call `start_agent_work` with the task, the agent's name (claude-code, codex, cursor), and the model, branch and session id when known. Keep the run id it returns.
2. Report while working. Call `update_agent_work` after every commit and at least every 15 minutes, with the branch, all commits so far, the pull request and the test results. Commits, releases and checks replace the stored list, so send all of them every time.
3. Pause or give up. Call `finish_agent_work` with `cancelled` or `failed` and a summary. Coming back later is a new `start_agent_work`.
4. Finish. Call `finish_agent_work` with `succeeded`, the summary, the pull request, commits and tests first. Then move the task to review or done, and add a comment about what was done.

A run with no update for 45 minutes is marked abandoned. The agent keeps calling `update_agent_work` or `finish_agent_work` with the same run id: the run resumes and the whole time counts. It must not start a new run for it.

Roles let several agents work on one task:

- An orchestrator that hands the task to other agents opens its own run with the role `orchestrator`, keeps updating it while it waits, and finishes it at the end.
- The agents it starts pass the role `lead` or `worker`, and the orchestrator's session id as the parent session.
- Runs of one agent with different roles run side by side. A new run of the same agent with the same role on the same task ends the old one.

Agent time is kept in agent work, never on the task timer. What you see of these runs in the app is in [Agent work](/docs/app/agent-work). From the terminal the same steps are `tmgr agent start`, `tmgr agent update` and `tmgr agent finish`: see [Agent work reports](/docs/cli/agent-work-reports).

## Local workspaces {#local}

A local workspace of the desktop app stays on your computer, so the task MCP server cannot see it. For local workspaces the desktop app has its own MCP server, `tmgr-local`. It connects Claude Code, Codex or Cursor with a local access token that starts with `lt_` and works for one local workspace. Setup is in [Local workspaces](/docs/desktop/local-workspaces#local-agent).

## The MCP server of tmgr {#cli-mcp}

The `tmgr` command line has a small MCP server of its own. It runs on your computer over stdio: add it to your agent as a local server with the command `tmgr mcp`. It needs a signed-in `tmgr`.

Its tools:

- `notify_user`: sends a push notification to your phone.
- `alarm`: for urgent cases only. A push alarm, then a phone call if you do not acknowledge it.
- `alarm_status`: checks an alarm that `alarm` raised.
- `tmgr_cli`: short help on the `tmgr` commands.
- `ask_owner`, `find_answers`, `get_answer`, `reply_owner`: questions to you in the Decisions inbox of the desktop app, on macOS and Linux. See [The Decisions inbox](/docs/cli/the-decisions-inbox).

It does not work with your tasks. For that, connect the task MCP server too. More in the [command line guide](/docs/cli#agents).
