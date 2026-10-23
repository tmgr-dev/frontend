# TMGR Desktop changelog

Each release needs a `## X.Y.Z — YYYY-MM-DD` section here before its `desktop-vX.Y.Z` tag is pushed. The release
workflow copies that section into the GitHub release and the in-app update notes, and fails without it.

## 0.9.65 — 2026-10-23

- Design: the remaining screens follow the current design in every theme: tasks, comments and attachments, Pages, the dashboard and board, onboarding and tours, files, invitations, dialogs and empty states. Old blue and navy colours are gone, and links and main buttons use your theme's accent.
- Editors: in dark themes the task description editor's toolbars, popovers and blocks are dark too, and the Pages editor's links and chips use the theme accent.
- Dialogs: dialog panels, confirmations and empty states are readable in light, dark and coloured themes.

## 0.9.64 — 2026-10-23

- Timezone: Settings → Theme has a Timezone setting. On first sign-in the app saves your device timezone, and you can change it there. Daily routines fire and days start in this timezone.
- Routines: today's highlight and the now-line follow your timezone.
- Attachments: chat attachments show previews and open in a full-window viewer with zoom, PDF and markdown.
- Quick add: choose routine, task or task with a screenshot; the last category is remembered and an unfinished draft survives reopening.
- Task form: a title that is too long continues in the description.
- Tasks list: the task count is shown next to the hours total, and tasks without tracked time show 0 minutes instead of NaN.
- Dashboard: team activity has an all-time window.

## 0.9.63 — 2026-10-22

- Search: press Cmd+K (Ctrl+K on Windows and Linux) anywhere to find tasks, pages, screens and workspaces; the sidebar Search opens the same palette.
- Pages: Trash is now Archive. Restore a page, delete it permanently, or empty the whole archive; this works in local workspaces too.
- Tasks: set a priority (Urgent, High, Medium or Low) in the task form; board cards and list rows show a badge for Urgent, High and Low.
- Board and list: filter tasks by priority and sort them Urgent → High → Medium → Low; the choice survives a reload.
- Board: the first-visit tour no longer interrupts a card you are dragging; it shows after the drop.
- Notifications: the bell badge follows what you read or delete on the Notifications page, and the panel closes when you open a notification or View all.
- Selects show which option is highlighted and which one is selected; the dashboard heatmap shows empty days in dark mode and opens on the latest weeks on narrow screens.
- Comments: the list scrolls to the bottom after you ask AI.

## 0.9.62 — 2026-10-22

- Agent chat: every message shows the time it was sent, with day separators; each assistant reply shows how long it took and how many tokens it used.
- Agent chat: Copy a message as Markdown, Reply to any message, or select a fragment and Quote it; quotes and replies attach to your next message, and clicking one in a sent message jumps to and highlights the original.

## 0.9.61 — 2026-10-21

- Daily Routines: dragging one occurrence of a recurring routine to another day now moves only that occurrence; it shows on the new day and disappears from the original one.

## 0.9.60 — 2026-10-21

- Agent chat: with Write mode on, the assistant can also update a task (status, assignees, estimate, deadline), add a checkpoint, start or stop your own timer and link tasks. Every change waits on the "Confirm changes" card.
- Agent chat: a personal routine planner. Ask about your routines or agenda, or say "plan my day"; new or changed routines, moved occurrences and pause/resume all go through the card and apply together or not at all.
- Attach: a questions pocket collects questions your agents ask in their sessions, with ?N badges and a menu.
- Notifications: the dropdown and the page use the shared buttons and theme colors.
- Daily Routines: the last routines show instantly on return visits and refresh in the background.

## 0.9.59 — 2026-10-21

