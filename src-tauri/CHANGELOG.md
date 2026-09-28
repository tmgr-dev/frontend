# TMGR Desktop changelog

Each release needs a `## X.Y.Z — YYYY-MM-DD` section here before its `desktop-vX.Y.Z` tag is pushed. The release
workflow copies that section into the GitHub release and the in-app update notes, and fails without it.

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
