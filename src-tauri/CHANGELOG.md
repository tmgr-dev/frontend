# TMGR Desktop changelog

Each release needs a `## X.Y.Z — YYYY-MM-DD` section here before its `desktop-vX.Y.Z` tag is pushed. The release
workflow copies that section into the GitHub release and the in-app update notes, and fails without it.

## 0.9.28 — 2026-10-09

- New workspace Map (sidebar → More → Map): the whole workspace as clusters, one per category, with the tasks and
  pages that tie the most together, the task that blocks the most open work, the links between areas and the items
  nothing links to. Search, pick a time range or replay how the map grew, click a cluster to zoom in and a dot to
  open its neighbourhood.
- Graph is now a workspace feature, off by default: the workspace owner turns it on in Settings → Features → Graph.
  It controls the Graph section on tasks and pages and the Map, in local workspaces too.

## 0.9.27 — 2026-10-09

- Tasks and pages have a Graph section: see what a task or page is connected to — related tasks, pages that mention
  it, people and personas, agent runs — as an interactive map. Expand it, pick a node to see why it's connected,
  open it or re-center the graph on it, and switch between 1 and 2 steps away.
- Works in local workspaces too; their graph is built on this computer and never leaves it.

## 0.9.26 — 2026-10-08

- Pages can be exported as Markdown: one page as a `.md` file (page "…" menu → Export as Markdown), a page with its
  subpages or all pages as a `.zip` with a folder tree, attachments in `assets/` and links between the pages kept
  as relative links. (#377)
- Markdown can be imported into Pages: `.md` files, images or a `.zip` (Obsidian vault, Notion export, a docs folder)
  through Pages → Import, "Import Markdown here…" on a page, or by dropping files onto the page tree. A preview shows
  the pages to create, warnings and what to do when a title already exists; links and images are reconnected. Works
  in local workspaces too, without sending anything to the server.

## 0.9.25 — 2026-10-08

- A timer you stop on another device (the phone, the watch, Siri, Control Center, a notification) now stops here
  too: the status bar, the tray, the open task and Active tasks update within seconds, including when a local
  workspace is open.
- After the laptop wakes up or the window comes back into focus, the app re-checks which timers are running instead
  of showing a timer that was stopped while it was asleep.

## 0.9.24 — 2026-10-04

- Plugins can add their own items to the task "…" menu: on board cards, in the task list, on the task page and in a
  task opened in its own window. Plugin items sit in their own group; with more than four they fold into one Plugins
  submenu. A plugin needs the new "add items to the task … menu" permission, shown when you install or update it.
- Plugin API 1.6 for plugin authors: `contributes.menus["task/card"]` with up to three items per plugin, each running
  one of the plugin's commands with the task id.

## 0.9.23 — 2026-10-04

- Settings → Agent notifications now walks you through connecting an agent: after Create token you get ready-to-paste
  setup for Claude Code, Codex, Cursor and hooks with the token filled in, plus a Send test notification button.
- A new "AI agents" card explains the two TMGR MCP servers: tmgr for tasks and time, tmgr-notify for notifications
  and alarms. The full setup guide is at tmgr.dev/docs/agents.

## 0.9.22 — 2026-10-04

- Alarm phone shows the number alarms call you from, with Copy and Add to contacts, plus tips to let it through
  Do Not Disturb and Sleep (Favorites and Repeated Calls).
- Alarm calls ring longer and repeat until you acknowledge them; a voicemail pickup no longer counts as answered.

## 0.9.21 — 2026-10-04

- Settings → Agent notifications has a new Alarm phone section for urgent alarms. Add your number, confirm it with
  the SMS code, and when an agent raises an alarm that nobody acknowledges in the app, TMGR calls you; press 1 to
  acknowledge. The number is stored encrypted and only its masked form is ever shown.

## 0.9.20 — 2026-10-01

- Pages now speak English like the rest of the app: sidebar, dialogs, properties, version history, trash, search and
  notifications. New person pages use Summary, Promises, Timeline, What I know and Insights; meeting pages use
  Agenda, Outcomes, Decisions and Action items; the workspace context page is called "Workspace context".
- Existing pages keep your text. Section headings managed by the app (Promises, Insights, Agent notes) and an
  unchanged context page title are renamed to English; older Russian headings you wrote keep working for appending
  and for "Make a task" on meeting action items.

## 0.9.19 — 2026-10-01

- Pages now work in local workspaces too: the page tree, editor, versions, links, search and person and meeting pages
  live in the workspace folder on this computer, work offline and never reach the server.
- Open a page in its own window from the page header or the sidebar menu. Links like `tmgr://page/<workspace>/<page>`
  open the page in the app.
- Quick Add has a new «В страницу» mode: pick a page and a section, then add text or a screenshot to it.
- Tools connected through local access can read and write pages (pages tools and the workspace context), and see
  page changes live.
- Plugins: Plugin API 1.5 lets plugins read pages, write to them and keep their own sections on a page, with new
  permissions you confirm at install. Existing plugins keep working unchanged.

## 0.9.18 — 2026-10-01

- Pages get types: «Человек» (person) and «Встреча» (meeting) pages with editable properties — aliases, company,
  role and last contact for people; date, participants and related tasks for meetings. A person page lists the open
  tasks that mention it under «Обещания», kept up to date automatically.
- Select text on a page and choose «Сделать задачей» to turn it into a task; the text becomes a link to the new task.
- Attach files to pages and paste or drop images straight into the page.
- Tasks show the pages that mention them and chips for the people mentioned.
- Follow a page to get notified about changes; new notification types for created, changed and mentioning pages.

## 0.9.17 — 2026-10-01

- New: assign tasks to personas. The assignee picker lists personas under the members — your own personas enabled in
  the workspace and the workspace's shared personas — and a task can have several. An assigned task waits in the
  persona's queue; an agent connected with the persona's token picks it up with the `list_my_queue` MCP tool. Cards,
  lists and the task window show the persona's avatar; click it to see whose persona it is. The board can be filtered
  by persona and by "My personas' queue".
- New: workspace personas. In Settings → Personas choose "This workspace" to create a persona every member of the
  workspace can assign tasks to; only you can edit it, and it is not visible in other workspaces.
- Local workspaces support persona assignment too, and local AI personas can list their queue over the local MCP.

## 0.9.16 — 2026-10-01

- New: Pages («Страницы») in cloud workspaces — markdown documents next to your tasks, with a page tree in the
  sidebar, version history with diff and restore, `@` and `[[` links to tasks, pages, categories and people, search
  across tasks and pages, and a shared context page that AI agents read and add notes to. New workspaces have Pages
  on; in existing workspaces the owner turns it on in workspace feature settings. Local workspaces get Pages in a
  later release.

## 0.9.15 — 2026-09-30

- Clicking a task in the tray menu or in the running timer at the bottom of the window now brings up the task's own
  window if it is open, instead of opening the task a second time in the main window. Without its own window the task
  opens in the main window as before.

## 0.9.14 — 2026-09-30

- The bottom of a task is more compact: open, created/edited time, Save and Delete now sit in one row under the
  comment box. Save lights up while there are unsaved changes; hover a time to see the exact date.
- The ✨ button in the comment box is now an "AI reply" switch. Turn it on and Enter or send asks the AI instead of
  posting a plain comment. The choice is remembered for your account in every task, window and device.

## 0.9.13 — 2026-09-30

- Fixed: a task opened in its own window was missing files, checkpoints, the timer, relations and assignees. The window
  now shows everything the task card shows.

## 0.9.12 — 2026-09-30

- A task can have its own window. In the task card, the open button in the bottom-left corner now opens the task in a
  separate window and closes the card. Everything from the card is there: description, status, timer, comments,
  files, checkpoints and agent work. The window stays open while you move around the app, can go to another screen,
  and is named after the task. You can open several tasks at once; opening the same task again brings its window to
  the front.
- Changes made in a task window show up in the main window and the other way round, in local workspaces too.
- Links in a task window to settings or other pages open in the main window; a linked task opens in its own window.

## 0.9.11 — 2026-09-30

- Plugins API 1.4: sidebar badges. A plugin can show a counter on its item in the sidebar, for example
  "Telegram 12" in red when something is overdue; in the collapsed sidebar it is a dot on the icon. It needs the new
  "show counters on its sidebar items" permission and disappears when the plugin is turned off.
- A clickable number tile on a plugin page no longer cuts off its value.
- Long plugin menu bar items with emoji are shortened with "…" instead of failing, and a menu bar error no longer
  breaks the plugin's page.

## 0.9.10 — 2026-09-30

- After an update, the app shows what changed: a "What's new" window lists the notes of every release since the version
  you last used, newest first. It appears once per update.
- To read the notes again, click the version number in the bottom-right corner of the status bar.

## 0.9.9 — 2026-09-30

- Password reset and "Change password" in Profile now ask whether to sign out of other devices (checked by default).
  On a reset, unchecking keeps your existing sessions; in Profile, checking signs out every other session while this
  one stays signed in.

## 0.9.8 — 2026-09-29

- In a local workspace, account settings reach the server again: creating agent-notification tokens, persona tokens,
  the smart-device token, Telegram linking and avatar upload no longer fail with "Could not create token".
- Changing the password in Profile now revokes the device, persona and agent-notification tokens (as a password reset
  already did) and says so; the current session stays signed in.

## 0.9.7 — 2026-09-29

- Clicking the running timer in the status bar or the running task in the tray menu opens the task again when it lives
  in another workspace (for example a cloud task while a local workspace is open): the window switches to the task's
  workspace and opens it.

## 0.9.6 — 2026-09-29

- Quick Add saves into the workspace picked in its header again. While a local workspace was open in the main window,
  adding to a cloud workspace failed with "Could not add the task"; Quick Add now also follows workspace switches made
  after it was first opened.
- When Quick Add cannot save, it shows the reason and writes it to the app log.

## 0.9.5 — 2026-09-29

- Each window keeps its own current workspace: switching in one window, tab or on your phone no longer moves the others,
  and a reload keeps the window where it was. New windows open on the workspace you used last.
- Settings → Workspaces has a "Default workspace" picker. It is used by Telegram, MCP, smart devices and when you sign
  in on a new device; switching workspaces no longer changes it.

## 0.9.4 — 2026-09-29

- The API token in Settings is shown only once, right after you generate it; afterwards it stays hidden (generate a
  new one if you lost it). Tokens are now stored hashed on the server; existing devices keep working.
- When a session ends because it could not be refreshed, the sign-in page says so instead of logging you out silently.

## 0.9.3 — 2026-09-29

- One page layout everywhere: content column aligned left with the same paddings, a page header (title, subtitle,
  actions) on every page; lists and dashboards are wide, settings and forms are narrow.
- Settings, Profile, Feature settings and Statistics moved to the current design: section cards, labelled switches with
  descriptions, design-system inputs, selects and buttons, dark mode throughout. `/settings` now opens Workspace
  settings; password fields hide what you type.
- Statistics: tiles and a table instead of raw text, no more NaN, "Last week" shows the week; a Statistics link in the
  account menu at the bottom of the sidebar.
- Fixes: no sideways scroll on the push notifications guide, Error404 readable in light theme, "Workspace Settings" typo.

## 0.9.2 — 2026-09-28

- Plugins API 1.3: new page building blocks for plugins — `card` (solid background, border, accent stripe, padding,
  optionally clickable with keyboard support), `grid` (1–6 equal columns, scrolls sideways when it doesn't fit),
  `menu` (a "⋯" menu with actions), `stack` alignment options, `button` variants and sizes, and clickable `stat`/`badge`.
  Older plugins render exactly as before.

## 0.9.1 — 2026-09-28

- Fixed global shortcuts (⌥Space quick add, ⌥⇧T timer, ⌥⇧S screenshot, ⌥⇧C selection) going dead after the app
  window reloaded (for example after signing out and back in): they showed as taken, did nothing, and a new
  shortcut could not be recorded in Settings → Shortcuts.

## 0.9.0 — 2026-09-28

- Plugins API 1.2: plugins can read and write routines and notes in local workspaces (`routines:read`,
  `routines:write`): list a period (recurring and one-off), create, rename, complete/skip, and convert a routine into a
  task with its category key in one call; `routine.created/updated/deleted` events, including routines added with
  Quick add (⌥Space). In shared (cloud) workspaces these calls answer `NOT_SUPPORTED` for now.
- Plugins can no longer read or edit routines through `tasks:*`; routines need the new permissions.

## 0.8.7 — 2026-09-28

- Local workspaces: a comment an agent or plugin deletes disappears from the open task panel right away; the task list
  page updates live too (covered by tests now).
- Personas "On this device": the local LLM API key is never sent to a different host after you change the endpoint,
  plain http to remote hosts is refused when a key is set (localhost/LAN still fine), and the key and persona cache
  are kept per account and cleared on logout.
- Personas: a hint in the Workspaces list shows where to connect a persona to a local workspace ("On this device").

## 0.8.6 — 2026-09-28

- Local workspaces update live when an agent or plugin writes through local access (REST or MCP): new comments and
  reactions, status changes (the board card moves), title/description edits (with the "updated elsewhere" banner),
  relations and agent work runs — no need to reopen the card.
- Local workspaces: a category created without a code gets one from its name, like in the cloud.
- Local access: editing someone else's comment returns `NOT_OWN`, deleting a missing comment returns 404;
  `GET /api/local/health` uses `app_version`; the local access API doc matches 0.8.3+.

## 0.8.5 — 2026-09-28

- File → "Check for Updates…" in the menu bar runs a manual update check with visible feedback
  (checking, up to date, update available with Install and restart, or an error), instead of waiting
  for the silent periodic check.

## 0.8.4 — 2026-09-28

- Fix: the Connect an agent dialog keeps the token, MCP config and buttons inside the dialog on wide windows
  (local and cloud personas); the dialog is wider once a token is issued.

## 0.8.3 — 2026-09-28

- MCP for local personas over the local access socket, plus the `TMGR mcp` stdio bridge for Claude Code and Codex
  ("Copy MCP config" in the Connect an agent dialog).
- Event stream for local agents: `GET /api/local/events` (SSE with a cursor, filtered by the persona's rights).
- Plugins can ask the owner to connect their companion (`tmgr.localAccess.requestConnection`, manifest `companion`
  section); a plugin's tokens are revoked when it is turned off or uninstalled.
- Fix: the Connect an agent dialog no longer closes right after a token is issued.

## 0.8.2 — 2026-09-28

- Local agent access: an external program (for example a plugin's companion) can work with a local workspace over a
  unix socket on this Mac, acting as one of your personas. Turn on "Local agent access" in the persona panel of a
  local workspace and issue a token with "Connect an agent"; the token lives in the Keychain.
- Local workspaces: action journal, editing your own comments, author on agent work runs, due dates stored in UTC.

## 0.8.1 — 2026-09-28

- Drop files anywhere on the task form; attachments can be queued before the task exists.
- Task settings from the three-dots menu in the list and on the board.
- Routines: offer to open the task a routine was converted into; converting a routine without an estimate works.
- Fix: text selection is visible in every theme.

## 0.8.0 — 2026-09-28

- Social login (Google, Apple, GitHub, Telegram) signs the desktop app in, not only the website.

## 0.7.0 — 2026-09-28

- Personas: identities you own that act in workspaces, with a system prompt, skills and per-workspace rights.
  Persona tokens for MCP ("Connect an agent"), every action signed "Name · persona of you".
- Local AI (preview): ask a persona about a task using your own local LLM.

## 0.6.0 — 2026-09-27

- Plugins API 1.1: comment authors, reactions, task relations, status and category writes, task filters, due time,
  per-task plugin data, agent work in local workspaces, alarms, notifications with an action, do not disturb, tray
  items, `tmgr://` deep links into plugins, more UI components, developer hot reload and live log.

## 0.5.3 — 2026-09-27

- Visual "How plugins work" help page.

## 0.5.2 — 2026-09-27

- Local routines: daily routines in local workspaces, clearly marked as stored only on this device.