- Agent chat: the assistant can now propose changes instead of only reading. With the workspace setting AI assistant > Write mode set to "Write with confirmation" (the default is "Read only"), it shows a "Confirm changes" card listing the planned changes (for example creating tasks or adding comments); nothing changes until you confirm.
- Agent chat: pin facts (from a message or typed by hand) in the chat header. Pinned facts are sent to the assistant with every message and survive history compaction.
- Board: a corner spinner replaces the refresh line that pushed columns down; Dashboard and List show their last data instantly on return visits.
- Task form: auto-growing comment field, AI step chips on one row, no empty agent-status row above the title, Esc leaves a comment draft.
- Agent status chip: an idle agent that still ticks the CPU reads Waiting, not Working.
- Attach: resize split panes by dragging the split lines.
- Updates: one Check for Updates click emits one event and joins a check that is already running.

## 0.9.58 — 2026-10-21

- Sidebar: the Pages section shows your pinned pages first, then the ten most recently edited ones as a flat list. The full page tree with drag and drop is on the All pages page.
- Pages: Unpin works for every pinned page, including pinned subpages under a collapsed parent.
- New task is back in the top bar; the separate Pinned group at the top of the sidebar is gone.
- The "…" menu on page rows shows on hover instead of on every row.
- Agent chat: compact a long chat's history with one button.
- Workspaces: owners no longer see Leave, and a failed Leave shows the server's reason.

## 0.9.57 — 2026-10-21

- Settings: their own grouped navigation, so every settings section is one click away instead of living in the user menu.
- Sidebar: a shorter user menu, plus search, quick actions and Pinned and Tools sections.
- Dashboard: a 7 / 30-day period switch, KPI cards with trends, an activity chart, top categories by tracked time and an active members table.
- Ask AI and task comments: attach files to a message or comment.
- Pages no longer shift sideways when the scrollbar appears or disappears.
- Product tours no longer cover open dialogs.

## 0.9.56 — 2026-10-21

- Voice input: a mic button in the Ask AI chat and in the task comment box. Press it to record, press stop to turn your speech into text at the cursor, then edit and send it yourself. Recording stops at 2 minutes. macOS asks for microphone access the first time.
- Ask AI: a chat list, so you can switch between and open earlier chats, and a context meter that shows how full the chat's context window is.
- Ask AI: quick scenario chips under the input start common questions in one click.
- Tasks: terminal session time entries are listed on a task.

## 0.9.55 — 2026-10-21

- "Open in TMGR Desktop": when you use the desktop app, tmgr.dev in a Mac browser shows a thin bar that opens the same page here. Links from the browser only navigate to regular pages (tasks, board, list, pages); they never open settings or forms and never run an action. The bar hides until your next sign-in.
- Runner sessions understand the mouse mode of the latest CLI (needed for clickable sessions in `tmgr attach`).

## 0.9.54 — 2026-10-20

- After you install an update, TMGR reopens in front of other apps with its window focused, also when it was started at login. This takes effect from the next update after this one.
- Category page: "Create task" moved into the header next to "+ Subcategory"; the empty row above the task list and the broken status filter and select-all button are gone.

## 0.9.53 — 2026-10-20

- Categories: the categories tree now lives in the sidebar, with a filter and "+" to create one. Each category has one page with Tasks, Subcategories and Settings tabs; the separate categories page with the list and the settings pane is gone.
- Category settings are one flat page. The save bar appears only when you change something; Cursor, GitHub and agent repositories are one row each and open in place.
- Default estimate and Timer reminder take durations like 4h, 20m or 1h 30m instead of a clock time.

## 0.9.52 — 2026-10-20

- AI usage: an Autopilot panel (desktop only) shows the local runner's autopilot live: state, Pause / Resume / Stop, task counters, active runs with Open task / Open session / Open PR, usage limits with the stop marks, cost and an event feed. Open task and the needs feedback / blocked / queued lists light up with the next CLI release.
- Includes the 0.9.51 changes below (0.9.51 was not published).

## 0.9.51 — 2026-10-20

- Confirm for agent runs (single and bulk) shows the persona instructions and the workspace rules your approval covers, in full, with hidden characters flagged. Cloud agent runs receive exactly that signed text.
- Tasks show "Needs confirmation" when the persona instructions or the workspace rules changed after you confirmed.

