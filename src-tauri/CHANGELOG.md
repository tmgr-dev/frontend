# TMGR Desktop changelog

Each release needs a `## X.Y.Z — YYYY-MM-DD` section here before its `desktop-vX.Y.Z` tag is pushed. The release
workflow copies that section into the GitHub release and the in-app update notes, and fails without it.

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