## 0.9.50 — 2026-10-19

- Task form: the footer shows who created the task (hidden on phone widths).
- Dashboard: clicking a day on the activity heatmap filters the activity feed to that day.
- Pages: clicking a task mention opens the task in a modal over the page instead of the full task form; closing it keeps you on the same page.
- Telegram sign-in no longer accepts a login token passed in the page link.

## 0.9.49 — 2026-10-19

- Cloud runs in several repositories: Category settings has an "Agent run repositories" list (shown when "Let my agents run my tasks" is on). "Confirm for agent runs" shows the repositories the approval covers.
- Runner settings list the repositories bound on this computer under each allowed workspace (read-only).

## 0.9.48 — 2026-10-19

- Cloud runs: Runner settings list the cloud workspaces this computer allows your agents to work in, with the expiry date and a copyable `tmgr runner allow <code>` command (allowing happens only in a terminal). Three days before an allow expires the app reminds you once a day.
- Tasks of cloud workspaces where "Let my agents run my tasks" is on show whether they are approved for agent runs. "Confirm for agent runs" shows the exact text and category that will be signed, with hidden characters marked; several tasks can be confirmed at once from Batch Actions.
- Tasks you write here are approved automatically; an edit or comment by someone else or by an agent cancels the approval.

## 0.9.47 — 2026-10-18

- Task keys: a key like TM-242 on a page, in the graph or in a link now opens the task whose title starts with it, with or without a colon, instead of an unrelated task with the same number. A key that several tasks carry stays plain text.
- The key shown for a task (pages, files, graph, plugins) is now the one from its title, so copying it finds the same task again.

## 0.9.46 — 2026-10-17

- Wide windows: page content now sits in one centered column up to 1280px wide instead of hugging the left edge. The board and the workspace map still use the full width.

## 0.9.45 — 2026-10-17

- Block editor: lists and checklists can now be nested. Lists and checklists from older tasks open and save as before.
- Block editor: clicking Create right after typing keeps the description, Done on a description you didn't change no longer re-saves the task, and Enter right after picking a block type starts a new block.
- Descriptions written in the old HTML format show in the preview and open in the block editor whatever editor you prefer; a description with only empty lists shows "No description".
- Markdown editor updated. Embedded frames in Markdown descriptions are limited to YouTube, CodePen and Figma and run sandboxed.
- Dashboard: clicking a task in the team activity widget opens that task.

## 0.9.44 — 2026-10-16

- Pages: images work like in Outline. Drag the side handles to resize (corners keep the proportions), align left, center, right or full width from the image toolbar, add a caption under the image, and click a selected image to open it full screen.
- Pages: while an image uploads, a faded preview of it sits where it will land instead of the "Upload in progress" text over the line hint. Pasting or dropping an image works the same way, and retina screenshots keep their on-screen size.
- Agent usage sync checks totals against the server's rules on your machine before uploading.

## 0.9.43 — 2026-10-16

- Settings → Agent work (user menu): turn on Agent usage sync to upload per-task AI usage totals so web and mobile can show them, see what you uploaded, and delete it. Turning it off deletes everything already uploaded. Prompts, replies, file paths and command arguments are never uploaded.

## 0.9.42 — 2026-10-16

- Terminal commands in the app now have a copy button: Settings → Experimental (`tmgr config allow-bypass`), runner messages such as `tmgr runner start` or `tmgr agents login`, AI usage, CLI devices, MCP setup and onboarding. One click puts the command on the clipboard.
- Settings → Personas lists the newest personas first.

## 0.9.41 — 2026-10-16

- Agent work details and the AI usage task list now link usage to tasks that agents tracked through the TMGR MCP tools (start, update and finish agent work), not only to runs started with `tmgr agent start`. Before, those tasks showed no local usage.

## 0.9.40 — 2026-10-16

- Task page: a short Agent work line (your time, agent time, AI cost, pull request) that opens "Agent work — details": totals, tokens by type, the agent tree (Claude Code, its subagents and the Codex runs they started), tools, skills, MCP servers, hooks, permission modes and the commits and pull request of the agent runs. AI usage is read from the Claude Code and Codex logs on this computer and stays here; AI cost is API-equivalent.
- Runner: the agy profile and opencode permission modes.

## 0.9.39 — 2026-10-16

- AI usage: Claude limits now show the fullest reading of the current window instead of the newest copy sent by an idle session.

## 0.9.38 — 2026-10-15

- AI usage screen: tokens, API-equivalent cost and limits per day, tool, model and task, read from the Claude Code and Codex logs on this computer (user menu, AI usage). Cost is API-equivalent (list prices), not your subscription bill. Claude limits need the status line hook (`tmgr usage statusline install`); Codex limits come from its local sessions. Nothing leaves this computer.

## 0.9.37 — 2026-10-12

- Getting started tours on desktop: quick add, screenshot capture, the menu bar or system tray timer, plugin and
  task windows, your local workspace, backups, local agent access, plugins and Move to cloud. Start them from the
  Getting started hub; the first few run on their own in a new local workspace.
- New local workspaces come with three sample tasks you can remove in one click.
- What's new now has a button to take the tour for each new feature.

## 0.9.36 — 2026-10-12

- Move to cloud: copy a local workspace into a new workspace in your TMGR account (Settings → Data, or the workspace
  menu). A dry run lists what moves and what stays (persona tokens, plugin data, files the server can't take, such as
  SVG or files over 25 MB) and lets you choose what happens to each local persona. The workspace is read-only for a
  moment while a backup snapshot is taken, running timers are stopped first, and the move continues and resumes in the
  background if it stops. Your local copy is kept.
- Remove local copy: after a published and verified move you can remove the local workspace from this computer. It is
  only possible when nothing changed since the move, every file that wasn't moved is exported or accepted as lost, and
  a fresh archive of the workspace is written; you confirm by typing the workspace name. The archive and the move
  snapshot stay in the backups folder.
- Onboarding: a welcome screen asks how you plan to use TMGR, a Connect your agent screen shows live connection status
  while you set up MCP, and a Getting started checklist tracks your first steps (sample tasks can be removed in one
  click).

## 0.9.35 — 2026-10-12

- Getting started hub and guided tours (skip anytime, restart from the user menu).
- Usage data: the app now asks whether you want to share usage data. It is off unless you choose "Share usage data".
  Events are not linked to your account and never contain task, page or comment text, names or emails. Change it any
  time in Settings → Privacy, where you can also reset the install ID and delete data already sent.
- Privacy policy: linked from Settings → Privacy and from the usage-data question.
- The desktop app still never loads Google Analytics, Yandex Metrica or Plausible.

## 0.9.34 — 2026-10-12

- Task links: `tmgr://task/<workspace>/<id>` and `https://tmgr.dev/<workspace>/tasks/<id>` links open the task in the app,
  switching workspace if needed. A task that no longer exists or that you cannot access shows a message. Clicking such
  a link inside the app (comments, pages, the editor) opens it in the app instead of the browser.
- Share: a Share menu in the task window footer and the task context menu with Copy link, Copy app link and Copy as
  Markdown.
- Back and forward: Cmd+[ and Cmd+] (Alt+Left and Alt+Right on Windows and Linux), the mouse back and forward buttons,
  arrows in the window header and View → Back / Forward walk through the tasks and screens you opened. The shortcuts
  are ignored while you type in a field.
- Fixed: opening and closing a task leaked the description editor, so memory grew with every task you opened.
- Notifications: the ntfy topic now comes from the server, so notification pushes keep working after the server-side
  change.
- Includes web 3.4.147 (editor description format fix, local API clear flags, preset card names).

## 0.9.33 — 2026-10-11

- Simple mode: a new Workflow module (Settings → Modules → Planning). With Workflow on nothing changes. With it off, a
  Done checkbox replaces the status picker in the task window, the task form and list rows; lists filter by
  Open / Done / All; the Board is hidden. Archive becomes an action: "Archive" in the task menu and "Archive done" on
  the list, and archived tasks can be restored. Statuses and their tasks stay and come back when Workflow is turned on
  again. The Personal and Minimal presets use simple mode, and the preset preview shows it. It also works in local
  workspaces.
- Settings → About shows the app version and "More from the maker", other apps by the TMGR maker. It is hidden when
  there is nothing to show, and in a local-only workspace.
- What's new ends with a short "By the way, we made…" block with the same apps.
- Fixed: the sidebar Pages tree waits until the workspace's modules are known before it loads, instead of loading
  right at startup.

## 0.9.32 — 2026-10-11

- Start without an account: on first launch choose "Start without an account" or "Sign in". Without an account your
  workspace lives only on this computer and works offline; you can create an account later and keep the local
  workspace next to your cloud ones.
- Automatic local backups: TMGR backs up every local workspace once a day into its `backups` folder (the last 7
  days, 4 weeks and 3 months are kept) and checks each copy. Settings → Local data shows the last backup and has
  "Back up now". A banner reminds you there is no cloud copy.
- Features that need the TMGR cloud (Dashboard, Pomodoro, AI assistant, GitHub, Telegram, notifications and more)
  show "Requires an account" in a local-only workspace.
- Local agents without an account: Settings → Agents creates a "Local agent" and a token, so Claude Code or Codex can
  work in your local workspace over MCP. Fixed the `TMGR mcp` bridge getting no answer from the app.
- The desktop app no longer loads Google Analytics, Yandex Metrica or Plausible.

## 0.9.31 — 2026-10-11

- TMGR Desktop now runs on Windows and Linux, not only on Macs with Apple Silicon. Every release ships a Windows
  installer (`-setup.exe`, plus an `.msi`) and Linux `.AppImage`, `.deb`, `.rpm` and `.snap` packages, and there is
  a Nix flake for NixOS. The Windows installer is not code-signed yet, so Windows SmartScreen may warn the first
  time: choose "More info" → "Run anyway".
- The Windows installer, the AppImage, `.deb` and `.rpm` update themselves like the Mac app. The snap and the Nix
  package are updated by the Snap Store or by Nix instead.
- On Windows and Linux shortcuts use Ctrl instead of ⌘, the app lives in the system tray, and "Check for Updates…"
  is in the tray menu. `tmgr://` links reach the window that is already open instead of starting a second copy.
- Not available yet: on Linux, idle detection; on Windows, local agent access (MCP) and plugin windows; outside
  macOS, the "selected text to task" shortcut.

## 0.9.30 — 2026-10-11

- Settings → Modules replaces Feature Settings. Modules are grouped into packs (Time & focus, Planning, Knowledge, AI
  agents, Integrations). The workspace owner turns a module on or off for everyone; anyone can hide a module just for
  themselves ("Hidden for you"). Core sections (Projects, Board, Timer, Comments, Assignees) can only be hidden for
  yourself.
- A module that is off or hidden disappears from the sidebar, the task window, cards, the status bar and the tray, and
  the app stops calling it. Its data stays and comes back when it is turned on again.
- Apply a preset (owner): Personal, Team, Developer with AI agents, Everything or Minimal, with a list of what turns on
  and off and a live preview of the menu and task blocks. New workspaces open the same picker once.

## 0.9.29 — 2026-10-09

- Agent work is now a workspace feature, on by default: the workspace owner can turn it off in Settings → Features →
  Agent work. When it is off, tasks have no Agent work section, agent activity is hidden from the dashboard and AI
  agents are told to skip reporting their work. Existing runs are kept and come back when it is turned on again.
  Works in local workspaces too.

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
